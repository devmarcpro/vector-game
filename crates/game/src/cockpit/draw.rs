//! Drawing toolbox of the cockpit: OPAQUE primitives (clipped once, whole spans at a time: the
//! console repaints a few hundred thousand pixels a frame and must stay cheap), the metal of the
//! dash (plates, wells, screws, lamps) and the engraved text. Only the halos and the glows are
//! additive (`Framebuffer::plot`).
use std::sync::OnceLock;
use vector_render::{font, mix_color, Framebuffer};

/// A rectangle (x, y, width, height) in pixels.
pub type Rc = (i32, i32, i32, i32);

pub const INK: u32 = 0x00E6_F0EC;
pub const GOOD: u32 = 0x0038_E060;
pub const WARN: u32 = 0x00F0_B030;
pub const BAD: u32 = 0x00FF_4040;

pub const METAL: u32 = 0x0019_1E26;
pub const METAL_HI: u32 = 0x0036_404D;
pub const METAL_LO: u32 = 0x0006_080B;
pub const LABEL: u32 = 0x008A_98AA;
pub const AMBER_LED: u32 = 0x00FF_B000;
pub const RED_LED: u32 = 0x00FF_3030;
pub const GREEN_LED: u32 = 0x0040_FF70;
pub const BLUE_LED: u32 = 0x0050_A8FF;
pub const SCREEN_BG: u32 = 0x0002_0807;
pub const DARK: u32 = 0x0008_0B0E;

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

// ---- primitives --------------------------------------------------------------------------------

/// A filled rectangle, clipped to the frame once and written a row at a time.
pub fn fill(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, c: u32) {
    let (x0, y0) = (x.max(0), y.max(0));
    let (x1, y1) = ((x + w).min(fb.w as i32), (y + h).min(fb.h as i32));
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let (x0, x1) = (x0 as usize, x1 as usize);
    for row in y0 as usize..y1 as usize {
        fb.px[row * fb.w + x0..row * fb.w + x1].fill(c);
    }
}

pub fn hl(fb: &mut Framebuffer, x: i32, y: i32, w: i32, c: u32) {
    fill(fb, x, y, w, 1, c);
}

pub fn vl(fb: &mut Framebuffer, x: i32, y: i32, h: i32, c: u32) {
    fill(fb, x, y, 1, h, c);
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

/// Opaque Bresenham line.
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
    let r2 = r * r;
    for dy in -r..=r {
        let dx = ((r2 - dy * dy) as f32).sqrt() as i32;
        hl(fb, cx - dx, cy + dy, 2 * dx + 1, c);
    }
}

pub fn ring(fb: &mut Framebuffer, cx: i32, cy: i32, r: i32, c: u32) {
    let n = (r * 6).max(24);
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
#[allow(clippy::too_many_arguments)]
pub fn arc(fb: &mut Framebuffer, cx: i32, cy: i32, r: i32, a0: f32, a1: f32, thick: i32, c: u32) {
    let n = ((a1 - a0).abs() * r as f32 * 1.5).max(8.0) as i32;
    for i in 0..=n {
        let a = a0 + (a1 - a0) * i as f32 / n as f32;
        let (ca, sa) = (a.cos(), a.sin());
        for t in 0..thick {
            let rr = (r - t) as f32;
            fb.px_set(
                cx + (rr * ca).round() as i32,
                cy + (rr * sa).round() as i32,
                c,
            );
        }
    }
}

/// Falloff tables of the halos: for each radius, the strength (0..=255) at each squared distance.
fn falloff(r: i32) -> &'static [u8] {
    static T: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    let t = T.get_or_init(|| {
        (0..=48)
            .map(|r| {
                (0..(r * r).max(1))
                    .map(|d2| {
                        let d = (d2 as f32).sqrt() / r.max(1) as f32;
                        ((1.0 - d) * (1.0 - d) * 255.0) as u8
                    })
                    .collect()
            })
            .collect()
    });
    &t[r.clamp(1, 48) as usize]
}

/// A soft additive glow around a lit lamp (`k` 0..1 is its strength).
pub fn halo(fb: &mut Framebuffer, cx: i32, cy: i32, r: i32, c: u32, k: f32) {
    let r = r.clamp(1, 48);
    let tab = falloff(r);
    let kk = (k.clamp(0.0, 1.0) * 256.0) as u32;
    let (cr, cg, cb) = ((c >> 16) & 255, (c >> 8) & 255, c & 255);
    for dy in -r..=r {
        let y = cy + dy;
        if y < 0 || y >= fb.h as i32 {
            continue;
        }
        let half = ((r * r - dy * dy).max(0) as f32).sqrt() as i32;
        for dx in -half..=half {
            let x = cx + dx;
            if x < 0 || x >= fb.w as i32 {
                continue;
            }
            let d2 = (dx * dx + dy * dy) as usize;
            let Some(&f) = tab.get(d2) else { continue };
            let f = (u32::from(f) * kk) >> 8;
            let p = &mut fb.px[y as usize * fb.w + x as usize];
            let add = |a: u32, b: u32| (a + ((b * f) >> 8)).min(255);
            let r_ = add((*p >> 16) & 255, cr);
            let g_ = add((*p >> 8) & 255, cg);
            let b_ = add(*p & 255, cb);
            *p = (r_ << 16) | (g_ << 8) | b_;
        }
    }
}

