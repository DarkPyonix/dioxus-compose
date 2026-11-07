//! CSS transitions and animations on the HTML path.
//!
//! Two kinds of motion reach a renderer differently. `opacity`, `transform` and
//! `background-color` change nothing about layout, so they are described once (a
//! transition as an event of the layout that started it, an `@keyframes` animation on the
//! entry that runs it) and the renderer plays them; the Host has nothing to do per frame.
//! Anything that changes layout (`width` here) is ticked by the Host, which resolves the
//! document at each frame's time and sends the entries that moved.
//!
//! Every expected number follows from the CSS in the fixtures. Times are the clock the test
//! sets with `HtmlDom::set_time`, in seconds, chosen so that every progress below is exact
//! in binary floating point. Text is sized by a measurer that gives every character 10px and
//! every line 20px, and counts the questions it is asked: layout asks it about every piece
//! of text, so a frame that does no layout asks nothing.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    AnimatedProperty, AnimatedValue, Animation, DisplayList, Easing, FillMode, HtmlConfig, HtmlDom,
    Iterations, LiteralColours, ModifierSlot, NodeEntry, NodeId, PlanChange, PlanKey, PlanModifier,
    PlayDirection, Rgba, StepPosition, TextMeasureRequest, TextMeasurer, TextMetrics,
    TransformFunction, TransitionStart, WidthConstraint, diff,
};
use dioxus_core::ScopeId;

struct Counting(Rc<Cell<usize>>);

impl TextMeasurer for Counting {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        self.0.set(self.0.get() + 1);
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

thread_local! {
    static ON: RefCell<HashSet<&'static str>> = RefCell::new(HashSet::new());
}

fn set(id: &'static str, on: bool) {
    ON.with(|set| {
        if on {
            set.borrow_mut().insert(id);
        } else {
            set.borrow_mut().remove(id);
        }
    });
}

fn class_of(id: &'static str) -> &'static str {
    if ON.with(|set| set.borrow().contains(id)) {
        "on"
    } else {
        ""
    }
}

fn open(app: fn() -> Element, css: &str, reduced_motion: bool) -> (HtmlDom, Rc<Cell<usize>>) {
    ON.with(|set| set.borrow_mut().clear());
    let questions = Rc::new(Cell::new(0));
    let mut dom = HtmlDom::with_config(
        app,
        HtmlConfig {
            stylesheets: vec![css.to_string()],
            measurer: Some(Box::new(Counting(questions.clone()))),
            reduced_motion,
            ..HtmlConfig::default()
        },
    );
    dom.set_time(0.0);
    dom.layout(800.0, 600.0, 1.0);
    (dom, questions)
}

/// Applies what the app's state now says and lays out at `seconds`.
fn relayout(dom: &mut HtmlDom, seconds: f64) -> DisplayList {
    dom.virtual_dom_mut().mark_dirty(ScopeId::APP);
    dom.render();
    dom.set_time(seconds);
    dom.layout(800.0, 600.0, 1.0).clone()
}

#[track_caller]
fn node(dom: &HtmlDom, id: &str) -> NodeId {
    dom.element_by_id(id)
        .unwrap_or_else(|| panic!("no element with id {id}"))
}

#[track_caller]
fn entry<'a>(dom: &HtmlDom, list: &'a DisplayList, id: &str) -> &'a NodeEntry {
    list.get(node(dom, id))
        .unwrap_or_else(|| panic!("#{id} has no display list entry"))
}

#[track_caller]
fn transition<'a>(
    dom: &HtmlDom,
    list: &'a DisplayList,
    id: &str,
    property: AnimatedProperty,
) -> &'a Animation {
    let target = node(dom, id);
    let found: Vec<&TransitionStart> = list
        .transitions
        .iter()
        .filter(|start| start.node == target && start.animation.property == property)
        .collect();
    assert_eq!(
        found.len(),
        1,
        "expected one {property:?} transition on #{id}: {:#?}",
        list.transitions
    );
    &found[0].animation
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

