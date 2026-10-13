//! Tests of the cockpit: every arrangement draws without panic at every size, the zones of the
//! organs stay on the screen and never overlap, each organ does what its key does, the thrust
//! lever is the real throttle, the options are saved.
use super::draw::AMBER_LED;
use super::*;
use crate::game::{Screen, DEFAULT_SEED};
use std::sync::Arc;
use vector_render::Framebuffer;

fn game() -> Game {
    let dir = vector_data::find_data_dir().expect("data");
    let data = Arc::new(vector_data::GameData::load(&dir).expect("data"));
    let mut g = Game::new(data, DEFAULT_SEED);
    g.new_game(DEFAULT_SEED);
    g.tut_step = None;
    g.settings = crate::settings::Settings::default();
    g.sim.undock();
    g.screen = Screen::Flight;
    g.cockpit.invalidate();
    g
}

/// A game in free flight in the arrangement `d`, the scene of `scenes::setup_free`.
fn free(d: Disposition) -> Game {
    let mut g = game();
    scenes::setup_free(&mut g);
    g.settings.cockpit = d.index();
    g.settings.cockpit_open = false;
    g
}

fn frame_at(g: &Game, w: usize, h: usize) -> Framebuffer {
    let mut fb = Framebuffer::new(w, h);
    crate::draw_frame(&mut fb, g);
    fb
}

const SIZES: [(usize, usize); 6] = [
    (1024, 576),
    (1280, 720),
    (1920, 1080),
    (2560, 1440),
    (1366, 768),
    (900, 520),
];

/// Every arrangement (C folded and unfolded), flying free and on the descent, in every window.
fn each_scene(mut f: impl FnMut(&str, &mut Game, usize, usize)) {
    for (d, open, name) in [
        (Disposition::A, false, "A"),
        (Disposition::B, false, "B"),
        (Disposition::C, false, "C"),
        (Disposition::C, true, "C ouverte"),
        (Disposition::Old, false, "ancien"),
    ] {
        for land in [false, true] {
            for (w, h) in SIZES {
                let mut g = if land {
                    let mut g = game();
                    crate::landing_scene(&mut g, "descent");
                    g.settings.cockpit = d.index();
                    g
                } else {
                    free(d)
                };
                g.settings.cockpit_open = open;
                let label = format!("{name} {w}x{h}{}", if land { " approche" } else { "" });
                f(&label, &mut g, w, h);
            }
        }
    }
}

#[test]
fn every_arrangement_draws_at_every_size() {
    each_scene(|label, g, w, h| {
        let fb = frame_at(g, w, h);
        // Something was drawn over the whole window (no hole of black rows at the bottom of
        // the console but the rows of the command bar and the ticker).
        let lit = fb.px.iter().filter(|&&p| p != 0).count();
        assert!(lit > w * h / 4, "{label}: only {lit} pixels lit");
    });
}

/// The zones of the organs: inside the frame, and none over another unless it contains it.
#[test]
fn organ_zones_stay_on_screen_and_do_not_overlap() {
    let mut bad: Vec<String> = Vec::new();
    each_scene(|label, g, w, h| {
        let _ = frame_at(g, w, h);
        if g.cockpit.last.get().is_none() {
            return;
        }
        let hits = g.pointer.hits.borrow();
        let organs = g.cockpit.organs.borrow();
        assert!(!organs.is_empty(), "{label}: no organ");
        for o in organs.iter() {
            let r = hits[o.hit].rect;
            if r.0 < 0 || r.1 < 0 || r.2 > w as i32 || r.3 > h as i32 || r.2 <= r.0 || r.3 <= r.1 {
                bad.push(format!(
                    "{label}: {:?}/{} zone {r:?} off the frame",
                    o.id, o.n
                ));
            }
        }
        for (i, a) in organs.iter().enumerate() {
            for b in organs.iter().skip(i + 1) {
                let (ra, rb) = (hits[a.hit].rect, hits[b.hit].rect);
                let ow = ra.2.min(rb.2) - ra.0.max(rb.0);
                let oh = ra.3.min(rb.3) - ra.1.max(rb.1);
                let inside = |p: (i32, i32, i32, i32), q: (i32, i32, i32, i32)| {
                    p.0 >= q.0 && p.2 <= q.2 && p.1 >= q.1 && p.3 <= q.3
                };
                // The blips of the radar sit on its scope.
                let on_scope =
                    matches!((a.id, b.id), (Id::Scope, Id::Blip) | (Id::Blip, Id::Scope));
                if ow > 2 && oh > 2 && !inside(ra, rb) && !inside(rb, ra) && !on_scope {
                    bad.push(format!(
                        "{label}: {:?}/{} {ra:?} overlaps {:?}/{} {rb:?}",
                        a.id, a.n, b.id, b.n
                    ));
                }
            }
        }
    });
    bad.sort();
    bad.dedup();
    assert!(
        bad.is_empty(),
        "{} problems:\n{}",
        bad.len(),
        bad[..bad.len().min(25)].join("\n")
    );
}

