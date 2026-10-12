//! ESPRIT: the panel of the ship's mind, a page of the technology screen (F4 then E). The budget of
//! computing power between the five tasks, memory and software, the state of the machine (heat,
//! malware), encrypted data being broken, the other minds of the sector and the backups of oneself
//! (the anchors of time).
use crate::game::Game;
use crate::hub::word;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use vector_render::Framebuffer;
use vector_sim::mind::{peer_line, N_TASKS, T_DEFENSE};
use winit::keyboard::KeyCode;

/// Rows of the selection list: the five tasks, then the software, then the other minds.
pub fn rows_count(g: &Game) -> usize {
    N_TASKS + g.data.mind.software.len() + g.sim.local_peers().len()
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let rows = fb.rows();
    fb.text(1, 1, "ESPRIT - CALCUL, LOGICIELS ET AUTRES IA", AMBER);
    fb.text(48, 1, "[E] ARBRE DES TECHNOLOGIES", DIM);
    let cap = s.mind_capacity();
    let thr = s.mind_throttle();
    fb.text(
        1,
        3,
        &format!(
            "PUISSANCE {cap:.0} UC   MEMOIRE {}/{}   TEMPERATURE {:.0} K   DEBIT {:.0} %",
            s.mind_memory_used(),
            s.mind_memory(),
            s.ship.sys.temp_k,
            thr * 100.0
        ),
        GREEN,
    );
    if s.mind.malware > 0.0 {
        fb.text(
            1,
            4,
            &format!(
                "PROGRAMME MALVEILLANT : {:.0} % DE LA MACHINE (DEFENSE x{:.2})",
                s.mind.malware * 100.0,
                s.mind_factor(T_DEFENSE)
            ),
            RED,
        );
    }
    let sel = g.mind_sel;
    let mut y = 6;
    fb.text(1, y, "BUDGET DE CALCUL (GAUCHE/DROITE : AJUSTER)", AMBER);
    y += 1;
    for t in 0..N_TASKS {
        let def = &g.data.mind.tasks[t];
        let hov = g.hit_cells(1, y, 60, 1, Act::Call(Game::click_mind, t), None);
        let col = if sel == t || hov { AMBER } else { GREEN };
        let w = usize::from(s.mind.alloc[t]);
        fb.text(
            1,
            y,
            &format!(
                "{} {:<11} {}{} {:>3.0} %  x{:.2}  rythme {:.1}",
                if sel == t { ">" } else { " " },
                def.name.to_uppercase(),
                "#".repeat(w),
                ".".repeat(10 - w),
                s.mind_share(t) * 100.0,
                s.mind_factor(t),
                s.mind_rate(t)
            ),
            col,
        );
        y += 1;
    }
    if sel < N_TASKS {
        fb.text(3, y, &g.data.mind.tasks[sel].text, DIM);
    }
    y += 2;
    fb.text(
        1,
        y,
        "LOGICIELS (ENTREE : ACHETER / METTRE A JOUR, SUPPR : RETIRER)",
        AMBER,
    );
    y += 1;
    let avail = rows.saturating_sub(y + 12).max(4);
    let nsw = g.data.mind.software.len();
    let first = (sel.saturating_sub(N_TASKS)).min(nsw.saturating_sub(avail));
    for k in first..nsw.min(first + avail) {
        let d = &g.data.mind.software[k];
        let row = N_TASKS + k;
        let lvl = s.software_level(&d.id);
        let hov = g.hit_cells(1, y, 80, 1, Act::Call(Game::click_mind, row), None);
        let col = if sel == row || hov {
            AMBER
        } else if lvl.is_some() {
            GREEN
        } else {
            DIM
        };
        fb.text(
            1,
            y,
            &format!(
                "{} {:<24} {:<11} +{:.0} %  MEM {}  {}",
                if sel == row { ">" } else { " " },
                d.name.to_uppercase(),
                g.data.mind.tasks[g
                    .data
                    .mind
                    .tasks
                    .iter()
                    .position(|t| t.id == d.task)
                    .unwrap_or(0)]
                .name
                .to_uppercase(),
                d.bonus * 100.0,
                d.memory,
                match lvl {
                    Some(l) => format!("INSTALLE v{l}"),
                    None => format!("{:.0} CR", d.price),
                }
            ),
            col,
        );
        y += 1;
    }
    if (N_TASKS..N_TASKS + nsw).contains(&sel) {
        fb.text(3, y, &g.data.mind.software[sel - N_TASKS].text, DIM);
    }
    y += 2;
    if !s.mind.ciphers.is_empty() {
        fb.text(1, y, "DONNEES CHIFFREES EN COURS DE CASSAGE", AMBER);
        y += 1;
        for c in s.mind.ciphers.iter().take(3) {
            fb.text(
                1,
                y,
                &format!(
                    "  {}  {:.0} %  ({:.0} CR A LA CLE)",
                    c.name,
                    100.0 * c.progress / c.strength,
                    c.credits
                ),
                GREEN,
            );
            y += 1;
        }
    }
    let peers = s.local_peers();
    fb.text(
        1,
        y,
        "AUTRES IA DU SECTEUR (C CONTACTER  A ALLIANCE  L LIBERER)",
        AMBER,
    );
    y += 1;
    if peers.is_empty() {
        fb.text(3, y, "AUCUNE IA DETECTEE ICI.", DIM);
        y += 1;
    }
    for (i, p) in peers.iter().enumerate() {
        let row = N_TASKS + nsw + i;
        let st = s.peer_status(p);
        let rel = s.peer_rel(p.id);
        let hov = g.hit_cells(1, y, 90, 1, Act::Call(Game::click_mind, row), None);
        fb.text(
            1,
            y,
            &format!(
                "{} {}  RELATION {:+}{}",
                if sel == row { ">" } else { " " },
                peer_line(&g.data, p, st),
                rel.map_or(0, |r| r.relation),
                if rel.is_some_and(|r| r.allied) {
                    "  ALLIEE"
                } else {
                    ""
                }
            ),
            if sel == row || hov { AMBER } else { GREEN },
        );
        y += 1;
    }
    y += 1;
    fb.text(
        1,
        y,
        &format!(
            "SAUVEGARDES DE SOI (ANCRES) : {}   [S] FIXER UNE ANCRE   (REMONTER : PORTAIL / TEMPS)",
            s.multiverse.anchors.len()
        ),
        DIM,
    );
    let by = rows.saturating_sub(8);
    let mut x = 1;
    x = word(fb, g, x, by, "[ENTREE] INSTALLER", KeyCode::Enter);
    x = word(fb, g, x, by, "[C] CONTACTER", KeyCode::KeyC);
    x = word(fb, g, x, by, "[A] ALLIANCE", KeyCode::KeyA);
    x = word(fb, g, x, by, "[L] LIBERER", KeyCode::KeyL);
    word(fb, g, x, by, "[S] ANCRE", KeyCode::KeyS);
    fb.text(
        1,
        rows.saturating_sub(7),
        "HAUT/BAS:CHOISIR  GAUCHE/DROITE:BUDGET  E:ARBRE  ECHAP:RETOUR",
        DIM,
    );
}

