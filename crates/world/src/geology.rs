//! Geology of a solid world: plates drifting on the mantle (Euler poles), the three kinds of
//! boundary they make, the relief that follows (mountain chains, trenches, ridges, rifts), the
//! hot spots and arcs of volcanoes, long-term erosion (water, wind, ice), the rock of every cell
//! and the ore bodies that sit in it. Everything is a pure function of the seed of the body, its
//! `Activity` (see `physics.rs`) and its age: nothing is stored, and the relief of the surface
//! map (`surface.rs`) is derived from it.
use crate::physics::Activity;
use crate::surface::{fbm, Surface, H, W};
use crate::PlanetKind;
use std::f64::consts::{PI, TAU};
use vector_core::hash_coord;

/// Latitude (rad) from which ice grinds the land.
pub const ICE_LATITUDE: f64 = 0.9;
/// Surface pressure (bar) from which the wind has the full effect.
pub const WIND_MIN_BAR: f64 = 0.05;

/// What the plates do where two of them meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boundary {
    /// They push: mountain chains on land, trenches under the sea, volcanic arcs.
    Convergent,
    /// They part: mid-ocean ridges, rift valleys.
    Divergent,
    /// They slide: faults, earthquakes, no relief.
    Transform,
}

impl Boundary {
    pub const ALL: [Boundary; 3] = [Self::Convergent, Self::Divergent, Self::Transform];
    pub fn id(self) -> &'static str {
        ["convergent", "divergent", "transform"][self as usize]
    }
    fn code(self) -> u8 {
        self as u8 + 1
    }
    pub fn from_code(c: u8) -> Option<Boundary> {
        c.checked_sub(1)
            .and_then(|k| Self::ALL.get(usize::from(k)).copied())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rock {
    Igneous,
    Sedimentary,
    Metamorphic,
}

impl Rock {
    pub const ALL: [Rock; 3] = [Self::Igneous, Self::Sedimentary, Self::Metamorphic];
    pub fn id(self) -> &'static str {
        ["igneous", "sedimentary", "metamorphic"][self as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolcanoKind {
    /// Above a subducting plate.
    Arc,
    /// Over a plume of the mantle, left behind by the drifting plate as a chain.
    Hotspot,
    /// Along a rift where the crust thins.
    Rift,
}

impl VolcanoKind {
    pub const ALL: [VolcanoKind; 3] = [Self::Arc, Self::Hotspot, Self::Rift];
    pub fn id(self) -> &'static str {
        ["arc", "hotspot", "rift"][self as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepositKind {
    /// Metal veins of the mountain belts and volcanic arcs.
    MetalVein,
    /// Sulphides of the hot springs of the ridges.
    Hydrothermal,
    /// Diamond pipes in the old stable heart of the continents.
    Kimberlite,
    /// Salt and lithium left by dried seas in the dry basins.
    Evaporite,
    /// Oil and gas of the thick sedimentary basins (worlds that had life).
    Petroleum,
    /// Lithium, uranium and rare earths of the granite veins at the edge of the belts.
    Pegmatite,
    /// Ice fields of the cold lands.
    IceField,
}

impl DepositKind {
    pub const ALL: [DepositKind; 7] = [
        Self::MetalVein,
        Self::Hydrothermal,
        Self::Kimberlite,
        Self::Evaporite,
        Self::Petroleum,
        Self::Pegmatite,
        Self::IceField,
    ];
    pub fn id(self) -> &'static str {
        [
            "metal_vein",
            "hydrothermal",
            "kimberlite",
            "evaporite",
            "petroleum",
            "pegmatite",
            "ice_field",
        ][self as usize]
    }
    pub fn from_id(id: &str) -> Option<DepositKind> {
        Self::ALL.iter().copied().find(|k| k.id() == id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plate {
    pub center: [f64; 3],
    /// Rotation vector (axis times rate): the ground at `p` moves at `pole x p`.
    pub pole: [f64; 3],
    pub oceanic: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volcano {
    pub x: usize,
    pub y: usize,
    pub kind: VolcanoKind,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Deposit {
    pub x: usize,
    pub y: usize,
    pub kind: DepositKind,
    /// 0.2..1: how rich the body is.
    pub richness: f32,
}

/// What a body needs to know about its inside to grow a ground.
#[derive(Clone, Copy, Debug)]
pub struct GeoParams {
    pub activity: Activity,
    /// Age of the system, Gyr.
    pub age_gyr: f64,
    /// Surface pressure, bar (wind needs air).
    pub pressure: f64,
}

impl GeoParams {
    /// What a kind of world usually does inside, when no physics is at hand.
    pub fn for_kind(kind: PlanetKind, pressure: f64) -> GeoParams {
        use PlanetKind::*;
        let activity = match kind {
            Terran | Ocean | Tundra => Activity::Plates,
            Lava => Activity::Volcanic,
            Desert | Venusian | Ice | Barren => Activity::Stagnant,
            Dwarf => Activity::Dead,
            GasGiant | IceGiant | HotJupiter => Activity::Fluid,
        };
        GeoParams {
            activity,
            age_gyr: 4.5,
            pressure,
        }
    }
}

/// Plates, boundaries, rock, volcanoes and ore of one world.
#[derive(Clone, Debug)]
pub struct Geology {
    pub activity: Activity,
    pub age_gyr: f32,
    pub plates: Vec<Plate>,
    /// Plate of every cell.
    pub plate: Vec<u8>,
    /// Boundary code of every cell (0 = none, else `Boundary::from_code`).
    pub boundary: Vec<u8>,
    /// How close to the line of a boundary, 0..1.
    pub strength: Vec<f32>,
    /// Rock of every cell (index into `Rock::ALL`).
    pub rock: Vec<u8>,
    /// Sediment piled up by erosion, in elevation units.
    pub sediment: Vec<f32>,
    pub volcanoes: Vec<Volcano>,
    pub deposits: Vec<Deposit>,
}

/// Result of the first phase: the relief the plates give, before sea level.
pub struct Tectonics {
    pub geo: Geology,
    /// Relief to add to the raw noise (already weighted by the activity of the world).
    pub relief: Vec<f32>,
}

fn unit(seed: u64, a: i64, b: i64, c: i64) -> f64 {
    (hash_coord(seed, a, b, c) >> 11) as f64 / (1u64 << 53) as f64
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> [f64; 3] {
    let l = dot(a, a).sqrt().max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn random_unit(seed: u64, i: i64, salt: i64) -> [f64; 3] {
    let z = 2.0 * unit(seed, i, salt, 1) - 1.0;
    let phi = TAU * unit(seed, i, salt, 2);
    let r = (1.0 - z * z).sqrt();
    [r * phi.cos(), z, r * phi.sin()]
}

/// Grid cell of a point of the unit sphere.
pub fn cell_of(p: [f64; 3]) -> (usize, usize) {
    let lat = p[1].clamp(-1.0, 1.0).asin();
    let lon = p[2].atan2(p[0]).rem_euclid(TAU);
    let x = ((lon / TAU * W as f64) as usize).min(W - 1);
    let y = (((0.5 - lat / PI) * H as f64) as usize).min(H - 1);
    (x, y)
}

fn sphere(x: usize, y: usize) -> [f64; 3] {
    let (lat, lon) = (Surface::lat(y), Surface::lon(x));
    [lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin()]
}

/// Rotate `p` about the unit `axis` by `angle` (Rodrigues).
fn rotate(p: [f64; 3], axis: [f64; 3], angle: f64) -> [f64; 3] {
    let (s, c) = angle.sin_cos();
    let k = cross(axis, p);
    let d = dot(axis, p) * (1.0 - c);
    [
        p[0] * c + k[0] * s + axis[0] * d,
        p[1] * c + k[1] * s + axis[1] * d,
        p[2] * c + k[2] * s + axis[2] * d,
    ]
}

/// Distance in cells between two cells (longitude wraps).
pub fn cell_dist(a: (usize, usize), b: (usize, usize)) -> f64 {
    let dx = (a.0 as isize - b.0 as isize).unsigned_abs();
    let dx = dx.min(W - dx) as f64;
    let dy = (a.1 as isize - b.1 as isize).unsigned_abs() as f64;
    (dx * dx + dy * dy).sqrt()
}

/// First phase: plates, boundaries, hot spots and the relief they give. `water` is the share of
/// the surface under the sea (it decides which plates are oceanic).
pub fn tectonics(seed: u64, water: f64, p: &GeoParams) -> Tectonics {
    let seed = seed ^ 0x7EC7_0141;
    let n_plates = match p.activity {
        Activity::Plates => 6 + (hash_coord(seed, 1, 1, 1) % 6) as usize,
        Activity::Volcanic => 10 + (hash_coord(seed, 1, 1, 2) % 6) as usize,
        _ => 1,
    };
    let weight = match p.activity {
        Activity::Plates => 1.0,
        Activity::Volcanic => 0.7,
        Activity::Stagnant => 0.5,
        _ => 0.0,
    };
    let oceanic_share = if water > 0.0 { water + 0.05 } else { 0.0 };
    let plates: Vec<Plate> = (0..n_plates)
        .map(|i| {
            let axis = random_unit(seed, i as i64, 3);
            let rate = if n_plates > 1 {
                0.3 + 0.7 * unit(seed, i as i64, 5, 0)
            } else {
                0.0
            };
            Plate {
                center: random_unit(seed, i as i64, 1),
                pole: [axis[0] * rate, axis[1] * rate, axis[2] * rate],
                oceanic: unit(seed, i as i64, 6, 0) < oceanic_share,
            }
        })
        .collect();
    let w = if p.activity == Activity::Volcanic {
        0.12
    } else {
        0.16
    };
    let mut plate = vec![0u8; W * H];
    let mut boundary = vec![0u8; W * H];
    let mut strength = vec![0.0f32; W * H];
    let mut relief = vec![0.0f32; W * H];
    let points: Vec<[f64; 3]> = (0..W * H).map(|i| sphere(i % W, i / W)).collect();
    for y in 0..H {
        for x in 0..W {
            let i = Surface::idx(x, y);
            let pt = points[i];
            // The edge between plates is not a straight line: warp the point a little.
            let q = norm([
                pt[0] + 0.10 * fbm(seed ^ 1, [pt[0] * 2.5, pt[1] * 2.5, pt[2] * 2.5], 3),
                pt[1] + 0.10 * fbm(seed ^ 2, [pt[0] * 2.5, pt[1] * 2.5, pt[2] * 2.5], 3),
                pt[2] + 0.10 * fbm(seed ^ 3, [pt[0] * 2.5, pt[1] * 2.5, pt[2] * 2.5], 3),
            ]);
            let (mut b1, mut b2) = ((f64::MIN, 0usize), (f64::MIN, 0usize));
            for (k, pl) in plates.iter().enumerate() {
                let d = dot(q, pl.center);
                if d > b1.0 {
                    b2 = b1;
                    b1 = (d, k);
                } else if d > b2.0 {
                    b2 = (d, k);
                }
            }
            plate[i] = b1.1 as u8;
            let own = plates[b1.1];
            let mut offset = if own.oceanic { -0.35 } else { 0.30 };
            if n_plates > 1 {
                let other = plates[b2.1];
                let gap = b2.0.clamp(-1.0, 1.0).acos() - b1.0.clamp(-1.0, 1.0).acos();
                let s = (-(gap / w).powi(2)).exp();
                // Soft step between the two crusts, so the plates do not end in a cliff.
                let other_off = if other.oceanic { -0.35 } else { 0.30 };
                offset = offset * (1.0 - 0.5 * s) + other_off * 0.5 * s;
                let dirv = sub(other.center, own.center);
                let n = norm(sub(
                    dirv,
                    [
                        pt[0] * dot(dirv, pt),
                        pt[1] * dot(dirv, pt),
                        pt[2] * dot(dirv, pt),
                    ],
                ));
                let rel = sub(cross(own.pole, pt), cross(other.pole, pt));
                let closing = dot(rel, n);
                let shear = dot(rel, cross(pt, n));
                let kind = if shear.abs() > 1.2 * closing.abs() {
                    Boundary::Transform
                } else if closing > 0.0 {
                    Boundary::Convergent
                } else {
                    Boundary::Divergent
                };
                let m = (closing.abs() / 0.6).min(1.0) * s;
                let mut t = 0.0;
                match kind {
                    Boundary::Convergent => {
                        t = match (own.oceanic, other.oceanic) {
                            (false, _) => 1.0 * m,
                            (true, false) => -0.9 * m,
                            (true, true) => {
                                if b1.1 < b2.1 {
                                    -0.9 * m
                                } else {
                                    0.5 * m
                                }
                            }
                        };
                    }
                    Boundary::Divergent => {
                        t = if own.oceanic { 0.35 * m } else { -0.5 * m };
                    }
                    Boundary::Transform => {}
                }
                if s > 0.25 {
                    boundary[i] = kind.code();
                    strength[i] = s as f32;
                }
                offset += t;
            }
            relief[i] = (offset * weight) as f32;
        }
    }
    // Hot spots: a plume under a plate leaves a chain of volcanoes as the plate drifts over it.
    let n_hot = match p.activity {
        Activity::Plates => 2 + (hash_coord(seed, 2, 1, 1) % 2) as usize,
        Activity::Volcanic => 6 + (hash_coord(seed, 2, 1, 2) % 3) as usize,
        Activity::Stagnant => 1 + (hash_coord(seed, 2, 1, 3) % 2) as usize,
        _ => 0,
    };
    let mut volcanoes = Vec::new();
    for h in 0..n_hot {
        let p0 = random_unit(seed, h as i64, 40);
        let (k_max, step, top) = if n_plates > 1 {
            (6, 0.13, 0.9)
        } else {
            (1, 0.0, 1.1)
        };
        let nearest = plates
            .iter()
            .max_by(|a, b| dot(a.center, p0).total_cmp(&dot(b.center, p0)))
            .copied()
            .expect("at least one plate");
        let rate = dot(nearest.pole, nearest.pole).sqrt();
        let axis = if rate > 1e-9 {
            norm(nearest.pole)
        } else {
            [0.0, 1.0, 0.0]
        };
        for k in 0..k_max {
            let pk = rotate(p0, axis, -(k as f64) * step * rate.max(0.3));
            let height = top * 0.7f64.powi(k);
            let sigma = if n_plates > 1 { 0.075 } else { 0.16 };
            for y in 0..H {
                for x in 0..W {
                    let d = dot(points[Surface::idx(x, y)], pk).clamp(-1.0, 1.0).acos();
                    if d < 3.0 * sigma {
                        relief[Surface::idx(x, y)] +=
                            (height * (-(d / sigma).powi(2)).exp() * weight) as f32;
                    }
                }
            }
            if k <= 3 {
                let (x, y) = cell_of(pk);
                volcanoes.push(Volcano {
                    x,
                    y,
                    kind: VolcanoKind::Hotspot,
                    active: k == 0,
                });
            }
        }
    }
    // Arcs above the trenches and volcanoes along the rifts.
    let (mut arcs, mut rifts) = (0, 0);
    for y in 0..H {
        for x in 0..W {
            let i = Surface::idx(x, y);
            let roll = unit(seed, i as i64, 77, 0);
            match Boundary::from_code(boundary[i]) {
                Some(Boundary::Convergent)
                    if (0.25..0.8).contains(&strength[i]) && relief[i] > 0.25 && arcs < 24 =>
                {
                    if roll < 0.07 {
                        arcs += 1;
                        volcanoes.push(Volcano {
                            x,
                            y,
                            kind: VolcanoKind::Arc,
                            active: unit(seed, i as i64, 78, 0) < 0.7,
                        });
                    }
                }
                Some(Boundary::Divergent) if strength[i] > 0.7 && rifts < 12 && roll < 0.04 => {
                    rifts += 1;
                    volcanoes.push(Volcano {
                        x,
                        y,
                        kind: VolcanoKind::Rift,
                        active: true,
                    });
                }
                _ => {}
            }
        }
    }
    if p.activity == Activity::Dead {
        volcanoes.clear();
    }
    Tectonics {
        geo: Geology {
            activity: p.activity,
            age_gyr: p.age_gyr as f32,
            plates,
            plate,
            boundary,
            strength,
            rock: Vec::new(),
            sediment: vec![0.0; W * H],
            volcanoes,
            deposits: Vec::new(),
        },
        relief,
    }
}

/// Long-term erosion of a relief in `[-1, 1]` (sea level at 0): rain and rivers carry the high
/// ground to the low (and the sea floor), wind smooths dry open worlds, ice grinds the high
/// latitudes. The longer the age and the wetter the world, the flatter the mountains. Land stays
/// land and sea stays sea, so the sea fraction is unchanged. Fills `sediment`.
pub fn erode(elev: &mut [f32], sediment: &mut [f32], p: &GeoParams, wet: f64) {
    let age = (p.age_gyr / 4.5).clamp(0.0, 2.0);
    let hyd = wet * age;
    let air = (p.pressure / WIND_MIN_BAR).min(1.0);
    let wind = (1.0 - wet).max(0.0) * air * age;
    if (hyd < 0.01 && wind < 0.01 && air < 0.5) || age <= 0.0 {
        return;
    }
    let rounds = 2 + (6.0 * age) as usize;
    let ice_start = ICE_LATITUDE;
    for _ in 0..rounds {
        let src = elev.to_vec();
        let mut delta = vec![0.0f32; W * H];
        for y in 0..H {
            let cold = ((Surface::lat(y).abs() - ice_start) / 0.4).clamp(0.0, 1.0);
            for x in 0..W {
                let i = Surface::idx(x, y);
                let e = src[i];
                if e <= 0.0 {
                    continue;
                }
                let (mut low, mut low_e, mut sum, mut n) = (i, e, 0.0f32, 0.0f32);
                for dy in -1isize..=1 {
                    let yy = y as isize + dy;
                    if !(0..H as isize).contains(&yy) {
                        continue;
                    }
                    for dx in -1isize..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let xx = (x as isize + dx).rem_euclid(W as isize) as usize;
                        let j = Surface::idx(xx, yy as usize);
                        sum += src[j];
                        n += 1.0;
                        if src[j] < low_e {
                            low_e = src[j];
                            low = j;
                        }
                    }
                }
                let drop = e - low_e;
                // Water: take a share of the slope off the cell and lay it downhill.
                let take = (0.12 * hyd as f32 * drop).min(e);
                if take > 0.0 && low != i {
                    delta[i] -= take;
                    delta[low] += 0.8 * take;
                    sediment[low] += 0.8 * take;
                }
                // Wind and ice smooth toward the mean of the neighbours.
                let smooth = (0.08 * wind as f32 + 0.12 * cold as f32 * air as f32) * (e - sum / n);
                delta[i] -= smooth;
                if smooth < 0.0 {
                    sediment[i] -= smooth;
                }
            }
        }
        for i in 0..W * H {
            let e = elev[i] + delta[i];
            elev[i] = if elev[i] > 0.0 {
                e.max(0.0)
            } else {
                // The sea floor takes sediment but never comes up to the surface.
                e.min(-0.002)
            };
        }
    }
}

impl Geology {
    /// Boundary at a cell, if it is within the reach of one.
    pub fn boundary_at(&self, x: usize, y: usize) -> Option<Boundary> {
        Boundary::from_code(self.boundary[Surface::idx(x, y)])
    }

    pub fn rock_at(&self, x: usize, y: usize) -> Rock {
        Rock::ALL[usize::from(self.rock[Surface::idx(x, y)]).min(2)]
    }

    pub fn plate_at(&self, x: usize, y: usize) -> usize {
        usize::from(self.plate[Surface::idx(x, y)])
    }

    /// Cells that lie on a fault line: where the ground shakes.
    pub fn fault_cells(&self) -> Vec<(usize, usize)> {
        let mut v = Vec::new();
        for y in 0..H {
            for x in 0..W {
                let i = Surface::idx(x, y);
                if self.boundary[i] != 0 && self.strength[i] > 0.5 {
                    v.push((x, y));
                }
            }
        }
        v
    }

    /// Where an earthquake strikes for the roll `h`: on a plate boundary of a world with plates
    /// or volcanoes, under a volcano on a world with a single rigid shell, nowhere on a dead one.
    pub fn quake_site(&self, h: u64) -> Option<(usize, usize)> {
        match self.activity {
            Activity::Plates | Activity::Volcanic => {
                let f = self.fault_cells();
                (!f.is_empty()).then(|| f[(h % f.len() as u64) as usize])
            }
            Activity::Stagnant => (!self.volcanoes.is_empty()).then(|| {
                let v = self.volcanoes[(h % self.volcanoes.len() as u64) as usize];
                (v.x, v.y)
            }),
            _ => None,
        }
    }

    /// Where a volcano erupts for the roll `h`: always at an active one.
    pub fn eruption_site(&self, h: u64) -> Option<(usize, usize)> {
        let act: Vec<&Volcano> = self.volcanoes.iter().filter(|v| v.active).collect();
        (!act.is_empty()).then(|| {
            let v = act[(h % act.len() as u64) as usize];
            (v.x, v.y)
        })
    }

    /// Heat of the ground at a cell, 0 (cold rock) to 1.5 (a lava lake): the base heat of the
    /// world plus its plate edges and active volcanoes. Feeds geothermal plants.
    pub fn heat_at(&self, x: usize, y: usize) -> f32 {
        let base = match self.activity {
            Activity::Volcanic => 0.7,
            Activity::Plates => 0.35,
            Activity::Stagnant => 0.12,
            _ => 0.02,
        };
        let i = Surface::idx(x, y);
        let edge = if matches!(
            Boundary::from_code(self.boundary[i]),
            Some(Boundary::Convergent | Boundary::Divergent)
        ) {
            0.5 * self.strength[i]
        } else {
            0.0
        };
        let volc = self
            .volcanoes
            .iter()
            .filter(|v| v.active)
            .map(|v| (1.0 - cell_dist((x, y), (v.x, v.y)) / 5.0).max(0.0) as f32)
            .fold(0.0f32, f32::max);
        (base + edge + 0.8 * volc).clamp(0.0, 1.5)
    }

    /// Deposits within `reach` cells of a point, nearest first.
    pub fn deposits_near(&self, c: (usize, usize), reach: f64) -> Vec<Deposit> {
        let mut v: Vec<(f64, Deposit)> = self
            .deposits
            .iter()
            .map(|d| (cell_dist(c, (d.x, d.y)), *d))
            .filter(|(d, _)| *d <= reach)
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v.into_iter().map(|p| p.1).collect()
    }
}

/// Greedy choice of the `n` best cells of a score map, at least `gap` cells apart.
fn best_cells(score: &[f32], n: usize, gap: f64) -> Vec<(usize, f32)> {
    let mut order: Vec<usize> = (0..W * H).filter(|&i| score[i] > 0.05).collect();
    order.sort_by(|&a, &b| score[b].total_cmp(&score[a]).then(a.cmp(&b)));
    let mut out: Vec<(usize, f32)> = Vec::new();
    for i in order {
        if out.len() >= n {
            break;
        }
        let c = (i % W, i / W);
        if out
            .iter()
            .all(|&(j, _)| cell_dist(c, (j % W, j / W)) >= gap)
        {
            out.push((i, score[i]));
        }
    }
    out
}

/// Second phase, once the surface is known: the rock of every cell and the ore bodies.
pub fn classify(
    geo: &mut Geology,
    seed: u64,
    kind: PlanetKind,
    elev: &[f32],
    hum: &[f32],
    temp: &[f32],
) {
    let seed = seed ^ 0x00DE_9051;
    let active = geo
        .volcanoes
        .iter()
        .filter(|v| v.active)
        .copied()
        .collect::<Vec<_>>();
    let weathered = geo.sediment.iter().any(|&s| s > 0.0);
    geo.rock = (0..W * H)
        .map(|i| {
            let c = (i % W, i / W);
            let near_volcano = geo.volcanoes.iter().any(|v| cell_dist(c, (v.x, v.y)) < 3.0);
            let b = Boundary::from_code(geo.boundary[i]);
            let r = if near_volcano {
                Rock::Igneous
            } else if elev[i] < 0.0 {
                if elev[i] < -0.15 && geo.sediment[i] < 0.02 {
                    Rock::Igneous
                } else {
                    Rock::Sedimentary
                }
            } else if b == Some(Boundary::Convergent) && geo.strength[i] > 0.3 {
                Rock::Metamorphic
            } else if geo.sediment[i] > 0.02 || (weathered && elev[i] < 0.10 && hum[i] > 0.1) {
                Rock::Sedimentary
            } else {
                Rock::Igneous
            };
            r as u8
        })
        .collect();
    let tectonic = matches!(geo.activity, Activity::Plates | Activity::Volcanic);
    let noise = |i: usize, k: i64| 0.15 * unit(seed, i as i64, k, 0) as f32;
    let mut out: Vec<Deposit> = Vec::new();
    let mut push = |kind: DepositKind, score: Vec<f32>, n: usize, gap: f64| {
        for (i, s) in best_cells(&score, n, gap) {
            out.push(Deposit {
                x: i % W,
                y: i / W,
                kind,
                richness: s.clamp(0.2, 1.0),
            });
        }
    };
    let land = |i: usize| elev[i] >= 0.0;
    let sc = |f: &dyn Fn(usize) -> f32| -> Vec<f32> { (0..W * H).map(f).collect() };
    if tectonic {
        push(
            DepositKind::MetalVein,
            sc(&|i| {
                let conv = geo.boundary[i] == Boundary::Convergent.code();
                let arc = active
                    .iter()
                    .filter(|v| v.kind == VolcanoKind::Arc)
                    .any(|v| cell_dist((i % W, i / W), (v.x, v.y)) < 4.0);
                if land(i) && (conv || arc) {
                    geo.strength[i].max(0.4) + noise(i, 1)
                } else {
                    0.0
                }
            }),
            7,
            5.0,
        );
        push(
            DepositKind::Hydrothermal,
            sc(&|i| {
                if geo.boundary[i] == Boundary::Divergent.code() {
                    geo.strength[i] + noise(i, 2)
                } else {
                    0.0
                }
            }),
            4,
            6.0,
        );
        push(
            DepositKind::Pegmatite,
            sc(&|i| {
                let s = geo.strength[i];
                if land(i) && geo.boundary[i] != 0 && (0.25..0.6).contains(&s) {
                    0.5 + noise(i, 3)
                } else {
                    0.0
                }
            }),
            3,
            8.0,
        );
    }
    // Old, stable heart of a continent: far from any boundary, on high ground, on hard rock.
    if tectonic || (geo.activity == Activity::Stagnant && geo.age_gyr > 2.0) {
        push(
            DepositKind::Kimberlite,
            sc(&|i| {
                if land(i)
                    && geo.strength[i] < 0.02
                    && elev[i] > 0.05
                    && geo.rock[i] == Rock::Igneous as u8
                {
                    0.4 + elev[i].min(0.5) + noise(i, 4)
                } else {
                    0.0
                }
            }),
            2,
            12.0,
        );
    }
    push(
        DepositKind::Evaporite,
        sc(&|i| {
            if land(i) && elev[i] < 0.12 && hum[i] < 0.35 && temp[i] > 285.0 && weathered {
                0.5 + geo.sediment[i].min(0.4) + noise(i, 5)
            } else {
                0.0
            }
        }),
        4,
        6.0,
    );
    if matches!(
        kind,
        PlanetKind::Terran
            | PlanetKind::Ocean
            | PlanetKind::Tundra
            | PlanetKind::Desert
            | PlanetKind::Ice
    ) {
        push(
            DepositKind::Petroleum,
            sc(&|i| {
                if geo.rock[i] == Rock::Sedimentary as u8
                    && geo.sediment[i] > 0.01
                    && elev[i] > -0.2
                {
                    (geo.sediment[i] * 4.0).min(0.8) + 0.2 + noise(i, 6)
                } else {
                    0.0
                }
            }),
            4,
            8.0,
        );
    }
    push(
        DepositKind::IceField,
        sc(&|i| {
            if temp[i] < 265.0 && land(i) && (hum[i] > 0.05 || kind == PlanetKind::Ice) {
                (270.0 - temp[i]) / 40.0 + noise(i, 7)
            } else {
                0.0
            }
        }),
        3,
        8.0,
    );
    geo.deposits = out;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::generate_with;

    fn earthlike(seed: u64, age: f64) -> Surface {
        let p = GeoParams {
            activity: Activity::Plates,
            age_gyr: age,
            pressure: 1.0,
        };
        generate_with(seed, PlanetKind::Terran, 285.0, &p).expect("solid")
    }

    #[test]
    fn plates_make_the_three_kinds_of_boundary_and_a_plate_covers_every_cell() {
        let s = earthlike(3, 4.5);
        let g = &s.geo;
        assert!(g.plates.len() >= 6);
        assert!(g.plate.iter().all(|&p| usize::from(p) < g.plates.len()));
        let count = |b: Boundary| g.boundary.iter().filter(|&&c| c == b.code()).count();
        for b in Boundary::ALL {
            assert!(count(b) > 20, "{b:?}: {}", count(b));
        }
    }

    #[test]
    fn mountains_stand_on_convergent_boundaries_and_trenches_are_under_the_sea() {
        let mut high = (0.0f64, 0u32);
        let mut other = (0.0f64, 0u32);
        let mut trench_sea = (0u32, 0u32);
        for seed in 1..8 {
            let s = earthlike(seed, 4.5);
            for i in 0..W * H {
                if s.elev[i] < 0.0 {
                    continue;
                }
                if s.geo.boundary[i] == Boundary::Convergent.code() && s.geo.strength[i] > 0.6 {
                    high = (high.0 + f64::from(s.elev[i]), high.1 + 1);
                } else if s.geo.boundary[i] == 0 {
                    other = (other.0 + f64::from(s.elev[i]), other.1 + 1);
                }
            }
            // Cells of the deepest tenth of the sea that lie on a boundary are mostly convergent.
            let mut deep: Vec<usize> = (0..W * H).filter(|&i| s.elev[i] < -0.5).collect();
            deep.sort_by(|&a, &b| s.elev[a].total_cmp(&s.elev[b]));
            for &i in deep.iter().take(60) {
                trench_sea.1 += 1;
                trench_sea.0 += u32::from(s.geo.boundary[i] != 0);
            }
        }
        assert!(high.1 > 50 && other.1 > 500);
        assert!(
            high.0 / f64::from(high.1) > other.0 / f64::from(other.1) + 0.1,
            "{} vs {}",
            high.0 / f64::from(high.1),
            other.0 / f64::from(other.1)
        );
        assert!(
            f64::from(trench_sea.0) > 0.4 * f64::from(trench_sea.1),
            "deep trenches follow plate edges: {trench_sea:?}"
        );
    }

    #[test]
    fn old_worlds_have_flatter_mountains_than_young_ones() {
        let rough = |s: &Surface| {
            let mut sum = 0.0;
            for y in 0..H {
                for x in 0..W - 1 {
                    let (a, b) = (s.elev[Surface::idx(x, y)], s.elev[Surface::idx(x + 1, y)]);
                    if a > 0.0 && b > 0.0 {
                        sum += f64::from((a - b).abs());
                    }
                }
            }
            sum
        };
        let young = rough(&earthlike(5, 0.1));
        let old = rough(&earthlike(5, 9.0));
        assert!(old < young * 0.95, "erosion must smooth: {old} vs {young}");
        // And it never turns land into sea.
        assert_eq!(earthlike(5, 0.1).sea_frac, earthlike(5, 9.0).sea_frac);
    }

    #[test]
    fn a_dead_world_has_no_plates_no_volcano_no_quake() {
        let p = GeoParams {
            activity: Activity::Dead,
            age_gyr: 8.0,
            pressure: 0.0,
        };
        let s = generate_with(4, PlanetKind::Dwarf, 150.0, &p).expect("solid");
        assert_eq!(s.geo.plates.len(), 1);
        assert!(s.geo.volcanoes.is_empty());
        assert!(s.geo.quake_site(7).is_none() && s.geo.eruption_site(7).is_none());
        let live = earthlike(4, 4.5);
        assert!(live.geo.quake_site(7).is_some());
    }

    #[test]
    fn eruptions_come_from_active_volcanoes_and_hotspots_leave_extinct_chains() {
        let s = earthlike(6, 4.5);
        for h in 0..30u64 {
            let (x, y) = s
                .geo
                .eruption_site(h)
                .expect("a living world has volcanoes");
            assert!(s
                .geo
                .volcanoes
                .iter()
                .any(|v| v.active && v.x == x && v.y == y));
        }
        assert!(s
            .geo
            .volcanoes
            .iter()
            .any(|v| v.kind == VolcanoKind::Hotspot && !v.active));
        assert!(s.geo.volcanoes.iter().any(|v| v.kind == VolcanoKind::Arc));
        // Heat is highest next to an active volcano.
        let v = s.geo.volcanoes.iter().find(|v| v.active).expect("active");
        assert!(s.geo.heat_at(v.x, v.y) > s.geo.heat_at((v.x + 20) % W, (v.y + 20) % H) - 0.0001);
    }

    #[test]
    fn ore_follows_the_geology_that_makes_it() {
        let mut seen = std::collections::BTreeSet::new();
        for seed in 1..12 {
            let s = earthlike(seed, 4.5);
            for d in &s.geo.deposits {
                let i = Surface::idx(d.x, d.y);
                seen.insert(d.kind as usize);
                match d.kind {
                    DepositKind::MetalVein => assert!(
                        s.elev[i] >= 0.0 && s.geo.boundary[i] != 0
                            || s.geo.volcanoes.iter().any(|v| v.kind == VolcanoKind::Arc)
                    ),
                    DepositKind::Hydrothermal => {
                        assert_eq!(s.geo.boundary[i], Boundary::Divergent.code())
                    }
                    DepositKind::Kimberlite => {
                        assert!(
                            s.geo.strength[i] < 0.02,
                            "diamonds sit far from the plate edges"
                        )
                    }
                    DepositKind::Petroleum => {
                        assert_eq!(s.geo.rock[i], Rock::Sedimentary as u8)
                    }
                    DepositKind::Evaporite => assert!(s.elev[i] < 0.12 && s.hum[i] < 0.35),
                    DepositKind::IceField => assert!(s.temp[i] < 265.0),
                    DepositKind::Pegmatite => assert!(s.geo.boundary[i] != 0),
                }
            }
        }
        assert!(seen.len() >= 5, "ore kinds met: {seen:?}");
        // A barren rock without sediment holds no oil.
        let p = GeoParams::for_kind(PlanetKind::Barren, 0.0);
        let b = generate_with(2, PlanetKind::Barren, 300.0, &p).expect("solid");
        assert!(b
            .geo
            .deposits
            .iter()
            .all(|d| d.kind != DepositKind::Petroleum));
    }

    #[test]
    fn geology_is_deterministic() {
        let (a, b) = (earthlike(9, 4.5), earthlike(9, 4.5));
        assert_eq!(a.geo.plate, b.geo.plate);
        assert_eq!(a.geo.deposits, b.geo.deposits);
        assert_eq!(a.geo.volcanoes, b.geo.volcanoes);
        assert_ne!(a.geo.plate, earthlike(10, 4.5).geo.plate);
    }
}
