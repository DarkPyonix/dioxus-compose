//! The HTML path's tolerances against VS Code, with the text measured by the renderer.
//!
//! `dioxus-compose/tests/fr34_box_tolerance.rs` lays the three captured VS Code workbench
//! regions (the activity bar, the explorer sidebar and the editor tab strip) out with the
//! text sizes VS Code itself measured, which shows the box path is right when the text
//! sizes are. This lays the same fixtures out with the measurer a running page uses,
//! [`RendererMeasurer`], answered by the native renderer in a real window, and compares
//! the result with the same capture:
//!
//! - boxes: every `data-probe-id` element against the rect VS Code drew, the same way the
//!   test does (at least 95 percent within 1px, none more than 4px off);
//! - one-line widths: every text run the regions draw on one line against the advance
//!   VS Code measured for that text, and every one-line case of
//!   `experiments/blitz-layout-probe/fixtures/text.html` against the line VS Code drew
//!   (within 1px);
//! - line breaks: every text run's line count against VS Code's (every text in the
//!   regions was drawn on one line), and every case of `text.html`, eight of which wrap,
//!   against the number of lines VS Code drew (no mismatch).
//!
//! Glyph positions inside a line cannot be compared yet: the measure call answers with a
//! run's width, height, baselines and line count, not with where each glyph starts. The
//! report says so instead of standing something else in for them.
//!
//! The renderer answers only once it has composed a frame, so each screen is drawn first
//! with Parley's sizes, laid out again every frame until the renderer has answered, and
//! accepted only when one more layout asks the renderer nothing new: every size in it is
//! then the renderer's. Each screen stays up for a moment after it is measured, then the
//! next one replaces it. At the end the report is printed, written to the path given on
//! the command line, and the process exits.
//!
//! Run it from the repository root, after `scripts/apply-patches.sh`:
//!
//! ```text
//! cargo run --release --manifest-path experiments/renderer-text-tolerance/Cargo.toml -- .scratch/renderer-text-tolerance/report.txt
//! ```
//!
//! The report path has to be inside the repository.

#[path = "../../../dioxus-compose/tests/support/mod.rs"]
mod support;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use std::task::{Context, Poll};

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus_compose::html::{
    BaseDocument, ClickTargets, DisplayList, NodeEntry, NodeId, PlanBridge, Rect, RendererMeasurer,
    TextRun, layout_document, plan_from,
};
use dioxus_compose::protocol::{HostEvent, ProtocolError};
use dioxus_compose::schema::Window;
use dioxus_compose::{Batch, Runtime};

use support::{
    Captured, CapturedMeasurer, REGIONS, Region, Status, WINDOW_HEIGHT, WINDOW_WIDTH, captured,
    parse_region, region,
};

/// The text cases: single lines and wrapping paragraphs in the workbench font, and where
/// VS Code broke and ended each line.
const TEXT_HTML: &str = include_str!("../../blitz-layout-probe/fixtures/text.html");
const TEXT_CAPTURE: &str = include_str!("../../blitz-layout-probe/captured/text.json");
/// The committed workbench stylesheets the region fixtures are laid out with, which stand
/// in for the two stylesheets `text.html` links and the repository does not hold.
const WORKBENCH_CSS: &str =
    include_str!("../../../dioxus-compose/tests/fixtures/fr34/workbench.css");
const RUNTIME_CSS: &str =
    include_str!("../../../dioxus-compose/tests/fixtures/fr34/runtime-rebuilt.css");

/// Drawn between two screens, so the next one is built from nothing rather than diffed
/// against a different document whose node ids happen to coincide.
const BLANK_HTML: &str = "<!DOCTYPE html><html><head></head><body></body></html>";

/// Box tolerance: at least this share of the compared elements within [`CLOSE`] pixels.
const SHARE_WITHIN_CLOSE: f64 = 0.95;
const CLOSE: f32 = 1.0;
/// Box tolerance: no compared element further off than this.
const WORST_ALLOWED: f32 = 4.0;
/// One-line width tolerance.
const LINE_WIDTH_ALLOWED: f32 = 1.0;

/// How long a measured screen stays up before the next one replaces it.
const SHOW_NANOS: u64 = 1_500_000_000;
/// How long a screen waits for the renderer to answer before the probe gives up.
const TIMEOUT_NANOS: u64 = 30_000_000_000;

