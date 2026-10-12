//! The ship designer: a tab of the shipyard where the player assembles a ship from parts on a
//! grid, fits modules into their slots, reads the physics of the whole, and orders its
//! construction. Everything the numbers say comes from `vector_sim::shipwright`; this file only
//! draws and turns clicks and keys into calls.
use crate::game::{Game, Screen};
use crate::hub::wrap;
use crate::hud::{AMBER, CYAN, DIM, GREEN, HOVER, RED};
use crate::mouse::Act;
use crate::ship_preview;
use vector_data::ship::{CELL_M, PORT_NX, PORT_NY, PORT_NZ, PORT_PX, PORT_PY, PORT_PZ};
use vector_data::{GameData, Module, SlotType};
use vector_render::Framebuffer;
use vector_sim::shipwright::{
    analyze, landing, Blueprint, Report, GRID_X, GRID_Y, GRID_Z, MAX_CELLS,
};
use winit::keyboard::KeyCode;

/// Surface gravity the landing checks use when no world is chosen: the Earth's.
const REF_G: f64 = 9.806_65;

/// Pixels: the cell grid, the preview and the column of the lists.
const GX: i32 = 336;
const GY: i32 = 124;
const CELL_PX: i32 = 24;
const PREVIEW: (i32, i32, i32, i32) = (736, 96, 1272, 400);

pub const TABS: [&str; 4] = ["COQUES", "CONCEPTION", "EQUIPEMENT", "PLANS ET COMMANDES"];

/// What the designer remembers between frames (interface only: never saved).
#[derive(Clone, Debug)]
pub struct DesignerUi {
    /// 0 design, 1 equipment, 2 plans and orders.
    pub tab: u8,
    pub bp: usize,
    /// 0 place, 1 erase, 2 choose.
    pub tool: u8,
    pub part: usize,
    pub layer: i8,
    pub cursor: (i8, i8),
    /// Equipment tab: 0 grid, 1 slots, 2 modules.
    pub focus: u8,
    pub slot: usize,
    pub module: usize,
    pub yaw: f32,
    pub pitch: f32,
    pub mirror: bool,
    pub teams: u8,
    /// Plans tab: 0 mine, 1 for sale, 2 fleet.
    pub column: u8,
    pub sale: usize,
    pub fleet: usize,
    pub files: Vec<String>,
    pub file: usize,
    pub naming: Option<String>,
}

impl Default for DesignerUi {
    fn default() -> Self {
        DesignerUi {
            tab: 0,
            bp: 0,
            tool: 0,
            part: 0,
            layer: 1,
            cursor: (3, 0),
            focus: 0,
            slot: 0,
            module: 0,
            yaw: 0.6,
            pitch: 0.45,
            mirror: true,
            teams: 1,
            column: 0,
            sale: 0,
            fleet: 0,
            files: Vec::new(),
            file: 0,
            naming: None,
        }
    }
}

pub(crate) fn plans_dir() -> std::path::PathBuf {
    std::path::PathBuf::from("saves").join("plans")
}

pub(crate) fn list_plan_files() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(plans_dir())
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".ron"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub(crate) fn file_name_of(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{}.ron", s.to_lowercase())
}

/// Short text of what a module does, for the lists.
fn module_stat(m: &Module) -> String {
    if m.thrust > 0.0 {
        format!("POUSSEE {:.0} KN", m.thrust / 1e3)
    } else if m.power > 0.0 {
        format!("+{:.0} KW", m.power)
    } else if m.fuel > 0.0 {
        format!("CARB. {:.0} KG", m.fuel)
    } else if m.cargo_kg > 0.0 {
        format!("SOUTE {:.0} KG", m.cargo_kg)
    } else if m.crew_cap > 0.0 {
        format!("EQUIPAGE {:.0}", m.crew_cap)
    } else if m.life_support > 0.0 {
        format!("VIE {:.0} PERS.", m.life_support)
    } else if m.radiate > 0.0 {
        format!("RADIE {:.0} KW", m.radiate)
    } else if m.battery > 0.0 {
        format!("BATT {:.0} MJ", m.battery)
    } else if m.rcs > 0.0 {
        format!("MANOEUVRE {:.0} KN", m.rcs / 1e3)
    } else if m.armor > 0.0 {
        format!("BLINDAGE {:.0}", m.armor)
    } else if m.damage > 0.0 {
        format!("DEGATS {:.0}", m.damage)
    } else if m.power < 0.0 {
        format!("{:.0} KW", -m.power)
    } else {
        String::new()
    }
}

fn slot_label(t: SlotType) -> &'static str {
    t.label()
}

fn kind_color(kind: &str) -> u32 {
    ship_preview::part_color(kind)
}

fn frame(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, c: u32) {
    fb.rect_over(x, y, w, 1, c);
    fb.rect_over(x, y + h - 1, w, 1, c);
    fb.rect_over(x, y, 1, h, c);
    fb.rect_over(x + w - 1, y, 1, h, c);
}

fn window(sel: usize, n: usize, rows: usize) -> std::ops::Range<usize> {
    let start = sel.saturating_sub(rows / 2).min(n.saturating_sub(rows));
    start..(start + rows).min(n)
}

