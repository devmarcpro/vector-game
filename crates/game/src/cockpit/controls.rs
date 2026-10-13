//! The controls of the console: toggle switches, lit push buttons, the thrust lever, the trim,
//! the console keys, the handle of the folding console. Each one declares the zone of the key it
//! stands for (`Act::Key`): the game has one implementation of every action.
use super::draw::*;
use super::{Id, Organ, Tel};
use crate::game::Game;
use crate::mouse::Act;
use vector_render::{mix_color, Framebuffer};
use vector_sim::landing::Sas;
use winit::keyboard::KeyCode;

/// Declare the zone of an organ (x, y, w, h) with its action and remember what it is (tooltip,
/// menu). Returns whether the pointer is over it.
pub fn organ(g: &Game, r: Rc, act: Act, dbl: Option<Act>, id: Id, n: usize) -> bool {
    let hov = g.hit((r.0, r.1, r.0 + r.2, r.1 + r.3), act, dbl);
    let hit = g.pointer.hits.borrow().len() - 1;
    g.cockpit.organs.borrow_mut().push(Organ {
        hit,
        id,
        n: n as u8,
    });
    hov
}

/// A display organ: a zone that only has a tooltip and a menu.
pub fn display(g: &Game, r: Rc, id: Id, n: usize) -> bool {
    organ(g, r, Act::Call(Game::cockpit_noop, 0), None, id, n)
}

// ---- toggle switches ---------------------------------------------------------------------------

/// A toggle switch in the cell (x, y, w, h): lamp, bezel, lever, engraved label. `mini` draws the
/// small version (30 px wide).
#[allow(clippy::too_many_arguments)]
pub fn toggle(
    fb: &mut Framebuffer,
    g: &Game,
    tel: &Tel,
    r: Rc,
    label: &str,
    on: bool,
    lamp_c: u32,
    act: Act,
    id: Id,
) {
    let (x, y, w, h) = r;
    let hov = organ(g, (x, y, w, h), act, None, id, 0);
    let mini = w < 42 || h < 60;
    let cx = x + w / 2;
    let rb = if mini { 9 } else { 13 };
    lamp(fb, cx, y + 7, lamp_c, on);
    let cy = y + 7 + 8 + rb + 1;
    disc(fb, cx, cy, rb + 1, 0x0010_1419);
    disc(fb, cx, cy, rb, if hov { 0x0066_7587 } else { 0x0048_5360 });
    disc(fb, cx, cy, rb - 3, METAL_LO);
    ring(fb, cx, cy, rb, 0x0090_9CAC);
    // Lever: up when on, down when off.
    let reach = if mini { 11 } else { 16 };
    let tip_y = if on { cy - reach } else { cy + reach };
    for d in -1..=1 {
        line_over(
            fb,
            cx + d,
            cy,
            cx + d,
            tip_y,
            if d == 0 { 0x00E0_E6EE } else { 0x0078_8496 },
        );
    }
    let kn = if mini { 3 } else { 5 };
    disc(
        fb,
        cx,
        tip_y,
        kn,
        if on { 0x00F2_F6FA } else { 0x00A8_B2C0 },
    );
    disc(fb, cx - 1, tip_y - 1, kn / 2, 0x00FF_FFFF);
    let ly = cy + rb + 8;
    let chars = ((w - 2) / 8).max(3) as usize;
    txt_c(fb, cx, ly, &fit(label, chars), if on { INK } else { LABEL });
    if on {
        // The bar under a switch that is on breathes a little.
        let c = mix_color(lamp_c, 0x00FF_FFFF, tel.pulse * 0.25);
        hl(fb, x + 5, ly + 10, w - 10, c);
    }
}

/// The six toggle switches in the rectangle: all in a row when they fit, else three by two.
pub fn toggles(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    let items: [(&str, &str, bool, u32, Act, Id); 6] = [
        (
            "TRAIN",
            "TRAIN",
            tel.gear,
            GREEN_LED,
            Act::Key(KeyCode::Numpad3),
            Id::Gear,
        ),
        (
            "BOUCL",
            "BOUCLIER",
            tel.shield_installed && tel.shield_on,
            BLUE_LED,
            Act::Key(KeyCode::NumpadDivide),
            Id::Shield,
        ),
        (
            "RADAR",
            "RADAR",
            tel.radar_on,
            GREEN_LED,
            Act::Key(KeyCode::Period),
            Id::Radar,
        ),
        (
            "LEURR",
            "LEURRE",
            tel.decoys > 0,
            AMBER_LED,
            Act::Key(KeyCode::Comma),
            Id::Decoy,
        ),
        (
            "LUMIE",
            "LUMIERES",
            tel.lights,
            0x00F0_F0A0,
            Act::Key(KeyCode::NumpadDecimal),
            Id::Lights,
        ),
        (
            "PILOT",
            "PILOTE",
            tel.auto || tel.pilot.is_some(),
            AMBER_LED,
            Act::Key(KeyCode::KeyG),
            Id::Auto,
        ),
    ];
    // One row when the cells are wide enough for a toggle, else two rows of three.
    let one_row = w / 6 >= 30 && h >= 52;
    let (cols, rows) = if one_row { (6, 1) } else { (3, 2) };
    let cw = (w / cols).min(52);
    let ch = (h / rows).min(70);
    let ox = x + (w - cw * cols) / 2;
    for (k, (short, long, on, c, act, id)) in items.iter().enumerate() {
        let (col, row) = (k as i32 % cols, k as i32 / cols);
        let label = if cw >= 56 { long } else { short };
        toggle(
            fb,
            g,
            tel,
            (ox + col * cw, y + row * ch, cw, ch),
            label,
            *on,
            *c,
            *act,
            *id,
        );
    }
}

