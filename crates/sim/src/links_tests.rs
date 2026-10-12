//! Links between systems of the second interaction audit (see docs/MATRICE_INTERACTIONS.md): one
//! test per link, each one measures that the state of a system changes what another one does.
use crate::alerts::{Severity, Topic};
use crate::colony::tests::founded;
use crate::crew::DAY;
use crate::politics::{cell_center, cell_of, cells_around};
use crate::scene::new_sim;
use crate::terraform::tests::earthlike;
use crate::terraform::{CO2, O2};
use crate::Sim;
use std::sync::Arc;
use vector_core::Vec3;
use vector_data::GameData;

fn data() -> Arc<GameData> {
    Arc::new(
        GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
            .expect("data"),
    )
}

fn sim() -> Sim {
    new_sim(7, data())
}

fn put(c: &mut crate::colony::Colony, d: &GameData, id: &str, n: usize) {
    let def = d.building_index(id).expect(id);
    for _ in 0..n {
        c.buildings.push(crate::colony::BuildingInst {
            def,
            progress: 1.0,
            enabled: true,
            recipe: 0,
            health: 1.0,
            ops: Default::default(),
        });
    }
}

// ---- 1. colonies -> atmosphere -----------------------------------------------------------------

#[test]
fn link_pollution_to_atmosphere_emissions_join_the_air_and_the_ledger_stays_closed() {
    let d = data();
    let (mut t, _) = earthlike(&d);
    let (co2, o2, l0) = (t.pressure_of(CO2), t.pressure_of(O2), t.closed_ledger());
    t.emit_co2(1.0e15);
    assert!(
        t.pressure_of(CO2) > co2 * 1.05,
        "{co2} -> {}",
        t.pressure_of(CO2)
    );
    assert!(t.pressure_of(O2) < o2, "burning eats oxygen");
    assert!(
        (t.closed_ledger() - l0).abs() < 1e-9 * l0.abs(),
        "mass is booked: {} vs {l0}",
        t.closed_ledger()
    );
    // Bounded: a silly figure cannot swallow the air in one go.
    let air: f64 = t.gas.iter().sum();
    let mut u = t.clone();
    u.emit_co2(1.0e30);
    assert!(
        u.gas.iter().sum::<f64>() < air * 1.01,
        "capped at a thousandth of the air"
    );
    // Nothing happens without a rocky world or with nothing to emit.
    let mut v = t.clone();
    v.emit_co2(-5.0);
    v.emit_co2(f64::NAN);
    assert_eq!(v.gas, t.gas);
}

#[test]
fn link_pollution_to_atmosphere_a_colony_with_industry_warms_its_world_slowly() {
    let (s, ci) = founded();
    let d = Arc::clone(&s.data);
    let mut base = s.colonies[ci].clone();
    base.caps = (1 << 16) - 1;
    base.pop = 200.0;
    base.treasury = 1.0e9;
    let mut dirty = base.clone();
    put(&mut dirty, &d, "smelter", 6);
    put(&mut dirty, &d, "chemical_plant", 4);
    for c in [&mut base, &mut dirty] {
        for k in 1..=120 {
            // Four months at a time: the world follows the colony's own tick.
            let t = c.last_tick + 30.0 * DAY;
            c.update(&d, t);
            for r in 0..c.storage.len() {
                c.storage[r] = c.storage[r].max(1.0e5);
            }
            let _ = k;
        }
    }
    assert!(
        dirty.works.emitted > base.works.emitted + 1.0,
        "industry emits: {} vs {}",
        dirty.works.emitted,
        base.works.emitted
    );
    let (a, b) = (dirty.terra.as_ref().unwrap(), base.terra.as_ref().unwrap());
    assert!(
        a.gas[CO2] > b.gas[CO2] + 1.0e9,
        "{} vs {}",
        a.gas[CO2],
        b.gas[CO2]
    );
    // A decade of industry is not the end of the world: slow, bounded.
    let (pa, pb) = (a.pressure_of(CO2), b.pressure_of(CO2));
    assert!(pa < 4.0 * pb.max(0.01) + 0.05, "{pa} vs {pb}");
    assert!((a.t - b.t).abs() < 25.0, "{} vs {}", a.t, b.t);
}

// ---- 5. runaway worlds -> alerts -----------------------------------------------------------------

