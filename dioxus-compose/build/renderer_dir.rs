// Resolving the Renderer distribution for the `native-renderer` feature (SPEC NFR-10,
// NFR-11).
//
// This file is `include!`d by `build.rs` and by `tests/renderer_resolution.rs`, so the
// rules a consumer hits at build time are the rules the tests exercise. It touches the
// filesystem and nothing else: no network, no Cargo directives, no process exit. The
// caller decides what to do with an error.

use std::path::{Path, PathBuf};

/// Points at a directory laid out like the release artifact, or at its `lib`
/// subdirectory. Takes precedence over the workspace build output (SPEC NFR-10).
pub const RENDERER_DIR_ENV: &str = "DIOXUS_COMPOSE_RENDERER_DIR";

/// The shared library every distribution of the Renderer contains, whatever else
/// travels alongside it. The name follows the platform's own convention, which is what
/// its loader will look for.
pub fn renderer_lib_file(target_os: &str) -> &'static str {
    match target_os {
        "windows" => "dioxus_compose_renderer.dll",
        "macos" => "libdioxus_compose_renderer.dylib",
        _ => "libdioxus_compose_renderer.so",
    }
}

/// One line holding the crate version the artifact was built for. The release packaging
/// script writes it into the artifact root; a renderer built straight from the workspace
/// has no such file (SPEC NFR-11).
pub const RENDERER_VERSION_FILE: &str = "dioxus-compose-renderer.version";

/// The release artifact for a crate version and platform target, as
/// `scripts/package-renderer.sh` and `.github/workflows/release.yml` name it.
pub fn artifact_file_name(crate_version: &str, target: &str) -> String {
    format!("dioxus-compose-renderer-v{crate_version}-{target}.tar.gz")
}

/// The platform target used in artifact names, built from Cargo's target triple parts.
pub fn artifact_target(target_os: &str, target_arch: &str) -> String {
    format!("{target_os}-{target_arch}")
}

/// Where the Renderer was found, and which version file (if any) vouched for it.
pub struct Renderer {
    /// The directory to add to the link search path and the rpath.
    pub lib_dir: PathBuf,
    /// The version recorded in the artifact, when it carries one.
    pub artifact_version: Option<String>,
}

/// Resolve the Renderer distribution, or explain precisely how to get one.
///
/// `env_dir` is the value of [`RENDERER_DIR_ENV`]; `workspace_lib_dir` is the
/// in-repository fallback, which does not exist for a consumer building a published
/// crate. That case is the whole point of the error paths here: without this check the
/// build would hand a nonexistent directory to the linker and the consumer's first signal
/// would be an undefined-symbol dump (SPEC NFR-11).
pub fn resolve_renderer(
    env_dir: Option<&Path>,
    workspace_lib_dir: &Path,
    crate_version: &str,
    target: &str,
    target_os: &str,
) -> Result<Renderer, String> {
    let lib_file = renderer_lib_file(target_os);
    let lib_dir = match env_dir {
        Some(dir) => lib_dir_within(dir, lib_file),
        None => workspace_lib_dir.to_path_buf(),
    };

    if !lib_dir.join(lib_file).is_file() {
        return Err(match env_dir {
            Some(dir) => missing_from_env_message(dir, &lib_dir, crate_version, target, lib_file),
            None => missing_entirely_message(&lib_dir, crate_version, target, lib_file),
        });
    }

    let artifact_version = read_artifact_version(&lib_dir);
    if let Some(found) = artifact_version.as_deref() {
        if found != crate_version {
            return Err(version_mismatch_message(
                &lib_dir,
                found,
                crate_version,
                target,
            ));
        }
    }

    Ok(Renderer {
        lib_dir,
        artifact_version,
    })
}

/// Accept either the unpacked artifact root or its `lib` directory. Both are natural
/// things for a human to point the variable at, and guessing wrong is a linker error.
fn lib_dir_within(dir: &Path, lib_file: &str) -> PathBuf {
    let nested = dir.join("lib");
    if nested.join(lib_file).is_file() {
        return nested;
    }
    dir.to_path_buf()
}

