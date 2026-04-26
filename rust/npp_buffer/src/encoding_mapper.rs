// src/encoding_mapper.rs
//
// Pure-Rust migration of EncodingMapper.cpp / EncodingMapper.h.
//
// Responsibility: bidirectional mapping between Notepad++'s sequential menu
// indices (IDM_FORMAT_*) and Windows code-page integers, plus a
// string-alias look-up used when parsing HTTP Content-Type or XML charset
// declarations.
//
// C++ source files migrated:
//   PowerEditor/src/EncodingMapper.cpp
//   PowerEditor/src/EncodingMapper.h
//
// ABI strategy: No FFI needed for the data and logic themselves.
// The `ffi/bridge.rs` module exposes thin `extern "C"` wrappers so that the
// remaining C++ code can call through without modification.
//
// Design notes:
//   • The original C++ code stores the encoding table as a C-array of
//     `EncodingUnit { int _codePage; const char* _aliasList }`.  We keep the
//     same layout as a Rust `&[(i32, &str)]` slice.  The ORDER of entries is
//     significant because the index is the same integer the menus use.
//   • `isInListA` in C++ searches a space-separated alias string with a
//     case-insensitive comparison.  The Rust equivalent (`alias_matches`) is
//     functionally identical but uses iterators instead of manual pointer
//     arithmetic.

/// A single entry in the encoding table.
/// `code_page`:  Windows code-page integer, or -1 if unsupported on Windows.
/// `aliases`:    Space-separated list of IANA / MIME names for this encoding.
struct EncodingEntry {
    code_page: i32,
    aliases:   &'static str,
}

