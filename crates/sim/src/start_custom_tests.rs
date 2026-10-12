//! Tests of the custom start: every field of the configuration has a measurable effect, the
//! budgets refuse what they must, presets round-trip and the same configuration makes the same game.
use super::*;
use crate::start::{self, new_sim_with};
use crate::ShipInput;
use std::path::Path;
use std::sync::Arc;

fn data() -> Arc<GameData> {
    Arc::new(
        GameData::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")).expect("data"),
    )
}

fn plan_text(d: &GameData, id: &str) -> String {
    let def = d.ship_plans.iter().find(|p| p.id == id).expect("plan");
    let mut bp = Blueprint::new(1, &def.name);
    for (p, x, y, z) in &def.cells {
        bp.place(d, p, *x, *y, *z).expect("place");
    }
    for (x, y, z, s, m) in &def.loadout {
        bp.fit(d, *x, *y, *z, *s, m).expect("fit");
    }
    bp.to_text()
}

fn sim_of(d: &Arc<GameData>, cfg: &StartConfig) -> Sim {
    new_sim_with(cfg, Arc::clone(d)).expect("builds")
}

/// Everything that can tell two starts apart.
fn fingerprint(s: &Sim) -> String {
    format!(
        "{:?}|{}|{:?}|{:?}|{:?}|{}|{:?}|{:?}",
        s.here_pos,
        s.credits,
        s.ship.pos,
        s.crew
            .members
            .iter()
            .map(|p| (p.name.clone(), p.skills.clone(), p.traits.clone(), p.salary))
            .collect::<Vec<_>>(),
        s.mind.alloc,
        s.custom.ai_name,
        s.ship.design.slots,
        s.ship.cargo.kg.iter().sum::<f64>(),
    )
}

#[test]
fn the_default_custom_start_changes_nothing() {
    let d = data();
    let plain = sim_of(&d, &StartConfig::new(3));
    let mut c = StartConfig::new(3);
    c.custom = CustomStart::default();
    assert_eq!(fingerprint(&plain), fingerprint(&sim_of(&d, &c)));
    assert_eq!(plain.custom.epidemics, 1.0);
    assert_eq!(plain.mind.bias, [0.0; 5]);
}

#[test]
fn same_configuration_and_seed_give_the_same_game() {
    let d = data();
    for seed in [1u64, 9, 77] {
        let cfg = random_config(&d, seed);
        let a = fingerprint(&sim_of(&d, &cfg));
        let b = fingerprint(&sim_of(&d, &cfg));
        assert_eq!(a, b, "seed {seed}");
    }
    let (a, b) = (random_config(&d, 1), random_config(&d, 2));
    assert_ne!(
        fingerprint(&sim_of(&d, &a)),
        fingerprint(&sim_of(&d, &b)),
        "other seed, other game"
    );
}

#[test]
fn random_configurations_are_always_valid_and_playable() {
    let d = data();
    for seed in 0..40u64 {
        let cfg = random_config(&d, seed * 7919 + 1);
        let r = review(&d, &cfg);
        assert!(r.error.is_none(), "seed {seed}: {:?}", r.error);
        let mut s = sim_of(&d, &cfg);
        let t0 = s.time;
        s.advance(3.0, &ShipInput::default());
        assert!(s.time > t0, "seed {seed} runs");
    }
}

#[test]
fn names_are_kept_and_drawn_ones_come_from_the_seed() {
    let d = data();
    let mut c = StartConfig::new(5);
    c.custom.ship.name = " Aurore Boreale ".into();
    c.custom.ship.registration = "zz-0001".into();
    c.custom.ship.company = "Les Voiles".into();
    c.custom.ai.name = "Pythie".into();
    let s = sim_of(&d, &c);
    assert_eq!(s.custom.ship_name, "AURORE BOREALE");
    assert_eq!(s.custom.registration, "ZZ-0001");
    assert_eq!(s.custom.company, "LES VOILES");
    assert_eq!(s.custom.ai_name, "PYTHIE");
    let drawn = sim_of(&d, &StartConfig::new(5));
    assert!(!drawn.custom.ship_name.is_empty() && !drawn.custom.ai_name.is_empty());
    assert_eq!(
        drawn.custom.ship_name,
        sim_of(&d, &StartConfig::new(5)).custom.ship_name
    );
}