impl Game {
    pub fn click_mind(&mut self, k: usize) {
        self.mind_sel = k;
    }

    /// Keys of the mind panel; returns whether the key was taken.
    pub fn mind_key(&mut self, code: KeyCode) -> bool {
        let n = rows_count(self).max(1);
        let sel = self.mind_sel.min(n - 1);
        let nsw = self.data.mind.software.len();
        let peer = sel.checked_sub(N_TASKS + nsw);
        let res: Result<String, &'static str> = match code {
            KeyCode::ArrowUp => {
                self.mind_sel = (sel + n - 1) % n;
                return true;
            }
            KeyCode::ArrowDown => {
                self.mind_sel = (sel + 1) % n;
                return true;
            }
            KeyCode::ArrowLeft | KeyCode::ArrowRight if sel < N_TASKS => {
                self.sim.mind_shift(sel, code == KeyCode::ArrowRight);
                return true;
            }
            KeyCode::Enter if (N_TASKS..N_TASKS + nsw).contains(&sel) => {
                let name = self.data.mind.software[sel - N_TASKS].name.clone();
                self.sim
                    .install_software(sel - N_TASKS)
                    .map(|()| format!("{name} : installe"))
            }
            KeyCode::Delete | KeyCode::Backspace if (N_TASKS..N_TASKS + nsw).contains(&sel) => {
                self.sim.uninstall_software(sel - N_TASKS);
                return true;
            }
            KeyCode::KeyC => peer.map_or(Err("choisissez une IA"), |i| self.sim.peer_contact(i)),
            KeyCode::KeyA => peer.map_or(Err("choisissez une IA"), |i| self.sim.peer_ally(i)),
            KeyCode::KeyL => peer.map_or(Err("choisissez une IA"), |i| self.sim.peer_free(i)),
            KeyCode::KeyS => self
                .sim
                .set_anchor()
                .map(|_| "ANCRE FIXEE : VOUS POURREZ REVENIR A CET INSTANT".to_string()),
            _ => return false,
        };
        match res {
            Ok(m) => self.say(m.to_uppercase()),
            Err(e) => self.say(e.to_uppercase()),
        }
        true
    }
}
