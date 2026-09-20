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
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if !matches!(target_os.as_str(), "macos" | "windows" | "linux") {
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
            if directory.join(renderer_file()).exists() {
                emit(&directory);
                return;
            }
        }
    }
}

/// The renderer's file name on this target.
fn renderer_file() -> &'static str {
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => "libdioxus_compose_renderer.dll",
        Ok("macos") => "libdioxus_compose_renderer.dylib",
        _ => "libdioxus_compose_renderer.so",
    }
}

fn emit(directory: &Path) {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Windows has no rpath. The loader searches the executable's own directory and
        // PATH, so the renderer's files have to sit beside the program or be on PATH,
        // which is what the release packaging arranges.
        return;
    }
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", directory.display());
    // The renderer resolves the Host's exported entry points out of this binary.
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

/// The target this build is for, which decides which linker spelling is correct.
fn cfg_target_os() -> String {
    std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default()
}
