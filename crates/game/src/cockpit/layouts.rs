//! The arrangements of the cockpit (A, B, C) for any window size. The modules (systems,
//! commands, flight instruments, radar, screens, throttle...) stretch between a minimum and a
//! maximum size (`spread`), and the secondary organs fall away inside each module when the
//! rectangle is too small, so that nothing overflows from 1024x576 up to full screen.
use super::controls::{self, fkeys, handle, sas_block, throttle, toggles, toggles_height, FKEYS};
use super::draw::*;
use super::instruments::{
    descent_screen, dock_screen, flight_cluster, lamp_panel, nav_screen, pilot_screen, plan_screen,
    radar, systems, target_panel,
};
use super::{prof, Disposition, Id, Tel};
use crate::game::Game;
use crate::hud::View;
use crate::mouse::Act;
use vector_render::Framebuffer;
use winit::keyboard::KeyCode;

/// Smallest window (width, height) where A and B still make sense; under it they become C.
pub const MIN_AB: (i32, i32) = (940, 500);
/// Smallest window where the cockpit is drawn at all; under it the old HUD is.
pub const MIN_C: (i32, i32) = (640, 380);

/// The height of the strip at the top of the window (the menu buttons of the game live there)
/// and the rows at the bottom the ticker and the command bar take.
fn insets(g: &Game) -> (i32, i32) {
    let bottom = 10 * (i32::from(g.settings.hints) + i32::from(g.settings.ticker));
    (12, bottom)
}

/// Share `avail` between modules of (min, preferred, max, weight) widths: the preferred sizes
/// shrink towards the minimums when the room is short, and grow (by weight, up to the maximum)
/// when there is more. Returns the sizes and the room left over.
pub fn spread<const N: usize>(avail: i32, specs: [(i32, i32, i32, i32); N]) -> ([i32; N], i32) {
    let mut out = [0; N];
    let pref: i32 = specs.iter().map(|s| s.1).sum();
    if pref >= avail {
        let slack: i32 = specs.iter().map(|s| s.1 - s.0).sum();
        let deficit = (pref - avail).min(slack);
        for (o, s) in out.iter_mut().zip(specs.iter()) {
            *o = s.1
                - if slack > 0 {
                    (s.1 - s.0) * deficit / slack
                } else {
                    0
                };
        }
        // Rounding: give what is missing to the largest module.
        let sum: i32 = out.iter().sum();
        if sum > avail {
            if let Some(i) = (0..N).max_by_key(|&i| out[i]) {
                out[i] -= sum - avail;
            }
        }
        return (out, 0);
    }
    for (o, s) in out.iter_mut().zip(specs.iter()) {
        *o = s.1;
    }
    let mut left = avail - pref;
    for _ in 0..4 {
        let wsum: i32 = (0..N)
            .filter(|&i| out[i] < specs[i].2)
            .map(|i| specs[i].3)
            .sum();
        if left <= 0 || wsum == 0 {
            break;
        }
        let mut given = 0;
        for i in 0..N {
            if out[i] < specs[i].2 {
                let add = (left * specs[i].3 / wsum).min(specs[i].2 - out[i]);
                out[i] += add;
                given += add;
            }
        }
        left -= given;
        if given == 0 {
            break;
        }
    }
    (out, left)
}

// ---- the scene ---------------------------------------------------------------------------------

fn scene_content(fb: &mut Framebuffer, v: &View) {
    if v.map {
        crate::hud::draw_map(fb, v);
    } else {
        crate::hud::draw_cockpit_scene(fb, v);
        crate::landing_view::draw_markers(fb, v);
    }
}

/// The 3D scene in a buffer the size of the window (kept between frames), copied into the frame;
/// the zones the scene declares (bodies and ships to click) are moved to where the window is.
fn scene(fb: &mut Framebuffer, v: &View, rc: Rc) {
    let (x, y, w, h) = rc;
    if w <= 0 || h <= 0 {
        return;
    }
    if x == 0 && y == 0 && w == fb.w as i32 && h == fb.h as i32 {
        scene_content(fb, v);
        return;
    }
    let started = std::time::Instant::now();
    let g = v.game;
    let mut sub = g
        .cockpit
        .scene
        .borrow_mut()
        .take()
        .filter(|s| s.w == w as usize && s.h == h as usize)
        .unwrap_or_else(|| Framebuffer::new(w as usize, h as usize));
    sub.clear();
    sub.clipped = 0;
    sub.clipped_first = None;
    let before = g.pointer.hits.borrow().len();
    scene_content(&mut sub, v);
    for hit in g.pointer.hits.borrow_mut().iter_mut().skip(before) {
        hit.rect = (
            hit.rect.0 + x,
            hit.rect.1 + y,
            hit.rect.2 + x,
            hit.rect.3 + y,
        );
    }
    // Copy the rows that are inside the frame.
    let (x0, x1) = (x.max(0), (x + w).min(fb.w as i32));
    if x1 > x0 {
        for row in 0..h {
            let dy = y + row;
            if dy < 0 || dy >= fb.h as i32 {
                continue;
            }
            let src = row as usize * sub.w + (x0 - x) as usize;
            let dst = dy as usize * fb.w + x0 as usize;
            let n = (x1 - x0) as usize;
            fb.px[dst..dst + n].copy_from_slice(&sub.px[src..src + n]);
        }
    }
    fb.clipped += sub.clipped;
    if fb.clipped_first.is_none() {
        fb.clipped_first = sub.clipped_first.take();
    }
    *g.cockpit.scene.borrow_mut() = Some(sub);
    let _ = started;
}

