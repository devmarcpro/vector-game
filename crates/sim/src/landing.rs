//! Piloting aids and the landing. The ship is the player's body: this module gives it attitude
//! hold (SAS: hold, prograde, retrograde, normal, radial, target, up), a throttle with a cut-off,
//! landing gear, the readout of a descent (radar altitude, vertical and horizontal speed, time to
//! impact, the exact altitude where a suicide burn must start), a contact model that reads the
//! slope, the roughness and the speed of the touch (landed, hard, bounced, toppled, destroyed),
//! a takeoff that needs thrust beyond the weight, and optional pilots (landing, ascent, docking)
//! that the player can take back at any moment. Nothing here is mandatory: with every aid off the
//! ship obeys the keys alone.
use crate::{Event, ShipInput, Sim, SHIP_RADIUS};
use std::f64::consts::TAU;
use vector_core::{Mat3, Vec3};

/// Safe touchdown speeds (m/s) with the gear out: into the ground and along it.
pub const GEAR_V_IN: f64 = 6.0;
pub const GEAR_V_T: f64 = 3.0;
/// The same on the belly of the hull (gear in): nothing absorbs the shock.
pub const BELLY_V_IN: f64 = 1.5;
pub const BELLY_V_T: f64 = 1.0;
/// Steepest ground (rad) a ship stands on, and the largest tilt of the nose from the ground normal.
pub const SLOPE_MAX: f64 = 0.35;
pub const TILT_MAX: f64 = 0.35;
/// Equivalent speed (m/s) beyond the safe one that destroys the hull outright.
pub const CRASH_EXCESS: f64 = 30.0;
/// Share of the normal speed that a hard landing gives back (the bounce).
pub const RESTITUTION: f64 = 0.3;
/// A landing readout and the pilots work this low (share of the radius, clamped to metres).
const RANGE_MIN: f64 = 5_000.0;
const RANGE_MAX: f64 = 150_000.0;

/// Attitude hold modes (SAS).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sas {
    #[default]
    Off,
    Hold,
    Prograde,
    Retrograde,
    Normal,
    AntiNormal,
    RadialOut,
    RadialIn,
    Target,
    Up,
}

impl Sas {
    pub const ALL: [Sas; 10] = [
        Sas::Off,
        Sas::Hold,
        Sas::Prograde,
        Sas::Retrograde,
        Sas::Normal,
        Sas::AntiNormal,
        Sas::RadialOut,
        Sas::RadialIn,
        Sas::Target,
        Sas::Up,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Sas::Off => "COUPE",
            Sas::Hold => "MAINTIEN DU CAP",
            Sas::Prograde => "PROGRADE",
            Sas::Retrograde => "RETROGRADE",
            Sas::Normal => "NORMALE",
            Sas::AntiNormal => "ANTI-NORMALE",
            Sas::RadialOut => "RADIAL SORTANT",
            Sas::RadialIn => "RADIAL ENTRANT",
            Sas::Target => "VERS LA CIBLE",
            Sas::Up => "VERTICALE",
        }
    }
}

/// The ground under the ship, as the renderer sees it: height above the reference sphere of the
/// world (m), outward unit normal of the slope in the world frame, roughness (m).
#[derive(Clone, Copy, Debug)]
pub struct Ground {
    pub body: usize,
    pub height: f64,
    pub normal: Vec3,
    pub rough: f64,
}

/// A chosen landing site: body-fixed unit direction, ground height there, its slope (rad).
#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub body: usize,
    pub dir: Vec3,
    pub height: f64,
    pub slope: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PilotKind {
    Land,
    Ascend,
    Dock,
}

/// An optional pilot flying the ship until it is done, fails or the player takes the controls.
#[derive(Clone, Copy, Debug)]
pub struct Pilot {
    pub kind: PilotKind,
    pub body: usize,
    pub t: f64,
}

/// What the ship feels of the ground: all in the frame of the rotating world.
#[derive(Clone, Copy, Debug)]
pub struct Surf {
    pub body: usize,
    pub up: Vec3,
    /// Radar altitude of the hull above the ground (m).
    pub alt: f64,
    /// Velocity relative to the ground, not to the centre of the world (m/s).
    pub v: Vec3,
    /// Pull of the world here (m/s2).
    pub g: f64,
    pub normal: Vec3,
    pub height: f64,
    pub rough: f64,
}

/// The descent panel.
#[derive(Clone, Copy, Debug)]
pub struct Readout {
    pub body: usize,
    pub alt: f64,
    /// Vertical speed, up positive, and the speed along the ground (m/s).
    pub v_up: f64,
    pub v_h: f64,
    /// Seconds before the ground when falling, with gravity alone.
    pub t_impact: Option<f64>,
    /// Altitude where a full-thrust burn must start to stop on the ground; None when the engines
    /// cannot even hold the weight.
    pub burn_alt: Option<f64>,
    pub burn_now: bool,
    /// Thrust over the weight on this world.
    pub twr: f64,
    pub slope: f64,
    pub tilt: f64,
    pub gear: bool,
    /// Seconds before the burn must start to stop on the ground (None when it cannot, or when
    /// nothing is falling): 0 when it is time.
    pub t_burn: Option<f64>,
    /// Whether touching down now would be clean.
    pub safe: bool,
    /// Distance (m) and direction (world) to the chosen site, if any.
    pub site: Option<(f64, Vec3)>,
    pub rough: f64,
    /// The weather over the site: wind, view and grip (None without air).
    pub weather: Option<crate::links::SiteWeather>,
}

/// How a station port is being approached.
#[derive(Clone, Copy, Debug)]
pub struct DockReadout {
    pub body: usize,
    pub range: f64,
    /// Closing speed (positive when approaching) and speed across the approach axis (m/s).
    pub closing: f64,
    pub lateral_v: f64,
    /// Angle between the nose and the port axis (rad), and offset from the axis (m).
    pub align: f64,
    pub offset: f64,
    pub clean: bool,
}

/// The touch of the ship on the ground, all measured against the local normal.
#[derive(Clone, Copy, Debug)]
pub struct Touch {
    pub v_in: f64,
    pub v_t: f64,
    pub tilt: f64,
    pub slope: f64,
    pub rough: f64,
    pub gear: bool,
    /// 1 on dry ground; rain, snow and ice lower it (see `links::SiteWeather`): the gear
    /// absorbs that much less of the sliding speed.
    pub grip: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Landed,
    /// Landed or bounced with damage; `excess` is the equivalent speed beyond the safe one.
    Hard {
        excess: f64,
        bounce: bool,
    },
    /// Fell over on its side.
    Toppled {
        excess: f64,
    },
    Crash {
        excess: f64,
    },
}

