use std::path::PathBuf;

// The resolution rules and every error message live here, free of Cargo directives so
// tests/renderer_resolution.rs can exercise them directly.
#[allow(dead_code)]
mod renderer_dir {
    include!("build/renderer_dir.rs");
}

use renderer_dir::{RENDERER_DIR_ENV, artifact_target, resolve_renderer};

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("Cargo sets CARGO_CFG_TARGET_OS");
    if !matches!(target_os.as_str(), "macos" | "windows" | "linux") {
        return;
    }

    // Mach-O can leave the Renderer's symbols unresolved and bind them from the shared
    // library at load time. PE/COFF cannot, which is why the Windows renderer defines
    // forwarding stubs instead, and ELF resolves them from the library we link below.
    if target_os == "macos" {
        println!("cargo:rustc-link-arg-cdylib=-Wl,-undefined,dynamic_lookup");
    }

    println!("cargo:rerun-if-env-changed={RENDERER_DIR_ENV}");

    if std::env::var_os("CARGO_FEATURE_NATIVE_RENDERER").is_none() {
        return;
    }

    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"),
    );
    // Only present in a checkout of this repository. A consumer of the published crate
    // has no workspace, which is exactly why resolution has to fail with instructions
    // rather than pass a nonexistent path to the linker.
    let workspace_lib_dir =
        manifest_dir.join("../dioxus-compose-renderer/build/native-image/dist/lib");
    let crate_version = std::env::var("CARGO_PKG_VERSION").expect("Cargo sets CARGO_PKG_VERSION");
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

    if target_os == "windows" {
        // MSVC links against the import library the native image produced beside the DLL,
        // and it carries the same `lib` prefix the image was named with. There is no
        // rpath: Windows finds the DLL through the loader's search path, so the renderer
        // directory has to be on PATH or its files beside the executable at run time.
        println!("cargo:rustc-link-lib=dylib=libdioxus_compose_renderer");
        return;
    }

    println!("cargo:rustc-link-lib=dylib=dioxus_compose_renderer");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    // The Renderer resolves the Host's dioxus_compose_host_* symbols from this executable.
    println!("cargo:rustc-link-arg=-Wl,-export_dynamic");
}
