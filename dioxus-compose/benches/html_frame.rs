//! What one frame of the HTML and CSS path costs the Host.
//!
//! A frame here is everything the Host does between a change and the drawing elements that
//! go to the renderer, in the order it does it:
//!
//! 1. `apply`: the change reaches the document (a Dioxus render, a style attribute written
//!    the way VS Code's script writes it on a sash drag, or a new viewport);
//! 2. `layout`: stylo resolves styles, blitz-dom rebuilds its layout tree, the layout pass
//!    places every box and text, and the display list is built (`layout_document`, or
//!    `HtmlDom::layout`);
//! 3. `display_list_diff`: `DisplayList::diff_from` the previous frame's list;
//! 4. `plan`: `plan_from` turns the list into drawing elements;
//! 5. `plan_diff`: `diff` against the previous frame's plan, which is what crosses to the
//!    renderer.
//!
//! `total` is the frame from start to end, including dropping the previous frame's list
//! and plan. Each stage is its own Criterion benchmark, timed inside full frames, so the
//! stages and the total come from the same work. After Criterion, every scenario is run
//! again and the p99 of each stage is printed, with the total against the Host's budget
//! for one interaction.
//!
//! The scenarios:
//! - `steady_<region>`: the activity bar, sidebar and tab strip VS Code fixtures laid out
//!   again with nothing changed. Whatever this costs, every frame costs at least that.
//! - `background_change_300_rows`: a Dioxus app of 300 rows in which one row's background
//!   colour flips every frame. One display list entry and one plan change should go out.
//! - `viewport_resize_sidebar`: the sidebar fixture with the window 1px wider or narrower
//!   every frame.
//! - `sash_drag_sidebar`: the sidebar fixture with the sidebar itself 1px wider or
//!   narrower every frame, written into the same two inline styles VS Code's script writes
//!   when its sash is dragged. The fixture pins the workbench to `width: 1440px` inline, so
//!   a window resize alone is expected to move little in it; the boxes inside the sidebar
//!   take their width from these two, so this is the resize that moves them. Each
//!   scenario prints how many entries one of its frames sends before it is measured.
//!
//! The budget is printed, not asserted. `scripts/check.sh` and CI run every bench target,
//! and this path has no recorded numbers yet; whether an overrun fails the build is for
//! whoever records the first baseline to decide.
//!
//! `blitz_resolve_phases/<region>` is a diagnostic: blitz-dom's four resolve phases alone,
//! called the way the layout pass calls them before its own layout, so the share of
//! `layout` that is style and box construction can be read off directly.
//!
//! The fixtures' text is sized by the measurer that answers with VS Code's own text sizes
//! (`tests/support/`), built so that it does not allocate per call. The app's text is
//! sized by a fixed advance per character, which allocates nothing either. Neither is the
//! cost of measuring real text: that is the renderer's, and blitz-dom shapes the same text
//! with Parley on its own regardless.

use std::cell::Cell;
use std::hint::black_box;
use std::time::{Duration, Instant};

use blitz_dom::{BaseDocument, LocalName, Namespace, QualName};
use blitz_traits::shell::{ColorScheme, Viewport};
use criterion::{Criterion, criterion_group, criterion_main};
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    DisplayList, DisplayListDiff, NodeId, Plan, PlanChange, TextLineHeight, TextMeasureRequest,
    TextMeasurer, TextMetrics, diff, layout_document, plan_from,
};
use dioxus_core::ScopeId;

#[path = "../tests/support/mod.rs"]
mod support;

use support::{Captured, CapturedMeasurer, WINDOW_HEIGHT, WINDOW_WIDTH};

/// The Host's share of a frame for one interaction, in nanoseconds: handler, diff and
/// encoding together, at p99.
const HOST_INTERACTION_BUDGET_NS: u128 = 500_000;

/// The diagnostic group: blitz-dom's resolve phases alone, per fixture.
const PHASES_GROUP: &str = "fr34_html_frame/blitz_resolve_phases";

