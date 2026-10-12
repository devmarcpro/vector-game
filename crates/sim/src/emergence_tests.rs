//! Links between systems added by the audit of interactions (see docs/MATRICE_INTERACTIONS.md):
//! one test per link, each one measures that a state of a system changes what another one does.
use crate::alerts::{Severity, Topic};
use crate::crew::DAY;
use crate::politics::{cell_center, cell_of, cells_around};
use crate::scene::new_sim;
use crate::surprises::Ctx;
use crate::Sim;
use std::sync::Arc;
use vector_data::surprise::{Cmp, Cond};
use vector_data::GameData;

fn data() -> Arc<GameData> {
    crate::test_support::data()
}

fn sim() -> Sim {
    new_sim(7, data())
}

fn hurt_crew(s: &mut Sim, hits: usize) -> (usize, f32) {
    for k in 0..hits {
        s.time += 7.0 + k as f64;
        s.hurt_ship(5000.0, 0);
    }
    let wounds = s.crew.members.iter().map(|p| p.body.wounds.len()).sum();
    let health = s.crew.members.iter().map(|p| p.health).sum();
    (wounds, health)
}

#[test]
fn link_crash_to_medicine_a_hit_on_the_hull_wounds_the_crew() {
    let mut s = sim();
    let (wounds, health) = hurt_crew(&mut s, 30);
    assert!(wounds > 0, "a crash that wrecks the hull leaves wounds");
    assert!(health < s.crew.members.len() as f32);
    // Invulnerable: no damage to the ship, none to the people.
    let mut t = sim();
    let mut rules = t.rules.clone();
    rules.invulnerable = true;
    t.set_rules(rules);
    let before: f32 = t.crew.members.iter().map(|p| p.health).sum();
    let (wounds, health) = hurt_crew(&mut t, 30);
    assert_eq!(wounds, 0);
    assert_eq!(health, before);
}

#[test]
fn link_gear_to_medicine_armour_worn_stops_part_of_the_injuries() {
    let exo = data().gear_index("exosquelette").expect("armour");
    let mut bare = sim();
    let mut armed = sim();
    for p in &mut armed.crew.members {
        p.gear = vec![None, Some(exo), None];
    }
    let (w_bare, h_bare) = hurt_crew(&mut bare, 8);
    let (w_armed, h_armed) = hurt_crew(&mut armed, 8);
    assert!(w_bare > 0);
    assert!(
        w_armed < w_bare && h_armed >= h_bare,
        "armour: {w_armed} wounds, health {h_armed}; bare: {w_bare} wounds, health {h_bare}"
    );
}

#[test]
fn link_language_to_dialogue_a_favour_is_worth_less_when_it_is_not_understood() {
    use vector_data::dialogue::Effect;
    let gain = |translation: bool| {
        let mut s = new_sim(11, data());
        s.spawn_traffic();
        let f = s
            .data
            .factions
            .iter()
            .position(|f| f.id == "concorde")
            .unwrap();
        s.npcs[0].faction = Some(f);
        s.crew.members.clear();
        s.mind.alloc = if translation {
            [0, 0, 100, 0, 0]
        } else {
            [100, 0, 0, 0, 0]
        };
        let before = s.reputation_with(Some(f));
        s.apply_dialogue_effect(0, &Effect::Rep(10.0)).expect("rep");
        s.reputation_with(Some(f)) - before
    };
    let (blind, heard) = (gain(false), gain(true));
    assert!(blind > 0.0 && heard > blind * 1.15, "{blind} vs {heard}");
}

/// The first date, in steps of ten days, at which `flip(t0, t1)` holds in some cell the news
/// listens to; with the delay the couriers add. Returns (cell index, t0, t1, delay).
fn find_flip(s: &Sim, flip: impl Fn([i64; 3], f64, f64) -> bool) -> Option<(f64, f64, f64)> {
    for c in cells_around(cell_of(s.here_pos), crate::news::HEARING_CELLS) {
        let d = {
            let m = cell_center(c);
            ((m[0] - s.here_pos[0]).powi(2)
                + (m[1] - s.here_pos[1]).powi(2)
                + (m[2] - s.here_pos[2]).powi(2))
            .sqrt()
        };
        let delay = s.courier_delay_days(
            d,
            (d - crate::politics::SECTOR_LY / 2.0).max(0.0) / crate::news::COURIER_LY_DAY,
        ) * DAY;
        for k in 30..400 {
            let (t0, t1) = (f64::from(k) * 10.0 * DAY, f64::from(k + 1) * 10.0 * DAY);
            if flip(c, t0, t1) {
                return Some((t0 + delay, t1 + delay, delay));
            }
        }
    }
    None
}

