//! The layout example: flexbox, CSS grid and positioning in an 800px-wide page.
//!
//! Every number follows from `layout::STYLE` (all boxes `border-box`) and the support
//! measurer (half the font size per character, the CSS line height per line). The page is
//! laid out at 800 by 1700, tall enough that nothing is cut off, and the measurer only
//! matters for the headings, whose line height is a fixed 24px.
//!
//! The frame of the page:
//!
//! - The sticky header is 48px tall and 800px wide at the top.
//! - `main` has 16px of padding and a 16px gap between panels, so the panels are 768px
//!   wide at x = 16, and the first one starts at y = 48 + 16 = 64.
//! - A panel has 16px of padding, so its contents start at x = 32 and are 736px wide. Its
//!   head is 32px tall with a 12px margin, so the contents start 16 + 32 + 12 = 60px below
//!   the panel's top.
//! - Demos inside a panel are a column 12px apart.
//!
//! Flexbox panel (top 64, contents from y = 124):
//!
//! - wrap box: 160 x 40 items, 8px gaps. Four fit in a row (4 x 160 + 3 x 8 = 664 <= 736,
//!   five would need 832), so the fifth starts the second row, 40 + 8 = 48px down. The
//!   box is 88px tall. As a column of the same items it is 5 x 40 + 4 x 8 = 232px tall.
//! - grow row (y = 124 + 88 + 12 = 224, 40 tall): bases 100, 100 and 136, free space
//!   736 - 336 = 400 shared 1 : 3, so 100 + 100 = 200 and 100 + 300 = 400 wide.
//! - shrink row (y = 276): bases 400, 400 and 136 make 936, 200 too much. Shrinking is
//!   weighted by shrink factor x basis: 400 x 1 and 400 x 3 are 400 and 1200 of 1600, so
//!   the first gives up 200 / 4 = 50 and the second 150, leaving 350 and 250.
//! - align rows (y = 328 and y = 420, 80 tall, 60px wide items 20, 40 and 60 tall). Spread
//!   with `space-between` the 556px of free space (736 - 180) sits as two gaps of 278, and
//!   centring puts each item (80 - height) / 2 below the top. At the end with centred
//!   justification the items start 556 / 2 = 278 in and rest on the bottom edge.
//! - order row (y = 512): 100 x 40 items 8px apart, shown B, C, A by their `order`. The
//!   panel ends at 512 + 40 + 16 = 568, so it is 504 tall.
//!
//! Grid panel (top 568 + 16 = 584, contents from y = 644):
//!
//! - areas: columns 160 and 736 - 160 - 8 = 568, rows 40, 120 and 40, 8px gaps. The header
//!   spans both columns, the nav and main share the middle row, the footer spans both.
//!   Rows are at y = 644, 692 and 820 and the grid is 216 tall.
//! - cards: `repeat(auto-fill, minmax(160px, 1fr))` makes four columns (4 x 160 + 3 x 8 =
//!   664, five need 832) of (736 - 24) / 4 = 178px. Six cards take two 60px rows, 68px
//!   apart, and the grid is 128 tall (y = 644 + 216 + 12 = 872).
//! - spans (y = 1012): four 178px columns, 40px rows. `a` spans two columns (178 x 2 + 8 =
//!   364 wide), `c` two rows (40 + 8 + 40 = 88 tall) and automatic placement fills the
//!   gaps around them: a (1-2, row 1), b (3, row 1), c (4, rows 1-2), d, e, f (1, 2 and 3,
//!   row 2).
//! - lines (y = 1112): three 40px rows. `a` is `2 / 4` by `1 / 3`: x = 32 + 186, 364 x 88.
//!   `b` is column 1 of row 3 (y = 1112 + 96 = 1208). `c` is `4 / 5` by `2 / 4`: x = 590,
//!   y = 1160, 178 x 88. The grid is 136 tall and the panel ends at 1248 + 16 = 1264.
//!
//! Position panel (top 1280, contents from y = 1340):
//!
//! - the stack is 140 tall; its layers are 100px squares at offsets (0, 0), (40, 20) and
//!   (80, 40) inside it, painted by `z-index` (green 1, blue 2, red 3) and not by tree
//!   order (red, green, blue).
//! - the anchor (y = 1340 + 140 + 12 = 1492) is 300 x 100. The badge is 8px from its right
//!   and bottom edges: x = 32 + 300 - 8 - 80, y = 1492 + 100 - 8 - 24. The pin is 10px from
//!   the top and at half the anchor's width from the left: x = 32 + 150.

