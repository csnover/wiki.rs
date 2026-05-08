//! Path clipping types.

use super::{
    super::{
        EPSILON,
        renderer::{Rect, Vec2, Vec3},
    },
    Adder, Direction, Listener, Projector, clamp_asin,
};
use core::{
    cmp::Ordering,
    f64::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU},
};
use either::Either;

/// A trait for preclippers that can be used by [`Clip`] to perform spherical
/// clipping.
pub(super) trait Preclipper<L>: Listener
where
    L: Listener,
{
    /// Extra arguments to pass to [`Self::clip_line`].
    type CArgs;

    /// Extra arguments to pass to [`Self::interpolate`].
    type IArgs;

    /// Creates a new `Self` with the given output listener and arguments.
    fn clip_line(listener: L, args: Self::CArgs) -> Self;

    /// Draws an interpolated spherical `line` in a `direction` using the given
    /// `args`, emitting the result to `listener`. If `line` is `None`, draws a
    /// line along the entire clipping edge?
    fn interpolate(
        line: Option<(Vec2, Vec2)>,
        direction: Direction,
        listener: &mut L,
        args: &Self::IArgs,
    );

    /// Returns the clipping state for the line segment being processed.
    fn clean(&self) -> Clean;

    /// Returns the starting coordinate for a clipping operation.
    fn clip_start(&self) -> Vec2;

    /// Returns the extra data required to pass to [`Self::interpolate`].
    fn interpolate_args(&self) -> Self::IArgs;

    /// Returns true if the given point is considered visible.
    fn point_visible(&self, point: Vec2) -> bool;

    /// Returns a mutable reference to the output listener.
    fn listener(&mut self) -> &mut L;
}

/// A listener that buffers line segments internally instead of emitting them to
/// another listener.
#[derive(Default)]
pub(super) struct BufferListener {
    /// The buffered lines.
    lines: Vec<Vec<Vec2>>,
}

impl BufferListener {
    /// If the buffer contains more than one line, merges the first line onto
    /// the end of the last line.
    #[inline]
    fn rejoin(&mut self) {
        if self.lines.len() > 1 {
            let first = self.lines.remove(0);
            self.lines.last_mut().unwrap().extend(first);
        }
    }

    /// Takes the lines from the buffer.
    #[inline]
    fn take(&mut self) -> Vec<Vec<Vec2>> {
        core::mem::take(&mut self.lines)
    }
}

impl Listener for BufferListener {
    fn line_end(&mut self) {}

    fn line_start(&mut self) {
        self.lines.push(<_>::default());
    }

    fn point(&mut self, point: Vec2) {
        self.lines.last_mut().unwrap().push(point);
    }

    fn polygon_start(&mut self) {}

    fn polygon_end(&mut self) {}

    fn sphere(&mut self) {}
}

/// A generic spherical clipper used for preclipping.
pub(super) struct Clip<'b, C, D, L>
where
    C: Preclipper<L>,
    D: Preclipper<BufferListener>,
    L: Listener,
{
    /// An accumulator for clipped line segments of the polygon being processed.
    /// Raw, unclipped line segments are accumulated in [`Self::raw_polygon`].
    clipped_polygon: Vec<Vec<Vec<Vec2>>>,

    /// An accumulator for clipped and rotated points for the line segment being
    /// processed. Raw, unrotated points for the same line segment are
    /// accumulated in [`Self::raw_ring`], which is a homonym for “roaring” in
    /// some English accents. That last thing isn’t important, I just thought it
    /// was fun.
    clipped_ring: D,

    /// If true, the output listener has received a [`Listener::polygon_start`]
    /// message.
    in_polygon: bool,

    /// The current [`Listener::line_end`] handler.
    line_end: fn(&mut Self),

    /// The current [`Listener::line_start`] handler.
    line_start: fn(&mut Self),

    /// The output listener wrapped by a [`Preclipper`].
    listener: C,

    /// The current [`Listener::point`] handler.
    point: fn(&mut Self, point: Vec2),

    /// The parent projector.
    projector: &'b Projector,

    /// An accumulator for the raw, unrotated line segments of the polygon being
    /// processed, whose entire purpose is to be used once to learn whether
    /// [`Self::rotated_clip_start`] is inside the area of the polygon.
    raw_polygon: Vec<Vec<Vec2>>,

    /// An accumulator for all of the raw, unrotated points of the line segment
    /// being processed. Clipped and rotated points for the same line segment
    /// are accumulated in [`Self::clipped_ring`].
    raw_ring: Option<Vec<Vec2>>,

    /// The rotated starting coordinate for a clip operation.
    rotated_clip_start: Vec2,
}

impl<'b, C, D, L> Clip<'b, C, D, L>
where
    C: Preclipper<L>,
    C::CArgs: Copy,
    D: Preclipper<BufferListener, CArgs = C::CArgs>,
    L: Listener,
{
    /// Creates a new `Clip` with the given projector and output listener.
    pub fn new(projector: &'b Projector, listener: L, args: C::CArgs) -> Self {
        let listener = C::clip_line(listener, args);
        let rotated_clip_start = projector.rotate_invert(listener.clip_start());
        Self {
            clipped_polygon: <_>::default(),
            clipped_ring: D::clip_line(BufferListener::default(), args),
            in_polygon: <_>::default(),
            line_end: clip::line_end,
            line_start: clip::line_start,
            listener,
            point: clip::point,
            projector,
            raw_polygon: <_>::default(),
            raw_ring: <_>::default(),
            rotated_clip_start,
        }
    }
}

impl<C, D, L> Listener for Clip<'_, C, D, L>
where
    C: Preclipper<L>,
    D: Preclipper<BufferListener>,
    L: Listener,
{
    fn point(&mut self, point: Vec2) {
        (self.point)(self, point);
    }

    fn line_start(&mut self) {
        (self.line_start)(self);
    }

    fn line_end(&mut self) {
        (self.line_end)(self);
    }

    fn polygon_start(&mut self) {
        self.point = clip::point_ring;
        self.line_start = clip::ring_start;
        self.line_end = clip::ring_end;
        assert!(
            self.clipped_polygon.is_empty() && self.raw_polygon.is_empty(),
            "polygon_start calls cannot be nested"
        );
    }

    fn polygon_end(&mut self) {
        self.point = clip::point;
        self.line_start = clip::line_start;
        self.line_end = clip::line_end;
        let segments = self.clipped_polygon.drain(..).flatten().collect::<Vec<_>>();
        let clip_start_inside =
            inside_polygon_cartographic(&self.raw_polygon, self.rotated_clip_start);
        if !segments.is_empty() {
            if !self.in_polygon {
                self.listener.listener().polygon_start();
                self.in_polygon = true;
            }
            let args = self.listener.interpolate_args();
            clip_polygon(
                &segments,
                clip::sort,
                clip_start_inside,
                move |line, direction, listener| C::interpolate(line, direction, listener, &args),
                self.listener.listener(),
            );
        } else if clip_start_inside {
            if !self.in_polygon {
                self.listener.listener().polygon_start();
                self.in_polygon = true;
            }
            self.listener.listener().line_start();
            let args = self.listener.interpolate_args();
            C::interpolate(None, Direction::Forward, self.listener.listener(), &args);
            self.listener.listener().line_end();
        }
        if self.in_polygon {
            self.listener.listener().polygon_end();
            self.in_polygon = false;
        }
        self.raw_polygon.clear();
    }

    fn sphere(&mut self) {
        self.listener.listener().polygon_start();
        self.listener.listener().line_start();
        let args = self.listener.interpolate_args();
        C::interpolate(None, Direction::Forward, self.listener.listener(), &args);
        self.listener.listener().line_end();
        self.listener.listener().polygon_end();
    }
}

