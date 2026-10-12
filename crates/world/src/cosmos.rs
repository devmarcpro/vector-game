//! What lies between the stars and beyond them: nebulae, clusters, star-forming regions and the
//! remnants of supernovae (the interstellar medium), the supermassive black hole at the centre
//! of the galaxy, quasars on the deep sky and the gravitational waves of merging compact
//! objects. Like everything else it is rebuilt from `hash(seed, coord)`; each object has an
//! effect on the places it covers (radiation, blinded sensors, interest for research).
use crate::galaxy::{Galaxy, R_SUN_LY};
use std::f64::consts::TAU;
use vector_core::{hash_coord, Rng};

/// Edge (ly) of the cells that hold at most one object of the medium.
pub const CELL_LY: f64 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FeatureKind {
    /// Gas lit by young stars: it glows red.
    EmissionNebula,
    /// Dust that scatters the light of a nearby star: it glows blue.
    ReflectionNebula,
    /// The shell a dying star has blown off, lit by the white dwarf it leaves.
    PlanetaryNebula,
    /// What a supernova threw out: hard radiation, a pulsar at the heart.
    SupernovaRemnant,
    /// A few hundred young stars born together.
    OpenCluster,
    /// A million old stars packed into a ball, in the halo and the bulge.
    GlobularCluster,
    /// A cloud collapsing into stars: dense, hot, along the spiral arms.
    StarFormingRegion,
}

impl FeatureKind {
    pub const ALL: [FeatureKind; 7] = [
        Self::EmissionNebula,
        Self::ReflectionNebula,
        Self::PlanetaryNebula,
        Self::SupernovaRemnant,
        Self::OpenCluster,
        Self::GlobularCluster,
        Self::StarFormingRegion,
    ];
    pub fn id(self) -> &'static str {
        [
            "emission_nebula",
            "reflection_nebula",
            "planetary_nebula",
            "supernova_remnant",
            "open_cluster",
            "globular_cluster",
            "star_forming_region",
        ][self as usize]
    }
    /// Range of the radius, ly.
    pub fn radius_range(self) -> (f64, f64) {
        match self {
            Self::EmissionNebula => (8.0, 30.0),
            Self::ReflectionNebula => (3.0, 12.0),
            Self::PlanetaryNebula => (1.0, 3.0),
            Self::SupernovaRemnant => (5.0, 25.0),
            Self::OpenCluster => (3.0, 8.0),
            Self::GlobularCluster => (12.0, 30.0),
            Self::StarFormingRegion => (10.0, 35.0),
        }
    }
    /// Dose rate at the heart, Gy/h (falls with the square of the distance to the heart).
    pub fn radiation(self) -> f64 {
        match self {
            Self::SupernovaRemnant => 30.0,
            Self::StarFormingRegion => 4.0,
            Self::PlanetaryNebula => 5.0,
            Self::EmissionNebula => 0.5,
            _ => 0.0,
        }
    }
    /// How much of the sensor range the medium eats at the heart, 0..1.
    pub fn opacity(self) -> f64 {
        match self {
            Self::EmissionNebula => 0.45,
            Self::ReflectionNebula => 0.30,
            Self::PlanetaryNebula => 0.20,
            Self::SupernovaRemnant => 0.50,
            Self::StarFormingRegion => 0.35,
            Self::OpenCluster | Self::GlobularCluster => 0.05,
        }
    }
    /// Interest for research at the heart (a multiplier of the lab output is `1 + interest`).
    pub fn interest(self) -> f64 {
        match self {
            Self::SupernovaRemnant => 0.8,
            Self::PlanetaryNebula => 0.7,
            Self::StarFormingRegion => 0.6,
            Self::EmissionNebula | Self::ReflectionNebula => 0.4,
            Self::OpenCluster => 0.3,
            Self::GlobularCluster => 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Feature {
    pub kind: FeatureKind,
    /// Centre, ly.
    pub pos: [f64; 3],
    /// Radius, ly.
    pub radius: f64,
}

impl Feature {
    /// How deep inside the object a point is: 0 outside, 1 at the heart.
    pub fn depth(&self, p: [f64; 3]) -> f64 {
        let d = ((p[0] - self.pos[0]).powi(2)
            + (p[1] - self.pos[1]).powi(2)
            + (p[2] - self.pos[2]).powi(2))
        .sqrt();
        (1.0 - d / self.radius).max(0.0)
    }
}

/// Combined effect of the medium at a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CosmicEffects {
    /// Gy/h.
    pub radiation: f64,
    /// Multiplier of the sensor range (1 = clear).
    pub sensor_mult: f64,
    /// Multiplier of the output of the labs is `1 + interest`.
    pub interest: f64,
}

impl Default for CosmicEffects {
    fn default() -> Self {
        CosmicEffects {
            radiation: 0.0,
            sensor_mult: 1.0,
            interest: 0.0,
        }
    }
}

/// The supermassive black hole at the centre of the galaxy (Sgr A* class).
pub const CENTRE: [f64; 3] = [-R_SUN_LY, 0.0, 0.0];
pub const CENTRE_MASS_SUN: f64 = 4.3e6;
/// Dose (Gy/h) 1000 ly from the centre, where the disc and the jets of the nucleus shine.
pub const CENTRE_DOSE_1000LY: f64 = 5.0;

/// A quasar of the deep sky: a supermassive black hole in the act of devouring a galaxy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quasar {
    /// Unit vector toward it.
    pub dir: [f64; 3],
    pub redshift: f64,
    /// Bolometric luminosity, 10^x suns.
    pub log_lum: f64,
}

