//! The motion example laid out in a 900 by 700 viewport.
//!
//! Every number follows from `motion::STYLE` and the support measurer (an advance of half
//! the font size per character, the CSS line height per line):
//!
//! - `.page` has 24px of padding, so its rows start at x = 24 and the first at y = 24. A
//!   row is 120px tall including 20px of padding above and below, so the second starts at
//!   y = 144, the third at 264, the fourth at 384, and the status line at 504. The boxes in
//!   a row are `flex-start` and 80px apart (the `gap`), so they start 20px into their row:
//!   y = 44, 164, 284 and 404.
//! - The five boxes of the first row are 80 by 80, at x = 24, 184, 344, 504 and 664, each
//!   with a 20 by 20 box inside it, 10px from its corner.
//! - In the second row the fader is 80 by 80 at (24, 164) and the toggle button, 100 by 32,
//!   follows it at x = 24 + 80 + 80 = 184. In the third, the spinner is 40 by 40 at (24,
//!   284) and the pulse 80 by 40 at x = 24 + 40 + 80 = 144. In the fourth, the meter is 100
//!   by 24 at (24, 404) and the grow button at x = 24 + 100 + 80 = 204; with the meter at
//!   300px wide it is at x = 24 + 300 + 80 = 404.

use dioxus_compose::html::{
    AnimatedProperty, AnimatedValue, Easing, FillMode, HtmlConfig, HtmlDom, Iterations,
    PlayDirection, Rgba, TransformFunction,
};
use sample_html_motion as motion;
use sample_html_support::{
    Measurer, assert_close, assert_rect, centre, config, entry, node, text_of,
};

const WIDTH: f32 = 900.0;
const HEIGHT: f32 = 700.0;

fn open_with(config: HtmlConfig) -> HtmlDom {
    let mut dom = HtmlDom::with_config(motion::app, config);
    dom.set_time(0.0);
    dom.layout(WIDTH, HEIGHT, 1.0);
    dom
}

fn open() -> (
    HtmlDom,
    std::rc::Rc<std::cell::RefCell<Vec<sample_html_support::Request>>>,
) {
    let measurer = Measurer::new();
    let requests = measurer.requests();
    (open_with(config(motion::STYLE, measurer)), requests)
}

/// Applies what the app changed and lays out at `seconds`.
fn relayout(dom: &mut HtmlDom, seconds: f64) {
    dom.render();
    dom.set_time(seconds);
    dom.layout(WIDTH, HEIGHT, 1.0);
}

#[track_caller]
fn click_centre(dom: &mut HtmlDom, id: &str) {
    let (x, y) = centre(entry(dom, id).rect);
    assert!(
        dom.click(x, y).is_some(),
        "nothing under #{id} listens for clicks"
    );
}

#[track_caller]
fn assert_points(actual: &[(f32, f32)], expected: &[(f32, f32)], what: &str) {
    assert_eq!(actual.len(), expected.len(), "{what}");
    for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (a.0 - e.0).abs() < 0.02 && (a.1 - e.1).abs() < 0.02,
            "{what}: point {index} is ({}, {}), expected ({}, {})",
            a.0,
            a.1,
            e.0,
            e.1
        );
    }
}

/// A point `(dx, dy)` from `centre`, turned clockwise by `degrees` as CSS `rotate()` turns
/// it: `(dx cos a - dy sin a, dx sin a + dy cos a)`.
fn turned(centre: (f32, f32), (dx, dy): (f32, f32), degrees: f32) -> (f32, f32) {
    let (sin, cos) = degrees.to_radians().sin_cos();
    (
        centre.0 + dx * cos - dy * sin,
        centre.1 + dx * sin + dy * cos,
    )
}

