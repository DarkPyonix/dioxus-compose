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

use std::f32::consts::{PI, TAU};

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
            Operation::InterpolateMatrix {
                from_list,
                to_list,
                progress,
            } => {
                let from = list_matrix_3d(&from_list.0, width, height);
                let to = list_matrix_3d(&to_list.0, width, height);
                interpolate_3d(&from, &to, progress.0)
            }
            Operation::AccumulateMatrix {
                from_list,
                to_list,
                count,
            } => {
                let from = list_matrix_3d(&from_list.0, width, height);
                let to = list_matrix_3d(&to_list.0, width, height);
                accumulate_3d(&from, &to, *count)
            }
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
            Operation::InterpolateMatrix {
                from_list,
                to_list,
                progress,
            } => {
                let from = list_matrix(&from_list.0, width, height)?;
                let to = list_matrix(&to_list.0, width, height)?;
                Some(TransformFunction::Matrix(interpolate_2d(
                    &from, &to, progress.0,
                )))
            }
            Operation::AccumulateMatrix {
                from_list,
                to_list,
                count,
            } => {
                let from = list_matrix(&from_list.0, width, height)?;
                let to = list_matrix(&to_list.0, width, height)?;
                Some(TransformFunction::Matrix(accumulate_2d(&from, &to, *count)))
            }
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

#[derive(Clone, Copy, Debug)]
struct Decomposed2D {
    translate: (f32, f32),
    scale: (f32, f32),
    angle: f32,
    m11: f32,
    m12: f32,
    m21: f32,
    m22: f32,
}

fn decompose_2d(matrix: &Matrix2D) -> Option<Decomposed2D> {
    let det = matrix.determinant();
    if det.abs() < 1e-12 || !det.is_finite() {
        return None;
    }
    let mut row0x = matrix.a;
    let mut row0y = matrix.b;
    let mut row1x = matrix.c;
    let mut row1y = matrix.d;

    let mut scale_x = row0x.hypot(row0y);
    let mut scale_y = row1x.hypot(row1y);
    if scale_x < 1e-12 || scale_y < 1e-12 {
        return None;
    }
    if det < 0.0 {
        if row0x < row1y {
            scale_x = -scale_x;
        } else {
            scale_y = -scale_y;
        }
    }
    row0x /= scale_x;
    row0y /= scale_x;
    row1x /= scale_y;
    row1y /= scale_y;

    let angle = row0y.atan2(row0x);
    let (m11, m12, m21, m22) = if angle != 0.0 {
        let cs = row0x;
        let sn = row0y;
        (1.0, 0.0, cs * row1x + sn * row1y, -sn * row1x + cs * row1y)
    } else {
        (row0x, row0y, row1x, row1y)
    };

    Some(Decomposed2D {
        translate: (matrix.e, matrix.f),
        scale: (scale_x, scale_y),
        angle,
        m11,
        m12,
        m21,
        m22,
    })
}

fn recompose_2d(d: &Decomposed2D) -> Matrix2D {
    let remainder = Matrix2D {
        a: d.m11,
        b: d.m12,
        c: d.m21,
        d: d.m22,
        e: 0.0,
        f: 0.0,
    };
    Matrix2D::translation(d.translate.0, d.translate.1)
        .times(&Matrix2D::rotation(d.angle))
        .times(&remainder)
        .times(&Matrix2D::scaling(d.scale.0, d.scale.1))
}

/// Interpolates two 2D matrices by decomposing them into translation, rotation, shear
/// remainder and scale, falling back to a discrete step if either matrix is singular.
pub(crate) fn interpolate_2d(from: &Matrix2D, to: &Matrix2D, progress: f32) -> Matrix2D {
    let (Some(mut a), Some(b)) = (decompose_2d(from), decompose_2d(to)) else {
        return if progress < 0.5 { *from } else { *to };
    };
    if (a.scale.0 < 0.0 && b.scale.1 < 0.0) || (a.scale.1 < 0.0 && b.scale.0 < 0.0) {
        a.scale.0 = -a.scale.0;
        a.scale.1 = -a.scale.1;
        a.angle += if a.angle < 0.0 { PI } else { -PI };
    }
    let mut angle_a = a.angle;
    let mut angle_b = b.angle;
    if angle_b - angle_a > PI {
        angle_a += TAU;
    } else if angle_a - angle_b > PI {
        angle_b += TAU;
    }
    let mix = |x: f32, y: f32| x + (y - x) * progress;
    recompose_2d(&Decomposed2D {
        translate: (
            mix(a.translate.0, b.translate.0),
            mix(a.translate.1, b.translate.1),
        ),
        scale: (mix(a.scale.0, b.scale.0), mix(a.scale.1, b.scale.1)),
        angle: mix(angle_a, angle_b),
        m11: mix(a.m11, b.m11),
        m12: mix(a.m12, b.m12),
        m21: mix(a.m21, b.m21),
        m22: mix(a.m22, b.m22),
    })
}

