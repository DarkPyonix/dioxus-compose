//! A document arriving a piece at a time draws what the finished text draws, and costs
//! only the block the piece landed in.

mod support;

use dioxus_compose::WidgetKind;
use dioxus_compose::prelude::ReadableExt;
use dioxus_compose::protocol::Mutation;
use dioxus_compose_markdown::{MarkdownOptions, MarkdownStream, parse};
use support::apps::{self, quiet};
use support::{Screen, corpus, one_shot_dump};

/// Streams `pieces` into a fresh screen, one frame per piece, and finishes.
fn streamed_dump(pieces: &[&str], options: MarkdownOptions) -> String {
    apps::reset("", options);
    let mut screen = Screen::new(apps::streamed);
    for piece in pieces {
        apps::push(piece);
        screen.frame();
    }
    apps::finish();
    screen.frame();
    screen.tree.dump()
}

fn boundaries(text: &str) -> Vec<usize> {
    (0..=text.len())
        .filter(|at| text.is_char_boundary(*at))
        .collect()
}

/// The blocks a stream holds are the blocks a one-shot parse finds, at every split of
/// every corpus document, both before `finish` for the part already frozen and after it
/// for everything.
#[test]
fn fr37_streaming_matches_one_shot_at_every_split_for_the_blocks() {
    for document in corpus() {
        let whole = parse(&document, dioxus_compose_markdown::DEFAULT_MAX_DEPTH);
        for at in boundaries(&document) {
            let mut stream = MarkdownStream::new(MarkdownOptions::default());
            stream.push_delta(&document[..at]);
            stream.push_delta(&document[at..]);
            let frozen: Vec<_> = stream.blocks().take(stream.frozen_len()).cloned().collect();
            assert_eq!(
                frozen,
                whole[..frozen.len()],
                "a block frozen before the end differs from the finished parse, split at {at}"
            );
            stream.finish();
            let streamed: Vec<_> = stream.blocks().cloned().collect();
            assert_eq!(streamed, whole, "split at byte {at} of\n{document}");
        }
    }
}

/// The same, one character at a time.
#[test]
fn fr37_streaming_matches_one_shot_one_character_at_a_time_for_the_blocks() {
    for document in corpus() {
        let whole = parse(&document, dioxus_compose_markdown::DEFAULT_MAX_DEPTH);
        let mut stream = MarkdownStream::new(MarkdownOptions::default());
        for character in document.chars() {
            stream.push_delta(character.encode_utf8(&mut [0; 4]));
            let frozen: Vec<_> = stream.blocks().take(stream.frozen_len()).cloned().collect();
            assert_eq!(frozen, whole[..frozen.len()]);
        }
        stream.finish();
        let streamed: Vec<_> = stream.blocks().cloned().collect();
        assert_eq!(streamed, whole);
    }
}

/// The drawn tree after `finish` is the tree the whole text draws, split in two at every
/// position and one character at a time.
#[test]
fn fr37_streaming_matches_one_shot_at_every_split_for_the_tree() {
    for document in corpus() {
        let expected = one_shot_dump(&document, quiet());
        for at in boundaries(&document) {
            let drawn = streamed_dump(&[&document[..at], &document[at..]], quiet());
            assert_eq!(
                drawn, expected,
                "split at byte {at}: the streamed tree differs from the one-shot tree"
            );
        }
        let characters: Vec<String> = document.chars().map(String::from).collect();
        let pieces: Vec<&str> = characters.iter().map(String::as_str).collect();
        assert_eq!(
            streamed_dump(&pieces, quiet()),
            expected,
            "one character at a time"
        );
    }
}

/// A reference definition after the link that uses it: while streaming the link is plain
/// text, because the definition has not arrived, and `finish` makes it the link CommonMark
/// says it is, by building that one block again.
#[test]
fn fr37_a_late_reference_definition_resolves_at_finish() {
    let before = "First [late][x] link.\n\nSecond paragraph.\n\n";
    let definition = "[x]: https://example.com/late\n";
    let document = format!("{before}{definition}");
    let whole = parse(&document, dioxus_compose_markdown::DEFAULT_MAX_DEPTH);
    let mut stream = MarkdownStream::new(MarkdownOptions::default());
    stream.push_delta(before);
    stream.push_delta(definition);
    assert!(
        stream.frozen_len() >= 1,
        "the first paragraph should be frozen by now"
    );
    let first = stream.block(0).expect("no first block").clone();
    assert_ne!(
        Some(&first),
        whole.first(),
        "the frozen block could not have known the definition yet"
    );
    let second = stream.block(1).expect("no second block").clone();
    stream.finish();
    let streamed: Vec<_> = stream.blocks().cloned().collect();
    assert_eq!(streamed, whole);
    assert!(
        stream.block(1).expect("no second block").same(&second),
        "a block the definition does not touch was built again"
    );
    assert_eq!(
        one_shot_dump(&document, quiet()),
        streamed_dump(&[before, definition], quiet())
    );
}

fn text_node(screen: &Screen, text: &str) -> u32 {
    screen
        .tree
        .text_node(text)
        .unwrap_or_else(|| panic!("no Text says {text:?}:\n{}", screen.tree.dump()))
}

