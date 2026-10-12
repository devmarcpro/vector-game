//! Cockpit view and system map: 3D scene + ASCII HUD.
use crate::console::Console;
use crate::game::Game;
use crate::mouse::Act;
use vector_core::{Mat3, Vec3};
use vector_render::{Camera, Framebuffer, Wireframe};
use vector_sim::design::{T_DAMAGE, T_SCRAM};
use vector_sim::npc::NpcRole;
use vector_sim::voyage::{SCAN_FULL, SCAN_SURVEYED};
use vector_sim::Sim;
use vector_world::BodyKind;

pub const GREEN: u32 = 0x0030_E050;
pub const DIM: u32 = 0x0020_9048;
pub const AMBER: u32 = 0x00E0_A020;
pub const RED: u32 = 0x00FF_3030;
/// Shared accents of the screens (one definition each; the views import them).
pub const WHITE: u32 = 0x00FF_FFFF;
pub const UP: u32 = 0x0040_E070;
pub const DOWN: u32 = 0x00FF_5050;
pub const HOVER: u32 = 0x00D0_FFD0;
pub const WIRE: u32 = 0x0040_7050;
pub const CYAN: u32 = 0x0050_C0E0;
pub const STAR: u32 = 0x0040_5060;
pub const SUN: u32 = 0x00E0_D060;
pub const MINER: u32 = 0x00C0_C040;
pub const PATROL: u32 = 0x0040_80FF;

pub fn npc_color(r: NpcRole) -> u32 {
    match r {
        NpcRole::Trader => GREEN,
        NpcRole::Miner => MINER,
        NpcRole::Pirate => RED,
        NpcRole::Patrol => PATROL,
    }
}

pub struct View<'a> {
    /// The game, for the clickable zones the view declares.
    pub game: &'a Game,
    pub sim: &'a Sim,
    pub stars: &'a [Vec3],
    pub timescale: f64,
    pub paused: bool,
    pub messages: &'a [String],
    pub console: &'a Console,
    pub map: bool,
    /// Metres per pixel on the system map.
    pub map_scale: f64,
    pub map_on_ship: bool,
    /// Overdrive held: wider field of view and brighter dust streaks.
    pub boost: bool,
}

/// Local dust: a stationary lattice of points in the reference frame, drawn as streaks along the
/// relative velocity. The stars are at infinity and never shift, so this is what shows speed.
fn draw_dust(fb: &mut Framebuffer, cam: &Camera, v: &View) {
    const CELL: f64 = 1500.0;
    const REACH: i64 = 4;
    let rel = v.sim.rel_velocity();
    let speed = rel.length();
    if v.sim.docked.is_some() || speed < 0.5 {
        return;
    }
    // Streaks strobe under time acceleration: keep dots only.
    let streak = if v.timescale > 20.0 {
        Vec3::ZERO
    } else {
        let len = (speed * 0.05).min(3.0 * CELL);
        rel.normalized() * len
    };
    let col = if v.boost { GREEN } else { DIM };
    let p = v.sim.ship.pos;
    let (cx, cy, cz) = (
        (p.x / CELL).floor() as i64,
        (p.y / CELL).floor() as i64,
        (p.z / CELL).floor() as i64,
    );
    for dx in -REACH..=REACH {
        for dy in -REACH..=REACH {
            for dz in -REACH..=REACH {
                let (x, y, z) = (cx + dx, cy + dy, cz + dz);
                let h = vector_core::hash_coord(0xD057, x, y, z);
                let f = |s: u32| ((h >> s) & 0xFFFF) as f64 / 65535.0;
                let q = Vec3::new(
                    (x as f64 + f(0)) * CELL,
                    (y as f64 + f(16)) * CELL,
                    (z as f64 + f(32)) * CELL,
                );
                if (q - p).length() > REACH as f64 * CELL {
                    continue;
                }
                if streak.length() > 1.0 {
                    cam.draw_line(fb, q, q + streak, col);
                } else if let Some((sx, sy)) = cam.project_world(q, fb) {
                    fb.plot(sx as i32, sy as i32, col);
                }
            }
        }
    }
}

pub fn fmt_dist(m: f64) -> String {
    let a = m.abs();
    if a < 10_000.0 {
        format!("{m:.0} m")
    } else if a < 1.0e9 {
        format!("{:.1} km", m / 1e3)
    } else if a < 1.0e12 {
        format!("{:.2} Gm", m / 1e9)
    } else {
        format!("{:.3} AU", m / vector_core::units::AU)
    }
}

/// Whether a zone of radius `reach` around (x, y) touches the frame: a body behind the view or far
/// to the side is projected to coordinates thousands of pixels away and gets no click zone.
fn on_screen(fb: &Framebuffer, x: f32, y: f32, reach: f32) -> bool {
    x + reach > 0.0 && y + reach > 0.0 && x - reach < fb.w as f32 && y - reach < fb.h as f32
}

pub fn fmt_time(s: f64) -> String {
    crate::fmt::duration(s)
}

pub fn draw(fb: &mut Framebuffer, v: &View) {
    fb.clear();
    if v.map {
        draw_map(fb, v);
    } else {
        draw_cockpit(fb, v);
    }
    draw_status(fb, v);
    crate::landing_view::draw(fb, v);
    crate::nav_view::draw_panel(fb, v);
    draw_console(fb, v.console);
}

/// Colour that tells the type of a body at a glance. Only used once the body has been surveyed.
pub fn kind_color(k: BodyKind) -> u32 {
    use vector_world::{AstType, PlanetKind as P, StarClass as S};
    match k {
        BodyKind::Star(S::O) => 0x0080_A0FF,
        BodyKind::Star(S::B) => 0x00A0_C0FF,
        BodyKind::Star(S::A) => 0x00D0_E0FF,
        BodyKind::Star(S::F) => 0x00F8_F8E0,
        BodyKind::Star(S::G) => 0x00FF_E060,
        BodyKind::Star(S::K) => 0x00FF_A040,
        BodyKind::Star(S::M) => 0x00FF_6040,
        BodyKind::Star(S::D) => 0x00E8_E8FF,
        BodyKind::Star(S::N) => 0x00C0_D8FF,
        BodyKind::Star(S::X) => 0x0030_3038,
        BodyKind::Star(S::R) => 0x00A0_3828,
        BodyKind::Planet(p) | BodyKind::Moon(p) => match p {
            P::Barren => 0x00A8_A8A8,
            P::Dwarf => 0x0088_90A0,
            P::Desert => 0x00E8_B060,
            P::Terran => 0x0050_E070,
            P::Ocean => 0x0040_90FF,
            P::Tundra => 0x0090_D8C8,
            P::Ice => 0x00C8_F0FF,
            P::Lava => 0x00FF_5020,
            P::Venusian => 0x00E8_D040,
            P::GasGiant => 0x00E8_9050,
            P::IceGiant => 0x0060_A8F0,
            P::HotJupiter => 0x00FF_3080,
        },
        BodyKind::Asteroid(t) => match t {
            AstType::C => 0x00A0_9078,
            AstType::S => 0x00C8_A870,
            AstType::M => 0x00B8_C8D8,
            AstType::I => 0x0088_C8FF,
            AstType::V => 0x00D8_9878,
            AstType::D => 0x0070_5040,
        },
        BodyKind::Station => 0x0040_E8E8,
        BodyKind::Anomaly(_) => 0x00C0_60FF,
    }
}