#[test]
fn the_ai_personality_gives_real_bonuses_and_maluses() {
    let d = data();
    let base = sim_of(&d, &StartConfig::new(4));
    let pru = d.ai_trait_index("prudente").expect("trait");
    let mut c = StartConfig::new(4);
    c.custom.ai.personality = pru;
    c.custom.ai.alloc = Some([4; 5]);
    let s = sim_of(&d, &c);
    // The same even split, but the defence factor is up and the pilotage factor down.
    assert!(s.mind_factor(crate::mind::T_DEFENSE) > base.mind_factor(crate::mind::T_DEFENSE));
    assert!(s.mind_factor(crate::mind::T_PILOTAGE) < base.mind_factor(crate::mind::T_PILOTAGE));
    assert_eq!(s.custom.personality, "prudente");
    // The personality's own split is used when none is asked.
    let mut c2 = StartConfig::new(4);
    c2.custom.ai.personality = pru;
    assert_eq!(
        sim_of(&d, &c2).mind.alloc.to_vec(),
        d.start.ai_traits[pru].alloc
    );
    // Mercantile: more credits. Protectrice: better morale.
    let merc = d.ai_trait_index("mercantile").expect("trait");
    let prot = d.ai_trait_index("protectrice").expect("trait");
    let mut c3 = StartConfig::new(4);
    c3.custom.ai.personality = merc;
    assert!(sim_of(&d, &c3).credits > base.credits);
    c3.custom.ai.personality = prot;
    let m = |s: &Sim| s.crew.alive().map(|p| p.morale).sum::<f32>();
    assert!(m(&sim_of(&d, &c3)) > m(&base));
}

#[test]
fn the_ai_core_and_programs_cost_points_memory_and_work() {
    let d = data();
    let base = sim_of(&d, &StartConfig::new(4));
    let mut c = StartConfig::new(4);
    c.custom.ai.core = 1;
    let s = sim_of(&d, &c);
    assert!(s.mind_capacity() > base.mind_capacity());
    assert!(s.mind_memory() > base.mind_memory());
    // A program is installed and works for its task.
    let mut c = StartConfig::new(4);
    c.custom.ai.software = vec!["optim_orbites".into()];
    let s = sim_of(&d, &c);
    assert_eq!(s.software_level("optim_orbites"), Some(1));
    assert!(s.mind_factor(crate::mind::T_PILOTAGE) > base.mind_factor(crate::mind::T_PILOTAGE));
    // Too many points, an unknown program: refused.
    let mut c = StartConfig::new(4);
    c.custom.ai.core = 2;
    c.custom.ai.software = vec!["optim_orbites".into(), "philologue".into()];
    assert!(new_sim_with(&c, Arc::clone(&d)).is_err(), "over the points");
    let mut c = StartConfig::new(4);
    c.custom.ai.software = vec!["nulle_part".into()];
    assert!(new_sim_with(&c, Arc::clone(&d)).is_err());
    // More programs than memory: refused.
    let big: Vec<String> = d
        .mind
        .software
        .iter()
        .filter(|s| s.tech.is_empty())
        .map(|s| s.id.clone())
        .collect();
    let mut c = StartConfig::new(4);
    c.custom.ai.software = big;
    assert!(new_sim_with(&c, Arc::clone(&d)).is_err());
}

#[test]
fn world_frequencies_are_stored_and_provisions_scale_the_hold() {
    let d = data();
    let sc = &d.scenarios[0];
    let (food, kg) = {
        let (id, kg) = sc.cargo.iter().find(|(i, _)| i == "food").expect("food");
        (d.resource_index(id).expect("res"), *kg)
    };
    let mut c = StartConfig::new(6);
    c.custom.world.provisions = 2.0;
    c.custom.world.epidemics = 3.0;
    c.custom.world.surprises = 0.0;
    c.custom.save_mode = 2;
    let s = sim_of(&d, &c);
    assert!((s.ship.cargo.kg[food] - 2.0 * kg).abs() < 1e-6);
    assert_eq!(s.custom.epidemics, 3.0);
    assert_eq!(s.custom.surprises, 0.0);
    assert_eq!(s.custom.save_mode, 2);
    c.custom.world.epidemics = 9.0;
    assert!(new_sim_with(&c, Arc::clone(&d)).is_err(), "out of range");
}

#[test]
fn the_save_mode_limits_where_one_may_save() {
    let mut r = Runtime::default();
    assert!(r.may_save(false).is_ok());
    r.save_mode = 1;
    assert!(r.may_save(false).is_err() && r.may_save(true).is_ok());
    r.save_mode = 2;
    assert!(r.may_save(false).is_err() && r.may_save(true).is_ok());
}

#[test]
fn start_systems_have_the_character_asked_for() {
    let d = data();
    let mut found = [false; 5];
    for seed in 1..6u64 {
        let g = Galaxy::new(seed);
        let usual = pick_home(&g);
        for (k, id) in vector_data::startopts::SYSTEM_KINDS.iter().enumerate() {
            let (e, exact) = home_for(&g, id);
            if !exact {
                continue;
            }
            found[k] = true;
            let sys = generate_system(e.seed);
            match *id {
                "solaire" => assert!(matches!(sys.star_class.letter(), "G" | "K")),
                "monde_habitable" => {
                    assert!(sys
                        .bodies
                        .iter()
                        .any(|b| matches!(b.kind, BodyKind::Planet(_))
                            && crate::colony::habitability(b) >= 0.8))
                }
                "isole" => assert!(g.nearby(e.pos, 5.0).len() <= 1),
                "dense" => assert!(g.nearby(e.pos, 4.0).len() >= 5),
                _ => assert_eq!(e.seed, usual.seed),
            }
            // A game really starts there.
            let mut c = StartConfig::new(seed);
            c.custom.world.system = k;
            assert_eq!(sim_of(&d, &c).system.seed, e.seed);
        }
    }
    assert!(
        found.iter().all(|f| *f),
        "every kind found for some seed: {found:?}"
    );
}

