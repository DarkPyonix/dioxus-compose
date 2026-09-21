//! The rules the build script acquires the renderer by, exercised directly.
//!
//! `build/renderer_dir.rs` is included from `build.rs`, and a build script is never
//! compiled for test, so anything asserted inside it is asserted nowhere. Including the
//! same file here is what gives it coverage.
//!
//! Nothing here touches the network. The module takes its downloader as a closure for
//! exactly that reason, so every branch of the search order, including the one that
//! downloads, runs from a temporary directory in a few milliseconds.

#![allow(dead_code)]

mod renderer_dir {
    include!("../build/renderer_dir.rs");
}

use renderer_dir::sha256::{Sha256, checksum_from_sha256_file, sha256_file, sha256_hex};
use renderer_dir::{
    CACHE_DIR_ENV, Fetch, FetchError, PUBLISHED_TARGETS, RENDERER_DIR_ENV, Renderer,
    RendererSource, Request, acquire_renderer, artifact_file_name, artifact_target, artifact_url,
    cached_renderer_dir, default_cache_root, download_dir, every_failure_message,
    renderer_lib_file, renderer_lib_subdir, sibling_checksum,
};
use std::path::{Path, PathBuf};

/// The crate version the published release this test suite describes was cut for.
const SAMPLE_VERSION: &str = "0.0.0";
/// The desktop target these tests build their fixtures for. Passed in rather than taken
/// from the host, so the same assertions run identically on every machine, and Linux
/// rather than macOS because a macOS fixture would have to be a real Mach-O library for
/// the naming step to accept it. That step has a test of its own, further down, which
/// only runs where it can build one.
const SAMPLE_TARGET: &str = "linux-x64";
const SAMPLE_TARGET_OS: &str = "linux";

// -------------------------------------------------------------------------------------
// Artifact naming
// -------------------------------------------------------------------------------------

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

/// The four names above are the four the build script will download for, and no more.
/// A target that reaches the download step with a name outside this list would ask for a
/// URL that does not exist.
#[test]
fn nfr11_the_downloadable_targets_are_the_ones_the_release_builds() {
    let built_by_the_release_workflow: Vec<String> = [
        ("macos", "aarch64"),
        ("windows", "x86_64"),
        ("linux", "x86_64"),
        ("linux", "aarch64"),
    ]
    .into_iter()
    .map(|(os, arch)| artifact_target(os, arch))
    .collect();
    assert_eq!(PUBLISHED_TARGETS, built_by_the_release_workflow);
}

/// The address the build script fetches from, spelled out once so that a change to it
/// has to be a deliberate edit to a test rather than a silent 404 on someone's machine.
#[test]
fn nfr11_the_download_address_is_the_release_for_this_crate_version() {
    let artifact = artifact_file_name("0.0.0", "macos-aarch64");
    assert_eq!(
        artifact,
        "dioxus-compose-renderer-v0.0.0-macos-aarch64.tar.gz"
    );
    assert_eq!(
        artifact_url("0.0.0", &artifact),
        "https://github.com/DarkPyonix/dioxus-compose/releases/download/v0.0.0/\
         dioxus-compose-renderer-v0.0.0-macos-aarch64.tar.gz"
    );
    assert_eq!(
        sibling_checksum(Path::new(&artifact)).to_string_lossy(),
        format!("{artifact}.sha256")
    );
}

// -------------------------------------------------------------------------------------
// The checksum the build script verifies with
// -------------------------------------------------------------------------------------

