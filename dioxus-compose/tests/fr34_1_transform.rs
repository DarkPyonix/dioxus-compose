//! CSS `transform` on the HTML path: a 2D matrix about `transform-origin` that moves how a
//! box and everything inside it is drawn, and where a pointer lands on it, and nothing
//! about layout. 3D transforms are left out and named.
//!
//! Every expected number is worked out by hand from the CSS in the fixture and the matrix
//! CSS defines for each function (`rotate(a)` is `(cos a, sin a, -sin a, cos a)`, `skewX(a)`
//! has `tan a` in its third component), never read back from the layout. Text is sized by
//! a measurer that gives every character 10px and every line 20px.

use std::cell::{Cell, RefCell};

use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    DisplayList, HtmlConfig, HtmlDom, LiteralColours, Matrix2D, ModifierSlot, NodeEntry, Plan,
    PlanChange, PlanKey, PlanModifier, TextMeasureRequest, TextMeasurer, TextMetrics, Transform2D,
    TransformFunction, WidthConstraint, diff,
};
use dioxus_core::ScopeId;

struct FixedAdvance;

impl TextMeasurer for FixedAdvance {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let width = match request.width {
            WidthConstraint::AtMost(width) => {
                (request.text.chars().count() as f32 * 10.0).min(width.max(0.0))
            }
            _ => request.text.chars().count() as f32 * 10.0,
        };
        TextMetrics {
            width,
            height: 20.0,
            first_baseline: 15.0,
            line_count: 1,
        }
    }
}

fn config(style: &str) -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![format!("html, body {{ margin: 0; padding: 0; }}\n{style}")],
        measurer: Some(Box::new(FixedAdvance)),
        ..HtmlConfig::default()
    }
}

thread_local! {
    static HITS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    static ANGLE: Cell<i32> = const { Cell::new(15) };
}

fn hit(name: &'static str) {
    HITS.with(|hits| hits.borrow_mut().push(name));
}

fn take_hits() -> Vec<&'static str> {
    HITS.with(|hits| std::mem::take(&mut *hits.borrow_mut()))
}

#[track_caller]
fn entry<'a>(dom: &HtmlDom, list: &'a DisplayList, id: &str) -> &'a NodeEntry {
    let node = dom
        .element_by_id(id)
        .unwrap_or_else(|| panic!("no element with id {id}"));
    list.get(node)
        .unwrap_or_else(|| panic!("#{id} has no display list entry"))
}

#[track_caller]
fn assert_points(actual: &[(f32, f32)], expected: &[(f32, f32)], what: &str) {
    assert_eq!(actual.len(), expected.len(), "{what}");
    for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (a.0 - e.0).abs() < 0.01 && (a.1 - e.1).abs() < 0.01,
            "{what}: point {index} is ({}, {}), expected ({}, {})",
            a.0,
            a.1,
            e.0,
            e.1
        );
    }
}

const SHAPES: &str = r#"
.box { position: absolute; width: 100px; height: 50px; background-color: rgb(51, 102, 153); }
#rot { left: 100px; top: 100px; transform: rotate(90deg); }
#scale { left: 300px; top: 100px; transform: scale(1.5); }
#skew { left: 500px; top: 100px; transform: skewX(10deg); }
#origin { left: 100px; top: 300px; transform: rotate(90deg); transform-origin: top left; }
"#;

fn shapes() -> Element {
    rsx! {
        div { id: "rot", class: "box", onclick: move |_| hit("rot") }
        div { id: "scale", class: "box", onclick: move |_| hit("scale") }
        div { id: "skew", class: "box" }
        div { id: "origin", class: "box" }
    }
}

fn open_shapes() -> (HtmlDom, DisplayList) {
    take_hits();
    let mut dom = HtmlDom::with_config(shapes, config(SHAPES));
    let list = dom.layout(800.0, 600.0, 1.0).clone();
    (dom, list)
}