const RENDERER_CSS: &str = r#"
html, body { margin: 0; padding: 0; }
#fader { width: 100px; height: 50px; background-color: rgb(255, 0, 0); opacity: 1; transition: opacity 0.5s linear, background-color 1s ease-in; }
#fader.on { opacity: 0.25; background-color: rgb(0, 0, 255); }
#mover { width: 100px; height: 50px; transition: transform 2s cubic-bezier(0.1, 0.2, 0.3, 0.4) 0.25s; }
#mover.on { transform: rotate(90deg); }
#slider { width: 100px; height: 50px; transition: transform 1s; }
#slider.on { transform: translateX(30px); }
#stepper { width: 10px; height: 10px; opacity: 1; transition: opacity 1s steps(4, end); }
#stepper.on { opacity: 0.5; }
@keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
@keyframes pulse { from { opacity: 0.2; } 50% { opacity: 0.6; } to { opacity: 1; } }
#spinner { width: 40px; height: 40px; animation: spin 2s linear infinite; }
#pulse { width: 40px; height: 40px; animation: pulse 1s ease-in-out 0.5s 3 alternate both; }
#label { height: 20px; }
"#;

fn renderer_motion() -> Element {
    rsx! {
        div { id: "fader", class: class_of("fader") }
        div { id: "mover", class: class_of("mover") }
        div { id: "slider", class: class_of("slider") }
        div { id: "stepper", class: class_of("stepper") }
        div { id: "spinner" }
        div { id: "pulse" }
        div { id: "label", "Hello" }
    }
}

/// A transition of `opacity` and `background-color` is described with its duration and
/// time function, and the display list holds the values it lands on, which are the values
/// a browser ends on.
///
/// `#fader` starts at opacity 1 and red. The `on` class sets 0.25 and blue, and the
/// transition list gives opacity 0.5 seconds, linear, and the colour 1 second, `ease-in`,
/// both with no delay.
#[test]
fn fr34_2_opacity_and_colour_transitions_carry_their_time_and_curve() {
    let (mut dom, _) = open(renderer_motion, RENDERER_CSS, false);
    let fader = node(&dom, "fader");
    let first = dom.display_list().unwrap().clone();
    assert_eq!(entry(&dom, &first, "fader").opacity, 1.0);
    assert!(first.transitions.is_empty(), "nothing has changed yet");

    set("fader", true);
    let list = relayout(&mut dom, 1.0);

    let after = entry(&dom, &list, "fader");
    assert_eq!(after.opacity, 0.25, "the end value");
    assert_eq!(after.background, Some(Rgba::new(0, 0, 255, 255)));
    assert!(
        after.animations.is_empty(),
        "a transition is not a keyframes animation"
    );
    assert_eq!(list.transitions.len(), 2, "{:#?}", list.transitions);

    let opacity = transition(&dom, &list, "fader", AnimatedProperty::Opacity);
    assert_eq!(opacity.start(), AnimatedValue::Opacity(1.0));
    assert_eq!(opacity.end(), AnimatedValue::Opacity(0.25));
    assert_eq!(opacity.keyframes.len(), 2);
    assert_eq!((opacity.duration, opacity.delay), (0.5, 0.0));
    assert_eq!(opacity.easing, Easing::Linear);
    assert_eq!(opacity.iterations, Iterations::Count(1.0));
    assert_eq!(opacity.direction, PlayDirection::Normal);
    assert_eq!(opacity.fill, FillMode::Backwards);
    assert_eq!(opacity.name, None);

    let colour = transition(&dom, &list, "fader", AnimatedProperty::BackgroundColor);
    assert_eq!(
        colour.start(),
        AnimatedValue::Colour(Rgba::new(255, 0, 0, 255))
    );
    assert_eq!(
        colour.end(),
        AnimatedValue::Colour(Rgba::new(0, 0, 255, 255))
    );
    assert_eq!((colour.duration, colour.delay), (1.0, 0.0));
    assert_eq!(colour.easing, Easing::EaseIn);

    assert_eq!(list.transitions[0].node, fader);
    assert!(
        !dom.needs_frames(),
        "the renderer plays these; the Host ticks nothing"
    );
}

