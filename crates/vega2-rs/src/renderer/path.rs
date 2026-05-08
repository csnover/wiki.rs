//! Types and functions for drawing SVG paths.

use super::{super::EPSILON, ValueDisplay as _, Vec2};
use core::{
    cmp::Ordering,
    f64::consts::{FRAC_PI_2, PI, TAU},
    fmt::Write as _,
};

/// An SVG path command.
#[derive(Clone, Copy, Debug)]
pub(super) enum Command {
    /// Relative elliptical arc.
    Arc {
        /// The radius of the arc.
        _radius: (f64, f64),
        /// The rotation of the arc, in degrees, relative to the x-axis.
        _angle: f64,
        /// If true, draw the arc with the larger angle.
        _large: bool,
        /// If true, draw the clockwise arc.
        _sweep: bool,
        /// The final coordinate.
        point: (f64, f64),
    },
    /// Absolute elliptical arc.
    ArcTo {
        /// The radius of the arc.
        _radius: (f64, f64),
        /// The rotation of the arc, in degrees, relative to the x-axis.
        _angle: f64,
        /// If true, draw the arc with the larger angle.
        _large: bool,
        /// If true, draw the clockwise arc.
        _sweep: bool,
        /// The final coordinate.
        point: (f64, f64),
    },
    /// Close the path.
    Close,
    /// Horizontal line to absolute x-coordinate.
    HorizontalTo(f64),
    /// Line to absolute `(x, y)` coordinate.
    LineTo(f64, f64),
    /// Move pen to relative `(x, y)` coordinate.
    Move(f64, f64),
    /// Move pen to absolute `(x, y)` coordinate.
    MoveTo(f64, f64),
    /// Line to absolute y-coordinate.
    VerticalTo(f64),
}

/// An SVG path writer.
pub(super) struct SvgPath {
    /// The x-position of the pen.
    x: f64,
    /// The y-position of the pen.
    y: f64,
    /// The accumulator.
    data: String,
}

impl Default for SvgPath {
    fn default() -> Self {
        Self {
            x: f64::INFINITY,
            y: f64::INFINITY,
            data: <_>::default(),
        }
    }
}

impl SvgPath {
    /// Draws an absolute elliptical arc curve around the origin `c` with radius
    /// `r` starting from angle `a_start` and ending at `a_end` in `clockwise`
    /// direction.
    pub fn arc(&mut self, c: Vec2, r: f64, a_start: f64, a_end: f64, clockwise: bool) -> &mut Self {
        // SPDX-SnippetBegin
        // SPDX-License-Identifier: ISC
        // SPDX-SnippetComment: Adapted from d3-path 3.1 by Mike Bostock
        assert!(r >= 0.0, "negative radius: {r}");

        let p_start = Vec2::polar(a_start, r);
        let start = c + p_start;
        let a_len = if clockwise {
            a_end - a_start
        } else {
            a_start - a_end
        };

        if self.data.is_empty() {
            self.move_to(start);
        } else if (self.x - start.x).abs() > EPSILON || (self.y - start.y).abs() > EPSILON {
            // `c` is not coincident with the previous point
            self.line_to(start);
        }

        if r == 0.0 {
            // Empty arc
            return self;
        }

        let a_len = if a_len < 0.0 {
            // Flip incorrect angle direction
            a_len % TAU + TAU
        } else {
            a_len
        };

        if a_len > TAU_EPSILON {
            // Arc is a complete circle
            let Vec2 { x: x0, y: y0 } = start;
            let Vec2 { x: x1, y: y1 } = c - p_start;
            let cw = i32::from(clockwise);
            let _ = write!(
                self.data,
                "A{r},{r},0,1,{cw},{x1},{y1}A{r},{r},0,1,{cw},{x0},{y0}",
                r = r.v(),
                x0 = x0.v(),
                y0 = y0.v(),
                x1 = x1.v(),
                y1 = y1.v(),
            );
            self.x = x0;
            self.y = y0;
        } else if a_len > EPSILON {
            // Arc is non-empty
            let large_arc = i32::from(a_len >= PI);
            let cw = i32::from(clockwise);
            let Vec2 { x, y } = c + Vec2::polar(a_end, r);
            let _ = write!(
                self.data,
                "A{r},{r},0,{large_arc},{cw},{x},{y}",
                r = r.v(),
                x = x.v(),
                y = y.v()
            );
            self.x = x;
            self.y = y;
        }
        // SPDX-SnippetEnd

        self
    }

