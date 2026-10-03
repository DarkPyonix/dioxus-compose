//! The display list turned into a tree of drawing elements, and the changes between two
//! such trees.
//!
//! Inputs are small HTML documents laid out through the crate's own API. Expected numbers
//! follow from the CSS in each fixture, never read back from the layout.

use std::cell::{Cell, RefCell};

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus_compose_html::prelude::*;
use dioxus_compose_html::{
    BaseDocument, BorderSide, Corners, HtmlConfig, HtmlDom, ModifierSlot, Plan, PlanChange,
    PlanKey, PlanKind, PlanModifier, PlanNode, Rgba, TextMeasureRequest, TextMeasurer, TextMetrics,
    WidthConstraint, diff, element_by_id, layout_document, plan_from,
};
use dioxus_core::ScopeId;

/// Every character is `char_width` wide and every line `line_height` tall; lines break
/// greedily at spaces.
struct FakeMeasurer {
    char_width: f32,
    line_height: f32,
}

impl TextMeasurer for FakeMeasurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let max_chars = match request.width {
            WidthConstraint::MaxContent => usize::MAX,
            WidthConstraint::MinContent => 0,
            WidthConstraint::AtMost(width) => (width / self.char_width).floor() as usize,
        };
        let mut lines: Vec<String> = Vec::new();
        let mut current = String::new();
        for word in request.text.split(' ') {
            if current.is_empty() {
                current.push_str(word);
            } else if current.chars().count() + 1 + word.chars().count() <= max_chars {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
            }
        }
        lines.push(current);
        let width = lines
            .iter()
            .map(|line| line.chars().count() as f32 * self.char_width)
            .fold(0.0, f32::max);
        TextMetrics {
            width,
            height: lines.len() as f32 * self.line_height,
            first_baseline: self.line_height * 0.75,
            line_count: lines.len() as u32,
        }
    }
}

fn measurer() -> FakeMeasurer {
    FakeMeasurer {
        char_width: 10.0,
        line_height: 20.0,
    }
}

/// Parses and lays out `html` in an 800 by 600 viewport, and plans it.
fn plan_html(html: &str) -> (BaseDocument, Plan) {
    let config = DocumentConfig {
        viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
        ..DocumentConfig::default()
    };
    let mut doc = HtmlDocument::from_html(html, config).into_inner();
    let list = layout_document(&mut doc, &mut measurer());
    let plan = plan_from(&list);
    (doc, plan)
}

fn key(doc: &BaseDocument, id: &str) -> PlanKey {
    PlanKey::Node(element_by_id(doc, id).unwrap_or_else(|| panic!("no element with id {id}")))
}

#[track_caller]
fn node(plan: &Plan, key: PlanKey) -> &PlanNode {
    plan.find(key)
        .unwrap_or_else(|| panic!("{key:?} is not in the plan:\n{plan:#?}"))
}

#[track_caller]
fn parent(plan: &Plan, key: PlanKey) -> &PlanNode {
    plan.parent_of(key)
        .unwrap_or_else(|| panic!("{key:?} has no parent in the plan"))
}

fn child_keys(node: &PlanNode) -> Vec<PlanKey> {
    node.children.iter().map(|child| child.key).collect()
}

const BLUE: Rgba = Rgba::new(0, 0, 255, 255);
const RED: Rgba = Rgba::new(255, 0, 0, 255);

