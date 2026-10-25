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
use dioxus_compose::protocol::{HostEvent, Mutation, decode_batch};
use dioxus_compose::schema::{
    AnimatedProperty, AnimationControl, AnimationEventKind, AnimationEvents, EventPayload,
    FillMode, KeyframeValue, Modifier, PlayState, PlaybackDirection, Timing, TransformFunctionKind,
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
        .dispatch(HostEvent {
            node_id,
            handler_id,
            payload: EventPayload::Clicked,
        })
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

    host.dispatch(HostEvent {
        node_id: target.0,
        handler_id: 0,
        payload: EventPayload::AnimationEvent {
            animation_id: animation.animation_id,
            kind: AnimationEventKind::End,
            property: AnimatedProperty::Alpha,
            slot: 0,
            iteration: 0,
            elapsed_ms: 500.0,
            time_nanos: 1,
        },
    })
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
        host.dispatch(HostEvent {
            node_id: animation.node_id,
            handler_id: 0,
            payload: EventPayload::AnimationEvent {
                animation_id: animation.animation_id,
                kind,
                property: AnimatedProperty::Transform,
                slot: animation.slot,
                iteration: 0,
                elapsed_ms,
                time_nanos: 1,
            },
        })
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
    host.dispatch(HostEvent {
        node_id,
        handler_id: 0,
        payload: EventPayload::AnimationEvent {
            animation_id,
            kind,
            property: AnimatedProperty::Alpha,
            slot: 0,
            iteration: 0,
            elapsed_ms: 0.0,
            time_nanos: 1,
        },
    })
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