/// The layout's viewport, in CSS pixels: the capture's window at a device pixel ratio of 1.
const VIEWPORT: (f32, f32) = (WINDOW_WIDTH as f32, WINDOW_HEIGHT as f32);

/// Where the report goes, from the command line.
static REPORT_PATH: OnceLock<PathBuf> = OnceLock::new();

fn main() {
    let path = match report_path() {
        Ok(path) => path,
        Err(problem) => {
            eprintln!("renderer-text-tolerance: {problem}");
            std::process::exit(2);
        }
    };
    let _ = REPORT_PATH.set(path);
    let status = dioxus_compose::LaunchBuilder::new()
        .with_window(
            Window::new()
                .with_title("Renderer text tolerance probe")
                .with_size(WINDOW_WIDTH as u16, WINDOW_HEIGHT as u16),
        )
        .into_core()
        .try_launch_runtime(|| Box::new(Probe::new()) as Box<dyn Runtime>);
    // The probe ends the process itself when it is done, so the loop ending first means
    // the window closed, or never opened, before anything was measured.
    eprintln!(
        "renderer-text-tolerance: the renderer loop ended (status {status}) before the \
         probe finished; no report was written"
    );
    std::process::exit(1);
}

/// The report path from the first argument, made absolute, which has to be inside the
/// repository this probe is in.
fn report_path() -> Result<PathBuf, String> {
    let arg = std::env::args_os().nth(1).ok_or_else(|| {
        "usage: renderer-text-tolerance <report path inside the repository>".to_string()
    })?;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|e| format!("cannot find the repository root: {e}"))?;
    let cwd = std::env::current_dir()
        .and_then(|dir| dir.canonicalize())
        .map_err(|e| format!("cannot read the working directory: {e}"))?;
    let path = normalize(&cwd.join(arg));
    if !path.starts_with(&repo) {
        return Err(format!(
            "{} is outside the repository ({}); everything this probe writes stays inside it",
            path.display(),
            repo.display()
        ));
    }
    Ok(path)
}

/// `path` with `.` and `..` resolved without touching the file system, since the file and
/// its directory need not exist yet.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

// ------------------------------------------------------------------------------------------
// Documents.

fn parse_html(html: &str) -> BaseDocument {
    let config = DocumentConfig {
        viewport: Some(Viewport::new(
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            1.0,
            ColorScheme::Dark,
        )),
        ..DocumentConfig::default()
    };
    HtmlDocument::from_html(html, config).into_inner()
}

/// `text.html` with its two `<link rel="stylesheet">` lines replaced by the committed
/// workbench stylesheets, in the same order.
fn text_cases_html() -> String {
    let mut out = String::with_capacity(TEXT_HTML.len() + WORKBENCH_CSS.len() + RUNTIME_CSS.len());
    let mut links = 0;
    for line in TEXT_HTML.lines() {
        if line.starts_with("<link rel=\"stylesheet\"") {
            links += 1;
            if links == 1 {
                out.push_str("<style>\n");
                out.push_str(WORKBENCH_CSS);
                out.push_str("\n</style>\n<style>\n");
                out.push_str(RUNTIME_CSS);
                out.push_str("\n</style>\n");
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    assert_eq!(
        links, 2,
        "text.html links the workbench and run-time stylesheets"
    );
    out
}

#[derive(Clone, Copy, Debug)]
enum Screen {
    Region(&'static str),
    TextCases,
}

impl Screen {
    fn name(self) -> &'static str {
        match self {
            Screen::Region(name) => name,
            Screen::TextCases => "text cases",
        }
    }

    fn document(self) -> BaseDocument {
        match self {
            Screen::Region(name) => parse_region(name),
            Screen::TextCases => parse_html(&text_cases_html()),
        }
    }
}

/// Nothing on these screens takes a click or a key.
struct Inert;

impl ClickTargets for Inert {
    fn takes_clicks(&self, _node: NodeId) -> bool {
        false
    }

    fn run_target(&self, _owner: NodeId, _container: NodeId) -> Option<NodeId> {
        None
    }
}

// ------------------------------------------------------------------------------------------
// Boxes: the comparison `tests/fr34_box_tolerance.rs` makes.

fn attr<'a>(doc: &'a BaseDocument, id: NodeId, name: &str) -> Option<&'a str> {
    doc.get_node(id)?
        .attrs()?
        .iter()
        .find(|attr| &*attr.name.local == name)
        .map(|attr| &*attr.value)
}

/// Every `data-probe-id` element's rect, the way `getBoundingClientRect` reports it: the
/// border box for an element with a box of its own; for an inline element without one, the
/// union of the content areas of the text inside it; (0, 0, 0, 0) for an element that is
/// not rendered.
///
/// The content area is the font's ascent and descent around the baseline the layout put
/// the run on. Those two font metrics come from the capture, because the measure call does
/// not report them; the baseline, the run's x and its width are the renderer's.
fn computed_rects(
    doc: &BaseDocument,
    list: &DisplayList,
    reference: &CapturedMeasurer,
) -> HashMap<u32, Rect> {
    let boxes: HashMap<usize, &NodeEntry> = list
        .entries
        .iter()
        .filter(|entry| !entry.tag.is_empty())
        .map(|entry| (entry.node, entry))
        .collect();

    let mut inline: HashMap<usize, Rect> = HashMap::new();
    for entry in &list.entries {
        for run in &entry.texts {
            let area = reference.content_area(run);
            let mut current = Some(run.owner);
            while let Some(id) = current {
                if boxes.contains_key(&id) {
                    break;
                }
                inline
                    .entry(id)
                    .and_modify(|rect| *rect = union(*rect, area))
                    .or_insert(area);
                current = doc.get_node(id).and_then(|node| node.parent);
            }
        }
    }

    let mut rects = HashMap::new();
    for (id, _) in doc.tree().iter() {
        let Some(probe_id) =
            attr(doc, id, "data-probe-id").and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let rect = boxes
            .get(&id)
            .map(|entry| entry.rect)
            .or_else(|| inline.get(&id).copied())
            .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
        rects.insert(probe_id, rect);
    }
    rects
}

fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect::new(
        x,
        y,
        a.right().max(b.right()) - x,
        a.bottom().max(b.bottom()) - y,
    )
}