#[test]
fn fr34_motion_boxes_keep_the_places_the_rows_give_them() {
    let (dom, _) = open();

    assert_rect(
        entry(&dom, "shapes").rect,
        24.0,
        24.0,
        852.0,
        120.0,
        "the first row",
    );
    for (id, x) in [
        ("rotate", 24.0),
        ("scale", 184.0),
        ("skew", 344.0),
        ("origin", 504.0),
        ("combo", 664.0),
    ] {
        assert_rect(entry(&dom, id).rect, x, 44.0, 80.0, 80.0, id);
        assert_rect(
            entry(&dom, &format!("{id}-inner")).rect,
            x + 10.0,
            54.0,
            20.0,
            20.0,
            &format!("{id}-inner"),
        );
    }
    assert_rect(entry(&dom, "fader").rect, 24.0, 164.0, 80.0, 80.0, "#fader");
    assert_rect(
        entry(&dom, "toggle").rect,
        184.0,
        164.0,
        100.0,
        32.0,
        "#toggle",
    );
    assert_rect(
        entry(&dom, "spinner").rect,
        24.0,
        284.0,
        40.0,
        40.0,
        "#spinner",
    );
    assert_rect(
        entry(&dom, "pulse").rect,
        144.0,
        284.0,
        80.0,
        40.0,
        "#pulse",
    );
    assert_rect(
        entry(&dom, "meter").rect,
        24.0,
        404.0,
        100.0,
        24.0,
        "#meter",
    );
    assert_rect(entry(&dom, "grow").rect, 204.0, 404.0, 100.0, 32.0, "#grow");
    assert_rect(
        entry(&dom, "status").rect,
        24.0,
        504.0,
        852.0,
        20.0,
        "#status",
    );
    assert_eq!(text_of(entry(&dom, "status")), "Last click: nothing");
}

/// Each transform puts the corners of its 80 by 80 box where CSS puts them.
///
/// - `#rotate` turns 15 degrees about its centre, (64, 84).
/// - `#scale` grows to 1.5 times about its centre, (224, 84): 120 by 120, from (164, 24).
/// - `#skew` skews x by tan(10 degrees) times the distance from the centre row (-40 on the
///   top edge, 40 on the bottom one), about its centre (384, 84): 7.053 either way.
/// - `#origin` turns 45 degrees about its own top-left corner (504, 44).
/// - `#combo` is `translate(10px, 5px) rotate(30deg) scale(0.5)` about its centre
///   (704, 84): the box is halved, turned, and then moved. Its corners are at
///   (+-20, +-20) turned 30 degrees, plus (10, 5), from the centre; the extremes of those
///   are 20 * (cos 30 + sin 30) = 27.32 from the middle, so x spans 10 - 27.32 to 10 + 27.32
///   and y spans 5 - 27.32 to 5 + 27.32.
#[test]
fn fr34_1_motion_each_transform_puts_the_corners_where_css_does() {
    let (dom, _) = open();
    let corners = |id: &str| {
        let entry = entry(&dom, id);
        let transform = entry
            .transform
            .as_ref()
            .unwrap_or_else(|| panic!("#{id} has no transform"));
        transform.corners(&entry.rect)
    };

    assert_points(
        &corners("rotate"),
        &[
            turned((64.0, 84.0), (-40.0, -40.0), 15.0),
            turned((64.0, 84.0), (40.0, -40.0), 15.0),
            turned((64.0, 84.0), (40.0, 40.0), 15.0),
            turned((64.0, 84.0), (-40.0, 40.0), 15.0),
        ],
        "#rotate",
    );

    assert_points(
        &corners("scale"),
        &[(164.0, 24.0), (284.0, 24.0), (284.0, 144.0), (164.0, 144.0)],
        "#scale",
    );

    let shear = 10.0f32.to_radians().tan() * 40.0;
    assert_points(
        &corners("skew"),
        &[
            (344.0 - shear, 44.0),
            (424.0 - shear, 44.0),
            (424.0 + shear, 124.0),
            (344.0 + shear, 124.0),
        ],
        "#skew",
    );

    assert_points(
        &corners("origin"),
        &[
            (504.0, 44.0),
            turned((504.0, 44.0), (80.0, 0.0), 45.0),
            turned((504.0, 44.0), (80.0, 80.0), 45.0),
            turned((504.0, 44.0), (0.0, 80.0), 45.0),
        ],
        "#origin",
    );

    let combo = entry(&dom, "combo");
    let bounds = combo.transform.as_ref().unwrap().bounds(&combo.rect);
    let reach = 20.0 * (30.0f32.to_radians().cos() + 30.0f32.to_radians().sin());
    assert_points(
        &[(bounds.x, bounds.y), (bounds.right(), bounds.bottom())],
        &[
            (704.0 + 10.0 - reach, 84.0 + 5.0 - reach),
            (704.0 + 10.0 + reach, 84.0 + 5.0 + reach),
        ],
        "#combo's bounds",
    );
}

