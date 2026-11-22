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
//! [`TextMeasurer`]: dioxus_compose::html::TextMeasurer

mod support;

use std::collections::HashMap;

use dioxus_compose::html::{BaseDocument, DisplayList, NodeEntry, Rect};

use support::{
    CapturedMeasurer, Region, SETI_ADVANCE_EM, Status, captured, lay_out, lay_out_html,
    parse_captured, region,
};

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

/// Lays out a workbench region fixture with the measured text table in `geometry`, compares
/// every `data-probe-id` element against its reference rect, and asserts the same tolerance
/// as the captured workbench regions: at least 95 percent within 1px and none beyond 4px.
fn assert_workbench_region_tolerance(region_name: &str, html: &str, geometry: &str) {
    let captured = parse_captured(geometry);
    let expected = region(&captured, region_name);
    let mut measurer = CapturedMeasurer::new(&captured);
    let (doc, list) = lay_out_html(html, &mut measurer);
    measurer.assert_nothing_unknown(region_name);
    let computed = computed_rects(&doc, &list, &measurer);
    let comparison = compare(expected, &computed);
    let summary = comparison.summary();
    eprintln!("{summary}");
    assert!(
        comparison.meets_tolerance(),
        "boxes outside the tolerance (at least {:.0}% within {CLOSE}px, none beyond \
         {WORST_ALLOWED}px):\n{summary}\n  worst elements:\n{}",
        SHARE_WITHIN_CLOSE * 100.0,
        comparison.worst_rows(15)
    );
}

#[test]
fn fr34_5_box_tolerance_panel() {
    let html = r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><style>
*, *::before, *::after { box-sizing: border-box; }
html, body { margin: 0; padding: 0; font-family: -apple-system, sans-serif; font-size: 13px; }
.part.panel {
    position: absolute; left: 0; top: 618px; width: 1440px; height: 260px;
    display: flex; flex-direction: column; border-top: 1px solid #2a2b2c;
}
.composite.title {
    height: 36px; display: flex; align-items: center; justify-content: space-between;
    padding: 0 12px;
}
.panel-tabs { display: flex; align-items: center; gap: 16px; height: 36px; }
.panel-tab {
    display: flex; align-items: center; height: 36px; line-height: 36px;
    font-size: 11px; font-weight: 400; white-space: nowrap;
}
.title-actions { display: flex; align-items: center; gap: 4px; height: 22px; }
.action-btn { width: 22px; height: 22px; }
.panel-body { flex: 1; padding: 8px 12px; display: flex; flex-direction: column; gap: 4px; }
.problem-row { height: 22px; display: flex; align-items: center; gap: 8px; }
.severity-icon { width: 16px; height: 16px; }
.problem-msg { height: 22px; line-height: 22px; font-size: 13px; white-space: nowrap; }
.problem-loc { height: 22px; line-height: 22px; font-size: 12px; white-space: nowrap; }
</style></head><body>
<div class="part panel" data-probe-id="0">
<div class="composite title" data-probe-id="1">
<div class="panel-tabs" data-probe-id="2">
<div class="panel-tab active" data-probe-id="3"><span class="tab-label" data-probe-id="4">PROBLEMS</span></div>
<div class="panel-tab" data-probe-id="5"><span class="tab-label" data-probe-id="6">OUTPUT</span></div>
<div class="panel-tab" data-probe-id="7"><span class="tab-label" data-probe-id="8">TERMINAL</span></div>
</div>
<div class="title-actions" data-probe-id="9">
<div class="action-btn" data-probe-id="10"></div>
<div class="action-btn" data-probe-id="11"></div>
</div>
</div>
<div class="panel-body" data-probe-id="12">
<div class="problem-row" data-probe-id="13">
<div class="severity-icon" data-probe-id="14"></div>
<div class="problem-msg" data-probe-id="15"><span class="msg-text" data-probe-id="16">unused variable x</span></div>
<div class="problem-loc" data-probe-id="17">main.rs:14</div>
</div>
<div class="problem-row" data-probe-id="18">
<div class="severity-icon" data-probe-id="19"></div>
<div class="problem-msg" data-probe-id="20">missing semicolon</div>
<div class="problem-loc" data-probe-id="21">lib.rs:42</div>
</div>
</div>
</div>
</body></html>"#;

    let geometry = "\
region\tpanel\t0
el\t0\tcompare\t0\t618\t1440\t260\tdiv.part.panel
el\t1\tcompare\t0\t619\t1440\t36\tdiv.composite.title
el\t2\tcompare\t12\t619\t196\t36\tdiv.panel-tabs
el\t3\tcompare\t12\t619\t60\t36\tdiv.panel-tab.active
el\t4\tcompare\t12\t630\t60\t14\tspan.tab-label
el\t5\tcompare\t88\t619\t46\t36\tdiv.panel-tab
el\t6\tcompare\t88\t630\t46\t14\tspan.tab-label
el\t7\tcompare\t150\t619\t58\t36\tdiv.panel-tab
el\t8\tcompare\t150\t630\t58\t14\tspan.tab-label
el\t9\tcompare\t1380\t626\t48\t22\tdiv.title-actions
el\t10\tcompare\t1380\t626\t22\t22\tdiv.action-btn
el\t11\tcompare\t1406\t626\t22\t22\tdiv.action-btn
el\t12\tcompare\t0\t655\t1440\t223\tdiv.panel-body
el\t13\tcompare\t12\t663\t1416\t22\tdiv.problem-row
el\t14\tcompare\t12\t666\t16\t16\tdiv.severity-icon
el\t15\tcompare\t36\t663\t110\t22\tdiv.problem-msg
el\t16\tcompare\t36\t666\t110\t16\tspan.msg-text
el\t17\tcompare\t154\t663\t62\t22\tdiv.problem-loc
el\t18\tcompare\t12\t689\t1416\t22\tdiv.problem-row
el\t19\tcompare\t12\t692\t16\t16\tdiv.severity-icon
el\t20\tcompare\t36\t689\t114\t22\tdiv.problem-msg
el\t21\tcompare\t158\t689\t50\t22\tdiv.problem-loc
text\t11\t400\t60\t14\t11\tPROBLEMS
text\t11\t400\t46\t14\t11\tOUTPUT
text\t11\t400\t58\t14\t11\tTERMINAL
text\t13\t400\t110\t16\t13\tunused variable x
text\t12\t400\t62\t14\t11\tmain.rs:14
text\t13\t400\t114\t16\t13\tmissing semicolon
text\t12\t400\t50\t14\t11\tlib.rs:42
";

    assert_workbench_region_tolerance("panel", html, geometry);
}

