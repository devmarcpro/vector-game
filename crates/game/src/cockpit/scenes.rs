//! Scripted scenes of the cockpit for `vector-game --snapshot shots/x.png <scene> [WxH]` and for
//! the tests: free flight in low orbit with a plan of burns ready, and the landing approach with
//! the descent pilot flying the burn.
//!
//! Scene names: `cockpit_a`, `cockpit_b`, `cockpit_c`, `cockpit_c_open`, `cockpit_old` (free
//! flight) and the same with `cockpit_land_` (approach), e.g. `cockpit_land_b`. After the layout,
//! `_tip_<organ>` rests the pointer on an organ (its tooltip), `_menu_<organ>` opens its menu,
//! `_help_<organ>` its help box and `_f11` the piloting menu, e.g. `cockpit_a_tip_lever`.
use super::{Disposition, Id};
use crate::game::Game;
use vector_core::{Mat3, Vec3};
use vector_sim::landing::Sas;

/// What a scene name asks for.
#[derive(Clone, Copy)]
pub struct Spec {
    /// The scene of `main::snapshot` that sets the game up before `after_setup`.
    pub base: &'static str,
    pub land: bool,
    pub layout: Disposition,
    pub open: bool,
    /// What the pointer does once the scene is set: (kind of the extra, organ).
    pub extra: Option<(Extra, Id)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Extra {
    Tip,
    Menu,
    Help,
    F11,
}

/// The organ a scene name designates.
fn organ_named(s: &str) -> Option<Id> {
    Some(match s {
        "lever" => Id::Lever,
        "gear" => Id::Gear,
        "shield" => Id::Shield,
        "radar" => Id::Radar,
        "lights" => Id::Lights,
        "scope" => Id::Scope,
        "trim" => Id::Trim,
        "speed" => Id::SpeedDial,
        "attitude" => Id::Attitude,
        "target" => Id::Target,
        "plan" => Id::PlanScreen,
        "nav" => Id::NavScreen,
        "systems" => Id::Systems,
        "lamp" => Id::Lamp,
        "sas" => Id::Sas,
        _ => return None,
    })
}

pub fn resolve(name: &str) -> Option<Spec> {
    let rest = name.strip_prefix("cockpit_")?;
    let (land, rest) = match rest.strip_prefix("land_") {
        Some(r) => (true, r),
        None => (false, rest),
    };
    let mut extra = None;
    let mut rest = rest;
    for (tag, kind) in [("_tip_", Extra::Tip), ("_menu_", Extra::Menu), ("_help_", Extra::Help)] {
        if let Some((a, b)) = rest.split_once(tag) {
            extra = Some((kind, organ_named(b)?));
            rest = a;
            break;
        }
    }
    if let Some(a) = rest.strip_suffix("_f11") {
        extra = Some((Extra::F11, Id::Layout));
        rest = a;
    }
    let (layout, open) = match rest {
        "a" => (Disposition::A, false),
        "b" => (Disposition::B, false),
        "c" => (Disposition::C, false),
        "c_open" => (Disposition::C, true),
        "old" => (Disposition::Old, false),
        _ => return None,
    };
    Some(Spec {
        base: if land { "land_descent" } else { "cockpit" },
        land,
        layout,
        open,
        extra,
    })
}

/// Choose the arrangement and set the scene up (after the base scene of the snapshot); `size` is
/// the window the scene will be drawn in.
pub fn after_setup(g: &mut Game, spec: &Spec, size: (usize, usize)) {
    g.tut_step = None;
    g.settings.cockpit = spec.layout.index();
    g.settings.cockpit_open = spec.open;
    if spec.land {
        setup_land(g);
    } else {
        setup_free(g);
    }
    let Some((kind, id)) = spec.extra else {
        return;
    };
    if kind == Extra::F11 {
        g.cockpit.menu.set(true);
        return;
    }
    // Draw once so that the organs are where the frame puts them, then act on one.
    let mut fb = vector_render::Framebuffer::new(size.0, size.1);
    crate::draw_frame(&mut fb, g);
    let found = {
        let hits = g.pointer.hits.borrow();
        g.cockpit
            .organs
            .borrow()
            .iter()
            .find(|o| o.id == id)
            .map(|o| (*o, hits[o.hit].rect))
    };
    let Some((o, r)) = found else {
        return;
    };
    let at = ((r.0 + r.2) / 2, (r.1 + r.3) / 2);
    g.pointer.pos = at;
    match kind {
        Extra::Tip => g.pointer.hover.set((at, 99)),
        Extra::Menu => g.pointer_context(at.0, at.1),
        Extra::Help => g.cockpit.help.set(Some((o.id, o.n))),
        Extra::F11 => {}
    }
}

/// Free flight: the ship on a circular orbit, a plan of burns ready, a target planet, a few
/// contacts around, and a few things worn so that the lamps have something to say.
pub fn setup_free(g: &mut Game) {
    use vector_sim::nav_auto::{Cond, Order};
    use vector_sim::nav_predict::{ManeuverNode, NodeWhen};
    use vector_world::BodyKind;
    g.sim.reveal_all();
    // The world to circle: the largest solid planet, low enough for the game to draw its ground
    // (and high enough for the landing aids to stay out of the way).
    let c = (1..g.sim.bodies.len())
        .filter(|&i| matches!(g.sim.bodies[i].kind, BodyKind::Planet(p) if !p.is_giant()))
        .max_by(|&a, &b| g.sim.bodies[a].radius.total_cmp(&g.sim.bodies[b].radius))
        .unwrap_or(1);
    let b = g.sim.bodies[c].clone();
    let r = b.radius + (b.radius * 0.029).clamp(40_000.0, 190_000.0);
    // The star 30 degrees over the horizon, the ship flying across the light.
    let sun = (b.pos * -1.0).normalized();
    let side = sun.cross(Vec3::new(0.0, 1.0, 0.0)).normalized();
    let radial = (sun * 0.5 + side * 0.866).normalized();
    let vdir = Vec3::new(0.0, 1.0, 0.0).cross(radial).normalized();
    g.sim.ship.pos = b.pos + radial * r;
    g.sim.ship.vel = b.vel + vdir * (b.gm / r).sqrt();
    g.sim.ship.rot = Mat3::look_at((vdir * 0.995 - radial * 0.1).normalized(), radial);
    g.sim.update_ref_pub();
    let other = (1..g.sim.bodies.len())
        .filter(|&i| i != c && matches!(g.sim.bodies[i].kind, BodyKind::Planet(_)))
        .max_by(|&a, &b| {
            let ra = g.sim.system.bodies[a].orbit.as_ref().map_or(0.0, |o| o.a);
            let rb = g.sim.system.bodies[b].orbit.as_ref().map_or(0.0, |o| o.a);
            ra.total_cmp(&rb)
        })
        .unwrap_or(1);
    g.sim.target = Some(other);
    let km = 1000.0;
    let _ = g.sim.nav_add(Order::Burn(ManeuverNode::new(
        NodeWhen::Periapsis,
        Some(c),
        180.0,
        0.0,
        0.0,
    )));
    let _ = g.sim.nav_add(Order::Orbit {
        body: c,
        peri_alt: 1200.0 * km,
        apo_alt: 3000.0 * km,
        stage: 0,
    });
    let _ = g
        .sim
        .nav_add_guarded(Order::Wait(600.0), Some(Cond::FuelAbove(0.5)));
    let _ = g.sim.nav_add(Order::Transfer {
        target: other,
        alt: 400.0 * km,
        aero: false,
    });
    let _ = g.sim.nav_add(Order::ReturnStation);
    g.nav_ui.preview = true;
    g.sim.set_sas(Sas::Prograde);
    g.sim.ship.assist = true;
    g.sim.set_throttle(0.6);
    g.sim.active_sensors = true;
    let _ = g.sim.launch_decoy();
    g.sim.ship.fuel = g.sim.ship.stats.fuel_cap * 0.21;
    g.sim.ship.hull = g.sim.ship.stats.hull_max * 0.46;
    g.sim.ship.sys.temp_k = 575.0;
    g.map = false;
    g.rear_view = false;
    // Contacts around the ship (no further step: they stay where they are put).
    let up = g.sim.ship.rot.col(1);
    let rot = g.sim.ship.rot;
    let ship_pos = g.sim.ship.pos;
    let ship_vel = g.sim.ship.vel;
    for (k, (d, side, hi)) in [
        (3_500.0, 0.30, 0.06),
        (14_000.0, -0.55, 0.10),
        (46_000.0, 0.85, -0.05),
    ]
    .iter()
    .enumerate()
    {
        if let Some(n) = g.sim.npcs.get_mut(k) {
            n.pos = ship_pos + rot.col(2) * *d + rot.col(0) * (*d * side) + up * (*d * hi);
            n.vel = ship_vel + rot.col(0) * 4.0;
        }
    }
    g.say("PLAN DE NAVIGATION PRET : ALLUMAGE AU PERIAPSIS");
    g.say("SAS : PROGRADE");
}

/// The landing approach: the descent pilot flying the burn over a solid world.
pub fn setup_land(g: &mut Game) {
    g.sim.ship.hull = g.sim.ship.stats.hull_max * 0.9;
    g.map = false;
    g.say("PILOTE : ATTERRISSAGE  SITE MARQUE");
}