// ------------------------------------------------------------------------------------------
// Frames and their stages.

/// Where one frame's time went.
#[derive(Clone, Copy, Debug, Default)]
struct FrameTimes {
    apply: Duration,
    layout: Duration,
    display_list_diff: Duration,
    plan: Duration,
    plan_diff: Duration,
    total: Duration,
}

/// What one frame would send to the renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Sent {
    /// Entries in the frame's display list, sent or not.
    entries: usize,
    entries_changed: usize,
    entries_removed: usize,
    order_changed: bool,
    plan_changes: usize,
}

impl Sent {
    fn of(list: &DisplayList, list_diff: &DisplayListDiff, changes: &[PlanChange]) -> Self {
        Self {
            entries: list.entries.len(),
            entries_changed: list_diff.changed.len(),
            entries_removed: list_diff.removed.len(),
            order_changed: list_diff.order.is_some(),
            plan_changes: changes.len(),
        }
    }
}

struct Stage {
    name: &'static str,
    time: fn(&FrameTimes) -> Duration,
}

const STAGE_COUNT: usize = 6;

const STAGES: [Stage; STAGE_COUNT] = [
    Stage {
        name: "apply",
        time: |times| times.apply,
    },
    Stage {
        name: "layout",
        time: |times| times.layout,
    },
    Stage {
        name: "display_list_diff",
        time: |times| times.display_list_diff,
    },
    Stage {
        name: "plan",
        time: |times| times.plan,
    },
    Stage {
        name: "plan_diff",
        time: |times| times.plan_diff,
    },
    Stage {
        name: "total",
        time: |times| times.total,
    },
];

trait Frames {
    fn name(&self) -> &str;
    /// Whether a frame starts with a change. Without one, `apply` is always zero and is not
    /// benchmarked.
    fn changes_something(&self) -> bool;
    fn frame(&mut self) -> (FrameTimes, Sent);
}

/// The diff and plan stages, shared by every scenario: everything after the display list.
struct Downstream {
    list: DisplayList,
    plan: Plan,
}

impl Downstream {
    fn new(list: DisplayList) -> Self {
        let plan = plan_from(&list);
        Self { list, plan }
    }

    /// Diffs `list` against the previous frame, plans it and diffs the plan. Returns the
    /// times of the three stages and what would be sent, and keeps `list` and the new plan
    /// for the next frame. `started` is when the frame began, for the total.
    fn finish(
        &mut self,
        list: DisplayList,
        started: Instant,
        mut times: FrameTimes,
    ) -> (FrameTimes, Sent) {
        let at = Instant::now();
        let list_diff = list.diff_from(&self.list);
        let diffed = Instant::now();
        let plan = plan_from(&list);
        let planned = Instant::now();
        let changes = diff(&self.plan, &plan);
        let plan_diffed = Instant::now();

        let sent = Sent::of(&list, &list_diff, &changes);
        drop(black_box(list_diff));
        drop(black_box(changes));
        self.list = list;
        self.plan = plan;
        let ended = Instant::now();

        times.display_list_diff = diffed - at;
        times.plan = planned - diffed;
        times.plan_diff = plan_diffed - planned;
        times.total = ended - started;
        (times, sent)
    }
}

// ------------------------------------------------------------------------------------------
// The VS Code fixtures.

/// What starts each frame of a fixture scenario.
enum Change {
    Nothing,
    /// The window alternates between the capture's width and 1px more.
    WindowWidth,
    /// The sidebar alternates between its captured 300px and 301px, written into the
    /// inline styles of the split view that holds it and of its content, as VS Code's
    /// split view writes them.
    SidebarWidth {
        view: NodeId,
        content: NodeId,
        narrow: [String; 2],
        wide: [String; 2],
    },
}

struct FixtureFrames {
    name: String,
    doc: BaseDocument,
    measurer: CapturedMeasurer,
    downstream: Downstream,
    change: Change,
    wide: bool,
}

