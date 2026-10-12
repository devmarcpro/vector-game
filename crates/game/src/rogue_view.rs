//! Exploration on foot (End key, in flight or docked; E on a planet's surface map): choose a place
//! within reach, or land on a world, then explore it turn by turn on a floor drawn with
//! characters. The squad moves and fights together; a click on a seen cell walks there; the panel
//! shows the squad, what is carried and the log. One screen for every place: a planet's ground,
//! a station, a wreck, a ruin, a portal, a dimension.
use crate::game::{Game, Screen};
use crate::hub::word;
use crate::hud::{AMBER, DIM, DOWN, GREEN, UP, WHITE};
use crate::mouse::Act;
use vector_data::ROGUE_TERMINALS;
use vector_render::Framebuffer;
use vector_sim::rogue::{
    Rogue, TargetKind, MAX_PARTY, T_DOWN, T_FEATURE, T_FLOOR, T_SITE, T_TERMINAL, T_UP, T_WALL,
    T_WATER, VIEW_RADIUS,
};
use winit::keyboard::KeyCode;

const WALL: u32 = 0x0030_5048;
const FAR: u32 = 0x0020_3830;
const WATER: u32 = 0x0040_80E0;
/// The map is drawn at twice the size of the text: tiles of 16 x 20 pixels from (OX, OY), in a
/// window of VIEW_COLS x VIEW_ROWS tiles that follows the squad on the big maps of the surfaces.
const TILE_W: i32 = 16;
const TILE_H: i32 = 20;
const OX: i32 = 8;
const OY: i32 = 30;
pub const VIEW_COLS: usize = 56;
pub const VIEW_ROWS: usize = 30;

/// The pixel at the middle of a cell of the map.
pub fn cell_px(r: &Rogue, x: i32, y: i32) -> (i32, i32) {
    let (cx, cy) = camera(r);
    (
        OX + (x - cx) * TILE_W + TILE_W / 2,
        OY + (y - cy) * TILE_H + TILE_H / 2,
    )
}

/// A cell as one number a zone can carry.
pub fn pack_cell(x: i32, y: i32) -> usize {
    ((y + 1000) as usize) * 100_000 + (x + 1000) as usize
}

pub fn unpack_cell(code: usize) -> (i32, i32) {
    (
        (code % 100_000) as i32 - 1000,
        (code / 100_000) as i32 - 1000,
    )
}

/// Top-left tile of the window on the map, keeping the squad in view.
pub fn camera(r: &Rogue) -> (i32, i32) {
    let fit = |pos: i32, size: usize, view: usize| -> i32 {
        if size <= view {
            0
        } else {
            (pos - view as i32 / 2).clamp(0, (size - view) as i32)
        }
    };
    (fit(r.x, r.w, VIEW_COLS), fit(r.y, r.h, VIEW_ROWS))
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
    /// Where the squad last turned to (map view): what X digs and B / N build.
    pub face: (i32, i32),
}

impl Game {
    pub fn click_rogue_target(&mut self, i: usize) {
        self.rogue_ui.sel = i;
    }

    /// A click on the map: step next to us, or walk there over what is known.
    /// The cell of the map under a pixel, if the walk is on.
    pub fn rogue_cell_at(&self, px: i32, py: i32) -> Option<(i32, i32)> {
        let r = self.sim.rogue.as_ref()?;
        if r.over {
            return None;
        }
        let (cx, cy) = camera(r);
        Some((
            (px - OX).div_euclid(TILE_W) + cx,
            (py - OY).div_euclid(TILE_H) + cy,
        ))
    }

    pub fn rogue_click(&mut self, (px, py): (i32, i32)) {
        if let Some((tx, ty)) = self.rogue_cell_at(px, py) {
            self.rogue_goto(tx, ty);
        }
    }

