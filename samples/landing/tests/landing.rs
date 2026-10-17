//! The landing page laid out at a phone (400px), tablet (800px) and desktop (1200px) width.
//!
//! Every number follows from `landing::STYLE` (all boxes `border-box`) and the support
//! measurer (an advance of half the font size per character, the CSS line height per
//! line). The page is `container` (max-width 960px, 24px side padding, 16px on a phone)
//! centred with `margin: 0 auto`, so its content is:
//!
//! - desktop, 1200px: container 960px wide at x = 120, content 912px wide from x = 144;
//! - tablet, 800px: the container is the whole window, content 752px wide from x = 24;
//! - phone, 400px: content 368px wide from x = 16.
//!
//! Vertical positions are the sums of the boxes above, worked out beside each test.

use dioxus_compose::html::{
    AssetId, BackgroundImage, HtmlDom, ImageSize, LiteralColours, PlanKey, PlanKind, Rect, Rgba,
};
use sample_html_landing as landing;
use sample_html_support::{
    Images, Measurer, assert_close, assert_rect, centre, config_with_images, entry, node,
    plan_node, text_of,
};

const PHONE: f32 = 400.0;
const TABLET: f32 = 800.0;
const DESKTOP: f32 = 1200.0;

fn images() -> Images {
    Images::new(vec![(
        "images/logo.png",
        ImageSize::new(56, 56),
        AssetId(1),
    )])
}

fn laid_out(width: f32) -> HtmlDom {
    let mut dom = HtmlDom::with_config(
        landing::app,
        config_with_images(landing::STYLE, Measurer::new(), images()),
    );
    dom.layout(width, 900.0, 1.0);
    dom
}

fn rect_of(dom: &HtmlDom, id: &str) -> Rect {
    entry(dom, id).rect
}

/// `max-width: 960px; margin: 0 auto` centres the container: (1200 - 960) / 2 = 120px from
/// the left on a desktop. A window narrower than the maximum gets the whole width, so the
/// margins are zero. The hero behind it is not in the container and spans the window.
#[test]
fn fr34_landing_container_is_centred_by_auto_margins() {
    let dom = laid_out(DESKTOP);
    assert_rect(
        rect_of(&dom, "hero-inner"),
        120.0,
        65.0,
        960.0,
        368.0,
        "desktop",
    );
    assert_rect(
        rect_of(&dom, "hero"),
        0.0,
        65.0,
        1200.0,
        368.0,
        "the hero's band",
    );
    // 24px of padding inside the container.
    assert_close(rect_of(&dom, "hero-title").x, 144.0, "desktop heading x");
    assert_close(
        rect_of(&dom, "hero-title").width,
        912.0,
        "desktop heading width",
    );

    let dom = laid_out(TABLET);
    assert_rect(
        rect_of(&dom, "hero-inner"),
        0.0,
        65.0,
        800.0,
        296.0,
        "tablet",
    );
    assert_close(
        rect_of(&dom, "hero-title").width,
        752.0,
        "tablet heading width",
    );

    let dom = laid_out(PHONE);
    assert_close(rect_of(&dom, "hero-inner").width, 400.0, "phone container");
    assert_close(rect_of(&dom, "hero-title").x, 16.0, "phone heading x");
    assert_close(
        rect_of(&dom, "hero-title").width,
        368.0,
        "phone heading width",
    );
}

