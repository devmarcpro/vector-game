//! Predicted trajectory and manoeuvre nodes. The ship is integrated forward under the pull of every
//! massive body (n-body, the bodies following their analytic orbits), with the planned burns applied
//! as impulses at their time. The same integration answers every question the pilot asks of a
//! course: where the periapsis and apoapsis are, when the ship enters or leaves a sphere of
//! influence, how close it passes a target, whether it will hit a world. The map, the cockpit panel,
//! the planner and the safety monitor all read this one prediction.
//!
//! A manoeuvre node is a burn in the frame of the orbit (prograde, normal, radial) at a time that
//! is either absolute or "at the next periapsis / apoapsis / node"; the times are resolved by the
//! integration itself, so a chain of nodes stays right whatever the earlier ones did.
use crate::navigation::elements;
use crate::{Sim, G0};
use serde::{Deserialize, Serialize};
use vector_core::Vec3;
use vector_world::BodyKind;

/// Most points kept for drawing a predicted path.
pub const PRED_MAX_POINTS: usize = 800;
/// Hard bound on integration steps of one prediction.
const MAX_STEPS: usize = 400_000;
/// Smallest and largest integration step (s).
const DT_MIN: f64 = 0.25;
const DT_MAX: f64 = 43_200.0;
/// Angle (rad) of the orbit the step may cover: the bound on the integration error.
pub const STEP_ANGLE: f64 = 0.008;

/// When a node burns.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum NodeWhen {
    /// An absolute time (s since the epoch).
    At(f64),
    /// The next periapsis / apoapsis of the orbit around the node's body.
    Periapsis,
    Apoapsis,
    /// The next crossing of the ecliptic plane upwards / downwards.
    Ascending,
    Descending,
}

impl NodeWhen {
    pub fn label(self) -> &'static str {
        match self {
            NodeWhen::At(_) => "A LA DATE",
            NodeWhen::Periapsis => "AU PERIAPSIS",
            NodeWhen::Apoapsis => "A L'APOAPSIS",
            NodeWhen::Ascending => "NOEUD ASCENDANT",
            NodeWhen::Descending => "NOEUD DESCENDANT",
        }
    }
}

/// A planned burn: delta-v along the prograde, normal and radial axes of the orbit around `body`
/// (None: the body that holds the ship at that time).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManeuverNode {
    pub when: NodeWhen,
    pub body: Option<usize>,
    pub prograde: f64,
    pub normal: f64,
    pub radial: f64,
    /// Delta-v (m/s) already burnt (progress of an interrupted burn).
    pub done: f64,
}

impl ManeuverNode {
    pub fn new(
        when: NodeWhen,
        body: Option<usize>,
        prograde: f64,
        normal: f64,
        radial: f64,
    ) -> Self {
        ManeuverNode {
            when,
            body,
            prograde,
            normal,
            radial,
            done: 0.0,
        }
    }

    /// Magnitude of the burn (m/s).
    pub fn dv(&self) -> f64 {
        (self.prograde * self.prograde + self.normal * self.normal + self.radial * self.radial)
            .sqrt()
    }

    /// Delta-v still to burn.
    pub fn remaining(&self) -> f64 {
        (self.dv() - self.done).max(0.0)
    }

    /// The burn as a vector in the world frame, from the position and velocity relative to the
    /// body at the time of the burn.
    pub fn vector(&self, rel_pos: Vec3, rel_vel: Vec3) -> Vec3 {
        let (pro, nor, rad) = frame(rel_pos, rel_vel);
        pro * self.prograde + nor * self.normal + rad * self.radial
    }
}

/// Unit vectors (prograde, normal, radial-out) of the orbit through this state.
pub fn frame(rel_pos: Vec3, rel_vel: Vec3) -> (Vec3, Vec3, Vec3) {
    let pro = if rel_vel.length() > 1e-9 {
        rel_vel.normalized()
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    };
    let mut nor = rel_pos.cross(rel_vel);
    if nor.length() < 1e-9 {
        nor = Vec3::new(0.0, -1.0, 0.0);
    }
    let nor = nor.normalized();
    let rad = pro.cross(nor);
    (pro, nor, rad)
}

