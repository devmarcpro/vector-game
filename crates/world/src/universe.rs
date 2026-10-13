//! The universe: a lattice of galaxies derived from one seed (minimal version used by the
//! intergalactic voyages; the sky and map views may extend it). Nothing is stored.
//!
//! One galaxy per cell of `CELL_LY` light years, jittered inside its cell. Galaxy 0 is the home
//! galaxy: it sits at the origin and keeps the universe seed as its own seed, so everything
//! generated before this module existed is unchanged.
use crate::galaxy::Galaxy;
use vector_core::{hash_coord, Rng};

/// Edge of the cell holding one galaxy (ly): neighbours are about two million ly apart.
pub const CELL_LY: f64 = 2.0e6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GalaxyKind {
    Spiral,
    Barred,
    Elliptical,
    Dwarf,
    Irregular,
}

impl GalaxyKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Spiral => "SPIRALE",
            Self::Barred => "SPIRALE BARREE",
            Self::Elliptical => "ELLIPTIQUE",
            Self::Dwarf => "NAINE",
            Self::Irregular => "IRREGULIERE",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GalaxyInfo {
    pub id: i64,
    pub seed: u64,
    /// Position in the universe (ly).
    pub pos: [f64; 3],
    pub radius_ly: f64,
    pub kind: GalaxyKind,
}

impl GalaxyInfo {
    pub fn galaxy(&self) -> Galaxy {
        Galaxy::new(self.seed)
    }
    pub fn name(&self) -> String {
        self.galaxy().name()
    }
    pub fn distance_to(&self, p: [f64; 3]) -> f64 {
        ((self.pos[0] - p[0]).powi(2) + (self.pos[1] - p[1]).powi(2) + (self.pos[2] - p[2]).powi(2))
            .sqrt()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Universe {
    pub seed: u64,
}

fn zig(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}
fn unzig(u: u64) -> i64 {
    ((u >> 1) as i64) ^ -((u & 1) as i64)
}

/// Lattice cell of a galaxy id (ids pack three zigzag-coded 20-bit coordinates; 0 is the origin).
pub fn cell_of(id: i64) -> (i64, i64, i64) {
    let u = id as u64;
    (
        unzig(u & 0xF_FFFF),
        unzig((u >> 20) & 0xF_FFFF),
        unzig((u >> 40) & 0xF_FFFF),
    )
}

pub fn id_of(c: (i64, i64, i64)) -> i64 {
    (zig(c.0) | (zig(c.1) << 20) | (zig(c.2) << 40)) as i64
}

impl Universe {
    pub fn new(seed: u64) -> Universe {
        Universe { seed }
    }

    /// Seed of galaxy `id` (the home galaxy keeps the universe seed).
    pub fn galaxy_seed(&self, id: i64) -> u64 {
        if id == 0 {
            self.seed
        } else {
            hash_coord(self.seed ^ 0x6A1A_C71C, id, id >> 20, id >> 40)
        }
    }

    pub fn galaxy(&self, id: i64) -> GalaxyInfo {
        let c = cell_of(id);
        let seed = self.galaxy_seed(id);
        let mut r = Rng::new(hash_coord(seed, 0x6E0, 3, 9));
        let kind = match r.next_u64() % 10 {
            0..=3 => GalaxyKind::Spiral,
            4 | 5 => GalaxyKind::Barred,
            6 | 7 => GalaxyKind::Elliptical,
            8 => GalaxyKind::Dwarf,
            _ => GalaxyKind::Irregular,
        };
        let radius_ly = match kind {
            GalaxyKind::Dwarf => r.range(6_000.0, 20_000.0),
            GalaxyKind::Irregular => r.range(15_000.0, 40_000.0),
            _ => r.range(40_000.0, 120_000.0),
        };
        let pos = if id == 0 {
            [0.0; 3]
        } else {
            let j = |r: &mut Rng| r.range(-0.3, 0.3) * CELL_LY;
            [
                c.0 as f64 * CELL_LY + j(&mut r),
                c.1 as f64 * CELL_LY + j(&mut r),
                c.2 as f64 * CELL_LY + j(&mut r),
            ]
        };
        GalaxyInfo {
            id,
            seed,
            pos,
            radius_ly: if id == 0 { 60_000.0 } else { radius_ly },
            kind,
        }
    }

    /// Galaxies within `radius` ly of `p`, nearest first.
    pub fn galaxies_near(&self, p: [f64; 3], radius: f64) -> Vec<GalaxyInfo> {
        let reach = (radius / CELL_LY).ceil() as i64 + 1;
        let c = (
            (p[0] / CELL_LY).round() as i64,
            (p[1] / CELL_LY).round() as i64,
            (p[2] / CELL_LY).round() as i64,
        );
        let mut v = Vec::new();
        for dx in -reach..=reach {
            for dy in -reach..=reach {
                for dz in -reach..=reach {
                    let g = self.galaxy(id_of((c.0 + dx, c.1 + dy, c.2 + dz)));
                    if g.distance_to(p) <= radius {
                        v.push(g);
                    }
                }
            }
        }
        v.sort_by(|a, b| {
            a.distance_to(p)
                .total_cmp(&b.distance_to(p))
                .then(a.id.cmp(&b.id))
        });
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_home_is_zero() {
        for c in [(0, 0, 0), (1, -2, 3), (-500, 40, -7)] {
            assert_eq!(cell_of(id_of(c)), c);
        }
        assert_eq!(id_of((0, 0, 0)), 0);
        let u = Universe::new(77);
        assert_eq!(u.galaxy_seed(0), 77);
        assert_eq!(u.galaxy(0).pos, [0.0; 3]);
    }

    #[test]
    fn neighbours_are_a_few_million_light_years_away_and_stable() {
        let u = Universe::new(5);
        let near = u.galaxies_near([0.0; 3], 3.5e6);
        assert!(near.len() >= 5, "{}", near.len());
        assert_eq!(near[0].id, 0);
        assert!(near[1].distance_to([0.0; 3]) > 1.0e6);
        let again = u.galaxies_near([0.0; 3], 3.5e6);
        assert_eq!(
            near.iter().map(|g| g.id).collect::<Vec<_>>(),
            again.iter().map(|g| g.id).collect::<Vec<_>>()
        );
        assert_ne!(near[1].seed, near[2].seed);
    }
}
