//! The invoice table, laid out in a 480px viewport where it is wider than the page, and in
//! an 800px one where it fits.
//!
//! Every number follows from `table::STYLE`:
//!
//! - The page's 24px padding puts the heading (one 32px line, 16px margin) at y = 24 and
//!   the wrapper at (24, 72), 480 - 48 = 432px wide.
//! - The table is `width: 100%` of that, but at least 640px: 640px.
//! - Columns take the widths of the header cells plus their 12px side padding (the cells are
//!   `content-box`): 96 + 24 = 120, 212 + 24 = 236, 116 + 24 = 140 and 120 + 24 = 144,
//!   which is 640 together. They start at x = 24, 144, 380 and 520.
//! - Every cell is 20px of content, 8px of padding above and below and a 1px bottom border:
//!   37px. The header row is at y = 72 and the four body rows follow every 37px.
//! - The wrapper is as tall as the table, 5 x 37 = 185px, and scrolls sideways only
//!   (`overflow-x: auto; overflow-y: hidden`) through the table's full 640px.
//!
//! Cell text is laid out by the table's own text layout, not by the test measurer, so its
//! width depends on the fonts on the machine. Only where the text starts or ends is
//! asserted, which `text-align` decides whatever the width.

use dioxus_compose::html::{
    DisplayList, HtmlDom, LiteralColours, ModifierSlot, NodeEntry, PlanKey, PlanKind, PlanModifier,
};
use sample_html_table as table;
use sample_html_support::{
    Measurer, assert_rect, centre, config, entries_with_tag, entry, node, plan_node, text_of,
};

const COLUMNS: [(f32, f32); 4] = [
    (24.0, 120.0),
    (144.0, 236.0),
    (380.0, 140.0),
    (520.0, 144.0),
];

fn laid_out(width: f32) -> HtmlDom {
    let mut dom = HtmlDom::with_config(table::app, config(table::STYLE, Measurer::new()));
    dom.layout(width, 600.0, 1.0);
    dom
}

/// The table's body cells, row by row.
fn body_rows(list: &DisplayList) -> Vec<Vec<&NodeEntry>> {
    entries_with_tag(list, "td")
        .chunks(4)
        .map(|row| row.to_vec())
        .collect()
}

#[test]
fn fr34_table_cells_lay_out_in_rows_and_columns() {
    let dom = laid_out(480.0);
    let list = dom.display_list().unwrap();

    assert_rect(
        entry(&dom, "invoices").rect,
        24.0,
        72.0,
        640.0,
        185.0,
        "the table",
    );

    let header = entries_with_tag(list, "th");
    assert_eq!(header.len(), 4);
    for (cell, (x, width)) in header.iter().zip(COLUMNS) {
        assert_rect(
            cell.rect,
            x,
            72.0,
            width,
            37.0,
            &format!("header cell at x = {x}"),
        );
    }

    let rows = body_rows(list);
    assert_eq!(rows.len(), 4, "four invoices");
    for (index, row) in rows.iter().enumerate() {
        let y = 72.0 + 37.0 * (index as f32 + 1.0);
        assert_eq!(row.len(), 4);
        for (cell, (x, width)) in row.iter().zip(COLUMNS) {
            assert_rect(
                cell.rect,
                x,
                y,
                width,
                37.0,
                &format!("row {index}, x = {x}"),
            );
        }
    }

    // In source order while unsorted.
    let numbers: Vec<String> = rows.iter().map(|row| text_of(row[0])).collect();
    assert_eq!(numbers, ["INV-1001", "INV-1002", "INV-1003", "INV-1004"]);
    assert_eq!(text_of(rows[0][3]), "$1,250.00");
    assert_eq!(text_of(rows[1][3]), "$98.40");
}

/// The header row sits directly above the first body row: every header cell ends where
/// the cell below it starts.
#[test]
fn fr34_table_header_row_is_above_the_body_rows() {
    let dom = laid_out(480.0);
    let list = dom.display_list().unwrap();
    let header = entries_with_tag(list, "th");
    let rows = body_rows(list);
    let first = &rows[0];
    for (above, below) in header.iter().zip(first) {
        assert_eq!(above.rect.bottom(), below.rect.y);
        assert_eq!(above.rect.x, below.rect.x);
    }
    let texts: Vec<String> = header.iter().map(|cell| text_of(cell)).collect();
    assert_eq!(texts, ["Invoice", "Client", "Due", "Amount"]);
}

