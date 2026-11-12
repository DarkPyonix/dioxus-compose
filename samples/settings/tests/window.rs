//! The page the window opens: the stylesheet the window hands over, and a first frame
//! that reaches the renderer.

use dioxus_compose::Host;
use dioxus_compose::protocol::{Mutation, decode_batch};
use dioxus_compose::schema::WidgetKind;
use sample_html_settings as sample;

/// The window's page is the one the tests lay out: it carries `STYLE`, and its first
/// batch draws text through the bridge.
#[test]
fn fr34_settings_window_draws_the_page_through_the_bridge() {
    assert_eq!(
        sample::config().stylesheets,
        vec![sample::STYLE.to_string()]
    );
    let mut host = Host::html(sample::app, sample::config);
    let first = decode_batch(host.rebuild().unwrap()).unwrap();
    assert!(
        first.iter().any(|record| matches!(
            record,
            Mutation::Create {
                widget: WidgetKind::Text,
                ..
            }
        )),
        "the first frame draws the page's text"
    );
}
