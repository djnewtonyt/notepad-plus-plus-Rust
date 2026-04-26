// src/utf_convert.rs
//
// Pure-Rust migration of the UTF-8 / UTF-16 detection and conversion logic in:
//
//   PowerEditor/src/Utf8_16.cpp
//   PowerEditor/src/Utf8_16.h
//
// ABI strategy: No FFI needed for pure-Rust callers.
// The `ffi/bridge.rs` module exposes `extern "C"` wrappers for the C++ side.
//
// What is migrated here:
//   • BOM detection (`Utf8_16_Read::determineEncodingFromBOM`) → `UniMode::from_bom`
//   • UTF-8 classification (7-bit, valid UTF-8, or raw 8-bit) → `classify_utf8`
//   • UTF-16 → UTF-8 conversion (used when loading files) → `Utf16ToUtf8`
//   • UTF-8 → UTF-16 conversion (used when saving files) → `Utf8ToUtf16`
//
// What is NOT migrated in this step:
//   • `Utf8_16_Write::openFile` / `writeFile` / `closeFile`: these call Win32
//     `CreateFileW` / `WriteFile` and are deferred to Phase 2 when the Win32
//     I/O abstraction layer is ported.  They are documented in the ADR.

use crate::types::UniMode;

// ─────────────────────────────────────────────────────────────────────────────
// BOM table
// ─────────────────────────────────────────────────────────────────────────────

/// BOM byte sequences indexed by `UniMode` discriminant.
///
/// Index 0 (`Ansi8Bit`) has an all-zero entry because ANSI files have no BOM.
/// C++ source: `Utf8_16::k_Boms`.
const BOMS: [[u8; 3]; 8] = [
    [0x00, 0x00, 0x00],  // 0 – Ansi8Bit  (no BOM)
    [0xEF, 0xBB, 0xBF],  // 1 – Utf8      (UTF-8 BOM)
    [0xFE, 0xFF, 0x00],  // 2 – Utf16BE   (big-endian BOM)
    [0xFF, 0xFE, 0x00],  // 3 – Utf16LE   (little-endian BOM)
    [0x00, 0x00, 0x00],  // 4 – Utf8NoBom (no BOM by definition)
    [0x00, 0x00, 0x00],  // 5 – Ascii7Bit (no BOM)
    [0x00, 0x00, 0x00],  // 6 – Utf16BENoBom
    [0x00, 0x00, 0x00],  // 7 – Utf16LENoBom
];

