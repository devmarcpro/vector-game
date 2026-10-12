//! Navigation automation: a queue of orders the ship's computer carries out by itself. It is the
//! one place where the manoeuvre nodes, the orbit changes, the transfers between worlds, the
//! station-keeping, the escort, the patrol, the landing and docking pilots, the autopilot and the
//! FTL jumps are chained. Each order is a small state machine; a macro (a transfer, an orbit
//! change) replaces itself by the lower-level steps when its turn comes, from the state the ship
//! is then really in, so the queue always shows what the computer is about to do.
//!
//! The computer never flies blind: every step it watches for a collision on the predicted path,
//! for a gravity well it did not plan to enter, for fire, for heat and radiation, and hands the
//! controls back with a message at the first danger. Thresholds of fuel, hull and radiation can
//! send the ship home to the nearest station instead. The player can take the controls at any
//! time (a touch of the stick pauses the plan).
//!
//! Burns are real: the ship turns to the burn direction with its attitude control, lights the
//! engines at the time the rocket equation gives (half the burn before the node), and cuts them
//! when the delta-v measured on the velocity reaches the one asked for, spending real fuel.
use crate::nav_predict::{ManeuverNode, NodeWhen, Resolved};
use crate::{Event, ShipInput, Sim};
use serde::{Deserialize, Serialize};
use vector_core::{Mat3, Vec3};
use vector_world::galaxy::StarRef;
use vector_world::BodyKind;

/// Largest number of entries of a queue.
pub const MAX_ORDERS: usize = 48;
/// Angle (rad) under which the nose counts as pointing at the burn.
pub const ALIGN_TOL: f64 = 0.02;
/// How often the safety monitor predicts the path (s of game time).
pub const SAFETY_PERIOD: f64 = 120.0;
/// Seconds of warning of an impact under which the computer hands the ship back.
pub const IMPACT_WARN: f64 = 600.0;

/// A condition of the ship, to guard an order or to wait for.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Cond {
    /// Share of the tank (0..1).
    FuelBelow(f64),
    FuelAbove(f64),
    /// Share of the hull (0..1).
    HullBelow(f64),
    HullAbove(f64),
    /// Dose rate aboard (Gy/h).
    DoseAbove(f64),
    Docked,
    Flying,
}

impl Cond {
    pub fn label(self) -> String {
        match self {
            Cond::FuelBelow(x) => format!("CARBURANT < {:.0} %", x * 100.0),
            Cond::FuelAbove(x) => format!("CARBURANT > {:.0} %", x * 100.0),
            Cond::HullBelow(x) => format!("COQUE < {:.0} %", x * 100.0),
            Cond::HullAbove(x) => format!("COQUE > {:.0} %", x * 100.0),
            Cond::DoseAbove(x) => format!("DOSE > {x:.3} GY/H"),
            Cond::Docked => "AMARRE".into(),
            Cond::Flying => "EN VOL".into(),
        }
    }
}

/// One order of the queue.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Order {
    // ---- steps
    /// A manoeuvre node.
    Burn(ManeuverNode),
    /// Do nothing for this many seconds.
    Wait(f64),
    /// Do nothing until the condition is true.
    WaitCond(Cond),
    /// Do nothing until this date (s since the epoch).
    WaitUntil(f64),
    /// Coast until the ship enters the sphere of influence of `body` (gives up after `max_s`).
    Coast {
        body: usize,
        max_s: f64,
    },
    /// Fine-tune the path so that it passes `alt` metres above `target`, which it reaches at
    /// `t_arr` (s since the epoch, 0 when unknown).
    Correct {
        target: usize,
        alt: f64,
        t_arr: f64,
    },
    /// Suspend the plan here.
    Halt,
    Undock,
    Dock,
    Land,
    Takeoff,
    /// Fly to a body (or station) with the assisted autopilot and stop near it.
    Approach {
        body: usize,
    },
    /// Cross the atmosphere of `body` on the path, braked by the air, and leave it.
    Aerobrake {
        body: usize,
    },
    /// An FTL jump to a star.
    Jump {
        star: StarRef,
    },
    // ---- macros
    /// Orbit `body` with the given periapsis and apoapsis altitudes (m). `stage` counts the
    /// corrections made so far.
    Orbit {
        body: usize,
        peri_alt: f64,
        apo_alt: f64,
        stage: u8,
    },
    /// Go from here to a circular orbit of `alt` metres around `target` (with the air as a brake
    /// when `aero`).
    Transfer {
        target: usize,
        alt: f64,
        aero: bool,
    },
    /// The departure of a transfer whose window is now: planned from the state the ship is in, for a
    /// burn within the day (a Transfer far from its window waits for it and then becomes this).
    Depart {
        target: usize,
        alt: f64,
        aero: bool,
    },
    /// Hold an orbit: correct it when it drifts by more than `tol` (share of the radius); for
    /// `duration` seconds, or forever when 0.
    KeepOrbit {
        body: usize,
        peri_alt: f64,
        apo_alt: f64,
        tol: f64,
        duration: f64,
    },
    /// Follow another ship at `dist` metres, matching its velocity (0 duration: until stopped).
    Escort {
        npc: u32,
        dist: f64,
        duration: f64,
    },
    /// Visit the waypoints in turn, `laps` times (0: forever).
    Patrol {
        waypoints: Vec<usize>,
        laps: u16,
    },
    /// Go back to the nearest station and dock.
    ReturnStation,
    /// Jump along the safest route to a star, a hop at a time.
    Route {
        dest: StarRef,
    },
}

impl Order {
    /// The body an order is about, if any (the plan is tied to one system).
    pub fn bodies(&self) -> Vec<usize> {
        match self {
            Order::Burn(n) => n.body.into_iter().collect(),
            Order::Coast { body, .. }
            | Order::Approach { body }
            | Order::Aerobrake { body }
            | Order::Orbit { body, .. }
            | Order::KeepOrbit { body, .. } => vec![*body],
            Order::Correct { target, .. }
            | Order::Transfer { target, .. }
            | Order::Depart { target, .. } => vec![*target],
            Order::Patrol { waypoints, .. } => waypoints.clone(),
            _ => Vec::new(),
        }
    }

    /// Whether the order is a macro that replaces itself by other orders.
    pub fn is_macro(&self) -> bool {
        matches!(
            self,
            Order::Orbit { .. }
                | Order::Transfer { .. }
                | Order::Depart { .. }
                | Order::Patrol { .. }
                | Order::ReturnStation
                | Order::Route { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Pending,
    Done,
    Skipped,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub order: Order,
    /// The order is carried out only if this is true when its turn comes.
    pub guard: Option<Cond>,
    pub status: Status,
    /// The system the order was written for (body numbers mean nothing in another one).
    pub star: Option<StarRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Run {
    Idle,
    Running,
    Paused,
    /// Stopped by the computer, with the reason; the plan can be resumed.
    Halted(String),
}

/// The thresholds that make the computer stop or send the ship home.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    /// Share of the tank, share of the hull and dose rate (Gy/h) that send the ship back.
    pub fuel_return: f64,
    pub hull_return: f64,
    pub dose_return: f64,
    /// Go back to the nearest station on a threshold (else only stop and warn).
    pub emergency: bool,
    /// Share of the tank a plan must leave (the planner refuses a plan that eats it).
    pub reserve: f64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            fuel_return: 0.15,
            hull_return: 0.30,
            dose_return: 0.20,
            emergency: true,
            reserve: 0.05,
        }
    }
}

/// What the computer is doing about the present order (not saved: a loaded plan restarts it).
#[derive(Clone, Debug, Default)]
pub struct Active {
    pub t0: f64,
    pub phase: Phase,
    pub resolved: Option<Resolved>,
    pub next_resolve: f64,
    pub next_safety: f64,
    pub dir: Vec3,
    pub dv_goal: f64,
    pub dv_done: f64,
    pub vel_before: Vec3,
    pub grav_before: Vec3,
    /// Bodies whose sphere the ship may enter without it being a surprise.
    pub expected: Vec<usize>,
    pub owns_ap: bool,
    pub owns_pilot: bool,
    pub owns_jump: bool,
    /// The ship may fall into the atmosphere of this body (an aerobraking pass).
    pub dive_ok: Option<usize>,
    pub hull0: f64,
    pub entered: bool,
    /// Waypoints of the approach to a port (relative to the station), the next one, the station.
    pub way: Vec<Vec3>,
    pub way_i: usize,
    pub station: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Resolve,
    Coast,
    Align,
    Burn,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NavPlan {
    pub orders: Vec<Entry>,
    pub run: Run,
    pub limits: Limits,
    /// Last messages of the computer (most recent last).
    pub log: Vec<String>,
    /// The emergency return is under way.
    pub emergency: bool,
    #[serde(skip)]
    pub active: Option<Active>,
}

impl Default for NavPlan {
    fn default() -> Self {
        NavPlan {
            orders: Vec::new(),
            run: Run::Idle,
            limits: Limits::default(),
            log: Vec::new(),
            emergency: false,
            active: None,
        }
    }
}

impl NavPlan {
    /// Index of the first entry still to do.
    pub fn cursor(&self) -> usize {
        self.orders
            .iter()
            .position(|e| e.status == Status::Pending)
            .unwrap_or(self.orders.len())
    }

    pub fn pending(&self) -> usize {
        self.orders
            .iter()
            .filter(|e| e.status == Status::Pending)
            .count()
    }

    fn say(&mut self, ev: &mut Vec<Event>, msg: impl Into<String>) {
        let msg = msg.into();
        self.log.push(msg.clone());
        if self.log.len() > 12 {
            self.log.remove(0);
        }
        ev.push(Event::Nav(msg));
    }
}

/// Next thing the order loop must do.
enum Step {
    Go(ShipInput),
    Done,
    Fail(String),
    /// The order replaced itself: look at the queue again.
    Again,
}

/// A duration as short text (S, MIN, H, J): the planner's and the screens' one format.
pub fn fmt_eta(s: f64) -> String {
    if s < 120.0 {
        format!("{s:.0} S")
    } else if s < 7200.0 {
        format!("{:.0} MIN", s / 60.0)
    } else if s < 172_800.0 {
        format!("{:.1} H", s / 3600.0)
    } else {
        format!("{:.1} J", s / 86_400.0)
    }
}

impl Sim {
    // ---- the queue ---------------------------------------------------------------------------

    /// Add an order at the end of the queue.
    pub fn nav_add(&mut self, order: Order) -> Result<(), &'static str> {
        self.nav_add_guarded(order, None)
    }

    pub fn nav_add_guarded(
        &mut self,
        order: Order,
        guard: Option<Cond>,
    ) -> Result<(), &'static str> {
        if self.nav.orders.len() >= MAX_ORDERS {
            return Err("file d'ordres pleine");
        }
        if let Order::Burn(n) = &order {
            if n.dv() <= 0.0 {
                return Err("manoeuvre nulle");
            }
        }
        if order.bodies().iter().any(|&b| b >= self.bodies.len()) {
            return Err("corps inconnu");
        }
        self.nav.orders.push(Entry {
            order,
            guard,
            status: Status::Pending,
            star: Some(self.here),
        });
        Ok(())
    }

    pub fn nav_remove(&mut self, i: usize) {
        if i < self.nav.orders.len() {
            let was_active = i == self.nav.cursor() && self.nav.active.is_some();
            if was_active {
                self.nav_abort_active();
            }
            self.nav.orders.remove(i);
        }
    }

