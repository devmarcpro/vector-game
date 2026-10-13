//! The ATTERRIR command: from anywhere in the system, put the ship on the ground of a world and
//! leave it standing there. It is one request to the navigation computer: a transfer to a low
//! orbit of the world (or an orbit change, or nothing when the ship is already low), then the
//! landing pilot of `landing.rs` (suicide burn, gear, flare, touch), then the ship is parked on the
//! ground of the chosen site and turns with the world. Before the engagement the brief says what
//! it costs and what is dangerous; at any moment a manual command takes the ship back.
use crate::landing::Site;
use crate::nav_auto::{Order, Run};
use crate::{Event, Sim};
use std::f64::consts::{PI, TAU};
use vector_core::Vec3;
use vector_world::surface::{H, W};
use vector_world::BodyKind;

/// What the player asked: the world, and the cell of its map to land on (None: where it is best).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LandGoal {
    pub body: usize,
    pub cell: Option<(usize, usize)>,
    /// The sea check of the site was done (once per landing: it reads the map).
    pub checked: bool,
}

/// What a landing costs and risks, computed before it is engaged.
#[derive(Clone, Debug, Default)]
pub struct LandBrief {
    pub body: usize,
    pub name: String,
    pub ok: bool,
    pub refusal: Option<String>,
    pub warnings: Vec<String>,
    /// Delta-v of the whole approach and landing, what the tank holds (m/s), fuel needed (%).
    pub dv_need: f64,
    pub dv_have: f64,
    pub fuel_pct: f64,
    /// Thrust over weight on the ground of that world.
    pub twr: f64,
    /// Height of the capture orbit (m).
    pub orbit_alt: f64,
    /// A small body: the ship is held next to it instead of standing on it.
    pub soft: bool,
}

impl Sim {
    /// Height (m) of the orbit from which the pilot lands: above the air, within the pilot's reach.
    pub fn landing_orbit_alt(&self, body: usize) -> f64 {
        let r = self.bodies[body].radius;
        let reach = (r * 0.1).clamp(5_000.0, 150_000.0) * 0.8;
        let top = self.air_of(body).map_or(0.0, |a| a.top());
        (top * 1.15).max(12_000.0).min(reach)
    }

    /// Latitude and longitude (degrees) of the point of world `i` under the ship.
    pub fn surface_latlon(&self, i: usize) -> (f64, f64) {
        let up = (self.ship.pos - self.bodies[i].pos).normalized();
        let nb = self.body_orient(i).transpose().mul_vec(up);
        let lat = nb.y.clamp(-1.0, 1.0).asin();
        let lon = nb.z.atan2(nb.x);
        (lat.to_degrees(), lon.to_degrees())
    }

    /// The cell of the map of world `i` under the ship.
    pub fn ship_cell(&self, i: usize) -> (usize, usize) {
        let (lat, lon) = self.surface_latlon(i);
        let lon = lon.to_radians().rem_euclid(TAU);
        let lat = lat.to_radians();
        (
            ((lon / TAU * W as f64) as usize).min(W - 1),
            (((0.5 - lat / PI) * H as f64).max(0.0) as usize).min(H - 1),
        )
    }

