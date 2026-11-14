//! CSS 3D transforms as the renderer receives them: one `Transform3D` modifier on the box,
//! holding the 4x4 matrix the transform list multiplies out to, the `transform-origin` as
//! a fraction of the box, and `backface-visibility`.
//!
//! Each test runs an app through a Host the way a window runs it and reads the records the
//! renderer would receive. Pages are laid out at 800 by 600. A box is told from the others
//! by its background colour.
//!
//! The expected matrices are written here from the matrix CSS defines for each function,
//! in the order of `matrix3d()` (column by column), and multiplied in list order. They are
//! never read back from the Host.

use dioxus_compose::Host;
use dioxus_compose::html::HtmlConfig;
use dioxus_compose::html::prelude::*;
use dioxus_compose::protocol::{Mutation, decode_batch};
use dioxus_compose::schema::{Color, Modifier, Paint};

type M = [f32; 16];

const IDENTITY: M = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// `a` times `b`, both column-major: `b` is applied to a point first.
fn mul(a: M, b: M) -> M {
    let mut out = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            out[4 * column + row] = (0..4).map(|k| a[4 * k + row] * b[4 * column + k]).sum();
        }
    }
    out
}

fn product(list: &[M]) -> M {
    list.iter().fold(IDENTITY, |total, next| mul(total, *next))
}

fn rotate_x(degrees: f32) -> M {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [
        1.0, 0.0, 0.0, 0.0, 0.0, cos, sin, 0.0, 0.0, -sin, cos, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn rotate_y(degrees: f32) -> M {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [
        cos, 0.0, -sin, 0.0, 0.0, 1.0, 0.0, 0.0, sin, 0.0, cos, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn translate(x: f32, y: f32, z: f32) -> M {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0,
    ]
}

fn scale(x: f32, y: f32, z: f32) -> M {
    [
        x, 0.0, 0.0, 0.0, 0.0, y, 0.0, 0.0, 0.0, 0.0, z, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn skew_x(degrees: f32) -> M {
    let mut matrix = IDENTITY;
    matrix[4] = degrees.to_radians().tan();
    matrix
}

fn perspective(depth: f32) -> M {
    let mut matrix = IDENTITY;
    matrix[11] = -1.0 / depth;
    matrix
}

fn rgb(r: u8, g: u8, b: u8) -> Paint {
    Paint::Literal(Color::argb(
        0xff00_0000 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
    ))
}

fn config() -> HtmlConfig {
    HtmlConfig {
        stylesheets: vec!["html, body { margin: 0; padding: 0; }".to_string()],
        ..HtmlConfig::default()
    }
}

fn owned(batch: &[u8]) -> Vec<Mutation<'static>> {
    let bytes: &'static [u8] = Box::leak(batch.to_vec().into_boxed_slice());
    decode_batch(bytes).expect("the batch decodes")
}

/// The modifiers the batch gives the one node whose background is `colour`.
#[track_caller]
fn modifiers_of(batch: &[Mutation<'static>], colour: (u8, u8, u8)) -> Vec<Modifier> {
    let background = Modifier::Background(rgb(colour.0, colour.1, colour.2));
    let nodes: Vec<u32> = batch
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier {
                node_id, modifier, ..
            } if *modifier == background => Some(*node_id),
            _ => None,
        })
        .collect();
    assert_eq!(nodes.len(), 1, "one node is painted {colour:?}: {batch:#?}");
    batch
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetModifier {
                node_id, modifier, ..
            } if *node_id == nodes[0] => Some(modifier.clone()),
            _ => None,
        })
        .collect()
}

/// What a `Transform3D` modifier carries: the matrix, the origin and the backface flag.
type Sent = (M, (f32, f32), bool);

/// The one 3D transform among `modifiers`, which must hold no 2D transform beside it.
#[track_caller]
fn transform_3d(modifiers: &[Modifier]) -> Sent {
    let sent: Vec<Sent> = modifiers
        .iter()
        .filter_map(|modifier| match modifier {
            Modifier::Transform3D {
                m,
                origin_x,
                origin_y,
                backface_hidden,
            } => Some((*m, (*origin_x, *origin_y), *backface_hidden)),
            _ => None,
        })
        .collect();
    assert_eq!(sent.len(), 1, "one Transform3D modifier: {modifiers:#?}");
    assert!(
        !modifiers
            .iter()
            .any(|modifier| matches!(modifier, Modifier::Transform { .. })),
        "no 2D transform beside it: {modifiers:#?}"
    );
    sent[0]
}

#[track_caller]
fn assert_matrix(actual: M, expected: M, what: &str) {
    for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (a - e).abs() < 1e-4,
            "{what}: element {index} is {a}, expected {e}\n{actual:?}\n{expected:?}"
        );
    }
}