    /// Walk to a cell (a step when it is next to the squad, which attacks or talks to what stands
    /// there), or travel there.
    pub fn rogue_goto(&mut self, tx: i32, ty: i32) {
        let Some(r) = self.sim.rogue.as_ref() else {
            return;
        };
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

    pub fn rogue_walk_cell(&mut self, code: usize) {
        let (x, y) = unpack_cell(code);
        self.rogue_goto(x, y);
    }

    /// Turn the squad towards an adjacent cell (what X digs and B / N build).
    pub fn rogue_face_cell(&mut self, code: usize) {
        let (x, y) = unpack_cell(code);
        if let Some(r) = self.sim.rogue.as_ref() {
            self.rogue_ui.face = ((x - r.x).signum(), (y - r.y).signum());
        }
    }

    /// Say what is on a cell.
    pub fn rogue_inspect_cell(&mut self, code: usize) {
        let (x, y) = unpack_cell(code);
        let Some(r) = self.sim.rogue.as_ref() else {
            return;
        };
        if !r.inside(x, y) {
            return;
        }
        let i = r.idx(x, y);
        let f = r.floor();
        let text = if let Some(m) = f
            .monsters
            .iter()
            .find(|m| (m.x, m.y) == (x, y) && r.visible()[i])
        {
            format!(
                "{} : SANTE {:.0}, ATTAQUE {:.0}, DEFENSE {:.0}{}",
                m.name.to_uppercase(),
                m.hp,
                m.attack,
                m.defense,
                if m.friendly {
                    " (PACIFIQUE)"
                } else if m.hunts {
                    " (HOSTILE)"
                } else {
                    ""
                }
            )
        } else if !f.seen[i] {
            "CASE INCONNUE".to_string()
        } else {
            let mat = r
                .mat(x, y)
                .and_then(|m| self.data.materials.get(m))
                .map_or("?".to_string(), |m| m.name.to_uppercase());
            format!(
                "CASE {x},{y} : {mat}{}",
                if r.owned(x, y) {
                    " (BATIE PAR VOUS)"
                } else {
                    ""
                }
            )
        };
        self.say(text);
    }

    pub fn rogue_key(&mut self, code: KeyCode) {
        if self.sim.rogue.is_none() {
            let n = self.sim.rogue_targets().len();
            match code {
                KeyCode::ArrowUp if n > 0 => self.rogue_ui.sel = (self.rogue_ui.sel + n - 1) % n,
                KeyCode::ArrowDown if n > 0 => self.rogue_ui.sel = (self.rogue_ui.sel + 1) % n,
                KeyCode::Enter => {
                    if let Err(e) = self
                        .sim
                        .rogue_enter(self.rogue_ui.sel.min(n.saturating_sub(1)), vec![])
                    {
                        self.say(e.to_uppercase());
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
                if self.rogue_from_surface {
                    self.rogue_from_surface = false;
                    self.screen = Screen::Surface;
                }
            }
            return;
        }
        // Zoom out: from the grid of the ground back up to the map of the region.
        if matches!(code, KeyCode::Minus | KeyCode::NumpadSubtract)
            && self.rogue_from_surface
            && self
                .sim
                .rogue
                .as_ref()
                .is_some_and(|r| r.on_surface() && r.parent.is_none())
        {
            match self.sim.rogue_recall() {
                Ok(m) => {
                    self.say(m.to_uppercase());
                    self.sim.rogue_close();
                    self.rogue_from_surface = false;
                    self.screen = Screen::Surface;
                }
                Err(e) => self.say(e.to_uppercase()),
            }
            return;
        }
        if code == KeyCode::KeyF {
            self.fpv = !self.fpv;
            if self.fpv {
                if let Some(r) = self.sim.rogue.as_ref() {
                    self.fpv_dir = crate::fpv_view::open_direction(r);
                }
            }
            return;
        }
        let step = |dx: i32, dy: i32| Some((dx, dy));
        let [up, down, right, left, _, _] = self.settings.move_keys();
        if self.fpv {
            // First person: left and right turn, up and down step along the facing.
            let (fx, fy) = crate::fpv_view::step_of(self.fpv_dir);
            let fp = match code {
                c if c == KeyCode::ArrowLeft || c == KeyCode::Numpad4 || c == left => {
                    self.fpv_dir = (self.fpv_dir + 7) % 8;
                    return;
                }
                c if c == KeyCode::ArrowRight || c == KeyCode::Numpad6 || c == right => {
                    self.fpv_dir = (self.fpv_dir + 1) % 8;
                    return;
                }
                c if c == KeyCode::ArrowUp || c == KeyCode::Numpad8 || c == up => step(fx, fy),
                c if c == KeyCode::ArrowDown || c == KeyCode::Numpad2 || c == down => {
                    step(-fx, -fy)
                }
                KeyCode::Space | KeyCode::Numpad5 | KeyCode::Period => step(0, 0),
                _ => None,
            };
            if let Some((dx, dy)) = fp {
                if (dx, dy) != (0, 0) {
                    self.rogue_ui.face = (fx, fy);
                }
                if let Err(e) = self.sim.rogue_step(dx, dy) {
                    self.say(e.to_uppercase());
                }
                return;
            }
        }
        let dir = match code {
            c if c == KeyCode::ArrowLeft || c == KeyCode::Numpad4 || c == left => step(-1, 0),
            c if c == KeyCode::ArrowRight || c == KeyCode::Numpad6 || c == right => step(1, 0),
            c if c == KeyCode::ArrowUp || c == KeyCode::Numpad8 || c == up => step(0, -1),
            c if c == KeyCode::ArrowDown || c == KeyCode::Numpad2 || c == down => step(0, 1),
            KeyCode::Home | KeyCode::Numpad7 => step(-1, -1),
            KeyCode::PageUp | KeyCode::Numpad9 => step(1, -1),
            KeyCode::End | KeyCode::Numpad1 => step(-1, 1),
            KeyCode::PageDown | KeyCode::Numpad3 => step(1, 1),
            KeyCode::Space | KeyCode::Numpad5 | KeyCode::Period => step(0, 0),
            _ => None,
        };
        if let Some((dx, dy)) = dir {
            if (dx, dy) != (0, 0) {
                self.rogue_ui.face = (dx, dy);
            }
            if let Err(e) = self.sim.rogue_step(dx, dy) {
                self.say(e.to_uppercase());
            }
            return;
        }
        let res = match code {
            KeyCode::Enter => {
                let res = self.sim.rogue_stairs();
                // A terminal under the squad: open its screen.
                if let Some(i) = self.sim.rogue.as_mut().and_then(|r| r.terminal.take()) {
                    self.open_terminal(i);
                    return;
                }
                res
            }
            KeyCode::KeyR => self.sim.rogue_recall(),
            KeyCode::KeyT => self.sim.rogue_fire(),
            KeyCode::KeyE => self.sim.rogue_reload(),
            KeyCode::KeyG => self.sim.rogue_grenade(),
            KeyCode::KeyC => self.sim.rogue_capture(),
            KeyCode::KeyV => self
                .sim
                .rogue_deploy_drone()
                .map(|()| "DRONE LANCE".to_string()),
            KeyCode::KeyX | KeyCode::KeyB | KeyCode::KeyN => {
                let (dx, dy) = if self.fpv {
                    crate::fpv_view::step_of(self.fpv_dir)
                } else if self.rogue_ui.face == (0, 0) {
                    (0, 1)
                } else {
                    self.rogue_ui.face
                };
                match code {
                    KeyCode::KeyX => self.sim.rogue_dig(dx, dy),
                    KeyCode::KeyB => self.sim.rogue_build(dx, dy, true),
                    _ => self.sim.rogue_build(dx, dy, false),
                }
            }
            KeyCode::Tab => {
                self.sim.rogue_switch();
                return;
            }
            _ => return,
        };
        match res {
            Ok(m) => self.say(m.to_uppercase()),
            Err(e) => self.say(e.to_uppercase()),
        }
    }
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
        "TOUT SE VISITE : MONDES, STATIONS, EPAVES, RUINES, GROTTES, RUCHES, PORTAILS... UNE EQUIPE Y VA, TOUR PAR TOUR.",
        DIM,
    );
    let targets = s.rogue_targets();
    if targets.is_empty() {
        fb.text(
            3,
            6,
            "RIEN A EXPLORER A PORTEE : APPROCHEZ-VOUS D'UNE PLANETE, D'UNE STATION, D'UNE ANOMALIE, D'UNE EPAVE OU D'UN ASTEROIDE.",
            DIM,
        );
    }
    let sel_i = g.rogue_ui.sel.min(targets.len().saturating_sub(1));
    for (i, t) in targets.iter().enumerate() {
        let sel = i == sel_i;
        let hov = g.hit_cells(
            2,
            6 + i,
            70,
            1,
            Act::Call(Game::click_rogue_target, i),
            None,
        );
        let th = &s.data.rogue_themes[t.theme];
        let what = match t.kind {
            TargetKind::Landing(_) => "SURFACE".to_string(),
            TargetKind::Ship => "SALLES ET EQUIPAGE".to_string(),
            TargetKind::Place if !th.social.is_empty() => "SERVICES ET PASSANTS".to_string(),
            TargetKind::Place => format!("{} ETAGES", th.floors),
        };
        fb.text(
            2,
            6 + i,
            &format!(
                "{}{:<44.44} {what}",
                if sel { ">" } else { " " },
                t.label.to_uppercase(),
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
    if let Some(t) = targets.get(sel_i) {
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
        if matches!(t.kind, TargetKind::Landing(_)) {
            fb.text(
                2,
                30,
                "POUR CHOISIR LA CASE ET LE ROVER : CARTE DE SURFACE (F8), PUIS E.",
                DIM,
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

/// A colour as it looks in the dark (remembered, out of sight).
/// A material's colour pulled towards amber: the mark of what the squad built.
fn amber_mix(c: u32) -> u32 {
    let ch = |s: u32| ((((c >> s) & 0xFF) + 2 * ((AMBER >> s) & 0xFF)) / 3) << s;
    ch(16) | ch(8) | ch(0)
}

fn dim(c: u32) -> u32 {
    let ch = |s: u32| (((c >> s) & 0xFF) * 38 / 100) << s;
    ch(16) | ch(8) | ch(0)
}

/// The map in characters: tiles, items, creatures, the squad, and the zone to click.
fn draw_map(fb: &mut Framebuffer, g: &Game, r: &Rogue) {
    let s = &g.sim;
    let vis = r.visible();
    let f = r.floor();
    let (cx, cy) = camera(r);
    let on_screen = |x: i32, y: i32| {
        (cx..cx + VIEW_COLS as i32).contains(&x) && (cy..cy + VIEW_ROWS as i32).contains(&y)
    };
    let put = |fb: &mut Framebuffer, x: i32, y: i32, ch: &str, col: u32| {
        fb.text_big(OX + (x - cx) * TILE_W, OY + (y - cy) * TILE_H, ch, 2, col);
    };
    for y in cy..(cy + VIEW_ROWS as i32).min(r.h as i32) {
        for x in cx..(cx + VIEW_COLS as i32).min(r.w as i32) {
            let i = r.idx(x, y);
            if !f.seen[i] {
                continue;
            }
            let lit = vis[i];
            let t = f.tiles[i];
            let glyph: String;
            let (ch, col) = match t {
                T_WALL | T_FLOOR | T_WATER => {
                    match r.mat(x, y).and_then(|m| s.data.materials.get(m)) {
                        // The character and the colour of the cell come from what it is made of.
                        Some(m) => {
                            let chars: Vec<char> = m.glyphs.chars().collect();
                            let n =
                                vector_core::hash_coord(7, i64::from(x), i64::from(y), 3) as usize;
                            glyph = chars[n % chars.len()].to_string();
                            let col = if r.owned(x, y) {
                                // What the squad built shows in the colour of ownership.
                                amber_mix(m.color)
                            } else {
                                m.color
                            };
                            (glyph.as_str(), if lit { col } else { dim(col) })
                        }
                        None if t == T_WALL => ("#", if lit { WALL } else { FAR }),
                        None if t == T_WATER => ("~", if lit { WATER } else { FAR }),
                        None => (".", if lit { GREEN } else { FAR }),
                    }
                }
                T_UP => ("<", AMBER),
                T_DOWN => (">", AMBER),
                T_SITE => match f.sites.iter().find(|st| (st.x, st.y) == (x, y)) {
                    Some(st) => {
                        let def = &s.data.sites[st.def];
                        (def.glyph.as_str(), if st.used { FAR } else { def.color })
                    }
                    None => ("?", AMBER),
                },
                t if t >= T_TERMINAL => {
                    let k = (t - T_TERMINAL) as usize;
                    match ROGUE_TERMINALS.get(k) {
                        Some(def) => {
                            glyph = def.2.to_string();
                            (glyph.as_str(), if lit { AMBER } else { FAR })
                        }
                        None => ("?", AMBER),
                    }
                }
                t if t >= T_FEATURE => {
                    let (c, col) = feature_glyph((t - T_FEATURE) as usize);
                    (c, if lit { col } else { FAR })
                }
                _ => (" ", DIM),
            };
            put(fb, x, y, ch, col);
        }
    }
    for it in &f.items {
        if on_screen(it.x, it.y) && f.seen[r.idx(it.x, it.y)] {
            put(
                fb,
                it.x,
                it.y,
                if it.plant { "\"" } else { "*" },
                if it.plant { UP } else { AMBER },
            );
        }
    }
    for m in &f.monsters {
        if on_screen(m.x, m.y) && vis[r.idx(m.x, m.y)] {
            put(
                fb,
                m.x,
                m.y,
                &m.glyph.to_string(),
                if m.boss {
                    0x00FF_40FF
                } else if m.friendly {
                    0x0060_E0E0
                } else if !m.hunts {
                    0x0090_D090
                } else if m.awake {
                    DOWN
                } else {
                    0x00C0_8040
                },
            );
        }
    }
    put(fb, r.x, r.y, "@", WHITE);
    if !r.over {
        g.hit(
            (
                OX,
                OY,
                OX + r.w.min(VIEW_COLS) as i32 * TILE_W,
                OY + r.h.min(VIEW_ROWS) as i32 * TILE_H,
            ),
            Act::At(Game::rogue_click),
            None,
        );
    }
}

fn draw_run(fb: &mut Framebuffer, g: &Game, r: &Rogue) {
    let s = &g.sim;
    let th = &s.data.rogue_themes[r.theme];
    let title = if r.on_surface() {
        format!("SURFACE - {}", r.place.to_uppercase())
    } else if r.parent.is_some() {
        format!(
            "{} (SOUS LA SURFACE)  -  ETAGE {}/{}",
            th.name.to_uppercase(),
            r.cur + 1,
            th.floors
        )
    } else {
        format!(
            "{}  -  ETAGE {}/{}",
            th.name.to_uppercase(),
            r.cur + 1,
            th.floors
        )
    };
    fb.text(1, 1, &title, AMBER);
    if g.fpv {
        if r.on_surface() {
            crate::fpv_view::draw_surface(fb, g, r, g.fpv_dir);
        } else {
            crate::fpv_view::draw(fb, &s.data.materials, r, g.fpv_dir);
        }
    } else {
        draw_map(fb, g, r);
    }
    // Side panel.
    let sx = if g.fpv {
        ((crate::fpv_view::X0 + crate::fpv_view::W) / 8 + 2) as usize
    } else {
        ((OX + r.w.min(VIEW_COLS) as i32 * TILE_W) / 8 + 2) as usize
    };
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
    if r.social {
        fb.text(sx, 9, &format!("TOUR {}", r.turn), DIM);
    } else {
        fb.text(sx, 9, &format!("TOUR {}   TUES {}", r.turn, r.kills), DIM);
    }
    if r.buff > 0 {
        fb.text(sx, 10, &format!("BENEDICTION {} TOURS", r.buff), UP);
    }
    if r.drone_turns > 0 {
        fb.text(sx, 11, &format!("DRONE {} TOURS", r.drone_turns), UP);
    }
    if let Some((p, t)) = r.env {
        if r.on_surface() {
            fb.text(sx, 12, &format!("AIR {p:.2} BAR  {t:.0} K"), DIM);
        }
    }
    if let Some((rounds, cap)) = s.rogue_ammo_status(r) {
        fb.text(
            sx,
            13,
            &format!("CHARGEURS {rounds}/{cap}   RECUL {:.0} %", r.recoil * 100.0),
            if rounds == 0 { 0x00FF_3030 } else { AMBER },
        );
    }
    let up_n = r
        .party
        .iter()
        .filter(|&&i| s.crew.members[i].health > 0.0)
        .count();
    if !r.social {
        fb.text(
            sx,
            14,
            &format!("PORTE {:.0}/{:.0} KG", r.loot_mass(), r.carry_cap(up_n)),
            AMBER,
        );
        for (k, (res, kg)) in r.loot.iter().take(8).enumerate() {
            fb.text(
                sx,
                15 + k,
                &format!(
                    "{:<14.14} {:>6.1} KG",
                    s.data.resources[*res].name.to_uppercase(),
                    kg
                ),
                GREEN,
            );
        }
        if r.loot.is_empty() {
            fb.text(sx, 15, "RIEN", DIM);
        }
    }
    fb.text(sx, 24, "COMMANDES", AMBER);
    let mut y = 25;
    let mut cmds: Vec<(&str, KeyCode)> = vec![
        ("[ENTREE] ESCALIER / SITE", KeyCode::Enter),
        ("[ESP] ATTENDRE", KeyCode::Space),
        ("[TAB] CHEF", KeyCode::Tab),
        ("[V] LANCER UN DRONE", KeyCode::KeyV),
        ("[F] VUE A LA PREMIERE PERSONNE", KeyCode::KeyF),
        ("[C] CAPTURER UNE BETE", KeyCode::KeyC),
        ("[X] CREUSER / CASSER DEVANT", KeyCode::KeyX),
        ("[B] BATIR UN MUR  [N] UN SOL", KeyCode::KeyB),
    ];
    if s.rogue_ammo_status(r).is_some() {
        cmds.push(("[T] TIRER  [E] RECHARGER", KeyCode::KeyT));
    }
    if s.data
        .resource_index(vector_sim::rogue::GRENADE_RESOURCE)
        .is_some_and(|i| s.ship.cargo.kg[i] >= 1.0)
    {
        cmds.push(("[G] LANCER UNE GRENADE", KeyCode::KeyG));
    }
    if r.on_surface() && r.parent.is_none() {
        cmds.push(("[R] RAPPELER L'EQUIPE", KeyCode::KeyR));
        if g.rogue_from_surface {
            cmds.push(("[-] DEZOOMER : CARTE DE LA REGION", KeyCode::Minus));
        }
    }
    for (label, key) in cmds {
        word(fb, g, sx, y, label, key);
        y += 1;
    }
    if g.fpv {
        // First person has no map to click on: four words turn and step.
        for (i, (label, key)) in [
            ("[<] TOURNER A GAUCHE", KeyCode::ArrowLeft),
            ("[>] TOURNER A DROITE", KeyCode::ArrowRight),
            ("[^] AVANCER", KeyCode::ArrowUp),
            ("[v] RECULER", KeyCode::ArrowDown),
        ]
        .into_iter()
        .enumerate()
        {
            word(fb, g, sx, y + 1 + i, label, key);
        }
        y += 4;
    } else {
        fb.text(sx, y + 1, "FLECHES / PAVE NUM. / ZQSD", DIM);
    }
    fb.text(sx, y + 2, "CLIC : MARCHER JUSQU'A LA CASE", DIM);
    fb.text(sx, y + 3, "FONCER SUR UN MONSTRE = ATTAQUER", DIM);
    fb.text(sx, y + 4, &format!("VUE : {VIEW_RADIUS} CASES"), DIM);
    if r.built_count() > 0 {
        fb.text(
            sx,
            y + 5,
            &format!("BATI : {} CASES A VOUS", r.built_count()),
            AMBER,
        );
    }
    // The terminals seen on this floor, and the one under the squad.
    if r.social {
        let mut seen: Vec<(usize, bool)> = Vec::new();
        for (i, t) in r.floor().tiles.iter().enumerate() {
            if *t >= T_TERMINAL && r.floor().seen[i] {
                seen.push(((*t - T_TERMINAL) as usize, i == r.idx(r.x, r.y)));
            }
        }
        seen.sort_unstable();
        fb.text(sx, y + 6, "BORNES (ENTREE SUR UNE BORNE)", AMBER);
        for (k, (t, here)) in seen.iter().take(14).enumerate() {
            if let Some(def) = ROGUE_TERMINALS.get(*t) {
                fb.text(
                    sx,
                    y + 7 + k,
                    &format!("{}  {}", def.2, def.1),
                    if *here { WHITE } else { GREEN },
                );
            }
        }
    }
    // Log.
    let log_row = if g.fpv {
        ((crate::fpv_view::Y0 + crate::fpv_view::H) / 10 + 1) as usize
    } else {
        ((OY + r.h.min(VIEW_ROWS) as i32 * TILE_H) / 10 + 1) as usize
    };
    for (k, line) in r.log.iter().rev().take(5).enumerate() {
        fb.text(1, log_row + k, line, if k == 0 { AMBER } else { DIM });
    }
    if r.over {
        fb.text(sx, 21, "[ENTREE] FERMER", AMBER);
        g.hit_cells(sx, 21, 15, 1, Act::Key(KeyCode::Enter), None);
    }
}