/// The version file lives in the artifact root, which is the parent of `lib/` for an
/// unpacked artifact. It is also accepted inside the library directory, so a vendored
/// copy that keeps only `lib/` can still declare its version.
fn read_artifact_version(lib_dir: &Path) -> Option<String> {
    let candidates = [
        lib_dir.join(RENDERER_VERSION_FILE),
        lib_dir.join("..").join(RENDERER_VERSION_FILE),
    ];
    for candidate in candidates {
        if let Ok(contents) = std::fs::read_to_string(&candidate) {
            let trimmed = contents.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn how_to_get_one(crate_version: &str, target: &str) -> String {
    let artifact = artifact_file_name(crate_version, target);
    format!(
        "\n\
         The renderer ships as a checksummed release artifact, not inside this crate: it is\n\
         about 85MB of shared library and Skia (SPEC NFR-11).\n\
         \n\
         Get it with one command, from a checkout of the repository:\n\
         \n\
         \x20   scripts/fetch-renderer.sh --version {crate_version}\n\
         \n\
         Or by hand:\n\
         \n\
         \x20   1. download {artifact} and {artifact}.sha256 from\n\
         \x20      https://github.com/DarkPyonix/dioxus-compose/releases/tag/v{crate_version}\n\
         \x20   2. shasum -a 256 -c {artifact}.sha256\n\
         \x20   3. mkdir -p <dir> && tar -xzf {artifact} -C <dir>\n\
         \x20   4. export {RENDERER_DIR_ENV}=<dir>\n\
         \n\
         Building the renderer from source instead: see\n\
         dioxus-compose-renderer/desktop/scripts/build-native.sh and point\n\
         {RENDERER_DIR_ENV} at its dist directory.\n\
         \n\
         The `native-renderer` feature is what requires all this. The crate's default\n\
         features need no renderer at all."
    )
}

fn missing_entirely_message(
    looked_in: &Path,
    crate_version: &str,
    target: &str,
    lib_file: &str,
) -> String {
    format!(
        "dioxus-compose: the `native-renderer` feature is enabled but no renderer was found.\n\
         \n\
         {RENDERER_DIR_ENV} is not set, and the workspace build output is not there either\n\
         (looked for {lib} in {looked_in}). A published crate has no workspace to fall back\n\
         on, so this variable is how you point the build at a renderer.{how}",
        lib = lib_file,
        looked_in = looked_in.display(),
        how = how_to_get_one(crate_version, target),
    )
}

fn missing_from_env_message(
    env_dir: &Path,
    looked_in: &Path,
    crate_version: &str,
    target: &str,
    lib_file: &str,
) -> String {
    format!(
        "dioxus-compose: {RENDERER_DIR_ENV} is set to {env_dir}, but no renderer is there.\n\
         \n\
         Looked for {lib} in {looked_in} and in {env_dir}/lib. Point the variable either at\n\
         the directory an artifact was unpacked into, or directly at its `lib` directory.{how}",
        lib = lib_file,
        env_dir = env_dir.display(),
        looked_in = looked_in.display(),
        how = how_to_get_one(crate_version, target),
    )
}

fn version_mismatch_message(
    lib_dir: &Path,
    artifact_version: &str,
    crate_version: &str,
    target: &str,
) -> String {
    format!(
        "dioxus-compose: renderer version mismatch.\n\
         \n\
         The renderer in {lib_dir} declares version {artifact_version} ({file}), but this\n\
         crate is version {crate_version}. Linking them would pair a Host against a Renderer\n\
         it was never built for, and the mismatch would surface as a protocol error at\n\
         runtime instead of here (SPEC NFR-11).\n\
         \n\
         Use the artifact for this crate version, {artifact}, or depend on\n\
         dioxus-compose {artifact_version} instead.",
        lib_dir = lib_dir.display(),
        file = RENDERER_VERSION_FILE,
        artifact = artifact_file_name(crate_version, target),
    )
}