fn every_function() -> Element {
    rsx! {
        style {
            ".box {{ position: absolute; width: 100px; height: 50px; }}"
        }
        div { class: "box", style: "background-color: rgb(1, 0, 0); transform: rotateX(60deg)" }
        div { class: "box", style: "background-color: rgb(2, 0, 0); transform: rotateY(45deg)" }
        div { class: "box", style: "background-color: rgb(3, 0, 0); transform: rotate3d(1, 1, 0, 90deg)" }
        div { class: "box", style: "background-color: rgb(4, 0, 0); transform: translateZ(10px)" }
        div { class: "box", style: "background-color: rgb(5, 0, 0); transform: translate3d(10px, 20px, 30px)" }
        div { class: "box", style: "background-color: rgb(6, 0, 0); transform: scaleZ(2)" }
        div { class: "box", style: "background-color: rgb(7, 0, 0); transform: scale3d(2, 3, 4)" }
        div { class: "box", style: "background-color: rgb(8, 0, 0); transform: perspective(400px)" }
        div { class: "box", style: "background-color: rgb(9, 0, 0); transform: matrix3d(1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0.001, 0, 0, 0, 1)" }
        div { class: "box", style: "background-color: rgb(10, 0, 0); transform: translate(10px, 20px) rotateY(30deg) scale(2)" }
        div { class: "box", style: "background-color: rgb(11, 0, 0); transform: rotateZ(30deg)" }
        div { class: "box", style: "background-color: rgb(12, 0, 0); transform: rotateX(60deg); transform-origin: 25px 100%" }
    }
}