fn is_empty(rect: &Rect) -> bool {
    rect.width == 0.0 && rect.height == 0.0
}

fn show_rect(rect: &Rect) -> String {
    format!(
        "({:.2}, {:.2}, {:.2}, {:.2})",
        rect.x, rect.y, rect.width, rect.height
    )
}

struct BoxRow {
    label: String,
    live: Rect,
    computed: Rect,
    error: f32,
}

struct BoxComparison {
    rows: Vec<BoxRow>,
    hidden_in_both: usize,
    /// Elements in the capture the laid-out fixture has no element for.
    missing: Vec<String>,
}

impl BoxComparison {
    fn within(&self, tolerance: f32) -> usize {
        self.rows
            .iter()
            .filter(|row| row.error <= tolerance)
            .count()
    }

    fn worst(&self) -> f32 {
        self.rows.iter().map(|row| row.error).fold(0.0, f32::max)
    }

    fn meets_tolerance(&self) -> bool {
        !self.rows.is_empty()
            && self.missing.is_empty()
            && self.within(CLOSE) as f64 >= SHARE_WITHIN_CLOSE * self.rows.len() as f64
            && self.worst() <= WORST_ALLOWED
    }
}

/// Matches elements by `data-probe-id`, as the tolerance test does: skipped and excluded
/// elements are left out, an element neither side renders is counted apart, and one only
/// one side renders is compared against (0, 0, 0, 0).
fn compare_boxes(region: &Region, computed: &HashMap<u32, Rect>) -> BoxComparison {
    let mut rows = Vec::new();
    let mut hidden_in_both = 0;
    let mut missing = Vec::new();
    for element in &region.elements {
        if element.status != Status::Compare {
            continue;
        }
        let Some(rect) = computed.get(&element.probe_id).copied() else {
            missing.push(element.label.clone());
            continue;
        };
        if is_empty(&element.rect) && is_empty(&rect) {
            hidden_in_both += 1;
            continue;
        }
        let error = [
            rect.x - element.rect.x,
            rect.y - element.rect.y,
            rect.width - element.rect.width,
            rect.height - element.rect.height,
        ]
        .into_iter()
        .map(f32::abs)
        .fold(0.0, f32::max);
        rows.push(BoxRow {
            label: element.label.clone(),
            live: element.rect,
            computed: rect,
            error,
        });
    }
    BoxComparison {
        rows,
        hidden_in_both,
        missing,
    }
}

// ------------------------------------------------------------------------------------------
// Text: widths and line counts.

