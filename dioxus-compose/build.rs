use std::path::{Path, PathBuf};

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    // The Renderer symbols are supplied by the GraalVM shared library at application load
    // time, not while this cdylib is being built.
    println!("cargo:rustc-link-arg-cdylib=-Wl,-undefined,dynamic_lookup");

    if std::env::var_os("CARGO_FEATURE_NATIVE_RENDERER").is_none() {
        return;
    }

    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"),
    );
    let renderer_lib_dir = renderer_lib_dir(&manifest_dir);

    println!("cargo:rerun-if-env-changed={RENDERER_DIR_ENV}");
    println!("cargo:rerun-if-changed={}", renderer_lib_dir.display());
    println!(
        "cargo:rustc-link-search=native={}",
        renderer_lib_dir.display()
    );
    println!("cargo:rustc-link-lib=dylib=dioxus_compose_renderer");
    println!(
        "cargo:rustc-link-arg=-Wl,-rpath,{}",
        renderer_lib_dir.display()
    );
    // The Renderer resolves the Host's dioxus_compose_host_* symbols from this executable.
    println!("cargo:rustc-link-arg=-Wl,-export_dynamic");
}

const RENDERER_DIR_ENV: &str = "DIOXUS_COMPOSE_RENDERER_DIR";

/// Where to find the Renderer shared library, in order of precedence:
///
/// 1. `DIOXUS_COMPOSE_RENDERER_DIR`, for a downloaded release artifact, a vendored copy, or
///    a distribution staged somewhere else entirely.
/// 2. The renderer built from this workspace by
///    `dioxus-compose-renderer/native/scripts/build-native.sh`.
///
/// Publishing the renderer as a prebuilt artifact is the planned distribution route, and it
/// arrives through (1): the build is ~85 MB of shared library and Skia, far past what a
/// crates.io package can carry, so the crate will fetch it and point this variable at the
/// unpacked directory. Keeping the lookup ordered this way now means that change does not
/// touch anything but the fetching step.
fn renderer_lib_dir(manifest_dir: &Path) -> PathBuf {
    if let Some(dir) = std::env::var_os(RENDERER_DIR_ENV) {
        return PathBuf::from(dir);
    }
    manifest_dir.join("../dioxus-compose-renderer/build/native-image/dist/lib")
}
