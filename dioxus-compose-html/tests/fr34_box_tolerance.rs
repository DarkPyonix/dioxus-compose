//! The box tolerance, measured against VS Code.
//!
//! The blitz-layout probe (experiments/blitz-layout-probe) captured three regions of a
//! running VS Code workbench (the activity bar, the explorer sidebar and the editor tab
//! strip): every
//! element's rect, and every text node's rect. It also found that blitz-dom lays out the
//! boxes exactly and that every error it makes comes from Parley's text widths. Text is
//! going to be measured by whoever draws it, so the box path is tested here the way it will
//! run: the fixtures are laid out through this crate with a [`TextMeasurer`] that answers
//! with the sizes VS Code itself measured, and every box is compared with the rect VS Code
//! drew. The tolerance: at least 95 percent of the compared elements within 1px, none more
//! than 4px off, in device pixels at scale 1 (the capture's window was at scale 1, so CSS
//! pixels are device pixels).
//!
//! Inputs, all committed:
//! - `experiments/blitz-layout-probe/fixtures/<region>.html`: the regions as static HTML,
//!   each element tagged with `data-probe-id`;
//! - `tests/fixtures/fr34/workbench.css`: the rules of the Code-OSS workbench stylesheet
//!   that can match those elements;
//! - `tests/fixtures/fr34/runtime-rebuilt.css`: what VS Code writes into the page at run
//!   time that can move a box here (icon glyphs and the sash size), rebuilt from Code-OSS;
//! - `tests/fixtures/fr34/geometry.txt`: the live rects and the text sizes.
//!
//! `tests/fixtures/fr34/extract.mjs` writes the last three and says where each part comes
//! from.

use std::collections::HashMap;

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus_compose_html::{
    BaseDocument, DisplayList, NodeEntry, Rect, TextLineHeight, TextMeasureRequest, TextMeasurer,
    TextMetrics, TextRun, layout_document,
};

const GEOMETRY: &str = include_str!("fixtures/fr34/geometry.txt");
const WORKBENCH_CSS: &str = include_str!("fixtures/fr34/workbench.css");
const RUNTIME_CSS: &str = include_str!("fixtures/fr34/runtime-rebuilt.css");

const ACTIVITYBAR_HTML: &str =
    include_str!("../../experiments/blitz-layout-probe/fixtures/activitybar.html");
const SIDEBAR_HTML: &str =
    include_str!("../../experiments/blitz-layout-probe/fixtures/sidebar.html");
const TABS_HTML: &str = include_str!("../../experiments/blitz-layout-probe/fixtures/tabs.html");

/// The window the capture was taken in, in CSS pixels, at a device pixel ratio of 1.
const WINDOW_WIDTH: u32 = 1440;
const WINDOW_HEIGHT: u32 = 900;

/// At least this share of the compared elements must be within [`CLOSE`] pixels.
const SHARE_WITHIN_CLOSE: f64 = 0.95;
const CLOSE: f32 = 1.0;
/// No compared element may be further off than this.
const WORST_ALLOWED: f32 = 4.0;

/// The Seti file icon font's advance, which the capture holds no measurement of: VS Code
/// drew its glyphs only as `::before` content in boxes that iconlabel.css sizes to 16px
/// wide and as tall as the line, so no rect in the capture depends on the glyph.
/// `fr34_box_tolerance_unmeasured_icon_glyphs_cannot_move_a_box` checks that this number
/// cannot move a box.
const SETI_ADVANCE_EM: f32 = 1.0;

/// Texts geometry.txt records as they stand in the DOM but that VS Code drew in capitals:
/// workbench.css sets `text-transform: uppercase` on the sidebar title's `h2` and on the
/// pane headers' `h3.title`. `Range.getClientRects` measured the capitals, and the layout
/// applies `text-transform` before it asks the measurer, so the measurer is asked about
/// `EXPLORER`, not `Explorer`.
const DRAWN_IN_CAPITALS: [&str; 4] = ["Explorer", "workspace", "Outline", "Timeline"];

// ------------------------------------------------------------------------------------------
// The captured data.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    /// Compared.
    Compare,
    /// The static fixture, laid out by VS Code's own Chromium, does not reproduce the live
    /// window here (the sidebar header's actions, shown live because the tree had focus), so
    /// a difference would be the fixture's and not the layout's.
    Excluded,
    /// `style`, `script`, `template`, `svg` and `path`, which the probe does not compare.
    Skip,
}

#[derive(Clone, Debug)]
struct LiveElement {
    probe_id: u32,
    status: Status,
    rect: Rect,
    label: String,
}