/// The small box inside a transformed one is drawn through the same transform: its
/// top-left corner, 30px up and to the left of `#rotate`'s centre, is turned with the rest.
#[test]
fn fr34_1_motion_the_box_inside_a_transformed_box_turns_with_it() {
    let (dom, _) = open();
    let list = dom.display_list().unwrap();
    let inner = node(&dom, "rotate-inner");
    assert!(entry(&dom, "rotate-inner").transform.is_none());

    let top_left = list.project(inner, 34.0, 54.0).unwrap();
    assert_points(
        &[top_left],
        &[turned((64.0, 84.0), (-30.0, -30.0), 15.0)],
        "#rotate-inner's corner",
    );
    let bottom_right = list.project(inner, 54.0, 74.0).unwrap();
    assert_points(
        &[bottom_right],
        &[turned((64.0, 84.0), (-10.0, -10.0), 15.0)],
        "#rotate-inner's far corner",
    );
}

/// A click lands on the shape that is drawn.
///
/// - `#rotate`'s top-right corner is drawn at (112.99, 55.72), past the layout box's right
///   edge (104). (108, 58) is 44 right of and 26 above its centre, which turned back 15
///   degrees is (35.8, -36.5) from the centre: inside the 40 half-size. It hits; (26, 46),
///   inside the layout box but at a corner that has turned away, does not.
/// - `#scale` is drawn from (164, 24), past the layout box (184, 44). (168, 30) is
///   (-56, -54) from the centre, which is (-37.3, -36) at a scale of 1.5: inside.
/// - `#origin` hangs from its top-left corner to the lower left. (480, 100) is (-24, 56)
///   from that corner, which turned back 45 degrees is (22.6, 56.6): inside, and outside
///   the layout box (x < 504).
#[test]
fn fr34_1_motion_a_click_hits_the_drawn_shape_and_misses_the_corner_that_turned_away() {
    let (mut dom, _) = open();

    assert_eq!(dom.click(108.0, 58.0), Some(node(&dom, "rotate")));
    dom.render();
    dom.layout(WIDTH, HEIGHT, 1.0);
    assert_eq!(text_of(entry(&dom, "status")), "Last click: rotate");

    assert_eq!(dom.click(26.0, 46.0), None);

    assert_eq!(dom.click(168.0, 30.0), Some(node(&dom, "scale")));
    dom.render();
    dom.layout(WIDTH, HEIGHT, 1.0);
    assert_eq!(text_of(entry(&dom, "status")), "Last click: scale");

    assert_eq!(dom.click(480.0, 100.0), Some(node(&dom, "origin")));
    dom.render();
    dom.layout(WIDTH, HEIGHT, 1.0);
    assert_eq!(text_of(entry(&dom, "status")), "Last click: origin");
}

