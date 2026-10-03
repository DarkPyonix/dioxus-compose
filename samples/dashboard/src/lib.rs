//! Dashboard: a top bar, a sidebar of sections and a wrapping row of metric cards, all
//! flexbox.

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
    background: #f6f8fa;
}

.app {
    display: flex;
    flex-direction: column;
    height: 100vh;
}

.topbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-shrink: 0;
    height: 56px;
    padding: 0 24px;
    background: #24292f;
    color: #ffffff;
}

.brand {
    font-size: 18px;
    line-height: 28px;
    font-weight: 600;
}

.refresh {
    height: 32px;
    padding: 0 12px;
    border: 1px solid #57606a;
    border-radius: 6px;
    background: transparent;
    color: #ffffff;
    font: inherit;
    cursor: pointer;
}

.body {
    display: flex;
    flex: 1;
    align-items: stretch;
    min-height: 0;
}

.sidebar {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 200px;
    flex-shrink: 0;
    padding: 16px;
    background: #ffffff;
    border-right: 1px solid #d0d7de;
}

.nav-item {
    display: block;
    padding: 8px 12px;
    border-radius: 6px;
    color: #1f2328;
    text-decoration: none;
}

.nav-item.active {
    background: #ddf4ff;
    color: #0969da;
}

.content {
    flex: 1;
    padding: 24px;
}

.page-title {
    margin: 0 0 16px;
    font-size: 24px;
    line-height: 32px;
}

.cards {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    justify-content: flex-start;
    align-items: flex-start;
}

.card {
    width: 200px;
    height: 120px;
    padding: 16px;
    background: #ffffff;
    border: 1px solid #d0d7de;
    border-radius: 12px;
    box-shadow: 0 1px 3px rgba(31, 35, 40, 0.2);
}

.card.alert {
    border-left: 4px solid #cf222e;
}

.card.stale {
    opacity: 0.6;
}

.card-label {
    margin: 0 0 8px;
    color: #57606a;
}

.card-value {
    margin: 0;
    font-size: 28px;
    line-height: 36px;
    font-weight: 600;
}

.card-note {
    margin: 4px 0 0;
    font-size: 12px;
    line-height: 16px;
    color: #57606a;
}
"#;

/// The sections the sidebar switches between.
pub const SECTIONS: [&str; 4] = ["Overview", "Orders", "Customers", "Reports"];

/// One card on the overview.
#[derive(Clone, Copy, PartialEq)]
pub struct Metric {
    pub id: &'static str,
    pub label: &'static str,
    pub value: &'static str,
    pub note: &'static str,
    /// Needs attention: drawn with a red edge.
    pub alert: bool,
    /// Comes from a check that has not run lately, and is faded until the next refresh.
    pub checked_lately: bool,
}

pub const METRICS: [Metric; 5] = [
    Metric {
        id: "revenue",
        label: "Revenue",
        value: "$48,210",
        note: "+12% this week",
        alert: false,
        checked_lately: true,
    },
    Metric {
        id: "orders",
        label: "Orders",
        value: "1,284",
        note: "+3% this week",
        alert: false,
        checked_lately: true,
    },
    Metric {
        id: "refunds",
        label: "Refunds",
        value: "37",
        note: "Above the usual 20",
        alert: true,
        checked_lately: true,
    },
    Metric {
        id: "visitors",
        label: "Visitors",
        value: "19,402",
        note: "+8% this week",
        alert: false,
        checked_lately: true,
    },
    Metric {
        id: "uptime",
        label: "Uptime",
        value: "99.2%",
        note: "Checked 2 hours ago",
        alert: false,
        checked_lately: false,
    },
];

fn card_class(metric: &Metric, refreshed: bool) -> &'static str {
    match (metric.alert, metric.checked_lately || refreshed) {
        (true, true) => "card alert",
        (true, false) => "card alert stale",
        (false, true) => "card",
        (false, false) => "card stale",
    }
}

pub fn app() -> Element {
    let mut section = use_signal(|| SECTIONS[0]);
    let mut refreshed = use_signal(|| false);

    rsx! {
        div { class: "app",
            header { class: "topbar",
                span { id: "brand", class: "brand", "Acme Analytics" }
                button {
                    id: "refresh",
                    class: "refresh",
                    r#type: "button",
                    onclick: move |_| refreshed.set(true),
                    "Refresh"
                }
            }
            div { class: "body",
                nav { id: "sidebar", class: "sidebar",
                    for (index, name) in SECTIONS.into_iter().enumerate() {
                        a {
                            key: "{name}",
                            id: "nav-{index}",
                            class: if section() == name { "nav-item active" } else { "nav-item" },
                            href: "#",
                            onclick: move |event| {
                                event.prevent_default();
                                section.set(name);
                            },
                            "{name}"
                        }
                    }
                }
                main { class: "content",
                    h1 { id: "page-title", class: "page-title", "{section}" }
                    div { id: "cards", class: "cards",
                        for metric in METRICS {
                            div {
                                key: "{metric.id}",
                                id: "card-{metric.id}",
                                class: card_class(&metric, refreshed()),
                                p { class: "card-label", "{metric.label}" }
                                p { class: "card-value", "{metric.value}" }
                                p { class: "card-note", "{metric.note}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