/// FIPS 180-4's own vectors. The build script refuses to unpack anything whose digest it
/// cannot reproduce, so a hash that is subtly wrong would refuse every artifact the
/// release publishes, and a hash that is wrong in the other direction would accept
/// anything at all.
#[test]
fn nfr11_sha256_reproduces_the_published_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    // A million bytes, which is the vector that catches a broken length field: everything
    // shorter fits in a block count whose top bytes are zero.
    let mut million = Sha256::new();
    for _ in 0..1000 {
        million.update(&[b'a'; 1000]);
    }
    assert_eq!(
        million.finish_hex(),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

/// The `.sha256` files on the release are `shasum -a 256` output: the digest, two spaces,
/// the file name. Only the digest is read, because the name in the file is whatever the
/// release job passed on its command line.
#[test]
fn nfr11_the_checksum_file_is_read_as_shasum_writes_it() {
    let line = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  \
                dioxus-compose-renderer-v0.0.0-macos-aarch64.tar.gz\n";
    assert_eq!(
        checksum_from_sha256_file(line).as_deref(),
        Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
    );
    assert_eq!(checksum_from_sha256_file(""), None);
    assert_eq!(checksum_from_sha256_file("not a digest  file"), None);
    // 63 characters. Close enough to look right in a log and wrong enough to accept a
    // different file.
    assert_eq!(
        checksum_from_sha256_file(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85  file"
        ),
        None
    );
}

// -------------------------------------------------------------------------------------
// The cache
// -------------------------------------------------------------------------------------

/// Outside `target/`, and keyed by both version and target. Inside `target/` it would be
/// re-downloaded after every `cargo clean` and duplicated for every project on the
/// machine; keyed by less than both, two crate versions or two architectures would land
/// on top of each other.
#[test]
fn nfr11_the_cache_lives_outside_target_and_is_keyed_by_version_and_target() {
    // Both names are a published contract: people set them in CI files and shell profiles,
    // so renaming one is a breaking change rather than a refactor.
    assert_eq!(RENDERER_DIR_ENV, "DIOXUS_COMPOSE_RENDERER_DIR");
    assert_eq!(CACHE_DIR_ENV, "DIOXUS_COMPOSE_CACHE_DIR");

    let unix = default_cache_root(Some(Path::new("/home/dev")), None, false).unwrap();
    assert_eq!(unix, Path::new("/home/dev/.cache/dioxus-compose"));

    let windows = default_cache_root(None, Some(Path::new("C:/Users/dev/AppData/Local")), true);
    assert_eq!(
        windows.unwrap(),
        Path::new("C:/Users/dev/AppData/Local/dioxus-compose")
    );
    // Nowhere to put it is a case with its own message, not a panic and not `target/`.
    assert_eq!(default_cache_root(None, None, false), None);

    let root = Path::new("/cache");
    assert_eq!(
        cached_renderer_dir(root, "0.1.0", "linux-x64"),
        Path::new("/cache/renderer/v0.1.0/linux-x64")
    );
    for (version, target) in [("0.1.0", "linux-arm64"), ("0.2.0", "linux-x64")] {
        assert_ne!(
            cached_renderer_dir(root, version, target),
            cached_renderer_dir(root, "0.1.0", "linux-x64"),
            "{version} {target} must not share a directory with 0.1.0 linux-x64"
        );
    }
    assert!(
        !cached_renderer_dir(root, "0.1.0", "linux-x64")
            .to_string_lossy()
            .contains("target"),
        "the cache must survive cargo clean"
    );
}

// -------------------------------------------------------------------------------------
// The search order
// -------------------------------------------------------------------------------------

/// The variable wins over everything, including a cache that is already populated and a
/// download that would otherwise happen. A build that was deliberately pointed at a
/// renderer, to test a local build of it or because the machine is air-gapped, must not
/// quietly acquire a different one.
#[test]
fn nfr10_the_renderer_dir_variable_beats_the_cache_and_the_download() {
    let temp = TempDir::new("env-wins");
    let pointed_at = renderer_tree(&temp.path().join("somewhere-else"));
    // A cache entry that would have answered, had the variable not been set.
    renderer_tree(&cached_renderer_dir(
        &temp.path().join("cache"),
        SAMPLE_VERSION,
        SAMPLE_TARGET,
    ));

    let renderer =
        acquire(&temp, Some(&pointed_at), Network::Forbidden).expect("the variable is set");
    assert_eq!(renderer.source, RendererSource::Environment);
    assert_eq!(
        renderer.lib_dir,
        pointed_at.join(renderer_lib_subdir(SAMPLE_TARGET_OS))
    );
}

/// Set but wrong is a failure, not a fallback. Falling through to the download would mean
/// a typo in the variable silently builds against something other than what it names.
#[test]
fn nfr10_a_renderer_dir_with_nothing_in_it_fails_rather_than_downloading() {
    let temp = TempDir::new("env-empty");
    let empty = temp.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    renderer_tree(&cached_renderer_dir(
        &temp.path().join("cache"),
        SAMPLE_VERSION,
        SAMPLE_TARGET,
    ));

    let message = acquire(&temp, Some(&empty), Network::Forbidden).expect_err("nothing is there");
    assert!(message.contains(RENDERER_DIR_ENV), "{message}");
    assert!(
        message.contains(renderer_lib_file(SAMPLE_TARGET_OS)),
        "the message has to name what it looked for: {message}"
    );
    assert!(
        message.contains("Nothing was downloaded"),
        "the message has to say why the cache and the release were not consulted: {message}"
    );
}

/// An unpacked renderer in the cache is the whole point of the cache: the second project
/// on the machine, and every build after `cargo clean`, costs no network at all.
#[test]
fn nfr11_a_cached_renderer_is_used_with_no_network() {
    let temp = TempDir::new("cache-hit");
    let cached = renderer_tree(&cached_renderer_dir(
        &temp.path().join("cache"),
        SAMPLE_VERSION,
        SAMPLE_TARGET,
    ));

    let renderer = acquire(&temp, None, Network::Forbidden).expect("the cache holds one");
    assert_eq!(renderer.source, RendererSource::Cache);
    assert_eq!(
        renderer.lib_dir,
        cached.join(renderer_lib_subdir(SAMPLE_TARGET_OS))
    );
}

/// The cache is keyed by version, so a renderer cached for another version is not this
/// version's renderer and must not be linked into it.
#[test]
fn nfr11_a_renderer_cached_for_another_version_is_not_a_cache_hit() {
    let temp = TempDir::new("cache-other-version");
    renderer_tree(&cached_renderer_dir(
        &temp.path().join("cache"),
        "0.9.9",
        SAMPLE_TARGET,
    ));

    let message = acquire(&temp, None, Network::Forbidden).expect_err("no entry for this version");
    assert!(
        message.contains(&artifact_file_name(SAMPLE_VERSION, SAMPLE_TARGET)),
        "{message}"
    );
}

/// The artifact already sitting in the download directory is unpacked without a network.
/// This is what makes the offline instructions real rather than an apology: the message
/// says to put two files somewhere, and putting them there works.
#[test]
fn nfr11_an_artifact_already_downloaded_is_verified_and_unpacked_offline() {
    let temp = TempDir::new("local-artifact");
    stage_artifact(&temp, Checksum::Correct);

    let renderer = acquire(&temp, None, Network::Forbidden).expect("the tarball is here");
    assert_eq!(renderer.source, RendererSource::LocalArtifact);
    assert!(
        renderer
            .lib_dir
            .join(renderer_lib_file(SAMPLE_TARGET_OS))
            .is_file(),
        "the renderer has to be unpacked at {}",
        renderer.lib_dir.display()
    );
}

/// The download path, with the network stubbed by a closure that copies a prepared file.
/// Everything after the transfer, which is the verification, the unpacking and the layout
/// check, is the real code.
#[test]
fn nfr11_a_downloaded_artifact_is_verified_and_unpacked() {
    let temp = TempDir::new("download");
    let prepared = build_artifact(&temp.path().join("release"), Checksum::Correct);

    let served = prepared.clone();
    let fetch = move |url: &str, destination: &Path| -> Result<(), FetchError> {
        let source = if url.ends_with(".sha256") {
            sibling_checksum(&served)
        } else {
            served.clone()
        };
        std::fs::copy(source, destination)
            .map(|_| ())
            .map_err(|error| FetchError::Failed(error.to_string()))
    };

    let renderer =
        acquire(&temp, None, Network::Allowed(&fetch)).expect("the release serves this target");
    assert_eq!(renderer.source, RendererSource::Download);
    assert!(
        renderer
            .lib_dir
            .join(renderer_lib_file(SAMPLE_TARGET_OS))
            .is_file(),
        "the renderer has to be unpacked at {}",
        renderer.lib_dir.display()
    );

    // And the next build is a cache hit that needs nothing at all.
    let again = acquire(&temp, None, Network::Forbidden).expect("now cached");
    assert_eq!(again.source, RendererSource::Cache);
    assert_eq!(again.lib_dir, renderer.lib_dir);
}

// -------------------------------------------------------------------------------------
// Refusals
// -------------------------------------------------------------------------------------

/// A digest that does not match means the bytes are not the bytes the release published,
/// and they are not unpacked. Much the likeliest cause is a transfer that was cut short,
/// which would otherwise unpack into a renderer that fails at link time with nothing
/// pointing back at the download.
#[test]
fn nfr11_an_artifact_whose_checksum_does_not_match_is_not_unpacked() {
    let temp = TempDir::new("bad-checksum");
    let tarball = stage_artifact(&temp, Checksum::Wrong);
    let unpacked = cached_renderer_dir(&temp.path().join("cache"), SAMPLE_VERSION, SAMPLE_TARGET);

    let message = acquire(&temp, None, Network::Forbidden).expect_err("the digest is wrong");
    assert!(
        message.contains(&sha256_file(&tarball).unwrap()),
        "the message has to show what the file actually hashes to: {message}"
    );
    assert!(
        message.contains("0000000000000000000000000000000000000000000000000000000000000000"),
        "the message has to show what was expected: {message}"
    );
    assert!(
        !unpacked.exists(),
        "nothing may be unpacked at {}",
        unpacked.display()
    );
}

/// A target the release does not build is told so by name. A bare 404 cannot distinguish
/// "this platform has no renderer" from "this release is missing", and those have
/// completely different answers.
#[test]
fn nfr11_a_target_with_no_published_artifact_says_so_instead_of_downloading() {
    let temp = TempDir::new("unpublished");
    let cache = temp.path().join("cache");
    let message = acquire_for(
        "freebsd-x86_64",
        "freebsd",
        &temp,
        None,
        Network::Forbidden,
        &cache,
    )
    .expect_err("freebsd is not published");

    assert!(message.contains("freebsd-x86_64"), "{message}");
    for published in PUBLISHED_TARGETS {
        assert!(
            message.contains(published),
            "the message has to list what is published, and {published} is missing: {message}"
        );
    }
    assert!(
        !message.contains("Put these two files"),
        "this is not the offline failure and must not read like one: {message}"
    );
}

/// No network means the build names the two files and the directory to put them in. A
/// build that could only say "network error" would leave an air-gapped machine with no
/// route at all.
#[test]
fn nfr11_an_offline_build_names_the_file_and_where_to_put_it() {
    let temp = TempDir::new("offline");
    let unreachable = |_: &str, _: &Path| -> Result<(), FetchError> {
        Err(FetchError::Unreachable(
            "curl could not resolve github.com".to_string(),
        ))
    };
    let message = acquire(&temp, None, Network::Allowed(&unreachable)).expect_err("no network");

    let artifact = artifact_file_name(SAMPLE_VERSION, SAMPLE_TARGET);
    let downloads = download_dir(&temp.path().join("cache"));
    assert!(message.contains(&artifact), "{message}");
    assert!(message.contains(&format!("{artifact}.sha256")), "{message}");
    assert!(
        message.contains(&downloads.display().to_string()),
        "the message has to give the directory to put them in: {message}"
    );
    assert!(
        message.contains("curl could not resolve github.com"),
        "the message has to say what actually failed: {message}"
    );

    // And the route it describes is a route: put the files there and the build works.
    stage_artifact(&temp, Checksum::Correct);
    let renderer = acquire(&temp, None, Network::Forbidden).expect("the instructions work");
    assert_eq!(renderer.source, RendererSource::LocalArtifact);
}

/// The server answering "no such file" is a different failure from not reaching it, and
/// gets a different message. This is the one a 404 would otherwise be.
#[test]
fn nfr11_a_release_that_does_not_carry_the_artifact_is_told_apart_from_being_offline() {
    let temp = TempDir::new("not-in-release");
    let missing = |_: &str, _: &Path| -> Result<(), FetchError> { Err(FetchError::NotFound) };
    let message = acquire(&temp, None, Network::Allowed(&missing)).expect_err("not published");

    assert!(
        message.contains(&artifact_file_name(SAMPLE_VERSION, SAMPLE_TARGET)),
        "{message}"
    );
    assert!(
        message.contains("network problem"),
        "the message has to rule out the failure it is not: {message}"
    );
    assert!(
        message.contains(&format!("releases/tag/v{SAMPLE_VERSION}")),
        "the message has to say where to look: {message}"
    );
}

/// A partially transferred file is never left where the next build would read it as a
/// complete download.
#[test]
fn nfr11_an_interrupted_download_is_not_mistaken_for_a_complete_one() {
    let temp = TempDir::new("interrupted");
    let cut_short = |_: &str, destination: &Path| -> Result<(), FetchError> {
        std::fs::write(destination, b"half a tarball").unwrap();
        Err(FetchError::Failed("the connection was reset".to_string()))
    };
    acquire(&temp, None, Network::Allowed(&cut_short)).expect_err("the transfer failed");

    let downloads = download_dir(&temp.path().join("cache"));
    let left_behind: Vec<PathBuf> = std::fs::read_dir(&downloads)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    assert!(
        left_behind.is_empty(),
        "a failed download must leave nothing behind, found {left_behind:?}"
    );
}

/// An artifact for the wrong platform unpacks perfectly well and contains no renderer.
/// Saying that beats an undefined-symbol dump from the linker.
#[test]
fn nfr11_an_artifact_without_a_renderer_in_it_is_reported_as_such() {
    let temp = TempDir::new("wrong-platform");
    let staging = temp.path().join("staging");
    std::fs::create_dir_all(staging.join("lib")).unwrap();
    std::fs::write(staging.join("lib").join("something-else.so"), b"not it").unwrap();
    let tarball = pack(
        &staging,
        &download_dir(&temp.path().join("cache")),
        Checksum::Correct,
        SAMPLE_TARGET,
    );

    let message = acquire(&temp, None, Network::Forbidden).expect_err("no renderer inside");
    assert!(
        message.contains(renderer_lib_file(SAMPLE_TARGET_OS)),
        "the message has to name what was missing: {message}"
    );
    assert!(
        message.contains(&tarball.display().to_string()),
        "{message}"
    );
}

/// An unpacked macOS renderer is named after the cache entry it lives in, so that an
/// application which only depends on this crate can load it.
///
/// The artifact is built with the name `@rpath/libdioxus_compose_renderer.dylib`, and
/// `@rpath` is resolved against the rpaths of whatever loaded it. Cargo hands a
/// dependency's link search paths and link libraries down to the final binary but not its
/// link arguments, and an rpath is a link argument, so the application has none. It links
/// cleanly and then dies on startup with dyld unable to find a library that is sitting in
/// the cache. This is the step that stops that, and a real Mach-O library is the only
/// thing that can prove it happened.
#[cfg(target_os = "macos")]
#[test]
fn nfr11_the_unpacked_macos_renderer_is_named_after_where_it_will_live() {
    let temp = TempDir::new("install-name");
    let staging = temp.path().join("staging");
    let lib_dir = staging.join(renderer_lib_subdir("macos"));
    std::fs::create_dir_all(&lib_dir).unwrap();

    // A real dylib, carrying the same name the release artifact carries.
    let source = temp.path().join("renderer.c");
    std::fs::write(
        &source,
        "int dioxus_compose_renderer_run(void) { return 0; }\n",
    )
    .unwrap();
    let library = lib_dir.join(renderer_lib_file("macos"));
    let built = std::process::Command::new("cc")
        // The renderer's own image is linked with room to grow its load commands, which
        // is what lets its install name be replaced with a longer one. A fixture linked
        // without that padding is rejected, correctly, so it asks for the same padding.
        .args([
            "-dynamiclib",
            "-Wl,-headerpad_max_install_names",
            "-install_name",
        ])
        .arg(format!("@rpath/{}", renderer_lib_file("macos")))
        .arg("-o")
        .arg(&library)
        .arg(&source)
        .status()
        .expect("cc comes with the tools the Rust linker already needs");
    assert!(built.success(), "building the fixture library failed");

    let cache = temp.path().join("cache");
    pack(
        &staging,
        &download_dir(&cache),
        Checksum::Correct,
        "macos-aarch64",
    );
    let renderer = acquire_for(
        "macos-aarch64",
        "macos",
        &temp,
        None,
        Network::Forbidden,
        &cache,
    )
    .expect("the tarball is here");

    let installed = renderer.lib_dir.join(renderer_lib_file("macos"));
    let name = std::process::Command::new("otool")
        .arg("-D")
        .arg(&installed)
        .output()
        .expect("otool comes with the same tools");
    let name = String::from_utf8_lossy(&name.stdout);
    let recorded = name.lines().nth(1).unwrap_or_default().trim();
    assert_eq!(
        recorded,
        installed.to_string_lossy(),
        "the unpacked library still answers to {recorded}, which no dependent can resolve"
    );
}

// -------------------------------------------------------------------------------------
// Instructions that rot
// -------------------------------------------------------------------------------------

/// No message may tell someone to run or read a file that is not in this repository.
///
/// This is the general form of a specific fault: the failure these messages replace said
/// to run `scripts/fetch-renderer.sh`, which has never existed here. The one automatic
/// route the crate offered was a dead end, and nothing noticed, because no test had ever
/// read the message. Any message added to the build script is walked by this test, so the
/// next dead path fails here instead of on someone's first build.
#[test]
fn nfr11_no_message_names_a_file_that_is_not_in_the_repository() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate sits in the repository");
    let temp = TempDir::new("messages");

    let mut messages = every_failure_message(temp.path());
    // The run-time half of the same promise: what an application with no renderer prints.
    messages.push(dioxus_compose::boundary::no_renderer_message());

    let mut checked = 0;
    for message in &messages {
        for token in message.split_whitespace() {
            let Some(candidate) = repository_path(token) else {
                continue;
            };
            checked += 1;
            assert!(
                repo_root.join(candidate).exists(),
                "a message names {candidate}, which is not in the repository:\n\n{message}"
            );
        }
    }
    assert!(
        checked > 0,
        "the messages named no repository file at all, so this test proved nothing"
    );
}