use dioxus_compose::html::{HtmlDom, NodeId};
use sample_html_layout as layout;
use sample_html_support::{Measurer, assert_rect, centre, config, entry, node};

const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 1700.0;

fn laid_out() -> HtmlDom {
    let mut dom = HtmlDom::with_config(layout::app, config(layout::STYLE, Measurer::new()));
    dom.layout(WIDTH, HEIGHT, 1.0);
    dom
}

fn rect_of(dom: &HtmlDom, id: &str, x: f32, y: f32, width: f32, height: f32) {
    assert_rect(entry(dom, id).rect, x, y, width, height, id);
}

/// The ids in `ids`, ordered by when the last layout paints them.
fn paint_order(dom: &HtmlDom, ids: &[&str]) -> Vec<NodeId> {
    let wanted: Vec<NodeId> = ids.iter().map(|id| node(dom, id)).collect();
    dom.display_list()
        .expect("the document has been laid out")
        .order()
        .into_iter()
        .filter(|node| wanted.contains(node))
        .collect()
}

#[test]
fn fr34_layout_panels_stack_under_the_header() {
    let dom = laid_out();
    rect_of(&dom, "site-header", 0.0, 0.0, 800.0, 48.0);
    rect_of(&dom, "panel-flex", 16.0, 64.0, 768.0, 504.0);
    rect_of(&dom, "panel-grid", 16.0, 584.0, 768.0, 680.0);
    rect_of(&dom, "panel-position", 16.0, 1280.0, 768.0, 328.0);
    // Each heading is centred in its 32px head, 16px inside the panel.
    rect_of(
        &dom,
        "title-flex",
        32.0,
        84.0,
        entry(&dom, "title-flex").rect.width,
        24.0,
    );
    rect_of(
        &dom,
        "title-grid",
        32.0,
        604.0,
        entry(&dom, "title-grid").rect.width,
        24.0,
    );
}

#[test]
fn fr34_layout_flex_wrap_breaks_after_four_items() {
    let dom = laid_out();
    rect_of(&dom, "wrap-box", 32.0, 124.0, 736.0, 88.0);
    let expected = [
        ("w1", 32.0, 124.0),
        ("w2", 200.0, 124.0),
        ("w3", 368.0, 124.0),
        ("w4", 536.0, 124.0),
        ("w5", 32.0, 172.0),
    ];
    for (id, x, y) in expected {
        rect_of(&dom, id, x, y, 160.0, 40.0);
    }
}

#[test]
fn fr34_layout_flex_grow_shares_free_space_by_factor() {
    let dom = laid_out();
    rect_of(&dom, "grow-row", 32.0, 224.0, 736.0, 40.0);
    rect_of(&dom, "g1", 32.0, 224.0, 200.0, 40.0);
    rect_of(&dom, "g2", 232.0, 224.0, 400.0, 40.0);
    rect_of(&dom, "g3", 632.0, 224.0, 136.0, 40.0);
}

#[test]
fn fr34_layout_flex_shrink_takes_back_overflow_by_factor_and_basis() {
    let dom = laid_out();
    rect_of(&dom, "k1", 32.0, 276.0, 350.0, 40.0);
    rect_of(&dom, "k2", 382.0, 276.0, 250.0, 40.0);
    rect_of(&dom, "k3", 632.0, 276.0, 136.0, 40.0);
}

