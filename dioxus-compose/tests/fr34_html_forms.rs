//! Form controls: the events a renderer sends back for them (`input`, `change`, `submit`),
//! what a click on a checkbox, radio button or submit button does as a browser does it,
//! checkboxes that the app unticks, and `<select>` as a field of its own.
//!
//! Text is sized by a measurer that gives every character 10px and every line 20px.

use std::cell::RefCell;

use dioxus_compose::html::prelude::*;
use dioxus_compose::html::{
    HtmlConfig, HtmlDom, InputKind, LiteralColours, NodeId, PlanKey, PlanKind, Rect,
    TextMeasureRequest, TextMeasurer, TextMetrics, WidthConstraint,
};
use dioxus_core::ScopeId;
use dioxus_html::FormValue;

struct FixedAdvance;

impl TextMeasurer for FixedAdvance {
    fn measure(&mut self, request: &TextMeasureRequest<'_>) -> TextMetrics {
        let width = match request.width {
            WidthConstraint::AtMost(width) => {
                (request.text.chars().count() as f32 * 10.0).min(width.max(0.0))
            }
            _ => request.text.chars().count() as f32 * 10.0,
        };
        TextMetrics {
            width,
            height: 20.0,
            first_baseline: 15.0,
            line_count: 1,
        }
    }
}

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
        measurer: Some(Box::new(FixedAdvance)),
        ..HtmlConfig::default()
    }
}

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static VALUES: RefCell<Vec<(String, FormValue)>> = const { RefCell::new(Vec::new()) };
    static CHECKED: RefCell<bool> = const { RefCell::new(true) };
}

