//! Over 9,000 map projection functions.

#![expect(
    clippy::float_cmp,
    reason = "good enough for D3, good enough for me for now"
)]

use super::{
    super::{EPSILON, renderer::Vec2},
    Centroid, ConicSettings, Direction, Listener, ModifiedStereographicCoefficient,
    Parallel1Settings, ProjectionSettings, Projector, ProjectorSettings, Rotate, Rotator,
    circle_iter, clamp_acos, clamp_asin,
};
use core::f64::consts::{FRAC_1_SQRT_2, FRAC_2_SQRT_PI, FRAC_PI_2, FRAC_PI_4, PI, SQRT_2, TAU};
use either::Either;

/// A cartographic projection.
#[derive(Clone, Debug)]
pub(super) enum Projection {
    /// Airy’s minimum-error azimuthal
    Airy(f64),
    /// Albers USA
    AlbersUsa {
        /// The Alaska projector.
        alaska: Box<Projector>,
        /// The Hawaii projector.
        hawaii: Box<Projector>,
        /// The CONUS projector.
        lower_48: Box<Projector>,
    },
    /// Armadillo
    Armadillo(ArmadilloParams),
    /// Basic projection
    Basic(fn(Vec2) -> Vec2),
    /// Berghaus Star
    Berghaus(BerghausParams),
    /// Bonne
    Bonne((f64, f64)),
    /// Bottomley
    Bottomley(f64),
    /// Chamberlin trimetric
    Chamberlin(ChamberlinParams, Rotate),
    /// Lambert conformal conic
    ConicConformal((f64, f64)),
    /// Conic equal area
    ConicEqualArea((f64, f64, f64)),
    /// Conic equidistant
    ConicEquidistant((f64, f64)),
    /// Craig retroazimuthal
    Craig(f64),
    /// Cylindrical equal-area, Gall–Peters, Hobo–Dyer, Tobler world-in-a-square
    CylindricalEqualArea(f64),
    /// Cylindrical stereographic, Gall’s stereographic
    CylindricalStereographic(f64),
    /// Gilbert
    ///
    /// In D3, this allowed to wrap another projection and then internally used
    /// an equirectangular projector with scale `1.0_f64.to_degrees()` and
    /// translate `(0, 0)` to transform streamed coordinates by creating an
    /// overridden listener chain `Proxy { point: |p, l| {
    ///     Vec2::new(p.x * 0.5, clamp_asin((-p.y * 0.5).to_radians().tan()))
    ///         .to_degrees()
    /// }}` → equirectangular → wrapped projection → output listener. Since the
    /// equirectangular projection is an identity projection this seems probably
    /// to just be a mechanism for doing degree-radian conversions? Since there
    /// is no way to communicate “wrap something else” in the Vega schema, this
    /// implementation just always uses the default orthographic projection as
    /// the “wrapped” projection, which then does not require any of this extra
    /// stuff. But I guess if you are here reading this because you want to
    /// unbundle this unreasonably nearly complete conversion of D3, you will
    /// want to fix this.
    Gilbert,
    /// Gingery
    Gingery(GingeryParams),
    /// Quincuncial where quincuncial = true
    QuincuncialTrue((f64, fn(Vec2) -> Vec2)),
    /// Quincuncial where quincuncial = false
    QuincuncialFalse((f64, fn(Vec2) -> Vec2)),
    /// Hammer, Eckert–Greifendorff, Briesemeister
    Hammer(f64),
    /// Hammer retroazimuthal
    HammerRetroazimuthal((f64, f64)),
    /// Hierarchical Equal Area isoLatitude Pixelisation of a 2-sphere
    Healpix(HealpixParams),
    /// Hill eucyclic, Maurer No. 73
    Hill(HillParams),
    /// Lagrange conformal
    Lagrange(f64),
    /// Loximuthal
    Loximuthal((f64, f64, f64)),
    /// Modified stereographic
    ModifiedStereographic(&'static [[f64; 2]]),
    /// Rectangular polyconic
    RectangularPolyconic((f64, f64)),
    /// Satellite (tilted perpsective)
    Satellite(SatelliteParams),
    /// Satellite (no perspective)
    SatelliteVertical(f64),
    /// Transverse Mercator
    TransverseMercator,
    /// Two-point azimuthal
    TwoPointAzimuthal(f64, Rotate),
    /// Two-point equidistant
    TwoPointEquidistant(Either<TwoPointEquidistantParams, fn(Vec2) -> Vec2>, Rotate),
}

impl Projection {
    /// Creates a new projection from the given projector settings.
    #[expect(clippy::too_many_lines, reason = "this is just a big switch")]
    pub fn new(s: &ProjectorSettings) -> Self {
        match s.projection {
            ProjectionSettings::Airy { radius } => Self::Airy(airy_params(radius)),
            ProjectionSettings::Albers { parallels } => Self::ConicEqualArea({
                conic_equal_area_params((parallels[0].to_radians(), parallels[1].to_radians()))
            }),
            ProjectionSettings::AlbersUsa => {
                let (lower_48, hawaii, alaska) = albers_usa_params(s);

                Self::AlbersUsa {
                    alaska: Box::new(alaska),
                    hawaii: Box::new(hawaii),
                    lower_48: Box::new(lower_48),
                }
            }
            ProjectionSettings::Armadillo { parallel } => {
                Self::Armadillo(armadillo_params(parallel))
            }
            ProjectionSettings::Berghaus { lobes } => Self::Berghaus(berghaus_params(lobes)),
            ProjectionSettings::Bonne { parallel } => {
                bonne_params(parallel.to_radians()).map_or(Self::Basic(sinusoidal), Self::Bonne)
            }
            ProjectionSettings::Bottomley { variant } => Self::Bottomley(variant.sin()),
            ProjectionSettings::Chamberlin { points } => {
                let params = chamberlin_params(points);
                Self::Chamberlin(params.0, params.1)
            }
            ProjectionSettings::ConicConformal(parallels) => {
                Self::ConicConformal(conic_conformal_params(parallels.to_radians()))
            }
            ProjectionSettings::ConicEqualArea(parallels) => {
                Self::ConicEqualArea(conic_equal_area_params(parallels.to_radians()))
            }
            ProjectionSettings::ConicEquidistant(parallels) => {
                conic_equidistant_params(parallels.to_radians())
                    .map_or(Self::Basic(equirectangular), Self::ConicEquidistant)
            }
            ProjectionSettings::Craig(parallel) => Self::Craig(parallel.to_radians().tan()),
            ProjectionSettings::CylindricalEqualArea(parallel) => {
                Self::CylindricalEqualArea(cylindrical_equal_area_params(parallel.to_radians()))
            }
            ProjectionSettings::CylindricalStereographic(parallel) => {
                Self::CylindricalStereographic(parallel.to_radians().cos())
            }
            ProjectionSettings::Gilbert => Self::Gilbert,
            ProjectionSettings::Gingery { lobes, radius } => {
                Self::Gingery(gingery_params(lobes, radius))
            }
            ProjectionSettings::Gringorten(super::QuincuncialSettings { quincuncial }) => {
                let params = (quincuncial_params(gringorten), gringorten as _);
                if quincuncial {
                    Self::QuincuncialTrue(params)
                } else {
                    Self::QuincuncialFalse(params)
                }
            }
            ProjectionSettings::PeirceQuincuncial { quincuncial }
            | ProjectionSettings::Guyou(super::QuincuncialSettings { quincuncial }) => {
                let params = (quincuncial_params(guyou), guyou as _);
                if quincuncial {
                    Self::QuincuncialTrue(params)
                } else {
                    Self::QuincuncialFalse(params)
                }
            }
            ProjectionSettings::Hammer { coefficient } => {
                if coefficient == 1.0 {
                    Self::Basic(azimuthal_equal_area)
                } else if coefficient == f64::INFINITY {
                    Self::Basic(hammer_quartic_authalic)
                } else {
                    Self::Hammer(coefficient)
                }
            }
            ProjectionSettings::HammerRetroazimuthal { parallel } => {
                Self::HammerRetroazimuthal(parallel.to_radians().sin_cos())
            }
            ProjectionSettings::Healpix { lobes } => Self::Healpix(healpix_params(lobes)),
            ProjectionSettings::Hill { ratio } => Self::Hill(hill_params(ratio)),
            ProjectionSettings::Lagrange { spacing } => Self::Lagrange(spacing),
            ProjectionSettings::Loximuthal { parallel } => {
                Self::Loximuthal(loximuthal_params(parallel))
            }
            ProjectionSettings::ModifiedStereographic { coefficients } => {
                Self::ModifiedStereographic(modified_stereographic_params(coefficients))
            }
            ProjectionSettings::RectangularPolyconic(parallel) => {
                Self::RectangularPolyconic(rectangular_polyconic_params(parallel))
            }
            ProjectionSettings::Satellite { distance, tilt } => {
                satellite_params(distance, tilt.to_radians())
                    .map_or(Self::SatelliteVertical(distance), Self::Satellite)
            }
            ProjectionSettings::TwoPointAzimuthal { points } => {
                let params = two_point_params(points);
                Self::TwoPointAzimuthal(params.0.cos(), params.1)
            }
            ProjectionSettings::TwoPointEquidistant { points } => {
                let params = two_point_params(points);
                let z0 = two_point_params(points).0;
                Self::TwoPointEquidistant(
                    if z0 == 0.0 {
                        Either::Right(azimuthal_equidistant)
                    } else {
                        Either::Left(two_point_equidistant_params(z0 * 2.0))
                    },
                    params.1,
                )
            }
            ProjectionSettings::Aitoff => Self::Basic(aitoff),
            ProjectionSettings::August => Self::Basic(august),
            ProjectionSettings::AzimuthalEqualArea => Self::Basic(azimuthal_equal_area),
            ProjectionSettings::AzimuthalEquidistant => Self::Basic(azimuthal_equidistant),
            ProjectionSettings::Baker => Self::Basic(baker),
            ProjectionSettings::Boggs => Self::Basic(boggs),
            ProjectionSettings::Bromley => Self::Basic(bromley),
            ProjectionSettings::Collignon => Self::Basic(collignon),
            ProjectionSettings::Craster => Self::Basic(craster),
            ProjectionSettings::Eckert1 => Self::Basic(eckert_1),
            ProjectionSettings::Eckert2 => Self::Basic(eckert_2),
            ProjectionSettings::Eckert3 => Self::Basic(eckert_3),
            ProjectionSettings::Eckert4 => Self::Basic(eckert_4),
            ProjectionSettings::Eckert5 => Self::Basic(eckert_5),
            ProjectionSettings::Eckert6 => Self::Basic(eckert_6),
            ProjectionSettings::Eisenlohr => Self::Basic(eisenlohr),
            ProjectionSettings::Equirectangular => Self::Basic(equirectangular),
            ProjectionSettings::Fahey => Self::Basic(fahey),
            ProjectionSettings::Foucaut => Self::Basic(foucaut),
            ProjectionSettings::Ginzburg4 => Self::Basic(ginzburg_4),
            ProjectionSettings::Ginzburg5 => Self::Basic(ginzburg_5),
            ProjectionSettings::Ginzburg6 => Self::Basic(ginzburg_6),
            ProjectionSettings::Ginzburg8 => Self::Basic(ginzburg_8),
            ProjectionSettings::Ginzburg9 => Self::Basic(ginzburg_9),
            ProjectionSettings::Gnomonic => Self::Basic(gnomonic),
            ProjectionSettings::Hatano => Self::Basic(hatano),
            ProjectionSettings::Homolosine => Self::Basic(homolosine),
            ProjectionSettings::Kavrayskiy7 => Self::Basic(kavrayskiy_7),
            ProjectionSettings::Larrivee => Self::Basic(larrivee),
            ProjectionSettings::Laskowski => Self::Basic(laskowski),
            ProjectionSettings::Littrow => Self::Basic(littrow),
            ProjectionSettings::Mercator => Self::Basic(mercator),
            ProjectionSettings::Miller => Self::Basic(miller),
            ProjectionSettings::Mollweide => Self::Basic(mollweide),
            ProjectionSettings::MtFlatPolarParabolic => Self::Basic(mt_flat_polar_parabolic),
            ProjectionSettings::MtFlatPolarQuartic => Self::Basic(mt_flat_polar_quartic),
            ProjectionSettings::MtFlatPolarSinusoidal => Self::Basic(mt_flat_polar_sinusoidal),
            ProjectionSettings::NaturalEarth => Self::Basic(natural_earth),
            ProjectionSettings::NellHammer => Self::Basic(nell_hammer),
            ProjectionSettings::Orthographic => Self::Basic(orthographic),
            ProjectionSettings::Patterson => Self::Basic(patterson),
            ProjectionSettings::Polyconic => Self::Basic(polyconic),
            ProjectionSettings::Robinson => Self::Basic(robinson),
            ProjectionSettings::SinuMollweide => Self::Basic(sinu_mollweide),
            ProjectionSettings::Sinusoidal => Self::Basic(sinusoidal),
            ProjectionSettings::Stereographic => Self::Basic(stereographic),
            ProjectionSettings::Times => Self::Basic(times),
            ProjectionSettings::TransverseMercator => Self::TransverseMercator,
            ProjectionSettings::VanDerGrinten => Self::Basic(van_der_grinten),
            ProjectionSettings::VanDerGrinten2 => Self::Basic(van_der_grinten_2),
            ProjectionSettings::VanDerGrinten3 => Self::Basic(van_der_grinten_3),
            ProjectionSettings::VanDerGrinten4 => Self::Basic(van_der_grinten_4),
            ProjectionSettings::Wagner4 => Self::Basic(wagner_4),
            ProjectionSettings::Wagner6 => Self::Basic(wagner_6),
            ProjectionSettings::Wagner7 => Self::Basic(wagner_7),
            ProjectionSettings::Wiechel => Self::Basic(wiechel),
            ProjectionSettings::Winkel3 => Self::Basic(winkel_3),
        }
    }

