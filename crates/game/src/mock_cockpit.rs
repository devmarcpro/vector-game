//! MAQUETTES DU COCKPIT (a retirer avec `mock_ui`) : la vue pilotage en vrai tableau de bord.
//! La scene 3D/vectorielle reste visible dans une grande verriere ; autour, une console
//! d'instruments (interrupteurs a bascule, boutons-poussoirs lumineux, manette de poussee,
//! cadrans, jauges a LED, radar, ecrans de navigation et de plan, voyants d'alerte, touches de
//! fonction). Tout est cliquable (zones `Act::Key` / `Act::Call` du jeu) et anime (voyants qui
//! clignotent, balayage du radar). Trois mises en page : A (console basse large), B (panneaux
//! lateraux et console compacte), C (HUD minimal et console repliable).
#![allow(clippy::too_many_arguments)]

use crate::game::Game;
use crate::mock_ui::{
    arc, big, cut, disc, fill, fit, frame, halo, hl, line_over, ring, tint, tw, txt, txt_b, txt_c,
    txt_r, txt_v, vgrad, vl, Rc, GOOD, INK, WARN,
};
use crate::mouse::Act;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use vector_core::{Mat3, Vec3};
use vector_render::{mix_color, scale_color, Framebuffer};
use vector_sim::landing::{PilotKind, Readout, Sas};
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    A,
    B,
    C,
    COpen,
}

// ---- the scenes --------------------------------------------------------------------------------

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

// ---- telemetry ---------------------------------------------------------------------------------