/// An unclosed fence is a code block from the frame its opening line arrives in, and the
/// lines after it land inside that block.
#[test]
fn fr37_an_open_fence_is_a_code_block_at_once() {
    apps::reset("", quiet());
    let mut screen = Screen::new(apps::streamed);
    apps::push("```rust");
    screen.frame();
    let language = text_node(&screen, "rust");
    let block = screen.tree.parent_of(screen.tree.parent_of(language).unwrap()).unwrap();
    apps::push("\nfn main() {");
    screen.frame();
    let code = text_node(&screen, "fn main() {");
    assert!(
        screen.tree.subtree(block).contains(&code),
        "the code did not land in the block the fence opened"
    );
    apps::push("}\n```\n");
    screen.frame();
    assert!(screen.tree.subtree(block).contains(&code));
    assert_eq!(screen.tree.nodes[&code].text.as_deref(), Some("fn main() {}"));
}

/// `**bold` is shown as written, with no bold run, and when the closing `**` arrives that
/// one Text changes its words and its runs, and nothing below it moves.
#[test]
fn fr37_unclosed_emphasis_is_plain_until_it_closes() {
    apps::reset("", quiet());
    let mut screen = Screen::new(apps::streamed);
    apps::push("Before.\n\n**bol");
    screen.frame();
    let open = text_node(&screen, "**bol");
    assert!(screen.tree.nodes[&open].spans.is_empty());

    apps::push("d**");
    let structural = screen.frame_with(|_, mutations| {
        let touched: Vec<_> = mutations
            .iter()
            .filter(|mutation| {
                matches!(
                    mutation,
                    Mutation::Insert { .. }
                        | Mutation::Move { .. }
                        | Mutation::Remove { .. }
                        | Mutation::Create { .. }
                )
            })
            .map(|mutation| format!("{mutation:?}"))
            .collect();
        let others: Vec<_> = mutations
            .iter()
            .filter_map(|mutation| match mutation {
                Mutation::SetProp { node_id, .. }
                | Mutation::AppendText { node_id, .. }
                | Mutation::SetModifier { node_id, .. } => Some(*node_id),
                _ => None,
            })
            .filter(|node_id| *node_id != open)
            .collect::<Vec<_>>();
        (touched, others)
    });
    assert!(structural.0.is_empty(), "closing the emphasis moved nodes: {:?}", structural.0);
    assert!(structural.1.is_empty(), "closing the emphasis touched other nodes: {:?}", structural.1);
    assert_eq!(screen.tree.nodes[&open].text.as_deref(), Some("bold"));
    assert!(!screen.tree.nodes[&open].spans.is_empty(), "no bold run after it closed");
}

/// A delta after a block closed sends nothing for that block, and a delta to a paragraph
/// whose runs do not change is one `AppendText` for its Text.
#[test]
fn fr37_a_frozen_block_gets_no_mutations_and_a_growing_paragraph_one_append() {
    apps::reset("", quiet());
    let mut screen = Screen::new(apps::streamed);
    apps::push("# Title\n\nFirst para");
    screen.frame();
    let title = text_node(&screen, "Title");
    let paragraph = text_node(&screen, "First para");
    let frozen = screen.tree.subtree(title);

    // The paragraph's first line completes, which is also what freezes the heading.
    apps::push("graph keeps\n");
    let mutations = screen.frame_with(|_, mutations| format!("{mutations:?}"));
    let expected = format!(
        "{:?}",
        vec![Mutation::AppendText {
            node_id: paragraph,
            text: "graph keeps",
        }]
    );
    assert_eq!(mutations, expected);
    assert_eq!(
        apps::stream().peek().frozen_len(),
        1,
        "the heading is not frozen"
    );

    for delta in ["going", " on", " and on.", "\nA second line."] {
        apps::push(delta);
        let touched_frozen = screen.frame_with(|_, mutations| {
            mutations
                .iter()
                .filter(|mutation| match mutation {
                    Mutation::SetProp { node_id, .. }
                    | Mutation::AppendText { node_id, .. }
                    | Mutation::SetText { node_id, .. }
                    | Mutation::SetModifier { node_id, .. }
                    | Mutation::Remove { node_id }
                    | Mutation::Move { node_id, .. } => frozen.contains(node_id),
                    _ => false,
                })
                .count()
        });
        assert_eq!(
            touched_frozen, 0,
            "a frozen block's nodes were touched by {delta:?}"
        );
    }
    assert_eq!(
        screen.tree.nodes[&paragraph].text.as_deref(),
        Some("First paragraph keeps going on and on. A second line.")
    );
}

/// Blocks arriving below a frozen one never move it, and the frozen block's Text keeps
/// its node from the first frame to the last.
#[test]
fn fr37_new_blocks_never_disturb_the_ones_above() {
    apps::reset("", quiet());
    let mut screen = Screen::new(apps::streamed);
    apps::push("Paragraph one.\n\n");
    screen.frame();
    apps::push("Paragraph two.\n\n");
    screen.frame();
    let first = text_node(&screen, "Paragraph one.");
    for index in 0..40 {
        apps::push(&format!("Paragraph {index} more.\n\n"));
        let moved = screen.frame_with(|_, mutations| {
            mutations.iter().any(|mutation| match mutation {
                Mutation::Remove { node_id } | Mutation::Move { node_id, .. } => {
                    *node_id == first
                }
                Mutation::SetProp { node_id, .. } | Mutation::AppendText { node_id, .. } => {
                    *node_id == first
                }
                _ => false,
            })
        });
        assert!(!moved, "the first paragraph was touched by block {index}");
    }
    apps::finish();
    screen.frame();
    assert_eq!(text_node(&screen, "Paragraph one."), first);
    let texts = screen
        .tree
        .find(|node| node.widget == WidgetKind::Text)
        .len();
    assert_eq!(texts, 42);
}
