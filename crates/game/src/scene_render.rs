//! The scene of the world, drawn ONCE for both views. The cockpit and the third-person view
//! differ only by the camera (and by the player's own ship, hidden from the cockpit); every
//! solid object of the sky (stations, ships, rocks, wrecks) is drawn here, the same way: the
//! same model, the same star light, the same depth buffer, the same distances of display.
//! The HUD of the cockpit (markers, labels, reticle) is an overlay drawn on top, never another
//! shape of the object.
//!
//! The models are built once per kind (station, rock) or per (design, state) (ships) and kept in
//! a per-thread cache; nothing is built or allocated per frame in the steady state.

use crate::ext_view::{self, Ground};
use crate::game::{Game, Screen};
use crate::hud::View;
use crate::ship_model::{self as sm, Light, Model, Pose, State, Zbuf};
use std::cell::RefCell;
use std::collections::HashMap;
use vector_core::{Mat3, Vec3};
use vector_render::{Camera, Framebuffer, NEAR};
use vector_sim::npc::{Npc, NpcRole, NpcState};
use vector_sim::Sim;
use vector_world::BodyKind;

/// Ships farther than this are only the markers of the HUD (m).
pub const SEE_NPC: f64 = 40_000.0;
/// Rocks (asteroids) are solid models from this apparent radius (px); under it, a lit ball.
pub const ROCK_PX: f64 = 6.0;
/// Rock shapes per colour.
const ROCK_SHAPES: u64 = 4;
/// Radius (m) and colour of a wreck as a solid piece of hull.
pub const WRECK_R: f64 = 7.0;
pub const WRECK_COLOR: u32 = 0x0090_7048;

#[derive(Default)]
struct Cache {
    zb: Option<Zbuf>,
    frame: u64,
    stations: HashMap<u64, (Model, Model)>,
    rocks: HashMap<(u32, u64), Model>,
    ships: HashMap<u64, (Model, u64)>,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

/// The depth buffer of the frame, emptied only when something solid is first drawn.
struct Pass<'a> {
    zb: &'a mut Zbuf,
    ready: bool,
    lit: Light,
}

impl Pass<'_> {
    /// Draw `m` at `pose` unless it is surely out of the picture.
    fn draw(&mut self, fb: &mut Framebuffer, cam: &Camera, m: &Model, pose: &Pose) {
        let r = f64::from(m.radius) * pose.scale + 1.0;
        let v = cam.to_view(pose.pos);
        if v.z + r < NEAR {
            return;
        }
        if v.z > r {
            let focal = fb.h as f64 * 0.5 / (cam.fov_y * 0.5).tan();
            let rp = r / v.z * focal;
            let (hx, hy) = (fb.w as f64 * 0.5, fb.h as f64 * 0.5);
            if (v.x / v.z * focal).abs() - rp > hx || (v.y / v.z * focal).abs() - rp > hy {
                return;
            }
        }
        if !self.ready {
            self.zb.reset(fb.w, fb.h);
            self.ready = true;
        }
        sm::draw(fb, self.zb, cam, m, pose, &self.lit);
    }
}

fn mix(h: u64, x: u64) -> u64 {
    (h ^ x).wrapping_mul(0x0000_0100_0000_01B3).rotate_left(23)
}

fn q(x: f64, k: f64) -> u64 {
    (x * k).round() as i64 as u64
}

fn role_name(r: NpcRole) -> &'static str {
    match r {
        NpcRole::Trader => "trader",
        NpcRole::Miner => "miner",
        NpcRole::Pirate => "pirate",
        NpcRole::Patrol => "patrol",
    }
}

/// What the shape of another ship depends on, quantised: the key of its model.
fn npc_key(n: &Npc, time_bucket: u64) -> u64 {
    let moving = n.vel.length() > 1.0 && !n.disabled;
    let firing = n.hostile && matches!(n.state, NpcState::Combat { .. }) && !n.disabled;
    let docked = matches!(n.state, NpcState::Docked { .. });
    let mut h = mix(0xA5, u64::from(n.id));
    h = mix(h, n.role as u64);
    h = mix(
        h,
        u64::from(moving) | u64::from(firing) << 1 | u64::from(docked) << 2,
    );
    h = mix(h, q((n.hull / n.hull_max.max(1.0)).clamp(0.0, 1.0), 6.0));
    h = mix(
        h,
        q((n.shield / n.shield_max.max(1.0)).clamp(0.0, 1.0), 6.0),
    );
    mix(h, time_bucket)
}

