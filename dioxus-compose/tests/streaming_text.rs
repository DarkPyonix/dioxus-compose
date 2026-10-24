//! Streaming text: the Host sends only the appended tail, coalesced per frame.

use dioxus_compose::Host;
use dioxus_compose::prelude::*;
use dioxus_compose::protocol::{Mutation, decode_batch};
use dioxus_compose::schema::WidgetKind;

const TOKENS_PER_SECOND: usize = 100;

fn app() -> Element {
    rsx! {
        Column {
            Text { text: "" }
        }
    }
}

fn host_and_text_node() -> (Host, u32) {
    let mut host = Host::new(app);
    let initial = decode_batch(host.rebuild().unwrap()).unwrap();
    let node_id = initial
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::Create {
                node_id,
                widget: WidgetKind::Text,
            } => Some(*node_id),
            _ => None,
        })
        .unwrap();
    drop(initial);
    (host, node_id)
}

#[test]
fn fr9_streaming_sends_only_the_appended_tail() {
    let (mut host, node_id) = host_and_text_node();
    let answer = "a long already streamed answer line\n".repeat(1_000);
    let baseline = host.set_text(node_id, &answer, None).unwrap().len();
    assert!(baseline > answer.len());

    host.append_text(node_id, " token");
    let batch = host.render_frame(0).unwrap();
    let batch_len = batch.len();
    let decoded = decode_batch(batch).unwrap();

    assert_eq!(
        decoded,
        vec![Mutation::AppendText {
            node_id,
            text: " token",
        }]
    );
    assert!(
        batch_len < 64,
        "append batch was {batch_len} bytes; the whole answer must not be re-sent"
    );
}

#[test]
fn fr9_tokens_coalesce_into_one_batch_per_frame() {
    let (mut host, node_id) = host_and_text_node();
    for _ in 0..TOKENS_PER_SECOND {
        host.append_text(node_id, "tok ");
    }

    let expected = "tok ".repeat(TOKENS_PER_SECOND);
    let decoded = decode_batch(host.render_frame(0).unwrap()).unwrap();
    assert_eq!(
        decoded,
        vec![Mutation::AppendText {
            node_id,
            text: &expected,
        }]
    );

    // A frame with no new tokens carries no append record.
    let decoded = decode_batch(host.render_frame(0).unwrap()).unwrap();
    assert!(
        decoded.is_empty(),
        "flushed tokens were resent: {decoded:?}"
    );
}

fn first_text_node(mutations: &[Mutation<'_>]) -> u32 {
    mutations
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::Create {
                node_id,
                widget: WidgetKind::Text,
            } => Some(*node_id),
            _ => None,
        })
        .expect("no Text was created")
}

/// A `Text` marked as streaming that only grew sends the tail, and nothing else.
///
/// This is what lets a paragraph of a streamed document grow without the whole paragraph
/// crossing the boundary again on every token.
#[test]
fn fr37_a_streaming_text_that_grows_sends_only_the_tail() {
    use dioxus_compose::PropertyKind;
    use dioxus_compose::protocol::PropertyValue;
    use std::sync::Mutex;

    static SAID: Mutex<Option<SyncSignal<String>>> = Mutex::new(None);

    fn growing() -> Element {
        let said = use_signal_sync(|| String::from("Hello"));
        *SAID.lock().unwrap() = Some(said);
        rsx! {
            Column {
                Text { text: said(), streaming: true }
            }
        }
    }

    let mut host = Host::new(growing);
    let initial = decode_batch(host.rebuild().unwrap()).unwrap();
    let node_id = first_text_node(&initial);
    assert!(
        initial.iter().any(|mutation| matches!(
            mutation,
            Mutation::SetProp {
                node_id: target,
                property: PropertyKind::Text,
                value: PropertyValue::String("Hello"),
            } if *target == node_id
        )),
        "the first value of a streaming text is sent whole: {initial:?}"
    );
    drop(initial);

    let mut said = SAID.lock().unwrap().expect("the component never ran");
    said.write().push_str(", world");
    let decoded = decode_batch(host.render_frame(0).unwrap()).unwrap();
    assert_eq!(
        decoded,
        vec![Mutation::AppendText {
            node_id,
            text: ", world",
        }]
    );

    // A change that is not growth at the end goes whole, so the screen never disagrees.
    said.set(String::from("Goodbye"));
    let decoded = decode_batch(host.render_frame(0).unwrap()).unwrap();
    assert_eq!(
        decoded,
        vec![Mutation::SetProp {
            node_id,
            property: PropertyKind::Text,
            value: PropertyValue::String("Goodbye"),
        }]
    );

    // And growth after that is measured from what was sent last.
    said.write().push('!');
    let decoded = decode_batch(host.render_frame(0).unwrap()).unwrap();
    assert_eq!(decoded, vec![Mutation::AppendText { node_id, text: "!" }]);
}

