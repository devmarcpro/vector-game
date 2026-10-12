//! Incidents of the companies: a convoy hijacked, a strike, a record profit, a scandal, a big
//! contract, a fire. They happen in a sector (the galaxy is infinite: companies have branches
//! everywhere), last a few weeks, and have real effects: the goods of the company get scarcer or
//! more plentiful in that sector, and the share price follows what happens in the sectors around
//! the company's headquarters. Pure functions of the seed, the sector and the date.
use crate::bourse::{ActiveEvent, DAY, WEEK};
use crate::planet::unit;
use crate::politics::{cell_center, cell_of, cells_around, Cell};
use vector_core::hash_coord;
use vector_data::GameData;

/// Chance per week, per company and per sector, of an incident.
pub const P_INCIDENT: f64 = 0.003;
/// How long incidents last at most, in weeks (for the look-back).
const LOOKBACK_WEEKS: i64 = 5;

#[derive(Clone, Debug, PartialEq)]
pub struct Incident {
    pub company: usize,
    pub cell: Cell,
    pub kind: usize,
    pub start: f64,
    pub end: f64,
    /// 0.6 .. 1.3, how hard it hits.
    pub strength: f64,
}

impl Incident {
    /// Share of the full effect at time `t` (ramp, plateau, fade).
    pub fn profile(&self, t: f64) -> f64 {
        ActiveEvent {
            def: 0,
            start: self.start,
            end: self.end,
            global: false,
            strength: self.strength,
        }
        .profile(t)
    }

    /// Effect on the company's share price at time `t` (the kinds are in `data/incidents.ron`).
    pub fn effect(&self, data: &GameData, t: f64) -> f64 {
        data.incidents[self.kind].share * self.strength * self.profile(t)
    }
}

/// The incidents of sector `c` that are running at some time in [t0, t1].
pub fn incidents_in_cell(data: &GameData, seed: u64, c: Cell, t0: f64, t1: f64) -> Vec<Incident> {
    let mut out = Vec::new();
    let (w0, w1) = (
        (t0 / WEEK).floor() as i64 - LOOKBACK_WEEKS,
        (t1 / WEEK).floor() as i64,
    );
    for company in 0..data.companies.len() {
        for w in w0..=w1 {
            let h = |k: i64| {
                hash_coord(
                    seed ^ 0x1C1D ^ (company as u64).wrapping_mul(0x9E37_79B9),
                    w,
                    c[0] * 7919 + c[1] * 104_729 + c[2] * 1_299_709,
                    k,
                )
            };
            if unit(h(0)) >= P_INCIDENT {
                continue;
            }
            let start = w as f64 * WEEK + unit(h(1)) * WEEK;
            let end = start + (14.0 + 16.0 * unit(h(2))) * DAY;
            if end >= t0 && start <= t1 {
                out.push(Incident {
                    company,
                    cell: c,
                    kind: (h(3) % data.incidents.len() as u64) as usize,
                    start,
                    end,
                    strength: 0.6 + 0.7 * unit(h(4)),
                });
            }
        }
    }
    out
}

/// Multiplier on the share price of `company` from what goes on in the sectors around its
/// headquarters (at `hq`).
pub fn share_factor(data: &GameData, seed: u64, company: usize, hq: [f64; 3], t: f64) -> f64 {
    let mut f = 1.0;
    for c in cells_around(cell_of(hq), 1) {
        for inc in incidents_in_cell(data, seed, c, t, t) {
            if inc.company == company {
                f *= 1.0 + inc.effect(data, t);
            }
        }
    }
    f
}

/// Multiplier on the price of each resource in the sector of `pos`: the goods of a company that
/// suffers an incident there get scarcer (dearer), those of a company in a boom plentiful.
pub fn goods_factors(data: &GameData, seed: u64, pos: [f64; 3], t: f64) -> Vec<f64> {
    let mut f = vec![1.0; data.resources.len()];
    for inc in incidents_in_cell(data, seed, cell_of(pos), t, t) {
        let k = &data.companies[inc.company];
        let e = inc.effect(data, t);
        for (res, w) in &k.goods {
            if let Some(r) = data.resource_index(res) {
                f[r] *= 1.0 - 0.8 * e * w;
            }
        }
    }
    f
}

/// Where an incident took place, for the news: the middle of its sector.
pub fn place(inc: &Incident) -> [f64; 3] {
    cell_center(inc.cell)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn data() -> Arc<GameData> {
        crate::test_support::data()
    }

    #[test]
    fn incidents_are_deterministic_local_and_move_prices_where_they_happen() {
        let d = data();
        let mut found = None;
        'seek: for c in cells_around([0, 0, 0], 3) {
            for w in 0..400 {
                let t = f64::from(w) * 7.0 * DAY;
                if let Some(i) = incidents_in_cell(&d, 7, c, t, t + 7.0 * DAY)
                    .into_iter()
                    .next()
                {
                    found = Some(i);
                    break 'seek;
                }
            }
        }
        let inc = found.expect("an incident near the origin within 8 years");
        assert_eq!(
            incidents_in_cell(&d, 7, inc.cell, inc.start, inc.start),
            incidents_in_cell(&d, 7, inc.cell, inc.start, inc.start),
            "deterministic"
        );
        assert!(inc.end - inc.start >= 14.0 * DAY && inc.end - inc.start <= 30.0 * DAY);
        let mid = (inc.start + inc.end) / 2.0;
        assert!(inc.profile(mid) > 0.99 && inc.profile(inc.end + DAY) == 0.0);
        // The goods of the company are dearer in the sector when it is hit, cheaper in a boom.
        let pos = cell_center(inc.cell);
        let f = goods_factors(&d, 7, pos, mid);
        let k = &d.companies[inc.company];
        let r = d.resource_index(&k.goods[0].0).unwrap();
        if d.incidents[inc.kind].share < 0.0 {
            assert!(f[r] > 1.0, "scarcity after a blow: {}", f[r]);
        } else {
            assert!(f[r] < 1.0, "plenty in a boom: {}", f[r]);
        }
        // The next sector over does not feel it.
        let far = cell_center([inc.cell[0] + 5, inc.cell[1], inc.cell[2]]);
        let g = goods_factors(&d, 7, far, mid);
        assert!((g[r] - f[r]).abs() > 1e-12 || f[r] == 1.0);
        assert!(
            incidents_in_cell(&d, 7, [inc.cell[0] + 5, inc.cell[1], inc.cell[2]], mid, mid)
                .iter()
                .all(|x| x != &inc)
        );
        // The infinite galaxy has incidents everywhere.
        let n: usize = (0..200)
            .map(|w| {
                incidents_in_cell(
                    &d,
                    7,
                    [90_000, 1, -5],
                    f64::from(w) * 7.0 * DAY,
                    f64::from(w) * 7.0 * DAY,
                )
                .len()
            })
            .sum();
        assert!(n > 0, "far sectors have their own incidents");
    }

    #[test]
    fn share_prices_follow_the_incidents_around_the_headquarters() {
        let d = data();
        let hq = [10.0, 5.0, -20.0];
        let (mut up, mut down) = (0, 0);
        for c in 0..d.companies.len() {
            for w in 0..600 {
                let f = share_factor(&d, 7, c, hq, f64::from(w) * 7.0 * DAY);
                assert!(f > 0.5 && f < 1.6, "{f}");
                if f > 1.01 {
                    up += 1;
                } else if f < 0.99 {
                    down += 1;
                }
            }
        }
        assert!(up > 5 && down > 5, "{up} up, {down} down");
    }
}
