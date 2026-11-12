//! A transform the renderer animates turns about the box's `transform-origin`, the same
//! point a static transform turns about.
//!
//! compose-rust plays a transform animation into the node's `Transform` modifier and draws
//! the animated matrix about that modifier's origin. So the origin the modifier carries is
//! the one every keyframe turns about, and it has to be the box's `transform-origin` (by
//! default its centre), not the top-left corner.
//!
//! Each test runs an app through a Host the way a window runs it and reads the records the
//! renderer would receive. Pages are laid out at 800 by 600.

use dioxus_compose::Host;
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::html::prelude::*;
use dioxus_compose::protocol::{Mutation, decode_batch};
use dioxus_compose::schema::{AnimatedProperty, Modifier};

const CSS: &str = r#"
html, body { margin: 0; padding: 0; }
@keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
#centre { width: 100px; height: 50px; animation: spin 1s linear infinite; }
#corner { width: 100px; height: 50px; transform-origin: 25px 100%; animation: spin 1s linear infinite; }
#still { width: 100px; height: 40px; transform: rotate(90deg); }
"#;

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec![CSS.to_string()],
        ..HtmlConfig::default()
    }
}

fn owned(batch: &[u8]) -> Vec<Mutation<'static>> {
    let bytes: &'static [u8] = Box::leak(batch.to_vec().into_boxed_slice());
    decode_batch(bytes).expect("the batch decodes")
}

/// The last `Transform` modifier the batch gives `node`, as `[a, b, c, d, e, f, origin_x,
/// origin_y]`.
fn transform_of(batch: &[Mutation<'static>], node: u32) -> Option<[f32; 8]> {
    batch.iter().rev().find_map(|mutation| match mutation {
        Mutation::SetModifier {
            node_id,
            modifier:
                Modifier::Transform {
                    a,
                    b,
                    c,
                    d,
                    e,
                    f,
                    origin_x,
                    origin_y,
                },
            ..
        } if *node_id == node => Some([*a, *b, *c, *d, *e, *f, *origin_x, *origin_y]),
        _ => None,
    })
}

fn close(actual: [f32; 8], expected: [f32; 8]) -> bool {
    actual
        .iter()
        .zip(expected.iter())
        .all(|(a, e)| (a - e).abs() < 1e-4)
}

/// The one transform animation the batch starts, with the `Transform` modifier its node
/// carries.
#[track_caller]
fn spun(app: fn() -> Element) -> ((f32, f32), [f32; 8]) {
    let mut host = Host::html(app, config);
    let first = owned(host.rebuild().unwrap());
    let animations: Vec<_> = first
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::StartAnimation(animation)
                if animation.property == AnimatedProperty::Transform =>
            {
                Some(animation.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(animations.len(), 1, "{first:#?}");
    let animation = &animations[0];
    let modifier = transform_of(&first, animation.node_id)
        .unwrap_or_else(|| panic!("the box has a transform to play on: {first:#?}"));
    ((animation.origin_x, animation.origin_y), modifier)
}

fn centre() -> Element {
    rsx! { div { id: "centre" } }
}

fn corner() -> Element {
    rsx! { div { id: "corner" } }
}

fn still() -> Element {
    rsx! { div { id: "still", style: "background-color: rgb(255, 0, 0)" } }
}

/// `samples/motion`'s spinner: a `@keyframes` rotation with no `transform-origin` turns
/// about the box's centre, so the modifier the renderer plays it into is about (50%, 50%).
#[test]
fn fr34_2_a_keyframes_rotation_turns_about_the_default_transform_origin() {
    let (origin, modifier) = spun(centre);
    assert_eq!(origin, (0.5, 0.5));
    assert!(
        close(modifier, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5, 0.5]),
        "the identity about the centre, not the top-left corner: {modifier:?}"
    );
}

/// A declared `transform-origin` is where a keyframe rotation turns, as a fraction of the
/// box: 25px of 100px across and 100% down.
#[test]
fn fr34_2_a_keyframes_rotation_turns_about_a_declared_transform_origin() {
    let (origin, modifier) = spun(corner);
    assert_eq!(origin, (0.25, 1.0));
    assert!(
        close(modifier, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.25, 1.0]),
        "{modifier:?}"
    );
}

/// A static transform is sent about the same origin an animation of it would turn about:
/// `rotate(90deg)` of a 100 by 40 box goes as the rotation itself about (50%, 50%), with
/// nothing folded into its translation.
#[test]
fn fr34_1_a_static_transform_is_sent_about_its_transform_origin() {
    let mut host = Host::html(still, config);
    let first = owned(host.rebuild().unwrap());
    let sent: Vec<[f32; 8]> = first
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier { node_id, .. } => transform_of(&first, *node_id),
            _ => None,
        })
        .collect();
    assert!(!sent.is_empty(), "{first:#?}");
    assert!(
        close(sent[0], [0.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.5, 0.5]),
        "{:?}",
        sent[0]
    );
}
