//! Rivers and lakes of a world, derived from its relief and its humidity. Nothing is stored in a
//! save: the same map always gives the same water. The ground is "filled" (priority flood from
//! the sea, the way water would find its level): every land cell then has a downhill neighbour
//! that is strictly lower, the cells of a basin share the level at which it spills, rain
//! accumulates down the slope, and where enough of it gathers there is a river; a basin that
//! gathers enough water is a lake. A world without sea has no rivers: the water has nowhere to go.
use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::surface::{H, W};

/// The eight neighbours of a cell (dx, dy). `Hydro::down` is an index into this list.
pub const DIRS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];
/// "No downhill neighbour": the sea, or a world with no sea.
pub const NO_DOWN: u8 = 255;
/// Rain gathered upstream (sum of the humidity of the cells above) from which a cell holds a river.
pub const RIVER_FLOW: f32 = 2.5;
/// Rain a basin must gather to hold a lake, and the depth (map elevation units) of its water.
pub const LAKE_FLOW: f32 = 1.0;
pub const LAKE_DEPTH: f32 = 0.004;
/// How much a cell is lifted above the one it drains into when a basin is filled.
const EPS: f32 = 1.0e-5;

#[derive(Clone, Debug, Default)]
pub struct Hydro {
    /// Direction (index into `DIRS`) of the neighbour a cell drains into; `NO_DOWN` for the sea.
    pub down: Vec<u8>,
    /// Rain that passes through each cell (its own plus all that comes from above), land only.
    pub flow: Vec<f32>,
    /// The level the ground would have if its basins were full (elevation units).
    pub filled: Vec<f32>,
    pub lake: Vec<bool>,
    pub river: Vec<bool>,
    pub max_flow: f32,
}

/// The neighbour of cell `(x, y)` in direction `d` (wrapped in longitude), None past a pole.
pub fn neighbour(x: usize, y: usize, d: u8) -> Option<(usize, usize)> {
    let (dx, dy) = DIRS[usize::from(d)];
    let ny = y as i32 + dy;
    if !(0..H as i32).contains(&ny) {
        return None;
    }
    Some(((x as i32 + dx).rem_euclid(W as i32) as usize, ny as usize))
}

impl Hydro {
    pub fn empty() -> Hydro {
        Hydro {
            down: vec![NO_DOWN; W * H],
            flow: vec![0.0; W * H],
            filled: vec![0.0; W * H],
            lake: vec![false; W * H],
            river: vec![false; W * H],
            max_flow: 0.0,
        }
    }

    /// Compute the water of a world from its elevation (negative = sea) and humidity.
    pub fn compute(elev: &[f32], hum: &[f32]) -> Hydro {
        let mut h = Hydro::empty();
        h.filled.copy_from_slice(elev);
        if !elev.iter().any(|&e| e < 0.0) {
            return h;
        }
        // Priority flood from the sea: the lowest frontier cell first, so that every cell is
        // reached from the lowest way out of its basin.
        let key = |e: f32| (f64::from(e) * 1.0e7) as i64;
        let mut seen = vec![false; W * H];
        let mut heap: BinaryHeap<Reverse<(i64, u32)>> = BinaryHeap::new();
        for (i, &e) in elev.iter().enumerate() {
            if e < 0.0 {
                seen[i] = true;
                heap.push(Reverse((key(e), i as u32)));
            }
        }
        let mut order: Vec<usize> = Vec::with_capacity(W * H);
        while let Some(Reverse((_, i))) = heap.pop() {
            let i = i as usize;
            order.push(i);
            let (x, y) = (i % W, i / W);
            for d in 0..8u8 {
                let Some((nx, ny)) = neighbour(x, y, d) else {
                    continue;
                };
                let n = ny * W + nx;
                if seen[n] {
                    continue;
                }
                seen[n] = true;
                h.filled[n] = elev[n].max(h.filled[i] + EPS);
                // The neighbour drains toward `i`: the opposite direction.
                h.down[n] = (d + 4) % 8;
                heap.push(Reverse((key(h.filled[n]), n as u32)));
            }
        }
        // Rain accumulates downhill: from the highest filled cell to the lowest.
        for (i, f) in h.flow.iter_mut().enumerate() {
            if elev[i] >= 0.0 {
                *f = 0.02 + hum[i].powf(1.2);
            }
        }
        for &i in order.iter().rev() {
            if h.down[i] == NO_DOWN || elev[i] < 0.0 {
                continue;
            }
            if let Some((nx, ny)) = neighbour(i % W, i / W, h.down[i]) {
                let n = ny * W + nx;
                if elev[n] >= 0.0 {
                    h.flow[n] += h.flow[i];
                }
            }
        }
        h.max_flow = h.flow.iter().copied().fold(0.0, f32::max);
        #[allow(clippy::needless_range_loop)]
        for i in 0..W * H {
            if elev[i] < 0.0 {
                continue;
            }
            let basin = h.filled[i] - elev[i] > LAKE_DEPTH;
            h.lake[i] = basin && h.flow[i] >= LAKE_FLOW;
            h.river[i] = !h.lake[i] && h.flow[i] >= RIVER_FLOW;
        }
        h
    }

