//! The VS Code workbench fixtures and the measurer that answers with VS Code's own text
//! sizes, shared by the box tolerance test and the frame benchmarks.
//!
//! The blitz-layout probe (experiments/blitz-layout-probe, on the develop branch) captured three regions of a
//! running VS Code workbench (the activity bar, the explorer sidebar and the editor tab
//! strip): every element's rect, and every text node's rect. Inputs, all committed:
//! - `tests/fixtures/fr34/<region>.html`: the regions as static HTML, each element tagged
//!   with `data-probe-id`. They are copies of the probe's `fixtures/<region>.html`, kept here
//!   because experiments/ is not on the published branch and these tests are;
//! - `tests/fixtures/fr34/workbench.css`: the rules of the Code-OSS workbench stylesheet
//!   that can match those elements;
//! - `tests/fixtures/fr34/runtime-rebuilt.css`: what VS Code writes into the page at run
//!   time that can move a box here (icon glyphs and the sash size), rebuilt from Code-OSS;
//! - `tests/fixtures/fr34/geometry.txt`: the live rects and the text sizes.
//!
//! `tests/fixtures/fr34/extract.mjs` writes the last three and says where each part comes
//! from.
//!
//! This lives under `tests/` and not in the crate because it embeds about 120KB of HTML and
//! CSS with `include_str!`; in the library that would ship in every application that links
//! it. The benchmarks include this file with `#[path]`.

// Each test or bench target that includes this module uses a different part of it.
#![allow(dead_code)]

use std::collections::HashMap;

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus_compose::html::{
    BaseDocument, DisplayList, NodeId, Rect, TextLineHeight, TextMeasureRequest, TextMeasurer,
    TextMetrics, TextRun, layout_document,
};

const GEOMETRY: &str = include_str!("../fixtures/fr34/geometry.txt");
const WORKBENCH_CSS: &str = include_str!("../fixtures/fr34/workbench.css");
const RUNTIME_CSS: &str = include_str!("../fixtures/fr34/runtime-rebuilt.css");

const ACTIVITYBAR_HTML: &str = include_str!("../fixtures/fr34/activitybar.html");
const SIDEBAR_HTML: &str = include_str!("../fixtures/fr34/sidebar.html");
const TABS_HTML: &str = include_str!("../fixtures/fr34/tabs.html");

/// The three captured regions, by the name geometry.txt and the fixture files use.
pub const REGIONS: [&str; 3] = ["activitybar", "sidebar", "tabs"];

/// The window the capture was taken in, in CSS pixels, at a device pixel ratio of 1.
pub const WINDOW_WIDTH: u32 = 1440;
pub const WINDOW_HEIGHT: u32 = 900;

/// The Seti file icon font's advance, which the capture holds no measurement of: VS Code
/// drew its glyphs only as `::before` content in boxes that iconlabel.css sizes to 16px
/// wide and as tall as the line, so no rect in the capture depends on the glyph.
/// `fr34_box_tolerance_unmeasured_icon_glyphs_cannot_move_a_box` checks that this number
/// cannot move a box.
pub const SETI_ADVANCE_EM: f32 = 1.0;

/// Texts geometry.txt records as they stand in the DOM but that VS Code drew in capitals:
/// workbench.css sets `text-transform: uppercase` on the sidebar title's `h2` and on the
/// pane headers' `h3.title`. `Range.getClientRects` measured the capitals, and the layout
/// applies `text-transform` before it asks the measurer, so the measurer is asked about
/// `EXPLORER`, not `Explorer`.
const DRAWN_IN_CAPITALS: [&str; 4] = ["Explorer", "workspace", "Outline", "Timeline"];

// ------------------------------------------------------------------------------------------
// The captured data.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
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
pub struct LiveElement {
    pub probe_id: u32,
    pub status: Status,
    pub rect: Rect,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct Region {
    pub name: String,
    pub excluded: usize,
    pub elements: Vec<LiveElement>,
}

/// One text node as VS Code measured it.
#[derive(Clone, Copy, Debug)]
pub struct MeasuredText {
    /// The advance Chromium shaped.
    pub width: f32,
    /// The font's content area, rounded ascent plus rounded descent.
    pub height: f32,
    /// The part of `height` above the baseline.
    pub ascent: f32,
}

/// Font size in hundredths of a pixel and numeric weight, so sizes compare exactly.
fn size_key(size: f32, weight: f32) -> (u32, u32) {
    ((size * 100.0).round() as u32, weight.round() as u32)
}

/// Text sizes by text, font size and weight.
///
/// Keyed by the text first so that a lookup borrows the `&str` it is asked about: the
/// measurer runs for every text on every layout, and building an owned key per call would
/// make the benchmarks measure this table's allocations instead of the layout.
#[derive(Clone, Debug, Default)]
pub struct TextTable {
    by_text: HashMap<String, Vec<(SizeAndWeight, MeasuredText)>>,
}

/// A font size and weight as whole numbers, so they can be compared exactly.
type SizeAndWeight = (u32, u32);

impl TextTable {
    /// Records a size; a later record for the same text, size and weight replaces it.
    fn insert(&mut self, text: &str, key: (u32, u32), measured: MeasuredText) {
        let sizes = self.by_text.entry(text.to_string()).or_default();
        match sizes.iter_mut().find(|(at, _)| *at == key) {
            Some((_, existing)) => *existing = measured,
            None => sizes.push((key, measured)),
        }
    }

