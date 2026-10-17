//! Landing: a responsive product page. A hero painted with a gradient, a grid of feature
//! cards, three pricing plans with the middle one highlighted, and a footer of link
//! columns, all inside a container centred with `margin: 0 auto`.
//!
//! The layout changes with the width of the window through `@media` queries: below 600px
//! (a phone) every grid is one column and the menu stacks, up to 1024px (a tablet) the
//! features take two columns, and above that (a desktop) three. Font sizes step down with
//! the width. A toggle switches the prices between monthly and yearly billing.

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
    font-size: 16px;
    line-height: 24px;
    color: #1f2328;
    background: #ffffff;
}

.container {
    max-width: 960px;
    margin: 0 auto;
    padding: 0 24px;
}

/* The top bar */

.topbar {
    border-bottom: 1px solid #d0d7de;
}

.nav {
    display: flex;
    justify-content: space-between;
    align-items: center;
    height: 64px;
}

.brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 20px;
    line-height: 28px;
    font-weight: 700;
}

.logo {
    width: 28px;
    height: 28px;
}

.menu {
    display: flex;
    gap: 24px;
}

.menu a {
    color: #1f2328;
    text-decoration: none;
}

/* The hero */

.hero {
    background-image: linear-gradient(to right, #4f46e5, #06b6d4);
    color: #ffffff;
}

.hero-inner {
    padding: 96px 24px;
}

.hero h1 {
    margin: 0 0 16px;
    font-size: 48px;
    line-height: 56px;
}

.hero-lead {
    margin: 0 0 32px;
}

.cta {
    display: flex;
    gap: 16px;
}

.button {
    width: 140px;
    height: 48px;
    padding: 0 24px;
    border: 2px solid #ffffff;
    border-radius: 8px;
    font: inherit;
    cursor: pointer;
}

.button.primary {
    background: #ffffff;
    color: #4f46e5;
}

.button.secondary {
    background: transparent;
    color: #ffffff;
}

/* Sections */

.section {
    padding: 64px 0;
}

.section.tinted {
    background: #f6f8fa;
}

.section h2 {
    margin: 0 0 24px;
    font-size: 32px;
    line-height: 40px;
}

/* Features */

.features {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 24px;
}

.card {
    padding: 24px;
    background: #ffffff;
    border: 1px solid #d0d7de;
    border-radius: 12px;
    box-shadow: 0 1px 3px rgba(31, 35, 40, 0.2);
}

.card h3 {
    margin: 0 0 8px;
    font-size: 20px;
    line-height: 28px;
}

.card p {
    margin: 0;
}

/* Pricing */

.toggle {
    display: flex;
    gap: 8px;
    margin-bottom: 24px;
}

.toggle-button {
    width: 96px;
    height: 40px;
    border: 1px solid #d0d7de;
    border-radius: 8px;
    background: #ffffff;
    color: #1f2328;
    font: inherit;
    cursor: pointer;
}

.toggle-button.on {
    background: #4f46e5;
    border-color: #4f46e5;
    color: #ffffff;
}

.plans {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 24px;
}

.plan {
    padding: 24px;
    background: #ffffff;
    border: 1px solid #d0d7de;
    border-radius: 12px;
    box-shadow: 0 1px 3px rgba(31, 35, 40, 0.2);
}

.plan.featured {
    background: #eef2ff;
    border-color: #4f46e5;
    box-shadow: 0 8px 24px rgba(79, 70, 229, 0.2);
}

.plan h3 {
    margin: 0;
    font-size: 20px;
    line-height: 28px;
}

.price {
    margin: 0;
    font-size: 36px;
    line-height: 44px;
    font-weight: 700;
}

.period {
    margin: 0 0 16px;
    font-size: 14px;
    line-height: 20px;
    color: #57606a;
}

.plan ul {
    margin: 0;
    padding: 0;
    list-style: none;
}

/* The footer */

.footer {
    padding: 48px 0;
    background: #1f2328;
    color: #d0d7de;
}

.footer-cols {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 24px;
}

.footer-col h4 {
    margin: 0 0 8px;
    font-size: 16px;
    line-height: 24px;
    color: #ffffff;
}

.footer-col a {
    display: block;
    height: 24px;
    color: #d0d7de;
    text-decoration: none;
}

.legal {
    margin: 32px 0 0;
    font-size: 14px;
    line-height: 20px;
}

/* Tablet: up to 1024px wide */

@media (max-width: 1024px) {
    .hero-inner {
        padding-top: 64px;
        padding-bottom: 64px;
    }

    .hero h1 {
        font-size: 40px;
        line-height: 48px;
    }

    .section {
        padding: 48px 0;
    }

    .section h2 {
        font-size: 28px;
        line-height: 36px;
    }

    .features {
        grid-template-columns: repeat(2, 1fr);
    }

    .plans, .footer-cols {
        gap: 16px;
    }
}

/* Phone: under 600px wide */

@media (max-width: 599px) {
    .container {
        padding-left: 16px;
        padding-right: 16px;
    }

    .nav {
        flex-direction: column;
        align-items: flex-start;
        height: auto;
        padding: 12px 0;
    }

    .menu {
        flex-direction: column;
        align-self: stretch;
        gap: 0;
    }

    .menu a {
        padding: 8px 0;
    }

    .hero-inner {
        padding-top: 48px;
        padding-bottom: 48px;
    }

    .hero h1 {
        font-size: 36px;
        line-height: 44px;
    }

    .hero-lead {
        margin-bottom: 24px;
    }

    .cta {
        flex-direction: column;
        gap: 12px;
    }

    .button {
        width: 100%;
    }

    .section h2 {
        font-size: 24px;
        line-height: 32px;
    }

    .features, .plans {
        grid-template-columns: 1fr;
    }

    .footer-cols {
        grid-template-columns: 1fr;
        gap: 24px;
    }
}
"#;

/// The links in the top bar.
pub const MENU: [(&str, &str); 3] = [
    ("Features", "#features"),
    ("Pricing", "#pricing"),
    ("Contact", "#contact"),
];

/// One feature card.
#[derive(Clone, Copy, PartialEq)]
pub struct Feature {
    pub title: &'static str,
    pub text: &'static str,
}

pub const FEATURES: [Feature; 6] = [
    Feature {
        title: "Responsive",
        text: "Columns follow the width.",
    },
    Feature {
        title: "Themeable",
        text: "Colours are CSS variables.",
    },
    Feature {
        title: "Accessible",
        text: "Real text and real focus.",
    },
    Feature {
        title: "Native text",
        text: "Compose draws every glyph.",
    },
    Feature {
        title: "Image aware",
        text: "Pictures come by URL.",
    },
    Feature {
        title: "Small",
        text: "No webview, no bundled JVM.",
    },
];

/// One pricing plan.
#[derive(Clone, Copy, PartialEq)]
pub struct Plan {
    pub name: &'static str,
    /// Dollars per month when billed monthly.
    pub monthly: u32,
    /// Dollars per month when billed yearly.
    pub yearly: u32,
    pub features: [&'static str; 3],
    /// Drawn with an accent border and shadow.
    pub featured: bool,
}

pub const PLANS: [Plan; 3] = [
    Plan {
        name: "Starter",
        monthly: 12,
        yearly: 10,
        features: ["1 project", "Community support", "Basic themes"],
        featured: false,
    },
    Plan {
        name: "Team",
        monthly: 29,
        yearly: 24,
        features: ["10 projects", "Email support", "All themes"],
        featured: true,
    },
    Plan {
        name: "Business",
        monthly: 79,
        yearly: 66,
        features: ["Unlimited projects", "Priority support", "Custom themes"],
        featured: false,
    },
];

/// The columns of links in the footer.
pub const FOOTER: [(&str, [&str; 3]); 3] = [
    ("Product", ["Features", "Pricing", "Changelog"]),
    ("Company", ["About", "Blog", "Careers"]),
    ("Help", ["Docs", "Contact", "Status"]),
];

pub fn app() -> Element {
    let mut yearly = use_signal(|| false);

    rsx! {
        header { id: "topbar", class: "topbar",
            div { class: "container",
                nav { id: "nav", class: "nav",
                    div { id: "brand", class: "brand",
                        img { id: "logo", class: "logo", src: "images/logo.png", alt: "Lumen" }
                        span { id: "brand-name", "Lumen" }
                    }
                    div { id: "menu", class: "menu",
                        for (index, (label, href)) in MENU.into_iter().enumerate() {
                            a { key: "{label}", id: "menu-{index}", href: href, "{label}" }
                        }
                    }
                }
            }
        }
        section { id: "hero", class: "hero",
            div { id: "hero-inner", class: "container hero-inner",
                h1 { id: "hero-title", "Build pages that adapt" }
                p { id: "hero-lead", class: "hero-lead",
                    "Layouts that rearrange themselves for any screen."
                }
                div { id: "hero-cta", class: "cta",
                    button { id: "cta-primary", class: "button primary", r#type: "button",
                        "Get started"
                    }
                    button { id: "cta-secondary", class: "button secondary", r#type: "button",
                        "See pricing"
                    }
                }
            }
        }
        section { id: "features", class: "section",
            div { class: "container",
                h2 { id: "features-title", "Features" }
                div { id: "feature-grid", class: "features",
                    for (index, feature) in FEATURES.into_iter().enumerate() {
                        div { key: "{feature.title}", id: "feature-{index}", class: "card",
                            h3 { "{feature.title}" }
                            p { "{feature.text}" }
                        }
                    }
                }
            }
        }
        section { id: "pricing", class: "section tinted",
            div { class: "container",
                h2 { id: "pricing-title", "Pricing" }
                div { id: "toggle", class: "toggle",
                    button {
                        id: "billing-monthly",
                        class: if yearly() { "toggle-button" } else { "toggle-button on" },
                        r#type: "button",
                        onclick: move |_| yearly.set(false),
                        "Monthly"
                    }
                    button {
                        id: "billing-yearly",
                        class: if yearly() { "toggle-button on" } else { "toggle-button" },
                        r#type: "button",
                        onclick: move |_| yearly.set(true),
                        "Yearly"
                    }
                }
                div { id: "plans", class: "plans",
                    for (index, plan) in PLANS.into_iter().enumerate() {
                        div {
                            key: "{plan.name}",
                            id: "plan-{index}",
                            class: if plan.featured { "plan featured" } else { "plan" },
                            h3 { "{plan.name}" }
                            p { id: "price-{index}", class: "price",
                                {price_label(plan, yearly())}
                            }
                            p { id: "period-{index}", class: "period",
                                {period_label(yearly())}
                            }
                            ul {
                                for feature in plan.features {
                                    li { key: "{feature}", "{feature}" }
                                }
                            }
                        }
                    }
                }
            }
        }
        footer { id: "footer", class: "footer",
            div { class: "container",
                div { id: "footer-cols", class: "footer-cols",
                    for (index, (title, links)) in FOOTER.into_iter().enumerate() {
                        div { key: "{title}", id: "footer-col-{index}", class: "footer-col",
                            h4 { "{title}" }
                            for link in links {
                                a { key: "{link}", href: "#", "{link}" }
                            }
                        }
                    }
                }
                p { id: "legal", class: "legal", "Copyright 2026 Lumen" }
            }
        }
    }
}

fn price_label(plan: Plan, yearly: bool) -> String {
    format!("${}", if yearly { plan.yearly } else { plan.monthly })
}

fn period_label(yearly: bool) -> &'static str {
    if yearly {
        "per month, billed yearly"
    } else {
        "per month"
    }
}
