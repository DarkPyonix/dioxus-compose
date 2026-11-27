//! CSS transitions and `@keyframes` animations reach the renderer as
//! `StartAnimation` records, and the renderer's reports come back as DOM events.
//!
//! Each test runs an app through a Host the way a window runs it and reads the records the
//! renderer would receive. The display-list side (what a transition or an animation is made
//! of) is `fr34_2_motion.rs`; this file is about what is sent. Text is sized by a measurer
//! that gives every character 10px and every line 20px, and the renderer has not reported a
//! window size, so pages are laid out at 800 by 600.

use std::cell::RefCell;

use dioxus_compose::Host;
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    HtmlConfig, TextMeasureRequest, TextMeasurer, TextMetrics, WidthConstraint,
};
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{
    AnimatedProperty, AnimationControl, AnimationEventKind, AnimationEvents, Color,
    ColorInterpolation, EventPayload, FillMode, KeyframeValue, Modifier, Paint, PlayState,
    PlaybackDirection, PropertyKind, ReducedMotion, StepPosition, Timing, TransformFunctionKind,
    WidgetKind,
};
use dioxus_core::Attribute;
use dioxus_elements::{AnimationEvent, TransitionEvent};
use dioxus_hooks::use_signal;

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

const CSS: &str = r#"
html, body { margin: 0; padding: 0; }
#fader { width: 100px; height: 50px; background-color: rgb(255, 0, 0); transition: opacity 0.5s linear; }
@keyframes pulse { from { opacity: 0.2; } to { opacity: 1; } }
#pulse { width: 40px; height: 40px; animation: pulse 2s ease-in-out 0.5s infinite alternate both; }
@keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
#spinner { width: 100px; height: 50px; animation: spin 1s linear; }
#toggled { width: 40px; height: 40px; }
#toggled.on { animation: pulse 1s linear infinite; }
#late { width: 100px; height: 50px; transition: opacity 1s linear -0.25s; }
#wrap { width: 100px; height: 100px; }
#child { width: 40px; height: 40px; transition: opacity 0.5s linear; }
"#;

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![CSS.to_string()],
        measurer: Some(Box::new(FixedAdvance)),
        ..HtmlConfig::default()
    }
}

fn owned(batch: &[u8]) -> Vec<Mutation<'static>> {
    // Leaked: a test keeps a few small batches past the call that produced them.
    let bytes: &'static [u8] = Box::leak(batch.to_vec().into_boxed_slice());
    decode_batch(bytes).expect("the batch decodes")
}

fn started(batch: &[Mutation<'static>]) -> Vec<dioxus_compose::schema::Animation<'static>> {
    batch
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::StartAnimation(animation) => Some(animation.clone()),
            _ => None,
        })
        .collect()
}

fn has_modifier(
    batch: &[Mutation<'static>],
    node: u32,
    wanted: impl Fn(&Modifier) -> bool,
) -> bool {
    batch.iter().any(|mutation| {
        matches!(mutation, Mutation::SetModifier { node_id, modifier, .. }
            if *node_id == node && wanted(modifier))
    })
}

#[track_caller]
fn clickable(batch: &[Mutation<'static>]) -> (u32, u64) {
    let found: Vec<(u32, u64)> = batch
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier {
                node_id,
                modifier: Modifier::Clickable { handler_id },
                ..
            } => Some((*node_id, *handler_id)),
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1, "one clickable node: {batch:#?}");
    found[0]
}

fn click(host: &mut Host, (node_id, handler_id): (u32, u64)) -> Vec<Mutation<'static>> {
    let (batch, _) = host
        .dispatch(HostEvent::new(node_id, handler_id, EventPayload::Clicked))
        .expect("the click is dispatched");
    owned(batch)
}

fn fader() -> Element {
    let mut presses = use_signal(|| 0u32);
    let opacity = if presses() % 2 == 1 { "0.25" } else { "1" };
    rsx! {
        div {
            id: "fader",
            style: "opacity: {opacity}",
            onclick: move |_| presses += 1,
            ontransitionend: move |event| {
                SEEN.with(|seen| {
                    seen.borrow_mut().push(format!("{:?}", event.data()));
                });
            },
        }
    }
}