/// Height needed by `toggles` for a width.
pub fn toggles_height(w: i32) -> i32 {
    if w / 6 >= 30 {
        if w / 6 >= 42 {
            66
        } else {
            54
        }
    } else {
        108
    }
}

// ---- push buttons ------------------------------------------------------------------------------

/// A lit push button: when lit, its face glows in `c` and it looks pressed in.
#[allow(clippy::too_many_arguments)]
pub fn pushbtn(
    fb: &mut Framebuffer,
    g: &Game,
    tel: &Tel,
    r: Rc,
    label: &str,
    lit: bool,
    c: u32,
    act: Act,
    dbl: Option<Act>,
    id: Id,
    n: usize,
) {
    let (x, y, w, h) = r;
    let hov = organ(g, (x, y, w, h), act, dbl, id, n);
    fill(fb, x, y, w, h, DARK);
    let face = if lit {
        mix_color(0x0010_1418, c, 0.70 + 0.10 * tel.pulse)
    } else if hov {
        0x0030_3844
    } else {
        0x0022_2A33
    };
    fill(fb, x + 2, y + 2, w - 4, h - 4, face);
    if lit {
        hl(
            fb,
            x + 2,
            y + h - 3,
            w - 4,
            mix_color(face, 0x00FF_FFFF, 0.25),
        );
        halo(
            fb,
            x + w / 2,
            y + h / 2,
            (w / 2 + 6).min(40),
            c,
            0.25 + 0.12 * tel.pulse,
        );
    } else {
        hl(fb, x + 2, y + 2, w - 4, mix_color(face, 0x00FF_FFFF, 0.22));
        vl(fb, x + 2, y + 2, h - 4, mix_color(face, 0x00FF_FFFF, 0.12));
        hl(fb, x + 2, y + h - 3, w - 4, METAL_LO);
    }
    if h >= 16 {
        // A LED strip in the top edge.
        fill(
            fb,
            x + w / 2 - 8,
            y + 3,
            16,
            2,
            if lit {
                0x00FF_FFFF
            } else {
                mix_color(DARK, c, 0.4)
            },
        );
    }
    let tc = if lit {
        0x0004_0608
    } else if hov {
        INK
    } else {
        mix_color(LABEL, INK, 0.3)
    };
    let label = fit(label, ((w - 6) / 8).max(1) as usize);
    let ty = y + h / 2 - 3;
    txt_c(fb, x + w / 2, ty, &label, tc);
    if lit {
        txt_c(fb, x + w / 2 + 1, ty, &label, tc);
    }
    frame(fb, x, y, w, h, if hov { INK } else { 0x0044_4E5C });
}

