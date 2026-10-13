//! The real sky seen from a point of the galaxy: the catalogue of the stars around the system
//! (positions in light years, spectral colour, apparent magnitude), plus the diffuse band of the
//! far disc whose density follows `Galaxy::density`. Pure and deterministic; the game caches the
//! result per sector (see `SkyCache`) so nothing is recomputed while the ship stays in one place.
use crate::galaxy::{Galaxy, SectorCoord, SECTOR_LY};
use crate::StarClass;
use vector_core::{hash_coord, Rng};

/// Light years per parsec.
pub const LY_PER_PC: f64 = 3.261_56;
/// Absolute magnitude of the Sun.
pub const SUN_ABS_MAG: f64 = 4.83;
/// Radius (ly) of the catalogue around the viewer: the stars further out are in the band.
pub const CATALOG_RADIUS_LY: f64 = 90.0;
/// Number of faint points kept for the galactic band.
pub const BAND_SAMPLES: usize = 4000;
/// Stars closer than this (ly) are the system's own star, drawn as a body.
const SELF_LY: f64 = 0.02;

/// One star of the sky: a direction (unit vector), an apparent magnitude and an sRGB colour.
/// For band points `mag` holds the relative intensity 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyStar {
    pub dir: [f64; 3],
    pub mag: f32,
    pub rgb: [u8; 3],
    /// Distance in ly (0 for the band points).
    pub dist_ly: f32,
}

/// Absolute magnitude from a luminosity in suns.
pub fn abs_mag(lum_sun: f64) -> f64 {
    SUN_ABS_MAG - 2.5 * lum_sun.max(1e-12).log10()
}

/// Apparent magnitude of a star of absolute magnitude `abs` seen from `dist_ly` light years:
/// `m = M + 5 log10(d / 10 pc)`. A star 100 times closer is 10 magnitudes brighter.
pub fn apparent_mag(abs: f64, dist_ly: f64) -> f64 {
    abs + 5.0 * (dist_ly.max(1e-6) / LY_PER_PC / 10.0).log10()
}

/// Colour of a spectral class (the usual blackbody tints).
pub fn class_rgb(c: StarClass) -> [u8; 3] {
    match c {
        StarClass::O => [155, 176, 255],
        StarClass::B => [170, 191, 255],
        StarClass::A => [202, 215, 255],
        StarClass::F => [248, 247, 255],
        StarClass::G => [255, 244, 234],
        StarClass::K => [255, 210, 161],
        StarClass::M => [255, 175, 100],
        StarClass::D => [215, 225, 255],
        StarClass::N => [160, 190, 255],
        _ => [255, 255, 255],
    }
}

/// Drawing brightness 0..1 of a magnitude against the limiting magnitude (linear in magnitude,
/// so logarithmic in flux, like the eye).
pub fn brightness(mag: f64, mag_limit: f64) -> f64 {
    ((mag_limit + 1.5 - mag) / (mag_limit + 3.0)).clamp(0.0, 1.0)
}

/// A figure joining bright stars: `lines` are pairs of indices into `Sky::stars`.
#[derive(Clone, Debug, PartialEq)]
pub struct Constellation {
    pub name: String,
    pub lines: Vec<(u16, u16)>,
    /// The star the name is written beside.
    pub label: u16,
}

/// A nebula or cluster of the medium, seen as a soft patch of angular radius `ang` (radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyNebula {
    pub dir: [f64; 3],
    pub ang: f32,
    pub rgb: [u8; 3],
}

#[derive(Clone, Debug, Default)]
pub struct Sky {
    /// Bright stars, brightest first.
    pub stars: Vec<SkyStar>,
    /// Faint points of the galactic band.
    pub band: Vec<SkyStar>,
    pub constellations: Vec<Constellation>,
    pub nebulae: Vec<SkyNebula>,
    /// Quasars (blue pinpricks): `mag` is the size in pixels.
    pub quasars: Vec<SkyStar>,
    /// Neighbouring galaxies as smudges: `mag` is the size in pixels.
    pub galaxies: Vec<SkyStar>,
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12);
    [v[0] / n, v[1] / n, v[2] / n]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Group the brightest stars into figures: seeds in order of brightness, each taking its nearest