thread_local! {
    static SEEN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// A change of `opacity` under a `transition` is one `StartAnimation` in the batch of the
/// frame that changed it, none in the first frame, and none in the frames after.
///
/// `#fader` has `transition: opacity 0.5s linear` and goes from opacity 1 to 0.25 on a
/// click. The record plays on the box of the element, in slot 0, from the value it shows
/// when it starts (`from_presented`, with 1 as the fallback) to 0.25, once, with a
/// backwards fill. Going back to opacity 1, the initial value, the box keeps its `Alpha`
/// modifier, because the renderer refuses an animation of a property the node has no
/// modifier for.
#[test]
fn fr34_2_a_transition_is_sent_as_one_start_animation() {
    let mut host = Host::html(fader, config);
    let first = owned(host.rebuild().unwrap());
    assert!(
        started(&first).is_empty(),
        "nothing begins in the first frame"
    );
    let target = clickable(&first);

    let changed = click(&mut host, target);
    let sent = started(&changed);
    assert_eq!(sent.len(), 1, "one transition: {changed:#?}");
    let animation = &sent[0];
    assert_eq!(animation.node_id, target.0);
    assert_eq!(animation.property, AnimatedProperty::Alpha);
    assert_eq!(animation.slot, 0);
    assert_eq!(animation.duration_ms, 500.0);
    assert_eq!(animation.delay_ms, 0.0);
    assert_eq!(animation.iterations, 1.0);
    assert_eq!(animation.direction, PlaybackDirection::Normal);
    assert_eq!(animation.fill, FillMode::Backwards);
    assert_eq!(animation.play_state, PlayState::Running);
    assert_eq!(animation.start_time_nanos, 0);
    assert_eq!(
        animation.events,
        AnimationEvents::END,
        "the page listens to transitionend, so the end is reported"
    );
    let keyframes = &animation.keyframes;
    assert_eq!(keyframes.len(), 2);
    assert_eq!(keyframes[0].offset, 0.0);
    assert_eq!(keyframes[0].value, KeyframeValue::Alpha(1.0));
    assert!(keyframes[0].from_presented);
    assert_eq!(keyframes[0].timing, Timing::Linear);
    assert_eq!(keyframes[1].offset, 1.0);
    assert_eq!(keyframes[1].value, KeyframeValue::Alpha(0.25));
    assert!(
        has_modifier(&changed, target.0, |m| *m == Modifier::Alpha(0.25)),
        "the underlying value is the target: {changed:#?}"
    );

    let after = click(&mut host, target);
    assert_eq!(started(&after).len(), 1, "the way back is a transition too");
    assert!(
        has_modifier(&after, target.0, |m| *m == Modifier::Alpha(1.0)),
        "opacity 1 is the initial value, but the box keeps an opacity to animate: {after:#?}"
    );
}

/// The renderer's end of a transition is `transitionend` on the element, with the
/// property's CSS name and the time it ran in seconds.
#[test]
fn fr34_2_the_end_of_a_transition_reaches_the_page_as_transitionend() {
    SEEN.with(|seen| seen.borrow_mut().clear());
    let mut host = Host::html(fader, config);
    let first = owned(host.rebuild().unwrap());
    let target = clickable(&first);
    let changed = click(&mut host, target);
    let animation = started(&changed).remove(0);

    host.dispatch(HostEvent::new(
        target.0,
        0,
        EventPayload::AnimationEvent {
            animation_id: animation.animation_id,
            kind: AnimationEventKind::End,
            property: AnimatedProperty::Alpha,
            slot: 0,
            iteration: 0,
            elapsed_ms: 500.0,
            time_nanos: 1,
        },
    ))
    .expect("the event is dispatched");
    SEEN.with(|seen| {
        let seen = seen.borrow();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert!(seen[0].contains("property_name: \"opacity\""), "{seen:?}");
        assert!(seen[0].contains("elapsed_time: 0.5"), "{seen:?}");
    });
}

fn pulse() -> Element {
    rsx! { div { id: "pulse" } }
}

/// An `@keyframes` animation is sent when its box first appears: one record per property it
/// changes, in the slot after the transition's, with the CSS delay, repeat, direction and
/// fill, the timing function on every keyframe but the last, and no events because the
/// page listens to none.
#[test]
fn fr34_2_a_keyframes_animation_is_sent_with_its_repeat_direction_delay_and_fill() {
    let mut host = Host::html(pulse, config);
    let first = owned(host.rebuild().unwrap());
    let sent = started(&first);
    assert_eq!(sent.len(), 1, "{first:#?}");
    let animation = &sent[0];
    assert_eq!(animation.property, AnimatedProperty::Alpha);
    assert_eq!(animation.slot, 1);
    assert_eq!(animation.duration_ms, 2000.0);
    assert_eq!(animation.delay_ms, 500.0);
    assert!(animation.iterations.is_infinite());
    assert_eq!(animation.direction, PlaybackDirection::Alternate);
    assert_eq!(animation.fill, FillMode::Both);
    assert_eq!(animation.events, AnimationEvents::NONE);
    assert_eq!(animation.keyframes.len(), 2);
    assert_eq!(animation.keyframes[0].value, KeyframeValue::Alpha(0.2));
    assert_eq!(animation.keyframes[1].value, KeyframeValue::Alpha(1.0));
    assert!(!animation.keyframes[0].from_presented);
    assert_eq!(
        animation.keyframes[0].timing,
        Timing::CubicBezier {
            x1: 0.42,
            y1: 0.0,
            x2: 0.58,
            y2: 1.0
        }
    );
    assert_eq!(animation.keyframes[1].timing, Timing::Linear);
    assert!(
        has_modifier(&first, animation.node_id, |m| matches!(
            m,
            Modifier::Alpha(_)
        )),
        "the box has an opacity to play on: {first:#?}"
    );
}

fn spinner() -> Element {
    rsx! {
        div {
            id: "spinner",
            onanimationstart: move |event| log("start", &event.animation_name(), event.elapsed_time()),
            onanimationend: move |event| log("end", &event.animation_name(), event.elapsed_time()),
        }
    }
}

fn log(kind: &str, name: &str, elapsed: f32) {
    SPINS.with(|spins| {
        spins
            .borrow_mut()
            .push((kind.to_string(), name.to_string(), elapsed));
    });
}

thread_local! {
    static SPINS: RefCell<Vec<(String, String, f32)>> = const { RefCell::new(Vec::new()) };
}

/// A transform animation sends its functions in degrees with the origin as a fraction of
/// the box, and a page that listens to `animationstart` and `animationend` gets those two
/// moments, with the animation's `@keyframes` name and its active time in seconds.
#[test]
fn fr34_2_a_transform_animation_is_sent_in_degrees_and_its_moments_come_back() {
    SPINS.with(|spins| spins.borrow_mut().clear());
    let mut host = Host::html(spinner, config);
    let first = owned(host.rebuild().unwrap());
    let sent = started(&first);
    assert_eq!(sent.len(), 1, "{first:#?}");
    let animation = &sent[0];
    assert_eq!(animation.property, AnimatedProperty::Transform);
    assert_eq!(animation.duration_ms, 1000.0);
    assert_eq!(animation.iterations, 1.0);
    assert_eq!((animation.origin_x, animation.origin_y), (0.5, 0.5));
    assert_eq!(
        animation.events,
        AnimationEvents::ACTIVE.union(AnimationEvents::END)
    );
    let degrees = |index: usize| match &animation.keyframes[index].value {
        KeyframeValue::Transform(functions) => {
            assert_eq!(functions.len(), 1);
            assert_eq!(functions[0].kind, TransformFunctionKind::Rotate);
            functions[0].values[0]
        }
        other => panic!("a transform keyframe, not {other:?}"),
    };
    assert!((degrees(0) - 0.0).abs() < 1e-3);
    assert!((degrees(1) - 360.0).abs() < 1e-2);
    assert!(
        has_modifier(&first, animation.node_id, |m| matches!(
            m,
            Modifier::Transform { .. }
        )),
        "the box has a transform to play on: {first:#?}"
    );

    for (kind, elapsed_ms) in [
        (AnimationEventKind::Active, 0.0),
        (AnimationEventKind::End, 1000.0),
    ] {
        host.dispatch(HostEvent::new(
            animation.node_id,
            0,
            EventPayload::AnimationEvent {
                animation_id: animation.animation_id,
                kind,
                property: AnimatedProperty::Transform,
                slot: animation.slot,
                iteration: 0,
                elapsed_ms,
                time_nanos: 1,
            },
        ))
        .expect("the event is dispatched");
    }
    SPINS.with(|spins| {
        assert_eq!(
            *spins.borrow(),
            vec![
                ("start".to_string(), "spin".to_string(), 0.0),
                ("end".to_string(), "spin".to_string(), 1.0),
            ]
        );
    });
}

fn toggled() -> Element {
    let mut presses = use_signal(|| 0u32);
    let class = if presses() % 2 == 1 { "on" } else { "" };
    rsx! { div { id: "toggled", class: "{class}", onclick: move |_| presses += 1 } }
}

/// An animation that starts when a class is added is sent in that frame, and cancelled
/// in the frame that takes the class off again, by the id it was started under.
#[test]
fn fr34_2_an_animation_that_goes_away_is_cancelled() {
    let mut host = Host::html(toggled, config);
    let first = owned(host.rebuild().unwrap());
    assert!(started(&first).is_empty());
    let target = clickable(&first);

    let on = click(&mut host, target);
    let sent = started(&on);
    assert_eq!(sent.len(), 1, "{on:#?}");
    assert_eq!(sent[0].node_id, target.0);

    let off = click(&mut host, target);
    assert!(started(&off).is_empty());
    let cancelled: Vec<_> = off
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::ControlAnimation {
                node_id,
                animation_id,
                property,
                slot,
                op,
                ..
            } => Some((*node_id, *animation_id, *property, *slot, *op)),
            _ => None,
        })
        .collect();
    assert_eq!(
        cancelled,
        vec![(
            target.0,
            sent[0].animation_id,
            AnimatedProperty::Alpha,
            sent[0].slot,
            AnimationControl::Cancel
        )],
        "{off:#?}"
    );
}