#[test]
fn link_terraform_to_alerts_a_runaway_or_drifting_climate_is_flagged() {
    let d = data();
    let (mut t, _) = earthlike(&d);
    t.version = 1;
    t.drift = 0.0;
    assert!(t.alarm().is_none(), "a stable world raises nothing");
    t.t = t.snap.t_base + 40.0;
    t.drift = 2.0;
    let (critical, text) = t.alarm().expect("runaway");
    assert!(critical && text.contains("EMBALLEMENT"), "{text}");
    t.t = t.snap.t_base + 2.0;
    t.drift = crate::terraform::DRIFT_ALARM_K_YR + 0.2;
    let (critical, text) = t.alarm().expect("drift");
    assert!(!critical && text.contains("DERIVE"), "{text}");
    // The alert reaches the list of the player through the colony.
    let (mut s, ci) = founded();
    let mut sick = t.clone();
    sick.t = sick.snap.t_base + 40.0;
    sick.drift = 2.0;
    s.colonies[ci].terra = Some(sick);
    let name = s.colonies[ci].name.to_uppercase();
    let hit = s
        .alerts()
        .into_iter()
        .find(|a| a.topic == Topic::Colony && a.text.contains("EMBALLEMENT"))
        .expect("alert");
    assert_eq!(hit.sev, Severity::Critical);
    assert!(hit.text.starts_with(&name));
}

// ---- 2. colonies -> faction ----------------------------------------------------------------------

#[test]
fn link_colony_to_faction_prosperity_wins_the_host_and_crime_and_plague_lose_it() {
    let (mut s, ci) = founded();
    let (mut good, _) = founded();
    let f = s
        .colony_faction(ci)
        .expect("the sector is held by somebody");
    let rep0 = s.reputation_with(Some(f));
    {
        let c = &mut good.colonies[ci];
        c.pop = 3000.0;
        c.happiness = 0.95;
        c.unrest = 0.0;
        c.works.crime = 0.0;
        c.stats.income = 100.0;
        c.stats.upkeep = 10.0;
    }
    assert!(
        good.colony_standing(ci) > 0.3,
        "{}",
        good.colony_standing(ci)
    );
    for _ in 0..40 {
        good.colony_relations_tick(ci, 10.0);
    }
    let up = good.reputation_with(Some(f));
    assert!(
        up > rep0 + 3.0,
        "a thriving colony wins friends: {rep0} -> {up}"
    );
    assert!(
        up <= crate::links::COLONY_REP_CAP + 1.0,
        "never past the cap: {up}"
    );
    {
        let c = &mut s.colonies[ci];
        c.pop = 3000.0;
        c.happiness = 0.1;
        c.unrest = 0.9;
        c.works.crime = 0.9;
        c.debt_days = 30.0;
        c.stats.income = 0.0;
        c.stats.upkeep = 50.0;
    }
    assert!(s.colony_standing(ci) < -0.5, "{}", s.colony_standing(ci));
    for _ in 0..40 {
        s.colony_relations_tick(ci, 10.0);
    }
    let down = s.reputation_with(Some(f));
    assert!(
        down < rep0 - 3.0,
        "a lawless colony loses them: {rep0} -> {down}"
    );
    // An empty colony counts for nothing.
    s.colonies[ci].pop = 0.0;
    assert_eq!(s.colony_standing(ci), 0.0);
}

// ---- 4. mind -> navigation -----------------------------------------------------------------------

#[test]
fn link_mind_to_navigation_the_pilotage_budget_sets_the_margin_and_the_precision() {
    let mut rich = sim();
    let mut poor = sim();
    rich.mind.alloc = [10, 0, 0, 0, 0];
    poor.mind.alloc = [0, 4, 4, 4, 4];
    assert!(rich.mind_factor(crate::mind::T_PILOTAGE) > poor.mind_factor(crate::mind::T_PILOTAGE));
    assert!(rich.nav_reserve() < poor.nav_reserve());
    assert!(rich.nav_step_angle() <= crate::nav_predict::STEP_ANGLE + 1e-12);
    assert!(poor.nav_step_angle() > rich.nav_step_angle() * 1.1);
    // The same ship on the same orbit: the starved computer integrates in fewer, coarser steps.
    let fine = rich.predict_ship(86_400.0, &[], None);
    let coarse = poor.predict_ship(86_400.0, &[], None);
    assert!(
        coarse.steps <= fine.steps,
        "{} vs {}",
        coarse.steps,
        fine.steps
    );
    // The player's own margin stays the base: nothing allotted doubles the doubt at most.
    assert!(poor.nav_reserve() <= poor.nav.limits.reserve * 1.5 + 1e-12);
}