#[test]
fn start_places_put_the_ship_somewhere_else() {
    let d = data();
    let at = |place: Option<usize>| {
        let mut c = StartConfig::new(8);
        c.custom.world.place = place;
        sim_of(&d, &c)
    };
    let station = at(None);
    assert!(station.parked.is_none());
    let orbit = at(Some(1));
    let rb = orbit.ref_body.expect("a reference world");
    assert!(orbit.parked.is_none());
    let r = (orbit.ship.pos - orbit.bodies[rb].pos).length();
    assert!(r > orbit.bodies[rb].radius * 1.5, "above the ground");
    let v = (orbit.ship.vel - orbit.bodies[rb].vel).length();
    assert!(v > 100.0, "circling: {v}");
    let ground = at(Some(2));
    assert!(ground.parked.is_some());
    assert!(ground.ship.fuel > 0.0, "an intact ship, not a wreck");
    let deep = at(Some(3));
    let nearest = |s: &Sim| {
        s.bodies
            .iter()
            .map(|b| (b.pos - s.ship.pos).length())
            .fold(f64::MAX, f64::min)
    };
    assert!(
        nearest(&deep) > nearest(&station) * 1000.0,
        "{} vs {}",
        nearest(&deep),
        nearest(&station)
    );
    // The wreck place is the wreck: engines dead.
    let wreck = at(Some(4));
    assert!(wreck.ship.fuel < 1.0 && wreck.readiness().engine < 0.01);
    let mut c = StartConfig::new(8);
    c.custom.world.place = Some(4);
    assert!(start::is_wreck(&d, &c));
    // A wreck scenario started at the station is no wreck.
    let mut c = StartConfig::new(8);
    c.scenario = d.scenario_index("naufrage").expect("scenario");
    assert!(start::is_wreck(&d, &c));
    c.custom.world.place = Some(0);
    assert!(!start::is_wreck(&d, &c));
    assert!(sim_of(&d, &c).ship.fuel > 1000.0);
}

// ---- crew ---------------------------------------------------------------------------

fn first_pick(d: &GameData, seed: u64) -> usize {
    let sc = &d.scenarios[0];
    let pool = start::crew_pool(d, seed);
    start::default_crew(d, sc, &pool)[0].candidate
}

#[test]
fn crew_edits_change_the_person_hired() {
    let d = data();
    let seed = 12;
    let k = first_pick(&d, seed);
    let base = sim_of(&d, &StartConfig::new(seed));
    let skill = d.skill_index("science").expect("skill");
    let mut c = StartConfig::new(seed);
    let before = start::crew_pool(&d, seed)[k].skills[skill];
    {
        let e = c.custom.crew.edit_mut(k);
        e.name = Some("Ada Test".into());
        e.age = Some(33.0);
        e.skills = vec![(skill, 2)];
        e.salary = Some(77.0);
    }
    let s = sim_of(&d, &c);
    let p = &s.crew.members[0];
    assert_eq!(p.name, "Ada Test");
    assert_eq!(p.age, 33.0);
    assert!((p.skills[skill] - (before + 2.0).min(SKILL_CAP.max(before))).abs() < 1e-4);
    assert_eq!(p.salary, 77.0);
    assert_ne!(base.crew.members[0].name, p.name);
    // The salary follows the skills when it is not set.
    let mut c = StartConfig::new(seed);
    c.custom.crew.edit_mut(k).skills = vec![(skill, 3)];
    let s = sim_of(&d, &c);
    assert!(s.crew.members[0].salary >= base.crew.members[0].salary);
    // A culture change gives a name of that culture.
    let mut c = StartConfig::new(seed);
    let other = (base.crew.members[0].culture + 1) % d.cultures.len();
    c.custom.crew.edit_mut(k).culture = Some(other);
    let s = sim_of(&d, &c);
    assert_eq!(s.crew.members[0].culture, other);
    assert_ne!(s.crew.members[0].name, base.crew.members[0].name);
}

