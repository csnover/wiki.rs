//! Geographic graph types and functions.

// SPDX-License-Identifier: BSD-3-clause
// Adapted from d3 3.5.17 by Mike Bostock

mod clip;
mod projection;
mod resample;
#[cfg(test)]
mod tests;

use super::{
    EPSILON,
    renderer::{Rect, Vec2, Vec3},
};
use core::{
    cmp::Ordering,
    f64::consts::{FRAC_PI_2, PI, TAU},
};
use serde_json_borrow::Value;

/// Projector output kinds.
enum AProjector {
    /// Albers composite.
    AlbersUsa,
    /// Overrides stream sphere listener.
    Armadillo,
    /// Overrides stream sphere listener.
    Berghaus,
    /// Overrides stream point listener.
    Gilbert,
    /// Overrides stream sphere listener.
    Gingery,
    /// Overrides stream sphere listener.
    HammerRetroazimuthal,
    /// Overrides stream sphere listener.
    Healpix,
    /// Adapts some other projector.
    Quincuncial,
    /// Rotates based on points, not rotate. (twopoint, chamberlin)
    Points,
    /// Default.
    Standard,
}

/// A listener which calculates the centroid of a list of points.
struct Centroid {
    /// The area-weighted centroid.
    area_weighted: Vec3,
    /// The arithmetic mean centroid.
    arithmetic_mean: Vec3,
    /// The first point of a polygon.
    first_ring: Vec2,
    /// The magnitude of the length-weighted centroid.
    length_magnitude: f64,
    /// The length-weighted centroid.
    length_weighted: Vec3,
    /// The current [`Listener::line_end`] handler.
    line_end: fn(&mut Self),
    /// The current [`Listener::line_start`] handler.
    line_start: fn(&mut Self),
    /// The current [`Listener::point`] handler.
    point: fn(&mut Self, Vec2),
    /// The magnitude of the arithmetic mean.
    point_magnitude: f64,
    /// The previous point of a line.
    prev_line: Vec3,
    /// The previous point of a polygon.
    prev_ring: Vec3,
}

impl Centroid {
    /// Creates a new `Centroid`.
    fn new() -> Self {
        Self {
            area_weighted: <_>::default(),
            arithmetic_mean: <_>::default(),
            first_ring: <_>::default(),
            length_magnitude: <_>::default(),
            length_weighted: <_>::default(),
            line_end: centroid::line_end,
            line_start: centroid::line_start,
            point: centroid::point,
            point_magnitude: <_>::default(),
            prev_line: <_>::default(),
            prev_ring: <_>::default(),
        }
    }

    /// Add a point to the arithmetic mean of point vectors.
    fn add(&mut self, point: Vec3) {
        self.point_magnitude += 1.0;
        self.arithmetic_mean += (point - self.arithmetic_mean) / self.point_magnitude;
    }

    /// Finishes calculating the centroid, returning its location in Cartesian
    /// space, in degrees.
    fn finish(self) -> Vec2 {
        const SQ_EPSILON: f64 = EPSILON * EPSILON;

        let m = self.area_weighted.square_len();
        let (p, m) = if m < SQ_EPSILON {
            let p = if self.length_magnitude < EPSILON {
                self.arithmetic_mean
            } else {
                self.length_weighted
            };

            let m = p.square_len();
            if m < SQ_EPSILON {
                return Vec2::invalid();
            }
            (p, m)
        } else {
            (self.area_weighted, m)
        };

        Vec2::new(p.y.atan2(p.x), clamp_asin(p.z / m.sqrt())).to_degrees()
    }
}

impl Listener for Centroid {
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
        self.line_start = centroid::line_start;
    }

    fn polygon_start(&mut self) {
        self.line_start = centroid::ring_start;
    }

    fn sphere(&mut self) {}
}

/// Swappable listener functions for [`Centroid`].
mod centroid {
    #[cfg(doc)]
    use super::Listener;
    use super::{Centroid, Vec2, Vec3, clamp_acos};

    /// The [`Listener::line_end`] handler used when drawing non-polygonal
    /// lines.
    pub(super) fn line_end(c: &mut Centroid) {
        c.point = point;
    }

    /// The [`Listener::point`] handler used when drawing the first point on a
    /// non-polygonal line segment.
    pub(super) fn line_first_point(c: &mut Centroid, point: Vec2) {
        let point = Vec3::from_cartesian(point.to_radians());
        c.point = line_next_point;
        c.prev_line = point;
        c.add(point);
    }

    /// The [`Listener::point`] handler used when drawing the rest of the points
    /// on a non-polygonal line segment.
    pub(super) fn line_next_point(c: &mut Centroid, point: Vec2) {
        let point = Vec3::from_cartesian(point.to_radians());
        let magnitude = c.prev_line.cross(point).len().atan2(c.prev_line.dot(point));
        c.length_magnitude += magnitude;
        c.length_weighted += (c.prev_line + point) * magnitude;
        c.prev_line = point;
        c.add(point);
    }

    /// The [`Listener::line_start`] handler used when drawing non-polygonal
    /// lines.
    pub(super) fn line_start(c: &mut Centroid) {
        c.point = line_first_point;
    }

    /// The [`Listener::point`] handler used when drawing bare points.
    pub(super) fn point(c: &mut Centroid, point: Vec2) {
        c.add(Vec3::from_cartesian(point.to_radians()));
    }

    /// The [`Listener::line_end`] handler used when drawing polygonal lines.
    pub(super) fn ring_end(c: &mut Centroid) {
        ring_next_point(c, c.first_ring);
        c.line_end = line_end;
        c.point = point;
    }

    /// The [`Listener::point`] handler used when drawing the first point in a
    /// polygonal line.
    pub(super) fn ring_first_point(c: &mut Centroid, point: Vec2) {
        c.first_ring = point;
        c.point = ring_next_point;
        let point = Vec3::from_cartesian(point.to_radians());
        c.prev_ring = point;
        c.add(point);
    }

    /// The [`Listener::point`] handler used when drawing the rest of the points
    /// in a polygonal line.
    pub(super) fn ring_next_point(c: &mut Centroid, point: Vec2) {
        let point = Vec3::from_cartesian(point.to_radians());
        let prev_x_point = c.prev_ring.cross(point);
        let magnitude = prev_x_point.len();
        let u = c.prev_ring.dot(point);
        let area_weight = if magnitude == 0.0 {
            0.0
        } else {
            -clamp_acos(u) / magnitude
        };
        let line_weight = magnitude.atan2(u);
        c.area_weighted += prev_x_point * area_weight;
        c.length_magnitude += line_weight;
        c.length_weighted += (c.prev_ring + point) * line_weight;
        c.prev_ring = point;
        c.add(point);
    }

    /// The [`Listener::line_start`] handler used when drawing polygonal lines.
    pub(super) fn ring_start(c: &mut Centroid) {
        c.line_end = ring_end;
        c.point = ring_first_point;
    }
}

/// Options for projections that derive from the abstract conic projection.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub(super) struct ConicSettings {
    /// The parallels to use, in degrees.
    #[serde(default = "ConicSettings::default_parallels")]
    parallels: [f64; 2],
}

impl ConicSettings {
    /// The default value for [`Self::parallels`].
    const fn default_parallels() -> [f64; 2] {
        [0.0, 60.0]
    }

    /// Converts these parallels to a pair of radians.
    #[inline]
    fn to_radians(self) -> (f64, f64) {
        (
            self.parallels[0].to_radians(),
            self.parallels[1].to_radians(),
        )
    }
}

impl Default for ConicSettings {
    fn default() -> Self {
        Self {
            parallels: Self::default_parallels(),
        }
    }
}

/// A list of coefficients for the modified stereographic projection.
#[derive(Clone, Copy, Debug, Default, Hash, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ModifiedStereographicCoefficient {
    /// Alaska coefficients.
    Alaska,
    /// GS-48 coefficients.
    Gs48,
    /// GS-50 coefficients.
    Gs50,
    /// Lee coefficients.
    Lee,
    /// Miller coefficients.
    #[default]
    Miller,
}