/// unused neighbours within 24 degrees, joined by a minimal spanning tree.
pub fn constellations(stars: &[SkyStar], seed: u64) -> Vec<Constellation> {
    let n = stars.iter().take_while(|s| s.mag < 5.0).count().min(90);
    let mut used = vec![false; n];
    let mut out = Vec::new();
    let cos_max = 24f64.to_radians().cos();
    for i in 0..n {
        if used[i] || out.len() >= 14 {
            continue;
        }
        let mut near: Vec<(f64, usize)> = (0..n)
            .filter(|&j| j != i && !used[j] && dot(stars[i].dir, stars[j].dir) > cos_max)
            .map(|j| (-dot(stars[i].dir, stars[j].dir), j))
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut group = vec![i];
        group.extend(near.iter().take(6).map(|p| p.1));
        if group.len() < 3 {
            continue;
        }
        for &g in &group {
            used[g] = true;
        }
        // Prim over the angular distance.
        let mut inside = vec![group[0]];
        let mut lines = Vec::new();
        while inside.len() < group.len() {
            let mut best: Option<(f64, usize, usize)> = None;
            for &a in &inside {
                for &b in group.iter().filter(|b| !inside.contains(b)) {
                    let d = -dot(stars[a].dir, stars[b].dir);
                    if best.is_none_or(|x| d < x.0) {
                        best = Some((d, a, b));
                    }
                }
            }
            let Some((_, a, b)) = best else { break };
            lines.push((a as u16, b as u16));
            inside.push(b);
        }
        let name = crate::names::Spec::new("constellation").name(hash_coord(
            seed ^ 0xC057,
            i as i64,
            group.len() as i64,
            5,
        ));
        out.push(Constellation {
            name,
            lines,
            label: i as u16,
        });
    }
    out
}

/// Column density of the far disc along `dir` from `p` (sum of `Galaxy::density`).
fn column(p: [f64; 3], dir: [f64; 3]) -> f64 {
    let mut s = 0.0;
    let mut d = 200.0;
    while d < 40_000.0 {
        s += Galaxy::density([p[0] + dir[0] * d, p[1] + dir[1] * d, p[2] + dir[2] * d])
            * (d / 1000.0);
        d *= 1.35;
    }
    s
}