    /// Applies this projection to the given `point`, in radians.
    pub fn apply(&self, point: Vec2) -> Vec2 {
        match self {
            Self::Airy(params) => airy(point, *params),
            Self::AlbersUsa {
                alaska,
                hawaii,
                lower_48,
            } => albers_usa(point, alaska, hawaii, lower_48),
            Self::Armadillo(params) => armadillo(point, *params),
            Self::Basic(func) | Self::TwoPointEquidistant(Either::Right(func), _) => func(point),
            Self::Berghaus(params) => berghaus(point, *params),
            Self::Bonne(params) => bonne(point, *params),
            Self::Bottomley(params) => bottomley(point, *params),
            Self::Chamberlin(params, _) => chamberlin(point, *params),
            Self::ConicConformal(params) => conic_conformal(point, *params),
            Self::ConicEqualArea(params) => conic_equal_area(point, *params),
            Self::ConicEquidistant(params) => conic_equidistant(point, *params),
            Self::Craig(params) => craig(point, *params),
            Self::CylindricalEqualArea(params) => cylindrical_equal_area(point, *params),
            Self::CylindricalStereographic(params) => cylindrical_stereographic(point, *params),
            Self::Gilbert => gilbert(point),
            Self::Gingery(params) => gingery(point, *params),
            Self::Hammer(params) => hammer(point, *params),
            Self::HammerRetroazimuthal(params) => hammer_retroazimuthal(point, *params),
            Self::Healpix(params) => healpix(point, *params),
            Self::Hill(params) => hill(point, *params),
            Self::Lagrange(params) => lagrange(point, *params),
            Self::Loximuthal(params) => loximuthal(point, *params),
            Self::ModifiedStereographic(params) => modified_stereographic(point, params),
            Self::QuincuncialFalse((dx, func)) => quincuncial_false(point, func, *dx),
            Self::QuincuncialTrue((dx, func)) => quincuncial_true(point, func, *dx),
            Self::RectangularPolyconic(params) => rectangular_polyconic(point, *params),
            Self::Satellite(params) => satellite(point, *params),
            Self::SatelliteVertical(params) => satellite_vertical(point, *params),
            Self::TransverseMercator => transverse_mercator(point),
            Self::TwoPointAzimuthal(params, _) => two_point_azimuthal(point, *params),
            Self::TwoPointEquidistant(Either::Left(params), _) => {
                two_point_equidistant(point, *params)
            }
        }
    }

    /// Returns a listener proxy for projections that meddle with streams, or
    /// `None` if this projection is standard enough that it does not need to
    /// do that.
    pub fn proxy<'a, L>(&'a self, listener: &'a mut L) -> Option<ProjectionStream<'a, L>>
    where
        L: Listener + ?Sized,
    {
        match self {
            Self::Armadillo(..)
            | Self::Berghaus(..)
            | Self::Gingery(..)
            | Self::HammerRetroazimuthal(..)
            | Self::Healpix(..) => Some(ProjectionStream {
                listener,
                projection: self,
            }),
            _ => None,
        }
    }

    /// Returns a calculated centre for the projector.
    pub fn center(&self, [x, y]: [f64; 2], scale: f64, translate: [f64; 2]) -> Vec2 {
        let translate = Vec2::new(translate[0], translate[1]);
        let (x, y) = if matches!(self, Self::TransverseMercator) {
            (-y, x)
        } else {
            (x, y)
        };
        let center = self.apply(Vec2::new(x, y).to_radians()) * scale;
        Vec2::new(translate.x - center.x, translate.y + center.y)
    }

    /// Returns a calculated rotation value for the projector.
    pub fn rotate(&self, rotate: Rotate) -> Rotate {
        match self {
            Self::Chamberlin(_, rotate)
            | Self::TwoPointAzimuthal(_, rotate)
            | Self::TwoPointEquidistant(_, rotate) => *rotate,
            Self::TransverseMercator => match rotate {
                Rotate::OneD(y) => Rotate::ThreeD([y, 0.0, 90.0]),
                Rotate::TwoD([y, p]) => Rotate::ThreeD([y, p, 90.0]),
                Rotate::ThreeD([y, p, r]) => Rotate::ThreeD([y, p, r + 90.0]),
            },
            _ => rotate,
        }
    }
}

/// A stream proxy for custom projections.
pub(super) struct ProjectionStream<'a, L>
where
    L: Listener + ?Sized,
{
    /// The output listener.
    listener: &'a mut L,
    /// The projection.
    projection: &'a Projection,
}

impl<L> Listener for ProjectionStream<'_, L>
where
    L: Listener + ?Sized,
{
    fn line_end(&mut self) {
        self.listener.line_end();
    }

    fn line_start(&mut self) {
        self.listener.line_start();
    }

    fn point(&mut self, point: Vec2) {
        self.listener.point(point);
    }

    fn polygon_end(&mut self) {
        self.listener.polygon_end();
    }

    fn polygon_start(&mut self) {
        self.listener.polygon_start();
    }

    fn sphere(&mut self) {
        match self.projection {
            Projection::Armadillo(params) => armadillo_sphere(*params, self.listener),
            Projection::Berghaus(params) => berghaus_sphere(*params, self.listener),
            Projection::Gingery(params) => gingery_sphere(*params, self.listener),
            Projection::HammerRetroazimuthal(_) => hammer_retroazimuthal_sphere(self.listener),
            Projection::Healpix(params) => healpix_sphere(*params, self.listener),
            _ => self.listener.sphere(),
        }
    }
}

/// Berghaus epsilon.
const BERGHAUS_GINGERY_EPSILON: f64 = 1e-2;
/// Healpix parallel.
const HEALPIX_PARALLEL: f64 = 41.0 + 48.0 / 36.0 + 37.0 / 3600.0;
/// Healpix parallel, in radians.
const HEALPIX_PARALLEL_RADIANS: f64 = HEALPIX_PARALLEL.to_radians();
/// Sinu-Mollweide φ.
const SINU_MOLLWEIDE_PHI: f64 = 0.710_988_959_620_756_7;
/// Sinu-Mollweide Y.
const SINU_MOLLWEIDE_Y: f64 = 0.052_803_527_454_2;

/// Airy projection.
fn airy(point: Vec2, big_b: f64) -> Vec2 {
    let (sin_l, cos_l) = point.x.sin_cos();
    let (sin_p, cos_p) = point.y.sin_cos();
    let cos_z = cos_p * cos_l;
    let e = if 1.0 - cos_z == 0.0 {
        -0.5
    } else {
        (0.5 * (1.0 + cos_z)).ln() / (1.0 - cos_z)
    };
    let scale = -(e + big_b / (1.0 + cos_z));
    Vec2::new(cos_p * sin_l, sin_p) * scale
}

/// Airy projection parameter calculator.
fn airy_params(radius: f64) -> f64 {
    let beta = radius.to_radians();
    let tan_beta2 = (0.5 * beta).tan();
    2.0 * (0.5 * beta).cos().ln() / (tan_beta2 * tan_beta2)
}

/// Aitoff projection.
fn aitoff(point: Vec2) -> Vec2 {
    let (sin_l, cos_l) = (point.x / 2.0).sin_cos();
    let (sin_p, cos_p) = point.y.sin_cos();
    let sinci_alpha = sinci((cos_p * cos_l).acos());
    Vec2::new(2.0 * cos_p * sin_l * sinci_alpha, sin_p * sinci_alpha)
}

/// Albers USA projection.
fn albers_usa(point: Vec2, alaska: &Projector, hawaii: &Projector, lower_48: &Projector) -> Vec2 {
    // The AlbersUsa composite projector is a Rube Goldberg machine with zero
    // code comments explaining what it is doing. So here is what it is doing:
    // Three projectors are created (CONUS, Alaska, Hawaii), each one gets a
    // clip extent and a custom listener which receives only the `point` event
    // and, when it does, it sets the `point` variable through closure. When
    // an input is received, AlbersUsa nulls out the closed-over variable and
    // tries sending the input point to each internal projector until the
    // closed-over variable is indirectly set by one of them. This works because
    // if the point is outside the clip extent the inner projector will not emit
    // a `point` event. One could argue this is either very elegant, or very
    // cursed.
    //
    // In any case, because this implementation does not just send arbitrary
    // functions to be used as projectors, it is necessary to have the inner
    // projectors emit normalised values instead of acting like outputs or else
    // the outer projector will try to scale the values twice, etc.

    struct PointListener(Option<Vec2>);
    impl Listener for PointListener {
        fn line_end(&mut self) {}
        fn line_start(&mut self) {}
        fn point(&mut self, point: Vec2) {
            self.0 = Some(point);
        }
        fn polygon_end(&mut self) {}
        fn polygon_start(&mut self) {}
        fn sphere(&mut self) {}
    }

    lower_48
        .with_listener(PointListener(None), |listener| listener.point(point))
        .0
        .or_else(|| {
            alaska
                .with_listener(PointListener(None), |listener| listener.point(point))
                .0
        })
        .or_else(|| {
            hawaii
                .with_listener(PointListener(None), |listener| listener.point(point))
                .0
        })
        .map_or(Vec2::invalid(), |point| Vec2::new(point.x, -point.y))
}

/// Albers USA projection parameter calculator.
fn albers_usa_params(s: &ProjectorSettings) -> (Projector, Projector, Projector) {
    let projection = ProjectionSettings::Albers {
        parallels: ProjectionSettings::default_albers_parallels(),
    };
    let s = projection.defaults().merge(s);
    let k = s.scale.expect("the default scale should have applied");
    let [x, y] = s
        .translate
        .expect("the default translate should have applied");

    let lower_48 = Projector::new(&ProjectorSettings {
        clip_extent: Some([
            [x - 0.455 * k, y - 0.238 * k],
            [x + 0.455 * k, y + 0.238 * k],
        ]),
        precision: s.precision,
        projection,
        scale: Some(k),
        translate: s.translate,
        ..projection.defaults()
    });

    let projection = ProjectionSettings::ConicEqualArea(ConicSettings {
        parallels: [8.0, 18.0],
    });
    let hawaii = Projector::new(&ProjectorSettings {
        center: Some([-3.0, 19.9]),
        clip_extent: Some([
            [x - 0.214 * k + EPSILON, y + 0.166 * k + EPSILON],
            [x - 0.115 * k - EPSILON, y + 0.234 * k - EPSILON],
        ]),
        precision: s.precision,
        projection,
        rotate: Some(Rotate::TwoD([157.0, 0.0])),
        scale: Some(k),
        translate: Some([x - 0.205 * k, y + 0.212 * k]),
        ..projection.defaults()
    });

    let projection = ProjectionSettings::ConicEqualArea(ConicSettings {
        parallels: [55.0, 65.0],
    });
    let alaska = Projector::new(&ProjectorSettings {
        center: Some([-2.0, 58.5]),
        clip_extent: Some([
            [x - 0.425 * k + EPSILON, y + 0.120 * k + EPSILON],
            [x - 0.214 * k - EPSILON, y + 0.234 * k - EPSILON],
        ]),
        precision: s.precision,
        projection,
        rotate: Some(Rotate::TwoD([154.0, 0.0])),
        scale: Some(k * 0.35),
        translate: Some([x - 0.307 * k, y + 0.201 * k]),
        ..projection.defaults()
    });
    (lower_48, hawaii, alaska)
}

/// Precomputed values for an Armadillo projection.
#[derive(Clone, Copy, Debug)]
pub(super) struct ArmadilloParams {
    /// The parallel, in radians.
    phi0: f64,
    /// The sine of [`Self::phi0`].
    sin_phi0: f64,
    /// The cosine of [`Self::phi0`].
    cos_phi0: f64,
    /// The signum of [`Self::phi0`].
    s_phi0: f64,
    /// The tangent of [`Self::phi0`].
    tan_phi0: f64,
    /// Some modifier for phi.
    k: f64,
}

/// Armadillo projection.
fn armadillo(
    point: Vec2,
    ArmadilloParams {
        sin_phi0,
        cos_phi0,
        s_phi0,
        tan_phi0,
        k,
        ..
    }: ArmadilloParams,
) -> Vec2 {
    let (sin_l, cos_l) = (point.x / 2.0).sin_cos();
    let (sin_p, cos_p) = point.y.sin_cos();
    let d = if s_phi0 * point.y > -cos_l.atan2(tan_phi0) - 1e-3 {
        0.0
    } else {
        -s_phi0 * 10.0
    };
    Vec2::new(
        (1.0 + cos_p) * sin_l,
        d + k + sin_p * cos_phi0 - (1.0 + cos_p) * sin_phi0 * cos_l,
    )
}

/// Armadillo projection parameter calculator.
fn armadillo_params(parallel: f64) -> ArmadilloParams {
    let phi0 = parallel.to_radians();
    let (sin_phi0, cos_phi0) = phi0.sin_cos();
    let s_phi0 = nsignum(phi0);
    let tan_phi0 = (s_phi0 * phi0).tan();
    let k = (1.0 + sin_phi0 - cos_phi0) / 2.0;
    ArmadilloParams {
        phi0,
        sin_phi0,
        cos_phi0,
        s_phi0,
        tan_phi0,
        k,
    }
}

/// Armadillo projection sphere point generator.
fn armadillo_sphere<L>(
    ArmadilloParams {
        phi0,
        s_phi0,
        tan_phi0,
        ..
    }: ArmadilloParams,
    listener: &mut L,
) where
    L: Listener + ?Sized,
{
    let mut lambda = s_phi0 * -180.0;
    while lambda * s_phi0 < 180.0 {
        listener.point(Vec2::new(lambda, s_phi0 * 90.0));
        lambda += s_phi0 * 90.0;
    }
    loop {
        lambda -= phi0;
        if s_phi0 * lambda < -180.0 {
            break;
        }
        listener.point(Vec2::new(
            lambda,
            s_phi0
                * -(lambda.to_radians() / 2.0)
                    .cos()
                    .atan2(tan_phi0)
                    .to_degrees(),
        ));
    }
}

/// August projection.
fn august(point: Vec2) -> Vec2 {
    let tan_p = (point.y / 2.0).tan();
    let k = clamp_sqrt(1.0 - tan_p * tan_p);
    let (sin_l, cos_l) = (point.x / 2.0).sin_cos();
    let c = 1.0 + k * cos_l;
    let x = sin_l * k / c;
    let y = tan_p / c;
    let x2 = x * x;
    let y2 = y * y;
    Vec2::new(
        4.0 / 3.0 * x * (3.0 + x2 - 3.0 * y2),
        4.0 / 3.0 * y * (3.0 + 3.0 * x2 - y2),
    )
}

/// Azimuthal projection using generic scaling function.
fn azimuthal(scale: impl FnOnce(f64) -> f64, point: Vec2) -> Vec2 {
    let (sin_l, cos_l) = point.x.sin_cos();
    let (sin_p, cos_p) = point.y.sin_cos();
    let k = scale(cos_l * cos_p);
    Vec2::new(cos_p * sin_l, sin_p) * k
}

/// Azimuthal equal area projection.
fn azimuthal_equal_area(point: Vec2) -> Vec2 {
    azimuthal(|k| (2.0 / (1.0 + k)).sqrt(), point)
}

/// Azimuthal equidistant projection.
fn azimuthal_equidistant(point: Vec2) -> Vec2 {
    azimuthal(
        |k| {
            let c = k.acos();
            if c == 0.0 { 0.0 } else { c / c.sin() }
        },
        point,
    )
}

/// Baker projection.
fn baker(point: Vec2) -> Vec2 {
    let phi0 = point.y.abs();
    if phi0 < FRAC_PI_4 {
        Vec2::new(point.x, (FRAC_PI_4 + point.y / 2.0).tan().ln())
    } else {
        let (sin_phi0, cos_phi0) = phi0.sin_cos();
        Vec2::new(
            point.x * cos_phi0 * (2.0 * SQRT_2 - sin_phi0.recip()),
            zsignum(point.y) * (2.0 * SQRT_2 * (phi0 - FRAC_PI_4) - (phi0 / 2.0).tan().ln()),
        )
    }
}

/// Precomputed values for a Berghaus projection.
#[derive(Clone, Copy, Debug)]
pub(super) struct BerghausParams {
    /// The negative cosine of [`BERGHAUS_GINGERY_EPSILON`].
    cr: f64,
    /// ???
    k: f64,
    /// The number of lobes.
    n: f64,
    /// The sine of [`BERGHAUS_GINGERY_EPSILON`].
    sr: f64,
}

/// Berghaus projection.
fn berghaus(point: Vec2, BerghausParams { k, .. }: BerghausParams) -> Vec2 {
    let azimuthal = azimuthal_equidistant(point);
    if point.x.abs() > FRAC_PI_2 {
        // back hemisphere
        let angle = azimuthal.angle();
        let r = azimuthal.len();
        let angle0 = k * ((angle - FRAC_PI_2) / k).round() + FRAC_PI_2;
        // angle relative to lobe end
        let alpha = {
            let delta = angle - angle0;
            let (sin_d, cos_d) = delta.sin_cos();
            sin_d.atan2(2.0 - cos_d)
        };
        let (sin_t, cos_t) = (angle0 + clamp_asin(PI / r * alpha.sin()) - alpha).sin_cos();
        Vec2::new(cos_t, sin_t) * r
    } else {
        azimuthal
    }
}

/// Berghaus projection parameter calculator.
fn berghaus_params(n: f64) -> BerghausParams {
    let (cr, sr) = BERGHAUS_GINGERY_EPSILON.to_radians().sin_cos();
    let k = 2.0 * PI / n;
    BerghausParams { cr: -cr, k, n, sr }
}

/// Berghaus projection sphere point generator.
fn berghaus_sphere<L>(BerghausParams { n, cr, sr, .. }: BerghausParams, listener: &mut L)
where
    L: Listener + ?Sized,
{
    let delta = 360.0 / n;
    let delta0 = TAU / n;
    let mut phi = 90.0 - 180.0 / n;
    let mut phi0 = FRAC_PI_2;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "n should be a small integer, it is just easier to hold as a float because that is how it is used mostly"
    )]
    for _ in 0..(n as i32) {
        listener.point(Vec2::new(
            (sr * phi0.cos()).atan2(cr).to_degrees(),
            clamp_asin(sr * phi0.sin()).to_degrees(),
        ));
        if phi < -90.0 {
            listener.point(Vec2::new(-90.0, -180.0 - phi - BERGHAUS_GINGERY_EPSILON));
            listener.point(Vec2::new(-90.0, -180.0 - phi + BERGHAUS_GINGERY_EPSILON));
        } else {
            listener.point(Vec2::new(90.0, phi + BERGHAUS_GINGERY_EPSILON));
            listener.point(Vec2::new(90.0, phi - BERGHAUS_GINGERY_EPSILON));
        }
        phi -= delta;
        phi0 -= delta0;
    }
}