/// `rotate(90deg)` about the centre, `scale(1.5)`, `skewX(10deg)` and a rotation about
/// `top left` each put the four corners of the box where the CSS matrices put them, while
/// the box keeps the rectangle layout gave it.
///
/// - `#rot` is 100 by 50 at (100, 100), so its centre is (150, 125). A corner at (dx, dy)
///   from the centre goes to (-dy, dx): the top-left (-50, -25) to (25, -50), and so on.
/// - `#scale` is the same box at (300, 100), centre (350, 125), half-sizes 75 and 37.5.
/// - `#skew` at (500, 100), centre (550, 125): x moves by tan(10 degrees) = 0.176327 times
///   dy, which is -25 on the top edge and 25 on the bottom one.
/// - `#origin` at (100, 300) turns about its own top-left corner, so that corner stays and
///   the top-right one, 100 to the right, ends up 100 below it.
#[test]
fn fr34_1_rotate_scale_skew_and_origin_put_the_corners_where_css_does() {
    let (dom, list) = open_shapes();

    let rot = entry(&dom, &list, "rot");
    assert_eq!(
        (rot.rect.x, rot.rect.y, rot.rect.width, rot.rect.height),
        (100.0, 100.0, 100.0, 50.0),
        "a transform does not change the box layout gave it"
    );
    let transform = rot.transform.as_ref().expect("#rot is rotated");
    assert_eq!(
        transform.origin,
        (50.0, 25.0),
        "the default origin is the centre"
    );
    assert_points(
        &transform.corners(&rot.rect),
        &[(175.0, 75.0), (175.0, 175.0), (125.0, 175.0), (125.0, 75.0)],
        "#rot",
    );

    let scale = entry(&dom, &list, "scale");
    assert_points(
        &scale
            .transform
            .as_ref()
            .expect("#scale is scaled")
            .corners(&scale.rect),
        &[(275.0, 87.5), (425.0, 87.5), (425.0, 162.5), (275.0, 162.5)],
        "#scale",
    );

    let skew = entry(&dom, &list, "skew");
    let shear = 10.0f32.to_radians().tan() * 25.0;
    assert_points(
        &skew
            .transform
            .as_ref()
            .expect("#skew is skewed")
            .corners(&skew.rect),
        &[
            (500.0 - shear, 100.0),
            (600.0 - shear, 100.0),
            (600.0 + shear, 150.0),
            (500.0 + shear, 150.0),
        ],
        "#skew",
    );

    let origin = entry(&dom, &list, "origin");
    let transform = origin.transform.as_ref().expect("#origin is rotated");
    assert_eq!(transform.origin, (0.0, 0.0), "transform-origin: top left");
    assert_points(
        &transform.corners(&origin.rect),
        &[(100.0, 300.0), (100.0, 400.0), (50.0, 400.0), (50.0, 300.0)],
        "#origin",
    );
}

const NESTED: &str = r#"
#outer {
    position: absolute; left: 100px; top: 100px; width: 100px; height: 100px;
    padding: 10px; box-sizing: border-box;
    transform: rotate(90deg);
    background-color: rgb(0, 0, 128);
}
#inner { width: 20px; height: 20px; background-color: rgb(255, 200, 0); }
"#;

fn nested() -> Element {
    rsx! {
        div { id: "outer",
            div { id: "inner" }
            div { id: "label", "Hi" }
        }
    }
}