// The events the Host fires itself: `transitionrun`, `transitioncancel` and
// `animationcancel`. The renderer reports the rest, and `transitionstart` is the renderer's
// Active moment on a transition.

thread_local! {
    static MOMENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn note(line: String) {
    MOMENTS.with(|moments| moments.borrow_mut().push(line));
}

fn moments() -> Vec<String> {
    MOMENTS.with(|moments| moments.borrow().clone())
}

fn transition_line(kind: &str, event: &TransitionEvent) -> String {
    let data = event
        .data()
        .downcast::<TransitionEventData>()
        .expect("the Host sent this event, so it carries the Host's data")
        .clone();
    assert!(
        data.elapsed_time().is_sign_positive(),
        "elapsedTime is never negative zero: {kind} {}",
        data.elapsed_time()
    );
    format!(
        "{kind} {} {:?} [{}]",
        data.property_name(),
        data.elapsed_time(),
        data.pseudo_element()
    )
}

/// What `transition_line` gives for a transition of `opacity`.
fn expected(kind: &str, elapsed: f32) -> String {
    format!("{kind} opacity {elapsed:?} []")
}

fn animation_line(kind: &str, event: &AnimationEvent) -> String {
    format!(
        "{kind} {} {} [{}]",
        event.animation_name(),
        event.elapsed_time(),
        event.pseudo_element()
    )
}

/// The three transition events `rsx!` has no attribute for, as listeners to spread into an
/// element.
fn transition_listeners() -> Vec<Attribute> {
    vec![
        ontransitionrun(|event| note(transition_line("run", &event))),
        ontransitionstart(|event| note(transition_line("start", &event))),
        ontransitioncancel(|event| note(transition_line("cancel", &event))),
    ]
}

fn watched() -> Element {
    let mut presses = use_signal(|| 0u32);
    let opacity = if presses() % 2 == 1 { "0.25" } else { "1" };
    rsx! {
        div {
            id: "fader",
            style: "opacity: {opacity}",
            onclick: move |_| presses += 1,
            ontransitionend: move |event| note(transition_line("end", &event)),
            ..transition_listeners(),
        }
    }
}

fn renderer_says(host: &mut Host, node_id: u32, animation_id: u32, kind: AnimationEventKind) {
    host.dispatch(HostEvent::new(
        node_id,
        0,
        EventPayload::AnimationEvent {
            animation_id,
            kind,
            property: AnimatedProperty::Alpha,
            slot: 0,
            iteration: 0,
            elapsed_ms: 0.0,
            time_nanos: 1,
        },
    ))
    .expect("the event is dispatched");
}

/// `transitionrun` is the Host's own event: it fires in the frame that sends the
/// transition, before the renderer reports `transitionstart`, and `transitionend` follows.
/// Each carries the CSS property name, the elapsed time and an empty pseudo element.
#[test]
fn fr34_2_transitionrun_comes_before_transitionstart_and_transitionend() {
    MOMENTS.with(|moments| moments.borrow_mut().clear());
    let mut host = Host::html(watched, config);
    let first = owned(host.rebuild().unwrap());
    assert!(moments().is_empty(), "nothing runs in the first frame");
    let target = clickable(&first);

    let changed = click(&mut host, target);
    let animation = started(&changed).remove(0);
    assert_eq!(
        animation.events,
        AnimationEvents::ACTIVE.union(AnimationEvents::END),
        "the page listens to transitionstart, transitionend and transitioncancel"
    );
    assert_eq!(
        moments(),
        vec![expected("run", 0.0)],
        "the run fires when the transition is sent, with no help from the renderer"
    );

    renderer_says(
        &mut host,
        target.0,
        animation.animation_id,
        AnimationEventKind::Active,
    );
    renderer_says(
        &mut host,
        target.0,
        animation.animation_id,
        AnimationEventKind::End,
    );
    assert_eq!(
        moments(),
        vec![
            expected("run", 0.0),
            expected("start", 0.0),
            expected("end", 0.0)
        ]
    );
}

fn late() -> Element {
    let mut presses = use_signal(|| 0u32);
    let opacity = if presses() % 2 == 1 { "0.25" } else { "1" };
    rsx! {
        div {
            id: "late",
            style: "opacity: {opacity}",
            onclick: move |_| presses += 1,
            ..transition_listeners(),
        }
    }
}

/// A transition with a negative delay has already run for that long when it is sent, so
/// `transitionrun` carries it as `elapsedTime`.
#[test]
fn fr34_2_transitionrun_carries_the_time_a_negative_delay_has_already_run() {
    MOMENTS.with(|moments| moments.borrow_mut().clear());
    let mut host = Host::html(late, config);
    let first = owned(host.rebuild().unwrap());
    let target = clickable(&first);
    click(&mut host, target);
    assert_eq!(moments(), vec![expected("run", 0.25)]);
}

/// A transition that is replaced by another of the same property before it ends is
/// cancelled, and the cancel comes before the replacement's run.
#[test]
fn fr34_2_a_replaced_transition_is_cancelled_before_the_new_one_runs() {
    MOMENTS.with(|moments| moments.borrow_mut().clear());
    let mut host = Host::html(watched, config);
    let first = owned(host.rebuild().unwrap());
    let target = clickable(&first);
    click(&mut host, target);
    click(&mut host, target);
    assert_eq!(
        moments(),
        vec![
            expected("run", 0.0),
            expected("cancel", 0.0),
            expected("run", 0.0)
        ]
    );
}

/// A transition that has ended is not cancelled when the next one replaces it.
#[test]
fn fr34_2_no_transitioncancel_after_the_transition_ended() {
    MOMENTS.with(|moments| moments.borrow_mut().clear());
    let mut host = Host::html(watched, config);
    let first = owned(host.rebuild().unwrap());
    let target = clickable(&first);
    let changed = click(&mut host, target);
    let animation = started(&changed).remove(0);
    renderer_says(
        &mut host,
        target.0,
        animation.animation_id,
        AnimationEventKind::End,
    );
    click(&mut host, target);
    assert_eq!(
        moments(),
        vec![
            expected("run", 0.0),
            expected("end", 0.0),
            expected("run", 0.0)
        ]
    );
}

fn removing() -> Element {
    let mut step = use_signal(|| 0u32);
    let opacity = if step() == 0 { "1" } else { "0.25" };
    rsx! {
        div {
            id: "wrap",
            onclick: move |_| step += 1,
            ontransitionend: move |event| note(transition_line("end", &event)),
            ..transition_listeners(),
            if step() < 2 {
                div { id: "child", style: "opacity: {opacity}" }
            }
        }
    }
}

/// Taking the element away while its transition runs cancels the transition, but only on
/// the element's own handlers, which go with it. A browser does not bubble an event from a
/// detached element, so the element around it never hears the cancel.
#[test]
fn fr34_2_removing_the_node_sends_no_cancel_to_its_ancestors() {
    MOMENTS.with(|moments| moments.borrow_mut().clear());
    let mut host = Host::html(removing, config);
    let first = owned(host.rebuild().unwrap());
    let target = clickable(&first);
    click(&mut host, target);
    assert_eq!(moments(), vec![expected("run", 0.0)]);
    click(&mut host, target);
    assert_eq!(moments(), vec![expected("run", 0.0)]);
}

/// Once a transition ended, removing its node cancels nothing.
#[test]
fn fr34_2_no_cancel_when_a_node_is_removed_after_its_transition_ended() {
    MOMENTS.with(|moments| moments.borrow_mut().clear());
    let mut host = Host::html(removing, config);
    let first = owned(host.rebuild().unwrap());
    let target = clickable(&first);
    let changed = click(&mut host, target);
    let animation = started(&changed).remove(0);
    renderer_says(
        &mut host,
        animation.node_id,
        animation.animation_id,
        AnimationEventKind::End,
    );
    click(&mut host, target);
    assert_eq!(
        moments(),
        vec![expected("run", 0.0), expected("end", 0.0)],
        "the end is the last word on the transition"
    );
}

fn pulsing() -> Element {
    let mut presses = use_signal(|| 0u32);
    let class = if presses() % 2 == 1 { "on" } else { "" };
    rsx! {
        div {
            id: "toggled",
            class: "{class}",
            onclick: move |_| presses += 1,
            onanimationstart: move |event| note(animation_line("start", &event)),
            onanimationend: move |event| note(animation_line("end", &event)),
            ..vec![onanimationcancel(|event| note(animation_line("cancel", &event)))],
        }
    }
}

/// A `@keyframes` animation taken off the element before it ends is cancelled, with the
/// `@keyframes` name. Animations have no run event.
#[test]
fn fr34_2_taking_a_keyframes_animation_off_fires_animationcancel() {
    MOMENTS.with(|moments| moments.borrow_mut().clear());
    let mut host = Host::html(pulsing, config);
    let first = owned(host.rebuild().unwrap());
    let target = clickable(&first);
    let on = click(&mut host, target);
    let animation = started(&on).remove(0);
    assert_eq!(
        animation.events,
        AnimationEvents::ACTIVE.union(AnimationEvents::END),
        "a page that listens to animationcancel has the end reported"
    );
    assert!(moments().is_empty(), "an animation has no run event");
    renderer_says(
        &mut host,
        target.0,
        animation.animation_id,
        AnimationEventKind::Active,
    );
    click(&mut host, target);
    assert_eq!(moments(), vec!["start pulse 0 []", "cancel pulse 0 []"]);
}

/// An animation the renderer reported as over is not cancelled when it is taken off.
#[test]
fn fr34_2_no_animationcancel_after_the_animation_ended() {
    MOMENTS.with(|moments| moments.borrow_mut().clear());
    let mut host = Host::html(pulsing, config);
    let first = owned(host.rebuild().unwrap());
    let target = clickable(&first);
    let on = click(&mut host, target);
    let animation = started(&on).remove(0);
    renderer_says(
        &mut host,
        target.0,
        animation.animation_id,
        AnimationEventKind::End,
    );
    click(&mut host, target);
    assert_eq!(moments(), vec!["end pulse 0 []"]);
}

const COLOR_SEND_CSS: &str = r#"
html, body { margin: 0; padding: 0; }
#ink { width: 100px; height: 30px; color: rgb(255, 0, 0); transition: color 0.5s linear; }
@keyframes glow { from { color: rgb(255, 0, 0); } to { color: rgb(0, 0, 255); } }
#glow { width: 100px; height: 30px; animation: glow 1s linear infinite; }
"#;

fn color_send_config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![COLOR_SEND_CSS.to_string()],
        measurer: Some(Box::new(FixedAdvance)),
        ..HtmlConfig::default()
    }
}

