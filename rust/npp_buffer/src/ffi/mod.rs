// src/ffi/mod.rs
//
// FFI boundary module — the ONLY place in this crate where `unsafe` code is
// permitted.
//
// Responsibility
// ──────────────
// This module bridges the Rust implementation in `buffer`, `encoding_mapper`,
// and `utf_convert` to the C++ code that has not yet been migrated.  Every
// `unsafe` block in this file must have an accompanying comment explaining:
//   1. Why `unsafe` is required.
//   2. What invariant(s) the caller must uphold for the code to be sound.
//
// Sub-modules
// ───────────
// • `bridge`  — cxx-based bridge (enabled when the `cxx-bridge` feature is
//               active).  Provides type-safe C++ ↔ Rust interop for types
//               that cxx can express (plain structs, slices, `&str`).
//
// • `raw`     — hand-written `extern "C"` functions.  Used for interfaces that
//               cxx cannot express cleanly:
//                 – Win32 HANDLE / FILETIME / pointer-sized integers that cxx
//                   does not model.
//                 – Callback function pointers.
//                 – The global singleton FileManager that must survive for the
//                   entire process lifetime.
//
// Calling convention
// ──────────────────
// All exported `extern "C"` functions use the standard C calling convention.
// On Windows (MSVC target) this is `__cdecl`, which is also the default for
// MSVC `extern "C"` functions; no `__stdcall` wrapper is needed.

pub mod bridge;
pub mod raw;
