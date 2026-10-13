//! THE COCKPIT: the flight view as a real dashboard. The scene (3D, vectors) stays visible in a
//! window; around it a console of instruments (artificial horizon, heading tape, dials, LED
//! gauges, radar, navigation and plan screens, alert lamps) and controls (toggle switches, lit
//! push buttons, the thrust lever, console keys). Four arrangements to choose from in the options
//! (`Settings::cockpit`): A (low wide console), B (side panels and a compact console), C (minimal
//! glass HUD and a folding console) and the OLD HUD of the game.
//!
//! Every organ does exactly what its key does: its zone declares the same `Act::Key` the keyboard
//! sends (one action = one identifier, never a second implementation). What has a position
//! (the thrust lever, the radar blips) calls the real command with that position.
//!
//! Layout and sizes: the arrangements are computed from the window (`layouts`): the modules
//! stretch between a minimum and a maximum width and the secondary organs fall away when the
//! window is too small, so that nothing overflows from 1024x576 to full screen.
//!
//! Performance: no allocation per frame in the console (the scene buffer is kept, the zones
//! and organs vectors keep their capacity, the heavy figures are cached a few times a second),
//! opaque spans instead of pixel by pixel, a lookup table for the horizon.
pub mod controls;
pub mod draw;
pub mod instruments;
pub mod layouts;
pub mod lights;
pub mod scenes;
#[cfg(test)]
mod tests;

use crate::context_menu::Item;
use crate::game::Game;
use crate::hud::View;
use crate::mouse::Act;
use std::cell::{Cell, RefCell};
use vector_core::Vec3;
use vector_render::Framebuffer;
use vector_sim::landing::{PilotKind, Readout, Sas};
use winit::keyboard::KeyCode;

/// The arrangement of the cockpit chosen in the options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// Low wide console, like an airliner.
    A,
    /// Panels on the sides and a compact console.
    B,
    /// Minimal glass HUD and a console that folds down.
    C,
    /// The HUD of the game before the cockpit.
    Old,
}

impl Disposition {
    pub const ALL: [Disposition; 4] = [
        Disposition::A,
        Disposition::B,
        Disposition::C,
        Disposition::Old,
    ];

    pub fn from_index(i: u8) -> Disposition {
        Disposition::ALL[usize::from(i).min(3)]
    }

    pub fn index(self) -> u8 {
        match self {
            Disposition::A => 0,
            Disposition::B => 1,
            Disposition::C => 2,
            Disposition::Old => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Disposition::A => "A : CONSOLE BASSE LARGE",
            Disposition::B => "B : PANNEAUX LATERAUX",
            Disposition::C => "C : HUD MINIMAL, CONSOLE REPLIABLE",
            Disposition::Old => "ANCIEN HUD",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Disposition::A => "A",
            Disposition::B => "B",
            Disposition::C => "C",
            Disposition::Old => "ANCIEN",
        }
    }

    pub fn next(self) -> Disposition {
        Disposition::from_index((self.index() + 1) % 4)
    }
}

/// A light profiler: the time spent in each part of the cockpit, summed over the frames (two
/// clock reads per part; `vector-game --bench` prints the shares).
pub mod prof {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    pub const NAMES: [&str; 11] = [
        "telemetry",
        "scene (buffer, copy)",
        "glass texts",
        "systems",
        "commands",
        "flight instruments",
        "radar",
        "nav / descent screen",
        "target and plan screens",
        "throttle",
        "keys and the rest",
    ];
    static NS: [AtomicU64; 11] = [const { AtomicU64::new(0) }; 11];

    /// Run `f` and add its time to part `slot`.
    #[inline]
    pub fn time<R>(slot: usize, f: impl FnOnce() -> R) -> R {
        let t = Instant::now();
        let r = f();
        NS[slot].fetch_add(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
        r
    }

    /// The parts and their total milliseconds, then forget them.
    pub fn take() -> Vec<(&'static str, f64)> {
        NAMES
            .iter()
            .zip(NS.iter())
            .map(|(n, c)| (*n, c.swap(0, Ordering::Relaxed) as f64 / 1e6))
            .collect()
    }
}

// ---- the organs ---------------------------------------------------------------------------------

/// What an organ of the console is (for its tooltip, its menu and its help).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    Gear,
    Shield,
    Radar,
    Decoy,
    Lights,
    Auto,
    Sas,
    Lever,
    Trim,
    SpeedDial,
    SpeedSel,
    Attitude,
    Tape,
    Vario,
    Digital,
    Systems,
    Lamp,
    Scope,
    Blip,
    NavScreen,
    DescentScreen,
    DockScreen,
    PlanScreen,
    PlanBtn,
    PilotScreen,
    PilotBtn,
    Target,
    TargetBtn,
    FKey,
    Handle,
    Layout,
}

/// One organ drawn this frame: the index of its zone in the zones of the frame, what it is, and
/// its number among its kind (which SAS button, which lamp...).
#[derive(Clone, Copy, Debug)]
pub struct Organ {
    pub hit: usize,
    pub id: Id,
    pub n: u8,
}