// ---- 8. history -> news -----------------------------------------------------------------------

#[test]
fn link_history_to_news_a_centenary_of_the_sector_is_told_when_it_comes() {
    let mut s = sim();
    let d = Arc::clone(&s.data);
    let seed = s.galaxy.seed;
    let galaxy = vector_world::galaxy::Galaxy::new(seed);
    let mut best: Option<([i64; 3], f64, f64, f64)> = None;
    for c in cells_around(cell_of(s.here_pos), crate::news::HEARING_CELLS) {
        let m = cell_center(c);
        let dist = ((m[0] - s.here_pos[0]).powi(2)
            + (m[1] - s.here_pos[1]).powi(2)
            + (m[2] - s.here_pos[2]).powi(2))
        .sqrt();
        let delay = s.courier_delay_days(
            dist,
            (dist - crate::politics::SECTOR_LY / 2.0).max(0.0) / crate::news::COURIER_LY_DAY,
        ) * DAY;
        let holder = crate::politics::faction_of(&d, &galaxy, c);
        for (year, text) in crate::history::anniversaries(&d, seed, c, holder, 2400.0, 2520.0) {
            assert!(
                text.contains("{place}") && text.contains("siecle"),
                "{text}"
            );
            let t = (year - 2400.0) * crate::history::YEAR;
            // Avoid the first days (the feed backfills) and keep the one heard soonest.
            if t > 40.0 * DAY && best.is_none_or(|b| t + delay < b.1 + b.3) {
                best = Some((c, t, dist, delay));
            }
        }
    }
    let (_, t, _, delay) = best.expect("a centenary within 120 years in the sectors heard");
    s.time = t - 3.0 * DAY;
    s.news_tick();
    s.news.clear();
    s.time = t + delay - 0.5 * DAY;
    s.news_tick();
    let before = s
        .news
        .iter()
        .filter(|n| n.text.contains("Commemoration"))
        .count();
    s.time = t + delay + 1.5 * DAY;
    s.news_tick();
    let after: Vec<_> = s
        .news
        .iter()
        .filter(|n| n.text.contains("Commemoration"))
        .collect();
    assert!(after.len() > before, "told once the couriers arrive");
    assert!(
        after.iter().any(|n| n.text.contains("Marche")),
        "with the name its history gave"
    );
}

#[test]
fn link_history_to_news_a_dig_on_a_ruin_makes_the_news() {
    let mut s = sim();
    s.news.clear();
    let found = s.dig_site("ruine", 4242);
    assert!(!found.is_empty());
    let item = s
        .news
        .iter()
        .find(|n| n.text.contains("Ruines explorees"))
        .expect("the discovery is told");
    assert_eq!(item.tag, 5);
    assert!(item.text.contains("Edifie par"), "{}", item.text);
    // The same ruin cannot be found twice: no second story.
    let n = s.news.len();
    assert!(s.dig_site("ruine", 4242).is_empty());
    assert_eq!(s.news.len(), n);
}

// ---- 13. dialogues -> news ---------------------------------------------------------------------

#[test]
fn link_dialogue_to_news_a_bribe_leaves_a_trace_in_the_feed() {
    let mut s = new_sim(11, data());
    s.spawn_traffic();
    let k = s
        .npcs
        .iter()
        .position(|n| n.role == crate::npc::NpcRole::Patrol)
        .expect("a patrol");
    let start = s.dialogue_start(k).expect("start");
    let bribe = s
        .data
        .dialogues
        .node(&start)
        .unwrap()
        .options
        .iter()
        .position(|o| o.effects.iter().any(|e| e == "bribe"))
        .expect("bribe option");
    s.news.clear();
    s.credits = 5000.0;
    let (_, text) = s.dialogue_choose(k, &start, bribe).expect("bribe");
    let item = s
        .news
        .iter()
        .find(|n| n.text.contains(&s.npcs[k].name))
        .expect("traced");
    assert_eq!(item.tag, 6);
    assert!(item.text.contains(&text), "{}", item.text);
}

