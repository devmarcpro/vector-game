//! Portals, dimensions and time (Home key, in flight or docked): the portal within reach, the rules
//! of the dimension you are in, the time anchors and the stasis chamber.
use crate::game::{Game, Screen};
use crate::hud::{AMBER, DIM, GREEN};
use crate::mouse::Act;
use vector_render::Framebuffer;
use vector_sim::bourse::DAY;
use vector_sim::multiverse::{
    ANCHOR_FEE, MAX_ANCHORS, PORTAL_RANGE, REWIND_FEE, STASIS_COST, STASIS_MAX,
};
use winit::keyboard::KeyCode;

const WHITE: u32 = 0x00FF_FFFF;
const UP: u32 = 0x0040_E070;
const DOWN: u32 = 0x00FF_5050;

#[derive(Default)]
pub struct CosmosUi {
    pub anchor: usize,
    /// Days of the next stasis (in steps of 10, at least 10).
    pub stasis: usize,
}

impl Game {
    pub fn click_anchor(&mut self, i: usize) {
        self.cosmos_ui.anchor = i;
    }

    pub fn stasis_days(&self) -> f64 {
        (self.cosmos_ui.stasis.max(1) * 10) as f64
    }

    pub fn cosmos_key(&mut self, code: KeyCode) {
        let n = self.sim.multiverse.anchors.len();
        let r: Result<String, &str> = match code {
            KeyCode::Enter => self.sim.use_portal(),
            KeyCode::KeyA => self
                .sim
                .set_anchor()
                .map(|c| format!("ANCRE POSEE ({c:.0} CR)")),
            KeyCode::KeyR => self.sim.rewind(self.cosmos_ui.anchor),
            KeyCode::KeyX => self
                .sim
                .drop_anchor(self.cosmos_ui.anchor)
                .map(|()| "ANCRE LEVEE".to_string()),
            KeyCode::ArrowUp if n > 0 => {
                self.cosmos_ui.anchor = (self.cosmos_ui.anchor + n - 1) % n;
                return;
            }
            KeyCode::ArrowDown if n > 0 => {
                self.cosmos_ui.anchor = (self.cosmos_ui.anchor + 1) % n;
                return;
            }
            KeyCode::Equal | KeyCode::NumpadAdd => {
                self.cosmos_ui.stasis = (self.cosmos_ui.stasis.max(1) + 1).min(36);
                return;
            }
            KeyCode::Minus | KeyCode::NumpadSubtract => {
                self.cosmos_ui.stasis = self.cosmos_ui.stasis.max(2) - 1;
                return;
            }
            KeyCode::KeyS => {
                let d = self.stasis_days();
                self.sim
                    .stasis(d)
                    .map(|c| format!("{d:.0} JOURS DE STASE ({c:.0} CR)"))
            }
            KeyCode::Escape | KeyCode::Home => {
                self.screen = if self.sim.docked.is_some() {
                    Screen::Station
                } else {
                    Screen::Flight
                };
                return;
            }
            _ => return,
        };
        let n = self.sim.multiverse.anchors.len();
        self.cosmos_ui.anchor = self.cosmos_ui.anchor.min(n.saturating_sub(1));
        match r {
            Ok(m) => self.say(m),
            Err(e) => self.say(e.to_uppercase()),
        }
    }
}