/// Accumulates `count` copies of `from` onto `to` in decomposed 2D space.
pub(crate) fn accumulate_2d(from: &Matrix2D, to: &Matrix2D, count: i32) -> Matrix2D {
    let (Some(a), Some(b)) = (decompose_2d(from), decompose_2d(to)) else {
        return *to;
    };
    let c = count as f32;
    let acc_zero = |x: f32, y: f32| x * c + y;
    let acc_one = |x: f32, y: f32| (x - 1.0) * c + y;
    recompose_2d(&Decomposed2D {
        translate: (
            acc_zero(a.translate.0, b.translate.0),
            acc_zero(a.translate.1, b.translate.1),
        ),
        scale: (acc_one(a.scale.0, b.scale.0), acc_one(a.scale.1, b.scale.1)),
        angle: acc_zero(a.angle, b.angle),
        m11: acc_one(a.m11, b.m11),
        m12: acc_zero(a.m12, b.m12),
        m21: acc_zero(a.m21, b.m21),
        m22: acc_one(a.m22, b.m22),
    })
}

#[derive(Clone, Copy, Debug)]
struct Decomposed3D {
    perspective: [f32; 4],
    translate: [f32; 3],
    quaternion: [f32; 4],
    skew: [f32; 3],
    scale: [f32; 3],
}

fn vec3_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn vec3_cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn vec3_len(v: [f32; 3]) -> f32 {
    vec3_dot(v, v).sqrt()
}

