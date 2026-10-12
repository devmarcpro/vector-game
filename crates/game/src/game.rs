//! Game state, screens, input handling and save/load glue.
use crate::{console, scene};
use std::{collections::HashSet, path::PathBuf, sync::Arc};
use vector_core::{mix64, Vec3};
use vector_data::GameData;
use vector_sim::fleet::Endpoint;
use vector_sim::{Event, ShipInput, Sim, SIM_DT};
use vector_world::galaxy::StarEntry;
use vector_world::BodyKind;
use winit::keyboard::KeyCode;

pub const DEFAULT_SEED: u64 = 0x5EC7;
pub const MAIN_MENU: [&str; 7] = [
    "PARTIE RAPIDE",
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

/// Reagents of one batch in the laboratory (codex, REACTIONS), kg.
pub const LAB_BATCH_KG: f64 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    MainMenu,
    Flight,
    Station,
    Inventory,
    Pause,
    Ship,
    Shipyard,
    Designer,
    Crew,
    Recruit,
    Journal,
    Galaxy,
    Contracts,
    Bank,
    Livestock,
    Casino,
    Trading,
    Cosmos,
    Rogue,
    Empire,
    Colony,
    Fleet,
    Destroyed,
    Tech,
    Rules,
    Options,
    Codex,
    Surface,
    Bourse,
    Family,
    Factions,
    Proto,
    Dialogue,
    Expedition,
    /// The new-game flow (scenario, ship, crew, galaxy, options).
    NewGame,
    /// The one menu: the eight spaces that gather the screens.
    Hub,
    /// The dashboard: the whole state of the game in nine panels (see `dash_view`).
    Dashboard,
    /// The navigation computer: the queue of orders, the planner, the thresholds (see `nav_view`).
    NavAuto,
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
    /// Pointer position and the clickable zones of the last frame (see `mouse`).
    pub pointer: crate::mouse::Pointer,
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
    /// The ship designer tab of the shipyard (see `designer_view`).
    pub dz: crate::designer_view::DesignerUi,
    pub crew_sel: usize,
    pub recruit_sel: usize,
    pub journal_scroll: usize,
    /// 0 the log of the ship, 1 the news of the world, 2 the archives (history and finds).
    pub journal_tab: u8,
    pub arch_sel: usize,
    /// Page of the technology screen that shows the mind of the ship, and its selected row.
    pub tech_mind: bool,
    pub mind_sel: usize,
    pub log_seen: usize,
    pub gal_cache: Vec<StarEntry>,
    /// Nebulae, clusters and remnants within reach of the galactic map (see `vector_world::cosmos`).
    pub gal_features: Vec<vector_world::cosmos::Feature>,
    pub gal_sel: usize,
    /// The screen of the navigation computer and the editor of its orders.
    pub nav_ui: crate::nav_view::NavUi,
    pub gal_zoom: f64,
    pub offers: Vec<vector_sim::economy::Contract>,
    pub contract_sel: usize,
    pub emp_sel: usize,
    pub col_idx: usize,
    /// 0 buildings, 1 storage, 2 policies, 3 build menu.
    pub col_focus: u8,
    /// Colony screen: production-chain diagram instead of the lists.
    pub col_flow: bool,
    /// Colony screen: health tracking (epidemics) instead of the lists.
    pub col_health: bool,
    /// Colony screen: 0 the lists, 1 industry, 2 farming, 3 the city, 4 logistics (`works_view`).
    pub col_tab: u8,
    /// Selected building of the industry tab, destination colony and goods of the logistics tab.
    pub tab_sel: usize,
    pub ship_to: usize,
    pub ship_res: usize,
    /// Colony screen: projects (terraforming, megastructures) instead of the lists.
    pub col_proj: bool,
    /// Projects screen: zone with the keys (0 works, 1 catalogue, 2 plants), selections, units
    /// step, seeded species (index among producers).
    pub proj_focus: u8,
    pub proj_sel: usize,
    pub cat_sel: usize,
    pub proj_units: usize,
    pub proj_species: usize,
    pub proc_sel: usize,
    pub fac_sel: usize,
    /// Factions screen: the registry of relations (every entity) instead of the ring.
    pub rel_page: bool,
    /// Factions screen: the society page (S).
    pub soc_page: bool,
    pub soc_who: usize,
    pub soc_study: usize,
    pub soc_art: usize,
    pub soc_sport: usize,
    pub rel_sel: usize,
    /// Factions screen: the war page (fronts, sieges, peace, mercenaries, spies).
    pub war_page: bool,
    pub war_agent: usize,
    pub war_target: usize,
    pub war_cells: Vec<vector_sim::politics::Cell>,
    /// Crew screen: relations network instead of the table.
    pub crew_net: bool,
    /// Crew screen: the organ list of the selected person has the keys.
    pub organ_mode: bool,
    /// Crew screen: the MEDECINE tab (silhouette, wounds, drugs, care).
    pub med_mode: bool,
    pub med_drug: usize,
    pub med_wound: usize,
    /// Crew screen: the equipment list of the selected person has the keys.
    pub gear_mode: bool,
    pub gear_sel: usize,
    pub organ_sel: usize,
    /// The piloting menu column is unfolded.
    pub flight_menu_open: bool,
    /// Third-person view of the ship (SUPPR): orbit angles, distance in ship lengths, chase or free.
    pub ext_view: bool,
    pub ext_yaw: f64,
    pub ext_pitch: f64,
    pub ext_zoom: f64,
    pub ext_chase: bool,
    /// Expedition: who goes (per crew index), the rover, the selected row, and the walk itself.
    pub exp_party: Vec<bool>,
    pub exp_vehicle: bool,
    pub exp_sel: usize,
    /// An exploration started from the surface map: closing it goes back there.
    pub rogue_from_surface: bool,
    /// A screen was opened from a terminal of a walk: Escape comes back to the walk.
    pub rogue_visit: bool,
    /// Radio conversation: who, where in the tree, what just happened, the lines so far.
    pub dlg_npc: usize,
    pub dlg_node: String,
    pub dlg_result: String,
    pub dlg_log: Vec<String>,
    pub dlg_sel: usize,
    /// Implant type chosen in the organ list (Left/Right).
    pub cyber_sel: usize,
    /// Interface prototypes (F12): which one, and the implant slot picked in the body one.
    pub proto: u8,
    pub proto_slot: usize,
    /// Fleet screen: map of colonies and routes instead of the lists.
    pub fleet_map: bool,
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
    /// Entries visited before the current one (Backspace goes back).
    pub codex_back: Vec<(usize, usize)>,
    /// Surface map screen (F8): the map, cursor, layer, globe view and its rotation.
    pub surf: Option<vector_sim::planet::PlanetMap>,
    /// Maps of the planets of the system, built when first seen from the cockpit (drawn as
    /// topographic globes): the system they belong to, and one per body.
    pub flight_maps: std::cell::RefCell<(
        u64,
        std::collections::HashMap<usize, std::rc::Rc<vector_sim::planet::PlanetMap>>,
    )>,
    pub surf_cx: usize,
    pub surf_cy: usize,
    pub surf_layer: usize,
    /// Where the map was drawn (column, row, columns, rows), for clicks.
    pub surf_geom: std::cell::Cell<(usize, usize, usize, usize)>,
    pub surf_globe: bool,
    pub surf_spin: f64,
    /// Cities of this world's colonies: (name, cell x, cell y), computed when the screen opens.
    pub surf_cities: Vec<(String, usize, usize)>,
    /// The people of the surveyed world, if any (computed when the surface opens).
    pub surf_civ: Option<vector_sim::civ::Civ>,
    /// Food web of the world on the surface screen (settled balance, or the colony's own).
    pub surf_eco: Option<vector_sim::ecology::Ecosystem>,
    /// BIOSPHERE layer: position of the shown species in `surface_view::alive_species`, whether
    /// the full card of its local variant is open, and the populations of the world with the day
    /// they were computed for.
    pub surf_species: usize,
    pub surf_card: bool,
    pub surf_variants: std::cell::RefCell<(i64, Vec<vector_sim::biosphere::Variant>)>,
    /// Exchange screen: station, selected resource, range (index into `BOURSE_RANGES`), the
    /// price series `[resource][day]` and the world events of the period.
    pub bourse_station: usize,
    pub bourse_sel: usize,
    pub bourse_range: usize,
    pub bourse_series: Vec<Vec<f64>>,
    /// Exchange tab (0 goods, 1 companies), the company chosen, and their price series.
    pub bourse_tab: u8,
    pub comp_sel: usize,
    pub farm: crate::farm_view::FarmUi,
    pub trade_ui: crate::trade_view::TradeUi,
    pub cosmos_ui: crate::cosmos_view::CosmosUi,
    pub rogue_ui: crate::rogue_view::RogueUi,
    /// First-person view of the exploration, and the way the squad faces (eighths of a circle).
    pub fpv: bool,
    pub fpv_dir: i32,
    pub comp_series: Vec<Vec<f64>>,
    pub bourse_live: Vec<f64>,
    pub bourse_news: Vec<vector_sim::bourse::ActiveEvent>,
    /// Family screen: the crew member shown and their pedigree.
    pub family_member: usize,
    /// 0: the tree, 1: the genes.
    pub family_tab: usize,
    pub family_ped: Option<vector_sim::genealogy::Pedigree>,
    pub help_open: bool,
    /// The cockpit looks astern (at the ground of a ship landing tail first).
    pub rear_view: bool,
    pub tut_step: Option<usize>,
    pub tut_keys: Vec<(Screen, KeyCode)>,
    pub save_path: PathBuf,
    /// New-game flow and main menu (see `newgame` and `hub`).
    pub ng: crate::newgame::NewGameUi,
    pub hub: crate::hub::HubUi,
    /// The space whose screen is open (Escape goes back to its page).
    pub hub_origin: Option<usize>,
    /// The dashboard: its selection, and whether the open screen came from it (Escape goes back).
    pub dash: crate::dash_view::DashUi,
    pub dash_origin: bool,
}

/// Ranges of the exchange charts, days.
/// What the quick futures keys of the exchange sign: kilograms and days.
pub const FUTURE_KG: f64 = 100.0;
pub const FUTURE_DAYS: f64 = 30.0;