/// The state after `dt` seconds on the conic through `r0`, `v0` (relative to a body of gravitational
/// parameter `mu`, nothing else pulling): universal variables, so that ellipses and hyperbolas
/// are one formula.
pub fn kepler_propagate(r0: Vec3, v0: Vec3, mu: f64, dt: f64) -> (Vec3, Vec3) {
    let r0l = r0.length().max(1e-9);
    let sm = mu.sqrt();
    let alpha = 2.0 / r0l - v0.dot(v0) / mu;
    let stumpff = |z: f64| -> (f64, f64) {
        if z > 1e-9 {
            let s = z.sqrt();
            ((1.0 - s.cos()) / z, (s - s.sin()) / (s * z))
        } else if z < -1e-9 {
            let s = (-z).sqrt();
            ((s.cosh() - 1.0) / -z, (s.sinh() - s) / (s * -z))
        } else {
            (0.5 - z / 24.0, 1.0 / 6.0 - z / 120.0)
        }
    };
    let rv = r0.dot(v0);
    let mut chi = if alpha > 1e-12 {
        sm * dt * alpha
    } else if alpha < -1e-12 {
        let a = -1.0 / alpha;
        let den = rv + dt.signum() * (mu * a).sqrt() * (1.0 - r0l * alpha);
        let arg = (-2.0 * mu * alpha * dt) / den;
        if arg > 1e-9 && arg.is_finite() {
            dt.signum() * a.sqrt() * arg.ln()
        } else {
            sm * dt / r0l
        }
    } else {
        sm * dt / r0l
    };
    for _ in 0..60 {
        let z = alpha * chi * chi;
        let (c, s) = stumpff(z);
        let f =
            rv / sm * chi * chi * c + (1.0 - alpha * r0l) * chi.powi(3) * s + r0l * chi - sm * dt;
        let df = rv / sm * chi * (1.0 - z * s) + (1.0 - alpha * r0l) * chi * chi * c + r0l;
        let ddf = rv / sm * (1.0 - z * c) + (1.0 - alpha * r0l) * chi * (1.0 - z * s);
        let disc = (16.0 * df * df - 20.0 * f * ddf).abs().sqrt();
        let step = 5.0 * f / (df + df.signum() * disc);
        chi -= step;
        if step.abs() < 1e-10 * chi.abs().max(1.0) {
            break;
        }
    }
    let z = alpha * chi * chi;
    let (c, s) = stumpff(z);
    let f = 1.0 - chi * chi / r0l * c;
    let g = dt - chi.powi(3) / sm * s;
    let r = r0 * f + v0 * g;
    let rl = r.length().max(1e-9);
    let fd = sm / (rl * r0l) * chi * (z * s - 1.0);
    let gd = 1.0 - chi * chi / rl * c;
    (r, r0 * fd + v0 * gd)
}

