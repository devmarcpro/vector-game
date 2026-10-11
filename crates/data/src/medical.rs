//! Medical content: body parts, localised wounds and drugs (`data/body.ron`, `wounds.ron`,
//! `drugs.ron`), and their validation.
use crate::{DataError, GameData};
use serde::{Deserialize, Serialize};

/// A part of the body: where wounds land, which organs it houses.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BodyPart {
    pub id: String,
    pub name: String,
    /// Relative chance of being struck.
    pub hit: f64,
    pub limb: bool,
    pub organs: Vec<String>,
    /// What a total permanent disability of the part takes off a trait key.
    pub effects: Vec<(String, f64)>,
}

/// A kind of localised injury.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WoundKind {
    pub id: String,
    pub name: String,
    /// Blood volume share lost per hour at severity 1, undressed.
    pub bleed: f64,
    pub pain: f64,
    pub heal_days: f64,
    /// Chance per day of infection while open and untreated.
    pub infect: f64,
    pub internal: bool,
    pub dress: bool,
    pub parts: Vec<String>,
    pub cause: String,
}

/// A drug (see `data/drugs.ron` for the meaning of the effect keys).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Drug {
    pub id: String,
    pub name: String,
    pub resource: String,
    pub dose: f64,
    pub half_life_h: f64,
    pub effects: Vec<(String, f64)>,
    pub tolerance: f64,
    pub dependence: f64,
    pub interactions: Vec<(String, f64)>,
    pub text: String,
}

impl Drug {
    pub fn effect(&self, key: &str) -> f64 {
        self.effects
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| *v)
            .sum()
    }
}

pub const DRUG_KEYS: [&str; 10] = [
    "pain",
    "clot",
    "infection",
    "stress",
    "fatigue",
    "rad",
    "heal",
    "blood_now",
    "resp",
    "liver",
];

impl GameData {
    pub fn part_index(&self, id: &str) -> Option<usize> {
        self.body_parts.iter().position(|p| p.id == id)
    }
    pub fn wound_index(&self, id: &str) -> Option<usize> {
        self.wounds.iter().position(|w| w.id == id)
    }
    pub fn drug_index(&self, id: &str) -> Option<usize> {
        self.drugs.iter().position(|d| d.id == id)
    }

    /// Parts, wounds and drugs: unique ids, sane numbers, known organs, parts, resources.
    pub(crate) fn validate_medical(&self) -> Result<(), DataError> {
        let bad = |s: String| Err(DataError::Invalid(s));
        let mut seen = std::collections::HashSet::new();
        for p in &self.body_parts {
            if !seen.insert(p.id.as_str()) {
                return bad(format!("partie du corps {} en double", p.id));
            }
            if !p.hit.is_finite() || p.hit <= 0.0 {
                return bad(format!("partie {}: chance de coup invalide", p.id));
            }
            for o in &p.organs {
                if self.organ_index(o).is_none() {
                    return bad(format!("partie {}: organe {o} inconnu", p.id));
                }
            }
        }
        if self.body_parts.is_empty() {
            return bad("aucune partie du corps".into());
        }
        let mut seen = std::collections::HashSet::new();
        for w in &self.wounds {
            if !seen.insert(w.id.as_str()) {
                return bad(format!("blessure {} en double", w.id));
            }
            if !(w.bleed >= 0.0 && w.pain >= 0.0 && w.heal_days > 0.0 && w.infect >= 0.0) {
                return bad(format!("blessure {}: valeurs invalides", w.id));
            }
            for p in &w.parts {
                if self.part_index(p).is_none() {
                    return bad(format!("blessure {}: partie {p} inconnue", w.id));
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        for d in &self.drugs {
            if !seen.insert(d.id.as_str()) {
                return bad(format!("medicament {} en double", d.id));
            }
            if !(d.dose > 0.0 && d.half_life_h > 0.0 && d.tolerance >= 0.0 && d.dependence >= 0.0) {
                return bad(format!("medicament {}: valeurs invalides", d.id));
            }
            if self.resource_index(&d.resource).is_none() {
                return bad(format!(
                    "medicament {}: ressource {} inconnue",
                    d.id, d.resource
                ));
            }
            for (k, v) in &d.effects {
                if !DRUG_KEYS.contains(&k.as_str()) || !v.is_finite() {
                    return bad(format!("medicament {}: effet {k} invalide", d.id));
                }
            }
        }
        for d in &self.drugs {
            for (o, _) in &d.interactions {
                if self.drug_index(o).is_none() {
                    return bad(format!("medicament {}: interaction {o} inconnue", d.id));
                }
            }
        }
        for need in [
            "coupure",
            "fracture_os",
            "barotraumatisme",
            "gelure",
            "contusion",
        ] {
            if self.wound_index(need).is_none() {
                return bad(format!("blessure {need} requise par la physique"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::GameData;

    fn load() -> GameData {
        GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
            .expect("data loads")
    }

    #[test]
    fn medical_content_is_validated() {
        let ok = load();
        assert!(ok.body_parts.len() >= 6 && ok.wounds.len() >= 9 && ok.drugs.len() >= 9);
        let mut d = ok.clone();
        d.body_parts[0].organs.push("organe_fantome".into());
        assert!(d.validate().is_err(), "unknown organ in a part");
        let mut d = ok.clone();
        d.wounds[0].parts.push("queue".into());
        assert!(d.validate().is_err(), "unknown part in a wound");
        let mut d = ok.clone();
        d.drugs[0].resource = "unobtainium".into();
        assert!(d.validate().is_err(), "unknown resource");
        let mut d = ok.clone();
        d.drugs[0].effects.push(("telepathie".into(), 1.0));
        assert!(d.validate().is_err(), "unknown effect key");
        let mut d = ok.clone();
        d.drugs[1].interactions.push(("poison_fantome".into(), 1.0));
        assert!(d.validate().is_err(), "unknown interaction");
        let mut d = ok.clone();
        let dup = d.wounds[0].clone();
        d.wounds.push(dup);
        assert!(d.validate().is_err(), "duplicate wound");
        let mut d = ok;
        d.wounds[0].heal_days = 0.0;
        assert!(
            d.validate().is_err(),
            "a wound that never heals in zero days"
        );
    }
}
