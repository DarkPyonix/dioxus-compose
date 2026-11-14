//! What a streamed delta costs the Host, with and without a long settled document above
//! the block it lands in, and what closing a very long code block costs the frame.
//!
//! The numbers go to standard error in the same `name=value` form as the dioxus-compose
//! benches, and the assertions at the end are the budgets: a delta must cost the same
//! with a hundred kilobytes above it as with nothing above it, give or take a fifth, and
//! must not allocate more because of what is above it.

use criterion::{Criterion, criterion_group, criterion_main};
use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose_markdown::{Markdown, MarkdownOptions, MarkdownStream};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::{Cell, RefCell};
use std::time::Instant;

/// The streaming frame budget: one delta, parsed, diffed and encoded.
const STREAMING_FRAME_BUDGET_NS: u128 = 1_000_000;
/// How much slower a delta may be with a long settled prefix than with none.
const PREFIX_RATIO_CEILING: f64 = 1.2;

struct CountingAllocator;

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    static STREAM: RefCell<Option<SyncSignal<MarkdownStream>>> = const { RefCell::new(None) };
}

fn record_allocation() {
    let _ = TRACKING.try_with(|tracking| {
        if tracking.get() {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
    });
}

// SAFETY: Every operation delegates to the process System allocator unchanged.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        // SAFETY: Delegating the caller-provided layout to System.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Delegating the original pointer and layout to System.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        // SAFETY: Delegating the original allocation and requested size to System.
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn options() -> MarkdownOptions {
    MarkdownOptions {
        // The highlighter's thread would land work in the middle of a measurement.
        highlight: false,
        ..MarkdownOptions::default()
    }
}

fn streamed() -> Element {
    let stream = use_signal_sync(|| MarkdownStream::new(options()));
    STREAM.with(|slot| *slot.borrow_mut() = Some(stream));
    rsx! {
        Column {
            Markdown { stream, options: options(), on_link: move |_: String| {} }
        }
    }
}

fn stream() -> SyncSignal<MarkdownStream> {
    STREAM.with(|slot| slot.borrow().expect("the screen has not been built"))
}

/// Settled paragraphs, each followed by a blank line, adding up to at least `bytes`.
fn settled(bytes: usize) -> String {
    let mut text = String::new();
    let mut index = 0;
    while text.len() < bytes {
        text.push_str(&format!(
            "Paragraph {index} of the settled part, with **some** emphasis and a \
             [link](https://example.com/{index}) so it is not trivially plain.\n\n"
        ));
        index += 1;
    }
    text
}

/// A Host and the stream its screen draws.
struct Screen {
    host: Host,
    stream: SyncSignal<MarkdownStream>,
}

/// A screen holding `prefix` bytes of settled blocks and a one-kilobyte open paragraph.
fn screen(prefix: usize) -> Screen {
    let mut host = Host::new(streamed);
    let _ = host.rebuild().unwrap();
    let mut stream = stream();
    stream.write().push_delta(&settled(prefix));
    let _ = host.render_frame(0).unwrap();
    let open = "word ".repeat(200);
    stream.write().push_delta(&open);
    let _ = host.render_frame(0).unwrap();
    Screen { host, stream }
}

/// One delta as the Host sees it: the parse, then the frame that draws it.
fn delta(screen: &mut Screen) {
    screen.stream.write().push_delta("a ");
    let batch = screen.host.render_frame(0).unwrap();
    std::hint::black_box(batch.len());
}

/// How many times the timed comparisons may be measured before the bench fails.
///
/// Load on the machine only ever adds time, and a ratio of two p99s moves with whatever
/// else the machine is doing: a saturated CI runner once measured a prefix ratio of 3.4 for
/// code that measures about 1 when idle. Code that really looks at the settled prefix again
/// is slow on every attempt, so passing on any one attempt still catches it. Allocation
/// counts do not depend on load, so that assertion is checked once.
const ATTEMPTS: usize = 3;

fn p99(mut samples: Vec<u128>) -> u128 {
    samples.sort_unstable();
    samples[(samples.len() * 99).div_ceil(100) - 1]
}

/// The p99 of a delta on a screen with no prefix and on one with 100KB above it, measured
/// in turn (one delta on each, then the next) so a burst of load falls on both alike.
fn prefix_p99s(iterations: usize) -> (u128, u128) {
    // Fresh screens, so both start from a one-kilobyte paragraph and grow alike.
    let mut bare = screen(0);
    let mut long = screen(100 * 1024);
    let mut bare_samples = Vec::with_capacity(iterations);
    let mut long_samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        delta(&mut bare);
        bare_samples.push(started.elapsed().as_nanos());
        let started = Instant::now();
        delta(&mut long);
        long_samples.push(started.elapsed().as_nanos());
    }
    (p99(bare_samples), p99(long_samples))
}

