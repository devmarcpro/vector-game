//! Start scene: a generated home system with a station, ship parked next to it.
use crate::voyage::SCAN_FULL;
use crate::Sim;
use std::sync::Arc;
use vector_data::GameData;
use vector_world::galaxy::{Galaxy, StarEntry};
use vector_world::{generate_system, BodyKind};

/// Natural dose (Gy/h, unshielded) at the station over a few moments of its orbit, worst case:
/// a home whose station sits in a lethal belt or beside a raging star is no place to start.
fn station_dose_worst(sys: &vector_world::System, cosmic: f64) -> f64 {
    use crate::navigation::{belt_dose, stellar_dose};
    let Some(st) = sys.bodies.iter().position(|b| b.kind == BodyKind::Station) else {
        return 0.0;
    };
    (0..8)
        .map(|k| {
            let t = f64::from(k) * 2500.0;
            let p = sys.state(st, t).0;
            let mut d = stellar_dose(sys.star_class, p.length())
                + sys.star_radiation(p.length())
                + cosmic;
            for (i, b) in sys.bodies.iter().enumerate().skip(1) {
                if !matches!(b.kind, BodyKind::Planet(_) | BodyKind::Moon(_)) {
                    continue;
                }
                let l = (p - sys.state(i, t).0).length() / b.radius.max(1.0);
                if (1.2..=7.0).contains(&l) {
                    d += belt_dose(sys.physics(i, 0.0).magnetic, l);
                }
            }
            d
        })
        .fold(0.0, f64::max)
}

/// Is this system a good place to start (a safe station, belt, a few planets)?
fn good_home(seed: u64, cosmic: f64) -> bool {
    let sys = generate_system(seed);
    // A close-in station: the flares of its star would kill a docked crew (see `flare_tick`).
    let far_enough = sys.bodies.iter().position(|b| b.kind == BodyKind::Station).is_none_or(
        |st| sys.state(st, 0.0).0.length() >= 0.3 * vector_world::orbit::AU,
    );
    far_enough
        && station_dose_worst(&sys, cosmic) <= 0.001
        && sys.bodies.iter().any(|b| b.kind == BodyKind::Station)
        && !sys.belts.is_empty()
        && sys
            .bodies
            .iter()
            .filter(|b| matches!(b.kind, BodyKind::Planet(_)))
            .count()
            >= 3
}

/// The nearest star to the origin with a good system and neighbours inside basic jump range.
pub fn pick_home(galaxy: &Galaxy) -> StarEntry {
    let near = galaxy.nearby([0.0; 3], 60.0);
    let cosmic = |e: &StarEntry| galaxy.cosmic_effects(e.pos).radiation;
    let fits = |e: &&StarEntry| {
        good_home(e.seed, cosmic(e)) && galaxy.nearby(e.pos, 4.0).len() >= 3
    };
    near.iter()
        .find(fits)
        .or_else(|| near.iter().find(|e| good_home(e.seed, cosmic(e))))
        .or_else(|| near.first())
        .cloned()
        .unwrap_or_else(|| galaxy.sector((0, 0, 0)).remove(0))
}

/// The world every start shares: the galaxy of the seed, its home system, the ship parked at
/// the station, with the default ship and nothing else decided yet.
pub fn base_sim(seed: u64, data: Arc<GameData>) -> Sim {
    let galaxy = Galaxy::new(seed);
    let home = pick_home(&galaxy);
    let mut sim = Sim::new_in_galaxy(galaxy, &home, data);
    let station = sim
        .system
        .bodies
        .iter()
        .position(|b| b.kind == BodyKind::Station);
    sim.spawn_near(station.unwrap_or(0), 3000.0);
    if let Some(st) = station {
        sim.scan[st] = SCAN_FULL;
    }
    sim
}

pub fn new_sim(seed: u64, data: Arc<GameData>) -> Sim {
    let mut sim = base_sim(seed, data);
    // Starting supplies and one engineer.
    let data = Arc::clone(&sim.data);
    for (id, kg) in [
        ("food", 60.0),
        ("water", 150.0),
        ("spare_parts", 20.0),
        ("medicine", 3.0),
    ] {
        if let Some(r) = data.resource_index(id) {
            sim.ship.cargo.add(&data, r, kg);
        }
    }
    if let Some(sk) = data.skill_index("ingenierie") {
        let p = crate::crew::generate_person(&data, vector_core::mix64(seed ^ 0xC4E0), Some(sk));
        let role = crate::crew::Crew::role_index(&data, "engineer");
        sim.crew.hire(&data, p, role, 0.0);
    }
    sim
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new game must not start in a lethal radiation field (a belt, a raging star beside the
    /// station): the QA robot found crews dying of irradiation within days on several seeds.
    #[test]
    fn homes_are_safe_to_start_in_and_to_dock_at() {
        let data = Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        );
        for seed in 1..=8u64 {
            let s = base_sim(seed, Arc::clone(&data));
            let rate = s.dose_rate_aboard();
            assert!(rate <= 0.002, "seed {seed}: {rate} Gy/h at the start");
            // Flares: the worst docked dose stays far from lethal.
            let d_au = (s.ship.pos.length() / vector_world::orbit::AU).max(0.05);
            let flare_docked = 2.0 / (d_au * d_au) * 0.05;
            assert!(flare_docked < 1.5, "seed {seed}: flare {flare_docked} Gy docked");
        }
    }
}