    /// Closes the path.
    pub fn close(&mut self) -> &mut Self {
        let _ = write!(self.data, "Z");
        self
    }

    /// Draws a cubic Bézier curve to the absolute position `(x, y)` using the
    /// control points `(x1, y1)` and `(x2, y2)`.
    pub fn curve_to(
        &mut self,
        Vec2 { x: x1, y: y1 }: Vec2,
        Vec2 { x: x2, y: y2 }: Vec2,
        Vec2 { x, y }: Vec2,
    ) -> &mut Self {
        let _ = write!(
            self.data,
            "C{x1},{y1},{x2},{y2},{x},{y}",
            x1 = x1.v(),
            y1 = y1.v(),
            x2 = x2.v(),
            y2 = y2.v(),
            x = x.v(),
            y = y.v()
        );
        self.x = x;
        self.y = y;
        self
    }

    /// Returns the SVG path, consuming this object.
    pub fn finish(self) -> String {
        self.data
    }

    /// Moves the pen to the absolute x-position.
    pub fn horizontal_to(&mut self, x: f64) -> &mut Self {
        if (self.x - x).abs() > EPSILON {
            let _ = write!(self.data, "H{x}", x = x.v());
            self.x = x;
        }
        self
    }

    /// Draws a line to the absolute position `(x, y)`.
    pub fn line_to(&mut self, Vec2 { x, y }: Vec2) -> &mut Self {
        if (self.x - x).abs() > EPSILON || (self.y - y).abs() > EPSILON {
            let _ = write!(self.data, "L{x},{y}", x = x.v(), y = y.v());
            self.x = x;
            self.y = y;
        }
        self
    }

    /// Moves the pen to the absolute position `(x, y)`.
    pub fn move_to(&mut self, Vec2 { x, y }: Vec2) -> &mut Self {
        if (self.x - x).abs() > EPSILON || (self.y - y).abs() > EPSILON {
            let _ = write!(self.data, "M{x},{y}", x = x.v(), y = y.v());
            self.x = x;
            self.y = y;
        }
        self
    }

    /// Moves the pen to the absolute y-position.
    pub fn vertical_to(&mut self, y: f64) -> &mut Self {
        if (self.y - y).abs() > EPSILON {
            let _ = write!(self.data, "V{y}", y = y.v());
            self.y = y;
        }
        self
    }
}

/// Iterator over an SVG path string.
pub(super) struct SvgPathIterator<'a>(&'a str);

impl<'a> SvgPathIterator<'a> {
    /// Creates a new iterator over the given path string.
    #[inline]
    pub fn new(path: &'a str) -> Self {
        Self(path)
    }
}

impl Iterator for SvgPathIterator<'_> {
    type Item = Command;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            return None;
        }

        let (command, next) = svg_path::command(self.0).unwrap();
        self.0 = &self.0[next..];
        Some(command)
    }
}

impl core::iter::FusedIterator for SvgPathIterator<'_> {}

/// Tau epsilon.
const TAU_EPSILON: f64 = TAU - EPSILON;

// SPDX-SnippetBegin
// SPDX-License-Identifier: ISC
// SPDX-SnippetComment: Adapted from d3-shape 3.2 by Mike Bostock