fn color_send_app() -> Element {
    let mut presses = use_signal(|| 0u32);
    let color = if presses() % 2 == 1 {
        "rgb(0, 0, 255)"
    } else {
        "rgb(255, 0, 0)"
    };
    rsx! {
        div {
            id: "ink",
            style: "color: {color}",
            onclick: move |_| presses += 1,
            ontransitionend: move |event| {
                SEEN.with(|seen| {
                    seen.borrow_mut().push(format!("{:?}", event.data()));
                });
            },
            "Hello"
            span { style: "color: rgb(0, 128, 0)", "Fixed" }
        }
        div { id: "glow", "Pulse" }
    }
}

fn last_color_prop(batch: &[Mutation<'static>], node: u32) -> Option<PropertyValue<'static>> {
    batch.iter().rev().find_map(|mutation| match mutation {
        Mutation::SetProp {
            node_id,
            property: PropertyKind::Color,
            value,
        } if *node_id == node => Some(value.clone()),
        _ => None,
    })
}

fn text_children(batch: &[Mutation<'static>], parent: u32) -> Vec<u32> {
    let text_nodes: Vec<u32> = batch
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::Create {
                node_id,
                widget: WidgetKind::Text,
            } => Some(*node_id),
            _ => None,
        })
        .collect();
    batch
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::Insert {
                parent_id, node_id, ..
            } if *parent_id == parent && text_nodes.contains(node_id) => Some(*node_id),
            _ => None,
        })
        .collect()
}

