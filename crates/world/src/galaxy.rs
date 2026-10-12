//! The galaxy: an infinite grid of 10-light-year sectors whose star density follows a spiral disc.
//! Nothing is stored: every sector is rebuilt from `hash(seed, coord)`.
use crate::{peek_star, StarClass, StarHeader};
use vector_core::{hash_coord, Rng};

/// Sector edge length in light years.
pub const SECTOR_LY: f64 = 10.0;
/// The Sun-like reference radius from the galactic centre (ly) and the disc scale length.
pub const R_SUN_LY: f64 = 26_000.0;
const SCALE_LEN: f64 = 9_000.0;
const SCALE_HEIGHT: f64 = 300.0;
const MAX_PER_SECTOR: usize = 40;
/// Edge of the cells holding one faction seed point.
pub const TERRITORY_CELL_LY: f64 = 60.0;
/// Beyond this radius from the centre the galaxy is empty.
const EDGE_LY: f64 = 60_000.0;

pub type SectorCoord = (i64, i64, i64);

/// Identifies one star: its sector and index within it.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct StarRef {
    pub sector: SectorCoord,
    pub index: u16,
}

#[derive(Clone, Debug)]
pub struct StarEntry {
    pub id: StarRef,
    /// Absolute position in light years (the home origin is the world origin).
    pub pos: [f64; 3],
    /// Seed for `generate_system`.
    pub seed: u64,
    pub header: StarHeader,
}

