//! Incremental style/layout/plan, `font-family` forwarding, programmatic focus, and
//! keyboard/IME composition events on the HTML path.

use std::cell::RefCell;
use std::ffi::c_int;
use std::rc::Rc;

use dioxus_compose::Host;
use dioxus_compose::boundary::{RendererApi, install_renderer_api};
use dioxus_compose::fonts::{FontRef, FontRefs};
use dioxus_compose::html::prelude::dioxus_elements::input_data::keyboard_types::{
    Code, Key, Location, Modifiers,
};
use dioxus_compose::html::prelude::dioxus_elements::point_interaction::ModifiersInteraction;
use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    HtmlConfig, HtmlDom, KeyInput, LiteralColours, OverflowWrap, PlanBridge, RendererMeasurer,
    TextLineHeight, TextMeasureRequest, TextMeasurer, TextMetrics, TextStyle, TextWhiteSpace,
    WidthConstraint, WordBreak,
};
use dioxus_compose::measure::{
    MEASURE_RECORD_LEN, MEASURE_TEXT, MeasureResult, RESULT_OK, RESULT_UNKNOWN_NODE,
    TEXT_FONT_COUNT_AT, TEXT_FONT_OFFSET_AT, TEXT_TEXT_LENGTH_AT, TEXT_TEXT_OFFSET_AT,
};
use dioxus_compose::protocol::{Mutation, PropertyValue, decode_batch};
use dioxus_compose::schema::{GenericFamily, Modifier, PropertyKind};
use dioxus_compose::window::reset_window_size;
use dioxus_hooks::use_signal;

struct CountingMeasurer {
    calls: Rc<RefCell<Vec<String>>>,
}

impl CountingMeasurer {
    fn new(calls: Rc<RefCell<Vec<String>>>) -> Self {
        Self { calls }
    }
}

impl TextMeasurer for CountingMeasurer {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        self.calls.borrow_mut().push(request.text.to_string());
        let width = match request.width {
            WidthConstraint::AtMost(width) => {
                (request.text.chars().count() as f32 * 8.0).min(width.max(0.0))
            }
            _ => request.text.chars().count() as f32 * 8.0,
        };
        TextMetrics {
            width,
            height: 18.0,
            first_baseline: 14.0,
            line_count: 1,
        }
    }
}

#[test]
fn fr34_6_unchanged_frame_skips_layout_and_emits_no_records() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let calls_for_config = calls.clone();
    thread_local! {
        static CALLS: RefCell<Option<Rc<RefCell<Vec<String>>>>> = const { RefCell::new(None) };
    }
    CALLS.with(|slot| *slot.borrow_mut() = Some(calls_for_config));

    fn app() -> Element {
        rsx! {
            div { style: "display: flex; flex-direction: column; width: 400px",
                span { "first line" }
                span { "second line" }
            }
        }
    }

    fn cfg() -> HtmlConfig {
        let sink = CALLS.with(|slot| slot.borrow().clone().unwrap());
        HtmlConfig {
            stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
            measurer: Some(Box::new(CountingMeasurer::new(sink))),
            ..HtmlConfig::default()
        }
    }

    reset_window_size();
    let mut host = Host::html(app, cfg);
    let initial = decode_batch(host.rebuild().unwrap()).unwrap();
    assert!(!initial.is_empty());
    let initial_measures = calls.borrow().len();
    assert!(initial_measures >= 2);

    calls.borrow_mut().clear();
    let second = decode_batch(host.render_frame(16_000_000).unwrap()).unwrap();
    assert!(
        second.is_empty(),
        "unchanged frame should emit no mutation records, got: {second:?}"
    );
    assert!(
        calls.borrow().is_empty(),
        "unchanged frame should skip layout and text measurement, got: {:?}",
        *calls.borrow()
    );
}