/// Click the centre of the topmost organ of this kind; false when there is none.
fn click(g: &mut Game, id: Id, n: usize) -> bool {
    let found = {
        let hits = g.pointer.hits.borrow();
        g.cockpit
            .organs
            .borrow()
            .iter()
            .rev()
            .find(|o| o.id == id && usize::from(o.n) == n)
            .map(|o| {
                let r = hits[o.hit].rect;
                ((r.0 + r.2) / 2, (r.1 + r.3) / 2)
            })
    };
    match found {
        Some((x, y)) => {
            g.pointer_click(x, y, 0);
            g.pointer_release();
            true
        }
        None => false,
    }
}

/// Everything an organ can change in the simulation, for comparing a click with its key.
fn state_of_sim(g: &Game) -> String {
    let s = &g.sim;
    format!(
        "{:?} {} {:.2} {} {} {} {} {} {}",
        s.sas,
        s.gear_down,
        s.throttle,
        s.lights_on,
        s.shield_on,
        s.trim,
        s.active_sensors,
        s.decoys.len(),
        s.autopilot.is_some()
    )
}

/// A game with what the organs need to do something (a shield generator, a radar, a launcher).
fn equipped(d: Disposition) -> Game {
    let mut g = free(d);
    let data = Arc::clone(&g.sim.data);
    let hull = data.hull_index("frigate").expect("hull");
    g.sim.ship.design = vector_sim::design::ShipDesign::empty(&data, hull);
    for id in [
        "fusion_mk3",
        "torch_mk3",
        "battery_mk3",
        "radiator_mk4",
        "lifesup_mk1",
        "tank_mk3",
        "shield_mk1",
        "sensor_active_mk1",
        "decoy_mk1",
    ] {
        g.sim
            .ship
            .design
            .install_first_fit(&data, id)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    g.sim.ship.sys =
        vector_sim::design::Systems::new(&g.sim.ship.design, g.sim.ship.stats.battery_cap);
    g.sim.ship.refresh_stats(&data);
    g.sim.ship.hull = g.sim.ship.stats.hull_max;
    g.sim.ship.fuel = g.sim.ship.stats.fuel_cap * 0.8;
    g.sim.ship.sys.battery_mj = g.sim.ship.stats.battery_cap;
    g.sim.weapon_cd = vec![0.0; g.sim.ship.design.slots.len()];
    g.cockpit.invalidate();
    g
}

#[test]
fn each_organ_does_what_its_key_does() {
    // (organ, number, key): the click and the key must leave the simulation in the same state.
    let cases: [(Id, usize, KeyCode); 21] = [
        (Id::Gear, 0, KeyCode::Numpad3),
        (Id::Shield, 0, KeyCode::NumpadDivide),
        (Id::Radar, 0, KeyCode::Period),
        (Id::Decoy, 0, KeyCode::Comma),
        (Id::Lights, 0, KeyCode::NumpadDecimal),
        (Id::Auto, 0, KeyCode::KeyG),
        (Id::Sas, 0, KeyCode::Digit0),
        (Id::Sas, 1, KeyCode::Digit1),
        (Id::Sas, 2, KeyCode::Digit2),
        (Id::Sas, 3, KeyCode::Digit3),
        (Id::Sas, 4, KeyCode::Digit4),
        (Id::Sas, 5, KeyCode::Digit6),
        (Id::Sas, 6, KeyCode::Digit8),
        (Id::Sas, 7, KeyCode::Digit9),
        (Id::Trim, 1, KeyCode::BracketRight),
        (Id::Trim, 0, KeyCode::BracketLeft),
        (Id::PilotBtn, 3, KeyCode::Numpad8),
        (Id::PilotBtn, 4, KeyCode::Numpad2),
        (Id::PilotBtn, 5, KeyCode::Numpad0),
        (Id::PilotBtn, 1, KeyCode::Numpad7),
        (Id::Scope, 0, KeyCode::Tab),
    ];
    for d in [Disposition::A, Disposition::B] {
        for (id, n, key) in cases {
            // The pilot panel only exists on the approach: use the descent scene for it.
            let land = id == Id::PilotBtn;
            let make = || {
                let mut g = if land {
                    let mut g = game();
                    crate::landing_scene(&mut g, "descent");
                    g.settings.cockpit = d.index();
                    g
                } else {
                    equipped(d)
                };
                g.settings.cockpit = d.index();
                g
            };
            let mut by_click = make();
            let mut by_key = make();
            let _ = frame_at(&by_click, 1280, 720);
            assert!(click(&mut by_click, id, n), "{d:?}: no {id:?}/{n} organ");
            by_key.key(key, None, true);
            by_key.key(key, None, false);
            assert_eq!(
                state_of_sim(&by_click),
                state_of_sim(&by_key),
                "{d:?}: {id:?}/{n} and {key:?} must do the same"
            );
        }
    }
}

#[test]
fn the_toggles_really_toggle_and_light_up() {
    let mut g = equipped(Disposition::A);
    let _ = frame_at(&g, 1280, 720);
    let gear = g.sim.gear_down;
    assert!(click(&mut g, Id::Gear, 0));
    assert_ne!(g.sim.gear_down, gear);
    let lights = g.sim.lights_on;
    let _ = frame_at(&g, 1280, 720);
    assert!(click(&mut g, Id::Lights, 0));
    assert_ne!(g.sim.lights_on, lights);
    let shield = g.sim.shield_on;
    let _ = frame_at(&g, 1280, 720);
    assert!(click(&mut g, Id::Shield, 0));
    assert_ne!(
        g.sim.shield_on, shield,
        "the generator is installed: the switch works"
    );
    // Another click puts it back.
    let _ = frame_at(&g, 1280, 720);
    assert!(click(&mut g, Id::Shield, 0));
    assert_eq!(g.sim.shield_on, shield);
}

#[test]
fn the_lever_is_the_throttle() {
    let mut g = free(Disposition::A);
    let _ = frame_at(&g, 1280, 720);
    let l = g.cockpit.lever.get();
    assert!(l.bottom > l.top + 50, "the slot is drawn");
    let lever_x = {
        let hits = g.pointer.hits.borrow();
        let o = *g
            .cockpit
            .organs
            .borrow()
            .iter()
            .find(|o| o.id == Id::Lever)
            .expect("lever");
        (hits[o.hit].rect.0 + hits[o.hit].rect.2) / 2
    };
    // A click at a height of the slot sets that share of the throttle.
    for f in [0.0, 0.25, 0.5, 0.8, 1.0] {
        let y = l.bottom - (f * f64::from(l.bottom - l.top)) as i32;
        g.pointer_click(lever_x, y, 0);
        g.pointer_release();
        assert!((g.sim.throttle - f).abs() < 0.03, "{f}: {}", g.sim.throttle);
        let _ = frame_at(&g, 1280, 720);
    }
    // A drag follows the pointer.
    let (x, top) = (lever_x, l.top);
    g.pointer_click(x, l.bottom, 0);
    g.pointer.down = true;
    g.pointer_move(x, top + (l.bottom - top) / 2);
    assert!((g.sim.throttle - 0.5).abs() < 0.03, "{}", g.sim.throttle);
    g.pointer_move(x, top);
    assert!((g.sim.throttle - 1.0).abs() < 1e-9);
    g.pointer.down = false;
    g.pointer_release();
    g.pointer_move(x, l.bottom);
    assert!(
        (g.sim.throttle - 1.0).abs() < 1e-9,
        "released: no longer dragging"
    );
    // The wheel over the lever moves it by 5 %.
    g.sim.set_throttle(0.5);
    g.pointer.pos = (x, (top + l.bottom) / 2);
    let _ = frame_at(&g, 1280, 720);
    g.pointer_wheel(1.0);
    assert!((g.sim.throttle - 0.55).abs() < 1e-9, "{}", g.sim.throttle);
    g.pointer_wheel(-2.0);
    assert!(g.sim.throttle < 0.55);
    // The keys: plus 10 %.
    g.sim.set_throttle(0.3);
    g.key(KeyCode::Numpad8, None, true);
    assert!((g.sim.throttle - 0.4).abs() < 1e-9);
}

#[test]
fn the_new_keys_drive_the_new_commands() {
    let mut g = equipped(Disposition::A);
    g.key(KeyCode::NumpadDecimal, None, true);
    assert!(g.sim.lights_on);
    g.key(KeyCode::NumpadDivide, None, true);
    assert!(!g.sim.shield_on);
    g.key(KeyCode::BracketRight, None, true);
    g.key(KeyCode::BracketRight, None, true);
    assert_eq!(g.sim.trim, 2);
    g.key(KeyCode::BracketLeft, None, true);
    assert_eq!(g.sim.trim, 1);
    // Shift + F11 changes the arrangement and saves it.
    g.keys.insert(KeyCode::ShiftLeft);
    let d0 = g.settings.cockpit;
    g.key(KeyCode::F11, None, true);
    assert_eq!(g.settings.cockpit, (d0 + 1) % 4);
    assert_eq!(
        crate::settings::Settings::from_text(&g.settings.to_text()).cockpit,
        g.settings.cockpit
    );
}

#[test]
fn the_options_choose_and_save_the_arrangement() {
    let mut s = crate::settings::Settings::default();
    assert_eq!(s.cockpit, Disposition::A.index(), "A by default");
    assert_eq!(s.describe().len(), crate::settings::Settings::COUNT);
    let row = crate::settings::Settings::COUNT - 1;
    assert_eq!(s.describe()[row].0, "Disposition du cockpit");
    for want in [1, 2, 3, 0] {
        s.adjust(row, true);
        assert_eq!(s.cockpit, want);
        assert_eq!(crate::settings::Settings::from_text(&s.to_text()), s);
    }
    s.adjust(row, false);
    assert_eq!(s.cockpit, 3, "left goes to the old HUD");
    s.cockpit_open = true;
    let back = crate::settings::Settings::from_text(&s.to_text());
    assert!(back.cockpit_open, "the console state is remembered");
    // Garbage is clamped.
    assert_eq!(
        crate::settings::Settings::from_text("cockpit=9\n").cockpit,
        3
    );
    // The option screen changes it with the arrows and the menu line of F11 with a click.
    let mut g = game();
    g.open_options(Screen::Flight);
    g.opt_sel = row;
    g.key(KeyCode::ArrowRight, None, true);
    assert_eq!(g.settings.cockpit, 1);
    g.screen = Screen::Flight;
    g.cockpit_cycle(0);
    assert_eq!(g.settings.cockpit, 2);
}

#[test]
fn the_folding_console_remembers_its_state() {
    let mut g = free(Disposition::C);
    let _ = frame_at(&g, 1280, 720);
    assert!(!g.settings.cockpit_open);
    assert!(click(&mut g, Id::Handle, 0), "the handle is drawn");
    assert!(g.settings.cockpit_open);
    let _ = frame_at(&g, 1280, 720);
    // Unfolded: the console has its organs.
    assert!(g.cockpit.organs.borrow().iter().any(|o| o.id == Id::Lever));
    g.key(KeyCode::Quote, None, true);
    assert!(!g.settings.cockpit_open);
    let _ = frame_at(&g, 1280, 720);
    assert!(!g.cockpit.organs.borrow().iter().any(|o| o.id == Id::Lever));
}

#[test]
fn the_approach_replaces_navigation_and_plan_by_descent_and_pilot() {
    for d in [Disposition::A, Disposition::B, Disposition::C] {
        let mut g = game();
        crate::landing_scene(&mut g, "descent");
        g.settings.cockpit = d.index();
        g.settings.cockpit_open = true;
        let _ = frame_at(&g, 1280, 720);
        let has = |id: Id| g.cockpit.organs.borrow().iter().any(|o| o.id == id);
        assert!(has(Id::DescentScreen), "{d:?}: descent screen");
        assert!(has(Id::PilotScreen), "{d:?}: pilot panel");
        assert!(
            !has(Id::NavScreen) && !has(Id::PlanScreen),
            "{d:?}: no nav/plan in approach"
        );
        // Out of the approach, the navigation and plan screens are back.
        let mut f = free(d);
        f.settings.cockpit_open = true;
        let _ = frame_at(&f, 1280, 720);
        let has = |id: Id| f.cockpit.organs.borrow().iter().any(|o| o.id == id);
        assert!(
            has(Id::NavScreen) && has(Id::PlanScreen),
            "{d:?}: nav and plan"
        );
    }
}

#[test]
fn the_radar_blip_and_the_menu_target_what_is_clicked() {
    let mut g = free(Disposition::A);
    let _ = frame_at(&g, 1280, 720);
    let blip = g
        .cockpit
        .organs
        .borrow()
        .iter()
        .find(|o| o.id == Id::Blip)
        .copied();
    let Some(b) = blip else {
        panic!("a contact is on the radar");
    };
    let r = g.pointer.hits.borrow()[b.hit].rect;
    let (x, y) = ((r.0 + r.2) / 2, (r.1 + r.3) / 2);
    g.sim.target = None;
    g.sim.target_npc = None;
    g.pointer_click(x, y, 0);
    assert!(
        g.sim.target.is_some() || g.sim.target_npc.is_some(),
        "a click on a blip targets it"
    );
    // A right click on the scope: the menu has CIBLER ICI and the help of the organ.
    let _ = frame_at(&g, 1280, 720);
    let scope = g
        .cockpit
        .organs
        .borrow()
        .iter()
        .find(|o| o.id == Id::Scope)
        .copied()
        .expect("scope");
    let r = g.pointer.hits.borrow()[scope.hit].rect;
    let (cx, cy) = ((r.0 + r.2) / 2, r.1 + 4);
    g.pointer_context(cx, cy);
    let m = g.ctx_menu.as_ref().expect("a menu");
    let labels: Vec<&str> = m.items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"CIBLER ICI"), "{labels:?}");
    assert!(labels.contains(&"AIDE SUR CET ORGANE"), "{labels:?}");
    // The help item opens the help box of the organ.
    let k = labels
        .iter()
        .position(|l| *l == "AIDE SUR CET ORGANE")
        .unwrap();
    g.ctx_pick(k);
    assert_eq!(g.cockpit.help.get().map(|h| h.0), Some(Id::Scope));
    let fb = frame_at(&g, 1280, 720);
    assert!(fb.px.contains(&AMBER_LED), "the help box is drawn");
    g.key(KeyCode::KeyP, None, true);
    assert!(g.cockpit.help.get().is_none(), "a key closes it");
}