fn dimmed(c: u32, k: f32) -> u32 {
    let ch = |s: u32| (((c >> s) & 0xFF) as f32 * k) as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// Draw solid world `i` as seen from the cockpit: a lit ball in the colours of its biomes with
/// its relief shaded and its air at the rim. Returns false when the world has no surface to show.
fn draw_world(fb: &mut Framebuffer, cam: &Camera, v: &View, i: usize) -> bool {
    let Some(map) = v.game.flight_map(i) else {
        return false;
    };
    let b = &v.sim.bodies[i];
    // The world turns on its tilted axis once per day of its own.
    let angle = std::f64::consts::TAU * v.sim.time / v.sim.system.spin_period(i).max(60.0);
    let orient = Mat3::rot_z(f64::from(map.surface.tilt)).mul(&Mat3::rot_y(angle));
    let sun = (b.pos * -1.0).normalized();
    // Low over the ground the world stops being a ball: the same ground is marched to the horizon.
    if terrain_view(fb, cam, v, i, &map, &orient) {
        return true;
    }
    // The ball itself, in plain colours: lit by the star, relief shaded, air at the rim.
    crate::globe::fill_world(
        fb,
        cam,
        b.pos,
        b.radius,
        &orient,
        sun,
        &crate::globe::Paint {
            map: Some(&map),
            color: &|x, y| crate::surface_view::ground_color(v.sim, &map, x, y),
            air: map.pressure,
            clouds: Some(&|lat, lon| v.sim.cloud_cover(&map, lat, lon)),
        },
    );
    true
}

/// How low (as a share of the radius, at most 200 km) the ship must be for the ground to be
/// drawn as terrain instead of a ball.
const LOW_SHARE: f64 = 0.03;

/// The ground seen from a ship flying low over world `i`: one heightfield from the feet of the
/// ship to the horizon, in the colours of the biomes, lit by the star, hazed by the air. Roll is
/// not shown; looking straight down falls back to the ball.
fn terrain_view(
    fb: &mut Framebuffer,
    cam: &Camera,
    v: &View,
    i: usize,
    map: &vector_sim::planet::PlanetMap,
    orient: &Mat3,
) -> bool {
    let b = &v.sim.bodies[i];
    let rel = cam.pos - b.pos;
    let alt = rel.length() - b.radius;
    if alt > (b.radius * LOW_SHARE).min(200_000.0) || alt < 0.0 {
        return false;
    }
    let up = rel.normalized();
    let inv = orient.transpose();
    let nb = inv.mul_vec(up);
    let (lat, lon) = (nb.y.clamp(-1.0, 1.0).asin(), nb.z.atan2(nb.x));
    let east = orient.mul_vec(Vec3::new(-lon.sin(), 0.0, lon.cos()));
    let north = orient.mul_vec(Vec3::new(
        -lat.sin() * lon.cos(),
        lat.cos(),
        -lat.sin() * lon.sin(),
    ));
    let fwd = cam.rot.transpose().mul_vec(Vec3::new(0.0, 0.0, 1.0));
    let pitch = fwd.dot(up).clamp(-1.0, 1.0).asin();
    // Straight down the heightfield degenerates: look at it from a steep angle instead.
    let pitch = pitch.max(-1.45);
    let yaw = fwd.dot(east).atan2(fwd.dot(north));
    let sun = (b.pos * -1.0).normalized();
    let air = map.pressure;
    let t = crate::terrain::Terrain::new(map, b.radius).with_season(v.sim.season_phase(map.body));
    let data = &v.sim.data;
    let focal = fb.h as f64 * 0.5 / (cam.fov_y * 0.5).tan();
    let horizon_dist = (2.0 * b.radius * alt.max(10.0)).sqrt();
    crate::terrain::render(
        fb,
        &t,
        &|x, y| data.biomes[map.biome_at(x, y)].color,
        &|_, _| None,
        &crate::terrain::Eye {
            lat,
            lon,
            height: alt,
            yaw,
            pitch,
        },
        &crate::terrain::Frame {
            x0: 0,
            y0: 0,
            w: fb.w as i32,
            h: fb.h as i32,
            focal,
        },
        &crate::terrain::Sky {
            cloud: v.sim.cloud_cover(map, lat, lon),
            air: v.sim.air_of(i).filter(|a| a.ground > 0.05).map(|a| {
                // The zenith goes dark with height as the air thins out.
                let k = (a.pressure(alt) / a.ground).sqrt() as f32;
                (
                    scale_rgb(0x0030_60B0, k),
                    scale_rgb(0x00A8_C8E8, 0.15 + 0.85 * k),
                )
            }),
            haze: 40_000.0 / air.max(0.1),
            scale: v.sim.air_of(i).map_or(8000.0, |a| a.height),
        },
        &crate::terrain::Light {
            up: sun.dot(up),
            east: sun.dot(east),
            north: sun.dot(north),
        },
        (horizon_dist * 1.15).min(900_000.0),
    );
    true
}

fn scale_rgb(c: u32, k: f32) -> u32 {
    let ch = |s: u32| (((c >> s) & 0xFF) as f32 * k).clamp(0.0, 255.0) as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// How much of the sky the air hides from where the ship is: 1 sees everything (the void, or
/// the night over thin air), 0 sees nothing but the haze (low in a thick air by day). Also the
/// layer of the air and its pressure, for the readout.
fn sky_view(v: &View) -> (f64, Option<(&'static str, f64)>) {
    let Some((i, air, alt)) = v.sim.in_air() else {
        return (1.0, None);
    };
    let b = &v.sim.bodies[i];
    let up = (v.sim.ship.pos - b.pos).normalized();
    let sun = (b.pos * -1.0).normalized();
    // Day when the star is above the horizon, with a twilight.
    let day = ((sun.dot(up) + 0.1) / 0.3).clamp(0.0, 1.0);
    let scatter = air.scattering(alt);
    let hidden = scatter * (day + (1.0 - day) * 0.15);
    (
        1.0 - hidden,
        air.layer(alt).map(|name| (name, air.pressure(alt))),
    )
}

/// A world as a lit ball of one colour, or with the bands of a gas giant.
fn draw_ball(fb: &mut Framebuffer, cam: &Camera, v: &View, i: usize, col: u32, bands: bool) {
    let b = &v.sim.bodies[i];
    let sys = &v.sim.system.bodies[i];
    let angle = std::f64::consts::TAU * v.sim.time / v.sim.system.spin_period(i).max(60.0);
    let orient = Mat3::rot_y(angle);
    let sun = (b.pos * -1.0).normalized();
    let seed = v.sim.system.seed ^ (i as u64) << 8;
    crate::globe::fill_world(
        fb,
        cam,
        b.pos,
        b.radius,
        &orient,
        sun,
        &crate::globe::Paint {
            map: None,
            color: &|x, y| {
                if !bands {
                    return col;
                }
                // Bands by latitude, each a little lighter or darker than the base colour.
                let band = (y / 4) as i64;
                let h = vector_core::hash_coord(seed, band, 3, 1);
                let k = 1.1 + 1.0 * vector_core::unit(h) as f32;
                let wobble = 1.0 + 0.06 * (((x as f32) * 0.2 + band as f32).sin());
                let ch =
                    |s: u32| ((((col >> s) & 0xFF) as f32) * k * wobble).clamp(0.0, 255.0) as u32;
                (ch(16) << 16) | (ch(8) << 8) | ch(0)
            },
            air: if bands { 1.0 } else { sys.pressure },
            clouds: None,
        },
    );
}

fn body_color(v: &View, i: usize) -> u32 {
    if v.sim.scan[i] >= SCAN_SURVEYED {
        kind_color(v.sim.bodies[i].kind)
    } else {
        GREEN
    }
}

/// Map and radar markers: the type colour, amber for the selected target.
fn marker_color(v: &View, i: usize) -> u32 {
    if Some(i) == v.sim.target {
        AMBER
    } else {
        body_color(v, i)
    }
}

/// Corner brackets around the selected object.
fn draw_reticle(fb: &mut Framebuffer, x: f32, y: f32, r: f32) {
    let (r, t) = (r.max(10.0) + 6.0, 6.0);
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let (cx, cy) = (x + sx * r, y + sy * r);
        fb.line(cx, cy, cx - sx * t, cy, AMBER);
        fb.line(cx, cy, cx, cy - sy * t, AMBER);
    }
}

fn draw_cockpit(fb: &mut Framebuffer, v: &View) {
    let ship = &v.sim.ship;
    // Third-person view: the same scene seen from a camera orbiting the ship.
    let ext = v.game.ext_active();
    let gr = if ext {
        crate::ext_view::ground(v.game, v.sim)
    } else {
        None
    };
    let cam = if ext {
        crate::ext_view::camera(v.game, v.sim, gr.as_ref())
    } else {
        Camera {
            pos: ship.pos,
            rot: v.game.view_rot(&ship.rot),
            fov_y: if v.boost { 1.4 } else { 1.2 },
        }
    };
    let focal = fb.h as f64 * 0.5 / (cam.fov_y * 0.5).tan();

    // Low in a thick air by day the sky hides the stars and the worlds beyond.
    let (clear, layer) = sky_view(v);
    if clear > 0.3 {
        for s in v.stars {
            if let Some((x, y)) = cam.project_dir(*s, fb) {
                fb.plot(x as i32, y as i32, scale_rgb(STAR, clear as f32));
            }
        }
    }
    if let Some((name, p)) = layer {
        fb.text(1, 12, &format!("ATMOSPHERE : {name}  {p:.4} BAR"), AMBER);
    }
    draw_dust(fb, &cam, v);
    for (i, b) in v.sim.bodies.iter().enumerate() {
        if v.sim.scan[i] == 0 {
            continue;
        }
        let col = body_color(v, i);
        let dist = (b.pos - ship.pos).length();
        // Seen through the haze, faraway worlds are gone (the star and what is near stay).
        if clear < 0.3
            && !matches!(b.kind, BodyKind::Star(_))
            && dist > 1.0e6
            && v.sim.in_air().is_none_or(|(w, ..)| w != i)
        {
            continue;
        }
        let px = b.radius / dist.max(1.0) * focal;
        let spin = Mat3::rot_y(v.sim.time * 2.0e-5 + i as f64);
        match b.kind {
            BodyKind::Star(c) => {
                // A variable star swells and shrinks with its light.
                let swell = (v.sim.system.luminosity_at(v.sim.time) / v.sim.system.luminosity)
                    .max(0.0)
                    .sqrt()
                    .sqrt();
                let swell = if i == 0 { swell } else { 1.0 };
                cam.draw_sphere(
                    fb,
                    b.pos,
                    b.radius * swell,
                    &spin,
                    4,
                    8,
                    kind_color(BodyKind::Star(c)),
                )
            }
            BodyKind::Planet(k) if k.is_giant() => draw_ball(fb, &cam, v, i, col, true),
            BodyKind::Planet(_) | BodyKind::Moon(_) => {
                // A surveyed solid world close enough to see: its relief, seas and continents;
                // otherwise a plain lit ball of its colour. No wireframe on worlds.
                if !(px > 22.0 && v.sim.scan[i] >= SCAN_SURVEYED && draw_world(fb, &cam, v, i)) {
                    draw_ball(fb, &cam, v, i, col, false);
                }
            }
            BodyKind::Anomaly(_) => {
                // Anomalies: a lattice sphere that pulses, so they do not look like rocks.
                let pulse = 1.0 + 0.08 * (v.sim.time * 0.7).sin();
                cam.draw_sphere(fb, b.pos, b.radius * pulse, &spin, 3, 5, 0x00C0_60FF);
            }
            BodyKind::Asteroid(_) if ext && px > 3.0 => draw_ball(fb, &cam, v, i, col, false),
            BodyKind::Asteroid(_) => cam.draw_sphere(fb, b.pos, b.radius, &spin, 4, 6, col),
            // Solid stations are drawn with the ships, below.
            BodyKind::Station if ext => {}
            BodyKind::Station => {
                let r = Mat3::rot_y(v.sim.time * 0.2);
                Wireframe::cube(b.radius * 0.5).draw(&cam, fb, &r, b.pos, col);
                let ring = Mat3::rot_x(std::f64::consts::FRAC_PI_2).mul(&r);
                cam.draw_sphere(fb, b.pos, b.radius * 0.9, &ring, 2, 0, col);
            }
        }
        // Marker + label for objects too small to see as shapes.
        if let Some((x, y)) = cam.project_world(b.pos, fb) {
            // Click to target, double click to fly there.
            let reach = (px as f32).clamp(9.0, 220.0);
            if on_screen(fb, x, y, reach) {
                v.game.hit(
                    (
                        (x - reach) as i32,
                        (y - reach) as i32,
                        (x + reach) as i32,
                        (y + reach) as i32,
                    ),
                    Act::Call(Game::click_body, i),
                    Some(Act::Call(Game::dbl_body, i)),
                );
            }
            if px < 6.0 && !matches!(b.kind, BodyKind::Star(_)) {
                fb.line(x - 3.0, y, x + 3.0, y, col);
                fb.line(x, y - 3.0, x, y + 3.0, col);
            }
            crate::sky_view::overlay(fb, &cam, v, i, x, y, px);
            // Atmosphere halo and giant rings, once the body is big enough to show them.
            if px > 10.0 && v.sim.scan[i] >= SCAN_SURVEYED {
                if let BodyKind::Planet(k) | BodyKind::Moon(k) = b.kind {
                    use vector_world::PlanetKind as P;
                    let r = px as f32;
                    if matches!(k, P::Terran | P::Ocean | P::Venusian | P::Tundra) || k.is_giant() {
                        fb.ellipse(x, y, r * 1.08, r * 1.08, dimmed(col, 0.5));
                    }
                }
            }
            let target = Some(i) == v.sim.target;
            if target {
                draw_reticle(fb, x, y, px as f32);
            }
            if target || (matches!(b.kind, BodyKind::Planet(_) | BodyKind::Station) && px < 40.0) {
                let tc = if target { AMBER } else { col };
                fb.text_at(x as i32 + 8, y as i32 - 12, body_label(v, i), tc);
            }
        }
    }

    if ext {
        crate::ext_view::draw_ships(fb, &cam, v, gr.as_ref());
    }
    let down = if ext {
        ship.rot.col(2) * (crate::ext_view::ship_length(v.sim) * 0.5)
    } else {
        ship.rot.col(1) * -6.0
    };
    for (a, b, mine) in &v.sim.beams {
        cam.draw_line(fb, *a + down, *b, if *mine { RED } else { AMBER });
    }
    for (a, b) in &v.sim.enemy_beams {
        cam.draw_line(fb, *a, *b, RED);
    }
    for p in &v.sim.projectiles {
        let tail = p.pos - p.vel.normalized() * 40.0;
        let col = match p.kind {
            vector_sim::combat::ProjKind::Kinetic => AMBER,
            vector_sim::combat::ProjKind::Missile { .. } => 0x00FF_8040,
            vector_sim::combat::ProjKind::Mine { .. } => 0x00A0_A0A0,
        };
        cam.draw_line(fb, tail, p.pos, col);
    }
    for w in &v.sim.wrecks {
        if let Some((x, y)) = cam.project_world(w.pos, fb) {
            fb.line(x - 4.0, y - 4.0, x + 4.0, y + 4.0, MINER);
            fb.line(x - 4.0, y + 4.0, x + 4.0, y - 4.0, MINER);
            if (w.pos - ship.pos).length() < 2.0e5 {
                fb.text_at(x as i32 + 8, y as i32 + 6, &w.name, MINER);
            }
        }
    }
    let see = 1.0e6 + 0.01 * v.sim.effective.scan_range;
    let ship_wire = Wireframe::ship();
    for (k, n) in v.sim.npcs.iter().enumerate() {
        if !n.visible() {
            continue;
        }
        let d = (n.pos - ship.pos).length();
        if d > see {
            continue;
        }
        let col = if Some(k) == v.sim.target_npc {
            AMBER
        } else {
            npc_color(n.role)
        };
        if d < 6.0e4 && !ext {
            let fwd = if n.vel.length() > 1.0 {
                n.vel
            } else {
                Vec3::new(0.0, 0.0, 1.0)
            };
            let rot = Mat3::look_at(fwd, Vec3::new(0.0, 1.0, 0.0));
            let scaled = Wireframe {
                verts: ship_wire.verts.iter().map(|p| *p * 80.0).collect(),
                edges: ship_wire.edges.clone(),
            };
            scaled.draw(&cam, fb, &rot, n.pos, col);
        }
        if let Some((x, y)) = cam.project_world(n.pos, fb) {
            if on_screen(fb, x, y, 12.0) {
                v.game.hit(
                    (
                        (x - 12.0) as i32,
                        (y - 12.0) as i32,
                        (x + 12.0) as i32,
                        (y + 12.0) as i32,
                    ),
                    Act::Call(Game::click_npc, k),
                    None,
                );
            }
            fb.line(x - 3.0, y, x, y - 3.0, col);
            fb.line(x, y - 3.0, x + 3.0, y, col);
            fb.line(x + 3.0, y, x, y + 3.0, col);
            fb.line(x, y + 3.0, x - 3.0, y, col);
            if Some(k) == v.sim.target_npc || d < 3.0e5 {
                fb.text_at(x as i32 + 6, y as i32 + 6, &n.name, col);
            }
            if Some(k) == v.sim.target_npc {
                for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    fb.line(
                        x + sx * 14.0,
                        y + sy * 14.0,
                        x + sx * 9.0,
                        y + sy * 14.0,
                        AMBER,
                    );
                    fb.line(
                        x + sx * 14.0,
                        y + sy * 14.0,
                        x + sx * 14.0,
                        y + sy * 9.0,
                        AMBER,
                    );
                }
            }
        }
    }

    let (w, h) = (fb.w as f32, fb.h as f32);
    let (cx, cy) = (w / 2.0, h / 2.0);
    for (dx, dy, ex, ey) in if ext {
        [(0.0, 0.0, 0.0, 0.0); 4]
    } else {
        [
            (-14.0, 0.0, 6.0, 0.0),
            (8.0, 0.0, 6.0, 0.0),
            (0.0, -14.0, 0.0, 6.0),
            (0.0, 8.0, 0.0, 6.0),
        ]
    } {
        fb.line(cx + dx, cy + dy, cx + dx + ex, cy + dy + ey, GREEN);
    }
    // Prograde marker (relative to the reference frame).
    let rel = v.sim.rel_velocity();
    if rel.length() > 0.5 {
        if let Some((x, y)) = cam.project_dir(rel.normalized(), fb) {
            fb.circle(x, y, 6.0, GREEN);
            fb.line(x - 10.0, y, x - 6.0, y, GREEN);
            fb.line(x + 6.0, y, x + 10.0, y, GREEN);
            fb.line(x, y - 10.0, x, y - 6.0, GREEN);
        }
    }
    if let Some(t) = v.sim.target.and_then(|i| v.sim.bodies.get(i)) {
        if let Some((x, y)) = cam.project_world(t.pos, fb) {
            let r = 16.0;
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                fb.line(
                    x + sx * r,
                    y + sy * r,
                    x + sx * (r - 6.0),
                    y + sy * r,
                    AMBER,
                );
                fb.line(
                    x + sx * r,
                    y + sy * r,
                    x + sx * r,
                    y + sy * (r - 6.0),
                    AMBER,
                );
            }
        }
    }
    if v.sim.laser_on && !ext {
        if let Some((x, y)) = v
            .sim
            .target
            .and_then(|i| cam.project_world(v.sim.bodies[i].pos, fb))
        {
            let flick = (v.sim.time * 40.0).sin() as f32 * 2.0;
            fb.line(cx - 30.0, h - 90.0, x + flick, y, RED);
            fb.line(cx + 30.0, h - 90.0, x - flick, y, RED);
            fb.circle(x, y, 4.0 + flick.abs(), AMBER);
        }
    }
    if ext {
        crate::ext_view::overlay(fb, v.game);
    }
    draw_radar(fb, v);
}