/// `color` transitions and `@keyframes` animations reach the renderer as `StartAnimation`
/// records with `AnimatedProperty::Color` and premultiplied sRGB interpolation, setting the
/// container's `Color` property so inheriting text children follow it without per-frame Host
/// work.
#[test]
fn fr34_2_a_color_transition_and_keyframes_animation_are_sent_to_the_renderer() {
    SEEN.with(|seen| seen.borrow_mut().clear());
    let mut host = Host::html(color_send_app, color_send_config);
    let first = owned(host.rebuild().unwrap());
    let target = clickable(&first);

    let initial = started(&first);
    assert_eq!(
        initial.len(),
        1,
        "one keyframes color animation: {first:#?}"
    );
    let glow = &initial[0];
    assert_eq!(glow.property, AnimatedProperty::Color);
    assert_eq!(glow.slot, 1);
    assert_eq!(glow.duration_ms, 1000.0);
    assert!(glow.iterations.is_infinite());
    assert_eq!(
        glow.interpolation,
        Some(ColorInterpolation::SrgbPremultiplied)
    );
    assert_eq!(glow.keyframes.len(), 2);
    assert_eq!(
        glow.keyframes[0].value,
        KeyframeValue::Paint(Paint::Literal(Color::argb(0xFFFF_0000)))
    );
    assert_eq!(
        glow.keyframes[1].value,
        KeyframeValue::Paint(Paint::Literal(Color::argb(0xFF00_00FF)))
    );
    assert_eq!(
        last_color_prop(&first, glow.node_id),
        Some(PropertyValue::Integer(
            Paint::Literal(Color::argb(0xFFFF_0000)).to_bits() as i64
        )),
        "the animated container carries its steady colour: {first:#?}"
    );
    let glow_texts = text_children(&first, glow.node_id);
    assert_eq!(
        glow_texts.len(),
        1,
        "one text child inside #glow: {first:#?}"
    );
    assert_eq!(
        last_color_prop(&first, glow_texts[0]),
        None,
        "inheriting text omits its own Color prop so it follows the container's animation"
    );

    let changed = click(&mut host, target);
    let sent = started(&changed);
    assert_eq!(sent.len(), 1, "one color transition: {changed:#?}");
    let trans = &sent[0];
    assert_eq!(trans.node_id, target.0);
    assert_eq!(trans.property, AnimatedProperty::Color);
    assert_eq!(trans.slot, 0);
    assert_eq!(trans.duration_ms, 500.0);
    assert_eq!(
        trans.interpolation,
        Some(ColorInterpolation::SrgbPremultiplied)
    );
    assert!(trans.keyframes[0].from_presented);
    assert_eq!(
        trans.keyframes[0].value,
        KeyframeValue::Paint(Paint::Literal(Color::argb(0xFFFF_0000)))
    );
    assert_eq!(
        trans.keyframes[1].value,
        KeyframeValue::Paint(Paint::Literal(Color::argb(0xFF00_00FF)))
    );
    assert_eq!(
        last_color_prop(&changed, target.0),
        Some(PropertyValue::Integer(
            Paint::Literal(Color::argb(0xFF00_00FF)).to_bits() as i64
        )),
        "the container holds the target colour: {changed:#?}"
    );

    host.dispatch(HostEvent::new(
        target.0,
        0,
        EventPayload::AnimationEvent {
            animation_id: trans.animation_id,
            kind: AnimationEventKind::End,
            property: AnimatedProperty::Color,
            slot: 0,
            iteration: 0,
            elapsed_ms: 500.0,
            time_nanos: 1,
        },
    ))
    .expect("the event is dispatched");
    SEEN.with(|seen| {
        let seen = seen.borrow();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert!(seen[0].contains("property_name: \"color\""), "{seen:?}");
        assert!(seen[0].contains("elapsed_time: 0.5"), "{seen:?}");
    });
}

