//! The planner of the navigation computer: orbit changes by successive apsis burns, transfers
//! between worlds found by searching the departure on the n-body prediction (a Hohmann guess, then
//! a pattern search that makes the predicted path pass where the ship must), the correction of a
//! course in flight, the options offered to the pilot with their cost, duration and risks, and the
//! route of an FTL trip over the stars in range.
use crate::nav_auto::Order;
use crate::nav_predict::{
    kepler_propagate, ManeuverNode, NodeWhen, PState, PredKind, Prediction, STEP_ANGLE,
};
use crate::navigation::{
    flyby_deflection, flyby_dv, hohmann, hohmann_lead_angle, shield_factor, transfer_window,
};
use crate::Sim;
use std::f64::consts::{PI, TAU};
use vector_core::Vec3;
use vector_world::galaxy::StarRef;
use vector_world::{BodyKind, StarClass};

/// The ratio of orbit radii above which a bi-elliptic transfer spends less delta-v than Hohmann's.
pub const BIELLIPTIC_RATIO: f64 = 11.94;
/// Step (rad of orbit) of the predictions of the searches: coarse, the corrections in flight
/// check with the fine one.
const SEARCH_STEP: f64 = 0.02;
/// Smallest burn worth making (m/s).
const MIN_BURN: f64 = 0.05;

/// Coordinate pattern search: minimise `f` from `x0` with the initial `steps`, shrinking them
/// down to `min_steps`. Returns the best point and its value.
pub fn pattern_search(
    mut f: impl FnMut(&[f64]) -> f64,
    x0: &[f64],
    steps: &[f64],
    min_steps: &[f64],
    max_evals: usize,
) -> (Vec<f64>, f64) {
    let mut x = x0.to_vec();
    let mut s = steps.to_vec();
    let mut best = f(&x);
    let mut evals = 1;
    while evals < max_evals {
        let mut improved = false;
        for i in 0..x.len() {
            for dir in [1.0, -1.0] {
                let mut y = x.clone();
                y[i] += dir * s[i];
                let v = f(&y);
                evals += 1;
                if v < best {
                    best = v;
                    x = y;
                    improved = true;
                    break;
                }
            }
        }
        if !improved {
            let mut all_small = true;
            for i in 0..s.len() {
                if s[i] > min_steps[i] {
                    s[i] *= 0.5;
                    all_small = false;
                }
            }
            if all_small {
                break;
            }
        }
    }
    (x, best)
}

/// A transfer found by the planner.
#[derive(Clone, Debug)]
pub struct TransferSolution {
    /// The departure burn.
    pub node: ManeuverNode,
    pub t_dep: f64,
    /// Time of the closest approach to the target.
    pub t_arr: f64,
    /// Distance of the closest approach to the target (m), and the one aimed at.
    pub miss: f64,
    pub aim: f64,
    pub dv_dep: f64,
    /// Estimated delta-v of the capture burn at the periapsis.
    pub dv_cap: f64,
    /// Relative speed at the closest approach.
    pub v_rel: f64,
    /// From a parking orbit: the direction and the speed the ship must leave the planet with.
    pub leave: Option<(Vec3, f64)>,
    pub pred: Prediction,
}

/// One way to do a trip, with what it costs.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanKind {
    Hohmann,
    Aero,
    Straight,
    Assist,
    BiElliptic,
    Orbit,
}

#[derive(Clone, Debug)]
pub struct PlanOption {
    pub kind: PlanKind,
    pub label: String,
    pub dv: f64,
    pub fuel: f64,
    /// Wait for the window, and the total duration (s).
    pub wait: f64,
    pub duration: f64,
    pub risks: Vec<String>,
    /// The option can be carried out by the computer (an indicative one cannot).
    pub ok: bool,
    pub why: Option<String>,
    pub orders: Vec<Order>,
}

/// An FTL route.
#[derive(Clone, Debug)]
pub struct Hop {
    pub star: StarRef,
    pub dist: f64,
    pub danger: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Route {
    pub hops: Vec<Hop>,
    pub total_ly: f64,
    pub fuel: f64,
    pub antimatter: f64,
    pub seconds: f64,
    pub danger: f64,
    pub ok: bool,
    pub why: Option<String>,
}

fn star_danger(c: StarClass) -> f64 {
    match c {
        StarClass::X => 8.0,
        StarClass::N => 4.0,
        StarClass::O => 2.0,
        StarClass::B => 1.5,
        StarClass::D => 0.5,
        _ => 0.0,
    }
}

/// Scan the departure dates around the windows and refine the best of each window; the earliest
/// window that is nearly as good as the best wins. `guess(date)` is the first burn tried at a date,
/// `eval(date, burn)` the score of a burn (0 is a perfect aim). Returns (score, date, burn).
#[allow(clippy::too_many_arguments)]
fn search_departure(
    now: f64,
    centers: &[f64],
    dates: i64,
    spacing: f64,
    guess: &dyn Fn(f64) -> [f64; 3],
    eval: &dyn Fn(f64, [f64; 3]) -> f64,
    step_t: f64,
    dvs: f64,
    evals: usize,
) -> Option<(f64, f64, [f64; 3])> {
    let mut refined: Vec<(f64, f64, f64, [f64; 3])> = Vec::new();
    for &c in centers {
        if c + dates as f64 * spacing < now {
            continue;
        }
        let mut cands: Vec<(f64, f64)> = Vec::new();
        for dj in -dates..=dates {
            let d = c + dj as f64 * spacing;
            if d < now {
                continue;
            }
            cands.push((eval(d, guess(d)), d));
        }
        cands.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut best: Option<(f64, f64, [f64; 3])> = None;
        let mut tried: Vec<f64> = Vec::new();
        for &(_, d) in &cands {
            if tried.len() >= 2 || best.as_ref().is_some_and(|b| b.0 < 1.0) {
                break;
            }
            if tried
                .iter()
                .any(|&u| (u - d).abs() < 3.0 * step_t.max(60.0))
            {
                continue;
            }
            tried.push(d);
            let g = guess(d);
            let (y, j) = pattern_search(
                |y| eval(y[0].max(now + 1.0), [y[1], y[2], y[3]]),
                &[d, g[0], g[1], g[2]],
                &[step_t, dvs, dvs, dvs],
                &[1.0, 0.02, 0.02, 0.02],
                evals,
            );
            if best.as_ref().is_none_or(|b| j < b.0) {
                best = Some((j, y[0].max(now + 1.0), [y[1], y[2], y[3]]));
            }
        }
        if let Some((j, d, x)) = best {
            refined.push((c, j, d, x));
        }
    }
    refined.sort_by(|a, b| a.0.total_cmp(&b.0));
    let jb = refined.iter().map(|r| r.1).fold(f64::MAX, f64::min);
    refined
        .into_iter()
        .find(|r| r.1 <= (3.0 * jb).max(0.35))
        .map(|r| (r.1, r.2, r.3))
}

impl Sim {
    /// The body whose gravity the ship flies in (a station's own sphere does not count).
    pub fn nav_center(&self) -> usize {
        match self.ref_body {
            Some(i) if matches!(self.bodies[i].kind, BodyKind::Station) => {
                self.system.bodies[i].orbit.as_ref().map_or(0, |o| o.parent)
            }
            Some(i) => i,
            None => 0,
        }
    }

