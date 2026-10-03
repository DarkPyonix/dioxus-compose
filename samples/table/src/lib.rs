//! Table: a list of invoices in an HTML table, wider than the page on a narrow screen, so
//! it scrolls sideways inside its wrapper. Clicking the amount header sorts by amount.

use dioxus_compose::html::prelude::*;
use dioxus_hooks::use_signal;
use dioxus_signals::WritableExt;

pub const STYLE: &str = r#"
body {
    margin: 0;
    font-family: system-ui, sans-serif;
    font-size: 14px;
    line-height: 20px;
    color: #1f2328;
    background: #ffffff;
}

.page {
    padding: 24px;
}

h1 {
    margin: 0 0 16px;
    font-size: 24px;
    line-height: 32px;
}

.table-wrap {
    overflow-x: auto;
    overflow-y: hidden;
    border-radius: 8px;
    background: #ffffff;
}

.invoices {
    width: 100%;
    min-width: 640px;
    border-collapse: collapse;
    border-spacing: 0;
}

th, td {
    height: 20px;
    padding: 8px 12px;
    border-bottom: 1px solid #d0d7de;
    text-align: left;
    vertical-align: middle;
}

th {
    background: #f6f8fa;
    color: #57606a;
    font-weight: 600;
}

th.sortable {
    cursor: pointer;
}

.num {
    text-align: right;
}

.col-number {
    width: 96px;
}

.col-client {
    width: 212px;
}

.col-due {
    width: 116px;
}

.col-amount {
    width: 120px;
}
"#;

/// One row of the table.
#[derive(Clone, Copy, PartialEq)]
pub struct Invoice {
    pub number: &'static str,
    pub client: &'static str,
    pub due: &'static str,
    /// In cents.
    pub amount: u64,
}

pub const INVOICES: [Invoice; 4] = [
    Invoice {
        number: "INV-1001",
        client: "Analytical Engines Ltd",
        due: "2026-10-12",
        amount: 125000,
    },
    Invoice {
        number: "INV-1002",
        client: "Difference & Sons",
        due: "2026-10-19",
        amount: 9840,
    },
    Invoice {
        number: "INV-1003",
        client: "Jacquard Looms",
        due: "2026-10-26",
        amount: 431075,
    },
    Invoice {
        number: "INV-1004",
        client: "Babbage Supplies",
        due: "2026-11-02",
        amount: 64000,
    },
];

/// Cents as dollars: 125000 is "$1,250.00".
pub fn format_amount(cents: u64) -> String {
    let dollars = (cents / 100).to_string();
    let mut grouped = String::new();
    for (index, digit) in dollars.chars().enumerate() {
        if index > 0 && (dollars.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("${grouped}.{:02}", cents % 100)
}

pub fn app() -> Element {
    let mut largest_first = use_signal(|| false);

    let mut rows = INVOICES.to_vec();
    if largest_first() {
        rows.sort_by_key(|row| std::cmp::Reverse(row.amount));
    }

    rsx! {
        div { class: "page",
            h1 { "Invoices" }
            div { id: "wrap", class: "table-wrap",
                table { id: "invoices", class: "invoices",
                    thead {
                        tr {
                            th { class: "col-number", "Invoice" }
                            th { class: "col-client", "Client" }
                            th { class: "col-due", "Due" }
                            th {
                                id: "amount-header",
                                class: "col-amount num sortable",
                                onclick: move |_| largest_first.toggle(),
                                if largest_first() { "Amount ↓" } else { "Amount" }
                            }
                        }
                    }
                    tbody {
                        for invoice in rows {
                            tr { key: "{invoice.number}",
                                td { "{invoice.number}" }
                                td { "{invoice.client}" }
                                td { "{invoice.due}" }
                                td { class: "num", {format_amount(invoice.amount)} }
                            }
                        }
                    }
                }
            }
        }
    }
}
