//! The universe: the neighbouring galaxies, all rebuilt from `hash(seed, cell)`. The home
//! galaxy (id `GalaxyId::HOME`) keeps the universe seed itself, so existing games do not change;
//! every other galaxy has its own derived seed for `Galaxy::new`, which is what intergalactic
//! travel will arrive in.
use crate::galaxy::Galaxy;
use vector_core::{hash_coord, Rng};

/// Edge of a universe cell in millions of light years.
pub const CELL_MLY: f64 = 2.0;

/// Identifies one galaxy: its cell and its index in the cell.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct GalaxyId {
    pub cell: (i64, i64, i64),
    pub index: u8,
}

impl GalaxyId {
    /// The galaxy the game starts in.
    pub const HOME: GalaxyId = GalaxyId {
        cell: (0, 0, 0),
        index: 0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GalaxyKind {
    Spiral,
    Barred,
    Elliptical,
    Irregular,
}

impl GalaxyKind {
    pub fn label(self) -> &'static str {
        match self {
            GalaxyKind::Spiral => "SPIRALE",
            GalaxyKind::Barred => "SPIRALE BARREE",
            GalaxyKind::Elliptical => "ELLIPTIQUE",
            GalaxyKind::Irregular => "IRREGULIERE",
        }
    }
}

#[derive(Clone, Debug)]
pub struct GalaxyInfo {
    pub id: GalaxyId,
    /// Position in millions of light years, the home galaxy at the origin.
    pub pos: [f64; 3],
    /// Radius in thousands of light years.
    pub radius_kly: f64,
    pub kind: GalaxyKind,
    /// Number of stars, in billions.
    pub stars_billions: f64,
    pub seed: u64,
    pub name: String,
}

#[derive(Clone, Copy, Debug)]
pub struct Universe {
    pub seed: u64,
}

impl Universe {
    pub fn new(seed: u64) -> Universe {
        Universe { seed }
    }

    /// The seed of galaxy `id`: the universe seed for home, a derived one for the others.
    pub fn galaxy_seed(&self, id: GalaxyId) -> u64 {
        if id == GalaxyId::HOME {
            self.seed
        } else {
            hash_coord(self.seed ^ 0x9A1A_C71C, id.cell.0, id.cell.1, id.cell.2)
                ^ u64::from(id.index)
        }
    }

    /// The `Galaxy` of `id`, ready to generate sectors.
    pub fn galaxy_of(&self, id: GalaxyId) -> Galaxy {
        Galaxy::new(self.galaxy_seed(id))
    }

    /// The galaxies of one cell.
    pub fn cell(&self, cell: (i64, i64, i64)) -> Vec<GalaxyInfo> {
        let mut rng = Rng::new(hash_coord(self.seed ^ 0x0C11, cell.0, cell.1, cell.2));
        let n = if cell == (0, 0, 0) {
            1 + (rng.next_u64() % 2) as u8
        } else {
            (rng.next_u64() % 3) as u8
        };
        (0..n)
            .map(|i| {
                let id = GalaxyId { cell, index: i };
                let pos = if id == GalaxyId::HOME {
                    [0.0; 3]
                } else {
                    [
                        (cell.0 as f64 + rng.f64()) * CELL_MLY,
                        (cell.1 as f64 + rng.f64()) * CELL_MLY,
                        (cell.2 as f64 + rng.f64()) * CELL_MLY,
                    ]
                };
                let roll = rng.f64();
                let kind = if id == GalaxyId::HOME {
                    GalaxyKind::Spiral
                } else if roll < 0.35 {
                    GalaxyKind::Spiral
                } else if roll < 0.55 {
                    GalaxyKind::Barred
                } else if roll < 0.85 {
                    GalaxyKind::Elliptical
                } else {
                    GalaxyKind::Irregular
                };
                let size = rng.range(0.15, 1.0);
                let radius_kly = if id == GalaxyId::HOME {
                    60.0
                } else {
                    10.0 + 140.0 * size * size
                };
                let stars_billions = if id == GalaxyId::HOME {
                    200.0
                } else {
                    (radius_kly / 60.0).powi(2) * 200.0 * rng.range(0.5, 1.5)
                };
                let seed = self.galaxy_seed(id);
                let name = if id == GalaxyId::HOME {
                    Galaxy::new(seed).name()
                } else {
                    format!("{} {}", Galaxy::new(seed).name(), kind_tag(kind))
                };
                GalaxyInfo {
                    id,
                    pos,
                    radius_kly,
                    kind,
                    stars_billions,
                    seed,
                    name,
                }
            })
            .collect()
    }

    /// One galaxy by id, or None when the id names nothing.
    pub fn galaxy(&self, id: GalaxyId) -> Option<GalaxyInfo> {
        self.cell(id.cell).into_iter().nth(usize::from(id.index))
    }

    /// The galaxies within `radius_mly` of `pos` (millions of light years), nearest first.
    pub fn galaxies_near(&self, pos: [f64; 3], radius_mly: f64) -> Vec<GalaxyInfo> {
        let reach = (radius_mly / CELL_MLY).ceil() as i64;
        let c = |v: f64| (v / CELL_MLY).floor() as i64;
        let (cx, cy, cz) = (c(pos[0]), c(pos[1]), c(pos[2]));
        let mut v = Vec::new();
        for dx in -reach..=reach {
            for dy in -reach..=reach {
                for dz in -reach..=reach {
                    for g in self.cell((cx + dx, cy + dy, cz + dz)) {
                        if dist(g.pos, pos) <= radius_mly {
                            v.push(g);
                        }
                    }
                }
            }
        }
        v.sort_by(|a, b| dist(a.pos, pos).total_cmp(&dist(b.pos, pos)));
        v
    }
}

fn kind_tag(k: GalaxyKind) -> &'static str {
    match k {
        GalaxyKind::Spiral => "S",
        GalaxyKind::Barred => "SB",
        GalaxyKind::Elliptical => "E",
        GalaxyKind::Irregular => "Irr",
    }
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_universe_is_deterministic_and_home_keeps_its_seed() {
        let u = Universe::new(42);
        let a: Vec<_> = u
            .galaxies_near([0.0; 3], 6.0)
            .iter()
            .map(|g| (g.id, g.seed))
            .collect();
        let b: Vec<_> = Universe::new(42)
            .galaxies_near([0.0; 3], 6.0)
            .iter()
            .map(|g| (g.id, g.seed))
            .collect();
        assert_eq!(a, b);
        assert_eq!(u.galaxy_seed(GalaxyId::HOME), 42);
        assert_eq!(u.galaxy(GalaxyId::HOME).expect("home").pos, [0.0; 3]);
        assert!(a.len() > 3);
    }

    #[test]
    fn neighbours_have_distinct_seeds_and_galaxies() {
        let u = Universe::new(1);
        let n = u.galaxies_near([0.0; 3], 8.0);
        let mut seeds: Vec<u64> = n.iter().map(|g| g.seed).collect();
        seeds.sort_unstable();
        seeds.dedup();
        assert_eq!(seeds.len(), n.len());
        let g = n
            .iter()
            .find(|g| g.id != GalaxyId::HOME)
            .expect("a neighbour");
        assert_ne!(u.galaxy_of(g.id).sector((0, 0, 0)).len(), usize::MAX);
        assert_eq!(u.galaxy(g.id).expect("same").seed, g.seed);
        assert!(n
            .windows(2)
            .all(|w| dist(w[0].pos, [0.0; 3]) <= dist(w[1].pos, [0.0; 3])));
        assert!(u
            .galaxy(GalaxyId {
                cell: (0, 0, 0),
                index: 200
            })
            .is_none());
    }
}
