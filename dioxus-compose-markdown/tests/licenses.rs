//! Everything this crate makes an application link is under a permissive licence, down to
//! the grammar files bundled inside the highlighter.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Licences that can ship beside Apache-2.0 without asking anything of the application
/// beyond keeping notices.
const PERMISSIVE: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "Apache-2.0+WITH+LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Zlib",
    "0BSD",
    "Unlicense",
    "Unicode-3.0",
    "Unicode-DFS-2016",
    "BSL-1.0",
    "CC0-1.0",
];

/// Crates that a workspace member depends on only to build its own tests and benches.
/// Cargo.lock does not say which dependencies are dev-only, so they are named here.
const DEV_ONLY: &[&str] = &["criterion"];

/// Whether an SPDX expression can be satisfied with permissive licences only.
fn permissive(expression: &str) -> bool {
    let normalized = expression
        .replace('/', " OR ")
        .replace(" WITH ", "+WITH+")
        .replace('(', " ( ")
        .replace(')', " ) ");
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    let mut at = 0;
    let result = or_expression(&tokens, &mut at);
    result && at == tokens.len()
}

fn or_expression(tokens: &[&str], at: &mut usize) -> bool {
    let mut any = and_expression(tokens, at);
    while tokens.get(*at) == Some(&"OR") {
        *at += 1;
        let next = and_expression(tokens, at);
        any = any || next;
    }
    any
}

fn and_expression(tokens: &[&str], at: &mut usize) -> bool {
    let mut all = term(tokens, at);
    while tokens.get(*at) == Some(&"AND") {
        *at += 1;
        let next = term(tokens, at);
        all = all && next;
    }
    all
}

fn term(tokens: &[&str], at: &mut usize) -> bool {
    match tokens.get(*at) {
        Some(&"(") => {
            *at += 1;
            let inner = or_expression(tokens, at);
            if tokens.get(*at) == Some(&")") {
                *at += 1;
            }
            inner
        }
        Some(token) => {
            *at += 1;
            PERMISSIVE.contains(token)
        }
        None => false,
    }
}

fn listed() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("licenses.txt");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            line.split_once(' ')
                .map(|(name, licence)| (name.to_owned(), licence.trim().to_owned()))
        })
        .collect()
}

struct Package {
    dependencies: Vec<String>,
    workspace: bool,
}

/// The packages of the workspace's Cargo.lock, by name. A name with several versions keeps
/// the union of their dependencies, which is all a closure needs.
fn lock() -> BTreeMap<String, Package> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.lock");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let mut packages: BTreeMap<String, Package> = BTreeMap::new();
    for block in text.split("[[package]]").skip(1) {
        let mut name = None;
        let mut workspace = true;
        let mut dependencies = Vec::new();
        let mut in_dependencies = false;
        for line in block.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("name = ") {
                name = Some(value.trim_matches('"').to_owned());
            } else if line.starts_with("source = ") {
                workspace = false;
            } else if line.starts_with("dependencies = [") {
                in_dependencies = true;
            } else if in_dependencies {
                if line.starts_with(']') {
                    in_dependencies = false;
                } else {
                    let entry = line.trim_matches(|character| character == '"' || character == ',');
                    if let Some(dependency) = entry.split(' ').next() {
                        dependencies.push(dependency.to_owned());
                    }
                }
            }
        }
        if let Some(name) = name {
            let package = packages.entry(name).or_insert(Package {
                dependencies: Vec::new(),
                workspace,
            });
            package.dependencies.extend(dependencies);
        }
    }
    packages
}

#[test]
fn fr37_every_linked_crate_is_listed_with_a_permissive_licence() {
    let packages = lock();
    assert!(
        packages.contains_key("dioxus-compose-markdown"),
        "Cargo.lock does not know this crate yet; build once so Cargo records it"
    );
    let mut reached = BTreeSet::new();
    let mut pending = vec!["dioxus-compose-markdown".to_owned()];
    while let Some(name) = pending.pop() {
        if !reached.insert(name.clone()) {
            continue;
        }
        let Some(package) = packages.get(&name) else {
            continue;
        };
        for dependency in &package.dependencies {
            if package.workspace && DEV_ONLY.contains(&dependency.as_str()) {
                continue;
            }
            pending.push(dependency.clone());
        }
    }
    let listed = listed();
    let mut missing = Vec::new();
    let mut refused = Vec::new();
    for name in &reached {
        let Some(package) = packages.get(name) else {
            continue;
        };
        if package.workspace {
            continue;
        }
        match listed.get(name) {
            None => missing.push(name.clone()),
            Some(licence) if !permissive(licence) => refused.push(format!("{name}: {licence}")),
            Some(_) => {}
        }
    }
    assert!(
        missing.is_empty(),
        "crates linked but not in licenses.txt (read their licences, then add them): {missing:?}"
    );
    assert!(
        refused.is_empty(),
        "crates whose licence is not permissive: {refused:?}"
    );
}

#[test]
fn fr37_the_licence_checker_reads_spdx_the_way_it_is_written() {
    assert!(permissive("MIT OR Apache-2.0"));
    assert!(permissive("Unlicense/MIT"));
    assert!(permissive("(MIT OR Apache-2.0) AND Unicode-3.0"));
    assert!(permissive("Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT"));
    assert!(!permissive("GPL-3.0"));
    assert!(!permissive("MIT AND GPL-3.0"));
    assert!(permissive("MIT OR GPL-3.0"));
}

/// The grammar and theme files two-face bundles each carry a licence of their own, and
/// two-face records which. Every one of them has to be permissive too.
#[cfg(not(target_family = "wasm"))]
#[test]
fn fr37_every_bundled_grammar_is_permissively_licensed() {
    use two_face::acknowledgement::{LicenseType, listing};

    let acknowledgements = listing();
    assert!(!acknowledgements.for_syntaxes().is_empty());
    for licence in acknowledgements
        .for_syntaxes()
        .iter()
        .chain(acknowledgements.for_themes())
    {
        // WTFPL is a public-domain-style grant with no conditions at all. It is listed
        // here deliberately; see README.md.
        let allowed = matches!(
            licence.ty,
            LicenseType::Sublime
                | LicenseType::Mit
                | LicenseType::Bsd2Clause
                | LicenseType::Bsd2ClauseFreeBsd
                | LicenseType::Unlicense
                | LicenseType::Bsd3Clause
                | LicenseType::Apache2
                | LicenseType::Wtfpl
        );
        assert!(
            allowed,
            "{} is under {:?}",
            licence.rel_path.display(),
            licence.ty
        );
    }
}
