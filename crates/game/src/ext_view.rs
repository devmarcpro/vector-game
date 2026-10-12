//! Third-person view: a camera that orbits the player's ship and the solid models of the ships
//! and stations around it, drawn with `ship_model` over the same scene as the cockpit (stars,
//! worlds, terrain are `hud`'s). The ship gets its shadow and dust on the ground when it is low,
//! and its landing gear when it is slow and close to the ground.
use crate::game::{Game, Screen};
use crate::hud::{View, AMBER, DIM, GREEN};
use crate::mouse::Act;
use crate::ship_model::{self as sm, Light, Model, Pose, State, Zbuf};
use vector_core::{hash_coord, Mat3, Vec3};
use vector_render::{Camera, Framebuffer, NEAR};
use vector_sim::npc::{NpcRole, NpcState};
use vector_sim::Sim;
use winit::keyboard::KeyCode;

/// Orbit distance bounds, in lengths of the ship.
const ZOOM_MIN: f64 = 0.7;
const ZOOM_MAX: f64 = 60.0;
pub const ZOOM_START: f64 = 1.9;
/// Default azimuth: three quarters from behind, so the ship reads as a shape.
pub const ZOOM_YAW: f64 = 0.7;
/// Models farther than this are only the markers of the cockpit (m).
const SEE_NPC: f64 = 40_000.0;

/// The ground under the ship, when there is one close enough to matter.
pub struct Ground {
    /// Local vertical (away from the world).
    pub up: Vec3,
    /// Height of the keel above the ground (m).
    pub h: f64,
}

/// Length of the hull of the design (m).
pub fn ship_length(sim: &Sim) -> f64 {
    sm::hull_length(&sim.data, sim.ship.design.hull)
}

/// The ground under the ship: the terrain of the nearest solid world when low over it.
pub fn ground(g: &Game, sim: &Sim) -> Option<Ground> {
    let (i, b) = sim
        .bodies
        .iter()
        .enumerate()
        .filter(|(_, b)| {
            matches!(
                b.kind,
                vector_world::BodyKind::Planet(_) | vector_world::BodyKind::Moon(_)
            )
        })
        .min_by(|a, b| {
            (a.1.pos - sim.ship.pos)
                .length()
                .total_cmp(&(b.1.pos - sim.ship.pos).length())
        })?;
    let rel = sim.ship.pos - b.pos;
    let alt = rel.length() - b.radius;
    if !(0.0..=(b.radius * 0.03).min(200_000.0)).contains(&alt) {
        return None;
    }
    let up = rel.normalized();
    let height = match g.flight_map(i) {
        Some(map) => {
            let orient = body_orient(sim, i, f64::from(map.surface.tilt));
            let nb = orient.transpose().mul_vec(up);
            let (lat, lon) = (nb.y.clamp(-1.0, 1.0).asin(), nb.z.atan2(nb.x));
            let t = crate::terrain::Terrain::new(&map, b.radius);
            f64::from(t.sample(lat, lon, 4.0).0.max(0.0))
        }
        None => 0.0,
    };
    Some(Ground {
        up,
        h: (alt - height).max(0.0),
    })
}

/// Orientation of world `i` (the one hud uses for its terrain).
pub fn body_orient(sim: &Sim, i: usize, tilt: f64) -> Mat3 {
    let angle = std::f64::consts::TAU * sim.time / sim.system.spin_period(i).max(60.0);
    Mat3::rot_z(tilt).mul(&Mat3::rot_y(angle))
}

/// Landing gear (0 stowed .. 1 down): out when low and slow over a ground.
pub fn gear_for(sim: &Sim, gr: Option<&Ground>) -> f64 {
    let Some(gr) = gr else {
        return 0.0;
    };
    let speed = sim.rel_velocity().length();
    if speed > 250.0 {
        return 0.0;
    }
    ((600.0 - gr.h) / 450.0).clamp(0.0, 1.0)
}

/// The orbiting camera: behind the ship by default, turning around it with the mouse.
pub fn camera(g: &Game, sim: &Sim, gr: Option<&Ground>) -> Camera {
    let ship = &sim.ship;
    let dist = ship_length(sim) * g.ext_zoom;
    let (y, p) = (g.ext_yaw, g.ext_pitch);
    let local = Vec3::new(p.cos() * y.sin(), p.sin(), -p.cos() * y.cos());
    let up_hint = gr.map_or_else(
        || {
            if g.ext_chase {
                ship.rot.col(1)
            } else {
                Vec3::new(0.0, 1.0, 0.0)
            }
        },
        |gr| {
            if g.ext_chase {
                ship.rot.col(1)
            } else {
                gr.up
            }
        },
    );
    let mut off = if g.ext_chase {
        ship.rot.mul_vec(local)
    } else {
        local
    };
    // Never under the ground: keep the camera above the local horizon.
    if let Some(gr) = gr {
        let h = off.dot(gr.up);
        let min = 0.06;
        if h < min {
            off = (off + gr.up * (min - h)).normalized();
        }
    }
    let pos = ship.pos + off * dist;
    let rot = Mat3::look_at(ship.pos - pos, up_hint).transpose();
    Camera {
        pos,
        rot,
        fov_y: 1.0,
    }
}