/// Boggs projection.
fn boggs(point: Vec2) -> Vec2 {
    const K: f64 = 2.00276;
    let (sin_t, cos_t) = mollweide_bromley_angle(PI, point.y).sin_cos();
    Vec2::new(
        K * point.x / (point.y.cos().recip() + 1.11072 / cos_t),
        (point.y + SQRT_2 * sin_t) / K,
    )
}

/// Bonne projection.
fn bonne(point: Vec2, (phi0, cot_phi0): (f64, f64)) -> Vec2 {
    let rho = cot_phi0 + phi0 - point.y;
    let big_e = if rho == 0.0 {
        rho
    } else {
        point.x * point.y.cos() / rho
    };
    let (sin_e, cos_e) = big_e.sin_cos();
    Vec2::new(rho * sin_e, cot_phi0 - rho * cos_e)
}

/// Bonne projection parameter calculator.
fn bonne_params(parallel: f64) -> Option<(f64, f64)> {
    (parallel != 0.0).then(|| (parallel, parallel.tan().recip()))
}

/// Bottomley projection.
fn bottomley(point: Vec2, sin_psi: f64) -> Vec2 {
    let rho = FRAC_PI_2 - point.y;
    let (sin_e, cos_e) = {
        let eta = if rho == 0.0 {
            rho
        } else {
            point.x * sin_psi * rho.sin() / rho
        };
        eta.sin_cos()
    };
    Vec2::new(rho * sin_e / sin_psi, FRAC_PI_2 - rho * cos_e)
}

/// Bromley projection.
fn bromley(point: Vec2) -> Vec2 {
    mollweide_bromley(point, 1.0, 4.0 / PI, PI)
}

/// Precomputed values for a Chamberlin projection.
#[derive(Clone, Copy, Debug)]
pub(super) struct ChamberlinParams {
    /// The precomputed points, in radians.
    points: [ChamberlinPoint; 3],
    /// Angle 1.
    beta1: f64,
    /// Angle 2.
    beta2: f64,
    /// The mean of points. (Sorry, I have no idea what these values represent.)
    mean: Vec2,
}

/// A point structure used by the Chamberlin projection.
#[derive(Clone, Copy, Debug)]
pub(super) struct ChamberlinPoint {
    /// Original control point.
    p0: Vec2,
    /// Sine and cosine of the control point.
    sc_phi: (f64, f64),
    /// The distance azimuth.
    v: Vec2,
    /// A calculated point.
    p1: Vec2,
}

/// Chamberlin projection.
fn chamberlin(
    point: Vec2,
    ChamberlinParams {
        points,
        beta1,
        beta2,
        mean,
    }: ChamberlinParams,
) -> Vec2 {
    fn norm_longitude(lambda: f64) -> f64 {
        lambda - TAU * ((lambda + PI) / TAU).floor()
    }

    let (sin_p, cos_p) = point.y.sin_cos();
    let mut v = [Vec2::zero(), Vec2::zero(), Vec2::zero()];

    for (control, new) in points.iter().zip(v.iter_mut()) {
        *new = chamberlin_distance_azimuth(
            point.y - control.p0.y,
            control.sc_phi.1,
            control.sc_phi.0,
            cos_p,
            sin_p,
            point.x - control.p0.x,
        );
        if new.x == 0.0 {
            return control.p1;
        }
        new.y = norm_longitude(new.y - control.v.y);
    }

    // Arithmetic mean of interception points
    let mut point = mean;
    for index in 0..v.len() {
        let next_index = (index + 1) % v.len();
        let angle =
            v[index].y.signum() * chamberlin_angle(points[index].v.x, v[index].x, v[next_index].x);
        let (d_lambda, d_phi) = if index == 0 {
            let (sin_a, cos_a) = angle.sin_cos();
            (cos_a, -sin_a)
        } else if index == 1 {
            let (sin_a, cos_a) = (beta1 - angle).sin_cos();
            (-cos_a, -sin_a)
        } else {
            let (sin_a, cos_a) = (beta2 - angle).sin_cos();
            (cos_a, sin_a)
        };
        point.x += v[index].x * d_lambda;
        point.y += v[index].x * d_phi;
    }

    point / 3.0
}

/// Angle opposite a, and contained between sides of lengths b and c.
fn chamberlin_angle(b: f64, c: f64, a: f64) -> f64 {
    clamp_acos(0.5 * (b * b + c * c - a * a) / (b * c))
}

/// Chamberlin distance azimuth calculation support function.
fn chamberlin_distance_azimuth(
    d_phi: f64,
    c1: f64,
    s1: f64,
    c2: f64,
    s2: f64,
    d_lambda: f64,
) -> Vec2 {
    let cosdλ = d_lambda.cos();
    let r = if d_phi.abs() > 1.0 || d_lambda.abs() > 1.0 {
        clamp_acos(s1 * s2 + c1 * c2 * cosdλ)
    } else {
        let sin_d_phi = (0.5 * d_phi).sin();
        let sin_d_lambda = (0.5 * d_lambda).sin();
        2.0 * clamp_asin((sin_d_phi * sin_d_phi + c1 * c2 * sin_d_lambda * sin_d_lambda).sqrt())
    };
    if r.abs() > EPSILON {
        Vec2::new(r, (c2 * d_lambda.sin()).atan2(c1 * s2 - s1 * c2 * cosdλ))
    } else {
        Vec2::zero()
    }
}