#[test]
fn fr34_plan_equal_borders_use_border_and_unequal_use_border_each() {
    let (doc, plan) = plan_html(
        r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
div { width: 20px; height: 20px; }
#same { border: 2px solid rgb(0, 0, 255); }
#widths { border-top: 1px solid rgb(255, 0, 0); border-bottom: 3px solid rgb(0, 0, 255); }
#colours { border: 2px solid rgb(0, 0, 255); border-left-color: rgb(255, 0, 0); }
</style></head><body><div id="same"></div><div id="widths"></div><div id="colours"></div></body></html>"#,
    );

    let same = node(&plan, key(&doc, "same"));
    assert_eq!(
        same.modifier(ModifierSlot::Border),
        Some(&PlanModifier::Border {
            width: 2.0,
            color: BLUE
        })
    );
    assert_eq!(same.modifier(ModifierSlot::BorderEach), None);

    let widths = node(&plan, key(&doc, "widths"));
    assert_eq!(widths.modifier(ModifierSlot::Border), None);
    let Some(PlanModifier::BorderEach(sides)) = widths.modifier(ModifierSlot::BorderEach) else {
        panic!("sides of different widths need BorderEach: {widths:#?}");
    };
    assert_eq!(
        sides.top,
        BorderSide {
            width: 1.0,
            color: RED
        }
    );
    assert_eq!(
        sides.bottom,
        BorderSide {
            width: 3.0,
            color: BLUE
        }
    );
    assert_eq!((sides.left.width, sides.right.width), (0.0, 0.0));

    // The same width everywhere is not enough: one side has another colour.
    let colours = node(&plan, key(&doc, "colours"));
    assert_eq!(colours.modifier(ModifierSlot::Border), None);
    let Some(PlanModifier::BorderEach(sides)) = colours.modifier(ModifierSlot::BorderEach) else {
        panic!("sides of different colours need BorderEach: {colours:#?}");
    };
    assert_eq!(sides.left.color, RED);
    assert_eq!(sides.top.color, BLUE);
    assert_eq!(sides.left.width, 2.0);
}

#[test]
fn fr34_plan_equal_radii_use_shape_and_unequal_use_corner_each() {
    let (doc, plan) = plan_html(
        r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
div { width: 40px; height: 40px; background-color: rgb(0, 0, 255); }
#round { border-radius: 6px; }
#mixed { border-radius: 4px 8px; }
#square { }
</style></head><body><div id="round"></div><div id="mixed"></div><div id="square"></div></body></html>"#,
    );

    let round = node(&plan, key(&doc, "round"));
    assert_eq!(
        round.modifier(ModifierSlot::Shape),
        Some(&PlanModifier::Shape { radius: 6.0 })
    );
    assert_eq!(round.modifier(ModifierSlot::CornerEach), None);

    // `border-radius: 4px 8px`: top-left and bottom-right 4px, the other two 8px.
    let mixed = node(&plan, key(&doc, "mixed"));
    assert_eq!(mixed.modifier(ModifierSlot::Shape), None);
    assert_eq!(
        mixed.modifier(ModifierSlot::CornerEach),
        Some(&PlanModifier::CornerEach(Corners {
            top_left: 4.0,
            top_right: 8.0,
            bottom_right: 4.0,
            bottom_left: 8.0,
        }))
    );

    let square = node(&plan, key(&doc, "square"));
    assert_eq!(square.modifier(ModifierSlot::Shape), None);
    assert_eq!(square.modifier(ModifierSlot::CornerEach), None);
}

/// #outer is at (100, 50) on the page and #inner at (30, 20) inside it, so (130, 70) on
/// the page. In the plan #inner is #outer's child, 30 and 20 from #outer's corner.
#[test]
fn fr34_plan_nested_absolute_positions_become_relative_offsets() {
    let (doc, plan) = plan_html(
        r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
#outer { position: absolute; left: 100px; top: 50px; width: 200px; height: 200px; background-color: rgb(0, 0, 255); }
#inner { position: absolute; left: 30px; top: 20px; width: 40px; height: 40px; background-color: rgb(255, 0, 0); }
</style></head><body><div id="outer"><div id="inner"></div></div></body></html>"#,
    );

    let outer = key(&doc, "outer");
    let inner = key(&doc, "inner");
    assert_eq!(parent(&plan, inner).key, outer);
    assert_eq!(parent(&plan, inner).kind, PlanKind::AbsoluteBox);
    assert_eq!(node(&plan, inner).offset(), (30.0, 20.0));
    assert_eq!(
        node(&plan, inner).modifier(ModifierSlot::RequiredSize),
        Some(&PlanModifier::RequiredSize {
            width: 40.0,
            height: 40.0
        })
    );
    assert_eq!(node(&plan, inner).kind, PlanKind::Box);
    // #outer sits in the body, which is at the page's origin.
    assert_eq!(node(&plan, outer).offset(), (100.0, 50.0));
}