/// The name, the key and the help of an organ.
pub fn spec(id: Id, n: u8) -> (&'static str, &'static str, &'static str) {
    match id {
        Id::Gear => (
            "TRAIN D'ATTERRISSAGE",
            "PAV3",
            "Sort ou rentre le train. Sorti : contact sur a 6 m/s ; rentre : 1,5 m/s.",
        ),
        Id::Shield => (
            "BOUCLIER",
            "PAV/",
            "Met les generateurs de bouclier en marche ou a l'arret. A l'arret ils ne consomment rien et ne protegent plus ; remis en marche le bouclier se recharge.",
        ),
        Id::Radar => (
            "RADAR ACTIF",
            ".",
            "Radar actif : vous voyez plus loin, mais on vous voit de tres loin.",
        ),
        Id::Decoy => (
            "LEURRE THERMIQUE",
            ",",
            "Lance un leurre thermique : les missiles a tete chercheuse le prennent pour cible.",
        ),
        Id::Lights => (
            "LUMIERES",
            "PAV.",
            "Projecteurs exterieurs : visibles en vue externe. Ils consomment et un vaisseau eclaire se voit de plus loin (pirates, patrouilles).",
        ),
        Id::Auto => (
            "PILOTE AUTO",
            "G",
            "Pilote automatique vers la cible (ou le pilote de descente et d'amarrage).",
        ),
        Id::Sas => (
            "SAS : MAINTIEN D'ATTITUDE",
            "0-9",
            "Maintient le nez dans une direction : cap retenu, prograde, retrograde, normale, radial, cible, verticale.",
        ),
        Id::Lever => (
            "MANETTE DE POUSSEE",
            "PAV8/2",
            "Accelerateur de 0 a 100 %. Glisser, cliquer ou molette ; Pave num. 8/2 : plus ou moins 10 %, 0 coupe, 5 plein. Espace pousse a ce taux.",
        ),
        Id::Trim => (
            "TRIM DE POUSSEE",
            "[ ]",
            "Reglage fin de la poussee, plus ou moins 10 crans d'un pour cent, ajoute a la manette. Clic sur la valeur : remise a zero.",
        ),
        Id::SpeedDial => (
            "CADRAN DE VITESSE",
            "-",
            "Vitesse par rapport au sol, a l'orbite ou a la cible (selecteur SOL / ORB / CIB).",
        ),
        Id::SpeedSel => (
            "SELECTEUR DE VITESSE",
            "-",
            "Choisit ce que mesure le cadran : sol, orbite (repere de reference) ou cible.",
        ),
        Id::Attitude => (
            "HORIZON ARTIFICIEL",
            "-",
            "Attitude reelle du vaisseau contre l'horizon local (celui du monde en dessous, sinon le plan du systeme).",
        ),
        Id::Tape => (
            "BANDE DE CAP",
            "-",
            "Cap en degres par rapport au nord du monde sous le vaisseau (ou du systeme).",
        ),
        Id::Vario => (
            "VARIOMETRE",
            "-",
            "Vitesse verticale : montee en haut, descente en bas, echelle automatique.",
        ),
        Id::Digital => match n {
            0 => (
                "ALTIMETRE RADAR",
                "-",
                "Hauteur au-dessus du sol du monde survole.",
            ),
            1 => (
                "VITESSE VERTICALE",
                "-",
                "Positive en montee, negative en descente.",
            ),
            _ => (
                "VITESSE HORIZONTALE",
                "-",
                "Vitesse le long du sol.",
            ),
        },
        Id::Systems => (
            "JAUGES DES SYSTEMES",
            "-",
            "Carburant, delta-v, batterie, coque, bouclier, temperature, radiation et charge en G. Vert, ambre puis rouge ; clignote en alarme.",
        ),
        Id::Lamp => (
            "VOYANTS D'ALERTE",
            "\\",
            "Ambre fixe : a surveiller. Rouge qui clignote : critique. Collision, carburant, surchauffe, radiation, coque, energie, bouclier, air.",
        ),
        Id::Scope => (
            "RADAR / SCANNER",
            "TAB",
            "Le vaisseau au centre, le nez en haut, echelle logarithmique de 1 km a 1 million de km. Clic sur un contact : le cibler ; ailleurs : cible suivante.",
        ),
        Id::Blip => (
            "CONTACT RADAR",
            "TAB",
            "Un corps ou un vaisseau vu par le radar. Clic : le cibler.",
        ),
        Id::NavScreen => (
            "ECRAN DE NAVIGATION",
            "-",
            "Orbite autour du monde de reference, periapsis, apoapsis, prochain evenement et temps avant allumage.",
        ),
        Id::DescentScreen => (
            "ECRAN DE DESCENTE",
            "-",
            "Altitude radar, temps avant l'impact, altitude d'allumage, poussee sur poids, pente et site.",
        ),
        Id::DockScreen => (
            "ECRAN D'AMARRAGE",
            "-",
            "Distance au port, vitesse de rapprochement, derive laterale et alignement du nez.",
        ),
        Id::PlanScreen => (
            "PLAN AUTOMATIQUE",
            "PAV*",
            "La file d'ordres de l'ordinateur de bord : etat, ordre courant et suivants.",
        ),
        Id::PlanBtn => match n {
            0 => ("LANCER / PAUSE", "-", "Lance ou met en pause le plan de navigation."),
            1 => ("ECRAN DE NAVIGATION AUTO", "PAV*", "Ouvre l'ecran complet de la navigation automatique."),
            _ => ("TRAJECTOIRE PREVUE", "-", "Montre ou cache la trajectoire prevue dans la verriere."),
        },
        Id::PilotScreen => (
            "PILOTE ET AIDES",
            "-",
            "Les aides de l'approche : pilote de descente, site plat, vue du sol, accelerateur.",
        ),
        Id::PilotBtn => match n {
            0 => ("PILOTE", "PAV4", "Engage ou debraye le pilote (atterrissage, decollage, amarrage)."),
            1 => ("SITE PLAT", "PAV7", "Marque le site le plus plat pres du point sous le vaisseau."),
            2 => ("VUE SOL", "PAV1", "Regarde derriere (le sol quand le nez est en haut)."),
            3 => ("ACCEL +", "PAV8", "Accelerateur plus 10 %."),
            4 => ("ACCEL -", "PAV2", "Accelerateur moins 10 %."),
            _ => ("COUPURE", "PAV0", "Accelerateur a zero."),
        },
        Id::Target => (
            "CIBLE",
            "TAB",
            "Nom, distance, approche, radiation, coque et bouclier de la cible choisie.",
        ),
        Id::TargetBtn => match n {
            0 => ("AMARRER", "D", "Amarre a la station proche (moins de 500 m, vitesse relative sous 8 m/s)."),
            1 => ("ATTERRIR", "PAV4", "Engage le pilote d'atterrissage sur le monde cible."),
            _ => ("APPELER", "ENTR", "Appelle la cible par radio."),
        },
        Id::FKey => ("TOUCHE DE CONSOLE", "-", "Ouvre l'ecran correspondant."),
        Id::Handle => (
            "POIGNEE DE CONSOLE",
            "'",
            "Deplie ou replie la console (disposition C). L'etat est memorise.",
        ),
        Id::Layout => (
            "DISPOSITION DU COCKPIT",
            "MAJ+F11",
            "Change l'arrangement du cockpit : A, B, C ou l'ancien HUD. Sauvegarde dans les options.",
        ),
    }
}