/// A transform transition carries its delay and its curve. A translation that is
/// transitioned stays a transform instead of moving the box's place, because the renderer
/// has to animate it. The transform is kept as the functions CSS wrote, so a renderer can
/// interpolate them one by one.
///
/// - `#mover`: `transform 2s cubic-bezier(0.1, 0.2, 0.3, 0.4) 0.25s`, from none to a
///   quarter turn about the centre of its 100 by 50 box, (50, 25).
/// - `#slider`: `transform 1s`, which is `ease`, from none to `translateX(30px)`.
/// - `#stepper`: `opacity 1s steps(4, end)`, from 1 to 0.5.
#[test]
fn fr34_2_transform_transitions_keep_the_transform_and_name_the_curve() {
    let (mut dom, _) = open(renderer_motion, RENDERER_CSS, false);
    set("mover", true);
    set("slider", true);
    set("stepper", true);
    let list = relayout(&mut dom, 1.0);

    let mover = entry(&dom, &list, "mover");
    let turned = mover.transform.as_ref().expect("#mover is turned");
    assert!(close(turned.matrix.a, 0.0) && close(turned.matrix.b, 1.0));
    let animation = transition(&dom, &list, "mover", AnimatedProperty::Transform);
    assert_eq!((animation.duration, animation.delay), (2.0, 0.25));
    let Easing::CubicBezier(x1, y1, x2, y2) = animation.easing else {
        panic!("{:?}", animation.easing);
    };
    assert!(close(x1, 0.1) && close(y1, 0.2) && close(x2, 0.3) && close(y2, 0.4));
    let AnimatedValue::Transform(from) = animation.start() else {
        panic!("{:?}", animation.start());
    };
    assert!(from.functions.is_empty(), "from `none`");
    assert_eq!(from.origin, (50.0, 25.0));
    let AnimatedValue::Transform(to) = animation.end() else {
        panic!("{:?}", animation.end());
    };
    assert!(
        matches!(to.functions[..], [TransformFunction::Rotate(r)] if close(r, std::f32::consts::FRAC_PI_2)),
        "{:?}",
        to.functions
    );

    let slider = entry(&dom, &list, "slider");
    assert_eq!(slider.rect.x, 0.0, "not folded into the place");
    let moved = slider.transform.as_ref().expect("#slider is a transform");
    assert_eq!((moved.matrix.e, moved.matrix.f), (30.0, 0.0));
    let animation = transition(&dom, &list, "slider", AnimatedProperty::Transform);
    assert_eq!(animation.easing, Easing::Ease);
    assert_eq!(animation.duration, 1.0);
    let AnimatedValue::Transform(to) = animation.end() else {
        panic!("{:?}", animation.end());
    };
    assert_eq!(to.functions, [TransformFunction::Translate(30.0, 0.0)]);

    let animation = transition(&dom, &list, "stepper", AnimatedProperty::Opacity);
    assert_eq!(
        animation.easing,
        Easing::Steps {
            count: 4,
            position: StepPosition::JumpEnd
        }
    );
    assert_eq!(entry(&dom, &list, "stepper").opacity, 0.5);
}

