//! Turns a box's computed `transform` into a 2D transform.
//!
//! The functions are the same pieces a browser multiplies: the individual
//! `translate`, `rotate` and `scale` properties first, in that order, then the `transform`
//! list. Percentages in a translation resolve against the box's own border box, and the
//! point the matrix is applied about is `transform-origin`.
//!
//! A transform that really is 3D (a rotation about x or y, a perspective, a `matrix3d()`
//! that is not flat) has no 2D form, and drawing a 2D guess of it would put the box
//! somewhere no browser does. Such a transform is reported as [`Moved::ThreeD`] so the
//! caller can leave it out and say so.

use style::properties::ComputedValues;
use style::values::computed::Length;
use style::values::computed::transform::TransformOperation;
use style::values::generics::transform::{
    GenericRotate, GenericScale, GenericTransformOperation as Operation, GenericTranslate,
};
use style_traits::ToCss;

use crate::paint::display_list::{Matrix2D, Transform2D, TransformFunction};

/// What a box's `transform` asks of its drawing.
pub(crate) enum Moved {
    /// No transform, or one that leaves every point where it is.
    Nothing,
    /// A translation and nothing else: it changes where the box is drawn and nothing
    /// about its shape, so it is folded into the box's position.
    Translated(f32, f32),
    /// Any other 2D transform.
    Matrix(Transform2D),
    /// A 3D transform, as CSS writes it. It is not applied.
    ThreeD(String),
}

/// How `style`'s transform moves a box of `width` by `height`.
///
/// With `keep` set a translation is returned as a [`Moved::Matrix`] too. A renderer that
/// plays a transition of `transform` needs the translation as a transform, not folded into
/// a position it would have to animate separately.
pub(crate) fn moved(style: &ComputedValues, width: f32, height: f32, keep: bool) -> Moved {
    let Some(functions) = functions_of(style, width, height) else {
        return Moved::ThreeD(describe(style));
    };
    let transform = Transform2D::new(functions, origin_of(style, width, height));
    // A transform a renderer plays an animation of is kept whole, even where it comes to
    // nothing: `rotate(360deg)` is the identity matrix and still a turn to animate to.
    if keep && !transform.functions.is_empty() {
        return Moved::Matrix(transform);
    }
    if transform.matrix.is_identity() {
        return Moved::Nothing;
    }
    if transform.matrix.is_translation() {
        return Moved::Translated(transform.matrix.e, transform.matrix.f);
    }
    Moved::Matrix(transform)
}

/// The functions of a box's `translate`, `rotate`, `scale` and `transform`, in the order a
/// browser applies them. `None` when any of them is 3D.
pub(crate) fn functions_of(
    style: &ComputedValues,
    width: f32,
    height: f32,
) -> Option<Vec<TransformFunction>> {
    let boxed = style.get_box();
    let mut functions = Vec::new();

    match &boxed.translate {
        GenericTranslate::None => {}
        GenericTranslate::Translate(x, y, z) => {
            if z.px() != 0.0 {
                return None;
            }
            functions.push(TransformFunction::Translate(
                x.resolve(Length::new(width)).px(),
                y.resolve(Length::new(height)).px(),
            ));
        }
    }
    match &boxed.rotate {
        GenericRotate::None => {}
        GenericRotate::Rotate(angle) => {
            functions.push(TransformFunction::Rotate(angle.radians()));
        }
        GenericRotate::Rotate3D(x, y, z, angle) => {
            if let Some(function) = axis_rotation(*x, *y, *z, angle.radians())? {
                functions.push(function);
            }
        }
    }
    match &boxed.scale {
        GenericScale::None => {}
        GenericScale::Scale(x, y, z) => {
            if *z != 1.0 {
                return None;
            }
            functions.push(TransformFunction::Scale(*x, *y));
        }
    }
    push_list(&mut functions, &boxed.transform.0, width, height)?;
    Some(functions)
}

/// `transform-origin` in CSS pixels from the box's top-left corner.
pub(crate) fn origin_of(style: &ComputedValues, width: f32, height: f32) -> (f32, f32) {
    let origin = &style.get_box().transform_origin;
    (
        origin.horizontal.resolve(Length::new(width)).px(),
        origin.vertical.resolve(Length::new(height)).px(),
    )
}

/// The transform as CSS writes it, for a warning.
fn describe(style: &ComputedValues) -> String {
    let boxed = style.get_box();
    let mut parts = Vec::new();
    for (name, text) in [
        ("translate", boxed.translate.to_css_string()),
        ("rotate", boxed.rotate.to_css_string()),
        ("scale", boxed.scale.to_css_string()),
        ("transform", boxed.transform.to_css_string()),
    ] {
        if text != "none" {
            parts.push(format!("{name}: {text}"));
        }
    }
    parts.join("; ")
}