fn log(line: String) {
    LOG.with(|log| log.borrow_mut().push(line));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

fn take_values() -> Vec<(String, FormValue)> {
    VALUES.with(|values| std::mem::take(&mut *values.borrow_mut()))
}

fn text(value: &str) -> FormValue {
    FormValue::Text(value.to_string())
}

fn open(app: fn() -> Element) -> HtmlDom {
    take_log();
    take_values();
    let mut dom = HtmlDom::with_config(app, config());
    dom.layout(400.0, 300.0, 1.0);
    dom
}

#[track_caller]
fn node(dom: &HtmlDom, id: &str) -> NodeId {
    dom.element_by_id(id)
        .unwrap_or_else(|| panic!("no element with id {id}"))
}

#[track_caller]
fn rect(dom: &HtmlDom, id: &str) -> Rect {
    let node = node(dom, id);
    dom.display_list()
        .expect("laid out")
        .get(node)
        .unwrap_or_else(|| panic!("#{id} has no display list entry"))
        .rect
}

#[track_caller]
fn click(dom: &mut HtmlDom, id: &str) -> Option<NodeId> {
    let rect = rect(dom, id);
    dom.click(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

#[track_caller]
fn kind(dom: &HtmlDom, id: &str) -> InputKind {
    let node = node(dom, id);
    dom.display_list()
        .expect("laid out")
        .get(node)
        .and_then(|entry| entry.input.clone())
        .unwrap_or_else(|| panic!("#{id} is not a form field"))
        .kind
}

fn relayout(dom: &mut HtmlDom) {
    dom.render();
    dom.layout(400.0, 300.0, 1.0);
}

fn text_field() -> Element {
    rsx! {
        input {
            id: "name",
            value: "Ada",
            style: "width: 120px; height: 24px",
            oninput: move |event| log(format!("input:{}", event.value())),
            onchange: move |event| log(format!("change:{}", event.value())),
        }
    }
}

/// The text a renderer commits reaches `oninput` and `onchange` as the event's value, and
/// is not written into the document: the field still shows what the app set.
#[test]
fn fr34_committed_text_reaches_input_and_change_handlers() {
    let mut dom = open(text_field);
    let name = node(&dom, "name");

    assert_eq!(dom.input(name, "Grace"), Some(name));
    assert_eq!(take_log(), ["input:Grace"]);
    assert_eq!(dom.change(name, "Grace Hopper"), Some(name));
    assert_eq!(take_log(), ["change:Grace Hopper"]);

    relayout(&mut dom);
    let entry = dom.display_list().unwrap().get(name).unwrap();
    assert_eq!(
        entry.input.as_ref().unwrap().value,
        "Ada",
        "the renderer owns the text being edited; nothing round-trips into the value"
    );
}

fn checkbox() -> Element {
    rsx! {
        input {
            id: "box",
            r#type: "checkbox",
            style: "width: 16px; height: 16px; margin: 0",
            oninput: move |event| log(format!("input:{}", event.checked())),
            onchange: move |event| log(format!("change:{}:{}", event.value(), event.checked())),
        }
    }
}

/// A click on a checkbox ticks it and sends `input` and `change`, as a browser does,
/// without any click handler; a second click clears it.
#[test]
fn fr34_checkbox_click_ticks_it_and_sends_change() {
    let mut dom = open(checkbox);
    assert_eq!(kind(&dom, "box"), InputKind::Checkbox { checked: false });

    assert_eq!(
        click(&mut dom, "box"),
        None,
        "nothing listens for the click"
    );
    assert_eq!(take_log(), ["input:true", "change:true:true"]);
    relayout(&mut dom);
    assert_eq!(kind(&dom, "box"), InputKind::Checkbox { checked: true });

    click(&mut dom, "box");
    assert_eq!(take_log(), ["input:false", "change:false:false"]);
    relayout(&mut dom);
    assert_eq!(kind(&dom, "box"), InputKind::Checkbox { checked: false });

    // What the renderer's own checkbox reports goes the same way.
    let node = node(&dom, "box");
    dom.check(node, true);
    assert_eq!(take_log(), ["input:true", "change:true:true"]);
    dom.check(node, true);
    assert!(take_log().is_empty(), "already ticked: nothing changes");
}

fn guarded_checkbox() -> Element {
    rsx! {
        input {
            id: "box",
            r#type: "checkbox",
            style: "width: 16px; height: 16px; margin: 0",
            onclick: move |event| {
                log("click".to_string());
                event.prevent_default();
            },
            onchange: move |_| log("change".to_string()),
        }
    }
}

/// A click handler that prevents the default keeps the checkbox as it was, and no
/// `change` is sent.
#[test]
fn fr34_checkbox_click_that_prevents_the_default_changes_nothing() {
    let mut dom = open(guarded_checkbox);
    let target = node(&dom, "box");
    assert_eq!(click(&mut dom, "box"), Some(target));
    assert_eq!(take_log(), ["click"]);
    relayout(&mut dom);
    assert_eq!(kind(&dom, "box"), InputKind::Checkbox { checked: false });
}

fn app_checkbox() -> Element {
    let checked = CHECKED.with(|checked| *checked.borrow());
    rsx! {
        input {
            id: "box",
            r#type: "checkbox",
            checked,
            style: "width: 16px; height: 16px; margin: 0",
        }
    }
}

/// When the app turns `checked` off, the box draws unchecked, and on again, checked.
#[test]
fn fr34_checkbox_follows_the_apps_checked_both_ways() {
    CHECKED.with(|checked| *checked.borrow_mut() = true);
    let mut dom = open(app_checkbox);
    assert_eq!(kind(&dom, "box"), InputKind::Checkbox { checked: true });

    CHECKED.with(|checked| *checked.borrow_mut() = false);
    dom.virtual_dom_mut().mark_dirty(ScopeId::APP);
    relayout(&mut dom);
    assert_eq!(kind(&dom, "box"), InputKind::Checkbox { checked: false });

    CHECKED.with(|checked| *checked.borrow_mut() = true);
    dom.virtual_dom_mut().mark_dirty(ScopeId::APP);
    relayout(&mut dom);
    assert_eq!(kind(&dom, "box"), InputKind::Checkbox { checked: true });
}

fn labelled() -> Element {
    rsx! {
        label { id: "label", style: "display: block; width: 300px",
            input {
                id: "box",
                r#type: "checkbox",
                style: "width: 16px; height: 16px; margin: 0",
                onclick: move |_| log("box click".to_string()),
                onchange: move |event| log(format!("change:{}", event.value())),
            }
            "Remember me"
        }
    }
}

/// A click on a label, away from its checkbox, clicks the checkbox too: its click
/// handler runs, it ticks, and it sends `change`.
#[test]
fn fr34_label_click_clicks_its_control() {
    let mut dom = open(labelled);
    let checkbox = node(&dom, "box");
    let label = rect(&dom, "label");

    let target = dom.click(label.x + 280.0, label.y + label.height / 2.0);
    assert_eq!(target, Some(checkbox));
    assert_eq!(take_log(), ["box click", "change:true"]);
    relayout(&mut dom);
    assert_eq!(kind(&dom, "box"), InputKind::Checkbox { checked: true });
}

fn radios() -> Element {
    rsx! {
        div {
            for (index, choice) in ["small", "large"].into_iter().enumerate() {
                input {
                    key: "{choice}",
                    id: "size-{index}",
                    r#type: "radio",
                    name: "size",
                    value: "{choice}",
                    checked: index == 0,
                    style: "display: block; width: 16px; height: 16px; margin: 0",
                    onchange: move |event| log(format!("change:size-{index}:{}", event.value())),
                }
            }
        }
    }
}

/// Picking a radio button checks it, clears the others of its group, and sends `change`
/// to the one picked only. Picking it again changes nothing.
#[test]
fn fr34_radio_click_picks_one_of_its_group() {
    let mut dom = open(radios);
    assert_eq!(kind(&dom, "size-0"), InputKind::Radio { checked: true });

    click(&mut dom, "size-1");
    assert_eq!(take_log(), ["change:size-1:true"]);
    relayout(&mut dom);
    assert_eq!(kind(&dom, "size-0"), InputKind::Radio { checked: false });
    assert_eq!(kind(&dom, "size-1"), InputKind::Radio { checked: true });

    click(&mut dom, "size-1");
    assert!(take_log().is_empty());
}

fn signup() -> Element {
    rsx! {
        form {
            id: "form",
            onsubmit: move |event| {
                log("submit".to_string());
                VALUES.with(|values| *values.borrow_mut() = event.values());
            },
            input {
                id: "title",
                name: "title",
                value: "Milk",
                style: "display: block; width: 100px; height: 20px",
            }
            input {
                id: "urgent",
                r#type: "checkbox",
                name: "urgent",
                checked: true,
                style: "display: block; width: 16px; height: 16px; margin: 0",
            }
            select { id: "list", name: "list", value: "home",
                option { value: "work", "Work" }
                option { value: "home", "Home" }
            }
            input { name: "token", r#type: "hidden", value: "x" }
            button {
                id: "save",
                name: "action",
                value: "save",
                style: "display: block; width: 60px; height: 20px",
                onclick: move |_| log("click".to_string()),
                "Save"
            }
        }
    }
}

/// Clicking a submit button runs its click handler, then submits its form with the
/// form's named values: the committed text, the ticked checkbox, the chosen option and the
/// button itself.
#[test]
fn fr34_submit_button_click_submits_its_form() {
    let mut dom = open(signup);
    let save = node(&dom, "save");
    let title = node(&dom, "title");

    assert_eq!(click(&mut dom, "save"), Some(save));
    assert_eq!(take_log(), ["click", "submit"]);
    assert_eq!(
        take_values(),
        vec![
            ("title".to_string(), text("Milk")),
            ("urgent".to_string(), text("on")),
            ("list".to_string(), text("home")),
            ("token".to_string(), text("x")),
            ("action".to_string(), text("save")),
        ]
    );

    // What the renderer committed is what the form holds.
    dom.input(title, "Bread");
    let list = node(&dom, "list");
    dom.select(list, 0);
    click(&mut dom, "save");
    assert_eq!(take_log(), ["click", "submit"]);
    let values = take_values();
    assert_eq!(values[0], ("title".to_string(), text("Bread")));
    assert_eq!(values[2], ("list".to_string(), text("work")));

    // Enter in a field clicks the form's submit button.
    assert_eq!(dom.submit(title), Some(save));
    assert_eq!(take_log(), ["click", "submit"]);
}

fn plain_form() -> Element {
    rsx! {
        form {
            id: "form",
            onsubmit: move |event| {
                event.prevent_default();
                log(format!("submit:{}", event.values().len()));
            },
            input { id: "query", name: "q", value: "" }
            button { id: "go", r#type: "button", onclick: move |_| log("click".to_string()), "Go" }
        }
    }
}

/// A form with no submit button is submitted directly when Enter is pressed in a field. A
/// `type="button"` button does not submit.
#[test]
fn fr34_enter_submits_a_form_without_a_submit_button() {
    let mut dom = open(plain_form);
    let query = node(&dom, "query");
    let form = node(&dom, "form");

    click(&mut dom, "go");
    assert_eq!(take_log(), ["click"]);

    dom.input(query, "rust");
    assert_eq!(dom.submit(query), Some(form));
    assert_eq!(take_log(), ["submit:1"]);
    assert_eq!(dom.submit(form), Some(form), "the form itself can be named");
    assert_eq!(take_log(), ["submit:1"]);
}

fn chooser() -> Element {
    rsx! {
        select {
            id: "size",
            value: "three",
            oninput: move |event| log(format!("input:{}", event.value())),
            onchange: move |event| log(format!("change:{}", event.value())),
            option { value: "one", "One" }
            optgroup { label: "More",
                option { value: "three", "Three" }
                option { value: "seven", label: "Seven!", "Seven" }
            }
        }
        p { id: "after", style: "margin: 0", "After" }
    }
}

/// A `<select>` is one field: a box as wide as its widest option plus room for the
/// dropdown's indicator, one line tall, listing its options with the chosen one, and a
/// `Dropdown` in the plan. Its options are not drawn as text in the page.
#[test]
fn fr34_select_is_a_dropdown_field_with_its_options() {
    let mut dom = open(chooser);

    assert_eq!(
        kind(&dom, "size"),
        InputKind::Select {
            options: vec!["One".to_string(), "Three".to_string(), "Seven!".to_string()],
            selected: Some(1),
        }
    );
    let size = node(&dom, "size");
    let list = dom.display_list().unwrap();
    let entry = list.get(size).unwrap();
    assert_eq!(entry.input.as_ref().unwrap().value, "three");
    // "Seven!" is the widest label: 6 characters at 10px, plus 24px for the indicator.
    assert_eq!(entry.rect.width, 60.0 + 24.0);
    assert_eq!(entry.input.as_ref().unwrap().content_rect.height, 20.0);
    assert!(entry.texts.is_empty());
    let drawn: Vec<&str> = list
        .entries
        .iter()
        .flat_map(|entry| entry.texts.iter())
        .map(|run| run.text.as_str())
        .collect();
    assert_eq!(drawn, ["After"], "no option is drawn in the page");

    let plan = dom.plan(&mut LiteralColours).expect("laid out");
    let field = plan
        .find(PlanKey::Field(size))
        .expect("the select has a field");
    let PlanKind::Dropdown(dropdown) = &field.kind else {
        panic!("a <select> is a Dropdown: {field:#?}");
    };
    assert_eq!(dropdown.options, ["One", "Three", "Seven!"]);
    assert_eq!(dropdown.selected, Some(1));
}

/// Picking an option sends `input` and then `change` with its value, as a browser does.
#[test]
fn fr34_select_pick_sends_input_then_change() {
    let mut dom = open(chooser);
    let size = node(&dom, "size");

    assert_eq!(dom.select(size, 2), Some(size));
    assert_eq!(take_log(), ["input:seven", "change:seven"]);
    assert_eq!(dom.change(size, "one"), Some(size));
    assert_eq!(take_log(), ["input:one", "change:one"]);
    assert_eq!(dom.select(size, 3), None, "there is no fourth option");
    assert!(take_log().is_empty());
}
