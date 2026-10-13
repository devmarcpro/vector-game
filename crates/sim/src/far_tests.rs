//! Tests of the long voyages: drive classes, planning, execution, galaxies, saves.
use crate::far::{Dest, Voyage};
use crate::scene::new_sim;
use crate::test_support::data;
use crate::Sim;
use vector_world::galaxy::StarRef;

pub(crate) fn rig(drive: &str) -> Sim {
    let mut s = new_sim(5, data());
    let slot = s
        .ship
        .design
        .modules(&s.data)
        .find(|(_, m)| m.jump_range > 0.0)
        .map(|(i, _)| i)
        .expect("an FTL drive");
    s.ship.design.slots[slot] = s.data.module_index(drive);
    // Enough power for the drive: every reactor slot that can takes the void reactor.
    let void = s.data.module_index("exo_void_reactor");
    for i in 0..s.ship.design.slots.len() {
        let is_reactor = s.ship.design.slots[i].is_some_and(|m| {
            s.data.modules[m].category == "reactor"
                && s.data.modules[m].slot == vector_data::SlotType::Core
        });
        if is_reactor && s.data.hulls[s.ship.design.hull].slots[i].1 >= 2 {
            s.ship.design.slots[i] = void;
        }
    }
    // A magnetic trap holds the antimatter of the intergalactic drive: it takes the place of a
    // cargo hold (or an empty internal slot), never of a tank.
    if drive == "ftl_intergalactic" {
        let trap = s.data.module_index("exo_penning_trap");
        let hull = &s.data.hulls[s.ship.design.hull];
        let spot = (0..s.ship.design.slots.len()).find(|&i| {
            hull.slots[i].0 == vector_data::SlotType::Internal
                && hull.slots[i].1 >= 2
                && s.ship.design.slots[i].is_none_or(|m| s.data.modules[m].category == "cargo")
        });
        if let Some(i) = spot {
            s.ship.design.slots[i] = trap;
        }
    }
    s.ship.refresh_stats(&s.data.clone());
    s.ship.fuel = s.ship.stats.fuel_cap;
    s.undock();
    s.ship.pos = vector_core::Vec3::new(4.0e11, 0.0, 0.0);
    s.update_ref();
    s
}

/// A star near `p` (ly) of the galaxy of `s`.
fn star_near(s: &Sim, p: [f64; 3]) -> StarRef {
    for r in [30.0, 60.0, 120.0] {
        if let Some(e) = s.galaxy.nearby(p, r).into_iter().next() {
            return e.id;
        }
    }
    panic!("no star near {p:?}");
}

/// The ship has no magnetic trap: antimatter would leak away, keep the stock up for the test.
fn topup_antimatter(s: &mut Sim) {
    if let Some(r) = s.data.resource_index("antimatter") {
        if s.galaxy_id != 0 || s.ship.cargo.kg[r] > 0.0 {
            let have = s.ship.cargo.kg[r];
            if have < 300.0 {
                let data = std::sync::Arc::clone(&s.data);
                s.ship.cargo.add(&data, r, 300.0 - have);
            }
        }
    }
}

fn dump(s: &Sim) -> String {
    match s.voyage.as_ref() {
        Some(v) => format!(
            "next {} paused {:?} refuel {} repair {} fuel {:.0} jump {} log {:?}",
            v.next,
            v.paused,
            v.refueling,
            v.repairing,
            s.ship.fuel,
            s.jump.is_some(),
            v.log.iter().rev().take(4).collect::<Vec<_>>()
        ),
        None => "no voyage".into(),
    }
}

/// Run the voyage until it ends; resume it after each stop (refilling the tank).
/// Returns the number of stops.
fn run_voyage(s: &mut Sim, max_steps: usize) -> usize {
    let mut stops = 0;
    for _ in 0..max_steps {
        topup_antimatter(s);
        s.advance(50_000.0, &Default::default());
        let Some(v) = s.voyage.as_ref() else { break };
        if v.done {
            break;
        }
        assert!(stops < 400, "too many stops: {}", dump(s));
        if v.paused.is_some() {
            stops += 1;
            s.ship.fuel = s.ship.stats.fuel_cap;
            s.voyage_resume().expect("resume");
        }
    }
    stops
}

#[test]
fn drive_classes_are_ordered_and_playable() {
    let mut last = 0.0;
    for (d, class) in [
        ("ftl_mk4", "courte"),
        ("ftl_mk5", "longue"),
        ("ftl_galactic", "galactique"),
        ("ftl_intergalactic", "intergalactique"),
    ] {
        let s = rig(d);
        let p = s.drive_profile();
        assert_eq!(p.class, class);
        assert!(p.range > last, "{d}");
        last = p.range;
        // A full-range jump never costs more than the tank holds.
        let c = s.jump_fuel_cost(p.range);
        assert!(c < s.ship.stats.fuel_cap * 0.6, "{d}: {c}");
        assert!(c > 100.0, "{d}: {c}");
    }
    // The classic drive is unchanged.
    let s = rig("ftl_mk1");
    let p = s.drive_profile();
    assert_eq!(
        (p.hours_per_ly, p.fuel_mult, p.risk, p.charge),
        (6.0, 1.0, 0.01, 20.0)
    );
}