#[test]
fn fr34_plan_overflow_scroll_becomes_scroll_column_with_content() {
    let (doc, plan) = plan_html(
        r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
#scroller { width: 60px; height: 60px; overflow-x: hidden; overflow-y: auto; }
#tall { width: 60px; height: 200px; background-color: rgb(0, 0, 255); }
#both { width: 60px; height: 60px; overflow: auto; }
#wide { width: 200px; height: 200px; background-color: rgb(255, 0, 0); }
</style></head><body><div id="scroller"><div id="tall"></div></div><div id="both"><div id="wide"></div></div></body></html>"#,
    );

    // Only the vertical axis scrolls: a ScrollColumn the size of the padding box, around
    // content as tall as #tall.
    let scroller = key(&doc, "scroller");
    let id = scroller.node().unwrap();
    assert_eq!(
        child_keys(node(&plan, scroller)),
        vec![PlanKey::ScrollColumn(id)]
    );
    let column = node(&plan, PlanKey::ScrollColumn(id));
    assert_eq!(column.kind, PlanKind::ScrollColumn);
    assert_eq!(column.offset(), (0.0, 0.0));
    assert_eq!(
        column.modifier(ModifierSlot::RequiredSize),
        Some(&PlanModifier::RequiredSize {
            width: 60.0,
            height: 60.0
        })
    );
    assert_eq!(child_keys(column), vec![PlanKey::ScrollContent(id)]);
    let content = node(&plan, PlanKey::ScrollContent(id));
    let Some(PlanModifier::RequiredSize { height, .. }) =
        content.modifier(ModifierSlot::RequiredSize)
    else {
        panic!("scroll content has a size: {content:#?}");
    };
    assert_eq!(*height, 200.0);
    // #tall is inside the content, at its origin, with no clip of its own: the scroll
    // viewport already clips it.
    let tall = key(&doc, "tall");
    assert_eq!(child_keys(content), vec![tall]);
    assert_eq!(node(&plan, tall).offset(), (0.0, 0.0));
    assert!(
        plan.find(PlanKey::InheritedClip(tall.node().unwrap()))
            .is_none()
    );

    // Both axes scroll: a ScrollColumn around a ScrollRow around the content.
    let both = key(&doc, "both");
    let id = both.node().unwrap();
    assert_eq!(
        child_keys(node(&plan, both)),
        vec![PlanKey::ScrollColumn(id)]
    );
    assert_eq!(
        child_keys(node(&plan, PlanKey::ScrollColumn(id))),
        vec![PlanKey::ScrollRow(id)]
    );
    let row = node(&plan, PlanKey::ScrollRow(id));
    assert_eq!(row.kind, PlanKind::ScrollRow);
    assert_eq!(child_keys(row), vec![PlanKey::ScrollContent(id)]);
    assert_eq!(
        child_keys(node(&plan, PlanKey::ScrollContent(id))),
        vec![key(&doc, "wide")]
    );
}

/// `opacity` fades a box and what is inside it as one group, once. A child without its
/// own opacity gets no Alpha; a child with `opacity: 0.5` inside a group at 0.5 gets its
/// own 0.5, not the 0.25 the two make together.
#[test]
fn fr34_plan_opacity_is_one_alpha_on_the_group() {
    let (doc, plan) = plan_html(
        r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
#group { width: 100px; opacity: 0.5; }
#plain { height: 10px; background-color: rgb(255, 0, 0); }
#faded { height: 10px; background-color: rgb(0, 0, 255); opacity: 0.5; }
</style></head><body><div id="group"><div id="plain"></div><div id="faded"></div></div></body></html>"#,
    );

    let group = key(&doc, "group");
    let plain = key(&doc, "plain");
    let faded = key(&doc, "faded");
    assert_eq!(
        node(&plan, group).modifier(ModifierSlot::Alpha),
        Some(&PlanModifier::Alpha(0.5))
    );
    assert_eq!(parent(&plan, plain).key, group);
    assert_eq!(parent(&plan, faded).key, group);
    assert_eq!(node(&plan, plain).modifier(ModifierSlot::Alpha), None);
    assert_eq!(
        node(&plan, faded).modifier(ModifierSlot::Alpha),
        Some(&PlanModifier::Alpha(0.5))
    );
    let alphas = plan
        .nodes()
        .into_iter()
        .filter(|node| node.modifier(ModifierSlot::Alpha).is_some())
        .count();
    assert_eq!(alphas, 2, "only #group and #faded carry an Alpha");
}

