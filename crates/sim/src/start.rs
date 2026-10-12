//! Ways to begin a game. A `StartConfig` (galaxy seed, scenario, ship option, crew picks, rules)
//! builds a real `Sim`: the ship's design, credits, hold, crew, technologies and standing with
//! the factions all follow from `data/scenarios.ron`. Nothing here is a second rule set: a wreck
//! is the ordinary ship with its modules broken, mended by the engineers with spare parts like
//! any damage, and the survivor walks the ground with the ordinary exploration on foot.
use crate::crew::{generate_person, Person};
use crate::design::ShipDesign;
use crate::rules::Rules;
use crate::{Sim, SHIP_RADIUS};
use std::sync::Arc;
use vector_core::{hash_coord, Vec3};
use vector_data::{GameData, Scenario};
use vector_world::BodyKind;

/// Candidates offered when composing the starting crew.
pub const POOL: usize = 12;
/// Most people a start may begin with.
pub const MAX_START_CREW: usize = 8;
use vector_core::units::DAY;

/// One person picked from the pool, and the post given to them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrewPick {
    pub candidate: usize,
    pub role: usize,
}

#[derive(Clone, Debug)]
pub struct StartConfig {
    pub seed: u64,
    /// Index in `GameData::scenarios`.
    pub scenario: usize,
    /// Index in the scenario's `ships`.
    pub ship: usize,
    /// None = the default crew of the scenario.
    pub crew: Option<Vec<CrewPick>>,
    pub rules: Rules,
    /// Everything the creation flow customises beyond the scenario (see `start_custom`); the
    /// default changes nothing.
    pub custom: crate::start_custom::CustomStart,
}

impl StartConfig {
    /// The classic start: the first scenario, its first ship and its default crew.
    pub fn new(seed: u64) -> StartConfig {
        StartConfig {
            seed,
            scenario: 0,
            ship: 0,
            crew: None,
            rules: Rules::default(),
            custom: Default::default(),
        }
    }
}

/// The people one can pick from, the same for every scenario of a seed: the first ones are
/// each good at one of the skills the posts use, the rest are drawn freely.
pub fn crew_pool(data: &GameData, seed: u64) -> Vec<Person> {
    crew_pool_for(data, seed, 0)
}

/// The pool of a seed after `salt` global re-rolls (0 = the plain pool).
pub fn crew_pool_for(data: &GameData, seed: u64, salt: u32) -> Vec<Person> {
    let seed = if salt == 0 {
        seed
    } else {
        vector_core::mix64(seed ^ (u64::from(salt) << 20) ^ 0x5A17)
    };
    let post_skills: Vec<usize> = data
        .roles
        .iter()
        .filter_map(|r| data.skill_index(&r.skill))
        .collect();
    (0..POOL)
        .map(|k| {
            let focus = if k < post_skills.len() {
                post_skills[k]
            } else {
                (hash_coord(seed, k as i64, 0x57A7, 1) % data.skills.len() as u64) as usize
            };
            generate_person(data, hash_coord(seed, k as i64, 0x57A7, 2), Some(focus))
        })
        .collect()
}

/// Points a person costs in the starting crew: 1, 2 for a good specialist, 3 for an expert.
pub fn pick_cost(p: &Person) -> u32 {
    let best = p.best_skill().1;
    1 + u32::from(best >= 5.0) + u32::from(best >= 7.0)
}

/// The post that suits a person best: the one driven by their best skill.
pub fn natural_role(data: &GameData, p: &Person) -> usize {
    let skill = &data.skills[p.best_skill().0].id;
    data.roles
        .iter()
        .position(|r| r.skill == *skill)
        .unwrap_or(0)
}

/// The default crew of a scenario: for each role of its list, the best still-free candidate
/// that the points allow.
pub fn default_crew(data: &GameData, sc: &Scenario, pool: &[Person]) -> Vec<CrewPick> {
    let mut picks: Vec<CrewPick> = Vec::new();
    let mut left = sc.crew_points;
    for (n, rid) in sc.crew.iter().enumerate() {
        let Some(role) = data.roles.iter().position(|r| r.id == *rid) else {
            continue;
        };
        let skill = data.skill_index(&data.roles[role].skill);
        // Keep one point for each person still to come.
        let reserve = (sc.crew.len() - n - 1) as u32;
        let best = (0..pool.len())
            .filter(|k| !picks.iter().any(|p| p.candidate == *k))
            .filter(|&k| pick_cost(&pool[k]) + reserve <= left)
            .max_by(|&a, &b| {
                let s = |k: usize| skill.map_or(0.0, |s| pool[k].skills[s]);
                s(a).total_cmp(&s(b)).then(b.cmp(&a))
            });
        if let Some(k) = best {
            left -= pick_cost(&pool[k]);
            picks.push(CrewPick { candidate: k, role });
        }
    }
    picks
}