/// The light of the star on the ship: dim at night under an air, with some sky light by day.
pub fn light(sim: &Sim) -> Light {
    let ship = &sim.ship;
    let sun = (ship.pos * -1.0).normalized();
    let (mut ambient, mut intensity) = (0.22f32, 1.0f32);
    if let Some((i, air, alt)) = sim.in_air() {
        let up = (ship.pos - sim.bodies[i].pos).normalized();
        let day = ((sun.dot(up) + 0.1) / 0.3).clamp(0.0, 1.0) as f32;
        let scatter = air.scattering(alt) as f32;
        intensity = day * (1.0 - 0.4 * scatter);
        ambient = 0.1 + 0.3 * scatter * day;
    }
    Light {
        sun,
        ambient,
        intensity,
    }
}

fn npc_state(sim: &Sim, n: &vector_sim::npc::Npc) -> State {
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
    st.time = sim.time + f64::from(n.id);
    st
}

fn role_name(r: NpcRole) -> &'static str {
    match r {
        NpcRole::Trader => "trader",
        NpcRole::Miner => "miner",
        NpcRole::Pirate => "pirate",
        NpcRole::Patrol => "patrol",
    }
}

/// Draw the stations, the other ships and the player's ship, solid, over the scene.
pub fn draw_ships(fb: &mut Framebuffer, cam: &Camera, v: &View, gr: Option<&Ground>) {
    let sim = v.sim;
    let g = v.game;
    let mut zb = Zbuf::new(fb.w, fb.h);
    let lit = light(sim);
    let focal = fb.h as f64 * 0.5 / (cam.fov_y * 0.5).tan();
    let ship = &sim.ship;
    // Stations.
    for b in &sim.bodies {
        if b.kind != vector_world::BodyKind::Station || sim.scan.is_empty() {
            continue;
        }
        let d = (b.pos - cam.pos).length();
        if b.radius / d.max(1.0) * focal < 2.0 {
            continue;
        }
        let m = sm::station_model(b.radius, sim.time * 0.05);
        let pose = Pose {
            pos: b.pos,
            rot: Mat3::rot_y(0.4),
            scale: 1.0,
        };
        sm::draw(fb, &mut zb, cam, &m, &pose, &lit);
    }
    // Other ships.
    for n in &sim.npcs {
        if !n.visible() {
            continue;
        }
        let d = (n.pos - cam.pos).length();
        if d > SEE_NPC || 30.0 / d.max(1.0) * focal < 1.5 {
            continue;
        }
        let design = sm::npc_design(&sim.data, role_name(n.role), n.id);
        let st = npc_state(sim, n);
        let m = sm::build(&sim.data, &design, &st);
        if f64::from(m.radius) / d.max(1.0) * focal < 1.0 {
            continue;
        }
        let fwd = if n.vel.length() > 1.0 {
            n.vel
        } else {
            ship.pos - n.pos
        };
        let pose = Pose {
            pos: n.pos,
            rot: Mat3::look_at(fwd, Vec3::new(0.0, 1.0, 0.0)),
            scale: 1.0,
        };
        sm::draw(fb, &mut zb, cam, &m, &pose, &lit);
    }
    // The player's ship, its shadow and its dust.
    let st = State::of_player(sim, gear_for(sim, gr));
    let m = sm::build(&sim.data, &ship.design, &st);
    let mut pose = Pose {
        pos: ship.pos,
        rot: ship.rot,
        scale: 1.0,
    };
    if let Some(gr) = gr {
        // Standing on its gear: the feet rest on the ground, not in it.
        let keel = f64::from(-m.foot_y);
        if st.gear > 0.95 && gr.h < keel {
            pose.pos = pose.pos + gr.up * (keel - gr.h);
        }
        shadow(fb, cam, &m, &pose, &lit, gr);
        dust(fb, cam, sim, &st, gr, f64::from(m.flame_len));
    }
    sm::draw(fb, &mut zb, cam, &m, &pose, &lit);
    let _ = g;
}

