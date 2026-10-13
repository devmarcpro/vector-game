//! Boite a outils de haut niveau des menus : toile logique 1280x720 mise a l'echelle de la fenetre
//! (zones cliquables comprises), contraste, tuiles KPI, tableaux, bandeau de didacticiel.
//! Les primitives (panneaux, jauges, texte opaque...) sont dans `theme`.
#![allow(clippy::too_many_arguments)]
// Widgets (KPI, tableaux, crans, contraste) en attente de la vague 2 des ecrans.
#![allow(dead_code)]

use crate::game::Game;
use crate::mouse::Act;
use crate::theme::*;
use vector_render::{mix_color, Framebuffer};
use winit::keyboard::KeyCode;

/// Taille de la toile logique sur laquelle sont dessines les ecrans du style NOUVEAU.
pub const LW: usize = 1280;
pub const LH: usize = 720;

type Xf = (f32, i32, i32);

thread_local! {
    /// Projection toile -> fenetre active pendant le dessin d'un ecran : (echelle, decalage x, y).
    static XF: std::cell::Cell<Option<Xf>> = const { std::cell::Cell::new(None) };
    /// La toile, gardee d'une image a l'autre (aucune allocation par image).
    static CANVAS: std::cell::RefCell<Option<Framebuffer>> = const { std::cell::RefCell::new(None) };
    /// Table d'echantillonnage horizontale du dernier format de fenetre.
    static XTAB: std::cell::RefCell<(usize, Vec<(u32, u32)>)> = const { std::cell::RefCell::new((0, Vec::new())) };
}

/// Un rectangle de la toile (x0, y0, x1, y1) vu depuis la fenetre : pour les zones cliquables.
pub fn xf_rect(r: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    match XF.with(std::cell::Cell::get) {
        None => r,
        Some((s, ox, oy)) => (
            ox + (r.0 as f32 * s).floor() as i32,
            oy + (r.1 as f32 * s).floor() as i32,
            ox + (r.2 as f32 * s).ceil() as i32,
            oy + (r.3 as f32 * s).ceil() as i32,
        ),
    }
}

/// Echelle d'une fenetre `w` x `h` : la toile y tient entierement.
pub fn scale_for(w: usize, h: usize) -> f32 {
    (w as f32 / LW as f32).min(h as f32 / LH as f32)
}

/// Dessine un ecran du style NOUVEAU : sur la toile 1280x720, puis projete (lissage bilineaire)
/// sur la fenetre, centre, avec des marges noires. A 1280x720 le dessin est direct.
pub fn draw_canvas(fb: &mut Framebuffer, g: &Game, f: fn(&mut Framebuffer, &Game)) {
    let (w, h) = (fb.w, fb.h);
    if w == LW && h == LH {
        fb.clear();
        f(fb, g);
        return;
    }
    let s = scale_for(w, h);
    let (dw, dh) = (
        ((LW as f32 * s) as usize).clamp(2, w),
        ((LH as f32 * s) as usize).clamp(2, h),
    );
    let (ox, oy) = ((w - dw) / 2, (h - dh) / 2);
    CANVAS.with(|c| {
        let mut slot = c.borrow_mut();
        let cv = slot.get_or_insert_with(|| Framebuffer::new(LW, LH));
        cv.clear();
        cv.clipped = 0;
        cv.clipped_first = None;
        XF.with(|x| x.set(Some((s, ox as i32, oy as i32))));
        f(cv, g);
        XF.with(|x| x.set(None));
        fb.clipped += cv.clipped;
        if fb.clipped_first.is_none() {
            fb.clipped_first = cv.clipped_first.take();
        }
        fb.clear();
        blit(fb, cv, (ox, oy, dw, dh));
    });
}

fn lerp(a: u32, b: u32, t: u32) -> u32 {
    let rb = (((a & 0x00FF_00FF) * (256 - t) + (b & 0x00FF_00FF) * t) >> 8) & 0x00FF_00FF;
    let g = (((a & 0x0000_FF00) * (256 - t) + (b & 0x0000_FF00) * t) >> 8) & 0x0000_FF00;
    rb | g
}