/// A five-thousand-line code block arrives open, then its closing fence: the frame that
/// closes it is the one that must stay inside the budget, because that is where the
/// highlighting would land if it ran on this thread.
fn closing_frame_ns() -> u128 {
    let mut host = Host::new(streamed);
    let _ = host.rebuild().unwrap();
    let mut code_stream = stream();
    let code: String = (0..5000)
        .map(|line| format!("let value_{line} = {line}; // line {line}\n"))
        .collect();
    code_stream.write().push_delta(&format!("```rust\n{code}"));
    let _ = host.render_frame(0).unwrap();
    code_stream.write().push_delta("```\n");
    let started = Instant::now();
    let _ = host.render_frame(0).unwrap();
    started.elapsed().as_nanos()
}

fn allocations_per_delta(screen: &mut Screen, deltas: usize) -> usize {
    // Warm first, so buffers that grow once are not counted against the steady state.
    for _ in 0..20 {
        delta(screen);
    }
    ALLOCATIONS.with(|count| count.set(0));
    TRACKING.with(|tracking| tracking.set(true));
    for _ in 0..deltas {
        delta(screen);
    }
    TRACKING.with(|tracking| tracking.set(false));
    ALLOCATIONS.with(Cell::get) / deltas
}

fn benchmarks(criterion: &mut Criterion) {
    let samples = if std::env::var_os("DXC_BENCH_QUICK").is_some() {
        300
    } else {
        2_000
    };

    let mut bare = screen(0);
    criterion.bench_function("markdown_delta_without_prefix", |bencher| {
        bencher.iter(|| delta(&mut bare))
    });
    let mut long = screen(100 * 1024);
    criterion.bench_function("markdown_delta_after_100kb", |bencher| {
        bencher.iter(|| delta(&mut long))
    });

    let mut half = screen(50 * 1024);
    let half_allocations = allocations_per_delta(&mut half, 200);
    let mut long = screen(100 * 1024);
    let long_allocations = allocations_per_delta(&mut long, 200);
    eprintln!("markdown_delta_allocations_after_50kb={half_allocations}");
    eprintln!("markdown_delta_allocations_after_100kb={long_allocations}");
    assert!(
        long_allocations <= half_allocations,
        "a delta allocated {long_allocations} times after 100KB and {half_allocations} after \
         50KB; allocations grow with the settled part"
    );

    // The timed budgets: pass if any attempt meets all of them (see `ATTEMPTS`).
    let mut failures = Vec::new();
    for attempt in 1..=ATTEMPTS {
        let (bare_p99, long_p99) = prefix_p99s(samples);
        let ratio = long_p99 as f64 / bare_p99.max(1) as f64;
        let closing_frame = closing_frame_ns();

        eprintln!("p99 markdown_delta_without_prefix_ns={bare_p99}");
        eprintln!("p99 markdown_delta_after_100kb_ns={long_p99}");
        eprintln!("markdown_delta_prefix_ratio={ratio:.3}");
        eprintln!("markdown_close_5000_line_code_block_frame_ns={closing_frame}");

        let mut problems = Vec::new();
        if long_p99 > STREAMING_FRAME_BUDGET_NS {
            problems.push(format!(
                "a delta after 100KB took {long_p99}ns at p99, over the \
                 {STREAMING_FRAME_BUDGET_NS}ns streaming frame budget"
            ));
        }
        if ratio > PREFIX_RATIO_CEILING {
            problems.push(format!(
                "a delta after 100KB took {ratio:.2} times as long as one with nothing above \
                 it ({long_p99}ns against {bare_p99}ns at p99); the settled part is being \
                 looked at again"
            ));
        }
        if closing_frame > STREAMING_FRAME_BUDGET_NS {
            problems.push(format!(
                "closing a 5000-line code block took {closing_frame}ns on the drawing thread"
            ));
        }
        if problems.is_empty() {
            return;
        }
        failures.push(format!("attempt {attempt}: {}", problems.join("; ")));
    }
    panic!(
        "the streaming budgets failed on all {ATTEMPTS} attempts:\n{}",
        failures.join("\n")
    );
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
