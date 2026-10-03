//! Untrusted input: random documents, split at random, must never panic and must always
//! agree with the one-shot parse; pathological nesting is capped and costs linear time.
//!
//! The random documents are drawn from markdown's own punctuation, so most of them land on
//! the edges where block structure is decided. The generator is seeded, so a failure names
//! the seed that reproduces it, and `DXC_FUZZ_SECONDS` runs it for longer than the default.

mod support;

use dioxus_compose_markdown::model::Block;
use dioxus_compose_markdown::{
    DEFAULT_MAX_DEPTH, MarkdownOptions, MarkdownStream, ansi, code, parse,
};
use std::time::{Duration, Instant};
use support::apps::{self, quiet};
use support::Screen;

/// A small, fixed, seedable generator. Reproducibility matters more here than quality.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound.max(1) as u64) as usize
    }
}

const PIECES: &[&str] = &[
    "#", "## ", "# ", " ", "  ", "\n", "\n\n", "*", "**", "_", "__", "~~", "`", "``", "```",
    "```rust\n", "```diff\n", "~~~\n", "> ", ">", "- ", "* ", "+ ", "1. ", "2) ", "[", "]",
    "(", ")", "![", "<", ">", "|", "| a | b |\n", "|---|:-:|\n", ":", "-", "---\n", "===\n",
    "+", "\t", "    ", "&amp;", "&#x41;", "&", "\\", "https://example.com/x", "<https://a.b>",
    "[x]: /url\n", "[x]", "[y][x]", "a", "word", "words here", "é", "한글", "😀",
    "\u{1b}[31m", "\r\n", "\r", "<div>", "</div>", "<!--", "-->", "<script>", "- [ ] ",
    "- [x] ", "\0", "\u{7f}", "    code\n",
];

fn document(random: &mut Random) -> String {
    let length = random.below(120);
    (0..length)
        .map(|_| PIECES[random.below(PIECES.len())])
        .collect()
}

fn split(random: &mut Random, text: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let mut at = 1 + random.below(rest.len().min(24));
        while !rest.is_char_boundary(at) {
            at += 1;
        }
        let (piece, tail) = rest.split_at(at.min(rest.len()));
        pieces.push(piece.to_owned());
        rest = tail;
    }
    pieces
}

fn fuzz_seconds() -> u64 {
    std::env::var("DXC_FUZZ_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(5)
}

#[test]
fn fr37_random_documents_never_panic_and_stream_like_they_parse() {
    let budget = Duration::from_secs(fuzz_seconds());
    let started = Instant::now();
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut cases = 0_u64;
    while started.elapsed() < budget || cases < 200 {
        seed = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut random = Random(seed | 1);
        let text = document(&mut random);
        let whole = parse(&text, DEFAULT_MAX_DEPTH);
        let mut stream = MarkdownStream::new(MarkdownOptions::default());
        for piece in split(&mut random, &text) {
            stream.push_delta(&piece);
            let frozen: Vec<_> = stream
                .blocks()
                .take(stream.frozen_len())
                .cloned()
                .collect();
            // A frozen block can only differ from the finished parse by a link whose
            // definition comes later; such a document is checked after `finish` only.
            if !text.contains("]:") {
                assert_eq!(
                    frozen,
                    whole[..frozen.len().min(whole.len())],
                    "seed {seed:#x}: a frozen block differs from the finished parse\n{text:?}"
                );
            }
        }
        stream.finish();
        let streamed: Vec<_> = stream.blocks().cloned().collect();
        assert_eq!(
            streamed, whole,
            "seed {seed:#x}: the streamed document differs from the one-shot parse\n{text:?}"
        );
        // The code paths that run on the drawing side, fed the same text.
        let _ = ansi::clean(&text, 20);
        let _ = code::diff_spans(&text);
        if cases % 25 == 0 {
            apps::reset(&text, quiet());
            let _ = Screen::new(apps::one_shot);
        }
        cases += 1;
    }
}

fn depth(blocks: &[Block]) -> usize {
    blocks
        .iter()
        .map(|block| match block {
            Block::Quote(inner) => 1 + depth(inner),
            Block::List { items, .. } => {
                1 + items
                    .iter()
                    .map(|item| depth(&item.blocks))
                    .max()
                    .unwrap_or(0)
            }
            _ => 0,
        })
        .max()
        .unwrap_or(0)
}

/// Ten thousand quotes and ten thousand brackets are drawn within the nesting cap.
#[test]
fn fr37_ten_thousand_levels_stay_within_the_cap() {
    for text in [
        format!("{}deep\n", ">".repeat(10_000)),
        format!("{}deep\n", "[".repeat(10_000)),
        format!("{}deep\n", "- ".repeat(10_000)),
    ] {
        let blocks: Vec<Block> = parse(&text, DEFAULT_MAX_DEPTH)
            .iter()
            .map(|block| block.block().clone())
            .collect();
        assert!(
            depth(&blocks) <= DEFAULT_MAX_DEPTH,
            "nested {} deep",
            depth(&blocks)
        );
        apps::reset(&text, quiet());
        let screen = Screen::new(apps::one_shot);
        assert!(screen.tree.text_node("deep").is_some() || screen.tree.dump().contains("deep"));
    }
}

fn best_of_three(text: &str) -> Duration {
    (0..3)
        .map(|_| {
            let started = Instant::now();
            let blocks = parse(text, DEFAULT_MAX_DEPTH);
            std::hint::black_box(blocks);
            started.elapsed()
        })
        .min()
        .unwrap()
}

/// Doubling a pathological input at most doubles the time, give or take noise: the
/// criterion allows two and a half.
#[test]
fn fr37_pathological_nesting_costs_linear_time() {
    for unit in [">", "[", "- ", "*", "`", "<"] {
        let small = format!("{}x\n", unit.repeat(20_000));
        let large = format!("{}x\n", unit.repeat(40_000));
        let (small, large) = (best_of_three(&small), best_of_three(&large));
        let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-9);
        assert!(
            ratio <= 2.5,
            "{unit:?}: doubling the input took {ratio:.2} times as long ({small:?} to {large:?})"
        );
    }
}