fn vec3_scale(v: [f32; 3], s: f32) -> [f32; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

fn vec3_combine(a: [f32; 3], sa: f32, b: [f32; 3], sb: f32) -> [f32; 3] {
    [
        a[0] * sa + b[0] * sb,
        a[1] * sa + b[1] * sb,
        a[2] * sa + b[2] * sb,
    ]
}

fn inverse_4x4(m: &[f32; 16]) -> Option<[f32; 16]> {
    let mut inv = [0.0f32; 16];
    inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
        + m[9] * m[7] * m[14]
        + m[13] * m[6] * m[11]
        - m[13] * m[7] * m[10];
    inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
        - m[8] * m[7] * m[14]
        - m[12] * m[6] * m[11]
        + m[12] * m[7] * m[10];
    inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
        + m[8] * m[7] * m[13]
        + m[12] * m[5] * m[11]
        - m[12] * m[7] * m[9];
    inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
        - m[8] * m[6] * m[13]
        - m[12] * m[5] * m[10]
        + m[12] * m[6] * m[9];
    inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
        - m[9] * m[3] * m[14]
        - m[13] * m[2] * m[11]
        + m[13] * m[3] * m[10];
    inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
        + m[8] * m[3] * m[14]
        + m[12] * m[2] * m[11]
        - m[12] * m[3] * m[10];
    inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
        - m[8] * m[3] * m[13]
        - m[12] * m[1] * m[11]
        + m[12] * m[3] * m[9];
    inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
        + m[8] * m[2] * m[13]
        + m[12] * m[1] * m[10]
        - m[12] * m[2] * m[9];
    inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
        + m[5] * m[3] * m[14]
        + m[13] * m[2] * m[7]
        - m[13] * m[3] * m[6];
    inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
        - m[4] * m[3] * m[14]
        - m[12] * m[2] * m[7]
        + m[12] * m[3] * m[6];
    inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
        + m[4] * m[3] * m[13]
        + m[12] * m[1] * m[7]
        - m[12] * m[3] * m[5];
    inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
        - m[4] * m[2] * m[13]
        - m[12] * m[1] * m[6]
        + m[12] * m[2] * m[5];
    inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
        - m[5] * m[3] * m[10]
        - m[9] * m[2] * m[7]
        + m[9] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
        + m[4] * m[3] * m[10]
        + m[8] * m[2] * m[7]
        - m[8] * m[3] * m[6];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
        - m[4] * m[3] * m[9]
        - m[8] * m[1] * m[7]
        + m[8] * m[3] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
        + m[4] * m[2] * m[9]
        + m[8] * m[1] * m[6]
        - m[8] * m[2] * m[5];

    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det.abs() < 1e-12 || !det.is_finite() {
        return None;
    }
    let scale = 1.0 / det;
    for value in &mut inv {
        *value *= scale;
    }
    Some(inv)
}

fn decompose_3d(matrix: &Matrix3D) -> Option<Decomposed3D> {
    let w = matrix.0[15];
    if w.abs() < 1e-12 || !w.is_finite() {
        return None;
    }
    let mut m = matrix.0;
    for value in &mut m {
        *value /= w;
    }

    let mut p = m;
    p[3] = 0.0;
    p[7] = 0.0;
    p[11] = 0.0;
    p[15] = 1.0;
    let inv_p = inverse_4x4(&p)?;

    let perspective = if m[3] != 0.0 || m[7] != 0.0 || m[11] != 0.0 {
        let rhs = [m[3], m[7], m[11], m[15]];
        // Multiply the transpose of `inv_p` by `rhs`.
        [
            inv_p[0] * rhs[0] + inv_p[1] * rhs[1] + inv_p[2] * rhs[2] + inv_p[3] * rhs[3],
            inv_p[4] * rhs[0] + inv_p[5] * rhs[1] + inv_p[6] * rhs[2] + inv_p[7] * rhs[3],
            inv_p[8] * rhs[0] + inv_p[9] * rhs[1] + inv_p[10] * rhs[2] + inv_p[11] * rhs[3],
            inv_p[12] * rhs[0] + inv_p[13] * rhs[1] + inv_p[14] * rhs[2] + inv_p[15] * rhs[3],
        ]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };

    let translate = [m[12], m[13], m[14]];

    let mut row = [
        [m[0], m[1], m[2]],
        [m[4], m[5], m[6]],
        [m[8], m[9], m[10]],
    ];
    let mut scale = [0.0f32; 3];
    let mut skew = [0.0f32; 3];

    scale[0] = vec3_len(row[0]);
    if scale[0] < 1e-12 {
        return None;
    }
    row[0] = vec3_scale(row[0], 1.0 / scale[0]);

    skew[0] = vec3_dot(row[0], row[1]);
    row[1] = vec3_combine(row[1], 1.0, row[0], -skew[0]);

    scale[1] = vec3_len(row[1]);
    if scale[1] < 1e-12 {
        return None;
    }
    row[1] = vec3_scale(row[1], 1.0 / scale[1]);
    skew[0] /= scale[1];

    skew[1] = vec3_dot(row[0], row[2]);
    row[2] = vec3_combine(row[2], 1.0, row[0], -skew[1]);
    skew[2] = vec3_dot(row[1], row[2]);
    row[2] = vec3_combine(row[2], 1.0, row[1], -skew[2]);

    scale[2] = vec3_len(row[2]);
    if scale[2] < 1e-12 {
        return None;
    }
    row[2] = vec3_scale(row[2], 1.0 / scale[2]);
    skew[1] /= scale[2];
    skew[2] /= scale[2];

    if vec3_dot(row[0], vec3_cross(row[1], row[2])) < 0.0 {
        for index in 0..3 {
            scale[index] = -scale[index];
            row[index] = vec3_scale(row[index], -1.0);
        }
    }

    let trace = row[0][0] + row[1][1] + row[2][2];
    let quaternion = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [
            (row[1][2] - row[2][1]) / s,
            (row[2][0] - row[0][2]) / s,
            (row[0][1] - row[1][0]) / s,
            0.25 * s,
        ]
    } else if row[0][0] > row[1][1] && row[0][0] > row[2][2] {
        let s = (1.0 + row[0][0] - row[1][1] - row[2][2]).sqrt() * 2.0;
        [
            0.25 * s,
            (row[0][1] + row[1][0]) / s,
            (row[2][0] + row[0][2]) / s,
            (row[1][2] - row[2][1]) / s,
        ]
    } else if row[1][1] > row[2][2] {
        let s = (1.0 + row[1][1] - row[0][0] - row[2][2]).sqrt() * 2.0;
        [
            (row[0][1] + row[1][0]) / s,
            0.25 * s,
            (row[1][2] + row[2][1]) / s,
            (row[2][0] - row[0][2]) / s,
        ]
    } else {
        let s = (1.0 + row[2][2] - row[0][0] - row[1][1]).sqrt() * 2.0;
        [
            (row[2][0] + row[0][2]) / s,
            (row[1][2] + row[2][1]) / s,
            0.25 * s,
            (row[0][1] - row[1][0]) / s,
        ]
    };

    Some(Decomposed3D {
        perspective,
        translate,
        quaternion,
        skew,
        scale,
    })
}

