//! Each game rule must visibly change the simulation.
use crate::combat::ProjKind;
use crate::npc::{AiMode, NpcRole, NpcState};
use crate::rules::Rules;
use crate::{Event, ShipInput, Sim};
use std::sync::Arc;
use vector_core::Vec3;
use vector_data::GameData;
use vector_world::galaxy::Galaxy;
use vector_world::{generate_system, BodyKind};

fn data() -> Arc<GameData> {
    crate::test_support::data()
}

fn sim() -> Sim {
    let g = Galaxy::new(8);
    let e = g
        .nearby([0.0; 3], 150.0)
        .into_iter()
        .find(|e| {
            let s = generate_system(e.seed);
            g.territory(e.pos, 8).is_none()
                && s.bodies
                    .iter()
                    .any(|b| matches!(b.kind, BodyKind::Asteroid(_)))
                && s.bodies.iter().any(|b| b.kind == BodyKind::Station)
        })
        .expect("frontier system");
    let mut s = Sim::new_in_galaxy(g, &e, data());
    s.researched.fill(true);
    s.ship.pos = Vec3::new(4.0e11, 0.0, 0.0);
    s.ship.vel = Vec3::ZERO;
    s.update_ref();
    s
}

fn with(r: impl FnOnce(&mut Rules)) -> Sim {
    let mut s = sim();
    let mut rules = Rules::default();
    r(&mut rules);
    s.set_rules(rules);
    s
}

fn fuel_after_burn(s: &mut Sim) -> f64 {
    s.ship.assist = false;
    let f0 = s.ship.fuel;
    for _ in 0..300 {
        s.step(&ShipInput {
            fwd: 1.0,
            ..Default::default()
        });
    }
    f0 - s.ship.fuel
}

#[test]
fn fuel_use_scales_consumption() {
    let normal = fuel_after_burn(&mut with(|_| {}));
    let double = fuel_after_burn(&mut with(|r| r.fuel_use = 2.0));
    let none = fuel_after_burn(&mut with(|r| r.fuel_use = 0.0));
    let infinite = fuel_after_burn(&mut with(|r| r.infinite_resources = true));
    assert!(normal > 1.0);
    assert!((double / normal - 2.0).abs() < 0.05, "{double} vs {normal}");
    assert_eq!(none, 0.0);
    assert_eq!(infinite, 0.0);
}

fn crew_after_neglect(rules: impl FnOnce(&mut Rules)) -> (f32, f32) {
    let mut s = with(rules);
    let data = Arc::clone(&s.data);
    let p = crate::crew::generate_person(&data, 3, None);
    s.crew.hire(&data, p, 0, 0.0);
    s.ship.cargo.kg.fill(0.0);
    s.ship.cargo.mass = 0.0;
    // No life support either.
    let ls: Vec<usize> = s
        .ship
        .design
        .modules(&data)
        .filter(|(_, m)| m.category == "life_support")
        .map(|(i, _)| i)
        .collect();
    for i in ls {
        s.ship.sys.health[i] = 0.0;
    }
    s.ship.refresh_stats(&data);
    s.advance(12.0 * crate::crew::DAY, &ShipInput::default());
    let p = &s.crew.members[0];
    (p.health, p.hunger)
}

#[test]
fn crew_needs_can_be_switched_off() {
    let (h_on, _) = crew_after_neglect(|_| {});
    let (h_off, hunger_off) = crew_after_neglect(|r| r.crew_needs = false);
    assert!(
        h_on < 0.2,
        "without food, water and air the crew is dying: {h_on}"
    );
    assert!(
        h_off > 0.8 && hunger_off < 0.4,
        "needs off: {h_off}, {hunger_off}"
    );
}

#[test]
fn permadeath_blocks_the_insurer() {
    let s = with(|r| r.permadeath = true);
    assert!(!s.can_respawn());
    assert!(with(|_| {}).can_respawn());
}

fn hull_lost_to_a_hit(rules: impl FnOnce(&mut Rules)) -> f64 {
    let mut s = with(rules);
    s.ship.sys.shield_pts = 0.0;
    let h0 = s.ship.hull;
    s.damage_player(300.0);
    h0 - s.ship.hull
}