// ---- state --------------------------------------------------------------------------------------

/// The needles and bars as the player sees them: they follow the simulation smoothly.
#[derive(Default)]
pub struct Smooth {
    init: bool,
    t: f32,
    spd: f64,
    vv: f64,
    ux: f32,
    uy: f32,
    sinp: f32,
    hdg: f32,
    thr: f64,
    bars: [f64; 8],
}

/// What is expensive and changes slowly: refreshed a few times a second.
#[derive(Default)]
pub struct Slow {
    at: f32,
    sim_at: f64,
    valid: bool,
    dv_full: f64,
    /// The level (0 none, 1 warning, 2 critical) the alerts of the game give each lamp.
    alerts: [u8; 8],
    env_radiation: f64,
}

/// Geometry of the thrust lever slot, for the drag and the wheel.
#[derive(Clone, Copy, Default)]
pub struct Lever {
    pub top: i32,
    pub bottom: i32,
}

/// Everything the cockpit keeps between two frames (none of it is the game's own state).
#[derive(Default)]
pub struct State {
    /// The organs drawn in this frame (cleared with the zones).
    pub organs: RefCell<Vec<Organ>>,
    pub sm: RefCell<Smooth>,
    pub slow: RefCell<Slow>,
    /// What the speed dial measures, plus one: 1 ground, 2 orbit, 3 target; 0 = automatic (the
    /// ground when there is one under the ship, else the orbit).
    pub speed_mode: Cell<u8>,
    /// The scene of the window, kept from one frame to the next.
    pub scene: RefCell<Option<Framebuffer>>,
    /// The organ whose help is open.
    pub help: Cell<Option<(Id, u8)>>,
    pub lever: Cell<Lever>,
    /// The left button is dragging the thrust lever.
    pub drag: Cell<bool>,
    /// The piloting menu (F11) is shown over the window.
    pub menu: Cell<bool>,
    /// The current layout the last frame used (for tests and tooltips).
    pub last: Cell<Option<Disposition>>,
    /// Frame time of the last draw, ms (the bench and the tests read it).
    pub radar_pts: RefCell<Vec<(i32, i32)>>,
}

impl State {
    /// Forget the cached figures (a test that edits the ship).
    #[cfg(test)]
    pub fn invalidate(&self) {
        self.slow.borrow_mut().valid = false;
        self.sm.borrow_mut().init = false;
    }
}

// ---- telemetry ----------------------------------------------------------------------------------

