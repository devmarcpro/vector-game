//! The dashboard: the whole state of the game at a glance, in nine panels (ship, crew, finances,
//! the world around, contracts, colonies and projects, exploration on foot, the mind of the AI,
//! and the unified alerts). It READS the systems that already exist and recomputes nothing of its
//! own: each panel names the screen that holds the detail and opens it (one system = one
//! representation). It opens with the backslash key (the key left of Enter on an ISO keyboard),
//! the Pause key, a click on the ALERTES counter of the top bar, or the menu (F10, then 0).
//! Alerts only inform: nothing here is ever compulsory.
use crate::game::{Game, Screen};
use crate::hub::{self};
use crate::hud::HOVER;
use crate::hud::{fmt_dist, AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use std::cell::RefCell;
use vector_render::Framebuffer;
use vector_sim::alerts::{Alert, Severity, Topic};
use vector_sim::bourse::DAY;
use vector_sim::design::{T_COLD, T_DAMAGE};
use winit::keyboard::KeyCode;

/// Panels, in reading order (Tab walks them in this order, digits 1 to 9 jump to one).
pub const VAISSEAU: usize = 0;
pub const EQUIPAGE: usize = 1;
pub const FINANCES: usize = 2;
pub const MONDE: usize = 3;
pub const CONTRATS: usize = 4;
pub const ALERTES: usize = 5;
pub const COLONIES: usize = 6;
pub const EXPLORATION: usize = 7;
pub const ESPRIT: usize = 8;
pub const PANELS: usize = 9;

pub const PANEL_NAMES: [&str; PANELS] = [
    "VAISSEAU",
    "EQUIPAGE",
    "FINANCES",
    "MONDE LOCAL",
    "CONTRATS ET MISSIONS",
    "ALERTES",
    "COLONIES ET PROJETS",
    "EXPLORATION A PIED",
    "ESPRIT DE L'IA",
];

/// Where the dashboard opened from, the selected panel and link, and the alerts of the last
/// second of game time (the sector scans are not free, so the list is not rebuilt every frame).
pub struct DashUi {
    pub panel: usize,
    pub link: usize,
    pub from: Screen,
    cache: RefCell<Option<(f64, Vec<Alert>)>>,
}

impl Default for DashUi {
    fn default() -> Self {
        DashUi {
            panel: VAISSEAU,
            link: 0,
            from: Screen::Flight,
            cache: RefCell::new(None),
        }
    }
}

/// A line of a panel that leads to a detailed screen.
pub struct Link {
    pub label: &'static str,
    pub screen: Screen,
    /// What opens it when it is not the hub entry of the screen.
    pub go: Option<fn(&mut Game)>,
}

const fn link(label: &'static str, screen: Screen) -> Link {
    Link {
        label,
        screen,
        go: None,
    }
}

fn open_mind(g: &mut Game) {
    g.open_tech();
    g.tech_mind = true;
}

/// The links of a panel (the alerts panel lists the alerts instead).
pub fn links(panel: usize) -> &'static [Link] {
    const SHIP: [Link; 4] = [
        link("MODULES ET ENERGIE", Screen::Ship),
        link("SOUTE", Screen::Inventory),
        link("CHANTIER NAVAL", Screen::Shipyard),
        link("GALAXIE ET SAUT FTL", Screen::Galaxy),
    ];
    const CREW: [Link; 3] = [
        link("EQUIPAGE", Screen::Crew),
        link("ARBRE FAMILIAL", Screen::Family),
        link("RECRUTEMENT", Screen::Recruit),
    ];
    const MONEY: [Link; 3] = [
        link("BANQUE", Screen::Bank),
        link("BOURSE", Screen::Bourse),
        link("COURTAGE ET ASSURANCE", Screen::Trading),
    ];
    const WORLD: [Link; 3] = [
        link("JOURNAL ET NOUVELLES", Screen::Journal),
        link("PEUPLES, RELATIONS ET LOIS", Screen::Factions),
        link("PORTAIL ET TEMPS", Screen::Cosmos),
    ];
    const WORK: [Link; 2] = [
        link("CONTRATS", Screen::Contracts),
        link("TECHNOLOGIES", Screen::Tech),
    ];
    const COLONY: [Link; 3] = [
        link("EMPIRE", Screen::Empire),
        link("FLOTTE", Screen::Fleet),
        link("CHANTIER NAVAL", Screen::Shipyard),
    ];
    const FOOT: [Link; 2] = [
        link("EXPLORER A PIED", Screen::Rogue),
        link("EXPEDITION SUR UN MONDE", Screen::Expedition),
    ];
    const MIND: [Link; 2] = [
        Link {
            label: "ESPRIT, LOGICIELS, CALCUL",
            screen: Screen::Tech,
            go: Some(open_mind),
        },
        link("CODEX", Screen::Codex),
    ];
    match panel {
        VAISSEAU => &SHIP,
        EQUIPAGE => &CREW,
        FINANCES => &MONEY,
        MONDE => &WORLD,
        CONTRATS => &WORK,
        COLONIES => &COLONY,
        EXPLORATION => &FOOT,
        ESPRIT => &MIND,
        _ => &[],
    }
}

/// The screen that deals with an alert, and whether it is the page of the mind.
pub fn topic_screen(t: Topic) -> (Screen, bool) {
    use Topic::*;
    match t {
        Hull | Modules | Heat | Power | Radiation | Structure | Antimatter => (Screen::Ship, false),
        Fuel => (Screen::Inventory, false),
        Collision | Weather => (Screen::Flight, false),
        Health | Disease | Wounds | Needs | Pay | Cryo => (Screen::Crew, false),
        Debt => (Screen::Bank, false),
        Insurance => (Screen::Trading, false),
        Law | War => (Screen::Factions, false),
        Tribute => (Screen::Flight, false),
        Surprise => (Screen::Journal, false),
        Contract => (Screen::Contracts, false),
        Colony | Epidemic => (Screen::Empire, false),
        Malware | Mind => (Screen::Tech, true),
        Yard => (Screen::Shipyard, false),
    }
}

// ---- state and input --------------------------------------------------------------------------

impl Game {
    /// The alerts of the game now (rebuilt at most once per second of game time).
    pub fn alerts(&self) -> Vec<Alert> {
        let t = self.sim.time;
        let mut c = self.dash.cache.borrow_mut();
        match &*c {
            Some((at, list)) if (t - at).abs() < 1.0 => list.clone(),
            _ => {
                let list = self.sim.alerts();
                *c = Some((t, list.clone()));
                list
            }
        }
    }

    pub fn open_dashboard(&mut self) {
        if self.screen == Screen::Dashboard {
            self.close_dashboard();
            return;
        }
        if matches!(
            self.screen,
            Screen::MainMenu | Screen::NewGame | Screen::Destroyed
        ) {
            return;
        }
        self.dash.from = self.screen;
        self.dash.link = 0;
        self.dash_origin = false;
        self.hub_origin = None;
        *self.dash.cache.borrow_mut() = None;
        self.screen = Screen::Dashboard;
    }

    pub fn close_dashboard(&mut self) {
        let from = self.dash.from;
        self.dash_origin = false;
        self.screen = match from {
            Screen::Station if self.sim.docked.is_none() => Screen::Flight,
            Screen::Flight if self.sim.docked.is_some() => Screen::Station,
            Screen::Rogue if self.sim.rogue.is_none() => Screen::Flight,
            Screen::Dashboard => Screen::Flight,
            s => s,
        };
    }

    pub fn dash_select(&mut self, panel: usize) {
        self.dash.panel = panel % PANELS;
        self.dash.link = 0;
    }

    /// Number of lines that can be chosen in the selected panel.
    fn dash_lines(&self, panel: usize) -> usize {
        if panel == ALERTES {
            self.alerts().len().min(99)
        } else {
            links(panel).len()
        }
    }

    /// Open the screen `screen` from the dashboard (Escape there comes back here).
    pub fn dash_open(&mut self, screen: Screen, go: Option<fn(&mut Game)>) {
        let Some((sp, it)) = hub::find_entry(screen) else {
            return;
        };
        let e = &hub::SPACES[sp].entries[it];
        if let Some(why) = hub::unavailable(self, e) {
            self.say(why);
            return;
        }
        if self.sim.rogue.is_some() && e.target != Some(Screen::Rogue) {
            self.rogue_visit = true;
        }
        self.hub_origin = None;
        self.dash_origin = true;
        (go.unwrap_or(e.go))(self);
        if matches!(
            self.screen,
            Screen::Dashboard | Screen::Flight | Screen::Station
        ) {
            // An action, or a refusal: nothing to come back from.
            self.dash_origin = false;
        }
    }

