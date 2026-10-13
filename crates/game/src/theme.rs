//! Boite a outils graphique de TOUS les menus : palette par espace du menu F10, fonds teintes,
//! panneaux, tableaux, jauges, sparklines, pastilles, barre de commandes, en-tete d'ecran, et la
//! mise a l'echelle automatique (toile logique 1280x720 projetee sur la fenetre). Rendu OPAQUE
//! (`px_set`) : `plot` est additif et ne sert qu'aux halos. Les zones cliquables passent par
//! `Game::hit` (Act::Key / Call / Hold) et suivent la mise a l'echelle.
#![allow(clippy::too_many_arguments)]

use crate::game::Game;
use crate::mouse::Act;
use vector_render::{font, mix_color, scale_color, Framebuffer};
use winit::keyboard::KeyCode;

pub type Rc = (i32, i32, i32, i32);

// ---- palette -----------------------------------------------------------------------------------

pub const INK: u32 = 0x00E6_F0EC;
pub const GOOD: u32 = 0x0038_E060;
pub const WARN: u32 = 0x00F0_B030;
pub const BAD: u32 = 0x00FF_4040;
pub const BLUE: u32 = 0x0050_A8FF;

/// Accent colour of each of the eight spaces of the F10 menu.
pub const SPACE_COLORS: [u32; 8] = [
    0x0038_E070, // piloting
    0x004A_A0FF, // navigation
    0x00FF_9A3A, // ship
    0x0040_D8E8, // crew
    0x00F0_B830, // trade
    0x00A8_E048, // exploration
    0x00FF_7A8A, // colonies
    0x00B4_8CFF, // codex
];

/// Colours derived from one accent: the look of a whole screen.
#[derive(Clone, Copy)]
pub struct Theme {
    pub bg0: u32,
    pub bg1: u32,
    pub panel: u32,
    pub panel2: u32,
    pub border: u32,
    pub accent: u32,
    pub dim: u32,
}

pub fn theme(accent: u32) -> Theme {
    let base = 0x0006_090D;
    Theme {
        bg0: mix_color(base, accent, 0.07),
        bg1: mix_color(base, accent, 0.015),
        panel: mix_color(0x000A_1016, accent, 0.075),
        panel2: mix_color(0x000A_1016, accent, 0.13),
        border: mix_color(0x0014_1A20, accent, 0.42),
        accent,
        dim: mix_color(0x0070_7C80, accent, 0.25),
    }
}

/// Green when the share is healthy, amber when low, red when critical.
pub fn tint(good: f64) -> u32 {
    if good >= 0.5 {
        GOOD
    } else if good >= 0.25 {
        WARN
    } else {
        BAD
    }
}

// ---- primitives (all opaque) -------------------------------------------------------------------

pub fn fill(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, c: u32) {
    fb.rect_over(x, y, w, h, c);
}

pub fn hl(fb: &mut Framebuffer, x: i32, y: i32, w: i32, c: u32) {
    fb.rect_over(x, y, w, 1, c);
}

pub fn vl(fb: &mut Framebuffer, x: i32, y: i32, h: i32, c: u32) {
    fb.rect_over(x, y, 1, h, c);
}

pub fn frame(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, c: u32) {
    hl(fb, x, y, w, c);
    hl(fb, x, y + h - 1, w, c);
    vl(fb, x, y, h, c);
    vl(fb, x + w - 1, y, h, c);
}

pub fn vgrad(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, c0: u32, c1: u32) {
    for i in 0..h {
        let c = mix_color(c0, c1, i as f32 / (h - 1).max(1) as f32);
        hl(fb, x, y + i, w, c);
    }
}

