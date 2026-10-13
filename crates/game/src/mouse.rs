//! Mouse. Every screen declares its clickable zones while it draws (`Game::hit`), the last frame's
//! zones are what a click lands on. A click on a zone runs its action (a key press or a function
//! with a number), a double click runs its second action, the wheel scrolls the current list,
//! and a right click opens the context menu of the zone under it (see `context_menu`), or goes
//! back on an empty zone.
use crate::game::{Game, Screen};
use std::cell::RefCell;
use winit::keyboard::KeyCode;

/// What a zone does when it is clicked.
#[derive(Clone, Copy)]
pub enum Act {
    /// The same as pressing this key.
    Key(KeyCode),
    /// Call a function of the game with a number (a row, a node...).
    Call(fn(&mut Game, usize), usize),
    /// Call a function of the game with the pixel position of the click (a map, a chart).
    At(fn(&mut Game, (i32, i32))),
    /// Hold this key down as long as the button is held (thrust, brake, boost, weapons).
    Hold(KeyCode),
}

/// A clickable rectangle in framebuffer pixels (left, top, right, bottom), and what it does.
#[derive(Clone, Copy)]
pub struct Hit {
    pub rect: (i32, i32, i32, i32),
    pub act: Act,
    pub double: Option<Act>,
}

/// Maximum delay between the two clicks of a double click, milliseconds.
const DOUBLE_MS: u64 = 450;

#[derive(Default)]
pub struct Pointer {
    pub pos: (i32, i32),
    /// Zones declared by the last frame, in drawing order (the last one is on top).
    pub hits: RefCell<Vec<Hit>>,
    /// The left button is held down (for dragging the globe).
    pub down: bool,
    /// The key held by a click on a `Hold` zone, released with the button.
    pub holding: Option<KeyCode>,
    last_click: Option<(u64, (i32, i32))>,
    /// Where the pointer rests and for how many frames (the tooltips).
    pub hover: std::cell::Cell<((i32, i32), u32)>,
}

impl Game {
    /// Forget the zones of the previous frame; called before drawing a frame.
    pub fn begin_frame(&self) {
        self.pointer.hits.borrow_mut().clear();
        self.cockpit.organs.borrow_mut().clear();
    }

    /// Declare a clickable zone in pixels; returns whether the pointer is over it right now (so
    /// the drawing can light it up).
    pub fn hit(&self, rect: (i32, i32, i32, i32), act: Act, double: Option<Act>) -> bool {
        let rect = crate::ui_kit::xf_rect(rect);
        self.pointer
            .hits
            .borrow_mut()
            .push(Hit { rect, act, double });
        let (x, y) = self.pointer.pos;
        x >= rect.0 && x < rect.2 && y >= rect.1 && y < rect.3
    }

    /// Declare a zone in text cells (column, row, width, height).
    pub fn hit_cells(
        &self,
        col: usize,
        row: usize,
        w: usize,
        h: usize,
        act: Act,
        double: Option<Act>,
    ) -> bool {
        self.hit(
            (
                (col * 8) as i32 - 2,
                (row * 10) as i32 - 1,
                ((col + w) * 8) as i32 + 2,
                ((row + h) * 10) as i32 - 1,
            ),
            act,
            double,
        )
    }

    pub fn pointer_move(&mut self, x: i32, y: i32) {
        let (px, py) = self.pointer.pos;
        self.pointer.pos = (x, y);
        if self.pointer.down {
            self.pointer_drag(x - px, y - py);
        }
    }

    /// The left button was released: let go of what a held click was pressing.
    pub fn pointer_release(&mut self) {
        self.cockpit.drag.set(false);
        if let Some(k) = self.pointer.holding.take() {
            self.key(k, None, false);
        }
    }

    /// The pointer moved by (dx, dy) pixels with the left button down: it turns the globe.
    pub fn pointer_drag(&mut self, dx: i32, dy: i32) {
        if self.cockpit.drag.get() {
            // The thrust lever of the cockpit follows the pointer.
            self.cockpit_drag(self.pointer.pos.1);
            return;
        }
        if self.ext_active() {
            self.ext_orbit(-f64::from(dx) * 0.008, f64::from(dy) * 0.008);
        }
        if self.screen == Screen::Galaxy {
            self.umap_drag(dx, dy);
        }
        if self.screen == Screen::Designer {
            self.dz_drag(dx, dy);
        }
        if self.screen == Screen::Surface && self.surf_globe {
            self.surf_spin += f64::from(dx) * 0.01;
        }
    }