fn draw_map(fb: &mut Framebuffer, v: &View) {
    let sim = v.sim;
    let center = if v.map_on_ship {
        sim.ship.pos
    } else {
        Vec3::ZERO
    };
    let (cx, cy) = (fb.w as f64 / 2.0, fb.h as f64 / 2.0);
    let to_px = |p: Vec3| -> (f32, f32) {
        (
            (cx + (p.x - center.x) / v.map_scale) as f32,
            (cy + (p.z - center.z) / v.map_scale) as f32,
        )
    };
    for (i, cb) in sim.system.bodies.iter().enumerate() {
        let b = &sim.bodies[i];
        let col = marker_color(v, i);
        if let Some(o) = &cb.orbit {
            if o.a / v.map_scale > 3.0 {
                let parent = sim.bodies[o.parent].pos;
                let period = o.period();
                let n = 120;
                let mut prev: Option<(f32, f32)> = None;
                for k in 0..=n {
                    let t = sim.time + period * k as f64 / n as f64;
                    let (rp, _) = o.state(t);
                    let p = to_px(parent + rp);
                    if let Some(q) = prev {
                        fb.line(q.0, q.1, p.0, p.1, DIM);
                    }
                    prev = Some(p);
                }
            }
        }
        let (x, y) = to_px(b.pos);
        let r = (b.radius / v.map_scale).clamp(2.0, 14.0) as f32;
        if v.sim.scan[i] > 0 && on_screen(fb, x, y, r.max(7.0)) {
            let reach = r.max(7.0);
            v.game.hit(
                (
                    (x - reach) as i32,
                    (y - reach) as i32,
                    (x + reach) as i32,
                    (y + reach) as i32,
                ),
                Act::Call(Game::click_body, i),
                Some(Act::Call(Game::dbl_body, i)),
            );
        }
        match cb.kind {
            BodyKind::Star(c) => fb.circle(
                x,
                y,
                r.max(4.0),
                if v.sim.scan[i] > 0 {
                    kind_color(BodyKind::Star(c))
                } else {
                    SUN
                },
            ),
            BodyKind::Asteroid(_) => fb.plot(x as i32, y as i32, col),
            BodyKind::Anomaly(_) => {
                fb.circle(x, y, 4.0, 0x00C0_60FF);
                fb.plot(x as i32, y as i32, 0x00C0_60FF);
            }
            BodyKind::Station => {
                fb.line(x - 3.0, y - 3.0, x + 3.0, y + 3.0, col);
                fb.line(x - 3.0, y + 3.0, x + 3.0, y - 3.0, col);
            }
            _ => fb.circle(x, y, r.min(8.0), col),
        }
        crate::sky_view::map_overlay(fb, v, i, &to_px);
        let shown = match cb.kind {
            BodyKind::Asteroid(_) => false,
            BodyKind::Anomaly(_) => true,
            BodyKind::Moon(_) | BodyKind::Station => {
                cb.orbit.as_ref().is_some_and(|o| o.a / v.map_scale > 12.0)
            }
            _ => true,
        };
        if shown || Some(i) == v.sim.target {
            fb.text_at(x as i32 + 6, y as i32 - 12, &cb.name, col);
        }
    }
    crate::nav_view::draw_map_overlay(fb, v, &to_px);
    let (sx, sy) = to_px(sim.ship.pos);
    fb.line(sx - 5.0, sy, sx + 5.0, sy, RED);
    fb.line(sx, sy - 5.0, sx, sy + 5.0, RED);
    fb.text_at(sx as i32 + 8, sy as i32 + 4, "VAISSEAU", RED);
    let rows = fb.rows();
    // Column 50: the gauges at the bottom left own the columns before it.
    fb.text(
        50,
        rows - 5,
        &format!(
            "CARTE SYSTEME  ECHELLE {}/PX  [+/-] ZOOM  [C] CENTRE:{}",
            fmt_dist(v.map_scale),
            if v.map_on_ship { "VAISSEAU" } else { "ETOILE" }
        ),
        DIM,
    );
}