fn word(
    fb: &mut Framebuffer,
    g: &Game,
    col: usize,
    row: usize,
    label: &str,
    key: KeyCode,
) -> usize {
    let hov = g.hit_cells(col, row, label.len(), 1, Act::Key(key), None);
    fb.text(col, row, label, if hov { AMBER } else { GREEN });
    col + label.len() + 3
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    fb.text(1, 1, "== PORTAILS, DIMENSIONS ET TEMPS ==", AMBER);
    fb.text(40, 1, &format!("CREDITS {:.0}", s.credits), GREEN);

    // Portal and dimension.
    fb.text(1, 3, "PORTAIL", AMBER);
    match vector_world::portal_index(&s.system) {
        None => fb.text(3, 4, "AUCUN PORTAIL DANS CE SYSTEME.", DIM),
        Some(i) => {
            let d = (s.bodies[i].pos - s.ship.pos).length();
            let near = d <= PORTAL_RANGE;
            fb.text(
                3,
                4,
                &format!(
                    "{} A {:.0} KM (PORTEE {:.0} KM)",
                    s.bodies[i].name.to_uppercase(),
                    d / 1000.0,
                    PORTEE_KM
                ),
                if near { UP } else { GREEN },
            );
            if near {
                word(
                    fb,
                    g,
                    3,
                    5,
                    "[ENTREE] FRANCHIR LE PORTAIL (5 % DU CARBURANT)",
                    KeyCode::Enter,
                );
            } else {
                fb.text(3, 5, "APPROCHEZ-VOUS POUR LE FRANCHIR.", DIM);
            }
        }
    }
    match s.dimension_def() {
        None => fb.text(3, 7, "VOUS ETES DANS NOTRE DIMENSION.", DIM),
        Some(d) => {
            fb.text(3, 7, &d.name.to_uppercase(), AMBER);
            fb.text(3, 8, &d.text, DIM);
            fb.text(
                3,
                9,
                &format!(
                    "MINERAI x{:.1}   DANGERS x{:.1}   ETUDE x{:.1}   INSTABILITE +{:.0} %/J   TEMPS DEHORS x{:.1}",
                    d.ore_scale,
                    d.hazard_scale,
                    d.research_scale,
                    d.instability * 100.0,
                    d.time_ratio
                ),
                GREEN,
            );
        }
    }

    // Time.
    fb.text(1, 12, "TEMPS", AMBER);
    let mv = &s.multiverse;
    fb.text(
        3,
        13,
        &format!(
            "PARADOXE {}   ANCRES {}/{}   {}",
            mv.paradox,
            mv.anchors.len(),
            MAX_ANCHORS,
            if mv.memory_until > s.time {
                format!(
                    "SOUVENIRS DU FUTUR JUSQU'A J+{:.0}",
                    (mv.memory_until - s.time) / DAY
                )
            } else {
                "AUCUN SOUVENIR DU FUTUR".to_string()
            }
        ),
        if mv.paradox > 2 { DOWN } else { GREEN },
    );
    for (i, a) in mv.anchors.iter().enumerate() {
        let sel = i == g.cosmos_ui.anchor;
        let hov = g.hit_cells(3, 15 + i, 60, 1, Act::Call(Game::click_anchor, i), None);
        fb.text(
            3,
            15 + i,
            &format!(
                "{}ANCRE {}  JOUR {:.0}  ({:.1} J DANS LE PASSE)",
                if sel { ">" } else { " " },
                i + 1,
                a.time / DAY,
                (s.time - a.time) / DAY
            ),
            if sel {
                AMBER
            } else if hov {
                WHITE
            } else {
                GREEN
            },
        );
    }
    let mut x = 3;
    x = word(
        fb,
        g,
        x,
        19,
        &format!("[A] POSER UNE ANCRE ({ANCHOR_FEE:.0} CR)"),
        KeyCode::KeyA,
    );
    x = word(
        fb,
        g,
        x,
        19,
        &format!("[R] REMONTER ({REWIND_FEE:.0} CR)"),
        KeyCode::KeyR,
    );
    word(fb, g, x, 19, "[X] LEVER", KeyCode::KeyX);
    fb.text(
        3,
        20,
        "REMONTER RESTAURE TOUT, SAUF VOS SOUVENIRS : LES EVENEMENTS DU MONDE VOUS SONT CONNUS (RUMEURS CERTAINES).",
        DIM,
    );
    fb.text(
        3,
        21,
        "CHAQUE REMONTEE LAISSE UN PARADOXE : L'EQUIPAGE S'EN RESSENT.",
        DIM,
    );

    // Stasis.
    fb.text(1, 24, "STASE", AMBER);
    let days = g.stasis_days();
    fb.text(
        3,
        25,
        &format!(
            "{days:.0} JOURS (+/-, JUSQU'A {STASIS_MAX:.0}) POUR {:.0} CR : LE MONDE AVANCE, A BORD RIEN NE VIEILLIT NI NE MANGE.",
            STASIS_COST * days
        ),
        if s.docked.is_some() { GREEN } else { DIM },
    );
    if s.docked.is_some() {
        word(fb, g, 3, 26, "[S] DORMIR", KeyCode::KeyS);
    } else {
        fb.text(3, 26, "A QUAI SEULEMENT.", DIM);
    }
    for (i, m) in g.messages.iter().rev().take(1).enumerate() {
        fb.text(1, 30 + i, m, AMBER);
    }
}

const PORTEE_KM: f64 = PORTAL_RANGE / 1000.0;
