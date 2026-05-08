//! Interpolation functions.

// Adapted from d3 3.5.17 by Mike Bostock
// SPDX-License-Identifier: ISC

use super::{
    super::{DoubleSizeIterator, mark::Interpolate},
    Vec2,
    path::SvgPath,
};

/// Interpolation mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Mode {
    /// Area drawing mode, drawing the `(n % 2 == 0)`th line.
    AreaEven,
    /// Area drawing mode, drawing the `(n % 2 == 1)`th line.
    AreaOdd,
    /// Line drawing mode.
    Line,
}

/// Gets the interpolation function for the given variant.
pub(super) fn get(
    kind: Interpolate,
) -> fn(&mut SvgPath, &mut dyn DoubleSizeIterator<Item = Vec2>, Mode, Option<f64>) {
    match kind {
        Interpolate::Basis => basis,
        Interpolate::BasisClosed => basis_closed,
        Interpolate::BasisOpen => basis_open,
        Interpolate::Bundle => bundle,
        Interpolate::Cardinal => cardinal,
        Interpolate::CardinalClosed => cardinal_closed,
        Interpolate::CardinalOpen => cardinal_open,
        Interpolate::Linear => linear,
        Interpolate::LinearClosed => linear_closed,
        Interpolate::Monotone => monotone,
        Interpolate::Step => step::<StepMid>,
        Interpolate::StepAfter => step::<StepAfter>,
        Interpolate::StepBefore => step::<StepBefore>,
    }
}

/// Cubic basis spline function.
fn basis(
    path: &mut SvgPath,
    points: &mut dyn DoubleSizeIterator<Item = Vec2>,
    mode: Mode,
    _: Option<f64>,
) {
    let mut step = 0;
    let (mut p0, mut p1) = <_>::default();
    for point in points {
        match step {
            0 => {
                step += 1;
                if mode == Mode::AreaOdd {
                    path.line_to(point);
                } else {
                    path.move_to(point);
                }
            }
            1 => {
                step += 1;
            }
            2 => {
                step += 1;
                path.line_to((p0 * 5.0 + p1) / 6.0);
                basis_point(path, p0, p1, point);
            }
            _ => basis_point(path, p0, p1, point),
        }
        p0 = p1;
        p1 = point;
    }

    if step >= 2 {
        if step == 3 {
            basis_point(path, p0, p1, p1);
        }

        path.line_to(p1);
    }

    if mode == Mode::AreaOdd || (mode == Mode::Line && step == 1) {
        path.close();
    }
}

/// Closed cubic basis spline function.
fn basis_closed(
    path: &mut SvgPath,
    points: &mut dyn DoubleSizeIterator<Item = Vec2>,
    _: Mode,
    _: Option<f64>,
) {
    let mut step = 0;
    let (mut p0, mut p1, mut p2, mut p3, mut p4) = <_>::default();
    for point in points {
        match step {
            0 => {
                step += 1;
                p2 = point;
            }
            1 => {
                step += 1;
                p3 = point;
            }
            2 => {
                step += 1;
                p4 = point;
                path.move_to((p0 + p1 * 4.0 + point) / 6.0);
            }
            _ => basis_point(path, p0, p1, point),
        }
        p0 = p1;
        p1 = point;
    }

    match step {
        1 => {
            path.move_to(p2);
            path.close();
        }
        2 => {
            path.move_to((p2 + p3 * 2.0) / 3.0);
            path.line_to((p3 + p2 * 2.0) / 3.0);
            path.close();
        }
        3 => {
            for point in [p2, p3, p4] {
                basis_point(path, p0, p1, point);
                p0 = p1;
                p1 = point;
            }
        }
        _ => {}
    }
}

/// Open cubic basis spline function.
fn basis_open(
    path: &mut SvgPath,
    points: &mut dyn DoubleSizeIterator<Item = Vec2>,
    mode: Mode,
    _: Option<f64>,
) {
    let mut step = 0;
    let (mut p0, mut p1) = <_>::default();
    for point in points {
        match step {
            0 | 1 => step += 1,
            2 => {
                step += 1;
                let p = (p0 + p1 * 4.0 + point) / 6.0;
                if mode == Mode::AreaOdd {
                    path.line_to(p);
                } else {
                    path.move_to(p);
                }
            }
            3 => {
                step += 1;
                basis_point(path, p0, p1, point);
            }
            _ => basis_point(path, p0, p1, point),
        }
        p0 = p1;
        p1 = point;
    }

    if mode == Mode::AreaOdd || (mode == Mode::Line && step == 3) {
        path.close();
    }
}