/// What is inside a rotated box turns with it. `#outer` is 100 by 100 at (100, 100),
/// centre (150, 150), turned a quarter turn, with 10px of padding: `#inner` is 20 by 20 at
/// (110, 110) and `#label` is 80 by 20 at (110, 130) with its 20px of text at its left.
///
/// A point (dx, dy) from the centre goes to (-dy, dx). `#inner`'s corners at (-40, -40) and
/// (-20, -20) go to (40, -40) and (20, -20): (190, 110) and (170, 130). The text's
/// top-left, (-40, -20), goes to (20, -40): (170, 110).
///
/// In the plan, the transform is one modifier of `#outer`'s node and the children are
/// inside that node, placed by their own offsets from it, so a renderer draws them through
/// it.
#[test]
fn fr34_1_text_and_children_of_a_rotated_box_turn_with_it() {
    let mut dom = HtmlDom::with_config(nested, config(NESTED));
    let list = dom.layout(800.0, 600.0, 1.0).clone();
    let outer = entry(&dom, &list, "outer");
    let inner = entry(&dom, &list, "inner");
    let label = entry(&dom, &list, "label");

    assert!(inner.transform.is_none() && label.transform.is_none());
    assert_eq!((inner.rect.x, inner.rect.y), (110.0, 110.0));
    assert_points(
        &[
            list.project(inner.node, 110.0, 110.0).unwrap(),
            list.project(inner.node, 130.0, 130.0).unwrap(),
        ],
        &[(190.0, 110.0), (170.0, 130.0)],
        "#inner's corners",
    );
    let run = &label.texts[0];
    assert_eq!((run.rect.x, run.rect.y), (110.0, 130.0));
    assert_points(
        &[list.project(label.node, run.rect.x, run.rect.y).unwrap()],
        &[(170.0, 110.0)],
        "the text's top-left",
    );

    let plan = dom.plan(&mut LiteralColours).expect("laid out");
    let key = |entry: &NodeEntry| PlanKey::Node(entry.node);
    let outer_node = plan.find(key(outer)).expect("#outer is in the plan");
    assert!(
        matches!(
            outer_node.modifier(ModifierSlot::Transform),
            Some(PlanModifier::Transform(t)) if t.origin == (50.0, 50.0)
        ),
        "{outer_node:#?}"
    );
    assert_eq!(plan.parent_of(key(inner)).map(|n| n.key), Some(key(outer)));
    assert_eq!(plan.parent_of(key(label)).map(|n| n.key), Some(key(outer)));
    assert_eq!(
        plan.parent_of(PlanKey::Text {
            node: label.node,
            index: 0
        })
        .map(|n| n.key),
        Some(key(label))
    );
    for child in [inner, label] {
        let node = plan.find(key(child)).unwrap();
        assert!(
            node.modifier(ModifierSlot::Transform).is_none(),
            "only the rotated box carries the transform: {node:#?}"
        );
    }
    assert_eq!(plan.find(key(inner)).unwrap().offset(), (10.0, 10.0));
}

const MIXED: &str = r#"
.box { position: absolute; width: 100px; height: 50px; background-color: rgb(51, 102, 153); }
#slide { left: 20px; top: 20px; transform: translate(10px, 20px); }
#flat { left: 20px; top: 100px; transform: translate3d(5px, 0px, 0px); }
#tilt { left: 20px; top: 200px; transform: rotateX(45deg); }
#deep { left: 20px; top: 300px; transform: perspective(100px) rotateY(10deg); }
#zero { left: 20px; top: 400px; transform: translateZ(0px); }
"#;

fn mixed() -> Element {
    rsx! {
        div { id: "slide", class: "box" }
        div { id: "flat", class: "box" }
        div { id: "tilt", class: "box" }
        div { id: "deep", class: "box" }
        div { id: "zero", class: "box" }
    }
}

/// A translation only moves the box, as before. A 3D transform is not approximated in 2D:
/// the box is drawn without it, and the list names which element's which transform was
/// left out. `translate3d(5px, 0, 0)` and `translateZ(0)` are flat, so they are not 3D.
#[test]
fn fr34_1_translation_folds_into_the_place_and_3d_is_skipped_with_a_warning() {
    let mut dom = HtmlDom::with_config(mixed, config(MIXED));
    let list = dom.layout(800.0, 600.0, 1.0).clone();

    let slide = entry(&dom, &list, "slide");
    assert!(slide.transform.is_none());
    assert_eq!((slide.rect.x, slide.rect.y), (30.0, 40.0));

    let flat = entry(&dom, &list, "flat");
    assert!(flat.transform.is_none());
    assert_eq!((flat.rect.x, flat.rect.y), (25.0, 100.0));

    let tilt = entry(&dom, &list, "tilt");
    assert!(tilt.transform.is_none(), "not approximated in 2D");
    assert_eq!((tilt.rect.x, tilt.rect.y), (20.0, 200.0));

    let deep = entry(&dom, &list, "deep");
    assert!(deep.transform.is_none());
    assert_eq!((deep.rect.x, deep.rect.y), (20.0, 300.0));

    let zero = entry(&dom, &list, "zero");
    assert!(zero.transform.is_none());
    assert_eq!((zero.rect.x, zero.rect.y), (20.0, 400.0));

    let skipped = &list.skipped_transforms;
    assert_eq!(skipped.len(), 2, "{skipped:#?}");
    assert_eq!(skipped[0].node, tilt.node);
    assert_eq!(skipped[0].tag, "div");
    assert!(
        skipped[0].transform.contains("rotateX(45deg)"),
        "{skipped:#?}"
    );
    assert_eq!(skipped[1].node, deep.node);
    assert!(
        skipped[1].transform.contains("perspective(100px)")
            && skipped[1].transform.contains("rotateY(10deg)"),
        "{skipped:#?}"
    );
}