    pub(crate) fn run_act(&mut self, act: Act) {
        let at = self.pointer.pos;
        match act {
            Act::Key(k) => {
                self.key(k, None, true);
                self.key(k, None, false);
            }
            Act::Call(f, n) => f(self, n),
            Act::At(f) => f(self, at),
            Act::Hold(k) => {
                self.key(k, None, true);
                self.pointer.holding = Some(k);
            }
        }
    }

    /// A left click at (x, y), `ms` milliseconds since the start.
    pub fn pointer_click(&mut self, x: i32, y: i32, ms: u64) {
        // While the mouse steers the ship, clicks are not for the interface.
        if self.mouse_look && self.screen == Screen::Flight {
            return;
        }
        self.pointer.pos = (x, y);
        if self.ctx_click_outside(x, y) {
            return;
        }
        let hit = self
            .pointer
            .hits
            .borrow()
            .iter()
            .rev()
            .find(|h| x >= h.rect.0 && x < h.rect.2 && y >= h.rect.1 && y < h.rect.3)
            .copied();
        let Some(h) = hit else {
            self.pointer.last_click = None;
            return;
        };
        let is_double = self.pointer.last_click.is_some_and(|(t, (px, py))| {
            ms.saturating_sub(t) <= DOUBLE_MS && (px - x).abs() < 8 && (py - y).abs() < 8
        });
        if is_double {
            self.pointer.last_click = None;
            self.run_act(h.double.unwrap_or(h.act));
        } else {
            self.pointer.last_click = Some((ms, (x, y)));
            self.run_act(h.act);
        }
    }

    /// Click on a line of the main or pause menu: select it and validate.
    pub fn click_menu(&mut self, i: usize) {
        self.menu_sel = i;
        self.run_act(Act::Key(KeyCode::Enter));
    }

    // ---- what the zones of each screen do -------------------------------------------

    pub fn click_tech(&mut self, t: usize) {
        self.tech_sel = t;
    }

    pub fn start_tech(&mut self, t: usize) {
        self.tech_sel = t;
        self.tech_start(t);
    }

    pub fn click_bourse_res(&mut self, r: usize) {
        self.bourse_sel = r;
    }

    pub fn click_bourse_range(&mut self, i: usize) {
        self.bourse_range = i;
        self.refresh_bourse();
    }

    pub fn click_surface_layer(&mut self, i: usize) {
        self.surf_layer = i;
    }

    /// A click on a species of the BIOSPHERE panel shows it on the map.
    pub fn click_surface_species(&mut self, i: usize) {
        self.surf_species = i;
    }

    /// A click on the surface map moves the cursor to the cell under it.
    pub fn click_surface_map(&mut self, (px, py): (i32, i32)) {
        use vector_world::surface::{H, W};
        let (x0, y0, cols, rows) = self.surf_geom.get();
        if cols == 0 || rows == 0 {
            return;
        }
        let (c, r) = ((px / 8) as usize, (py / 10) as usize);
        if c >= x0 && r >= y0 {
            self.surf_cx = ((c - x0) * W / cols).min(W - 1);
            self.surf_cy = ((r - y0) * H / rows).min(H - 1);
        }
    }

    /// Click on a row of the rules or options screen: the label selects, the arrows around the
    /// value change it (`code` = row x 3 + 0 for "<", 1 for the label, 2 for ">").
    pub fn click_rules_row(&mut self, code: usize) {
        self.rules_sel = code / 3;
        match code % 3 {
            0 => self.run_act(Act::Key(KeyCode::ArrowLeft)),
            2 => self.run_act(Act::Key(KeyCode::ArrowRight)),
            _ => {}
        }
    }

    pub fn click_options_row(&mut self, code: usize) {
        self.opt_sel = code / 3;
        match code % 3 {
            0 => self.run_act(Act::Key(KeyCode::ArrowLeft)),
            2 => self.run_act(Act::Key(KeyCode::ArrowRight)),
            _ => {}
        }
    }

