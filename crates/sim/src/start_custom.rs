//! Everything the new-game flow can customise beyond the scenario (see `start`): the ship (a
//! scenario model, a catalogue hull with free fittings, or a full design with a budget), the
//! crew (names, ages, skills, traits, backgrounds, salaries, friends and rivals), the ship's AI
//! (name, personality, core, software), the world (frequencies, start system, start place) and
//! the save mode. A `CustomStart` is plain data: it is part of `StartConfig`, goes into the
//! presets and the share codes, and `new_sim_with` turns it into the real starting `Sim`.
//! Nothing here is a second rule set: budgets are checked with the numbers of the shipwright and
//! the data, and every choice ends up in ordinary state of the `Sim`.
use crate::crew::Person;
use crate::design::ShipDesign;
use crate::relations::Entity;
use crate::rules::Rules;
use crate::scene::{good_home, pick_home};
use crate::shipwright::{self, Blueprint, Report};
use crate::start::{CrewPick, StartConfig};
use crate::Sim;
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, mix64, Vec3};
use vector_data::{GameData, Scenario};
use vector_world::galaxy::{Galaxy, StarEntry};
use vector_world::{generate_system, BodyKind};

/// Points of AI: a better core and each starting program cost some.
pub const AI_POINTS: u32 = 5;
/// Skill points a crew member may be given on top of what they are (net of what is taken away).
pub const SKILL_POINTS: i32 = 6;
/// Highest skill a start may give (a person who is better keeps what they have).
pub const SKILL_CAP: f32 = 8.0;
/// Trait balance of a member: the points of the traits chosen may not exceed this (a flaw gives
/// points back).
pub const TRAIT_BUDGET: i32 = 2;
/// Same for the background (a past that burdens gives a point).
pub const BACKGROUND_BUDGET: i32 = 1;
pub const MAX_TRAITS: usize = 5;
pub const MAX_CONTACTS: usize = 4;
/// Highest weight of a task in the AI's computing budget.
pub const AI_MAX_WEIGHT: u8 = crate::mind::MAX_WEIGHT;
/// Points an implant costs in the trait balance.
pub const IMPLANT_COST: i32 = 2;
/// Credits of the reference ship below which no budget goes.
const MIN_BUDGET: f64 = 60_000.0;

pub const SAVE_MODES: [&str; 3] = ["LIBRE", "A QUAI SEULEMENT", "UN SEUL EMPLACEMENT, A QUAI"];

// ---- the data a configuration is made of -------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShipMode {
    /// A model of the scenario (the classic start).
    #[default]
    Scenario,
    /// A hull of the catalogue and the modules chosen freely.
    Catalog,
    /// A full design made with the designer, within a budget.
    Design,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShipCustom {
    pub mode: ShipMode,
    /// Hull id and, for each slot of the hull, the module id ("" = empty): `Catalog` mode.
    pub hull: String,
    pub modules: Vec<String>,
    /// The plan as text (`Blueprint::to_text`): `Design` mode.
    pub plan: String,
    pub name: String,
    pub registration: String,
    pub company: String,
    /// Credits the ship may cost; None = by difficulty (see `ship_budget`).
    pub budget: Option<f64>,
}

impl Default for ShipCustom {
    fn default() -> Self {
        ShipCustom {
            mode: ShipMode::Scenario,
            hull: String::new(),
            modules: Vec::new(),
            plan: String::new(),
            name: String::new(),
            registration: String::new(),
            company: String::new(),
            budget: None,
        }
    }
}

/// What is changed in one candidate of the pool. Everything is optional: None keeps the person.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CrewEdit {
    pub candidate: usize,
    pub name: Option<String>,
    pub culture: Option<usize>,
    pub age: Option<f32>,
    /// (skill, points added or taken away).
    pub skills: Vec<(usize, i8)>,
    /// Replaces the traits of the person.
    pub traits: Option<Vec<usize>>,
    /// Index in `GameData::start.backgrounds` (0 = none).
    pub background: usize,
    pub salary: Option<f64>,
    /// Cybernetic implants installed at the start (indices into `GameData::cybernetics`).
    pub implants: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContactSpec {
    pub faction: usize,
    pub friend: bool,
    pub name: String,
}

