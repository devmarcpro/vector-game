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
    /// Weapon class: beam, kinetic, missile, mine, pd (point defence) or ecm.
    pub weapon: String,
    /// Damage per second for beams, per hit otherwise.
    pub damage: f64,
    /// Shots per second.
    pub rate: f64,
    /// Projectile speed (m/s).
    pub speed: f64,
    pub range: f64,
    pub ammo: String,
    pub ammo_use: f64,
    /// Missile turn rate (rad/s).
    pub guidance: f64,
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

/// A technology: researched with points, may require others, unlocks content.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tech {
    pub id: String,
    pub name: String,
    pub field: String,
    pub cost: f64,
    pub requires: Vec<String>,
    /// (kind, id) with kind in module | hull | building.
    pub unlocks: Vec<(String, String)>,
    pub start: bool,
}

/// A point of interest in a system (see `vector_world::AnomalyKind::id`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnomalyDef {
    pub id: String,
    pub name: String,
    /// Reach of its effect (m).
    pub radius: f64,
    pub hazard: String,
    pub strength: f64,
    pub research: (f64, f64),
    pub credits: (f64, f64),
    pub loot: Vec<(String, f64, f64)>,
    pub text: String,
}

/// A surface biome. A cell takes the first biome whose ranges contain it (lo <= v < hi):
/// mean annual temperature   (K), humidity hum, elevation elev (negative = sea floor)
/// and surface pressure p (bar). habit and ertility are in [0, 1].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Biome {
    pub id: String,
    pub name: String,
    pub glyph: String,
    pub color: u32,
    pub t: (f64, f64),
    pub hum: (f64, f64),
    pub elev: (f64, f64),
    pub p: (f64, f64),
    pub habit: f64,
    pub fertility: f64,
    pub text: String,
}

impl Biome {
    pub fn contains(&self, t: f64, hum: f64, elev: f64, p: f64) -> bool {
        let inside = |v: f64, r: (f64, f64)| v >= r.0 && v < r.1;
        inside(t, self.t) && inside(hum, self.hum) && inside(elev, self.elev) && inside(p, self.p)
    }
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
    pub techs: Vec<Tech>,
    pub anomalies_def: Vec<AnomalyDef>,
    pub biomes: Vec<Biome>,
}

fn read<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T, DataError> {
    let text = std::fs::read_to_string(path).map_err(|e| DataError::Io(path.to_path_buf(), e))?;
    ron::from_str(&text).map_err(|e| DataError::Parse(path.to_path_buf(), e.to_string()))
}