#[test]
fn every_organ_has_a_tooltip_and_a_menu() {
    for d in [Disposition::A, Disposition::B, Disposition::C] {
        let mut g = free(d);
        g.settings.cockpit_open = true;
        let _ = frame_at(&g, 1280, 720);
        let organs: Vec<Organ> = g.cockpit.organs.borrow().clone();
        for o in organs {
            let r = g.pointer.hits.borrow()[o.hit].rect;
            let at = ((r.0 + r.2) / 2, (r.1 + r.3) / 2);
            // The organ found under the centre of its zone is one drawn after it at most.
            assert!(
                organ_under(&g, at).is_some(),
                "{d:?}: {:?} unreachable",
                o.id
            );
            let tip = tip_lines(&g, o);
            assert!(tip.len() >= 2 && !tip[0].0.is_empty());
            let (title, items) = menu_for(&g, o, at);
            assert!(!title.is_empty());
            assert!(items.iter().any(|i| i.label == "AIDE SUR CET ORGANE"));
            let (name, _, help) = spec(o.id, o.n);
            assert!(!name.is_empty() && help.len() > 10 || o.id == Id::FKey);
        }
    }
}

#[test]
fn the_other_states_are_drawn_right() {
    // Docked, third-person view, map, destroyed hull, a tiny window: no panic, the console stays.
    for d in [Disposition::A, Disposition::B, Disposition::C] {
        let mut g = free(d);
        g.settings.cockpit_open = true;
        g.ext_view = true;
        let _ = frame_at(&g, 1280, 720);
        assert!(
            g.cockpit.organs.borrow().iter().any(|o| o.id == Id::Lever),
            "{d:?} external view"
        );
        g.ext_view = false;
        g.map = true;
        let _ = frame_at(&g, 1280, 720);
        assert!(
            g.cockpit.organs.borrow().iter().any(|o| o.id == Id::Lever),
            "{d:?} map"
        );
        g.map = false;
        g.sim.ship.hull = 0.0;
        let _ = frame_at(&g, 1280, 720);
        g.sim.ship.hull = g.sim.ship.stats.hull_max;
        // A window too small for the cockpit: the old HUD takes over.
        let _ = frame_at(&g, 600, 360);
        assert_eq!(g.cockpit.last.get(), None);
        // A window too small for A or B: C.
        let _ = frame_at(&g, 800, 480);
        assert_eq!(g.cockpit.last.get(), Some(Disposition::C), "{d:?}");
    }
    // Docked at a station.
    let mut g = free(Disposition::A);
    let st = g.sim.nearest_station().map(|s| s.0);
    g.sim.docked = st;
    let _ = frame_at(&g, 1280, 720);
    // The old HUD is still the fourth choice.
    let mut g = free(Disposition::Old);
    let _ = frame_at(&g, 1280, 720);
    assert_eq!(g.cockpit.last.get(), None);
    g.settings.cockpit = Disposition::A.index();
    let _ = frame_at(&g, 1280, 720);
    assert_eq!(g.cockpit.last.get(), Some(Disposition::A));
}

