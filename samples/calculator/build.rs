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
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    // The renderer resolves dioxus_compose_host_* out of this executable.
    println!("cargo:rustc-link-arg=-Wl,-export_dynamic");

    let Some(configured) = std::env::var_os(RENDERER_DIR_ENV).map(PathBuf::from) else {
        return;
    };
    let library = "libdioxus_compose_renderer.dylib";
    let directory = if configured.join(library).exists() {
        configured
    } else if configured.join("lib").join(library).exists() {
        configured.join("lib")
    } else {
        // Nothing to point at. The dioxus-compose build script is the place that explains
        // how to get the renderer, so stay quiet and let it do that.
        return;
    };
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", directory.display());
}
