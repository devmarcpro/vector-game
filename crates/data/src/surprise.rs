//! Surprises (see `data/surprises.ron`): rare events born of the crossing of systems. A surprise
//! lists conditions on the state of the world around the ship (a war and a cartel, an epidemic
//! and a colony, a very warm relation, an anomaly and a contract...), a weight, effects on the
//! world, and optionally choices offered to the player. The simulation rolls them with the hash
//! of the seed, the sector and the day: nothing here is scripted, every line is a rule.
use serde::{Deserialize, Serialize};

/// Whom a relation effect or condition is about. `Here`, `Ally`, `Rival` and `Contact` are
/// worked out from the territory the ship is in; `Civ` from the first civilised world of the
/// system. A target that cannot be worked out makes the condition false and the effect skipped.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Target {
    Faction(String),
    Here,
    Ally,
    Rival,
    Cartel(String),
    Company(String),
    Contact,
    Civ,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cmp {
    Lt,
    Gt,
}

impl Cmp {
    pub fn test(self, a: f64, b: f64) -> bool {
        match self {
            Cmp::Lt => a < b,
            Cmp::Gt => a > b,
        }
    }
}

/// A condition on the state of the world around the ship; all those of a surprise must hold.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Cond {
    /// A world event (`world_events.ron` id) is running in the region.
    WorldEvent(String),
    AtWar,
    /// The sector is shaken by a murder or a coup, at least this much (0..1).
    Unrest(f64),
    /// A cartel (id) holds prices in the region this month.
    CartelHere(String),
    /// A company of the sector suffers or enjoys an incident.
    Incident,
    /// One of your colonies, within this many ly, has an epidemic.
    Epidemic(f64),
    /// One of your colonies lies within this many ly.
    ColonyNear(f64),
    /// One of your colonies within 30 ly is unhappy (unrest above this, 0..1).
    ColonyUnrest(f64),
    /// The ship is in the territory of the faction (id).
    Territory(String),
    /// The ship is in the territory of some faction.
    Claimed,
    /// A native civilisation lives in the system.
    Civ,
    /// An anomaly floats in the system, of any kind or of the kind (id).
    Anomaly,
    AnomalyOf(String),
    Docked,
    InFlight,
    /// Credits above / below.
    Credits(Cmp, f64),
    /// At least this many active contracts.
    Contracts(u32),
    /// The relation (-1..1) with the target is above / below.
    Rel(Target, Cmp, f64),
    /// Average crew morale (0..1) above / below.
    Morale(Cmp, f64),
    /// At least this many people aboard.
    Crew(u32),
    /// At least this many kg of a resource (id) in the hold.
    Hold(String, f64),
    /// You hold a seat of the cartel (id).
    Seat(String),
    Wanted,
    /// The bank is owed money.
    InDebt,
    /// You hold shares.
    Shares,
    /// You keep animals aboard.
    Herd,
    /// At least this many people sleep in the cryo caissons.
    Cryo(u32),
    /// One of your colonies within 30 ly has terraforming works or a megastructure under way.
    Terraform,
    /// Somebody aboard is ill.
    Sick,
    /// Somebody aboard, asleep or not, has an acute dose of at least this many Sv.
    Dose(f64),
    /// What the ship understands of the language spoken here (0..1), where somebody holds the
    /// space, is under / over the value.
    Language(Cmp, f64),
}

/// What a surprise does to the world.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Effect {
    Credits(f64),
    /// Change the relation with the target (it echoes on its allies and rivals).
    Rel(Target, f64),
    /// Form a tie between two targets for some days: a positive weight is an alliance, a negative
    /// one a rivalry.
    Bond(Target, Target, f64, f64),
    /// Price shock in the region: (resource id or "cat:<category>", multiplier, days).
    Shock(String, f64, f64),
    /// Give (or take, if negative) kg of a resource (id).
    Cargo(String, f64),
    Morale(f64),
    /// A contract at the nearest station of the system: (resource id, kg, reward multiplier of
    /// the reference price).
    Contract(String, f64, f64),
    Research(f64),
    /// Happiness of the colonies within 30 ly.
    ColonyHappiness(f64),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub label: String,
    /// What is told when the player takes it.
    pub result: String,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Surprise {
    pub id: String,
    pub name: String,
    /// One of the tags of the news feed (POLITIQUE, GUERRE, ENTREPRISES, BOURSE, COLONIES,
    /// SCIENCE, CRIME).
    pub tag: String,
    /// Relative frequency among the surprises whose conditions hold.
    pub weight: f64,
    /// Days before it may happen again.
    pub cooldown: f64,
    pub when: Vec<Cond>,
    /// Applied at once (before the choices, which come on top).
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub choices: Vec<Choice>,
    /// Days left to choose (0 when there is no choice).
    #[serde(default)]
    pub window: f64,
    /// Placeholders: {here} {ally} {rival} {place}.
    pub text: String,
}

/// Highest base chance per day for a surprise of weight 1 whose conditions hold.
pub const DAILY_CHANCE: f64 = 0.05;