/// `rotate3d(x, y, z, angle)`: flat only when the axis is the z axis, or the angle is
/// nothing so that nothing turns. The outer `None` is "3D"; the inner one is "no function
/// needed".
fn axis_rotation(x: f32, y: f32, z: f32, radians: f32) -> Option<Option<TransformFunction>> {
    if radians == 0.0 {
        return Some(None);
    }
    if x == 0.0 && y == 0.0 && z != 0.0 {
        return Some(Some(TransformFunction::Rotate(radians * z.signum())));
    }
    None
}

/// Appends the functions of a `transform` list. `None` when one of them is 3D.
fn push_list(
    out: &mut Vec<TransformFunction>,
    ops: &[TransformOperation],
    width: f32,
    height: f32,
) -> Option<()> {
    let across =
        |value: &style::values::computed::LengthPercentage| value.resolve(Length::new(width)).px();
    let down =
        |value: &style::values::computed::LengthPercentage| value.resolve(Length::new(height)).px();
    for op in ops {
        let function = match op {
            Operation::Matrix(m) => Some(TransformFunction::Matrix(Matrix2D {
                a: m.a,
                b: m.b,
                c: m.c,
                d: m.d,
                e: m.e,
                f: m.f,
            })),
            Operation::Matrix3D(m) => {
                let flat = (*m).into_2d().ok()?;
                Some(TransformFunction::Matrix(Matrix2D {
                    a: flat.a,
                    b: flat.b,
                    c: flat.c,
                    d: flat.d,
                    e: flat.e,
                    f: flat.f,
                }))
            }
            Operation::Skew(x, y) => Some(TransformFunction::Skew(x.radians(), y.radians())),
            Operation::SkewX(x) => Some(TransformFunction::Skew(x.radians(), 0.0)),
            Operation::SkewY(y) => Some(TransformFunction::Skew(0.0, y.radians())),
            Operation::Translate(x, y) => Some(TransformFunction::Translate(across(x), down(y))),
            Operation::TranslateX(x) => Some(TransformFunction::Translate(across(x), 0.0)),
            Operation::TranslateY(y) => Some(TransformFunction::Translate(0.0, down(y))),
            Operation::TranslateZ(z) => {
                if z.px() != 0.0 {
                    return None;
                }
                None
            }
            Operation::Translate3D(x, y, z) => {
                if z.px() != 0.0 {
                    return None;
                }
                Some(TransformFunction::Translate(across(x), down(y)))
            }
            Operation::Scale(x, y) => Some(TransformFunction::Scale(*x, *y)),
            Operation::ScaleX(x) => Some(TransformFunction::Scale(*x, 1.0)),
            Operation::ScaleY(y) => Some(TransformFunction::Scale(1.0, *y)),
            Operation::ScaleZ(z) => {
                if *z != 1.0 {
                    return None;
                }
                None
            }
            Operation::Scale3D(x, y, z) => {
                if *z != 1.0 {
                    return None;
                }
                Some(TransformFunction::Scale(*x, *y))
            }
            Operation::Rotate(angle) | Operation::RotateZ(angle) => {
                Some(TransformFunction::Rotate(angle.radians()))
            }
            Operation::RotateX(angle) | Operation::RotateY(angle) => {
                if angle.radians() != 0.0 {
                    return None;
                }
                None
            }
            Operation::Rotate3D(x, y, z, angle) => axis_rotation(*x, *y, *z, angle.radians())?,
            Operation::Perspective(_) => return None,
            // The two operations stylo makes up while it interpolates between lists that do
            // not line up. Interpolating the matrices component by component is not what a
            // browser does (it decomposes them), but it keeps the ends right.
            Operation::InterpolateMatrix {
                from_list,
                to_list,
                progress,
            } => {
                let from = list_matrix(&from_list.0, width, height)?;
                let to = list_matrix(&to_list.0, width, height)?;
                Some(TransformFunction::Matrix(from.lerp(&to, progress.0)))
            }
            Operation::AccumulateMatrix { .. } => None,
        };
        out.extend(function);
    }
    Some(())
}

fn list_matrix(ops: &[TransformOperation], width: f32, height: f32) -> Option<Matrix2D> {
    let mut functions = Vec::new();
    push_list(&mut functions, ops, width, height)?;
    Some(
        functions
            .iter()
            .fold(Matrix2D::IDENTITY, |total, function| {
                total.times(&function.matrix())
            }),
    )
}