#[derive(Clone, Debug)]
struct Region {
    name: String,
    excluded: usize,
    elements: Vec<LiveElement>,
}

/// One text node as VS Code measured it.
#[derive(Clone, Copy, Debug)]
struct MeasuredText {
    /// The advance Chromium shaped.
    width: f32,
    /// The font's content area, rounded ascent plus rounded descent.
    height: f32,
    /// The part of `height` above the baseline.
    ascent: f32,
}

/// Font size in hundredths of a pixel and numeric weight, so the key can be hashed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TextKey {
    text: String,
    size: u32,
    weight: u32,
}

impl TextKey {
    fn new(text: &str, size: f32, weight: f32) -> Self {
        Self {
            text: text.to_string(),
            size: (size * 100.0).round() as u32,
            weight: weight.round() as u32,
        }
    }
}

struct Captured {
    regions: Vec<Region>,
    texts: HashMap<TextKey, MeasuredText>,
    /// Icon fonts, by family, with their advance in em.
    glyphs: HashMap<String, f32>,
}

fn number(field: Option<&str>, line: &str) -> f32 {
    field
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("geometry.txt: not a number in {line:?}"))
}

fn captured() -> Captured {
    let mut regions: Vec<Region> = Vec::new();
    let mut texts = HashMap::new();
    let mut glyphs = HashMap::new();
    for line in GEOMETRY.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut fields = line.split('\t');
        match fields.next() {
            Some("region") => regions.push(Region {
                name: fields.next().unwrap_or_default().to_string(),
                excluded: number(fields.next(), line) as usize,
                elements: Vec::new(),
            }),
            Some("el") => {
                let probe_id = number(fields.next(), line) as u32;
                let status = match fields.next() {
                    Some("compare") => Status::Compare,
                    Some("excluded") => Status::Excluded,
                    Some("skip") => Status::Skip,
                    other => panic!("geometry.txt: unknown status {other:?} in {line:?}"),
                };
                let rect = Rect::new(
                    number(fields.next(), line),
                    number(fields.next(), line),
                    number(fields.next(), line),
                    number(fields.next(), line),
                );
                let label = format!("{}#{probe_id}", fields.next().unwrap_or_default());
                regions
                    .last_mut()
                    .expect("geometry.txt lists elements after their region")
                    .elements
                    .push(LiveElement {
                        probe_id,
                        status,
                        rect,
                        label,
                    });
            }
            Some("text") => {
                let size = number(fields.next(), line);
                let weight = number(fields.next(), line);
                let measured = MeasuredText {
                    width: number(fields.next(), line),
                    height: number(fields.next(), line),
                    ascent: number(fields.next(), line),
                };
                let text = fields.next().unwrap_or_default();
                texts.insert(TextKey::new(text, size, weight), measured);
            }
            Some("glyph") => {
                let family = fields.next().unwrap_or_default().to_string();
                glyphs.insert(family, number(fields.next(), line));
            }
            other => panic!("geometry.txt: unknown record {other:?}"),
        }
    }
    Captured {
        regions,
        texts,
        glyphs,
    }
}

// ------------------------------------------------------------------------------------------
// The measurer.

/// Answers with the text sizes VS Code measured.
///
/// Every text node in the three regions sits in an element with `white-space: nowrap` or
/// `pre`, so VS Code laid each out on one line at its full width whatever room it had; this
/// answers the same for every width constraint. A text it has no measurement for is
/// recorded in [`CapturedMeasurer::unknown`] and answered with zero, and the tests fail on
/// it by name.
struct CapturedMeasurer {
    texts: HashMap<TextKey, MeasuredText>,
    /// Icon fonts whose glyphs VS Code drew only as `::before` content, by family, with
    /// their advance in em.
    glyphs: HashMap<String, f32>,
    /// Added to the width of a text, to show that the answer reaches the layout.
    width_errors: HashMap<String, f32>,
    /// Every request that was answered from the table, as (text, font size).
    answered: Vec<(String, f32)>,
    /// Every request the table had no answer for.
    unknown: Vec<String>,
}

impl CapturedMeasurer {
    fn new(captured: &Captured) -> Self {
        let mut glyphs = captured.glyphs.clone();
        glyphs.insert("seti".to_string(), SETI_ADVANCE_EM);
        // The table is keyed by the text VS Code measured, which for these is the capitals
        // it drew; the source text is no longer an answer the layout can get.
        let texts = captured
            .texts
            .iter()
            .map(|(key, measured)| {
                let mut key = key.clone();
                if DRAWN_IN_CAPITALS.contains(&key.text.as_str()) {
                    key.text = key.text.to_uppercase();
                }
                (key, *measured)
            })
            .collect();
        Self {
            texts,
            glyphs,
            width_errors: HashMap::new(),
            answered: Vec::new(),
            unknown: Vec::new(),
        }
    }

