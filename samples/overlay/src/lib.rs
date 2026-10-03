//! Things drawn over the page: a dropdown menu under a toolbar button, and a confirmation
//! dialog over a dimmed backdrop. The page below them also has a preview box that clips
//! its banner to rounded corners.
//!
//! The menu is positioned against its button's wrapper (`position: relative`), and the
//! backdrop and dialog are `position: fixed`, measured from the viewport wherever they sit
//! in the page.

use dioxus_compose::html::prelude::*;
use dioxus_hooks::use_signal;
use dioxus_signals::{ReadableExt, WritableExt};

pub const STYLE: &str = r#"
* { box-sizing: border-box; }
html, body { margin: 0; height: 100%; }
body { font-family: system-ui, sans-serif; font-size: 14px; color: #0f172a; background-color: #f8fafc; }

.toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 48px;
    padding: 0 16px;
    background-color: #1e293b;
}
.toolbar button {
    height: 32px;
    padding: 0 12px;
    border: 0;
    border-radius: 6px;
    background-color: #334155;
    color: #f8fafc;
}

.menu-anchor { position: relative; display: flex; }
.menu {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 10;
    display: flex;
    flex-direction: column;
    width: 180px;
    padding: 4px 0;
    border: 1px solid #e2e8f0;
    border-radius: 8px;
    background-color: #ffffff;
    box-shadow: 0 8px 24px rgba(15, 23, 42, 0.2);
}
.menu button {
    height: 32px;
    padding: 0 12px;
    border-radius: 0;
    background-color: #ffffff;
    color: #0f172a;
    text-align: left;
}
.menu button.danger { color: #dc2626; }

.page { padding: 16px; }
.card {
    height: 120px;
    margin-bottom: 16px;
    padding: 16px;
    border: 1px solid #e2e8f0;
    border-radius: 8px;
    background-color: #ffffff;
}
.card h2 { margin: 0 0 8px; font-size: 16px; }
.card p { margin: 0; color: #475569; }
.preview {
    width: 240px;
    height: 80px;
    overflow: hidden;
    border-radius: 8px;
    background-color: #e2e8f0;
}
.preview-banner { width: 400px; height: 160px; background-color: #6366f1; }

.backdrop {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    left: 0;
    z-index: 100;
    background-color: #0f172a;
    opacity: 0.5;
}
.dialog {
    position: fixed;
    top: 50%;
    left: 50%;
    z-index: 101;
    width: 320px;
    padding: 24px;
    transform: translate(-50%, -50%);
    border-radius: 12px;
    background-color: #ffffff;
    box-shadow: 0 16px 48px rgba(15, 23, 42, 0.3);
}
.dialog h2 { margin: 0 0 8px; font-size: 18px; }
.dialog p { margin: 0 0 16px; color: #475569; }
.dialog .actions { display: flex; justify-content: flex-end; gap: 8px; }
.dialog button {
    height: 32px;
    padding: 0 12px;
    border: 1px solid #cbd5e1;
    border-radius: 6px;
    background-color: #ffffff;
    color: #0f172a;
}
.dialog button.danger { border-color: #dc2626; background-color: #dc2626; color: #ffffff; }
"#;

pub fn app() -> Element {
    let mut menu_open = use_signal(|| false);
    let mut dialog_open = use_signal(|| false);
    let mut deleted = use_signal(|| false);
    let show_menu = menu_open.cloned();
    let show_dialog = dialog_open.cloned();
    let status = if deleted.cloned() {
        "The project was deleted."
    } else {
        "Last edited today."
    };

    rsx! {
        header { class: "toolbar",
            div { class: "menu-anchor",
                button {
                    id: "file-button",
                    onclick: move |_| menu_open.set(!show_menu),
                    "File"
                }
                if show_menu {
                    div { id: "menu", class: "menu", role: "menu",
                        button {
                            id: "menu-rename",
                            role: "menuitem",
                            onclick: move |_| menu_open.set(false),
                            "Rename"
                        }
                        button {
                            id: "menu-duplicate",
                            role: "menuitem",
                            onclick: move |_| menu_open.set(false),
                            "Duplicate"
                        }
                        button {
                            id: "menu-delete",
                            class: "danger",
                            role: "menuitem",
                            onclick: move |_| {
                                menu_open.set(false);
                                dialog_open.set(true);
                            },
                            "Delete"
                        }
                    }
                }
            }
            button { id: "share-button", "Share" }
        }
        main { id: "page", class: "page",
            div { id: "card", class: "card",
                h2 { "Project overview" }
                p { id: "status", "{status}" }
            }
            div { id: "preview", class: "preview",
                div { id: "preview-banner", class: "preview-banner" }
            }
        }
        if show_dialog {
            div { id: "backdrop", class: "backdrop", onclick: move |_| dialog_open.set(false) }
            div { id: "dialog", class: "dialog", role: "dialog",
                h2 { id: "dialog-title", "Delete project?" }
                p { "This cannot be undone." }
                div { class: "actions",
                    button { id: "cancel", onclick: move |_| dialog_open.set(false), "Cancel" }
                    button {
                        id: "confirm",
                        class: "danger",
                        onclick: move |_| {
                            deleted.set(true);
                            dialog_open.set(false);
                        },
                        "Delete"
                    }
                }
            }
        }
    }
}