/// Merger of two compact objects, heard as a ripple of space itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GwEvent {
    pub day: i64,
    pub pos: [f64; 3],
    pub m1: f64,
    pub m2: f64,
    /// A merger of two neutron stars also throws a kilonova.
    pub kilonova: bool,
}

impl GwEvent {
    pub fn chirp_mass(&self) -> f64 {
        (self.m1 * self.m2).powf(0.6) / (self.m1 + self.m2).powf(0.2)
    }
    /// Strain at a point (ly): 1e-21 for a 28 solar-mass chirp at 410 Mpc (1.34e9 ly) and
    /// falling as 1/d.
    pub fn strain_at(&self, p: [f64; 3]) -> f64 {
        let d = ((p[0] - self.pos[0]).powi(2)
            + (p[1] - self.pos[1]).powi(2)
            + (p[2] - self.pos[2]).powi(2))
        .sqrt()
        .max(1.0);
        1.0e-21 * (self.chirp_mass() / 28.0).powf(5.0 / 3.0) * 1.34e9 / d
    }
}

/// Mean number of mergers per day heard in the whole galaxy.
pub const GW_RATE_PER_DAY: f64 = 0.2;
/// The ship's gravimeters hear a strain above this.
pub const GW_THRESHOLD: f64 = 1.0e-16;