#[test]
fn damage_multiplier_and_invulnerability() {
    let base = hull_lost_to_a_hit(|_| {});
    let double = hull_lost_to_a_hit(|r| r.damage = 2.0);
    let zero = hull_lost_to_a_hit(|r| r.damage = 0.0);
    let god = hull_lost_to_a_hit(|r| r.invulnerable = true);
    assert!(base > 50.0);
    // Twice the energy is a little more than twice the harm to the ship as a whole, but a
    // deeper blow reaches the cabins: the hull alone takes slightly less than twice.
    assert!((1.7..2.05).contains(&(double / base)), "{}", double / base);
    assert_eq!(zero, 0.0);
    assert_eq!(god, 0.0);
}

#[test]
fn build_costs_scale_and_can_be_free() {
    let price = |rules: Box<dyn FnOnce(&mut Rules)>| {
        let mut s = with(rules);
        let st = s
            .system
            .bodies
            .iter()
            .position(|b| b.kind == BodyKind::Station)
            .expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.credits = 1.0e7;
        let m = s.data.module_index("radiator_mk2").expect("module");
        let c0 = s.credits;
        s.shop_buy_module(m).expect("buy");
        c0 - s.credits
    };
    let base = price(Box::new(|_| {}));
    let half = price(Box::new(|r| r.build_cost = 0.5));
    let free = price(Box::new(|r| r.infinite_resources = true));
    assert!(base > 1000.0);
    assert!((half / base - 0.5).abs() < 1e-9);
    assert_eq!(free, 0.0);
}

#[test]
fn research_speed_multiplies_points() {
    let gain = |speed: f64| {
        let mut s = with(|r| r.research_speed = speed);
        let data = Arc::clone(&s.data);
        let planet = s
            .system
            .bodies
            .iter()
            .position(|b| matches!(b.kind, BodyKind::Planet(k) if !k.is_giant()))
            .expect("planet");
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        for (id, kg) in [
            ("construction_materials", 3000.0),
            ("colonists", 400.0),
            ("food", 300.0),
            ("water", 600.0),
        ] {
            s.ship
                .cargo
                .add(&data, data.resource_index(id).expect("res"), kg);
        }
        s.credits = 1.0e6;
        s.ship.pos = s.bodies[planet].pos - Vec3::new(0.0, 0.0, s.bodies[planet].radius + 5000.0);
        s.ship.vel = s.bodies[planet].vel;
        s.update_ref();
        let ci = s.found_colony(planet, None).expect("colony");
        s.colonies[ci].research = 100.0;
        s.research_pool = 0.0;
        s.tick_research(0.0);
        s.research_pool
    };
    assert!((gain(2.0) / gain(1.0) - 2.0).abs() < 1e-6);
}

#[test]
fn aggression_scales_firepower_and_zero_is_pacifist() {
    let hurt = |aggr: f64| {
        let mut s = with(|r| r.ai_aggression = aggr);
        s.ship.assist = false;
        let k = s
            .npcs
            .iter()
            .position(|n| n.role == NpcRole::Pirate)
            .expect("pirate");
        s.npcs[k].pos = s.ship.pos + Vec3::new(0.0, 0.0, 3000.0);
        s.npcs[k].vel = s.ship.vel;
        s.npcs[k].hostile = true;
        s.npcs[k].state = NpcState::Combat {
            mode: AiMode::Pursue,
        };
        let h0 = s.ship.hull;
        for _ in 0..120 {
            s.ship.sys.shield_pts = 0.0;
            s.step(&ShipInput::default());
        }
        h0 - s.ship.hull
    };
    let (calm, normal, fierce) = (hurt(0.0), hurt(1.0), hurt(2.0));
    assert_eq!(calm, 0.0, "a pacifist galaxy never hurts");
    assert!(normal > 5.0 && fierce > normal * 1.5, "{normal} {fierce}");
    // Pirates do not even notice rich ships.
    let mut s = with(|r| r.ai_aggression = 0.0);
    let data = Arc::clone(&s.data);
    s.ship.cargo.cap_mass = 1.0e6;
    s.ship.cargo.cap_vol = 1.0e6;
    s.ship
        .cargo
        .add(&data, data.resource_index("gold").expect("gold"), 100.0);
    let k = s
        .npcs
        .iter()
        .position(|n| n.role == NpcRole::Pirate)
        .expect("pirate");
    s.npcs[k].last_scan = -1.0e9;
    s.ship.pos = s.npcs[k].pos + Vec3::new(5000.0, 0.0, 0.0);
    s.events.clear();
    s.update_npcs();
    assert!(!s.events.iter().any(|e| matches!(e, Event::PirateAlert)));
}