/// Chamberlin projection parameter calculator.
fn chamberlin_params(points: [[f64; 2]; 3]) -> (ChamberlinParams, Rotate) {
    let mut origin = Centroid::new();
    for [x, y] in points {
        origin.point(Vec2::new(x, y));
    }
    let origin = origin.finish();
    let rotate = Rotate::TwoD([-origin.x, -origin.y]);
    let rotator = Rotator::from(rotate);

    let mut points = points.map(|[x, y]| {
        let point = rotator.rotate(Vec2::new(x, y).to_radians());
        ChamberlinPoint {
            p0: point,
            sc_phi: point.y.sin_cos(),
            v: Vec2::zero(),
            p1: Vec2::zero(),
        }
    });

    let len = points.len();
    for index in 0..len {
        let [a, b] = points
            .get_disjoint_mut([(len + index - 1) % len, index])
            .unwrap();
        a.v = chamberlin_distance_azimuth(
            b.p0.y - a.p0.y,
            a.sc_phi.1,
            a.sc_phi.0,
            b.sc_phi.1,
            b.sc_phi.0,
            b.p0.x - a.p0.x,
        );
    }

    let beta0 = chamberlin_angle(points[0].v.x, points[2].v.x, points[1].v.x);
    let beta1 = chamberlin_angle(points[0].v.x, points[1].v.x, points[2].v.x);
    let beta2 = PI - beta0;

    points[2].p1.y = 0.0;
    points[1].p1.x = 0.5 * points[0].v.x;
    points[0].p1.x = -points[1].p1.x;

    let (sin_b, cos_b) = beta0.sin_cos();
    points[2].p1.x = points[0].p1.x + points[2].v.x * cos_b;
    points[1].p1.y = points[2].v.x * sin_b;
    points[0].p1.y = points[1].p1.y;

    let mean = Vec2::new(points[2].p1.x, 2.0 * points[0].p1.y);

    (
        ChamberlinParams {
            points,
            beta1,
            beta2,
            mean,
        },
        rotate,
    )
}

/// Collignon projection.
fn collignon(point: Vec2) -> Vec2 {
    let alpha = clamp_sqrt(1.0 - point.y.sin());
    Vec2::new(FRAC_2_SQRT_PI * point.x * alpha, PI.sqrt() * (1.0 - alpha))
}

/// Conic conformal projection.
fn conic_conformal(point: Vec2, (n, big_f): (f64, f64)) -> Vec2 {
    debug_assert_ne!(n, 0.0, "should have picked mercator");

    let phi = if big_f > 0.0 {
        point.y.max(-FRAC_PI_2 + EPSILON)
    } else {
        point.y.min(FRAC_PI_2 - EPSILON)
    };
    let rho = big_f / conic_conformal_t(phi).powf(n);
    let (sin_l, cos_l) = (point.x * n).sin_cos();
    Vec2::new(rho * sin_l, big_f - rho * cos_l)
}

/// Conic conformal projection parameter calculator.
fn conic_conformal_params((phi0, phi1): (f64, f64)) -> (f64, f64) {
    let cos_phi0 = phi0.cos();
    let n = if phi0 == phi1 {
        phi0.sin()
    } else {
        (cos_phi0 / phi1.cos()).ln() / (conic_conformal_t(phi1) / conic_conformal_t(phi0)).ln()
    };
    let big_f = cos_phi0 * conic_conformal_t(phi0).powf(n) / n;
    (n, big_f)
}

/// Calculates the tangent of `phi`.
fn conic_conformal_t(phi: f64) -> f64 {
    (FRAC_PI_4 + phi / 2.0).tan()
}

/// Conic equidistant projection.
fn conic_equidistant(point: Vec2, (n, big_g): (f64, f64)) -> Vec2 {
    debug_assert!(n.abs() >= EPSILON, "should have picked equirectangular");

    let rho = big_g - point.y;
    let (sin_l, cos_l) = (n * point.x).sin_cos();
    Vec2::new(rho * sin_l, big_g - rho * cos_l)
}

/// Conic equidistant projection parameter calculator.
fn conic_equidistant_params((phi0, phi1): (f64, f64)) -> Option<(f64, f64)> {
    let cos_phi0 = phi0.cos();
    let n = if phi0 == phi1 {
        phi0.sin()
    } else {
        (cos_phi0 - phi1.cos()) / (phi1 - phi0)
    };

    (n.abs() >= EPSILON).then(|| {
        let big_g = cos_phi0 / n + phi0;
        (n, big_g)
    })
}

/// Conic equal area projection.
fn conic_equal_area(point: Vec2, (n, big_c, rho0): (f64, f64, f64)) -> Vec2 {
    let (sin_l, cos_l) = (point.x * n).sin_cos();
    let sin_p = point.y.sin();
    let rho = (big_c - 2.0 * n * sin_p).sqrt() / n;
    Vec2::new(rho * sin_l, rho0 - rho * cos_l)
}

/// Conic equal area parameter calculator.
fn conic_equal_area_params((phi0, phi1): (f64, f64)) -> (f64, f64, f64) {
    let sin_phi0 = phi0.sin();
    let n = sin_phi0.midpoint(phi1.sin());
    let big_c = 1.0 + sin_phi0 * (2.0 * n - sin_phi0);
    let rho0 = big_c.sqrt() / n;
    (n, big_c, rho0)
}

/// Craig projection.
fn craig(point: Vec2, tan_phi0: f64) -> Vec2 {
    let (sin_l, cos_l) = point.x.sin_cos();
    let (sin_p, cos_p) = point.y.sin_cos();
    let lambda = if point.x == 0.0 { 1.0 } else { point.x / sin_l };
    Vec2::new(point.x, lambda * (sin_p * cos_l - tan_phi0 * cos_p))
}

/// Craster projection.
fn craster(point: Vec2) -> Vec2 {
    Vec2::new(
        point.x * (2.0 * (2.0 * point.y / 3.0).cos() - 1.0) / PI.sqrt(),
        PI.sqrt() * (point.y / 3.0).sin(),
    ) * 3.0_f64.sqrt()
}

/// Cylindrical equal area projection.
fn cylindrical_equal_area(point: Vec2, cos_phi0: f64) -> Vec2 {
    Vec2::new(point.x * cos_phi0, point.y.sin() / cos_phi0)
}

/// Cylindrical equal area parameter calculator.
fn cylindrical_equal_area_params(phi0: f64) -> f64 {
    phi0.cos()
}

/// Cylindrical stereographic projection.
fn cylindrical_stereographic(point: Vec2, cos_phi0: f64) -> Vec2 {
    Vec2::new(point.x * cos_phi0, (1.0 + cos_phi0) * (point.y * 0.5).tan())
}

/// Eckert I projection.
fn eckert_1(point: Vec2) -> Vec2 {
    let alpha = (8.0 / (3.0 * PI)).sqrt();
    Vec2::new(point.x * (1.0 - point.y.abs() / PI), point.y) * alpha
}

/// Eckert II projection.
fn eckert_2(point: Vec2) -> Vec2 {
    let alpha = (4.0 - 3.0 * point.y.abs().sin()).sqrt();
    Vec2::new(
        2.0 / (6.0 * PI).sqrt() * point.x * alpha,
        zsignum(point.y) * (TAU / 3.0).sqrt() * (2.0 - alpha),
    )
}

/// Eckert III projection.
fn eckert_3(point: Vec2) -> Vec2 {
    let k = (PI * (4.0 + PI)).sqrt();
    Vec2::new(
        2.0 / k * point.x * (1.0 + (1.0 - 4.0 * point.y * point.y / (PI * PI)).sqrt()),
        4.0 / k * point.y,
    )
}

/// Eckert IV projection.
fn eckert_4(point: Vec2) -> Vec2 {
    let k = (2.0 + FRAC_PI_2) * point.y.sin();
    let mut phi = point.y / 2.0;
    for _ in 0..10 {
        let cos_p = phi.cos();
        let delta = (phi + phi.sin() * (cos_p + 2.0) - k) / (2.0 * cos_p * (1.0 + cos_p));
        phi -= delta;
        if delta.abs() <= EPSILON {
            break;
        }
    }
    let (sin_p, cos_p) = phi.sin_cos();
    Vec2::new(
        2.0 / (PI * (4.0 + PI)).sqrt() * point.x * (1.0 + cos_p),
        2.0 * (PI / (4.0 + PI)).sqrt() * sin_p,
    )
}

/// Eckert V projection.
fn eckert_5(point: Vec2) -> Vec2 {
    Vec2::new(
        point.x * (1.0 + point.y.cos()) / (2.0 + PI).sqrt(),
        2.0 * point.y / (2.0 + PI).sqrt(),
    )
}

/// Eckert VI projection.
fn eckert_6(point: Vec2) -> Vec2 {
    let k = (1.0 + FRAC_PI_2) * point.y.sin();
    let mut phi = point.y;
    for _ in 0..10 {
        let delta = (phi + phi.sin() - k) / (1.0 + phi.cos());
        phi -= delta;
        if delta.abs() <= EPSILON {
            break;
        }
    }
    Vec2::new(point.x * (1.0 + phi.cos()), 2.0 * phi) / (2.0 + PI).sqrt()
}

/// Eisenlohr projection.
fn eisenlohr(point: Vec2) -> Vec2 {
    const K: f64 = 3.0 + 2.0 * SQRT_2;
    let (s0, c0) = (point.x / 2.0).sin_cos();
    let k = point.y.cos().sqrt();
    let (sin_p, c1) = (point.y / 2.0).sin_cos();
    let t = sin_p / (c1 + SQRT_2 * c0 * k);
    let c = (2.0 / (1.0 + t * t)).sqrt();
    let v = ((SQRT_2 * c1 + (c0 + s0) * k) / (SQRT_2 * c1 + (c0 - s0) * k)).sqrt();
    Vec2::new(
        c * (v - v.recip()) - 2.0 * v.ln(),
        c * t * (v + v.recip()) - 2.0 * t.atan(),
    ) * K
}

/// Calculate F(φ|m) where m = k² = sin²α.
/// See Abramowitz and Stegun, 17.6.7.
#[expect(
    clippy::many_single_char_names,
    reason = "blame mathematicians. or maths. or numbers"
)]
fn elliptic_f(mut phi: f64, m: f64) -> f64 {
    if m == 0.0 {
        return phi;
    }
    if m == 1.0 {
        return (phi / 2.0 + FRAC_PI_4).tan().ln();
    }
    let mut a = 1.0;
    let mut b = (1.0 - m).sqrt();
    let mut c = m.sqrt();
    let mut i = 0;
    while c.abs() > EPSILON {
        if phi % PI == 0.0 {
            phi += phi;
        } else {
            let mut delta_phi = (b * phi.tan() / a).atan();
            if delta_phi < 0.0 {
                delta_phi += PI;
            }
            phi += delta_phi + (phi / PI).trunc() * PI;
        }
        c = a.midpoint(b);
        b = (a * b).sqrt();
        a = c;
        c = (c - b) / 2.0;
        i += 1;
    }
    phi / (2.0_f64.powi(i) * a)
}

/// Calculate F(φ+iψ|m).
/// See Abramowitz and Stegun, 17.4.11.
#[expect(
    clippy::many_single_char_names,
    reason = "blame mathematicians. or maths. or numbers"
)]
fn elliptic_fi(Vec2 { x: phi, y: psi }: Vec2, m: f64) -> Vec2 {
    let r = phi.abs();
    let i = psi.abs();
    let sinh_psi = i.sinh();
    if r == 0.0 {
        Vec2::new(0.0, elliptic_f(sinh_psi.atan(), 1.0 - m) * zsignum(psi))
    } else {
        let csc_phi = r.sin().recip();
        let cot_phi2 = (r.tan() * r.tan()).recip();
        let b = -(cot_phi2 + m * (sinh_psi * sinh_psi * csc_phi * csc_phi) - 1.0 + m);
        let c = (m - 1.0) * cot_phi2;
        let cot_lambda2 = 0.5 * (-b + (b * b - 4.0 * c).sqrt());
        Vec2::new(
            elliptic_f((cot_lambda2.sqrt().recip()).atan(), m) * zsignum(phi),
            elliptic_f(
                clamp_sqrt((cot_lambda2 / cot_phi2 - 1.0) / m).atan(),
                1.0 - m,
            ) * zsignum(psi),
        )
    }
}

/// Equirectangular projection.
fn equirectangular(point: Vec2) -> Vec2 {
    point
}

/// Fahey projection.
fn fahey(point: Vec2) -> Vec2 {
    let fahey_k = (35.0_f64.to_radians()).cos();
    let t = (point.y / 2.0).tan();
    Vec2::new(
        point.x * fahey_k * clamp_sqrt(1.0 - t * t),
        (1.0 + fahey_k) * t,
    )
}

