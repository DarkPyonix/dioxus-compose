use std::path::{Path, PathBuf};
use std::process::Command;

// The resolution rules and every error message live here, free of Cargo directives so
// tests/renderer_resolution.rs can exercise them directly.
#[allow(dead_code)]
mod renderer_dir {
    include!("build/renderer_dir.rs");
}

use renderer_dir::{
    CACHE_DIR_ENV, FetchError, RENDERER_DIR_ENV, RendererSource, Request, acquire_renderer,
    artifact_target, default_cache_root,
};

/// docs.rs builds with the network switched off. Linking a renderer is not what building
/// the documentation needs, so that build skips the whole thing and produces a library
/// with no renderer in it, which is one of the cases the Host is loud about at run time.
const DOCS_RS_ENV: &str = "DOCS_RS";

fn main() {
    // Set when a renderer is actually linked into this build, which is not the same thing
    // as the feature being on: docs.rs turns the feature on and links nothing.
    println!("cargo:rustc-check-cfg=cfg(renderer_linked)");
    println!("cargo:rerun-if-env-changed={RENDERER_DIR_ENV}");
    println!("cargo:rerun-if-env-changed={CACHE_DIR_ENV}");
    println!("cargo:rerun-if-env-changed={DOCS_RS_ENV}");
    println!("cargo:rerun-if-changed=build/renderer_dir.rs");
    println!("cargo:rerun-if-changed=build/sha256.rs");
    println!("cargo:rerun-if-changed=build/elf.rs");

    if std::env::var_os("CARGO_FEATURE_NATIVE_RENDERER").is_none() {
        return;
    }

    // `mock-renderer` says this build draws nothing, so there is nothing to link and no
    // reason to spend a download on it. It wins over the default feature rather than
    // forcing anyone who wants the mock to also spell out `default-features = false`.
    if std::env::var_os("CARGO_FEATURE_MOCK_RENDERER").is_some() {
        return;
    }

    // A browser resolves nothing at load time: a wasm import nobody supplies stops the
    // module from being instantiated whether or not anything calls it. So the Host declares
    // no renderer symbols there and the generated web shims install the renderer API from
    // `dioxus_compose_host_web_start` instead, before anything can ask for a frame.
    let target_family =
        std::env::var("CARGO_CFG_TARGET_FAMILY").expect("Cargo sets CARGO_CFG_TARGET_FAMILY");
    if target_family.split(',').any(|family| family == "wasm") {
        return;
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("Cargo sets CARGO_CFG_TARGET_OS");
    if !matches!(target_os.as_str(), "macos" | "windows" | "linux") {
        // iOS links the XCFramework through Xcode and the web build resolves its imports
        // through the Kotlin/Wasm module. Cargo does not link the renderer on either, but
        // the symbols are there by the time anything runs, so the Host must call them
        // rather than take its no-renderer path.
        println!("cargo:rustc-cfg=renderer_linked");
        return;
    }

    // Mach-O can leave the Renderer's symbols unresolved and bind them from the shared
    // library at load time. PE/COFF cannot, which is why the Windows renderer defines
    // forwarding stubs instead, and ELF resolves them from the library we link below.
    if target_os == "macos" {
        println!("cargo:rustc-link-arg-cdylib=-Wl,-undefined,dynamic_lookup");
    }

    if std::env::var_os(DOCS_RS_ENV).is_some() {
        return;
    }

    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"),
    );
    // Only present in a checkout of this repository, and only once its renderer has been
    // built. A consumer of the published crate has neither, which is why everything below
    // it exists.
    let workspace_lib_dir =
        manifest_dir.join("../dioxus-compose-renderer/build/native-image/dist/lib");
    let crate_version = std::env::var("CARGO_PKG_VERSION").expect("Cargo sets CARGO_PKG_VERSION");
    let target = artifact_target(
        &target_os,
        &std::env::var("CARGO_CFG_TARGET_ARCH").expect("Cargo sets CARGO_CFG_TARGET_ARCH"),
    );
    let cache_root = cache_root();

    let renderer = match acquire_renderer(&Request {
        env_dir: std::env::var_os(RENDERER_DIR_ENV)
            .map(PathBuf::from)
            .as_deref(),
        workspace_lib_dir: &workspace_lib_dir,
        cache_root: cache_root.as_deref(),
        crate_version: &crate_version,
        target: &target,
        target_os: &target_os,
        fetch: Some(&fetch),
    }) {
        Ok(renderer) => renderer,
        // A build script panic is reported as the build failure itself, message and all.
        // That is the point: the consumer reads this instead of an undefined-symbol dump.
        Err(message) => panic!("\n\n{message}\n\n"),
    };

    if matches!(
        renderer.source,
        RendererSource::Download | RendererSource::LocalArtifact
    ) {
        // Cargo gives a build script one channel to the terminal and this is it. Fetching
        // tens of megabytes is worth a line; it is written in the past tense because Cargo
        // replays a cached build script's warnings on later builds, and a line claiming to
        // be downloading something would then be a lie.
        println!(
            "cargo:warning=dioxus-compose: unpacked the renderer for v{crate_version} ({target}) into {}",
            renderer.lib_dir.display()
        );
    }

    let lib_dir = renderer.lib_dir;
    println!("cargo:rerun-if-changed={}", lib_dir.display());
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-cfg=renderer_linked");

    if target_os == "windows" {
        // MSVC links against the import library the native image produced beside the DLL,
        // and it carries the same `lib` prefix the image was named with.
        println!("cargo:rustc-link-lib=dylib=libdioxus_compose_renderer");
        // Windows has no rpath and no name inside the file that the loader consults: a
        // DLL is found on the loader's search path and nowhere else. The other platforms
        // are handled by naming the library after where it sits, which does nothing here,
        // so this is the one platform where the application's author has a step to take.
        // Saying it is the whole of the fix until a Windows machine is available to check
        // the alternative on, which is copying tens of megabytes beside every profile's
        // executable.
        println!(
            "cargo:warning=dioxus-compose: on Windows the renderer is found through the \
             loader's search path. Put {} on PATH, or copy its contents next to the \
             executable, or the program will not start.",
            lib_dir.display()
        );
        return;
    }

    println!("cargo:rustc-link-lib=dylib=dioxus_compose_renderer");
    // No rpath. It would not reach an application that merely depends on this crate,
    // because Cargo does not pass a dependency's link arguments on, and emitting one here
    // anyway would leave this repository's own binaries loading the renderer by a route no
    // consumer has. The renderer is named after the absolute path it sits at instead,
    // which every binary that links it records for itself.
    //
    // The Renderer resolves the Host's dioxus_compose_host_* symbols from this executable.
    // GNU ld spells this `--export-dynamic`. Passing the macOS spelling to it is not a
    // harmless no-op: `-export_dynamic` parses as `-e xport_dynamic`, which sets the
    // entry point to a symbol that does not exist, so the link succeeds with a warning
    // and the program jumps into the middle of its own text and dies on the first
    // instruction.
    let export_dynamic = if target_os == "macos" {
        "-Wl,-export_dynamic"
    } else {
        "-Wl,--export-dynamic"
    };
    println!("cargo:rustc-link-arg={export_dynamic}");
}