/// A token out of a message that is meant to be a path in this repository.
///
/// Absolute paths are someone's cache or home directory, `http` is a link, and a
/// placeholder in angle brackets is a placeholder. What is left is a relative path with a
/// source or script extension, which is a claim about this tree.
fn repository_path(token: &str) -> Option<&str> {
    let trimmed = token.trim_matches(|c: char| "`'\"(),.;:".contains(c));
    if trimmed.is_empty()
        || trimmed.starts_with("http")
        || trimmed.starts_with('/')
        || trimmed.starts_with('~')
        || trimmed.starts_with('<')
        || trimmed.starts_with('$')
        || trimmed.starts_with('%')
        || trimmed.contains('\\')
        || trimmed.contains(':')
    {
        return None;
    }
    let extension = Path::new(trimmed).extension()?.to_str()?;
    matches!(
        extension,
        "sh" | "rs" | "toml" | "yml" | "yaml" | "kt" | "kts" | "md" | "ps1"
    )
    .then_some(trimmed)
}

// -------------------------------------------------------------------------------------
// The run-time half: a build with no renderer does not pretend to have worked
// -------------------------------------------------------------------------------------

/// Whatever this test run was built with, the message an application with no renderer
/// prints has to say what is missing and how a build gets one.
#[test]
fn nfr11_the_no_renderer_message_says_what_is_missing_and_how_builds_get_one() {
    let message = dioxus_compose::boundary::no_renderer_message();
    assert!(message.contains("without a renderer"), "{message}");
    assert!(
        message.contains("default-features = false"),
        "the message has to name what turns the renderer off: {message}"
    );
    assert_ne!(
        dioxus_compose::boundary::STATUS_NO_RENDERER,
        dioxus_compose::boundary::STATUS_OK,
        "a build with no renderer must not report success"
    );
}

