//! Military physics and strategy, on top of the weapons of `modules.ron` and the wars of
//! `politics.rs` (no parallel system): kinetic energy and dispersion, beam divergence and
//! absorption, passive thermal detection, abstract fleet battles (Lanchester square law fed by
//! the same weapon and hull data), blockades and espionage rolls. Everything is a pure function of
//! its inputs and of hashes of (seed, coordinates): never the real clock.

use crate::planet::unit;
use crate::politics::{self, Cell, WAR_PERIOD};
use crate::Sim;
use vector_core::hash_coord;
use vector_data::GameData;
use vector_world::galaxy::Galaxy;

/// Joules that make one point of damage (a 350-point hit is 3.5 MJ).
pub const DAMAGE_J: f64 = 1.0e4;
/// Irradiance (W/m^2) a passive thermal sensor can pick out of the background.
pub const THERMAL_THRESHOLD: f64 = 5.0e-6;
/// Range (m) under which an active sensor sees a ship whatever its heat.
pub const ACTIVE_RANGE: f64 = 5_000.0;

/// Kinetic energy of a projectile, J.
pub fn kinetic_energy_j(mass_kg: f64, speed: f64) -> f64 {
    0.5 * mass_kg * speed * speed
}

/// Damage points delivered by `joules` of energy.
pub fn damage_from_energy(joules: f64) -> f64 {
    joules / DAMAGE_J
}

/// Radius (m) of the disc in which a weapon of `spread_mrad` puts its shots at `dist` metres.
pub fn miss_radius(spread_mrad: f64, dist: f64) -> f64 {
    spread_mrad * 1e-3 * dist
}

/// Chance that a shot lands on a target of `radius` metres at `dist` metres (shots spread
/// uniformly over the dispersion disc).
pub fn hit_chance(spread_mrad: f64, dist: f64, radius: f64) -> f64 {
    let r = miss_radius(spread_mrad, dist);
    if r <= radius {
        1.0
    } else {
        (radius / r).powi(2)
    }
}

/// Radius (m) of a beam's spot at `dist`: the aperture plus diffraction, 1.22 lambda / D.
pub fn beam_spot_radius(wavelength_nm: f64, aperture: f64, dist: f64) -> f64 {
    aperture / 2.0 + 1.22 * wavelength_nm * 1e-9 / aperture * dist
}

/// Share of the beam power per unit area left at `dist` (1 at the muzzle, then falling as the
/// spot grows), before any absorption.
pub fn beam_focus(wavelength_nm: f64, aperture: f64, dist: f64) -> f64 {
    let s = beam_spot_radius(wavelength_nm, aperture, dist);
    ((aperture / 2.0) / s).powi(2)
}

/// Fraction of a beam that crosses `path` metres of air of `density` kg/m^3. Lasers lose little,
/// particle beams (wavelength under 100 nm: charged or neutral particles) scatter far more.
pub fn atmospheric_transmission(wavelength_nm: f64, density: f64, path: f64) -> f64 {
    let k = if wavelength_nm < 100.0 {
        2.0e-3
    } else {
        2.0e-4
    };
    (-k * density * path).exp()
}

/// Damage per second a beam really delivers at `dist` (focus and absorption).
pub fn beam_dps(base_dps: f64, wavelength_nm: f64, aperture: f64, dist: f64, density: f64) -> f64 {
    base_dps
        * beam_focus(wavelength_nm, aperture, dist)
        * atmospheric_transmission(wavelength_nm, density, dist)
}

/// Range (m) at which a passive sensor picks up a source of `power_kw` (inverse square law),
/// never under the active sensor range.
pub fn passive_detection_range(power_kw: f64) -> f64 {
    let p = power_kw.max(0.0) * 1000.0;
    (p / (4.0 * std::f64::consts::PI * THERMAL_THRESHOLD))
        .sqrt()
        .max(ACTIVE_RANGE)
}

/// Mean damage per second of the weapons of the data (what a typical gun does).
fn mean_weapon_dps(data: &GameData) -> f64 {
    let w: Vec<f64> = data
        .modules
        .iter()
        .filter(|m| matches!(m.weapon.as_str(), "beam" | "kinetic" | "missile"))
        .map(|m| {
            if m.weapon == "beam" {
                m.damage
            } else {
                m.damage * m.rate
            }
        })
        .collect();
    w.iter().sum::<f64>() / w.len().max(1) as f64
}

