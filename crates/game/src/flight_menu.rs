//! The piloting menu: a column of clickable words on the right of the cockpit that opens every
//! screen and operates the ship's controls (a held click on a thrust, brake, boost or weapon
//! word works like holding the key). F11 or a click on its title folds it.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, HOVER};
use crate::mouse::Act;
use vector_render::Framebuffer;
use winit::keyboard::KeyCode;

/// (label, key shown, action)
type Entry = (&'static str, String, Act);

fn menus() -> Vec<Entry> {
    let raw: [(&str, &str, Act); 15] = [
        ("CARTE SYSTEME", "M", Act::Key(KeyCode::KeyM)),
        ("GALAXIE", "L", Act::Key(KeyCode::KeyL)),
        ("VAISSEAU", "H", Act::Key(KeyCode::KeyH)),
        ("SOUTE", "I", Act::Key(KeyCode::KeyI)),
        ("EQUIPAGE", "K", Act::Key(KeyCode::KeyK)),
        ("JOURNAL", "J", Act::Key(KeyCode::KeyJ)),
        ("EMPIRE", "F2", Act::Key(KeyCode::F2)),
        ("FLOTTE", "F3", Act::Key(KeyCode::F3)),
        ("TECHNOLOGIES", "F4", Act::Key(KeyCode::F4)),
        ("CODEX", "F6", Act::Key(KeyCode::F6)),
        ("FACTIONS", "X", Act::Key(KeyCode::KeyX)),
        ("SURFACE", "F8", Act::Key(KeyCode::F8)),
        ("APPELER (RADIO)", "ENTR", Act::Key(KeyCode::Enter)),
        ("PORTAIL / TEMPS", "ACCUEIL", Act::Key(KeyCode::Home)),
        ("EXPLORER A PIED", "FIN", Act::Key(KeyCode::End)),
    ];
    raw.into_iter()
        .map(|(a, b, c)| (a, b.to_string(), c))
        .collect()
}

fn controls(g: &Game) -> Vec<Entry> {
    let mv = g.settings.move_keys();
    let nm = crate::context_menu::key_name;
    let raw: Vec<(&str, String, Act)> = vec![
        ("ASSISTANCE", "F".into(), Act::Key(KeyCode::KeyF)),
        ("AMARRER", "D".into(), Act::Key(KeyCode::KeyD)),
        ("PILOTE AUTO", "G".into(), Act::Key(KeyCode::KeyG)),
        ("CIBLE SUIVANTE", "TAB".into(), Act::Key(KeyCode::Tab)),
        (
            "CIBLE PLANETE",
            "MAJ+TAB".into(),
            Act::Call(Game::menu_target_planet, 0),
        ),
        ("PAUSE", "P".into(), Act::Key(KeyCode::KeyP)),
        ("TEMPS +", "PGUP".into(), Act::Key(KeyCode::PageUp)),
        ("TEMPS -", "PGDN".into(), Act::Key(KeyCode::PageDown)),
        ("POUSSEE (MAINT.)", "ESP".into(), Act::Hold(KeyCode::Space)),
        ("RECUL (MAINT.)", "X".into(), Act::Hold(KeyCode::KeyX)),
        ("FREIN (MAINT.)", "C".into(), Act::Hold(KeyCode::KeyC)),
        (
            "BOOST (MAINT.)",
            "MAJ".into(),
            Act::Hold(KeyCode::ShiftLeft),
        ),
        ("LASER (MAINT.)", "V".into(), Act::Hold(KeyCode::KeyV)),
        (
            "TIR (MAINT.)",
            "CTRL".into(),
            Act::Hold(KeyCode::ControlLeft),
        ),
        ("SAUVER", "F5".into(), Act::Key(KeyCode::F5)),
        (
            "BOUCLIER ON/OFF",
            "PAV/".into(),
            Act::Key(KeyCode::NumpadDivide),
        ),
        ("LUMIERES", "PAV.".into(), Act::Key(KeyCode::NumpadDecimal)),
        (
            "TRIM POUSSEE +",
            "]".into(),
            Act::Key(KeyCode::BracketRight),
        ),
        ("TRIM POUSSEE -", "[".into(), Act::Key(KeyCode::BracketLeft)),
        (
            "DISPOSITION COCKPIT",
            "MAJ+F11".into(),
            Act::Call(Game::cockpit_cycle, 0),
        ),
        ("CONSOLE REPLIABLE", "'".into(), Act::Key(KeyCode::Quote)),
        (
            "ORBITE CIRCULAIRE",
            "INSER".into(),
            Act::Key(KeyCode::Insert),
        ),
        ("VUE EXTERNE", "SUPPR".into(), Act::Key(KeyCode::Delete)),
        (
            "TRAIN D'ATTERRISSAGE",
            "PAV3".into(),
            Act::Key(KeyCode::Numpad3),
        ),
        (
            "ATTERRIR (CIBLE)",
            "PAV4".into(),
            Act::Call(Game::menu_land, 0),
        ),
        ("DECOLLER", "PAV4".into(), Act::Call(Game::menu_takeoff, 0)),
        (
            "DEBARQUER A PIED ICI",
            "FIN".into(),
            Act::Call(Game::menu_disembark, 0),
        ),
        ("SITE PLAT", "PAV7".into(), Act::Key(KeyCode::Numpad7)),
        ("TANGAGE HAUT (MAINT.)", nm(mv[0]), Act::Hold(mv[0])),
        ("TANGAGE BAS (MAINT.)", nm(mv[1]), Act::Hold(mv[1])),
        ("LACET GAUCHE (MAINT.)", nm(mv[3]), Act::Hold(mv[3])),
        ("LACET DROITE (MAINT.)", nm(mv[2]), Act::Hold(mv[2])),
        ("ROULIS GAUCHE (MAINT.)", nm(mv[4]), Act::Hold(mv[4])),
        ("ROULIS DROIT (MAINT.)", nm(mv[5]), Act::Hold(mv[5])),
        (
            "RCS HAUT (MAINT.)",
            "HAUT".into(),
            Act::Hold(KeyCode::ArrowUp),
        ),
        (
            "RCS BAS (MAINT.)",
            "BAS".into(),
            Act::Hold(KeyCode::ArrowDown),
        ),
        (
            "RCS GAUCHE (MAINT.)",
            "GAUCHE".into(),
            Act::Hold(KeyCode::ArrowLeft),
        ),
        (
            "RCS DROITE (MAINT.)",
            "DROITE".into(),
            Act::Hold(KeyCode::ArrowRight),
        ),
        ("MISSILES (MAINT.)", "B".into(), Act::Hold(KeyCode::KeyB)),
        ("LEURRE THERMIQUE", ",".into(), Act::Key(KeyCode::Comma)),
        ("RADAR ACTIF", ".".into(), Act::Key(KeyCode::Period)),
        ("ABORDER A PIED", "/".into(), Act::Key(KeyCode::Slash)),
        ("COLONISER LA CIBLE", "O".into(), Act::Key(KeyCode::KeyO)),
        ("EPAVE : RECUPERER", "N".into(), Act::Key(KeyCode::KeyN)),
        ("EPAVE : PILLER", "U".into(), Act::Key(KeyCode::KeyU)),
        ("VISEE SOURIS", "F7".into(), Act::Key(KeyCode::F7)),
        (
            "NAVIGATION AUTO",
            "PAV*".into(),
            Act::Key(KeyCode::NumpadMultiply),
        ),
        ("ZOOM CARTE +", "+".into(), Act::Key(KeyCode::Equal)),
        ("ZOOM CARTE -", "-".into(), Act::Key(KeyCode::Minus)),
    ];
    raw
}

/// Whether the menu is on screen right now.
pub fn visible(g: &Game) -> bool {
    g.screen == crate::game::Screen::Flight
        && g.settings.flight_menu
        && !g.mouse_look
        && !g.console.open
        // With the cockpit console the menu is an extra: F11 shows it over the window.
        && (g.cockpit.last.get().is_none() || g.cockpit.menu.get())
}

/// Whether the column is unfolded: always when the cockpit shows it on request.
fn unfolded(g: &Game) -> bool {
    g.cockpit.last.get().is_some() || g.flight_menu_open
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    if !visible(g) {
        return;
    }
    let cols = fb.cols();
    let w = 30usize;
    let x = cols.saturating_sub(w + 1);
    let title = if unfolded(g) {
        "[-] PILOTAGE (F11)"
    } else {
        "[+] PILOTAGE (F11)"
    };
    let hover = g.hit_cells(x, 3, title.len(), 1, Act::Key(KeyCode::F11), None);
    fb.text(x, 3, title, if hover { HOVER } else { AMBER });
    if !unfolded(g) {
        return;
    }
    // Over the console the list gets its own dark backing.
    if g.cockpit.last.get().is_some() {
        let h = (fb.rows().saturating_sub(4) * 10) as i32;
        fb.rect_over((x * 8) as i32 - 4, 28, (w * 8) as i32 + 8, h, 0x0006_0A10);
    }
    let mut row = 5;
    for (heading, entries) in [("MENUS", menus()), ("COMMANDES", controls(g))] {
        fb.text(x, row, heading, DIM);
        row += 1;
        for (label, key, act) in entries {
            // A small window shows what fits (the rest is in the context menu and the keys).
            if row + 3 >= fb.rows() {
                break;
            }
            let hover = g.hit_cells(x, row, w, 1, act, None);
            let c = if hover { AMBER } else { GREEN };
            fb.text(x, row, label, c);
            fb.text(x + w - key.chars().count(), row, &key, DIM);
            row += 1;
        }
        row += 1;
    }
}

impl Game {
    pub fn menu_target_planet(&mut self, _: usize) {
        self.target_planet();
    }

    pub fn toggle_flight_menu(&mut self) {
        if self.cockpit.last.get().is_some() {
            self.cockpit.menu.set(!self.cockpit.menu.get());
        } else {
            self.flight_menu_open = !self.flight_menu_open;
        }
    }
}