/// Creates an SVG path for an arc shape with the given start and end angles and
/// inner and outer radii.
pub(super) fn arc_path(
    start_angle: f64,
    end_angle: f64,
    inner_radius: f64,
    outer_radius: f64,
) -> String {
    let (r_inner, r_outer) = (
        inner_radius.min(outer_radius),
        outer_radius.max(inner_radius),
    );
    let (a_start, a_end) = (start_angle - FRAC_PI_2, end_angle - FRAC_PI_2);
    let a_delta = (a_end - a_start).abs();
    let cw = a_end > a_start;
    let mut context = SvgPath::default();

    if r_outer.partial_cmp(&EPSILON) != Some(Ordering::Greater) {
        // A point
        context.move_to(Vec2::zero());
    } else if a_delta > TAU_EPSILON {
        // A circle or annulus
        context.move_to(Vec2::polar(a_start, r_outer));
        context.arc(Vec2::zero(), r_outer, a_start, a_end, cw);
        if r_inner > EPSILON {
            context.move_to(Vec2::polar(a_end, r_inner));
            context.arc(Vec2::zero(), r_inner, a_end, a_start, !cw);
        }
    } else {
        // A circular or annular sector
        let r_corner = 0.0_f64.min((r_outer - r_inner).abs() / 2.0);
        let r_corner_inner;
        let r_corner_outer;
        let p00;
        let p01 = Vec2::polar(a_start, r_outer);
        let p10 = Vec2::polar(a_end, r_inner);
        let p11;
        if r_corner > EPSILON {
            // Apply rounded corners
            p00 = Vec2::polar(a_start, r_inner);
            p11 = Vec2::polar(a_end, r_outer);

            if a_delta < PI {
                // Restrict the corner radius according to the sector angle.
                if let Some(oc) = intersect(p01, p00, p11, p10) {
                    let a = p01 - oc;
                    let b = p11 - oc;
                    let kc = {
                        let ab_dot = a.dot(b);
                        let ab_len = a.len() * b.len();
                        1.0 / ((ab_dot / ab_len).acos() / 2.0).sin()
                    };
                    let lc = oc.len();
                    r_corner_inner = r_corner.min((r_inner - lc) / (kc - 1.0));
                    r_corner_outer = r_corner.min((r_outer - lc) / (kc + 1.0));
                } else {
                    // If this intersection fails, it’s probably because the arc
                    // is too small, so disable the corner radius entirely.
                    r_corner_inner = 0.0;
                    r_corner_outer = 0.0;
                }
            } else {
                r_corner_inner = r_corner;
                r_corner_outer = r_corner;
            }
        } else {
            p11 = Vec2::zero();
            p00 = Vec2::zero();
            r_corner_inner = r_corner;
            r_corner_outer = r_corner;
        }

        if a_delta.partial_cmp(&EPSILON) != Some(Ordering::Greater) {
            // Sector collapsed to a line
            context.move_to(p01);
        } else if r_corner_outer > EPSILON {
            // Sector’s outer ring has rounded corners
            let t0 = corner_tangents(p00, p01, r_outer, r_corner_outer, cw);
            let t1 = corner_tangents(p11, p10, r_outer, r_corner_outer, cw);

            context.move_to(t0.c + t0.p0);
            draw_arc_sector(
                &mut context,
                &t0,
                &t1,
                r_outer,
                r_corner,
                r_corner_outer,
                cw,
                !cw,
            );
        } else {
            // Outer ring is just a circular arc
            context.move_to(p01);
            context.arc(<_>::default(), r_outer, a_start, a_end, cw);
        }

        if r_inner.partial_cmp(&EPSILON) != Some(Ordering::Greater)
            || a_delta.partial_cmp(&EPSILON) != Some(Ordering::Greater)
        {
            // No inner ring, and it’s a circular sector, or it’s an annular
            // sector that collapsed due to padding
            context.line_to(p10);
        } else if r_corner_inner > EPSILON {
            // Sector’s inner ring (or point) has rounded corners
            let t0 = corner_tangents(p10, p11, r_inner, -r_corner_inner, cw);
            let t1 = corner_tangents(p01, p00, r_inner, -r_corner_inner, cw);

            context.line_to(t0.c + t0.p0);
            draw_arc_sector(
                &mut context,
                &t0,
                &t1,
                r_inner,
                r_corner,
                r_corner_inner,
                cw,
                cw,
            );
        } else {
            // Inner ring is just a circular arc
            context.arc(Vec2::zero(), r_inner, a_end, a_start, !cw);
        }
    }

    context.close();
    context.finish()
}