fn quat_to_matrix(q: [f32; 4]) -> Matrix3D {
    let [x, y, z, w] = q;
    Matrix3D([
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        0.0,
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        0.0,
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ])
}

fn quat_slerp(a: [f32; 4], mut b: [f32; 4], progress: f32) -> [f32; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    if dot < 0.0 {
        dot = -dot;
        b = [-b[0], -b[1], -b[2], -b[3]];
    }
    let dot = dot.clamp(-1.0, 1.0);
    if dot > 0.9995 {
        let raw = [
            a[0] + (b[0] - a[0]) * progress,
            a[1] + (b[1] - a[1]) * progress,
            a[2] + (b[2] - a[2]) * progress,
            a[3] + (b[3] - a[3]) * progress,
        ];
        let len = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2] + raw[3] * raw[3]).sqrt();
        if len > 1e-12 {
            return [raw[0] / len, raw[1] / len, raw[2] / len, raw[3] / len];
        }
        return [0.0, 0.0, 0.0, 1.0];
    }
    let theta = dot.acos();
    let sin_theta = theta.sin();
    let w0 = ((1.0 - progress) * theta).sin() / sin_theta;
    let w1 = (progress * theta).sin() / sin_theta;
    [
        w0 * a[0] + w1 * b[0],
        w0 * a[1] + w1 * b[1],
        w0 * a[2] + w1 * b[2],
        w0 * a[3] + w1 * b[3],
    ]
}

fn quat_pow(q: [f32; 4], power: f32) -> [f32; 4] {
    let v_len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
    if v_len < 1e-12 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let half_angle = v_len.atan2(q[3]);
    let (sin, cos) = (half_angle * power).sin_cos();
    let scale = sin / v_len;
    [q[0] * scale, q[1] * scale, q[2] * scale, cos]
}

fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn recompose_3d(d: &Decomposed3D) -> Matrix3D {
    let mut matrix = Matrix3D::IDENTITY;
    matrix.0[3] = d.perspective[0];
    matrix.0[7] = d.perspective[1];
    matrix.0[11] = d.perspective[2];
    matrix.0[15] = d.perspective[3];

    matrix = matrix.times(&Matrix3D::translation(
        d.translate[0],
        d.translate[1],
        d.translate[2],
    ));
    matrix = matrix.times(&quat_to_matrix(d.quaternion));

    if d.skew[2] != 0.0 {
        let mut shear = Matrix3D::IDENTITY;
        shear.0[9] = d.skew[2];
        matrix = matrix.times(&shear);
    }
    if d.skew[1] != 0.0 {
        let mut shear = Matrix3D::IDENTITY;
        shear.0[8] = d.skew[1];
        matrix = matrix.times(&shear);
    }
    if d.skew[0] != 0.0 {
        let mut shear = Matrix3D::IDENTITY;
        shear.0[4] = d.skew[0];
        matrix = matrix.times(&shear);
    }

    matrix.times(&Matrix3D::scaling(d.scale[0], d.scale[1], d.scale[2]))
}

/// Interpolates two 4x4 matrices by decomposing them into perspective, translation,
/// rotation quaternion, shear and scale, falling back to a discrete step if either matrix
/// is singular.
pub(crate) fn interpolate_3d(from: &Matrix3D, to: &Matrix3D, progress: f32) -> Matrix3D {
    let (Some(a), Some(b)) = (decompose_3d(from), decompose_3d(to)) else {
        return if progress < 0.5 { *from } else { *to };
    };
    let mix = |x: f32, y: f32| x + (y - x) * progress;
    recompose_3d(&Decomposed3D {
        perspective: [
            mix(a.perspective[0], b.perspective[0]),
            mix(a.perspective[1], b.perspective[1]),
            mix(a.perspective[2], b.perspective[2]),
            mix(a.perspective[3], b.perspective[3]),
        ],
        translate: [
            mix(a.translate[0], b.translate[0]),
            mix(a.translate[1], b.translate[1]),
            mix(a.translate[2], b.translate[2]),
        ],
        quaternion: quat_slerp(a.quaternion, b.quaternion, progress),
        skew: [
            mix(a.skew[0], b.skew[0]),
            mix(a.skew[1], b.skew[1]),
            mix(a.skew[2], b.skew[2]),
        ],
        scale: [
            mix(a.scale[0], b.scale[0]),
            mix(a.scale[1], b.scale[1]),
            mix(a.scale[2], b.scale[2]),
        ],
    })
}