/// Starting the application with no renderer and no mock does not return success.
///
/// Only a build that really has no renderer can answer this, so it compiles in exactly
/// that configuration. `scripts/check.sh` runs the suite a second time with
/// `--no-default-features`, which is where this actually executes.
#[cfg(not(any(renderer_linked, feature = "mock-renderer")))]
#[test]
fn nfr11_launching_without_a_renderer_does_not_return_success() {
    use dioxus_compose::boundary::{STATUS_NO_RENDERER, STATUS_OK};
    use dioxus_compose::prelude::*;

    fn app() -> Element {
        rsx! { Text { text: "nothing will draw this" } }
    }

    let status = LaunchBuilder::new().try_launch(app);
    assert_ne!(
        status, STATUS_OK,
        "a binary with no renderer drew nothing, so it must not report success"
    );
    assert_eq!(status, STATUS_NO_RENDERER);
}

// -------------------------------------------------------------------------------------
// Fixtures
// -------------------------------------------------------------------------------------

/// Whether the build under test may use the network. Every test here says no, except the
/// one that stubs the transfer with a file copy.
enum Network<'a> {
    Forbidden,
    Allowed(Fetch<'a>),
}

fn acquire(temp: &TempDir, env_dir: Option<&Path>, fetch: Network) -> Result<Renderer, String> {
    let cache = temp.path().join("cache");
    acquire_for(
        SAMPLE_TARGET,
        SAMPLE_TARGET_OS,
        temp,
        env_dir,
        fetch,
        &cache,
    )
}