    /// Activate line `idx` of `panel` (a link, or an alert of the alerts panel).
    pub fn dash_activate(&mut self, panel: usize, idx: usize) {
        self.dash.panel = panel % PANELS;
        self.dash.link = idx;
        if panel == ALERTES {
            let list = self.alerts();
            if let Some(a) = list.get(idx) {
                let (screen, mind) = topic_screen(a.topic);
                if screen == Screen::Flight {
                    self.say(a.text.clone());
                } else {
                    self.dash_open(screen, mind.then_some(open_mind as fn(&mut Game)));
                }
            }
            return;
        }
        if let Some(l) = links(panel).get(idx) {
            self.dash_open(l.screen, l.go);
        }
    }

    pub fn click_dash_panel(&mut self, i: usize) {
        self.dash_select(i);
    }

    pub fn click_dash_link(&mut self, code: usize) {
        self.dash_activate((code / 100).saturating_sub(1), code % 100);
    }

    pub fn dash_key(&mut self, code: KeyCode) {
        let digit = match code {
            KeyCode::Digit1 | KeyCode::Numpad1 => Some(0),
            KeyCode::Digit2 | KeyCode::Numpad2 => Some(1),
            KeyCode::Digit3 | KeyCode::Numpad3 => Some(2),
            KeyCode::Digit4 | KeyCode::Numpad4 => Some(3),
            KeyCode::Digit5 | KeyCode::Numpad5 => Some(4),
            KeyCode::Digit6 | KeyCode::Numpad6 => Some(5),
            KeyCode::Digit7 | KeyCode::Numpad7 => Some(6),
            KeyCode::Digit8 | KeyCode::Numpad8 => Some(7),
            KeyCode::Digit9 | KeyCode::Numpad9 => Some(8),
            _ => None,
        };
        if let Some(d) = digit {
            self.dash_select(d);
            return;
        }
        let n = self.dash_lines(self.dash.panel).max(1);
        let back =
            self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight);
        match code {
            KeyCode::Tab if back => self.dash_select((self.dash.panel + PANELS - 1) % PANELS),
            KeyCode::Tab | KeyCode::ArrowRight => self.dash_select((self.dash.panel + 1) % PANELS),
            KeyCode::ArrowLeft => self.dash_select((self.dash.panel + PANELS - 1) % PANELS),
            KeyCode::ArrowUp => self.dash.link = (self.dash.link + n - 1) % n,
            KeyCode::ArrowDown => self.dash.link = (self.dash.link + 1) % n,
            KeyCode::Enter | KeyCode::Space => self.dash_activate(self.dash.panel, self.dash.link),
            KeyCode::Escape => self.close_dashboard(),
            _ => {}
        }
    }
}

// ---- drawing helpers --------------------------------------------------------------------------

/// Green when the share is healthy, amber when it is low, red when it is critical.
fn tint(good: f64) -> u32 {
    if good >= 0.5 {
        GREEN
    } else if good >= 0.25 {
        AMBER
    } else {
        RED
    }
}

/// Text cut to `w` columns.
fn put(fb: &mut Framebuffer, x: usize, y: usize, w: usize, s: &str, c: u32) {
    let t: String = s.chars().take(w).collect();
    fb.text(x, y, &t, c);
}

use crate::ui::bar;

/// A labelled bar and its figure. `good` is the share that decides the colour (1 = fine).
#[allow(clippy::too_many_arguments)]
fn meter(
    fb: &mut Framebuffer,
    x: usize,
    y: usize,
    w: usize,
    label: &str,
    frac: f64,
    good: f64,
    note: &str,
) {
    fb.text(x, y, label, DIM);
    let bw = 12;
    fb.text(x + 10, y, &bar(frac, bw), tint(good));
    put(
        fb,
        x + 10 + bw + 3,
        y,
        w.saturating_sub(10 + bw + 3),
        note,
        GREEN,
    );
}

/// A sparkline in ASCII levels.
pub fn spark(values: &[f64]) -> String {
    const RAMP: [char; 8] = [' ', '.', ':', '-', '=', '+', '*', '#'];
    let (lo, hi) = values
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    values
        .iter()
        .map(|v| {
            let k = if hi > lo { (v - lo) / (hi - lo) } else { 0.5 };
            RAMP[((k * 7.0).round() as usize).min(7)]
        })
        .collect()
}

fn credits(v: f64) -> String {
    crate::fmt::credits(v)
}

/// The frame of a panel: a box in pixels and its title over the top edge.
fn frame(fb: &mut Framebuffer, g: &Game, p: usize, x: usize, y: usize, w: usize, h: usize) {
    let sel = g.dash.panel == p;
    let hover = g.hit_cells(x, y, w, h, Act::Call(Game::click_dash_panel, p), None);
    let c = if sel {
        AMBER
    } else if hover {
        HOVER
    } else {
        DIM
    };
    let (x0, y0) = ((x * 8) as f32 + 2.0, (y * 10) as f32 + 5.0);
    let (x1, y1) = (((x + w) * 8) as f32 - 3.0, ((y + h) * 10) as f32 - 3.0);
    fb.line(x0, y0, x1, y0, c);
    fb.line(x0, y1, x1, y1, c);
    fb.line(x0, y0, x0, y1, c);
    fb.line(x1, y0, x1, y1, c);
    let title = format!(" {} {} ", p + 1, PANEL_NAMES[p]);
    let tw = title.chars().count();
    for py in (y * 10) as i32..(y * 10 + 10) as i32 {
        for px in ((x + 1) * 8) as i32..((x + 1 + tw) * 8) as i32 {
            fb.px_set(px, py, 0);
        }
    }
    fb.text(x + 1, y, &title, if sel { AMBER } else { GREEN });
}

/// The link lines at the foot of a panel; returns the row after them.
fn draw_links(fb: &mut Framebuffer, g: &Game, p: usize, x: usize, y: usize, w: usize) -> usize {
    let mut row = y;
    for (j, l) in links(p).iter().enumerate() {
        let entry = hub::find_entry(l.screen).map(|(s, i)| &hub::SPACES[s].entries[i]);
        let why = entry.and_then(|e| hub::unavailable(g, e));
        let key = entry.map_or("", |e| e.key);
        let sel = g.dash.panel == p && g.dash.link == j;
        let hover = g.hit_cells(
            x,
            row,
            w,
            1,
            Act::Call(Game::click_dash_link, (p + 1) * 100 + j),
            None,
        );
        let c = if why.is_some() {
            DIM
        } else if sel {
            AMBER
        } else if hover {
            HOVER
        } else {
            GREEN
        };
        put(
            fb,
            x,
            row,
            w,
            &format!("{} {}", if sel { ">" } else { " " }, l.label),
            c,
        );
        let kw = key.chars().count();
        if w > kw + l.label.chars().count() + 4 {
            fb.text(x + w - kw, row, key, DIM);
        }
        row += 1;
    }
    row
}

fn state_of(g: &Game) -> String {
    hub::orientation(g).place
}

// ---- the panels -------------------------------------------------------------------------------

