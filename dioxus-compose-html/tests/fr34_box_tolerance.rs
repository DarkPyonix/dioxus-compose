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
//! The fixtures, the captured geometry and the measurer are in `tests/support/`, which says
//! where each comes from; the frame benchmarks lay out the same fixtures.
//!
//! [`TextMeasurer`]: dioxus_compose_html::TextMeasurer

mod support;

use std::collections::HashMap;

use dioxus_compose_html::{BaseDocument, DisplayList, NodeEntry, Rect};

use support::{CapturedMeasurer, Region, SETI_ADVANCE_EM, Status, captured, lay_out, region};

/// At least this share of the compared elements must be within [`CLOSE`] pixels.
const SHARE_WITHIN_CLOSE: f64 = 0.95;
const CLOSE: f32 = 1.0;
/// No compared element may be further off than this.
const WORST_ALLOWED: f32 = 4.0;

// ------------------------------------------------------------------------------------------
// Where this crate put each element.

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