#[test]
fn a_galactic_route_respects_range_fuel_and_is_deterministic() {
    let s = rig("ftl_mk5");
    let goal = star_near(&s, [-26000.0, 0.0, 12000.0]);
    let a = s.voyage_plan(Dest::Star(goal)).expect("plan");
    let b = s.voyage_plan(Dest::Star(goal)).expect("plan");
    assert_eq!(a.legs.len(), b.legs.len());
    assert!(a.legs.iter().zip(&b.legs).all(|(x, y)| x.star == y.star));
    assert!(a.legs.len() > 30, "{}", a.legs.len());
    let range = s.drive_profile().range;
    let mut prev = s.here_pos;
    for l in &a.legs {
        let d = ((l.pos[0] - prev[0]).powi(2)
            + (l.pos[1] - prev[1]).powi(2)
            + (l.pos[2] - prev[2]).powi(2))
        .sqrt();
        assert!(d <= range + 1e-6, "hop {d} over range {range}");
        assert!((d - l.dist).abs() < 1e-6);
        prev = l.pos;
    }
    assert_eq!(a.legs.last().unwrap().star, goal);
    assert!(
        a.refuels > 0 && a.halt_s > 0.0,
        "the tank cannot last the whole trip"
    );
    assert!(a.ok, "{:?}", a.why);
    assert!(a.total_ly > 10_000.0 && a.travel_s > 86_400.0 * 100.0 && a.credits > 0.0);
}

#[test]
fn a_fifty_hop_voyage_runs_to_the_end_at_high_speed() {
    let mut s = rig("ftl_mk5");
    let goal = star_near(&s, [-14000.0, 0.0, 0.0]);
    let plan = s.voyage_plan(Dest::Star(goal)).expect("plan");
    assert!(plan.legs.len() >= 45, "{}", plan.legs.len());
    s.voyage_begin(plan).expect("begin");
    assert!(s.voyage_warp().is_some());
    let t0 = s.time;
    let stops = run_voyage(&mut s, 20_000);
    let v = s.voyage.as_ref().unwrap();
    assert!(v.done, "{:?} {:?}", v.paused, v.log.last());
    assert_eq!(s.here, goal);
    assert!(s.time - t0 > 86_400.0 * 100.0);
    assert!(s.ship.fuel >= 0.0 && s.ship.fuel.is_finite());
    assert!(v.log.iter().any(|l| l.1.contains("DESTINATION ATTEINTE")));
    assert!(stops < 40, "{stops}");
}

#[test]
fn the_voyage_stops_when_the_tank_runs_dry_and_resumes() {
    let mut s = rig("ftl_mk5");
    let goal = star_near(&s, [-4000.0, 0.0, 0.0]);
    s.voyage_go(Dest::Star(goal)).expect("go");
    for _ in 0..200 {
        s.advance(50_000.0, &Default::default());
        if s.voyage.as_ref().is_some_and(|v| v.next >= 1) {
            break;
        }
    }
    // Sabotage: empty the tank right after the first jump.
    s.ship.fuel = 0.0;
    for _ in 0..200 {
        s.advance(50_000.0, &Default::default());
        if s.voyage
            .as_ref()
            .is_some_and(|v| v.paused.is_some() || v.refueling)
        {
            break;
        }
    }
    let v = s.voyage.as_ref().unwrap();
    assert!(v.paused.is_some() || v.refueling, "{:?}", v.log.last());
    run_voyage(&mut s, 20_000);
    assert!(s.voyage.as_ref().unwrap().done);
    assert_eq!(s.here, goal);
}