/// Computes the perpendicular offset line of length `rc`.
/// <http://mathworld.wolfram.com/Circle-LineIntersection.html>
fn corner_tangents(p0: Vec2, p1: Vec2, r1: f64, rc: f64, cw: bool) -> Intersection {
    let o = {
        let p_diff = p0 - p1;
        let o_len = if cw { -rc } else { rc } / p_diff.len();
        Vec2::new(o_len * p_diff.y, -o_len * p_diff.x)
    };

    let r = r1 - rc;

    let (c0, c1, p_mid) = {
        let (p0, p1) = (p0 + o, p1 + o);

        let p_diff = p1 - p0;
        let d2 = p_diff.square_len();
        let cross = p0.cross(p1);

        let pd = {
            let d = p_diff.y.signum() * (r * r * d2 - cross * cross).sqrt().max(0.0);
            p_diff * d
        };

        let (c0, c1) = {
            let cdx = cross * p_diff.y;
            let cdy = -cross * p_diff.x;
            let c0 = Vec2::new((cdy - pd.x) / d2, (cdx - pd.y) / d2);
            let c1 = Vec2::new((cdy + pd.x) / d2, (cdx + pd.y) / d2);
            (c0, c1)
        };

        (c0, c1, p0.mid(p1))
    };

    let c = if (c0 - p_mid).square_len() > (c1 - p_mid).square_len() {
        c1
    } else {
        c0
    };

    Intersection {
        c,
        p0: -o,
        p1: c * (r1 / r - 1.0),
    }
}

/// Draws a sector of an arc path using the given tangents, radius, corner
/// radii, and sweep directions.
#[expect(clippy::too_many_arguments, reason = "there just are")]
fn draw_arc_sector(
    context: &mut SvgPath,
    t0: &Intersection,
    t1: &Intersection,
    r: f64,
    r_corner: f64,
    r_corner_arc: f64,
    cw_first: bool,
    cw_second: bool,
) {
    let a0 = t0.p0.angle();
    let a1 = t1.p0.angle();
    context.arc(t0.c, r_corner_arc, a0, a1, cw_first);
    // If the corners merged then there is no more arc to draw
    if r_corner_arc >= r_corner {
        let a0 = (t0.c + t0.p1).angle();
        let a1 = (t1.c + t1.p1).angle();
        context.arc(Vec2::zero(), r, a0, a1, !cw_second);
        context.arc(t1.c, r_corner_arc, t1.p1.angle(), t1.p0.angle(), cw_first);
    }
}

/// Calculates the intersection point of two lines.
fn intersect(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) -> Option<Vec2> {
    let dx1 = p1 - p0;
    let dx2 = p3 - p2;
    let t = dx1.cross(dx2);
    (t * t >= EPSILON).then(|| {
        let t = (dx2.x * (p0.y - p2.y) - dx2.y * (p0.x - p2.x)) / t;
        (p0.x + t * dx1.x, p0.y + t * dx1.y).into()
    })
}

// SPDX-SnippetEnd

/// A circle-line intersection.
struct Intersection {
    /// The centre of the circle.
    c: Vec2,
    /// The first intersection point.
    p0: Vec2,
    /// The second intersection point.
    p1: Vec2,
}