/// Draws a Bézier curve for a three-point basis spline.
fn basis_point(path: &mut SvgPath, p0: Vec2, p1: Vec2, p2: Vec2) {
    path.curve_to(
        (p0 * 2.0 + p1) / 3.0,
        (p0 + p1 * 2.0) / 3.0,
        (p0 + p1 * 4.0 + p2) / 6.0,
    );
}

/// Straightened cubic basis spline function.
fn bundle(
    path: &mut SvgPath,
    points: &mut dyn DoubleSizeIterator<Item = Vec2>,
    mode: Mode,
    beta: Option<f64>,
) {
    let mut points = peeknth::sizedpeekdn::<_, 1, 1>(points);

    if points.len() > 1 && beta != Some(1.0) {
        #[expect(
            clippy::cast_precision_loss,
            reason = "if there are ever ≥2**53 points, something sure happened"
        )]
        let len = points.len() as f64;
        let (first, delta) = {
            let first = points.peek_front().copied().unwrap();
            let last = points.peek_back().copied().unwrap();
            (first, last - first)
        };
        let beta = beta.unwrap_or(0.85);
        basis(
            path,
            &mut points.enumerate().map(|(i, point)| {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "if there are ever ≥2**53 points, something sure happened"
                )]
                let t = i as f64 / len;
                let mut point = point * beta;
                point.x += (1.0 - beta) * (first.x + t * delta.x);
                point.y += (1.0 - beta) * (first.y + t * delta.y);
                point
            }),
            mode,
            None,
        );
    } else {
        basis(path, &mut points, mode, None);
    }
}

/// Open cardinal spline function.
fn cardinal(
    path: &mut SvgPath,
    points: &mut dyn DoubleSizeIterator<Item = Vec2>,
    mode: Mode,
    tension: Option<f64>,
) {
    let mut step = 0;
    let (mut p0, mut p1, mut p2) = <_>::default();
    let k = (1.0 - tension.unwrap_or(0.0)) / 6.0;
    for point in points {
        match step {
            0 => {
                step += 1;
                if mode == Mode::AreaOdd {
                    path.line_to(point);
                } else {
                    path.move_to(point);
                }
            }
            1 => {
                step += 1;
                p1 = point;
            }
            2 => {
                step += 1;
                cardinal_point(path, p0, p1, p2, point, k);
            }
            _ => cardinal_point(path, p0, p1, p2, point, k),
        }
        p0 = p1;
        p1 = p2;
        p2 = point;
    }

    match step {
        2 => {
            path.line_to(p2);
        }
        3 => {
            cardinal_point(path, p0, p1, p2, p1, k);
        }
        _ => {}
    }

    if mode == Mode::AreaOdd || (mode == Mode::Line && step == 1) {
        path.close();
    }
}

/// Open cardinal spline function.
fn cardinal_closed(
    path: &mut SvgPath,
    points: &mut dyn DoubleSizeIterator<Item = Vec2>,
    _: Mode,
    tension: Option<f64>,
) {
    let mut step = 0;
    let (mut p0, mut p1, mut p2, mut p3, mut p4, mut p5) = <_>::default();
    let k = (1.0 - tension.unwrap_or(0.0)) / 6.0;

    for point in points {
        match step {
            0 => {
                step += 1;
                p3 = point;
            }
            1 => {
                step += 1;
                p4 = point;
                path.move_to(point);
            }
            2 => {
                step += 1;
                p5 = point;
            }
            _ => cardinal_point(path, p0, p1, p2, point, k),
        }
        p0 = p1;
        p1 = p2;
        p2 = point;
    }

    match step {
        1 => {
            path.move_to(p3);
            path.close();
        }
        2 => {
            path.line_to(p3);
            path.close();
        }
        3 => {
            cardinal_point(path, p0, p1, p2, p3, k);
            cardinal_point(path, p1, p2, p3, p4, k);
            cardinal_point(path, p2, p3, p4, p5, k);
        }
        _ => {}
    }
}

