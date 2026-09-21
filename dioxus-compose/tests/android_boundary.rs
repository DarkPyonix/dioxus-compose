//! The Android boundary: the lifecycle events the Host answers for itself, and the frame
//! request a Renderer that is not yet listening would otherwise swallow.
//!
//! Each `tests/*.rs` file is its own binary, so the process-global frame state these tests
//! read belongs to this file alone. They still take `FRAME_STATE` in turn, because that
//! state is one set of flags and the test harness runs them on several threads.

use dioxus_compose::boundary::STATUS_OK;
use dioxus_compose::codegen::{
    generate_android_bridge_kotlin, generate_fast_native_kotlin, generate_jni_rust,
};
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{HostEvent, Mutation, decode_batch, encode_event};
use dioxus_compose::schema::{BOUNDARY_SCHEMA, EventPayload};
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

/// A runtime that cannot read Host memory directly is given the arena once and reads the
/// batch where it lies, so the batch has to be inside the arena the Host reports.
#[test]
fn pr4_the_batch_lies_inside_the_reported_arena() {
    let _state = frame_state();
    let mut host = Host::new(app);
    let batch = host.rebuild().unwrap();
    let (batch_start, batch_len) = (batch.as_ptr() as usize, batch.len());
    let (base, capacity) = host.arena();

    assert!(!base.is_null());
    let base = base as usize;
    assert!(batch_start >= base, "the batch starts before the arena");
    assert!(
        batch_start + batch_len <= base + capacity,
        "the batch runs past the end of the arena"
    );
}

/// Both halves of the boundary are generated from one table, so they cannot drift, and
/// what is checked in has to be what the generator produces today.
#[test]
fn pr5_generated_shims_match_the_boundary_schema() {
    let rust = generate_jni_rust();
    let kotlin = generate_android_bridge_kotlin();
    for op in BOUNDARY_SCHEMA {
        let symbol = format!(
            "Java_dioxus_compose_ui_platform_HostBridge_native{}",
            op.name
        );
        assert!(rust.contains(&symbol), "missing shim for {}", op.name);
        assert!(rust.contains(op.symbol), "shim does not call {}", op.symbol);
        assert!(
            kotlin.contains(&format!("external fun native{}(", op.name)),
            "missing declaration for {}",
            op.name
        );
        assert_eq!(
            op.fast,
            kotlin.contains(&format!("@FastNative\nexternal fun native{}(", op.name)),
            "the fast annotation on {} does not match the schema",
            op.name
        );
    }
    // The batch is read where it lies, so the shims map the arena and copy nothing.
    assert!(rust.contains("new_direct_byte_buffer"));
    assert!(!rust.contains("copy_from_slice"));
    // A worker thread attaches to the JavaVM once and stays attached.
    assert!(rust.contains("attach_current_thread_permanently"));

    assert_eq!(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/boundary_jni.gen.rs"
        )),
        rust,
        "generated JNI shims are stale; run `cargo run -p dioxus-compose --bin codegen`",
    );
    assert_eq!(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../dioxus-compose-renderer/android/src/bridge/HostBridge.gen.kt"
        )),
        kotlin,
        "generated Kotlin bridge is stale; run `cargo run -p dioxus-compose --bin codegen`",
    );
    assert_eq!(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../dioxus-compose-renderer/android/src/bridge/FastNative.gen.kt"
        )),
        generate_fast_native_kotlin(),
        "the generated annotation is stale; run `cargo run -p dioxus-compose --bin codegen`",
    );
}
