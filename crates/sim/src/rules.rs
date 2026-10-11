//! Sandbox game rules: every option changes the simulation for real (and is tested).
use serde::{Deserialize, Serialize};

/// Per-game rule set; any deviation from the default marks the game as "modified".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rules {
    /// Propellant consumption multiplier (0 = none).
    pub fuel_use: f64,
    /// Whether crew hunger, thirst and air matter.
    pub crew_needs: bool,
    /// A destroyed ship ends the game.
    pub permadeath: bool,
    /// Multiplier on all damage the player takes.
    pub damage: f64,
    /// Multiplier on prices of ships, modules and building materials.
    pub build_cost: f64,
    /// Multiplier on research points.
    pub research_speed: f64,
    /// Multiplier on hostile ships' firepower and appetite for a fight (0 = pacifist galaxy).
    pub ai_aggression: f64,
    /// Multiplier on random events (crew events, flares, meteoroids).
    pub event_freq: f64,
    /// The whole galaxy is known from the start.
    pub reveal_map: bool,
    /// Fuel, ammunition and purchases cost nothing.
    pub infinite_resources: bool,
    pub invulnerable: bool,
    /// Console cheats (teleport, spawn, warp...) are allowed.
    pub creative: bool,
    /// Highest time acceleration available.
    pub max_timescale: f64,
    /// Hulls, modules and buildings need their technology (off by default: a free sandbox, everything is available).
    pub tech_locks: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            fuel_use: 1.0,
            crew_needs: true,
            permadeath: false,
            damage: 1.0,
            build_cost: 1.0,
            research_speed: 1.0,
            ai_aggression: 1.0,
            event_freq: 1.0,
            reveal_map: false,
            infinite_resources: false,
            invulnerable: false,
            creative: false,
            max_timescale: 100_000.0,
            tech_locks: false,
        }
    }
}

impl Rules {
    /// True when anything differs from the standard rules.
    pub fn is_modified(&self) -> bool {
        *self != Rules::default()
    }

    /// Effective fuel multiplier.
    pub fn fuel_mult(&self) -> f64 {
        if self.infinite_resources {
            0.0
        } else {
            self.fuel_use
        }
    }

    /// Named options for the rules screen: (label, current value as text).
    pub fn describe(&self) -> Vec<(&'static str, String)> {
        let yn = |b: bool| if b { "OUI" } else { "NON" }.to_string();
        vec![
            (
                "Consommation de carburant",
                format!("x{:.1}", self.fuel_use),
            ),
            ("Besoins de l'equipage", yn(self.crew_needs)),
            ("Mort permanente", yn(self.permadeath)),
            ("Degats subis", format!("x{:.1}", self.damage)),
            ("Cout des constructions", format!("x{:.1}", self.build_cost)),
            (
                "Vitesse de recherche",
                format!("x{:.1}", self.research_speed),
            ),
            ("Agressivite des IA", format!("x{:.1}", self.ai_aggression)),
            (
                "Frequence des evenements",
                format!("x{:.1}", self.event_freq),
            ),
            ("Carte revelee", yn(self.reveal_map)),
            ("Ressources infinies", yn(self.infinite_resources)),
            ("Invulnerabilite", yn(self.invulnerable)),
            ("Mode creatif (console)", yn(self.creative)),
            ("Acceleration max", format!("x{:.0}", self.max_timescale)),
            ("Technologies requises pour construire", yn(self.tech_locks)),
        ]
    }

    pub const OPTION_COUNT: usize = 14;

    /// Change option `i` one step up or down.
    pub fn adjust(&mut self, i: usize, up: bool) {
        let step = |v: f64, lo: f64, hi: f64, d: f64| (v + if up { d } else { -d }).clamp(lo, hi);
        match i {
            0 => self.fuel_use = (step(self.fuel_use, 0.0, 3.0, 0.5) * 10.0).round() / 10.0,
            1 => self.crew_needs = up,
            2 => self.permadeath = up,
            3 => self.damage = (step(self.damage, 0.0, 3.0, 0.5) * 10.0).round() / 10.0,
            4 => self.build_cost = (step(self.build_cost, 0.0, 3.0, 0.5) * 10.0).round() / 10.0,
            5 => {
                self.research_speed =
                    (step(self.research_speed, 0.5, 10.0, 0.5) * 10.0).round() / 10.0
            }
            6 => {
                self.ai_aggression = (step(self.ai_aggression, 0.0, 3.0, 0.5) * 10.0).round() / 10.0
            }
            7 => self.event_freq = (step(self.event_freq, 0.0, 3.0, 0.5) * 10.0).round() / 10.0,
            8 => self.reveal_map = up,
            9 => self.infinite_resources = up,
            10 => self.invulnerable = up,
            11 => self.creative = up,
            12 => {
                let ladder = [1.0, 10.0, 100.0, 1000.0, 10_000.0, 100_000.0];
                let cur = ladder
                    .iter()
                    .position(|&v| v >= self.max_timescale)
                    .unwrap_or(5);
                let next = if up {
                    (cur + 1).min(5)
                } else {
                    cur.saturating_sub(1)
                };
                self.max_timescale = ladder[next];
            }
            13 => self.tech_locks = up,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_unmodified_and_changes_are_detected() {
        let mut r = Rules::default();
        assert!(!r.is_modified());
        r.adjust(10, true);
        assert!(r.is_modified() && r.invulnerable);
        r.adjust(10, false);
        assert!(!r.is_modified());
    }

    #[test]
    fn every_option_is_adjustable_and_described() {
        let base = Rules::default();
        assert_eq!(base.describe().len(), Rules::OPTION_COUNT);
        for i in 0..Rules::OPTION_COUNT {
            let mut up = base.clone();
            let mut down = base.clone();
            up.adjust(i, true);
            down.adjust(i, false);
            assert!(up != base || down != base, "option {i} must be adjustable");
            assert_ne!(up.describe()[i].1, "", "option {i}");
        }
    }

    #[test]
    fn infinite_resources_zero_the_fuel_multiplier() {
        let mut r = Rules::default();
        assert_eq!(r.fuel_mult(), 1.0);
        r.infinite_resources = true;
        assert_eq!(r.fuel_mult(), 0.0);
    }
}