const BESIDE_2D: &str = r#"
.box { position: absolute; width: 100px; height: 50px; background-color: rgb(51, 102, 153); }
#tilt { left: 100px; top: 100px; transform: rotateX(45deg); }
#turn { left: 300px; top: 100px; transform: rotate(90deg); }
#move { left: 500px; top: 100px; transform: translate(20px, 0px); }
"#;

fn beside_2d() -> Element {
    rsx! {
        div { id: "tilt", class: "box" }
        div { id: "turn", class: "box" }
        div { id: "move", class: "box" }
    }
}

/// A 3D transform on one element is skipped with a warning (the layout writes it to stderr
/// and lists it in `skipped_transforms`), and the 2D transforms beside it still apply. The
/// stderr line itself is not asserted here, since the list is what a test can read.
///
/// - `#tilt` is drawn flat at (100, 100) with no transform and is the only skipped entry.
/// - `#turn` is 100 by 50 at (300, 100), centre (350, 125), turned a quarter turn, so its
///   corners are (375, 75), (375, 175), (325, 175) and (325, 75).
/// - `#move` is translated 20px, which only moves it to x = 520 and adds no transform.
#[test]
fn fr34_1_3d_transform_is_skipped_with_a_warning_and_2d_siblings_keep_working() {
    let mut dom = HtmlDom::with_config(beside_2d, config(BESIDE_2D));
    let list = dom.layout(800.0, 600.0, 1.0).clone();

    let tilt = entry(&dom, &list, "tilt");
    assert!(tilt.transform.is_none(), "rotateX is not drawn in 2D");
    assert_eq!(
        (tilt.rect.x, tilt.rect.y, tilt.rect.width, tilt.rect.height),
        (100.0, 100.0, 100.0, 50.0)
    );

    let skipped = &list.skipped_transforms;
    assert_eq!(skipped.len(), 1, "{skipped:#?}");
    assert_eq!(skipped[0].node, tilt.node);
    assert_eq!(skipped[0].tag, "div");
    assert!(skipped[0].transform.contains("rotateX(45deg)"), "{skipped:#?}");

    let turn = entry(&dom, &list, "turn");
    assert_points(
        &turn
            .transform
            .as_ref()
            .expect("#turn is rotated in 2D")
            .corners(&turn.rect),
        &[(375.0, 75.0), (375.0, 175.0), (325.0, 175.0), (325.0, 75.0)],
        "#turn",
    );

    let moved = entry(&dom, &list, "move");
    assert!(moved.transform.is_none(), "a translation folds into the place");
    assert_eq!((moved.rect.x, moved.rect.y), (520.0, 100.0));
}

fn flow() -> Element {
    rsx! {
        div { id: "a", style: "width: 100px; height: 50px; transform: scale(2) rotate(10deg)" }
        div { id: "b", style: "height: 30px" }
    }
}

/// A transform never changes layout: the box after a transformed one is where it would be
/// without the transform.
#[test]
fn fr34_1_a_transform_does_not_move_the_boxes_around_it() {
    let mut dom = HtmlDom::with_config(flow, config(""));
    let list = dom.layout(800.0, 600.0, 1.0).clone();
    let a = entry(&dom, &list, "a");
    let b = entry(&dom, &list, "b");
    assert_eq!(
        (a.rect.x, a.rect.y, a.rect.width, a.rect.height),
        (0.0, 0.0, 100.0, 50.0)
    );
    assert_eq!(
        (b.rect.x, b.rect.y, b.rect.width, b.rect.height),
        (0.0, 50.0, 800.0, 30.0)
    );
    assert!(a.transform.is_some());
}