#[test]
fn fr34_5_box_tolerance_status_bar() {
    let html = r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><style>
*, *::before, *::after { box-sizing: border-box; }
html, body { margin: 0; padding: 0; font-family: -apple-system, sans-serif; font-size: 12px; }
.part.statusbar {
    position: absolute; left: 0; top: 878px; width: 1440px; height: 22px;
    display: flex; justify-content: space-between; align-items: center; padding: 0 8px;
}
.left-items, .right-items { display: flex; align-items: center; gap: 4px; height: 22px; }
.statusbar-item {
    display: flex; align-items: center; padding: 0 6px; height: 22px;
    line-height: 22px; font-size: 12px; white-space: nowrap;
}
</style></head><body>
<footer class="part statusbar" data-probe-id="0">
<div class="left-items" data-probe-id="1">
<div class="statusbar-item" data-probe-id="2"><span class="item-label" data-probe-id="3">main*</span></div>
<div class="statusbar-item" data-probe-id="4"><span class="item-label" data-probe-id="5">0 0</span></div>
</div>
<div class="right-items" data-probe-id="6">
<div class="statusbar-item" data-probe-id="7">Ln 14, Col 8</div>
<div class="statusbar-item" data-probe-id="8">Spaces: 4</div>
<div class="statusbar-item" data-probe-id="9">UTF-8</div>
<div class="statusbar-item" data-probe-id="10">Rust</div>
</div>
</footer>
</body></html>"#;

    let geometry = "\
region\tstatus_bar\t0
el\t0\tcompare\t0\t878\t1440\t22\tfooter.part.statusbar
el\t1\tcompare\t8\t878\t88\t22\tdiv.left-items
el\t2\tcompare\t8\t878\t48\t22\tdiv.statusbar-item
el\t3\tcompare\t14\t882\t36\t14\tspan.item-label
el\t4\tcompare\t60\t878\t36\t22\tdiv.statusbar-item
el\t5\tcompare\t66\t882\t24\t14\tspan.item-label
el\t6\tcompare\t1176\t878\t256\t22\tdiv.right-items
el\t7\tcompare\t1176\t878\t84\t22\tdiv.statusbar-item
el\t8\tcompare\t1264\t878\t70\t22\tdiv.statusbar-item
el\t9\tcompare\t1338\t878\t50\t22\tdiv.statusbar-item
el\t10\tcompare\t1392\t878\t40\t22\tdiv.statusbar-item
text\t12\t400\t36\t14\t11\tmain*
text\t12\t400\t24\t14\t11\t0 0
text\t12\t400\t72\t14\t11\tLn 14, Col 8
text\t12\t400\t58\t14\t11\tSpaces: 4
text\t12\t400\t38\t14\t11\tUTF-8
text\t12\t400\t28\t14\t11\tRust
";

    assert_workbench_region_tolerance("status_bar", html, geometry);
}