    pub fn get(&self, text: &str, size: f32, weight: f32) -> Option<MeasuredText> {
        let key = size_key(size, weight);
        self.by_text
            .get(text)?
            .iter()
            .find(|(at, _)| *at == key)
            .map(|(_, measured)| *measured)
    }
}

pub struct Captured {
    pub regions: Vec<Region>,
    pub texts: TextTable,
    /// Icon fonts, by family, with their advance in em.
    pub glyphs: HashMap<String, f32>,
}

fn number(field: Option<&str>, line: &str) -> f32 {
    field
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("geometry.txt: not a number in {line:?}"))
}

pub fn captured() -> Captured {
    let mut regions: Vec<Region> = Vec::new();
    let mut texts = TextTable::default();
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
                texts.insert(text, size_key(size, weight), measured);
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

pub fn region<'a>(captured: &'a Captured, name: &str) -> &'a Region {
    captured
        .regions
        .iter()
        .find(|region| region.name == name)
        .unwrap_or_else(|| panic!("geometry.txt has no region {name}"))
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
///
/// A measurement it has is answered without allocating, unless it was built with
/// [`CapturedMeasurer::new`], which also records every answer in
/// [`CapturedMeasurer::answered`] for the tests that check what the layout asked.
pub struct CapturedMeasurer {
    texts: TextTable,
    /// Icon fonts whose glyphs VS Code drew only as `::before` content, by family, with
    /// their advance in em.
    pub glyphs: HashMap<String, f32>,
    /// Added to the width of a text, to show that the answer reaches the layout.
    pub width_errors: HashMap<String, f32>,
    /// Whether answers are recorded in `answered`.
    record_answers: bool,
    /// Every request that was answered from the table, as (text, font size).
    pub answered: Vec<(String, f32)>,
    /// Every request the table had no answer for.
    pub unknown: Vec<String>,
}

impl CapturedMeasurer {
    /// A measurer that records every answer it gives.
    pub fn new(captured: &Captured) -> Self {
        let mut glyphs = captured.glyphs.clone();
        glyphs.insert("seti".to_string(), SETI_ADVANCE_EM);
        // The table is keyed by the text VS Code measured, which for these is the capitals
        // it drew; the source text is no longer an answer the layout can get.
        let mut texts = TextTable::default();
        for (text, sizes) in &captured.texts.by_text {
            let drawn = if DRAWN_IN_CAPITALS.contains(&text.as_str()) {
                text.to_uppercase()
            } else {
                text.clone()
            };
            for (key, measured) in sizes {
                texts.insert(&drawn, *key, *measured);
            }
        }
        Self {
            texts,
            glyphs,
            width_errors: HashMap::new(),
            record_answers: true,
            answered: Vec::new(),
            unknown: Vec::new(),
        }
    }

    /// A measurer that keeps no record of its answers, so a layout that asks for text it
    /// knows allocates nothing in here: what a benchmark needs, where `answered` would
    /// otherwise grow on every frame.
    pub fn without_recording(captured: &Captured) -> Self {
        Self {
            record_answers: false,
            ..Self::new(captured)
        }
    }

    pub fn lookup(&self, text: &str, size: f32, weight: f32) -> Option<MeasuredText> {
        self.texts.get(text, size, weight)
    }

    /// Where Chromium puts the rect of an inline element around this run: the font's content
    /// area around the baseline, which is narrower than the line box the run occupies.
    pub fn content_area(&self, run: &TextRun) -> Rect {
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
    pub fn assert_nothing_unknown(&self, region: &str) {
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
        if self.record_answers {
            self.answered
                .push((request.text.to_string(), style.font_size));
        }
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
// Loading a region.

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
pub fn fixture_with_committed_css(region: &str) -> String {
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

/// The region parsed into a document with the capture's viewport, not yet laid out.
pub fn parse_region(region: &str) -> BaseDocument {
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
    HtmlDocument::from_html(&html, config).into_inner()
}

/// The region parsed and laid out once.
pub fn lay_out(region: &str, measurer: &mut CapturedMeasurer) -> (BaseDocument, DisplayList) {
    let mut doc = parse_region(region);
    let list = layout_document(&mut doc, measurer);
    (doc, list)
}

/// The element whose `data-probe-id` is `probe_id`.
pub fn node_by_probe_id(doc: &BaseDocument, probe_id: u32) -> NodeId {
    for (id, node) in doc.tree().iter() {
        let Some(attrs) = node.attrs() else {
            continue;
        };
        let tagged = attrs.iter().any(|attr| {
            let probe = attr.value.parse::<u32>().ok();
            &*attr.name.local == "data-probe-id" && probe == Some(probe_id)
        });
        if tagged {
            return id;
        }
    }
    panic!("no element in the fixture has data-probe-id {probe_id}")
}