/// Foucaut projection.
fn foucaut(point: Vec2) -> Vec2 {
    let k = point.y / 2.0;
    let cosk = k.cos();
    Vec2::new(
        2.0 * point.x / PI.sqrt() * point.y.cos() * cosk * cosk,
        PI.sqrt() * k.tan(),
    )
}

/// Precomputed values for a Gingery projection.
#[derive(Clone, Copy, Debug)]
pub(super) struct GingeryParams {
    /// The number of lobes.
    n: f64,
    /// The sine of [`BERGHAUS_GINGERY_EPSILON`].
    sr: f64,
    /// The cosine of [`BERGHAUS_GINGERY_EPSILON`].
    cr: f64,
    /// The sine of [`Self::rho`].
    sin_rho: f64,
    /// The cosine of [`Self::rho`].
    cos_rho: f64,
    /// The radius, in radians.
    rho: f64,
    /// The square of [`Self::rho`].
    sq_rho: f64,
    /// ???
    k: f64,
}

/// Gingery projection.
#[expect(
    clippy::many_single_char_names,
    reason = "blame mathematicians. or maths. or numbers"
)]
fn gingery(point: Vec2, GingeryParams { rho, k, sq_rho, .. }: GingeryParams) -> Vec2 {
    fn arc_length(alpha: f64, k: f64, x: f64) -> f64 {
        let mut y = alpha * x.cos();
        if x < FRAC_PI_2 {
            y -= k;
        }
        (1.0 + y * y).sqrt()
    }

    // Numerical integration: trapezoidal rule.
    fn gingery_integrate(alpha: f64, k: f64, a: f64, b: f64) -> f64 {
        const N: i32 = 50;
        let h = (b - a) / f64::from(N);
        let mut s = arc_length(alpha, k, a) + arc_length(alpha, k, b);
        let mut x = a;
        for _ in 1..N {
            x += h;
            s += 2.0 * arc_length(alpha, k, x);
        }
        s * 0.5 * h
    }

    let point = azimuthal_equidistant(point);
    let r2 = point.square_len();

    if r2 > sq_rho {
        let r = r2.sqrt();
        let theta = point.angle();
        let theta0 = k * (theta / k).round();
        let alpha = theta - theta0;
        let rho_cos_a = rho * alpha.cos();
        let k = (rho * alpha.sin() - alpha * rho_cos_a.sin()) / (FRAC_PI_2 - rho_cos_a);
        let e = (PI - rho) / gingery_integrate(alpha, k, rho_cos_a, PI);

        let mut x = r;
        for _ in 0..50 {
            let delta = (rho + gingery_integrate(alpha, k, rho_cos_a, x) * e - r)
                / (arc_length(alpha, k, x) * e);
            x -= delta;
            if delta.abs() <= EPSILON {
                break;
            }
        }

        let mut y = alpha * x.sin();
        if x < FRAC_PI_2 {
            y -= k * (x - FRAC_PI_2);
        }

        let (sin_t, cos_t) = theta0.sin_cos();
        Vec2::new(x * cos_t - y * sin_t, x * sin_t + y * cos_t)
    } else {
        point
    }
}

/// Gingery projection parameter calculator.
fn gingery_params(n: f64, rho: f64) -> GingeryParams {
    let rho = rho.to_radians();
    let (sin_rho, cos_rho) = rho.sin_cos();
    let (cr, sr) = BERGHAUS_GINGERY_EPSILON.sin_cos();
    GingeryParams {
        n,
        sr,
        cr,
        sin_rho,
        cos_rho,
        rho,
        sq_rho: rho * rho,
        k: TAU / n,
    }
}

/// Gingery projection sphere point generator.
fn gingery_sphere<L>(
    GingeryParams {
        n,
        sr,
        cr,
        sin_rho,
        cos_rho,
        ..
    }: GingeryParams,
    listener: &mut L,
) where
    L: Listener + ?Sized,
{
    let delta = TAU / n;
    let mut phi = 0.0_f64;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "n should be a small integer, it is just easier to hold as a float because that is how it is used mostly"
    )]
    for _ in 0..(n as i32) {
        listener.point(Vec2::new((sr * phi.cos()).atan2(cr), (sr * phi.sin()).asin()).to_degrees());
        listener.point(
            Vec2::new(
                (sin_rho * (phi - delta / 2.0).cos()).atan2(cos_rho),
                (sin_rho * (phi - delta / 2.0).sin()).asin(),
            )
            .to_degrees(),
        );
        phi -= delta;
    }
}

/// Ginzburg IV projection.
fn ginzburg_4(point: Vec2) -> Vec2 {
    ginzburg_polyconic(
        (
            2.8284, -1.6988, 0.75432, -0.18071, 1.76003, -0.38914, 0.042_555, 0.0,
        ),
        point,
    )
}

/// Ginzburg V projection.
fn ginzburg_5(point: Vec2) -> Vec2 {
    ginzburg_polyconic(
        (
            2.583_819, -0.835_827, 0.170_354, -0.038_094, 1.543_313, -0.411_435, 0.082_742, 0.0,
        ),
        point,
    )
}

/// Ginzburg VI projection.
fn ginzburg_6(point: Vec2) -> Vec2 {
    ginzburg_polyconic(
        (
            5.0 / 6.0 * PI,
            -0.62636,
            -0.0344,
            0.0,
            1.3493,
            -0.05524,
            0.0,
            0.045,
        ),
        point,
    )
}

/// Ginzburg VIII projection.
fn ginzburg_8(point: Vec2) -> Vec2 {
    let sq_l = point.x * point.x;
    let sq_p = point.y * point.y;
    Vec2::new(
        point.x * (1.0 - 0.162_388 * sq_p) * (0.87 - 0.000_952_426 * sq_l * sq_l),
        point.y * (1.0 + sq_p / 12.0),
    )
}

/// Ginzburg IX projection.
fn ginzburg_9(point: Vec2) -> Vec2 {
    ginzburg_polyconic(
        (
            2.6516, -0.76534, 0.19123, -0.047_094, 1.36289, -0.13965, 0.031_762, 0.0,
        ),
        point,
    )
}

/// Ginzburg polyconic calculation.
#[expect(
    clippy::many_single_char_names,
    reason = "blame mathematicians. or maths. or numbers"
)]
fn ginzburg_polyconic(
    (a, b, c, d, e, f, g, h): (f64, f64, f64, f64, f64, f64, f64, f64),
    point: Vec2,
) -> Vec2 {
    if point.y == 0.0 {
        Vec2::new(a * point.x / PI, 0.0)
    } else {
        let phi_sq = point.y * point.y;
        let x_b = a + phi_sq * (b + phi_sq * (c + phi_sq * d));
        let y_b = point.y * (e - 1.0 + phi_sq * (f - h + phi_sq * g));
        let m = (x_b * x_b + y_b * y_b) / (2.0 * y_b);
        let alpha = point.x * (x_b / m).asin() / PI;
        let (sin_a, cos_a) = alpha.sin_cos();
        Vec2::new(m * sin_a, point.y * (1.0 + phi_sq * h) + m * (1.0 - cos_a))
    }
}

/// Gilbert projection.
fn gilbert(point: Vec2) -> Vec2 {
    // D3 creates a different projector for Gilbert which means this function
    // in D3 was running in degrees, but since this implementation uses the same
    // base projector for everything it is in radians
    orthographic(Vec2::new(point.x * 0.5, clamp_asin((point.y * 0.5).tan())))
}

/// Gnomonic projection.
fn gnomonic(point: Vec2) -> Vec2 {
    azimuthal(f64::recip, point)
}

/// Gringorten projection.
fn gringorten(point: Vec2) -> Vec2 {
    let signum_l = zsignum(point.x);
    let signum_p = zsignum(point.y);
    let (sin_l, cos_l) = point.x.sin_cos();
    let cos_p = point.y.cos();
    let x = cos_l * cos_p;
    let y = sin_l * cos_p;
    let z = (signum_p * point.y).sin();

    let mut lambda = y.atan2(z).abs();
    let phi = clamp_asin(x);

    if (lambda - FRAC_PI_2).abs() > EPSILON {
        lambda %= FRAC_PI_2;
    }

    let mut point = gringorten_hexadecant(Vec2::new(
        if lambda > FRAC_PI_4 {
            FRAC_PI_2 - lambda
        } else {
            lambda
        },
        phi,
    ));

    if lambda > FRAC_PI_4 {
        let z = point.x;
        point.x = -point.y;
        point.y = -z;
    }

    Vec2::new(point.x * signum_l, point.y * -signum_p)
}

/// Gringorten hexadecant calculation.
#[expect(
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "blame mathematicians. or maths. or numbers"
)]
fn gringorten_hexadecant(point: Vec2) -> Vec2 {
    if point.y == FRAC_PI_2 {
        return Vec2::zero();
    }

    let lambda = point.x;
    let phi = point.y;
    let sin_phi = phi.sin();
    let r = sin_phi * sin_phi;
    let r2 = r * r;
    let j = 1.0 + r2;
    let k = 1.0 + 3.0 * r2;
    let q = 1.0 - r2;
    let z = clamp_asin(j.sqrt().recip());
    let v = q + r * j * z;
    let p2 = (1.0 - sin_phi) / v;
    let p = p2.sqrt();
    let a2 = p2 * j;
    let a = a2.sqrt();
    let h = p * q;
    if lambda == 0.0 {
        return Vec2::new(0.0, -(h + r * a));
    }

    let cos_phi = phi.cos();
    let sec_phi = cos_phi.recip();
    let drd_phi = 2.0 * sin_phi * cos_phi;
    let dvd_phi = (-3.0 * r + z * k) * drd_phi;
    let dp2d_phi = (-v * cos_phi - (1.0 - sin_phi) * dvd_phi) / (v * v);
    let dpd_phi = (0.5 * dp2d_phi) / p;
    let dhd_phi = q * dpd_phi - 2.0 * r * p * drd_phi;
    let dra2d_phi = r * j * dp2d_phi + p2 * k * drd_phi;
    let mu = -sec_phi * drd_phi;
    let nu = -sec_phi * dra2d_phi;
    let zeta = -2.0 * sec_phi * dhd_phi;
    let lamda = 4.0 * lambda / PI;

    let x = if lambda > 0.222 * PI || phi < FRAC_PI_4 && lambda > 0.175 * PI {
        // Slower but accurate bisection method.
        let mut x = (h + r * clamp_sqrt(a2 * (1.0 + r2) - h * h)) / (1.0 + r2);
        if lambda > FRAC_PI_4 {
            return Vec2::new(x, x);
        }

        let mut x1 = x;
        let mut x0 = 0.5 * x;
        x = 0.5 * (x0 + x1);
        for _ in 0..50 {
            let g = (a2 - x * x).sqrt();
            let f = (x * (zeta + mu * g) + nu * clamp_asin(x / a)) - lamda;
            if f == 0.0 {
                break;
            }
            if f < 0.0 {
                x0 = x;
            } else {
                x1 = x;
            }
            x = 0.5 * (x0 + x1);
            if (x1 - x0).abs() <= EPSILON {
                break;
            }
        }
        x
    } else {
        // Newton-Raphson.
        let mut x = EPSILON;
        for _ in 0..25 {
            let sq_x = x * x;
            let g = clamp_sqrt(a2 - sq_x);
            let zeta_mu_g = zeta + mu * g;
            let f = x * zeta_mu_g + nu * clamp_asin(x / a) - lamda;
            let df = zeta_mu_g + (nu - mu * sq_x) / g;
            let delta = if g == 0.0 { 0.0 } else { f / df };
            x -= delta;
            if delta.abs() <= EPSILON {
                break;
            }
        }
        x
    };
    Vec2::new(x, -h - r * clamp_sqrt(a2 - x * x))
}

/// Guyou projection.
fn guyou(point: Vec2) -> Vec2 {
    let k_ = (SQRT_2 - 1.0) / (SQRT_2 + 1.0);
    let k = (1.0 - k_ * k_).sqrt();
    let big_k = elliptic_f(FRAC_PI_2, k * k);
    let f = -1.0;

    let psi = (FRAC_PI_4 + point.y.abs() / 2.0).tan().ln();
    let r = (f * psi).exp() / k_.sqrt();
    let (sin_l, cos_l) = (point.x * f).sin_cos();
    let at = guyou_complex_atan(Vec2::new(cos_l, sin_l) * r);
    let t = elliptic_fi(at, k * k);

    Vec2::new(-t.y, point.y.signum() * (0.5 * big_k - t.x))
}

/// Guyou support function.
fn guyou_complex_atan(Vec2 { x, y }: Vec2) -> Vec2 {
    let x_sq = x * x;
    let y_1 = y + 1.0;
    let t = 1.0 - x_sq - y * y;
    Vec2::new(
        0.5 * (x.signum() * FRAC_PI_2 - t.atan2(2.0 * x)),
        -0.25 * (t * t + 4.0 * x_sq).ln() + 0.5 * (y_1 * y_1 + x_sq).ln(),
    )
}