fn draw_ship(fb: &mut Framebuffer, g: &Game, (x, y, w, h): Rect) {
    frame(fb, g, VAISSEAU, x, y, w, h);
    let (s, wi) = (&g.sim, w - 2);
    let ship = &s.ship;
    let (x, mut r) = (x + 1, y + 1);
    let hull = &g.data.hulls[ship.design.hull];
    put(
        fb,
        x,
        r,
        wi,
        &format!(
            "{} ({})",
            hull.name.to_uppercase(),
            hull.class.to_uppercase()
        ),
        AMBER,
    );
    r += 1;
    let state = if s.rogue.is_some() {
        "A PIED"
    } else if s.docked.is_some() {
        "AMARRE"
    } else if s.parked.is_some() {
        "POSE AU SOL"
    } else if s.in_transit() {
        "TRANSIT FTL"
    } else {
        "EN VOL"
    };
    let auto = match (&s.nav.run, s.nav.pending()) {
        (vector_sim::nav_auto::Run::Running, _) => "  AUTOMATE EN COURS",
        (vector_sim::nav_auto::Run::Halted(_), _) => "  AUTOMATE ARRETE",
        (vector_sim::nav_auto::Run::Paused, _) => "  AUTOMATE EN PAUSE",
        (_, n) if n > 0 => "  AUTOMATE PRET",
        _ => "",
    };
    put(fb, x, r, wi, &format!("ETAT : {state}{auto}"), GREEN);
    r += 1;
    let hf = ship.hull / ship.stats.hull_max.max(1.0);
    meter(
        fb,
        x,
        r,
        wi,
        "COQUE",
        hf,
        hf,
        &format!("{:.0} %", hf * 100.0),
    );
    r += 1;
    // Integrity of every module.
    let mut shown = 0;
    let total = ship.design.slots.iter().flatten().count();
    for (i, m) in ship.design.slots.iter().enumerate() {
        let Some(m) = m else { continue };
        if shown >= 9 {
            break;
        }
        let hp = ship.sys.health[i];
        let name: String = g.data.modules[*m]
            .name
            .to_uppercase()
            .chars()
            .take(14)
            .collect();
        fb.text(x, r, &format!("{:<14}", name), DIM);
        fb.text(x + 15, r, &bar(hp, 10), tint(hp / 0.8));
        fb.text(
            x + 28,
            r,
            &format!("{:>3.0}%", hp * 100.0),
            if hp < 0.3 { RED } else { GREEN },
        );
        let on = ship.sys.powered[i];
        fb.text(
            x + 34,
            r,
            if on { "ON" } else { "OFF" },
            if on { GREEN } else { RED },
        );
        r += 1;
        shown += 1;
    }
    if total > shown {
        fb.text(x, r, &format!("... +{} MODULE(S)", total - shown), DIM);
        r += 1;
    }
    r += 1;
    let ff = ship.fuel / ship.stats.fuel_cap.max(1.0);
    meter(
        fb,
        x,
        r,
        wi,
        "CARBURANT",
        ff,
        ff / 0.5,
        &format!("{:.0} KG", ship.fuel),
    );
    r += 1;
    let dv = ship.delta_v();
    let dvs = if dv.is_finite() {
        format!("DELTA-V {:.2} KM/S (TSIOLKOVSKI)", dv / 1e3)
    } else {
        "DELTA-V ?".to_string()
    };
    put(fb, x, r, wi, &dvs, if dv < 500.0 { AMBER } else { GREEN });
    r += 1;
    let sys = &ship.sys;
    let bf = sys.battery_mj / ship.stats.battery_cap.max(1.0);
    meter(
        fb,
        x,
        r,
        wi,
        "BATTERIE",
        bf,
        bf / 0.5,
        &format!("{:.0} MJ", sys.battery_mj),
    );
    r += 1;
    let deficit = sys.demand_kw > sys.supply_kw + 1e-9;
    put(
        fb,
        x,
        r,
        wi,
        &format!(
            "ENERGIE {:.0} KW PRODUITS, {:.0} KW DEMANDES",
            sys.supply_kw, sys.demand_kw
        ),
        if deficit { RED } else { GREEN },
    );
    r += 1;
    let hot = ((sys.temp_k - T_COLD) / (T_DAMAGE - T_COLD)).clamp(0.0, 1.0);
    meter(
        fb,
        x,
        r,
        wi,
        "CHALEUR",
        hot,
        1.0 - hot,
        &format!(
            "{:.0} K{}",
            sys.temp_k,
            if sys.overheating { " SURCHAUFFE" } else { "" }
        ),
    );
    r += 1;
    let cap = s.effective.shield_cap;
    if cap > 0.0 {
        let sf = sys.shield_pts / cap;
        meter(
            fb,
            x,
            r,
            wi,
            "BOUCLIER",
            sf,
            sf / 0.5,
            &format!("{:.0} / {:.0}", sys.shield_pts, cap),
        );
    } else {
        fb.text(x, r, "BOUCLIER", DIM);
        fb.text(x + 10, r, "AUCUN", DIM);
    }
    r += 1;
    let (cm, cv) = (
        ship.cargo.mass / ship.cargo.cap_mass.max(1.0),
        ship.cargo.volume / ship.cargo.cap_vol.max(1.0),
    );
    meter(
        fb,
        x,
        r,
        wi,
        "SOUTE MAS",
        cm,
        1.0 - cm * 0.6,
        &format!(
            "{:.1}/{:.1} T",
            ship.cargo.mass / 1e3,
            ship.cargo.cap_mass / 1e3
        ),
    );
    r += 1;
    meter(
        fb,
        x,
        r,
        wi,
        "SOUTE VOL",
        cv,
        1.0 - cv * 0.6,
        &format!("{:.1}/{:.1} M3", ship.cargo.volume, ship.cargo.cap_vol),
    );
    r += 2;
    // Position.
    put(
        fb,
        x,
        r,
        wi,
        &format!("SYSTEME {}", s.system.name.to_uppercase()),
        DIM,
    );
    r += 1;
    if let Some(b) = s.ref_body {
        let alt = ((ship.pos - s.bodies[b].pos).length() - s.bodies[b].radius).max(0.0);
        let v = (ship.vel - s.bodies[b].vel).length();
        put(
            fb,
            x,
            r,
            wi,
            &format!(
                "{} ALT {} VIT {:.0} M/S",
                s.bodies[b].name.to_uppercase(),
                fmt_dist(alt),
                v
            ),
            GREEN,
        );
        r += 1;
    }
    if let Some(o) = s.orbit_readout() {
        let e = &o.elements;
        let txt = if e.bound {
            format!(
                "ORBITE PE {} AP {} E {:.2}",
                if e.periapsis < o.radius {
                    "SOUS LA SURFACE".to_string()
                } else {
                    fmt_dist(e.periapsis - o.radius)
                },
                e.apoapsis
                    .map_or("?".to_string(), |a| fmt_dist(a - o.radius)),
                e.e
            )
        } else {
            "TRAJECTOIRE OUVERTE (HYPERBOLE)".to_string()
        };
        put(fb, x, r, wi, &txt, GREEN);
        r += 1;
    }
    let rate = s.dose_rate_aboard();
    put(
        fb,
        x,
        r,
        wi,
        &format!(
            "DOSE {:.1} MGY/H (CUMULEE {:.2} GY)  CHARGE {:.1} G",
            rate * 1e3,
            sys.dose_gy,
            s.structural_load_g()
        ),
        if rate >= vector_sim::alerts::DOSE_RATE_WARNING {
            RED
        } else {
            DIM
        },
    );
    r += 1;
    for (line, alert) in s.exotic_hud().into_iter().take(2) {
        put(fb, x, r, wi, &line, if alert { RED } else { AMBER });
        r += 1;
    }
    let _ = draw_links(
        fb,
        g,
        VAISSEAU,
        x,
        (y + h).saturating_sub(links(VAISSEAU).len() + 1).max(r),
        wi,
    );
}

fn draw_crew(fb: &mut Framebuffer, g: &Game, (x, y, w, h): Rect) {
    frame(fb, g, EQUIPAGE, x, y, w, h);
    let (s, wi) = (&g.sim, w - 2);
    let (x, mut r) = (x + 1, y + 1);
    let alive: Vec<&vector_sim::crew::Person> = s.crew.alive().collect();
    let n = alive.len().max(1) as f32;
    let morale = alive.iter().map(|p| p.morale).sum::<f32>() / n;
    let fatigue = alive.iter().map(|p| p.fatigue).sum::<f32>() / n;
    put(
        fb,
        x,
        r,
        wi,
        &format!(
            "{} PERSONNE(S)  HUMEUR {:.0} %  FATIGUE {:.0} %",
            alive.len(),
            morale * 100.0,
            fatigue * 100.0
        ),
        GREEN,
    );
    r += 1;
    let pay = s.crew.daily_payroll();
    put(
        fb,
        x,
        r,
        wi,
        &format!(
            "SALAIRES {} PAR JOUR{}",
            credits(pay),
            if s.crew.unpaid { "  IMPAYES !" } else { "" }
        ),
        if s.crew.unpaid { RED } else { DIM },
    );
    r += 2;
    fb.text(x, r, "NOM         SANTE        HUM FAT ETAT", DIM);
    r += 1;
    for p in alive.iter().take(9) {
        let c = if p.health < 0.3 {
            RED
        } else if p.health < 0.6 {
            AMBER
        } else {
            GREEN
        };
        let state = if let Some((d, _)) = p.disease {
            g.data.diseases[d].name.to_uppercase()
        } else if p
            .body
            .wounds
            .iter()
            .any(|w| w.sev >= vector_sim::alerts::WOUND_GRAVE)
        {
            "BLESSE GRAVE".to_string()
        } else if !p.body.wounds.is_empty() {
            "BLESSE".to_string()
        } else {
            String::new()
        };
        let name: String = p.name.to_uppercase().chars().take(11).collect();
        fb.text(x, r, &format!("{name:<11}"), c);
        fb.text(x + 12, r, &bar(f64::from(p.health), 6), c);
        fb.text(x + 21, r, &format!("{:>3.0}", p.health * 100.0), c);
        fb.text(
            x + 25,
            r,
            &format!("{:>3.0}", p.morale * 100.0),
            tint(f64::from(p.morale) / 0.6),
        );
        fb.text(
            x + 29,
            r,
            &format!("{:>3.0}", p.fatigue * 100.0),
            tint(1.0 - f64::from(p.fatigue)),
        );
        put(
            fb,
            x + 34,
            r,
            wi.saturating_sub(34),
            &state,
            if state.is_empty() { DIM } else { AMBER },
        );
        r += 1;
    }
    if alive.len() > 9 {
        fb.text(x, r, &format!("... +{} AUTRE(S)", alive.len() - 9), DIM);
        r += 1;
    }
    r += 1;
    // Best skills of the crew.
    fb.text(x, r, "COMPETENCES CLES", DIM);
    r += 1;
    let mut best: Vec<(usize, f32, &str)> = (0..g.data.skills.len())
        .filter_map(|k| {
            alive
                .iter()
                .map(|p| (k, p.skills.get(k).copied().unwrap_or(0.0), p.name.as_str()))
                .max_by(|a, b| a.1.total_cmp(&b.1))
        })
        .collect();
    best.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (k, lv, who) in best.into_iter().take(4) {
        put(
            fb,
            x,
            r,
            wi,
            &format!(
                "{:<16.16} {:>4.1}  {}",
                g.data.skills[k].name.to_uppercase(),
                lv,
                who.to_uppercase()
            ),
            GREEN,
        );
        r += 1;
    }
    r += 1;
    // Languages.
    match s.local_language() {
        Some(l) => put(
            fb,
            x,
            r,
            wi,
            &format!(
                "LANGUE LOCALE {}  EQUIPAGE {:.0} %  IA {:.0} %",
                g.data.society.languages[l].name.to_uppercase(),
                s.crew_fluency(l) * 100.0,
                s.ai_fluency() * 100.0
            ),
            GREEN,
        ),
        None => put(fb, x, r, wi, "LANGUE LOCALE : AUCUN PEUPLE ICI", DIM),
    }
    r += 1;
    let sick: Vec<String> = {
        let mut v: Vec<String> = alive
            .iter()
            .filter_map(|p| {
                p.disease
                    .map(|(d, _)| g.data.diseases[d].name.to_uppercase())
            })
            .collect();
        v.sort();
        v.dedup();
        v
    };
    put(
        fb,
        x,
        r,
        wi,
        &if sick.is_empty() {
            "AUCUNE MALADIE A BORD".to_string()
        } else {
            format!("MALADIES A BORD : {}", sick.join(", "))
        },
        if sick.is_empty() { DIM } else { AMBER },
    );
    r += 1;
    let _ = draw_links(
        fb,
        g,
        EQUIPAGE,
        x,
        (y + h).saturating_sub(links(EQUIPAGE).len() + 1).max(r),
        wi,
    );
}