fn npc_model(sim: &Sim, n: &Npc, time: f64) -> Model {
    let design = sm::npc_design(&sim.data, role_name(n.role), n.id);
    let mut st = State::idle(&design);
    let moving = n.vel.length() > 1.0 && !n.disabled;
    st.thrust = if moving { 0.4 } else { 0.0 };
    st.hull = (n.hull / n.hull_max.max(1.0)).clamp(0.0, 1.0);
    st.shield = if n.shield_max > 0.0 {
        (n.shield / n.shield_max).clamp(0.0, 1.0)
    } else {
        0.0
    };
    st.firing = n.hostile && matches!(n.state, NpcState::Combat { .. }) && !n.disabled;
    st.deploy = if matches!(n.state, NpcState::Docked { .. }) {
        0.0
    } else {
        1.0
    };
    st.time = time + f64::from(n.id);
    sm::build(&sim.data, &design, &st)
}

/// The key of the player's model: its design, the health of its parts and its state.
fn player_key(sim: &Sim, st: &State) -> u64 {
    let d = &sim.ship.design;
    let mut h = mix(0x91, d.hull as u64);
    for s in &d.slots {
        h = mix(h, s.map_or(u64::MAX, |m| m as u64));
    }
    for (i, hp) in st.health.iter().enumerate() {
        h = mix(
            h,
            q(*hp, 8.0) ^ u64::from(st.powered.get(i).copied().unwrap_or(true)) << 40,
        );
    }
    for (x, k) in [
        (st.temp_k, 0.05),
        (st.thrust, 8.0),
        (st.shield, 8.0),
        (st.hit_age.min(5.0), 6.0),
        (st.hull, 8.0),
        (st.gear, 12.0),
        (st.deploy, 12.0),
        (st.entry, 8.0),
        (st.rcs, 8.0),
        (st.time, 8.0),
    ] {
        h = mix(h, q(x, k));
    }
    mix(h, u64::from(st.firing) | u64::from(st.laser) << 1)
}

/// The camera of the scene: the one thing that differs between the two views. From the cockpit,
/// the eye of the ship; outside, the camera that orbits it.
pub fn camera(v: &View, ext: bool, gr: Option<&Ground>) -> Camera {
    if ext {
        ext_view::camera(v.game, v.sim, gr)
    } else {
        Camera {
            pos: v.sim.ship.pos,
            rot: v.game.view_rot(&v.sim.ship.rot),
            fov_y: if v.boost { 1.4 } else { 1.2 },
        }
    }
}

/// Milliseconds per frame of the object layer alone over `n` frames (measures).
pub fn bench_objects(g: &Game, fb: &mut Framebuffer, ext: bool, n: usize) -> f64 {
    let v = g.view();
    let gr = if ext {
        ext_view::ground(g, &g.sim)
    } else {
        None
    };
    let cam = camera(&v, ext, gr.as_ref());
    draw_objects(fb, &cam, &v, gr.as_ref(), ext);
    let t = std::time::Instant::now();
    for _ in 0..n {
        draw_objects(fb, &cam, &v, gr.as_ref(), ext);
    }
    t.elapsed().as_secs_f64() * 1000.0 / n as f64
}

