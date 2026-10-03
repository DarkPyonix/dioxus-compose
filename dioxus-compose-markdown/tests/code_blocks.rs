//! Code blocks: colouring off the UI thread, copying, folding, sideways scrolling, raw
//! terminal output and diffs.

mod support;

use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{Mutation, PropertyValue};
use dioxus_compose::schema::PropertyKind;
use dioxus_compose::spans::TextSpans;
use dioxus_compose::{ColorRole, TypeRole, WidgetKind};
use dioxus_compose_markdown::highlight::{self, canonical_language};
use dioxus_compose_markdown::{CodeBlock, CopyRequest, MarkdownOptions};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::time::Duration;
use support::apps::{self, quiet};
use support::{Prop, Screen, Tree};

const WAIT: Duration = Duration::from_secs(60);

/// The Text a code block's code is drawn in: the monospace Text inside a ScrollRow.
fn code_texts(tree: &Tree) -> Vec<u32> {
    tree.find(|node| {
        node.widget == WidgetKind::Text
            && node.props.get(&(PropertyKind::TypeRole as u16))
                == Some(&Prop::Int(i64::from(u16::from(TypeRole::Mono))))
    })
}

fn only_code_text(tree: &Tree) -> u32 {
    let found = code_texts(tree);
    assert_eq!(found.len(), 1, "expected one code block:\n{}", tree.dump());
    found[0]
}

fn span_roles(tree: &Tree, node: u32) -> Vec<(String, Option<ColorRole>)> {
    let text = tree.nodes[&node].text.clone().unwrap_or_default();
    TextSpans::from_bytes(tree.nodes[&node].spans.clone())
        .spans()
        .map(|span| {
            let start = span.start as usize;
            let end = start + span.length as usize;
            let role = match span.color {
                Some(Paint::Role(role)) => Some(role),
                _ => None,
            };
            (text[start..end].to_owned(), role)
        })
        .collect()
}

/// One sample per language the crate promises, each with something every grammar colours.
const SAMPLES: &[(&str, &str)] = &[
    ("rust", "// comment\nfn main() { let x = 1; }\n"),
    ("kotlin", "// comment\nfun main() { val x = 1 }\n"),
    ("swift", "// comment\nlet x = 1\n"),
    ("java", "// comment\nclass A { int x = 1; }\n"),
    ("c", "/* comment */\nint main(void) { return 0; }\n"),
    ("cpp", "// comment\nint main() { return 0; }\n"),
    ("csharp", "// comment\nclass A { int x = 1; }\n"),
    ("go", "// comment\nfunc main() {}\n"),
    ("python", "# comment\ndef f():\n    return 1\n"),
    ("javascript", "// comment\nconst x = 1;\n"),
    ("typescript", "// comment\nconst x: number = 1;\n"),
    ("tsx", "// comment\nconst element = <div>hi</div>;\n"),
    ("json", "{\"key\": \"value\", \"n\": 1}\n"),
    ("yaml", "# comment\nkey: value\n"),
    ("toml", "# comment\nkey = \"value\"\n"),
    ("html", "<!-- comment -->\n<p class=\"x\">hi</p>\n"),
    ("css", "/* comment */\nbody { color: red; }\n"),
    ("shell", "# comment\necho \"hi\"\n"),
    ("sql", "-- comment\nSELECT 1;\n"),
    ("markdown", "# Heading\n\nSome *emphasis*.\n"),
    ("dockerfile", "# comment\nFROM rust:1\n"),
    ("ruby", "# comment\ndef f; 1; end\n"),
];

const DIFF: &str = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-    old();\n+    new();\n }\n";