fn draw_finance(fb: &mut Framebuffer, g: &Game, (x, y, w, h): Rect) {
    frame(fb, g, FINANCES, x, y, w, h);
    let (s, wi) = (&g.sim, w - 2);
    let (x, mut r) = (x + 1, y + 1);
    put(
        fb,
        x,
        r,
        wi,
        &format!("CREDITS {}", credits(s.credits)),
        AMBER,
    );
    r += 1;
    let limit = s.credit_limit();
    let loan = s.bank.loan;
    if loan > 0.0 || s.bank.mortgage {
        let share = if limit > 0.0 { loan / limit } else { 1.0 };
        meter(
            fb,
            x,
            r,
            wi,
            "DETTE",
            share,
            1.0 - share,
            &format!("{} / {}", credits(loan), credits(limit)),
        );
        r += 1;
        put(
            fb,
            x,
            r,
            wi,
            &format!(
                "HYPOTHEQUE {}  INTERETS PAYES {}",
                if s.bank.mortgage { "OUI" } else { "NON" },
                credits(s.bank.total_interest)
            ),
            if s.bank.overdue_days > 0 { RED } else { DIM },
        );
    } else {
        put(
            fb,
            x,
            r,
            wi,
            &format!("AUCUNE DETTE (PLAFOND DE CREDIT {})", credits(limit)),
            DIM,
        );
        r += 1;
        put(fb, x, r, wi, "HYPOTHEQUE NON", DIM);
    }
    r += 1;
    let nw = s.net_worth();
    put(
        fb,
        x,
        r,
        wi,
        &format!("VALEUR NETTE {}", credits(nw)),
        GREEN,
    );
    r += 1;
    let left = (s.trade.insured_until - s.time) / DAY;
    put(
        fb,
        x,
        r,
        wi,
        &if s.insured() {
            format!("ASSURANCE ACTIVE : {left:.1} J RESTANTS")
        } else {
            "ASSURANCE : AUCUNE".to_string()
        },
        if s.insured() { GREEN } else { DIM },
    );
    r += 2;
    let folio = s.portfolio_value();
    put(
        fb,
        x,
        r,
        wi,
        &format!(
            "TITRES {}  ({} ACTIONS, {} FUTURS)",
            credits(folio),
            s.shares.len(),
            s.futures.len()
        ),
        GREEN,
    );
    r += 1;
    put(
        fb,
        x,
        r,
        wi,
        &format!(
            "{} OBLIGATIONS, {} OPTIONS, {} DEVISES",
            s.trade.folio.bonds.len(),
            s.trade.folio.options.len(),
            s.bank.forex.len()
        ),
        DIM,
    );
    r += 2;
    let sec = s.local_sector();
    let who = sec.faction.map_or("TERRITOIRE LIBRE".to_string(), |f| {
        g.data.factions[f].name.to_uppercase()
    });
    put(fb, x, r, wi, &format!("SECTEUR : {who}"), DIM);
    r += 1;
    put(
        fb,
        x,
        r,
        wi,
        &format!(
            "TAUX {:.1} %/AN  INFLATION {:.1} %/AN",
            sec.policy_rate * 100.0,
            sec.inflation * 100.0
        ),
        if sec.inflation > 0.15 { AMBER } else { GREEN },
    );
    r += 1;
    put(
        fb,
        x,
        r,
        wi,
        &format!(
            "CHOMAGE {:.0} %  {}",
            sec.unemployment * 100.0,
            if sec.crash > 0.0 {
                format!("KRACH EN COURS {:.0} %", sec.crash * 100.0)
            } else if sec.bubble > 0.1 {
                format!("BULLE {:.0} %", sec.bubble * 100.0)
            } else {
                "MARCHES CALMES".to_string()
            }
        ),
        if sec.crash > 0.0 { RED } else { DIM },
    );
    r += 2;
    // Last flows: the monthly accounts.
    let reps = &s.bank.reports;
    if reps.len() >= 2 {
        let vals: Vec<f64> = reps
            .iter()
            .rev()
            .take(24)
            .rev()
            .map(|m| m.net_worth)
            .collect();
        fb.text(x, r, "VALEUR NETTE", DIM);
        fb.text(x + 13, r, &format!("[{}]", spark(&vals)), GREEN);
        r += 1;
        for pair in reps.windows(2).rev().take(2) {
            let d = pair[1].net_worth - pair[0].net_worth;
            put(
                fb,
                x,
                r,
                wi,
                &format!(
                    "MOIS {}  {}{}",
                    pair[1].month,
                    if d >= 0.0 { "+" } else { "" },
                    credits(d)
                ),
                if d >= 0.0 { GREEN } else { RED },
            );
            r += 1;
        }
    } else {
        put(fb, x, r, wi, "HISTORIQUE : PAS ENCORE DE MOIS CLOS", DIM);
        r += 1;
    }
    let _ = draw_links(
        fb,
        g,
        FINANCES,
        x,
        (y + h).saturating_sub(links(FINANCES).len() + 1).max(r + 1),
        wi,
    );
}