#[test]
fn fr34_6_sibling_subtree_change_does_not_remeasure_clean_text() {
    let calls = Rc::new(RefCell::new(Vec::new()));

    fn app() -> Element {
        let mut count = use_signal(|| 0);
        rsx! {
            div { style: "display: flex; flex-direction: column; width: 500px",
                button {
                    id: "bump",
                    style: "width: 120px; height: 24px",
                    onclick: move |_| count += 1,
                    "count: {count}"
                }
                div { style: "height: 20px", "alpha stable line" }
                div { style: "height: 20px", "bravo stable line" }
                div { style: "height: 20px", "charlie stable line" }
            }
        }
    }

    let mut dom = HtmlDom::with_config(
        app,
        HtmlConfig {
            stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
            measurer: Some(Box::new(CountingMeasurer::new(calls.clone()))),
            ..HtmlConfig::default()
        },
    );

    dom.layout(600.0, 400.0, 1.0);
    assert!(calls.borrow().iter().any(|t| t == "alpha stable line"));
    assert!(calls.borrow().iter().any(|t| t == "bravo stable line"));
    assert!(calls.borrow().iter().any(|t| t == "charlie stable line"));

    calls.borrow_mut().clear();
    let button = dom.element_by_id("bump").unwrap();
    dom.click_node(button);
    dom.render();
    let list = dom.layout(600.0, 400.0, 1.0);

    let remeasured = calls.borrow().clone();
    assert!(
        remeasured.iter().any(|t| t == "count: 1"),
        "changed subtree text should be measured: {remeasured:?}"
    );
    for clean in [
        "alpha stable line",
        "bravo stable line",
        "charlie stable line",
    ] {
        assert!(
            !remeasured.iter().any(|t| t == clean),
            "clean sibling text {clean:?} should not be remeasured when only sibling changed: {remeasured:?}"
        );
        assert!(
            list.entries
                .iter()
                .any(|e| e.texts.iter().any(|r| r.text == clean)),
            "clean sibling text {clean:?} must remain in the display list"
        );
    }
}

#[test]
fn fr34_6_container_offset_or_transform_change_does_not_remeasure_children() {
    let calls = Rc::new(RefCell::new(Vec::new()));

    fn app() -> Element {
        let mut step = use_signal(|| 0);
        let top = if step() == 0 { "0px" } else { "-24px" };
        let transform = if step() < 2 {
            "scale(1)"
        } else {
            "scale(1.25)"
        };
        rsx! {
            div {
                id: "viewport",
                style: "position: relative; width: 600px; height: 400px; overflow: hidden",
                onclick: move |_| step += 1,
                div {
                    id: "lines",
                    style: "position: absolute; left: 0px; top: {top}; width: 600px; height: 800px; transform: {transform}",
                    div { style: "position: absolute; top: 0px; left: 0px; height: 20px", "line zero" }
                    div { style: "position: absolute; top: 20px; left: 0px; height: 20px", "line one" }
                    div { style: "position: absolute; top: 40px; left: 0px; height: 20px", "line two" }
                }
            }
        }
    }

    let mut dom = HtmlDom::with_config(
        app,
        HtmlConfig {
            stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
            measurer: Some(Box::new(CountingMeasurer::new(calls.clone()))),
            ..HtmlConfig::default()
        },
    );

    dom.layout(600.0, 400.0, 1.0);
    let mut bridge = PlanBridge::new();
    let mut batch = dioxus_compose::Batch::new();
    batch.begin();
    let plan0 = dom.plan(&mut LiteralColours).unwrap();
    bridge.apply(plan0, (600.0, 400.0), &dom, &mut batch);
    let _ = batch.finish().unwrap();

    // Step 1: change container's `top` from 0px to -24px.
    calls.borrow_mut().clear();
    let viewport = dom.element_by_id("viewport").unwrap();
    dom.click_node(viewport);
    dom.render();
    dom.layout(600.0, 400.0, 1.0);
    assert!(
        calls.borrow().is_empty(),
        "changing container `top` must not remeasure child text: {:?}",
        *calls.borrow()
    );
    let mut batch1 = dioxus_compose::Batch::new();
    batch1.begin();
    let plan1 = dom.plan(&mut LiteralColours).unwrap();
    bridge.apply(plan1, (600.0, 400.0), &dom, &mut batch1);
    let mutations1 = decode_batch(batch1.finish().unwrap()).unwrap();
    assert!(
        mutations1.iter().all(|m| matches!(
            m,
            Mutation::SetModifier {
                modifier: Modifier::Offset { .. },
                ..
            }
        )),
        "only the container's Offset modifier should be emitted when `top` changes, got: {mutations1:?}"
    );

    // Step 2: change container's `transform` from translateY(0px) to translateY(-48px).
    calls.borrow_mut().clear();
    dom.click_node(viewport);
    dom.render();
    dom.layout(600.0, 400.0, 1.0);
    assert!(
        calls.borrow().is_empty(),
        "changing container `transform` must not remeasure child text: {:?}",
        *calls.borrow()
    );
    let mut batch2 = dioxus_compose::Batch::new();
    batch2.begin();
    let plan2 = dom.plan(&mut LiteralColours).unwrap();
    bridge.apply(plan2, (600.0, 400.0), &dom, &mut batch2);
    let mutations2 = decode_batch(batch2.finish().unwrap()).unwrap();
    assert!(
        mutations2.iter().all(|m| matches!(
            m,
            Mutation::SetModifier {
                modifier: Modifier::Transform { .. },
                ..
            }
        )),
        "only the container's Transform modifier should be emitted when `transform` changes, got: {mutations2:?}"
    );
}