/// The numbers the instruments show for this frame.
pub struct Tel {
    pub t: f32,
    pub blink: bool,
    pub blink_slow: bool,
    /// 0..1, a slow pulse for the glow of the lit controls.
    pub pulse: f32,
    pub spd_orb: f64,
    pub spd_surf: Option<f64>,
    pub spd_tgt: Option<f64>,
    /// Smoothed speed of the dial and its mode (0 ground, 1 orbit, 2 target).
    pub spd: f64,
    pub spd_mode: usize,
    pub vv_s: f64,
    pub ux: f32,
    pub uy: f32,
    pub sinp: f32,
    pub heading: f32,
    pub alt: Option<f64>,
    pub vv: Option<f64>,
    pub vh: Option<f64>,
    pub fuel: f64,
    pub dv: f64,
    pub dv_frac: f64,
    pub batt: f64,
    pub temp_k: f64,
    pub shield: Option<f64>,
    pub shield_on: bool,
    pub shield_installed: bool,
    pub hull: f64,
    pub dose_mgy: f64,
    pub dose_frac: f64,
    pub gload: f64,
    pub g_frac: f64,
    /// Smoothed values of the eight LED columns (fuel, delta-v, battery, hull, shield, temperature,
    /// radiation, load).
    pub bars: [f64; 8],
    pub thr: f64,
    pub thr_vis: f64,
    pub trim: i8,
    pub gear: bool,
    pub sas: Sas,
    pub assist: bool,
    pub auto: bool,
    pub radar_on: bool,
    pub decoys: usize,
    pub lights: bool,
    pub pilot: Option<PilotKind>,
    pub landing: Option<Readout>,
    pub dock: Option<vector_sim::landing::DockReadout>,
    pub lamps: [(&'static str, u8); 8],
}

impl Tel {
    pub fn new(g: &Game) -> Tel {
        let sim = &g.sim;
        let ship = &sim.ship;
        let t = crate::proto_view::anim_time();
        let surf = sim.surface_state();
        let up_w = if let Some(s) = &surf {
            s.up
        } else if let Some(b) = sim.ref_body {
            (ship.pos - sim.bodies[b].pos).normalized()
        } else {
            Vec3::new(0.0, 1.0, 0.0)
        };
        let f = ship.forward();
        let (sr, su) = (ship.rot.col(0), ship.rot.col(1));
        let (ux, uy) = (up_w.dot(sr) as f32, up_w.dot(su) as f32);
        let sinp = f.dot(up_w).clamp(-1.0, 1.0) as f32;
        let mut north = Vec3::new(0.0, 1.0, 0.0);
        if north.dot(up_w).abs() > 0.95 {
            north = Vec3::new(1.0, 0.0, 0.0);
        }
        let north = (north - up_w * north.dot(up_w)).normalized();
        let east = north.cross(up_w);
        let mut heading = f.dot(east).atan2(f.dot(north)).to_degrees() as f32;
        if heading < 0.0 {
            heading += 360.0;
        }
        let rel = sim.rel_velocity();
        let spd_tgt = if let Some(n) = sim.target_npc.and_then(|k| sim.npcs.get(k)) {
            Some((ship.vel - n.vel).length())
        } else {
            sim.target
                .and_then(|i| sim.bodies.get(i))
                .map(|b| (ship.vel - b.vel).length())
        };
        let landing = sim.landing_readout();
        // The slow figures: a few times a second (or when the game clock jumps).
        {
            let mut s = g.cockpit.slow.borrow_mut();
            if !s.valid || (t - s.at).abs() > 0.4 || (sim.time - s.sim_at).abs() > 0.5 {
                let mut full = ship.clone();
                full.fuel = full.stats.fuel_cap;
                s.dv_full = full.delta_v().max(1.0);
                s.env_radiation = sim.environment_at(ship.pos).radiation;
                s.alerts = [0; 8];
                for a in g.alerts() {
                    use vector_sim::alerts::{Severity, Topic};
                    let lamp = match a.topic {
                        Topic::Collision => 0,
                        Topic::Fuel => 1,
                        Topic::Heat => 2,
                        Topic::Radiation => 3,
                        Topic::Hull | Topic::Modules | Topic::Structure => 4,
                        Topic::Power | Topic::Antimatter => 5,
                        Topic::Needs | Topic::Health | Topic::Cryo | Topic::Wounds => 7,
                        _ => continue,
                    };
                    let level = match a.sev {
                        Severity::Critical => 2,
                        Severity::Warning => 1,
                        Severity::Info => 0,
                    };
                    s.alerts[lamp] = s.alerts[lamp].max(level);
                }
                s.at = t;
                s.sim_at = sim.time;
                s.valid = true;
            }
        }
        let (dv_full, env_rad, from_alerts) = {
            let s = g.cockpit.slow.borrow();
            (s.dv_full, s.env_radiation, s.alerts)
        };
        let dv = ship.delta_v();
        let fuel = ship.fuel / ship.stats.fuel_cap.max(1.0);
        let batt = ship.sys.battery_mj / ship.stats.battery_cap.max(1.0);
        let installed = sim.shield_installed();
        let cap = sim.effective.shield_cap;
        let shield = if sim.shield_on {
            (cap > 0.0).then(|| (ship.sys.shield_pts / cap).clamp(0.0, 1.0))
        } else {
            installed.then_some(0.0)
        };
        let hull = ship.hull / ship.stats.hull_max.max(1.0);
        let rate = sim.dose_rate_aboard();
        let gload = sim.structural_load_g();
        let temp =
            ((ship.sys.temp_k - 150.0) / (vector_sim::design::T_SCRAM - 150.0)).clamp(0.0, 1.0);
        let impact = landing
            .as_ref()
            .is_some_and(|r| r.burn_now || r.t_impact.is_some_and(|t| t < 12.0 && r.v_up < -8.0));
        let lvl = |bad2: bool, bad1: bool| -> u8 {
            if bad2 {
                2
            } else {
                u8::from(bad1)
            }
        };
        let shield_lamp = if installed && !sim.shield_on {
            1
        } else {
            lvl(
                shield.is_some_and(|s| s < 0.1),
                shield.is_some_and(|s| s < 0.25),
            )
        };
        // Each lamp is the worse of what the alerts of the game say (the dashboard's list, the
        // same thresholds) and the live figure, so that it never lags behind the gauge.
        let lamps = [
            ("COLLISION", if impact { 2 } else { from_alerts[0] }),
            (
                "CARBURANT",
                lvl(fuel < 0.1, fuel < 0.25).max(from_alerts[1]),
            ),
            (
                "SURCHAUFFE",
                lvl(
                    ship.sys.temp_k > vector_sim::design::T_DAMAGE,
                    ship.sys.temp_k > 500.0,
                )
                .max(from_alerts[2]),
            ),
            (
                "RADIATION",
                lvl(rate > 0.05, rate > 0.005 || env_rad > 0.0).max(from_alerts[3]),
            ),
            ("COQUE", lvl(hull < 0.25, hull < 0.5).max(from_alerts[4])),
            ("ENERGIE", lvl(batt < 0.1, batt < 0.25).max(from_alerts[5])),
            ("BOUCLIER", shield_lamp),
            (
                "AIR / VIE",
                lvl(sim.crew.air < 0.5, sim.crew.air < 0.8).max(from_alerts[7]),
            ),
        ];
        let dv_frac = (dv / dv_full).clamp(0.0, 1.0);
        let dose_frac = (rate / 0.1).clamp(0.0, 1.0);
        let g_frac = (gload / vector_sim::navigation::STRUCTURE_LIMIT_G).clamp(0.0, 1.0);
        let spd_surf = surf.as_ref().map(|s| s.v.length());
        let spd_orb = rel.length();
        let spd_mode = match g.cockpit.speed_mode.get() {
            m @ 1..=3 => usize::from(m - 1),
            _ => {
                if spd_surf.is_some() {
                    0
                } else {
                    1
                }
            }
        };
        let spd_now = match spd_mode {
            0 => spd_surf.unwrap_or(0.0),
            2 => spd_tgt.unwrap_or(0.0),
            _ => spd_orb,
        };
        let vv_now = landing.as_ref().map_or(0.0, |r| r.v_up);
        let targets_bars = [
            fuel,
            dv_frac,
            batt,
            hull,
            shield.unwrap_or(0.0),
            temp,
            dose_frac,
            g_frac,
        ];
        // Smoothing: the needles and bars follow the figures with a short lag.
        let (spd, vv_s, sm_ux, sm_uy, sm_sinp, sm_hdg, thr_vis, bars) = {
            let mut sm = g.cockpit.sm.borrow_mut();
            let dt = f64::from((t - sm.t).clamp(0.0, 0.1));
            let snap = !sm.init || t < sm.t || (t - sm.t) > 0.5;
            let k = |rate: f64| if snap { 1.0 } else { 1.0 - (-dt * rate).exp() };
            let (kn, kb) = (k(9.0), k(5.0));
            sm.spd += (spd_now - sm.spd) * kn;
            sm.vv += (vv_now - sm.vv) * kn;
            sm.ux += (ux - sm.ux) * kn as f32;
            sm.uy += (uy - sm.uy) * kn as f32;
            sm.sinp += (sinp - sm.sinp) * kn as f32;
            let d = (heading - sm.hdg + 540.0).rem_euclid(360.0) - 180.0;
            sm.hdg = (sm.hdg + d * kn as f32).rem_euclid(360.0);
            sm.thr += (sim.throttle - sm.thr) * k(14.0);
            for (b, tgt) in sm.bars.iter_mut().zip(targets_bars) {
                *b += (tgt - *b) * kb;
            }
            sm.init = true;
            sm.t = t;
            (
                sm.spd, sm.vv, sm.ux, sm.uy, sm.sinp, sm.hdg, sm.thr, sm.bars,
            )
        };
        Tel {
            t,
            blink: (t * 3.0) as i64 % 2 == 0,
            blink_slow: (t * 1.2) as i64 % 2 == 0,
            pulse: 0.5 + 0.5 * (t * 3.2).sin(),
            spd_orb,
            spd_surf,
            spd_tgt,
            spd,
            spd_mode,
            vv_s,
            ux: sm_ux,
            uy: sm_uy,
            sinp: sm_sinp,
            heading: sm_hdg,
            alt: landing.as_ref().map(|r| r.alt),
            vv: landing.as_ref().map(|r| r.v_up),
            vh: landing.as_ref().map(|r| r.v_h),
            fuel,
            dv,
            dv_frac,
            batt,
            temp_k: ship.sys.temp_k,
            shield,
            shield_on: sim.shield_on,
            shield_installed: installed,
            hull,
            dose_mgy: rate * 1e3,
            dose_frac,
            gload,
            g_frac,
            bars,
            thr: sim.throttle,
            thr_vis,
            trim: sim.trim,
            gear: sim.gear_down,
            sas: sim.sas,
            assist: ship.assist,
            auto: sim.autopilot.is_some(),
            radar_on: sim.active_sensors,
            decoys: sim.decoys.len(),
            lights: sim.lights_on,
            pilot: sim.pilot.map(|p| p.kind),
            landing,
            dock: sim.dock_readout(),
            lamps,
        }
    }
}

// ---- entry point --------------------------------------------------------------------------------

/// The arrangement the cockpit really uses in a frame of this size: the chosen one, or a simpler
/// one when the window is too small for it (None: the old HUD).
pub fn disposition_for(g: &Game, w: usize, h: usize) -> Option<Disposition> {
    let (w, h) = (w as i32, h as i32);
    let want = Disposition::from_index(g.settings.cockpit);
    let d = match want {
        Disposition::Old => return None,
        Disposition::A | Disposition::B if w < layouts::MIN_AB.0 || h < layouts::MIN_AB.1 => {
            Disposition::C
        }
        other => other,
    };
    if w < layouts::MIN_C.0 || h < layouts::MIN_C.1 {
        return None;
    }
    Some(d)
}

/// The first text column of the scene: clear of the struts of the canopy in arrangement A.
pub fn scene_text_col(g: &Game, width: usize) -> usize {
    if g.cockpit.last.get() == Some(Disposition::A) && !g.ext_active() {
        ((width / 9).min(150) / 8 + 1).min(width / 16)
    } else {
        1
    }
}

/// Draw the cockpit instead of the old HUD. False: the old HUD must be drawn (chosen, or the
/// window is too small).
pub fn draw(fb: &mut Framebuffer, v: &View) -> bool {
    let g = v.game;
    let Some(d) = disposition_for(g, fb.w, fb.h) else {
        g.cockpit.last.set(None);
        return false;
    };
    g.cockpit.last.set(Some(d));
    let tel = prof::time(0, || Tel::new(g));
    layouts::draw(fb, v, &tel, d);
    true
}

// ---- organs: zones, tooltips, menus -------------------------------------------------------------

impl Game {
    /// A zone that does nothing (a display organ: it only has a tooltip and a menu).
    pub fn cockpit_noop(&mut self, _: usize) {}
}

/// The organ whose zone is the topmost one under `pos`.
pub fn organ_under(g: &Game, pos: (i32, i32)) -> Option<Organ> {
    let hits = g.pointer.hits.borrow();
    let top = hits.iter().rposition(|h| {
        pos.0 >= h.rect.0 && pos.0 < h.rect.2 && pos.1 >= h.rect.1 && pos.1 < h.rect.3
    })?;
    g.cockpit
        .organs
        .borrow()
        .iter()
        .find(|o| o.hit == top)
        .copied()
}

/// The state of an organ in words, for its tooltip.
fn state_of(g: &Game, id: Id, n: u8) -> String {
    let s = &g.sim;
    let onoff = |b: bool| if b { "ACTIF" } else { "COUPE" };
    match id {
        Id::Gear => if s.gear_down { "SORTI" } else { "RENTRE" }.into(),
        Id::Shield => {
            if !s.shield_installed() {
                "AUCUN GENERATEUR".into()
            } else if s.shield_on {
                format!(
                    "EN MARCHE  {:.0} %",
                    100.0 * s.ship.sys.shield_pts / s.effective.shield_cap.max(1.0)
                )
            } else {
                "ARRETE : NE PROTEGE PLUS".into()
            }
        }
        Id::Radar => onoff(s.active_sensors).into(),
        Id::Decoy => format!("{} EN VOL", s.decoys.len()),
        Id::Lights => if s.lights_on { "ALLUMES" } else { "ETEINTS" }.into(),
        Id::Auto => {
            if s.pilot.is_some() {
                "PILOTE ENGAGE".into()
            } else {
                onoff(s.autopilot.is_some()).into()
            }
        }
        Id::Sas => format!("SAS : {}", s.sas.label()),
        Id::Lever => format!("{:.0} %", s.throttle * 100.0),
        Id::Trim => format!(
            "{:+} CRAN(S)  POUSSEE REELLE {:.0} %",
            s.trim,
            s.effective_throttle() * 100.0
        ),
        Id::Lamp => {
            let t = Tel::new(g);
            let (name, lv) = t.lamps[usize::from(n).min(7)];
            format!(
                "{name} : {}",
                match lv {
                    2 => "CRITIQUE",
                    1 => "ATTENTION",
                    _ => "NORMAL",
                }
            )
        }
        Id::Layout => Disposition::from_index(g.settings.cockpit).label().into(),
        Id::Handle => {
            if g.settings.cockpit_open {
                "DEPLOYEE".into()
            } else {
                "REPLIEE".into()
            }
        }
        _ => String::new(),
    }
}

/// The lines of the tooltip of an organ: name, state, key, how to ask for more.
pub fn tip_lines(g: &Game, o: Organ) -> Vec<(String, u32)> {
    use crate::hud::{AMBER, DIM, GREEN};
    let (name, key, _) = spec(o.id, o.n);
    let mut out = vec![(name.to_string(), AMBER)];
    let st = state_of(g, o.id, o.n);
    if !st.is_empty() {
        out.push((st, GREEN));
    }
    if key != "-" {
        out.push((format!("TOUCHE : {key}"), GREEN));
    }
    out.push(("CLIC DROIT : ACTIONS ET AIDE".into(), DIM));
    out
}

/// The menu of an organ (right click): what it does, then "CIBLER ICI" where it makes sense, the
/// layout and the help on the organ.
pub fn menu_for(g: &Game, o: Organ, at: (i32, i32)) -> (String, Vec<Item>) {
    let (name, key, _) = spec(o.id, o.n);
    let mut it: Vec<Item> = Vec::new();
    let k = |c: KeyCode| Act::Key(c);
    let key_item = |label: String, k_: &str, c: KeyCode| Item::new(label, k_, vec![Act::Key(c)]);
    match o.id {
        Id::Gear => it.push(key_item(
            if g.sim.gear_down {
                "RENTRER LE TRAIN".into()
            } else {
                "SORTIR LE TRAIN".into()
            },
            key,
            KeyCode::Numpad3,
        )),
        Id::Shield => it.push(
            key_item(
                if g.sim.shield_on {
                    "ARRETER LE BOUCLIER".into()
                } else {
                    "REMETTRE LE BOUCLIER".into()
                },
                key,
                KeyCode::NumpadDivide,
            )
            .off_if(!g.sim.shield_installed(), "AUCUN GENERATEUR INSTALLE"),
        ),
        Id::Radar => it.push(key_item(
            if g.sim.active_sensors {
                "COUPER LE RADAR".into()
            } else {
                "ALLUMER LE RADAR".into()
            },
            key,
            KeyCode::Period,
        )),
        Id::Decoy => it.push(key_item("LANCER UN LEURRE".into(), key, KeyCode::Comma)),
        Id::Lights => it.push(key_item(
            if g.sim.lights_on {
                "ETEINDRE LES PROJECTEURS".into()
            } else {
                "ALLUMER LES PROJECTEURS".into()
            },
            key,
            KeyCode::NumpadDecimal,
        )),
        Id::Auto => it.push(key_item(
            if g.sim.autopilot.is_some() || g.sim.pilot.is_some() {
                "COUPER LE PILOTE AUTO".into()
            } else {
                "ENGAGER LE PILOTE AUTO".into()
            },
            key,
            KeyCode::KeyG,
        )),
        Id::Sas => {
            for (l, c, kk) in [
                ("COUPER LE SAS", KeyCode::Digit0, "0"),
                ("MAINTIEN DU CAP", KeyCode::Digit1, "1"),
                ("PROGRADE", KeyCode::Digit2, "2"),
                ("RETROGRADE", KeyCode::Digit3, "3"),
                ("VERS LA CIBLE", KeyCode::Digit8, "8"),
            ] {
                it.push(key_item(l.into(), kk, c));
            }
        }
        Id::Lever => {
            it.push(key_item(
                "ACCELERATEUR +10 %".into(),
                "PAV8",
                KeyCode::Numpad8,
            ));
            it.push(key_item(
                "ACCELERATEUR -10 %".into(),
                "PAV2",
                KeyCode::Numpad2,
            ));
            it.push(key_item("COUPURE".into(), "PAV0", KeyCode::Numpad0));
            it.push(key_item("PLEIN GAZ".into(), "PAV5", KeyCode::Numpad5));
        }
        Id::Trim => {
            it.push(key_item("TRIM +1".into(), "]", KeyCode::BracketRight));
            it.push(key_item("TRIM -1".into(), "[", KeyCode::BracketLeft));
            it.push(Item::new(
                "REMETTRE LE TRIM A ZERO",
                "-",
                vec![Act::Call(Game::cockpit_trim_reset, 0)],
            ));
        }
        Id::SpeedDial | Id::SpeedSel => {
            for (l, n_) in [
                ("VITESSE SOL", 0usize),
                ("VITESSE ORBITALE", 1),
                ("VITESSE / CIBLE", 2),
            ] {
                it.push(Item::new(
                    l,
                    "-",
                    vec![Act::Call(Game::cockpit_speed_mode, n_)],
                ));
            }
        }
        Id::Scope | Id::Blip => {
            it.push(Item::new(
                "CIBLER ICI",
                "CLIC",
                vec![Act::Call(Game::cockpit_target_here, pack(at))],
            ));
            it.push(key_item("CIBLE SUIVANTE".into(), "TAB", KeyCode::Tab));
            it.push(key_item(
                if g.sim.active_sensors {
                    "COUPER LE RADAR".into()
                } else {
                    "ALLUMER LE RADAR".into()
                },
                ".",
                KeyCode::Period,
            ));
        }
        Id::PlanScreen | Id::PlanBtn => {
            it.push(Item::new(
                if g.sim.nav_running() {
                    "METTRE LE PLAN EN PAUSE"
                } else {
                    "LANCER LE PLAN"
                },
                "-",
                vec![Act::Call(Game::nav_click_cockpit, 0)],
            ));
            it.push(Item::new(
                "ECRAN DE NAVIGATION AUTO",
                "PAV*",
                vec![k(KeyCode::NumpadMultiply)],
            ));
        }
        Id::NavScreen => it.push(Item::new("CARTE DU SYSTEME", "M", vec![k(KeyCode::KeyM)])),
        Id::Target | Id::TargetBtn => {
            it.push(key_item("CIBLE SUIVANTE".into(), "TAB", KeyCode::Tab));
            it.push(key_item("APPELER (RADIO)".into(), "ENTR", KeyCode::Enter));
            it.push(key_item("AMARRER".into(), "D", KeyCode::KeyD));
        }
        Id::PilotScreen | Id::PilotBtn | Id::DescentScreen | Id::DockScreen => {
            it.push(key_item(
                "PILOTE : ENGAGER / DEBRAYER".into(),
                "PAV4",
                KeyCode::Numpad4,
            ));
            it.push(key_item(
                "MARQUER LE SITE PLAT".into(),
                "PAV7",
                KeyCode::Numpad7,
            ));
            it.push(key_item(
                "VUE ARRIERE / AVANT".into(),
                "PAV1",
                KeyCode::Numpad1,
            ));
        }
        Id::Handle => it.push(key_item(
            if g.settings.cockpit_open {
                "REPLIER LA CONSOLE".into()
            } else {
                "DEPLOYER LA CONSOLE".into()
            },
            key,
            KeyCode::Quote,
        )),
        Id::Lamp => it.push(Item::new(
            "TABLEAU DE BORD (ALERTES)",
            "\\",
            vec![k(KeyCode::Backslash)],
        )),
        _ => {}
    }
    // Where there is a world to aim at, "CIBLER ICI" is on every instrument of the glass window.
    it.push(Item::new(
        format!("DISPOSITION : {}", g.settings.cockpit_label_next()),
        "MAJ+F11",
        vec![Act::Call(Game::cockpit_cycle, 0)],
    ));
    it.push(Item::new(
        "AIDE SUR CET ORGANE",
        "-",
        vec![Act::Call(Game::cockpit_help, encode(o.id, o.n))],
    ));
    it.push(Item::new(
        "MENU : TOUS LES ECRANS",
        "F10",
        vec![k(KeyCode::F10)],
    ));
    (name.to_string(), it)
}

pub fn pack((x, y): (i32, i32)) -> usize {
    ((x.clamp(0, 0xFFFF) as usize) << 16) | y.clamp(0, 0xFFFF) as usize
}

fn unpack(n: usize) -> (i32, i32) {
    (((n >> 16) & 0xFFFF) as i32, (n & 0xFFFF) as i32)
}

const IDS: [Id; 31] = [
    Id::Gear,
    Id::Shield,
    Id::Radar,
    Id::Decoy,
    Id::Lights,
    Id::Auto,
    Id::Sas,
    Id::Lever,
    Id::Trim,
    Id::SpeedDial,
    Id::SpeedSel,
    Id::Attitude,
    Id::Tape,
    Id::Vario,
    Id::Digital,
    Id::Systems,
    Id::Lamp,
    Id::Scope,
    Id::Blip,
    Id::NavScreen,
    Id::DescentScreen,
    Id::DockScreen,
    Id::PlanScreen,
    Id::PlanBtn,
    Id::PilotScreen,
    Id::PilotBtn,
    Id::Target,
    Id::TargetBtn,
    Id::FKey,
    Id::Handle,
    Id::Layout,
];

fn encode(id: Id, n: u8) -> usize {
    let i = IDS.iter().position(|x| *x == id).unwrap_or(0);
    i * 256 + usize::from(n)
}

fn decode(code: usize) -> (Id, u8) {
    (IDS[(code / 256).min(IDS.len() - 1)], (code % 256) as u8)
}

// ---- actions of the game ------------------------------------------------------------------------

impl Game {
    fn save_settings_file(&self) {
        let path = if cfg!(test) {
            std::env::temp_dir().join("vector_test_settings.cfg")
        } else {
            std::path::PathBuf::from("saves").join("settings.cfg")
        };
        self.settings.save(&path);
    }

    /// Next arrangement of the cockpit (MAJ+F11, the option, the menu), saved with the options.
    pub fn cockpit_cycle(&mut self, _: usize) {
        let d = Disposition::from_index(self.settings.cockpit).next();
        self.settings.cockpit = d.index();
        self.save_settings_file();
        self.say(format!("COCKPIT : {}", d.label()));
    }

    /// Fold or unfold the console of arrangement C (remembered in the options).
    pub fn cockpit_fold(&mut self, _: usize) {
        self.settings.cockpit_open = !self.settings.cockpit_open;
        self.save_settings_file();
        self.say(if self.settings.cockpit_open {
            "CONSOLE DEPLOYEE"
        } else {
            "CONSOLE REPLIEE"
        });
    }

    /// What the speed dial measures: 0 ground, 1 orbit, 2 target.
    pub fn cockpit_speed_mode(&mut self, n: usize) {
        self.cockpit.speed_mode.set(n.min(2) as u8 + 1);
        self.say(match n {
            0 => "CADRAN : VITESSE SOL",
            1 => "CADRAN : VITESSE ORBITALE",
            _ => "CADRAN : VITESSE RELATIVE A LA CIBLE",
        });
    }

    pub fn cockpit_trim_reset(&mut self, _: usize) {
        self.sim.trim_reset();
        self.say("TRIM : ZERO");
    }

    /// Open the help of an organ in the glass window.
    pub fn cockpit_help(&mut self, code: usize) {
        self.cockpit.help.set(Some(decode(code)));
    }

    pub fn cockpit_help_close(&mut self, _: usize) {
        self.cockpit.help.set(None);
    }

    /// Target what is nearest to where the menu was opened: a blip of the radar or a body or ship
    /// of the window (the very zones the left click uses).
    pub fn cockpit_target_here(&mut self, packed: usize) {
        let (x, y) = unpack(packed);
        let best = self
            .pointer
            .hits
            .borrow()
            .iter()
            .filter(|h| {
                matches!(h.act, Act::Call(f, _) if std::ptr::fn_addr_eq(f, Game::click_body as fn(&mut Game, usize))
                    || std::ptr::fn_addr_eq(f, Game::click_npc as fn(&mut Game, usize)))
            })
            .map(|h| {
                let (cx, cy) = ((h.rect.0 + h.rect.2) / 2, (h.rect.1 + h.rect.3) / 2);
                (i64::from((cx - x).pow(2) + (cy - y).pow(2)), h.act)
            })
            .min_by_key(|c| c.0)
            .filter(|c| c.0 < 160 * 160)
            .map(|c| c.1);
        match best {
            Some(a) => self.run_act(a),
            None => self.say("RIEN A CIBLER ICI"),
        }
    }

    /// Click or drag on the thrust lever: the throttle follows the pointer.
    pub fn cockpit_lever_at(&mut self, (_, y): (i32, i32)) {
        let l = self.cockpit.lever.get();
        if l.bottom <= l.top {
            return;
        }
        self.cockpit.drag.set(true);
        self.cockpit_lever_y(y);
    }

    fn cockpit_lever_y(&mut self, y: i32) {
        let l = self.cockpit.lever.get();
        let f = f64::from(l.bottom - y) / f64::from((l.bottom - l.top).max(1));
        self.sim
            .set_throttle((f.clamp(0.0, 1.0) * 100.0).round() / 100.0);
    }

    /// The pointer moved with the left button down.
    pub fn cockpit_drag(&mut self, y: i32) {
        if self.cockpit.drag.get() {
            self.cockpit_lever_y(y);
        }
    }

    /// The wheel over the lever or the trim. True when it was used.
    pub fn cockpit_wheel(&mut self, lines: f64) -> bool {
        let Some(o) = organ_under(self, self.pointer.pos) else {
            return false;
        };
        match o.id {
            Id::Lever => {
                let t = self.sim.throttle + if lines > 0.0 { 0.05 } else { -0.05 };
                self.sim.set_throttle((t * 20.0).round() / 20.0);
                let pct = (self.sim.throttle * 100.0).round();
                self.say(format!("ACCELERATEUR {pct:.0} %"));
                true
            }
            Id::Trim => {
                self.trim_key(if lines > 0.0 { 1 } else { -1 });
                true
            }
            _ => false,
        }
    }

    pub fn shield_key(&mut self) {
        match self.sim.toggle_shield() {
            Ok(true) => self.say("BOUCLIER EN MARCHE : IL SE RECHARGE"),
            Ok(false) => self.say("BOUCLIER ARRETE : AUCUNE PROTECTION, AUCUNE CONSOMMATION"),
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    pub fn lights_key(&mut self) {
        let on = self.sim.toggle_lights();
        self.say(if on {
            "PROJECTEURS ALLUMES : ON VOUS VOIT DE LOIN"
        } else {
            "PROJECTEURS ETEINTS"
        });
    }

    pub fn trim_key(&mut self, delta: i8) {
        let t = self.sim.trim_step(delta);
        self.say(format!(
            "TRIM {t:+} : POUSSEE {:.0} %",
            self.sim.effective_throttle() * 100.0
        ));
    }

    /// The keys of the cockpit; true when the key was one of them.
    pub fn cockpit_key(&mut self, code: KeyCode) -> bool {
        self.cockpit.help.set(None);
        match code {
            KeyCode::NumpadDivide => self.shield_key(),
            KeyCode::NumpadDecimal => self.lights_key(),
            KeyCode::BracketRight => self.trim_key(1),
            KeyCode::BracketLeft => self.trim_key(-1),
            KeyCode::Quote => {
                if self.settings.cockpit == Disposition::C.index() {
                    self.cockpit_fold(0);
                } else {
                    self.say("LA CONSOLE SE REPLIE EN DISPOSITION C (MAJ+F11 POUR CHANGER)");
                }
            }
            KeyCode::F11
                if self.keys.contains(&KeyCode::ShiftLeft)
                    || self.keys.contains(&KeyCode::ShiftRight) =>
            {
                self.cockpit_cycle(0)
            }
            _ => return false,
        }
        true
    }
}

impl crate::settings::Settings {
    /// The label of the arrangement after the current one (for the menu line).
    pub fn cockpit_label_next(&self) -> &'static str {
        Disposition::from_index(self.cockpit).next().short()
    }
}
