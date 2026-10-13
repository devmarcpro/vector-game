//! MAQUETTES D'INTERFACE (a retirer apres decision) : a quoi pourraient ressembler les ecrans de
//! gestion et le cockpit une fois restyles. Chaque maquette est dessinee avec les VRAIES donnees
//! de la partie (`--snapshot shots/mock_<nom>.png mock_<nom>`) et a son pendant actuel
//! (`now_<nom>`) pour comparer. Rien ici n'est utilise par le jeu normal.
//!
//! Boite a outils : rendu OPAQUE (`px_set`) pour les panneaux, le texte et les instruments, rendu
//! additif (`plot`) seulement pour les halos et les lueurs. Les zones cliquables sont declarees
//! avec `Game::hit` comme partout ailleurs (Act::Key / Act::Call).
#![allow(clippy::too_many_arguments)]

use crate::game::{CrewTab, Game, Screen};
use crate::mouse::Act;
use vector_render::{font, mix_color, scale_color, Framebuffer};
use winit::keyboard::KeyCode;

pub type Rc = (i32, i32, i32, i32);

// ---- the scene names ---------------------------------------------------------------------------

/// What a snapshot name asks for: the scene that sets the game up, and which drawing to use.
#[derive(Clone, Copy)]
pub struct Spec {
    pub base: &'static str,
    pub kind: Kind,
    /// True: the mock-up; false: the screen as it is now (`now_<name>`).
    pub mock: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Colony,
    Hub,
    Crew,
    CrewMed,
    Surface,
    Station,
    Dash,
    Cockpit(crate::mock_cockpit::Layout),
    CockpitLand(crate::mock_cockpit::Layout),
}

pub fn resolve(name: &str) -> Option<Spec> {
    use crate::mock_cockpit::Layout::{COpen, A, B, C};
    let (mock, rest) = match name.strip_prefix("mock_") {
        Some(r) => (true, r),
        None => (false, name.strip_prefix("now_")?),
    };
    let (base, kind) = match rest {
        "colony" => ("city", Kind::Colony),
        "hub" => ("hub_dock", Kind::Hub),
        "crew" => ("medicine", Kind::Crew),
        "crew_med" => ("medicine", Kind::CrewMed),
        "surface" => ("surface", Kind::Surface),
        "station" => ("station", Kind::Station),
        "dash" => ("dash_dock", Kind::Dash),
        "cockpit" | "cockpit_a" => ("cockpit", Kind::Cockpit(A)),
        "cockpit_b" => ("cockpit", Kind::Cockpit(B)),
        "cockpit_c" => ("cockpit", Kind::Cockpit(C)),
        "cockpit_c_open" => ("cockpit", Kind::Cockpit(COpen)),
        "cockpit_land" | "cockpit_land_a" => ("land_descent", Kind::CockpitLand(A)),
        "cockpit_land_b" => ("land_descent", Kind::CockpitLand(B)),
        "cockpit_land_c" => ("land_descent", Kind::CockpitLand(C)),
        "cockpit_land_c_open" => ("land_descent", Kind::CockpitLand(COpen)),
        "land" => ("land_descent", Kind::CockpitLand(A)),
        _ => return None,
    };
    Some(Spec { base, kind, mock })
}

