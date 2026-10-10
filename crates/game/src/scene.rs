//! Start scene: a generated system with a station, ship parked next to it.
use std::sync::Arc;
use vector_core::{Rng, Vec3};
use vector_data::GameData;
use vector_sim::Sim;
use vector_world::{generate_system, BodyKind};

/// Pick the first generated system (from `seed`) that has a station, a belt and a few planets.
pub fn new_sim(seed: u64, data: Arc<GameData>) -> Sim {
    let mut sys = generate_system(seed);
    for k in 1..3000u64 {
        let ok = sys.bodies.iter().any(|b| b.kind == BodyKind::Station)
            && sys
                .bodies
                .iter()
                .any(|b| matches!(b.kind, BodyKind::Asteroid(_)))
            && sys
                .bodies
                .iter()
                .filter(|b| matches!(b.kind, BodyKind::Planet(_)))
                .count()
                >= 3;
        if ok {
            break;
        }
        sys = generate_system(vector_core::mix64(seed.wrapping_add(k)));
    }
    let station = sys.bodies.iter().position(|b| b.kind == BodyKind::Station);
    let mut sim = Sim::new(sys, data);
    sim.spawn_near(station.unwrap_or(0), 3000.0);
    sim
}

/// Deterministic star directions on the unit sphere.
pub fn star_field(seed: u64, n: usize) -> Vec<Vec3> {
    let mut rng = Rng::new(seed ^ 0x57A2);
    (0..n)
        .map(|_| {
            let z = rng.range(-1.0, 1.0);
            let a = rng.range(0.0, std::f64::consts::TAU);
            let r = (1.0 - z * z).sqrt();
            Vec3::new(r * a.cos(), z, r * a.sin())
        })
        .collect()
}