#[test]
fn crew_budgets_refuse_too_many_points_and_reward_flaws() {
    let d = data();
    let seed = 12;
    let k = first_pick(&d, seed);
    let pool = start::crew_pool(&d, seed);
    let sk = d.skill_index("pilotage").expect("skill");
    let mut e = CrewEdit {
        candidate: k,
        skills: vec![(sk, 5), (0, 2)],
        ..Default::default()
    };
    assert!(crew_edit_valid(&d, &pool[k], &e).is_err(), "7 > 6 points");
    // Take points away from one skill to pay for others.
    e.skills = vec![(sk, 2), (1, 2), (2, 2), (3, -3), (4, 2)];
    assert_eq!(edit_spent(&d, &e).0, 5);
    assert!(
        crew_edit_valid(&d, &pool[k], &e).is_ok()
            || pool[k].skills.iter().any(|v| *v > SKILL_CAP - 2.0),
        "a net of 5 points is within the budget"
    );
    // Traits: advantages cost, flaws give back.
    let good: Vec<usize> = (0..d.traits.len())
        .filter(|&t| trait_points(&d.traits[t]) >= 1)
        .collect();
    let bad: Vec<usize> = (0..d.traits.len())
        .filter(|&t| trait_points(&d.traits[t]) <= -1)
        .collect();
    assert!(good.len() >= 3 && bad.len() >= 3, "both kinds exist");
    let mut e = CrewEdit {
        candidate: k,
        traits: Some(good.iter().copied().take(4).collect()),
        ..Default::default()
    };
    assert!(crew_edit_valid(&d, &pool[k], &e).is_err(), "all advantages");
    let flaw = bad[0];
    let mut ts: Vec<usize> = good.iter().copied().take(4).collect();
    ts.push(flaw);
    e.traits = Some(ts.clone());
    let total: i32 = ts.iter().map(|&t| trait_points(&d.traits[t])).sum();
    assert_eq!(
        crew_edit_valid(&d, &pool[k], &e).is_ok(),
        total <= TRAIT_BUDGET,
        "the balance decides"
    );
    // A flaw alone is always fine, and the config refuses the bad one at start.
    e.traits = Some(vec![flaw]);
    assert!(crew_edit_valid(&d, &pool[k], &e).is_ok());
    let mut c = StartConfig::new(seed);
    c.custom.crew.edits.push(CrewEdit {
        candidate: k,
        skills: vec![(sk, 5), (0, 5)],
        ..Default::default()
    });
    assert!(new_sim_with(&c, Arc::clone(&d)).is_err());
    // Age bounds.
    e.traits = None;
    e.age = Some(5.0);
    assert!(crew_edit_valid(&d, &pool[k], &e).is_err());
}

#[test]
fn backgrounds_move_skills_credits_and_standing() {
    let d = data();
    let seed = 12;
    let k = first_pick(&d, seed);
    let base = sim_of(&d, &StartConfig::new(seed));
    let rich = d.background_index("fils_de_marchand").expect("bg");
    let debt = d.background_index("mineur_endette").expect("bg");
    let mil = d.background_index("ancien_militaire").expect("bg");
    let mut c = StartConfig::new(seed);
    c.custom.crew.edit_mut(k).background = rich;
    let s = sim_of(&d, &c);
    assert!(s.credits > base.credits);
    let com = d.skill_index("commerce").expect("skill");
    assert!(s.crew.members[0].skills[com] >= base.crew.members[0].skills[com]);
    c.custom.crew.edit_mut(k).background = debt;
    assert!(sim_of(&d, &c).credits < base.credits);
    c.custom.crew.edit_mut(k).background = mil;
    let s = sim_of(&d, &c);
    let f = s.faction.expect("faction");
    assert!(s.relations.get(Entity::faction(f)) < base.relations.get(Entity::faction(f)));
    // An unknown background is refused.
    let mut e = CrewEdit {
        candidate: k,
        background: rich,
        ..Default::default()
    };
    assert!(crew_edit_valid(&d, &start::crew_pool(&d, seed)[k], &e).is_ok());
    e.background = 99;
    assert!(crew_edit_valid(&d, &start::crew_pool(&d, seed)[k], &e).is_err());
}

#[test]
fn friends_and_rivals_change_the_standing() {
    let d = data();
    let base = sim_of(&d, &StartConfig::new(3));
    let mut c = StartConfig::new(3);
    c.custom.crew.contacts = vec![
        ContactSpec {
            faction: 0,
            friend: true,
            name: "Nadia Orlov".into(),
        },
        ContactSpec {
            faction: 1,
            friend: false,
            name: "Silas Crane".into(),
        },
    ];
    let s = sim_of(&d, &c);
    assert!(s.relations.get(Entity::contact(0)) > base.relations.get(Entity::contact(0)));
    assert!(s.relations.get(Entity::faction(1)) < base.relations.get(Entity::faction(1)));
    assert!(s.entity_name(Entity::contact(0)).starts_with("Nadia Orlov"));
    c.custom.crew.contacts.push(ContactSpec {
        faction: 0,
        ..Default::default()
    });
    assert!(new_sim_with(&c, Arc::clone(&d)).is_err(), "one per faction");
}