/// One text run the renderer measured, beside what VS Code drew for the same text.
struct TextRow {
    label: String,
    /// The renderer's lines; VS Code's lines.
    lines: (u32, u32),
    /// The run's width (its widest line) and VS Code's widest line.
    width: (f32, f32),
}

impl TextRow {
    fn one_line(&self) -> bool {
        self.lines == (1, 1)
    }

    fn width_error(&self) -> f32 {
        (self.width.0 - self.width.1).abs()
    }
}

#[derive(Default)]
struct TextComparison {
    rows: Vec<TextRow>,
    /// Runs in an icon font, whose glyphs the capture holds no width for.
    icon_runs: usize,
    /// Runs the capture has no measurement for, by label.
    unmatched: Vec<String>,
}

impl TextComparison {
    fn one_line_worst(&self) -> Option<&TextRow> {
        self.rows
            .iter()
            .filter(|row| row.one_line())
            .max_by(|a, b| a.width_error().total_cmp(&b.width_error()))
    }

    fn one_line_count(&self) -> usize {
        self.rows.iter().filter(|row| row.one_line()).count()
    }

    fn line_mismatches(&self) -> Vec<&TextRow> {
        self.rows
            .iter()
            .filter(|row| row.lines.0 != row.lines.1)
            .collect()
    }

    fn widths_ok(&self) -> bool {
        self.one_line_worst()
            .is_none_or(|row| row.width_error() <= LINE_WIDTH_ALLOWED)
    }

    fn breaks_ok(&self) -> bool {
        self.line_mismatches().is_empty() && self.unmatched.is_empty()
    }
}

fn run_label(run: &TextRun) -> String {
    format!(
        "{:?} at {}px, weight {}",
        run.text, run.style.font_size, run.style.font_weight
    )
}

/// Every text run in a region against the text table: VS Code drew each of those texts on
/// one line, at the advance the table holds.
fn compare_region_text(list: &DisplayList, reference: &CapturedMeasurer) -> TextComparison {
    let mut comparison = TextComparison::default();
    for entry in &list.entries {
        for run in &entry.texts {
            let family = run
                .style
                .font_family
                .first()
                .map(String::as_str)
                .unwrap_or("");
            if reference.glyphs.contains_key(family) {
                comparison.icon_runs += 1;
                continue;
            }
            match reference.lookup(&run.text, run.style.font_size, run.style.font_weight) {
                Some(measured) => comparison.rows.push(TextRow {
                    label: run_label(run),
                    lines: (run.line_count, 1),
                    width: (run.rect.width, measured.width),
                }),
                None => comparison.unmatched.push(run_label(run)),
            }
        }
    }
    comparison
}