    /// Where the water of a cell goes next (None for the sea and for a world without sea).
    pub fn next(&self, x: usize, y: usize) -> Option<(usize, usize)> {
        let d = self.down[y * W + x];
        if d == NO_DOWN {
            None
        } else {
            neighbour(x, y, d)
        }
    }

    /// Size of the river of a cell compared with the biggest of the world, 0..1 (square root:
    /// a river's width grows slower than its flow).
    pub fn rel(&self, i: usize) -> f32 {
        if self.max_flow <= 0.0 {
            0.0
        } else {
            (self.flow[i] / self.max_flow).sqrt().clamp(0.0, 1.0)
        }
    }

    /// Distance (in cells) from the point `(fx, fy)` (cell centres at integer coordinates) to
    /// the nearest river, and the relative size of that river. Each river runs as a straight
    /// segment from the centre of its cell to the centre of the next one.
    pub fn river_near(&self, fx: f64, fy: f64) -> Option<(f64, f32)> {
        let (cx, cy) = (fx.round() as i64, fy.round() as i64);
        let mut best: Option<(f64, f32)> = None;
        for oy in -1..=1i64 {
            let y = cy + oy;
            if !(0..H as i64).contains(&y) {
                continue;
            }
            for ox in -1..=1i64 {
                let x = (cx + ox).rem_euclid(W as i64) as usize;
                let i = y as usize * W + x;
                if !self.river[i] {
                    continue;
                }
                let d = self.down[i];
                if d == NO_DOWN {
                    continue;
                }
                // The cell's centre relative to the point, wrapped in longitude.
                let mut rx = x as f64 - fx;
                if rx > W as f64 / 2.0 {
                    rx -= W as f64;
                } else if rx < -(W as f64) / 2.0 {
                    rx += W as f64;
                }
                let ry = y as f64 - fy;
                let (dx, dy) = DIRS[usize::from(d)];
                let (sx, sy) = (f64::from(dx), f64::from(dy));
                // Distance from the origin (the point) to the segment (rx, ry) + t (sx, sy).
                let len2 = sx * sx + sy * sy;
                let t = (-(rx * sx + ry * sy) / len2).clamp(0.0, 1.0);
                let (px, py) = (rx + t * sx, ry + t * sy);
                let dist = (px * px + py * py).sqrt();
                if best.is_none_or(|(b, _)| dist < b) {
                    best = Some((dist, self.rel(i)));
                }
            }
        }
        best
    }

    /// How much of a lake is at `(fx, fy)`: the lake cells blended between their centres, 0..1.
    pub fn lake_at(&self, fx: f64, fy: f64) -> f32 {
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = ((fx - x0) as f32, (fy - y0) as f32);
        let at = |x: f64, y: f64| -> f32 {
            let yi = (y as i64).clamp(0, H as i64 - 1) as usize;
            let xi = (x as i64).rem_euclid(W as i64) as usize;
            f32::from(self.lake[yi * W + xi])
        };
        let a = at(x0, y0) + (at(x0 + 1.0, y0) - at(x0, y0)) * tx;
        let b = at(x0, y0 + 1.0) + (at(x0 + 1.0, y0 + 1.0) - at(x0, y0 + 1.0)) * tx;
        a + (b - a) * ty
    }

    /// Level of the filled ground at a point, bilinear (what a lake's surface is made of).
    pub fn filled_at(&self, fx: f64, fy: f64) -> f32 {
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = ((fx - x0) as f32, (fy - y0) as f32);
        let at = |x: f64, y: f64| -> f32 {
            let yi = (y as i64).clamp(0, H as i64 - 1) as usize;
            let xi = (x as i64).rem_euclid(W as i64) as usize;
            self.filled[yi * W + xi]
        };
        let a = at(x0, y0) + (at(x0 + 1.0, y0) - at(x0, y0)) * tx;
        let b = at(x0, y0 + 1.0) + (at(x0 + 1.0, y0 + 1.0) - at(x0, y0 + 1.0)) * tx;
        a + (b - a) * ty
    }
}