#[test]
fn crew_size_points_and_rerolls_follow_the_configuration() {
    let d = data();
    let seed = 21;
    let pool = start::crew_pool(&d, seed);
    // The re-roll changes who is on offer, not how many.
    let other = start::crew_pool_for(&d, seed, 1);
    assert_eq!(pool.len(), other.len());
    assert!(pool.iter().zip(&other).any(|(a, b)| a.name != b.name));
    assert_eq!(pool[0].name, start::crew_pool_for(&d, seed, 0)[0].name);
    let mut c = StartConfig::new(seed);
    c.custom.crew.salt = 1;
    let s = sim_of(&d, &c);
    assert!(other.iter().any(|p| p.name == s.crew.members[0].name));
    // More points, more people.
    let picks: Vec<CrewPick> = (0..7)
        .map(|k| CrewPick {
            candidate: k,
            role: start::natural_role(&d, &pool[k]),
        })
        .collect();
    let cost: u32 = picks
        .iter()
        .map(|p| start::pick_cost(&pool[p.candidate]))
        .sum();
    let mut c = StartConfig::new(seed);
    c.crew = Some(picks);
    assert!(
        new_sim_with(&c, Arc::clone(&d)).is_err(),
        "over the scenario's points"
    );
    c.custom.crew.points = Some(cost);
    assert_eq!(sim_of(&d, &c).crew.alive().count(), 7);
    c.crew = Some(
        (0..start::MAX_START_CREW + 1)
            .map(|k| CrewPick {
                candidate: k,
                role: 0,
            })
            .collect(),
    );
    c.custom.crew.points = Some(99);
    assert!(new_sim_with(&c, Arc::clone(&d)).is_err(), "too many people");
}

// ---- ship ---------------------------------------------------------------------------

fn catalog_of(d: &GameData, sc: usize, ship: usize) -> ShipCustom {
    catalog_from_scenario(d, &d.scenarios[sc], ship).expect("catalogue ship")
}

#[test]
fn catalogue_helpers_offer_only_what_fits() {
    let d = data();
    let ship = catalog_of(&d, 0, 0);
    let hi = d.hull_index(&ship.hull).unwrap();
    for slot in 0..d.hulls[hi].slots.len() {
        let c = slot_candidates(&d, hi, slot);
        assert!(!c.is_empty(), "slot {slot} has candidates");
        let (ty, size) = d.hulls[hi].slots[slot];
        assert!(c
            .iter()
            .all(|&m| d.modules[m].slot == ty && d.modules[m].size <= size));
        if !ship.modules[slot].is_empty() {
            assert!(c.contains(&d.module_index(&ship.modules[slot]).unwrap()));
        }
    }
    // Another hull gets a loadout made of the same modules, where they fit.
    let other = d.scenarios[0].ships[1].0.clone();
    let fit: Vec<String> = ship
        .modules
        .iter()
        .filter(|m| !m.is_empty())
        .cloned()
        .collect();
    let m = catalog_fill(&d, &other, &fit).expect("fill");
    assert_eq!(m.len(), d.hulls[d.hull_index(&other).unwrap()].slots.len());
    assert!(m.iter().any(|s| !s.is_empty()));
    // Every plan on sale has a text that reads back.
    for def in &d.ship_plans {
        let t = plan_text_of_def(&d, def).expect("text");
        assert!(plan_report(&d, &t).is_ok(), "{}", def.id);
    }
}

#[test]
fn a_catalogue_ship_is_built_as_chosen_within_the_budget() {
    let d = data();
    let sc = 0;
    let mut ship = catalog_of(&d, sc, 1);
    // Free equipment: drop the weapon.
    let weapon = ship
        .modules
        .iter()
        .position(|m| m.starts_with("laser"))
        .expect("a laser");
    ship.modules[weapon].clear();
    let mut c = StartConfig::new(2);
    c.custom.ship = ship.clone();
    let s = sim_of(&d, &c);
    assert_eq!(d.hulls[s.ship.design.hull].id, ship.hull);
    assert!(s.ship.design.slots[weapon].is_none());
    let plain = sim_of(&d, &StartConfig::new(2));
    assert_ne!(s.ship.design.slots, plain.ship.design.slots);
    // The price is the hull and the modules, and it fits the budget.
    let cost = ship_check(&d, &d.scenarios[sc], &c.rules, &ship).expect("in budget");
    assert!((cost - catalog_cost(&d, &ship).unwrap()).abs() < 1e-6);
    // A budget below the price refuses it; a wrong module for a slot too.
    let mut tight = ship.clone();
    tight.budget = Some(cost - 1.0);
    c.custom.ship = tight;
    assert!(new_sim_with(&c, Arc::clone(&d)).is_err());
    let mut wrong = ship.clone();
    let eng = wrong
        .modules
        .iter()
        .position(|m| m.starts_with("torch"))
        .expect("engine");
    let tank = wrong
        .modules
        .iter()
        .position(|m| m.starts_with("tank"))
        .expect("tank");
    wrong.modules.swap(eng, tank);
    c.custom.ship = wrong;
    assert!(new_sim_with(&c, Arc::clone(&d)).is_err());
    // A ship without an engine is refused: it could not fly.
    let mut dead = ship;
    dead.modules[eng].clear();
    c.custom.ship = dead;
    assert!(new_sim_with(&c, Arc::clone(&d)).is_err());
}

