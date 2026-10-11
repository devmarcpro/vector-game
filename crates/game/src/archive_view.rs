//! ARCHIVES: the third tab of the journal. The history of the sector the ship is in (the ages, the
//! recent years told by politics) and, below, the finds of the digs on foot: date them, translate
//! them, sell them, give them to a museum or hand them back to the heirs.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use vector_render::Framebuffer;
use vector_sim::archive::{artefact_line, Dest};
use winit::keyboard::KeyCode;

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
    fb.text(1, 1, "ARCHIVES - HISTOIRE ET ARTEFACTS", AMBER);
    fb.text(40, 1, "[TAB] JOURNAL DE BORD", DIM);
    let lines = s.sector_history();
    let rows = fb.rows();
    let hist_rows = (rows.saturating_sub(16)).max(6);
    let start = g.journal_scroll.min(lines.len().saturating_sub(1));
    for (i, l) in lines.iter().skip(start).take(hist_rows).enumerate() {
        let col = if i == 0 && start == 0 { AMBER } else { GREEN };
        fb.text(1, 3 + i, l, col);
    }
    let y = 5 + lines.len().min(hist_rows);
    let n = s.archive.artefacts.len();
    fb.text(
        1,
        y,
        &format!(
            "ARTEFACTS ({n})   FOUILLES REALISEES {}",
            s.archive.dug.len()
        ),
        AMBER,
    );
    if n == 0 {
        fb.text(
            1,
            y + 1,
            "(RIEN : FOUILLEZ DES RUINES ET DES EPAVES A PIED, F8 PUIS FIN)",
            DIM,
        );
    }
    let sel = g.arch_sel.min(n.saturating_sub(1));
    for i in 0..n.min(5) {
        let k = (sel / 5) * 5 + i;
        if k >= n {
            break;
        }
        let hov = g.hit_cells(
            1,
            y + 1 + i,
            90,
            1,
            Act::Call(Game::click_artefact, k),
            None,
        );
        let col = if k == sel || hov { AMBER } else { GREEN };
        let mark = if k == sel { ">" } else { " " };
        fb.text(
            1,
            y + 1 + i,
            &format!("{mark} {}", artefact_line(s, k)),
            col,
        );
    }
    if n > 0 {
        let a = &s.archive.artefacts[sel];
        let def = &g.data.history.artefacts[a.kind];
        fb.text(1, y + 7, &def.text, DIM);
        fb.text(
            1,
            y + 8,
            &vector_sim::history::provenance_line(
                &g.data,
                &vector_sim::history::Provenance {
                    age: 0,
                    builder: a.builder,
                    built: a.built,
                    fallen: a.fallen,
                    cause: a.cause,
                },
            ),
            DIM,
        );
        if a.suspect(s.archive_years_now()) {
            fb.text(
                1,
                y + 9,
                "LA DATATION CONTREDIT SON APPARENCE : C'EST UN FAUX.",
                RED,
            );
        }
    }
    let by = rows.saturating_sub(8);
    let mut x = 1;
    x = word(fb, g, x, by, "[D] DATER", KeyCode::KeyD);
    x = word(fb, g, x, by, "[C] COLLECTIONNEUR", KeyCode::KeyC);
    x = word(fb, g, x, by, "[M] MUSEE", KeyCode::KeyM);
    x = word(fb, g, x, by, "[R] RESTITUER", KeyCode::KeyR);
    let t = if s.archive.careful {
        "[T] FOUILLE PRUDENTE : OUI"
    } else {
        "[T] FOUILLE PRUDENTE : NON (PILLAGE)"
    };
    word(fb, g, x, by, t, KeyCode::KeyT);
    fb.text(
        1,
        rows.saturating_sub(7),
        "HAUT/BAS:ARTEFACT  PGHAUT/PGBAS:HISTOIRE  TAB:ONGLET  J/ECHAP:RETOUR",
        DIM,
    );
}

impl Game {
    pub fn click_artefact(&mut self, k: usize) {
        self.arch_sel = k;
    }

    /// Keys of the archives tab; returns whether the key was taken.
    pub fn archive_key(&mut self, code: KeyCode) -> bool {
        let n = self.sim.archive.artefacts.len();
        let sel = self.arch_sel.min(n.saturating_sub(1));
        match code {
            KeyCode::ArrowUp if n > 0 => self.arch_sel = (sel + n - 1) % n,
            KeyCode::ArrowDown if n > 0 => self.arch_sel = (sel + 1) % n,
            KeyCode::PageUp => self.journal_scroll = self.journal_scroll.saturating_sub(5),
            KeyCode::PageDown => self.journal_scroll += 5,
            KeyCode::KeyT => {
                self.sim.toggle_careful_dig();
                self.say(if self.sim.archive.careful {
                    "FOUILLE PRUDENTE : MOINS DE TROUVAILLES, TOUT EST DOCUMENTE"
                } else {
                    "PILLAGE : PLUS DE TROUVAILLES, LE CONTEXTE EST DETRUIT"
                });
            }
            KeyCode::KeyD | KeyCode::KeyC | KeyCode::KeyM | KeyCode::KeyR if n > 0 => {
                let res = match code {
                    KeyCode::KeyD => self.sim.date_artefact(sel),
                    KeyCode::KeyC => self.sim.dispose_artefact(sel, Dest::Collector),
                    KeyCode::KeyM => self.sim.dispose_artefact(sel, Dest::Museum),
                    _ => self.sim.dispose_artefact(sel, Dest::Heirs),
                };
                match res {
                    Ok(m) => self.say(m.to_uppercase()),
                    Err(e) => self.say(e.to_uppercase()),
                }
            }
            _ => return false,
        }
        true
    }
}