/// A pointer is tested against the shape that is drawn.
///
/// - `#rot` is drawn in x 125 to 175, y 75 to 175 (the corners above). (150, 80) is above
///   the box layout gave it (y 100 to 150) and inside what is drawn, so the click reaches
///   its handler. (110, 125) is inside the layout box and left of what is drawn (x < 125),
///   so it does not.
/// - `#scale` is drawn 275 to 425 across. (424, 125) is 24px past the layout box and inside
///   the drawing: 74 from the centre, 49.3 in the box's own coordinates. (426, 125) is
///   outside.
#[test]
fn fr34_1_a_click_outside_the_drawn_corner_misses_and_inside_it_hits() {
    let (mut dom, _) = open_shapes();
    let rot = dom.element_by_id("rot").unwrap();
    let scale = dom.element_by_id("scale").unwrap();

    assert_eq!(dom.click(150.0, 80.0), Some(rot));
    assert_eq!(take_hits(), ["rot"]);

    assert_eq!(dom.click(110.0, 125.0), None);
    assert_eq!(take_hits(), Vec::<&str>::new());

    assert_eq!(dom.click(424.0, 125.0), Some(scale));
    assert_eq!(take_hits(), ["scale"]);

    assert_eq!(dom.click(426.0, 125.0), None);
    assert_eq!(take_hits(), Vec::<&str>::new());
}

fn turning() -> Element {
    let angle = ANGLE.with(Cell::get);
    rsx! {
        div {
            id: "spin",
            style: "position: absolute; left: 10px; top: 10px; width: 100px; height: 50px; background-color: rgb(1, 2, 3); transform: rotate({angle}deg)",
        }
        div {
            id: "still",
            style: "position: absolute; left: 200px; top: 10px; width: 20px; height: 20px; background-color: rgb(4, 5, 6)",
        }
    }
}

fn plan_of(dom: &mut HtmlDom) -> Plan {
    dom.layout(800.0, 600.0, 1.0);
    dom.plan(&mut LiteralColours).expect("laid out")
}

/// Changing only the angle sends one change: the node's transform modifier. Its offset,
/// size and everything else stay as they were.
#[test]
fn fr34_1_changing_only_the_transform_sends_only_that_modifier() {
    ANGLE.with(|angle| angle.set(15));
    let mut dom = HtmlDom::with_config(turning, config(""));
    let before = plan_of(&mut dom);
    let spin = dom.element_by_id("spin").unwrap();

    ANGLE.with(|angle| angle.set(30));
    dom.virtual_dom_mut().mark_dirty(ScopeId::APP);
    dom.render();
    let after = plan_of(&mut dom);

    let changes = diff(&before, &after);
    assert_eq!(changes.len(), 1, "{changes:#?}");
    let PlanChange::ModifierSet {
        key,
        slot,
        modifier:
            PlanModifier::Transform(Transform2D {
                matrix,
                origin,
                functions,
            }),
    } = &changes[0]
    else {
        panic!("expected a transform modifier, got {changes:#?}");
    };
    assert_eq!(*key, PlanKey::Node(spin));
    assert_eq!(*slot, ModifierSlot::Transform);
    // rotate(30deg): cos 30 = 0.866025, sin 30 = 0.5.
    let expected = Matrix2D::rotation(30.0f32.to_radians());
    assert!(
        (matrix.a - expected.a).abs() < 1e-4
            && (matrix.b - expected.b).abs() < 1e-4
            && (matrix.c - expected.c).abs() < 1e-4
            && (matrix.d - expected.d).abs() < 1e-4,
        "{matrix:?}"
    );
    assert_eq!(*origin, (50.0, 25.0));
    // The list is kept with the matrix: one rotation of 30 degrees.
    assert!(
        matches!(functions[..], [TransformFunction::Rotate(radians)] if (radians - 30.0f32.to_radians()).abs() < 1e-4),
        "{functions:?}"
    );
    // The identical layout again is no change at all.
    assert!(diff(&after, &plan_of(&mut dom)).is_empty());
}
