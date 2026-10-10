//! Game state, screens, input handling and save/load glue.
use crate::{console, scene};
use std::{collections::HashSet, path::PathBuf, sync::Arc};
use vector_core::{mix64, Vec3};
use vector_data::GameData;
use vector_sim::{autopilot::Autopilot, Event, ShipInput, Sim, SIM_DT};
use vector_world::BodyKind;
use winit::keyboard::KeyCode;

pub const DEFAULT_SEED: u64 = 0x5EC7;
pub const MAIN_MENU: [&str; 3] = ["NOUVELLE PARTIE", "CONTINUER", "QUITTER"];
pub const PAUSE_MENU: [&str; 5] = [
    "REPRENDRE",
    "SAUVEGARDER",
    "CHARGER",
    "MENU PRINCIPAL",
    "QUITTER",
];
const TIME_LADDER: [f64; 10] = [
    0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 100.0, 1000.0, 10_000.0, 100_000.0,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    MainMenu,
    Flight,
    Station,
    Inventory,
    Pause,
    Ship,
    Shipyard,
}

pub struct Game {
    pub data: Arc<GameData>,
    pub sim: Sim,
    pub stars: Vec<Vec3>,
    pub keys: HashSet<KeyCode>,
    pub timescale: f64,
    pub paused: bool,
    pub messages: Vec<String>,
    pub console: console::Console,
    pub map: bool,
    pub map_scale: f64,
    pub map_on_ship: bool,
    pub screen: Screen,
    pub menu_sel: usize,
    pub seed_text: String,
    pub sel_res: usize,
    pub quit: bool,
    pub ship_sel: usize,
    pub picker: bool,
    pub pick_sel: usize,
    pub yard_sel: usize,
    pub save_path: PathBuf,
}

pub fn parse_seed(text: &str) -> u64 {
    let t = text.trim();
    if t.is_empty() {
        return DEFAULT_SEED;
    }
    t.parse::<u64>()
        .unwrap_or_else(|_| t.bytes().fold(0xC0FFEE, |h, b| mix64(h ^ u64::from(b))))
}

impl Game {
    pub fn new(data: Arc<GameData>, seed: u64) -> Game {
        let sim = scene::new_sim(seed, Arc::clone(&data));
        let mut g = Game {
            data,
            sim,
            stars: scene::star_field(seed, 900),
            keys: HashSet::new(),
            timescale: 1.0,
            paused: false,
            messages: vec!["BIENVENUE A BORD - [F1] AIDE".into()],
            console: console::Console::default(),
            map: false,
            map_scale: 1.0,
            map_on_ship: false,
            screen: Screen::MainMenu,
            menu_sel: 0,
            seed_text: String::new(),
            sel_res: 0,
            quit: false,
            ship_sel: 0,
            picker: false,
            pick_sel: 0,
            yard_sel: 0,
            save_path: PathBuf::from("saves").join("slot1.sav"),
        };
        g.fit_map();
        g
    }

    /// Start a fresh game from a seed and go to the cockpit.
    pub fn new_game(&mut self, seed: u64) {
        self.sim = scene::new_sim(seed, Arc::clone(&self.data));
        self.stars = scene::star_field(seed, 900);
        self.timescale = 1.0;
        self.keys.clear();
        self.messages = vec![format!("NOUVELLE PARTIE - GRAINE {seed}")];
        self.screen = Screen::Flight;
        self.fit_map();
    }

    pub fn fit_map(&mut self) {
        let max_a = self
            .sim
            .system
            .bodies
            .iter()
            .filter_map(|b| {
                b.orbit
                    .as_ref()
                    .filter(|o| o.parent == 0)
                    .map(|o| o.a * (1.0 + o.e))
            })
            .fold(1.0e9, f64::max);
        self.map_scale = max_a * 1.15 / 270.0;
    }