#[test]
fn fr37_every_promised_language_is_coloured() {
    for (language, sample) in SAMPLES {
        let spans = highlight::highlight(language, sample)
            .unwrap_or_else(|| panic!("no grammar for {language}"));
        assert!(!spans.is_empty(), "{language} came back with no colour");
        assert!(
            spans.iter().all(|span| span.role.is_some()),
            "{language} has a run with no role"
        );
    }
    // The diff is the twenty-second language, coloured by line rather than by grammar.
    assert!(!dioxus_compose_markdown::code::diff_spans(DIFF).is_empty());
}

#[test]
fn fr37_aliases_colour_like_the_names_they_stand_for() {
    let pairs = [
        ("rs", "rust"),
        ("kt", "kotlin"),
        ("py", "python"),
        ("js", "javascript"),
        ("ts", "typescript"),
        ("sh", "shell"),
        ("yml", "yaml"),
    ];
    for (alias, name) in pairs {
        assert_eq!(canonical_language(alias), canonical_language(name));
        let sample = SAMPLES
            .iter()
            .find(|(language, _)| *language == name)
            .map(|(_, sample)| *sample)
            .unwrap();
        assert_eq!(
            highlight::highlight(alias, sample),
            highlight::highlight(name, sample),
            "{alias} and {name} differ"
        );
    }
}