/// The encoding table.
///
/// Entry order matches the C++ array in EncodingMapper.cpp exactly; do not
/// reorder without also updating the IDM_FORMAT_* constants in the C++ code.
static ENCODINGS: &[EncodingEntry] = &[
    EncodingEntry { code_page: 1250,  aliases: "windows-1250" },
    EncodingEntry { code_page: 1251,  aliases: "windows-1251" },
    EncodingEntry { code_page: 1252,  aliases: "windows-1252" },
    EncodingEntry { code_page: 1253,  aliases: "windows-1253" },
    EncodingEntry { code_page: 1254,  aliases: "windows-1254" },
    EncodingEntry { code_page: 1255,  aliases: "windows-1255" },
    EncodingEntry { code_page: 1256,  aliases: "windows-1256" },
    EncodingEntry { code_page: 1257,  aliases: "windows-1257" },
    EncodingEntry { code_page: 1258,  aliases: "windows-1258" },
    EncodingEntry { code_page: 28591, aliases: "latin1 ISO_8859-1 ISO-8859-1 CP819 IBM819 csISOLatin1 iso-ir-100 l1" },
    EncodingEntry { code_page: 28592, aliases: "latin2 ISO_8859-2 ISO-8859-2 csISOLatin2 iso-ir-101 l2" },
    EncodingEntry { code_page: 28593, aliases: "latin3 ISO_8859-3 ISO-8859-3 csISOLatin3 iso-ir-109 l3" },
    EncodingEntry { code_page: 28594, aliases: "latin4 ISO_8859-4 ISO-8859-4 csISOLatin4 iso-ir-110 l4" },
    EncodingEntry { code_page: 28595, aliases: "cyrillic ISO_8859-5 ISO-8859-5 csISOLatinCyrillic iso-ir-144" },
    EncodingEntry { code_page: 28596, aliases: "arabic ISO_8859-6 ISO-8859-6 csISOLatinArabic iso-ir-127 ASMO-708 ECMA-114" },
    EncodingEntry { code_page: 28597, aliases: "greek ISO_8859-7 ISO-8859-7 csISOLatinGreek greek8 iso-ir-126 ELOT_928 ECMA-118" },
    EncodingEntry { code_page: 28598, aliases: "hebrew ISO_8859-8 ISO-8859-8 csISOLatinHebrew iso-ir-138" },
    EncodingEntry { code_page: 28599, aliases: "latin5 ISO_8859-9 ISO-8859-9 csISOLatin5 iso-ir-148 l5" },
    // ISO-8859-10 and ISO-8859-11 are not supported on Windows (code_page -1).
    EncodingEntry { code_page: -1,    aliases: "" },  // latin6 / ISO-8859-10
    EncodingEntry { code_page: -1,    aliases: "" },  // ISO-8859-11
    EncodingEntry { code_page: 28603, aliases: "ISO_8859-13 ISO-8859-13" },
    EncodingEntry { code_page: 28604, aliases: "iso-celtic latin8 ISO_8859-14 ISO-8859-14 18 iso-ir-199" },
    EncodingEntry { code_page: 28605, aliases: "Latin-9 ISO_8859-15 ISO-8859-15" },
    // ISO-8859-16 is not supported on Windows.
    EncodingEntry { code_page: -1,    aliases: "" },  // latin10 / ISO-8859-16
    EncodingEntry { code_page: 437,   aliases: "IBM437 cp437 437 csPC8CodePage437" },
    EncodingEntry { code_page: 720,   aliases: "IBM720 cp720 oem720 720" },
    EncodingEntry { code_page: 737,   aliases: "IBM737 cp737 oem737 737" },
    EncodingEntry { code_page: 775,   aliases: "IBM775 cp775 oem775 775" },
    EncodingEntry { code_page: 850,   aliases: "IBM850 cp850 oem850 850" },
    EncodingEntry { code_page: 852,   aliases: "IBM852 cp852 oem852 852" },
    EncodingEntry { code_page: 855,   aliases: "IBM855 cp855 oem855 855 csIBM855" },
    EncodingEntry { code_page: 857,   aliases: "IBM857 cp857 oem857 857" },
    EncodingEntry { code_page: 858,   aliases: "IBM858 cp858 oem858 858" },
    EncodingEntry { code_page: 860,   aliases: "IBM860 cp860 oem860 860" },
    EncodingEntry { code_page: 861,   aliases: "IBM861 cp861 oem861 861" },
    EncodingEntry { code_page: 862,   aliases: "IBM862 cp862 oem862 862" },
    EncodingEntry { code_page: 863,   aliases: "IBM863 cp863 oem863 863" },
    EncodingEntry { code_page: 865,   aliases: "IBM865 cp865 oem865 865" },
    EncodingEntry { code_page: 866,   aliases: "IBM866 cp866 oem866 866" },
    EncodingEntry { code_page: 869,   aliases: "IBM869 cp869 oem869 869" },
    EncodingEntry { code_page: 950,   aliases: "big5 csBig5" },
    EncodingEntry { code_page: 936,   aliases: "gb2312 gbk csGB2312 gb18030" },
    EncodingEntry { code_page: 932,   aliases: "Shift_JIS MS_Kanji csShiftJIS csWindows31J" },
    EncodingEntry { code_page: 949,   aliases: "windows-949 korean" },
    EncodingEntry { code_page: 51949, aliases: "euc-kr csEUCKR" },
    EncodingEntry { code_page: 874,   aliases: "tis-620" },
    EncodingEntry { code_page: 10007, aliases: "x-mac-cyrillic xmaccyrillic" },
    EncodingEntry { code_page: 21866, aliases: "koi8_u" },
    EncodingEntry { code_page: 20866, aliases: "koi8_r csKOI8R" },
];

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Return `true` if `token` matches any space-separated word in `list`,
/// using a case-insensitive ASCII comparison.
///
/// This is the Rust equivalent of the `isInListA` function in EncodingMapper.cpp.
/// The original used `_stricmp` (Windows); we use `eq_ignore_ascii_case` which
/// is available in stable Rust and has the same semantics for the ASCII
/// characters found in codec names.
fn alias_matches(token: &str, list: &str) -> bool {
    if token.is_empty() || list.is_empty() {
        return false;
    }
    list.split_ascii_whitespace()
        .any(|word| word.eq_ignore_ascii_case(token))
}

// ─────────────────────────────────────────────────────────────────────────────
// EncodingMapper
// ─────────────────────────────────────────────────────────────────────────────

/// Maps between Notepad++ menu indices and Windows code-page integers.
///
/// This is a zero-sized type whose methods operate on the global `ENCODINGS`
/// table.  It mirrors the C++ `EncodingMapper` singleton; the singleton
/// pattern is replaced by associated functions that operate on shared
/// immutable state, which is idiomatic in Rust.
pub struct EncodingMapper;

impl EncodingMapper {
    // ── Public API ───────────────────────────────────────────────────────────

    /// Return the Windows code-page integer for the given 0-based menu index.
    ///
    /// Returns `-1` if `index` is out of range or if that encoding has no
    /// Windows code-page mapping.
    ///
    /// C++ equivalent: `EncodingMapper::getEncodingFromIndex`.
    pub fn get_encoding_from_index(index: i32) -> i32 {
        if index < 0 {
            return -1;
        }
        ENCODINGS
            .get(index as usize)
            .map(|e| e.code_page)
            .unwrap_or(-1)
    }

