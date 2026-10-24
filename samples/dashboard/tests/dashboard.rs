//! The dashboard example: flexbox rows, columns and a wrapping grid of cards.
//!
//! Every number follows from `dashboard::STYLE` (all boxes `border-box`) and the support
//! measurer (an advance of half the font size per character, the CSS line height per line).
//!
//! - The top bar is 56px tall. The rest of the 800px-tall viewport, 744px, is the body row.
//! - The sidebar is a fixed 200px column stretched to the body's height. Inside its 16px
//!   padding and 1px right border, links are 200 - 32 - 1 = 167px wide and 36px tall (a
//!   20px line and 8px padding above and below), 4px apart: at y = 72, 112, 152, 192.
//! - The content area takes the rest of the width. Its 24px padding puts the heading (one
//!   32px line) at (224, 80) and, after a 16px margin, the cards at y = 128, in a row
//!   `viewport width - 200 - 48` wide.
//! - Cards are 200 by 120 with 16px gaps. In a 1000px viewport the row is 752px wide and
//!   holds three cards (3 x 200 + 2 x 16 = 632; four would need 848). In a 700px viewport
//!   it is 452px wide and holds two (416; three would need 632).

use dioxus_compose::html::{
    BorderSide, HtmlDom, LiteralColours, ModifierSlot, PlanKey, PlanModifier, Rgba, Sides,
};
use sample_html_dashboard as dashboard;
use sample_html_support::{
    Measurer, assert_close, assert_rect, centre, config, entry, node, plan_node, text_of,
};

fn laid_out(width: f32) -> HtmlDom {
    let mut dom = HtmlDom::with_config(dashboard::app, config(dashboard::STYLE, Measurer::new()));
    dom.layout(width, 800.0, 1.0);
    dom
}

const CARD_IDS: [&str; 5] = [
    "card-revenue",
    "card-orders",
    "card-refunds",
    "card-visitors",
    "card-uptime",
];

const WHITE: Rgba = Rgba::new(255, 255, 255, 255);
const CARD_BORDER: Rgba = Rgba::new(208, 215, 222, 255);
const ALERT_RED: Rgba = Rgba::new(207, 34, 46, 255);
/// rgba(31, 35, 40, 0.2): 0.2 x 255 = 51.
const CARD_SHADOW: Rgba = Rgba::new(31, 35, 40, 51);

/// The top bar spreads its two items to the edges and centres them vertically: the brand
/// (one 28px line) 14px down at the left padding, the button (32px) 12px down, ending at
/// the right padding.
#[test]
fn fr34_dashboard_header_and_sidebar_are_flex_rows_and_columns() {
    let dom = laid_out(1000.0);

    let brand = entry(&dom, "brand").rect;
    assert_eq!(
        (brand.x, brand.y, brand.height),
        (24.0, 14.0, 28.0),
        "{brand:?}"
    );
    let refresh = entry(&dom, "refresh").rect;
    assert_eq!(refresh.right(), 1000.0 - 24.0, "{refresh:?}");
    assert_eq!((refresh.y, refresh.height), (12.0, 32.0), "{refresh:?}");

    assert_rect(
        entry(&dom, "sidebar").rect,
        0.0,
        56.0,
        200.0,
        744.0,
        "sidebar",
    );
    for (index, y) in [72.0, 112.0, 152.0, 192.0].into_iter().enumerate() {
        assert_rect(
            entry(&dom, &format!("nav-{index}")).rect,
            16.0,
            y,
            167.0,
            36.0,
            &format!("link {index}"),
        );
    }

    assert_rect(
        entry(&dom, "page-title").rect,
        224.0,
        80.0,
        752.0,
        32.0,
        "heading",
    );
    assert_rect(
        entry(&dom, "cards").rect,
        224.0,
        128.0,
        752.0,
        256.0,
        "card row",
    );
}

/// Three cards to a row at 1000px: x = 224, 440, 656; the next row 120 + 16 = 136px down.
#[test]
fn fr34_dashboard_cards_wrap_three_to_a_row_at_1000px() {
    let dom = laid_out(1000.0);
    let expected = [
        (224.0, 128.0),
        (440.0, 128.0),
        (656.0, 128.0),
        (224.0, 264.0),
        (440.0, 264.0),
    ];
    for (id, (x, y)) in CARD_IDS.into_iter().zip(expected) {
        assert_rect(entry(&dom, id).rect, x, y, 200.0, 120.0, id);
    }
}

/// Narrower, the third card no longer fits and starts the second row: two to a row, the
/// fifth card alone on the third.
#[test]
fn fr34_dashboard_cards_wrap_to_the_next_row_when_narrower() {
    let mut dom = laid_out(1000.0);
    dom.layout(700.0, 800.0, 1.0);
    let expected = [
        (224.0, 128.0),
        (440.0, 128.0),
        (224.0, 264.0),
        (440.0, 264.0),
        (224.0, 400.0),
    ];
    for (id, (x, y)) in CARD_IDS.into_iter().zip(expected) {
        assert_rect(entry(&dom, id).rect, x, y, 200.0, 120.0, id);
    }
    assert_rect(
        entry(&dom, "cards").rect,
        224.0,
        128.0,
        452.0,
        392.0,
        "card row",
    );
}

