//! Where a frame of the editor goes on the Host, with no renderer.
//!
//! Every frame runs the stages `HtmlRuntime` runs, in its order, through the crate's public
//! API, and times each one:
//!
//! - `event`: the change reaches the page through the handler it would reach in a window
//!   (`HtmlDom::input` on the textarea for a keystroke, `HtmlDom::pointer_event` with a
//!   wheel turn on the scrollable element for a scroll);
//! - `render`: the `VirtualDom` runs the dirty scope and writes into the document;
//! - `layout`: stylo, blitz-dom and Taffy lay the document out and the display list is
//!   built (`HtmlDom::layout`);
//! - `plan`: the display list becomes drawing elements (`HtmlDom::plan`);
//! - `apply`: the bridge diffs the plan against the last one and writes the records
//!   (`PlanBridge::apply`), and the batch is closed.
//!
//! The records and bytes of each batch are what would cross to the renderer. Text is
//! measured by Parley here, where a window has the renderer measure it, so `layout` here
//! leaves out the measure call and includes Parley's shaping instead.
//!
//! Usage: `headless [real file] [out.md]`. Prints a Markdown table.

use std::fmt::Write as _;
use std::time::Instant;

use dioxus_compose::Batch;
use dioxus_compose::html::prelude::dioxus_core::ScopeId;
use dioxus_compose::html::{
    HtmlConfig, HtmlDom, LiteralColours, ParleyMeasurer, PlanBridge, PointerInput,
};
use dioxus_compose::protocol::decode_batch;
use editor_spike::{
    Options, ScrollMode, Stats, count_cell, drive, generate, load, set_options, set_source,
};

const WIDTH: f32 = editor_spike::VIEWPORT.0;
const HEIGHT: f32 = editor_spike::VIEWPORT.1;

#[derive(Default)]
struct Samples {
    event: Vec<u64>,
    render: Vec<u64>,
    layout: Vec<u64>,
    plan: Vec<u64>,
    apply: Vec<u64>,
    total: Vec<u64>,
    records: Vec<u64>,
    bytes: Vec<u64>,
}

struct Harness {
    dom: HtmlDom,
    bridge: PlanBridge,
    batch: Batch,
}

impl Harness {
    fn new() -> Self {
        let config = HtmlConfig {
            stylesheets: vec![editor_spike::STYLE.to_string()],
            measurer: Some(Box::new(ParleyMeasurer::new())),
            ..HtmlConfig::default()
        };
        Self {
            dom: HtmlDom::with_config(editor_spike::app, config),
            bridge: PlanBridge::new(),
            batch: Batch::new(),
        }
    }

    /// One frame: `event`, then the stages a window's frame runs. Adds its times and its
    /// batch to `samples` when given.
    fn frame(&mut self, event: impl FnOnce(&mut HtmlDom), samples: Option<&mut Samples>) {
        let t0 = Instant::now();
        event(&mut self.dom);
        let t1 = Instant::now();
        self.dom.render();
        let t2 = Instant::now();
        self.dom.layout(WIDTH, HEIGHT, 1.0);
        let t3 = Instant::now();
        let plan = self
            .dom
            .plan(&mut LiteralColours)
            .expect("a laid out page has a plan");
        let t4 = Instant::now();
        self.batch.begin();
        self.bridge
            .apply(plan, (WIDTH, HEIGHT), &self.dom, &mut self.batch);
        let bytes = self.batch.finish().expect("the bridge writes a valid batch");
        let t5 = Instant::now();
        let records = decode_batch(bytes).map_or(0, |records| records.len());
        if let Some(samples) = samples {
            let ns = |a: Instant, b: Instant| (b - a).as_nanos() as u64;
            samples.event.push(ns(t0, t1));
            samples.render.push(ns(t1, t2));
            samples.layout.push(ns(t2, t3));
            samples.plan.push(ns(t3, t4));
            samples.apply.push(ns(t4, t5));
            samples.total.push(ns(t0, t5));
            samples.records.push(records as u64);
            samples.bytes.push(bytes.len() as u64);
        }
    }

    /// Changes the editor from outside a handler, for setting a scenario up.
    fn drive(&mut self, change: impl FnOnce(&mut editor_spike::Editor)) {
        let mut change = Some(change);
        self.dom.virtual_dom_mut().in_scope(ScopeId::ROOT, || {
            drive(|editor| (change.take().unwrap())(editor));
        });
    }

    fn node(&self, id: &str) -> usize {
        self.dom
            .element_by_id(id)
            .unwrap_or_else(|| panic!("the page has #{id}"))
    }
}