fn draw_status(fb: &mut Framebuffer, v: &View) {
    let ship = &v.sim.ship;
    let sim = v.sim;
    let rows = fb.rows();
    let cols = fb.cols();
    let rel = sim.rel_velocity();
    let frame = sim
        .ref_body
        .map_or("ETOILE".to_string(), |i| sim.bodies[i].name.clone());
    fb.text(
        1,
        1,
        &format!(
            "{} ({})  CLASSE {}",
            sim.system.name,
            sim.system.designation,
            sim.system.star_class.letter()
        ),
        GREEN,
    );
    fb.text(
        1,
        2,
        &format!("VITESSE {}/S  REF {}", fmt_dist(rel.length()), frame),
        GREEN,
    );
    fb.text(
        1,
        3,
        &format!(
            "ASSIST {}  TEMPS x{}{}  T+{}",
            if ship.assist { "ON" } else { "OFF" },
            v.timescale,
            if v.paused { " PAUSE" } else { "" },
            fmt_time(sim.time)
        ),
        if ship.assist { GREEN } else { AMBER },
    );
    if sim.rules.is_modified() {
        fb.text(cols.saturating_sub(18), 1, "PARTIE MODIFIEE", AMBER);
    }
    if let Some(ap) = &sim.autopilot {
        // How long and how much: the trip can take days and a good part of the tank.
        let (eta, dv) = ap.eta_and_dv();
        fb.text(
            1,
            4,
            &format!(
                "PILOTE AUTO -> {}  A={:.2} M/S2  RESTE {}  DV {:.0} KM/S",
                sim.bodies[ap.target].name,
                ap.a_cruise,
                fmt_time(eta),
                dv / 1e3
            ),
            AMBER,
        );
    }
    fb.gauge(
        1,
        rows - 4,
        "CARBURANT",
        ship.fuel / ship.stats.fuel_cap.max(1.0),
        20,
        if ship.fuel < ship.stats.fuel_cap * 0.1 {
            RED
        } else {
            GREEN
        },
    );
    fb.gauge(
        1,
        rows - 3,
        "COQUE    ",
        ship.hull / ship.stats.hull_max.max(1.0),
        20,
        if ship.hull < ship.stats.hull_max * 0.3 {
            RED
        } else {
            GREEN
        },
    );
    if v.sim.effective.shield_cap > 0.0 {
        let frac = ship.sys.shield_pts / v.sim.effective.shield_cap;
        fb.gauge(
            1,
            rows - 7,
            "BOUCLIER ",
            frac,
            20,
            if frac < 0.25 { RED } else { PATROL },
        );
    }
    let batt = ship.sys.battery_mj / ship.stats.battery_cap.max(1.0);
    fb.gauge(
        1,
        rows - 6,
        "BATTERIE ",
        batt,
        20,
        if batt < 0.15 { RED } else { GREEN },
    );
    let temp_frac = ((ship.sys.temp_k - 150.0) / (T_SCRAM - 150.0)).clamp(0.0, 1.0);
    let temp_col = if ship.sys.temp_k > T_DAMAGE {
        RED
    } else if ship.sys.temp_k > 500.0 {
        AMBER
    } else {
        GREEN
    };
    fb.gauge(1, rows - 5, "TEMP     ", temp_frac, 20, temp_col);
    // The gauge above is 32 columns wide from column 1: the numbers start after it.
    fb.text(
        35,
        rows - 5,
        &format!(
            "{:.0} K  {:.1}/{:.1} MW",
            ship.sys.temp_k,
            ship.sys.demand_kw / 1e3,
            ship.sys.supply_kw / 1e3
        ),
        DIM,
    );
    fb.text(
        1,
        rows - 2,
        &format!("DELTA-V {:.1} KM/S", ship.delta_v() / 1e3),
        DIM,
    );

    // The days the cash pays the crew: a quiet clock in the credits line.
    let payroll = sim.crew.daily_payroll();
    let (pay_note, pay_col) = if payroll > 0.0 && sim.credits < payroll * 15.0 {
        let days = (sim.credits / payroll).max(0.0);
        (
            format!("  PAIE : {days:.0} J"),
            if days < 5.0 { RED } else { AMBER },
        )
    } else {
        (String::new(), GREEN)
    };
    fb.text(1, 5, &format!("CREDITS {:.0}", sim.credits), GREEN);
    if !pay_note.is_empty() {
        let col = format!("CREDITS {:.0}", sim.credits).len() + 1;
        fb.text(col, 5, &pay_note, pay_col);
    }
    fb.text(
        1,
        6,
        &format!("SOUTE {:.0}/{:.0} KG", ship.cargo.mass, ship.cargo.cap_mass),
        if ship.cargo.is_full() { RED } else { GREEN },
    );
    fb.text(
        1,
        8,
        &format!(
            "EQUIPAGE {}/{:.0}  AIR {:.0}%",
            sim.crew.count(),
            ship.stats.crew_cap,
            sim.crew.air * 100.0
        ),
        if sim.crew.air < 0.5 { RED } else { GREEN },
    );
    if let Some((_, range)) = sim.nearest_station() {
        if range < vector_sim::DOCK_RANGE * 4.0 {
            fb.text(
                1,
                7,
                &format!("STATION A {} - [D] AMARRER", fmt_dist(range)),
                AMBER,
            );
        }
    }
    let terr = match sim.faction {
        Some(f) => format!(
            "TERRITOIRE {}  REP {:+.0}",
            sim.data.factions[f].name.to_uppercase(),
            sim.reputation_with(Some(f))
        ),
        None => "ESPACE NON REVENDIQUE".to_string(),
    };
    fb.text(
        1,
        10,
        &terr,
        sim.faction.map_or(DIM, |f| sim.data.factions[f].color),
    );
    {
        let env = sim.environment_at(ship.pos);
        let mut danger = Vec::new();
        if env.radiation > 0.0 {
            danger.push(format!("RADIATIONS {:.1} GY/H", env.radiation));
        }
        if env.ion > 0.0 {
            danger.push("TEMPETE IONIQUE".to_string());
        }
        if env.gravity.length() > 0.5 {
            danger.push(format!(
                "PUITS GRAVITATIONNEL {:.1} M/S2",
                env.gravity.length()
            ));
        }
        if env.sensor_mult < 0.9 {
            danger.push("CAPTEURS BRUITES".to_string());
        }
        if env.agility_mult < 0.9 {
            danger.push("INTERFERENCES MAGNETIQUES".to_string());
        }
        if env.rift > 0.0 {
            danger.push("FAILLE SPATIALE".to_string());
        }
        if !danger.is_empty() {
            fb.text(1, 13, &format!("DANGER : {}", danger.join(" / ")), RED);
        }
        let cur = sim.current_research.map_or("AUCUNE".to_string(), |t| {
            format!(
                "{} {:.0}%",
                sim.data.techs[t].name.to_uppercase(),
                100.0 * sim.research_progress / sim.data.techs[t].cost.max(1.0)
            )
        });
        fb.text(
            1,
            14,
            &format!(
                "RECHERCHE : {cur}   RESERVE {:.0} PTS  [F4]",
                sim.research_pool
            ),
            DIM,
        );
    }
    if sim.is_wanted() {
        fb.text(1, 11, "RECHERCHE DANS CE TERRITOIRE !", RED);
    }
    // The war around the ship: a siege, the radar, the flares, the squadron.
    {
        let mut war: Vec<String> = Vec::new();
        if sim.here_besieged().is_some() {
            war.push("BLOCUS DU SECTEUR".to_string());
        }
        if sim.active_sensors {
            war.push("RADAR ACTIF".to_string());
        }
        if !sim.decoys.is_empty() {
            war.push(format!("LEURRES {}", sim.decoys.len()));
        }
        let escorts = sim
            .fleet
            .iter()
            .filter(|f| f.order != vector_sim::fleet::FleetOrder::Trade)
            .count();
        if escorts > 0 {
            war.push(format!("ESCADRE {escorts}"));
        }
        if !war.is_empty() {
            fb.text(1, 10, &war.join("  "), AMBER);
        }
    }
    let ammo = |id: &str| {
        sim.data
            .resource_index(id)
            .map_or(0.0, |r| ship.cargo.kg[r])
    };
    if ship
        .design
        .modules(&sim.data)
        .any(|(_, m)| !m.weapon.is_empty())
    {
        fb.text(
            1,
            12,
            &format!(
                "MUNITIONS  OBUS {:.0}  MISSILES {:.0}  MINES {:.0}",
                ammo("ammo_slugs"),
                ammo("missiles"),
                ammo("mines")
            ),
            DIM,
        );
    }
    draw_physics_panel(fb, v, rows);
    draw_target_panel(fb, v, rows, cols);
    if let Some(j) = &sim.jump {
        let line = if j.in_transit {
            let done = 1.0 - j.transit_left / j.total_transit.max(1.0);
            format!(
                "TRANSIT FTL {:.0}%  RESTE {}",
                done * 100.0,
                fmt_time(j.transit_left)
            )
        } else {
            format!(
                "CHARGE FTL {:.0}S  [RETOUR ARR.] ANNULER",
                j.charge_left.max(0.0)
            )
        };
        fb.text(1, 9, &line, AMBER);
    }
    for (i, m) in v.messages.iter().rev().take(4).enumerate() {
        fb.text(
            cols / 2 - m.len().min(cols) / 2,
            2 + i,
            m,
            if i == 0 { AMBER } else { DIM },
        );
    }
    if ship.hull <= 0.0 {
        fb.text(cols / 2 - 10, rows / 2 + 4, "*** COQUE DETRUITE ***", RED);
    }
}