fn acquire_for(
    target: &str,
    target_os: &str,
    temp: &TempDir,
    env_dir: Option<&Path>,
    fetch: Network,
    cache: &Path,
) -> Result<Renderer, String> {
    // A directory that does not exist, which is what a consumer of the published crate
    // has instead of a workspace.
    let no_workspace = temp.path().join("no-workspace/lib");
    acquire_renderer(&Request {
        env_dir,
        workspace_lib_dir: &no_workspace,
        cache_root: Some(cache),
        crate_version: SAMPLE_VERSION,
        target,
        target_os,
        fetch: match fetch {
            Network::Forbidden => None,
            Network::Allowed(fetch) => Some(fetch),
        },
    })
}

/// An unpacked renderer: the directory layout the artifact has inside it.
fn renderer_tree(root: &Path) -> PathBuf {
    let lib_dir = root.join(renderer_lib_subdir(SAMPLE_TARGET_OS));
    std::fs::create_dir_all(&lib_dir).unwrap();
    std::fs::write(
        lib_dir.join(renderer_lib_file(SAMPLE_TARGET_OS)),
        b"a renderer, for the purposes of a path check",
    )
    .unwrap();
    root.to_path_buf()
}

#[derive(Clone, Copy)]
enum Checksum {
    Correct,
    /// All zeroes: a digest that is the right shape and the wrong value, which is what a
    /// truncated download looks like from here.
    Wrong,
}