/// A SAS button: label, mode, key, and the mode and key of a double click.
type SasItem = (&'static str, Sas, KeyCode, Option<(Sas, KeyCode)>);

/// The eight SAS push buttons in two columns of four.
pub fn sas_block(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    let items: [SasItem; 8] = [
        ("COUPE", Sas::Off, KeyCode::Digit0, None),
        ("MAINTIEN", Sas::Hold, KeyCode::Digit1, None),
        ("PROGRADE", Sas::Prograde, KeyCode::Digit2, None),
        ("RETROGRADE", Sas::Retrograde, KeyCode::Digit3, None),
        (
            "NORMAL",
            Sas::Normal,
            KeyCode::Digit4,
            Some((Sas::AntiNormal, KeyCode::Digit5)),
        ),
        (
            "RADIAL",
            Sas::RadialOut,
            KeyCode::Digit6,
            Some((Sas::RadialIn, KeyCode::Digit7)),
        ),
        ("CIBLE", Sas::Target, KeyCode::Digit8, None),
        ("VERTICALE", Sas::Up, KeyCode::Digit9, None),
    ];
    let bw = (w - 4) / 2;
    let step = ((h + 2) / 4).clamp(14, 24);
    let bh = step - 2;
    for (k, (l, m, code, alt)) in items.iter().enumerate() {
        let bx = x + (k as i32 / 4) * (bw + 4);
        let by = y + (k as i32 % 4) * step;
        let lit = tel.sas == *m || alt.is_some_and(|a| tel.sas == a.0);
        let c = if *m == Sas::Off { RED_LED } else { GREEN_LED };
        let label = match (alt, lit && alt.is_some_and(|a| tel.sas == a.0)) {
            (Some(_), true) if *m == Sas::Normal => "ANTI-NORM",
            (Some(_), true) => "RAD. ENTR.",
            _ => l,
        };
        pushbtn(
            fb,
            g,
            tel,
            (bx, by, bw, bh),
            label,
            lit,
            c,
            Act::Key(*code),
            alt.map(|a| Act::Key(a.1)),
            Id::Sas,
            k,
        );
    }
}

// ---- the thrust lever --------------------------------------------------------------------------

/// The thrust lever: a vertical slot with graduations, a lit level strip, a grip that follows
/// the real throttle, the digital value and the trim. Click or drag the slot, wheel over it.
pub fn throttle(fb: &mut Framebuffer, g: &Game, tel: &Tel, r: Rc) {
    let (x, y, w, h) = r;
    plate(fb, r, if w >= 60 { "POUSSEE" } else { "ACC" });
    let with_trim = h >= 150;
    let bottom = if with_trim { 66 } else { 30 };
    let sy = y + 22;
    let slot_h = (h - 22 - bottom).max(30);
    let sx = x + w / 2 - 6;
    well(fb, (sx - 2, sy - 2, 16, slot_h + 4), 0x0002_0507);
    // The lit strip, brighter in the pulse.
    let lit = (tel.thr_vis * f64::from(slot_h)) as i32;
    for i in 0..lit {
        let k = f64::from(i) / f64::from(slot_h);
        let c = if k > 0.85 { WARN } else { GOOD };
        hl(fb, sx + 3, sy + slot_h - 1 - i, 6, mix_color(DARK, c, 0.55));
    }
    // Graduations and numbers every 10 %.
    let big_step = if slot_h >= 120 { 1 } else { 2 };
    for k in 0..=10 {
        let yy = sy + slot_h - 1 - (k * (slot_h - 1)) / 10;
        let major = k % 5 == 0;
        hl(
            fb,
            sx - 8 - i32::from(major) * 4,
            yy,
            6 + i32::from(major) * 4,
            0x00C8_D4DC,
        );
        hl(fb, sx + 14, yy, 4 + i32::from(major) * 4, 0x00C8_D4DC);
        if major || (k % big_step == 0 && slot_h >= 160) {
            txt_r(fb, sx - 14, yy - 3, &format!("{}", k * 10), LABEL);
        }
    }
    // The zone of the slot (and of the grip around it) is the lever itself.
    g.cockpit.lever.set(super::Lever {
        top: sy,
        bottom: sy + slot_h - 1,
    });
    organ(
        g,
        (x + 2, sy - 8, w - 4, slot_h + 12),
        Act::At(Game::cockpit_lever_at),
        None,
        Id::Lever,
        0,
    );
    // The grip follows the (smoothed) throttle.
    let ky = sy + slot_h - 1 - (tel.thr_vis * f64::from(slot_h - 1)) as i32;
    let kx = sx + 6;
    let gw = (w - 12).clamp(24, 40);
    fill(fb, kx - gw / 2, ky - 10, gw, 20, 0x0026_2D38);
    frame(fb, kx - gw / 2, ky - 10, gw, 20, 0x0090_9CAC);
    for k in 0..4 {
        hl(
            fb,
            kx - gw / 2 + 4,
            ky - 6 + k * 4,
            gw - 8,
            if k % 2 == 0 { 0x0010_141A } else { 0x0050_5A68 },
        );
    }
    hl(fb, kx - gw / 2, ky - 10, gw, 0x00C0_C8D4);
    fill(
        fb,
        kx - 3,
        ky - 10,
        6,
        20,
        mix_color(0x0026_2D38, AMBER_LED, 0.8),
    );
    // Digital value.
    let v = format!("{:.0}", tel.thr * 100.0);
    well(fb, (x + 8, y + h - bottom + 4, w - 16, 18), SCREEN_BG);
    txt_c(fb, x + w / 2 - 4, y + h - bottom + 8, &v, GREEN_LED);
    txt(
        fb,
        x + w / 2 + 12 - i32::from(w < 60) * 6,
        y + h - bottom + 8,
        "%",
        LABEL,
    );
    // Trim: a short scale under the value (clicks on its halves are the trim keys).
    if with_trim {
        let ty = y + h - bottom + 28;
        let label = format!("TRIM {:+}", tel.trim);
        txt_c(
            fb,
            x + w / 2,
            ty + 2,
            &label,
            if tel.trim == 0 { LABEL } else { AMBER_LED },
        );
        let trim = i32::from(tel.trim);
        for k in -5i32..=5 {
            let on = (k > 0 && trim >= k * 2) || (k < 0 && trim <= k * 2) || k == 0;
            vl(
                fb,
                x + w / 2 + k * 5,
                ty + 14,
                if k == 0 { 7 } else { 4 },
                if on && k != 0 { AMBER_LED } else { 0x0068_7684 },
            );
        }
        let half = (w - 8) / 2;
        organ(
            g,
            (x + 4, ty - 2, half, 26),
            Act::Key(KeyCode::BracketLeft),
            None,
            Id::Trim,
            0,
        );
        organ(
            g,
            (x + 4 + half, ty - 2, half, 26),
            Act::Key(KeyCode::BracketRight),
            None,
            Id::Trim,
            1,
        );
    }
}

// ---- console keys and handle -------------------------------------------------------------------

pub const FKEYS: [(&str, &str, KeyCode); 10] = [
    ("CARTE", "M", KeyCode::KeyM),
    ("GALAXIE", "L", KeyCode::KeyL),
    ("SOUTE", "I", KeyCode::KeyI),
    ("EQUIPAGE", "K", KeyCode::KeyK),
    ("JOURNAL", "J", KeyCode::KeyJ),
    ("CODEX", "F6", KeyCode::F6),
    ("VAISSEAU", "H", KeyCode::KeyH),
    ("NAV AUTO", "*", KeyCode::NumpadMultiply),
    ("TECH", "F4", KeyCode::F4),
    ("MENU", "F10", KeyCode::F10),
];

/// A row of console keys; the labels shrink to fit. Each is the zone of the key it names (the
/// tooltip and the menu are those of the key).
pub fn fkeys(fb: &mut Framebuffer, g: &Game, r: Rc, items: &[(&str, &str, KeyCode)], cols: usize) {
    let (x, y, w, h) = r;
    let n = items.len() as i32;
    let cols = (cols as i32).max(1);
    let rows = (n + cols - 1) / cols;
    let bw = (w - 4 - (cols - 1) * 3) / cols;
    let bh = (h - (rows - 1) * 3) / rows;
    for (k, (label, tag, code)) in items.iter().enumerate() {
        let (c, r_) = (k as i32 % cols, k as i32 / cols);
        let bx = x + 2 + c * (bw + 3);
        let by = y + r_ * (bh + 3);
        let hov = g.hit((bx, by, bx + bw, by + bh), Act::Key(*code), None);
        fill(fb, bx, by, bw, bh, DARK);
        fill(
            fb,
            bx + 2,
            by + 2,
            bw - 4,
            bh - 4,
            if hov { 0x0038_4252 } else { 0x0022_2A33 },
        );
        hl(fb, bx + 2, by + 2, bw - 4, 0x0044_4E5C);
        hl(fb, bx + 2, by + bh - 3, bw - 4, METAL_LO);
        frame(fb, bx, by, bw, bh, if hov { INK } else { 0x0044_4E5C });
        let room = (bw - 8) / 8;
        let ty = by + bh / 2 - 3;
        let col = if hov { INK } else { AMBER_LED };
        if tw(label) + tw(tag) + 14 <= bw {
            txt_b(fb, bx + 6, ty, label, col);
            txt_r(fb, bx + bw - 6, ty, tag, LABEL);
        } else if room >= 3 {
            txt_c(fb, bx + bw / 2, ty, &fit(label, room as usize), col);
        } else {
            txt_c(fb, bx + bw / 2, ty, tag, col);
        }
    }
}

/// The handle of the folding console (arrangement C): a tab on the edge of the console.
pub fn handle(fb: &mut Framebuffer, g: &Game, x: i32, y: i32, up: bool) {
    let (w, h) = (112, 16);
    let hov = super::controls::organ(
        g,
        (x - w / 2, y, w, h),
        Act::Key(KeyCode::Quote),
        None,
        Id::Handle,
        0,
    );
    fill(fb, x - w / 2, y, w, h, 0x0026_2D38);
    frame(fb, x - w / 2, y, w, h, if hov { INK } else { 0x0078_8496 });
    txt_c(
        fb,
        x,
        y + 4,
        if up { "CONSOLE ^" } else { "CONSOLE v" },
        if hov { INK } else { AMBER_LED },
    );
}