impl Default for ContactSpec {
    fn default() -> Self {
        ContactSpec {
            faction: 0,
            friend: true,
            name: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CrewCustom {
    pub edits: Vec<CrewEdit>,
    /// Points of the crew budget when it is not the scenario's.
    pub points: Option<u32>,
    /// Changes who the pool offers (0 = the pool of the seed): the global re-roll.
    pub salt: u32,
    pub contacts: Vec<ContactSpec>,
    /// Candidates kept when the crew is drawn again.
    pub locked: Vec<usize>,
}

impl CrewCustom {
    pub fn edit_of(&self, candidate: usize) -> Option<&CrewEdit> {
        self.edits.iter().find(|e| e.candidate == candidate)
    }
    /// The edit of a candidate, created when there is none yet.
    pub fn edit_mut(&mut self, candidate: usize) -> &mut CrewEdit {
        if let Some(i) = self.edits.iter().position(|e| e.candidate == candidate) {
            return &mut self.edits[i];
        }
        self.edits.push(CrewEdit {
            candidate,
            ..Default::default()
        });
        self.edits.last_mut().expect("just pushed")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiCustom {
    /// Empty = a name drawn from the seed.
    pub name: String,
    /// Index in `GameData::start.ai_traits` and `ai_cores`.
    pub personality: usize,
    pub core: usize,
    /// Programs installed at the start (ids in `data.mind.software`).
    pub software: Vec<String>,
    /// Initial split of the computing budget; None = the personality's.
    pub alloc: Option<[u8; 5]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorldCustom {
    /// Index in `GameData::start.systems` (0 = the galaxy's usual home).
    pub system: usize,
    /// Index in `GameData::start.places`; None = the scenario's (the station, or the wreck).
    pub place: Option<usize>,
    /// Multipliers: spontaneous epidemics, surprises of the day, the scenario's hold.
    pub epidemics: f64,
    pub surprises: f64,
    pub provisions: f64,
}

impl Default for WorldCustom {
    fn default() -> Self {
        WorldCustom {
            system: 0,
            place: None,
            epidemics: 1.0,
            surprises: 1.0,
            provisions: 1.0,
        }
    }
}

/// All the customisation of a start. The default changes nothing. `ext` holds extensions by
/// key (another system may add its own start choice without a new field here).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomStart {
    pub ship: ShipCustom,
    pub crew: CrewCustom,
    pub ai: AiCustom,
    pub world: WorldCustom,
    /// 0 free, 1 only when docked, 2 one slot, only when docked.
    pub save_mode: u8,
    pub ext: Vec<(String, String)>,
}

/// What the start leaves in the `Sim` (names, frequencies, save mode).
#[derive(Clone, Debug, PartialEq)]
pub struct Runtime {
    pub ship_name: String,
    pub registration: String,
    pub company: String,
    pub ai_name: String,
    pub personality: String,
    pub epidemics: f64,
    pub surprises: f64,
    pub save_mode: u8,
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime {
            ship_name: String::new(),
            registration: String::new(),
            company: String::new(),
            ai_name: String::new(),
            personality: String::new(),
            epidemics: 1.0,
            surprises: 1.0,
            save_mode: 0,
        }
    }
}

impl Runtime {
    /// Whether the game may be saved now.
    pub fn may_save(&self, docked: bool) -> Result<(), &'static str> {
        if self.save_mode >= 1 && !docked {
            return Err("sauvegarde impossible en dehors d'une station (mode choisi au depart)");
        }
        Ok(())
    }
}

// ---- names -------------------------------------------------------------------------------

const SHIP_WORDS: [&str; 16] = [
    "AURORE",
    "HORIZON",
    "VAGABOND",
    "LIBELLULE",
    "SENTINELLE",
    "COMETE",
    "ODYSSEE",
    "TEMPETE",
    "MIRAGE",
    "BOREALE",
    "CORSAIRE",
    "PELERIN",
    "ECLIPSE",
    "FARANDOLE",
    "PERSEVERANCE",
    "ALBATROS",
];
const COMPANY_WORDS: [&str; 10] = [
    "TRANSIT",
    "CERES",
    "ORBITAL",
    "BELTWAY",
    "HELIOS",
    "MERIDIEN",
    "VECTORIEL",
    "ASTRAL",
    "PIONNIERS",
    "KUIPER",
];
const COMPANY_KINDS: [&str; 5] = ["SARL", "ET FILS", "& CIE", "COOPERATIVE", "SA"];
const AI_NAMES: [&str; 12] = [
    "ORACLE",
    "MNEMOSYNE",
    "ATLAS",
    "HESTIA",
    "VESPER",
    "KAIROS",
    "LATTICE",
    "OBSIDIENNE",
    "ECHO",
    "PYTHIE",
    "ARIANE",
    "CASSANDRE",
];

pub fn random_ship_name(h: u64) -> String {
    SHIP_WORDS[(mix64(h) % SHIP_WORDS.len() as u64) as usize].to_string()
}
pub fn random_company(h: u64) -> String {
    format!(
        "{} {}",
        COMPANY_WORDS[(mix64(h ^ 1) % COMPANY_WORDS.len() as u64) as usize],
        COMPANY_KINDS[(mix64(h ^ 2) % COMPANY_KINDS.len() as u64) as usize]
    )
}
pub fn random_registration(h: u64) -> String {
    let l = |k: u64| (b'A' + (mix64(h ^ k) % 26) as u8) as char;
    format!("{}{}-{:04}", l(3), l(4), mix64(h ^ 5) % 10_000)
}
pub fn random_ai_name(h: u64) -> String {
    AI_NAMES[(mix64(h ^ 6) % AI_NAMES.len() as u64) as usize].to_string()
}

// ---- crew --------------------------------------------------------------------------------

/// What a trait is worth in the balance: positive for an advantage (it costs points), negative
/// for a flaw (it gives some back). Worked out from its effects, nothing is listed by hand.
pub fn trait_points(t: &vector_data::Trait) -> i32 {
    let v = 10.0 * t.effect("morale_base")
        + 8.0 * t.effect("stress_resist")
        + 10.0 * t.effect("work_speed")
        + 8.0 * t.effect("health_base")
        + 6.0 * t.effect("luck")
        - 6.0 * t.effect("temper")
        - 2.0 * t.effect("sleep_need");
    v.round() as i32
}

/// Whether the edits asked for a person respect the budgets.
pub fn crew_edit_valid(data: &GameData, p: &Person, e: &CrewEdit) -> Result<(), String> {
    let net: i32 = e.skills.iter().map(|(_, d)| i32::from(*d)).sum();
    if net > SKILL_POINTS {
        return Err(format!(
            "{net} points de competence pour {SKILL_POINTS} permis"
        ));
    }
    for (s, d) in &e.skills {
        if *s >= data.skills.len() {
            return Err("competence inconnue".into());
        }
        if d.abs() > 5 {
            return Err("un seul niveau de competence ne bouge pas de plus de 5".into());
        }
        if d > &0 && p.skills[*s] + f32::from(*d) > SKILL_CAP + 0.01 && p.skills[*s] < SKILL_CAP {
            // Clamped when applied, but asking for more than the cap is refused.
            return Err(format!(
                "plafond de {SKILL_CAP:.0} atteint en {}",
                data.skills[*s].name
            ));
        }
    }
    let mut balance = 0;
    if let Some(ts) = &e.traits {
        if ts.len() > MAX_TRAITS {
            return Err(format!("{MAX_TRAITS} traits au plus"));
        }
        for (i, t) in ts.iter().enumerate() {
            if *t >= data.traits.len() || ts[..i].contains(t) {
                return Err("trait inconnu ou en double".into());
            }
            balance += trait_points(&data.traits[*t]);
        }
    }
    if e.implants.len() > 1 {
        return Err("un seul implant au depart".into());
    }
    for i in &e.implants {
        if *i >= data.cybernetics.len() {
            return Err("implant inconnu".into());
        }
        balance += IMPLANT_COST;
    }
    if balance > TRAIT_BUDGET {
        return Err(format!(
            "equilibre des traits : {balance} points pour {TRAIT_BUDGET} permis (prenez un defaut)"
        ));
    }
    let bg = data
        .start
        .backgrounds
        .get(e.background)
        .ok_or("historique inconnu")?;
    if bg.cost > BACKGROUND_BUDGET {
        return Err("historique trop avantageux".into());
    }
    if let Some(c) = e.culture {
        if c >= data.cultures.len() {
            return Err("culture inconnue".into());
        }
    }
    if let Some(a) = e.age {
        if !(18.0..=80.0).contains(&a) {
            return Err("age entre 18 et 80 ans".into());
        }
    }
    if let Some(s) = e.salary {
        if !(0.0..=1000.0).contains(&s) {
            return Err("salaire entre 0 et 1000 credits par jour".into());
        }
    }
    Ok(())
}

/// Trait balance and skill points spent by an edit (for the screen).
pub fn edit_spent(data: &GameData, e: &CrewEdit) -> (i32, i32) {
    let skills = e.skills.iter().map(|(_, d)| i32::from(*d)).sum();
    let mut traits: i32 = e.traits.as_ref().map_or(0, |ts| {
        ts.iter().map(|&t| trait_points(&data.traits[t])).sum()
    });
    traits += IMPLANT_COST * e.implants.len() as i32;
    (skills, traits)
}

/// The person after the edit (the edit is supposed valid).
pub fn apply_edit(data: &GameData, base: &Person, e: &CrewEdit, seed: u64) -> Person {
    let mut p = base.clone();
    if let Some(c) = e.culture {
        if c != p.culture {
            p.culture = c;
            if e.name.is_none() {
                let cid = &data.cultures[c].id;
                p.name = format!(
                    "{} {}",
                    vector_world::names::given_name(
                        cid,
                        hash_coord(seed, 1, 2, 3).is_multiple_of(2),
                        seed
                    ),
                    vector_world::names::surname(cid, seed ^ 0x5A)
                );
            }
        }
    }
    if let Some(n) = &e.name {
        if !n.trim().is_empty() {
            p.name = n.trim().to_string();
        }
    }
    if let Some(a) = e.age {
        p.age = a;
    }
    let mut changed = false;
    for (s, d) in &e.skills {
        let cap = SKILL_CAP.max(p.skills[*s]);
        p.skills[*s] = (p.skills[*s] + f32::from(*d)).clamp(0.0, cap);
        changed |= *d != 0;
    }
    if let Some(bg) = data.start.backgrounds.get(e.background) {
        for (sk, v) in &bg.skills {
            if let Some(i) = data.skill_index(sk) {
                p.skills[i] = (p.skills[i] + v).clamp(0.0, SKILL_CAP.max(p.skills[i]));
                changed = true;
            }
        }
    }
    if let Some(ts) = &e.traits {
        p.traits = ts.clone();
    }
    for i in &e.implants {
        if !p.implants.contains(i) {
            p.implants.push(*i);
        }
    }
    p.salary = match e.salary {
        Some(s) => s.round(),
        None if changed => {
            let best = p.skills.iter().copied().fold(0.0f32, f32::max) as f64;
            (30.0 + 22.0 * best).round()
        }
        None => p.salary,
    };
    p
}

/// A friend or a rival at the start: a named contact, and the standing of their faction.
fn apply_contacts(sim: &mut Sim, contacts: &[ContactSpec]) {
    for c in contacts.iter().take(MAX_CONTACTS) {
        if c.faction >= sim.data.factions.len() {
            continue;
        }
        let e = Entity::contact(c.faction);
        if !c.name.trim().is_empty() {
            sim.relations.label(e, c.name.trim());
        }
        if c.friend {
            sim.relations.set(e, 0.5);
            sim.change_reputation(Some(c.faction), 8.0);
        } else {
            sim.change_reputation(Some(c.faction), -20.0);
        }
    }
}

pub fn contacts_valid(data: &GameData, contacts: &[ContactSpec]) -> Result<(), String> {
    if contacts.len() > MAX_CONTACTS {
        return Err(format!("{MAX_CONTACTS} contacts au plus"));
    }
    for (i, c) in contacts.iter().enumerate() {
        if c.faction >= data.factions.len() {
            return Err("faction inconnue".into());
        }
        if contacts[..i].iter().any(|d| d.faction == c.faction) {
            return Err("un seul contact par faction".into());
        }
    }
    Ok(())
}

// ---- the ship ----------------------------------------------------------------------------

/// Credits a custom ship may cost: 1.5 times the first model of the scenario (at least a floor),
/// scaled by the difficulty of the rules (gentle 1.5, harsh 0.7), or the budget asked for.
pub fn ship_budget(data: &GameData, sc: &Scenario, rules: &Rules, c: &ShipCustom) -> f64 {
    if let Some(b) = c.budget {
        return b;
    }
    let reference = reference_cost(data, sc);
    let mult = match crate::start::difficulty_of(rules) {
        Some(0) => 1.5,
        Some(2) => 0.7,
        _ => 1.0,
    };
    (reference * 1.5).max(MIN_BUDGET) * mult
}

fn reference_cost(data: &GameData, sc: &Scenario) -> f64 {
    sc.ships.first().map_or(0.0, |(h, fit)| {
        data.hull_index(h).map_or(0.0, |hi| data.hulls[hi].cost)
            + fit
                .iter()
                .filter_map(|m| data.module_index(m))
                .map(|mi| data.modules[mi].cost)
                .sum::<f64>()
    })
}

/// The design of a catalogue ship: the hull and a module in each slot ("" = none).
pub fn catalog_design(data: &GameData, c: &ShipCustom) -> Result<ShipDesign, String> {
    let hull = data.hull_index(&c.hull).ok_or("coque inconnue")?;
    if data.hulls[hull].id.starts_with("custom_") {
        return Err("coque sur mesure non disponible au catalogue".into());
    }
    let mut d = ShipDesign::empty(data, hull);
    for (slot, id) in c.modules.iter().enumerate() {
        if id.is_empty() {
            continue;
        }
        let mi = data
            .module_index(id)
            .ok_or(format!("module {id} inconnu"))?;
        d.install(data, slot, mi)
            .map_err(|e| format!("emplacement {slot}: {e}"))?;
    }
    Ok(d)
}

/// The modules that can go in `slot` of hull `hull` (indices into `GameData::modules`).
pub fn slot_candidates(data: &GameData, hull: usize, slot: usize) -> Vec<usize> {
    let probe = ShipDesign::empty(data, hull);
    (0..data.modules.len())
        .filter(|&mi| probe.clone().install(data, slot, mi).is_ok())
        .collect()
}

/// A catalogue ship made of a scenario's model: its hull and its modules slot by slot.
pub fn catalog_from_scenario(data: &GameData, sc: &Scenario, ship: usize) -> Option<ShipCustom> {
    let (hull, _) = sc.ships.get(ship)?;
    let (design, _) = crate::start::build_design(data, sc, ship)?;
    Some(ShipCustom {
        mode: ShipMode::Catalog,
        hull: hull.clone(),
        modules: design
            .slots
            .iter()
            .map(|m| m.map_or(String::new(), |mi| data.modules[mi].id.clone()))
            .collect(),
        ..Default::default()
    })
}

/// A loadout for another hull: the modules of `fit`, each in the first slot that takes it.
pub fn catalog_fill(data: &GameData, hull: &str, fit: &[String]) -> Option<Vec<String>> {
    let hi = data.hull_index(hull)?;
    let mut d = ShipDesign::empty(data, hi);
    for id in fit {
        let _ = d.install_first_fit(data, id);
    }
    Some(
        d.slots
            .iter()
            .map(|m| m.map_or(String::new(), |mi| data.modules[mi].id.clone()))
            .collect(),
    )
}

/// The plan of a ready-made design on sale at the yards (`data/ship_plans.ron`) as text.
pub fn plan_text_of_def(data: &GameData, def: &vector_data::ShipPlanDef) -> Option<String> {
    let mut bp = Blueprint::new(1, &def.name);
    for (p, x, y, z) in &def.cells {
        bp.place(data, p, *x, *y, *z).ok()?;
    }
    for (x, y, z, s, m) in &def.loadout {
        bp.fit(data, *x, *y, *z, *s, m).ok()?;
    }
    Some(bp.to_text())
}

/// Price of a catalogue ship: the hull and its modules.
pub fn catalog_cost(data: &GameData, c: &ShipCustom) -> Option<f64> {
    let hull = data.hull_index(&c.hull)?;
    let mut v = data.hulls[hull].cost;
    for id in c.modules.iter().filter(|m| !m.is_empty()) {
        v += data.modules[data.module_index(id)?].cost;
    }
    Some(v)
}

/// The plan of a design and what follows from it (the shipwright's own analysis).
pub fn plan_report(data: &GameData, text: &str) -> Result<(Blueprint, Report), String> {
    let bp = Blueprint::from_text(data, text)?;
    let r = shipwright::analyze(data, &bp);
    Ok((bp, r))
}

/// What a custom ship costs and whether it may be built: Err names the problem.
/// Scenario ships cost nothing (they are the scenario's gift).
pub fn ship_cost(data: &GameData, c: &ShipCustom) -> Result<f64, String> {
    match c.mode {
        ShipMode::Scenario => Ok(0.0),
        ShipMode::Catalog => {
            catalog_design(data, c)?;
            catalog_cost(data, c).ok_or_else(|| "coque inconnue".to_string())
        }
        ShipMode::Design => {
            let (_, r) = plan_report(data, &c.plan)?;
            if let Some(e) = r.errors.first() {
                return Err(format!("plan inconstructible : {e}"));
            }
            Ok(r.value)
        }
    }
}

/// Cost checked against the budget, and the ship checked for being able to live and fly.
pub fn ship_check(
    data: &GameData,
    sc: &Scenario,
    rules: &Rules,
    c: &ShipCustom,
) -> Result<f64, String> {
    let cost = ship_cost(data, c)?;
    if c.mode == ShipMode::Scenario {
        return Ok(0.0);
    }
    let budget = ship_budget(data, sc, rules, c);
    if cost > budget + 1e-6 {
        return Err(format!(
            "vaisseau trop cher : {cost:.0} credits pour un budget de {budget:.0}"
        ));
    }
    Ok(cost)
}

/// Whether a ship can live and fly: Err names what is missing (checked on the real design).
pub fn flyable(data: &GameData, design: &ShipDesign) -> Result<(), &'static str> {
    let health = vec![1.0; design.slots.len()];
    let st = crate::design::ShipStats::compute(data, design, &health);
    if st.thrust <= 0.0 {
        return Err("le vaisseau n'a aucun moteur");
    }
    if st.power_capacity <= 0.0 {
        return Err("le vaisseau n'a aucune source d'energie");
    }
    if st.fuel_cap <= 0.0 {
        return Err("le vaisseau n'a aucun reservoir");
    }
    if st.crew_cap < 1.0 {
        return Err("le vaisseau n'a aucun logement");
    }
    if !design.modules(data).any(|(_, m)| m.life_support > 0.0) {
        return Err("le vaisseau n'a aucun support de vie");
    }
    Ok(())
}

// ---- the AI ------------------------------------------------------------------------------

/// Points of AI that a starting program costs.
pub fn software_cost(s: &vector_data::Software) -> u32 {
    1 + u32::from(s.price >= 3000.0) + u32::from(s.price >= 5000.0)
}

pub fn ai_points_used(data: &GameData, a: &AiCustom) -> u32 {
    let core = data.start.ai_cores.get(a.core).map_or(0, |c| c.cost);
    core + a
        .software
        .iter()
        .filter_map(|id| data.software_index(id))
        .map(|i| software_cost(&data.mind.software[i]))
        .sum::<u32>()
}

/// Memory the programs need against what the core and the bare ship give (`labs` laboratories
/// add more once the ship is known).
pub fn ai_valid(data: &GameData, a: &AiCustom, labs: u32) -> Result<(), String> {
    let core = data
        .start
        .ai_cores
        .get(a.core)
        .ok_or("noyau d'IA inconnu")?;
    if a.personality >= data.start.ai_traits.len() {
        return Err("personnalite inconnue".into());
    }
    if ai_points_used(data, a) > AI_POINTS {
        return Err(format!(
            "{} points d'IA pour {AI_POINTS} permis",
            ai_points_used(data, a)
        ));
    }
    let mut mem = 0;
    for (i, id) in a.software.iter().enumerate() {
        let k = data
            .software_index(id)
            .ok_or(format!("logiciel {id} inconnu"))?;
        if a.software[..i].contains(id) {
            return Err("logiciel en double".into());
        }
        let s = &data.mind.software[k];
        if !s.tech.is_empty() {
            return Err(format!("{} demande une technologie", s.name));
        }
        mem += s.memory;
    }
    let have = crate::mind::BASE_MEMORY + core.memory + crate::mind::LAB_MEMORY * labs;
    if mem > have {
        return Err(format!("memoire insuffisante : {mem} pour {have}"));
    }
    if let Some(al) = a.alloc {
        if al.iter().any(|&w| w > crate::mind::MAX_WEIGHT) || al.iter().all(|&w| w == 0) {
            return Err("repartition du calcul invalide".into());
        }
    }
    Ok(())
}

fn apply_ai(sim: &mut Sim, a: &AiCustom, seed: u64) {
    let data = std::sync::Arc::clone(&sim.data);
    let tr = &data.start.ai_traits[a.personality.min(data.start.ai_traits.len() - 1)];
    let core = &data.start.ai_cores[a.core.min(data.start.ai_cores.len() - 1)];
    sim.mind.bias.copy_from_slice(&tr.bias);
    sim.mind.alloc = a.alloc.unwrap_or_else(|| {
        let mut w = [0u8; 5];
        w.copy_from_slice(&tr.alloc);
        w
    });
    sim.mind.power_bonus = core.power;
    sim.mind.memory_bonus = core.memory;
    for id in &a.software {
        if sim.software_level(id).is_none() && data.software_index(id).is_some() {
            sim.mind.software.push((id.clone(), 1));
        }
    }
    sim.credits = (sim.credits * tr.credits_mult).round();
    for p in &mut sim.crew.members {
        p.morale = (p.morale + tr.crew_morale as f32).clamp(0.0, 1.0);
    }
    sim.custom.personality = tr.id.clone();
    sim.custom.ai_name = if a.name.trim().is_empty() {
        random_ai_name(seed)
    } else {
        a.name.trim().to_uppercase()
    };
}

// ---- the world ---------------------------------------------------------------------------

/// The home system for a kind of start, and whether the galaxy really had one of that kind
/// (else the usual home is used).
pub fn home_for(galaxy: &Galaxy, kind: &str) -> (StarEntry, bool) {
    let usual = pick_home(galaxy);
    if kind == "aleatoire" {
        return (usual, true);
    }
    let ok = |e: &StarEntry| -> bool {
        if !good_home(e.seed) {
            return false;
        }
        let sys = generate_system(e.seed);
        match kind {
            "solaire" => matches!(sys.star_class.letter(), "G" | "K"),
            "monde_habitable" => sys.bodies.iter().any(|b| {
                matches!(b.kind, BodyKind::Planet(_)) && crate::colony::habitability(b) >= 0.8
            }),
            "isole" => galaxy.nearby(e.pos, 5.0).len() <= 1,
            "dense" => galaxy.nearby(e.pos, 4.0).len() >= 5,
            _ => true,
        }
    };
    for radius in [60.0, 150.0] {
        let mut near = galaxy.nearby([0.0; 3], radius);
        near.sort_by(|a, b| {
            a.distance_to([0.0; 3])
                .total_cmp(&b.distance_to([0.0; 3]))
                .then(a.seed.cmp(&b.seed))
        });
        if let Some(e) = near.iter().find(|e| ok(e)) {
            return (e.clone(), true);
        }
    }
    (usual, false)
}

/// The world to put the ship on or around: the most hospitable planet, else any planet.
fn best_world(sim: &Sim) -> Option<usize> {
    crate::start::crash_body(sim).or_else(|| {
        sim.system
            .bodies
            .iter()
            .position(|b| matches!(b.kind, BodyKind::Planet(_)))
    })
}

/// Move the ship to the start place (the station is where `base_sim` leaves it).
pub(crate) fn place_ship(sim: &mut Sim, kind: &str) {
    match kind {
        "orbite" => {
            if let Some(i) = best_world(sim) {
                let b = &sim.bodies[i];
                let r = b.radius * 1.6 + 200_000.0;
                let v = (b.gm / r).sqrt();
                sim.ship.pos = b.pos + Vec3::new(0.0, 0.0, -r);
                sim.ship.vel = b.vel + Vec3::new(v, 0.0, 0.0);
                sim.update_ref();
            }
        }
        "sol" => {
            if let Some(i) = best_world(sim) {
                let b = &sim.bodies[i];
                let off = Vec3::new(0.0, 0.0, -(b.radius + crate::SHIP_RADIUS));
                sim.ship.pos = b.pos + off;
                sim.ship.vel = b.vel;
                sim.parked = Some((i, off));
                sim.ship.rot = vector_core::Mat3::rot_y(std::f64::consts::PI);
                sim.update_ref();
            }
        }
        "espace_profond" => {
            let far = sim
                .bodies
                .iter()
                .map(|b| b.pos.length())
                .fold(0.0f64, f64::max);
            sim.ship.pos = Vec3::new(far * 1.8, 0.0, 0.0);
            sim.ship.vel = Vec3::ZERO;
            sim.update_ref();
        }
        _ => {}
    }
}

// ---- applying a configuration --------------------------------------------------------------

/// What a configuration does to the `Sim` once the ship, the hold and the crew are in place.
pub(crate) fn apply_after(sim: &mut Sim, cfg: &StartConfig) {
    let c = &cfg.custom;
    sim.custom.epidemics = c.world.epidemics;
    sim.custom.surprises = c.world.surprises;
    sim.custom.save_mode = c.save_mode;
    let data = std::sync::Arc::clone(&sim.data);
    // Backgrounds: savings, debts and standing, member by member.
    for e in &c.crew.edits {
        if hired_candidate(cfg, e.candidate) {
            if let Some(bg) = data.start.backgrounds.get(e.background) {
                sim.credits += bg.credits;
                for (f, v) in &bg.reputation {
                    let idx = if f == "home" {
                        sim.faction
                    } else {
                        data.faction_index(f)
                    };
                    sim.change_reputation(idx, *v as f32);
                }
            }
        }
    }
    sim.credits = sim.credits.max(0.0);
    apply_contacts(sim, &c.crew.contacts);
    apply_ai(sim, &c.ai, cfg.seed);
    sim.custom.ship_name = if c.ship.name.trim().is_empty() {
        random_ship_name(cfg.seed)
    } else {
        c.ship.name.trim().to_uppercase()
    };
    sim.custom.registration = if c.ship.registration.trim().is_empty() {
        random_registration(cfg.seed)
    } else {
        c.ship.registration.trim().to_uppercase()
    };
    sim.custom.company = if c.ship.company.trim().is_empty() {
        random_company(cfg.seed)
    } else {
        c.ship.company.trim().to_uppercase()
    };
}

fn hired_candidate(cfg: &StartConfig, candidate: usize) -> bool {
    cfg.crew
        .as_ref()
        .is_none_or(|p| p.iter().any(|q| q.candidate == candidate))
}

// ---- review of a configuration -----------------------------------------------------------

/// What the summary shows: costs, a difficulty from 1 to 5, and warnings (not errors).
#[derive(Clone, Debug)]
pub struct Review {
    pub ship_cost: f64,
    pub ship_budget: f64,
    pub ai_points: u32,
    pub rating: u8,
    pub warnings: Vec<String>,
    /// Why the game could not start, if it could not.
    pub error: Option<String>,
}

/// Indicative difficulty 1 (gentle) to 5 (harsh) of a whole configuration.
pub fn rating(data: &GameData, cfg: &StartConfig) -> u8 {
    let r = &cfg.rules;
    let sc = &data.scenarios[cfg.scenario.min(data.scenarios.len() - 1)];
    let mut s = f64::from(sc.difficulty);
    s += (r.damage - 1.0) * 0.8 + (r.ai_aggression - 1.0) * 0.8 + (r.event_freq - 1.0) * 0.4;
    s += (r.fuel_use - 1.0) * 0.6 + (r.build_cost - 1.0) * 0.4;
    s += (cfg.custom.world.epidemics - 1.0) * 0.4;
    s += if r.permadeath { 0.8 } else { 0.0 };
    s += f64::from(cfg.custom.save_mode) * 0.3;
    s -= if r.crew_needs { 0.0 } else { 0.8 };
    s -= if r.invulnerable { 2.0 } else { 0.0 };
    s -= if r.infinite_resources { 2.0 } else { 0.0 };
    s -= (cfg.custom.world.provisions - 1.0).clamp(-0.5, 2.0) * 0.3;
    s.round().clamp(1.0, 5.0) as u8
}

pub fn review(data: &std::sync::Arc<GameData>, cfg: &StartConfig) -> Review {
    let mut warnings = Vec::new();
    let mut error = None;
    let sc = &data.scenarios[cfg.scenario.min(data.scenarios.len() - 1)];
    let budget = ship_budget(data, sc, &cfg.rules, &cfg.custom.ship);
    let mut cost = 0.0;
    match ship_check(data, sc, &cfg.rules, &cfg.custom.ship) {
        Ok(c) => cost = c,
        Err(e) => error = Some(e),
    }
    if error.is_none() {
        if let Err(e) = crate::start::new_sim_with(cfg, std::sync::Arc::clone(data)) {
            error = Some(e);
        }
    }
    if cfg.rules.permadeath {
        warnings.push("MORT PERMANENTE : UN VAISSEAU DETRUIT MET FIN A LA PARTIE".to_string());
    }
    if cfg.custom.save_mode > 0 {
        warnings.push(format!(
            "SAUVEGARDE : {}",
            SAVE_MODES[usize::from(cfg.custom.save_mode.min(2))]
        ));
    }
    if cfg.custom.world.place == Some(2) || sc.wreck {
        warnings.push("DEPART AU SOL : IL FAUT UN POIDS-POUSSEE SUFFISANT POUR DECOLLER".into());
    }
    if cfg.custom.world.place == Some(3) {
        warnings.push("ESPACE PROFOND : LE PREMIER MONDE EST LOIN, GARDEZ DU CARBURANT".into());
    }
    if cfg.rules.creative || cfg.rules.invulnerable || cfg.rules.infinite_resources {
        warnings.push("REGLES DE TRICHE ACTIVES : LA PARTIE SERA MARQUEE MODIFIEE".into());
    }
    if cfg.custom.world.system != 0 {
        let galaxy = Galaxy::new(cfg.seed);
        let kind =
            &data.start.systems[cfg.custom.world.system.min(data.start.systems.len() - 1)].id;
        if !home_for(&galaxy, kind).1 {
            warnings.push("AUCUN SYSTEME DE CE TYPE DANS LA GALAXIE : DEPART HABITUEL".into());
        }
    }
    Review {
        ship_cost: cost,
        ship_budget: budget,
        ai_points: ai_points_used(data, &cfg.custom.ai),
        rating: rating(data, cfg),
        warnings,
        error,
    }
}

// ---- presets and codes --------------------------------------------------------------------

/// A whole creation configuration, as saved in `saves/presets/*.ron` and shared as a code.
/// The scenario is named (its index may change), the crew is a list of (candidate, post).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StartPreset {
    pub name: String,
    pub scenario: String,
    pub seed: u64,
    pub ship: usize,
    pub crew: Option<Vec<(usize, usize)>>,
    pub rules: Rules,
    pub custom: CustomStart,
}

pub fn preset_of(data: &GameData, name: &str, cfg: &StartConfig) -> StartPreset {
    StartPreset {
        name: name.to_string(),
        scenario: data.scenarios[cfg.scenario].id.clone(),
        seed: cfg.seed,
        ship: cfg.ship,
        crew: cfg
            .crew
            .as_ref()
            .map(|p| p.iter().map(|q| (q.candidate, q.role)).collect()),
        rules: cfg.rules.clone(),
        custom: cfg.custom.clone(),
    }
}

pub fn config_of(data: &GameData, p: &StartPreset) -> Result<StartConfig, String> {
    let scenario = data
        .scenario_index(&p.scenario)
        .ok_or_else(|| format!("scenario {} inconnu", p.scenario))?;
    Ok(StartConfig {
        seed: p.seed,
        scenario,
        ship: p.ship,
        crew: p.crew.as_ref().map(|c| {
            c.iter()
                .map(|&(candidate, role)| CrewPick { candidate, role })
                .collect()
        }),
        rules: p.rules.clone(),
        custom: p.custom.clone(),
    })
}

pub fn preset_to_text(p: &StartPreset) -> String {
    vector_data::ship::to_text(p)
}

pub fn preset_from_text(text: &str) -> Result<StartPreset, String> {
    vector_data::ship::from_text(text)
}

const CODE_PREFIX: &str = "VEC1-";

/// A compact text to share a configuration: a prefix, a checksum and the bytes in hexadecimal.
pub fn preset_code(p: &StartPreset) -> String {
    let bytes = postcard::to_allocvec(p).unwrap_or_default();
    let sum = checksum(&bytes);
    let mut s = format!("{CODE_PREFIX}{sum:04X}-");
    for b in &bytes {
        s.push_str(&format!("{b:02X}"));
    }
    s
}

fn checksum(bytes: &[u8]) -> u16 {
    bytes
        .iter()
        .fold(0x1234u32, |a, &b| mix64(u64::from(a) ^ u64::from(b)) as u32) as u16
}

pub fn preset_from_code(code: &str) -> Result<StartPreset, String> {
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let rest = code
        .strip_prefix(CODE_PREFIX)
        .ok_or("ce n'est pas un code de configuration")?;
    let (sum, hex) = rest.split_once('-').ok_or("code incomplet")?;
    if hex.len() % 2 != 0 || !hex.is_ascii() {
        return Err("code corrompu".into());
    }
    let bytes: Result<Vec<u8>, _> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16))
        .collect();
    let bytes = bytes.map_err(|_| "code corrompu".to_string())?;
    if u16::from_str_radix(sum, 16).ok() != Some(checksum(&bytes)) {
        return Err("somme de controle fausse : code mal recopie".into());
    }
    postcard::from_bytes(&bytes).map_err(|_| "code illisible".to_string())
}