/// A transition is an event: the plan diff of the layout that started it says so once,
/// beside the modifiers that changed, and a later layout with nothing new sends nothing.
#[test]
fn fr34_2_a_transition_is_reported_once_in_the_plan_diff() {
    let (mut dom, _) = open(renderer_motion, RENDERER_CSS, false);
    let fader = node(&dom, "fader");
    let before = dom.plan(&mut LiteralColours).expect("laid out");

    set("fader", true);
    relayout(&mut dom, 1.0);
    let after = dom.plan(&mut LiteralColours).expect("laid out");
    let changes = diff(&before, &after);

    let mut started: Vec<AnimatedProperty> = changes
        .iter()
        .filter_map(|change| match change {
            PlanChange::AnimationStarted { key, animation } => {
                assert_eq!(*key, PlanKey::Node(fader));
                Some(animation.property)
            }
            _ => None,
        })
        .collect();
    started.sort_by_key(|property| format!("{property:?}"));
    assert_eq!(
        started,
        [AnimatedProperty::BackgroundColor, AnimatedProperty::Opacity]
    );
    assert!(changes.contains(&PlanChange::ModifierSet {
        key: PlanKey::Node(fader),
        slot: ModifierSlot::Alpha,
        modifier: PlanModifier::Alpha(0.25),
    }));
    assert!(changes.contains(&PlanChange::ModifierSet {
        key: PlanKey::Node(fader),
        slot: ModifierSlot::Background,
        modifier: PlanModifier::Background(Rgba::new(0, 0, 255, 255)),
    }));

    relayout(&mut dom, 2.0);
    let later = dom.plan(&mut LiteralColours).expect("laid out");
    assert!(later.transitions.is_empty());
    let changes = diff(&after, &later);
    assert!(changes.is_empty(), "{changes:#?}");
}

/// A keyframes animation is described on the entry that runs it, once per property it
/// changes, with its repeat, direction, delay, fill and time function. The entry's own
/// value is the one the animation starts from.
///
/// - `#spinner`: `spin 2s linear infinite`, a turn about the centre of its 40 by 40 box.
///   The keyframes keep `rotate(0deg)` and `rotate(360deg)` as angles, because as matrices
///   they are the same.
/// - `#pulse`: `pulse 1s ease-in-out 0.5s 3 alternate both` over opacity 0.2, 0.6 at the
///   middle, and 1.
#[test]
fn fr34_2_keyframes_animations_carry_repeat_direction_delay_and_fill() {
    let (dom, _) = open(renderer_motion, RENDERER_CSS, false);
    let list = dom.display_list().unwrap();

    let spinner = entry(&dom, list, "spinner");
    assert_eq!(spinner.animations.len(), 1, "{:#?}", spinner.animations);
    let spin = &spinner.animations[0];
    assert_eq!(spin.property, AnimatedProperty::Transform);
    assert_eq!(spin.name.as_deref(), Some("spin"));
    assert_eq!((spin.duration, spin.delay), (2.0, 0.0));
    assert_eq!(spin.easing, Easing::Linear);
    assert_eq!(spin.iterations, Iterations::Infinite);
    assert_eq!(spin.direction, PlayDirection::Normal);
    assert_eq!(spin.fill, FillMode::None);
    assert!(!spin.paused);
    assert_eq!(
        spin.keyframes.iter().map(|k| k.offset).collect::<Vec<_>>(),
        [0.0, 1.0]
    );
    let AnimatedValue::Transform(from) = spin.start() else {
        panic!("{:?}", spin.start());
    };
    let AnimatedValue::Transform(to) = spin.end() else {
        panic!("{:?}", spin.end());
    };
    assert_eq!(from.origin, (20.0, 20.0));
    assert!(matches!(from.functions[..], [TransformFunction::Rotate(r)] if close(r, 0.0)));
    assert!(
        matches!(to.functions[..], [TransformFunction::Rotate(r)] if close(r, std::f32::consts::TAU)),
        "{:?}",
        to.functions
    );
    assert!(
        close(from.matrix.a, to.matrix.a) && close(from.matrix.b, to.matrix.b),
        "the same matrix: only the angles say it is a full turn"
    );

    let pulse = entry(&dom, list, "pulse");
    assert_eq!(pulse.opacity, 0.2, "where the animation starts");
    assert_eq!(pulse.animations.len(), 1, "{:#?}", pulse.animations);
    let fade = &pulse.animations[0];
    assert_eq!(fade.property, AnimatedProperty::Opacity);
    assert_eq!(fade.name.as_deref(), Some("pulse"));
    assert_eq!((fade.duration, fade.delay), (1.0, 0.5));
    assert_eq!(fade.easing, Easing::EaseInOut);
    assert_eq!(fade.iterations, Iterations::Count(3.0));
    assert_eq!(fade.direction, PlayDirection::Alternate);
    assert_eq!(fade.fill, FillMode::Both);
    assert_eq!(
        fade.keyframes
            .iter()
            .map(|k| (k.offset, k.value.clone()))
            .collect::<Vec<_>>(),
        [
            (0.0, AnimatedValue::Opacity(0.2)),
            (0.5, AnimatedValue::Opacity(0.6)),
            (1.0, AnimatedValue::Opacity(1.0)),
        ]
    );
    assert!(list.transitions.is_empty());
}