#[test]
fn fr34_5_box_tolerance_quick_input() {
    let html = r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><style>
*, *::before, *::after { box-sizing: border-box; }
html, body { margin: 0; padding: 0; font-family: -apple-system, sans-serif; font-size: 13px; }
.quick-input-widget {
    position: absolute; left: 420px; top: 36px; width: 600px;
    border: 1px solid #3c3c3c; display: flex; flex-direction: column;
}
.quick-input-header { padding: 6px; height: 38px; display: flex; align-items: center; }
.quick-input-box {
    width: 586px; height: 26px; border: 1px solid #007fd4; padding: 0 6px;
    display: flex; align-items: center; line-height: 24px; font-size: 13px; white-space: nowrap;
}
.quick-input-list { padding: 4px 6px 6px 6px; display: flex; flex-direction: column; }
.quick-input-row {
    height: 22px; padding: 0 6px; display: flex; align-items: center;
    justify-content: space-between;
}
.row-label { height: 22px; line-height: 22px; font-size: 13px; white-space: nowrap; }
.row-keybinding {
    height: 18px; line-height: 18px; padding: 0 4px; font-size: 11px; white-space: nowrap;
}
</style></head><body>
<div class="quick-input-widget" data-probe-id="0">
<div class="quick-input-header" data-probe-id="1">
<div class="quick-input-box" data-probe-id="2"><span class="quick-input-text" data-probe-id="3">&gt;View: Toggle Terminal</span></div>
</div>
<div class="quick-input-list" data-probe-id="4">
<div class="quick-input-row focused" data-probe-id="5">
<div class="row-label" data-probe-id="6">View: Toggle Terminal</div>
<div class="row-keybinding" data-probe-id="7">Ctrl+`</div>
</div>
<div class="quick-input-row" data-probe-id="8">
<div class="row-label" data-probe-id="9">File: Open Folder</div>
<div class="row-keybinding" data-probe-id="10">Cmd+O</div>
</div>
<div class="quick-input-row" data-probe-id="11">
<div class="row-label" data-probe-id="12">Preferences: Color Theme</div>
</div>
</div>
</div>
</body></html>"#;

    let geometry = "\
region\tquick_input\t0
el\t0\tcompare\t420\t36\t600\t116\tdiv.quick-input-widget
el\t1\tcompare\t421\t37\t598\t38\tdiv.quick-input-header
el\t2\tcompare\t427\t43\t586\t26\tdiv.quick-input-box
el\t3\tcompare\t434\t48\t148\t16\tspan.quick-input-text
el\t4\tcompare\t421\t75\t598\t76\tdiv.quick-input-list
el\t5\tcompare\t427\t79\t586\t22\tdiv.quick-input-row.focused
el\t6\tcompare\t433\t79\t140\t22\tdiv.row-label
el\t7\tcompare\t963\t81\t44\t18\tdiv.row-keybinding
el\t8\tcompare\t427\t101\t586\t22\tdiv.quick-input-row
el\t9\tcompare\t433\t101\t108\t22\tdiv.row-label
el\t10\tcompare\t959\t103\t48\t18\tdiv.row-keybinding
el\t11\tcompare\t427\t123\t586\t22\tdiv.quick-input-row
el\t12\tcompare\t433\t123\t156\t22\tdiv.row-label
text\t13\t400\t148\t16\t13\t>View: Toggle Terminal
text\t13\t400\t140\t16\t13\tView: Toggle Terminal
text\t11\t400\t36\t12\t10\tCtrl+`
text\t13\t400\t108\t16\t13\tFile: Open Folder
text\t11\t400\t40\t12\t10\tCmd+O
text\t13\t400\t156\t16\t13\tPreferences: Color Theme
";

    assert_workbench_region_tolerance("quick_input", html, geometry);
}

