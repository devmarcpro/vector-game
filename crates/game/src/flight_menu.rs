//! The piloting menu: a column of clickable words on the right of the cockpit that opens every
//! screen and operates the ship's controls (a held click on a thrust, brake, boost or weapon
//! word works like holding the key). F11 or a click on its title folds it.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN};
use crate::mouse::Act;
use vector_render::Framebuffer;
use winit::keyboard::KeyCode;

const HOVER: u32 = 0x00D0_FFD0;

/// (label, key shown, action)
type Entry = (&'static str, &'static str, Act);

fn menus() -> [Entry; 15] {
    [
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
    ]
}

fn controls() -> [Entry; 16] {
    [
        ("ASSISTANCE", "F", Act::Key(KeyCode::KeyF)),
        ("AMARRER", "D", Act::Key(KeyCode::KeyD)),
        ("PILOTE AUTO", "G", Act::Key(KeyCode::KeyG)),
        ("CIBLE SUIVANTE", "TAB", Act::Key(KeyCode::Tab)),
        (
            "CIBLE PLANETE",
            "MAJ+TAB",
            Act::Call(Game::menu_target_planet, 0),
        ),
        ("PAUSE", "P", Act::Key(KeyCode::KeyP)),
        ("TEMPS +", "PGUP", Act::Key(KeyCode::PageUp)),
        ("TEMPS -", "PGDN", Act::Key(KeyCode::PageDown)),
        ("POUSSEE (MAINT.)", "ESP", Act::Hold(KeyCode::Space)),
        ("RECUL (MAINT.)", "X", Act::Hold(KeyCode::KeyX)),
        ("FREIN (MAINT.)", "C", Act::Hold(KeyCode::KeyC)),
        ("BOOST (MAINT.)", "MAJ", Act::Hold(KeyCode::ShiftLeft)),
        ("LASER (MAINT.)", "V", Act::Hold(KeyCode::KeyV)),
        ("TIR (MAINT.)", "CTRL", Act::Hold(KeyCode::ControlLeft)),
        ("SAUVER", "F5", Act::Key(KeyCode::F5)),
        ("ORBITE CIRCULAIRE", "INSER", Act::Key(KeyCode::Insert)),
    ]
}

/// Whether the menu is on screen right now.
pub fn visible(g: &Game) -> bool {
    g.screen == crate::game::Screen::Flight
        && g.settings.flight_menu
        && !g.mouse_look
        && !g.console.open
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    if !visible(g) {
        return;
    }
    let cols = fb.cols();
    let w = 24usize;
    let x = cols.saturating_sub(w + 1);
    let title = if g.flight_menu_open {
        "[-] PILOTAGE (F11)"
    } else {
        "[+] PILOTAGE (F11)"
    };
    let hover = g.hit_cells(x, 3, title.len(), 1, Act::Key(KeyCode::F11), None);
    fb.text(x, 3, title, if hover { HOVER } else { AMBER });
    if !g.flight_menu_open {
        return;
    }
    let mut row = 5;
    for (heading, entries) in [
        ("MENUS", menus().to_vec()),
        ("COMMANDES", controls().to_vec()),
    ] {
        fb.text(x, row, heading, DIM);
        row += 1;
        for (label, key, act) in entries {
            let hover = g.hit_cells(x, row, w, 1, act, None);
            let c = if hover { AMBER } else { GREEN };
            fb.text(x, row, label, c);
            fb.text(x + w - key.len(), row, key, DIM);
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
        self.flight_menu_open = !self.flight_menu_open;
    }
}