impl FixtureFrames {
    fn new(
        name: &str,
        region: &str,
        captured: &Captured,
        change: fn(&BaseDocument) -> Change,
    ) -> Self {
        let mut doc = support::parse_region(region);
        let mut measurer = CapturedMeasurer::without_recording(captured);
        let list = layout_document(&mut doc, &mut measurer);
        measurer.assert_nothing_unknown(region);
        let change = change(&doc);
        Self {
            name: name.to_string(),
            doc,
            measurer,
            downstream: Downstream::new(list),
            change,
            wide: false,
        }
    }
}

fn no_change(_: &BaseDocument) -> Change {
    Change::Nothing
}

fn window_width(_: &BaseDocument) -> Change {
    Change::WindowWidth
}

/// Finds the sidebar's split view (the parent of the sidebar part, probe 0) and its
/// content (probe 16), and prepares their inline styles at both widths.
fn sidebar_width(doc: &BaseDocument) -> Change {
    let style = LocalName::from("style");
    let style_of = |id: NodeId| {
        doc.get_node(id)
            .and_then(|node| node.attr(style.clone()))
            .map(str::to_string)
            .unwrap_or_default()
    };
    let part = support::node_by_probe_id(doc, 0);
    let Some(view) = doc.get_node(part).and_then(|node| node.parent) else {
        panic!("the sidebar part sits in a split view");
    };
    let content = support::node_by_probe_id(doc, 16);
    let (view_style, content_style) = (style_of(view), style_of(content));
    assert!(
        view_style.contains("width: 300px") && content_style.contains("width: 299px"),
        "sidebar.html no longer sizes the sidebar's split view to 300px and its content to \
         299px inline (they are {view_style:?} and {content_style:?}), so the sash drag would \
         not resize anything"
    );
    Change::SidebarWidth {
        view,
        content,
        wide: [
            view_style.replace("width: 300px", "width: 301px"),
            content_style.replace("width: 299px", "width: 300px"),
        ],
        narrow: [view_style, content_style],
    }
}

impl Frames for FixtureFrames {
    fn name(&self) -> &str {
        &self.name
    }

    fn changes_something(&self) -> bool {
        !matches!(self.change, Change::Nothing)
    }

    fn frame(&mut self) -> (FrameTimes, Sent) {
        let started = Instant::now();
        self.wide = !self.wide;
        match &self.change {
            Change::Nothing => {}
            Change::WindowWidth => {
                let width = if self.wide {
                    WINDOW_WIDTH + 1
                } else {
                    WINDOW_WIDTH
                };
                let viewport = Viewport::new(width, WINDOW_HEIGHT, 1.0, ColorScheme::Dark);
                self.doc.set_viewport(viewport);
            }
            Change::SidebarWidth {
                view,
                content,
                narrow,
                wide,
            } => {
                let styles = if self.wide { wide } else { narrow };
                let name = QualName::new(None, Namespace::from(""), LocalName::from("style"));
                let mut mutator = self.doc.mutate();
                mutator.set_attribute(*view, name.clone(), &styles[0]);
                mutator.set_attribute(*content, name, &styles[1]);
            }
        }
        let applied = Instant::now();
        let list = layout_document(&mut self.doc, &mut self.measurer);
        let laid_out = Instant::now();
        let times = FrameTimes {
            apply: applied - started,
            layout: laid_out - applied,
            ..FrameTimes::default()
        };
        self.downstream.finish(list, started, times)
    }
}

/// blitz-dom's resolve phases as the layout pass runs them before its own layout: the
/// style cascade, layout tree construction, the deferred construction tasks (Parley
/// shaping each inline formatting context) and the conversion of computed styles to
/// Taffy styles. Kept in step with `resolve_styles` in src/layout.rs by hand.
fn blitz_resolve_phases(doc: &mut BaseDocument) {
    doc.resolve_stylist(0.0);
    doc.resolve_layout_children();
    doc.resolve_deferred_tasks();
    let root = doc.root_element().id;
    doc.flush_styles_to_layout(root);
}