    fn nav_parent(&self, i: usize) -> Option<usize> {
        self.system.bodies.get(i)?.orbit.as_ref().map(|o| o.parent)
    }

    /// Periapsis radius that an aerobraking pass aims at: deep enough in the air to brake.
    pub fn aero_alt(&self, body: usize) -> f64 {
        self.air_of(body).map_or(0.0, |a| 0.35 * a.top())
    }

    // ---- orbit changes -----------------------------------------------------------------------

    /// The next steps that bring the ship to the orbit (peri_alt, apo_alt) around `body`: one apsis
    /// burn, then the same order again with the next stage (the second burn is computed from the
    /// orbit the first one really made). An empty list: already there.
    pub(crate) fn nav_orbit_steps(
        &self,
        body: usize,
        peri_alt: f64,
        apo_alt: f64,
        stage: u8,
    ) -> Result<Vec<Order>, String> {
        let b = &self.bodies[body];
        if self.ref_body != Some(body) {
            return Err(format!(
                "HORS DE LA SPHERE D'INFLUENCE DE {}",
                b.name.to_uppercase()
            ));
        }
        if b.gm <= 0.0 {
            return Err("CE CORPS N'A PAS DE GRAVITE".into());
        }
        let mu = b.gm;
        let el = self.elements_about(body);
        let rpt = b.radius + peri_alt.min(apo_alt);
        let rat = b.radius + peri_alt.max(apo_alt);
        if rpt <= b.radius * 1.0 {
            return Err("ORBITE SOUS LA SURFACE".into());
        }
        if rat >= self.influence_of(body) * 0.9 {
            return Err("ORBITE AU-DELA DE LA SPHERE D'INFLUENCE".into());
        }
        let tol = |r: f64| 0.01 * r + 1000.0;
        let (rp, ra) = (el.periapsis, el.apoapsis);
        if let Some(ra) = ra {
            if (rp - rpt).abs() < tol(rpt) && (ra - rat).abs() < tol(rat) {
                return Ok(Vec::new());
            }
        }
        if stage >= 6 {
            return Err("L'ORBITE NE CONVERGE PAS".into());
        }
        let air_top = self.air_of(body).map_or(0.0, |a| a.top());
        let node = |when: NodeWhen, pro: f64| {
            Order::Burn(ManeuverNode::new(when, Some(body), pro, 0.0, 0.0))
        };
        let next = Order::Orbit {
            body,
            peri_alt,
            apo_alt,
            stage: stage + 1,
        };
        let v_at = |r: f64| (mu * (2.0 / r - 1.0 / el.a)).max(0.0).sqrt();
        // Burn at the periapsis so that the apoapsis becomes `rat`.
        let at_peri = |rat: f64| {
            let new = (mu * (2.0 / rp - 2.0 / (rp + rat))).sqrt();
            new - v_at(rp)
        };
        let Some(ra) = ra else {
            // An open path: capture at the periapsis.
            if rp <= b.radius {
                return Err("PERIAPSIS SOUS LA SURFACE".into());
            }
            let pro = at_peri(rat.max(rp));
            return Ok(vec![node(NodeWhen::Periapsis, pro), next]);
        };
        let low = rp < b.radius + air_top + 1.0;
        let burn_apo = || {
            let new = (mu * (2.0 / ra - 2.0 / (ra + rpt.min(ra)))).sqrt();
            new - v_at(ra)
        };
        // The periapsis cannot be set above the apoapsis: the burn then makes the orbit circular.
        let rpt_eff = rpt.min(ra);
        let a_needed = (rp - rpt_eff).abs() > tol(rpt_eff);
        // The apoapsis cannot be set below the periapsis: the burn then makes the orbit circular.
        let rat_eff = rat.max(rp);
        let p_needed = (ra - rat_eff).abs() > tol(rat_eff);
        let mut steps = None;
        // Never burn at a periapsis inside the air or the ground: raise it from the apoapsis.
        if low && a_needed {
            steps = Some((NodeWhen::Apoapsis, burn_apo()));
        } else if p_needed && rat >= rp {
            steps = Some((NodeWhen::Periapsis, at_peri(rat_eff)));
        } else if a_needed {
            steps = Some((NodeWhen::Apoapsis, burn_apo()));
        } else if p_needed {
            steps = Some((NodeWhen::Periapsis, at_peri(rat_eff)));
        }
        match steps {
            Some((when, pro)) if pro.abs() >= MIN_BURN => Ok(vec![node(when, pro), next]),
            _ => Err("ORBITE INATTEIGNABLE DEPUIS CETTE ORBITE".into()),
        }
    }

    /// Delta-v (m/s) an orbit change to circular `alt` would cost from the present orbit around
    /// `body` (the two apsis burns, as the planner makes them), and how long it takes (s).
    pub fn orbit_change_cost(
        &self,
        body: usize,
        peri_alt: f64,
        apo_alt: f64,
    ) -> Option<(f64, f64)> {
        let b = &self.bodies[body];
        let mu = b.gm;
        let el = self.elements_about(body);
        let (rpt, rat) = (
            b.radius + peri_alt.min(apo_alt),
            b.radius + peri_alt.max(apo_alt),
        );
        let (rp, ra) = (el.periapsis, el.apoapsis.unwrap_or(el.periapsis));
        let r1 = 0.5 * (rp + ra);
        let r2 = 0.5 * (rpt + rat);
        let h = hohmann(r1, r2, mu);
        // Two apsis burns; their second leg takes half the transfer ellipse.
        Some((h.total, h.time + el.period.unwrap_or(0.0) * 0.5))
    }

    /// Cost of a bi-elliptic change between circular orbits `r1` and `r2` through `rb`: (dv, time).
    pub fn bielliptic_cost(mu: f64, r1: f64, r2: f64, rb: f64) -> (f64, f64) {
        let v = |r: f64, a: f64| (mu * (2.0 / r - 1.0 / a)).max(0.0).sqrt();
        let a1 = 0.5 * (r1 + rb);
        let a2 = 0.5 * (rb + r2);
        let dv1 = v(r1, a1) - (mu / r1).sqrt();
        let dv2 = v(rb, a2) - v(rb, a1);
        let dv3 = (mu / r2).sqrt() - v(r2, a2);
        let t = PI * ((a1.powi(3) / mu).sqrt() + (a2.powi(3) / mu).sqrt());
        (dv1.abs() + dv2.abs() + dv3.abs(), t)
    }