// ---- glass texts -------------------------------------------------------------------------------

/// A short glass text line (additive, like the HUD of the game).
fn glass(fb: &mut Framebuffer, x: i32, y: i32, s: &str, c: u32) {
    fb.text_at(x, y, s, c);
}

/// The lines the old HUD wrote at the top left that the console does not show: autopilot, jump,
/// dangers, the law, the siege, a station in range. (text, colour)
fn status_lines(g: &Game, max: usize) -> Vec<(String, u32)> {
    use crate::hud::{fmt_dist, AMBER, DIM, GREEN, RED};
    let sim = &g.sim;
    let mut out: Vec<(String, u32)> = Vec::with_capacity(max + 2);
    if let Some(ap) = &sim.autopilot {
        let (eta, dv) = ap.eta_and_dv();
        out.push((
            format!(
                "PILOTE AUTO -> {}  RESTE {}  DV {:.0} KM/S",
                sim.bodies[ap.target].name.to_uppercase(),
                crate::fmt::duration(eta),
                dv / 1e3
            ),
            AMBER,
        ));
    }
    if let Some(j) = &sim.jump {
        out.push((
            if j.in_transit {
                let done = 1.0 - j.transit_left / j.total_transit.max(1.0);
                format!(
                    "TRANSIT FTL {:.0}%  RESTE {}",
                    done * 100.0,
                    crate::fmt::duration(j.transit_left)
                )
            } else {
                format!(
                    "CHARGE FTL {:.0}S  [RETOUR ARR.] ANNULER",
                    j.charge_left.max(0.0)
                )
            },
            AMBER,
        ));
    }
    let env = sim.environment_at(sim.ship.pos);
    let mut danger = Vec::new();
    if env.radiation > 0.0 {
        danger.push(format!("RADIATIONS {:.1} GY/H", env.radiation));
    }
    if env.ion > 0.0 {
        danger.push("TEMPETE IONIQUE".to_string());
    }
    if env.gravity.length() > 0.5 {
        danger.push(format!("PUITS {:.1} M/S2", env.gravity.length()));
    }
    if env.sensor_mult < 0.9 {
        danger.push("CAPTEURS BRUITES".to_string());
    }
    if env.agility_mult < 0.9 {
        danger.push("INTERFERENCES".to_string());
    }
    if env.rift > 0.0 {
        danger.push("FAILLE SPATIALE".to_string());
    }
    if !danger.is_empty() {
        out.push((format!("DANGER : {}", danger.join(" / ")), RED));
    }
    if sim.is_wanted() {
        out.push(("RECHERCHE DANS CE TERRITOIRE !".into(), RED));
    }
    if sim.here_besieged().is_some() {
        out.push(("BLOCUS DU SECTEUR".into(), AMBER));
    }
    if let Some(st) = sim.docked {
        out.push((
            format!("AMARRE : {}", sim.bodies[st].name.to_uppercase()),
            GREEN,
        ));
    } else if let Some((_, range)) = sim.nearest_station() {
        if range < vector_sim::DOCK_RANGE * 4.0 {
            out.push((
                format!("STATION A {} - [D] AMARRER", fmt_dist(range)),
                AMBER,
            ));
        }
    }
    if sim.ship.hull <= 0.0 {
        out.push(("*** COQUE DETRUITE ***".into(), RED));
    }
    if sim.rules.is_modified() {
        out.push(("PARTIE MODIFIEE".into(), AMBER));
    }
    // The bank line always stays: credits, hold and the days the cash pays the crew.
    out.truncate(max.saturating_sub(1));
    let payroll = sim.crew.daily_payroll();
    let (pay, pay_col) = if payroll > 0.0 && sim.credits < payroll * 15.0 {
        let days = (sim.credits / payroll).max(0.0);
        (
            format!("   PAIE : {days:.0} J"),
            if days < 5.0 { RED } else { AMBER },
        )
    } else {
        (String::new(), DIM)
    };
    out.push((
        format!(
            "CREDITS {:.0}   SOUTE {:.0}/{:.0} KG{pay}",
            sim.credits, sim.ship.cargo.mass, sim.ship.cargo.cap_mass
        ),
        if sim.ship.cargo.is_full() {
            RED
        } else if !pay.is_empty() {
            pay_col
        } else {
            DIM
        },
    ));
    out
}