/// Orbit, heat, dose and load of the ship: the numbers a pilot flies by.
fn draw_physics_panel(fb: &mut Framebuffer, v: &View, rows: usize) {
    use vector_sim::navigation as nav;
    let sim = v.sim;
    if rows < 36 {
        return;
    }
    let mut y = 16;
    if let Some(ro) = sim.orbit_readout() {
        let e = &ro.elements;
        let alt = |d: f64| fmt_dist((d - ro.radius).max(0.0));
        let apo = e.apoapsis.map_or("OUVERTE".to_string(), alt);
        fb.text(
            1,
            y,
            &format!(
                "ORBITE {}  PERI {}  APO {}",
                sim.bodies[ro.body].name.to_uppercase(),
                alt(e.periapsis),
                apo
            ),
            if e.periapsis < ro.radius { RED } else { GREEN },
        );
        y += 1;
        let period = e.period.map_or("--".to_string(), fmt_time);
        fb.text(
            1,
            y,
            &format!(
                "V {:.0} M/S  CIRC {:.0}  LIBER. {:.0}  E {:.3}  P {}",
                e.speed, e.v_circ, e.v_esc, e.e, period
            ),
            DIM,
        );
        y += 1;
    }
    if let Some(t) = sim.target.and_then(|t| sim.transfer_to(t)) {
        fb.text(
            1,
            y,
            &format!(
                "HOHMANN DV {:.0} M/S  VOL {}  FENETRE DANS {}",
                t.hohmann.total,
                fmt_time(t.hohmann.time),
                fmt_time(t.wait)
            ),
            AMBER,
        );
        y += 1;
    }
    let ship = &sim.ship;
    let sun = (sim.bodies[0].pos - ship.pos).length();
    fb.text(
        1,
        y,
        &format!(
            "FLUX SOLAIRE {:.2} KW/M2  LUMIERE ETOILE {}  G {:.1}",
            nav::SOLAR_CONSTANT_KW * sim.solar_flux(),
            fmt_time(nav::light_delay(sun)),
            sim.structural_load_g()
        ),
        DIM,
    );
    y += 1;
    let rate = sim.dose_rate_aboard();
    fb.text(
        1,
        y,
        &format!(
            "DOSE {:.2} MGY/H  CUMULEE {:.1} MGY  SIGNATURE IR {:.0} KW",
            rate * 1e3,
            ship.sys.dose_gy * 1e3,
            sim.ir_signature_kw()
        ),
        if rate > 0.05 { RED } else { DIM },
    );
    for (k, (line, alert)) in sim.exotic_hud().into_iter().enumerate() {
        fb.text(1, y + 2 + k, &line, if alert { RED } else { AMBER });
    }
    let beta = sim.rel_velocity().length() / nav::C;
    if beta > 0.001 {
        fb.text(
            1,
            y + 1,
            &format!(
                "RELATIVISTE  V {:.3} C  GAMMA {:.4}",
                beta,
                nav::gamma(beta * nav::C)
            ),
            AMBER,
        );
    }
}