    /// The three burns of a bi-elliptic change from the circular orbit of radius `r1` (the ship's)
    /// to `r2` through `rb`, as nodes at the apsides, followed by a final trim of the orbit.
    pub fn bielliptic_orders(
        &self,
        body: usize,
        r1: f64,
        r2: f64,
        rb: f64,
        alt: f64,
    ) -> Vec<Order> {
        let mu = self.bodies[body].gm;
        let v = |r: f64, a: f64| (mu * (2.0 / r - 1.0 / a)).max(0.0).sqrt();
        let a1 = 0.5 * (r1 + rb);
        let a2 = 0.5 * (rb + r2);
        let n = |when, pro: f64| Order::Burn(ManeuverNode::new(when, Some(body), pro, 0.0, 0.0));
        vec![
            n(NodeWhen::Periapsis, v(r1, a1) - (mu / r1).sqrt()),
            n(NodeWhen::Apoapsis, v(rb, a2) - v(rb, a1)),
            n(NodeWhen::Periapsis, (mu / r2).sqrt() - v(r2, a2)),
            Order::Orbit {
                body,
                peri_alt: alt,
                apo_alt: alt,
                stage: 3,
            },
        ]
    }

    // ---- transfers ---------------------------------------------------------------------------

    /// Penalty-free score of a path: how far its closest approach is from the aimed distance, in
    /// units of the aim (a path that hits another body is rejected).
    fn transfer_score(&self, p: &Prediction, target: usize, aim: f64) -> f64 {
        if let Some((b, _, _)) = p.impact {
            if b != target {
                return 1.0e12;
            }
        }
        match p.closest {
            Some((_, d, _)) => (d - aim).abs() / aim.max(1.0e6),
            None => 1.0e12,
        }
    }

    /// Find the departure burn that takes the ship from here to pass `alt` above `target`, by a
    /// Hohmann guess refined on the n-body prediction.
    pub fn solve_transfer(
        &self,
        target: usize,
        alt: f64,
        aero: bool,
    ) -> Result<TransferSolution, String> {
        self.solve_transfer_from(target, alt, aero, None, None)
    }

