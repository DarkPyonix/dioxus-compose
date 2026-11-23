//! A document arriving a piece at a time draws what the finished text draws, and costs
//! only the block the piece landed in.

mod support;

use dioxus_compose::WidgetKind;
use dioxus_compose::prelude::{ReadableExt, WritableExt};
use dioxus_compose::protocol::Mutation;
use dioxus_compose_markdown::{MarkdownOptions, MarkdownStream, parse};
use std::collections::HashSet;
use std::time::Instant;
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
    let block = screen
        .tree
        .parent_of(screen.tree.parent_of(language).unwrap())
        .unwrap();
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
    assert_eq!(
        screen.tree.nodes[&code].text.as_deref(),
        Some("fn main() {}")
    );
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
    assert!(
        structural.0.is_empty(),
        "closing the emphasis moved nodes: {:?}",
        structural.0
    );
    assert!(
        structural.1.is_empty(),
        "closing the emphasis touched other nodes: {:?}",
        structural.1
    );
    assert_eq!(screen.tree.nodes[&open].text.as_deref(), Some("bold"));
    assert!(
        !screen.tree.nodes[&open].spans.is_empty(),
        "no bold run after it closed"
    );
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
                Mutation::Remove { node_id } | Mutation::Move { node_id, .. } => *node_id == first,
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

