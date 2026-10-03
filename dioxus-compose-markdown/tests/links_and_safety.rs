//! Links report and open nothing; untrusted input is shown, never run or fetched; the
//! theme belongs to the Renderer.

mod support;

use dioxus_compose::protocol::{HostEvent, decode_batch};
use dioxus_compose::schema::{DesignSystem, EventPayload};
use dioxus_compose::spans::TextSpans;
use dioxus_compose::WidgetKind;
use dioxus_compose_markdown::{ImageResolver, MarkdownOptions};
use support::Screen;
use support::apps::{self, quiet};

const LINKED: &str =
    "Read [the docs](https://example.com/docs) or the [reference][r], plainly.\n\n[r]: https://example.com/ref\n";

fn paragraph(screen: &Screen) -> u32 {
    screen
        .tree
        .text_node("Read the docs or the reference, plainly.")
        .unwrap_or_else(|| panic!("no paragraph:\n{}", screen.tree.dump()))
}

/// A press on a link run reports that link's address once, a reference link reports the
/// address its definition gave, and the words around the links report nothing.
#[test]
fn fr37_a_link_reports_its_address_once() {
    apps::reset(LINKED, quiet());
    let mut screen = Screen::new(apps::one_shot);
    let node = paragraph(&screen);
    let runs: Vec<_> = TextSpans::from_bytes(screen.tree.nodes[&node].spans.clone())
        .spans()
        .collect();
    let linked: Vec<_> = runs.iter().filter_map(|run| run.on_click).collect();
    assert_eq!(linked.len(), 2, "two links, two runs that report: {runs:?}");
    let text = screen.tree.nodes[&node].text.clone().unwrap();
    let plain: Vec<_> = runs
        .iter()
        .filter(|run| run.on_click.is_none())
        .map(|run| &text[run.start as usize..(run.start + run.length) as usize])
        .collect();
    assert!(plain.is_empty(), "words outside a link report a press: {plain:?}");

    screen.click(node, linked[0]);
    assert_eq!(apps::links(), vec!["https://example.com/docs".to_owned()]);
    screen.click(node, linked[1]);
    assert_eq!(
        apps::links(),
        vec![
            "https://example.com/docs".to_owned(),
            "https://example.com/ref".to_owned()
        ]
    );
}

/// Without `on_link` a link is underlined words, and pressing it can report nothing
/// because no run carries anything to report.
#[test]
fn fr37_without_on_link_a_link_is_inert() {
    apps::reset(LINKED, quiet());
    apps::without_link_handler();
    let screen = Screen::new(apps::one_shot);
    let node = paragraph(&screen);
    let runs: Vec<_> = TextSpans::from_bytes(screen.tree.nodes[&node].spans.clone())
        .spans()
        .collect();
    assert_eq!(runs.len(), 2);
    assert!(runs.iter().all(|run| run.underline && run.on_click.is_none()));
}

/// Raw HTML, block and inline, is shown as the characters that were written.
#[test]
fn fr37_raw_html_is_shown_as_written() {
    let source = "<script>alert(1)</script>\n\n<iframe src=\"https://example.com\"></iframe>\n\nText with <img src=x onerror=alert(1)> inline.\n";
    apps::reset(source, quiet());
    let screen = Screen::new(apps::one_shot);
    for written in [
        "<script>alert(1)</script>",
        "<iframe src=\"https://example.com\"></iframe>",
        "Text with <img src=x onerror=alert(1)> inline.",
    ] {
        assert!(
            screen.tree.text_node(written).is_some(),
            "{written:?} is not shown as written:\n{}",
            screen.tree.dump()
        );
    }
}

/// An image is its description and its address unless the application resolves it, and
/// nothing is registered or fetched on the way.
#[test]
fn fr37_images_are_words_unless_the_application_resolves_them() {
    let source = "A ![cat](https://example.com/cat.png) and ![dot](data:image/png;base64,AAAA).\n";
    apps::reset(source, quiet());
    let screen = Screen::new(apps::one_shot);
    let node = screen
        .tree
        .text_node(
            "A cat (https://example.com/cat.png) and dot (data:image/png;base64,AAAA).",
        )
        .unwrap_or_else(|| panic!("the images are not words:\n{}", screen.tree.dump()));
    let text = screen.tree.nodes[&node].text.clone().unwrap();
    let links: Vec<_> = TextSpans::from_bytes(screen.tree.nodes[&node].spans.clone())
        .spans()
        .filter(|run| run.on_click.is_some())
        .map(|run| text[run.start as usize..(run.start + run.length) as usize].to_owned())
        .collect();
    assert_eq!(
        links,
        vec![
            "https://example.com/cat.png".to_owned(),
            "data:image/png;base64,AAAA".to_owned()
        ]
    );
    assert!(screen.tree.find(|node| node.widget == WidgetKind::Image).is_empty());

    let options = MarkdownOptions {
        image_resolver: Some(ImageResolver::new(|url| {
            (url == "https://example.com/cat.png").then_some(42)
        })),
        ..quiet()
    };
    apps::reset(source, options);
    let screen = Screen::new(apps::one_shot);
    let images = screen.tree.find(|node| node.widget == WidgetKind::Image);
    assert_eq!(images.len(), 1, "{}", screen.tree.dump());
    assert!(
        screen.tree.dump().contains("Image asset=42"),
        "{}",
        screen.tree.dump()
    );
    // The image the resolver declined is still its words.
    assert!(
        screen
            .tree
            .find(|node| node
                .text
                .as_deref()
                .is_some_and(|text| text.contains("data:image/png;base64,AAAA")))
            .len()
            == 1
    );
}

/// When the Renderer resolves a different design system, which is what a theme or scheme
/// change amounts to on this side, the document sends nothing at all.
#[test]
fn fr37_a_theme_change_costs_the_document_nothing() {
    // Colouring off, so a highlighter answer landing during the switch cannot be
    // mistaken for a reaction to it.
    apps::reset(&support::fixture("streaming.md"), quiet());
    let mut screen = Screen::new(apps::one_shot);
    for system in [DesignSystem::Fluent, DesignSystem::Cupertino, DesignSystem::Material3] {
        let (batch, _) = screen
            .host
            .dispatch(HostEvent {
                node_id: 0,
                handler_id: 0,
                payload: EventPayload::DesignSystemResolved(system),
            })
            .expect("the design system change failed");
        let mutations = decode_batch(batch).expect("decode");
        assert!(
            mutations.is_empty(),
            "switching to {system:?} sent {mutations:?}"
        );
    }
}