impl Galaxy {
    /// Objects of the medium whose centre lies in a 40 ly cell.
    pub fn features_in_cell(&self, cell: (i64, i64, i64)) -> Vec<Feature> {
        let origin = [
            cell.0 as f64 * CELL_LY,
            cell.1 as f64 * CELL_LY,
            cell.2 as f64 * CELL_LY,
        ];
        let c = [
            origin[0] + CELL_LY / 2.0,
            origin[1] + CELL_LY / 2.0,
            origin[2] + CELL_LY / 2.0,
        ];
        let density = Galaxy::density(c);
        let mut rng = Rng::new(hash_coord(self.seed ^ 0x15A_0001, cell.0, cell.1, cell.2));
        // Where there are stars there is gas and dust; the empty halo holds only the clusters.
        let present = (0.012 * (0.3 + density)).min(0.5);
        if rng.f64() >= present {
            return Vec::new();
        }
        let arm = Galaxy::arm_strength(c);
        let (x, z) = (c[0] + R_SUN_LY, c[2]);
        let r = (x * x + z * z).sqrt();
        let halo = c[1].abs() > 1000.0 || r < 8000.0;
        let w = [
            2.0 * arm,
            1.5 * arm,
            1.5,
            1.5,
            2.0,
            if halo { 0.6 } else { 0.05 },
            3.0 * arm * arm,
        ];
        let mut roll = rng.f64() * w.iter().sum::<f64>();
        let mut kind = FeatureKind::OpenCluster;
        for (k, wk) in FeatureKind::ALL.iter().zip(w) {
            if roll < wk {
                kind = *k;
                break;
            }
            roll -= wk;
        }
        let (r0, r1) = kind.radius_range();
        vec![Feature {
            kind,
            pos: [
                origin[0] + rng.range(0.2, 0.8) * CELL_LY,
                origin[1] + rng.range(0.2, 0.8) * CELL_LY,
                origin[2] + rng.range(0.2, 0.8) * CELL_LY,
            ],
            radius: rng.range(r0, r1),
        }]
    }

    /// Objects of the medium within `reach` ly of `p` (their edge counts), nearest first.
    pub fn features_near(&self, p: [f64; 3], reach: f64) -> Vec<Feature> {
        let n = ((reach + 35.0) / CELL_LY).ceil() as i64;
        let c = (
            (p[0] / CELL_LY).floor() as i64,
            (p[1] / CELL_LY).floor() as i64,
            (p[2] / CELL_LY).floor() as i64,
        );
        let mut v = Vec::new();
        for dx in -n..=n {
            for dy in -n..=n {
                for dz in -n..=n {
                    for f in self.features_in_cell((c.0 + dx, c.1 + dy, c.2 + dz)) {
                        let d = ((p[0] - f.pos[0]).powi(2)
                            + (p[1] - f.pos[1]).powi(2)
                            + (p[2] - f.pos[2]).powi(2))
                        .sqrt();
                        if d <= reach + f.radius {
                            v.push((d, f));
                        }
                    }
                }
            }
        }
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v.into_iter().map(|p| p.1).collect()
    }

    /// Distance from `p` to the supermassive black hole at the centre, ly.
    pub fn centre_distance(p: [f64; 3]) -> f64 {
        ((p[0] - CENTRE[0]).powi(2) + (p[1] - CENTRE[1]).powi(2) + (p[2] - CENTRE[2]).powi(2))
            .sqrt()
    }

    /// Radiation of the nucleus (Gy/h) at `p`: the disc and the jets of the central hole, a
    /// square law, felt only in the heart of the galaxy.
    pub fn centre_radiation(p: [f64; 3]) -> f64 {
        let d = Self::centre_distance(p).max(1.0);
        if d > 3000.0 {
            0.0
        } else {
            CENTRE_DOSE_1000LY * (1000.0 / d).powi(2)
        }
    }

    /// Everything the medium and the nucleus do at a point of space (the position of a system).
    pub fn cosmic_effects(&self, p: [f64; 3]) -> CosmicEffects {
        let mut e = CosmicEffects {
            radiation: Self::centre_radiation(p),
            ..Default::default()
        };
        for f in self.features_near(p, 0.0) {
            let depth = f.depth(p);
            if depth <= 0.0 {
                continue;
            }
            e.radiation += f.kind.radiation() * depth * depth;
            e.sensor_mult *= 1.0 - f.kind.opacity() * depth.sqrt();
            e.interest += f.kind.interest() * depth;
        }
        e.sensor_mult = e.sensor_mult.max(0.1);
        e
    }