pub fn line_over(fb: &mut Framebuffer, x0: i32, y0: i32, x1: i32, y1: i32, c: u32) {
    let (mut x, mut y) = (x0, y0);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let mut err = dx + dy;
    loop {
        fb.px_set(x, y, c);
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

pub fn disc(fb: &mut Framebuffer, cx: i32, cy: i32, r: i32, c: u32) {
    for dy in -r..=r {
        let dx = ((r * r - dy * dy) as f32).sqrt() as i32;
        hl(fb, cx - dx, cy + dy, 2 * dx + 1, c);
    }
}

pub fn ring(fb: &mut Framebuffer, cx: i32, cy: i32, r: i32, c: u32) {
    let n = (r * 8).max(24);
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        fb.px_set(
            cx + (r as f32 * a.cos()).round() as i32,
            cy + (r as f32 * a.sin()).round() as i32,
            c,
        );
    }
}

/// An arc of the circle (angles in radians, 0 = right, clockwise on the screen), `thick` pixels.
pub fn arc(fb: &mut Framebuffer, cx: i32, cy: i32, r: i32, a0: f32, a1: f32, thick: i32, c: u32) {
    let n = ((a1 - a0).abs() * r as f32 * 1.5).max(8.0) as i32;
    for i in 0..=n {
        let a = a0 + (a1 - a0) * i as f32 / n as f32;
        for t in 0..thick {
            let rr = (r - t) as f32;
            fb.px_set(
                cx + (rr * a.cos()).round() as i32,
                cy + (rr * a.sin()).round() as i32,
                c,
            );
        }
    }
}

/// A soft additive glow around a lit lamp.
pub fn halo(fb: &mut Framebuffer, cx: i32, cy: i32, r: i32, c: u32, k: f32) {
    for dy in -r..=r {
        for dx in -r..=r {
            let d = ((dx * dx + dy * dy) as f32).sqrt() / r as f32;
            if d < 1.0 {
                let f = (1.0 - d) * (1.0 - d) * k;
                fb.plot(cx + dx, cy + dy, scale_color(c, f));
            }
        }
    }
}

pub fn tw(s: &str) -> i32 {
    s.chars().count() as i32 * font::CELL_W as i32
}

/// Opaque text at pixel (x, y).
pub fn txt(fb: &mut Framebuffer, x: i32, y: i32, s: &str, c: u32) {
    for (i, ch) in s.chars().enumerate() {
        let g = font::glyph(ch);
        let ox = x + (i * font::CELL_W) as i32;
        for (cx, bits) in g.iter().enumerate() {
            for cy in 0..7 {
                if bits >> cy & 1 == 1 {
                    fb.px_set(ox + cx as i32, y + cy, c);
                }
            }
        }
        if let Some((_, mark)) = font::decompose(ch) {
            for &(mx, my) in mark.pixels() {
                fb.px_set(ox + mx, y + my, c);
            }
        }
    }
}

/// Opaque text running upwards from (x, y_bottom): a label along a vertical gauge.
pub fn txt_v(fb: &mut Framebuffer, x: i32, y_bottom: i32, s: &str, c: u32) {
    for (i, ch) in s.chars().enumerate() {
        let g = font::glyph(ch);
        let base = y_bottom - i as i32 * 8;
        for (cx, bits) in g.iter().enumerate() {
            for cy in 0..7 {
                if bits >> cy & 1 == 1 {
                    fb.px_set(x + cy, base - cx as i32, c);
                }
            }
        }
    }
}

/// Bold (double-struck) opaque text.
pub fn txt_b(fb: &mut Framebuffer, x: i32, y: i32, s: &str, c: u32) {
    txt(fb, x, y, s, c);
    txt(fb, x + 1, y, s, c);
}

pub fn txt_c(fb: &mut Framebuffer, cx: i32, y: i32, s: &str, c: u32) {
    txt(fb, cx - tw(s) / 2, y, s, c);
}

/// Text ending at x (right aligned).
pub fn txt_r(fb: &mut Framebuffer, x: i32, y: i32, s: &str, c: u32) {
    txt(fb, x - tw(s), y, s, c);
}

pub fn big(fb: &mut Framebuffer, x: i32, y: i32, s: &str, scale: i32, c: u32) {
    fb.text_big_over(x, y, s, scale, c);
}

pub fn big_w(s: &str, scale: i32) -> i32 {
    tw(s) * scale
}

/// The first `n` characters of `s`.
pub fn cut(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Text shortened with an ellipsis to fit `n` characters.
pub fn fit(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}.", cut(s, n.saturating_sub(1)))
    }
}

// ---- widgets -----------------------------------------------------------------------------------

/// The background of a screen: a gradient wash and a faint blueprint grid.
pub fn backdrop(fb: &mut Framebuffer, th: &Theme) {
    let (w, h) = (fb.w as i32, fb.h as i32);
    vgrad(fb, 0, 0, w, h, th.bg0, th.bg1);
    let g = mix_color(th.bg1, th.accent, 0.05);
    for x in (0..w).step_by(40) {
        vl(fb, x, 0, h, g);
    }
    for y in (0..h).step_by(40) {
        hl(fb, 0, y, w, g);
    }
}

