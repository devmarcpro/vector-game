//! Data schemas and loading/validation of `data/*.ron`. Content lives in data, not code.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub name: String,
    pub category: String,
    /// kg/m3
    pub density: f64,
    /// credits per kg (base price)
    pub value: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AsteroidClass {
    pub id: String,
    pub name: String,
    pub weight: f64,
    pub density: f64,
    /// (resource id, mass fraction)
    pub composition: Vec<(String, f64)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotType {
    #[default]
    Core,
    Internal,
    External,
    Hardpoint,
}

impl SlotType {
    pub fn label(self) -> &'static str {
        match self {
            SlotType::Core => "NOYAU",
            SlotType::Internal => "INTERNE",
            SlotType::External => "EXTERNE",
            SlotType::Hardpoint => "ARMEMENT",
        }
    }
}

/// A ship module. Unused stats default to zero so data files only list what matters.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Module {
    pub id: String,
    pub name: String,
    pub category: String,
    pub slot: SlotType,
    pub size: u8,
    pub mass: f64,
    pub hp: f64,
    pub cost: f64,
    /// kW: positive = produced (reactor/solar), negative = consumed at full load.
    pub power: f64,
    pub priority: u8,
    pub thrust: f64,
    pub isp: f64,
    pub fuel: f64,
    pub cargo_kg: f64,
    pub cargo_m3: f64,
    /// MJ
    pub battery: f64,
    /// kW dissipated at reference temperature
    pub radiate: f64,
    pub mining_rate: f64,
    pub mining_range: f64,
    pub life_support: f64,
    pub crew_cap: f64,
    pub medical: f64,
    pub science: f64,
    pub repair: f64,
    pub food: f64,
    pub scan_range: f64,
    /// Light years per jump (FTL drives).
    pub jump_range: f64,
    pub armor: f64,
    pub agility: f64,
    pub rcs: f64,
    pub shield: f64,
    pub dock: f64,
    pub hangar: f64,
    pub armory: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hull {
    pub id: String,
    pub name: String,
    pub class: String,
    pub mass: f64,
    pub hp: f64,
    pub cost: f64,
    pub rcs: f64,
    pub max_rate: f64,
    pub ang_accel: f64,
    pub slots: Vec<(SlotType, u8)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
}

/// A crew post; `skill` is the skill id that drives efficiency (empty = none).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub skill: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Trait {
    pub id: String,
    pub name: String,
    pub effects: Vec<(String, f64)>,
}

