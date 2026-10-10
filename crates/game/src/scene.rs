//! Hard-coded M1 test scene (replaced by procedural systems in M2).
use vector_core::{Rng, Vec3};
use vector_sim::{Body, BodyKind, Ship, Sim};

pub fn new_sim(seed: u64) -> Sim {
    let mut bodies = vec![
        Body {
            name: "SOL".into(),
            kind: BodyKind::Star,
            pos: Vec3::new(0.0, 2.0e10, 1.5e11),
            radius: 7.0e8,
        },
        Body {
            name: "TERRA".into(),
            kind: BodyKind::Planet,
            pos: Vec3::new(0.0, -1.0e7, 3.0e7),
            radius: 6.4e6,
        },
        Body {
            name: "LUNA".into(),
            kind: BodyKind::Moon,
            pos: Vec3::new(4.0e7, 5.0e6, 6.0e7),
            radius: 1.7e6,
        },
        Body {
            name: "ARGO STATION".into(),
            kind: BodyKind::Station,
            pos: Vec3::new(300.0, 80.0, 4000.0),
            radius: 120.0,
        },
    ];
    let mut rng = Rng::new(seed);
    for i in 0..14 {
        let pos = Vec3::new(
            rng.range(-6000.0, 6000.0),
            rng.range(-2000.0, 2000.0),
            rng.range(1500.0, 9000.0),
        );
        bodies.push(Body {
            name: format!("ROCK-{:02}", i + 1),
            kind: BodyKind::Asteroid,
            pos,
            radius: rng.range(20.0, 90.0),
        });
    }
    Sim::new(Ship::new(Vec3::ZERO), bodies)
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
