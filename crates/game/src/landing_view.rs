//! Piloting and landing in the cockpit: the markers of the SAS (prograde, retrograde, normal, the
//! target, the landing site, the point of impact), the artificial horizon, the descent panel
//! (radar altitude, speeds, time to impact, the altitude of the suicide burn), clickable words for
//! the gear, the throttle and the attitude modes, and the link between the ground the cockpit
//! draws and the ground the ship touches (one terrain function serves both).
use crate::game::{Game, Screen};
use crate::hud::{View, AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use crate::terrain::Terrain;
use vector_core::{Mat3, Vec3};
use vector_render::{Camera, Framebuffer};
use vector_sim::landing::{self, Ground, PilotKind, Sas, Site};
use vector_world::BodyKind;
use winit::keyboard::KeyCode;

/// The ground is sampled for the physics under this height (m).
const GROUND_RANGE: f64 = 40_000.0;
/// Baselines (m) over which the slope of the ground is read.
const SLOPE_BASE: f64 = 25.0;
const SLOPE_WIDE: f64 = 150.0;

/// Latitude and longitude of the unit vector `up` (world) on a world turning with `orient`, and
/// the east and north directions there.
fn frame_at(orient: &Mat3, up: Vec3) -> (f64, f64, Vec3, Vec3) {
    let nb = orient.transpose().mul_vec(up);
    let (lat, lon) = (nb.y.clamp(-1.0, 1.0).asin(), nb.z.atan2(nb.x));
    let east = orient.mul_vec(Vec3::new(-lon.sin(), 0.0, lon.cos()));
    let north = orient.mul_vec(Vec3::new(
        -lat.sin() * lon.cos(),
        lat.cos(),
        -lat.sin() * lon.sin(),
    ));
    (lat, lon, east, north)
}

/// Height of the ground above the reference sphere (m): the sea is flat and a ship rests on it.
fn height_at(t: &Terrain, lat: f64, lon: f64, lod: f64) -> f64 {
    f64::from(t.sample(lat, lon, lod).0).max(0.0)
}

/// Height, outward normal (world frame) and roughness (m) of the ground at the point `up` of the
/// world (the same height function the cockpit draws).
pub fn ground_sample(t: &Terrain, orient: &Mat3, up: Vec3) -> (f64, Vec3, f64) {
    let (lat, lon, east, north) = frame_at(orient, up);
    let h0 = height_at(t, lat, lon, 8.0);
    let slope = |base: f64| -> (f64, f64) {
        let dlat = base / t.radius;
        let dlon = base / (t.radius * lat.cos().abs().max(0.05));
        (
            (height_at(t, lat, lon + dlon, base * 0.4) - height_at(t, lat, lon - dlon, base * 0.4))
                / (2.0 * base),
            (height_at(t, lat + dlat, lon, base * 0.4) - height_at(t, lat - dlat, lon, base * 0.4))
                / (2.0 * base),
        )
    };
    let (gx, gy) = slope(SLOPE_BASE);
    let normal = (up - east * gx - north * gy).normalized();
    let rough = (height_at(t, lat, lon, 3.0) - height_at(t, lat, lon, 40.0)).abs();
    (h0, normal, rough)
}

/// Steepness (rad) of the ground at `up` over two baselines, the steeper of the two.
fn steepness(t: &Terrain, orient: &Mat3, up: Vec3) -> f64 {
    let (lat, lon, ..) = frame_at(orient, up);
    let at = |base: f64| -> f64 {
        let dlat = base / t.radius;
        let dlon = base / (t.radius * lat.cos().abs().max(0.05));
        let gx = (height_at(t, lat, lon + dlon, base * 0.4)
            - height_at(t, lat, lon - dlon, base * 0.4))
            / (2.0 * base);
        let gy = (height_at(t, lat + dlat, lon, base * 0.4)
            - height_at(t, lat - dlat, lon, base * 0.4))
            / (2.0 * base);
        gx.hypot(gy).atan()
    };
    at(SLOPE_BASE).max(at(SLOPE_WIDE))
}

impl Game {
    /// Feed the sim the ground under the ship, as the cockpit draws it, while it flies low over a
    /// solid world. Called before every advance of the simulation.
    pub fn update_ground(&mut self) {
        let sim = &self.sim;
        let Some(i) = sim.ref_body.filter(|&i| sim.landable(i)) else {
            self.sim.ground = None;
            return;
        };
        let b = &sim.bodies[i];
        let rel = sim.ship.pos - b.pos;
        if rel.length() - b.radius > GROUND_RANGE.min(b.radius * 0.1) {
            self.sim.ground = None;
            return;
        }
        let Some(map) = self.flight_map(i) else {
            self.sim.ground = None;
            return;
        };
        let t = Terrain::new(&map, b.radius);
        let (height, normal, rough) = ground_sample(&t, &sim.body_orient(i), rel.normalized());
        self.sim.ground = Some(Ground {
            body: i,
            height,
            normal,
            rough,
        });
    }

    /// Mark the flattest zone near the point under the ship as the landing site: a spiral of
    /// candidates, scored by slope (two baselines), roughness and distance.
    pub fn find_site(&mut self) -> Result<String, &'static str> {
        let sim = &self.sim;
        let i = sim
            .ref_body
            .filter(|&i| sim.landable(i))
            .ok_or("aucun monde solide pres du vaisseau")?;
        let b = &sim.bodies[i];
        let rel = sim.ship.pos - b.pos;
        let alt = sim
            .surface_state()
            .map(|s| s.alt)
            .ok_or("trop haut pour lire le terrain")?;
        let map = self.flight_map(i).ok_or("monde non releve")?;
        let t = Terrain::new(&map, b.radius);
        let orient = sim.body_orient(i);
        let up = rel.normalized();
        let (_, _, east, north) = frame_at(&orient, up);
        let reach = (alt * 3.0).clamp(1_500.0, 12_000.0);
        let mut best: Option<(f64, Vec3, f64, f64)> = None;
        for k in 0..60 {
            let r = reach * ((f64::from(k) + 0.5) / 60.0).sqrt();
            let a = f64::from(k) * 2.399_963; // golden angle
            let dir = (up + (east * a.cos() + north * a.sin()) * (r / b.radius)).normalized();
            let (lat, lon, ..) = frame_at(&orient, dir);
            let h = f64::from(t.sample(lat, lon, 8.0).0);
            if h < 1.0 {
                continue; // water, or the sea floor
            }
            let slope = steepness(&t, &orient, dir);
            let (_, _, rough) = ground_sample(&t, &orient, dir);
            let score = slope * 10.0 + rough * 0.2 + r / reach * 0.3;
            if best.is_none_or(|(s, ..)| score < s) {
                best = Some((score, dir, h, slope));
            }
        }
        let (_, dir, height, slope) = best.ok_or("aucun terrain sec a portee")?;
        let site = Site {
            body: i,
            dir: orient.transpose().mul_vec(dir),
            height,
            slope,
        };
        let km = ((dir - up) * b.radius).length() / 1000.0;
        self.sim.set_site(Some(site));
        Ok(format!(
            "SITE MARQUE A {km:.1} KM, PENTE {:.0} DEG",
            slope.to_degrees()
        ))
    }

    /// The world the ATTERRIR command means now: the target if it is a world, else the world the
    /// ship is over.
    pub fn land_target(&self) -> Option<usize> {
        let world = |i: usize| {
            matches!(
                self.sim.bodies[i].kind,
                BodyKind::Planet(_) | BodyKind::Moon(_) | BodyKind::Asteroid(_)
            )
        };
        [self.sim.target, self.sim.ref_body]
            .into_iter()
            .flatten()
            .find(|&i| world(i))
    }

    /// ATTERRIR: from anywhere, the ship plans the approach, comes down and stands on the ground
    /// of `body` (at map cell `cell`, or where it is best). The warnings are shown first; asking
    /// again confirms. Refusals say why.
    pub fn land_request(&mut self, body: usize, cell: Option<(usize, usize)>) {
        let brief = self.sim.land_brief(body, cell);
        if let Some(why) = &brief.refusal {
            self.say(format!("ATTERRISSAGE REFUSE : {why}"));
            self.land_confirm = None;
            return;
        }
        let key = (body, cell);
        if !brief.warnings.is_empty() && self.land_confirm != Some(key) {
            for w in &brief.warnings {
                self.say(w.clone());
            }
            self.say(format!(
                "ATTERRIR SUR {} : {:.0} M/S SUR {:.0} DISPONIBLES ({:.0} % DU RESERVOIR) : REPETEZ POUR CONFIRMER",
                brief.name, brief.dv_need, brief.dv_have, brief.fuel_pct
            ));
            self.land_confirm = Some(key);
            return;
        }
        self.land_confirm = None;
        match self.sim.land_command(body, cell) {
            Ok(b) => {
                self.say(format!(
                    "ATTERRISSAGE SUR {} ENGAGE : {:.0} M/S, {:.0} % DU RESERVOIR (UNE COMMANDE ET VOUS AVEZ LA MAIN)",
                    b.name, b.dv_need, b.fuel_pct
                ));
                self.rear_view = false;
            }
            Err(e) => self.say(format!("ATTERRISSAGE REFUSE : {e}")),
        }
    }

    /// The word ATTERRIR of the menus and the cockpit: land on the target world, or take the
    /// landing back when it is under way.
    pub fn menu_land(&mut self, _: usize) {
        if self.sim.land_goal.is_some() {
            self.sim.cancel_landing();
            self.handle_events();
            return;
        }
        if self.sim.landed_on().is_some() {
            self.say("DEJA AU SOL : DECOLLER OU DEBARQUER");
            return;
        }
        match self.land_target() {
            Some(b) => self.land_request(b, None),
            None => self.say("CIBLEZ UN MONDE (TAB / MAJ+TAB) POUR ATTERRIR"),
        }
    }

    /// ATTERRIR ICI on the cell chosen on the surface map.
    pub fn land_from_surface(&mut self) {
        let Some(map) = self.surf.as_ref() else {
            return;
        };
        let (body, cx, cy) = (map.body, self.surf_cx, self.surf_cy);
        if map.surface.is_sea(cx, cy) || map.surface.is_lake(cx, cy) {
            self.say("PAS DE TERRE FERME ICI : CHOISISSEZ UNE CASE SECHE");
            return;
        }
        self.land_request(body, Some((cx, cy)));
        if self.sim.land_goal.is_some() {
            self.screen = Screen::Flight;
        }
    }

    pub fn menu_takeoff(&mut self, _: usize) {
        match self.sim.engage_pilot(PilotKind::Ascend) {
            Ok(m) => self.say(m),
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    /// DEBARQUER: the squad steps out at the point where the ship stands.
    pub fn menu_disembark(&mut self, _: usize) {
        match self.sim.disembark_here() {
            Ok(()) => self.screen = Screen::Rogue,
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    /// The pilot key: the one command that fits the moment. Landed: take off. Landing under way
    /// or a pilot flying: take the controls back. Over or near a world (or with one targeted):
    /// land on it from wherever the ship is. Near a station: dock.
    pub fn pilot_key(&mut self) {
        if self.sim.pilot.is_some() {
            self.sim.cancel_pilot();
            self.handle_events();
            return;
        }
        if self.sim.land_goal.is_some() {
            self.menu_land(0);
            return;
        }
        if self.sim.landed_on().is_some() {
            self.menu_takeoff(0);
            return;
        }
        if let Some(b) = self.land_target() {
            self.land_request(b, None);
            return;
        }
        if self
            .sim
            .nearest_station()
            .is_some_and(|(_, r)| r < 20_000.0)
        {
            match self.sim.engage_pilot(PilotKind::Dock) {
                Ok(m) => self.say(m),
                Err(e) => self.say(e.to_uppercase()),
            }
            return;
        }
        self.say("AUCUN MONDE CIBLE NI STATION A PORTEE : CIBLEZ UN MONDE (TAB)");
    }

    /// World-to-view rotation of the cockpit camera: ahead, or astern when the pilot looks at the
    /// ground of a ship that lands tail first.
    pub fn view_rot(&self, rot: &Mat3) -> Mat3 {
        if self.rear_view {
            Mat3::look_at(-rot.col(2), rot.col(1)).transpose()
        } else {
            rot.transpose()
        }
    }

    pub fn sas_key(&mut self, mode: Sas) {
        self.sim.set_sas(mode);
        self.say(format!("SAS : {}", mode.label()));
    }

    pub fn landing_gear_key(&mut self) {
        let out = self.sim.toggle_gear();
        self.say(if out {
            "TRAIN D'ATTERRISSAGE SORTI"
        } else {
            "TRAIN D'ATTERRISSAGE RENTRE"
        });
    }

    pub fn throttle_key(&mut self, code: KeyCode) {
        let t = self.sim.throttle;
        let next = match code {
            KeyCode::Numpad8 => t + 0.1,
            KeyCode::Numpad2 => t - 0.1,
            KeyCode::Numpad0 => 0.0,
            _ => 1.0,
        };
        self.sim.set_throttle((next * 10.0).round() / 10.0);
        let pct = (self.sim.throttle * 100.0).round();
        self.say(format!("ACCELERATEUR {pct:.0} %"));
    }

    /// The flight keys of the piloting aids; true when the key was one of them.
    pub fn pilot_aid_key(&mut self, code: KeyCode) -> bool {
        let sas = match code {
            KeyCode::Digit0 => Some(Sas::Off),
            KeyCode::Digit1 => Some(Sas::Hold),
            KeyCode::Digit2 => Some(Sas::Prograde),
            KeyCode::Digit3 => Some(Sas::Retrograde),
            KeyCode::Digit4 => Some(Sas::Normal),
            KeyCode::Digit5 => Some(Sas::AntiNormal),
            KeyCode::Digit6 => Some(Sas::RadialOut),
            KeyCode::Digit7 => Some(Sas::RadialIn),
            KeyCode::Digit8 => Some(Sas::Target),
            KeyCode::Digit9 => Some(Sas::Up),
            _ => None,
        };
        if let Some(m) = sas {
            self.sas_key(m);
            return true;
        }
        match code {
            KeyCode::Numpad3 => self.landing_gear_key(),
            KeyCode::Numpad4 => self.pilot_key(),
            KeyCode::Numpad8 | KeyCode::Numpad2 | KeyCode::Numpad0 | KeyCode::Numpad5 => {
                self.throttle_key(code)
            }
            KeyCode::Numpad7 => match self.find_site() {
                Ok(m) => self.say(m),
                Err(e) => self.say(e.to_uppercase()),
            },
            KeyCode::Numpad1 => {
                self.rear_view = !self.rear_view;
                self.say(if self.rear_view {
                    "VUE ARRIERE (LE SOL QUAND LE NEZ EST EN HAUT)"
                } else {
                    "VUE AVANT"
                });
            }
            KeyCode::Numpad9 => {
                self.sim.set_site(None);
                self.say("SITE D'ATTERRISSAGE EFFACE");
            }
            _ => return false,
        }
        true
    }

    /// What the new events of piloting say (the rest is in `handle_events`).
    pub fn say_pilot_event(&mut self, e: &vector_sim::Event) -> bool {
        use vector_sim::Event as E;
        match e {
            E::Toppled { damage, .. } => {
                self.say(format!(
                    "BASCULE ! LE VAISSEAU EST COUCHE SUR LE FLANC  DEGATS {damage:.0}"
                ));
                self.note(
                    "le vaisseau a bascule a l'atterrissage (pente ou inclinaison trop forte)."
                        .into(),
                );
                self.timescale = 1.0;
            }
            E::PilotDone(s) => {
                self.say(*s);
                self.timescale = 1.0;
            }
            E::PilotFailed(s) => {
                self.say(format!("{s} : LE PILOTE LACHE LES COMMANDES"));
                self.timescale = 1.0;
            }
            E::LandingEnded(m) => {
                self.say(m.clone());
                self.note(m.to_lowercase());
                self.timescale = 1.0;
            }
            E::TooHeavy => self.say("POUSSEE INSUFFISANTE : LE VAISSEAU NE DECOLLE PAS"),
            E::HardDock { damage } => {
                self.say(format!("AMARRAGE BRUTAL  DEGATS {damage:.0}"));
                self.note("amarrage trop rapide ou de travers, la coque a souffert.".into());
            }
            _ => return false,
        }
        true
    }
}

fn word(
    fb: &mut Framebuffer,
    g: &Game,
    col: usize,
    row: usize,
    label: &str,
    key: KeyCode,
    on: bool,
) -> usize {
    let hov = g.hit_cells(col, row, label.len(), 1, Act::Key(key), None);
    fb.text(
        col,
        row,
        label,
        if hov {
            0x00D0_FFD0
        } else if on {
            AMBER
        } else {
            GREEN
        },
    );
    col + label.len() + 2
}

fn ring(fb: &mut Framebuffer, x: f32, y: f32, r: f32, c: u32) {
    fb.circle(x, y, r, c);
}

/// A marker on the sky at the direction `dir`: kind 0 prograde, 1 retrograde, 2 target, 3 normal
/// (up), 4 site, 5 impact.
fn marker(fb: &mut Framebuffer, cam: &Camera, p: Vec3, kind: u8, label: &str, c: u32) {
    let Some((x, y)) = cam.project_world(p, fb) else {
        return;
    };
    if x < 4.0 || y < 4.0 || x > fb.w as f32 - 4.0 || y > fb.h as f32 - 4.0 {
        return;
    }
    match kind {
        0 => {
            ring(fb, x, y, 7.0, c);
            fb.line(x - 12.0, y, x - 7.0, y, c);
            fb.line(x + 7.0, y, x + 12.0, y, c);
            fb.line(x, y - 12.0, x, y - 7.0, c);
        }
        1 => {
            ring(fb, x, y, 7.0, c);
            fb.line(x - 5.0, y - 5.0, x + 5.0, y + 5.0, c);
            fb.line(x - 5.0, y + 5.0, x + 5.0, y - 5.0, c);
        }
        2 => {
            fb.line(x - 7.0, y, x, y - 7.0, c);
            fb.line(x, y - 7.0, x + 7.0, y, c);
            fb.line(x + 7.0, y, x, y + 7.0, c);
            fb.line(x, y + 7.0, x - 7.0, y, c);
        }
        3 => {
            fb.line(x - 6.0, y, x + 6.0, y, c);
            fb.line(x, y - 6.0, x, y + 6.0, c);
            fb.line(x - 3.0, y - 3.0, x, y - 6.0, c);
            fb.line(x + 3.0, y - 3.0, x, y - 6.0, c);
        }
        4 => {
            fb.line(x - 6.0, y - 6.0, x + 6.0, y - 6.0, c);
            fb.line(x + 6.0, y - 6.0, x + 6.0, y + 6.0, c);
            fb.line(x + 6.0, y + 6.0, x - 6.0, y + 6.0, c);
            fb.line(x - 6.0, y + 6.0, x - 6.0, y - 6.0, c);
            fb.line(x, y + 6.0, x, y + 12.0, c);
        }
        _ => {
            ring(fb, x, y, 10.0, c);
            fb.line(x - 6.0, y - 6.0, x + 6.0, y + 6.0, c);
            fb.line(x - 6.0, y + 6.0, x + 6.0, y - 6.0, c);
        }
    }
    if !label.is_empty() {
        fb.text_at(x as i32 + 10, y as i32 - 14, label, c);
    }
}

/// The horizon of the world under the ship: the circle of directions level with the ground.
fn horizon(fb: &mut Framebuffer, cam: &Camera, up: Vec3, c: u32) {
    let a = if up.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    };
    let e = up.cross(a).normalized();
    let n = up.cross(e);
    let mut prev: Option<(f32, f32)> = None;
    for k in 0..=72 {
        let t = f64::from(k) * std::f64::consts::TAU / 72.0;
        let p = cam.project_dir(e * t.cos() + n * t.sin(), fb);
        if let (Some(a), Some(b)) = (prev, p) {
            if (a.0 - b.0).abs() < 400.0 && (a.1 - b.1).abs() < 400.0 {
                fb.line(a.0, a.1, b.0, b.1, c);
            }
        }
        prev = p;
    }
}

pub fn draw(fb: &mut Framebuffer, v: &View) {
    if v.map {
        return;
    }
    let sim = v.sim;
    let surf = sim.surface_state();
    let dock = sim.dock_readout().filter(|_| surf.is_none());
    let aids = sim.sas != Sas::Off || sim.pilot.is_some() || sim.gear_down || sim.throttle < 1.0;
    if surf.is_none() && dock.is_none() && !aids {
        return;
    }
    let (rows, cols) = (fb.rows(), fb.cols());
    let ship = &sim.ship;
    let cam = Camera {
        pos: ship.pos,
        rot: v.game.view_rot(&ship.rot),
        fov_y: if v.boost { 1.4 } else { 1.2 },
    };
    // Markers of the velocity, the target, the up direction and the site.
    let vel = surf.map_or_else(|| sim.rel_velocity(), |s| s.v);
    if vel.length() > 0.5 {
        let d = vel.normalized();
        marker(fb, &cam, ship.pos + d * 1000.0, 0, "PRO", GREEN);
        marker(fb, &cam, ship.pos - d * 1000.0, 1, "RET", AMBER);
    }
    if let Some(t) = sim.target.filter(|&t| t < sim.bodies.len()) {
        marker(fb, &cam, sim.bodies[t].pos, 2, "CIBLE", 0x0040_80FF);
    }
    if let Some(s) = &surf {
        horizon(fb, &cam, s.up, DIM);
        marker(fb, &cam, ship.pos + s.normal * 1000.0, 3, "VERT.", GREEN);
        if let Some(p) = sim
            .site_position()
            .filter(|_| sim.site.is_some_and(|x| x.body == s.body))
        {
            marker(fb, &cam, p, 4, "SITE", AMBER);
        }
        let up = s.up;
        if let Some(t) =
            landing::fall_time(s.alt.max(0.0), s.v.dot(up), s.g).filter(|_| s.v.dot(up) < 0.0)
        {
            let vh = s.v - up * s.v.dot(up);
            marker(
                fb,
                &cam,
                ship.pos + vh * t - up * s.alt.max(0.0),
                5,
                "IMPACT",
                RED,
            );
        }
    }

    // The mode line.
    let pct = (sim.throttle * 100.0).round();
    let mut line = format!(
        "SAS {}  ACCEL {pct:.0}%  TRAIN {}",
        sim.sas.label(),
        if sim.gear_down { "SORTI" } else { "RENTRE" }
    );
    if let Some(p) = sim.pilot {
        line += match p.kind {
            PilotKind::Land => "  PILOTE : ATTERRISSAGE",
            PilotKind::Ascend => "  PILOTE : DECOLLAGE",
            PilotKind::Dock => "  PILOTE : AMARRAGE",
        };
    }
    fb.text(1, 15, &line, if sim.pilot.is_some() { AMBER } else { DIM });
    if rows < 40 {
        return;
    }

    // The panel, low on the left above the gauges.
    let g = v.game;
    let mut y = rows.saturating_sub(24);
    if let Some(r) = sim.landing_readout() {
        fb.text(
            1,
            y,
            &format!(
                "== DESCENTE SUR {} ==",
                sim.bodies[r.body].name.to_uppercase()
            ),
            AMBER,
        );
        y += 1;
        fb.text(
            1,
            y,
            &format!(
                "ALT RADAR {:>7.0} M   V.VERT {:+7.1} M/S   V.HORIZ {:6.1} M/S",
                r.alt, r.v_up, r.v_h
            ),
            if r.v_up < -landing::GEAR_V_IN * 3.0 && r.alt < 300.0 {
                RED
            } else {
                GREEN
            },
        );
        y += 1;
        let impact = r.t_impact.map_or("--".to_string(), |t| format!("{t:.0} S"));
        let burn = match r.burn_alt {
            Some(h) if r.v_up < -1.0 => match r.t_burn {
                Some(t) if t > 0.5 => format!("ALLUMAGE A {h:.0} M (DANS {t:.0} S)"),
                _ => format!("ALLUMAGE A {h:.0} M"),
            },
            None if r.v_up < -1.0 => "POUSSEE < POIDS".to_string(),
            _ => "ALLUMAGE --".to_string(),
        };
        let blink = (sim.time * 3.0) as i64 % 2 == 0;
        fb.text(
            1,
            y,
            &format!(
                "IMPACT DANS {impact}   {burn}   TWR {:.1}   DELTA-V {:.0} M/S",
                r.twr,
                ship.delta_v()
            ),
            if r.burn_now && blink { RED } else { GREEN },
        );
        if r.burn_now {
            fb.text(
                1,
                y + 1,
                ">>> ALLUMER LES MOTEURS <<<",
                if blink { RED } else { AMBER },
            );
        }
        y += 2;
        fb.text(
            1,
            y,
            &format!(
                "PENTE {:.0} DEG   NEZ/NORMALE {:.0} DEG   RUGOSITE {:.1} M   {}",
                r.slope.to_degrees(),
                r.tilt.to_degrees(),
                r.rough,
                if r.v_up >= 0.0 {
                    "EN VOL"
                } else if r.safe {
                    "CONTACT SUR"
                } else {
                    "CONTACT DANGEREUX"
                }
            ),
            if r.safe || r.v_up > 0.0 { GREEN } else { AMBER },
        );
        y += 1;
        if let Some(w) = r.weather {
            fb.text(
                1,
                y,
                &format!(
                    "METEO {}   VENT {:.0} M/S   VISIBILITE {:.0} %   ADHERENCE {:.0} %",
                    w.weather.label().to_uppercase(),
                    w.wind,
                    w.visibility * 100.0,
                    w.grip * 100.0
                ),
                if w.rough() { RED } else { GREEN },
            );
            y += 1;
        }
        if let Some((d, _)) = r.site {
            fb.text(
                1,
                y,
                &format!(
                    "SITE A {:.0} M   PENTE DU SITE {:.0} DEG",
                    d,
                    sim.site.map_or(0.0, |s| s.slope.to_degrees())
                ),
                AMBER,
            );
            y += 1;
        }
    } else if let Some(d) = dock {
        fb.text(
            1,
            y,
            &format!("== AMARRAGE {} ==", sim.bodies[d.body].name.to_uppercase()),
            AMBER,
        );
        y += 1;
        fb.text(
            1,
            y,
            &format!(
                "DIST {:.0} M   APPROCHE {:+.1} M/S   LATERAL {:.1} M/S",
                d.range, d.closing, d.lateral_v
            ),
            GREEN,
        );
        y += 1;
        fb.text(
            1,
            y,
            &format!(
                "AXE {:.0} DEG   DECALAGE {:.0} M   {}",
                d.align.to_degrees(),
                d.offset,
                if d.clean {
                    "PRISE PROPRE"
                } else {
                    "TROP RAPIDE OU DE TRAVERS"
                }
            ),
            if d.clean { GREEN } else { AMBER },
        );
        y += 1;
    }
    y += 1;
    // Clickable words.
    let mut x = 1;
    x = word(fb, g, x, y, "[PAV3] TRAIN", KeyCode::Numpad3, sim.gear_down);
    x = word(
        fb,
        g,
        x,
        y,
        "[PAV4] PILOTE",
        KeyCode::Numpad4,
        sim.pilot.is_some(),
    );
    x = word(
        fb,
        g,
        x,
        y,
        "[PAV7] SITE PLAT",
        KeyCode::Numpad7,
        sim.site.is_some(),
    );
    x = word(fb, g, x, y, "[PAV8/2] ACCEL +/-", KeyCode::Numpad8, false);
    x = word(fb, g, x, y, "[PAV0] COUPURE", KeyCode::Numpad0, false);
    x = word(fb, g, x, y, "[PAV1] VUE SOL", KeyCode::Numpad1, g.rear_view);
    word(fb, g, x, y, "[PAV5] PLEIN", KeyCode::Numpad5, false);
    y += 1;
    let mut x = 1;
    for (label, key, mode) in [
        ("[0] SAS OFF", KeyCode::Digit0, Sas::Off),
        ("[1] CAP", KeyCode::Digit1, Sas::Hold),
        ("[2] PRO", KeyCode::Digit2, Sas::Prograde),
        ("[3] RET", KeyCode::Digit3, Sas::Retrograde),
        ("[4] NOR", KeyCode::Digit4, Sas::Normal),
        ("[5] ANTI", KeyCode::Digit5, Sas::AntiNormal),
        ("[6] RAD+", KeyCode::Digit6, Sas::RadialOut),
        ("[7] RAD-", KeyCode::Digit7, Sas::RadialIn),
        ("[8] CIBLE", KeyCode::Digit8, Sas::Target),
        ("[9] VERT", KeyCode::Digit9, Sas::Up),
    ] {
        x = word(fb, g, x, y, label, key, sim.sas == mode);
    }
    let _ = cols;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vector_data::GameData;

    fn game() -> Game {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let mut g = Game::new(Arc::new(GameData::load(&dir).expect("data")), 1);
        g.key(KeyCode::Enter, None, true);
        g
    }

    /// The ship put `alt` metres over the ground of the largest solid world, at rest on it.
    fn over_ground(g: &mut Game, alt: f64) -> usize {
        g.sim.reveal_all();
        let i = (0..g.sim.bodies.len())
            .filter(|&i| g.sim.landable(i))
            .max_by(|&a, &b| g.sim.bodies[a].radius.total_cmp(&g.sim.bodies[b].radius))
            .expect("a solid world");
        let b = g.sim.bodies[i].clone();
        let up = Vec3::new(0.3, 0.8, 0.5).normalized();
        g.sim.ship.pos = b.pos + up * (b.radius + 15.0 + alt);
        g.sim.ship.vel = b.vel + g.sim.surface_velocity(i, g.sim.ship.pos - b.pos);
        g.sim.ship.rot = Mat3::look_at(up, Vec3::new(0.0, 0.0, 1.0));
        g.sim.ship.assist = false;
        g.sim.gear_down = true;
        g.sim.update_ref_pub();
        g.update_ground();
        i
    }

    #[test]
    fn the_piloting_keys_drive_the_aids() {
        let mut g = game();
        g.key(KeyCode::Digit3, None, true);
        assert_eq!(g.sim.sas, Sas::Retrograde);
        g.key(KeyCode::Digit0, None, true);
        assert_eq!(g.sim.sas, Sas::Off);
        let gear = g.sim.gear_down;
        g.key(KeyCode::Numpad3, None, true);
        assert_ne!(g.sim.gear_down, gear);
        g.key(KeyCode::Numpad2, None, true);
        g.key(KeyCode::Numpad2, None, true);
        assert!((g.sim.throttle - 0.8).abs() < 1e-9);
        g.key(KeyCode::Numpad0, None, true);
        assert_eq!(g.sim.throttle, 0.0);
        g.key(KeyCode::Numpad5, None, true);
        assert_eq!(g.sim.throttle, 1.0);
    }

    #[test]
    fn the_ship_rests_on_the_ground_the_cockpit_draws() {
        let mut g = game();
        let i = over_ground(&mut g, 3.0);
        assert!(g.sim.ground.is_some(), "ground under the ship");
        let up = (g.sim.ship.pos - g.sim.bodies[i].pos).normalized();
        g.sim.ship.vel = g.sim.ship.vel - up * 2.0;
        for _ in 0..300 {
            g.advance(1.0 / 30.0);
        }
        assert_eq!(g.sim.landed_on(), Some(i), "landed");
        // The height of the terrain at the point where it stands, sampled independently.
        let b = &g.sim.bodies[i];
        let rel = g.sim.ship.pos - b.pos;
        let map = g.flight_map(i).expect("map");
        let t = Terrain::new(&map, b.radius);
        let (lat, lon, ..) = frame_at(&g.sim.body_orient(i), rel.normalized());
        let h = f64::from(t.sample(lat, lon, 8.0).0).max(0.0);
        assert!(
            (rel.length() - (b.radius + h + 15.0)).abs() < 1.5,
            "stands on the terrain: {} vs {}",
            rel.length() - b.radius - 15.0,
            h
        );
    }

    #[test]
    fn the_marked_site_is_a_landable_slope() {
        let mut g = game();
        let i = over_ground(&mut g, 300.0);
        let msg = g.find_site().expect("a site");
        assert!(msg.starts_with("SITE MARQUE"), "{msg}");
        let site = g.sim.site.expect("site set");
        assert!(site.slope < landing::SLOPE_MAX, "slope {}", site.slope);
        assert_eq!(site.body, i);
        // It turns with the world: the same body-fixed point is somewhere else later.
        let p0 = g.sim.site_position().expect("position");
        g.sim.time += 3600.0;
        let p1 = g.sim.site_position().expect("position");
        assert!((p1 - p0).length() > 1.0);
    }

    #[test]
    fn the_land_command_from_the_menu_puts_the_ship_on_the_ground() {
        let mut g = game();
        g.sim.reveal_all();
        let i = (0..g.sim.bodies.len())
            .filter(|&i| g.sim.landable(i) && g.sim.system.bodies[i].pressure < 0.05)
            .find(|&i| g.sim.bodies[i].radius > 4.0e5 && g.sim.system.bodies[i].orbit.is_some())
            .expect("a world");
        let b = g.sim.bodies[i].clone();
        let r = b.radius * 1.04;
        g.sim.ship.pos = b.pos + Vec3::new(r, 0.0, 0.0);
        g.sim.ship.vel = b.vel + Vec3::new(0.0, 0.0, (b.gm / r).sqrt());
        g.sim.update_ref_pub();
        g.tut_step = None;
        // Nothing targeted and nothing under the ship but a world: the command names it.
        g.sim.target = Some(i);
        g.key(KeyCode::Numpad4, None, true);
        if g.sim.land_goal.is_none() {
            // The warnings came first: asking again confirms.
            g.key(KeyCode::Numpad4, None, true);
        }
        assert!(g.sim.land_goal.is_some(), "{:?}", g.messages);
        for _ in 0..4000 {
            g.advance(1.0);
            if g.sim.land_goal.is_none() {
                break;
            }
        }
        assert_eq!(g.sim.landed_on(), Some(i), "{:?}", g.messages);
        assert!(
            g.messages.iter().any(|m| m.starts_with("POSE SUR")),
            "{:?}",
            g.messages
        );
        assert!(g
            .sim
            .ground_status()
            .is_some_and(|s| s.starts_with("AU SOL")));
        // On the ground the same key takes off, and the menu offers to step out.
        g.key(KeyCode::Numpad4, None, true);
        assert!(g.sim.pilot.is_some(), "the ascent pilot flies");
    }

    #[test]
    fn a_landing_on_a_giant_or_a_station_is_refused_in_words() {
        let mut g = game();
        let st = g
            .sim
            .bodies
            .iter()
            .position(|b| b.kind == BodyKind::Station)
            .expect("station");
        g.sim.docked = None;
        g.land_request(st, None);
        assert!(
            g.messages.iter().any(|m| m.contains("REFUSE")),
            "{:?}",
            g.messages
        );
        assert!(g.sim.land_goal.is_none());
    }
}
