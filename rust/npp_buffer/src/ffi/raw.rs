// src/ffi/raw.rs
//
// Hand-written `extern "C"` FFI layer.
//
// This file contains all `unsafe` code in the crate.  It is the ONLY file
// in which `unsafe` blocks are permitted.
//
// Why extern "C" instead of cxx here?
// ────────────────────────────────────
// The interfaces in this file involve types that cxx cannot express:
//   • Win32 `HANDLE` (an opaque void pointer).
//   • Win32 `FILETIME` (a 64-bit value in two 32-bit halves, passed by value
//     in C++ but not representable as a cxx-safe type without a C shim).
//   • Raw `*const u16` / `*mut u16` (wide-char strings) — cxx maps strings
//     to `CxxString` (UTF-8) and has no UTF-16 type.
//   • `BufferState` raw pointer passed back to C++ as an opaque handle.
//
// Safety contract for callers (C++ side)
// ───────────────────────────────────────
// Every `unsafe` block below documents the specific invariant that the C++
// caller must satisfy.  The general rules are:
//   1. Pointer arguments must not be null unless the function's documentation
//      explicitly permits null.
//   2. String arguments (`*const u8`, `*const u16`) must be null-terminated and
//      point to memory that remains valid for the duration of the call.
//   3. `handle` arguments returned by `npp_buffer_*_new` must be freed with
//      the corresponding `npp_buffer_*_free` function and must not be used
//      after being freed.
//   4. No function is re-entrant on the same `handle` from multiple threads
//      simultaneously.  The C++ `FileManager` already enforces single-threaded
//      access; the Rust side relies on that invariant.

use crate::buffer::{BufferId, FileManager};
use crate::encoding_mapper::EncodingMapper;
use crate::types::{DocFileStatus, EolType, UniMode};
use crate::utf_convert::detect_encoding_from_bom;
use core::ffi::c_int;

// ─────────────────────────────────────────────────────────────────────────────
// EncodingMapper
// ─────────────────────────────────────────────────────────────────────────────
//
// These three functions expose the encoding mapper to C++ without needing a
// C++ object at all.  The C++ side declares them in a header and can call
// them directly, replacing calls to `EncodingMapper::getInstance().getXxx(…)`.

/// Return the Windows code-page integer for the given 0-based menu index.
///
/// # Safety
/// No pointers; always safe to call.
#[no_mangle]
pub extern "C" fn npp_encoding_from_index(index: c_int) -> c_int {
    // SAFETY: purely functional — no pointers, no global mutable state.
    EncodingMapper::get_encoding_from_index(index)
}

/// Return the 0-based menu index for the given Windows code-page integer.
///
/// # Safety
/// No pointers; always safe to call.
#[no_mangle]
pub extern "C" fn npp_encoding_index_from(encoding: c_int) -> c_int {
    // SAFETY: purely functional — no pointers, no global mutable state.
    EncodingMapper::get_index_from_encoding(encoding)
}

/// Return the Windows code-page integer for a null-terminated ASCII alias.
///
/// Returns `-1` if `alias` is null or unrecognised.
///
/// # Safety
/// `alias` must be a valid pointer to a null-terminated ASCII string or null.
#[no_mangle]
pub unsafe extern "C" fn npp_encoding_from_string(alias: *const u8) -> c_int {
    // SAFETY: caller guarantees `alias` is null or a null-terminated ASCII
    // string.  We produce a `&str` only after validating the pointer and
    // computing the length up to the NUL byte.
    if alias.is_null() {
        return -1;
    }
    // Walk to the NUL terminator without reading past it.
    let mut len = 0usize;
    while *alias.add(len) != 0 {
        len += 1;
    }
    let bytes = core::slice::from_raw_parts(alias, len);
    // The alias strings in the encoding table are ASCII; `from_utf8` will
    // always succeed here but we guard with `unwrap_or` for robustness.
    let s = core::str::from_utf8(bytes).unwrap_or("");
    EncodingMapper::get_encoding_from_string(s)
}