fn draw_target_panel(fb: &mut Framebuffer, v: &View, rows: usize, cols: usize) {
    let sim = v.sim;
    let ship = &sim.ship;
    let x = cols.saturating_sub(40);
    if let Some(n) = sim.target_npc.and_then(|k| sim.npcs.get(k)) {
        let r = n.pos - ship.pos;
        let closing = (ship.vel - n.vel).dot(r.normalized());
        let faction = n.faction.map_or("INDEPENDANT".to_string(), |f| {
            sim.data.factions[f].name.to_uppercase()
        });
        let status = match &n.state {
            vector_sim::npc::NpcState::Docked { .. } => "AMARRE / EN ATTENTE",
            vector_sim::npc::NpcState::Travel { .. } => "EN TRANSIT",
            vector_sim::npc::NpcState::Working { .. } => "AU TRAVAIL",
            vector_sim::npc::NpcState::Lurk { .. } => "EN EMBUSCADE",
            vector_sim::npc::NpcState::Orbit { .. } => "EN PATROUILLE",
            vector_sim::npc::NpcState::Away { .. } => "ABSENT",
            vector_sim::npc::NpcState::Combat { .. } => {
                if n.disabled {
                    "NEUTRALISE (ABORDABLE)"
                } else if n.hostile {
                    "HOSTILE !"
                } else {
                    "EN ALERTE"
                }
            }
        };
        fb.text(x, rows - 8, &format!("CIBLE {}", n.name), AMBER);
        fb.text(
            x,
            rows - 7,
            &format!("TYPE  {}  {}", n.role.label(), faction),
            AMBER,
        );
        fb.text(
            x,
            rows - 6,
            &format!("DIST  {}", fmt_dist(r.length())),
            AMBER,
        );
        fb.text(x, rows - 5, &format!("APPR  {closing:+.1} M/S"), AMBER);
        fb.text(x, rows - 4, status, if n.hostile { RED } else { DIM });
        fb.text(
            x,
            rows - 2,
            &format!(
                "COQUE {:.0}%  BOUCLIER {:.0}%",
                100.0 * n.hull / n.hull_max.max(1.0),
                100.0 * n.shield / n.shield_max.max(1.0)
            ),
            if n.hostile { RED } else { DIM },
        );
        if let Some((res, kg)) = n.cargo {
            fb.text(
                x,
                rows - 3,
                &format!(
                    "SOUTE {:.0} KG {}",
                    kg,
                    sim.data.resources[res].name.to_uppercase()
                ),
                DIM,
            );
        }
        return;
    }
    let Some(i) = sim.target else {
        fb.text(x, rows - 2, "AUCUNE CIBLE [TAB]", DIM);
        return;
    };
    let t = &sim.bodies[i];
    let cb = &sim.system.bodies[i];
    let level = sim.scan[i];
    let r = t.pos - ship.pos;
    let range = r.length() - t.radius;
    let closing = (ship.vel - t.vel).dot(r.normalized());
    fb.text(x, rows - 8, &format!("CIBLE {}", body_label(v, i)), AMBER);
    fb.text(x, rows - 6, &format!("DIST  {}", fmt_dist(range)), AMBER);
    fb.text(x, rows - 5, &format!("APPR  {:+.1} M/S", closing), AMBER);
    if level < SCAN_SURVEYED {
        fb.text(x, rows - 7, "TYPE  NON IDENTIFIE", AMBER);
        fb.text(x, rows - 3, "APPROCHEZ POUR SCANNER", DIM);
        return;
    }
    fb.text(
        x,
        rows - 7,
        &format!("TYPE  {}", kind_label(cb.kind)),
        AMBER,
    );
    if let BodyKind::Star(_) = cb.kind {
        // What the star is, and what it does to whoever comes close.
        let sys = &sim.system;
        let what = if i == 0 {
            sys.stage.label().to_string()
        } else {
            "ETOILE COMPAGNE".to_string()
        };
        let extra = match (i, sys.variable(), sys.pulsar()) {
            (0, Some(var), _) => format!(
                "  VARIABLE P {:.0} J +/-{:.0} %",
                var.period_days,
                var.amplitude * 100.0
            ),
            (0, _, Some(p)) => format!(
                "  {} P {:.3} S",
                if p.magnetar { "MAGNETAR" } else { "PULSAR" },
                p.period_s
            ),
            (0, _, _) if sys.accreting() => "  DISQUE D'ACCRETION".to_string(),
            _ => String::new(),
        };
        fb.text(x, rows - 4, &format!("{what}{extra}"), DIM);
        fb.text(
            x,
            rows - 3,
            &format!(
                "RAYON {}  T {:.0} K  {:.2} MSOL",
                fmt_dist(t.radius),
                cb.temp,
                cb.mass / vector_world::orbit::M_SUN
            ),
            DIM,
        );
        let dose = if i == 0 {
            sys.star_radiation(range.max(1.0e6))
        } else {
            0.0
        };
        let gain = sys.lens_gain(r.length());
        fb.text(
            x,
            rows - 2,
            &format!(
                "DOSE ICI {dose:.2} GY/H  LENTILLE x{gain:.1}  {} COMPAGNONS",
                sys.companions().len()
            ),
            if dose > 0.5 { RED } else { DIM },
        );
        return;
    }
    if let BodyKind::Anomaly(a) = cb.kind {
        let def = sim.data.anomaly_def(a.id());
        let tail = sim
            .system
            .comet_tail(i, sim.time)
            .map_or(String::new(), |t| {
                format!("  QUEUE {}", fmt_dist(t.length()))
            });
        fb.text(
            x,
            rows - 4,
            &format!(
                "RAYON {}  ZONE {}{tail}",
                fmt_dist(t.radius),
                fmt_dist(def.map_or(0.0, |d| d.radius))
            ),
            DIM,
        );
        let text = def.map_or(String::new(), |d| d.text.to_uppercase());
        fb.text(x, rows - 3, &text.chars().take(38).collect::<String>(), DIM);
        fb.text(
            x,
            rows - 2,
            if sim.explored.get(i).copied().unwrap_or(false) {
                "EXPLOREE"
            } else {
                "SCAN COMPLET = RECOMPENSES"
            },
            DIM,
        );
        return;
    }
    fb.text(
        x,
        rows - 4,
        &format!("RAYON {}  G {:.1}", fmt_dist(t.radius), cb.gravity),
        DIM,
    );
    fb.text(
        x,
        rows - 3,
        &format!("TEMP  {:.0} K  PRES {:.2} BAR", cb.temp, cb.pressure),
        DIM,
    );
    if level < SCAN_FULL {
        fb.text(x, rows - 2, "SCAN COMPLET: PLUS PRES", DIM);
        return;
    }
    if let BodyKind::Asteroid(class) = cb.kind {
        let range_laser = ship
            .design
            .modules(&sim.data)
            .map(|(_, m)| m.mining_range)
            .fold(0.0, f64::max);
        let comp: Vec<String> = sim
            .data
            .asteroids
            .iter()
            .find(|a| a.id == class.id())
            .map(|a| {
                let mut c = a.composition.clone();
                c.sort_by(|x, y| y.1.total_cmp(&x.1));
                c.iter()
                    .take(3)
                    .map(|(r, f)| format!("{r} {:.0}%", f * 100.0))
                    .collect()
            })
            .unwrap_or_default();
        fb.text(
            x,
            rows - 2,
            &format!(
                "MASSE {:.0} T  {}  LASER<{}",
                sim.remaining_mass(i) / 1e3,
                comp.join(" "),
                fmt_dist(range_laser)
            ),
            DIM,
        );
    } else if !cb.atmosphere.is_empty() {
        let a: Vec<String> = cb
            .atmosphere
            .iter()
            .map(|(g, f)| format!("{g} {:.0}%", f * 100.0))
            .collect();
        fb.text(x, rows - 2, &a.join(" "), DIM);
    }
}