/// System, frame of reference, time and the status lines in the corners of the window `win`;
/// `margin` keeps the texts clear of the struts of the canopy.
fn glass_texts(fb: &mut Framebuffer, g: &Game, win: Rc, margin: i32) {
    let (x, y, w, h) = win;
    let sim = &g.sim;
    let green = crate::hud::GREEN;
    let head = format!(
        "{} ({})  CLASSE {}",
        sim.system.name,
        sim.system.designation,
        sim.system.star_class.letter()
    );
    let frame_name = sim
        .ref_body
        .map_or("ETOILE".to_string(), |i| sim.bodies[i].name.clone());
    let refl = format!("REF {frame_name}");
    let lines = status_lines(g, ((h / 3 - 40) / 11).clamp(0, 6) as usize);
    let widest = lines
        .iter()
        .map(|l| tw(&l.0))
        .max()
        .unwrap_or(0)
        .max(tw(&head));
    let widest = widest.min(w - margin - 20);
    shade(
        fb,
        x + margin - 6,
        y + 6,
        widest + 12,
        28 + lines.len() as i32 * 11,
        0.45,
    );
    glass(
        fb,
        x + margin,
        y + 8,
        &fit(&head, (widest / 8) as usize),
        green,
    );
    glass(fb, x + margin, y + 19, &refl, crate::hud::DIM);
    for (i, (t, c)) in lines.iter().enumerate() {
        glass(
            fb,
            x + margin,
            y + 33 + i as i32 * 11,
            &fit(t, (widest / 8) as usize),
            *c,
        );
    }
    let ts = format!(
        "TEMPS x{}{}  T+{}",
        g.timescale,
        if g.paused { " PAUSE" } else { "" },
        crate::fmt::duration(sim.time)
    );
    shade(
        fb,
        x + w - margin - tw(&ts) - 6,
        y + 6,
        tw(&ts) + 12,
        14,
        0.45,
    );
    glass(fb, x + w - margin - tw(&ts), y + 8, &ts, green);
}

/// The last messages, low in the window.
fn glass_messages(fb: &mut Framebuffer, g: &Game, cx: i32, y: i32, maxw: i32) {
    let chars = (maxw / 8).max(10) as usize;
    let msgs: Vec<String> = g
        .messages
        .iter()
        .rev()
        .take(2)
        .map(|m| fit(&m.to_uppercase(), chars))
        .collect();
    let widest = msgs.iter().map(|m| tw(m)).max().unwrap_or(0);
    if widest == 0 {
        return;
    }
    shade(fb, cx - widest / 2 - 8, y - 14, widest + 16, 36, 0.5);
    for (i, m) in msgs.iter().enumerate() {
        glass(
            fb,
            cx - tw(m) / 2,
            y - i as i32 * 11,
            m,
            if i == 0 {
                crate::hud::AMBER
            } else {
                crate::hud::DIM
            },
        );
    }
}

/// The help of an organ, in a box of the window; a click closes it.
fn draw_help(fb: &mut Framebuffer, g: &Game, win: Rc) {
    let Some((id, n)) = g.cockpit.help.get() else {
        return;
    };
    let (x, y, w, h) = win;
    let (name, key, text) = super::spec(id, n);
    let chars = ((w - 60) / 8).clamp(24, 52) as usize;
    let lines = crate::hub::wrap(text, chars);
    let bw = (lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(10)
        .max(name.chars().count() + 4)
        .max(28) as i32)
        * 8
        + 20;
    let bh = (lines.len() as i32 + 4) * 11 + 8;
    let bx = x + (w - bw) / 2;
    let by = (y + (h - bh) / 3).max(y + 4);
    g.hit(
        (bx, by, bx + bw, by + bh),
        Act::Call(Game::cockpit_help_close, 0),
        None,
    );
    fill(fb, bx, by, bw, bh, 0x0008_1018);
    frame(fb, bx, by, bw, bh, AMBER_LED);
    txt_b(fb, bx + 10, by + 6, name, AMBER_LED);
    if key != "-" {
        txt_r(
            fb,
            bx + bw - 10,
            by + 6,
            &format!("TOUCHE {key}"),
            GREEN_LED,
        );
    }
    for (i, l) in lines.iter().enumerate() {
        txt(fb, bx + 10, by + 22 + i as i32 * 11, l, INK);
    }
    txt(fb, bx + 10, by + bh - 14, "CLIC OU TOUCHE : FERMER", LABEL);
}

// ---- dash ---------------------------------------------------------------------------------------

fn console_bg(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32) {
    vgrad(fb, x, y, w, h, 0x001B_2129, 0x000F_1318);
    hl(fb, x, y, w, 0x0050_5C6C);
    hl(fb, x, y + 1, w, 0x0006_080B);
}