impl ModifiedStereographicCoefficient {
    /// Gets the list of coefficients.
    fn get(self) -> &'static [[f64; 2]] {
        match self {
            Self::Alaska => &[
                [0.997_252_3, 0.0],
                [0.005_251_3, -0.004_117_5],
                [0.007_460_6, 0.004_812_5],
                [-0.015_378_3, -0.196_825_3],
                [0.063_687_1, -0.140_802_7],
                [0.366_097_6, -0.293_738_2],
            ],
            Self::Gs48 => &[
                [0.98879, 0.0],
                [0.0, 0.0],
                [-0.050_909, 0.0],
                [0.0, 0.0],
                [0.075_528, 0.0],
            ],
            Self::Gs50 => &[
                [0.984_299_0, 0.0],
                [0.021_164_2, 0.003_760_8],
                [-0.103_601_8, -0.057_510_2],
                [-0.032_909_5, -0.032_011_9],
                [0.049_947_1, 0.122_333_5],
                [0.026_046_0, 0.089_980_5],
                [0.000_738_8, -0.143_579_2],
                [0.007_584_8, -0.133_410_8],
                [-0.021_647_3, 0.077_664_5],
                [-0.022_516_1, 0.085_367_3],
            ],
            Self::Miller => &[[0.9245, 0.0], [0.0, 0.0], [0.019_43, 0.0]],
            Self::Lee => &[[0.721_316, 0.0], [0.0, 0.0], [-0.008_816_25, -0.006_173_25]],
        }
    }
}

/// The draw direction of a line segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Direction {
    /// Drawing forward.
    Forward,
    /// Drawing backward.
    Backward,
}

impl Direction {
    /// Returns true if this is [`Self::Forward`].
    #[inline]
    fn forward(self) -> bool {
        matches!(self, Self::Forward)
    }

    /// Returns a number that represents the sign of `self`.
    #[inline]
    fn signum(self) -> f64 {
        match self {
            Self::Forward => 1.0,
            Self::Backward => -1.0,
        }
    }
}

/// Options for projections that derive from the abstract parallel1 projection.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
pub(super) struct Parallel1Settings {
    /// The parallel to use, in degrees.
    #[serde(default)]
    parallel: f64,
}

impl Parallel1Settings {
    /// Converts this parallel to radians.
    #[inline]
    fn to_radians(self) -> f64 {
        self.parallel.to_radians()
    }
}

/// A cartographic projection.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "projection")]
pub(super) enum ProjectionSettings {
    /// Airy’s minimum-error azimuthal
    Airy {
        /// The radius to use, in degrees.
        #[serde(default = "ProjectionSettings::default_airy_radius")]
        radius: f64,
    },
    /// Aitoff
    Aitoff,
    /// Albers equal-area conic
    Albers {
        /// The parallels to use, in degrees.
        #[serde(default = "ProjectionSettings::default_albers_parallels")]
        parallels: [f64; 2],
    },
    /// Albers USA
    AlbersUsa,
    /// Armadillo
    Armadillo {
        /// The parallel to use, in degrees.
        #[serde(default = "ProjectionSettings::default_armadillo_parallel")]
        parallel: f64,
    },
    /// August conformal
    August,
    /// Lambert azimuthal equal-area
    AzimuthalEqualArea,
    /// azimuthal equidistant
    AzimuthalEquidistant,
    /// Baker Dinomic
    Baker,
    /// Berghaus Star
    Berghaus {
        /// The number of lobes to use.
        #[serde(default = "ProjectionSettings::default_berghaus_lobes")]
        lobes: f64,
    },
    /// Boggs eumorphic
    Boggs,
    /// Bonne
    Bonne {
        /// The parallel to use, in degrees.
        #[serde(default = "ProjectionSettings::default_bonne_parallel")]
        parallel: f64,
    },
    /// Bottomley
    Bottomley {
        /// The variant to use.
        #[serde(default = "ProjectionSettings::default_bottomley_variant")]
        variant: f64,
    },
    /// Bromley
    Bromley,
    /// Chamberlin trimetric
    Chamberlin {
        /// The points to use, in degrees latitude/longitude.
        #[serde(default = "ProjectionSettings::default_chamberlin_points")]
        points: [[f64; 2]; 3],
    },
    /// Collignon
    Collignon,
    /// Lambert conformal conic
    ConicConformal(ConicSettings),
    /// Conic equal area
    ConicEqualArea(ConicSettings),
    /// Conic equidistant
    ConicEquidistant(ConicSettings),
    /// Craig retroazimuthal
    Craig(Parallel1Settings),
    /// Craster parabolic
    Craster,
    /// Cylindrical equal-area, Gall–Peters, Hobo–Dyer, Tobler world-in-a-square
    CylindricalEqualArea(Parallel1Settings),
    /// Cylindrical stereographic, Gall’s stereographic
    CylindricalStereographic(Parallel1Settings),
    /// Eckert I
    Eckert1,
    /// Eckert II
    Eckert2,
    /// Eckert III
    Eckert3,
    /// Eckert IV
    Eckert4,
    /// Eckert V
    Eckert5,
    /// Eckert VI
    Eckert6,
    /// Eisenlohr conformal
    Eisenlohr,
    /// Equirectangular (Plate Carrée), Cassini
    Equirectangular,
    /// Fahey
    Fahey,
    /// Foucaut
    Foucaut,
    /// Gilbert’s two-world perspective (Note: this wraps a projection such as d3.geo.orthographic.)
    Gilbert,
    /// Gingery
    Gingery {
        /// The number of lobes to use.
        #[serde(default = "ProjectionSettings::default_gingery_lobes")]
        lobes: f64,

        /// The radius to use, in degrees.
        #[serde(default = "ProjectionSettings::default_gingery_radius")]
        radius: f64,
    },
    /// Ginzburg IV
    Ginzburg4,
    /// Ginzburg V
    Ginzburg5,
    /// Ginzburg VI
    Ginzburg6,
    /// Ginzburg VIII
    Ginzburg8,
    /// Ginzburg IX
    Ginzburg9,
    /// Gnomonic
    Gnomonic,
    /// Gringorten
    Gringorten(QuincuncialSettings),
    /// Guyou hemisphere-in-a-square
    Guyou(QuincuncialSettings),
    /// Hammer, Eckert–Greifendorff, quartic authalic, Briesemeister
    Hammer {
        /// The coefficient to use.
        #[serde(default = "ProjectionSettings::default_hammer_coefficient")]
        coefficient: f64,
    },
    /// Hammer retroazimuthal
    HammerRetroazimuthal {
        /// The parallel to use, in degrees.
        #[serde(default)]
        parallel: f64,
    },
    /// Hatano
    Hatano,
    /// Hierarchical Equal Area isoLatitude Pixelisation of a 2-sphere
    Healpix {
        /// The number of lobes.
        #[serde(default = "ProjectionSettings::default_healpix_lobes")]
        lobes: f64,
    },
    /// Hill eucyclic, Maurer No. 73
    Hill {
        /// The ratio to use.
        #[serde(default = "ProjectionSettings::default_hill_ratio")]
        ratio: f64,
    },
    /// Goode homolosine
    Homolosine,
    /// Kavrayskiy VII
    Kavrayskiy7,
    /// Lagrange conformal
    Lagrange {
        /// The spacing to use.
        #[serde(default = "ProjectionSettings::default_lagrange_spacing")]
        spacing: f64,
    },
    /// Larrivée
    Larrivee,
    /// Laskowski tri-optimal
    Laskowski,
    /// Littrow
    Littrow,
    /// Loximuthal
    Loximuthal {
        /// The parallel to use, in degrees.
        #[serde(default = "ProjectionSettings::default_loximuthal_parallel")]
        parallel: f64,
    },
    /// Mercator
    #[default]
    Mercator,
    /// Miller
    Miller,
    /// Modified stereographic
    ModifiedStereographic {
        /// The coefficients to use.
        #[serde(default)]
        coefficients: ModifiedStereographicCoefficient,
    },
    /// Mollweide, Atlantis
    Mollweide,
    /// McBryde–Thomas flat-polar parabolic
    MtFlatPolarParabolic,
    /// McBryde–Thomas flat-polar quartic
    MtFlatPolarQuartic,
    /// McBryde–Thomas flat-polar sinusoidal
    MtFlatPolarSinusoidal,
    /// Natural Earth
    NaturalEarth,
    /// Nell–Hammer
    NellHammer,
    /// Orthographic
    Orthographic,
    /// Patterson
    Patterson,
    /// Pierce quincuncial
    PeirceQuincuncial {
        /// If true, enables the quincuncial projection.
        #[serde(default = "ProjectionSettings::default_peirce_quincuncial")]
        quincuncial: bool,
    },
    /// Polyconic
    Polyconic,
    /// Rectangular polyconic
    RectangularPolyconic(Parallel1Settings),
    /// Robinson
    Robinson,
    /// Satellite (tilted perpsective)
    Satellite {
        /// The distance to use.
        #[serde(default = "ProjectionSettings::default_satellite_distance")]
        distance: f64,

        /// The tilt to use, in degrees.
        #[serde(default)]
        tilt: f64,
    },
    /// Sinu-Mollweide
    SinuMollweide,
    /// Sinusoidal
    Sinusoidal,
    /// Stereographic
    Stereographic,
    /// Times
    Times,
    /// Transverse Mercator
    TransverseMercator,
    /// Two-point azimuthal
    TwoPointAzimuthal {
        /// The points to use, in degrees latitude/longitude.
        #[serde(default)]
        points: [[f64; 2]; 2],
    },
    /// Two-point equidistant
    TwoPointEquidistant {
        /// The points to use, in degrees latitude/longitude.
        #[serde(default)]
        points: [[f64; 2]; 2],
    },
    /// Van der Grinten
    VanDerGrinten,
    /// Van der Grinten II
    VanDerGrinten2,
    /// Van der Grinten III
    VanDerGrinten3,
    /// Van der Grinten IV
    VanDerGrinten4,
    /// Wagner IV, Putniṇš P2´
    Wagner4,
    /// Wagner VI
    Wagner6,
    /// Wagner VII
    Wagner7,
    /// Wiechel
    Wiechel,
    /// Winkel tripel
    Winkel3,
}