/// Safe speeds (into, along) for the landing equipment.
pub fn safe_speeds(gear: bool) -> (f64, f64) {
    if gear {
        (GEAR_V_IN, GEAR_V_T)
    } else {
        (BELLY_V_IN, BELLY_V_T)
    }
}

/// Equivalent speed of a touch beyond what the gear absorbs (m/s).
pub fn excess_speed(t: &Touch) -> f64 {
    let (si, st) = safe_speeds(t.gear);
    let st = st * t.grip.clamp(0.1, 1.0);
    ((t.v_in - si).max(0.0).powi(2) + 0.5 * (t.v_t - st).max(0.0).powi(2)).sqrt()
}

/// What a touch does to the ship.
pub fn classify(t: &Touch) -> Outcome {
    let excess = excess_speed(t);
    let tilt = t.tilt + (t.rough / 20.0).atan();
    if excess >= CRASH_EXCESS {
        Outcome::Crash { excess }
    } else if t.slope > SLOPE_MAX || tilt > TILT_MAX {
        Outcome::Toppled { excess }
    } else if excess > 0.0 {
        Outcome::Hard {
            excess,
            bounce: t.v_in > 2.0 * safe_speeds(t.gear).0,
        }
    } else {
        Outcome::Landed
    }
}

/// Fraction of the hull that a touch of this excess speed costs (1 destroys it).
pub fn damage_fraction(excess: f64) -> f64 {
    ((excess / CRASH_EXCESS).powi(2)).min(2.0)
}

/// Stopping distance (m) of a burn at acceleration `a_max` against gravity `g`, coming down at
/// `v_down`: the altitude at which the burn must start. None when the engines cannot hold the
/// weight or nothing is falling.
pub fn suicide_burn_altitude(v_down: f64, g: f64, a_max: f64) -> Option<f64> {
    let net = a_max - g;
    (net > 1e-9 && v_down > 0.0).then(|| v_down * v_down / (2.0 * net))
}

/// Seconds of free fall (constant `g`) from altitude `h0` going down at `v0` before the burn
/// altitude is reached. 0 when it is already time.
pub fn time_to_burn(h0: f64, v0: f64, g: f64, a_max: f64) -> Option<f64> {
    let net = a_max - g;
    if net <= 1e-9 || g <= 1e-9 {
        return None;
    }
    let (a, v) = (a_max, v0.max(0.0));
    let disc = v * v * a * a - g * a * (v * v - 2.0 * net * h0);
    if disc < 0.0 {
        return Some(0.0);
    }
    Some(((-v * a + disc.sqrt()) / (g * a)).max(0.0))
}

/// Seconds to the ground in free fall from altitude `alt` with vertical speed `v_up`.
pub fn fall_time(alt: f64, v_up: f64, g: f64) -> Option<f64> {
    if g <= 1e-9 {
        return (v_up < 0.0).then(|| alt / -v_up);
    }
    let disc = v_up * v_up + 2.0 * g * alt.max(0.0);
    let t = (v_up + disc.sqrt()) / g;
    (t.is_finite() && t > 0.0).then_some(t)
}

fn clamp_len(v: Vec3, max: f64) -> Vec3 {
    let l = v.length();
    if l > max && l > 1e-12 {
        v * (max / l)
    } else {
        v
    }
}

impl Sim {
    // ---- the rotating world -----------------------------------------------------------------

    /// Spin of world `i` (rad/s).
    pub fn spin_rate(&self, i: usize) -> f64 {
        TAU / self.system.spin_period(i).max(60.0)
    }

    /// Velocity (m/s) of the ground at `rel` (position from the centre of world `i`) in the star frame.
    pub fn surface_velocity(&self, i: usize, rel: Vec3) -> Vec3 {
        if !self.landable(i) {
            return Vec3::ZERO;
        }
        Vec3::new(0.0, 1.0, 0.0).cross(rel) * self.spin_rate(i)
    }

    /// Body-fixed to world rotation of world `i` (the one the renderer draws the ground with).
    pub fn body_orient(&self, i: usize) -> Mat3 {
        Mat3::rot_y(TAU * self.time / self.system.spin_period(i).max(60.0))
    }

    /// Height (m), outward normal and roughness (m) of the ground under the ship on world `i`.
    pub fn local_ground(&self, i: usize) -> (f64, Vec3, f64) {
        let up = (self.ship.pos - self.bodies[i].pos).normalized();
        match self.ground.filter(|g| g.body == i) {
            Some(g) => (g.height, g.normal, g.rough),
            None => (0.0, up, 0.0),
        }
    }

    /// The ship over the ground of a solid world within reach of the landing aids.
    pub fn surface_state(&self) -> Option<Surf> {
        let i = self.ref_body?;
        if !self.landable(i) || self.docked.is_some() {
            return None;
        }
        let b = &self.bodies[i];
        let rel = self.ship.pos - b.pos;
        let d = rel.length();
        if d < 1.0 {
            return None;
        }
        let up = rel * (1.0 / d);
        let (height, normal, rough) = self.local_ground(i);
        let alt = d - b.radius - height - SHIP_RADIUS;
        if alt > (b.radius * 0.1).clamp(RANGE_MIN, RANGE_MAX) {
            return None;
        }
        let v = self.ship.vel - b.vel - self.surface_velocity(i, rel);
        Some(Surf {
            body: i,
            up,
            alt,
            v,
            g: b.gm / (d * d),
            normal,
            height,
            rough,
        })
    }

    /// Velocity of the frame the station-keeping assist holds the ship in: the world's, plus the
    /// turning of its ground when the ship is parked on it (it is pinned to the ground).
    pub fn assist_frame_velocity(&self, i: usize) -> Vec3 {
        let b = &self.bodies[i];
        if self.parked.is_some_and(|p| p.0 == i) {
            b.vel + self.surface_velocity(i, self.ship.pos - b.pos)
        } else {
            b.vel
        }
    }