/// A `Text` that is not marked keeps sending its whole string, as it always did.
#[test]
fn fr37_an_unmarked_text_still_sends_the_whole_string() {
    use dioxus_compose::protocol::PropertyValue;
    use std::sync::Mutex;

    static SAID: Mutex<Option<SyncSignal<String>>> = Mutex::new(None);

    fn plain() -> Element {
        let said = use_signal_sync(|| String::from("12"));
        *SAID.lock().unwrap() = Some(said);
        rsx! { Text { text: said() } }
    }

    let mut host = Host::new(plain);
    let _ = host.rebuild().unwrap();
    let mut said = SAID.lock().unwrap().expect("the component never ran");
    said.write().push('3');
    let decoded = decode_batch(host.render_frame(0).unwrap()).unwrap();
    assert!(
        decoded
            .iter()
            .all(|mutation| !matches!(mutation, Mutation::AppendText { .. })),
        "a text nobody marked as streaming was sent as an append: {decoded:?}"
    );
    assert!(decoded.iter().any(|mutation| matches!(
        mutation,
        Mutation::SetProp {
            value: PropertyValue::String("123"),
            ..
        }
    )));
}

/// A press on a link run reaches the `Text`'s `on_link` with the value the run was given,
/// and two links in one string report two different values.
#[test]
fn fr37_a_link_run_reports_its_own_value_to_on_link() {
    use dioxus_compose::protocol::{HostEvent, PropertyValue};
    use dioxus_compose::schema::{EventPayload, PropertyKind};
    use dioxus_compose::spans::{TextSpan, TextSpans};
    use std::sync::Mutex;

    static PRESSED: Mutex<Vec<u64>> = Mutex::new(Vec::new());

    fn linked() -> Element {
        let mut first = TextSpan::new(0, 3).underline();
        first.on_click = Some(7);
        let mut second = TextSpan::new(8, 4).underline();
        second.on_click = Some(9);
        let spans = TextSpans::new([first, TextSpan::new(4, 3).bold(), second]);
        rsx! {
            Text {
                text: "one two four",
                spans,
                on_link: move |value: u64| PRESSED.lock().unwrap().push(value),
            }
        }
    }

    let mut host = Host::new(linked);
    let initial = decode_batch(host.rebuild().unwrap()).unwrap();
    let (node_id, bytes) = initial
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::SetProp {
                node_id,
                property: PropertyKind::Spans,
                value: PropertyValue::Bytes(bytes),
            } => Some((*node_id, bytes.to_vec())),
            _ => None,
        })
        .expect("the runs did not travel");
    drop(initial);
    let runs: Vec<_> = TextSpans::from_bytes(bytes).spans().collect();
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[1].on_click, None, "the bold run is not a link");
    let first = runs[0].on_click.expect("the first link has no handler");
    let second = runs[2].on_click.expect("the second link has no handler");
    assert_ne!(first, second, "two links share one handler");

    for handler_id in [second, first] {
        let _ = host
            .dispatch(HostEvent {
                node_id,
                handler_id,
                payload: EventPayload::Clicked,
            })
            .unwrap();
    }
    assert_eq!(*PRESSED.lock().unwrap(), vec![9, 7]);
}
