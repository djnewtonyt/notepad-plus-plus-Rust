// build.rs – compile the C++ side of the cxx bridge shim.
//
// This script is only active when the `cxx-bridge` feature is enabled.
// Without that feature the crate compiles as pure Rust on any host, which
// makes Linux CI straightforward.
//
// When building the full hybrid Windows application:
//
//   cargo build --features cxx-bridge
//
// cxx-build will compile `src/ffi/bridge.rs` (Rust side) together with the
// generated C++ shim and produce a static library that CMake can link into
// the Notepad++ executable.

fn main() {
    // Nothing to do in the default (no cxx-bridge) configuration.
    // The `#[cfg(feature = "cxx-bridge")]` blocks in the source files are
    // sufficient to gate the generated code.

    #[cfg(feature = "cxx-bridge")]
    {
        // Inform Cargo that we depend on the bridge source file so that the
        // build script is re-run whenever the bridge changes.
        println!("cargo:rerun-if-changed=src/ffi/bridge.rs");

        cxx_build::bridge("src/ffi/bridge.rs")
            // Notepad++ targets Windows with MSVC, which defaults to C++17.
            .std("c++17")
            // Point cxx-build at the Notepad++ and Scintilla headers so the
            // generated shim can see the C++ types it needs.
            .include("../../PowerEditor/src")
            .include("../../PowerEditor/src/ScintillaComponent")
            .include("../../scintilla/include")
            .include("../../lexilla/include")
            .compile("npp_buffer_cxx");
    }
}