/// A titled panel. Returns the inner rectangle.
pub fn panel(fb: &mut Framebuffer, r: Rc, title: &str, th: &Theme) -> Rc {
    let (x, y, w, h) = r;
    fill(fb, x, y, w, h, th.panel);
    frame(fb, x, y, w, h, th.border);
    fill(
        fb,
        x + 1,
        y + 1,
        w - 2,
        13,
        mix_color(th.panel, th.accent, 0.20),
    );
    fill(fb, x + 1, y + 1, 3, 13, th.accent);
    txt_b(fb, x + 9, y + 3, title, mix_color(th.accent, INK, 0.35));
    // Corner ticks.
    for (cx, cy, sx, sy) in [(x, y + h - 1, 1, -1), (x + w - 1, y + h - 1, -1, -1)] {
        hl(fb, cx.min(cx + 5 * sx), cy, 6, th.accent);
        vl(fb, cx, cy.min(cy + 5 * sy), 6, th.accent);
    }
    (x + 7, y + 19, w - 14, h - 24)
}

/// A rounded-looking chip of text: background `bg`, text `fg`. Returns its width.
pub fn pill(fb: &mut Framebuffer, x: i32, y: i32, s: &str, bg: u32, fg: u32) -> i32 {
    let w = tw(s) + 10;
    fill(fb, x, y, w, 12, bg);
    fill(fb, x, y, 1, 1, 0);
    fill(fb, x + w - 1, y, 1, 1, 0);
    fill(fb, x, y + 11, 1, 1, 0);
    fill(fb, x + w - 1, y + 11, 1, 1, 0);
    txt(fb, x + 5, y + 2, s, fg);
    w
}

/// A status dot with a halo.
pub fn dot(fb: &mut Framebuffer, x: i32, y: i32, c: u32) {
    halo(fb, x, y, 7, c, 0.55);
    disc(fb, x, y, 3, c);
    disc(fb, x - 1, y - 1, 1, mix_color(c, 0x00FF_FFFF, 0.6));
}

/// A segmented gauge in one colour.
pub fn seg_bar(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, frac: f64, c: u32) {
    let n = ((w + 1) / 6).max(1);
    let lit = (frac.clamp(0.0, 1.0) * f64::from(n)).round() as i32;
    for i in 0..n {
        let col = if i < lit {
            c
        } else {
            mix_color(0x0008_0C10, c, 0.14)
        };
        fill(fb, x + i * 6, y, 5, h, col);
    }
}

/// A segmented gauge whose segments take the colour of the zone they stand in: red near the
/// empty end, amber, green (or the reverse for what is bad when high).
pub fn seg_bar_z(
    fb: &mut Framebuffer,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    frac: f64,
    high_is_bad: bool,
) {
    let n = ((w + 1) / 6).max(1);
    let lit = (frac.clamp(0.0, 1.0) * f64::from(n)).round() as i32;
    for i in 0..n {
        let k = f64::from(i) / f64::from(n - 1).max(1.0);
        let k = if high_is_bad { 1.0 - k } else { k };
        let c = tint(k.max(0.02) * 1.0);
        let col = if i < lit {
            c
        } else {
            mix_color(0x0008_0C10, c, 0.14)
        };
        fill(fb, x + i * 6, y, 5, h, col);
    }
}

/// A thin continuous bar.
pub fn thin_bar(fb: &mut Framebuffer, x: i32, y: i32, w: i32, frac: f64, c: u32) {
    fill(fb, x, y, w, 4, 0x0008_0C10);
    let f = (frac.clamp(0.0, 1.0) * f64::from(w)) as i32;
    fill(fb, x, y, f, 4, c);
    hl(fb, x, y, f, mix_color(c, 0x00FF_FFFF, 0.35));
}

