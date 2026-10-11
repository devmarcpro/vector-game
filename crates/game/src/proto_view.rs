//! PROTOTYPES of a more visual, livelier interface (F12 in flight): the same engine, lines and
//! text only, but screens built from clickable drawings and buttons instead of lists.
//! 0 = the body (organs and implants on a wireframe silhouette), 1 = the station hall.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use std::sync::OnceLock;
use std::time::Instant;
use vector_render::Framebuffer;
use winit::keyboard::KeyCode;

const WIRE: u32 = 0x0040_7050;
const HOVER: u32 = 0x00D0_FFD0;
const CYAN: u32 = 0x0050_C0E0;

/// Seconds since the first call: drives the little animations (interface only, never the sim).
pub fn anim_time() -> f32 {
    phase()
}

fn phase() -> f32 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f32()
}

fn rect(fb: &mut Framebuffer, l: i32, t: i32, r: i32, b: i32, c: u32) {
    let (l, t, r, b) = (l as f32, t as f32, r as f32, b as f32);
    fb.line(l, t, r, t, c);
    fb.line(r, t, r, b, c);
    fb.line(r, b, l, b, c);
    fb.line(l, b, l, t, c);
}

fn diamond(fb: &mut Framebuffer, x: i32, y: i32, r: i32, c: u32, filled: bool) {
    let (x, y, r) = (x as f32, y as f32, r as f32);
    fb.line(x, y - r, x + r, y, c);
    fb.line(x + r, y, x, y + r, c);
    fb.line(x, y + r, x - r, y, c);
    fb.line(x - r, y, x, y - r, c);
    if filled {
        fb.line(x - r + 2.0, y, x + r - 2.0, y, c);
        fb.line(x, y - r + 2.0, x, y + r - 2.0, c);
    }
}