/// Name shown for a body, depending on how well it is known.
pub fn body_label<'a>(v: &'a View, i: usize) -> &'a str {
    if v.sim.scan[i] >= SCAN_SURVEYED {
        &v.sim.bodies[i].name
    } else {
        "OBJET INCONNU"
    }
}

pub fn kind_label(k: BodyKind) -> String {
    match k {
        BodyKind::Star(vector_world::StarClass::D) => "NAINE BLANCHE".into(),
        BodyKind::Star(vector_world::StarClass::N) => "ETOILE A NEUTRONS".into(),
        BodyKind::Star(vector_world::StarClass::X) => "TROU NOIR STELLAIRE".into(),
        BodyKind::Star(vector_world::StarClass::R) => "PLANETE ERRANTE".into(),
        BodyKind::Star(c) => format!("ETOILE {}", c.letter()),
        BodyKind::Planet(p) => format!("PLANETE {}", p.label()),
        BodyKind::Moon(p) => format!("LUNE {}", p.label()),
        BodyKind::Asteroid(t) => format!("ASTEROIDE TYPE {}", t.id()),
        BodyKind::Anomaly(a) => format!("ANOMALIE : {}", a.label().to_uppercase()),
        BodyKind::Station => "STATION".into(),
    }
}

fn draw_radar(fb: &mut Framebuffer, v: &View) {
    let ship = &v.sim.ship;
    let (cx, cy) = (fb.w as f32 / 2.0, fb.h as f32 - 44.0);
    let (rx, ry) = (80.0f32, 26.0f32);
    let range = 20_000.0f64;
    for k in [1.0, 0.5] {
        fb.ellipse(cx, cy, rx * k, ry * k, DIM);
    }
    fb.line(cx - rx, cy, cx + rx, cy, DIM);
    fb.line(cx, cy - ry, cx, cy + ry, DIM);
    radar_npcs(fb, v, cx, cy, rx, ry, range);
    let inv = ship.rot.transpose();
    for (i, b) in v.sim.bodies.iter().enumerate() {
        if matches!(b.kind, BodyKind::Star(_)) || v.sim.scan[i] == 0 {
            continue;
        }
        let l = inv.mul_vec(b.pos - ship.pos);
        let flat = (l.x * l.x + l.z * l.z).sqrt();
        let scale = if flat > range { range / flat } else { 1.0 };
        let px = cx + (l.x * scale / range) as f32 * rx;
        let py = cy - (l.z * scale / range) as f32 * ry;
        let top = py - (l.y * scale / range) as f32 * ry * 1.5;
        let col = marker_color(v, i);
        fb.line(px, py, px, top, col);
        fb.plot(px as i32, top as i32, col);
        fb.plot(px as i32 + 1, top as i32, col);
    }
}