/// Build the sky seen from the middle of `sector`.
pub fn build(galaxy: &Galaxy, sector: SectorCoord) -> Sky {
    let p = [
        (sector.0 as f64 + 0.5) * SECTOR_LY,
        (sector.1 as f64 + 0.5) * SECTOR_LY,
        (sector.2 as f64 + 0.5) * SECTOR_LY,
    ];
    let mut stars: Vec<SkyStar> = galaxy
        .nearby(p, CATALOG_RADIUS_LY)
        .into_iter()
        .filter_map(|e| {
            let d = e.distance_to(p);
            if d < SELF_LY || matches!(e.class(), StarClass::X | StarClass::R) {
                return None;
            }
            let mag = apparent_mag(abs_mag(e.header.luminosity), d);
            if mag > 9.0 {
                return None;
            }
            let v = [
                (e.pos[0] - p[0]) / d,
                (e.pos[1] - p[1]) / d,
                (e.pos[2] - p[2]) / d,
            ];
            Some(SkyStar {
                dir: v,
                mag: mag as f32,
                rgb: class_rgb(e.class()),
                dist_ly: d as f32,
            })
        })
        .collect();
    stars.sort_by(|a, b| a.mag.total_cmp(&b.mag));
    // The band: directions kept with a probability proportional to the column density.
    let mut rng = Rng::new(hash_coord(
        galaxy.seed ^ 0xBA4D,
        sector.0,
        sector.1,
        sector.2,
    ));
    let cand: Vec<([f64; 3], f64)> = (0..BAND_SAMPLES * 3)
        .map(|_| {
            let z = rng.range(-1.0, 1.0);
            let a = rng.range(0.0, std::f64::consts::TAU);
            let r = (1.0 - z * z).sqrt();
            let v = [r * a.cos(), z, r * a.sin()];
            (v, column(p, v))
        })
        .collect();
    let cmax = cand.iter().map(|c| c.1).fold(1e-9, f64::max);
    let mut band = Vec::with_capacity(BAND_SAMPLES);
    for (v, c) in cand {
        let k = (c / cmax).sqrt();
        if rng.f64() < k * k && band.len() < BAND_SAMPLES {
            let warm = 0.5 + 0.5 * k;
            band.push(SkyStar {
                dir: v,
                mag: k as f32,
                rgb: [
                    (200.0 + 55.0 * warm) as u8,
                    (200.0 + 30.0 * warm) as u8,
                    (230.0 - 50.0 * warm) as u8,
                ],
                dist_ly: 0.0,
            });
        }
    }
    let constellations = constellations(
        &stars,
        galaxy.seed ^ hash_coord(0, sector.0, sector.1, sector.2),
    );
    let nebulae = galaxy
        .features_near(p, 600.0)
        .into_iter()
        .take(12)
        .filter_map(|f| {
            let v = [f.pos[0] - p[0], f.pos[1] - p[1], f.pos[2] - p[2]];
            let d = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if d < 1.0 {
                return None;
            }
            let c = feature_rgb(f.kind);
            Some(SkyNebula {
                dir: unit(v),
                ang: (f.radius / d).atan().min(1.2) as f32,
                rgb: c,
            })
        })
        .collect();
    let quasars = galaxy
        .quasars()
        .into_iter()
        .map(|q| SkyStar {
            dir: unit(q.dir),
            mag: 1.0 + (q.log_lum - 11.0) as f32,
            rgb: [190, 220, 255],
            dist_ly: 0.0,
        })
        .collect();
    let galaxies = crate::universe::Universe::new(galaxy.seed)
        .galaxies_near([0.0; 3], 4.0)
        .into_iter()
        .filter(|g| g.id != crate::universe::GalaxyId::HOME)
        .take(8)
        .map(|g| {
            let v = [
                g.pos[0] * 1.0e6 - crate::galaxy::R_SUN_LY - p[0],
                g.pos[1] * 1.0e6 - p[1],
                g.pos[2] * 1.0e6 - p[2],
            ];
            let d = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            SkyStar {
                dir: unit(v),
                mag: ((g.radius_kly * 2000.0 / d).atan().to_degrees() * 8.0 + 2.0).min(14.0) as f32,
                rgb: [230, 215, 190],
                dist_ly: d as f32,
            }
        })
        .collect();
    Sky {
        stars,
        band,
        constellations,
        nebulae,
        quasars,
        galaxies,
    }
}

fn feature_rgb(k: crate::cosmos::FeatureKind) -> [u8; 3] {
    use crate::cosmos::FeatureKind::*;
    match k {
        EmissionNebula => [255, 64, 80],
        ReflectionNebula => [96, 160, 255],
        PlanetaryNebula => [64, 255, 192],
        SupernovaRemnant => [255, 160, 48],
        OpenCluster => [160, 192, 255],
        GlobularCluster => [255, 224, 128],
        StarFormingRegion => [255, 112, 192],
    }
}

/// A sky kept until the viewer leaves its sector.
#[derive(Clone, Debug, Default)]
pub struct SkyCache {
    key: Option<(u64, SectorCoord)>,
    pub sky: Sky,
    /// How many times the sky was rebuilt.
    pub rebuilds: u32,
}