/// Accumulates `count` copies of `from` onto `to` in decomposed 3D space.
pub(crate) fn accumulate_3d(from: &Matrix3D, to: &Matrix3D, count: i32) -> Matrix3D {
    let (Some(a), Some(b)) = (decompose_3d(from), decompose_3d(to)) else {
        return *to;
    };
    let c = count as f32;
    let acc_zero = |x: f32, y: f32| x * c + y;
    let acc_one = |x: f32, y: f32| (x - 1.0) * c + y;
    recompose_3d(&Decomposed3D {
        perspective: [
            acc_zero(a.perspective[0], b.perspective[0]),
            acc_zero(a.perspective[1], b.perspective[1]),
            acc_zero(a.perspective[2], b.perspective[2]),
            acc_one(a.perspective[3], b.perspective[3]),
        ],
        translate: [
            acc_zero(a.translate[0], b.translate[0]),
            acc_zero(a.translate[1], b.translate[1]),
            acc_zero(a.translate[2], b.translate[2]),
        ],
        quaternion: quat_mul(quat_pow(a.quaternion, c), b.quaternion),
        skew: [
            acc_zero(a.skew[0], b.skew[0]),
            acc_zero(a.skew[1], b.skew[1]),
            acc_zero(a.skew[2], b.skew[2]),
        ],
        scale: [
            acc_one(a.scale[0], b.scale[0]),
            acc_one(a.scale[1], b.scale[1]),
            acc_one(a.scale[2], b.scale[2]),
        ],
    })
}

#[cfg(test)]
mod tests {
    use style::values::computed::angle::Angle;
    use style::values::computed::transform::Transform;
    use style::values::computed::{LengthPercentage, Percentage};

    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn assert_matrix_2d_close(actual: Matrix2D, expected: Matrix2D) {
        assert!(
            close(actual.a, expected.a)
                && close(actual.b, expected.b)
                && close(actual.c, expected.c)
                && close(actual.d, expected.d)
                && close(actual.e, expected.e)
                && close(actual.f, expected.f),
            "actual {actual:?} != expected {expected:?}"
        );
    }

    fn assert_matrix_3d_close(actual: Matrix3D, expected: Matrix3D) {
        for (index, (&a, &e)) in actual.0.iter().zip(expected.0.iter()).enumerate() {
            assert!(
                close(a, e),
                "entry {index}: actual {a} != expected {e}\nactual:   {actual:?}\nexpected: {expected:?}"
            );
        }
    }

    #[test]
    fn fr34_1_interpolate_matrix_2d_rotates_without_shrinking() {
        let from = Transform(vec![Operation::Rotate(Angle::from_degrees(0.0))].into());
        let to = Transform(vec![Operation::Rotate(Angle::from_degrees(90.0))].into());
        let op = Operation::InterpolateMatrix {
            from_list: from,
            to_list: to,
            progress: Percentage(0.5),
        };
        let matrix = list_matrix(&[op], 100.0, 100.0).expect("2D matrix");
        let expected = Matrix2D::rotation(45.0f32.to_radians());
        assert_matrix_2d_close(matrix, expected);
        assert!(
            close(matrix.determinant(), 1.0),
            "decomposed rotation preserves area: det={}",
            matrix.determinant()
        );
    }

