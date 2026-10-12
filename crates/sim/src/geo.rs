//! Geology in play: what the plates, the volcanoes and the ore bodies of a world (see
//! `vector_world::geology`) do for the people who settle on it. A colony founded near a deposit
//! finds its ore in the ground (the deposit yields of `data/geology.ron` are added to the
//! abundances of the site); a colony on hot ground gets more from a geothermal plant; the
//! catastrophes of `planet.rs` strike the faults and the volcanoes of the map.
use crate::planet::{build_map, city_site_in, PlanetMap};
use crate::Sim;
use vector_data::GameData;
use vector_world::geology::{Deposit, DepositKind, Rock};

/// A deposit this close to the city (in map cells) is in reach of the colony.
pub const SITE_REACH_CELLS: f64 = 12.0;
/// Abundance (relative to the other ores of the ground, which add up to about 1.5) that a deposit
/// of richness 1 right next to the city adds to each of its resources, per unit of yield share.
pub const GEO_ABUNDANCE: f64 = 0.04;
/// Heat of the ground (see `Geology::heat_at`) from which a site is tagged hot, warm or cold.
pub const HOT: f32 = 0.8;
pub const WARM: f32 = 0.4;
pub const COLD: f32 = 0.2;

/// What the geology of a site gives to the colony founded on it.
#[derive(Clone, Debug, PartialEq)]
pub struct SiteGeology {
    /// Extra site tags: `geo_hot`, `geo_warm` or `geo_cold` (it drives geothermal plants).
    pub tags: Vec<String>,
    /// Extra abundances (resource index, relative abundance) from the deposits in reach.
    pub deposits: Vec<(usize, f64)>,
    pub heat: f32,
    pub rock: Rock,
    /// Deposits in reach, nearest first, with their distance in cells.
    pub in_reach: Vec<(DepositKind, f64)>,
}

/// The geology around `city` on `map`.
pub fn site_geology(data: &GameData, map: &PlanetMap, city: (usize, usize)) -> SiteGeology {
    let geo = &map.surface.geo;
    let heat = geo.heat_at(city.0, city.1);
    let tag = if heat >= HOT {
        "geo_hot"
    } else if heat >= WARM {
        "geo_warm"
    } else if heat < COLD {
        "geo_cold"
    } else {
        ""
    };
    let near: Vec<Deposit> = geo.deposits_near(city, SITE_REACH_CELLS);
    let mut deposits: Vec<(usize, f64)> = Vec::new();
    let mut in_reach = Vec::new();
    for d in &near {
        let dist = vector_world::geology::cell_dist(city, (d.x, d.y));
        in_reach.push((d.kind, dist));
        let Some(def) = data.geo_def(d.kind.id()) else {
            continue;
        };
        let fall = 1.0 - 0.5 * dist / SITE_REACH_CELLS;
        for (rid, share) in &def.yields {
            if let Some(r) = data.resource_index(rid) {
                add(
                    &mut deposits,
                    r,
                    GEO_ABUNDANCE * share * f64::from(d.richness) * fall,
                );
            }
        }
    }
    SiteGeology {
        tags: if tag.is_empty() {
            Vec::new()
        } else {
            vec![tag.to_string()]
        },
        deposits,
        heat,
        rock: geo.rock_at(city.0, city.1),
        in_reach,
    }
}

fn add(v: &mut Vec<(usize, f64)>, r: usize, x: f64) {
    match v.iter_mut().find(|(i, _)| *i == r) {
        Some(e) => e.1 += x,
        None => v.push((r, x)),
    }
}

/// Add the abundances of the geology to those of the ground.
pub fn merge_deposits(ground: &mut Vec<(usize, f64)>, extra: &[(usize, f64)]) {
    for &(r, x) in extra {
        add(ground, r, x);
    }
}

/// A deposit this close to the landing cell (in map cells) is under the pick of the squad.
pub const VEIN_REACH_CELLS: f64 = 4.0;
/// Ore (kg) a hit of the pick turns up in a vein of richness 1, per unit of yield share.
pub const VEIN_KG: f64 = 8.0;

/// The ore bodies under the pick of a squad landed on `cell` of `map`, as (resource, kg, chance
/// per hit) of the same yields as `site_geology` gives a colony. `scan` is what the scanner
/// reads of the ground (the analysis budget of the mind, 1 = even): it sharpens the chance.
pub fn veins_at(
    data: &GameData,
    map: &PlanetMap,
    cell: (usize, usize),
    scan: f64,
) -> Vec<(usize, f64, f64)> {
    let mut out: Vec<(usize, f64, f64)> = Vec::new();
    for d in map.surface.geo.deposits_near(cell, VEIN_REACH_CELLS) {
        let dist = vector_world::geology::cell_dist(cell, (d.x, d.y));
        let Some(def) = data.geo_def(d.kind.id()) else {
            continue;
        };
        let fall = 1.0 - dist / (VEIN_REACH_CELLS + 1.0);
        for (rid, share) in &def.yields {
            let Some(r) = data.resource_index(rid) else {
                continue;
            };
            let richness = f64::from(d.richness);
            let kg = VEIN_KG * (0.5 + share) * richness;
            let chance = (0.9 * richness * fall * (3.0 * share).min(1.0) * scan.clamp(0.5, 1.5))
                .clamp(0.0, 0.9);
            match out.iter_mut().find(|v| v.0 == r) {
                Some(v) => {
                    v.1 += kg;
                    v.2 = (v.2 + chance).min(0.95);
                }
                None => out.push((r, kg, chance)),
            }
        }
    }
    out
}