/// The shadow of the ship on the ground: every triangle projected along the light on the plane
/// of the ground under it, darkened once.
fn shadow(fb: &mut Framebuffer, cam: &Camera, m: &Model, pose: &Pose, lit: &Light, gr: &Ground) {
    let s = lit.sun.normalized();
    let su = s.dot(gr.up);
    if su < 0.05 || gr.h > 900.0 || lit.intensity < 0.1 {
        return;
    }
    let q0 = pose.pos - gr.up * gr.h;
    let focal = fb.h as f64 * 0.5 / (cam.fov_y * 0.5).tan();
    let (cx, cy) = (fb.w as f64 * 0.5, fb.h as f64 * 0.5);
    let mut mask = vec![false; fb.w * fb.h];
    let (w, h) = (fb.w, fb.h);
    for t in m.tris.iter().filter(|t| !t.add) {
        let mut pts = [(0.0f32, 0.0f32, 0.0f32); 3];
        let mut ok = true;
        for (k, p) in t.v.iter().enumerate() {
            let wp = pose.pos
                + pose
                    .rot
                    .mul_vec(Vec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2])));
            let tt = (wp - q0).dot(gr.up) / su;
            let on = wp - s * tt;
            let vw = cam.to_view(on);
            if vw.z < NEAR {
                ok = false;
                break;
            }
            pts[k] = (
                (cx + vw.x / vw.z * focal) as f32,
                (cy - vw.y / vw.z * focal) as f32,
                (1.0 / vw.z) as f32,
            );
        }
        if ok {
            sm::raster_tri(pts, w, h, |x, y, _| {
                mask[y as usize * w + x as usize] = true
            });
        }
    }
    let k = 0.55 + 0.35 * (1.0 - lit.intensity).clamp(0.0, 1.0);
    for (i, on) in mask.iter().enumerate() {
        if *on {
            let c = fb.px[i];
            let ch = |sft: u32| (((c >> sft) & 0xFF) as f32 * k) as u32;
            fb.px[i] = (ch(16) << 16) | (ch(8) << 8) | ch(0);
        }
    }
}

/// Dust and grit raised by the exhaust when the ship is low and burning.
fn dust(fb: &mut Framebuffer, cam: &Camera, sim: &Sim, st: &State, gr: &Ground, flame: f64) {
    if st.thrust < 0.05 || gr.h > 120.0 {
        return;
    }
    let q0 = sim.ship.pos - gr.up * gr.h;
    let (e, n) = {
        let a = gr.up.cross(Vec3::new(0.0, 0.0, 1.0));
        let e = if a.length() < 1e-6 {
            Vec3::new(1.0, 0.0, 0.0)
        } else {
            a.normalized()
        };
        (e, gr.up.cross(e))
    };
    let bucket = (sim.time * 12.0).floor() as i64;
    let reach = 8.0 + flame * 0.8 + gr.h * 1.2;
    let strength = ((120.0 - gr.h) / 120.0 * st.thrust.min(1.5)).clamp(0.0, 1.0);
    for k in 0..90i64 {
        let h = hash_coord(0xD057, bucket, k, 11);
        let f = |s: u32| ((h >> s) & 0xFFFF) as f64 / 65535.0;
        let (a, r, up) = (
            f(0) * std::f64::consts::TAU,
            f(16).sqrt() * reach,
            f(32) * 4.0,
        );
        let p = q0 + e * (a.cos() * r) + n * (a.sin() * r) + gr.up * (up * (1.0 - r / reach));
        if let Some((x, y)) = cam.project_world(p, fb) {
            let fade = (1.0 - r / reach) as f32 * strength as f32;
            let c = (0x7A as f32 * fade) as u32;
            let col = (c << 16) | ((c as f32 * 0.9) as u32) << 8 | (c as f32 * 0.7) as u32;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                fb.plot(x as i32 + dx, y as i32 + dy, col);
            }
        }
    }
}

/// The words of the third-person view: where to click to leave it, and what turns it.
pub fn overlay(fb: &mut Framebuffer, g: &Game) {
    let row = fb.rows().saturating_sub(12);
    let items: [(&str, Act); 3] = [
        ("[SUPPR] COCKPIT", Act::Key(KeyCode::Delete)),
        (
            if g.ext_chase {
                "[5] SUIVI"
            } else {
                "[5] LIBRE"
            },
            Act::Call(Game::ext_toggle_chase, 0),
        ),
        ("[0] RECENTRER", Act::Call(Game::ext_reset, 0)),
    ];
    fb.text(1, row - 1, "VUE EXTERNE", AMBER);
    let mut x = 1;
    for (label, act) in items {
        let hover = g.hit_cells(x, row, label.len(), 1, act, None);
        fb.text(x, row, label, if hover { 0x00D0_FFD0 } else { GREEN });
        x += label.len() + 2;
    }
    fb.text(1, row + 1, "SOURIS : ORBITE  MOLETTE : ZOOM", DIM);
}