/// Border, corner radius, shadow and opacity as CSS gives them, in the display list and
/// as modifiers of the card's plan node.
#[test]
fn fr34_dashboard_card_decorations_reach_the_display_list_and_the_plan() {
    let mut dom = laid_out(1000.0);

    let revenue = entry(&dom, "card-revenue");
    assert_eq!(revenue.background, Some(WHITE));
    let border = revenue.border.expect("a card has a border");
    assert_eq!(
        (
            border.widths.top,
            border.widths.right,
            border.widths.bottom,
            border.widths.left
        ),
        (1.0, 1.0, 1.0, 1.0)
    );
    assert_eq!(border.colors.left, CARD_BORDER);
    let radii = revenue.radii.expect("a card has rounded corners");
    for corner in [
        radii.top_left,
        radii.top_right,
        radii.bottom_right,
        radii.bottom_left,
    ] {
        assert_eq!((corner.x, corner.y), (12.0, 12.0));
    }
    assert_eq!(revenue.shadows.len(), 1);
    let shadow = revenue.shadows[0];
    assert_eq!(
        (
            shadow.offset_x,
            shadow.offset_y,
            shadow.blur,
            shadow.spread,
            shadow.inset
        ),
        (0.0, 1.0, 3.0, 0.0, false)
    );
    assert_eq!(shadow.color, CARD_SHADOW);
    assert_eq!(revenue.opacity, 1.0);

    let refunds = entry(&dom, "card-refunds");
    let border = refunds.border.expect("the alert card has a border");
    assert_eq!((border.widths.left, border.widths.top), (4.0, 1.0));
    assert_eq!(border.colors.left, ALERT_RED);
    assert_eq!(border.colors.top, CARD_BORDER);

    assert_close(
        entry(&dom, "card-uptime").opacity,
        0.6,
        "the stale card's opacity",
    );

    let plan = dom.plan(&mut LiteralColours).expect("laid out above");

    let card = plan_node(&plan, PlanKey::Node(node(&dom, "card-revenue")));
    assert_eq!(
        card.modifier(ModifierSlot::Shape),
        Some(&PlanModifier::Shape { radius: 12.0 })
    );
    assert_eq!(
        card.modifier(ModifierSlot::Shadow(0)),
        Some(&PlanModifier::Shadow {
            x: 0.0,
            y: 1.0,
            blur: 3.0,
            spread: 0.0,
            color: CARD_SHADOW
        })
    );
    assert_eq!(
        card.modifier(ModifierSlot::Background),
        Some(&PlanModifier::Background(WHITE))
    );
    assert_eq!(
        card.modifier(ModifierSlot::Border),
        Some(&PlanModifier::Border {
            width: 1.0,
            color: CARD_BORDER
        })
    );
    assert_eq!(card.modifier(ModifierSlot::Alpha), None);

    // One side differs, so the alert card's border is drawn side by side.
    let alert = plan_node(&plan, PlanKey::Node(node(&dom, "card-refunds")));
    assert_eq!(alert.modifier(ModifierSlot::Border), None);
    let side = |width, color| BorderSide { width, color };
    assert_eq!(
        alert.modifier(ModifierSlot::BorderEach),
        Some(&PlanModifier::BorderEach(Sides {
            top: side(1.0, CARD_BORDER),
            right: side(1.0, CARD_BORDER),
            bottom: side(1.0, CARD_BORDER),
            left: side(4.0, ALERT_RED),
        }))
    );
    assert_eq!(
        alert.modifier(ModifierSlot::Shape),
        Some(&PlanModifier::Shape { radius: 12.0 })
    );

    let stale = plan_node(&plan, PlanKey::Node(node(&dom, "card-uptime")));
    match stale.modifier(ModifierSlot::Alpha) {
        Some(PlanModifier::Alpha(alpha)) => assert_close(*alpha, 0.6, "the stale card's alpha"),
        other => panic!("the stale card is faded as a group: {other:?}"),
    }
}

/// Clicking a section link selects it, and the heading follows. Refreshing brings the
/// faded card back to full opacity.
#[test]
fn fr34_dashboard_links_and_refresh_reach_their_handlers() {
    let mut dom = laid_out(1000.0);
    assert_eq!(text_of(entry(&dom, "page-title")), "Overview");

    let reports = node(&dom, "nav-3");
    let (x, y) = centre(entry(&dom, "nav-3").rect);
    assert_eq!(dom.click(x, y), Some(reports));
    dom.render();
    dom.layout(1000.0, 800.0, 1.0);
    assert_eq!(text_of(entry(&dom, "page-title")), "Reports");
    assert_eq!(
        entry(&dom, "nav-3").background,
        Some(Rgba::new(221, 244, 255, 255)),
        "the selected link is highlighted"
    );
    assert_eq!(entry(&dom, "nav-0").background, None);

    let refresh = node(&dom, "refresh");
    let (x, y) = centre(entry(&dom, "refresh").rect);
    assert_eq!(dom.click(x, y), Some(refresh));
    dom.render();
    dom.layout(1000.0, 800.0, 1.0);
    assert_eq!(entry(&dom, "card-uptime").opacity, 1.0);
}