impl ProjectionSettings {
    /// The default [`Self::Airy`] radius.
    const fn default_airy_radius() -> f64 {
        90.0
    }

    /// The default [`Self::Albers`] parallels.
    const fn default_albers_parallels() -> [f64; 2] {
        [29.5, 45.5]
    }

    /// The default [`Self::Armadillo`] parallel.
    const fn default_armadillo_parallel() -> f64 {
        20.0
    }

    /// The default [`Self::Berghaus`] lobes.
    const fn default_berghaus_lobes() -> f64 {
        5.0
    }

    /// The default [`Self::Bonne`] parallel.
    const fn default_bonne_parallel() -> f64 {
        45.0_f64
    }

    /// The default [`Self::Bottomley`] variant.
    const fn default_bottomley_variant() -> f64 {
        // Bottomley expects values in radians
        30.0_f64.to_radians()
    }

    /// The default [`Self::Chamberlin`] points.
    const fn default_chamberlin_points() -> [[f64; 2]; 3] {
        [[-150.0, 55.0], [-35.0, 55.0], [-92.5, 10.0]]
    }

    /// The default [`Self::Gingery`] lobes.
    const fn default_gingery_lobes() -> f64 {
        6.0
    }

    /// The default [`Self::Gingery`] radius.
    const fn default_gingery_radius() -> f64 {
        30.0
    }

    /// The default [`Self::Hammer`] coefficient.
    const fn default_hammer_coefficient() -> f64 {
        2.0
    }

    /// The default [`Self::Healpix`] lobes.
    const fn default_healpix_lobes() -> f64 {
        2.0
    }

    /// The default [`Self::Hill`] ratio.
    const fn default_hill_ratio() -> f64 {
        1.0
    }

    /// The default [`Self::Loximuthal`] parallel.
    const fn default_loximuthal_parallel() -> f64 {
        40.0
    }

    /// The default [`Self::PeirceQuincuncial`] quincuncial.
    const fn default_peirce_quincuncial() -> bool {
        true
    }

    /// The default [`Self::Lagrange`] spacing.
    const fn default_lagrange_spacing() -> f64 {
        0.5
    }

    /// The default [`Self::Satellite`] distance.
    const fn default_satellite_distance() -> f64 {
        1.4
    }

    /// Gets the default `ProjectorSettings` for a projector with this
    /// projection.
    fn defaults(self) -> ProjectorSettings {
        match self {
            Self::Albers { .. } => {
                // TODO: Also uses parallels.
                ProjectorSettings {
                    center: Some([-0.6, 38.7]),
                    projection: Self::ConicEqualArea(ConicSettings {
                        parallels: [29.5, 45.5],
                    }),
                    rotate: Some(Rotate::TwoD([96.0, 0.0])),
                    scale: Some(1070.0),
                    ..ProjectorSettings::baseline(self)
                }
            }
            Self::AlbersUsa => {
                // TODO: This thing is nuts! extends albers, overrides stream, precision, scale, and translate
                ProjectorSettings {
                    scale: Some(1070.0),
                    ..ProjectorSettings::baseline(self)
                }
            }
            Self::Gilbert => {
                // TODO: Replaces stream using an inner equirectangular
                // projection with these settings:
                // ProjectorSettings {
                //     scale: Some(1.0_f64.to_degrees()),
                //     translate: Some([0.0, 0.0]),
                //     ..ProjectorSettings::baseline(self)
                // }
                ProjectorSettings::baseline(self)
            }
            Self::PeirceQuincuncial { .. } => ProjectorSettings {
                clip_angle: Some(180.0 - 1e-6),
                rotate: Some(Rotate::ThreeD([-90.0, -90.0, 45.0])),
                ..ProjectorSettings::baseline(self)
            },
            Self::SinuMollweide => ProjectorSettings {
                rotate: Some(Rotate::TwoD([-20.0, -55.0])),
                ..ProjectorSettings::baseline(self)
            },
            Self::TransverseMercator => {
                // TODO: replaces center, rotate
                ProjectorSettings {
                    rotate: Some(Rotate::ThreeD([0.0, 0.0, 90.0])),
                    ..Self::Mercator.defaults()
                }
            }
            _ => ProjectorSettings::baseline(self),
        }
    }
}

/// A cartographic projector.
#[derive(Clone, Debug)]
pub(super) struct Projector {
    /// The translated origin of the projection, in cartesian coordinates.
    center: Vec2,

    /// The clip angle of the projection.
    clip_angle: Option<f64>,

    /// The clipping box of the projection.
    extent: Option<Rect>,

    /// The desired precision of the projection.
    precision: Option<f64>,

    /// The projection.
    projection: projection::Projection,

    /// The rotation of the projection.
    rotate: Rotator,

    /// The scale of the projection.
    scale: f64,
}

impl Projector {
    /// Creates a new `Projector` using the given settings.
    pub fn new(s: &ProjectorSettings) -> Self {
        let s = s.projection.defaults().merge(s);
        let scale = s.scale.unwrap_or(150.0);
        let translate = s
            .translate
            .map_or(Vec2::new(480.0, 250.0), |[x, y]| Vec2::new(x, y));
        let projection = projection::Projection::new(&s);
        let rotate = projection.rotate().or(s.rotate).unwrap_or_default();
        let mut this = Self {
            center: Vec2::zero(),
            clip_angle: s.clip_angle.map(f64::to_radians),
            extent: s
                .clip_extent
                .map(|[[left, top], [right, bottom]]| Rect::new(left, top, right, bottom)),
            precision: s.precision,
            projection,
            rotate: Rotator::from(rotate),
            scale,
        };
        // TODO: There needs to be a separate trait for built projections that
        // allows them to do different stuff because AlbersUsa is a unique
        // snowflake.
        if !matches!(s.projection, ProjectionSettings::AlbersUsa) {
            let center = s
                .center
                .map_or(<_>::default(), |[x, y]| Vec2::new(x, y).to_radians());
            let center = this.projection.apply(center) * scale;
            this.center = Vec2::new(translate.x - center.x, translate.y + center.y);
        }
        this
    }

    /// Converts a GeoJSON object into an SVG path.
    pub fn path(&self, object: &Value<'_>) -> String {
        self.with_listener(SvgPathOutput::new(), |listener| {
            geojson::emit_object(listener, object);
        })
        .finish()
    }

    /// Applies the [`Projection`] specified by [`ProjectorSettings::projection`]
    /// to the given point.
    #[inline]
    fn project(&self, point: Vec2) -> Vec2 {
        self.projection.apply(point)
    }

    /// Projects a single geographic `point`, in degrees latitude and longitude,
    /// from the cartographic space into cartesian space, with rotation,
    /// scaling, and translation.
    #[inline]
    pub fn projection_degrees(&self, point: Vec2) -> Vec2 {
        let point = self.project(self.rotate(point.to_radians()));
        // TODO: There needs to be a trait for projections that lets them do
        // insane things.
        if matches!(self.projection, projection::Projection::AlbersUsa { .. }) {
            point.to_degrees()
        } else {
            self.scale(point)
        }
    }