/// Open cardinal spline function.
fn cardinal_open(
    path: &mut SvgPath,
    points: &mut dyn DoubleSizeIterator<Item = Vec2>,
    mode: Mode,
    tension: Option<f64>,
) {
    let mut step = 0;
    let (mut p0, mut p1, mut p2) = <_>::default();
    let k = (1.0 - tension.unwrap_or(0.0)) / 6.0;

    for point in points {
        match step {
            0 | 1 => {
                step += 1;
            }
            2 => {
                step += 1;
                if mode == Mode::AreaOdd {
                    path.line_to(p2);
                } else {
                    path.move_to(p2);
                }
            }
            3 => {
                step += 1;
                cardinal_point(path, p0, p1, p2, point, k);
            }
            _ => cardinal_point(path, p0, p1, p2, point, k),
        }
        p0 = p1;
        p1 = p2;
        p2 = point;
    }

    if mode == Mode::AreaOdd || (mode == Mode::Line && step == 3) {
        path.close();
    }
}

/// Draws a Bézier curve for a three-point basis spline.
fn cardinal_point(path: &mut SvgPath, p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, k: f64) {
    path.curve_to(p1 + (p2 - p0) * k, p2 + (p1 - p3) * k, p2);
}

/// Linear path function.
fn linear(
    path: &mut SvgPath,
    points: &mut dyn DoubleSizeIterator<Item = Vec2>,
    mode: Mode,
    _: Option<f64>,
) {
    let mut step = 0;

    for point in points {
        match step {
            0 => {
                step += 1;
                if mode == Mode::AreaOdd {
                    path.line_to(point);
                } else {
                    path.move_to(point);
                }
            }
            1 => {
                step += 1;
                path.line_to(point);
            }
            _ => {
                path.line_to(point);
            }
        }
    }

    if mode == Mode::AreaOdd || (mode == Mode::Line && step == 1) {
        path.close();
    }
}

/// Closed linear path function.
fn linear_closed(
    path: &mut SvgPath,
    points: &mut dyn DoubleSizeIterator<Item = Vec2>,
    _: Mode,
    _: Option<f64>,
) {
    let has_points = if let Some(point) = points.next() {
        path.move_to(point);
        true
    } else {
        false
    };

    for point in points {
        path.line_to(point);
    }

    if has_points {
        path.close();
    }
}

/// Cubic spline that preserves monotonicity.
// TODO: This output does not appear to precisely match that from D3 v3; this
// generator version of the algorithm is from d3-shape and is newer, so it may
// be a transliteration bug, or it may be a change to the algorithm.
fn monotone(
    path: &mut SvgPath,
    points: &mut dyn DoubleSizeIterator<Item = Vec2>,
    mode: Mode,
    _: Option<f64>,
) {
    /// Calculate a one-sided slope.
    fn slope2(p0: Vec2, p1: Vec2, t: f64) -> f64 {
        let h = p1.x - p0.x;
        if h == 0.0 {
            t
        } else {
            (3.0 * (p1.y - p0.y) / h - t) / 2.0
        }
    }

    /// Calculate the slopes of the tangents (Hermite-type interpolation) based
    /// on the following paper: Steffen, M. 1990. A Simple Method for Monotonic
    /// Interpolation in One Dimension. Astronomy and Astrophysics, Vol. 239,
    /// NO. NOV(II), P. 443, 1990.
    fn slope3(p0: Vec2, p1: Vec2, p2: Vec2) -> f64 {
        #[inline]
        fn div_inf(n: f64, d0: f64, d1: f64) -> f64 {
            if d0 == 0.0 {
                d1.signum() * f64::INFINITY
            } else {
                n / d0
            }
        }

        let h0 = p1.x - p0.x;
        let h1 = p2.x - p1.x;
        let s0 = div_inf(p1.y - p0.y, h0, h1);
        let s1 = div_inf(p2.y - p1.y, h1, h0);
        let h = h0 + h1;
        let p = if h == 0.0 {
            f64::INFINITY
        } else {
            (s0 * h1 + s1 * h0) / h
        };
        let s = (s0.signum() + s1.signum()) * s0.abs().min(s1.abs()).min(0.5 * p.abs());
        if s.is_nan() { 0.0 } else { s }
    }

    let mut step = 0;
    let (mut p0, mut t0, mut t1) = <_>::default();
    // Since there is a condition check for every loop it is important that this
    // point not be initialised in a state where it might inadvertently match
    let mut p1 = Vec2::invalid();

    for point in points {
        if point == p1 {
            // Coincident points cause infinite slopes which break the curve
            continue;
        }
        match step {
            0 => {
                step += 1;
                if mode == Mode::AreaOdd {
                    path.line_to(point);
                } else {
                    path.move_to(point);
                }
            }
            1 => {
                step += 1;
            }
            2 => {
                step += 1;
                t1 = slope3(p0, p1, point);
                let t0 = slope2(p0, p1, t1);
                monotone_point(path, p0, p1, t0, t1);
            }
            _ => {
                t1 = slope3(p0, p1, point);
                monotone_point(path, p0, p1, t0, t1);
            }
        }
        p0 = p1;
        p1 = point;
        t0 = t1;
    }

    match step {
        2 => {
            path.line_to(p1);
        }
        3 => monotone_point(path, p0, p1, t0, slope2(p0, p1, t0)),
        _ => {}
    }

    if mode == Mode::AreaOdd || (mode == Mode::Line && step == 1) {
        path.close();
    }
}

