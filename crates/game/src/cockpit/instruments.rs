//! The instruments of the console: artificial horizon, heading tape, dials, LED gauges, radar,
//! the screens (navigation, descent, docking, plan, target, pilot) and the alert lamps. Each one
//! is drawn in the rectangle it is given and falls back to a simpler face when the rectangle is
//! small, so the arrangements can stretch or shrink them.
use super::controls::{display, organ, pushbtn};
use super::draw::*;
use super::{Id, Tel};
use crate::game::Game;
use crate::mouse::Act;
use std::sync::OnceLock;
use vector_core::Vec3;
use vector_render::{mix_color, scale_color, Framebuffer};
use vector_sim::landing::PilotKind;
use winit::keyboard::KeyCode;

// ---- LED gauges --------------------------------------------------------------------------------

/// A vertical LED bar: small segments coloured by zone (red low / amber / green; reversed for
/// what is bad when high).
#[allow(clippy::too_many_arguments)]
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
            mix_color(DARK, c, 0.12)
        };
        fill(fb, x, yy, w, seg - 1, col);
        if on {
            hl(fb, x, yy, w, mix_color(col, 0x00FF_FFFF, 0.35));
        }
    }
}

/// A horizontal LED bar with its label (`lw` pixels wide) on the left and value on the right.
#[allow(clippy::too_many_arguments)]
fn led_bar_h(
    fb: &mut Framebuffer,
    x: i32,
    y: i32,
    w: i32,
    lw: i32,
    vw: i32,
    f: f64,
    high_bad: bool,
    label: &str,
    value: &str,
    alarm: bool,
    blink: bool,
) {
    txt(fb, x, y + 1, label, if alarm { RED_LED } else { LABEL });
    let bx = x + lw;
    let bw = w - lw - vw;
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
            mix_color(DARK, c, 0.12)
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

/// One horizontal gauge of the folded strip (arrangement C): label, bar and value.
#[allow(clippy::too_many_arguments)]
pub fn strip_bar(
    fb: &mut Framebuffer,
    g: &Game,
    tel: &Tel,
    at: (i32, i32, i32),
    label: &str,
    idx: usize,
    high_bad: bool,
    value: &str,
    alarm: bool,
) {
    let (x, y, w) = at;
    led_bar_h(
        fb,
        x,
        y,
        w,
        36,
        40,
        tel.bars[idx],
        high_bad,
        label,
        value,
        alarm,
        tel.blink,
    );
    display(g, (x - 2, y - 2, w + 4, 14), Id::Systems, idx);
}

/// Name, short name, high is bad, and the figures of the eight systems gauges.
pub const BARS: [(&str, &str, bool); 8] = [
    ("CARBURANT", "CARB", false),
    ("DELTA-V", "DV", false),
    ("BATTERIE", "BATT", false),
    ("COQUE", "COQ", false),
    ("BOUCLIER", "BOUC", false),
    ("TEMPERAT.", "TEMP", true),
    ("RADIATION", "RAD", true),
    ("CHARGE G", "G", true),
];

/// The figures of the gauges: (value text, alarm).
pub fn bar_values(tel: &Tel) -> [(String, bool); 8] {
    [
        (format!("{:.0}%", tel.fuel * 100.0), tel.fuel < 0.25),
        (format!("{:.1}K", tel.dv / 1e3), tel.dv_frac < 0.2),
        (format!("{:.0}%", tel.batt * 100.0), tel.batt < 0.25),
        (format!("{:.0}%", tel.hull * 100.0), tel.hull < 0.5),
        (
            if !tel.shield_installed {
                "--".to_string()
            } else if !tel.shield_on {
                "OFF".to_string()
            } else {
                tel.shield
                    .map_or("--".into(), |s| format!("{:.0}%", s * 100.0))
            },
            tel.shield_on && tel.shield.is_some_and(|s| s < 0.25),
        ),
        (format!("{:.0}K", tel.temp_k), tel.temp_k > 500.0),
        (format!("{:.2}", tel.dose_mgy), tel.dose_frac > 0.5),
        (format!("{:.1}G", tel.gload), tel.g_frac > 0.5),
    ]
}

/// The eight systems gauges in the rectangle: LED columns with the name and value running up
/// beside them when there is room, else bars in rows.
pub fn systems(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    plate(fb, r, "SYSTEMES");
    let vals = bar_values(tel);
    let vertical = w >= 150 && h >= 120;
    if vertical {
        let pitch = (w - 12) / 8;
        let bar_w = if pitch >= 19 { 9 } else { 7 };
        let (top, bh) = (y + 22, h - 30);
        let long = bh >= 130 && pitch >= 19;
        for (k, (name, short, high_bad)) in BARS.iter().enumerate() {
            let bx = x + 9 + k as i32 * pitch;
            let (value, alarm) = &vals[k];
            led_bar_v(
                fb,
                bx,
                top,
                bar_w,
                bh,
                tel.bars[k],
                *high_bad,
                *alarm,
                tel.blink,
            );
            let base = top + bh - 1;
            let lc = if *alarm { RED_LED } else { LABEL };
            let label = if long { *name } else { *short };
            txt_v(fb, bx + bar_w + 2, base, label, lc);
            let vc = if *alarm && tel.blink { RED_LED } else { INK };
            txt_v(
                fb,
                bx + bar_w + 2,
                base - (label.chars().count() as i32 + 1) * 8,
                value,
                vc,
            );
            display(g, (bx - 2, top - 2, pitch - 1, bh + 4), Id::Systems, k);
        }
    } else {
        let step = ((h - 24) / 8).clamp(11, 16);
        let lw = 40;
        let vw = 40;
        for (k, (_, short, high_bad)) in BARS.iter().enumerate() {
            let (value, alarm) = &vals[k];
            let yy = y + 19 + k as i32 * step;
            led_bar_h(
                fb,
                x + 8,
                yy,
                w - 16,
                lw,
                vw,
                tel.bars[k],
                *high_bad,
                short,
                value,
                *alarm,
                tel.blink,
            );
            display(g, (x + 6, yy - 1, w - 12, step - 1), Id::Systems, k);
        }
    }
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

fn nice_max(v: f64) -> f64 {
    let ladder = [
        100.0, 200.0, 400.0, 800.0, 2e3, 4e3, 8e3, 2e4, 4e4, 8e4, 2e5, 4e5, 8e5, 2e6, 2e7, 2e8,
    ];
    let t = v * 1.1;
    ladder.into_iter().find(|&l| l >= t).unwrap_or(1e9)
}

pub fn scale_for(v: f64) -> f64 {
    nice_max(v)
}

/// The speed dial: ground, orbit or target speed, with its three-way selector (when `sel`).
pub fn speed_dial(fb: &mut Framebuffer, g: &Game, tel: &Tel, cx: i32, cy: i32, r: i32, sel: bool) {
    let mode = tel.spd_mode;
    let v = tel.spd;
    let top = nice_max(tel.spd.max(match mode {
        0 => tel.spd_surf.unwrap_or(0.0),
        2 => tel.spd_tgt.unwrap_or(0.0),
        _ => tel.spd_orb,
    }));
    let (a0, a1) = (std::f32::consts::PI * 0.75, std::f32::consts::PI * 2.25);
    bezel(fb, cx, cy, r);
    for i in 0..=20 {
        let a = a0 + (a1 - a0) * i as f32 / 20.0;
        let big_tick = i % 5 == 0;
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
        // A small dial keeps the labels of its ends and its middle: no clutter.
        if big_tick && r >= 30 && (r >= 44 || i % 10 == 0) {
            let (lx, ly) = pt(cx, cy, r as f32 - 16.0, a);
            txt_c(
                fb,
                lx,
                ly - 3,
                &short(top * f64::from(i / 5) / 4.0),
                0x00C8_D4DC,
            );
        }
    }
    // A red zone over the last tenth.
    arc(fb, cx, cy, r - 1, a1 - (a1 - a0) * 0.1, a1, 2, 0x00C0_2020);
    let f = (v / top).clamp(0.0, 1.0) as f32;
    needle(fb, cx, cy, r as f32 - 7.0, a0 + (a1 - a0) * f, 0x00FF_8A2A);
    let name = ["SOL", "ORB", "CIB"][mode.min(2)];
    if r >= 44 {
        txt_c(fb, cx, cy + r / 2 - 8, &format!("{} M/S", short(v)), INK);
    } else {
        txt_c(fb, cx, cy + r / 2 + 2, &short(v), INK);
    }
    // The dial itself cycles the mode; the selector below chooses it.
    organ(
        g,
        (cx - r, cy - r, 2 * r, 2 * r),
        Act::Call(Game::cockpit_speed_mode, (mode + 1) % 3),
        None,
        Id::SpeedDial,
        mode,
    );
    if sel {
        for (k, (l, ok)) in [
            ("SOL", tel.spd_surf.is_some()),
            ("ORB", true),
            ("CIB", tel.spd_tgt.is_some()),
        ]
        .iter()
        .enumerate()
        {
            let x = cx - 35 + k as i32 * 24;
            let y = cy + r + 9;
            let on = k == mode;
            let hov = organ(
                g,
                (x, y, 23, 11),
                Act::Call(Game::cockpit_speed_mode, k),
                None,
                Id::SpeedSel,
                k,
            );
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
    } else {
        txt_c(fb, cx, cy + r + 8, name, LABEL);
    }
}

/// The vertical speed dial (zero at nine o'clock like an airliner's VSI).
pub fn vario_dial(fb: &mut Framebuffer, g: &Game, tel: &Tel, cx: i32, cy: i32, r: i32) {
    let vv = tel.vv_s;
    let top = if vv.abs() < 10.0 {
        10.0
    } else if vv.abs() < 50.0 {
        50.0
    } else if vv.abs() < 200.0 {
        200.0
    } else {
        1000.0
    };
    bezel(fb, cx, cy, r);
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
        if major && r >= 30 && (r >= 44 || i % 10 == 0) {
            let (lx, ly) = pt(cx, cy, r as f32 - 17.0, a);
            txt_c(
                fb,
                lx,
                ly - 3,
                &format!("{:.0}", (top * f64::from(f.abs())).round()),
                0x00C8_D4DC,
            );
        }
    }
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
    display(g, (cx - r, cy - r, 2 * r, 2 * r), Id::Vario, 0);
}

/// The sky and ground colours seen through the horizon ball, by sine of elevation.
fn horizon_lut() -> &'static [u32; 256] {
    static T: OnceLock<[u32; 256]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, c) in t.iter_mut().enumerate() {
            let e = i as f32 / 127.5 - 1.0;
            let el = e.clamp(-1.0, 1.0).asin();
            *c = if e.abs() < 0.012 {
                0x00F8_FCFF
            } else if e > 0.0 {
                mix_color(0x0058_A0E8, 0x0010_3C86, (el / 0.9).clamp(0.0, 1.0))
            } else {
                mix_color(0x009A_6B3A, 0x0038_2210, (-el / 0.9).clamp(0.0, 1.0))
            };
        }
        t
    })
}