/// A button: just its text, lit when the pointer is over it. Returns whether it is.
pub fn button(
    fb: &mut Framebuffer,
    g: &Game,
    rc: (i32, i32, i32, i32),
    label: &str,
    act: Act,
) -> bool {
    let hover = g.hit(rc, act, None);
    let c = if hover { AMBER } else { GREEN };
    let w = label.chars().count() as i32 * 8;
    let x = rc.0 + (rc.2 - rc.0 - w) / 2;
    let y = rc.1 + (rc.3 - rc.1 - 7) / 2;
    fb.text_at(x, y, label, c);
    if hover {
        // A thin underline under the word, nothing more.
        fb.line(
            x as f32,
            (y + 10) as f32,
            (x + w) as f32,
            (y + 10) as f32,
            AMBER,
        );
    }
    hover
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    fb.text(1, 1, "PROTOTYPES D'INTERFACE", AMBER);
    let names = ["LE CORPS", "LE HALL DE LA STATION"];
    for (i, n) in names.iter().enumerate() {
        let x0 = 24 + i as i32 * 220;
        let on = g.proto as usize == i;
        let hover = g.hit(
            (x0, 28, x0 + 208, 50),
            Act::Call(Game::click_proto, i),
            None,
        );
        let c = if on {
            AMBER
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        let label = format!("{} {}", i + 1, n);
        fb.text_at(x0 + 10, 35, &label, c);
        if on {
            fb.line(
                (x0 + 10) as f32,
                45.0,
                (x0 + 10) as f32 + label.chars().count() as f32 * 8.0,
                45.0,
                AMBER,
            );
        }
    }
    match g.proto {
        0 => body(fb, g),
        _ => hall(fb, g),
    }
    fb.text(
        1,
        fb.rows() - 3,
        "CLIC SUR LES ELEMENTS   GAUCHE/DROITE OU 1-2 : PROTOTYPE   ECHAP : RETOUR   (PROTOTYPE : RIEN N'EST ENCORE BRANCHE)",
        DIM,
    );
    if let Some(m) = g.messages.last() {
        fb.text(1, fb.rows() - 2, m, AMBER);
    }
}

// ---- the body ---------------------------------------------------------------------------

fn body(fb: &mut Framebuffer, g: &Game) {
    let t = phase();
    let cx = 640;
    let top = 80;
    // Silhouette in wireframe.
    let o = |dx: i32, y: i32| ((cx + dx) as f32, (top + y) as f32);
    type Seg = ((i32, i32), (i32, i32));
    let lines: &[Seg] = &[
        ((-62, 120), (62, 120)),
        ((-62, 120), (-50, 300)),
        ((62, 120), (50, 300)),
        ((-50, 300), (50, 300)),
        ((-62, 120), (-92, 215)),
        ((-92, 215), (-84, 300)),
        ((62, 120), (92, 215)),
        ((92, 215), (84, 300)),
        ((-50, 300), (-42, 420)),
        ((-42, 420), (-38, 500)),
        ((50, 300), (42, 420)),
        ((42, 420), (38, 500)),
        ((-2, 300), (-6, 420)),
        ((-6, 420), (-8, 500)),
        ((2, 300), (6, 420)),
        ((6, 420), (8, 500)),
        ((-8, 100), (-8, 120)),
        ((8, 100), (8, 120)),
    ];
    for &((x0, y0), (x1, y1)) in lines {
        let (a, b) = (o(x0, y0), o(x1, y1));
        fb.line(a.0, a.1, b.0, b.1, WIRE);
    }
    fb.circle(cx as f32, (top + 66) as f32, 28.0, WIRE);
    // A scanning line sweeps the body.
    let sweep = top as f32 + 40.0 + (t * 70.0) % 460.0;
    fb.line(cx as f32 - 110.0, sweep, cx as f32 + 110.0, sweep, DIM);

    // Organs: where they sit on the silhouette (x offset, y), by id.
    let anchor = |id: &str| -> Option<(i32, i32)> {
        Some(match id {
            "cerveau" => (0, 56),
            "yeux" => (0, 70),
            "poumons" => (0, 160),
            "coeur" => (10, 172),
            "foie" => (28, 205),
            "estomac" => (-18, 210),
            "reins" => (0, 250),
            "muscles" => (-76, 190),
            "os" => (0, 150),
            "peau" => (50, 260),
            _ => return None,
        })
    };
    let member = g.crew_sel.min(g.sim.crew.members.len().saturating_sub(1));
    let person = g.sim.crew.members.get(member);
    let value = |i: usize| person.and_then(|p| p.organs.get(i)).copied().unwrap_or(1.0);
    let color_of = |v: f32| {
        if v <= 0.0 {
            RED
        } else if v < 0.5 {
            AMBER
        } else {
            GREEN
        }
    };
    // Little drawings of each organ on the body (they breathe and beat).
    for (i, def) in g.data.organs.iter().enumerate() {
        let Some((dx, y)) = anchor(&def.id) else {
            continue;
        };
        let c = if i == g.organ_sel {
            AMBER
        } else {
            color_of(value(i))
        };
        let (x, yy) = ((cx + dx) as f32, (top + y) as f32);
        match def.id.as_str() {
            "poumons" => {
                let b = 1.0 + 0.06 * (t * 2.0).sin();
                fb.circle(x - 24.0, yy, 15.0 * b, c);
                fb.circle(x + 24.0, yy, 15.0 * b, c);
            }
            "coeur" => {
                let b = 1.0 + 0.25 * (t * 7.0).sin().max(0.0);
                fb.circle(x, yy, 8.0 * b, c);
            }
            "yeux" => {
                fb.circle(x - 10.0, yy, 4.0, c);
                fb.circle(x + 10.0, yy, 4.0, c);
            }
            "cerveau" => {
                fb.circle(x, yy - 6.0, 14.0, c);
                fb.line(x, yy - 20.0, x, yy + 8.0, c);
            }
            "foie" => rect(
                fb,
                x as i32 - 14,
                yy as i32 - 6,
                x as i32 + 14,
                yy as i32 + 8,
                c,
            ),
            "estomac" => fb.circle(x, yy, 10.0, c),
            "reins" => {
                fb.circle(x - 22.0, yy, 6.0, c);
                fb.circle(x + 22.0, yy, 6.0, c);
            }
            "os" => {
                for k in 0..9 {
                    let yk = (top + 130 + k * 17) as f32;
                    fb.line(x - 3.0, yk, x + 3.0, yk, c);
                }
            }
            "muscles" => {
                fb.line(x, yy - 18.0, x, yy + 18.0, c);
                fb.line(x - 3.0, yy - 14.0, x - 3.0, yy + 14.0, c);
            }
            _ => {}
        }
    }
    // Cards on both sides, joined to their organ by a line.
    let card = |fb: &mut Framebuffer, i: usize, left: bool, slot: i32| {
        let def = &g.data.organs[i];
        let Some((dx, y)) = anchor(&def.id) else {
            return;
        };
        let v = value(i);
        let (x0, y0) = if left {
            (60, 110 + slot * 66)
        } else {
            (900, 110 + slot * 66)
        };
        let sel = i == g.organ_sel;
        let hover = g.hit(
            (x0, y0, x0 + 230, y0 + 52),
            Act::Call(Game::click_organ, i),
            None,
        );
        let c = if sel {
            AMBER
        } else if hover {
            HOVER
        } else {
            color_of(v)
        };
        fb.text_at(x0 + 8, y0 + 7, &def.name.to_uppercase(), c);
        let kind = match person.and_then(|p| p.organ_kind.get(i)) {
            Some(1) => "GREFFE",
            Some(2) => "PROTHESE",
            _ => "NATUREL",
        };
        fb.text_at(x0 + 120, y0 + 7, kind, DIM);
        let w = (200.0 * v.clamp(0.0, 1.0)) as i32;
        rect(fb, x0 + 8, y0 + 24, x0 + 208, y0 + 34, DIM);
        for k in 0..w / 2 {
            fb.line(
                (x0 + 9 + k * 2) as f32,
                (y0 + 26) as f32,
                (x0 + 9 + k * 2) as f32,
                (y0 + 32) as f32,
                c,
            );
        }
        fb.text_at(x0 + 8, y0 + 39, &format!("{:.0} %", v * 100.0), c);
        if def.vital {
            fb.text_at(x0 + 160, y0 + 39, "VITAL", RED);
        }
        // Leader line to the organ (dotted, pulsing when selected).
        let (ax, ay) = if left {
            (x0 + 230, y0 + 26)
        } else {
            (x0, y0 + 26)
        };
        let (bx, by) = ((cx + dx) as f32, (top + y) as f32);
        let steps = 24;
        for s in 0..steps {
            if s % 2 == 0 || (sel && ((t * 10.0) as i32 + s) % 3 == 0) {
                let (f0, f1) = (s as f32 / steps as f32, (s + 1) as f32 / steps as f32);
                fb.line(
                    ax as f32 + (bx - ax as f32) * f0,
                    ay as f32 + (by - ay as f32) * f0,
                    ax as f32 + (bx - ax as f32) * f1,
                    ay as f32 + (by - ay as f32) * f1,
                    if sel { AMBER } else { DIM },
                );
            }
        }
    };
    let left_ids = ["cerveau", "yeux", "poumons", "coeur", "estomac"];
    let right_ids = ["foie", "reins", "muscles", "os", "peau"];
    for (slot, id) in left_ids.iter().enumerate() {
        if let Some(i) = g.data.organ_index(id) {
            card(fb, i, true, slot as i32);
        }
    }
    for (slot, id) in right_ids.iter().enumerate() {
        if let Some(i) = g.data.organ_index(id) {
            card(fb, i, false, slot as i32);
        }
    }
    // Implant slots: diamonds on the body, filled when something is installed (demo data here).
    let slots: [(&str, i32, i32); 7] = [
        ("TETE", 44, 50),
        ("NEURO", -44, 40),
        ("TORSE", 78, 140),
        ("BRAS G", -108, 235),
        ("BRAS D", 108, 235),
        ("JAMBES", 0, 400),
        ("PEAU", 60, 330),
    ];
    for (k, (name, dx, y)) in slots.iter().enumerate() {
        let (x, yy) = (cx + dx, top + y);
        let hover = g.hit(
            (x - 12, yy - 12, x + 12, yy + 12),
            Act::Call(Game::click_proto_slot, k),
            None,
        );
        let on = (g.proto_slot == k) || hover;
        let filled = k % 3 == 0;
        diamond(fb, x, yy, 9, if on { AMBER } else { CYAN }, filled);
        if on {
            fb.text_at(x + 14, yy - 3, &format!("IMPLANT {name}"), AMBER);
        }
    }
    // Header and action buttons.
    if let Some(p) = person {
        fb.text(
            1,
            7,
            &format!(
                "{}  -  {}   ETAT GENERAL {:.0} %",
                p.name.to_uppercase(),
                g.data.roles[p.role].name.to_uppercase(),
                p.condition(&g.data) * 100.0
            ),
            GREEN,
        );
    }
    let y = 650;
    for (i, (label, n)) in [
        ("GREFFER", 0usize),
        ("PROTHESE", 1),
        ("PRELEVER", 2),
        ("IMPLANTER", 3),
        ("RETIRER", 4),
    ]
    .iter()
    .enumerate()
    {
        let x0 = 340 + i as i32 * 130;
        button(
            fb,
            g,
            (x0, y, x0 + 118, y + 26),
            label,
            Act::Call(Game::click_proto_action, *n),
        );
    }
}

// ---- the station hall -------------------------------------------------------------------------

fn hall(fb: &mut Framebuffer, g: &Game) {
    let t = phase();
    let (cx, cy) = (640.0f32, 360.0f32);
    // The station: a rotating ring with docking arms.
    fb.circle(cx, cy, 120.0, WIRE);
    fb.circle(cx, cy, 150.0, DIM);
    fb.circle(cx, cy, 36.0, GREEN);
    for k in 0..8 {
        let a = std::f32::consts::TAU * k as f32 / 8.0 + t * 0.12;
        fb.line(
            cx + 36.0 * a.cos(),
            cy + 36.0 * a.sin(),
            cx + 120.0 * a.cos(),
            cy + 120.0 * a.sin(),
            WIRE,
        );
    }
    // Lights running around the outer ring.
    for k in 0..16 {
        let a = std::f32::consts::TAU * k as f32 / 16.0 - t * 0.6;
        let lit = ((t * 3.0) as i32 + k) % 4 == 0;
        fb.circle(
            cx + 150.0 * a.cos(),
            cy + 150.0 * a.sin(),
            2.0,
            if lit { AMBER } else { DIM },
        );
    }
    // Traffic: small ships on approach.
    for k in 0..3 {
        let f = ((t * 0.15 + k as f32 / 3.0) % 1.0).max(0.0);
        let a = 0.7 + k as f32 * 2.1;
        let r = 420.0 - 270.0 * f;
        let (x, y) = (cx + r * a.cos(), cy + r * 0.6 * a.sin());
        fb.line(x - 6.0, y, x, y - 3.0, CYAN);
        fb.line(x, y - 3.0, x + 6.0, y, CYAN);
        fb.line(x + 6.0, y, x, y + 3.0, CYAN);
        fb.line(x, y + 3.0, x - 6.0, y, CYAN);
    }
    fb.text_at(cx as i32 - 44, cy as i32 - 4, "STATION", GREEN);
    // Doors: buttons around the ring, each wired to the core.
    let doors = [
        ("MARCHE", "ACHETER ET VENDRE", 0.0f32),
        ("BOURSE", "COURS ET GRAPHES", 0.785),
        ("CONTRATS", "MISSIONS DE LIVRAISON", 1.571),
        ("BANQUE", "PRETS ET DETTES", 2.356),
        ("CHANTIER", "COQUES ET MODULES", std::f32::consts::PI),
        ("RECRUTER", "NOUVEAUX EQUIPIERS", 3.927),
        ("HOPITAL", "GREFFES ET IMPLANTS", 4.712),
        ("CODEX", "TOUT SAVOIR", 5.498),
    ];
    for (k, (name, tip, a)) in doors.iter().enumerate() {
        let (px, py) = (cx + 330.0 * a.cos(), cy + 215.0 * a.sin());
        let rc = (
            px as i32 - 78,
            py as i32 - 22,
            px as i32 + 78,
            py as i32 + 22,
        );
        // The wire first, then the box on top.
        let (ex, ey) = (cx + 150.0 * a.cos(), cy + 150.0 * a.sin());
        let hover = g.hit(rc, Act::Call(Game::click_proto_door, k), None);
        let (tx, ty) = if a.cos().abs() > 0.5 {
            (px - 78.0 * a.cos().signum(), py)
        } else {
            (px, py - 22.0 * a.sin().signum())
        };
        fb.line(ex, ey, tx, ty, if hover { AMBER } else { DIM });
        let c = if hover { HOVER } else { GREEN };
        fb.text_at(rc.0 + 10, rc.1 + 8, name, c);
        fb.text_at(
            rc.0 + 10,
            rc.1 + 26,
            &tip.chars().take(19).collect::<String>(),
            DIM,
        );
    }
    // Ticker with a few prices, scrolling.
    let items = [
        "FER 0.4",
        "EAU 1.2",
        "ELECTRONIQUE 61",
        "GREFFON COEUR 90000",
        "PROTHESE OEIL 16000",
    ];
    let line = items.join("   |   ");
    let off = ((t * 40.0) as usize) % (line.len() + 40);
    let shown: String = std::iter::repeat_n(' ', 40)
        .chain(line.chars())
        .skip(off)
        .take(120)
        .collect();
    fb.text(1, fb.rows() - 5, &shown, CYAN);
}

impl Game {
    pub fn click_proto(&mut self, i: usize) {
        self.proto = i as u8;
    }

    pub fn click_proto_slot(&mut self, k: usize) {
        self.proto_slot = k;
        self.say("PROTOTYPE : EMPLACEMENT D'IMPLANT CHOISI");
    }

    pub fn click_proto_action(&mut self, n: usize) {
        let names = ["GREFFER", "PROTHESE", "PRELEVER", "IMPLANTER", "RETIRER"];
        self.say(format!("PROTOTYPE : {} SUR L'ORGANE CHOISI", names[n % 5]));
    }

    pub fn click_proto_door(&mut self, k: usize) {
        self.say(format!(
            "PROTOTYPE : PORTE {} (OUVRIRAIT L'ECRAN CORRESPONDANT)",
            k + 1
        ));
    }

    pub fn proto_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::Tab => self.proto = 1 - self.proto,
            KeyCode::Digit1 => self.proto = 0,
            KeyCode::Digit2 => self.proto = 1,
            KeyCode::Escape | KeyCode::F12 => self.screen = crate::game::Screen::Flight,
            _ => {}
        }
    }
}