#[test]
fn link_history_to_dialogue_the_radio_names_the_march_of_the_neighbour() {
    let mut s = new_sim(11, data());
    s.spawn_traffic();
    let text = s
        .apply_dialogue_effect(0, &vector_data::dialogue::Effect::Region)
        .expect("ok")
        .expect("text");
    assert!(text.contains("(Marche"), "{text}");
}

// ---- 10. sector epidemics -> surprises -----------------------------------------------------------

#[test]
fn link_epidemic_to_surprise_an_outbreak_of_the_sector_triggers_the_sanitary_surprises() {
    use vector_data::surprise::Cond;
    let mut s = sim();
    let (seed, pos) = (s.galaxy.seed, s.here_pos);
    let d = Arc::clone(&s.data);
    let mut sick = None;
    let mut calm = None;
    for day in 100..6000 {
        let t = f64::from(day) * DAY;
        let worst = crate::epidemic::prevalence_at(&d, seed, pos, t)
            .into_iter()
            .map(|(_, i)| i)
            .fold(0.0, f64::max);
        if worst >= 0.02 && sick.is_none() {
            sick = Some(t);
        }
        if worst == 0.0 && calm.is_none() {
            calm = Some(t);
        }
        if sick.is_some() && calm.is_some() {
            break;
        }
    }
    let (sick, calm) = (
        sick.expect("an outbreak in the sector"),
        calm.expect("a calm day"),
    );
    s.time = sick;
    let mut ctx = s.surprise_ctx();
    assert!(s.surprise_cond(&Cond::SectorOutbreak(0.01), &mut ctx));
    assert!(
        !s.surprise_cond(&Cond::Epidemic(60.0), &mut ctx),
        "no colony is sick"
    );
    s.time = calm;
    let mut ctx = s.surprise_ctx();
    assert!(!s.surprise_cond(&Cond::SectorOutbreak(0.01), &mut ctx));
    assert!(d.surprises.iter().any(|x| x.id == "foyer_sur_le_quai"));
}

// ---- 11. mind -> economy -------------------------------------------------------------------------

#[test]
fn link_mind_to_economy_a_rich_simulation_budget_cuts_the_broker_and_widens_the_board() {
    let mut rich = sim();
    let mut poor = sim();
    rich.mind.alloc = [0, 0, 0, 10, 0];
    poor.mind.alloc = [4, 4, 4, 0, 4];
    assert!(rich.broker_fee() < crate::lanes::BROKER_FEE);
    assert!(poor.broker_fee() > crate::lanes::BROKER_FEE);
    assert!(rich.broker_fee() < poor.broker_fee());
    assert!(rich.arbitrage_breadth() > poor.arbitrage_breadth());
    // The bounds: never free, never more than half again.
    assert!(rich.broker_fee() >= 0.5 * crate::lanes::BROKER_FEE - 1e-12);
    assert!(poor.broker_fee() <= 1.5 * crate::lanes::BROKER_FEE + 1e-12);
}

// ---- 12. high SF -> economy ----------------------------------------------------------------------

#[test]
fn link_dimension_to_economy_rich_rock_is_cheap_and_the_peoples_ban_exotic_matter() {
    let mut s = sim();
    let d = Arc::clone(&s.data);
    let iron = d.resource_index("iron").unwrap();
    let anti = d.resource_index("antimatter").unwrap();
    let water = d.resource_index("water").unwrap();
    assert_eq!(s.dimension_price_factor(iron), 1.0);
    let rich = d
        .dimensions
        .iter()
        .position(|x| x.id == "prodigue")
        .unwrap();
    s.multiverse.dimension = rich as u32 + 1;
    let f = s.dimension_price_factor(iron);
    assert!(f < 0.7, "three times the ore, much cheaper: {f}");
    assert_eq!(
        s.dimension_price_factor(water),
        1.0,
        "what is not ore is left alone"
    );
    let hazard = d.dimensions.iter().position(|x| x.id == "cristal").unwrap();
    s.multiverse.dimension = hazard as u32 + 1;
    assert!(
        s.dimension_price_factor(anti) < 0.7,
        "strong fields make exotic matter"
    );
    // The market of a station follows.
    let st = (0..s.bodies.len())
        .find(|&i| s.bodies[i].kind == vector_world::BodyKind::Station)
        .expect("station");
    s.multiverse.dimension = 0;
    let home = s.market(st).price(&d, iron);
    s.markets.clear();
    s.multiverse.dimension = rich as u32 + 1;
    let there = s.market(st).price(&d, iron);
    assert!(there < home * 0.7, "{there} vs {home}");
    // Law and contraband: pacifists and traditionalists refuse antimatter, others do not.
    let bans: Vec<bool> = (0..d.factions.len())
        .map(|f| crate::economy::banned_goods_of(&d, f).contains(&anti))
        .collect();
    assert!(
        bans.iter().any(|b| *b) && bans.iter().any(|b| !*b),
        "{bans:?}"
    );
}