/// Whether a crew composition respects the points, the size and the pool.
pub fn crew_valid(
    data: &GameData,
    sc: &Scenario,
    pool: &[Person],
    picks: &[CrewPick],
) -> Result<(), &'static str> {
    crew_valid_with(data, sc.crew_points, pool, picks)
}

/// The same with a budget of `points` (the scenario's or the one the player chose).
pub fn crew_valid_with(
    data: &GameData,
    points: u32,
    pool: &[Person],
    picks: &[CrewPick],
) -> Result<(), &'static str> {
    if picks.is_empty() {
        return Err("il faut au moins une personne a bord");
    }
    if picks.len() > MAX_START_CREW {
        return Err("equipage de depart trop nombreux");
    }
    let mut cost = 0;
    for (i, p) in picks.iter().enumerate() {
        if p.candidate >= pool.len() || p.role >= data.roles.len() {
            return Err("candidat ou poste inconnu");
        }
        if picks[..i].iter().any(|q| q.candidate == p.candidate) {
            return Err("une personne ne peut etre prise qu'une fois");
        }
        cost += pick_cost(&pool[p.candidate]);
    }
    if cost > points {
        return Err("budget de points depasse");
    }
    Ok(())
}

/// The design of ship option `ship` of a scenario, and the modules its hull had no room for.
pub fn build_design(
    data: &GameData,
    sc: &Scenario,
    ship: usize,
) -> Option<(ShipDesign, Vec<String>)> {
    let (hull, fit) = sc.ships.get(ship)?;
    let mut design = ShipDesign::empty(data, data.hull_index(hull)?);
    let mut skipped = Vec::new();
    for id in fit {
        if design.install_first_fit(data, id).is_err() {
            skipped.push(id.clone());
        }
    }
    Some((design, skipped))
}

/// What the ship of an option can do, for the preview before the game starts.
pub fn ship_stats(data: &GameData, sc: &Scenario, ship: usize) -> Option<crate::design::ShipStats> {
    let (design, _) = build_design(data, sc, ship)?;
    let health = vec![1.0; design.slots.len()];
    Some(crate::design::ShipStats::compute(data, &design, &health))
}

/// How ready the ship is to leave the ground (or any orbit): what is working, what is missing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Readiness {
    /// Share of the engines' nominal thrust that works (0..1).
    pub engine: f64,
    /// Share of the reactors' nominal power that works (0..1).
    pub power: f64,
    /// Thrust over weight where the ship is (the gravity of the world it rests on or circles).
    pub twr: f64,
    pub fuel: f64,
    pub fuel_cap: f64,
    /// Spare parts in the hold, kg: what the engineers mend with.
    pub parts_kg: f64,
    /// Fuel in the hold, kg: poured into the tank with `refuel_from_hold`.
    pub hold_fuel_kg: f64,
}

impl Readiness {
    /// Whether the ship can thrust now and lift off a world: thrust over weight above 1.2.
    pub fn can_fly(&self) -> bool {
        self.engine > 0.0 && self.power > 0.0 && self.fuel > 0.0 && self.twr > 1.2
    }
}

impl Sim {
    pub fn readiness(&self) -> Readiness {
        let d = &self.data;
        let (mut thrust, mut power) = (0.0, 0.0);
        for (_, m) in self.ship.design.modules(d) {
            thrust += m.thrust;
            power += m.power.max(0.0);
        }
        let res = |id: &str| {
            d.resource_index(id)
                .map_or(0.0, |r| self.ship.cargo.kg.get(r).copied().unwrap_or(0.0))
        };
        // Gravity at the ship: of the world it rests on or the one it is nearest to.
        let g_local = self.parked.map(|p| p.0).or(self.ref_body).map_or(0.0, |b| {
            let d = (self.ship.pos - self.bodies[b].pos).length().max(1.0);
            self.bodies[b].gm / (d * d)
        });
        let weight = self.ship.mass() * g_local;
        Readiness {
            twr: if weight > 0.0 {
                self.ship.stats.thrust / weight
            } else {
                f64::INFINITY
            },
            engine: if thrust > 0.0 {
                self.ship.stats.thrust / thrust
            } else {
                0.0
            },
            power: if power > 0.0 {
                self.ship.stats.power_capacity / power
            } else {
                0.0
            },
            fuel: self.ship.fuel,
            fuel_cap: self.ship.stats.fuel_cap,
            parts_kg: res("spare_parts"),
            hold_fuel_kg: res("fuel"),
        }
    }