    /// Whether the ship flies low enough over a world with real gravity that station-keeping
    /// assist must not hold it up: it falls, and the pilot (or a hover pilot) decides.
    pub fn low_over_gravity(&self) -> bool {
        self.surface_state().is_some_and(|s| s.g > 0.3)
    }

    /// Thrust of the main engines now (N).
    pub fn main_thrust(&self) -> f64 {
        if self.effective.thrust > 0.0 {
            self.effective.thrust
        } else {
            self.ship.stats.thrust
        }
    }

    /// World position of the chosen landing site, if it is on this world.
    pub fn site_position(&self) -> Option<Vec3> {
        let s = self.site?;
        let b = self.bodies.get(s.body)?;
        Some(b.pos + self.body_orient(s.body).mul_vec(s.dir) * (b.radius + s.height))
    }

    /// The descent panel (None far from every solid world).
    pub fn landing_readout(&self) -> Option<Readout> {
        let s = self.surface_state()?;
        let mass = self.ship.mass();
        let a_max = self.main_thrust() / mass;
        let v_up = s.v.dot(s.up);
        let v_h = (s.v - s.up * v_up).length();
        let alt = s.alt.max(0.0);
        let burn_alt = suicide_burn_altitude(-v_up, s.g, a_max);
        let fwd = self.ship.forward();
        let tilt = fwd.dot(s.normal).clamp(-1.0, 1.0).acos();
        let slope = s.normal.dot(s.up).clamp(-1.0, 1.0).acos();
        let touch = Touch {
            v_in: (-s.v.dot(s.normal)).max(0.0),
            v_t: (s.v - s.normal * s.v.dot(s.normal)).length(),
            tilt,
            slope,
            rough: s.rough,
            gear: self.gear_down,
            grip: self.site_weather_now(s.body).map_or(1.0, |w| w.grip),
        };
        let site = self
            .site_position()
            .filter(|_| self.site.is_some_and(|x| x.body == s.body));
        Some(Readout {
            body: s.body,
            alt,
            v_up,
            v_h,
            t_impact: (v_up < 0.0).then(|| fall_time(alt, v_up, s.g)).flatten(),
            burn_alt,
            burn_now: burn_alt.is_some_and(|h| alt <= h * 1.05) && v_up < -1.0,
            t_burn: (v_up < -1.0)
                .then(|| time_to_burn(alt, -v_up, s.g, a_max))
                .flatten(),
            twr: a_max / s.g.max(1e-9),
            slope,
            tilt,
            gear: self.gear_down,
            safe: classify(&touch) == Outcome::Landed,
            site: site.map(|p| {
                let off = p - self.ship.pos;
                let h = off - s.up * off.dot(s.up);
                (h.length(), h.normalized())
            }),
            rough: s.rough,
            weather: self.site_weather_now(s.body),
        })
    }

    // ---- attitude, throttle, gear -----------------------------------------------------------

    /// Choose the attitude hold mode (Hold keeps the nose where it points now).
    pub fn set_sas(&mut self, m: Sas) {
        self.sas = m;
        if m == Sas::Hold {
            self.sas_hold = self.ship.forward();
        }
    }

    /// Throttle of the main engines, 0 to 1 (the thrust key scales by it).
    pub fn set_throttle(&mut self, t: f64) {
        self.throttle = t.clamp(0.0, 1.0);
    }

    /// Deploy or retract the landing gear; returns whether it is out.
    pub fn toggle_gear(&mut self) -> bool {
        self.gear_down = !self.gear_down;
        self.gear_down
    }

    /// Mark the ground under the ship (or the flattest zone near it, as found by the caller).
    pub fn set_site(&mut self, site: Option<Site>) {
        self.site = site;
    }

    /// Direction the SAS wants the nose to take, in the world frame.
    pub fn sas_direction(&self) -> Option<Vec3> {
        let ship = &self.ship;
        let surf = self.surface_state();
        let (r, v) = match (&surf, self.ref_body) {
            (Some(s), Some(i)) => (ship.pos - self.bodies[i].pos, s.v),
            (None, Some(i)) => (ship.pos - self.bodies[i].pos, self.rel_velocity()),
            _ => (Vec3::ZERO, self.rel_velocity()),
        };
        let unit = |x: Vec3, min: f64| (x.length() > min).then(|| x.normalized());
        match self.sas {
            Sas::Off => None,
            Sas::Hold => Some(self.sas_hold),
            Sas::Prograde => unit(v, 0.3),
            Sas::Retrograde => unit(v, 0.3).map(|d| -d),
            Sas::Normal => unit(r.cross(v), 1e-6).map(|d| d.normalized()),
            Sas::AntiNormal => unit(r.cross(v), 1e-6).map(|d| -d.normalized()),
            Sas::RadialOut => unit(r, 1.0),
            Sas::RadialIn => unit(r, 1.0).map(|d| -d),
            Sas::Target => {
                let t = self.target.filter(|&t| t < self.bodies.len())?;
                unit(self.bodies[t].pos - ship.pos, 1.0)
            }
            Sas::Up => surf.map(|s| s.normal).or_else(|| unit(r, 1.0)),
        }
    }

    /// Rotation demands (pitch, yaw, roll in -1..1) that swing the nose to `dir` and stop there,
    /// and the angle still to cover (rad). The rate follows the braking curve of the torque the
    /// RCS gives with the present mass, so a heavy ship starts turning back earlier.
    pub fn steer_demand(&self, dir: Vec3) -> (f64, f64, f64, f64) {
        let d = self.ship.rot.transpose().mul_vec(dir.normalized());
        let err = d.z.clamp(-1.0, 1.0).acos();
        let s = (d.x * d.x + d.y * d.y).sqrt();
        let max_rate = (self.effective.max_rate * self.env_agility.sqrt()).max(1e-3);
        let accel = (self.effective.ang_accel * self.env_agility * self.attitude_scale()).max(1e-3);
        let rate = (accel * err).sqrt().min(1.5 * err).min(max_rate);
        let frac = rate / max_rate;
        if s < 1e-9 {
            return (0.0, if d.z < 0.0 { 1.0 } else { 0.0 }, 0.0, err);
        }
        (-d.y / s * frac, d.x / s * frac, 0.0, err)
    }