const RUNTIME_REDUCED_CSS: &str = r#"
html, body { margin: 0; padding: 0; }
#reduced_fade { width: 100px; height: 50px; opacity: 1; transition: opacity 0.5s linear; }
@media (prefers-reduced-motion: reduce) { #reduced_fade { transition: none; } }
@keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
#reduced_spin { width: 40px; height: 40px; }
@media (prefers-reduced-motion: no-preference) { #reduced_spin { animation: spin 1s linear infinite; } }
"#;

fn runtime_reduced_config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![RUNTIME_REDUCED_CSS.to_string()],
        measurer: Some(Box::new(FixedAdvance)),
        ..HtmlConfig::default()
    }
}

fn runtime_reduced_app() -> Element {
    let mut presses = use_signal(|| 0u32);
    let opacity = if presses() % 2 == 1 { "0.25" } else { "1" };
    rsx! {
        div {
            id: "reduced_fade",
            style: "opacity: {opacity}",
            onclick: move |_| presses += 1,
        }
        div { id: "reduced_spin" }
    }
}

/// `EventPayload::ReducedMotionChanged` updates `@media (prefers-reduced-motion: ...)` on a
/// live `HtmlRuntime`: turning it on cancels active `no-preference` animations and disables
/// `reduce`-guarded transitions, and turning it off restores them.
#[test]
fn fr34_2_runtime_reduced_motion_event_updates_prefers_reduced_motion_rules() {
    let mut host = Host::html(runtime_reduced_app, runtime_reduced_config);
    let first = owned(host.rebuild().unwrap());
    let target = clickable(&first);
    let initial = started(&first);
    assert_eq!(
        initial.len(),
        1,
        "spin starts when reduced motion is off: {first:#?}"
    );
    let spin = &initial[0];

    let (reduced_batch, _) = host
        .dispatch(HostEvent::new(
            0,
            0,
            EventPayload::ReducedMotionChanged(ReducedMotion::On),
        ))
        .expect("reduced motion event is dispatched");
    let reduced = owned(reduced_batch);
    let cancelled: Vec<_> = reduced
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::ControlAnimation {
                node_id,
                animation_id,
                op,
                ..
            } => Some((*node_id, *animation_id, *op)),
            _ => None,
        })
        .collect();
    assert_eq!(
        cancelled,
        vec![(spin.node_id, spin.animation_id, AnimationControl::Cancel)],
        "turning reduced motion on cancels the no-preference loop: {reduced:#?}"
    );

    let toggled = click(&mut host, target);
    assert!(
        started(&toggled).is_empty(),
        "transition: none under prefers-reduced-motion: reduce starts no transition: {toggled:#?}"
    );

    let (restored_batch, _) = host
        .dispatch(HostEvent::new(
            0,
            0,
            EventPayload::ReducedMotionChanged(ReducedMotion::Off),
        ))
        .expect("reduced motion off event is dispatched");
    let restored = owned(restored_batch);
    let restarted = started(&restored);
    assert_eq!(
        restarted.len(),
        1,
        "turning reduced motion off restores the no-preference loop: {restored:#?}"
    );
    assert_eq!(restarted[0].node_id, spin.node_id);
    assert_eq!(restarted[0].property, AnimatedProperty::Transform);
}

// Motion through a 3D transform: 4x4 matrices, column-major (`m[4 * column + row]`), each
// keyframe's transform without its origin, which the node's `Transform3D` carries.

const THROUGH_3D_CSS: &str = r#"
html, body { margin: 0; padding: 0; }
#flip { width: 100px; height: 50px; transition: transform 1s linear; }
#flip.on { transform: rotateY(90deg); }
@keyframes tilt { from { transform: rotateX(0deg); } to { transform: rotateX(90deg); } }
#tilt { width: 40px; height: 40px; animation: tilt 1s linear infinite; }
@keyframes tumble { from { transform: rotateX(0deg); } to { transform: rotateX(360deg); } }
#tumble { width: 40px; height: 40px; animation: tumble 3s linear infinite; }
@keyframes swing { from { transform: rotateX(0deg); } to { transform: rotateX(270deg); } }
#swing { width: 40px; height: 40px; animation: swing 2s ease-in-out infinite; }
#stair { width: 40px; height: 40px; animation: tumble 4s steps(4) infinite; }
"#;

fn through_3d_config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![THROUGH_3D_CSS.to_string()],
        measurer: Some(Box::new(FixedAdvance)),
        ..HtmlConfig::default()
    }
}

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// `rotateX(degrees)`: y goes to `(0, cos, sin)` and z to `(0, -sin, cos)`.
fn rotate_x(degrees: f32) -> [f32; 16] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [
        1.0, 0.0, 0.0, 0.0, 0.0, cos, sin, 0.0, 0.0, -sin, cos, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

/// `rotateY(degrees)`: x goes to `(cos, 0, -sin)` and z to `(sin, 0, cos)`.
fn rotate_y(degrees: f32) -> [f32; 16] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [
        cos, 0.0, -sin, 0.0, 0.0, 1.0, 0.0, 0.0, sin, 0.0, cos, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

#[track_caller]
fn matrix_of(value: &KeyframeValue) -> [f32; 16] {
    match value {
        KeyframeValue::Matrix3D(m) => *m,
        other => panic!("a matrix keyframe, not {other:?}"),
    }
}

#[track_caller]
fn assert_matrix(actual: [f32; 16], expected: [f32; 16], tolerance: f32, what: &str) {
    for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (a - e).abs() < tolerance,
            "{what}: element {index} is {a}, not {e}\nactual:   {actual:?}\nexpected: {expected:?}"
        );
    }
}