// ---- 9. colony imports -> market stock -------------------------------------------------------------

#[test]
fn link_colony_trade_to_market_the_food_a_colony_imports_leaves_the_shelves_of_the_station() {
    let (mut s, ci) = founded();
    let d = Arc::clone(&s.data);
    let food = d.resource_index("food").unwrap();
    let st = (0..s.bodies.len()).find(|&i| s.bodies[i].kind == vector_world::BodyKind::Station);
    let Some(st) = st else {
        panic!("the test system needs a station");
    };
    {
        let c = &mut s.colonies[ci];
        c.caps = (1 << 16) - 1;
        c.pop = 400.0;
        c.treasury = 1.0e7;
        c.storage[food] = 0.0;
        c.works.env.ready = true;
        assert!(c.has_port(&d), "the founding kit has a port");
    }
    // A thin shelf, a colony that must import: the volume leaves the stock of the station.
    let shelf = 300.0;
    s.market(st).stock[food] = shelf;
    s.market(st).target[food] = shelf;
    s.time += 3.0 * DAY;
    s.update_colonies_pub();
    let left = s.market(st).stock[food];
    let imported = s.colonies[ci].works.bought;
    assert!(imported > 0.0, "the colony imported food");
    assert!(
        left < shelf - 1.0,
        "the import came out of the stock: {left}"
    );
    assert!(left >= 0.0, "a shelf is never below nothing");
    // The player feels it: the same food costs more where the colony has emptied the shelves.
    let mut full = founded().0;
    let st_full = (0..full.bodies.len())
        .find(|&i| full.bodies[i].kind == vector_world::BodyKind::Station)
        .unwrap();
    full.market(st_full).stock[food] = shelf;
    full.market(st_full).target[food] = shelf;
    let (p_full, p_left) = (
        full.market(st_full).price(&d, food),
        s.market(st).price(&d, food),
    );
    assert!(p_left > p_full, "{p_left} vs {p_full}");
    // A glut sold by the colony fills the shelves, within what they can hold.
    {
        let c = &mut s.colonies[ci];
        c.storage[food] = 5.0e7;
        c.pop = 400.0;
    }
    s.time += 3.0 * DAY;
    s.update_colonies_pub();
    let m = s.market(st);
    assert!(m.stock[food] > left, "the exports landed on the shelves");
    assert!(
        m.stock[food] <= crate::links::MARKET_ROOM * m.reference[food] + 1e-6,
        "{} vs {}",
        m.stock[food],
        m.reference[food]
    );
}

// ---- 7. weather -> landing -----------------------------------------------------------------------

