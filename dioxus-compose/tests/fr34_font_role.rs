//! Text from a page is text with no rung of the type ladder.
//!
//! compose-rust takes a node's `Font` only when its type role is `TypeRole::None`, which is
//! tag 0 on the wire. Text with any other role, or with none sent, is drawn and measured in
//! the theme's family, so a page's `font-family` would be lost. Every Text and TextField a
//! page draws, and every question the page asks the renderer's measure call, says no role.

use std::cell::RefCell;
use std::ffi::c_int;

use dioxus_compose::Host;
use dioxus_compose::boundary::{RendererApi, install_renderer_api};
use dioxus_compose::fonts::{FontRef, FontRefs};
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::html::prelude::*;
use dioxus_compose::measure::{
    MEASURE_RECORD_LEN, MEASURE_TEXT, MeasureResult, RESULT_OK, RESULT_UNKNOWN_NODE,
    TEXT_TEXT_LENGTH_AT, TEXT_TEXT_OFFSET_AT, TEXT_TYPE_ROLE_AT,
};
use dioxus_compose::protocol::{Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{GenericFamily, PropertyKind, TypeRole, WidgetKind};
use dioxus_compose::window::reset_window_size;

thread_local! {
    /// The text and the type role tag of every text question this thread's renderer got.
    static ASKED: RefCell<Vec<(String, u16)>> = const { RefCell::new(Vec::new()) };
}

extern "C" fn never_run() -> c_int {
    0
}

extern "C" fn no_frames() {}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Records each text question and answers it with a fixed size.
unsafe extern "C" fn record_roles(
    requests: *const u8,
    length: u32,
    count: u32,
    results: *mut MeasureResult,
) -> i32 {
    // SAFETY: the caller hands `length` readable bytes and room for `count` results.
    let buffer = unsafe { std::slice::from_raw_parts(requests, length as usize) };
    for index in 0..count as usize {
        let record = &buffer[index * MEASURE_RECORD_LEN..(index + 1) * MEASURE_RECORD_LEN];
        let result = if read_u16(record, 0) == MEASURE_TEXT {
            let at = read_u32(record, TEXT_TEXT_OFFSET_AT) as usize;
            let len = read_u32(record, TEXT_TEXT_LENGTH_AT) as usize;
            let text = std::str::from_utf8(&buffer[at..at + len])
                .expect("the text is UTF-8")
                .to_string();
            let role = read_u16(record, TEXT_TYPE_ROLE_AT);
            ASKED.with(|asked| asked.borrow_mut().push((text, role)));
            MeasureResult {
                width: 80.0,
                height: 20.0,
                first_baseline: 15.0,
                line_count: 1,
                status: RESULT_OK,
                ..MeasureResult::default()
            }
        } else {
            MeasureResult {
                status: RESULT_UNKNOWN_NODE,
                ..MeasureResult::default()
            }
        };
        // SAFETY: `index` is below `count`.
        unsafe { results.add(index).write(result) };
    }
    0
}

fn install() {
    let _ = install_renderer_api(RendererApi {
        run: never_run,
        request_frame: no_frames,
        measure: record_roles,
    });
    ASKED.with(|asked| asked.borrow_mut().clear());
}

fn page() -> Element {
    rsx! {
        div { style: "display: flex; flex-direction: column; width: 400px",
            p { style: "font-family: Georgia, serif", "serif-run" }
            input { style: "font-family: Menlo, monospace", value: "field-run" }
        }
    }
}

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
        ..HtmlConfig::default()
    }
}

/// One property as it travelled: its kind and its integer or byte value.
#[derive(Debug)]
enum Sent {
    Integer(i64),
    Str(String),
    Bytes(Vec<u8>),
    Other,
}

/// The first frame of the page, as (node, property, value) for every property it sets,
/// and the nodes it creates with their widget.
fn first_frame() -> (Vec<(u32, WidgetKind)>, Vec<(u32, PropertyKind, Sent)>) {
    reset_window_size();
    let mut host = Host::html(page, config);
    let batch = host.rebuild().unwrap().to_vec();
    let mutations = decode_batch(&batch).unwrap();
    let mut created = Vec::new();
    let mut props = Vec::new();
    for mutation in mutations {
        match mutation {
            Mutation::Create { node_id, widget } => created.push((node_id, widget)),
            Mutation::SetProp {
                node_id,
                property,
                value,
            } => props.push((
                node_id,
                property,
                match value {
                    PropertyValue::Integer(value) => Sent::Integer(value),
                    PropertyValue::String(value) => Sent::Str(value.to_string()),
                    PropertyValue::Bytes(bytes) => Sent::Bytes(bytes.to_vec()),
                    _ => Sent::Other,
                },
            )),
            _ => {}
        }
    }
    (created, props)
}

#[track_caller]
fn assert_no_role_and_font(
    what: &str,
    node: u32,
    props: &[(u32, PropertyKind, Sent)],
    expected: &FontRefs,
) {
    let of_node: Vec<_> = props.iter().filter(|(id, ..)| *id == node).collect();
    let role = of_node
        .iter()
        .find(|(_, property, _)| *property == PropertyKind::TypeRole)
        .unwrap_or_else(|| panic!("the {what} sent no type role: {of_node:?}"));
    assert!(
        matches!(role.2, Sent::Integer(tag) if tag == TypeRole::None as i64),
        "the {what} has to say no role (tag {}), so the renderer takes its fonts: {:?}",
        TypeRole::None as i64,
        role.2
    );
    let font = of_node
        .iter()
        .find(|(_, property, _)| *property == PropertyKind::Font)
        .unwrap_or_else(|| panic!("the {what} sent no font: {of_node:?}"));
    assert!(
        matches!(&font.2, Sent::Bytes(bytes) if bytes.as_slice() == expected.as_bytes()),
        "the {what} has to carry its font-family list as its fonts: {:?}",
        font.2
    );
}

#[test]
fn fr34_html_text_and_field_carry_no_role_and_their_fonts() {
    install();
    let (created, props) = first_frame();

    let text = props
        .iter()
        .find_map(|(id, property, value)| {
            let is_run = *property == PropertyKind::Text
                && matches!(value, Sent::Str(text) if text == "serif-run");
            is_run.then_some(*id)
        })
        .filter(|id| created.contains(&(*id, WidgetKind::Text)))
        .expect("the paragraph was not drawn as a Text");
    let serif = FontRefs::new([
        FontRef::System("Georgia".to_string()),
        FontRef::Generic(GenericFamily::Serif),
    ]);
    assert_no_role_and_font("paragraph's Text", text, &props, &serif);

    let field = created
        .iter()
        .find_map(|(id, widget)| (*widget == WidgetKind::TextField).then_some(*id))
        .expect("the input was not drawn as a TextField");
    let mono = FontRefs::new([
        FontRef::System("Menlo".to_string()),
        FontRef::Generic(GenericFamily::Monospace),
    ]);
    assert_no_role_and_font("input's TextField", field, &props, &mono);
}

#[test]
fn fr34_html_measure_requests_carry_no_role() {
    install();
    first_frame();
    let asked = ASKED.with(|asked| asked.borrow().clone());
    let serif = asked
        .iter()
        .find(|(text, _)| text == "serif-run")
        .unwrap_or_else(|| panic!("the paragraph was never measured: {asked:?}"));
    assert_eq!(
        serif.1,
        TypeRole::None as u16,
        "the paragraph is measured with a role, so the renderer measures it in the theme's family"
    );
    for (text, role) in &asked {
        assert_eq!(
            *role,
            TypeRole::None as u16,
            "{text:?} was measured with role tag {role}, not with no role"
        );
    }
}