// ---- text --------------------------------------------------------------------------------------

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

/// The first `n` characters of `s`.
pub fn cut(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Text shortened with a dot to fit `n` characters.
pub fn fit(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}.", cut(s, n.saturating_sub(1)))
    }
}

// ---- metal -------------------------------------------------------------------------------------

pub fn screw(fb: &mut Framebuffer, x: i32, y: i32) {
    disc(fb, x, y, 2, 0x004A_5563);
    hl(fb, x - 1, y, 3, 0x0018_1C22);
    fb.px_set(x - 1, y - 1, 0x0090_9CAC);
}

/// A bevelled metal plate with an engraved title.
pub fn plate(fb: &mut Framebuffer, r: Rc, title: &str) {
    let (x, y, w, h) = r;
    if w < 12 || h < 12 {
        return;
    }
    vgrad(fb, x, y, w, h, METAL, mix_color(METAL, 0, 0.35));
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
pub fn well(fb: &mut Framebuffer, r: Rc, bg: u32) {
    let (x, y, w, h) = r;
    fill(fb, x, y, w, h, bg);
    hl(fb, x, y, w, METAL_LO);
    vl(fb, x, y, h, METAL_LO);
    hl(fb, x, y + h - 1, w, METAL_HI);
    vl(fb, x + w - 1, y, h, METAL_HI);
}

/// A lamp: halo and bright when lit, a dull glass when not.
pub fn lamp(fb: &mut Framebuffer, x: i32, y: i32, c: u32, on: bool) {
    if on {
        halo(fb, x, y, 9, c, 0.7);
        disc(fb, x, y, 4, c);
        disc(fb, x, y, 2, mix_color(c, 0x00FF_FFFF, 0.55));
    } else {
        disc(fb, x, y, 4, mix_color(DARK, c, 0.16));
        ring(fb, x, y, 4, 0x0030_3844);
        fb.px_set(x - 1, y - 1, mix_color(DARK, c, 0.35));
    }
}

/// Darken a region of the scene (a backing for the glass texts over a bright sky).
pub fn shade(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, k: f32) {
    let k256 = (k.clamp(0.0, 1.0) * 256.0) as u32;
    let (x0, y0) = (x.max(0), y.max(0));
    let (x1, y1) = ((x + w).min(fb.w as i32), (y + h).min(fb.h as i32));
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    for row in y0 as usize..y1 as usize {
        for p in &mut fb.px[row * fb.w + x0 as usize..row * fb.w + x1 as usize] {
            let c = *p;
            let r = (((c >> 16) & 255) * k256) >> 8;
            let g = (((c >> 8) & 255) * k256) >> 8;
            let b = ((c & 255) * k256) >> 8;
            *p = (r << 16) | (g << 8) | b;
        }
    }
}

/// A filled triangle.
pub fn tri(fb: &mut Framebuffer, a: (i32, i32), b: (i32, i32), c: (i32, i32), col: u32) {
    let (miny, maxy) = (a.1.min(b.1).min(c.1), a.1.max(b.1).max(c.1));
    for y in miny..=maxy {
        let mut xs = [0.0f32; 3];
        let mut n = 0;
        for (p, q) in [(a, b), (b, c), (c, a)] {
            if ((p.1 <= y && q.1 > y) || (q.1 <= y && p.1 > y)) && n < 3 {
                let t = (y - p.1) as f32 / (q.1 - p.1) as f32;
                xs[n] = p.0 as f32 + t * (q.0 - p.0) as f32;
                n += 1;
            }
        }
        if n >= 2 {
            let (lo, hi) = (xs[0].min(xs[1]), xs[0].max(xs[1]));
            hl(fb, lo.round() as i32, y, (hi - lo).round() as i32 + 1, col);
        }
    }
}

/// Shorten a number for a dial: 1200 -> 1.2K, 3 000 000 -> 3M.
pub fn short(v: f64) -> String {
    let a = v.abs();
    if a >= 1e9 {
        format!("{:.0}G", v / 1e9)
    } else if a >= 1e6 {
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