/// `text-align: left` starts a cell's text at its 12px padding; `text-align: right` ends it
/// at the right padding, however wide the text is. Within a pixel: the text's own width is
/// the font's.
#[test]
fn fr34_table_columns_align_their_text() {
    let dom = laid_out(480.0);
    let list = dom.display_list().unwrap();
    let rows = body_rows(list);
    let close = |a: f32, b: f32| (a - b).abs() <= 1.0;

    for row in &rows {
        let number = row[0];
        let run = number
            .texts
            .first()
            .expect("the number cell draws its text");
        assert!(
            close(run.rect.x, number.rect.x + 12.0),
            "left-aligned: {:?} in {:?}",
            run.rect,
            number.rect
        );

        let amount = row[3];
        let run = amount.texts.last().expect("the amount cell draws its text");
        assert!(
            close(run.rect.right(), amount.rect.right() - 12.0),
            "right-aligned: {:?} in {:?}",
            run.rect,
            amount.rect
        );
    }
}

/// The wrapper scrolls sideways: in the display list its viewport is its own 432px box
/// and its content the table's 640px; in the plan it is a `ScrollRow` around content of
/// that width, with the table inside.
#[test]
fn fr34_table_wrapper_is_a_horizontal_scroll_container() {
    let mut dom = laid_out(480.0);

    let wrap = entry(&dom, "wrap");
    assert_rect(wrap.rect, 24.0, 72.0, 432.0, 185.0, "the wrapper");
    assert!(wrap.clips_children);
    let scroll = wrap.scroll.expect("overflow-x: auto scrolls");
    assert!(scroll.horizontal);
    assert!(
        !scroll.vertical,
        "overflow-y: hidden clips without scrolling"
    );
    assert_rect(
        scroll.viewport,
        24.0,
        72.0,
        432.0,
        185.0,
        "the scroll viewport",
    );
    assert_eq!(scroll.content_width, 640.0);

    let wrap_id = node(&dom, "wrap");
    let table_id = node(&dom, "invoices");
    assert_eq!(entry(&dom, "invoices").scroll_parent, Some(wrap_id));
    assert_eq!(
        entry(&dom, "invoices").clip.map(|clip| clip.width),
        Some(432.0)
    );

    let plan = dom.plan(&mut LiteralColours).expect("laid out above");
    let outer = plan_node(&plan, PlanKey::Node(wrap_id));
    let child_keys: Vec<PlanKey> = outer.children.iter().map(|child| child.key).collect();
    assert_eq!(child_keys, vec![PlanKey::ScrollRow(wrap_id)]);

    let row = plan_node(&plan, PlanKey::ScrollRow(wrap_id));
    assert_eq!(row.kind, PlanKind::ScrollRow);
    assert_eq!(
        row.modifier(ModifierSlot::RequiredSize),
        Some(&PlanModifier::RequiredSize {
            width: 432.0,
            height: 185.0
        })
    );
    let child_keys: Vec<PlanKey> = row.children.iter().map(|child| child.key).collect();
    assert_eq!(child_keys, vec![PlanKey::ScrollContent(wrap_id)]);

    let content = plan_node(&plan, PlanKey::ScrollContent(wrap_id));
    let Some(PlanModifier::RequiredSize { width, .. }) =
        content.modifier(ModifierSlot::RequiredSize)
    else {
        panic!("scroll content has a size: {content:#?}");
    };
    assert_eq!(*width, 640.0, "the content is the table's full width");
    assert_eq!(
        plan.parent_of(PlanKey::Node(table_id))
            .map(|parent| parent.key),
        Some(PlanKey::ScrollContent(wrap_id))
    );
    assert_eq!(
        plan_node(&plan, PlanKey::Node(table_id)).offset(),
        (0.0, 0.0)
    );
}

/// Clicking the amount header sorts the rows, largest amount first. At 800px the whole
/// table is in view, so the header can be clicked where it is drawn.
#[test]
fn fr34_table_clicking_the_amount_header_sorts_the_rows() {
    let mut dom = laid_out(800.0);
    let header = node(&dom, "amount-header");
    let (x, y) = centre(entry(&dom, "amount-header").rect);
    assert_eq!(dom.click(x, y), Some(header));
    dom.render();
    dom.layout(800.0, 600.0, 1.0);

    let list = dom.display_list().unwrap();
    let rows = body_rows(list);
    let numbers: Vec<String> = rows.iter().map(|row| text_of(row[0])).collect();
    assert_eq!(numbers, ["INV-1003", "INV-1001", "INV-1004", "INV-1002"]);
    let amounts: Vec<String> = rows.iter().map(|row| text_of(row[3])).collect();
    assert_eq!(amounts, ["$4,310.75", "$1,250.00", "$640.00", "$98.40"]);
    assert_eq!(text_of(entry(&dom, "amount-header")), "Amount \u{2193}");

    // The rows moved; the columns did not.
    for (index, row) in rows.iter().enumerate() {
        let y = 72.0 + 37.0 * (index as f32 + 1.0);
        for (cell, (x, width)) in row.iter().zip(COLUMNS) {
            assert_rect(cell.rect, x, y, width, 37.0, &format!("sorted row {index}"));
        }
    }
}