/// Return the BOM bytes for a given `UniMode`, or an empty slice if the mode
/// has no BOM.
pub fn bom_for(mode: UniMode) -> &'static [u8] {
    let row = &BOMS[mode as usize];
    // Trim trailing zero bytes that are padding (not part of the BOM).
    match mode {
        UniMode::Utf8   => &row[..3],
        UniMode::Utf16BE | UniMode::Utf16LE => &row[..2],
        _ => &[],
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BOM detection
// ─────────────────────────────────────────────────────────────────────────────

/// Inspect the first bytes of a file buffer to determine its Unicode encoding
/// from BOM.
///
/// Returns the detected `UniMode` or `None` if no BOM was found (the caller
/// should then use `classify_utf8` to distinguish ANSI / UTF-8-no-BOM / ASCII).
///
/// C++ equivalent: `Utf8_16_Read::determineEncodingFromBOM`.
pub fn detect_encoding_from_bom(buf: &[u8]) -> Option<UniMode> {
    if buf.len() >= 3 && buf[0] == 0xEF && buf[1] == 0xBB && buf[2] == 0xBF {
        return Some(UniMode::Utf8);
    }
    if buf.len() >= 2 {
        if buf[0] == 0xFE && buf[1] == 0xFF {
            return Some(UniMode::Utf16BE);
        }
        if buf[0] == 0xFF && buf[1] == 0xFE {
            return Some(UniMode::Utf16LE);
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// UTF-8 classification
// ─────────────────────────────────────────────────────────────────────────────

/// Result of scanning a byte buffer for UTF-8 validity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Utf8Class {
    /// All bytes are in the range 0x00–0x7F (pure ASCII / 7-bit).
    Ascii7Bit,
    /// Bytes outside 0x7F are present AND the sequence is valid UTF-8.
    ValidUtf8,
    /// The buffer contains bytes that are not valid UTF-8 (raw 8-bit).
    Raw8Bit,
}

/// Classify a byte slice as 7-bit ASCII, valid UTF-8, or raw 8-bit data.
///
/// A NUL byte is treated as evidence of non-UTF-8 data (matches the C++
/// behaviour in `Utf8_16_Read::utf8_7bits_8bits`).
///
/// C++ equivalent: `Utf8_16_Read::utf8_7bits_8bits` (returns `u78` enum).
pub fn classify_utf8(buf: &[u8]) -> Utf8Class {
    let mut i = 0;
    let mut has_multibyte = false;

    while i < buf.len() {
        let b = buf[i];

        if b == 0x00 {
            // NUL byte: the C++ code uses this as a signal that the file is
            // binary / not UTF-8.
            return Utf8Class::Raw8Bit;
        }

        if b & 0x80 == 0x00 {
            // 0xxxxxxx – single-byte ASCII character.
            i += 1;
        } else if b & 0xC0 == 0x80 {
            // 10xxxxxx – continuation byte appearing as a lead byte: invalid.
            return Utf8Class::Raw8Bit;
        } else if b & 0xE0 == 0xC0 {
            // 110xxxxx – 2-byte sequence.
            if i + 1 >= buf.len() || (buf[i + 1] & 0xC0) != 0x80 {
                return Utf8Class::Raw8Bit;
            }
            has_multibyte = true;
            i += 2;
        } else if b & 0xF0 == 0xE0 {
            // 1110xxxx – 3-byte sequence.
            if i + 2 >= buf.len()
                || (buf[i + 1] & 0xC0) != 0x80
                || (buf[i + 2] & 0xC0) != 0x80
            {
                return Utf8Class::Raw8Bit;
            }
            has_multibyte = true;
            i += 3;
        } else if b & 0xF8 == 0xF0 {
            // 11110xxx – 4-byte sequence.
            if i + 3 >= buf.len()
                || (buf[i + 1] & 0xC0) != 0x80
                || (buf[i + 2] & 0xC0) != 0x80
                || (buf[i + 3] & 0xC0) != 0x80
            {
                return Utf8Class::Raw8Bit;
            }
            has_multibyte = true;
            i += 4;
        } else {
            // 5- or 6-byte sequences were never part of Unicode; treat as raw.
            return Utf8Class::Raw8Bit;
        }
    }

    if has_multibyte {
        Utf8Class::ValidUtf8
    } else {
        Utf8Class::Ascii7Bit
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UTF-16 → UTF-8 conversion
// ─────────────────────────────────────────────────────────────────────────────

/// Decode a single UTF-16 code unit pair and return the corresponding Unicode
/// scalar value, or `None` if the input is an unpaired surrogate.
///
/// This handles surrogate pairs: if `high` is a high surrogate (0xD800–0xDBFF)
/// and `low` is a matching low surrogate (0xDC00–0xDFFF), the pair is decoded
/// into a supplementary code point.
fn decode_surrogate_pair(high: u16, low: u16) -> Option<char> {
    if (0xD800..=0xDBFF).contains(&high) && (0xDC00..=0xDFFF).contains(&low) {
        let code_point =
            0x10000u32 + ((high as u32 - 0xD800) << 10) + (low as u32 - 0xDC00);
        char::from_u32(code_point)
    } else {
        None
    }
}

/// Encode a Unicode scalar value into UTF-8 bytes and append them to `out`.
fn encode_utf8_into(c: char, out: &mut Vec<u8>) {
    let mut buf = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
}

/// Convert a buffer of UTF-16 code units (as raw bytes in the specified byte
/// order) to a UTF-8 byte vector.
///
/// `is_big_endian` controls whether the input is interpreted as big-endian
/// (UTF-16 BE) or little-endian (UTF-16 LE).
///
/// Returns `None` if the input length is odd (incomplete final code unit) or
/// if an invalid surrogate sequence is encountered.
///
/// C++ equivalent: the conversion path inside `Utf8_16_Read::convert` when
/// `m_eEncoding` is `uni16BE` or `uni16LE`.
pub fn utf16_to_utf8(input: &[u8], is_big_endian: bool) -> Option<Vec<u8>> {
    if input.len() % 2 != 0 {
        // Odd number of bytes: the last code unit is incomplete.
        return None;
    }

    let mut out = Vec::with_capacity(input.len());  // UTF-8 is at most 1.5× UTF-16
    let mut i = 0;

    while i + 1 < input.len() {
        let lo = input[i];
        let hi = input[i + 1];
        let unit = if is_big_endian {
            u16::from_be_bytes([lo, hi])
        } else {
            u16::from_le_bytes([lo, hi])
        };
        i += 2;

        if (0xD800..=0xDBFF).contains(&unit) {
            // High surrogate: must be followed by a low surrogate.
            if i + 1 >= input.len() {
                return None; // truncated surrogate pair
            }
            let lo2 = input[i];
            let hi2 = input[i + 1];
            let low = if is_big_endian {
                u16::from_be_bytes([lo2, hi2])
            } else {
                u16::from_le_bytes([lo2, hi2])
            };
            i += 2;
            let c = decode_surrogate_pair(unit, low)?;
            encode_utf8_into(c, &mut out);
        } else if (0xDC00..=0xDFFF).contains(&unit) {
            // Orphan low surrogate: invalid.
            return None;
        } else {
            // BMP character: directly convertible.
            let c = char::from_u32(unit as u32)?;
            encode_utf8_into(c, &mut out);
        }
    }

    Some(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// UTF-8 → UTF-16 conversion
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a UTF-8 byte slice to a UTF-16 little-endian byte vector.
///
/// Returns `Err` if the input is not valid UTF-8.
///
/// C++ equivalent: the `Utf8_Iter`-based conversion path in
/// `Utf8_16_Write::convert` / `Utf8_16_Read::convert` for writing.
pub fn utf8_to_utf16_le(input: &[u8]) -> Result<Vec<u8>, core::str::Utf8Error> {
    let s = core::str::from_utf8(input)?;
    let mut out = Vec::with_capacity(s.len() * 2);
    for c in s.encode_utf16() {
        out.extend_from_slice(&c.to_le_bytes());
    }
    Ok(out)
}

/// Convert a UTF-8 byte slice to a UTF-16 big-endian byte vector.
pub fn utf8_to_utf16_be(input: &[u8]) -> Result<Vec<u8>, core::str::Utf8Error> {
    let s = core::str::from_utf8(input)?;
    let mut out = Vec::with_capacity(s.len() * 2);
    for c in s.encode_utf16() {
        out.extend_from_slice(&c.to_be_bytes());
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── BOM detection ────────────────────────────────────────────────────────

    #[test]
    fn bom_utf8() {
        assert_eq!(
            detect_encoding_from_bom(&[0xEF, 0xBB, 0xBF, b'H']),
            Some(UniMode::Utf8)
        );
    }

    #[test]
    fn bom_utf16_be() {
        assert_eq!(
            detect_encoding_from_bom(&[0xFE, 0xFF, 0x00, b'H']),
            Some(UniMode::Utf16BE)
        );
    }

    #[test]
    fn bom_utf16_le() {
        assert_eq!(
            detect_encoding_from_bom(&[0xFF, 0xFE, b'H', 0x00]),
            Some(UniMode::Utf16LE)
        );
    }

    #[test]
    fn bom_none() {
        assert_eq!(detect_encoding_from_bom(b"Hello"), None);
        assert_eq!(detect_encoding_from_bom(b""), None);
    }

    // ── UTF-8 classification ─────────────────────────────────────────────────

    #[test]
    fn classify_pure_ascii() {
        assert_eq!(classify_utf8(b"Hello, world!"), Utf8Class::Ascii7Bit);
        assert_eq!(classify_utf8(b""), Utf8Class::Ascii7Bit);
    }

    #[test]
    fn classify_valid_utf8() {
        // "Héllo" in UTF-8
        let bytes: &[u8] = &[0x48, 0xC3, 0xA9, 0x6C, 0x6C, 0x6F];
        assert_eq!(classify_utf8(bytes), Utf8Class::ValidUtf8);
    }

    #[test]
    fn classify_nul_is_raw() {
        assert_eq!(classify_utf8(&[0x41, 0x00, 0x42]), Utf8Class::Raw8Bit);
    }

    #[test]
    fn classify_raw_8bit() {
        // A lone 0x80 continuation byte is invalid as a lead byte.
        assert_eq!(classify_utf8(&[0x41, 0x80, 0x42]), Utf8Class::Raw8Bit);
    }

    // ── UTF-16 ↔ UTF-8 round-trip ────────────────────────────────────────────

    #[test]
    fn utf16_le_to_utf8_ascii() {
        // "Hi" in UTF-16 LE
        let input: &[u8] = &[b'H', 0x00, b'i', 0x00];
        let result = utf16_to_utf8(input, false).unwrap();
        assert_eq!(result, b"Hi");
    }

    #[test]
    fn utf16_be_to_utf8_ascii() {
        let input: &[u8] = &[0x00, b'H', 0x00, b'i'];
        let result = utf16_to_utf8(input, true).unwrap();
        assert_eq!(result, b"Hi");
    }

    #[test]
    fn utf8_to_utf16_le_roundtrip() {
        let original = "Hello, 世界!";
        let utf16 = utf8_to_utf16_le(original.as_bytes()).unwrap();
        let back = utf16_to_utf8(&utf16, false).unwrap();
        assert_eq!(back, original.as_bytes());
    }

    #[test]
    fn utf8_to_utf16_be_roundtrip() {
        let original = "Héllo";
        let utf16 = utf8_to_utf16_be(original.as_bytes()).unwrap();
        let back = utf16_to_utf8(&utf16, true).unwrap();
        assert_eq!(back, original.as_bytes());
    }

    #[test]
    fn utf16_surrogate_pair() {
        // U+1F600 (GRINNING FACE) encoded as a surrogate pair in UTF-16 LE:
        // high surrogate 0xD83D, low surrogate 0xDE00
        let input: &[u8] = &[0x3D, 0xD8, 0x00, 0xDE];
        let result = utf16_to_utf8(input, false).unwrap();
        assert_eq!(result, "\u{1F600}".as_bytes());
    }

    #[test]
    fn utf16_odd_length_returns_none() {
        assert!(utf16_to_utf8(&[0x48], false).is_none());
    }

    #[test]
    fn bom_for_utf8() {
        assert_eq!(bom_for(UniMode::Utf8), &[0xEF, 0xBB, 0xBF]);
    }

    #[test]
    fn bom_for_ansi_empty() {
        assert_eq!(bom_for(UniMode::Ansi8Bit), &[] as &[u8]);
    }
}