    #[test]
    fn fr34_1_accumulate_matrix_2d_combines_translation_scale_and_rotation() {
        let from = Transform(
            vec![
                Operation::TranslateX(LengthPercentage::new_length(Length::new(10.0))),
                Operation::Rotate(Angle::from_degrees(30.0)),
                Operation::Scale(2.0, 1.5),
            ]
            .into(),
        );
        let to = Transform(
            vec![
                Operation::TranslateX(LengthPercentage::new_length(Length::new(5.0))),
                Operation::Rotate(Angle::from_degrees(15.0)),
                Operation::Scale(1.25, 2.0),
            ]
            .into(),
        );
        let op = Operation::AccumulateMatrix {
            from_list: from,
            to_list: to,
            count: 2,
        };
        let matrix = list_matrix(&[op], 100.0, 100.0).expect("2D matrix");
        // translate: 10*2 + 5 = 25; rotate: 30*2 + 15 = 75deg; scale: (2-1)*2+1.25=3.25, (1.5-1)*2+2=3.0
        let expected = Matrix2D::translation(25.0, 0.0)
            .times(&Matrix2D::rotation(75.0f32.to_radians()))
            .times(&Matrix2D::scaling(3.25, 3.0));
        assert_matrix_2d_close(matrix, expected);
    }

    #[test]
    fn fr34_1_interpolate_matrix_3d_slerps_rotation_and_interpolates_perspective() {
        let from = Matrix3D::perspective(400.0).times(&Matrix3D::rotation(0.0, 1.0, 0.0, 0.0));
        let to = Matrix3D::perspective(200.0)
            .times(&Matrix3D::rotation(0.0, 1.0, 0.0, 90.0f32.to_radians()));
        let mid = interpolate_3d(&from, &to, 0.5);
        // Perspective -1/d interpolates linearly from -1/400 to -1/200 (-0.00375),
        // and rotation slerps to rotateY(45deg).
        let mut expected = Matrix3D::IDENTITY;
        expected.0[11] = (-1.0 / 400.0 + -1.0 / 200.0) * 0.5;
        expected = expected.times(&Matrix3D::rotation(0.0, 1.0, 0.0, 45.0f32.to_radians()));
        assert_matrix_3d_close(mid, expected);
    }

    #[test]
    fn fr34_1_accumulate_matrix_3d_combines_translation_scale_and_rotation() {
        let from = Transform(
            vec![
                Operation::Translate3D(
                    LengthPercentage::new_length(Length::new(10.0)),
                    LengthPercentage::new_length(Length::new(0.0)),
                    Length::new(20.0),
                ),
                Operation::RotateY(Angle::from_degrees(30.0)),
                Operation::Scale3D(1.5, 1.0, 2.0),
            ]
            .into(),
        );
        let to = Transform(
            vec![
                Operation::Translate3D(
                    LengthPercentage::new_length(Length::new(5.0)),
                    LengthPercentage::new_length(Length::new(4.0)),
                    Length::new(10.0),
                ),
                Operation::RotateY(Angle::from_degrees(15.0)),
                Operation::Scale3D(1.0, 1.5, 1.0),
            ]
            .into(),
        );
        let op = Operation::AccumulateMatrix {
            from_list: from,
            to_list: to,
            count: 2,
        };
        let matrix = list_matrix_3d(&[op], 100.0, 100.0);
        // translate: (25, 4, 50); rotateY: 75deg; scale: (2.0, 1.5, 3.0)
        let expected = Matrix3D::translation(25.0, 4.0, 50.0)
            .times(&Matrix3D::rotation(0.0, 1.0, 0.0, 75.0f32.to_radians()))
            .times(&Matrix3D::scaling(2.0, 1.5, 3.0));
        assert_matrix_3d_close(matrix, expected);
    }

    #[test]
    fn fr34_1_singular_matrix_interpolation_falls_back_to_discrete_step() {
        let singular = Matrix2D::scaling(0.0, 1.0);
        let target = Matrix2D::translation(20.0, 10.0);
        assert_matrix_2d_close(interpolate_2d(&singular, &target, 0.25), singular);
        assert_matrix_2d_close(interpolate_2d(&singular, &target, 0.75), target);
        assert_matrix_2d_close(accumulate_2d(&singular, &target, 2), target);

        let singular_3d = Matrix3D::scaling(0.0, 1.0, 1.0);
        let target_3d = Matrix3D::translation(20.0, 10.0, 5.0);
        assert_matrix_3d_close(interpolate_3d(&singular_3d, &target_3d, 0.25), singular_3d);
        assert_matrix_3d_close(interpolate_3d(&singular_3d, &target_3d, 0.75), target_3d);
        assert_matrix_3d_close(accumulate_3d(&singular_3d, &target_3d, 2), target_3d);
    }
}