    pub fn click_codex_cat(&mut self, i: usize) {
        self.codex_cat = i;
        self.codex_sel = 0;
    }

    /// Follow a cross-link of the codex: remember where we were.
    pub fn follow_codex_link(&mut self, code: usize) {
        self.codex_back.push((self.codex_cat, self.codex_sel));
        self.codex_cat = code / 10_000;
        self.codex_sel = code % 10_000;
    }

    pub fn click_codex_sel(&mut self, k: usize) {
        self.codex_sel = k;
    }

    pub fn click_sel_res(&mut self, i: usize) {
        self.sel_res = i;
    }

    pub fn click_building(&mut self, k: usize) {
        self.col_focus = 0;
        self.b_sel = k;
    }

    pub fn click_faction(&mut self, i: usize) {
        self.fac_sel = i;
    }

    /// Justice of the factions screen: 0 pay the fines, 1 bribe, 2 hire a lawyer (selected people).
    /// Buttons of the society page (see `society_view`).
    pub fn click_society(&mut self, k: usize) {
        use crate::society_view::*;
        let who = self.sim.crew.alive().nth(self.soc_who).map(|p| p.id);
        let said: Result<String, &str> = match k {
            ENROLL => who
                .ok_or("PERSONNE")
                .and_then(|id| self.sim.enroll(id, self.soc_study)),
            COMPOSE => who.ok_or("PERSONNE").and_then(|id| {
                self.sim.compose(id, self.soc_art).map(|i| {
                    format!(
                        "OEUVRE : {}",
                        self.sim.society.works[i].title.to_uppercase()
                    )
                })
            }),
            SELL => match self.sim.society.works.len() {
                0 => Err("AUCUNE OEUVRE"),
                n => self
                    .sim
                    .sell_work(n - 1)
                    .map(|v| format!("OEUVRE VENDUE {v:.0} CR")),
            },
            DONATE => match self.sim.society.works.len() {
                0 => Err("AUCUNE OEUVRE"),
                n => self.sim.donate_work(n - 1),
            },
            BET => {
                let odds = self.sim.field_odds(self.soc_sport);
                let fav = odds
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1 .0.total_cmp(&b.1 .0))
                    .map_or(0, |(i, _)| i);
                self.sim.bet(self.soc_sport, fav, 100.0).map(|r| {
                    if r.payout > 0.0 {
                        format!("PARI GAGNE : {:.0} CR", r.payout)
                    } else {
                        format!(
                            "PARI PERDU : GAGNANT {}",
                            self.sim
                                .runner_name(self.soc_sport, r.winner)
                                .to_uppercase()
                        )
                    }
                })
            }
            NEXT_ART => {
                self.soc_art = (self.soc_art + 1) % self.data.society.arts.len();
                Ok(format!(
                    "ART : {}",
                    self.data.society.arts[self.soc_art].name.to_uppercase()
                ))
            }
            _ => {
                self.soc_sport = (self.soc_sport + 1) % self.data.society.sports.len();
                Ok(format!(
                    "SPORT : {}",
                    self.data.society.sports[self.soc_sport].name.to_uppercase()
                ))
            }
        };
        match said {
            Ok(t) => self.say(t),
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    pub fn click_justice(&mut self, k: usize) {
        let f = self.fac_sel;
        let said = match k {
            0 => self
                .sim
                .pay_fines(f)
                .map(|x| format!("AMENDES PAYEES: {x:.0} CR")),
            1 => self.sim.bribe_officials(f).map(|ok| {
                if ok {
                    "POT-DE-VIN ACCEPTE: CASIER EFFACE".to_string()
                } else {
                    "POT-DE-VIN REFUSE: VOUS ETES FICHE".to_string()
                }
            }),
            _ => self
                .sim
                .hire_lawyer(f)
                .map(|()| "AVOCAT ENGAGE: AMENDES ET CASIER REDUITS DE MOITIE".to_string()),
        };
        match said {
            Ok(t) => self.say(t),
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    /// Row of the policies of the colony screen: code = row x 3 + 0 "<", 1 name, 2 ">".
    pub fn click_policy(&mut self, code: usize) {
        self.col_focus = 2;
        self.p_sel = code / 3;
        match code % 3 {
            0 => self.run_act(Act::Key(KeyCode::ArrowLeft)),
            2 => self.run_act(Act::Key(KeyCode::ArrowRight)),
            _ => {}
        }
    }

    /// A building of the build menu (a double click builds it).
    pub fn click_build(&mut self, k: usize) {
        self.col_focus = 3;
        self.build_sel = k;
    }

    pub fn click_storage(&mut self, i: usize) {
        self.col_focus = 1;
        self.s_sel = i;
    }

    pub fn click_contract(&mut self, i: usize) {
        self.contract_sel = i;
    }

    pub fn click_empire(&mut self, i: usize) {
        self.emp_sel = i;
    }

    pub fn click_fleet_ship(&mut self, i: usize) {
        self.fleet_focus = 0;
        self.ship_sel2 = i;
    }

    pub fn click_ship_slot(&mut self, i: usize) {
        self.ship_sel = i;
    }

    pub fn click_yard(&mut self, i: usize) {
        self.yard_sel = i;
    }

    pub fn click_crew(&mut self, i: usize) {
        self.crew_sel = i;
    }

    pub fn click_recruit(&mut self, i: usize) {
        self.recruit_sel = i;
    }

    /// Double click on a crew member: their family tree.
    pub fn open_family_of(&mut self, i: usize) {
        self.crew_sel = i;
        self.open_crew_tab(crate::game::CrewTab::Family);
    }

    /// Click on a body of the cockpit or of the map: target it.
    pub fn click_body(&mut self, i: usize) {
        if i < self.sim.bodies.len() {
            self.sim.target = Some(i);
            self.sim.target_npc = None;
        }
    }

    /// Double click on a body: target it and fly there (the autopilot).
    pub fn dbl_body(&mut self, i: usize) {
        self.click_body(i);
        self.run_act(Act::Key(KeyCode::KeyG));
    }

    pub fn click_npc(&mut self, k: usize) {
        if k < self.sim.npcs.len() {
            self.sim.target_npc = Some(k);
        }
    }

    pub fn click_star(&mut self, k: usize) {
        self.gal_sel = k;
    }

    /// Click on a crew member of the family tree: show their tree.
    pub fn click_family_member(&mut self, member: usize) {
        self.family_member = member;
        self.refresh_family();
    }

    /// A right click goes back, like Escape (except in flight, where it does nothing).
    pub fn pointer_back(&mut self) {
        if !matches!(
            self.screen,
            Screen::Flight | Screen::MainMenu | Screen::Destroyed
        ) {
            self.run_act(Act::Key(KeyCode::Escape));
        }
    }

    /// The wheel moves the selection of the current screen (`lines` > 0: up); in flight it
    /// changes the time scale.
    pub fn pointer_wheel(&mut self, lines: f64) {
        if let Some(m) = self.ctx_menu.as_mut() {
            let n = m.items.len().max(1);
            m.sel = if lines > 0.0 {
                (m.sel + n - 1) % n
            } else {
                (m.sel + 1) % n
            };
            return;
        }
        // Over the thrust lever or the trim of the cockpit, the wheel is theirs.
        if self.screen == Screen::Flight && self.cockpit_wheel(lines) {
            return;
        }
        let steps = lines.abs().ceil().max(1.0) as usize;
        for _ in 0..steps.min(5) {
            if self.screen == Screen::Galaxy {
                self.umap_zoom_by(if lines > 0.0 { 1.25 } else { 0.8 });
            } else if self.ext_active() {
                // In the third-person view the wheel zooms the camera.
                self.ext_zoom_by(if lines > 0.0 { 0.88 } else { 1.0 / 0.88 });
            } else if self.screen == Screen::Flight && self.map {
                // On the system map the wheel zooms.
                self.run_act(Act::Key(if lines > 0.0 {
                    KeyCode::Equal
                } else {
                    KeyCode::Minus
                }));
            } else if self.screen == Screen::Flight {
                self.run_act(Act::Key(if lines > 0.0 {
                    KeyCode::PageUp
                } else {
                    KeyCode::PageDown
                }));
            } else {
                self.run_act(Act::Key(if lines > 0.0 {
                    KeyCode::ArrowUp
                } else {
                    KeyCode::ArrowDown
                }));
            }
        }
    }
}

/// The two keys of a "A/B" word of the command bar (up and down, left and right, plus and minus):
/// the left half of the word presses the first, the right half the second.
pub fn pair_of_word(word: &str) -> Option<(KeyCode, KeyCode)> {
    match word {
        "HAUT/BAS" => Some((KeyCode::ArrowUp, KeyCode::ArrowDown)),
        "GAUCHE/DROITE" | "</>" => Some((KeyCode::ArrowLeft, KeyCode::ArrowRight)),
        "+/-" => Some((KeyCode::Equal, KeyCode::Minus)),
        _ => None,
    }
}

/// The key of a lone arrow word ("HAUT EMPRUNTER", "BAS REMBOURSER").
pub fn arrow_of_word(word: &str) -> Option<KeyCode> {
    match word {
        "HAUT" => Some(KeyCode::ArrowUp),
        "BAS" => Some(KeyCode::ArrowDown),
        "GAUCHE" => Some(KeyCode::ArrowLeft),
        "DROITE" => Some(KeyCode::ArrowRight),
        _ => None,
    }
}

/// The key a command-bar word stands for, if a click on it can sensibly press it.
pub fn key_of_word(screen: Screen, word: &str) -> Option<KeyCode> {
    let w = word.split('/').next().unwrap_or("");
    let key = match w {
        "TAB" => KeyCode::Tab,
        "ENTREE" | "ENT" => KeyCode::Enter,
        "FIN" => KeyCode::End,
        "ACCUEIL" => KeyCode::Home,
        "INSER" => KeyCode::Insert,
        "ECHAP" => KeyCode::Escape,
        "ESP" | "ESPACE" => KeyCode::Space,
        "PGUP" => KeyCode::PageUp,
        "PGDN" => KeyCode::PageDown,
        "SUPPR" => KeyCode::Delete,
        "F1" => KeyCode::F1,
        "F2" => KeyCode::F2,
        "F3" => KeyCode::F3,
        "F4" => KeyCode::F4,
        "F5" => KeyCode::F5,
        "F6" => KeyCode::F6,
        "F7" => KeyCode::F7,
        "F8" => KeyCode::F8,
        "F9" => KeyCode::F9,
        "F10" => KeyCode::F10,
        l if l.len() == 1 && l.as_bytes()[0].is_ascii_uppercase() => crate::script::key_code(l)?,
        _ => return None,
    };
    // In flight only the keys that make sense as a tap: held keys (thrust, brake, lasers) stay on
    // the keyboard.
    if screen == Screen::Flight
        && !matches!(
            key,
            KeyCode::Tab
                | KeyCode::KeyG
                | KeyCode::KeyD
                | KeyCode::KeyM
                | KeyCode::KeyL
                | KeyCode::KeyH
                | KeyCode::KeyI
                | KeyCode::KeyK
                | KeyCode::F1
                | KeyCode::F2
                | KeyCode::F3
                | KeyCode::F4
                | KeyCode::F6
                | KeyCode::F8
                | KeyCode::F10
                | KeyCode::End
                | KeyCode::Home
                | KeyCode::Insert
        )
    {
        return None;
    }
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_bar_words_map_to_taps_and_flight_holds_are_left_alone() {
        assert_eq!(key_of_word(Screen::Station, "ENTREE"), Some(KeyCode::Enter));
        assert_eq!(key_of_word(Screen::Station, "T"), Some(KeyCode::KeyT));
        assert_eq!(key_of_word(Screen::Station, "T/ECHAP"), Some(KeyCode::KeyT));
        assert_eq!(key_of_word(Screen::Crew, "G"), Some(KeyCode::KeyG));
        assert_eq!(key_of_word(Screen::Station, "HAUT/BAS"), None);
        assert_eq!(
            key_of_word(Screen::Flight, "C"),
            None,
            "the brake is held, not tapped"
        );
        assert_eq!(key_of_word(Screen::Flight, "MAJ"), None);
        assert_eq!(key_of_word(Screen::Flight, "M"), Some(KeyCode::KeyM));
        assert_eq!(key_of_word(Screen::Flight, "WASD/QE"), None);
    }
}