// ------------------------------------------------------------------------------------------
// A Dioxus app of a few hundred boxes.

const ROWS: usize = 300;
const TARGET_ROW: usize = ROWS / 2;
const ROW_BACKGROUND: &str = "rgb(30, 30, 30)";
const APP_WIDTH: f32 = 400.0;
const APP_HEIGHT: f32 = 800.0;

thread_local! {
    static TARGET_IS_RED: Cell<bool> = const { Cell::new(false) };
}

/// Each row's index and background: every row the same but the target.
fn rows(target: &'static str) -> impl Iterator<Item = (usize, &'static str)> {
    (0..ROWS).map(move |index| {
        let background = if index == TARGET_ROW {
            target
        } else {
            ROW_BACKGROUND
        };
        (index, background)
    })
}

fn list_of_rows() -> Element {
    let target = if TARGET_IS_RED.with(Cell::get) {
        "rgb(255, 0, 0)"
    } else {
        "rgb(0, 0, 255)"
    };
    rsx! {
        div { id: "list", style: "display: flex; flex-direction: column; width: 400px",
            for (index, background) in rows(target) {
                div {
                    key: "{index}",
                    style: "height: 20px; padding: 0px 8px; background-color: {background}",
                    "Row {index}"
                }
            }
        }
    }
}

/// Half an em per character, one line: every row label fits its row, so the width
/// constraint never matters. Allocates nothing.
struct FixedAdvance;

impl TextMeasurer for FixedAdvance {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let size = request.style.font_size;
        let line = match request.style.line_height {
            TextLineHeight::Px(px) => px,
            TextLineHeight::Normal => size * 1.2,
        };
        TextMetrics {
            width: request.text.chars().count() as f32 * size * 0.5,
            height: line,
            first_baseline: line * 0.8,
            line_count: 1,
        }
    }
}

struct AppFrames {
    dom: HtmlDom,
    downstream: Downstream,
}

impl AppFrames {
    fn new() -> Self {
        TARGET_IS_RED.with(|red| red.set(false));
        let config = HtmlConfig {
            stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
            measurer: Some(Box::new(FixedAdvance)),
            ..HtmlConfig::default()
        };
        let mut dom = HtmlDom::with_config(list_of_rows, config);
        let list = dom.layout(APP_WIDTH, APP_HEIGHT, 1.0).clone();
        Self {
            dom,
            downstream: Downstream::new(list),
        }
    }
}

impl Frames for AppFrames {
    fn name(&self) -> &str {
        "background_change_300_rows"
    }

    fn changes_something(&self) -> bool {
        true
    }

    fn frame(&mut self) -> (FrameTimes, Sent) {
        let started = Instant::now();
        TARGET_IS_RED.with(|red| red.set(!red.get()));
        self.dom.virtual_dom_mut().mark_dirty(ScopeId::APP);
        self.dom.render();
        let applied = Instant::now();
        let list = self.dom.layout(APP_WIDTH, APP_HEIGHT, 1.0);
        let laid_out = Instant::now();
        // HtmlDom keeps the list it diffs against itself but does not lend it out, so the
        // shared stages get a copy. The copy is made outside every timed stage, and the
        // pause is taken out of the total.
        let list = list.clone();
        let copied = Instant::now();
        let times = FrameTimes {
            apply: applied - started,
            layout: laid_out - applied,
            ..FrameTimes::default()
        };
        let (mut times, sent) = self.downstream.finish(list, started, times);
        times.total -= copied - laid_out;
        (times, sent)
    }
}

// ------------------------------------------------------------------------------------------
// Running them.

fn p99(samples: &mut [u128]) -> u128 {
    samples.sort_unstable();
    samples[(samples.len() * 99).div_ceil(100) - 1]
}

