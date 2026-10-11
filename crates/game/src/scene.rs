//! Start scene helpers: the world set-up lives in `vector_sim::scene`; the star backdrop is a view concern.
use vector_core::{Rng, Vec3};

pub use vector_sim::scene::new_sim;

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
