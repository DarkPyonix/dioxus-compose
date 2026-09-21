//! The Android boundary: the shims that carry it, the lifecycle events the Host answers
//! for itself, and the frame request a Renderer that is not yet listening would otherwise
//! swallow.
//!
//! Each `tests/*.rs` file is its own binary, so the process-global frame state these tests
//! read belongs to this file alone.

use dioxus_compose::boundary::STATUS_OK;
use dioxus_compose::{RendererApi, install_renderer_api, request_frame_from_worker};
use std::ffi::c_int;
use std::sync::atomic::{AtomicUsize, Ordering};

static FRAME_REQUESTS: AtomicUsize = AtomicUsize::new(0);

extern "C" fn count_request() {
    FRAME_REQUESTS.fetch_add(1, Ordering::AcqRel);
}

extern "C" fn never_run() -> c_int {
    STATUS_OK as c_int
}

fn install() {
    let _ = install_renderer_api(RendererApi {
        run: never_run,
        request_frame: count_request,
    });
}

/// The Renderer folds requests into its frame clock, so the Host has to keep asking.
/// Holding the flag until a frame came back lost every request that followed one the
/// Renderer was not yet listening for, and on a cold start that is the first one.
#[test]
fn pr3_a_missed_frame_request_does_not_silence_the_next() {
    install();
    FRAME_REQUESTS.store(0, Ordering::Release);
    request_frame_from_worker();
    request_frame_from_worker();
    request_frame_from_worker();
    assert_eq!(FRAME_REQUESTS.load(Ordering::Acquire), 3);
}
