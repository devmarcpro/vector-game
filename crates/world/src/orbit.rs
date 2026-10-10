//! Keplerian orbits on rails: position and velocity are analytic functions of time.
use vector_core::{Mat3, Vec3};

pub const G: f64 = 6.674e-11;
pub const AU: f64 = 1.495_978_707e11;
pub const M_SUN: f64 = 1.989e30;
pub const R_SUN: f64 = 6.957e8;
pub const M_EARTH: f64 = 5.972e24;
pub const R_EARTH: f64 = 6.371e6;

/// Orbit around `parent` (index into the system's body list). The reference plane is XZ (Y is "up").
#[derive(Clone, Debug)]
pub struct Orbit {
    pub parent: usize,
    pub a: f64,
    pub e: f64,
    pub incl: f64,
    pub lan: f64,
    pub argp: f64,
    pub m0: f64,
    /// Gravitational parameter of the parent (m^3/s^2).
    pub mu: f64,
}

/// Solve Kepler's equation M = E - e sin E for E (Newton, bounded iterations).
pub fn solve_kepler(m: f64, e: f64) -> f64 {
    let m = m.rem_euclid(std::f64::consts::TAU);
    let mut ea = if e < 0.8 { m } else { std::f64::consts::PI };
    for _ in 0..30 {
        let d = (ea - e * ea.sin() - m) / (1.0 - e * ea.cos());
        ea -= d;
        if d.abs() < 1e-13 {
            break;
        }
    }
    ea
}

impl Orbit {
    pub fn mean_motion(&self) -> f64 {
        (self.mu / self.a.powi(3)).sqrt()
    }
    pub fn period(&self) -> f64 {
        std::f64::consts::TAU / self.mean_motion()
    }
    /// Position and velocity relative to the parent at time `t` (seconds).
    pub fn state(&self, t: f64) -> (Vec3, Vec3) {
        let n = self.mean_motion();
        let ea = solve_kepler(self.m0 + n * t, self.e);
        let (s, c) = ea.sin_cos();
        let b = (1.0 - self.e * self.e).sqrt();
        let p = Vec3::new(self.a * (c - self.e), 0.0, self.a * b * s);
        let k = n * self.a / (1.0 - self.e * c);
        let v = Vec3::new(-k * s, 0.0, k * b * c);
        let rot = Mat3::rot_y(self.lan)
            .mul(&Mat3::rot_x(self.incl))
            .mul(&Mat3::rot_y(self.argp));
        (rot.mul_vec(p), rot.mul_vec(v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn earth_like(e: f64) -> Orbit {
        Orbit {
            parent: 0,
            a: AU,
            e,
            incl: 0.3,
            lan: 1.0,
            argp: 2.0,
            m0: 0.5,
            mu: G * M_SUN,
        }
    }

    #[test]
    fn period_is_about_a_year() {
        let y = earth_like(0.0).period() / 86400.0;
        assert!((y - 365.25).abs() < 1.0, "{y}");
    }

    #[test]
    fn orbit_closes_after_one_period() {
        let o = earth_like(0.4);
        let (p0, _) = o.state(1234.0);
        let (p1, _) = o.state(1234.0 + o.period());
        assert!((p1 - p0).length() < 1.0);
    }

    #[test]
    fn vis_viva_holds() {
        let o = earth_like(0.6);
        for k in 0..20 {
            let (p, v) = o.state(k as f64 * 1.0e6);
            let lhs = v.dot(v);
            let rhs = o.mu * (2.0 / p.length() - 1.0 / o.a);
            assert!((lhs - rhs).abs() / rhs < 1e-6);
        }
    }

    #[test]
    fn velocity_matches_finite_difference() {
        let o = earth_like(0.3);
        let (p0, v) = o.state(5.0e6);
        let (p1, _) = o.state(5.0e6 + 1.0);
        assert!(((p1 - p0) - v).length() < 1e-2);
    }
}