/// Every case of `text.html` against the lines VS Code drew for it.
fn compare_text_cases(
    doc: &BaseDocument,
    list: &DisplayList,
    capture: &serde_json::Value,
) -> TextComparison {
    let mut cases: HashMap<&str, NodeId> = HashMap::new();
    for (id, _) in doc.tree().iter() {
        if let Some(case) = attr(doc, id, "data-case") {
            cases.insert(case, id);
        }
    }
    let mut runs: HashMap<NodeId, Vec<&TextRun>> = HashMap::new();
    for entry in &list.entries {
        for run in &entry.texts {
            runs.entry(run.owner).or_default().push(run);
        }
    }

    let mut comparison = TextComparison::default();
    let results = capture["results"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for result in results {
        let id = result["id"].as_str().unwrap_or("?");
        let widths: Vec<f32> = result["lines"]
            .as_array()
            .map(|lines| {
                lines
                    .iter()
                    .map(|line| line["w"].as_f64().unwrap_or(0.0) as f32)
                    .collect()
            })
            .unwrap_or_default();
        let found = cases.get(id).and_then(|node| runs.get(node));
        let Some(found) = found.filter(|found| !found.is_empty()) else {
            comparison
                .unmatched
                .push(format!("case {id}: no text run in the layout"));
            continue;
        };
        let lines: u32 = found.iter().map(|run| run.line_count).sum();
        let width = found.iter().map(|run| run.rect.width).fold(0.0, f32::max);
        comparison.rows.push(TextRow {
            label: format!("case {id} ({:?})", found[0].text),
            lines: (lines, widths.len() as u32),
            width: (width, widths.iter().copied().fold(0.0, f32::max)),
        });
    }
    comparison
}

// ------------------------------------------------------------------------------------------
// The report.

#[derive(Default)]
struct Verdicts {
    boxes_ok: bool,
    widths_ok: bool,
    breaks_ok: bool,
    /// The largest one-line width difference over every screen, with its label.
    width_worst: Option<(f32, String)>,
    line_mismatches: usize,
}

impl Verdicts {
    fn new() -> Self {
        Self {
            boxes_ok: true,
            widths_ok: true,
            breaks_ok: true,
            ..Self::default()
        }
    }

    fn add_text(&mut self, text: &TextComparison) {
        self.widths_ok &= text.widths_ok();
        self.breaks_ok &= text.breaks_ok();
        self.line_mismatches += text.line_mismatches().len();
        if let Some(row) = text.one_line_worst() {
            let error = row.width_error();
            if self
                .width_worst
                .as_ref()
                .is_none_or(|(worst, _)| error > *worst)
            {
                self.width_worst = Some((error, row.label.clone()));
            }
        }
    }
}

fn write_text_section(out: &mut String, text: &TextComparison) {
    let one_line = text.one_line_count();
    let _ = writeln!(
        out,
        "  text runs compared: {} ({} on one line in both), {} in an icon font not compared",
        text.rows.len(),
        one_line,
        text.icon_runs
    );
    match text.one_line_worst() {
        Some(row) => {
            let beyond = text
                .rows
                .iter()
                .filter(|row| row.one_line() && row.width_error() > LINE_WIDTH_ALLOWED)
                .count();
            let _ = writeln!(
                out,
                "  one-line width: worst {:.2}px ({}: renderer {:.2}, VS Code {:.2}), {beyond} of \
                 {one_line} beyond {LINE_WIDTH_ALLOWED}px",
                row.width_error(),
                row.label,
                row.width.0,
                row.width.1
            );
        }
        None => {
            let _ = writeln!(out, "  one-line width: no run on one line in both");
        }
    }
    let mut beyond: Vec<&TextRow> = text
        .rows
        .iter()
        .filter(|row| row.one_line() && row.width_error() > LINE_WIDTH_ALLOWED)
        .collect();
    beyond.sort_by(|a, b| b.width_error().total_cmp(&a.width_error()));
    for row in beyond.iter().take(15) {
        let _ = writeln!(
            out,
            "    {:.2}px  {}  renderer {:.2}  VS Code {:.2}",
            row.width_error(),
            row.label,
            row.width.0,
            row.width.1
        );
    }
    let mismatches = text.line_mismatches();
    let _ = writeln!(
        out,
        "  line-count mismatches: {} of {}",
        mismatches.len(),
        text.rows.len()
    );
    for row in mismatches {
        let _ = writeln!(
            out,
            "    {}: renderer {} lines, VS Code {} (widest line: renderer {:.2}, VS Code {:.2})",
            row.label, row.lines.0, row.lines.1, row.width.0, row.width.1
        );
    }
    let wrapped: Vec<&TextRow> = text
        .rows
        .iter()
        .filter(|row| row.lines.0 == row.lines.1 && row.lines.1 > 1)
        .collect();
    for row in wrapped {
        let _ = writeln!(
            out,
            "    {} lines as VS Code: {}, widest line renderer {:.2}, VS Code {:.2}",
            row.lines.1, row.label, row.width.0, row.width.1
        );
    }
    if !text.unmatched.is_empty() {
        let _ = writeln!(
            out,
            "  not compared, no capture to compare with ({}):",
            text.unmatched.len()
        );
        for label in &text.unmatched {
            let _ = writeln!(out, "    {label}");
        }
    }
}

// ------------------------------------------------------------------------------------------
// The runtime.

enum Phase {
    /// Laid out every frame until the renderer has answered every size. `since` is the
    /// frame the screen was first laid out in.
    Measuring { since: Option<u64> },
    /// Measured, and shown until `until`.
    Showing { until: u64 },
}

struct Probe {
    batch: Batch,
    bridge: PlanBridge,
    measurer: RendererMeasurer,
    captured: Captured,
    reference: CapturedMeasurer,
    text_capture: serde_json::Value,
    screens: Vec<Screen>,
    current: usize,
    doc: BaseDocument,
    phase: Phase,
    report: String,
    verdicts: Verdicts,
    finished: bool,
}

impl Probe {
    fn new() -> Self {
        let captured = captured();
        let reference = CapturedMeasurer::new(&captured);
        let text_capture = serde_json::from_str(TEXT_CAPTURE)
            .expect("experiments/blitz-layout-probe/captured/text.json is JSON");
        let mut screens: Vec<Screen> = REGIONS.iter().map(|&name| Screen::Region(name)).collect();
        screens.push(Screen::TextCases);
        let doc = screens[0].document();
        Self {
            batch: Batch::new(),
            bridge: PlanBridge::new(),
            measurer: RendererMeasurer::new(),
            captured,
            reference,
            text_capture,
            screens,
            current: 0,
            doc,
            phase: Phase::Measuring { since: None },
            report: String::new(),
            verdicts: Verdicts::new(),
            finished: false,
        }
    }

    fn screen(&self) -> Screen {
        self.screens[self.current]
    }

    fn draw(&mut self, list: &DisplayList) {
        self.bridge
            .apply(plan_from(list), VIEWPORT, &Inert, &mut self.batch);
    }

    /// Lays the screen out, and returns the layout when every text size in it is one the
    /// renderer answered: the measurer has called the renderer, and laying out once more
    /// asks it nothing new. Draws the latest layout either way.
    fn measure(&mut self) -> Option<DisplayList> {
        let list = layout_document(&mut self.doc, &mut self.measurer);
        if self.measurer.calls() == 0 || self.measurer.unsettled() {
            self.draw(&list);
            return None;
        }
        let calls = self.measurer.calls();
        let again = layout_document(&mut self.doc, &mut self.measurer);
        self.draw(&again);
        (self.measurer.calls() == calls && !self.measurer.unsettled()).then_some(again)
    }

    fn evaluate(&mut self, list: &DisplayList) {
        let screen = self.screen();
        let mut out = String::new();
        let _ = writeln!(out, "{}:", screen.name());
        let text = match screen {
            Screen::Region(name) => {
                let computed = computed_rects(&self.doc, list, &self.reference);
                let boxes = compare_boxes(region(&self.captured, name), &computed);
                let compared = boxes.rows.len();
                let close = boxes.within(CLOSE);
                let _ = writeln!(
                    out,
                    "  boxes: {compared} elements compared ({} rendered in neither), {close} \
                     within {CLOSE}px ({:.1}%), {} beyond {WORST_ALLOWED}px, worst {:.2}px: {}",
                    boxes.hidden_in_both,
                    100.0 * close as f64 / compared.max(1) as f64,
                    compared - boxes.within(WORST_ALLOWED),
                    boxes.worst(),
                    if boxes.meets_tolerance() {
                        "within the tolerance"
                    } else {
                        "OUTSIDE the tolerance"
                    }
                );
                for label in &boxes.missing {
                    let _ = writeln!(out, "    in the capture, not in the fixture: {label}");
                }
                let mut worst: Vec<&BoxRow> =
                    boxes.rows.iter().filter(|row| row.error > CLOSE).collect();
                worst.sort_by(|a, b| b.error.total_cmp(&a.error));
                for row in worst.iter().take(15) {
                    let _ = writeln!(
                        out,
                        "    {:.2}px  {}  VS Code {}  computed {}  dx {:.2} dy {:.2} dw {:.2} \
                         dh {:.2}",
                        row.error,
                        row.label,
                        show_rect(&row.live),
                        show_rect(&row.computed),
                        row.computed.x - row.live.x,
                        row.computed.y - row.live.y,
                        row.computed.width - row.live.width,
                        row.computed.height - row.live.height,
                    );
                }
                self.verdicts.boxes_ok &= boxes.meets_tolerance();
                compare_region_text(list, &self.reference)
            }
            Screen::TextCases => compare_text_cases(&self.doc, list, &self.text_capture),
        };
        write_text_section(&mut out, &text);
        self.verdicts.add_text(&text);
        let _ = writeln!(out, "  measure calls so far: {}", self.measurer.calls());
        print!("{out}");
        self.report.push_str(&out);
        self.report.push('\n');
    }

    /// Replaces the screen with the next one, or ends the probe after the last.
    fn advance(&mut self, now: u64) {
        self.current += 1;
        if self.current == self.screens.len() {
            self.finish(None);
            return;
        }
        let mut blank = parse_html(BLANK_HTML);
        let list = layout_document(&mut blank, &mut self.measurer);
        self.draw(&list);
        self.doc = self.screen().document();
        self.phase = Phase::Measuring { since: Some(now) };
    }

    /// Writes the report and ends the process: with 0 when every screen was measured,
    /// whatever the numbers say, and 1 when `failure` says why one could not be.
    fn finish(&mut self, failure: Option<String>) {
        self.finished = true;
        let zoom = dioxus_compose::zoom::zoom();
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Renderer-measured text against the VS Code capture (macOS, scale 1 layout, \
             1440 x 900 CSS px; renderer zoom k {}, system text size {})",
            zoom.k, zoom.os
        );
        let _ = writeln!(
            out,
            "Every size below is the renderer's answer to the measure call; none is Parley's."
        );
        let _ = writeln!(out);
        out.push_str(&self.report);
        let exit = match failure {
            Some(problem) => {
                let _ = writeln!(out, "NOT MEASURED: {problem}");
                1
            }
            None => {
                let v = &self.verdicts;
                let verdict = |ok: bool| if ok { "met" } else { "NOT met" };
                let _ = writeln!(out, "Summary:");
                let _ = writeln!(
                    out,
                    "  boxes (95% within {CLOSE}px, none beyond {WORST_ALLOWED}px, each \
                     region): {}",
                    verdict(v.boxes_ok)
                );
                let _ = writeln!(
                    out,
                    "  one-line width (within {LINE_WIDTH_ALLOWED}px): {}{}",
                    verdict(v.widths_ok),
                    v.width_worst
                        .as_ref()
                        .map(|(worst, label)| format!(", worst {worst:.2}px ({label})"))
                        .unwrap_or_default()
                );
                let _ = writeln!(
                    out,
                    "  line breaks, as a line count per text (no mismatch): {}, {} mismatches",
                    verdict(v.breaks_ok),
                    v.line_mismatches
                );
                let _ = writeln!(
                    out,
                    "  glyph positions (each glyph's start x within 0.5px): not measurable \
                     until compose-rust's measure call reports per-glyph metrics \
                     (compose-rust #961); the call answers only a run's width, height, \
                     baselines, last line width and line count"
                );
                0
            }
        };
        print!("{out}");
        if let Some(path) = REPORT_PATH.get() {
            let written = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(path, &out));
            match written {
                Ok(()) => println!("report written to {}", path.display()),
                Err(e) => {
                    eprintln!(
                        "renderer-text-tolerance: cannot write {}: {e}",
                        path.display()
                    );
                    std::process::exit(1);
                }
            }
        }
        std::process::exit(exit);
    }
}

