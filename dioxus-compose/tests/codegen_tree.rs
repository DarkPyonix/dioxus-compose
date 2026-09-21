//! The codegen binary writes into the checkout it was compiled in, or it writes nothing.
//!
//! More than one checkout of this repository building into one directory is what makes
//! this necessary. Cargo leaves the package path out of the unit hash for a path package,
//! so two checkouts holding identical sources become a single cache entry and the binary
//! that runs is whichever one compiled first. It then generates into that checkout. That
//! has happened here: a run in the main checkout overwrote a file in a worktree somebody
//! was working in.

use std::path::PathBuf;
use std::process::Command;

/// A directory of our own to point the binary at, named after the test that owns it so
/// two tests never collide.
fn empty_directory(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("dioxus-compose-{name}"));
    std::fs::remove_dir_all(&path).ok();
    std::fs::create_dir_all(&path).expect("a temporary directory to point codegen at");
    path
}

#[test]
fn fr7_codegen_refuses_a_checkout_it_was_not_compiled_in() {
    let elsewhere = empty_directory("codegen-elsewhere");
    let output = Command::new(env!("CARGO_BIN_EXE_codegen"))
        .env("CARGO_MANIFEST_DIR", &elsewhere)
        .output()
        .expect("the codegen binary runs");

    assert!(
        !output.status.success(),
        "codegen exited {:?} instead of failing; it was pointed at {}",
        output.status.code(),
        elsewhere.display()
    );

    let message = String::from_utf8_lossy(&output.stderr);
    assert!(
        message.contains(&*elsewhere.to_string_lossy()),
        "the message has to name the checkout it was run in, got: {message}"
    );
    assert!(
        message.contains("compiled in:") && message.contains("running in:"),
        "the message has to name both checkouts, got: {message}"
    );

    let written: Vec<_> = std::fs::read_dir(&elsewhere)
        .expect("the directory we made is still there")
        .map(|entry| entry.expect("a readable directory entry").path())
        .collect();
    assert!(
        written.is_empty(),
        "codegen wrote into a checkout it was not compiled in: {written:?}"
    );

    std::fs::remove_dir_all(&elsewhere).ok();
}

// There is no sibling test for the accepting side here. Making it pass would mean
// compiling a codegen binary whose own CARGO_MANIFEST_DIR is a temporary directory,
// which is a second build of this crate, and running the real binary against the real
// checkout would have it rewrite files that generated_protocol.rs is reading in the same
// cargo test run. The accepting side is a unit test next to the function instead, and
// generated_protocol.rs already fails when the generated files are stale.