    fn lookup(&self, text: &str, size: f32, weight: f32) -> Option<MeasuredText> {
        self.texts.get(&TextKey::new(text, size, weight)).copied()
    }

    /// Where Chromium puts the rect of an inline element around this run: the font's content
    /// area around the baseline, which is narrower than the line box the run occupies.
    fn content_area(&self, run: &TextRun) -> Rect {
        match self.lookup(&run.text, run.style.font_size, run.style.font_weight) {
            Some(measured) => Rect::new(
                run.rect.x,
                run.rect.y + run.baseline - measured.ascent,
                run.rect.width,
                measured.height,
            ),
            None => run.rect,
        }
    }

    #[track_caller]
    fn assert_nothing_unknown(&self, region: &str) {
        assert!(
            self.unknown.is_empty(),
            "{region}: the layout asked for text VS Code never measured, so the comparison \
             would rest on made-up sizes:\n  {}",
            self.unknown.join("\n  ")
        );
    }
}

impl TextMeasurer for CapturedMeasurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let style = request.style;
        let line_height = match style.line_height {
            TextLineHeight::Px(px) => Some(px),
            TextLineHeight::Normal => None,
        };

        let family = style.font_family.first().map(String::as_str).unwrap_or("");
        if let Some(&em) = self.glyphs.get(family) {
            let height = line_height.unwrap_or(style.font_size);
            return TextMetrics {
                width: request.text.chars().count() as f32 * em * style.font_size,
                height,
                // The glyph's em box centred in the line, sitting on its baseline.
                first_baseline: (height + style.font_size) / 2.0,
                line_count: 1,
            };
        }

        let Some(measured) = self.lookup(request.text, style.font_size, style.font_weight) else {
            self.unknown.push(format!(
                "{:?} at {}px, weight {}, family {:?}",
                request.text, style.font_size, style.font_weight, style.font_family
            ));
            return TextMetrics::default();
        };
        self.answered
            .push((request.text.to_string(), style.font_size));
        let width = measured.width + self.width_errors.get(request.text).copied().unwrap_or(0.0);
        let (height, first_baseline) = match line_height {
            // Chromium splits the leading in two and puts the floor of the half above the
            // text: a 16px tall text in a 35px line has its content area 9px down.
            Some(line) => (
                line,
                ((line - measured.height) / 2.0).floor() + measured.ascent,
            ),
            None => (measured.height, measured.ascent),
        };
        TextMetrics {
            width,
            height,
            first_baseline,
            line_count: 1,
        }
    }
}

// ------------------------------------------------------------------------------------------
// Laying a region out.

fn fixture_html(region: &str) -> &'static str {
    match region {
        "activitybar" => ACTIVITYBAR_HTML,
        "sidebar" => SIDEBAR_HTML,
        "tabs" => TABS_HTML,
        other => panic!("no fixture for region {other}"),
    }
}

/// The fixture with its two `<link rel="stylesheet">` elements (the rebuilt workbench
/// stylesheet in `.scratch/` and the run-time stylesheet, neither committed) replaced by the
/// committed CSS, in the same order.
fn fixture_with_committed_css(region: &str) -> String {
    let html = fixture_html(region);
    let mut out = String::with_capacity(html.len() + WORKBENCH_CSS.len() + RUNTIME_CSS.len());
    let mut links = 0;
    for line in html.lines() {
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
        "{region}.html links the workbench and run-time stylesheets"
    );
    out
}

fn lay_out(region: &str, measurer: &mut CapturedMeasurer) -> (BaseDocument, DisplayList) {
    let config = DocumentConfig {
        viewport: Some(Viewport::new(
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            1.0,
            ColorScheme::Dark,
        )),
        ..DocumentConfig::default()
    };
    let html = fixture_with_committed_css(region);
    let mut doc = HtmlDocument::from_html(&html, config).into_inner();
    let list = layout_document(&mut doc, measurer);
    (doc, list)
}