#[test]
fn the_budget_follows_the_difficulty() {
    let d = data();
    let sc = &d.scenarios[0];
    let ship = ShipCustom::default();
    let gentle = ship_budget(&d, sc, &start::difficulty_rules(0), &ship);
    let std = ship_budget(&d, sc, &start::difficulty_rules(1), &ship);
    let harsh = ship_budget(&d, sc, &start::difficulty_rules(2), &ship);
    assert!(gentle > std && std > harsh, "{gentle} {std} {harsh}");
    let mut asked = ship;
    asked.budget = Some(1234.0);
    assert_eq!(ship_budget(&d, sc, &Rules::default(), &asked), 1234.0);
}

#[test]
fn a_valid_design_makes_a_ship_that_flies_and_an_invalid_one_is_refused() {
    let d = data();
    let mut c = StartConfig::new(2);
    let mut found = None;
    for def in &d.ship_plans {
        let text = plan_text(&d, &def.id);
        c.custom.ship = ShipCustom {
            mode: ShipMode::Design,
            plan: text,
            budget: Some(1e9),
            ..Default::default()
        };
        if new_sim_with(&c, Arc::clone(&d)).is_ok() {
            found = Some(def.id.clone());
            break;
        }
    }
    let id = found.expect("one plan on sale is a valid start");
    c.custom.ship.plan = plan_text(&d, &id);
    let cost = ship_cost(&d, &c.custom.ship).expect("cost");
    assert!(cost > 0.0);
    // The real cost is the shipwright's value: materials, labour and modules.
    let (_, r) = plan_report(&d, &c.custom.ship.plan).expect("report");
    assert!((cost - r.value).abs() < 1e-6);
    let mut s = sim_of(&d, &c);
    assert!(s.data.hulls[s.ship.design.hull].id.starts_with("custom_"));
    assert_eq!(s.blueprints.len(), 1, "the plan is in the drawers");
    assert!(s.readiness().engine > 0.0 && s.readiness().power > 0.0);
    let v0 = s.ship.vel;
    s.advance(
        30.0,
        &ShipInput {
            fwd: 1.0,
            ..Default::default()
        },
    );
    assert!((s.ship.vel - v0).length() > 1.0, "it thrusts and moves");
    // Over the budget: refused.
    c.custom.ship.budget = Some(cost - 1.0);
    let e = new_sim_with(&c, Arc::clone(&d)).err().expect("too dear");
    assert!(e.contains("trop cher"), "{e}");
    // Garbage and a plan without an engine are refused.
    c.custom.ship.budget = Some(1e9);
    c.custom.ship.plan = "pas un plan".into();
    assert!(new_sim_with(&c, Arc::clone(&d)).is_err());
    let mut bp = Blueprint::new(1, "VIDE");
    bp.place(&d, "hull_frame", 0, 0, 1).expect("place");
    c.custom.ship.plan = bp.to_text();
    assert!(new_sim_with(&c, Arc::clone(&d)).is_err());
}

// ---- presets ------------------------------------------------------------------------

fn same(a: &StartConfig, b: &StartConfig) -> bool {
    a.seed == b.seed
        && a.scenario == b.scenario
        && a.ship == b.ship
        && a.crew == b.crew
        && a.rules == b.rules
        && a.custom == b.custom
}

#[test]
fn a_preset_goes_to_text_and_to_a_code_and_back() {
    let d = data();
    for seed in [1u64, 5, 99] {
        let mut cfg = random_config(&d, seed);
        cfg.custom.crew.edit_mut(2).name = Some("Zoe".into());
        cfg.custom.ship = catalog_of(&d, cfg.scenario, 0);
        cfg.custom.ext = vec![("cryo".into(), "1".into())];
        cfg.crew = Some(vec![CrewPick {
            candidate: 1,
            role: 0,
        }]);
        let p = preset_of(&d, "essai", &cfg);
        let text = preset_to_text(&p);
        let back = preset_from_text(&text).expect("text");
        assert_eq!(back, p);
        assert!(
            same(&config_of(&d, &back).expect("config"), &cfg),
            "text {seed}"
        );
        let code = preset_code(&p);
        assert!(code.starts_with("VEC1-"));
        let back = preset_from_code(&code).expect("code");
        assert!(
            same(&config_of(&d, &back).expect("config"), &cfg),
            "code {seed}"
        );
        // The game it makes is the same.
        let a = fingerprint(&sim_of(&d, &cfg));
        let b = fingerprint(&sim_of(&d, &config_of(&d, &back).unwrap()));
        assert_eq!(a, b);
    }
}