peg::parser! {grammar svg_path() for str {
    #[no_eof]
    pub rule command() -> (Command, usize)
    = c:any() _ e:position!()
    { (c, e) }

    rule any() -> Command
    = arc()
    / arc_to()
    / close()
    / horizontal_to()
    / line_to()
    / move()
    / move_to()
    / vertical_to()

    rule arc() -> Command
    = "a" _ rx:number() "," ry:number() "," angle:number() "," large:bool() "," sweep:bool() "," x:number() "," y:number()
    { Command::Arc { _radius: (rx, ry), _angle: angle, _large: large, _sweep: sweep, point: (x, y) } }

    rule arc_to() -> Command
    = "A" _ rx:number() "," ry:number() "," angle:number() "," large:bool() "," sweep:bool() "," x:number() "," y:number()
    { Command::ArcTo { _radius: (rx, ry), _angle: angle, _large: large, _sweep: sweep, point: (x, y) } }

    rule close() -> Command
    = ['Z'|'z'] _
    { Command::Close }

    rule horizontal_to() -> Command
    = "H" _ x:number()
    { Command::HorizontalTo(x) }

    rule line_to() -> Command
    = "L" _ x:number() "," y:number()
    { Command::LineTo(x, y) }

    rule move() -> Command
    = "m" _ x:number() "," y:number()
    { Command::Move(x, y) }

    rule move_to() -> Command
    = "M" _ x:number() "," y:number()
    { Command::MoveTo(x, y) }

    rule vertical_to() -> Command
    = "V" _ y:number()
    { Command::VerticalTo(y) }

    rule _
    = [c if c.is_ascii_whitespace()]*

    rule number() -> f64
    = s:$(['0'..='9' | '.' | '-']+)
    {? s.parse().map_err(|_| "number") }

    rule bool() -> bool
    = "0" { false }
    / "1" { true }
}}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f64::consts::{FRAC_PI_2, PI, TAU};

    // SPDX-SnippetBegin
    // SPDX-License-Identifier: ISC
    // SPDX-SnippetComment: Adapted from d3-shape 3.2 by Mike Bostock

    #[test]
    fn null() {
        assert_eq!(arc_path(0.0, TAU, 0.0, 0.0), "M0,0Z");
        assert_eq!(arc_path(0.0, 0.0, 0.0, 0.0), "M0,0Z");
    }

    #[test]
    fn negative() {
        assert_eq!(
            arc_path(0.0, -FRAC_PI_2, 0.0, 100.0),
            "M0,-100A100,100,0,0,0,-100,0L0,0Z"
        );
    }

    #[test]
    fn clockwise() {
        assert_eq!(
            arc_path(0.0, TAU, 0.0, 100.0),
            "M0,-100A100,100,0,1,1,0,100A100,100,0,1,1,0,-100Z"
        );
        assert_eq!(
            arc_path(0.0, 3.0 * PI, 0.0, 100.0),
            "M0,-100A100,100,0,1,1,0,100A100,100,0,1,1,0,-100Z"
        );
        assert_eq!(
            arc_path(-2.0 * PI, 0.0, 0.0, 100.0),
            "M0,-100A100,100,0,1,1,0,100A100,100,0,1,1,0,-100Z"
        );
        assert_eq!(
            arc_path(-PI, PI, 0.0, 100.0),
            "M0,100A100,100,0,1,1,0,-100A100,100,0,1,1,0,100Z"
        );
        assert_eq!(
            arc_path(-3.0 * PI, 0.0, 0.0, 100.0),
            "M0,100A100,100,0,1,1,0,-100A100,100,0,1,1,0,100Z"
        );
    }

    #[test]
    fn anticlockwise() {
        assert_eq!(
            arc_path(0.0, -TAU, 0.0, 100.0),
            "M0,-100A100,100,0,1,0,0,100A100,100,0,1,0,0,-100Z"
        );
        assert_eq!(
            arc_path(0.0, -3.0 * PI, 0.0, 100.0),
            "M0,-100A100,100,0,1,0,0,100A100,100,0,1,0,0,-100Z"
        );
        assert_eq!(
            arc_path(TAU, 0.0, 0.0, 100.0),
            "M0,-100A100,100,0,1,0,0,100A100,100,0,1,0,0,-100Z"
        );
        assert_eq!(
            arc_path(PI, -PI, 0.0, 100.0),
            "M0,100A100,100,0,1,0,0,-100A100,100,0,1,0,0,100Z"
        );
        assert_eq!(
            arc_path(3.0 * PI, 0.0, 0.0, 100.0),
            "M0,100A100,100,0,1,0,0,-100A100,100,0,1,0,0,100Z"
        );
    }

    #[test]
    fn outer_cw_inner_acw() {
        // Note: The outer ring starts and ends at θ₀, but the inner ring starts and ends at θ₁.
        // Note: The outer ring is clockwise, but the inner ring is anticlockwise.
        assert_eq!(
            arc_path(0.0, 2.0 * PI, 50.0, 100.0),
            "M0,-100A100,100,0,1,1,0,100A100,100,0,1,1,0,-100M0,-50A50,50,0,1,0,0,50A50,50,0,1,0,0,-50Z"
        );
        assert_eq!(
            arc_path(0.0, 3.0 * PI, 50.0, 100.0),
            "M0,-100A100,100,0,1,1,0,100A100,100,0,1,1,0,-100M0,50A50,50,0,1,0,0,-50A50,50,0,1,0,0,50Z"
        );
        assert_eq!(
            arc_path(-2.0 * PI, 0.0, 50.0, 100.0),
            "M0,-100A100,100,0,1,1,0,100A100,100,0,1,1,0,-100M0,-50A50,50,0,1,0,0,50A50,50,0,1,0,0,-50Z"
        );
        assert_eq!(
            arc_path(-PI, PI, 50.0, 100.0),
            "M0,100A100,100,0,1,1,0,-100A100,100,0,1,1,0,100M0,50A50,50,0,1,0,0,-50A50,50,0,1,0,0,50Z"
        );
        assert_eq!(
            arc_path(-3.0 * PI, 0.0, 50.0, 100.0),
            "M0,100A100,100,0,1,1,0,-100A100,100,0,1,1,0,100M0,-50A50,50,0,1,0,0,50A50,50,0,1,0,0,-50Z"
        );
    }

    #[test]
    fn outer_acw_inner_cw() {
        // Note: The outer ring starts and ends at θ₀, but the inner ring starts and ends at θ₁.
        // Note: The outer ring is anticlockwise, but the inner ring is clockwise.
        assert_eq!(
            arc_path(0.0, -TAU, 50.0, 100.0),
            "M0,-100A100,100,0,1,0,0,100A100,100,0,1,0,0,-100M0,-50A50,50,0,1,1,0,50A50,50,0,1,1,0,-50Z"
        );
        assert_eq!(
            arc_path(0.0, -3.0 * PI, 50.0, 100.0),
            "M0,-100A100,100,0,1,0,0,100A100,100,0,1,0,0,-100M0,50A50,50,0,1,1,0,-50A50,50,0,1,1,0,50Z"
        );
        assert_eq!(
            arc_path(TAU, 0.0, 50.0, 100.0),
            "M0,-100A100,100,0,1,0,0,100A100,100,0,1,0,0,-100M0,-50A50,50,0,1,1,0,50A50,50,0,1,1,0,-50Z"
        );
        assert_eq!(
            arc_path(PI, -PI, 50.0, 100.0),
            "M0,100A100,100,0,1,0,0,-100A100,100,0,1,0,0,100M0,50A50,50,0,1,1,0,-50A50,50,0,1,1,0,50Z"
        );
        assert_eq!(
            arc_path(3.0 * PI, 0.0, 50.0, 100.0),
            "M0,100A100,100,0,1,0,0,-100A100,100,0,1,0,0,100M0,-50A50,50,0,1,1,0,50A50,50,0,1,1,0,-50Z"
        );
    }

    #[test]
    fn small_cw() {
        assert_eq!(
            arc_path(0.0, FRAC_PI_2, 0.0, 100.0),
            "M0,-100A100,100,0,0,1,100,0L0,0Z"
        );
        assert_eq!(
            arc_path(TAU, 5.0 * FRAC_PI_2, 0.0, 100.0),
            "M0,-100A100,100,0,0,1,100,0L0,0Z"
        );
        assert_eq!(
            arc_path(-PI, -FRAC_PI_2, 0.0, 100.0),
            "M0,100A100,100,0,0,1,-100,0L0,0Z"
        );
    }

    #[test]
    fn small_acw() {
        assert_eq!(
            arc_path(0.0, -FRAC_PI_2, 0.0, 100.0),
            "M0,-100A100,100,0,0,0,-100,0L0,0Z"
        );
        assert_eq!(
            arc_path(-TAU, -5.0 * FRAC_PI_2, 0.0, 100.0),
            "M0,-100A100,100,0,0,0,-100,0L0,0Z"
        );
        assert_eq!(
            arc_path(PI, FRAC_PI_2, 0.0, 100.0),
            "M0,100A100,100,0,0,0,100,0L0,0Z"
        );
    }

    #[test]
    fn large_cw() {
        assert_eq!(
            arc_path(0.0, 3.0 * FRAC_PI_2, 0.0, 100.0),
            "M0,-100A100,100,0,1,1,-100,0L0,0Z"
        );
        assert_eq!(
            arc_path(TAU, 7.0 * FRAC_PI_2, 0.0, 100.0),
            "M0,-100A100,100,0,1,1,-100,0L0,0Z"
        );
        assert_eq!(
            arc_path(-PI, FRAC_PI_2, 0.0, 100.0),
            "M0,100A100,100,0,1,1,100,0L0,0Z"
        );
    }

    #[test]
    fn large_acw() {
        assert_eq!(
            arc_path(0.0, -3.0 * FRAC_PI_2, 0.0, 100.0),
            "M0,-100A100,100,0,1,0,100,0L0,0Z"
        );
        assert_eq!(
            arc_path(-TAU, -7.0 * FRAC_PI_2, 0.0, 100.0),
            "M0,-100A100,100,0,1,0,100,0L0,0Z"
        );
        assert_eq!(
            arc_path(PI, -FRAC_PI_2, 0.0, 100.0),
            "M0,100A100,100,0,1,0,-100,0L0,0Z"
        );
    }

    #[test]
    fn small_outer_cw_inner_acw() {
        assert_eq!(
            arc_path(0.0, FRAC_PI_2, 50.0, 100.0),
            "M0,-100A100,100,0,0,1,100,0L50,0A50,50,0,0,0,0,-50Z"
        );
        assert_eq!(
            arc_path(TAU, 5.0 * FRAC_PI_2, 50.0, 100.0),
            "M0,-100A100,100,0,0,1,100,0L50,0A50,50,0,0,0,0,-50Z"
        );
        assert_eq!(
            arc_path(-PI, -FRAC_PI_2, 50.0, 100.0),
            "M0,100A100,100,0,0,1,-100,0L-50,0A50,50,0,0,0,0,50Z"
        );
    }

    #[test]
    fn small_outer_acw_inner_cw() {
        assert_eq!(
            arc_path(0.0, -FRAC_PI_2, 50.0, 100.0),
            "M0,-100A100,100,0,0,0,-100,0L-50,0A50,50,0,0,1,0,-50Z"
        );
        assert_eq!(
            arc_path(-TAU, -5.0 * FRAC_PI_2, 50.0, 100.0),
            "M0,-100A100,100,0,0,0,-100,0L-50,0A50,50,0,0,1,0,-50Z"
        );
        assert_eq!(
            arc_path(PI, FRAC_PI_2, 50.0, 100.0),
            "M0,100A100,100,0,0,0,100,0L50,0A50,50,0,0,1,0,50Z"
        );
    }

    #[test]
    fn large_outer_cw_inner_acw() {
        assert_eq!(
            arc_path(0.0, 3.0 * FRAC_PI_2, 50.0, 100.0),
            "M0,-100A100,100,0,1,1,-100,0L-50,0A50,50,0,1,0,0,-50Z"
        );
        assert_eq!(
            arc_path(TAU, 7.0 * FRAC_PI_2, 50.0, 100.0),
            "M0,-100A100,100,0,1,1,-100,0L-50,0A50,50,0,1,0,0,-50Z"
        );
        assert_eq!(
            arc_path(-PI, FRAC_PI_2, 50.0, 100.0),
            "M0,100A100,100,0,1,1,100,0L50,0A50,50,0,1,0,0,50Z"
        );
    }

    #[test]
    fn large_outer_acw_inner_cw() {
        assert_eq!(
            arc_path(0.0, -3.0 * FRAC_PI_2, 50.0, 100.0),
            "M0,-100A100,100,0,1,0,100,0L50,0A50,50,0,1,1,0,-50Z"
        );
        assert_eq!(
            arc_path(-TAU, -7.0 * FRAC_PI_2, 50.0, 100.0),
            "M0,-100A100,100,0,1,0,100,0L50,0A50,50,0,1,1,0,-50Z"
        );
        assert_eq!(
            arc_path(PI, -FRAC_PI_2, 50.0, 100.0),
            "M0,100A100,100,0,1,0,-100,0L-50,0A50,50,0,1,1,0,50Z"
        );
    }

    // SPDX-SnippetEnd
}