/// What a geothermal plant yields on a site with these tags, relative to its rated power.
pub fn geothermal_factor(tags: &[String]) -> f64 {
    let has = |t: &str| tags.iter().any(|x| x == t);
    if has("geo_hot") {
        2.0
    } else if has("geo_warm") {
        1.2
    } else if has("geo_cold") {
        0.4
    } else {
        1.0
    }
}

impl Sim {
    /// The map and the geology of the site where a colony `id` would be founded on `body`.
    pub(crate) fn founding_site(&self, body: usize, id: u32) -> Option<(SiteGeology, PlanetMap)> {
        let map = build_map(&self.data, &self.system, body)?;
        let mut ids: Vec<u32> = self
            .colonies
            .iter()
            .filter(|c| c.star == self.here && c.body == body)
            .map(|c| c.id)
            .collect();
        ids.push(id);
        let city = city_site_in(&self.data, &map, &ids, id)?;
        Some((site_geology(&self.data, &map, city), map))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vector_world::surface::{H, W};
    use vector_world::{generate_system, BodyKind, PlanetKind};

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    /// A temperate world with plates, from the first seeds that have one.
    fn living_world(d: &GameData) -> PlanetMap {
        for seed in 0..400u64 {
            let sys = generate_system(seed);
            for (i, b) in sys.bodies.iter().enumerate() {
                if matches!(b.kind, BodyKind::Planet(PlanetKind::Terran)) {
                    if let Some(m) = build_map(d, &sys, i) {
                        if !m.surface.geo.deposits.is_empty() {
                            return m;
                        }
                    }
                }
            }
        }
        panic!("no living world among 400 systems");
    }

    #[test]
    fn a_site_next_to_a_deposit_holds_its_ore_and_a_site_far_from_any_does_not() {
        let d = data();
        let map = living_world(&d);
        let dep = map.surface.geo.deposits[0];
        let here = site_geology(&d, &map, (dep.x, dep.y));
        assert!(here.in_reach.iter().any(|(k, _)| *k == dep.kind));
        assert!(!here.deposits.is_empty());
        let def = d.geo_def(dep.kind.id()).expect("def");
        for (rid, _) in &def.yields {
            let r = d.resource_index(rid).expect("res");
            assert!(here.deposits.iter().any(|(i, x)| *i == r && *x > 0.0));
        }
        // A cell far from every deposit gets nothing from them.
        let far = (0..H * W)
            .map(|k| (k % W, k / W))
            .find(|&c| {
                map.surface
                    .geo
                    .deposits
                    .iter()
                    .all(|q| vector_world::geology::cell_dist(c, (q.x, q.y)) > SITE_REACH_CELLS)
            })
            .expect("a far cell");
        assert!(site_geology(&d, &map, far).deposits.is_empty());
        // The yield scales with richness: the same deposit twice as rich gives twice the ore.
        let mut rich = map.clone();
        rich.surface.geo.deposits = vec![Deposit {
            richness: 1.0,
            ..dep
        }];
        let mut poor = map.clone();
        poor.surface.geo.deposits = vec![Deposit {
            richness: 0.5,
            ..dep
        }];
        let c = (dep.x, dep.y);
        let (a, b) = (site_geology(&d, &rich, c), site_geology(&d, &poor, c));
        assert!((a.deposits[0].1 / b.deposits[0].1 - 2.0).abs() < 1e-9);
    }

    #[test]
    fn hot_ground_gets_more_from_a_geothermal_plant() {
        let d = data();
        let map = living_world(&d);
        let v = map
            .surface
            .geo
            .volcanoes
            .iter()
            .find(|v| v.active)
            .expect("active volcano");
        let hot = site_geology(&d, &map, (v.x, v.y));
        assert!(hot.heat >= HOT && hot.tags == vec!["geo_hot".to_string()]);
        // Find the coldest cell.
        let cold = (0..W * H)
            .map(|k| (k % W, k / W))
            .min_by(|a, b| {
                map.surface
                    .geo
                    .heat_at(a.0, a.1)
                    .total_cmp(&map.surface.geo.heat_at(b.0, b.1))
            })
            .expect("cell");
        let c = site_geology(&d, &map, cold);
        assert!(geothermal_factor(&hot.tags) > geothermal_factor(&c.tags));
        assert_eq!(geothermal_factor(&[]), 1.0, "old colonies keep their power");
        assert!(geothermal_factor(&["geo_warm".to_string()]) > 1.0);
    }

    #[test]
    fn merging_adds_abundance_to_an_existing_resource_and_creates_the_missing_ones() {
        let mut ground = vec![(0usize, 0.3)];
        merge_deposits(&mut ground, &[(0, 0.1), (5, 0.2)]);
        assert_eq!(ground.len(), 2);
        assert!((ground[0].1 - 0.4).abs() < 1e-12);
    }
}