/// Prints what a frame of `frames` sends, so a scenario that stops changing what it
/// should is visible next to its numbers.
fn report_sent(frames: &mut dyn Frames) {
    let (_, sent) = frames.frame();
    eprintln!(
        "html_frame {}: one frame sends {} of {} display list entries, {} removed, \
         order changed: {}, {} plan changes",
        frames.name(),
        sent.entries_changed,
        sent.entries,
        sent.entries_removed,
        sent.order_changed,
        sent.plan_changes
    );
}

fn bench_frames(criterion: &mut Criterion, frames: &mut dyn Frames) {
    report_sent(frames);
    let name = format!("fr34_html_frame/{}", frames.name());
    let mut group = criterion.benchmark_group(name);
    group.sample_size(20);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));
    for stage in &STAGES {
        if stage.name == "apply" && !frames.changes_something() {
            continue;
        }
        group.bench_function(stage.name, |bencher| {
            bencher.iter_custom(|iterations| {
                let mut spent = Duration::ZERO;
                for _ in 0..iterations {
                    spent += (stage.time)(&frames.frame().0);
                }
                spent
            });
        });
    }
    group.finish();
}

fn print_p99(frames: &mut dyn Frames, samples: usize) {
    let mut by_stage = [(); STAGE_COUNT].map(|_| Vec::with_capacity(samples));
    for _ in 0..samples {
        let (times, _) = frames.frame();
        for (stage, recorded) in STAGES.iter().zip(&mut by_stage) {
            recorded.push((stage.time)(&times).as_nanos());
        }
    }
    let mut total = 0;
    for (stage, recorded) in STAGES.iter().zip(&mut by_stage) {
        if stage.name == "apply" && !frames.changes_something() {
            continue;
        }
        let value = p99(recorded);
        eprintln!("html_frame_p99 {}_{}_ns={value}", frames.name(), stage.name);
        if stage.name == "total" {
            total = value;
        }
    }
    let verdict = if total <= HOST_INTERACTION_BUDGET_NS {
        "within"
    } else {
        "OVER"
    };
    eprintln!(
        "html_frame_p99 {}: total {total}ns at p99 is {verdict} the {HOST_INTERACTION_BUDGET_NS}ns \
         a frame may spend in the Host for one interaction",
        frames.name()
    );
}

fn benchmarks(criterion: &mut Criterion) {
    let captured = support::captured();
    let mut scenarios: Vec<Box<dyn Frames>> = Vec::new();
    for region in support::REGIONS {
        scenarios.push(Box::new(FixtureFrames::new(
            &format!("steady_{region}"),
            region,
            &captured,
            no_change,
        )));
    }
    scenarios.push(Box::new(AppFrames::new()));
    scenarios.push(Box::new(FixtureFrames::new(
        "viewport_resize_sidebar",
        "sidebar",
        &captured,
        window_width,
    )));
    scenarios.push(Box::new(FixtureFrames::new(
        "sash_drag_sidebar",
        "sidebar",
        &captured,
        sidebar_width,
    )));

    for frames in &mut scenarios {
        bench_frames(criterion, frames.as_mut());
    }

    let mut phases = criterion.benchmark_group(PHASES_GROUP);
    phases.sample_size(20);
    phases.warm_up_time(Duration::from_secs(1));
    phases.measurement_time(Duration::from_secs(5));
    for region in support::REGIONS {
        let mut doc = support::parse_region(region);
        let mut measurer = CapturedMeasurer::without_recording(&captured);
        black_box(layout_document(&mut doc, &mut measurer));
        phases.bench_function(region, |bencher| {
            bencher.iter(|| blitz_resolve_phases(black_box(&mut doc)));
        });
    }
    phases.finish();

    let samples = if std::env::var_os("DXC_BENCH_QUICK").is_some() {
        100
    } else {
        1_000
    };
    for frames in &mut scenarios {
        print_p99(frames.as_mut(), samples);
    }
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