/// Draw the solid objects of the world as seen from `cam`: stations, rocks, ships, wrecks and,
/// from the third-person view only, the player's own ship with its shadow and its dust.
pub fn draw_objects(
    fb: &mut Framebuffer,
    cam: &Camera,
    v: &View,
    gr: Option<&Ground>,
    show_player: bool,
) {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let Cache {
            zb,
            frame,
            stations,
            rocks,
            ships,
        } = &mut *c;
        *frame += 1;
        let frame = *frame;
        let zb = zb.get_or_insert_with(|| Zbuf::new(0, 0));
        let sim = v.sim;
        let mut pass = Pass {
            zb,
            ready: false,
            lit: ext_view::light(sim),
        };
        let focal = fb.h as f64 * 0.5 / (cam.fov_y * 0.5).tan();
        for (i, b) in sim.bodies.iter().enumerate() {
            if sim.scan.get(i).copied().unwrap_or(0) == 0 {
                continue;
            }
            let d = (b.pos - cam.pos).length().max(1.0);
            let px = b.radius / d * focal;
            match b.kind {
                BodyKind::Station if px >= 2.0 => {
                    let (hub, ring) = stations
                        .entry(b.radius.to_bits())
                        .or_insert_with(|| sm::station_parts(b.radius));
                    let rot = Mat3::rot_y(0.4);
                    let fixed = Pose {
                        pos: b.pos,
                        rot,
                        scale: 1.0,
                    };
                    pass.draw(fb, cam, hub, &fixed);
                    let spun = Pose {
                        pos: b.pos,
                        rot: rot.mul(&Mat3::rot_z(sim.time * 0.05)),
                        scale: 1.0,
                    };
                    pass.draw(fb, cam, ring, &spun);
                }
                BodyKind::Asteroid(_) if px > ROCK_PX => {
                    let col = crate::hud::body_color(v, i);
                    let m = rocks
                        .entry((col, i as u64 % ROCK_SHAPES))
                        .or_insert_with(|| sm::rock_model(i as u64 % ROCK_SHAPES + 17, col));
                    let spin = Mat3::rot_y(sim.time * 2.0e-5 + i as f64);
                    let pose = Pose {
                        pos: b.pos,
                        rot: spin,
                        scale: b.radius,
                    };
                    pass.draw(fb, cam, m, &pose);
                }
                _ => {}
            }
        }
        for w in &sim.wrecks {
            let d = (w.pos - cam.pos).length().max(1.0);
            if WRECK_R / d * focal < 1.5 {
                continue;
            }
            let m = rocks
                .entry((WRECK_COLOR, 99))
                .or_insert_with(|| sm::rock_model(99, WRECK_COLOR));
            let a = f64::from(w.id) + sim.time * 0.3;
            let pose = Pose {
                pos: w.pos,
                rot: Mat3::rot_y(a).mul(&Mat3::rot_x(a * 0.7)),
                scale: WRECK_R,
            };
            pass.draw(fb, cam, m, &pose);
        }
        for n in &sim.npcs {
            if !n.visible() {
                continue;
            }
            let d = (n.pos - cam.pos).length();
            if d > SEE_NPC || 30.0 / d.max(1.0) * focal < 1.5 {
                continue;
            }
            // Flames flicker only where they can be seen.
            let near = 30.0 / d.max(1.0) * focal > 40.0;
            let bucket = if near {
                (sim.time * 6.0).floor() as u64
            } else {
                0
            };
            let key = npc_key(n, bucket);
            let e = ships
                .entry(key)
                .or_insert_with(|| (npc_model(sim, n, bucket as f64 / 6.0), frame));
            e.1 = frame;
            let m = &e.0;
            if f64::from(m.radius) / d.max(1.0) * focal < 1.0 {
                continue;
            }
            let fwd = if n.vel.length() > 1.0 {
                n.vel
            } else {
                sim.ship.pos - n.pos
            };
            let pose = Pose {
                pos: n.pos,
                rot: Mat3::look_at(fwd, Vec3::new(0.0, 1.0, 0.0)),
                scale: 1.0,
            };
            pass.draw(fb, cam, m, &pose);
        }
        if show_player {
            let st = State::of_player(sim, ext_view::gear_for(sim, gr));
            let key = player_key(sim, &st);
            let e = ships.entry(key).or_insert_with(|| {
                let mut st = st.clone();
                st.time = (st.time * 8.0).round() / 8.0;
                (sm::build(&sim.data, &sim.ship.design, &st), frame)
            });
            e.1 = frame;
            let m = &e.0;
            let mut pose = Pose {
                pos: sim.ship.pos,
                rot: sim.ship.rot,
                scale: 1.0,
            };
            if let Some(gr) = gr {
                let keel = f64::from(-m.foot_y);
                if st.gear > 0.95 && gr.h < keel {
                    pose.pos = pose.pos + gr.up * (keel - gr.h);
                }
                ext_view::shadow(fb, cam, m, &pose, &pass.lit, gr);
                ext_view::dust(fb, cam, sim, &st, gr, f64::from(m.flame_len));
            }
            pass.draw(fb, cam, m, &pose);
        }
        if ships.len() > 160 {
            ships.retain(|_, e| frame - e.1 < 240);
        }
    });
}