/// While the renderer plays the loops above, the Host does nothing per frame: it asks to be
/// ticked by no one, a tick lays nothing out and asks the measurer nothing, and a layout
/// for another reason sends the same description it sent before.
#[test]
fn fr34_2_the_host_does_no_work_per_frame_while_the_renderer_plays() {
    let (mut dom, questions) = open(renderer_motion, RENDERER_CSS, false);
    assert!(dom.is_animating(), "two loops are running");
    assert!(!dom.needs_frames());
    let asked = questions.get();
    assert!(asked > 0, "laying out asks the measurer about the label");

    let ticked = dom.tick(0.5, 800.0, 600.0, 1.0);
    assert!(ticked.is_empty(), "{ticked:#?}");
    assert_eq!(questions.get(), asked, "no layout was done");
    assert_eq!(dom.time(), 0.0, "the Host did not even move its clock");

    dom.set_time(0.75);
    let again = dom.layout_diff(800.0, 600.0, 1.0);
    assert!(
        again.is_empty(),
        "the same description again is no change: {again:#?}"
    );
}

/// A finite animation that has run its course is no longer described, and the entry has the
/// value its fill leaves. `pulse` ends 0.5 + 3 x 1 = 3.5 seconds after it began; it
/// alternates, so its third pass runs forwards and ends on 1, which `both` holds.
#[test]
fn fr34_2_a_finished_animation_is_no_longer_described() {
    let (mut dom, _) = open(renderer_motion, RENDERER_CSS, false);
    let list = relayout(&mut dom, 10.0);
    let pulse = entry(&dom, &list, "pulse");
    assert!(pulse.animations.is_empty(), "{:#?}", pulse.animations);
    assert_eq!(pulse.opacity, 1.0);
    let spinner = entry(&dom, &list, "spinner");
    assert_eq!(spinner.animations.len(), 1, "the infinite loop goes on");
}