/// Copy `src` (1280x720) into the rectangle (x, y, w, h) of `dst`, bilinear.
fn blit(dst: &mut Framebuffer, src: &Framebuffer, (ox, oy, dw, dh): (usize, usize, usize, usize)) {
    XTAB.with(|t| {
        let mut t = t.borrow_mut();
        if t.0 != dw {
            t.0 = dw;
            t.1.clear();
            let fx = ((src.w as u64 - 1) << 16) / (dw as u64 - 1).max(1);
            for x in 0..dw {
                let p = x as u64 * fx;
                t.1.push(((p >> 16) as u32, ((p & 0xFFFF) >> 8) as u32));
            }
        }
        let fy = ((src.h as u64 - 1) << 16) / (dh as u64 - 1).max(1);
        for y in 0..dh {
            let p = y as u64 * fy;
            let (y0, wy) = ((p >> 16) as usize, ((p & 0xFFFF) >> 8) as u32);
            let y1 = (y0 + 1).min(src.h - 1);
            let r0 = &src.px[y0 * src.w..(y0 + 1) * src.w];
            let r1 = &src.px[y1 * src.w..(y1 + 1) * src.w];
            let row = &mut dst.px[(oy + y) * dst.w + ox..(oy + y) * dst.w + ox + dw];
            for (d, &(x0, wx)) in row.iter_mut().zip(t.1.iter()) {
                let x0 = x0 as usize;
                let x1 = (x0 + 1).min(src.w - 1);
                *d = lerp(lerp(r0[x0], r0[x1], wx), lerp(r1[x0], r1[x1], wx), wy);
            }
        }
    });
}

// ---- contraste ---------------------------------------------------------------------------------