/// A line chart with a faint fill under it.
pub fn spark(
    fb: &mut Framebuffer,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    v: &[f64],
    c: u32,
    fill_under: bool,
) {
    if v.len() < 2 {
        return;
    }
    let (lo, hi) = v
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), q| (a.min(*q), b.max(*q)));
    let span = (hi - lo).max(1e-9);
    let pt = |i: usize| {
        (
            x + (i as i32 * (w - 1)) / (v.len() as i32 - 1),
            y + h - 1 - (((v[i] - lo) / span) * f64::from(h - 1)) as i32,
        )
    };
    let dim = mix_color(0x0008_0C10, c, 0.22);
    if fill_under {
        for i in 0..v.len() {
            let (px, py) = pt(i);
            vl(fb, px, py, y + h - py, dim);
        }
    }
    for i in 1..v.len() {
        let (a, b) = (pt(i - 1), pt(i));
        line_over(fb, a.0, a.1, b.0, b.1, c);
    }
    if fill_under {
        let l = pt(v.len() - 1);
        disc(fb, l.0, l.1, 2, c);
    }
}

/// The permanent command bar: each command is a key cap and its label; both are clickable.
pub fn cmdbar(fb: &mut Framebuffer, g: &Game, th: &Theme, items: &[(&str, &str)]) {
    let (w, y) = (fb.w as i32, fb.h as i32 - 22);
    fill(fb, 0, y, w, 22, mix_color(0x0004_0608, th.accent, 0.10));
    hl(fb, 0, y, w, th.border);
    let mut x = 10;
    for (key, label) in items {
        let kw = tw(key) + 8;
        let lw = tw(label);
        let act = crate::mouse::key_of_word(g.screen, key).map(Act::Key);
        let hov = act.is_some_and(|a| g.hit((x, y + 3, x + kw + 6 + lw, y + 19), a, None));
        let kc = if hov { INK } else { th.accent };
        fill(fb, x, y + 4, kw, 14, kc);
        fill(fb, x, y + 4, 1, 1, 0);
        fill(fb, x + kw - 1, y + 4, 1, 1, 0);
        txt_b(fb, x + 4, y + 7, key, 0x0006_090D);
        txt(
            fb,
            x + kw + 6,
            y + 7,
            label,
            if hov { INK } else { 0x00A8_B4B0 },
        );
        x += kw + lw + 22;
    }
}

/// Top bar of a screen: logo, title, breadcrumb, credits, alerts badge and the MENU button.
pub fn topbar(fb: &mut Framebuffer, g: &Game, th: &Theme, title: &str, crumb: &str) {
    let w = fb.w as i32;
    fill(fb, 0, 0, w, 26, mix_color(0x0004_0608, th.accent, 0.13));
    hl(fb, 0, 26, w, th.accent);
    hl(fb, 0, 27, w, mix_color(th.bg1, th.accent, 0.2));
    // Logo.
    fill(fb, 8, 5, 16, 16, th.accent);
    big(fb, 11, 8, "V", 2, 0x0006_090D);
    txt_b(fb, 30, 9, "VECTOR", INK);
    vl(fb, 82, 5, 16, th.border);
    big(fb, 92, 6, title, 2, mix_color(th.accent, INK, 0.25));
    let tx = 92 + big_w(title, 2) + 14;
    txt(fb, tx, 9, crumb, th.dim);
    // Right side: menu, alerts, credits.
    let alerts = g.alerts();
    let (c, a, _) = vector_sim::Sim::alert_counts(&alerts);
    let menu = "MENU F10";
    let mw = tw(menu) + 14;
    let mx = w - mw - 8;
    let hov = g.hit((mx, 3, mx + mw, 23), Act::Key(KeyCode::F10), None);
    fill(fb, mx, 3, mw, 20, if hov { INK } else { th.accent });
    txt_b(fb, mx + 7, 9, menu, 0x0006_090D);
    let (label, col) = if c > 0 {
        (format!("ALERTES {c} CRIT {a} ATT"), BAD)
    } else if a > 0 {
        (format!("ALERTES {a} ATT"), WARN)
    } else {
        ("ALERTES 0".to_string(), GOOD)
    };
    let aw = tw(&label) + 22;
    let ax = mx - aw - 10;
    g.hit((ax, 3, ax + aw, 23), Act::Key(KeyCode::Backslash), None);
    fill(fb, ax, 3, aw, 20, mix_color(0x0006_090D, col, 0.22));
    frame(fb, ax, 3, aw, 20, col);
    dot(fb, ax + 11, 13, col);
    txt_b(fb, ax + 20, 9, &label, col);
    let cr = format!("CREDITS {}", crate::fmt::amount(g.sim.credits));
    txt_r(fb, ax - 16, 9, &cr, INK);
}
