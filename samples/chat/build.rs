//! Point the sample binary at the renderer shared library.
//!
//! The `dioxus-compose` build script emits the link search path and the rpath for its own
//! crate. Link arguments do not travel to a dependent package, so a sample that lives in a
//! separate package links fine and then fails at startup with "Library not loaded". The
//! sample therefore repeats the rpath for itself.
//!
//! Nothing is emitted when no renderer is present, so a plain `cargo check` of the
//! workspace stays green without one.

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=DIOXUS_COMPOSE_RENDERER_DIR");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"),
    );
    let workspace_root = manifest_dir.join("../..");
    let mut candidates = Vec::new();
    if let Some(configured) = std::env::var_os("DIOXUS_COMPOSE_RENDERER_DIR").map(PathBuf::from) {
        // A relative value is written against the workspace root, which is where the
        // documented command is run from, not against this package directory.
        if configured.is_relative() {
            candidates.push(workspace_root.join(&configured));
        }
        candidates.push(configured);
    }
    candidates.push(workspace_root.join("dioxus-compose-renderer/build/native-image/dist/lib"));

    for candidate in candidates {
        for directory in [candidate.clone(), candidate.join("lib")] {
            if directory.join("libdioxus_compose_renderer.dylib").exists() {
                emit(&directory);
                return;
            }
        }
    }
}

fn emit(directory: &Path) {
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", directory.display());
    // The renderer resolves the Host's exported entry points out of this binary.
    println!("cargo:rustc-link-arg=-Wl,-export_dynamic");
}