impl Trait {
    pub fn effect(&self, key: &str) -> f64 {
        self.effects
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v)
            .sum()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Disease {
    pub id: String,
    pub name: String,
    pub severity: f64,
    pub duration_days: f64,
    pub contagion: f64,
    pub lethality: f64,
    pub cure: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Culture {
    pub id: String,
    pub name: String,
    pub first_start: Vec<String>,
    pub first_end: Vec<String>,
    pub last_start: Vec<String>,
    pub last_end: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventDef {
    pub id: String,
    pub kind: String,
    pub weight: f64,
    pub needs_other: bool,
    pub cond: Vec<(String, String, f64)>,
    pub effects: Vec<(String, f64)>,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Faction {
    pub id: String,
    pub name: String,
    pub culture: String,
    pub ideology: String,
    pub color: u32,
    pub tax: f64,
    pub contraband: Vec<String>,
    pub aggression: f64,
    pub allies: Vec<String>,
    pub enemies: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Industry {
    pub id: String,
    pub name: String,
    pub produces: Vec<String>,
    pub consumes: Vec<String>,
}

/// A production recipe. `extract` recipes draw from the site's deposits (the single output names a
/// pseudo-resource: "ore", "volatiles", "gas" or a real resource such as "regolith").
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub inputs: Vec<(String, f64)>,
    pub outputs: Vec<(String, f64)>,
    /// Duration of one batch.
    pub hours: f64,
    pub energy_kwh: f64,
}

/// Site tags a body can carry (see `Building::sites`).
pub const SITE_TAGS: [&str; 6] = ["any", "rocky", "icy", "asteroid", "habitable", "gas_giant"];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Building {
    pub id: String,
    pub name: String,
    pub category: String,
    /// Construction materials (resource id, kg).
    pub cost: Vec<(String, f64)>,
    pub days: f64,
    /// kW: positive = produced, negative = consumed.
    pub power: f64,
    pub workers: f64,
    pub housing: f64,
    pub storage: f64,
    /// Credits per day.
    pub upkeep: f64,
    pub recipes: Vec<String>,
    /// Batches per day at full staffing.
    pub throughput: f64,
    pub sites: Vec<String>,
    /// Fuel burnt per day by power plants (resource id, kg).
    pub burns: Vec<(String, f64)>,
    pub medical: f64,
    pub education: f64,
    pub recreation: f64,
    pub security: f64,
    pub recycling: f64,
    pub research: f64,
    pub port: f64,
    pub terraform: f64,
    pub defense: f64,
}

/// A colony policy with discrete levels: (label, credits per person per day, productivity delta, happiness delta).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Policy {
    pub id: String,
    pub name: String,
    pub levels: Vec<(String, f64, f64, f64)>,
    pub default_level: usize,
}

const PERSON_FIELDS: [&str; 14] = [
    "morale", "health", "loyalty", "stress", "fatigue", "hunger", "thirst", "social", "relation",
    "age", "credits", "death", "quit", "salary",
];

#[derive(Debug)]
pub enum DataError {
    Io(PathBuf, std::io::Error),
    Parse(PathBuf, String),
    Invalid(String),
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataError::Io(p, e) => write!(f, "{}: {e}", p.display()),
            DataError::Parse(p, e) => write!(f, "{}: {e}", p.display()),
            DataError::Invalid(s) => write!(f, "donnees invalides: {s}"),
        }
    }
}
impl std::error::Error for DataError {}

#[derive(Clone, Debug, Default)]
pub struct GameData {
    pub resources: Vec<Resource>,
    pub asteroids: Vec<AsteroidClass>,
    pub modules: Vec<Module>,
    pub hulls: Vec<Hull>,
    pub skills: Vec<Skill>,
    pub roles: Vec<Role>,
    pub traits: Vec<Trait>,
    pub diseases: Vec<Disease>,
    pub cultures: Vec<Culture>,
    pub events: Vec<EventDef>,
    pub texts: Vec<(String, Vec<String>)>,
    pub factions: Vec<Faction>,
    pub industries: Vec<Industry>,
    pub recipes: Vec<Recipe>,
    pub buildings: Vec<Building>,
    pub policies: Vec<Policy>,
}

fn read<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T, DataError> {
    let text = std::fs::read_to_string(path).map_err(|e| DataError::Io(path.to_path_buf(), e))?;
    ron::from_str(&text).map_err(|e| DataError::Parse(path.to_path_buf(), e.to_string()))
}

impl GameData {
    pub fn load(dir: &Path) -> Result<GameData, DataError> {
        let d = GameData {
            resources: read(&dir.join("resources.ron"))?,
            asteroids: read(&dir.join("worldgen/asteroids.ron"))?,
            modules: read(&dir.join("modules.ron"))?,
            hulls: read(&dir.join("hulls.ron"))?,
            skills: read(&dir.join("skills.ron"))?,
            roles: read(&dir.join("roles.ron"))?,
            traits: read(&dir.join("traits.ron"))?,
            diseases: read(&dir.join("diseases.ron"))?,
            cultures: read(&dir.join("cultures_names.ron"))?,
            events: read(&dir.join("events.ron"))?,
            texts: read(&dir.join("text/crew.ron"))?,
            factions: read(&dir.join("factions.ron"))?,
            industries: read(&dir.join("industries.ron"))?,
            recipes: read(&dir.join("recipes.ron"))?,
            buildings: read(&dir.join("buildings.ron"))?,
            policies: read(&dir.join("colony_policies.ron"))?,
        };
        d.validate()?;
        Ok(d)
    }

    pub fn skill_index(&self, id: &str) -> Option<usize> {
        self.skills.iter().position(|s| s.id == id)
    }
    pub fn disease_index(&self, id: &str) -> Option<usize> {
        self.diseases.iter().position(|s| s.id == id)
    }
    pub fn trait_index(&self, id: &str) -> Option<usize> {
        self.traits.iter().position(|s| s.id == id)
    }
    /// Template variants for a text key (empty when unknown).
    pub fn text(&self, key: &str) -> &[String] {
        self.texts
            .iter()
            .find(|(k, _)| k == key)
            .map_or(&[], |(_, v)| v.as_slice())
    }

