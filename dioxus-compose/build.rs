use std::path::PathBuf;

// The resolution rules and every error message live here, free of Cargo directives so
// tests/renderer_resolution.rs can exercise them directly (SPEC NFR-10, NFR-11).
#[allow(dead_code)]
mod renderer_dir {
    include!("build/renderer_dir.rs");
}

use renderer_dir::{RENDERER_DIR_ENV, artifact_target, resolve_renderer};

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    // The Renderer symbols are supplied by the GraalVM shared library at application load
    // time, not while this cdylib is being built.
    println!("cargo:rustc-link-arg-cdylib=-Wl,-undefined,dynamic_lookup");

    println!("cargo:rerun-if-env-changed={RENDERER_DIR_ENV}");

    if std::env::var_os("CARGO_FEATURE_NATIVE_RENDERER").is_none() {
        return;
    }

    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"),
    );
    // Only present in a checkout of this repository. A consumer of the published crate
    // has no workspace, which is exactly why resolution has to fail with instructions
    // rather than pass a nonexistent path to the linker (SPEC NFR-11).
    let workspace_lib_dir =
        manifest_dir.join("../dioxus-compose-renderer/build/native-image/dist/lib");
    let crate_version = std::env::var("CARGO_PKG_VERSION").expect("Cargo sets CARGO_PKG_VERSION");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("checked above");
    let target = artifact_target(
        &target_os,
        &std::env::var("CARGO_CFG_TARGET_ARCH").expect("Cargo sets CARGO_CFG_TARGET_ARCH"),
    );

    let renderer = match resolve_renderer(
        std::env::var_os(RENDERER_DIR_ENV)
            .map(PathBuf::from)
            .as_deref(),
        &workspace_lib_dir,
        &crate_version,
        &target,
        &target_os,
    ) {
        Ok(renderer) => renderer,
        // A build script panic is reported as the build failure itself, message and all.
        // That is the point: the consumer reads this instead of an undefined-symbol dump.
        Err(message) => panic!("\n\n{message}\n\n"),
    };

    let lib_dir = renderer.lib_dir;
    println!("cargo:rerun-if-changed={}", lib_dir.display());
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=dioxus_compose_renderer");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    // The Renderer resolves the Host's dioxus_compose_host_* symbols from this executable.
    println!("cargo:rustc-link-arg=-Wl,-export_dynamic");
}
