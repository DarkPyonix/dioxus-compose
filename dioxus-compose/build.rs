use std::path::{Path, PathBuf};
use std::process::Command;

// The resolution rules and every error message live here, free of Cargo directives so
// tests/renderer_resolution.rs can exercise them directly.
#[allow(dead_code)]
mod renderer_dir {
    include!("build/renderer_dir.rs");
}

use renderer_dir::{
    CACHE_DIR_ENV, FetchError, RENDERER_DIR_ENV, RendererLinkage, RendererSource, Request,
    acquire_renderer, artifact_target, default_cache_root, renderer_linkage,
};

/// docs.rs builds with the network switched off. Linking a renderer is not what building
/// the documentation needs, so that build skips the whole thing and produces a library
/// with no renderer in it, which is one of the cases the Host is loud about at run time.
const DOCS_RS_ENV: &str = "DOCS_RS";

/// Where an Android application's Gradle project keeps the Kotlin it compiles.
const ANDROID_KOTLIN_DIR_ENV: &str = "DIOXUS_COMPOSE_ANDROID_KOTLIN_DIR";

/// Where the crate carries that Kotlin, relative to the crate root.
const ANDROID_KOTLIN_DIR: &str = "android-kotlin";

/// The package an Android application's generated Activity belongs to.
const ANDROID_PACKAGE_ENV: &str = "DIOXUS_COMPOSE_ANDROID_PACKAGE";

/// Where dx says to put generated Kotlin, and the package it belongs to.
///
/// The names are wry's: dx sets them for every Android build so that wry can generate the
/// Activity that hosts its webview. We draw with Compose and have no webview in us, but
/// the two values say the same thing either way, which is the whole Gradle project this
/// build is part of. Reading them is what makes an APK come out of `dx build` with
/// nothing written in `Dioxus.toml` about us.
const DX_KOTLIN_DIR_ENV: &str = "WRY_ANDROID_KOTLIN_FILES_OUT_DIR";
const DX_PACKAGE_ENV: &str = "WRY_ANDROID_PACKAGE";
const DX_LIBRARY_ENV: &str = "WRY_ANDROID_LIBRARY";

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

    let target_family =
        std::env::var("CARGO_CFG_TARGET_FAMILY").expect("Cargo sets CARGO_CFG_TARGET_FAMILY");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("Cargo sets CARGO_CFG_TARGET_OS");
    match renderer_linkage(&target_os, &target_family, false) {
        // Nobody resolves a declared symbol on these, because there is none to resolve.
        // The browser and Android both run the renderer in a managed runtime beside the
        // Host, and both install their entry points at load time. Declaring the desktop
        // renderer's symbols instead leaves the loader looking for something that does
        // not exist, which on Android stops the library opening at all.
        RendererLinkage::Installed | RendererLinkage::None => {
            // Android is also where the renderer's Kotlin has to reach the application's
            // own Gradle build, because ART compiles it rather than us. Everywhere else
            // the Kotlin was frozen ahead of time into a library.
            if target_os == "android" {
                unpack_android_kotlin();
            }
            return;
        }
        // iOS links the XCFramework through Xcode. Cargo does not link the renderer, but
        // the symbols are there by the time anything runs, so the Host must call them
        // rather than take its no-renderer path.
        RendererLinkage::Provided => {
            println!("cargo:rustc-cfg=renderer_linked");
            // Same reason as macOS below, and the same Mach-O. Xcode links the renderer
            // into the application, so the symbols are real by the time anything runs,
            // but a shared library built here has to be allowed to leave them open.
            // Without this every iOS build of this crate fails at its own cdylib, which
            // is not even the artifact an iOS application uses.
            if target_os == "ios" {
                println!("cargo:rustc-link-arg-cdylib=-Wl,-undefined,dynamic_lookup");
            }
            return;
        }
        RendererLinkage::Linked => {}
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

/// The Android renderer's Kotlin, copied into the Gradle project that is building this.
///
/// Android is the one platform where the Kotlin cannot travel as a compiled artifact. The
/// desktop ships a native-image shared library and iOS a Kotlin/Native archive; Android
/// runs on ART, so the Kotlin is compiled by the application's own Gradle build and has
/// to be there in source form when it runs. dx's template accepts Maven coordinates and
/// nothing else for dependencies, so handing it a compiled library is not possible even
/// if one existed.
///
/// `DIOXUS_COMPOSE_ANDROID_KOTLIN_DIR` says where the Gradle project's Kotlin source
/// directory is. dx sets it, or a person building by hand does. Without it this does
/// nothing and says so once: a cargo build of the crate on its own is a perfectly
/// ordinary thing to do and is not the moment to fail.
fn unpack_android_kotlin() {
    println!("cargo:rerun-if-env-changed={ANDROID_KOTLIN_DIR_ENV}");
    println!("cargo:rerun-if-env-changed={DX_KOTLIN_DIR_ENV}");
    let Some((destination, package)) = android_gradle_kotlin() else {
        println!(
            "cargo:warning=dioxus-compose: neither {ANDROID_KOTLIN_DIR_ENV} nor \
             {DX_KOTLIN_DIR_ENV} is set, so the renderer's Kotlin was not unpacked. An \
             Android application needs it in its own source directory, because ART \
             compiles it rather than us."
        );
        return;
    };
    let staged = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ANDROID_KOTLIN_DIR);
    if !staged.is_dir() {
        panic!(
            "\n\ndioxus-compose: the Android Kotlin is missing from this copy of the \
             crate.\n\nIt should be at {}. A checkout regenerates it with \
             scripts/stage-android-kotlin.sh; a published crate carries it.\n\n",
            staged.display()
        );
    }
    println!("cargo:rerun-if-changed={}", staged.display());
    if let Err(error) = copy_tree(&staged, &destination) {
        panic!(
            "\n\ndioxus-compose: could not put the renderer's Kotlin into {}: {error}\n\n",
            destination.display()
        );
    }
    write_android_activity(&destination, package);
    add_compose_to_gradle(&destination);
}