    /// Projects a single geographic `point`, in radians latitude and longitude,
    /// from the cartographic space into cartesian space, with scaling and
    /// translation.
    fn project_resample(&self, point: Vec2) -> Vec2 {
        self.scale(self.project(point))
    }

    /// Rotates the given point according to the projection’s [`Self::rotate`]
    /// setting.
    fn rotate(&self, point: Vec2) -> Vec2 {
        self.rotate.rotate(point)
    }

    /// Reverses the rotation of the given point according to the projection’s
    /// [`Self::rotate`] setting.
    fn rotate_invert(&self, point: Vec2) -> Vec2 {
        self.rotate.invert(point)
    }

    /// Scales and translates the given `point` around [`Self::center`].
    #[inline]
    fn scale(&self, point: Vec2) -> Vec2 {
        Vec2::new(
            point.x * self.scale + self.center.x,
            self.center.y - point.y * self.scale,
        )
    }

    /// Invokes `f` with a mutable reference to the composed listener for this
    /// projector, using the given `output_listener` as the target output.
    /// Returns `output_listener`.
    fn with_listener<F, L>(&self, mut output_listener: L, f: F) -> L
    where
        F: FnOnce(&mut dyn Listener),
        L: Listener,
    {
        let postclip = if let Some(extent) = self.extent {
            &mut clip::ClipExtent::<&mut dyn Listener>::new(extent, &mut output_listener)
                as &mut dyn Listener
        } else {
            &mut output_listener
        };

        let project_resample = if self.precision == Some(0.0) {
            &mut resample::NullResampler::new(self, postclip)
        } else {
            &mut resample::Resampler::new(self, postclip, self.precision) as &mut dyn Listener
        };

        let preclip = if let Some(radius) = self.clip_angle {
            &mut clip::Clip::<clip::ClipCircle<_>, clip::ClipCircle<_>, _>::new(
                self,
                project_resample,
                radius,
            ) as &mut dyn Listener
        } else {
            &mut clip::Clip::<clip::ClipAntimeridian<_>, clip::ClipAntimeridian<_>, _>::new(
                self,
                project_resample,
                (),
            ) as &mut dyn Listener
        };

        // D3 also chains a degrees-to-radians converter, but this
        // implementation just has `Self::point` converts to radians itself so
        // that it is not necessary to stick yet another slow layer of
        // indirection in the chain of slow indirections and everything internal
        // can just use radians
        f(preclip);

        output_listener
    }
}

/// Functions for emitting GeoJSON objects to [`Listener`]s.
mod geojson {
    use super::{super::data::ValueExt as _, Listener, Value, Vec2};

    /// Emits a GeoJSON `line` to the given `listener`.
    pub(super) fn emit_line<L>(listener: &mut L, line: &Value<'_>, closed: bool)
    where
        L: Listener + ?Sized,
    {
        let points = line.as_array().unwrap();
        listener.line_start();
        for point in &points[..points.len() - usize::from(closed)] {
            emit_point(listener, point);
        }
        listener.line_end();
    }

    /// Emits the given GeoJSON `object` to the given `listener`.
    pub(super) fn emit_object<L>(listener: &mut L, object: &Value<'_>)
    where
        L: Listener + ?Sized,
    {
        match object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "Sphere" => listener.sphere(),
            "Point" => {
                if let Some(point) = object.get("coordinates") {
                    emit_point(listener, point);
                }
            }
            "MultiPoint" => {
                if let Some(points) = object.get("coordinates").and_then(Value::as_array) {
                    for point in points {
                        emit_point(listener, point);
                    }
                }
            }
            "LineString" => {
                if let Some(line) = object.get("coordinates") {
                    emit_line(listener, line, false);
                }
            }
            "MultiLineString" => {
                if let Some(lines) = object.get("coordinates").and_then(Value::as_array) {
                    for line in lines {
                        emit_line(listener, line, false);
                    }
                }
            }
            "Polygon" => {
                if let Some(poly) = object.get("coordinates") {
                    emit_polygon(listener, poly);
                }
            }
            "MultiPolygon" => {
                if let Some(polys) = object.get("coordinates").and_then(Value::as_array) {
                    for poly in polys {
                        emit_polygon(listener, poly);
                    }
                }
            }
            "GeometryCollection" => {
                if let Some(geoms) = object.get("geometries").and_then(Value::as_array) {
                    for geom in geoms {
                        emit_object(listener, geom);
                    }
                }
            }
            "Feature" => {
                if let Some(geom) = object.get("geometry") {
                    emit_object(listener, geom);
                }
            }
            "FeatureCollection" => {
                if let Some(features) = object.get("features").and_then(Value::as_array) {
                    for geom in features.iter().filter_map(|f| f.get("geometry")) {
                        emit_object(listener, geom);
                    }
                }
            }
            _ => {}
        }
    }

    /// Emits a GeoJSON `point` to the given `listener`.
    pub(super) fn emit_point<L>(listener: &mut L, point: &Value<'_>)
    where
        L: Listener + ?Sized,
    {
        let point = point.as_array().unwrap();
        listener.point(Vec2::new(point[0].to_f64(), point[1].to_f64()).to_radians());
    }

    /// Emits a GeoJSON `poly` to the given `listener`.
    pub(super) fn emit_polygon<L>(listener: &mut L, poly: &Value<'_>)
    where
        L: Listener + ?Sized,
    {
        let lines = poly.as_array().unwrap();
        listener.polygon_start();
        for line in lines {
            emit_line(listener, line, true);
        }
        listener.polygon_end();
    }
}

/// Common cartographic projection transformer options.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProjectorSettings {
    /// The center of the projection.
    #[serde(default)]
    center: Option<[f64; 2]>,

    /// The clip angle of the projection.
    #[serde(default)]
    clip_angle: Option<f64>,

    /// The clip extent of the projection.
    #[serde(default)]
    clip_extent: Option<[[f64; 2]; 2]>,

    /// The desired precision of the projection.
    #[serde(default)]
    precision: Option<f64>,

    /// The kind of cartographic projection to use.
    #[serde(default, flatten)]
    projection: ProjectionSettings,

    /// The rotation of the projection.
    #[serde(default)]
    rotate: Option<Rotate>,

    /// The scale of the projection.
    #[serde(default)]
    scale: Option<f64>,

    /// The translation of the projection.
    #[serde(default)]
    translate: Option<[f64; 2]>,
}

impl ProjectorSettings {
    /// Creates a new `ProjectorSettings` with default settings.
    fn baseline(projection: ProjectionSettings) -> Self {
        Self {
            projection,
            ..Default::default()
        }
    }

    /// Creates a new `ProjectorSettings` by overwriting any properties of
    /// `self` with non-`None` properties of `other`.
    fn merge(&self, other: &Self) -> Self {
        Self {
            center: self.center.or(other.center),
            clip_angle: self.clip_angle.or(other.clip_angle),
            clip_extent: self.clip_extent.or(other.clip_extent),
            precision: self.precision.or(other.precision),
            projection: other.projection,
            rotate: self.rotate.or(other.rotate),
            scale: self.scale.or(other.scale),
            translate: self.translate.or(other.translate),
        }
    }
}

/// Options for projections that derive from the abstract quincuncial
/// projection.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
pub(super) struct QuincuncialSettings {
    /// If true, enables the quincuncial projection.
    #[serde(default)]
    quincuncial: bool,
}

/// A rotation vector.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum Rotate {
    /// Yaw, in degrees.
    OneD(f64),
    /// Pitch and roll, in degrees.
    TwoD([f64; 2]),
    /// Yaw, pitch, roll, in degrees.
    ThreeD([f64; 3]),
}

impl Default for Rotate {
    fn default() -> Self {
        Self::OneD(0.0)
    }
}

/// An event listener trait for streaming shapes.
trait Listener {
    /// Finish drawing the line.
    fn line_end(&mut self);
    /// Start drawing a new line.
    fn line_start(&mut self);
    /// Draw a point.
    fn point(&mut self, point: Vec2);
    /// Finish drawing the polygon.
    fn polygon_end(&mut self);
    /// Start drawing a new polygon.
    fn polygon_start(&mut self);
    /// Draw an approximation of a sphere by plotting nine points equally spaced
    /// around `(0,0)` in a clockwise direction, starting at the top-left
    /// (quadrant I) corner, with `rx = π` and `ry = π / 2`.
    ///
    /// This is a non-standard shape type which does not correspond to any
    /// shape type in the GeoJSON specification.
    fn sphere(&mut self);
}

impl Listener for &mut dyn Listener {
    fn point(&mut self, point: Vec2) {
        (**self).point(point);
    }

