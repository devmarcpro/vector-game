//! Roguelike exploration (End key, in flight or docked): choose a place within reach, then walk
//! it turn by turn on a floor drawn with characters. The squad moves and fights together; a click
//! on a seen cell walks there; the panel shows the squad, what is carried and the log.
use crate::game::{Game, Screen};
use crate::hud::{AMBER, DIM, GREEN};
use crate::mouse::Act;
use vector_render::Framebuffer;
use vector_sim::rogue::{Rogue, MAX_PARTY, T_DOWN, T_FEATURE, T_FLOOR, T_UP, T_WALL, VIEW_RADIUS};
use winit::keyboard::KeyCode;

const WHITE: u32 = 0x00FF_FFFF;
const UP: u32 = 0x0040_E070;
const DOWN: u32 = 0x00FF_5050;
const WALL: u32 = 0x0030_5048;
const FAR: u32 = 0x0020_3830;
/// The map is drawn at twice the size of the text: tiles of 16 x 20 pixels from (OX, OY).
const TILE_W: i32 = 16;
const TILE_H: i32 = 20;
const OX: i32 = 8;
const OY: i32 = 30;

/// One tile of the map, in big characters.
fn tile(fb: &mut Framebuffer, x: usize, y: usize, ch: &str, col: u32) {
    fb.text_big(OX + x as i32 * TILE_W, OY + y as i32 * TILE_H, ch, 2, col);
}

/// Glyph and colour of the feature tile of kind `k` of `ROGUE_FEATURES`.
fn feature_glyph(k: usize) -> (&'static str, u32) {
    match k {
        0 => ("&", 0x0060_C0FF), // terminal
        1 => ("+", UP),          // medbay
        2 => ("=", AMBER),       // cache
        3 => ("_", 0x00FF_80FF), // altar
        4 => ("~", 0x0080_FF80), // mutagen
        5 => ("{", 0x0060_A0FF), // fountain
        6 => (".", GREEN),       // trap (hidden)
        7 => ("O", 0x00FF_A040), // portal
        _ => ("%", AMBER),       // vault
    }
}

#[derive(Default)]
pub struct RogueUi {
    pub sel: usize,
}

impl Game {
    pub fn click_rogue_target(&mut self, i: usize) {
        self.rogue_ui.sel = i;
    }

    /// A click on the map: step next to us, or walk there over what is known.
    pub fn rogue_click(&mut self, (px, py): (i32, i32)) {
        let (tx, ty) = ((px - OX).div_euclid(TILE_W), (py - OY).div_euclid(TILE_H));
        let Some(r) = self.sim.rogue.as_ref() else {
            return;
        };
        if r.over {
            return;
        }
        let (dx, dy) = (tx - r.x, ty - r.y);
        let res = if dx.abs().max(dy.abs()) == 1 {
            self.sim.rogue_step(dx, dy).map(|()| 1)
        } else {
            self.sim.rogue_travel(tx, ty, 25)
        };
        if let Err(e) = res {
            self.say(e.to_uppercase());
        }
    }