/// Finished list items and finished table rows freeze while their container is still
/// growing: deltas to the last item or row touch only that item or row, and streaming cost
/// on a 300-item list stays bounded relative to a short list.
#[test]
fn fr37_c6_finished_list_items_and_table_rows_freeze_on_300_items() {
    // 1. 300-item list: items 0..299 freeze before the list ends.
    let mut list_prefix = String::new();
    for i in 0..299 {
        list_prefix.push_str(&format!("- item {i}\n"));
    }
    let list_open = "- item 299 growing";
    apps::reset("", quiet());
    let mut screen = Screen::new(apps::streamed);
    apps::push(&list_prefix);
    screen.frame();
    apps::push(list_open);
    screen.frame();

    let frozen_item_nodes: HashSet<u32> = (0..299)
        .map(|i| text_node(&screen, &format!("item {i}")))
        .collect();
    let open_item_node = text_node(&screen, "item 299 growing");

    for delta in [" more", " words", " here"] {
        apps::push(delta);
        let mutations = screen.frame_with(|_, mutations| format!("{mutations:?}"));
        let expected = format!(
            "{:?}",
            vec![Mutation::AppendText {
                node_id: open_item_node,
                text: delta,
            }]
        );
        assert_eq!(
            mutations, expected,
            "delta {delta:?} on item 299 should emit only one AppendText"
        );
    }

    apps::finish();
    let touched_on_finish = screen.frame_with(|_, mutations| {
        mutations
            .iter()
            .filter(|mutation| match mutation {
                Mutation::SetProp { node_id, .. }
                | Mutation::AppendText { node_id, .. }
                | Mutation::SetText { node_id, .. }
                | Mutation::SetModifier { node_id, .. }
                | Mutation::Remove { node_id }
                | Mutation::Move { node_id, .. } => frozen_item_nodes.contains(node_id),
                _ => false,
            })
            .count()
    });
    assert_eq!(
        touched_on_finish, 0,
        "finishing the 300-item list touched frozen list items"
    );
    let full_list = format!("{list_prefix}{list_open} more words here");
    assert_eq!(screen.tree.dump(), one_shot_dump(&full_list, quiet()));

    // 2. 300-row table: header and rows 0..299 freeze before the table ends.
    let mut table_prefix = String::from("| Key | Value |\n|---|---|\n");
    for i in 0..299 {
        table_prefix.push_str(&format!("| row {i} | val {i} |\n"));
    }
    let table_open = "| row 299 | growing";
    apps::reset("", quiet());
    let mut table_screen = Screen::new(apps::streamed);
    apps::push(&table_prefix);
    table_screen.frame();
    apps::push(table_open);
    table_screen.frame();

    let mut frozen_table_nodes: HashSet<u32> = HashSet::new();
    frozen_table_nodes.insert(text_node(&table_screen, "Key"));
    frozen_table_nodes.insert(text_node(&table_screen, "Value"));
    for i in 0..299 {
        frozen_table_nodes.insert(text_node(&table_screen, &format!("row {i}")));
        frozen_table_nodes.insert(text_node(&table_screen, &format!("val {i}")));
    }
    let open_cell_node = text_node(&table_screen, "growing");

    for delta in [" more", " text"] {
        apps::push(delta);
        let mutations = table_screen.frame_with(|_, mutations| format!("{mutations:?}"));
        let expected = format!(
            "{:?}",
            vec![Mutation::AppendText {
                node_id: open_cell_node,
                text: delta,
            }]
        );
        assert_eq!(
            mutations, expected,
            "delta {delta:?} on row 299 should emit only one AppendText"
        );
    }

    apps::finish();
    let touched_table_finish = table_screen.frame_with(|_, mutations| {
        mutations
            .iter()
            .filter(|mutation| match mutation {
                Mutation::SetProp { node_id, .. }
                | Mutation::AppendText { node_id, .. }
                | Mutation::SetText { node_id, .. }
                | Mutation::SetModifier { node_id, .. }
                | Mutation::Remove { node_id }
                | Mutation::Move { node_id, .. } => frozen_table_nodes.contains(node_id),
                _ => false,
            })
            .count()
    });
    assert_eq!(
        touched_table_finish, 0,
        "finishing the 300-row table touched frozen table rows"
    );
    let full_table = format!("{table_prefix}{table_open} more text");
    assert_eq!(
        table_screen.tree.dump(),
        one_shot_dump(&full_table, quiet())
    );

    // 3. Scaling check: a delta on item 300 costs no more than on a 2-item list.
    let open_body = format!("- {}", "word ".repeat(200));
    let short_prefix = "- item 0\n";

    let build = |prefix: &str| {
        apps::reset("", quiet());
        let mut screen = Screen::new(apps::streamed);
        let mut stream = apps::stream();
        stream.write().push_delta(prefix);
        screen.frame();
        stream.write().push_delta(&open_body);
        screen.frame();
        for _ in 0..10 {
            stream.write().push_delta("a ");
            screen.frame();
        }
        (screen, stream)
    };

    let p99 = |mut samples: Vec<u128>| -> u128 {
        samples.sort_unstable();
        samples[(samples.len() * 99).div_ceil(100) - 1]
    };

    const ATTEMPTS: usize = 5;
    const ITERATIONS: usize = 100;
    let mut failures = Vec::new();
    for attempt in 1..=ATTEMPTS {
        let (mut short_screen, mut short_stream) = build(short_prefix);
        let (mut long_screen, mut long_stream) = build(&list_prefix);
        let mut short_samples = Vec::with_capacity(ITERATIONS);
        let mut long_samples = Vec::with_capacity(ITERATIONS);
        for _ in 0..ITERATIONS {
            let started = Instant::now();
            short_stream.write().push_delta("a ");
            short_screen.frame();
            short_samples.push(started.elapsed().as_nanos());

            let started = Instant::now();
            long_stream.write().push_delta("a ");
            long_screen.frame();
            long_samples.push(started.elapsed().as_nanos());
        }
        let short_p99 = p99(short_samples);
        let long_p99 = p99(long_samples);
        let ratio = long_p99 as f64 / short_p99.max(1) as f64;
        if ratio <= 2.0 {
            failures.clear();
            break;
        }
        failures.push(format!(
            "attempt {attempt}: ratio={ratio:.2} (long_p99={long_p99}ns, short_p99={short_p99}ns)"
        ));
    }
    assert!(
        failures.is_empty(),
        "a delta on item 300 scaled with the 299 frozen items before it:\n{}",
        failures.join("\n")
    );
}