#[test]
fn fr34_5_box_tolerance_context_menu() {
    let html = r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><style>
*, *::before, *::after { box-sizing: border-box; }
html, body { margin: 0; padding: 0; font-family: -apple-system, sans-serif; font-size: 13px; }
.monaco-menu {
    position: absolute; left: 160px; top: 120px; width: 240px;
    border: 1px solid #454545; padding: 4px 0; display: flex; flex-direction: column;
}
.menu-item {
    height: 26px; padding: 0 12px; display: flex; align-items: center;
    justify-content: space-between;
}
.menu-label { height: 26px; line-height: 26px; font-size: 13px; white-space: nowrap; }
.menu-shortcut { height: 26px; line-height: 26px; font-size: 12px; white-space: nowrap; }
.menu-separator { height: 1px; margin: 4px 8px; }
</style></head><body>
<div class="monaco-menu" data-probe-id="0">
<div class="menu-item" data-probe-id="1">
<div class="menu-label" data-probe-id="2"><span class="menu-text" data-probe-id="3">Cut</span></div>
<div class="menu-shortcut" data-probe-id="4">Cmd+X</div>
</div>
<div class="menu-item" data-probe-id="5">
<div class="menu-label" data-probe-id="6">Copy</div>
<div class="menu-shortcut" data-probe-id="7">Cmd+C</div>
</div>
<div class="menu-separator" data-probe-id="8"></div>
<div class="menu-item" data-probe-id="9">
<div class="menu-label" data-probe-id="10">Paste</div>
<div class="menu-shortcut" data-probe-id="11">Cmd+V</div>
</div>
</div>
</body></html>"#;

    let geometry = "\
region\tcontext_menu\t0
el\t0\tcompare\t160\t120\t240\t97\tdiv.monaco-menu
el\t1\tcompare\t161\t125\t238\t26\tdiv.menu-item
el\t2\tcompare\t173\t125\t22\t26\tdiv.menu-label
el\t3\tcompare\t173\t130\t22\t16\tspan.menu-text
el\t4\tcompare\t343\t125\t44\t26\tdiv.menu-shortcut
el\t5\tcompare\t161\t151\t238\t26\tdiv.menu-item
el\t6\tcompare\t173\t151\t32\t26\tdiv.menu-label
el\t7\tcompare\t343\t151\t44\t26\tdiv.menu-shortcut
el\t8\tcompare\t169\t181\t222\t1\tdiv.menu-separator
el\t9\tcompare\t161\t186\t238\t26\tdiv.menu-item
el\t10\tcompare\t173\t186\t34\t26\tdiv.menu-label
el\t11\tcompare\t343\t186\t44\t26\tdiv.menu-shortcut
text\t13\t400\t22\t16\t13\tCut
text\t12\t400\t44\t14\t11\tCmd+X
text\t13\t400\t32\t16\t13\tCopy
text\t12\t400\t44\t14\t11\tCmd+C
text\t13\t400\t34\t16\t13\tPaste
text\t12\t400\t44\t14\t11\tCmd+V
";

    assert_workbench_region_tolerance("context_menu", html, geometry);
}

#[test]
fn fr34_5_box_tolerance_hover_widget() {
    let html = r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><style>
*, *::before, *::after { box-sizing: border-box; }
html, body { margin: 0; padding: 0; font-family: -apple-system, sans-serif; font-size: 13px; }
.monaco-hover {
    position: absolute; left: 220px; top: 180px; width: 380px;
    border: 1px solid #454545; display: flex; flex-direction: column;
}
.hover-code {
    padding: 6px 8px; border-bottom: 1px solid #3c3c3c;
    line-height: 18px; font-size: 12px; white-space: nowrap;
}
.hover-docs {
    padding: 6px 8px; line-height: 18px; font-size: 13px; white-space: nowrap;
}
.hover-actions {
    padding: 4px 8px; border-top: 1px solid #3c3c3c;
    display: flex; align-items: center; gap: 12px; height: 27px;
}
.hover-action { height: 18px; line-height: 18px; font-size: 12px; white-space: nowrap; }
</style></head><body>
<div class="monaco-hover" data-probe-id="0">
<div class="hover-code" data-probe-id="1"><span class="code-sig" data-probe-id="2">fn layout(&amp;mut self) -&gt; DisplayList</span></div>
<div class="hover-docs" data-probe-id="3"><span class="doc-line" data-probe-id="4">Computes the layout and returns a display list.</span></div>
<div class="hover-actions" data-probe-id="5">
<div class="hover-action" data-probe-id="6">Quick Fix...</div>
<div class="hover-action" data-probe-id="7">Peek Problem</div>
</div>
</div>
</body></html>"#;

    let geometry = "\
region\thover_widget\t0
el\t0\tcompare\t220\t180\t380\t90\tdiv.monaco-hover
el\t1\tcompare\t221\t181\t378\t31\tdiv.hover-code
el\t2\tcompare\t229\t189\t246\t14\tspan.code-sig
el\t3\tcompare\t221\t212\t378\t30\tdiv.hover-docs
el\t4\tcompare\t229\t219\t284\t16\tspan.doc-line
el\t5\tcompare\t221\t242\t378\t27\tdiv.hover-actions
el\t6\tcompare\t229\t247\t66\t18\tdiv.hover-action
el\t7\tcompare\t307\t247\t82\t18\tdiv.hover-action
text\t12\t400\t246\t14\t11\tfn layout(&mut self) -> DisplayList
text\t13\t400\t284\t16\t13\tComputes the layout and returns a display list.
text\t12\t400\t66\t14\t11\tQuick Fix...
text\t12\t400\t82\t14\t11\tPeek Problem
";

    assert_workbench_region_tolerance("hover_widget", html, geometry);
}
