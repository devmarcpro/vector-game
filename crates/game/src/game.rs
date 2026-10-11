//! Game state, screens, input handling and save/load glue.
use crate::{console, scene};
use std::{collections::HashSet, path::PathBuf, sync::Arc};
use vector_core::{mix64, Vec3};
use vector_data::GameData;
use vector_sim::fleet::Endpoint;
use vector_sim::{autopilot::Autopilot, Event, ShipInput, Sim, SIM_DT};
use vector_world::galaxy::StarEntry;
use vector_world::BodyKind;
use winit::keyboard::KeyCode;

pub const DEFAULT_SEED: u64 = 0x5EC7;
pub const MAIN_MENU: [&str; 6] = [
    "NOUVELLE PARTIE",
    "CONTINUER",
    "REGLES DE PARTIE",
    "OPTIONS",
    "CODEX / AIDE",
    "QUITTER",
];
pub const PAUSE_MENU: [&str; 8] = [
    "REPRENDRE",
    "SAUVEGARDER",
    "CHARGER",
    "REGLES DE PARTIE",
    "OPTIONS",
    "CODEX / AIDE",
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
    Crew,
    Recruit,
    Journal,
    Galaxy,
    Contracts,
    Bank,
    Empire,
    Colony,
    Fleet,
    Destroyed,
    Tech,
    Rules,
    Options,
    Codex,
}

/// Route being composed in the fleet screen.
#[derive(Clone, Debug)]
pub struct RouteEditor {
    pub from: usize,
    pub to: usize,
    pub res: usize,
    pub kg: f64,
    /// Field being edited: 0 from, 1 to, 2 resource, 3 lot.
    pub field: u8,
}