    fn valid_field(&self, f: &str) -> bool {
        let f = f.strip_prefix("other.").unwrap_or(f);
        if let Some(sk) = f.strip_prefix("skill:") {
            return sk == "*" || self.skill_index(sk).is_some();
        }
        if let Some(t) = f.strip_prefix("trait:") {
            return self.trait_index(t).is_some();
        }
        if let Some(d) = f.strip_prefix("disease:") {
            return self.disease_index(d).is_some();
        }
        PERSON_FIELDS.contains(&f)
    }

    pub fn faction_index(&self, id: &str) -> Option<usize> {
        self.factions.iter().position(|f| f.id == id)
    }
    pub fn industry_index(&self, id: &str) -> Option<usize> {
        self.industries.iter().position(|f| f.id == id)
    }

    pub fn recipe_index(&self, id: &str) -> Option<usize> {
        self.recipes.iter().position(|r| r.id == id)
    }
    pub fn building_index(&self, id: &str) -> Option<usize> {
        self.buildings.iter().position(|r| r.id == id)
    }
    pub fn policy_index(&self, id: &str) -> Option<usize> {
        self.policies.iter().position(|r| r.id == id)
    }

    pub fn module_index(&self, id: &str) -> Option<usize> {
        self.modules.iter().position(|m| m.id == id)
    }
    pub fn hull_index(&self, id: &str) -> Option<usize> {
        self.hulls.iter().position(|h| h.id == id)
    }

    pub fn resource_index(&self, id: &str) -> Option<usize> {
        self.resources.iter().position(|r| r.id == id)
    }

