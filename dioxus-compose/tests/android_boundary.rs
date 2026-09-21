//! The Android boundary: the lifecycle events the Host answers for itself, and the frame
//! request a Renderer that is not yet listening would otherwise swallow.
//!
//! Each `tests/*.rs` file is its own binary, so the process-global frame state these tests
//! read belongs to this file alone. They still take `FRAME_STATE` in turn, because that
//! state is one set of flags and the test harness runs them on several threads.

use dioxus_compose::boundary::STATUS_OK;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, decode_batch, encode_event};
use dioxus_compose::schema::EventPayload;
use dioxus_compose::{Host, RendererApi, install_renderer_api, request_frame_from_worker};
use std::ffi::c_int;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

static FRAME_REQUESTS: AtomicUsize = AtomicUsize::new(0);
static FRAME_STATE: Mutex<()> = Mutex::new(());

extern "C" fn count_request() {
    FRAME_REQUESTS.fetch_add(1, Ordering::AcqRel);
}

extern "C" fn never_run() -> c_int {
    STATUS_OK as c_int
}

/// Installs the counting renderer and takes the frame state, counting from zero.
fn frame_state() -> MutexGuard<'static, ()> {
    let guard = FRAME_STATE
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let _ = install_renderer_api(RendererApi {
        run: never_run,
        request_frame: count_request,
    });
    FRAME_REQUESTS.store(0, Ordering::Release);
    guard
}

fn requests() -> usize {
    FRAME_REQUESTS.load(Ordering::Acquire)
}

fn app() -> Element {
    rsx! {
        Column {
            Text { text: "android" }
            Button { text: "send", on_click: move |_| {} }
        }
    }
}

fn event(payload: EventPayload<'static>) -> Vec<u8> {
    let mut bytes = Vec::new();
    encode_event(
        &HostEvent {
            node_id: 0,
            handler_id: 0,
            payload,
        },
        &mut bytes,
    )
    .unwrap();
    bytes
}

/// The Renderer folds requests into its frame clock, so the Host has to keep asking.
/// Holding the flag until a frame came back lost every request that followed one the
/// Renderer was not yet listening for, and on a cold start that is the first one.
#[test]
fn pr3_a_missed_frame_request_does_not_silence_the_next() {
    let _state = frame_state();
    request_frame_from_worker();
    request_frame_from_worker();
    request_frame_from_worker();
    assert_eq!(requests(), 3);
}

/// Stopping suppresses timers and animations, starting resumes them, and the request that
/// arrived while the UI was off screen is delivered once rather than lost.
#[test]
fn pr5_lifecycle_stop_suppresses_frame_requests() {
    let _state = frame_state();
    let mut host = Host::new(app);
    host.rebuild().unwrap();

    host.dispatch_event(&event(EventPayload::LifecycleStop))
        .unwrap();
    FRAME_REQUESTS.store(0, Ordering::Release);
    request_frame_from_worker();
    request_frame_from_worker();
    assert_eq!(requests(), 0, "a stopped UI must not be asked to draw");

    host.dispatch_event(&event(EventPayload::LifecycleStart))
        .unwrap();
    assert_eq!(
        requests(),
        1,
        "the suppressed request is delivered once, on start"
    );
}

/// A Renderer that lost its node table asks for the whole tree, and gets a batch that
/// creates it rather than a diff against something it no longer has.
#[test]
fn pr5_resync_answers_with_the_whole_tree() {
    let _state = frame_state();
    let mut host = Host::new(app);
    let initial = decode_batch(host.rebuild().unwrap()).unwrap().len();

    let (batch, result) = host.dispatch_event(&event(EventPayload::Resync)).unwrap();
    let mutations = decode_batch(batch).unwrap();
    assert_eq!(result, 0);
    assert_eq!(mutations.len(), initial);
    assert!(
        mutations
            .iter()
            .any(|mutation| matches!(mutation, Mutation::Create { .. })),
        "a resync batch has to create the tree, not diff it"
    );
}