/// On a tablet and a desktop the menu is a row at the right edge of the content, vertically
/// centred in the 64px bar: the links are 24px tall, (64 - 24) / 2 = 20px down. Their text
/// is "Features" 8 x 8 = 64px, "Pricing" and "Contact" 7 x 8 = 56px, with 24px gaps, so the
/// menu is 64 + 56 + 56 + 2 x 24 = 224px wide and ends at the content's right edge.
///
/// - desktop: content ends at 144 + 912 = 1056, so the links start at 832, 920 and 1000;
/// - tablet: content ends at 24 + 752 = 776, so they start at 552, 640 and 720.
///
/// The brand is a 28px logo, an 8px gap and "Lumen" (5 x 10 = 50px at 20px): 86px wide,
/// 28px tall, so 18px down at the left edge of the content.
#[test]
fn fr34_landing_menu_is_a_row_on_tablet_and_desktop() {
    for (width, left, xs) in [
        (DESKTOP, 144.0, [832.0, 920.0, 1000.0]),
        (TABLET, 24.0, [552.0, 640.0, 720.0]),
    ] {
        let dom = laid_out(width);
        for (index, (x, link_width)) in xs.into_iter().zip([64.0, 56.0, 56.0]).enumerate() {
            assert_rect(
                rect_of(&dom, &format!("menu-{index}")),
                x,
                20.0,
                link_width,
                24.0,
                &format!("link {index} at {width}"),
            );
        }
        assert_rect(rect_of(&dom, "brand"), left, 18.0, 86.0, 28.0, "the brand");
        // The bar is 64px plus its 1px bottom border, and the hero starts below it.
        assert_rect(rect_of(&dom, "topbar"), 0.0, 0.0, width, 65.0, "the bar");
    }
}

/// Under 600px the nav is a column: 12px of padding, the brand (28px) at the top, and the
/// menu stretched under it with each link 8 + 24 + 8 = 40px tall, so at y = 40, 80 and 120.
/// The nav is 12 + 28 + 120 + 12 = 172px tall, and the bar 173px with its border.
#[test]
fn fr34_landing_menu_stacks_on_a_phone() {
    let dom = laid_out(PHONE);
    assert_rect(rect_of(&dom, "brand"), 16.0, 12.0, 86.0, 28.0, "the brand");
    for (index, y) in [40.0, 80.0, 120.0].into_iter().enumerate() {
        assert_rect(
            rect_of(&dom, &format!("menu-{index}")),
            16.0,
            y,
            368.0,
            40.0,
            &format!("link {index}"),
        );
    }
    assert_rect(rect_of(&dom, "nav"), 16.0, 0.0, 368.0, 172.0, "the nav");
    assert_rect(rect_of(&dom, "topbar"), 0.0, 0.0, 400.0, 173.0, "the bar");
}