/// Every 3D function goes to the renderer as one `Transform3D` modifier on its box: the
/// one matrix, the origin as a fraction of the box (its centre by default), and nothing
/// else about the transform. A list that mixes 2D functions in is still one matrix.
///
/// - `rotate3d(1, 1, 0, 90deg)` turns a quarter turn about the diagonal of the plane. With
///   the unit axis (s, s, 0), s = 0.707107, the matrix CSS defines has 0.5 in its top-left
///   four entries, (-s, s) down the z row and (s, -s) down the z column, and 0 at the z
///   diagonal.
/// - `translate(10px, 20px) rotateY(30deg) scale(2)` is the product of the three, in that
///   order.
/// - `rotateZ(30deg)` is flat, so it is the 2D rotation it reduces to and goes as a 2D
///   `Transform`: (cos 30, sin 30, -sin 30, cos 30).
/// - `transform-origin: 25px 100%` on a 100 by 50 box is the fraction (0.25, 1), and the
///   matrix is the rotation itself, with nothing folded into it.
#[test]
fn fr34_1_3d_functions_fold_into_one_transform3d() {
    let mut host = Host::html(every_function, config);
    let first = owned(host.rebuild().unwrap());
    let s = std::f32::consts::FRAC_1_SQRT_2;
    let cases: [(u8, &str, M); 10] = [
        (1, "rotateX", rotate_x(60.0)),
        (2, "rotateY", rotate_y(45.0)),
        (
            3,
            "rotate3d",
            [
                0.5, 0.5, -s, 0.0, 0.5, 0.5, s, 0.0, s, -s, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
        ),
        (4, "translateZ", translate(0.0, 0.0, 10.0)),
        (5, "translate3d", translate(10.0, 20.0, 30.0)),
        (6, "scaleZ", scale(1.0, 1.0, 2.0)),
        (7, "scale3d", scale(2.0, 3.0, 4.0)),
        (8, "perspective", perspective(400.0)),
        (9, "matrix3d", {
            let mut matrix = IDENTITY;
            matrix[11] = 0.001;
            matrix
        }),
        (
            10,
            "a mixed list",
            product(&[
                translate(10.0, 20.0, 0.0),
                rotate_y(30.0),
                scale(2.0, 2.0, 1.0),
            ]),
        ),
    ];
    for (red, what, expected) in cases {
        let (m, origin, backface_hidden) = transform_3d(&modifiers_of(&first, (red, 0, 0)));
        assert_matrix(m, expected, what);
        assert_eq!(origin, (0.5, 0.5), "{what}: about the centre");
        assert!(!backface_hidden, "{what}");
    }

    let flat = modifiers_of(&first, (11, 0, 0));
    let (sin, cos) = 30.0f32.to_radians().sin_cos();
    assert!(
        flat.iter().any(|modifier| matches!(
            modifier,
            Modifier::Transform { a, b, c, d, .. }
                if (a - cos).abs() < 1e-4
                    && (b - sin).abs() < 1e-4
                    && (c + sin).abs() < 1e-4
                    && (d - cos).abs() < 1e-4
        )) && !flat
            .iter()
            .any(|modifier| matches!(modifier, Modifier::Transform3D { .. })),
        "rotateZ is the 2D rotation: {flat:#?}"
    );

    let (m, origin, _) = transform_3d(&modifiers_of(&first, (12, 0, 0)));
    assert_matrix(m, rotate_x(60.0), "a declared origin");
    assert_eq!(origin, (0.25, 1.0));
}

/// Where a box-local point is drawn by a `Transform3D` modifier on a box of `size`: the
/// matrix is applied about the origin, and the result is divided by its fourth component.
fn drawn((m, origin, _): Sent, size: (f32, f32), point: (f32, f32)) -> (f32, f32) {
    let (ox, oy) = (origin.0 * size.0, origin.1 * size.1);
    let (x, y) = (point.0 - ox, point.1 - oy);
    let w = m[3] * x + m[7] * y + m[15];
    (
        (m[0] * x + m[4] * y + m[12]) / w + ox,
        (m[1] * x + m[5] * y + m[13]) / w + oy,
    )
}

/// Where CSS draws the point (dx, dy) from the centre of a box, from that centre.
type Expected = fn(f32, f32) -> (f32, f32);

/// `rotateX(60deg)`.
fn tilted(dx: f32, dy: f32) -> (f32, f32) {
    (dx, dy * 60.0f32.to_radians().cos())
}

/// `rotateY(45deg)`.
fn turned(dx: f32, dy: f32) -> (f32, f32) {
    (dx * 45.0f32.to_radians().cos(), dy)
}

/// `perspective(400px) rotateY(45deg)`.
fn turned_in_perspective(dx: f32, dy: f32) -> (f32, f32) {
    let (s45, c45) = 45.0f32.to_radians().sin_cos();
    let w = 1.0 + dx * s45 / 400.0;
    (dx * c45 / w, dy / w)
}

/// `perspective(400px) rotateX(60deg) rotateY(45deg)`.
fn tumbled_in_perspective(dx: f32, dy: f32) -> (f32, f32) {
    let (s45, c45) = 45.0f32.to_radians().sin_cos();
    let (s60, c60) = 60.0f32.to_radians().sin_cos();
    let z = dy * s60 - dx * s45 * c60;
    let w = 1.0 - z / 400.0;
    (dx * c45 / w, (dy * c60 + dx * s45 * s60) / w)
}

fn corners() -> Element {
    rsx! {
        style {
            ".box {{ position: absolute; width: 200px; height: 100px; }}"
        }
        div { class: "box", style: "background-color: rgb(1, 0, 0); transform: rotateX(60deg)" }
        div { class: "box", style: "background-color: rgb(2, 0, 0); transform: rotateY(45deg)" }
        div { class: "box", style: "background-color: rgb(3, 0, 0); transform: perspective(400px) rotateY(45deg)" }
        div { class: "box", style: "background-color: rgb(4, 0, 0); transform: perspective(400px) rotateX(60deg) rotateY(45deg)" }
    }
}

/// The four corners of a box under a 3D transform are drawn where CSS puts them, within a
/// pixel. Each box is 200 by 100, so a corner is (dx, dy) = (+-100, +-50) from the centre
/// the transform is applied about, and the expected places are worked out from what each
/// function does to that point:
///
/// - `rotateX(60deg)`: x stays, y goes to dy cos 60 = +-25.
/// - `rotateY(45deg)`: x goes to dx cos 45 = +-70.71, y stays.
/// - `perspective(400px) rotateY(45deg)`: the rotation sends the point to depth
///   -dx sin 45, and the perspective divides by w = 1 + dx sin 45 / 400. The right corners
///   (w = 1.17678) are at x = 60.09, y = +-42.49; the left ones (w = 0.82322) at
///   x = -85.89, y = +-60.74.
/// - `perspective(400px) rotateX(60deg) rotateY(45deg)`: after `rotateY` the point is
///   (dx cos 45, dy, -dx sin 45); `rotateX` then gives y = dy cos 60 + dx sin 45 sin 60 and
///   depth z = dy sin 60 - dx sin 45 cos 60; the perspective divides by w = 1 - z / 400.
///
/// The perspective travels inside the matrix: the bottom row of the third box's matrix
/// holds sin 45 / 400 under x and -cos 45 / 400 under z, and no other record carries it.
#[test]
fn fr34_1_3d_corners_match_css_within_1px() {
    let mut host = Host::html(corners, config);
    let first = owned(host.rebuild().unwrap());
    let size = (200.0, 100.0);
    let expectations: [(u8, &str, Expected); 4] = [
        (1, "rotateX", tilted),
        (2, "rotateY", turned),
        (3, "perspective rotateY", turned_in_perspective),
        (4, "perspective rotateX rotateY", tumbled_in_perspective),
    ];
    for (red, what, expected) in expectations {
        let sent = transform_3d(&modifiers_of(&first, (red, 0, 0)));
        for (dx, dy) in [
            (-100.0, -50.0),
            (100.0, -50.0),
            (100.0, 50.0),
            (-100.0, 50.0),
        ] {
            let (ex, ey) = expected(dx, dy);
            let (x, y) = drawn(sent, size, (100.0 + dx, 50.0 + dy));
            assert!(
                (x - (100.0 + ex)).abs() < 0.05 && (y - (50.0 + ey)).abs() < 0.05,
                "{what}: the corner ({dx}, {dy}) is drawn at ({x}, {y}), expected ({}, {})",
                100.0 + ex,
                50.0 + ey
            );
        }
    }

    let (s45, c45) = 45.0f32.to_radians().sin_cos();
    let (m, _, _) = transform_3d(&modifiers_of(&first, (3, 0, 0)));
    assert!(
        (m[3] - s45 / 400.0).abs() < 1e-6 && (m[11] + c45 / 400.0).abs() < 1e-6,
        "the perspective is inside the matrix: {m:?}"
    );
}

fn undecomposable() -> Element {
    rsx! {
        style {
            ".box {{ position: absolute; width: 200px; height: 100px; }}"
        }
        div { class: "box", style: "background-color: rgb(1, 0, 0); transform: perspective(400px) skewX(20deg) rotateY(30deg)" }
        div { class: "box", style: "background-color: rgb(2, 0, 0); transform: matrix3d(1, 0.2, 0, 0.001, 0.3, 1, 0, 0.002, 0, 0, 1, 0, 10, 20, 0, 1)" }
    }
}

/// A matrix that no single set of rotations, scales and a camera distance reproduces (a
/// skew together with a perspective) is sent as it is: the Host does not take it apart,
/// round it or replace it with something close. Drawing it is the renderer's.
///
/// - `perspective(400px) skewX(20deg) rotateY(30deg)` is the product of the three matrices
///   in that order, with tan 20 in the skew.
/// - A `matrix3d()` with a shear and two perspective terms arrives with the sixteen
///   numbers it was written with, in the order it was written.
#[test]
fn fr34_1_undecomposable_3d_matrix_is_sent_unchanged() {
    let mut host = Host::html(undecomposable, config);
    let first = owned(host.rebuild().unwrap());

    let (m, origin, _) = transform_3d(&modifiers_of(&first, (1, 0, 0)));
    let expected = product(&[perspective(400.0), skew_x(20.0), rotate_y(30.0)]);
    for (index, (a, e)) in m.iter().zip(expected).enumerate() {
        assert!((a - e).abs() < 1e-5, "element {index} is {a}, expected {e}");
    }
    assert_eq!(origin, (0.5, 0.5));

    let (m, _, _) = transform_3d(&modifiers_of(&first, (2, 0, 0)));
    assert_eq!(
        m,
        [
            1.0, 0.2, 0.0, 0.001, 0.3, 1.0, 0.0, 0.002, 0.0, 0.0, 1.0, 0.0, 10.0, 20.0, 0.0, 1.0
        ]
    );
}

fn faces() -> Element {
    rsx! {
        style {
            ".box {{ position: absolute; width: 100px; height: 50px; }}"
        }
        div { class: "box", style: "background-color: rgb(1, 0, 0); transform: rotateY(180deg); backface-visibility: hidden" }
        div { class: "box", style: "background-color: rgb(2, 0, 0); transform: rotateY(180deg)" }
    }
}

/// `backface-visibility: hidden` reaches the renderer as the flag of the box's
/// `Transform3D`, beside the matrix that turns its back to the viewer, so the renderer can
/// leave the box out. Without the property the flag is off and the box is drawn mirrored.
#[test]
fn fr34_1_backface_hidden_reaches_the_renderer_with_the_matrix() {
    let mut host = Host::html(faces, config);
    let first = owned(host.rebuild().unwrap());

    let (m, _, hidden) = transform_3d(&modifiers_of(&first, (1, 0, 0)));
    assert!(hidden);
    assert_matrix(m, rotate_y(180.0), "rotateY(180deg)");

    let (m, _, hidden) = transform_3d(&modifiers_of(&first, (2, 0, 0)));
    assert!(!hidden);
    assert_matrix(m, rotate_y(180.0), "rotateY(180deg)");
}

fn under_a_perspective() -> Element {
    rsx! {
        style {
            ".stage {{ position: absolute; left: 0; top: 0; width: 400px; height: 200px; }}
             .card {{ position: absolute; left: 50px; top: 20px; width: 200px; height: 100px; }}"
        }
        div { class: "stage", style: "perspective: 400px",
            div { class: "card", style: "background-color: rgb(1, 0, 0); transform: rotateY(45deg)" }
        }
        div { class: "stage", style: "perspective: 400px; perspective-origin: 25% 150px",
            div { class: "card", style: "background-color: rgb(2, 0, 0); transform: rotateY(45deg)" }
        }
        div { class: "stage",
            div { class: "card", style: "background-color: rgb(3, 0, 0); transform: rotateY(45deg)" }
        }
        div { class: "stage", style: "perspective: none; perspective-origin: 25% 150px",
            div { class: "card", style: "background-color: rgb(4, 0, 0); transform: rotateY(45deg)" }
        }
        div { class: "stage", style: "perspective: 400px",
            div { class: "card", style: "background-color: rgb(5, 0, 0); transform: rotate(30deg)" }
        }
        div { class: "stage", style: "perspective: 400px",
            div { class: "card",
                div { class: "card", style: "background-color: rgb(6, 0, 0); transform: rotateY(45deg)" }
            }
        }
    }
}

/// Where CSS draws the point (dx, dy) from the centre of a card under `rotateY(45deg)`,
/// from that centre, when its parent has `perspective: 400px` and the perspective origin
/// is (px, py) from the card's centre.
///
/// The rotation sends the point to (dx cos 45, dy, -dx sin 45). The perspective then
/// divides its distance from the perspective origin by w = 1 - z / 400 = 1 + dx sin 45 /
/// 400: x = (dx cos 45 - px) / w + px and y = (dy - py) / w + py.
fn turned_under_parent(dx: f32, dy: f32, (px, py): (f32, f32)) -> (f32, f32) {
    let (s45, c45) = 45.0f32.to_radians().sin_cos();
    let w = 1.0 + dx * s45 / 400.0;
    ((dx * c45 - px) / w + px, (dy - py) / w + py)
}

/// A parent's `perspective` property reaches its child's 3D transform inside the child's
/// one matrix, in front of the child's own functions, and nothing else carries it.
///
/// Each stage is 400 by 200 at the page's corner and each card is 200 by 100 at (50, 20)
/// inside it, so a card's centre, which its transform is applied about, is at (150, 70)
/// of the stage.
///
/// - `perspective: 400px` with the default origin, the stage's centre (200, 100): the
///   origin is (50, 30) from the card's centre, and the matrix is
///   translate(50, 30) perspective(400) translate(-50, -30) rotateY(45deg). The right
///   corners (dx = 100, w = 1.17678) are drawn at x = (70.71 - 50) / w + 50 = 67.60 and
///   y = (-50 - 30) / w + 30 = -37.98 and (50 - 30) / w + 30 = 47.00 from the centre; the
///   left ones (dx = -100, w = 0.82322) at x = -120.71 / w + 50 = -96.63 and y = -67.18
///   and 54.29.
/// - `perspective-origin: 25% 150px` is (100, 150) of the stage, (-50, 80) from the
///   card's centre, and the matrix is
///   translate(-50, 80) perspective(400) translate(50, -80) rotateY(45deg). The right
///   corners are drawn at x = (70.71 + 50) / 1.17678 - 50 = 52.58, not 67.60.
/// - A stage without the property, and one with `perspective: none` (whatever its
///   `perspective-origin`), leave the card's matrix the rotation alone.
/// - A card that only turns in the plane, `rotate(30deg)`, lies where a perspective moves
///   nothing: it keeps the 2D `Transform` it has anywhere else.
/// - A perspective is for the stage's own children. A card inside a card is not one, so
///   its matrix is the rotation alone.
#[test]
fn fr34_1_parent_perspective_property_folds_into_the_child_matrix() {
    let mut host = Host::html(under_a_perspective, config);
    let first = owned(host.rebuild().unwrap());
    let size = (200.0, 100.0);

    let cases: [(u8, &str, (f32, f32)); 2] = [
        (1, "the default perspective origin", (50.0, 30.0)),
        (2, "perspective-origin: 25% 150px", (-50.0, 80.0)),
    ];
    for (red, what, (px, py)) in cases {
        let sent = transform_3d(&modifiers_of(&first, (red, 0, 0)));
        let (m, origin, backface_hidden) = sent;
        assert_matrix(
            m,
            product(&[
                translate(px, py, 0.0),
                perspective(400.0),
                translate(-px, -py, 0.0),
                rotate_y(45.0),
            ]),
            what,
        );
        assert_eq!(origin, (0.5, 0.5), "{what}: about the card's own centre");
        assert!(!backface_hidden, "{what}");
        for (dx, dy) in [
            (-100.0, -50.0),
            (100.0, -50.0),
            (100.0, 50.0),
            (-100.0, 50.0),
        ] {
            let (ex, ey) = turned_under_parent(dx, dy, (px, py));
            let (x, y) = drawn(sent, size, (100.0 + dx, 50.0 + dy));
            assert!(
                (x - (100.0 + ex)).abs() < 0.05 && (y - (50.0 + ey)).abs() < 0.05,
                "{what}: the corner ({dx}, {dy}) is drawn at ({x}, {y}), expected ({}, {})",
                100.0 + ex,
                50.0 + ey
            );
        }
    }

    // The numbers of the doc comment, so the expectation does not rest on the helper alone.
    let default_origin = transform_3d(&modifiers_of(&first, (1, 0, 0)));
    for (corner, expected) in [
        ((200.0, 0.0), (167.60, 12.02)),
        ((200.0, 100.0), (167.60, 97.00)),
        ((0.0, 0.0), (3.37, -17.18)),
        ((0.0, 100.0), (3.37, 104.29)),
    ] {
        let (x, y) = drawn(default_origin, size, corner);
        assert!(
            (x - expected.0).abs() < 1.0 && (y - expected.1).abs() < 1.0,
            "the corner {corner:?} is drawn at ({x}, {y}), expected {expected:?}"
        );
    }
    let moved_origin = transform_3d(&modifiers_of(&first, (2, 0, 0)));
    let (x, _) = drawn(moved_origin, size, (200.0, 0.0));
    assert!(
        (x - 152.58).abs() < 0.05,
        "a perspective origin to the left draws the right edge at {x}, expected 152.58"
    );

    for (red, what) in [
        (3, "no perspective"),
        (4, "perspective: none"),
        (6, "a grandchild of the perspective"),
    ] {
        let (m, origin, _) = transform_3d(&modifiers_of(&first, (red, 0, 0)));
        assert_matrix(m, rotate_y(45.0), what);
        assert_eq!(m[3], 0.0, "{what}: no perspective term");
        assert_eq!(m[7], 0.0, "{what}: no perspective term");
        assert_eq!(m[11], 0.0, "{what}: no perspective term");
        assert_eq!(origin, (0.5, 0.5), "{what}");
    }
    let (plain, _, _) = transform_3d(&modifiers_of(&first, (3, 0, 0)));
    let (none, _, _) = transform_3d(&modifiers_of(&first, (4, 0, 0)));
    assert_eq!(plain, none, "perspective: none changes nothing");

    let flat = modifiers_of(&first, (5, 0, 0));
    let (sin, cos) = 30.0f32.to_radians().sin_cos();
    assert!(
        flat.iter().any(|modifier| matches!(
            modifier,
            Modifier::Transform { a, b, c, d, .. }
                if (a - cos).abs() < 1e-4
                    && (b - sin).abs() < 1e-4
                    && (c + sin).abs() < 1e-4
                    && (d - cos).abs() < 1e-4
        )) && !flat
            .iter()
            .any(|modifier| matches!(modifier, Modifier::Transform3D { .. })),
        "a 2D transform under a perspective stays the 2D transform: {flat:#?}"
    );
}