pub const BOURSE_RANGES: [usize; 3] = [30, 90, 180];

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
            pointer: Default::default(),
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
            dz: crate::designer_view::DesignerUi::default(),
            crew_sel: 0,
            recruit_sel: 0,
            journal_scroll: 0,
            journal_tab: 0,
            arch_sel: 0,
            tech_mind: false,
            mind_sel: 0,
            log_seen: 0,
            gal_cache: Vec::new(),
            gal_features: Vec::new(),
            gal_sel: 0,
            nav_ui: Default::default(),
            gal_zoom: 14.0,
            offers: Vec::new(),
            contract_sel: 0,
            emp_sel: 0,
            col_idx: 0,
            col_focus: 0,
            col_flow: false,
            col_health: false,
            col_tab: 0,
            tab_sel: 0,
            ship_to: 0,
            ship_res: 0,
            col_proj: false,
            proj_focus: 1,
            proj_sel: 0,
            cat_sel: 0,
            proj_units: 0,
            proj_species: 0,
            proc_sel: 0,
            fac_sel: 0,
            rel_page: false,
            soc_page: false,
            soc_who: 0,
            soc_study: 0,
            soc_art: 0,
            soc_sport: 0,
            rel_sel: 0,
            war_page: false,
            war_agent: 0,
            war_target: 0,
            war_cells: Vec::new(),
            crew_net: false,
            organ_mode: false,
            med_mode: false,
            med_drug: 0,
            med_wound: 0,
            gear_mode: false,
            gear_sel: 0,
            organ_sel: 0,
            flight_menu_open: true,
            ext_view: false,
            ext_yaw: crate::ext_view::ZOOM_YAW,
            ext_pitch: 0.3,
            ext_zoom: 1.9,
            ext_chase: true,
            exp_party: Vec::new(),
            exp_vehicle: false,
            exp_sel: 0,
            rogue_from_surface: false,
            rogue_visit: false,
            dlg_npc: 0,
            dlg_node: String::new(),
            dlg_result: String::new(),
            dlg_log: Vec::new(),
            dlg_sel: 0,
            cyber_sel: 0,
            proto: 0,
            proto_slot: 0,
            fleet_map: false,
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
            codex_back: Vec::new(),
            flight_maps: Default::default(),
            surf: None,
            surf_cx: 0,
            surf_cy: 0,
            surf_layer: 0,
            surf_geom: Default::default(),
            surf_globe: false,
            surf_spin: 0.0,
            surf_cities: Vec::new(),
            surf_civ: None,
            surf_eco: None,
            surf_species: 0,
            surf_card: false,
            surf_variants: std::cell::RefCell::new((i64::MIN, Vec::new())),
            bourse_station: 0,
            bourse_sel: 0,
            bourse_range: 1,
            bourse_series: Vec::new(),
            bourse_tab: 0,
            comp_sel: 0,
            farm: Default::default(),
            rogue_ui: Default::default(),
            fpv: false,
            fpv_dir: 0,
            cosmos_ui: crate::cosmos_view::CosmosUi {
                stasis: 3,
                ..Default::default()
            },
            trade_ui: crate::trade_view::TradeUi {
                bid: 12,
                ..Default::default()
            },
            comp_series: Vec::new(),
            bourse_live: Vec::new(),
            bourse_news: Vec::new(),
            family_member: 0,
            family_tab: 0,
            family_ped: None,
            help_open: false,
            rear_view: false,
            tut_step: None,
            tut_keys: Vec::new(),
            save_path: PathBuf::from("saves").join("slot1.sav"),
            ng: Default::default(),
            hub: Default::default(),
            hub_origin: None,
            dash: Default::default(),
            dash_origin: false,
        };
        g.fit_map();
        g
    }

    /// Start a fresh game from a seed (the classic scenario) and go to the cockpit.
    pub fn new_game(&mut self, seed: u64) {
        let mut cfg = vector_sim::start::StartConfig::new(seed);
        cfg.rules = self.pending_rules.clone();
        self.start_game(&cfg);
    }

    /// Start the game a configuration describes: the scenario's ship, hold, crew and standing.
    /// A wreck begins on foot on the ground; any other start begins in the cockpit.
    pub fn start_game(&mut self, cfg: &vector_sim::start::StartConfig) -> bool {
        let sim = match vector_sim::start::new_sim_with(cfg, Arc::clone(&self.data)) {
            Ok(s) => s,
            Err(e) => {
                self.say(format!("ECHEC : {}", e.to_uppercase()));
                return false;
            }
        };
        let seed = cfg.seed;
        let wreck = self.data.scenarios[cfg.scenario].wreck;
        self.sim = sim;
        self.help_open = false;
        self.tut_step = None;
        self.hub_origin = None;
        self.dash_origin = false;
        self.rogue_visit = false;
        self.rogue_from_surface = false;
        // The guided tutorial teaches the cockpit: a castaway has no cockpit yet.
        if self.settings.tutorial && !wreck {
            self.start_tutorial();
        }
        self.stars = scene::star_field(seed, 900);
        self.timescale = 1.0;
        self.keys.clear();
        self.messages = vec![format!("NOUVELLE PARTIE - GRAINE {seed}")];
        if wreck && self.sim.rogue.is_some() {
            self.messages.push(
                "NAUFRAGE : CHERCHEZ DES PIECES DETACHEES ET DU CARBURANT (EPAVES, CAMPEMENTS, RUINES)".into(),
            );
            self.messages.push(
                "L'INGENIEUR REPARE SEUL ; F10 : MENU, VOUS DIT OU EN EST LE VAISSEAU".into(),
            );
            // The walk shows its own log: the same words open it.
            if let Some(r) = self.sim.rogue.as_mut() {
                r.log
                    .push("NAUFRAGE : LE VAISSEAU EST HORS D'ETAT DE VOLER.".into());
                r.log.push("CHERCHEZ PIECES DETACHEES ET CARBURANT (EPAVES, CAMPEMENTS, RUINES) ; F10 : MENU.".into());
            }
            self.screen = Screen::Rogue;
        } else {
            self.screen = Screen::Flight;
        }
        self.fit_map();
        self.log_seen = self.sim.crew.log.len();
        true
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
        let [pitch_up, pitch_down, yaw_right, yaw_left, roll_left, roll_right] =
            self.settings.move_keys();
        ShipInput {
            pitch: (ax(pitch_up, pitch_down) + self.mouse.1).clamp(-1.0, 1.0),
            yaw: (ax(yaw_right, yaw_left) + self.mouse.0).clamp(-1.0, 1.0),
            roll: ax(roll_left, roll_right),
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

    /// The map of a solid world of the system, for its globe in the cockpit (built once).
    pub fn flight_map(&self, body: usize) -> Option<std::rc::Rc<vector_sim::planet::PlanetMap>> {
        let mut cache = self.flight_maps.borrow_mut();
        // A terraformed world changes: the stamp makes the maps be built again.
        let key = self.sim.system.seed ^ self.sim.terra_stamp().wrapping_mul(0x9E37_79B9_7F4A_7C15);
        if cache.0 != key {
            cache.0 = key;
            cache.1.clear();
        }
        if let Some(m) = cache.1.get(&body) {
            return Some(std::rc::Rc::clone(m));
        }
        let m = std::rc::Rc::new(self.sim.planet_map(body)?);
        cache.1.insert(body, std::rc::Rc::clone(&m));
        Some(m)
    }

    pub fn view(&self) -> crate::hud::View<'_> {
        crate::hud::View {
            game: self,
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
                    && !matches!(
                        self.screen,
                        Screen::MainMenu | Screen::Pause | Screen::NewGame | Screen::Hub
                    )))
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
        self.update_ground();
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

    /// Write a line in the ship's journal in the AI's own voice.
    pub(crate) fn note(&mut self, text: String) {
        let now = self.sim.time;
        self.sim.crew.log_event(now, format!("IA : {text}"));
    }

    pub(crate) fn handle_events(&mut self) {
        let evs = std::mem::take(&mut self.sim.events);
        for e in evs {
            if self.say_pilot_event(&e) {
                continue;
            }
            match e {
                Event::Impact {
                    body,
                    speed,
                    damage,
                } => {
                    let name = self.sim.bodies[body].name.clone();
                    self.say(format!("IMPACT {name} {speed:.0} M/S  DEGATS {damage:.0}"));
                    self.note(format!(
                        "impact sur {name} a {speed:.0} m/s, coque endommagee ({damage:.0})."
                    ));
                    self.timescale = 1.0;
                }
                Event::Landed { body } => {
                    let name = self.sim.bodies[body].name.to_uppercase();
                    self.say(format!(
                        "POSE SUR {name} : FIN POUR EXPLORER A PIED, POUSSEE POUR DECOLLER"
                    ));
                    self.note(format!("pose sur {}.", self.sim.bodies[body].name));
                    self.timescale = 1.0;
                }
                Event::Orbit { .. } => {}
                Event::Toppled { .. }
                | Event::PilotDone(_)
                | Event::PilotFailed(_)
                | Event::TooHeavy
                | Event::HardDock { .. } => {}
                Event::OutOfFuel => self.say("CARBURANT EPUISE"),
                Event::Arrived { body } => {
                    let name = self.sim.bodies[body].name.clone();
                    self.say(format!("ARRIVE A {name}"));
                    self.note(format!("arrivee pres de {name}."));
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
                    self.note(format!("saut termine : systeme {name}."));
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
                    self.say(format!("CONTRAT REMPLI: +{reward:.0} CR"));
                    self.note(format!("contrat rempli, {reward:.0} cr recus."));
                }
                Event::ContractExpired => self.say("UN CONTRAT A EXPIRE (-REPUTATION)"),
                Event::BankSeizure { amount } => {
                    self.say(format!("LA BANQUE SAISIT {amount:.0} CR POUR PRET IMPAYE"))
                }
                Event::PatrolScan => self.say("PATROUILLE: SCAN DE VOTRE SOUTE"),
                Event::ColonyFounded { index } => {
                    let name = self.sim.colonies[index].name.clone();
                    self.say(format!("COLONIE FONDEE : {name}"));
                    self.note(format!("colonie fondee : {name}."));
                }
                Event::Surprise(t) => {
                    self.note(t.clone());
                    self.say(t.to_uppercase())
                }
                Event::Nav(t) => {
                    // The computer gave the ship back (or finished): time goes back to normal.
                    if t.starts_with("NAVIGATION AUTO") || t.starts_with("RETOUR D'URGENCE") {
                        if !t.contains("ENGAGE") {
                            self.timescale = 1.0;
                        }
                        self.note(t.to_lowercase());
                    }
                    self.say(t.to_uppercase())
                }
                Event::ColonyNews(t) | Event::MarketNews(t) => {
                    if t.starts_with("INTRUSION") || t.starts_with("Contrat a terme") {
                        self.note(t.clone());
                    }
                    self.say(t.to_uppercase())
                }
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
                    self.note(format!("des pirates exigent {amount:.0} cr."));
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
                Event::ShipBuilt { name } => {
                    self.say(format!("LE CHANTIER LIVRE : {}", name.to_uppercase()))
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
        if self.sim.jump_run.take().is_some() {
            self.say("MANOEUVRE DE SAUT ANNULEE");
            return;
        }
        if self.sim.autopilot.take().is_some() {
            self.say("PILOTE AUTO DESACTIVE");
            return;
        }
        let Some(t) = self.sim.target else {
            self.say("PILOTE AUTO: AUCUNE CIBLE");
            return;
        };
        match self.sim.engage_autopilot(t) {
            Ok(()) => {
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
    /// Target the next planet or moon (Maj + Tab).
    pub fn target_planet(&mut self) {
        self.sim.target_npc = None;
        self.cycle_target(|k| matches!(k, BodyKind::Planet(_) | BodyKind::Moon(_)));
    }

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
        // The market opened from a terminal: Escape goes back to the walk, not out of the dock.
        if pressed
            && self.rogue_visit
            && at == Screen::Station
            && code == KeyCode::Escape
            && !self.console.open
            && !self.help_open
        {
            self.screen = Screen::Rogue;
            return;
        }
        self.key_inner(code, text, pressed);
        // Escape from a screen the menu opened goes back to that space's page.
        if let Some(space) = self.hub_origin {
            let root = matches!(self.screen, Screen::Flight | Screen::Station);
            if self.screen == Screen::Hub {
                self.hub_origin = None;
            } else if root {
                if pressed
                    && code == KeyCode::Escape
                    && !matches!(at, Screen::Flight | Screen::Station | Screen::Hub)
                {
                    self.hub_return(space);
                } else {
                    self.hub_origin = None;
                }
            }
        }
        // Escape from a screen the dashboard opened comes back to the dashboard.
        if self.dash_origin {
            let root = matches!(self.screen, Screen::Flight | Screen::Station);
            if self.screen == Screen::Dashboard {
                self.dash_origin = false;
            } else if root {
                if pressed
                    && code == KeyCode::Escape
                    && !matches!(at, Screen::Flight | Screen::Station | Screen::Dashboard)
                {
                    self.screen = Screen::Dashboard;
                }
                self.dash_origin = false;
            }
        }
        if self.rogue_visit {
            if self.sim.rogue.is_none() {
                self.rogue_visit = false;
            } else if matches!(self.screen, Screen::Station | Screen::Flight)
                && at != self.screen
                && at != Screen::Rogue
            {
                self.screen = Screen::Rogue;
            }
        }
        if pressed && !matches!(self.screen, Screen::MainMenu | Screen::NewGame) {
            self.tutorial_tick(Some((at, code)));
        }
    }

    fn key_inner(&mut self, code: KeyCode, text: Option<&str>, pressed: bool) {
        if pressed
            && code == KeyCode::Backquote
            && !matches!(self.screen, Screen::MainMenu | Screen::NewGame)
        {
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
        if code == KeyCode::Delete && self.tut_step.is_some() {
            self.tut_step = None;
            self.say("TUTORIEL IGNORE (REACTIVABLE DANS OPTIONS)");
            return;
        }
        // The menu is one key away from every screen of the game.
        if code == KeyCode::F10
            && !matches!(
                self.screen,
                Screen::MainMenu | Screen::NewGame | Screen::Destroyed
            )
        {
            self.help_open = false;
            self.open_hub();
            return;
        }
        // The dashboard is one key away from every screen (backslash, or Pause).
        if matches!(
            code,
            KeyCode::Backslash | KeyCode::IntlBackslash | KeyCode::Pause
        ) && !matches!(
            self.screen,
            Screen::MainMenu | Screen::NewGame | Screen::Destroyed | Screen::Designer
        ) {
            self.help_open = false;
            self.open_dashboard();
            return;
        }
        self.keys.insert(code);
        match self.screen {
            Screen::MainMenu => self.main_menu_key(code, text),
            Screen::NewGame => self.newgame_key(code, text),
            Screen::Hub => self.hub_key(code),
            Screen::Dashboard => self.dash_key(code),
            Screen::NavAuto => self.nav_key(code),
            Screen::Pause => self.pause_key(code),
            Screen::Station => self.station_key(code),
            Screen::Inventory => self.inventory_key(code),
            Screen::Ship => self.ship_key(code),
            Screen::Shipyard => self.shipyard_key(code),
            Screen::Designer => self.designer_key(code, text),
            Screen::Crew => self.crew_key(code),
            Screen::Recruit => self.recruit_key(code),
            Screen::Journal => self.journal_key(code),
            Screen::Galaxy => self.galaxy_key(code),
            Screen::Contracts => self.contracts_key(code),
            Screen::Bank => self.bank_key(code),
            Screen::Livestock => self.livestock_key(code),
            Screen::Casino => self.casino_key(code),
            Screen::Trading => self.trade_key(code),
            Screen::Cosmos => self.cosmos_key(code),
            Screen::Rogue => self.rogue_key(code),
            Screen::Empire => self.empire_key(code),
            Screen::Colony => self.colony_key(code),
            Screen::Fleet => self.fleet_key(code),
            Screen::Tech => self.tech_key(code),
            Screen::Rules => self.rules_key(code),
            Screen::Options => self.options_key(code),
            Screen::Codex => self.codex_key(code),
            Screen::Surface => self.surface_key(code),
            Screen::Bourse => self.bourse_key(code),
            Screen::Factions => self.factions_key(code),
            Screen::Proto => self.proto_key(code),
            Screen::Dialogue => self.dialogue_key(code),
            Screen::Expedition => self.expedition_key(code),
            Screen::Family => self.family_key(code),
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
                1 => self.open_new_game(),
                2 => {
                    self.load_game();
                }
                3 => self.open_rules(Screen::MainMenu),
                4 => self.open_options(Screen::MainMenu),
                5 => self.open_codex(Screen::MainMenu),
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

    pub fn open_rules(&mut self, back: Screen) {
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

    pub fn open_options(&mut self, back: Screen) {
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
                let path = if cfg!(test) {
                    std::env::temp_dir().join("vector_test_settings.cfg")
                } else {
                    PathBuf::from("saves").join("settings.cfg")
                };
                self.settings.save(&path);
            }
            KeyCode::Escape | KeyCode::Enter => self.screen = self.back_to,
            _ => {}
        }
    }

    /// Recompute the pedigree of the crew member shown on the family screen.
    pub fn refresh_family(&mut self) {
        let n = self.sim.crew.members.len();
        if n == 0 {
            self.family_ped = None;
            return;
        }
        self.family_member = self.family_member.min(n - 1);
        let p = &self.sim.crew.members[self.family_member];
        self.family_ped = Some(vector_sim::genealogy::pedigree(
            &self.sim.crew.families,
            &self.data,
            p.kin,
            f64::from(p.age),
        ));
    }

    fn family_key(&mut self, code: KeyCode) {
        let n = self.sim.crew.members.len().max(1);
        match code {
            KeyCode::ArrowLeft | KeyCode::ArrowUp => {
                self.family_member = (self.family_member + n - 1) % n;
                self.refresh_family();
            }
            KeyCode::ArrowRight | KeyCode::ArrowDown => {
                self.family_member = (self.family_member + 1) % n;
                self.refresh_family();
            }
            KeyCode::Tab => self.family_tab = 1 - self.family_tab,
            KeyCode::Escape | KeyCode::KeyG | KeyCode::KeyK => {
                self.crew_sel = self.family_member;
                self.screen = Screen::Crew;
            }
            _ => {}
        }
    }

    /// Open the exchange of the station the ship is docked at.
    pub fn open_bourse(&mut self) {
        let Ok(st) = self.sim.docked_station() else {
            self.say("PAS DE BOURSE SUR CETTE BASE");
            return;
        };
        self.bourse_station = st;
        self.screen = Screen::Bourse;
        self.refresh_bourse();
    }

    /// Recompute the price series and the news for the selected range.
    pub fn refresh_bourse(&mut self) {
        let days = BOURSE_RANGES[self.bourse_range];
        self.bourse_series = self.sim.price_series(self.bourse_station, days);
        self.bourse_news = self.sim.market_news(days as f64);
        self.comp_series = (0..self.data.companies.len())
            .map(|c| self.sim.company_series(c, days))
            .collect();
        let data = std::sync::Arc::clone(&self.data);
        let market = self.sim.market(self.bourse_station);
        self.bourse_live = (0..data.resources.len())
            .map(|r| market.price(&data, r))
            .collect();
    }

    pub fn click_company(&mut self, c: usize) {
        self.comp_sel = c;
    }

    fn bourse_key(&mut self, code: KeyCode) {
        if code == KeyCode::KeyE {
            self.bourse_tab = 1 - self.bourse_tab;
            return;
        }
        if self.bourse_tab == 1 {
            let nc = self.data.companies.len().max(1);
            match code {
                KeyCode::ArrowUp => self.comp_sel = (self.comp_sel + nc - 1) % nc,
                KeyCode::ArrowDown => self.comp_sel = (self.comp_sel + 1) % nc,
                KeyCode::Tab | KeyCode::ArrowRight => {
                    self.bourse_range = (self.bourse_range + 1) % BOURSE_RANGES.len();
                    self.refresh_bourse();
                }
                KeyCode::ArrowLeft => {
                    self.bourse_range =
                        (self.bourse_range + BOURSE_RANGES.len() - 1) % BOURSE_RANGES.len();
                    self.refresh_bourse();
                }
                KeyCode::KeyB => match self.sim.buy_shares(self.comp_sel, 10.0) {
                    Ok(c) => self.say(format!("10 ACTIONS ACHETEES POUR {c:.0} CR")),
                    Err(e) => self.say(e.to_uppercase()),
                },
                KeyCode::KeyN => match self.sim.sell_shares(self.comp_sel, 10.0) {
                    Ok(c) => self.say(format!("ACTIONS VENDUES POUR {c:.0} CR")),
                    Err(e) => self.say(e.to_uppercase()),
                },
                KeyCode::Escape | KeyCode::KeyT => self.screen = Screen::Station,
                _ => {}
            }
            return;
        }
        let n = self.data.resources.len();
        match code {
            KeyCode::ArrowUp => self.bourse_sel = (self.bourse_sel + n - 1) % n,
            KeyCode::ArrowDown => self.bourse_sel = (self.bourse_sel + 1) % n,
            KeyCode::PageUp => self.bourse_sel = self.bourse_sel.saturating_sub(10),
            KeyCode::PageDown => self.bourse_sel = (self.bourse_sel + 10).min(n - 1),
            KeyCode::Tab | KeyCode::ArrowRight => {
                self.bourse_range = (self.bourse_range + 1) % BOURSE_RANGES.len();
                self.refresh_bourse();
            }
            KeyCode::ArrowLeft => {
                self.bourse_range =
                    (self.bourse_range + BOURSE_RANGES.len() - 1) % BOURSE_RANGES.len();
                self.refresh_bourse();
            }
            KeyCode::KeyB | KeyCode::KeyN => {
                let long = code == KeyCode::KeyB;
                match self
                    .sim
                    .open_future(self.bourse_sel, FUTURE_KG, FUTURE_DAYS, long)
                {
                    Ok(_) => self.say(if long {
                        "CONTRAT A TERME A LA HAUSSE SIGNE (MARGE BLOQUEE)"
                    } else {
                        "CONTRAT A TERME A LA BAISSE SIGNE (MARGE BLOQUEE)"
                    }),
                    Err(e) => self.say(e.to_uppercase()),
                }
            }
            KeyCode::KeyC => {
                // Close the oldest contract on the selected good, or the oldest of all.
                let idx = self
                    .sim
                    .futures
                    .iter()
                    .position(|f| f.res == self.bourse_sel)
                    .or(if self.sim.futures.is_empty() {
                        None
                    } else {
                        Some(0)
                    });
                match idx.map(|i| self.sim.close_future(i)) {
                    Some(Ok(p)) => self.say(format!("CONTRAT CLOS : {p:.0} CR RECUPERES")),
                    Some(Err(e)) => self.say(e.to_uppercase()),
                    None => self.say("AUCUN CONTRAT OUVERT"),
                }
            }
            KeyCode::Escape | KeyCode::KeyT => self.screen = Screen::Station,
            _ => {}
        }
    }

    /// Open the surface map of the targeted (or else the reference) planet or moon.
    pub fn open_surface(&mut self) {
        let body = [self.sim.target, self.sim.ref_body]
            .into_iter()
            .flatten()
            .find(|&i| {
                matches!(
                    self.sim.bodies[i].kind,
                    BodyKind::Planet(_) | BodyKind::Moon(_)
                )
            });
        let Some(body) = body else {
            self.say("CIBLEZ UNE PLANETE OU UNE LUNE (TAB/T) POUR VOIR SA SURFACE");
            return;
        };
        if self.sim.scan[body] < vector_sim::voyage::SCAN_SURVEYED {
            self.say("CORPS NON RELEVE : APPROCHEZ-VOUS POUR LE SCANNER (NIVEAU 2)");
            return;
        }
        match self.sim.planet_map(body) {
            Some(map) => {
                self.surf_cx = vector_world::surface::W / 2;
                self.surf_cy = vector_world::surface::H / 2;
                self.surf_cities = self
                    .sim
                    .colonies
                    .iter()
                    .filter(|c| c.star == self.sim.here && c.body == body)
                    .filter_map(|c| {
                        self.sim
                            .city_site(&map, c.id)
                            .map(|(x, y)| (c.name.clone(), x, y))
                    })
                    .collect();
                self.surf_eco = Some(vector_sim::ecology::Ecosystem::new(&self.data, &map));
                self.surf_civ = self.sim.civilisation(body);
                self.surf = Some(map);
                self.screen = Screen::Surface;
            }
            None => self.say("PAS DE SURFACE SOLIDE (GEANTE GAZEUSE OU GLACEE)"),
        }
    }

    fn surface_key(&mut self, code: KeyCode) {
        use vector_world::surface::{H, W};
        let step = if self.keys.contains(&KeyCode::ShiftLeft) {
            4
        } else {
            1
        };
        match code {
            KeyCode::ArrowLeft if self.surf_globe => self.surf_spin -= 0.15 * step as f64,
            KeyCode::ArrowRight if self.surf_globe => self.surf_spin += 0.15 * step as f64,
            KeyCode::ArrowLeft => self.surf_cx = (self.surf_cx + W - step % W) % W,
            KeyCode::ArrowRight => self.surf_cx = (self.surf_cx + step) % W,
            KeyCode::ArrowUp => self.surf_cy = self.surf_cy.saturating_sub(step),
            KeyCode::ArrowDown => self.surf_cy = (self.surf_cy + step).min(H - 1),
            KeyCode::Tab => {
                self.surf_layer = (self.surf_layer + 1) % crate::surface_view::LAYERS.len()
            }
            KeyCode::KeyG => self.surf_globe = !self.surf_globe,
            // BIOSPHERE layer: V / Maj+V pick the species, Enter opens the card of its variant.
            KeyCode::KeyV if self.surf_layer == crate::surface_view::BIOSPHERE => {
                let n = self
                    .surf_eco
                    .as_ref()
                    .map_or(1, |e| crate::surface_view::alive_species(e).len().max(1));
                self.surf_species = if self.keys.contains(&KeyCode::ShiftLeft) {
                    (self.surf_species + n - 1) % n
                } else {
                    (self.surf_species + 1) % n
                };
            }
            KeyCode::Enter if self.surf_layer == crate::surface_view::BIOSPHERE => {
                self.surf_card = !self.surf_card
            }
            // Zoom in: from the map of the region to the grid of the ground, on foot.
            KeyCode::KeyE | KeyCode::Equal | KeyCode::NumpadAdd => self.open_expedition(),
            KeyCode::KeyF => self.open_codex(Screen::Surface),
            KeyCode::Escape | KeyCode::F8 => self.screen = Screen::Flight,
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
            KeyCode::Digit1
            | KeyCode::Digit2
            | KeyCode::Digit3
            | KeyCode::Digit4
            | KeyCode::Digit5
            | KeyCode::Digit6
            | KeyCode::Digit7
            | KeyCode::Digit8
            | KeyCode::Digit9 => {
                let n = match code {
                    KeyCode::Digit1 => 0,
                    KeyCode::Digit2 => 1,
                    KeyCode::Digit3 => 2,
                    KeyCode::Digit4 => 3,
                    KeyCode::Digit5 => 4,
                    KeyCode::Digit6 => 5,
                    KeyCode::Digit7 => 6,
                    KeyCode::Digit8 => 7,
                    _ => 8,
                };
                let links = crate::codex::links(&self.data, self.codex_cat, self.codex_sel);
                if let Some((c, j, _)) = links.get(n) {
                    self.follow_codex_link(c * 10_000 + j);
                }
            }
            KeyCode::Backspace => {
                if let Some((c, k)) = self.codex_back.pop() {
                    self.codex_cat = c;
                    self.codex_sel = k;
                }
            }
            KeyCode::KeyL | KeyCode::Enter | KeyCode::NumpadEnter => self.codex_lab(0),
            KeyCode::Escape | KeyCode::F6 => self.screen = self.back_to,
            _ => {}
        }
    }

    /// Laboratory: run a batch of the reaction selected in the codex (REACTIONS) on the cargo.
    pub fn codex_lab(&mut self, _arg: usize) {
        if self.codex_cat != crate::codex::LAB_CATEGORY {
            return;
        }
        let Some(r) = crate::codex::source_index(&self.data, self.codex_cat, self.codex_sel) else {
            return;
        };
        let name = self.data.reactions[r].name.to_uppercase();
        match self.sim.lab_run(r, LAB_BATCH_KG) {
            Ok(rep) => {
                let names = |l: &[(usize, f64)]| {
                    l.iter()
                        .map(|&(i, kg)| format!("{kg:.1} KG {}", self.data.resources[i].name))
                        .collect::<Vec<_>>()
                        .join(" + ")
                        .to_uppercase()
                };
                let (a, b) = (names(&rep.consumed), names(&rep.produced));
                self.say(format!(
                    "LABO {name} : {a} -> {b}   BATTERIE -{:.1} MJ   CHALEUR +{:.1} MJ",
                    rep.electric_kj / 1000.0,
                    rep.heat_kj / 1000.0
                ));
                if rep.blast_mj > 0.0 {
                    self.say(format!(
                        "EXPLOSION A BORD : {:.0} MJ, DES MODULES SONT ENDOMMAGES",
                        rep.blast_mj
                    ));
                }
            }
            Err(e) => self.say(format!("LABO {name} : {}", e.to_uppercase())),
        }
    }

    /// Open the screen of a terminal of a walk through a social place; Escape comes back.
    pub fn open_terminal(&mut self, idx: usize) {
        let id = vector_data::ROGUE_TERMINALS[idx].0;
        self.rogue_visit = true;
        match id {
            "marche" => self.screen = Screen::Station,
            "chantier" => self.screen = Screen::Shipyard,
            "banque" => self.screen = Screen::Bank,
            "bourse" => self.open_bourse(),
            "courtage" => self.screen = Screen::Trading,
            "contrats" => {
                self.offers = self.sim.offered_contracts();
                self.contract_sel = 0;
                self.screen = Screen::Contracts;
            }
            "recrutement" => {
                self.screen = Screen::Recruit;
                self.recruit_sel = 0;
            }
            "eleveur" | "elevage" => {
                self.farm.dealer = id == "eleveur";
                self.screen = Screen::Livestock;
            }
            "casino" => self.screen = Screen::Casino,
            "infirmerie" | "equipage" => self.screen = Screen::Crew,
            "soute" => self.screen = Screen::Inventory,
            "ingenierie" => self.open_ship(),
            "science" => self.open_tech(),
            "carte" => self.open_galaxy(),
            "gestion" => self.open_empire(),
            "temps" => self.screen = Screen::Cosmos,
            _ => {
                self.rogue_visit = false;
                self.screen = Screen::Flight;
            }
        }
    }

    pub fn station_key(&mut self, code: KeyCode) {
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
            KeyCode::KeyT => {
                self.open_bourse();
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
            KeyCode::Home => {
                self.screen = Screen::Cosmos;
                return;
            }
            KeyCode::End => {
                self.screen = Screen::Rogue;
                return;
            }
            KeyCode::KeyD => {
                self.screen = Screen::Trading;
                return;
            }
            KeyCode::KeyL => {
                self.farm.dealer = true;
                self.screen = Screen::Livestock;
                return;
            }
            KeyCode::KeyA => {
                self.screen = Screen::Casino;
                return;
            }
            KeyCode::KeyG => self
                .sim
                .black_buy(res, 50.0)
                .map(|c| format!("MARCHE NOIR : ACHAT POUR {c:.0} CR")),
            KeyCode::KeyJ => self
                .sim
                .black_sell(res, 50.0)
                .map(|c| format!("MARCHE NOIR : VENTE POUR {c:.0} CR")),
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

    /// Keys of the organ list (surgery) of the crew screen.
    fn organ_key(&mut self, code: KeyCode) {
        use vector_sim::surgery::Implant;
        let no = self.data.organs.len().max(1);
        let n = self.sim.crew.members.len();
        let member = self.crew_sel.min(n.saturating_sub(1));
        let organ = self.organ_sel.min(no - 1);
        let result = match code {
            KeyCode::ArrowUp => {
                self.organ_sel = (organ + no - 1) % no;
                return;
            }
            KeyCode::ArrowDown => {
                self.organ_sel = (organ + 1) % no;
                return;
            }
            KeyCode::Escape | KeyCode::KeyO => {
                self.organ_mode = false;
                return;
            }
            KeyCode::Enter if n > 0 => {
                // A graft if the hold has one, else a prosthesis.
                let graft = self
                    .data
                    .resource_index(&format!("organ_{}", self.data.organs[organ].id))
                    .is_some_and(|r| self.sim.ship.cargo.kg[r] >= 1.0);
                self.sim.implant(
                    member,
                    organ,
                    if graft {
                        Implant::Graft
                    } else {
                        Implant::Prosthesis
                    },
                )
            }
            KeyCode::ArrowLeft => {
                let nc = self.data.cybernetics.len().max(1);
                self.cyber_sel = (self.cyber_sel + nc - 1) % nc;
                return;
            }
            KeyCode::ArrowRight => {
                let nc = self.data.cybernetics.len().max(1);
                self.cyber_sel = (self.cyber_sel + 1) % nc;
                return;
            }
            KeyCode::KeyM if n > 0 => self.sim.inject_mutagen(member),
            KeyCode::KeyK => self
                .sim
                .grow_clone(organ)
                .map(|c| format!("CLONE PRET (-{c:.0} CR) : EN SOUTE")),
            KeyCode::KeyI if n > 0 => self.sim.install_cyber(member, self.cyber_sel),
            KeyCode::KeyU if n > 0 => self.sim.remove_cyber(member, self.cyber_sel),
            KeyCode::KeyP if n > 0 => self.sim.implant(member, organ, Implant::Prosthesis),
            KeyCode::KeyH if n > 0 => self.sim.harvest(member, organ),
            _ => return,
        };
        match result {
            Ok(r) => self.say(r.to_uppercase()),
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    pub fn click_organ(&mut self, i: usize) {
        self.organ_mode = true;
        self.organ_sel = i;
    }

    /// Keys of the equipment list of the crew screen.
    fn gear_key(&mut self, code: KeyCode) {
        let ng = self.data.gear.len().max(1);
        let n = self.sim.crew.members.len();
        let member = self.crew_sel.min(n.saturating_sub(1));
        let result = match code {
            KeyCode::ArrowUp | KeyCode::ArrowLeft => {
                self.gear_sel = (self.gear_sel + ng - 1) % ng;
                return;
            }
            KeyCode::ArrowDown | KeyCode::ArrowRight => {
                self.gear_sel = (self.gear_sel + 1) % ng;
                return;
            }
            KeyCode::Escape | KeyCode::KeyE => {
                self.gear_mode = false;
                return;
            }
            KeyCode::Enter if n > 0 => self.sim.equip(member, self.gear_sel),
            KeyCode::KeyU if n > 0 => {
                let slot = self.data.gear[self.gear_sel.min(ng - 1)].slot_index();
                self.sim.unequip(member, slot)
            }
            _ => return,
        };
        match result {
            Ok(r) => self.say(r.to_uppercase()),
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    /// Click on a slot line: open the equipment list on what is worn there (or the first item).
    pub fn click_gear_slot(&mut self, slot: usize) {
        self.gear_mode = true;
        let worn = self
            .sim
            .crew
            .members
            .get(self.crew_sel)
            .and_then(|p| p.gear.get(slot).copied().flatten());
        self.gear_sel = worn
            .or_else(|| self.data.gear.iter().position(|g| g.slot_index() == slot))
            .unwrap_or(0);
    }

    /// Keys of the MEDECINE tab of the crew screen.
    fn med_key(&mut self, code: KeyCode) {
        let n = self.sim.crew.members.len();
        let member = self.crew_sel.min(n.saturating_sub(1));
        let nd = self.data.drugs.len().max(1);
        let result = match code {
            KeyCode::ArrowUp if n > 0 => {
                self.crew_sel = (self.crew_sel + n - 1) % n;
                self.med_wound = 0;
                return;
            }
            KeyCode::ArrowDown if n > 0 => {
                self.crew_sel = (self.crew_sel + 1) % n;
                self.med_wound = 0;
                return;
            }
            KeyCode::ArrowLeft => {
                self.med_drug = (self.med_drug + nd - 1) % nd;
                return;
            }
            KeyCode::ArrowRight => {
                self.med_drug = (self.med_drug + 1) % nd;
                return;
            }
            KeyCode::Tab => {
                self.med_wound += 1;
                return;
            }
            KeyCode::Escape | KeyCode::KeyM => {
                self.med_mode = false;
                return;
            }
            KeyCode::KeyA if n > 0 => self.sim.first_aid(member),
            KeyCode::KeyS if n > 0 => {
                // The selected wound as the carer sees it (internal ones may be hidden).
                let skill = self.sim.carer_skill(member);
                let (list, _) = self.sim.crew.members[member].body.diagnose(
                    &self.data,
                    skill,
                    self.sim.effective.medbay,
                );
                match list.get(self.med_wound.min(list.len().saturating_sub(1))) {
                    Some((wi, _)) => self.sim.operate_wound(member, *wi),
                    None => Err("aucune plaie visible a operer"),
                }
            }
            KeyCode::Enter if n > 0 => self.sim.give_drug(member, self.med_drug.min(nd - 1)),
            KeyCode::KeyR if n > 0 => self.sim.defibrillate(member),
            KeyCode::KeyC if n > 0 => {
                if self.sim.crew.members[member].body.cryo {
                    self.sim.cryo_wake(member)
                } else {
                    self.sim.cryo_start(member)
                }
            }
            _ => return,
        };
        match result {
            Ok(r) => self.say(r.to_uppercase()),
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    pub fn click_wound(&mut self, k: usize) {
        self.med_wound = k;
    }

    pub fn crew_key(&mut self, code: KeyCode) {
        if self.med_mode {
            self.med_key(code);
            return;
        }
        if self.organ_mode {
            self.organ_key(code);
            return;
        }
        if self.gear_mode {
            self.gear_key(code);
            return;
        }
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
            KeyCode::KeyV => self.crew_net = !self.crew_net,
            KeyCode::KeyO if n > 0 => self.organ_mode = true,
            KeyCode::KeyM if n > 0 => self.med_mode = true,
            KeyCode::KeyE if n > 0 => self.gear_mode = true,
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
            KeyCode::KeyG if n > 0 => {
                self.family_member = self.crew_sel.min(n - 1);
                self.refresh_family();
                self.screen = Screen::Family;
            }
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

    fn factions_key(&mut self, code: KeyCode) {
        if self.war_page {
            match code {
                KeyCode::KeyG | KeyCode::Escape => self.war_page = false,
                KeyCode::KeyX => self.screen = Screen::Flight,
                c => {
                    self.war_key(c);
                }
            }
            return;
        }
        if code == KeyCode::KeyG {
            self.open_war_page();
            return;
        }
        if code == KeyCode::Tab {
            self.rel_page = !self.rel_page;
            return;
        }
        if self.rel_page {
            let n = self.sim.relation_rows().len().max(1);
            match code {
                KeyCode::ArrowDown => self.rel_sel = (self.rel_sel + 1) % n,
                KeyCode::ArrowUp => self.rel_sel = (self.rel_sel + n - 1) % n,
                KeyCode::PageDown => self.rel_sel = (self.rel_sel + 8).min(n - 1),
                KeyCode::PageUp => self.rel_sel = self.rel_sel.saturating_sub(8),
                KeyCode::Escape | KeyCode::KeyX => self.screen = Screen::Flight,
                _ => {}
            }
            return;
        }
        if code == KeyCode::KeyS {
            self.soc_page = !self.soc_page;
            return;
        }
        let n = self.data.factions.len().max(1);
        if self.soc_page {
            let crew = self.sim.crew.count().max(1);
            let studies = self.data.society.studies.len();
            match code {
                KeyCode::ArrowRight => self.fac_sel = (self.fac_sel + 1) % n,
                KeyCode::ArrowLeft => self.fac_sel = (self.fac_sel + n - 1) % n,
                KeyCode::ArrowDown => self.soc_who = (self.soc_who + 1) % crew,
                KeyCode::ArrowUp => self.soc_who = (self.soc_who + crew - 1) % crew,
                KeyCode::PageDown => self.soc_study = (self.soc_study + 1) % studies,
                KeyCode::PageUp => self.soc_study = (self.soc_study + studies - 1) % studies,
                KeyCode::KeyE => self.click_society(crate::society_view::ENROLL),
                KeyCode::KeyC => self.click_society(crate::society_view::COMPOSE),
                KeyCode::KeyV => self.click_society(crate::society_view::SELL),
                KeyCode::KeyD => self.click_society(crate::society_view::DONATE),
                KeyCode::KeyP => self.click_society(crate::society_view::BET),
                KeyCode::KeyA => self.click_society(crate::society_view::NEXT_ART),
                KeyCode::KeyJ => self.click_society(crate::society_view::NEXT_SPORT),
                KeyCode::Escape | KeyCode::KeyX => self.screen = Screen::Flight,
                _ => {}
            }
            return;
        }
        match code {
            KeyCode::ArrowRight | KeyCode::ArrowDown => self.fac_sel = (self.fac_sel + 1) % n,
            KeyCode::ArrowLeft | KeyCode::ArrowUp => self.fac_sel = (self.fac_sel + n - 1) % n,
            KeyCode::KeyP => self.click_justice(0),
            KeyCode::KeyB => self.click_justice(1),
            KeyCode::KeyL => self.click_justice(2),
            KeyCode::Escape | KeyCode::KeyX => self.screen = Screen::Flight,
            _ => {}
        }
    }

    fn journal_key(&mut self, code: KeyCode) {
        if self.journal_tab == 2 && self.archive_key(code) {
            return;
        }
        match code {
            KeyCode::ArrowUp | KeyCode::PageUp => self.journal_scroll += 5,
            KeyCode::ArrowDown | KeyCode::PageDown => {
                self.journal_scroll = self.journal_scroll.saturating_sub(5)
            }
            KeyCode::KeyK => self.screen = Screen::Crew,
            KeyCode::Digit1 | KeyCode::Digit2 | KeyCode::Digit3 => {
                let choice = match code {
                    KeyCode::Digit1 => 0,
                    KeyCode::Digit2 => 1,
                    _ => 2,
                };
                self.answer_surprise(choice);
            }
            KeyCode::Tab => {
                self.journal_tab = (self.journal_tab + 1) % 3;
                self.journal_scroll = 0;
            }
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

    /// Answer the oldest surprise that waits for a choice.
    fn answer_surprise(&mut self, choice: usize) {
        match self.sim.surprise_choose(0, choice) {
            Ok(words) => {
                self.say(words.to_uppercase());
                self.note(words);
            }
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    pub fn open_empire(&mut self) {
        self.emp_sel = 0;
        self.screen = Screen::Empire;
    }

    pub fn open_fleet(&mut self) {
        self.endpoints = self.sim.known_endpoints();
        self.fleet_focus = 0;
        self.editor = None;
        self.hull_pick = None;
        self.screen = Screen::Fleet;
    }

    /// Found a colony on the targeted body with the kit in the hold.
    pub fn found_colony_at_target(&mut self) {
        let Some(t) = self.sim.target else {
            self.say("CIBLEZ D ABORD UN CORPS A COLONISER");
            return;
        };
        match self.sim.found_colony(t, None) {
            Ok(_) => self.handle_events(),
            Err(e) => self.say(format!("COLONISATION IMPOSSIBLE: {}", e.to_uppercase())),
        }
    }

    pub fn empire_key(&mut self, code: KeyCode) {
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
    pub fn dock_screen(&self) -> Screen {
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
        if self.col_proj {
            self.project_key(code);
            return;
        }
        let here = self.sim.docked_colony() == Some(ci);
        let nb = self.sim.colonies[ci].buildings.len();
        let nres = self.data.resources.len();
        let np = self.data.policies.len();
        if self.colony_tab_key(code, ci) {
            return;
        }
        match code {
            KeyCode::KeyF => {
                self.col_flow = !self.col_flow;
                self.col_health = false;
                self.col_tab = 0;
            }
            KeyCode::KeyH => {
                self.col_health = !self.col_health;
                self.col_flow = false;
                self.col_tab = 0;
            }
            KeyCode::KeyP => self.col_proj = true,
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

    /// Keys of the works tabs (industry, farming, city, logistics). True when the key was used.
    fn colony_tab_key(&mut self, code: KeyCode, ci: usize) -> bool {
        for (i, (_, key, _)) in crate::works_view::TABS.iter().enumerate() {
            if code == *key {
                let tab = (i + 1) as u8;
                self.col_tab = if self.col_tab == tab { 0 } else { tab };
                self.col_flow = false;
                self.col_health = false;
                return true;
            }
        }
        if self.col_tab == 0 {
            return false;
        }
        let nb = self.sim.colonies[ci].buildings.len();
        match (self.col_tab, code) {
            (_, KeyCode::Escape) => self.col_tab = 0,
            (1, KeyCode::ArrowUp) if nb > 0 => self.tab_sel = (self.tab_sel + nb - 1) % nb,
            (1, KeyCode::ArrowDown) if nb > 0 => self.tab_sel = (self.tab_sel + 1) % nb,
            (1, KeyCode::Space) if nb > 0 => {
                let b = &mut self.sim.colonies[ci].buildings[self.tab_sel.min(nb - 1)];
                b.enabled = !b.enabled;
            }
            (1, KeyCode::KeyR) if nb > 0 => {
                self.sim.colonies[ci].buildings[self.tab_sel.min(nb - 1)].recipe += 1;
            }
            (2, KeyCode::KeyC) => {
                let n = self.data.crops.len();
                let w = &mut self.sim.colonies[ci].works;
                if n > 0 {
                    w.prev_crop = w.crop;
                    w.crop = if w.crop >= n { 0 } else { (w.crop + 1) % n };
                    w.planted = self.sim.time;
                    self.say("NOUVELLE CULTURE SEMEE");
                }
            }
            (2, KeyCode::KeyR) => {
                let w = &mut self.sim.colonies[ci].works;
                w.rotate = !w.rotate;
            }
            (4, KeyCode::ArrowLeft) => self.ship_to = self.ship_to.saturating_sub(1),
            (4, KeyCode::ArrowRight) => self.ship_to += 1,
            (4, KeyCode::ArrowUp) => self.ship_res = self.ship_res.saturating_sub(1),
            (4, KeyCode::ArrowDown) => self.ship_res += 1,
            (4, KeyCode::Enter) => self.click_ship(1),
            _ => return false,
        }
        true
    }

    /// Send a convoy of the goods and destination chosen in the logistics tab: 0..2 the preset
    /// loads, 3 everything.
    pub fn click_ship(&mut self, which: usize) {
        let ci = self.col_idx;
        let Some(c) = self.sim.colonies.get(ci) else {
            return;
        };
        let dests: Vec<usize> = self
            .sim
            .colonies
            .iter()
            .enumerate()
            .filter(|(i, o)| *i != ci && o.system_seed == c.system_seed)
            .map(|(i, _)| i)
            .collect();
        let items: Vec<usize> = (0..self.data.resources.len())
            .filter(|&i| c.storage[i] > 0.5)
            .collect();
        if dests.is_empty() || items.is_empty() {
            self.say("RIEN A ENVOYER OU PERSONNE A QUI L'ENVOYER");
            return;
        }
        let to = dests[self.ship_to % dests.len()];
        let res = items[self.ship_res % items.len()];
        let kg = match which {
            0 => 500.0,
            1 => 2000.0,
            2 => 10_000.0,
            _ => c.storage[res],
        }
        .min(c.storage[res]);
        match self.sim.colony_dispatch(ci, to, res, kg) {
            Ok(d) => self.say(format!("CONVOI PARTI : ARRIVEE DANS {d:.1} JOURS")),
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    pub fn click_tab_row(&mut self, k: usize) {
        self.tab_sel = k;
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
            KeyCode::KeyM => self.fleet_map = !self.fleet_map,
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
            KeyCode::KeyO if ns > 0 => self.squad_click(0),
            KeyCode::KeyR if ns > 0 => self.squad_click(1),
            KeyCode::KeyV if ns > 0 => self.squad_click(2),
            KeyCode::Digit1 if ns > 0 => self.squad_click(3),
            KeyCode::Digit2 if ns > 0 => self.squad_click(4),
            KeyCode::Digit3 if ns > 0 => self.squad_click(5),
            KeyCode::Digit4 if ns > 0 => self.squad_click(6),
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
    pub fn open_tech(&mut self) {
        // Start on what is being researched, else on the first technology within reach.
        self.tech_sel = self
            .sim
            .current_research
            .or_else(|| (0..self.data.techs.len()).find(|&t| self.sim.tech_available(t)))
            .unwrap_or(0);
        self.screen = Screen::Tech;
    }

    /// Move the selection on the technology graph. `dx` walks to a prerequisite (-1) or to what
    /// the technology opens (+1), `dy` to the neighbour above or below in the same column.
    pub fn tech_walk(&mut self, dx: i32, dy: i32) {
        use crate::tech_view::{layout, TechLayout};
        let lay = layout(&self.data);
        let t = self.tech_sel.min(self.data.techs.len() - 1);
        let next = match (dx, dy) {
            (-1, _) => TechLayout::requires(&self.data, t).first().copied(),
            (1, _) => TechLayout::dependents(&self.data, t).first().copied(),
            (_, d) => {
                let col = &lay.layers[lay.layer[t]];
                let i = lay.row[t] as i32 + d;
                (!col.is_empty()).then(|| col[i.rem_euclid(col.len() as i32) as usize])
            }
        };
        if let Some(n) = next {
            self.tech_sel = n;
        }
    }

    /// Select a technology and, if it can be researched, start it.
    pub fn tech_start(&mut self, t: usize) {
        match self.sim.start_research(t) {
            Ok(()) => {
                let name = self.data.techs[t].name.to_uppercase();
                self.say(format!("RECHERCHE LANCEE : {name}"));
            }
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    fn tech_key(&mut self, code: KeyCode) {
        if code == KeyCode::KeyE {
            self.tech_mind = !self.tech_mind;
            return;
        }
        if self.tech_mind && code != KeyCode::Escape && code != KeyCode::F4 {
            self.mind_key(code);
            return;
        }
        let n = self.data.techs.len();
        match code {
            KeyCode::ArrowUp => self.tech_walk(0, -1),
            KeyCode::ArrowDown => self.tech_walk(0, 1),
            KeyCode::ArrowLeft => self.tech_walk(-1, 0),
            KeyCode::ArrowRight => self.tech_walk(1, 0),
            KeyCode::PageUp => self.tech_sel = self.tech_sel.saturating_sub(10),
            KeyCode::PageDown => self.tech_sel = (self.tech_sel + 10).min(n - 1),
            KeyCode::Tab => {
                // Jump to what is being researched, else to the next technology within reach.
                let from = self.tech_sel;
                let found = self
                    .sim
                    .current_research
                    .filter(|&t| t != from)
                    .or_else(|| {
                        (1..=n)
                            .map(|k| (from + k) % n)
                            .find(|&t| self.sim.tech_available(t))
                    });
                if let Some(t) = found {
                    self.tech_sel = t;
                }
            }
            KeyCode::Enter => self.tech_start(self.tech_sel.min(n - 1)),
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
            KeyCode::KeyH => {
                if self.sim.bank.mortgage {
                    self.sim
                        .release_mortgage()
                        .map(|()| "HYPOTHEQUE LEVEE".to_string())
                } else {
                    self.sim
                        .mortgage_ship()
                        .map(|()| "VAISSEAU HYPOTHEQUE : PLAFOND RELEVE".to_string())
                }
            }
            KeyCode::KeyC => self
                .sim
                .buy_currency(1000.0)
                .map(|u| format!("ACHAT DE {u:.0} UNITES DE MONNAIE LOCALE")),
            KeyCode::KeyV => self
                .sim
                .sell_currency()
                .map(|g| format!("MONNAIE LOCALE VENDUE : {g:.0} CR")),
            KeyCode::KeyB => self
                .sim
                .launder(f64::MAX)
                .map(|fee| format!("ARGENT BLANCHI (COMMISSION {fee:.0} CR)")),
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
        self.gal_features = self.sim.galaxy.features_near(self.sim.here_pos, 90.0);
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
                    match self.sim.jump_or_run(&target) {
                        Ok(false) => {
                            self.screen = Screen::Flight;
                            self.handle_events();
                        }
                        Ok(true) => {
                            self.screen = Screen::Flight;
                            self.say("MANOEUVRE : LE VAISSEAU S'ELOIGNE DES CORPS MASSIFS, LE SAUT PARTIRA TOUT SEUL (G ANNULE)");
                        }
                        Err(e) => self.say(format!("SAUT IMPOSSIBLE: {}", e.to_uppercase())),
                    }
                }
            }
            KeyCode::Escape | KeyCode::KeyL => self.screen = Screen::Flight,
            _ => {}
        }
    }

    pub fn open_ship(&mut self) {
        self.screen = Screen::Ship;
        self.picker = false;
        self.ship_sel = 0;
    }

    /// Modules that can go into slot `slot` of the current hull, grouped by category then size.
    pub fn compatible_modules(&self, slot: usize) -> Vec<usize> {
        let (ty, size) = self.sim.data.hulls[self.sim.ship.design.hull].slots[slot];
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
                        match self.sim.swap_module(self.ship_sel, mi) {
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
                self.picker = true;
                self.pick_sel = 0;
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
        let n = self.sim.base_hulls;
        match code {
            KeyCode::KeyC => self.dz_open(),
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
            // The fuel carried in the hold goes into the propellant tank.
            KeyCode::KeyF => match self.sim.refuel_from_hold() {
                Ok(kg) => self.say(format!("RESERVOIR : +{kg:.0} KG DE CARBURANT")),
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::Escape | KeyCode::KeyI => self.screen = Screen::Flight,
            _ => {}
        }
    }

    fn flight_key(&mut self, code: KeyCode) {
        if self.ext_key(code) {
            return;
        }
        if self.pilot_aid_key(code) {
            return;
        }
        match code {
            KeyCode::Delete => self.toggle_ext_view(),
            KeyCode::Home => self.screen = Screen::Cosmos,
            KeyCode::Insert => match self.sim.orbit_here() {
                Ok(m) => self.say(m),
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::End => self.screen = Screen::Rogue,
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
            KeyCode::Tab if self.keys.contains(&KeyCode::ShiftLeft) => self.target_planet(),
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
            KeyCode::KeyX => {
                self.fac_sel = self.sim.faction.unwrap_or(self.fac_sel);
                self.screen = Screen::Factions;
            }
            KeyCode::KeyD => match self.sim.try_dock() {
                Ok(_) => self.handle_events(),
                // D also steers (yaw): stay quiet when no station is anywhere near.
                Err(e) if e.starts_with("aucune") || e.starts_with("trop loin") => {}
                Err(e) => self.say(format!("AMARRAGE IMPOSSIBLE: {}", e.to_uppercase())),
            },
            KeyCode::F5 => self.save_game(),
            KeyCode::F12 => self.screen = Screen::Proto,
            KeyCode::F11 => self.toggle_flight_menu(),
            KeyCode::Enter => self.open_dialogue(),
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
            KeyCode::NumpadMultiply => self.open_nav_auto(),
            KeyCode::KeyL => self.open_galaxy(),
            KeyCode::F2 => self.open_empire(),
            KeyCode::F3 => self.open_fleet(),
            KeyCode::KeyZ if !self.settings.azerty => match self.sim.pay_tribute() {
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
            KeyCode::Comma => match self.sim.launch_decoy() {
                Ok(()) => self.say("LEURRE THERMIQUE LANCE"),
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::Period => match self.sim.toggle_active_sensors() {
                Ok(true) => self.say("RADAR ACTIF : VOUS VOYEZ LOIN, ET ON VOUS VOIT DE TRES LOIN"),
                Ok(false) => self.say("RADAR COUPE : DETECTION PASSIVE SEULE"),
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::Slash => match self.sim.board_on_foot() {
                Ok(()) => {
                    self.rogue_from_surface = false;
                    self.screen = Screen::Rogue;
                }
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::F4 => self.open_tech(),
            KeyCode::F8 => self.open_surface(),
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
            (KeyCode::KeyX, Screen::Factions),
            (KeyCode::KeyH, Screen::Ship),
            (KeyCode::KeyI, Screen::Inventory),
            (KeyCode::NumpadMultiply, Screen::NavAuto),
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
    fn command_bar_follows_the_screen_and_can_be_switched_off() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let flight = crate::tutorial::hint_line(&g);
        assert!(flight.contains("BOOST") && flight.contains("FREIN"));
        g.key(KeyCode::KeyM, None, true);
        let map = crate::tutorial::hint_line(&g);
        assert_ne!(map, flight, "the map has its own keys");
        assert!(map.contains("ZOOM"));
        g.key(KeyCode::KeyM, None, true);
        // Every screen has a bar that fits a 1024 px wide window (128 columns).
        for s in [
            Screen::MainMenu,
            Screen::Flight,
            Screen::Station,
            Screen::Inventory,
            Screen::Pause,
            Screen::Ship,
            Screen::Shipyard,
            Screen::Designer,
            Screen::Crew,
            Screen::Recruit,
            Screen::Journal,
            Screen::Galaxy,
            Screen::Contracts,
            Screen::Bank,
            Screen::Livestock,
            Screen::Casino,
            Screen::Trading,
            Screen::Cosmos,
            Screen::Rogue,
            Screen::Empire,
            Screen::Colony,
            Screen::Fleet,
            Screen::Destroyed,
            Screen::Tech,
            Screen::Rules,
            Screen::Options,
            Screen::Codex,
            Screen::Surface,
            Screen::Bourse,
            Screen::Family,
            Screen::Factions,
            Screen::Proto,
            Screen::Dialogue,
            Screen::Expedition,
            Screen::NavAuto,
        ] {
            g.screen = s;
            let line = crate::tutorial::hint_line(&g);
            assert!(
                !line.is_empty() && line.len() <= 126,
                "{s:?}: {} chars",
                line.len()
            );
        }
        // Drawn on the last row by default, absent when the option is off.
        let lit = |g: &Game| {
            let mut fb = vector_render::Framebuffer::new(1024, 576);
            crate::ui::draw_overlay(&mut fb, g);
            let y0 = (fb.rows() - 1) * 10;
            (y0..y0 + 10).any(|y| (0..fb.w).any(|x| fb.px[y * fb.w + x] != 0))
        };
        // (the news banner has its own option and row)
        g.settings.ticker = false;
        g.screen = Screen::Flight;
        assert!(lit(&g));
        g.settings.hints = false;
        assert!(!lit(&g));
        g.settings.ticker = true;
        assert!(
            lit(&g),
            "the banner takes the last row when the command bar is off"
        );
    }

    #[test]
    fn f8_opens_the_surface_of_a_surveyed_world_and_its_keys_work() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        // Nothing targeted, or an unsurveyed world: the screen refuses and says why.
        g.key(KeyCode::F8, None, true);
        assert_eq!(g.screen, Screen::Flight);
        assert!(
            g.messages.last().is_some_and(|m| m.contains("PLANETE")),
            "{:?}",
            g.messages.last()
        );
        g.sim.reveal_all();
        g.cycle_target(|k| matches!(k, BodyKind::Planet(p) if !p.is_giant()));
        g.key(KeyCode::F8, None, true);
        assert_eq!(g.screen, Screen::Surface);
        let map = g.surf.as_ref().expect("map");
        assert_eq!(
            map.biome.len(),
            vector_world::surface::W * vector_world::surface::H
        );
        let (x, y) = (g.surf_cx, g.surf_cy);
        g.key(KeyCode::ArrowRight, None, true);
        g.key(KeyCode::ArrowDown, None, true);
        assert_eq!(
            (g.surf_cx, g.surf_cy),
            ((x + 1) % vector_world::surface::W, y + 1)
        );
        let layer = g.surf_layer;
        g.key(KeyCode::Tab, None, true);
        assert_ne!(g.surf_layer, layer);
        // The BIOSPHERE layer: V changes the species, Enter opens the card and back.
        g.surf_layer = crate::surface_view::BIOSPHERE;
        let sp = g.surf_species;
        g.key(KeyCode::KeyV, None, true);
        let alive = crate::surface_view::alive_species(g.surf_eco.as_ref().expect("eco")).len();
        assert_eq!(g.surf_species, (sp + 1) % alive.max(1));
        g.key(KeyCode::Enter, None, true);
        assert!(g.surf_card);
        g.key(KeyCode::Enter, None, true);
        assert!(!g.surf_card);
        g.surf_layer = layer;
        g.key(KeyCode::KeyG, None, true);
        assert!(g.surf_globe);
        g.key(KeyCode::ArrowRight, None, true);
        assert!(g.surf_spin > 0.0, "left/right turn the globe");
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Flight);
        // A gas giant has no surface.
        g.sim.target = None;
        g.cycle_target(|k| matches!(k, BodyKind::Planet(p) if p.is_giant()));
        if g.sim.target.is_some() {
            g.key(KeyCode::F8, None, true);
            assert_eq!(g.screen, Screen::Flight);
        }
    }

    #[test]
    fn the_exchange_opens_at_a_station_and_its_keys_work() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        // Not docked: no exchange.
        g.open_bourse();
        assert_eq!(g.screen, Screen::Flight);
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.key(KeyCode::KeyD, None, true);
        assert_eq!(g.screen, Screen::Station);
        g.key(KeyCode::KeyT, None, true);
        assert_eq!(g.screen, Screen::Bourse);
        let n = g.data.resources.len();
        assert_eq!(g.bourse_series.len(), n);
        assert_eq!(g.bourse_series[0].len(), BOURSE_RANGES[g.bourse_range]);
        assert_eq!(g.bourse_live.len(), n);
        assert!(g
            .bourse_series
            .iter()
            .flatten()
            .all(|p| p.is_finite() && *p > 0.0));
        let days = g.bourse_series[0].len();
        g.key(KeyCode::Tab, None, true);
        assert_ne!(g.bourse_series[0].len(), days, "Tab changes the period");
        g.key(KeyCode::ArrowDown, None, true);
        assert_eq!(g.bourse_sel, 1);
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Station);
    }

    #[test]
    fn the_family_screen_opens_from_the_crew_and_walks_through_the_members() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.key(KeyCode::KeyD, None, true);
        // A second member, a sibling of the first, so there is someone to walk to.
        let me = g.sim.crew.members[0].kin;
        let sib = vector_sim::genealogy::siblings(&g.sim.crew.families, me)[0];
        let p =
            vector_sim::crew::person_from_kin(&g.data, &g.sim.crew.families, sib, 99, None, None);
        let data = std::sync::Arc::clone(&g.data);
        g.sim.crew.hire(&data, p, 2, 0.0);
        g.key(KeyCode::KeyK, None, true);
        assert_eq!(g.screen, Screen::Crew);
        g.key(KeyCode::KeyG, None, true);
        assert_eq!(g.screen, Screen::Family);
        let ped = g.family_ped.as_ref().expect("pedigree");
        assert_eq!(ped.parents.len(), 2);
        assert!(
            ped.siblings.iter().any(|s| s.kin == sib),
            "the hired brother is listed"
        );
        let first = g.family_member;
        g.key(KeyCode::ArrowRight, None, true);
        assert_ne!(g.family_member, first);
        assert!(g.family_ped.is_some());
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Crew);
    }

    #[test]
    fn the_tech_graph_is_walked_with_the_arrows_and_research_starts_with_enter() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.key(KeyCode::F4, None, true);
        assert_eq!(g.screen, Screen::Tech);
        // It opens on something within reach.
        assert!(g.sim.tech_available(g.tech_sel) || g.sim.current_research == Some(g.tech_sel));
        let start = g.tech_sel;
        // Right goes to what it opens, Left comes back to a prerequisite of it.
        g.key(KeyCode::ArrowRight, None, true);
        if g.tech_sel != start {
            assert!(crate::tech_view::TechLayout::requires(&g.data, g.tech_sel).contains(&start));
            let opened = g.tech_sel;
            g.key(KeyCode::ArrowLeft, None, true);
            assert!(
                crate::tech_view::TechLayout::requires(&g.data, opened).contains(&g.tech_sel),
                "Left walks back to a prerequisite"
            );
        }
        // Up and Down stay in the same column.
        let lay = crate::tech_view::layout(&g.data);
        let col = lay.layer[g.tech_sel];
        g.key(KeyCode::ArrowDown, None, true);
        assert_eq!(lay.layer[g.tech_sel], col);
        // A locked technology cannot be started, an available one can.
        let locked = (0..g.data.techs.len())
            .find(|&t| !g.sim.tech_available(t) && !g.sim.researched[t])
            .expect("a locked tech");
        g.tech_sel = locked;
        g.key(KeyCode::Enter, None, true);
        assert!(g.sim.current_research != Some(locked));
        let open = (0..g.data.techs.len())
            .find(|&t| g.sim.tech_available(t))
            .expect("an available tech");
        g.tech_sel = open;
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.sim.current_research, Some(open));
        g.key(KeyCode::Escape, None, true);
        assert_ne!(g.screen, Screen::Tech);
    }

    fn frame(g: &Game) {
        let mut fb = vector_render::Framebuffer::new(1280, 720);
        crate::draw_frame(&mut fb, g);
    }

    #[test]
    fn the_mouse_clicks_menus_graphs_tabs_maps_and_the_command_bar() {
        let mut g = Game::new(data(), 1);
        // Main menu: a click on the first line starts a game.
        frame(&g);
        let col = 1280 / 8 / 2 - 12;
        g.pointer_click(col * 8 + 20, 13 * 10 + 3, 1000);
        assert_eq!(
            g.screen,
            Screen::Flight,
            "clicking NOUVELLE PARTIE starts the game"
        );
        // Tech graph: a double click on a node starts its research, whichever it is.
        g.key(KeyCode::F4, None, true);
        assert_eq!(g.screen, Screen::Tech);
        frame(&g);
        let zones: Vec<(i32, i32)> = g
            .pointer
            .hits
            .borrow()
            .iter()
            .map(|h| ((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2))
            .collect();
        assert!(zones.len() > 20, "{} zones on the graph", zones.len());
        let mut t = 5000u64;
        for (x, y) in zones {
            g.pointer_click(x, y, t);
            g.pointer_click(x, y, t + 100);
            t += 2000;
            if g.sim.current_research.is_some() {
                break;
            }
        }
        assert!(
            g.sim.current_research.is_some(),
            "a double click started a research"
        );
        // The wheel moves the selection, a right click goes back.
        let sel = g.tech_sel;
        g.pointer_wheel(-1.0);
        assert_ne!(g.tech_sel, sel);
        g.pointer_back();
        assert_ne!(g.screen, Screen::Tech);
        // Exchange: the period tabs and the rows are clickable, and so is the command bar.
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.key(KeyCode::KeyD, None, true);
        g.key(KeyCode::KeyT, None, true);
        assert_eq!(g.screen, Screen::Bourse);
        frame(&g);
        g.pointer_click(78 * 8 + 8, 10 + 3, 20_000);
        assert_eq!(g.bourse_range, 2, "the third tab shows 180 days");
        assert_eq!(g.bourse_series[0].len(), BOURSE_RANGES[2]);
        frame(&g);
        g.pointer_click(2 * 8 + 4, 7 * 10 + 3, 22_000);
        assert_ne!(g.bourse_sel, 0, "a click on a row selects that resource");
        // The last segment of the command bar is "T/ECHAP RETOUR": clicking it goes back.
        frame(&g);
        let last = g
            .pointer
            .hits
            .borrow()
            .last()
            .copied()
            .expect("a command-bar zone");
        g.pointer_click(
            (last.rect.0 + last.rect.2) / 2,
            (last.rect.1 + last.rect.3) / 2,
            30_000,
        );
        assert_eq!(g.screen, Screen::Station);
        // Surface: a click on the map moves the cursor.
        g.sim.reveal_all();
        g.screen = Screen::Flight;
        g.sim.docked = None;
        g.cycle_target(|k| matches!(k, BodyKind::Planet(p) if !p.is_giant()));
        g.open_surface();
        assert_eq!(g.screen, Screen::Surface);
        frame(&g);
        let (x0, y0, cols, rows) = g.surf_geom.get();
        let before = (g.surf_cx, g.surf_cy);
        g.pointer_click(
            ((x0 + cols / 4) * 8) as i32,
            ((y0 + rows / 4) * 10 + 3) as i32,
            40_000,
        );
        assert_ne!((g.surf_cx, g.surf_cy), before);
        assert!(
            g.surf_cx < vector_world::surface::W / 2 && g.surf_cy < vector_world::surface::H / 2
        );
        // Clicking empty space does nothing; the pointer never steers while mouse look is on.
        let screen = g.screen;
        g.pointer_click(1, 715, 50_000);
        assert_eq!(g.screen, screen);
    }

    #[test]
    fn the_mouse_targets_objects_in_flight_and_changes_options_and_follows_the_family() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.screen, Screen::Flight);
        g.settings.tutorial = false;
        // The station sits at the middle of the first view: a click targets it.
        g.sim.target = None;
        frame(&g);
        g.pointer_click(640, 360, 1_000);
        let station = g.sim.target.expect("the click targeted something");
        assert!(matches!(
            g.sim.bodies[station].kind,
            BodyKind::Station | BodyKind::Planet(_) | BodyKind::Moon(_) | BodyKind::Asteroid(_)
        ));
        // A second click on it, quickly, is a double click: the autopilot (or its refusal) answers.
        g.pointer_click(640, 360, 1_200);
        assert!(
            g.sim.autopilot.is_some() || g.messages.last().is_some(),
            "{:?}",
            g.messages.last()
        );
        // The map: the wheel zooms.
        g.key(KeyCode::KeyM, None, true);
        let scale = g.map_scale;
        g.pointer_wheel(1.0);
        assert!(g.map_scale < scale, "wheel up zooms in");
        g.key(KeyCode::KeyM, None, true);
        // Options: the arrows next to a value change it.
        g.screen = Screen::Options;
        frame(&g);
        let before = g.settings.clone();
        // Row 0 is at text row 5; its "<" is at column 38.
        g.pointer_click(38 * 8 + 3, 5 * 10 + 3, 10_000);
        assert_ne!(g.settings, before, "the arrow changed the first option");
        // Family tree: a relative aboard is a button.
        let me = g.sim.crew.members[0].kin;
        let sib = vector_sim::genealogy::siblings(&g.sim.crew.families, me)[0];
        let p =
            vector_sim::crew::person_from_kin(&g.data, &g.sim.crew.families, sib, 99, None, None);
        let data = std::sync::Arc::clone(&g.data);
        g.sim.crew.hire(&data, p, 2, 0.0);
        g.family_member = 0;
        g.refresh_family();
        g.screen = Screen::Family;
        frame(&g);
        // Click every zone of the tree: one of them is the brother, whose tree then opens.
        let zones: Vec<(i32, i32)> = g
            .pointer
            .hits
            .borrow()
            .iter()
            .map(|h| ((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2))
            .collect();
        for (i, (x, y)) in zones.into_iter().enumerate() {
            g.pointer_click(x, y, 100_000 + 1_000 * i as u64);
        }
        assert_eq!(g.family_member, 1, "the brother's tree is shown");
    }

    #[test]
    fn the_factions_screen_opens_on_x_and_a_click_selects_a_faction() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.key(KeyCode::KeyX, None, true);
        assert_eq!(g.screen, Screen::Factions);
        frame(&g);
        let n = g.data.factions.len();
        let target = n - 1;
        let (x, y) = {
            let hits = g.pointer.hits.borrow();
            let h = hits.get(target).expect("one zone per faction");
            ((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2)
        };
        g.pointer_click(x, y, 1_000);
        assert_eq!(g.fac_sel, target);
        g.key(KeyCode::ArrowRight, None, true);
        assert_eq!(g.fac_sel, 0, "the selection wraps around");
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Flight);
    }

    #[test]
    fn s_opens_the_society_page_and_its_buttons_act() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.key(KeyCode::KeyX, None, true);
        assert!(!g.soc_page);
        g.key(KeyCode::KeyS, None, true);
        assert!(g.soc_page, "S shows the society page");
        frame(&g);
        // Not docked: the school, the gallery and the bets refuse.
        g.sim.docked = None;
        g.key(KeyCode::KeyP, None, true);
        assert!(g.sim.society.rounds == 0);
        // Composing works anywhere on the ship.
        g.key(KeyCode::KeyC, None, true);
        assert_eq!(g.sim.society.works.len(), 1);
        frame(&g);
        g.key(KeyCode::KeyA, None, true);
        assert_eq!(g.soc_art, 1);
        g.key(KeyCode::KeyS, None, true);
        assert!(!g.soc_page);
    }

    #[test]
    fn tab_opens_the_relations_registry_and_the_journal_answers_a_surprise() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.key(KeyCode::KeyX, None, true);
        assert!(!g.rel_page);
        g.key(KeyCode::Tab, None, true);
        assert!(g.rel_page, "Tab shows the relations");
        let rows = g.sim.relation_rows().len();
        g.key(KeyCode::ArrowUp, None, true);
        assert_eq!(g.rel_sel, rows - 1, "the selection wraps around");
        frame(&g);
        g.key(KeyCode::Tab, None, true);
        assert!(!g.rel_page);
        g.key(KeyCode::Escape, None, true);
        // A surprise with choices is answered from the journal with the number keys.
        let i = g
            .data
            .surprises
            .iter()
            .position(|s| s.id == "invitation_gala")
            .expect("surprise");
        g.sim
            .surprises
            .pending
            .push(vector_sim::surprises::Pending {
                def: i,
                time: g.sim.time,
                expires: g.sim.time + 1.0e9,
                text: "Une invitation.".to_string(),
            });
        g.key(KeyCode::KeyJ, None, true);
        assert_eq!(g.screen, Screen::Journal);
        frame(&g);
        g.key(KeyCode::Digit2, None, true);
        assert!(g.sim.surprises.pending.is_empty(), "answered");
        g.key(KeyCode::Digit1, None, true);
        assert!(g.sim.surprises.pending.is_empty(), "nothing left to answer");
    }

    #[test]
    fn the_hold_plan_blocks_are_clickable_and_sized_by_volume() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let d = Arc::clone(&g.data);
        let iron = d.resource_index("iron").expect("iron");
        let water = d.resource_index("water").expect("water");
        g.sim.ship.cargo.cap_vol = 40.0;
        g.sim.ship.cargo.cap_mass = 1.0e6;
        g.sim.ship.cargo.add(&d, iron, 5_000.0);
        g.sim.ship.cargo.add(&d, water, 5_000.0);
        g.sel_res = water;
        g.screen = Screen::Inventory;
        frame(&g);
        let zones = g.pointer.hits.borrow().len();
        assert!(zones >= 2, "each block declares zones, got {zones}");
        // The first block (iron, listed first) is under the grid's top-left cell.
        g.pointer_click(20, 104, 1_000);
        assert_eq!(g.sel_res, iron);
        // Water is far more voluminous than the same mass of iron: it has more zone rows.
        let c = &g.sim.ship.cargo;
        assert!(
            c.kg[water] / d.resources[water].density > 5.0 * c.kg[iron] / d.resources[iron].density
        );
    }

    #[test]
    fn the_crew_network_toggles_with_v_and_its_boxes_select_people() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let me = g.sim.crew.members[0].kin;
        let sib = vector_sim::genealogy::siblings(&g.sim.crew.families, me)[0];
        let p =
            vector_sim::crew::person_from_kin(&g.data, &g.sim.crew.families, sib, 99, None, None);
        let data = std::sync::Arc::clone(&g.data);
        g.sim.crew.hire(&data, p, 2, 0.0);
        g.screen = Screen::Crew;
        let n = g.sim.crew.members.len();
        assert!(n >= 2, "two people aboard");
        g.key(KeyCode::KeyV, None, true);
        assert!(g.crew_net);
        frame(&g);
        // The command bar adds zones too; the boxes are the ones with a double-click action.
        let boxes: Vec<_> = g
            .pointer
            .hits
            .borrow()
            .iter()
            .filter(|h| h.double.is_some())
            .copied()
            .collect();
        assert_eq!(boxes.len(), n, "one box per person");
        let (x, y) = {
            let h = boxes[n - 1];
            ((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2)
        };
        g.pointer_click(x, y, 1_000);
        assert_eq!(g.crew_sel, n - 1);
        g.key(KeyCode::KeyV, None, true);
        assert!(!g.crew_net);
    }

    #[test]
    fn the_medicine_tab_opens_with_m_dresses_wounds_and_gives_drugs() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.screen = Screen::Crew;
        g.key(KeyCode::KeyM, None, true);
        assert!(g.med_mode);
        frame(&g);
        // Nothing to dress yet.
        g.key(KeyCode::KeyA, None, true);
        assert!(g
            .messages
            .last()
            .is_some_and(|m| m.contains("RIEN A PANSER")));
        let d = std::sync::Arc::clone(&g.data);
        g.sim.crew.members[0].body.add_wound(&d, "coupure", 2, 0.5);
        g.key(KeyCode::KeyA, None, true);
        assert!(g.sim.crew.members[0].body.wounds[0].dressed > 0.0);
        // A dose of the drug picked with the arrows.
        g.key(KeyCode::ArrowRight, None, true);
        assert_eq!(g.med_drug, 1);
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.sim.crew.members[0].body.drugs.len(), 1);
        frame(&g);
        assert!(
            g.pointer.hits.borrow().len() > 5,
            "the care words are clickable"
        );
        g.key(KeyCode::KeyM, None, true);
        assert!(!g.med_mode);
    }

    #[test]
    fn the_fleet_map_toggles_with_m_and_draws_without_routes() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.key(KeyCode::F3, None, true);
        assert_eq!(g.screen, Screen::Fleet);
        g.key(KeyCode::KeyM, None, true);
        assert!(g.fleet_map);
        frame(&g);
        g.key(KeyCode::KeyM, None, true);
        assert!(!g.fleet_map);
    }

    #[test]
    fn codex_links_follow_names_and_backspace_returns() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.key(KeyCode::F6, None, true);
        // Find a resource entry that names a recipe: it must offer at least one link.
        let (k, links) = (0..crate::codex::count(&g.data, 0))
            .map(|k| (k, crate::codex::links(&g.data, 0, k)))
            .find(|(_, l)| l.iter().any(|(c, _, _)| *c == 5))
            .expect("some resource is made by a named recipe");
        g.codex_cat = 0;
        g.codex_sel = k;
        let (c, j, _) = links.iter().find(|(c, _, _)| *c == 5).cloned().unwrap();
        let n = links
            .iter()
            .position(|(lc, lj, _)| *lc == c && *lj == j)
            .unwrap();
        assert!(n < 9, "the link has a digit key");
        let key = [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
        ][n];
        g.key(key, None, true);
        assert_eq!((g.codex_cat, g.codex_sel), (c, j));
        g.key(KeyCode::Backspace, None, true);
        assert_eq!((g.codex_cat, g.codex_sel), (0, k));
    }

    #[test]
    fn the_organ_keys_operate_the_selected_person() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let d = Arc::clone(&g.data);
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.sim.try_dock().expect("dock");
        g.sim.credits = 1.0e6;
        g.sim.ship.cargo.cap_mass = 1.0e6;
        g.sim.ship.cargo.cap_vol = 1.0e6;
        let lungs = d.organ_index("poumons").expect("lungs");
        let part = d.resource_index("prosthesis_poumons").expect("part");
        g.sim.ship.cargo.add(&d, part, 1.0);
        g.screen = Screen::Crew;
        g.crew_sel = 0;
        g.key(KeyCode::KeyO, None, true);
        assert!(g.organ_mode);
        g.organ_sel = lungs;
        g.sim.crew.members[0].organs[lungs] = 0.2;
        g.key(KeyCode::KeyP, None, true);
        assert_eq!(g.sim.crew.members[0].organs[lungs], 0.85);
        assert_eq!(g.sim.crew.members[0].organ_kind[lungs], 2);
        frame(&g);
        g.key(KeyCode::Escape, None, true);
        assert!(!g.organ_mode);
        assert_eq!(g.screen, Screen::Crew, "Escape only left the organ list");
    }

    #[test]
    fn enter_calls_the_targeted_ship_and_the_answers_work_by_key_and_mouse() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.screen, Screen::Flight);
        g.settings.tutorial = false;
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.screen, Screen::Flight, "nobody targeted: nothing happens");
        g.sim.spawn_traffic();
        let k = g
            .sim
            .npcs
            .iter()
            .position(|n| n.role == vector_sim::npc::NpcRole::Trader)
            .expect("a trader");
        g.sim.target_npc = Some(k);
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.screen, Screen::Dialogue);
        let start = g.dlg_node.clone();
        // Ask for the news by key: we land on the reply node.
        g.key(KeyCode::Digit1, None, true);
        assert_eq!(g.dlg_node, "news_reply");
        assert_ne!(g.dlg_node, start);
        assert!(!g.dlg_log.is_empty());
        // "Autre chose" goes back to the start by mouse (first button).
        frame(&g);
        let (x, y) = {
            let hits = g.pointer.hits.borrow();
            let h = hits.first().expect("answer buttons are zones");
            ((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2)
        };
        g.pointer_click(x, y, 1_000);
        assert_eq!(g.dlg_node, start, "back to the start node");
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Flight);
    }

    #[test]
    fn azerty_is_the_default_and_moves_with_zqsd_and_ae_while_qwerty_keeps_wasd() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.settings = crate::settings::Settings::default();
        assert!(g.settings.azerty, "AZERTY by default");
        g.key(KeyCode::KeyZ, None, true);
        assert!(g.input().pitch > 0.5, "Z pitches up on AZERTY");
        g.key(KeyCode::KeyZ, None, false);
        g.key(KeyCode::KeyQ, None, true);
        assert!(g.input().yaw < -0.5, "Q yaws left on AZERTY");
        g.key(KeyCode::KeyQ, None, false);
        g.key(KeyCode::KeyA, None, true);
        assert!(g.input().roll > 0.5, "A rolls on AZERTY");
        g.key(KeyCode::KeyA, None, false);
        // W does nothing on AZERTY labels.
        g.key(KeyCode::KeyW, None, true);
        assert_eq!(g.input().pitch, 0.0);
        g.key(KeyCode::KeyW, None, false);
        // Z does not pay the tribute on AZERTY (it pitches).
        g.sim.tribute_due = Some((0, 1.0));
        g.key(KeyCode::KeyZ, None, true);
        assert!(g.sim.tribute_due.is_some());
        g.key(KeyCode::KeyZ, None, false);
        // Toggling in the options gives QWERTY.
        g.settings.adjust(6, false);
        assert!(!g.settings.azerty);
        g.key(KeyCode::KeyW, None, true);
        assert!(g.input().pitch > 0.5, "W pitches up on QWERTY");
        // The physical keys are read by label: the key at the QWERTY W position is a Z on AZERTY.
        let az = crate::settings::Settings::default();
        assert_eq!(az.translate(KeyCode::KeyW), KeyCode::KeyZ);
        assert_eq!(az.translate(KeyCode::KeyQ), KeyCode::KeyA);
        assert_eq!(az.translate(KeyCode::Semicolon), KeyCode::KeyM);
        assert_eq!(az.translate(KeyCode::KeyD), KeyCode::KeyD);
        let qw = crate::settings::Settings {
            azerty: false,
            ..az
        };
        assert_eq!(qw.translate(KeyCode::KeyW), KeyCode::KeyW);
        assert!(qw.localize("WASD/QE").contains("WASD"));
        assert!(crate::settings::Settings::default()
            .localize("WASD/QE")
            .contains("ZQSD/AE"));
    }

    #[test]
    fn an_expedition_goes_from_the_surface_map_to_the_exploration_and_back_with_the_loot() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.settings.tutorial = false;
        g.sim.reveal_all();
        let body = g
            .sim
            .bodies
            .iter()
            .position(|b| {
                matches!(
                    b.kind,
                    BodyKind::Planet(
                        vector_world::PlanetKind::Terran
                            | vector_world::PlanetKind::Ocean
                            | vector_world::PlanetKind::Tundra
                            | vector_world::PlanetKind::Desert
                    )
                )
            })
            .expect("a world that can hold life");
        g.sim.target = Some(body);
        g.sim.ship.pos = g.sim.bodies[body].pos
            - vector_core::Vec3::new(0.0, 0.0, g.sim.bodies[body].radius * 3.0);
        g.sim.ship.vel = g.sim.bodies[body].vel;
        g.sim.update_ref_pub();
        g.open_surface();
        assert_eq!(g.screen, Screen::Surface);
        // A land cell with life.
        let mut found = false;
        'find: for cy in (6..vector_world::surface::H - 6).step_by(4) {
            for cx in (0..vector_world::surface::W).step_by(5) {
                if g.sim.rogue_land(body, cx, cy, vec![0], false).is_ok() {
                    let plants = g
                        .sim
                        .rogue
                        .as_ref()
                        .map_or(0, |r| r.floor().items.iter().filter(|i| i.plant).count());
                    g.sim.rogue = None;
                    if plants > 3 {
                        g.surf_cx = cx;
                        g.surf_cy = cy;
                        found = true;
                        break 'find;
                    }
                }
            }
        }
        assert!(found, "seed 1 has a living land cell");
        g.key(KeyCode::KeyE, None, true);
        assert_eq!(g.screen, Screen::Expedition);
        g.key(KeyCode::Space, None, true); // untick the first member...
        g.key(KeyCode::Enter, None, true);
        assert_eq!(
            g.screen,
            Screen::Expedition,
            "nobody selected: no departure"
        );
        g.key(KeyCode::Space, None, true); // ...and tick again
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.screen, Screen::Rogue);
        assert!(g.sim.rogue.as_ref().unwrap().on_surface());
        frame(&g);
        let t0 = g.sim.rogue.as_ref().unwrap().turn;
        // Some step is open around the landing.
        for k in [
            KeyCode::ArrowRight,
            KeyCode::ArrowLeft,
            KeyCode::ArrowDown,
            KeyCode::ArrowUp,
        ] {
            g.key(k, None, true);
        }
        assert!(
            g.sim.rogue.as_ref().unwrap().turn > t0,
            "a move takes a turn"
        );
        g.key(KeyCode::KeyR, None, true);
        assert!(g.sim.rogue.as_ref().unwrap().over);
        frame(&g);
        g.key(KeyCode::Enter, None, true);
        assert_eq!(
            g.screen,
            Screen::Surface,
            "closing the finished expedition goes back to the map"
        );
        assert!(g.sim.rogue.is_none());
    }

    #[test]
    fn the_bank_text_buttons_borrow_and_repay_by_click() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.sim.try_dock().expect("dock");
        g.screen = Screen::Bank;
        frame(&g);
        let loan0 = g.sim.bank.loan;
        // "[+] EMPRUNTER 1000 CR" is the first text button, at text row 8 + 9.
        g.pointer_click(20, 17 * 10 + 3, 1_000);
        assert_eq!(g.sim.bank.loan, loan0 + 1000.0);
        g.pointer_click(20, 18 * 10 + 3, 5_000);
        assert_eq!(g.sim.bank.loan, loan0, "repaid by the second button");
    }

    #[test]
    fn dragging_with_the_left_button_turns_the_globe_only_then() {
        let mut g = Game::new(data(), 1);
        g.screen = Screen::Surface;
        g.surf_globe = true;
        let spin = g.surf_spin;
        g.pointer_move(100, 100);
        g.pointer_move(140, 100);
        assert_eq!(g.surf_spin, spin, "no drag without the button");
        g.pointer.down = true;
        g.pointer_move(190, 100);
        assert!(g.surf_spin > spin + 0.4, "50 px turns the globe");
        g.pointer.down = false;
        let after = g.surf_spin;
        g.pointer_move(300, 100);
        assert_eq!(g.surf_spin, after);
        g.surf_globe = false;
        g.pointer.down = true;
        g.pointer_move(400, 100);
        assert_eq!(g.surf_spin, after, "the flat map is not dragged");
    }

    #[test]
    fn the_piloting_menu_opens_screens_and_holds_ship_controls_by_click() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.settings.tutorial = false;
        g.settings.flight_menu = true;
        frame(&g);
        // Words of the menu are zones: find them by their position (right column).
        let x = 135 * 8 + 20;
        let click = |g: &mut Game, row: i32, ms: u64| g.pointer_click(x, row * 10 + 3, ms);
        // Row 6 is the first menu entry: the system map.
        assert!(!g.map);
        click(&mut g, 6, 1_000);
        assert!(g.map, "CARTE SYSTEME toggled the map");
        click(&mut g, 6, 5_000);
        assert!(!g.map);
        // Row 8 is the ship screen (H): a click opens it.
        click(&mut g, 8, 9_000);
        assert_eq!(g.screen, Screen::Ship);
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Flight);
        // Holding a control word holds the key, and releasing the button lets go.
        // The controls heading is at row 22, their entries from row 23.
        frame(&g);
        let brake_row = 23 + 10; // the 11th control, FREIN (MAINT.)
        click(&mut g, brake_row, 20_000);
        assert!(g.keys.contains(&KeyCode::KeyC), "the brake key is held");
        g.pointer_release();
        assert!(!g.keys.contains(&KeyCode::KeyC), "released with the button");
        // F11 folds the menu away: its zones disappear.
        g.key(KeyCode::F11, None, true);
        assert!(!g.flight_menu_open);
        frame(&g);
        click(&mut g, 6, 30_000);
        assert!(!g.map, "folded: nothing there");
    }

    #[test]
    fn shift_tab_targets_planets_and_moons_only() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.settings.tutorial = false;
        g.key(KeyCode::ShiftLeft, None, true);
        g.key(KeyCode::Tab, None, true);
        let t = g.sim.target.expect("a target");
        assert!(matches!(
            g.sim.bodies[t].kind,
            BodyKind::Planet(_) | BodyKind::Moon(_)
        ));
        g.key(KeyCode::Tab, None, true);
        let t2 = g.sim.target.expect("next");
        assert!(matches!(
            g.sim.bodies[t2].kind,
            BodyKind::Planet(_) | BodyKind::Moon(_)
        ));
        assert_ne!(t, t2, "it cycles");
    }

    #[test]
    fn enter_on_a_targeted_world_with_people_opens_a_conversation_with_them() {
        let mut found = None;
        for seed in 1..60u64 {
            let mut g = Game::new(data(), seed);
            g.key(KeyCode::Enter, None, true);
            if let Some(b) = g.sim.first_civilised_world() {
                found = Some((g, b));
                break;
            }
        }
        let (mut g, b) = found.expect("a seed with a civilised world");
        g.settings.tutorial = false;
        // Too far: a clear refusal and no screen change.
        g.sim.target = Some(b);
        g.sim.target_npc = None;
        g.sim.ship.pos = g.sim.bodies[b].pos
            + vector_core::Vec3::new(0.0, 0.0, g.sim.bodies[b].radius * 900.0 + 5.0e9);
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.screen, Screen::Flight);
        // Near it: the conversation opens, the people speak, the answers work, Escape ends it.
        g.sim.ship.pos =
            g.sim.bodies[b].pos - vector_core::Vec3::new(0.0, 0.0, g.sim.bodies[b].radius * 3.0);
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.screen, Screen::Dialogue);
        frame(&g);
        let view = g
            .sim
            .dialogue_view(g.dlg_npc, &g.dlg_node, &g.dlg_result)
            .expect("view");
        assert_eq!(view.role, "CIVILISATION");
        g.key(KeyCode::Digit2, None, true); // study their mores
        assert_eq!(g.dlg_node, "civ_mores");
        assert!(g.dlg_log.iter().any(|l| l.starts_with("VOUS")));
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Flight);
    }

    #[test]
    fn the_exchange_keys_sign_and_close_futures() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.sim.try_dock().expect("dock");
        g.sim.credits = 1.0e6;
        g.screen = Screen::Station;
        g.open_bourse();
        assert_eq!(g.screen, Screen::Bourse);
        let credits = g.sim.credits;
        g.key(KeyCode::KeyB, None, true);
        assert_eq!(g.sim.futures.len(), 1);
        assert!(
            g.sim.futures[0].long && g.sim.credits < credits,
            "margin locked"
        );
        g.key(KeyCode::KeyN, None, true);
        assert_eq!(g.sim.futures.len(), 2);
        assert!(!g.sim.futures[1].long);
        frame(&g);
        // The clickable word does the same as the key.
        let n = g.sim.futures.len();
        g.key(KeyCode::KeyC, None, true);
        assert_eq!(g.sim.futures.len(), n - 1, "C closes one");
    }

    #[test]
    fn the_equipment_keys_equip_from_the_hold_and_unequip() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let d = Arc::clone(&g.data);
        g.sim.ship.cargo.cap_mass = 1.0e6;
        g.sim.ship.cargo.cap_vol = 1.0e6;
        let item = d.resource_index("gear_fusil").unwrap();
        g.sim.ship.cargo.add(&d, item, 1.0);
        g.screen = Screen::Crew;
        g.crew_sel = 0;
        g.key(KeyCode::KeyE, None, true);
        assert!(g.gear_mode);
        g.gear_sel = d.gear_index("fusil").unwrap();
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.sim.crew.members[0].gear[0], Some(g.gear_sel));
        assert!(g.sim.ship.cargo.kg[item] < 0.5);
        frame(&g);
        g.key(KeyCode::KeyU, None, true);
        assert!(g.sim.crew.members[0].gear[0].is_none());
        assert!(g.sim.ship.cargo.kg[item] >= 0.99, "back in the hold");
        g.key(KeyCode::Escape, None, true);
        assert!(!g.gear_mode);
        assert_eq!(g.screen, Screen::Crew);
    }

    #[test]
    fn the_exchange_has_a_companies_tab_where_shares_are_bought_and_sold() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.sim.try_dock().expect("dock");
        g.sim.credits = 1.0e6;
        g.screen = Screen::Station;
        g.open_bourse();
        assert_eq!(g.bourse_tab, 0);
        g.key(KeyCode::KeyE, None, true);
        assert_eq!(g.bourse_tab, 1);
        assert_eq!(g.comp_series.len(), g.data.companies.len());
        frame(&g);
        g.key(KeyCode::ArrowDown, None, true);
        assert_eq!(g.comp_sel, 1);
        let credits = g.sim.credits;
        g.key(KeyCode::KeyB, None, true);
        assert_eq!(g.sim.shares_of(1), 10.0);
        assert!(g.sim.credits < credits);
        g.key(KeyCode::KeyN, None, true);
        assert_eq!(g.sim.shares_of(1), 0.0);
        // Clicking a company selects it; E goes back to the goods.
        let hit = g
            .pointer
            .hits
            .borrow()
            .iter()
            .copied()
            .find(|h| h.rect.1 > 40 && h.rect.1 < 60);
        if let Some(h) = hit {
            g.pointer_click((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2, 1_000);
        }
        g.key(KeyCode::KeyE, None, true);
        assert_eq!(g.bourse_tab, 0);
    }

    #[test]
    fn the_roguelike_screen_explores_a_station_with_keys_and_the_mouse() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.sim.try_dock().expect("dock");
        g.key(KeyCode::End, None, true);
        assert_eq!(g.screen, Screen::Rogue);
        frame(&g);
        // The station, not one of the worlds that may also be in reach.
        g.rogue_ui.sel = g
            .sim
            .rogue_targets()
            .iter()
            .position(|t| g.data.rogue_themes[t.theme].id == "station")
            .expect("the station is a target");
        g.key(KeyCode::Enter, None, true);
        assert!(g.sim.rogue.is_some(), "set out");
        frame(&g);
        let (x0, y0) = {
            let r = g.sim.rogue.as_ref().unwrap();
            (r.x, r.y)
        };
        // Some direction is open from the entrance room: walk it with the arrows.
        for k in [
            KeyCode::ArrowRight,
            KeyCode::ArrowLeft,
            KeyCode::ArrowDown,
            KeyCode::ArrowUp,
        ] {
            g.key(k, None, true);
        }
        let turns = g.sim.rogue.as_ref().unwrap().turn;
        assert!(turns >= 1, "turns passed ({turns})");
        let _ = (x0, y0);
        // Clicking on a seen floor cell walks toward it.
        let (cx, cy) = {
            let r = g.sim.rogue.as_ref().unwrap();
            let mut best = (r.x, r.y);
            let mut d = 0;
            for y in 0..r.h as i32 {
                for x in 0..r.w as i32 {
                    if r.floor().seen[r.idx(x, y)] && r.tile(x, y) == 1 {
                        let dd = (x - r.x).abs().max((y - r.y).abs());
                        if dd > d {
                            d = dd;
                            best = (x, y);
                        }
                    }
                }
            }
            best
        };
        frame(&g);
        let before = g.sim.rogue.as_ref().unwrap().turn;
        let (camx, camy) = crate::rogue_view::camera(g.sim.rogue.as_ref().unwrap());
        g.pointer_click(8 + 16 * (cx - camx) + 3, 30 + 20 * (cy - camy) + 3, 1_000);
        assert!(g.sim.rogue.as_ref().unwrap().turn > before, "walked");
        frame(&g);
        g.sim.rogue.as_mut().unwrap().over = true;
        frame(&g);
        g.key(KeyCode::Enter, None, true);
        assert!(g.sim.rogue.is_none(), "closed");
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Station);
    }

    #[test]
    fn the_cosmos_screen_sets_anchors_rewinds_and_sleeps() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.key(KeyCode::Home, None, true);
        assert_eq!(g.screen, Screen::Cosmos);
        frame(&g);
        g.sim.credits = 1.0e6;
        g.key(KeyCode::Enter, None, true);
        assert!(g.sim.multiverse.dimension == 0, "no portal in range here");
        g.key(KeyCode::KeyA, None, true);
        assert_eq!(g.sim.multiverse.anchors.len(), 1);
        let t = g.sim.time;
        g.sim.time += 5.0 * 86_400.0;
        g.key(KeyCode::KeyR, None, true);
        assert!((g.sim.time - t).abs() < 1.0, "rewound");
        assert_eq!(g.sim.multiverse.paradox, 1);
        frame(&g);
        g.key(KeyCode::KeyX, None, true);
        assert!(g.sim.multiverse.anchors.is_empty());
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Flight);
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.sim.try_dock().expect("dock");
        g.screen = Screen::Station;
        g.key(KeyCode::Home, None, true);
        let t = g.sim.time;
        g.key(KeyCode::KeyS, None, true);
        assert!(g.sim.time > t + 20.0 * 86_400.0, "slept 30 days");
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Station);
    }

    #[test]
    fn the_brokerage_tabs_work_from_the_station() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.sim.try_dock().expect("dock");
        g.sim.credits = 1.0e6;
        g.screen = Screen::Station;
        g.key(KeyCode::KeyD, None, true);
        assert_eq!(g.screen, Screen::Trading);
        for tab in 0..crate::trade_view::TABS.len() {
            assert_eq!(g.trade_ui.tab, tab);
            frame(&g);
            g.key(KeyCode::ArrowRight, None, true);
        }
        assert_eq!(g.trade_ui.tab, 0);
        // Cartels: a seat is bought and ceded with the same key.
        g.key(KeyCode::ArrowRight, None, true);
        g.key(KeyCode::ArrowRight, None, true);
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.sim.trade.seats, vec![0]);
        g.key(KeyCode::Enter, None, true);
        assert!(g.sim.trade.seats.is_empty());
        // Tenders: a bid at 60 % of the ceiling may be beaten, 5 % always wins.
        g.key(KeyCode::ArrowRight, None, true);
        for _ in 0..15 {
            g.key(KeyCode::Minus, None, true);
        }
        g.key(KeyCode::Enter, None, true);
        assert!(g
            .sim
            .contracts
            .iter()
            .any(|c| c.dest_name == g.sim.bodies[g.sim.docked.unwrap()].name));
        frame(&g);
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Station);
    }

    #[test]
    fn the_economy_tabs_buy_insure_bid_and_draw() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.sim.try_dock().expect("dock");
        g.sim.credits = 1.0e6;
        g.sim.time = 200.0 * 86_400.0;
        g.screen = Screen::Station;
        g.key(KeyCode::KeyD, None, true);
        let gold = g.data.resource_index("gold").unwrap();
        g.sim.ship.cargo.cap_mass = 1.0e7;
        g.sim.ship.cargo.cap_vol = 1.0e7;
        let data = std::sync::Arc::clone(&g.data);
        g.sim.ship.cargo.add(&data, gold, 100.0);
        // Insurance tab (4): the Enter key and the buttons insure the hold, then the hull.
        g.trade_ui.tab = 4;
        let c = g.sim.credits;
        g.key(KeyCode::Enter, None, true);
        assert!(g.sim.insured() && g.sim.credits < c, "policy bought");
        g.click_eco(501);
        assert!(g.sim.trade.hull_covered > 0.0);
        frame(&g);
        // Titles (5): bonds and a fund; the net worth stays about the same minus fees.
        g.trade_ui.tab = 5;
        let nw = g.sim.net_worth();
        g.click_eco(100);
        g.click_eco(120);
        assert_eq!(g.sim.trade.folio.bonds.len(), 1);
        assert!(g.sim.fund_units(0) > 0.0);
        assert!(
            g.sim.net_worth() > nw - 0.02 * 2000.0 - 1.0,
            "only fees are lost"
        );
        g.trade_ui.sel = gold;
        g.click_eco(130);
        assert_eq!(g.sim.trade.folio.options.len(), 1);
        frame(&g);
        // Sales (6): a bid is held, an ad is run.
        g.trade_ui.tab = 6;
        g.click_eco(200);
        assert_eq!(g.sim.trade.dealings.bids.len(), 1);
        g.click_eco(240);
        assert_eq!(g.sim.trade.dealings.ads.len(), 1);
        frame(&g);
        // Lines (7) and the economic board (8) draw, and a jurisdiction can be picked.
        g.trade_ui.tab = 7;
        frame(&g);
        g.trade_ui.tab = 8;
        frame(&g);
        g.click_eco(401);
        assert_eq!(g.trade_ui.sel, 1);
        frame(&g);
        // The arrows move through the goods on the goods tabs.
        g.trade_ui.tab = 5;
        g.trade_ui.sel = 0;
        g.key(KeyCode::ArrowDown, None, true);
        assert_eq!(g.trade_ui.sel, 1);
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Station);
    }

    #[test]
    fn the_pens_and_the_casino_work_from_the_station() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.sim.try_dock().expect("dock");
        g.sim.credits = 1.0e6;
        g.screen = Screen::Station;
        g.key(KeyCode::KeyL, None, true);
        assert_eq!(g.screen, Screen::Livestock);
        frame(&g);
        g.key(KeyCode::KeyB, None, true);
        g.key(KeyCode::KeyB, None, true);
        assert_eq!(g.sim.herd.len(), 2);
        frame(&g);
        g.key(KeyCode::KeyH, None, true);
        assert_eq!(g.sim.herd.len(), 2, "no slaughter at the dealer's");
        g.key(KeyCode::KeyS, None, true);
        assert_eq!(g.sim.herd.len(), 1, "an animal was sold to the dealer");
        // The pens aboard the ship: slaughter there, no buying there.
        g.farm.dealer = false;
        g.key(KeyCode::KeyB, None, true);
        assert_eq!(g.sim.herd.len(), 1, "no dealer in the pens");
        g.key(KeyCode::KeyH, None, true);
        assert_eq!(
            g.sim.herd.len(),
            0,
            "an adult animal was slaughtered aboard"
        );
        g.farm.dealer = true;
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Station);
        g.key(KeyCode::KeyA, None, true);
        assert_eq!(g.screen, Screen::Casino);
        let credits = g.sim.credits;
        g.key(KeyCode::Enter, None, true);
        assert_ne!(g.sim.credits, credits, "a round was played");
        assert!(g.farm.last.is_some());
        frame(&g);
        g.key(KeyCode::ArrowRight, None, true);
        g.key(KeyCode::ArrowRight, None, true);
        g.key(KeyCode::Equal, None, true);
        assert_eq!(g.farm.number, 1);
        g.key(KeyCode::Enter, None, true);
        frame(&g);
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Station);
    }

    #[test]
    fn the_mouse_selects_contract_rows() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let offer = vector_sim::economy::Contract {
            pax: 0,
            pax_kind: 0,
            legs: Vec::new(),
            guide: false,
            origin: [0; 3],
            id: 1,
            dest_star: g.sim.here,
            dest_seed: 0,
            dest_body: 0,
            dest_name: "X".into(),
            dest_system: "X".into(),
            dist_ly: 0.0,
            resource: 0,
            kg: 10.0,
            reward: 100.0,
            expires: g.sim.time + 86_400.0,
            faction: None,
        };
        g.offers = vec![offer.clone(), offer.clone(), offer];
        g.screen = Screen::Contracts;
        g.contract_sel = 0;
        frame(&g);
        // The third offer is drawn at text row 8 + 1 + 2.
        g.pointer_click(40, 11 * 10 + 3, 1_000);
        assert_eq!(g.contract_sel, 2);
    }

    #[test]
    fn menus_reach_rules_options_and_codex() {
        let mut g = Game::new(data(), 1);
        // Main menu: REGLES (index 3) -> raise damage x -> back -> new game uses it.
        g.key(KeyCode::ArrowDown, None, true);
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
        let pitch_up = g.settings.move_keys()[0];
        g.key(pitch_up, None, true);
        assert_eq!(g.tut_step, Some(1), "rotating completes step 1");
        g.key(KeyCode::Space, None, true);
        g.key(KeyCode::KeyF, None, true);
        assert_eq!(g.tut_step, Some(3));
        g.key(KeyCode::Delete, None, true);
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
            Screen::Designer,
            Screen::Crew,
            Screen::Recruit,
            Screen::Journal,
            Screen::Galaxy,
            Screen::Contracts,
            Screen::Bank,
            Screen::Livestock,
            Screen::Casino,
            Screen::Trading,
            Screen::Cosmos,
            Screen::Rogue,
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
            (KeyCode::KeyL, Screen::Livestock),
            (KeyCode::KeyD, Screen::Trading),
            (KeyCode::KeyA, Screen::Casino),
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
        g.settings.azerty = false;
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

    #[test]
    fn the_war_page_hires_spies_and_answers_every_button() {
        use crate::war_view::{W_AGENT, W_CELL, W_PEACE, W_RECRUIT};
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        assert_eq!(g.screen, Screen::Flight);
        g.key(KeyCode::KeyX, None, true);
        assert_eq!(g.screen, Screen::Factions);
        assert!(!g.war_page);
        g.key(KeyCode::KeyG, None, true);
        assert!(g.war_page, "G opens the war page");
        // Hiring needs a quay.
        g.click_war(W_RECRUIT);
        assert!(g.sim.military.agents.is_empty());
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.docked = Some(st);
        g.sim.credits = 100_000.0;
        g.key(KeyCode::KeyR, None, true);
        assert_eq!(g.sim.military.agents.len(), 1, "R recruits");
        assert!(g.sim.credits < 100_000.0);
        g.click_war(W_AGENT);
        g.click_war(W_CELL);
        // No war with the people selected: peace is refused with a reason.
        g.click_war(W_PEACE);
        assert!(g.messages.last().is_some_and(|m| m.contains("GUERRE")));
        // Escape leaves the page, X leaves the screen.
        g.key(KeyCode::Escape, None, true);
        assert!(!g.war_page && g.screen == Screen::Factions);
        g.key(KeyCode::KeyX, None, true);
        assert_eq!(g.screen, Screen::Flight);
    }

    #[test]
    fn flight_keys_for_flares_radar_and_boarding_answer_with_a_reason() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g.key(KeyCode::Comma, None, true);
        assert!(g
            .messages
            .last()
            .is_some_and(|m| m.contains("LANCE-LEURRES")));
        g.key(KeyCode::Period, None, true);
        assert!(g.messages.last().is_some_and(|m| m.contains("RADAR")));
        g.key(KeyCode::Slash, None, true);
        assert!(g.messages.last().is_some_and(|m| m.contains("CIBLE")));
        assert_eq!(g.screen, Screen::Flight);
    }

    #[test]
    fn the_fleet_screen_gives_orders_and_judges_prizes() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.docked = Some(st);
        let frigate = g.data.hull_index("frigate").expect("hull");
        g.sim.buy_fleet_ship_free_pub(frigate);
        g.screen = Screen::Fleet;
        assert_eq!(g.sim.fleet[0].order, vector_sim::fleet::FleetOrder::Trade);
        g.key(KeyCode::KeyO, None, true);
        assert_eq!(g.sim.fleet[0].order, vector_sim::fleet::FleetOrder::Follow);
        g.key(KeyCode::KeyO, None, true);
        assert_eq!(g.sim.fleet[0].order, vector_sim::fleet::FleetOrder::Attack);
        // A prize with prisoners: key 1 enlists them.
        g.sim.fleet[0].prize = true;
        g.sim.fleet[0].prisoners = 5;
        g.key(KeyCode::Digit1, None, true);
        assert_eq!(g.sim.fleet[0].prisoners, 0);
        g.key(KeyCode::Digit1, None, true);
        assert!(g.messages.last().is_some_and(|m| m.contains("PRISONNIERS")));
    }

    fn press(g: &mut Game, k: KeyCode) {
        g.key(k, None, true);
        g.key(k, None, false);
    }

    /// Walk the new-game flow from the main menu to the last step.
    fn to_recap(g: &mut Game) {
        g.menu_sel = 1;
        press(g, KeyCode::Enter);
        assert_eq!(g.screen, Screen::NewGame);
        for _ in 0..6 {
            press(g, KeyCode::Enter);
        }
        assert_eq!(g.ng.step, 6, "the recap is the seventh step");
    }

    #[test]
    fn the_new_game_flow_walks_back_and_forth_and_leaves_to_the_main_menu() {
        let mut g = Game::new(data(), 1);
        g.menu_sel = 1;
        press(&mut g, KeyCode::Enter);
        assert_eq!((g.screen, g.ng.step), (Screen::NewGame, 0));
        press(&mut g, KeyCode::Enter);
        press(&mut g, KeyCode::Enter);
        assert_eq!(g.ng.step, 2);
        press(&mut g, KeyCode::Escape);
        assert_eq!(g.ng.step, 1, "Escape goes up one step");
        press(&mut g, KeyCode::Escape);
        press(&mut g, KeyCode::Escape);
        assert_eq!(
            g.screen,
            Screen::MainMenu,
            "the first step goes back to the menu"
        );
        // F10 does nothing in the menus of the game itself.
        press(&mut g, KeyCode::F10);
        assert_eq!(g.screen, Screen::MainMenu);
    }

    #[test]
    fn every_scenario_chosen_in_the_flow_starts_a_playable_game() {
        for i in 0..data().scenarios.len() {
            let mut g = Game::new(data(), 1);
            g.save_path = std::env::temp_dir().join("vector_test_flow.sav");
            to_recap(&mut g);
            g.click_ng_scenario(i);
            g.ng_goto(6);
            press(&mut g, KeyCode::Enter);
            let (id, wreck, credits) = {
                let sc = &g.data.scenarios[i];
                (sc.id.clone(), sc.wreck, sc.credits)
            };
            let expected = if wreck { Screen::Rogue } else { Screen::Flight };
            assert_eq!(g.screen, expected, "{id}");
            assert!((g.sim.credits - credits).abs() < 1e-9, "{id} credits");
            assert_eq!(g.sim.rogue.is_some(), wreck, "{id}");
            // It draws and runs.
            frame(&g);
            g.advance(1.0);
            // The menu works at once, wherever the game begins.
            press(&mut g, KeyCode::F10);
            assert_eq!(g.screen, Screen::Hub, "{id}");
            frame(&g);
            press(&mut g, KeyCode::Escape);
            assert_eq!(g.screen, expected, "{id}");
        }
    }

    #[test]
    fn the_flow_choices_reach_the_world_ship_crew_seed_and_rules() {
        let mut g = Game::new(data(), 1);
        to_recap(&mut g);
        let trader = g.data.scenario_index("marchand").expect("sc");
        g.click_ng_scenario(trader);
        g.click_ng_ship(1);
        g.ng.seed_text = "ANDROMEDE".into();
        g.ng_goto(4);
        press(&mut g, KeyCode::Backspace);
        assert_eq!(g.ng.seed_text, "ANDROMED");
        g.key(KeyCode::KeyX, Some("X"), true);
        assert_eq!(g.ng.seed_text, "ANDROMEDX", "typed text goes to the seed");
        g.ng.opt_sel = 3;
        g.ng_goto(5);
        press(&mut g, KeyCode::ArrowRight);
        assert_eq!(
            vector_sim::start::difficulty_of(&g.pending_rules),
            Some(2),
            "the difficulty line sets the rules"
        );
        g.ng_goto(6);
        press(&mut g, KeyCode::Enter);
        assert_eq!(g.screen, Screen::Flight);
        let hauler = g.data.hull_index("hauler").expect("hull");
        assert_eq!(g.sim.ship.design.hull, hauler, "the chosen ship");
        assert_eq!(g.sim.rules.damage, 1.5, "the chosen difficulty");
        let direct = scene::new_sim(parse_seed("ANDROMEDX"), Arc::clone(&g.data));
        assert_eq!(g.sim.here_pos, direct.here_pos, "the typed seed");
        assert!(g.sim.credits >= 3000.0);
    }

    #[test]
    fn the_crew_step_keeps_to_the_points_and_the_roles_change() {
        let mut g = Game::new(data(), 1);
        to_recap(&mut g);
        let castaway = g.data.scenario_index("naufrage").expect("sc");
        g.click_ng_scenario(castaway);
        g.ng_goto(3);
        let budget = g.data.scenarios[castaway].crew_points;
        // Try to take everybody: the budget stops it.
        for k in 0..g.ng.pool.len() {
            g.ng.cursor = k;
            press(&mut g, KeyCode::Space);
            assert!(g.ng_points_used() <= budget, "never over {budget} points");
        }
        let n = g.ng.picks.len();
        assert!(
            n >= 1 && n < g.ng.pool.len(),
            "the budget refused some: {n}"
        );
        // The last person cannot be taken off.
        let all: Vec<usize> = g.ng.picks.iter().map(|p| p.candidate).collect();
        for k in &all[1..] {
            g.ng.cursor = *k;
            press(&mut g, KeyCode::Space);
        }
        g.ng.cursor = all[0];
        press(&mut g, KeyCode::Space);
        assert_eq!(g.ng.picks.len(), 1, "somebody stays aboard");
        // Left/Right change the post of the person under the cursor.
        let role = g.ng.picks[0].role;
        press(&mut g, KeyCode::ArrowRight);
        assert_ne!(g.ng.picks[0].role, role);
        // The chosen crew is the crew of the game.
        g.ng_goto(6);
        press(&mut g, KeyCode::Enter);
        assert_eq!(g.sim.crew.alive().count(), 1);
        assert_eq!(g.sim.crew.members[0].role, g.ng.picks[0].role);
    }

    #[test]
    fn the_castaway_flow_plays_on_foot_then_in_the_cockpit() {
        let mut g = Game::new(data(), 1);
        to_recap(&mut g);
        g.click_ng_scenario(g.data.scenario_index("naufrage").expect("sc"));
        g.ng_goto(6);
        press(&mut g, KeyCode::Enter);
        assert_eq!(g.screen, Screen::Rogue);
        assert!(
            g.tut_step.is_none(),
            "the cockpit tutorial does not apply on foot"
        );
        assert!(!g.sim.readiness().can_fly());
        // The menu tells the situation.
        let o = crate::hub::orientation(&g);
        assert!(o.place.starts_with("A PIED"), "{}", o.place);
        assert!(
            o.todo.iter().any(|l| l.contains("NE PEUT PAS VOLER")),
            "{:?}",
            o.todo
        );
        // Cockpit screens are refused on foot, information screens work and come back to the walk.
        press(&mut g, KeyCode::F10);
        g.hub_activate(crate::hub::COMMERCE, 0);
        assert_eq!(g.screen, Screen::Hub, "no market on foot");
        g.hub_activate(crate::hub::VAISSEAU, 0);
        assert_eq!(g.screen, Screen::Ship);
        press(&mut g, KeyCode::Escape);
        assert_eq!(
            (g.screen, g.hub.sel, g.hub.page),
            (Screen::Hub, crate::hub::VAISSEAU, true)
        );
        press(&mut g, KeyCode::Escape);
        press(&mut g, KeyCode::Escape);
        assert_eq!(g.screen, Screen::Rogue, "the menu closes onto the walk");
        // Fuel and spare parts found on foot: the tank fills from the hold (key F of the hold).
        let d = Arc::clone(&g.data);
        for (id, kg) in [("fuel", 300.0), ("spare_parts", 200.0)] {
            let r = d.resource_index(id).expect("res");
            g.sim.ship.cargo.add(&d, r, kg);
        }
        g.screen = Screen::Inventory;
        press(&mut g, KeyCode::KeyF);
        assert!(g.sim.ship.fuel > 100.0, "{}", g.sim.ship.fuel);
        assert!(crate::hub::orientation(&g)
            .todo
            .iter()
            .any(|l| l.contains("MOTEUR 0 %")));
    }

    #[test]
    fn the_menu_opens_on_every_screen_and_escape_climbs_one_level_at_a_time() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        // From the cockpit.
        press(&mut g, KeyCode::F10);
        assert_eq!(g.screen, Screen::Hub);
        assert!(!g.running(), "the menu freezes the flight");
        press(&mut g, KeyCode::F10);
        assert_eq!(g.screen, Screen::Flight, "F10 again closes it");
        // A space, an entry, its screen, and the way back up.
        press(&mut g, KeyCode::F10);
        press(&mut g, KeyCode::Digit4);
        assert!(g.hub.page && g.hub.sel == crate::hub::EQUIPAGE);
        press(&mut g, KeyCode::Enter);
        assert_eq!(g.screen, Screen::Crew, "EQUIPAGE opens the crew screen");
        press(&mut g, KeyCode::Escape);
        assert_eq!(
            (g.screen, g.hub.sel, g.hub.page),
            (Screen::Hub, crate::hub::EQUIPAGE, true)
        );
        press(&mut g, KeyCode::Escape);
        assert!(
            !g.hub.page && g.screen == Screen::Hub,
            "then the list of spaces"
        );
        press(&mut g, KeyCode::Escape);
        assert_eq!(g.screen, Screen::Flight, "then where the menu was opened");
        // Two levels deep: crew -> family tree -> back to the crew -> back to the page.
        g.hub_activate(crate::hub::EQUIPAGE, 1);
        assert_eq!(g.screen, Screen::Family);
        press(&mut g, KeyCode::Escape);
        assert_eq!(g.screen, Screen::Crew);
        press(&mut g, KeyCode::Escape);
        assert_eq!(g.screen, Screen::Hub);
        press(&mut g, KeyCode::Escape);
        press(&mut g, KeyCode::Escape);
        // The same on a screen that is not the cockpit.
        g.key(KeyCode::KeyJ, None, true);
        assert_eq!(g.screen, Screen::Journal);
        press(&mut g, KeyCode::F10);
        assert_eq!(
            g.hub.sel,
            crate::hub::CODEX,
            "the space of the screen is selected"
        );
        press(&mut g, KeyCode::Escape);
        assert_eq!(
            g.screen,
            Screen::Journal,
            "the menu closes onto the screen it came from"
        );
    }

    #[test]
    fn menu_lines_that_need_a_dock_or_a_flight_say_why_and_work_when_they_can() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        // Flying: no market.
        g.hub_activate(crate::hub::COMMERCE, 1);
        assert_eq!(g.screen, Screen::Flight, "nothing opened");
        assert!(
            g.messages.last().is_some_and(|m| m.contains("AMARREZ")),
            "{:?}",
            g.messages.last()
        );
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        g.key(KeyCode::KeyD, None, true);
        assert_eq!(g.screen, Screen::Station);
        // Docked: the exchange opens, and Escape comes back to the page of the space.
        press(&mut g, KeyCode::F10);
        assert_eq!(g.hub.sel, crate::hub::COMMERCE);
        g.hub_activate(crate::hub::COMMERCE, 1);
        assert_eq!(g.screen, Screen::Bourse);
        press(&mut g, KeyCode::Escape);
        assert_eq!((g.screen, g.hub.page), (Screen::Hub, true));
        // Docked: the map of the flight is refused, the cockpit line undocks.
        g.hub_activate(crate::hub::NAVIGATION, 0);
        assert_eq!(g.screen, Screen::Hub);
        assert!(g.messages.last().is_some_and(|m| m.contains("DECOLLEZ")));
        g.hub_activate(crate::hub::PILOTAGE, 0);
        assert_eq!(g.screen, Screen::Flight);
        assert!(g.sim.docked.is_none(), "the cockpit line took off");
    }

    #[test]
    fn the_menu_and_the_flow_are_clickable_text() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        // The MENU button at the top right of the cockpit.
        frame(&g);
        let col = 1280 / 8 - "MENU (F10)".len() - 1;
        g.pointer_click((col * 8 + 10) as i32, 3, 1000);
        assert_eq!(g.screen, Screen::Hub);
        // A space, then an entry.
        frame(&g);
        let zones: Vec<(i32, i32)> = g
            .pointer
            .hits
            .borrow()
            .iter()
            .map(|h| ((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2))
            .collect();
        assert!(zones.len() >= 8 + 3, "{} zones", zones.len());
        // The third space line: VAISSEAU.
        let top = zones.iter().skip(1).take(8).copied().nth(2).expect("zone");
        g.pointer_click(top.0, top.1, 2000);
        assert!(g.hub.page && g.hub.sel == crate::hub::VAISSEAU);
        frame(&g);
        let entry = g
            .pointer
            .hits
            .borrow()
            .iter()
            .filter(|h| h.rect.0 == 46 * 8 - 2)
            .map(|h| ((h.rect.0 + h.rect.2) / 2, h.rect.1 + 4))
            .next()
            .expect("an entry");
        g.pointer_click(entry.0, entry.1, 3000);
        assert_eq!(
            g.screen,
            Screen::Ship,
            "a click on MODULES opens the ship screen"
        );
        g.pointer_back();
        assert_eq!(g.screen, Screen::Hub, "a right click climbs one level");
        // The flow: its steps and its buttons are clickable.
        let mut g = Game::new(data(), 1);
        g.menu_sel = 1;
        press(&mut g, KeyCode::Enter);
        frame(&g);
        // The label of the third step is a button.
        let x3 = 2 + "1 EXPLICATIONS".len() + 3 + "2 SCENARIO".len() + 3;
        g.pointer_click((x3 * 8 + 4) as i32, 3 * 10 + 3, 1000);
        assert_eq!(g.ng.step, 2, "a click on a step goes there");
        frame(&g);
        // A right click goes back one step.
        g.pointer_back();
        assert_eq!(g.ng.step, 1);
    }

    #[test]
    fn every_screen_has_a_command_bar_a_help_and_a_place_in_the_menu() {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        for s in crate::hub::ALL_SCREENS {
            g.screen = s;
            let line = crate::tutorial::hint_line(&g);
            assert!(
                !line.is_empty() && line.len() <= 126,
                "{s:?}: {} chars",
                line.len()
            );
            assert!(!crate::tutorial::help_lines(s).is_empty(), "{s:?}");
            let _ = crate::hub::place_of(s);
        }
        g.open_new_game();
        for step in 0..7 {
            g.ng.step = step;
            let line = crate::tutorial::hint_line(&g);
            assert!(line.len() <= 126, "step {step}: {} chars", line.len());
            let mut fb = vector_render::Framebuffer::new(1280, 720);
            crate::draw_frame(&mut fb, &g);
        }
    }
}