/// The heading and the paragraph step down with the width, and the buttons go from a row
/// to a column.
///
/// - Desktop: the hero starts at y = 65 with 96px of padding. The heading is 48px type on a
///   56px line, "Build pages that adapt" is 22 x 24 = 528px and fits one line. 16px margin,
///   then the paragraph (49 characters x 8 = 392px, one 24px line) at 161 + 56 + 16 = 233,
///   32px margin, and the buttons at 233 + 24 + 32 = 289, 140 by 48 with a 16px gap. The
///   hero ends at 289 + 48 + 96 = 433.
/// - Tablet: 64px of padding and 40px type on a 48px line (22 x 20 = 440px, one line):
///   heading at 129, paragraph at 129 + 48 + 16 = 193, buttons at 193 + 24 + 32 = 249,
///   ending at 249 + 48 + 64 = 361.
/// - Phone: the bar is 173px, 48px of padding, 36px type on a 44px line. The heading has
///   368 / 18 = 20 characters a line, so "Build pages that" and "adapt" take two lines, 88px.
///   The paragraph at 221 + 88 + 16 = 325 also wraps (368 / 8 = 46 characters, it has 49):
///   48px. The buttons stack, 24px below it at 325 + 48 + 24 = 397, 368 by 48 with a 12px
///   gap, the second at 457. The hero ends at 505 + 48 = 553.
#[test]
fn fr34_landing_hero_steps_down_and_stacks_its_buttons() {
    let dom = laid_out(DESKTOP);
    assert_rect(
        rect_of(&dom, "hero-title"),
        144.0,
        161.0,
        912.0,
        56.0,
        "title",
    );
    assert_rect(
        rect_of(&dom, "hero-lead"),
        144.0,
        233.0,
        912.0,
        24.0,
        "lead",
    );
    assert_rect(
        rect_of(&dom, "cta-primary"),
        144.0,
        289.0,
        140.0,
        48.0,
        "first",
    );
    assert_rect(
        rect_of(&dom, "cta-secondary"),
        300.0,
        289.0,
        140.0,
        48.0,
        "second",
    );
    assert_rect(rect_of(&dom, "hero"), 0.0, 65.0, 1200.0, 368.0, "hero");

    let dom = laid_out(TABLET);
    assert_rect(
        rect_of(&dom, "hero-title"),
        24.0,
        129.0,
        752.0,
        48.0,
        "title",
    );
    assert_rect(rect_of(&dom, "hero-lead"), 24.0, 193.0, 752.0, 24.0, "lead");
    assert_rect(
        rect_of(&dom, "cta-primary"),
        24.0,
        249.0,
        140.0,
        48.0,
        "first",
    );
    assert_rect(
        rect_of(&dom, "cta-secondary"),
        180.0,
        249.0,
        140.0,
        48.0,
        "second",
    );
    assert_rect(rect_of(&dom, "hero"), 0.0, 65.0, 800.0, 296.0, "hero");

    let dom = laid_out(PHONE);
    assert_rect(
        rect_of(&dom, "hero-title"),
        16.0,
        221.0,
        368.0,
        88.0,
        "title",
    );
    assert_rect(rect_of(&dom, "hero-lead"), 16.0, 325.0, 368.0, 48.0, "lead");
    assert_rect(
        rect_of(&dom, "cta-primary"),
        16.0,
        397.0,
        368.0,
        48.0,
        "first",
    );
    assert_rect(
        rect_of(&dom, "cta-secondary"),
        16.0,
        457.0,
        368.0,
        48.0,
        "second",
    );
    assert_rect(rect_of(&dom, "hero"), 0.0, 173.0, 400.0, 380.0, "hero");
}

/// Feature cards are 1 + 24 + 28 + 8 + 24 + 24 + 1 = 110px tall (border, padding, a 28px
/// title, a margin, a 24px line), 24px apart.
///
/// - Desktop: three columns of (912 - 2 x 24) / 3 = 288px at x = 144, 456 and 768. The
///   section starts at the hero's end, 433, with 64px of padding, a 40px heading and a 24px
///   margin, so the grid is at 433 + 64 + 40 + 24 = 561 and the second row at 561 + 134 = 695.
/// - Tablet: two columns of (752 - 24) / 2 = 364px at x = 24 and 412. Section at 361 with
///   48px of padding and a 36px heading: grid at 361 + 48 + 36 + 24 = 469, rows 134 apart:
///   469, 603 and 737.
/// - Phone: one column of 368px at x = 16. Section at 553, 48px padding, a 32px heading:
///   grid at 553 + 48 + 32 + 24 = 657, six rows 134px apart.
#[test]
fn fr34_landing_features_collapse_from_three_columns_to_one() {
    let dom = laid_out(DESKTOP);
    let expected = [
        (144.0, 561.0),
        (456.0, 561.0),
        (768.0, 561.0),
        (144.0, 695.0),
        (456.0, 695.0),
        (768.0, 695.0),
    ];
    for (index, (x, y)) in expected.into_iter().enumerate() {
        assert_rect(
            rect_of(&dom, &format!("feature-{index}")),
            x,
            y,
            288.0,
            110.0,
            "desktop",
        );
    }

    let dom = laid_out(TABLET);
    let expected = [
        (24.0, 469.0),
        (412.0, 469.0),
        (24.0, 603.0),
        (412.0, 603.0),
        (24.0, 737.0),
        (412.0, 737.0),
    ];
    for (index, (x, y)) in expected.into_iter().enumerate() {
        assert_rect(
            rect_of(&dom, &format!("feature-{index}")),
            x,
            y,
            364.0,
            110.0,
            "tablet",
        );
    }

    let dom = laid_out(PHONE);
    for index in 0..6 {
        assert_rect(
            rect_of(&dom, &format!("feature-{index}")),
            16.0,
            657.0 + 134.0 * index as f32,
            368.0,
            110.0,
            "phone",
        );
    }
}

