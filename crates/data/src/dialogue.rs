//! Dialogues (see `data/dialogues.ron`): conversation trees held over the radio with other
//! ships. Every node has several variants of its text by the tone of the speaker (derived from
//! the mores of their faction), and options with a condition and effects. Conditions and effects
//! are small strings, parsed here so the data can be validated at load.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DialogueOption {
    pub label: String,
    /// "" or conditions joined by '&' (see `Cond`).
    pub cond: String,
    pub effects: Vec<String>,
    /// Next node id, or "end".
    pub next: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DialogueNode {
    pub id: String,
    /// (tone, text): tones are formel, cordial, mefiant, neutre. Placeholders: {name}
    /// {faction} {system} {cargo} {news} {region} {result}.
    pub text: Vec<(String, String)>,
    pub options: Vec<DialogueOption>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Dialogues {
    /// (npc role label as `NpcRole::label`, start node id).
    pub starts: Vec<(String, String)>,
    pub nodes: Vec<DialogueNode>,
}

pub const TONES: [&str; 4] = ["formel", "cordial", "mefiant", "neutre"];
pub const ROLES: [&str; 5] = ["CARGO", "MINEUR", "PIRATE", "PATROUILLE", "CIVILISATION"];

impl Dialogues {
    pub fn node(&self, id: &str) -> Option<&DialogueNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn start_for(&self, role: &str) -> Option<&str> {
        self.starts
            .iter()
            .find(|(r, _)| r == role)
            .map(|(_, n)| n.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// One condition of an option.
#[derive(Clone, Debug, PartialEq)]
pub enum Cond {
    Rep(Cmp, f64),
    Credits(Cmp, f64),
    Cargo(String, Cmp, f64),
    Ethos(String, Cmp, f64),
    Role(String),
    Hostile(bool),
}

fn split_cmp(s: &str) -> Option<(&str, Cmp, f64)> {
    let at = s.find(['<', '>'])?;
    let cmp = if s.as_bytes()[at] == b'<' {
        Cmp::Lt
    } else {
        Cmp::Gt
    };
    let v: f64 = s[at + 1..].parse().ok()?;
    Some((&s[..at], cmp, v))
}

/// Parse a whole condition string ("rep>0.3&cargo:food>20").
pub fn parse_conds(s: &str) -> Result<Vec<Cond>, String> {
    let mut out = Vec::new();
    for part in s.split('&').map(str::trim).filter(|p| !p.is_empty()) {
        let bad = || format!("condition invalide: {part}");
        let c = if part == "hostile" {
            Cond::Hostile(true)
        } else if part == "!hostile" {
            Cond::Hostile(false)
        } else if let Some(r) = part.strip_prefix("role:") {
            Cond::Role(r.to_string())
        } else {
            let (lhs, cmp, v) = split_cmp(part).ok_or_else(bad)?;
            if lhs == "rep" {
                Cond::Rep(cmp, v)
            } else if lhs == "credits" {
                Cond::Credits(cmp, v)
            } else if let Some(r) = lhs.strip_prefix("cargo:") {
                Cond::Cargo(r.to_string(), cmp, v)
            } else if let Some(a) = lhs.strip_prefix("ethos:") {
                Cond::Ethos(a.to_string(), cmp, v)
            } else {
                return Err(bad());
            }
        };
        out.push(c);
    }
    Ok(out)
}

/// One effect of an option.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Rep(f64),
    Credits(f64),
    Give(String, f64),
    News,
    Region,
    Peace,
    Hostile,
    Bluff,
    Bribe,
    Tip,
    /// Where the richest rock of the system is.
    Ore,
    /// Learn the mores of the people you talk to.
    Mores,
    /// A civilisation gives you something it makes.
    CivGift,
}

pub fn parse_effect(s: &str) -> Result<Effect, String> {
    let bad = || format!("effet invalide: {s}");
    Ok(match s {
        "news" => Effect::News,
        "region" => Effect::Region,
        "peace" => Effect::Peace,
        "hostile" => Effect::Hostile,
        "bluff" => Effect::Bluff,
        "bribe" => Effect::Bribe,
        "tip" => Effect::Tip,
        "ore" => Effect::Ore,
        "mores" => Effect::Mores,
        "civgift" => Effect::CivGift,
        _ => {
            if let Some(v) = s.strip_prefix("rep:") {
                Effect::Rep(v.parse().map_err(|_| bad())?)
            } else if let Some(v) = s.strip_prefix("credits:") {
                Effect::Credits(v.parse().map_err(|_| bad())?)
            } else if let Some(v) = s.strip_prefix("give:") {
                let (res, kg) = v.split_once(':').ok_or_else(bad)?;
                Effect::Give(res.to_string(), kg.parse().map_err(|_| bad())?)
            } else {
                return Err(bad());
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditions_and_effects_parse_and_bad_ones_fail() {
        let c =
            parse_conds("rep>0.3 & cargo:food>20&!hostile&ethos:honneur<0.2&role:CARGO").unwrap();
        assert_eq!(c.len(), 5);
        assert_eq!(c[0], Cond::Rep(Cmp::Gt, 0.3));
        assert_eq!(c[1], Cond::Cargo("food".into(), Cmp::Gt, 20.0));
        assert!(parse_conds("rep=3").is_err());
        assert!(parse_conds("banane>1").is_err());
        assert_eq!(
            parse_effect("give:food:20").unwrap(),
            Effect::Give("food".into(), 20.0)
        );
        assert_eq!(parse_effect("rep:-0.2").unwrap(), Effect::Rep(-0.2));
        assert!(parse_effect("explose").is_err());
        assert!(parse_effect("credits:beaucoup").is_err());
        assert!(Cmp::Gt.test(2.0, 1.0) && !Cmp::Lt.test(2.0, 1.0));
    }
}
