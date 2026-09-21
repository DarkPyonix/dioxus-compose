//! The rules the build script resolves the renderer by, exercised directly.
//!
//! `build/renderer_dir.rs` is included from `build.rs`, and a build script is never
//! compiled for test, so anything asserted inside it is asserted nowhere. Including the
//! same file here is what gives it coverage.

#[allow(dead_code)]
mod renderer_dir {
    include!("../build/renderer_dir.rs");
}

use renderer_dir::artifact_target;

/// Every one of these has to name a file the release actually publishes, or the crate's
/// instructions send someone to a download link for something that does not exist.
///
/// Cargo and the release artifacts do not spell architectures the same way. Joining the
/// two halves raw was correct on exactly one platform, macOS, which is the platform
/// everything here was checked on.
#[test]
fn nfr4_target_names_match_the_published_artifacts() {
    assert_eq!(artifact_target("macos", "aarch64"), "macos-aarch64");
    assert_eq!(artifact_target("linux", "x86_64"), "linux-x64");
    assert_eq!(artifact_target("linux", "aarch64"), "linux-arm64");
    assert_eq!(artifact_target("windows", "x86_64"), "windows-x64");
}