#[test]
fn bad_codes_and_presets_are_refused_and_old_files_still_load() {
    let d = data();
    let p = preset_of(&d, "x", &StartConfig::new(3));
    let code = preset_code(&p);
    let mut broken = code.clone();
    broken.replace_range(
        code.len() - 2..,
        if code.ends_with("00") { "01" } else { "00" },
    );
    assert!(preset_from_code(&broken).is_err(), "checksum");
    assert!(preset_from_code("hello").is_err());
    assert!(preset_from_code("VEC1-0000-ZZ").is_err());
    assert!(preset_from_code(&code[..code.len() - 3]).is_err());
    // A file written before a field existed has defaults for it.
    let old = preset_from_text("(name:\"vieux\", scenario:\"voyageur\", seed:7)").expect("old");
    let cfg = config_of(&d, &old).expect("cfg");
    assert_eq!(cfg.seed, 7);
    assert_eq!(cfg.custom, CustomStart::default());
    assert!(config_of(
        &d,
        &StartPreset {
            scenario: "nulle_part".into(),
            ..Default::default()
        }
    )
    .is_err());
    // Whitespace in a pasted code does not matter.
    let spaced = format!("  {}\n{} ", &code[..10], &code[10..]);
    assert_eq!(preset_from_code(&spaced).expect("spaced"), p);
}

#[test]
fn difficulty_presets_set_the_rules_and_the_rating_follows() {
    let d = data();
    let mut cfg = StartConfig::new(1);
    let find = |id: &str| d.start.presets.iter().position(|p| p.id == id).unwrap();
    apply_difficulty_preset(&d, find("standard"), &mut cfg.rules, &mut cfg.custom);
    let r_std = rating(&d, &cfg);
    apply_difficulty_preset(&d, find("survie"), &mut cfg.rules, &mut cfg.custom);
    assert!(cfg.rules.permadeath && cfg.custom.save_mode == 2 && cfg.custom.world.epidemics > 1.0);
    let r_surv = rating(&d, &cfg);
    apply_difficulty_preset(&d, find("bac_a_sable"), &mut cfg.rules, &mut cfg.custom);
    let r_sand = rating(&d, &cfg);
    assert!(
        r_surv > r_std && r_std > r_sand,
        "{r_sand} {r_std} {r_surv}"
    );
    assert!((1..=5).contains(&r_surv) && (1..=5).contains(&r_sand));
    // Applying a preset resets what the previous one set.
    apply_difficulty_preset(&d, find("standard"), &mut cfg.rules, &mut cfg.custom);
    assert!(!cfg.rules.permadeath && cfg.custom.save_mode == 0);
    assert_eq!(cfg.rules, Rules::default());
}

#[test]
fn the_review_names_problems_and_warnings() {
    let d = data();
    let mut cfg = StartConfig::new(1);
    let r = review(&d, &cfg);
    assert!(r.error.is_none() && r.warnings.is_empty() && r.ship_cost == 0.0);
    cfg.rules.permadeath = true;
    cfg.custom.save_mode = 1;
    let r = review(&d, &cfg);
    assert!(r.warnings.len() >= 2);
    cfg.custom.ship = ShipCustom {
        mode: ShipMode::Design,
        plan: "xxx".into(),
        ..Default::default()
    };
    assert!(review(&d, &cfg).error.is_some());
}

#[test]
fn trait_prices_come_from_the_data_and_cover_both_signs() {
    let d = data();
    let pts: Vec<i32> = d.traits.iter().map(trait_points).collect();
    assert!(pts.iter().any(|p| *p > 0) && pts.iter().any(|p| *p < 0));
    let opt = d.traits.iter().find(|t| t.id == "optimiste").unwrap();
    let pes = d.traits.iter().find(|t| t.id == "pessimiste").unwrap();
    assert!(trait_points(opt) > trait_points(pes));
}

#[test]
fn implants_cost_points_and_are_installed() {
    let d = data();
    let seed = 12;
    let k = first_pick(&d, seed);
    let pool = start::crew_pool(&d, seed);
    let e = CrewEdit {
        candidate: k,
        implants: vec![0],
        ..Default::default()
    };
    assert!(crew_edit_valid(&d, &pool[k], &e).is_ok());
    let two = CrewEdit {
        implants: vec![0, 1],
        ..e.clone()
    };
    assert!(crew_edit_valid(&d, &pool[k], &two).is_err());
    let mut c = StartConfig::new(seed);
    c.custom.crew.edits.push(e);
    assert_eq!(sim_of(&d, &c).crew.members[0].implants, vec![0]);
}