/// Three overlapping boxes with z-index 3, 1 and 2 in tree order are children 1, 2, 3
/// in z-index order. A box raised by z-index above a later sibling of its parent that it
/// overlaps cannot stay inside its parent (a child is drawn before its parent's later
/// siblings), so it is placed after that sibling instead.
#[test]
fn fr34_plan_z_index_order_is_child_order() {
    let (doc, plan) = plan_html(
        r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
.stacked { position: absolute; left: 0px; top: 0px; width: 50px; height: 50px; background-color: rgb(0, 0, 255); }
#top { z-index: 3; }
#bottom { z-index: 1; }
#middle { z-index: 2; }
#wrap { height: 60px; margin-top: 100px; }
#raised { position: relative; z-index: 1; height: 40px; background-color: rgb(255, 0, 0); }
#cover { margin-top: -30px; height: 40px; background-color: rgb(0, 128, 0); }
</style></head><body><div id="top" class="stacked"></div><div id="bottom" class="stacked"></div><div id="middle" class="stacked"></div><div id="wrap"><div id="raised"></div></div><div id="cover"></div></body></html>"#,
    );

    let top = key(&doc, "top");
    let bottom = key(&doc, "bottom");
    let middle = key(&doc, "middle");
    let container = parent(&plan, top);
    assert_eq!(parent(&plan, bottom).key, container.key);
    assert_eq!(parent(&plan, middle).key, container.key);
    let order: Vec<PlanKey> = child_keys(container)
        .into_iter()
        .filter(|key| [top, bottom, middle].contains(key))
        .collect();
    assert_eq!(order, vec![bottom, middle, top]);

    // #raised spans y 100 to 140 and #cover y 130 to 170: they overlap, and #raised is on
    // top.
    let raised = key(&doc, "raised");
    let cover = key(&doc, "cover");
    let wrap = key(&doc, "wrap");
    assert_ne!(parent(&plan, raised).key, wrap);
    let shared = parent(&plan, cover);
    assert_eq!(parent(&plan, raised).key, shared.key);
    let siblings = child_keys(shared);
    let position = |key: PlanKey| siblings.iter().position(|k| *k == key).unwrap();
    assert!(
        position(raised) > position(cover),
        "#raised is drawn after #cover: {siblings:?}"
    );
    // Moved out of #wrap, it still sits where the layout put it.
    let (_, y) = node(&plan, raised).offset();
    let (_, cover_y) = node(&plan, cover).offset();
    assert_eq!(cover_y - y, 30.0);
}

#[test]
fn fr34_plan_text_runs_are_text_children_with_offset_and_width() {
    let (doc, plan) = plan_html(
        r#"<!DOCTYPE html><html><head><style>
html, body { margin: 0; }
#label { padding: 4px 6px; font-size: 16px; color: rgb(10, 20, 30); }
</style></head><body><div id="label">Hello</div></body></html>"#,
    );

    let label = key(&doc, "label");
    let id = label.node().unwrap();
    let text_key = PlanKey::Text { node: id, index: 0 };
    assert_eq!(parent(&plan, text_key).key, label);
    let text = node(&plan, text_key);
    let PlanKind::Text(run) = &text.kind else {
        panic!("a text run is a Text: {text:#?}");
    };
    assert_eq!(run.text, "Hello");
    assert_eq!(run.color, Rgba::new(10, 20, 30, 255));
    // Inside the padding: 6px from the left, 4px from the top; five characters at 10px.
    assert_eq!(text.offset(), (6.0, 4.0));
    assert_eq!(
        text.modifier(ModifierSlot::Width),
        Some(&PlanModifier::Width(50.0))
    );
}