#[test]
fn link_weather_to_landing_wind_moves_the_ship_and_wet_ground_takes_the_grip() {
    use crate::landing::{classify, Outcome, Touch};
    use crate::links::SiteWeather;
    use crate::planet::Weather;
    let up = Vec3::new(0.0, 1.0, 0.0);
    let storm = SiteWeather::of(Weather::Storm, 0.8, 1.0, up, 7);
    let clear = SiteWeather::of(Weather::Clear, 0.2, 1.0, up, 7);
    assert!(storm.wind > 3.0 * clear.wind && storm.rough() && !clear.rough());
    assert!(storm.visibility < 0.3 && storm.grip < clear.grip);
    assert_eq!(
        SiteWeather::of(Weather::Storm, 0.8, 0.0, up, 7).wind,
        0.0,
        "no air, no wind"
    );
    assert!(
        storm.air_velocity(up, 0.0).dot(up).abs() < 1e-9,
        "the wind is horizontal"
    );
    assert!(
        storm.air_velocity(up, 9_000.0).length() < 1e-9,
        "calm above the weather"
    );
    // A touch that dry ground forgives is a fall on ice.
    let touch = Touch {
        v_in: 2.0,
        v_t: 2.5,
        tilt: 0.05,
        slope: 0.1,
        rough: 1.0,
        gear: true,
        grip: 1.0,
    };
    assert_eq!(classify(&touch), Outcome::Landed);
    let ice = Touch {
        grip: 0.35,
        ..touch
    };
    assert!(
        matches!(classify(&ice), Outcome::Hard { .. }),
        "{:?}",
        classify(&ice)
    );
    // The air itself moves: the same ship dives in still air and in a gale, and ends up elsewhere.
    let mut a = sim();
    let mut b = sim();
    a.docked = None;
    b.docked = None;
    let i = (1..a.bodies.len())
        .find(|&i| a.air_of(i).is_some() && a.landable(i))
        .expect("a world with air");
    let air = a.air_of(i).unwrap();
    let dir = Vec3::new(0.0, 1.0, 0.0);
    for s in [&mut a, &mut b] {
        s.ship.pos = s.bodies[i].pos + dir * (s.bodies[i].radius + 2_000.0_f64.min(air.height));
        s.ship.vel = s.bodies[i].vel + s.surface_velocity(i, s.ship.pos - s.bodies[i].pos);
    }
    let nb = a.body_orient(i).transpose().mul_vec(dir);
    let (cx, cy) = vector_world::geology::cell_of([nb.x, nb.y, nb.z]);
    let (rx, ry) = crate::planet::region_of(cx, cy);
    let key = (a.day_number(), i, rx, ry);
    a.site_weather_cache = Some((key, SiteWeather::of(Weather::Clear, 0.0, 1.0, dir, 3)));
    b.site_weather_cache = Some((
        key,
        SiteWeather {
            wind: 60.0,
            ..SiteWeather::of(Weather::Storm, 1.0, 1.0, dir, 3)
        },
    ));
    assert!(a.wind_over_ship(i).length() < 5.0, "a calm day");
    assert!(
        b.wind_over_ship(i).length() > 20.0,
        "a gale: {:?}",
        b.wind_over_ship(i)
    );
    let v0 = a.ship.vel;
    for _ in 0..30 {
        a.atmosphere_drag(1.0 / 30.0);
        b.atmosphere_drag(1.0 / 30.0);
    }
    assert!(
        (a.ship.vel - b.ship.vel).length() > 0.05,
        "{:?} {:?} {:?}",
        a.ship.vel - v0,
        b.ship.vel - v0,
        (a.ship.vel - b.ship.vel).length()
    );
    // Parked on the ground the ship rests against it: no wind drags it.
    b.parked = Some((i, Vec3::ZERO));
    assert_eq!(b.wind_over_ship(i), Vec3::ZERO);
}

// ---- 3. combat: one model of the shot ----------------------------------------------------------------

#[test]
fn link_combat_the_guns_of_the_others_follow_the_physics_of_the_player() {
    use crate::combat::{npc_stats, Gun, AIM_RADIUS};
    use crate::npc::NpcRole;
    // Damage is the kinetic energy of the shell, and the dps of the old table is kept.
    for l in 1..=5u8 {
        let p = npc_stats(NpcRole::Pirate, l);
        assert!(
            (p.dps - (30.0 + 18.0 * f64::from(l))).abs() < 1e-6,
            "{}",
            p.dps
        );
        let g = p.gun.expect("a gun");
        let ke = crate::arms::kinetic_energy_j(g.mass, g.speed);
        assert!((g.damage() - crate::arms::damage_from_energy(ke)).abs() < 1e-9);
        let q = npc_stats(NpcRole::Patrol, l);
        assert!(
            (q.dps - (50.0 + 25.0 * f64::from(l))).abs() < 1e-6,
            "{}",
            q.dps
        );
    }
    assert!(npc_stats(NpcRole::Trader, 3).gun.is_none());
    assert_eq!(npc_stats(NpcRole::Miner, 3).dps, 0.0);
    // The chance to hit is the dispersion: tighter guns, shorter ranges, more hits.
    let g = npc_stats(NpcRole::Pirate, 3).gun.unwrap();
    assert_eq!(g.hit_chance(100.0), 1.0);
    assert!(g.hit_chance(7_000.0) < g.hit_chance(3_000.0));
    let tight = Gun {
        spread: g.spread / 2.0,
        ..g
    };
    assert!(tight.hit_chance(7_000.0) > g.hit_chance(7_000.0));
    assert!((AIM_RADIUS - 1.5 * crate::SHIP_RADIUS).abs() < 1e-12);
    // The balance of the fights is the one of the table it replaces: on the same ranges the new
    // accuracy stays within a third of the old linear one and averages the same.
    for (role, range) in [(NpcRole::Pirate, 7_000.0), (NpcRole::Patrol, 9_000.0)] {
        let s = npc_stats(role, 2);
        let gun = s.gun.unwrap();
        let (mut old, mut new) = (0.0, 0.0);
        for k in 0..=20 {
            let d = range * f64::from(k) / 20.0;
            let o = (1.05 - 0.6 * d / range).clamp(0.12, 0.9);
            let n = gun.hit_chance(d).clamp(0.12, 0.9);
            old += o;
            new += n;
        }
        assert!(
            (new / old - 1.0).abs() < 0.3,
            "{role:?}: average accuracy {} vs {}",
            new / 21.0,
            old / 21.0
        );
    }
}