impl Game {
    /// The third-person view is what the flight screen shows right now.
    pub fn ext_active(&self) -> bool {
        self.ext_view && self.screen == Screen::Flight && !self.map
    }

    pub fn toggle_ext_view(&mut self) {
        self.ext_view = !self.ext_view;
        self.say(if self.ext_view {
            "VUE EXTERNE (SUPPR POUR REVENIR AU COCKPIT)"
        } else {
            "VUE COCKPIT"
        });
    }

    pub fn ext_toggle_chase(&mut self, _: usize) {
        self.ext_chase = !self.ext_chase;
    }

    pub fn ext_reset(&mut self, _: usize) {
        self.ext_yaw = ZOOM_YAW;
        self.ext_pitch = 0.3;
        self.ext_zoom = ZOOM_START;
    }

    /// Turn the camera round the ship by (yaw, pitch) radians.
    pub fn ext_orbit(&mut self, dyaw: f64, dpitch: f64) {
        self.ext_yaw = (self.ext_yaw + dyaw).rem_euclid(std::f64::consts::TAU);
        self.ext_pitch = (self.ext_pitch + dpitch).clamp(-1.45, 1.45);
    }

    /// Move the camera nearer (factor < 1) or farther.
    pub fn ext_zoom_by(&mut self, factor: f64) {
        self.ext_zoom = (self.ext_zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
    }

    /// Keys of the third-person view; true when the key was one of them.
    pub fn ext_key(&mut self, code: KeyCode) -> bool {
        if !self.ext_active() {
            return false;
        }
        match code {
            KeyCode::Numpad4 => self.ext_orbit(-0.12, 0.0),
            KeyCode::Numpad6 => self.ext_orbit(0.12, 0.0),
            KeyCode::Numpad8 => self.ext_orbit(0.0, 0.1),
            KeyCode::Numpad2 => self.ext_orbit(0.0, -0.1),
            KeyCode::Equal | KeyCode::NumpadAdd => self.ext_zoom_by(0.85),
            KeyCode::Minus | KeyCode::NumpadSubtract => self.ext_zoom_by(1.0 / 0.85),
            KeyCode::Numpad5 => self.ext_toggle_chase(0),
            KeyCode::Numpad0 => self.ext_reset(0),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vector_data::GameData;

    fn game() -> Game {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let data = Arc::new(GameData::load(&dir).expect("data"));
        let mut g = Game::new(data, 5);
        g.new_game(5);
        g
    }

    #[test]
    fn the_key_switches_views_and_the_orbit_and_zoom_are_bounded() {
        let mut g = game();
        assert!(!g.ext_active());
        g.key(KeyCode::Delete, None, true);
        assert!(g.ext_active(), "SUPPR enters the third-person view");
        let z = g.ext_zoom;
        g.key(KeyCode::Minus, None, true);
        assert!(g.ext_zoom > z, "- zooms out");
        for _ in 0..200 {
            g.key(KeyCode::Numpad8, None, true);
            g.ext_zoom_by(0.5);
        }
        assert!(g.ext_pitch <= 1.45 && g.ext_zoom >= ZOOM_MIN);
        g.key(KeyCode::Delete, None, true);
        assert!(!g.ext_active(), "SUPPR again returns to the cockpit");
    }

    #[test]
    fn the_camera_orbits_at_the_asked_distance_and_looks_at_the_ship() {
        let mut g = game();
        g.ext_view = true;
        let len = ship_length(&g.sim);
        let cam = camera(&g, &g.sim, None);
        let d = (cam.pos - g.sim.ship.pos).length();
        assert!((d - len * g.ext_zoom).abs() < 1e-6, "{d}");
        // The ship is at the centre of the view.
        let v = cam.to_view(g.sim.ship.pos);
        assert!(v.z > 0.0 && v.x.abs() < 1e-6 * v.z && v.y.abs() < 1e-6 * v.z);
        g.ext_orbit(1.0, 0.0);
        let cam2 = camera(&g, &g.sim, None);
        assert!((cam2.pos - cam.pos).length() > 1.0, "yaw moves the camera");
    }

    #[test]
    fn the_gear_comes_down_close_to_the_ground_only() {
        let g = game();
        let near = Ground {
            up: Vec3::new(0.0, 1.0, 0.0),
            h: 30.0,
        };
        let far = Ground {
            h: 5000.0,
            ..near_like(&near)
        };
        assert_eq!(gear_for(&g.sim, Some(&near)), 1.0);
        assert_eq!(gear_for(&g.sim, Some(&far)), 0.0);
        assert_eq!(gear_for(&g.sim, None), 0.0);
    }

    fn near_like(g: &Ground) -> Ground {
        Ground { up: g.up, h: g.h }
    }
}