/// The widths where the queries switch. The tablet query is `max-width: 1024px` and the
/// phone query `max-width: 599px`, so 1024 is still a tablet (two feature columns, the
/// third card on the second row), 1025 is a desktop (three, all on the first), 600 is a
/// tablet (the menu is a row, the bar 65px) and 599 a phone (stacked, 173px).
#[test]
fn fr34_landing_queries_switch_at_600_and_1024() {
    let third_is_on_the_first_row = |width: f32| {
        let dom = laid_out(width);
        rect_of(&dom, "feature-2").y == rect_of(&dom, "feature-0").y
    };
    assert!(!third_is_on_the_first_row(1024.0));
    assert!(third_is_on_the_first_row(1025.0));

    assert_close(
        rect_of(&laid_out(600.0), "topbar").height,
        65.0,
        "600px bar",
    );
    assert_close(
        rect_of(&laid_out(599.0), "topbar").height,
        173.0,
        "599px bar",
    );
}

/// Pricing: the section starts where the features end.
///
/// - Desktop: features end at 561 + 244 + 64 = 869 (two rows of 110 and a 24px gap, then
///   the padding). 64px padding, 40px heading, 24px margin, the 40px toggle and another
///   24px margin put the plans at 869 + 64 + 40 + 24 + 40 + 24 = 1061. Three columns of
///   288px with 24px gaps. A plan is 1 + 24 + 28 (name) + 44 (price) + 20 + 16 (period)
///   + 72 (three lines) + 24 + 1 = 230px tall.
/// - Tablet: features end at 469 + 378 + 48 = 895, so the plans are at
///   895 + 48 + 36 + 24 + 40 + 24 = 1067. The gap is 16px, so three columns are
///   (752 - 32) / 3 = 240px wide at x = 24, 280 and 536.
/// - Phone: features end at 657 + 780 + 48 = 1485, plans at 1485 + 48 + 32 + 24 + 40 + 24
///   = 1653, one 368px column with 16px gaps: 230 + 16 = 246px apart.
#[test]
fn fr34_landing_plans_sit_in_three_columns_then_one() {
    let dom = laid_out(DESKTOP);
    for (index, x) in [144.0, 456.0, 768.0].into_iter().enumerate() {
        assert_rect(
            rect_of(&dom, &format!("plan-{index}")),
            x,
            1061.0,
            288.0,
            230.0,
            "desktop",
        );
    }
    assert_rect(
        rect_of(&dom, "billing-monthly"),
        144.0,
        997.0,
        96.0,
        40.0,
        "monthly",
    );
    assert_rect(
        rect_of(&dom, "billing-yearly"),
        248.0,
        997.0,
        96.0,
        40.0,
        "yearly",
    );
    // Inside the middle plan: 1 + 24 below its top the name, 28px under that the price.
    assert_rect(
        rect_of(&dom, "price-1"),
        481.0,
        1114.0,
        238.0,
        44.0,
        "middle price",
    );

    let dom = laid_out(TABLET);
    for (index, x) in [24.0, 280.0, 536.0].into_iter().enumerate() {
        assert_rect(
            rect_of(&dom, &format!("plan-{index}")),
            x,
            1067.0,
            240.0,
            230.0,
            "tablet",
        );
    }

    let dom = laid_out(PHONE);
    for index in 0..3 {
        assert_rect(
            rect_of(&dom, &format!("plan-{index}")),
            16.0,
            1653.0 + 246.0 * index as f32,
            368.0,
            230.0,
            "phone",
        );
    }
}

