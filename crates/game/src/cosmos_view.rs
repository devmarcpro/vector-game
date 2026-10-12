//! Portals, dimensions and time (Home key, in flight or docked): the portal within reach, the rules
//! of the dimension you are in, the time anchors and the stasis chamber.
use crate::game::{Game, Screen};
use crate::hub::word;
use crate::hud::{AMBER, DIM, DOWN, GREEN, UP, WHITE};
use crate::mouse::Act;
use vector_render::Framebuffer;
use vector_sim::bourse::DAY;
use vector_sim::multiverse::{
    ANCHOR_FEE, MAX_ANCHORS, PORTAL_RANGE, REWIND_FEE, STASIS_COST, STASIS_MAX,
};
use winit::keyboard::KeyCode;

#[derive(Default)]
pub struct CosmosUi {
    pub anchor: usize,
    /// Days of the next stasis (in steps of 10, at least 10).
    pub stasis: usize,
    pub relay: usize,
    /// Crew member shown for cloning (index into the crew).
    pub clone: usize,
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
            KeyCode::Digit1 | KeyCode::Digit2 | KeyCode::Digit3 => {
                let how = match code {
                    KeyCode::Digit1 => 0,
                    KeyCode::Digit2 => 1,
                    _ => 2,
                };
                self.sim.meet_echo(how)
            }
            KeyCode::KeyB => self.sim.break_loop(),
            KeyCode::KeyN => {
                let k = self.sim.crew.members.len();
                if k > 0 {
                    self.cosmos_ui.clone = (self.cosmos_ui.clone + 1) % k;
                }
                return;
            }
            KeyCode::KeyK => self.sim.clone_crew(self.cosmos_ui.clone),
            KeyCode::KeyO => self.sim.build_outpost(),
            KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                let k = self.sim.relay_nodes().len();
                if k > 0 {
                    let r = &mut self.cosmos_ui.relay;
                    *r = if code == KeyCode::ArrowRight {
                        (*r + 1) % k
                    } else {
                        (*r + k - 1) % k
                    };
                }
                return;
            }
            KeyCode::KeyJ => {
                let nodes = self.sim.relay_nodes();
                match nodes.get(self.cosmos_ui.relay) {
                    Some(n) => self.sim.relay_jump(n.0),
                    None => Err("aucun relais choisi"),
                }
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
    draw_oddities(fb, g);
    draw_high_scifi(fb, g);
    for (i, m) in g.messages.iter().rev().take(1).enumerate() {
        fb.text(1, 37 + i, m, AMBER);
    }
}

/// Exotic systems of the ship, the Kardashev scale and the cloning vat (right-hand column).
fn draw_high_scifi(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let x = 64;
    fb.text(x, 3, "HAUTE SCIENCE-FICTION", AMBER);
    fb.text(
        x + 2,
        4,
        &format!(
            "ECHELLE DE KARDASHEV DU VAISSEAU : K{:.2}",
            s.ship_kardashev()
        ),
        GREEN,
    );
    let lines = s.exotic_hud();
    if lines.is_empty() {
        fb.text(x + 2, 5, "AUCUN SYSTEME EXOTIQUE ACTIF.", DIM);
    }
    for (i, (l, alert)) in lines.iter().take(4).enumerate() {
        fb.text(x + 2, 5 + i, l, if *alert { DOWN } else { UP });
    }
    fb.text(
        x + 2,
        9,
        &format!(
            "LIAISON QUANTIQUE {}",
            if s.qcomm_active() {
                "ACTIVE : NOUVELLES INSTANTANEES"
            } else {
                "ABSENTE"
            }
        ),
        DIM,
    );
    fb.text(x, 11, "CLONAGE (1 KG NANITES, 80000 CR)", AMBER);
    let who = s
        .crew
        .members
        .get(g.cosmos_ui.clone)
        .map_or("PERSONNE".to_string(), |p| p.name.to_uppercase());
    fb.text(x + 2, 12, &who, WHITE);
    let nx = word(fb, g, x + 2, 13, "[N] SUIVANT", KeyCode::KeyN);
    word(fb, g, nx, 13, "[K] CLONER", KeyCode::KeyK);
}

/// Echoes, loops of time, relay portals and outposts.
fn draw_oddities(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    fb.text(1, 28, "ECHO", AMBER);
    match s.echo_ready() {
        Some(c) => {
            fb.text(
                3,
                29,
                &format!("UN ECHO DE VOTRE LIGNE ABANDONNEE VOUS ATTEND ({c:.0} CR) :"),
                UP,
            );
            let mut x = 3;
            x = word(fb, g, x, 30, "[1] L'AIDER", KeyCode::Digit1);
            x = word(fb, g, x, 30, "[2] LE COMBATTRE", KeyCode::Digit2);
            word(fb, g, x, 30, "[3] FUSIONNER", KeyCode::Digit3);
        }
        None => fb.text(
            3,
            29,
            "AUCUN ECHO. (UNE REMONTEE QUI PERD DES GAINS EN LAISSE UN)",
            DIM,
        ),
    }
    fb.text(1, 31, "BOUCLE DE TEMPS", AMBER);
    if s.in_time_loop() {
        fb.text(
            3,
            32,
            &format!(
                "CE SYSTEME REJOUE SES {:.0} JOURS ({} FOIS). PARTEZ QUAND VOUS VOULEZ.",
                vector_sim::multiverse::LOOP_DAYS,
                s.multiverse.loops_done
            ),
            UP,
        );
        word(
            fb,
            g,
            3,
            33,
            "[B] BRISER LA BOUCLE (5 ARTEFACTS)",
            KeyCode::KeyB,
        );
    } else {
        fb.text(3, 32, "CE SYSTEME VIT NORMALEMENT.", DIM);
    }
    fb.text(1, 34, "RELAIS ET AVANT-POSTES", AMBER);
    let nodes = s.relay_nodes();
    if s.relay_here() && !nodes.is_empty() {
        let k = g.cosmos_ui.relay.min(nodes.len() - 1);
        fb.text(
            3,
            35,
            &format!(
                "< {} > A {:.0} AL : {:.0} CR",
                nodes[k].1.to_uppercase(),
                nodes[k].2,
                nodes[k].2 * vector_sim::multiverse::RELAY_CR_PER_LY
            ),
            GREEN,
        );
        word(fb, g, 52, 35, "[J] TRAVERSER", KeyCode::KeyJ);
    } else {
        fb.text(
            3,
            35,
            "PORTAIL RELAIS : BATISSEZ-EN UN DANS DEUX COLONIES POUR LES RELIER.",
            DIM,
        );
    }
    fb.text(
        3,
        36,
        &format!(
            "AVANT-POSTES DIMENSIONNELS {}/{}   [O] EN BATIR UN PRES DU PORTAIL (DANS UNE DIMENSION)",
            s.multiverse.outposts.len(),
            vector_sim::multiverse::MAX_OUTPOSTS
        ),
        DIM,
    );
}

const PORTEE_KM: f64 = PORTAL_RANGE / 1000.0;