    /// Move an entry one place up (`up`) or down; only entries still to do move.
    pub fn nav_move(&mut self, i: usize, up: bool) {
        let n = self.nav.orders.len();
        let j = if up { i.checked_sub(1) } else { Some(i + 1) };
        let Some(j) = j.filter(|&j| j < n && i < n) else {
            return;
        };
        if self.nav.orders[i].status != Status::Pending
            || self.nav.orders[j].status != Status::Pending
        {
            return;
        }
        if self.nav.active.is_some() && (i == self.nav.cursor() || j == self.nav.cursor()) {
            self.nav_abort_active();
        }
        self.nav.orders.swap(i, j);
    }

    pub fn nav_set_guard(&mut self, i: usize, guard: Option<Cond>) {
        if let Some(e) = self.nav.orders.get_mut(i) {
            e.guard = guard;
        }
    }

    /// Forget what is done (and what failed or was skipped).
    pub fn nav_clear_done(&mut self) {
        self.nav.orders.retain(|e| e.status == Status::Pending);
    }

    pub fn nav_clear(&mut self) {
        self.nav_abort_active();
        self.nav.orders.clear();
        self.nav.run = Run::Idle;
        self.nav.emergency = false;
    }

    /// Start (or resume) the plan: orders failed or skipped before are not retried, the rest runs.
    pub fn nav_start(&mut self) -> Result<(), &'static str> {
        let retry = matches!(self.nav.run, Run::Halted(_))
            && self.nav.orders.iter().any(|e| e.status == Status::Failed);
        if self.nav.pending() == 0 && !retry {
            return Err("file d'ordres vide");
        }
        if self.nav.run == Run::Running {
            return Ok(());
        }
        // After an incident the order that failed is tried again (the reason may be gone: fuel
        // taken, a hostile ship away...).
        if matches!(self.nav.run, Run::Halted(_)) {
            if let Some(e) = self
                .nav
                .orders
                .iter_mut()
                .rev()
                .find(|e| e.status == Status::Failed)
            {
                e.status = Status::Pending;
            }
        }
        self.nav.emergency = false;
        self.nav.run = Run::Running;
        let mut ev = Vec::new();
        self.nav.say(&mut ev, "NAVIGATION AUTO : PLAN ENGAGE");
        self.events.extend(ev);
        Ok(())
    }

    pub fn nav_pause(&mut self) {
        if self.nav.run == Run::Running {
            self.nav_abort_active();
            self.nav.run = Run::Paused;
            let mut ev = Vec::new();
            self.nav.say(&mut ev, "NAVIGATION AUTO : PLAN EN PAUSE");
            self.events.extend(ev);
        }
    }

    /// Stop the plan for a reason; the queue is kept.
    pub fn nav_halt(&mut self, why: impl Into<String>) {
        let why = why.into();
        self.nav_abort_active();
        let mut ev = Vec::new();
        self.nav.say(&mut ev, format!("NAVIGATION AUTO : {why}"));
        self.events.extend(ev);
        self.nav.run = Run::Halted(why);
    }

    /// Give up the order in hand: whatever it flew (autopilot, pilot) is let go.
    pub fn nav_abort_active(&mut self) {
        let mut nav = std::mem::take(&mut self.nav);
        self.nav_release(&mut nav);
        self.nav = nav;
    }

    /// Let go of the order in hand: the pilots it started are stopped, and a burn in progress
    /// keeps what it has already given, so that the plan can resume it.
    fn nav_release(&mut self, nav: &mut NavPlan) {
        let cur = nav.cursor();
        if let Some(a) = nav.active.take() {
            if a.phase == Phase::Burn {
                if let Some(Entry {
                    order: Order::Burn(n),
                    ..
                }) = nav.orders.get_mut(cur)
                {
                    n.done += a.dv_done.max(0.0);
                }
            }
            if a.owns_ap {
                self.autopilot = None;
            }
            if a.owns_pilot {
                self.pilot = None;
            }
            if a.owns_jump {
                self.cancel_jump();
                self.jump_run = None;
            }
        }
    }

    /// Put the return to the nearest station in front of the queue and start it (the button of
    /// the thresholds page).
    pub fn nav_emergency_now(&mut self) {
        if self.docked.is_some() {
            return;
        }
        let mut nav = std::mem::take(&mut self.nav);
        let mut ev = Vec::new();
        self.nav_emergency(&mut nav, "RETOUR DEMANDE", &mut ev);
        nav.run = Run::Running;
        self.nav = nav;
        self.events.extend(ev);
    }

    pub fn nav_running(&self) -> bool {
        self.nav.run == Run::Running
    }

    /// The nodes of the queue still to burn (those that can be previewed in the cockpit).
    pub fn nav_nodes(&self) -> Vec<ManeuverNode> {
        self.nav
            .orders
            .iter()
            .filter(|e| e.status == Status::Pending)
            .filter_map(|e| match &e.order {
                Order::Burn(n) => Some(*n),
                _ => None,
            })
            .collect()
    }

    /// Evaluate a condition.
    pub fn nav_cond(&self, c: Cond) -> bool {
        let fuel = self.ship.fuel / self.ship.stats.fuel_cap.max(1.0);
        let hull = self.ship.hull / self.ship.stats.hull_max.max(1.0);
        match c {
            Cond::FuelBelow(x) => fuel < x,
            Cond::FuelAbove(x) => fuel > x,
            Cond::HullBelow(x) => hull < x,
            Cond::HullAbove(x) => hull > x,
            Cond::DoseAbove(x) => self.dose_rate_aboard() > x,
            Cond::Docked => self.docked.is_some(),
            Cond::Flying => self.docked.is_none(),
        }
    }

    /// The bodies whose sphere may be entered without surprise: the one the ship is in with its
    /// ancestors, and the bodies the order is about with theirs.
    fn nav_expected(&self, order: &Order, rest: &[Entry]) -> Vec<usize> {
        let mut out = vec![0usize];
        let add = |i: usize, out: &mut Vec<usize>| {
            let mut k = Some(i);
            while let Some(b) = k {
                if !out.contains(&b) {
                    out.push(b);
                }
                k = self
                    .system
                    .bodies
                    .get(b)
                    .and_then(|x| x.orbit.as_ref())
                    .map(|o| o.parent);
            }
        };
        if let Some(r) = self.ref_body {
            add(r, &mut out);
        }
        for b in order
            .bodies()
            .into_iter()
            .chain(rest.iter().flat_map(|e| e.order.bodies()))
        {
            add(b, &mut out);
        }
        if matches!(order, Order::Dock | Order::Land | Order::Takeoff) {
            if let Some((s, _)) = self.nearest_station() {
                add(s, &mut out);
            }
        }
        out
    }

    // ---- the layers of the step -------------------------------------------------------------

    /// Between the pilots and the physics: the queue flies the ship while it runs.
    pub(crate) fn nav_layer(&mut self, manual: &ShipInput, input: ShipInput, dt: f64) -> ShipInput {
        if self.nav.run != Run::Running {
            return input;
        }
        let mut nav = std::mem::take(&mut self.nav);
        let out = self.nav_step(&mut nav, manual, input, dt);
        let burning = matches!(&nav.active, Some(a) if a.phase == Phase::Burn);
        if burning {
            if let Some(a) = nav.active.as_mut() {
                a.vel_before = self.ship.vel;
                a.grav_before = self.gravity_at(self.ship.pos);
            }
        }
        self.nav = nav;
        out
    }

    /// While docked the queue can still wait, undock or stop.
    pub(crate) fn nav_docked_tick(&mut self) {
        if self.nav.run != Run::Running {
            return;
        }
        let mut nav = std::mem::take(&mut self.nav);
        self.nav_step(&mut nav, &ShipInput::default(), ShipInput::default(), 1.0);
        self.nav = nav;
    }

    /// After the physics of a step: the delta-v the engines really gave during a burn.
    pub(crate) fn nav_post_step(&mut self, dt: f64) {
        let Some(a) = self.nav.active.as_mut() else {
            return;
        };
        if a.phase != Phase::Burn {
            return;
        }
        let gain = (self.ship.vel - a.vel_before) - a.grav_before * dt;
        a.dv_done += gain.dot(a.dir);
    }

    /// The step the queue needs to keep its appointments (None: no constraint). Burns and
    /// alignments are flown in small steps; a coast stops at the time the ship must start turning.
    pub fn nav_dt_cap(&self) -> Option<f64> {
        if self.nav.run != Run::Running {
            return None;
        }
        let a = self.nav.active.as_ref()?;
        let e = self
            .nav
            .orders
            .iter()
            .find(|e| e.status == Status::Pending)?;
        let now = self.time;
        let own = match &e.order {
            Order::Burn(n) => match a.phase {
                Phase::Burn => {
                    let (full, _) = self.burn_time(n.remaining());
                    Some((full / 120.0).clamp(0.05, 2.0))
                }
                Phase::Align => Some(0.2),
                Phase::Resolve => Some(1.0),
                Phase::Coast => a.resolved.map(|r| {
                    let (_, half) = self.burn_time(n.remaining());
                    let lead = self.nav_align_lead();
                    (r.t - half - lead - now)
                        .min(a.next_resolve - now)
                        .max(0.05)
                }),
            },
            Order::Wait(s) => Some((a.t0 + s - now).max(0.05)),
            Order::WaitUntil(t) => Some((t - now).max(0.05)),
            Order::Escort { .. } => Some(0.2),
            Order::KeepOrbit { .. } => Some(60.0),
            Order::Dock if a.way_i < a.way.len() => Some(0.2),
            _ => None,
        };
        // Flying by itself the ship follows its orbits as exactly as the prediction does.
        let own_flight = !(a.owns_ap || a.owns_pilot || a.owns_jump);
        let orbit = own_flight.then(|| self.nav_orbit_dt());
        match (own, orbit) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (x, y) => x.or(y),
        }
    }

    /// Largest step that keeps the integration of an orbit within a few thousandths of a radian
    /// per step: finer than manual flight, so that the ship really goes where the prediction says.
    fn nav_orbit_dt(&self) -> f64 {
        let mut dt = f64::MAX;
        for b in &self.bodies {
            if b.gm <= 1e6
                || matches!(
                    b.kind,
                    BodyKind::Station | BodyKind::Asteroid(_) | BodyKind::Anomaly(_)
                )
            {
                continue;
            }
            let d = (self.ship.pos - b.pos).length().max(1.0);
            dt = dt.min(0.003 * (d * d * d / b.gm).sqrt());
        }
        dt
    }

    /// Longest step of a coast under the queue (the plan is checked, so a long step is as exact
    /// as the integration of the ship in flight).
    pub fn nav_longest_dt(&self) -> f64 {
        if self.nav.run == Run::Running && self.autopilot.is_none() {
            1_800.0
        } else {
            120.0
        }
    }

    /// Time to turn the ship half a turn, plus a margin (s).
    pub fn nav_align_lead(&self) -> f64 {
        let accel = (self.effective.ang_accel * self.env_agility * self.attitude_scale()).max(1e-3);
        (2.0 * (std::f64::consts::PI / accel).sqrt() + 15.0).clamp(20.0, 900.0)
    }

    // ---- the engine of the queue ------------------------------------------------------------

    fn nav_step(
        &mut self,
        nav: &mut NavPlan,
        manual: &ShipInput,
        input: ShipInput,
        dt: f64,
    ) -> ShipInput {
        let mut ev: Vec<Event> = Vec::new();
        let out = self.nav_step_inner(nav, manual, input, dt, &mut ev);
        self.events.extend(ev);
        out
    }

    fn nav_halt_in(&mut self, nav: &mut NavPlan, ev: &mut Vec<Event>, why: String) {
        self.nav_release(nav);
        nav.say(ev, format!("NAVIGATION AUTO : {why}"));
        nav.run = Run::Halted(why);
    }

    fn nav_step_inner(
        &mut self,
        nav: &mut NavPlan,
        manual: &ShipInput,
        input: ShipInput,
        dt: f64,
        ev: &mut Vec<Event>,
    ) -> ShipInput {
        // The hands win: any real touch of the controls stops the computer.
        let rot_in = manual.pitch.abs() + manual.yaw.abs() + manual.roll.abs();
        let move_in = manual.fwd.abs() + manual.lat.abs() + manual.vert.abs();
        if move_in > 0.0 || rot_in > 0.5 || manual.brake {
            self.nav_halt_in(nav, ev, "MAINS REPRISES PAR LE PILOTE".into());
            return input;
        }
        for _ in 0..12 {
            let cur = nav.cursor();
            if cur >= nav.orders.len() {
                nav.say(ev, "NAVIGATION AUTO : PLAN ACHEVE");
                if nav.orders.iter().any(|e| e.status == Status::Done) {
                    self.cryo_arrival_wake();
                }
                nav.run = Run::Idle;
                nav.emergency = false;
                return input;
            }
            if nav.active.is_none() {
                match self.nav_begin(nav, cur, ev) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(m) => {
                        nav.orders[cur].status = Status::Failed;
                        self.nav_halt_in(nav, ev, m);
                        return input;
                    }
                }
            }
            if let Some(m) = self.nav_danger(nav, ev) {
                if let Some(r) = m.strip_prefix('!') {
                    // a threshold, not a surprise
                    if nav.limits.emergency && !nav.emergency && self.docked.is_none() {
                        self.nav_emergency(nav, r, ev);
                        continue;
                    }
                    self.nav_halt_in(nav, ev, r.to_string());
                } else {
                    self.nav_halt_in(nav, ev, m);
                }
                return input;
            }
            let cur = nav.cursor();
            match self.nav_run(nav, cur, input, dt, ev) {
                Step::Go(i) => return i,
                Step::Again => continue,
                Step::Done => {
                    nav.orders[cur].status = Status::Done;
                    nav.active = None;
                    continue;
                }
                Step::Fail(m) => {
                    nav.orders[cur].status = Status::Failed;
                    self.nav_halt_in(nav, ev, m);
                    return input;
                }
            }
        }
        input
    }

    /// Put the return home in front of the queue.
    fn nav_emergency(&mut self, nav: &mut NavPlan, why: &str, ev: &mut Vec<Event>) {
        let Some((st, _)) = self.nearest_station() else {
            self.nav_halt_in(nav, ev, format!("{why} : AUCUNE STATION"));
            return;
        };
        self.nav_release(nav);
        let at = nav.cursor();
        let star = Some(self.here);
        for (k, o) in [Order::Approach { body: st }, Order::Dock, Order::Halt]
            .into_iter()
            .enumerate()
        {
            nav.orders.insert(
                at + k,
                Entry {
                    order: o,
                    guard: None,
                    status: Status::Pending,
                    star,
                },
            );
        }
        nav.emergency = true;
        nav.say(ev, format!("RETOUR D'URGENCE : {why}"));
    }

    /// The monitor: None when all is well. A message starting with `!` is a threshold of the
    /// limits (it may send the ship home); the others are surprises that hand the ship back.
    fn nav_danger(&mut self, nav: &mut NavPlan, _ev: &mut Vec<Event>) -> Option<String> {
        if self.docked.is_some() {
            return None;
        }
        let lim = nav.limits;
        let fuel = self.ship.fuel / self.ship.stats.fuel_cap.max(1.0);
        let hull = self.ship.hull / self.ship.stats.hull_max.max(1.0);
        let emergency_on = nav.emergency;
        if !emergency_on {
            if fuel < lim.fuel_return {
                return Some(format!("!CARBURANT BAS ({:.0} %)", fuel * 100.0));
            }
            if hull < lim.hull_return {
                return Some(format!("!COQUE ENDOMMAGEE ({:.0} %)", hull * 100.0));
            }
            if self.dose_rate_aboard() > lim.dose_return {
                return Some(format!("!RADIATION ({:.2} GY/H)", self.dose_rate_aboard()));
            }
        }
        // The unified alerts: a critical one about the ship itself sends it home.
        if !emergency_on && self.steps.is_multiple_of(150) {
            use crate::alerts::{Severity, Topic};
            if let Some(al) = self.alerts().into_iter().find(|a| {
                a.sev == Severity::Critical
                    && (a.topic != Topic::Radiation || lim.dose_return < 5.0)
                    && matches!(
                        a.topic,
                        Topic::Hull | Topic::Fuel | Topic::Radiation | Topic::Heat | Topic::Power
                    )
            }) {
                return Some(format!("!ALERTE CRITIQUE : {}", al.text.to_uppercase()));
            }
        }
        if self.combat_active() {
            return Some("TIR OU COMBAT DETECTE : COMMANDES RENDUES".into());
        }
        if self.ship.sys.overheating {
            return Some("SURCHAUFFE : COMMANDES RENDUES".into());
        }
        if self.ship.fuel <= 0.0 {
            return Some("PLUS DE CARBURANT : COMMANDES RENDUES".into());
        }
        if self.structural_load_g() > 0.9 * crate::navigation::STRUCTURE_LIMIT_G {
            return Some("CHARGE STRUCTURELLE EXCESSIVE : COMMANDES RENDUES".into());
        }
        let a = nav.active.as_mut()?;
        // A gravity well that was not planned.
        if let Some(r) = self.ref_body {
            if !a.expected.contains(&r) && !matches!(self.bodies[r].kind, BodyKind::Station) {
                return Some(format!(
                    "PUITS DE GRAVITE NON PREVU : {} : COMMANDES RENDUES",
                    self.bodies[r].name.to_uppercase()
                ));
            }
        }
        // A fall toward a world that nobody asked to approach.
        if self.autopilot.is_none() && self.pilot.is_none() {
            if let Some((b, eta)) = self.impact_eta() {
                if eta < crate::alerts::IMPACT_SECONDS && a.dive_ok != Some(b) {
                    return Some(format!(
                        "COLLISION PREVUE SUR {} DANS {} : COMMANDES RENDUES",
                        self.bodies[b].name.to_uppercase(),
                        fmt_eta(eta)
                    ));
                }
            }
        }
        // The predicted path (not under a pilot that flies its own approach).
        let flies_own = a.owns_ap || a.owns_pilot || a.owns_jump;
        if !flies_own && self.time >= a.next_safety && !self.in_transit() {
            let horizon = self.default_horizon().min(86_400.0 * 30.0);
            a.next_safety = self.time + (horizon * 0.05).max(SAFETY_PERIOD);
            let dive = a.dive_ok;
            let nodes: Vec<ManeuverNode> = nav
                .orders
                .iter()
                .skip(nav.cursor())
                .take(1)
                .filter_map(|e| match &e.order {
                    Order::Burn(n) => Some(*n),
                    _ => None,
                })
                .collect();
            let p = self.predict_ship(horizon, &nodes, None);
            if let Some((b, t, _)) = p.impact {
                if dive != Some(b) && t - self.time < IMPACT_WARN {
                    return Some(format!(
                        "COLLISION PREVUE SUR {} DANS {} : COMMANDES RENDUES",
                        self.bodies[b].name.to_uppercase(),
                        fmt_eta(t - self.time)
                    ));
                }
            }
        }
        None
    }

    /// Start the order `cur`: guards, macros that replace themselves, and the set-up of the state.
    /// Ok(true): started; Ok(false): the queue changed, look again.
    fn nav_begin(
        &mut self,
        nav: &mut NavPlan,
        cur: usize,
        ev: &mut Vec<Event>,
    ) -> Result<bool, String> {
        let entry = nav.orders[cur].clone();
        if let Some(g) = entry.guard {
            if !self.nav_cond(g) {
                nav.orders[cur].status = Status::Skipped;
                nav.say(
                    ev,
                    format!("ORDRE IGNORE : CONDITION {} NON REMPLIE", g.label()),
                );
                return Ok(false);
            }
        }
        let order = entry.order.clone();
        if !order.bodies().is_empty() && entry.star != Some(self.here) {
            return Err("ORDRE ECRIT POUR UN AUTRE SYSTEME : PLAN A REVOIR".into());
        }
        if order.bodies().iter().any(|&b| b >= self.bodies.len()) {
            return Err("CORPS INCONNU : PLAN A REVOIR".into());
        }
        let needs_flight = !matches!(
            order,
            Order::Wait(_)
                | Order::WaitCond(_)
                | Order::WaitUntil(_)
                | Order::Halt
                | Order::Undock
                | Order::Dock
        );
        if self.docked.is_some() && needs_flight {
            if matches!(order, Order::Route { .. } | Order::Jump { .. }) {
                return Err("AMARRE : DECOLLER D'ABORD".into());
            }
            return Err("AMARRE : AJOUTER UN ORDRE DE DECOLLAGE EN TETE DE FILE".into());
        }
        let star = entry.star;
        let splice = |nav: &mut NavPlan, orders: Vec<Order>| {
            let guard = None;
            nav.orders.remove(cur);
            for (k, o) in orders.into_iter().enumerate() {
                nav.orders.insert(
                    cur + k,
                    Entry {
                        order: o,
                        guard,
                        status: Status::Pending,
                        star,
                    },
                );
            }
        };
        match &order {
            Order::Orbit {
                body,
                peri_alt,
                apo_alt,
                stage,
            } => {
                let steps = self.nav_orbit_steps(*body, *peri_alt, *apo_alt, *stage)?;
                if steps.is_empty() {
                    nav.say(ev, "ORBITE ATTEINTE");
                    nav.orders[cur].status = Status::Done;
                } else {
                    splice(nav, steps);
                }
                return Ok(false);
            }
            Order::Transfer { target, alt, aero } | Order::Depart { target, alt, aero } => {
                let soon = matches!(order, Order::Depart { .. });
                let steps = self.nav_transfer_steps(*target, *alt, *aero, soon)?;
                nav.say(
                    ev,
                    format!(
                        "TRANSFERT VERS {} : {} ETAPES",
                        self.bodies[*target].name.to_uppercase(),
                        steps.len()
                    ),
                );
                splice(nav, steps);
                return Ok(false);
            }
            Order::Patrol { waypoints, laps } => {
                if waypoints.is_empty() {
                    return Err("PATROUILLE SANS POINT DE PASSAGE".into());
                }
                let mut steps: Vec<Order> = waypoints
                    .iter()
                    .map(|&w| Order::Approach { body: w })
                    .collect();
                if *laps == 0 || *laps > 1 {
                    steps.push(Order::Patrol {
                        waypoints: waypoints.clone(),
                        laps: if *laps == 0 { 0 } else { laps - 1 },
                    });
                }
                splice(nav, steps);
                return Ok(false);
            }
            Order::ReturnStation => {
                let (st, _) = self
                    .nearest_station()
                    .ok_or("AUCUNE STATION DANS CE SYSTEME")?;
                splice(nav, vec![Order::Approach { body: st }, Order::Dock]);
                return Ok(false);
            }
            Order::Route { dest } => {
                let route = self.plan_route(*dest)?;
                nav.say(
                    ev,
                    format!(
                        "ROUTE FTL : {} SAUT(S), {:.1} AL",
                        route.hops.len(),
                        route.total_ly
                    ),
                );
                splice(
                    nav,
                    route
                        .hops
                        .iter()
                        .map(|h| Order::Jump { star: h.star })
                        .collect(),
                );
                return Ok(false);
            }
            Order::Correct { target, alt, t_arr } => {
                let (target, alt, t_arr) = (*target, *alt, *t_arr);
                match self.nav_correction(target, alt, t_arr)? {
                    Some(node) => {
                        nav.say(
                            ev,
                            format!("CORRECTION DE TRAJECTOIRE : {:.1} M/S", node.dv()),
                        );
                        splice(nav, vec![Order::Burn(node)]);
                    }
                    None => {
                        nav.say(ev, "TRAJECTOIRE DEJA BONNE : AUCUNE CORRECTION");
                        nav.orders[cur].status = Status::Done;
                    }
                }
                return Ok(false);
            }
            _ => {}
        }
        // A plain step: check what it needs, set the state.
        let mut a = Active {
            t0: self.time,
            hull0: self.ship.hull,
            expected: self.nav_expected(&order, &nav.orders[cur + 1..]),
            next_safety: self.time + 5.0,
            ..Default::default()
        };
        match &order {
            Order::Burn(n) => {
                if self.ship.stats.thrust <= 0.0 {
                    return Err("PAS DE PROPULSION".into());
                }
                if n.remaining() > self.ship.delta_v() * 0.98 {
                    return Err(format!(
                        "DELTA-V INSUFFISANT : {:.0} M/S DEMANDES, {:.0} DISPONIBLES",
                        n.remaining(),
                        self.ship.delta_v()
                    ));
                }
            }
            Order::Approach { body } => {
                if self.autopilot.is_none() {
                    self.engage_autopilot(*body).map_err(|e| e.to_uppercase())?;
                    a.owns_ap = true;
                }
            }
            Order::Land => {
                let m = self.engage_pilot(crate::landing::PilotKind::Land)?;
                nav.say(ev, m);
                a.owns_pilot = true;
                if let Some(s) = self.surface_state() {
                    a.dive_ok = Some(s.body);
                }
            }
            Order::Takeoff => {
                self.engage_pilot(crate::landing::PilotKind::Ascend)?;
                a.owns_pilot = true;
            }
            Order::Dock => {
                if self.docked.is_none() {
                    let (st, range) = self.nearest_station().ok_or("AUCUNE STATION")?;
                    if range > 20_000.0 {
                        return Err("STATION TROP LOIN POUR L'AMARRAGE".into());
                    }
                    a.station = st;
                    a.way = self.dock_waypoints(st);
                }
            }
            Order::Aerobrake { body } => {
                if self.air_of(*body).is_none() {
                    return Err("AUCUNE ATMOSPHERE POUR UN AEROFREINAGE".into());
                }
                a.dive_ok = Some(*body);
            }
            Order::Jump { star } => {
                let entry = self
                    .galaxy
                    .star(*star)
                    .ok_or("ETOILE INCONNUE : PLAN A REVOIR")?;
                self.jump_or_run(&entry).map_err(|e| e.to_uppercase())?;
                a.owns_jump = true;
            }
            _ => {}
        }
        nav.active = Some(a);
        Ok(true)
    }

    /// One step of the present order.
    fn nav_run(
        &mut self,
        nav: &mut NavPlan,
        cur: usize,
        input: ShipInput,
        dt: f64,
        ev: &mut Vec<Event>,
    ) -> Step {
        let order = nav.orders[cur].order.clone();
        let now = self.time;
        let Some(a) = nav.active.as_mut() else {
            return Step::Again;
        };
        match order {
            Order::Wait(s) => {
                if now - a.t0 >= s - 1e-6 {
                    Step::Done
                } else {
                    Step::Go(input)
                }
            }
            Order::WaitCond(c) => {
                if self.nav_cond(c) {
                    Step::Done
                } else {
                    Step::Go(input)
                }
            }
            Order::WaitUntil(t) => {
                if now >= t - 1e-6 {
                    Step::Done
                } else {
                    Step::Go(input)
                }
            }
            Order::Halt => {
                nav.orders[cur].status = Status::Done;
                nav.active = None;
                nav.emergency = false;
                self.nav_halt_in(nav, ev, "PLAN SUSPENDU : REPRENEZ QUAND VOUS VOULEZ".into());
                Step::Go(input)
            }
            Order::Undock => {
                self.undock();
                Step::Done
            }
            Order::Coast { body, max_s } => {
                if self.ref_body == Some(body) {
                    nav.say(
                        ev,
                        format!(
                            "SPHERE D'INFLUENCE DE {} ATTEINTE",
                            self.bodies[body].name.to_uppercase()
                        ),
                    );
                    Step::Done
                } else if now - a.t0 > max_s {
                    Step::Fail("DELAI DEPASSE : LA CIBLE N'A PAS ETE ATTEINTE".into())
                } else {
                    Step::Go(input)
                }
            }
            Order::Burn(n) => self.nav_run_burn(nav, n, input, dt, ev),
            Order::Approach { body } => {
                if self.autopilot.is_some() {
                    return Step::Go(input);
                }
                let b = &self.bodies[body];
                let d = (self.ship.pos - b.pos).length() - b.radius;
                let arrived = self
                    .events
                    .iter()
                    .any(|e| matches!(e, Event::Arrived { body: x } if *x == body));
                if arrived || d < 0.6 * b.radius + 3000.0 {
                    a.owns_ap = false;
                    Step::Done
                } else {
                    Step::Fail("APPROCHE INTERROMPUE".into())
                }
            }
            Order::Land => {
                if self.pilot.is_some() {
                    return Step::Go(input);
                }
                a.owns_pilot = false;
                if self.landed_on().is_some() {
                    Step::Done
                } else if let Some(m) = self.events.iter().rev().find_map(|e| match e {
                    Event::PilotFailed(m) => Some(*m),
                    _ => None,
                }) {
                    Step::Fail(m.to_string())
                } else {
                    Step::Fail("ATTERRISSAGE NON ACHEVE".into())
                }
            }
            Order::Takeoff => {
                if self.pilot.is_some() {
                    return Step::Go(input);
                }
                a.owns_pilot = false;
                if self.surface_state().is_none_or(|s| s.alt > 1000.0) {
                    Step::Done
                } else {
                    Step::Fail("DECOLLAGE NON ACHEVE".into())
                }
            }
            Order::Dock => {
                if self.docked.is_some() {
                    a.owns_pilot = false;
                    return Step::Done;
                }
                // First fly to the front of the port (around the station if need be).
                if a.way_i < a.way.len() {
                    let b = &self.bodies[a.station];
                    let wp = b.pos + a.way[a.way_i];
                    let near = (wp - self.ship.pos).length() < 60.0
                        && (self.ship.vel - b.vel).length() < 3.0;
                    if near {
                        a.way_i += 1;
                        return Step::Again;
                    }
                    let (p, v) = (wp, b.vel);
                    return Step::Go(self.nav_track(p, v, 20.0));
                }
                if self.pilot.is_some() {
                    return Step::Go(input);
                }
                if !a.owns_pilot {
                    return match self.engage_pilot(crate::landing::PilotKind::Dock) {
                        Ok(_) => {
                            a.owns_pilot = true;
                            Step::Go(input)
                        }
                        Err(e) => Step::Fail(e.to_uppercase()),
                    };
                }
                // The pilot is over: dock if close enough, else it failed.
                match self.try_dock() {
                    Ok(_) => Step::Done,
                    Err(e) => Step::Fail(format!("AMARRAGE IMPOSSIBLE : {}", e.to_uppercase())),
                }
            }
            Order::Aerobrake { body } => {
                let b = &self.bodies[body];
                let alt = (self.ship.pos - b.pos).length() - b.radius;
                let top = self.air_of(body).map_or(0.0, |x| x.top());
                if self.ship.hull < a.hull0 * 0.7 {
                    return Step::Fail("L'AEROFREINAGE ABIME LA COQUE : ABANDON".into());
                }
                let out = (self.ship.vel - b.vel).dot(self.ship.pos - b.pos) > 0.0;
                if alt < top {
                    a.entered = true;
                    Step::Go(input)
                } else if a.entered && out {
                    nav.say(ev, "AEROFREINAGE ACHEVE");
                    Step::Done
                } else if now - a.t0 > 4.0 * 86_400.0 {
                    Step::Fail("L'ATMOSPHERE N'A PAS ETE TRAVERSEE".into())
                } else {
                    Step::Go(input)
                }
            }
            Order::Jump { star } => {
                if self.here == star {
                    a.owns_jump = false;
                    self.nav_arrival(nav, ev);
                    return Step::Done;
                }
                if self.jump.is_some() || self.jump_run.is_some() {
                    return Step::Go(input);
                }
                Step::Fail("SAUT INTERROMPU : REPRISE POSSIBLE".into())
            }
            Order::KeepOrbit {
                body,
                peri_alt,
                apo_alt,
                tol,
                duration,
            } => {
                let elapsed = now - a.t0;
                if duration > 0.0 && elapsed >= duration {
                    return Step::Done;
                }
                if self.ref_body != Some(body) {
                    return Step::Fail("L'ORBITE A ETE QUITTEE".into());
                }
                let b = &self.bodies[body];
                let el = self.elements_about(body);
                let (rpt, rat) = (
                    b.radius + peri_alt.min(apo_alt),
                    b.radius + peri_alt.max(apo_alt),
                );
                let off = !el.bound
                    || (el.periapsis - rpt).abs() > tol * rpt
                    || el.apoapsis.is_none_or(|ra| (ra - rat).abs() > tol * rat);
                if off && now - a.t0 > 1.0 {
                    let rest = if duration > 0.0 {
                        (duration - elapsed).max(1.0)
                    } else {
                        0.0
                    };
                    nav.say(ev, "MAINTIEN D'ORBITE : DERIVE CORRIGEE");
                    let star = nav.orders[cur].star;
                    nav.orders.remove(cur);
                    for (k, o) in [
                        Order::Orbit {
                            body,
                            peri_alt,
                            apo_alt,
                            stage: 0,
                        },
                        Order::KeepOrbit {
                            body,
                            peri_alt,
                            apo_alt,
                            tol,
                            duration: rest,
                        },
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        nav.orders.insert(
                            cur + k,
                            Entry {
                                order: o,
                                guard: None,
                                status: Status::Pending,
                                star,
                            },
                        );
                    }
                    nav.active = None;
                    return Step::Again;
                }
                Step::Go(input)
            }
            Order::Escort {
                npc,
                dist,
                duration,
            } => {
                if duration > 0.0 && now - a.t0 >= duration {
                    return Step::Done;
                }
                match self.nav_escort_input(npc, dist) {
                    Some(i) => Step::Go(ShipInput {
                        mine: input.mine,
                        fire: input.fire,
                        fire_missile: input.fire_missile,
                        ..i
                    }),
                    None => Step::Fail("LE VAISSEAU SUIVI A DISPARU".into()),
                }
            }
            // macros never reach here
            _ => Step::Done,
        }
    }

    /// A burn, from the first prediction of its time to the cut-off.
    fn nav_run_burn(
        &mut self,
        nav: &mut NavPlan,
        node: ManeuverNode,
        input: ShipInput,
        dt: f64,
        ev: &mut Vec<Event>,
    ) -> Step {
        let now = self.time;
        let remaining = node.remaining();
        let (_, half) = self.burn_time(remaining);
        let lead = self.nav_align_lead();
        let Some(a) = nav.active.as_mut() else {
            return Step::Again;
        };
        if a.phase == Phase::Burn {
            // Cut-off: what the engines gave is measured on the velocity.
            let rem = a.dv_goal - a.dv_done;
            if rem <= 1e-4 + 1e-6 * a.dv_goal {
                let done = a.dv_done;
                let goal = a.dv_goal;
                nav.say(
                    ev,
                    format!("COMBUSTION ACHEVEE : {done:.1} M/S SUR {goal:.1} DEMANDES"),
                );
                return Step::Done;
            }
            if self.ship.fuel <= 0.0 {
                return Step::Fail("CARBURANT EPUISE PENDANT LA COMBUSTION".into());
            }
            let thrust = self.main_thrust();
            if thrust <= 0.0 {
                return Step::Fail("PLUS DE POUSSEE PENDANT LA COMBUSTION".into());
            }
            if now - a.t0 > 6.0 * self.burn_time(a.dv_goal).0 + 600.0 {
                return Step::Fail("LA COMBUSTION N'AVANCE PAS".into());
            }
            self.ship.rot = Mat3::look_at(a.dir, self.ship.rot.col(1));
            self.ship.ang = Vec3::ZERO;
            let a_full = thrust / self.ship.mass();
            let fwd = (rem / (a_full * dt.max(1e-6))).clamp(0.0, 1.0);
            return Step::Go(ShipInput {
                fwd,
                mine: input.mine,
                fire: input.fire,
                fire_missile: input.fire_missile,
                ..Default::default()
            });
        }
        // Not burning yet: find the time of the node (again, now and then).
        let force = a.phase == Phase::Coast && a.resolved.is_some_and(|r| now >= r.t - half - lead)
            || a.resolved.is_none();
        if (now >= a.next_resolve && a.phase != Phase::Align) || force {
            let horizon = match node.when {
                NodeWhen::At(t) => (t - now).max(0.0) + 120.0,
                _ => (self.default_horizon() * 1.2).clamp(600.0, 400.0 * 86_400.0),
            };
            let p = self.predict_ship(horizon, &[node], None);
            let Some(Some(r)) = p.nodes.first().copied() else {
                if let Some((b, t, _)) = p.impact {
                    return Step::Fail(format!(
                        "COLLISION SUR {} DANS {} AVANT LA MANOEUVRE",
                        self.bodies[b].name.to_uppercase(),
                        fmt_eta(t - now)
                    ));
                }
                return Step::Fail("LE POINT DE LA MANOEUVRE N'EST PAS ATTEINT".into());
            };
            if let Some((b, _, _)) = p.impact {
                if r.t > p.impact.map_or(f64::MAX, |x| x.1) && a.dive_ok != Some(b) {
                    return Step::Fail("COLLISION AVANT LA MANOEUVRE".into());
                }
            }
            a.resolved = Some(r);
            // An absolute date does not move: the next look is at the alignment. A relative one
            // (at the periapsis...) shifts with the perturbations: look again from time to time.
            a.next_resolve = if matches!(node.when, NodeWhen::At(_)) {
                (r.t - half - lead - 30.0).max(now + 5.0)
            } else {
                now + (0.2 * (r.t - now)).clamp(5.0, 3600.0)
            };
            // The burn direction at the node.
            let (bp, bv) = self.system.state(r.body, r.ts);
            let dv = node.vector(r.pos - bp, r.vel - bv);
            if dv.length() > 1e-9 {
                a.dir = dv.normalized();
            }
            if a.phase == Phase::Resolve {
                a.phase = Phase::Coast;
            }
        }
        let Some(r) = a.resolved else {
            return Step::Go(input);
        };
        let ignite = r.t - half;
        match a.phase {
            Phase::Coast => {
                if now >= ignite - lead {
                    a.phase = Phase::Align;
                    a.next_resolve = 0.0;
                }
                Step::Go(input)
            }
            Phase::Align => {
                let (p, y, rl, err) = self.steer_demand(a.dir);
                if now >= ignite && err < ALIGN_TOL {
                    a.phase = Phase::Burn;
                    // The direction of the burn is the node frame at the moment it starts.
                    let b = &self.bodies[r.body];
                    let dv = node.vector(self.ship.pos - b.pos, self.ship.vel - b.vel);
                    if dv.length() > 1e-9 {
                        a.dir = dv.normalized();
                    }
                    a.dv_goal = remaining;
                    a.dv_done = 0.0;
                    a.t0 = now;
                    a.vel_before = self.ship.vel;
                    a.grav_before = self.gravity_at(self.ship.pos);
                    nav.say(
                        ev,
                        format!(
                            "ALLUMAGE : {remaining:.1} M/S, {} DE COMBUSTION",
                            fmt_eta(self.burn_time(remaining).0)
                        ),
                    );
                    return Step::Go(input);
                }
                if now > ignite + 600.0 && err > 1.0 {
                    return Step::Fail("ORIENTATION IMPOSSIBLE : MANOEUVRE MANQUEE".into());
                }
                Step::Go(ShipInput {
                    pitch: p,
                    yaw: y,
                    roll: rl,
                    mine: input.mine,
                    fire: input.fire,
                    fire_missile: input.fire_missile,
                    ..Default::default()
                })
            }
            _ => Step::Go(input),
        }
    }

    /// After an arrival: the computer looks around and tells what it sees.
    fn nav_arrival(&mut self, nav: &mut NavPlan, ev: &mut Vec<Event>) {
        nav.emergency = false;
        let known = self.scan.iter().filter(|&&l| l >= 1).count();
        let hostile = self
            .npcs
            .iter()
            .filter(|n| n.hostile && !n.disabled && n.visible())
            .count();
        let msg = format!(
            "ARRIVEE : BALAYAGE {} CORPS, {} HOSTILE(S) DETECTE(S)",
            known, hostile
        );
        nav.say(ev, msg);
        for e in nav.orders.iter_mut() {
            if e.status == Status::Pending {
                // the orders of the next hops stay valid; orders of this system get its stamp
                if e.star.is_none() {
                    e.star = Some(self.here);
                }
            }
        }
        if hostile > 0 {
            // A queue that goes on jumping flees; the others stop and give the ship back.
            let flees = nav
                .orders
                .iter()
                .skip(nav.cursor() + 1)
                .find(|e| e.status == Status::Pending)
                .is_some_and(|e| matches!(e.order, Order::Jump { .. }));
            if !flees {
                nav.say(ev, "DANGER A L'ARRIVEE : COMMANDES RENDUES");
                nav.run = Run::Halted("HOSTILES A L'ARRIVEE".into());
            }
        }
    }

    /// Controls that hold the ship at `dist` of another ship and at its velocity.
    fn nav_escort_input(&self, id: u32, dist: f64) -> Option<ShipInput> {
        let n = self.npcs.iter().find(|n| n.id == id && n.visible())?;
        let rel = n.pos - self.ship.pos;
        let d = rel.length().max(1.0);
        let u = rel * (1.0 / d);
        let rv = n.vel - self.ship.vel;
        let t = 40.0;
        Some(self.nav_accel_input(u * ((d - dist) / (t * t)) + rv * (2.0 / t)))
    }

    /// Controls that fly the ship to the point `p` (star frame) and bring it to rest in the
    /// frame that moves at `v_frame`, at most `vmax` m/s.
    fn nav_track(&self, p: Vec3, v_frame: Vec3, vmax: f64) -> ShipInput {
        let off = p - self.ship.pos;
        let len = off.length();
        let speed = if len > 1e-9 {
            (0.05 * len).min(vmax)
        } else {
            0.0
        };
        let v_des = if len > 1e-9 {
            off * (speed / len) + v_frame
        } else {
            v_frame
        };
        self.nav_accel_input((v_des - self.ship.vel) * 0.5)
    }

    /// Controls that produce the acceleration `a_cmd` (m/s^2, star frame): the nose turns to it and
    /// the main engine pushes once it points there.
    fn nav_accel_input(&self, a_cmd: Vec3) -> ShipInput {
        let a_main = (self.main_thrust() / self.ship.mass()).max(1e-6);
        let want = a_cmd.length();
        if want < 0.002 * a_main {
            return ShipInput::default();
        }
        let (p, y, r, err) = self.steer_demand(a_cmd.normalized());
        ShipInput {
            pitch: p,
            yaw: y,
            roll: r,
            fwd: if err < 0.15 {
                (want / a_main).clamp(0.0, 1.0)
            } else {
                0.0
            },
            ..Default::default()
        }
    }

    /// The way to the front of the port of station `st` (relative to the station): straight when
    /// the ship is already on the port side, around the equator of the station otherwise (the
    /// port faces -Z; a ship behind it would be pressed against the hull).
    fn dock_waypoints(&self, st: usize) -> Vec<Vec3> {
        let b = &self.bodies[st];
        let q = self.ship.pos - b.pos;
        let ax = Vec3::new(0.0, 0.0, -1.0);
        let front_pt = ax * (b.radius + 20.0 + 250.0);
        let in_front = q.dot(ax) > 0.5 * q.length();
        if in_front {
            if q.length() > b.radius + 600.0 {
                vec![front_pt]
            } else {
                Vec::new()
            }
        } else {
            let mut perp = q - ax * q.dot(ax);
            if perp.length() < 1.0 {
                perp = Vec3::new(1.0, 0.0, 0.0);
            }
            vec![perp.normalized() * (b.radius + 250.0), front_pt]
        }
    }

    /// Entries of the plan as short lines for the panels: (text, status).
    pub fn nav_lines(&self) -> Vec<(String, Status)> {
        self.nav
            .orders
            .iter()
            .map(|e| (self.order_text(&e.order, e.guard), e.status))
            .collect()
    }

    pub fn order_text(&self, o: &Order, guard: Option<Cond>) -> String {
        let name = |i: usize| {
            self.bodies
                .get(i)
                .map_or("?".to_string(), |b| b.name.to_uppercase())
        };
        let km = |m: f64| format!("{:.0} KM", m / 1000.0);
        let mut s = match o {
            Order::Burn(n) => {
                let mut axes = Vec::new();
                if n.prograde != 0.0 {
                    axes.push(format!("{:+.1} PRO", n.prograde));
                }
                if n.normal != 0.0 {
                    axes.push(format!("{:+.1} NOR", n.normal));
                }
                if n.radial != 0.0 {
                    axes.push(format!("{:+.1} RAD", n.radial));
                }
                format!("MANOEUVRE {} {} M/S", n.when.label(), axes.join(" "))
            }
            Order::Wait(t) => format!("ATTENDRE {}", fmt_eta(*t)),
            Order::WaitCond(c) => format!("ATTENDRE QUE {}", c.label()),
            Order::WaitUntil(t) => {
                format!("ATTENDRE JUSQU'A T+{}", fmt_eta((*t - self.time).max(0.0)))
            }
            Order::Coast { body, .. } => format!("DERIVER JUSQU'A {}", name(*body)),
            Order::Correct { target, alt, .. } => {
                format!("CORRIGER VERS {} A {}", name(*target), km(*alt))
            }
            Order::Halt => "SUSPENDRE LE PLAN".into(),
            Order::Undock => "DECOLLER DU QUAI".into(),
            Order::Dock => "AMARRER".into(),
            Order::Land => "ATTERRIR".into(),
            Order::Takeoff => "DECOLLER VERS L'ORBITE".into(),
            Order::Approach { body } => format!("APPROCHER {}", name(*body)),
            Order::Aerobrake { body } => format!("AEROFREINER SUR {}", name(*body)),
            Order::Jump { star } => format!("SAUT FTL {:?}", star.index),
            Order::Orbit {
                body,
                peri_alt,
                apo_alt,
                ..
            } => {
                if (peri_alt - apo_alt).abs() < 1.0 {
                    format!("ORBITE CIRCULAIRE {} A {}", name(*body), km(*peri_alt))
                } else {
                    format!(
                        "ORBITE {} {} X {}",
                        name(*body),
                        km(*peri_alt),
                        km(*apo_alt)
                    )
                }
            }
            Order::Transfer { target, alt, aero } | Order::Depart { target, alt, aero } => format!(
                "{} VERS {} ORBITE {}{}",
                if matches!(o, Order::Depart { .. }) {
                    "DEPART"
                } else {
                    "TRANSFERT"
                },
                name(*target),
                km(*alt),
                if *aero { " AEROFREINAGE" } else { "" }
            ),
            Order::KeepOrbit {
                body,
                peri_alt,
                apo_alt,
                duration,
                ..
            } => format!(
                "MAINTENIR ORBITE {} {} X {}{}",
                name(*body),
                km(*peri_alt),
                km(*apo_alt),
                if *duration > 0.0 {
                    format!(" {}", fmt_eta(*duration))
                } else {
                    String::new()
                }
            ),
            Order::Escort { npc, dist, .. } => format!("ESCORTER VAISSEAU {npc} A {}", km(*dist)),
            Order::Patrol { waypoints, laps } => format!(
                "PATROUILLE {} POINTS {}",
                waypoints.len(),
                if *laps == 0 {
                    "SANS FIN".to_string()
                } else {
                    format!("{laps} TOUR(S)")
                }
            ),
            Order::ReturnStation => "RETOUR A LA STATION".into(),
            Order::Route { dest } => format!("ROUTE FTL VERS {:?}", dest.index),
        };
        if let Some(g) = guard {
            s = format!("SI {} : {s}", g.label());
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav_predict::PredKind;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn data() -> Arc<GameData> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        Arc::new(GameData::load(&dir).expect("data"))
    }

    /// The ship on a circular orbit of `radii` radii of body `i` (seed `seed`), with the limits
    /// that do not depend on the test world (belts, heat) out of the way.
    fn orbiting(seed: u64, i: usize, radii: f64) -> Sim {
        let mut s = new_sim(seed, data());
        let b = s.bodies[i].clone();
        let r = b.radius * radii;
        s.ship.pos = b.pos + Vec3::new(r, 0.0, 0.0);
        s.ship.vel = b.vel + Vec3::new(0.0, 0.0, (b.gm / r).sqrt());
        s.update_ref_pub();
        s.ship.fuel = s.ship.stats.fuel_cap;
        s.nav.limits.dose_return = 1.0e9;
        s.nav.limits.emergency = false;
        s
    }

    /// Fly until the queue stops (or `max_s` of game time pass).
    fn run(s: &mut Sim, max_s: f64, chunk: f64) {
        let t0 = s.time;
        while s.nav_running() && s.time - t0 < max_s {
            s.events.clear();
            s.advance(chunk, &ShipInput::default());
        }
    }

    #[test]
    fn a_burn_node_gives_the_delta_v_asked_to_one_percent_and_burns_real_fuel() {
        let (mut s, i) = (orbiting(1, 2, 1.5), 2usize);
        let (r, mu) = {
            let b = &s.bodies[i];
            ((s.ship.pos - b.pos).length(), b.gm)
        };
        let dv = 300.0;
        let node = ManeuverNode::new(NodeWhen::Periapsis, Some(i), dv, 0.0, 0.0);
        s.nav_add(Order::Burn(node)).unwrap();
        let (fuel0, m0) = (s.ship.fuel, s.ship.mass());
        let isp = s.nav_isp();
        s.nav_start().unwrap();
        run(&mut s, 20_000.0, 20.0);
        assert_eq!(s.nav.run, Run::Idle, "plan ended: {:?}", s.nav.log);
        assert!(s.nav.orders.iter().all(|e| e.status == Status::Done));
        // The orbit that results is the vis-viva one for that delta-v.
        let v1 = (mu / r).sqrt() + dv;
        let a = 1.0 / (2.0 / r - v1 * v1 / mu);
        let ra = 2.0 * a - r;
        let apo = s.elements_about(i).apoapsis.expect("bound");
        assert!((apo - ra).abs() / ra < 0.01, "apoapsis {apo} expected {ra}");
        // The propellant is what the rocket equation says.
        let used = fuel0 - s.ship.fuel;
        let want = crate::navigation::propellant_for(dv, isp, m0);
        assert!((used - want).abs() / want < 0.03, "fuel {used} vs {want}");
    }

    #[test]
    fn the_burn_is_the_same_at_any_time_acceleration() {
        let result = |chunk: f64| {
            let (mut s, i) = (orbiting(1, 2, 1.5), 2usize);
            let node = ManeuverNode::new(NodeWhen::Periapsis, Some(i), 250.0, 0.0, 0.0);
            s.nav_add(Order::Burn(node)).unwrap();
            s.nav_start().unwrap();
            run(&mut s, 20_000.0, chunk);
            s.elements_about(i).apoapsis.expect("bound")
        };
        let slow = result(5.0);
        let fast = result(4000.0);
        assert!(
            (slow - fast).abs() / slow < 0.01,
            "{slow} at x1 against {fast} warped"
        );
    }

    #[test]
    fn an_orbit_order_reaches_a_circular_orbit_at_the_altitude_asked() {
        let (mut s, i) = (orbiting(1, 2, 1.4), 2usize);
        let rad = s.bodies[i].radius;
        let alt = 1.0 * rad;
        s.nav_add(Order::Orbit {
            body: i,
            peri_alt: alt,
            apo_alt: alt,
            stage: 0,
        })
        .unwrap();
        s.nav_start().unwrap();
        run(&mut s, 400_000.0, 30.0);
        assert_eq!(s.nav.run, Run::Idle, "{:?}", s.nav.log);
        let el = s.elements_about(i);
        let want = rad + alt;
        assert!(
            (el.periapsis - want).abs() / want < 0.02
                && (el.apoapsis.unwrap() - want).abs() / want < 0.02,
            "{} x {} for {want}",
            el.periapsis,
            el.apoapsis.unwrap()
        );
        // And it is a plan the preview can show: no node left, no impact.
        assert!(s.nav_nodes().is_empty());
        assert!(s.predict_ship(86_400.0, &[], None).impact.is_none());
        let _ = PredKind::Periapsis;
    }

    /// The ship on a circular orbit around the star at the distance of planet `from`, placed so
    /// that the window of a Hohmann transfer to planet `to` is open, and well away from every
    /// sphere of influence.
    fn cruising(seed: u64, from: usize, to: usize) -> Sim {
        let mut s = new_sim(seed, data());
        let r = s.system.bodies[from].orbit.as_ref().expect("orbit").a;
        let r2 = s.system.bodies[to].orbit.as_ref().expect("orbit").a;
        let pt = s.bodies[to].pos;
        let theta = pt.z.atan2(pt.x) - crate::navigation::hohmann_lead_angle(r, r2);
        let v = (s.bodies[0].gm / r).sqrt();
        s.ship.pos = Vec3::new(r * theta.cos(), 0.0, r * theta.sin());
        s.ship.vel = Vec3::new(-v * theta.sin(), 0.0, v * theta.cos());
        s.update_ref_pub();
        s.ship.fuel = s.ship.stats.fuel_cap;
        s.nav.limits.dose_return = 1.0e9;
        s.nav.limits.emergency = false;
        s
    }

    #[test]
    fn a_hohmann_plan_to_another_world_arrives_and_ends_on_a_circular_orbit_there() {
        // From a heliocentric orbit at the distance of planet 3 to planet 4 of seed 1 (beyond the
        // belt of asteroids, which would cost the ship its hull).
        let mut s = cruising(1, 3, 4);
        let rad = s.bodies[4].radius;
        let alt = 0.8 * rad;
        let opts = s.plan_transfer_options(4, alt);
        let h = opts
            .iter()
            .find(|o| o.kind == crate::nav_plan::PlanKind::Hohmann)
            .expect("option");
        assert!(h.ok, "{:?}", h.why);
        assert!(h.dv > 1000.0 && h.dv < 20_000.0, "{}", h.dv);
        assert!(h.duration > 86_400.0 * 30.0);
        s.nav_add(Order::Transfer {
            target: 4,
            alt,
            aero: false,
        })
        .unwrap();
        s.nav_start().unwrap();
        run(&mut s, 3.0e8, 86_400.0);
        assert_eq!(s.nav.run, Run::Idle, "{:?}", s.nav.log);
        assert_eq!(s.ref_body, Some(4), "ship is in the sphere of the target");
        let el = s.elements_about(4);
        let want = rad + alt;
        assert!(el.bound, "captured");
        assert!(
            (el.periapsis - want).abs() / want < 0.03
                && (el.apoapsis.unwrap() - want).abs() / want < 0.03,
            "{} x {} for {want}",
            el.periapsis,
            el.apoapsis.unwrap()
        );
        assert!(s.ship.fuel < s.ship.stats.fuel_cap, "paid in fuel");
    }

    /// Set the clock to the next time the Hohmann window from `from` to `to` is open.
    fn open_window(s: &mut Sim, from: usize, to: usize) {
        use std::f64::consts::TAU;
        let a = |i: usize| s.system.bodies[i].orbit.as_ref().expect("orbit").a;
        let need = crate::navigation::hohmann_lead_angle(a(from), a(to)).rem_euclid(TAU);
        let ang = |t: f64, i: usize| {
            let p = s.system.state(i, t).0;
            p.z.atan2(p.x)
        };
        let (mut best, mut at) = (f64::MAX, 0.0);
        let mut t = 0.0;
        while t < 1.3e8 {
            let lead = (ang(t, to) - ang(t, from)).rem_euclid(TAU);
            let err =
                ((lead - need + std::f64::consts::PI).rem_euclid(TAU) - std::f64::consts::PI).abs();
            if err < best {
                best = err;
                at = t;
            }
            t += 86_400.0;
        }
        s.time = at;
        s.refresh_bodies();
    }

    #[test]
    fn a_transfer_from_a_planet_orbit_to_a_sibling_world_arrives_and_captures() {
        // Seed 1: from a circular orbit of planet 3 to planet 4, the window open.
        let mut s = orbiting(1, 3, 2.0);
        open_window(&mut s, 3, 4);
        let b = s.bodies[3].clone();
        let r = b.radius * 2.0;
        s.ship.pos = b.pos + Vec3::new(r, 0.0, 0.0);
        s.ship.vel = b.vel + Vec3::new(0.0, 0.0, (b.gm / r).sqrt());
        s.update_ref_pub();
        let rad = s.bodies[4].radius;
        let alt = 0.8 * rad;
        s.nav_add(Order::Transfer {
            target: 4,
            alt,
            aero: false,
        })
        .unwrap();
        s.nav_start().unwrap();
        run(&mut s, 1.5e8, 86_400.0);
        assert_eq!(s.nav.run, Run::Idle, "{:?}", s.nav.log);
        assert_eq!(s.ref_body, Some(4));
        let el = s.elements_about(4);
        let want = rad + alt;
        assert!(el.bound);
        assert!(
            (el.periapsis - want).abs() / want < 0.05
                && (el.apoapsis.unwrap() - want).abs() / want < 0.05,
            "{} x {} for {want}",
            el.periapsis,
            el.apoapsis.unwrap()
        );
    }

    #[test]
    fn the_queue_runs_in_order_skips_a_false_guard_and_can_be_edited() {
        let mut s = orbiting(1, 2, 1.5);
        s.nav_add(Order::Wait(30.0)).unwrap();
        s.nav_add_guarded(Order::Wait(5_000.0), Some(Cond::FuelBelow(0.01)))
            .unwrap();
        s.nav_add(Order::Wait(20.0)).unwrap();
        s.nav_add(Order::Wait(10.0)).unwrap();
        // Reorder: the 10 s wait goes before the 20 s one.
        s.nav_move(3, true);
        assert_eq!(s.nav.orders[2].order, Order::Wait(10.0));
        let t0 = s.time;
        s.nav_start().unwrap();
        run(&mut s, 1000.0, 5.0);
        let st: Vec<Status> = s.nav.orders.iter().map(|e| e.status).collect();
        assert_eq!(
            st,
            vec![Status::Done, Status::Skipped, Status::Done, Status::Done]
        );
        let spent = s.time - t0;
        assert!((spent - 60.0).abs() < 8.0, "the waits add up: {spent}");
        assert_eq!(s.nav.run, Run::Idle);
        // Editing: remove and clear.
        s.nav_clear_done();
        assert!(s.nav.orders.is_empty());
        s.nav_add(Order::Wait(1.0)).unwrap();
        s.nav_remove(0);
        assert!(s.nav.orders.is_empty());
        assert!(s.nav_start().is_err(), "an empty queue does not start");
    }

    #[test]
    fn two_burns_light_in_the_order_of_the_queue() {
        let mut s = orbiting(1, 2, 1.5);
        for dv in [5.0, 7.0] {
            s.nav_add(Order::Burn(ManeuverNode::new(
                NodeWhen::Periapsis,
                Some(2),
                dv,
                0.0,
                0.0,
            )))
            .unwrap();
        }
        s.nav_start().unwrap();
        run(&mut s, 50_000.0, 20.0);
        let lights: Vec<&String> = s
            .nav
            .log
            .iter()
            .filter(|m| m.starts_with("ALLUMAGE"))
            .collect();
        assert_eq!(lights.len(), 2, "{:?}", s.nav.log);
        assert!(
            lights[0].contains("5.0 M/S") && lights[1].contains("7.0 M/S"),
            "{lights:?}"
        );
    }

    #[test]
    fn the_computer_stops_and_gives_the_ship_back_on_a_predicted_collision() {
        let mut s = orbiting(1, 2, 3.0);
        let b = s.bodies[2].clone();
        // Almost at rest, the flight assist off: the ship falls on the world.
        s.ship.assist = false;
        s.ship.vel = b.vel + Vec3::new(0.0, 0.0, 30.0);
        s.nav_add(Order::Wait(1.0e7)).unwrap();
        s.nav_start().unwrap();
        run(&mut s, 200_000.0, 20.0);
        match &s.nav.run {
            Run::Halted(m) => assert!(m.contains("COLLISION"), "{m}"),
            r => panic!("expected a halt, got {r:?}"),
        }
        let alt = (s.ship.pos - s.bodies[2].pos).length() - s.bodies[2].radius;
        assert!(
            alt > 0.0,
            "the computer gave the controls back before the ground"
        );
    }

    #[test]
    fn the_computer_stops_before_a_gravity_well_it_did_not_plan() {
        let mut s = new_sim(1, data());
        let p = s.bodies[3].clone();
        let soi = s.influence_of(3);
        let dir = Vec3::new(1.0, 0.0, 0.0);
        s.ship.pos = p.pos + dir * (soi * 1.05);
        s.ship.vel = p.vel - dir * 20_000.0;
        s.update_ref_pub();
        assert!(s.ref_body.is_none());
        s.nav.limits.dose_return = 1.0e9;
        s.nav_add(Order::Wait(1.0e8)).unwrap();
        s.nav_start().unwrap();
        run(&mut s, 1.0e6, 500.0);
        match &s.nav.run {
            Run::Halted(m) => assert!(m.contains("PUITS"), "{m}"),
            r => panic!("expected a halt, got {r:?}"),
        }
    }

    #[test]
    fn a_low_tank_sends_the_ship_home_to_the_station_and_docks_it() {
        let mut s = orbiting(1, 2, 1.5);
        s.nav.limits.emergency = true;
        s.nav.limits.fuel_return = 0.6;
        s.ship.fuel = 0.5 * s.ship.stats.fuel_cap;
        s.nav_add(Order::Wait(1.0e6)).unwrap();
        s.nav_start().unwrap();
        for _ in 0..600 {
            s.events.clear();
            s.advance(60.0, &ShipInput::default());
            if !s.nav_running() {
                break;
            }
        }
        assert!(
            s.docked.is_some(),
            "docked: {:?} {:?}",
            s.nav.run,
            s.nav.log
        );
        assert!(
            s.nav
                .log
                .iter()
                .any(|m| m.contains("RETOUR D'URGENCE") && m.contains("CARBURANT")),
            "{:?}",
            s.nav.log
        );
        assert!(matches!(s.nav.run, Run::Halted(_)));
        // The interrupted order is still there, to resume.
        assert_eq!(s.nav.orders.last().map(|e| e.status), Some(Status::Pending));
    }

    #[test]
    fn a_damaged_hull_sends_the_ship_home_too_and_a_disabled_threshold_only_stops() {
        let mut s = orbiting(1, 2, 1.5);
        s.nav.limits.emergency = true;
        s.ship.hull = 0.2 * s.ship.stats.hull_max;
        s.nav_add(Order::Wait(1.0e6)).unwrap();
        s.nav_start().unwrap();
        s.advance(2.0, &ShipInput::default());
        assert!(s.nav.emergency, "{:?}", s.nav.log);
        assert!(s.nav.log.iter().any(|m| m.contains("COQUE")));
        let mut t = orbiting(1, 2, 1.5);
        t.nav.limits.emergency = false;
        t.ship.hull = 0.2 * t.ship.stats.hull_max;
        t.nav_add(Order::Wait(1.0e6)).unwrap();
        t.nav_start().unwrap();
        t.advance(2.0, &ShipInput::default());
        assert!(!t.nav.emergency);
        assert!(matches!(t.nav.run, Run::Halted(_)), "{:?}", t.nav.run);
    }

    #[test]
    fn a_touch_of_the_controls_takes_the_ship_back() {
        let mut s = orbiting(1, 2, 1.5);
        s.nav_add(Order::Wait(1.0e6)).unwrap();
        s.nav_start().unwrap();
        s.advance(1.0, &ShipInput::default());
        assert!(s.nav_running());
        s.advance(
            1.0,
            &ShipInput {
                fwd: 1.0,
                ..Default::default()
            },
        );
        assert!(matches!(s.nav.run, Run::Halted(_)));
    }

    #[test]
    fn keeping_an_orbit_corrects_its_drift() {
        let mut s = orbiting(1, 2, 1.5);
        let rad = s.bodies[2].radius;
        let alt = 0.5 * rad;
        // A kick that makes the orbit elliptic.
        s.ship.vel = s.ship.vel + (s.ship.vel - s.bodies[2].vel).normalized() * 60.0;
        s.nav_add(Order::KeepOrbit {
            body: 2,
            peri_alt: alt,
            apo_alt: alt,
            tol: 0.02,
            duration: 60_000.0,
        })
        .unwrap();
        s.nav_start().unwrap();
        run(&mut s, 200_000.0, 60.0);
        assert_eq!(s.nav.run, Run::Idle, "{:?}", s.nav.log);
        let el = s.elements_about(2);
        let want = rad + alt;
        assert!(
            (el.periapsis - want).abs() / want < 0.03
                && (el.apoapsis.unwrap() - want).abs() / want < 0.03,
            "{} x {} for {want}",
            el.periapsis,
            el.apoapsis.unwrap()
        );
    }

    #[test]
    fn the_same_plan_flown_twice_gives_the_same_ship() {
        let fly = || {
            let mut s = orbiting(1, 2, 1.4);
            let alt = 0.9 * s.bodies[2].radius;
            s.nav_add(Order::Orbit {
                body: 2,
                peri_alt: alt,
                apo_alt: alt,
                stage: 0,
            })
            .unwrap();
            s.nav_start().unwrap();
            run(&mut s, 400_000.0, 30.0);
            (s.ship.pos, s.ship.vel, s.ship.fuel, s.nav.log.clone())
        };
        let (a, b) = (fly(), fly());
        assert_eq!(a.0.x.to_bits(), b.0.x.to_bits());
        assert_eq!(a.1.z.to_bits(), b.1.z.to_bits());
        assert_eq!(a.2.to_bits(), b.2.to_bits());
        assert_eq!(a.3, b.3);
    }

    #[test]
    fn the_plan_survives_a_save_and_a_load() {
        let mut s = orbiting(1, 2, 1.5);
        let mut node = ManeuverNode::new(NodeWhen::Apoapsis, Some(2), 12.5, -3.0, 1.0);
        node.done = 4.0;
        s.nav_add(Order::Burn(node)).unwrap();
        s.nav_add_guarded(
            Order::Orbit {
                body: 2,
                peri_alt: 1.0e6,
                apo_alt: 2.0e6,
                stage: 0,
            },
            Some(Cond::FuelAbove(0.3)),
        )
        .unwrap();
        s.nav_add(Order::Patrol {
            waypoints: vec![2, 3],
            laps: 2,
        })
        .unwrap();
        s.nav.limits.fuel_return = 0.17;
        let bytes = s.save_bytes().expect("save");
        let t = Sim::load_bytes(&bytes, data()).expect("load");
        assert_eq!(t.nav.orders, s.nav.orders);
        assert_eq!(t.nav.limits, s.nav.limits);
        assert_eq!(t.nav.run, Run::Idle);
        // A plan that was running comes back paused: a loaded game does not fly by itself.
        let mut u = orbiting(1, 2, 1.5);
        u.nav_add(Order::Wait(1.0e5)).unwrap();
        u.nav_start().unwrap();
        u.advance(2.0, &ShipInput::default());
        assert!(u.nav_running());
        let bytes = u.save_bytes().expect("save");
        let v = Sim::load_bytes(&bytes, data()).expect("load");
        assert_eq!(v.nav.run, Run::Paused);
        assert_eq!(v.nav.orders.len(), 1);
    }

    #[test]
    fn an_orbit_around_a_world_that_is_not_ours_is_refused_with_a_reason() {
        let mut s = orbiting(1, 2, 1.5);
        s.nav_add(Order::Orbit {
            body: 3,
            peri_alt: 1.0e6,
            apo_alt: 1.0e6,
            stage: 0,
        })
        .unwrap();
        s.nav_start().unwrap();
        s.advance(2.0, &ShipInput::default());
        match &s.nav.run {
            Run::Halted(m) => assert!(m.contains("SPHERE"), "{m}"),
            r => panic!("{r:?}"),
        }
        assert_eq!(s.nav.orders[0].status, Status::Failed);
        // Not enough delta-v for a burn is refused before the first drop of fuel is spent.
        let mut t = orbiting(1, 2, 1.5);
        t.ship.fuel = 1.0;
        let f0 = t.ship.fuel;
        t.nav_add(Order::Burn(ManeuverNode::new(
            NodeWhen::Periapsis,
            Some(2),
            500_000.0,
            0.0,
            0.0,
        )))
        .unwrap();
        t.nav.limits.fuel_return = 0.0;
        t.nav_start().unwrap();
        t.advance(2.0, &ShipInput::default());
        assert!(
            matches!(&t.nav.run, Run::Halted(m) if m.contains("DELTA-V")),
            "{:?}",
            t.nav.run
        );
        assert_eq!(t.ship.fuel, f0);
    }

    #[test]
    fn a_route_of_jumps_is_planned_within_range_and_flown_to_the_destination() {
        use vector_world::galaxy::Galaxy;
        let g = Galaxy::new(5);
        // A start from which a star needs a chain of jumps (farther than one jump, reachable).
        let mut found = None;
        'starts: for entry in g
            .nearby([0.0; 3], 40.0)
            .into_iter()
            .filter(|e| g.nearby(e.pos, 4.0).len() >= 3)
            .take(12)
        {
            let mut s = Sim::new_in_galaxy(g, &entry, data());
            s.rules.reveal_map = true;
            let range = s.jump_range();
            for e in g
                .nearby(s.here_pos, 4.0 * range)
                .into_iter()
                .filter(|e| e.distance_to(s.here_pos) > 1.2 * range)
            {
                if s.plan_route(e.id).is_ok_and(|r| r.hops.len() >= 2 && r.ok) {
                    found = Some((entry.clone(), e));
                    break 'starts;
                }
            }
        }
        let (entry, dest) = found.expect("a destination that needs a chain of jumps");
        let mut s = Sim::new_in_galaxy(g, &entry, data());
        s.rules.reveal_map = true;
        s.ship.pos = Vec3::new(4.0e11, 0.0, 0.0);
        s.ship.vel = Vec3::ZERO;
        s.update_ref_pub();
        s.nav.limits.dose_return = 1.0e9;
        let range = s.jump_range();
        let route = s.plan_route(dest.id).unwrap();
        assert!(route.hops.iter().all(|h| h.dist <= range + 1e-9));
        assert_eq!(route.hops.last().map(|h| h.star), Some(dest.id));
        assert!(route.total_ly >= dest.distance_to(s.here_pos) - 1e-9);
        // A tank too small makes the same route a refused one.
        let mut poor = Sim::new_in_galaxy(g, &entry, data());
        poor.rules.reveal_map = true;
        poor.ship.fuel = 1.0;
        let r = poor.plan_route(dest.id).unwrap();
        assert!(!r.ok && r.why.is_some());
        // Fly it.
        s.nav_add(Order::Route { dest: dest.id }).unwrap();
        s.nav_start().unwrap();
        for _ in 0..3000 {
            s.events.clear();
            s.advance(1800.0, &ShipInput::default());
            if !s.nav_running() {
                break;
            }
        }
        assert_eq!(s.here, dest.id, "{:?} {:?}", s.nav.run, s.nav.log);
        assert_eq!(s.nav.run, Run::Idle);
        assert!(s.nav.log.iter().any(|m| m.starts_with("ARRIVEE")));
    }

    #[test]
    fn a_failed_order_is_tried_again_after_the_incident_is_over() {
        // A jump without the fuel for it: refused with a reason; with the tank full the same plan
        // goes on from where it stopped.
        use vector_world::galaxy::Galaxy;
        let g = Galaxy::new(5);
        let entry = g
            .nearby([0.0; 3], 30.0)
            .into_iter()
            .find(|e| g.nearby(e.pos, 4.0).len() >= 3)
            .expect("a star with neighbours");
        let mut s = Sim::new_in_galaxy(g, &entry, data());
        s.rules.reveal_map = true;
        s.ship.pos = Vec3::new(4.0e11, 0.0, 0.0);
        s.ship.vel = Vec3::ZERO;
        s.update_ref_pub();
        s.nav.limits.dose_return = 1.0e9;
        s.nav.limits.fuel_return = 0.0;
        let dest = g
            .nearby(s.here_pos, s.jump_range())
            .into_iter()
            .find(|e| e.id != s.here)
            .expect("a neighbour");
        let full = s.ship.fuel;
        s.ship.fuel = 1.0;
        s.nav_add(Order::Jump { star: dest.id }).unwrap();
        s.nav_start().unwrap();
        s.advance(5.0, &ShipInput::default());
        assert!(
            matches!(&s.nav.run, Run::Halted(m) if m.contains("CARBURANT")),
            "{:?}",
            s.nav.run
        );
        assert_eq!(s.nav.orders[0].status, Status::Failed);
        s.ship.fuel = full;
        s.nav_start().expect("the plan can be started again");
        for _ in 0..3000 {
            s.events.clear();
            s.advance(1800.0, &ShipInput::default());
            if !s.nav_running() {
                break;
            }
        }
        assert_eq!(s.here, dest.id, "{:?} {:?}", s.nav.run, s.nav.log);
    }

    #[test]
    fn a_patrol_becomes_one_approach_per_waypoint_and_goes_round_again() {
        let mut s = orbiting(1, 2, 1.5);
        s.nav_add(Order::Patrol {
            waypoints: vec![53, 2],
            laps: 2,
        })
        .unwrap();
        s.nav_start().unwrap();
        // One step: the macro is replaced by its waypoints and a patrol with one lap less.
        s.advance(1.0, &ShipInput::default());
        let orders: Vec<&Order> = s.nav.orders.iter().map(|e| &e.order).collect();
        assert_eq!(orders[0], &Order::Approach { body: 53 });
        assert_eq!(orders[1], &Order::Approach { body: 2 });
        assert_eq!(
            orders[2],
            &Order::Patrol {
                waypoints: vec![53, 2],
                laps: 1
            }
        );
    }

    #[test]
    fn an_approach_order_brings_the_ship_to_the_station_and_stops_there() {
        let mut s = orbiting(1, 2, 1.5);
        s.nav.limits.emergency = false;
        s.nav_add(Order::Approach { body: 53 }).unwrap();
        s.nav_start().unwrap();
        run(&mut s, 100_000.0, 60.0);
        assert_eq!(s.nav.run, Run::Idle, "{:?}", s.nav.log);
        let d = (s.ship.pos - s.bodies[53].pos).length();
        assert!(d < 2000.0, "{d}");
        assert!((s.ship.vel - s.bodies[53].vel).length() < 5.0);
    }

    #[test]
    fn an_escort_pushes_towards_a_ship_that_is_too_far_and_matches_its_speed() {
        let mut s = orbiting(1, 2, 1.5);
        s.spawn_traffic();
        let k = s.npcs.iter().position(|n| n.visible()).expect("a ship");
        let id = s.npcs[k].id;
        // The ship ahead, 30 km away, nose on it: thrust. Too close and moving away: no push on
        // the nose, the controller turns the ship round first.
        s.ship.rot = Mat3::look_at(Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));
        s.npcs[k].pos = s.ship.pos + Vec3::new(30_000.0, 0.0, 0.0);
        s.npcs[k].vel = s.ship.vel;
        let far = s.nav_escort_input(id, 5_000.0).expect("controls");
        assert!(far.fwd > 0.0, "closes in: {far:?}");
        s.npcs[k].pos = s.ship.pos + Vec3::new(1_000.0, 0.0, 0.0);
        let near = s.nav_escort_input(id, 5_000.0).expect("controls");
        assert_eq!(near.fwd, 0.0, "too close: it turns first: {near:?}");
        assert!(near.pitch.abs() + near.yaw.abs() > 0.0);
        // A ship that went away is a failed order, with a reason.
        assert!(s.nav_escort_input(id + 9999, 5_000.0).is_none());
        // Matching the speed: the other ship runs ahead at 40 m/s and the ship wants to follow.
        s.npcs[k].pos = s.ship.pos + Vec3::new(5_000.0, 0.0, 0.0);
        s.npcs[k].vel = s.ship.vel + Vec3::new(40.0, 0.0, 0.0);
        let chase = s.nav_escort_input(id, 5_000.0).expect("controls");
        assert!(chase.fwd > 0.0, "{chase:?}");
    }

    #[test]
    fn an_orbit_is_never_corrected_from_a_periapsis_inside_the_air() {
        // A world with air; the ship on an ellipse whose periapsis dips into it.
        let mut found = None;
        for seed in 1..40u64 {
            let s = new_sim(seed, data());
            if let Some(i) = (1..s.bodies.len()).find(|&i| {
                matches!(s.bodies[i].kind, BodyKind::Planet(_) | BodyKind::Moon(_))
                    && s.air_of(i).is_some()
                    && s.bodies[i].radius > 2.0e6
                    && !matches!(s.bodies[i].kind, BodyKind::Planet(k) | BodyKind::Moon(k) if k.is_giant())
            }) {
                found = Some((seed, i));
                break;
            }
        }
        let (seed, i) = found.expect("a world with air");
        let mut s = orbiting(seed, i, 6.0);
        let b = s.bodies[i].clone();
        let top = s.air_of(i).expect("air").top();
        let rp = b.radius + 0.4 * top;
        let ra = 6.0 * b.radius;
        let a = 0.5 * (rp + ra);
        let v_apo = (b.gm * (2.0 / ra - 1.0 / a)).sqrt();
        s.ship.vel = b.vel + Vec3::new(0.0, 0.0, v_apo);
        s.update_ref_pub();
        let steps = s
            .nav_orbit_steps(i, 0.5 * b.radius, 0.5 * b.radius, 0)
            .expect("a way");
        match steps.first() {
            Some(Order::Burn(n)) => {
                assert_eq!(n.when, NodeWhen::Apoapsis, "the periapsis is in the air");
                assert!(n.prograde > 0.0, "it is raised");
            }
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn a_fight_hands_the_ship_back_and_hostiles_at_the_arrival_stop_a_plan_that_does_not_flee() {
        let mut s = orbiting(1, 2, 1.5);
        s.spawn_traffic();
        let k = s.npcs.iter().position(|n| n.visible()).expect("a ship");
        s.npcs[k].state = crate::npc::NpcState::Combat {
            mode: crate::npc::AiMode::Pursue,
        };
        s.npcs[k].disabled = false;
        s.nav_add(Order::Wait(1.0e6)).unwrap();
        s.nav_start().unwrap();
        s.advance(2.0, &ShipInput::default());
        assert!(
            matches!(&s.nav.run, Run::Halted(m) if m.contains("COMBAT")),
            "{:?}",
            s.nav.run
        );
        // At an arrival with hostile ships around: the queue gives the ship back, unless the next
        // order is another jump (it then flees by it).
        let mut t = orbiting(1, 2, 1.5);
        t.spawn_traffic();
        for n in t.npcs.iter_mut() {
            n.hostile = true;
            n.disabled = false;
        }
        t.nav_add(Order::Wait(1.0)).unwrap();
        let mut nav = std::mem::take(&mut t.nav);
        nav.run = Run::Running;
        let mut ev = Vec::new();
        t.nav_arrival(&mut nav, &mut ev);
        assert!(matches!(nav.run, Run::Halted(_)), "{:?}", nav.log);
        nav.run = Run::Running;
        nav.orders.push(Entry {
            order: Order::Jump { star: t.here },
            guard: None,
            status: Status::Pending,
            star: None,
        });
        t.nav_arrival(&mut nav, &mut ev);
        assert_eq!(nav.run, Run::Running, "it flees by the next jump");
    }

    #[test]
    fn the_end_of_a_plan_fires_the_wakes_scheduled_on_arrival() {
        let mut s = orbiting(1, 2, 1.5);
        s.crew.cryo.orders.push(crate::cryo::WakeOrder {
            who: crate::cryo::Who::All,
            when: crate::cryo::When::Arrival,
        });
        s.crew.cryo.orders.push(crate::cryo::WakeOrder {
            who: crate::cryo::Who::All,
            when: crate::cryo::When::At(1.0e12),
        });
        s.nav_add(Order::Wait(5.0)).unwrap();
        s.nav_start().unwrap();
        run(&mut s, 100.0, 1.0);
        assert_eq!(s.nav.run, Run::Idle);
        assert_eq!(s.crew.cryo.orders.len(), 1, "only the dated wake is left");
    }
}