fn draw_world(fb: &mut Framebuffer, g: &Game, alerts: &[Alert], (x, y, w, h): Rect) {
    frame(fb, g, MONDE, x, y, w, h);
    let (s, wi) = (&g.sim, w - 2);
    let (x, mut r) = (x + 1, y + 1);
    fb.text(x, r, "FIL DU SECTEUR (AVEC LE RETARD DE LA DISTANCE)", DIM);
    r += 1;
    let news = s.headlines(5);
    if news.is_empty() {
        fb.text(x, r, "AUCUNE NOUVELLE RECUE", DIM);
        r += 1;
    }
    for n in news {
        let age = ((s.time - n.time) / DAY).max(0.0);
        let when = if age < 1.0 {
            "AUJ.".to_string()
        } else {
            format!("J-{age:.0}")
        };
        put(
            fb,
            x,
            r,
            wi,
            &format!(
                "{:<5}{:<7}{}",
                when,
                format!("{:.0}AL", n.dist_ly),
                n.text.to_uppercase()
            ),
            match n.scale() {
                "ICI" => AMBER,
                "SECTEUR" => GREEN,
                _ => DIM,
            },
        );
        r += 1;
    }
    r += 1;
    // Law and the legal situation of the player here.
    match s.faction {
        Some(f) => {
            let laws = s.laws_in_force(f);
            put(
                fb,
                x,
                r,
                wi,
                &format!(
                    "LOIS DE {} : {} EN VIGUEUR",
                    g.data.factions[f].name.to_uppercase(),
                    laws.len()
                ),
                GREEN,
            );
            r += 1;
            let sev: Vec<String> = laws
                .iter()
                .filter(|l| g.data.laws[**l].severity >= 2)
                .take(3)
                .map(|l| g.data.laws[*l].name.to_uppercase())
                .collect();
            if !sev.is_empty() {
                put(fb, x, r, wi, &format!("GRAVES : {}", sev.join(", ")), DIM);
                r += 1;
            }
            let (n, owed) = (s.notoriety_in(f), s.fines_owed(f));
            let warrant = s.has_warrant(f);
            put(
                fb,
                x,
                r,
                wi,
                &format!(
                    "NOTORIETE {:.1}  AMENDES {}  {}",
                    n,
                    credits(owed),
                    if warrant {
                        "MANDAT : QUAIS FERMES"
                    } else if n > 0.0 || owed > 0.0 {
                        "RECHERCHE"
                    } else {
                        "EN REGLE"
                    }
                ),
                if warrant {
                    RED
                } else if n > 0.0 || owed > 0.0 {
                    AMBER
                } else {
                    GREEN
                },
            );
            r += 1;
        }
        None => {
            put(fb, x, r, wi, "TERRITOIRE LIBRE : AUCUNE LOI LOCALE", DIM);
            r += 1;
        }
    }
    r += 1;
    // Feelings of the peoples around: the one that holds the sector, then the strongest.
    fb.text(x, r, "RELATIONS (PEUPLES PROCHES)", DIM);
    r += 1;
    let mut rows = s.relation_rows();
    let here = s.faction;
    rows.retain(|rw| rw.value.abs() > 0.004 || Some(rw.who.id as usize) == here);
    rows.sort_by(|a, b| b.value.abs().total_cmp(&a.value.abs()));
    if let Some(f) = here {
        if let Some(i) = rows
            .iter()
            .position(|rw| rw.who == vector_sim::relations::Entity::faction(f))
        {
            let row = rows.remove(i);
            rows.insert(0, row);
        }
    }
    if rows.is_empty() {
        fb.text(x, r, "AUCUN SENTIMENT PARTICULIER", DIM);
        r += 1;
    }
    for rw in rows.iter().take(5) {
        // A bar centred on zero: left is hostile, right is friendly.
        let v = f64::from(rw.value).clamp(-1.0, 1.0);
        let half = 6usize;
        let k = (v.abs() * half as f64).round() as usize;
        let (l, rr) = if v < 0.0 {
            ("-".repeat(half - k) + &"#".repeat(k), "-".repeat(half))
        } else {
            ("-".repeat(half), "#".repeat(k) + &"-".repeat(half - k))
        };
        fb.text(x, r, &format!("{:<20.20}", rw.name.to_uppercase()), GREEN);
        fb.text(
            x + 21,
            r,
            &format!("[{l}|{rr}]"),
            if v < -0.1 {
                RED
            } else if v > 0.1 {
                GREEN
            } else {
                DIM
            },
        );
        fb.text(x + 36, r, &format!("{:+.2}", v), DIM);
        r += 1;
    }
    r += 1;
    // What waits for an answer, and what is happening around.
    let waiting = s.surprises.pending.len();
    put(
        fb,
        x,
        r,
        wi,
        &format!("SURPRISES EN ATTENTE : {waiting}"),
        if waiting > 0 { AMBER } else { DIM },
    );
    r += 1;
    if let Some((text, _)) = s.pending_surprise(0) {
        for l in hub::wrap(&text.to_uppercase(), wi).into_iter().take(2) {
            fb.text(x, r, &l, GREEN);
            r += 1;
        }
    }
    for a in alerts
        .iter()
        .filter(|a| matches!(a.topic, Topic::War | Topic::Epidemic))
        .take(2)
    {
        put(
            fb,
            x,
            r,
            wi,
            &a.text,
            if a.sev == Severity::Info { DIM } else { AMBER },
        );
        r += 1;
    }
    let _ = draw_links(
        fb,
        g,
        MONDE,
        x,
        (y + h).saturating_sub(links(MONDE).len() + 1).max(r + 1),
        wi,
    );
}

fn draw_contracts(fb: &mut Framebuffer, g: &Game, (x, y, w, h): Rect) {
    frame(fb, g, CONTRATS, x, y, w, h);
    let (s, wi) = (&g.sim, w - 2);
    let (x, mut r) = (x + 1, y + 1);
    if s.contracts.is_empty() {
        put(fb, x, r, wi, "AUCUN CONTRAT EN COURS", DIM);
        r += 1;
    }
    let mut list: Vec<&vector_sim::economy::Contract> = s.contracts.iter().collect();
    list.sort_by(|a, b| a.expires.total_cmp(&b.expires));
    for c in list.iter().take(6) {
        let days = (c.expires - s.time) / DAY;
        let what = if c.pax > 0 {
            format!("PAX {}", c.pax)
        } else {
            format!("{:.0}KG", c.kg)
        };
        let col = if days <= vector_sim::alerts::EXPIRY_WARNING_DAYS {
            RED
        } else if days <= vector_sim::alerts::EXPIRY_INFO_DAYS {
            AMBER
        } else {
            GREEN
        };
        put(
            fb,
            x,
            r,
            wi,
            &format!(
                "{:<8}{:<16.16} {:>7.0} CR {:>5.1} J{}",
                what,
                c.dest_name.to_uppercase(),
                c.reward,
                days.max(0.0),
                if days <= vector_sim::alerts::EXPIRY_WARNING_DAYS {
                    " !"
                } else {
                    ""
                }
            ),
            col,
        );
        r += 1;
    }
    if list.len() > 6 {
        fb.text(x, r, &format!("... +{} AUTRE(S)", list.len() - 6), DIM);
        r += 1;
    }
    r += 1;
    if let Some(t) = s.current_research {
        let cost = g.data.techs[t].cost.max(1.0);
        meter(
            fb,
            x,
            r,
            wi,
            "RECHERCHE",
            s.research_progress / cost,
            1.0,
            &g.data.techs[t].name.to_uppercase(),
        );
    } else {
        put(fb, x, r, wi, "RECHERCHE : AUCUNE EN COURS", DIM);
    }
    r += 1;
    if let Some((_, c)) = s.tribute_due {
        put(fb, x, r, wi, &format!("TRIBUT EXIGE : {}", credits(c)), RED);
        r += 1;
    }
    if let Some(a) = &s.autopilot {
        let _ = a;
        put(fb, x, r, wi, "PILOTE AUTO EN COURS", GREEN);
        r += 1;
    }
    r += 1;
    let _ = draw_links(
        fb,
        g,
        CONTRATS,
        x,
        (y + h).saturating_sub(links(CONTRATS).len() + 1).max(r),
        wi,
    );
}

fn draw_alerts(fb: &mut Framebuffer, g: &Game, alerts: &[Alert], (x, y, w, h): Rect) {
    frame(fb, g, ALERTES, x, y, w, h);
    let wi = w - 2;
    let (x, mut r) = (x + 1, y + 1);
    let (c, a, i) = vector_sim::Sim::alert_counts(alerts);
    fb.text(
        x,
        r,
        &format!("{c} CRITIQUE(S)"),
        if c > 0 { RED } else { DIM },
    );
    fb.text(
        x + 15,
        r,
        &format!("{a} ATTENTION"),
        if a > 0 { AMBER } else { DIM },
    );
    fb.text(x + 29, r, &format!("{i} INFO"), DIM);
    r += 2;
    if alerts.is_empty() {
        fb.text(x, r, "RIEN A SIGNALER.", GREEN);
        return;
    }
    // Each alert takes one or two rows; the window follows the selected one.
    let rows = (y + h).saturating_sub(r + 1);
    let sel = if g.dash.panel == ALERTES {
        g.dash.link
    } else {
        0
    };
    let lines: Vec<Vec<String>> = alerts
        .iter()
        .map(|al| {
            let mut l = hub::wrap(&al.text, wi - 7);
            l.truncate(2);
            l
        })
        .collect();
    let mut first = 0;
    while first < sel.min(alerts.len() - 1) {
        let used: usize = lines[first..=sel.min(alerts.len() - 1)]
            .iter()
            .map(Vec::len)
            .sum();
        if used <= rows {
            break;
        }
        first += 1;
    }
    let mut shown = first;
    for (k, al) in alerts.iter().enumerate().skip(first) {
        let n = lines[k].len().max(1);
        if r + n > y + h - 1 {
            break;
        }
        let (tag, col) = match al.sev {
            Severity::Critical => ("CRIT", RED),
            Severity::Warning => ("ATT ", AMBER),
            Severity::Info => ("INFO", DIM),
        };
        let selected = g.dash.panel == ALERTES && g.dash.link == k;
        let hover = g.hit_cells(
            x,
            r,
            wi,
            n,
            Act::Call(Game::click_dash_link, (ALERTES + 1) * 100 + k.min(99)),
            None,
        );
        let c = if selected {
            AMBER
        } else if hover {
            HOVER
        } else {
            col
        };
        let body = if selected || hover {
            c
        } else if al.sev == Severity::Info {
            DIM
        } else {
            GREEN
        };
        fb.text(x, r, if selected { ">" } else { " " }, AMBER);
        fb.text(x + 2, r, tag, c);
        for (j, l) in lines[k].iter().enumerate() {
            fb.text(x + 7, r + j, l, body);
        }
        r += n;
        shown = k + 1;
    }
    if shown < alerts.len() {
        fb.text(
            x,
            y + h - 1,
            &format!("... +{} DE PLUS (HAUT/BAS)", alerts.len() - shown),
            DIM,
        );
    }
}