/// The strip at the top of the window: the menu buttons of the game sit on its right; on its
/// left the little chip that tells the arrangement (a click goes to the next one).
fn top_rail(fb: &mut Framebuffer, g: &Game, w: i32, label: &str) {
    fill(fb, 0, 0, w, 12, 0x0012_161C);
    hl(fb, 0, 11, w, 0x0038_4250);
    let s = format!("COCKPIT {label}");
    let hov = controls::organ(
        g,
        (4, 0, tw(&s) + 8, 11),
        Act::Call(Game::cockpit_cycle, 0),
        None,
        Id::Layout,
        0,
    );
    txt(fb, 8, 2, &s, if hov { INK } else { LABEL });
}

/// The canopy frame around the window of size (w, h) at (x, y): struts and glare shield.
fn canopy(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32) {
    let frame_c = 0x0012_161C;
    let hi = 0x0038_4250;
    let sw = (w / 9).min(150);
    tri(fb, (x, y), (x + sw, y), (x, y + h), frame_c);
    tri(fb, (x + w - sw, y), (x + w, y), (x + w, y + h), frame_c);
    line_over(fb, x + sw, y, x, y + h, hi);
    line_over(fb, x + w - sw, y, x + w, y + h, hi);
    // Glare shield: a hump along the bottom.
    let hump = (h / 10).clamp(8, 24);
    for col in 0..w {
        let t = std::f32::consts::PI * col as f32 / w as f32;
        let hh = 6 + ((hump - 6) as f32 * t.sin().powf(0.6)) as i32;
        vl(fb, x + col, y + h - hh, hh, frame_c);
        fb.px_set(x + col, y + h - hh, hi);
        fb.px_set(x + col, y + h - hh - 1, 0x0006_080B);
    }
}

fn nav_slot(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    if tel.landing.is_some() {
        descent_screen(fb, g, tel, r);
    } else if tel.dock.is_some() {
        dock_screen(fb, g, tel, r);
    } else {
        nav_screen(fb, g, tel, r);
    }
}

fn plan_slot(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    if tel.landing.is_some() || tel.dock.is_some() {
        pilot_screen(fb, g, tel, r);
    } else {
        plan_screen(fb, g, tel, r);
    }
}

/// The commands: six toggle switches, the SAS buttons and (room permitting) the alert lamps.
fn commands(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    plate(fb, r, "COMMANDES");
    let iw = w - 10;
    let th = toggles_height(iw);
    toggles(fb, g, tel, (x + 5, y + 15, iw, th));
    let mut cy = y + 15 + th + 3;
    let rem = y + h - 6 - cy;
    // Lamps first take what they need, the SAS gets the rest.
    let (lcols, lamps_h) = if rem >= 200 {
        (2, 86)
    } else if rem >= 108 {
        (4, 38)
    } else {
        (0, 0)
    };
    let label_h = if rem >= 200 { 12 } else { 0 };
    let sas_h = rem - lamps_h - if lamps_h > 0 { 4 } else { 0 } - label_h * 2;
    if label_h > 0 {
        txt(fb, x + 8, cy, "SAS - MAINTIEN D'ATTITUDE", LABEL);
        cy += label_h;
    }
    let step = ((sas_h + 2) / 4).clamp(14, 24);
    sas_block(fb, g, tel, (x + 6, cy, w - 12, step * 4));
    cy += step * 4 + 4;
    if lamps_h > 0 {
        if label_h > 0 {
            txt(fb, x + 8, cy, "ALERTES", LABEL);
            cy += label_h;
        }
        let lh = (y + h - 6 - cy).min(lamps_h);
        lamp_panel(fb, g, tel, (x + 6, cy, w - 12, lh), lcols);
    }
}