#[test]
fn fr34_layout_flex_align_items_and_justify_content() {
    let dom = laid_out();
    rect_of(&dom, "align-spread", 32.0, 328.0, 736.0, 80.0);
    // space-between and centred across.
    rect_of(&dom, "a1", 32.0, 358.0, 60.0, 20.0);
    rect_of(&dom, "a2", 370.0, 348.0, 60.0, 40.0);
    rect_of(&dom, "a3", 708.0, 338.0, 60.0, 60.0);
    // Centred along the row and resting on the bottom edge (y = 420 + 80 = 500).
    rect_of(&dom, "e1", 310.0, 480.0, 60.0, 20.0);
    rect_of(&dom, "e2", 370.0, 460.0, 60.0, 40.0);
    rect_of(&dom, "e3", 430.0, 440.0, 60.0, 60.0);
}

#[test]
fn fr34_layout_flex_order_reorders_the_row_not_the_tree() {
    let dom = laid_out();
    rect_of(&dom, "o2", 32.0, 512.0, 100.0, 40.0);
    rect_of(&dom, "o3", 140.0, 512.0, 100.0, 40.0);
    rect_of(&dom, "o1", 248.0, 512.0, 100.0, 40.0);
}

#[test]
fn fr34_layout_grid_areas_place_each_region() {
    let dom = laid_out();
    rect_of(&dom, "areas", 32.0, 644.0, 736.0, 216.0);
    rect_of(&dom, "area-head", 32.0, 644.0, 736.0, 40.0);
    rect_of(&dom, "area-nav", 32.0, 692.0, 160.0, 120.0);
    rect_of(&dom, "area-main", 200.0, 692.0, 568.0, 120.0);
    rect_of(&dom, "area-foot", 32.0, 820.0, 736.0, 40.0);
}

#[test]
fn fr34_layout_grid_auto_fill_makes_four_columns() {
    let dom = laid_out();
    rect_of(&dom, "card-grid", 32.0, 872.0, 736.0, 128.0);
    let expected = [
        ("card-1", 32.0, 872.0),
        ("card-2", 218.0, 872.0),
        ("card-3", 404.0, 872.0),
        ("card-4", 590.0, 872.0),
        ("card-5", 32.0, 940.0),
        ("card-6", 218.0, 940.0),
    ];
    for (id, x, y) in expected {
        rect_of(&dom, id, x, y, 178.0, 60.0);
    }
}

#[test]
fn fr34_layout_grid_items_span_columns_and_rows() {
    let dom = laid_out();
    rect_of(&dom, "span-grid", 32.0, 1012.0, 736.0, 88.0);
    rect_of(&dom, "span-a", 32.0, 1012.0, 364.0, 40.0);
    rect_of(&dom, "span-b", 404.0, 1012.0, 178.0, 40.0);
    rect_of(&dom, "span-c", 590.0, 1012.0, 178.0, 88.0);
    rect_of(&dom, "span-d", 32.0, 1060.0, 178.0, 40.0);
    rect_of(&dom, "span-e", 218.0, 1060.0, 178.0, 40.0);
    rect_of(&dom, "span-f", 404.0, 1060.0, 178.0, 40.0);
}

#[test]
fn fr34_layout_grid_explicit_lines_place_items() {
    let dom = laid_out();
    rect_of(&dom, "line-grid", 32.0, 1112.0, 736.0, 136.0);
    rect_of(&dom, "line-a", 218.0, 1112.0, 364.0, 88.0);
    rect_of(&dom, "line-b", 32.0, 1208.0, 178.0, 40.0);
    rect_of(&dom, "line-c", 590.0, 1160.0, 178.0, 88.0);
}