    /// The same, refining a solution already found (for a nearby aim) instead of scanning again.
    pub fn solve_transfer_from(
        &self,
        target: usize,
        alt: f64,
        aero: bool,
        hint: Option<&TransferSolution>,
        within: Option<f64>,
    ) -> Result<TransferSolution, String> {
        if self.docked.is_some() {
            return Err("AMARRE : DECOLLER D'ABORD".into());
        }
        if self.ship.stats.thrust <= 0.0 {
            return Err("PAS DE PROPULSION".into());
        }
        let c0 = self.nav_center();
        if target == c0 {
            return Err("DEJA DANS LA SPHERE D'INFLUENCE DE LA CIBLE".into());
        }
        let tb = &self.bodies[target];
        let Some(tparent) = self.nav_parent(target) else {
            return Err("LA CIBLE N'ORBITE RIEN".into());
        };
        let alt_eff = if aero { self.aero_alt(target) } else { alt };
        let aim = tb.radius + alt_eff;
        if tb.radius + alt >= self.influence_of(target) * 0.9 {
            return Err("ALTITUDE AU-DELA DE LA SPHERE D'INFLUENCE DE LA CIBLE".into());
        }
        let now = self.time;
        let ang = |v: Vec3| v.z.atan2(v.x);
        let (levels, centre_above): (usize, usize) = if tparent == c0 {
            (0, c0)
        } else if self.nav_parent(c0) == Some(tparent) {
            (1, tparent)
        } else {
            return Err(
                "ROUTE TROP COMPLEXE POUR L'AUTOMATE : RALLIEZ D'ABORD LE CORPS PARENT DE LA CIBLE"
                    .into(),
            );
        };
        let ta = self.system.bodies[target]
            .orbit
            .as_ref()
            .map_or(0.0, |o| o.a);
        let mu1 = self.bodies[centre_above].gm;
        // The "ship" that leaves for the target: the ship itself in the open; from a parking orbit
        // the planet it circles (a first plan of the heliocentric leg), then the escape.
        let (r_ref, a_ref, mu_ref) = if levels == 0 {
            (
                (self.ship.pos - self.bodies[c0].pos).length(),
                0.0,
                self.bodies[c0].gm,
            )
        } else {
            let a_p = self.system.bodies[c0].orbit.as_ref().map_or(0.0, |o| o.a);
            (a_p, a_p, mu1)
        };
        let h = hohmann(r_ref, ta, mu_ref);
        let horizon = h.time * 1.3 + 3600.0;
        // The windows: where the target leads the ship (or the planet) by the Hohmann angle.
        let (lead_now, n_ship) = if levels == 0 {
            let tr = self.transfer_to(target).ok_or("AUCUN TRANSFERT POSSIBLE")?;
            (tr.lead_now, (mu_ref / r_ref.powi(3)).sqrt())
        } else {
            let rp_pos = self.bodies[c0].pos - self.bodies[centre_above].pos;
            let rt_pos = self.bodies[target].pos - self.bodies[centre_above].pos;
            (
                (ang(rt_pos) - ang(rp_pos)).rem_euclid(TAU),
                (mu_ref / a_ref.powi(3)).sqrt(),
            )
        };
        let n_target = (mu_ref / ta.powi(3)).sqrt();
        let lead_need = hohmann_lead_angle(r_ref, ta).rem_euclid(TAU);
        let wait = transfer_window(lead_now, lead_need, n_ship, n_target).min(1.0e9);
        let synodic = TAU / (n_target - n_ship).abs().max(1e-15);
        let mut centers = vec![now + wait, now + wait - synodic];
        let spacing = 0.001 * h.time;
        let mut dates = (0.12 * h.time / spacing).ceil() as i64;
        if let Some(w) = within {
            // The window is open now: only the departures of the coming day count.
            centers = vec![now + 0.5 * w];
            dates = ((0.5 * w / spacing).ceil() as i64).max(1);
        }
        let dvs = (0.02 * h.dv1).max(0.5);
        let step_t = spacing;
        let finish = |node: ManeuverNode,
                      t_dep: f64,
                      j: f64,
                      leave: Option<(Vec3, f64)>|
         -> Result<TransferSolution, String> {
            let _ = j;
            // The path again, with the fine steps, for the figures that are shown.
            let pred = self.predict_ex(
                self.coast_state(c0, t_dep),
                horizon,
                &[node],
                Some(target),
                STEP_ANGLE,
            );
            let (t_arr, miss, v_rel) = pred.closest.ok_or("PAS D'APPROCHE")?;
            // Good enough for a plan: well inside the sphere of the target (the corrections in
            // flight make the rest).
            // (or a miss that a few tens of m/s over the flight take away).
            let cheap_fix = miss / (t_arr - t_dep).max(1.0) < 60.0;
            if miss > 0.8 * self.influence_of(target) && !cheap_fix {
                return Err(format!(
                    "AUCUNE TRAJECTOIRE TROUVEE VERS {} (ECART {:.0} KM)",
                    self.bodies[target].name.to_uppercase(),
                    miss / 1000.0
                ));
            }
            // Capture at the periapsis from the speed there into a circular orbit of that radius.
            let v_circ = (tb.gm / miss.max(tb.radius)).sqrt();
            Ok(TransferSolution {
                node,
                t_dep,
                t_arr,
                miss,
                aim,
                dv_dep: node.dv(),
                dv_cap: if aero { 0.0 } else { (v_rel - v_circ).abs() },
                v_rel,
                leave,
                pred,
            })
        };
        // The burn from the ship's own orbit at a date: a node there, the path from there.
        let eval_burn = |t: f64, x: [f64; 3]| -> f64 {
            let t = t.max(now + 1.0);
            let node = ManeuverNode::new(NodeWhen::At(t), Some(c0), x[0], x[1], x[2]);
            // From the burn on: the wait on the orbit is a Kepler arc, not thousands of steps.
            let p = self.predict_ex(
                self.coast_state(c0, t),
                horizon,
                &[node],
                Some(target),
                SEARCH_STEP,
            );
            self.transfer_score(&p, target, aim)
        };
        if levels == 0 {
            let r1 = r_ref;
            let guess = |_: f64| [if ta > r1 { h.dv1 } else { -h.dv1 }, 0.0, 0.0];
            let (j, date, x) = match hint {
                Some(hs) => {
                    let x = [hs.node.prograde, hs.node.normal, hs.node.radial];
                    let (y, j) = pattern_search(
                        |y| eval_burn(y[0], [y[1], y[2], y[3]]),
                        &[hs.t_dep, x[0], x[1], x[2]],
                        &[step_t, dvs, dvs, dvs],
                        &[1.0, 0.02, 0.02, 0.02],
                        150,
                    );
                    (j, y[0], [y[1], y[2], y[3]])
                }
                None => search_departure(
                    now, &centers, dates, spacing, &guess, &eval_burn, step_t, dvs, 400,
                )
                .ok_or("AUCUNE FENETRE DE TRANSFERT")?,
            };
            let t_dep = date.max(now + 1.0);
            let node = ManeuverNode::new(NodeWhen::At(t_dep), Some(c0), x[0], x[1], x[2]);
            return finish(node, t_dep, j, None);
        }
        // From the orbit of a planet: first the heliocentric leg of the planet itself (the velocity
        // it must gain), then the escape that gives it.
        let mu_p = self.bodies[c0].gm;
        let r_park = (self.ship.pos - self.bodies[c0].pos).length();
        let r0 = self.ship.pos - self.bodies[c0].pos;
        let hn = r0.cross(self.ship.vel - self.bodies[c0].vel).normalized();
        let r0h = r0.normalized();
        let n_park = (mu_p / r_park.powi(3)).sqrt();
        let leave_at = |date: f64| -> Vec3 {
            let (pp, pv) = self.system.state(c0, date);
            let (cp, cv) = self.system.state(centre_above, date);
            let (rp, vp) = (pp - cp, pv - cv);
            let r = rp.length().max(1.0);
            let tangent = rp.cross(vp).normalized().cross(rp * (1.0 / r));
            let v_dep = (mu1 * (2.0 / r - 2.0 / (r + ta))).max(0.0).sqrt();
            tangent * v_dep - vp
        };
        // The burn that sends the ship away from the planet with the velocity `w` (relative to it),
        // given the date: among the hyperbolas that have that asymptote and pass through the
        // parking orbit (their eccentricity is free, their plane follows from the burn point,
        // which lies on the parking orbit) the cheapest one, as (time of the burn, (prograde,
        // normal, radial) of the parking orbit). The time is the first one after the date, less
        // the delay of the asymptote (the planet leg starts when the ship has left the planet).
        let escape = |date: f64, w: Vec3| -> (f64, [f64; 3]) {
            let date = date.max(now + 1.0);
            let v_inf = w.length().max(1.0);
            let u = w * (1.0 / v_inf);
            let v_c = (mu_p / r_park).sqrt();
            let up = u - hn * u.dot(hn);
            let upl = up.length();
            let upn = if upl > 1e-9 { up * (1.0 / upl) } else { r0h };
            // (cost, burn point, velocity after, for the best hyperbola)
            let mut best: Option<(f64, Vec3, Vec3)> = None;
            for k in 0..400 {
                let e = 1.0 + 10f64.powf(-4.0 + 5.0 * k as f64 / 399.0);
                let pp = mu_p * (e * e - 1.0) / (v_inf * v_inf);
                let cos_nu = (pp / r_park - 1.0) / e;
                if cos_nu.abs() > 1.0 {
                    continue;
                }
                let nu = cos_nu.acos();
                let psi = (-1.0 / e).acos() - nu;
                if psi < 0.0 {
                    continue;
                }
                let cg = psi.cos() / upl.max(1e-9);
                if cg.abs() > 1.0 {
                    continue;
                }
                for sign in [-1.0, 1.0] {
                    let gam = sign * cg.acos();
                    let bp = upn * gam.cos() + hn.cross(upn) * gam.sin();
                    let nvec = bp.cross(u);
                    if nvec.length() < 1e-6 {
                        continue;
                    }
                    let nn = nvec.normalized();
                    let h_ang = (mu_p * pp).sqrt();
                    let v_t = h_ang / r_park;
                    let v_r = mu_p / h_ang * e * nu.sin();
                    let after = bp * v_r + nn.cross(bp) * v_t;
                    let cost = (after - hn.cross(bp) * v_c).length();
                    if best.as_ref().is_none_or(|b| cost < b.0) {
                        best = Some((cost, bp, after));
                    }
                }
            }
            let Some((_, bp, after)) = best else {
                return (
                    date,
                    [(v_inf * v_inf + 2.0 * mu_p / r_park).sqrt() - v_c, 0.0, 0.0],
                );
            };
            let delta = after - hn.cross(bp) * v_c;
            // The asymptote is reached later than the burn: the date is that of the planet leg.
            let (rt, vt) = kepler_propagate(bp * r_park, after, mu_p, 4.0e6);
            let t_a = 4.0e6 - rt.dot(u) / vt.length().max(1.0);
            let date = (date - t_a).max(now + 1.0);
            let alpha = bp.dot(hn.cross(r0h)).atan2(bp.dot(r0h)).rem_euclid(TAU);
            let m = (((date - now) * n_park - alpha) / TAU).ceil().max(0.0);
            let t = (now + (alpha + TAU * m) / n_park).max(now + 1.0);
            (t, [delta.dot(hn.cross(bp)), delta.dot(hn), delta.dot(bp)])
        };
        // Stage one: the planet as a ship, its velocity changed by a free vector at the date.
        let eval_proxy = |d: f64, x: [f64; 3]| -> f64 {
            let d = d.max(now + 1.0);
            let (pp, pv) = self.system.state(c0, d);
            let start = PState {
                pos: pp,
                vel: pv + Vec3::new(x[0], x[1], x[2]),
                t: d,
            };
            let p = self.predict_leaving(start, horizon, Some(target), SEARCH_STEP, c0);
            // Among the paths that hit, the one that changes the planet's velocity least.
            let g = leave_at(d);
            let extra = ((x[0] - g.x).powi(2) + (x[1] - g.y).powi(2) + (x[2] - g.z).powi(2)).sqrt();
            self.transfer_score(&p, target, aim) + 0.02 * extra
        };
        let (w, start_date) = match hint.and_then(|hs| hs.leave.map(|l| (hs.t_dep, l))) {
            Some((t_dep, (u, v))) => (u * v, t_dep - 0.25 * TAU / n_park),
            None => {
                let guess = |d: f64| {
                    let w = leave_at(d.max(now));
                    [w.x, w.y, w.z]
                };
                let (_, d, x) = search_departure(
                    now,
                    &centers,
                    dates,
                    spacing,
                    &guess,
                    &eval_proxy,
                    step_t,
                    dvs,
                    300,
                )
                .ok_or("AUCUNE FENETRE DE TRANSFERT")?;
                (Vec3::new(x[0], x[1], x[2]), d)
            }
        };
        // Stage two: the escape, polished on the real path from the parking orbit.
        let eval_escape = |date: f64, x: [f64; 3]| -> f64 { eval_burn(escape(date, w).0, x) };
        let first = escape(start_date, w).1;
        // The search is restarted from its best point with its steps back: a few rounds reach
        // what one long one does not.
        let mut y = vec![start_date, first[0], first[1], first[2]];
        let mut j = f64::MAX;
        for _ in 0..4 {
            let (y2, j2) = pattern_search(
                |y| eval_escape(y[0], [y[1], y[2], y[3]]),
                &y,
                &[600.0, 2.0, 2.0, 2.0],
                &[1.0, 0.01, 0.01, 0.01],
                if hint.is_some() { 150 } else { 300 },
            );
            let better = j2 < j;
            if better {
                (y, j) = (y2, j2);
            }
            if !better || j < 0.3 || hint.is_some() {
                break;
            }
        }
        let t_dep = escape(y[0], w).0;
        let node = ManeuverNode::new(NodeWhen::At(t_dep), Some(c0), y[1], y[2], y[3]);
        finish(node, t_dep, j, Some((w.normalized(), w.length())))
    }