#[test]
fn spread_shares_the_room_between_minimum_and_maximum() {
    // Short of room: between the minimums and the preferences, exactly the room.
    let (w, left) = layouts::spread(600, [(100, 200, 300, 1), (100, 300, 400, 1)]);
    assert_eq!(w.iter().sum::<i32>(), 600);
    assert!(w[0] >= 100 && w[1] >= 100 && left == 0);
    // Not even the minimums: the minimums (the caller falls back).
    let (w, _) = layouts::spread(150, [(100, 200, 300, 1), (100, 300, 400, 1)]);
    assert!(w[0] + w[1] >= 150);
    // Plenty of room: the maximums, the rest is left over.
    let (w, left) = layouts::spread(2000, [(100, 200, 300, 1), (100, 300, 400, 1)]);
    assert_eq!(w, [300, 400]);
    assert_eq!(left, 1300);
    // In between: more than the preferences, never past the maximums.
    let (w, left) = layouts::spread(600, [(100, 200, 300, 1), (100, 250, 400, 3)]);
    assert!(w[0] > 200 && w[1] > 250 && w[0] <= 300 && w[1] <= 400);
    assert_eq!(w.iter().sum::<i32>() + left, 600);
}

/// Drawing the console costs little: in empty space (the scene is nothing), each arrangement
/// stays within a loose ceiling of the old HUD plus a few milliseconds. The figures are in
/// docs/PERFORMANCE.md (`vector-game --bench`).
#[test]
fn the_console_stays_cheap() {
    let mut old = 0.0;
    for d in [
        Disposition::Old,
        Disposition::A,
        Disposition::B,
        Disposition::C,
    ] {
        let mut g = game();
        g.settings.cockpit = d.index();
        let mut fb = Framebuffer::new(1280, 720);
        crate::draw_frame(&mut fb, &g);
        let t = std::time::Instant::now();
        for _ in 0..30 {
            crate::draw_frame(&mut fb, &g);
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0 / 30.0;
        eprintln!("cockpit {d:?} 1280x720 in empty space: {ms:.2} ms/frame");
        if d == Disposition::Old {
            old = ms;
        } else {
            // A very loose bound: this is a test build on a shared machine.
            assert!(
                ms < old + 40.0,
                "{d:?}: {ms} ms against {old} ms for the old HUD"
            );
        }
    }
}