/// The sticky header sits in the flow at the top and is painted over the panels, since it
/// is positioned with a `z-index`. Overlapping layers are painted green, blue, red by
/// `z-index`, and a point in an overlap belongs to the topmost one.
#[test]
fn fr34_layout_z_index_orders_overlapping_boxes_and_the_sticky_header() {
    let dom = laid_out();

    let order = paint_order(&dom, &["panel-flex", "panel-position", "site-header"]);
    assert_eq!(
        order,
        vec![
            node(&dom, "panel-flex"),
            node(&dom, "panel-position"),
            node(&dom, "site-header")
        ]
    );

    rect_of(&dom, "stack", 32.0, 1340.0, 736.0, 140.0);
    rect_of(&dom, "layer-red", 32.0, 1340.0, 100.0, 100.0);
    rect_of(&dom, "layer-green", 72.0, 1360.0, 100.0, 100.0);
    rect_of(&dom, "layer-blue", 112.0, 1380.0, 100.0, 100.0);

    let layers = paint_order(&dom, &["layer-red", "layer-green", "layer-blue"]);
    assert_eq!(
        layers,
        vec![
            node(&dom, "layer-green"),
            node(&dom, "layer-blue"),
            node(&dom, "layer-red")
        ]
    );

    let list = dom.display_list().expect("laid out");
    // Only red: (50, 1350). Red over green: (80, 1370). All three: (120, 1400). Green
    // under blue: (150, 1450).
    assert_eq!(list.hit_test(50.0, 1350.0), Some(node(&dom, "layer-red")));
    assert_eq!(list.hit_test(80.0, 1370.0), Some(node(&dom, "layer-red")));
    assert_eq!(list.hit_test(120.0, 1400.0), Some(node(&dom, "layer-red")));
    assert_eq!(list.hit_test(150.0, 1450.0), Some(node(&dom, "layer-blue")));
    assert_eq!(list.hit_test(160.0, 1455.0), Some(node(&dom, "layer-blue")));
    // Below the stack's reach: only green is under (80, 1455).
    assert_eq!(list.hit_test(80.0, 1455.0), Some(node(&dom, "layer-green")));
}

#[test]
fn fr34_layout_absolute_boxes_sit_in_their_relative_parent() {
    let dom = laid_out();
    rect_of(&dom, "anchor", 32.0, 1492.0, 300.0, 100.0);
    rect_of(&dom, "badge", 244.0, 1560.0, 80.0, 24.0);
    rect_of(&dom, "pin", 182.0, 1502.0, 40.0, 20.0);
}

/// The toggle turns the wrap box into a column. The box grows from 88 to 232px, and
/// everything under it moves down by the 144px difference. A second click goes back.
#[test]
fn fr34_layout_toggle_switches_the_wrap_box_to_a_column() {
    let mut dom = laid_out();
    let toggle = node(&dom, "toggle-direction");
    let (x, y) = centre(entry(&dom, "toggle-direction").rect);
    assert_eq!(dom.click(x, y), Some(toggle));
    dom.render();
    dom.layout(WIDTH, HEIGHT, 1.0);

    rect_of(&dom, "wrap-box", 32.0, 124.0, 736.0, 232.0);
    for (index, id) in ["w1", "w2", "w3", "w4", "w5"].into_iter().enumerate() {
        rect_of(&dom, id, 32.0, 124.0 + 48.0 * index as f32, 160.0, 40.0);
    }
    rect_of(&dom, "panel-flex", 16.0, 64.0, 768.0, 648.0);
    rect_of(&dom, "panel-grid", 16.0, 728.0, 768.0, 680.0);

    let (x, y) = centre(entry(&dom, "toggle-direction").rect);
    assert_eq!(dom.click(x, y), Some(toggle));
    dom.render();
    dom.layout(WIDTH, HEIGHT, 1.0);
    rect_of(&dom, "w5", 32.0, 172.0, 160.0, 40.0);
    rect_of(&dom, "panel-grid", 16.0, 584.0, 768.0, 680.0);
}