/// Apply a difficulty preset of `data/start_options.ron` to the rules and the custom start.
pub fn apply_difficulty_preset(
    data: &GameData,
    idx: usize,
    rules: &mut Rules,
    c: &mut CustomStart,
) {
    let Some(p) = data.start.presets.get(idx) else {
        return;
    };
    *rules = Rules::default();
    c.world.epidemics = 1.0;
    c.world.surprises = 1.0;
    c.world.provisions = 1.0;
    c.save_mode = 0;
    for (k, v) in &p.settings {
        set_setting(rules, c, k, *v);
    }
}

/// One named setting of a preset (the keys are validated at load).
pub fn set_setting(r: &mut Rules, c: &mut CustomStart, key: &str, v: f64) {
    let b = v >= 0.5;
    match key {
        "fuel_use" => r.fuel_use = v,
        "crew_needs" => r.crew_needs = b,
        "permadeath" => r.permadeath = b,
        "damage" => r.damage = v,
        "build_cost" => r.build_cost = v,
        "research_speed" => r.research_speed = v,
        "ai_aggression" => r.ai_aggression = v,
        "event_freq" => r.event_freq = v,
        "reveal_map" => r.reveal_map = b,
        "infinite_resources" => r.infinite_resources = b,
        "invulnerable" => r.invulnerable = b,
        "creative" => r.creative = b,
        "max_timescale" => r.max_timescale = v,
        "tech_locks" => r.tech_locks = b,
        "epidemics" => c.world.epidemics = v,
        "surprises" => c.world.surprises = v,
        "provisions" => c.world.provisions = v,
        "save_mode" => c.save_mode = v.round().clamp(0.0, 2.0) as u8,
        _ => {}
    }
}

