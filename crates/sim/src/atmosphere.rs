//! The air of a world in layers. Pressure falls with height as a decaying exponential whose scale
//! height comes from the gravity, the temperature and the composition of the air (H = R T / (M g)):
//! a dense, heavy, cold air is thin in height, a light hot one goes up for hundreds of kilometres.
//! The layers (troposphere to exosphere) are cut at fractions of the pressure on the ground. What
//! the air does follows: it brakes a falling ship, it scatters the light of the day (low in a thick
//! atmosphere you no longer see the stars or the other worlds), it hazes the distance.
use crate::Sim;
use vector_world::BodyKind;

/// Universal gas constant, J / (mol K).
pub const R_GAS: f64 = 8.314;

/// The layers: name and the pressure at their foot as a share of the pressure on the ground.
pub const LAYERS: [(&str, f64); 5] = [
    ("TROPOSPHERE", 1.0),
    ("STRATOSPHERE", 0.1),
    ("MESOSPHERE", 0.01),
    ("THERMOSPHERE", 0.0003),
    ("EXOSPHERE", 0.000_01),
];

/// Molar mass (g/mol) of the gases of the atmospheres.
pub fn molar_mass(gas: &str) -> f64 {
    match gas {
        "H2" => 2.0,
        "He" => 4.0,
        "CH4" => 16.0,
        "NH3" => 17.0,
        "H2O" => 18.0,
        "N2" | "CO" => 28.0,
        "O2" => 32.0,
        "Ar" => 40.0,
        "CO2" => 44.0,
        "SO2" => 64.0,
        _ => 30.0,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Air {
    /// Pressure on the ground, bar.
    pub ground: f64,
    /// Scale height, m.
    pub height: f64,
    /// Mean molar mass, g/mol.
    pub mass: f64,
    pub gravity: f64,
    pub temp: f64,
}

impl Air {
    /// The air of a world: ground pressure (bar), temperature (K), mean molar mass (g/mol) and
    /// gravity (m/s^2); the scale height follows (H = R T / (M g)).
    pub fn new(ground: f64, temp: f64, mass: f64, gravity: f64) -> Air {
        Air {
            ground,
            height: R_GAS * temp.max(30.0) / (mass / 1000.0 * gravity.max(0.05)),
            mass,
            gravity,
            temp,
        }
    }

    /// Pressure at `alt` metres above the ground, bar.
    pub fn pressure(&self, alt: f64) -> f64 {
        self.ground * (-alt.max(0.0) / self.height).exp()
    }

    /// Name of the layer at `alt`, or None above the exosphere.
    pub fn layer(&self, alt: f64) -> Option<&'static str> {
        let share = self.pressure(alt) / self.ground.max(1e-12);
        if share < 1e-7 {
            return None;
        }
        // The last layer whose foot is still at least as high in pressure as we are.
        LAYERS.iter().rfind(|(_, foot)| *foot >= share).map(|l| l.0)
    }

    /// Height of the top of the air (where the pressure is a ten-millionth of the ground's), m.
    pub fn top(&self) -> f64 {
        self.height * 16.0
    }

    /// How much of the daylight is scattered at `alt` (0 in the void, 1 under thick air): hides
    /// the stars and the faraway worlds.
    pub fn scattering(&self, alt: f64) -> f64 {
        (self.pressure(alt) / 0.02).clamp(0.0, 1.0)
    }
}

impl Sim {
    /// The air of solid world or giant `i` (None when it has none to speak of).
    pub fn air_of(&self, i: usize) -> Option<Air> {
        let b = self.system.bodies.get(i)?;
        if !matches!(b.kind, BodyKind::Planet(_) | BodyKind::Moon(_)) || b.pressure < 0.001 {
            return None;
        }
        let (mut mass, mut share) = (0.0, 0.0);
        // The composition counts what life has done to it (photosynthesis: CO2 into O2).
        for (gas, f) in &self.atmosphere_of(i) {
            mass += molar_mass(gas) * f64::from(*f);
            share += f64::from(*f);
        }
        let mass = if share > 0.0 { mass / share } else { 29.0 };
        Some(Air::new(b.pressure, b.temp, mass, b.gravity))
    }

    /// The world whose air the ship is in (within the top of the air), and the height in it.
    pub fn in_air(&self) -> Option<(usize, Air, f64)> {
        (0..self.bodies.len()).find_map(|i| {
            let air = self.air_of(i)?;
            let alt = (self.ship.pos - self.bodies[i].pos).length() - self.bodies[i].radius;
            (alt < air.top()).then_some((i, air, alt))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn sim() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        new_sim(5, Arc::new(GameData::load(&dir).expect("data")))
    }

    #[test]
    fn panels_of_a_ship_in_the_air_are_dimmed_like_the_solar_farms_of_a_colony() {
        let mut s = sim();
        assert!(
            (s.solar_dimming() - 1.0).abs() < 1e-9,
            "no air in deep space"
        );
        let i = (0..s.bodies.len())
            .find(|&i| s.air_of(i).is_some_and(|a| a.ground > 0.5))
            .expect("a world with air");
        let ground = s.air_of(i).expect("air").ground;
        s.ship.pos = s.bodies[i].pos + vector_core::Vec3::new(0.0, 0.0, s.bodies[i].radius + 10.0);
        let low = s.solar_dimming();
        assert!(low < 0.95, "{low}");
        assert!(
            (low - crate::colony::solar_transmittance(ground)).abs() < 0.02,
            "the same law as the colonies: {low}"
        );
        let top = s.air_of(i).expect("air").top();
        s.ship.pos =
            s.bodies[i].pos + vector_core::Vec3::new(0.0, 0.0, s.bodies[i].radius + top * 0.5);
        assert!(s.solar_dimming() > low, "thinner air up there");
    }

    #[test]
    fn the_scale_height_follows_gravity_temperature_and_composition() {
        let earth = Air::new(1.0, 288.0, 29.0, 9.8);
        assert!((earth.height - 8_400.0).abs() < 300.0, "{}", earth.height);
        // Pressure falls by e every scale height, and the layers go from the ground up.
        assert!((earth.pressure(earth.height) - (-1.0f64).exp()).abs() < 1e-9);
        assert_eq!(earth.layer(0.0), Some("TROPOSPHERE"));
        assert_eq!(earth.layer(earth.height * 3.0), Some("STRATOSPHERE"));
        assert_eq!(earth.layer(earth.height * 6.0), Some("MESOSPHERE"));
        assert_eq!(earth.layer(earth.height * 9.0), Some("THERMOSPHERE"));
        assert_eq!(earth.layer(earth.height * 12.0), Some("EXOSPHERE"));
        assert_eq!(earth.layer(earth.height * 30.0), None);
        // Day sky: thick air hides the stars, a high or thin one does not.
        assert!(earth.scattering(0.0) > 0.99);
        assert!(earth.scattering(earth.height * 8.0) < 0.1);
        // Real worlds: a heavy gas on a big world is thinner in height than a light one.
        let s = sim();
        let airs: Vec<Air> = (0..s.bodies.len()).filter_map(|i| s.air_of(i)).collect();
        for a in &airs {
            assert!(a.height > 100.0 && a.height < 2.0e6, "{a:?}");
        }
        let light = Air::new(1.0, 288.0, 4.0, 9.8);
        let heavy = Air::new(1.0, 288.0, 44.0, 9.8);
        let hot = Air::new(1.0, 600.0, 29.0, 9.8);
        let big = Air::new(1.0, 288.0, 29.0, 25.0);
        assert!(
            light.height > earth.height && earth.height > heavy.height && hot.height > earth.height
        );
        assert!(
            big.height < earth.height,
            "a stronger gravity packs the air"
        );
    }
}