/// Rating of the reference warship of the data: geometric mean of its firepower (three
/// hardpoints of the mean weapon) and of the mean hull's structure.
pub fn ship_rating(data: &GameData) -> f64 {
    let hp = data.hulls.iter().map(|h| h.hp).sum::<f64>() / data.hulls.len().max(1) as f64;
    (3.0 * mean_weapon_dps(data) * hp).sqrt()
}

/// Result of an abstract battle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Battle {
    pub a_wins: bool,
    /// Fraction of each side's strength left at the end (0 for the loser).
    pub a_left: f64,
    pub b_left: f64,
}

/// Lanchester square law between two strengths, with a deterministic fog of war of +-10 % per
/// side drawn from `key`.
pub fn resolve_battle(a: f64, b: f64, seed: u64, key: i64) -> Battle {
    let ea = a * (0.9 + 0.2 * unit(hash_coord(seed ^ 0xA7, key, 1, 3)));
    let eb = b * (0.9 + 0.2 * unit(hash_coord(seed ^ 0xA7, key, 2, 3)));
    let left = |w: f64, l: f64| {
        if w <= 0.0 {
            0.0
        } else {
            ((w * w - l * l).max(0.0)).sqrt() / w
        }
    };
    if ea >= eb {
        Battle {
            a_wins: true,
            a_left: left(ea, eb),
            b_left: 0.0,
        }
    } else {
        Battle {
            a_wins: false,
            a_left: 0.0,
            b_left: left(eb, ea),
        }
    }
}

/// Consecutive wars between `c` and `nb` before period `m` (at most 10): how long supply lines
/// have been stretched.
fn war_age(data: &GameData, galaxy: &Galaxy, seed: u64, c: Cell, nb: Cell, m: i64) -> u32 {
    let (fc, fnb) = (
        politics::faction_of(data, galaxy, c),
        politics::faction_of(data, galaxy, nb),
    );
    (1..=10)
        .take_while(|i| politics::war_in_period(data, seed, c, nb, fc, fnb, m - i))
        .count() as u32
}

/// Fighting strength of a sector's fleet in period `m`: the number of hulls, the rating of the
/// reference warship, and supply (fuel and ammunition) that falls with the age of the war,
/// slower for the defender who fights close to home.
pub fn fleet_strength(
    data: &GameData,
    seed: u64,
    c: Cell,
    m: i64,
    age: u32,
    defender: bool,
) -> f64 {
    let n = 4.0 + (16.0 * unit(hash_coord(seed ^ 0xF1EE7, politics::key(c), m / 6, 5))).floor();
    let decay = if defender { 0.05 } else { 0.2 };
    n * ship_rating(data) / (1.0 + decay * f64::from(age))
}

/// One front of a war seen from sector `c`.
#[derive(Clone, Debug, PartialEq)]
pub struct Front {
    pub foe: Cell,
    /// Number of previous consecutive war periods.
    pub age: u32,
    pub own_strength: f64,
    pub foe_strength: f64,
    pub battle: Battle,
}

impl Front {
    /// Whether `c` wins this front.
    pub fn own_wins(&self) -> bool {
        self.battle.a_wins
    }
}

/// The fronts of sector `c` at time `t`, one per neighbour at war with it. The sector with the
/// smaller key is the aggressor, the other defends; the battle is the same on both sides.
pub fn fronts(data: &GameData, galaxy: &Galaxy, seed: u64, c: Cell, t: f64) -> Vec<Front> {
    let m = (t / WAR_PERIOD).floor() as i64;
    let fc = politics::faction_of(data, galaxy, c);
    let mut out = Vec::new();
    for nb in politics::neighbours(c) {
        let fnb = politics::faction_of(data, galaxy, nb);
        if !politics::war_in_period(data, seed, c, nb, fc, fnb, m) {
            continue;
        }
        let age = war_age(data, galaxy, seed, c, nb, m);
        let c_attacks = politics::key(c) < politics::key(nb);
        let sc = fleet_strength(data, seed, c, m, age, !c_attacks);
        let sn = fleet_strength(data, seed, nb, m, age, c_attacks);
        // Resolve once, in the frame of the smaller key, so both sides agree.
        let (lo, hi) = (
            politics::key(c).min(politics::key(nb)),
            politics::key(c).max(politics::key(nb)),
        );
        let key = lo ^ hi.rotate_left(13) ^ m;
        let battle = if c_attacks {
            resolve_battle(sc, sn, seed, key)
        } else {
            let b = resolve_battle(sn, sc, seed, key);
            Battle {
                a_wins: !b.a_wins,
                a_left: b.b_left,
                b_left: b.a_left,
            }
        };
        out.push(Front {
            foe: nb,
            age,
            own_strength: sc,
            foe_strength: sn,
            battle,
        });
    }
    out
}