/// The console of arrangement A (also the drawer of C): the modules in a row, the console keys
/// under them. `rc` is the whole console, from its top edge to the bottom of the window.
fn console_a(fb: &mut Framebuffer, g: &Game, tel: &Tel, rc: Rc) {
    let (x0, y0, w, ch) = rc;
    console_bg(fb, x0, y0, w, ch);
    let kh = 22;
    let ky = y0 + ch - kh - 4;
    let my = y0 + 8;
    let mh = ky - my - 4;
    let gap = 5;
    let edge = 6;
    // systems, commands, flight, right block (radar, nav / target, plan), throttle; a window
    // that cannot hold them all drops the systems, then the commands (the strip has the
    // gauges and the keys have the rest).
    let mins = [128, 190, 240, 300, 52];
    let room = w - 2 * edge;
    let show_sys = room >= mins.iter().sum::<i32>() + 4 * gap;
    let show_cmd = show_sys || room >= mins[1..].iter().sum::<i32>() + 3 * gap;
    let n_gaps = 2 + i32::from(show_sys) + i32::from(show_cmd);
    let avail = room - n_gaps * gap;
    let (ws, left) = spread(
        avail,
        [
            (
                if show_sys { 128 } else { 0 },
                if show_sys { 176 } else { 0 },
                if show_sys { 236 } else { 0 },
                1,
            ),
            (
                if show_cmd { 190 } else { 0 },
                if show_cmd { 286 } else { 0 },
                if show_cmd { 390 } else { 0 },
                2,
            ),
            (240, 352, 520, 3),
            (300, 364, 600, 3),
            (52, 66, 84, 0),
        ],
    );
    let mut x = x0 + edge + left.max(0) / 2;
    let h = mh;
    if show_sys {
        prof::time(3, || systems(fb, g, tel, (x, my, ws[0], h)));
        x += ws[0] + gap;
    }
    if show_cmd {
        prof::time(4, || commands(fb, g, tel, (x, my, ws[1], h)));
        x += ws[1] + gap;
    }
    prof::time(5, || flight_cluster(fb, g, tel, (x, my, ws[2], h)));
    x += ws[2] + gap;
    // The right block: two rows of two screens.
    let rw = ws[3];
    let top_h = (h - 5) / 2;
    let bot_h = h - 5 - top_h;
    let w1 = (rw * 150 / 364).max(118);
    let w2 = rw - w1 - 5;
    let ry2 = my + top_h + 5;
    // Radar and navigation / descent.
    plate(fb, (x, my, w1, top_h), "");
    let rr = ((w1 - 24) / 2).min((top_h - 34) / 2).max(24);
    prof::time(6, || {
        radar(fb, g, tel, x + w1 / 2, my + 4 + (top_h - 14) / 2, rr)
    });
    txt_c(fb, x + w1 / 2, my + 2, "SCANNER", LABEL);
    plate(fb, (x + w1 + 5, my, w2, top_h), "");
    prof::time(7, || {
        nav_slot(fb, g, tel, (x + w1 + 11, my + 6, w2 - 12, top_h - 12))
    });
    // Target and plan / pilot.
    let w3 = (rw * 190 / 364).max(150);
    let w4 = rw - w3 - 5;
    prof::time(8, || {
        plate(fb, (x, ry2, w3, bot_h), "");
        target_panel(fb, g, tel, (x + 6, ry2 + 6, w3 - 12, bot_h - 12));
        plate(fb, (x + w3 + 5, ry2, w4, bot_h), "");
        plan_slot(fb, g, tel, (x + w3 + 11, ry2 + 6, w4 - 12, bot_h - 12));
    });
    x += rw + gap;
    prof::time(9, || throttle(fb, g, tel, (x, my, ws[4], h)));
    // The console keys.
    prof::time(10, || {
        fkeys(fb, g, (x0 + edge, ky, w - 2 * edge, kh), &FKEYS, 10)
    });
}

// ---- A ------------------------------------------------------------------------------------------

fn layout_a(fb: &mut Framebuffer, v: &View, tel: &Tel, top: i32, bot: i32) {
    let g = v.game;
    let (w, h) = (fb.w as i32, fb.h as i32);
    let area_h = h - top - bot;
    let ch = (area_h * 49 / 100).clamp(236, 420).min(area_h - 100);
    let wh = area_h - ch;
    let win = (0, top, w, wh);
    prof::time(1, || scene(fb, v, win));
    let ext = g.ext_active();
    let margin = if ext { 12 } else { (w / 9).min(150) + 8 };
    prof::time(2, || {
        glass_texts(fb, g, win, margin);
        glass_messages(fb, g, w / 2, top + wh - 44, w - 2 * margin);
    });
    if !ext {
        canopy(fb, 0, top, w, wh);
    }
    top_rail(fb, g, w, "A");
    console_a(fb, g, tel, (0, top + wh, w, ch));
    draw_help(fb, g, win);
}

// ---- B ------------------------------------------------------------------------------------------