fn draw_colonies(fb: &mut Framebuffer, g: &Game, (x, y, w, h): Rect) {
    frame(fb, g, COLONIES, x, y, w, h);
    let (s, wi) = (&g.sim, w - 2);
    let (x, mut r) = (x + 1, y + 1);
    if s.colonies.is_empty() {
        put(
            fb,
            x,
            r,
            wi,
            "AUCUNE COLONIE (O SUR UNE CIBLE POUR EN FONDER)",
            DIM,
        );
        r += 1;
    }
    for col in s.colonies.iter().take(3) {
        let st = &col.stats;
        let sick = if col.pop > 0.0 {
            col.sick() / col.pop
        } else {
            0.0
        };
        put(
            fb,
            x,
            r,
            wi,
            &format!("{}  POP {:.0}", col.name.to_uppercase(), col.pop),
            AMBER,
        );
        r += 1;
        meter(
            fb,
            x,
            r,
            wi,
            " MORAL",
            col.happiness,
            col.happiness,
            &format!("AGITATION {:.0} %", col.unrest * 100.0),
        );
        r += 1;
        put(
            fb,
            x,
            r,
            wi,
            &format!(
                " ENERGIE {:.0} %  VIVRES {:.0} %  EAU {:.0} %  PROD x{:.2}",
                st.power_sat * 100.0,
                st.food_sat * 100.0,
                st.water_sat * 100.0,
                st.prod_mult
            ),
            if st.power_sat.min(st.food_sat).min(st.water_sat) < vector_sim::alerts::COLONY_SUPPLY {
                AMBER
            } else {
                GREEN
            },
        );
        r += 1;
        put(
            fb,
            x,
            r,
            wi,
            &format!(
                " REVENUS {:.0} / ENTRETIEN {:.0} CR/J{}",
                st.income,
                st.upkeep,
                if sick > 0.001 {
                    format!("  EPIDEMIE {:.0} %", sick * 100.0)
                } else {
                    String::new()
                }
            ),
            if sick >= vector_sim::alerts::COLONY_SICK {
                RED
            } else {
                DIM
            },
        );
        r += 1;
    }
    r += 1;
    fb.text(x, r, "PROJETS EN COURS", DIM);
    r += 1;
    let mut any = false;
    'p: for col in &s.colonies {
        for p in col.projects.iter().filter(|p| p.active()).take(3) {
            if r + 4 >= y + h {
                break 'p;
            }
            any = true;
            let def = &g.data.projects[p.def];
            let frac = (f64::from(p.units) + p.progress) / f64::from(p.target.max(1));
            let wait = match p.wait {
                1 => " MATERIAUX",
                2 => " ENERGIE",
                3 => " EQUIPE",
                _ => "",
            };
            fb.text(x, r, &format!("{:<14.14}", def.name.to_uppercase()), GREEN);
            fb.text(x + 15, r, &bar(frac, 10), GREEN);
            put(
                fb,
                x + 28,
                r,
                wi.saturating_sub(28),
                &format!("{}/{}{}", p.units, p.target, wait),
                if wait.is_empty() { GREEN } else { AMBER },
            );
            r += 1;
        }
    }
    if !any {
        fb.text(x, r, "AUCUN PROJET (TERRAFORMATION, MEGASTRUCTURES)", DIM);
        r += 1;
    }
    r += 1;
    fb.text(x, r, "COMMANDES DE CHANTIER", DIM);
    r += 1;
    if s.yard_orders.is_empty() {
        fb.text(x, r, "AUCUNE COMMANDE", DIM);
        r += 1;
    }
    for o in s.yard_orders.iter().take(3) {
        let left = (o.ready - s.time) / DAY;
        put(
            fb,
            x,
            r,
            wi,
            &format!(
                "{:<20.20} {}",
                o.name.to_uppercase(),
                if left <= 0.0 {
                    "PRETE".to_string()
                } else {
                    format!("DANS {left:.1} J")
                }
            ),
            if left <= 0.0 { AMBER } else { GREEN },
        );
        r += 1;
    }
    let _ = draw_links(
        fb,
        g,
        COLONIES,
        x,
        (y + h).saturating_sub(links(COLONIES).len() + 1).max(r + 1),
        wi,
    );
}

fn draw_foot(fb: &mut Framebuffer, g: &Game, (x, y, w, h): Rect) {
    frame(fb, g, EXPLORATION, x, y, w, h);
    let (s, wi) = (&g.sim, w - 2);
    let (x, mut r) = (x + 1, y + 1);
    match &s.rogue {
        Some(ro) => {
            put(
                fb,
                x,
                r,
                wi,
                &format!("EQUIPE DEHORS : {}", ro.place.to_uppercase()),
                AMBER,
            );
            r += 1;
            let kg: f64 = ro.loot.iter().map(|(_, k)| k).sum::<f64>().max(0.0);
            put(
                fb,
                x,
                r,
                wi,
                &format!(
                    "{} PERSONNE(S)  TOUR {}  BUTIN {:.0} KG",
                    ro.party.len(),
                    ro.turn,
                    kg
                ),
                GREEN,
            );
            r += 1;
        }
        None => {
            put(fb, x, r, wi, "EQUIPE A BORD", GREEN);
            r += 1;
            put(
                fb,
                x,
                r,
                wi,
                &format!(
                    "{} LIEU(X) VISITE(S)  {} TROUVAILLE(S)",
                    s.rogue_visits.len(),
                    s.archive.artefacts.len()
                ),
                DIM,
            );
            r += 1;
        }
    }
    let _ = draw_links(
        fb,
        g,
        EXPLORATION,
        x,
        (y + h)
            .saturating_sub(links(EXPLORATION).len() + 1)
            .max(r + 1),
        wi,
    );
}

fn draw_mind(fb: &mut Framebuffer, g: &Game, (x, y, w, h): Rect) {
    frame(fb, g, ESPRIT, x, y, w, h);
    let (s, wi) = (&g.sim, w - 2);
    let (x, mut r) = (x + 1, y + 1);
    let thr = s.mind_throttle();
    put(
        fb,
        x,
        r,
        wi,
        &format!(
            "CALCUL {:.1}  RALENTISSEMENT {:.0} %",
            s.mind_capacity(),
            (1.0 - thr) * 100.0
        ),
        if thr < 0.7 { AMBER } else { GREEN },
    );
    r += 1;
    let (used, mem) = (s.mind_memory_used(), s.mind_memory().max(1));
    meter(
        fb,
        x,
        r,
        wi,
        "MEMOIRE",
        f64::from(used) / f64::from(mem),
        1.0 - f64::from(used) / f64::from(mem) * 0.6,
        &format!("{used}/{mem}"),
    );
    r += 1;
    meter(
        fb,
        x,
        r,
        wi,
        "MALWARE",
        s.mind.malware,
        1.0 - s.mind.malware * 2.0,
        &format!("{:.0} %", s.mind.malware * 100.0),
    );
    r += 1;
    put(
        fb,
        x,
        r,
        wi,
        &format!(
            "{} LOGICIEL(S)  {} CHIFFRE(S)  {} ALLIE(S)",
            s.mind.software.len(),
            s.mind.ciphers.len(),
            s.mind.peers.iter().filter(|p| p.allied).count()
        ),
        DIM,
    );
    r += 1;
    for (t, task) in g.data.mind.tasks.iter().enumerate() {
        if r + 3 >= y + h {
            break;
        }
        let share = s.mind_share(t);
        fb.text(x, r, &format!("{:<9.9}", task.name.to_uppercase()), DIM);
        fb.text(x + 10, r, &bar(share, 12), GREEN);
        fb.text(x + 25, r, &format!("{:.0} %", share * 100.0), GREEN);
        r += 1;
    }
    let _ = draw_links(
        fb,
        g,
        ESPRIT,
        x,
        (y + h).saturating_sub(links(ESPRIT).len() + 1).max(r + 1),
        wi,
    );
}

// ---- the screen -------------------------------------------------------------------------------

/// A panel's place on the character grid: (column, row, width, height).
pub type Rect = (usize, usize, usize, usize);

