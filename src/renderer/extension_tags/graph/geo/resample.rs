//! Line resamplers.

use super::{
    super::{
        EPSILON,
        renderer::{Vec2, Vec3},
    },
    Listener, Projector,
};

/// A resampler which passes points without any interpolation.
pub(super) struct NullResampler<'b, L: Listener> {
    /// The output event listener.
    listener: L,
    /// The source projector.
    projector: &'b Projector,
}

impl<'b, L: Listener> NullResampler<'b, L> {
    /// Creates a new `NullResampler` with the given projector and output
    /// listener.
    pub fn new(projector: &'b Projector, listener: L) -> Self {
        Self {
            listener,
            projector,
        }
    }
}

impl<L> Listener for NullResampler<'_, L>
where
    L: Listener,
{
    fn line_end(&mut self) {
        self.listener.line_end();
    }

    fn line_start(&mut self) {
        self.listener.line_start();
    }

    fn point(&mut self, point: Vec2) {
        self.listener.point(self.projector.project_resample(point));
    }

    fn polygon_end(&mut self) {
        self.listener.polygon_end();
    }

    fn polygon_start(&mut self) {
        self.listener.polygon_start();
    }

    fn sphere(&mut self) {
        self.listener.sphere();
    }
}

/// An interpolating point resampler.
pub(super) struct Resampler<'b, L: Listener> {
    /// The current [`Listener::line_end`] handler.
    line_end: fn(&mut Self),
    /// The current [`Listener::line_start`] handler.
    line_start: fn(&mut Self),
    /// The output event listener.
    listener: L,
    /// The current [`Listener::point`] handler.
    point: fn(&mut Self, Vec2),
    /// The threshold for the projection’s adaptive resample, in pixels.
    precision: f64,
    /// The previous projected point from a polygon without its azimuthal axis.
    prev_poly_projected: Vec2,
    /// The previous raw point from a polygon.
    prev_poly_raw: Vec2,
    /// The previous raw point from a polygon in spherical coordinates.
    prev_poly_raw_spherical: Vec3,
    /// The previous projected point in spherical coordinates.
    prev_projected: Vec3,
    /// The previous raw point in spherical coordinates.
    prev_raw: Vec3,
    /// The parent projector.
    projector: &'b Projector,
}

impl<'b, L: Listener> Resampler<'b, L> {
    /// Creates a new `Resampler` with the given projector, output listener, and
    /// precision.
    pub fn new(projector: &'b Projector, listener: L, precision: Option<f64>) -> Self {
        assert!(
            precision != Some(0.0),
            "for the love of god, use the non-resampling resampler"
        );
        Self {
            line_start: resampler::line_start,
            line_end: resampler::line_end,
            listener,
            point: resampler::point,
            precision: precision.map_or(0.5, |value| value * value),
            prev_poly_projected: <_>::default(),
            prev_poly_raw: <_>::default(),
            prev_poly_raw_spherical: <_>::default(),
            prev_projected: <_>::default(),
            prev_raw: <_>::default(),
            projector,
        }
    }

    /// Recursively decimates a curve using the iterative end-point fit
    /// algorithm.
    fn resample_line_to(&mut self, p0: Vec3, p0a: Vec3, p1: Vec3, p1a: Vec3, depth: i32) {
        if depth == 0 {
            return;
        }

        let d = p1.xy() - p0.xy();
        let d2 = d.square_len();
        if d2 > 4.0 * self.precision {
            let abc = p0a + p1a;
            let m = abc.len();
            let geo_p = {
                let c = abc.z / m;
                let lat = if (c.abs() - 1.0).abs() < EPSILON || (p0.z - p1.z).abs() < EPSILON {
                    p0.z.midpoint(p1.z)
                } else {
                    abc.xy().angle()
                };
                Vec2::new(lat, c.asin())
            };
            let p = self.projector.project_resample(geo_p);
            let dx2 = p - p0.xy();
            let dz = d.yx().cross(dx2.yx());
            if dz * dz / d2 > self.precision // perpendicular projected distance
                || (d.dot(dx2) / d2 - 0.5).abs() > 0.3 // midpoint close to an end
                || p0a.dot(p1a) < 30.0_f64.to_radians().cos()
            // angular distance
            {
                self.resample_line_to(p0, p0a, p1, abc / m, depth - 1);
                self.listener.point(p);
                self.resample_line_to(p.extend(geo_p.x), abc, p1, p1a, depth - 1);
            }
        }
    }
}