/// Position, velocity (star frame) and time.
#[derive(Clone, Copy, Debug)]
pub struct PState {
    pub pos: Vec3,
    pub vel: Vec3,
    pub t: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct PredPoint {
    pub t: f64,
    pub pos: Vec3,
    pub center: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PredKind {
    Periapsis,
    Apoapsis,
    /// Enters the sphere of influence of `body`.
    SoiEnter,
    /// Leaves the sphere of influence of `body`.
    SoiExit,
    /// Closest approach to the target (`value` is the distance, m).
    Closest,
    /// Hits `body` (`value` is the speed, m/s).
    Impact,
    /// Burn number `index` of the list of nodes.
    Node(usize),
}

#[derive(Clone, Copy, Debug)]
pub struct PredEvent {
    pub t: f64,
    pub kind: PredKind,
    pub body: usize,
    /// The body whose frame the position belongs to (the one that holds the ship): a map that
    /// follows the planet draws the position as an offset from where that body is now.
    pub center: usize,
    /// Distance from the body's centre (apsides), distance to the target (closest approach), speed
    /// (impact), delta-v (node).
    pub value: f64,
    pub pos: Vec3,
}

/// Where a node ended up once resolved: the time, the state just before the burn.
#[derive(Clone, Copy, Debug)]
pub struct Resolved {
    pub t: f64,
    /// The time of the state (the burn starts earlier than the date of its node).
    pub ts: f64,
    pub pos: Vec3,
    pub vel: Vec3,
    pub body: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Prediction {
    pub points: Vec<PredPoint>,
    pub events: Vec<PredEvent>,
    pub nodes: Vec<Option<Resolved>>,
    pub end: Option<PState>,
    /// Closest approach to the target: (time, distance, relative speed).
    pub closest: Option<(f64, f64, f64)>,
    pub impact: Option<(usize, f64, f64)>,
    pub dv_total: f64,
    /// Integration steps taken (a cost indicator).
    pub steps: usize,
}

/// What a prediction needs of the bodies: those that pull, those that hold the ship in their sphere.
struct Field {
    /// Bodies tracked: (index, gm, radius, sphere of influence, pulls).
    tracked: Vec<(usize, f64, f64, f64, bool)>,
}

impl Sim {
    fn pred_field(&self, target: Option<usize>, skip: Option<usize>) -> Field {
        let mut tracked = Vec::new();
        for (i, b) in self.bodies.iter().enumerate() {
            let pulls = Some(i) != skip
                && b.gm > 0.0
                && !matches!(
                    b.kind,
                    BodyKind::Station | BodyKind::Asteroid(_) | BodyKind::Anomaly(_)
                );
            if pulls || Some(i) == target {
                let soi = if matches!(b.kind, BodyKind::Station) || Some(i) == skip {
                    0.0
                } else {
                    self.influence_of(i)
                };
                tracked.push((i, if pulls { b.gm } else { 0.0 }, b.radius, soi, pulls));
            }
        }
        Field { tracked }
    }

    fn field_states(&self, f: &Field, t: f64) -> Vec<(Vec3, Vec3)> {
        f.tracked
            .iter()
            .map(|&(i, ..)| self.system.state(i, t))
            .collect()
    }

    /// The body (index) whose sphere of influence holds `p`, else the star.
    fn field_center(f: &Field, bs: &[(Vec3, Vec3)], p: Vec3) -> usize {
        let mut best: Option<(usize, f64)> = None;
        for (k, &(i, _, _, soi, _)) in f.tracked.iter().enumerate() {
            if soi > 0.0 && (p - bs[k].0).length() < soi && best.is_none_or(|(_, s)| soi < s) {
                best = Some((i, soi));
            }
        }
        best.map_or(0, |(i, _)| i)
    }

    fn field_slot(f: &Field, body: usize) -> Option<usize> {
        f.tracked.iter().position(|t| t.0 == body)
    }

    fn field_accel(f: &Field, bs: &[(Vec3, Vec3)], p: Vec3) -> Vec3 {
        let mut a = Vec3::ZERO;
        for (k, &(_, gm, _, _, pulls)) in f.tracked.iter().enumerate() {
            if !pulls {
                continue;
            }
            let d = bs[k].0 - p;
            let r2 = d.dot(d).max(1.0);
            a = a + d * (gm / (r2 * r2.sqrt()));
        }
        a
    }

    /// Largest step that keeps the integration inside `STEP_ANGLE` of every orbit around any body
    /// and out of the ground.
    fn field_dt(f: &Field, bs: &[(Vec3, Vec3)], p: Vec3, v: Vec3, step_angle: f64) -> f64 {
        let mut dt = DT_MAX;
        for (k, &(_, gm, radius, _, pulls)) in f.tracked.iter().enumerate() {
            if !pulls || gm < 1e6 {
                continue;
            }
            let d = (p - bs[k].0).length().max(1.0);
            dt = dt.min(step_angle * (d * d * d / gm).sqrt());
            let vrel = (v - bs[k].1).length().max(1.0);
            dt = dt.min(0.1 * (d - radius).max(1.0) / vrel);
        }
        dt.max(DT_MIN)
    }

    /// Predict the path from `start` for `horizon` seconds with the burns of `nodes` (applied in
    /// order); `target` is the body whose closest approach is tracked.
    pub fn predict(
        &self,
        start: PState,
        horizon: f64,
        nodes: &[ManeuverNode],
        target: Option<usize>,
    ) -> Prediction {
        self.predict_ex(start, horizon, nodes, target, STEP_ANGLE)
    }

    /// The same with a chosen step (rad of orbit per step): the planner searches with coarse
    /// steps and checks with the fine ones.
    pub fn predict_ex(
        &self,
        start: PState,
        horizon: f64,
        nodes: &[ManeuverNode],
        target: Option<usize>,
        step_angle: f64,
    ) -> Prediction {
        self.predict_core(start, horizon, nodes, target, step_angle, None)
    }

    /// The path of a probe that starts at the heart of body  (the planet whose velocity it
    /// shares): that body does not pull it, the planner's first plan of a heliocentric leg.
    pub fn predict_leaving(
        &self,
        start: PState,
        horizon: f64,
        target: Option<usize>,
        step_angle: f64,
        skip: usize,
    ) -> Prediction {
        self.predict_core(start, horizon, &[], target, step_angle, Some(skip))
    }

    fn predict_core(
        &self,
        start: PState,
        horizon: f64,
        nodes: &[ManeuverNode],
        target: Option<usize>,
        step_angle: f64,
        skip: Option<usize>,
    ) -> Prediction {
        let f = self.pred_field(target, skip);
        let mut out = Prediction {
            nodes: vec![None; nodes.len()],
            ..Default::default()
        };
        let (mut pos, mut vel, mut t) = (start.pos, start.vel, start.t);
        let end_t = start.t + horizon.max(0.0);
        let mut bs = self.field_states(&f, t);
        let mut acc = Self::field_accel(&f, &bs, pos);
        let mut center = Self::field_center(&f, &bs, pos);
        let mut next = 0usize;
        // What the engines give: a burn at a date is flown for its whole duration (centred on the
        // date), not applied at once; the others are impulses at their apsis or node.
        let (thrust, m0, ve) = (self.main_thrust(), self.ship.mass(), self.nav_isp() * G0);
        let half_of = |dv: f64| -> f64 {
            if thrust > 0.0 {
                ve * m0 / thrust * (1.0 - (-0.5 * dv / ve).exp())
            } else {
                0.0
            }
        };
        // The earliest time the next relative node may fire (after the previous one).
        let mut earliest = t;
        let rel_to = |f: &Field, bs: &[(Vec3, Vec3)], body: usize, p: Vec3, v: Vec3| {
            Self::field_slot(f, body).map_or((p, v), |k| (p - bs[k].0, v - bs[k].1))
        };
        let (rp0, rv0) = rel_to(&f, &bs, center, pos, vel);
        let mut prev_vr = rp0.dot(rv0);
        let mut prev_y = pos.y;
        let tslot = target.and_then(|b| Self::field_slot(&f, b));
        let mut prev_td = tslot.map(|k| (pos - bs[k].0).length());
        let mut prev_td_rate = 0.0f64;
        let mut stride = 1usize;
        let mut counter = 0usize;
        out.points.push(PredPoint { t, pos, center });
        let mut steps = 0usize;
        while t < end_t - 1e-6 && steps < MAX_STEPS {
            steps += 1;
            let mut dt = Self::field_dt(&f, &bs, pos, vel, step_angle).min(end_t - t);
            if let Some(n) = nodes.get(next) {
                if let NodeWhen::At(tn) = n.when {
                    let trig = tn - half_of(n.remaining());
                    if trig <= t + 1e-9 {
                        // Already due: fire now.
                        dt = dt.min(1e-6);
                    } else if trig < t + dt {
                        dt = trig - t;
                    }
                }
            }
            // Kick-drift-kick leapfrog.
            let vh = vel + acc * (0.5 * dt);
            let np = pos + vh * dt;
            let nbs = self.field_states(&f, t + dt);
            let na = Self::field_accel(&f, &nbs, np);
            let nv = vh + na * (0.5 * dt);
            let (old_pos, old_t) = (pos, t);
            pos = np;
            vel = nv;
            t += dt;
            bs = nbs;
            acc = na;
            // The sphere that holds the ship.
            let c = Self::field_center(&f, &bs, pos);
            if c != center {
                if center != 0 {
                    out.events.push(PredEvent {
                        t,
                        kind: PredKind::SoiExit,
                        body: center,
                        center,
                        value: 0.0,
                        pos,
                    });
                }
                if c != 0 {
                    out.events.push(PredEvent {
                        t,
                        kind: PredKind::SoiEnter,
                        body: c,
                        center: c,
                        value: 0.0,
                        pos,
                    });
                }
                center = c;
                let (rp, rv) = rel_to(&f, &bs, center, pos, vel);
                prev_vr = rp.dot(rv);
            }
            // Impact on any body that pulls (the star too).
            for (k, &(i, gm, radius, _, pulls)) in f.tracked.iter().enumerate() {
                if pulls && gm > 1e6 && (pos - bs[k].0).length() < radius {
                    let speed = (vel - bs[k].1).length();
                    out.impact = Some((i, t, speed));
                    out.events.push(PredEvent {
                        t,
                        kind: PredKind::Impact,
                        body: i,
                        center: i,
                        value: speed,
                        pos,
                    });
                }
            }
            if out.impact.is_some() {
                break;
            }
            // On a circular orbit every point is an apsis: the node fires at once.
            if let Some(n) = nodes.get(next) {
                if matches!(n.when, NodeWhen::Periapsis | NodeWhen::Apoapsis) && t >= earliest {
                    let body = n.body.unwrap_or(center);
                    if let Some(k) = Self::field_slot(&f, body) {
                        let mu = f.tracked[k].1;
                        if mu > 0.0 {
                            let el = elements(pos - bs[k].0, vel - bs[k].1, mu);
                            if el.bound && el.e < 1.0e-3 {
                                let here = pos;
                                Self::apply_node(
                                    self,
                                    &f,
                                    &mut out,
                                    nodes,
                                    &mut next,
                                    &mut pos,
                                    &mut vel,
                                    t,
                                    here,
                                    center,
                                    &bs,
                                    &mut earliest,
                                );
                                acc = Self::field_accel(&f, &bs, pos);
                            }
                        }
                    }
                }
            }
            // Apsides of the orbit around the centre.
            let (rp, rv) = rel_to(&f, &bs, center, pos, vel);
            let vr = rp.dot(rv);
            let apsis = if prev_vr < 0.0 && vr >= 0.0 {
                Some(PredKind::Periapsis)
            } else if prev_vr > 0.0 && vr <= 0.0 {
                Some(PredKind::Apoapsis)
            } else {
                None
            };
            // The crossing time and the state at it, by linear interpolation inside the step.
            let frac_of = |a: f64, b: f64| {
                if (a - b).abs() < 1e-30 {
                    1.0
                } else {
                    (a / (a - b)).clamp(0.0, 1.0)
                }
            };
            if let Some(kind) = apsis {
                let fr = frac_of(prev_vr, vr);
                let tc = old_t + (t - old_t) * fr;
                let pc = old_pos + (pos - old_pos) * fr;
                if out
                    .events
                    .iter()
                    .filter(|e| matches!(e.kind, PredKind::Periapsis | PredKind::Apoapsis))
                    .count()
                    < 10
                {
                    out.events.push(PredEvent {
                        t: tc,
                        kind,
                        body: center,
                        center,
                        value: rp.length(),
                        pos: pc,
                    });
                }
                if let Some(n) = nodes.get(next) {
                    let want = matches!(
                        (n.when, kind),
                        (NodeWhen::Periapsis, PredKind::Periapsis)
                            | (NodeWhen::Apoapsis, PredKind::Apoapsis)
                    );
                    if want && tc >= earliest {
                        Self::apply_node(
                            self,
                            &f,
                            &mut out,
                            nodes,
                            &mut next,
                            &mut pos,
                            &mut vel,
                            tc,
                            pc,
                            center,
                            &bs,
                            &mut earliest,
                        );
                        acc = Self::field_accel(&f, &bs, pos);
                    }
                }
            }
            prev_vr = {
                let (rp2, rv2) = rel_to(&f, &bs, center, pos, vel);
                rp2.dot(rv2)
            };
            // Crossings of the ecliptic.
            if let Some(n) = nodes.get(next) {
                let up = prev_y < 0.0 && pos.y >= 0.0;
                let down = prev_y > 0.0 && pos.y <= 0.0;
                let want = matches!(
                    (n.when, up, down),
                    (NodeWhen::Ascending, true, _) | (NodeWhen::Descending, _, true)
                );
                if want && t >= earliest {
                    let fr = frac_of(prev_y, pos.y);
                    let tc = old_t + (t - old_t) * fr;
                    let pc = old_pos + (pos - old_pos) * fr;
                    Self::apply_node(
                        self,
                        &f,
                        &mut out,
                        nodes,
                        &mut next,
                        &mut pos,
                        &mut vel,
                        tc,
                        pc,
                        center,
                        &bs,
                        &mut earliest,
                    );
                    acc = Self::field_accel(&f, &bs, pos);
                }
            }
            prev_y = pos.y;
            // Absolute-time nodes.
            if let Some(n) = nodes.get(next) {
                if let NodeWhen::At(tn) = n.when {
                    if tn - half_of(n.remaining()) <= t + 1e-3 {
                        if thrust > 0.0 && n.remaining() > 1e-9 {
                            let mut st = (pos, vel, t);
                            self.finite_burn(
                                &f,
                                &mut out,
                                n,
                                next,
                                &mut st,
                                &mut bs,
                                &mut acc,
                                center,
                                tn,
                                (thrust, m0, ve),
                            );
                            (pos, vel, t) = st;
                            next += 1;
                            earliest = t + 1.0;
                            center = Self::field_center(&f, &bs, pos);
                            prev_vr = {
                                let (rp2, rv2) = rel_to(&f, &bs, center, pos, vel);
                                rp2.dot(rv2)
                            };
                        } else {
                            let here = pos;
                            Self::apply_node(
                                self,
                                &f,
                                &mut out,
                                nodes,
                                &mut next,
                                &mut pos,
                                &mut vel,
                                t,
                                here,
                                center,
                                &bs,
                                &mut earliest,
                            );
                            acc = Self::field_accel(&f, &bs, pos);
                        }
                    }
                }
            }
            // Closest approach to the target.
            if let (Some(k), Some(prev)) = (tslot, prev_td) {
                let d = (pos - bs[k].0).length();
                let rate = d - prev;
                if prev_td_rate < 0.0 && rate >= 0.0 {
                    let speed = (vel - bs[k].1).length();
                    let b = target.unwrap_or(0);
                    out.events.push(PredEvent {
                        t,
                        kind: PredKind::Closest,
                        body: b,
                        center: b,
                        value: prev,
                        pos,
                    });
                    if out.closest.is_none_or(|c| prev < c.1) {
                        out.closest = Some((t, prev, speed));
                    }
                }
                prev_td_rate = rate;
                prev_td = Some(d);
            }
            // Keep the path for drawing, thinning it as it grows.
            counter += 1;
            if counter >= stride {
                counter = 0;
                out.points.push(PredPoint { t, pos, center });
                if out.points.len() >= 2 * PRED_MAX_POINTS {
                    let mut k = 0;
                    out.points.retain(|_| {
                        k += 1;
                        k % 2 == 1
                    });
                    stride *= 2;
                }
            }
        }
        // A still-falling approach counts as a closest approach at the end.
        if let (Some(k), Some(prev)) = (tslot, prev_td) {
            if out.closest.is_none() {
                let speed = (vel - bs[k].1).length();
                out.closest = Some((t, prev, speed));
            }
        }
        out.points.push(PredPoint { t, pos, center });
        out.end = Some(PState { pos, vel, t });
        out.steps = steps;
        out
    }

    /// Fly the burn of node `k` from the state `st` (position, velocity, time) at its start:
    /// the engines push along the direction of the node frame at that moment (fixed during the
    /// burn, as the computer flies it) with the thrust over the mass that falls as propellant is
    /// spent, the planets pulling meanwhile. `tn` is the date of the node (the middle of the burn).
    #[allow(clippy::too_many_arguments)]
    fn finite_burn(
        &self,
        f: &Field,
        out: &mut Prediction,
        n: &ManeuverNode,
        k: usize,
        st: &mut (Vec3, Vec3, f64),
        bs: &mut Vec<(Vec3, Vec3)>,
        acc: &mut Vec3,
        center: usize,
        tn: f64,
        engine: (f64, f64, f64),
    ) {
        let (thrust, m0, ve) = engine;
        let (mut pos, mut vel, mut t) = *st;
        let body = n.body.unwrap_or(center);
        let (rp, rv) = match Self::field_slot(f, body) {
            Some(j) => (pos - bs[j].0, vel - bs[j].1),
            None => (pos, vel),
        };
        let dv = n.remaining();
        let dir = n.vector(rp, rv).normalized();
        out.nodes[k] = Some(Resolved {
            t: tn,
            ts: t,
            pos,
            vel,
            body,
        });
        out.events.push(PredEvent {
            t: tn,
            kind: PredKind::Node(k),
            body,
            center,
            value: dv,
            pos,
        });
        out.dv_total += dv;
        let dur = ve * m0 / thrust * (1.0 - (-dv / ve).exp());
        let mdot = thrust / ve;
        let sub = (dur / 80.0).clamp(0.1, 5.0);
        let nsub = (dur / sub).ceil().max(1.0) as usize;
        let h = dur / nsub as f64;
        let mut tau = 0.0;
        for _ in 0..nsub {
            let a_th = dir * (thrust / (m0 - mdot * tau).max(1.0));
            let vh = vel + (*acc + a_th) * (0.5 * h);
            let np = pos + vh * h;
            let nbs = self.field_states(f, t + h);
            let na = Self::field_accel(f, &nbs, np);
            let a_end = dir * (thrust / (m0 - mdot * (tau + h)).max(1.0));
            vel = vh + (na + a_end) * (0.5 * h);
            pos = np;
            t += h;
            tau += h;
            *bs = nbs;
            *acc = na;
        }
        *st = (pos, vel, t);
    }

    /// Fire node `next` at time `tc` from the position `pc` (velocity as integrated).
    #[allow(clippy::too_many_arguments)]
    fn apply_node(
        &self,
        f: &Field,
        out: &mut Prediction,
        nodes: &[ManeuverNode],
        next: &mut usize,
        pos: &mut Vec3,
        vel: &mut Vec3,
        tc: f64,
        pc: Vec3,
        center: usize,
        bs: &[(Vec3, Vec3)],
        earliest: &mut f64,
    ) {
        let n = nodes[*next];
        let body = n.body.unwrap_or(center);
        let (rp, rv) = match Self::field_slot(f, body) {
            Some(k) => (*pos - bs[k].0, *vel - bs[k].1),
            None => (*pos, *vel),
        };
        let mut v = n.vector(rp, rv);
        let mag = n.dv();
        if mag > 1e-12 {
            v = v * (n.remaining() / mag);
        }
        out.nodes[*next] = Some(Resolved {
            t: tc,
            ts: tc,
            pos: pc,
            vel: *vel,
            body,
        });
        out.events.push(PredEvent {
            t: tc,
            kind: PredKind::Node(*next),
            body,
            center,
            value: n.remaining(),
            pos: pc,
        });
        out.dv_total += n.remaining();
        *vel = *vel + v;
        *next += 1;
        *earliest = tc + 1.0;
    }

    /// The path of the ship from now on, with the planned burns.
    pub fn predict_ship(
        &self,
        horizon: f64,
        nodes: &[ManeuverNode],
        target: Option<usize>,
    ) -> Prediction {
        self.predict_ship_ex(horizon, nodes, target, STEP_ANGLE)
    }

    pub fn predict_ship_ex(
        &self,
        horizon: f64,
        nodes: &[ManeuverNode],
        target: Option<usize>,
        step_angle: f64,
    ) -> Prediction {
        self.predict_ex(
            PState {
                pos: self.ship.pos,
                vel: self.ship.vel,
                t: self.time,
            },
            horizon,
            nodes,
            target,
            step_angle,
        )
    }

    /// A horizon that covers one orbit of the ship around its reference body (a day of flight in
    /// the open): the default of the cockpit preview.
    pub fn default_horizon(&self) -> f64 {
        match self.orbit_readout() {
            Some(r) if r.elements.bound => r.elements.period.unwrap_or(86_400.0) * 1.05,
            Some(r) => {
                // Out of a well on an open path: until it is far away.
                let v = r.elements.speed.max(100.0);
                (self.influence_of(r.body) * 1.5 / v).clamp(600.0, 30.0 * 86_400.0)
            }
            None => 30.0 * 86_400.0,
        }
    }

    /// Mass of propellant a chain of burns costs a ship of wet mass `m0` (rocket equation at the
    /// ship's effective specific impulse, and the rules' fuel multiplier).
    pub fn fuel_for_dv(&self, dv: f64) -> f64 {
        let isp = self.nav_isp();
        crate::navigation::propellant_for(dv, isp, self.ship.mass())
    }

    /// Specific impulse (s) that the planner counts on: what the engines give, divided by the
    /// fuel multiplier of the rules (a costlier game burns more per second).
    pub fn nav_isp(&self) -> f64 {
        let isp = if self.effective.isp > 0.0 {
            self.effective.isp
        } else {
            self.ship.stats.isp
        };
        (isp / self.rules.fuel_mult().max(1e-6)).max(1.0)
    }

    /// Duration (s) of a burn of `dv` for the present ship (Tsiolkovsky and the real thrust), and
    /// the time to deliver the first half of it: the burn starts that long before its node.
    pub fn burn_time(&self, dv: f64) -> (f64, f64) {
        let f = self.main_thrust();
        if f <= 0.0 || dv <= 0.0 {
            return (0.0, 0.0);
        }
        let ve = self.nav_isp() * G0;
        let m0 = self.ship.mass();
        let t = |d: f64| ve * m0 / f * (1.0 - (-d / ve).exp());
        (t(dv), t(0.5 * dv))
    }

    /// Where the ship will be at `t` if nothing but body `c` pulls it (a Kepler arc): the start of
    /// the planner's predictions, which would waste thousands of steps coasting in a parking orbit.
    pub fn coast_state(&self, c: usize, t: f64) -> PState {
        let b = &self.bodies[c];
        let (bp, bv) = self.system.state(c, t);
        let (r, v) = if t > self.time && b.gm > 0.0 {
            kepler_propagate(
                self.ship.pos - b.pos,
                self.ship.vel - b.vel,
                b.gm,
                t - self.time,
            )
        } else {
            (self.ship.pos - b.pos, self.ship.vel - b.vel)
        };
        PState {
            pos: bp + r,
            vel: bv + v,
            t: t.max(self.time),
        }
    }

    /// Osculating elements of the ship around body `i`.
    pub fn elements_about(&self, i: usize) -> crate::navigation::Elements {
        let b = &self.bodies[i];
        elements(self.ship.pos - b.pos, self.ship.vel - b.vel, b.gm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    /// A circular orbit of the ship around planet 2 of seed 1 (a cold barren world).
    fn orbiting(alt_radii: f64) -> (Sim, usize) {
        let mut s = new_sim(
            1,
            Arc::new(
                GameData::load(
                    &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"),
                )
                .expect("data"),
            ),
        );
        let i = 2;
        let b = s.bodies[i].clone();
        let r = b.radius * alt_radii;
        s.ship.pos = b.pos + Vec3::new(r, 0.0, 0.0);
        s.ship.vel = b.vel + Vec3::new(0.0, 0.0, (b.gm / r).sqrt());
        s.update_ref_pub();
        s.ship.fuel = s.ship.stats.fuel_cap;
        (s, i)
    }

    #[test]
    fn the_kepler_arc_keeps_the_energy_and_matches_the_integration() {
        let (mut s, i) = orbiting(1.5);
        let (bpos, bvel, bgm) = (s.bodies[i].pos, s.bodies[i].vel, s.bodies[i].gm);
        let (r0, v0) = (s.ship.pos - bpos, s.ship.vel - bvel);
        let period = std::f64::consts::TAU * (r0.length().powi(3) / bgm).sqrt();
        // A quarter, a whole turn and a long wait: the radius and the speed do not change.
        for k in [0.25, 1.0, 7.3] {
            let (r, v) = kepler_propagate(r0, v0, bgm, k * period);
            assert!((r.length() - r0.length()).abs() / r0.length() < 1e-6, "{k}");
            assert!((v.length() - v0.length()).abs() / v0.length() < 1e-6, "{k}");
        }
        let (r, _) = kepler_propagate(r0, v0, bgm, period);
        assert!((r - r0).length() / r0.length() < 1e-6, "back at the start");
        // An elliptic and a hyperbolic arc agree with the integrated path (the planet alone is
        // not what pulls in the integration: the star's tide shows at 1e-3).
        let fast = v0 * 1.3;
        let (rk, _) = kepler_propagate(r0, fast, bgm, 0.2 * period);
        s.ship.vel = bvel + fast;
        let p = s.predict_ship(0.2 * period, &[], None);
        let end = p.end.expect("end");
        let rel = end.pos - s.system.state(i, end.t).0;
        assert!(
            (rel - rk).length() / rk.length() < 5e-3,
            "{:?} vs {:?}",
            rel,
            rk
        );
        let esc = v0 * 2.0;
        let (rh, vh) = kepler_propagate(r0, esc, bgm, 3.0 * period);
        let e0 = 0.5 * esc.dot(esc) - bgm / r0.length();
        let e1 = 0.5 * vh.dot(vh) - bgm / rh.length();
        assert!((e0 - e1).abs() / e0.abs() < 1e-6, "{e0} {e1}");
    }

    #[test]
    fn a_circular_orbit_predicts_apsides_at_its_radius_and_one_period() {
        let (s, i) = orbiting(1.5);
        let r = (s.ship.pos - s.bodies[i].pos).length();
        let period = std::f64::consts::TAU * (r.powi(3) / s.bodies[i].gm).sqrt();
        let p = s.predict_ship(period * 1.01, &[], None);
        let end = p.end.expect("end");
        assert!(((end.t - s.time) - period * 1.01).abs() < 1.0);
        // Back near the start after one period (leapfrog, small steps).
        let again = (end.pos - s.system.state(i, end.t).0).length();
        assert!((again - r).abs() / r < 1e-3, "{again} vs {r}");
        assert!(p.impact.is_none());
    }

    #[test]
    fn a_prograde_node_at_periapsis_raises_the_apoapsis_by_the_vis_viva_amount() {
        let (s, i) = orbiting(1.5);
        let b = &s.bodies[i];
        let r = (s.ship.pos - b.pos).length();
        let dv = 0.1 * (b.gm / r).sqrt();
        let node = ManeuverNode::new(NodeWhen::Periapsis, Some(i), dv, 0.0, 0.0);
        let period = std::f64::consts::TAU * (r.powi(3) / b.gm).sqrt();
        let p = s.predict_ship(period * 2.5, &[node], None);
        assert!(p.nodes[0].is_some(), "the node is resolved");
        let v1 = (b.gm / r).sqrt() + dv;
        let a = 1.0 / (2.0 / r - v1 * v1 / b.gm);
        let ra = 2.0 * a - r;
        let apo = p
            .events
            .iter()
            .find(|e| {
                e.kind == PredKind::Apoapsis && e.body == i && e.t > p.nodes[0].unwrap().t + 10.0
            })
            .expect("an apoapsis after the burn");
        assert!((apo.value - ra).abs() / ra < 0.01, "{} vs {ra}", apo.value);
    }

    #[test]
    fn a_path_into_the_ground_is_an_impact_and_the_prediction_stops() {
        let (mut s, i) = orbiting(1.5);
        let b = s.bodies[i].clone();
        // Almost at rest: it falls.
        s.ship.vel = b.vel + Vec3::new(0.0, 0.0, 50.0);
        let p = s.predict_ship(86_400.0, &[], None);
        let (hit, _, speed) = p.impact.expect("it falls on the world");
        assert_eq!(hit, i);
        assert!(speed > 100.0);
        assert!(p.end.expect("end").t < s.time + 86_400.0);
    }

    #[test]
    fn the_time_of_a_burn_follows_the_rocket_equation() {
        let (s, _) = orbiting(1.5);
        let (t, half) = s.burn_time(100.0);
        assert!(t > 0.0 && half > 0.0 && half < t);
        // Twice the delta-v takes more than twice... no: less than double the fuel-adjusted time;
        // at least it grows.
        assert!(s.burn_time(200.0).0 > t);
        // The time is the propellant over the mass flow.
        let ve = s.nav_isp() * G0;
        let used = s.fuel_for_dv(100.0);
        let mdot = s.main_thrust() / ve;
        assert!((used / mdot - t).abs() / t < 1e-6);
    }
}