#[test]
fn an_intergalactic_trip_goes_and_comes_back() {
    let mut s = rig("ftl_intergalactic");
    let r = s.data.resource_index("antimatter").expect("antimatter");
    let data = std::sync::Arc::clone(&s.data);
    s.ship.cargo.cap_mass = 1.0e6;
    s.ship.cargo.cap_vol = 1.0e6;
    s.ship.cargo.add(&data, r, 400.0);
    s.universe_seed = s.galaxy.seed;
    s.change_reputation(Some(0), 40.0);
    let rep0 = s.reputation_with(Some(0));
    let (home_here, home_seed) = (s.here, s.galaxy.seed);
    let uni = vector_world::universe::Universe::new(s.universe_seed);
    let near = uni.galaxies_near([0.0; 3], 3.0e6);
    let other = near
        .iter()
        .find(|g| g.id != 0)
        .expect("a neighbour galaxy")
        .id;
    let plan = s.voyage_plan(Dest::Galaxy(other)).expect("plan");
    assert!(plan.ok, "{:?}", plan.why);
    assert!(plan.intergalactic && plan.total_ly > 1.0e6);
    assert!(
        plan.travel_s > 86_400.0 * 200.0,
        "an expedition, not a teleport"
    );
    s.voyage_begin(plan).expect("begin");
    run_voyage(&mut s, 20_000);
    assert!(s.voyage.as_ref().unwrap().done);
    assert_eq!(s.galaxy_id, other);
    assert_eq!(s.galaxy.seed, uni.galaxy_seed(other));
    assert_ne!(s.galaxy.seed, home_seed);
    assert_eq!(s.reputation_with(Some(0)), 0.0, "a stranger far from home");
    assert!(s.galaxy_stash.contains_key(&0));
    // And back.
    s.ship.fuel = s.ship.stats.fuel_cap;
    s.voyage_go(Dest::Galaxy(0)).expect("back");
    run_voyage(&mut s, 20_000);
    assert!(s.voyage.as_ref().unwrap().done);
    assert_eq!(s.galaxy_id, 0);
    assert_eq!(s.galaxy.seed, home_seed);
    assert_eq!(s.here, home_here, "back where we left");
    assert!(s.reputation_with(Some(0)) > 0.25 * rep0, "home remembers");
}

#[test]
fn a_voyage_survives_saving_and_loading_midway() {
    let mut a = rig("ftl_mk5");
    let goal = star_near(&a, [-6000.0, 0.0, 0.0]);
    a.voyage_go(Dest::Star(goal)).expect("go");
    for _ in 0..30 {
        a.advance(50_000.0, &Default::default());
    }
    assert!(
        a.voyage.as_ref().is_some_and(|v| !v.done && v.next >= 1),
        "{}",
        dump(&a)
    );
    let bytes = a.save_bytes().expect("save");
    let mut b = Sim::load_bytes(&bytes, data()).expect("load");
    assert_eq!(
        b.voyage.as_ref().map(|v: &Voyage| v.next),
        a.voyage.as_ref().map(|v| v.next)
    );
    for s in [&mut a, &mut b] {
        run_voyage(s, 60_000);
    }
    assert!(a.voyage.as_ref().unwrap().done && b.voyage.as_ref().unwrap().done);
    assert_eq!(a.here, goal);
    assert_eq!(b.here, goal);
}

#[test]
fn route_planning_and_stepping_stay_fast() {
    let s = rig("ftl_mk5");
    // Across the whole disc: about two hundred hops of 300 ly.
    let goal = star_near(&s, [-26000.0, 0.0, 34000.0]);
    let t = std::time::Instant::now();
    let plan = s.voyage_plan(Dest::Star(goal)).expect("plan");
    let cold = t.elapsed();
    let t = std::time::Instant::now();
    let again = s.voyage_plan(Dest::Star(goal)).expect("plan");
    let warm = t.elapsed();
    assert_eq!(plan.legs.len(), again.legs.len());
    eprintln!(
        "PERF plan {} hops: cold {cold:?}, warm {warm:?}",
        plan.legs.len()
    );
    assert!(plan.legs.len() > 100, "{}", plan.legs.len());
    assert!(cold.as_secs_f64() < 30.0, "{cold:?}");
    assert!(warm.as_secs_f64() < 3.0, "{warm:?}");
    // Stepping during a voyage.
    let mut s = s;
    let near = star_near(&s, [-26000.0, 0.0, 600.0]);
    s.voyage_go(Dest::Star(near)).expect("go");
    let t = std::time::Instant::now();
    let steps0 = s.steps;
    for _ in 0..200 {
        s.advance(50_000.0, &Default::default());
    }
    let n = (s.steps - steps0).max(1);
    let ms = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
    eprintln!("PERF {n} steps, {ms:.4} ms per step");
    assert!(ms < 20.0, "{ms}");
}

#[test]
fn profile_the_parts_of_a_hop() {
    let mut s = rig("ftl_mk5");
    let goal = star_near(&s, [-26000.0, 0.0, 600.0]);
    let e = s.galaxy.star(goal).expect("star");
    let time = |label: &str, n: usize, f: &mut dyn FnMut()| {
        let t = std::time::Instant::now();
        for _ in 0..n {
            f();
        }
        eprintln!(
            "PROFILE {label}: {:.3} ms",
            t.elapsed().as_secs_f64() * 1000.0 / n as f64
        );
    };
    time("generate_system", 5, &mut || {
        let _ = vector_world::generate_system(e.seed);
    });
    time("alerts", 20, &mut || {
        let _ = s.alerts();
    });
    time("spawn_traffic", 5, &mut || s.spawn_traffic());
    time("compute_faction", 20, &mut || s.compute_faction());
    time("step 600s (no transit)", 50, &mut || {
        s.step_dt(&Default::default(), 600.0)
    });
    let sys = vector_world::generate_system(e.seed);
    time("load_system", 5, &mut || s.load_system(sys.clone()));
    time("refresh_stats", 20, &mut || {
        s.ship.refresh_stats(&s.data.clone())
    });
}
