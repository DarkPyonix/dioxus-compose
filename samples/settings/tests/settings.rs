//! The settings form in an 800 by 900 viewport.
//!
//! Every number follows from `settings::STYLE` (all boxes `border-box`) and the support
//! measurer (an advance of half the font size per character, the CSS line height per line):
//!
//! - The form is 560px wide at x = 0; its 32px padding puts the content at x = 32, 496px
//!   wide. The heading is one 32px line at y = 32 with a 24px margin, so the first row is
//!   at y = 88.
//! - In each row the label is a fixed 120px and the control takes the rest after a 12px
//!   gap: x = 32 + 120 + 12 = 164, 496 - 132 = 364px wide. Controls are 36px tall (the
//!   text area 96px); rows are 16px apart: y = 88, 140 and 252.
//! - A label is one 20px line. Centred in a 36px row it sits 8px down; in the text area's
//!   row (`align-items: flex-start`) at the top.
//! - A control's text goes inside its 1px border and 7px by 11px padding: 12px in from the
//!   left and 8px down, 364 - 24 = 340px wide and 36 - 16 = 20px tall.
//! - The checkbox row is at y = 304 and is 20px tall; the 16px box is centred in it, 2px
//!   down. The radio rows are at y = 340 and 340 + 20 + 8 = 368.
//! - The actions row is at y = 340 + 48 + 16 = 404, and its 132px of left padding lines the
//!   button up with the controls at x = 164. "Save changes" is 12 characters at 7px, 84px,
//!   plus 16px of padding a side: 116px.
//!
//! Composing Korean (or any IME text) in these fields is not tested here: composition
//! belongs to the renderer's text field and never reaches the Host, so it is checked by
//! hand on the native-image build, following the manual IME checklist.

use dioxus_compose::html::{HtmlDom, InputKind, LiteralColours, Plan, PlanKey, PlanKind};
use sample_html_settings as settings;
use sample_html_support::{Measurer, assert_rect, centre, config, entry, node, plan_node, text_of};

fn laid_out() -> HtmlDom {
    let mut dom = HtmlDom::with_config(settings::app, config(settings::STYLE, Measurer::new()));
    dom.layout(800.0, 900.0, 1.0);
    dom
}

fn relayout(dom: &mut HtmlDom) {
    dom.render();
    dom.layout(800.0, 900.0, 1.0);
}

#[track_caller]
fn field_kind(plan: &Plan, dom: &HtmlDom, id: &str) -> PlanKind {
    plan_node(plan, PlanKey::Field(node(dom, id))).kind.clone()
}

/// The text field and text area are form fields in the display list, with the value the
/// app set and their placeholders, and `TextField`s in the plan. The checkbox and the radio
/// buttons are `Checkbox` and `RadioButton` with their states.
#[test]
fn fr34_settings_inputs_are_fields_with_their_values() {
    let mut dom = laid_out();

    let name = entry(&dom, "name");
    let field = name.input.as_ref().expect("the text input is a form field");
    assert_eq!(field.kind, InputKind::Text);
    assert_eq!(field.value, "Ada Lovelace");
    assert_eq!(field.placeholder.as_deref(), Some("Your name"));
    assert_rect(
        field.content_rect,
        176.0,
        96.0,
        340.0,
        20.0,
        "the name's text box",
    );

    let bio = entry(&dom, "bio");
    let field = bio.input.as_ref().expect("the text area is a form field");
    assert_eq!(field.kind, InputKind::TextArea);
    assert_eq!(
        field.value,
        "Writes programs for engines that do not exist yet."
    );
    assert_rect(
        field.content_rect,
        176.0,
        148.0,
        340.0,
        80.0,
        "the bio's text box",
    );

    assert_eq!(
        entry(&dom, "weekly")
            .input
            .as_ref()
            .map(|field| field.kind.clone()),
        Some(InputKind::Checkbox { checked: true })
    );
    assert_eq!(
        entry(&dom, "density-0")
            .input
            .as_ref()
            .map(|field| field.kind.clone()),
        Some(InputKind::Radio { checked: true })
    );
    assert_eq!(
        entry(&dom, "density-1")
            .input
            .as_ref()
            .map(|field| field.kind.clone()),
        Some(InputKind::Radio { checked: false })
    );

    let plan = dom.plan(&mut LiteralColours).expect("laid out above");
    let PlanKind::TextField(text) = field_kind(&plan, &dom, "name") else {
        panic!("the name is a TextField");
    };
    assert_eq!(text.input, InputKind::Text);
    assert_eq!(text.value, "Ada Lovelace");
    assert_eq!(text.placeholder.as_deref(), Some("Your name"));
    assert!(!text.multiline);

    let PlanKind::TextField(text) = field_kind(&plan, &dom, "bio") else {
        panic!("the bio is a TextField");
    };
    assert_eq!(
        text.value,
        "Writes programs for engines that do not exist yet."
    );
    assert!(text.multiline);

    assert_eq!(
        field_kind(&plan, &dom, "weekly"),
        PlanKind::Checkbox { checked: true }
    );
    assert_eq!(
        field_kind(&plan, &dom, "density-0"),
        PlanKind::RadioButton { selected: true }
    );
    assert_eq!(
        field_kind(&plan, &dom, "density-1"),
        PlanKind::RadioButton { selected: false }
    );
}