fn layout_b(fb: &mut Framebuffer, v: &View, tel: &Tel, top: i32, bot: i32) {
    let g = v.game;
    let (w, h) = (fb.w as i32, fb.h as i32);
    let area_h = h - top - bot;
    vgrad(fb, 0, 0, w, h, 0x001B_2129, 0x000F_1318);
    top_rail(fb, g, w, "B");
    let pw = (w * 19 / 100).clamp(186, 300);
    let gap = 5;
    let y0 = top + 4;
    let col_h = area_h - 8;
    // The window and the compact console under it.
    let wx = pw + 12;
    let ww = w - 2 * pw - 24;
    let bh = (area_h * 34 / 100).clamp(190, 300);
    let wy = y0 + 4;
    let wh = col_h - bh - 8 - 4;
    fill(fb, wx - 6, wy - 6, ww + 12, wh + 12, DARK);
    frame(fb, wx - 6, wy - 6, ww + 12, wh + 12, 0x0056_6272);
    let win = (wx, wy, ww, wh);
    scene(fb, v, win);
    glass_texts(fb, g, win, 10);
    glass_messages(fb, g, wx + ww / 2, wy + wh - 24, ww - 20);
    for (sx, sy) in [
        (wx - 3, wy - 3),
        (wx + ww + 2, wy - 3),
        (wx - 3, wy + wh + 2),
        (wx + ww + 2, wy + wh + 2),
    ] {
        screw(fb, sx, sy);
    }
    // Left column: systems, toggles, SAS, navigation.
    let ix = 5;
    let (hs, _) = spread(
        col_h - 3 * gap,
        [
            (104, 150, 200, 2),
            (58, 78, 90, 0),
            (96, 112, 130, 0),
            (80, 200, 420, 3),
        ],
    );
    let mut y = y0;
    systems(fb, g, tel, (ix, y, pw, hs[0]));
    y += hs[0] + gap;
    plate(fb, (ix, y, pw, hs[1]), "");
    let th = toggles_height(pw - 10).min(hs[1] - 6);
    toggles(fb, g, tel, (ix + 5, y + 3, pw - 10, th));
    y += hs[1] + gap;
    plate(fb, (ix, y, pw, hs[2]), "SAS");
    let step = ((hs[2] - 22) / 4).clamp(14, 24);
    sas_block(fb, g, tel, (ix + 6, y + 16, pw - 12, step * 4));
    y += hs[2] + gap;
    plate(fb, (ix, y, pw, hs[3]), "");
    nav_slot(fb, g, tel, (ix + 6, y + 6, pw - 12, hs[3] - 12));
    // Right column: radar, plan, target, lamps.
    let rx = w - 5 - pw;
    let (hr, _) = spread(
        col_h - 3 * gap,
        [
            (140, pw.min(236), 250, 3),
            (100, 150, 170, 1),
            (118, 140, 160, 1),
            (70, 120, 152, 1),
        ],
    );
    let mut y = y0;
    plate(fb, (rx, y, pw, hr[0]), "");
    let rr = ((pw - 24) / 2).min((hr[0] - 34) / 2).max(24);
    radar(fb, g, tel, rx + pw / 2, y + 4 + (hr[0] - 14) / 2, rr);
    txt_c(fb, rx + pw / 2, y + 2, "SCANNER", LABEL);
    y += hr[0] + gap;
    plate(fb, (rx, y, pw, hr[1]), "");
    plan_slot(fb, g, tel, (rx + 6, y + 6, pw - 12, hr[1] - 12));
    y += hr[1] + gap;
    plate(fb, (rx, y, pw, hr[2]), "");
    target_panel(fb, g, tel, (rx + 6, y + 6, pw - 12, hr[2] - 12));
    y += hr[2] + gap;
    plate(fb, (rx, y, pw, hr[3]), "ALERTES");
    let lcols = if hr[3] >= 120 { 2 } else { 4 };
    lamp_panel(fb, g, tel, (rx + 6, y + 16, pw - 12, hr[3] - 22), lcols);
    // The compact console under the window.
    let cy = wy + wh + 10;
    let thr_w = (ww / 10).clamp(64, 90);
    let cw = ww - thr_w - 5;
    let keys_h = 22;
    flight_cluster(fb, g, tel, (wx, cy, cw, bh - keys_h - 4 - 6));
    let keys = [
        FKEYS[0], FKEYS[1], FKEYS[2], FKEYS[3], FKEYS[4], FKEYS[5], FKEYS[6], FKEYS[9],
    ];
    fkeys(fb, g, (wx, cy + bh - keys_h - 6, cw, keys_h), &keys, 8);
    throttle(fb, g, tel, (wx + cw + 5, cy, thr_w, bh - 6));
    draw_help(fb, g, win);
}

// ---- C ------------------------------------------------------------------------------------------