/// A line clipped to a disc.
#[allow(clippy::too_many_arguments)]
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

/// Artificial horizon: sky and ground from the real attitude of the ship against the local
/// horizon (of the world below, or of the plane of the system), pitch ladder, bank scale.
pub fn attitude(fb: &mut Framebuffer, g: &Game, tel: &Tel, cx: i32, cy: i32, r: i32) {
    bezel(fb, cx, cy, r);
    let k = r as f32 / 0.62; // pixels per radian: the ball shows +-35 degrees
    let (ux, uy, sp) = (tel.ux, tel.uy, tel.sinp);
    let lut = horizon_lut();
    let (ex, ey) = (ux / k, uy / k);
    for dy in -r..=r {
        let yy = cy + dy;
        if yy < 0 || yy >= fb.h as i32 {
            continue;
        }
        let w = (((r * r - dy * dy) as f32).sqrt() as i32).max(0);
        let (x0, x1) = ((cx - w).max(0), (cx + w + 1).min(fb.w as i32));
        if x1 <= x0 {
            continue;
        }
        let e0 = sp + (-dy as f32) * ey + (x0 - cx) as f32 * ex;
        let row = yy as usize * fb.w;
        let mut e = e0;
        for x in x0..x1 {
            let i = ((e + 1.0) * 127.5).clamp(0.0, 255.0) as usize;
            fb.px[row + x as usize] = lut[i];
            e += ex;
        }
    }
    // Pitch ladder: lines at -30..30 degrees.
    let n = ux.hypot(uy).max(1e-4);
    let (nx, ny) = (ux / n, uy / n); // "up" of the world in the aircraft frame
    for deg in [-30i32, -20, -10, 10, 20, 30] {
        let s = (deg as f32).to_radians().sin();
        let off = k * (s - sp) / n.max(0.2);
        let (mx, my) = (cx as f32 + nx * off, cy as f32 - ny * off);
        let half = if deg % 20 == 0 { 12.0 } else { 20.0 } * (r as f32 / 70.0).clamp(0.6, 1.4);
        let (dxl, dyl) = (-ny * half, -nx * half);
        let (x0, y0) = ((mx - dxl) as i32, (my + dyl) as i32);
        let (x1, y1) = ((mx + dxl) as i32, (my - dyl) as i32);
        clipped_line(fb, cx, cy, r - 3, x0, y0, x1, y1, 0x00F0_F6FF);
        let (tx, ty) = (x1 + 4, y1 - 3);
        if r >= 40 && ((tx - cx).pow(2) + (ty - cy).pow(2)) < (r - 10) * (r - 10) {
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
    let wing = (r as f32 * 0.5) as i32;
    let gap = (r as f32 * 0.17) as i32;
    for d in -1..=1 {
        line_over(fb, cx - wing, cy + d, cx - gap, cy + d, 0x00FF_A020);
        line_over(fb, cx + gap, cy + d, cx + wing, cy + d, 0x00FF_A020);
        vl(fb, cx - gap + d, cy, 6, 0x00FF_A020);
        vl(fb, cx + gap + d, cy, 6, 0x00FF_A020);
    }
    disc(fb, cx, cy, 3, 0x00FF_A020);
    ring(fb, cx, cy, r, 0x0010_141A);
    display(g, (cx - r, cy - r, 2 * r, 2 * r), Id::Attitude, 0);
}

/// The heading tape: a window on the compass.
pub fn heading_tape(fb: &mut Framebuffer, g: &Game, r: Rc, hdg: f32) {
    let (x, y, w, h) = r;
    well(fb, r, SCREEN_BG);
    let ppd = w as f32 / 110.0;
    let cx = x + w / 2;
    let hr = hdg.round() as i32;
    for d in -60..=60 {
        let deg = (hr + d).rem_euclid(360);
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
    display(g, r, Id::Tape, 0);
}

/// Three digital cells: radar altitude, vertical and horizontal speed.
pub fn digital_cells(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
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
    let cw = w / 3;
    for (k, (l, v, alarm)) in cells.iter().enumerate() {
        let bx = x + k as i32 * cw;
        well(fb, (bx, y, cw - 4, h), SCREEN_BG);
        txt(fb, bx + 4, y + 2, l, LABEL);
        let s = v.clone().unwrap_or_else(|| "--".to_string());
        txt_r(
            fb,
            bx + cw - 8,
            y + h - 10,
            &s,
            if *alarm && tel.blink {
                RED_LED
            } else {
                GREEN_LED
            },
        );
        display(g, (bx, y, cw - 4, h), Id::Digital, k);
    }
}

/// The flight instruments in one block: heading tape, three dials around the attitude ball,
/// annunciators and a digital strip, as much of it as the rectangle can hold.
pub fn flight_cluster(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    plate(fb, r, "");
    let mut top = y + 8;
    if h >= 170 {
        heading_tape(fb, g, (x + 14, top, w - 28, 22), tel.heading);
        top += 30;
    }
    let strip = h >= 150;
    let annun = h >= 250 && w >= 300;
    let bottom = y + h - if strip { 30 } else { 6 } - if annun { 22 } else { 0 };
    let avail = bottom - top;
    let rb = ((avail - 8) / 2).min((w - 56) / 5).clamp(28, 112);
    let cy = top + avail / 2 - 2;
    let cx = x + w / 2;
    let rd = (rb * 3 / 4).clamp(24, 84);
    let dx = rb + rd + 14;
    let sel = rd >= 30 && avail >= 2 * rd + 26;
    attitude(fb, g, tel, cx, cy, rb);
    speed_dial(fb, g, tel, cx - dx, cy - i32::from(sel) * 4, rd, sel);
    vario_dial(fb, g, tel, cx + dx, cy - i32::from(sel) * 4, rd);
    if annun {
        let ay = y + h - 30 - 20;
        let mut ax = x + 10;
        let sas_l = format!("SAS {}", cut(tel.sas.label(), 12));
        for (l, on, c) in [
            ("ASSIST", tel.assist, GREEN_LED),
            (
                sas_l.as_str(),
                tel.sas != vector_sim::landing::Sas::Off,
                GREEN_LED,
            ),
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
                    mix_color(DARK, c, 0.55)
                } else {
                    0x0010_1418
                },
            );
            frame(fb, ax, ay, wpx, 13, if on { c } else { 0x0030_3844 });
            txt(fb, ax + 6, ay + 3, l, if on { INK } else { LABEL });
            ax += wpx + 5;
        }
    }
    if strip {
        digital_cells(fb, g, tel, (x + 10, y + h - 28, w - 20, 22));
    }
}

// ---- radar -------------------------------------------------------------------------------------

fn log_r(d: f64) -> f32 {
    ((1.0 + d / 100.0).log10() / 8.0).clamp(0.0, 1.0) as f32
}

/// The scope: ship at the centre, nose up, log range, rotating sweep. A click on a blip targets
/// it (the same zone the window gives the body or the ship); anywhere else, next target.
pub fn radar(fb: &mut Framebuffer, g: &Game, tel: &Tel, cx: i32, cy: i32, r: i32) {
    let sim = &g.sim;
    let ship = &sim.ship;
    disc(fb, cx, cy, r + 6, 0x000B_0E12);
    disc(fb, cx, cy, r + 5, 0x003A_4452);
    disc(fb, cx, cy, r + 2, 0x0002_0807);
    ring(fb, cx, cy, r + 5, 0x0078_8496);
    organ(
        g,
        (cx - r, cy - r, 2 * r, 2 * r),
        Act::Key(KeyCode::Tab),
        None,
        Id::Scope,
        0,
    );
    for (d, l) in [(1.0e3, "1"), (1.0e5, "100"), (1.0e7, "10K"), (1.0e9, "1M")] {
        let rr = (log_r(d) * r as f32).round() as i32;
        ring(fb, cx, cy, rr, 0x0013_3A20);
        if rr > 10 && r >= 40 {
            txt(fb, cx + 2, cy - rr - 1, l, 0x001F_6A3A);
        }
    }
    line_over(fb, cx - r, cy, cx + r, cy, 0x0013_3A20);
    line_over(fb, cx, cy - r, cx, cy + r, 0x0013_3A20);
    // Sweep: a wedge that fades behind the line.
    let a = tel.t * 1.6;
    let step = if r > 70 { 3 } else { 2 };
    for k in 0..36 {
        let aa = a - k as f32 * 0.028;
        let f = 1.0 - k as f32 / 36.0;
        let (ca, sa) = (aa.cos(), aa.sin());
        let c = scale_color(0x0030_FF70, 0.22 * f * f);
        let mut rr = 4;
        while rr < r {
            fb.plot(
                cx + (rr as f32 * ca) as i32,
                cy + (rr as f32 * sa) as i32,
                c,
            );
            rr += step;
        }
    }
    let (sx, sy) = pt(cx, cy, r as f32, a);
    line_over(fb, cx, cy, sx, sy, 0x0040_C060);
    // Contacts.
    let inv = ship.rot.transpose();
    let mut pts = g.cockpit.radar_pts.borrow_mut();
    pts.clear();
    let mut plot_contact =
        |fb: &mut Framebuffer, l: Vec3, c: u32, sel: bool, sq: bool, act: Act| {
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
            // A blip is a zone of its own (none closer than 5 px to another).
            if pts
                .iter()
                .all(|&(qx, qy)| (qx - px).abs().max((qy - py).abs()) > 5)
            {
                pts.push((px, py));
                organ(g, (px - 3, py - 3, 7, 7), act, None, Id::Blip, 0);
            }
        };
    for (i, b) in sim.bodies.iter().enumerate() {
        if matches!(b.kind, vector_world::BodyKind::Star(_)) || sim.scan[i] == 0 {
            continue;
        }
        let l = inv.mul_vec(b.pos - ship.pos);
        plot_contact(
            fb,
            l,
            crate::hud::kind_color(b.kind),
            Some(i) == sim.target,
            matches!(b.kind, vector_world::BodyKind::Station),
            Act::Call(Game::click_body, i),
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
            Act::Call(Game::click_npc, k),
        );
    }
    // The ship.
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
pub fn nav_screen(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    well(fb, r, SCREEN_BG);
    display(g, r, Id::NavScreen, 0);
    let sim = &g.sim;
    let title = sim.orbit_readout().map_or("NAVIGATION".to_string(), |ro| {
        format!(
            "NAV {}",
            fit(
                &sim.bodies[ro.body].name.to_uppercase(),
                ((w as usize).saturating_sub(8) / 8).saturating_sub(4)
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
    // The sketch of the orbit needs some room; a tiny screen shows the figures only.
    let orbit_w = ((h - 40).min(w * 38 / 100)).max(0);
    let sketch = orbit_w >= 30;
    let orbit_w = if sketch { orbit_w } else { -8 };
    let ocx = x + 6 + orbit_w / 2 + 4;
    let ocy = y + 14 + (h - 14 - 24) / 2;
    // Orbit sketch.
    let rmax = e.apoapsis.unwrap_or(e.periapsis * 2.0).max(e.periapsis);
    let scale = f64::from(orbit_w.max(1)) * 0.42 / rmax.max(1.0);
    let planet_r = (ro.radius * scale).clamp(3.0, f64::from(orbit_w.max(10)) * 0.3) as i32;
    if !sketch {
        // No sketch: the figures start at the left.
    } else {
        disc(fb, ocx, ocy, planet_r, 0x0020_6A98);
        ring(fb, ocx, ocy, planet_r, 0x0060_B0E0);
    }
    if let Some(ra) = e.apoapsis.filter(|_| sketch) {
        let a = (e.periapsis + ra) / 2.0;
        let b = a * (1.0 - e.e * e.e).max(0.0).sqrt();
        let cxo = -a * e.e * scale;
        let n = 48;
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
        // Periapsis (right) and apoapsis (left) marks.
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
    } else if sketch {
        txt_c(fb, ocx, ocy + planet_r + 8, "OUVERTE", WARN);
    }
    // Numbers.
    let tx = x + orbit_w + 14;
    let mut line = y + 17;
    let room = y + h - 28;
    // (long label, short label when the line is narrow, value, colour)
    let mut put = |fb: &mut Framebuffer, l: (&str, &str), v: &str, c: u32| {
        if line + 9 <= room {
            let label = if tx + tw(l.0) + 8 + tw(v) > x + w - 5 {
                l.1
            } else {
                l.0
            };
            txt(fb, tx, line, label, LABEL);
            txt_r(fb, x + w - 5, line, v, c);
            line += 11;
        }
    };
    put(
        fb,
        ("PERI", "PERI"),
        &alt(e.periapsis),
        if e.periapsis < ro.radius {
            RED_LED
        } else {
            GREEN_LED
        },
    );
    put(
        fb,
        ("APO", "APO"),
        &e.apoapsis.map_or("OUVERTE".into(), alt),
        GREEN_LED,
    );
    put(
        fb,
        ("PERIODE", "PER."),
        &e.period.map_or("--".into(), crate::fmt::duration),
        INK,
    );
    put(fb, ("VITESSE", "VIT."), &format!("{:.0}", e.speed), INK);
    put(fb, ("EXCENTR.", "EXC."), &format!("{:.3}", e.e), INK);
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
        txt(
            fb,
            x + 5,
            by,
            &fit("AUCUN EVENEMENT PREVU", ((w - 8) / 8) as usize),
            LABEL,
        );
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
pub fn plan_screen(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    well(fb, r, SCREEN_BG);
    display(g, r, Id::PlanScreen, 0);
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
    txt_b(fb, x + 20, y + 17, &fit(&state, ((w - 60) / 8) as usize), c);
    txt_r(
        fb,
        x + w - 5,
        y + 17,
        &format!("x{}", sim.nav.pending()),
        LABEL,
    );
    let cur = sim.nav.cursor();
    let mut ly = y + 30;
    let last = y + h - 22;
    if let Some(e) = sim.nav.orders.get(cur) {
        let t = sim.order_text(&e.order, e.guard);
        txt(fb, x + 5, ly, ">", AMBER_LED);
        txt(fb, x + 15, ly, &fit(&t, ((w - 20) / 8) as usize), INK);
        ly += 12;
    }
    for e in sim.nav.orders.iter().skip(cur + 1) {
        if ly + 9 > last {
            break;
        }
        let t = sim.order_text(&e.order, e.guard);
        txt(fb, x + 15, ly, &fit(&t, ((w - 20) / 8) as usize), LABEL);
        ly += 11;
    }
    // Buttons.
    let by = y + h - 17;
    let run_label = if sim.nav_running() { "PAUSE" } else { "LANCER" };
    let btns = [
        (run_label, 0usize, 0usize),
        ("ECRAN", 90, 1),
        ("TRAJ", 9, 2),
    ];
    let bw = (w - 8) / 3;
    for (l, id, n) in btns.iter() {
        let bx = x + 4 + *n as i32 * bw;
        let hov = organ(
            g,
            (bx, by, bw - 3, 14),
            Act::Call(Game::nav_click_cockpit, *id),
            None,
            Id::PlanBtn,
            *n,
        );
        let on = *n == 0 && sim.nav_running();
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
            &fit(l, ((bw - 6) / 8).max(2) as usize),
            if on { 0x0004_0608 } else { AMBER_LED },
        );
    }
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
            if i < lit { c } else { mix_color(DARK, c, 0.14) },
        );
    }
}

/// Target panel: name, distance, closing speed, radiation and the actions; the lines the
/// rectangle cannot hold are dropped, the buttons stay.
pub fn target_panel(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    well(fb, r, SCREEN_BG);
    display(g, r, Id::Target, 0);
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
    let bh = if h >= 100 { 22 } else { 18 };
    let by = y + h - bh - 4;
    let Some(t) = t else {
        txt(fb, x + 8, y + 26, "AUCUNE CIBLE", LABEL);
        pushbtn(
            fb,
            g,
            tel,
            (x + 4, by, w - 8, bh),
            "TAB : SUIVANTE",
            false,
            AMBER_LED,
            Act::Key(KeyCode::Tab),
            None,
            Id::TargetBtn,
            3,
        );
        return;
    };
    let chars = ((w - 14) / 8) as usize;
    let mut ly = y + 15;
    txt_b(
        fb,
        x + 6,
        ly,
        &fit(&t.name, chars),
        if t.hostile { RED_LED } else { AMBER_LED },
    );
    ly += 11;
    txt(fb, x + 6, ly, &fit(&t.kind, chars), LABEL);
    ly += 11;
    let dist = crate::hud::fmt_dist(t.dist);
    // The big distance when there is room for it, else on one line.
    let tall = by - ly >= 62;
    if tall {
        txt(fb, x + 6, ly, "DISTANCE", LABEL);
        big(fb, x + 6, ly + 10, &dist, 2, INK);
        ly += 28;
    } else {
        txt(fb, x + 6, ly, "DIST", LABEL);
        txt_r(fb, x + w - 6, ly, &dist, INK);
        ly += 11;
    }
    // Closing speed with an arrow: green when the gap shrinks.
    let closing = t.closing > 0.0;
    if ly + 10 <= by {
        for d in 0..5 {
            if closing {
                hl(fb, x + 10 - d, ly + 1 + d, 2 * d + 1, GREEN_LED);
            } else {
                hl(fb, x + 10 - d, ly + 5 - d, 2 * d + 1, AMBER_LED);
            }
        }
        txt(
            fb,
            x + 20,
            ly + 1,
            if w < 190 { "APPR." } else { "APPROCHE" },
            LABEL,
        );
        txt_r(
            fb,
            x + w - 6,
            ly + 1,
            &format!("{:+.0} M/S", t.closing),
            if closing { GREEN_LED } else { AMBER_LED },
        );
        ly += 12;
    }
    if ly + 10 <= by {
        txt(
            fb,
            x + 6,
            ly,
            if w < 190 { "RAD." } else { "RADIATION" },
            LABEL,
        );
        txt_r(
            fb,
            x + w - 6,
            ly,
            &format!("{:.2} GY/H", t.rad),
            if t.rad > 0.5 { RED_LED } else { INK },
        );
        ly += 11;
    }
    if ly + 10 <= by {
        if let Some((hu, sh)) = t.hull {
            txt(fb, x + 6, ly, "COQUE", LABEL);
            seg(fb, x + 54, ly + 1, 64.min(w / 3), hu);
            txt(fb, x + 54 + 64.min(w / 3) + 8, ly, "BCL", LABEL);
            seg(fb, x + 54 + 64.min(w / 3) + 36, ly + 1, 40.min(w / 5), sh);
        } else {
            txt(fb, x + 6, ly, "STATION", LABEL);
            txt_r(
                fb,
                x + w - 6,
                ly,
                if station_near { "A PORTEE" } else { "LOIN" },
                INK,
            );
        }
    }
    let bw = ((w - 8 - 2) / 3).max(40);
    let acts = [
        ("AMARRER", t.dock_ok, GREEN_LED, KeyCode::KeyD, 0usize),
        ("ATTERRIR", t.land_ok, GREEN_LED, KeyCode::Numpad4, 1),
        ("APPELER", t.call_ok, AMBER_LED, KeyCode::Enter, 2),
    ];
    for (k, (l, ok, c, code, n)) in acts.iter().enumerate() {
        pushbtn(
            fb,
            g,
            tel,
            (x + 4 + k as i32 * (bw + 1), by, bw, bh),
            l,
            *ok,
            *c,
            Act::Key(*code),
            None,
            Id::TargetBtn,
            *n,
        );
    }
}

/// The descent screen (replaces the navigation screen over a world): radar altitude, time to
/// impact, where to light the engines, slope and site.
pub fn descent_screen(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    well(fb, r, SCREEN_BG);
    display(g, r, Id::DescentScreen, 0);
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
            fit(
                &sim.bodies[ro.body].name.to_uppercase(),
                ((w - 90) / 8).max(4) as usize
            )
        ),
        AMBER_LED,
    );
    let alarm = ro.burn_now && tel.blink;
    let narrow = w < 170;
    txt(fb, x + 6, y + 16, "ALT RADAR", LABEL);
    let alt = crate::hud::fmt_dist(ro.alt);
    let impact = ro
        .t_impact
        .map_or("--".to_string(), |t| format!("{t:.0} S"));
    let scale = if narrow { 1 } else { 2 };
    big(
        fb,
        x + 6,
        y + 27,
        &alt,
        scale,
        if alarm { RED_LED } else { GREEN_LED },
    );
    if narrow {
        txt(fb, x + 6, y + 37, "IMPACT", LABEL);
        txt_r(
            fb,
            x + w - 6,
            y + 37,
            &impact,
            if alarm { RED_LED } else { INK },
        );
    } else {
        txt(fb, x + w / 2 + 6, y + 16, "IMPACT", LABEL);
        big(
            fb,
            x + w / 2 + 6,
            y + 27,
            &impact,
            2,
            if alarm { RED_LED } else { INK },
        );
    }
    let lines: [(&str, String, u32); 6] = [
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
    let top = y + 48;
    for (k, (l, v, c)) in lines.iter().enumerate() {
        let yy = top + k as i32 * 11;
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
    fill(fb, x + 4, py, w - 8, 12, mix_color(DARK, c, 0.28));
    txt_c(
        fb,
        x + w / 2,
        py + 2,
        &fit(label, ((w - 10) / 8) as usize),
        c,
    );
    if alarm && h >= 110 {
        txt_c(
            fb,
            x + w / 2,
            y + 40,
            &fit(">>> ALLUMER LES MOTEURS <<<", ((w - 4) / 8) as usize),
            RED_LED,
        );
    }
}

/// The docking screen: distance to the port, closing speed, drift and alignment.
pub fn dock_screen(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    well(fb, r, SCREEN_BG);
    display(g, r, Id::DockScreen, 0);
    let Some(d) = &tel.dock else {
        return;
    };
    let sim = &g.sim;
    screen_title(
        fb,
        x + 1,
        y + 1,
        w - 2,
        &format!(
            "AMARRAGE  {}",
            fit(
                &sim.bodies[d.body].name.to_uppercase(),
                ((w - 100) / 8).max(4) as usize
            )
        ),
        GREEN_LED,
    );
    txt(fb, x + 6, y + 16, "DISTANCE", LABEL);
    big(
        fb,
        x + 6,
        y + 27,
        &crate::hud::fmt_dist(d.range),
        if w < 170 { 1 } else { 2 },
        GREEN_LED,
    );
    let lines: [(&str, String, u32); 4] = [
        (
            "APPROCHE",
            format!("{:+.1} M/S", d.closing),
            if d.closing > 8.0 { RED_LED } else { INK },
        ),
        (
            "DERIVE",
            format!("{:.1} M/S", d.lateral_v),
            if d.lateral_v > 2.0 { AMBER_LED } else { INK },
        ),
        (
            "ALIGNEMENT",
            format!("{:.0} DEG", d.align.to_degrees()),
            if d.align.to_degrees() > 15.0 {
                AMBER_LED
            } else {
                INK
            },
        ),
        ("DECALAGE", format!("{:.0} M", d.offset), INK),
    ];
    for (k, (l, v, c)) in lines.iter().enumerate() {
        let yy = y + 48 + k as i32 * 11;
        if yy + 10 > y + h - 18 {
            break;
        }
        txt(fb, x + 6, yy, l, LABEL);
        txt_r(fb, x + w - 6, yy, v, *c);
    }
    let (label, c) = if d.clean {
        ("PRISE PROPRE", GREEN_LED)
    } else {
        ("PRISE BRUTALE", AMBER_LED)
    };
    let py = y + h - 15;
    fill(fb, x + 4, py, w - 8, 12, mix_color(DARK, c, 0.28));
    txt_c(fb, x + w / 2, py + 2, label, c);
}

/// The pilot panel: the aids of the approach as lit push buttons.
pub fn pilot_screen(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    well(fb, r, SCREEN_BG);
    display(g, r, Id::PilotScreen, 0);
    screen_title(fb, x + 1, y + 1, w - 2, "PILOTE ET AIDES", AMBER_LED);
    let (state, c) = match tel.pilot {
        Some(PilotKind::Land) => ("ATTERRISSAGE", GREEN_LED),
        Some(PilotKind::Ascend) => ("DECOLLAGE", GREEN_LED),
        Some(PilotKind::Dock) => ("AMARRAGE", GREEN_LED),
        None => ("MANUEL", LABEL),
    };
    lamp(fb, x + 10, y + 21, c, tel.pilot.is_some());
    txt_b(fb, x + 20, y + 17, &fit(state, ((w - 28) / 8) as usize), c);
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
        pushbtn(
            fb,
            g,
            tel,
            (bx, by, bw, bh),
            l,
            *on,
            *c,
            Act::Key(*code),
            None,
            Id::PilotBtn,
            k,
        );
    }
}

// ---- alert lamps -------------------------------------------------------------------------------

/// The alert lamps: amber fixed, red blinking. A click opens the dashboard with the alerts.
pub fn lamp_panel(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc, cols: i32) {
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
        organ(
            g,
            (lx, ly, cw - 3, ch - 3),
            Act::Key(KeyCode::Backslash),
            None,
            Id::Lamp,
            k,
        );
        fill(fb, lx, ly, cw - 3, ch - 3, DARK);
        let face = if on {
            mix_color(0x0010_1418, col, 0.85)
        } else {
            mix_color(0x0010_1418, col, 0.09)
        };
        fill(fb, lx + 2, ly + 2, cw - 7, ch - 7, face);
        if on && ch >= 18 {
            halo(fb, lx + cw / 2, ly + ch / 2, (cw / 2).min(34), col, 0.3);
        }
        frame(fb, lx, ly, cw - 3, ch - 3, 0x0044_4E5C);
        let tc = if on {
            0x0004_0608
        } else {
            mix_color(0x0050_5A66, col, 0.25)
        };
        let label = fit(label, ((cw - 10) / 8).max(3) as usize);
        txt_c(fb, lx + (cw - 3) / 2, ly + (ch - 3) / 2 - 3, &label, tc);
    }
}