    /// Fails on duplicate ids, unknown references and out-of-range values.
    pub fn validate(&self) -> Result<(), DataError> {
        let bad = |s: String| Err(DataError::Invalid(s));
        let mut ids = BTreeSet::new();
        for r in &self.resources {
            if !ids.insert(r.id.as_str()) {
                return bad(format!("ressource en double: {}", r.id));
            }
            if r.density <= 0.0 || r.value < 0.0 {
                return bad(format!("valeurs hors bornes: {}", r.id));
            }
        }
        let mut cls = BTreeSet::new();
        for a in &self.asteroids {
            if !cls.insert(a.id.as_str()) {
                return bad(format!("classe d'asteroide en double: {}", a.id));
            }
            let mut sum = 0.0;
            for (rid, f) in &a.composition {
                if !ids.contains(rid.as_str()) {
                    return bad(format!(
                        "asteroide {} reference une ressource inconnue: {rid}",
                        a.id
                    ));
                }
                if !(0.0..=1.0).contains(f) {
                    return bad(format!("fraction hors bornes dans {}", a.id));
                }
                sum += f;
            }
            if !(0.99..=1.01).contains(&sum) {
                return bad(format!("composition de {} somme a {sum}", a.id));
            }
        }
        let mut mids = BTreeSet::new();
        for m in &self.modules {
            if !mids.insert(m.id.as_str()) {
                return bad(format!("module en double: {}", m.id));
            }
            if !(1..=4).contains(&m.size) || m.mass <= 0.0 || m.hp <= 0.0 || m.cost < 0.0 {
                return bad(format!("module invalide: {}", m.id));
            }
            let produces = matches!(m.category.as_str(), "reactor" | "solar");
            if produces != (m.power > 0.0) && m.power != 0.0 {
                return bad(format!("signe de puissance incoherent: {}", m.id));
            }
        }
        let mut hids = BTreeSet::new();
        for h in &self.hulls {
            if !hids.insert(h.id.as_str()) {
                return bad(format!("coque en double: {}", h.id));
            }
            if h.slots.is_empty()
                || h.slots.iter().any(|(_, s)| !(1..=4).contains(s))
                || h.mass <= 0.0
                || h.hp <= 0.0
            {
                return bad(format!("coque invalide: {}", h.id));
            }
        }
        let mut skill_ids = BTreeSet::new();
        for k in &self.skills {
            if !skill_ids.insert(k.id.as_str()) {
                return bad(format!("competence en double: {}", k.id));
            }
        }
        for r in &self.roles {
            if !r.skill.is_empty() && !skill_ids.contains(r.skill.as_str()) {
                return bad(format!("poste {} reference une competence inconnue", r.id));
            }
        }
        if self.roles.is_empty() || self.cultures.is_empty() {
            return bad("roles ou cultures vides".into());
        }
        let mut tids = BTreeSet::new();
        for t in &self.traits {
            if !tids.insert(t.id.as_str()) {
                return bad(format!("trait en double: {}", t.id));
            }
        }
        let mut dids = BTreeSet::new();
        for d in &self.diseases {
            if !dids.insert(d.id.as_str()) {
                return bad(format!("maladie en double: {}", d.id));
            }
            if !d.cure.is_empty() && self.resource_index(&d.cure).is_none() {
                return bad(format!("maladie {}: remede inconnu {}", d.id, d.cure));
            }
            if d.duration_days <= 0.0 {
                return bad(format!("maladie {}: duree invalide", d.id));
            }
        }
        let mut eids = BTreeSet::new();
        for e in &self.events {
            if !eids.insert(e.id.as_str()) {
                return bad(format!("evenement en double: {}", e.id));
            }
            if e.weight <= 0.0 || !e.text.contains("{a}") {
                return bad(format!("evenement invalide: {}", e.id));
            }
            if e.text.contains("{b}") && !e.needs_other {
                return bad(format!("evenement {} utilise {{b}} sans needs_other", e.id));
            }
            for (f, op, _) in &e.cond {
                if (op != "<" && op != ">") || !(self.valid_field(f) || f == "skill:*") {
                    return bad(format!("evenement {}: condition invalide {f} {op}", e.id));
                }
            }
            for (f, _) in &e.effects {
                if !self.valid_field(f) {
                    return bad(format!("evenement {}: effet invalide {f}", e.id));
                }
            }
        }
        let mut fids = BTreeSet::new();
        for fc in &self.factions {
            if !fids.insert(fc.id.as_str()) {
                return bad(format!("faction en double: {}", fc.id));
            }
            if !self.cultures.iter().any(|c| c.id == fc.culture) {
                return bad(format!(
                    "faction {}: culture inconnue {}",
                    fc.id, fc.culture
                ));
            }
            for r in &fc.contraband {
                if self.resource_index(r).is_none() {
                    return bad(format!("faction {}: contrebande inconnue {r}", fc.id));
                }
            }
            if !(0.0..=1.0).contains(&fc.tax) || !(0.0..=1.0).contains(&fc.aggression) {
                return bad(format!("faction {}: valeurs hors bornes", fc.id));
            }
        }
        for fc in &self.factions {
            for o in fc.allies.iter().chain(&fc.enemies) {
                if !fids.contains(o.as_str()) {
                    return bad(format!("faction {}: relation avec {o} inconnue", fc.id));
                }
            }
            if fc.allies.iter().any(|a| fc.enemies.contains(a)) {
                return bad(format!("faction {}: allie et ennemi a la fois", fc.id));
            }
        }
        let mut iids = BTreeSet::new();
        for ind in &self.industries {
            if !iids.insert(ind.id.as_str()) {
                return bad(format!("industrie en double: {}", ind.id));
            }
            for r in ind.produces.iter().chain(&ind.consumes) {
                if self.resource_index(r).is_none() {
                    return bad(format!("industrie {}: ressource inconnue {r}", ind.id));
                }
            }
            if ind.produces.iter().any(|p| ind.consumes.contains(p)) {
                return bad(format!(
                    "industrie {}: produit et consomme la meme chose",
                    ind.id
                ));
            }
        }
        if self.industries.is_empty() {
            return bad("aucune industrie".into());
        }
        let mut rids = BTreeSet::new();
        for r in &self.recipes {
            if !rids.insert(r.id.as_str()) {
                return bad(format!("recette en double: {}", r.id));
            }
            if !["extract", "process", "grow", "data"].contains(&r.kind.as_str())
                || r.hours <= 0.0
                || r.energy_kwh < 0.0
            {
                return bad(format!("recette invalide: {}", r.id));
            }
            if r.outputs.is_empty() {
                return bad(format!("recette sans sortie: {}", r.id));
            }
            for (id, kg) in &r.inputs {
                if self.resource_index(id).is_none() || *kg <= 0.0 {
                    return bad(format!("recette {}: entree invalide {id}", r.id));
                }
            }
            if r.kind != "extract" {
                for (id, kg) in &r.outputs {
                    if self.resource_index(id).is_none() || *kg <= 0.0 {
                        return bad(format!("recette {}: sortie invalide {id}", r.id));
                    }
                }
            }
            if r.kind == "process" || r.kind == "grow" {
                let (i, o): (f64, f64) = (
                    r.inputs.iter().map(|x| x.1).sum(),
                    r.outputs.iter().map(|x| x.1).sum(),
                );
                if o > i + 1e-9 {
                    return bad(format!(
                        "recette {}: la masse n'est pas conservee ({i} -> {o})",
                        r.id
                    ));
                }
            }
        }
        let mut bids = BTreeSet::new();
        for b in &self.buildings {
            if !bids.insert(b.id.as_str()) {
                return bad(format!("batiment en double: {}", b.id));
            }
            if b.days <= 0.0
                || b.sites.is_empty()
                || b.sites.iter().any(|t| !SITE_TAGS.contains(&t.as_str()))
            {
                return bad(format!("batiment invalide: {}", b.id));
            }
            for (r, kg) in b.cost.iter().chain(&b.burns) {
                if self.resource_index(r).is_none() || *kg <= 0.0 {
                    return bad(format!("batiment {}: ressource invalide {r}", b.id));
                }
            }
            for r in &b.recipes {
                if !rids.contains(r.as_str()) {
                    return bad(format!("batiment {}: recette inconnue {r}", b.id));
                }
            }
            if !b.recipes.is_empty() && b.throughput <= 0.0 {
                return bad(format!("batiment {}: debit nul", b.id));
            }
        }
        for p in &self.policies {
            if p.levels.is_empty() || p.default_level >= p.levels.len() {
                return bad(format!("politique invalide: {}", p.id));
            }
        }
        Ok(())
    }
}

/// Locate the `data/` directory: in the cwd or next to the executable, searching upward.
pub fn find_data_dir() -> Option<PathBuf> {
    let starts = [
        std::env::current_dir().ok(),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf)),
    ];
    for s in starts.into_iter().flatten() {
        let mut p = Some(s.as_path());
        while let Some(d) = p {
            if d.join("data/resources.ron").exists() {
                return Some(d.join("data"));
            }
            p = d.parent();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")
    }

    #[test]
    fn shipped_data_is_valid() {
        let d = GameData::load(&dir()).expect("data loads");
        assert!(d.resources.len() >= 40, "{}", d.resources.len());
        assert!(d.asteroids.len() >= 4);
        assert!(d.modules.len() >= 60, "{}", d.modules.len());
        assert!(d.hulls.len() >= 15);
        assert!(d.factions.len() >= 6 && d.industries.len() >= 8);
        assert!(d.recipes.len() >= 35 && d.buildings.len() >= 25 && d.policies.len() >= 4);
        assert!(d.skills.len() >= 12 && d.traits.len() >= 30 && d.events.len() >= 25);
        assert!(d.resource_index("food").is_some() && !d.text("hire").is_empty());
    }

    #[test]
    fn mass_conservation_is_enforced_for_recipes() {
        let mut d = GameData::load(&dir()).expect("data loads");
        let i = d.recipe_index("smelt_steel").expect("recipe");
        d.recipes[i].outputs[0].1 = 5000.0;
        assert!(
            d.validate().is_err(),
            "creating mass from nothing must be rejected"
        );
        let mut d = GameData::load(&dir()).expect("data loads");
        d.buildings[0].recipes.push("nope".into());
        assert!(d.validate().is_err());
    }

    #[test]
    fn production_chains_reach_every_building_input() {
        // Every construction material and burnt fuel must be producible somewhere or be a raw resource.
        let d = GameData::load(&dir()).expect("data loads");
        let produced: std::collections::BTreeSet<&str> = d
            .recipes
            .iter()
            .flat_map(|r| r.outputs.iter().map(|o| o.0.as_str()))
            .collect();
        for b in &d.buildings {
            for (r, _) in &b.cost {
                let res = &d.resources[d.resource_index(r).expect("res")];
                let raw = [
                    "metal", "mineral", "volatile", "gas", "precious", "nuclear", "organic",
                ]
                .contains(&res.category.as_str());
                assert!(
                    raw || produced.contains(r.as_str()),
                    "{} needs {r}, which nothing produces",
                    b.id
                );
            }
        }
    }

    #[test]
    fn validation_catches_errors() {
        let mut d = GameData::load(&dir()).expect("data loads");
        d.asteroids[0].composition.push(("unobtainium".into(), 0.0));
        assert!(d.validate().is_err());
        let mut d = GameData::load(&dir()).expect("data loads");
        let dup = d.resources[0].clone();
        d.resources.push(dup);
        assert!(d.validate().is_err());
    }
}