/// Hammer projection.
fn hammer(point: Vec2, big_b: f64) -> Vec2 {
    debug_assert!(
        big_b != 1.0 && big_b != f64::INFINITY,
        "should have picked something else"
    );

    let point = azimuthal_equal_area(Vec2::new(point.x / big_b, point.y));
    Vec2::new(point.x * big_b, point.y)
}

/// Hammer quartic authalic projection.
fn hammer_quartic_authalic(point: Vec2) -> Vec2 {
    let cos_p = point.y.cos();
    let (sin_half_p, cos_half_p) = (point.y / 2.0).sin_cos();
    Vec2::new(point.x * cos_p / cos_half_p, 2.0 * sin_half_p)
}

/// Hammer retroazimuthal projection.
fn hammer_retroazimuthal(point: Vec2, (sin_phi0, cos_phi0): (f64, f64)) -> Vec2 {
    let point = hammer_retroazimuthal_rotation(sin_phi0, cos_phi0, point);
    let (sin_l, cos_l) = point.x.sin_cos();
    let (sin_p, cos_p) = point.y.sin_cos();
    let z = (sin_phi0 * sin_p + cos_phi0 * cos_p * cos_l).acos();
    let sinz = z.sin();
    let k = if sinz.abs() > EPSILON { z / sinz } else { 1.0 };
    Vec2::new(
        k * cos_phi0 * sin_l,
        // rotate for back hemisphere
        if point.x.abs() > FRAC_PI_2 { k } else { -k }
            * (sin_phi0 * cos_p - cos_phi0 * sin_p * cos_l),
    )
}

/// Hammer retroazimuthal support function.
fn hammer_retroazimuthal_rotation(sin_phi0: f64, cos_phi0: f64, point: Vec2) -> Vec2 {
    let (sin_l, cos_l) = point.x.sin_cos();
    let (sin_p, cos_p) = point.y.sin_cos();

    let x = cos_l * cos_p;
    let y = sin_l * cos_p;
    let z = sin_p;
    Vec2::new(
        y.atan2(x * cos_phi0 - z * sin_phi0),
        clamp_asin(z * cos_phi0 + x * sin_phi0),
    )
}

/// Hammer retroazimuthal projection sphere point generator.
fn hammer_retroazimuthal_sphere<L>(listener: &mut L)
where
    L: Listener + ?Sized,
{
    const EPSILON: f64 = 1e-2;
    for point in circle_iter(90.0 - EPSILON, Direction::Forward) {
        listener.point(point);
    }
    listener.line_end();
    listener.line_start();
    for point in circle_iter(90.0 + EPSILON, Direction::Backward) {
        listener.point(point);
    }
}

/// Hatano projection.
fn hatano(point: Vec2) -> Vec2 {
    let mut phi = point.y;
    let c = phi.sin() * if phi < 0.0 { 2.43763 } else { 2.67595 };
    for _ in 0..20 {
        let delta = (phi + phi.sin() - c) / (1.0 + phi.cos());
        phi -= delta;
        if delta.abs() < EPSILON {
            break;
        }
    }
    phi *= 0.5;
    let (sin_p, cos_p) = phi.sin_cos();
    Vec2::new(
        0.85 * point.x * cos_p,
        sin_p * if phi < 0.0 { 1.93052 } else { 1.75859 },
    )
}

/// Precomputed values for a Healpix projection.
#[derive(Clone, Copy, Debug)]
pub(super) struct HealpixParams {
    /// The cosine of 0.0.
    cos_zero: f64,
    /// ???
    dx1: f64,
    /// ???
    dy1: f64,
    /// ???
    k: f64,
    /// The number of lobes.
    n: f64,
    /// ???
    y0: f64,
    /// ???
    y1: f64,
}

/// Hierarchical Equal Area isoLatitude Pixelisation of a 2-sphere projection.
fn healpix(
    point: Vec2,
    HealpixParams {
        cos_zero,
        n,
        dx1,
        y0,
        y1,
        dy1,
        k,
    }: HealpixParams,
) -> Vec2 {
    let dx0 = TAU;
    let abs_phi = point.y.abs();
    let point = if abs_phi > HEALPIX_PARALLEL_RADIANS {
        let i = ((point.x + PI) / k).floor().clamp(0.0, n - 1.0);
        let lambda = point.x + (PI * (n - 1.0) / n - i * k);
        let signum_phi = point.y.signum();
        let point = collignon(Vec2::new(lambda, abs_phi));
        Vec2::new(
            point.x * dx0 / dx1 - dx0 * (n - 1.0) / (2.0 * n) + i * dx0 / n,
            (y0 + (point.y - y1) * 4.0 * dy1 / dx0) * signum_phi,
        )
    } else {
        cylindrical_equal_area(point, cos_zero)
    };
    Vec2::new(point.x / 2.0, point.y)
}

/// Hierarchical Equal Area isoLatitude Pixelisation of a 2-sphere projection
/// parameter calculator.
fn healpix_params(n: f64) -> HealpixParams {
    let phi0 = HEALPIX_PARALLEL_RADIANS;
    let cos_phi0 = cylindrical_equal_area_params(0.0);
    let dx1 = collignon(Vec2::new(PI, phi0)).x - collignon(Vec2::new(-PI, phi0)).x;
    let y0 = cylindrical_equal_area(Vec2::new(0.0, phi0), cos_phi0).y;
    let y1 = collignon(Vec2::new(0.0, phi0)).y;
    let dy1 = collignon(Vec2::new(0.0, FRAC_PI_2)).y - y1;
    let k = TAU / n;
    HealpixParams {
        cos_zero: 0.0_f64.cos(),
        dx1,
        dy1,
        k,
        n,
        y0,
        y1,
    }
}

/// Healpix projection sphere point generator.
fn healpix_sphere<L>(HealpixParams { n, .. }: HealpixParams, listener: &mut L)
where
    L: Listener + ?Sized,
{
    let step = 180.0 / n;
    let mut x = -180.0;
    let mut even = true;
    while x < 180.0 + step / 2.0 {
        listener.point(Vec2::new(
            x,
            if even {
                HEALPIX_PARALLEL
            } else {
                90.0 - EPSILON
            },
        ));
        x += step;
        even = !even;
    }
    x = 180.0;
    even = true;
    while x > -180.0 - step / 2.0 {
        listener.point(Vec2::new(
            x,
            if even {
                -HEALPIX_PARALLEL
            } else {
                -90.9 + EPSILON
            },
        ));
        x -= step;
        even = !even;
    }
}

/// Precomputed values for a Hill projection.
#[derive(Clone, Copy, Debug)]
pub(super) struct HillParams {
    /// The clamped arcsine of [`Self::big_b`].
    beta: f64,
    /// ???
    big_a: f64,
    /// The sine of the reciprocal of [`Self::big_l`].
    big_b: f64,
    /// The ratio.
    big_k: f64,
    /// [`Self::big_k`] plus one.
    big_l: f64,
    /// ???
    rho0: f64,
    /// The square of [`Self::big_k`].
    sq_big_k: f64,
    /// The square of [`Self::big_l`].
    sq_big_l: f64,
}

/// Hill projection.
fn hill(
    point: Vec2,
    HillParams {
        big_k,
        big_b,
        big_a,
        big_l,
        beta,
        rho0,
        sq_big_k,
        sq_big_l,
    }: HillParams,
) -> Vec2 {
    let t = 1.0 - point.y.sin();
    let (rho, omega) = if t != 0.0 && t < 2.0 {
        let mut theta = FRAC_PI_2 - point.y;
        let mut beta_beta1 = 0.0;
        let mut big_c = 0.0;
        for _ in 0..25 {
            let (sin_t, cos_t) = theta.sin_cos();
            beta_beta1 = beta + sin_t.atan2(big_l - cos_t);
            big_c = 1.0 + sq_big_l - 2.0 * big_l * cos_t;
            let delta = (theta - sq_big_k * beta - big_l * sin_t + big_c * beta_beta1
                - 0.5 * t * big_b)
                / (2.0 * big_l * sin_t * beta_beta1);
            theta -= delta;
            if delta.abs() < EPSILON * EPSILON {
                break;
            }
        }
        (big_a * big_c.sqrt(), point.x * beta_beta1 / PI)
    } else {
        (big_a * (big_k + t), point.x * beta / PI)
    };

    let (sin_o, cos_o) = omega.sin_cos();
    Vec2::new(rho * sin_o, rho0 - rho * cos_o)
}

/// Hill projection parameter calculator.
fn hill_params(big_k: f64) -> HillParams {
    let big_l = 1.0 + big_k;
    let sin_b = big_l.recip().sin();
    let beta = clamp_asin(sin_b);
    let big_b = PI + 4.0 * beta * big_l;
    let big_a = 2.0 * (PI / big_b).sqrt();
    let rho0 = 0.5 * big_a * (big_l + (big_k * (2.0 + big_k)).sqrt());
    let sq_big_k = big_k * big_k;
    let sq_big_l = big_l * big_l;
    HillParams {
        beta,
        big_a,
        big_b,
        big_k,
        big_l,
        rho0,
        sq_big_k,
        sq_big_l,
    }
}

/// Homolosine projection.
fn homolosine(point: Vec2) -> Vec2 {
    if point.y.abs() > SINU_MOLLWEIDE_PHI {
        let point = mollweide(point);
        Vec2::new(point.x, point.y - point.y.signum() * SINU_MOLLWEIDE_Y)
    } else {
        sinusoidal(point)
    }
}

/// Gets the point, in degrees, and distance along the arc for the line
/// `(p0, p1)`, in degrees, at the normalised `t`.
fn interpolate_line(p0: Vec2, p1: Vec2, t: f64) -> (Vec2, f64) {
    #[inline]
    fn haversin(x: f64) -> f64 {
        let x = (x / 2.0).sin();
        x * x
    }

    let p0 = p0.to_radians();
    let p1 = p1.to_radians();
    let (sy0, cy0) = p0.y.sin_cos();
    let (sy1, cy1) = p1.y.sin_cos();
    let (sx0, cx0) = p0.x.sin_cos();
    let (sx1, cx1) = p1.x.sin_cos();
    let distance = 2.0
        * (haversin(p1.y - p0.y) + cy0 * cy1 * haversin(p1.x - p0.x))
            .sqrt()
            .asin();

    let origin = if distance == 0.0 {
        p0
    } else {
        let kx0 = cy0 * cx0;
        let ky0 = cy0 * sx0;
        let kx1 = cy1 * cx1;
        let ky1 = cy1 * sx1;
        let sin_d_recip = distance.sin().recip();
        let t = t * distance;
        let big_b = t.sin() * sin_d_recip;
        let big_a = (distance - t).sin() * sin_d_recip;
        let x = big_a * kx0 + big_b * kx1;
        let y = big_a * ky0 + big_b * ky1;
        let z = big_a * sy0 + big_b * sy1;
        Vec2::new(y.atan2(x), z.atan2((x * x + y * y).sqrt()))
    };

    (origin.to_degrees(), distance)
}

/// Kavrayskiy VII projection.
fn kavrayskiy_7(point: Vec2) -> Vec2 {
    Vec2::new(
        3.0 * point.x / TAU * (PI * PI / 3.0 - point.y * point.y).sqrt(),
        point.y,
    )
}

/// Lagrange projection.
fn lagrange(point: Vec2, big_n: f64) -> Vec2 {
    if (point.y.abs() - FRAC_PI_2).abs() < EPSILON {
        return Vec2::new(0.0, point.y.signum() * 2.0);
    }
    let sin_p = point.y.sin();
    let v = ((1.0 + sin_p) / (1.0 - sin_p)).powf(big_n / 2.0);
    let (sin_l, cos_l) = (point.x * big_n).sin_cos();
    let c = 0.5 * (v + v.recip()) + cos_l;
    Vec2::new(2.0 * sin_l / c, (v - v.recip()) / c)
}

/// Larrivee projection.
fn larrivee(point: Vec2) -> Vec2 {
    Vec2::new(
        point.x * (1.0 + point.y.cos().sqrt()) / 2.0,
        point.y / ((point.y / 2.0).cos() * (point.x / 6.0).cos()),
    )
}

/// Laskowski projection.
fn laskowski(point: Vec2) -> Vec2 {
    let sq_l = point.x * point.x;
    let sq_p = point.y * point.y;
    Vec2::new(
        point.x * (0.975_534 + sq_p * (-0.119_161 + sq_l * -0.014_305_9 + sq_p * -0.054_700_9)),
        point.y
            * (1.00384
                + sq_l * (0.080_289_4 + sq_p * -0.02855 + sq_l * 0.000_199_025)
                + sq_p * (0.099_890_9 + sq_p * -0.049_103_2)),
    )
}

