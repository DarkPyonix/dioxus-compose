//! `AnimatedVisibility`: the child is declared when it appears, stays declared through the
//! exit animation, and is removed when the Renderer reports that animation's end.
//! The batches are the ones the Renderer would be sent; the events are the ones it would
//! send back.

use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::PropertyKind;
use dioxus_compose::protocol::encode_event;
use dioxus_compose::{
    Animation, AnimationEventKind, AnimationEvents, EventPayload, Host, ReducedMotion, WidgetKind,
};
use std::cell::Cell;

thread_local! {
    static START_OPEN: Cell<bool> = const { Cell::new(false) };
    static RENDERS: Cell<usize> = const { Cell::new(0) };
}

fn screen() -> Element {
    let mut open = use_signal(|| START_OPEN.with(Cell::get));
    RENDERS.with(|renders| renders.set(renders.get() + 1));
    rsx! {
        Column {
            Button { text: "toggle", on_click: move |()| open.toggle() }
            AnimatedVisibility {
                visible: open(),
                enter: Enter::fade(),
                exit: Exit::fade(),
                Text { text: "body" }
            }
        }
    }
}

fn owned(batch: &[u8]) -> Vec<Mutation<'static>> {
    let bytes: &'static [u8] = Box::leak(batch.to_vec().into_boxed_slice());
    decode_batch(bytes).unwrap()
}

fn start(open: bool) -> (Host, Vec<Mutation<'static>>, u32, u64) {
    dioxus_compose::animation::reset_animations();
    dioxus_compose::window::reset_window_size();
    START_OPEN.with(|flag| flag.set(open));
    let mut host = Host::new(screen);
    let first = owned(host.rebuild().unwrap());
    let button = created(&first, WidgetKind::Button);
    let handler = first
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::SetProp {
                node_id,
                property: PropertyKind::OnClick,
                value: PropertyValue::Integer(handler),
            } if *node_id == button => Some(*handler as u64),
            _ => None,
        })
        .expect("the button has a click handler");
    (host, first, button, handler)
}

fn created(mutations: &[Mutation<'_>], widget: WidgetKind) -> u32 {
    mutations
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::Create { node_id, widget: kind } if *kind == widget => Some(*node_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {widget:?} was created"))
}

fn event(host: &mut Host, node_id: u32, handler_id: u64, payload: EventPayload) -> Vec<Mutation<'static>> {
    let mut wire = Vec::new();
    encode_event(
        &HostEvent {
            node_id,
            handler_id,
            payload,
        },
        &mut wire,
    )
    .unwrap();
    let (batch, _) = host.dispatch_event(&wire).unwrap();
    owned(batch)
}

fn click(host: &mut Host, button: u32, handler: u64) -> Vec<Mutation<'static>> {
    event(host, button, handler, EventPayload::Clicked)
}