    /// Pour the fuel carried in the hold (salvaged, bought as cargo) into the propellant tank.
    /// Returns the kilograms moved.
    pub fn refuel_from_hold(&mut self) -> Result<f64, &'static str> {
        let data = Arc::clone(&self.data);
        let fuel = data
            .resource_index("fuel")
            .ok_or("pas de carburant dans les donnees")?;
        let room = self.ship.stats.fuel_cap - self.ship.fuel;
        if room < 1e-6 {
            return Err("le reservoir est plein");
        }
        if self.ship.cargo.kg[fuel] < 1e-6 {
            return Err("pas de carburant en soute");
        }
        let kg = self.ship.cargo.remove(&data, fuel, room);
        self.ship.fuel += kg;
        Ok(kg)
    }
}

/// The world of the system where a wreck lies: the most hospitable solid one with ground to
/// walk on (breathable first, then thick air, then any rock).
pub(crate) fn crash_body(sim: &Sim) -> Option<usize> {
    let score = |i: usize| -> Option<f64> {
        let map = sim.planet_map(i)?;
        sim.default_landing(i)?;
        let cb = &sim.system.bodies[i];
        let air = if (0.3..=3.0).contains(&map.pressure) {
            2.0
        } else if map.pressure > 0.05 {
            1.0
        } else {
            0.0
        };
        Some(crate::colony::habitability(cb) * 10.0 + air)
    };
    (0..sim.system.bodies.len())
        .filter(|&i| {
            matches!(
                sim.system.bodies[i].kind,
                BodyKind::Planet(_) | BodyKind::Moon(_)
            )
        })
        .filter_map(|i| score(i).map(|s| (i, s)))
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(i, _)| i)
}

/// Break the ship (see the module doc): engines dead, tanks dry, the rest damaged, the hold
/// and life support limping. Mending is the engineers' ordinary job with spare parts.
fn wreck_ship(sim: &mut Sim) {
    let data = Arc::clone(&sim.data);
    let slots: Vec<(usize, &str)> = sim
        .ship
        .design
        .modules(&data)
        .map(|(i, m)| (i, m.category.as_str()))
        .collect();
    for (i, cat) in slots {
        sim.ship.sys.health[i] = match cat {
            "engine" => 0.0,
            "reactor" => 0.3,
            "life_support" | "cargo" => 0.6,
            "habitat" => 0.5,
            _ => 0.35,
        };
    }
    sim.ship.refresh_stats(&data);
    sim.ship.fuel = 0.0;
    sim.ship.hull = sim.ship.stats.hull_max * 0.4;
    sim.ship.sys.battery_mj = sim.ship.stats.battery_cap * 0.1;
}

/// What the galaxy of a seed offers at the start: shown while the player chooses the seed.
#[derive(Clone, Debug, PartialEq)]
pub struct HomePreview {
    pub system: String,
    pub class: &'static str,
    pub planets: usize,
    pub moons: usize,
    pub asteroids: usize,
    pub stations: usize,
    /// Faction whose territory it is, if any.
    pub faction: Option<String>,
}

pub fn home_preview(data: &GameData, seed: u64) -> HomePreview {
    home_preview_kind(data, seed, "aleatoire").0
}

/// The home of a kind of start system (see `start_custom::home_for`) and whether the galaxy had
/// one of that kind.
pub fn home_preview_kind(data: &GameData, seed: u64, kind: &str) -> (HomePreview, bool) {
    let galaxy = vector_world::galaxy::Galaxy::new(seed);
    let (home, exact) = crate::start_custom::home_for(&galaxy, kind);
    let sys = vector_world::generate_system(home.seed);
    let count = |f: fn(&BodyKind) -> bool| sys.bodies.iter().filter(|b| f(&b.kind)).count();
    let preview = HomePreview {
        system: sys.name.clone(),
        class: sys.star_class.letter(),
        planets: count(|k| matches!(k, BodyKind::Planet(_))),
        moons: count(|k| matches!(k, BodyKind::Moon(_))),
        asteroids: count(|k| matches!(k, BodyKind::Asteroid(_))),
        stations: count(|k| matches!(k, BodyKind::Station)),
        faction: galaxy
            .territory(home.pos, data.factions.len())
            .map(|f| data.factions[f].name.clone()),
    };
    (preview, exact)
}

/// The rules a difficulty level stands for (0 gentle, 1 standard, 2 harsh): damage taken, the
/// aggression of the AIs, random events and fuel use.
pub const DIFFICULTY_NAMES: [&str; 3] = ["DOUX", "STANDARD", "RUDE"];

pub fn difficulty_rules(level: usize) -> Rules {
    let k = match level {
        0 => 0.5,
        1 => 1.0,
        _ => 1.5,
    };
    Rules {
        damage: k,
        ai_aggression: k,
        event_freq: k,
        fuel_use: if level >= 2 { 1.5 } else { 1.0 },
        ..Rules::default()
    }
}