/// Set up one of the situations of the measures and of the tests: the ship in flight, undocked.
pub fn setup(g: &mut Game, kind: &str) {
    g.sim.undock();
    g.screen = Screen::Flight;
    g.map = false;
    g.sim.reveal_all();
    let look =
        |from: Vec3, to: Vec3| Mat3::look_at((to - from).normalized(), Vec3::new(0.0, 1.0, 0.0));
    let first =
        |g: &Game, f: &dyn Fn(&BodyKind) -> bool| g.sim.bodies.iter().position(|b| f(&b.kind));
    match kind {
        "empty" => {
            g.sim.npcs.clear();
            g.sim.ship.pos = Vec3::new(3.0e12, 2.0e11, 1.0e12);
            g.sim.ship.rot = Mat3::look_at(Vec3::new(1.0, 0.2, 0.3), Vec3::new(0.0, 1.0, 0.0));
        }
        "station" => {
            if let Some(i) = first(g, &|k| *k == BodyKind::Station) {
                let b = g.sim.bodies[i].clone();
                g.sim.ship.pos = b.pos + Vec3::new(0.0, b.radius * 0.7, -b.radius * 3.0);
                g.sim.ship.vel = b.vel;
                g.sim.ship.rot = look(g.sim.ship.pos, b.pos);
                g.sim.target = Some(i);
            }
        }
        "ships20" | "convoy" => {
            let n = if kind == "convoy" { 8 } else { 20 };
            g.sim.npcs.clear();
            let p = g.sim.ship.pos;
            g.sim.ship.rot = Mat3::look_at(Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 1.0, 0.0));
            for k in 0..n {
                let roles = [
                    NpcRole::Trader,
                    NpcRole::Miner,
                    NpcRole::Pirate,
                    NpcRole::Patrol,
                ];
                let a = k as f64;
                g.sim.npcs.push(Npc {
                    id: 100 + k as u32,
                    role: roles[k % 4],
                    name: format!("NPC {k}"),
                    faction: None,
                    pos: p + Vec3::new(
                        (a % 5.0 - 2.0) * 90.0,
                        (a / 5.0).floor() * 50.0 - 50.0,
                        300.0 + a * 60.0,
                    ),
                    vel: g.sim.ship.vel + Vec3::new(0.0, 0.0, 20.0),
                    state: NpcState::Orbit {
                        around: 0,
                        phase: 0.0,
                    },
                    cargo: None,
                    last_scan: 0.0,
                    home: 0,
                    hull: 100.0,
                    hull_max: 100.0,
                    shield: 50.0,
                    shield_max: 100.0,
                    level: 1,
                    hostile: k % 4 == 2,
                    disabled: false,
                    provoked: false,
                    hail_deadline: None,
                });
            }
        }
        "belt" => {
            // Beside the biggest rock, with the others around it.
            if let Some(i) = first(g, &|k| matches!(k, BodyKind::Asteroid(_))) {
                let b = g.sim.bodies[i].clone();
                g.sim.ship.pos = b.pos + Vec3::new(0.0, 0.0, -b.radius * 6.0);
                g.sim.ship.vel = b.vel;
                g.sim.ship.rot = look(g.sim.ship.pos, b.pos);
            }
        }
        "planet" => {
            if let Some(i) = first(g, &|k| matches!(k, BodyKind::Planet(_))) {
                let b = g.sim.bodies[i].clone();
                g.sim.ship.pos = b.pos + Vec3::new(0.0, 0.0, -b.radius * 3.0);
                g.sim.ship.vel = b.vel;
                g.sim.ship.rot = look(g.sim.ship.pos, b.pos);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::DEFAULT_SEED;
    use std::sync::Arc;

    fn game() -> Game {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let data = Arc::new(vector_data::GameData::load(&dir).expect("data"));
        let mut g = Game::new(data, DEFAULT_SEED);
        g.new_game(DEFAULT_SEED);
        g
    }

    const KINDS: [&str; 6] = ["empty", "station", "ships20", "convoy", "belt", "planet"];

    fn cam_of(g: &Game) -> Camera {
        Camera {
            pos: g.sim.ship.pos,
            rot: g.view_rot(&g.sim.ship.rot),
            fov_y: 1.2,
        }
    }

    /// The third-person view adds only the player's ship: with the ship out of the picture,
    /// both views paint the same pixels, scene after scene (and again from the model cache).
    #[test]
    fn both_views_paint_the_same_pixels_at_the_same_camera() {
        let mut g = game();
        for kind in KINDS {
            setup(&mut g, kind);
            // The camera is 80 m ahead of the ship: the ship is behind it.
            let mut cam = cam_of(&g);
            cam.pos = cam.pos + g.sim.ship.rot.col(2) * 80.0;
            let mut a = Framebuffer::new(320, 180);
            let mut b = Framebuffer::new(320, 180);
            let v = g.view();
            for _ in 0..2 {
                a.px.fill(0);
                b.px.fill(0);
                draw_objects(&mut a, &cam, &v, None, false);
                draw_objects(&mut b, &cam, &v, None, true);
                assert!(a.px == b.px, "{kind}: the two views differ");
            }
        }
    }

    #[test]
    fn solid_objects_are_drawn_when_close() {
        let mut g = game();
        for kind in ["station", "ships20", "belt"] {
            setup(&mut g, kind);
            let cam = cam_of(&g);
            let mut fb = Framebuffer::new(320, 180);
            draw_objects(&mut fb, &cam, &g.view(), None, false);
            assert!(fb.px.iter().any(|p| *p != 0), "{kind}: nothing drawn");
        }
    }

    #[test]
    fn odd_cameras_do_not_panic() {
        let mut g = game();
        for kind in KINDS {
            setup(&mut g, kind);
            let mut fb = Framebuffer::new(160, 90);
            let base = cam_of(&g);
            let spots = [
                base.pos,
                base.pos + Vec3::new(1.0e15, 0.0, 0.0),
                base.pos + Vec3::new(0.0, 1.0e-3, 0.0),
                Vec3::ZERO,
            ];
            for pos in spots {
                for rot in [base.rot, Mat3::rot_y(3.1), Mat3::rot_x(1.5)] {
                    let cam = Camera {
                        pos,
                        rot,
                        fov_y: 1.0,
                    };
                    draw_objects(&mut fb, &cam, &g.view(), None, true);
                }
            }
        }
    }

    /// No other path draws the objects of the scene: the views hold no model, no wireframe of
    /// a scene object, and no fork on the third-person flag around an object.
    #[test]
    fn only_the_shared_layer_draws_the_objects() {
        let hud = include_str!("hud.rs");
        let cockpit = &hud[hud.find("fn draw_cockpit").unwrap()..hud.find("fn draw_map").unwrap()];
        for banned in [
            "Wireframe",
            "station_model",
            "sm::draw",
            "ship_model",
            "draw_ships",
        ] {
            assert!(!cockpit.contains(banned), "draw_cockpit uses {banned}");
        }
        assert!(cockpit.contains("scene_render::draw_objects"));
        assert!(!cockpit.contains("BodyKind::Station if ext"));
        assert!(!cockpit.contains("if ext && px"));
        let ext = include_str!("ext_view.rs");
        for banned in ["sm::draw(", "station_model", "npc_design", "draw_ships"] {
            assert!(!ext.contains(banned), "ext_view uses {banned}");
        }
    }
}
