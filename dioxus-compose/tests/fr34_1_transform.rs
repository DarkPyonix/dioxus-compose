//! CSS `transform` on the HTML path: a 2D matrix about `transform-origin` that moves how a
//! box and everything inside it is drawn, and where a pointer lands on it, and nothing
//! about layout. A 3D transform is one 4x4 matrix with its perspective inside it, and
//! moves the same things.
//!
//! Every expected number is worked out by hand from the CSS in the fixture and the matrix
//! CSS defines for each function (`rotate(a)` is `(cos a, sin a, -sin a, cos a)`, `skewX(a)`
//! has `tan a` in its third component, `rotateY(a)` takes x to `x cos a` and its depth to
//! `-x sin a`, `perspective(d)` divides by `1 - z / d`), never read back from the layout. Text is sized by
//! a measurer that gives every character 10px and every line 20px.

use std::cell::{Cell, RefCell};

use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    DisplayList, FlattenedSpace, HtmlConfig, HtmlDom, LiteralColours, Matrix2D, ModifierSlot,
    NodeEntry, Plan, PlanChange, PlanKey, PlanModifier, TextMeasureRequest, TextMeasurer,
    TextMetrics, Transform2D, TransformFunction, WidthConstraint, diff,
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

/// A translation only moves the box, as before. A 3D transform is not approximated in 2D
/// and not left out: it is kept whole, as one 4x4 matrix about the box's centre, and
/// nothing is reported as skipped. `translate3d(5px, 0, 0)` and `translateZ(0)` are flat,
/// so they are not 3D.
///
/// - `#tilt`, `rotateX(45deg)`: y goes to `y cos 45` and its depth to `y sin 45`, so the
///   matrix has cos 45 = 0.707107 at row 1 of column 1 and sin 45 at row 2 of column 1.
/// - `#deep`, `perspective(100px) rotateY(10deg)`: the perspective's `-1 / 100` in the
///   bottom row meets the rotation's z column, so row 3 of column 2 is `-cos 10 / 100`
///   and row 3 of column 0 is `sin 10 / 100`. The perspective is in the matrix.
#[test]
fn fr34_1_translation_folds_into_the_place_and_a_3d_transform_is_kept_whole() {
    let mut dom = HtmlDom::with_config(mixed, config(MIXED));
    let list = dom.layout(800.0, 600.0, 1.0).clone();

    let slide = entry(&dom, &list, "slide");
    assert!(slide.transform.is_none() && slide.transform_3d.is_none());
    assert_eq!((slide.rect.x, slide.rect.y), (30.0, 40.0));

    let flat = entry(&dom, &list, "flat");
    assert!(flat.transform.is_none() && flat.transform_3d.is_none());
    assert_eq!((flat.rect.x, flat.rect.y), (25.0, 100.0));

    let tilt = entry(&dom, &list, "tilt");
    assert!(tilt.transform.is_none(), "not approximated in 2D");
    assert_eq!((tilt.rect.x, tilt.rect.y), (20.0, 200.0));
    let tilted = tilt.transform_3d.as_ref().expect("#tilt is turned about x");
    assert_eq!(
        tilted.origin,
        (50.0, 25.0),
        "the default origin is the centre"
    );
    assert!(!tilted.backface_hidden);
    let (sin, cos) = 45.0f32.to_radians().sin_cos();
    let expected = [
        1.0, 0.0, 0.0, 0.0, 0.0, cos, sin, 0.0, 0.0, -sin, cos, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    for (index, (a, e)) in tilted.matrix.0.iter().zip(expected).enumerate() {
        assert!((a - e).abs() < 1e-5, "#tilt element {index}: {a} for {e}");
    }

    let deep = entry(&dom, &list, "deep");
    assert!(deep.transform.is_none());
    assert_eq!((deep.rect.x, deep.rect.y), (20.0, 300.0));
    let sunk = deep.transform_3d.as_ref().expect("#deep has a perspective");
    let (sin, cos) = 10.0f32.to_radians().sin_cos();
    assert!(
        (sunk.matrix.0[11] + cos / 100.0).abs() < 1e-6
            && (sunk.matrix.0[3] - sin / 100.0).abs() < 1e-6,
        "the perspective is inside the matrix: {:?}",
        sunk.matrix
    );

    let zero = entry(&dom, &list, "zero");
    assert!(zero.transform.is_none() && zero.transform_3d.is_none());
    assert_eq!((zero.rect.x, zero.rect.y), (20.0, 400.0));

    assert!(
        list.skipped_transforms.is_empty(),
        "{:#?}",
        list.skipped_transforms
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

/// A 3D transform on one element is drawn, and the 2D transforms beside it still apply.
///
/// - `#tilt` is 100 by 50 at (100, 100), centre (150, 125). `rotateX(45deg)` with no
///   perspective leaves x alone and takes y to `y cos 45`: the top and bottom edges move
///   to 25 cos 45 = 17.6777 above and below the centre. Its layout box does not change.
/// - `#turn` is 100 by 50 at (300, 100), centre (350, 125), turned a quarter turn, so its
///   corners are (375, 75), (375, 175), (325, 175) and (325, 75).
/// - `#move` is translated 20px, which only moves it to x = 520 and adds no transform.
#[test]
fn fr34_1_a_3d_transform_is_drawn_beside_2d_siblings() {
    let mut dom = HtmlDom::with_config(beside_2d, config(BESIDE_2D));
    let list = dom.layout(800.0, 600.0, 1.0).clone();

    let tilt = entry(&dom, &list, "tilt");
    assert!(tilt.transform.is_none(), "rotateX is not drawn in 2D");
    assert_eq!(
        (tilt.rect.x, tilt.rect.y, tilt.rect.width, tilt.rect.height),
        (100.0, 100.0, 100.0, 50.0)
    );
    let half = 25.0 * 45.0f32.to_radians().cos();
    assert_points(
        &tilt
            .transform_3d
            .as_ref()
            .expect("#tilt is turned about x")
            .corners(&tilt.rect)
            .expect("nothing is behind the viewer"),
        &[
            (100.0, 125.0 - half),
            (200.0, 125.0 - half),
            (200.0, 125.0 + half),
            (100.0, 125.0 + half),
        ],
        "#tilt",
    );
    assert!(
        list.skipped_transforms.is_empty(),
        "{:#?}",
        list.skipped_transforms
    );

    let turn = entry(&dom, &list, "turn");
    assert!(turn.transform_3d.is_none());
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
    assert!(
        moved.transform.is_none() && moved.transform_3d.is_none(),
        "a translation folds into the place"
    );
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

const ONE_BOX: &str = r#"
.box { position: absolute; left: 20px; top: 20px; width: 100px; height: 50px; background-color: rgb(51, 102, 153); }
"#;

fn one_box() -> Element {
    rsx! {
        div { id: "subject", class: "box" }
    }
}

enum Expect {
    /// Kept as one 3D matrix, with no 2D transform, at its layout place.
    ThreeD,
    /// Not 3D and does nothing: no transform, no entry in the list, and the box stays put.
    Untransformed,
    /// A translation folded into the box's place: no transform, no entry, and the box moves
    /// by the given amount.
    Folded(f32, f32),
    /// A flat transform kept as a 2D matrix, with no entry in the list.
    Matrix,
}

/// Every 3D function the layout can be given, one box each: which are a 3D matrix and
/// which are flat.
///
/// A function that is 3D makes the whole transform one 4x4 matrix. A function that is 3D
/// only by its name (a `z` of 0 or a scale of 1 on the z axis, a turn about the z axis) is
/// flat, and is folded into a 2D form or dropped, as the 2D function it reduces to.
///
/// - `rotateX`, `rotateY`, `rotate3d` with an axis that is not the z axis: a turn about x or
///   y is a 3D rotation.
/// - `rotate3d(0, 0, 1, 30deg)` is a turn about z, which is a 2D rotation, so it is a 2D
///   matrix.
/// - `translateZ` and `translate3d` with a non-zero z move the box out of the plane, so they
///   are 3D. With z = 0 (`translateZ(0px)`, or `translate3d(10px, 20px, 0px)`) they reduce
///   to the 2D form: `translateZ(0)` is nothing, `translate3d` is a plain translation.
/// - `scaleZ` and `scale3d` with a z other than 1 scale the depth, so they are 3D.
///   `scaleZ(1)` does nothing, and `scale3d(2, 3, 1)` is the 2D `scale(2, 3)`.
/// - `perspective()` has no 2D form.
/// - `matrix3d()` is 3D when it is not flat (here the z row carries a perspective term of
///   0.001). A flat `matrix3d()` would be a 2D matrix.
/// - A mixed list is one transform: `translate(10px) rotateY(30deg)` has a 3D part, so the
///   whole list is one 3D matrix, the `translate(10px)` inside it. The box stays at x = 20
///   and the matrix carries the 10px.
/// - The `rotate`, `translate` and `scale` properties go through the same checks, one value
///   per property, and a flat value such as `translate: 10px 20px 0` folds into the place
///   like its function does.
///
/// Nothing is reported as skipped in any of them.
#[test]
fn fr34_1_flat_3d_functions_stay_2d_and_the_rest_are_one_3d_matrix() {
    let cases: &[(&str, Expect)] = &[
        ("transform: rotateX(45deg)", Expect::ThreeD),
        ("transform: rotateY(30deg)", Expect::ThreeD),
        ("transform: rotate3d(1, 1, 0, 45deg)", Expect::ThreeD),
        ("transform: rotate3d(0, 0, 1, 30deg)", Expect::Matrix),
        ("transform: translateZ(10px)", Expect::ThreeD),
        ("transform: translateZ(0px)", Expect::Untransformed),
        ("transform: translate3d(10px, 0px, 10px)", Expect::ThreeD),
        (
            "transform: translate3d(10px, 20px, 0px)",
            Expect::Folded(10.0, 20.0),
        ),
        ("transform: scaleZ(2)", Expect::ThreeD),
        ("transform: scaleZ(1)", Expect::Untransformed),
        ("transform: scale3d(2, 2, 2)", Expect::ThreeD),
        ("transform: scale3d(2, 3, 1)", Expect::Matrix),
        ("transform: perspective(100px)", Expect::ThreeD),
        (
            "transform: matrix3d(1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0.001, 0, 0, 0, 1)",
            Expect::ThreeD,
        ),
        ("transform: translate(10px) rotateY(30deg)", Expect::ThreeD),
        // The individual properties are read by the same code as the functions.
        ("rotate: x 45deg", Expect::ThreeD),
        ("rotate: 1 1 0 30deg", Expect::ThreeD),
        ("rotate: 0 0 1 30deg", Expect::Matrix),
        ("translate: 10px 0 5px", Expect::ThreeD),
        ("translate: 10px 20px 0", Expect::Folded(10.0, 20.0)),
        ("scale: 1 1 2", Expect::ThreeD),
        ("scale: 2 3 1", Expect::Matrix),
    ];

    for (css, expect) in cases {
        let style = format!("{ONE_BOX}#subject {{ {css}; }}");
        let mut dom = HtmlDom::with_config(one_box, config(&style));
        let list = dom.layout(800.0, 600.0, 1.0).clone();
        let subject = entry(&dom, &list, "subject");
        let skipped = &list.skipped_transforms;
        let rect = (subject.rect.x, subject.rect.y);

        match expect {
            Expect::ThreeD => {
                assert!(subject.transform.is_none(), "`{css}` has no 2D form");
                let transform = subject
                    .transform_3d
                    .as_ref()
                    .unwrap_or_else(|| panic!("`{css}` is a 3D matrix"));
                assert_eq!(transform.origin, (50.0, 25.0), "`{css}`");
                assert_eq!(rect, (20.0, 20.0), "`{css}` stays at its layout place");
                assert!(skipped.is_empty(), "`{css}` is drawn: {skipped:#?}");
                if *css == "transform: translate(10px) rotateY(30deg)" {
                    // The translation is the last column of the one matrix.
                    assert!(
                        (transform.matrix.0[12] - 10.0).abs() < 1e-5,
                        "{:?}",
                        transform.matrix
                    );
                }
            }
            Expect::Untransformed => {
                assert!(
                    subject.transform.is_none() && subject.transform_3d.is_none(),
                    "`{css}`"
                );
                assert_eq!(rect, (20.0, 20.0), "`{css}`");
                assert!(skipped.is_empty(), "`{css}` is not 3D: {skipped:#?}");
            }
            Expect::Folded(dx, dy) => {
                assert!(
                    subject.transform.is_none() && subject.transform_3d.is_none(),
                    "`{css}` is folded into the place"
                );
                assert_eq!(rect, (20.0 + dx, 20.0 + dy), "`{css}`");
                assert!(skipped.is_empty(), "`{css}` is not 3D: {skipped:#?}");
            }
            Expect::Matrix => {
                assert!(
                    subject.transform.is_some() && subject.transform_3d.is_none(),
                    "`{css}` is drawn as a 2D matrix"
                );
                assert_eq!(rect, (20.0, 20.0), "`{css}` keeps the layout place");
                assert!(skipped.is_empty(), "`{css}` is not 3D: {skipped:#?}");
            }
        }
    }
}

const DEEP: &str = r#"
.box { position: absolute; width: 200px; height: 100px; background-color: rgb(51, 102, 153); }
#deep { left: 100px; top: 100px; transform: perspective(400px) rotateY(45deg); }
"#;

fn deep() -> Element {
    rsx! {
        div { id: "deep", class: "box", onclick: move |_| hit("deep") }
    }
}

/// A pointer is tested against the shape a 3D transform draws.
///
/// `#deep` is 200 by 100 at (100, 100), centre (200, 150), under
/// `perspective(400px) rotateY(45deg)`. A point dx from the centre goes to depth
/// `-dx sin 45`, so it is divided by `w = 1 + dx sin 45 / 400`:
///
/// - the right edge, dx = 100: w = 1.176777, x = 70.7107 / w = 60.088, half height
///   50 / w = 42.489. It is drawn at x = 260.088, from y = 107.511 to 192.489.
/// - the left edge, dx = -100: w = 0.823223, x = -85.895, half height 60.737. It is drawn
///   at x = 114.105, from y = 89.263 to 210.737.
///
/// The top edge is the straight line between (114.105, 89.263) and (260.088, 107.511),
/// which falls 0.125 for each pixel across: y = 90.0 at x = 120 and y = 106.25 at x = 250.
///
/// - (200, 150), the centre, is inside.
/// - (120, 95) is above the layout box (y < 100) and below the drawn top edge, so inside.
/// - (290, 150) is inside the layout box and right of the drawn right edge.
/// - (105, 150) is inside the layout box and left of the drawn left edge.
/// - (250, 103) is inside the layout box and above the drawn top edge.
#[test]
fn fr34_1_3d_hit_test_follows_the_drawn_shape() {
    take_hits();
    let mut dom = HtmlDom::with_config(deep, config(DEEP));
    let list = dom.layout(800.0, 600.0, 1.0).clone();
    let deep = dom.element_by_id("deep").unwrap();

    let (sin, cos) = 45.0f32.to_radians().sin_cos();
    let right = 1.0 + 100.0 * sin / 400.0;
    let left = 1.0 - 100.0 * sin / 400.0;
    let transform = entry(&dom, &list, "deep")
        .transform_3d
        .as_ref()
        .expect("#deep is 3D")
        .clone();
    assert_points(
        &transform
            .corners(&entry(&dom, &list, "deep").rect)
            .expect("nothing is behind the viewer"),
        &[
            (200.0 - 100.0 * cos / left, 150.0 - 50.0 / left),
            (200.0 + 100.0 * cos / right, 150.0 - 50.0 / right),
            (200.0 + 100.0 * cos / right, 150.0 + 50.0 / right),
            (200.0 - 100.0 * cos / left, 150.0 + 50.0 / left),
        ],
        "#deep",
    );
    // The middle of the right edge, and back again.
    let drawn = list.project(deep, 300.0, 150.0).unwrap();
    assert_points(&[drawn], &[(200.0 + 100.0 * cos / right, 150.0)], "project");
    let back = list.local_point(deep, drawn.0, drawn.1).unwrap();
    assert_points(&[back], &[(300.0, 150.0)], "local_point");

    assert_eq!(dom.click(200.0, 150.0), Some(deep));
    assert_eq!(take_hits(), ["deep"]);

    assert_eq!(dom.click(120.0, 95.0), Some(deep));
    assert_eq!(take_hits(), ["deep"]);

    for (x, y) in [(290.0, 150.0), (105.0, 150.0), (250.0, 103.0)] {
        assert_eq!(dom.click(x, y), None, "({x}, {y}) is outside what is drawn");
        assert_eq!(take_hits(), Vec::<&str>::new());
    }
}

const FACES: &str = r#"
.box { position: absolute; top: 100px; width: 100px; height: 50px; background-color: rgb(51, 102, 153); }
#back { left: 100px; transform: rotateY(180deg); backface-visibility: hidden; }
#front { left: 300px; transform: rotateY(30deg); backface-visibility: hidden; }
#shown { left: 500px; transform: rotateY(180deg); }
"#;

fn faces() -> Element {
    rsx! {
        div { id: "back", class: "box", onclick: move |_| hit("back") }
        div { id: "front", class: "box", onclick: move |_| hit("front") }
        div { id: "shown", class: "box", onclick: move |_| hit("shown") }
    }
}

/// `backface-visibility: hidden` goes with the 3D transform, and a box whose hidden back
/// is turned to the viewer is not there: it is marked as not drawn and takes no pointer.
///
/// - `#back`, `rotateY(180deg)`, shows its back and hides it. A click at its centre,
///   (150, 125), reaches nothing.
/// - `#front`, `rotateY(30deg)`, hides its back too, and shows its front: it is drawn (as
///   a box 100 cos 30 = 86.6 wide about x = 350) and a click at (350, 125) reaches it.
/// - `#shown`, `rotateY(180deg)` with the default `backface-visibility: visible`, is drawn
///   mirrored where it was, and a click at (550, 125) reaches it.
///
/// Leaving the box out of the picture is the renderer's part: it is told with the flag on
/// the plan's 3D transform modifier, which is what is asserted here.
#[test]
fn fr34_1_backface_hidden_box_is_not_drawn_from_behind() {
    take_hits();
    let mut dom = HtmlDom::with_config(faces, config(FACES));
    let list = dom.layout(800.0, 600.0, 1.0).clone();
    let back = entry(&dom, &list, "back");
    let front = entry(&dom, &list, "front");
    let shown = entry(&dom, &list, "shown");

    let of = |entry: &NodeEntry| entry.transform_3d.clone().expect("a 3D transform");
    assert!(of(back).backface_hidden && of(back).faces_away());
    assert!(back.is_turned_away());
    assert!(of(front).backface_hidden && !of(front).faces_away());
    assert!(!front.is_turned_away());
    assert!(!of(shown).backface_hidden && of(shown).faces_away());
    assert!(!shown.is_turned_away());

    let plan = dom.plan(&mut LiteralColours).expect("laid out");
    for (entry, hidden) in [(back, true), (front, true), (shown, false)] {
        let node = plan.find(PlanKey::Node(entry.node)).expect("in the plan");
        assert!(
            matches!(
                node.modifier(ModifierSlot::Transform),
                Some(PlanModifier::Transform3D(t)) if t.backface_hidden == hidden
            ),
            "{node:#?}"
        );
    }

    let (front, shown) = (front.node, shown.node);
    assert_ne!(list.hit_test(150.0, 125.0), Some(back.node));
    assert_eq!(dom.click(150.0, 125.0), None, "#back is not there");
    assert_eq!(take_hits(), Vec::<&str>::new());
    assert_eq!(dom.click(350.0, 125.0), Some(front));
    assert_eq!(take_hits(), ["front"]);
    assert_eq!(dom.click(550.0, 125.0), Some(shown));
    assert_eq!(take_hits(), ["shown"]);
}

const SCENE: &str = r#"
#scene {
    position: absolute; left: 100px; top: 100px; width: 200px; height: 100px;
    transform-style: preserve-3d; transform: rotateX(20deg);
    background-color: rgb(0, 0, 128);
}
#card { width: 100px; height: 50px; transform: rotateY(30deg) translateZ(40px); background-color: rgb(255, 200, 0); }
#plain { position: absolute; left: 400px; top: 100px; width: 50px; height: 50px; transform: rotateX(20deg); }
"#;

fn scene() -> Element {
    rsx! {
        div { id: "scene",
            div { id: "card" }
        }
        div { id: "plain" }
    }
}

/// `transform-style: preserve-3d` is drawn flat, and the element that asked for it is
/// named so the developer can be warned (the layout writes the warning to stderr, once for
/// each element; the list is what a test can read).
///
/// `#scene` asks for it and `#plain`, with the same transform and the default
/// `transform-style: flat`, does not, so the list names `#scene` alone. `#card` keeps its
/// own 3D transform and is a child of `#scene`'s node in the plan, placed by its offset in
/// `#scene`'s plane: it is drawn into that plane and then through `#scene`'s transform,
/// which is how `flat` composes. Nothing stops: the layout, the plan and a hit test all
/// answer.
#[test]
fn fr34_1_preserve_3d_warns_and_flattens() {
    let mut dom = HtmlDom::with_config(scene, config(SCENE));
    let list = dom.layout(800.0, 600.0, 1.0).clone();
    let scene = entry(&dom, &list, "scene");
    let card = entry(&dom, &list, "card");

    assert_eq!(
        list.flattened_spaces,
        vec![FlattenedSpace {
            node: scene.node,
            tag: "div".to_string(),
        }]
    );
    assert!(scene.transform_3d.is_some() && card.transform_3d.is_some());
    assert_eq!((card.rect.x, card.rect.y), (100.0, 100.0));

    let plan = dom.plan(&mut LiteralColours).expect("laid out");
    let key = |entry: &NodeEntry| PlanKey::Node(entry.node);
    assert_eq!(plan.parent_of(key(card)).map(|n| n.key), Some(key(scene)));
    for entry in [scene, card] {
        let node = plan.find(key(entry)).unwrap();
        assert!(
            matches!(
                node.modifier(ModifierSlot::Transform),
                Some(PlanModifier::Transform3D(_))
            ),
            "{node:#?}"
        );
    }
    assert_eq!(plan.find(key(card)).unwrap().offset(), (0.0, 0.0));

    // The centre of `#scene` is the origin of its transform, so it is drawn where it lies.
    // It is drawn 50 cos 20 = 46.98 above and below that centre, so (250, 180) is on it,
    // right of `#card`.
    let scene = scene.node;
    assert_points(
        &[list.project(scene, 200.0, 150.0).unwrap()],
        &[(200.0, 150.0)],
        "#scene's centre",
    );
    assert_eq!(list.hit_test(250.0, 180.0), Some(scene));

    // A second layout names the element again and changes nothing else.
    let again = dom.layout(800.0, 600.0, 1.0).clone();
    assert_eq!(again.flattened_spaces, list.flattened_spaces);
}