fn ended(host: &mut Host, animation: &Animation<'_>) -> Vec<Mutation<'static>> {
    event(
        host,
        animation.node_id,
        0,
        EventPayload::AnimationEvent {
            animation_id: animation.animation_id,
            kind: AnimationEventKind::End,
            property: animation.property,
            slot: animation.slot,
            iteration: 0,
            elapsed_ms: animation.duration_ms,
            time_nanos: 0,
        },
    )
}

fn animations(mutations: &[Mutation<'static>]) -> Vec<Animation<'static>> {
    mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::StartAnimation(animation) => Some(animation.clone()),
            _ => None,
        })
        .collect()
}

fn removes(mutations: &[Mutation<'_>]) -> usize {
    mutations
        .iter()
        .filter(|mutation| matches!(mutation, Mutation::Remove { .. }))
        .count()
}

fn creates_text(mutations: &[Mutation<'_>]) -> bool {
    mutations.iter().any(|mutation| {
        matches!(mutation, Mutation::Create { widget: WidgetKind::Text, .. })
    })
}

#[test]
fn fr24_1_enter_starts_one_animation() {
    let (mut host, first, button, handler) = start(false);
    assert!(!creates_text(&first), "a hidden child is not declared");
    let shown = click(&mut host, button, handler);
    assert!(creates_text(&shown), "the child is declared: {shown:?}");
    let started = animations(&shown);
    assert_eq!(started.len(), 1, "one animation: {shown:?}");
    assert_eq!(started[0].keyframes[0].value, dioxus_compose::KeyframeValue::Alpha(0.0));
    assert!(!started[0].keyframes[0].from_presented, "it starts from the hidden value");
    let renders = RENDERS.with(Cell::get);
    for frame in 1..=60_u64 {
        let batch = owned(host.render_frame(frame * 16_666_667).unwrap());
        assert!(batch.is_empty(), "frame {frame} carried {batch:?}");
    }
    assert_eq!(renders, RENDERS.with(Cell::get), "the screen was rendered again");
}

#[test]
fn fr24_1_exit_removes_after_end_event() {
    let (mut host, _, button, handler) = start(true);
    let hidden = click(&mut host, button, handler);
    let started = animations(&hidden);
    assert_eq!(started.len(), 1, "{hidden:?}");
    assert_eq!(removes(&hidden), 0, "not removed at once: {hidden:?}");
    assert_ne!(started[0].events, AnimationEvents::NONE, "the end is asked for");
    let after = ended(&mut host, &started[0]);
    assert!(removes(&after) >= 1, "removed after the end: {after:?}");
}

#[test]
fn fr24_1_interrupted_exit_resumes() {
    let (mut host, _, button, handler) = start(true);
    let hidden = click(&mut host, button, handler);
    let leaving = animations(&hidden);
    assert_eq!(leaving.len(), 1);
    let back = click(&mut host, button, handler);
    assert_eq!(removes(&back), 0, "{back:?}");
    let resumed = animations(&back);
    assert_eq!(resumed.len(), 1, "{back:?}");
    assert_eq!(resumed[0].node_id, leaving[0].node_id, "on the same node");
    assert_eq!(resumed[0].keyframes[0].value, dioxus_compose::KeyframeValue::Alpha(1.0));
    assert!(resumed[0].keyframes[0].from_presented, "from the value on screen");
    let late = ended(&mut host, &leaving[0]);
    assert_eq!(removes(&late), 0, "an exit that was cancelled removes nothing: {late:?}");
}

#[test]
fn fr24_1_reduced_motion_is_immediate() {
    let (mut host, _, button, handler) = start(false);
    event(
        &mut host,
        0,
        0,
        EventPayload::ReducedMotionChanged(ReducedMotion::On),
    );
    let shown = click(&mut host, button, handler);
    assert!(creates_text(&shown), "{shown:?}");
    assert!(animations(&shown).is_empty(), "{shown:?}");
    let hidden = click(&mut host, button, handler);
    assert!(animations(&hidden).is_empty(), "{hidden:?}");
    assert!(removes(&hidden) >= 1, "removed in the same batch: {hidden:?}");
}

#[test]
fn fr24_1_initially_visible_does_not_animate() {
    let (_, first, _, _) = start(true);
    assert!(creates_text(&first));
    assert!(animations(&first).is_empty(), "{first:?}");
}

#[test]
fn fr24_1_exiting_child_stays_composed_until_end() {
    let (mut host, _, button, handler) = start(true);
    let hidden = click(&mut host, button, handler);
    let started = animations(&hidden);
    assert_eq!(started.len(), 1, "{hidden:?}");
    assert_eq!(removes(&hidden), 0, "{hidden:?}");
    for frame in 1..=30_u64 {
        let batch = owned(host.render_frame(frame * 16_666_667).unwrap());
        assert_eq!(removes(&batch), 0, "frame {frame} removed the child: {batch:?}");
    }
    let after = ended(&mut host, &started[0]);
    assert!(removes(&after) >= 1, "{after:?}");
}