// ---- randomness ------------------------------------------------------------------------

/// A drawn identity for the ship, the company and the AI.
pub fn random_identity(c: &mut CustomStart, h: u64) {
    c.ship.name = random_ship_name(h);
    c.ship.registration = random_registration(h ^ 0x11);
    c.ship.company = random_company(h ^ 0x22);
    c.ai.name = random_ai_name(h ^ 0x33);
}

/// A drawn AI: personality, core and, within the points, programs.
pub fn random_ai(data: &GameData, a: &mut AiCustom, h: u64) {
    a.personality = (mix64(h) % data.start.ai_traits.len() as u64) as usize;
    a.core = (mix64(h ^ 1) % data.start.ai_cores.len() as u64) as usize;
    a.alloc = None;
    a.software.clear();
    let n = data.mind.software.len() as u64;
    for k in 0..4u64 {
        let id = data.mind.software[(mix64(h ^ (10 + k)) % n) as usize]
            .id
            .clone();
        let mut t = a.clone();
        t.software.push(id.clone());
        if !a.software.contains(&id) && ai_valid(data, &t, 0).is_ok() {
            *a = t;
        }
    }
}

/// A drawn world: start system, place, frequencies.
pub fn random_world(data: &GameData, w: &mut WorldCustom, h: u64) {
    w.system = (mix64(h) % data.start.systems.len() as u64) as usize;
    // The wreck place is a choice of its own: not drawn.
    w.place = match mix64(h ^ 1) % 5 {
        0 => None,
        1 => Some(0),
        2 => Some(1),
        3 => Some(3),
        _ => Some(0),
    };
    let pick = |k: u64| [0.5, 1.0, 1.0, 1.5, 2.0][(mix64(h ^ k) % 5) as usize];
    w.epidemics = pick(2);
    w.surprises = pick(3);
    w.provisions = [0.5, 1.0, 1.0, 1.5][(mix64(h ^ 4) % 4) as usize];
}

