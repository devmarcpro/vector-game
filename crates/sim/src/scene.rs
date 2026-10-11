//! Start scene: a generated home system with a station, ship parked next to it.
use crate::voyage::SCAN_FULL;
use crate::Sim;
use std::sync::Arc;
use vector_data::GameData;
use vector_world::galaxy::{Galaxy, StarEntry};
use vector_world::{generate_system, BodyKind};

/// Is this system a good place to start (station, belt, a few planets)?
fn good_home(seed: u64) -> bool {
    let sys = generate_system(seed);
    sys.bodies.iter().any(|b| b.kind == BodyKind::Station)
        && sys
            .bodies
            .iter()
            .any(|b| matches!(b.kind, BodyKind::Asteroid(_)))
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
    let fits = |e: &&StarEntry| good_home(e.seed) && galaxy.nearby(e.pos, 4.0).len() >= 3;
    near.iter()
        .find(fits)
        .or_else(|| near.iter().find(|e| good_home(e.seed)))
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