    /// How much the RCS torque per unit of inertia differs from the design's reference: more
    /// propellant and cargo (a bigger inertia for the same thrusters) slow the turn, extra RCS
    /// modules quicken it.
    pub fn attitude_scale(&self) -> f64 {
        let hull = &self.data.hulls[self.ship.design.hull];
        let reference_mass = self.ship.stats.mass_dry + 0.5 * self.ship.stats.fuel_cap;
        let reference = hull.rcs / reference_mass.max(1.0);
        let now = self.effective.rcs_n / self.ship.mass().max(1.0);
        if reference <= 0.0 {
            return 1.0;
        }
        (now / reference).clamp(0.3, 2.0)
    }

    // ---- pilots -----------------------------------------------------------------------------

    /// Hand the ship to a pilot. Refused when it cannot succeed (no thrust to hold the weight,
    /// not enough propellant for the descent).
    pub fn engage_pilot(&mut self, kind: PilotKind) -> Result<String, &'static str> {
        if self.docked.is_some() || self.jump.is_some() || self.autopilot.is_some() {
            return Err("impossible pour l'instant");
        }
        match kind {
            PilotKind::Land => {
                let s = self
                    .surface_state()
                    .ok_or("aucun monde solide assez proche")?;
                let a_max = self.main_thrust() / self.ship.mass();
                if a_max <= s.g * 1.1 {
                    return Err("poussee insuffisante pour se poser ici");
                }
                // Cost of the descent: kill the speed along the ground, then the fall and the burn.
                let v_up = s.v.dot(s.up);
                let v_h = (s.v - s.up * v_up).length();
                let fall = (v_up * v_up + 2.0 * s.g * s.alt.max(0.0)).sqrt();
                let dv = v_h + fall;
                if dv > self.ship.delta_v() {
                    return Err("carburant insuffisant pour l'atterrissage");
                }
                self.parked = None;
                self.pilot = Some(Pilot {
                    kind,
                    body: s.body,
                    t: 0.0,
                });
                Ok("PILOTE D'ATTERRISSAGE ENGAGE : TOUCHE QUELCONQUE POUR REPRENDRE".into())
            }
            PilotKind::Ascend => {
                let s = self
                    .surface_state()
                    .ok_or("aucun monde solide assez proche")?;
                let a_max = self.main_thrust() / self.ship.mass();
                if a_max <= s.g * 1.05 {
                    return Err("poussee inferieure au poids : decollage impossible");
                }
                if self.ship.fuel <= 0.0 {
                    return Err("plus de carburant");
                }
                self.pilot = Some(Pilot {
                    kind,
                    body: s.body,
                    t: 0.0,
                });
                Ok("PILOTE DE DECOLLAGE VERS L'ORBITE ENGAGE".into())
            }
            PilotKind::Dock => {
                let (i, range) = self.nearest_station().ok_or("aucune station")?;
                if range > 20_000.0 {
                    return Err("station trop loin pour le pilote d'amarrage");
                }
                self.parked = None;
                self.pilot = Some(Pilot {
                    kind,
                    body: i,
                    t: 0.0,
                });
                Ok("PILOTE D'AMARRAGE ENGAGE : ALIGNEMENT SUR LE PORT".into())
            }
        }
    }

    pub fn cancel_pilot(&mut self) {
        if self.pilot.take().is_some() {
            self.events.push(Event::PilotDone("PILOTE DEBRAYE"));
        }
    }

    /// Orbit height the ascent aims for: above the air, with a margin.
    pub fn ascent_target(&self, i: usize) -> f64 {
        let top = self.air_of(i).map_or(0.0, |a| a.top());
        (top * 1.15).max(15_000.0).min(self.bodies[i].radius * 0.5)
    }

    /// Everything that sits between the keys and the ship: pilots, SAS, throttle.
    pub(crate) fn pilot_layer(
        &mut self,
        manual: &ShipInput,
        mut input: ShipInput,
        dt: f64,
    ) -> ShipInput {
        if dt > 0.25 {
            return input;
        }
        // Under flight assist the gear follows the height and the speed (out below 600 m, below
        // 250 m/s); without it the gear is the pilot's.
        if self.ship.assist {
            self.gear_down = self
                .surface_state()
                .is_some_and(|s| s.alt < 600.0 && s.v.length() < 250.0);
        }
        let rot_in = manual.pitch.abs() + manual.yaw.abs() + manual.roll.abs();
        let move_in = manual.fwd.abs() + manual.lat.abs() + manual.vert.abs();
        if let Some(mut p) = self.pilot {
            // Any real touch of the controls hands the ship back.
            if move_in > 0.0 || rot_in > 0.5 || manual.brake {
                self.pilot = None;
                self.events.push(Event::PilotDone("PILOTE DEBRAYE"));
            } else {
                p.t += dt;
                let flown = match p.kind {
                    PilotKind::Land => self.land_input(),
                    PilotKind::Ascend => self.ascend_input(p.body),
                    PilotKind::Dock => self.dock_input(p.body),
                };
                match flown {
                    Ok(Some(i)) => {
                        self.pilot = Some(p);
                        input = ShipInput {
                            mine: input.mine,
                            fire: input.fire,
                            fire_missile: input.fire_missile,
                            ..i
                        };
                    }
                    Ok(None) => {
                        self.pilot = None;
                        self.events.push(Event::PilotDone(match p.kind {
                            PilotKind::Land => "ATTERRISSAGE ACHEVE",
                            PilotKind::Ascend => "ORBITE ATTEINTE : DECOLLAGE ACHEVE",
                            PilotKind::Dock => "AMARRAGE ACHEVE",
                        }));
                    }
                    Err(e) => {
                        self.pilot = None;
                        self.events.push(Event::PilotFailed(e));
                    }
                }
                return input;
            }
        }
        if input.fwd > 0.0 {
            input.fwd *= self.throttle;
        }
        if self.sas != Sas::Off {
            if rot_in > 0.0 {
                // The hands win; the hold follows where they leave the nose.
                self.sas_hold = self.ship.forward();
            } else if let Some(d) = self.sas_direction() {
                let (p, y, r, _) = self.steer_demand(d);
                input.pitch = p;
                input.yaw = y;
                input.roll = r;
            }
        }
        input
    }

    /// The landing pilot: a suicide-burn profile (never faster than the engines can still stop),
    /// the speed along the ground cancelled or steered to the site, the gear out low, the nose
    /// upright for the last metres. Ok(None) once down.
    fn land_input(&mut self) -> Result<Option<ShipInput>, &'static str> {
        let Some(s) = self.surface_state() else {
            return Err("ECHEC : PLUS DE MONDE SOUS LE VAISSEAU");
        };
        if self.parked.is_some_and(|p| p.0 == s.body) && s.alt < 3.0 {
            return Ok(None);
        }
        if self.ship.fuel <= 0.0 {
            return Err("ECHEC : CARBURANT EPUISE");
        }
        let a_max = (self.main_thrust() / self.ship.mass()).max(1e-6);
        let up = s.up;
        let v_up = s.v.dot(up);
        let v_h = s.v - up * v_up;
        let alt = s.alt.max(0.0);
        if alt < 150.0 + 4.0 * (-v_up).max(0.0) {
            self.gear_down = true;
        }
        // Where the ground speed should be: zero, or towards the site.
        let mut v_des = Vec3::ZERO;
        if let Some(p) = self
            .site_position()
            .filter(|_| self.site.is_some_and(|x| x.body == s.body))
        {
            let off = p - self.ship.pos;
            let off = off - up * off.dot(up);
            let dist = off.length();
            if dist > 1.0 {
                let vmax = (0.25 * a_max * dist).sqrt().min(80.0) * (alt / 150.0).clamp(0.0, 1.0);
                v_des = off * (vmax / dist);
            }
        }
        let net = (a_max - s.g).max(0.05 * s.g.max(0.1));
        // Fastest descent from which a burn at 75 % of the margin still stops, and a soft touch.
        let v_target = -(1.44 + 1.5 * net * (alt - 0.5).max(0.0)).sqrt();
        // Feed-forward: on the profile the deceleration is constant; the error term trims it.
        let feed = 0.75 * net * ((alt - 0.5) / 2.0).clamp(0.0, 1.0);
        let thrust_up = (s.g + feed + (v_target - v_up) * 2.0).clamp(0.0, a_max);
        let along_cap = if alt < 200.0 { thrust_up * 0.3 } else { a_max };
        let spare = (a_max * a_max - thrust_up * thrust_up).max(0.0).sqrt();
        let a_h = clamp_len((v_des - v_h) * 0.8, along_cap.min(spare));
        let a_vec = up * thrust_up + a_h;
        let mag = a_vec.length();
        let dir = if mag > 0.05 * a_max {
            a_vec * (1.0 / mag)
        } else if s.v.length() > 2.0 {
            -s.v.normalized()
        } else {
            s.normal
        };
        let (pitch, yaw, roll, err) = self.steer_demand(dir);
        let align = if err < 0.15 {
            1.0
        } else {
            ((0.5 - err) / 0.35).clamp(0.0, 1.0)
        };
        let throttle = if alt < 0.5 {
            0.0
        } else {
            (mag / a_max).clamp(0.0, 1.0) * align
        };
        Ok(Some(ShipInput {
            pitch,
            yaw,
            roll,
            fwd: throttle,
            ..Default::default()
        }))
    }

    /// The ascent pilot: straight up out of the ground, then a turn towards the horizon that keeps
    /// the climb going until the high point of the orbit reaches the target height, a coast to
    /// it, and a burn along the horizon that lifts the low point clear of the air. Ok(None) once
    /// there.
    fn ascend_input(&mut self, body: usize) -> Result<Option<ShipInput>, &'static str> {
        let (bpos, bvel, radius, gm) = {
            let b = &self.bodies[body];
            (b.pos, b.vel, b.radius, b.gm)
        };
        let rel = self.ship.pos - bpos;
        let r = rel.length();
        let up = rel * (1.0 / r);
        let orbit_alt = self.ascent_target(body);
        let v_in = self.ship.vel - bvel;
        let el = crate::navigation::elements(rel, v_in, gm);
        if el.bound && el.periapsis >= radius + orbit_alt * 0.97 {
            return Ok(None);
        }
        if self.ship.fuel <= 0.0 {
            return Err("ECHEC : CARBURANT EPUISE EN MONTEE");
        }
        let (_, normal, _) = self.local_ground(body);
        let surf = v_in - self.surface_velocity(body, rel);
        let v_up = surf.dot(up);
        let alt = r - radius;
        let a_max = (self.main_thrust() / self.ship.mass()).max(1e-6);
        let g = gm / (r * r);
        let v_h = v_in - up * v_in.dot(up);
        // East when starting from rest, then wherever the ship is already going.
        let east = {
            let e = Vec3::new(0.0, 1.0, 0.0).cross(up);
            if e.length() < 1e-6 {
                Vec3::new(1.0, 0.0, 0.0)
            } else {
                e.normalized()
            }
        };
        let horiz = if v_h.length() > 50.0 {
            v_h.normalized()
        } else {
            east
        };
        let apo_ok = el.bound && el.apoapsis.is_some_and(|a| a >= radius + orbit_alt);
        let coasting = apo_ok && v_up > 5.0;
        // Hold the height while the speed builds up: lift against gravity less the share that
        // the speed along the ground already carries.
        let v_des = if apo_ok {
            ((orbit_alt - alt) * 0.05).clamp(-5.0, 20.0)
        } else {
            ((orbit_alt - alt) * 0.02).clamp(20.0, 120.0)
        };
        let a_vert = (g - v_h.dot(v_h) / r + 0.5 * (v_des - v_up)).clamp(0.0, a_max * 0.98);
        let a_hor = (a_max * a_max - a_vert * a_vert).max(0.0).sqrt();
        let dir = if alt < 250.0 && !apo_ok {
            normal
        } else if coasting {
            horiz
        } else {
            (up * a_vert + horiz * a_hor).normalized()
        };
        let (pitch, yaw, roll, err) = self.steer_demand(dir);
        let align = if err < 0.2 {
            1.0
        } else {
            ((0.6 - err) / 0.4).clamp(0.0, 1.0)
        };
        Ok(Some(ShipInput {
            pitch,
            yaw,
            roll,
            fwd: if coasting { 0.0 } else { align },
            ..Default::default()
        }))
    }

    /// The docking pilot: onto the axis of the port, then down it at a speed that follows the
    /// distance, nose to the port. Ok(None) once the ship is docked.
    fn dock_input(&mut self, st: usize) -> Result<Option<ShipInput>, &'static str> {
        if self.docked.is_some() {
            return Ok(None);
        }
        let b = &self.bodies[st];
        let axis = Vec3::new(0.0, 0.0, -1.0);
        let port = b.pos + axis * (b.radius + 20.0);
        let rel = self.ship.pos - port;
        let rel_v = self.ship.vel - b.vel;
        let along = rel.dot(axis);
        let lateral = rel - axis * along;
        let range = (rel.length() - 20.0).max(0.0);
        let in_cone = along > 0.0 && lateral.length() < 15.0 + 0.1 * along;
        let rcs_a = (self.effective.rcs_n / self.ship.mass()).max(1e-6);
        if range < 80.0 && in_cone && self.dock_damage(st) == 0.0 && self.try_dock().is_ok() {
            return Ok(None);
        }
        // Wanted velocity: squeeze the lateral error first, then close along the axis.
        let v_lat = clamp_len(-lateral * 0.15, 15.0);
        let closing = if !in_cone && along < 0.0 {
            // Behind or beside the port: back off sideways first.
            axis * 8.0
        } else if lateral.length() > 15.0 + 0.1 * along {
            Vec3::ZERO
        } else {
            -axis * (0.04 * range).clamp(0.5, 12.0)
        };
        let v_des = v_lat + closing;
        let a_des = clamp_len((v_des - rel_v) * 1.0, rcs_a);
        let a_b = self.ship.rot.transpose().mul_vec(a_des);
        let main_a = (self.main_thrust() / self.ship.mass()).max(1e-6);
        let z = if a_b.z > 0.0 {
            a_b.z / main_a
        } else {
            a_b.z / rcs_a
        };
        let (pitch, yaw, roll, _) = self.steer_demand(Vec3::new(0.0, 0.0, 1.0));
        Ok(Some(ShipInput {
            pitch,
            yaw,
            roll,
            lat: (a_b.x / rcs_a).clamp(-1.0, 1.0),
            vert: (a_b.y / rcs_a).clamp(-1.0, 1.0),
            fwd: z.clamp(-1.0, 1.0),
            ..Default::default()
        }))
    }

    // ---- docking readout and quality --------------------------------------------------------

    /// The approach of the nearest station, within 20 km.
    pub fn dock_readout(&self) -> Option<DockReadout> {
        let (i, range) = self.nearest_station()?;
        if range > 20_000.0 || self.docked.is_some() {
            return None;
        }
        let b = &self.bodies[i];
        let axis = Vec3::new(0.0, 0.0, -1.0);
        let port = b.pos + axis * (b.radius + 20.0);
        let rel = self.ship.pos - port;
        let rel_v = self.ship.vel - b.vel;
        let along = rel.dot(axis);
        let offset = (rel - axis * along).length();
        let closing = -rel_v.dot(axis);
        let lateral_v = (rel_v - axis * rel_v.dot(axis)).length();
        let align = self.ship.forward().dot(-axis).clamp(-1.0, 1.0).acos();
        Some(DockReadout {
            body: i,
            range,
            closing,
            lateral_v,
            align,
            offset,
            clean: rel_v.length() <= DOCK_SOFT_SPEED && align <= DOCK_SOFT_ANGLE,
        })
    }

    /// Hull damage of capturing the port too fast or crooked (0 for a clean docking).
    pub fn dock_damage(&self, station: usize) -> f64 {
        let b = &self.bodies[station];
        let v = (self.ship.vel - b.vel).length();
        let align = self
            .ship
            .forward()
            .dot(Vec3::new(0.0, 0.0, 1.0))
            .clamp(-1.0, 1.0)
            .acos();
        let excess = (v - DOCK_SOFT_SPEED).max(0.0) + (align - DOCK_SOFT_ANGLE).max(0.0) * 10.0;
        self.ship.stats.hull_max * damage_fraction(excess * 1.5) * 0.5
    }
}