#[test]
fn event_frequency_scales_crew_stories() {
    let stories = |f: f64| {
        let mut s = with(|r| r.event_freq = f);
        let data = Arc::clone(&s.data);
        for i in 0..4 {
            let p = crate::crew::generate_person(&data, 40 + i, None);
            s.crew.hire(&data, p, 0, 0.0);
        }
        s.ship.cargo.cap_mass = 1.0e7;
        s.ship.cargo.cap_vol = 1.0e7;
        for (id, kg) in [("food", 5000.0), ("water", 9000.0)] {
            s.ship
                .cargo
                .add(&data, data.resource_index(id).expect("res"), kg);
        }
        s.credits = 1.0e9;
        s.advance(40.0 * crate::crew::DAY, &ShipInput::default());
        s.crew.log.len()
    };
    let (none, normal, many) = (stories(0.0), stories(1.0), stories(3.0));
    assert!(normal > none + 10, "normal {normal} vs none {none}");
    assert!(many > normal, "more events at x3: {many} vs {normal}");
}

#[test]
fn revealing_the_map_removes_fog_of_war() {
    let far = [900.0, 0.0, 900.0];
    let s = with(|_| {});
    assert!(!s.is_seen(far));
    let s = with(|r| r.reveal_map = true);
    assert!(s.is_seen(far));
    assert!(
        s.scan.iter().all(|&l| l >= crate::voyage::SCAN_SURVEYED),
        "bodies are identified too"
    );
}

#[test]
fn turning_tech_locks_off_unlocks_every_hull() {
    let locked = |s: &Sim| {
        s.data
            .hulls
            .iter()
            .filter(|h| !s.is_unlocked("hull", &h.id))
            .count()
    };
    let mut strict = with(|r| r.tech_locks = true);
    strict.researched.iter_mut().for_each(|r| *r = false);
    assert!(
        locked(&strict) > 0,
        "some hull needs a technology when locks are on"
    );
    let free = with(|_| {});
    assert_eq!(locked(&free), 0);
}

#[test]
fn infinite_resources_make_ammunition_free() {
    let shots = |infinite: bool| {
        let mut s = with(|r| r.infinite_resources = infinite);
        s.refit(
            "frigate",
            &[
                "fusion_mk3",
                "torch_mk3",
                "tank_mk3",
                "lifesup_mk1",
                "cargo_mk3",
                "massdriver_mk1",
            ],
        )
        .expect("refit");
        let data = Arc::clone(&s.data);
        let ammo = data.resource_index("ammo_slugs").expect("ammo");
        s.ship.cargo.add(&data, ammo, 100.0);
        let a0 = s.ship.cargo.kg[ammo];
        for _ in 0..90 {
            s.step(&ShipInput {
                fire: true,
                ..Default::default()
            });
        }
        (
            a0 - s.ship.cargo.kg[ammo],
            s.projectiles
                .iter()
                .filter(|p| matches!(p.kind, ProjKind::Kinetic))
                .count(),
        )
    };
    let (used_n, fired_n) = shots(false);
    let (used_i, fired_i) = shots(true);
    assert!(used_n > 0.5 && fired_n > 0);
    assert_eq!(used_i, 0.0, "no ammunition used");
    assert!(fired_i > 0, "but the guns still fire");
}