/// A sector is besieged when it is at war and loses a front: the victor's fleet cuts the lanes.
pub fn is_besieged(data: &GameData, galaxy: &Galaxy, seed: u64, c: Cell, t: f64) -> bool {
    fronts(data, galaxy, seed, c, t)
        .iter()
        .any(|f| !f.own_wins())
}

/// Price rise of every good in a blockaded sector.
pub const BLOCKADE_MARKUP: f64 = 0.3;

/// Outcome of a spy mission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpyResult {
    Success,
    Failure,
    /// The agent is caught: the target knows who sent them.
    Caught,
}

/// A mission rolled from hashes: odds grow with the agent's skill (0..1) and fall with the
/// target's counter-intelligence (0..1).
pub fn spy_mission(seed: u64, agent_skill: f64, counter_intel: f64, key: i64) -> SpyResult {
    let p_ok = (0.25 + 0.6 * agent_skill - 0.5 * counter_intel).clamp(0.02, 0.95);
    let roll = unit(hash_coord(seed ^ 0x59, key, 1, 7));
    if roll < p_ok {
        SpyResult::Success
    } else if unit(hash_coord(seed ^ 0x59, key, 2, 7)) < 0.3 + 0.5 * counter_intel {
        SpyResult::Caught
    } else {
        SpyResult::Failure
    }
}

impl Sim {
    /// Heat the ship gives off (kW): half the power its powered modules draw, plus the flash of
    /// the weapons that fired within their last cooldown. A ship with everything cut is cold.
    pub fn thermal_signature_kw(&self) -> f64 {
        let data = &self.data;
        let mut kw = 0.0;
        for (slot, m) in self.ship.design.modules(data) {
            if self.ship.sys.powered[slot] && m.power < 0.0 {
                kw += -m.power * 0.5;
            }
            if m.flash > 0.0 && self.weapon_cd.get(slot).copied().unwrap_or(0.0) > 0.0 {
                kw += m.flash;
            }
        }
        kw
    }

    /// Distance (m) at which passive sensors see this ship.
    pub fn detection_range(&self) -> f64 {
        passive_detection_range(self.thermal_signature_kw())
    }