#[test]
fn fr37_an_unknown_language_is_plain_and_not_an_error() {
    assert_eq!(highlight::highlight("no-such-language", "x = 1\n"), None);
    for source in ["```no-such-language\nx = 1\n```\n", "```\nx = 1\n```\n"] {
        apps::reset(source, MarkdownOptions::default());
        let mut screen = Screen::new(apps::one_shot);
        let code = only_code_text(&screen.tree);
        // Give the highlighter thread every chance to answer before looking.
        for _ in 0..40 {
            screen.frame();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(screen.tree.nodes[&code].text.as_deref(), Some("x = 1"));
        assert!(screen.tree.nodes[&code].spans.is_empty(), "{source:?} got runs");
    }
}

/// A block of five thousand lines is drawn without its colours, the colours are worked
/// out on another thread, and they arrive later as one change to that Text's runs.
#[test]
fn fr37_colouring_happens_off_the_ui_thread_and_lands_as_one_change() {
    let code: String = (0..5000)
        .map(|line| format!("let value_{line} = {line}; // line {line}\n"))
        .collect();
    let source = format!("```rust\n{code}```\n");
    apps::reset(&source, MarkdownOptions::default());
    let before = highlight::runs_on_this_thread();
    let mut screen = Screen::new(apps::one_shot);
    let node = only_code_text(&screen.tree);
    assert!(
        screen.tree.nodes[&node].spans.is_empty(),
        "the code was coloured in the frame that drew it"
    );

    let started = std::time::Instant::now();
    let mut arrival = None;
    while started.elapsed() < WAIT {
        let carried = screen.frame_with(|_, mutations| {
            let spans_changed = mutations.iter().any(|mutation| {
                matches!(
                    mutation,
                    Mutation::SetProp {
                        node_id,
                        property: PropertyKind::Spans,
                        ..
                    } if *node_id == node
                )
            });
            spans_changed.then(|| format!("{mutations:?}"))
        });
        if let Some(carried) = carried {
            arrival = Some(carried);
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let arrival = arrival.expect("the colours never arrived");
    let only = screen.tree.nodes[&node].spans.clone();
    assert_eq!(
        arrival,
        format!(
            "{:?}",
            vec![Mutation::SetProp {
                node_id: node,
                property: PropertyKind::Spans,
                value: PropertyValue::Bytes(&only),
            }]
        ),
        "the colours brought other changes with them"
    );
    assert_eq!(
        highlight::runs_on_this_thread(),
        before,
        "the highlighter ran on the thread that draws"
    );
}

/// The copy control hands over the language word as written and the code byte for byte,
/// however the block is coloured, folded or cleaned.
#[test]
fn fr37_copy_gives_the_raw_text_and_the_language_as_written() {
    let long: String = (1..=30).map(|line| format!("line {line}\n")).collect();
    let raw_output = "\u{1b}[31mfailed\u{1b}[0m\n";
    let source = format!("```rs\nfn x() {{}}\n```\n\n```py\n{long}```\n\n```\n{raw_output}```\n");
    apps::reset(&source, MarkdownOptions::default());
    let mut screen = Screen::new(apps::one_shot);
    let copies: Vec<u32> = screen
        .tree
        .find(|node| node.widget == WidgetKind::Button && node.text.as_deref() == Some("Copy"));
    assert_eq!(copies.len(), 3);
    for button in copies {
        let handler = screen.button_handler(button);
        screen.click(button, handler);
    }
    assert_eq!(
        apps::copies(),
        vec![
            CopyRequest {
                language: Some("rs".to_owned()),
                text: "fn x() {}\n".to_owned(),
            },
            CopyRequest {
                language: Some("py".to_owned()),
                text: long,
            },
            CopyRequest {
                language: None,
                text: raw_output.to_owned(),
            },
        ]
    );
}

/// Code scrolls sideways inside its own ScrollRow; the paragraphs around it are not in one.
#[test]
fn fr37_code_scrolls_sideways_and_nothing_else_does() {
    let wide = "x".repeat(400);
    let source = format!("Before.\n\n```\n{wide}\n```\n\nAfter.\n");
    apps::reset(&source, quiet());
    let screen = Screen::new(apps::one_shot);
    let in_scroll_row = |node: u32| {
        let mut current = screen.tree.parent_of(node);
        while let Some(parent) = current {
            if format!("{:?}", screen.tree.nodes[&parent].widget) == "ScrollRow" {
                return true;
            }
            current = screen.tree.parent_of(parent);
        }
        false
    };
    let code = only_code_text(&screen.tree);
    assert_eq!(screen.tree.nodes[&code].text.as_deref(), Some(wide.as_str()));
    assert!(in_scroll_row(code), "the code is not in a ScrollRow");
    for paragraph in ["Before.", "After."] {
        let node = screen.tree.text_node(paragraph).unwrap();
        assert!(!in_scroll_row(node), "{paragraph} scrolls sideways");
    }
}

fn numbered(lines: usize) -> String {
    (1..=lines).map(|line| format!("line {line}\n")).collect()
}

fn first_lines(lines: usize) -> String {
    (1..=lines)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Past twenty lines a block shows twenty and says how many it hides; unfolding shows
/// everything and folding goes back.
#[test]
fn fr37_long_blocks_fold_and_unfold() {
    let source = format!("```\n{}```\n", numbered(25));
    apps::reset(&source, quiet());
    let mut screen = Screen::new(apps::one_shot);
    let code = only_code_text(&screen.tree);
    assert_eq!(
        screen.tree.nodes[&code].text.as_deref(),
        Some(first_lines(20).as_str())
    );
    let more = screen.button("Show 5 more lines");
    let handler = screen.button_handler(more);
    screen.click(more, handler);
    assert_eq!(
        screen.tree.nodes[&code].text.as_deref(),
        Some(first_lines(25).as_str())
    );
    let less = screen.button("Show less");
    let handler = screen.button_handler(less);
    screen.click(less, handler);
    assert_eq!(
        screen.tree.nodes[&code].text.as_deref(),
        Some(first_lines(20).as_str())
    );

    // Twenty lines exactly is not long, and has no control.
    let source = format!("```\n{}```\n", numbered(20));
    apps::reset(&source, quiet());
    let screen = Screen::new(apps::one_shot);
    assert!(
        screen
            .tree
            .find(|node| node.widget == WidgetKind::Button
                && node.text.as_deref().is_some_and(|text| text.starts_with("Show")))
            .is_empty()
    );
}

/// A folded block that is still streaming keeps its height: the code Text does not change
/// as lines arrive past the fold, and nothing in the block moves.
#[test]
fn fr37_a_folded_streaming_block_keeps_its_height() {
    apps::reset("", quiet());
    let mut screen = Screen::new(apps::streamed);
    apps::push("```\n");
    apps::push(&numbered(21));
    screen.frame();
    let code = only_code_text(&screen.tree);
    let shown = screen.tree.nodes[&code].text.clone();
    for line in 22..60 {
        apps::push(&format!("line {line}\n"));
        let touched = screen.frame_with(|_, mutations| {
            mutations
                .iter()
                .filter(|mutation| match mutation {
                    Mutation::Insert { .. }
                    | Mutation::Move { .. }
                    | Mutation::Remove { .. }
                    | Mutation::Create { .. } => true,
                    Mutation::SetProp { node_id, .. } | Mutation::AppendText { node_id, .. } => {
                        *node_id == code
                    }
                    _ => false,
                })
                .count()
        });
        assert_eq!(touched, 0, "line {line} changed the folded block's shape");
    }
    assert_eq!(screen.tree.nodes[&code].text, shown);
    screen.button("Show 39 more lines");
}

thread_local! {
    static SHOWN: RefCell<Option<SyncSignal<bool>>> = const { RefCell::new(None) };
}

/// A message that leaves the screen and comes back, the way a row of a lazy list does,
/// with its fold state held where the application keeps the message.
fn remountable() -> Element {
    let shown = use_signal_sync(|| true);
    SHOWN.with(|slot| *slot.borrow_mut() = Some(shown));
    let expanded = use_signal(BTreeSet::new);
    let source = apps::SOURCE.with(|slot| slot.borrow().clone());
    rsx! {
        Column {
            if shown() {
                dioxus_compose_markdown::Markdown { source, options: quiet(), expanded }
            }
        }
    }
}

#[test]
fn fr37_an_unfolded_block_stays_unfolded_after_leaving_and_returning() {
    let source = format!("```\n{}```\n", numbered(25));
    apps::reset(&source, quiet());
    let mut screen = Screen::new(remountable);
    let more = screen.button("Show 5 more lines");
    let handler = screen.button_handler(more);
    screen.click(more, handler);
    let mut shown = SHOWN.with(|slot| slot.borrow().unwrap());
    shown.set(false);
    screen.frame();
    assert!(code_texts(&screen.tree).is_empty(), "the message did not leave");
    shown.set(true);
    screen.frame();
    let code = only_code_text(&screen.tree);
    assert_eq!(
        screen.tree.nodes[&code].text.as_deref(),
        Some(first_lines(25).as_str()),
        "the block came back folded"
    );
    screen.button("Show less");
}

thread_local! {
    static OUTPUT: RefCell<Option<SyncSignal<String>>> = const { RefCell::new(None) };
    static OUTPUT_COPIES: RefCell<Vec<CopyRequest>> = const { RefCell::new(Vec::new()) };
}

fn tool_output() -> Element {
    let text = use_signal_sync(String::new);
    OUTPUT.with(|slot| *slot.borrow_mut() = Some(text));
    rsx! {
        Column {
            CodeBlock {
                text: text(),
                streaming: true,
                on_copy: move |copy: CopyRequest| OUTPUT_COPIES.with(|slot| slot.borrow_mut().push(copy)),
            }
        }
    }
}

/// A tool's raw output in a `CodeBlock` with no language: escapes are gone from the text,
/// red and green become code colour roles, the copy control gives the raw bytes back, and
/// output that keeps arriving is appended rather than resent.
#[test]
fn fr37_raw_tool_output_keeps_its_colours_as_roles_and_streams_as_appends() {
    let mut screen = Screen::new(tool_output);
    let mut output = OUTPUT.with(|slot| slot.borrow().unwrap());
    let first = "\u{1b}[1m\u{1b}[31merror\u{1b}[0m: \u{1b}[2Kbuild failed\n";
    output.set(first.to_owned());
    screen.frame();
    let code = only_code_text(&screen.tree);
    let shown = screen.tree.nodes[&code].text.clone().unwrap();
    assert_eq!(shown, "error: build failed");
    assert!(!shown.contains('\u{1b}'));

    let second = "\u{1b}[32mtest ok\u{1b}[39m\n";
    output.write().push_str(second);
    let appended = screen.frame_with(|_, mutations| {
        mutations.iter().any(|mutation| {
            matches!(mutation, Mutation::AppendText { node_id, .. } if *node_id == code)
        }) && !mutations.iter().any(|mutation| {
            matches!(
                mutation,
                Mutation::SetProp {
                    node_id,
                    property: PropertyKind::Text,
                    ..
                } if *node_id == code
            )
        })
    });
    assert!(appended, "streamed output was not appended");
    assert_eq!(
        screen.tree.nodes[&code].text.as_deref(),
        Some("error: build failed\ntest ok")
    );
    assert_eq!(
        span_roles(&screen.tree, code),
        vec![
            ("error".to_owned(), Some(ColorRole::DiffRemoved)),
            ("test ok".to_owned(), Some(ColorRole::DiffAdded)),
        ]
    );

    let copy = screen.button("Copy");
    let handler = screen.button_handler(copy);
    screen.click(copy, handler);
    assert_eq!(
        OUTPUT_COPIES.with(|slot| slot.borrow().clone()),
        vec![CopyRequest {
            language: None,
            text: format!("{first}{second}"),
        }]
    );
}

/// A unified diff: added lines in the added role, removed in the removed role, headers in
/// the quiet role, and the `+` and `-` still there to read.
#[test]
fn fr37_a_diff_is_coloured_by_line_and_keeps_its_signs() {
    let source = format!("```diff\n{DIFF}```\n");
    apps::reset(&source, MarkdownOptions::default());
    let screen = Screen::new(apps::one_shot);
    let code = only_code_text(&screen.tree);
    assert_eq!(
        screen.tree.nodes[&code].text.as_deref(),
        Some(DIFF.trim_end_matches('\n'))
    );
    assert_eq!(
        span_roles(&screen.tree, code),
        vec![
            ("--- a/src/lib.rs".to_owned(), Some(ColorRole::SyntaxComment)),
            ("+++ b/src/lib.rs".to_owned(), Some(ColorRole::SyntaxComment)),
            ("@@ -1,3 +1,3 @@".to_owned(), Some(ColorRole::SyntaxComment)),
            ("-    old();".to_owned(), Some(ColorRole::DiffRemoved)),
            ("+    new();".to_owned(), Some(ColorRole::DiffAdded)),
        ]
    );
}

/// No colour this crate draws is a literal: not in the reference documents, not in any
/// language sample once coloured, not in a diff.
#[test]
fn fr37_no_literal_colour_is_ever_drawn() {
    let mut documents = support::corpus();
    for (language, sample) in SAMPLES {
        documents.push(format!("```{language}\n{sample}```\n"));
    }
    documents.push(format!("```diff\n{DIFF}```\n"));
    for document in documents {
        apps::reset(&document, MarkdownOptions::default());
        // Every frame a Screen takes is checked for literals as it is applied.
        let mut screen = Screen::new(apps::one_shot);
        let fenced_language = document.starts_with("```") && !document.starts_with("```diff");
        if fenced_language {
            let code = only_code_text(&screen.tree);
            let coloured = screen.frame_until(WAIT, |tree| !tree.nodes[&code].spans.is_empty());
            assert!(coloured, "no colours arrived for\n{document}");
        } else {
            for _ in 0..20 {
                screen.frame();
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        for node in screen.tree.nodes.values() {
            for span in TextSpans::from_bytes(node.spans.clone()).spans() {
                assert!(
                    !matches!(span.color, Some(Paint::Literal(_))),
                    "a literal colour in a run"
                );
            }
        }
    }
}