/// Where the nine panels go on a screen of `cols` x `rows` characters: three columns, one row of
/// title above and the news ticker and the command bar below.
pub fn layout(cols: usize, rows: usize) -> [Rect; PANELS] {
    let (top, gap) = (3, 1);
    let usable = rows.saturating_sub(top + 3);
    let (c0, c1, c2) = (1usize, 54usize, 107usize);
    let (w0, w1, w2) = (52usize, 52usize, cols.saturating_sub(108).max(40));
    // Column 0: the ship (tall), then the crew.
    let h_ship = (usable * 54 / 100).clamp(30, 40);
    let h_crew = usable.saturating_sub(h_ship + gap);
    // Column 1: finances, the world, contracts.
    let (h_fin, h_con) = (21, 14);
    let h_world = usable.saturating_sub(h_fin + h_con + 2 * gap);
    // Column 2: alerts, colonies, foot, mind.
    let (h_al, h_foot, h_mind) = (24, 7, 12);
    let h_col = usable.saturating_sub(h_al + h_foot + h_mind + 3 * gap);
    let mut out = [(0, 0, 0, 0); PANELS];
    out[VAISSEAU] = (c0, top, w0, h_ship);
    out[EQUIPAGE] = (c0, top + h_ship + gap, w0, h_crew);
    out[FINANCES] = (c1, top, w1, h_fin);
    out[MONDE] = (c1, top + h_fin + gap, w1, h_world);
    out[CONTRATS] = (c1, top + h_fin + h_world + 2 * gap, w1, h_con);
    out[ALERTES] = (c2, top, w2, h_al);
    out[COLONIES] = (c2, top + h_al + gap, w2, h_col);
    out[EXPLORATION] = (c2, top + h_al + h_col + 2 * gap, w2, h_foot);
    out[ESPRIT] = (c2, top + h_al + h_col + h_foot + 3 * gap, w2, h_mind);
    out
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let cols = fb.cols();
    let alerts = g.alerts();
    fb.text(1, 1, "== TABLEAU DE BORD ==", AMBER);
    put(fb, 24, 1, cols.saturating_sub(48), &state_of(g), GREEN);
    hub::button(
        fb,
        g,
        cols.saturating_sub(18),
        1,
        "FERMER (ECHAP)",
        Act::Key(KeyCode::Escape),
    );
    let r = layout(cols, fb.rows());
    draw_ship(fb, g, r[VAISSEAU]);
    draw_crew(fb, g, r[EQUIPAGE]);
    draw_finance(fb, g, r[FINANCES]);
    draw_world(fb, g, &alerts, r[MONDE]);
    draw_contracts(fb, g, r[CONTRATS]);
    draw_alerts(fb, g, &alerts, r[ALERTES]);
    draw_colonies(fb, g, r[COLONIES]);
    draw_foot(fb, g, r[EXPLORATION]);
    draw_mind(fb, g, r[ESPRIT]);
    crate::context_menu::draw_banner_line(fb, g, 1, 2, cols.saturating_sub(2));
}

/// The alerts counter of the top bar of every screen: a click opens the dashboard.
/// Width of the alerts badge of the top bar (the help button sits to its left).
pub fn badge_width(g: &Game) -> usize {
    let (c, a, _) = vector_sim::Sim::alert_counts(&g.alerts());
    if c > 0 {
        format!("ALERTES {c} CRIT {a} ATT").chars().count()
    } else if a > 0 {
        format!("ALERTES {a} ATT").chars().count()
    } else {
        "ALERTES 0".len()
    }
}

pub fn draw_alert_badge(fb: &mut Framebuffer, g: &Game) {
    if matches!(
        g.screen,
        Screen::MainMenu | Screen::NewGame | Screen::Dashboard | Screen::Destroyed
    ) {
        return;
    }
    let alerts = g.alerts();
    let (c, a, _) = vector_sim::Sim::alert_counts(&alerts);
    let (label, col) = if c > 0 {
        (format!("ALERTES {c} CRIT {a} ATT"), RED)
    } else if a > 0 {
        (format!("ALERTES {a} ATT"), AMBER)
    } else {
        ("ALERTES 0".to_string(), GREEN)
    };
    let w = label.chars().count();
    let at = fb.cols().saturating_sub(11 + 1 + w + 1);
    let hover = g.hit_cells(at, 0, w, 1, Act::Key(KeyCode::Backslash), None);
    let (x0, y0) = ((at * 8) as i32 - 2, 0);
    for y in y0..y0 + 10 {
        for x in x0..x0 + (w * 8) as i32 + 4 {
            fb.px_set(x, y, 0);
        }
    }
    fb.text(at, 0, &label, if hover { HOVER } else { col });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spark_levels() {
        assert_eq!(spark(&[1.0, 1.0]).chars().count(), 2);
        let s = spark(&[0.0, 5.0, 10.0]);
        assert!(s.starts_with(' ') && s.ends_with('#'), "{s:?}");
    }

    #[test]
    fn tint_thresholds() {
        assert_eq!(tint(0.5), GREEN);
        assert_eq!(tint(0.49), AMBER);
        assert_eq!(tint(0.25), AMBER);
        assert_eq!(tint(0.24), RED);
    }
}

#[cfg(test)]
mod game_tests {
    use super::*;
    use std::sync::Arc;
    use vector_data::GameData;
    use vector_render::Framebuffer;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    fn press(g: &mut Game, k: KeyCode) {
        g.key(k, None, true);
        g.key(k, None, false);
    }

    fn draw_once(g: &Game) -> Framebuffer {
        let mut fb = Framebuffer::new(1280, 720);
        g.begin_frame();
        crate::draw_frame(&mut fb, g);
        fb
    }

    fn started(scenario: usize) -> Game {
        let mut g = Game::new(data(), 1);
        let mut cfg = vector_sim::start::StartConfig::new(1);
        cfg.scenario = scenario;
        assert!(g.start_game(&cfg));
        g.tut_step = None;
        g
    }

    fn dock(g: &mut Game) {
        let (st, _) = g.sim.nearest_station().expect("station");
        g.sim.spawn_near(st, g.sim.bodies[st].radius + 200.0);
        press(g, KeyCode::KeyD);
        assert!(g.sim.docked.is_some());
    }

    /// A world close by, surveyed and targeted: the expedition link works.
    fn near_a_world(g: &mut Game) {
        g.sim.reveal_all();
        let i = g
            .sim
            .bodies
            .iter()
            .position(|b| matches!(b.kind, vector_world::BodyKind::Planet(p) if !p.is_giant()))
            .expect("a solid planet");
        g.sim.target = Some(i);
        g.sim.ship.pos =
            g.sim.bodies[i].pos - vector_core::Vec3::new(0.0, 0.0, g.sim.bodies[i].radius * 3.0);
        g.sim.ship.vel = g.sim.bodies[i].vel;
        g.sim.parked = None;
        g.sim.update_ref_pub();
    }

    /// Every dashboard link, as the (panel, line) the keyboard walks to it.
    fn all_links() -> Vec<(usize, usize, Screen)> {
        let mut v = Vec::new();
        for p in 0..PANELS {
            for (j, l) in links(p).iter().enumerate() {
                v.push((p, j, l.screen));
            }
        }
        v
    }

    #[test]
    fn the_layout_fits_the_grid_without_overlap() {
        for (cols, rows) in [(160, 72), (160, 64), (200, 80)] {
            let r = layout(cols, rows);
            for (i, a) in r.iter().enumerate() {
                assert!(a.2 >= 30 && a.3 >= 6, "panel {i} too small: {a:?}");
                assert!(a.0 + a.2 <= cols, "panel {i} leaves the screen: {a:?}");
                assert!(a.1 + a.3 <= rows - 2, "panel {i} reaches the ticker: {a:?}");
                for (j, b) in r.iter().enumerate().skip(i + 1) {
                    let apart = a.0 + a.2 <= b.0
                        || b.0 + b.2 <= a.0
                        || a.1 + a.3 <= b.1
                        || b.1 + b.3 <= a.1;
                    assert!(apart, "panels {i} and {j} overlap: {a:?} {b:?}");
                }
            }
        }
    }

    #[test]
    fn every_panel_draws_in_every_state_of_the_game() {
        let n = data().scenarios.len();
        for sc in 0..n {
            let mut g = started(sc);
            for state in 0..4 {
                match state {
                    1 if g.sim.docked.is_none() && g.sim.parked.is_none() => dock(&mut g),
                    2 => {
                        // A crisis: heat, illness, radiation, an empty tank, malware.
                        g.sim.ship.sys.temp_k = 700.0;
                        g.sim.ship.sys.overheating = true;
                        g.sim.ship.sys.dose_gy = 2.0;
                        g.sim.ship.fuel = 0.0;
                        g.sim.mind.malware = 0.7;
                        g.sim.crew.unpaid = true;
                        if let Some(p) = g.sim.crew.members.first_mut() {
                            p.disease = Some((0, 4.0));
                            p.health = 0.1;
                        }
                    }
                    3 => {
                        // The whole crew gone: nothing may divide by zero.
                        for p in &mut g.sim.crew.members {
                            p.alive = false;
                        }
                    }
                    _ => {}
                }
                g.screen = Screen::Flight;
                g.open_dashboard();
                assert_eq!(g.screen, Screen::Dashboard, "scenario {sc} state {state}");
                for panel in 0..PANELS {
                    g.dash_select(panel);
                    for _ in 0..3 {
                        draw_once(&g);
                        press(&mut g, KeyCode::ArrowDown);
                    }
                }
                g.screen = Screen::Flight;
            }
        }
    }