    /// The deep sky: a dozen quasars at fixed directions with their redshifts.
    pub fn quasars(&self) -> Vec<Quasar> {
        (0..12)
            .map(|i| {
                let u = |k: i64| {
                    (hash_coord(self.seed ^ 0x0A5A_2000, i, k, 1) >> 11) as f64
                        / (1u64 << 53) as f64
                };
                let z = 2.0 * u(1) - 1.0;
                let phi = TAU * u(2);
                let r = (1.0 - z * z).sqrt();
                Quasar {
                    dir: [r * phi.cos(), z, r * phi.sin()],
                    redshift: 0.4 + 5.0 * u(3) * u(3),
                    log_lum: 11.0 + 3.0 * u(4),
                }
            })
            .collect()
    }

    /// The merger heard on day `day`, if any: a fresh draw each day, one day in five.
    pub fn gw_event(&self, day: i64) -> Option<GwEvent> {
        let mut rng = Rng::new(hash_coord(self.seed ^ 0x6A41_7700, day, 5, 5));
        if rng.f64() >= GW_RATE_PER_DAY {
            return None;
        }
        // Exponential disc around the centre, as thick as the disc.
        let r = -9000.0 * (1.0 - rng.f64()).ln();
        let th = rng.range(0.0, TAU);
        let pos = [
            CENTRE[0] + r * th.cos(),
            rng.range(-300.0, 300.0),
            CENTRE[2] + r * th.sin(),
        ];
        let kilonova = rng.f64() < 0.25;
        let (m1, m2) = if kilonova {
            (rng.range(1.2, 2.0), rng.range(1.2, 2.0))
        } else {
            (rng.range(5.0, 60.0), rng.range(5.0, 60.0))
        };
        Some(GwEvent {
            day,
            pos,
            m1,
            m2,
            kilonova,
        })
    }

