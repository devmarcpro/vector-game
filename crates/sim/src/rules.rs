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
    /// Fine-grained creative switches (all inert unless `creative` is on).
    #[serde(default)]
    pub cx: CreativeFlags,
}

/// Thrust multiplier of creative free flight.
pub const FLY_FREE_MULT: f64 = 25.0;

/// Creative-mode switches: each lifts one restriction in the normal game systems. Every
/// accessor on `Rules` ignores them when `Rules::creative` is off, so a normal game is unchanged.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CreativeFlags {
    pub invulnerable: bool,
    /// Purchases, builds and ammunition cost nothing; no stock needed.
    pub free_build: bool,
    /// Construction and works complete at once.
    pub instant_build: bool,
    /// Jumps and burns use no propellant.
    pub free_travel: bool,
    pub no_hunger: bool,
    pub no_radiation: bool,
    /// No illegal acts, no wanted status.
    pub no_law: bool,
    /// Every technology counts as researched.
    pub all_tech: bool,
    pub all_knowledge: bool,
    /// Unlimited thrust: no structural limit, no fuel.
    pub fly_free: bool,
    pub ghost: bool,
    pub one_hit_kill: bool,
}

impl CreativeFlags {
    pub const COUNT: usize = 12;

    pub fn get_mut(&mut self, i: usize) -> Option<&mut bool> {
        Some(match i {
            0 => &mut self.invulnerable,
            1 => &mut self.free_build,
            2 => &mut self.instant_build,
            3 => &mut self.free_travel,
            4 => &mut self.no_hunger,
            5 => &mut self.no_radiation,
            6 => &mut self.no_law,
            7 => &mut self.all_tech,
            8 => &mut self.all_knowledge,
            9 => &mut self.fly_free,
            10 => &mut self.ghost,
            11 => &mut self.one_hit_kill,
            _ => return None,
        })
    }

    pub fn get(&self, i: usize) -> bool {
        self.clone().get_mut(i).is_some_and(|b| *b)
    }

    pub fn name(i: usize) -> &'static str {
        [
            "Invulnerable",
            "Construction gratuite",
            "Construction instantanee",
            "Voyage sans carburant",
            "Sans faim ni soif",
            "Sans radiation",
            "Sans loi ni police",
            "Toutes les technologies",
            "Tout le savoir",
            "Vol libre (poussee infinie)",
            "Mode fantome",
            "Tout mourir en un coup",
        ]
        .get(i)
        .copied()
        .unwrap_or("?")
    }
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
            cx: CreativeFlags::default(),
        }
    }
}

impl Rules {
    /// True when anything differs from the standard rules.
    pub fn is_modified(&self) -> bool {
        *self != Rules::default()
    }

    fn cx_on(&self) -> bool {
        self.creative
    }
    /// Purchases and builds are free (rule, or creative free-build).
    pub fn free(&self) -> bool {
        self.infinite_resources || (self.cx_on() && self.cx.free_build)
    }
    pub fn invuln(&self) -> bool {
        self.invulnerable || (self.cx_on() && self.cx.invulnerable)
    }
    /// Whether crew needs apply.
    pub fn needs(&self) -> bool {
        self.crew_needs && !(self.cx_on() && self.cx.no_hunger)
    }
    pub fn no_radiation(&self) -> bool {
        self.invuln() || (self.cx_on() && self.cx.no_radiation)
    }
    pub fn no_law(&self) -> bool {
        self.cx_on() && self.cx.no_law
    }
    pub fn instant_build(&self) -> bool {
        self.cx_on() && self.cx.instant_build
    }
    /// Technologies do not gate hulls, modules or buildings.
    pub fn tech_open(&self) -> bool {
        !self.tech_locks || (self.cx_on() && (self.cx.all_tech || self.cx.free_build))
    }
    /// Creative free build: distance, docking, kits and permits are ignored too.
    pub fn creative_free(&self) -> bool {
        self.cx_on() && self.cx.free_build
    }
    pub fn ghost(&self) -> bool {
        self.cx_on() && self.cx.ghost
    }
    pub fn all_knowledge(&self) -> bool {
        self.cx_on() && self.cx.all_knowledge
    }
    /// Highest time acceleration: lifted to x1e6 in creative mode.
    pub fn max_ts(&self) -> f64 {
        if self.cx_on() {
            1_000_000.0
        } else {
            self.max_timescale
        }
    }
    /// Free travel: jumps ignore range, wells, antimatter and propellant.
    pub fn free_travel(&self) -> bool {
        self.cx_on() && (self.cx.free_travel || self.cx.fly_free)
    }
    pub fn one_hit_kill(&self) -> bool {
        self.cx_on() && self.cx.one_hit_kill
    }
    pub fn fly_free(&self) -> bool {
        self.cx_on() && self.cx.fly_free
    }

    /// Effective fuel multiplier.
    pub fn fuel_mult(&self) -> f64 {
        if self.infinite_resources || (self.cx_on() && (self.cx.free_travel || self.cx.fly_free)) {
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
    fn creative_flags_are_inert_without_creative() {
        let mut r = Rules::default();
        for i in 0..CreativeFlags::COUNT {
            *r.cx.get_mut(i).unwrap() = true;
        }
        assert!(!r.free() && !r.invuln() && r.needs() && !r.no_law());
        assert_eq!(r.fuel_mult(), 1.0);
        r.creative = true;
        assert!(r.free() && r.invuln() && !r.needs() && r.no_law() && r.no_radiation());
        assert_eq!(r.fuel_mult(), 0.0);
    }

    #[test]
    fn infinite_resources_zero_the_fuel_multiplier() {
        let mut r = Rules::default();
        assert_eq!(r.fuel_mult(), 1.0);
        r.infinite_resources = true;
        assert_eq!(r.fuel_mult(), 0.0);
    }
}