fn lin(c: u8) -> f64 {
    let v = f64::from(c) / 255.0;
    if v <= 0.039_28 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Relative luminance of a 0x00RRGGBB colour (WCAG).
pub fn luminance(c: u32) -> f64 {
    0.2126 * lin((c >> 16) as u8) + 0.7152 * lin((c >> 8) as u8) + 0.0722 * lin(c as u8)
}

/// WCAG contrast ratio of two colours (1..21).
pub fn contrast(a: u32, b: u32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

// ---- widgets de plus haut niveau ---------------------------------------------------------------

/// Tuile KPI : libelle attenue, grande valeur, detail et jauge.
pub fn kpi(
    fb: &mut Framebuffer,
    r: Rc,
    th: &Theme,
    label: &str,
    value: &str,
    sub: &str,
    f: f64,
    c: u32,
) {
    let (x, y, w, h) = r;
    fill(fb, x, y, w, h, th.panel);
    frame(fb, x, y, w, h, th.border);
    fill(fb, x, y, 3, h, c);
    txt(fb, x + 9, y + 5, label, th.dim);
    big(fb, x + 9, y + 17, value, 2, INK);
    txt(fb, x + 9, y + h - 21, sub, th.dim);
    thin_bar(fb, x + 9, y + h - 9, w - 18, f, c);
}

/// Selecteur a crans : `n` crans dont ceux jusqu'a `sel` sont allumes.
pub fn notches(fb: &mut Framebuffer, x: i32, y: i32, n: i32, sel: i32, c: u32) {
    for i in 0..n {
        let col = if i <= sel {
            c
        } else {
            mix_color(0x0008_0C10, c, 0.16)
        };
        fill(fb, x + i * 9, y, 7, 10, col);
    }
}

/// En-tete de tableau : (decalage x, libelle) par colonne ; `sort` = colonne triee (surlignee).
pub fn table_head(
    fb: &mut Framebuffer,
    r: (i32, i32, i32),
    th: &Theme,
    cols: &[(i32, &str)],
    sort: Option<usize>,
) {
    let (x, y, w) = r;
    fill(fb, x, y, w, 13, th.panel2);
    for (i, (cx, l)) in cols.iter().enumerate() {
        let c = if sort == Some(i) { INK } else { th.dim };
        txt(fb, x + cx, y + 3, l, c);
    }
    hl(fb, x, y + 13, w, th.border);
}

/// Fond d'une ligne de tableau : alternee, ou en surbrillance si selectionnee.
pub fn table_row(fb: &mut Framebuffer, r: Rc, th: &Theme, i: usize, selected: bool) {
    let (x, y, w, h) = r;
    if selected {
        fill(fb, x, y, w, h, mix_color(th.panel, th.accent, 0.30));
        fill(fb, x, y, 3, h, th.accent);
    } else if i % 2 == 1 {
        fill(fb, x, y, w, h, mix_color(th.panel, th.accent, 0.06));
    }
}

/// Bandeau de didacticiel / dernier message, sous l'en-tete de l'ecran (toile logique).
pub fn notice_strip(fb: &mut Framebuffer, g: &Game) {
    let y = LH as i32 - 22 - 14;
    let wide = LW as i32;
    if let Some((i, text)) = g.tutorial_line() {
        let head = format!("TUTO {}/{}", i + 1, crate::tutorial::STEPS.len());
        fill(fb, 0, y, wide, 13, mix_color(0x0006_090D, WARN, 0.18));
        txt_b(fb, 8, y + 3, &head, WARN);
        let room = ((wide - 190 - tw(&head)) / 8).max(10) as usize;
        txt(fb, 16 + tw(&head), y + 3, &fit(&text, room), INK);
        let l = "SUPPR IGNORER";
        let x = wide - tw(l) - 10;
        let hov = g.hit((x - 4, y, wide, y + 13), Act::Key(KeyCode::Delete), None);
        txt(fb, x, y + 3, l, if hov { INK } else { WARN });
    } else if let Some(m) = g.messages.last() {
        let room = ((wide - 20) / 8) as usize;
        fill(fb, 0, y, wide, 13, mix_color(0x0006_090D, BLUE, 0.12));
        txt(fb, 10, y + 3, &fit(m, room), INK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_fits_every_window() {
        for (w, h) in [
            (1024, 576),
            (1280, 720),
            (1920, 1080),
            (2560, 1080),
            (800, 900),
        ] {
            let s = scale_for(w, h);
            assert!((LW as f32 * s) as usize <= w && (LH as f32 * s) as usize <= h);
        }
    }

    #[test]
    fn contrast_basics() {
        assert!(contrast(0, 0x00FF_FFFF) > 20.0);
        assert!((contrast(INK, INK) - 1.0).abs() < 1e-9);
        for s in 0..8 {
            let th = theme(SPACE_COLORS[s]);
            for bg in [th.bg0, th.panel, th.panel2] {
                assert!(contrast(INK, bg) >= 4.5, "INK on space {s}");
                assert!(contrast(th.dim, th.panel) >= 4.5, "dim on panel, space {s}");
            }
        }
    }

    #[test]
    fn bar_is_proportional_and_deterministic() {
        let mut a = Framebuffer::new(200, 20);
        let mut b = Framebuffer::new(200, 20);
        seg_bar(&mut a, 0, 0, 120, 6, 0.5, GOOD);
        seg_bar(&mut b, 0, 0, 120, 6, 0.5, GOOD);
        assert_eq!(a.px, b.px);
        let lit = |f: &Framebuffer| (0..120).filter(|&x| f.px[3 * 200 + x] == GOOD).count();
        let (l, full) = (lit(&a), {
            let mut c = Framebuffer::new(200, 20);
            seg_bar(&mut c, 0, 0, 120, 6, 1.0, GOOD);
            lit(&c)
        });
        assert!(l * 2 >= full - 6 && l * 2 <= full + 6);
    }

    #[test]
    fn blit_keeps_inside_its_rectangle() {
        let mut src = Framebuffer::new(LW, LH);
        src.rect_over(0, 0, LW as i32, LH as i32, 0x00FF_FFFF);
        let mut dst = Framebuffer::new(1024, 700);
        blit(&mut dst, &src, (0, 62, 1024, 576));
        assert_eq!(dst.px[10], 0);
        assert_eq!(dst.px[62 * 1024 + 10], 0x00FF_FFFF);
        assert_eq!(dst.px[(62 + 575) * 1024 + 1023], 0x00FF_FFFF);
        assert_eq!(dst.px[(62 + 576) * 1024 + 10], 0);
    }
}