    pub fn rogue_key(&mut self, code: KeyCode) {
        if self.sim.rogue.is_none() {
            let n = self.sim.rogue_targets().len();
            match code {
                KeyCode::ArrowUp if n > 0 => self.rogue_ui.sel = (self.rogue_ui.sel + n - 1) % n,
                KeyCode::ArrowDown if n > 0 => self.rogue_ui.sel = (self.rogue_ui.sel + 1) % n,
                KeyCode::Enter => {
                    match self
                        .sim
                        .rogue_enter(self.rogue_ui.sel.min(n.saturating_sub(1)), vec![])
                    {
                        Ok(()) => {}
                        Err(e) => self.say(e.to_uppercase()),
                    }
                }
                KeyCode::Escape | KeyCode::End => {
                    self.screen = if self.sim.docked.is_some() {
                        Screen::Station
                    } else {
                        Screen::Flight
                    };
                }
                _ => {}
            }
            return;
        }
        if self.sim.rogue.as_ref().is_some_and(|r| r.over) {
            if matches!(code, KeyCode::Enter | KeyCode::Escape | KeyCode::Space) {
                self.sim.rogue_close();
                if let Some(back) = self.rogue_return.take() {
                    self.screen = back;
                }
            }
            return;
        }
        let step = |dx: i32, dy: i32| Some((dx, dy));
        let dir = match code {
            KeyCode::ArrowLeft | KeyCode::Numpad4 => step(-1, 0),
            KeyCode::ArrowRight | KeyCode::Numpad6 => step(1, 0),
            KeyCode::ArrowUp | KeyCode::Numpad8 => step(0, -1),
            KeyCode::ArrowDown | KeyCode::Numpad2 => step(0, 1),
            KeyCode::Home | KeyCode::Numpad7 => step(-1, -1),
            KeyCode::PageUp | KeyCode::Numpad9 => step(1, -1),
            KeyCode::End | KeyCode::Numpad1 => step(-1, 1),
            KeyCode::PageDown | KeyCode::Numpad3 => step(1, 1),
            KeyCode::Space | KeyCode::Numpad5 | KeyCode::Period => step(0, 0),
            _ => None,
        };
        if let Some((dx, dy)) = dir {
            if let Err(e) = self.sim.rogue_step(dx, dy) {
                self.say(e.to_uppercase());
            }
            return;
        }
        match code {
            KeyCode::Enter => match self.sim.rogue_stairs() {
                Ok(m) => self.say(m.to_uppercase()),
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::Tab => self.sim.rogue_switch(),
            _ => {}
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
    match g.sim.rogue.as_ref() {
        None => draw_choose(fb, g),
        Some(r) => draw_run(fb, g, r),
    }
}

fn draw_choose(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    fb.text(1, 1, "== EXPLORER A PIED ==", AMBER);
    fb.text(
        1,
        3,
        "TOUT SE VISITE : STATIONS, EPAVES, RUINES, GROTTES, RUCHES, PORTAILS... UNE EQUIPE Y VA, TOUR PAR TOUR.",
        DIM,
    );
    let targets = s.rogue_targets();
    if targets.is_empty() {
        fb.text(
            3,
            6,
            "RIEN A EXPLORER A PORTEE : APPROCHEZ-VOUS D'UNE STATION, D'UNE ANOMALIE, D'UNE EPAVE, D'UN ASTEROIDE OU D'UNE PLANETE.",
            DIM,
        );
    }
    for (i, t) in targets.iter().enumerate() {
        let sel = i == g.rogue_ui.sel.min(targets.len() - 1);
        let hov = g.hit_cells(
            2,
            6 + i,
            70,
            1,
            Act::Call(Game::click_rogue_target, i),
            None,
        );
        let th = &s.data.rogue_themes[t.theme];
        fb.text(
            2,
            6 + i,
            &format!(
                "{}{:<44.44} {} ETAGES",
                if sel { ">" } else { " " },
                t.label.to_uppercase(),
                th.floors
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
    if let Some(t) = targets.get(g.rogue_ui.sel.min(targets.len().saturating_sub(1))) {
        let th = &s.data.rogue_themes[t.theme];
        fb.text(2, 28, &th.name.to_uppercase(), AMBER);
        fb.text(2, 29, &th.text, DIM);
        if th.hazard > 0.0 {
            fb.text(
                2,
                30,
                &format!("DANGER : {}", th.hazard_name.to_uppercase()),
                DOWN,
            );
        }
    }
    let squad = s.rogue_squad();
    fb.text(
        2,
        33,
        &format!("EQUIPE ({} SUR {MAX_PARTY} AU PLUS) :", squad.len()),
        AMBER,
    );
    for (k, &i) in squad.iter().enumerate() {
        let p = &s.crew.members[i];
        fb.text(
            4,
            34 + k,
            &format!(
                "{:<24.24} SANTE {:>3.0} %",
                p.name.to_uppercase(),
                p.health * 100.0
            ),
            GREEN,
        );
    }
    if !targets.is_empty() {
        word(fb, g, 2, 40, "[ENTREE] PARTIR", KeyCode::Enter);
    }
    for (i, m) in g.messages.iter().rev().take(1).enumerate() {
        fb.text(1, 42 + i, m, AMBER);
    }
}

fn draw_run(fb: &mut Framebuffer, g: &Game, r: &Rogue) {
    let s = &g.sim;
    let th = &s.data.rogue_themes[r.theme];
    fb.text(
        1,
        1,
        &format!(
            "{}  -  ETAGE {}/{}",
            th.name.to_uppercase(),
            r.cur + 1,
            th.floors
        ),
        AMBER,
    );
    let vis = r.visible();
    let f = r.floor();
    for y in 0..r.h {
        for x in 0..r.w {
            let i = y * r.w + x;
            if !f.seen[i] {
                continue;
            }
            let lit = vis[i];
            let t = f.tiles[i];
            let (ch, col) = match t {
                T_WALL => ("#", if lit { WALL } else { FAR }),
                T_FLOOR => (".", if lit { GREEN } else { FAR }),
                T_UP => ("<", AMBER),
                T_DOWN => (">", AMBER),
                t if t >= T_FEATURE => {
                    let (c, col) = feature_glyph((t - T_FEATURE) as usize);
                    (c, if lit { col } else { FAR })
                }
                _ => (" ", DIM),
            };
            tile(fb, x, y, ch, col);
        }
    }
    for it in &f.items {
        if f.seen[it.y as usize * r.w + it.x as usize] {
            tile(fb, it.x as usize, it.y as usize, "*", AMBER);
        }
    }
    for m in &f.monsters {
        if vis[m.y as usize * r.w + m.x as usize] {
            let def = &s.data.rogue_monsters[m.kind];
            tile(
                fb,
                m.x as usize,
                m.y as usize,
                &def.glyph,
                if m.boss {
                    0x00FF_40FF
                } else if m.awake {
                    DOWN
                } else {
                    0x00C0_8040
                },
            );
        }
    }
    tile(fb, r.x as usize, r.y as usize, "@", WHITE);
    if !r.over {
        g.hit(
            (OX, OY, OX + r.w as i32 * TILE_W, OY + r.h as i32 * TILE_H),
            Act::At(Game::rogue_click),
            None,
        );
    }
    // Side panel.
    let sx = ((OX + r.w as i32 * TILE_W) / 8 + 2) as usize;
    fb.text(sx, 3, "EQUIPE", AMBER);
    for (k, &i) in r.party.iter().enumerate() {
        let p = &s.crew.members[i];
        let up = p.alive && p.health > 0.0;
        fb.text(
            sx,
            4 + k,
            &format!(
                "{}{:<14.14} {:>3.0}%",
                if k == r.active { ">" } else { " " },
                p.name.to_uppercase(),
                p.health * 100.0
            ),
            if !up {
                DOWN
            } else if p.health < 0.4 {
                0x00FF_A040
            } else {
                GREEN
            },
        );
    }
    fb.text(sx, 9, &format!("TOUR {}   TUES {}", r.turn, r.kills), DIM);
    if r.buff > 0 {
        fb.text(sx, 10, &format!("BENEDICTION {} TOURS", r.buff), UP);
    }
    fb.text(sx, 12, "PORTE", AMBER);
    for (k, (res, kg)) in r.loot.iter().take(8).enumerate() {
        fb.text(
            sx,
            13 + k,
            &format!(
                "{:<14.14} {:>6.1} KG",
                s.data.resources[*res].name.to_uppercase(),
                kg
            ),
            GREEN,
        );
    }
    if r.loot.is_empty() {
        fb.text(sx, 13, "RIEN", DIM);
    }
    fb.text(sx, 23, "COMMANDES", AMBER);
    let mut y = 24;
    for (label, key) in [
        ("[ENTREE] ESCALIER", KeyCode::Enter),
        ("[ESP] ATTENDRE", KeyCode::Space),
        ("[TAB] CHEF", KeyCode::Tab),
    ] {
        word(fb, g, sx, y, label, key);
        y += 1;
    }
    fb.text(sx, y + 1, "FLECHES + PAV. NUM. : DEPLACER", DIM);
    fb.text(sx, y + 2, "CLIC : MARCHER JUSQU'A LA", DIM);
    fb.text(sx, y + 3, "CASE ; FONCER = ATTAQUER", DIM);
    fb.text(sx, y + 4, &format!("VUE : {VIEW_RADIUS} CASES"), DIM);
    // Log.
    let log_row = ((OY + r.h as i32 * TILE_H) / 10 + 1) as usize;
    for (k, line) in r.log.iter().rev().take(5).enumerate() {
        fb.text(1, log_row + k, line, if k == 0 { AMBER } else { DIM });
    }
    if r.over {
        fb.text(sx, 21, "[ENTREE] FERMER", AMBER);
        g.hit_cells(sx, 21, 15, 1, Act::Key(KeyCode::Enter), None);
    }
}
