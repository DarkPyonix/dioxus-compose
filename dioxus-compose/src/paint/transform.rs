//! Turns a box's computed `transform` into a 2D or a 3D transform.
//!
//! The functions are the same pieces a browser multiplies: the individual
//! `translate`, `rotate` and `scale` properties first, in that order, then the `transform`
//! list. Percentages in a translation resolve against the box's own border box, and the
//! point the matrix is applied about is `transform-origin`.
//!
//! A transform that really is 3D (a rotation about x or y, a move or a scale in depth, a
//! perspective, a `matrix3d()` that is not flat) has no 2D form, and drawing a 2D guess of
//! it would put the box somewhere no browser does. Its functions, the 2D ones among them,
//! are multiplied into one 4x4 matrix instead, with the perspective inside it, and it is
//! reported as [`Moved::ThreeD`]. A 3D function whose arguments leave it flat
//! (`translateZ(0)`, `scale3d(2, 3, 1)`, `rotate3d(0, 0, 1, 30deg)`, `rotateZ()`) is the
//! 2D function it reduces to.
//!
//! A parent's `perspective` property reaches a child's 3D transform through
//! [`under_perspective`], which multiplies it in front of the child's matrix. A child whose
//! transform is 2D lies in the parent's plane, where a perspective moves nothing, so it
//! keeps its 2D transform.

use style::properties::ComputedValues;
use style::values::computed::Length;
use style::values::computed::transform::TransformOperation;
use style::values::generics::transform::{
    GenericRotate, GenericScale, GenericTransformOperation as Operation, GenericTranslate,
};

use crate::paint::display_list::{Matrix2D, Matrix3D, Transform2D, Transform3D, TransformFunction};

/// What a box's `transform` asks of its drawing.
pub(crate) enum Moved {
    /// No transform, or one that leaves every point where it is.
    Nothing,
    /// A translation and nothing else: it changes where the box is drawn and nothing
    /// about its shape, so it is folded into the box's position.
    Translated(f32, f32),
    /// Any other 2D transform.
    Matrix(Transform2D),
    /// A 3D transform.
    ThreeD(Transform3D),
}