/// Puts Compose into the application module's generated build file.
///
/// `destination` is the Gradle source root, so the module's own directory is three levels
/// above it: `<module>/src/main/kotlin`. The CLI regenerates both files on every build, so
/// this runs every time rather than once, and it runs before Gradle does because the
/// project is written before this crate is compiled.
///
/// A project that does not look like the one the CLI generates is left alone. Someone
/// building by hand has their own build file and it is not this crate's to rewrite.
fn add_compose_to_gradle(destination: &Path) {
    let Some(module_dir) = destination
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
    else {
        return;
    };
    let module_path = module_dir.join("build.gradle.kts");
    let Some(root_dir) = module_dir.parent() else {
        return;
    };
    let root_path = root_dir.join("build.gradle.kts");
    // The application's tooling regenerates both files on every build, so the build has
    // to run again whenever they change or a rebuild would leave them as the template
    // wrote them and nothing would compile against Compose.
    println!("cargo:rerun-if-changed={}", module_path.display());
    println!("cargo:rerun-if-changed={}", root_path.display());
    let (Ok(module), Ok(root)) = (
        std::fs::read_to_string(&module_path),
        std::fs::read_to_string(&root_path),
    ) else {
        return;
    };
    let Some((module_text, root_text)) = renderer_dir::with_compose(&module, &root) else {
        return;
    };
    for (path, text) in [(module_path, Some(module_text)), (root_path, root_text)] {
        let Some(text) = text else { continue };
        if let Err(error) = std::fs::write(&path, text) {
            panic!(
                "\n\ndioxus-compose: could not put Compose into {}: {error}\n\n",
                path.display()
            );
        }
    }
}

