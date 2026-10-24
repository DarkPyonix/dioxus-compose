//! A small code editor screen: an activity bar of icon buttons down the left, a sidebar
//! with a file tree whose folders open and close, an editor with a strip of tabs, and a
//! status bar along the bottom.
//!
//! Tabs are at least 120px wide and grow to fit a longer name (`min-width: fit-content`).
//! Clicking a file in the tree opens it in a tab; clicking a tab shows its file.

use std::collections::HashSet;

use dioxus_compose::html::prelude::*;
use dioxus_hooks::use_signal;
use dioxus_signals::{ReadableExt, Signal, WritableExt};

pub const STYLE: &str = r#"
* { box-sizing: border-box; }
html, body { margin: 0; height: 100%; }
body { font-family: system-ui, sans-serif; font-size: 13px; color: #cccccc; background-color: #1e1e1e; }

.workbench { display: flex; flex-direction: column; height: 100%; }
.main { display: flex; flex: 1; min-height: 0; }

.activity-bar {
    display: flex;
    flex-direction: column;
    width: 48px;
    flex-shrink: 0;
    background-color: #333333;
}
.activity-bar button {
    width: 48px;
    height: 48px;
    padding: 0;
    border: 0;
    border-left: 2px solid transparent;
    background-color: transparent;
    color: #858585;
    font-size: 20px;
}
.activity-bar button.active { border-left-color: #ffffff; color: #ffffff; }

.sidebar {
    width: 240px;
    flex-shrink: 0;
    overflow-x: hidden;
    overflow-y: auto;
    background-color: #252526;
}
.sidebar-title {
    display: flex;
    align-items: center;
    height: 32px;
    padding: 0 12px;
    font-size: 11px;
    text-transform: uppercase;
}
.sidebar-empty { margin: 0; padding: 0 12px; }

.tree, .tree ul { list-style: none; margin: 0; padding: 0; }
.tree ul { padding-left: 12px; }
.tree-row {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 22px;
    padding: 0 8px;
    white-space: nowrap;
}
.tree-row.selected { background-color: #37373d; color: #ffffff; }
.twisty { width: 16px; flex-shrink: 0; }

.editor { display: flex; flex-direction: column; flex: 1; min-width: 0; }
.tabs {
    display: flex;
    height: 35px;
    flex-shrink: 0;
    overflow: hidden;
    background-color: #252526;
}
.tab {
    display: flex;
    align-items: center;
    width: 120px;
    min-width: fit-content;
    flex-shrink: 0;
    padding: 0 12px;
    border-right: 1px solid #1e1e1e;
    white-space: nowrap;
    background-color: #2d2d2d;
    color: #969696;
}
.tab.active { background-color: #1e1e1e; color: #ffffff; }
.breadcrumbs { display: flex; align-items: center; height: 22px; flex-shrink: 0; padding: 0 12px; }
.code {
    flex: 1;
    margin: 0;
    padding: 8px 12px;
    overflow: auto;
    font-family: monospace;
    color: #d4d4d4;
}

.statusbar {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 22px;
    flex-shrink: 0;
    padding: 0 8px;
    background-color: #007acc;
    color: #ffffff;
}
"#;

/// A file or a folder in the tree.
pub struct Entry {
    pub name: &'static str,
    /// From the workspace root, `/`-separated. Also the entry's identity.
    pub path: &'static str,
    /// `Some` for a folder, even an empty one.
    pub children: Option<&'static [Entry]>,
}

const fn file(name: &'static str, path: &'static str) -> Entry {
    Entry {
        name,
        path,
        children: None,
    }
}

const fn folder(name: &'static str, path: &'static str, children: &'static [Entry]) -> Entry {
    Entry {
        name,
        path,
        children: Some(children),
    }
}

pub const TREE: &[Entry] = &[
    folder(
        "src",
        "src",
        &[
            folder(
                "ui",
                "src/ui",
                &[
                    file("mod.rs", "src/ui/mod.rs"),
                    file("tabs.rs", "src/ui/tabs.rs"),
                ],
            ),
            file("layout_engine.rs", "src/layout_engine.rs"),
            file("main.rs", "src/main.rs"),
        ],
    ),
    folder("tests", "tests", &[file("layout.rs", "tests/layout.rs")]),
    file("Cargo.toml", "Cargo.toml"),
    file("README.md", "README.md"),
];

/// The views the activity bar switches between, with the glyph each button shows.
const VIEWS: &[(&str, &str)] = &[
    ("Explorer", "\u{2630}"),
    ("Search", "\u{2315}"),
    ("Source Control", "\u{2442}"),
];

fn find(entries: &'static [Entry], path: &str) -> Option<&'static Entry> {
    entries.iter().find_map(|entry| {
        if entry.path == path {
            Some(entry)
        } else {
            entry.children.and_then(|children| find(children, path))
        }
    })
}

/// What a file shows in the editor.
fn source(path: &str) -> &'static str {
    match path {
        "src/main.rs" => "fn main() {\n    app::launch();\n}",
        "src/layout_engine.rs" => "pub fn layout(tree: &Tree) -> Layout {\n    todo()\n}",
        "README.md" => "# Workbench\n\nA small editor screen.",
        _ => "",
    }
}

#[derive(Clone, Copy)]
struct Workbench {
    view: Signal<&'static str>,
    /// Paths of the folders that are open.
    expanded: Signal<HashSet<&'static str>>,
    tabs: Signal<Vec<&'static Entry>>,
    /// Path of the file the editor shows.
    active: Signal<&'static str>,
}

/// The workbench's state, kept by the component that calls this.
fn use_workbench() -> Workbench {
    Workbench {
        view: use_signal(|| "Explorer"),
        expanded: use_signal(|| HashSet::from(["src", "src/ui"])),
        tabs: use_signal(|| {
            ["src/main.rs", "src/layout_engine.rs", "README.md"]
                .into_iter()
                .filter_map(|path| find(TREE, path))
                .collect()
        }),
        active: use_signal(|| "src/main.rs"),
    }
}

impl Workbench {
    fn show(mut self, view: &'static str) {
        self.view.set(view);
    }

    /// A folder opens or closes; a file opens in a tab, or comes to the front if it has
    /// one already.
    fn choose(mut self, entry: &'static Entry) {
        if entry.children.is_some() {
            let mut expanded = self.expanded.write();
            if !expanded.remove(entry.path) {
                expanded.insert(entry.path);
            }
            return;
        }
        if !self.tabs.read().iter().any(|tab| tab.path == entry.path) {
            self.tabs.write().push(entry);
        }
        self.active.set(entry.path);
    }
}

/// The rows of one level of the tree, each open folder followed by its own rows one level
/// further in.
fn tree_rows(entries: &'static [Entry], state: Workbench) -> Element {
    let expanded = state.expanded.cloned();
    let active = state.active.cloned();

    rsx! {
        for entry in entries.iter() {
            li { key: "{entry.path}",
                div {
                    id: "row-{entry.path}",
                    class: if entry.path == active { "tree-row selected" } else { "tree-row" },
                    onclick: move |_| state.choose(entry),
                    span { class: "twisty",
                        if entry.children.is_some() {
                            if expanded.contains(entry.path) { "▾" } else { "▸" }
                        }
                    }
                    span { class: "label", "{entry.name}" }
                }
                if entry.children.is_some() && expanded.contains(entry.path) {
                    ul { {tree_rows(entry.children.unwrap_or(&[]), state)} }
                }
            }
        }
    }
}

pub fn app() -> Element {
    let state = use_workbench();
    let view = state.view.cloned();
    let tabs = state.tabs.cloned();
    let active = state.active.cloned();
    let crumbs = active.replace('/', " \u{203a} ");
    let code = source(active);

    rsx! {
        div { class: "workbench",
            div { class: "main",
                nav { id: "activity-bar", class: "activity-bar",
                    for (name, glyph) in VIEWS.iter().copied() {
                        button {
                            key: "{name}",
                            id: "view-{name}",
                            title: name,
                            class: if name == view { "active" } else { "" },
                            onclick: move |_| state.show(name),
                            "{glyph}"
                        }
                    }
                }
                aside { id: "sidebar", class: "sidebar",
                    div { class: "sidebar-title", "{view}" }
                    if view == "Explorer" {
                        ul { id: "tree", class: "tree", {tree_rows(TREE, state)} }
                    } else {
                        p { class: "sidebar-empty", "Nothing to show yet." }
                    }
                }
                section { id: "editor", class: "editor",
                    div { id: "tabs", class: "tabs",
                        for tab in tabs {
                            div {
                                key: "{tab.path}",
                                id: "tab-{tab.name}",
                                class: if tab.path == active { "tab active" } else { "tab" },
                                onclick: move |_| state.choose(tab),
                                span { class: "tab-label", "{tab.name}" }
                            }
                        }
                    }
                    div { id: "breadcrumbs", class: "breadcrumbs", "{crumbs}" }
                    pre { id: "code", class: "code", "{code}" }
                }
            }
            footer { id: "statusbar", class: "statusbar",
                span { "main" }
                span { "Ln 1, Col 1" }
                span { "UTF-8" }
            }
        }
    }
}