impl SkyCache {
    /// Refresh for a viewer at `pos`; returns true when the sky was rebuilt.
    pub fn update(&mut self, galaxy: &Galaxy, pos: [f64; 3]) -> bool {
        let key = (galaxy.seed, Galaxy::sector_of(pos));
        if self.key == Some(key) {
            return false;
        }
        self.sky = build(galaxy, key.1);
        self.key = Some(key);
        self.rebuilds += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hundred_times_closer_is_ten_magnitudes_brighter() {
        let m1 = apparent_mag(4.83, 100.0);
        let m2 = apparent_mag(4.83, 1.0);
        assert!((m1 - m2 - 10.0).abs() < 1e-9);
        assert!((apparent_mag(0.0, 100.0) - apparent_mag(0.0, 10.0) - 5.0).abs() < 1e-9);
        assert!((apparent_mag(abs_mag(1.0), 10.0 * LY_PER_PC) - 4.83).abs() < 1e-9);
    }

    #[test]
    fn colours_run_from_blue_to_red() {
        let o = class_rgb(StarClass::O);
        let m = class_rgb(StarClass::M);
        assert!(o[2] > o[0] && m[0] > m[2]);
    }

    #[test]
    fn the_catalogue_is_deterministic_sorted_and_cached() {
        let g = Galaxy::new(7);
        let a = build(&g, (0, 0, 0));
        let b = build(&g, (0, 0, 0));
        assert_eq!(a.stars, b.stars);
        assert_eq!(a.band, b.band);
        assert!(a.stars.len() > 20);
        assert!(a.stars.windows(2).all(|w| w[0].mag <= w[1].mag));
        assert!(a.stars.iter().all(|s| {
            let n = s.dir[0].powi(2) + s.dir[1].powi(2) + s.dir[2].powi(2);
            (n - 1.0).abs() < 1e-9
        }));
        let mut c = SkyCache::default();
        assert!(c.update(&g, [1.0, 2.0, 3.0]));
        assert!(!c.update(&g, [9.0, 9.0, 9.0]));
        assert_eq!(c.rebuilds, 1);
        assert!(c.update(&g, [15.0, 2.0, 3.0]));
        assert_eq!(c.rebuilds, 2);
    }

    #[test]
    fn the_band_follows_the_disc_and_the_sky_changes_after_a_jump() {
        let g = Galaxy::new(3);
        let home = build(&g, (0, 0, 0));
        let flat = home.band.iter().filter(|s| s.dir[1].abs() < 0.3).count();
        assert!(
            flat as f64 > 0.4 * home.band.len() as f64,
            "{flat}/{}",
            home.band.len()
        );
        let far = build(&g, (300, 5, -40));
        assert_ne!(home.stars.first(), far.stars.first());
    }

    #[test]
    fn no_panic_at_the_centre_the_edge_and_far_outside() {
        let g = Galaxy::new(1);
        for s in [(-2600, 0, 0), (3400, 0, 0), (0, 5000, 0), (900_000, 0, 0)] {
            assert!(build(&g, s).band.len() <= BAND_SAMPLES);
        }
    }

    #[test]
    fn constellations_join_bright_stars_and_are_named() {
        let g = Galaxy::new(7);
        let sky = build(&g, (0, 0, 0));
        assert!(!sky.constellations.is_empty());
        for c in &sky.constellations {
            assert!(!c.name.is_empty());
            assert!(c.lines.len() >= 2);
            assert!(c
                .lines
                .iter()
                .all(|l| usize::from(l.0.max(l.1)) < sky.stars.len()));
        }
        assert_eq!(sky.quasars.len(), 12);
        assert!(!sky.galaxies.is_empty());
        assert_eq!(build(&g, (0, 0, 0)).constellations, sky.constellations);
    }

    #[test]
    fn the_sky_budget_is_tolerant() {
        let g = Galaxy::new(5);
        let t = std::time::Instant::now();
        let _ = build(&g, (0, 0, 0));
        assert!(t.elapsed().as_secs_f64() < 10.0);
    }
}