/// The footer starts where the pricing ends (the plans' bottom plus the section padding)
/// and has 48px of padding. Its link columns are 24 + 8 + 3 x 24 = 104px tall.
///
/// - Desktop: pricing ends at 1061 + 230 + 64 = 1355, columns at 1355 + 48 = 1403, three of
///   288px at x = 144, 456 and 768. Footer height 48 + 104 + 32 (margin) + 20 (the
///   copyright line) + 48 = 252.
/// - Tablet: 1067 + 230 + 48 = 1345, columns at 1393, 240px wide with 16px gaps at
///   x = 24, 280 and 536.
/// - Phone: 1653 + 722 + 48 = 2423, columns at 2471 in one column 128px apart (104 and a
///   24px gap). Footer height 48 + 3 x 104 + 2 x 24 + 32 + 20 + 48 = 508.
#[test]
fn fr34_landing_footer_columns_collapse() {
    let dom = laid_out(DESKTOP);
    assert_rect(
        rect_of(&dom, "footer"),
        0.0,
        1355.0,
        1200.0,
        252.0,
        "desktop footer",
    );
    for (index, x) in [144.0, 456.0, 768.0].into_iter().enumerate() {
        assert_rect(
            rect_of(&dom, &format!("footer-col-{index}")),
            x,
            1403.0,
            288.0,
            104.0,
            "desktop",
        );
    }

    let dom = laid_out(TABLET);
    assert_rect(
        rect_of(&dom, "footer"),
        0.0,
        1345.0,
        800.0,
        252.0,
        "tablet footer",
    );
    for (index, x) in [24.0, 280.0, 536.0].into_iter().enumerate() {
        assert_rect(
            rect_of(&dom, &format!("footer-col-{index}")),
            x,
            1393.0,
            240.0,
            104.0,
            "tablet",
        );
    }

    let dom = laid_out(PHONE);
    assert_rect(
        rect_of(&dom, "footer"),
        0.0,
        2423.0,
        400.0,
        508.0,
        "phone footer",
    );
    for index in 0..3 {
        assert_rect(
            rect_of(&dom, &format!("footer-col-{index}")),
            16.0,
            2471.0 + 128.0 * index as f32,
            368.0,
            104.0,
            "phone",
        );
    }
}

const WHITE: Rgba = Rgba::new(255, 255, 255, 255);
const BORDER: Rgba = Rgba::new(208, 215, 222, 255);
const INDIGO: Rgba = Rgba::new(79, 70, 229, 255);
/// rgba(31, 35, 40, 0.2): 0.2 x 255 = 51.
const CARD_SHADOW: Rgba = Rgba::new(31, 35, 40, 51);
/// rgba(79, 70, 229, 0.2).
const FEATURED_SHADOW: Rgba = Rgba::new(79, 70, 229, 51);

/// Cards have 12px corners and a 0 1px 3px shadow. The middle plan alone is highlighted: a
/// tinted background, an indigo border and a 0 8px 24px shadow.
#[test]
fn fr34_landing_cards_are_rounded_and_the_middle_plan_is_highlighted() {
    let dom = laid_out(DESKTOP);

    let card = entry(&dom, "feature-0");
    let radii = card.radii.expect("a card has rounded corners");
    for corner in [
        radii.top_left,
        radii.top_right,
        radii.bottom_right,
        radii.bottom_left,
    ] {
        assert_eq!((corner.x, corner.y), (12.0, 12.0));
    }
    assert_eq!(card.shadows.len(), 1);
    let shadow = card.shadows[0];
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
    assert_eq!(card.background, Some(WHITE));

    let plain = entry(&dom, "plan-0");
    assert_eq!(plain.background, Some(WHITE));
    assert_eq!(plain.border.expect("a border").colors.left, BORDER);

    let featured = entry(&dom, "plan-1");
    assert_eq!(featured.background, Some(Rgba::new(238, 242, 255, 255)));
    let border = featured.border.expect("a border");
    assert_eq!((border.widths.left, border.widths.top), (1.0, 1.0));
    assert_eq!(border.colors.left, INDIGO);
    assert_eq!(featured.shadows.len(), 1);
    let shadow = featured.shadows[0];
    assert_eq!(
        (shadow.offset_x, shadow.offset_y, shadow.blur, shadow.spread),
        (0.0, 8.0, 24.0, 0.0)
    );
    assert_eq!(shadow.color, FEATURED_SHADOW);
}