fn radar_npcs(fb: &mut Framebuffer, v: &View, cx: f32, cy: f32, rx: f32, ry: f32, range: f64) {
    let ship = &v.sim.ship;
    let inv = ship.rot.transpose();
    for (k, n) in v.sim.npcs.iter().enumerate() {
        if !n.visible() {
            continue;
        }
        let l = inv.mul_vec(n.pos - ship.pos);
        let flat = (l.x * l.x + l.z * l.z).sqrt();
        if flat > range * 3.0 {
            continue;
        }
        let scale = if flat > range { range / flat } else { 1.0 };
        let px = cx + (l.x * scale / range) as f32 * rx;
        let py = cy - (l.z * scale / range) as f32 * ry;
        let col = if Some(k) == v.sim.target_npc {
            AMBER
        } else {
            npc_color(n.role)
        };
        fb.plot(px as i32, py as i32, col);
        fb.plot(px as i32 + 1, py as i32, col);
        fb.plot(px as i32, py as i32 + 1, col);
    }
}

pub fn draw_console(fb: &mut Framebuffer, c: &Console) {
    if !c.open {
        return;
    }
    let lines = c.log.len() + 1;
    for y in 0..(lines * 10 + 4) {
        for x in 0..fb.w {
            fb.plot(x as i32, y as i32, 0x0000_0800);
        }
    }
    for (i, l) in c.log.iter().enumerate() {
        fb.text(1, i, l, DIM);
    }
    fb.text(1, c.log.len(), &format!("> {}_", c.line), GREEN);
}