    /// The mergers of days `from..=to` that the gravimeters of a ship at `p` hear, loudest first.
    pub fn gw_heard(&self, p: [f64; 3], from: i64, to: i64) -> Vec<GwEvent> {
        let mut v: Vec<GwEvent> = (from..=to)
            .filter_map(|d| self.gw_event(d))
            .filter(|e| e.strain_at(p) >= GW_THRESHOLD)
            .collect();
        v.sort_by(|a, b| b.strain_at(p).total_cmp(&a.strain_at(p)));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_of_object_of_the_medium_exists_in_its_place() {
        let g = Galaxy::new(5);
        let mut count = std::collections::BTreeMap::new();
        let mut halo_globular = 0;
        // Share of star-forming regions among the objects, along the arms and between them.
        let mut on_arm = (0u32, 0u32);
        let mut between = (0u32, 0u32);
        for x in -200..200 {
            for z in -200..200 {
                for f in g.features_in_cell((x, 0, z)) {
                    *count.entry(f.kind).or_insert(0u32) += 1;
                    let (lo, hi) = f.kind.radius_range();
                    assert!(f.radius >= lo && f.radius <= hi);
                    if f.kind == FeatureKind::GlobularCluster {
                        halo_globular += 1;
                    }
                    let arm = Galaxy::arm_strength(f.pos);
                    let slot = if arm > 1.3 {
                        &mut on_arm
                    } else if arm < 0.7 {
                        &mut between
                    } else {
                        continue;
                    };
                    slot.0 += u32::from(f.kind == FeatureKind::StarFormingRegion);
                    slot.1 += 1;
                }
            }
        }
        assert_eq!(count.len(), 7, "{count:?}");
        assert!(halo_globular < count.values().sum::<u32>() / 4);
        // Star-forming regions hug the spiral arms more than the rest.
        assert!(on_arm.1 > 50 && between.1 > 50, "{on_arm:?} {between:?}");
        assert!(
            f64::from(on_arm.0) / f64::from(on_arm.1)
                > 1.5 * f64::from(between.0) / f64::from(between.1),
            "{on_arm:?} {between:?}"
        );
    }

    #[test]
    fn the_medium_is_rebuilt_identically_and_depends_on_the_seed() {
        let a = Galaxy::new(1).features_near([0.0, 0.0, 0.0], 300.0);
        let b = Galaxy::new(1).features_near([0.0, 0.0, 0.0], 300.0);
        assert_eq!(a, b);
        assert_ne!(a, Galaxy::new(2).features_near([0.0, 0.0, 0.0], 300.0));
        assert!(!a.is_empty());
    }

    #[test]
    fn a_supernova_remnant_burns_and_a_nebula_blinds_only_inside() {
        let g = Galaxy::new(7);
        let snr = Feature {
            kind: FeatureKind::SupernovaRemnant,
            pos: [0.0; 3],
            radius: 10.0,
        };
        assert!((snr.depth([0.0; 3]) - 1.0).abs() < 1e-12 && snr.depth([11.0, 0.0, 0.0]) == 0.0);
        // Find a real remnant and compare inside and outside.
        let mut checked = false;
        for x in -80..80 {
            for z in -80..80 {
                for f in g.features_in_cell((x, 0, z)) {
                    if f.kind != FeatureKind::SupernovaRemnant {
                        continue;
                    }
                    let inside = g.cosmic_effects(f.pos);
                    let outside = g.cosmic_effects([f.pos[0] + 2.0 * f.radius, f.pos[1], f.pos[2]]);
                    assert!(
                        inside.radiation > 10.0
                            && inside.sensor_mult < 0.6
                            && inside.interest > 0.5
                    );
                    assert!(outside.radiation < inside.radiation * 0.2);
                    checked = true;
                }
            }
        }
        assert!(checked);
        // Empty interstellar space is clear.
        let e = g.cosmic_effects([0.0, 5000.0, 0.0]);
        assert_eq!(e, CosmicEffects::default());
    }

    #[test]
    fn the_nucleus_shines_in_its_neighbourhood_only() {
        assert!(Galaxy::centre_radiation([CENTRE[0] + 100.0, 0.0, 0.0]) > 100.0);
        assert_eq!(Galaxy::centre_radiation([0.0; 3]), 0.0);
        let a = Galaxy::centre_radiation([CENTRE[0] + 500.0, 0.0, 0.0]);
        let b = Galaxy::centre_radiation([CENTRE[0] + 1000.0, 0.0, 0.0]);
        assert!((a / b - 4.0).abs() < 1e-9, "inverse square");
        assert_eq!(Galaxy::centre_distance(CENTRE), 0.0);
    }

    #[test]
    fn quasars_are_far_and_fixed_and_waves_follow_the_chirp_mass() {
        let g = Galaxy::new(3);
        let q = g.quasars();
        assert_eq!(q.len(), 12);
        assert_eq!(q, Galaxy::new(3).quasars());
        for x in &q {
            assert!((x.dir.iter().map(|c| c * c).sum::<f64>() - 1.0).abs() < 1e-9);
            assert!(x.redshift >= 0.4 && x.log_lum >= 11.0);
        }
        let ev = (0..400).filter_map(|d| g.gw_event(d)).collect::<Vec<_>>();
        let rate = ev.len() as f64 / 400.0;
        assert!((0.1..0.3).contains(&rate), "about one day in five: {rate}");
        let e = ev[0];
        let near = e.strain_at([e.pos[0] + 100.0, e.pos[1], e.pos[2]]);
        let far = e.strain_at([e.pos[0] + 1000.0, e.pos[1], e.pos[2]]);
        assert!((near / far - 10.0).abs() < 1e-6, "strain falls as 1/d");
        let heavy = GwEvent {
            m1: 50.0,
            m2: 50.0,
            ..e
        };
        let light = GwEvent {
            m1: 1.4,
            m2: 1.4,
            ..e
        };
        assert!(heavy.strain_at(CENTRE) > light.strain_at(CENTRE) * 50.0);
        // A ship far from the merger hears nothing; one next to it does.
        assert!(g
            .gw_heard([e.pos[0] + 1.0e7, 0.0, 0.0], e.day, e.day)
            .is_empty());
        assert_eq!(
            g.gw_heard([e.pos[0] + 50.0, e.pos[1], e.pos[2]], e.day, e.day)
                .len(),
            1
        );
    }
}