/// The angle in degrees, from 0 up to 360, of a turn about x.
fn angle_about_x(m: [f32; 16]) -> f32 {
    let degrees = m[6].atan2(m[5]).to_degrees();
    if degrees < -1e-3 {
        degrees + 360.0
    } else {
        degrees
    }
}

/// The progress CSS `cubic-bezier(x1, y1, x2, y2)` gives at time `x`.
fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let at = |c1: f32, c2: f32, t: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * c1 + 3.0 * u * t * t * c2 + t * t * t
    };
    let (mut low, mut high) = (0.0f32, 1.0f32);
    for _ in 0..64 {
        let middle = (low + high) / 2.0;
        if at(x1, x2, middle) < x {
            low = middle;
        } else {
            high = middle;
        }
    }
    at(y1, y2, (low + high) / 2.0)
}

fn flip() -> Element {
    let mut presses = use_signal(|| 0u32);
    let class = if presses() % 2 == 1 { "on" } else { "" };
    rsx! { div { id: "flip", class: "{class}", onclick: move |_| presses += 1 } }
}

/// `#flip` declares `transition: transform 1s linear` and a click turns it to
/// `rotateY(90deg)`. That is one `StartAnimation` of `Transform3D` in slot 0 with two
/// matrix keyframes, the identity (no transform, carried on from what the box shows) and
/// the matrix of `rotateY(90deg)`, with no origin of its own: the box is 100 by 50, and its
/// `Transform3D` modifier turns about the centre, (0.5, 0.5) of it. A quarter turn is less
/// than half a turn, so nothing is cut.
#[test]
fn fr34_2_a_transition_through_a_3d_transform_plays_as_matrix_keyframes() {
    let mut host = Host::html(flip, through_3d_config);
    let first = owned(host.rebuild().unwrap());
    assert!(started(&first).is_empty(), "{first:#?}");
    let target = clickable(&first);

    let changed = click(&mut host, target);
    let sent = started(&changed);
    assert_eq!(sent.len(), 1, "one transition: {changed:#?}");
    let animation = &sent[0];
    assert_eq!(animation.node_id, target.0);
    assert_eq!(animation.property, AnimatedProperty::Transform3D);
    assert_eq!(animation.slot, 0);
    assert_eq!(animation.duration_ms, 1000.0);
    assert_eq!(animation.iterations, 1.0);
    assert_eq!(animation.fill, FillMode::Backwards);
    assert_eq!((animation.origin_x, animation.origin_y), (0.0, 0.0));
    let keyframes = &animation.keyframes;
    assert_eq!(keyframes.len(), 2, "{keyframes:#?}");
    assert_eq!(keyframes[0].offset, 0.0);
    assert!(keyframes[0].from_presented);
    assert_eq!(keyframes[0].timing, Timing::Linear);
    assert_matrix(matrix_of(&keyframes[0].value), IDENTITY, 1e-4, "the start");
    assert_eq!(keyframes[1].offset, 1.0);
    assert!(!keyframes[1].from_presented);
    assert_matrix(
        matrix_of(&keyframes[1].value),
        rotate_y(90.0),
        1e-4,
        "the end",
    );
    assert!(
        has_modifier(&changed, target.0, |modifier| matches!(
            modifier,
            Modifier::Transform3D { m, origin_x, origin_y, .. }
                if (*origin_x - 0.5).abs() < 1e-6
                    && (*origin_y - 0.5).abs() < 1e-6
                    && m.iter().zip(rotate_y(90.0)).all(|(a, e)| (a - e).abs() < 1e-4)
        )),
        "the box rests on its 3D transform: {changed:#?}"
    );
    assert!(
        !has_modifier(&changed, target.0, |modifier| matches!(
            modifier,
            Modifier::Transform { .. }
        )),
        "and has no 2D transform beside it: {changed:#?}"
    );
}

/// The one animated box of an app, and the animations its first frame starts.
fn first_frame(
    app: fn() -> Element,
) -> (
    Vec<Mutation<'static>>,
    Vec<dioxus_compose::schema::Animation<'static>>,
) {
    let mut host = Host::html(app, through_3d_config);
    let first = owned(host.rebuild().unwrap());
    let sent = started(&first);
    (first, sent)
}

fn tilt() -> Element {
    rsx! { div { id: "tilt" } }
}

/// `tilt` goes from `rotateX(0deg)` to `rotateX(90deg)`. It is sent as `Transform3D` in the
/// slot after the transition's, with the identity and the matrix of a quarter turn about x
/// as its keyframes, and the box carries a `Transform3D` modifier, the identity it rests
/// at, about its centre.
#[test]
fn fr34_2_a_3d_keyframes_animation_plays_as_matrix_keyframes() {
    let (first, sent) = first_frame(tilt);
    assert_eq!(sent.len(), 1, "{first:#?}");
    let animation = &sent[0];
    assert_eq!(animation.property, AnimatedProperty::Transform3D);
    assert_eq!(animation.slot, 1);
    assert_eq!(animation.duration_ms, 1000.0);
    assert!(animation.iterations.is_infinite());
    assert_eq!((animation.origin_x, animation.origin_y), (0.0, 0.0));
    let keyframes = &animation.keyframes;
    assert_eq!(keyframes.len(), 2, "{keyframes:#?}");
    assert_eq!((keyframes[0].offset, keyframes[1].offset), (0.0, 1.0));
    assert!(!keyframes[0].from_presented);
    assert_eq!(keyframes[0].timing, Timing::Linear);
    assert_eq!(keyframes[1].timing, Timing::Linear);
    assert_matrix(matrix_of(&keyframes[0].value), IDENTITY, 1e-4, "from");
    assert_matrix(matrix_of(&keyframes[1].value), rotate_x(90.0), 1e-4, "to");
    assert!(
        has_modifier(&first, animation.node_id, |modifier| matches!(
            modifier,
            Modifier::Transform3D { m, origin_x, origin_y, .. }
                if (*origin_x - 0.5).abs() < 1e-6
                    && (*origin_y - 0.5).abs() < 1e-6
                    && m.iter().zip(IDENTITY).all(|(a, e)| (a - e).abs() < 1e-4)
        )),
        "the box has a 3D transform to play on: {first:#?}"
    );
}