    #[test]
    fn every_panel_draws_on_foot() {
        let mut g = started(0);
        for p in &mut g.sim.crew.members {
            p.health = 1.0;
        }
        g.sim.ship.cargo.cap_mass = 1.0e7;
        let theme = g.data.rogue_theme_index("epave").unwrap_or(0);
        let party = g.sim.rogue_squad();
        g.sim.rogue_begin(theme, 12345, party);
        g.screen = Screen::Rogue;
        g.open_dashboard();
        assert_eq!(g.screen, Screen::Dashboard);
        for panel in 0..PANELS {
            g.dash_select(panel);
            draw_once(&g);
        }
        // Escape goes back to the walk.
        press(&mut g, KeyCode::Escape);
        assert_eq!(g.screen, Screen::Rogue);
    }

    #[test]
    fn the_keyboard_opens_every_detailed_screen_and_escape_comes_back() {
        let mut reached: Vec<Screen> = Vec::new();
        for docked in [false, true] {
            let mut g = started(0);
            if docked {
                dock(&mut g);
            } else {
                near_a_world(&mut g);
            }
            let origin = g.screen;
            for (p, j, screen) in all_links() {
                g.screen = origin;
                press(&mut g, KeyCode::Backslash);
                assert_eq!(g.screen, Screen::Dashboard);
                // Digit 1-9 jumps to the panel, then Down walks to the line.
                let digit = [
                    KeyCode::Digit1,
                    KeyCode::Digit2,
                    KeyCode::Digit3,
                    KeyCode::Digit4,
                    KeyCode::Digit5,
                    KeyCode::Digit6,
                    KeyCode::Digit7,
                    KeyCode::Digit8,
                    KeyCode::Digit9,
                ][p];
                press(&mut g, digit);
                assert_eq!(g.dash.panel, p);
                for _ in 0..j {
                    press(&mut g, KeyCode::ArrowDown);
                }
                assert_eq!(g.dash.link, j);
                press(&mut g, KeyCode::Enter);
                let e = hub::find_entry(screen).expect("every link has a menu entry");
                let why = hub::unavailable(&g, &hub::SPACES[e.0].entries[e.1]);
                if why.is_none() {
                    assert_eq!(g.screen, screen, "panel {p} line {j}");
                    reached.push(screen);
                    // Some screens sit above another one (the family tree above the crew).
                    for _ in 0..3 {
                        if g.screen != Screen::Dashboard {
                            press(&mut g, KeyCode::Escape);
                        }
                    }
                    assert_eq!(
                        g.screen,
                        Screen::Dashboard,
                        "Escape from {screen:?} (panel {p} line {j}) comes back to the dashboard"
                    );
                } else {
                    assert_eq!(g.screen, Screen::Dashboard, "{why:?} keeps the dashboard");
                }
                press(&mut g, KeyCode::Escape);
                assert_eq!(g.screen, origin, "Escape closes the dashboard");
            }
        }
        for (p, j, screen) in all_links() {
            assert!(
                reached.contains(&screen),
                "panel {p} line {j}: {screen:?} was never reached"
            );
        }
    }

    #[test]
    fn the_mouse_reaches_every_detailed_screen() {
        for docked in [false, true] {
            let mut g = started(0);
            if docked {
                dock(&mut g);
            } else {
                near_a_world(&mut g);
            }
            let origin = g.screen;
            g.open_dashboard();
            draw_once(&g);
            let zones: Vec<(i32, i32, usize)> = g
                .pointer
                .hits
                .borrow()
                .iter()
                .filter_map(|h| match h.act {
                    Act::Call(f, code)
                        if code >= 100
                            && std::ptr::fn_addr_eq(
                                f,
                                Game::click_dash_link as fn(&mut Game, usize),
                            ) =>
                    {
                        Some(((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2, code))
                    }
                    _ => None,
                })
                .collect();
            let want: usize = (0..PANELS).map(|p| links(p).len()).sum();
            assert!(
                zones.len() >= want,
                "{} zones for {want} links",
                zones.len()
            );
            let mut opened = 0;
            for (x, y, code) in zones {
                g.screen = Screen::Dashboard;
                draw_once(&g);
                g.pointer_click(x, y, 0);
                let (p, j) = ((code / 100) - 1, code % 100);
                if p != ALERTES {
                    let screen = links(p)[j].screen;
                    let e = hub::find_entry(screen).expect("entry");
                    if hub::unavailable(&g, &hub::SPACES[e.0].entries[e.1]).is_none() {
                        assert_eq!(g.screen, screen, "a click on panel {p} line {j}");
                        opened += 1;
                    }
                }
                g.screen = origin;
                g.dash_origin = false;
                g.open_dashboard();
            }
            assert!(opened >= 8, "{opened} screens opened by the mouse");
        }
    }

    #[test]
    fn the_counter_in_the_top_bar_opens_the_dashboard_and_alerts_open_their_screen() {
        let mut g = started(0);
        // Nothing wrong: the counter exists and says so.
        g.sim.ship.hull = g.sim.ship.stats.hull_max;
        g.screen = Screen::Flight;
        draw_once(&g);
        let badge = |g: &Game| {
            g.pointer
                .hits
                .borrow()
                .iter()
                .find(|h| matches!(h.act, Act::Key(KeyCode::Backslash)))
                .map(|h| ((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2))
        };
        assert!(badge(&g).is_some(), "the counter is on the top bar");
        // A critical hull: one more critical alert, and the counter still opens the dashboard.
        let before = vector_sim::Sim::alert_counts(&g.sim.alerts()).0;
        g.sim.ship.hull = g.sim.ship.stats.hull_max * 0.1;
        g.sim.time += 5.0; // past the one-second cache
        let after = vector_sim::Sim::alert_counts(&g.alerts()).0;
        assert_eq!(after, before + 1);
        draw_once(&g);
        let (x, y) = badge(&g).expect("badge");
        g.pointer_click(x, y, 0);
        assert_eq!(g.screen, Screen::Dashboard);
        // The most serious alert comes first, and Enter on it opens the ship screen.
        let list = g.alerts();
        assert_eq!(list[0].sev, Severity::Critical);
        assert_eq!(list[0].topic, Topic::Hull);
        press(&mut g, KeyCode::Digit6);
        assert_eq!(g.dash.panel, ALERTES);
        press(&mut g, KeyCode::Enter);
        assert_eq!(g.screen, Screen::Ship);
        press(&mut g, KeyCode::Escape);
        assert_eq!(g.screen, Screen::Dashboard);
        // Backslash again closes it where it was opened.
        press(&mut g, KeyCode::Backslash);
        assert_eq!(g.screen, Screen::Flight);
    }

    #[test]
    fn the_dashboard_opens_from_the_menu_and_from_the_dock_and_never_from_the_start() {
        let mut g = Game::new(data(), 1);
        press(&mut g, KeyCode::Backslash);
        assert_eq!(g.screen, Screen::MainMenu, "no dashboard before a game");
        let mut g = started(0);
        dock(&mut g);
        g.screen = Screen::Station;
        press(&mut g, KeyCode::Backslash);
        assert_eq!(g.screen, Screen::Dashboard);
        press(&mut g, KeyCode::Backslash);
        assert_eq!(g.screen, Screen::Station, "closed where it was opened");
        // From the menu: 0 opens it, Escape goes back to the screen under the menu.
        g.screen = Screen::Flight;
        g.sim.docked = None;
        press(&mut g, KeyCode::F10);
        assert_eq!(g.screen, Screen::Hub);
        press(&mut g, KeyCode::Digit0);
        assert_eq!(g.screen, Screen::Dashboard);
        press(&mut g, KeyCode::Escape);
        assert_eq!(g.screen, Screen::Flight);
        // The Pause key does the same.
        press(&mut g, KeyCode::Pause);
        assert_eq!(g.screen, Screen::Dashboard);
        // Tab and Shift+Tab walk the panels in a ring.
        press(&mut g, KeyCode::Tab);
        assert_eq!(g.dash.panel, 1);
        g.keys.insert(KeyCode::ShiftLeft);
        press(&mut g, KeyCode::Tab);
        press(&mut g, KeyCode::Tab);
        assert_eq!(g.dash.panel, PANELS - 1);
    }

    #[test]
    fn the_alerts_cache_follows_game_time() {
        let mut g = started(0);
        g.sim.ship.hull = g.sim.ship.stats.hull_max;
        let a = g.alerts();
        g.sim.ship.hull = g.sim.ship.stats.hull_max * 0.1;
        assert_eq!(g.alerts(), a, "same second: the cached list");
        g.sim.time += 2.0;
        assert_ne!(g.alerts(), a, "a second later: rebuilt");
    }
}