pub struct Game {
    pub data: Arc<GameData>,
    pub sim: Sim,
    pub stars: Vec<Vec3>,
    pub keys: HashSet<KeyCode>,
    /// Mouse look (F7): turn demand in [-1, 1] as (yaw, pitch), fed each frame by the window.
    pub mouse_look: bool,
    pub mouse: (f64, f64),
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
    pub crew_sel: usize,
    pub recruit_sel: usize,
    pub journal_scroll: usize,
    pub log_seen: usize,
    pub gal_cache: Vec<StarEntry>,
    pub gal_sel: usize,
    pub gal_zoom: f64,
    pub offers: Vec<vector_sim::economy::Contract>,
    pub contract_sel: usize,
    pub emp_sel: usize,
    pub col_idx: usize,
    /// 0 buildings, 1 storage, 2 policies, 3 build menu.
    pub col_focus: u8,
    pub b_sel: usize,
    pub s_sel: usize,
    pub p_sel: usize,
    pub build_sel: usize,
    /// 0 ships, 1 routes.
    pub fleet_focus: u8,
    pub ship_sel2: usize,
    pub route_sel: usize,
    pub endpoints: Vec<Endpoint>,
    pub editor: Option<RouteEditor>,
    pub hull_pick: Option<usize>,
    pub tech_sel: usize,
    pub rules_sel: usize,
    /// Rules for the next new game (edited live when opened from the pause menu).
    pub pending_rules: vector_sim::rules::Rules,
    /// Where the rules / options / codex screens return to.
    pub back_to: Screen,
    pub settings: crate::settings::Settings,
    pub opt_sel: usize,
    pub codex_cat: usize,
    pub codex_sel: usize,
    pub help_open: bool,
    pub tut_step: Option<usize>,
    pub tut_keys: Vec<(Screen, KeyCode)>,
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
            mouse_look: false,
            mouse: (0.0, 0.0),
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
            crew_sel: 0,
            recruit_sel: 0,
            journal_scroll: 0,
            log_seen: 0,
            gal_cache: Vec::new(),
            gal_sel: 0,
            gal_zoom: 14.0,
            offers: Vec::new(),
            contract_sel: 0,
            emp_sel: 0,
            col_idx: 0,
            col_focus: 0,
            b_sel: 0,
            s_sel: 0,
            p_sel: 0,
            build_sel: 0,
            fleet_focus: 0,
            ship_sel2: 0,
            route_sel: 0,
            endpoints: Vec::new(),
            editor: None,
            hull_pick: None,
            tech_sel: 0,
            rules_sel: 0,
            pending_rules: vector_sim::rules::Rules::default(),
            back_to: Screen::MainMenu,
            settings: crate::settings::Settings::load(&PathBuf::from("saves").join("settings.cfg")),
            opt_sel: 0,
            codex_cat: 0,
            codex_sel: 0,
            help_open: false,
            tut_step: None,
            tut_keys: Vec::new(),
            save_path: PathBuf::from("saves").join("slot1.sav"),
        };
        g.fit_map();
        g
    }

    /// Start a fresh game from a seed and go to the cockpit.
    pub fn new_game(&mut self, seed: u64) {
        self.sim = scene::new_sim(seed, Arc::clone(&self.data));
        self.sim.set_rules(self.pending_rules.clone());
        self.help_open = false;
        self.tut_step = None;
        if self.settings.tutorial {
            self.start_tutorial();
        }
        self.stars = scene::star_field(seed, 900);
        self.timescale = 1.0;
        self.keys.clear();
        self.messages = vec![format!("NOUVELLE PARTIE - GRAINE {seed}")];
        self.screen = Screen::Flight;
        self.fit_map();
        self.log_seen = self.sim.crew.log.len();
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
            pitch: (ax(KeyCode::KeyW, KeyCode::KeyS) + self.mouse.1).clamp(-1.0, 1.0),
            yaw: (ax(KeyCode::KeyD, KeyCode::KeyA) + self.mouse.0).clamp(-1.0, 1.0),
            roll: ax(KeyCode::KeyQ, KeyCode::KeyE),
            fwd: ax(KeyCode::Space, KeyCode::KeyX),
            lat: ax(KeyCode::ArrowRight, KeyCode::ArrowLeft),
            vert: ax(KeyCode::ArrowUp, KeyCode::ArrowDown),
            mine: self.keys.contains(&KeyCode::KeyV),
            fire: self.keys.contains(&KeyCode::ControlLeft)
                || self.keys.contains(&KeyCode::ControlRight)
                || self.keys.contains(&KeyCode::Enter),
            fire_missile: self.keys.contains(&KeyCode::KeyB),
            boost: self.keys.contains(&KeyCode::ShiftLeft),
            brake: !self.map && self.keys.contains(&KeyCode::KeyC),
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
            boost: self.input().boost,
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
        // Overlays (ship, crew, map...) freeze time in flight; at a station time keeps flowing.
        !self.paused
            && (self.screen == Screen::Flight
                || (self.sim.docked.is_some()
                    && !matches!(self.screen, Screen::MainMenu | Screen::Pause)))
    }

    /// Advance the simulation by `seconds` of real time at the current time scale.
    pub fn advance(&mut self, seconds: f64) {
        if self.sim.in_transit() {
            self.timescale = self.timescale.max(10_000.0);
        }
        if self.sim.combat_active() {
            self.timescale = 1.0;
        }
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
        self.forward_log();
    }

    /// Show the newest crew journal lines in the HUD message area.
    fn forward_log(&mut self) {
        let len = self.sim.crew.log.len();
        if len < self.log_seen {
            self.log_seen = len;
        }
        let fresh: Vec<String> = self.sim.crew.log[self.log_seen..]
            .iter()
            .rev()
            .take(2)
            .map(|l| l.text.to_uppercase())
            .collect();
        self.log_seen = len;
        for t in fresh.into_iter().rev() {
            self.say(t);
        }
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
                Event::JumpCharging => self.say("PROPULSEUR FTL EN CHARGE..."),
                Event::JumpStarted { dist_ly } => {
                    self.say(format!("SAUT FTL ENGAGE: {dist_ly:.1} AL"));
                    self.timescale = 10_000.0;
                }
                Event::JumpArrived { .. } => {
                    self.timescale = 1.0;
                    self.stars = scene::star_field(self.sim.system.seed, 900);
                    self.fit_map();
                    self.map = false;
                    let name = self.sim.system.name.clone();
                    self.say(format!("ARRIVEE DANS LE SYSTEME {name}"));
                }
                Event::JumpAborted(why) => {
                    self.say(format!("SAUT ANNULE: {}", why.to_uppercase()));
                    self.timescale = 1.0;
                }
                Event::JumpMishap => self.say("INCIDENT DE SAUT: DEGATS SUR LE PROPULSEUR"),
                Event::Customs { seized_kg, fine } => {
                    self.say(format!(
                        "DOUANES: {seized_kg:.0} KG DE CONTREBANDE SAISIS, AMENDE {fine:.0} CR"
                    ));
                }
                Event::ContractDone { reward } => {
                    self.say(format!("CONTRAT REMPLI: +{reward:.0} CR"))
                }
                Event::ContractExpired => self.say("UN CONTRAT A EXPIRE (-REPUTATION)"),
                Event::BankSeizure { amount } => {
                    self.say(format!("LA BANQUE SAISIT {amount:.0} CR POUR PRET IMPAYE"))
                }
                Event::PatrolScan => self.say("PATROUILLE: SCAN DE VOTRE SOUTE"),
                Event::ColonyFounded { index } => {
                    let name = self.sim.colonies[index].name.clone();
                    self.say(format!("COLONIE FONDEE : {name}"));
                }
                Event::ColonyNews(t) => self.say(t.to_uppercase()),
                Event::FleetDelivery { ship, revenue } => {
                    let n = self
                        .sim
                        .fleet
                        .get(ship)
                        .map_or(String::new(), |f| f.name.to_uppercase());
                    self.say(format!("{n}: LIVRAISON +{revenue:.0} CR"));
                }
                Event::PlayerDestroyed => {
                    self.screen = Screen::Destroyed;
                    self.timescale = 1.0;
                }
                Event::PirateDemand { amount } => {
                    self.say(format!("PIRATES : PAYEZ {amount:.0} CR [Z] OU COMBATTEZ !"));
                    self.timescale = 1.0;
                }
                Event::NpcDestroyed => self.say("VAISSEAU DETRUIT"),
                Event::BountyPaid { amount } => self.say(format!("PRIME RECUE : +{amount:.0} CR")),
                Event::CrimeCommitted => self.say("CRIME : VOUS ETES RECHERCHE DANS CE TERRITOIRE"),
                Event::PatrolHostile => self.say("LA PATROUILLE VOUS CONSIDERE COMME HOSTILE !"),
                Event::Respawned => self.say("VOTRE VAISSEAU A ETE RECONSTRUIT PAR L ASSUREUR"),
                Event::ResearchDone { tech } => {
                    let name = self.data.techs[tech].name.to_uppercase();
                    self.say(format!("RECHERCHE TERMINEE : {name}"));
                }
                Event::AnomalyExplored {
                    kind_id,
                    research,
                    credits,
                } => {
                    let label = self
                        .data
                        .anomaly_def(kind_id)
                        .map_or(kind_id.to_string(), |a| a.name.to_uppercase());
                    self.say(format!(
                        "{label} EXPLOREE : +{research:.0} RECHERCHE, +{credits:.0} CR"
                    ));
                }
                Event::RiftJump => self.say("UNE FAILLE SPATIALE VOUS A DEPLACE !"),
                Event::FlareWarning => {
                    self.say("ALERTE : ERUPTION STELLAIRE IMMINENTE - ABRITEZ-VOUS !")
                }
                Event::FlareHit { dose } => {
                    self.say(format!("ERUPTION STELLAIRE : DOSE {dose:.2} GY"))
                }
                Event::MeteorImpact { damage } => {
                    self.say(format!("IMPACT DE METEOROIDE : -{damage:.0}"))
                }
                Event::PirateAlert => self.say("ALERTE: PIRATES A PROXIMITE !"),
                Event::Overheat => self.say("ALERTE: SURCHAUFFE !"),
                Event::ModuleLost { slot } => {
                    self.say(format!("MODULE PERDU (EMPLACEMENT {})", slot + 1))
                }
                Event::Docked { body } => {
                    let name = self.sim.bodies[body].name.clone();
                    self.say(format!("AMARRE A {name}"));
                    self.timescale = 1.0;
                    if let Some(ci) = self.sim.docked_colony() {
                        self.col_idx = ci;
                        self.screen = Screen::Colony;
                    } else {
                        self.sim.market(body);
                        self.screen = Screen::Station;
                    }
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
        let top = TIME_LADDER
            .iter()
            .rposition(|&t| t <= self.sim.rules.max_timescale)
            .unwrap_or(2);
        let i = TIME_LADDER
            .iter()
            .position(|&t| t >= self.timescale)
            .unwrap_or(2);
        let j = if up {
            (i + 1).min(top)
        } else {
            i.saturating_sub(1)
        };
        self.timescale = TIME_LADDER[j.min(top)];
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
        let at = self.screen;
        self.key_inner(code, text, pressed);
        if pressed && self.screen != Screen::MainMenu {
            self.tutorial_tick(Some((at, code)));
        }
    }

    fn key_inner(&mut self, code: KeyCode, text: Option<&str>, pressed: bool) {
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
        if code == KeyCode::F1 && self.screen != Screen::MainMenu {
            self.help_open = !self.help_open;
            return;
        }
        if code == KeyCode::F10 && self.tut_step.is_some() {
            self.tut_step = None;
            self.say("TUTORIEL IGNORE (REACTIVABLE DANS OPTIONS)");
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
            Screen::Crew => self.crew_key(code),
            Screen::Recruit => self.recruit_key(code),
            Screen::Journal => self.journal_key(code),
            Screen::Galaxy => self.galaxy_key(code),
            Screen::Contracts => self.contracts_key(code),
            Screen::Bank => self.bank_key(code),
            Screen::Empire => self.empire_key(code),
            Screen::Colony => self.colony_key(code),
            Screen::Fleet => self.fleet_key(code),
            Screen::Tech => self.tech_key(code),
            Screen::Rules => self.rules_key(code),
            Screen::Options => self.options_key(code),
            Screen::Codex => self.codex_key(code),
            Screen::Destroyed => {
                if code == KeyCode::Enter {
                    if self.sim.can_respawn() {
                        self.sim.insurance_respawn();
                        self.handle_events();
                        self.screen = Screen::Flight;
                    } else {
                        self.screen = Screen::MainMenu;
                        self.menu_sel = 0;
                    }
                }
            }
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
                2 => self.open_rules(Screen::MainMenu),
                3 => self.open_options(Screen::MainMenu),
                4 => self.open_codex(Screen::MainMenu),
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
                3 => self.open_rules(Screen::Pause),
                4 => self.open_options(Screen::Pause),
                5 => self.open_codex(Screen::Pause),
                6 => {
                    self.screen = Screen::MainMenu;
                    self.menu_sel = 0;
                }
                _ => self.quit = true,
            }
        }
    }

    fn open_rules(&mut self, back: Screen) {
        self.rules_sel = 0;
        self.back_to = back;
        if back == Screen::Pause {
            self.pending_rules = self.sim.rules.clone();
        }
        self.screen = Screen::Rules;
    }

    fn rules_key(&mut self, code: KeyCode) {
        let n = vector_sim::rules::Rules::OPTION_COUNT;
        let live = self.back_to == Screen::Pause;
        match code {
            KeyCode::ArrowUp => self.rules_sel = (self.rules_sel + n - 1) % n,
            KeyCode::ArrowDown => self.rules_sel = (self.rules_sel + 1) % n,
            KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                self.pending_rules
                    .adjust(self.rules_sel, code == KeyCode::ArrowRight);
            }
            KeyCode::KeyR => self.pending_rules = vector_sim::rules::Rules::default(),
            KeyCode::Escape | KeyCode::Enter => self.screen = self.back_to,
            _ => {}
        }
        if live {
            self.sim.set_rules(self.pending_rules.clone());
            self.timescale = self.timescale.min(self.pending_rules.max_timescale);
        }
    }

    fn open_options(&mut self, back: Screen) {
        self.opt_sel = 0;
        self.back_to = back;
        self.screen = Screen::Options;
    }

    fn options_key(&mut self, code: KeyCode) {
        let n = crate::settings::Settings::COUNT;
        match code {
            KeyCode::ArrowUp => self.opt_sel = (self.opt_sel + n - 1) % n,
            KeyCode::ArrowDown => self.opt_sel = (self.opt_sel + 1) % n,
            KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                self.settings
                    .adjust(self.opt_sel, code == KeyCode::ArrowRight);
                self.settings
                    .save(&PathBuf::from("saves").join("settings.cfg"));
            }
            KeyCode::Escape | KeyCode::Enter => self.screen = self.back_to,
            _ => {}
        }
    }

    pub fn open_codex(&mut self, back: Screen) {
        self.back_to = back;
        self.screen = Screen::Codex;
    }

    fn codex_key(&mut self, code: KeyCode) {
        let nc = crate::codex::CATEGORIES.len();
        let n = crate::codex::count(&self.data, self.codex_cat).max(1);
        match code {
            KeyCode::ArrowLeft => {
                self.codex_cat = (self.codex_cat + nc - 1) % nc;
                self.codex_sel = 0;
            }
            KeyCode::ArrowRight | KeyCode::Tab => {
                self.codex_cat = (self.codex_cat + 1) % nc;
                self.codex_sel = 0;
            }
            KeyCode::ArrowUp => self.codex_sel = (self.codex_sel + n - 1) % n,
            KeyCode::ArrowDown => self.codex_sel = (self.codex_sel + 1) % n,
            KeyCode::PageUp => self.codex_sel = self.codex_sel.saturating_sub(10),
            KeyCode::PageDown => self.codex_sel = (self.codex_sel + 10).min(n - 1),
            KeyCode::Escape | KeyCode::F6 => self.screen = self.back_to,
            _ => {}
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
            KeyCode::F2 => {
                self.open_empire();
                return;
            }
            KeyCode::F3 => {
                self.open_fleet();
                return;
            }
            KeyCode::KeyN => {
                self.screen = Screen::Recruit;
                self.recruit_sel = 0;
                return;
            }
            KeyCode::KeyK => {
                self.screen = Screen::Crew;
                return;
            }
            KeyCode::F4 => {
                self.open_tech();
                return;
            }
            KeyCode::KeyC => {
                self.offers = self.sim.offered_contracts();
                self.contract_sel = 0;
                self.screen = Screen::Contracts;
                return;
            }
            KeyCode::KeyE => {
                self.screen = Screen::Bank;
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

    fn crew_key(&mut self, code: KeyCode) {
        let n = self.sim.crew.members.len();
        let docked = self.sim.docked.is_some();
        match code {
            KeyCode::ArrowUp if n > 0 => self.crew_sel = (self.crew_sel + n - 1) % n,
            KeyCode::ArrowDown if n > 0 => self.crew_sel = (self.crew_sel + 1) % n,
            KeyCode::ArrowRight | KeyCode::ArrowLeft if n > 0 => {
                let r = self.data.roles.len();
                let cur = self.sim.crew.members[self.crew_sel.min(n - 1)].role;
                let next = if code == KeyCode::ArrowRight {
                    (cur + 1) % r
                } else {
                    (cur + r - 1) % r
                };
                self.sim.set_role(self.crew_sel.min(n - 1), next);
            }
            KeyCode::KeyT if n > 0 => {
                let p = &mut self.sim.crew.members[self.crew_sel.min(n - 1)];
                p.shift = (p.shift + 1) % 3;
            }
            KeyCode::KeyX if n > 0 => match self.sim.dismiss(self.crew_sel.min(n - 1)) {
                Ok(()) => {
                    self.crew_sel = 0;
                    self.say("MEMBRE DEBARQUE");
                }
                Err(e) => self.say(format!(
                    "{} (DEBARQUEMENT A QUAI SEULEMENT)",
                    e.to_uppercase()
                )),
            },
            KeyCode::KeyJ => self.screen = Screen::Journal,
            KeyCode::KeyN if docked => {
                self.screen = Screen::Recruit;
                self.recruit_sel = 0;
            }
            KeyCode::Escape | KeyCode::KeyK => {
                self.screen = if docked {
                    Screen::Station
                } else {
                    Screen::Flight
                }
            }
            _ => {}
        }
    }

    fn recruit_key(&mut self, code: KeyCode) {
        let cands = self.sim.candidates();
        let n = cands.len().max(1);
        match code {
            KeyCode::ArrowUp => self.recruit_sel = (self.recruit_sel + n - 1) % n,
            KeyCode::ArrowDown => self.recruit_sel = (self.recruit_sel + 1) % n,
            KeyCode::Enter => {
                if let Some(c) = cands.get(self.recruit_sel) {
                    // Assign the post matching the candidate's strongest skill.
                    let best = self.data.skills[c.best_skill().0].id.clone();
                    let role = self
                        .data
                        .roles
                        .iter()
                        .position(|r| r.skill == best)
                        .unwrap_or(0);
                    let name = c.name.clone();
                    match self.sim.hire(self.recruit_sel, role) {
                        Ok(cost) => self.say(format!("{name} EMBAUCHE ({cost:.0} CR)")),
                        Err(e) => self.say(e.to_uppercase()),
                    }
                }
            }
            KeyCode::KeyK => self.screen = Screen::Crew,
            KeyCode::Escape => self.screen = Screen::Station,
            _ => {}
        }
    }

    fn journal_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::ArrowUp | KeyCode::PageUp => self.journal_scroll += 5,
            KeyCode::ArrowDown | KeyCode::PageDown => {
                self.journal_scroll = self.journal_scroll.saturating_sub(5)
            }
            KeyCode::KeyK => self.screen = Screen::Crew,
            KeyCode::Escape | KeyCode::KeyJ => {
                self.screen = if self.sim.docked.is_some() {
                    Screen::Station
                } else {
                    Screen::Flight
                }
            }
            _ => {}
        }
    }

    fn open_empire(&mut self) {
        self.emp_sel = 0;
        self.screen = Screen::Empire;
    }

    fn open_fleet(&mut self) {
        self.endpoints = self.sim.known_endpoints();
        self.fleet_focus = 0;
        self.editor = None;
        self.hull_pick = None;
        self.screen = Screen::Fleet;
    }

    /// Found a colony on the targeted body with the kit in the hold.
    fn found_colony_at_target(&mut self) {
        let Some(t) = self.sim.target else {
            self.say("CIBLEZ D ABORD UN CORPS A COLONISER");
            return;
        };
        match self.sim.found_colony(t, None) {
            Ok(_) => self.handle_events(),
            Err(e) => self.say(format!("COLONISATION IMPOSSIBLE: {}", e.to_uppercase())),
        }
    }

    fn empire_key(&mut self, code: KeyCode) {
        let n = self.sim.colonies.len();
        match code {
            KeyCode::ArrowUp if n > 0 => self.emp_sel = (self.emp_sel + n - 1) % n,
            KeyCode::ArrowDown if n > 0 => self.emp_sel = (self.emp_sel + 1) % n,
            KeyCode::Enter if n > 0 => {
                self.col_idx = self.emp_sel.min(n - 1);
                self.col_focus = 0;
                self.screen = Screen::Colony;
            }
            KeyCode::F3 => self.open_fleet(),
            KeyCode::Escape | KeyCode::F2 => {
                self.screen = if self.sim.docked.is_some() {
                    self.dock_screen()
                } else {
                    Screen::Flight
                }
            }
            _ => {}
        }
    }

    /// The screen to return to while docked.
    fn dock_screen(&self) -> Screen {
        if self.sim.docked_colony().is_some() {
            Screen::Colony
        } else {
            Screen::Station
        }
    }

    fn colony_key(&mut self, code: KeyCode) {
        let ci = self.col_idx;
        if ci >= self.sim.colonies.len() {
            self.screen = Screen::Empire;
            return;
        }
        let here = self.sim.docked_colony() == Some(ci);
        let nb = self.sim.colonies[ci].buildings.len();
        let nres = self.data.resources.len();
        let np = self.data.policies.len();
        match code {
            KeyCode::Tab => self.col_focus = (self.col_focus + 1) % 4,
            KeyCode::ArrowUp => match self.col_focus {
                0 if nb > 0 => self.b_sel = (self.b_sel + nb - 1) % nb,
                1 => self.s_sel = (self.s_sel + nres - 1) % nres,
                2 => self.p_sel = (self.p_sel + np - 1) % np,
                _ => {
                    self.build_sel =
                        (self.build_sel + self.data.buildings.len() - 1) % self.data.buildings.len()
                }
            },
            KeyCode::ArrowDown => match self.col_focus {
                0 if nb > 0 => self.b_sel = (self.b_sel + 1) % nb,
                1 => self.s_sel = (self.s_sel + 1) % nres,
                2 => self.p_sel = (self.p_sel + 1) % np,
                _ => self.build_sel = (self.build_sel + 1) % self.data.buildings.len(),
            },
            KeyCode::ArrowLeft | KeyCode::ArrowRight if self.col_focus == 2 => {
                let len = self.data.policies[self.p_sel].levels.len();
                let cur = self.sim.colonies[ci].policies[self.p_sel];
                let next = if code == KeyCode::ArrowRight {
                    (cur + 1).min(len - 1)
                } else {
                    cur.saturating_sub(1)
                };
                let _ = self.sim.colony_set_policy(ci, self.p_sel, next);
            }
            KeyCode::Space if self.col_focus == 0 && nb > 0 => {
                let b = &mut self.sim.colonies[ci].buildings[self.b_sel.min(nb - 1)];
                b.enabled = !b.enabled;
            }
            KeyCode::KeyR if self.col_focus == 0 && nb > 0 => {
                let b = &mut self.sim.colonies[ci].buildings[self.b_sel.min(nb - 1)];
                b.recipe += 1;
            }
            KeyCode::KeyX if self.col_focus == 0 && nb > 0 => {
                let k = self.b_sel.min(nb - 1);
                match self.sim.colony_demolish(ci, k) {
                    Ok(()) => self.say("BATIMENT DEMOLI"),
                    Err(e) => self.say(e.to_uppercase()),
                }
                self.b_sel = 0;
            }
            KeyCode::KeyD if self.col_focus == 1 => {
                match self.sim.colony_deposit(ci, self.s_sel, None) {
                    Ok(kg) => self.say(format!("DEPOSE {kg:.0} KG")),
                    Err(e) => self.say(e.to_uppercase()),
                }
            }
            KeyCode::KeyW if self.col_focus == 1 => {
                match self.sim.colony_withdraw(ci, self.s_sel, None) {
                    Ok(kg) => self.say(format!("RETIRE {kg:.0} KG")),
                    Err(e) => self.say(e.to_uppercase()),
                }
            }
            KeyCode::Enter if self.col_focus == 3 => {
                match self.sim.colony_build(ci, self.build_sel) {
                    Ok(()) => self.say("CONSTRUCTION LANCEE"),
                    Err(e) => self.say(e.to_uppercase()),
                }
            }
            KeyCode::KeyT => match self.sim.colony_withdraw_treasury(ci) {
                Ok(c) => self.say(format!("TRESORERIE RECUPEREE: {c:.0} CR")),
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::KeyU if here => {
                self.sim.undock();
                self.handle_events();
            }
            KeyCode::F3 => self.open_fleet(),
            KeyCode::Escape | KeyCode::F2 => self.screen = Screen::Empire,
            _ => {}
        }
        let _ = here;
    }

    fn fleet_key(&mut self, code: KeyCode) {
        // Hull picker (docked at a station).
        if let Some(sel) = self.hull_pick {
            let n = self.data.hulls.len();
            match code {
                KeyCode::ArrowUp => self.hull_pick = Some((sel + n - 1) % n),
                KeyCode::ArrowDown => self.hull_pick = Some((sel + 1) % n),
                KeyCode::Enter => {
                    match self.sim.buy_fleet_ship(sel) {
                        Ok(_) => self.say("VAISSEAU ACHETE"),
                        Err(e) => self.say(e.to_uppercase()),
                    }
                    self.hull_pick = None;
                }
                KeyCode::Escape => self.hull_pick = None,
                _ => {}
            }
            return;
        }
        // Route editor.
        if let Some(mut ed) = self.editor.clone() {
            let ne = self.endpoints.len().max(1);
            let nr = self.data.resources.len();
            let step =
                |cur: usize, n: usize, up: bool| if up { (cur + 1) % n } else { (cur + n - 1) % n };
            match code {
                KeyCode::Tab | KeyCode::ArrowDown => ed.field = (ed.field + 1) % 4,
                KeyCode::ArrowUp => ed.field = (ed.field + 3) % 4,
                KeyCode::ArrowRight | KeyCode::ArrowLeft => {
                    let up = code == KeyCode::ArrowRight;
                    match ed.field {
                        0 => ed.from = step(ed.from, ne, up),
                        1 => ed.to = step(ed.to, ne, up),
                        2 => ed.res = step(ed.res, nr, up),
                        _ => {
                            ed.kg =
                                (ed.kg + if up { 1000.0 } else { -1000.0 }).clamp(500.0, 200_000.0)
                        }
                    }
                }
                KeyCode::Enter => {
                    if let (Some(a), Some(b)) = (
                        self.endpoints.get(ed.from).cloned(),
                        self.endpoints.get(ed.to).cloned(),
                    ) {
                        match self.sim.create_route(a, b, ed.res, ed.kg) {
                            Ok(_) => self.say("ROUTE CREEE"),
                            Err(e) => self.say(e.to_uppercase()),
                        }
                    }
                    self.editor = None;
                    return;
                }
                KeyCode::Escape => {
                    self.editor = None;
                    return;
                }
                _ => {}
            }
            self.editor = Some(ed);
            return;
        }
        let (ns, nr) = (self.sim.fleet.len(), self.sim.routes.len());
        match code {
            KeyCode::Tab => self.fleet_focus = 1 - self.fleet_focus,
            KeyCode::ArrowUp => {
                if self.fleet_focus == 0 && ns > 0 {
                    self.ship_sel2 = (self.ship_sel2 + ns - 1) % ns;
                } else if nr > 0 {
                    self.route_sel = (self.route_sel + nr - 1) % nr;
                }
            }
            KeyCode::ArrowDown => {
                if self.fleet_focus == 0 && ns > 0 {
                    self.ship_sel2 = (self.ship_sel2 + 1) % ns;
                } else if nr > 0 {
                    self.route_sel = (self.route_sel + 1) % nr;
                }
            }
            KeyCode::KeyN => {
                if self.endpoints.len() < 2 {
                    self.say("IL FAUT AU MOINS DEUX POINTS (COLONIES / STATIONS)");
                } else {
                    self.editor = Some(RouteEditor {
                        from: 0,
                        to: 1,
                        res: 0,
                        kg: 5000.0,
                        field: 0,
                    });
                }
            }
            KeyCode::KeyA if ns > 0 && nr > 0 => match self
                .sim
                .assign_route(self.ship_sel2.min(ns - 1), self.route_sel.min(nr - 1))
            {
                Ok(()) => self.say("VAISSEAU AFFECTE A LA ROUTE"),
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::KeyU if ns > 0 => self.sim.unassign_ship(self.ship_sel2.min(ns - 1)),
            KeyCode::KeyX => {
                if self.fleet_focus == 1 && nr > 0 {
                    match self.sim.delete_route(self.route_sel.min(nr - 1)) {
                        Ok(()) => self.route_sel = 0,
                        Err(e) => self.say(e.to_uppercase()),
                    }
                } else if ns > 0 {
                    match self.sim.sell_fleet_ship(self.ship_sel2.min(ns - 1)) {
                        Ok(v) => {
                            self.say(format!("VAISSEAU REVENDU {v:.0} CR"));
                            self.ship_sel2 = 0;
                        }
                        Err(e) => self.say(e.to_uppercase()),
                    }
                }
            }
            KeyCode::KeyB => {
                if self.sim.docked_station().is_ok() {
                    self.hull_pick = Some(0);
                } else {
                    self.say("ACHAT DE VAISSEAUX : A QUAI D UNE STATION");
                }
            }
            KeyCode::F2 => self.open_empire(),
            KeyCode::Escape | KeyCode::F3 => {
                self.screen = if self.sim.docked.is_some() {
                    self.dock_screen()
                } else {
                    Screen::Flight
                }
            }
            _ => {}
        }
    }

    /// Tech indices in display order: by field, then by cost.
    pub fn tech_order(&self) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.data.techs.len()).collect();
        v.sort_by(|&a, &b| {
            let (ta, tb) = (&self.data.techs[a], &self.data.techs[b]);
            ta.field
                .cmp(&tb.field)
                .then(ta.cost.total_cmp(&tb.cost))
                .then(ta.id.cmp(&tb.id))
        });
        v
    }

    fn open_tech(&mut self) {
        self.tech_sel = 0;
        self.screen = Screen::Tech;
    }

    fn tech_key(&mut self, code: KeyCode) {
        let order = self.tech_order();
        let n = order.len();
        match code {
            KeyCode::ArrowUp => self.tech_sel = (self.tech_sel + n - 1) % n,
            KeyCode::ArrowDown => self.tech_sel = (self.tech_sel + 1) % n,
            KeyCode::PageUp => self.tech_sel = self.tech_sel.saturating_sub(10),
            KeyCode::PageDown => self.tech_sel = (self.tech_sel + 10).min(n - 1),
            KeyCode::Enter => {
                let t = order[self.tech_sel.min(n - 1)];
                match self.sim.start_research(t) {
                    Ok(()) => {
                        let name = self.data.techs[t].name.to_uppercase();
                        self.say(format!("RECHERCHE LANCEE : {name}"));
                    }
                    Err(e) => self.say(e.to_uppercase()),
                }
            }
            KeyCode::Escape | KeyCode::F4 => {
                self.screen = if self.sim.docked.is_some() {
                    self.dock_screen()
                } else {
                    Screen::Flight
                }
            }
            _ => {}
        }
    }

    fn contracts_key(&mut self, code: KeyCode) {
        let total = self.offers.len() + self.sim.contracts.len();
        match code {
            KeyCode::ArrowUp if total > 0 => {
                self.contract_sel = (self.contract_sel + total - 1) % total
            }
            KeyCode::ArrowDown if total > 0 => self.contract_sel = (self.contract_sel + 1) % total,
            KeyCode::Enter if total > 0 => {
                let sel = self.contract_sel.min(total - 1);
                if sel < self.offers.len() {
                    let c = self.offers[sel].clone();
                    match self.sim.accept_contract(c) {
                        Ok(()) => {
                            self.say("CONTRAT ACCEPTE");
                            self.offers = self.sim.offered_contracts();
                            self.contract_sel = 0;
                        }
                        Err(e) => self.say(e.to_uppercase()),
                    }
                } else {
                    match self.sim.deliver_contract(sel - self.offers.len()) {
                        Ok(_) => {
                            self.handle_events();
                            self.contract_sel = 0;
                        }
                        Err(e) => self.say(e.to_uppercase()),
                    }
                }
            }
            KeyCode::KeyX if total > 0 => {
                let sel = self.contract_sel.min(total - 1);
                if sel >= self.offers.len() {
                    let c = self.sim.contracts.remove(sel - self.offers.len());
                    self.sim.change_reputation(c.faction, -1.0);
                    self.say("CONTRAT ABANDONNE");
                    self.contract_sel = 0;
                }
            }
            KeyCode::Escape => self.screen = Screen::Station,
            _ => {}
        }
    }

    fn bank_key(&mut self, code: KeyCode) {
        let r = match code {
            KeyCode::ArrowUp | KeyCode::Equal | KeyCode::NumpadAdd => self
                .sim
                .borrow(1000.0)
                .map(|()| "EMPRUNT DE 1000 CR".to_string()),
            KeyCode::ArrowDown | KeyCode::Minus | KeyCode::NumpadSubtract => self
                .sim
                .repay(1000.0)
                .map(|p| format!("REMBOURSE {p:.0} CR")),
            KeyCode::Enter => self
                .sim
                .repay(f64::MAX)
                .map(|p| format!("PRET REMBOURSE ({p:.0} CR)")),
            KeyCode::Escape | KeyCode::KeyE => {
                self.screen = Screen::Station;
                return;
            }
            _ => return,
        };
        match r {
            Ok(m) => self.say(m),
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    /// Refresh the cached list of stars shown on the galactic map.
    pub fn open_galaxy(&mut self) {
        let here = self.sim.here;
        self.gal_cache = self
            .sim
            .galaxy
            .nearby(self.sim.here_pos, 45.0)
            .into_iter()
            .filter(|e| e.id != here && self.sim.is_seen(e.pos))
            .collect();
        self.gal_sel = 0;
        self.screen = Screen::Galaxy;
    }

    fn galaxy_key(&mut self, code: KeyCode) {
        let n = self.gal_cache.len().max(1);
        match code {
            KeyCode::ArrowUp => self.gal_sel = (self.gal_sel + n - 1) % n,
            KeyCode::ArrowDown | KeyCode::Tab => self.gal_sel = (self.gal_sel + 1) % n,
            KeyCode::Equal | KeyCode::NumpadAdd => self.gal_zoom = (self.gal_zoom * 1.4).min(80.0),
            KeyCode::Minus | KeyCode::NumpadSubtract => {
                self.gal_zoom = (self.gal_zoom / 1.4).max(2.0)
            }
            KeyCode::Enter => {
                if let Some(target) = self.gal_cache.get(self.gal_sel).cloned() {
                    match self.sim.start_jump(&target) {
                        Ok(()) => {
                            self.screen = Screen::Flight;
                            self.handle_events();
                        }
                        Err(e) => self.say(format!("SAUT IMPOSSIBLE: {}", e.to_uppercase())),
                    }
                }
            }
            KeyCode::Escape | KeyCode::KeyL => self.screen = Screen::Flight,
            _ => {}
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
            KeyCode::Tab => {
                self.sim.target_npc = None;
                self.cycle_target(|_| true);
            }
            KeyCode::KeyR => {
                let list = self.sim.visible_npcs();
                if list.is_empty() {
                    self.say("AUCUN VAISSEAU A PORTEE");
                } else {
                    let next = match self
                        .sim
                        .target_npc
                        .and_then(|t| list.iter().position(|&i| i == t))
                    {
                        Some(k) => list[(k + 1) % list.len()],
                        None => list[0],
                    };
                    self.sim.target_npc = Some(next);
                }
            }
            KeyCode::KeyT => self.cycle_target(|k| matches!(k, BodyKind::Asteroid(_))),
            KeyCode::KeyY => self.cycle_target(|k| matches!(k, BodyKind::Station)),
            KeyCode::KeyP => self.paused = !self.paused,
            KeyCode::PageUp => self.step_time(true),
            KeyCode::PageDown => self.step_time(false),
            KeyCode::KeyG => self.toggle_autopilot(),
            KeyCode::F7 => {
                self.mouse_look = !self.mouse_look;
                self.mouse = (0.0, 0.0);
                self.say(if self.mouse_look {
                    "VISEE SOURIS ACTIVEE (F7 POUR LA COUPER)"
                } else {
                    "VISEE SOURIS COUPEE"
                });
            }
            KeyCode::KeyM => self.map = !self.map,
            KeyCode::KeyC => self.map_on_ship = !self.map_on_ship,
            KeyCode::KeyI => self.screen = Screen::Inventory,
            KeyCode::KeyH => self.open_ship(),
            KeyCode::KeyK => self.screen = Screen::Crew,
            KeyCode::KeyJ => self.screen = Screen::Journal,
            KeyCode::KeyD => match self.sim.try_dock() {
                Ok(_) => self.handle_events(),
                // D also steers (yaw): stay quiet when no station is anywhere near.
                Err(e) if e.starts_with("aucune") || e.starts_with("trop loin") => {}
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
                self.say("MAJ:BOOST C(MAINTENIR):FREIN SUR LA CIBLE F7:VISEE SOURIS");
                self.say("TAB/T/Y/R:CIBLE G:AUTO D:AMARRER I:SOUTE M:CARTE F5/F9:SAUV/CHARG");
                self.say(
                    "F2:EMPIRE F3:FLOTTE O:COLONISER LA CIBLE L:GALAXIE K:EQUIPAGE H:VAISSEAU",
                );
            }
            KeyCode::KeyL => self.open_galaxy(),
            KeyCode::F2 => self.open_empire(),
            KeyCode::F3 => self.open_fleet(),
            KeyCode::KeyZ => match self.sim.pay_tribute() {
                Ok(a) => self.say(format!("TRIBUT PAYE: {a:.0} CR")),
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::KeyN => match self.sim.salvage() {
                Ok(kg) => self.say(format!("EPAVE RECUPEREE: {kg:.0} KG")),
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::KeyU => match self.sim.board() {
                Ok(vector_sim::combat::BoardOutcome::Captured { loot_kg }) => {
                    self.say(format!("ABORDAGE REUSSI: {loot_kg:.0} KG DE BUTIN"))
                }
                Ok(vector_sim::combat::BoardOutcome::Repelled { casualties }) => {
                    self.say(format!("ABORDAGE REPOUSSE: {casualties} PERTE(S)"))
                }
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::KeyO => self.found_colony_at_target(),
            KeyCode::F4 => self.open_tech(),
            KeyCode::F6 => self.open_codex(Screen::Flight),
            KeyCode::Backspace if self.sim.is_charging_jump() => {
                self.sim.cancel_jump();
                self.handle_events();
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

    /// Every documented flight hotkey must lead somewhere (guards against mis-wired match arms).
    #[test]
    fn flight_hotkeys_open_their_screens() {
        let cases = [
            (KeyCode::KeyL, Screen::Galaxy),
            (KeyCode::F2, Screen::Empire),
            (KeyCode::F3, Screen::Fleet),
            (KeyCode::F4, Screen::Tech),
            (KeyCode::KeyK, Screen::Crew),
            (KeyCode::KeyJ, Screen::Journal),
            (KeyCode::KeyH, Screen::Ship),
            (KeyCode::KeyI, Screen::Inventory),
            (KeyCode::Escape, Screen::Pause),
        ];
        for (key, expected) in cases {
            let mut g = Game::new(data(), 1);
            g.key(KeyCode::Enter, None, true);
            assert_eq!(g.screen, Screen::Flight);
            g.key(key, None, true);
            assert_eq!(g.screen, expected, "{key:?}");
        }
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.key(KeyCode::KeyM, None, true);
        assert!(g.map, "M toggles the system map");
        g.key(KeyCode::KeyF, None, true);
        assert!(!g.sim.ship.assist, "F toggles flight assist");
        g.key(KeyCode::Tab, None, true);
        assert!(g.sim.target.is_some(), "Tab selects a target");
        g.key(KeyCode::KeyG, None, true);
        assert!(
            g.sim.autopilot.is_some() || g.messages.last().is_some_and(|m| m.contains("REFUSE")),
            "G engages or explains"
        );
        g.key(KeyCode::PageUp, None, true);
        assert!(g.timescale > 1.0, "PageUp speeds time up");
    }

    #[test]
    fn mouse_look_boost_and_brake_reach_the_ship_input() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.screen, Screen::Flight);
        g.mouse = (0.6, -0.3);
        let i = g.input();
        assert!((i.yaw - 0.6).abs() < 1e-9 && (i.pitch + 0.3).abs() < 1e-9);
        g.key(KeyCode::KeyD, None, true);
        g.mouse = (0.9, 0.0);
        assert_eq!(g.input().yaw, 1.0, "keyboard and mouse add, then clamp");
        g.key(KeyCode::KeyD, None, false);
        g.key(KeyCode::F7, None, true);
        assert!(g.mouse_look, "F7 toggles mouse look");
        assert_eq!(g.mouse, (0.0, 0.0), "toggling clears stale motion");
        assert!(!g.input().boost && !g.input().brake);
        g.key(KeyCode::ShiftLeft, None, true);
        g.key(KeyCode::KeyC, None, true);
        assert!(g.input().boost && g.input().brake);
    }

    #[test]
    fn menus_reach_rules_options_and_codex() {
        let mut g = Game::new(data(), 1);
        // Main menu: REGLES (index 2) -> raise damage x -> back -> new game uses it.
        g.key(KeyCode::ArrowDown, None, true);
        g.key(KeyCode::ArrowDown, None, true);
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.screen, Screen::Rules);
        g.key(KeyCode::ArrowDown, None, true);
        g.key(KeyCode::ArrowDown, None, true);
        g.key(KeyCode::ArrowDown, None, true);
        g.key(KeyCode::ArrowRight, None, true);
        assert!(g.pending_rules.damage > 1.0);
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::MainMenu);
        g.menu_sel = 0;
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.screen, Screen::Flight);
        assert!(g.sim.rules.is_modified() && g.sim.rules.damage > 1.0);
        // Pause: options and codex open and come back.
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Pause);
        g.menu_sel = 4;
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.screen, Screen::Options);
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Pause);
        g.key(KeyCode::Escape, None, true);
        g.key(KeyCode::F6, None, true);
        assert_eq!(g.screen, Screen::Codex);
        g.key(KeyCode::ArrowRight, None, true);
        assert_eq!(g.codex_cat, 1);
        g.key(KeyCode::F6, None, true);
        assert_eq!(g.screen, Screen::Flight);
    }

    #[test]
    fn tutorial_advances_on_actions_and_can_be_skipped() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.tut_step, Some(0), "new games start the tutorial");
        g.key(KeyCode::KeyW, None, true);
        assert_eq!(g.tut_step, Some(1), "rotating completes step 1");
        g.key(KeyCode::Space, None, true);
        g.key(KeyCode::KeyF, None, true);
        assert_eq!(g.tut_step, Some(3));
        g.key(KeyCode::F10, None, true);
        assert_eq!(g.tut_step, None);
    }

    #[test]
    fn f1_toggles_contextual_help_on_every_screen() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.key(KeyCode::F1, None, true);
        assert!(g.help_open);
        g.key(KeyCode::F1, None, true);
        assert!(!g.help_open);
        for screen in [
            Screen::Flight,
            Screen::Station,
            Screen::Inventory,
            Screen::Ship,
            Screen::Shipyard,
            Screen::Crew,
            Screen::Recruit,
            Screen::Journal,
            Screen::Galaxy,
            Screen::Contracts,
            Screen::Bank,
            Screen::Empire,
            Screen::Colony,
            Screen::Fleet,
            Screen::Tech,
            Screen::Rules,
            Screen::Options,
            Screen::Codex,
            Screen::Pause,
            Screen::MainMenu,
            Screen::Destroyed,
        ] {
            assert!(
                !crate::tutorial::help_lines(screen).is_empty(),
                "{screen:?}"
            );
        }
    }

    #[test]
    fn max_timescale_rule_caps_acceleration() {
        let mut g = Game::new(data(), 1);
        g.pending_rules.max_timescale = 10.0;
        g.key(KeyCode::Enter, None, true);
        for _ in 0..12 {
            g.key(KeyCode::PageUp, None, true);
        }
        assert_eq!(g.timescale, 10.0);
    }

    #[test]
    fn permadeath_returns_to_the_main_menu() {
        let mut g = Game::new(data(), 1);
        g.pending_rules.permadeath = true;
        g.key(KeyCode::Enter, None, true);
        g.screen = Screen::Destroyed;
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.screen, Screen::MainMenu);
    }

    #[test]
    fn station_hotkeys_open_their_screens() {
        let cases = [
            (KeyCode::KeyH, Screen::Ship),
            (KeyCode::KeyY, Screen::Shipyard),
            (KeyCode::KeyN, Screen::Recruit),
            (KeyCode::KeyK, Screen::Crew),
            (KeyCode::KeyC, Screen::Contracts),
            (KeyCode::KeyE, Screen::Bank),
            (KeyCode::F2, Screen::Empire),
            (KeyCode::F3, Screen::Fleet),
        ];
        for (key, expected) in cases {
            let mut g = Game::new(data(), 1);
            g.key(KeyCode::Enter, None, true);
            let (st, _) = g.sim.nearest_station().expect("station");
            g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
            g.key(KeyCode::KeyD, None, true);
            assert_eq!(g.screen, Screen::Station);
            g.key(key, None, true);
            assert_eq!(g.screen, expected, "{key:?}");
        }
    }

    #[test]
    fn firing_and_utility_keys_reach_the_simulation() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.key(KeyCode::ControlLeft, None, true);
        assert!(g.input().fire);
        g.key(KeyCode::ControlLeft, None, false);
        assert!(!g.input().fire);
        g.key(KeyCode::KeyB, None, true);
        assert!(g.input().fire_missile);
        g.key(KeyCode::KeyB, None, false);
        g.key(KeyCode::KeyZ, None, true);
        assert!(
            g.messages
                .last()
                .is_some_and(|m| m.contains("TRIBUT") || m.contains("PERSONNE")),
            "Z reports on tribute"
        );
        g.key(KeyCode::KeyN, None, true);
        assert!(
            g.messages.last().is_some_and(|m| m.contains("EPAVE")),
            "N reports on salvage"
        );
        g.key(KeyCode::KeyU, None, true);
        assert!(
            g.messages
                .last()
                .is_some_and(|m| m.contains("CIBLE") || m.contains("ABORDAGE")),
            "U reports on boarding"
        );
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