/// Swappable listener functions for [`Clip`].
mod clip {
    use super::{
        BufferListener, Clean, Clip, EPSILON, FRAC_PI_2, Listener, Ordering, Preclipper, Vec2,
    };

    /// The [`Listener::line_end`] handler used when drawing non-polygonal
    /// lines.
    pub(super) fn line_end<C, D, L>(c: &mut Clip<'_, C, D, L>)
    where
        C: Preclipper<L>,
        D: Preclipper<BufferListener>,
        L: Listener,
    {
        c.point = point;
        c.listener.line_end();
    }

    /// The [`Listener::line_start`] handler used when drawing non-polygonal
    /// lines.
    pub(super) fn line_start<C, D, L>(c: &mut Clip<'_, C, D, L>)
    where
        C: Preclipper<L>,
        D: Preclipper<BufferListener>,
        L: Listener,
    {
        c.point = point_line;
        c.listener.line_start();
    }

    /// The [`Listener::point`] handler used when drawing bare points.
    pub(super) fn point<C, D, L>(c: &mut Clip<'_, C, D, L>, point: Vec2)
    where
        C: Preclipper<L>,
        D: Preclipper<BufferListener>,
        L: Listener,
    {
        let point = c.projector.rotate(point);
        if c.listener.point_visible(point) {
            c.listener.listener().point(point);
        }
    }

    /// The [`Listener::point`] handler used when drawing points on a
    /// non-polygonal line segment.
    pub(super) fn point_line<C, D, L>(c: &mut Clip<'_, C, D, L>, point: Vec2)
    where
        C: Preclipper<L>,
        D: Preclipper<BufferListener>,
        L: Listener,
    {
        let point = c.projector.rotate(point);
        c.listener.point(point);
    }

    /// The [`Listener::point`] handler used when drawing points on a polygonal
    /// line segment.
    pub(super) fn point_ring<C, D, L>(c: &mut Clip<'_, C, D, L>, point: Vec2)
    where
        C: Preclipper<L>,
        D: Preclipper<BufferListener>,
        L: Listener,
    {
        c.raw_ring.as_mut().unwrap().push(point);
        let point = c.projector.rotate(point);
        c.clipped_ring.point(point);
    }

    /// The [`Listener::line_end`] handler used when drawing a line segment for
    /// a polygon.
    pub(super) fn ring_end<C, D, L>(c: &mut Clip<'_, C, D, L>)
    where
        C: Preclipper<L>,
        D: Preclipper<BufferListener>,
        L: Listener,
    {
        point_ring(c, *c.raw_ring.as_ref().unwrap().first().unwrap());
        c.clipped_ring.line_end();

        let clean = c.clipped_ring.clean();
        let mut ring = c.raw_ring.take().unwrap();
        ring.pop();
        c.raw_polygon.push(ring);

        // In D3, this is explicitly not treated as a bit flag but as a whole
        // value
        if clean == Clean::NEEDS_REJOIN {
            c.clipped_ring.listener().rejoin();
        }

        let ring_segments = c.clipped_ring.listener().take();
        if ring_segments.is_empty() {
            return;
        }

        if clean.contains(Clean::CLEAN) {
            let segment = &ring_segments[0];
            if segment.len() > 1 {
                if !c.in_polygon {
                    c.listener.listener().polygon_start();
                    c.in_polygon = true;
                }
                c.listener.listener().line_start();
                for point in segment {
                    c.listener.listener().point(*point);
                }
                c.listener.listener().line_end();
            }
        } else {
            let poly = ring_segments
                .into_iter()
                .filter(|segment| segment.len() > 1)
                .collect::<Vec<_>>();
            c.clipped_polygon.push(poly);
        }
    }

    /// The [`Listener::line_start`] handler used when drawing a line segment
    /// for a polygon.
    pub(super) fn ring_start<C, D, L>(c: &mut Clip<'_, C, D, L>)
    where
        C: Preclipper<L>,
        D: Preclipper<BufferListener>,
        L: Listener,
    {
        c.clipped_ring.line_start();
        c.raw_ring = Some(vec![]);
    }

    /// The comparator to use with [`clip_polygon`](super::fn@clip_polygon).
    pub(super) fn sort(a: Vec2, b: Vec2) -> Ordering {
        let a = if a.x < 0.0 {
            a.y - FRAC_PI_2 - EPSILON
        } else {
            FRAC_PI_2 - a.y
        };
        let b = if b.x < 0.0 {
            b.y - FRAC_PI_2 - EPSILON
        } else {
            FRAC_PI_2 - b.y
        };
        a.total_cmp(&b)
    }
}

/// A preclipper which cuts lines in half when they cross the antimeridian line.
pub(super) struct ClipAntimeridian<L>
where
    L: Listener,
{
    /// Whether the line segment being processed has crossed any lines and needs
    /// to go to counselling.
    clean: Clean,

    /// The output listener.
    listener: L,

    /// The previous point.
    prev_point: Vec2,

    /// The previous point’s precomputed, uh, signed pi. (Sorry, someone else
    /// who actually understands trig needs to be documenting this. I am doing
    /// my best to make almost totally undocumented single-letter-variable code
    /// intelligible.)
    prev_sx: f64,
}

impl<L> ClipAntimeridian<L>
where
    L: Listener,
{
    /// Creates a new `ClipAntimeridian` using the given output listener.
    fn new(listener: L) -> Self {
        Self {
            clean: <_>::default(),
            listener,
            prev_point: Vec2::invalid(),
            prev_sx: f64::NAN,
        }
    }

    /// Returns the intersection of the line segment `(a, b)` with the
    /// antimeridian line.
    fn intersect(a: Vec2, b: Vec2) -> f64 {
        let sinp0x_p1x = (a.x - b.x).sin();
        if sinp0x_p1x.abs() > EPSILON {
            let cosp0y = a.y.cos();
            let cosp1y = b.y.cos();
            ((a.y.sin() * cosp1y * b.x.sin() - b.y.sin() * cosp0y * a.x.sin())
                / (cosp0y * cosp1y * sinp0x_p1x))
                .atan()
        } else {
            a.y.midpoint(b.y)
        }
    }
}

impl<L> Listener for ClipAntimeridian<L>
where
    L: Listener,
{
    fn line_end(&mut self) {
        self.listener.line_end();
        self.prev_point = Vec2::invalid();
    }

    fn line_start(&mut self) {
        self.listener.line_start();
        self.clean = Clean::CLEAN;
    }

    #[expect(
        clippy::float_cmp,
        reason = "good enough for D3, good enough for me for now"
    )]
    fn point(&mut self, mut point: Vec2) {
        let sx = if point.x > 0.0 { PI } else { -PI };
        let dx = (point.x - self.prev_point.x).abs();
        if (dx - PI).abs() < EPSILON {
            // Line crosses a pole
            self.prev_point.y = if self.prev_point.y.midpoint(point.y) > 0.0 {
                FRAC_PI_2
            } else {
                -FRAC_PI_2
            };

            self.listener.point(self.prev_point);
            self.listener
                .point(Vec2::new(self.prev_sx, self.prev_point.y));
            self.listener.line_end();
            self.listener.line_start();
            self.listener.point(Vec2::new(sx, self.prev_point.y));
            self.listener.point(Vec2::new(point.x, self.prev_point.y));
            self.clean = Clean::DIRTY;
        } else if self.prev_sx != sx && dx >= PI {
            // Line crosses antimeridian

            // handle degeneracies
            if (self.prev_point.x - self.prev_sx).abs() < EPSILON {
                self.prev_point.x -= self.prev_sx * EPSILON;
            }
            if (point.x - sx).abs() < EPSILON {
                point.x -= sx * EPSILON;
            }
            self.prev_point.y = Self::intersect(self.prev_point, point);
            self.listener
                .point(Vec2::new(self.prev_sx, self.prev_point.y));
            self.listener.line_end();
            self.listener.line_start();
            self.listener.point(Vec2::new(sx, self.prev_point.y));
            self.clean = Clean::DIRTY;
        }
        self.prev_point = point;
        self.prev_sx = sx;
        self.listener.point(point);
    }

    fn polygon_end(&mut self) {}

    fn polygon_start(&mut self) {}

    fn sphere(&mut self) {}
}

impl<L> Preclipper<L> for ClipAntimeridian<L>
where
    L: Listener,
{
    type CArgs = ();
    type IArgs = ();

    fn clip_line(listener: L, (): ()) -> Self {
        Self::new(listener)
    }

    fn interpolate(line: Option<(Vec2, Vec2)>, direction: Direction, listener: &mut L, &(): &()) {
        if let Some((from, to)) = line {
            if (from.x - to.x) > EPSILON {
                let s = if from.x < to.x { PI } else { -PI };
                let lon = direction.signum() * s / 2.0;
                listener.point(Vec2::new(-s, lon));
                listener.point(Vec2::new(0.0, lon));
                listener.point(Vec2::new(s, lon));
            } else {
                listener.point(to);
            }
        } else {
            let lon = direction.signum() * FRAC_PI_2;
            listener.point(Vec2::new(-PI, lon));
            listener.point(Vec2::new(0.0, lon));
            listener.point(Vec2::new(PI, lon));
            listener.point(Vec2::new(PI, 0.0));
            listener.point(Vec2::new(PI, -lon));
            listener.point(Vec2::new(0.0, -lon));
            listener.point(Vec2::new(-PI, -lon));
            listener.point(Vec2::new(-PI, 0.0));
            listener.point(Vec2::new(-PI, lon));
        }
    }

    fn clean(&self) -> Clean {
        Clean::NEEDS_REJOIN - self.clean
    }

    fn clip_start(&self) -> Vec2 {
        Vec2::new(-PI, -FRAC_PI_2)
    }

    fn interpolate_args(&self) -> Self::IArgs {}

    fn listener(&mut self) -> &mut L {
        &mut self.listener
    }

    fn point_visible(&self, _: Vec2) -> bool {
        true
    }
}

/// A preclipper which clips lines to a radius around the projection’s origin.
#[expect(clippy::struct_excessive_bools, reason = "yes, many states, much bool")]
pub(super) struct ClipCircle<L>
where
    L: Listener,
{
    /// Whether the line segment being processed has crossed any lines and needs
    /// to go to counselling.
    clean: Clean,
    /// The start coordinate for a clipping operation.
    clip_start: Vec2,
    /// Whether the first point of line segment being processed was visible
    /// (i.e. not clipped).
    first_visible: bool,
    /// The output listener.
    listener: L,
    /// I don’t know. A circle is not a hemisphere I guess?
    not_hemisphere: bool,
    /// The direction in which the previous point clipped outside the circle.
    prev_clip: ClippedDirection,
    /// The previous point.
    prev_point: Option<Vec2>,
    /// Whether the previous point on line segment being processed was visible
    /// (i.e. not clipped).
    prev_visible: bool,
    /// The radius of the circle.
    radius: f64,
    /// The precomputed cosine of the radius.
    rcos: f64,
    /// The precomputed sine of the radius.
    rsin: f64,
    /// If true, use the small circle for clipping.
    small_radius: bool,
}

impl<L> ClipCircle<L>
where
    L: Listener,
{
    /// Creates a new `ClipCircle` with the given output listener and radius.
    fn new(listener: L, radius: f64) -> Self {
        let rcos = radius.cos();
        let rsin = radius.sin();
        let not_hemisphere = rcos.abs() > EPSILON;
        let small_radius = rcos > 0.0;
        let clip_start = if small_radius {
            Vec2::new(0.0, -radius)
        } else {
            Vec2::new(-PI, radius - PI)
        };
        Self {
            clean: <_>::default(),
            clip_start,
            first_visible: <_>::default(),
            listener,
            not_hemisphere,
            prev_clip: <_>::default(),
            prev_point: <_>::default(),
            prev_visible: <_>::default(),
            radius,
            rcos,
            rsin,
            small_radius,
        }
    }

    /// Returns in which direction the given `point` is out of range of the
    /// clipping circle.
    fn clipped(&self, point: Vec2) -> ClippedDirection {
        let r = if self.small_radius {
            self.radius
        } else {
            PI - self.radius
        };
        let mut clipped = ClippedDirection::empty();
        if point.x < -r {
            clipped |= ClippedDirection::LEFT;
        } else if point.x > r {
            clipped |= ClippedDirection::RIGHT;
        }

        if point.y < -r {
            clipped |= ClippedDirection::BOTTOM;
        } else if point.y > r {
            clipped |= ClippedDirection::TOP;
        }
        clipped
    }

    /// Finds the single intersection between the given line segment `(a, b)`
    /// and the circle.
    fn intersect(&self, (a, b): (Vec2, Vec2)) -> Option<Vec2> {
        Some(match self.first_intersect((a, b), true)? {
            Either::Left(q) | Either::Right((_, q)) => q,
        })
    }

    /// Finds the two intersections between the given line segment `(a, b)` and
    /// the circle, or `None` if there are zero or one intersections.
    fn intersect_two(&self, (a, b): (Vec2, Vec2)) -> Option<(Vec2, Vec2)> {
        let ((big_a, u, big_a_u, uu, t), point) = match self.first_intersect((a, b), false)? {
            Either::Left(_) => return None,
            Either::Right(value) => value,
        };

        let lambda0 = a.x.min(b.x);
        let lambda1 = b.x.max(a.x);
        let mut phi0 = a.y.min(b.y);
        let mut phi1 = b.y.max(a.y);
        let d_lambda = lambda1 - lambda0;
        let polar = (d_lambda - PI).abs() < EPSILON;
        let meridian = polar || d_lambda < EPSILON;

        if !polar && phi1 < phi0 {
            core::mem::swap(&mut phi0, &mut phi1);
        }

        let first_point_is_between_ab = if meridian {
            if polar {
                let best_phi = if (point.x - lambda0).abs() < EPSILON {
                    phi0
                } else {
                    phi1
                };

                (phi0 + phi1 > 0.0) ^ (point.y < best_phi)
            } else {
                phi0 <= point.y && point.y <= phi1
            }
        } else {
            (d_lambda > PI) ^ (lambda0 <= point.x && point.x <= lambda1)
        };

        first_point_is_between_ab.then(|| {
            let q1 = u * ((-big_a_u + t) / uu) + big_a;
            (point, Vec2::from_spherical(q1))
        })
    }

    /// Computes the first intersection of the given line segment `(a, b)` with
    /// this clipping circle, returning either the single intersection, or the
    /// first intersection plus intermediates for computing the second
    /// intersection.
    fn first_intersect(
        &self,
        (a, b): (Vec2, Vec2),
        only_one: bool,
    ) -> Option<Either<Vec2, (CircleIntercept, Vec2)>> {
        let pa = Vec3::from_cartesian(a);
        let pb = Vec3::from_cartesian(b);
        let n1 = Vec3::new(1.0, 0.0, 0.0);
        let n2 = pa.cross(pb);
        let n1n2 = n2.x;
        let n2n2 = n2.square_len();
        let determinant = n2n2 - n1n2 * n1n2;
        if determinant == 0.0 {
            return only_one.then_some(Either::Left(a));
        }
        let c1 = self.rcos * n2n2 / determinant;
        let c2 = -self.rcos * n1n2 / determinant;
        let big_a = (n1 * c1) + (n2 * c2);
        let u = n1.cross(n2);
        let big_a_u = big_a.dot(u);
        let uu = u.square_len();
        let t2 = big_a_u * big_a_u - uu * (big_a.square_len() - 1.0);
        if t2 < 0.0 {
            return None;
        }
        let t2_sqrt = t2.sqrt();
        let point = Vec2::from_spherical(u * (-big_a_u - t2_sqrt) / uu + big_a);
        Some(Either::Right(((big_a, u, big_a_u, uu, t2_sqrt), point)))
    }
}

/// Circle intersection calculation intermediates.
type CircleIntercept = (Vec3, Vec3, f64, f64, f64);

impl<L> Listener for ClipCircle<L>
where
    L: Listener,
{
    fn line_end(&mut self) {
        if self.prev_visible {
            self.listener.line_end();
        }
        self.prev_point = None;
    }

    fn line_start(&mut self) {
        self.first_visible = false;
        self.prev_visible = false;
        self.clean = Clean::CLEAN;
    }

    fn point(&mut self, mut point: Vec2) {
        let visible = self.point_visible(point);

        let clipped = if self.small_radius {
            if visible {
                <_>::default()
            } else {
                self.clipped(point)
            }
        } else if visible {
            self.clipped(Vec2::new(
                point.x + if point.x < 0.0 { PI } else { -PI },
                point.y,
            ))
        } else {
            <_>::default()
        };

        let visible = if let Some(last_point) = self.prev_point {
            // Handle degeneracies.
            let visible = if visible == self.prev_visible {
                visible
            } else {
                let intersection = self.intersect((last_point, point));
                if let Some(intersection) = intersection
                    && (close_enough(last_point, intersection) || close_enough(point, intersection))
                {
                    point += EPSILON;
                    self.point_visible(point)
                } else {
                    visible
                }
            };

            if visible != self.prev_visible {
                self.clean = Clean::DIRTY;
                self.prev_point = Some(if visible {
                    // outside going in
                    self.listener.line_start();
                    let intersection = self.intersect((point, last_point)).expect("intersection");
                    self.listener.point(intersection);
                    intersection
                } else {
                    // inside going out
                    let intersection = self.intersect((last_point, point)).expect("intersection");
                    self.listener.point(intersection);
                    self.listener.line_end();
                    intersection
                });
            } else if self.not_hemisphere
                && (self.small_radius ^ visible)
                && (clipped & self.prev_clip) == ClippedDirection::empty()
                && let Some(t) = self.intersect_two((point, last_point))
            {
                // If the clips for two points are different, or are both zero,
                // this segment intersects with the small circle.
                self.clean = Clean::DIRTY;
                if self.small_radius {
                    self.listener.line_start();
                    self.listener.point(t.0);
                    self.listener.point(t.1);
                    self.listener.line_end();
                } else {
                    self.listener.point(t.1);
                    self.listener.line_end();
                    self.listener.line_start();
                    self.listener.point(t.0);
                }
            }
            visible
        } else {
            self.first_visible = visible;
            self.prev_visible = visible;
            if visible {
                self.listener.line_start();
            }
            visible
        };

        if visible
            && self
                .prev_point
                .is_none_or(|last_point| !close_enough(last_point, point))
        {
            self.listener.point(point);
        }

        self.prev_point = Some(point);
        self.prev_visible = visible;
        self.prev_clip = clipped;
    }

    fn polygon_end(&mut self) {}

    fn polygon_start(&mut self) {}

    fn sphere(&mut self) {}
}

impl<L> Preclipper<L> for ClipCircle<L>
where
    L: Listener,
{
    type CArgs = f64;
    type IArgs = (f64, f64, f64);

    fn clean(&self) -> Clean {
        self.clean
            | if self.first_visible && self.prev_visible {
                Clean::NEEDS_REJOIN
            } else {
                Clean::empty()
            }
    }

    /// Draws an interpolated spherical `line` in a `direction` using the given
    /// `args`, emitting the result to `listener`. If `line` is `None`, draws a
    /// line along the entire clipping edge?
    fn interpolate(
        line: Option<(Vec2, Vec2)>,
        direction: Direction,
        listener: &mut L,
        &(radius, cr, sr): &Self::IArgs,
    ) {
        const PRECISION: f64 = 6.0_f64.to_radians();

        /// Returns the signed angle of the given point relative to
        /// `(cr, 0, 0)`.
        fn circle_angle(cr: f64, point: Vec2) -> f64 {
            let a = {
                let mut a = Vec3::from_cartesian(point);
                a.x -= cr;
                a.norm()
            };
            let angle = (-a.y).acos();
            (if a.z >= 0.0 { -angle } else { angle } + TAU - EPSILON) % TAU
        }

        let step = direction.signum() * PRECISION;
        let (from, to) = if let Some((from, to)) = line {
            let mut from = circle_angle(cr, from);
            let to = circle_angle(cr, to);

            if direction.forward() != (from > to) {
                from += direction.signum() * TAU;
            }

            (from, to)
        } else {
            (radius + direction.signum() * TAU, radius - 0.5 * step)
        };

        let mut t = from;
        while direction.forward() != (t < to) {
            listener.point(Vec2::from_spherical(Vec3::new(
                cr,
                -sr * t.cos(),
                -sr * t.sin(),
            )));
            t -= step;
        }
    }

    fn clip_line(listener: L, angle: Self::CArgs) -> Self {
        Self::new(listener, angle)
    }

    fn clip_start(&self) -> Vec2 {
        self.clip_start
    }

    fn interpolate_args(&self) -> Self::IArgs {
        (self.radius, self.rcos, self.rsin)
    }

    fn point_visible(&self, point: Vec2) -> bool {
        point.x.cos() * point.y.cos() > self.rcos
    }

    fn listener(&mut self) -> &mut L {
        &mut self.listener
    }
}

/// A Cartesian clipper (postclipper) which clips points to the bounds of a
/// rectangle.
pub(super) struct ClipExtent<L>
where
    L: Listener,
{
    /// Whether the line segment being processed has crossed any lines and needs
    /// to go to counselling.
    clean: Clean,

    /// A buffer containing the clipped line segments for the polygon being
    /// processed. Raw, unclipped line segments are accumulated in
    /// [`Self::raw_polygon`].
    clipped_buffer: BufferListener,

    /// A buffer containing all the clipped line segments for the polygon being
    /// processed.
    clipped_polygon: Vec<Vec<Vec<Vec2>>>,

    /// The clipping rectangle.
    extent: Rect,

    /// The first point of the line segment being processed and its visibility
    /// state.
    first: Option<(Vec2, bool)>,

    /// If true, currently drawing a polygon.
    in_polygon: bool,

    /// The output event listener.
    listener: L,

    /// The current [`Listener::point`] handler.
    point: fn(&mut Self, point: Vec2),

    /// The previous point of the line segment being processed and its
    /// visibility state.
    prev: Option<(Vec2, bool)>,

    /// An accumulator for the raw, unrotated line segments of the polygon being
    /// processed, whose entire purpose is to be used once to learn whether
    /// the origin of [`Self::extent`] is inside the area of the polygon.
    raw_polygon: Vec<Vec<Vec2>>,
}

impl<L> ClipExtent<L>
where
    L: Listener,
{
    /// Creates a new `ClipExtent` with the given bounds and output listener.
    pub fn new(extent: Rect, listener: L) -> Self {
        Self {
            clean: <_>::default(),
            clipped_buffer: <_>::default(),
            clipped_polygon: <_>::default(),
            extent,
            first: <_>::default(),
            in_polygon: <_>::default(),
            listener,
            point: clip_extent::point,
            prev: <_>::default(),
            raw_polygon: <_>::default(),
        }
    }

    /// Emits an event to the appropriate output according to the current state
    /// of the clipper.
    //
    // D3 swaps out the listener, but this is not so easily possible because it
    // would result in a self-referential struct. Mercifully, this is only
    // needed for line and point emitters, so it is not necessary to do even
    // more extremely sad things to make `clip_polygon` happy.
    #[inline]
    fn emit<F>(&mut self, f: F)
    where
        F: FnOnce(&mut dyn Listener),
    {
        if self.in_polygon {
            f(&mut self.clipped_buffer);
        } else {
            f(&mut self.listener);
        }
    }

    /// Draws an interpolated cartesian `line` in a `direction` using the given
    /// `extent`, emitting the result to `listener`. If `line` is `None`, draws
    /// a line along the entire clipping edge?
    fn interpolate(
        line: Option<(Vec2, Vec2)>,
        direction: Direction,
        listener: &mut L,
        extent: &Rect,
    ) {
        use clip_extent::{Corner, compare_points, corner};

        let corners = if let Some((from, to)) = line {
            let start = corner(extent, from, direction);
            let end = corner(extent, to, direction);
            (start != end || ((compare_points(extent, from, to).is_lt()) ^ direction.forward()))
                .then_some((start, end))
        } else {
            Some((Corner::TopLeft, Corner::TopLeft))
        };

        if let Some((mut corner, end)) = corners {
            loop {
                let x = if corner == Corner::TopLeft || corner == Corner::BottomLeft {
                    extent.left
                } else {
                    extent.right
                };
                let y = if corner.bottom() {
                    extent.bottom
                } else {
                    extent.top
                };

                listener.point(Vec2::new(x, y));

                corner = corner.next(direction);
                if corner == end {
                    break;
                }
            }
        } else {
            listener.point(line.map(|(_, to)| to).unwrap());
        }
    }
}

impl<L> Listener for ClipExtent<L>
where
    L: Listener,
{
    fn line_end(&mut self) {
        if self.in_polygon {
            let (first, first_visible) = self.first.unwrap();
            let (_, prev_visible) = self.prev.unwrap();
            clip_extent::line_point(self, first);
            if first_visible && prev_visible {
                self.clipped_buffer.rejoin();
            }
            self.clipped_polygon.push(self.clipped_buffer.take());
        }
        self.point = clip_extent::point;
        if self.prev.is_some_and(|(_, visible)| visible) {
            self.emit(|listener| listener.line_end());
        }
    }

    fn line_start(&mut self) {
        self.point = clip_extent::line_point;
        if self.in_polygon {
            self.raw_polygon.push(vec![]);
        }
        self.first = None;
        self.prev = None;
    }

    fn point(&mut self, point: Vec2) {
        (self.point)(self, point);
    }

    fn polygon_end(&mut self) {
        let segments = self.clipped_polygon.drain(..).flatten().collect::<Vec<_>>();
        let clip_start_inside = inside_polygon_cartesian(&self.raw_polygon, self.extent.bl());
        let inside = self.clean != Clean::DIRTY && clip_start_inside;
        let visible = !segments.is_empty();
        if inside || visible {
            self.listener.polygon_start();
            if inside {
                self.listener.line_start();
                Self::interpolate(None, Direction::Forward, &mut self.listener, &self.extent);
                self.listener.line_end();
            }
            if visible {
                clip_polygon(
                    &segments,
                    |a, b| clip_extent::compare_points(&self.extent, a, b),
                    clip_start_inside,
                    |line, direction, listener| {
                        Self::interpolate(line, direction, listener, &self.extent);
                    },
                    &mut self.listener,
                );
            }
            self.listener.polygon_end();
        }
        self.raw_polygon.clear();
        self.in_polygon = false;
    }

    fn polygon_start(&mut self) {
        self.clean = Clean::CLEAN;
        self.in_polygon = true;
        assert!(
            self.clipped_polygon.is_empty() && self.raw_polygon.is_empty(),
            "polygon_start calls cannot be nested"
        );
    }

    fn sphere(&mut self) {}
}

/// Swappable listener functions and other helpers for [`ClipExtent`].
mod clip_extent {
    use super::{Clean, ClipExtent, Direction, EPSILON, Listener, Ordering, Rect, Vec2, clip_line};

    /// A corner of a rectangle.
    #[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
    pub(super) enum Corner {
        /// The top left.
        TopLeft = 0,
        /// The top right.
        TopRight = 1,
        /// The bottom right.
        BottomRight = 2,
        /// The bottom left.
        BottomLeft = 3,
    }

    impl Corner {
        /// Gets the next corner in the anticlockwise direction.
        #[inline]
        const fn backward(self) -> Self {
            match self {
                Self::TopLeft => Self::BottomLeft,
                Self::TopRight => Self::TopLeft,
                Self::BottomRight => Self::TopRight,
                Self::BottomLeft => Self::BottomRight,
            }
        }

        /// Returns true if this is a corner on the bottom edge.
        #[inline]
        pub const fn bottom(self) -> bool {
            matches!(self, Self::BottomLeft | Self::BottomRight)
        }

        /// Gets the next corner in the clockwise direction.
        #[inline]
        const fn forward(self) -> Self {
            match self {
                Self::TopLeft => Self::TopRight,
                Self::TopRight => Self::BottomRight,
                Self::BottomRight => Self::BottomLeft,
                Self::BottomLeft => Self::TopLeft,
            }
        }

        /// Gets the next corner in the given `direction`.
        #[inline]
        pub const fn next(self, direction: Direction) -> Self {
            match direction {
                Direction::Forward => self.forward(),
                Direction::Backward => self.backward(),
            }
        }
    }

    /// The comparator to use with [`clip_polygon`](super::fn@clip_polygon).
    pub(super) fn compare_points(extent: &Rect, a: Vec2, b: Vec2) -> Ordering {
        let ca = corner(extent, a, Direction::Forward);
        let cb = corner(extent, b, Direction::Forward);
        if ca != cb {
            ca.cmp(&cb)
        } else if ca == Corner::TopLeft {
            b.y.total_cmp(&a.y)
        } else if ca == Corner::TopRight {
            a.x.total_cmp(&b.x)
        } else if ca == Corner::BottomRight {
            a.y.total_cmp(&b.y)
        } else {
            b.x.total_cmp(&a.x)
        }
    }

    /// Given a `point` sitting on the edge of `extent`, returns the next corner
    /// in the given `direction`.
    pub(super) fn corner(extent: &Rect, point: Vec2, direction: Direction) -> Corner {
        if (point.x - extent.left).abs() < EPSILON {
            if direction.forward() {
                Corner::TopLeft
            } else {
                Corner::BottomLeft
            }
        } else if (point.x - extent.right).abs() < EPSILON {
            if direction.forward() {
                Corner::BottomRight
            } else {
                Corner::TopRight
            }
        } else if (point.y - extent.top).abs() < EPSILON {
            if direction.forward() {
                Corner::TopRight
            } else {
                Corner::TopLeft
            }
        } else
        /* by process of elimination, this is the bottom */
        if direction.forward() {
            Corner::BottomLeft
        } else {
            Corner::BottomRight
        }
    }

    /// The [`Listener::point`] handler used when drawing points on a line.
    pub(super) fn line_point<L>(c: &mut ClipExtent<L>, mut point: Vec2)
    where
        L: Listener,
    {
        const MAX: f64 = 1e9;

        point.x = point.x.clamp(-MAX, MAX);
        point.y = point.y.clamp(-MAX, MAX);
        let visible = c.extent.contains(point);
        if c.in_polygon {
            c.raw_polygon.last_mut().unwrap().push(point);
        }
        if c.first.is_some() {
            // If there was a first, there must also have been a prev
            let (prev, prev_visible) = c.prev.unwrap();
            if visible && prev_visible {
                c.emit(|listener| listener.point(point));
            } else if let Some(line) = clip_line(&c.extent, (prev, point)) {
                if !prev_visible {
                    c.emit(|listener| listener.line_start());
                    c.emit(|listener| listener.point(line.0));
                }
                c.emit(|listener| listener.point(line.1));
                if !visible {
                    c.emit(|listener| listener.line_end());
                }
                c.clean = Clean::DIRTY;
            } else if visible {
                c.emit(|listener| listener.line_start());
                c.emit(|listener| listener.point(point));
                c.clean = Clean::DIRTY;
            }
        } else {
            c.first = Some((point, visible));
            if visible {
                c.emit(|listener| listener.line_start());
                c.emit(|listener| listener.point(point));
            }
        }
        c.prev = Some((point, visible));
    }

    /// The [`Listener::point`] handler used when drawing standalone points.
    pub(super) fn point<L>(c: &mut ClipExtent<L>, point: Vec2)
    where
        L: Listener,
    {
        if c.extent.contains(point) {
            c.emit(|listener| listener.point(point));
        }
    }
}

bitflags::bitflags! {
    /// Status flags for line segment clipping.
    #[derive(Clone, Copy, Default, Eq, PartialEq)]
    pub(super) struct Clean: u8 {
        /// Someone cut this line in half! There is blood and guts all over the
        /// place! Oh, the humanity!
        const DIRTY = 0;
        /// The line segment didn’t get itself chopped in half.
        const CLEAN = 1;
        /// The line segment needs to be joined up with its pals on the other
        /// side of the intersection.
        const NEEDS_REJOIN = 2;
    }
}

bitflags::bitflags! {
    /// The clipped direction of a line segment.
    #[derive(Clone, Copy, Default, Eq, PartialEq)]
    struct ClippedDirection: u8 {
        /// Clipped off the left.
        const LEFT = 1;
        /// Clipped off the right.
        const RIGHT = 2;
        /// Clipped off the top.
        const TOP = 4;
        /// Clipped off the bottom.
        const BOTTOM = 8;
    }
}

/// Returns true if the given points are approximately equal.
#[inline]
fn close_enough(a: Vec2, b: Vec2) -> bool {
    (a.x - b.x).abs() < EPSILON && (a.y - b.y).abs() < EPSILON
}

/// Clips the line `(a, b)` to the bounds given by `extent` using Liang-Barsky
/// line clipping. If the line is out of bounds, `None` is returned.
fn clip_line(extent: &Rect, (a, b): (Vec2, Vec2)) -> Option<(Vec2, Vec2)> {
    fn clip_one(n: f64, d: f64, (min, max): (f64, f64)) -> Option<(f64, f64)> {
        if d.abs() < f64::EPSILON {
            return (n < 0.0).then_some((min, max));
        }
        let t = n / d;
        if d > 0.0 {
            (t <= max).then(|| (min.max(t), max))
        } else {
            (t >= min).then(|| (min, max.min(t)))
        }
    }

    let len = b - a;

    clip_one(extent.left - a.x, len.x, (0.0, 1.0))
        .and_then(|r| clip_one(a.x - extent.right, -len.x, r))
        .and_then(|r| clip_one(extent.top - a.y, len.y, r))
        .and_then(|r| clip_one(a.y - extent.bottom, -len.y, r))
        .map(|(min, max)| {
            let b = if max < 1.0 { a + len * max } else { b };
            let a = if min > 0.0 { a + len * min } else { a };
            (a, b)
        })
}

/// Clips a `polygon` by decomposing it into line segments, then rejoining the
/// segments using the given `interpolate` function for any clipped edges.
fn clip_polygon<C, L, I>(
    polygon: &[Vec<Vec2>],
    compare: C,
    clip_start_inside: bool,
    mut interpolate: I,
    listener: &mut L,
) where
    C: Fn(Vec2, Vec2) -> Ordering,
    L: Listener + ?Sized,
    I: FnMut(Option<(Vec2, Vec2)>, Direction, &mut L),
{
    let Some((mut vertices, subject, clip)) = clip_polygon::prepare(polygon, compare, listener)
    else {
        return;
    };

    let mut entry = clip_start_inside;
    for index in clip {
        entry = !entry;
        vertices[index].is_entry = entry;
    }

    let start = subject[0];
    loop {
        let mut index = start;
        let mut is_subject = true;
        while vertices[index].is_visited {
            index = vertices[index].next;
            if index == start {
                return;
            }
        }

        let mut points = vertices[index].points;

        listener.line_start();

        loop {
            let other = vertices[index].other;
            vertices[other].is_visited = true;
            vertices[index].is_visited = true;
            let current = &vertices[index];
            if current.is_entry {
                if is_subject {
                    for point in points.map_or(&[][..], |index| &polygon[index]) {
                        listener.point(*point);
                    }
                } else {
                    interpolate(
                        Some((current.point, vertices[current.next].point)),
                        Direction::Forward,
                        listener,
                    );
                }
                index = current.next;
            } else {
                if is_subject {
                    points = vertices[current.prev].points;
                    for point in points.map_or(&[][..], |index| &polygon[index]).iter().rev() {
                        listener.point(*point);
                    }
                } else {
                    interpolate(
                        Some((current.point, vertices[current.prev].point)),
                        Direction::Backward,
                        listener,
                    );
                }
                index = current.prev;
            }
            index = vertices[index].other;
            let current = &vertices[index];
            if current.is_visited {
                break;
            }
            points = current.points;
            is_subject = !is_subject;
        }

        listener.line_end();
    }
}

/// Helper types and functions for [`clip_polygon`](fn@clip_polygon).
mod clip_polygon {
    use super::{Listener, Ordering, Vec2, close_enough};

    /// A vertex used for clipping polygons which corresponds to the start or end of
    /// a line segment.
    pub(super) struct Vertex {
        /// The coordinate of the vertex.
        pub point: Vec2,
        /// The index of the line segment corresponding to this vertex in the
        /// list of line segments (`segments`).
        pub points: Option<usize>,
        /// The vertex index of the vertex with the same coordinate in the opposite
        /// list of vertices (`subject` or `clip`).
        pub other: usize,
        /// If true, this vertex is the start of a `subject` or end of a `clip`.
        pub is_entry: bool,
        /// Set once the vertex has been processed.
        pub is_visited: bool,
        /// The index of the next vertex in the array of `vertices`.
        pub next: usize,
        /// The index of the previous vertex in the array of `vertices`.
        pub prev: usize,
    }

    impl Vertex {
        /// Creates a new vertex at the given `point`, from the line segment given
        /// in `points`, with its mirror at the index `other`.
        pub fn new(point: Vec2, points: Option<usize>, other: usize, is_entry: bool) -> Self {
            Self {
                point,
                points,
                other,
                is_entry,
                is_visited: false,
                next: usize::MAX,
                prev: usize::MAX,
            }
        }
    }

    /// Builds the doubly-linked list of vertices.
    fn link_list(intersections: &mut [Vertex], links: &[usize]) {
        if links.is_empty() {
            return;
        }

        let mut a = links[0];
        for &b in &links[1..] {
            intersections[a].next = b;
            intersections[b].prev = a;
            a = b;
        }
        let b = links[0];
        intersections[a].next = b;
        intersections[b].prev = a;
    }

    /// Prepares the vertex lists used for polygon clipping. Returns `None` if
    /// the given `polygon` does not require clipping.
    pub(super) fn prepare<C, L>(
        polygon: &[Vec<Vec2>],
        compare: C,
        listener: &mut L,
    ) -> Option<(Vec<Vertex>, Vec<usize>, Vec<usize>)>
    where
        C: Fn(Vec2, Vec2) -> Ordering,
        L: Listener + ?Sized,
    {
        let mut vertices = vec![];
        let mut subject = vec![];
        let mut clip = vec![];
        for (index, line) in polygon.iter().enumerate() {
            if line.len() < 2 {
                return None;
            }

            let start_point = line[0];
            let end_point = line[line.len() - 1];

            // If the first and last points of a segment are coincident, then
            // treat as a closed ring.
            if close_enough(start_point, end_point) {
                listener.line_start();
                for point in &line[..line.len() - 1] {
                    listener.point(*point);
                }
                listener.line_end();
                return None;
            }

            let a = Vertex::new(start_point, Some(index), vertices.len() + 1, true);
            let b = Vertex::new(start_point, None, vertices.len(), false);
            subject.push(vertices.len());
            vertices.push(a);
            clip.push(vertices.len());
            vertices.push(b);

            let a = Vertex::new(end_point, Some(index), vertices.len() + 1, false);
            let b = Vertex::new(end_point, None, vertices.len(), true);
            subject.push(vertices.len());
            vertices.push(a);
            clip.push(vertices.len());
            vertices.push(b);
        }

        if subject.is_empty() {
            return None;
        }

        clip.sort_by(|a, b| compare(vertices[*a].point, vertices[*b].point));
        link_list(&mut vertices, &subject);
        link_list(&mut vertices, &clip);

        Some((vertices, subject, clip))
    }
}

/// Returns true if the given Cartesian `point` is inside the given Cartesian
/// `polygon` using the winding number method.
fn inside_polygon_cartesian(polygon: &[Vec<Vec2>], point: Vec2) -> bool {
    /// Returns the 2D cross product of AB and AC vectors, i.e., the z-component
    /// of the 3D cross product in Cartesian quadrant I. Returns a positive
    /// value if ABC is anticlockwise, negative if clockwise, and zero if the
    /// points are collinear.
    #[inline]
    fn cross2d(a: Vec2, b: Vec2, c: Vec2) -> f64 {
        (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
    }

    let mut winding_number = 0;
    for v in polygon {
        // TODO: Is it stable yet?
        // https://github.com/rust-lang/rust/issues/75027
        for w in v.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a.y <= point.y {
                if b.y > point.y && cross2d(a, b, point) > 0.0 {
                    winding_number += 1;
                } else if b.y <= point.y && cross2d(a, b, point) < 0.0 {
                    winding_number -= 1;
                }
            }
        }
    }
    winding_number != 0
}

/// Returns true if the given cartographic `point` is inside of the given
/// `polygon` using the winding number method.
fn inside_polygon_cartographic(polygon: &[Vec<Vec2>], point: Vec2) -> bool {
    let meridian = point.x;
    let parallel = point.y;
    let meridian_normal = Vec3::new(meridian.sin(), -meridian.cos(), 0.0);
    let mut polar_angle = 0.0;
    let mut winding = 0;
    let mut area_ring_sum = Adder::default();

    for ring in polygon {
        if ring.is_empty() {
            continue;
        }

        let mut prev_point = ring[0];
        let mut prev_lambda = prev_point.x;
        let (mut prev_sin_phi, mut prev_cos_phi) = {
            let phi = prev_point.y / 2.0 + FRAC_PI_4;
            (phi.sin(), phi.cos())
        };

        let mut index = 1;
        loop {
            if index == ring.len() {
                index = 0;
            }

            let point = ring[index];
            let lambda = point.x;
            let phi = point.y / 2.0 + FRAC_PI_4;
            let sin_phi = phi.sin();
            let cos_phi = phi.cos();
            let d_lambda = lambda - prev_lambda;
            let sign_d_lambda = d_lambda.signum();
            let abs_d_lambda = d_lambda.abs();
            let antimeridian = abs_d_lambda > PI;
            let k = prev_sin_phi * sin_phi;

            area_ring_sum += (k * sign_d_lambda * abs_d_lambda.sin())
                .atan2(prev_cos_phi * cos_phi + k * abs_d_lambda.cos());

            polar_angle += if antimeridian {
                d_lambda + sign_d_lambda * TAU
            } else {
                d_lambda
            };

            if antimeridian ^ (prev_lambda >= meridian) ^ (lambda >= meridian) {
                // The longitudes are either side of the point’s meridian, and
                // the latitudes are smaller than the parallel
                let arc = Vec3::from_cartesian(prev_point)
                    .cross(Vec3::from_cartesian(point))
                    .norm();
                let intersection = meridian_normal.cross(arc).norm();
                let direction = if antimeridian ^ (d_lambda >= 0.0) {
                    1
                } else {
                    -1
                };
                let phi_arc = f64::from(-direction) * clamp_asin(intersection.z);
                #[expect(
                    clippy::float_cmp,
                    reason = "good enough for D3, good enough for me for now"
                )]
                if parallel > phi_arc || parallel == phi_arc && (arc.x != 0.0 || arc.y != 0.0) {
                    winding += direction;
                }
            }

            if index == 0 {
                break;
            }

            index += 1;
            prev_lambda = lambda;
            prev_sin_phi = sin_phi;
            prev_cos_phi = cos_phi;
            prev_point = point;
        }
    }

    // First, determine whether the South pole is inside or outside:
    //
    // It is inside if:
    // * the polygon winds around it in a clockwise direction.
    // * the polygon does not (cumulatively) wind around it, but has a negative
    //   (counter-clockwise) area.
    //
    // Second, count the (signed) number of times a segment crosses a meridian
    // from the point to the South pole.  If it is zero, then the point is the
    // same side as the South pole.
    (polar_angle < -EPSILON || polar_angle < EPSILON && area_ring_sum < -EPSILON)
        ^ ((winding & 1) != 0)
}

#[cfg(test)]
mod tests {
    use super::{super::circle_iter, *};

    #[derive(Clone, Copy, Debug)]
    enum Event {
        LineEnd,
        LineStart,
        Point(Vec2),
        PolygonEnd,
        PolygonStart,
        Sphere,
    }

    impl PartialEq for Event {
        fn eq(&self, other: &Self) -> bool {
            match (self, other) {
                (Self::Point(lhs), Self::Point(rhs)) => lhs.round() == rhs.round(),
                _ => core::mem::discriminant(self) == core::mem::discriminant(other),
            }
        }
    }

    #[derive(Default)]
    struct TestListener {
        events: Vec<Event>,
    }

    impl Listener for TestListener {
        fn line_end(&mut self) {
            self.events.push(Event::LineEnd);
        }

        fn line_start(&mut self) {
            self.events.push(Event::LineStart);
        }

        fn point(&mut self, point: Vec2) {
            self.events.push(Event::Point(point));
        }

        fn polygon_end(&mut self) {
            self.events.push(Event::PolygonEnd);
        }

        fn polygon_start(&mut self) {
            self.events.push(Event::PolygonStart);
        }

        fn sphere(&mut self) {
            self.events.push(Event::Sphere);
        }
    }

    #[test]
    fn clip_circle_null_resampler() {
        //         "with an equirectangular projection clipped to 90°": {
        //   topic: function(path) {
        //     return path()
        //         .context(testContext)
        //         .projection(_.geo.equirectangular()
        //           .scale(900 / Math.PI)
        //           .precision(0)
        //           .clipAngle(90));
        //   },
        //   "renders a point": function(p) {
        //     p({
        //       type: "Point",
        //       coordinates: [-63, 18]
        //     });
        //     assert.deepEqual(testContext.buffer(), [
        //       {type: "moveTo", x: 170, y: 160}, {type: "arc", x: 165, y: 160, r: 4.5}
        //     ]);
        //   },
        //   "renders a multipoint": function(p) {
        //     p({
        //       type: "MultiPoint",
        //       coordinates: [[-63, 18], [-62, 18], [-62, 17]]
        //     });
        //     assert.deepEqual(testContext.buffer(), [
        //       {type: "moveTo", x: 170, y: 160}, {type: "arc", x: 165, y: 160, r: 4.5},
        //       {type: "moveTo", x: 175, y: 160}, {type: "arc", x: 170, y: 160, r: 4.5},
        //       {type: "moveTo", x: 175, y: 165}, {type: "arc", x: 170, y: 165, r: 4.5}
        //     ]);
        //   },
        //   "inserts exterior along clip edge if polygon interior surrounds it": function(p) {
        //     p({type: "Polygon", coordinates: [[[80, -80], [80, 80], [-80, 80], [-80, -80], [80, -80]]]});
        //     assert.equal(testContext.buffer().filter(function(d) { return d.type === "moveTo"; }).length, 2);
        //   },
        //   "inserts exterior along clip edge if polygon exterior surrounds it": function(p) {
        //     p({type: "Polygon", coordinates: [[[100, -80], [-100, -80], [-100, 80], [100, 80], [100, -80]]]});
        //     assert.equal(testContext.buffer().filter(function(d) { return d.type === "moveTo"; }).length, 1);
        //   },
        //   "renders a small circle of 60°": function(p) {
        //     p(_.geo.circle().angle(60)());
        //     assert.deepEqual(testContext.buffer().filter(function(d) { return d.type === "moveTo"; }), [{type: "moveTo", x: 276, y: 493}]);
        //   },
        //   "renders a small circle of 120°": function(p) {
        //     p(_.geo.circle().angle(120)());
        //     assert.deepEqual(testContext.buffer().filter(function(d) { return d.type === "moveTo"; }), [{type: "moveTo", x: 87, y: 700}]);
        //   },
        //   "degenerate polygon": function(p) {
        //     p({type: "Polygon", coordinates: [[[0, 0], [0, 0], [0, 0], [0, 0]]]});
        //     assert.deepEqual(testContext.buffer(), []);
        //   }
        // },
    }

    #[test]
    fn clip_extent() {
        let tester = TestListener::default();
        let mut clip_extent = ClipExtent::new(Rect::new(100.0, 200.0, 300.0, 400.0), tester);
        clip_extent.line_start();
        clip_extent.point(Vec2::new(0.0, 0.0));
        clip_extent.point(Vec2::new(500.0, 500.0));
        clip_extent.line_end();

        assert_eq!(
            clip_extent.listener.events,
            &[
                Event::LineStart,
                Event::Point(Vec2::new(200.0, 200.0)),
                Event::Point(Vec2::new(300.0, 300.0)),
                Event::LineEnd
            ]
        );
    }

    #[test]
    fn clip_extent_point() {
        let tester = TestListener::default();
        let mut clip_extent = ClipExtent::new(Rect::new(0.0, 0.0, 960.0, 500.0), tester);

        clip_extent.point(Vec2::new(-100.0, -100.0));
        clip_extent.point(Vec2::new(0.0, 0.0));
        clip_extent.point(Vec2::new(480.0, 250.0));
        clip_extent.point(Vec2::new(960.0, 500.0));
        clip_extent.point(Vec2::new(1060.0, 6000.0));

        assert_eq!(
            clip_extent.listener.events,
            &[
                Event::Point(Vec2::new(0.0, 0.0)),
                Event::Point(Vec2::new(480.0, 250.0)),
                Event::Point(Vec2::new(960.0, 500.0))
            ]
        );
    }

    #[test]
    fn clip_extent_line() {
        let tester = TestListener::default();
        let mut clip_extent = ClipExtent::new(Rect::new(0.0, 0.0, 960.0, 500.0), tester);

        clip_extent.line_start();
        clip_extent.point(Vec2::new(-100.0, -100.0));
        clip_extent.point(Vec2::new(1060.0, 600.0));
        clip_extent.line_end();

        assert_eq!(
            clip_extent.listener.events,
            &[
                Event::LineStart,
                Event::Point(Vec2::new(66.0, 0.0)),
                Event::Point(Vec2::new(894.0, 500.0)),
                Event::LineEnd
            ]
        );
    }

    #[test]
    fn clip_extent_polygon() {
        let tester = TestListener::default();
        let mut clip_extent = ClipExtent::new(Rect::new(0.0, 0.0, 960.0, 500.0), tester);

        clip_extent.polygon_start();
        clip_extent.line_start();
        clip_extent.point(Vec2::new(-100.0, -100.0));
        clip_extent.point(Vec2::new(1060.0, -100.0));
        clip_extent.point(Vec2::new(1060.0, 600.0));
        clip_extent.point(Vec2::new(-100.0, 600.0));
        clip_extent.line_end();
        clip_extent.polygon_end();
        assert_eq!(
            clip_extent.listener.events,
            &[
                Event::PolygonStart,
                Event::LineStart,
                Event::Point(Vec2::new(0.0, 0.0)),
                Event::Point(Vec2::new(960.0, 0.0)),
                Event::Point(Vec2::new(960.0, 500.0)),
                Event::Point(Vec2::new(0.0, 500.0)),
                Event::LineEnd,
                Event::PolygonEnd
            ]
        );
    }

    #[test]
    fn clip_extent_polyhole() {
        let tester = TestListener::default();
        let mut clip_extent = ClipExtent::new(Rect::new(1.0, 1.0, 9.0, 9.0), tester);

        clip_extent.polygon_start();
        clip_extent.line_start();
        clip_extent.point(Vec2::new(0.0, 0.0));
        clip_extent.point(Vec2::new(10.0, 0.0));
        clip_extent.point(Vec2::new(10.0, 10.0));
        clip_extent.point(Vec2::new(0.0, 10.0));
        clip_extent.line_end();
        clip_extent.line_start();
        clip_extent.point(Vec2::new(4.0, 4.0));
        clip_extent.point(Vec2::new(4.0, 6.0));
        clip_extent.point(Vec2::new(6.0, 6.0));
        clip_extent.point(Vec2::new(6.0, 4.0));
        clip_extent.line_end();
        clip_extent.polygon_end();

        assert_eq!(
            clip_extent.listener.events,
            &[
                Event::PolygonStart,
                Event::LineStart,
                Event::Point(Vec2::new(1.0, 1.0)),
                Event::Point(Vec2::new(9.0, 1.0)),
                Event::Point(Vec2::new(9.0, 9.0)),
                Event::Point(Vec2::new(1.0, 9.0)),
                Event::LineEnd,
                Event::LineStart,
                Event::Point(Vec2::new(4.0, 4.0)),
                Event::Point(Vec2::new(4.0, 6.0)),
                Event::Point(Vec2::new(6.0, 6.0)),
                Event::Point(Vec2::new(6.0, 4.0)),
                Event::LineEnd,
                Event::PolygonEnd
            ]
        );
    }

    #[test]
    fn inside_polygon() {
        fn to_radians(line: Vec<Vec2>) -> Vec<Vec2> {
            line.into_iter().map(Vec2::to_radians).collect()
        }

        assert!(!inside_polygon_cartographic(
            &[vec![]],
            Vec2::zero().to_radians()
        ));

        let poly = &[to_radians(vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 0.0),
        ])];
        assert!(!inside_polygon_cartographic(
            poly,
            Vec2::new(0.1, 2.0).to_radians()
        ));
        assert!(inside_polygon_cartographic(
            poly,
            Vec2::new(0.1, 0.1).to_radians()
        ));

        let poly = &[to_radians(circle_iter(60.0, Direction::Forward).collect())];
        assert!(!inside_polygon_cartographic(
            poly,
            Vec2::new(-180.0, 0.0).to_radians()
        ));
        assert!(inside_polygon_cartographic(
            poly,
            Vec2::new(1.0, 1.0).to_radians()
        ));
    }
}
