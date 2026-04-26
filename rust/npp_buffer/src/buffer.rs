// src/buffer.rs
//
// Pure-Rust migration of the *document-state* portion of Buffer.cpp / Buffer.h.
//
// Scope of this migration step
// ─────────────────────────────
// Buffer in C++ is responsible for two distinct concerns:
//
//   1. Pure document state (encoding, language, EOL, dirty flag, read-only,
//      reference counting, etc.)  ← migrated here, in safe Rust.
//
//   2. Win32 filesystem operations (file timestamps, monitoring handles,
//      backup file management, large file loading)
//      ← deferred to Phase 2; the FFI boundary stubs are in `ffi/mod.rs`.
//
// Design notes
// ─────────────
// • The C++ `Buffer` class uses a raw pointer (`_id`) as a unique identifier.
//   In Rust we use a plain `u64` ID.  The `FileManager` hands out `BufferId`
//   values and owns the `BufferState` structs in a `Vec`.
//
// • C++ mutation triggers `doNotify(mask)` which calls back into
//   `FileManager::beNotifiedOfBufferChange`.  In Rust, setters return a
//   `BufferChangeFlags` value; it is the caller's responsibility to forward
//   those flags to the notification channel (e.g. through the `ffi` layer).
//   This keeps `BufferState` free of any callback pointer and therefore fully
//   thread-safe.
//
// • Win32-specific fields (`FILETIME _timeStamp`, `HANDLE _eventHandle`, etc.)
//   are replaced by platform-neutral placeholders.  On Windows, the cxx bridge
//   maps them back to their Win32 types.

use crate::types::{
    BufferChangeFlags, DocFileStatus, EolType, LangType, UniMode,
};

// ─────────────────────────────────────────────────────────────────────────────
// Stable unique ID for a buffer
// ─────────────────────────────────────────────────────────────────────────────

/// Opaque, stable identifier for a `BufferState`.
///
/// In C++ this was the raw `Buffer*` pointer cast to `BufferID`.  We use a
/// monotonically increasing `u64` so that IDs remain valid even when the
/// underlying `Vec` is reallocated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct BufferId(pub u64);

impl BufferId {
    /// Sentinel value meaning "no buffer" (matches `BUFFER_INVALID` in C++).
    pub const INVALID: BufferId = BufferId(0);