impl Runtime for Probe {
    fn batch(&self) -> &Batch {
        &self.batch
    }

    fn batch_mut(&mut self) -> &mut Batch {
        &mut self.batch
    }

    /// Draws the first screen with whatever sizes there are, so the renderer composes a
    /// frame and can answer the measure call from the next one on.
    fn rebuild(&mut self) {
        let list = layout_document(&mut self.doc, &mut self.measurer);
        self.draw(&list);
    }

    fn render(&mut self, frame_time_nanos: Option<u64>) {
        if self.finished {
            return;
        }
        // Only frames move the probe on; a call that answers an event has no clock.
        let Some(now) = frame_time_nanos else {
            return;
        };
        match self.phase {
            Phase::Measuring { since } => {
                let since = since.unwrap_or(now);
                self.phase = Phase::Measuring { since: Some(since) };
                if let Some(list) = self.measure() {
                    self.evaluate(&list);
                    self.phase = Phase::Showing {
                        until: now.saturating_add(SHOW_NANOS),
                    };
                } else if now.saturating_sub(since) > TIMEOUT_NANOS {
                    let problem = format!(
                        "{}: the renderer did not answer every text size within {} s \
                         ({} measure calls made, questions still open: {})",
                        self.screen().name(),
                        TIMEOUT_NANOS / 1_000_000_000,
                        self.measurer.calls(),
                        self.measurer.unsettled()
                    );
                    self.finish(Some(problem));
                }
            }
            Phase::Showing { until } => {
                if now >= until {
                    self.advance(now);
                }
            }
        }
    }

    fn handle_event(&mut self, _event: &HostEvent<'_>) -> Result<i64, ProtocolError> {
        // Nothing on these screens was given a handler.
        Err(ProtocolError::InvalidValueKind(0))
    }

    /// Asks for frame after frame until the probe is done, since what moves it on is the
    /// renderer's answers and the clock, neither of which wakes a frame by itself.
    fn poll_work(&mut self, _context: &mut Context<'_>) -> Poll<()> {
        if self.finished {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}