/// Where downloads are cached. This is a property of the machine running the build, not
/// of the target being built for, so it reads the host's own environment.
fn cache_root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(CACHE_DIR_ENV) {
        return Some(PathBuf::from(dir));
    }
    default_cache_root(
        std::env::var_os("HOME").map(PathBuf::from).as_deref(),
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .as_deref(),
        cfg!(windows),
    )
}

/// One GET, with whatever this machine already has to make it with.
///
/// No HTTP client in `build-dependencies`: that would put a TLS stack and its tree into
/// every consumer's build for two downloads that happen once per version. `curl` ships
/// with macOS, with Windows since 10 1803, and with nearly every Linux image; `wget`
/// covers the images that leave curl out.
fn fetch(url: &str, destination: &Path) -> Result<(), FetchError> {
    match which("curl") {
        true => curl(url, destination),
        false if which("wget") => wget(url, destination),
        false => Err(FetchError::Unreachable(
            "neither curl nor wget is on PATH".to_string(),
        )),
    }
}

fn which(program: &str) -> bool {
    let finder = if cfg!(windows) { "where" } else { "which" };
    Command::new(finder)
        .arg(program)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn curl(url: &str, destination: &Path) -> Result<(), FetchError> {
    // --location because a release download is a redirect to object storage, and
    // --write-out because the status code is the difference between "this platform has no
    // artifact" and "something is wrong", and curl otherwise reports both as exit 22.
    let output = Command::new("curl")
        .args(["--silent", "--show-error", "--location"])
        .args(["--connect-timeout", "30", "--retry", "2"])
        .arg("--output")
        .arg(destination)
        .args(["--write-out", "%{http_code}"])
        .arg(url)
        .output()
        .map_err(|error| FetchError::Unreachable(format!("could not run curl: {error}")))?;

    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let code = output.status.code().unwrap_or(-1);
        // 6 is an unresolved host, 7 a refused connection, 28 a timeout: all of them mean
        // the request never got an answer, which is the case worth telling someone how to
        // work around without a network.
        return Err(if matches!(code, 6 | 7 | 28) {
            FetchError::Unreachable(if detail.is_empty() {
                format!("curl exited {code}")
            } else {
                detail
            })
        } else {
            FetchError::Failed(format!("curl exited {code}: {detail}"))
        });
    }

    match String::from_utf8_lossy(&output.stdout).trim() {
        "200" => Ok(()),
        "404" => Err(FetchError::NotFound),
        code => Err(FetchError::Failed(format!("the server answered {code}"))),
    }
}

fn wget(url: &str, destination: &Path) -> Result<(), FetchError> {
    let output = Command::new("wget")
        .args(["--quiet", "--timeout=30", "--tries=2"])
        .arg("--output-document")
        .arg(destination)
        .arg(url)
        .output()
        .map_err(|error| FetchError::Unreachable(format!("could not run wget: {error}")))?;

    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let code = output.status.code().unwrap_or(-1);
    // wget's exit codes stop at "server error response" and never say which one, so a
    // missing artifact cannot be told from a failing server here. The failure message
    // prints the address for that reason: opening it answers the question in one click.
    Err(match code {
        4 => FetchError::Unreachable(format!("wget could not reach the network: {detail}")),
        _ => FetchError::Failed(format!("wget exited {code}: {detail}")),
    })
}