/// The glass HUD of arrangement C: tapes and ladder drawn as additive vectors over the scene,
/// in the part of the window above `bottom`.
fn glass_hud(fb: &mut Framebuffer, g: &Game, tel: &Tel, bottom: i32) {
    use crate::hud::{AMBER, DIM, GREEN, HOVER};
    let c = GREEN;
    let dim = DIM;
    let w = fb.w as f32;
    let u = (w / 1280.0).clamp(0.62, 1.5);
    let cx = w / 2.0;
    let top = 30.0f32;
    let cy = top + (bottom as f32 - top) / 2.0 - 4.0;
    let half = ((bottom as f32 - top) / 2.0 - 16.0).clamp(30.0, 100.0);
    let ext = g.ext_active();
    // Backings under the numbers so that they read over a bright ground.
    let tape_dx = 230.0 * u;
    shade(fb, (cx - 150.0 * u) as i32, 22, (300.0 * u) as i32, 50, 0.5);
    shade(
        fb,
        (cx - tape_dx - 100.0) as i32,
        (cy - half - 6.0) as i32,
        110,
        (half * 2.0 + 12.0) as i32,
        0.55,
    );
    if tel.alt.is_some() {
        shade(
            fb,
            (cx + tape_dx - 8.0) as i32,
            (cy - half - 6.0) as i32,
            112,
            (half * 2.0 + 12.0) as i32,
            0.55,
        );
    }
    // Heading tape.
    let ppd = 6.0 * u;
    let hr = tel.heading.round() as i32;
    for d in -40..=40 {
        let deg = (hr + d).rem_euclid(360);
        let x = cx + ((deg as f32 - tel.heading + 540.0).rem_euclid(360.0) - 180.0) * ppd;
        if (x - cx).abs() > 240.0 * u {
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
                if deg % 90 == 0 { AMBER } else { c },
            );
        } else if deg % 5 == 0 {
            fb.line(x, 40.0, x, 46.0, dim);
        }
    }
    fb.line(cx, 52.0, cx - 4.0, 58.0, c);
    fb.line(cx, 52.0, cx + 4.0, 58.0, c);
    fb.text_at(cx as i32 - 12, 60, &format!("{:03.0}", tel.heading), c);
    // Speed tape (left) and altitude (right).
    let tape = |fb: &mut Framebuffer, x: f32, v: f64, step: f64, unit: &str, left: bool| {
        fb.line(x, cy - half, x, cy + half, dim);
        for k in -9..=9 {
            let val = (v / step).round() * step + f64::from(k) * step;
            let y = cy - ((val - v) / step * 18.0) as f32;
            if (y - cy).abs() > half {
                continue;
            }
            let len = if k % 2 == 0 { 8.0 } else { 4.0 };
            let (x0, x1) = if left { (x - len, x) } else { (x, x + len) };
            fb.line(x0, y, x1, y, c);
            if k % 2 == 0 && val >= 0.0 && (y - cy).abs() > 10.0 {
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
        fb.outline(bx as f32, cy - 8.0, (bx + 64) as f32, cy + 8.0, HOVER);
        let s = short(v);
        fb.text_at(bx + 32 - tw(&s) / 2, cy as i32 - 3, &s, HOVER);
        fb.text_at(bx + 32 - tw(unit) / 2, cy as i32 + 12, unit, dim);
    };
    tape(
        fb,
        cx - tape_dx,
        tel.spd,
        super::instruments::scale_for(tel.spd) / 20.0,
        "M/S",
        true,
    );
    if let Some(a) = tel.alt {
        tape(fb, cx + tape_dx, a, 20.0, "ALT M", false);
    }
    // Pitch ladder (not over the ship seen from outside, nor in a window too low for it).
    if !ext && bottom >= 380 {
        let k = (bottom as f32) * 0.8;
        let n = tel.ux.hypot(tel.uy).max(1e-4);
        let (nx, ny) = (tel.ux / n, tel.uy / n);
        for deg in [-30i32, -20, -10, 0, 10, 20, 30] {
            let s = (deg as f32).to_radians().sin();
            let off = k * (s - tel.sinp);
            let (mx, my) = (cx + nx * off, cy - ny * off);
            let hw = if deg == 0 { 120.0 * u } else { 40.0 * u };
            let (dx, dy) = (-ny * hw, -nx * hw);
            let (x0, y0, x1, y1) = (mx - dx, my + dy, mx + dx, my - dy);
            if (mx - cx).abs() < 260.0 * u && (my - cy).abs() < half + 30.0 {
                if deg == 0 {
                    fb.line(x0, y0, x1, y1, c);
                } else {
                    fb.line(x0, y0, x0 + (x1 - x0) * 0.35, y0 + (y1 - y0) * 0.35, c);
                    fb.line(x1, y1, x1 + (x0 - x1) * 0.35, y1 + (y0 - y1) * 0.35, c);
                    fb.text_at(x1 as i32 + 4, y1 as i32 - 3, &format!("{deg:+}"), c);
                }
            }
        }
    }
    // Reticle.
    fb.circle(cx, cy, 5.0, c);
    fb.line(cx - 16.0, cy, cx - 7.0, cy, c);
    fb.line(cx + 7.0, cy, cx + 16.0, cy, c);
    fb.line(cx, cy - 12.0, cx, cy - 7.0, c);
    // Corner readouts, bottom left and right, above the console.
    let y = bottom - 46;
    shade(fb, 8, y - 4, 230, 40, 0.5);
    fb.text_at(
        14,
        y,
        &format!("ACC {:.0}%   TRIM {:+}", tel.thr * 100.0, tel.trim),
        c,
    );
    fb.text_at(14, y + 11, &format!("SAS {}", tel.sas.label()), dim);
    fb.text_at(
        14,
        y + 22,
        &format!(
            "TRAIN {}  {}{}",
            if tel.gear { "SORTI" } else { "RENTRE" },
            if tel.shield_on { "BCL " } else { "" },
            if tel.lights { "LUM" } else { "" }
        ),
        if tel.gear { AMBER } else { dim },
    );
    let sim = &g.sim;
    if let Some(t) = sim
        .target
        .map(|i| format!("CIBLE {}", sim.bodies[i].name.to_uppercase()))
    {
        shade(fb, fb.w as i32 - tw(&t) - 22, y - 4, tw(&t) + 16, 16, 0.5);
        fb.text_at(fb.w as i32 - 14 - tw(&t), y, &t, AMBER);
    }
}

/// The folded strip of arrangement C: key gauges, the lamps, a few console keys, the handle.
fn strip_c(fb: &mut Framebuffer, g: &Game, tel: &Tel, y: i32, w: i32) {
    console_bg(fb, 0, y, w, fb.h as i32 - y);
    let ty = y + 4;
    let kw = (w * 9 / 100).clamp(60, 96);
    let keys = [FKEYS[0], FKEYS[1], FKEYS[2], FKEYS[9]];
    let keys_w = 4 * kw + 9;
    let lamps_w = 8 * 18 + 8;
    let bars_w = w - keys_w - lamps_w - 36;
    let per = 118;
    let n = (bars_w / per).clamp(1, 4) as usize;
    let per = bars_w / n as i32;
    let bars: [(&str, usize, bool, String, bool); 4] = [
        (
            "CARB",
            0,
            false,
            format!("{:.0}%", tel.fuel * 100.0),
            tel.fuel < 0.25,
        ),
        (
            "BATT",
            2,
            false,
            format!("{:.0}%", tel.batt * 100.0),
            tel.batt < 0.25,
        ),
        (
            "COQ",
            3,
            false,
            format!("{:.0}%", tel.hull * 100.0),
            tel.hull < 0.5,
        ),
        (
            "TEMP",
            5,
            true,
            format!("{:.0}K", tel.temp_k),
            tel.temp_k > 500.0,
        ),
    ];
    for (k, (l, idx, hb, value, alarm)) in bars.iter().take(n).enumerate() {
        super::instruments::strip_bar(
            fb,
            g,
            tel,
            (10 + k as i32 * per, ty + 7, per - 10),
            l,
            *idx,
            *hb,
            value,
            *alarm,
        );
    }
    // The folded lamps.
    let lx = 10 + n as i32 * per + 4;
    for (k, (_, lv)) in tel.lamps.iter().enumerate() {
        let on = match lv {
            2 => tel.blink,
            1 => true,
            _ => false,
        };
        let cx = lx + 9 + k as i32 * 18;
        controls::organ(
            g,
            (cx - 8, ty + 3, 17, 20),
            Act::Key(KeyCode::Backslash),
            None,
            Id::Lamp,
            k,
        );
        lamp(
            fb,
            cx,
            ty + 13,
            if *lv == 2 { RED_LED } else { AMBER_LED },
            on,
        );
    }
    fkeys(fb, g, (w - keys_w - 6, ty, keys_w, 24), &keys, 4);
}

fn layout_c(fb: &mut Framebuffer, v: &View, tel: &Tel, bot: i32) {
    let g = v.game;
    let (w, h) = (fb.w as i32, fb.h as i32);
    let open = g.settings.cockpit_open;
    let area_bottom = h - bot;
    let strip = 34;
    let full = (0, 0, w, h);
    scene(fb, v, full);
    // The top of the console (or of the strip): the window above it ends 18 px higher.
    let console_top = if open {
        let ch = (area_bottom * 42 / 100)
            .clamp(236, 380)
            .min(area_bottom - 120);
        let y0 = area_bottom - ch;
        console_a(fb, g, tel, (0, y0, w, ch));
        handle(fb, g, w / 2, y0 - 16, false);
        y0
    } else {
        let y0 = area_bottom - strip;
        strip_c(fb, g, tel, y0, w);
        handle(fb, g, w / 2, y0 - 16, true);
        y0
    };
    let win_bottom = console_top - 18;
    if !v.map {
        glass_hud(fb, g, tel, win_bottom);
    }
    // The first rows belong to the menu buttons of the game: the texts start under them.
    let win = (0, 12, w, win_bottom - 12);
    glass_texts(fb, g, win, 14);
    glass_messages(fb, g, w / 2, win_bottom - 44, (w - 520).max(300));
    top_rail_chip(fb, g, "C");
    draw_help(fb, g, win);
}

/// The layout chip on the scene (C has no strip at the top).
fn top_rail_chip(fb: &mut Framebuffer, g: &Game, label: &str) {
    let s = format!("COCKPIT {label}");
    let hov = controls::organ(
        g,
        (2, 0, tw(&s) + 8, 11),
        Act::Call(Game::cockpit_cycle, 0),
        None,
        Id::Layout,
        0,
    );
    shade(fb, 2, 0, tw(&s) + 8, 11, 0.5);
    fb.text_at(
        6,
        2,
        &s,
        if hov {
            crate::hud::HOVER
        } else {
            crate::hud::DIM
        },
    );
}

// ---- entry --------------------------------------------------------------------------------------

/// Draw the whole cockpit (scene, glass texts, console) in the arrangement `d`.
pub fn draw(fb: &mut Framebuffer, v: &View, tel: &Tel, d: Disposition) {
    let (top, bot) = insets(v.game);
    match d {
        Disposition::A => layout_a(fb, v, tel, top, bot),
        Disposition::B => layout_b(fb, v, tel, top, bot),
        Disposition::C => layout_c(fb, v, tel, bot),
        Disposition::Old => {}
    }
}
