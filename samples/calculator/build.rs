//! Link arguments the sample binary needs when it is built against the native renderer.
//!
//! `dioxus-compose`'s own build script emits these, but link arguments do not propagate
//! from a dependency to the binary that depends on it, so every application that links the
//! renderer has to repeat them. The renderer is loaded by the dynamic linker at start up
//! and then looks the Host's exported functions back up in this executable, which is why
//! both the search path and the dynamic export are needed here.

use std::path::PathBuf;

const RENDERER_DIR_ENV: &str = "DIOXUS_COMPOSE_RENDERER_DIR";

fn main() {
    println!("cargo:rerun-if-env-changed={RENDERER_DIR_ENV}");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if !matches!(target_os.as_str(), "macos" | "windows" | "linux") {
        return;
    }
    if target_os != "windows" {
        // The renderer resolves dioxus_compose_host_* out of this executable. Windows
        // cannot do that, so its renderer forwards through stubs instead, and there is no
        // flag to pass here.
        // GNU ld spells this `--export-dynamic`. Passing the macOS spelling to it is not a
        // harmless no-op: `-export_dynamic` parses as `-e xport_dynamic`, which sets the
        // entry point to a symbol that does not exist, so the link succeeds with a warning
        // and the program jumps into the middle of its own text and dies on the first
        // instruction.
        let export_dynamic = if cfg_target_os() == "macos" {
            "-Wl,-export_dynamic"
        } else {
            "-Wl,--export-dynamic"
        };
        println!("cargo:rustc-link-arg={export_dynamic}");
    }

    let Some(configured) = std::env::var_os(RENDERER_DIR_ENV).map(PathBuf::from) else {
        return;
    };
    let library = match target_os.as_str() {
        "windows" => "libdioxus_compose_renderer.dll",
        "macos" => "libdioxus_compose_renderer.dylib",
        _ => "libdioxus_compose_renderer.so",
    };
    let directory = if configured.join(library).exists() {
        configured
    } else if configured.join("lib").join(library).exists() {
        configured.join("lib")
    // Windows stages the renderer in bin, beside the AWT and Skia DLLs the loader has to
    // find together.
    } else if configured.join("bin").join(library).exists() {
        configured.join("bin")
    } else {
        // Nothing to point at. The dioxus-compose build script is the place that explains
        // how to get the renderer, so stay quiet and let it do that.
        return;
    };
    if target_os == "windows" {
        // Windows has no rpath. The loader searches the executable's own directory and
        // PATH, so the renderer's files have to sit beside the program or be on PATH,
        // which is what the release packaging arranges.
        return;
    }
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", directory.display());
}

/// The target this build is for, which decides which linker spelling is correct.
fn cfg_target_os() -> String {
    std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default()
}
