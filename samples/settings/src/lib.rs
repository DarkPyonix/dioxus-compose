//! Settings: a form with a text field, a text area, a select, a checkbox, a radio group and
//! a save button.
//!
//! The text fields are uncontrolled: they start from the saved values, the renderer owns
//! their text while the user types, and `oninput` keeps what was typed until it is saved.

use dioxus_compose::html::prelude::*;
use dioxus_hooks::use_signal;
use dioxus_signals::WritableExt;

pub const STYLE: &str = r#"
*, *::before, *::after {
    box-sizing: border-box;
}

body {
    margin: 0;
    font-family: system-ui, sans-serif;
    font-size: 14px;
    line-height: 20px;
    color: #1f2328;
    background: #ffffff;
}

.settings {
    max-width: 560px;
    padding: 32px;
}

h1 {
    margin: 0 0 24px;
    font-size: 24px;
    line-height: 32px;
}

.field {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 16px;
}

.field.tall {
    align-items: flex-start;
}

.field > label,
.group-label {
    width: 120px;
    flex-shrink: 0;
    margin: 0;
    color: #57606a;
}

.control {
    flex: 1;
    min-width: 0;
    height: 36px;
    padding: 7px 11px;
    border: 1px solid #d0d7de;
    border-radius: 6px;
    background: #ffffff;
    color: #1f2328;
    font: inherit;
}

textarea.control {
    height: 96px;
}

.group {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    margin-bottom: 16px;
}

.options {
    display: flex;
    flex-direction: column;
    gap: 8px;
}

.check {
    display: flex;
    align-items: center;
    gap: 8px;
}

.check input {
    width: 16px;
    height: 16px;
    margin: 0;
}

.actions {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-left: 132px;
}

.save {
    height: 36px;
    padding: 0 16px;
    border: 0;
    border-radius: 6px;
    background: #1f883d;
    color: #ffffff;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
}

.status {
    margin: 0;
    color: #1a7f37;
}
"#;

/// The densities the radio group offers.
pub const DENSITIES: [&str; 2] = ["Comfortable", "Compact"];

/// What the text fields edit.
#[derive(Clone, PartialEq)]
pub struct Profile {
    pub name: String,
    pub bio: String,
    pub theme: String,
}

impl Profile {
    /// The profile the form opens with.
    pub fn initial() -> Self {
        Self {
            name: "Ada Lovelace".to_string(),
            bio: "Writes programs for engines that do not exist yet.".to_string(),
            theme: "system".to_string(),
        }
    }
}

pub fn app() -> Element {
    // What was last saved. The fields start from it.
    let mut saved = use_signal(Profile::initial);
    // What has been typed since. Nothing renders from it, so typing does not render the
    // form again and never writes a field's text back while it is being edited.
    let mut draft = use_signal(Profile::initial);
    let mut weekly = use_signal(|| true);
    let mut density = use_signal(|| DENSITIES[0]);
    let mut saves = use_signal(|| 0u32);

    let profile = saved();

    rsx! {
        form {
            id: "settings",
            class: "settings",
            // Pressing Enter in a field submits the form.
            onsubmit: move |event| {
                event.prevent_default();
                saved.set(draft());
                saves += 1;
            },
            h1 { "Settings" }

            div { class: "field",
                label { id: "name-label", r#for: "name", "Display name" }
                input {
                    id: "name",
                    class: "control",
                    r#type: "text",
                    value: "{profile.name}",
                    placeholder: "Your name",
                    oninput: move |event| {
                        draft.write().name = event.value();
                    },
                }
            }

            div { class: "field tall",
                label { id: "bio-label", r#for: "bio", "Bio" }
                textarea {
                    id: "bio",
                    class: "control",
                    value: "{profile.bio}",
                    placeholder: "A line about you",
                    oninput: move |event| {
                        draft.write().bio = event.value();
                    },
                }
            }

            div { class: "field",
                label { id: "theme-label", r#for: "theme", "Theme" }
                select {
                    id: "theme",
                    class: "control",
                    value: "{profile.theme}",
                    onchange: move |event| {
                        draft.write().theme = event.value();
                    },
                    option { value: "light", "Light" }
                    option { value: "dark", "Dark" }
                    option { value: "system", "Match the system" }
                }
            }

            div { class: "group",
                p { class: "group-label", "Email" }
                div { class: "options",
                    label { class: "check",
                        input {
                            id: "weekly",
                            r#type: "checkbox",
                            checked: weekly(),
                            onclick: move |_| weekly.toggle(),
                        }
                        "Send me a weekly summary"
                    }
                }
            }

            div { class: "group",
                p { class: "group-label", "Density" }
                div { class: "options",
                    for (index, choice) in DENSITIES.into_iter().enumerate() {
                        label { key: "{choice}", class: "check",
                            input {
                                id: "density-{index}",
                                r#type: "radio",
                                name: "density",
                                value: "{choice}",
                                checked: density() == choice,
                                onclick: move |_| density.set(choice),
                            }
                            "{choice}"
                        }
                    }
                }
            }

            div { class: "actions",
                button {
                    id: "save",
                    class: "save",
                    r#type: "button",
                    onclick: move |_| {
                        saved.set(draft());
                        saves += 1;
                    },
                    "Save changes"
                }
                if saves() > 0 {
                    p { id: "status", class: "status", "Saved" }
                }
            }
        }
    }
}