/// Littrow projection.
fn littrow(point: Vec2) -> Vec2 {
    let (sin_l, cos_l) = point.x.sin_cos();
    Vec2::new(sin_l / point.y.cos(), point.y.tan() * cos_l)
}

/// Loximuthal projection.
fn loximuthal(point: Vec2, (phi0, cos_phi0, tan_phi0): (f64, f64, f64)) -> Vec2 {
    let y = point.y - phi0;
    let x = if y.abs() < EPSILON {
        point.x * cos_phi0
    } else {
        let x = FRAC_PI_4 + point.y / 2.0;
        if x.abs() < EPSILON || (x.abs() - FRAC_PI_2).abs() < EPSILON {
            0.0
        } else {
            point.x * y / (x.tan() / tan_phi0).ln()
        }
    };
    Vec2::new(x, y)
}

/// Loximuthal projection parameter calculator.
fn loximuthal_params(parallel: f64) -> (f64, f64, f64) {
    let parallel = parallel.to_radians();
    (parallel, parallel.cos(), (FRAC_PI_4 + parallel / 2.0).tan())
}

/// Mercator projection.
fn mercator(point: Vec2) -> Vec2 {
    Vec2::new(point.x, (FRAC_PI_4 + point.y / 2.0).tan().ln())
}

/// Miller projection.
fn miller(point: Vec2) -> Vec2 {
    Vec2::new(point.x, 1.25 * (FRAC_PI_4 + 0.4 * point.y).tan().ln())
}

/// Modified stereographic projection.
fn modified_stereographic(point: Vec2, coefficients: &[[f64; 2]]) -> Vec2 {
    let (sin_l, cos_l) = point.x.sin_cos();
    let (sin_p, cos_p) = point.y.sin_cos();
    let k = 2.0 / (1.0 + cos_p * cos_l);
    let zr = k * cos_p * sin_l;
    let zi = k * sin_p;
    let [mut ar, mut ai] = coefficients.last().copied().unwrap();
    for [wx, wy] in coefficients.iter().rev().skip(1) {
        (ar, ai) = (wx + zr * ar - zi * ai, wy + zr * ai + zi * ar);
    }
    Vec2::new(zr * ar - zi * ai, zr * ai + zi * ar)
}

/// Modified stereographic projection parameter calculator.
fn modified_stereographic_params(
    coefficients: ModifiedStereographicCoefficient,
) -> &'static [[f64; 2]] {
    coefficients.get()
}

/// Molleweide projection.
fn mollweide(point: Vec2) -> Vec2 {
    mollweide_bromley(point, SQRT_2 / FRAC_PI_2, SQRT_2, PI)
}

/// Generic Molleweide-Bromley projection function.
fn mollweide_bromley(point: Vec2, cx: f64, cy: f64, cp: f64) -> Vec2 {
    let (sin_p, cos_p) = mollweide_bromley_angle(cp, point.y).sin_cos();
    Vec2::new(cx * point.x * cos_p, cy * sin_p)
}

/// Molleweide-Bromley support function.
fn mollweide_bromley_angle(cp: f64, mut angle: f64) -> f64 {
    let cp_sin_t = cp * angle.sin();
    for _ in 0..30 {
        let (sin_t, cos_t) = angle.sin_cos();
        let delta = (angle + sin_t - cp_sin_t) / (1.0 + cos_t);
        angle -= delta;
        if delta.abs() <= EPSILON {
            break;
        }
    }
    angle / 2.0
}

/// McBryde-Thomas flat-polar parabolic projection.
fn mt_flat_polar_parabolic(point: Vec2) -> Vec2 {
    let sqrt6 = 6.0_f64.sqrt();
    let sqrt7 = 7.0_f64.sqrt();
    let theta = clamp_asin(7.0 * point.y.sin() / (3.0 * sqrt6));
    Vec2::new(
        sqrt6 * point.x * (2.0 * (2.0 * theta / 3.0).cos() - 1.0) / sqrt7,
        9.0 * (theta / 3.0).sin() / sqrt7,
    )
}

/// McBryde-Thomas flat-polar quartic projection.
fn mt_flat_polar_quartic(point: Vec2) -> Vec2 {
    let k = (1.0 + FRAC_1_SQRT_2) * point.y.sin();
    let mut theta = point.y;
    for _ in 0..25 {
        let delta =
            ((theta / 2.0).sin() + theta.sin() - k) / (0.5 * (theta / 2.0).cos() + theta.cos());
        theta -= delta;
        if delta.abs() < EPSILON {
            break;
        }
    }
    let (sin_half_t, cos_half_t) = (theta / 2.0).sin_cos();
    Vec2::new(
        point.x * (1.0 + 2.0 * theta.cos() / cos_half_t) / (3.0 * SQRT_2),
        2.0 * 3.0_f64.sqrt() * sin_half_t / (2.0 + SQRT_2).sqrt(),
    )
}

/// McBryde-Thomas flat-polar sinusoidal projection.
fn mt_flat_polar_sinusoidal(point: Vec2) -> Vec2 {
    let big_a = (6.0 / (4.0 + PI)).sqrt();
    let k = (1.0 + FRAC_PI_4) * point.y.sin();
    let mut theta = point.y / 2.0;
    for _ in 0..25 {
        let (sin_t, cos_t) = theta.sin_cos();
        let delta = (theta / 2.0 + sin_t - k) / (0.5 + cos_t);
        theta -= delta;
        if delta.abs() < EPSILON {
            break;
        }
    }
    Vec2::new((0.5 + theta.cos()) * point.x / 1.5, theta) * big_a
}

/// Natural earth projection.
fn natural_earth(point: Vec2) -> Vec2 {
    let sq_p = point.y * point.y;
    let quad_p = sq_p * sq_p;
    Vec2::new(
        point.x
            * (0.8707 - 0.131_979 * sq_p
                + quad_p * (-0.013_791 + quad_p * (0.003_971 * sq_p - 0.001_529 * quad_p))),
        point.y
            * (1.007_226
                + sq_p
                    * (0.015_085 + quad_p * (-0.044_475 + 0.028_874 * sq_p - 0.005_916 * quad_p))),
    )
}

/// Nell-Hammer projection.
fn nell_hammer(point: Vec2) -> Vec2 {
    Vec2::new(
        point.x * (1.0 + point.y.cos()) / 2.0,
        2.0 * (point.y - (point.y / 2.0).tan()),
    )
}

/// Orthographic projection.
fn orthographic(point: Vec2) -> Vec2 {
    azimuthal(|_| 1.0, point)
}

/// Patterson projection.
fn patterson(point: Vec2) -> Vec2 {
    const PATTERSON_K1: f64 = 1.0148;
    const PATTERSON_K2: f64 = 0.23185;
    const PATTERSON_K3: f64 = -0.14499;
    const PATTERSON_K4: f64 = 0.02406;
    let sq_p = point.y * point.y;
    Vec2::new(
        point.x,
        point.y
            * (PATTERSON_K1
                + sq_p * sq_p * (PATTERSON_K2 + sq_p * (PATTERSON_K3 + PATTERSON_K4 * sq_p))),
    )
}

/// Polyconic projection.
fn polyconic(point: Vec2) -> Vec2 {
    if point.y.abs() < EPSILON {
        Vec2::new(point.x, 0.0)
    } else {
        let tan_p = point.y.tan();
        let (sin_k, cos_k) = (point.x * point.y.sin()).sin_cos();
        Vec2::new(sin_k / tan_p, point.y + (1.0 - cos_k) / tan_p)
    }
}

/// Disabled quincuncial projection using the given `project_hemisphere`
/// projection.
fn quincuncial_false<F>(point: Vec2, project_hemisphere: F, dx: f64) -> Vec2
where
    F: Fn(Vec2) -> Vec2,
{
    let s = if point.x > 0.0 { -0.5 } else { 0.5 };
    let point = project_hemisphere(Vec2::new(point.x + s * PI, point.y));
    Vec2::new(point.x - s * dx, point.y)
}

/// Quincuncial projection parameter calculator.
fn quincuncial_params<F>(f: F) -> f64
where
    F: Fn(Vec2) -> Vec2,
{
    f(Vec2::new(FRAC_PI_2, 0.0)).x - f(Vec2::new(-FRAC_PI_2, 0.0)).x
}

/// Enabled quincuncial projection using the given `project_hemisphere`
/// projection.
fn quincuncial_true<F>(point: Vec2, project_hemisphere: F, dx: f64) -> Vec2
where
    F: Fn(Vec2) -> Vec2,
{
    let t = point.x.abs() < FRAC_PI_2;
    let point = project_hemisphere(Vec2::new(
        if t {
            point.x
        } else if point.x > 0.0 {
            point.x - PI
        } else {
            point.x + PI
        },
        point.y,
    ));

    let x = (point.x - point.y) * FRAC_1_SQRT_2;
    let y = (point.x + point.y) * FRAC_1_SQRT_2;

    if t {
        return Vec2::new(x, y);
    }

    let d = dx * FRAC_1_SQRT_2;
    let sign = if (x > 0.0) ^ (y > 0.0) { -1.0 } else { 1.0 };

    Vec2::new(sign * x - zsignum(y) * d, sign * y - zsignum(x) * d)
}

/// Rectangular polyconic projection.
fn rectangular_polyconic(point: Vec2, (phi0, sin_phi0): (f64, f64)) -> Vec2 {
    let big_a = if sin_phi0 == 0.0 {
        point.x / 2.0
    } else {
        (point.x * sin_phi0 / 2.0).tan() / sin_phi0
    };
    if point.y == 0.0 {
        Vec2::new(2.0 * big_a, -phi0)
    } else {
        let (sin_e, cos_e) = (2.0 * (big_a * point.y.sin()).atan()).sin_cos();
        let cot_p = point.y.tan().recip();
        Vec2::new(sin_e * cot_p, point.y + (1.0 - cos_e) * cot_p - phi0)
    }
}

/// Rectangular polyconic projection parameter calculator.
fn rectangular_polyconic_params(parallel: Parallel1Settings) -> (f64, f64) {
    let parallel = parallel.to_radians();
    (parallel, parallel.sin())
}

/// Robinson projection.
fn robinson(point: Vec2) -> Vec2 {
    const ROBINSON_CONSTANTS: &[[f64; 2]] = &[
        [0.9986, -0.062 * 1.0144],
        [1.0000, 0.0000 * 1.0144],
        [0.9986, 0.0620 * 1.0144],
        [0.9954, 0.1240 * 1.0144],
        [0.9900, 0.1860 * 1.0144],
        [0.9822, 0.2480 * 1.0144],
        [0.9730, 0.3100 * 1.0144],
        [0.9600, 0.3720 * 1.0144],
        [0.9427, 0.4340 * 1.0144],
        [0.9216, 0.4958 * 1.0144],
        [0.8962, 0.5571 * 1.0144],
        [0.8679, 0.6176 * 1.0144],
        [0.8350, 0.6769 * 1.0144],
        [0.7986, 0.7346 * 1.0144],
        [0.7597, 0.7903 * 1.0144],
        [0.7186, 0.8435 * 1.0144],
        [0.6732, 0.8936 * 1.0144],
        [0.6213, 0.9394 * 1.0144],
        [0.5722, 0.9761 * 1.0144],
        [0.5322, 1.0000 * 1.0144],
    ];
    #[expect(
        clippy::cast_precision_loss,
        reason = "value is small and from a const source"
    )]
    let i = (point.y.abs() * 36.0 / PI).min(ROBINSON_CONSTANTS.len() as f64 - 2.0);
    let i0 = i.floor();
    let di = i - i0;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is clamped to unsigned range and rounded before truncation"
    )]
    let i0 = i0 as usize;
    let [ax, ay] = ROBINSON_CONSTANTS[i0];
    let [bx, by] = ROBINSON_CONSTANTS[i0 + 1];
    let [cx, cy] = ROBINSON_CONSTANTS[(i0 + 2).min(ROBINSON_CONSTANTS.len() - 1)];
    Vec2::new(
        point.x * (bx + di * (cx - ax) / 2.0 + di * di * (cx - 2.0 * bx + ax) / 2.0),
        (point.y.signum() * FRAC_PI_2)
            * (by + di * (cy - ay) / 2.0 + di * di * (cy - 2.0 * by + ay) / 2.0),
    )
}

/// Precomputed values for a Satellite projection.
#[derive(Clone, Copy, Debug)]
pub(super) struct SatelliteParams {
    /// The distance.
    big_p: f64,
    /// The cosine of the tilt.
    cos_o: f64,
    /// The sine of the tilt.
    sin_o: f64,
}