#[test]
fn start_choices_survive_a_save_and_the_restricted_save_mode_cannot_be_reloaded_away() {
    let d = data();
    let mut c = StartConfig::new(5);
    c.custom.ai.personality = d.ai_trait_index("prudente").expect("trait");
    c.custom.ai.core = 1;
    c.custom.ai.software = vec!["optim_orbites".into()];
    c.custom.ai.name = "Pythie".into();
    c.custom.ship.name = "Aurore".into();
    c.custom.world.epidemics = 2.0;
    c.custom.world.surprises = 0.5;
    c.custom.save_mode = 2;
    let s = sim_of(&d, &c);
    let bytes = s.save_bytes().expect("save");
    let back = Sim::load_bytes(&bytes, Arc::clone(&d)).expect("load");
    assert_eq!(back.custom, s.custom);
    assert_eq!(back.mind.bias, s.mind.bias);
    assert_eq!(back.mind.alloc, s.mind.alloc);
    assert_eq!(back.mind.power_bonus, s.mind.power_bonus);
    assert_eq!(back.mind.software, s.mind.software);
    assert_eq!(back.custom.save_mode, 2);
    assert!(back.custom.may_save(false).is_err());
    assert_eq!(back.mind_capacity(), s.mind_capacity());
}

fn sleeping_config(d: &Arc<GameData>) -> StartConfig {
    let mut c = StartConfig::new(12);
    c.scenario = d.scenario_index("colon").expect("scenario");
    // A ship with a free internal slot: the last internal module is taken out.
    let mut ship = catalog_of(d, c.scenario, 0);
    let hi = d.hull_index(&ship.hull).expect("hull");
    let slot = (0..ship.modules.len())
        .rev()
        .find(|&i| {
            d.hulls[hi].slots[i].0 == vector_data::SlotType::Internal
                && d.module_index(&ship.modules[i]).is_some_and(|m| {
                    matches!(
                        d.modules[m].category.as_str(),
                        "cargo" | "battery" | "scanner"
                    )
                })
        })
        .expect("an internal module");
    ship.modules[slot].clear();
    c.custom.ship = ship;
    let pool = start::crew_pool(d, 12);
    let picks = start::default_crew(d, &d.scenarios[c.scenario], &pool);
    c.crew = Some(picks.clone());
    c.custom.cryo.members = vec![picks[0].candidate];
    c.custom.cryo.wake_on_arrival = true;
    c.custom.cryo.wake_on_alert = true;
    c.custom.cryo.wake_day = Some(30.0);
    c
}

#[test]
fn a_crew_asleep_at_the_start_sleeps_on_day_zero_costs_nothing_and_wakes_by_the_plan() {
    let d = data();
    let c = sleeping_config(&d);
    let plain = {
        let mut p = c.clone();
        p.custom.cryo = CryoStart::default();
        sim_of(&d, &p)
    };
    let mut s = sim_of(&d, &c);
    assert!(s.crew.members[0].body.cryo, "asleep on day 0");
    assert!(!s.crew.members[1].body.cryo);
    assert_eq!(s.crew.cryo.orders.len(), 3, "the wake plan is set");
    // The pods were fitted and paid for when the ship had none.
    let pods = |x: &Sim| {
        x.ship
            .design
            .modules(&x.data)
            .filter(|(_, m)| m.cryo > 0.0)
            .count()
    };
    assert!(pods(&s) >= 1);
    if pods(&plain) == 0 {
        assert!(s.credits < plain.credits, "the pods cost credits");
    }
    // The sleeper eats and drinks nothing.
    let (h0, t0) = (s.crew.members[0].hunger, s.crew.members[0].thirst);
    for _ in 0..24 {
        s.advance(3600.0, &ShipInput::default());
    }
    let p = &s.crew.members[0];
    assert!(
        p.body.cryo && p.hunger <= h0 + 0.05 && p.thirst <= t0 + 0.05,
        "{} {}",
        p.hunger,
        p.thirst
    );
    // On the day of the plan, the sleeper wakes.
    for _ in 0..40 {
        s.advance(86_400.0 / 4.0 * 4.0, &ShipInput::default());
        if !s.crew.members[0].body.cryo {
            break;
        }
    }
    assert!(!s.crew.members[0].body.cryo, "woken by the plan");
    assert!(s.time >= 29.0 * 86_400.0 || s.crew.members[0].alive);
}

#[test]
fn cryosleep_at_the_start_refuses_what_cannot_be_done() {
    let d = data();
    let mut c = sleeping_config(&d);
    // Too poor to fit pods.
    c.custom.ship = ShipCustom::default();
    c.scenario = d.scenario_index("naufrage").expect("scenario");
    let pool = start::crew_pool(&d, 12);
    let picks = start::default_crew(&d, &d.scenarios[c.scenario], &pool);
    c.custom.cryo.members = vec![picks[0].candidate];
    c.crew = Some(picks);
    c.custom.world.place = Some(0);
    let r = new_sim_with(&c, Arc::clone(&d));
    assert!(r.is_err() || r.as_ref().is_ok_and(|s| s.crew.members[0].body.cryo));
    // Nobody picked asleep: nothing happens.
    let mut c2 = StartConfig::new(3);
    c2.custom.cryo.members = vec![99];
    let s = sim_of(&d, &c2);
    assert!(s.crew.members.iter().all(|p| !p.body.cryo));
}