/// How `style`'s transform moves a box of `width` by `height`.
///
/// With `keep` set a translation is returned as a [`Moved::Matrix`] too. A renderer that
/// plays a transition of `transform` needs the translation as a transform, not folded into
/// a position it would have to animate separately.
pub(crate) fn moved(style: &ComputedValues, width: f32, height: f32, keep: bool) -> Moved {
    let Some(functions) = functions_of(style, width, height) else {
        return Moved::ThreeD(three_d(style, width, height));
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

/// The box's transform as one 4x4 matrix about `transform-origin`, for a transform that
/// [`functions_of`] found to be 3D.
fn three_d(style: &ComputedValues, width: f32, height: f32) -> Transform3D {
    let boxed = style.get_box();
    let mut matrix = Matrix3D::IDENTITY;
    match &boxed.translate {
        GenericTranslate::None => {}
        GenericTranslate::Translate(x, y, z) => {
            matrix = matrix.times(&Matrix3D::translation(
                x.resolve(Length::new(width)).px(),
                y.resolve(Length::new(height)).px(),
                z.px(),
            ));
        }
    }
    match &boxed.rotate {
        GenericRotate::None => {}
        GenericRotate::Rotate(angle) => {
            matrix = matrix.times(&Matrix3D::rotation(0.0, 0.0, 1.0, angle.radians()));
        }
        GenericRotate::Rotate3D(x, y, z, angle) => {
            matrix = matrix.times(&Matrix3D::rotation(*x, *y, *z, angle.radians()));
        }
    }
    match &boxed.scale {
        GenericScale::None => {}
        GenericScale::Scale(x, y, z) => {
            matrix = matrix.times(&Matrix3D::scaling(*x, *y, *z));
        }
    }
    matrix = matrix.times(&list_matrix_3d(&boxed.transform.0, width, height));

    // The origin a renderer is given lies in the box's plane, so a depth in
    // `transform-origin` goes into the matrix: move the origin's depth to zero, apply the
    // matrix, move it back.
    let depth = boxed.transform_origin.depth.px();
    if depth != 0.0 {
        matrix = Matrix3D::translation(0.0, 0.0, depth)
            .times(&matrix)
            .times(&Matrix3D::translation(0.0, 0.0, -depth));
    }
    Transform3D {
        matrix,
        origin: origin_of(style, width, height),
        backface_hidden: style.clone_backface_visibility()
            == style::computed_values::backface_visibility::T::Hidden,
    }
}

/// A parent's `perspective` as its children see it: the viewer's distance from the plane,
/// and the point the viewer looks at (`perspective-origin`) in document coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Perspective {
    pub(crate) depth: f32,
    pub(crate) x: f32,
    pub(crate) y: f32,
}

/// The `perspective` that `style` gives the children of a box whose border box is at
/// (`x`, `y`) and is `width` by `height`. `None` for `perspective: none`.
///
/// `perspective-origin` is measured from the top-left corner of the box's reference box,
/// which for an element laid out by CSS is its border box, and its percentages are of that
/// box's size.
pub(crate) fn perspective_of(
    style: &ComputedValues,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Option<Perspective> {
    let boxed = style.get_box();
    let depth = match &boxed.perspective {
        style::values::computed::Perspective::None => return None,
        style::values::computed::Perspective::Length(depth) => depth.0.px(),
    };
    let origin = &boxed.perspective_origin;
    Some(Perspective {
        depth,
        x: x + origin.horizontal.resolve(Length::new(width)).px(),
        y: y + origin.vertical.resolve(Length::new(height)).px(),
    })
}

/// Puts the parent's `perspective` in front of a child's 3D transform, for the child whose
/// untransformed border box has its top-left corner at (`x`, `y`) in the document.
///
/// CSS draws the child with `T(p) P T(-p)` applied after its own transform, where `P` is
/// the matrix of `perspective(depth)` and `p` is the perspective origin: move the origin
/// to zero, divide by the distance, move it back. The child's own transform is
/// `T(o) M T(-o)` about its `transform-origin` `o`. With both points measured from the
/// child's corner, what is drawn is
///
/// ```text
/// T(p) P T(-p) T(o) M T(-o)  =  T(o) [T(p - o) P T(o - p) M] T(-o)
/// ```
///
/// so the matrix in the brackets, about the same `transform-origin`, is the one transform
/// the renderer needs. Nothing about the parent travels beside it.
pub(crate) fn under_perspective(
    transform: Transform3D,
    perspective: &Perspective,
    x: f32,
    y: f32,
) -> Transform3D {
    let across = perspective.x - x - transform.origin.0;
    let down = perspective.y - y - transform.origin.1;
    let matrix = Matrix3D::translation(across, down, 0.0)
        .times(&Matrix3D::perspective(perspective.depth))
        .times(&Matrix3D::translation(-across, -down, 0.0))
        .times(&transform.matrix);
    Transform3D {
        matrix,
        ..transform
    }
}

/// The product of a `transform` list as a 4x4 matrix, in list order.
fn list_matrix_3d(ops: &[TransformOperation], width: f32, height: f32) -> Matrix3D {
    let across =
        |value: &style::values::computed::LengthPercentage| value.resolve(Length::new(width)).px();
    let down =
        |value: &style::values::computed::LengthPercentage| value.resolve(Length::new(height)).px();
    let flat = |matrix: Matrix2D| Matrix3D::from_2d(&matrix);
    let mut total = Matrix3D::IDENTITY;
    for op in ops {
        let matrix = match op {
            Operation::Matrix(m) => flat(Matrix2D {
                a: m.a,
                b: m.b,
                c: m.c,
                d: m.d,
                e: m.e,
                f: m.f,
            }),
            Operation::Matrix3D(m) => Matrix3D([
                m.m11, m.m12, m.m13, m.m14, m.m21, m.m22, m.m23, m.m24, m.m31, m.m32, m.m33, m.m34,
                m.m41, m.m42, m.m43, m.m44,
            ]),
            Operation::Skew(x, y) => flat(Matrix2D::skew(x.radians(), y.radians())),
            Operation::SkewX(x) => flat(Matrix2D::skew(x.radians(), 0.0)),
            Operation::SkewY(y) => flat(Matrix2D::skew(0.0, y.radians())),
            Operation::Translate(x, y) => Matrix3D::translation(across(x), down(y), 0.0),
            Operation::TranslateX(x) => Matrix3D::translation(across(x), 0.0, 0.0),
            Operation::TranslateY(y) => Matrix3D::translation(0.0, down(y), 0.0),
            Operation::TranslateZ(z) => Matrix3D::translation(0.0, 0.0, z.px()),
            Operation::Translate3D(x, y, z) => Matrix3D::translation(across(x), down(y), z.px()),
            Operation::Scale(x, y) => Matrix3D::scaling(*x, *y, 1.0),
            Operation::ScaleX(x) => Matrix3D::scaling(*x, 1.0, 1.0),
            Operation::ScaleY(y) => Matrix3D::scaling(1.0, *y, 1.0),
            Operation::ScaleZ(z) => Matrix3D::scaling(1.0, 1.0, *z),
            Operation::Scale3D(x, y, z) => Matrix3D::scaling(*x, *y, *z),
            Operation::Rotate(angle) | Operation::RotateZ(angle) => {
                Matrix3D::rotation(0.0, 0.0, 1.0, angle.radians())
            }
            Operation::RotateX(angle) => Matrix3D::rotation(1.0, 0.0, 0.0, angle.radians()),
            Operation::RotateY(angle) => Matrix3D::rotation(0.0, 1.0, 0.0, angle.radians()),
            Operation::Rotate3D(x, y, z, angle) => Matrix3D::rotation(*x, *y, *z, angle.radians()),
            Operation::Perspective(depth) => {
                Matrix3D::perspective(depth.infinity_or(|length| length.px()))
            }
            // As in the 2D list: the matrices are interpolated component by component,
            // which keeps the two ends right.
            Operation::InterpolateMatrix {
                from_list,
                to_list,
                progress,
            } => {
                let from = list_matrix_3d(&from_list.0, width, height);
                let to = list_matrix_3d(&to_list.0, width, height);
                from.lerp(&to, progress.0)
            }
            Operation::AccumulateMatrix { .. } => Matrix3D::IDENTITY,
        };
        total = total.times(&matrix);
    }
    total
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
