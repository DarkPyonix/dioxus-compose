//! Point this binary at the renderer the library resolved.
//!
//! Cargo does not pass a dependency's link arguments on to the binary that uses it, so the
//! rpath `dioxus-compose` emits for itself does not reach here. Without repeating it the
//! program links and then dies at start up with "Library not loaded".
//!
//! The resolution rules are the library's, included rather than copied, so a sample can
//! never look somewhere the crate does not.

use std::path::{Path, PathBuf};

#[allow(dead_code)]
mod renderer_dir {
    include!("../../dioxus-compose/build/renderer_dir.rs");
}

fn main() {
    println!(
        "cargo:rerun-if-env-changed={}",
        renderer_dir::RENDERER_DIR_ENV
    );
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if !matches!(target_os.as_str(), "macos" | "windows" | "linux") {
        return;
    }

    let Some(directory) = renderer_lib_dir(&target_os) else {
        // The library's own build script is where the explanation of how to get a renderer
        // lives. Staying quiet here leaves that one message to do the explaining.
        return;
    };

    if target_os == "windows" {
        // Windows has no rpath. The loader searches the executable's own directory and
        // PATH, so the renderer's files have to sit beside the program or be on PATH.
        return;
    }

    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", directory.display());
    // The renderer resolves the Host's exported entry points out of this executable. GNU
    // ld spells this `--export-dynamic`. Passing the macOS spelling to it is not a
    // harmless no-op: `-export_dynamic` parses as `-e xport_dynamic`, which sets the entry
    // point to a symbol that does not exist, so the link succeeds with a warning and the
    // program jumps into the middle of its own text and dies on its first instruction.
    let export_dynamic = if target_os == "macos" {
        "-Wl,-export_dynamic"
    } else {
        "-Wl,--export-dynamic"
    };
    println!("cargo:rustc-link-arg={export_dynamic}");
}

/// The same three places the library looks, in the same order.
fn renderer_lib_dir(target_os: &str) -> Option<PathBuf> {
    let library = renderer_dir::renderer_lib_file(target_os);

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(configured) = std::env::var_os(renderer_dir::RENDERER_DIR_ENV) {
        roots.push(PathBuf::from(configured));
    }
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR")?);
    roots.push(manifest.join("../../dioxus-compose-renderer/build/native-image/dist"));
    roots.push(manifest.join("../../dioxus-compose-renderer/build/native-image-linux/dist"));

    let cache_root = renderer_dir::default_cache_root(
        std::env::var_os("HOME").map(PathBuf::from).as_deref(),
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .as_deref(),
        cfg!(windows),
    );
    if let Some(cache_root) = cache_root {
        let version = std::env::var("CARGO_PKG_VERSION").ok()?;
        let arch = std::env::var("CARGO_CFG_TARGET_ARCH").ok()?;
        let target = renderer_dir::artifact_target(target_os, &arch);
        roots.push(renderer_dir::cached_renderer_dir(
            &cache_root,
            &version,
            &target,
        ));
    }

    roots
        .into_iter()
        .find_map(|root| first_dir_holding(&root, library))
}

/// The renderer sits in `lib/` on the platforms that use an rpath and in `bin/` on the one
/// that does not, and straight in the root when someone points at a staged directory.
fn first_dir_holding(root: &Path, library: &str) -> Option<PathBuf> {
    [root.to_path_buf(), root.join("lib"), root.join("bin")]
        .into_iter()
        .find(|dir| dir.join(library).exists())
}