/// Each label is beside its control: its right edge 12px before the control, and centred
/// on it (or level with its top, beside the text area).
#[test]
fn fr34_settings_labels_lay_out_beside_their_controls() {
    let dom = laid_out();

    assert_rect(
        entry(&dom, "name-label").rect,
        32.0,
        96.0,
        120.0,
        20.0,
        "name label",
    );
    assert_rect(
        entry(&dom, "name").rect,
        164.0,
        88.0,
        364.0,
        36.0,
        "name field",
    );
    assert_rect(
        entry(&dom, "bio-label").rect,
        32.0,
        140.0,
        120.0,
        20.0,
        "bio label",
    );
    assert_rect(
        entry(&dom, "bio").rect,
        164.0,
        140.0,
        364.0,
        96.0,
        "bio field",
    );
    assert_rect(
        entry(&dom, "theme-label").rect,
        32.0,
        260.0,
        120.0,
        20.0,
        "theme label",
    );
    assert_rect(
        entry(&dom, "theme").rect,
        164.0,
        252.0,
        364.0,
        36.0,
        "theme select",
    );

    let pairs = [
        ("name-label", "name"),
        ("bio-label", "bio"),
        ("theme-label", "theme"),
    ];
    for (label, control) in pairs {
        assert_eq!(
            entry(&dom, label).rect.right() + 12.0,
            entry(&dom, control).rect.x,
            "#{label} is 12px before #{control}"
        );
    }

    // A checkbox or radio button comes first in its label, and the label's text follows
    // after an 8px gap.
    assert_rect(
        entry(&dom, "weekly").rect,
        164.0,
        306.0,
        16.0,
        16.0,
        "the checkbox",
    );
    let weekly_label = dom
        .display_list()
        .unwrap()
        .entries
        .iter()
        .flat_map(|entry| entry.texts.iter())
        .find(|run| run.text == "Send me a weekly summary")
        .expect("the checkbox's label is drawn")
        .clone();
    assert_rect(
        weekly_label.rect,
        188.0,
        304.0,
        168.0,
        20.0,
        "the checkbox's label text",
    );

    assert_rect(
        entry(&dom, "density-0").rect,
        164.0,
        342.0,
        16.0,
        16.0,
        "first radio",
    );
    assert_rect(
        entry(&dom, "density-1").rect,
        164.0,
        370.0,
        16.0,
        16.0,
        "second radio",
    );

    assert_rect(
        entry(&dom, "save").rect,
        164.0,
        404.0,
        116.0,
        36.0,
        "the save button",
    );
}

/// The button's click reaches its handler, which saves and shows that it did. It is a
/// `type="button"` button, so the click does not also submit the form.
#[test]
fn fr34_settings_save_button_reaches_its_handler() {
    let mut dom = laid_out();
    assert!(
        dom.element_by_id("status").is_none(),
        "nothing has been saved yet"
    );

    let save = node(&dom, "save");
    let (x, y) = centre(entry(&dom, "save").rect);
    assert_eq!(dom.click(x, y), Some(save));
    relayout(&mut dom);

    let status = entry(&dom, "status");
    assert_eq!(text_of(status), "Saved");
    // After the 116px button and a 12px gap, centred in the 36px row.
    assert_rect(status.rect, 292.0, 412.0, 35.0, 20.0, "the status line");
}

/// Clicks on the checkbox and on a radio button reach their handlers, and the radio button
/// that was picked draws selected.
#[test]
fn fr34_settings_checkbox_and_radio_clicks_reach_their_handlers() {
    let mut dom = laid_out();

    let weekly = node(&dom, "weekly");
    let (x, y) = centre(entry(&dom, "weekly").rect);
    assert_eq!(dom.click(x, y), Some(weekly));

    let compact = node(&dom, "density-1");
    let (x, y) = centre(entry(&dom, "density-1").rect);
    assert_eq!(dom.click(x, y), Some(compact));
    relayout(&mut dom);

    let plan = dom.plan(&mut LiteralColours).expect("laid out above");
    assert_eq!(
        field_kind(&plan, &dom, "density-1"),
        PlanKind::RadioButton { selected: true }
    );
}