    /// Return the 0-based menu index for the given Windows code-page integer.
    ///
    /// Returns `-1` if the encoding is not in the table (including when
    /// `encoding == -1`).
    ///
    /// C++ equivalent: `EncodingMapper::getIndexFromEncoding`.
    pub fn get_index_from_encoding(encoding: i32) -> i32 {
        if encoding == -1 {
            return -1;
        }
        ENCODINGS
            .iter()
            .position(|e| e.code_page == encoding)
            .map(|i| i as i32)
            .unwrap_or(-1)
    }

    /// Return the Windows code-page integer for an IANA/MIME encoding alias
    /// string (e.g. `"utf-8"`, `"windows-1252"`, `"ISO-8859-1"`).
    ///
    /// The look-up is case-insensitive.  UTF-8 is a special case that returns
    /// `SC_CP_UTF8` (65001) directly, matching the Scintilla constant.
    ///
    /// Returns `-1` if the alias is not recognised.
    ///
    /// C++ equivalent: `EncodingMapper::getEncodingFromString`.
    pub fn get_encoding_from_string(alias: &str) -> i32 {
        // UTF-8 is handled specially in the C++ code; replicate that here.
        // SC_CP_UTF8 == 65001
        if alias_matches(alias, "utf-8 utf8") {
            return 65001;
        }
        ENCODINGS
            .iter()
            .find(|e| alias_matches(alias, e.aliases))
            .map(|e| e.code_page)
            .unwrap_or(-1)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_encoding_from_index_first_entry() {
        assert_eq!(EncodingMapper::get_encoding_from_index(0), 1250);
    }

    #[test]
    fn get_encoding_from_index_out_of_range() {
        assert_eq!(EncodingMapper::get_encoding_from_index(-1), -1);
        assert_eq!(EncodingMapper::get_encoding_from_index(9999), -1);
    }

    #[test]
    fn get_index_from_encoding_known() {
        let idx = EncodingMapper::get_index_from_encoding(1252);
        assert_eq!(idx, 2);  // windows-1252 is the third entry (0-based index 2)
    }

    #[test]
    fn get_index_from_encoding_negative_one() {
        assert_eq!(EncodingMapper::get_index_from_encoding(-1), -1);
    }

    #[test]
    fn get_encoding_from_string_utf8() {
        assert_eq!(EncodingMapper::get_encoding_from_string("utf-8"), 65001);
        assert_eq!(EncodingMapper::get_encoding_from_string("UTF-8"), 65001);
        assert_eq!(EncodingMapper::get_encoding_from_string("utf8"), 65001);
    }

    #[test]
    fn get_encoding_from_string_iso8859() {
        assert_eq!(EncodingMapper::get_encoding_from_string("ISO-8859-1"), 28591);
        assert_eq!(EncodingMapper::get_encoding_from_string("latin1"),     28591);
        assert_eq!(EncodingMapper::get_encoding_from_string("csISOLatin1"),28591);
    }

    #[test]
    fn get_encoding_from_string_case_insensitive() {
        assert_eq!(
            EncodingMapper::get_encoding_from_string("WINDOWS-1250"),
            1250,
        );
    }

    #[test]
    fn get_encoding_from_string_unknown() {
        assert_eq!(EncodingMapper::get_encoding_from_string("nonexistent"), -1);
        assert_eq!(EncodingMapper::get_encoding_from_string(""), -1);
    }

    #[test]
    fn alias_matches_basic() {
        assert!(alias_matches("latin1", "latin1 ISO_8859-1 ISO-8859-1"));
        assert!(alias_matches("ISO-8859-1", "latin1 ISO_8859-1 ISO-8859-1"));
        assert!(!alias_matches("utf-8", "latin1 ISO_8859-1 ISO-8859-1"));
    }

    #[test]
    fn roundtrip_index_encoding() {
        // For every entry with a valid code-page, going via index → encoding →
        // index should be identity.
        for (i, entry) in ENCODINGS.iter().enumerate() {
            if entry.code_page != -1 {
                let enc = EncodingMapper::get_encoding_from_index(i as i32);
                let idx = EncodingMapper::get_index_from_encoding(enc);
                assert_eq!(idx, i as i32, "roundtrip failed for entry {i}");
            }
        }
    }
}