    fn line_start(&mut self) {
        (**self).line_start();
    }

    fn line_end(&mut self) {
        (**self).line_end();
    }

    fn polygon_start(&mut self) {
        (**self).polygon_start();
    }

    fn polygon_end(&mut self) {
        (**self).polygon_end();
    }

    fn sphere(&mut self) {
        (**self).sphere();
    }
}

/// An adder for floating point numbers with twice the normal precision.
///
/// Reference: J. R. Shewchuk, Adaptive Precision Floating-Point Arithmetic and
/// Fast Robust Geometric Predicates, Discrete & Computational Geometry 18(3)
/// 305–363 (1997).
// Code adapted from GeographicLib by Charles F. F. Karney,
// http://geographiclib.sourceforge.net/
#[derive(Clone, Copy, Default)]
struct Adder {
    /// The accumulated error.
    error: f64,
    /// The rounded value.
    rounded: f64,
}

impl Adder {
    /// Creates a new `Adder` with the given operands.
    fn new(lhs: f64, rhs: f64) -> Self {
        let rounded = lhs + rhs;
        let b_virtual = rounded - lhs;
        let a_virtual = rounded - b_virtual;
        Self {
            error: (lhs - a_virtual) + (rhs - b_virtual),
            rounded,
        }
    }
}

impl core::ops::Add<f64> for Adder {
    type Output = Self;

    fn add(self, rhs: f64) -> Self::Output {
        let partial = Adder::new(rhs, self.error);
        let mut result = Adder::new(partial.rounded, self.rounded);
        if result.rounded == 0.0 {
            result.rounded = partial.error;
        } else {
            result.error += partial.error;
        }
        result
    }
}

impl core::ops::AddAssign<f64> for Adder {
    fn add_assign(&mut self, rhs: f64) {
        *self = *self + rhs;
    }
}

impl core::cmp::PartialEq<f64> for Adder {
    fn eq(&self, other: &f64) -> bool {
        self.rounded.eq(other)
    }
}

impl core::cmp::PartialOrd<f64> for Adder {
    fn partial_cmp(&self, other: &f64) -> Option<Ordering> {
        self.rounded.partial_cmp(other)
    }
}

/// A thing that spins things in up to three dimensions.
#[derive(Clone, Copy, Debug)]
enum Rotator {
    /// Just normalising.
    Identity,
    /// Yaw, in radians.
    OneD(f64),
    /// Pitch and roll, in radians.
    TwoD {
        /// Pitch sin/cos.
        pitch: (f64, f64),
        /// Roll sin/cos.
        roll: (f64, f64),
    },
    /// Yaw, pitch, and roll, in radians.
    ThreeD {
        /// Yaw, in radians.
        yaw: f64,
        /// Pitch sin/cos.
        pitch: (f64, f64),
        /// Roll sin/cos.
        roll: (f64, f64),
    },
}

impl Rotator {
    /// Rotate the point backwards.
    fn invert(self, point: Vec2) -> Vec2 {
        match self {
            Self::Identity => point,
            Self::OneD(yaw) => Self::yaw(-yaw, point),
            Self::TwoD { pitch, roll } => Self::roll_invert(pitch, roll, point),
            Self::ThreeD { yaw, pitch, roll } => {
                Self::yaw(-yaw, Self::roll_invert(pitch, roll, point))
            }
        }
    }

    /// Normalise a point lambda to the range `-PI..=PI`.
    fn norm(point: Vec2) -> Vec2 {
        Vec2::new(
            if point.x > PI {
                point.x - TAU
            } else if point.x < -PI {
                point.x + TAU
            } else {
                point.x
            },
            point.y,
        )
    }

    /// Pitch and roll the point.
    fn roll(
        (sin_pitch, cos_pitch): (f64, f64),
        (sin_roll, cos_roll): (f64, f64),
        point: Vec2,
    ) -> Vec2 {
        let (sin_l, cos_l) = point.x.sin_cos();
        let (z, cos_p) = point.y.sin_cos();
        let (x, y) = (cos_l * cos_p, sin_l * cos_p);
        let k = z * cos_pitch + x * sin_pitch;
        Vec2::new(
            (y * cos_roll - k * sin_roll).atan2(x * cos_pitch - z * sin_pitch),
            clamp_asin(k * cos_roll + y * sin_roll),
        )
    }

    /// Invert pitch and roll.
    fn roll_invert(
        (sin_pitch, cos_pitch): (f64, f64),
        (sin_roll, cos_roll): (f64, f64),
        point: Vec2,
    ) -> Vec2 {
        let (sin_l, cos_l) = point.x.sin_cos();
        let (z, cos_p) = point.y.sin_cos();
        let (x, y) = (cos_l * cos_p, sin_l * cos_p);
        let k = z * cos_roll - y * sin_roll;
        Vec2::new(
            (y * cos_roll + z * sin_roll).atan2(x * cos_pitch + k * sin_pitch),
            clamp_asin(k * cos_pitch - x * sin_pitch),
        )
    }

    /// Rotates the given `point`, in radians, returning the rotated point,
    /// in radians.
    fn rotate(self, point: Vec2) -> Vec2 {
        match self {
            Self::Identity => Self::norm(point),
            Self::OneD(yaw) => Self::yaw(yaw, point),
            Self::TwoD { pitch, roll } => Self::roll(pitch, roll, point),
            Self::ThreeD { yaw, pitch, roll } => Self::roll(pitch, roll, Self::yaw(yaw, point)),
        }
    }

    /// Yaw the point.
    fn yaw(yaw: f64, point: Vec2) -> Vec2 {
        Self::norm(Vec2::new(point.x + yaw, point.y))
    }
}

impl From<Rotate> for Rotator {
    fn from(value: Rotate) -> Self {
        match value {
            Rotate::ThreeD([yaw, pitch, roll]) if yaw != 0.0 && (pitch != 0.0 || roll != 0.0) => {
                Self::ThreeD {
                    yaw: yaw.to_radians(),
                    pitch: pitch.to_radians().sin_cos(),
                    roll: roll.to_radians().sin_cos(),
                }
            }
            Rotate::TwoD([yaw, pitch]) if yaw != 0.0 && pitch != 0.0 => Self::ThreeD {
                yaw: yaw.to_radians(),
                pitch: pitch.to_radians().sin_cos(),
                roll: 0.0_f64.sin_cos(),
            },
            Rotate::ThreeD([_, pitch, roll]) if pitch != 0.0 || roll != 0.0 => Self::TwoD {
                pitch: pitch.to_radians().sin_cos(),
                roll: roll.to_radians().sin_cos(),
            },
            Rotate::OneD(yaw) | Rotate::TwoD([yaw, _]) | Rotate::ThreeD([yaw, _, _])
                if yaw != 0.0 =>
            {
                Self::OneD(yaw.to_radians())
            }
            _ => Self::Identity,
        }
    }
}

/// An output event listener which constructs an SVG path.
struct SvgPathOutput<'a, 'b> {
    /// The SVG path.
    out: String,
    /// The current [`Listener::point`] handler.
    point: fn(&mut Self, Vec2),
    /// The current [`Listener::line_end`] handler.
    line_end: fn(&mut Self),
}

impl SvgPathOutput<'_, '_> {
    /// The path used to draw standalone points.
    #[rustfmt::skip]
    const CIRCLE: &'static str = {
        concat!(
            "m0,", 4.5,
            "a", 4.5, ",", 4.5, ",0,1,1,0,", -9, /* -2 * 4.5 */
            "a", 4.5, ",", 4.5, ",0,1,1,0,",  9, /* 2 * 4.5 */
            "z"
        )
    };

    /// Creates a new `PathListener`.
    fn new() -> Self {
        Self {
            out: <_>::default(),
            point: svg_path_output::point,
            line_end: svg_path_output::line_end,
        }
    }

    /// Returns the generated SVG path, consuming the listener.
    fn finish(self) -> String {
        self.out
    }
}

impl Listener for SvgPathOutput<'_, '_> {
    fn point(&mut self, point: Vec2) {
        (self.point)(self, point);
    }

    fn line_start(&mut self) {
        self.point = svg_path_output::point_line_start;
    }

    fn line_end(&mut self) {
        (self.line_end)(self);
    }

    fn polygon_start(&mut self) {
        self.line_end = svg_path_output::line_end_polygon;
    }

    fn polygon_end(&mut self) {
        self.line_end = svg_path_output::line_end;
        self.point = svg_path_output::point;
    }

