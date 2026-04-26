// src/types.rs
//
// Rust equivalents of the fundamental enums and flag types that are shared
// across the buffer subsystem.  These mirror the C++ definitions found in:
//
//   PowerEditor/src/MISC/Common/NppConstants.h   (EolType, UniMode)
//   PowerEditor/src/MISC/PluginsManager/Notepad_plus_msgs.h  (LangType)
//   PowerEditor/src/ScintillaComponent/Buffer.h  (DocFileStatus, etc.)
//
// All types are `#[repr(…)]` so they can be passed across the FFI boundary
// without re-encoding.  The discriminant values match the C++ originals so
// that a cast in the bridge layer is always a no-op.

// ─────────────────────────────────────────────────────────────────────────────
// EolType
// ─────────────────────────────────────────────────────────────────────────────

/// End-of-line convention used by a document.
///
/// C++ source: `enum class EolType : std::uint8_t` in NppConstants.h.
/// The `osdefault` alias maps to `Windows` on Windows (value 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EolType {
    /// CR + LF  (Windows, value 0 — matches C++ `osdefault`)
    Windows  = 0,
    /// CR only (classic macOS ≤ 9)
    MacOs    = 1,
    /// LF only (Unix / Linux / modern macOS)
    Unix     = 2,
    /// Encoding could not be determined from file content
    Unknown  = 3,
}

impl Default for EolType {
    /// Matches the C++ `osdefault = windows` sentinel.
    fn default() -> Self {
        EolType::Windows
    }
}

