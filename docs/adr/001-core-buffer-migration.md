# ADR 001 — Migration of Core Editor Buffer from C++ to Rust — Step 1

**Status**: Accepted  
**Date**: 2026-04-26  
**Authors**: Engineering team (migration project)

---

## Context

### What is the core editor buffer?

Notepad++ manages open files through a subsystem called the *core editor buffer*. Each file the user opens is represented by a `Buffer` object (defined in `Buffer.cpp` / `Buffer.h`) that holds:

- The document's *encoding* — which character set and byte representation (UTF-8, UTF-16, Windows-1252, etc.) the file uses.
- The *end-of-line* convention — Windows (`\r\n`), Unix (`\n`), or classic macOS (`\r`).
- The *syntax language* — Python, C++, JSON, etc. — used to choose the lexer.
- *Status flags* — whether the file has unsaved changes (dirty), is read-only, has been deleted on disk, etc.
- *Per-view saved state* — cursor position and code-folding layout for each open editor pane.

A second class, `FileManager`, acts as the owner and registry of all `Buffer` instances. It loads files from disk, manages reference counts between open editor panes (Scintilla views), and co-ordinates save/reload/backup operations.

Two supporting files complete the subsystem:

- **`EncodingMapper.cpp`** — A static look-up table that maps Notepad++'s sequential menu indices (used by the Format menu) to Windows code-page integers and to IANA/MIME charset strings (used when parsing HTTP Content-Type or XML declarations).

- **`Utf8_16.cpp`** — A byte-level encoding-conversion library that detects the encoding of a newly opened file from its BOM (byte-order mark), and converts between UTF-16 (which Win32 file I/O uses) and UTF-8 (which Scintilla stores internally).

### Why start here?

The core buffer was chosen as Step 1 for three reasons:

1. **Self-contained logic**: `EncodingMapper` has zero external dependencies — it is a pure look-up table. `Utf8_16.cpp` has no Win32 calls in its encoding-detection and conversion paths. The pure-state portion of `Buffer` (encoding, language, EOL, dirty flag) is similarly platform-neutral.

2. **High leverage**: These files are touched by almost every file-open and file-save operation. Getting their logic into safe Rust early provides a stable, tested foundation for the rest of the migration.

3. **Clear FFI boundary**: The C++ *caller* of the buffer subsystem is the UI layer (`Notepad_plus.cpp`, `NppIO.cpp`, `ScintillaEditView.cpp`). That boundary is well-defined: a small number of `Buffer` / `FileManager` entry points. It is straightforward to expose Rust implementations behind `extern "C"` functions without disturbing the callers.

### Constraints

- **Stable Rust only** — No nightly-only features, no `unsafe` outside the designated FFI boundary module. Rust 2021 edition.
- **Incremental migration** — The C++ files being replaced (`Buffer.cpp`, `EncodingMapper.cpp`, `Utf8_16.cpp`) remain in the repository and continue to be referenced by the C++ build until the hybrid build is fully wired up in a later step. At no point should the project fail to build.
- **ABI compatibility** — The remaining C++ code must continue to compile without modification. The Rust library is linked as a static library (`libnpp_buffer.a`) and the C++ code calls into it through `extern "C"` function declarations.
- **Windows target** — The production binary targets Windows (MSVC or MinGW). The Rust crate must produce code that links cleanly with MSVC's runtime.

---

## Decision

### 1. Scope of Step 1

Step 1 migrates the following C++ files:

| C++ File | Rust Equivalent |
|---|---|
| `EncodingMapper.cpp` / `.h` | `rust/npp_buffer/src/encoding_mapper.rs` |
| `Utf8_16.cpp` / `.h` (detection + conversion) | `rust/npp_buffer/src/utf_convert.rs` |
| `Buffer.cpp` / `.h` (pure state only) | `rust/npp_buffer/src/buffer.rs` |

The Win32-dependent portions of `Buffer.cpp` (file timestamp reading, monitoring `HANDLE`, Win32 file I/O for save/load) are **not migrated in this step**. They are deferred to Step 2 and documented explicitly in the summary table. See the *Consequences* section for reasoning.

### 2. Module structure