#[test]
fn link_harvest_to_news_a_bad_year_and_a_bumper_one_are_told() {
    let mut s = sim();
    let seed = s.galaxy.seed;
    for (bad, text) in [
        (true, "Mauvaise recolte"),
        (false, "Recolte exceptionnelle"),
    ] {
        let found = find_flip(&s, |c, t0, t1| {
            let (h0, h1) = (
                crate::macroeco::harvest(seed, c, t0),
                crate::macroeco::harvest(seed, c, t1),
            );
            if bad {
                h0 >= crate::news::BAD_HARVEST && h1 < crate::news::BAD_HARVEST
            } else {
                h0 < crate::news::BUMPER_HARVEST && h1 >= crate::news::BUMPER_HARVEST
            }
        });
        let (t0, t1, _) = found.expect("a harvest crossing in a few years");
        s.news.clear();
        s.time = t1;
        s.news_scanned_to = t0;
        s.news_tick();
        assert!(
            s.news.iter().any(|n| n.text.contains(text)),
            "{text}: {:?}",
            s.news.iter().map(|n| &n.text).collect::<Vec<_>>()
        );
    }
}

#[test]
fn link_blockade_to_news_a_siege_and_its_end_are_told() {
    let mut s = sim();
    let seed = s.galaxy.seed;
    let d = Arc::clone(&s.data);
    let g = vector_world::galaxy::Galaxy::new(seed);
    for (start, text) in [(true, "Blocus pres de"), (false, "Fin du blocus")] {
        let found = find_flip(&s, |c, t0, t1| {
            let (b0, b1) = (
                crate::arms::is_besieged(&d, &g, seed, c, t0),
                crate::arms::is_besieged(&d, &g, seed, c, t1),
            );
            b0 != start && b1 == start
        });
        let Some((t0, t1, _)) = found else {
            panic!("no blockade flip found around the start for '{text}'");
        };
        s.news.clear();
        s.time = t1;
        s.news_scanned_to = t0;
        s.news_tick();
        assert!(
            s.news.iter().any(|n| n.text.contains(text)),
            "{text}: {:?}",
            s.news.iter().map(|n| &n.text).collect::<Vec<_>>()
        );
    }
}

fn has(s: &Sim, topic: Topic, sev: Severity, needle: &str) -> bool {
    s.alerts()
        .iter()
        .any(|a| a.topic == topic && a.sev == sev && a.text.contains(needle))
}

#[test]
fn link_pests_to_alerts_an_infested_colony_warns() {
    let (mut s, ci) = crate::colony::tests::founded();
    assert!(!has(&s, Topic::Colony, Severity::Warning, "RAVAGEURS"));
    s.colonies[ci].pest = 0.3;
    assert!(has(&s, Topic::Colony, Severity::Warning, "RAVAGEURS"));
}

#[test]
fn link_heat_and_power_to_mind_alerts_a_throttled_mind_warns() {
    let mut s = sim();
    assert!(!s.alerts().iter().any(|a| a.topic == Topic::Mind));
    s.ship.sys.temp_k = crate::design::T_DAMAGE;
    assert!(s.mind_throttle() < crate::alerts::MIND_THROTTLE_WARNING);
    assert!(s.alerts().iter().any(|a| a.topic == Topic::Mind));
}

#[test]
fn link_radiation_to_alerts_a_dose_is_told_asleep_or_awake() {
    let mut s = sim();
    assert!(!s.alerts().iter().any(|a| a.text.contains("DOSE AIGUE")));
    s.crew.members[0].body.acute_dose = 1.5;
    assert!(has(&s, Topic::Radiation, Severity::Warning, "DOSE AIGUE"));
    s.crew.members[0].body.acute_dose = 5.0;
    s.crew.members[0].body.cryo = true;
    assert!(has(&s, Topic::Radiation, Severity::Critical, "(EN CRYO)"));
}

