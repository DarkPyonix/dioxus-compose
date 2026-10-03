//! The reference document, drawn and compared against the tree checked in beside it.
//!
//! To accept a deliberate change to the drawing, run with `DXC_BLESS=1` and read the diff
//! of `tests/fixtures/reference.tree` before committing it: blessing writes whatever the
//! code does now, right or wrong.

mod support;

use dioxus_compose::prelude::*;
use dioxus_compose_markdown::{CopyRequest, Markdown, MarkdownOptions};
use std::cell::RefCell;
use support::{ALLOWED_WIDGETS, Screen, fixture};

thread_local! {
    static SOURCE: RefCell<String> = const { RefCell::new(String::new()) };
}

fn quiet() -> MarkdownOptions {
    MarkdownOptions {
        highlight: false,
        ..MarkdownOptions::default()
    }
}

fn reference_app() -> Element {
    let source = SOURCE.with(|source| source.borrow().clone());
    rsx! {
        Column {
            Markdown {
                source,
                options: quiet(),
                on_link: move |_: String| {},
                on_copy: move |_: CopyRequest| {},
            }
        }
    }
}

fn reference_screen() -> Screen {
    SOURCE.with(|source| *source.borrow_mut() = fixture("reference.md"));
    Screen::new(reference_app)
}

#[test]
fn fr37_the_reference_document_draws_the_reference_tree() {
    let screen = reference_screen();
    let drawn = screen.tree.dump();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.tree");
    if std::env::var_os("DXC_BLESS").is_some() {
        std::fs::write(&path, &drawn).expect("could not write the reference tree");
        return;
    }
    let expected = fixture("reference.tree");
    if drawn != expected {
        let mut report = String::new();
        for (line, (want, got)) in expected.lines().zip(drawn.lines()).enumerate() {
            if want != got {
                report.push_str(&format!(
                    "first difference at line {}:\n  expected: {want}\n  drawn:    {got}\n",
                    line + 1
                ));
                break;
            }
        }
        panic!(
            "the reference document no longer draws the reference tree.\n{report}\n\
             drawn tree:\n{drawn}"
        );
    }
}

/// Only the widgets a document is allowed to be made of, so nothing about markdown ever
/// reaches the Renderer as a widget of its own.
#[test]
fn fr37_a_document_is_made_of_existing_widgets_only() {
    let screen = reference_screen();
    for node in screen.tree.nodes.values() {
        let name = format!("{:?}", node.widget);
        assert!(
            ALLOWED_WIDGETS.contains(&name.as_str()),
            "a document drew a {name}"
        );
    }
}

/// The whole message is one selection area: the document's root is the one
/// SelectionContainer, and every Text of the document is inside it.
#[test]
fn fr37_the_document_is_one_selection_area() {
    let screen = reference_screen();
    let containers = screen
        .tree
        .find(|node| format!("{:?}", node.widget) == "SelectionContainer");
    assert_eq!(containers.len(), 1, "{}", screen.tree.dump());
    let inside = screen.tree.subtree(containers[0]);
    for text in screen
        .tree
        .find(|node| node.widget == dioxus_compose::WidgetKind::Text)
    {
        assert!(inside.contains(&text), "a Text sits outside the selection");
    }
}