/// Draws a Bézier curve for a monotone spline.
fn monotone_point(path: &mut SvgPath, p0: Vec2, p1: Vec2, t0: f64, t1: f64) {
    let dx = (p1.x - p0.x) / 3.0;
    path.curve_to(
        Vec2::new(p0.x + dx, p0.y + dx * t0),
        Vec2::new(p1.x - dx, p1.y - dx * t1),
        p1,
    );
}

/// Step function drawing strategy.
trait StepStrategy {
    /// The reverse strategy.
    type Reverse: StepStrategy;

    /// Whether a line should be drawn between a set of two points.
    const END_WITH_LINE: bool;

    /// Draws a step.
    fn point(path: &mut SvgPath, p0: Vec2, p1: Vec2);
}

/// Step strategy to step at the next point.
struct StepAfter;
impl StepStrategy for StepAfter {
    type Reverse = StepBefore;

    const END_WITH_LINE: bool = false;

    #[inline]
    fn point(path: &mut SvgPath, _: Vec2, p1: Vec2) {
        path.horizontal_to(p1.x);
        path.vertical_to(p1.y);
    }
}

/// Step strategy to step at the previous point.
struct StepBefore;
impl StepStrategy for StepBefore {
    type Reverse = StepAfter;

    const END_WITH_LINE: bool = false;

    #[inline]
    fn point(path: &mut SvgPath, _: Vec2, p1: Vec2) {
        path.vertical_to(p1.y);
        path.horizontal_to(p1.x);
    }
}

/// Step strategy to step at the midpoint of two points.
struct StepMid;
impl StepStrategy for StepMid {
    type Reverse = StepMid;

    const END_WITH_LINE: bool = true;

    #[inline]
    fn point(path: &mut SvgPath, p0: Vec2, p1: Vec2) {
        path.horizontal_to(p0.x.midpoint(p1.x));
        path.vertical_to(p1.y);
    }
}

/// Step function.
fn step<S: StepStrategy>(
    path: &mut SvgPath,
    points: &mut dyn DoubleSizeIterator<Item = Vec2>,
    mode: Mode,
    _: Option<f64>,
) {
    let mut step = 0;
    let mut p0 = <_>::default();

    for point in points {
        match step {
            0 => {
                step += 1;
                if mode == Mode::AreaOdd {
                    path.line_to(point);
                } else {
                    path.move_to(point);
                }
            }
            1 => {
                step += 1;
                if mode == Mode::AreaOdd {
                    S::Reverse::point(path, p0, point);
                } else {
                    S::point(path, p0, point);
                }
            }
            _ => {
                if mode == Mode::AreaOdd {
                    S::Reverse::point(path, p0, point);
                } else {
                    S::point(path, p0, point);
                }
            }
        }
        p0 = point;
    }

    if S::END_WITH_LINE && step == 2 {
        path.line_to(p0);
    }

    if mode == Mode::AreaOdd || (mode == Mode::Line && step == 1) {
        path.close();
    }
}