impl<L> Listener for Resampler<'_, L>
where
    L: Listener,
{
    fn line_end(&mut self) {
        (self.line_end)(self);
    }

    fn line_start(&mut self) {
        (self.line_start)(self);
    }

    fn point(&mut self, point: Vec2) {
        (self.point)(self, point);
    }

    fn polygon_end(&mut self) {
        self.listener.polygon_end();
        self.line_start = resampler::line_start;
    }

    fn polygon_start(&mut self) {
        self.listener.polygon_start();
        self.line_start = resampler::ring_start;
    }

    fn sphere(&mut self) {}
}

/// Swappable listener functions for [`Resampler`].
mod resampler {
    use super::{Listener, Resampler, Vec2, Vec3};

    /// The maximum amount of allowable recursion.
    const MAX_DEPTH: i32 = 16;

    /// The [`Listener::line_end`] handler used when drawing a non-polygonal
    /// line segment.
    pub(super) fn line_end<L>(r: &mut Resampler<'_, L>)
    where
        L: Listener,
    {
        r.point = point;
        r.listener.line_end();
    }

    /// The [`Listener::point`] handler used when drawing a point on a line.
    pub(super) fn line_point<L>(r: &mut Resampler<'_, L>, point: Vec2)
    where
        L: Listener,
    {
        let raw_spherical = Vec3::from_cartesian(point);
        let projected_point = r.projector.project_resample(point);
        let projected_spherical = projected_point.extend(point.x);
        r.resample_line_to(
            r.prev_projected,
            r.prev_raw,
            projected_spherical,
            raw_spherical,
            MAX_DEPTH,
        );
        r.prev_projected = projected_spherical;
        r.prev_raw = raw_spherical;
        r.listener.point(projected_point);
    }

    /// The [`Listener::line_start`] handler used when drawing any line segment.
    pub(super) fn line_start<L>(r: &mut Resampler<'_, L>)
    where
        L: Listener,
    {
        r.prev_projected.x = f64::NAN;
        r.point = line_point;
        r.listener.line_start();
    }

    /// The [`Listener::point`] handler used when drawing standalone points.
    pub(super) fn point<L>(r: &mut Resampler<'_, L>, point: Vec2)
    where
        L: Listener,
    {
        r.listener.point(r.projector.project_resample(point));
    }

    /// The [`Listener::line_end`] handler used when drawing a polygonal line
    /// segment.
    pub(super) fn ring_end<L>(r: &mut Resampler<'_, L>)
    where
        L: Listener,
    {
        r.resample_line_to(
            r.prev_projected,
            r.prev_raw,
            Vec3::with_z(r.prev_poly_projected, r.prev_poly_raw.x),
            r.prev_poly_raw_spherical,
            MAX_DEPTH,
        );
    }

    /// The [`Listener::point`] handler used when drawing points on a polygonal
    /// line segment.
    pub(super) fn ring_point<L>(r: &mut Resampler<'_, L>, point: Vec2)
    where
        L: Listener,
    {
        r.prev_poly_raw = point;
        line_point(r, point);
        r.prev_poly_projected = r.prev_projected.xy();
        r.prev_poly_raw_spherical = r.prev_raw;
        r.point = line_point;
    }

    /// The [`Listener::line_start`] handler used when drawing a polygonal line
    /// segment.
    pub(super) fn ring_start<L>(r: &mut Resampler<'_, L>)
    where
        L: Listener,
    {
        line_start(r);
        r.point = ring_point;
        r.line_end = ring_end;
    }
}