struct Tel {
    t: f32,
    blink: bool,
    blink_slow: bool,
    spd_orb: f64,
    spd_surf: Option<f64>,
    spd_tgt: Option<f64>,
    ux: f32,
    uy: f32,
    sinp: f32,
    heading: f32,
    alt: Option<f64>,
    vv: Option<f64>,
    vh: Option<f64>,
    fuel: f64,
    dv: f64,
    dv_frac: f64,
    batt: f64,
    temp_k: f64,
    temp: f64,
    shield: Option<f64>,
    hull: f64,
    dose_mgy: f64,
    dose_frac: f64,
    gload: f64,
    g_frac: f64,
    thr: f64,
    gear: bool,
    sas: Sas,
    assist: bool,
    auto: bool,
    radar_on: bool,
    decoys: usize,
    pilot: Option<PilotKind>,
    landing: Option<Readout>,
    lamps: [(&'static str, u8); 8],
}

static SPEED_MODE: AtomicUsize = AtomicUsize::new(usize::MAX);
static LIGHTS_ON: AtomicBool = AtomicBool::new(true);
static SHIELD_ON: AtomicBool = AtomicBool::new(true);
static CONSOLE_OPEN: AtomicBool = AtomicBool::new(false);

impl Tel {
    fn new(g: &Game) -> Tel {
        let sim = &g.sim;
        let ship = &sim.ship;
        let t = crate::proto_view::anim_time();
        let surf = sim.surface_state();
        let up_w = if let Some(s) = &surf {
            s.up
        } else if let Some(b) = sim.ref_body {
            (ship.pos - sim.bodies[b].pos).normalized()
        } else {
            Vec3::new(0.0, 1.0, 0.0)
        };
        let f = ship.forward();
        let (sr, su) = (ship.rot.col(0), ship.rot.col(1));
        let (ux, uy) = (up_w.dot(sr) as f32, up_w.dot(su) as f32);
        let sinp = f.dot(up_w).clamp(-1.0, 1.0) as f32;
        let mut north = Vec3::new(0.0, 1.0, 0.0);
        if north.dot(up_w).abs() > 0.95 {
            north = Vec3::new(1.0, 0.0, 0.0);
        }
        let north = (north - up_w * north.dot(up_w)).normalized();
        let east = north.cross(up_w);
        let mut heading = f.dot(east).atan2(f.dot(north)).to_degrees() as f32;
        if heading < 0.0 {
            heading += 360.0;
        }
        let rel = sim.rel_velocity();
        let spd_tgt = if let Some(n) = sim.target_npc.and_then(|k| sim.npcs.get(k)) {
            Some((ship.vel - n.vel).length())
        } else {
            sim.target
                .and_then(|i| sim.bodies.get(i))
                .map(|b| (ship.vel - b.vel).length())
        };
        let landing = sim.landing_readout();
        let mut full = ship.clone();
        full.fuel = full.stats.fuel_cap;
        let dv = ship.delta_v();
        let dv_full = full.delta_v().max(1.0);
        let fuel = ship.fuel / ship.stats.fuel_cap.max(1.0);
        let batt = ship.sys.battery_mj / ship.stats.battery_cap.max(1.0);
        let shield = (sim.effective.shield_cap > 0.0)
            .then(|| (ship.sys.shield_pts / sim.effective.shield_cap).clamp(0.0, 1.0));
        let hull = ship.hull / ship.stats.hull_max.max(1.0);
        let rate = sim.dose_rate_aboard();
        let gload = sim.structural_load_g();
        let temp =
            ((ship.sys.temp_k - 150.0) / (vector_sim::design::T_SCRAM - 150.0)).clamp(0.0, 1.0);
        let env = sim.environment_at(ship.pos);
        let alerts = g.alerts();
        let collision = alerts
            .iter()
            .filter(|a| a.topic == vector_sim::alerts::Topic::Collision)
            .map(|a| match a.sev {
                vector_sim::alerts::Severity::Critical => 2u8,
                _ => 1,
            })
            .max()
            .unwrap_or(0);
        let impact = landing
            .as_ref()
            .is_some_and(|r| r.burn_now || r.t_impact.is_some_and(|t| t < 12.0 && r.v_up < -8.0));
        let lvl = |bad2: bool, bad1: bool| -> u8 {
            if bad2 {
                2
            } else {
                u8::from(bad1)
            }
        };
        let lamps = [
            ("COLLISION", if impact { 2 } else { collision }),
            ("CARBURANT", lvl(fuel < 0.1, fuel < 0.25)),
            (
                "SURCHAUFFE",
                lvl(
                    ship.sys.temp_k > vector_sim::design::T_DAMAGE,
                    ship.sys.temp_k > 500.0,
                ),
            ),
            (
                "RADIATION",
                lvl(rate > 0.05, rate > 0.005 || env.radiation > 0.0),
            ),
            ("COQUE", lvl(hull < 0.25, hull < 0.5)),
            ("ENERGIE", lvl(batt < 0.1, batt < 0.25)),
            (
                "BOUCLIER",
                lvl(
                    shield.is_some_and(|s| s < 0.1),
                    shield.is_some_and(|s| s < 0.25),
                ),
            ),
            ("AIR / VIE", lvl(sim.crew.air < 0.5, sim.crew.air < 0.8)),
        ];
        Tel {
            t,
            blink: (t * 3.0) as i64 % 2 == 0,
            blink_slow: (t * 1.2) as i64 % 2 == 0,
            spd_orb: rel.length(),
            spd_surf: surf.as_ref().map(|s| s.v.length()),
            spd_tgt,
            ux,
            uy,
            sinp,
            heading,
            alt: landing.as_ref().map(|r| r.alt),
            vv: landing.as_ref().map(|r| r.v_up),
            vh: landing.as_ref().map(|r| r.v_h),
            fuel,
            dv,
            dv_frac: (dv / dv_full).clamp(0.0, 1.0),
            batt,
            temp_k: ship.sys.temp_k,
            temp,
            shield,
            hull,
            dose_mgy: rate * 1e3,
            dose_frac: (rate / 0.1).clamp(0.0, 1.0),
            gload,
            g_frac: (gload / vector_sim::navigation::STRUCTURE_LIMIT_G).clamp(0.0, 1.0),
            thr: sim.throttle,
            gear: sim.gear_down,
            sas: sim.sas,
            assist: ship.assist,
            auto: sim.autopilot.is_some(),
            radar_on: sim.active_sensors,
            decoys: sim.decoys.len(),
            pilot: sim.pilot.map(|p| p.kind),
            landing,
            lamps,
        }
    }

    /// Which speed the dial shows: 0 surface, 1 orbit, 2 target.
    fn speed_mode(&self) -> usize {
        let m = SPEED_MODE.load(Ordering::Relaxed);
        if m <= 2 {
            return m;
        }
        if self.spd_surf.is_some() {
            0
        } else {
            1
        }
    }
}

// ---- clicks of the mock-up ---------------------------------------------------------------------

fn set_throttle_n(g: &mut Game, n: usize) {
    g.sim.set_throttle(n as f64 / 10.0);
    g.say(format!("ACCELERATEUR {} %", n * 10));
}

fn set_speed_mode(g: &mut Game, n: usize) {
    SPEED_MODE.store(n, Ordering::Relaxed);
    g.say(match n {
        0 => "CADRAN : VITESSE SOL",
        1 => "CADRAN : VITESSE ORBITALE",
        _ => "CADRAN : VITESSE RELATIVE A LA CIBLE",
    });
}

/// Controls that the game does not have yet (shield switch, lights): the mock-up flips them.
fn mock_switch(g: &mut Game, n: usize) {
    let (flag, name) = if n == 0 {
        (&SHIELD_ON, "BOUCLIER")
    } else {
        (&LIGHTS_ON, "LUMIERES")
    };
    let v = !flag.load(Ordering::Relaxed);
    flag.store(v, Ordering::Relaxed);
    g.say(format!(
        "{name} {} (COMMANDE A CREER : MAQUETTE)",
        if v { "ON" } else { "OFF" }
    ));
}

fn toggle_console(g: &mut Game, _: usize) {
    let v = !CONSOLE_OPEN.load(Ordering::Relaxed);
    CONSOLE_OPEN.store(v, Ordering::Relaxed);
    g.say(if v {
        "CONSOLE DEPLOYEE"
    } else {
        "CONSOLE REPLIEE"
    });
}

// ---- metal, screws, lamps ----------------------------------------------------------------------

const METAL: u32 = 0x0019_1E26;
const METAL_HI: u32 = 0x0036_404D;
const METAL_LO: u32 = 0x0006_080B;
const LABEL: u32 = 0x008A_98AA;
const AMBER_LED: u32 = 0x00FF_B000;
const RED_LED: u32 = 0x00FF_3030;
const GREEN_LED: u32 = 0x0040_FF70;
const SCREEN_BG: u32 = 0x0002_0807;

fn screw(fb: &mut Framebuffer, x: i32, y: i32) {
    disc(fb, x, y, 2, 0x004A_5563);
    hl(fb, x - 1, y, 3, 0x0018_1C22);
    fb.px_set(x - 1, y - 1, 0x0090_9CAC);
}

/// A bevelled metal plate with an engraved title.
fn plate(fb: &mut Framebuffer, r: Rc, title: &str) {
    let (x, y, w, h) = r;
    vgrad(
        fb,
        x,
        y,
        w,
        h,
        mix_color(METAL, 0x0000_0000, 0.0),
        mix_color(METAL, 0, 0.35),
    );
    hl(fb, x, y, w, METAL_HI);
    vl(fb, x, y, h, METAL_HI);
    hl(fb, x, y + h - 1, w, METAL_LO);
    vl(fb, x + w - 1, y, h, METAL_LO);
    for (sx, sy) in [
        (x + 5, y + 5),
        (x + w - 6, y + 5),
        (x + 5, y + h - 6),
        (x + w - 6, y + h - 6),
    ] {
        screw(fb, sx, sy);
    }
    if !title.is_empty() {
        txt_c(fb, x + w / 2, y + 4, title, LABEL);
    }
}

/// A recessed window (a screen or a gauge well).
fn well(fb: &mut Framebuffer, r: Rc, bg: u32) {
    let (x, y, w, h) = r;
    fill(fb, x, y, w, h, bg);
    hl(fb, x, y, w, METAL_LO);
    vl(fb, x, y, h, METAL_LO);
    hl(fb, x, y + h - 1, w, METAL_HI);
    vl(fb, x + w - 1, y, h, METAL_HI);
}

/// A lamp: halo and bright when lit, a dull glass when not.
fn lamp(fb: &mut Framebuffer, x: i32, y: i32, c: u32, on: bool) {
    if on {
        halo(fb, x, y, 9, c, 0.7);
        disc(fb, x, y, 4, c);
        disc(fb, x, y, 2, mix_color(c, 0x00FF_FFFF, 0.55));
    } else {
        disc(fb, x, y, 4, mix_color(0x0008_0B0E, c, 0.16));
        ring(fb, x, y, 4, 0x0030_3844);
        fb.px_set(x - 1, y - 1, mix_color(0x0008_0B0E, c, 0.35));
    }
}

// ---- toggle switches ---------------------------------------------------------------------------

/// A toggle switch of 46 x 66 pixels at (x, y): lamp, bezel, lever, engraved label.
fn toggle(
    fb: &mut Framebuffer,
    g: &Game,
    x: i32,
    y: i32,
    label: &str,
    on: bool,
    lamp_c: u32,
    act: Act,
) {
    let hov = g.hit((x, y, x + 46, y + 66), act, None);
    lamp(fb, x + 23, y + 7, lamp_c, on);
    let (cx, cy) = (x + 23, y + 31);
    // Hex nut and bezel.
    disc(fb, cx, cy, 14, 0x0010_1419);
    disc(fb, cx, cy, 13, if hov { 0x0066_7587 } else { 0x0048_5360 });
    disc(fb, cx, cy, 10, 0x0006_080B);
    ring(fb, cx, cy, 13, 0x0090_9CAC);
    // Lever: up when on, down when off.
    let tip_y = if on { cy - 16 } else { cy + 16 };
    for d in -1..=1 {
        line_over(
            fb,
            cx + d,
            cy,
            cx + d,
            tip_y,
            if d == 0 { 0x00E0_E6EE } else { 0x0078_8496 },
        );
    }
    disc(fb, cx, tip_y, 5, if on { 0x00F2_F6FA } else { 0x00A8_B2C0 });
    disc(fb, cx - 1, tip_y - 1, 2, 0x00FF_FFFF);
    txt_c(fb, cx, y + 54, label, if on { INK } else { LABEL });
    if on {
        hl(fb, x + 7, y + 64, 32, lamp_c);
    }
}

// ---- push buttons ------------------------------------------------------------------------------

/// A lit push button: when lit, its face glows in `c` and it looks pressed in.
fn pushbtn(fb: &mut Framebuffer, g: &Game, r: Rc, label: &str, lit: bool, c: u32, act: Act) {
    let (x, y, w, h) = r;
    let hov = g.hit((x, y, x + w, y + h), act, None);
    fill(fb, x, y, w, h, 0x0008_0B0E);
    let face = if lit {
        mix_color(0x0010_1418, c, 0.78)
    } else if hov {
        0x0030_3844
    } else {
        0x0022_2A33
    };
    fill(fb, x + 2, y + 2, w - 4, h - 4, face);
    if lit {
        hl(
            fb,
            x + 2,
            y + h - 3,
            w - 4,
            mix_color(face, 0x00FF_FFFF, 0.25),
        );
        halo(fb, x + w / 2, y + h / 2, (w / 2 + 6).min(40), c, 0.35);
    } else {
        hl(fb, x + 2, y + 2, w - 4, mix_color(face, 0x00FF_FFFF, 0.22));
        vl(fb, x + 2, y + 2, h - 4, mix_color(face, 0x00FF_FFFF, 0.12));
        hl(fb, x + 2, y + h - 3, w - 4, METAL_LO);
    }
    // A LED strip in the top edge.
    fill(
        fb,
        x + w / 2 - 8,
        y + 3,
        16,
        2,
        if lit {
            0x00FF_FFFF
        } else {
            mix_color(0x0008_0B0E, c, 0.4)
        },
    );
    let tc = if lit {
        0x0004_0608
    } else if hov {
        INK
    } else {
        mix_color(LABEL, INK, 0.3)
    };
    let label = fit(label, ((w - 6) / 8) as usize);
    txt_c(fb, x + w / 2, y + h / 2 - 2, &label, tc);
    if lit {
        txt_c(fb, x + w / 2 + 1, y + h / 2 - 2, &label, tc);
    }
    frame(fb, x, y, w, h, if hov { INK } else { 0x0044_4E5C });
}

// ---- LED bar gauges ----------------------------------------------------------------------------

/// A vertical LED bar: small segments coloured by zone (red low / amber / green; reversed for
/// what is bad when high). The label runs up along it (see `systems_v`).
fn led_bar_v(
    fb: &mut Framebuffer,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    f: f64,
    high_bad: bool,
    alarm: bool,
    blink: bool,
) {
    well(fb, (x - 2, y - 2, w + 4, h + 4), SCREEN_BG);
    let seg = 5;
    let n = h / seg;
    let lit = (f.clamp(0.0, 1.0) * f64::from(n)).round() as i32;
    for i in 0..n {
        let k = f64::from(i) / f64::from((n - 1).max(1));
        let kk = if high_bad { 1.0 - k } else { k };
        let c = tint(kk.max(0.02));
        let on = i < lit;
        let yy = y + h - (i + 1) * seg;
        let col = if on {
            if alarm && blink && i + 1 == lit {
                0x00FF_FFFF
            } else {
                c
            }
        } else {
            mix_color(0x0008_0B0E, c, 0.12)
        };
        fill(fb, x, yy, w, seg - 1, col);
        if on {
            hl(fb, x, yy, w, mix_color(col, 0x00FF_FFFF, 0.35));
        }
    }
}

/// A horizontal LED bar with its label (`lw` pixels wide) on the left and value on the right.
fn led_bar_h(
    fb: &mut Framebuffer,
    x: i32,
    y: i32,
    w: i32,
    lw: i32,
    f: f64,
    high_bad: bool,
    label: &str,
    value: &str,
    alarm: bool,
    blink: bool,
) {
    txt(fb, x, y + 1, label, if alarm { RED_LED } else { LABEL });
    let bx = x + lw;
    let bw = w - lw - 54;
    well(fb, (bx - 2, y - 1, bw + 4, 12), SCREEN_BG);
    let n = bw / 6;
    let lit = (f.clamp(0.0, 1.0) * f64::from(n)).round() as i32;
    for i in 0..n {
        let k = f64::from(i) / f64::from((n - 1).max(1));
        let kk = if high_bad { 1.0 - k } else { k };
        let c = tint(kk.max(0.02));
        let on = i < lit;
        let col = if on {
            if alarm && blink && i + 1 == lit {
                0x00FF_FFFF
            } else {
                c
            }
        } else {
            mix_color(0x0008_0B0E, c, 0.12)
        };
        fill(fb, bx + i * 6, y + 1, 5, 8, col);
    }
    txt_r(
        fb,
        x + w,
        y + 1,
        value,
        if alarm && blink { RED_LED } else { INK },
    );
}

// ---- circular instruments ----------------------------------------------------------------------

fn pt(cx: i32, cy: i32, r: f32, a: f32) -> (i32, i32) {
    (
        cx + (r * a.cos()).round() as i32,
        cy + (r * a.sin()).round() as i32,
    )
}

/// A bezel with a dark face.
fn bezel(fb: &mut Framebuffer, cx: i32, cy: i32, r: i32) {
    disc(fb, cx, cy, r + 6, 0x000B_0E12);
    disc(fb, cx, cy, r + 5, 0x003A_4452);
    disc(fb, cx, cy, r + 3, 0x001A_2028);
    ring(fb, cx, cy, r + 5, 0x0078_8496);
    disc(fb, cx, cy, r + 1, 0x0003_0608);
}

/// Needle with a tail and a hub.
fn needle(fb: &mut Framebuffer, cx: i32, cy: i32, r: f32, a: f32, c: u32) {
    let (tx, ty) = pt(cx, cy, r, a);
    let (bx, by) = pt(cx, cy, -9.0, a);
    for d in [-1, 0, 1] {
        let (ox, oy) = (
            (-a.sin() * d as f32).round() as i32,
            (a.cos() * d as f32).round() as i32,
        );
        line_over(
            fb,
            bx + ox,
            by + oy,
            tx + ox,
            ty + oy,
            if d == 0 {
                mix_color(c, 0x00FF_FFFF, 0.3)
            } else {
                c
            },
        );
    }
    disc(fb, cx, cy, 5, 0x0045_4F5C);
    disc(fb, cx, cy, 3, 0x0010_1418);
}

/// A dial from angle `a0` to `a1` with `major` big divisions of `minor` small ones.
fn dial_face(
    fb: &mut Framebuffer,
    cx: i32,
    cy: i32,
    r: i32,
    a0: f32,
    a1: f32,
    major: i32,
    minor: i32,
    labels: &[String],
) {
    bezel(fb, cx, cy, r);
    for i in 0..=major * minor {
        let a = a0 + (a1 - a0) * i as f32 / (major * minor) as f32;
        let big_tick = i % minor == 0;
        let (x0, y0) = pt(cx, cy, r as f32 - 1.0, a);
        let (x1, y1) = pt(cx, cy, r as f32 - if big_tick { 8.0 } else { 4.0 }, a);
        line_over(
            fb,
            x0,
            y0,
            x1,
            y1,
            if big_tick { 0x00E6_F0EC } else { 0x0068_7684 },
        );
        if big_tick {
            if let Some(l) = labels.get((i / minor) as usize) {
                let (lx, ly) = pt(cx, cy, r as f32 - 16.0, a);
                txt_c(fb, lx, ly - 3, l, 0x00C8_D4DC);
            }
        }
    }
}

fn nice_max(v: f64) -> f64 {
    let ladder = [
        100.0, 200.0, 400.0, 800.0, 2e3, 4e3, 8e3, 2e4, 4e4, 8e4, 2e5, 4e5, 8e5, 2e6, 2e7, 2e8,
    ];
    let t = v * 1.1;
    ladder.into_iter().find(|&l| l >= t).unwrap_or(1e9)
}

fn short(v: f64) -> String {
    let a = v.abs();
    if a >= 1e6 {
        format!("{:.0}M", v / 1e6)
    } else if a >= 1e4 {
        format!("{:.0}K", v / 1e3)
    } else if a >= 1e3 {
        if (v / 1e3).fract().abs() < 1e-9 {
            format!("{:.0}K", v / 1e3)
        } else {
            format!("{:.1}K", v / 1e3)
        }
    } else {
        format!("{v:.0}")
    }
}

/// The speed dial: surface, orbit or target speed, with its three-way selector.
fn speed_dial(fb: &mut Framebuffer, g: &Game, tel: &Tel, cx: i32, cy: i32, r: i32) {
    let mode = tel.speed_mode();
    let (v, name) = match mode {
        0 => (tel.spd_surf.unwrap_or(0.0), "SOL"),
        2 => (tel.spd_tgt.unwrap_or(0.0), "CIBLE"),
        _ => (tel.spd_orb, "ORBITE"),
    };
    let top = nice_max(v);
    let labels: Vec<String> = (0..=4).map(|i| short(top * f64::from(i) / 4.0)).collect();
    let (a0, a1) = (std::f32::consts::PI * 0.75, std::f32::consts::PI * 2.25);
    dial_face(fb, cx, cy, r, a0, a1, 4, 5, &labels);
    // A red zone over the last tenth.
    arc(fb, cx, cy, r - 1, a1 - (a1 - a0) * 0.1, a1, 2, 0x00C0_2020);
    let f = (v / top).clamp(0.0, 1.0) as f32;
    needle(fb, cx, cy, r as f32 - 7.0, a0 + (a1 - a0) * f, 0x00FF_8A2A);
    txt_c(fb, cx, cy + r / 2 - 8, &format!("{} M/S", short(v)), INK);
    // Selector.
    for (k, (l, ok)) in [
        ("SOL", tel.spd_surf.is_some()),
        ("ORB", true),
        ("CIB", tel.spd_tgt.is_some()),
    ]
    .iter()
    .enumerate()
    {
        let x = cx - 33 + k as i32 * 24;
        let y = cy + r + 12;
        let on = k == mode;
        let hov = g.hit((x, y, x + 23, y + 11), Act::Call(set_speed_mode, k), None);
        fill(fb, x, y, 22, 11, if on { 0x00B8_7A10 } else { 0x0014_1A20 });
        frame(fb, x, y, 22, 11, if hov { INK } else { 0x0044_4E5C });
        txt_c(
            fb,
            x + 11,
            y + 2,
            l,
            if on {
                0x0004_0608
            } else if *ok {
                LABEL
            } else {
                0x0044_4E58
            },
        );
    }
    let _ = name;
}

/// The vertical speed dial (zero at nine o'clock like an airliner's VSI).
fn vario_dial(fb: &mut Framebuffer, tel: &Tel, cx: i32, cy: i32, r: i32) {
    let vv = tel.vv.unwrap_or(0.0);
    let top = if vv.abs() < 10.0 {
        10.0
    } else if vv.abs() < 50.0 {
        50.0
    } else if vv.abs() < 200.0 {
        200.0
    } else {
        1000.0
    };
    let (a0, a1) = (std::f32::consts::PI * 0.1, std::f32::consts::PI * 1.9);
    bezel(fb, cx, cy, r);
    // Ticks: 0 at the left, + above, - below.
    for i in -10..=10 {
        let f = i as f32 / 10.0;
        let a = std::f32::consts::PI + f * std::f32::consts::PI * 0.84;
        let major = i % 5 == 0;
        let (x0, y0) = pt(cx, cy, r as f32 - 1.0, a);
        let (x1, y1) = pt(cx, cy, r as f32 - if major { 8.0 } else { 4.0 }, a);
        line_over(
            fb,
            x0,
            y0,
            x1,
            y1,
            if major { 0x00E6_F0EC } else { 0x0068_7684 },
        );
        if major {
            let (lx, ly) = pt(cx, cy, r as f32 - 17.0, a);
            let txtv = format!("{:.0}", (top * f64::from(f.abs())).round());
            txt_c(fb, lx, ly - 3, &txtv, 0x00C8_D4DC);
        }
    }
    let _ = (a0, a1);
    let f = (vv / top).clamp(-1.0, 1.0) as f32;
    let a = std::f32::consts::PI + f * std::f32::consts::PI * 0.84;
    needle(
        fb,
        cx,
        cy,
        r as f32 - 7.0,
        a,
        if vv < -5.0 { 0x00FF_5A40 } else { 0x0040_FF70 },
    );
    txt_c(
        fb,
        cx + r / 4,
        cy + r / 2 - 8,
        &format!("{vv:+.0}"),
        if vv < -15.0 { RED_LED } else { INK },
    );
}

/// Artificial horizon: sky and ground from the real attitude of the ship against the local
/// horizon (of the world below, or of the plane of the system), pitch ladder, bank scale.
fn attitude(fb: &mut Framebuffer, tel: &Tel, cx: i32, cy: i32, r: i32) {
    bezel(fb, cx, cy, r);
    let k = r as f32 / 0.62; // pixels per radian: the ball shows +-35 degrees
    let (ux, uy, sp) = (tel.ux, tel.uy, tel.sinp);
    let cosp = (1.0 - sp * sp).max(1e-4).sqrt();
    for dy in -r..=r {
        let w = ((r * r - dy * dy) as f32).sqrt() as i32;
        for dx in -w..=w {
            // Elevation (about) of the direction seen at this pixel.
            let up_px = -dy as f32;
            let e = sp + (dx as f32 * ux + up_px * uy) / k;
            let el = e.clamp(-1.0, 1.0).asin();
            let c = if e > 0.0 {
                // Sky: lighter near the horizon.
                let t = (el / 0.9).clamp(0.0, 1.0);
                mix_color(0x0058_A0E8, 0x0010_3C86, t)
            } else {
                let t = (-el / 0.9).clamp(0.0, 1.0);
                mix_color(0x009A_6B3A, 0x0038_2210, t)
            };
            // The horizon line itself.
            let c = if e.abs() < 0.012 { 0x00F8_FCFF } else { c };
            fb.px_set(cx + dx, cy + dy, c);
        }
    }
    // Pitch ladder: lines at -30..30 degrees.
    let n = (ux.hypot(uy)).max(1e-4);
    let (nx, ny) = (ux / n, uy / n); // "up" of the world in the aircraft frame
    for deg in [-30i32, -20, -10, 10, 20, 30] {
        let s = (deg as f32).to_radians().sin();
        let off = k * (s - sp) / n.max(0.2) * cosp.max(0.2) / cosp.max(0.2);
        let (mx, my) = (cx as f32 + nx * off, cy as f32 - ny * off);
        let half = if deg % 20 == 0 { 12.0 } else { 20.0 };
        let (dxl, dyl) = (-ny * half, -nx * half);
        let (x0, y0) = ((mx - dxl) as i32, (my + dyl) as i32);
        let (x1, y1) = ((mx + dxl) as i32, (my - dyl) as i32);
        clipped_line(fb, cx, cy, r - 3, x0, y0, x1, y1, 0x00F0_F6FF);
        let (tx, ty) = (x1 + 4, y1 - 3);
        if ((tx - cx).pow(2) + (ty - cy).pow(2)) < (r - 10) * (r - 10) {
            txt(fb, tx, ty, &format!("{}", deg.abs()), 0x00F0_F6FF);
        }
    }
    // Bank scale on the rim and the sky pointer.
    for d in [-60, -45, -30, -20, -10, 0, 10, 20, 30, 45, 60] {
        let a = -std::f32::consts::FRAC_PI_2 + (d as f32).to_radians();
        let (x0, y0) = pt(cx, cy, r as f32 - 1.0, a);
        let (x1, y1) = pt(cx, cy, r as f32 - if d % 30 == 0 { 8.0 } else { 5.0 }, a);
        line_over(fb, x0, y0, x1, y1, 0x00F0_F6FF);
    }
    let bank = ux.atan2(uy);
    let a = -std::f32::consts::FRAC_PI_2 + -bank.clamp(-1.2, 1.2);
    let (px, py) = pt(cx, cy, r as f32 - 10.0, a);
    for d in 0..5 {
        hl(fb, px - d, py + d, 2 * d + 1, 0x00FF_C020);
    }
    // The fixed aircraft symbol.
    for d in -1..=1 {
        line_over(fb, cx - 38, cy + d, cx - 12, cy + d, 0x00FF_A020);
        line_over(fb, cx + 12, cy + d, cx + 38, cy + d, 0x00FF_A020);
        vl(fb, cx - 12 + d, cy, 6, 0x00FF_A020);
        vl(fb, cx + 12 + d, cy, 6, 0x00FF_A020);
    }
    disc(fb, cx, cy, 3, 0x00FF_A020);
    ring(fb, cx, cy, r, 0x0010_141A);
}

/// A line clipped to a disc.
fn clipped_line(
    fb: &mut Framebuffer,
    cx: i32,
    cy: i32,
    r: i32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    c: u32,
) {
    let (mut x, mut y) = (x0, y0);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let mut err = dx + dy;
    loop {
        if (x - cx).pow(2) + (y - cy).pow(2) <= r * r {
            fb.px_set(x, y, c);
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// The heading tape: a window on the compass.
fn heading_tape(fb: &mut Framebuffer, r: Rc, hdg: f32) {
    let (x, y, w, h) = r;
    well(fb, r, SCREEN_BG);
    let ppd = w as f32 / 110.0;
    let cx = x + w / 2;
    for d in -60..=60 {
        let deg = (hdg.round() as i32 + d).rem_euclid(360);
        let delta = (deg as f32 - hdg + 540.0).rem_euclid(360.0) - 180.0;
        let px = cx + (delta * ppd).round() as i32;
        if px < x + 2 || px > x + w - 2 {
            continue;
        }
        if deg % 10 == 0 {
            vl(fb, px, y + h - 9, 8, 0x00C8_D4DC);
            let label = match deg {
                0 => "N".to_string(),
                90 => "E".to_string(),
                180 => "S".to_string(),
                270 => "O".to_string(),
                d => format!("{:02}", d / 10),
            };
            txt_c(
                fb,
                px,
                y + 3,
                &label,
                if deg % 90 == 0 {
                    AMBER_LED
                } else {
                    0x00C8_D4DC
                },
            );
        } else if deg % 5 == 0 {
            vl(fb, px, y + h - 6, 5, 0x0068_7684);
        }
    }
    // Caret and digital value.
    for d in 0..4 {
        hl(fb, cx - d, y + h - 1 - d, 2 * d + 1, AMBER_LED);
    }
    let v = format!("{:03.0}", hdg.rem_euclid(360.0));
    fill(fb, cx - 14, y - 1, 28, 11, 0x0004_0608);
    frame(fb, cx - 14, y - 1, 28, 11, AMBER_LED);
    txt_c(fb, cx, y + 1, &v, AMBER_LED);
}

// ---- throttle ----------------------------------------------------------------------------------

/// The thrust lever: a vertical slot with graduations, a lit level strip and a grip.
fn throttle(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    plate(fb, r, "POUSSEE");
    let slot_h = h - 76;
    let sx = x + w / 2 - 6;
    let sy = y + 24;
    well(fb, (sx - 2, sy - 2, 16, slot_h + 4), 0x0002_0507);
    // The lit strip.
    let lit = (tel.thr * f64::from(slot_h)) as i32;
    for i in 0..lit {
        let k = f64::from(i) / f64::from(slot_h);
        let c = if k > 0.85 { WARN } else { GOOD };
        hl(
            fb,
            sx + 3,
            sy + slot_h - 1 - i,
            6,
            mix_color(0x0008_0B0E, c, 0.55),
        );
    }
    // Graduations and numbers every 10 %.
    for k in 0..=10 {
        let yy = sy + slot_h - 1 - (k * (slot_h - 1)) / 10;
        let major = k % 5 == 0;
        hl(
            fb,
            sx - 8 - i32::from(major) * 4,
            yy,
            6 + i32::from(major) * 4,
            0x00C8_D4DC,
        );
        hl(fb, sx + 14, yy, 4 + i32::from(major) * 4, 0x00C8_D4DC);
        if major {
            txt_r(fb, sx - 14, yy - 3, &format!("{}", k * 10), LABEL);
        }
        // Click zone of this notch.
        g.hit(
            (x + 2, yy - slot_h / 20, x + w - 2, yy + slot_h / 20 + 1),
            Act::Call(set_throttle_n, k as usize),
            None,
        );
    }
    // The grip.
    let ky = sy + slot_h - 1 - (tel.thr * f64::from(slot_h - 1)) as i32;
    let kx = sx + 6;
    fill(fb, kx - 20, ky - 10, 40, 20, 0x0026_2D38);
    frame(fb, kx - 20, ky - 10, 40, 20, 0x0090_9CAC);
    for k in 0..4 {
        hl(
            fb,
            kx - 16,
            ky - 6 + k * 4,
            32,
            if k % 2 == 0 { 0x0010_141A } else { 0x0050_5A68 },
        );
    }
    hl(fb, kx - 20, ky - 10, 40, 0x00C0_C8D4);
    fill(
        fb,
        kx - 3,
        ky - 10,
        6,
        20,
        mix_color(0x0026_2D38, AMBER_LED, 0.8),
    );
    // Digital value.
    let v = format!("{:.0}", tel.thr * 100.0);
    well(fb, (x + 8, y + h - 44, w - 16, 20), SCREEN_BG);
    txt_c(fb, x + w / 2 - 4, y + h - 40, &v, GREEN_LED);
    txt(fb, x + w / 2 + 12, y + h - 40, "%", LABEL);
    // Trim (mock: not wired).
    txt_c(fb, x + w / 2, y + h - 18, "TRIM  0", LABEL);
    for k in -3..=3 {
        vl(
            fb,
            x + w / 2 + k * 7,
            y + h - 29,
            if k == 0 { 6 } else { 3 },
            0x0068_7684,
        );
    }
}

// ---- radar -------------------------------------------------------------------------------------

fn log_r(d: f64) -> f32 {
    ((1.0 + d / 100.0).log10() / 8.0).clamp(0.0, 1.0) as f32
}

/// The scope: ship at the centre, nose up, log range, rotating sweep.
fn radar(fb: &mut Framebuffer, g: &Game, tel: &Tel, cx: i32, cy: i32, r: i32) {
    let sim = &g.sim;
    let ship = &sim.ship;
    disc(fb, cx, cy, r + 6, 0x000B_0E12);
    disc(fb, cx, cy, r + 5, 0x003A_4452);
    disc(fb, cx, cy, r + 2, 0x0002_0807);
    ring(fb, cx, cy, r + 5, 0x0078_8496);
    g.hit(
        (cx - r, cy - r, cx + r, cy + r),
        Act::Key(KeyCode::Tab),
        None,
    );
    for (d, l) in [(1.0e3, "1"), (1.0e5, "100"), (1.0e7, "10K"), (1.0e9, "1M")] {
        let rr = (log_r(d) * r as f32).round() as i32;
        ring(fb, cx, cy, rr, 0x0013_3A20);
        if rr > 10 {
            txt(fb, cx + 2, cy - rr - 1, l, 0x001F_6A3A);
        }
    }
    line_over(fb, cx - r, cy, cx + r, cy, 0x0013_3A20);
    line_over(fb, cx, cy - r, cx, cy + r, 0x0013_3A20);
    // Sweep: a wedge that fades behind the line.
    let a = tel.t * 1.6;
    for k in 0..40 {
        let aa = a - k as f32 * 0.025;
        let f = 1.0 - k as f32 / 40.0;
        for rr in (4..r).step_by(2) {
            let (px, py) = pt(cx, cy, rr as f32, aa);
            fb.plot(px, py, scale_color(0x0030_FF70, 0.22 * f * f));
        }
    }
    let (sx, sy) = pt(cx, cy, r as f32, a);
    line_over(fb, cx, cy, sx, sy, 0x0040_C060);
    // Contacts.
    let inv = ship.rot.transpose();
    let plot_contact = |fb: &mut Framebuffer, l: Vec3, c: u32, sel: bool, sq: bool| {
        let flat = (l.x * l.x + l.z * l.z).sqrt();
        let rho = log_r(flat.max(10.0)) * r as f32;
        let ang = l.x.atan2(l.z) as f32;
        let (px, py) = (cx as f32 + rho * ang.sin(), cy as f32 - rho * ang.cos());
        let (px, py) = (px as i32, py as i32);
        let up = ((l.y / flat.max(1.0)).clamp(-1.0, 1.0) as f32 * 9.0) as i32;
        let (y0, y1) = if up >= 0 {
            (py - up, py)
        } else {
            (py, py - up)
        };
        vl(fb, px, y0, y1 - y0 + 1, mix_color(0x0010_1418, c, 0.6));
        if sq {
            fill(fb, px - 2, py - 2, 5, 5, c);
        } else {
            disc(fb, px, py, 2, c);
        }
        if sel {
            let o = if tel.blink { 6 } else { 5 };
            frame(fb, px - o, py - o, 2 * o + 1, 2 * o + 1, 0x00FF_C020);
        }
    };
    for (i, b) in sim.bodies.iter().enumerate() {
        if matches!(b.kind, vector_world::BodyKind::Star(_)) || sim.scan[i] == 0 {
            continue;
        }
        let l = inv.mul_vec(b.pos - ship.pos);
        let col = crate::hud::kind_color(b.kind);
        plot_contact(
            fb,
            l,
            col,
            Some(i) == sim.target,
            matches!(b.kind, vector_world::BodyKind::Station),
        );
    }
    for (k, n) in sim.npcs.iter().enumerate() {
        if !n.visible() {
            continue;
        }
        let l = inv.mul_vec(n.pos - ship.pos);
        if (l.x * l.x + l.z * l.z).sqrt() > 5.0e5 {
            continue;
        }
        plot_contact(
            fb,
            l,
            crate::hud::npc_color(n.role),
            Some(k) == sim.target_npc,
            false,
        );
    }
    // The ship and the field of view.
    for d in 0..3 {
        hl(fb, cx - d, cy - 3 + d, 2 * d + 1, 0x00F0_F6FF);
    }
    txt_c(
        fb,
        cx,
        cy + r + 9,
        if tel.radar_on {
            "RADAR ACTIF  KM"
        } else {
            "PASSIF  KM"
        },
        if tel.radar_on { GREEN_LED } else { LABEL },
    );
}

// ---- screens: navigation, plan, target ---------------------------------------------------------

fn screen_title(fb: &mut Framebuffer, x: i32, y: i32, w: i32, s: &str, c: u32) {
    fill(fb, x, y, w, 11, mix_color(SCREEN_BG, c, 0.22));
    txt_b(fb, x + 4, y + 2, s, c);
}

/// Navigation screen: orbit sketch, periapsis, apoapsis, next event and time before the burn.
fn nav_screen(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    well(fb, r, SCREEN_BG);
    let sim = &g.sim;
    let title = sim.orbit_readout().map_or("NAVIGATION".to_string(), |ro| {
        format!(
            "NAV {}",
            fit(
                &sim.bodies[ro.body].name.to_uppercase(),
                (w as usize - 8) / 8 - 4
            )
        )
    });
    screen_title(fb, x + 1, y + 1, w - 2, &title, GREEN_LED);
    let Some(ro) = sim.orbit_readout() else {
        txt(fb, x + 8, y + 24, "HORS DE TOUTE SPHERE", LABEL);
        txt(fb, x + 8, y + 38, "D'INFLUENCE", LABEL);
        return;
    };
    let e = &ro.elements;
    let alt = |d: f64| {
        let a = (d - ro.radius).max(0.0);
        if a >= 1.0e4 {
            format!("{:.0} KM", a / 1e3)
        } else {
            format!("{a:.0} M")
        }
    };
    let orbit_w = (h - 40).min(w * 38 / 100);
    let ocx = x + 6 + orbit_w / 2 + 4;
    let ocy = y + 14 + (h - 14 - 24) / 2;
    // Orbit sketch.
    let rmax = e.apoapsis.unwrap_or(e.periapsis * 2.0).max(e.periapsis);
    let scale = f64::from(orbit_w) * 0.42 / rmax.max(1.0);
    let planet_r = (ro.radius * scale).clamp(3.0, f64::from(orbit_w) * 0.3) as i32;
    disc(fb, ocx, ocy, planet_r, 0x0020_6A98);
    ring(fb, ocx, ocy, planet_r, 0x0060_B0E0);
    if let Some(ra) = e.apoapsis {
        let a = (e.periapsis + ra) / 2.0;
        let b = a * (1.0 - e.e * e.e).max(0.0).sqrt();
        let cxo = -a * e.e * scale;
        let n = 64;
        let mut prev = None;
        for i in 0..=n {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            let p = (
                ocx as f32 + cxo as f32 + (a * scale) as f32 * t.cos(),
                ocy as f32 + (b * scale) as f32 * t.sin(),
            );
            if let Some((px, py)) = prev {
                line_over(fb, px, py, p.0 as i32, p.1 as i32, 0x0030_C060);
            }
            prev = Some((p.0 as i32, p.1 as i32));
        }
        // Periapsis (left) and apoapsis (right) marks.
        let pp = (ocx + (e.periapsis * scale) as i32, ocy);
        let ap = (ocx - (ra * scale) as i32, ocy);
        txt_c(fb, pp.0, pp.1 + 4, "P", AMBER_LED);
        txt_c(fb, ap.0, ap.1 + 4, "A", AMBER_LED);
        // The ship.
        let b0 = &sim.bodies[ro.body];
        let rr = sim.ship.pos - b0.pos;
        let vr = (sim.ship.vel - b0.vel).dot(rr);
        let cosn = if e.e > 1e-4 {
            ((a * (1.0 - e.e * e.e) / e.r - 1.0) / e.e).clamp(-1.0, 1.0)
        } else {
            1.0
        };
        let nu = cosn.acos() * if vr >= 0.0 { 1.0 } else { -1.0 };
        let (sxp, syp) = (
            ocx as f32 + (e.r * scale) as f32 * -(nu as f32).cos(),
            ocy as f32 - (e.r * scale) as f32 * (nu as f32).sin(),
        );
        disc(
            fb,
            sxp as i32,
            syp as i32,
            3,
            if tel.blink { 0x00FF_FFFF } else { 0x00FF_C020 },
        );
    } else {
        // Open path: a hyperbola sketch is not drawn.
        txt_c(fb, ocx, ocy + planet_r + 8, "TRAJECTOIRE OUVERTE", WARN);
    }
    // Numbers.
    let tx = x + orbit_w + 14;
    let mut line = y + 17;
    let mut put = |fb: &mut Framebuffer, l: &str, v: &str, c: u32| {
        txt(fb, tx, line, l, LABEL);
        txt_r(fb, x + w - 5, line, v, c);
        line += 11;
    };
    put(
        fb,
        "PERI",
        &alt(e.periapsis),
        if e.periapsis < ro.radius {
            RED_LED
        } else {
            GREEN_LED
        },
    );
    put(
        fb,
        "APO",
        &e.apoapsis.map_or("OUVERTE".into(), alt),
        GREEN_LED,
    );
    put(
        fb,
        "PERIODE",
        &e.period.map_or("--".into(), crate::fmt::duration),
        INK,
    );
    put(fb, "VITESSE", &format!("{:.0}", e.speed), INK);
    put(fb, "EXCENTR.", &format!("{:.3}", e.e), INK);
    // Next event / burn.
    let by = y + h - 24;
    hl(fb, x + 2, by - 2, w - 4, 0x0013_3A20);
    let burn = sim
        .nav
        .active
        .as_ref()
        .and_then(|a| a.resolved)
        .map(|rv| (rv.t - sim.time).max(0.0));
    if let Some(eta) = burn {
        txt(fb, x + 5, by, "ALLUMAGE DANS", AMBER_LED);
        txt_r(fb, x + w - 5, by, &crate::fmt::duration(eta), AMBER_LED);
    } else if let Some(ev) =
        crate::nav_view::with_pred(g, |p| p.events.iter().find(|ev| ev.t > sim.time).cloned())
            .flatten()
    {
        let (label, c) = crate::nav_view::event_label(g, &ev);
        let c = if c == crate::hud::RED {
            RED_LED
        } else {
            AMBER_LED
        };
        txt(fb, x + 5, by, &fit(&label, ((w - 60) / 8) as usize), c);
        txt_r(fb, x + w - 5, by, &crate::fmt::duration(ev.t - sim.time), c);
    } else {
        txt(fb, x + 5, by, "AUCUN EVENEMENT PREVU", LABEL);
    }
    txt(
        fb,
        x + 5,
        by + 11,
        &format!("DELTA-V {:.0} M/S", tel.dv),
        LABEL,
    );
}

/// The automatic plan screen: state, current order, next events and the buttons.
fn plan_screen(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    well(fb, r, SCREEN_BG);
    screen_title(fb, x + 1, y + 1, w - 2, "PLAN AUTOMATIQUE", AMBER_LED);
    let sim = &g.sim;
    use vector_sim::nav_auto::Run;
    let (state, c) = match &sim.nav.run {
        Run::Running => ("EN COURS".to_string(), GREEN_LED),
        Run::Paused => ("EN PAUSE".to_string(), AMBER_LED),
        Run::Halted(m) => (format!("ARRETE {}", cut(m, 14)), RED_LED),
        Run::Idle if sim.nav.pending() > 0 => ("PRET".to_string(), LABEL),
        Run::Idle => ("INACTIF".to_string(), LABEL),
    };
    lamp(
        fb,
        x + 10,
        y + 21,
        c,
        matches!(sim.nav.run, Run::Running) && tel.blink_slow
            || matches!(sim.nav.run, Run::Paused | Run::Halted(_)),
    );
    txt_b(fb, x + 20, y + 17, &state, c);
    txt_r(
        fb,
        x + w - 5,
        y + 17,
        &format!("x{}", sim.nav.pending()),
        LABEL,
    );
    let cur = sim.nav.cursor();
    let mut ly = y + 30;
    if let Some(e) = sim.nav.orders.get(cur) {
        let t = sim.order_text(&e.order, e.guard);
        txt(fb, x + 5, ly, ">", AMBER_LED);
        txt(fb, x + 15, ly, &fit(&t, ((w - 20) / 8) as usize), INK);
        ly += 12;
    }
    for e in sim
        .nav
        .orders
        .iter()
        .skip(cur + 1)
        .take(((h - 64) / 11).max(0) as usize)
    {
        let t = sim.order_text(&e.order, e.guard);
        txt(fb, x + 15, ly, &fit(&t, ((w - 20) / 8) as usize), LABEL);
        ly += 11;
    }
    // Buttons (clickable).
    let by = y + h - 17;
    let run_label = if sim.nav_running() { "PAUSE" } else { "LANCER" };
    let btns = [(run_label, 0usize), ("ECRAN", 90), ("TRAJ", 9)];
    let bw = (w - 8) / 3;
    for (k, (l, id)) in btns.iter().enumerate() {
        let bx = x + 4 + k as i32 * bw;
        let hov = g.hit(
            (bx, by, bx + bw - 3, by + 14),
            Act::Call(Game::nav_click_cockpit, *id),
            None,
        );
        let on = k == 0 && sim.nav_running();
        fill(
            fb,
            bx,
            by,
            bw - 3,
            14,
            if on {
                0x00B8_7A10
            } else if hov {
                0x0030_3844
            } else {
                0x0016_1C23
            },
        );
        frame(fb, bx, by, bw - 3, 14, if hov { INK } else { 0x0044_4E5C });
        txt_c(
            fb,
            bx + (bw - 3) / 2,
            by + 3,
            l,
            if on { 0x0004_0608 } else { AMBER_LED },
        );
    }
}

/// Target panel: name, distance, closing speed, radiation and the actions.
fn target_panel(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    well(fb, r, SCREEN_BG);
    screen_title(fb, x + 1, y + 1, w - 2, "CIBLE", AMBER_LED);
    let sim = &g.sim;
    let ship = &sim.ship;
    let v = g.view();
    struct T {
        name: String,
        kind: String,
        dist: f64,
        closing: f64,
        rad: f64,
        hostile: bool,
        hull: Option<(f64, f64)>,
        dock_ok: bool,
        land_ok: bool,
        call_ok: bool,
    }
    let station_near = sim
        .nearest_station()
        .is_some_and(|(_, d)| d < vector_sim::DOCK_RANGE * 4.0);
    let t = if let Some(n) = sim.target_npc.and_then(|k| sim.npcs.get(k)) {
        let rr = n.pos - ship.pos;
        Some(T {
            name: n.name.to_uppercase(),
            kind: n.role.label().to_uppercase(),
            dist: rr.length(),
            closing: (ship.vel - n.vel).dot(rr.normalized()),
            rad: 0.0,
            hostile: n.hostile,
            hull: Some((
                n.hull / n.hull_max.max(1.0),
                n.shield / n.shield_max.max(1.0),
            )),
            dock_ok: false,
            land_ok: false,
            call_ok: true,
        })
    } else {
        sim.target
            .and_then(|i| sim.bodies.get(i).map(|b| (i, b)))
            .map(|(i, b)| {
                let rr = b.pos - ship.pos;
                let range = rr.length() - b.radius;
                T {
                    name: crate::hud::body_label(&v, i).to_uppercase(),
                    kind: crate::hud::kind_label(sim.system.bodies[i].kind),
                    dist: range,
                    closing: (ship.vel - b.vel).dot(rr.normalized()),
                    rad: sim.environment_at(b.pos).radiation,
                    hostile: false,
                    hull: None,
                    dock_ok: matches!(b.kind, vector_world::BodyKind::Station) && station_near,
                    land_ok: sim.landable(i) && range < 3.0e5,
                    call_ok: matches!(b.kind, vector_world::BodyKind::Station),
                }
            })
    };
    let bh = 22;
    let by = y + h - bh - 4;
    let Some(t) = t else {
        txt(fb, x + 8, y + 26, "AUCUNE CIBLE", LABEL);
        pushbtn(
            fb,
            g,
            (x + 4, by, w - 8, bh),
            "TAB : CIBLE SUIVANTE",
            false,
            AMBER_LED,
            Act::Key(KeyCode::Tab),
        );
        return;
    };
    let chars = ((w - 14) / 8) as usize;
    txt_b(
        fb,
        x + 6,
        y + 15,
        &fit(&t.name, chars),
        if t.hostile { RED_LED } else { AMBER_LED },
    );
    txt(fb, x + 6, y + 26, &fit(&t.kind, chars), LABEL);
    let dist = crate::hud::fmt_dist(t.dist);
    txt(fb, x + 6, y + 37, "DISTANCE", LABEL);
    big(fb, x + 6, y + 47, &dist, 2, INK);
    // Closing speed with an arrow: green when the gap shrinks.
    let closing = t.closing > 0.0;
    let ay = y + 65;
    for d in 0..5 {
        if closing {
            hl(fb, x + 10 - d, ay + 1 + d, 2 * d + 1, GREEN_LED);
        } else {
            hl(fb, x + 10 - d, ay + 5 - d, 2 * d + 1, AMBER_LED);
        }
    }
    txt(fb, x + 20, ay + 1, "APPROCHE", LABEL);
    txt_r(
        fb,
        x + w - 6,
        ay + 1,
        &format!("{:+.0} M/S", t.closing),
        if closing { GREEN_LED } else { AMBER_LED },
    );
    let ry = ay + 12;
    txt(fb, x + 6, ry, "RADIATION", LABEL);
    txt_r(
        fb,
        x + w - 6,
        ry,
        &format!("{:.2} GY/H", t.rad),
        if t.rad > 0.5 { RED_LED } else { INK },
    );
    if let Some((hu, sh)) = t.hull {
        txt(fb, x + 6, ry + 11, "COQUE", LABEL);
        seg(fb, x + 54, ry + 12, 64, hu);
        txt(fb, x + 126, ry + 11, "BCL", LABEL);
        seg(fb, x + 154, ry + 12, 40, sh);
    } else {
        txt(fb, x + 6, ry + 11, "STATION", LABEL);
        txt_r(
            fb,
            x + w - 6,
            ry + 11,
            if station_near { "A PORTEE" } else { "LOIN" },
            INK,
        );
    }
    let bw = ((w - 8 - 2) / 3).max(40);
    let acts = [
        ("AMARRER", t.dock_ok, GREEN_LED, Act::Key(KeyCode::KeyD)),
        ("ATTERRIR", t.land_ok, GREEN_LED, Act::Key(KeyCode::Numpad4)),
        ("APPELER", t.call_ok, AMBER_LED, Act::Key(KeyCode::Enter)),
    ];
    for (k, (l, ok, c, act)) in acts.iter().enumerate() {
        pushbtn(
            fb,
            g,
            (x + 4 + k as i32 * (bw + 1), by, bw, bh),
            l,
            *ok,
            *c,
            *act,
        );
    }
    let _ = tel;
}

fn seg(fb: &mut Framebuffer, x: i32, y: i32, w: i32, f: f64) {
    let n = w / 4;
    let lit = (f.clamp(0.0, 1.0) * f64::from(n)).round() as i32;
    for i in 0..n {
        let c = tint(f.max(0.02));
        fill(
            fb,
            x + i * 4,
            y,
            3,
            6,
            if i < lit {
                c
            } else {
                mix_color(0x0008_0B0E, c, 0.14)
            },
        );
    }
}

/// The descent screen (replaces the navigation screen over a world): radar altitude, time to
/// impact, where to light the engines, slope and site.
fn descent_screen(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    well(fb, r, SCREEN_BG);
    let Some(ro) = &tel.landing else {
        return;
    };
    let sim = &g.sim;
    screen_title(
        fb,
        x + 1,
        y + 1,
        w - 2,
        &format!(
            "DESCENTE  {}",
            fit(&sim.bodies[ro.body].name.to_uppercase(), 10)
        ),
        AMBER_LED,
    );
    let alarm = ro.burn_now && tel.blink;
    txt(fb, x + 6, y + 16, "ALT RADAR", LABEL);
    big(
        fb,
        x + 6,
        y + 27,
        &crate::hud::fmt_dist(ro.alt),
        2,
        if alarm { RED_LED } else { GREEN_LED },
    );
    let impact = ro
        .t_impact
        .map_or("--".to_string(), |t| format!("{t:.0} S"));
    txt(fb, x + w / 2 + 6, y + 16, "IMPACT", LABEL);
    big(
        fb,
        x + w / 2 + 6,
        y + 27,
        &impact,
        2,
        if alarm { RED_LED } else { INK },
    );
    let lines: Vec<(&str, String, u32)> = vec![
        (
            "V.VERT",
            format!("{:+.1} M/S", ro.v_up),
            if ro.v_up < -15.0 { RED_LED } else { INK },
        ),
        ("V.HORIZ", format!("{:.1} M/S", ro.v_h), INK),
        (
            "ALLUMAGE",
            match ro.burn_alt {
                Some(a) => format!("A {a:.0} M"),
                None => "POUSSEE < POIDS".to_string(),
            },
            if alarm { RED_LED } else { AMBER_LED },
        ),
        (
            "TWR",
            format!("{:.1}", ro.twr),
            if ro.twr < 1.2 { RED_LED } else { INK },
        ),
        (
            "PENTE / NEZ",
            format!(
                "{:.0} / {:.0} DEG",
                ro.slope.to_degrees(),
                ro.tilt.to_degrees()
            ),
            INK,
        ),
        (
            "SITE",
            ro.site
                .map_or("NON MARQUE".to_string(), |(d, _)| format!("A {d:.0} M")),
            if ro.site.is_some() { GREEN_LED } else { LABEL },
        ),
    ];
    for (k, (l, v, c)) in lines.iter().enumerate() {
        let yy = y + 48 + k as i32 * 11;
        if yy + 10 > y + h - 18 {
            break;
        }
        txt(fb, x + 6, yy, l, LABEL);
        txt_r(fb, x + w - 6, yy, v, *c);
    }
    let (label, c) = if ro.v_up >= 0.0 {
        ("EN VOL", LABEL)
    } else if ro.safe {
        ("CONTACT SUR", GREEN_LED)
    } else {
        ("CONTACT DANGEREUX", AMBER_LED)
    };
    let py = y + h - 15;
    fill(fb, x + 4, py, w - 8, 12, mix_color(0x0008_0B0E, c, 0.28));
    txt_c(fb, x + w / 2, py + 2, label, c);
    if alarm {
        txt_c(
            fb,
            x + w / 2,
            y + 40,
            ">>> ALLUMER LES MOTEURS <<<",
            RED_LED,
        );
    }
}

/// The pilot panel: the landing aids as lit push buttons.
fn pilot_screen(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    well(fb, r, SCREEN_BG);
    screen_title(fb, x + 1, y + 1, w - 2, "PILOTE ET AIDES", AMBER_LED);
    let (state, c) = match tel.pilot {
        Some(PilotKind::Land) => ("ATTERRISSAGE", GREEN_LED),
        Some(PilotKind::Ascend) => ("DECOLLAGE", GREEN_LED),
        Some(PilotKind::Dock) => ("AMARRAGE", GREEN_LED),
        None => ("MANUEL", LABEL),
    };
    lamp(
        fb,
        x + 10,
        y + 21,
        c,
        tel.pilot.is_some() && tel.blink_slow || tel.pilot.is_some(),
    );
    txt_b(fb, x + 20, y + 17, state, c);
    let sim = &g.sim;
    let items: [(&str, bool, u32, KeyCode); 6] = [
        ("PILOTE", tel.pilot.is_some(), GREEN_LED, KeyCode::Numpad4),
        ("SITE PLAT", sim.site.is_some(), AMBER_LED, KeyCode::Numpad7),
        ("VUE SOL", g.rear_view, AMBER_LED, KeyCode::Numpad1),
        ("ACCEL +", false, GREEN_LED, KeyCode::Numpad8),
        ("ACCEL -", false, GREEN_LED, KeyCode::Numpad2),
        ("COUPURE", tel.thr < 0.01, RED_LED, KeyCode::Numpad0),
    ];
    let bw = (w - 12) / 2;
    let bh = ((h - 40) / 3 - 3).clamp(14, 22);
    for (k, (l, on, c, code)) in items.iter().enumerate() {
        let bx = x + 4 + (k as i32 % 2) * (bw + 4);
        let by = y + 30 + (k as i32 / 2) * (bh + 3);
        pushbtn(fb, g, (bx, by, bw, bh), l, *on, *c, Act::Key(*code));
    }
}

// ---- alert lamps and F keys --------------------------------------------------------------------

fn lamp_panel(fb: &mut Framebuffer, tel: &Tel, r: Rc, cols: i32) {
    let (x, y, w, h) = r;
    let rows = (8 + cols - 1) / cols;
    let (cw, ch) = ((w - 4) / cols, ((h - 2) / rows).min(34));
    for (k, (label, level)) in tel.lamps.iter().enumerate() {
        let (c, r_) = (k as i32 % cols, k as i32 / cols);
        let (lx, ly) = (x + 2 + c * cw, y + 1 + r_ * ch);
        let on = match level {
            2 => tel.blink,
            1 => true,
            _ => false,
        };
        let col = if *level == 2 { RED_LED } else { AMBER_LED };
        fill(fb, lx, ly, cw - 3, ch - 3, 0x0008_0B0E);
        let face = if on {
            mix_color(0x0010_1418, col, 0.85)
        } else {
            mix_color(0x0010_1418, col, 0.09)
        };
        fill(fb, lx + 2, ly + 2, cw - 7, ch - 7, face);
        if on {
            halo(fb, lx + cw / 2, ly + ch / 2, (cw / 2).min(34), col, 0.3);
        }
        frame(fb, lx, ly, cw - 3, ch - 3, 0x0044_4E5C);
        let tc = if on {
            0x0004_0608
        } else {
            mix_color(0x0050_5A66, col, 0.25)
        };
        let label = fit(label, ((cw - 10) / 8) as usize);
        txt_c(fb, lx + (cw - 3) / 2, ly + (ch - 3) / 2 - 3, &label, tc);
    }
}

/// The console keys: each opens a screen of the game; `cols` per row.
fn fkeys_grid(fb: &mut Framebuffer, g: &Game, r: Rc, items: &[(&str, &str, KeyCode)], cols: usize) {
    let (x, y, w, h) = r;
    let n = items.len() as i32;
    let cols = cols as i32;
    let rows = (n + cols - 1) / cols;
    let bw = (w - 4 - (cols - 1) * 3) / cols;
    let bh = (h - (rows - 1) * 3) / rows;
    for (k, (label, tag, code)) in items.iter().enumerate() {
        let (c, r_) = (k as i32 % cols, k as i32 / cols);
        let bx = x + 2 + c * (bw + 3);
        let by = y + r_ * (bh + 3);
        let hov = g.hit((bx, by, bx + bw, by + bh), Act::Key(*code), None);
        fill(fb, bx, by, bw, bh, 0x0008_0B0E);
        fill(
            fb,
            bx + 2,
            by + 2,
            bw - 4,
            bh - 4,
            if hov { 0x0038_4252 } else { 0x0022_2A33 },
        );
        hl(fb, bx + 2, by + 2, bw - 4, 0x0044_4E5C);
        hl(fb, bx + 2, by + bh - 3, bw - 4, METAL_LO);
        frame(fb, bx, by, bw, bh, if hov { INK } else { 0x0044_4E5C });
        txt_b(
            fb,
            bx + 8,
            by + bh / 2 - 3,
            label,
            if hov { INK } else { AMBER_LED },
        );
        txt_r(fb, bx + bw - 6, by + bh / 2 - 3, tag, LABEL);
    }
}

fn fkeys(fb: &mut Framebuffer, g: &Game, r: Rc, items: &[(&str, &str, KeyCode)]) {
    fkeys_grid(fb, g, r, items, items.len());
}

const FKEYS: [(&str, &str, KeyCode); 10] = [
    ("CARTE", "M", KeyCode::KeyM),
    ("GALAXIE", "L", KeyCode::KeyL),
    ("SOUTE", "I", KeyCode::KeyI),
    ("EQUIPAGE", "K", KeyCode::KeyK),
    ("JOURNAL", "J", KeyCode::KeyJ),
    ("CODEX", "F6", KeyCode::F6),
    ("VAISSEAU", "H", KeyCode::KeyH),
    ("NAV AUTO", "*", KeyCode::NumpadMultiply),
    ("TECH", "F4", KeyCode::F4),
    ("MENU", "F10", KeyCode::F10),
];

// ---- control blocks ----------------------------------------------------------------------------

/// Six toggle switches, `per_row` in a row.
fn toggles(fb: &mut Framebuffer, g: &Game, tel: &Tel, x: i32, y: i32, per_row: i32) {
    let shield = SHIELD_ON.load(Ordering::Relaxed) && tel.shield.is_some();
    let items: [(&str, bool, u32, Act); 6] = [
        ("TRAIN", tel.gear, GREEN_LED, Act::Key(KeyCode::Numpad3)),
        ("BOUCL", shield, 0x0050_A8FF, Act::Call(mock_switch, 0)),
        ("RADAR", tel.radar_on, GREEN_LED, Act::Key(KeyCode::Period)),
        ("LEURR", tel.decoys > 0, AMBER_LED, Act::Key(KeyCode::Comma)),
        (
            "LUMIE",
            LIGHTS_ON.load(Ordering::Relaxed),
            0x00F0_F0A0,
            Act::Call(mock_switch, 1),
        ),
        (
            "PILOT",
            tel.auto || tel.pilot.is_some(),
            AMBER_LED,
            Act::Key(KeyCode::KeyG),
        ),
    ];
    for (k, (l, on, c, act)) in items.iter().enumerate() {
        let (col, row) = (k as i32 % per_row, k as i32 / per_row);
        toggle(fb, g, x + col * 46, y + row * 70, l, *on, *c, *act);
    }
}

/// Eight SAS push buttons in two columns of four.
fn sas_block(fb: &mut Framebuffer, g: &Game, tel: &Tel, x: i32, y: i32, w: i32) {
    let items: [(&str, Sas, KeyCode); 8] = [
        ("COUPE", Sas::Off, KeyCode::Digit0),
        ("MAINTIEN", Sas::Hold, KeyCode::Digit1),
        ("PROGRADE", Sas::Prograde, KeyCode::Digit2),
        ("RETROGRADE", Sas::Retrograde, KeyCode::Digit3),
        ("NORMAL", Sas::Normal, KeyCode::Digit4),
        ("RADIAL", Sas::RadialOut, KeyCode::Digit6),
        ("CIBLE", Sas::Target, KeyCode::Digit8),
        ("VERTICALE", Sas::Up, KeyCode::Digit9),
    ];
    let bw = (w - 4) / 2;
    for (k, (l, m, code)) in items.iter().enumerate() {
        let bx = x + (k as i32 / 4) * (bw + 4);
        let by = y + (k as i32 % 4) * 22;
        let lit = tel.sas == *m
            || (*m == Sas::RadialOut && tel.sas == Sas::RadialIn)
            || (*m == Sas::Normal && tel.sas == Sas::AntiNormal);
        let c = if *m == Sas::Off { RED_LED } else { GREEN_LED };
        pushbtn(fb, g, (bx, by, bw, 20), l, lit, c, Act::Key(*code));
    }
}

/// The eight LED columns of the systems, each with its label and value running up beside it.
fn systems_v(fb: &mut Framebuffer, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    plate(fb, r, "SYSTEMES");
    let items: [(&str, f64, bool, String, bool); 8] = [
        (
            "CARBURANT",
            tel.fuel,
            false,
            format!("{:.0}%", tel.fuel * 100.0),
            tel.fuel < 0.25,
        ),
        (
            "DELTA-V",
            tel.dv_frac,
            false,
            format!("{:.1}K", tel.dv / 1e3),
            tel.dv_frac < 0.2,
        ),
        (
            "BATTERIE",
            tel.batt,
            false,
            format!("{:.0}%", tel.batt * 100.0),
            tel.batt < 0.25,
        ),
        (
            "COQUE",
            tel.hull,
            false,
            format!("{:.0}%", tel.hull * 100.0),
            tel.hull < 0.5,
        ),
        (
            "BOUCLIER",
            tel.shield.unwrap_or(0.0),
            false,
            tel.shield
                .map_or("--".into(), |s| format!("{:.0}%", s * 100.0)),
            tel.shield.is_some_and(|s| s < 0.25),
        ),
        (
            "TEMPERAT.",
            tel.temp,
            true,
            format!("{:.0}K", tel.temp_k),
            tel.temp_k > 500.0,
        ),
        (
            "RADIATION",
            tel.dose_frac,
            true,
            format!("{:.2}", tel.dose_mgy),
            tel.dose_frac > 0.5,
        ),
        (
            "CHARGE G",
            tel.g_frac,
            true,
            format!("{:.1}G", tel.gload),
            tel.g_frac > 0.5,
        ),
    ];
    let pitch = (w - 14) / 8;
    let (top, bh) = (y + 22, h - 30);
    for (k, (l, f, hb, v, alarm)) in items.iter().enumerate() {
        let bx = x + 9 + k as i32 * pitch;
        led_bar_v(fb, bx, top, 9, bh, *f, *hb, *alarm, tel.blink);
        let base = top + bh - 1;
        let lc = if *alarm { RED_LED } else { LABEL };
        txt_v(fb, bx + 11, base, l, lc);
        let vc = if *alarm && tel.blink { RED_LED } else { INK };
        txt_v(
            fb,
            bx + 11,
            base - (l.chars().count() as i32 + 1) * 8,
            v,
            vc,
        );
    }
}

/// The same gauges, horizontal, for the narrow side panels.
fn systems_h(fb: &mut Framebuffer, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    plate(fb, r, "SYSTEMES");
    let items: [(&str, f64, bool, String, bool); 8] = [
        (
            "CARBURANT",
            tel.fuel,
            false,
            format!("{:.0}%", tel.fuel * 100.0),
            tel.fuel < 0.25,
        ),
        (
            "DELTA-V",
            tel.dv_frac,
            false,
            format!("{:.1}K", tel.dv / 1e3),
            tel.dv_frac < 0.2,
        ),
        (
            "BATTERIE",
            tel.batt,
            false,
            format!("{:.0}%", tel.batt * 100.0),
            tel.batt < 0.25,
        ),
        (
            "COQUE",
            tel.hull,
            false,
            format!("{:.0}%", tel.hull * 100.0),
            tel.hull < 0.5,
        ),
        (
            "BOUCLIER",
            tel.shield.unwrap_or(0.0),
            false,
            tel.shield
                .map_or("--".into(), |s| format!("{:.0}%", s * 100.0)),
            tel.shield.is_some_and(|s| s < 0.25),
        ),
        (
            "TEMP",
            tel.temp,
            true,
            format!("{:.0}K", tel.temp_k),
            tel.temp_k > 500.0,
        ),
        (
            "RADIATION",
            tel.dose_frac,
            true,
            format!("{:.2}", tel.dose_mgy),
            tel.dose_frac > 0.5,
        ),
        (
            "CHARGE G",
            tel.g_frac,
            true,
            format!("{:.1}", tel.gload),
            tel.g_frac > 0.5,
        ),
    ];
    let step = ((h - 26) / items.len() as i32).max(12);
    for (k, (l, f, hb, v, alarm)) in items.iter().enumerate() {
        led_bar_h(
            fb,
            x + 8,
            y + 20 + k as i32 * step,
            w - 16,
            72,
            *f,
            *hb,
            l,
            v,
            *alarm,
            tel.blink,
        );
    }
}

/// The flight instruments: heading tape, three dials around the attitude ball, annunciators and
/// a digital strip.
fn flight_cluster(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc, with_tape: bool) {
    let (x, y, w, h) = r;
    plate(fb, r, "");
    let mut top = y + 8;
    if with_tape {
        heading_tape(fb, (x + 14, top, w - 28, 24), tel.heading);
        top += 32;
    }
    let bottom = y + h - 62;
    let avail = bottom - top;
    let rb = ((avail - 12) / 2).min((w - 56) / 5).min(80);
    let cy = top + avail / 2 - 2;
    let cx = x + w / 2;
    attitude(fb, tel, cx, cy, rb);
    let rd = (rb * 3 / 4).max(30);
    let dx = rb + rd + 18;
    speed_dial(fb, g, tel, cx - dx, cy, rd);
    vario_dial(fb, tel, cx + dx, cy, rd);
    // Annunciators.
    let ay = y + h - 54;
    let mut ax = x + 10;
    let sas_l = format!("SAS {}", cut(tel.sas.label(), 12));
    for (l, on, c) in [
        ("ASSIST", tel.assist, GREEN_LED),
        (sas_l.as_str(), tel.sas != Sas::Off, GREEN_LED),
        ("TRAIN", tel.gear, AMBER_LED),
    ] {
        let wpx = tw(l) + 12;
        fill(
            fb,
            ax,
            ay,
            wpx,
            13,
            if on {
                mix_color(0x0008_0B0E, c, 0.55)
            } else {
                0x0010_1418
            },
        );
        frame(fb, ax, ay, wpx, 13, if on { c } else { 0x0030_3844 });
        txt(fb, ax + 6, ay + 3, l, if on { INK } else { LABEL });
        ax += wpx + 5;
    }
    // Digital strip: radar altitude, vertical and horizontal speed.
    let sy = y + h - 28;
    let cells = [
        (
            "ALT RADAR",
            tel.alt.map(crate::hud::fmt_dist),
            tel.alt.is_some_and(|a| a < 100.0),
        ),
        (
            "V.VERT",
            tel.vv.map(|v| format!("{v:+.1} M/S")),
            tel.vv.is_some_and(|v| v < -15.0),
        ),
        ("V.HORIZ", tel.vh.map(|v| format!("{v:.1} M/S")), false),
    ];
    let cw = (w - 20) / 3;
    for (k, (l, v, alarm)) in cells.iter().enumerate() {
        let bx = x + 10 + k as i32 * cw;
        well(fb, (bx, sy, cw - 4, 22), SCREEN_BG);
        txt(fb, bx + 4, sy + 2, l, LABEL);
        let s = v.clone().unwrap_or_else(|| "--".to_string());
        txt_r(
            fb,
            bx + cw - 8,
            sy + 11,
            &s,
            if *alarm && tel.blink {
                RED_LED
            } else {
                GREEN_LED
            },
        );
    }
}

// ---- the window --------------------------------------------------------------------------------

/// The 3D scene in a sub-framebuffer, copied into the window; the zones the scene declares
/// (bodies and ships to click) are moved to where the window is.
fn scene(fb: &mut Framebuffer, g: &Game, x: i32, y: i32, w: i32, h: i32) {
    let mut sub = Framebuffer::new(w as usize, h as usize);
    let before = g.pointer.hits.borrow().len();
    let v = g.view();
    crate::hud::draw_cockpit_scene(&mut sub, &v);
    crate::landing_view::draw_markers(&mut sub, &v);
    for hit in g.pointer.hits.borrow_mut().iter_mut().skip(before) {
        hit.rect = (
            hit.rect.0 + x,
            hit.rect.1 + y,
            hit.rect.2 + x,
            hit.rect.3 + y,
        );
    }
    for row in 0..h {
        let src = &sub.px[(row * w) as usize..((row + 1) * w) as usize];
        for (col, p) in src.iter().enumerate() {
            fb.px_set(x + col as i32, y + row, *p);
        }
    }
}

/// A filled triangle.
fn tri(fb: &mut Framebuffer, a: (i32, i32), b: (i32, i32), c: (i32, i32), col: u32) {
    let (miny, maxy) = (a.1.min(b.1).min(c.1), a.1.max(b.1).max(c.1));
    for y in miny..=maxy {
        let mut xs = Vec::new();
        for (p, q) in [(a, b), (b, c), (c, a)] {
            if (p.1 <= y && q.1 > y) || (q.1 <= y && p.1 > y) {
                let t = (y - p.1) as f32 / (q.1 - p.1) as f32;
                xs.push(p.0 as f32 + t * (q.0 - p.0) as f32);
            }
        }
        if xs.len() >= 2 {
            let (lo, hi) = (xs[0].min(xs[1]), xs[0].max(xs[1]));
            hl(fb, lo.round() as i32, y, (hi - lo).round() as i32 + 1, col);
        }
    }
}

/// The canopy frame around a window of size (w, h) at (x, y): struts, top rail, glare shield.
fn canopy(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32) {
    let frame_c = 0x0012_161C;
    let hi = 0x0038_4250;
    // Top rail.
    fill(fb, x, y, w, 12, frame_c);
    hl(fb, x, y + 12, w, hi);
    // Side struts.
    let sw = w / 9;
    tri(fb, (x, y), (x + sw, y), (x, y + h), frame_c);
    tri(fb, (x + w - sw, y), (x + w, y), (x + w, y + h), frame_c);
    line_over(fb, x + sw, y + 12, x, y + h, hi);
    line_over(fb, x + w - sw, y + 12, x + w, y + h, hi);
    // Glare shield: a hump along the bottom.
    for col in 0..w {
        let t = std::f32::consts::PI * col as f32 / w as f32;
        let hh = 8 + (16.0 * t.sin().powf(0.6)) as i32;
        vl(fb, x + col, y + h - hh, hh, frame_c);
        fb.px_set(x + col, y + h - hh, hi);
        fb.px_set(x + col, y + h - hh - 1, 0x0006_080B);
    }
    for k in 0..6 {
        screw(fb, x + 24 + k * ((w - 48) / 5), y + 6);
    }
}

/// Darken a region of the scene (a backing for the glass texts over a bright sky).
fn shade(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, k: f32) {
    for yy in y.max(0)..(y + h).min(fb.h as i32) {
        for xx in x.max(0)..(x + w).min(fb.w as i32) {
            let i = yy as usize * fb.w + xx as usize;
            fb.px[i] = scale_color(fb.px[i], k);
        }
    }
}

/// A short glass text line (additive, like the HUD of the game).
fn glass(fb: &mut Framebuffer, x: i32, y: i32, s: &str, c: u32) {
    fb.text_at(x, y, s, c);
}

fn glass_texts(fb: &mut Framebuffer, g: &Game, x: i32, y: i32, w: i32, margin: i32) {
    let sim = &g.sim;
    let green = crate::hud::GREEN;
    shade(fb, x + margin - 6, y + 17, 330, 26, 0.45);
    shade(fb, x + w - margin - 200, y + 17, 206, 14, 0.45);
    glass(
        fb,
        x + margin,
        y + 20,
        &format!(
            "{} ({})  CLASSE {}",
            sim.system.name,
            sim.system.designation,
            sim.system.star_class.letter()
        ),
        green,
    );
    let frame_name = sim
        .ref_body
        .map_or("ETOILE".to_string(), |i| sim.bodies[i].name.clone());
    glass(
        fb,
        x + margin,
        y + 31,
        &format!("REF {frame_name}"),
        crate::hud::DIM,
    );
    let ts = format!(
        "TEMPS x{}{}  T+{}",
        g.timescale,
        if g.paused { " PAUSE" } else { "" },
        crate::fmt::duration(sim.time)
    );
    glass(fb, x + w - margin - tw(&ts), y + 20, &ts, green);
}

/// The last messages, low in the window.
fn glass_messages(fb: &mut Framebuffer, g: &Game, cx: i32, y: i32) {
    let widest = g
        .messages
        .iter()
        .rev()
        .take(2)
        .map(|m| tw(m))
        .max()
        .unwrap_or(0);
    shade(fb, cx - widest / 2 - 8, y - 14, widest + 16, 36, 0.5);
    for (i, m) in g.messages.iter().rev().take(2).enumerate() {
        let m = m.to_uppercase();
        glass(
            fb,
            cx - tw(&m) / 2,
            y - i as i32 * 11,
            &m,
            if i == 0 {
                crate::hud::AMBER
            } else {
                crate::hud::DIM
            },
        );
    }
}

// ---- layouts -----------------------------------------------------------------------------------

pub fn draw(fb: &mut Framebuffer, g: &Game, layout: Layout) {
    let tel = Tel::new(g);
    match layout {
        Layout::A => layout_a(fb, g, &tel),
        Layout::B => layout_b(fb, g, &tel),
        Layout::C => layout_c(fb, g, &tel, false),
        Layout::COpen => layout_c(fb, g, &tel, true),
    }
}

fn console_bg(fb: &mut Framebuffer, y: i32) {
    let (w, h) = (fb.w as i32, fb.h as i32);
    vgrad(fb, 0, y, w, h - y, 0x001B_2129, 0x000F_1318);
    hl(fb, 0, y, w, 0x0050_5C6C);
    hl(fb, 0, y + 1, w, 0x0006_080B);
}

fn nav_slot(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    if tel.landing.is_some() {
        descent_screen(fb, g, tel, r);
    } else {
        nav_screen(fb, g, tel, r);
    }
}

fn plan_slot(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    if tel.landing.is_some() {
        pilot_screen(fb, g, tel, r);
    } else {
        plan_screen(fb, g, tel, r);
    }
}

/// A: wide low console, like an airliner's panel.
fn layout_a(fb: &mut Framebuffer, g: &Game, tel: &Tel) {
    let wh = 380;
    scene(fb, g, 0, 0, 1280, wh);
    glass_texts(fb, g, 0, 0, 1280, 150);
    glass_messages(fb, g, 640, wh - 44);
    canopy(fb, 0, 0, 1280, wh);
    console_bg(fb, wh);
    let y0 = wh + 8;
    let h = 296;
    systems_v(fb, tel, (6, y0, 176, h));
    // Controls: toggles, SAS, lamps.
    let (cx, cw) = (188, 286);
    plate(fb, (cx, y0, cw, h), "COMMANDES");
    toggles(fb, g, tel, cx + 5, y0 + 16, 6);
    txt(fb, cx + 8, y0 + 88, "SAS - MAINTIEN D'ATTITUDE", LABEL);
    sas_block(fb, g, tel, cx + 6, y0 + 100, cw - 12);
    txt(fb, cx + 8, y0 + 192, "ALERTES", LABEL);
    lamp_panel(fb, tel, (cx + 6, y0 + 204, cw - 12, 86), 2);
    // Flight cluster.
    flight_cluster(fb, g, tel, (480, y0, 352, h), true);
    // Radar and navigation.
    plate(fb, (838, y0, 150, 146), "");
    radar(fb, g, tel, 838 + 75, y0 + 70, 50);
    txt_c(fb, 838 + 75, y0 + 3, "SCANNER", LABEL);
    plate(fb, (994, y0, 208, 146), "");
    nav_slot(fb, g, tel, (1000, y0 + 6, 196, 134));
    // Target and plan.
    let ty = y0 + 152;
    plate(fb, (838, ty, 190, 144), "");
    target_panel(fb, g, tel, (844, ty + 6, 178, 132));
    plate(fb, (1034, ty, 168, 144), "");
    plan_slot(fb, g, tel, (1040, ty + 6, 156, 132));
    // Throttle.
    throttle(fb, g, tel, (1208, y0, 66, h));
    // Console keys.
    fkeys(fb, g, (6, y0 + h + 6, 1268, 24), &FKEYS);
}

fn nav_screen_plate(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    plate(fb, r, "");
    nav_slot(fb, g, tel, (r.0 + 6, r.1 + 6, r.2 - 12, r.3 - 12));
}

/// B: side panels and a compact low console around a smaller window.
fn layout_b(fb: &mut Framebuffer, g: &Game, tel: &Tel) {
    // The dash is metal everywhere; the window is a porthole in the middle.
    vgrad(fb, 0, 0, 1280, 720, 0x001B_2129, 0x000F_1318);
    let (wx, wy, ww, wh) = (250, 8, 780, 458);
    fill(fb, wx - 6, wy - 6, ww + 12, wh + 12, 0x0008_0B0E);
    frame(fb, wx - 6, wy - 6, ww + 12, wh + 12, 0x0056_6272);
    scene(fb, g, wx, wy, ww, wh);
    glass_texts(fb, g, wx, wy - 6, ww, 14);
    glass_messages(fb, g, wx + ww / 2, wy + wh - 24);
    for (sx, sy) in [
        (wx - 3, wy - 3),
        (wx + ww + 2, wy - 3),
        (wx - 3, wy + wh + 2),
        (wx + ww + 2, wy + wh + 2),
    ] {
        screw(fb, sx, sy);
    }
    // Left column.
    systems_h(fb, tel, (6, 6, 238, 166));
    plate(fb, (6, 178, 238, 156), "");
    toggles(fb, g, tel, 6 + 50, 178 + 8, 3);
    plate(fb, (6, 340, 238, 106), "SAS");
    sas_block(fb, g, tel, 12, 354, 226);
    nav_screen_plate(fb, g, tel, (6, 452, 238, 262));
    // Right column.
    plate(fb, (1036, 6, 238, 236), "");
    radar(fb, g, tel, 1036 + 119, 6 + 116, 94);
    plate(fb, (1036, 248, 238, 164), "");
    plan_slot(fb, g, tel, (1042, 254, 226, 152));
    plate(fb, (1036, 418, 238, 142), "");
    target_panel(fb, g, tel, (1042, 424, 226, 130));
    plate(fb, (1036, 566, 238, 148), "ALERTES SYSTEME");
    lamp_panel(fb, tel, (1042, 584, 226, 124), 2);
    // Compact bottom console.
    plate(fb, (250, 472, 696, 242), "");
    heading_tape(fb, (262, 480, 672, 22), tel.heading);
    let cy = 586;
    attitude(fb, tel, 598, cy, 68);
    speed_dial(fb, g, tel, 440, cy + 2, 52);
    vario_dial(fb, tel, 756, cy + 2, 52);
    let cells = [
        (
            "ALT RADAR",
            tel.alt.map(crate::hud::fmt_dist),
            tel.alt.is_some_and(|a| a < 100.0),
        ),
        (
            "V.VERT",
            tel.vv.map(|v| format!("{v:+.1} M/S")),
            tel.vv.is_some_and(|v| v < -15.0),
        ),
        ("V.HORIZ", tel.vh.map(|v| format!("{v:.1} M/S")), false),
    ];
    for (k, (l, v, alarm)) in cells.iter().enumerate() {
        let bx = 262 + k as i32 * 225;
        well(fb, (bx, 664, 220, 22), SCREEN_BG);
        txt(fb, bx + 6, 667, l, LABEL);
        txt_r(
            fb,
            bx + 214,
            676,
            &v.clone().unwrap_or_else(|| "--".into()),
            if *alarm && tel.blink {
                RED_LED
            } else {
                GREEN_LED
            },
        );
    }
    let keys = [
        ("CARTE", "M", KeyCode::KeyM),
        ("GALAXIE", "L", KeyCode::KeyL),
        ("SOUTE", "I", KeyCode::KeyI),
        ("EQUIP.", "K", KeyCode::KeyK),
        ("JOURNAL", "J", KeyCode::KeyJ),
        ("CODEX", "F6", KeyCode::F6),
        ("VAISS.", "H", KeyCode::KeyH),
        ("MENU", "F10", KeyCode::F10),
    ];
    fkeys_grid(fb, g, (262, 690, 672, 20), &keys, 8);
    throttle(fb, g, tel, (952, 472, 78, 242));
}

/// The glass HUD of layout C: tapes and ladder drawn as additive vectors over the scene.
fn glass_hud(fb: &mut Framebuffer, g: &Game, tel: &Tel, bottom: i32) {
    let c = crate::hud::GREEN;
    let dim = crate::hud::DIM;
    let (w, cx) = (1280.0f32, 640.0f32);
    let cy = bottom as f32 / 2.0;
    // Backings under the numbers so that they read over a bright ground.
    shade(fb, 380, 22, 520, 52, 0.55);
    shade(fb, 50, 84, 200, 44, 0.55);
    shade(fb, 1000, 84, 230, 16, 0.55);
    shade(fb, (cx - 330.0) as i32, (cy - 96.0) as i32, 110, 192, 0.6);
    if tel.alt.is_some() {
        shade(fb, (cx + 220.0) as i32, (cy - 96.0) as i32, 120, 192, 0.6);
    }
    // Heading tape.
    let ppd = 6.0f32;
    for d in -40..=40 {
        let deg = (tel.heading.round() as i32 + d).rem_euclid(360);
        let x = cx + ((deg as f32 - tel.heading + 540.0).rem_euclid(360.0) - 180.0) * ppd;
        if (x - cx).abs() > 240.0 {
            continue;
        }
        if deg % 10 == 0 {
            fb.line(x, 40.0, x, 50.0, c);
            let l = match deg {
                0 => "N".to_string(),
                90 => "E".to_string(),
                180 => "S".to_string(),
                270 => "O".to_string(),
                d => format!("{:02}", d / 10),
            };
            fb.text_at(
                x as i32 - tw(&l) / 2,
                28,
                &l,
                if deg % 90 == 0 { crate::hud::AMBER } else { c },
            );
        } else if deg % 5 == 0 {
            fb.line(x, 40.0, x, 46.0, dim);
        }
    }
    fb.line(cx, 52.0, cx - 4.0, 58.0, c);
    fb.line(cx, 52.0, cx + 4.0, 58.0, c);
    fb.text_at(cx as i32 - 12, 60, &format!("{:03.0}", tel.heading), c);
    // Speed tape (left) and altitude / vertical speed (right).
    let tape = |fb: &mut Framebuffer, x: f32, v: f64, step: f64, unit: &str, left: bool| {
        fb.line(x, cy - 90.0, x, cy + 90.0, dim);
        for k in -9..=9 {
            let val = (v / step).round() * step + f64::from(k) * step;
            let y = cy - ((val - v) / step * 18.0) as f32;
            if (y - cy).abs() > 90.0 {
                continue;
            }
            let len = if k % 2 == 0 { 8.0 } else { 4.0 };
            let (x0, x1) = if left { (x - len, x) } else { (x, x + len) };
            fb.line(x0, y, x1, y, c);
            if k % 2 == 0 && val >= 0.0 {
                let s = short(val);
                let tx = if left {
                    x as i32 - 14 - tw(&s)
                } else {
                    x as i32 + 14
                };
                fb.text_at(tx, y as i32 - 3, &s, c);
            }
        }
        let bx = if left { x as i32 - 78 } else { x as i32 + 14 };
        fb.outline(
            bx as f32,
            cy - 8.0,
            (bx + 64) as f32,
            cy + 8.0,
            crate::hud::HOVER,
        );
        let s = short(v);
        fb.text_at(bx + 32 - tw(&s) / 2, cy as i32 - 3, &s, crate::hud::HOVER);
        fb.text_at(bx + 32 - tw(unit) / 2, cy as i32 + 12, unit, dim);
    };
    let spd = match tel.speed_mode() {
        0 => tel.spd_surf.unwrap_or(tel.spd_orb),
        2 => tel.spd_tgt.unwrap_or(tel.spd_orb),
        _ => tel.spd_orb,
    };
    tape(fb, cx - 230.0, spd, nice_max(spd) / 20.0, "M/S", true);
    if let Some(a) = tel.alt {
        tape(fb, cx + 230.0, a, 20.0, "ALT M", false);
    }
    // Pitch ladder.
    let k = bottom as f32 * 0.8;
    let n = tel.ux.hypot(tel.uy).max(1e-4);
    let (nx, ny) = (tel.ux / n, tel.uy / n);
    for deg in [-30i32, -20, -10, 0, 10, 20, 30] {
        let s = (deg as f32).to_radians().sin();
        let off = k * (s - tel.sinp);
        let (mx, my) = (cx + nx * off, cy - ny * off);
        let half = if deg == 0 { 120.0 } else { 40.0 };
        let (dx, dy) = (-ny * half, -nx * half);
        let (x0, y0, x1, y1) = (mx - dx, my + dy, mx + dx, my - dy);
        if (mx - cx).abs() < 260.0 && (my - cy).abs() < 150.0 {
            if deg == 0 {
                fb.line(x0, y0, x1, y1, c);
            } else {
                fb.line(x0, y0, x0 + (x1 - x0) * 0.35, y0 + (y1 - y0) * 0.35, c);
                fb.line(x1, y1, x1 + (x0 - x1) * 0.35, y1 + (y0 - y1) * 0.35, c);
                fb.text_at(x1 as i32 + 4, y1 as i32 - 3, &format!("{deg:+}"), c);
            }
        }
    }
    // Reticle.
    fb.circle(cx, cy, 5.0, c);
    fb.line(cx - 16.0, cy, cx - 7.0, cy, c);
    fb.line(cx + 7.0, cy, cx + 16.0, cy, c);
    fb.line(cx, cy - 12.0, cx, cy - 7.0, c);
    // Corner readouts.
    let thr = format!("ACC {:.0}%", tel.thr * 100.0);
    fb.text_at(60, 88, &thr, c);
    fb.text_at(60, 99, &format!("SAS {}", tel.sas.label()), dim);
    fb.text_at(
        60,
        110,
        &format!("TRAIN {}", if tel.gear { "SORTI" } else { "RENTRE" }),
        if tel.gear { crate::hud::AMBER } else { dim },
    );
    let sim = &g.sim;
    let tgt = sim
        .target
        .map(|i| format!("CIBLE {}", sim.bodies[i].name.to_uppercase()));
    if let Some(t) = tgt {
        fb.text_at(1280 - 60 - tw(&t), 88, &t, crate::hud::AMBER);
    }
    let _ = w;
}

/// C: minimal glass HUD over a full-screen scene; the console folds down to a strip.
fn layout_c(fb: &mut Framebuffer, g: &Game, tel: &Tel, open: bool) {
    let strip = 34;
    let open_h = 258;
    let top = if open { 720 - open_h - 8 } else { 720 - strip };
    scene(fb, g, 0, 0, 1280, 720);
    glass_hud(fb, g, tel, top);
    glass_messages(fb, g, 640, if open { top - 56 } else { top - 30 });
    if !open {
        // The folded strip: key readouts, the lamps, the console keys and the handle.
        console_bg(fb, 720 - strip);
        let y = 720 - strip + 4;
        let items: [(&str, f64, bool, String, bool); 4] = [
            (
                "CARB",
                tel.fuel,
                false,
                format!("{:.0}%", tel.fuel * 100.0),
                tel.fuel < 0.25,
            ),
            (
                "BATT",
                tel.batt,
                false,
                format!("{:.0}%", tel.batt * 100.0),
                tel.batt < 0.25,
            ),
            (
                "COQ",
                tel.hull,
                false,
                format!("{:.0}%", tel.hull * 100.0),
                tel.hull < 0.5,
            ),
            (
                "TEMP",
                tel.temp,
                true,
                format!("{:.0}K", tel.temp_k),
                tel.temp_k > 500.0,
            ),
        ];
        for (k, (l, f, hb, v, a)) in items.iter().enumerate() {
            led_bar_h(
                fb,
                12 + k as i32 * 176,
                y + 7,
                164,
                36,
                *f,
                *hb,
                l,
                v,
                *a,
                tel.blink,
            );
        }
        // Folded lamps.
        for (k, (_, lv)) in tel.lamps.iter().enumerate() {
            let on = match lv {
                2 => tel.blink,
                1 => true,
                _ => false,
            };
            lamp(
                fb,
                730 + k as i32 * 18,
                y + 13,
                if *lv == 2 { RED_LED } else { AMBER_LED },
                on,
            );
        }
        fkeys(
            fb,
            g,
            (886, y, 386, 24),
            &[FKEYS[0], FKEYS[1], FKEYS[2], FKEYS[9]],
        );
        // The handle.
        let hx = 640;
        let hov = g.hit(
            (hx - 56, 720 - strip - 14, hx + 56, 720 - strip + 2),
            Act::Call(toggle_console, 0),
            None,
        );
        fill(fb, hx - 56, 720 - strip - 14, 112, 16, 0x0026_2D38);
        frame(
            fb,
            hx - 56,
            720 - strip - 14,
            112,
            16,
            if hov { INK } else { 0x0078_8496 },
        );
        txt_c(
            fb,
            hx,
            720 - strip - 10,
            "CONSOLE ^",
            if hov { INK } else { AMBER_LED },
        );
        return;
    }
    // The console, deployed: a compact version of A.
    console_bg(fb, top - 36);
    let y0 = top;
    fkeys(
        fb,
        g,
        (704, top - 32, 568, 22),
        &[FKEYS[0], FKEYS[1], FKEYS[2], FKEYS[3], FKEYS[4], FKEYS[9]],
    );
    txt(fb, 14, top - 27, "CONSOLE DEPLOYEE", LABEL);
    let hov = g.hit(
        (584, top - 30, 696, top - 14),
        Act::Call(toggle_console, 0),
        None,
    );
    fill(fb, 584, top - 30, 112, 16, 0x0026_2D38);
    frame(
        fb,
        584,
        top - 30,
        112,
        16,
        if hov { INK } else { 0x0078_8496 },
    );
    txt_c(
        fb,
        640,
        top - 26,
        "CONSOLE v",
        if hov { INK } else { AMBER_LED },
    );
    let h = open_h;
    systems_h(fb, tel, (6, y0, 226, h));
    plate(fb, (238, y0, 290, h), "COMMANDES");
    toggles(fb, g, tel, 238 + 7, y0 + 16, 6);
    sas_block(fb, g, tel, 244, y0 + 88, 278);
    lamp_panel(fb, tel, (244, y0 + 182, 278, 68), 4);
    plate(fb, (534, y0, 240, h), "");
    attitude(fb, tel, 654, y0 + 78, 62);
    speed_dial(fb, g, tel, 590, y0 + 190, 34);
    vario_dial(fb, tel, 718, y0 + 190, 34);
    plate(fb, (780, y0, 170, h), "");
    radar(fb, g, tel, 865, y0 + 62, 52);
    plan_slot(fb, g, tel, (786, y0 + 132, 158, 120));
    plate(fb, (956, y0, 244, h), "");
    target_panel(fb, g, tel, (962, y0 + 6, 232, 128));
    nav_slot(fb, g, tel, (962, y0 + 138, 232, 114));
    throttle(fb, g, tel, (1206, y0, 68, h));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vector_data::GameData;

    fn game() -> Game {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let mut g = Game::new(Arc::new(GameData::load(&dir).expect("data")), 1);
        g.key(KeyCode::Enter, None, true);
        g
    }

    /// Draw a layout and click the centre of the first zone that `want` accepts.
    fn click(g: &mut Game, layout: Layout, want: impl Fn(&crate::mouse::Hit) -> bool) -> bool {
        let mut fb = Framebuffer::new(1280, 720);
        g.begin_frame();
        draw(&mut fb, g, layout);
        let found = g
            .pointer
            .hits
            .borrow()
            .iter()
            .rev()
            .find(|h| want(h))
            .map(|h| ((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2));
        match found {
            Some((x, y)) => {
                g.pointer_click(x, y, 0);
                true
            }
            None => false,
        }
    }

    #[test]
    fn every_layout_draws_and_the_console_answers_the_mouse() {
        for layout in [Layout::A, Layout::B, Layout::C, Layout::COpen] {
            let mut g = game();
            setup_free(&mut g);
            // The SAS push buttons are keys of the game.
            assert!(
                click(&mut g, layout, |h| matches!(
                    h.act,
                    Act::Key(KeyCode::Digit3)
                )) == (layout != Layout::C),
                "SAS button"
            );
            if layout != Layout::C {
                assert_eq!(g.sim.sas, Sas::Retrograde);
            }
            // The landing gear toggle.
            if layout != Layout::C {
                let gear = g.sim.gear_down;
                assert!(click(&mut g, layout, |h| matches!(
                    h.act,
                    Act::Key(KeyCode::Numpad3)
                )));
                assert_ne!(g.sim.gear_down, gear);
                // A notch of the thrust lever (the zones of the lever are on the right).
                assert!(click(&mut g, layout, |h| matches!(h.act, Act::Call(_, 3))
                    && h.rect.0 > 950));
                assert!((g.sim.throttle - 0.3).abs() < 1e-9, "{}", g.sim.throttle);
                // A console key opens a screen.
                assert!(click(&mut g, layout, |h| matches!(
                    h.act,
                    Act::Key(KeyCode::KeyI)
                )));
                assert_eq!(g.screen, crate::game::Screen::Inventory);
            }
        }
    }

    #[test]
    fn the_scene_zones_stay_on_screen() {
        let mut g = game();
        g.sim.reveal_all();
        let tel = Tel::new(&g);
        assert!(tel.landing.is_none(), "nothing to land on at the start");
        let mut fb = Framebuffer::new(1280, 720);
        g.begin_frame();
        draw(&mut fb, &g, Layout::A);
        // The scene zones are moved to where the window is: none outside the screen.
        for h in g.pointer.hits.borrow().iter() {
            assert!(h.rect.2 > 0 && h.rect.3 > 0, "zone outside the frame");
        }
    }
}