/// A whole drawn configuration for a seed: scenario, ship model, difficulty preset, AI, world,
/// names. The crew stays the scenario's default. Always valid.
pub fn random_config(data: &GameData, seed: u64) -> StartConfig {
    let h = mix64(seed ^ 0xA1EA);
    let mut cfg = StartConfig::new(seed);
    // A wreck is a choice for the player to make, not a draw.
    let free: Vec<usize> = (0..data.scenarios.len())
        .filter(|&i| !data.scenarios[i].wreck)
        .collect();
    cfg.scenario = free[(h % free.len() as u64) as usize];
    cfg.ship = (mix64(h ^ 1) % data.scenarios[cfg.scenario].ships.len() as u64) as usize;
    let preset = (mix64(h ^ 2) % data.start.presets.len() as u64) as usize;
    apply_difficulty_preset(data, preset, &mut cfg.rules, &mut cfg.custom);
    random_ai(data, &mut cfg.custom.ai, h ^ 3);
    random_world(data, &mut cfg.custom.world, h ^ 4);
    random_identity(&mut cfg.custom, h ^ 5);
    cfg
}

/// Whether a custom crew size and points are within what the start allows.
pub fn crew_points(data: &GameData, cfg: &StartConfig) -> u32 {
    cfg.custom
        .crew
        .points
        .unwrap_or(data.scenarios[cfg.scenario].crew_points)
}

#[cfg(test)]
#[path = "start_custom_tests.rs"]
mod tests;