/// A tarball in the download directory, where a build with no network looks.
fn stage_artifact(temp: &TempDir, checksum: Checksum) -> PathBuf {
    let staging = temp.path().join("staging");
    renderer_tree(&staging);
    pack(
        &staging,
        &download_dir(&temp.path().join("cache")),
        checksum,
        SAMPLE_TARGET,
    )
}

/// The same tarball somewhere the build script will not find on its own, for the test
/// that hands it over through a stubbed download.
fn build_artifact(dir: &Path, checksum: Checksum) -> PathBuf {
    let staging = dir.join("staging");
    renderer_tree(&staging);
    pack(&staging, dir, checksum, SAMPLE_TARGET)
}

fn pack(staging: &Path, into: &Path, checksum: Checksum, target: &str) -> PathBuf {
    std::fs::create_dir_all(into).unwrap();
    let tarball = into.join(artifact_file_name(SAMPLE_VERSION, target));
    let status = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&tarball)
        .arg("-C")
        .arg(staging)
        .arg(".")
        .status()
        .expect("tar is on PATH: the build script needs it too");
    assert!(status.success(), "packing the fixture failed: {status}");

    let digest = match checksum {
        Checksum::Correct => sha256_file(&tarball).unwrap(),
        Checksum::Wrong => "0".repeat(64),
    };
    std::fs::write(
        sibling_checksum(&tarball),
        format!("{digest}  {}\n", artifact_file_name(SAMPLE_VERSION, target)),
    )
    .unwrap();
    tarball
}

/// A throwaway directory. Small enough not to be worth a dependency, and a dependency in
/// this crate's dev tree would be one more thing a consumer's `cargo test` has to build.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "dioxus-compose-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