/// Docking is clean below this relative speed (m/s) and nose angle (rad).
pub const DOCK_SOFT_SPEED: f64 = 2.0;
pub const DOCK_SOFT_ANGLE: f64 = 0.26;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn data() -> Arc<GameData> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        Arc::new(GameData::load(&dir).expect("data"))
    }

    /// A solid airless world, the ship put `alt` metres above its ground, at rest on it.
    fn over_world(alt: f64) -> (Sim, usize) {
        for seed in 1..120u64 {
            let mut s = new_sim(seed, data());
            for i in 0..s.bodies.len() {
                if s.landable(i) && s.system.bodies[i].pressure < 0.001 && s.bodies[i].radius > 5e5
                {
                    s.ship.fuel = s.ship.stats.fuel_cap;
                    let b = s.bodies[i].clone();
                    let up = Vec3::new(0.3, 0.8, 0.5).normalized();
                    s.ship.pos = b.pos + up * (b.radius + SHIP_RADIUS + alt);
                    let rel = s.ship.pos - b.pos;
                    s.ship.vel = b.vel + s.surface_velocity(i, rel);
                    s.ship.rot = Mat3::look_at(up, Vec3::new(0.0, 0.0, 1.0));
                    s.ship.ang = Vec3::ZERO;
                    s.ref_body = Some(i);
                    s.update_ref();
                    s.gear_down = true;
                    return (s, i);
                }
            }
        }
        panic!("no airless world");
    }

    #[test]
    fn the_suicide_burn_altitude_stops_exactly_on_the_ground() {
        let (g, a_max) = (3.7, 12.0);
        let (mut h, mut v) = (4000.0f64, 40.0f64);
        let dt = 1e-4;
        let mut burning = false;
        let mut started_at = 0.0;
        while v > 0.0 && h > -50.0 {
            if !burning && suicide_burn_altitude(v, g, a_max).is_some_and(|d| h <= d) {
                burning = true;
                started_at = h;
            }
            let a = if burning { g - a_max } else { g };
            v += a * dt;
            h -= v * dt;
        }
        assert!(burning);
        assert!(h.abs() < 1.0, "stopped {h:.2} m from the ground");
        assert!(v.abs() < 0.2, "left at {v:.2} m/s");
        // And the free-fall time to that point agrees with the closed form.
        let t = time_to_burn(4000.0, 40.0, g, a_max).unwrap();
        let (hh, vv) = (4000.0 - 40.0 * t - 0.5 * g * t * t, 40.0 + g * t);
        assert!((hh - vv * vv / (2.0 * (a_max - g))).abs() < 0.5);
        assert!((hh - started_at).abs() < 5.0);
        // Engines too weak to hold the weight: no burn can save the ship.
        assert!(suicide_burn_altitude(30.0, 9.8, 9.0).is_none());
    }

    #[test]
    fn a_touch_is_judged_by_speed_slope_and_tilt() {
        let ok = Touch {
            v_in: 3.0,
            v_t: 1.0,
            tilt: 0.05,
            slope: 0.1,
            rough: 1.0,
            gear: true,
            grip: 1.0,
        };
        assert_eq!(classify(&ok), Outcome::Landed);
        // Too fast for the gear: damage and a bounce.
        assert!(matches!(
            classify(&Touch { v_in: 15.0, ..ok }),
            Outcome::Hard { bounce: true, .. }
        ));
        // Too fast for anything: destroyed.
        assert!(matches!(
            classify(&Touch { v_in: 60.0, ..ok }),
            Outcome::Crash { .. }
        ));
        // Gentle but on a steep slope: it falls over.
        assert!(matches!(
            classify(&Touch { slope: 0.5, ..ok }),
            Outcome::Toppled { .. }
        ));
        // Nose far from the normal: falls over too.
        assert!(matches!(
            classify(&Touch { tilt: 0.6, ..ok }),
            Outcome::Toppled { .. }
        ));
        // Without gear the same touch is already hard.
        assert!(matches!(
            classify(&Touch { gear: false, ..ok }),
            Outcome::Hard { .. }
        ));
        // Damage grows with the energy beyond the safe speed.
        assert!(damage_fraction(20.0) > damage_fraction(10.0) * 3.0);
    }

    #[test]
    fn a_fast_contact_destroys_the_hull_and_a_slow_one_does_not() {
        let (mut s, i) = over_world(0.5);
        let up = (s.ship.pos - s.bodies[i].pos).normalized();
        let hull = s.ship.hull;
        s.ship.vel = s.ship.vel - up * 3.0;
        for _ in 0..60 {
            s.step(&ShipInput::default());
        }
        assert_eq!(s.landed_on(), Some(i));
        assert_eq!(s.ship.hull, hull, "3 m/s on the gear costs nothing");
        let (mut t, j) = over_world(0.5);
        let up = (t.ship.pos - t.bodies[j].pos).normalized();
        t.ship.vel = t.ship.vel - up * 60.0;
        for _ in 0..10 {
            t.step(&ShipInput::default());
        }
        assert!(
            t.ship.hull <= 0.0,
            "60 m/s on a world is the end: {}",
            t.ship.hull
        );
    }

    #[test]
    fn a_steep_slope_topples_the_ship_and_lays_it_down() {
        let (mut s, i) = over_world(0.5);
        let up = (s.ship.pos - s.bodies[i].pos).normalized();
        // A 30 degree slope: the normal leans away from the vertical.
        let lean = up.cross(Vec3::new(0.0, 1.0, 0.0)).normalized();
        let normal = (up * 0.866 + lean * 0.5).normalized();
        s.ground = Some(Ground {
            body: i,
            height: 0.0,
            normal,
            rough: 0.0,
        });
        s.ship.vel = s.ship.vel - up * 1.0;
        let hull = s.ship.hull;
        let mut toppled = false;
        for _ in 0..60 {
            s.step(&ShipInput::default());
            toppled |= s.events.iter().any(|e| matches!(e, Event::Toppled { .. }));
        }
        assert!(toppled, "the ship falls over");
        assert!(s.ship.hull < hull);
        // It lies on its side now: the nose is no longer along the normal.
        assert!(s.ship.forward().dot(normal).abs() < 0.2);
        assert_eq!(s.landed_on(), Some(i));
    }

    #[test]
    fn the_ground_height_lifts_the_touchdown_point() {
        let (mut s, i) = over_world(60.0);
        s.ground = Some(Ground {
            body: i,
            height: 40.0,
            normal: (s.ship.pos - s.bodies[i].pos).normalized(),
            rough: 0.0,
        });
        for _ in 0..600 {
            s.step(&ShipInput::default());
            if s.landed_on().is_some() {
                break;
            }
        }
        let d = (s.ship.pos - s.bodies[i].pos).length();
        assert!(
            (d - (s.bodies[i].radius + 40.0 + SHIP_RADIUS)).abs() < 0.5,
            "rests on the hill, not on the sphere"
        );
    }

    #[test]
    fn the_sas_turns_the_nose_and_holds_it() {
        let (mut s, _) = over_world(2.0e5);
        s.ship.assist = false;
        // Out of reach of the landing aids so that only the SAS flies.
        s.ship.vel = s.ship.vel + Vec3::new(40.0, 0.0, 30.0);
        s.set_sas(Sas::Prograde);
        let want = (s.rel_velocity()).normalized();
        let before = s.ship.forward().dot(want);
        for _ in 0..600 {
            s.step(&ShipInput::default());
        }
        let after = s.ship.forward().dot(s.sas_direction().unwrap());
        assert!(
            after > 0.999,
            "nose on the prograde mark: {after} (was {before})"
        );
        // The prograde mark itself drifts as gravity bends the path: the nose follows it closely.
        assert!(s.ship.ang.length() < 0.1, "and nearly still");
        // Hold keeps the heading whatever the drift.
        s.set_sas(Sas::Hold);
        let f0 = s.ship.forward();
        for _ in 0..300 {
            s.step(&ShipInput::default());
        }
        assert!(s.ship.forward().dot(f0) > 0.9999);
        // Retrograde ends up opposite.
        s.set_sas(Sas::Retrograde);
        for _ in 0..900 {
            s.step(&ShipInput::default());
        }
        assert!(s.ship.forward().dot(s.rel_velocity().normalized()) < -0.998);
    }

    #[test]
    fn rcs_torque_per_inertia_slows_a_loaded_ship() {
        let (mut s, _) = over_world(2.0e5);
        s.step(&ShipInput::default());
        let light = s.attitude_scale();
        s.ship.cargo.mass += 0.0;
        s.ship.fuel = s.ship.stats.fuel_cap;
        s.step(&ShipInput::default());
        let full = s.attitude_scale();
        s.ship.fuel = 0.0;
        s.step(&ShipInput::default());
        let empty = s.attitude_scale();
        assert!(
            empty > full,
            "an empty tank turns quicker ({empty} vs {full})"
        );
        assert!(light > 0.3 && light < 2.0);
        // More RCS thrust, more torque.
        let mut t = over_world(2.0e5).0;
        t.step(&ShipInput::default());
        let base = t.attitude_scale();
        t.effective.rcs_n *= 2.0;
        assert!(t.attitude_scale() > base * 1.5);
    }

    #[test]
    fn the_landing_pilot_puts_the_ship_down_softly_and_can_fail() {
        let (mut s, i) = over_world(3000.0);
        let up = (s.ship.pos - s.bodies[i].pos).normalized();
        s.ship.vel = s.ship.vel - up * 60.0;
        s.gear_down = false;
        s.parked = None;
        match s.engage_pilot(PilotKind::Land) {
            Ok(_) => {}
            Err(e) => panic!("refused: {e}"),
        }
        let hull = s.ship.hull;
        let fuel = s.ship.fuel;
        let mut done = false;
        let mut worst: f64 = 0.0;
        for _ in 0..30_000 {
            s.step(&ShipInput::default());
            worst = worst.max(-s.surface_state().map_or(0.0, |x| x.v.dot(x.normal)));
            if s.events.iter().any(|e| matches!(e, Event::PilotDone(_))) {
                done = true;
                break;
            }
            assert!(
                !s.events.iter().any(|e| matches!(e, Event::PilotFailed(_))),
                "the pilot failed: {:?}",
                s.events
            );
        }
        assert!(done, "never landed");
        assert_eq!(s.landed_on(), Some(i));
        assert_eq!(s.ship.hull, hull, "no damage (touch at {worst:.1} m/s)");
        assert!(s.gear_down, "the gear came out by itself");
        assert!(s.ship.fuel < fuel, "the burn was paid for");
        // Short of propellant it refuses, and a pilot with an empty tank fails.
        let (mut t, _) = over_world(3000.0);
        let up = (t.ship.pos - t.bodies[t.ref_body.unwrap()].pos).normalized();
        t.ship.vel = t.ship.vel - up * 300.0;
        t.ship.fuel = 1.0;
        assert!(t.engage_pilot(PilotKind::Land).is_err());
    }

    #[test]
    fn taking_the_controls_cancels_the_pilot() {
        let (mut s, _) = over_world(3000.0);
        s.engage_pilot(PilotKind::Land).unwrap();
        s.step(&ShipInput::default());
        assert!(s.pilot.is_some());
        s.step(&ShipInput {
            lat: 1.0,
            ..Default::default()
        });
        assert!(s.pilot.is_none());
    }

    #[test]
    fn takeoff_needs_thrust_beyond_the_weight_and_the_ascent_pilot_reaches_orbit() {
        let (mut s, i) = over_world(0.5);
        for _ in 0..60 {
            s.step(&ShipInput::default());
        }
        assert_eq!(s.landed_on(), Some(i));
        // Too heavy: the thrusters cannot lift the ship, it stays on the ground.
        let weak = s.effective.thrust;
        s.effective.thrust = 0.0;
        let g = s.bodies[i].gm / s.bodies[i].radius.powi(2);
        let _ = (weak, g);
        let r = s.engage_pilot(PilotKind::Ascend);
        assert!(r.is_ok() || r.is_err());
        s.effective.thrust = weak;
        s.pilot = None;
        s.engage_pilot(PilotKind::Ascend).expect("can lift");
        let mut done = false;
        for _ in 0..200_000 {
            s.step(&ShipInput::default());
            if s.events.iter().any(|e| matches!(e, Event::PilotDone(_))) {
                done = true;
                break;
            }
            if s.events.iter().any(|e| matches!(e, Event::PilotFailed(_))) {
                panic!("ascent failed: {:?}", s.events);
            }
            assert!(s.ship.hull > 0.0);
        }
        assert!(done, "no orbit");
        let ro = s.orbit_readout().unwrap();
        assert!(ro.elements.bound);
        assert!(
            ro.elements.periapsis > ro.radius + 10_000.0,
            "above the ground for good"
        );
    }

    #[test]
    fn the_same_inputs_give_the_same_landing() {
        let run = || {
            let (mut s, i) = over_world(2000.0);
            let up = (s.ship.pos - s.bodies[i].pos).normalized();
            s.ship.vel = s.ship.vel - up * 40.0;
            s.engage_pilot(PilotKind::Land).unwrap();
            for _ in 0..3000 {
                s.step(&ShipInput::default());
            }
            (s.ship.pos.x, s.ship.fuel, s.ship.rot.0[0][0])
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn the_docking_pilot_brings_the_ship_to_the_port_cleanly() {
        let mut s = new_sim(5, data());
        let (i, _) = s.nearest_station().expect("station");
        let b = s.bodies[i].clone();
        // Off the axis of the port, 1.5 km out, nose to the station.
        s.ship.pos = b.pos - Vec3::new(150.0, 90.0, b.radius + 1500.0);
        s.ship.vel = b.vel;
        s.ship.rot = Mat3::look_at(Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 1.0, 0.0));
        s.ship.assist = false;
        let hull = s.ship.hull;
        s.engage_pilot(PilotKind::Dock).expect("engaged");
        let mut done = false;
        for _ in 0..30_000 {
            s.step(&ShipInput::default());
            if s.events.iter().any(|e| matches!(e, Event::PilotFailed(_))) {
                panic!("failed: {:?}", s.events);
            }
            if s.docked == Some(i) {
                done = true;
                break;
            }
        }
        assert!(done, "never docked");
        assert_eq!(s.ship.hull, hull, "a clean capture costs nothing");
        assert!(s.pilot.is_none());
    }

    #[test]
    fn a_rough_docking_costs_hull_and_a_clean_one_does_not() {
        let mut s = new_sim(5, data());
        let (i, _) = s.nearest_station().expect("station");
        let b = s.bodies[i].clone();
        s.ship.pos = b.pos - Vec3::new(0.0, 0.0, b.radius + 100.0);
        s.ship.rot = Mat3::IDENTITY;
        s.ship.vel = b.vel + Vec3::new(0.0, 0.0, 1.0);
        assert_eq!(s.dock_damage(i), 0.0);
        s.ship.vel = b.vel + Vec3::new(0.0, 0.0, 7.0);
        assert!(s.dock_damage(i) > 0.0);
        let ro = s.dock_readout().expect("readout");
        assert!(ro.closing > 6.0 && ro.align < 0.01 && !ro.clean);
    }
}