fn margin_free() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
        measurer: Some(Box::new(measurer())),
        ..HtmlConfig::default()
    }
}

fn plan_of(dom: &mut HtmlDom) -> Plan {
    plan_from(dom.layout(400.0, 300.0, 1.0))
}

fn form() -> Element {
    rsx! {
        input { id: "name", value: "Ada", placeholder: "Name", style: "width: 120px; height: 24px" }
    }
}

#[test]
fn fr34_plan_input_is_a_text_field() {
    let mut dom = HtmlDom::with_config(form, margin_free());
    let plan = plan_of(&mut dom);
    let name = dom.element_by_id("name").unwrap();
    let field = node(&plan, PlanKey::Field(name));
    let PlanKind::TextField(text_field) = &field.kind else {
        panic!("an <input> is a TextField: {field:#?}");
    };
    assert_eq!(text_field.value, "Ada");
    assert_eq!(text_field.placeholder.as_deref(), Some("Name"));
    assert!(!text_field.multiline);
    assert_eq!(parent(&plan, PlanKey::Field(name)).key, PlanKey::Node(name));
}

thread_local! {
    static TARGET_IS_RED: Cell<bool> = const { Cell::new(false) };
    static MOVER_LEFT: Cell<f32> = const { Cell::new(0.0) };
    static ITEMS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

fn three_bars() -> Element {
    let color = if TARGET_IS_RED.with(Cell::get) {
        "rgb(255, 0, 0)"
    } else {
        "rgb(0, 0, 255)"
    };
    rsx! {
        div { id: "list", style: "display: flex; flex-direction: column; width: 100px",
            div { id: "first", style: "height: 20px; background-color: rgb(0, 128, 0)" }
            div { id: "target", style: "height: 20px; background-color: {color}" }
            div { id: "third", style: "height: 20px; background-color: rgb(0, 128, 0)" }
        }
    }
}

#[test]
fn fr34_plan_one_background_change_is_one_plan_change() {
    TARGET_IS_RED.with(|red| red.set(false));
    let mut dom = HtmlDom::with_config(three_bars, margin_free());
    let before = plan_of(&mut dom);
    let target = dom.element_by_id("target").unwrap();
    assert!(
        diff(&before, &before).is_empty(),
        "a plan does not differ from itself"
    );

    TARGET_IS_RED.with(|red| red.set(true));
    dom.virtual_dom_mut().mark_dirty(ScopeId::APP);
    dom.render();
    let after = plan_of(&mut dom);

    assert_eq!(
        diff(&before, &after),
        vec![PlanChange::ModifierSet {
            key: PlanKey::Node(target),
            slot: ModifierSlot::Background,
            modifier: PlanModifier::Background(RED),
        }]
    );
}

fn mover() -> Element {
    let left = MOVER_LEFT.with(Cell::get);
    rsx! {
        div { id: "stage", style: "position: relative; width: 400px; height: 300px",
            div {
                id: "mover",
                style: "position: absolute; left: {left}px; top: 0px; width: 50px; height: 50px; background-color: rgb(0, 0, 255)",
                div { id: "dot", style: "width: 10px; height: 10px; background-color: rgb(255, 0, 0)" }
            }
            div { id: "still", style: "position: absolute; left: 300px; top: 200px; width: 20px; height: 20px; background-color: rgb(0, 128, 0)" }
        }
    }
}

/// Moving a box changes where it is in its container and nothing else: what is inside it
/// is placed relative to it, so it does not move in its own container.
#[test]
fn fr34_plan_moved_box_is_one_offset_change() {
    MOVER_LEFT.with(|left| left.set(0.0));
    let mut dom = HtmlDom::with_config(mover, margin_free());
    let before = plan_of(&mut dom);
    let mover = dom.element_by_id("mover").unwrap();
    assert_eq!(node(&before, PlanKey::Node(mover)).offset(), (0.0, 0.0));

    MOVER_LEFT.with(|left| left.set(100.0));
    dom.virtual_dom_mut().mark_dirty(ScopeId::APP);
    dom.render();
    let after = plan_of(&mut dom);

    assert_eq!(
        diff(&before, &after),
        vec![PlanChange::ModifierSet {
            key: PlanKey::Node(mover),
            slot: ModifierSlot::Offset,
            modifier: PlanModifier::Offset { x: 100.0, y: 0.0 },
        }]
    );
}

fn keyed_list() -> Element {
    let items = ITEMS.with(|items| items.borrow().clone());
    rsx! {
        div { id: "list", style: "width: 100px",
            for item in items {
                div {
                    key: "{item}",
                    id: "item{item}",
                    style: "height: 10px; background-color: rgb(0, 0, 255)",
                }
            }
        }
    }
}

fn set_items(dom: &mut HtmlDom, items: &[u32]) -> Plan {
    ITEMS.with(|current| *current.borrow_mut() = items.to_vec());
    dom.virtual_dom_mut().mark_dirty(ScopeId::APP);
    dom.render();
    plan_of(dom)
}

#[test]
fn fr34_plan_diff_reports_insert_remove_and_reorder() {
    ITEMS.with(|items| *items.borrow_mut() = vec![1, 2, 3]);
    let mut dom = HtmlDom::with_config(keyed_list, margin_free());
    let first = plan_of(&mut dom);
    let list = PlanKey::Node(dom.element_by_id("list").unwrap());
    let one = PlanKey::Node(dom.element_by_id("item1").unwrap());
    let two = PlanKey::Node(dom.element_by_id("item2").unwrap());
    let three = PlanKey::Node(dom.element_by_id("item3").unwrap());
    assert_eq!(child_keys(node(&first, list)), vec![one, two, three]);

    // Removed: #item2 goes, and #item3 moves up into its place.
    let second = set_items(&mut dom, &[1, 3]);
    let changes = diff(&first, &second);
    assert!(
        changes.contains(&PlanChange::Removed { key: two }),
        "{changes:#?}"
    );
    assert!(
        changes.contains(&PlanChange::ModifierSet {
            key: three,
            slot: ModifierSlot::Offset,
            modifier: PlanModifier::Offset { x: 0.0, y: 10.0 },
        }),
        "{changes:#?}"
    );
    assert!(
        !changes
            .iter()
            .any(|change| matches!(change, PlanChange::Reordered { .. })),
        "removing a child does not reorder the others: {changes:#?}"
    );

    // Reordered: the same two children, the other way round.
    let third = set_items(&mut dom, &[3, 1]);
    let changes = diff(&second, &third);
    assert!(
        changes.contains(&PlanChange::Reordered {
            parent: list,
            order: vec![three, one],
        }),
        "{changes:#?}"
    );
    assert!(
        !changes.iter().any(|change| matches!(
            change,
            PlanChange::Removed { .. } | PlanChange::Inserted { .. }
        )),
        "a reorder keeps the nodes: {changes:#?}"
    );

    // Inserted: a new child at the end, with everything it draws.
    let fourth = set_items(&mut dom, &[3, 1, 4]);
    let four = PlanKey::Node(dom.element_by_id("item4").unwrap());
    let changes = diff(&third, &fourth);
    let inserted: Vec<_> = changes
        .iter()
        .filter_map(|change| match change {
            PlanChange::Inserted {
                parent,
                index,
                node,
            } => Some((*parent, *index, node.key)),
            _ => None,
        })
        .collect();
    assert_eq!(inserted, vec![(list, 2, four)], "{changes:#?}");
    assert!(
        !changes.iter().any(|change| matches!(
            change,
            PlanChange::Removed { .. } | PlanChange::Reordered { .. }
        )),
        "{changes:#?}"
    );
}