fn tumble() -> Element {
    rsx! { div { id: "tumble" } }
}

/// `tumble` turns from `rotateX(0deg)` to `rotateX(360deg)`, linearly. Interpolated as
/// matrices that is no turn at all, so the segment is cut into three of 120 degrees each,
/// less than half a turn: keyframes at 0, 1/3, 2/3 and 1, the ones between being
/// `rotateX(120deg)` and `rotateX(240deg)`, every piece still linear.
#[test]
fn fr34_2_a_rotation_of_180_degrees_or_more_is_split() {
    let (first, sent) = first_frame(tumble);
    assert_eq!(sent.len(), 1, "{first:#?}");
    let animation = &sent[0];
    assert_eq!(animation.property, AnimatedProperty::Transform3D);
    let keyframes = &animation.keyframes;
    assert_eq!(keyframes.len(), 4, "{keyframes:#?}");
    for (index, keyframe) in keyframes.iter().enumerate() {
        let share = index as f32 / 3.0;
        assert!(
            (keyframe.offset - share).abs() < 1e-6,
            "keyframe {index} is at {}, not {share}",
            keyframe.offset
        );
        assert_eq!(keyframe.timing, Timing::Linear, "keyframe {index}");
        assert_matrix(
            matrix_of(&keyframe.value),
            rotate_x(120.0 * index as f32),
            1e-4,
            &format!("keyframe {index}"),
        );
    }
    // Each piece turns 120 degrees, which the renderer's matrix interpolation turns the
    // same way, because it is less than half a turn.
    let angles: Vec<f32> = keyframes
        .iter()
        .map(|keyframe| angle_about_x(matrix_of(&keyframe.value)))
        .collect();
    for pair in angles[..3].windows(2) {
        assert!((pair[1] - pair[0]).abs() < 180.0, "{angles:?}");
    }
}

fn swing() -> Element {
    rsx! { div { id: "swing" } }
}

fn stair() -> Element {
    rsx! { div { id: "stair" } }
}

/// A cut segment keeps its timing.
///
/// - `swing` turns 270 degrees with `ease-in-out`, `cubic-bezier(0.42, 0, 0.58, 1)`. It is
///   cut in two where the progress is one half, which by the curve's symmetry is at half
///   the time: keyframes at 0, 0.5 and 1, the middle one `rotateX(135deg)`, and each piece
///   timed by its half of the curve stretched to the unit square. Sampled at every
///   twentieth of the time, the progress the two pieces give (the angle of the keyframe a
///   piece starts at, plus its curve's share of the angle to the next) is the progress of
///   the whole curve within 1e-3.
/// - `stair` turns 360 degrees in `steps(4)`. Each step is a keyframe of its own, a quarter
///   of the time apart and a quarter turn further, held with `steps(1)` (jump-end, so a
///   step shows its own keyframe until the next one).
#[test]
fn fr34_2_a_split_segment_keeps_its_curve() {
    let (first, sent) = first_frame(swing);
    assert_eq!(sent.len(), 1, "{first:#?}");
    let keyframes = &sent[0].keyframes;
    assert_eq!(keyframes.len(), 3, "{keyframes:#?}");
    assert_eq!(keyframes[0].offset, 0.0);
    assert!((keyframes[1].offset - 0.5).abs() < 1e-3, "{keyframes:#?}");
    assert_eq!(keyframes[2].offset, 1.0);
    assert_matrix(
        matrix_of(&keyframes[1].value),
        rotate_x(135.0),
        1e-3,
        "the middle",
    );
    let curves: Vec<[f32; 4]> = keyframes[..2]
        .iter()
        .map(|keyframe| match keyframe.timing {
            Timing::CubicBezier { x1, y1, x2, y2 } => [x1, y1, x2, y2],
            other => panic!("a cubic-bezier piece, not {other:?}"),
        })
        .collect();
    assert_eq!(keyframes[2].timing, Timing::Linear);
    let progress: Vec<f32> = keyframes
        .iter()
        .map(|keyframe| angle_about_x(matrix_of(&keyframe.value)) / 270.0)
        .collect();
    assert!(
        progress[0].abs() < 1e-4 && (progress[2] - 1.0).abs() < 1e-4,
        "{progress:?}"
    );
    for sample in 0..=20 {
        let time = sample as f32 / 20.0;
        let piece = if time < keyframes[1].offset { 0 } else { 1 };
        let (start, end) = (keyframes[piece].offset, keyframes[piece + 1].offset);
        let [x1, y1, x2, y2] = curves[piece];
        let local = bezier(x1, y1, x2, y2, (time - start) / (end - start));
        let pieced = progress[piece] + (progress[piece + 1] - progress[piece]) * local;
        let whole = bezier(0.42, 0.0, 0.58, 1.0, time);
        assert!(
            (pieced - whole).abs() < 1e-3,
            "at {time}: the pieces give {pieced}, the curve {whole}; {curves:?}, {progress:?}"
        );
    }

    let (first, sent) = first_frame(stair);
    assert_eq!(sent.len(), 1, "{first:#?}");
    let keyframes = &sent[0].keyframes;
    assert_eq!(
        keyframes.len(),
        5,
        "one keyframe per step and the end: {keyframes:#?}"
    );
    for (index, keyframe) in keyframes.iter().enumerate() {
        assert_eq!(keyframe.offset, index as f32 / 4.0, "keyframe {index}");
        let timing = if index < 4 {
            Timing::Steps {
                count: 1,
                position: StepPosition::JumpEnd,
            }
        } else {
            Timing::Linear
        };
        assert_eq!(keyframe.timing, timing, "keyframe {index}");
        assert_matrix(
            matrix_of(&keyframe.value),
            rotate_x(90.0 * index as f32),
            1e-4,
            &format!("step {index}"),
        );
    }
}