    /// Body-fixed unit direction of the middle of a map cell.
    pub fn cell_dir(cx: usize, cy: usize) -> Vec3 {
        let lon = (cx as f64 + 0.5) / W as f64 * TAU;
        let lat = (0.5 - (cy as f64 + 0.5) / H as f64) * PI;
        Vec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin())
    }

    /// What landing on `body` (at `cell`) would cost and risk, and whether it is possible.
    pub fn land_brief(&self, body: usize, cell: Option<(usize, usize)>) -> LandBrief {
        let b = &self.bodies[body];
        let mut r = LandBrief {
            body,
            name: b.name.to_uppercase(),
            dv_have: self.ship.delta_v(),
            ..Default::default()
        };
        let refuse = |r: &LandBrief, why: &str| {
            let mut r = r.clone();
            r.refusal = Some(why.to_string());
            r
        };
        match b.kind {
            BodyKind::Station => return refuse(&r, "UNE STATION S'AMARRE (D), ON NE S'Y POSE PAS"),
            BodyKind::Star(_) | BodyKind::Anomaly(_) => {
                return refuse(&r, "AUCUNE SURFACE OU S'POSER ICI")
            }
            BodyKind::Planet(k) | BodyKind::Moon(k) if k.is_giant() => {
                return refuse(&r, "GEANTE : AUCUNE SURFACE SOLIDE")
            }
            _ => {}
        }
        if self.docked.is_some() {
            return refuse(&r, "QUITTEZ D'ABORD LA STATION (U)");
        }
        if self.jump.is_some() || self.in_transit() {
            return refuse(&r, "IMPOSSIBLE PENDANT UN SAUT");
        }
        if self.landed_on() == Some(body) {
            return refuse(&r, "LE VAISSEAU EST DEJA AU SOL ICI");
        }
        if self.ship.fuel <= 0.0 {
            return refuse(&r, "PLUS DE CARBURANT");
        }
        if matches!(b.kind, BodyKind::Asteroid(_)) {
            // A small rock: the ship is brought alongside and held there.
            r.soft = true;
            r.ok = true;
            r.warnings
                .push("ASTEROIDE : PAS DE SOL, LE VAISSEAU SERA MAINTENU AU CONTACT".into());
            return r;
        }
        let g = b.gm / (b.radius * b.radius);
        r.twr = self.main_thrust() / (self.ship.mass() * g);
        if r.twr < 1.1 {
            return refuse(
                &r,
                &format!(
                    "POUSSEE INSUFFISANTE : {:.2} FOIS LE POIDS SUR {} (IL FAUT 1,1)",
                    r.twr,
                    b.name.to_uppercase()
                ),
            );
        }
        let alt = self.landing_orbit_alt(body);
        r.orbit_alt = alt;
        let v_circ = (b.gm / (b.radius + alt)).sqrt();
        // After the capture: kill the orbital speed, fall, brake. The air helps; not counted.
        let land_dv = v_circ + (2.0 * g * alt).sqrt() * 0.5;
        let near = self.surface_state().is_some_and(|s| s.body == body);
        let approach: f64;
        if near {
            // Already low: only what is left of the speed along the ground and the fall.
            let s = self.surface_state().expect("near");
            let v_up = s.v.dot(s.up);
            let v_h = (s.v - s.up * v_up).length();
            approach = -land_dv + v_h + (v_up * v_up + 2.0 * g * s.alt.max(0.0)).sqrt();
        } else if self.ref_body == Some(body) {
            let v_here = (b.gm / (self.ship.pos - b.pos).length().max(1.0)).sqrt();
            approach = (v_here - v_circ).abs() * 2.0;
        } else {
            let opts = self.plan_transfer_options(body, alt);
            match opts
                .iter()
                .filter(|o| o.ok)
                .min_by(|a, c| a.dv.total_cmp(&c.dv))
            {
                Some(o) => approach = o.dv,
                None => {
                    let why = opts
                        .iter()
                        .find_map(|o| o.why.clone())
                        .unwrap_or_else(|| "AUCUN TRANSFERT REALISABLE VERS CE MONDE".into());
                    return refuse(&r, &why);
                }
            }
        }
        // A site far from the point under the ship costs the speed to go there.
        let mut far = 0.0;
        if let Some(c) = cell {
            let here = (self.ship.pos - b.pos).normalized();
            let site = self.body_orient(body).mul_vec(Self::cell_dir(c.0, c.1));
            let angle = here.dot(site).clamp(-1.0, 1.0).acos();
            far = 0.5 * v_circ * angle.min(1.5);
            if angle * b.radius > 100_000.0 {
                r.warnings.push(format!(
                    "SITE LOIN : {:.0} KM DE LA TRAJECTOIRE ACTUELLE",
                    angle * b.radius / 1000.0
                ));
            }
        }
        r.dv_need = (approach + far + land_dv).max(land_dv * 0.2);
        let fuel = self.fuel_for_dv(r.dv_need);
        r.fuel_pct = 100.0 * fuel / self.ship.stats.fuel_cap.max(1.0);
        if r.dv_need > r.dv_have {
            return refuse(
                &r,
                &format!(
                    "CARBURANT INSUFFISANT : {:.0} M/S DEMANDES, {:.0} DISPONIBLES",
                    r.dv_need, r.dv_have
                ),
            );
        }
        if r.dv_need > 0.7 * r.dv_have {
            r.warnings.push(format!(
                "MARGE DE CARBURANT FAIBLE : {:.0} % DU RESERVOIR",
                r.fuel_pct
            ));
        }
        if r.twr < 2.0 {
            r.warnings.push(format!(
                "MARGE DE POUSSEE FAIBLE ({:.1} FOIS LE POIDS) : FREINAGE LONG",
                r.twr
            ));
        }
        let pressure = self.system.bodies[body].pressure;
        if pressure > 5.0 {
            r.warnings.push(format!(
                "ATMOSPHERE DENSE : {pressure:.0} BAR, ECHAUFFEMENT"
            ));
        }
        let temp = self.system.bodies[body].temp;
        if temp > 450.0 || temp < 150.0 {
            r.warnings
                .push(format!("TEMPERATURE EXTREME AU SOL : {temp:.0} K"));
        }
        let spin = self.spin_rate(body) * b.radius;
        if spin > 200.0 {
            r.warnings
                .push(format!("SOL RAPIDE A L'EQUATEUR : {spin:.0} M/S"));
        }
        if let Some(s) = self.site.filter(|s| s.body == body) {
            if s.slope > 0.25 {
                r.warnings.push(format!(
                    "SITE EN PENTE : {:.0} DEG (LIMITE {:.0})",
                    s.slope.to_degrees(),
                    crate::landing::SLOPE_MAX.to_degrees()
                ));
            }
        }
        if cell.is_some() && pressure < 0.001 {
            r.warnings
                .push("MONDE SANS AIR : AUCUN FREIN ATMOSPHERIQUE".into());
        }
        r.ok = true;
        r
    }

    /// Where the ship will touch down if it brakes along its ground track from here, and when
    /// that is sea or lake, the nearest dry cell of the map to it (None when it is dry already,
    /// or when the world has no map).
    pub fn dry_site_ahead(&self, body: usize) -> Option<(usize, usize)> {
        let map = self.planet_map(body)?;
        let b = &self.bodies[body];
        let rel = self.ship.pos - b.pos;
        let up = rel.normalized();
        let v = self.ship.vel - b.vel - self.surface_velocity(body, rel);
        let vh = v - up * v.dot(up);
        let a = (self.main_thrust() / self.ship.mass()).max(1.0);
        let reach = (vh.length().powi(2) / a).min(0.5 * PI * b.radius);
        let ahead = if vh.length() > 1.0 {
            (up * b.radius + vh.normalized() * reach).normalized()
        } else {
            up
        };
        let nb = self.body_orient(body).transpose().mul_vec(ahead);
        let lon = nb.z.atan2(nb.x).rem_euclid(TAU);
        let lat = nb.y.clamp(-1.0, 1.0).asin();
        let cx = ((lon / TAU * W as f64) as usize).min(W - 1);
        let cy = (((0.5 - lat / PI) * H as f64).max(0.0) as usize).min(H - 1);
        let wet = |x: usize, y: usize| map.surface.is_sea(x, y) || map.surface.is_lake(x, y);
        if !wet(cx, cy) {
            return None;
        }
        for r in 1..=80i64 {
            let mut best: Option<(i64, (usize, usize))> = None;
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let y = cy as i64 + dy;
                    if !(0..H as i64).contains(&y) {
                        continue;
                    }
                    let x = (cx as i64 + dx).rem_euclid(W as i64) as usize;
                    if !wet(x, y as usize) {
                        let d = dx * dx + dy * dy;
                        if best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, (x, y as usize)));
                        }
                    }
                }
            }
            if let Some((_, cell)) = best {
                return Some(cell);
            }
        }
        None
    }

    /// Engage the landing: refused with the reason, else the brief (with its warnings).
    pub fn land_command(
        &mut self,
        body: usize,
        cell: Option<(usize, usize)>,
    ) -> Result<LandBrief, String> {
        let brief = self.land_brief(body, cell);
        if let Some(why) = &brief.refusal {
            return Err(why.clone());
        }
        self.nav_clear();
        self.pilot = None;
        self.autopilot = None;
        self.parked = None;
        self.site = cell.map(|(cx, cy)| Site {
            body,
            dir: Self::cell_dir(cx, cy),
            height: 0.0,
            slope: 0.0,
        });
        let mut orders = Vec::new();
        if brief.soft {
            orders.push(Order::Approach { body });
        } else {
            let near = self.surface_state().is_some_and(|s| s.body == body);
            if !near {
                let alt = brief.orbit_alt;
                if self.ref_body == Some(body) {
                    orders.push(Order::Orbit {
                        body,
                        peri_alt: alt,
                        apo_alt: alt,
                        stage: 0,
                    });
                } else {
                    orders.push(Order::Transfer {
                        target: body,
                        alt,
                        aero: false,
                    });
                }
            }
            orders.push(Order::Land);
        }
        for o in orders {
            self.nav_add(o).map_err(str::to_string)?;
        }
        self.nav_start().map_err(str::to_string)?;
        self.land_goal = Some(LandGoal {
            body,
            cell,
            checked: false,
        });
        Ok(brief)
    }

    /// Give up the landing under way (the ship keeps what it flies and is left to the player).
    pub fn cancel_landing(&mut self) {
        if self.land_goal.take().is_some() {
            self.nav_abort_active();
            self.nav_clear();
            self.pilot = None;
            self.events.push(Event::LandingEnded(
                "ATTERRISSAGE ANNULE : VOUS AVEZ LA MAIN".into(),
            ));
        }
    }

    /// Follow the landing: done when the ship stands on the world, ended when the computer stops.
    pub(crate) fn land_tick(&mut self) {
        let Some(goal) = self.land_goal else {
            return;
        };
        // Over the sea nobody lands: when the pilot starts to brake and the point it would reach
        // is water, the nearest dry ground is its site.
        if goal.cell.is_none()
            && !goal.checked
            && self.site.is_none_or(|s| s.body != goal.body)
            && self
                .pilot
                .is_some_and(|p| p.kind == crate::landing::PilotKind::Land)
        {
            if let Some(g) = self.land_goal.as_mut() {
                g.checked = true;
            }
            if let Some((cx, cy)) = self.dry_site_ahead(goal.body) {
                self.site = Some(Site {
                    body: goal.body,
                    dir: Self::cell_dir(cx, cy),
                    height: 0.0,
                    slope: 0.0,
                });
            }
        }
        let bname = self.bodies[goal.body].name.to_uppercase();
        let soft_done = matches!(self.bodies[goal.body].kind, BodyKind::Asteroid(_))
            && self.parked.is_some_and(|p| p.0 == goal.body);
        if self.landed_on() == Some(goal.body) || soft_done {
            self.land_goal = None;
            self.nav_clear();
            let (lat, lon) = self.surface_latlon(goal.body);
            self.events.push(Event::LandingEnded(if soft_done {
                format!("AMARRE A {bname}")
            } else {
                format!(
                    "POSE SUR {} A {:.1} {} {:.1} {}",
                    bname,
                    lat.abs(),
                    if lat >= 0.0 { "N" } else { "S" },
                    lon.abs(),
                    if lon >= 0.0 { "E" } else { "O" }
                )
            }));
            return;
        }
        let why = match &self.nav.run {
            Run::Halted(w) => Some(w.clone()),
            Run::Paused => Some("PLAN EN PAUSE : VOUS AVEZ LA MAIN".into()),
            Run::Idle => Some(
                self.nav
                    .log
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "ATTERRISSAGE ARRETE".into()),
            ),
            Run::Running => None,
        };
        if let Some(w) = why {
            self.land_goal = None;
            self.events
                .push(Event::LandingEnded(format!("ATTERRISSAGE ARRETE : {w}")));
        }
    }

    /// Short state for the dashboard: standing on a world, or the landing under way.
    pub fn ground_status(&self) -> Option<String> {
        if let Some(i) = self.landed_on() {
            let (lat, lon) = self.surface_latlon(i);
            return Some(format!(
                "AU SOL SUR {}  {:.1}{} {:.1}{}",
                self.bodies[i].name.to_uppercase(),
                lat.abs(),
                if lat >= 0.0 { "N" } else { "S" },
                lon.abs(),
                if lon >= 0.0 { "E" } else { "O" }
            ));
        }
        self.land_goal.map(|g| {
            let step = match self.nav.cursor() {
                c if self
                    .nav
                    .orders
                    .get(c)
                    .is_some_and(|e| e.order == Order::Land) =>
                {
                    "DESCENTE"
                }
                _ => "APPROCHE",
            };
            format!(
                "ATTERRISSAGE SUR {} : {step}",
                self.bodies[g.body].name.to_uppercase()
            )
        })
    }

    /// After a load: a ship that was standing on the ground (resting on it, at rest relative to
    /// it) is parked there again, gear out. The parked state itself is not in the file.
    pub(crate) fn rest_on_ground_after_load(&mut self) {
        let Some(i) = self.ref_body.filter(|&i| self.landable(i)) else {
            return;
        };
        let b = &self.bodies[i];
        let rel = self.ship.pos - b.pos;
        let (height, ..) = self.local_ground(i);
        let rest = b.radius + height + crate::SHIP_RADIUS;
        let v = self.ship.vel - b.vel - self.surface_velocity(i, rel);
        if (rel.length() - rest).abs() < 3.0 && v.length() < 2.0 && self.docked.is_none() {
            self.parked = Some((i, rel));
            self.gear_down = true;
        }
    }

    /// Set out on foot from the point where the ship stands: the squad lands on the cell under
    /// the ship (the ship stays there, parked).
    pub fn disembark_here(&mut self) -> Result<(), &'static str> {
        let body = self.landed_on().ok_or("le vaisseau n'est pas au sol")?;
        let (cx, cy) = self.ship_cell(body);
        let party = self.rogue_squad();
        self.rogue_land(body, cx, cy, party, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use crate::ShipInput;
    use vector_core::Vec3;

    /// A seed and a solid world the ship can land on, with the ship on a circular orbit of
    /// `radii` radii of it; `skip` worlds are passed over first.
    fn orbiting(want_air: bool, radii: f64, skip: usize) -> (Sim, usize) {
        let mut n = 0;
        for seed in 1..200u64 {
            let mut s = new_sim(seed, crate::test_support::data());
            for i in 0..s.bodies.len() {
                let air = s.system.bodies[i].pressure > 0.05;
                if !s.landable(i) || air != want_air || s.bodies[i].radius < 4.0e5 {
                    continue;
                }
                if s.system.bodies[i].orbit.is_none() {
                    continue;
                }
                n += 1;
                if n <= skip {
                    continue;
                }
                let b = s.bodies[i].clone();
                let r = b.radius * radii;
                s.ship.pos = b.pos + Vec3::new(r, 0.0, 0.0);
                s.ship.vel = b.vel + Vec3::new(0.0, 0.0, (b.gm / r).sqrt());
                s.update_ref_pub();
                s.ship.fuel = s.ship.stats.fuel_cap;
                s.nav.limits.dose_return = 1.0e9;
                s.nav.limits.emergency = false;
                if s.land_brief(i, None).ok {
                    return (s, i);
                }
            }
        }
        panic!("no such world");
    }

    /// Fly until the landing ends or `max_s` pass; the events of the landing.
    fn fly(s: &mut Sim, max_s: f64, chunk: f64) -> Vec<Event> {
        let t0 = s.time;
        let mut all = Vec::new();
        while s.land_goal.is_some() && s.time - t0 < max_s {
            s.events.clear();
            s.advance(chunk, &ShipInput::default());
            all.extend(s.events.iter().cloned());
        }
        all
    }

    fn assert_standing(s: &Sim, i: usize, hull0: f64, fuel0: f64, evs: &[Event]) {
        assert!(
            evs.iter()
                .any(|e| matches!(e, Event::LandingEnded(m) if m.starts_with("POSE SUR"))),
            "{:?} {:?} t={}",
            evs.iter()
                .filter(|e| matches!(
                    e,
                    Event::LandingEnded(_) | Event::PilotFailed(_) | Event::Impact { .. }
                ))
                .take(4)
                .collect::<Vec<_>>(),
            s.nav.log.iter().rev().take(4).collect::<Vec<_>>(),
            s.time
        );
        assert_eq!(s.landed_on(), Some(i), "standing on the world");
        assert!(s.parked.is_some_and(|p| p.0 == i), "parked on it");
        let rel =
            s.ship.vel - s.bodies[i].vel - s.surface_velocity(i, s.ship.pos - s.bodies[i].pos);
        assert!(
            rel.length() < 0.5,
            "at rest on the ground: {}",
            rel.length()
        );
        assert!(s.ship.hull > 0.9 * hull0, "hull {} of {hull0}", s.ship.hull);
        assert!(s.ship.fuel < fuel0 && s.ship.fuel > 0.0, "paid in fuel");
        assert!(s.gear_down, "gear out");
    }

    #[test]
    fn from_a_low_orbit_the_command_ends_with_the_ship_standing_on_the_ground() {
        for skip in 0..2 {
            let (mut s, i) = orbiting(false, 1.04, skip);
            let (hull0, fuel0) = (s.ship.hull, s.ship.fuel);
            let brief = s.land_command(i, None).expect("engaged");
            assert!(brief.dv_need > 100.0);
            let evs = fly(&mut s, 20_000.0, 5.0);
            assert_standing(&s, i, hull0, fuel0, &evs);
            // It stays there and turns with the world.
            let p0 = s.ship.pos - s.bodies[i].pos;
            s.advance(600.0, &ShipInput::default());
            assert_eq!(s.landed_on(), Some(i));
            let (a, b) = (p0.normalized(), (s.ship.pos - s.bodies[i].pos).normalized());
            let turned = a.dot(b).clamp(-1.0, 1.0).acos();
            let want = s.spin_rate(i) * 600.0;
            assert!(
                (turned - want).abs() < 0.02 + want * 0.05,
                "{turned} vs {want}"
            );
        }
    }

    #[test]
    fn from_a_high_orbit_it_comes_down_by_itself() {
        let (mut s, i) = orbiting(false, 4.0, 0);
        let (hull0, fuel0) = (s.ship.hull, s.ship.fuel);
        s.land_command(i, None).expect("engaged");
        let evs = fly(&mut s, 1_000_000.0, 100.0);
        eprintln!(
            "HIGH {:?} {:?}",
            s.nav
                .orders
                .iter()
                .map(|e| (e.order.clone(), e.status))
                .collect::<Vec<_>>(),
            s.elements_about(i)
        );
        assert_standing(&s, i, hull0, fuel0, &evs);
    }

    #[test]
    fn a_site_chosen_on_the_map_is_where_the_ship_ends_up() {
        let (mut s, i) = orbiting(false, 1.04, 0);
        let (cx, cy) = s.ship_cell(i);
        let cell = ((cx + 6) % W, cy);
        s.land_command(i, Some(cell)).expect("engaged");
        let evs = fly(&mut s, 20_000.0, 5.0);
        let dbg = format!(
            "{:?} {:?} {:?}",
            s.nav.log,
            evs.iter().rev().take(3).collect::<Vec<_>>(),
            s.nav
                .orders
                .iter()
                .map(|e| (e.order.clone(), e.status))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            s.landed_on(),
            Some(i),
            "{}",
            dbg.chars().take(900).collect::<String>()
        );
        let (ex, ey) = s.ship_cell(i);
        let dx = (ex as i64 - cell.0 as i64).rem_euclid(W as i64);
        let dx = dx.min(W as i64 - dx);
        assert!(
            dx <= 3 && (ey as i64 - cell.1 as i64).abs() <= 3,
            "{ex},{ey} for {cell:?}"
        );
    }

    #[test]
    fn impossible_landings_are_refused_with_a_reason() {
        let (mut s, i) = orbiting(false, 1.5, 0);
        let star = s
            .bodies
            .iter()
            .position(|b| matches!(b.kind, BodyKind::Star(_)))
            .unwrap();
        assert!(s.land_command(star, None).is_err());
        if let Some(st) = s.bodies.iter().position(|b| b.kind == BodyKind::Station) {
            let e = s.land_command(st, None).unwrap_err();
            assert!(e.contains("STATION"), "{e}");
        }
        if let Some(g) = (0..s.bodies.len())
            .find(|&k| matches!(s.bodies[k].kind, BodyKind::Planet(p) if p.is_giant()))
        {
            let e = s.land_command(g, None).unwrap_err();
            assert!(e.contains("GEANTE"), "{e}");
        }
        // Not enough thrust for the weight.
        let thrust = s.ship.stats.thrust;
        s.ship.stats.thrust = 1000.0;
        s.effective.thrust = 1000.0;
        let e = s.land_command(i, None).unwrap_err();
        assert!(e.contains("POUSSEE"), "{e}");
        s.ship.stats.thrust = thrust;
        s.effective.thrust = thrust;
        // Not enough fuel.
        s.ship.fuel = 5.0;
        let e = s.land_command(i, None).unwrap_err();
        assert!(e.contains("CARBURANT"), "{e}");
        assert!(s.land_goal.is_none() && !s.nav_running());
    }

    #[test]
    fn taking_the_controls_stops_the_landing_and_says_so() {
        let (mut s, i) = orbiting(false, 1.04, 0);
        s.land_command(i, None).expect("engaged");
        s.advance(30.0, &ShipInput::default());
        assert!(s.land_goal.is_some());
        s.events.clear();
        s.advance(
            0.5,
            &ShipInput {
                lat: 1.0,
                ..Default::default()
            },
        );
        assert!(s.land_goal.is_none(), "the goal is dropped");
        assert!(s.events.iter().any(
            |e| matches!(e, Event::LandingEnded(m) if m.contains("ARRETE") || m.contains("ANNULE"))
        ));
        assert!(s.pilot.is_none() && s.landed_on().is_none());
    }

    #[test]
    fn after_the_landing_the_ship_can_take_off_again() {
        let (mut s, i) = orbiting(false, 1.04, 0);
        s.land_command(i, None).expect("engaged");
        fly(&mut s, 20_000.0, 5.0);
        assert_eq!(s.landed_on(), Some(i));
        s.engage_pilot(crate::landing::PilotKind::Ascend)
            .expect("can lift");
        let mut done = false;
        for _ in 0..4000 {
            s.events.clear();
            s.advance(1.0, &ShipInput::default());
            if s.events.iter().any(|e| matches!(e, Event::PilotDone(_))) {
                done = true;
                break;
            }
        }
        assert!(done, "reaches orbit");
        assert!(s.landed_on().is_none());
    }

    /// The ship on a circular orbit around the star at the distance of planet `from`, placed so
    /// that the Hohmann window to planet `to` is open, far from every sphere of influence.
    fn cruising(seed: u64, from: usize, to: usize) -> Sim {
        let mut s = new_sim(seed, crate::test_support::data());
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

    fn free_flight_case(boost: f64) -> Sim {
        // Seed 1: planets 3 and 4 circle the star (used by the navigation tests); land on the
        // outer one when it is solid and thin-aired.
        for seed in 1..60u64 {
            let s = new_sim(seed, crate::test_support::data());
            let sys = &s.system;
            let direct = |i: usize| {
                sys.bodies[i]
                    .orbit
                    .as_ref()
                    .is_some_and(|o| o.parent == 0 && o.e < 0.1)
            };
            let Some((from, to)) = (1..s.bodies.len())
                .filter(|&i| direct(i) && s.landable(i) && sys.bodies[i].pressure < 0.05)
                .find_map(|to| {
                    (1..s.bodies.len())
                        .find(|&f| {
                            f != to
                                && direct(f)
                                && !matches!(s.bodies[f].kind, BodyKind::Asteroid(_))
                                && sys.bodies[f].orbit.as_ref().unwrap().a
                                    < sys.bodies[to].orbit.as_ref().unwrap().a
                                && sys.bodies[to].orbit.as_ref().unwrap().a
                                    < 5.0 * sys.bodies[f].orbit.as_ref().unwrap().a
                        })
                        .map(|f| (f, to))
                })
            else {
                continue;
            };
            let mut s = cruising(seed, from, to);
            let dir = s.ship.vel.normalized();
            s.ship.vel = s.ship.vel + dir * boost;
            s.update_ref_pub();
            let (hull0, fuel0) = (s.ship.hull, s.ship.fuel);
            match s.land_command(to, None) {
                Err(e) => {
                    eprintln!("REFUSED seed {seed}: {e}");
                    continue;
                }
                Ok(brief) => assert!(brief.dv_need > 1000.0, "{brief:?}"),
            }
            let evs = fly(&mut s, 4.0e8, 20_000.0);
            if boost == 0.0 {
                assert_standing(&s, to, hull0, fuel0, &evs);
            } else if s.landed_on() != Some(to) {
                // A fast arrival can be too hard for the computer: it must then give the ship back
                // whole and say why.
                assert!(s.land_goal.is_none());
                assert!(s.ship.hull > 0.9 * hull0, "hull {}", s.ship.hull);
                assert!(evs
                    .iter()
                    .any(|e| matches!(e, Event::LandingEnded(m) if m.contains("ARRETE"))));
            }
            return s;
        }
        panic!("no crossing found");
    }

    #[test]
    fn from_free_flight_millions_of_km_away_it_crosses_the_system_and_lands() {
        free_flight_case(0.0);
    }

    #[test]
    fn from_fast_flight_it_still_ends_on_the_ground_or_refuses_cleanly() {
        free_flight_case(3000.0);
    }

    #[test]
    fn the_ship_on_the_ground_stays_when_saved_and_loaded() {
        let (mut s, i) = orbiting(false, 1.04, 0);
        s.land_command(i, None).expect("engaged");
        fly(&mut s, 20_000.0, 5.0);
        assert_eq!(s.landed_on(), Some(i));
        let bytes = s.save_bytes().expect("saves");
        let mut t = Sim::load_bytes(&bytes, crate::test_support::data()).expect("loads");
        assert_eq!(t.landed_on(), Some(i), "still standing after the load");
        let hull = t.ship.hull;
        t.advance(120.0, &ShipInput::default());
        assert_eq!(t.landed_on(), Some(i));
        assert_eq!(t.ship.hull, hull, "no fall on loading");
    }

    #[test]
    fn a_world_with_seas_is_landed_on_dry_ground() {
        for skip in 0..6 {
            let (mut s, i) = orbiting(true, 1.04, skip);
            let Some(map) = s.planet_map(i) else { continue };
            if !(0..W).any(|x| map.surface.is_sea(x, H / 2)) {
                continue;
            }
            let (hull0, fuel0) = (s.ship.hull, s.ship.fuel);
            let Ok(_) = s.land_command(i, None) else {
                continue;
            };
            let evs = fly(&mut s, 60_000.0, 5.0);
            assert_standing(&s, i, hull0, fuel0, &evs);
            let (cx, cy) = s.ship_cell(i);
            assert!(
                !map.surface.is_sea(cx, cy) && !map.surface.is_lake(cx, cy),
                "stands on dry ground, not on the sea"
            );
            return;
        }
        panic!("no world with seas");
    }
}
