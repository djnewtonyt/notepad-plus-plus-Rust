# Core Buffer Migration — Summary Table

Migration step 1: core editor buffer subsystem.  
Rust crate: `rust/npp_buffer/`  
ADR: `docs/adr/001-core-buffer-migration.md`

---

## Migrated Files

| C++ File | C++ Responsibility | Rust Equivalent File | ABI Strategy | Unsafe Code Present | Status | Notes |
|---|---|---|---|---|---|---|
| `PowerEditor/src/EncodingMapper.cpp` + `.h` | Bidirectional mapping between Notepad++ menu indices and Windows code-page integers; IANA/MIME alias look-up used when parsing charset declarations | `rust/npp_buffer/src/encoding_mapper.rs` | `cxx bridge` (feature-gated) + `extern C FFI` (raw.rs `npp_encoding_*` functions) | No | Migrated | `isInListA` replaced by `str::split_ascii_whitespace` + `eq_ignore_ascii_case`. The C++ version uses `_stricmp` (Windows-only); Rust uses ASCII case-folding, which is equivalent for all the codec names in the table. |
| `PowerEditor/src/Utf8_16.cpp` + `.h` — detection + conversion path | BOM detection (`determineEncodingFromBOM`); UTF-16 → UTF-8 conversion for file loading; UTF-8 → UTF-16 conversion for file saving | `rust/npp_buffer/src/utf_convert.rs` | `cxx bridge` (`detect_bom`) + `extern C FFI` (`npp_detect_bom_encoding`) | No | Migrated | `Utf8_16_Write::openFile` / `writeFile` / `closeFile` are **not** migrated (Win32 I/O; see below). Surrogate-pair handling (`Utf16_Iter`) is fully implemented. |
| `PowerEditor/src/Utf8_16.cpp` + `.h` — Win32 file-write path | `Utf8_16_Write` opens, writes, and closes files on disk using `CreateFileW` / `WriteFile` | *(deferred to Step 2)* | N/A | N/A | Pending | Requires a Win32 I/O abstraction trait before it can be migrated safely. The C++ code continues to use this path. |
| `PowerEditor/src/ScintillaComponent/Buffer.cpp` + `.h` — pure state | Document-state fields: encoding, EOL format, language, dirty flag, read-only, reference counting, per-view cursor/fold state | `rust/npp_buffer/src/buffer.rs` | `extern C FFI` (raw.rs `npp_buffer_*` functions) | No | Migrated | All `doNotify` calls now return a `BufferChangeFlags` bitmask to the caller instead of calling back into `FileManager` directly. This removes the callback coupling and makes the state logic independently testable. |
| `PowerEditor/src/ScintillaComponent/Buffer.cpp` + `.h` — Win32 state | File timestamp (`FILETIME _timeStamp`), monitoring event (`HANDLE _eventHandle`), `checkFileState`, `updateTimeStamp`, `getFileLength`, `getFileTime` | *(deferred to Step 2)* | N/A | N/A | Pending | All these methods depend on Win32 `GetFileAttributesExW`, `CompareFileTime`, and `CreateEvent`. Migration requires a platform abstraction layer. |
| `PowerEditor/src/ScintillaComponent/Buffer.cpp` — `FileManager::loadFile` + `loadFileData` | Reads file from disk, detects encoding, populates Scintilla document via `SCI_ADDTEXT` | *(deferred to Step 2)* | N/A | N/A | Pending | The Scintilla `Document` handle is an opaque Win32-era pointer. A safe Rust wrapper for Scintilla messaging needs to be designed before this can be migrated. |
| `PowerEditor/src/ScintillaComponent/Buffer.cpp` — `FileManager::saveBuffer` | Writes Scintilla document content to disk, handling encoding conversion via `Utf8_16_Write` | *(deferred to Step 2)* | N/A | N/A | Pending | Depends on `Utf8_16_Write` Win32 path (see above) and Scintilla `SCI_GETTEXT`. |
| `PowerEditor/src/ScintillaComponent/Buffer.cpp` — `FileManager::backupCurrentBuffer` | Periodic auto-save of unsaved documents to a backup directory | *(deferred to Step 2)* | N/A | N/A | Pending | Uses async Win32 thread and `CopyFileW`. Low priority; can wait until the main save path is migrated. |

---

## What Still Needs to Be Rewritten

The following C++ subsystems have **not** been touched in Step 1.  They are listed in suggested priority order for future migration steps.

### Step 2 — Win32 file I/O layer (high priority)

**`PowerEditor/src/MISC/Common/FileInterface.cpp`** — Wraps `CreateFileW` / `ReadFile` / `WriteFile` / `CloseHandle` into a thin C++ class (`Win32_IO_File`).  Migrating this first in Step 2 will unblock the `Utf8_16_Write` and `FileManager::loadFile` migrations.

### Step 3 — Scintilla integration (high priority, complex)

**`PowerEditor/src/ScintillaComponent/ScintillaEditView.cpp` (~ 4,500 lines)** — Wraps every Scintilla message (`SCI_*`) into C++ convenience methods.  This is the largest migration target and should be approached by first writing a safe Rust wrapper for the `SendMessage`-based Scintilla API, then migrating the individual methods one cluster at a time.

### Step 4 — Parameter / settings system (medium priority)

**`PowerEditor/src/Parameters.cpp` (~ 3,000 lines)** — Reads and writes the application configuration from XML files using PugiXML.  The migration can be driven by replacing the XML layer with a Rust XML or TOML library and then wrapping it with `extern "C"` getters/setters.

### Step 5 — Plugin API (medium priority, ABI-critical)

**`PowerEditor/src/MISC/PluginsManager/PluginsManager.cpp`** — Loads third-party DLLs and dispatches `NPPN_*` notifications.  The plugin ABI is public and documented; it **must not change**.  The Rust implementation must expose the same `NPPMSG` message numbers and calling conventions.  Use hand-written `extern "C"` for everything; cxx is not appropriate here because the plugin interface is a raw Win32 `SendMessage` protocol.

### Step 6 — Win32 UI layer (low priority, largest surface area)

All files under `PowerEditor/src/WinControls/` — Window procedures, dialog boxes, toolbar, tab bar, docking framework, etc.  These are the hardest files to migrate because they depend heavily on Win32 message loops and resource IDs.  A practical approach is to use the `windows` crate or `winapi` crate as a Rust-friendly Win32 binding layer.  Recommend leaving these until Steps 2–5 are complete so the non-UI logic is stable before the window-management code is touched.

### Step 7 — Dark mode and DPI (low priority)

**`PowerEditor/src/DarkMode/DarkMode.cpp`** — Hooks Win32 theme APIs to apply dark mode; uses IAT patching (`IatHook.h`).  Migration of IAT hooks requires `unsafe` and is best done after the rest of the Win32 UI layer is ported.