/// After the checkbox is cleared and the other radio button picked, the cleared checkbox
/// and the radio button that lost its selection draw unchecked.
#[test]
fn fr34_settings_unchecked_controls_draw_unchecked() {
    let mut dom = laid_out();

    let (x, y) = centre(entry(&dom, "weekly").rect);
    dom.click(x, y);
    let (x, y) = centre(entry(&dom, "density-1").rect);
    dom.click(x, y);
    relayout(&mut dom);

    assert_eq!(
        entry(&dom, "weekly")
            .input
            .as_ref()
            .map(|field| field.kind.clone()),
        Some(InputKind::Checkbox { checked: false })
    );
    assert_eq!(
        entry(&dom, "density-0")
            .input
            .as_ref()
            .map(|field| field.kind.clone()),
        Some(InputKind::Radio { checked: false })
    );
    let plan = dom.plan(&mut LiteralColours).expect("laid out above");
    assert_eq!(
        field_kind(&plan, &dom, "weekly"),
        PlanKind::Checkbox { checked: false }
    );
    assert_eq!(
        field_kind(&plan, &dom, "density-0"),
        PlanKind::RadioButton { selected: false }
    );
}

/// The theme select is one field listing its three options with the saved one chosen,
/// and a `Dropdown` in the plan. Its options are not drawn as text on the page.
#[test]
fn fr34_settings_theme_select_is_a_dropdown() {
    let mut dom = laid_out();

    let theme = entry(&dom, "theme");
    let field = theme.input.as_ref().expect("the select is a form field");
    assert_eq!(
        field.kind,
        InputKind::Select {
            options: vec![
                "Light".to_string(),
                "Dark".to_string(),
                "Match the system".to_string()
            ],
            selected: Some(2),
        }
    );
    assert_eq!(field.value, "system");
    assert_rect(
        field.content_rect,
        176.0,
        260.0,
        340.0,
        20.0,
        "the select's text box",
    );
    assert!(theme.texts.is_empty());
    let drawn_options = dom
        .display_list()
        .unwrap()
        .entries
        .iter()
        .flat_map(|entry| entry.texts.iter())
        .filter(|run| run.text.contains("Match the system"))
        .count();
    assert_eq!(drawn_options, 0, "the options are the dropdown's to show");

    let plan = dom.plan(&mut LiteralColours).expect("laid out above");
    let PlanKind::Dropdown(dropdown) = field_kind(&plan, &dom, "theme") else {
        panic!("the theme select is a Dropdown");
    };
    assert_eq!(dropdown.options, ["Light", "Dark", "Match the system"]);
    assert_eq!(dropdown.selected, Some(2));
}

/// What the user commits to the fields and the theme they pick reach `oninput` and
/// `onchange`, which keep them as a draft without drawing anything again; pressing Enter
/// in the name field submits the form, whose `onsubmit` saves the draft, and the fields
/// then show what was saved.
#[test]
fn fr34_settings_typing_and_enter_save_the_form() {
    let mut dom = laid_out();
    let name = node(&dom, "name");
    let bio = node(&dom, "bio");
    let theme = node(&dom, "theme");
    let form = node(&dom, "settings");

    assert_eq!(dom.input(name, "Grace Hopper"), Some(name));
    assert_eq!(dom.input(bio, "Finds bugs."), Some(bio));
    assert_eq!(dom.select(theme, 1), Some(theme));
    relayout(&mut dom);
    assert_eq!(
        entry(&dom, "name").input.as_ref().unwrap().value,
        "Ada Lovelace",
        "typing renders nothing: the field's text stays the renderer's"
    );
    assert!(dom.element_by_id("status").is_none());

    // The save button is `type="button"`, so Enter submits the form directly.
    assert_eq!(dom.submit(name), Some(form));
    relayout(&mut dom);

    assert_eq!(text_of(entry(&dom, "status")), "Saved");
    assert_eq!(
        entry(&dom, "name").input.as_ref().unwrap().value,
        "Grace Hopper"
    );
    assert_eq!(
        entry(&dom, "bio").input.as_ref().unwrap().value,
        "Finds bugs."
    );
    let theme_field = entry(&dom, "theme").input.clone().unwrap();
    assert_eq!(theme_field.value, "dark");
    assert!(matches!(
        theme_field.kind,
        InputKind::Select {
            selected: Some(1),
            ..
        }
    ));
}
