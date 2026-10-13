//! NAVIGATION AUTO: the computer that flies the plan. The cockpit shows a small panel (state, order
//! in hand, what the predicted path meets) and draws the predicted path on the map and in the sky;
//! the screen of the menu (F10, space NAVIGATION) edits the queue of orders, plans a trip (the
//! options with their cost and risks), routes FTL jumps and sets the thresholds that stop the
//! computer or send the ship home. Everything here is a view of `vector_sim::nav_auto`: no rule
//! lives in this file.
use crate::game::{Game, Screen};
use crate::hud::HOVER;
use crate::hud::{fmt_dist, fmt_time, View, AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use std::cell::RefCell;
use vector_core::Vec3;
use vector_render::{Camera, Framebuffer};
use vector_sim::nav_auto::fmt_eta;
use vector_sim::nav_auto::{Cond, Order, Run, Status, MAX_ORDERS};
use vector_sim::nav_plan::{PlanOption, Route};
use vector_sim::nav_predict::{ManeuverNode, NodeWhen, PredKind, Prediction};
use vector_sim::Sim;
use winit::keyboard::KeyCode;

/// The kinds of order the editor makes.
pub const KINDS: [&str; 17] = [
    "ORBITE",
    "TRANSFERT",
    "MANOEUVRE",
    "APPROCHER",
    "ATTERRIR",
    "DECOLLER VERS L'ORBITE",
    "AMARRER",
    "QUITTER LE QUAI",
    "MAINTENIR L'ORBITE",
    "ESCORTER",
    "PATROUILLE",
    "ATTENDRE",
    "ATTENDRE QUE",
    "RETOUR A LA STATION",
    "SAUT FTL",
    "ROUTE FTL",
    "SUSPENDRE LE PLAN",
];

const WHENS: [&str; 5] = [
    "DANS (MIN)",
    "AU PERIAPSIS",
    "A L'APOAPSIS",
    "NOEUD ASCENDANT",
    "NOEUD DESCENDANT",
];

const GUARDS: [&str; 8] = [
    "TOUJOURS",
    "SI CARBURANT <",
    "SI CARBURANT >",
    "SI COQUE <",
    "SI COQUE >",
    "SI DOSE >",
    "SI AMARRE",
    "SI EN VOL",
];

const PAGES: [&str; 5] = ["PLAN", "PLANIFICATEUR", "SEUILS", "JOURNAL", "VOYAGE"];

/// The prediction shown in the cockpit and on the map, kept between frames.
#[derive(Default)]
pub struct PredCache {
    key: (u64, usize, Option<usize>),
    pred: Option<Prediction>,
}

/// The state of the screen and of the editor.
pub struct NavUi {
    pub page: usize,
    pub sel: usize,
    pub kind: usize,
    pub slot: usize,
    /// Values of the editor: meaning depends on the kind (see `params`).
    pub v: [f64; 6],
    pub guard: usize,
    pub guard_x: f64,
    pub waypoints: Vec<usize>,
    /// Planner: altitude (km), the options found and the one chosen, the FTL route.
    pub plan_alt: f64,
    pub options: Vec<PlanOption>,
    pub opt_sel: usize,
    pub route: Option<Result<Route, String>>,
    /// Voyage page: the destinations offered, the chosen one and the plan computed for it.
    pub vy_dests: Vec<(String, vector_sim::far::Dest)>,
    pub vy_sel: usize,
    pub vy_plan: Option<Result<vector_sim::far::VoyagePlan, String>>,
    /// Draw the predicted path (the plan has to be non-empty, or this on).
    pub preview: bool,
    pub cache: RefCell<PredCache>,
}

impl Default for NavUi {
    fn default() -> Self {
        NavUi {
            page: 0,
            sel: 0,
            kind: 0,
            slot: 0,
            v: [300.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            guard: 0,
            guard_x: 20.0,
            waypoints: Vec::new(),
            plan_alt: 300.0,
            options: Vec::new(),
            opt_sel: 0,
            route: None,
            vy_dests: Vec::new(),
            vy_sel: 0,
            vy_plan: None,
            preview: false,
            cache: RefCell::new(PredCache::default()),
        }
    }
}

fn body_name(g: &Game, i: usize) -> String {
    g.sim
        .bodies
        .get(i)
        .map_or("?".into(), |b| b.name.to_uppercase())
}

// ---- the editor -------------------------------------------------------------------------------

impl NavUi {
    /// Default values of the editor for a kind.
    fn reset_values(&mut self) {
        self.slot = 0;
        self.v = match self.kind {
            0 => [300.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            1 => [300.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            2 => [100.0, 0.0, 0.0, 1.0, 10.0, 0.0],
            8 => [300.0, 0.0, 2.0, 0.0, 0.0, 0.0],
            9 => [5.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            10 => [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            11 => [10.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            12 => [50.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            _ => [0.0; 6],
        };
    }

    /// The lines of the editor: (label, value) of each parameter of the chosen kind.
    pub fn params(&self) -> Vec<(&'static str, String)> {
        let v = &self.v;
        let km = |x: f64| format!("{x:.0} KM");
        match self.kind {
            0 => vec![
                ("ALTITUDE DU PERIAPSIS", km(v[0])),
                (
                    "ALTITUDE DE L'APOAPSIS",
                    if v[1] <= 0.0 {
                        "CIRCULAIRE".into()
                    } else {
                        km(v[1])
                    },
                ),
            ],
            1 => vec![
                ("ALTITUDE D'ORBITE", km(v[0])),
                (
                    "AEROFREINAGE",
                    if v[1] > 0.5 { "OUI" } else { "NON" }.into(),
                ),
            ],
            2 => vec![
                (
                    "QUAND",
                    WHENS[(v[3] as usize).min(WHENS.len() - 1)].to_string(),
                ),
                ("PROGRADE", format!("{:+.1} M/S", v[4])),
                ("NORMAL", format!("{:+.1} M/S", v[5])),
                ("RADIAL", format!("{:+.1} M/S", v[2])),
                ("DANS (MIN)", format!("{:.0}", v[0])),
            ],
            8 => vec![
                ("ALTITUDE DU PERIAPSIS", km(v[0])),
                (
                    "ALTITUDE DE L'APOAPSIS",
                    if v[1] <= 0.0 {
                        "CIRCULAIRE".into()
                    } else {
                        km(v[1])
                    },
                ),
                ("TOLERANCE", format!("{:.0} %", v[2])),
                (
                    "DUREE",
                    if v[3] <= 0.0 {
                        "SANS FIN".into()
                    } else {
                        format!("{:.0} H", v[3])
                    },
                ),
            ],
            9 => vec![
                ("DISTANCE", km(v[0])),
                (
                    "DUREE",
                    if v[1] <= 0.0 {
                        "SANS FIN".into()
                    } else {
                        format!("{:.0} MIN", v[1])
                    },
                ),
            ],
            10 => vec![
                ("POINTS DU CIRCUIT", format!("{}", self.waypoints.len())),
                (
                    "TOURS",
                    if v[0] <= 0.0 {
                        "SANS FIN".into()
                    } else {
                        format!("{:.0}", v[0])
                    },
                ),
            ],
            11 => vec![("DUREE", format!("{:.0} MIN", v[0]))],
            12 => vec![
                (
                    "CONDITION",
                    GUARDS[self.guard.clamp(1, GUARDS.len() - 1)].to_string(),
                ),
                ("SEUIL", format!("{:.2}", self.guard_x)),
            ],
            _ => Vec::new(),
        }
    }

    /// Change the parameter `slot` of the editor one step up (`dir` = 1) or down.
    fn adjust(&mut self, slot: usize, dir: f64) {
        let mul = |x: f64, lo: f64, hi: f64, dir: f64| -> f64 {
            (if dir > 0.0 {
                x * 1.25 + 1.0
            } else {
                x / 1.25 - 1.0
            })
            .clamp(lo, hi)
        };
        match (self.kind, slot) {
            (0 | 8, 0) => self.v[0] = mul(self.v[0], 20.0, 1.0e6, dir).round(),
            (0 | 8, 1) => {
                let x = if self.v[1] <= 0.0 {
                    self.v[0]
                } else {
                    self.v[1]
                };
                let n = mul(x, 20.0, 1.0e6, dir).round();
                self.v[1] = if dir < 0.0 && n <= self.v[0] { 0.0 } else { n };
            }
            (1, 0) => self.v[0] = mul(self.v[0], 20.0, 1.0e6, dir).round(),
            (1, 1) => self.v[1] = 1.0 - self.v[1].round().clamp(0.0, 1.0),
            (2, 0) => {
                self.v[3] = (self.v[3] + dir).rem_euclid(WHENS.len() as f64);
            }
            (2, 1) => self.v[4] += dir * step_dv(self.v[4]),
            (2, 2) => self.v[5] += dir * step_dv(self.v[5]),
            (2, 3) => self.v[2] += dir * step_dv(self.v[2]),
            (2, 4) => self.v[0] = (self.v[0] + dir * 10.0).max(0.0),
            (8, 2) => self.v[2] = (self.v[2] + dir).clamp(1.0, 20.0),
            (8, 3) => self.v[3] = (self.v[3] + dir * 6.0).max(0.0),
            (9, 0) => self.v[0] = mul(self.v[0], 0.2, 5000.0, dir),
            (9, 1) => self.v[1] = (self.v[1] + dir * 10.0).max(0.0),
            (10, 1) => self.v[0] = (self.v[0] + dir).max(0.0),
            (11, 0) => self.v[0] = mul(self.v[0], 1.0, 1.0e6, dir).round(),
            (12, 0) => {
                self.guard = ((self.guard as isize - 1 + dir as isize).rem_euclid(7) + 1) as usize
            }
            (12, 1) => self.guard_x = (self.guard_x + dir * 5.0).max(0.0),
            _ => {}
        }
    }
}

fn step_dv(x: f64) -> f64 {
    let a = x.abs();
    if a < 10.0 {
        1.0
    } else if a < 100.0 {
        5.0
    } else if a < 1000.0 {
        25.0
    } else {
        100.0
    }
}

impl Game {
    /// The order the editor describes, with the target of the game as the body.
    fn nav_make_order(&self) -> Result<Order, String> {
        let ui = &self.nav_ui;
        let target = self.sim.target.filter(|&t| t < self.sim.bodies.len());
        let body = || target.ok_or("CIBLEZ D'ABORD UN CORPS (TAB)");
        let km = |x: f64| x * 1000.0;
        let v = &ui.v;
        Ok(match ui.kind {
            0 => {
                let b = self.sim.ref_body.ok_or("HORS D'UNE SPHERE D'INFLUENCE")?;
                let apo = if v[1] <= 0.0 { v[0] } else { v[1] };
                Order::Orbit {
                    body: b,
                    peri_alt: km(v[0].min(apo)),
                    apo_alt: km(v[0].max(apo)),
                    stage: 0,
                }
            }
            1 => Order::Transfer {
                target: body()?,
                alt: km(v[0]),
                aero: v[1] > 0.5,
            },
            2 => {
                let when = match v[3] as usize {
                    0 => NodeWhen::At(self.sim.time + v[0] * 60.0),
                    1 => NodeWhen::Periapsis,
                    2 => NodeWhen::Apoapsis,
                    3 => NodeWhen::Ascending,
                    _ => NodeWhen::Descending,
                };
                Order::Burn(ManeuverNode::new(when, self.sim.ref_body, v[4], v[5], v[2]))
            }
            3 => Order::Approach { body: body()? },
            4 => Order::Land,
            5 => Order::Takeoff,
            6 => Order::Dock,
            7 => Order::Undock,
            8 => {
                let b = self.sim.ref_body.ok_or("HORS D'UNE SPHERE D'INFLUENCE")?;
                let apo = if v[1] <= 0.0 { v[0] } else { v[1] };
                Order::KeepOrbit {
                    body: b,
                    peri_alt: km(v[0].min(apo)),
                    apo_alt: km(v[0].max(apo)),
                    tol: v[2] / 100.0,
                    duration: v[3] * 3600.0,
                }
            }
            9 => {
                let id = self
                    .sim
                    .target_npc
                    .and_then(|k| self.sim.npcs.get(k))
                    .map(|n| n.id)
                    .ok_or("CIBLEZ UN VAISSEAU (R)")?;
                Order::Escort {
                    npc: id,
                    dist: km(v[0]),
                    duration: v[1] * 60.0,
                }
            }
            10 => {
                if ui.waypoints.is_empty() {
                    return Err("AJOUTEZ DES POINTS AU CIRCUIT (CIBLE PUIS [+ POINT])".into());
                }
                Order::Patrol {
                    waypoints: ui.waypoints.clone(),
                    laps: v[0] as u16,
                }
            }
            11 => Order::Wait(v[0] * 60.0),
            12 => Order::WaitCond(self.nav_cond_of(ui.guard.clamp(1, 7), ui.guard_x)),
            13 => Order::ReturnStation,
            14 | 15 => {
                let star = self
                    .gal_cache
                    .get(self.gal_sel)
                    .ok_or("CHOISISSEZ UNE ETOILE DANS LA GALAXIE (L)")?
                    .id;
                if ui.kind == 14 {
                    Order::Jump { star }
                } else {
                    Order::Route { dest: star }
                }
            }
            _ => Order::Halt,
        })
    }

    fn nav_cond_of(&self, guard: usize, x: f64) -> Cond {
        match guard {
            1 => Cond::FuelBelow(x / 100.0),
            2 => Cond::FuelAbove(x / 100.0),
            3 => Cond::HullBelow(x / 100.0),
            4 => Cond::HullAbove(x / 100.0),
            5 => Cond::DoseAbove(x / 100.0),
            6 => Cond::Docked,
            _ => Cond::Flying,
        }
    }

    pub fn open_nav_auto(&mut self) {
        if self.gal_cache.is_empty() {
            let here = self.sim.here;
            self.gal_cache = self
                .sim
                .galaxy
                .nearby(self.sim.here_pos, 45.0)
                .into_iter()
                .filter(|e| e.id != here && self.sim.is_seen(e.pos))
                .collect();
        }
        self.screen = Screen::NavAuto;
    }

    /// Add the order of the editor to the queue (with its guard).
    pub fn nav_add_order(&mut self) {
        let order = match self.nav_make_order() {
            Ok(o) => o,
            Err(e) => {
                self.say(e);
                return;
            }
        };
        let guard = (self.nav_ui.guard > 0 && self.nav_ui.kind != 12)
            .then(|| self.nav_cond_of(self.nav_ui.guard, self.nav_ui.guard_x));
        match self.sim.nav_add_guarded(order, guard) {
            Ok(()) => {
                self.nav_ui.sel = self.sim.nav.orders.len().saturating_sub(1);
                self.say("ORDRE AJOUTE A LA FILE");
            }
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    /// Start, pause or resume the plan.
    pub fn nav_toggle_run(&mut self) {
        match self.sim.nav.run.clone() {
            Run::Running => self.sim.nav_pause(),
            _ => match self.sim.nav_start() {
                Ok(()) => {}
                Err(e) => self.say(e.to_uppercase()),
            },
        }
        self.handle_events();
    }

    fn nav_calc_options(&mut self) {
        let alt = self.nav_ui.plan_alt * 1000.0;
        let mut out = Vec::new();
        if let Some(t) = self.sim.target.filter(|&t| t < self.sim.bodies.len()) {
            if self.sim.ref_body != Some(t) {
                out.extend(self.sim.plan_transfer_options(t, alt));
            }
        }
        if let Some(b) = self.sim.ref_body {
            if !matches!(self.sim.bodies[b].kind, vector_world::BodyKind::Station) {
                out.extend(self.sim.plan_orbit_options(b, alt, alt));
            }
        }
        if out.is_empty() {
            self.say("CIBLEZ UN CORPS (TAB) OU ENTREZ DANS UNE SPHERE D'INFLUENCE");
        }
        self.nav_ui.options = out;
        self.nav_ui.opt_sel = 0;
    }

    fn nav_adopt(&mut self, run: bool) {
        let Some(o) = self.nav_ui.options.get(self.nav_ui.opt_sel).cloned() else {
            return;
        };
        if !o.ok {
            self.say(o.why.unwrap_or("PLAN IMPOSSIBLE".into()));
            return;
        }
        for order in o.orders {
            if let Err(e) = self.sim.nav_add(order) {
                self.say(e.to_uppercase());
                return;
            }
        }
        self.nav_ui.page = 0;
        self.nav_ui.sel = self.sim.nav.orders.len().saturating_sub(1);
        self.say("PLAN ADOPTE : LES ETAPES SONT DANS LA FILE");
        if run {
            self.nav_toggle_run();
        }
    }

    /// Clicks and keys of the screen share these numbers.
    pub fn nav_click(&mut self, id: usize) {
        match id {
            0 => self.nav_toggle_run(),
            1 => {
                let s = self.nav_ui.sel;
                self.sim.nav_remove(s);
                self.nav_ui.sel = s.min(self.sim.nav.orders.len().saturating_sub(1));
            }
            2 => {
                let s = self.nav_ui.sel;
                self.sim.nav_move(s, true);
                self.nav_ui.sel = s.saturating_sub(1);
            }
            3 => {
                let s = self.nav_ui.sel;
                self.sim.nav_move(s, false);
                self.nav_ui.sel = (s + 1).min(self.sim.nav.orders.len().saturating_sub(1));
            }
            4 => {
                self.sim.nav_clear_done();
                self.nav_ui.sel = 0;
            }
            5 => {
                self.sim.nav_clear();
                self.nav_ui.sel = 0;
            }
            6 => self.nav_add_order(),
            7 => {
                // add the target to the patrol circuit
                if let Some(t) = self.sim.target.filter(|&t| t < self.sim.bodies.len()) {
                    self.nav_ui.waypoints.push(t);
                    self.nav_ui.waypoints.truncate(12);
                } else {
                    self.say("CIBLEZ UN CORPS (TAB)");
                }
            }
            8 => self.nav_ui.waypoints.clear(),
            9 => self.nav_ui.preview = !self.nav_ui.preview,
            10..=14 => {
                self.nav_ui.page = id - 10;
                if id == 14 && self.nav_ui.vy_dests.is_empty() {
                    self.voyage_refresh();
                }
            }
            90 => self.voyage_compute(),
            91 => self.voyage_start(),
            92 => match self.sim.voyage_resume() {
                Ok(_) => self.say("VOYAGE REPRIS"),
                Err(e) => self.say(e),
            },
            93 => {
                self.sim.voyage_cancel();
                self.say("VOYAGE ANNULE");
            }
            94 => self.voyage_refresh(),
            100..=115 => self.nav_ui.vy_sel = id - 100,
            20 => {
                self.nav_ui.kind = (self.nav_ui.kind + KINDS.len() - 1) % KINDS.len();
                self.nav_ui.reset_values();
            }
            21 => {
                self.nav_ui.kind = (self.nav_ui.kind + 1) % KINDS.len();
                self.nav_ui.reset_values();
            }
            22 => self.nav_ui.guard = (self.nav_ui.guard + 1) % GUARDS.len(),
            23 => self.nav_ui.guard_x = (self.nav_ui.guard_x + 5.0).min(100.0),
            24 => self.nav_ui.guard_x = (self.nav_ui.guard_x - 5.0).max(0.0),
            30..=39 => self.nav_ui.slot = id - 30,
            40..=59 => {
                let slot = (id - 40) / 2;
                let dir = if (id - 40).is_multiple_of(2) {
                    1.0
                } else {
                    -1.0
                };
                self.nav_ui.slot = slot;
                self.nav_ui.adjust(slot, dir);
            }
            60 => self.nav_calc_options(),
            61 => self.nav_adopt(false),
            62 => self.nav_adopt(true),
            63 => {
                self.nav_ui.plan_alt = (self.nav_ui.plan_alt * 1.25 + 1.0).min(1.0e6).round();
            }
            64 => {
                self.nav_ui.plan_alt = (self.nav_ui.plan_alt / 1.25 - 1.0).max(20.0).round();
            }
            65 => {
                let r = self
                    .gal_cache
                    .get(self.gal_sel)
                    .map(|s| self.sim.plan_route(s.id));
                match r {
                    Some(r) => self.nav_ui.route = Some(r),
                    None => self.say("CHOISISSEZ UNE ETOILE DANS LA GALAXIE (L)"),
                }
            }
            66 => {
                if let Some(Ok(_)) = &self.nav_ui.route {
                    if let Some(s) = self.gal_cache.get(self.gal_sel).map(|s| s.id) {
                        match self.sim.nav_add(Order::Route { dest: s }) {
                            Ok(()) => {
                                self.say("ROUTE FTL AJOUTEE A LA FILE");
                                self.nav_ui.page = 0;
                            }
                            Err(e) => self.say(e.to_uppercase()),
                        }
                    }
                }
            }
            67 => {
                let n = self.gal_cache.len().max(1);
                self.gal_sel = (self.gal_sel + 1) % n;
                self.nav_ui.route = None;
            }
            68 => {
                self.nav_ui.opt_sel = (self.nav_ui.opt_sel + 1) % self.nav_ui.options.len().max(1);
            }
            // thresholds
            70 | 71 => {
                let d = if id == 70 { 0.05 } else { -0.05 };
                let l = &mut self.sim.nav.limits;
                l.fuel_return = (l.fuel_return + d).clamp(0.0, 0.9);
            }
            72 | 73 => {
                let d = if id == 72 { 0.05 } else { -0.05 };
                let l = &mut self.sim.nav.limits;
                l.hull_return = (l.hull_return + d).clamp(0.0, 0.9);
            }
            74 | 75 => {
                let l = &mut self.sim.nav.limits;
                l.dose_return = if id == 74 {
                    (l.dose_return * 1.5 + 0.01).min(10.0)
                } else {
                    (l.dose_return / 1.5 - 0.01).max(0.01)
                };
            }
            76 => {
                let l = &mut self.sim.nav.limits;
                l.emergency = !l.emergency;
            }
            77 | 78 => {
                let d = if id == 77 { 0.01 } else { -0.01 };
                let l = &mut self.sim.nav.limits;
                l.reserve = (l.reserve + d).clamp(0.0, 0.5);
            }
            79 => self.sim.nav_emergency_now(),
            80 => self.screen = Screen::Flight,
            _ => {}
        }
    }

    pub fn nav_key(&mut self, code: KeyCode) {
        let n = self.sim.nav.orders.len();
        match code {
            KeyCode::Escape | KeyCode::NumpadMultiply => {
                self.screen = Screen::Flight;
                self.map = false;
            }
            KeyCode::Tab => self.nav_ui.page = (self.nav_ui.page + 1) % PAGES.len(),
            _ if self.nav_ui.page == 0 => match code {
                KeyCode::ArrowUp if self.nav_ui.sel > 0 => self.nav_ui.sel -= 1,
                KeyCode::ArrowDown if self.nav_ui.sel + 1 < n => self.nav_ui.sel += 1,
                KeyCode::Space => self.nav_click(0),
                KeyCode::Delete => self.nav_click(1),
                KeyCode::PageUp => self.nav_click(2),
                KeyCode::PageDown => self.nav_click(3),
                KeyCode::KeyC => self.nav_click(4),
                KeyCode::ArrowLeft => self.nav_click(20),
                KeyCode::ArrowRight => self.nav_click(21),
                KeyCode::Enter => self.nav_click(6),
                KeyCode::KeyG => self.nav_click(22),
                KeyCode::Equal | KeyCode::NumpadAdd => {
                    let s = self.nav_ui.slot;
                    self.nav_click(40 + 2 * s);
                }
                KeyCode::Minus | KeyCode::NumpadSubtract => {
                    let s = self.nav_ui.slot;
                    self.nav_click(41 + 2 * s);
                }
                KeyCode::Digit1
                | KeyCode::Digit2
                | KeyCode::Digit3
                | KeyCode::Digit4
                | KeyCode::Digit5 => {
                    let k = match code {
                        KeyCode::Digit1 => 0,
                        KeyCode::Digit2 => 1,
                        KeyCode::Digit3 => 2,
                        KeyCode::Digit4 => 3,
                        _ => 4,
                    };
                    self.nav_ui.slot = k.min(self.nav_ui.params().len().saturating_sub(1));
                }
                _ => {}
            },
            _ if self.nav_ui.page == 4 => match code {
                KeyCode::ArrowUp => {
                    let n = self.nav_ui.vy_dests.len().max(1);
                    self.nav_ui.vy_sel = (self.nav_ui.vy_sel + n - 1) % n;
                    self.nav_ui.vy_plan = None;
                }
                KeyCode::ArrowDown => {
                    let n = self.nav_ui.vy_dests.len().max(1);
                    self.nav_ui.vy_sel = (self.nav_ui.vy_sel + 1) % n;
                    self.nav_ui.vy_plan = None;
                }
                KeyCode::Enter => self.nav_click(90),
                KeyCode::Space => self.nav_click(91),
                KeyCode::KeyP => self.nav_click(92),
                KeyCode::KeyX => self.nav_click(93),
                KeyCode::KeyR => self.nav_click(94),
                _ => {}
            },
            _ if self.nav_ui.page == 1 => match code {
                KeyCode::Enter => self.nav_click(60),
                KeyCode::ArrowUp => {
                    let n = self.nav_ui.options.len().max(1);
                    self.nav_ui.opt_sel = (self.nav_ui.opt_sel + n - 1) % n;
                }
                KeyCode::ArrowDown => self.nav_click(68),
                KeyCode::Space => self.nav_click(61),
                KeyCode::KeyR => self.nav_click(65),
                KeyCode::Equal | KeyCode::NumpadAdd => self.nav_click(63),
                KeyCode::Minus | KeyCode::NumpadSubtract => self.nav_click(64),
                _ => {}
            },
            _ => {}
        }
    }
}

// ---- the prediction ---------------------------------------------------------------------------

/// The predicted path of the ship with the nodes of the queue (cached between frames).
pub(crate) fn with_pred<R>(g: &Game, f: impl FnOnce(&Prediction) -> R) -> Option<R> {
    let sim = &g.sim;
    let nodes = sim.nav_nodes();
    if !g.nav_ui.preview && sim.nav.pending() == 0 {
        return None;
    }
    let key = (sim.steps / 6, nodes.len(), sim.target);
    let mut c = g.nav_ui.cache.borrow_mut();
    if c.pred.is_none() || c.key != key {
        let h = (sim.default_horizon() * (1.0 + nodes.len() as f64)).clamp(600.0, 400.0 * 86_400.0);
        c.pred = Some(sim.predict_ship(h, &nodes, sim.target));
        c.key = key;
    }
    c.pred.as_ref().map(f)
}

pub(crate) fn event_label(g: &Game, e: &vector_sim::nav_predict::PredEvent) -> (String, u32) {
    let sim = &g.sim;
    let name = |i: usize| {
        sim.bodies
            .get(i)
            .map_or("?".into(), |b| b.name.to_uppercase())
    };
    let alt = |i: usize, d: f64| d - sim.bodies.get(i).map_or(0.0, |b| b.radius);
    match e.kind {
        PredKind::Periapsis => (
            format!(
                "PERIAPSIS {} {}",
                name(e.body),
                fmt_dist(alt(e.body, e.value))
            ),
            GREEN,
        ),
        PredKind::Apoapsis => (
            format!(
                "APOAPSIS {} {}",
                name(e.body),
                fmt_dist(alt(e.body, e.value))
            ),
            GREEN,
        ),
        PredKind::SoiEnter => (format!("ENTREE SPHERE {}", name(e.body)), AMBER),
        PredKind::SoiExit => (format!("SORTIE SPHERE {}", name(e.body)), AMBER),
        PredKind::Closest => (
            format!("RENCONTRE {} A {}", name(e.body), fmt_dist(e.value)),
            AMBER,
        ),
        PredKind::Impact => (
            format!("IMPACT SUR {} A {:.0} M/S", name(e.body), e.value),
            RED,
        ),
        PredKind::Node(k) => (format!("MANOEUVRE {} : {:.0} M/S", k + 1, e.value), AMBER),
    }
}

/// Where a predicted position is drawn: as an offset from the body that holds the ship, taken
/// from where that body is now (the path of an orbit stays a closed curve on a moving world).
fn shown(sim: &Sim, center: usize, t: f64, pos: Vec3) -> Vec3 {
    if center == 0 || center >= sim.bodies.len() {
        pos
    } else {
        sim.bodies[center].pos + (pos - sim.system.state(center, t).0)
    }
}

/// The predicted path in the sky of the cockpit: a line, with tags at the notable points.
pub fn draw_cockpit_overlay(fb: &mut Framebuffer, v: &View, cam: &Camera) {
    let g = v.game;
    with_pred(g, |p| {
        let sim = &g.sim;
        let mut prev: Option<(Vec3, usize)> = None;
        for pt in &p.points {
            let q = shown(sim, pt.center, pt.t, pt.pos);
            if let Some((a, c)) = prev {
                if c == pt.center {
                    cam.draw_line(fb, a, q, 0x0060_C0FF);
                }
            }
            prev = Some((q, pt.center));
        }
        for e in p.events.iter().take(12) {
            let at = shown(sim, e.center, e.t, e.pos);
            if let Some((x, y)) = cam.project_world(at, fb) {
                let (label, col) = event_label(g, e);
                fb.line(x - 3.0, y, x + 3.0, y, col);
                fb.line(x, y - 3.0, x, y + 3.0, col);
                fb.text_at(x as i32 + 6, y as i32 - 4, &label, col);
            }
        }
    });
}

/// The predicted path on the map of the system.
pub fn draw_map_overlay(fb: &mut Framebuffer, v: &View, to_px: &dyn Fn(Vec3) -> (f32, f32)) {
    let g = v.game;
    with_pred(g, |p| {
        let sim = &g.sim;
        let mut prev: Option<((f32, f32), usize)> = None;
        for pt in &p.points {
            let q = to_px(shown(sim, pt.center, pt.t, pt.pos));
            if let Some((a, c)) = prev {
                if c == pt.center {
                    fb.line(a.0, a.1, q.0, q.1, 0x0060_C0FF);
                }
            }
            prev = Some((q, pt.center));
        }
        for e in p.events.iter().take(16) {
            let (x, y) = to_px(shown(sim, e.center, e.t, e.pos));
            let (label, col) = event_label(g, e);
            fb.circle(x, y, 3.0, col);
            fb.text_at(x as i32 + 5, y as i32 - 10, &label, col);
        }
    });
}

// ---- the cockpit panel ------------------------------------------------------------------------

/// The state line of the plan.
fn state_text(g: &Game) -> (String, u32) {
    let nav = &g.sim.nav;
    match &nav.run {
        Run::Running => ("EN COURS".into(), GREEN),
        Run::Paused => ("EN PAUSE".into(), AMBER),
        Run::Halted(m) => (format!("ARRETE : {m}"), RED),
        Run::Idle if nav.pending() > 0 => ("PRET".into(), DIM),
        Run::Idle => ("INACTIF".into(), DIM),
    }
}

/// The small panel of the cockpit: shown while a plan exists or runs.
pub fn draw_panel(fb: &mut Framebuffer, v: &View) {
    let g = v.game;
    let sim = v.sim;
    if !v.map {
        let cam = Camera {
            pos: sim.ship.pos,
            rot: g.view_rot(&sim.ship.rot),
            fov_y: if v.boost { 1.4 } else { 1.2 },
        };
        draw_cockpit_overlay(fb, v, &cam);
    }
    if sim.nav.orders.is_empty() && !g.nav_ui.preview {
        return;
    }
    let (cols, rows) = (fb.cols(), fb.rows());
    if rows < 40 || cols < 100 {
        return;
    }
    // Left of the flight menu (the right edge) and of the target panel (low right).
    let x = cols.saturating_sub(70);
    let mut y = 9;
    fb.text(x, y, "== NAVIGATION AUTO ==", AMBER);
    y += 1;
    let (st, col) = state_text(g);
    let st: String = st.chars().take(40).collect();
    fb.text(x, y, &st, col);
    y += 1;
    let cur = sim.nav.cursor();
    if let Some(e) = sim.nav.orders.get(cur) {
        let t: String = sim.order_text(&e.order, e.guard).chars().take(38).collect();
        fb.text(
            x,
            y,
            &format!("> {t}"),
            if sim.nav_running() { GREEN } else { DIM },
        );
        y += 1;
        if let Some(a) = sim.nav.active.as_ref() {
            if let Some(r) = a.resolved {
                if matches!(e.order, Order::Burn(_)) {
                    fb.text(
                        x,
                        y,
                        &format!(
                            "ALLUMAGE DANS {}  ({:?})",
                            fmt_eta((r.t - sim.time).max(0.0)),
                            a.phase
                        ),
                        GREEN,
                    );
                    y += 1;
                }
            }
        }
    }
    fb.text(
        x,
        y,
        &format!(
            "{} A FAIRE  DV {:.0} M/S",
            sim.nav.pending(),
            sim.ship.delta_v()
        ),
        DIM,
    );
    y += 1;
    with_pred(g, |p| {
        let mut shown = 0;
        for e in p.events.iter().filter(|e| e.t > sim.time).take(5) {
            let (label, col) = event_label(g, e);
            let line: String = format!("T+{:>6} {label}", fmt_eta(e.t - sim.time))
                .chars()
                .take(40)
                .collect();
            fb.text(x, y, &line, col);
            y += 1;
            shown += 1;
        }
        if shown == 0 {
            fb.text(x, y, "AUCUN EVENEMENT PREVU", DIM);
            y += 1;
        }
    });
    let mut cx = x;
    let run_label = if sim.nav_running() {
        "[PAUSE]"
    } else {
        "[LANCER]"
    };
    for (label, id) in [(run_label, 0usize), ("[ECRAN]", 90), ("[TRAJ.]", 9)] {
        let hov = g.hit_cells(
            cx,
            y,
            label.len(),
            1,
            Act::Call(Game::nav_click_cockpit, id),
            None,
        );
        fb.text(cx, y, label, if hov { HOVER } else { GREEN });
        cx += label.len() + 1;
    }
}

impl Game {
    /// Clicks of the cockpit panel (90 opens the screen).
    pub fn nav_click_cockpit(&mut self, id: usize) {
        if id == 90 {
            self.open_nav_auto();
        } else {
            self.nav_click(id);
        }
    }
}

// ---- the screen -------------------------------------------------------------------------------

fn button(
    fb: &mut Framebuffer,
    g: &Game,
    col: usize,
    row: usize,
    label: &str,
    id: usize,
    on: bool,
) -> usize {
    let hov = g.hit_cells(
        col,
        row,
        label.len(),
        1,
        Act::Call(Game::nav_click, id),
        None,
    );
    fb.text(
        col,
        row,
        label,
        if hov {
            HOVER
        } else if on {
            AMBER
        } else {
            GREEN
        },
    );
    col + label.len() + 2
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let ui = &g.nav_ui;
    fb.text(1, 1, "NAVIGATION AUTO", AMBER);
    let (st, col) = state_text(g);
    fb.text(20, 1, &st, col);
    let mut x = 1;
    for (i, p) in PAGES.iter().enumerate() {
        let label = format!("[{p}]");
        x = button(fb, g, x, 2, &label, 10 + i, ui.page == i);
    }
    button(fb, g, 100, 2, "[RETOUR AU COCKPIT]", 80, false);
    fb.text(
        1,
        3,
        "L'ORDINATEUR EXECUTE LE PLAN ET REND LA MAIN A LA MOINDRE TOUCHE OU AU PREMIER DANGER.",
        DIM,
    );
    match ui.page {
        0 => draw_plan(fb, g),
        1 => draw_planner(fb, g),
        2 => draw_limits(fb, g),
        3 => draw_log(fb, g),
        _ => draw_voyage(fb, g),
    }
}

fn draw_plan(fb: &mut Framebuffer, g: &Game) {
    let ui = &g.nav_ui;
    let sim = &g.sim;
    let lines = sim.nav_lines();
    let cursor = sim.nav.cursor();
    fb.text(
        1,
        5,
        &format!("FILE D'ORDRES ({}/{})", lines.len(), MAX_ORDERS),
        AMBER,
    );
    let visible = 16usize;
    let sel = ui.sel.min(lines.len().saturating_sub(1));
    let start = sel
        .saturating_sub(visible / 2)
        .min(lines.len().saturating_sub(visible));
    for (i, (text, status)) in lines.iter().enumerate().skip(start).take(visible) {
        let y = 6 + i - start;
        let mark = match status {
            Status::Done => 'v',
            Status::Skipped => '-',
            Status::Failed => 'x',
            Status::Pending if i == cursor && sim.nav_running() => '>',
            Status::Pending => ' ',
        };
        let color = match status {
            Status::Done => DIM,
            Status::Skipped => DIM,
            Status::Failed => RED,
            Status::Pending if i == cursor && sim.nav_running() => GREEN,
            Status::Pending => AMBER,
        };
        let t: String = text.chars().take(86).collect();
        let hov = g.hit_cells(1, y, 90, 1, Act::Call(Game::nav_pick, i), None);
        fb.text(
            1,
            y,
            &format!(
                "{}{:>2} {} {}",
                if i == sel { ">" } else { " " },
                i + 1,
                mark,
                t
            ),
            if hov {
                HOVER
            } else if i == sel {
                AMBER
            } else {
                color
            },
        );
    }
    if lines.is_empty() {
        fb.text(
            2,
            7,
            "FILE VIDE : COMPOSEZ UN ORDRE CI-DESSOUS, OU ADOPTEZ UN PLAN DU PLANIFICATEUR.",
            DIM,
        );
    }
    // commands
    let mut x = 1;
    let run = if sim.nav_running() {
        "[PAUSE]"
    } else {
        "[LANCER]"
    };
    x = button(fb, g, x, 23, run, 0, sim.nav_running());
    x = button(fb, g, x, 23, "[RETIRER]", 1, false);
    x = button(fb, g, x, 23, "[MONTER]", 2, false);
    x = button(fb, g, x, 23, "[DESCENDRE]", 3, false);
    x = button(fb, g, x, 23, "[VIDER LES FAITS]", 4, false);
    button(fb, g, x, 23, "[TOUT VIDER]", 5, false);
    fb.text(
        1,
        24,
        "ESPACE LANCER/PAUSE  SUPPR RETIRER  PGHAUT/PGBAS DEPLACER  C VIDER LES FAITS  TAB PAGE SUIVANTE",
        DIM,
    );
    // the editor
    fb.text(1, 26, "NOUVEL ORDRE", AMBER);
    let mut x = 1;
    x = button(fb, g, x, 27, "[<]", 20, false);
    fb.text(x, 27, &format!("{:^24}", KINDS[ui.kind]), AMBER);
    x += 25;
    button(fb, g, x, 27, "[>]", 21, false);
    let target = g
        .sim
        .target
        .filter(|&t| t < g.sim.bodies.len())
        .map_or("AUCUNE (TAB)".to_string(), |t| body_name(g, t));
    fb.text(
        45,
        27,
        &format!(
            "CIBLE {target}   REF {}",
            g.sim
                .ref_body
                .map_or("ETOILE".to_string(), |b| body_name(g, b))
        ),
        DIM,
    );
    let params = ui.params();
    for (k, (label, value)) in params.iter().enumerate() {
        let y = 29 + k;
        let on = ui.slot == k;
        let hov = g.hit_cells(1, y, 60, 1, Act::Call(Game::nav_click, 30 + k), None);
        fb.text(
            1,
            y,
            &format!("{}{:<26}{:>18}", if on { ">" } else { " " }, label, value),
            if hov {
                HOVER
            } else if on {
                AMBER
            } else {
                GREEN
            },
        );
        button(fb, g, 48, y, "[-]", 41 + 2 * k, false);
        button(fb, g, 53, y, "[+]", 40 + 2 * k, false);
    }
    if ui.kind == 10 {
        let y = 29 + params.len();
        let mut x = button(fb, g, 1, y, "[+ POINT : LA CIBLE]", 7, false);
        button(fb, g, x, y, "[EFFACER LE CIRCUIT]", 8, false);
        x = 1;
        let names: Vec<String> = ui.waypoints.iter().map(|&w| body_name(g, w)).collect();
        fb.text(x, y + 1, &names.join(" > "), DIM);
    }
    // guard and add
    let yg = 36;
    if ui.kind != 12 {
        let gl = if ui.guard == 0 {
            "TOUJOURS".to_string()
        } else {
            format!("{} {:.0}", GUARDS[ui.guard], ui.guard_x)
        };
        fb.text(1, yg, "CONDITION DE L'ORDRE :", DIM);
        let mut x = button(fb, g, 24, yg, &format!("[{gl}]"), 22, ui.guard > 0);
        x = button(fb, g, x, yg, "[-]", 24, false);
        button(fb, g, x, yg, "[+]", 23, false);
    }
    button(fb, g, 1, 38, "[AJOUTER A LA FILE]", 6, false);
    fb.text(
        22,
        38,
        "ENTREE  GAUCHE/DROITE TYPE  1-5 PARAMETRE  +/- CHANGER  G CONDITION",
        DIM,
    );
    // the right column: the ship and the path
    let rx = 92;
    fb.text(rx, 5, "VAISSEAU", AMBER);
    let s = &g.sim.ship;
    fb.text(
        rx,
        6,
        &format!(
            "CARBURANT {:.0} %",
            100.0 * s.fuel / s.stats.fuel_cap.max(1.0)
        ),
        GREEN,
    );
    fb.text(rx, 7, &format!("DELTA-V {:.0} M/S", s.delta_v()), GREEN);
    fb.text(
        rx,
        8,
        &format!("COQUE {:.0} %", 100.0 * s.hull / s.stats.hull_max.max(1.0)),
        GREEN,
    );
    fb.text(
        rx,
        9,
        &format!("POUSSEE {:.0} KN", g.sim.main_thrust() / 1000.0),
        GREEN,
    );
    fb.text(rx, 11, "TRAJECTOIRE PREVUE", AMBER);
    let on = ui.preview;
    button(
        fb,
        g,
        rx,
        12,
        if on {
            "[MASQUER LE TRACE]"
        } else {
            "[AFFICHER LE TRACE]"
        },
        9,
        on,
    );
    let shown = with_pred(g, |p| {
        let mut y = 13;
        for e in p.events.iter().filter(|e| e.t > g.sim.time).take(8) {
            let (label, col) = event_label(g, e);
            let t: String = format!("T+{:>6} {label}", fmt_eta(e.t - g.sim.time))
                .chars()
                .take(38)
                .collect();
            fb.text(rx, y, &t, col);
            y += 1;
        }
        y > 13
    });
    if shown != Some(true) {
        fb.text(rx, 13, "RIEN A SIGNALER", DIM);
    }
    let _ = fmt_time(0.0);
}

impl Game {
    pub fn nav_pick(&mut self, i: usize) {
        self.nav_ui.sel = i;
    }
}

fn draw_planner(fb: &mut Framebuffer, g: &Game) {
    let ui = &g.nav_ui;
    let sim = &g.sim;
    fb.text(1, 5, "PLANIFICATEUR DE TRAJET", AMBER);
    let target = sim
        .target
        .filter(|&t| t < sim.bodies.len())
        .map_or("AUCUNE (TAB POUR CIBLER)".to_string(), |t| body_name(g, t));
    fb.text(
        1,
        6,
        &format!(
            "DESTINATION : {target}   REFERENCE : {}",
            sim.ref_body
                .map_or("ETOILE".to_string(), |b| body_name(g, b))
        ),
        GREEN,
    );
    let mut x = 1;
    fb.text(
        x,
        8,
        &format!("ALTITUDE D'ORBITE : {:.0} KM", ui.plan_alt),
        GREEN,
    );
    x += 34;
    x = button(fb, g, x, 8, "[-]", 64, false);
    x = button(fb, g, x, 8, "[+]", 63, false);
    button(fb, g, x, 8, "[CALCULER LES OPTIONS]", 60, false);
    if ui.options.is_empty() {
        fb.text(1, 10, "AUCUNE OPTION : CIBLEZ UN CORPS (TAB) PUIS CALCULEZ. L'ORDINATEUR PROPOSE HOHMANN, AEROFREINAGE, LIGNE DROITE, ORBITES.", DIM);
    }
    for (i, o) in ui.options.iter().enumerate().take(8) {
        let y = 10 + i;
        let c = if i == ui.opt_sel {
            AMBER
        } else if o.ok {
            GREEN
        } else {
            DIM
        };
        let hov = g.hit_cells(1, y, 120, 1, Act::Call(Game::nav_opt_pick, i), None);
        let label: String = o.label.chars().take(54).collect();
        fb.text(
            1,
            y,
            &format!(
                "{}{:<54} DV {:>7.0}  CARB {:>6.0} KG  DUREE {:>8}  ATTENTE {:>8}",
                if i == ui.opt_sel { ">" } else { " " },
                label,
                o.dv,
                o.fuel,
                fmt_eta(o.duration),
                fmt_eta(o.wait)
            ),
            if hov { HOVER } else { c },
        );
    }
    if let Some(o) = ui.options.get(ui.opt_sel) {
        fb.text(1, 19, "RISQUES ET REMARQUES", AMBER);
        let mut y = 20;
        if let Some(w) = &o.why {
            fb.text(2, y, w, RED);
            y += 1;
        }
        if o.risks.is_empty() && o.why.is_none() {
            fb.text(2, y, "AUCUN RISQUE CALCULE", GREEN);
            y += 1;
        }
        for r in o.risks.iter().take(6) {
            let t: String = r.chars().take(120).collect();
            fb.text(2, y, &t, AMBER);
            y += 1;
        }
        let mut x = 1;
        x = button(fb, g, x, 27, "[ADOPTER LE PLAN]", 61, false);
        button(fb, g, x, 27, "[ADOPTER ET LANCER]", 62, false);
    }
    // FTL
    fb.text(1, 30, "SAUTS FTL ASSISTES", AMBER);
    let star = g.gal_cache.get(g.gal_sel);
    match star {
        Some(s) => {
            let d = s.distance_to(sim.here_pos);
            fb.text(
                1,
                31,
                &format!(
                    "ETOILE CHOISIE (GALAXIE L) : CLASSE {}  A {:.1} AL  PORTEE {:.1} AL",
                    s.class().letter(),
                    d,
                    sim.jump_range()
                ),
                GREEN,
            );
        }
        None => fb.text(1, 31, "AUCUNE ETOILE : OUVREZ LA GALAXIE (L)", DIM),
    }
    let mut x = button(fb, g, 1, 32, "[ETOILE SUIVANTE]", 67, false);
    x = button(fb, g, x, 32, "[CALCULER LA ROUTE]", 65, false);
    button(fb, g, x, 32, "[ADOPTER LA ROUTE]", 66, false);
    match &ui.route {
        Some(Ok(r)) => {
            fb.text(
                1,
                34,
                &format!(
                    "{} SAUT(S)  {:.1} AL  CARBURANT {:.0} KG  ANTIMATIERE {:.2} KG  DUREE {}  DANGER {:.1}",
                    r.hops.len(),
                    r.total_ly,
                    r.fuel,
                    r.antimatter,
                    fmt_eta(r.seconds),
                    r.danger
                ),
                if r.ok { GREEN } else { RED },
            );
            if let Some(w) = &r.why {
                fb.text(1, 35, w, RED);
            }
            for (i, h) in r.hops.iter().take(8).enumerate() {
                fb.text(
                    2,
                    36 + i,
                    &format!(
                        "ETAPE {} : {:.2} AL  DANGER {:.1}{}",
                        i + 1,
                        h.dist,
                        h.danger,
                        if h.danger > 0.0 {
                            "  (ETOILE OU TERRITOIRE A RISQUE)"
                        } else {
                            ""
                        }
                    ),
                    if h.danger > 0.0 { AMBER } else { DIM },
                );
            }
            fb.text(
                1,
                45,
                "A CHAQUE ARRIVEE : BALAYAGE ; HOSTILES => FUITE AU SAUT SUIVANT OU COMMANDES RENDUES",
                DIM,
            );
        }
        Some(Err(e)) => fb.text(1, 34, e, RED),
        None => {}
    }
}

impl Game {
    pub fn nav_opt_pick(&mut self, i: usize) {
        self.nav_ui.opt_sel = i;
    }
}

fn draw_limits(fb: &mut Framebuffer, g: &Game) {
    let l = g.sim.nav.limits;
    fb.text(1, 5, "SEUILS DE L'AUTOMATE", AMBER);
    fb.text(
        1,
        6,
        "SOUS CES SEUILS L'ORDINATEUR RAMENE LE VAISSEAU A LA STATION LA PLUS PROCHE (SI LE RETOUR EST ACTIF), SINON IL S'ARRETE.",
        DIM,
    );
    let rows: [(&str, String, usize, usize); 4] = [
        (
            "CARBURANT SOUS",
            format!("{:.0} %", l.fuel_return * 100.0),
            71,
            70,
        ),
        (
            "COQUE SOUS",
            format!("{:.0} %", l.hull_return * 100.0),
            73,
            72,
        ),
        (
            "DOSE AU-DESSUS",
            format!("{:.2} GY/H", l.dose_return),
            75,
            74,
        ),
        (
            "RESERVE DE CARBURANT DU PLAN",
            format!(
                "{:.0} % ~{:.1}",
                l.reserve * 100.0,
                g.sim.nav_reserve() * 100.0
            ),
            78,
            77,
        ),
    ];
    for (i, (label, val, minus, plus)) in rows.iter().enumerate() {
        let y = 8 + 2 * i;
        fb.text(2, y, &format!("{label:<34}{val:>12}"), GREEN);
        button(fb, g, 50, y, "[-]", *minus, false);
        button(fb, g, 55, y, "[+]", *plus, false);
    }
    button(
        fb,
        g,
        2,
        17,
        &format!(
            "[RETOUR D'URGENCE : {}]",
            if l.emergency { "ACTIF" } else { "INACTIF" }
        ),
        76,
        l.emergency,
    );
    button(fb, g, 2, 19, "[RENTRER A LA STATION MAINTENANT]", 79, false);
    fb.text(2, 22, "TOUJOURS ACTIFS : COLLISION PREVUE, PUITS DE GRAVITE NON PREVU, TIR, SURCHAUFFE, CHARGE STRUCTURELLE.", DIM);
}

fn draw_log(fb: &mut Framebuffer, g: &Game) {
    fb.text(1, 5, "JOURNAL DE L'AUTOMATE", AMBER);
    for (i, m) in g.sim.nav.log.iter().rev().enumerate().take(30) {
        let t: String = m.chars().take(120).collect();
        fb.text(2, 7 + i, &t, if i == 0 { GREEN } else { DIM });
    }
    if g.sim.nav.log.is_empty() {
        fb.text(2, 7, "RIEN ENCORE.", DIM);
    }
}

/// The hint line of the screen.
pub fn hint(g: &Game) -> &'static str {
    match g.nav_ui.page {
        0 => "HAUT/BAS ORDRE  ESPACE LANCER/PAUSE  SUPPR RETIRER  GAUCHE/DROITE TYPE  +/- VALEUR  ENTREE AJOUTER  TAB PAGE  ECHAP",
        1 => "ENTREE CALCULER  HAUT/BAS OPTION  ESPACE ADOPTER  R ROUTE FTL  +/- ALTITUDE  TAB PAGE  ECHAP RETOUR",
        4 => "HAUT/BAS DESTINATION  ENTREE CALCULER  ESPACE PARTIR  P REPRENDRE  X ANNULER  R ACTUALISER  TAB PAGE  ECHAP",
        _ => "CLIC SUR LES TEXTES VERTS  TAB PAGE  ECHAP RETOUR",
    }
}

// ---- the voyage page ----------------------------------------------------------------------------

impl Game {
    /// List the destinations: the star picked on the map, galactic landmarks, the galaxies around
    /// and the way home.
    pub fn voyage_refresh(&mut self) {
        use vector_sim::far::Dest;
        use vector_world::universe::GalaxyId;
        let s = &self.sim;
        let mut v: Vec<(String, Dest)> = Vec::new();
        if let Some(t) = self.gal_cache.get(self.gal_sel) {
            v.push((
                format!("ETOILE CHOISIE : {}", t.header.name.to_uppercase()),
                Dest::Star(t.id),
            ));
        }
        for (label, p) in [
            ("CENTRE GALACTIQUE", [-26_000.0, 0.0, 0.0]),
            ("BRAS OPPOSE", [-26_000.0, 0.0, 26_000.0]),
            ("MARCHES EXTERIEURES", [-26_000.0, 0.0, -34_000.0]),
            ("A 5000 AL VERS LE CENTRE", [-5000.0, 0.0, 0.0]),
        ] {
            if let Some(e) = [40.0, 90.0]
                .iter()
                .find_map(|r| s.galaxy.nearby(p, *r).into_iter().next())
            {
                if e.id != s.here {
                    v.push((
                        format!("{label} : {}", e.header.name.to_uppercase()),
                        Dest::Star(e.id),
                    ));
                }
            }
        }
        let uni = s.universe();
        let here = uni.galaxy(s.galaxy_id).expect("current galaxy");
        let ly = |a: [f64; 3], b: [f64; 3]| {
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
        };
        if s.galaxy_id != GalaxyId::HOME {
            let home = uni.galaxy(GalaxyId::HOME).expect("home galaxy");
            v.push((
                format!(
                    "RETOUR A LA GALAXIE D'ORIGINE : {} ({:.1} MAL)",
                    home.name.to_uppercase(),
                    ly(here.pos, home.pos)
                ),
                Dest::Galaxy(GalaxyId::HOME),
            ));
        }
        for gx in uni
            .galaxies_near(here.pos, 9.0)
            .into_iter()
            .filter(|g| g.id != s.galaxy_id && g.id != GalaxyId::HOME)
            .take(8)
        {
            v.push((
                format!(
                    "GALAXIE {} ({}, {:.1} MAL)",
                    gx.name.to_uppercase(),
                    gx.kind.label(),
                    ly(here.pos, gx.pos)
                ),
                Dest::Galaxy(gx.id),
            ));
        }
        self.nav_ui.vy_dests = v;
        self.nav_ui.vy_sel = self
            .nav_ui
            .vy_sel
            .min(self.nav_ui.vy_dests.len().saturating_sub(1));
        self.nav_ui.vy_plan = None;
    }

    pub fn voyage_compute(&mut self) {
        if self.nav_ui.vy_dests.is_empty() {
            self.voyage_refresh();
        }
        if let Some((_, d)) = self.nav_ui.vy_dests.get(self.nav_ui.vy_sel).cloned() {
            self.nav_ui.vy_plan = Some(self.sim.voyage_plan(d));
        }
    }

    pub fn voyage_start(&mut self) {
        if self.nav_ui.vy_plan.is_none() {
            self.voyage_compute();
        }
        match self.nav_ui.vy_plan.clone() {
            Some(Ok(plan)) => match self.sim.voyage_begin(plan) {
                Ok(m) => {
                    self.say(m);
                    self.voyage_seen = self.sim.voyage.as_ref().map_or(0, |v| v.log.len());
                    self.screen = Screen::Flight;
                }
                Err(e) => self.say(e),
            },
            Some(Err(e)) => self.say(e),
            None => self.say("AUCUNE DESTINATION"),
        }
    }

    /// "ALLER A": plan and run the whole trip to `dest` in one command.
    pub fn voyage_to(&mut self, dest: vector_sim::far::Dest) {
        match self.sim.voyage_go(dest) {
            Ok(m) => {
                self.say(m);
                self.voyage_seen = self.sim.voyage.as_ref().map_or(0, |v| v.log.len());
                self.screen = Screen::Flight;
            }
            Err(e) => self.say(format!("VOYAGE IMPOSSIBLE : {e}")),
        }
    }

    /// Say the new lines of the voyage log and drop the time warp when it stops.
    pub fn forward_voyage(&mut self) {
        let Some(v) = self.sim.voyage.as_ref() else {
            return;
        };
        let len = v.log.len();
        if len < self.voyage_seen {
            self.voyage_seen = 0;
        }
        let new: Vec<String> = v.log[self.voyage_seen..]
            .iter()
            .map(|l| l.1.clone())
            .collect();
        self.voyage_seen = len;
        let stop = !v.running();
        for m in new {
            if m.starts_with("ARRET") || m.starts_with("DESTINATION") || m.starts_with("DEPART") {
                self.say(m);
            }
        }
        if stop {
            self.timescale = 1.0;
        }
    }
}

fn draw_voyage(fb: &mut Framebuffer, g: &Game) {
    let ui = &g.nav_ui;
    fb.text(1, 5, "VOYAGE : ALLER A", AMBER);
    fb.text(
        1,
        6,
        "UNE COMMANDE : ROUTE, RAVITAILLEMENTS, CRYOSOMMEIL ET TEMPS ACCELERE SONT AUTOMATIQUES. LE VOYAGE S'ARRETE SUR DANGER.",
        DIM,
    );
    let s = &g.sim;
    let dp = s.drive_profile();
    fb.text(
        1,
        8,
        &format!(
            "PROPULSEUR : CLASSE {}  PORTEE {:.0} AL",
            dp.class.to_uppercase(),
            dp.range
        ),
        GREEN,
    );
    let gname: String = s.galaxy_name().to_uppercase().chars().take(40).collect();
    fb.text(1, 9, &format!("GALAXIE : {gname}"), DIM);
    for (i, (label, _)) in ui.vy_dests.iter().enumerate().take(16) {
        let on = i == ui.vy_sel;
        let t: String = label.chars().take(58).collect();
        let hov = g.hit_cells(2, 10 + i, 58, 1, Act::Call(Game::nav_click, 100 + i), None);
        fb.text(
            2,
            10 + i,
            &format!("{}{}", if on { ">" } else { " " }, t),
            if hov {
                HOVER
            } else if on {
                AMBER
            } else {
                GREEN
            },
        );
    }
    if ui.vy_dests.is_empty() {
        fb.text(2, 10, "(AUCUNE DESTINATION : ACTUALISER)", DIM);
    }
    let mut x = 2;
    for (label, id) in [
        ("[CALCULER]", 90),
        ("[PARTIR]", 91),
        ("[REPRENDRE]", 92),
        ("[ANNULER]", 93),
        ("[ACTUALISER]", 94),
    ] {
        x = button(fb, g, x, 28, label, id, false);
    }
    // Right side: the voyage under way, or the plan computed.
    let rx = 64;
    if let Some(v) = s
        .voyage
        .as_ref()
        .filter(|v| !v.done || ui.vy_plan.is_none())
    {
        let n = v.plan.legs.len().max(1);
        let frac = v.next as f64 / n as f64;
        let state = match (&v.paused, v.done, v.refueling, v.repairing) {
            (_, true, _, _) => "TERMINE".to_string(),
            (Some(w), _, _, _) => format!("ARRETE : {w}"),
            (_, _, true, _) => "RAVITAILLEMENT".into(),
            (_, _, _, true) => "REPARATION".into(),
            _ => "EN ROUTE".into(),
        };
        fb.text(
            rx,
            8,
            &format!("VERS {}", v.plan.dest_name.to_uppercase()),
            AMBER,
        );
        fb.text(rx, 9, &state, if v.paused.is_some() { RED } else { GREEN });
        let w = 40usize;
        let fill = ((frac * w as f64) as usize).min(w);
        fb.text(
            rx,
            10,
            &format!("[{}{}]", "#".repeat(fill), "-".repeat(w - fill)),
            GREEN,
        );
        let left = (1.0 - frac) * v.plan.total_s();
        fb.text(
            rx,
            11,
            &format!(
                "SAUT {}/{}  RESTE {}  DEPUIS {}",
                v.next,
                v.plan.legs.len(),
                vector_sim::far::span_text(left),
                vector_sim::far::span_text(s.time - v.started)
            ),
            GREEN,
        );
        fb.text(rx, 12, &format!("CARBURANT {:.0} KG", s.ship.fuel), GREEN);
        for (i, (_, m)) in v.log.iter().rev().take(14).enumerate() {
            let t: String = m.chars().take(70).collect();
            fb.text(rx, 14 + i, &t, if i == 0 { GREEN } else { DIM });
        }
    } else if let Some(p) = &ui.vy_plan {
        match p {
            Ok(p) => {
                fb.text(
                    rx,
                    8,
                    &format!("PLAN VERS {}", p.dest_name.to_uppercase()),
                    AMBER,
                );
                fb.text(
                    rx,
                    9,
                    &format!(
                        "{} SAUTS  {:.0} AL  PLUS LONG {:.0} AL",
                        p.legs.len(),
                        p.total_ly,
                        p.max_hop
                    ),
                    GREEN,
                );
                fb.text(
                    rx,
                    10,
                    &format!(
                        "DUREE {}  (HALTES {})  {} RAVITAILLEMENTS",
                        vector_sim::far::span_text(p.total_s()),
                        vector_sim::far::span_text(p.halt_s),
                        p.refuels
                    ),
                    GREEN,
                );
                fb.text(
                    rx,
                    11,
                    &format!(
                        "CARBURANT BRULE {:.0} KG  ANTIMATIERE {:.1} KG  COUT ~{:.0} CR",
                        p.fuel, p.antimatter, p.credits
                    ),
                    GREEN,
                );
                if p.total_s() >= vector_sim::far::CRYO_THRESHOLD_S {
                    fb.text(
                        rx,
                        12,
                        "TRAJET LONG : L'EQUIPAGE SERA MIS EN CRYOSOMMEIL",
                        AMBER,
                    );
                }
                if let Some(w) = &p.why {
                    fb.text(rx, 13, w, RED);
                }
            }
            Err(e) => fb.text(rx, 8, e, RED),
        }
    } else {
        fb.text(rx, 8, "CHOISISSEZ UNE DESTINATION PUIS CALCULER.", DIM);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vector_data::GameData;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    fn started() -> Game {
        let mut g = Game::new(data(), 1);
        g.key(KeyCode::Enter, None, true);
        g
    }

    fn drawn(g: &Game) -> Framebuffer {
        let mut fb = Framebuffer::new(1280, 720);
        crate::draw_frame(&mut fb, g);
        fb
    }

    #[test]
    fn the_key_under_escape_and_the_menu_open_the_screen_and_escape_comes_back() {
        let mut g = started();
        g.key(KeyCode::NumpadMultiply, None, true);
        assert_eq!(g.screen, Screen::NavAuto);
        drawn(&g);
        g.key(KeyCode::Escape, None, true);
        assert_eq!(g.screen, Screen::Flight);
        // The menu has one entry for it, in the NAVIGATION space.
        let (space, item) = crate::hub::find_entry(Screen::NavAuto).expect("an entry");
        assert_eq!(space, crate::hub::NAVIGATION);
        let doubles = crate::hub::SPACES
            .iter()
            .flat_map(|s| s.entries.iter())
            .filter(|e| e.target == Some(Screen::NavAuto))
            .count();
        assert_eq!(doubles, 1, "one entry, no duplicate");
        g.hub_activate(space, item);
        assert_eq!(g.screen, Screen::NavAuto);
    }

    #[test]
    fn an_order_is_composed_added_run_paused_moved_and_removed_with_the_mouse_numbers() {
        let mut g = started();
        g.open_nav_auto();
        // A wait of the editor: kind 11, 10 min.
        g.nav_ui.kind = 11;
        g.nav_ui.v[0] = 10.0;
        g.nav_click(6);
        g.nav_ui.v[0] = 20.0;
        g.nav_click(6);
        assert_eq!(g.sim.nav.orders.len(), 2);
        assert_eq!(g.sim.nav.orders[1].order, Order::Wait(1200.0));
        // Move the second up, then the first order is the 20 min one.
        g.nav_ui.sel = 1;
        g.nav_click(2);
        assert_eq!(g.sim.nav.orders[0].order, Order::Wait(1200.0));
        // Run, pause.
        g.nav_click(0);
        assert!(g.sim.nav_running());
        g.nav_click(0);
        assert_eq!(g.sim.nav.run, Run::Paused);
        // A guard from the editor.
        g.nav_ui.guard = 1;
        g.nav_ui.guard_x = 20.0;
        g.nav_ui.kind = 11;
        g.nav_click(6);
        assert_eq!(g.sim.nav.orders[2].guard, Some(Cond::FuelBelow(0.2)));
        // Remove, clear.
        g.nav_ui.sel = 0;
        g.nav_click(1);
        assert_eq!(g.sim.nav.orders.len(), 2);
        g.nav_click(5);
        assert!(g.sim.nav.orders.is_empty());
        drawn(&g);
    }

    #[test]
    fn an_order_that_needs_a_target_says_so_and_adds_nothing() {
        let mut g = started();
        g.open_nav_auto();
        g.sim.target = None;
        g.nav_ui.kind = 3;
        g.nav_click(6);
        assert!(g.sim.nav.orders.is_empty());
        assert!(g.messages.last().is_some_and(|m| m.contains("CIBLEZ")));
        // With a target the approach is added.
        g.cycle_target(|_| true);
        g.nav_click(6);
        assert_eq!(g.sim.nav.orders.len(), 1);
    }

    #[test]
    fn the_cockpit_shows_the_panel_only_when_there_is_a_plan_and_the_map_the_path() {
        let mut g = started();
        // The panel of the old HUD (the cockpit console has its own plan screen).
        g.settings.cockpit = crate::cockpit::Disposition::Old.index();
        // Nothing planned: no panel; with a plan, the panel shows.
        let none = drawn(&g);
        g.sim.nav_add(Order::Wait(100.0)).unwrap();
        let planned = drawn(&g);
        let x = none.cols() - 70;
        let changed = (9..14)
            .filter(|&r| {
                (x..x + 40).any(|c| {
                    (0..10).any(|y| {
                        (0..8).any(|k| {
                            let i = (r * 10 + y) * none.w + c * 8 + k;
                            none.px[i] != planned.px[i]
                        })
                    })
                })
            })
            .count();
        assert!(changed >= 3, "the panel shows with a plan: {changed} rows");
        // The map draws the predicted path (the preview on).
        g.map = true;
        g.nav_ui.preview = true;
        let with = drawn(&g);
        g.nav_ui.preview = false;
        g.sim.nav_clear();
        let without = drawn(&g);
        let diff = with
            .px
            .iter()
            .zip(&without.px)
            .filter(|(a, b)| a != b)
            .count();
        assert!(diff > 200, "the path is drawn on the map: {diff} pixels");
    }

    #[test]
    fn the_planner_page_computes_options_and_adopts_one_into_the_queue() {
        let mut g = started();
        g.open_nav_auto();
        g.nav_ui.page = 1;
        // The nearest world as the destination.
        let home = g.sim.nav_center();
        let other = (1..g.sim.bodies.len())
            .find(|&i| {
                i != home && matches!(g.sim.bodies[i].kind, vector_world::BodyKind::Planet(_))
            })
            .expect("another planet");
        g.sim.target = Some(other);
        g.nav_click(60);
        assert!(!g.nav_ui.options.is_empty(), "{:?}", g.messages.last());
        drawn(&g);
        // The straight autopilot is always among them; adopt the first one that can be run.
        let k = g
            .nav_ui
            .options
            .iter()
            .position(|o| o.ok)
            .unwrap_or_else(|| {
                panic!(
                    "one can be run: {:?}",
                    g.nav_ui
                        .options
                        .iter()
                        .map(|o| (&o.label, o.ok, &o.why))
                        .collect::<Vec<_>>()
                )
            });
        g.nav_ui.opt_sel = k;
        g.nav_click(61);
        assert!(!g.sim.nav.orders.is_empty());
        assert_eq!(g.nav_ui.page, 0);
    }

    #[test]
    fn the_thresholds_page_changes_the_limits_the_computer_obeys() {
        let mut g = started();
        g.open_nav_auto();
        let before = g.sim.nav.limits;
        g.nav_click(70);
        g.nav_click(72);
        g.nav_click(76);
        let after = g.sim.nav.limits;
        assert!(after.fuel_return > before.fuel_return);
        assert!(after.hull_return > before.hull_return);
        assert_ne!(after.emergency, before.emergency);
        g.nav_ui.page = 2;
        drawn(&g);
    }

    #[test]
    fn the_voyage_page_lists_destinations_plans_starts_and_stops_the_trip() {
        let mut g = started();
        let slot = g
            .sim
            .ship
            .design
            .modules(&g.sim.data)
            .find(|(_, m)| m.jump_range > 0.0)
            .map(|(i, _)| i)
            .expect("an FTL drive");
        g.sim.ship.design.slots[slot] = g.sim.data.module_index("ftl_mk5");
        let data = Arc::clone(&g.sim.data);
        g.sim.ship.refresh_stats(&data);
        g.sim.ship.fuel = g.sim.ship.stats.fuel_cap;
        g.sim.docked = None;
        g.sim.ship.pos = Vec3::new(4.0e11, 0.0, 0.0);
        g.open_nav_auto();
        g.nav_click(14);
        assert_eq!(g.nav_ui.page, 4);
        assert!(g.nav_ui.vy_dests.len() >= 3, "{:?}", g.nav_ui.vy_dests);
        drawn(&g);
        // The first landmark: plan, then go.
        g.nav_click(101);
        g.nav_click(90);
        assert!(
            matches!(g.nav_ui.vy_plan, Some(Ok(_))),
            "{:?}",
            g.nav_ui.vy_plan
        );
        drawn(&g);
        g.nav_click(91);
        assert!(g.sim.voyage.is_some(), "the trip starts");
        assert_eq!(g.screen, Screen::Flight);
        // The voyage asks for the time warp and the screen draws its progress.
        assert!(g.sim.voyage_warp().is_some(), "the voyage wants the warp");
        g.advance(1.0);
        g.open_nav_auto();
        g.nav_ui.page = 4;
        drawn(&g);
        g.nav_click(93);
        assert!(g.sim.voyage.as_ref().is_some_and(|v| v.done));
        g.advance(1.0);
        assert_eq!(
            g.timescale, 1.0,
            "no voyage, no forced acceleration: back to 1"
        );
    }
}