    /// The steps of a transfer to a circular orbit of `alt` around `target`.
    pub(crate) fn nav_transfer_steps(
        &self,
        target: usize,
        alt: f64,
        aero: bool,
        soon: bool,
    ) -> Result<Vec<Order>, String> {
        let sol = self.solve_transfer_from(target, alt, aero, None, soon.then_some(86_400.0))?;
        // A window far off: wait for it, then plan again from where the ship really is (the orbit
        // it waits on is not a Kepler arc for days: the tides of the star move its phase).
        if !soon && sol.t_dep - self.time > 86_400.0 {
            return Ok(vec![
                Order::WaitUntil(sol.t_dep - 7_200.0),
                Order::Depart { target, alt, aero },
            ]);
        }
        let total = sol.dv_dep + sol.dv_cap + 8.0;
        let fuel_after = self.ship.fuel - self.fuel_for_dv(total);
        if total > self.ship.delta_v()
            || fuel_after < self.nav.limits.reserve * self.ship.stats.fuel_cap
        {
            return Err(format!(
                "DELTA-V INSUFFISANT : {:.0} M/S NECESSAIRES, {:.0} DISPONIBLES",
                total,
                self.ship.delta_v()
            ));
        }
        let eta = (sol.t_arr - sol.t_dep).max(60.0);
        let aim_alt = if aero { self.aero_alt(target) } else { alt };
        let correct = Order::Correct {
            target,
            alt: aim_alt,
            t_arr: sol.t_arr,
        };
        // Corrections on the way, the last one close to the arrival, where the errors of the
        // flight since the first one have piled up.
        let mut steps = vec![
            Order::Burn(sol.node),
            correct.clone(),
            Order::WaitUntil(sol.t_dep + 0.5 * eta),
            correct.clone(),
            Order::WaitUntil(sol.t_arr - 0.12 * eta),
            correct,
            Order::Coast {
                body: target,
                max_s: 0.8 * eta + 5.0 * 86_400.0,
            },
        ];
        if aero {
            steps.push(Order::Aerobrake { body: target });
        }
        steps.push(Order::Orbit {
            body: target,
            peri_alt: alt,
            apo_alt: alt,
            stage: 0,
        });
        Ok(steps)
    }

