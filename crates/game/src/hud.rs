//! Cockpit view: 3D scene + ASCII HUD.
use crate::console::Console;
use vector_core::{Mat3, Vec3};
use vector_render::{Camera, Framebuffer, Wireframe};
use vector_sim::{BodyKind, Sim, DRY_MASS, FUEL_CAP, G0, HULL_MAX, ISP};

pub const GREEN: u32 = 0x0030_E050;
pub const DIM: u32 = 0x0010_6020;
pub const AMBER: u32 = 0x00E0_A020;
pub const RED: u32 = 0x00FF_3030;
pub const STAR: u32 = 0x0040_5060;

pub struct View<'a> {
    pub sim: &'a Sim,
    pub stars: &'a [Vec3],
    pub target: Option<usize>,
    pub timescale: f64,
    pub paused: bool,
    pub messages: &'a [String],
    pub console: &'a Console,
}

pub fn fmt_dist(m: f64) -> String {
    let a = m.abs();
    if a < 10_000.0 {
        format!("{m:.0} m")
    } else if a < 1.0e9 {
        format!("{:.1} km", m / 1e3)
    } else {
        format!("{:.3} Gm", m / 1e9)
    }
}

pub fn draw(fb: &mut Framebuffer, v: &View) {
    fb.clear();
    let ship = &v.sim.ship;
    let cam = Camera {
        pos: ship.pos,
        rot: ship.rot.transpose(),
        fov_y: 1.2,
    };

    for s in v.stars {
        if let Some((x, y)) = cam.project_dir(*s, fb) {
            fb.plot(x as i32, y as i32, STAR);
        }
    }
    let spin = Mat3::rot_y(v.sim.time * 0.05);
    for (i, b) in v.sim.bodies.iter().enumerate() {
        let col = if Some(i) == v.target { AMBER } else { GREEN };
        match b.kind {
            BodyKind::Star => cam.draw_sphere(fb, b.pos, b.radius, &spin, 4, 8, 0x00E0_D060),
            BodyKind::Planet => cam.draw_sphere(fb, b.pos, b.radius, &spin, 8, 14, col),
            BodyKind::Moon => cam.draw_sphere(fb, b.pos, b.radius, &spin, 6, 10, col),
            BodyKind::Asteroid => cam.draw_sphere(fb, b.pos, b.radius, &spin, 4, 6, col),
            BodyKind::Station => {
                let r = Mat3::rot_y(v.sim.time * 0.2);
                Wireframe::cube(b.radius * 0.5).draw(&cam, fb, &r, b.pos, col);
                let ring = Mat3::rot_x(std::f64::consts::FRAC_PI_2).mul(&r);
                cam.draw_sphere(fb, b.pos, b.radius * 0.9, &ring, 2, 0, col);
            }
        }
    }

    let (w, h) = (fb.w as f32, fb.h as f32);
    let (cx, cy) = (w / 2.0, h / 2.0);
    // Crosshair.
    for (dx, dy) in [(-14.0, 0.0), (8.0, 0.0), (0.0, -14.0), (0.0, 8.0)] {
        fb.line(
            cx + dx,
            cy + dy,
            cx + dx + if dx == 0.0 { 0.0 } else { 6.0 },
            cy + dy + if dy == 0.0 { 0.0 } else { 6.0 },
            GREEN,
        );
    }
    // Prograde marker.
    let speed = ship.vel.length();
    if speed > 0.5 {
        if let Some((x, y)) = cam.project_dir(ship.vel.normalized(), fb) {
            fb.circle(x, y, 6.0, GREEN);
            fb.line(x - 10.0, y, x - 6.0, y, GREEN);
            fb.line(x + 6.0, y, x + 10.0, y, GREEN);
            fb.line(x, y - 10.0, x, y - 6.0, GREEN);
        }
    }
    // Target brackets.
    if let Some(t) = v.target.and_then(|i| v.sim.bodies.get(i)) {
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

    draw_radar(fb, v);

    let rows = fb.rows();
    let cols = fb.cols();
    fb.text(1, 1, &format!("VITESSE {:>10}/s", fmt_dist(speed)), GREEN);
    fb.text(
        1,
        2,
        &format!(
            "ASSISTANCE {}   TEMPS x{}{}",
            if ship.assist { "ON " } else { "OFF" },
            v.timescale,
            if v.paused { " PAUSE" } else { "" }
        ),
        if ship.assist { GREEN } else { AMBER },
    );
    fb.text(1, 3, &format!("T+{:.0}s", v.sim.time), DIM);
    let dv = ISP * G0 * ((DRY_MASS + ship.fuel) / DRY_MASS).ln();
    fb.gauge(
        1,
        rows - 4,
        "CARBURANT",
        ship.fuel / FUEL_CAP,
        20,
        if ship.fuel < FUEL_CAP * 0.1 {
            RED
        } else {
            GREEN
        },
    );
    fb.gauge(
        1,
        rows - 3,
        "COQUE    ",
        ship.hull / HULL_MAX,
        20,
        if ship.hull < HULL_MAX * 0.3 {
            RED
        } else {
            GREEN
        },
    );
    fb.text(1, rows - 2, &format!("DELTA-V {:.1} KM/S", dv / 1e3), DIM);

    // Target panel.
    if let Some(t) = v.target.and_then(|i| v.sim.bodies.get(i)) {
        let rel = t.pos - ship.pos;
        let range = rel.length() - t.radius;
        let closing = ship.vel.dot(rel.normalized());
        let x = cols.saturating_sub(34);
        fb.text(x, rows - 5, &format!("CIBLE {}", t.name), AMBER);
        fb.text(x, rows - 4, &format!("DIST  {}", fmt_dist(range)), AMBER);
        fb.text(x, rows - 3, &format!("APPR  {:+.1} M/S", closing), AMBER);
        fb.text(x, rows - 2, &format!("RAYON {}", fmt_dist(t.radius)), DIM);
    } else {
        fb.text(cols.saturating_sub(34), rows - 2, "AUCUNE CIBLE [TAB]", DIM);
    }
    // Messages.
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
    draw_console(fb, v.console);
}

fn draw_radar(fb: &mut Framebuffer, v: &View) {
    let ship = &v.sim.ship;
    let (cx, cy) = (fb.w as f32 / 2.0, fb.h as f32 - 34.0);
    let (rx, ry) = (80.0f32, 26.0f32);
    let range = 20_000.0f64;
    fb.circle(cx, cy, 1.0, DIM);
    for k in [1.0, 0.5] {
        ellipse(fb, cx, cy, rx * k, ry * k, DIM);
    }
    fb.line(cx - rx, cy, cx + rx, cy, DIM);
    fb.line(cx, cy - ry, cx, cy + ry, DIM);
    let inv = ship.rot.transpose();
    for (i, b) in v.sim.bodies.iter().enumerate() {
        if matches!(b.kind, BodyKind::Star) {
            continue;
        }
        let l = inv.mul_vec(b.pos - ship.pos);
        let flat = (l.x * l.x + l.z * l.z).sqrt();
        let scale = if flat > range { range / flat } else { 1.0 };
        let (lx, lz, ly) = (l.x * scale, l.z * scale, l.y * scale);
        let px = cx + (lx / range) as f32 * rx;
        let py = cy - (lz / range) as f32 * ry;
        let top = py - (ly / range) as f32 * ry * 1.5;
        let col = if Some(i) == v.target { AMBER } else { GREEN };
        fb.line(px, py, px, top, col);
        fb.plot(px as i32, top as i32, col);
        fb.plot(px as i32 + 1, top as i32, col);
    }
}

fn ellipse(fb: &mut Framebuffer, cx: f32, cy: f32, rx: f32, ry: f32, c: u32) {
    let mut prev = (cx + rx, cy);
    for i in 1..=48 {
        let a = i as f32 / 48.0 * std::f32::consts::TAU;
        let p = (cx + rx * a.cos(), cy + ry * a.sin());
        fb.line(prev.0, prev.1, p.0, p.1, c);
        prev = p;
    }
}

fn draw_console(fb: &mut Framebuffer, c: &Console) {
    if !c.open {
        return;
    }
    let lines = c.log.len() + 1;
    for y in 0..(lines * 8 + 4) {
        for x in 0..fb.w {
            fb.plot(x as i32, y as i32, 0x0000_0800);
        }
    }
    for (i, l) in c.log.iter().enumerate() {
        fb.text(1, i, l, DIM);
    }
    fb.text(1, c.log.len(), &format!("> {}_", c.line), GREEN);
}
