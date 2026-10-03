//! A panic in user code must come back across the C ABI as `STATUS_PANIC`.
//!
//! Unwinding out of an `extern "C"` function aborts the process, so the exports catch
//! it. These cases put the panic where an application would: in a component's render
//! and in an event handler.
//!
//! They live in their own test binary because each one launches a different root
//! component, and the launched component is process-global. The lock below keeps the
//! two from launching over each other when the harness runs them in parallel.

use dioxus_compose::boundary::{
    MutationBatch, STATUS_OK, STATUS_PANIC, dioxus_compose_host_dispatch_event,
    dioxus_compose_host_init, dioxus_compose_host_shutdown,
};
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, PropertyValue, decode_batch, encode_event};
use dioxus_compose::schema::{EventPayload, PROTOCOL_VERSION, PropertyKind, SCHEMA_HASH};
use std::sync::{Mutex, MutexGuard};

static LAUNCH: Mutex<()> = Mutex::new(());

fn exclusive_launch() -> MutexGuard<'static, ()> {
    // A test that failed while holding the lock poisons it; the next one can still run.
    LAUNCH.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn panicking_app() -> Element {
    panic!("component panic");
}

fn panicking_handler_app() -> Element {
    rsx! {
        Button {
            text: "go",
            on_click: move |_| {
                if std::hint::black_box(true) {
                    panic!("handler panic");
                }
            }
        }
    }
}

fn handshake() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(12);
    bytes.extend_from_slice(&SCHEMA_HASH.to_le_bytes());
    bytes.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
    bytes.extend_from_slice(&[LoopMode::Platform as u8, 0]);
    bytes
}

fn launch_and_init(app: fn() -> Element, out: &mut MutationBatch) -> i32 {
    LaunchBuilder::new()
        .with_mode(LoopMode::Platform)
        .launch(app);
    let bytes = handshake();
    // SAFETY: `bytes` and `out` are live test-owned storage for the call.
    unsafe { dioxus_compose_host_init(bytes.as_ptr(), bytes.len() as u32, out) }
}

fn click_handler(out: &MutationBatch) -> (u32, u64) {
    // SAFETY: A successful init left a readable batch owned by the Host.
    let initial = unsafe { std::slice::from_raw_parts(out.ptr, out.len as usize) };
    decode_batch(initial)
        .unwrap()
        .into_iter()
        .find_map(|mutation| match mutation {
            Mutation::SetProp {
                node_id,
                property: PropertyKind::OnClick,
                value: PropertyValue::Integer(handler),
            } => Some((node_id, handler as u64)),
            _ => None,
        })
        .expect("the app registers a click handler")
}

/// Runs `body` with the default panic hook silenced, so an expected panic does not print
/// a backtrace into the test output.
fn quietly<T>(body: impl FnOnce() -> T) -> T {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = body();
    std::panic::set_hook(previous);
    result
}

#[test]
fn nfr7_a_panicking_component_is_caught_by_init() {
    let _launch = exclusive_launch();
    let mut out = MutationBatch::default();
    let status = quietly(|| launch_and_init(panicking_app, &mut out));
    assert_eq!(status, STATUS_PANIC, "init must not unwind across the C ABI");
    dioxus_compose_host_shutdown();
}

#[test]
fn nfr7_a_panicking_handler_is_caught_by_dispatch_event() {
    let _launch = exclusive_launch();
    let mut out = MutationBatch::default();
    assert_eq!(launch_and_init(panicking_handler_app, &mut out), STATUS_OK);
    let (node_id, handler_id) = click_handler(&out);
    let mut bytes = Vec::new();
    encode_event(
        &HostEvent {
            node_id,
            handler_id,
            payload: EventPayload::Clicked,
        },
        &mut bytes,
    )
    .unwrap();
    // SAFETY: `bytes` and `out` are live test-owned storage.
    let status = quietly(|| unsafe {
        dioxus_compose_host_dispatch_event(bytes.as_ptr(), bytes.len() as u32, &mut out)
    });
    assert_eq!(
        status, STATUS_PANIC,
        "dispatch_event must not unwind across the C ABI"
    );
    dioxus_compose_host_shutdown();
}