fn android_gradle_kotlin() -> Option<(PathBuf, Option<String>)> {
    println!("cargo:rerun-if-env-changed={ANDROID_PACKAGE_ENV}");
    println!("cargo:rerun-if-env-changed={DX_PACKAGE_ENV}");
    let read = |name: &str| std::env::var(name).ok();
    renderer_dir::android_gradle_kotlin(
        read(ANDROID_KOTLIN_DIR_ENV).as_deref(),
        read(ANDROID_PACKAGE_ENV).as_deref(),
        read(DX_KOTLIN_DIR_ENV).as_deref(),
        read(DX_PACKAGE_ENV).as_deref(),
    )
}

/// The Activity the application starts at, in the package its own tooling expects.
///
/// Generated rather than shipped. The tooling fixes the package a generated project uses
/// and writes the application id into a build config alias, so an Activity handed over as
/// a static file would sit in the wrong package and never be found. The one the renderer
/// carries is in this crate's own package, which is right for this repository's own
/// application and wrong for everyone else's.
///
/// `DIOXUS_COMPOSE_ANDROID_PACKAGE` names the package. Without it nothing is written,
/// which is what a project supplying its own Activity wants: it has one already, and a
/// second in the same package would not compile.
fn write_android_activity(destination: &Path, package: Option<String>) {
    let Some(package) = package else {
        return;
    };
    // Into the package's own directory, which is where the Kotlin compiler looks for it
    // and where dx put the one it generates. Ours replaces that one: dx writes an Activity
    // that extends wry's, and there is no webview in this application to extend it with.
    let mut path = destination.to_path_buf();
    for component in package.split('.') {
        path = path.join(component);
    }
    if let Err(error) = std::fs::create_dir_all(&path) {
        panic!(
            "\n\ndioxus-compose: could not make {} for the generated Activity: \
             {error}\n\n",
            path.display()
        );
    }
    let path = path.join("MainActivity.kt");
    // The file name of the application's own cdylib, without the `lib` prefix and the
    // extension. It is the application's to choose and its tooling knows it, so the
    // Activity carries the answer rather than the runtime guessing at one.
    println!("cargo:rerun-if-env-changed={DX_LIBRARY_ENV}");
    let library = std::env::var(DX_LIBRARY_ENV).unwrap_or_else(|_| "main".to_owned());
    let source = format!(
        r#"// Generated by dioxus-compose. DO NOT EDIT.
//
// The Android host. Kotlin owns the process and the frame loop: this Activity puts a
// ComposeView on screen and the interpreter draws the Host's tree inside it. Rust runs as
// a cdylib in the same process and never starts a loop of its own.
//
// The Host outlives the Activity, so a configuration change recomposes and nothing else.
package {package}

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.ui.platform.DioxusRuntime

class MainActivity : ComponentActivity() {{
    override fun onCreate(savedInstanceState: Bundle?) {{
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        DioxusRuntime.load("{library}")
        val host = DioxusRuntime.host()
        val view = ComposeView(this)
        view.setContent {{
            DioxusContent(host, Modifier.fillMaxSize().safeDrawingPadding())
        }}
        setContentView(view)
    }}

    override fun onStart() {{
        super.onStart()
        DioxusRuntime.start()
    }}

    override fun onStop() {{
        DioxusRuntime.stop()
        super.onStop()
    }}
}}
"#
    );
    if let Err(error) = std::fs::write(&path, source) {
        panic!(
            "\n\ndioxus-compose: could not write the Activity to {}: {error}\n\n",
            path.display()
        );
    }
}

/// Every file under `from`, into the same shape under `to`.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        // Two files at the top of that tree belong to this repository's own Android
        // application rather than to the renderer. The Activity is generated per
        // application, because the package and the library name are the application's,
        // and the manifest is written by the application's own tooling. Copying either
        // would put a second Activity in someone else's project and declare it twice.
        if matches!(
            entry.file_name().to_str(),
            Some("MainActivity.kt") | Some("AndroidManifest.xml")
        ) {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