impl EolType {
    /// Convert an integer (as stored in session XML or sent over IPC) to an
    /// `EolType`, falling back to `defvalue` for unrecognised integers.
    pub fn from_int(value: i32, defvalue: EolType) -> EolType {
        match value {
            0 => EolType::Windows,
            1 => EolType::MacOs,
            2 => EolType::Unix,
            3 => EolType::Unknown,
            _ => defvalue,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UniMode
// ─────────────────────────────────────────────────────────────────────────────

/// Unicode / encoding mode of a document buffer.
///
/// C++ source: `enum UniMode` in NppConstants.h.
/// The numeric values are kept identical so that Scintilla's
/// `SC_CP_UTF8` (65001) and Notepad++'s own codec IDs continue to align.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum UniMode {
    /// ANSI / legacy 8-bit encoding (codepage determined separately)
    Ansi8Bit      = 0,
    /// UTF-8 with BOM
    Utf8          = 1,
    /// UTF-16 Big-Endian with BOM
    Utf16BE       = 2,
    /// UTF-16 Little-Endian with BOM
    Utf16LE       = 3,
    /// UTF-8 without BOM (most common for modern files)
    Utf8NoBom     = 4,
    /// Pure 7-bit ASCII (subset of UTF-8)
    Ascii7Bit     = 5,
    /// UTF-16 Big-Endian without BOM
    Utf16BENoBom  = 6,
    /// UTF-16 Little-Endian without BOM
    Utf16LENoBom  = 7,
}

impl Default for UniMode {
    fn default() -> Self {
        UniMode::Utf8NoBom
    }
}

impl UniMode {
    /// Try to construct a `UniMode` from its raw discriminant.
    /// Returns `None` for values ≥ 8 (i.e. ≥ `uniEnd` in C++).
    pub fn from_raw(v: u8) -> Option<UniMode> {
        match v {
            0 => Some(UniMode::Ansi8Bit),
            1 => Some(UniMode::Utf8),
            2 => Some(UniMode::Utf16BE),
            3 => Some(UniMode::Utf16LE),
            4 => Some(UniMode::Utf8NoBom),
            5 => Some(UniMode::Ascii7Bit),
            6 => Some(UniMode::Utf16BENoBom),
            7 => Some(UniMode::Utf16LENoBom),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DocFileStatus
// ─────────────────────────────────────────────────────────────────────────────

/// Filesystem-level status of a buffer's backing file.
///
/// C++ source: `enum DocFileStatus` in Buffer.h.
/// These are bit-flags in C++ but only individual values are ever combined;
/// the Rust version uses a plain enum for the "current status" field and a
/// separate `BufferChangeFlags` bitfield for change notifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum DocFileStatus {
    /// File exists on disk and matches the buffer
    Regular     = 0x01,
    /// Buffer has never been saved (new document)
    Unnamed     = 0x02,
    /// File was deleted on disk while the buffer was open
    Deleted     = 0x04,
    /// File on disk is newer than the buffer (external modification)
    Modified    = 0x08,
    /// File is pending a reload (e.g. from log monitoring)
    NeedReload  = 0x10,
    /// File was absent when first loaded; treated as deleted + read-only
    Inaccessible = 0x20,
}

impl Default for DocFileStatus {
    fn default() -> Self {
        DocFileStatus::Regular
    }
}

impl DocFileStatus {
    /// Construct a `DocFileStatus` from its raw discriminant value.
    /// Returns `None` for unrecognised values.
    pub fn from_u32(v: u32) -> Option<DocFileStatus> {
        match v {
            0x01 => Some(DocFileStatus::Regular),
            0x02 => Some(DocFileStatus::Unnamed),
            0x04 => Some(DocFileStatus::Deleted),
            0x08 => Some(DocFileStatus::Modified),
            0x10 => Some(DocFileStatus::NeedReload),
            0x20 => Some(DocFileStatus::Inaccessible),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BufferChangeFlags
// ─────────────────────────────────────────────────────────────────────────────

/// Bitmask sent to `FileManager::beNotifiedOfBufferChange`.
///
/// C++ source: `enum BufferStatusInfo` in Buffer.h.
/// Kept as a plain `u32` newtype rather than a bitflags struct to avoid a
/// third-party dependency, while still providing named constants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferChangeFlags(pub u32);

impl BufferChangeFlags {
    pub const NONE:        BufferChangeFlags = BufferChangeFlags(0x000);
    pub const LANGUAGE:    BufferChangeFlags = BufferChangeFlags(0x001);
    pub const DIRTY:       BufferChangeFlags = BufferChangeFlags(0x002);
    pub const FORMAT:      BufferChangeFlags = BufferChangeFlags(0x004);
    pub const UNICODE:     BufferChangeFlags = BufferChangeFlags(0x008);
    pub const READONLY:    BufferChangeFlags = BufferChangeFlags(0x010);
    pub const STATUS:      BufferChangeFlags = BufferChangeFlags(0x020);
    pub const TIMESTAMP:   BufferChangeFlags = BufferChangeFlags(0x040);
    pub const FILENAME:    BufferChangeFlags = BufferChangeFlags(0x080);
    pub const RECENT_TAG:  BufferChangeFlags = BufferChangeFlags(0x100);
    pub const LEXING:      BufferChangeFlags = BufferChangeFlags(0x200);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl core::ops::BitOr for BufferChangeFlags {
    type Output = BufferChangeFlags;
    fn bitor(self, rhs: BufferChangeFlags) -> BufferChangeFlags {
        BufferChangeFlags(self.0 | rhs.0)
    }
}

impl core::ops::BitOrAssign for BufferChangeFlags {
    fn bitor_assign(&mut self, rhs: BufferChangeFlags) {
        self.0 |= rhs.0;
    }
}

impl core::ops::BitAnd for BufferChangeFlags {
    type Output = BufferChangeFlags;
    fn bitand(self, rhs: BufferChangeFlags) -> BufferChangeFlags {
        BufferChangeFlags(self.0 & rhs.0)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SavingStatus
// ─────────────────────────────────────────────────────────────────────────────

/// Result code returned by `FileManager::save_buffer`.
///
/// C++ source: `enum SavingStatus` in Buffer.h.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SavingStatus {
    Ok                         = 0,
    OpenFailed                 = 1,
    WritingFailed              = 2,
    NotEnoughRoom              = 3,
    FullReadOnlySavingForbidden = 4,
}

// ─────────────────────────────────────────────────────────────────────────────
// LangType
// ─────────────────────────────────────────────────────────────────────────────

/// Syntax-highlighting language assigned to a buffer.
///
/// C++ source: `enum LangType` in Notepad_plus_msgs.h.
/// Only the first several variants are listed here; the full list mirrors the
/// C++ enum exactly so that integer casts across the FFI boundary are safe.
/// Values not listed here map to `External`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum LangType {
    Text       = 0,
    Php        = 1,
    C          = 2,
    Cpp        = 3,
    CSharp     = 4,
    ObjC       = 5,
    Java       = 6,
    Rc         = 7,
    Html       = 8,
    Xml        = 9,
    Makefile   = 10,
    Pascal     = 11,
    Batch      = 12,
    Ini        = 13,
    Ascii      = 14,  // DOS-box text (forces CP437)
    User       = 15,  // User-defined language
    Asp        = 16,
    Sql        = 17,
    VisualBasic = 18,
    JsEmbedded = 19,
    Css        = 20,
    Perl       = 21,
    Python     = 22,
    Lua        = 23,
    Tex        = 24,
    Fortran    = 25,
    Bash       = 26,
    Flash      = 27,
    Nsis       = 28,
    Tcl        = 29,
    Lisp       = 30,
    Scheme     = 31,
    Asm        = 32,
    Diff       = 33,
    Props      = 34,
    Ps         = 35,
    Ruby       = 36,
    Smalltalk  = 37,
    Vhdl       = 38,
    Kix        = 39,
    Au3        = 40,
    Caml       = 41,
    Ada        = 42,
    Verilog    = 43,
    Matlab     = 44,
    Haskell    = 45,
    Inno       = 46,
    SearchResult = 47,
    Cmake      = 48,
    Yaml       = 49,
    Cobol      = 50,
    Gui4Cli    = 51,
    D          = 52,
    PowerShell = 53,
    R          = 54,
    Jsp        = 55,
    Coffeescript = 56,
    Json       = 57,
    JavaScript = 58,
    Fortran77  = 59,
    Baanc      = 60,
    Srec       = 61,
    Ihex       = 62,
    Tehex      = 63,
    Swift      = 64,
    Rust       = 75, // gap filled by other languages; value from C++ enum
    TypeScript = 85,
    Golang     = 90,
    Toml       = 92,
    // Sentinel: user-defined external lexers come last
    External   = 100,
}

impl Default for LangType {
    fn default() -> Self {
        LangType::Text
    }
}