const REDUCED_CSS: &str = r#"
html, body { margin: 0; padding: 0; }
#fader { width: 100px; height: 50px; opacity: 1; transition: opacity 0.5s linear; }
#fader.on { opacity: 0.25; }
@media (prefers-reduced-motion: reduce) { #fader { transition: none; } }
@keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
#spinner { width: 40px; height: 40px; }
@media (prefers-reduced-motion: no-preference) { #spinner { animation: spin 2s linear infinite; } }
"#;

fn reduced() -> Element {
    rsx! {
        div { id: "fader", class: class_of("fader") }
        div { id: "spinner" }
    }
}

/// `prefers-reduced-motion` is the user's setting, and the stylesheet decides what it
/// does: with `reduce` the transition is `none` and the loop does not exist; without it
/// they run.
#[test]
fn fr34_2_prefers_reduced_motion_picks_the_rules_the_stylesheet_gives_it() {
    let (mut calm, _) = open(reduced, REDUCED_CSS, true);
    set("fader", true);
    let list = relayout(&mut calm, 1.0);
    assert!(list.transitions.is_empty(), "{:#?}", list.transitions);
    assert_eq!(
        entry(&calm, &list, "fader").opacity,
        0.25,
        "it still changes"
    );
    assert!(entry(&calm, &list, "spinner").animations.is_empty());

    let (mut moving, _) = open(reduced, REDUCED_CSS, false);
    set("fader", true);
    let list = relayout(&mut moving, 1.0);
    assert_eq!(list.transitions.len(), 1, "{:#?}", list.transitions);
    assert_eq!(entry(&moving, &list, "spinner").animations.len(), 1);
}

const LAYOUT_CSS: &str = r#"
html, body { margin: 0; padding: 0; }
#row { display: flex; height: 20px; }
#meter { flex: none; width: 100px; height: 20px; background-color: rgb(1, 2, 3); transition: width 0.5s linear; }
#meter.on { width: 300px; }
#next { flex: none; width: 50px; height: 20px; background-color: rgb(4, 5, 6); }
#label { height: 20px; }
"#;

fn layout_motion() -> Element {
    rsx! {
        div { id: "row",
            div { id: "meter", class: class_of("meter") }
            div { id: "next" }
        }
        div { id: "label", "Hello" }
    }
}

/// A transition of `width` changes layout, so the Host ticks it: each frame resolves the
/// document at the frame's time, lays it out and sends only the boxes that moved.
///
/// `#meter` goes from 100 to 300px in 0.5 seconds, linear, starting at the layout at 100.0.
/// Its sibling `#next` is 50px wide and follows it, so it is at x = the meter's width.
/// Halfway, at 100.25, the meter is 200 wide; at 100.75 the transition is over.
#[test]
fn fr34_2_a_width_transition_is_ticked_by_the_host_and_sends_only_what_moved() {
    let (mut dom, questions) = open(layout_motion, LAYOUT_CSS, false);
    assert!(!dom.needs_frames());

    set("meter", true);
    let start = relayout(&mut dom, 100.0);
    assert_eq!(
        entry(&dom, &start, "meter").rect.width,
        100.0,
        "no progress yet"
    );
    assert_eq!(entry(&dom, &start, "next").rect.x, 100.0);
    assert!(dom.needs_frames(), "the Host has frames to produce");
    assert!(
        start.transitions.is_empty(),
        "a layout motion is not handed to the renderer"
    );
    let planned = dom.plan(&mut LiteralColours).unwrap();

    let half = dom.tick(100.25, 800.0, 600.0, 1.0);
    let mut moved: Vec<NodeId> = half.changed.iter().map(|entry| entry.node).collect();
    moved.sort();
    let mut expected = vec![node(&dom, "meter"), node(&dom, "next")];
    expected.sort();
    assert_eq!(moved, expected, "only the meter and the box it pushes");
    let list = dom.display_list().unwrap();
    assert!(close(entry(&dom, list, "meter").rect.width, 200.0));
    assert!(close(entry(&dom, list, "next").rect.x, 200.0));
    assert!(dom.needs_frames());

    let halfway = dom.plan(&mut LiteralColours).unwrap();
    let changes = diff(&planned, &halfway);
    let mut touched: Vec<(PlanKey, ModifierSlot)> = changes
        .iter()
        .map(|change| match change {
            PlanChange::ModifierSet { key, slot, .. } => (*key, *slot),
            other => panic!("unexpected {other:#?}"),
        })
        .collect();
    touched.sort_by_key(|(key, _)| *key);
    let mut wanted = vec![
        (
            PlanKey::Node(node(&dom, "meter")),
            ModifierSlot::RequiredSize,
        ),
        (PlanKey::Node(node(&dom, "next")), ModifierSlot::Offset),
    ];
    wanted.sort_by_key(|(key, _)| *key);
    assert_eq!(touched, wanted, "{changes:#?}");

    let done = dom.tick(100.75, 800.0, 600.0, 1.0);
    assert_eq!(done.changed.len(), 2);
    let list = dom.display_list().unwrap();
    assert!(close(entry(&dom, list, "meter").rect.width, 300.0));
    assert!(close(entry(&dom, list, "next").rect.x, 300.0));
    assert!(!dom.needs_frames(), "the transition is over");

    let asked = questions.get();
    let idle = dom.tick(101.0, 800.0, 600.0, 1.0);
    assert!(idle.is_empty());
    assert_eq!(questions.get(), asked, "and the Host is idle again");
}