    fn sphere(&mut self) {}
}

/// Swappable listener functions for [`SvgPathOutput`].
mod svg_path_output {
    #[cfg(doc)]
    use super::Listener;
    use super::{super::super::svg::ValueDisplay as _, SvgPathOutput, Vec2};
    use core::fmt::Write as _;

    /// The [`Listener::line_end`] handler used when drawing a non-polygonal
    /// line segment.
    pub(super) fn line_end(p: &mut SvgPathOutput<'_, '_>) {
        p.point = point;
    }

    /// The [`Listener::line_end`] handler used when drawing a line segment for
    /// a polygon.
    pub(super) fn line_end_polygon(p: &mut SvgPathOutput<'_, '_>) {
        p.out.push('Z');
    }

    /// The [`Listener::point`] handler used when drawing standalone points.
    pub(super) fn point(p: &mut SvgPathOutput<'_, '_>, point: Vec2) {
        let _ = write!(
            p.out,
            "M{x},{y}{c}",
            x = point.x.v(),
            y = point.y.v(),
            c = SvgPathOutput::CIRCLE
        );
    }

    /// The [`Listener::point`] handler used when drawing points on a line after
    /// the first point.
    pub(super) fn point_line(p: &mut SvgPathOutput<'_, '_>, point: Vec2) {
        let _ = write!(p.out, "L{x},{y}", x = point.x.v(), y = point.y.v());
    }

    /// The [`Listener::point`] handler used to draw the first point on a line.
    pub(super) fn point_line_start(p: &mut SvgPathOutput<'_, '_>, point: Vec2) {
        let _ = write!(p.out, "M{x},{y}", x = point.x.v(), y = point.y.v());
        p.point = point_line;
    }
}

/// TopoJSON to GeoJSON conversion functions.
///
/// “TopoJSON is an extension of GeoJSON that encodes topology. Rather than
/// representing geometries discretely, geometries in TopoJSON files are
/// stitched together from shared line segments called *arcs*.”
// SPDX-SnippetBegin
// SPDX-License-Identifier: BSD-3-clause
// SPDX-SnippetComment: Adapted from topojson 1.6 by Michael Bostock
pub(super) mod topojson {
    use super::super::data::ValueExt;
    use indexmap::IndexMap;
    use serde_json_borrow::Value;
    use std::{
        cell::RefCell,
        collections::{BTreeMap, BTreeSet},
        rc::Rc,
    };

    /// Converts a TopoJSON geometry object into one or more GeoJSON Feature
    /// objects.
    pub(crate) fn features<'s>(topology: &Value<'s>, o: &Value<'s>) -> Vec<Value<'s>> {
        if o.get("type").and_then(Value::as_str) == Some("GeometryCollection") {
            get_array(o, "geometries")
                .iter()
                .map(|o| feature(topology, o))
                .collect()
        } else {
            vec![feature(topology, o)]
        }
    }

    /// Converts a TopoJSON geometry object into a GeoJSON Geometry object.
    pub(crate) fn mesh<'s>(topology: &Value<'s>, o: &Value<'s>) -> Value<'s> {
        let arcs = mesh_arcs(topology, o);
        object(topology, &arcs)
    }

    /// Converts a TopoJSON geometry object into a GeoJSON Feature object.
    fn feature<'s>(topology: &Value<'s>, o: &Value<'s>) -> Value<'s> {
        let mut f = Value::from([
            ("type", "Feature".into()),
            (
                "properties",
                o.get("properties")
                    .cloned()
                    .unwrap_or(Value::Object(<_>::default())),
            ),
            ("geometry", object(topology, o)),
        ]);

        if let Some(id) = o.get("id") {
            f.insert("id", id.clone());
        }

        f
    }

    /// Converts the given TopoJSON geometry `object` into a GeoJSON object.
    fn object<'s>(topology: &Value<'s>, object: &Value<'s>) -> Value<'s> {
        GeoConvert::new(topology).geometry(object)
    }

    /// An accumulator for mesh arc indexes.
    #[derive(Default)]
    struct ArcIndexCollector {
        /// Accumulated arc references.
        ///
        /// Because Vega only cares about the single mesh path and never
        /// filters, this is simplified to exclude the geometry objects, since
        /// they are only used on the code path that filters.
        ///
        /// It is necessary to use an ordered map instead of a Vec because this
        /// may be a sparse array, and because the *first* inserted index is the
        /// one that TopoJSON uses.
        arcs: BTreeMap<usize, i32>,
    }

    impl ArcIndexCollector {
        /// Collect the given arc index.
        fn arc(&mut self, i: i32) {
            let index = arc_index(i);
            self.arcs.entry(index).or_insert(i);
        }

        /// Collect all arc indexes for the given line.
        fn line(&mut self, arcs: &[Value<'_>]) {
            for index in arcs {
                // Clippy: TopoJSON defines these values as i32.
                #[allow(clippy::cast_possible_truncation)]
                self.arc(index.to_f64() as i32);
            }
        }

        /// Collect all arc indexes for the given polygon.
        fn polygon(&mut self, arcs: &[Value<'_>]) {
            for line in arcs {
                self.line(line.as_array().unwrap());
            }
        }

        /// Collect all arc indexes for the given geometry object.
        fn geometry(&mut self, o: &Value<'_>) {
            match o.get("type").and_then(Value::as_str).unwrap_or_default() {
                "GeometryCollection" => {
                    for geom in get_array(o, "geometries") {
                        self.geometry(geom);
                    }
                }
                "LineString" => {
                    self.line(get_array(o, "arcs"));
                }
                "MultiLineString" | "Polygon" => {
                    self.polygon(get_array(o, "arcs"));
                }
                "MultiPolygon" => {
                    for arc in get_array(o, "arcs") {
                        self.polygon(arc.as_array().unwrap());
                    }
                }
                _ => {}
            }
        }
    }

    /// Converts an arc index into an array index.
    // Clippy: Because TopoJSON defines bit-not to invert negative indexes, the
    // arc indexes must be no greater than i32 because ECMAScript uses that type
    // for bit operations. Negative values are one’s-complement inverted.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[inline]
    fn arc_index(i: i32) -> usize {
        (if i < 0 { !i } else { i }) as usize
    }

    /// Unconditionally retrieves an array at the given `key` of the given
    /// `object`, panicking if no such key or array exists.
    #[inline]
    fn get_array<'b, 's>(object: &'b Value<'s>, key: &str) -> &'b [Value<'s>] {
        object.get(key).and_then(Value::as_array).unwrap()
    }

    /// A TopoJSON geometry object converter.
    struct GeoConvert<'b, 's> {
        /// The list of arcs.
        arcs: &'b [Value<'s>],
        /// The delta coordinate transformer.
        transformer: Option<Transformer>,
    }

    impl<'b, 's> GeoConvert<'b, 's> {
        /// Creates a new converter from the given `topology`.
        fn new(topology: &'b Value<'s>) -> Self {
            Self {
                arcs: topology.get("arcs").and_then(Value::as_array).unwrap(),
                transformer: topology.get("transform").map(Transformer::new),
            }
        }

        /// Adds the arc at the given `arc_index` to the given list of `points`.
        fn arc(&mut self, points: &mut Vec<Value<'s>>, index: i32) {
            points.pop();

            let arc = self.arcs[arc_index(index)].as_array().unwrap();
            for (index, mut point) in arc.iter().cloned().enumerate() {
                if let Some(transformer) = &mut self.transformer {
                    transformer.absolute(point.as_array_mut().unwrap(), index);
                }
                points.push(point);
            }

            if index < 0 {
                let start = points.len() - arc.len();
                points[start..].reverse();
            }
        }