```
rust/
  Cargo.toml                   – workspace manifest
  npp_buffer/
    Cargo.toml                 – crate manifest; declares the `cxx-bridge` feature
    build.rs                   – compiles the C++ side of the cxx bridge (feature-gated)
    src/
      lib.rs                   – module root; enforces the unsafe-only-in-ffi policy
      types.rs                 – shared enums: EolType, UniMode, LangType, DocFileStatus, …
      encoding_mapper.rs       – EncodingMapper logic (pure Rust, no FFI)
      utf_convert.rs           – UTF-8/16 BOM detection and conversion (pure Rust, no FFI)
      buffer.rs                – Buffer document state + FileManager (pure Rust, no FFI)
      ffi/
        mod.rs                 – declares sub-modules; documents the unsafe policy
        bridge.rs              – cxx bridge (feature-gated; type-safe C++ ↔ Rust)
        raw.rs                 – hand-written extern "C" functions; ALL unsafe code lives here
```

`lib.rs` contains `#![deny(unsafe_code)]` at the crate root and then `#[allow(unsafe_code)]` on `mod ffi` only. This enforces at compile time that no `unsafe` code appears outside the FFI boundary.

### 3. ABI strategy: cxx bridge vs. extern "C"

Two strategies are used, each for specific reasons:

**cxx bridge** (`src/ffi/bridge.rs`, guarded by the `cxx-bridge` Cargo feature):

Used for the `EncodingMapper` and BOM-detection interfaces because:
- Their signatures involve only types cxx supports: `i32`, `&str`, `&[u8]`, `u32`.
- cxx generates both the Rust glue and the C++ header from a single source, preventing declaration drift.
- cxx's `rust::Str` type prevents the C++ side from passing a dangling `const char*` into Rust.

C++ signatures generated by cxx (in namespace `npp_buffer`):
```cpp
int32_t encoding_get_from_index(int32_t index);
int32_t encoding_get_index_from(int32_t encoding);
int32_t encoding_get_from_string(rust::Str alias);
uint32_t detect_bom(rust::Slice<const uint8_t> buf);
```

**extern "C"** (`src/ffi/raw.rs`):

Used for the `BufferState` / `FileManager` interfaces because:
- They involve Win32 `HANDLE` / `FILETIME` / `HWND` types that cxx cannot represent.
- The `FileManager` is managed as an opaque heap-allocated handle (`*mut FileManager`) because the C++ singleton pattern maps naturally to a pointer-based C API.
- Wide-character (`*const u16`) file paths from Win32 cannot be expressed as cxx `&str`.

Every `extern "C"` function has a `# Safety` section explaining what invariant the C++ caller must uphold.

### 4. Unsafe code isolation

All `unsafe` blocks are in `src/ffi/raw.rs`. The three patterns used are:

1. **Null-terminated C string to `&str`**: Walk to the NUL byte, construct a `&[u8]` slice, validate with `core::str::from_utf8`, then pass to the pure-Rust implementation.
2. **Opaque handle dereference** (`*mut FileManager`): After a null check, dereference the pointer. The caller must have obtained the pointer from `npp_file_manager_new` and must not use it after `npp_file_manager_free`.
3. **Slice from pointer + length** (`detect_encoding_from_bom`): Validated with a null check before constructing the slice.

No `unsafe` appears in `encoding_mapper.rs`, `utf_convert.rs`, `buffer.rs`, or `types.rs`.

### 5. Build system integration

The Rust crate is compiled by a CMake `add_custom_target` that runs `cargo build` and then the resulting `.a` file is added to `TARGET_LINK_LIBRARIES`. The `--features cxx-bridge` flag is intentionally left as a comment in `CMakeLists.txt` because the cxx bridge requires the C++ headers to be set up (done in a later step).

Key CMake additions in `PowerEditor/src/CMakeLists.txt`:
```cmake
add_custom_target(npp_buffer_rust ALL
    COMMAND cargo build ${CARGO_BUILD_TYPE_FLAG} ${CARGO_FLAGS}
    WORKING_DIRECTORY "${RUST_MANIFEST_DIR}"
    …)
add_dependencies(notepad++ npp_buffer_rust)
TARGET_LINK_LIBRARIES(notepad++ … ${NPP_BUFFER_RUST_LIB})
```

On Windows with MSVC, `cargo` will automatically link the MSVC C runtime. No additional link flags are required.