#[test]
fn fr34_7_font_family_reaches_renderer_measurer_and_plan_bridge() {
    thread_local! {
        static RECORDED_FONTS: RefCell<Vec<(String, Vec<u8>)>> = const { RefCell::new(Vec::new()) };
    }

    extern "C" fn never_run() -> c_int {
        0
    }
    extern "C" fn no_frames() {}

    fn read_u32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }

    unsafe extern "C" fn capture_font_measure(
        requests: *const u8,
        length: u32,
        count: u32,
        results: *mut MeasureResult,
    ) -> i32 {
        let buffer = unsafe { std::slice::from_raw_parts(requests, length as usize) };
        for index in 0..count as usize {
            let record = &buffer[index * MEASURE_RECORD_LEN..(index + 1) * MEASURE_RECORD_LEN];
            let result = if u16::from_le_bytes([record[0], record[1]]) == MEASURE_TEXT {
                let text_at = read_u32(record, TEXT_TEXT_OFFSET_AT) as usize;
                let text_len = read_u32(record, TEXT_TEXT_LENGTH_AT) as usize;
                let text = std::str::from_utf8(&buffer[text_at..text_at + text_len])
                    .unwrap()
                    .to_string();
                let font_at = read_u32(record, TEXT_FONT_OFFSET_AT) as usize;
                let font_count = read_u32(record, TEXT_FONT_COUNT_AT) as usize;
                let mut refs = Vec::with_capacity(font_count);
                for i in 0..font_count {
                    let rec = font_at + i * 12;
                    let kind = read_u32(buffer, rec);
                    let value = read_u32(buffer, rec + 4);
                    let len = read_u32(buffer, rec + 8);
                    match kind {
                        2 => {
                            let name = std::str::from_utf8(
                                &buffer[value as usize..(value + len) as usize],
                            )
                            .unwrap()
                            .to_string();
                            refs.push(FontRef::System(name));
                        }
                        3 => {
                            let fam = match value {
                                v if v == GenericFamily::Serif as u32 => GenericFamily::Serif,
                                v if v == GenericFamily::SansSerif as u32 => {
                                    GenericFamily::SansSerif
                                }
                                v if v == GenericFamily::Monospace as u32 => {
                                    GenericFamily::Monospace
                                }
                                v if v == GenericFamily::SystemUi as u32 => GenericFamily::SystemUi,
                                _ => GenericFamily::SansSerif,
                            };
                            refs.push(FontRef::Generic(fam));
                        }
                        _ => {}
                    }
                }
                let font = FontRefs::new(refs).as_bytes().to_vec();
                RECORDED_FONTS.with(|slot| slot.borrow_mut().push((text, font)));
                MeasureResult {
                    width: 60.0,
                    height: 18.0,
                    first_baseline: 14.0,
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
            unsafe { results.add(index).write(result) };
        }
        0
    }

    let _ = install_renderer_api(RendererApi {
        run: never_run,
        request_frame: no_frames,
        measure: capture_font_measure,
    });
    RECORDED_FONTS.with(|slot| slot.borrow_mut().clear());

    let expected_mono = FontRefs::new([
        FontRef::System("JetBrains Mono".to_string()),
        FontRef::Generic(GenericFamily::Monospace),
    ]);
    let expected_sans = FontRefs::new([FontRef::Generic(GenericFamily::SansSerif)]);

    // 1. Verify RendererMeasurer cache key separates distinct font-family lists with identical text.
    let mut measurer = RendererMeasurer::new();
    let style_mono = TextStyle {
        font_family: vec!["JetBrains Mono".to_string(), "monospace".to_string()],
        font_size: 13.0,
        font_weight: 400.0,
        italic: false,
        letter_spacing: 0.0,
        line_height: TextLineHeight::Normal,
        word_break: WordBreak::Normal,
        overflow_wrap: OverflowWrap::Normal,
    };
    let style_sans = TextStyle {
        font_family: vec!["sans-serif".to_string()],
        ..style_mono.clone()
    };
    let _ = measurer.measure(&TextMeasureRequest {
        text: "same-glyphs",
        style: &style_mono,
        width: WidthConstraint::MaxContent,
        white_space: TextWhiteSpace::NOWRAP,
    });
    let _ = measurer.measure(&TextMeasureRequest {
        text: "same-glyphs",
        style: &style_sans,
        width: WidthConstraint::MaxContent,
        white_space: TextWhiteSpace::NOWRAP,
    });

    // 2. Run through Host::html to verify both measure wire records and PlanBridge PropertyKind::Font.
    fn app() -> Element {
        rsx! {
            div { style: "display: flex; flex-direction: column; width: 400px",
                span { style: "font-family: 'JetBrains Mono', monospace", "code-run" }
                span { style: "font-family: sans-serif", "ui-run" }
            }
        }
    }
    fn cfg() -> HtmlConfig {
        HtmlConfig {
            stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
            ..HtmlConfig::default()
        }
    }

    reset_window_size();
    let mut host = Host::html(app, cfg);
    let mutations = decode_batch(host.rebuild().unwrap()).unwrap();

    let recorded = RECORDED_FONTS.with(|slot| slot.borrow().clone());
    assert!(
        recorded
            .iter()
            .any(|(text, font)| text == "code-run" && font.as_slice() == expected_mono.as_bytes()),
        "RendererMeasurer should send JetBrains Mono, monospace FontRefs for 'code-run': {recorded:?}"
    );
    assert!(
        recorded
            .iter()
            .any(|(text, font)| text == "ui-run" && font.as_slice() == expected_sans.as_bytes()),
        "RendererMeasurer should send sans-serif FontRefs for 'ui-run': {recorded:?}"
    );

    let font_props: Vec<Vec<u8>> = mutations
        .iter()
        .filter_map(|m| match m {
            Mutation::SetProp {
                property: PropertyKind::Font,
                value: PropertyValue::Bytes(bytes),
                ..
            } => Some(bytes.to_vec()),
            _ => None,
        })
        .collect();
    assert!(
        font_props
            .iter()
            .any(|b| b.as_slice() == expected_mono.as_bytes()),
        "PlanBridge should emit PropertyKind::Font with JetBrains Mono, monospace FontRefs: {font_props:?}"
    );
    assert!(
        font_props
            .iter()
            .any(|b| b.as_slice() == expected_sans.as_bytes()),
        "PlanBridge should emit PropertyKind::Font with sans-serif FontRefs: {font_props:?}"
    );
}

#[test]
fn fr34_5_onmounted_set_focus_and_autofocus_focus_element() {
    thread_local! {
        static EVENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }
    EVENTS.with(|e| e.borrow_mut().clear());

    fn app_autofocus() -> Element {
        rsx! {
            div {
                input {
                    id: "auto-input",
                    autofocus: true,
                    onfocus: move |_| EVENTS.with(|e| e.borrow_mut().push("focus:auto".to_string())),
                    onblur: move |_| EVENTS.with(|e| e.borrow_mut().push("blur:auto".to_string())),
                }
                input {
                    id: "second-input",
                    onmounted: move |evt| async move {
                        EVENTS.with(|e| e.borrow_mut().push("mounted:second".to_string()));
                        let _ = evt.set_focus(true).await;
                    },
                    onfocus: move |_| EVENTS.with(|e| e.borrow_mut().push("focus:second".to_string())),
                }
            }
        }
    }

    let dom = HtmlDom::with_config(
        app_autofocus,
        HtmlConfig {
            stylesheets: vec![
                "html, body { margin: 0; padding: 0; } input:focus { background-color: rgb(255, 0, 0); }"
                    .to_string(),
            ],
            ..HtmlConfig::default()
        },
    );
    let second = dom.element_by_id("second-input").unwrap();
    assert_eq!(
        dom.focused(),
        Some(second),
        "onmounted set_focus(true) should leave #second-input focused"
    );
    let log = EVENTS.with(|e| e.borrow().clone());
    assert!(
        log.contains(&"mounted:second".to_string()),
        "onmounted handler should have run: {log:?}"
    );
    assert!(
        log.contains(&"focus:second".to_string()),
        "set_focus(true) in onmounted should fire onfocus on #second-input: {log:?}"
    );
}

#[test]
fn fr34_5_key_events_carry_metadata_and_tab_traverses_focus() {
    thread_local! {
        static KEY_LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }
    KEY_LOG.with(|l| l.borrow_mut().clear());

    fn app() -> Element {
        rsx! {
            div {
                onkeydown: move |evt| {
                    KEY_LOG.with(|l| {
                        l.borrow_mut().push(format!(
                            "down:{:?}:{:?}:{:?}:{}:{}:{}",
                            evt.key(),
                            evt.code(),
                            evt.location(),
                            evt.modifiers().contains(Modifiers::SHIFT),
                            evt.is_auto_repeating(),
                            evt.is_composing()
                        ));
                    });
                },
                onkeyup: move |evt| {
                    KEY_LOG.with(|l| {
                        l.borrow_mut().push(format!(
                            "up:{:?}:{:?}:{}",
                            evt.key(),
                            evt.code(),
                            evt.is_composing()
                        ));
                    });
                },
                input { id: "first" }
                button { id: "second", "Action" }
                div { id: "third", tabindex: "0", "Focusable box" }
            }
        }
    }

    let mut dom = HtmlDom::new(app);
    dom.layout(400.0, 300.0, 1.0);
    let first = dom.element_by_id("first").unwrap();
    let second = dom.element_by_id("second").unwrap();
    let third = dom.element_by_id("third").unwrap();

    dom.focus(first);
    assert_eq!(dom.focused(), Some(first));

    // Dispatch a repeated Numpad key with Shift and composing=true on the focused element.
    let repeat_key = KeyInput {
        key: Key::Character("1".to_string()),
        code: Code::Numpad1,
        location: Location::Numpad,
        modifiers: Modifiers::SHIFT,
        repeat: true,
        is_composing: true,
    };
    dom.key_down_input(None, &repeat_key);
    dom.key_up_input(None, &repeat_key);

    let log = KEY_LOG.with(|l| l.borrow().clone());
    assert!(
        log.contains(&"down:Character(\"1\"):Numpad1:Numpad:true:true:true".to_string()),
        "keydown should carry key, code, location, modifiers, repeat, and is_composing: {log:?}"
    );
    assert!(
        log.contains(&"up:Character(\"1\"):Numpad1:true".to_string()),
        "keyup should carry key, code, and is_composing: {log:?}"
    );

    // Tab moves focus from #first -> #second -> #third -> #first, and Shift+Tab moves back.
    let tab = KeyInput::new(Key::Tab, Code::Tab, Modifiers::empty());
    dom.key_down_input(None, &tab);
    assert_eq!(
        dom.focused(),
        Some(second),
        "Tab should move focus from #first to #second"
    );
    dom.key_down_input(None, &tab);
    assert_eq!(
        dom.focused(),
        Some(third),
        "Tab should move focus from #second to #third"
    );

    let shift_tab = KeyInput::new(Key::Tab, Code::Tab, Modifiers::SHIFT);
    dom.key_down_input(None, &shift_tab);
    assert_eq!(
        dom.focused(),
        Some(second),
        "Shift+Tab should move focus back from #third to #second"
    );
}

#[test]
fn fr34_5_composition_events_reach_focused_element() {
    thread_local! {
        static COMP_LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }
    COMP_LOG.with(|l| l.borrow_mut().clear());

    fn app() -> Element {
        rsx! {
            div {
                oncompositionstart: move |evt| {
                    COMP_LOG.with(|l| l.borrow_mut().push(format!("start:{}", evt.data().data())));
                },
                oncompositionupdate: move |evt| {
                    COMP_LOG.with(|l| l.borrow_mut().push(format!("update:{}", evt.data().data())));
                },
                oncompositionend: move |evt| {
                    COMP_LOG.with(|l| l.borrow_mut().push(format!("end:{}", evt.data().data())));
                },
                input { id: "editor" }
            }
        }
    }

    let mut dom = HtmlDom::new(app);
    let editor = dom.element_by_id("editor").unwrap();
    dom.focus(editor);

    assert!(dom.composition_start(editor, "ㅎ").is_some());
    assert!(dom.composition_update(editor, "하").is_some());
    assert!(dom.composition_end(editor, "한").is_some());

    let log = COMP_LOG.with(|l| l.borrow().clone());
    assert_eq!(
        log,
        vec![
            "start:ㅎ".to_string(),
            "update:하".to_string(),
            "end:한".to_string()
        ]
    );
}