        /// Converts the given TopoJSON geometry `object` to a GeoJSON geometry
        /// object.
        fn geometry(&mut self, object: &'b Value<'s>) -> Value<'s> {
            let t = object
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let mut geom = Value::from([("type", t.to_owned())]);
            let (k, v) = match t {
                "GeometryCollection" => {
                    let geometries = get_array(object, "geometries")
                        .iter()
                        .map(|geom| self.geometry(geom))
                        .collect::<Vec<_>>();
                    ("geometries", Value::Array(geometries))
                }
                "Point" => {
                    let point = self.point(get_array(object, "coordinates"));
                    ("coordinates", Value::Array(point))
                }
                "MultiPoint" => {
                    let points = get_array(object, "coordinates")
                        .iter()
                        .map(|point| Value::Array(self.point(point.as_array().unwrap())))
                        .collect::<Vec<_>>();
                    ("coordinates", Value::Array(points))
                }
                "LineString" => {
                    let line = self.line(get_array(object, "arcs"));
                    ("coordinates", Value::Array(line))
                }
                "MultiLineString" => {
                    let lines = get_array(object, "arcs")
                        .iter()
                        .map(|arcs| Value::Array(self.line(arcs.as_array().unwrap())))
                        .collect::<Vec<_>>();
                    ("coordinates", Value::Array(lines))
                }
                "Polygon" => {
                    let poly = self.polygon(get_array(object, "arcs"));
                    ("coordinates", Value::Array(poly))
                }
                "MultiPolygon" => {
                    let polys = get_array(object, "arcs")
                        .iter()
                        .map(|arcs| Value::Array(self.polygon(arcs.as_array().unwrap())))
                        .collect::<Vec<_>>();
                    ("coordinates", Value::Array(polys))
                }
                _ => return Value::Null,
            };
            geom.insert(k, v);
            geom
        }

        /// Creates a line from the given list of arc indexes.
        fn line(&mut self, arcs: &[Value<'s>]) -> Vec<Value<'s>> {
            let mut points = vec![];
            for arc_index in arcs {
                // Clippy: TopoJSON defines these values as i32.
                #[allow(clippy::cast_possible_truncation)]
                self.arc(&mut points, arc_index.to_f64() as i32);
            }
            if points.len() < 2 {
                points.push(points.first().cloned().unwrap());
            }
            points
        }

        /// Converts the given delta-encoded point to an absolute coordinate.
        fn point(&mut self, point: &[Value<'s>]) -> Vec<Value<'s>> {
            let mut point = point.to_vec();
            if let Some(transformer) = &mut self.transformer {
                transformer.absolute(&mut point, 0);
            }
            point
        }

        /// Creates a polygon from the given list of linear rings.
        fn polygon(&mut self, rings: &[Value<'s>]) -> Vec<Value<'s>> {
            rings
                .iter()
                .map(|arc| Value::Array(self.ring(arc.as_array().unwrap())))
                .collect()
        }

        /// Creates a linear ring from the given list of arc indexes.
        fn ring(&mut self, arcs: &[Value<'s>]) -> Vec<Value<'s>> {
            let mut points = self.line(arcs);
            while points.len() < 4 {
                points.push(points.first().cloned().unwrap());
            }
            points
        }
    }

    /// Takes the arcs from the given `object` and converts them into a GeoJSON
    /// multi-line string object.
    fn mesh_arcs<'s>(topology: &Value<'s>, object: &Value<'s>) -> Value<'s> {
        let mut converter = ArcIndexCollector::default();
        converter.geometry(object);
        Value::from([
            ("type", Value::Str("MultiLineString".into())),
            (
                "arcs",
                stitch_arcs(topology, converter.arcs.into_values().collect()).into(),
            ),
        ])
    }

    /// Converts a list of arc indexes into a list of line fragments.
    fn stitch_arcs<'s>(topology: &Value<'s>, mut arc_indexes: Vec<i32>) -> Vec<Value<'s>> {
        use stitch_arcs::{
            FragMap, Fragment, HashF64, RcFragment, combine, ends, finalize, insert, to_point,
        };

        let topo_arcs = get_array(topology, "arcs");
        let transform = topology.get("transform").is_some_and(ValueExt::to_bool);

        let mut fragment_by_start = FragMap::new();
        let mut fragment_by_end = FragMap::new();

        let mut empty_index = 0;
        for local_index in 0..arc_indexes.len() {
            let topo_index = arc_indexes[local_index];
            let arc = topo_arcs[arc_index(topo_index)].as_array().unwrap();
            if arc.len() < 3 && to_point(&arc[1]) == [HashF64(0.0), HashF64(0.0)] {
                arc_indexes[local_index] =
                    core::mem::replace(&mut arc_indexes[empty_index], topo_index);
                empty_index += 1;
            }
        }

        for &topo_index in &arc_indexes {
            let (start, end) = ends(topo_arcs, transform, topo_index);

            if let Some(f) = fragment_by_end.get(&start).cloned() {
                {
                    let mut f = f.borrow_mut();
                    fragment_by_end.shift_remove(&f.end);
                    f.data.push(topo_index);
                    f.end = end;
                }

                if let Some(g) = fragment_by_start.get(&end).cloned() {
                    fragment_by_start.shift_remove(&g.borrow().start);
                    let fg = combine(f, &g);
                    insert(&mut fragment_by_start, &mut fragment_by_end, fg);
                } else {
                    insert(&mut fragment_by_start, &mut fragment_by_end, Rc::clone(&f));
                }
            } else if let Some(f) = fragment_by_start.get_mut(&end).cloned() {
                {
                    let mut f = f.borrow_mut();
                    fragment_by_start.shift_remove(&f.start);
                    f.data.insert(0, topo_index);
                    f.start = start;
                }

                if let Some(g) = fragment_by_end.get(&start).cloned() {
                    fragment_by_end.shift_remove(&g.borrow().end);
                    let gf = combine(g, &f);
                    insert(&mut fragment_by_start, &mut fragment_by_end, gf);
                } else {
                    insert(&mut fragment_by_start, &mut fragment_by_end, Rc::clone(&f));
                }
            } else {
                let f = RcFragment::new(
                    Fragment {
                        start,
                        end,
                        data: vec![topo_index],
                    }
                    .into(),
                );
                fragment_by_start.insert(start, Rc::clone(&f));
                fragment_by_end.insert(end, f);
            }
        }

        finalize(arc_indexes, fragment_by_start, fragment_by_end)
    }

    /// Helper functions and types for [`stitch_arcs`](fn@stitch_arcs).
    mod stitch_arcs {
        use super::{BTreeSet, IndexMap, Rc, RefCell, Value, ValueExt, arc_index};

        /// A line fragment.
        #[derive(Clone)]
        pub(super) struct Fragment {
            /// The start point of the line.
            pub start: Point,
            /// The end point of the line.
            pub end: Point,
            /// The arc indexes contributing to the line.
            pub data: Vec<i32>,
        }

        /// A hashable newtype for f64.
        #[derive(Clone, Copy, PartialEq)]
        pub(super) struct HashF64(pub f64);
        impl core::hash::Hash for HashF64 {
            fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                if self.0 == 0.0 {
                    // Treat ±0.0 the same
                    0.0_f64
                } else if self.0.is_nan() {
                    // Treat all NaN the same
                    f64::NAN
                } else {
                    self.0
                }
                .to_bits()
                .hash(state);
            }
        }
        impl Eq for HashF64 {}

        /// A map from a point to a corresponding fragment.
        pub(super) type FragMap = IndexMap<Point, RcFragment>;
        /// An intermediate 2-dimensional point type.
        pub(super) type Point = [HashF64; 2];
        /// A reference-counted line fragment.
        pub(super) type RcFragment = Rc<RefCell<Fragment>>;

        /// Creates a new fragment by combining `a` and `b`, or uses `a` as-is
        /// if `a` and `b` are the same fragment.
        pub(super) fn combine(a: RcFragment, b: &RcFragment) -> RcFragment {
            if Rc::ptr_eq(&a, b) {
                a
            } else {
                let a = a.borrow();
                let b = b.borrow();
                let mut data = a.data.clone();
                data.extend(&b.data);
                RcFragment::new(
                    Fragment {
                        start: a.start,
                        end: b.end,
                        data,
                    }
                    .into(),
                )
            }
        }

        /// Gets the endpoints of the arc at the given `index`.
        pub(super) fn ends(topo_arcs: &[Value<'_>], transform: bool, index: i32) -> (Point, Point) {
            let arc = topo_arcs[arc_index(index)].as_array().unwrap();
            let p0 = to_point(&arc[0]);
            let p1 = if transform {
                let mut p1 = [HashF64(0.0), HashF64(0.0)];
                for dp in arc.iter().map(to_point) {
                    p1[0].0 += dp[0].0;
                    p1[1].0 += dp[1].0;
                }
                p1
            } else {
                to_point(arc.last().unwrap())
            };
            if index < 0 { (p1, p0) } else { (p0, p1) }
        }

        /// Creates the final reduced list of stitched arc indexes.
        pub(super) fn finalize<'s>(
            arc_indexes: Vec<i32>,
            mut starts: FragMap,
            mut ends: FragMap,
        ) -> Vec<Value<'s>> {
            let mut stitched_arcs = BTreeSet::new();
            let mut fragments = vec![];

            flush(&mut stitched_arcs, &mut fragments, &ends, &mut starts);
            flush(&mut stitched_arcs, &mut fragments, &starts, &mut ends);

            for index in arc_indexes {
                if !stitched_arcs.contains(&(arc_index(index))) {
                    fragments.push(Value::Array(vec![Value::Number(f64::from(index).into())]));
                }
            }

            fragments
        }

        /// Pushes a new line fragment to `out` containing the arcs accumulated
        /// in `commit`. The committed fragments are removed from `revert` and
        /// the array indexes for the committed arc indexes are recorded in
        /// `flushed`.
        pub(super) fn flush(
            flushed: &mut BTreeSet<usize>,
            out: &mut Vec<Value<'_>>,
            commit: &FragMap,
            revert: &mut FragMap,
        ) {
            for fragment in commit.values() {
                let fragment = fragment.borrow();
                revert.shift_remove(&fragment.start);

                for &index in &fragment.data {
                    flushed.insert(arc_index(index));
                }

                let fragment = fragment
                    .data
                    .iter()
                    .map(|v| Value::Number(f64::from(*v).into()))
                    .collect::<Vec<_>>();

                out.push(Value::Array(fragment));
            }
        }

        /// Inserts the given fragment `fg` into the two fragment maps.
        pub(super) fn insert(starts: &mut FragMap, ends: &mut FragMap, fg: RcFragment) {
            let (start, end) = {
                let fg = fg.borrow();
                (fg.start, fg.end)
            };
            starts.insert(start, Rc::clone(&fg));
            ends.insert(end, fg);
        }

        /// Converts an array value into an intermediate point.
        pub(super) fn to_point(value: &Value<'_>) -> Point {
            [
                HashF64(value.get(0).map(ValueExt::to_f64).unwrap()),
                HashF64(value.get(1).map(ValueExt::to_f64).unwrap()),
            ]
        }
    }

    /// A delta-encoded coordinate transformer.
    struct Transformer {
        /// The current x position.
        x0: f64,
        /// The current y position.
        y0: f64,
        /// The x-multiplier.
        kx: f64,
        /// The y-multiplier.
        ky: f64,
        /// The x-offset.
        dx: f64,
        /// The y-offset.
        dy: f64,
    }

    impl Transformer {
        /// Creates a new coordinate transformer using the given `transform`.
        fn new(transform: &Value<'_>) -> Self {
            fn coord(value: &Value<'_>) -> (f64, f64) {
                (
                    value.get(0).map_or(0.0, ValueExt::to_f64),
                    value.get(1).map_or(0.0, ValueExt::to_f64),
                )
            }

            let (kx, ky) = coord(transform.get("scale").unwrap());
            let (dx, dy) = coord(transform.get("translate").unwrap());
            Self {
                x0: 0.0,
                y0: 0.0,
                kx,
                ky,
                dx,
                dy,
            }
        }

        /// Converts the given `point` in place from a delta to an absolute
        /// coordinate.
        fn absolute(&mut self, point: &mut [Value<'_>], index: usize) {
            if index == 0 {
                self.x0 = 0.0;
                self.y0 = 0.0;
            }
            self.x0 += point[0].to_f64();
            self.y0 += point[1].to_f64();
            point[0] = (self.x0 * self.kx + self.dx).into();
            point[1] = (self.y0 * self.ky + self.dy).into();
        }
    }

    #[cfg(test)]
    mod tests {
        use serde_json_borrow::Value;

        #[test]
        fn feature() {
            const TOPO: &str = r#"{
                "type": "Topology",
                "transform": {
                    "scale": [0.045770440458280465, 0.04600460046004601],
                    "translate": [251.17068292882684, 20]
                },
                "objects": {
                    "clockwise": { "type": "Polygon", "arcs": [[0]] },
                    "counterclockwise": { "type": "Polygon", "arcs": [[0]] }
                },
                "arcs": [
                    [[0.0, 9999.0], [0.0, -9999.0], [9999.0, 0.0], [0.0, 9999.0], [-9999.0, 0.0]]
                ]
            }"#;

            const GEO: &str = r#"{
                "type": "Feature",
                "properties": {},
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [
                        [
                            [251.17068292882684, 480.00000000000006],
                            [251.17068292882684, 20.0],
                            [708.8293170711733, 20.0],
                            [708.8293170711733, 480.00000000000006],
                            [251.17068292882684, 480.00000000000006]
                        ]
                    ]
                }
            }"#;

            let topo = serde_json::from_str::<Value<'_>>(TOPO).unwrap();
            let expected = serde_json::from_str::<Value<'_>>(GEO).unwrap();

            let poly = topo
                .get("objects")
                .and_then(Value::as_object)
                .unwrap()
                .get("clockwise")
                .unwrap();
            let actual = super::features(&topo, poly);

            assert_eq!(actual, vec![expected]);
        }

        #[test]
        fn connected_mesh() {
            const TOPO: &str = r#"{
                "type": "Topology",
                "objects": {
                    "collection": {
                        "type": "GeometryCollection",
                        "geometries": [
                            {"type": "LineString", "arcs": [0]},
                            {"type": "LineString", "arcs": [1]}
                        ]
                    }
                },
                "arcs": [[[1.0, 0.0], [2.0, 0.0]], [[0.0, 0.0], [1.0, 0.0]]]
            }"#;

            const GEO: &str = r#"{
                "type": "MultiLineString",
                "coordinates": [[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]]]
            }"#;