impl GameData {
    pub fn load(dir: &Path) -> Result<GameData, DataError> {
        let mut d = GameData {
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
            techs: read(&dir.join("techs.ron"))?,
            anomalies_def: read(&dir.join("worldgen/anomalies.ron"))?,
            biomes: read(&dir.join("biomes.ron"))?,
        };
        // Generated extension files (tools/gen_scale.py) extend the hand-written tables.
        let mut extra_events: Vec<EventDef> = read(&dir.join("events_extra.ron"))?;
        d.events.append(&mut extra_events);
        let mut extra_recipes: Vec<Recipe> = read(&dir.join("recipes_extra.ron"))?;
        d.recipes.append(&mut extra_recipes);
        let extra_texts: Vec<(String, Vec<String>)> = read(&dir.join("text/extra.ron"))?;
        for (k, mut v) in extra_texts {
            match d.texts.iter_mut().find(|(key, _)| *key == k) {
                Some((_, cur)) => cur.append(&mut v),
                None => d.texts.push((k, v)),
            }
        }
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

    pub fn tech_index(&self, id: &str) -> Option<usize> {
        self.techs.iter().position(|t| t.id == id)
    }
    /// Index of the first biome containing this cell (None only if the table has a hole, which
    /// alidate rules out).
    pub fn classify_biome(&self, t: f64, hum: f64, elev: f64, p: f64) -> Option<usize> {
        self.biomes.iter().position(|b| b.contains(t, hum, elev, p))
    }

    pub fn anomaly_def(&self, id: &str) -> Option<&AnomalyDef> {
        self.anomalies_def.iter().find(|a| a.id == id)
    }
    /// The technology that unlocks `(kind, id)`, if it is gated at all.
    pub fn unlocking_tech(&self, kind: &str, id: &str) -> Option<usize> {
        self.techs
            .iter()
            .position(|t| t.unlocks.iter().any(|(k, i)| k == kind && i == id))
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
            if !m.weapon.is_empty() {
                if !["beam", "kinetic", "missile", "mine", "pd", "ecm"].contains(&m.weapon.as_str())
                {
                    return bad(format!("arme {}: type inconnu {}", m.id, m.weapon));
                }
                if !m.ammo.is_empty() && self.resource_index(&m.ammo).is_none() {
                    return bad(format!("arme {}: munition inconnue {}", m.id, m.ammo));
                }
                if m.weapon != "ecm" && m.range <= 0.0 {
                    return bad(format!("arme {}: portee nulle", m.id));
                }
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
        let mut tids = BTreeSet::new();
        for t in &self.techs {
            if !tids.insert(t.id.as_str()) {
                return bad(format!("technologie en double: {}", t.id));
            }
            if t.cost < 0.0 {
                return bad(format!("technologie {}: cout negatif", t.id));
            }
            for (k, id) in &t.unlocks {
                let ok = match k.as_str() {
                    "module" => self.module_index(id).is_some(),
                    "hull" => self.hull_index(id).is_some(),
                    "building" => self.building_index(id).is_some(),
                    _ => false,
                };
                if !ok {
                    return bad(format!("technologie {}: deblocage inconnu {k}:{id}", t.id));
                }
            }
        }
        for t in &self.techs {
            for r in &t.requires {
                if !tids.contains(r.as_str()) {
                    return bad(format!("technologie {}: prerequis inconnu {r}", t.id));
                }
            }
        }
        // The prerequisite graph must be acyclic and everything must be reachable from the start techs.
        let mut known: BTreeSet<&str> = self
            .techs
            .iter()
            .filter(|t| t.start)
            .map(|t| t.id.as_str())
            .collect();
        loop {
            let before = known.len();
            for t in &self.techs {
                if t.requires.iter().all(|r| known.contains(r.as_str())) {
                    known.insert(t.id.as_str());
                }
            }
            if known.len() == before {
                break;
            }
        }
        if known.len() != self.techs.len() {
            let stuck: Vec<&str> = self
                .techs
                .iter()
                .map(|t| t.id.as_str())
                .filter(|i| !known.contains(i))
                .collect();
            return bad(format!("technologies inaccessibles (cycle ?): {stuck:?}"));
        }
        for a in &self.anomalies_def {
            for (r, lo, hi) in &a.loot {
                if self.resource_index(r).is_none() || lo > hi {
                    return bad(format!("anomalie {}: butin invalide {r}", a.id));
                }
            }
            if a.radius <= 0.0 || a.research.0 > a.research.1 || a.credits.0 > a.credits.1 {
                return bad(format!("anomalie {}: valeurs invalides", a.id));
            }
        }
        self.validate_biomes()
    }

    /// Ids, ranges, and a proof by sampling that every surface cell has a biome.
    fn validate_biomes(&self) -> Result<(), DataError> {
        let bad = |s: String| Err(DataError::Invalid(s));
        let mut seen = std::collections::HashSet::new();
        for b in &self.biomes {
            if !seen.insert(b.id.as_str()) {
                return bad(format!("biome {} en double", b.id));
            }
            if b.glyph.chars().count() != 1 {
                return bad(format!(
                    "biome {}: le glyphe doit etre un seul caractere",
                    b.id
                ));
            }
            if [b.t, b.hum, b.elev, b.p].iter().any(|r| r.0 >= r.1) {
                return bad(format!("biome {}: intervalle vide ou inverse", b.id));
            }
            if !(0.0..=1.0).contains(&b.habit) || !(0.0..=1.0).contains(&b.fertility) {
                return bad(format!("biome {}: habit/fertility hors de [0, 1]", b.id));
            }
        }
        // Sample every range edge, just inside it, and its middle, on each axis.
        let axis = |get: fn(&Biome) -> (f64, f64)| -> Vec<f64> {
            let mut v = vec![0.0];
            for b in &self.biomes {
                let (lo, hi) = get(b);
                v.extend([lo, (lo + hi.min(1.0e4)) / 2.0]);
                if hi < 1.0e8 {
                    v.push(hi - 1e-6);
                }
            }
            v
        };
        let (ts, hs, es, ps) = (
            axis(|b| b.t),
            axis(|b| b.hum),
            axis(|b| b.elev),
            axis(|b| b.p),
        );
        for &t in ts.iter().filter(|&&t| t >= 0.0) {
            for &h in hs.iter().filter(|&&h| (0.0..=1.0).contains(&h)) {
                for &e in es.iter().filter(|&&e| (-1.0..=1.0).contains(&e)) {
                    for &p in ps.iter().filter(|&&p| p >= 0.0) {
                        if self.classify_biome(t, h, e, p).is_none() {
                            return bad(format!(
                                "aucun biome pour t={t:.1} K, humidite={h:.2}, altitude={e:.2}, pression={p} bar"
                            ));
                        }
                    }
                }
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
    fn biome_table_must_cover_every_cell_and_have_unique_ids() {
        let d = GameData::load(&dir()).expect("data loads");
        assert!(d.biomes.len() >= 18, "{}", d.biomes.len());
        // A hole in the table is found and reported.
        let mut holed = d.clone();
        holed.biomes.retain(|b| b.id != "desert");
        let err = holed
            .validate()
            .expect_err("a missing biome leaves a hole")
            .to_string();
        assert!(err.contains("aucun biome"), "{err}");
        let mut twice = d.clone();
        twice.biomes.push(twice.biomes[0].clone());
        assert!(twice
            .validate()
            .expect_err("duplicate")
            .to_string()
            .contains("en double"));
        let mut wide = d.clone();
        wide.biomes[0].glyph = "ab".into();
        assert!(wide.validate().is_err());
        // Classification works on a few well-known cells.
        let id = |t, h, e, p| {
            d.biomes[d.classify_biome(t, h, e, p).expect("covered")]
                .id
                .clone()
        };
        assert_eq!(id(288.0, 0.5, 0.2, 1.0), "foret_temperee");
        assert_eq!(id(300.0, 0.9, 0.3, 1.0), "jungle");
        assert_eq!(id(285.0, 0.5, -0.8, 1.0), "ocean_profond");
        assert_eq!(id(150.0, 0.0, 0.1, 0.0), "plaine_rocheuse");
    }

    #[test]
    fn shipped_data_is_valid() {
        let d = GameData::load(&dir()).expect("data loads");
        assert!(d.resources.len() >= 40, "{}", d.resources.len());
        assert!(d.asteroids.len() >= 4);
        assert!(d.modules.len() >= 60, "{}", d.modules.len());
        assert!(d.hulls.len() >= 15);
        assert!(d.factions.len() >= 6 && d.industries.len() >= 8);
        assert!(
            d.techs.len() >= 60 && d.anomalies_def.len() >= 10 && d.diseases.len() >= 30,
            "{} {} {}",
            d.techs.len(),
            d.anomalies_def.len(),
            d.diseases.len()
        );
        assert!(d.recipes.len() >= 100 && d.buildings.len() >= 25 && d.policies.len() >= 4);
        let defences = d.modules.iter().filter(|m| !m.weapon.is_empty()).count();
        assert!(defences >= 20, "{defences}");
        assert!(d.skills.len() >= 12 && d.traits.len() >= 30 && d.events.len() >= 150);
        assert!(d.resource_index("food").is_some() && !d.text("hire").is_empty());
        let variants: usize = d.texts.iter().map(|(_, v)| v.len()).sum();
        assert!(variants >= 300, "{variants} text templates");
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
    fn tech_graph_is_checked_for_cycles_and_unlocks_exist() {
        let mut d = GameData::load(&dir()).expect("data loads");
        let a = d.tech_index("plasma_physics").expect("tech");
        let b = d.tech_index("spatial_theory").expect("tech");
        let (ida, idb) = (d.techs[a].id.clone(), d.techs[b].id.clone());
        d.techs[a].requires.push(idb);
        d.techs[b].requires.push(ida);
        assert!(
            d.validate().is_err(),
            "a prerequisite cycle must be rejected"
        );
        let mut d = GameData::load(&dir()).expect("data loads");
        d.techs[1].unlocks.push(("module".into(), "nope".into()));
        assert!(d.validate().is_err());
        let d = GameData::load(&dir()).expect("data loads");
        assert!(d.unlocking_tech("module", "fusion_mk3").is_some());
        assert!(
            d.unlocking_tech("module", "fusion_mk1").is_none(),
            "tier 1 is free"
        );
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