// ─────────────────────────────────────────────────────────────────────────────
// BOM detection
// ─────────────────────────────────────────────────────────────────────────────

/// Inspect the first `len` bytes of `buf` and return the `UniMode` discriminant
/// (as `u32`) of the detected BOM encoding.
///
/// Returns `u32::MAX` when no BOM is present.
///
/// Replaces `Utf8_16_Read::determineEncodingFromBOM`.
///
/// # Safety
/// `buf` must be a valid pointer to at least `len` readable bytes or null.
/// A null `buf` is treated as an empty slice (returns `u32::MAX`).
#[no_mangle]
pub unsafe extern "C" fn npp_detect_bom_encoding(buf: *const u8, len: usize) -> u32 {
    // SAFETY: caller guarantees `buf` points to `len` readable bytes or is
    // null.  We construct a slice only after checking for null.
    if buf.is_null() || len == 0 {
        return u32::MAX;
    }
    let slice = core::slice::from_raw_parts(buf, len);
    detect_encoding_from_bom(slice)
        .map(|m| m as u32)
        .unwrap_or(u32::MAX)
}

// ─────────────────────────────────────────────────────────────────────────────
// FileManager handle API
// ─────────────────────────────────────────────────────────────────────────────
//
// The C++ FileManager is a singleton.  We expose the Rust FileManager as an
// opaque heap-allocated handle so the C++ code can hold a pointer to it.
// Allocation / deallocation follow the standard "new/free" pattern used by
// many C APIs.

/// Allocate a new `FileManager` on the heap and return an opaque handle.
///
/// The returned pointer must be freed with `npp_file_manager_free`.
#[no_mangle]
pub extern "C" fn npp_file_manager_new() -> *mut FileManager {
    // SAFETY: `Box::into_raw` produces a non-null, properly aligned pointer.
    // Ownership is transferred to the C++ caller.
    Box::into_raw(Box::new(FileManager::new()))
}