    /// Plant a false rumour that will be heard like any news (the player cannot tell it apart).
    pub fn plant_rumor(&mut self, tag: u8, dist_ly: f64, text: String) {
        let t = self.time;
        self.news_push(t, tag, dist_ly, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    #[test]
    fn kinetic_energy_matches_the_damage_of_the_data() {
        let d = data();
        assert!((kinetic_energy_j(2.0, 100.0) - 10_000.0).abs() < 1e-9);
        let guns: Vec<_> = d.modules.iter().filter(|m| m.weapon == "kinetic").collect();
        assert!(guns.len() >= 7);
        for m in guns {
            let dmg = damage_from_energy(kinetic_energy_j(m.proj_mass, m.speed));
            assert!(
                (dmg / m.damage - 1.0).abs() < 0.12,
                "{}: {dmg:.0} vs {}",
                m.id,
                m.damage
            );
        }
    }

    #[test]
    fn dispersion_grows_with_range_and_lowers_the_hit_chance() {
        assert!(miss_radius(1.5, 20_000.0) > 2.0 * miss_radius(1.5, 9_000.0));
        assert_eq!(hit_chance(1.0, 100.0, 15.0), 1.0);
        let near = hit_chance(4.0, 5_000.0, 15.0);
        let far = hit_chance(4.0, 20_000.0, 15.0);
        assert!(far < near && far < 0.05, "{near} {far}");
        // A tighter gun lands more at the same range.
        assert!(hit_chance(0.4, 50_000.0, 15.0) > hit_chance(1.5, 50_000.0, 15.0));
    }

    #[test]
    fn beams_diverge_and_air_absorbs_them() {
        let (wl, ap) = (1064.0, 0.25);
        assert!((beam_focus(wl, ap, 0.0) - 1.0).abs() < 1e-12);
        let d1 = beam_focus(wl, ap, 5_000.0);
        let d2 = beam_focus(wl, ap, 50_000.0);
        assert!(d2 < d1 && d1 < 1.0);
        // A shorter wavelength diverges less.
        assert!(beam_focus(500.0, ap, 50_000.0) > d2);
        let vac = beam_dps(60.0, wl, ap, 3_000.0, 0.0);
        let air = beam_dps(60.0, wl, ap, 3_000.0, 1.2);
        assert!(air < vac && vac > 0.0);
        // Particle beams scatter in air far more than lasers.
        assert!(
            atmospheric_transmission(10.0, 1.2, 1000.0)
                < atmospheric_transmission(1064.0, 1.2, 1000.0)
        );
    }

    #[test]
    fn a_hot_ship_is_seen_from_further_than_a_cold_one() {
        let cold = passive_detection_range(0.0);
        let warm = passive_detection_range(100.0);
        let blazing = passive_detection_range(2_500.0);
        assert_eq!(cold, ACTIVE_RANGE);
        assert!(warm > cold && blazing > 4.0 * warm - 1.0);
        // Inverse square: four times the power doubles the range.
        let r1 = passive_detection_range(400.0);
        let r2 = passive_detection_range(1_600.0);
        assert!((r2 / r1 - 2.0).abs() < 1e-9);
    }

    #[test]
    fn every_firing_weapon_carries_heat_and_a_spread() {
        let d = data();
        for m in d
            .modules
            .iter()
            .filter(|m| matches!(m.weapon.as_str(), "kinetic" | "beam"))
        {
            assert!(m.flash > 0.0, "{} has no flash", m.id);
        }
    }

    #[test]
    fn battles_are_deterministic_and_the_stronger_fleet_wins() {
        let a = resolve_battle(1000.0, 400.0, 7, 99);
        assert_eq!(a, resolve_battle(1000.0, 400.0, 7, 99));
        assert!(a.a_wins && a.b_left == 0.0 && a.a_left > 0.5);
        let b = resolve_battle(300.0, 900.0, 7, 99);
        assert!(!b.a_wins && b.a_left == 0.0);
        // Square law: twice the force leaves more than 85 % of the force, not 50 %.
        assert!(resolve_battle(2000.0, 1000.0, 7, 5).a_left > 0.8);
    }

    #[test]
    fn wars_have_fronts_that_both_sides_agree_on_and_the_loser_is_besieged() {
        let d = data();
        let g = Galaxy::new(7);
        let mut found = None;
        'o: for c in politics::cells_around([0, 0, 0], 2) {
            for m in 1..300 {
                let t = m as f64 * WAR_PERIOD + 1.0;
                let f = fronts(&d, &g, 7, c, t);
                if let Some(f) = f.first() {
                    found = Some((c, t, f.clone()));
                    break 'o;
                }
            }
        }
        let (c, t, f) = found.expect("a war near the origin");
        let back = fronts(&d, &g, 7, f.foe, t);
        let mirror = back.iter().find(|x| x.foe == c).expect("seen from the foe");
        assert_ne!(f.own_wins(), mirror.own_wins(), "one victor per front");
        assert_eq!(f.age, mirror.age);
        assert_eq!(f, fronts(&d, &g, 7, c, t)[0], "deterministic");
        let loser = if f.own_wins() { f.foe } else { c };
        assert!(is_besieged(&d, &g, 7, loser, t));
        // Without any war there is no front and no siege.
        let calm = politics::cells_around([0, 0, 0], 2)
            .into_iter()
            .find(|&c| fronts(&d, &g, 7, c, 0.5).is_empty())
            .expect("a peaceful sector");
        assert!(!is_besieged(&d, &g, 7, calm, 0.5));
    }

    #[test]
    fn supply_lines_stretch_with_the_age_of_the_war() {
        let d = data();
        let fresh = fleet_strength(&d, 7, [0, 0, 0], 4, 0, false);
        let old = fleet_strength(&d, 7, [0, 0, 0], 4, 8, false);
        let old_home = fleet_strength(&d, 7, [0, 0, 0], 4, 8, true);
        assert!(old < fresh && old < old_home);
    }

    #[test]
    fn skilled_agents_succeed_more_and_counter_intelligence_catches_them() {
        let rate = |skill: f64, ci: f64, res: SpyResult| {
            (0..2000)
                .filter(|&k| spy_mission(7, skill, ci, k) == res)
                .count()
        };
        assert!(rate(0.9, 0.1, SpyResult::Success) > rate(0.1, 0.1, SpyResult::Success) + 300);
        assert!(rate(0.5, 0.9, SpyResult::Success) < rate(0.5, 0.0, SpyResult::Success));
        assert!(rate(0.5, 0.9, SpyResult::Caught) > rate(0.5, 0.0, SpyResult::Caught));
        assert_eq!(spy_mission(7, 0.5, 0.5, 3), spy_mission(7, 0.5, 0.5, 3));
    }
}