    /// A small burn that makes the predicted path pass `alt` above `target`, or None when the
    /// course is good enough (or the ship is already in the sphere of the target).
    pub(crate) fn nav_correction(
        &self,
        target: usize,
        alt: f64,
        t_arr: f64,
    ) -> Result<Option<ManeuverNode>, String> {
        if self.ref_body == Some(target) {
            return Ok(None);
        }
        let tb = &self.bodies[target];
        let aim = tb.radius + alt;
        let rel = tb.pos - self.ship.pos;
        let d = rel.length();
        let closing = ((self.ship.vel - tb.vel).dot(rel * (1.0 / d.max(1.0)))).max(20.0);
        let horizon = if t_arr > self.time {
            1.2 * (t_arr - self.time) + 3600.0
        } else {
            (1.6 * d / closing + 3600.0).clamp(3600.0, 600.0 * 86_400.0)
        };
        let p0 = self.predict_ship(horizon, &[], Some(target));
        let j0 = self.transfer_score(&p0, target, aim);
        if j0 < 0.08 {
            return Ok(None);
        }
        let c0 = self.nav_center();
        let t_burn = self.time + self.nav_align_lead() + 60.0;
        let eval = |x: &[f64]| {
            let node = ManeuverNode::new(NodeWhen::At(t_burn), Some(c0), x[0], x[1], x[2]);
            let p = self.predict_ship_ex(horizon, &[node], Some(target), 0.012);
            self.transfer_score(&p, target, aim)
        };
        // The first step is what the miss asks of a burn now over the time left.
        let (eta0, d0) = p0
            .closest
            .map_or((horizon, 1.0e9), |c| (c.0 - self.time, (c.1 - aim).abs()));
        let s0 = (0.3 * d0 / eta0.max(1000.0)).clamp(1.0, 300.0);
        let (x, j) = pattern_search(
            eval,
            &[0.0, 0.0, 0.0],
            &[s0, s0, s0],
            &[0.004, 0.004, 0.004],
            300,
        );
        let node = ManeuverNode::new(NodeWhen::At(t_burn), Some(c0), x[0], x[1], x[2]);
        if j >= j0 * 0.9 {
            return Ok(None);
        }
        if node.dv() < 0.2 {
            return Ok(None);
        }
        if node.dv() > 0.2 * self.ship.delta_v() {
            return Err("CORRECTION TROP COUTEUSE : TRAJECTOIRE PERDUE".into());
        }
        Ok(Some(node))
    }

    // ---- the options offered ---------------------------------------------------------------

    /// What the path will cost the crew and the ship, from its prediction.
    fn path_risks(
        &self,
        p: &Prediction,
        duration_s: f64,
        origin: usize,
        target: usize,
    ) -> Vec<String> {
        let mut risks = Vec::new();
        if let Some((b, _, _)) = p.impact {
            if b != target {
                risks.push(format!(
                    "COLLISION SUR {}",
                    self.bodies[b].name.to_uppercase()
                ));
            }
        }
        let n = p.points.len();
        if n > 0 {
            let samples = 24usize.min(n);
            let mut sum = 0.0;
            let mut nearest_star = f64::MAX;
            for k in 0..samples {
                let pt = p.points[k * (n - 1) / samples.max(1)];
                sum += self.natural_dose_at(pt.pos).total();
                nearest_star = nearest_star.min(pt.pos.length());
            }
            let rate = sum / samples as f64 * shield_factor(self.effective.armor);
            let dose = rate * duration_s / 3600.0;
            if dose > 0.05 * crate::alerts::DOSE_TOTAL_WARNING {
                risks.push(format!("RADIATION : {dose:.2} GY AU TOTAL"));
            }
            if nearest_star < 0.25 * vector_world::orbit::AU {
                risks.push("PASSAGE PRES DE L'ETOILE : CHALEUR".into());
            }
        }
        // Belts of asteroids: the meteoroids that hit the ship while it crosses them.
        let mut in_belt = 0.0;
        for w in p.points.windows(2) {
            let r = w[1].pos.length();
            if self
                .system
                .belts
                .iter()
                .any(|b| r >= b.inner && r <= b.outer)
            {
                in_belt += w[1].t - w[0].t;
            }
        }
        if in_belt > 0.0 {
            let hits = in_belt / crate::science::METEOR_MEAN_INTERVAL * self.rules.event_freq;
            let share = hits * 70.0 / self.ship.stats.hull_max.max(1.0);
            risks.push(format!(
                "CEINTURE D'ASTEROIDES : {:.0} J DE TRAVERSEE, ~{:.0} IMPACTS ({:.0} % DE LA COQUE)",
                in_belt / 86_400.0,
                hits,
                share * 100.0
            ));
        }
        let mut seen: Vec<usize> = Vec::new();
        for e in &p.events {
            if e.kind == PredKind::SoiEnter
                && e.body != target
                && e.body != origin
                && !seen.contains(&e.body)
            {
                seen.push(e.body);
                risks.push(format!(
                    "TRAVERSE LE PUITS DE {}",
                    self.bodies[e.body].name.to_uppercase()
                ));
            }
        }
        risks
    }

    /// The ways to go to a circular orbit of `alt` around `target`, each with its cost.
    pub fn plan_transfer_options(&self, target: usize, alt: f64) -> Vec<PlanOption> {
        let mut out = Vec::new();
        let tname = self.bodies[target].name.to_uppercase();
        let origin = self.nav_center();
        // The body must be one the computer can plan for.
        let direct = self.solve_transfer(target, alt, false);
        match &direct {
            Ok(sol) => {
                let dv = sol.dv_dep + sol.dv_cap + 8.0;
                let wait = (sol.t_dep - self.time).max(0.0);
                let dur = sol.t_arr - self.time;
                let mut risks = self.path_risks(&sol.pred, dur, origin, target);
                let fuel = self.fuel_for_dv(dv);
                let mut ok = true;
                let mut why = None;
                if dv > self.ship.delta_v() {
                    ok = false;
                    why = Some(format!(
                        "DELTA-V INSUFFISANT ({:.0} DISPONIBLES)",
                        self.ship.delta_v()
                    ));
                } else if self.ship.fuel - fuel < self.nav.limits.reserve * self.ship.stats.fuel_cap
                {
                    risks.push("LA RESERVE DE CARBURANT SERAIT ENTAMEE".into());
                }
                out.push(PlanOption {
                    kind: PlanKind::Hohmann,
                    label: format!("TRANSFERT DE HOHMANN VERS {tname}"),
                    dv,
                    fuel,
                    wait,
                    duration: dur,
                    risks,
                    ok,
                    why,
                    orders: vec![Order::Transfer {
                        target,
                        alt,
                        aero: false,
                    }],
                });
                // With the air as a brake: no capture burn.
                if self.air_of(target).is_some() {
                    let hull_ok = self.ship.hull >= 0.6 * self.ship.stats.hull_max;
                    if let Ok(sa) = self.solve_transfer_from(target, alt, true, Some(sol), None) {
                        let dva = sa.dv_dep + 8.0 + 0.05 * sa.dv_cap;
                        let mut r = self.path_risks(&sa.pred, sa.t_arr - self.time, origin, target);
                        r.push(format!(
                            "CHALEUR D'ENTREE : PASSAGE A {:.0} KM DANS L'AIR",
                            self.aero_alt(target) / 1000.0
                        ));
                        out.push(PlanOption {
                            kind: PlanKind::Aero,
                            label: format!("AEROFREINAGE SUR {tname}"),
                            dv: dva,
                            fuel: self.fuel_for_dv(dva),
                            wait: (sa.t_dep - self.time).max(0.0),
                            duration: sa.t_arr - self.time + 6.0 * 3600.0,
                            risks: r,
                            ok: hull_ok && dva <= self.ship.delta_v(),
                            why: (!hull_ok).then(|| "COQUE TROP ABIMEE POUR L'AEROFREINAGE".into()),
                            orders: vec![Order::Transfer {
                                target,
                                alt,
                                aero: true,
                            }],
                        });
                    }
                }
                // An indicative gravity assist: a massive world on the way.
                if let Some(opt) = self.assist_estimate(target, sol, origin) {
                    out.push(opt);
                }
            }
            Err(e) => out.push(PlanOption {
                kind: PlanKind::Hohmann,
                label: format!("TRANSFERT VERS {tname}"),
                dv: 0.0,
                fuel: 0.0,
                wait: 0.0,
                duration: 0.0,
                risks: Vec::new(),
                ok: false,
                why: Some(e.clone()),
                orders: Vec::new(),
            }),
        }
        // The straight line of the assisted autopilot.
        match crate::autopilot::Autopilot::new(self, target) {
            Ok(ap) => {
                let rel = (self.ship.vel - self.bodies[target].vel).length();
                let dv = 2.0 * (ap.a_cruise * ap.eta() / 2.0) + rel;
                out.push(PlanOption {
                    kind: PlanKind::Straight,
                    label: format!("PILOTE AUTO EN LIGNE DROITE VERS {tname}"),
                    dv,
                    fuel: self.fuel_for_dv(dv),
                    wait: 0.0,
                    duration: ap.eta(),
                    risks: vec!["IGNORE LES PUITS DE GRAVITE SUR LE TRAJET".into()],
                    ok: dv <= self.ship.delta_v(),
                    why: None,
                    orders: vec![Order::Approach { body: target }],
                });
            }
            Err(e) => out.push(PlanOption {
                kind: PlanKind::Straight,
                label: format!("PILOTE AUTO EN LIGNE DROITE VERS {tname}"),
                dv: 0.0,
                fuel: 0.0,
                wait: 0.0,
                duration: 0.0,
                risks: Vec::new(),
                ok: false,
                why: Some(e.to_uppercase()),
                orders: Vec::new(),
            }),
        }
        out
    }

