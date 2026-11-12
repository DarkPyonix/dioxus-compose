//! Writes each HTML sample's first render as a standalone HTML file, for a browser to draw.
use blitz_dom::node::NodeData;
use blitz_dom::BaseDocument;
use dioxus_compose::html::prelude::*;
use std::fmt::Write;

fn esc(s: &str, attr: bool) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' if attr => o.push_str("&quot;"),
            '\u{a0}' => o.push_str("&nbsp;"),
            c => o.push(c),
        }
    }
    o
}

const VOID: &[&str] = &["area","base","br","col","embed","hr","img","input","link","meta","source","track","wbr"];

fn write(doc: &BaseDocument, id: usize, out: &mut String) {
    let node = doc.get_node(id).unwrap();
    let raw = node.parent.and_then(|p| doc.get_node(p)).and_then(|p| p.element_data()).is_some_and(|e| matches!(&*e.name.local, "style" | "script"));
    match &node.data {
        NodeData::Text(t) if raw => out.push_str(&t.content),
        NodeData::Text(t) => out.push_str(&esc(&t.content, false)),
        NodeData::Element(e) => {
            let name = e.name.local.to_string();
            write!(out, "<{name}").unwrap();
            for a in e.attrs() {
                write!(out, " {}=\"{}\"", a.name.local, esc(&a.value, true)).unwrap();
            }
            // A select's value names the option that is selected.
            if name == "option" {
                let value = e.attr(blitz_dom::local_name!("value")).map(str::to_string).unwrap_or_else(|| node.text_content());
                let sel = node.parent.and_then(|p| doc.get_node(p)).and_then(|p| p.element_data()).and_then(|p| p.attr(blitz_dom::local_name!("value")).map(str::to_string));
                if sel.as_deref() == Some(value.as_str()) { out.push_str(" selected"); }
            }
            out.push('>');
            // A textarea's value is its content.
            if name == "textarea" {
                if let Some(v) = e.attr(blitz_dom::local_name!("value")) { out.push_str(&esc(v, false)); write!(out, "</textarea>").unwrap(); return; }
            }
            if VOID.contains(&name.as_str()) { return; }
            for &c in &node.children { write(doc, c, out); }
            write!(out, "</{name}>").unwrap();
        }
        _ => for &c in &node.children { write(doc, c, out); },
    }
}

fn dump(name: &str, app: fn() -> Element, style: &str) {
    let mut dom = HtmlDom::with_config(app, HtmlConfig { stylesheets: vec![style.to_string()], measurer: None, ..HtmlConfig::default() });
    dom.render();
    let doc = dom.document();
    // find <body>
    let mut html = String::from("<!doctype html>\n");
    let root = doc.root_node();
    let mut s = String::new();
    write(doc, root.id, &mut s);
    html.push_str(&s);
    std::fs::write(format!("{}/{name}.html", std::env::args().nth(1).unwrap()), html).unwrap();
}

fn main() {
    dump("article", sample_html_article::app, sample_html_article::STYLE);
    dump("chat", sample_html_chat::app, sample_html_chat::STYLE);
    dump("dashboard", sample_html_dashboard::app, sample_html_dashboard::STYLE);
    dump("gallery", sample_html_gallery::app, sample_html_gallery::STYLE);
    dump("hello", sample_html_hello::app, sample_html_hello::STYLE);
    dump("helpcenter", sample_html_helpcenter::app, sample_html_helpcenter::STYLE);
    dump("kanban", sample_html_kanban::app, sample_html_kanban::STYLE);
    dump("landing", sample_html_landing::app, sample_html_landing::STYLE);
    dump("layout", sample_html_layout::app, sample_html_layout::STYLE);
    dump("motion", sample_html_motion::app, sample_html_motion::STYLE);
    dump("overlay", sample_html_overlay::app, sample_html_overlay::STYLE);
    dump("settings", sample_html_settings::app, sample_html_settings::STYLE);
    dump("table", sample_html_table::app, sample_html_table::STYLE);
    dump("theme", sample_html_theme::app, sample_html_theme::STYLE);
    dump("todo", sample_html_todo::app, sample_html_todo::STYLE);
    dump("typography", sample_html_typography::app, sample_html_typography::STYLE);
    dump("workbench", sample_html_workbench::app, sample_html_workbench::STYLE);
}
