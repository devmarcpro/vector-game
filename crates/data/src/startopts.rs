//! Options of the new-game flow (see `data/start_options.ron`): AI cores and personalities,
//! crew backgrounds, start systems and places, difficulty presets. Checked at load.
use crate::{DataError, GameData};
use serde::{Deserialize, Serialize};

/// Keys a preset may set (the rules, the world parameters and the save mode).
pub const PRESET_KEYS: [&str; 18] = [
    "fuel_use",
    "crew_needs",
    "permadeath",
    "damage",
    "build_cost",
    "research_speed",
    "ai_aggression",
    "event_freq",
    "reveal_map",
    "infinite_resources",
    "invulnerable",
    "creative",
    "max_timescale",
    "tech_locks",
    "epidemics",
    "surprises",
    "provisions",
    "save_mode",
];
/// Kinds of start system and start place the code knows how to build.
pub const SYSTEM_KINDS: [&str; 5] = ["aleatoire", "solaire", "monde_habitable", "isole", "dense"];
pub const PLACE_KINDS: [&str; 5] = ["station", "orbite", "sol", "espace_profond", "epave"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiCore {
    pub id: String,
    pub name: String,
    pub power: f64,
    pub memory: u32,
    pub cost: u32,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiTrait {
    pub id: String,
    pub name: String,
    /// Permanent bonus or malus on each task: pilotage, analyse, traduction, simulation, defense.
    /// Five numbers each (see `MIND_TASKS`).
    pub bias: Vec<f64>,
    pub alloc: Vec<u8>,
    pub credits_mult: f64,
    pub crew_morale: f64,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Background {
    pub id: String,
    pub name: String,
    pub cost: i32,
    pub skills: Vec<(String, f32)>,
    pub credits: f64,
    pub reputation: Vec<(String, f64)>,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StartKind {
    pub id: String,
    pub name: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub text: String,
    pub settings: Vec<(String, f64)>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StartData {
    pub ai_cores: Vec<AiCore>,
    pub ai_traits: Vec<AiTrait>,
    pub backgrounds: Vec<Background>,
    pub systems: Vec<StartKind>,
    pub places: Vec<StartKind>,
    pub presets: Vec<Preset>,
}

impl GameData {
    pub fn ai_trait_index(&self, id: &str) -> Option<usize> {
        self.start.ai_traits.iter().position(|t| t.id == id)
    }
    pub fn background_index(&self, id: &str) -> Option<usize> {
        self.start.backgrounds.iter().position(|t| t.id == id)
    }

    pub(crate) fn validate_start_options(&self) -> Result<(), DataError> {
        let bad = |s: String| Err(DataError::Invalid(format!("start_options.ron: {s}")));
        let s = &self.start;
        fn unique<'a>(ids: impl Iterator<Item = &'a str>) -> Option<&'a str> {
            let mut seen = std::collections::HashSet::new();
            ids.into_iter().find(|i| !seen.insert(*i))
        }
        if s.ai_cores.is_empty() || s.ai_cores[0].cost != 0 {
            return bad("le premier noyau d'IA doit exister et etre gratuit".into());
        }
        if s.ai_traits.is_empty() || s.backgrounds.is_empty() || s.presets.is_empty() {
            return bad("personnalites, historiques et presets ne peuvent pas etre vides".into());
        }
        for (what, dup) in [
            ("noyau", unique(s.ai_cores.iter().map(|x| x.id.as_str()))),
            (
                "personnalite",
                unique(s.ai_traits.iter().map(|x| x.id.as_str())),
            ),
            (
                "historique",
                unique(s.backgrounds.iter().map(|x| x.id.as_str())),
            ),
            ("preset", unique(s.presets.iter().map(|x| x.id.as_str()))),
        ] {
            if let Some(d) = dup {
                return bad(format!("{what} {d} en double"));
            }
        }
        for c in &s.ai_cores {
            if !c.power.is_finite() || !(0.0..=500.0).contains(&c.power) || c.memory > 20 {
                return bad(format!("noyau {}: puissance ou memoire hors limites", c.id));
            }
        }
        for t in &s.ai_traits {
            if t.bias.len() != 5 || t.alloc.len() != 5 {
                return bad(format!("personnalite {}: cinq taches attendues", t.id));
            }
            if t.bias.iter().any(|b| !b.is_finite() || b.abs() > 0.5) {
                return bad(format!("personnalite {}: biais hors de -0.5..0.5", t.id));
            }
            if t.alloc.iter().any(|&a| a > 10) || t.alloc.iter().all(|&a| a == 0) {
                return bad(format!("personnalite {}: repartition invalide", t.id));
            }
            if !(0.5..=2.0).contains(&t.credits_mult) || t.crew_morale.abs() > 0.5 {
                return bad(format!(
                    "personnalite {}: multiplicateur hors limites",
                    t.id
                ));
            }
        }
        if s.ai_traits[0].bias.iter().any(|b| *b != 0.0) {
            return bad("la premiere personnalite doit etre neutre".into());
        }
        for b in &s.backgrounds {
            if !(-3..=3).contains(&b.cost) || !b.credits.is_finite() {
                return bad(format!("historique {}: cout ou credits invalides", b.id));
            }
            for (sk, v) in &b.skills {
                if self.skill_index(sk).is_none() || !(0.0..=3.0).contains(v) {
                    return bad(format!("historique {}: competence {sk} invalide", b.id));
                }
            }
            for (f, v) in &b.reputation {
                if (f != "home" && self.faction_index(f).is_none()) || !v.is_finite() {
                    return bad(format!("historique {}: faction {f} inconnue", b.id));
                }
            }
        }
        if s.backgrounds[0].cost != 0 || !s.backgrounds[0].skills.is_empty() {
            return bad("le premier historique doit etre neutre".into());
        }
        let ids: Vec<&str> = s.systems.iter().map(|k| k.id.as_str()).collect();
        if ids != SYSTEM_KINDS {
            return bad("systemes: la liste doit etre exactement celle connue du code".into());
        }
        let ids: Vec<&str> = s.places.iter().map(|k| k.id.as_str()).collect();
        if ids != PLACE_KINDS {
            return bad("lieux: la liste doit etre exactement celle connue du code".into());
        }
        for p in &s.presets {
            for (k, v) in &p.settings {
                if !PRESET_KEYS.contains(&k.as_str()) || !v.is_finite() {
                    return bad(format!("preset {}: cle {k} inconnue", p.id));
                }
            }
        }
        Ok(())
    }
}