    pub fn input(&self) -> ShipInput {
        if self.console.open || self.screen != Screen::Flight {
            return ShipInput::default();
        }
        let ax = |pos: KeyCode, neg: KeyCode| {
            f64::from(self.keys.contains(&pos) as i8) - f64::from(self.keys.contains(&neg) as i8)
        };
        ShipInput {
            pitch: ax(KeyCode::KeyW, KeyCode::KeyS),
            yaw: ax(KeyCode::KeyD, KeyCode::KeyA),
            roll: ax(KeyCode::KeyQ, KeyCode::KeyE),
            fwd: ax(KeyCode::Space, KeyCode::KeyX),
            lat: ax(KeyCode::ArrowRight, KeyCode::ArrowLeft),
            vert: ax(KeyCode::ArrowUp, KeyCode::ArrowDown),
            mine: self.keys.contains(&KeyCode::KeyV),
        }
    }

    pub fn view(&self) -> crate::hud::View<'_> {
        crate::hud::View {
            sim: &self.sim,
            stars: &self.stars,
            timescale: self.timescale,
            paused: self.paused,
            messages: &self.messages,
            console: &self.console,
            map: self.map,
            map_scale: self.map_scale,
            map_on_ship: self.map_on_ship,
        }
    }

    pub fn say(&mut self, s: impl Into<String>) {
        self.messages.push(s.into());
        if self.messages.len() > 8 {
            self.messages.remove(0);
        }
    }

    /// Whether the simulation should currently run.
    pub fn running(&self) -> bool {
        !self.paused
            && matches!(
                self.screen,
                Screen::Flight | Screen::Station | Screen::Inventory
            )
    }

    /// Advance the simulation by `seconds` of real time at the current time scale.
    pub fn advance(&mut self, seconds: f64) {
        let input = if self.timescale > 2.0 {
            ShipInput::default()
        } else {
            self.input()
        };
        self.sim.events.clear();
        if self.timescale <= 1.0 {
            let steps = (seconds / SIM_DT).round() as usize;
            for _ in 0..steps {
                self.sim.step_dt(&input, SIM_DT);
            }
        } else {
            self.sim.advance(seconds * self.timescale, &input);
        }
        self.handle_events();
    }

    fn handle_events(&mut self) {
        let evs = std::mem::take(&mut self.sim.events);
        for e in evs {
            match e {
                Event::Impact {
                    body,
                    speed,
                    damage,
                } => {
                    let name = self.sim.bodies[body].name.clone();
                    self.say(format!("IMPACT {name} {speed:.0} M/S  DEGATS {damage:.0}"));
                    self.timescale = 1.0;
                }
                Event::OutOfFuel => self.say("CARBURANT EPUISE"),
                Event::Arrived { body } => {
                    let name = self.sim.bodies[body].name.clone();
                    self.say(format!("ARRIVE A {name}"));
                    self.timescale = 1.0;
                }
                Event::CargoFull => self.say("SOUTE PLEINE"),
                Event::Overheat => self.say("ALERTE: SURCHAUFFE !"),
                Event::ModuleLost { slot } => {
                    self.say(format!("MODULE PERDU (EMPLACEMENT {})", slot + 1))
                }
                Event::Docked { body } => {
                    let name = self.sim.bodies[body].name.clone();
                    self.sim.market(body);
                    self.say(format!("AMARRE A {name}"));
                    self.screen = Screen::Station;
                    self.timescale = 1.0;
                }
                Event::Undocked => {
                    self.screen = Screen::Flight;
                    self.say("DECOLLAGE");
                }
            }
        }
    }

    fn toggle_autopilot(&mut self) {
        if self.sim.autopilot.take().is_some() {
            self.say("PILOTE AUTO DESACTIVE");
            return;
        }
        let Some(t) = self.sim.target else {
            self.say("PILOTE AUTO: AUCUNE CIBLE");
            return;
        };
        match Autopilot::new(&self.sim, t) {
            Ok(ap) => {
                self.sim.autopilot = Some(ap);
                self.say("PILOTE AUTO ENGAGE");
            }
            Err(e) => self.say(format!("PILOTE AUTO REFUSE: {e}")),
        }
    }

    fn step_time(&mut self, up: bool) {
        let i = TIME_LADDER
            .iter()
            .position(|&t| t >= self.timescale)
            .unwrap_or(2);
        let j = if up {
            (i + 1).min(TIME_LADDER.len() - 1)
        } else {
            i.saturating_sub(1)
        };
        self.timescale = TIME_LADDER[j];
    }

    /// Cycle targets nearest-first, optionally restricted by a filter.
    pub fn cycle_target(&mut self, filter: impl Fn(&BodyKind) -> bool) {
        let p = self.sim.ship.pos;
        let mut idx: Vec<usize> = (0..self.sim.bodies.len())
            .filter(|&i| filter(&self.sim.bodies[i].kind))
            .collect();
        if idx.is_empty() {
            self.say("AUCUNE CIBLE DE CE TYPE");
            return;
        }
        idx.sort_by(|&a, &b| {
            (self.sim.bodies[a].pos - p)
                .length()
                .total_cmp(&(self.sim.bodies[b].pos - p).length())
        });
        let next = match self
            .sim
            .target
            .and_then(|t| idx.iter().position(|&i| i == t))
        {
            Some(k) => idx[(k + 1) % idx.len()],
            None => idx[0],
        };
        self.sim.target = Some(next);
    }

    pub fn save_game(&mut self) {
        let result = self
            .sim
            .save_bytes()
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                if let Some(dir) = self.save_path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                std::fs::write(&self.save_path, bytes).map_err(|e| e.to_string())
            });
        match result {
            Ok(()) => self.say("PARTIE SAUVEGARDEE"),
            Err(e) => self.say(format!("ECHEC SAUVEGARDE: {e}")),
        }
    }

    pub fn load_game(&mut self) -> bool {
        let loaded = std::fs::read(&self.save_path)
            .map_err(|e| e.to_string())
            .and_then(|b| Sim::load_bytes(&b, Arc::clone(&self.data)).map_err(|e| e.to_string()));
        match loaded {
            Ok(sim) => {
                self.stars = scene::star_field(sim.system.seed, 900);
                self.screen = if sim.docked.is_some() {
                    Screen::Station
                } else {
                    Screen::Flight
                };
                self.sim = sim;
                self.timescale = 1.0;
                self.fit_map();
                self.say("PARTIE CHARGEE");
                true
            }
            Err(e) => {
                self.say(format!("ECHEC CHARGEMENT: {e}"));
                false
            }
        }
    }

    pub fn key(&mut self, code: KeyCode, text: Option<&str>, pressed: bool) {
        if pressed && code == KeyCode::Backquote && self.screen != Screen::MainMenu {
            self.console.open = !self.console.open;
            return;
        }
        if self.console.open {
            if pressed {
                self.console_key(code, text);
            }
            return;
        }
        if !pressed {
            self.keys.remove(&code);
            return;
        }
        self.keys.insert(code);
        match self.screen {
            Screen::MainMenu => self.main_menu_key(code, text),
            Screen::Pause => self.pause_key(code),
            Screen::Station => self.station_key(code),
            Screen::Inventory => self.inventory_key(code),
            Screen::Ship => self.ship_key(code),
            Screen::Shipyard => self.shipyard_key(code),
            Screen::Flight => self.flight_key(code),
        }
    }

    fn console_key(&mut self, code: KeyCode, text: Option<&str>) {
        match code {
            KeyCode::Enter => {
                let line = std::mem::take(&mut self.console.line);
                self.console.print(format!("> {line}"));
                for l in console::run(&line, &mut self.sim, &mut self.timescale) {
                    self.console.print(l);
                }
            }
            KeyCode::Backspace => {
                self.console.line.pop();
            }
            KeyCode::Escape => self.console.open = false,
            _ => {
                if let Some(t) = text {
                    self.console
                        .line
                        .extend(t.chars().filter(|c| !c.is_control()));
                }
            }
        }
    }

    fn menu_nav(&mut self, code: KeyCode, len: usize) -> bool {
        match code {
            KeyCode::ArrowUp => self.menu_sel = (self.menu_sel + len - 1) % len,
            KeyCode::ArrowDown => self.menu_sel = (self.menu_sel + 1) % len,
            KeyCode::Enter => return true,
            _ => {}
        }
        false
    }

    fn main_menu_key(&mut self, code: KeyCode, text: Option<&str>) {
        if self.menu_nav(code, MAIN_MENU.len()) {
            match self.menu_sel {
                0 => {
                    let seed = parse_seed(&self.seed_text);
                    self.new_game(seed);
                }
                1 => {
                    self.load_game();
                }
                _ => self.quit = true,
            }
            return;
        }
        if self.menu_sel == 0 {
            match code {
                KeyCode::Backspace => {
                    self.seed_text.pop();
                }
                _ => {
                    if let Some(t) = text {
                        if self.seed_text.len() < 20 {
                            self.seed_text
                                .extend(t.chars().filter(|c| c.is_alphanumeric()));
                        }
                    }
                }
            }
        }
        if code == KeyCode::Escape {
            self.quit = true;
        }
    }

    fn pause_key(&mut self, code: KeyCode) {
        if code == KeyCode::Escape {
            self.screen = Screen::Flight;
            return;
        }
        if self.menu_nav(code, PAUSE_MENU.len()) {
            match self.menu_sel {
                0 => self.screen = Screen::Flight,
                1 => {
                    self.save_game();
                    self.screen = Screen::Flight;
                }
                2 => {
                    self.load_game();
                }
                3 => {
                    self.screen = Screen::MainMenu;
                    self.menu_sel = 0;
                }
                _ => self.quit = true,
            }
        }
    }

    fn station_key(&mut self, code: KeyCode) {
        let n = self.data.resources.len();
        let res = self.sel_res.min(n - 1);
        let msg: Result<String, &str> = match code {
            KeyCode::ArrowUp => {
                self.sel_res = (res + n - 1) % n;
                return;
            }
            KeyCode::ArrowDown => {
                self.sel_res = (res + 1) % n;
                return;
            }
            KeyCode::Enter => self
                .sim
                .sell(res, None)
                .map(|c| format!("VENDU POUR {c:.0} CR")),
            KeyCode::KeyS => self
                .sim
                .sell(res, Some(100.0))
                .map(|c| format!("VENDU 100 KG POUR {c:.0} CR")),
            KeyCode::KeyB => self
                .sim
                .buy(res, 100.0)
                .map(|(kg, c)| format!("ACHETE {kg:.0} KG POUR {c:.0} CR")),
            KeyCode::KeyF => self
                .sim
                .refuel()
                .map(|(kg, c)| format!("PLEIN: +{kg:.0} KG POUR {c:.0} CR")),
            KeyCode::KeyH => {
                self.open_ship();
                return;
            }
            KeyCode::KeyY => {
                self.screen = Screen::Shipyard;
                return;
            }
            KeyCode::KeyR => self.sim.repair().map(|c| format!("REPARATIONS: {c:.0} CR")),
            KeyCode::KeyU | KeyCode::Escape => {
                self.sim.undock();
                self.handle_events();
                return;
            }
            _ => return,
        };
        match msg {
            Ok(m) => self.say(m),
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    fn open_ship(&mut self) {
        self.screen = Screen::Ship;
        self.picker = false;
        self.ship_sel = 0;
    }

    /// Modules that can go into slot `slot` of the current hull, grouped by category then size.
    pub fn compatible_modules(&self, slot: usize) -> Vec<usize> {
        let (ty, size) = self.data.hulls[self.sim.ship.design.hull].slots[slot];
        let mut v: Vec<usize> = (0..self.data.modules.len())
            .filter(|&i| self.data.modules[i].slot == ty && self.data.modules[i].size <= size)
            .collect();
        v.sort_by(|&a, &b| {
            let (ma, mb) = (&self.data.modules[a], &self.data.modules[b]);
            ma.category.cmp(&mb.category).then(ma.size.cmp(&mb.size))
        });
        v
    }

    fn ship_key(&mut self, code: KeyCode) {
        let n = self.sim.ship.design.slots.len();
        let docked = self.sim.docked.is_some();
        if self.picker {
            let list = self.compatible_modules(self.ship_sel);
            let len = list.len().max(1);
            match code {
                KeyCode::ArrowUp => self.pick_sel = (self.pick_sel + len - 1) % len,
                KeyCode::ArrowDown => self.pick_sel = (self.pick_sel + 1) % len,
                KeyCode::Escape => self.picker = false,
                KeyCode::Enter => {
                    if let Some(&mi) = list.get(self.pick_sel) {
                        let res = if self.sim.module_stock[mi] > 0 {
                            Ok(())
                        } else {
                            self.sim.shop_buy_module(mi).map(|_| ())
                        }
                        .and_then(|()| self.sim.install_module(self.ship_sel, mi));
                        match res {
                            Ok(()) => {
                                let name = self.data.modules[mi].name.clone();
                                self.say(format!("INSTALLE: {name}"));
                                self.picker = false;
                            }
                            Err(e) => self.say(e.to_uppercase()),
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        match code {
            KeyCode::ArrowUp => self.ship_sel = (self.ship_sel + n - 1) % n,
            KeyCode::ArrowDown => self.ship_sel = (self.ship_sel + 1) % n,
            KeyCode::Enter if docked => {
                if self.sim.ship.design.slots[self.ship_sel].is_some() {
                    self.say("EMPLACEMENT OCCUPE - [X] RETIRER D ABORD");
                } else {
                    self.picker = true;
                    self.pick_sel = 0;
                }
            }
            KeyCode::KeyX if docked => match self.sim.uninstall_module(self.ship_sel) {
                Ok(()) => self.say("MODULE RETIRE (STOCK)"),
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::KeyY if docked => self.screen = Screen::Shipyard,
            KeyCode::Escape | KeyCode::KeyH => {
                self.screen = if docked {
                    Screen::Station
                } else {
                    Screen::Flight
                };
            }
            _ => {}
        }
    }

    fn shipyard_key(&mut self, code: KeyCode) {
        let n = self.data.hulls.len();
        match code {
            KeyCode::ArrowUp => self.yard_sel = (self.yard_sel + n - 1) % n,
            KeyCode::ArrowDown => self.yard_sel = (self.yard_sel + 1) % n,
            KeyCode::Enter => match self.sim.buy_hull(self.yard_sel) {
                Ok(p) => {
                    let name = self.data.hulls[self.yard_sel].name.clone();
                    self.say(format!("NOUVELLE COQUE: {name} ({p:.0} CR)"));
                    self.screen = Screen::Ship;
                    self.ship_sel = 0;
                }
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::Escape => self.screen = Screen::Station,
            _ => {}
        }
    }

    fn inventory_key(&mut self, code: KeyCode) {
        let n = self.data.resources.len();
        match code {
            KeyCode::ArrowUp | KeyCode::ArrowDown => {
                let step = if code == KeyCode::ArrowUp { n - 1 } else { 1 };
                let mut i = self.sel_res;
                for _ in 0..n {
                    i = (i + step) % n;
                    if self.sim.ship.cargo.kg[i] > 0.01 {
                        self.sel_res = i;
                        break;
                    }
                }
            }
            KeyCode::KeyJ => {
                let kg = self.sim.jettison(self.sel_res);
                self.say(format!("JETE {kg:.0} KG"));
            }
            KeyCode::Escape | KeyCode::KeyI => self.screen = Screen::Flight,
            _ => {}
        }
    }

    fn flight_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Escape => {
                self.screen = Screen::Pause;
                self.menu_sel = 0;
            }
            KeyCode::KeyF => {
                self.sim.ship.assist = !self.sim.ship.assist;
                let a = self.sim.ship.assist;
                self.say(if a {
                    "ASSISTANCE DE VOL ACTIVEE"
                } else {
                    "ASSISTANCE DE VOL COUPEE"
                });
            }
            KeyCode::Tab => self.cycle_target(|_| true),
            KeyCode::KeyT => self.cycle_target(|k| matches!(k, BodyKind::Asteroid(_))),
            KeyCode::KeyY => self.cycle_target(|k| matches!(k, BodyKind::Station)),
            KeyCode::KeyP => self.paused = !self.paused,
            KeyCode::PageUp => self.step_time(true),
            KeyCode::PageDown => self.step_time(false),
            KeyCode::KeyG => self.toggle_autopilot(),
            KeyCode::KeyM => self.map = !self.map,
            KeyCode::KeyC => self.map_on_ship = !self.map_on_ship,
            KeyCode::KeyI => self.screen = Screen::Inventory,
            KeyCode::KeyH => self.open_ship(),
            KeyCode::KeyD => match self.sim.try_dock() {
                Ok(_) => self.handle_events(),
                Err(e) => self.say(format!("AMARRAGE IMPOSSIBLE: {}", e.to_uppercase())),
            },
            KeyCode::F5 => self.save_game(),
            KeyCode::F9 => {
                self.load_game();
            }
            KeyCode::Equal | KeyCode::NumpadAdd => self.map_scale /= 1.5,
            KeyCode::Minus | KeyCode::NumpadSubtract => self.map_scale *= 1.5,
            KeyCode::F1 => {
                self.say("WASD/QE:ROT ESPACE/X:POUSSEE FLECHES:RCS F:ASSIST V:LASER");
                self.say("TAB/T/Y:CIBLE G:AUTO D:AMARRER I:SOUTE M:CARTE F5/F9:SAUV/CHARG");
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    #[test]
    fn seed_parsing() {
        assert_eq!(parse_seed("123"), 123);
        assert_eq!(parse_seed(""), DEFAULT_SEED);
        assert_eq!(parse_seed("abc"), parse_seed("abc"));
        assert_ne!(parse_seed("abc"), parse_seed("abd"));
    }

    #[test]
    fn menu_flow_new_game_and_save_load() {
        let mut g = Game::new(data(), 1);
        g.save_path = std::env::temp_dir().join("vector_test_save.sav");
        assert_eq!(g.screen, Screen::MainMenu);
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.screen, Screen::Flight);
        g.sim.credits = 4242.0;
        g.save_game();
        g.sim.credits = 1.0;
        assert!(g.load_game());
        assert_eq!(g.sim.credits, 4242.0);
        let _ = std::fs::remove_file(&g.save_path);
    }

    #[test]
    fn escape_opens_pause_and_back() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Pause);
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Flight);
    }

    #[test]
    fn dock_sell_undock_via_keys() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.key(KeyCode::KeyD, None, true);
        assert_eq!(g.screen, Screen::Station);
        let iron = g.data.resource_index("iron").expect("iron");
        let data = Arc::clone(&g.data);
        g.sim.ship.cargo.add(&data, iron, 500.0);
        g.sel_res = iron;
        let c0 = g.sim.credits;
        g.key(KeyCode::Enter, None, true);
        assert!(g.sim.credits > c0);
        g.key(KeyCode::KeyU, None, true);
        assert_eq!(g.screen, Screen::Flight);
    }
}