    /// An indicative estimate of a gravity assist on the way: a massive world of the same centre
    /// that bends the path and spares part of the departure burn. The computer does not fly it.
    fn assist_estimate(
        &self,
        target: usize,
        sol: &TransferSolution,
        origin: usize,
    ) -> Option<PlanOption> {
        let centre = self.nav_parent(target)?;
        let mut best: Option<(usize, f64)> = None;
        for (i, b) in self.bodies.iter().enumerate() {
            if i == target || i == origin || self.nav_parent(i) != Some(centre) {
                continue;
            }
            if !matches!(b.kind, BodyKind::Planet(_)) || b.gm < 1.0e15 {
                continue;
            }
            let v_inf = sol.dv_dep.max(1.0);
            let defl = flyby_deflection(v_inf, 1.5 * b.radius, b.gm);
            let gain = flyby_dv(v_inf, defl).min(0.3 * sol.dv_dep);
            if best.is_none_or(|x| gain > x.1) {
                best = Some((i, gain));
            }
        }
        let (i, gain) = best?;
        if gain < 0.05 * sol.dv_dep {
            return None;
        }
        let dv = (sol.dv_dep - gain + sol.dv_cap + 8.0).max(0.0);
        Some(PlanOption {
            kind: PlanKind::Assist,
            label: format!(
                "ASSISTANCE VIA {} : GAIN ESTIME {:.0} M/S (INDICATIF)",
                self.bodies[i].name.to_uppercase(),
                gain
            ),
            dv,
            fuel: self.fuel_for_dv(dv),
            wait: (sol.t_dep - self.time).max(0.0),
            duration: (sol.t_arr - self.time) * 1.4,
            risks: vec![
                "ESTIMATION : A PILOTER A LA MAIN".into(),
                format!(
                    "PASSAGE A 1,5 RAYON DE {}",
                    self.bodies[i].name.to_uppercase()
                ),
            ],
            ok: false,
            why: Some("INDICATIF : L'AUTOMATE NE FAIT PAS L'ASSISTANCE".into()),
            orders: Vec::new(),
        })
    }

    /// The ways to change the orbit around `body`: the two apsis burns and, when the radii are
    /// far apart, the bi-elliptic change.
    pub fn plan_orbit_options(&self, body: usize, peri_alt: f64, apo_alt: f64) -> Vec<PlanOption> {
        let mut out = Vec::new();
        let b = &self.bodies[body];
        let name = b.name.to_uppercase();
        let circ = (peri_alt - apo_alt).abs() < 1.0;
        let label = if circ {
            format!("ORBITE CIRCULAIRE {} A {:.0} KM", name, peri_alt / 1000.0)
        } else {
            format!(
                "ORBITE {} {:.0} X {:.0} KM",
                name,
                peri_alt / 1000.0,
                apo_alt / 1000.0
            )
        };
        let order = Order::Orbit {
            body,
            peri_alt,
            apo_alt,
            stage: 0,
        };
        if self.ref_body != Some(body) {
            out.push(PlanOption {
                kind: PlanKind::Orbit,
                label,
                dv: 0.0,
                fuel: 0.0,
                wait: 0.0,
                duration: 0.0,
                risks: Vec::new(),
                ok: false,
                why: Some("HORS DE LA SPHERE D'INFLUENCE : FAITES D'ABORD UN TRANSFERT".into()),
                orders: Vec::new(),
            });
            return out;
        }
        let (dv, t) = self
            .orbit_change_cost(body, peri_alt, apo_alt)
            .unwrap_or((0.0, 0.0));
        let mut risks = Vec::new();
        if b.radius + peri_alt.min(apo_alt) < b.radius + self.air_of(body).map_or(0.0, |a| a.top())
        {
            risks.push("PERIAPSIS DANS L'ATMOSPHERE : FREINAGE ET CHALEUR".into());
        }
        out.push(PlanOption {
            kind: PlanKind::Orbit,
            label,
            dv,
            fuel: self.fuel_for_dv(dv),
            wait: 0.0,
            duration: t,
            risks: risks.clone(),
            ok: dv <= self.ship.delta_v(),
            why: None,
            orders: vec![order],
        });
        // Far apart circular radii: through a high apoapsis it costs less.
        let el = self.elements_about(body);
        let r1 = 0.5 * (el.periapsis + el.apoapsis.unwrap_or(el.periapsis));
        let r2 = b.radius + 0.5 * (peri_alt + apo_alt);
        let (lo, hi) = (r1.min(r2), r1.max(r2));
        if circ && el.bound && hi / lo > BIELLIPTIC_RATIO && self.influence_of(body) > 3.0 * hi {
            let rb = (hi * 4.0).min(0.5 * self.influence_of(body));
            let (dvb, tb) = Self::bielliptic_cost(b.gm, r1, r2, rb);
            out.push(PlanOption {
                kind: PlanKind::BiElliptic,
                label: format!("BI-ELLIPTIQUE VIA {:.0} KM", (rb - b.radius) / 1000.0),
                dv: dvb,
                fuel: self.fuel_for_dv(dvb),
                wait: 0.0,
                duration: tb,
                risks,
                ok: dvb <= self.ship.delta_v() && dvb < dv,
                why: (dvb >= dv).then(|| "PAS MOINS CHER QUE LE TRANSFERT DE HOHMANN".into()),
                orders: self.bielliptic_orders(body, r1, r2, rb, peri_alt),
            });
        }
        out
    }