/// Half the width of a river of relative size `rel`, in metres on the ground.
pub fn river_half_width_m(rel: f32) -> f64 {
    3.0 + 15.0 * f64::from(rel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::{self, Surface};
    use crate::PlanetKind;

    fn world(seed: u64, kind: PlanetKind) -> Surface {
        surface::generate(seed, kind, 285.0).expect("solid")
    }

    #[test]
    fn a_river_always_goes_down_and_ends_in_the_sea_or_a_lake() {
        for seed in [3, 5, 11, 42] {
            let s = world(seed, PlanetKind::Terran);
            let h = &s.hydro;
            let rivers: Vec<usize> = (0..W * H).filter(|&i| h.river[i]).collect();
            assert!(
                !rivers.is_empty(),
                "seed {seed} has rivers: max {} sea {}",
                h.max_flow,
                s.sea_frac
            );
            for &start in &rivers {
                let (mut x, mut y) = (start % W, start / W);
                let mut steps = 0;
                loop {
                    let i = y * W + x;
                    match h.next(x, y) {
                        None => {
                            assert!(s.is_sea(x, y), "seed {seed}: the water ends in the sea");
                            break;
                        }
                        Some((nx, ny)) => {
                            assert!(
                                h.filled[ny * W + nx] < h.filled[i],
                                "seed {seed}: the water only goes down"
                            );
                            // Cells downstream of a river are a river, a lake or the sea.
                            let n = ny * W + nx;
                            assert!(h.river[n] || h.lake[n] || s.elev[n] < 0.0);
                            (x, y) = (nx, ny);
                        }
                    }
                    steps += 1;
                    assert!(steps < W * H, "no loop");
                }
            }
        }
    }

    #[test]
    fn rivers_grow_downstream_and_lakes_sit_in_basins() {
        let s = world(5, PlanetKind::Terran);
        let h = &s.hydro;
        #[allow(clippy::needless_range_loop)]
        for i in 0..W * H {
            if h.river[i] {
                if let Some((nx, ny)) = h.next(i % W, i / W) {
                    let n = ny * W + nx;
                    assert!(
                        s.elev[n] < 0.0 || h.flow[n] >= h.flow[i],
                        "more water downstream"
                    );
                }
            }
            if h.lake[i] {
                assert!(
                    h.filled[i] - s.elev[i] > LAKE_DEPTH,
                    "a lake lies in a hollow"
                );
                assert!(s.elev[i] >= 0.0 && !h.river[i]);
            }
        }
        // Not everywhere and not nowhere.
        let land = (0..W * H).filter(|&i| s.elev[i] >= 0.0).count();
        let rivers = (0..W * H).filter(|&i| h.river[i]).count();
        assert!(
            rivers * 100 > land && rivers * 4 < land,
            "{rivers} of {land}"
        );
    }

    #[test]
    fn worlds_without_sea_have_no_water_and_the_same_seed_the_same_rivers() {
        let moon = world(7, PlanetKind::Barren);
        assert!(moon.hydro.river.iter().all(|r| !r) && moon.hydro.lake.iter().all(|l| !l));
        let a = world(9, PlanetKind::Terran);
        let b = world(9, PlanetKind::Terran);
        assert_eq!(a.hydro.river, b.hydro.river);
        assert_eq!(a.hydro.down, b.hydro.down);
        assert_ne!(a.hydro.river, world(10, PlanetKind::Terran).hydro.river);
    }

    #[test]
    fn some_basins_hold_lakes_on_some_worlds() {
        let n: usize = (1..12u64)
            .map(|seed| {
                world(seed, PlanetKind::Terran)
                    .hydro
                    .lake
                    .iter()
                    .filter(|&&l| l)
                    .count()
            })
            .sum();
        assert!(n > 0, "lakes exist");
    }

    #[test]
    fn the_river_line_runs_through_its_cells_and_wraps_around_the_date_line() {
        let s = world(5, PlanetKind::Terran);
        let h = &s.hydro;
        let i = (0..W * H).find(|&i| h.river[i]).expect("a river");
        let (x, y) = (i % W, i / W);
        let (d, rel) = h.river_near(x as f64, y as f64).expect("on it");
        assert!(d < 1e-9 && (0.0..=1.0).contains(&rel));
        // A river on the last column flowing east crosses the seam into column 0.
        let mut h2 = Hydro::empty();
        h2.river[W - 1] = true;
        h2.down[W - 1] = 0;
        let (d, _) = h2.river_near(-0.5, 0.0).expect("across the seam");
        assert!(d <= 0.5 + 1e-9);
        assert!(h2.river_near(40.0, 0.0).is_none());
    }
}
