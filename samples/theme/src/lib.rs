//! A page whose colours come from CSS custom properties. The light set is the default and
//! `prefers-color-scheme: dark` swaps in the dark one; a button overrides the system's
//! choice by putting `light` or `dark` on the app's root element.
//!
//! Every colour on the page is a `var()`, so changing the theme changes colours and
//! nothing else.

use dioxus_compose::html::prelude::*;
use dioxus_hooks::use_signal;
use dioxus_signals::{ReadableExt, WritableExt};

pub const STYLE: &str = r#"
:root, .app.light {
    --bg: #ffffff;
    --surface: #f3f4f6;
    --text: #111827;
    --muted: #6b7280;
    --border: #d1d5db;
    --accent: #2563eb;
    --on-accent: #ffffff;
}

@media (prefers-color-scheme: dark) {
    :root {
        --bg: #111827;
        --surface: #1f2937;
        --text: #f9fafb;
        --muted: #9ca3af;
        --border: #374151;
        --accent: #60a5fa;
        --on-accent: #111827;
    }
}

.app.dark {
    --bg: #111827;
    --surface: #1f2937;
    --text: #f9fafb;
    --muted: #9ca3af;
    --border: #374151;
    --accent: #60a5fa;
    --on-accent: #111827;
}

* { box-sizing: border-box; }
html, body { margin: 0; height: 100%; }
body { font-family: system-ui, sans-serif; font-size: 14px; }

.app { height: 100%; padding: 24px; background-color: var(--bg); color: var(--text); }

.header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 40px;
    margin-bottom: 16px;
}
.header h1 { margin: 0; font-size: 20px; }

.toggle {
    width: 120px;
    height: 32px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    background-color: var(--accent);
    color: var(--on-accent);
}

.card {
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background-color: var(--surface);
}
.card p { margin: 0; }
.card .muted { margin-top: 8px; color: var(--muted); }
"#;

/// A theme chosen with the button, overriding the system's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

pub fn app() -> Element {
    // `None` follows the system through the media query.
    let mut theme = use_signal(|| None::<Theme>);
    let class = match theme.cloned() {
        None => "app",
        Some(Theme::Light) => "app light",
        Some(Theme::Dark) => "app dark",
    };

    rsx! {
        div { id: "app", class: "{class}",
            div { class: "header",
                h1 { id: "title", "Appearance" }
                button {
                    id: "theme-toggle",
                    class: "toggle",
                    onclick: move |_| {
                        let next = match theme.cloned() {
                            Some(Theme::Dark) => Theme::Light,
                            _ => Theme::Dark,
                        };
                        theme.set(Some(next));
                    },
                    "Toggle theme"
                }
            }
            div { id: "card", class: "card",
                p { id: "body-text", "Colours follow the theme." }
                p { id: "muted-text", class: "muted", "Set by a class or the system." }
            }
        }
    }
}