/// Free a `FileManager` allocated by `npp_file_manager_new`.
///
/// # Safety
/// `handle` must be a non-null pointer previously returned by
/// `npp_file_manager_new` and must not have been freed already.
#[no_mangle]
pub unsafe extern "C" fn npp_file_manager_free(handle: *mut FileManager) {
    // SAFETY: caller guarantees `handle` is a valid, live pointer from
    // `npp_file_manager_new`.  `Box::from_raw` takes back ownership and the
    // resulting `Box` is dropped (freed) immediately.
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// Return the number of open buffers.
///
/// # Safety
/// `handle` must be a valid, non-null pointer from `npp_file_manager_new`.
#[no_mangle]
pub unsafe extern "C" fn npp_file_manager_buffer_count(handle: *const FileManager) -> usize {
    // SAFETY: `handle` is a valid, aligned, live pointer (caller contract).
    (*handle).buffer_count()
}

/// Return the number of unsaved (dirty) buffers.
///
/// # Safety
/// `handle` must be a valid, non-null pointer from `npp_file_manager_new`.
#[no_mangle]
pub unsafe extern "C" fn npp_file_manager_dirty_count(handle: *const FileManager) -> usize {
    // SAFETY: same as above.
    (*handle).dirty_buffer_count()
}

/// Create a new buffer record and return its `BufferId` (as `u64`).
///
/// `path` must be a null-terminated UTF-8 string.
/// `status` must be a valid `DocFileStatus` discriminant.
/// `is_large_file`: non-zero means the file is considered large.
///
/// Returns `0` (`BufferId::INVALID`) on failure.
///
/// # Safety
/// `handle` must be non-null and valid.  `path` must be non-null,
/// null-terminated, and valid UTF-8.
#[no_mangle]
pub unsafe extern "C" fn npp_file_manager_create_buffer(
    handle:       *mut FileManager,
    path:         *const u8,
    status:       u32,
    is_large_file: u8,
) -> u64 {
    // SAFETY: `handle` is valid (caller contract).
    // `path` is a null-terminated UTF-8 string (caller contract).
    if handle.is_null() || path.is_null() {
        return 0;
    }

    // Build a &str from the null-terminated C string.
    let mut len = 0usize;
    while *path.add(len) != 0 {
        len += 1;
    }
    let bytes = core::slice::from_raw_parts(path, len);
    let path_str = match core::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let file_status = match status {
        0x01 => DocFileStatus::Regular,
        0x02 => DocFileStatus::Unnamed,
        _ => return 0,
    };

    (*handle)
        .create_buffer(path_str, file_status, is_large_file != 0)
        .0
}

/// Mark a buffer dirty or clean.  `dirty` non-zero means dirty.
///
/// Returns the `BufferChangeFlags` bitmask (as `u32`) to forward to the
/// notification system; returns `0` if the buffer was not found.
///
/// # Safety
/// `handle` must be non-null and valid.
#[no_mangle]
pub unsafe extern "C" fn npp_buffer_set_dirty(
    handle:    *mut FileManager,
    buffer_id: u64,
    dirty:     u8,
) -> u32 {
    // SAFETY: `handle` is valid (caller contract).
    if handle.is_null() {
        return 0;
    }
    (*handle)
        .get_buffer_mut(BufferId(buffer_id))
        .map(|b| b.set_dirty(dirty != 0).0)
        .unwrap_or(0)
}

/// Set the encoding (Windows code-page integer, or -1 to use `unicode_mode`).
///
/// Returns the `BufferChangeFlags` bitmask or `0` if not found.
///
/// # Safety
/// `handle` must be non-null and valid.
#[no_mangle]
pub unsafe extern "C" fn npp_buffer_set_encoding(
    handle:    *mut FileManager,
    buffer_id: u64,
    encoding:  c_int,
) -> u32 {
    // SAFETY: `handle` is valid (caller contract).
    if handle.is_null() {
        return 0;
    }
    (*handle)
        .get_buffer_mut(BufferId(buffer_id))
        .map(|b| b.set_encoding(encoding).0)
        .unwrap_or(0)
}

/// Set the UniMode for a buffer.
///
/// `mode` must be a valid `UniMode` discriminant (0–7).
/// Returns the change flags bitmask or `0` if not found / invalid mode.
///
/// # Safety
/// `handle` must be non-null and valid.
#[no_mangle]
pub unsafe extern "C" fn npp_buffer_set_unicode_mode(
    handle:    *mut FileManager,
    buffer_id: u64,
    mode:      u8,
) -> u32 {
    // SAFETY: `handle` is valid (caller contract).  We validate `mode` before
    // constructing a `UniMode`.
    if handle.is_null() {
        return 0;
    }
    let uni_mode = match UniMode::from_raw(mode) {
        Some(m) => m,
        None => return 0,
    };
    (*handle)
        .get_buffer_mut(BufferId(buffer_id))
        .map(|b| b.set_unicode_mode(uni_mode).0)
        .unwrap_or(0)
}

/// Set the EOL format for a buffer.
///
/// `format` must be a valid `EolType` discriminant (0–3).
/// Returns the change flags bitmask or `0` if not found / invalid format.
///
/// # Safety
/// `handle` must be non-null and valid.
#[no_mangle]
pub unsafe extern "C" fn npp_buffer_set_eol_format(
    handle:    *mut FileManager,
    buffer_id: u64,
    format:    u8,
) -> u32 {
    // SAFETY: `handle` is valid (caller contract).
    if handle.is_null() {
        return 0;
    }
    let eol = match format {
        0 => EolType::Windows,
        1 => EolType::MacOs,
        2 => EolType::Unix,
        3 => EolType::Unknown,
        _ => return 0,
    };
    (*handle)
        .get_buffer_mut(BufferId(buffer_id))
        .map(|b| b.set_eol_format(eol).0)
        .unwrap_or(0)
}

/// Query whether a buffer is dirty.
///
/// Returns `1` if dirty, `0` if clean or not found.
///
/// # Safety
/// `handle` must be non-null and valid.
#[no_mangle]
pub unsafe extern "C" fn npp_buffer_is_dirty(
    handle:    *const FileManager,
    buffer_id: u64,
) -> u8 {
    // SAFETY: `handle` is valid (caller contract).
    if handle.is_null() {
        return 0;
    }
    (*handle)
        .get_buffer(BufferId(buffer_id))
        .map(|b| u8::from(b.is_dirty))
        .unwrap_or(0)
}

/// Query whether a buffer is read-only (user-set OR filesystem).
///
/// Returns `1` if read-only, `0` otherwise.
///
/// # Safety
/// `handle` must be non-null and valid.
#[no_mangle]
pub unsafe extern "C" fn npp_buffer_is_read_only(
    handle:    *const FileManager,
    buffer_id: u64,
) -> u8 {
    // SAFETY: `handle` is valid (caller contract).
    if handle.is_null() {
        return 0;
    }
    (*handle)
        .get_buffer(BufferId(buffer_id))
        .map(|b| u8::from(b.is_read_only()))
        .unwrap_or(0)
}

/// Query whether a buffer is untitled (never saved).
///
/// Returns `1` if untitled, `0` otherwise.
///
/// # Safety
/// `handle` must be non-null and valid.
#[no_mangle]
pub unsafe extern "C" fn npp_buffer_is_untitled(
    handle:    *const FileManager,
    buffer_id: u64,
) -> u8 {
    // SAFETY: `handle` is valid (caller contract).
    if handle.is_null() {
        return 0;
    }
    (*handle)
        .get_buffer(BufferId(buffer_id))
        .map(|b| u8::from(b.is_untitled()))
        .unwrap_or(0)
}

/// Add a Scintilla view reference to a buffer.
///
/// `referee` is the `ScintillaEditView*` pointer value cast to `u64`.
/// Returns the new reference count, or `0` if the buffer was not found.
///
/// # Safety
/// `handle` must be non-null and valid.
#[no_mangle]
pub unsafe extern "C" fn npp_buffer_add_reference(
    handle:    *mut FileManager,
    buffer_id: u64,
    referee:   u64,
) -> usize {
    // SAFETY: `handle` is valid (caller contract).
    if handle.is_null() {
        return 0;
    }
    (*handle)
        .get_buffer_mut(BufferId(buffer_id))
        .map(|b| b.add_reference(referee))
        .unwrap_or(0)
}

/// Remove a Scintilla view reference from a buffer.
///
/// Returns the remaining reference count, or `0` if the buffer was not found.
/// When the count reaches `0`, the C++ side should release the Scintilla
/// `Document` and call `npp_file_manager_remove_buffer`.
///
/// # Safety
/// `handle` must be non-null and valid.
#[no_mangle]
pub unsafe extern "C" fn npp_buffer_remove_reference(
    handle:    *mut FileManager,
    buffer_id: u64,
    referee:   u64,
) -> usize {
    // SAFETY: `handle` is valid (caller contract).
    if handle.is_null() {
        return 0;
    }
    (*handle)
        .get_buffer_mut(BufferId(buffer_id))
        .map(|b| b.remove_reference(referee))
        .unwrap_or(0)
}

/// Remove a buffer from the manager.
///
/// Returns `1` if removed, `0` if not found.
///
/// # Safety
/// `handle` must be non-null and valid.
#[no_mangle]
pub unsafe extern "C" fn npp_file_manager_remove_buffer(
    handle:    *mut FileManager,
    buffer_id: u64,
) -> u8 {
    // SAFETY: `handle` is valid (caller contract).
    if handle.is_null() {
        return 0;
    }
    u8::from((*handle).remove_buffer(BufferId(buffer_id)))
}