impl StarEntry {
    pub fn class(&self) -> StarClass {
        self.header.class
    }
    pub fn distance_to(&self, p: [f64; 3]) -> f64 {
        let d = [self.pos[0] - p[0], self.pos[1] - p[1], self.pos[2] - p[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Galaxy {
    pub seed: u64,
}

impl Galaxy {
    pub fn new(seed: u64) -> Galaxy {
        Galaxy { seed }
    }

    /// Name of this galaxy, from its seed.
    pub fn name(&self) -> String {
        crate::names::Spec::new("galaxy").name(self.seed)
    }

    /// Name of the 8x8x8-sector block (80 ly) a sector belongs to: the "secteur" of the maps.
    pub fn region_name(&self, coord: SectorCoord) -> String {
        let (x, y, z) = (
            coord.0.div_euclid(8),
            coord.1.div_euclid(8),
            coord.2.div_euclid(8),
        );
        crate::names::Spec::new("sector").name(hash_coord(self.seed, x, y, z ^ 0x5EC7))
    }

    /// Name of the nebula (or cluster) of the 8x8x8-sector block holding `coord`, if it has one
    /// (about one block in five).
    pub fn nebula_name(&self, coord: SectorCoord) -> Option<String> {
        let (x, y, z) = (
            coord.0.div_euclid(8),
            coord.1.div_euclid(8),
            coord.2.div_euclid(8),
        );
        let h = hash_coord(self.seed, x, y, z ^ 0x4E0B);
        (h.is_multiple_of(5)).then(|| {
            let kind = if h.is_multiple_of(2) {
                "nebula"
            } else {
                "cluster"
            };
            crate::names::Spec::new(kind).name(h >> 8)
        })
    }

    /// Mean number of stars in a sector centred at `c` (ly).
    pub fn density(c: [f64; 3]) -> f64 {
        // The galactic centre sits 26 000 ly to the -X side of the origin.
        let (x, y, z) = (c[0] + R_SUN_LY, c[1], c[2]);
        let r = (x * x + z * z).sqrt();
        if r > EDGE_LY {
            return 0.0;
        }
        let disc = 4.0 * (-(r - R_SUN_LY) / SCALE_LEN).exp() * (-y.abs() / SCALE_HEIGHT).exp();
        let bulge = 60.0 * (-r / 2_000.0).exp() * (-y.abs() / 1_000.0).exp();
        // Two logarithmic spiral arms modulate the disc.
        let theta = z.atan2(x);
        let arms = 1.0 + 0.7 * (2.0 * theta - 4.0 * (r.max(1.0) / 3_000.0).ln()).cos();
        ((disc * arms + bulge) * (1.0 - (r / EDGE_LY).powi(4))).clamp(0.0, MAX_PER_SECTOR as f64)
    }

    /// How much the spiral arms add at `c` (ly): 0.3 between the arms, 1.7 along them.
    pub fn arm_strength(c: [f64; 3]) -> f64 {
        let (x, z) = (c[0] + R_SUN_LY, c[2]);
        let r = (x * x + z * z).sqrt();
        let theta = z.atan2(x);
        1.0 + 0.7 * (2.0 * theta - 4.0 * (r.max(1.0) / 3_000.0).ln()).cos()
    }

    fn poisson(rng: &mut Rng, lambda: f64) -> usize {
        // Knuth for small lambda, normal approximation above.
        if lambda <= 0.0 {
            return 0;
        }
        if lambda < 12.0 {
            let l = (-lambda).exp();
            let (mut k, mut p) = (0usize, 1.0);
            loop {
                p *= rng.f64();
                if p <= l || k >= MAX_PER_SECTOR {
                    return k;
                }
                k += 1;
            }
        }
        let u1 = rng.f64().max(1e-12);
        let g = (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * rng.f64()).cos();
        ((lambda + g * lambda.sqrt()).round().max(0.0) as usize).min(MAX_PER_SECTOR)
    }

    /// Stars of a sector. Sector (0,0,0) always contains the home region around the origin.
    pub fn sector(&self, coord: SectorCoord) -> Vec<StarEntry> {
        let centre = [
            (coord.0 as f64 + 0.5) * SECTOR_LY,
            (coord.1 as f64 + 0.5) * SECTOR_LY,
            (coord.2 as f64 + 0.5) * SECTOR_LY,
        ];
        let mut rng = Rng::new(hash_coord(self.seed, coord.0, coord.1, coord.2));
        let lambda = Self::density(centre);
        // The home sector is never empty so every game has somewhere to start.
        let n = Self::poisson(&mut rng, lambda).max(usize::from(coord == (0, 0, 0)) * 3);
        (0..n)
            .map(|i| {
                let pos = [
                    (coord.0 as f64 + rng.f64()) * SECTOR_LY,
                    (coord.1 as f64 + rng.f64()) * SECTOR_LY,
                    (coord.2 as f64 + rng.f64()) * SECTOR_LY,
                ];
                let seed = hash_coord(
                    self.seed ^ 0x5747_4152,
                    coord.0 * 7919 + i as i64,
                    coord.1,
                    coord.2,
                );
                StarEntry {
                    id: StarRef {
                        sector: coord,
                        index: i as u16,
                    },
                    pos,
                    seed,
                    header: peek_star(seed),
                }
            })
            .collect()
    }

    pub fn star(&self, id: StarRef) -> Option<StarEntry> {
        self.sector(id.sector)
            .into_iter()
            .nth(usize::from(id.index))
    }

    /// Which faction (0..n) claims the point `p`, or None for unclaimed frontier space.
    /// Territories are the Voronoi cells of one jittered seed point per 60 ly cell.
    pub fn territory(&self, p: [f64; 3], n_factions: usize) -> Option<usize> {
        let cell = |v: f64| (v / TERRITORY_CELL_LY).floor() as i64;
        let c = (cell(p[0]), cell(p[1]), cell(p[2]));
        let mut best: Option<(f64, Option<usize>)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let (cx, cy, cz) = (c.0 + dx, c.1 + dy, c.2 + dz);
                    let h = hash_coord(self.seed ^ 0x7E44, cx, cy, cz);
                    let mut r = Rng::new(h);
                    let sp = [
                        (cx as f64 + r.f64()) * TERRITORY_CELL_LY,
                        (cy as f64 + r.f64()) * TERRITORY_CELL_LY,
                        (cz as f64 + r.f64()) * TERRITORY_CELL_LY,
                    ];
                    let d =
                        (sp[0] - p[0]).powi(2) + (sp[1] - p[1]).powi(2) + (sp[2] - p[2]).powi(2);
                    let slot = (r.next_u64() % (n_factions as u64 + 2)) as usize;
                    let owner = (slot < n_factions).then_some(slot);
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, owner));
                    }
                }
            }
        }
        best.and_then(|(_, o)| o)
    }

    pub fn sector_of(p: [f64; 3]) -> SectorCoord {
        (
            (p[0] / SECTOR_LY).floor() as i64,
            (p[1] / SECTOR_LY).floor() as i64,
            (p[2] / SECTOR_LY).floor() as i64,
        )
    }

    /// All stars within `radius` ly of `p`, nearest first.
    pub fn nearby(&self, p: [f64; 3], radius: f64) -> Vec<StarEntry> {
        let reach = (radius / SECTOR_LY).ceil() as i64;
        let c = Self::sector_of(p);
        let mut v = Vec::new();
        for dx in -reach..=reach {
            for dy in -reach..=reach {
                for dz in -reach..=reach {
                    for s in self.sector((c.0 + dx, c.1 + dy, c.2 + dz)) {
                        if s.distance_to(p) <= radius {
                            v.push(s);
                        }
                    }
                }
            }
        }
        v.sort_by(|a, b| a.distance_to(p).total_cmp(&b.distance_to(p)));
        v
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn regions_and_galaxies_have_stable_names() {
        use super::*;
        let g = Galaxy::new(5);
        assert_eq!(g.name(), Galaxy::new(5).name());
        assert_ne!(g.name(), Galaxy::new(6).name());
        // Every sector of an 8x8x8 block shares one region name; the next block differs.
        assert_eq!(g.region_name((0, 0, 0)), g.region_name((7, 7, 7)));
        assert_eq!(g.region_name((-1, 0, 0)), g.region_name((-8, 0, 0)));
        let names: std::collections::HashSet<String> =
            (0..200).map(|i| g.region_name((i * 8, 0, 0))).collect();
        assert!(names.len() > 180, "{}", names.len());
        let nebulae = (0..400)
            .filter(|&i| g.nebula_name((i * 8, 3, 0)).is_some())
            .count();
        assert!((40..130).contains(&nebulae), "{nebulae}");
    }
    use super::*;

    #[test]
    fn sectors_are_deterministic_and_inside_their_cell() {
        let g = Galaxy::new(42);
        for c in [(0, 0, 0), (5, -1, 3), (-100, 2, 40), (1234, 0, -77)] {
            let a = g.sector(c);
            let b = g.sector(c);
            assert_eq!(a.len(), b.len());
            for (x, y) in a.iter().zip(&b) {
                assert_eq!(x.seed, y.seed);
                assert_eq!(x.header.name, y.header.name);
                assert_eq!(Galaxy::sector_of(x.pos), c, "star must lie in its sector");
            }
        }
        assert_ne!(
            Galaxy::new(1).sector((0, 0, 0))[0].seed,
            Galaxy::new(2).sector((0, 0, 0))[0].seed
        );
    }

    #[test]
    fn solar_neighbourhood_density_is_realistic() {
        // About 4 stars per 1000 ly^3 near the Sun, i.e. a few per sector.
        let g = Galaxy::new(7);
        let mut total = 0usize;
        let mut cells = 0usize;
        for x in -6..6 {
            for z in -6..6 {
                total += g.sector((x, 0, z)).len();
                cells += 1;
            }
        }
        let mean = total as f64 / cells as f64;
        assert!((1.5..9.0).contains(&mean), "mean stars per sector {mean}");
    }

    #[test]
    fn density_falls_off_with_height_and_vanishes_at_the_edge() {
        let mid = Galaxy::density([0.0, 0.0, 0.0]);
        assert!(Galaxy::density([0.0, 3000.0, 0.0]) < mid * 0.01);
        assert_eq!(Galaxy::density([-26_000.0 + 70_000.0, 0.0, 0.0]), 0.0);
        let core = Galaxy::density([-25_500.0, 0.0, 0.0]);
        assert!(
            core > 10.0 * mid,
            "the bulge is much denser: {core} vs {mid}"
        );
    }

    #[test]
    fn territories_are_stable_contiguous_and_varied() {
        let g = Galaxy::new(9);
        let mut owners = std::collections::BTreeSet::new();
        let mut unclaimed = 0;
        for i in 0..400 {
            let p = [f64::from(i) * 7.0 - 1400.0, 0.0, f64::from(i % 20) * 31.0];
            let o = g.territory(p, 8);
            assert_eq!(o, g.territory(p, 8));
            match o {
                Some(f) => {
                    assert!(f < 8);
                    owners.insert(f);
                }
                None => unclaimed += 1,
            }
        }
        assert!(
            owners.len() >= 5,
            "several factions should be met: {owners:?}"
        );
        assert!(unclaimed > 0, "some frontier space remains");
        // Two points 1 ly apart almost always share an owner (territories are tens of ly wide).
        let same = (0..200)
            .filter(|&i| {
                let p = [f64::from(i) * 13.0, 0.0, 0.0];
                g.territory(p, 8) == g.territory([p[0] + 1.0, 0.0, 0.0], 8)
            })
            .count();
        assert!(same > 180, "territories must be contiguous: {same}/200");
    }

    #[test]
    fn nearby_is_sorted_and_star_lookup_roundtrips() {
        let g = Galaxy::new(3);
        let near = g.nearby([5.0, 0.0, 5.0], 15.0);
        assert!(!near.is_empty());
        for w in near.windows(2) {
            assert!(w[0].distance_to([5.0, 0.0, 5.0]) <= w[1].distance_to([5.0, 0.0, 5.0]));
        }
        let s = &near[0];
        let again = g.star(s.id).expect("star");
        assert_eq!(again.seed, s.seed);
    }

    #[test]
    fn all_star_classes_occur_and_generation_is_fast_enough() {
        let g = Galaxy::new(11);
        let mut classes = std::collections::BTreeSet::new();
        let t = std::time::Instant::now();
        let mut systems = 0;
        'outer: for x in -20..20 {
            for z in -20..20 {
                for s in g.sector((x, 0, z)) {
                    classes.insert(s.class() as usize);
                    // Full generation must be cheap enough to do on demand.
                    let sys = crate::generate_system(s.seed);
                    assert_eq!(sys.name, s.header.name, "header must match full generation");
                    assert_eq!(sys.star_class, s.header.class);
                    systems += 1;
                    if systems >= 300 {
                        break 'outer;
                    }
                }
            }
        }
        assert!(classes.len() >= 7, "{classes:?}");
        assert!(
            t.elapsed().as_secs_f64() / systems as f64 * 1000.0 < 20.0,
            "system generation too slow"
        );
    }
}