/// The tab bar shared by the shipyard and the designer; `active` is the lit one.
pub fn tabs(fb: &mut Framebuffer, g: &Game, active: usize) {
    let mut col = 1;
    for (i, t) in TABS.iter().enumerate() {
        let label = format!("[{t}]");
        let hover = g.hit_cells(col, 7, label.len(), 1, Act::Call(Game::dz_goto, i), None);
        let c = if i == active {
            AMBER
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        fb.text(col, 7, &label, c);
        col += label.len() + 2;
    }
}

/// A clickable text button; returns whether the pointer is over it.
fn button(fb: &mut Framebuffer, g: &Game, col: usize, row: usize, label: &str, act: Act) -> usize {
    let text = format!("[{label}]");
    let hover = g.hit_cells(col, row, text.chars().count(), 1, act, None);
    fb.text(col, row, &text, if hover { HOVER } else { GREEN });
    col + text.chars().count() + 1
}

impl Game {
    /// Blueprint being worked on (there is always one once the designer is open).
    fn dz_bp(&self) -> Option<&Blueprint> {
        self.sim.blueprints.get(self.dz.bp)
    }

    /// Make sure there is a plan to edit and the choices point at something real.
    pub fn dz_open(&mut self) {
        if self.sim.blueprints.is_empty() {
            let i = self.sim.new_blueprint("NOUVEAU VAISSEAU");
            self.dz.bp = i;
        }
        self.dz.bp = self.dz.bp.min(self.sim.blueprints.len() - 1);
        self.dz.part = self.dz.part.min(self.sim.data.ship_parts.len() - 1);
        self.dz.files = list_plan_files();
        self.screen = Screen::Designer;
    }

    pub fn dz_goto(&mut self, i: usize) {
        match i {
            0 if self.ng.designing => self.ng_close_designer(),
            0 => self.screen = Screen::Shipyard,
            n => {
                self.dz_open();
                self.dz.tab = (n - 1) as u8;
            }
        }
    }

    pub fn dz_tool(&mut self, i: usize) {
        self.dz.tool = i as u8;
    }

    pub fn dz_part(&mut self, i: usize) {
        self.dz.part = i.min(self.sim.data.ship_parts.len() - 1);
        self.dz.tool = 0;
    }

    pub fn dz_layer(&mut self, up: usize) {
        let z = self.dz.layer + if up == 1 { 1 } else { -1 };
        self.dz.layer = z.clamp(0, GRID_Z - 1);
    }

    pub fn dz_mirror(&mut self, _: usize) {
        self.dz.mirror = !self.dz.mirror;
    }

    pub fn dz_new(&mut self, _: usize) {
        self.dz.bp = self.sim.new_blueprint("NOUVEAU VAISSEAU");
        self.dz.tab = 0;
    }

    pub fn dz_copy(&mut self, _: usize) {
        if let Some(i) = self.sim.duplicate_blueprint(self.dz.bp) {
            self.dz.bp = i;
        }
    }

    pub fn dz_delete(&mut self, _: usize) {
        if self.sim.blueprints.len() > 1 {
            self.sim.delete_blueprint(self.dz.bp);
            self.dz.bp = self.dz.bp.min(self.sim.blueprints.len() - 1);
        } else {
            self.say("IL FAUT GARDER AU MOINS UN PLAN");
        }
    }

    pub fn dz_pick_plan(&mut self, i: usize) {
        if i < self.sim.blueprints.len() {
            self.dz.bp = i;
            self.dz.column = 0;
        }
    }

    pub fn dz_edit(&mut self, _: usize) {
        self.dz.tab = 0;
    }

    pub fn dz_rename(&mut self, _: usize) {
        if let Some(bp) = self.dz_bp() {
            self.dz.naming = Some(bp.name.clone());
        }
    }

    pub fn dz_buy(&mut self, i: usize) {
        match self.sim.buy_plan(i) {
            Ok(bi) => {
                let name = self.sim.blueprints[bi].name.to_uppercase();
                self.dz.bp = bi;
                self.say(format!("PLAN ACHETE : {name}"));
            }
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    pub fn dz_pick_sale(&mut self, i: usize) {
        self.dz.sale = i;
        self.dz.column = 1;
    }

    pub fn dz_pick_fleet(&mut self, i: usize) {
        self.dz.fleet = i;
        self.dz.column = 2;
    }

    pub fn dz_teams(&mut self, up: usize) {
        let t = i32::from(self.dz.teams) + if up == 1 { 1 } else { -1 };
        self.dz.teams = t.clamp(1, i32::from(vector_sim::yard::MAX_TEAMS)) as u8;
    }

    pub fn dz_order(&mut self, _: usize) {
        match self.sim.order_ship(self.dz.bp, self.dz.teams) {
            Ok(_) => {
                let (name, days) = self
                    .sim
                    .yard_orders
                    .last()
                    .map(|o| (o.name.to_uppercase(), (o.ready - self.sim.time) / 86_400.0))
                    .unwrap_or_default();
                self.say(format!("CONSTRUCTION COMMANDEE : {name} ({days:.1} JOURS)"));
            }
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    pub fn dz_board(&mut self, _: usize) {
        match self.sim.board_fleet_ship(self.dz.fleet) {
            Ok(()) => {
                self.say("VOUS AVEZ PRIS LE NOUVEAU VAISSEAU");
                self.screen = Screen::Station;
            }
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    pub fn dz_scrap(&mut self, _: usize) {
        match self.sim.recycle_fleet_ship(self.dz.fleet) {
            Ok(c) => self.say(format!("DEMONTE : MATERIAUX EN SOUTE, +{c:.0} CR")),
            Err(e) => self.say(e.to_uppercase()),
        }
        self.dz.fleet = self.dz.fleet.min(self.sim.fleet.len().saturating_sub(1));
    }

    pub fn dz_refit(&mut self, _: usize) {
        match self
            .sim
            .refit_fleet_ship(self.dz.fleet, self.dz.bp, self.dz.teams)
        {
            Ok(_) => self.say("REFONTE COMMANDEE AU CHANTIER"),
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    pub fn dz_export(&mut self, _: usize) {
        let Some(bp) = self.dz_bp() else { return };
        let (name, text) = (file_name_of(&bp.name), bp.to_text());
        let dir = plans_dir();
        let res =
            std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(dir.join(&name), text));
        match res {
            Ok(()) => self.say(format!("PLAN EXPORTE : {}", dir.join(&name).display())),
            Err(e) => self.say(format!("ECHEC EXPORT : {e}")),
        }
        self.dz.files = list_plan_files();
    }

    pub fn dz_import(&mut self, i: usize) {
        let Some(name) = self.dz.files.get(i).cloned() else {
            return;
        };
        self.dz.file = i;
        match std::fs::read_to_string(plans_dir().join(&name)) {
            Ok(text) => match self.sim.import_blueprint(&text) {
                Ok(bi) => {
                    self.dz.bp = bi;
                    self.say(format!("PLAN IMPORTE : {}", name.to_uppercase()));
                }
                Err(e) => self.say(e.to_uppercase()),
            },
            Err(e) => self.say(format!("ECHEC LECTURE : {e}")),
        }
    }

    pub fn dz_slot(&mut self, i: usize) {
        self.dz.slot = i;
        self.dz.module = 0;
        self.dz.focus = 2;
    }

    pub fn dz_module(&mut self, i: usize) {
        self.dz.module = i;
    }

    pub fn dz_fit(&mut self, _: usize) {
        let Some(bi) = self.sim.blueprints.get(self.dz.bp).map(|_| self.dz.bp) else {
            return;
        };
        let data = std::sync::Arc::clone(&self.sim.data);
        let (x, y) = self.dz.cursor;
        let z = self.dz.layer;
        let Some(m) = self.dz_candidates().get(self.dz.module).copied() else {
            return;
        };
        let slot = self.dz.slot as u8;
        let id = data.modules[m].id.clone();
        let bp = &mut self.sim.blueprints[bi];
        if bp.fit_at(x, y, z, slot).is_some() {
            bp.unfit(x, y, z, slot);
        }
        match bp.fit(&data, x, y, z, slot, &id) {
            Ok(()) => {}
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    pub fn dz_unfit(&mut self, _: usize) {
        let (x, y) = self.dz.cursor;
        let (z, slot) = (self.dz.layer, self.dz.slot as u8);
        if let Some(bp) = self.sim.blueprints.get_mut(self.dz.bp) {
            bp.unfit(x, y, z, slot);
        }
    }

    /// Modules that fit the selected slot of the cell under the cursor.
    fn dz_candidates(&self) -> Vec<usize> {
        let data = &self.sim.data;
        let Some(bp) = self.dz_bp() else {
            return Vec::new();
        };
        let (x, y) = self.dz.cursor;
        let Some(ci) = bp.cell_at(x, y, self.dz.layer) else {
            return Vec::new();
        };
        let Some(p) = data.ship_part_index(&bp.cells[ci].part) else {
            return Vec::new();
        };
        let Some(&(ty, size)) = data.ship_parts[p].slots.get(self.dz.slot) else {
            return Vec::new();
        };
        (0..data.modules.len())
            .filter(|&i| data.modules[i].slot == ty && data.modules[i].size <= size)
            .collect()
    }

    /// Put or remove the selected part on (x, y) of the current layer, with its mirror image.
    fn dz_apply(&mut self, x: i8, y: i8) {
        let z = self.dz.layer;
        self.dz.cursor = (x, y);
        if self.dz.tool == 2 || self.dz.tab == 1 {
            self.dz.slot = 0;
            self.dz.module = 0;
            return;
        }
        let data = std::sync::Arc::clone(&self.sim.data);
        let part = data.ship_parts[self.dz.part].id.clone();
        let mirror = self.dz.mirror;
        let Some(bp) = self.sim.blueprints.get_mut(self.dz.bp) else {
            return;
        };
        let mut ys = vec![y];
        if mirror && y != 0 {
            ys.push(-y);
        }
        let mut err = None;
        for yy in ys {
            if self.dz.tool == 0 {
                if let Err(e) = bp.place(&data, &part, x, yy, z) {
                    err = Some(e);
                }
            } else {
                bp.remove(x, yy, z);
            }
        }
        if let Some(e) = err {
            self.say(e.to_uppercase());
        }
    }

    pub fn dz_grid_click(&mut self, at: (i32, i32)) {
        let (cx, cy) = ((at.0 - GX) / CELL_PX, (at.1 - GY) / CELL_PX);
        if (0..i32::from(GRID_X)).contains(&cx) && (0..=2 * i32::from(GRID_Y)).contains(&cy) {
            self.dz_apply(cx as i8, (cy - i32::from(GRID_Y)) as i8);
        }
    }

    /// The pointer dragged over the preview: turn the ship.
    pub fn dz_drag(&mut self, dx: i32, dy: i32) {
        let (x, y) = self.pointer.pos;
        if x >= PREVIEW.0 && x < PREVIEW.2 && y >= PREVIEW.1 && y < PREVIEW.3 {
            self.dz.yaw += dx as f32 * 0.012;
            self.dz.pitch = (self.dz.pitch + dy as f32 * 0.012).clamp(-1.4, 1.4);
        }
    }

    pub fn designer_key(&mut self, code: KeyCode, text: Option<&str>) {
        if let Some(buf) = &mut self.dz.naming {
            match code {
                KeyCode::Enter => {
                    let name = buf.trim().to_string();
                    self.dz.naming = None;
                    if !name.is_empty() {
                        if let Some(bp) = self.sim.blueprints.get_mut(self.dz.bp) {
                            bp.name = name.to_uppercase();
                        }
                    }
                }
                KeyCode::Escape => self.dz.naming = None,
                KeyCode::Backspace => {
                    buf.pop();
                }
                _ => {
                    if let Some(t) = text {
                        for ch in t.chars().filter(|c| !c.is_control()) {
                            if buf.chars().count() < 24 {
                                buf.push(ch);
                            }
                        }
                    }
                }
            }
            return;
        }
        match code {
            KeyCode::Escape => {
                if self.ng.designing {
                    self.ng_close_designer();
                } else {
                    self.screen = Screen::Shipyard;
                }
                return;
            }
            KeyCode::Tab => {
                self.dz.tab = (self.dz.tab + 1) % 3;
                return;
            }
            KeyCode::KeyN => {
                self.dz_rename(0);
                return;
            }
            _ => {}
        }
        match self.dz.tab {
            0 => self.dz_key_design(code),
            1 => self.dz_key_equip(code),
            _ => self.dz_key_plans(code),
        }
    }

    fn dz_cursor(&mut self, code: KeyCode) -> bool {
        let (x, y) = self.dz.cursor;
        let c = match code {
            KeyCode::ArrowLeft => (x - 1, y),
            KeyCode::ArrowRight => (x + 1, y),
            KeyCode::ArrowUp => (x, y - 1),
            KeyCode::ArrowDown => (x, y + 1),
            _ => return false,
        };
        self.dz.cursor = (c.0.clamp(0, GRID_X - 1), c.1.clamp(-GRID_Y, GRID_Y));
        true
    }

    fn dz_key_design(&mut self, code: KeyCode) {
        if self.dz_cursor(code) {
            return;
        }
        let n = self.sim.data.ship_parts.len();
        match code {
            KeyCode::PageUp => self.dz.part = (self.dz.part + n - 1) % n,
            KeyCode::PageDown => self.dz.part = (self.dz.part + 1) % n,
            KeyCode::Home => self.dz_layer(1),
            KeyCode::End => self.dz_layer(0),
            KeyCode::Enter => {
                self.dz.tool = 0;
                let (x, y) = self.dz.cursor;
                self.dz_apply(x, y);
            }
            KeyCode::KeyX | KeyCode::Delete => {
                let t = self.dz.tool;
                self.dz.tool = 1;
                let (x, y) = self.dz.cursor;
                self.dz_apply(x, y);
                self.dz.tool = t;
            }
            KeyCode::KeyM => self.dz_mirror(0),
            _ => {}
        }
    }

    fn dz_key_equip(&mut self, code: KeyCode) {
        match code {
            KeyCode::Space => self.dz.focus = (self.dz.focus + 1) % 3,
            KeyCode::Home => self.dz_layer(1),
            KeyCode::End => self.dz_layer(0),
            KeyCode::Enter => self.dz_fit(0),
            KeyCode::KeyX | KeyCode::Delete => self.dz_unfit(0),
            KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                let up = matches!(code, KeyCode::ArrowUp | KeyCode::ArrowLeft);
                match self.dz.focus {
                    1 => {
                        let n = self.dz_slot_count().max(1);
                        self.dz.slot = if up {
                            (self.dz.slot + n - 1) % n
                        } else {
                            (self.dz.slot + 1) % n
                        };
                        self.dz.module = 0;
                    }
                    2 => {
                        let n = self.dz_candidates().len().max(1);
                        self.dz.module = if up {
                            (self.dz.module + n - 1) % n
                        } else {
                            (self.dz.module + 1) % n
                        };
                    }
                    _ => {
                        self.dz_cursor(code);
                        self.dz.slot = 0;
                        self.dz.module = 0;
                    }
                }
            }
            _ => {}
        }
    }

    fn dz_slot_count(&self) -> usize {
        let data = &self.sim.data;
        let Some(bp) = self.dz_bp() else { return 0 };
        bp.cell_at(self.dz.cursor.0, self.dz.cursor.1, self.dz.layer)
            .and_then(|ci| data.ship_part_index(&bp.cells[ci].part))
            .map_or(0, |p| data.ship_parts[p].slots.len())
    }

    fn dz_key_plans(&mut self, code: KeyCode) {
        let nm = self.sim.blueprints.len();
        let ns = self.sim.data.ship_plans.len();
        let nf = self.sim.fleet.len();
        match code {
            KeyCode::ArrowLeft => self.dz.column = (self.dz.column + 2) % 3,
            KeyCode::ArrowRight => self.dz.column = (self.dz.column + 1) % 3,
            KeyCode::ArrowUp | KeyCode::ArrowDown => {
                let up = code == KeyCode::ArrowUp;
                let step = |v: usize, n: usize| {
                    if n == 0 {
                        0
                    } else if up {
                        (v + n - 1) % n
                    } else {
                        (v + 1) % n
                    }
                };
                match self.dz.column {
                    0 => self.dz.bp = step(self.dz.bp, nm),
                    1 => self.dz.sale = step(self.dz.sale, ns),
                    _ => self.dz.fleet = step(self.dz.fleet, nf),
                }
            }
            KeyCode::PageUp => self.dz_teams(1),
            KeyCode::PageDown => self.dz_teams(0),
            KeyCode::Enter => match self.dz.column {
                0 => self.dz_edit(0),
                1 => self.dz_buy(self.dz.sale),
                _ => self.dz_board(0),
            },
            KeyCode::KeyO => self.dz_order(0),
            KeyCode::KeyC => self.dz_copy(0),
            KeyCode::KeyX => self.dz_delete(0),
            KeyCode::KeyE => self.dz_export(0),
            KeyCode::KeyI => self.dz_import(self.dz.file),
            KeyCode::KeyR => self.dz_refit(0),
            KeyCode::KeyD => self.dz_scrap(0),
            KeyCode::KeyF => self.dz_new(0),
            _ => {}
        }
    }
}

/// The commands of the screen, always shown at the bottom.
pub fn hint(g: &Game) -> &'static str {
    if g.dz.naming.is_some() {
        return "TAPEZ LE NOM  ENTREE VALIDER  ECHAP ANNULER";
    }
    match g.dz.tab {
        0 => "CLIC POSER  FLECHES CURSEUR  ENTREE POSER  X RETIRER  PGUP/PGDN PIECE  DEBUT/FIN COUCHE  M SYMETRIE  TAB ONGLET",
        1 => "CLIC CASE, EMPLACEMENT, MODULE  ESPACE ZONE  ENTREE INSTALLER  X RETIRER  DEBUT/FIN COUCHE  TAB ONGLET  ECHAP",
        _ => "GAUCHE/DROITE COLONNE  ENTREE VALIDER  O COMMANDER  PGUP/PGDN EQUIPES  F NOUVEAU  C COPIER  E EXPORTER  ECHAP",
    }
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    crate::ui::header(fb, g, "CHANTIER NAVAL : CONCEPTION");
    tabs(fb, g, 1 + usize::from(g.dz.tab));
    if let Some(buf) = &g.dz.naming {
        fb.text(40, 3, &format!("NOM DU PLAN : {buf}_"), AMBER);
    }
    let data = &g.sim.data;
    let Some(bp) = g.sim.blueprints.get(g.dz.bp) else {
        fb.text(1, 10, "AUCUN PLAN", RED);
        return;
    };
    let report = analyze(data, bp);
    match g.dz.tab {
        0 | 1 => {
            if g.dz.tab == 0 {
                draw_parts(fb, g, data);
            } else {
                draw_slots(fb, g, bp, data);
            }
            draw_grid(fb, g, bp, data);
            draw_preview(fb, g, bp, data, &report);
            draw_report(fb, g, bp, &report);
        }
        _ => draw_plans(fb, g, bp, &report),
    }
    // The commands are in the bar at the bottom (see `hint`); the messages above it.
    let b = fb.rows() - 5;
    for (i, msg) in g.messages.iter().rev().take(2).enumerate() {
        fb.text(1, b + 1 - i, msg, if i == 0 { AMBER } else { DIM });
    }
}

fn draw_parts(fb: &mut Framebuffer, g: &Game, data: &GameData) {
    fb.text(1, 9, "PIECES (CLIC : CHOISIR)", DIM);
    for (i, p) in data.ship_parts.iter().enumerate() {
        let row = 10 + i;
        let hover = g.hit_cells(1, row, 40, 1, Act::Call(Game::dz_part, i), None);
        let sel = i == g.dz.part;
        let c = if sel {
            AMBER
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        fb.rect_over(8, row as i32 * 10 + 1, 5, 7, kind_color(&p.kind));
        fb.text(
            3,
            row,
            &format!(
                "{}{:<28.28} {:>5.0}KG",
                if sel { ">" } else { " " },
                p.name,
                p.mass
            ),
            c,
        );
    }
    let p = &data.ship_parts[g.dz.part];
    let mut row = 10 + data.ship_parts.len() + 1;
    let ports: String = [
        (PORT_PX, "AV"),
        (PORT_NX, "AR"),
        (PORT_PY, "TRIB"),
        (PORT_NY, "BAB"),
        (PORT_PZ, "HAUT"),
        (PORT_NZ, "BAS"),
    ]
    .iter()
    .filter(|(b, _)| p.ports & b != 0)
    .map(|(_, n)| *n)
    .collect::<Vec<_>>()
    .join(" ");
    let slots: String = p
        .slots
        .iter()
        .map(|(t, s)| format!("{} {s}", slot_label(*t)))
        .collect::<Vec<_>>()
        .join(", ");
    let cost: f64 = p
        .materials
        .iter()
        .filter_map(|(r, kg)| data.resource_index(r).map(|i| data.resources[i].value * kg))
        .sum();
    let lines = [
        p.name.to_uppercase(),
        format!("MASSE {:.0} KG  STRUCTURE {:.0}", p.mass, p.hp),
        format!("VOLUME {:.0} M3 (HABITABLE {:.0})", p.volume, p.hab_m3),
        format!("COUCHETTES {}  TRAVAIL {:.0} J", p.berths, p.labor),
        format!("MATERIAUX {cost:.0} CR"),
        format!("RACCORDS {ports}"),
        format!(
            "EMPLACEMENTS {}",
            if slots.is_empty() {
                "AUCUN".to_string()
            } else {
                slots
            }
        ),
    ];
    for l in lines.iter() {
        fb.text(
            1,
            row,
            &l.chars().take(40).collect::<String>(),
            if row == 10 + data.ship_parts.len() + 1 {
                AMBER
            } else {
                GREEN
            },
        );
        row += 1;
    }
    for l in wrap(&p.text.to_uppercase(), 40) {
        fb.text(1, row, &l, DIM);
        row += 1;
    }
}

fn draw_slots(fb: &mut Framebuffer, g: &Game, bp: &Blueprint, data: &GameData) {
    let (x, y) = g.dz.cursor;
    let z = g.dz.layer;
    fb.text(1, 9, &format!("CASE ({x}, {y}, {z})"), AMBER);
    let Some(ci) = bp.cell_at(x, y, z) else {
        fb.text(1, 10, "CASE VIDE : CLIQUEZ UNE PIECE DE LA GRILLE", DIM);
        return;
    };
    let Some(pi) = data.ship_part_index(&bp.cells[ci].part) else {
        return;
    };
    let p = &data.ship_parts[pi];
    fb.text(1, 10, &p.name.to_uppercase(), GREEN);
    if p.slots.is_empty() {
        fb.text(1, 12, "CETTE PIECE N'A AUCUN EMPLACEMENT", DIM);
        return;
    }
    fb.text(1, 12, "EMPLACEMENTS", DIM);
    for (k, (ty, size)) in p.slots.iter().enumerate() {
        let row = 13 + k;
        let hover = g.hit_cells(1, row, 40, 1, Act::Call(Game::dz_slot, k), None);
        let fitted = bp
            .fit_at(x, y, z, k as u8)
            .and_then(|f| data.module_index(&f.module))
            .map_or("VIDE".to_string(), |m| data.modules[m].name.to_uppercase());
        let sel = k == g.dz.slot;
        fb.text(
            1,
            row,
            &format!(
                "{}{} {size} : {:<24.24}",
                if sel { ">" } else { " " },
                slot_label(*ty),
                fitted
            ),
            if sel {
                AMBER
            } else if hover {
                HOVER
            } else {
                GREEN
            },
        );
    }
    let cands: Vec<usize> = (0..data.modules.len())
        .filter(|&i| {
            let m = &data.modules[i];
            p.slots
                .get(g.dz.slot)
                .is_some_and(|(t, s)| m.slot == *t && m.size <= *s)
        })
        .collect();
    let top = 14 + p.slots.len();
    fb.text(
        1,
        top,
        &format!("MODULES POSSIBLES ({})  [ENTREE : INSTALLER]", cands.len()),
        DIM,
    );
    let rows = 40usize.min(60 - top);
    for (r, i) in window(g.dz.module, cands.len(), rows).enumerate() {
        let m = &data.modules[cands[i]];
        let row = top + 1 + r;
        let act = Act::Call(Game::dz_module, i);
        let hover = g.hit_cells(1, row, 40, 1, act, Some(Act::Call(Game::dz_fit, 0)));
        let sel = i == g.dz.module;
        fb.text(
            1,
            row,
            &format!(
                "{}{:<22.22} {:>5.0}KG {}",
                if sel { ">" } else { " " },
                m.name.to_uppercase(),
                m.mass,
                module_stat(m)
            )
            .chars()
            .take(40)
            .collect::<String>(),
            if sel {
                AMBER
            } else if hover {
                HOVER
            } else {
                GREEN
            },
        );
    }
    let b = 8;
    let _ = b;
}

fn draw_grid(fb: &mut Framebuffer, g: &Game, bp: &Blueprint, data: &GameData) {
    // Tools and layer.
    let mut col = 42;
    if g.dz.tab == 0 {
        fb.text(col - 1, 9, "OUTIL", DIM);
        col += 5;
        for (i, name) in ["POSER", "RETIRER", "CHOISIR"].iter().enumerate() {
            let label = format!("[{name}]");
            let hover = g.hit_cells(col, 9, label.len(), 1, Act::Call(Game::dz_tool, i), None);
            let c = if g.dz.tool as usize == i {
                AMBER
            } else if hover {
                HOVER
            } else {
                GREEN
            };
            fb.text(col, 9, &label, c);
            col += label.len() + 1;
        }
    } else {
        fb.text(col, 9, "CHOISISSEZ UNE CASE", DIM);
        col += 20;
    }
    fb.text(col + 1, 9, "COUCHE", DIM);
    col = button(fb, g, col + 8, 9, "-", Act::Call(Game::dz_layer, 0));
    fb.text(col, 9, &format!("{}", g.dz.layer), AMBER);
    col = button(fb, g, col + 2, 9, "+", Act::Call(Game::dz_layer, 1));
    let sym = format!("[SYMETRIE {}]", if g.dz.mirror { "OUI" } else { "NON" });
    let hover = g.hit_cells(
        col + 1,
        9,
        sym.len(),
        1,
        Act::Call(Game::dz_mirror, 0),
        None,
    );
    fb.text(col + 1, 9, &sym, if hover { HOVER } else { GREEN });
    fb.text(
        42,
        10,
        &format!(
            "PLAN : {}   PIECES {}/{}   AVANT ->",
            bp.name.to_uppercase(),
            bp.cells.len(),
            MAX_CELLS
        ),
        DIM,
    );
    let connected = bp.connected(data);
    let (w, h) = (
        i32::from(GRID_X) * CELL_PX,
        (2 * i32::from(GRID_Y) + 1) * CELL_PX,
    );
    g.hit((GX, GY, GX + w, GY + h), Act::At(Game::dz_grid_click), None);
    for gy in 0..=(2 * GRID_Y) {
        for gx in 0..GRID_X {
            let (x, y) = (gx, gy - GRID_Y);
            let (px, py) = (GX + i32::from(gx) * CELL_PX, GY + i32::from(gy) * CELL_PX);
            let here = bp.cell_at(x, y, g.dz.layer);
            let is_cursor = g.dz.cursor == (x, y);
            frame(
                fb,
                px,
                py,
                CELL_PX,
                CELL_PX,
                if y == 0 { 0x0020_5030 } else { 0x0014_2418 },
            );
            match here {
                Some(ci) => {
                    let c = &bp.cells[ci];
                    let part = data.ship_part_index(&c.part).map(|i| &data.ship_parts[i]);
                    let col = part.map_or(0x00FF_00FF, |p| kind_color(&p.kind));
                    fb.rect_over(px + 2, py + 2, CELL_PX - 4, CELL_PX - 4, col);
                    let fits = bp
                        .fits
                        .iter()
                        .filter(|f| (f.x, f.y, f.z) == (x, y, c.z))
                        .count();
                    if let Some(p) = part {
                        let initial = p.name.chars().next().unwrap_or('?');
                        fb.text_big_over(px + 8, py + 8, &initial.to_string(), 1, 0x0000_0000);
                    }
                    if fits > 0 {
                        fb.text_at(px + 3, py + 3, &fits.to_string(), 0x00FF_FFFF);
                    }
                    if !connected[ci] {
                        frame(fb, px + 1, py + 1, CELL_PX - 2, CELL_PX - 2, RED);
                    }
                }
                None => {
                    // A part on another layer at the same place: a dot.
                    if bp.cells.iter().any(|c| (c.x, c.y) == (x, y)) {
                        fb.rect_over(px + 10, py + 10, 4, 4, 0x0030_5040);
                    }
                }
            }
            if is_cursor {
                frame(fb, px, py, CELL_PX, CELL_PX, 0x00FF_FFFF);
                frame(fb, px + 1, py + 1, CELL_PX - 2, CELL_PX - 2, AMBER);
            }
        }
    }
}

fn draw_preview(fb: &mut Framebuffer, g: &Game, bp: &Blueprint, data: &GameData, r: &Report) {
    frame(
        fb,
        PREVIEW.0 - 1,
        PREVIEW.1 - 1,
        PREVIEW.2 - PREVIEW.0 + 2,
        PREVIEW.3 - PREVIEW.1 + 2,
        0x0020_5030,
    );
    fb.text_at(
        PREVIEW.0 + 4,
        PREVIEW.1 + 3,
        "APERCU (GLISSER : TOURNER)",
        DIM,
    );
    if bp.cells.is_empty() {
        return;
    }
    // The ship the plan makes, drawn by the same model as the outside view (audit B5).
    let hull = vector_sim::shipwright::hull_of(data, bp, r);
    let design = crate::ship_model::blueprint_design(data, bp);
    crate::ship_model::blueprint_view(
        fb,
        data,
        &hull,
        &design,
        (PREVIEW.0, PREVIEW.1 + 12, PREVIEW.2, PREVIEW.3),
        f64::from(g.dz.yaw),
        f64::from(g.dz.pitch),
        f64::from(crate::proto_view::anim_time()),
    );
}

fn draw_report(fb: &mut Framebuffer, g: &Game, bp: &Blueprint, r: &Report) {
    let data = &g.sim.data;
    let top = 41;
    let c_ok = |ok: bool| if ok { GREEN } else { AMBER };
    let col_a = 42;
    let col_b = 82;
    let col_c = 122;
    let torque = (r.parasitic[0].powi(2) + r.parasitic[1].powi(2)).sqrt();
    let a = [
        (
            format!("MASSE {:.0} KG (PLEINE {:.0})", r.mass_dry, r.mass_wet),
            GREEN,
        ),
        (
            format!(
                "COQUE {:.0} + MODULES {:.0} KG",
                r.mass_structure, r.mass_modules
            ),
            DIM,
        ),
        (
            format!(
                "CENTRE DE MASSE ({:.1}; {:.1}; {:.1}) M",
                r.com_wet[0], r.com_wet[1], r.com_wet[2]
            ),
            GREEN,
        ),
        (
            format!(
                "DIMENSIONS {:.0} x {:.0} x {:.0} M",
                r.extent[0], r.extent[1], r.extent[2]
            ),
            GREEN,
        ),
        (
            format!(
                "INERTIE {:.1E} {:.1E} {:.1E}",
                r.inertia[0], r.inertia[1], r.inertia[2]
            ),
            DIM,
        ),
        (
            format!("POUSSEE {:.0} KN   TWR {:.2}", r.thrust / 1e3, r.twr),
            c_ok(r.twr >= 1.0),
        ),
        (
            format!("COUPLE PARASITE {:.0} KN.M", torque / 1e3),
            c_ok(torque < 0.3 * r.rcs_torque.max(1.0)),
        ),
        (
            format!(
                "MANOEUVRE {:.0} KN.M {:.2} RAD/S2",
                r.rcs_torque / 1e3,
                r.ang_accel
            ),
            GREEN,
        ),
    ];
    let b = [
        (
            format!(
                "DELTA-V {:.1} / CHARGE {:.1} KM/S",
                r.dv_empty / 1e3,
                r.dv_loaded / 1e3
            ),
            c_ok(r.dv_empty >= 1000.0),
        ),
        (
            format!("CARBURANT {:.0} KG   ISP {:.0} S", r.fuel_kg, r.isp),
            GREEN,
        ),
        (
            format!(
                "ENERGIE {:.0} KW / PIC {:.0} KW",
                r.power_supply, r.power_peak
            ),
            c_ok(r.power_supply >= r.power_peak),
        ),
        (
            format!("TEMPERATURE A PLEINE CHARGE {:.0} K", r.temp_eq),
            c_ok(r.temp_eq < 650.0),
        ),
        (
            format!("STRUCTURE {:.0} PV   BLINDAGE {:.0}", r.hp, r.armor),
            GREEN,
        ),
        (
            format!("SOUTE {:.0} KG / {:.0} M3", r.cargo_kg, r.cargo_m3),
            GREEN,
        ),
        (
            format!(
                "COUCH. {} EQUIP. {:.0} VIE {:.0}",
                r.berths, r.crew_cap, r.life_cap
            ),
            GREEN,
        ),
        (
            format!("VOLUME {:.0} M3 (HAB. {:.0})", r.volume, r.hab_m3),
            DIM,
        ),
    ];
    for (i, (s, c)) in a.iter().enumerate() {
        fb.text(col_a, top + i, &s.chars().take(38).collect::<String>(), *c);
    }
    for (i, (s, c)) in b.iter().enumerate() {
        fb.text(col_b, top + i, &s.chars().take(38).collect::<String>(), *c);
    }
    // Landing and flight in air.
    let land = landing(r, REF_G);
    let lines = [
        format!(
            "POSER 1G {}  DECOLLER {}",
            if land.can_land { "OUI" } else { "NON" },
            if land.can_take_off { "OUI" } else { "NON" }
        ),
        format!("TRAINS {} PORTE {:.0} KG", r.gears, r.gear_support_kg),
        format!(
            "AIR : MARGE {:+.2}  AILES {:.0} M2",
            r.aero_margin, r.lift_area
        ),
    ];
    for (i, s) in lines.iter().enumerate() {
        fb.text(
            col_c,
            top + i,
            &s.chars().take(38).collect::<String>(),
            if i == 0 { c_ok(land.can_land) } else { GREEN },
        );
    }
    let mats: f64 = r
        .materials
        .iter()
        .map(|(i, kg)| data.resources[*i].value * kg)
        .sum();
    fb.text(
        col_c,
        top + 4,
        &format!("COUT {:.0} CR (MATERIAUX {:.0})", r.value, mats),
        AMBER,
    );
    fb.text(
        col_c,
        top + 5,
        &format!("TRAVAIL {:.0} J-HOMME", r.labor),
        GREEN,
    );
    // Verdict.
    let mut row = top + 9;
    if r.errors.is_empty() {
        fb.text(col_a, row, "PLAN CONSTRUCTIBLE", GREEN);
    } else {
        fb.text(col_a, row, "PLAN INCONSTRUCTIBLE", RED);
    }
    row += 1;
    for e in &r.errors {
        fb.text(
            col_a,
            row,
            &format!("ERREUR : {}", e.to_uppercase())
                .chars()
                .take(110)
                .collect::<String>(),
            RED,
        );
        row += 1;
    }
    for w in &r.warnings {
        if row >= fb.rows() - 5 {
            break;
        }
        fb.text(
            col_a,
            row,
            &format!("ATTENTION : {}", w.to_uppercase())
                .chars()
                .take(110)
                .collect::<String>(),
            AMBER,
        );
        row += 1;
    }
    let _ = (bp, CYAN, CELL_M);
}

fn draw_plans(fb: &mut Framebuffer, g: &Game, bp: &Blueprint, r: &Report) {
    let data = &g.sim.data;
    let s = &g.sim;
    let focus = |c: u8| if g.dz.column == c { AMBER } else { DIM };
    // Column 0: my plans.
    fb.text(1, 9, "MES PLANS", focus(0));
    for (i, p) in s.blueprints.iter().enumerate().take(24) {
        let row = 10 + i;
        let hover = g.hit_cells(
            1,
            row,
            48,
            1,
            Act::Call(Game::dz_pick_plan, i),
            Some(Act::Call(Game::dz_edit, 0)),
        );
        let sel = i == g.dz.bp;
        fb.text(
            1,
            row,
            &format!(
                "{}{:<26.26} {:>3} P. {}",
                if sel { ">" } else { " " },
                p.name.to_uppercase(),
                p.cells.len(),
                p.origin.to_uppercase()
            ),
            if sel {
                AMBER
            } else if hover {
                HOVER
            } else {
                GREEN
            },
        );
    }
    let mut col = 1;
    for (label, f) in [
        ("NOUVEAU", Game::dz_new as fn(&mut Game, usize)),
        ("COPIER", Game::dz_copy),
        ("SUPPRIMER", Game::dz_delete),
        ("NOM", Game::dz_rename),
    ] {
        col = button(fb, g, col, 35, label, Act::Call(f, 0));
    }
    let mut col = 1;
    col = button(fb, g, col, 37, "MODIFIER", Act::Call(Game::dz_edit, 0));
    button(fb, g, col, 37, "EXPORTER", Act::Call(Game::dz_export, 0));
    // Files that can be imported.
    fb.text(1, 39, "FICHIERS DE PLANS (CLIC : IMPORTER)", DIM);
    if g.dz.files.is_empty() {
        fb.text(1, 40, "AUCUN (DOSSIER saves/plans)", DIM);
    }
    for (i, f) in g.dz.files.iter().enumerate().take(8) {
        let row = 40 + i;
        let hover = g.hit_cells(1, row, 48, 1, Act::Call(Game::dz_import, i), None);
        fb.text(
            1,
            row,
            &format!(" {f}").to_uppercase(),
            if hover { HOVER } else { GREEN },
        );
    }
    // Column 1: plans for sale.
    fb.text(52, 9, "PLANS EN VENTE (STATION)", focus(1));
    for (i, p) in data.ship_plans.iter().enumerate() {
        let row = 10 + i;
        let owned = s.bought_plans.contains(&p.id);
        let hover = g.hit_cells(
            52,
            row,
            48,
            1,
            Act::Call(Game::dz_pick_sale, i),
            Some(Act::Call(Game::dz_buy, i)),
        );
        let sel = i == g.dz.sale;
        fb.text(
            52,
            row,
            &format!(
                "{}{:<24.24} {:>8.0} CR{}",
                if sel { ">" } else { " " },
                p.name.to_uppercase(),
                p.price,
                if owned { " POSSEDE" } else { "" }
            ),
            if sel {
                AMBER
            } else if hover {
                HOVER
            } else if owned {
                DIM
            } else {
                GREEN
            },
        );
    }
    if let Some(p) = data.ship_plans.get(g.dz.sale) {
        for (k, l) in wrap(&p.text.to_uppercase(), 48).iter().enumerate() {
            fb.text(52, 10 + data.ship_plans.len() + 1 + k, l, DIM);
        }
        button(
            fb,
            g,
            52,
            10 + data.ship_plans.len() + 6,
            "ACHETER LE PLAN",
            Act::Call(Game::dz_buy, g.dz.sale),
        );
    }
    // Column 2: construction.
    let x = 103;
    fb.text(
        x,
        9,
        &format!("CONSTRUIRE : {}", bp.name.to_uppercase()),
        focus(2),
    );
    let q = s.quote_build(bp, g.dz.teams, false);
    let mut row = 10;
    let mut put = |fb: &mut Framebuffer, text: String, c: u32| {
        fb.text(x, row, &text.chars().take(56).collect::<String>(), c);
        row += 1;
    };
    match (&q.problem, q.site) {
        (Some(p), _) => put(fb, format!("IMPOSSIBLE : {}", p.to_uppercase()), RED),
        (None, Some(vector_sim::yard::Site::Colony(_))) => {
            put(fb, "CHANTIER : VOTRE COLONIE".to_string(), GREEN)
        }
        (None, _) => put(fb, "CHANTIER : STATION".to_string(), GREEN),
    }
    put(
        fb,
        format!("EQUIPES {}  DELAI {:.1} JOURS", g.dz.teams, q.days),
        GREEN,
    );
    put(
        fb,
        format!("COUT {:.0} CR  (VOUS AVEZ {:.0})", q.credits, s.credits),
        if q.credits <= s.credits { GREEN } else { RED },
    );
    put(
        fb,
        "MATERIAUX (BESOIN / EN SOUTE / A ACHETER)".to_string(),
        DIM,
    );
    for (res, need, have, buy) in q.lines.iter().take(14) {
        put(
            fb,
            format!(
                " {:<22.22} {:>7.0} {:>7.0} {:>7.0}",
                data.resources[*res].name.to_uppercase(),
                need,
                have,
                buy
            ),
            if *buy > 0.0 { AMBER } else { GREEN },
        );
    }
    let by = row + 1;
    let mut c = button(fb, g, x, by, "-", Act::Call(Game::dz_teams, 0));
    c = button(fb, g, c, by, "+", Act::Call(Game::dz_teams, 1));
    button(
        fb,
        g,
        c + 1,
        by,
        "COMMANDER LA CONSTRUCTION",
        Act::Call(Game::dz_order, 0),
    );
    let _ = r;
    // Orders in progress and the custom ships of the fleet.
    let mut row = by + 2;
    fb.text(x, row, "EN CONSTRUCTION", DIM);
    row += 1;
    if s.yard_orders.is_empty() {
        fb.text(x, row, " RIEN", DIM);
        row += 1;
    }
    for o in &s.yard_orders {
        let days = ((o.ready - s.time) / 86_400.0).max(0.0);
        fb.text(
            x,
            row,
            &format!(
                " {:<24.24} {:>5.1} J  {} EQ.",
                o.name.to_uppercase(),
                days,
                o.teams
            )
            .chars()
            .take(56)
            .collect::<String>(),
            GREEN,
        );
        row += 1;
    }
    row += 1;
    fb.text(x, row, "MA FLOTTE (CLIC : CHOISIR)", focus(2));
    row += 1;
    for (i, f) in s.fleet.iter().enumerate().take(8) {
        let custom = s.plan_of_hull(f.hull).is_some();
        let hover = g.hit_cells(
            x,
            row,
            56,
            1,
            Act::Call(Game::dz_pick_fleet, i),
            Some(Act::Call(Game::dz_board, 0)),
        );
        let sel = i == g.dz.fleet;
        fb.text(
            x,
            row,
            &format!(
                "{}{:<26.26} {}",
                if sel { ">" } else { " " },
                f.name.to_uppercase(),
                if custom { "SUR MESURE" } else { "SERIE" }
            ),
            if sel {
                AMBER
            } else if hover {
                HOVER
            } else {
                GREEN
            },
        );
        row += 1;
    }
    let mut c = x;
    c = button(fb, g, c, row + 1, "EMBARQUER", Act::Call(Game::dz_board, 0));
    c = button(fb, g, c, row + 1, "DEMONTER", Act::Call(Game::dz_scrap, 0));
    button(
        fb,
        g,
        c,
        row + 1,
        "REFONDRE AVEC LE PLAN CHOISI",
        Act::Call(Game::dz_refit, 0),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_files_get_safe_names() {
        assert_eq!(file_name_of("Courrier Zéphyr-X"), "courrier_z_phyr_x.ron");
        assert!(file_name_of("../../etc/passwd").ends_with(".ron"));
        assert!(!file_name_of("../../etc/passwd").contains('/'));
    }

    #[test]
    fn wrapping_keeps_lines_short() {
        let l = wrap("un deux trois quatre cinq six sept huit neuf dix", 12);
        assert!(l.iter().all(|s| s.chars().count() <= 12), "{l:?}");
        assert_eq!(
            l.join(" "),
            "un deux trois quatre cinq six sept huit neuf dix"
        );
    }

    fn data() -> std::sync::Arc<GameData> {
        std::sync::Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    fn docked_game() -> Game {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.key(KeyCode::KeyD, None, true);
        g.sim.credits = 3.0e6;
        g.sim.researched.fill(true);
        g
    }

    fn part(g: &Game, id: &str) -> usize {
        g.sim.data.ship_part_index(id).expect("part")
    }

    /// Draw the screen so its clickable zones exist, then click at (x, y) like the mouse does.
    fn click(g: &mut Game, x: i32, y: i32) {
        let mut fb = Framebuffer::new(1280, 720);
        g.begin_frame();
        crate::ui::draw(&mut fb, g);
        g.pointer_click(x, y, 0);
    }

    fn cell_px(x: i32, y: i32) -> (i32, i32) {
        (
            GX + x * CELL_PX + CELL_PX / 2,
            GY + (y + i32::from(GRID_Y)) * CELL_PX + CELL_PX / 2,
        )
    }

    #[test]
    fn c_opens_the_designer_from_the_shipyard_and_escape_goes_back() {
        let mut g = docked_game();
        g.screen = Screen::Shipyard;
        g.key(KeyCode::KeyC, None, true);
        assert_eq!(g.screen, Screen::Designer);
        assert!(!g.sim.blueprints.is_empty(), "a plan to work on");
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Shipyard);
    }

    #[test]
    fn the_tabs_are_clickable_and_cycle_with_tab() {
        let mut g = docked_game();
        g.screen = Screen::Shipyard;
        // CONCEPTION is the second label of the bar (row 7).
        let col = format!("[{}]", TABS[0]).len() + 2 + 2;
        click(&mut g, (col * 8) as i32, 7 * 10 + 4);
        assert_eq!(g.screen, Screen::Designer);
        assert_eq!(g.dz.tab, 0);
        g.key(KeyCode::Tab, None, true);
        assert_eq!(g.dz.tab, 1);
        g.key(KeyCode::Tab, None, true);
        g.key(KeyCode::Tab, None, true);
        assert_eq!(g.dz.tab, 0);
        // The first label goes back to the hulls.
        click(&mut g, 2 * 8, 7 * 10 + 4);
        assert_eq!(g.screen, Screen::Shipyard);
    }

    #[test]
    fn clicking_the_grid_places_the_part_and_its_mirror_and_the_eraser_removes_them() {
        let mut g = docked_game();
        g.dz_open();
        g.dz.part = part(&g, "hull_frame");
        g.dz.layer = 1;
        let (px, py) = cell_px(3, 1);
        click(&mut g, px, py);
        let bp = &g.sim.blueprints[g.dz.bp];
        assert!(
            bp.cell_at(3, 1, 1).is_some() && bp.cell_at(3, -1, 1).is_some(),
            "mirrored"
        );
        // Symmetry off: one only.
        g.dz_mirror(0);
        let (px, py) = cell_px(5, 2);
        click(&mut g, px, py);
        let bp = &g.sim.blueprints[g.dz.bp];
        assert!(bp.cell_at(5, 2, 1).is_some() && bp.cell_at(5, -2, 1).is_none());
        // The eraser.
        g.dz_tool(1);
        let (px, py) = cell_px(5, 2);
        click(&mut g, px, py);
        assert!(g.sim.blueprints[g.dz.bp].cell_at(5, 2, 1).is_none());
        // Occupied: refused with a message.
        g.dz_tool(0);
        let (px, py) = cell_px(3, 1);
        click(&mut g, px, py);
        assert_eq!(g.sim.blueprints[g.dz.bp].cells.len(), 2);
        assert!(
            g.messages.last().is_some_and(|m| m.contains("OCCUPEE")),
            "{:?}",
            g.messages.last()
        );
        // The layer is a different place.
        g.dz_layer(1);
        let (px, py) = cell_px(3, 1);
        click(&mut g, px, py);
        assert!(g.sim.blueprints[g.dz.bp].cell_at(3, 1, 2).is_some());
    }

    #[test]
    fn the_keyboard_moves_the_cursor_and_places() {
        let mut g = docked_game();
        g.dz_open();
        g.dz.part = part(&g, "core_cockpit");
        g.dz.cursor = (0, 0);
        g.key(KeyCode::ArrowRight, None, true);
        g.key(KeyCode::ArrowDown, None, true);
        assert_eq!(g.dz.cursor, (1, 1));
        g.key(KeyCode::Enter, None, true);
        let bp = &g.sim.blueprints[g.dz.bp];
        assert!(bp.cell_at(1, 1, g.dz.layer).is_some() && bp.cell_at(1, -1, g.dz.layer).is_some());
        g.key(KeyCode::KeyX, None, true);
        assert!(g.sim.blueprints[g.dz.bp]
            .cell_at(1, 1, g.dz.layer)
            .is_none());
        for _ in 0..40 {
            g.key(KeyCode::ArrowLeft, None, true);
            g.key(KeyCode::ArrowUp, None, true);
        }
        assert_eq!(g.dz.cursor, (0, -GRID_Y), "the cursor stays on the grid");
    }

    #[test]
    fn fitting_modules_through_the_equipment_tab_follows_the_slot_rules() {
        let mut g = docked_game();
        g.dz_open();
        let bi = g.dz.bp;
        let d = std::sync::Arc::clone(&g.sim.data);
        g.sim.blueprints[bi]
            .place(&d, "core_cockpit", 0, 0, 1)
            .expect("place");
        g.dz.tab = 1;
        g.dz.cursor = (0, 0);
        g.dz.layer = 1;
        g.dz.slot = 0;
        // Slot 0 of the cockpit is Core 2: only core modules up to size 2.
        let cands = g.dz_candidates();
        assert!(!cands.is_empty());
        assert!(cands
            .iter()
            .all(|&m| d.modules[m].slot == SlotType::Core && d.modules[m].size <= 2));
        let torch = d.module_index("torch_mk2").expect("torch");
        g.dz.module = cands.iter().position(|&m| m == torch).expect("listed");
        g.key(KeyCode::Enter, None, true);
        assert!(g.sim.blueprints[bi].fit_at(0, 0, 1, 0).is_some());
        // Fitting another replaces it (no double use of the slot).
        let chem = d.module_index("chem_mk2").expect("chem");
        g.dz.module = g
            .dz_candidates()
            .iter()
            .position(|&m| m == chem)
            .expect("listed");
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.sim.blueprints[bi].fits.len(), 1);
        assert_eq!(g.sim.blueprints[bi].fits[0].module, "chem_mk2");
        g.key(KeyCode::KeyX, None, true);
        assert!(g.sim.blueprints[bi].fits.is_empty());
        // Empty cell: nothing to list.
        g.dz.cursor = (9, 3);
        assert!(g.dz_candidates().is_empty());
        // Removing the part takes its modules away.
        g.dz.cursor = (0, 0);
        g.dz_fit(0);
        assert!(!g.sim.blueprints[bi].fits.is_empty());
        g.sim.blueprints[bi].remove(0, 0, 1);
        assert!(g.sim.blueprints[bi].fits.is_empty());
    }

    #[test]
    fn dragging_on_the_preview_turns_it_and_elsewhere_does_nothing() {
        let mut g = docked_game();
        g.dz_open();
        let (yaw, pitch) = (g.dz.yaw, g.dz.pitch);
        g.pointer.pos = (400, 300);
        g.dz_drag(20, 5);
        assert_eq!((g.dz.yaw, g.dz.pitch), (yaw, pitch), "outside the preview");
        g.pointer.pos = (900, 250);
        g.pointer.down = true;
        g.pointer_move(910, 255);
        assert!(g.dz.yaw != yaw && g.dz.pitch != pitch);
        for _ in 0..500 {
            g.dz_drag(0, 100);
        }
        assert!(g.dz.pitch <= 1.4);
    }

    #[test]
    fn plans_are_bought_and_ordered_from_the_plans_tab() {
        let mut g = docked_game();
        g.dz_open();
        g.dz.tab = 2;
        let c0 = g.sim.credits;
        // The "ACHETER LE PLAN" button (a clickable text).
        g.dz.sale = 0;
        g.dz_buy(0);
        assert!(g.sim.credits < c0);
        assert_eq!(g.sim.blueprints[g.dz.bp].name, "Courrier Zéphyr-X");
        let c1 = g.sim.credits;
        g.dz_order(0);
        assert_eq!(g.sim.yard_orders.len(), 1);
        assert!(g.sim.credits < c1);
        assert!(g.messages.last().is_some_and(|m| m.contains("COMMANDEE")));
        // The order is on screen.
        let mut fb = Framebuffer::new(1280, 720);
        g.begin_frame();
        crate::ui::draw(&mut fb, &g);
        assert!(fb.px.iter().any(|&p| p != 0));
        // Done: the ship is in the fleet and can be boarded from the screen.
        g.sim.time = g.sim.yard_orders[0].ready + 1.0;
        g.advance(0.2);
        assert_eq!(g.sim.fleet.len(), 1);
        g.dz.fleet = 0;
        // A loaded hold forbids the swap, and says so.
        g.sim.ship.cargo.mass = 5.0;
        g.dz_board(0);
        assert!(
            g.messages.last().is_some_and(|m| m.contains("SOUTE")),
            "{:?}",
            g.messages.last()
        );
        g.sim.ship.cargo.mass = 0.0;
        g.dz_board(0);
        assert_eq!(
            g.sim.data.hulls[g.sim.ship.design.hull].name,
            "Courrier Zéphyr-X"
        );
        assert_eq!(g.screen, Screen::Station);
    }

    #[test]
    fn naming_a_plan_takes_typed_text() {
        let mut g = docked_game();
        g.dz_open();
        g.key(KeyCode::KeyN, None, true);
        assert!(g.dz.naming.is_some());
        for _ in 0..30 {
            g.key(KeyCode::Backspace, None, true);
        }
        g.key(KeyCode::KeyA, Some("a"), true);
        g.key(KeyCode::KeyB, Some("b"), true);
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.sim.blueprints[g.dz.bp].name, "AB");
        assert!(g.dz.naming.is_none());
        // Escape cancels without leaving the screen.
        g.key(KeyCode::KeyN, None, true);
        g.key(KeyCode::Escape, None, true);
        assert!(g.dz.naming.is_none());
        assert_eq!(g.screen, Screen::Designer);
    }

    #[test]
    fn every_tab_draws_and_its_hint_fits_the_bar() {
        let mut g = docked_game();
        g.dz_open();
        let _ = g.sim.buy_plan(0);
        let _ = g.sim.buy_plan(1);
        for tab in 0..3 {
            g.dz.tab = tab;
            let mut fb = Framebuffer::new(1280, 720);
            g.begin_frame();
            crate::ui::draw(&mut fb, &g);
            assert!(
                fb.px.iter().filter(|&&p| p != 0).count() > 2000,
                "tab {tab} draws"
            );
            let line = crate::tutorial::hint_line(&g);
            assert!(
                !line.is_empty() && line.len() <= 126,
                "tab {tab}: {} chars",
                line.len()
            );
            assert!(
                g.pointer.hits.borrow().len() > 5,
                "tab {tab} declares clickable zones"
            );
        }
        g.dz.naming = Some(String::new());
        assert!(hint(&g).len() <= 126);
        assert!(!crate::tutorial::help_lines(Screen::Designer).is_empty());
    }

    #[test]
    fn a_plan_with_everything_draws_the_preview_without_leaving_its_frame() {
        let mut g = docked_game();
        g.dz_open();
        let bi = g.sim.buy_plan(2).expect("plan");
        g.dz.bp = bi;
        let mut fb = Framebuffer::new(1280, 720);
        g.begin_frame();
        crate::ui::draw(&mut fb, &g);
        // Inside the preview something coloured has been drawn.
        let mut lit = 0;
        for y in PREVIEW.1..PREVIEW.3 {
            for x in PREVIEW.0..PREVIEW.2 {
                if fb.px[y as usize * 1280 + x as usize] != 0 {
                    lit += 1;
                }
            }
        }
        assert!(lit > 3000, "{lit}");
    }
}