/// The level whose rules these are, if they are exactly one of the presets.
pub fn difficulty_of(rules: &Rules) -> Option<usize> {
    (0..DIFFICULTY_NAMES.len()).find(|&l| *rules == difficulty_rules(l))
}

/// Indicative difficulty of a start, 1 (gentle) to 5 (harsh): the scenario's, moved by the rules.
pub fn difficulty_rating(sc: &Scenario, rules: &Rules) -> u8 {
    let shift: i32 = match difficulty_of(rules) {
        Some(0) => -1,
        Some(2) => 1,
        Some(_) => 0,
        None if rules.invulnerable || rules.infinite_resources => -2,
        None => 0,
    };
    (i32::from(sc.difficulty) + shift).clamp(1, 5) as u8
}

/// Whether a configuration begins as a wreck on the ground (the scenario's, unless a start place
/// was chosen).
pub fn is_wreck(data: &GameData, cfg: &StartConfig) -> bool {
    match cfg.custom.world.place {
        Some(i) => data.start.places.get(i).is_some_and(|p| p.id == "epave"),
        None => data.scenarios[cfg.scenario].wreck,
    }
}

/// Build the world a configuration describes.
pub fn new_sim_with(cfg: &StartConfig, data: Arc<GameData>) -> Result<Sim, String> {
    use crate::start_custom as sc_;
    let sc = data.scenarios.get(cfg.scenario).ok_or("scenario inconnu")?;
    let custom = &cfg.custom;
    // The ship is checked first: a design too dear for its budget is refused.
    sc_::ship_check(&data, sc, &cfg.rules, &custom.ship)?;
    let pool = crew_pool_for(&data, cfg.seed, custom.crew.salt);
    let picks = match &cfg.crew {
        Some(p) => p.clone(),
        None => default_crew(&data, sc, &pool),
    };
    crew_valid_with(&data, sc_::crew_points(&data, cfg), &pool, &picks)?;
    for e in &custom.crew.edits {
        if picks.iter().any(|p| p.candidate == e.candidate) {
            sc_::crew_edit_valid(&data, &pool[e.candidate], e)?;
        }
    }
    sc_::contacts_valid(&data, &custom.crew.contacts)?;
    if custom.world.system >= data.start.systems.len()
        || custom
            .world
            .place
            .is_some_and(|i| i >= data.start.places.len())
    {
        return Err("systeme ou lieu de depart inconnu".into());
    }
    for m in [
        custom.world.epidemics,
        custom.world.surprises,
        custom.world.provisions,
    ] {
        if !(0.0..=5.0).contains(&m) {
            return Err("frequence du monde hors de 0..5".into());
        }
    }
    if custom.save_mode > 2 {
        return Err("mode de sauvegarde inconnu".into());
    }

    let galaxy = vector_world::galaxy::Galaxy::new(cfg.seed);
    let (home, _) = sc_::home_for(&galaxy, &data.start.systems[custom.world.system].id);
    let mut sim = crate::scene::base_sim_at(galaxy, &home, Arc::clone(&data));
    sim.set_rules(cfg.rules.clone());
    // The ship.
    let mut design = match custom.ship.mode {
        sc_::ShipMode::Scenario => {
            build_design(&data, sc, cfg.ship)
                .ok_or("vaisseau inconnu")?
                .0
        }
        sc_::ShipMode::Catalog => sc_::catalog_design(&data, &custom.ship)?,
        sc_::ShipMode::Design => {
            let (bp, _) = sc_::plan_report(&data, &custom.ship.plan)?;
            let hull = sim.register_custom(&bp);
            let d = crate::shipwright::design_of(&sim.data, &bp, hull);
            // The plan stays in the shipyard's drawers: it can be refitted and copied later.
            let _ = sim.import_blueprint(&custom.ship.plan);
            d
        }
    };
    let data = Arc::clone(&sim.data);
    if custom.ship.mode != sc_::ShipMode::Scenario {
        sc_::flyable(&data, &design)?;
    }
    let sleepers = custom
        .cryo
        .members
        .iter()
        .filter(|m| picks.iter().any(|p| p.candidate == **m))
        .count();
    let pods_cost = sc_::cryo_fit(&data, &mut design, sleepers)?;
    let labs = design
        .modules(&data)
        .filter(|(_, m)| m.science > 0.0)
        .count() as u32;
    sc_::ai_valid(&data, &custom.ai, labs)?;
    sim.ship.design = design;
    sim.ship.sys = crate::design::Systems::new(&sim.ship.design, 0.0);
    sim.ship.cargo.cap_mass = 0.0;
    sim.ship.refresh_stats(&data);
    sim.ship.hull = sim.ship.stats.hull_max;
    sim.ship.fuel = sim.ship.stats.fuel_cap;
    sim.ship.sys.battery_mj = sim.ship.stats.battery_cap;
    sim.weapon_cd = vec![0.0; sim.ship.design.slots.len()];
    // Money, hold, knowledge.
    sim.credits = sc.credits;
    if pods_cost > sim.credits + 1e-6 {
        return Err(format!(
            "les caissons cryo coutent {pods_cost:.0} credits : pas assez pour les installer"
        ));
    }
    sim.credits -= pods_cost;
    for (id, kg) in &sc.cargo {
        if let Some(r) = data.resource_index(id) {
            sim.ship.cargo.add(&data, r, *kg * custom.world.provisions);
        }
    }
    for t in &sc.techs {
        if let Some(i) = data.tech_index(t) {
            sim.researched[i] = true;
        }
    }
    // Standing with the factions and the law.
    for (f, v) in &sc.reputation {
        let idx = if f == "home" {
            sim.faction
        } else {
            data.faction_index(f)
        };
        sim.change_reputation(idx, *v as f32);
    }
    if sc.wanted_days > 0.0 {
        if let Some(f) = sim.faction {
            sim.wanted_until.insert(f, sim.time + sc.wanted_days * DAY);
        }
    }
    // The crew, as picked and as edited.
    for p in &picks {
        let base = &pool[p.candidate];
        let person = match custom.crew.edit_of(p.candidate) {
            Some(e) => sc_::apply_edit(
                &data,
                base,
                e,
                vector_core::hash_coord(cfg.seed, p.candidate as i64, 0xED17, 1),
            ),
            None => base.clone(),
        };
        sim.crew.hire(&data, person, p.role, 0.0);
    }
    // Where the ship begins.
    let place = match custom.world.place {
        Some(i) => data.start.places[i].id.as_str(),
        None if sc.wreck => "epave",
        None => "station",
    };
    if place == "epave" {
        // A wreck: down on a world, the ship broken, the survivors outside.
        wreck_ship(&mut sim);
        if let Some(body) = crash_body(&sim) {
            let b = &sim.bodies[body];
            let off = Vec3::new(0.0, 0.0, -(b.radius + SHIP_RADIUS));
            sim.ship.pos = b.pos + off;
            sim.ship.vel = b.vel;
            sim.parked = Some((body, off));
            // Resting on its tail: the nose (the thrust) points away from the ground.
            sim.ship.rot = vector_core::Mat3::rot_y(std::f64::consts::PI);
            sim.update_ref();
            if let Some((cx, cy)) = sim.default_landing(body) {
                let party: Vec<usize> = (0..sim.crew.members.len()).take(1).collect();
                let _ = sim.rogue_land(body, cx, cy, party, false);
            }
        }
    } else {
        sc_::place_ship(&mut sim, place);
    }
    sc_::apply_after(&mut sim, cfg);
    let mut cfg2 = cfg.clone();
    cfg2.crew = Some(picks);
    sc_::apply_cryo(&mut sim, &cfg2)?;
    Ok(sim)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ShipInput;
    use std::path::Path;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    fn cfg(d: &GameData, id: &str, seed: u64) -> StartConfig {
        StartConfig {
            scenario: d.scenario_index(id).expect("scenario"),
            ..StartConfig::new(seed)
        }
    }

    #[test]
    fn every_ship_option_fits_its_hull_and_can_live_and_fly() {
        let d = data();
        for sc in &d.scenarios {
            for (k, (hull, _)) in sc.ships.iter().enumerate() {
                let (design, skipped) = build_design(&d, sc, k).expect("design");
                assert!(
                    skipped.is_empty(),
                    "{} / {hull}: no room for {skipped:?}",
                    sc.id
                );
                let st = ship_stats(&d, sc, k).expect("stats");
                assert!(st.thrust > 0.0, "{} / {hull}: no engine", sc.id);
                assert!(
                    st.power_capacity > st.power_peak * 0.3,
                    "{} / {hull}",
                    sc.id
                );
                assert!(st.fuel_cap > 1000.0, "{} / {hull}: no fuel", sc.id);
                assert!(st.cargo_kg > 5000.0, "{} / {hull}: no hold", sc.id);
                assert!(st.crew_cap >= 2.0, "{} / {hull}: no quarters", sc.id);
                assert!(
                    design.modules(&d).any(|(_, m)| m.life_support > 0.0),
                    "{} / {hull}: no life support",
                    sc.id
                );
            }
        }
    }

    #[test]
    fn every_scenario_builds_a_valid_sim_with_what_it_promises() {
        let d = data();
        for (i, sc) in d.scenarios.iter().enumerate() {
            let c = StartConfig {
                scenario: i,
                ..StartConfig::new(7)
            };
            let sim = new_sim_with(&c, Arc::clone(&d)).expect("builds");
            assert!((sim.credits - sc.credits).abs() < 1e-9, "{} credits", sc.id);
            assert!(sim.crew.alive().count() >= 1, "{} crew", sc.id);
            assert!(sim.crew.alive().count() <= sc.crew.len(), "{} crew", sc.id);
            let hull = d.hull_index(&sc.ships[0].0).expect("hull");
            assert_eq!(sim.ship.design.hull, hull, "{} hull", sc.id);
            for (id, kg) in &sc.cargo {
                let r = d.resource_index(id).expect("resource");
                assert!(
                    (sim.ship.cargo.kg[r] - kg).abs() < 1e-6,
                    "{}: the hold lacks {id} ({} of {kg})",
                    sc.id,
                    sim.ship.cargo.kg[r]
                );
            }
            for t in &sc.techs {
                assert!(sim.researched[d.tech_index(t).expect("tech")], "{t}");
            }
            // The world runs: a few seconds pass without panic and with the time moving.
            let mut sim = sim;
            let t0 = sim.time;
            sim.advance(5.0, &ShipInput::default());
            assert!(sim.time > t0, "{} runs", sc.id);
        }
    }

    #[test]
    fn a_scenario_is_not_the_default_one_it_changes_the_start() {
        let d = data();
        let std = new_sim_with(&cfg(&d, "voyageur", 5), Arc::clone(&d)).expect("std");
        let trader = new_sim_with(&cfg(&d, "marchand", 5), Arc::clone(&d)).expect("trader");
        let outlaw = new_sim_with(&cfg(&d, "hors_la_loi", 5), Arc::clone(&d)).expect("outlaw");
        let settler = new_sim_with(&cfg(&d, "colon", 5), Arc::clone(&d)).expect("settler");
        let explorer = new_sim_with(&cfg(&d, "explorateur", 5), Arc::clone(&d)).expect("expl");
        assert!(
            trader.credits > std.credits && trader.ship.cargo.mass > std.ship.cargo.mass * 10.0
        );
        assert!(trader.ship.stats.cargo_kg > std.ship.stats.cargo_kg * 4.0);
        assert!(outlaw
            .ship
            .design
            .modules(&d)
            .any(|(_, m)| !m.weapon.is_empty()));
        assert!(!std
            .ship
            .design
            .modules(&d)
            .any(|(_, m)| !m.weapon.is_empty()));
        let home = outlaw.faction;
        if let Some(f) = home {
            assert!(outlaw.reputation_with(Some(f)) < -20.0, "hunted at home");
            assert!(outlaw
                .wanted_until
                .get(&f)
                .is_some_and(|t| *t > outlaw.time));
            assert!(std.wanted_until.is_empty());
        }
        let errants = d.faction_index("errants").expect("errants");
        assert!(
            outlaw.reputation_with(Some(errants)) > 20.0,
            "welcome among the clans"
        );
        // The settler can found a colony straight away: the kit is complete and the permit paid.
        let mut settler = settler;
        let body = (0..settler.system.bodies.len())
            .find(|&i| {
                matches!(settler.system.bodies[i].kind, BodyKind::Planet(_))
                    && settler.colony_at(i).is_none()
            })
            .expect("a planet");
        settler.spawn_near(body, settler.system.bodies[body].radius * 1.5);
        let found = settler.found_colony(body, None);
        assert!(found.is_ok(), "the kit is whole: {found:?}");
        assert!(explorer.ship.stats.scan_range > std.ship.stats.scan_range * 100.0);
        let xeno = d.tech_index("xenology").expect("tech");
        assert!(explorer.researched[xeno] && !std.researched[xeno]);
    }

    #[test]
    fn the_standard_start_matches_the_historical_one() {
        let d = data();
        let a = crate::scene::new_sim(5, Arc::clone(&d));
        let b = new_sim_with(&StartConfig::new(5), Arc::clone(&d)).expect("standard");
        assert_eq!(a.ship.design.hull, b.ship.design.hull);
        assert_eq!(a.ship.design.slots, b.ship.design.slots);
        assert_eq!(a.credits, b.credits);
        assert_eq!(a.ship.cargo.kg, b.ship.cargo.kg);
        assert_eq!(a.here, b.here);
        assert_eq!(a.crew.alive().count(), b.crew.alive().count());
    }

    #[test]
    fn the_start_depends_on_the_choices_hull_crew_and_seed() {
        let d = data();
        let base = new_sim_with(&cfg(&d, "marchand", 3), Arc::clone(&d)).expect("base");
        let mut other = cfg(&d, "marchand", 3);
        other.ship = 1;
        let small = new_sim_with(&other, Arc::clone(&d)).expect("small");
        assert_ne!(base.ship.design.hull, small.ship.design.hull);
        assert!(small.ship.stats.cargo_kg < base.ship.stats.cargo_kg);
        // A custom crew: three people instead of the default two.
        let pool = crew_pool(&d, 3);
        let sc = &d.scenarios[d.scenario_index("marchand").expect("sc")];
        let cheap: Vec<usize> = (0..pool.len())
            .filter(|&k| pick_cost(&pool[k]) == 1)
            .collect();
        assert!(cheap.len() >= 3, "the pool has cheap hands");
        let picks: Vec<CrewPick> = cheap
            .iter()
            .take(3)
            .map(|&k| CrewPick {
                candidate: k,
                role: natural_role(&d, &pool[k]),
            })
            .collect();
        let mut c = cfg(&d, "marchand", 3);
        c.crew = Some(picks.clone());
        let three = new_sim_with(&c, Arc::clone(&d)).expect("three");
        assert_eq!(three.crew.alive().count(), 3);
        assert!(crew_valid(&d, sc, &pool, &picks).is_ok());
        // The seed changes the galaxy.
        let s2 = new_sim_with(&cfg(&d, "marchand", 4), Arc::clone(&d)).expect("seed 4");
        assert_ne!(base.here_pos, s2.here_pos);
    }

    #[test]
    fn the_crew_budget_is_enforced() {
        let d = data();
        let sc = &d.scenarios[d.scenario_index("naufrage").expect("sc")];
        let pool = crew_pool(&d, 9);
        let dear: Vec<usize> = (0..pool.len())
            .filter(|&k| pick_cost(&pool[k]) >= 2)
            .collect();
        assert!(dear.len() >= 2, "the pool has experts");
        let two: Vec<CrewPick> = dear
            .iter()
            .take(2)
            .map(|&k| CrewPick {
                candidate: k,
                role: 0,
            })
            .collect();
        assert!(
            two.iter()
                .map(|p| pick_cost(&pool[p.candidate]))
                .sum::<u32>()
                > sc.crew_points,
            "two experts cost more than the castaway's budget"
        );
        assert_eq!(
            crew_valid(&d, sc, &pool, &two),
            Err("budget de points depasse")
        );
        assert!(crew_valid(&d, sc, &pool, &[]).is_err());
        let twice = [two[0], two[0]];
        assert!(crew_valid(&d, sc, &pool, &twice).is_err());
        let mut c = cfg(&d, "naufrage", 9);
        c.crew = Some(two);
        assert!(new_sim_with(&c, Arc::clone(&d)).is_err());
        // Every default crew respects its own scenario's points and posts.
        for sc in &d.scenarios {
            let pool = crew_pool(&d, 21);
            let def = default_crew(&d, sc, &pool);
            assert_eq!(def.len(), sc.crew.len(), "{}", sc.id);
            assert!(crew_valid(&d, sc, &pool, &def).is_ok(), "{}", sc.id);
        }
    }

    #[test]
    fn the_castaway_starts_on_the_ground_without_a_ship_that_flies() {
        let d = data();
        for seed in [1u64, 2, 3, 11] {
            let sim = new_sim_with(&cfg(&d, "naufrage", seed), Arc::clone(&d)).expect("wreck");
            let r = sim.readiness();
            assert!(!r.can_fly(), "seed {seed}: the wreck must not fly");
            assert_eq!(r.engine, 0.0);
            assert_eq!(r.fuel, 0.0);
            assert_eq!(sim.credits, 0.0);
            assert!(sim.docked.is_none());
            let (body, _) = sim.parked.expect("down on a world");
            assert!(
                matches!(
                    sim.system.bodies[body].kind,
                    BodyKind::Planet(_) | BodyKind::Moon(_)
                ),
                "seed {seed}"
            );
            let walk = sim.rogue.as_ref().expect("the survivor walks the ground");
            assert!(walk.on_surface(), "seed {seed}");
            assert_eq!(walk.party.len(), 1);
            assert_eq!(sim.crew.alive().count(), 1);
            // Ordinary thrust does nothing: no engine.
            let mut s2 = sim;
            s2.rogue = None;
            s2.advance(
                20.0,
                &ShipInput {
                    fwd: 1.0,
                    ..Default::default()
                },
            );
            let alt = (s2.ship.pos - s2.bodies[body].pos).length() - s2.bodies[body].radius;
            assert!(
                alt < 100.0,
                "seed {seed}: a dead engine does not lift: {alt}"
            );
        }
    }

    #[test]
    fn the_castaway_can_reach_space_by_mending_and_refuelling_with_what_he_finds() {
        let d = data();
        let mut sim = new_sim_with(&cfg(&d, "naufrage", 2), Arc::clone(&d)).expect("wreck");
        let (body, off0) = sim.parked.expect("down");
        sim.rogue = None;
        // Flares and meteoroids are the ordinary hazards of any ship; keep the test about the repair.
        sim.rules.event_freq = 0.0;
        // What exploration brings back: spare parts and fuel, into the hold.
        for (id, kg) in [("spare_parts", 400.0), ("fuel", 600.0)] {
            let r = d.resource_index(id).expect("res");
            assert!(
                sim.ship.cargo.add(&d, r, kg) > 0.0,
                "the broken hold still holds"
            );
        }
        assert!(!sim.readiness().can_fly());
        // The tank is filled from the hold.
        let moved = sim.refuel_from_hold().expect("refuel");
        assert!(moved > 100.0 && sim.ship.fuel == moved);
        assert!(
            !sim.readiness().can_fly(),
            "fuel alone does not mend the engine"
        );
        // The engineer works shift after shift; the parts are consumed.
        let before = sim.readiness().parts_kg;
        // He forages for food and water between shifts (the hold is topped up to its start).
        let (food, water) = (
            sim.data.resource_index("food").unwrap(),
            sim.data.resource_index("water").unwrap(),
        );
        for _ in 0..120 {
            // The ground turns under the parked ship (day and night): keep the wreck in the light
            // it crashed in, the test is about the mending.
            sim.parked = Some((body, off0));
            sim.ship.cargo.kg[food] = sim.ship.cargo.kg[food].max(25.0);
            sim.ship.cargo.kg[water] = sim.ship.cargo.kg[water].max(60.0);
            for _ in 0..16 {
                sim.advance(1800.0, &ShipInput::default());
            }
            if sim.readiness().can_fly() {
                break;
            }
        }
        let r = sim.readiness();
        assert!(
            r.engine > 0.0 && r.twr > 1.2,
            "the engine is mended enough: {r:?}"
        );
        assert!(r.parts_kg < before, "mending consumes spare parts");
        assert!(r.can_fly());
        // Now thrust leaves the ground.
        sim.advance(
            60.0,
            &ShipInput {
                fwd: 1.0,
                ..Default::default()
            },
        );
        let alt = (sim.ship.pos - sim.bodies[body].pos).length() - sim.bodies[body].radius;
        assert!(sim.parked.is_none(), "the ship thrusted off");
        assert!(alt > 500.0, "the castaway reaches the sky: {alt}");
    }

    #[test]
    fn without_parts_the_wreck_stays_broken_and_an_empty_hold_gives_no_fuel() {
        let d = data();
        let mut sim = new_sim_with(&cfg(&d, "naufrage", 2), Arc::clone(&d)).expect("wreck");
        sim.rogue = None;
        sim.advance(3.0 * 86_400.0, &ShipInput::default());
        assert_eq!(sim.readiness().engine, 0.0, "no parts, no repair");
        assert!(sim.refuel_from_hold().is_err());
    }

    #[test]
    fn the_seed_preview_describes_the_real_home_system() {
        let d = data();
        let p = home_preview(&d, 5);
        let sim = crate::scene::new_sim(5, Arc::clone(&d));
        assert_eq!(p.system, sim.system.name);
        assert!(p.stations >= 1 && p.planets >= 3, "{p:?}");
        assert_ne!(home_preview(&d, 6), p, "another seed, another sky");
    }

    #[test]
    fn difficulty_presets_change_the_rules_and_the_rating() {
        let d = data();
        let sc = &d.scenarios[0];
        assert_eq!(difficulty_of(&difficulty_rules(1)), Some(1));
        assert_eq!(difficulty_rules(1), Rules::default());
        assert!(difficulty_rules(2).damage > difficulty_rules(0).damage);
        let easy = difficulty_rating(sc, &difficulty_rules(0));
        let std = difficulty_rating(sc, &difficulty_rules(1));
        let hard = difficulty_rating(sc, &difficulty_rules(2));
        assert!(easy < std && std < hard, "{easy} {std} {hard}");
        let custom = Rules {
            permadeath: true,
            ..Rules::default()
        };
        assert_eq!(difficulty_of(&custom), None);
        // The rules reach the world.
        let mut c = cfg(&d, "voyageur", 5);
        c.rules = difficulty_rules(2);
        let sim = new_sim_with(&c, Arc::clone(&d)).expect("hard");
        assert_eq!(sim.rules.damage, 1.5);
    }

    #[test]
    fn the_hold_fuel_never_overfills_the_tank() {
        let d = data();
        let mut sim = new_sim_with(&cfg(&d, "voyageur", 2), Arc::clone(&d)).expect("std");
        let fuel = d.resource_index("fuel").expect("fuel");
        sim.ship.cargo.add(&d, fuel, 500.0);
        assert!(sim.refuel_from_hold().is_err(), "a full tank takes nothing");
        sim.ship.fuel = sim.ship.stats.fuel_cap - 100.0;
        let kg = sim.refuel_from_hold().expect("top up");
        assert!((kg - 100.0).abs() < 1e-6);
        assert!((sim.ship.fuel - sim.ship.stats.fuel_cap).abs() < 1e-6);
    }
}
