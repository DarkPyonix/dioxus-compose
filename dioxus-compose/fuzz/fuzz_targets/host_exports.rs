#![no_main]

use arbitrary::Arbitrary;
use dioxus_compose::boundary::{
    MutationBatch, dioxus_compose_host_dispatch_event, dioxus_compose_host_init,
    dioxus_compose_host_release_batch, dioxus_compose_host_render_frame,
    dioxus_compose_host_shutdown,
};
use dioxus_compose::prelude::*;
use libfuzzer_sys::fuzz_target;

fn app() -> Element {
    let mut count = use_signal(|| 0_i64);
    rsx! {
        Column {
            Text { text: count().to_string() }
            TextField { placeholder: "Message" }
            Button { text: "Increment", on_click: move |_| *count.write() += 1 }
        }
    }
}

#[derive(Arbitrary, Debug)]
enum Call {
    Init { bytes: Vec<u8>, null_input: bool, null_out: bool },
    DispatchEvent { bytes: Vec<u8>, null_input: bool, null_out: bool },
    RenderFrame { frame_time_nanos: u64, null_out: bool },
    ReleaseBatch { null: bool },
    Shutdown,
}

// Drives the C exports in any order with any bytes and null pointers. Each call has to
// return a status: a panic, an unwind across the ABI or an abort is a finding.
fuzz_target!(|calls: Vec<Call>| {
    LaunchBuilder::new()
        .with_mode(LoopMode::Platform)
        .launch(app);
    let mut out = MutationBatch::default();
    for call in &calls {
        // SAFETY: pointers are either null (deliberately) or owned by this frame.
        unsafe {
            match call {
                Call::Init {
                    bytes,
                    null_input,
                    null_out,
                } => {
                    let ptr = if *null_input {
                        std::ptr::null()
                    } else {
                        bytes.as_ptr()
                    };
                    let out_ptr = if *null_out {
                        std::ptr::null_mut()
                    } else {
                        &raw mut out
                    };
                    dioxus_compose_host_init(ptr, bytes.len() as u32, out_ptr);
                }
                Call::DispatchEvent {
                    bytes,
                    null_input,
                    null_out,
                } => {
                    let ptr = if *null_input {
                        std::ptr::null()
                    } else {
                        bytes.as_ptr()
                    };
                    let out_ptr = if *null_out {
                        std::ptr::null_mut()
                    } else {
                        &raw mut out
                    };
                    dioxus_compose_host_dispatch_event(ptr, bytes.len() as u32, out_ptr);
                }
                Call::RenderFrame {
                    frame_time_nanos,
                    null_out,
                } => {
                    let out_ptr = if *null_out {
                        std::ptr::null_mut()
                    } else {
                        &raw mut out
                    };
                    dioxus_compose_host_render_frame(*frame_time_nanos, out_ptr);
                }
                Call::ReleaseBatch { null } => {
                    let ptr = if *null {
                        std::ptr::null_mut()
                    } else {
                        &raw mut out
                    };
                    dioxus_compose_host_release_batch(ptr);
                }
                Call::Shutdown => dioxus_compose_host_shutdown(),
            }
        }
    }
    dioxus_compose_host_shutdown();
});