    // ---- FTL routes ---------------------------------------------------------------------------

    /// The safest chain of jumps from here to `dest` through the stars in range of each other:
    /// the cost of a hop is its length plus a penalty for dangerous stars (hot, dead or collapsed)
    /// and for hostile territory.
    pub fn plan_route(&self, dest: StarRef) -> Result<Route, String> {
        let range = self.jump_range();
        if range <= 0.0 {
            return Err("AUCUN PROPULSEUR FTL INSTALLE".into());
        }
        let goal = self.galaxy.star(dest).ok_or("ETOILE INCONNUE")?;
        if dest == self.here {
            return Err("DEJA DANS CE SYSTEME".into());
        }
        let direct = goal.distance_to(self.here_pos);
        let bound = 2.5 * direct + 2.0 * range;
        let nfac = self.data.factions.len();
        let hostile = |pos: [f64; 3]| -> bool {
            self.galaxy
                .territory(pos, nfac)
                .is_some_and(|f| self.reputation_with(Some(f)) <= crate::economy::HOSTILE_REP)
        };
        // Dijkstra over the stars met (a few hundred at most).
        struct Node {
            id: StarRef,
            pos: [f64; 3],
            cost: f64,
            from: Option<usize>,
            danger: f64,
            done: bool,
        }
        let mut nodes: Vec<Node> = vec![Node {
            id: self.here,
            pos: self.here_pos,
            cost: 0.0,
            from: None,
            danger: 0.0,
            done: false,
        }];
        let mut found: Option<usize> = None;
        for _ in 0..260 {
            let cur = nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| !n.done)
                .min_by(|a, b| a.1.cost.total_cmp(&b.1.cost))
                .map(|(i, _)| i);
            let Some(ci) = cur else { break };
            nodes[ci].done = true;
            if nodes[ci].id == dest {
                found = Some(ci);
                break;
            }
            let (cpos, ccost) = (nodes[ci].pos, nodes[ci].cost);
            for s in self.galaxy.nearby(cpos, range) {
                if s.id == nodes[ci].id || !self.is_seen(s.pos) {
                    continue;
                }
                if s.distance_to(self.here_pos) + s.distance_to(goal.pos) > bound {
                    continue;
                }
                let d = s.distance_to(cpos);
                let mut danger = star_danger(s.class());
                if s.id != dest && hostile(s.pos) {
                    danger += 3.0;
                }
                let cost = ccost + d + danger;
                match nodes.iter().position(|n| n.id == s.id) {
                    Some(k) => {
                        if !nodes[k].done && cost < nodes[k].cost {
                            nodes[k].cost = cost;
                            nodes[k].from = Some(ci);
                            nodes[k].danger = danger;
                        }
                    }
                    None => nodes.push(Node {
                        id: s.id,
                        pos: s.pos,
                        cost,
                        from: Some(ci),
                        danger,
                        done: false,
                    }),
                }
            }
        }
        let Some(mut k) = found else {
            return Err("AUCUNE ROUTE PAR LES ETOILES CONNUES".into());
        };
        let mut chain = Vec::new();
        while let Some(f) = nodes[k].from {
            chain.push(k);
            k = f;
        }
        chain.reverse();
        let mut route = Route {
            ok: true,
            ..Default::default()
        };
        let mut prev = self.here_pos;
        for &i in &chain {
            let n = &nodes[i];
            let d = ((n.pos[0] - prev[0]).powi(2)
                + (n.pos[1] - prev[1]).powi(2)
                + (n.pos[2] - prev[2]).powi(2))
            .sqrt();
            route.hops.push(Hop {
                star: n.id,
                dist: d,
                danger: n.danger,
            });
            route.total_ly += d;
            route.fuel += self.jump_fuel_cost(d);
            route.antimatter += self.warp_cost_kg(d);
            route.seconds += d * crate::voyage::HOURS_PER_LY * 3600.0 + crate::voyage::CHARGE_TIME;
            route.danger += n.danger;
            prev = n.pos;
        }
        if route.fuel > self.ship.fuel {
            route.ok = false;
            route.why = Some(format!(
                "CARBURANT INSUFFISANT : {:.0} KG NECESSAIRES, {:.0} A BORD",
                route.fuel, self.ship.fuel
            ));
        } else if route.antimatter > self.antimatter_stored() {
            route.ok = false;
            route.why = Some("ANTIMATIERE INSUFFISANTE POUR LA BULLE".into());
        }
        Ok(route)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pattern_search_finds_the_minimum_of_a_bowl() {
        let f = |x: &[f64]| (x[0] - 3.0).powi(2) + 4.0 * (x[1] + 1.0).powi(2);
        let (x, v) = pattern_search(f, &[0.0, 0.0], &[1.0, 1.0], &[1e-4, 1e-4], 400);
        assert!(
            (x[0] - 3.0).abs() < 1e-3 && (x[1] + 1.0).abs() < 1e-3,
            "{x:?}"
        );
        assert!(v < 1e-5);
    }

    #[test]
    fn a_bi_elliptic_change_beats_hohmann_only_beyond_the_known_ratio() {
        let mu = 3.986e14;
        let r1 = 7.0e6;
        // Ratio 20: bi-elliptic through 10 r2 is cheaper; ratio 5: Hohmann is.
        let big = hohmann(r1, 20.0 * r1, mu).total;
        let (bi, _) = Sim::bielliptic_cost(mu, r1, 20.0 * r1, 200.0 * r1);
        assert!(bi < big, "{bi} vs {big}");
        let small = hohmann(r1, 5.0 * r1, mu).total;
        let (bi5, _) = Sim::bielliptic_cost(mu, r1, 5.0 * r1, 50.0 * r1);
        assert!(bi5 > small, "{bi5} vs {small}");
    }
}