fn ctx_of(s: &Sim) -> Ctx {
    s.surprise_ctx()
}

#[test]
fn link_new_states_to_surprises_cryo_sickness_dose_language_and_terraforming_are_read() {
    let mut s = sim();
    let mut ctx = ctx_of(&s);
    for c in [Cond::Cryo(1), Cond::Terraform, Cond::Sick, Cond::Dose(1.0)] {
        assert!(
            !s.surprise_cond(&c, &mut ctx),
            "{c:?} is false at the start"
        );
    }
    s.crew.members[0].body.cryo = true;
    s.crew.members[0].body.acute_dose = 2.0;
    s.crew.members[0].disease = Some((0, 5.0));
    let (mut t, ci) = crate::colony::tests::founded();
    t.colonies[ci]
        .projects
        .push(crate::megastructure::ProjectInst {
            def: 0,
            arg: 0,
            target: 3,
            units: 1,
            stage: 0,
            progress: 0.0,
            taper: 1.0,
            health: 1.0,
            enabled: true,
            wait: 0,
            short: 0,
        });
    s.colonies = t.colonies.clone();
    s.here = t.here;
    let mut ctx = ctx_of(&s);
    for c in [Cond::Cryo(1), Cond::Terraform, Cond::Sick, Cond::Dose(1.0)] {
        assert!(
            s.surprise_cond(&c, &mut ctx),
            "{c:?} holds once the state is there"
        );
    }
    assert!(!s.surprise_cond(&Cond::Cryo(2), &mut ctx));
    assert!(!s.surprise_cond(&Cond::Dose(3.0), &mut ctx));
    // The language heard where somebody holds the space.
    s.faction = Some(0);
    let mut ctx = ctx_of(&s);
    let heard = ctx.lingo.expect("a language is spoken here");
    assert!(s.surprise_cond(&Cond::Language(Cmp::Lt, heard + 0.01), &mut ctx));
    assert!(s.surprise_cond(&Cond::Language(Cmp::Gt, heard - 0.01), &mut ctx));
    s.faction = None;
    let mut ctx = ctx_of(&s);
    assert!(
        !s.surprise_cond(&Cond::Language(Cmp::Lt, 1.0), &mut ctx),
        "nobody speaks"
    );
}

#[test]
fn link_housing_to_colonisation_no_kit_is_lost_on_a_world_with_nowhere_to_live() {
    use vector_world::BodyKind;
    let (mut s, _) = crate::colony::tests::founded();
    let giant = s
        .system
        .bodies
        .iter()
        .position(|b| matches!(b.kind, BodyKind::Planet(k) if k.is_giant()));
    let Some(g) = giant else {
        return; // this system has no giant: the rule is exercised by the other seeds
    };
    s.spawn_near(g, s.bodies[g].radius + 5_000.0);
    let before = s.credits;
    let err = s
        .found_colony(g, None)
        .expect_err("a gas giant has no housing site");
    assert!(err.contains("habitat"), "{err}");
    assert_eq!(s.credits, before, "nothing was spent");
}

#[test]
fn modules_data_keeps_exhaust_velocities_below_a_fifth_of_light() {
    let d = data();
    for m in d.modules.iter().filter(|m| m.thrust > 0.0) {
        let ve = m.isp * crate::G0;
        assert!(
            ve < 0.2 * crate::navigation::C,
            "{}: exhaust velocity {ve:.3e} m/s is relativistic",
            m.id
        );
    }
}

#[test]
fn link_ethos_to_alerts_goods_the_local_law_bans_are_flagged_before_the_docks() {
    let mut s = sim();
    let f = s.faction.expect("a faction holds the start");
    let banned = s.banned_goods(f);
    let Some(&r) = banned.first() else {
        return; // this faction bans nothing: nothing to warn about
    };
    assert!(!has(
        &s,
        Topic::Law,
        Severity::Warning,
        "MARCHANDISES INTERDITES"
    ));
    s.ship.cargo.cap_mass = 1.0e6;
    s.ship.cargo.cap_vol = 1.0e6;
    let d = Arc::clone(&s.data);
    s.ship.cargo.add(&d, r, 10.0);
    assert!(has(
        &s,
        Topic::Law,
        Severity::Warning,
        "MARCHANDISES INTERDITES"
    ));
}