/// The toggle button changes the fader's class, and its `opacity`, `background-color` and
/// `transform` transition over 0.4 seconds with `ease`. They are described for the
/// renderer, with the end values in the entry, and the Host has no frames to produce.
#[test]
fn fr34_2_motion_the_toggle_starts_three_transitions_a_renderer_plays() {
    let (mut dom, requests) = open();
    let fader = entry(&dom, "fader");
    assert_eq!(fader.opacity, 0.3);
    assert!(fader.transform.is_none());

    click_centre(&mut dom, "toggle");
    relayout(&mut dom, 1.0);

    let list = dom.display_list().unwrap();
    let fader = node(&dom, "fader");
    let started: Vec<_> = list
        .transitions
        .iter()
        .filter(|start| start.node == fader)
        .map(|start| &start.animation)
        .collect();
    assert_eq!(started.len(), 3, "{:#?}", list.transitions);
    for animation in &started {
        assert_eq!((animation.duration, animation.delay), (0.4, 0.0));
        assert_eq!(animation.easing, Easing::Ease);
        assert_eq!(animation.fill, FillMode::Backwards);
    }
    let find = |property: AnimatedProperty| {
        started
            .iter()
            .find(|animation| animation.property == property)
            .unwrap_or_else(|| panic!("no {property:?} transition"))
    };

    let opacity = find(AnimatedProperty::Opacity);
    assert_eq!(opacity.start(), AnimatedValue::Opacity(0.3));
    assert_eq!(opacity.end(), AnimatedValue::Opacity(1.0));

    // #94a3b8 to #22c55e.
    let colour = find(AnimatedProperty::BackgroundColor);
    assert_eq!(
        colour.start(),
        AnimatedValue::Colour(Rgba::new(148, 163, 184, 255))
    );
    assert_eq!(
        colour.end(),
        AnimatedValue::Colour(Rgba::new(34, 197, 94, 255))
    );

    let slide = find(AnimatedProperty::Transform);
    let AnimatedValue::Transform(from) = slide.start() else {
        panic!("{:?}", slide.start());
    };
    assert!(from.functions.is_empty());
    let AnimatedValue::Transform(to) = slide.end() else {
        panic!("{:?}", slide.end());
    };
    assert!(
        matches!(
            to.functions[..],
            [TransformFunction::Translate(x, y), TransformFunction::Rotate(r)]
                if x == 20.0 && y == 0.0 && (r - 10.0f32.to_radians()).abs() < 1e-4
        ),
        "{:?}",
        to.functions
    );
    assert_eq!(
        (to.matrix.e, to.matrix.f),
        (20.0, 0.0),
        "translate(20px) comes first"
    );
    assert_eq!(to.origin, (40.0, 40.0));

    // The entry has the end values.
    let entry = entry(&dom, "fader");
    assert_eq!(entry.opacity, 1.0);
    assert_eq!(entry.background, Some(Rgba::new(34, 197, 94, 255)));
    assert_eq!(
        entry.rect.x, 24.0,
        "the slide is a transform, not a new place"
    );

    assert!(!dom.needs_frames(), "nothing here changes layout");
    let asked = requests.borrow().len();
    assert!(dom.tick(1.2, WIDTH, HEIGHT, 1.0).is_empty());
    assert_eq!(
        requests.borrow().len(),
        asked,
        "the Host did no work for the frame"
    );
}

/// The two loops are written once, and the Host has nothing to do per frame while a
/// renderer plays them.
#[test]
fn fr34_2_motion_the_loops_are_described_once_and_cost_the_host_nothing() {
    let (mut dom, requests) = open();

    let spin = &entry(&dom, "spinner").animations;
    assert_eq!(spin.len(), 1, "{spin:#?}");
    assert_eq!(spin[0].property, AnimatedProperty::Transform);
    assert_eq!(spin[0].name.as_deref(), Some("spin"));
    assert_eq!((spin[0].duration, spin[0].delay), (2.0, 0.0));
    assert_eq!(spin[0].easing, Easing::Linear);
    assert_eq!(spin[0].iterations, Iterations::Infinite);
    let AnimatedValue::Transform(to) = spin[0].end() else {
        panic!("{:?}", spin[0].end());
    };
    assert_eq!(to.origin, (20.0, 20.0));
    assert!(
        matches!(to.functions[..], [TransformFunction::Rotate(r)] if (r - std::f32::consts::TAU).abs() < 1e-4),
        "{:?}",
        to.functions
    );

    let pulse = entry(&dom, "pulse");
    assert_eq!(pulse.opacity, 0.3, "where the pulse starts");
    assert_eq!(pulse.animations.len(), 1, "{:#?}", pulse.animations);
    let fade = &pulse.animations[0];
    assert_eq!(fade.property, AnimatedProperty::Opacity);
    assert_eq!((fade.duration, fade.delay), (1.0, 0.5));
    assert_eq!(fade.easing, Easing::EaseInOut);
    assert_eq!(fade.iterations, Iterations::Infinite);
    assert_eq!(fade.direction, PlayDirection::Alternate);
    assert_eq!(fade.fill, FillMode::None);
    assert_eq!(fade.start(), AnimatedValue::Opacity(0.3));
    assert_eq!(fade.end(), AnimatedValue::Opacity(1.0));

    assert!(dom.is_animating());
    assert!(!dom.needs_frames());
    let asked = requests.borrow().len();
    assert!(asked > 0);
    assert!(dom.tick(0.5, WIDTH, HEIGHT, 1.0).is_empty());
    assert_eq!(requests.borrow().len(), asked);
}