/// Satellite projection.
fn satellite(
    point: Vec2,
    SatelliteParams {
        big_p,
        sin_o,
        cos_o,
    }: SatelliteParams,
) -> Vec2 {
    let point = satellite_vertical(point, big_p);
    let y = point.y;
    let big_a = y * sin_o / (big_p - 1.0) + cos_o;
    Vec2::new(point.x * cos_o, y) / big_a
}

/// Satellite projection parameter calculator.
fn satellite_params(big_p: f64, tilt: f64) -> Option<SatelliteParams> {
    (tilt != 0.0).then(|| {
        let (sin_o, cos_o) = tilt.sin_cos();
        SatelliteParams {
            big_p,
            cos_o,
            sin_o,
        }
    })
}

/// Vertical satellite projection.
fn satellite_vertical(point: Vec2, big_p: f64) -> Vec2 {
    let (sin_l, cos_l) = point.x.sin_cos();
    let (sin_p, cos_p) = point.y.sin_cos();
    let k = (big_p - 1.0) / (big_p - cos_p * cos_l);
    Vec2::new(cos_p * sin_l, sin_p) * k
}

/// Sinu-Mollweide projection.
fn sinu_mollweide(point: Vec2) -> Vec2 {
    if point.y > -SINU_MOLLWEIDE_PHI {
        let point = mollweide(point);
        Vec2::new(point.x, point.y + SINU_MOLLWEIDE_Y)
    } else {
        sinusoidal(point)
    }
}

/// Sinusoidal projection.
fn sinusoidal(point: Vec2) -> Vec2 {
    Vec2::new(point.x * point.y.cos(), point.y)
}

/// Stereographic projection.
fn stereographic(point: Vec2) -> Vec2 {
    azimuthal(|k| (1.0 + k).recip(), point)
}

/// Times projection.
fn times(point: Vec2) -> Vec2 {
    let t = (point.y / 2.0).tan();
    let s = (FRAC_PI_4 * t).sin();
    Vec2::new(point.x * (0.74482 - 0.34588 * s * s), 1.70711 * t)
}

/// Transverse Mercator projection.
fn transverse_mercator(point: Vec2) -> Vec2 {
    Vec2::new((FRAC_PI_4 + point.y / 2.0).tan().ln(), -point.x)
}

/// Two-point azimuthal projection.
fn two_point_azimuthal(point: Vec2, d_cos: f64) -> Vec2 {
    let point = gnomonic(point);
    Vec2::new(point.x * d_cos, point.y)
}

/// Precomputed values for a two-point equidistant projection.
#[derive(Clone, Copy, Debug)]
pub(super) struct TwoPointEquidistantParams {
    /// Half the negative half-distance.
    lambda_a: f64,
    /// Half the positive half-distance.
    lambda_b: f64,
    /// The square of [`Self::z0`].
    sq_z0: f64,
    /// The half-distance of the two points.
    z0: f64,
}

/// Two-point equidistant projection.
fn two_point_equidistant(
    point: Vec2,
    TwoPointEquidistantParams {
        z0,
        lambda_a,
        lambda_b,
        sq_z0,
    }: TwoPointEquidistantParams,
) -> Vec2 {
    debug_assert_ne!(z0, 0.0, "should have picked azimuthal_equidistant");

    let cos_p = point.y.cos();
    let za = clamp_acos(cos_p * (point.x - lambda_a).cos());
    let zb = clamp_acos(cos_p * (point.x - lambda_b).cos());
    let ys = point.y.signum();
    let za = za * za;
    let zb = zb * zb;
    Vec2::new(
        (za - zb) / (2.0 * z0),
        ys * clamp_sqrt(4.0 * sq_z0 * zb - (sq_z0 - za + zb) * (sq_z0 - za + zb)) / (2.0 * z0),
    )
}

/// Two-point equidistant projection parameter calculator.
fn two_point_equidistant_params(z0: f64) -> TwoPointEquidistantParams {
    let lambda_b = z0 / 2.0;
    let lambda_a = -lambda_b;
    let sq_z0 = z0 * z0;
    TwoPointEquidistantParams {
        lambda_a,
        lambda_b,
        sq_z0,
        z0,
    }
}

/// General two-point projection parameter calculator.
fn two_point_params(points: [[f64; 2]; 2]) -> (f64, Rotate) {
    let from = Vec2::new(points[0][0], points[0][1]);
    let to = Vec2::new(points[1][0], points[1][1]);

    let (origin, distance) = interpolate_line(from, to, 0.5);

    let r = Rotator::from(Rotate::TwoD([-origin.x, -origin.y]));
    let p = r.rotate(from.to_radians());

    let sin_d = distance.sin();
    let gamma = if sin_d == 0.0 {
        0.0
    } else {
        -clamp_asin(p.y.sin() / distance.sin())
    };
    let gamma = if p.x > 0.0 { PI - gamma } else { gamma };

    (
        distance * 0.5,
        Rotate::ThreeD([-origin.x, -origin.y, -gamma.to_degrees()]),
    )
}

/// Van der Grinten projection.
fn van_der_grinten(point: Vec2) -> Vec2 {
    if point.y.abs() < EPSILON {
        return Vec2::new(point.x, 0.0);
    }
    let sin_t = (point.y / FRAC_PI_2).abs();
    let theta = clamp_asin(sin_t);
    if point.x.abs() < EPSILON || (point.y.abs() - FRAC_PI_2).abs() < EPSILON {
        return Vec2::new(0.0, zsignum(point.y) * PI * (theta / 2.0).tan());
    }
    let cos_t = theta.cos();
    let big_a = (PI / point.x - point.x / PI).abs() / 2.0;
    let sq_big_a = big_a * big_a;
    let g = cos_t / (sin_t + cos_t - 1.0);
    let big_p = g * (2.0 / sin_t - 1.0);
    let sq_big_p = big_p * big_p;
    let p2_a2 = sq_big_p + sq_big_a;
    let g_p2 = g - sq_big_p;
    let big_q = sq_big_a + g;
    Vec2::new(
        zsignum(point.x)
            * PI
            * (big_a * g_p2 + (sq_big_a * g_p2 * g_p2 - p2_a2 * (g * g - sq_big_p)).sqrt())
            / p2_a2,
        zsignum(point.y)
            * PI
            * (big_p * big_q - big_a * ((sq_big_a + 1.0) * p2_a2 - big_q * big_q).sqrt())
            / p2_a2,
    )
}

/// Van der Grinten II projection.
fn van_der_grinten_2(point: Vec2) -> Vec2 {
    if point.y.abs() < EPSILON {
        return Vec2::new(point.x, 0.0);
    }
    let sin_t = (point.y / FRAC_PI_2).abs();
    let theta = clamp_asin(sin_t);
    if point.x.abs() < EPSILON || (point.y.abs() - FRAC_PI_2).abs() < EPSILON {
        return Vec2::new(0.0, zsignum(point.y) * PI * (theta / 2.0).tan());
    }

    let cos_t = theta.cos();
    let big_a = (PI / point.x - point.x / PI).abs() / 2.0;
    let sq_big_a = big_a * big_a;
    let x1 = cos_t * ((1.0 + sq_big_a).sqrt() - big_a * cos_t) / (1.0 + sq_big_a * sin_t * sin_t);
    Vec2::new(
        zsignum(point.x) * PI * x1,
        zsignum(point.y) * PI * clamp_sqrt(1.0 - x1 * (2.0 * big_a + x1)),
    )
}

/// Van der Grinten III projection.
fn van_der_grinten_3(point: Vec2) -> Vec2 {
    if point.y.abs() < EPSILON {
        return Vec2::new(point.x, 0.0);
    }
    let sin_t = point.y / FRAC_PI_2;
    let theta = clamp_asin(sin_t);
    if point.x.abs() < EPSILON || (point.y.abs() - FRAC_PI_2).abs() < EPSILON {
        return Vec2::new(0.0, PI * (theta / 2.0).tan());
    }
    let big_a = (PI / point.x - point.x / PI) / 2.0;
    let y1 = sin_t / (1.0 + theta.cos());
    Vec2::new(
        PI * (zsignum(point.x) * clamp_sqrt(big_a * big_a + 1.0 - y1 * y1) - big_a),
        PI * y1,
    )
}

/// Van der Grinten IV projection.
fn van_der_grinten_4(point: Vec2) -> Vec2 {
    if point.y == 0.0 {
        return Vec2::new(point.x, 0.0);
    }
    let phi0 = point.y.abs();
    if point.x == 0.0 || phi0 == FRAC_PI_2 {
        return Vec2::new(0.0, point.y);
    }
    let big_b = phi0 / FRAC_PI_2;
    let sq_big_b = big_b * big_b;
    let big_c =
        (8.0 * big_b - sq_big_b * (sq_big_b + 2.0) - 5.0) / (2.0 * sq_big_b * (big_b - 1.0));
    let sq_big_c = big_c * big_c;
    let big_b_c = big_b * big_c;
    let sq_big_b_c = sq_big_b + sq_big_c + 2.0 * big_b_c;
    let big_b_3_c = big_b + 3.0 * big_c;
    let lambda0 = point.x / FRAC_PI_2;
    let lambda1 = lambda0 + lambda0.recip();
    let big_d = zsignum(point.x.abs() - FRAC_PI_2) * (lambda1 * lambda1 - 4.0).sqrt();
    let sq_big_d = big_d * big_d;
    let big_f = sq_big_b_c * (sq_big_b + sq_big_c * sq_big_d - 1.0)
        + (1.0 - sq_big_b)
            * (sq_big_b * (big_b_3_c * big_b_3_c + 4.0 * sq_big_c)
                + 12.0 * big_b_c * sq_big_c
                + 4.0 * sq_big_c * sq_big_c);
    let x1 = (big_d * (sq_big_b_c + sq_big_c - 1.0) + 2.0 * clamp_sqrt(big_f))
        / (4.0 * sq_big_b_c + sq_big_d);
    Vec2::new(
        zsignum(point.x) * FRAC_PI_2 * x1,
        zsignum(point.y) * FRAC_PI_2 * clamp_sqrt(1.0 + big_d * x1.abs() - x1 * x1),
    )
}

/// Wagner IV projection.
fn wagner_4(point: Vec2) -> Vec2 {
    let big_a = 4.0 * PI + 3.0 * 3.0_f64.sqrt();
    let big_b = 2.0 * (2.0 * PI * 3.0_f64.sqrt() / big_a).sqrt();
    mollweide_bromley(point, big_b * 3.0_f64.sqrt() / PI, big_b, big_a / 6.0)
}

/// Wagner VI projection.
fn wagner_6(point: Vec2) -> Vec2 {
    Vec2::new(
        point.x * (1.0 - 3.0 * point.y * point.y / (PI * PI)).sqrt(),
        point.y,
    )
}

/// Wagner VII projection.
fn wagner_7(point: Vec2) -> Vec2 {
    let s = 0.90631 * point.y.sin();
    let c0 = (1.0 - s * s).sqrt();
    let (sin_l, cos_l) = (point.x / 3.0).sin_cos();
    let c1 = (2.0 / (1.0 + c0 * cos_l)).sqrt();
    Vec2::new(2.66723 * c0 * c1 * sin_l, 1.24104 * s * c1)
}

/// Wiechel projection.
fn wiechel(point: Vec2) -> Vec2 {
    let cos_p = point.y.cos();
    let sin_p = point.x.cos() * cos_p;
    let one_sin_p = 1.0 - sin_p;
    let (sin_l, cos_l) = (point.x.sin() * cos_p).atan2(-point.y.sin()).sin_cos();
    let cos_p = clamp_sqrt(1.0 - sin_p * sin_p);
    Vec2::new(
        sin_l * cos_p - cos_l * one_sin_p,
        -cos_l * cos_p - sin_l * one_sin_p,
    )
}

/// Winkel tripel projection.
fn winkel_3(point: Vec2) -> Vec2 {
    let aitoff = aitoff(point);
    Vec2::new(aitoff.x + point.x / FRAC_PI_2, aitoff.y + point.y) / 2.0
}

/// The same as [`f64::sqrt`], but returns zero for negative inputs
/// instead of `f64::NAN`.
#[inline]
fn clamp_sqrt(x: f64) -> f64 {
    if x > 0.0 { x.sqrt() } else { 0.0 }
}

/// Like [`f64::signum`], except 0.0 is given a negative sign.
#[inline]
fn nsignum(x: f64) -> f64 {
    if x > 0.0 { 1.0 } else { -1.0 }
}

/// Inverse sine, or 1.0 if `x` is zero.
#[inline]
fn sinci(x: f64) -> f64 {
    if x == 0.0 { 1.0 } else { x / x.sin() }
}

/// Like [`f64::signum`], except 0.0 is given a zero sign.
#[inline]
fn zsignum(x: f64) -> f64 {
    if x == 0.0 { 0.0 } else { x.signum() }
}