            let topo = serde_json::from_str::<Value<'_>>(TOPO).unwrap();
            let expected = serde_json::from_str::<Value<'_>>(GEO).unwrap();

            let poly = topo
                .get("objects")
                .and_then(Value::as_object)
                .unwrap()
                .get("collection")
                .unwrap();
            let actual = super::mesh(&topo, poly);

            assert_eq!(actual, expected);
        }

        #[test]
        fn disconnected_mesh() {
            const TOPO: &str = r#"{
                "type": "Topology",
                "objects": {
                    "collection": {
                        "type": "GeometryCollection",
                        "geometries": [
                            {"type": "LineString", "arcs": [0]},
                            {"type": "LineString", "arcs": [1]}
                        ]
                    }
                },
                "arcs": [[[2.0, 0.0], [3.0, 0.0]], [[0.0, 0.0], [1.0, 0.0]]]
            }"#;

            const GEO: &str = r#"{
                "type": "MultiLineString",
                "coordinates": [[[2.0, 0.0], [3.0, 0.0]], [[0.0, 0.0], [1.0, 0.0]]]
            }"#;

            let topo = serde_json::from_str::<Value<'_>>(TOPO).unwrap();
            let expected = serde_json::from_str::<Value<'_>>(GEO).unwrap();

            let poly = topo
                .get("objects")
                .and_then(Value::as_object)
                .unwrap()
                .get("collection")
                .unwrap();
            let actual = super::mesh(&topo, poly);

            assert_eq!(actual, expected);
        }
    }
}
// SPDX-SnippetEnd

/// Interpolates points, in degrees, around a sphere with the given `radius`, in
/// degrees, moving in the given `direction`.
fn circle_iter(radius: f64, direction: Direction) -> impl Iterator<Item = Vec2> {
    const PRECISION: f64 = 6.0_f64.to_radians();
    let radius = radius.to_radians();
    let (sin_r, cos_r) = radius.sin_cos();
    let step = direction.signum() * PRECISION;
    let mut t = radius + direction.signum() * TAU;
    let to = radius - 0.5 * step;
    core::iter::from_fn(move || {
        if direction.forward() == (t < to) {
            None
        } else {
            let (sin_t, cos_t) = t.sin_cos();
            let point = Vec3::new(cos_r, -sin_r * cos_t, -sin_r * sin_t);
            t -= step;
            Some(Vec2::from_spherical(point).to_degrees())
        }
    })
    .fuse()
}

/// The same as [`f64::acos`], but clamped to the range `0..=PI`.
#[inline]
fn clamp_acos(x: f64) -> f64 {
    if x > 1.0 {
        0.0
    } else if x < -1.0 {
        PI
    } else {
        x.acos()
    }
}

/// The same as [`f64::asin`], but clamped to the range `-PI/2..=PI/2`.
#[inline]
fn clamp_asin(x: f64) -> f64 {
    if x > 1.0 {
        FRAC_PI_2
    } else if x < -1.0 {
        -FRAC_PI_2
    } else {
        x.asin()
    }
}