/// The hero is painted by a left-to-right gradient from #4f46e5 to #06b6d4 across the
/// whole 1200px window: from x = 0 to x = 1200. The logo is an image the resolver names,
/// laid out at the 28px the stylesheet gives it, not at its natural 56px.
#[test]
fn fr34_landing_hero_gradient_and_logo_image() {
    let mut dom = laid_out(DESKTOP);

    let hero = entry(&dom, "hero");
    assert_eq!(hero.backgrounds.len(), 1, "{:#?}", hero.backgrounds);
    let BackgroundImage::Linear(gradient) = &hero.backgrounds[0].image else {
        panic!("a linear gradient: {:#?}", hero.backgrounds[0]);
    };
    assert_close(gradient.angle, 90.0, "`to right`");
    assert_close(gradient.start.0, 0.0, "start x");
    assert_close(gradient.end.0, 1200.0, "end x");
    assert_eq!(gradient.stops.len(), 2);
    assert_eq!(gradient.stops[0].color, INDIGO);
    assert_eq!(gradient.stops[1].color, Rgba::new(6, 182, 212, 255));

    // Centred in the 64px bar: (64 - 28) / 2 = 18px down, at the content's left edge.
    let logo = entry(&dom, "logo");
    assert_rect(logo.rect, 144.0, 18.0, 28.0, 28.0, "the logo");
    let image = logo.image.as_ref().expect("the logo is an image");
    assert_eq!(image.source, "images/logo.png");
    assert_eq!(image.natural_size, Some((56.0, 56.0)));

    let plan = dom.plan(&mut LiteralColours).expect("laid out above");
    let drawn = plan_node(&plan, PlanKey::Image(node(&dom, "logo")));
    let PlanKind::Image(drawn) = &drawn.kind else {
        panic!("the logo draws an image");
    };
    assert_eq!(drawn.asset, AssetId(1));
}

/// The billing toggle starts on monthly: $12, $29 and $79 a month, and the monthly button
/// is the highlighted one. Clicking "Yearly" shows $10, $24 and $66 under "per month,
/// billed yearly" and moves the highlight; the layout does not move, and clicking
/// "Monthly" brings the first prices back.
#[test]
fn fr34_landing_billing_toggle_changes_the_prices() {
    let mut dom = laid_out(DESKTOP);
    let prices = |dom: &HtmlDom| -> Vec<String> {
        (0..3)
            .map(|index| text_of(entry(dom, &format!("price-{index}"))))
            .collect()
    };
    assert_eq!(prices(&dom), ["$12", "$29", "$79"]);
    assert_eq!(text_of(entry(&dom, "period-0")), "per month");
    assert_eq!(entry(&dom, "billing-monthly").background, Some(INDIGO));
    assert_eq!(entry(&dom, "billing-yearly").background, Some(WHITE));
    let before = rect_of(&dom, "plan-1");

    let yearly = node(&dom, "billing-yearly");
    let (x, y) = centre(rect_of(&dom, "billing-yearly"));
    assert_eq!(dom.click(x, y), Some(yearly));
    dom.render();
    dom.layout(DESKTOP, 900.0, 1.0);
    assert_eq!(prices(&dom), ["$10", "$24", "$66"]);
    assert_eq!(text_of(entry(&dom, "period-2")), "per month, billed yearly");
    assert_eq!(entry(&dom, "billing-yearly").background, Some(INDIGO));
    assert_eq!(entry(&dom, "billing-monthly").background, Some(WHITE));
    assert_eq!(rect_of(&dom, "plan-1"), before);

    let monthly = node(&dom, "billing-monthly");
    let (x, y) = centre(rect_of(&dom, "billing-monthly"));
    assert_eq!(dom.click(x, y), Some(monthly));
    dom.render();
    dom.layout(DESKTOP, 900.0, 1.0);
    assert_eq!(prices(&dom), ["$12", "$29", "$79"]);
}