// ---- ten simulated years: every loop stays bounded --------------------------------------------------

#[test]
fn link_ten_simulated_years_of_a_dirty_colony_keep_every_figure_bounded() {
    let (mut s, ci) = founded();
    let d = Arc::clone(&s.data);
    let food = d.resource_index("food").unwrap();
    {
        let c = &mut s.colonies[ci];
        c.caps = (1 << 16) - 1;
        c.pop = 800.0;
        c.treasury = 1.0e8;
        put(c, &d, "smelter", 8);
        put(c, &d, "chemical_plant", 6);
    }
    let f = s.colony_faction(ci).expect("a faction holds the sector");
    let t0 = s.colonies[ci]
        .terra
        .clone()
        .expect("a world with a climate");
    let (l0, rep0) = (t0.closed_ledger(), s.reputation_with(Some(f)));
    let station =
        (0..s.bodies.len()).find(|&i| s.bodies[i].kind == vector_world::BodyKind::Station);
    for month in 0..120 {
        s.time += 30.0 * DAY;
        {
            // The town is kept supplied and populated: the test is about the loops, not about
            // surviving on a hot rock.
            let c = &mut s.colonies[ci];
            for r in 0..c.storage.len() {
                c.storage[r] = c.storage[r].max(2.0e5);
            }
            c.storage[food] = c.storage[food].max(5.0e5);
            c.pop = c.pop.max(500.0);
        }
        s.update_colonies_pub();
        let c = &s.colonies[ci];
        let t = c.terra.as_ref().unwrap();
        assert!(
            t.t.is_finite() && t.gas.iter().all(|g| g.is_finite() && *g >= 0.0),
            "month {month}: the air is a number"
        );
        assert!(c.pop.is_finite() && c.treasury.is_finite() && c.works.pollution <= 1.0);
        let rep = s.reputation_with(Some(f));
        assert!((-100.0..=100.0).contains(&rep), "month {month}: {rep}");
        if let Some(st) = station {
            let m = s.market(st);
            assert!(
                m.stock[food].is_finite()
                    && m.stock[food] >= 0.0
                    && m.stock[food] <= 7.0 * m.reference[food],
                "month {month}: food on the shelves {}",
                m.stock[food]
            );
        }
    }
    let c = &s.colonies[ci];
    let t = c.terra.as_ref().unwrap();
    assert!(
        c.works.emitted > 0.0 && t.gas[CO2] > t0.gas[CO2],
        "the works fed the air: emitted {} CO2 {} -> {} ground {} pop {} t {} -> {}",
        c.works.emitted,
        t0.gas[CO2],
        t.gas[CO2],
        t.ground,
        c.pop,
        t0.t,
        t.t
    );
    assert!(
        t.t > t0.t - 5.0 && t.t < t0.t + 40.0,
        "ten years of industry move the climate slowly: {} -> {}",
        t0.t,
        t.t
    );
    assert!(
        (t.closed_ledger() - l0).abs() < 1.0e-6 * l0.abs(),
        "the ledger is closed: {} vs {l0}",
        t.closed_ledger()
    );
    assert!(
        s.reputation_with(Some(f)) != rep0 || c.pop < 1.0,
        "the host noticed the colony"
    );
}