### 6. No nightly Rust

The crate uses only stable Rust 1.56+ features (Rust 2021 edition). Specifically:
- `char::from_u32` (stable since 1.0)
- `str::encode_utf16` (stable since 1.8)
- `core::ffi::c_int` (stable since 1.64)
- `core::ops::BitOr` / `BitOrAssign` / `BitAnd` for `BufferChangeFlags`

The `dep:` syntax in `[features]` requires Cargo 1.60+ (stable).

---

## Consequences

### What becomes easier

- **Encoding logic is now testable without a running Notepad++ instance.** `cargo test` runs 34 unit tests covering BOM detection, UTF-8 classification, UTF-16 ↔ UTF-8 round-trips, encoding alias look-ups, and buffer state transitions.
- **The encoding table has a single source of truth.** Previously the alias string parsing in `EncodingMapper.cpp` used manual pointer arithmetic. The Rust version uses `str::split_ascii_whitespace` and `eq_ignore_ascii_case`, which are easier to reason about.
- **New contributors can read and modify the pure-state logic without understanding Win32.** The `buffer.rs` and `encoding_mapper.rs` files have no platform-specific dependencies.

### What new constraints or risks are introduced

- **Duplicate implementations temporarily coexist.** Until the hybrid build is fully wired (Step 2), both `EncodingMapper.cpp` and `encoding_mapper.rs` exist. It is the build system's job to use only one; the C++ code still calls the C++ version. This duplication must be resolved in Step 2 to avoid divergence.
- **The cxx bridge is not yet active.** The `cxx-bridge` feature requires the C++ headers to be reachable from `build.rs`. This is a one-line change (`cargo build --features cxx-bridge`), but it requires the CMake build to be fully set up on Windows first.
- **Win32-dependent Buffer methods are not yet migrated.** File loading, saving, timestamp polling, and the backup system all still live in C++. Any bugs found in those paths cannot be fixed in Rust until Step 2.
- **The `LangType` enum is a partial copy.** The C++ `LangType` enum has ~90 entries. The Rust version includes the most common ones; the remaining entries fall back to `External`. New contributors adding a language must add it in both places until the full migration is complete (documented in the summary table Notes column).

### Next migration step

**Step 2** should migrate `FileManager`'s file I/O operations:
- `loadFile` / `loadFileData` — the hot path for opening files; depends on Win32 `CreateFileW` / `ReadFile` and the Scintilla `SCI_ADDTEXT` message.
- `saveBuffer` — uses `Utf8_16_Write::openFile` / `writeFile` / `closeFile`.
- `backupCurrentBuffer` — uses Win32 async file operations.

Step 2 will introduce a platform abstraction trait (e.g. `FileSystem`) so that the Rust code can be unit-tested without actual file I/O. The `unsafe` surface will grow in `ffi/raw.rs` as Win32 calls are wrapped, but the core logic will remain safe Rust.

---

## Alternatives Considered

### Full rewrite all at once

Rejected. A complete port of all ~12,000 lines of C++ in one go would produce a codebase that doesn't compile for weeks, making it impossible to review incrementally, catch regressions early, or keep the `main` branch in a buildable state. The incremental approach gives reviewers manageable diffs and gives CI a chance to catch breakage at every step.

### Nightly Rust

Rejected. Nightly Rust has an unstable ABI and occasionally breaks. Using nightly would add a toolchain-management burden for every contributor and could block the Windows MSVC build if a nightly change broke a nightly-only API we depended on. Stable Rust provides everything needed for this step.

### Using only extern "C" (no cxx)

Considered but not preferred for the `EncodingMapper` boundary. Hand-writing `extern "C"` declarations for string-passing functions is error-prone: the C++ side must independently declare the function signature, and a mismatch (e.g., returning `int` vs `int32_t`) is a silent ABI bug. cxx eliminates that risk for the interfaces where it can be used.

### Keeping EncodingMapper and Utf8_16 in C++ and only migrating Buffer state

Considered. The problem is that `EncodingMapper` and `Utf8_16` have the cleanest migration path (no Win32 dependencies). Starting with them builds confidence in the tooling, the build system integration, and the test harness before tackling harder files. Skipping them would mean migrating harder code first without a proven workflow.