thread_local! {
    /// Reference prices of the last 30 days of every good (filled by `after_setup`).
    static SERIES: std::cell::RefCell<Vec<Vec<f64>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Adjust the game after the scene of `spec.base` has set it up.
pub fn after_setup(g: &mut Game, spec: &Spec) {
    g.tut_step = None;
    match spec.kind {
        Kind::Colony => {
            g.col_tab = 0;
            g.col_focus = 0;
            g.b_sel = 3;
            g.build_sel = 4;
            g.col_flow = false;
            g.col_health = false;
            g.col_proj = false;
            g.screen = Screen::Colony;
        }
        Kind::Crew => {
            g.crew_tab = CrewTab::List;
            g.crew_sel = 1.min(g.sim.crew.members.len().saturating_sub(1));
            g.screen = Screen::Crew;
        }
        Kind::CrewMed => {
            g.crew_tab = CrewTab::Medicine;
            g.crew_sel = 0;
            g.screen = Screen::Crew;
        }
        Kind::Station => {
            g.sim
                .advance(4.0 * 86_400.0, &vector_sim::ShipInput::default());
            g.market_sort = 1;
            g.screen = Screen::Station;
            if let Some(st) = g.sim.docked {
                let s = g.sim.price_series(st, 30);
                SERIES.with(|c| *c.borrow_mut() = s);
            }
        }
        Kind::Cockpit(_) => crate::mock_cockpit::setup_free(g),
        Kind::CockpitLand(_) => crate::mock_cockpit::setup_land(g),
        Kind::Hub | Kind::Surface | Kind::Dash => {}
    }
}

/// Draw the frame of the spec: the mock-up, or the screen as it is now.
pub fn render(fb: &mut Framebuffer, g: &Game, spec: &Spec) {
    if !spec.mock {
        crate::draw_frame(fb, g);
        return;
    }
    g.begin_frame();
    fb.clear();
    match spec.kind {
        Kind::Colony => draw_colony(fb, g),
        Kind::Hub => draw_hub(fb, g),
        Kind::Crew => draw_crew(fb, g),
        Kind::CrewMed => draw_crew_med(fb, g),
        Kind::Surface => draw_surface(fb, g),
        Kind::Station => draw_station(fb, g),
        Kind::Dash => draw_dash(fb, g),
        Kind::Cockpit(l) | Kind::CockpitLand(l) => crate::mock_cockpit::draw(fb, g, l),
    }
}

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

// ---- helpers on the game -----------------------------------------------------------------------

fn pct(v: f64) -> String {
    format!("{:.0} %", v * 100.0)
}

fn frac(a: f64, b: f64) -> f64 {
    if b.abs() < 1e-9 {
        0.0
    } else {
        (a / b).clamp(0.0, 1.0)
    }
}

// ---- 2. the F10 menu ---------------------------------------------------------------------------

fn draw_hub(fb: &mut Framebuffer, g: &Game) {
    use crate::hub;
    let sel = g.hub.sel.min(7);
    let th = theme(SPACE_COLORS[sel]);
    backdrop(fb, &th);
    topbar(fb, g, &th, "MENU PRINCIPAL", "HUIT ESPACES, UN SEUL MENU");
    let s = &g.sim;
    let o = hub::orientation(g);
    // Orientation strip.
    let (x, y, w, h) = (8, 36, 1264, 50);
    fill(fb, x, y, w, h, th.panel);
    frame(fb, x, y, w, h, th.border);
    fill(fb, x, y, 4, h, th.accent);
    txt(fb, x + 14, y + 7, "VOUS ETES", th.dim);
    let (main, sub) = match o.place.split_once(" (") {
        Some((m, r)) => (m.to_string(), r.trim_end_matches(')').to_string()),
        None => (o.place.clone(), String::new()),
    };
    big(fb, x + 14, y + 20, &fit(&main, 41), 2, INK);
    if !sub.is_empty() {
        txt(fb, x + 14, y + 38, &sub, th.dim);
    }
    let st = &s.ship;
    let mut cx = x + 700;
    for (label, v, good, note) in [
        (
            "CARBURANT",
            st.fuel / st.stats.fuel_cap.max(1.0),
            1.0,
            String::new(),
        ),
        (
            "COQUE",
            st.hull / st.stats.hull_max.max(1.0),
            1.0,
            String::new(),
        ),
        (
            "SOUTE",
            st.cargo.mass / st.cargo.cap_mass.max(1.0),
            0.0,
            format!("{:.1} T", st.cargo.mass / 1000.0),
        ),
    ] {
        txt(fb, cx, y + 7, label, th.dim);
        let c = if good > 0.5 { tint(v) } else { BLUE };
        seg_bar(fb, cx, y + 20, 84, 8, v, c);
        let t = if note.is_empty() { pct(v) } else { note };
        txt(fb, cx, y + 33, &t, INK);
        cx += 104;
    }
    txt(fb, cx + 4, y + 7, "CREDITS", th.dim);
    big(fb, cx + 4, y + 22, &crate::fmt::amount(s.credits), 2, WARN);
    txt(
        fb,
        cx + 4,
        y + 40,
        &format!("EQUIPAGE {}", s.crew.alive().count()),
        th.dim,
    );

    // The banner: what can I do here.
    let (bx, by, bw, bh) = (8, 94, 1264, 66);
    let inner = panel(
        fb,
        (bx, by, bw, bh),
        "QUE PUIS-JE FAIRE ICI ?  (CLIQUEZ)",
        &th,
    );
    let list = crate::context_menu::suggestions(g);
    for (i, sg) in list.iter().enumerate() {
        let col = i as i32 % 2;
        let row = i as i32 / 2;
        let sx = inner.0 + col * 626;
        let sy = inner.1 + row * 13;
        let off = sg.off.is_some();
        let hov = g.hit((sx, sy - 2, sx + 616, sy + 11), sg.act, None);
        let c = if off {
            th.dim
        } else if hov {
            INK
        } else {
            mix_color(th.accent, INK, 0.5)
        };
        txt_b(fb, sx, sy, ">", th.accent);
        txt(fb, sx + 14, sy, &fit(&sg.label, 66), c);
        txt_r(fb, sx + 600, sy, &cut(&sg.key, 10), th.dim);
    }

    // The eight spaces.
    let (lx, ly, lw) = (8, 170, 340);
    for (i, sp) in hub::SPACES.iter().enumerate() {
        let acc = SPACE_COLORS[i];
        let y = ly + i as i32 * 64;
        let on = i == sel;
        let hov = g.hit(
            (lx, y, lx + lw, y + 60),
            Act::Call(Game::click_hub_space, i),
            None,
        );
        let bg = if on {
            mix_color(th.panel, acc, 0.22)
        } else if hov {
            mix_color(th.panel, acc, 0.12)
        } else {
            th.panel
        };
        fill(fb, lx, y, lw, 60, bg);
        frame(
            fb,
            lx,
            y,
            lw,
            60,
            if on {
                acc
            } else {
                mix_color(th.border, acc, 0.3)
            },
        );
        fill(fb, lx, y, 5, 60, acc);
        // Number badge.
        fill(
            fb,
            lx + 14,
            y + 11,
            36,
            36,
            mix_color(0x0006_090D, acc, if on { 0.5 } else { 0.18 }),
        );
        frame(fb, lx + 14, y + 11, 36, 36, acc);
        big(
            fb,
            lx + 25,
            y + 20,
            &format!("{}", i + 1),
            3,
            if on { 0x0006_090D } else { acc },
        );
        txt_b(
            fb,
            lx + 62,
            y + 10,
            sp.name,
            if on { INK } else { mix_color(acc, INK, 0.45) },
        );
        txt(
            fb,
            lx + 62,
            y + 25,
            &fit(&hub::space_status(g, i), 32),
            th.dim,
        );
        let n = sp.entries.len();
        txt(
            fb,
            lx + 62,
            y + 39,
            &format!("{n} ECRANS"),
            mix_color(th.dim, acc, 0.4),
        );
        if on {
            for k in 0..8 {
                vl(fb, lx + lw - 22 + k, y + 30 - (8 - k), 2 * (8 - k) + 1, acc);
            }
        }
    }

    // The page of the selected space.
    let sp = &hub::SPACES[sel];
    let acc = SPACE_COLORS[sel];
    let (px, py, pw, ph) = (360, 170, 912, 526 - 8);
    fill(fb, px, py, pw, ph, th.panel);
    frame(fb, px, py, pw, ph, th.border);
    vgrad(
        fb,
        px + 1,
        py + 1,
        pw - 2,
        56,
        mix_color(th.panel, acc, 0.32),
        th.panel,
    );
    fill(fb, px + 1, py + 1, pw - 2, 2, acc);
    big(fb, px + 18, py + 14, sp.name, 3, mix_color(acc, INK, 0.2));
    for (k, l) in hub::wrap(sp.blurb, 112).iter().take(2).enumerate() {
        txt(
            fb,
            px + 18,
            py + 42 + k as i32 * 12,
            l,
            mix_color(th.dim, INK, 0.3),
        );
    }
    let first = py + 74;
    let rowh = (ph - 74 - 4) / sp.entries.len().max(1) as i32;
    let rowh = rowh.clamp(30, 44);
    for (j, e) in sp.entries.iter().enumerate() {
        let y = first + j as i32 * rowh;
        let why = hub::unavailable(g, e);
        let on = g.hub.page && j == g.hub.item;
        let hov = g.hit(
            (px + 6, y, px + pw - 6, y + rowh - 3),
            Act::Call(Game::click_hub_entry, sel * 100 + j),
            None,
        );
        let bg = if on {
            mix_color(th.panel, acc, 0.28)
        } else if hov {
            mix_color(th.panel, acc, 0.16)
        } else if j % 2 == 0 {
            th.panel
        } else {
            th.panel2
        };
        fill(fb, px + 6, y, pw - 12, rowh - 3, bg);
        let lit = if why.is_some() { th.dim } else { acc };
        fill(fb, px + 6, y, 3, rowh - 3, lit);
        // A small tile with the initial of the entry.
        fill(
            fb,
            px + 18,
            y + 6,
            rowh - 15,
            rowh - 15,
            mix_color(0x0006_090D, lit, 0.25),
        );
        frame(fb, px + 18, y + 6, rowh - 15, rowh - 15, lit);
        let ini = cut(e.label, 1);
        txt_c(
            fb,
            px + 18 + (rowh - 15) / 2,
            y + 6 + (rowh - 15) / 2 - 3,
            &ini,
            lit,
        );
        let tx = px + 18 + rowh - 15 + 12;
        let lc = if why.is_some() {
            th.dim
        } else if on || hov {
            INK
        } else {
            mix_color(acc, INK, 0.62)
        };
        txt_b(fb, tx, y + 7, e.label, lc);
        let room = ((px + pw - 20 - tw(e.key) - 10 - 14 - tx) / 8).max(10) as usize;
        match why {
            Some(wy) => txt(fb, tx, y + 21, &fit(wy, room), BAD),
            None => txt(fb, tx, y + 21, &fit(e.desc, room), th.dim),
        }
        // The key that does the same.
        let kw = tw(e.key) + 10;
        let kx = px + pw - 20 - kw;
        fill(fb, kx, y + 7, kw, 14, mix_color(0x0006_090D, lit, 0.2));
        frame(fb, kx, y + 7, kw, 14, lit);
        txt_c(fb, kx + kw / 2, y + 10, e.key, mix_color(lit, INK, 0.4));
    }
    // Reminders.
    let ry = py + ph - 26;
    if let Some(l) = o.todo.last() {
        hl(fb, px + 14, ry - 5, pw - 28, th.border);
        txt(fb, px + 18, ry, &fit(&format!("RAPPEL : {l}"), 112), th.dim);
    }
    cmdbar(
        fb,
        g,
        &th,
        &[
            ("1-8", "ESPACE"),
            ("HAUT/BAS", "CHOISIR"),
            ("ENTREE", "OUVRIR"),
            ("ECHAP", "REMONTER"),
            ("0", "TABLEAU DE BORD"),
            ("F1", "AIDE"),
        ],
    );
}

// ---- 1. the colony -----------------------------------------------------------------------------

fn draw_colony(fb: &mut Framebuffer, g: &Game) {
    let th = theme(SPACE_COLORS[6]);
    backdrop(fb, &th);
    let s = &g.sim;
    let Some(c) = s.colonies.get(g.col_idx) else {
        return;
    };
    topbar(
        fb,
        g,
        &th,
        &format!("COLONIE {}", c.name.to_uppercase()),
        &format!("SUR {}   ESPACE COLONIES", c.body_name.to_uppercase()),
    );
    // KPI tiles.
    let tiles = 6;
    let tw_ = (1264 - 8 * (tiles - 1)) / tiles;
    let docked = s.docked_colony() == Some(g.col_idx);
    let kpis: Vec<(&str, String, String, f64, u32)> = vec![
        (
            "POPULATION",
            format!("{:.0}", c.pop),
            format!("LOGEMENTS {:.0}", c.stats.housing),
            frac(c.pop, c.stats.housing),
            BLUE,
        ),
        (
            "MORAL",
            pct(c.happiness),
            format!("AGITATION {:.0} %", c.unrest * 100.0),
            c.happiness,
            tint(c.happiness),
        ),
        (
            "CAISSE",
            crate::fmt::amount(c.treasury),
            format!("+{:.0} / -{:.0} PAR JOUR", c.stats.income, c.stats.upkeep),
            frac(c.stats.income, c.stats.income + c.stats.upkeep),
            WARN,
        ),
        (
            "ENERGIE",
            format!("{:.1} MW", c.stats.power_supply / 1000.0),
            format!("DEMANDE {:.1} MW", c.stats.power_demand / 1000.0),
            frac(c.stats.power_supply, c.stats.power_demand.max(1.0)),
            tint(frac(c.stats.power_supply, c.stats.power_demand.max(1.0))),
        ),
        (
            "EMPLOIS",
            format!("{:.0}", c.stats.jobs),
            format!("TRAVAILLEURS {:.0}", c.stats.workers),
            frac(c.stats.jobs, c.stats.workers.max(1.0)),
            tint(frac(c.stats.jobs, c.stats.workers.max(1.0))),
        ),
        (
            "PRODUCTIVITE",
            pct(c.stats.prod_mult),
            format!(
                "VIVRES {:.0}  EAU {:.0} %",
                c.stats.food_sat * 100.0,
                c.stats.water_sat * 100.0
            ),
            c.stats.prod_mult.min(1.0),
            tint(c.stats.prod_mult.min(1.0)),
        ),
    ];
    for (i, (name, val, sub, f, col)) in kpis.iter().enumerate() {
        let x = 8 + i as i32 * (tw_ + 8);
        fill(fb, x, 36, tw_, 56, th.panel);
        frame(fb, x, 36, tw_, 56, th.border);
        fill(fb, x, 36, tw_, 2, *col);
        txt(fb, x + 8, 43, name, th.dim);
        big(fb, x + 8, 55, val, 2, INK);
        seg_bar(fb, x + 8, 73, tw_ - 16, 4, *f, *col);
        txt(
            fb,
            x + 8,
            81,
            &fit(sub, (tw_ as usize - 16) / 8),
            mix_color(th.dim, INK, 0.3),
        );
    }
    if docked {
        pill(fb, 1180, 96, "AMARRE", GOOD, 0x0006_090D);
    }
    // The zone tabs and the tabs of the works.
    let zones = ["BATIMENTS", "STOCKS", "POLITIQUES", "CONSTRUIRE"];
    let mut x = 8;
    for (i, z) in zones.iter().enumerate() {
        let on = g.col_focus as usize == i;
        let w = tw(z) + 20;
        let hov = g.hit((x, 98, x + w, 116), Act::Key(KeyCode::Tab), None);
        fill(
            fb,
            x,
            98,
            w,
            18,
            if on {
                th.accent
            } else if hov {
                th.panel2
            } else {
                th.panel
            },
        );
        frame(fb, x, 98, w, 18, th.border);
        txt_b(fb, x + 10, 103, z, if on { 0x0006_090D } else { INK });
        x += w + 4;
    }
    let works: [(&str, KeyCode); 6] = [
        ("F CHAINES", KeyCode::KeyF),
        ("H SANTE", KeyCode::KeyH),
        ("I INDUSTRIE", KeyCode::KeyI),
        ("G AGRICULTURE", KeyCode::KeyG),
        ("V VILLE", KeyCode::KeyV),
        ("L LOGISTIQUE", KeyCode::KeyL),
    ];
    let total: i32 = works.iter().map(|(l, _)| tw(l) + 24).sum();
    let mut x = 1272 - total;
    for (l, k) in works {
        let w = tw(l) + 20;
        let hov = g.hit((x, 98, x + w, 116), Act::Key(k), None);
        let (key, rest) = l.split_at(1);
        txt_b(fb, x + 6, 103, key, th.accent);
        txt(fb, x + 6 + 8, 103, rest, if hov { INK } else { th.dim });
        if hov {
            hl(fb, x + 6, 113, w - 12, th.accent);
        }
        x += w + 4;
    }
    // Epidemic / mores line.
    let worst = c
        .outbreaks
        .iter()
        .filter(|o| o.active())
        .max_by(|a, b| a.i.total_cmp(&b.i));
    let note = match worst {
        Some(o) => (
            format!(
                "EPIDEMIE : {}  {:.0} MALADES ({:.0} %)",
                g.data.pathogens[o.pathogen].name.to_uppercase(),
                o.i * c.pop,
                o.i * 100.0
            ),
            BAD,
        ),
        None => (
            format!(
                "MOEURS : {}   CROYANCE : {}",
                g.data.ethos.traits(&c.culture, 4).join(", ").to_uppercase(),
                g.data
                    .religions
                    .get(c.faith)
                    .map_or("-".to_string(), |r| r.name.to_uppercase())
            ),
            th.dim,
        ),
    };
    txt(fb, 8, 123, &fit(&note.0, 150), note.1);

    // Buildings.
    let foc = |f: u8| g.col_focus == f;
    let (bx, by, bw, bh) = (8, 136, 470, 262);
    let inner = panel(
        fb,
        (bx, by, bw, bh),
        &format!("BATIMENTS  ({})", c.buildings.len()),
        &th,
    );
    if foc(0) {
        frame(fb, bx, by, bw, bh, th.accent);
    }
    txt(fb, inner.0 + 6, inner.1, "NOM", th.dim);
    txt(fb, inner.0 + 214, inner.1, "ETAT", th.dim);
    txt(fb, inner.0 + 262, inner.1, "SANTE", th.dim);
    txt(fb, inner.0 + 316, inner.1, "RECETTE", th.dim);
    let rows = 16usize;
    let start = g
        .b_sel
        .saturating_sub(rows / 2)
        .min(c.buildings.len().saturating_sub(rows));
    for (line, k) in (start..(start + rows).min(c.buildings.len())).enumerate() {
        let b = &c.buildings[k];
        let d = &g.data.buildings[b.def];
        let y = inner.1 + 13 + line as i32 * 14;
        let on = foc(0) && k == g.b_sel;
        let hov = g.hit(
            (inner.0, y - 2, inner.0 + inner.2, y + 12),
            Act::Call(Game::click_building, k),
            None,
        );
        let bg = if on {
            mix_color(th.panel, th.accent, 0.30)
        } else if hov {
            mix_color(th.panel, th.accent, 0.16)
        } else if line % 2 == 0 {
            th.panel
        } else {
            th.panel2
        };
        fill(fb, inner.0, y - 2, inner.2, 14, bg);
        if on {
            fill(fb, inner.0, y - 2, 3, 14, th.accent);
        }
        txt(
            fb,
            inner.0 + 8,
            y,
            &fit(&d.name, 25),
            if on {
                INK
            } else {
                mix_color(th.accent, INK, 0.6)
            },
        );
        let (label, bgc, fgc) = if !b.built() {
            (format!("{:>3.0}%", b.progress * 100.0), WARN, 0x0006_090D)
        } else if !b.enabled {
            ("OFF".to_string(), 0x0040_4A50, INK)
        } else {
            ("ON".to_string(), GOOD, 0x0006_090D)
        };
        pill(fb, inner.0 + 212, y - 1, &label, bgc, fgc);
        thin_bar(fb, inner.0 + 262, y + 2, 44, b.health, tint(b.health));
        let recipe = if d.recipes.is_empty() {
            String::new()
        } else {
            g.data.recipes[g
                .data
                .recipe_index(&d.recipes[b.recipe % d.recipes.len()])
                .unwrap_or(0)]
            .name
            .clone()
        };
        txt(fb, inner.0 + 316, y, &fit(&recipe, 17), th.dim);
    }

    // Stocks.
    let (sx, sy, sw, sh) = (486, 136, 330, 262);
    let inner = panel(
        fb,
        (sx, sy, sw, sh),
        &format!(
            "STOCKS  {:.0} / {:.0} KG",
            c.total_stored(),
            c.stats.storage_cap.max(5000.0)
        ),
        &th,
    );
    if foc(1) {
        frame(fb, sx, sy, sw, sh, th.accent);
    }
    let items: Vec<usize> = (0..g.data.resources.len())
        .filter(|&i| c.storage[i] > 0.5 || (g.col_focus == 1 && i == g.s_sel))
        .collect();
    let maxq = items.iter().map(|&i| c.storage[i]).fold(1.0, f64::max);
    for (line, &i) in items.iter().take(16).enumerate() {
        let y = inner.1 + line as i32 * 14;
        let on = foc(1) && i == g.s_sel;
        let hov = g.hit(
            (inner.0, y - 2, inner.0 + inner.2, y + 12),
            Act::Call(Game::click_storage, i),
            None,
        );
        let bg = if on {
            mix_color(th.panel, th.accent, 0.30)
        } else if hov {
            mix_color(th.panel, th.accent, 0.16)
        } else if line % 2 == 0 {
            th.panel
        } else {
            th.panel2
        };
        fill(fb, inner.0, y - 2, inner.2, 14, bg);
        txt(
            fb,
            inner.0 + 6,
            y,
            &fit(&g.data.resources[i].name, 16),
            if on {
                INK
            } else {
                mix_color(th.accent, INK, 0.6)
            },
        );
        thin_bar(
            fb,
            inner.0 + 138,
            y + 2,
            74,
            c.storage[i] / maxq,
            mix_color(th.accent, INK, 0.1),
        );
        txt_r(
            fb,
            inner.0 + inner.2 - 4,
            y,
            &format!("{:.0}", c.storage[i]),
            INK,
        );
    }

    // Policies.
    let (ox, oy, ow, oh) = (824, 136, 448, 262);
    let inner = panel(fb, (ox, oy, ow, oh), "POLITIQUES", &th);
    if foc(2) {
        frame(fb, ox, oy, ow, oh, th.accent);
    }
    for (i, p) in g.data.policies.iter().enumerate() {
        let y = inner.1 + i as i32 * 22;
        let on = foc(2) && i == g.p_sel;
        let bg = if on {
            mix_color(th.panel, th.accent, 0.30)
        } else if i % 2 == 0 {
            th.panel
        } else {
            th.panel2
        };
        fill(fb, inner.0, y - 3, inner.2, 21, bg);
        g.hit(
            (inner.0, y - 3, inner.0 + 150, y + 17),
            Act::Call(Game::click_policy, i * 3 + 1),
            None,
        );
        txt(
            fb,
            inner.0 + 6,
            y + 1,
            &fit(&p.name, 20),
            if on {
                INK
            } else {
                mix_color(th.accent, INK, 0.6)
            },
        );
        // Level selector: < [|||.. ] > and the level name.
        let ax = inner.0 + 172;
        let hl_ = g.hit(
            (ax - 4, y - 3, ax + 14, y + 17),
            Act::Call(Game::click_policy, i * 3),
            None,
        );
        txt_b(fb, ax, y + 1, "<", if hl_ { INK } else { th.accent });
        let n = p.levels.len().max(1) as i32;
        for k in 0..n {
            let lit = k as usize <= c.policies[i];
            fill(
                fb,
                ax + 16 + k * 14,
                y + 2,
                12,
                8,
                if lit {
                    th.accent
                } else {
                    mix_color(0x0008_0C10, th.accent, 0.15)
                },
            );
        }
        let ex = ax + 16 + n * 14 + 2;
        let hr = g.hit(
            (ex - 2, y - 3, ex + 16, y + 17),
            Act::Call(Game::click_policy, i * 3 + 2),
            None,
        );
        txt_b(fb, ex, y + 1, ">", if hr { INK } else { th.accent });
        txt(
            fb,
            ex + 18,
            y + 1,
            &fit(&p.levels[c.policies[i]].0, 12),
            INK,
        );
    }

    // Build menu.
    let (cx, cy, cw, ch) = (8, 406, 470, 286);
    let inner = panel(fb, (cx, cy, cw, ch), "CONSTRUIRE  (ENTREE : BATIR)", &th);
    if foc(3) {
        frame(fb, cx, cy, cw, ch, th.accent);
    }
    let n = g.data.buildings.len();
    let show = 17usize;
    let st = g
        .build_sel
        .saturating_sub(show / 2)
        .min(n.saturating_sub(show));
    for (line, k) in (st..(st + show).min(n)).enumerate() {
        let d = &g.data.buildings[k];
        let y = inner.1 + line as i32 * 14;
        let ok = d.sites.iter().any(|t| c.tags.contains(t));
        let on = k == g.build_sel;
        let hov = g.hit(
            (inner.0, y - 2, inner.0 + inner.2, y + 12),
            Act::Call(Game::click_build, k),
            Some(Act::Key(KeyCode::Enter)),
        );
        let bg = if on {
            mix_color(th.panel, th.accent, 0.30)
        } else if hov {
            mix_color(th.panel, th.accent, 0.16)
        } else if line % 2 == 0 {
            th.panel
        } else {
            th.panel2
        };
        fill(fb, inner.0, y - 2, inner.2, 14, bg);
        let col = if !ok {
            BAD
        } else if on {
            INK
        } else {
            mix_color(th.accent, INK, 0.6)
        };
        txt(fb, inner.0 + 6, y, &fit(&d.name, 24), col);
        txt_r(fb, inner.0 + 232, y, &format!("{:.0} J", d.days), th.dim);
        let cost: Vec<String> = d
            .cost
            .iter()
            .take(2)
            .map(|(r, kg)| format!("{r} {kg:.0}"))
            .collect();
        txt(fb, inner.0 + 244, y, &fit(&cost.join(", "), 24), th.dim);
    }

    // Energy.
    let (ex, ey, ew, eh) = (486, 406, 330, 286);
    let inner = panel(fb, (ex, ey, ew, eh), "ENERGIE ET RESEAU", &th);
    let (sup, dem) = (c.stats.power_supply / 1000.0, c.stats.power_demand / 1000.0);
    let top = sup.max(dem).max(1.0) * 1.15;
    txt(fb, inner.0, inner.1, "PRODUCTION", th.dim);
    txt_r(
        fb,
        inner.0 + inner.2,
        inner.1,
        &format!("{sup:.1} MW"),
        GOOD,
    );
    thin_bar(fb, inner.0, inner.1 + 12, inner.2, sup / top, GOOD);
    txt(fb, inner.0, inner.1 + 26, "DEMANDE", th.dim);
    txt_r(
        fb,
        inner.0 + inner.2,
        inner.1 + 26,
        &format!("{dem:.1} MW"),
        WARN,
    );
    thin_bar(fb, inner.0, inner.1 + 38, inner.2, dem / top, WARN);
    txt(fb, inner.0, inner.1 + 52, "BATTERIE", th.dim);
    txt_r(
        fb,
        inner.0 + inner.2,
        inner.1 + 52,
        &format!(
            "{:.0} / {:.0} MWH",
            c.stats.battery_charge / 1000.0,
            c.stats.battery_cap / 1000.0
        ),
        BLUE,
    );
    thin_bar(
        fb,
        inner.0,
        inner.1 + 64,
        inner.2,
        frac(c.stats.battery_charge, c.stats.battery_cap),
        BLUE,
    );
    let bal = sup - dem;
    let (lab, col) = if bal >= 0.0 {
        (format!("EXCEDENT +{bal:.1} MW"), GOOD)
    } else {
        (format!("DEFICIT {bal:.1} MW"), BAD)
    };
    pill(
        fb,
        inner.0,
        inner.1 + 84,
        &lab,
        mix_color(0x0006_090D, col, 0.3),
        col,
    );
    txt(
        fb,
        inner.0 + 170,
        inner.1 + 86,
        &format!("PERTES {:.0} %", c.stats.grid_loss * 100.0),
        th.dim,
    );
    let stats = [
        ("RECHERCHE", format!("{:.0}", c.research)),
        ("TERRAFORMAGE", format!("{:.1} %", c.terraform * 100.0)),
        ("MORAL", pct(c.happiness)),
        ("AGITATION", format!("{:.0} %", c.unrest * 100.0)),
    ];
    for (k, (l, v)) in stats.iter().enumerate() {
        let sx = inner.0 + (k as i32 % 2) * 150;
        let sy = inner.1 + 118 + (k as i32 / 2) * 38;
        fill(fb, sx, sy, 142, 32, th.panel2);
        txt(fb, sx + 6, sy + 5, l, th.dim);
        big(fb, sx + 6, sy + 16, v, 1, INK);
    }

    // Log.
    let (lx, ly, lw, lh) = (824, 406, 448, 286);
    let inner = panel(fb, (lx, ly, lw, lh), "JOURNAL DE LA COLONIE", &th);
    for (i, (t, l)) in c.log.iter().rev().take(18).enumerate() {
        let y = inner.1 + i as i32 * 14;
        if i % 2 == 1 {
            fill(fb, inner.0, y - 2, inner.2, 14, th.panel2);
        }
        let day = (t / 86_400.0).floor() as i64;
        txt(fb, inner.0 + 4, y, &format!("J{day}"), th.accent);
        txt(
            fb,
            inner.0 + 52,
            y,
            &fit(&l.to_uppercase(), 48),
            mix_color(th.dim, INK, 0.45),
        );
    }
    cmdbar(
        fb,
        g,
        &th,
        &[
            ("TAB", "ZONE"),
            ("HAUT/BAS", "CHOISIR"),
            ("ESPACE", "ON/OFF"),
            ("R", "RECETTE"),
            ("X", "DEMOLIR"),
            ("ENTREE", "CONSTRUIRE"),
            ("T", "CAISSE"),
            ("U", "DECOLLER"),
            ("P", "PROJETS"),
        ],
    );
}

// ---- 3. the crew -------------------------------------------------------------------------------

fn crew_tabs(fb: &mut Framebuffer, g: &Game, th: &Theme, y: i32) {
    let mut x = 8;
    for (i, tab) in CrewTab::ALL.iter().enumerate() {
        let label = tab.label();
        let w = tw(label) + 22;
        let on = *tab == g.crew_tab;
        let hov = g.hit(
            (x, y, x + w, y + 20),
            Act::Call(Game::click_crew_tab, i),
            None,
        );
        fill(
            fb,
            x,
            y + if on { 0 } else { 3 },
            w,
            if on { 21 } else { 18 },
            if on {
                th.accent
            } else if hov {
                th.panel2
            } else {
                th.panel
            },
        );
        frame(
            fb,
            x,
            y + if on { 0 } else { 3 },
            w,
            if on { 21 } else { 18 },
            th.border,
        );
        txt_b(
            fb,
            x + 11,
            y + 7,
            label,
            if on {
                0x0006_090D
            } else {
                mix_color(th.accent, INK, 0.5)
            },
        );
        x += w + 3;
    }
    hl(fb, 8, y + 21, 1264, th.accent);
}

fn avatar(fb: &mut Framebuffer, x: i32, y: i32, size: i32, name: &str, c: u32) {
    fill(fb, x, y, size, size, mix_color(0x0006_090D, c, 0.35));
    frame(fb, x, y, size, size, c);
    let init: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect();
    let s = (size / 18).max(1);
    big(
        fb,
        x + (size - big_w(&init, s)) / 2,
        y + (size - 7 * s) / 2,
        &init.to_uppercase(),
        s,
        INK,
    );
}

fn draw_crew(fb: &mut Framebuffer, g: &Game) {
    let th = theme(SPACE_COLORS[3]);
    backdrop(fb, &th);
    let s = &g.sim;
    let crew = &s.crew;
    topbar(fb, g, &th, "EQUIPAGE", "ESPACE EQUIPAGE   LISTE ET FICHE");
    crew_tabs(fb, g, &th, 34);
    // Summary strip.
    let strip = [
        (
            "MEMBRES",
            format!("{}/{:.0}", crew.count(), s.ship.stats.crew_cap),
            BLUE,
        ),
        (
            "AIR",
            format!("{:.0} %", crew.air * 100.0),
            tint(f64::from(crew.air)),
        ),
        ("PAIE", format!("{:.0} CR/J", crew.daily_payroll()), WARN),
        ("CREDITS", crate::fmt::amount(s.credits), WARN),
    ];
    for (i, (l, v, c)) in strip.iter().enumerate() {
        let x = 8 + i as i32 * 158;
        fill(fb, x, 64, 150, 34, th.panel);
        frame(fb, x, 64, 150, 34, th.border);
        fill(fb, x, 64, 3, 34, *c);
        txt(fb, x + 10, 70, l, th.dim);
        big(fb, x + 10, 82, v, 1, INK);
    }
    // Roster.
    let (rx, ry, rw, rh) = (8, 106, 610, 300);
    let inner = panel(fb, (rx, ry, rw, rh), "LISTE DE L'EQUIPAGE", &th);
    txt(fb, inner.0 + 56, inner.1, "NOM / POSTE", th.dim);
    for (k, l) in ["SANTE", "MORAL", "FATIGUE", "FAIM"].iter().enumerate() {
        txt(fb, inner.0 + 236 + k as i32 * 64, inner.1, l, th.dim);
    }
    txt(fb, inner.0 + 496, inner.1, "ETAT", th.dim);
    for (i, p) in crew.members.iter().enumerate() {
        let y = inner.1 + 14 + i as i32 * 46;
        if y + 44 > ry + rh {
            break;
        }
        let on = i == g.crew_sel;
        let hov = g.hit(
            (inner.0, y, inner.0 + inner.2, y + 43),
            Act::Call(Game::click_crew, i),
            Some(Act::Call(Game::open_family_of, i)),
        );
        let bg = if on {
            mix_color(th.panel, th.accent, 0.28)
        } else if hov {
            mix_color(th.panel, th.accent, 0.14)
        } else if i % 2 == 0 {
            th.panel
        } else {
            th.panel2
        };
        fill(fb, inner.0, y, inner.2, 43, bg);
        if on {
            fill(fb, inner.0, y, 3, 43, th.accent);
        }
        let hue = SPACE_COLORS[(p.role + 3) % 8];
        avatar(fb, inner.0 + 8, y + 3, 37, &p.name, hue);
        txt_b(
            fb,
            inner.0 + 54,
            y + 8,
            &fit(&p.name, 20),
            if on {
                INK
            } else {
                mix_color(th.accent, INK, 0.6)
            },
        );
        txt(
            fb,
            inner.0 + 54,
            y + 22,
            &format!(
                "{}  QUART {}",
                g.data.roles[p.role].name.to_uppercase(),
                p.shift + 1
            ),
            th.dim,
        );
        for (k, (v, bad_high)) in [
            (f64::from(p.health), false),
            (f64::from(p.morale), false),
            (f64::from(p.fatigue), true),
            (f64::from(p.hunger), true),
        ]
        .iter()
        .enumerate()
        {
            let good = if *bad_high { 1.0 - v } else { *v };
            let bx = inner.0 + 236 + k as i32 * 64;
            thin_bar(fb, bx, y + 12, 54, *v, tint(good));
            txt(
                fb,
                bx,
                y + 22,
                &format!("{:.0}", v * 100.0),
                mix_color(th.dim, INK, 0.5),
            );
        }
        let (label, col) = if !p.alive {
            ("MORT".to_string(), 0x0060_6870)
        } else if let Some((d, left)) = p.disease {
            (
                format!("{} {:.0}J", g.data.diseases[d].name.to_uppercase(), left),
                BAD,
            )
        } else if p.on_duty(s.time) {
            ("AU TRAVAIL".to_string(), GOOD)
        } else {
            ("REPOS".to_string(), BLUE)
        };
        pill(
            fb,
            inner.0 + 494,
            y + 15,
            &fit(&label, 10),
            mix_color(0x0006_090D, col, 0.3),
            col,
        );
    }
    // Shifts and relations under the roster.
    let (qx, qy, qw, qh) = (8, 414, 610, 272);
    let inner = panel(fb, (qx, qy, qw, qh), "QUARTS ET RELATIONS", &th);
    for q in 0..3u8 {
        let x0 = inner.0 + i32::from(q) * 200;
        fill(fb, x0, inner.1, 194, 14, th.panel2);
        txt_b(
            fb,
            x0 + 6,
            inner.1 + 3,
            &format!("QUART {}", q + 1),
            th.accent,
        );
        let members: Vec<_> = crew.members.iter().filter(|p| p.shift == q).collect();
        if members.is_empty() {
            txt(fb, x0 + 6, inner.1 + 22, "PERSONNE", th.dim);
        }
        for (k, p) in members.iter().enumerate() {
            let y = inner.1 + 20 + k as i32 * 14;
            dot(
                fb,
                x0 + 8,
                y + 4,
                if p.on_duty(s.time) { GOOD } else { BLUE },
            );
            txt(
                fb,
                x0 + 18,
                y,
                &fit(&p.name.to_uppercase(), 16),
                mix_color(th.accent, INK, 0.5),
            );
        }
    }
    let ry2 = inner.1 + 70;
    txt_b(fb, inner.0, ry2, "RELATIONS", th.accent);
    hl(fb, inner.0 + 84, ry2 + 4, inner.2 - 84, th.border);
    let mut k = 0;
    for p in &crew.members {
        for (oid, rel) in p.relations.iter().take(5) {
            let Some(o) = crew.members.iter().find(|m| m.id == *oid) else {
                continue;
            };
            let y = ry2 + 16 + k * 16;
            if y + 12 > qy + qh - 8 {
                break;
            }
            txt(
                fb,
                inner.0,
                y,
                &format!(
                    "{}  >  {}",
                    fit(&p.name.to_uppercase(), 14),
                    fit(&o.name.to_uppercase(), 14)
                ),
                mix_color(th.accent, INK, 0.5),
            );
            // A bar centred on zero: red to the left, green to the right.
            let (cx0, half) = (inner.0 + 340, 100);
            fill(fb, cx0 - half, y + 1, 2 * half, 8, 0x0008_0C10);
            vl(fb, cx0, y - 1, 12, th.dim);
            let v = (f64::from(*rel) / 100.0).clamp(-1.0, 1.0);
            let wpx = (v.abs() * f64::from(half)) as i32;
            if v >= 0.0 {
                fill(fb, cx0, y + 1, wpx, 8, GOOD);
            } else {
                fill(fb, cx0 - wpx, y + 1, wpx, 8, BAD);
            }
            txt(fb, cx0 + half + 10, y, &format!("{rel:+}"), INK);
            k += 1;
        }
    }
    if k == 0 {
        txt(fb, inner.0, ry2 + 16, "AUCUNE RELATION NOTEE", th.dim);
    }
    // The sheet.
    let (fx, fy, fw, fh) = (626, 106, 646, 580);
    let Some(p) = crew.members.get(g.crew_sel) else {
        return;
    };
    let inner = panel(
        fb,
        (fx, fy, fw, fh),
        &format!("FICHE : {}", p.name.to_uppercase()),
        &th,
    );
    let hue = SPACE_COLORS[(p.role + 3) % 8];
    avatar(fb, inner.0, inner.1, 56, &p.name, hue);
    big(
        fb,
        inner.0 + 68,
        inner.1 + 2,
        &fit(&p.name.to_uppercase(), 24),
        2,
        INK,
    );
    txt(
        fb,
        inner.0 + 68,
        inner.1 + 24,
        &format!(
            "{}  -  CULTURE {}",
            g.data.roles[p.role].name.to_uppercase(),
            g.data.cultures[p.culture].name.to_uppercase()
        ),
        th.dim,
    );
    let mut x = inner.0 + 68;
    x += pill(
        fb,
        x,
        inner.1 + 40,
        &format!("AGE {:.0}", p.age),
        th.panel2,
        INK,
    ) + 6;
    x += pill(
        fb,
        x,
        inner.1 + 40,
        &format!("{:.0} CR/J", p.salary),
        th.panel2,
        WARN,
    ) + 6;
    pill(
        fb,
        x,
        inner.1 + 40,
        &format!("LOYAUTE {:.0} %", p.loyalty * 100.0),
        th.panel2,
        tint(f64::from(p.loyalty)),
    );
    // Vitals.
    let vy = inner.1 + 70;
    txt_b(fb, inner.0, vy, "ETAT", th.accent);
    hl(fb, inner.0 + 40, vy + 4, inner.2 - 40, th.border);
    for (k, (l, v, bad_high)) in [
        ("SANTE", f64::from(p.health), false),
        ("MORAL", f64::from(p.morale), false),
        ("FATIGUE", f64::from(p.fatigue), true),
        ("FAIM", f64::from(p.hunger), true),
    ]
    .iter()
    .enumerate()
    {
        let y = vy + 14 + k as i32 * 15;
        txt(fb, inner.0, y, l, th.dim);
        let good = if *bad_high { 1.0 - v } else { *v };
        seg_bar(fb, inner.0 + 72, y, 150, 8, *v, tint(good));
        txt(fb, inner.0 + 232, y, &format!("{:.0} %", v * 100.0), INK);
    }
    // Skills.
    let kx = inner.0 + 310;
    txt_b(fb, kx, vy, "COMPETENCES", th.accent);
    hl(fb, kx + 100, vy + 4, inner.2 - 410, th.border);
    let mut sk: Vec<(usize, f32)> = p.skills.iter().copied().enumerate().collect();
    sk.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (k, (si, v)) in sk.iter().take(6).enumerate() {
        let y = vy + 14 + k as i32 * 15;
        txt(
            fb,
            kx,
            y,
            &fit(&g.data.skills[*si].name.to_uppercase(), 12),
            th.dim,
        );
        seg_bar(fb, kx + 102, y, 150, 8, f64::from(*v) / 10.0, BLUE);
        txt(fb, kx + 262, y, &format!("{v:.1}"), INK);
    }
    // Traits.
    let ty = vy + 112;
    txt_b(fb, inner.0, ty, "TRAITS", th.accent);
    hl(fb, inner.0 + 60, ty + 4, inner.2 - 60, th.border);
    let mut x = inner.0;
    let mut y = ty + 14;
    for &t in &p.traits {
        let name = g.data.traits[t].name.to_uppercase();
        let w = tw(&name) + 10;
        if x + w > inner.0 + inner.2 {
            x = inner.0;
            y += 16;
        }
        pill(fb, x, y, &name, th.panel2, mix_color(th.accent, INK, 0.5));
        x += w + 5;
    }
    // Organs.
    let oy = ty + 52;
    txt_b(fb, inner.0, oy, "ORGANES", th.accent);
    hl(fb, inner.0 + 66, oy + 4, inner.2 - 66, th.border);
    for (k, (def, v)) in g.data.organs.iter().zip(&p.organs).enumerate() {
        let col = k as i32 / 6;
        let row = k as i32 % 6;
        let x = inner.0 + col * 214;
        let y = oy + 14 + row * 14;
        txt(fb, x, y, &fit(&def.name.to_uppercase(), 9), th.dim);
        let c = if *v <= 0.0 {
            BAD
        } else if *v < 0.5 {
            WARN
        } else {
            GOOD
        };
        thin_bar(fb, x + 76, y + 2, 90, f64::from(*v), c);
        txt_r(fb, x + 204, y, &format!("{:.0}", v * 100.0), INK);
    }
    // Equipment slots.
    let ey = oy + 110;
    txt_b(fb, inner.0, ey, "EQUIPEMENT", th.accent);
    hl(fb, inner.0 + 90, ey + 4, inner.2 - 90, th.border);
    for (slot, label) in ["ARME", "ARMURE", "OUTIL"].iter().enumerate() {
        let worn = p.gear.get(slot).copied().flatten();
        let x = inner.0 + slot as i32 * 206;
        let hov = g.hit(
            (x, ey + 14, x + 198, ey + 52),
            Act::Call(Game::click_gear_slot, slot),
            None,
        );
        fill(
            fb,
            x,
            ey + 14,
            198,
            38,
            if hov { th.panel2 } else { th.panel },
        );
        frame(
            fb,
            x,
            ey + 14,
            198,
            38,
            if worn.is_some() { th.accent } else { th.border },
        );
        txt(fb, x + 8, ey + 20, label, th.dim);
        txt(
            fb,
            x + 8,
            ey + 34,
            &fit(
                &worn.map_or("VIDE".to_string(), |gi| g.data.gear[gi].name.to_uppercase()),
                23,
            ),
            if worn.is_some() { INK } else { th.dim },
        );
    }
    let ay = ey + 60;
    txt(
        fb,
        inner.0,
        ay,
        &format!(
            "ATTAQUE +{:.1}   DEFENSE {:.0} %   RECOLTE +{:.0} %   INSTABILITE GENETIQUE {:.0} %",
            p.gear_stat(&g.data, "attack"),
            p.gear_stat(&g.data, "defense") * 100.0,
            p.gear_stat(&g.data, "gather") * 100.0,
            p.instability * 100.0
        ),
        th.dim,
    );
    cmdbar(
        fb,
        g,
        &th,
        &[
            ("HAUT/BAS", "CHOISIR"),
            ("GAUCHE/DROITE", "POSTE"),
            ("T", "QUART"),
            ("M", "MEDECINE"),
            ("O", "ORGANES"),
            ("E", "EQUIPEMENT"),
            ("G", "ARBRE"),
            ("J", "JOURNAL"),
            ("ECHAP", "RETOUR"),
        ],
    );
}

/// A human silhouette in filled shapes; each part takes the colour of its worst wound.
fn silhouette(fb: &mut Framebuffer, x: i32, y: i32, cols: &[u32; 6], cryo: bool) {
    // order: head, torso, left arm, right arm, left leg, right leg
    let edge = if cryo { 0x0050_C0E0 } else { 0x0020_2830 };
    let part = |fb: &mut Framebuffer, px: i32, py: i32, w: i32, h: i32, c: u32| {
        fill(fb, px, py, w, h, mix_color(0x0006_090D, c, 0.55));
        frame(fb, px, py, w, h, c);
        hl(fb, px + 1, py + 1, w - 2, mix_color(c, 0x00FF_FFFF, 0.25));
    };
    // head
    disc(fb, x + 50, y + 22, 20, edge);
    disc(
        fb,
        x + 50,
        y + 22,
        18,
        mix_color(0x0006_090D, cols[0], 0.55),
    );
    ring(fb, x + 50, y + 22, 18, cols[0]);
    ring(fb, x + 50, y + 22, 19, cols[0]);
    // neck
    fill(
        fb,
        x + 44,
        y + 40,
        12,
        8,
        mix_color(0x0006_090D, cols[1], 0.4),
    );
    part(fb, x + 24, y + 48, 52, 78, cols[1]);
    part(fb, x + 4, y + 50, 18, 84, cols[2]);
    part(fb, x + 78, y + 50, 18, 84, cols[3]);
    part(fb, x + 26, y + 128, 23, 110, cols[4]);
    part(fb, x + 51, y + 128, 23, 110, cols[5]);
    // a rib cage hint
    for k in 0..4 {
        hl(
            fb,
            x + 32,
            y + 60 + k * 9,
            36,
            mix_color(0x0006_090D, cols[1], 0.3),
        );
    }
}

fn draw_crew_med(fb: &mut Framebuffer, g: &Game) {
    let th = theme(SPACE_COLORS[3]);
    backdrop(fb, &th);
    let s = &g.sim;
    let crew = &s.crew;
    topbar(fb, g, &th, "EQUIPAGE", "ESPACE EQUIPAGE   MEDECINE");
    crew_tabs(fb, g, &th, 34);
    let (lx, ly, lw, lh) = (8, 64, 270, 620);
    let inner = panel(fb, (lx, ly, lw, lh), "PATIENTS", &th);
    for (i, p) in crew.members.iter().enumerate() {
        let y = inner.1 + i as i32 * 36;
        let on = i == g.crew_sel;
        let hov = g.hit(
            (inner.0, y, inner.0 + inner.2, y + 33),
            Act::Call(Game::click_crew, i),
            None,
        );
        fill(
            fb,
            inner.0,
            y,
            inner.2,
            33,
            if on {
                mix_color(th.panel, th.accent, 0.28)
            } else if hov {
                mix_color(th.panel, th.accent, 0.14)
            } else if i % 2 == 0 {
                th.panel
            } else {
                th.panel2
            },
        );
        if on {
            fill(fb, inner.0, y, 3, 33, th.accent);
        }
        let mark = if !p.alive {
            ("MORT", 0x0060_6870)
        } else if p.body.cryo {
            ("CRYO", 0x0050_C0E0)
        } else if p.body.unconscious() {
            ("INCONSCIENT", BAD)
        } else if p.body.is_hurt() || p.disease.is_some() {
            ("SOINS", WARN)
        } else {
            ("STABLE", GOOD)
        };
        avatar(fb, inner.0 + 8, y + 4, 25, &p.name, mark.1);
        txt_b(
            fb,
            inner.0 + 42,
            y + 6,
            &fit(&p.name, 17),
            if on {
                INK
            } else {
                mix_color(th.accent, INK, 0.6)
            },
        );
        pill(
            fb,
            inner.0 + 42,
            y + 19,
            mark.0,
            mix_color(0x0006_090D, mark.1, 0.3),
            mark.1,
        );
        txt_r(
            fb,
            inner.0 + inner.2 - 6,
            y + 8,
            &format!("{:.0}%", p.health * 100.0),
            tint(f64::from(p.health)),
        );
    }
    let Some(p) = crew.members.get(g.crew_sel) else {
        return;
    };
    // The body.
    let (bx, by, bw, bh) = (286, 64, 250, 620);
    let inner = panel(fb, (bx, by, bw, bh), &fit(&p.name.to_uppercase(), 28), &th);
    let part_of = |id: &str| g.data.part_index(id);
    let st = |id: &str| {
        let v = part_of(id).map_or(0.0, |k| {
            let wounds = p
                .body
                .wounds
                .iter()
                .filter(|w| w.part as usize == k)
                .map(|w| w.sev)
                .fold(0.0f32, f32::max);
            wounds.max(p.body.disability.get(k).copied().unwrap_or(0.0) * 0.8)
        });
        if v <= 0.0 {
            GOOD
        } else if v < 0.5 {
            WARN
        } else {
            BAD
        }
    };
    let cols = [
        st("tete"),
        st("torse"),
        st("bras_g"),
        st("bras_d"),
        st("jambe_g"),
        st("jambe_d"),
    ];
    silhouette(
        fb,
        inner.0 + (inner.2 - 100) / 2,
        inner.1 + 20,
        &cols,
        p.body.cryo,
    );
    // Legend.
    let ly2 = inner.1 + 290;
    for (k, (c, l)) in [(GOOD, "SAIN"), (WARN, "BLESSE"), (BAD, "GRAVE")]
        .iter()
        .enumerate()
    {
        let x = inner.0 + 6 + k as i32 * 72;
        fill(fb, x, ly2, 10, 10, *c);
        txt(fb, x + 14, ly2 + 1, l, th.dim);
    }
    txt(
        fb,
        inner.0 + 6,
        ly2 + 24,
        &format!("TEMPERATURE  {:.1} C", p.body.core_temp),
        INK,
    );
    txt(
        fb,
        inner.0 + 6,
        ly2 + 38,
        &format!("DOSE RAYONS  {:.2} SV", p.body.acute_dose),
        INK,
    );
    txt(
        fb,
        inner.0 + 6,
        ly2 + 52,
        &format!(
            "AGE {:.0}   {}",
            p.age,
            g.data.roles[p.role].name.to_uppercase()
        ),
        th.dim,
    );
    // Vitals.
    let (vx, vy, vw, vh) = (544, 64, 360, 620);
    let inner = panel(fb, (vx, vy, vw, vh), "SIGNES VITAUX", &th);
    let b = &p.body;
    let vitals = [
        ("SANTE", p.health, false),
        ("SANG", b.blood, false),
        ("OXYGENE", b.oxygen, false),
        ("DOULEUR", b.pain, true),
        ("TRAUMA", b.trauma, true),
        ("OS", b.bone, false),
        ("FATIGUE", p.fatigue, true),
        ("FAIM", p.hunger, true),
        ("SOIF", p.thirst, true),
    ];
    for (k, (l, v, bad_high)) in vitals.iter().enumerate() {
        let y = inner.1 + k as i32 * 24;
        txt(fb, inner.0, y, l, th.dim);
        let v = f64::from(*v);
        let good = if *bad_high { 1.0 - v } else { v };
        seg_bar(fb, inner.0 + 76, y - 1, 200, 9, v, tint(good));
        txt_r(
            fb,
            inner.0 + inner.2,
            y,
            &format!("{:.0} %", v * 100.0),
            if good < 0.3 { BAD } else { INK },
        );
    }
    let hy = inner.1 + 9 * 24 + 8;
    txt_b(fb, inner.0, hy, "HANDICAPS", th.accent);
    hl(fb, inner.0 + 80, hy + 4, inner.2 - 80, th.border);
    let mut any = false;
    for (k, d) in b.disability.iter().enumerate() {
        if *d > 0.05 {
            if let Some(def) = g.data.body_parts.get(k) {
                any = true;
                let y = hy + 14 + (k as i32 % 8) * 14;
                txt(fb, inner.0, y, &fit(&def.name.to_uppercase(), 14), th.dim);
                thin_bar(
                    fb,
                    inner.0 + 120,
                    y + 2,
                    140,
                    f64::from(*d),
                    tint(1.0 - f64::from(*d)),
                );
            }
        }
    }
    if !any {
        txt(fb, inner.0, hy + 16, "AUCUN", th.dim);
    }
    // Wounds.
    let (wx, wy, ww, wh) = (912, 64, 360, 620);
    let inner = panel(fb, (wx, wy, ww, wh), "BLESSURES ET SOINS", &th);
    let mut y = inner.1;
    if b.wounds.is_empty() {
        txt(fb, inner.0, y, "AUCUNE BLESSURE", GOOD);
    }
    for w in b.wounds.iter().take(10) {
        let name = g
            .data
            .body_parts
            .get(w.part as usize)
            .map_or("?".to_string(), |d| d.name.to_uppercase());
        let col = if w.sev >= 0.5 { BAD } else { WARN };
        fill(fb, inner.0, y, inner.2, 30, th.panel2);
        fill(fb, inner.0, y, 3, 30, col);
        txt_b(fb, inner.0 + 10, y + 5, &fit(&name, 20), INK);
        txt(
            fb,
            inner.0 + 10,
            y + 17,
            &format!("GRAVITE {:.0} %", w.sev * 100.0),
            th.dim,
        );
        thin_bar(
            fb,
            inner.0 + inner.2 - 96,
            y + 12,
            86,
            f64::from(w.sev),
            col,
        );
        y += 34;
    }
    let ay = inner.1 + 380;
    txt_b(fb, inner.0, ay, "SOINS (CLIQUEZ)", th.accent);
    hl(fb, inner.0 + 130, ay + 4, inner.2 - 130, th.border);
    for (k, (key, label)) in [
        ("A", "PREMIERS SOINS"),
        ("S", "OPERER"),
        ("R", "CHOC ELECTRIQUE"),
        ("C", "CRYOSTASE"),
        ("ENTREE", "DONNER LE MEDICAMENT"),
    ]
    .iter()
    .enumerate()
    {
        let yy = ay + 16 + k as i32 * 20;
        let code = match *key {
            "A" => KeyCode::KeyA,
            "S" => KeyCode::KeyS,
            "R" => KeyCode::KeyR,
            "C" => KeyCode::KeyC,
            _ => KeyCode::Enter,
        };
        let hov = g.hit(
            (inner.0, yy - 2, inner.0 + inner.2, yy + 14),
            Act::Key(code),
            None,
        );
        fill(
            fb,
            inner.0,
            yy - 2,
            inner.2,
            17,
            if hov { th.panel2 } else { th.panel },
        );
        let kw = tw(key) + 8;
        fill(fb, inner.0 + 4, yy, kw, 12, th.accent);
        txt_b(fb, inner.0 + 8, yy + 2, key, 0x0006_090D);
        txt(
            fb,
            inner.0 + kw + 14,
            yy + 2,
            label,
            if hov {
                INK
            } else {
                mix_color(th.accent, INK, 0.6)
            },
        );
    }
    cmdbar(
        fb,
        g,
        &th,
        &[
            ("HAUT/BAS", "PERSONNE"),
            ("TAB", "PLAIE"),
            ("GAUCHE/DROITE", "MEDICAMENT"),
            ("ENTREE", "DONNER"),
            ("A", "PREMIERS SOINS"),
            ("S", "OPERER"),
            ("C", "CRYOSTASE"),
            ("M", "RETOUR"),
        ],
    );
}

// ---- 4. the surface ----------------------------------------------------------------------------

fn draw_surface(fb: &mut Framebuffer, g: &Game) {
    use crate::surface_view as sv;
    let th = theme(SPACE_COLORS[1]);
    backdrop(fb, &th);
    let Some(map) = g.surf.as_ref() else {
        return;
    };
    let body = &g.sim.system.bodies[map.body];
    topbar(
        fb,
        g,
        &th,
        &format!("SURFACE : {}", body.name.to_uppercase()),
        &crate::hud::kind_label(body.kind),
    );
    // Layer tabs.
    let mut x = 8;
    for (i, l) in sv::LAYERS.iter().enumerate() {
        let w = tw(l) + 20;
        let on = i == g.surf_layer;
        let hov = g.hit(
            (x, 34, x + w, 54),
            Act::Call(Game::click_surface_layer, i),
            None,
        );
        fill(
            fb,
            x,
            34,
            w,
            20,
            if on {
                th.accent
            } else if hov {
                th.panel2
            } else {
                th.panel
            },
        );
        frame(fb, x, 34, w, 20, th.border);
        txt_b(
            fb,
            x + 10,
            40,
            l,
            if on {
                0x0006_090D
            } else {
                mix_color(th.accent, INK, 0.5)
            },
        );
        x += w + 3;
    }
    // The map: each cell has the tint of its ground.
    let (mx, my) = (8i32, 62i32);
    let cols_map = 100usize;
    let rows_map = 62usize;
    let (mw, mh) = (cols_map as i32 * 8 + 4, rows_map as i32 * 10 + 4);
    fill(fb, mx, my, mw + 8, mh + 8, 0x0004_0608);
    frame(fb, mx, my, mw + 8, mh + 8, th.border);
    let (ox, oy) = (mx + 6, my + 6);
    g.surf_geom
        .set((ox as usize / 8, oy as usize / 10, cols_map, rows_map));
    g.hit(
        (ox, oy, ox + cols_map as i32 * 8, oy + rows_map as i32 * 10),
        Act::At(Game::click_surface_map),
        None,
    );
    use vector_world::surface::{H, W};
    for r in 0..rows_map {
        let yy = r * H / rows_map;
        for c in 0..cols_map {
            let xx = c * W / cols_map;
            let (ch, col) = sv::cell(g, map, xx, yy);
            let (px, py) = (ox + c as i32 * 8, oy + r as i32 * 10);
            if col != 0 {
                fill(fb, px, py, 8, 10, scale_color(col, 0.20));
            }
            txt(fb, px, py + 1, &ch.to_string(), scale_color(col, 0.95));
        }
    }
    // Cursor.
    let (cc, cr) = (
        (g.surf_cx * cols_map / W).min(cols_map - 1) as i32,
        (g.surf_cy * rows_map / H).min(rows_map - 1) as i32,
    );
    let (cx, cy) = (ox + cc * 8, oy + cr * 10);
    for d in 0..2 {
        frame(
            fb,
            cx - 3 - d,
            cy - 3 - d,
            14 + 2 * d,
            16 + 2 * d,
            if d == 0 { INK } else { th.accent },
        );
    }
    hl(
        fb,
        ox,
        cy + 5,
        (cx - ox - 5).max(0),
        mix_color(th.accent, 0, 0.4),
    );
    hl(
        fb,
        cx + 12,
        cy + 5,
        (ox + cols_map as i32 * 8 - cx - 12).max(0),
        mix_color(th.accent, 0, 0.4),
    );
    for (name, ccx, ccy) in &g.surf_cities {
        let (c, r) = (
            (*ccx * cols_map / W).min(cols_map - 1) as i32,
            (*ccy * rows_map / H).min(rows_map - 1) as i32,
        );
        let (px, py) = (ox + c * 8, oy + r * 10);
        disc(fb, px + 4, py + 5, 4, 0x00FF_FFFF);
        ring(fb, px + 4, py + 5, 7, WARN);
        let label = name.to_uppercase();
        fill(fb, px + 14, py, tw(&label) + 8, 11, 0x0006_090D);
        txt(fb, px + 18, py + 1, &label, WARN);
    }
    // Scale ruler of latitudes.
    for k in 0..5 {
        let yy = oy + k * (rows_map as i32 * 10 / 4);
        txt_r(
            fb,
            mx + mw + 6,
            (yy - 3).min(my + mh - 8),
            &format!("{:+}", 90 - k * 45),
            th.dim,
        );
    }

    // Right: panels.
    let px = mx + mw + 20;
    let pw = 1272 - px;
    let sim = &g.sim;
    let (cxm, cym) = (g.surf_cx, g.surf_cy);
    let i = vector_world::surface::Surface::idx(cxm, cym);
    let biome = &g.data.biomes[map.biome_at(cxm, cym)];
    // Position card.
    let inner = panel(fb, (px, 62, pw, 128), "POSITION DU CURSEUR", &th);
    fill(
        fb,
        inner.0,
        inner.1 + 2,
        30,
        30,
        scale_color(biome.color, 0.4),
    );
    frame(fb, inner.0, inner.1 + 2, 30, 30, biome.color);
    txt_c(
        fb,
        inner.0 + 15,
        inner.1 + 14,
        &cut(&biome.glyph, 1),
        biome.color,
    );
    txt_b(
        fb,
        inner.0 + 40,
        inner.1 + 4,
        &fit(&biome.name.to_uppercase(), 24),
        INK,
    );
    txt(
        fb,
        inner.0 + 40,
        inner.1 + 18,
        &sv::lat_lon(cxm, cym),
        th.dim,
    );
    let e = f64::from(map.surface.elev[i]);
    let (tm, tn) = (f64::from(map.surface.temp[i]), sim.temp_now(map, cxm, cym));
    let rows = [
        (
            "ALTITUDE",
            if e < 0.0 {
                format!("-{:.0} M", -e * sv::RELIEF_M)
            } else {
                format!("{:.0} M", e * sv::RELIEF_M)
            },
        ),
        ("T ACTUELLE", format!("{:.0} C", tn - 273.15)),
        ("T MOYENNE", format!("{:.0} C", tm - 273.15)),
        (
            "HUMIDITE",
            format!("{:.0} %", f64::from(map.surface.hum[i]) * 100.0),
        ),
    ];
    for (k, (l, v)) in rows.iter().enumerate() {
        let y = inner.1 + 40 + k as i32 * 14;
        txt(fb, inner.0, y, l, th.dim);
        txt_r(fb, inner.0 + inner.2, y, v, INK);
    }
    // World card.
    let inner = panel(fb, (px, 198, pw, 232), "MONDE", &th);
    let ph = sim.system.physics(map.body, f64::from(map.surface.tilt));
    let tiles = [
        ("GRAVITE", format!("{:.1}", body.gravity), "M/S2"),
        ("PRESSION", format!("{:.2}", body.pressure), "BAR"),
        ("T EQUIL.", format!("{:.0}", body.temp - 273.15), "C"),
        ("MERS", format!("{:.0}", map.surface.sea_frac * 100.0), "%"),
    ];
    for (k, (l, v, u)) in tiles.iter().enumerate() {
        let x = inner.0 + (k as i32 % 2) * ((inner.2 - 6) / 2 + 6);
        let y = inner.1 + (k as i32 / 2) * 42;
        let w = (inner.2 - 6) / 2;
        fill(fb, x, y, w, 36, th.panel2);
        txt(fb, x + 6, y + 5, l, th.dim);
        big(fb, x + 6, y + 17, v, 2, INK);
        txt(fb, x + 8 + big_w(v, 2), y + 24, u, th.dim);
    }
    let year_days = sim.year_length(map.body) / 86_400.0;
    let day = match (ph.locked, ph.solar_day) {
        (Some((1, 1)), _) | (_, None) => "VERROUILLEE".to_string(),
        (_, Some(d)) => format!("{:.1} H", d / 3600.0),
    };
    let lines = [
        ("ANNEE", format!("{year_days:.0} J")),
        ("JOUR", day),
        ("DENSITE", format!("{:.0} KG/M3", ph.density)),
        ("LIBERATION", format!("{:.1} KM/S", ph.escape / 1000.0)),
        ("INTERIEUR", ph.activity.label().to_uppercase()),
        ("MAGNETISME", format!("{:.2} (TERRE=1)", ph.magnetic)),
    ];
    for (k, (l, v)) in lines.iter().enumerate() {
        let y = inner.1 + 92 + k as i32 * 14;
        txt(fb, inner.0, y, l, th.dim);
        txt_r(fb, inner.0 + inner.2, y, &fit(v, 22), INK);
    }
    // Ground card.
    let inner = panel(fb, (px, 438, pw, 246), "SOL ET GEOLOGIE", &th);
    let geo = &map.surface.geo;
    let plate = &geo.plates[geo.plate_at(cxm, cym)];
    let rock = match geo.rock_at(cxm, cym) {
        vector_world::geology::Rock::Igneous => "IGNEE",
        vector_world::geology::Rock::Sedimentary => "SEDIMENTAIRE",
        vector_world::geology::Rock::Metamorphic => "METAMORPHIQUE",
    };
    let lines = [
        (
            "PLAQUE",
            format!(
                "{} ({})",
                geo.plate_at(cxm, cym) + 1,
                if plate.oceanic {
                    "OCEANIQUE"
                } else {
                    "CONTINENTALE"
                }
            ),
        ),
        ("ROCHE", rock.to_string()),
        (
            "CHALEUR DU SOL",
            format!("{:.0} %", geo.heat_at(cxm, cym) * 100.0),
        ),
        (
            "NUAGES",
            if map.pressure < 0.01 {
                "PAS D'AIR".to_string()
            } else {
                format!("{:.0} %", f64::from(sim.cloud_at(map, cxm, cym)) * 100.0)
            },
        ),
    ];
    for (k, (l, v)) in lines.iter().enumerate() {
        let y = inner.1 + k as i32 * 14;
        txt(fb, inner.0, y, l, th.dim);
        txt_r(fb, inner.0 + inner.2, y, &fit(v, 22), INK);
    }
    if let Some(b) = geo.boundary_at(cxm, cym) {
        let t = match b {
            vector_world::geology::Boundary::Convergent => "FRONTIERE CONVERGENTE",
            vector_world::geology::Boundary::Divergent => "FRONTIERE DIVERGENTE",
            vector_world::geology::Boundary::Transform => "FAILLE TRANSFORMANTE",
        };
        pill(
            fb,
            inner.0,
            inner.1 + 62,
            t,
            mix_color(0x0006_090D, WARN, 0.3),
            WARN,
        );
    }
    txt_b(fb, inner.0, inner.1 + 86, "GISEMENTS PROCHES", th.accent);
    hl(fb, inner.0 + 144, inner.1 + 90, inner.2 - 144, th.border);
    let deps = geo.deposits_near((cxm, cym), 6.0);
    if deps.is_empty() {
        txt(fb, inner.0, inner.1 + 104, "AUCUN", th.dim);
    }
    for (k, d) in deps.iter().take(4).enumerate() {
        let name = g
            .data
            .geo_def(d.kind.id())
            .map_or(d.kind.id(), |def| def.name.as_str())
            .to_uppercase();
        let y = inner.1 + 104 + k as i32 * 16;
        txt(fb, inner.0, y, &fit(&name, 18), 0x00FF_E060);
        seg_bar(
            fb,
            inner.0 + 150,
            y,
            90,
            7,
            f64::from(d.richness),
            0x00FF_E060,
        );
        txt_r(
            fb,
            inner.0 + inner.2,
            y,
            &format!("{:.0} %", d.richness * 100.0),
            INK,
        );
    }
    cmdbar(
        fb,
        g,
        &th,
        &[
            ("FLECHES", "CURSEUR"),
            ("TAB", "COUCHE"),
            ("G", "GLOBE"),
            ("ENTREE", "FICHE"),
            ("E", "EXPEDITION"),
            ("ECHAP", "RETOUR"),
        ],
    );
}

// ---- 5. the station market -------------------------------------------------------------------

fn draw_station(fb: &mut Framebuffer, g: &Game) {
    use crate::game::MARKET_SORTS;
    let th = theme(SPACE_COLORS[4]);
    backdrop(fb, &th);
    let s = &g.sim;
    let Some(st) = s.docked else {
        return;
    };
    let Some(m) = s.markets.get(&st) else {
        return;
    };
    topbar(
        fb,
        g,
        &th,
        &s.bodies[st].name.to_uppercase(),
        "ESPACE COMMERCE",
    );
    // Tabs.
    let words = ["MARCHE", "NOIR", "BOURSE"]
        .into_iter()
        .chain(crate::trade_view::TABS);
    let mut x = 8;
    for (i, label) in words.enumerate() {
        let w = tw(label) + 18;
        let on = i == 0;
        let hov = g.hit(
            (x, 34, x + w, 54),
            Act::Call(Game::click_market_tab, i),
            None,
        );
        fill(
            fb,
            x,
            34,
            w,
            20,
            if on {
                th.accent
            } else if hov {
                th.panel2
            } else {
                th.panel
            },
        );
        frame(fb, x, 34, w, 20, th.border);
        txt_b(
            fb,
            x + 9,
            40,
            label,
            if on {
                0x0006_090D
            } else {
                mix_color(th.accent, INK, 0.5)
            },
        );
        x += w + 3;
    }
    // Header cards.
    let owner = match s.faction {
        Some(f) => (
            s.data.factions[f].name.to_uppercase(),
            format!(
                "REP {:+.0}   TAXE {:.0} %",
                s.reputation_with(Some(f)),
                s.tax_rate() * 100.0
            ),
            s.data.factions[f].color,
        ),
        None => ("STATION INDEPENDANTE".to_string(), String::new(), WARN),
    };
    fill(fb, 8, 62, 410, 48, th.panel);
    frame(fb, 8, 62, 410, 48, th.border);
    fill(fb, 8, 62, 4, 48, owner.2);
    txt_b(fb, 22, 67, &fit(&owner.0, 40), INK);
    txt(fb, 22, 80, &owner.1, th.dim);
    txt(
        fb,
        22,
        93,
        &fit(&s.data.industries[m.industry].name.to_uppercase(), 48),
        mix_color(th.dim, INK, 0.3),
    );
    let cargo = &s.ship.cargo;
    let chips = [
        ("CREDITS", crate::fmt::amount(s.credits), WARN),
        (
            "SOUTE",
            format!(
                "{:.1} / {:.1} T",
                cargo.mass / 1000.0,
                cargo.cap_mass / 1000.0
            ),
            BLUE,
        ),
        (
            "CARBURANT",
            pct(s.ship.fuel / s.ship.stats.fuel_cap.max(1.0)),
            tint(s.ship.fuel / s.ship.stats.fuel_cap.max(1.0)),
        ),
        (
            "COQUE",
            pct(s.ship.hull / s.ship.stats.hull_max.max(1.0)),
            tint(s.ship.hull / s.ship.stats.hull_max.max(1.0)),
        ),
    ];
    for (k, (l, v, c)) in chips.iter().enumerate() {
        let x = 426 + k as i32 * 140;
        fill(fb, x, 62, 132, 48, th.panel);
        frame(fb, x, 62, 132, 48, th.border);
        fill(fb, x, 62, 132, 2, *c);
        txt(fb, x + 8, 70, l, th.dim);
        big(fb, x + 8, 84, &fit(v, 15), 1, INK);
    }
    // The market table.
    let (tx, ty, tw_, th_) = (8, 118, 870, 570);
    let inner = panel(
        fb,
        (tx, ty, tw_, th_),
        &format!(
            "MARCHANDISES   TRI : {}   FILTRE : {}",
            MARKET_SORTS[g.market_sort % MARKET_SORTS.len()],
            g.market_filter_name().to_uppercase()
        ),
        &th,
    );
    let cols = [
        ("RESSOURCE", 8),
        ("STOCK", 230),
        ("ACHAT", 372),
        ("VENTE", 440),
        ("ECART", 506),
        ("30 JOURS", 580),
        ("SOUTE KG", 700),
    ];
    for (l, dx) in cols {
        txt(fb, inner.0 + dx, inner.1, l, th.dim);
    }
    hl(fb, inner.0, inner.1 + 11, inner.2, th.border);
    let order = g.market_order();
    let rows = 37usize;
    let sel_pos = order.iter().position(|&i| i == g.sel_res).unwrap_or(0);
    let startrow = sel_pos
        .saturating_sub(rows / 2)
        .min(order.len().saturating_sub(rows));
    let max_stock = order.iter().map(|&i| m.stock[i]).fold(1.0, f64::max);
    for (line, &i) in order.iter().skip(startrow).take(rows).enumerate() {
        let r = &g.data.resources[i];
        let y = inner.1 + 16 + line as i32 * 14;
        let on = i == g.sel_res;
        let hov = g.hit(
            (inner.0, y - 2, inner.0 + inner.2, y + 12),
            Act::Call(Game::click_sel_res, i),
            Some(Act::Key(KeyCode::Enter)),
        );
        fill(
            fb,
            inner.0,
            y - 2,
            inner.2,
            14,
            if on {
                mix_color(th.panel, th.accent, 0.32)
            } else if hov {
                mix_color(th.panel, th.accent, 0.16)
            } else if line % 2 == 0 {
                th.panel
            } else {
                th.panel2
            },
        );
        if on {
            fill(fb, inner.0, y - 2, 3, 14, th.accent);
        }
        let held = cargo.kg[i];
        txt(
            fb,
            inner.0 + 8,
            y,
            &fit(&r.name.to_uppercase(), 21),
            if on {
                INK
            } else if held > 0.0 {
                WARN
            } else {
                mix_color(th.accent, INK, 0.6)
            },
        );
        thin_bar(
            fb,
            inner.0 + 230,
            y + 2,
            60,
            m.stock[i] / max_stock,
            mix_color(th.accent, 0, 0.35),
        );
        txt_r(fb, inner.0 + 366, y, &format!("{:.0}", m.stock[i]), th.dim);
        let (ask, bid) = (m.ask(&g.data, i), m.bid(&g.data, i));
        txt_r(fb, inner.0 + 430, y, &crate::fmt::price(ask), INK);
        txt_r(
            fb,
            inner.0 + 498,
            y,
            &crate::fmt::price(bid),
            mix_color(INK, 0, 0.15),
        );
        let dev = ask / r.value.max(1e-9) - 1.0;
        let dc = if dev > 0.08 {
            BAD
        } else if dev < -0.08 {
            GOOD
        } else {
            th.dim
        };
        // Triangle for the trend of the price against the base value.
        let (tx0, ty0) = (inner.0 + 510, y + 1);
        if dev > 0.02 {
            for k in 0..5 {
                hl(fb, tx0 + 2 - k / 2, ty0 + 5 - k, 1 + k.min(2) * 2, dc);
            }
        } else if dev < -0.02 {
            for k in 0..5 {
                hl(
                    fb,
                    tx0 + 2 - (4 - k) / 2,
                    ty0 + k,
                    1 + (4 - k).min(2) * 2,
                    dc,
                );
            }
        } else {
            hl(fb, tx0, ty0 + 3, 6, dc);
        }
        txt(fb, inner.0 + 522, y, &format!("{:+.0} %", dev * 100.0), dc);
        SERIES.with(|c| {
            if let Some(v) = c.borrow().get(i) {
                spark(
                    fb,
                    inner.0 + 580,
                    y,
                    90,
                    10,
                    v,
                    mix_color(th.accent, INK, 0.3),
                    false,
                );
            }
        });
        if held > 0.0 {
            txt_r(fb, inner.0 + inner.2 - 4, y, &format!("{held:.0}"), WARN);
        }
    }
    // Detail of the selected good.
    let rr = &g.data.resources[g.sel_res];
    let (dx, dy, dw, dh) = (886, 118, 386, 570);
    let inner = panel(fb, (dx, dy, dw, dh), &fit(&rr.name.to_uppercase(), 36), &th);
    fill(fb, inner.0, inner.1, inner.2, 150, 0x0004_0608);
    frame(fb, inner.0, inner.1, inner.2, 150, th.border);
    if let Some(segs) = crate::wire_shape::resource_shape(&g.data, g.sel_res) {
        let t = crate::proto_view::anim_time();
        crate::wire_shape::draw(
            fb,
            &segs,
            (inner.0 + inner.2 / 2) as f32,
            (inner.1 + 75) as f32,
            130.0,
            t * 0.8 + 0.6,
            th.accent,
        );
    }
    txt(
        fb,
        inner.0 + 6,
        inner.1 + 6,
        &rr.category.to_uppercase(),
        th.dim,
    );
    let (ask, bid) = (m.ask(&g.data, g.sel_res), m.bid(&g.data, g.sel_res));
    let y0 = inner.1 + 160;
    for (k, (l, v, c)) in [
        ("ACHAT", crate::fmt::price(ask), INK),
        ("VENTE", crate::fmt::price(bid), GOOD),
        ("VALEUR DE BASE", crate::fmt::price(rr.value), th.dim),
    ]
    .iter()
    .enumerate()
    {
        let x = inner.0 + k as i32 * 120;
        txt(fb, x, y0, l, th.dim);
        big(fb, x, y0 + 12, v, 2, *c);
    }
    SERIES.with(|c| {
        if let Some(v) = c.borrow().get(g.sel_res) {
            txt(
                fb,
                inner.0,
                y0 + 40,
                "COURS DE REFERENCE SUR 30 JOURS",
                th.dim,
            );
            fill(fb, inner.0, y0 + 52, inner.2, 64, 0x0004_0608);
            frame(fb, inner.0, y0 + 52, inner.2, 64, th.border);
            spark(
                fb,
                inner.0 + 4,
                y0 + 56,
                inner.2 - 8,
                56,
                v,
                th.accent,
                true,
            );
        }
    });
    let y1 = y0 + 126;
    txt(fb, inner.0, y1, "STOCK DE LA STATION", th.dim);
    txt_r(
        fb,
        inner.0 + inner.2,
        y1,
        &format!("{:.0} KG", m.stock[g.sel_res]),
        INK,
    );
    txt(fb, inner.0, y1 + 14, "EN SOUTE", th.dim);
    txt_r(
        fb,
        inner.0 + inner.2,
        y1 + 14,
        &format!("{:.0} KG", cargo.kg[g.sel_res]),
        WARN,
    );
    // Action buttons (clickable text).
    let y2 = y1 + 44;
    txt_b(fb, inner.0, y2, "ACTIONS", th.accent);
    hl(fb, inner.0 + 64, y2 + 4, inner.2 - 64, th.border);
    let acts = [
        ("ENTREE", "VENDRE TOUT", KeyCode::Enter),
        ("S", "VENDRE 100 KG", KeyCode::KeyS),
        ("B", "ACHETER 100 KG", KeyCode::KeyB),
        ("F", "PLEIN DE CARBURANT", KeyCode::KeyF),
        ("R", "REPARER LA COQUE", KeyCode::KeyR),
        ("U", "DECOLLER", KeyCode::KeyU),
    ];
    for (k, (key, label, code)) in acts.iter().enumerate() {
        let y = y2 + 16 + k as i32 * 19;
        let hov = g.hit(
            (inner.0, y - 3, inner.0 + inner.2, y + 14),
            Act::Key(*code),
            None,
        );
        fill(
            fb,
            inner.0,
            y - 3,
            inner.2,
            17,
            if hov {
                mix_color(th.panel, th.accent, 0.3)
            } else {
                th.panel2
            },
        );
        let kw = tw(key) + 8;
        fill(fb, inner.0 + 4, y - 1, kw, 13, th.accent);
        txt_b(fb, inner.0 + 8, y + 1, key, 0x0006_090D);
        txt_b(
            fb,
            inner.0 + kw + 14,
            y + 1,
            label,
            if hov {
                INK
            } else {
                mix_color(th.accent, INK, 0.6)
            },
        );
    }
    cmdbar(
        fb,
        g,
        &th,
        &[
            ("HAUT/BAS", "CHOISIR"),
            ("V", "TRI"),
            ("O", "FILTRE"),
            ("ENTREE", "VENDRE TOUT"),
            ("S", "VENDRE"),
            ("B", "ACHETER"),
            ("C", "CONTRATS"),
            ("E", "BANQUE"),
            ("T", "BOURSE"),
            ("U", "DECOLLER"),
        ],
    );
}

// ---- 6. the dashboard --------------------------------------------------------------------------

fn draw_dash(fb: &mut Framebuffer, g: &Game) {
    let th = theme(SPACE_COLORS[0]);
    backdrop(fb, &th);
    let s = &g.sim;
    let ship = &s.ship;
    topbar(
        fb,
        g,
        &th,
        "TABLEAU DE BORD",
        &fit(&crate::hub::orientation(g).place, 64),
    );
    let alerts = g.alerts();
    let (crit, att, _) = vector_sim::Sim::alert_counts(&alerts);
    // "What can I do here" in one line.
    let list = crate::context_menu::suggestions(g);
    txt(fb, 10, 36, "ICI :", th.dim);
    let mut x = 52;
    for sg in &list {
        let label = format!("[{}]", sg.short);
        let hov = g.hit((x - 2, 33, x + tw(&label) + 2, 46), sg.act, None);
        txt_b(
            fb,
            x,
            36,
            &label,
            if sg.off.is_some() {
                th.dim
            } else if hov {
                INK
            } else {
                th.accent
            },
        );
        x += tw(&label) + 14;
    }
    let xs = [8, 432, 856];
    let cw = 416;
    let top = 52;

    // 1. The ship: gauges, the turning model, the modules.
    let h1 = 346;
    let inner = panel(fb, (xs[0], top, cw, h1), "1 VAISSEAU", &th);
    let hull_def = &g.data.hulls[ship.design.hull];
    txt_b(
        fb,
        inner.0,
        inner.1,
        &fit(&hull_def.name.to_uppercase(), 24),
        INK,
    );
    let state = if s.rogue.is_some() {
        "A PIED"
    } else if s.docked.is_some() {
        "AMARRE"
    } else if s.parked.is_some() {
        "POSE AU SOL"
    } else {
        "EN VOL"
    };
    let mut px = inner.0 + 8 + tw(&fit(&hull_def.name.to_uppercase(), 24)) + 8;
    px += pill(
        fb,
        px,
        inner.1 - 2,
        &hull_def.class.to_uppercase(),
        th.panel2,
        th.dim,
    ) + 4;
    pill(
        fb,
        px,
        inner.1 - 2,
        state,
        mix_color(0x0006_090D, GOOD, 0.35),
        GOOD,
    );
    let hull = ship.hull / ship.stats.hull_max.max(1.0);
    let fuel = ship.fuel / ship.stats.fuel_cap.max(1.0);
    let batt = ship.sys.battery_mj / ship.stats.battery_cap.max(1.0);
    let temp = ((ship.sys.temp_k - 150.0) / (vector_sim::design::T_SCRAM - 150.0)).clamp(0.0, 1.0);
    let shield = if s.effective.shield_cap > 0.0 {
        ship.sys.shield_pts / s.effective.shield_cap
    } else {
        0.0
    };
    let cargo = ship.cargo.mass / ship.cargo.cap_mass.max(1.0);
    let rows: [(&str, f64, String, u8); 6] = [
        ("COQUE", hull, pct(hull), 0),
        ("CARBURANT", fuel, pct(fuel), 0),
        ("BATTERIE", batt, pct(batt), 0),
        ("TEMPERATURE", temp, format!("{:.0} K", ship.sys.temp_k), 1),
        ("BOUCLIER", shield, pct(shield), 0),
        (
            "SOUTE",
            cargo,
            format!(
                "{:.1}/{:.0} T",
                ship.cargo.mass / 1000.0,
                ship.cargo.cap_mass / 1000.0
            ),
            2,
        ),
    ];
    for (k, (l, v, note, kind)) in rows.iter().enumerate() {
        let y = inner.1 + 20 + k as i32 * 17;
        txt(fb, inner.0, y, l, th.dim);
        match kind {
            2 => seg_bar(fb, inner.0 + 92, y - 1, 130, 8, *v, BLUE),
            k2 => seg_bar_z(fb, inner.0 + 92, y - 1, 130, 8, *v, *k2 == 1),
        }
        txt(fb, inner.0 + 230, y, &fit(note, 12), INK);
    }
    // The model of the ship, turning.
    let t = crate::proto_view::anim_time();
    fill(fb, inner.0 + 300, inner.1 + 16, 100, 108, 0x0004_0608);
    frame(fb, inner.0 + 300, inner.1 + 16, 100, 108, th.border);
    crate::ship_model::turntable(
        fb,
        &g.data,
        &ship.design,
        &crate::ship_model::State::of_player(s, if s.gear_down { 1.0 } else { 0.0 }),
        inner.0 + 350,
        inner.1 + 70,
        96,
        f64::from(t) * 0.6,
    );
    // Modules.
    let my = inner.1 + 132;
    txt_b(fb, inner.0, my, "MODULES", th.accent);
    hl(fb, inner.0 + 66, my + 4, inner.2 - 66, th.border);
    let mut shown = 0;
    for (i, m) in ship.design.slots.iter().enumerate() {
        let Some(m) = m else { continue };
        if shown >= 10 {
            break;
        }
        let hp = ship.sys.health[i];
        let col = shown / 5;
        let y = my + 14 + (shown % 5) * 16;
        let x0 = inner.0 + col * 204;
        let name = fit(&g.data.modules[*m].name.to_uppercase(), 13);
        dot(
            fb,
            x0 + 4,
            y + 4,
            if ship.sys.powered[i] {
                tint(hp / 0.8)
            } else {
                0x0050_5860
            },
        );
        txt(fb, x0 + 14, y, &name, mix_color(th.accent, INK, 0.5));
        thin_bar(fb, x0 + 122, y + 2, 50, hp, tint(hp / 0.8));
        txt_r(
            fb,
            x0 + 196,
            y,
            &format!("{:.0}", hp * 100.0),
            if hp < 0.3 { BAD } else { INK },
        );
        shown += 1;
    }
    let ey = my + 14 + 5 * 16 + 4;
    txt(
        fb,
        inner.0,
        ey,
        &format!(
            "ENERGIE {:.1}/{:.1} MW   DELTA-V {:.1} KM/S",
            ship.sys.demand_kw / 1e3,
            ship.sys.supply_kw / 1e3,
            ship.delta_v() / 1e3
        ),
        th.dim,
    );
    txt(
        fb,
        inner.0,
        ey + 12,
        &format!(
            "CHARGE {:.1} G   DOSE {:.2} MGY/H",
            s.structural_load_g(),
            s.dose_rate_aboard() * 1e3
        ),
        th.dim,
    );

    // 2. The crew.
    let y2 = top + h1 + 8;
    let h2 = 720 - 22 - 6 - y2;
    let inner = panel(fb, (xs[0], y2, cw, h2), "2 EQUIPAGE", &th);
    let n = s.crew.alive().count();
    big(fb, inner.0, inner.1 + 2, &format!("{n}"), 3, INK);
    txt(
        fb,
        inner.0 + 34,
        inner.1 + 2,
        &format!("A BORD SUR {:.0} COUCHETTES", ship.stats.crew_cap),
        th.dim,
    );
    txt(
        fb,
        inner.0 + 34,
        inner.1 + 14,
        &format!(
            "AIR {:.0} %   PAIE {:.0} CR/J",
            s.crew.air * 100.0,
            s.crew.daily_payroll()
        ),
        mix_color(th.dim, INK, 0.3),
    );
    txt(fb, inner.0 + 230, inner.1 + 30, "SANTE", th.dim);
    txt(fb, inner.0 + 310, inner.1 + 30, "MORAL", th.dim);
    if n < 3 {
        txt(
            fb,
            inner.0,
            inner.1 + 42 + 6 * 28 - 20,
            "N : RECRUTER A QUAI   K : OUVRIR L'EQUIPAGE",
            th.dim,
        );
    }
    for (k, p) in s.crew.members.iter().take(6).enumerate() {
        let y = inner.1 + 42 + k as i32 * 28;
        fill(
            fb,
            inner.0,
            y - 2,
            inner.2,
            26,
            if k % 2 == 0 { th.panel } else { th.panel2 },
        );
        avatar(
            fb,
            inner.0 + 4,
            y,
            22,
            &p.name,
            SPACE_COLORS[(p.role + 3) % 8],
        );
        txt_b(
            fb,
            inner.0 + 34,
            y + 2,
            &fit(&p.name.to_uppercase(), 20),
            mix_color(th.accent, INK, 0.6),
        );
        txt(
            fb,
            inner.0 + 34,
            y + 13,
            &g.data.roles[p.role].name.to_uppercase(),
            th.dim,
        );
        thin_bar(
            fb,
            inner.0 + 230,
            y + 8,
            70,
            f64::from(p.health),
            tint(f64::from(p.health)),
        );
        thin_bar(
            fb,
            inner.0 + 310,
            y + 8,
            70,
            f64::from(p.morale),
            tint(f64::from(p.morale)),
        );
    }

    // 3. Finance.
    let h3 = 196;
    let inner = panel(fb, (xs[1], top, cw, h3), "3 FINANCES", &th);
    let cr = crate::fmt::amount(s.credits);
    big(fb, inner.0, inner.1, &cr, 3, WARN);
    txt(
        fb,
        inner.0 + big_w(&cr, 3) + 8,
        inner.1 + 12,
        "CREDITS",
        th.dim,
    );
    let limit = s.credit_limit();
    let loan = s.bank.loan;
    txt(
        fb,
        inner.0,
        inner.1 + 30,
        &format!("VALEUR NETTE {}", crate::fmt::credits(s.net_worth())),
        INK,
    );
    let dette = if loan > 0.0 {
        format!(
            "DETTE {} / {}",
            crate::fmt::amount(loan),
            crate::fmt::amount(limit)
        )
    } else {
        format!("AUCUNE DETTE  PLAFOND {}", crate::fmt::amount(limit))
    };
    txt_r(
        fb,
        inner.0 + inner.2,
        inner.1 + 30,
        &dette,
        if loan > 0.0 { WARN } else { th.dim },
    );
    let reps = &s.bank.reports;
    let vals: Vec<f64> = reps
        .iter()
        .rev()
        .take(24)
        .rev()
        .map(|m| m.net_worth)
        .collect();
    fill(fb, inner.0, inner.1 + 46, inner.2, 50, 0x0004_0608);
    frame(fb, inner.0, inner.1 + 46, inner.2, 50, th.border);
    if vals.len() >= 2 {
        spark(
            fb,
            inner.0 + 3,
            inner.1 + 49,
            inner.2 - 6,
            44,
            &vals,
            WARN,
            true,
        );
    } else {
        txt_c(
            fb,
            inner.0 + inner.2 / 2,
            inner.1 + 68,
            "PAS ENCORE DE MOIS CLOS",
            th.dim,
        );
    }
    let sec = s.local_sector();
    let tiles = [
        ("TAUX", format!("{:.1} %", sec.policy_rate * 100.0), th.dim),
        (
            "INFLATION",
            format!("{:.1} %", sec.inflation * 100.0),
            if sec.inflation > 0.15 { WARN } else { GOOD },
        ),
        (
            "CHOMAGE",
            format!("{:.0} %", sec.unemployment * 100.0),
            th.dim,
        ),
        ("TITRES", crate::fmt::amount(s.portfolio_value()), BLUE),
    ];
    for (k, (l, v, c)) in tiles.iter().enumerate() {
        let tx = inner.0 + k as i32 * 100;
        fill(fb, tx, inner.1 + 104, 96, 30, th.panel2);
        txt(fb, tx + 5, inner.1 + 108, l, th.dim);
        txt_b(fb, tx + 5, inner.1 + 120, v, *c);
    }
    txt(
        fb,
        inner.0,
        inner.1 + 142,
        &format!(
            "ASSURANCE : {}",
            if s.insured() { "ACTIVE" } else { "AUCUNE" }
        ),
        if s.insured() { GOOD } else { th.dim },
    );

    // 4. The local world.
    let h5 = 150;
    let h4 = 720 - 22 - 6 - top - h3 - h5 - 16;
    let inner = panel(fb, (xs[1], top + h3 + 8, cw, h4), "4 MONDE LOCAL", &th);
    let terr = match s.faction {
        Some(f) => (
            s.data.factions[f].name.to_uppercase(),
            s.data.factions[f].color,
            format!("REP {:+.0}", s.reputation_with(Some(f))),
        ),
        None => ("ESPACE NON REVENDIQUE".to_string(), th.dim, String::new()),
    };
    let w1 = pill(
        fb,
        inner.0,
        inner.1,
        &fit(&terr.0, 30),
        mix_color(0x0006_090D, terr.1, 0.3),
        terr.1,
    );
    txt(fb, inner.0 + w1 + 8, inner.1 + 2, &terr.2, th.dim);
    txt_b(
        fb,
        inner.0,
        inner.1 + 22,
        &format!(
            "{}  CLASSE {}",
            s.system.name.to_uppercase(),
            s.system.star_class.letter()
        ),
        INK,
    );
    txt(
        fb,
        inner.0,
        inner.1 + 36,
        &format!("T+ {}", crate::fmt::duration(s.time)),
        th.dim,
    );
    if let Some((_, range)) = s.nearest_station() {
        txt(fb, inner.0, inner.1 + 50, "STATION LA PLUS PROCHE", th.dim);
        txt_r(
            fb,
            inner.0 + inner.2,
            inner.1 + 50,
            &crate::hud::fmt_dist(range),
            INK,
        );
    }
    txt(fb, inner.0, inner.1 + 64, "RECHERCHE", th.dim);
    txt_r(
        fb,
        inner.0 + inner.2,
        inner.1 + 64,
        &s.current_research.map_or("AUCUNE".to_string(), |t| {
            s.data.techs[t].name.to_uppercase()
        }),
        INK,
    );
    txt_b(fb, inner.0, inner.1 + 88, "NOUVELLES DU SECTEUR", th.accent);
    hl(fb, inner.0 + 168, inner.1 + 92, inner.2 - 168, th.border);
    for (k, nws) in s
        .headlines(8)
        .iter()
        .take(((h4 - 130) / 22).max(1) as usize)
        .enumerate()
    {
        let y = inner.1 + 104 + k as i32 * 22;
        fill(
            fb,
            inner.0,
            y - 2,
            inner.2,
            21,
            if k % 2 == 0 { th.panel } else { th.panel2 },
        );
        let pw_ = pill(fb, inner.0 + 2, y, nws.scale(), th.panel2, th.accent);
        let t = nws.text.to_uppercase();
        txt(
            fb,
            inner.0 + pw_ + 8,
            y + 2,
            &fit(&t, ((inner.2 - pw_ - 12) / 8) as usize),
            mix_color(th.dim, INK, 0.4),
        );
    }

    // 5. Contracts.
    let y5 = top + h3 + h4 + 16;
    let inner = panel(fb, (xs[1], y5, cw, h5), "5 CONTRATS ET MISSIONS", &th);
    if s.contracts.is_empty() {
        txt(fb, inner.0, inner.1, "AUCUN CONTRAT EN COURS", th.dim);
    }
    for (k, c) in s.contracts.iter().take(6).enumerate() {
        let y = inner.1 + k as i32 * 20;
        fill(
            fb,
            inner.0,
            y - 2,
            inner.2,
            19,
            if k % 2 == 0 { th.panel } else { th.panel2 },
        );
        let left = ((c.expires - s.time) / 86_400.0).max(0.0);
        txt(
            fb,
            inner.0 + 6,
            y + 2,
            &fit(&c.dest_name.to_uppercase(), 24),
            mix_color(th.accent, INK, 0.6),
        );
        txt_r(
            fb,
            inner.0 + inner.2 - 70,
            y + 2,
            &crate::fmt::amount(c.reward),
            WARN,
        );
        pill(
            fb,
            inner.0 + inner.2 - 62,
            y,
            &format!("{left:.0} J"),
            if left < 3.0 {
                mix_color(0x0006_090D, BAD, 0.4)
            } else {
                th.panel2
            },
            if left < 3.0 { BAD } else { th.dim },
        );
    }

    // 6. Alerts.
    let h6 = 206;
    let inner = panel(
        fb,
        (xs[2], top, cw, h6),
        &format!("6 ALERTES   {crit} CRIT  {att} ATT"),
        &th,
    );
    for (k, a) in alerts.iter().take(10).enumerate() {
        let y = inner.1 + k as i32 * 18;
        let c = match a.sev {
            vector_sim::alerts::Severity::Critical => BAD,
            vector_sim::alerts::Severity::Warning => WARN,
            vector_sim::alerts::Severity::Info => BLUE,
        };
        fill(
            fb,
            inner.0,
            y - 2,
            inner.2,
            17,
            if k % 2 == 0 { th.panel } else { th.panel2 },
        );
        dot(fb, inner.0 + 8, y + 5, c);
        pill(
            fb,
            inner.0 + 20,
            y,
            a.sev.label(),
            mix_color(0x0006_090D, c, 0.3),
            c,
        );
        txt(
            fb,
            inner.0 + 20 + tw(a.sev.label()) + 18,
            y + 2,
            &fit(&a.text.to_uppercase(), 34),
            if c == BLUE { th.dim } else { INK },
        );
    }
    if alerts.is_empty() {
        txt(fb, inner.0, inner.1, "RIEN A SIGNALER", GOOD);
    }

    // 7. Colonies.
    let h7 = 140;
    let inner = panel(
        fb,
        (xs[2], top + h6 + 8, cw, h7),
        "7 COLONIES ET PROJETS",
        &th,
    );
    if s.colonies.is_empty() {
        txt(fb, inner.0, inner.1, "AUCUNE COLONIE", th.dim);
        txt(
            fb,
            inner.0,
            inner.1 + 14,
            "O : FONDER UNE COLONIE SUR LE CORPS CIBLE",
            th.dim,
        );
    }
    for (k, c) in s.colonies.iter().take(3).enumerate() {
        let y = inner.1 + k as i32 * 34;
        txt_b(fb, inner.0, y, &fit(&c.name.to_uppercase(), 24), INK);
        txt_r(
            fb,
            inner.0 + inner.2,
            y,
            &format!("POP {:.0}", c.pop),
            th.dim,
        );
        seg_bar(fb, inner.0, y + 14, 120, 6, c.happiness, tint(c.happiness));
        txt(
            fb,
            inner.0 + 128,
            y + 13,
            &format!("MORAL {:.0} %", c.happiness * 100.0),
            th.dim,
        );
    }

    // 8. On foot.
    let y8 = top + h6 + h7 + 16;
    let inner = panel(fb, (xs[2], y8, cw, 58), "8 EXPLORATION A PIED", &th);
    txt(
        fb,
        inner.0,
        inner.1,
        if s.rogue.is_some() {
            "EQUIPE DEHORS"
        } else {
            "AUCUNE SORTIE EN COURS"
        },
        th.dim,
    );

    // 9. The mind of the AI.
    let y9 = y8 + 58 + 8;
    let h9 = 720 - 22 - 6 - y9;
    let inner = panel(fb, (xs[2], y9, cw, h9), "9 ESPRIT DE L'IA", &th);
    let thr = s.mind_throttle();
    txt(
        fb,
        inner.0,
        inner.1,
        &format!("CALCUL {:.1}", s.mind_capacity()),
        INK,
    );
    txt_r(
        fb,
        inner.0 + inner.2,
        inner.1,
        &format!("RALENTISSEMENT {:.0} %", (1.0 - thr) * 100.0),
        if thr < 0.7 { WARN } else { GOOD },
    );
    let (used, mem) = (s.mind_memory_used(), s.mind_memory().max(1));
    txt(fb, inner.0, inner.1 + 16, "MEMOIRE", th.dim);
    seg_bar(
        fb,
        inner.0 + 72,
        inner.1 + 15,
        150,
        8,
        f64::from(used) / f64::from(mem),
        BLUE,
    );
    txt(
        fb,
        inner.0 + 232,
        inner.1 + 16,
        &format!("{used}/{mem}"),
        INK,
    );
    txt(fb, inner.0, inner.1 + 30, "MALWARE", th.dim);
    seg_bar(
        fb,
        inner.0 + 72,
        inner.1 + 29,
        150,
        8,
        s.mind.malware,
        tint(1.0 - s.mind.malware * 2.0),
    );
    txt(
        fb,
        inner.0 + 232,
        inner.1 + 30,
        &format!("{:.0} %", s.mind.malware * 100.0),
        INK,
    );
    for (t, task) in g.data.mind.tasks.iter().enumerate() {
        let y = inner.1 + 50 + (t as i32 % 4) * 16;
        if y + 10 > y9 + h9 - 6 {
            break;
        }
        let x0 = inner.0 + (t as i32 / 4) * 204;
        let share = s.mind_share(t);
        txt(fb, x0, y, &fit(&task.name.to_uppercase(), 9), th.dim);
        thin_bar(fb, x0 + 76, y + 2, 80, share, 0x00B4_8CFF);
        txt_r(fb, x0 + 196, y, &format!("{:.0}%", share * 100.0), INK);
    }
    cmdbar(
        fb,
        g,
        &th,
        &[
            ("TAB", "PANNEAU SUIVANT"),
            ("1-9", "ALLER AU PANNEAU"),
            ("HAUT/BAS", "LIGNE"),
            ("ENTREE", "OUVRIR"),
            ("ECHAP", "FERMER"),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mock_has_its_current_screen_to_compare_with() {
        for n in [
            "colony",
            "hub",
            "crew",
            "crew_med",
            "surface",
            "station",
            "dash",
            "cockpit_a",
            "cockpit_b",
            "cockpit_c",
            "cockpit_c_open",
            "cockpit_land",
            "cockpit_land_b",
            "cockpit_land_c",
            "cockpit_land_c_open",
        ] {
            let m = resolve(&format!("mock_{n}")).unwrap_or_else(|| panic!("mock_{n}"));
            let w = resolve(&format!("now_{n}")).unwrap_or_else(|| panic!("now_{n}"));
            assert!(m.mock && !w.mock);
            assert_eq!(m.base, w.base);
        }
        assert!(resolve("colony").is_none());
        assert!(resolve("mock_nothing").is_none());
    }

    #[test]
    fn text_helpers_cut_and_fit() {
        assert_eq!(fit("ABCDEFGH", 8), "ABCDEFGH");
        assert_eq!(fit("ABCDEFGHI", 8), "ABCDEFG.");
        assert_eq!(cut("ABC", 2), "AB");
        assert_eq!(tw("ABC"), 24);
    }
}