/// The grow button animates the meter's `width` over 0.3 seconds, linear. A width moves
/// the button beside it, so the Host ticks: at the layout that starts the transition
/// (10.0) the meter has not moved; at 10.15, halfway, it is 200 wide and the button is at
/// x = 24 + 200 + 80 = 304; at 10.4 it is over, 300 wide, with the button at 404, and the
/// Host stops.
///
/// While the Host ticks, the loops of the same page are ticked with it (blitz-dom has one
/// clock for the whole page), so their entries are in the frames too.
#[test]
fn fr34_2_motion_the_width_transition_is_ticked_and_pushes_the_button_along() {
    let (mut dom, requests) = open();
    click_centre(&mut dom, "grow");
    relayout(&mut dom, 10.0);

    assert_close(entry(&dom, "meter").rect.width, 100.0, "no progress yet");
    assert!(dom.needs_frames(), "the Host has frames to produce");
    assert!(dom.display_list().unwrap().transitions.is_empty());

    let half = dom.tick(10.15, WIDTH, HEIGHT, 1.0);
    let moved: Vec<_> = half.changed.iter().map(|entry| entry.node).collect();
    assert!(moved.contains(&node(&dom, "meter")), "{moved:?}");
    assert!(moved.contains(&node(&dom, "grow")), "{moved:?}");
    assert_close(entry(&dom, "meter").rect.width, 200.0, "halfway");
    assert_close(entry(&dom, "grow").rect.x, 304.0, "pushed along");
    assert!(dom.needs_frames());

    let done = dom.tick(10.4, WIDTH, HEIGHT, 1.0);
    assert!(!done.is_empty());
    assert_close(entry(&dom, "meter").rect.width, 300.0, "over");
    assert_close(entry(&dom, "grow").rect.x, 404.0, "pushed to the end");
    assert!(!dom.needs_frames(), "the transition is over");
    assert!(
        !entry(&dom, "spinner").animations.is_empty(),
        "the renderer has the loops again"
    );

    let asked = requests.borrow().len();
    assert!(dom.tick(10.5, WIDTH, HEIGHT, 1.0).is_empty());
    assert_eq!(requests.borrow().len(), asked);
}

/// With the user's setting at `reduce` the stylesheet's own rule turns the motion off: the
/// transitions and the loops do not exist, and the values still change.
#[test]
fn fr34_2_motion_reduced_motion_removes_the_transitions_and_the_loops() {
    let mut dom = open_with(HtmlConfig {
        reduced_motion: true,
        ..config(motion::STYLE, Measurer::new())
    });
    assert!(entry(&dom, "spinner").animations.is_empty());
    assert!(entry(&dom, "pulse").animations.is_empty());

    click_centre(&mut dom, "toggle");
    relayout(&mut dom, 1.0);
    let list = dom.display_list().unwrap();
    assert!(list.transitions.is_empty(), "{:#?}", list.transitions);
    assert_eq!(entry(&dom, "fader").opacity, 1.0);

    click_centre(&mut dom, "grow");
    relayout(&mut dom, 2.0);
    assert_close(entry(&dom, "meter").rect.width, 300.0, "at once");
    assert!(!dom.needs_frames());
}