/// The letters a scenario types: words and spaces, and a new line every fifty keys.
fn key(index: usize) -> char {
    const TEXT: &[u8] = b"let value = compute(index, 42); ";
    if index % 50 == 49 {
        '\n'
    } else {
        TEXT[index % TEXT.len()] as char
    }
}

fn row(out: &mut String, file: &str, mode: ScrollMode, scenario: &str, samples: &Samples) {
    let _ = writeln!(
        out,
        "| {file} | {} | {scenario} | {} | {} | {} | {} | {} | {} | {} | {} |",
        mode.name(),
        Stats::of(&samples.total).cell(),
        Stats::of(&samples.event).cell(),
        Stats::of(&samples.render).cell(),
        Stats::of(&samples.layout).cell(),
        Stats::of(&samples.plan).cell(),
        Stats::of(&samples.apply).cell(),
        count_cell(&samples.records),
        count_cell(&samples.bytes),
    );
}

fn run(out: &mut String, file: &str, lines: &[String], mode: ScrollMode) {
    set_source(lines.to_vec());
    set_options(Options {
        viewport: (WIDTH, HEIGHT),
        scroll_mode: mode,
    });

    // The first frame builds everything.
    let mut first = Samples::default();
    let built = Instant::now();
    let mut harness = Harness::new();
    let construct = built.elapsed();
    harness.frame(|_| {}, Some(&mut first));
    let _ = writeln!(
        out,
        "<!-- {file} {}: HtmlDom::with_config {:.2} ms -->",
        mode.name(),
        construct.as_secs_f64() * 1e3
    );
    row(out, file, mode, "first frame", &first);

    // Nothing changed: what every frame costs at least, since a frame lays the whole
    // page out again.
    let mut idle = Samples::default();
    for _ in 0..60 {
        harness.frame(|_| {}, Some(&mut idle));
    }
    row(out, file, mode, "frame, nothing changed", &idle);

    let scrollable = harness.node("scrollable");
    let wheel_at = (WIDTH / 2.0, HEIGHT / 2.0);

    // Page down, a hundred times, from the top.
    let mut page = Samples::default();
    for _ in 0..100 {
        harness.frame(
            |dom| {
                dom.pointer_event(
                    scrollable,
                    &PointerInput::wheel(wheel_at.0, wheel_at.1, 0.0, HEIGHT - 19.0),
                );
            },
            Some(&mut page),
        );
    }
    row(out, file, mode, "scroll one page (wheel)", &page);

    // Smooth scrolling: 300 wheel turns of 12px from the middle of the file, about what a
    // trackpad sends at 60 frames a second.
    let middle = lines.len() / 2;
    harness.drive(|editor| editor.jump_to(middle, 0, HEIGHT));
    harness.frame(|_| {}, None);
    let mut wheel = Samples::default();
    for _ in 0..300 {
        harness.frame(
            |dom| {
                dom.pointer_event(
                    scrollable,
                    &PointerInput::wheel(wheel_at.0, wheel_at.1, 0.0, 12.0),
                );
            },
            Some(&mut wheel),
        );
    }
    row(out, file, mode, "smooth wheel 12px/frame", &wheel);

    // Typing: 500 keystrokes into the textarea in the middle of the file, through the
    // `input` handler a window's text field reaches.
    harness.drive(|editor| editor.jump_to(middle, 12, HEIGHT));
    harness.frame(|_| {}, None);
    let input = harness.node("inputarea");
    let mut typed = String::new();
    let mut typing = Samples::default();
    for index in 0..500 {
        typed.push(key(index));
        let value = typed.clone();
        harness.frame(
            |dom| {
                dom.input(input, &value);
            },
            Some(&mut typing),
        );
    }
    row(out, file, mode, "keystroke (500)", &typing);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let real = args.next().filter(|path| !path.is_empty());
    let out_path = args.next();

    let mut out = String::new();
    let _ = writeln!(
        out,
        "Host stages per frame, ms as p50 / p95 / max (n). Records and bytes as p50 / p95 / max. Viewport {WIDTH}x{HEIGHT}, Parley measuring.\n"
    );
    let _ = writeln!(
        out,
        "| file | scroll mode | scenario | total | event | render | layout | plan | apply | records | bytes |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|---|---|");

    let generated = generate(100_000);
    let generated_label = "generated (100000 lines)".to_string();
    for mode in [ScrollMode::Top, ScrollMode::Transform] {
        run(&mut out, &generated_label, &generated, mode);
    }
    if let Some(path) = real.as_deref() {
        let (lines, label) = load(Some(path), 0);
        run(&mut out, &label, &lines, ScrollMode::Top);
    }

    print!("{out}");
    if let Some(path) = out_path {
        std::fs::write(&path, &out).unwrap_or_else(|error| panic!("cannot write {path}: {error}"));
    }
}
