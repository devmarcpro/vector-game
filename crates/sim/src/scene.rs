//! Start scene: a generated home system with a station, ship parked next to it.
use crate::voyage::SCAN_FULL;
use crate::Sim;
use std::sync::Arc;
use vector_data::GameData;
use vector_world::galaxy::{Galaxy, StarEntry};
use vector_world::{generate_system, BodyKind};

/// Is this system a good place to start (station, belt, a few planets)?
pub(crate) fn good_home(seed: u64) -> bool {
    let sys = generate_system(seed);
    sys.bodies.iter().any(|b| b.kind == BodyKind::Station)
        && !sys.belts.is_empty()
        && sys
            .bodies
            .iter()
            .filter(|b| matches!(b.kind, BodyKind::Planet(_)))
            .count()
            >= 3
}

/// The nearest star to the origin with a good system and neighbours inside basic jump range
/// (the home of the test fixtures: `new_sim`).
pub fn pick_home(galaxy: &Galaxy) -> StarEntry {
    let near = galaxy.nearby([0.0; 3], 60.0);
    let fits = |e: &&StarEntry| good_home(e.seed) && galaxy.nearby(e.pos, 4.0).len() >= 3;
    near.iter()
        .find(fits)
        .or_else(|| near.iter().find(|e| good_home(e.seed)))
        .or_else(|| near.first())
        .cloned()
        .unwrap_or_else(|| galaxy.sector((0, 0, 0)).remove(0))
}

/// The home of a real new game: `pick_home`, but among the places where a ship that waits does
/// not die of it (no red dwarf, no hot region of the galaxy, no station in the belt of a big
/// planet) when the galaxy has one.
pub fn pick_start_home(galaxy: &Galaxy) -> StarEntry {
    let near = galaxy.nearby([0.0; 3], 60.0);
    let fits = |e: &&StarEntry| good_home(e.seed) && galaxy.nearby(e.pos, 4.0).len() >= 3;
    // A red dwarf flares the most and its habitable zone is close to it: a start under one is a
    // start under fire. Take the nearest quieter star when there is one.
    let calm = |e: &&StarEntry| calm_home(galaxy, e);
    let quiet = |e: &&StarEntry| fits(e) && calm(e) && e.header.class != vector_world::StarClass::M;
    near.iter()
        .find(quiet)
        .or_else(|| near.iter().find(|e| fits(e) && calm(e)))
        .or_else(|| near.iter().find(fits))
        .or_else(|| near.iter().find(|e| good_home(e.seed)))
        .or_else(|| near.first())
        .cloned()
        .unwrap_or_else(|| galaxy.sector((0, 0, 0)).remove(0))
}

/// Whether the place a new game starts in is free of the radiation that kills a ship that waits
/// there: no hot region of the galaxy around the star, and no station inside the radiation belt of
/// a magnetised world (a start is "in flight beside a station": the belts of big planets give
/// tens of mGy/h, Io-like).
pub(crate) fn calm_home(galaxy: &Galaxy, e: &StarEntry) -> bool {
    if galaxy.cosmic_effects(e.pos).radiation > 0.0 {
        return false;
    }
    let sys = generate_system(e.seed);
    sys.bodies
        .iter()
        .filter(|b| b.kind == BodyKind::Station)
        .filter_map(|b| b.orbit.as_ref())
        .all(|o| {
            let host = &sys.bodies[o.parent];
            let l = o.a / host.radius.max(1.0);
            let magnetic = sys.physics(o.parent, 0.0).magnetic;
            crate::navigation::belt_dose(magnetic, l) < 0.003
        })
}

/// The world every start shares: the galaxy of the seed, its home system, the ship parked at
/// the station, with the default ship and nothing else decided yet.
pub fn base_sim(seed: u64, data: Arc<GameData>) -> Sim {
    let galaxy = Galaxy::new(seed);
    let home = pick_home(&galaxy);
    base_sim_at(galaxy, &home, data)
}

/// The same world with another system as home (see `start_custom::home_for`).
pub fn base_sim_at(galaxy: Galaxy, home: &StarEntry, data: Arc<GameData>) -> Sim {
    let mut sim = Sim::new_in_galaxy(galaxy, home, data);
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

/// Distance (m) from the station of a system to its nearest asteroid at time 0 (None without
/// either).
#[cfg(test)]
pub(crate) fn station_to_asteroid_m(seed: u64) -> Option<f64> {
    let sys = generate_system(seed);
    let st = sys
        .bodies
        .iter()
        .position(|b| b.kind == BodyKind::Station)?;
    let sp = sys.state(st, 0.0).0;
    sys.bodies
        .iter()
        .enumerate()
        .filter(|(_, b)| matches!(b.kind, BodyKind::Asteroid(_)))
        .map(|(i, _)| (sys.state(i, 0.0).0 - sp).length())
        .min_by(f64::total_cmp)
}