    pub fn is_valid(self) -> bool {
        self.0 != 0
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Cursor / scroll position saved per-view
// ─────────────────────────────────────────────────────────────────────────────

/// Saved cursor and scroll position within a Scintilla view.
///
/// Mirrors the C++ `Position` struct used in `Buffer::_positions`.
#[derive(Clone, Debug, Default)]
pub struct Position {
    /// Byte offset of the caret in the Scintilla document.
    pub caret:        i64,
    /// First visible line (scroll position).
    pub top_line:     i64,
    /// Horizontal scroll offset in pixels.
    pub x_offset:     i32,
    /// Byte offset of the start of the selection anchor.
    pub anchor:       i64,
    /// Column number of the caret (for sticky column tracking).
    pub column:       i32,
}

// ─────────────────────────────────────────────────────────────────────────────
// BufferState
// ─────────────────────────────────────────────────────────────────────────────

/// Document-state record for one open file in Notepad++.
///
/// This is the Rust equivalent of the data members of the C++ `Buffer` class
/// that do not require Win32 APIs.  Win32-dependent state (file timestamps,
/// monitoring event handle, etc.) lives in the FFI layer.
///
/// # Thread safety
/// All fields are accessed through `&mut self` (exclusive access), which means
/// the wrapping container (`FileManager`) must hold an appropriate lock before
/// calling any setter.  This matches the C++ behaviour where the UI thread
/// serialises all buffer mutations.
#[derive(Debug)]
pub struct BufferState {
    // ── Identity ─────────────────────────────────────────────────────────────

    /// Stable identifier assigned at construction; never changes.
    pub id: BufferId,

    /// Full path to the file on disk (UTF-8 on all platforms; UTF-16 on the
    /// C++ side — the bridge converts at the boundary).
    pub full_path: String,

    // ── Document properties ───────────────────────────────────────────────────

    /// Syntax-highlighting language assigned to this buffer.
    pub lang: LangType,

    /// User-defined language name (valid only when `lang == LangType::User`).
    pub user_lang_ext: String,

    /// `true` if the in-memory document differs from the on-disk file.
    pub is_dirty: bool,

    /// End-of-line convention used in this document.
    pub eol_format: EolType,

    /// Code-page integer used when `encoding != -1`.
    /// When `encoding == -1`, `unicode_mode` is authoritative.
    pub encoding: i32,

    /// Unicode / encoding mode when `encoding == -1`.
    pub unicode_mode: UniMode,

    // ── Read-only state ───────────────────────────────────────────────────────

    /// Set by the user explicitly (right-click → "Read-only").
    pub is_user_read_only: bool,

    /// Set when the backing file is read-only on the filesystem.
    pub is_file_read_only: bool,

    // ── Filesystem status ─────────────────────────────────────────────────────

    /// The current relationship between this buffer and its backing file.
    pub current_status: DocFileStatus,

    /// `true` if the backing file is on a network share.
    pub is_from_network: bool,

    // ── Lifecycle flags ───────────────────────────────────────────────────────

    /// `true` if the buffer needs to be reloaded on next activation.
    pub need_reloading: bool,

    /// `true` if the lexer needs to re-scan the document on next display.
    pub need_lexer: bool,

    /// `true` if the user has explicitly set the language from the menu,
    /// suppressing the automatic language-from-extension logic.
    pub has_lang_been_set_from_menu: bool,

    /// `true` if this buffer was loaded from a very large file.
    /// Disables auto-completion, backup snapshots, and word-wrap.
    pub is_large_file: bool,

    /// `true` after the document has been converted to a different encoding
    /// and the undo stack was cleared; keeps the buffer dirty until it is
    /// explicitly saved.
    pub is_save_point_dirty: bool,

    /// `true` when the buffer is "unsynchronised" with the file on disk:
    /// either the file was deleted externally or modified by another process
    /// and the user declined to reload.
    pub is_unsync: bool,

    /// `true` if the buffer is inaccessible (absent when first loaded).
    pub is_inaccessible: bool,

    /// `true` after the buffer's tab label was renamed by the user.
    pub is_untitled_tab_renamed: bool,

    /// `true` if this was a "dirty-loaded" buffer that needs tracking.
    pub is_loaded_dirty: bool,

    // ── Per-view state ────────────────────────────────────────────────────────

    /// One `Position` per Scintilla view that has this buffer open.
    /// Parallel to `fold_states` and `referee_ids`.
    pub positions:    Vec<Position>,

    /// Fold (outline) state for each registered Scintilla view.
    pub fold_states:  Vec<Vec<usize>>,

    /// Opaque view identifiers (Scintilla pointer values, cast to `u64` by the
    /// bridge).  Used to map from a view to its entry in `positions` /
    /// `fold_states`.
    pub referee_ids:  Vec<u64>,

    // ── Miscellaneous ─────────────────────────────────────────────────────────

    /// Monotonically increasing "recency" tag used by the recent-file list.
    pub recent_tag: i64,

    /// Colour slot allocated for this buffer's tab indicator.  -1 = none.
    pub doc_color_id: i32,

    /// Path to the auto-backup file, or empty if none exists.
    pub backup_file_name: String,

    /// `true` if the on-disk file has been externally modified since load.
    pub is_modified: bool,

    /// RTL (right-to-left) reading direction for this buffer's view.
    pub is_rtl: bool,

    /// `true` if the tab for this buffer is "pinned".
    pub is_pinned: bool,
}

impl BufferState {
    // ── Constructor ───────────────────────────────────────────────────────────

    /// Create a new `BufferState` with sensible defaults.
    ///
    /// `initial_status` must be either `DocFileStatus::Regular` (loaded from
    /// disk) or `DocFileStatus::Unnamed` (new unsaved document).
    pub fn new(
        id:             BufferId,
        full_path:      impl Into<String>,
        initial_status: DocFileStatus,
        is_large_file:  bool,
    ) -> Self {
        BufferState {
            id,
            full_path:                   full_path.into(),
            lang:                        LangType::Text,
            user_lang_ext:               String::new(),
            is_dirty:                    false,
            eol_format:                  EolType::default(),
            encoding:                    -1,
            unicode_mode:                UniMode::Utf8NoBom,
            is_user_read_only:           false,
            is_file_read_only:           false,
            current_status:              initial_status,
            is_from_network:             false,
            need_reloading:              false,
            need_lexer:                  false,
            has_lang_been_set_from_menu: false,
            is_large_file,
            is_save_point_dirty:         false,
            is_unsync:                   false,
            is_inaccessible:             false,
            is_untitled_tab_renamed:     false,
            is_loaded_dirty:             false,
            positions:                   Vec::new(),
            fold_states:                 Vec::new(),
            referee_ids:                 Vec::new(),
            recent_tag:                  -1,
            doc_color_id:                -1,
            backup_file_name:            String::new(),
            is_modified:                 false,
            is_rtl:                      false,
            is_pinned:                   false,
        }
    }

    // ── Read-only predicates ─────────────────────────────────────────────────

    /// `true` if the user or the file system has marked this buffer read-only.
    pub fn is_read_only(&self) -> bool {
        self.is_user_read_only || self.is_file_read_only
    }

    /// `true` if the buffer has never been saved.
    pub fn is_untitled(&self) -> bool {
        self.current_status == DocFileStatus::Unnamed
    }

    // ── Setters that return change flags ─────────────────────────────────────

    /// Mark the document dirty (or clean) and return the change mask.
    pub fn set_dirty(&mut self, dirty: bool) -> BufferChangeFlags {
        self.is_dirty = dirty;
        BufferChangeFlags::DIRTY
    }

    /// Set the encoding and return the change mask.
    ///
    /// Passing `-1` clears a previously set ANSI encoding and restores the
    /// `unicode_mode` field as the authority.
    pub fn set_encoding(&mut self, encoding: i32) -> BufferChangeFlags {
        self.encoding = encoding;
        BufferChangeFlags::UNICODE | BufferChangeFlags::DIRTY
    }

    /// Set the Unicode mode and return the change mask.
    pub fn set_unicode_mode(&mut self, mode: UniMode) -> BufferChangeFlags {
        self.unicode_mode = mode;
        BufferChangeFlags::UNICODE | BufferChangeFlags::DIRTY
    }

    /// Set the EOL convention and return the change mask.
    pub fn set_eol_format(&mut self, format: EolType) -> BufferChangeFlags {
        self.eol_format = format;
        BufferChangeFlags::FORMAT
    }

    /// Set the syntax language and return the change mask.
    ///
    /// If `lang` is `LangType::User` then `user_lang_name` must be provided.
    /// Setting the same non-User language twice is a no-op (returns
    /// `NONE`), matching the C++ guard in `Buffer::setLangType`.
    ///
    /// C++ equivalent: `Buffer::setLangType`.
    pub fn set_lang_type(
        &mut self,
        lang: LangType,
        user_lang_name: &str,
    ) -> BufferChangeFlags {
        if lang == self.lang && lang != LangType::User {
            return BufferChangeFlags::NONE;
        }
        self.lang = lang;
        if lang == LangType::User {
            self.user_lang_ext = user_lang_name.to_owned();
        } else if lang == LangType::Ascii {
            // DOS-box text forces CP437 (matches C++ `NPP_CP_DOS_437 == 437`).
            self.encoding = 437;
        }
        self.need_lexer = true;
        BufferChangeFlags::LANGUAGE | BufferChangeFlags::LEXING
    }

    /// Set the user read-only flag and return the change mask.
    pub fn set_user_read_only(&mut self, ro: bool) -> BufferChangeFlags {
        self.is_user_read_only = ro;
        BufferChangeFlags::READONLY
    }

    /// Set the filesystem read-only flag and return the change mask.
    pub fn set_file_read_only(&mut self, ro: bool) -> BufferChangeFlags {
        self.is_file_read_only = ro;
        BufferChangeFlags::READONLY
    }

    /// Defer a reload: mark the buffer clean but schedule a full reload on the
    /// next activation.  Returns the `DIRTY` change flag.
    pub fn set_deferred_reload(&mut self) -> BufferChangeFlags {
        self.is_dirty = false;
        self.need_reloading = true;
        BufferChangeFlags::DIRTY
    }

    // ── Reference management ─────────────────────────────────────────────────

    /// Register a Scintilla view with this buffer.
    ///
    /// `referee_id` is the raw pointer value (cast to `u64`) of the
    /// `ScintillaEditView*` on the C++ side.
    ///
    /// Returns the new reference count, which matches the C++
    /// `Buffer::addReference` return value.
    pub fn add_reference(&mut self, referee_id: u64) -> usize {
        if self.index_of_reference(referee_id).is_none() {
            self.referee_ids.push(referee_id);
            self.positions.push(Position::default());
            self.fold_states.push(Vec::new());
        }
        self.referee_ids.len()
    }

    /// Unregister a Scintilla view from this buffer.
    ///
    /// Returns the remaining reference count.  When it reaches 0 the
    /// `FileManager` should free the buffer's Scintilla `Document`.
    pub fn remove_reference(&mut self, referee_id: u64) -> usize {
        if let Some(idx) = self.index_of_reference(referee_id) {
            self.referee_ids.remove(idx);
            self.positions.remove(idx);
            self.fold_states.remove(idx);
        }
        self.referee_ids.len()
    }

    /// Set the saved `Position` for a given view.
    pub fn set_position(&mut self, referee_id: u64, pos: Position) {
        if let Some(idx) = self.index_of_reference(referee_id) {
            self.positions[idx] = pos;
        }
    }

    /// Get the saved `Position` for a given view, or a default value.
    pub fn get_position(&self, referee_id: u64) -> Option<&Position> {
        self.index_of_reference(referee_id)
            .map(|idx| &self.positions[idx])
    }

    /// Set the fold state for a given view.
    pub fn set_fold_state(&mut self, referee_id: u64, folds: Vec<usize>) {
        if let Some(idx) = self.index_of_reference(referee_id) {
            self.fold_states[idx] = folds;
        }
    }

    /// Get the fold state for a given view.
    pub fn get_fold_state(&self, referee_id: u64) -> Option<&Vec<usize>> {
        self.index_of_reference(referee_id)
            .map(|idx| &self.fold_states[idx])
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn index_of_reference(&self, referee_id: u64) -> Option<usize> {
        self.referee_ids.iter().position(|&r| r == referee_id)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FileManager (pure state)
// ─────────────────────────────────────────────────────────────────────────────

/// Manages the collection of open `BufferState` records.
///
/// This is the Rust equivalent of the state-management portions of the C++
/// `FileManager` class.  File I/O operations (`loadFile`, `saveBuffer`,
/// `backupCurrentBuffer`, etc.) are not included in this step; they depend on
/// Win32 APIs and will be migrated in Phase 2.
///
/// # Singleton vs. dependency injection
/// The C++ code uses a global singleton (`FileManager::getInstance()`).
/// The Rust version is a plain struct.  In the hybrid build the cxx bridge
/// wraps it in a `static Mutex<FileManager>` exposed as a thread-safe global.
#[derive(Debug, Default)]
pub struct FileManager {
    buffers:       Vec<BufferState>,
    next_id:       u64,
}

impl FileManager {
    /// Create an empty `FileManager`.
    pub fn new() -> Self {
        FileManager {
            buffers: Vec::new(),
            next_id: 1,  // 0 is reserved for `BufferId::INVALID`
        }
    }

    // ── Buffer lifecycle ─────────────────────────────────────────────────────

    /// Allocate a new `BufferState` and return its `BufferId`.
    pub fn create_buffer(
        &mut self,
        full_path:      impl Into<String>,
        initial_status: DocFileStatus,
        is_large_file:  bool,
    ) -> BufferId {
        let id = BufferId(self.next_id);
        self.next_id += 1;
        self.buffers.push(BufferState::new(id, full_path, initial_status, is_large_file));
        id
    }

    /// Return the number of open buffers.
    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    /// Return the number of dirty (unsaved) buffers.
    pub fn dirty_buffer_count(&self) -> usize {
        self.buffers.iter().filter(|b| b.is_dirty).count()
    }

    /// Look up a buffer by ID.  Returns `None` for `BufferId::INVALID` or
    /// unknown IDs.
    pub fn get_buffer(&self, id: BufferId) -> Option<&BufferState> {
        self.buffers.iter().find(|b| b.id == id)
    }

    /// Mutable look-up by ID.
    pub fn get_buffer_mut(&mut self, id: BufferId) -> Option<&mut BufferState> {
        self.buffers.iter_mut().find(|b| b.id == id)
    }

    /// Return a buffer by its 0-based index in the internal list.
    pub fn get_buffer_by_index(&self, index: usize) -> Option<&BufferState> {
        self.buffers.get(index)
    }

    /// Find a buffer by its full file path.
    pub fn get_buffer_by_path(&self, path: &str) -> Option<&BufferState> {
        self.buffers.iter().find(|b| b.full_path == path)
    }

    /// Find the 0-based index of a buffer by ID, or return `None`.
    pub fn index_of(&self, id: BufferId) -> Option<usize> {
        self.buffers.iter().position(|b| b.id == id)
    }

    /// Remove a buffer from the manager when all Scintilla references have been
    /// dropped (reference count reaches 0).
    ///
    /// Returns `true` if the buffer was found and removed.
    pub fn remove_buffer(&mut self, id: BufferId) -> bool {
        if let Some(idx) = self.index_of(id) {
            self.buffers.remove(idx);
            true
        } else {
            false
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SavingStatus;

    fn make_manager() -> FileManager {
        FileManager::new()
    }

    #[test]
    fn create_and_look_up_buffer() {
        let mut fm = make_manager();
        let id = fm.create_buffer("/tmp/hello.txt", DocFileStatus::Regular, false);
        assert!(id.is_valid());
        let buf = fm.get_buffer(id).unwrap();
        assert_eq!(buf.full_path, "/tmp/hello.txt");
        assert!(!buf.is_dirty);
    }

    #[test]
    fn buffer_id_invalid_sentinel() {
        let fm = make_manager();
        assert!(fm.get_buffer(BufferId::INVALID).is_none());
    }

    #[test]
    fn dirty_count() {
        let mut fm = make_manager();
        let id1 = fm.create_buffer("/a.txt", DocFileStatus::Regular, false);
        let id2 = fm.create_buffer("/b.txt", DocFileStatus::Regular, false);
        assert_eq!(fm.dirty_buffer_count(), 0);

        fm.get_buffer_mut(id1).unwrap().set_dirty(true);
        assert_eq!(fm.dirty_buffer_count(), 1);

        fm.get_buffer_mut(id2).unwrap().set_dirty(true);
        assert_eq!(fm.dirty_buffer_count(), 2);
    }

    #[test]
    fn remove_buffer() {
        let mut fm = make_manager();
        let id = fm.create_buffer("/x.txt", DocFileStatus::Regular, false);
        assert_eq!(fm.buffer_count(), 1);
        assert!(fm.remove_buffer(id));
        assert_eq!(fm.buffer_count(), 0);
        assert!(!fm.remove_buffer(id));  // removing again is safe
    }

    #[test]
    fn reference_management() {
        let mut fm = make_manager();
        let id = fm.create_buffer("/r.txt", DocFileStatus::Regular, false);
        let buf = fm.get_buffer_mut(id).unwrap();

        let view_a: u64 = 0xDEAD;
        let view_b: u64 = 0xBEEF;

        assert_eq!(buf.add_reference(view_a), 1);
        assert_eq!(buf.add_reference(view_b), 2);
        // Adding the same view again is idempotent.
        assert_eq!(buf.add_reference(view_a), 2);

        assert_eq!(buf.remove_reference(view_a), 1);
        assert_eq!(buf.remove_reference(view_b), 0);
    }

    #[test]
    fn set_lang_noop_for_same_lang() {
        let mut fm = make_manager();
        let id = fm.create_buffer("/code.py", DocFileStatus::Regular, false);
        let buf = fm.get_buffer_mut(id).unwrap();

        // First call: language changes from Text → Python; need_lexer becomes true.
        buf.set_lang_type(LangType::Python, "");
        assert!(buf.need_lexer);

        // Reset the lexer flag so we can detect whether the second call touches it.
        buf.need_lexer = false;

        // Setting the same non-User language again is a no-op.
        let flags = buf.set_lang_type(LangType::Python, "");
        assert_eq!(flags, BufferChangeFlags::NONE);
        assert!(!buf.need_lexer);
    }

    #[test]
    fn set_lang_ascii_forces_cp437() {
        let mut fm = make_manager();
        let id = fm.create_buffer("/old.txt", DocFileStatus::Regular, false);
        let buf = fm.get_buffer_mut(id).unwrap();
        buf.set_lang_type(LangType::Ascii, "");
        assert_eq!(buf.encoding, 437);
    }

    #[test]
    fn saving_status_ok() {
        // Just verify the enum exists and can be compared.
        assert_eq!(SavingStatus::Ok, SavingStatus::Ok);
        assert_ne!(SavingStatus::Ok, SavingStatus::OpenFailed);
    }
}
