// lib.rs — npp_buffer crate root
//
// This crate is the first incremental Rust rewrite of the Notepad++ core
// editor buffer subsystem.  See `docs/adr/001-core-buffer-migration.md` for
// the full Architectural Decision Record.
//
// Module layout
// ─────────────
//   types           — shared enums and flag types (EolType, UniMode, LangType, …)
//   encoding_mapper — bidirectional index ↔ code-page mapping (EncodingMapper.cpp)
//   utf_convert     — UTF-8/16 BOM detection and encoding conversion (Utf8_16.cpp)
//   buffer          — document state (Buffer.cpp, pure-state portions only)
//   ffi             — ALL unsafe code; C++/Rust ABI boundary
//     ffi::bridge   — cxx type-safe bridge (enabled with `--features cxx-bridge`)
//     ffi::raw      — hand-written extern "C" functions for Win32-heavy interfaces
//
// Unsafe policy
// ─────────────
// `unsafe` code is FORBIDDEN outside the `ffi` module.  Every `unsafe` block
// inside `ffi` carries a comment explaining the invariant that makes it sound.
// Violations of this policy should be treated as bugs and reported immediately.

// Lint configuration — all warnings are errors in CI.
// We suppress only `unsafe_code` for the `ffi` module (where it is expected).
#![deny(unsafe_code)]       // forbid unsafe outside ffi/
#![forbid(unused_unsafe)]   // catch dead unsafe blocks

pub mod buffer;
pub mod encoding_mapper;
pub mod types;
pub mod utf_convert;

// Allow unsafe inside the ffi module only.
#[allow(unsafe_code)]
pub mod ffi;
