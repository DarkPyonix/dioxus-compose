use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{BatchEncoder, HostEvent, Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{EventPayload, PropertyKind};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

const NFR9_HOST_ALLOCATION_CEILING: usize = 200;
const NFR9_REPEATED_INTERACTIONS: usize = 100;

struct CountingAllocator;

// Counting is per thread, not per process: the test harness runs tests concurrently, so a
// global counter would attribute other tests' allocations to whichever measurement is open.
thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn record_allocation() {
    // `try_with` because a thread tearing down its locals must not re-enter them.
    let _ = TRACKING.try_with(|tracking| {
        if tracking.get() {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
    });
}

// SAFETY: Every operation delegates to the process System allocator unchanged.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        // SAFETY: Delegating the caller-provided layout to System.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Delegating the original pointer and layout to System.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        // SAFETY: Delegating the original allocation and requested size to System.
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

struct AllocationMeasurement;

impl AllocationMeasurement {
    fn start() -> Self {
        ALLOCATIONS.with(|count| count.set(0));
        TRACKING.with(|tracking| tracking.set(true));
        Self
    }

    fn finish(self) -> usize {
        TRACKING.with(|tracking| tracking.set(false));
        ALLOCATIONS.with(|count| count.get())
    }
}
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
                value: PropertyValue::String("steady-state"),
            })
            .unwrap();
    }
    std::hint::black_box(encoder.finish().unwrap());
}

fn measure_click(host: &mut Host, click: &HostEvent<'static>) -> usize {
    let measurement = AllocationMeasurement::start();
    let (batch, result) = host.dispatch(click.clone()).unwrap();
    std::hint::black_box((batch.len(), result));
    measurement.finish()
}

#[test]
fn nfr9_boundary_encoding_does_not_allocate() {
    let mut encoder = BatchEncoder::with_capacity(4096, 2048, 128);
    encode_one_hundred(&mut encoder);

    let measurement = AllocationMeasurement::start();
    encode_one_hundred(&mut encoder);
    let allocations = measurement.finish();

    assert_eq!(allocations, 0, "warm arena encoding allocated");
}

#[test]
fn nfr9_host_path_allocation_ceiling() {
    let (mut host, click) = host_and_click();
    for _ in 0..2 {
        let _ = host.dispatch(click.clone()).unwrap();
    }

    let allocations = measure_click(&mut host, &click);

    assert!(
        allocations <= NFR9_HOST_ALLOCATION_CEILING,
        "Host click path allocated {allocations} times; SPEC ceiling is {NFR9_HOST_ALLOCATION_CEILING}"
    );
}

#[test]
fn nfr9_allocations_do_not_grow_across_repeated_interactions() {
    let (mut host, click) = host_and_click();
    for _ in 0..2 {
        let _ = host.dispatch(click.clone()).unwrap();
    }

    let allocations: Vec<_> = (0..NFR9_REPEATED_INTERACTIONS)
        .map(|_| measure_click(&mut host, &click))
        .collect();
    let expected = allocations[0];

    assert!(
        allocations.iter().all(|&count| count == expected),
        "per-interaction allocations grew or varied: {allocations:?}"
    );
}
