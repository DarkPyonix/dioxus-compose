use criterion::{Criterion, criterion_group, criterion_main};
use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{BatchEncoder, HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{EventPayload, PropertyKind};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

struct CountingAllocator;

static TRACKING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: Every operation delegates to the process System allocator unchanged.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACKING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: Delegating the caller-provided layout to System.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Delegating the original pointer and layout to System.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if TRACKING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: Delegating the original allocation and requested size to System.
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn app() -> Element {
    let mut count = use_signal(|| 0_u64);
    rsx! {
        Column {
            Text { text: count().to_string() }
            TextField { placeholder: "Message" }
            Button {
                text: "Increment",
                on_click: move |_| *count.write() += 1,
            }
        }
    }
}

fn host_and_click() -> (Host, HostEvent<'static>) {
    let mut host = Host::new(app);
    let initial = decode_batch(host.rebuild().unwrap()).unwrap();
    let (node_id, handler_id) = initial
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::SetProp {
                node_id,
                property: PropertyKind::OnClick,
                value: PropertyValue::Integer(handler),
            } => Some((*node_id, *handler as u64)),
            _ => None,
        })
        .unwrap();
    drop(initial);
    (
        host,
        HostEvent {
            node_id,
            handler_id,
            payload: EventPayload::Clicked,
        },
    )
}

fn encode_one_hundred(encoder: &mut BatchEncoder) {
    encoder.clear();
    for node_id in 0..100 {
        encoder
            .encode(&Mutation::SetProp {
                node_id,
                property: PropertyKind::Text,
                value: PropertyValue::String("benchmark"),
            })
            .unwrap();
    }
    std::hint::black_box(encoder.finish().unwrap());
}

fn benchmarks(criterion: &mut Criterion) {
    let (mut host, click) = host_and_click();
    criterion.bench_function("click_dispatch_diff_encode", |bencher| {
        bencher.iter(|| {
            let (batch, result) = host.dispatch(click.clone()).unwrap();
            std::hint::black_box((batch.len(), result));
        });
    });

    let mut encoder = BatchEncoder::with_capacity(4096, 2048, 128);
    encode_one_hundred(&mut encoder);
    criterion.bench_function("encode_100_mutations", |bencher| {
        bencher.iter(|| encode_one_hundred(&mut encoder));
    });

    criterion.bench_function("steady_state_encoder_zero_alloc", |bencher| {
        bencher.iter(|| {
            ALLOCATIONS.store(0, Ordering::SeqCst);
            TRACKING.store(true, Ordering::SeqCst);
            encode_one_hundred(&mut encoder);
            TRACKING.store(false, Ordering::SeqCst);
            assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), 0);
        });
    });

    let (mut allocation_host, allocation_click) = host_and_click();
    let _ = allocation_host.dispatch(allocation_click.clone()).unwrap();
    ALLOCATIONS.store(0, Ordering::SeqCst);
    TRACKING.store(true, Ordering::SeqCst);
    let _ = allocation_host.dispatch(allocation_click).unwrap();
    TRACKING.store(false, Ordering::SeqCst);
    // SPEC-GAP: dioxus-core's event dispatch/diff path allocates internally;
    // this measurement separates that cost from the zero-allocation arena.
    eprintln!(
        "measured_host_steady_state_allocations={}",
        ALLOCATIONS.load(Ordering::SeqCst)
    );

    let mut samples = Vec::with_capacity(10_000);
    let (mut sampled_host, sampled_click) = host_and_click();
    for _ in 0..10_000 {
        let started = Instant::now();
        let _ = sampled_host.dispatch(sampled_click.clone()).unwrap();
        samples.push(started.elapsed().as_nanos());
    }
    samples.sort_unstable();
    eprintln!("measured_click_p99_ns={}", samples[9_899]);

    let mut encode_samples = Vec::with_capacity(10_000);
    for _ in 0..10_000 {
        let started = Instant::now();
        encode_one_hundred(&mut encoder);
        encode_samples.push(started.elapsed().as_nanos());
    }
    encode_samples.sort_unstable();
    eprintln!("measured_encode_100_p99_ns={}", encode_samples[9_899]);
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