/// Every `data-probe-id` element's rect as this crate laid it out, the way
/// `getBoundingClientRect` reports it: the border box for an element with a box of its own;
/// for an inline element without one (a `<span>` in a line of text), the union of the
/// content areas of the text inside it; nothing at all, (0, 0, 0, 0), for an element that is
/// not rendered.
fn computed_rects(
    doc: &BaseDocument,
    list: &DisplayList,
    measurer: &CapturedMeasurer,
) -> HashMap<u32, Rect> {
    // An anonymous block is listed under one of the nodes it wraps, which has no box.
    let boxes: HashMap<usize, &NodeEntry> = list
        .entries
        .iter()
        .filter(|entry| !entry.tag.is_empty())
        .map(|entry| (entry.node, entry))
        .collect();

    let mut inline: HashMap<usize, Rect> = HashMap::new();
    for entry in &list.entries {
        for run in &entry.texts {
            let area = measurer.content_area(run);
            // The run's owner and every ancestor up to the element whose box holds the line.
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
    for (id, node) in doc.tree().iter() {
        let Some(probe_id) = node
            .attrs()
            .and_then(|attrs| {
                attrs
                    .iter()
                    .find(|attr| &*attr.name.local == "data-probe-id")
            })
            .and_then(|attr| attr.value.parse::<u32>().ok())
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

// ------------------------------------------------------------------------------------------
// Comparing.

struct Row {
    label: String,
    live: Rect,
    computed: Rect,
    error: f32,
}

struct Comparison {
    region: String,
    rows: Vec<Row>,
    hidden_in_both: usize,
}

impl Comparison {
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
            && self.within(CLOSE) as f64 >= SHARE_WITHIN_CLOSE * self.rows.len() as f64
            && self.worst() <= WORST_ALLOWED
    }

    fn summary(&self) -> String {
        let compared = self.rows.len();
        let close = self.within(CLOSE);
        format!(
            "{}: {compared} elements compared ({} rendered in neither), {close} within {CLOSE}px \
             ({:.1}%), {} beyond {WORST_ALLOWED}px, worst {:.2}px",
            self.region,
            self.hidden_in_both,
            100.0 * close as f64 / compared.max(1) as f64,
            compared - self.within(WORST_ALLOWED),
            self.worst()
        )
    }

    fn worst_rows(&self, count: usize) -> String {
        let mut rows: Vec<&Row> = self.rows.iter().filter(|row| row.error > CLOSE).collect();
        rows.sort_by(|a, b| b.error.total_cmp(&a.error));
        let show = |rect: &Rect| {
            format!(
                "({:.2}, {:.2}, {:.2}, {:.2})",
                rect.x, rect.y, rect.width, rect.height
            )
        };
        rows.iter()
            .take(count)
            .map(|row| {
                format!(
                    "    {:.2}px  {}  VS Code {}  computed {}  dx {:.2} dy {:.2} dw {:.2} dh {:.2}",
                    row.error,
                    row.label,
                    show(&row.live),
                    show(&row.computed),
                    row.computed.x - row.live.x,
                    row.computed.y - row.live.y,
                    row.computed.width - row.live.width,
                    row.computed.height - row.live.height,
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Matches elements by `data-probe-id`, as experiments/blitz-layout-probe/scripts/compare.mjs
/// does: elements the probe skips or excludes are left out, and an element neither side
/// renders is counted apart. An element only one side renders is compared, with the other
/// side's rect at (0, 0, 0, 0).
fn compare(region: &Region, computed: &HashMap<u32, Rect>) -> Comparison {
    let mut rows = Vec::new();
    let mut hidden_in_both = 0;
    for element in &region.elements {
        if element.status != Status::Compare {
            continue;
        }
        let rect = *computed.get(&element.probe_id).unwrap_or_else(|| {
            panic!(
                "{}: {} is in the capture but not in the parsed fixture",
                region.name, element.label
            )
        });
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
        rows.push(Row {
            label: element.label.clone(),
            live: element.rect,
            computed: rect,
            error,
        });
    }
    Comparison {
        region: region.name.clone(),
        rows,
        hidden_in_both,
    }
}

fn region<'a>(captured: &'a Captured, name: &str) -> &'a Region {
    captured
        .regions
        .iter()
        .find(|region| region.name == name)
        .unwrap_or_else(|| panic!("geometry.txt has no region {name}"))
}

// ------------------------------------------------------------------------------------------
// Tests.

/// The activity bar, the explorer sidebar and the editor tab strip, laid out with the text
/// sizes VS Code measured, meet the box tolerance: at least 95 percent of the elements in
/// each region within 1px of where VS Code drew them, and none more than 4px off.
#[test]
fn fr34_boxes_meet_the_tolerance_with_text_sized_as_vs_code_measured() {
    let captured = captured();
    assert_eq!(
        captured
            .regions
            .iter()
            .map(|region| (region.name.as_str(), region.excluded))
            .collect::<Vec<_>>(),
        vec![("activitybar", 0), ("sidebar", 12), ("tabs", 0)],
        "the probe excludes the twelve sidebar header elements its static fixture does not \
         reproduce, and nothing else"
    );

    let mut comparisons = Vec::new();
    for region in &captured.regions {
        let mut measurer = CapturedMeasurer::new(&captured);
        let (doc, list) = lay_out(&region.name, &mut measurer);
        measurer.assert_nothing_unknown(&region.name);
        let computed = computed_rects(&doc, &list, &measurer);
        comparisons.push(compare(region, &computed));
    }

    let summary = comparisons
        .iter()
        .map(Comparison::summary)
        .collect::<Vec<_>>()
        .join("\n");
    // Printed on success too: these are the numbers the requirement records.
    eprintln!("{summary}");
    let failures = comparisons
        .iter()
        .filter(|comparison| !comparison.meets_tolerance())
        .map(|comparison| {
            format!(
                "{}\n  worst elements:\n{}",
                comparison.summary(),
                comparison.worst_rows(15)
            )
        })
        .collect::<Vec<_>>();
    assert!(
        failures.is_empty(),
        "boxes outside the tolerance (at least {:.0}% within {CLOSE}px, none beyond \
         {WORST_ALLOWED}px):\n{summary}\n\n{}",
        SHARE_WITHIN_CLOSE * 100.0,
        failures.join("\n\n")
    );
}

/// The measurer's answer is what sizes the text: README.md 20px wider than VS Code
/// measured makes the first tab 20px wider and moves the second tab 20px to the right.
#[test]
fn fr34_box_tolerance_measurer_answers_reach_the_layout() {
    let captured = captured();
    let tabs = region(&captured, "tabs");
    let label = |probe_id: u32| {
        &tabs
            .elements
            .iter()
            .find(|element| element.probe_id == probe_id)
            .expect("the tab strip's elements are in the capture")
            .label
    };
    // #4 is the README.md tab and #18 the index.ts tab after it.
    assert!(label(4).starts_with("div.tab."), "{}", label(4));
    assert!(label(18).starts_with("div.tab."), "{}", label(18));

    let mut honest = CapturedMeasurer::new(&captured);
    let (doc, list) = lay_out("tabs", &mut honest);
    honest.assert_nothing_unknown("tabs");
    assert!(
        honest
            .answered
            .iter()
            .any(|(text, size)| text == "README.md" && *size == 13.0),
        "the layout asked for README.md at 13px: {:?}",
        honest.answered
    );
    let before = computed_rects(&doc, &list, &honest);

    let mut wrong = CapturedMeasurer::new(&captured);
    wrong.width_errors.insert("README.md".to_string(), 20.0);
    let (doc, list) = lay_out("tabs", &mut wrong);
    wrong.assert_nothing_unknown("tabs");
    let after = computed_rects(&doc, &list, &wrong);

    let moved = |probe_id: u32| {
        let (a, b) = (before[&probe_id], after[&probe_id]);
        (b.x - a.x, b.width - a.width)
    };
    let (dx, dw) = moved(4);
    assert!(
        dx.abs() < 0.01 && (dw - 20.0).abs() < 0.01,
        "the README.md tab grew by {dw} and moved by {dx}"
    );
    let (dx, dw) = moved(18);
    assert!(
        (dx - 20.0).abs() < 0.01 && dw.abs() < 0.01,
        "the index.ts tab moved by {dx} and grew by {dw}"
    );
}

/// The Seti file icon glyphs are the one size the capture does not hold. Doubling their
/// advance moves no compared box, so the number chosen for it cannot decide the result.
#[test]
fn fr34_box_tolerance_unmeasured_icon_glyphs_cannot_move_a_box() {
    let captured = captured();
    for name in ["sidebar", "tabs"] {
        let region = region(&captured, name);
        let rects = |advance: f32| {
            let mut measurer = CapturedMeasurer::new(&captured);
            measurer.glyphs.insert("seti".to_string(), advance);
            let (doc, list) = lay_out(name, &mut measurer);
            measurer.assert_nothing_unknown(name);
            computed_rects(&doc, &list, &measurer)
        };
        let (one, two) = (rects(SETI_ADVANCE_EM), rects(2.0 * SETI_ADVANCE_EM));
        let moved: Vec<String> = region
            .elements
            .iter()
            .filter(|element| element.status == Status::Compare)
            .filter(|element| one[&element.probe_id] != two[&element.probe_id])
            .map(|element| {
                format!(
                    "{}: {:?} -> {:?}",
                    element.label, one[&element.probe_id], two[&element.probe_id]
                )
            })
            .collect();
        assert!(
            moved.is_empty(),
            "{name}: the Seti glyph advance moved boxes:\n  {}",
            moved.join("\n  ")
        );
    }
}
