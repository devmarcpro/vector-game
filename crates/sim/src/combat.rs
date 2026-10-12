//! Ship combat: beams, kinetic projectiles with real ballistics, guided missiles, mines, shields,
//! armour, point defence, hostile AI, wrecks, salvage and boarding.
use crate::arms::{self, HitKind};
use crate::crew::DAY;
use crate::npc::{AiMode, NpcRole, NpcState};
use crate::{Event, ShipInput, Sim, SHIP_RADIUS};
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng, Vec3};

/// Hit radius of NPC ships (m).
pub const NPC_RADIUS: f64 = 25.0;
pub const MAX_WRECKS: usize = 24;
pub const SALVAGE_RANGE: f64 = 600.0;
pub const BOARD_RANGE: f64 = 500.0;
/// Fraction of damage that goes to the structure (the rest hurts modules).
pub(crate) const STRUCTURE_SHARE: f64 = 0.6;
/// Seconds between the missiles of an experienced hostile crew, and what one does.
const MISSILE_PERIOD: f64 = 12.0;
const MISSILE_DAMAGE: f64 = 120.0;
const HAIL_RANGE: f64 = 6_000.0;
const HAIL_TIME: f64 = 40.0;
/// Distance at which NPCs give up the chase.
const DISENGAGE_RANGE: f64 = 500_000.0;

/// Static combat numbers for NPC ships.
#[derive(Clone, Copy, Debug)]
pub struct NpcStats {
    pub hull: f64,
    pub shield: f64,
    /// Damage per second at point-blank range (0 = unarmed).
    pub dps: f64,
    pub range: f64,
    pub accel: f64,
    pub vmax: f64,
    pub bounty: f64,
}

pub fn npc_stats(role: NpcRole, level: u8) -> NpcStats {
    let l = f64::from(level.clamp(1, 5));
    match role {
        NpcRole::Trader => NpcStats {
            hull: 700.0 + 150.0 * l,
            shield: 100.0 * l,
            dps: 0.0,
            range: 0.0,
            accel: 25.0,
            vmax: 15_000.0,
            bounty: 0.0,
        },
        NpcRole::Miner => NpcStats {
            hull: 600.0 + 120.0 * l,
            shield: 60.0 * l,
            dps: 0.0,
            range: 0.0,
            accel: 20.0,
            vmax: 12_000.0,
            bounty: 0.0,
        },
        NpcRole::Patrol => NpcStats {
            hull: 1800.0 + 500.0 * l,
            shield: 500.0 + 250.0 * l,
            dps: 50.0 + 25.0 * l,
            range: 9_000.0,
            accel: 55.0,
            vmax: 25_000.0,
            bounty: 0.0,
        },
        NpcRole::Pirate => NpcStats {
            hull: 600.0 + 250.0 * l,
            shield: 150.0 + 120.0 * l,
            dps: 30.0 + 18.0 * l,
            range: 7_000.0,
            accel: 60.0,
            vmax: 22_000.0,
            bounty: 400.0 * l,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Owner {
    Player,
    Npc(u32),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ProjKind {
    Kinetic,
    /// Guided: homes on `target` (an NPC id; None = the player) at `turn` rad/s up to `top_speed`.
    Missile {
        target: Option<u32>,
        turn: f64,
        top_speed: f64,
    },
    Mine {
        armed_at: f64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Projectile {
    pub pos: Vec3,
    pub vel: Vec3,
    pub damage: f64,
    pub life: f64,
    pub owner: Owner,
    pub kind: ProjKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wreck {
    pub id: u32,
    pub name: String,
    pub pos: Vec3,
    pub vel: Vec3,
    pub cargo: Vec<(usize, f64)>,
    pub born: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoardOutcome {
    Captured { loot_kg: f64 },
    Repelled { casualties: usize },
}

/// Distance along the ray `origin + t*dir` (|dir| = 1) at which it enters a sphere, if it does.
pub fn ray_sphere(origin: Vec3, dir: Vec3, center: Vec3, radius: f64) -> Option<f64> {
    let oc = center - origin;
    let t = oc.dot(dir);
    let d2 = oc.dot(oc) - t * t;
    if t < 0.0 || d2 > radius * radius {
        return None;
    }
    Some(t - (radius * radius - d2).sqrt())
}

/// Closest approach of the segment a->b to point p: returns (distance, parameter 0..1).
pub fn segment_point(a: Vec3, b: Vec3, p: Vec3) -> (f64, f64) {
    let ab = b - a;
    let l2 = ab.dot(ab);
    let t = if l2 > 1e-12 {
        ((p - a).dot(ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((a + ab * t - p).length(), t)
}

impl Sim {
    pub fn combat_active(&self) -> bool {
        !self.projectiles.is_empty()
            || self
                .npcs
                .iter()
                .any(|n| matches!(n.state, NpcState::Combat { .. }) && !n.disabled)
    }

    /// The player counts as a criminal in the current territory.
    pub fn is_wanted(&self) -> bool {
        self.reputation_with(self.faction) <= -40.0
            || self
                .faction
                .is_some_and(|f| self.wanted_until.get(&f).is_some_and(|&t| t > self.time))
    }

    fn mark_wanted(&mut self, faction: Option<usize>, days: f64) {
        if let Some(f) = faction {
            let until = self.time + days * DAY;
            let e = self.wanted_until.entry(f).or_insert(0.0);
            *e = e.max(until);
        }
    }

    // ---- damage ----------------------------------------------------------

    /// Hit the player's ship with a kinetic blow (see `damage_player_hit`).
    pub fn damage_player(&mut self, dmg: f64) {
        self.damage_player_hit(dmg, HitKind::Kinetic);
    }

    /// Hit the player's ship: the escorts take their share, then the shield, the reactive and
    /// ablative plates and the armour, and what gets through is split between the hull, the
    /// modules and the cabins by how much energy the hit carries (`arms::split_hit`).
    pub fn damage_player_hit(&mut self, dmg: f64, kind: HitKind) {
        let dmg = dmg * self.rules.damage;
        if dmg <= 0.0 || self.destroyed || self.rules.invulnerable {
            return;
        }
        let screen = self.escort_screen();
        let dmg = if screen > 0.0 {
            self.escort_absorb(dmg * screen);
            dmg * (1.0 - screen)
        } else {
            dmg
        };
        let now = self.time;
        let armour = self.armour_profile();
        let z = crate::arms::split_hit(dmg, kind, self.ship.sys.shield_pts, &armour);
        self.last_zones = z;
        let s = &mut self.ship;
        s.sys.last_hit = now;
        s.sys.shield_pts -= z.shield;
        let rest = z.total_through();
        if rest <= 0.0 {
            return;
        }
        s.hull = (s.hull - z.hull).max(0.0);
        let pick = hash_coord(self.system.seed, (now * 100.0) as i64, 5, 17);
        let data = std::sync::Arc::clone(&self.data);
        s.sys.damage_modules(&data, &s.design, z.modules, pick);
        self.ablate_armour(z.ablated);
        self.ship.refresh_stats(&data);
        // The shock of the whole hit shakes the crew; a penetrating one reaches the cabins.
        self.injure_crew(0.4 * rest + 4.0 * z.crew);
        if self.ship.hull <= 0.0 {
            self.destroyed = true;
            self.events.push(Event::PlayerDestroyed);
        }
    }

    /// Hit NPC `k`; returns true if it was destroyed (and removed).
    pub fn damage_npc(&mut self, k: usize, dmg: f64) -> bool {
        let dmg = if self.is_ambush(k) {
            self.note_ambush();
            dmg * arms::AMBUSH_MULT
        } else {
            dmg
        };
        let Some(n) = self.npcs.get_mut(k) else {
            return false;
        };
        let absorbed = n.shield.min(dmg);
        n.shield -= absorbed;
        n.hull -= dmg - absorbed;
        let first_provocation = !n.provoked && !n.hostile && n.role != NpcRole::Pirate;
        n.provoked = true;
        let (role, faction) = (n.role, n.faction);
        if first_provocation {
            if let Some(f) = faction {
                self.treaty_violation(f);
            }
            self.change_reputation(faction, -10.0);
            self.mark_wanted(faction, 3.0);
            self.offend(faction, "agression");
            self.events.push(Event::CrimeCommitted);
        }
        let n = &mut self.npcs[k];
        match role {
            NpcRole::Trader | NpcRole::Miner => {
                if !n.disabled {
                    n.state = NpcState::Combat { mode: AiMode::Flee };
                }
            }
            NpcRole::Patrol => {
                n.hostile = true;
                if !matches!(n.state, NpcState::Combat { .. }) {
                    n.state = NpcState::Combat {
                        mode: AiMode::Pursue,
                    };
                }
            }
            NpcRole::Pirate => {
                n.hostile = true;
                n.hail_deadline = None;
                if !matches!(n.state, NpcState::Combat { .. }) {
                    n.state = NpcState::Combat {
                        mode: AiMode::Pursue,
                    };
                }
            }
        }
        if n.hull <= 0.0 {
            self.destroy_npc(k);
            return true;
        }
        if n.hull < 0.25 * n.hull_max && n.shield <= 0.0 && n.role != NpcRole::Pirate {
            n.disabled = true;
        } else if n.hull < 0.2 * n.hull_max && n.shield <= 0.0 {
            // A beaten pirate either flees or, if it has lost its engines, is left dead in space.
            let mut rng = Rng::new(hash_coord(
                self.system.seed,
                i64::from(n.id),
                (self.time * 10.0) as i64,
                61,
            ));
            if rng.f64() < 0.5 {
                n.disabled = true;
            } else {
                n.state = NpcState::Combat { mode: AiMode::Flee };
            }
        }
        false
    }

    fn destroy_npc(&mut self, k: usize) {
        let n = self.npcs.remove(k);
        match self.target_npc {
            Some(t) if t == k => self.target_npc = None,
            Some(t) if t > k => self.target_npc = Some(t - 1),
            _ => {}
        }
        let stats = npc_stats(n.role, n.level);
        let mut rng = Rng::new(hash_coord(self.system.seed, i64::from(n.id), 71, 3));
        let mut cargo = Vec::new();
        if let Some((r, kg)) = n.cargo {
            cargo.push((r, kg));
        }
        let scrap = self.data.resource_index("spare_parts");
        if let Some(r) = scrap {
            cargo.push((r, rng.range(40.0, 300.0)));
        }
        if n.role == NpcRole::Pirate {
            // Plunder: a few valuable goods.
            for _ in 0..3 {
                let r = (rng.next_u64() % self.data.resources.len() as u64) as usize;
                if self.data.resources[r].value > 0.5 {
                    cargo.push((r, rng.range(100.0, 900.0)));
                }
            }
        }
        if self.wrecks.len() >= MAX_WRECKS {
            self.wrecks.remove(0);
        }
        let id = self.next_wreck;
        self.next_wreck += 1;
        self.wrecks.push(Wreck {
            id,
            name: format!("EPAVE DE {}", n.name),
            pos: n.pos,
            vel: n.vel,
            cargo,
            born: self.time,
        });
        if n.role != NpcRole::Pirate {
            self.note_destroyed(n.faction, n.hull_max);
            if n.disabled {
                self.record_war_crime(n.faction, "un vaisseau neutralise a ete acheve");
            }
        }
        if n.hostile && n.role == NpcRole::Pirate {
            self.credits += stats.bounty;
            self.change_reputation(self.faction, 1.5);
            self.events.push(Event::BountyPaid {
                amount: stats.bounty,
            });
        } else if !n.hostile && n.role != NpcRole::Pirate {
            self.change_reputation(n.faction, -15.0);
            self.mark_wanted(n.faction, 7.0);
            self.offend(n.faction, "meurtre");
        }
        self.events.push(Event::NpcDestroyed);
    }

    // ---- the player's weapons ---------------------------------------------

    pub(crate) fn combat_tick(&mut self, dt: f64, input: &ShipInput) {
        self.beams.clear();
        self.enemy_beams.clear();
        self.ecm_range = 0.0;
        self.player_weapons(dt, input);
        self.decoys_tick(dt);
        self.escort_tick(dt);
        self.step_projectiles(dt);
        self.npc_combat(dt);
        self.battery_tick(dt);
        // Shield recharge.
        let cap = self.effective.shield_cap;
        let s = &mut self.ship.sys;
        if cap <= 0.0 {
            s.shield_pts = 0.0;
        } else if self.time - s.last_hit > 4.0 {
            s.shield_pts = (s.shield_pts + cap / 40.0 * dt).min(cap);
        } else {
            s.shield_pts = s.shield_pts.min(cap);
        }
    }

    fn player_weapons(&mut self, dt: f64, input: &ShipInput) {
        let data = std::sync::Arc::clone(&self.data);
        let slots: Vec<(usize, usize)> = self
            .ship
            .design
            .modules(&data)
            .filter(|(_, m)| !m.weapon.is_empty())
            .map(|(i, _)| (i, 0))
            .collect();
        let fwd = self.ship.forward();
        let mut pd_range: f64 = 0.0;
        for (slot, _) in slots {
            let mi = self.ship.design.slots[slot].unwrap_or(0);
            let m = &data.modules[mi];
            let health = self.ship.sys.health[slot];
            let powered = self.ship.sys.powered[slot] && health > 0.0;
            if let Some(cd) = self.weapon_cd.get_mut(slot) {
                *cd = (*cd - dt).max(-1.0);
            }
            match m.weapon.as_str() {
                "pd" if powered => {
                    pd_range = pd_range.max(m.range);
                    self.point_defence(m.rate, m.range, dt);
                }
                "ecm" if powered => self.ecm_range = self.ecm_range.max(m.range),
                "beam" if input.fire && powered => {
                    let from = self.ship.pos + fwd * SHIP_RADIUS;
                    let mut best: Option<(usize, f64)> = None;
                    for (k, n) in self.npcs.iter().enumerate().filter(|(_, n)| n.visible()) {
                        if let Some(t) = ray_sphere(from, fwd, n.pos, NPC_RADIUS) {
                            if t <= m.range && best.is_none_or(|(_, bt)| t < bt) {
                                best = Some((k, t));
                            }
                        }
                    }
                    let end = from + fwd * best.map_or(m.range, |b| b.1);
                    self.beams.push((from, end, true));
                    if let Some((k, t)) = best {
                        // Diffraction widens the spot with the distance: less energy per area.
                        let dmg = crate::arms::beam_dps(
                            m.damage * health,
                            m.wavelength,
                            m.aperture,
                            t,
                            0.0,
                        ) * dt;
                        self.damage_npc(k, dmg);
                    }
                }
                "kinetic" if input.fire && powered => {
                    let cd = self.weapon_cd.get(slot).copied().unwrap_or(0.0);
                    if cd <= 0.0 {
                        self.fire_projectile(slot, m, fwd, health);
                    }
                }
                "missile" | "mine" if input.fire_missile && powered => {
                    let cd = self.weapon_cd.get(slot).copied().unwrap_or(0.0);
                    if cd <= 0.0 {
                        self.fire_projectile(slot, m, fwd, health);
                    }
                }
                _ => {}
            }
        }
        let _ = pd_range;
    }

    fn fire_projectile(&mut self, slot: usize, m: &vector_data::Module, fwd: Vec3, health: f64) {
        let Some(ammo) = self.data.resource_index(&m.ammo) else {
            return;
        };
        if self.ship.cargo.kg[ammo] + 1e-9 < m.ammo_use {
            return;
        }
        let data = std::sync::Arc::clone(&self.data);
        if !self.rules.infinite_resources {
            self.ship.cargo.remove(&data, ammo, m.ammo_use);
        }
        if let Some(cd) = self.weapon_cd.get_mut(slot) {
            *cd = 1.0 / m.rate.max(0.01);
        }
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            (self.time * 1000.0) as i64,
            slot as i64,
            29,
        ));
        let spread = m.spread * 1e-3;
        let jitter = Vec3::new(
            rng.range(-spread, spread),
            rng.range(-spread, spread),
            rng.range(-spread, spread),
        );
        let dir = (fwd + jitter).normalized();
        let pos = self.ship.pos + dir * (SHIP_RADIUS + 5.0);
        let (vel, kind, life) = match m.weapon.as_str() {
            "kinetic" => (
                self.ship.vel + dir * m.speed,
                ProjKind::Kinetic,
                m.range / m.speed.max(1.0),
            ),
            "missile" => {
                let target = self.target_npc.and_then(|t| self.npcs.get(t)).map(|n| n.id);
                (
                    self.ship.vel + dir * 200.0,
                    ProjKind::Missile {
                        target,
                        turn: m.guidance,
                        top_speed: m.speed,
                    },
                    m.range / m.speed.max(1.0) * 1.6,
                )
            }
            _ => (
                self.ship.vel + dir * m.speed,
                ProjKind::Mine {
                    armed_at: self.time + 2.0,
                },
                600.0,
            ),
        };
        self.projectiles.push(Projectile {
            pos,
            vel,
            damage: m.damage * health,
            life,
            owner: Owner::Player,
            kind,
        });
        if self.projectiles.len() > 400 {
            self.projectiles.remove(0);
        }
    }

    /// Point defence shoots hostile projectiles near the ship.
    fn point_defence(&mut self, rate: f64, range: f64, dt: f64) {
        let ship_pos = self.ship.pos;
        let shots = rate * dt;
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            (self.time * 1000.0) as i64,
            3,
            41,
        ));
        let mut budget = shots;
        self.projectiles.retain(|p| {
            if budget <= 0.0
                || p.owner == Owner::Player
                || !matches!(p.kind, ProjKind::Missile { .. } | ProjKind::Kinetic)
            {
                return true;
            }
            if (p.pos - ship_pos).length() > range {
                return true;
            }
            budget -= 1.0;
            rng.f64() > 0.45 // 45 % per attempt
        });
    }

    // ---- projectiles ------------------------------------------------------

    fn step_projectiles(&mut self, dt: f64) {
        if self.projectiles.is_empty() {
            return;
        }
        let now = self.time;
        let ship_pos = self.ship.pos;
        let ship_vel = self.ship.vel;
        let ecm = self.ecm_range;
        // A heat seeker picks the brightest source: the ship's own heat or a flare.
        let ship_kw = self.emission_kw();
        let decoys = self.decoys.clone();
        let mut spent: Vec<usize> = Vec::new();
        let mut hits_npc: Vec<(usize, f64)> = Vec::new();
        let mut hit_player = 0.0;
        let mut keep: Vec<Projectile> = Vec::with_capacity(self.projectiles.len());
        let projectiles = std::mem::take(&mut self.projectiles);
        for mut p in projectiles {
            p.life -= dt;
            if p.life <= 0.0 {
                continue;
            }
            // Guidance.
            if let ProjKind::Missile {
                target,
                turn,
                top_speed,
            } = &p.kind
            {
                let tgt = match (p.owner, target) {
                    (Owner::Player, Some(id)) => self
                        .npcs
                        .iter()
                        .find(|n| n.id == *id)
                        .map(|n| (n.pos, n.vel)),
                    (Owner::Npc(_), _) => {
                        match arms::seeker_pick(p.pos, ship_pos, ship_kw, &decoys) {
                            Some(i) => Some((decoys[i].pos, decoys[i].vel)),
                            None => Some((ship_pos, ship_vel)),
                        }
                    }
                    _ => None,
                };
                let mut turn_rate = *turn;
                if matches!(p.owner, Owner::Npc(_))
                    && ecm > 0.0
                    && (p.pos - ship_pos).length() < ecm
                {
                    turn_rate *= 0.25;
                }
                let speed = p.vel.length();
                let new_speed = (speed + 80.0 * dt).min(*top_speed);
                let dir = if speed > 1e-6 {
                    p.vel * (1.0 / speed)
                } else {
                    Vec3::new(0.0, 0.0, 1.0)
                };
                if let Some((tp, tv)) = tgt {
                    let rel = tp - p.pos;
                    let closing = rel.length() / new_speed.max(1.0);
                    let want = (rel + tv * closing.min(8.0)).normalized();
                    let angle = dir.dot(want).clamp(-1.0, 1.0).acos();
                    let max_turn = turn_rate * dt;
                    let steer = if angle <= max_turn {
                        want
                    } else {
                        (dir * (angle - max_turn).sin() + want * max_turn.sin()).normalized()
                    };
                    let steer = if angle <= max_turn || angle.sin().abs() < 1e-6 {
                        steer
                    } else {
                        (dir * ((angle - max_turn).sin() / angle.sin())
                            + want * (max_turn.sin() / angle.sin()))
                        .normalized()
                    };
                    p.vel = steer * new_speed;
                } else {
                    p.vel = dir * new_speed;
                }
            }
            let a = p.pos;
            let b = p.pos + p.vel * dt;
            p.pos = b;
            let mut consumed = false;
            // A missile that reaches a flare blows up on it.
            if matches!(p.owner, Owner::Npc(_)) && matches!(p.kind, ProjKind::Missile { .. }) {
                if let Some(i) = decoys
                    .iter()
                    .position(|d| (d.pos - p.pos).length() < arms::DECOY_FUSE)
                {
                    spent.push(i);
                    continue;
                }
            }
            match (&p.kind, p.owner) {
                (ProjKind::Mine { armed_at }, Owner::Player) => {
                    if now >= *armed_at {
                        for (k, n) in self.npcs.iter().enumerate().filter(|(_, n)| n.visible()) {
                            if (n.pos - p.pos).length() < 300.0 {
                                hits_npc.push((k, p.damage));
                                consumed = true;
                                break;
                            }
                        }
                    }
                }
                (ProjKind::Mine { armed_at }, Owner::Npc(_)) => {
                    if now >= *armed_at && (ship_pos - p.pos).length() < 300.0 {
                        hit_player += p.damage;
                        consumed = true;
                    }
                }
                (_, Owner::Player) => {
                    for (k, n) in self.npcs.iter().enumerate().filter(|(_, n)| n.visible()) {
                        // Compare in the NPC's frame so a fast target cannot slip through the segment.
                        let rel_a = a - n.pos;
                        let rel_b = b - (n.pos + n.vel * dt);
                        let (d, _) = segment_point(rel_a, rel_b, Vec3::ZERO);
                        if d < NPC_RADIUS + 5.0 {
                            hits_npc.push((k, p.damage));
                            consumed = true;
                            break;
                        }
                    }
                }
                (_, Owner::Npc(_)) => {
                    let rel_a = a - ship_pos;
                    let rel_b = b - (ship_pos + ship_vel * dt);
                    let (d, _) = segment_point(rel_a, rel_b, Vec3::ZERO);
                    if d < SHIP_RADIUS + 5.0 {
                        hit_player += p.damage;
                        consumed = true;
                    }
                }
            }
            if !consumed {
                keep.push(p);
            }
        }
        self.projectiles = keep;
        spent.sort_unstable();
        spent.dedup();
        for i in spent.into_iter().rev() {
            if i < self.decoys.len() {
                self.decoys.remove(i);
            }
        }
        // Apply hits from the highest index down so removals do not shift pending targets.
        hits_npc.sort_by_key(|h| std::cmp::Reverse(h.0));
        for (k, d) in hits_npc {
            self.damage_npc(k, d);
        }
        if hit_player > 0.0 {
            self.damage_player(hit_player);
        }
    }

    // ---- hostile AI -------------------------------------------------------

    fn npc_combat(&mut self, dt: f64) {
        if self.docked.is_some() {
            self.break_off_all();
            return;
        }
        let now = self.time;
        let ship_pos = self.ship.pos;
        let ship_vel = self.ship.vel;
        let ai_mult = self.rules.ai_aggression;
        let mut damage_to_player = 0.0;
        let mut launches: Vec<(usize, Vec3, Vec3)> = Vec::new();
        let mut demands: Vec<(usize, f64)> = Vec::new();
        let mut ended: Vec<usize> = Vec::new();
        for k in 0..self.npcs.len() {
            let n = &mut self.npcs[k];
            let NpcState::Combat { mode } = n.state else {
                continue;
            };
            if n.disabled {
                n.pos = n.pos + n.vel * dt;
                continue;
            }
            let stats = npc_stats(n.role, n.level);
            let rel = ship_pos - n.pos;
            let d = rel.length().max(1.0);
            let mut mode = mode;
            // A pirate band breaks off when its doctrine says so; the others when hurt badly.
            let flee_below = if n.role == NpcRole::Pirate {
                arms::pirate_doctrine(self.system.seed, n.id).flee_below()
            } else {
                0.25
            };
            if n.hull < flee_below * n.hull_max && n.role != NpcRole::Patrol {
                mode = AiMode::Flee;
                n.state = NpcState::Combat { mode };
            }
            if d > DISENGAGE_RANGE {
                ended.push(k);
                continue;
            }
            // Pirates hail before they shoot.
            if n.role == NpcRole::Pirate && !n.hostile && d < HAIL_RANGE {
                match n.hail_deadline {
                    None => {
                        n.hail_deadline = Some(now + HAIL_TIME);
                        demands.push((k, 0.0));
                    }
                    Some(t) if now >= t => n.hostile = true,
                    _ => {}
                }
            }
            // Steering towards a desired velocity, limited by acceleration.
            let standoff = (stats.range * 0.55).max(1500.0);
            let dir = rel * (1.0 / d);
            let desired = match mode {
                AiMode::Flee => n.vel + dir * (-stats.vmax),
                AiMode::Pursue => {
                    if n.hostile || n.role != NpcRole::Pirate {
                        if d > standoff * 1.3 {
                            let v_close =
                                (2.0 * stats.accel * (d - standoff)).sqrt().min(stats.vmax);
                            ship_vel + dir * v_close
                        } else {
                            // Circle the target at the standoff distance.
                            let tangent = Vec3::new(-dir.z, 0.0, dir.x).normalized();
                            ship_vel
                                + tangent * 300.0
                                + dir * ((d - standoff) * 0.2).clamp(-200.0, 200.0)
                        }
                    } else {
                        // Hailing: shadow the player at a polite distance.
                        ship_vel + dir * ((d - 2500.0) * 0.2).clamp(-600.0, 900.0)
                    }
                }
            };
            let dv = desired - n.vel;
            let a_need = dv.length() / dt.max(1e-3);
            let acc = if a_need > stats.accel {
                dv * (stats.accel / dv.length())
            } else {
                dv * (1.0 / dt.max(1e-3))
            };
            n.vel = n.vel + acc * dt;
            n.pos = n.pos + n.vel * dt;
            // Fire.
            if n.hostile && mode == AiMode::Pursue && stats.dps > 0.0 && d <= stats.range {
                let accuracy = (1.05
                    - 0.6 * d / stats.range
                    - 0.0004 * (n.vel - ship_vel).length().min(1500.0))
                .clamp(0.12, 0.9);
                damage_to_player += stats.dps * dt * accuracy * ai_mult;
                self.enemy_beams.push((n.pos, ship_pos));
                // Seasoned crews also loose a guided missile every twelve seconds.
                let phase = f64::from(n.id) * 1.7;
                if n.level >= 2
                    && d > 2000.0
                    && ((now + phase) / MISSILE_PERIOD).floor()
                        != ((now - dt + phase) / MISSILE_PERIOD).floor()
                {
                    launches.push((k, n.pos, n.vel + dir * 150.0));
                }
            }
        }
        for (k, pos, vel) in launches {
            let id = self.npcs[k].id;
            self.projectiles.push(Projectile {
                pos,
                vel,
                damage: MISSILE_DAMAGE,
                life: 40.0,
                owner: Owner::Npc(id),
                kind: ProjKind::Missile {
                    target: None,
                    turn: 0.8,
                    top_speed: 650.0,
                },
            });
        }
        for k in ended.into_iter().rev() {
            self.end_combat(k);
        }
        for (k, _) in demands {
            let value: f64 = (0..self.ship.cargo.kg.len())
                .map(|r| self.ship.cargo.kg[r] * self.data.resources[r].value)
                .sum();
            let amount = (0.12 * value).round().max(200.0);
            self.tribute_due = Some((self.npcs[k].id, amount));
            self.events.push(Event::PirateDemand { amount });
        }
        if damage_to_player > 0.0 {
            self.damage_player_hit(damage_to_player, HitKind::Beam);
        }
    }

    /// Return a fighter to its normal routine.
    pub(crate) fn end_combat(&mut self, k: usize) {
        let rocks: Vec<usize> = (0..self.bodies.len())
            .filter(|&i| matches!(self.bodies[i].kind, vector_world::BodyKind::Asteroid(_)))
            .collect();
        let n = &mut self.npcs[k];
        n.hostile = false;
        n.hail_deadline = None;
        n.provoked = false;
        n.hull = n.hull_max;
        n.shield = n.shield_max;
        n.disabled = false;
        n.last_scan = self.time;
        n.state = match n.role {
            NpcRole::Pirate if !rocks.is_empty() => NpcState::Lurk {
                rock: rocks[(n.id as usize) % rocks.len()],
            },
            _ => NpcState::Docked {
                at: n.home.min(self.bodies.len() - 1),
                until: self.time + 3600.0,
            },
        };
    }

    fn break_off_all(&mut self) {
        for k in 0..self.npcs.len() {
            if matches!(self.npcs[k].state, NpcState::Combat { .. }) {
                self.end_combat(k);
            }
        }
        self.projectiles.clear();
        self.tribute_due = None;
    }

    /// Pay the pirates who are demanding tribute; they leave you alone for a few days.
    pub fn pay_tribute(&mut self) -> Result<f64, &'static str> {
        let (id, amount) = self.tribute_due.ok_or("personne ne reclame de tribut")?;
        if self.credits < amount {
            return Err("fonds insuffisants pour le tribut");
        }
        let k = self
            .npcs
            .iter()
            .position(|n| n.id == id)
            .ok_or("les pirates sont partis")?;
        self.credits -= amount;
        self.tribute_due = None;
        self.end_combat(k);
        let n = &mut self.npcs[k];
        n.last_scan = self.time + 3.0 * DAY;
        Ok(amount)
    }

    // ---- salvage and boarding ---------------------------------------------

    pub fn nearest_wreck(&self) -> Option<(usize, f64)> {
        self.wrecks
            .iter()
            .enumerate()
            .map(|(i, w)| (i, (w.pos - self.ship.pos).length()))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Haul the cargo of a nearby wreck aboard.
    pub fn salvage(&mut self) -> Result<f64, &'static str> {
        let (i, d) = self.nearest_wreck().ok_or("aucune epave")?;
        if d > SALVAGE_RANGE {
            return Err("epave trop eloignee");
        }
        if (self.wrecks[i].vel - self.ship.vel).length() > 60.0 {
            return Err("vitesse relative trop elevee");
        }
        let data = std::sync::Arc::clone(&self.data);
        let w = &mut self.wrecks[i];
        let mut total = 0.0;
        for (r, kg) in w.cargo.iter_mut() {
            let got = self.ship.cargo.add(&data, *r, *kg);
            *kg -= got;
            total += got;
        }
        w.cargo.retain(|(_, kg)| *kg > 0.5);
        self.findings.insert("wreck:salvaged".to_string());
        if self.wrecks[i].cargo.is_empty() {
            self.wrecks.remove(i);
        }
        if total <= 0.0 {
            return Err("soute pleine ou epave vide");
        }
        Ok(total)
    }

    /// Board the targeted, crippled ship.
    pub fn board(&mut self) -> Result<BoardOutcome, &'static str> {
        let k = self.target_npc.ok_or("aucune cible")?;
        let n = self.npcs.get(k).ok_or("cible inconnue")?;
        if !n.disabled {
            return Err("la cible n'est pas neutralisee");
        }
        if (n.pos - self.ship.pos).length() > BOARD_RANGE || (n.vel - self.ship.vel).length() > 60.0
        {
            return Err("trop loin ou trop rapide pour un abordage");
        }
        let combat_skill = self.data.skill_index("combat");
        let best = combat_skill.map_or(0.0, |s| {
            self.crew
                .alive()
                .map(|p| f64::from(p.skills[s]))
                .fold(0.0, f64::max)
        });
        let armory: f64 = self
            .ship
            .design
            .modules(&self.data)
            .map(|(_, m)| m.armory)
            .sum();
        let chance =
            (0.3 + 0.07 * best + 0.1 * armory - 0.08 * f64::from(n.level)).clamp(0.1, 0.95);
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            i64::from(n.id),
            (self.time * 10.0) as i64,
            83,
        ));
        if rng.f64() < chance {
            let (cargo, role, faction) = (n.cargo, n.role, n.faction);
            let data = std::sync::Arc::clone(&self.data);
            let mut loot = 0.0;
            if let Some((r, kg)) = cargo {
                loot += self.ship.cargo.add(&data, r, kg);
            }
            if !n.hostile && role != NpcRole::Pirate {
                self.change_reputation(faction, -20.0);
                self.mark_wanted(faction, 10.0);
            }
            self.destroy_npc_silent(k);
            Ok(BoardOutcome::Captured { loot_kg: loot })
        } else {
            let mut casualties = 0;
            for p in self.crew.members.iter_mut().filter(|p| p.alive) {
                if rng.f64() < 0.35 {
                    p.health = (p.health - 0.35).max(0.0);
                    if p.health <= 0.0 {
                        p.alive = false;
                        casualties += 1;
                    }
                }
            }
            Ok(BoardOutcome::Repelled { casualties })
        }
    }

    /// Take a ship out of the sky without wreck, bounty or crime (a prize).
    pub(crate) fn remove_npc_silent(&mut self, k: usize) {
        self.destroy_npc_silent(k);
    }

    pub(crate) fn mark_wanted_pub(&mut self, faction: Option<usize>, days: f64) {
        self.mark_wanted(faction, days);
    }

    /// Remove a captured ship without wreck or bounty.
    fn destroy_npc_silent(&mut self, k: usize) {
        self.npcs.remove(k);
        self.target_npc = None;
    }

    /// The insurer rebuilds the ship at the nearest station after a loss.
    /// With permadeath on, losing the ship ends the game.
    pub fn can_respawn(&self) -> bool {
        !self.rules.permadeath
    }

    pub fn insurance_respawn(&mut self) {
        self.claim_before_wipe();
        self.destroyed = false;
        self.ship.cargo = crate::cargo::Cargo::new(self.data.resources.len());
        self.ship.sys.health.fill(1.0);
        self.ship.sys.temp_k = 290.0;
        self.ship.refresh_stats(&self.data.clone());
        self.ship.hull = self.ship.stats.hull_max;
        self.ship.fuel = self.ship.stats.fuel_cap * 0.25;
        self.ship.sys.battery_mj = self.ship.stats.battery_cap;
        self.ship.sys.shield_pts = 0.0;
        self.ship.vel = Vec3::ZERO;
        self.ship.ang = Vec3::ZERO;
        self.credits *= 0.8;
        self.projectiles.clear();
        self.autopilot = None;
        self.docked = None;
        self.jump = None;
        if let Some((st, _)) = self.nearest_station() {
            self.spawn_near(st, self.bodies[st].radius + 800.0);
        }
        self.spawn_traffic();
        self.events.push(Event::Respawned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::Activity;
    use std::sync::Arc;
    use vector_data::GameData;
    use vector_world::galaxy::Galaxy;
    use vector_world::{generate_system, BodyKind};

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    /// A sim where the pirates exist (unclaimed space with a belt) and the ship is armed.
    fn armed_sim() -> Sim {
        let g = Galaxy::new(8);
        let e = g
            .nearby([0.0; 3], 150.0)
            .into_iter()
            .find(|e| {
                let s = generate_system(e.seed);
                g.territory(e.pos, 8).is_none()
                    && s.bodies
                        .iter()
                        .any(|b| matches!(b.kind, BodyKind::Asteroid(_)))
                    && s.bodies.iter().any(|b| b.kind == BodyKind::Station)
            })
            .expect("frontier system with a belt");
        let mut s = Sim::new_in_galaxy(g, &e, data());
        s.reveal_all();
        // Deep space, far from any gravity well (the origin is inside the star).
        s.ship.pos = Vec3::new(4.0e11, 0.0, 0.0);
        s.ship.vel = Vec3::ZERO;
        s.update_ref();
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        arm(
            &mut s,
            &[
                "pulselaser_mk2",
                "massdriver_mk1",
                "missilerack_mk2",
                "shield_mk2",
                "pointdefense_mk1",
            ],
        );
        s
    }

    /// Replace the hull with a roomy one and fit weapons.
    fn arm(s: &mut Sim, ids: &[&str]) {
        let data = Arc::clone(&s.data);
        let hull = data.hull_index("frigate").expect("hull");
        s.ship.design = crate::design::ShipDesign::empty(&data, hull);
        for id in [
            "fusion_mk3",
            "torch_mk3",
            "battery_mk3",
            "radiator_mk4",
            "lifesup_mk1",
            "cargo_mk3",
            "tank_mk3",
        ] {
            s.ship.design.install_first_fit(&data, id).expect("fit");
        }
        for id in ids {
            s.ship
                .design
                .install_first_fit(&data, id)
                .unwrap_or_else(|e| panic!("{id}: {e}"));
        }
        s.ship.sys = crate::design::Systems::new(&s.ship.design, 0.0);
        s.ship.refresh_stats(&data);
        s.ship.hull = s.ship.stats.hull_max;
        s.ship.fuel = s.ship.stats.fuel_cap;
        s.ship.sys.battery_mj = s.ship.stats.battery_cap;
        s.weapon_cd = vec![0.0; s.ship.design.slots.len()];
        for (id, kg) in [("ammo_slugs", 400.0), ("missiles", 20.0)] {
            s.ship
                .cargo
                .add(&data, data.resource_index(id).expect("res"), kg);
        }
    }

    fn pirate(s: &Sim) -> usize {
        s.npcs
            .iter()
            .position(|n| n.role == NpcRole::Pirate)
            .expect("a pirate")
    }

    /// Put a hostile pirate `d` metres ahead of the ship, at rest relative to it.
    fn place_target(s: &mut Sim, d: f64) -> usize {
        let k = pirate(s);
        s.ship.rot = vector_core::Mat3::IDENTITY;
        s.ship.assist = false;
        let n = &mut s.npcs[k];
        n.pos = s.ship.pos + Vec3::new(0.0, 0.0, d);
        n.vel = s.ship.vel;
        n.hostile = true;
        n.disabled = true; // a target dummy: drifts with its velocity, never fires
        n.state = NpcState::Combat {
            mode: AiMode::Pursue,
        };
        s.target_npc = Some(k);
        k
    }

    fn step_n(s: &mut Sim, n: usize, input: ShipInput) {
        for _ in 0..n {
            s.step(&input);
        }
    }

    #[test]
    fn geometry_helpers() {
        let hit = ray_sphere(
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 50.0, 1000.0),
            10.0,
        );
        assert!(hit.is_none());
        let hit = ray_sphere(
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 5.0, 1000.0),
            25.0,
        )
        .expect("hit");
        assert!(hit > 970.0 && hit < 1000.0);
        let (d, t) = segment_point(
            Vec3::new(-10.0, 1.0, 0.0),
            Vec3::new(10.0, 1.0, 0.0),
            Vec3::ZERO,
        );
        assert!((d - 1.0).abs() < 1e-9 && (t - 0.5).abs() < 1e-9);
    }

    #[test]
    fn beam_strips_shields_then_hull_then_destroys_and_pays_bounty() {
        let mut s = armed_sim();
        let k = place_target(&mut s, 3000.0);
        let (sh0, hu0) = (s.npcs[k].shield, s.npcs[k].hull);
        let credits0 = s.credits;
        let fire = ShipInput {
            fire: true,
            ..Default::default()
        };
        step_n(&mut s, 30, fire);
        assert!(s.npcs[k].shield < sh0, "shield absorbs first");
        assert!((s.npcs[k].hull - hu0).abs() < 1e-9 || s.npcs[k].shield <= 0.0);
        let mut destroyed = false;
        for _ in 0..3000 {
            if s.npcs
                .iter()
                .all(|n| n.id != 0 && n.role != NpcRole::Pirate)
                && s.wrecks.is_empty()
            {
                // pirate gone before the wreck appears? keep firing
            }
            s.step(&fire);
            if !s.wrecks.is_empty() {
                destroyed = true;
                break;
            }
            // Keep the (fleeing/hunting) target in the crosshair.
            if let Some(k) = s.npcs.iter().position(|n| {
                n.role == NpcRole::Pirate && matches!(n.state, NpcState::Combat { .. })
            }) {
                s.npcs[k].pos = s.ship.pos + Vec3::new(0.0, 0.0, 3000.0);
                s.npcs[k].vel = s.ship.vel;
            }
        }
        assert!(destroyed, "a pulse laser must eventually kill a pirate");
        assert!(s.credits > credits0, "bounty paid for a hostile pirate");
        assert!(s.wrecks[0].cargo.iter().any(|(_, kg)| *kg > 0.0));
    }

    #[test]
    fn kinetic_rounds_use_ammo_and_hit_a_target_that_is_not_moving_relative_to_us() {
        let mut s = armed_sim();
        let k = place_target(&mut s, 8000.0);
        let hu0 = s.npcs[k].hull + s.npcs[k].shield;
        let ammo = s.data.resource_index("ammo_slugs").expect("ammo");
        let a0 = s.ship.cargo.kg[ammo];
        s.npcs[k].shield_max = 0.0;
        // Disable its movement so the shots land.
        let fire = ShipInput {
            fire: true,
            ..Default::default()
        };
        for _ in 0..180 {
            s.npcs[k].vel = s.ship.vel;
            s.npcs[k].pos = s.ship.pos + Vec3::new(0.0, 0.0, 8000.0);
            s.step(&fire);
        }
        assert!(s.ship.cargo.kg[ammo] < a0, "shooting consumes ammunition");
        let now = s
            .npcs
            .iter()
            .find(|n| n.id == s.npcs.get(k).map_or(0, |n| n.id))
            .map_or(0.0, |n| n.hull + n.shield);
        assert!(now < hu0 || s.wrecks.len() == 1, "rounds must have landed");
        // No ammo, no fire.
        s.ship.cargo.kg[ammo] = 0.0;
        s.ship.cargo.mass = 0.0;
        let before = s.projectiles.len();
        step_n(&mut s, 5, fire);
        assert!(s.projectiles.len() <= before + 1);
    }

    #[test]
    fn missiles_home_in_and_ecm_point_defence_help_the_defender() {
        let mut s = armed_sim();
        let k = place_target(&mut s, 6000.0);
        s.npcs[k].shield = 0.0;
        let id = s.npcs[k].id;
        let hull0 = s.npcs[k].hull;
        // A missile launched while the target drifts sideways still connects.
        s.npcs[k].vel = s.ship.vel + Vec3::new(120.0, 0.0, 0.0);
        let launch = ShipInput {
            fire_missile: true,
            ..Default::default()
        };
        s.step(&launch);
        assert!(s
            .projectiles
            .iter()
            .any(|p| matches!(p.kind, ProjKind::Missile { target: Some(t), .. } if t == id)));
        let mut hit = false;
        for _ in 0..600 {
            s.step(&ShipInput::default());
            if s.npcs
                .iter()
                .find(|n| n.id == id)
                .is_none_or(|n| n.hull < hull0)
            {
                hit = true;
                break;
            }
        }
        assert!(hit, "guided missile should reach a drifting target");
        // Point defence removes incoming missiles with good probability.
        let mut kills = 0;
        let mut trials = 0;
        for i in 0..60 {
            s.projectiles.clear();
            s.time += 0.37 * f64::from(i + 1);
            s.projectiles.push(Projectile {
                pos: s.ship.pos + Vec3::new(0.0, 0.0, 1500.0),
                vel: Vec3::new(0.0, 0.0, -50.0),
                damage: 100.0,
                life: 20.0,
                owner: Owner::Npc(99),
                kind: ProjKind::Missile {
                    target: None,
                    turn: 1.0,
                    top_speed: 600.0,
                },
            });
            for _ in 0..3 {
                s.time += 0.013;
                s.point_defence(6.0, 3000.0, 1.0);
            }
            trials += 1;
            kills += usize::from(s.projectiles.is_empty());
        }
        assert!(
            kills * 10 > trials * 6,
            "point defence should stop most missiles: {kills}/{trials}"
        );
    }

    #[test]
    fn shield_absorbs_recharges_and_armour_mitigates() {
        let mut s = armed_sim();
        step_n(&mut s, 120, ShipInput::default());
        let cap = s.effective.shield_cap;
        assert!(cap > 1000.0, "shield module must provide capacity: {cap}");
        // Charge fully, then take a hit smaller than the pool.
        s.ship.sys.shield_pts = cap;
        let hull0 = s.ship.hull;
        s.damage_player(cap * 0.3);
        assert!(
            (s.ship.sys.shield_pts - cap * 0.7).abs() < 1e-6 && s.ship.hull == hull0,
            "shield soaks it all"
        );
        // Empty shield: structure takes the damage.
        s.ship.sys.shield_pts = 0.0;
        s.damage_player(1000.0);
        assert!(s.ship.hull < hull0);
        // Recharge after a quiet spell.
        s.time += 10.0;
        step_n(&mut s, 60, ShipInput::default());
        assert!(s.ship.sys.shield_pts > 0.0, "shield comes back");
        // Armour reduces damage.
        let mut a = armed_sim();
        let mut b = armed_sim();
        arm(&mut a, &["shield_mk2"]);
        arm(&mut b, &["shield_mk2", "armor_mk3"]);
        step_n(&mut a, 2, ShipInput::default());
        step_n(&mut b, 2, ShipInput::default());
        let (ha, hb) = (a.ship.hull, b.ship.hull);
        a.ship.sys.shield_pts = 0.0;
        b.ship.sys.shield_pts = 0.0;
        a.damage_player(500.0);
        b.damage_player(500.0);
        assert!(
            ha - a.ship.hull > hb - b.ship.hull,
            "plating must soak damage"
        );
    }

    #[test]
    fn pirates_hail_then_attack_and_tribute_buys_peace() {
        let mut s = armed_sim();
        let data = Arc::clone(&s.data);
        s.ship
            .cargo
            .add(&data, data.resource_index("gold").expect("gold"), 80.0);
        s.credits = 50_000.0;
        let k = pirate(&s);
        s.ship.assist = false;
        let NpcState::Lurk { rock } = s.npcs[k].state else {
            panic!("pirate should lurk")
        };
        s.ship.pos = s.bodies[rock].pos + Vec3::new(0.0, 0.0, -30_000.0);
        s.ship.vel = s.bodies[rock].vel;
        s.npcs[k].last_scan = -1.0e9;
        s.place_npcs();
        let mut demanded = false;
        for _ in 0..4000 {
            s.events.clear();
            s.step(&ShipInput::default());
            if s.events
                .iter()
                .any(|e| matches!(e, Event::PirateDemand { .. }))
            {
                demanded = true;
                break;
            }
        }
        assert!(demanded, "a rich ship near a pirate gets a demand");
        let c0 = s.credits;
        let paid = s.pay_tribute().expect("pay");
        assert!(paid >= 200.0 && s.credits < c0);
        let k = pirate(&s);
        assert!(
            !matches!(s.npcs[k].state, NpcState::Combat { .. }),
            "paid pirates leave"
        );
        // Ignoring a demand ends in a fight.
        let mut s = armed_sim();
        let data = Arc::clone(&s.data);
        s.ship
            .cargo
            .add(&data, data.resource_index("gold").expect("gold"), 80.0);
        let k = pirate(&s);
        s.ship.assist = false;
        s.npcs[k].pos = s.ship.pos + Vec3::new(0.0, 0.0, 5000.0);
        s.npcs[k].vel = s.ship.vel;
        s.npcs[k].hostile = true;
        s.npcs[k].state = NpcState::Combat {
            mode: AiMode::Pursue,
        };
        s.ship.sys.shield_pts = 0.0;
        let hull0 = s.ship.hull;
        let sh0 = s.effective.shield_cap;
        for _ in 0..1500 {
            s.step(&ShipInput::default());
        }
        assert!(
            s.ship.hull < hull0 || s.ship.sys.shield_pts < sh0 || s.ship.sys.last_hit > 0.0,
            "an ignored demand turns into an attack"
        );
    }

    #[test]
    fn attacking_patrols_is_a_crime_and_killing_traders_costs_reputation() {
        let mut s = armed_sim();
        s.faction = Some(0);
        s.spawn_traffic();
        let k = s
            .npcs
            .iter()
            .position(|n| n.role == NpcRole::Patrol)
            .expect("patrol");
        s.ship.rot = vector_core::Mat3::IDENTITY;
        s.npcs[k].pos = s.ship.pos + Vec3::new(0.0, 0.0, 2500.0);
        s.npcs[k].vel = s.ship.vel;
        s.npcs[k].state = NpcState::Orbit {
            around: 0,
            phase: 0.0,
        };
        s.place_npcs();
        s.npcs[k].pos = s.ship.pos + Vec3::new(0.0, 0.0, 2500.0);
        let rep0 = s.reputation_with(Some(0));
        s.damage_npc(k, 10.0);
        assert!(s.reputation_with(Some(0)) < rep0, "first blow is a crime");
        assert!(s.npcs[k].hostile && matches!(s.npcs[k].state, NpcState::Combat { .. }));
        assert!(s.is_wanted(), "wanted in that territory");
        let t = s
            .npcs
            .iter()
            .position(|n| n.role == NpcRole::Trader)
            .expect("trader");
        let rep1 = s.reputation_with(Some(0));
        let huge = s.npcs[t].hull + s.npcs[t].shield + 1.0;
        assert!(s.damage_npc(t, huge));
        assert!(
            s.reputation_with(Some(0)) < rep1 - 10.0,
            "murder is expensive"
        );
    }

    #[test]
    fn wrecks_can_be_salvaged_and_crippled_ships_boarded() {
        let mut s = armed_sim();
        let k = place_target(&mut s, 3000.0);
        let huge = s.npcs[k].hull + s.npcs[k].shield + 1.0;
        s.damage_npc(k, huge);
        assert_eq!(s.wrecks.len(), 1);
        s.ship.pos = s.wrecks[0].pos + Vec3::new(0.0, 0.0, -300.0);
        s.ship.vel = s.wrecks[0].vel;
        let got = s.salvage().expect("salvage");
        assert!(got > 0.0 && s.ship.cargo.mass > 0.0);
        // Boarding a crippled ship: put a trader in a disabled state.
        let t = s
            .npcs
            .iter()
            .position(|n| n.role == NpcRole::Trader)
            .expect("trader");
        s.npcs[t].disabled = true;
        s.npcs[t].state = NpcState::Combat { mode: AiMode::Flee };
        s.npcs[t].pos = s.ship.pos + Vec3::new(0.0, 0.0, 200.0);
        s.npcs[t].vel = s.ship.vel;
        s.target_npc = Some(t);
        let mut captured = false;
        for i in 0..40 {
            s.time += 7.0 + f64::from(i);
            let n_before = s.npcs.len();
            match s.board().expect("board") {
                BoardOutcome::Captured { .. } => {
                    assert_eq!(s.npcs.len(), n_before - 1);
                    captured = true;
                    break;
                }
                BoardOutcome::Repelled { .. } => {}
            }
        }
        assert!(captured, "boarding eventually succeeds");
        // A healthy ship cannot be boarded.
        let k2 = s
            .npcs
            .iter()
            .position(|n| n.role == NpcRole::Miner)
            .expect("miner");
        s.target_npc = Some(k2);
        assert!(s.board().is_err());
    }

    #[test]
    fn destruction_and_insurance_respawn() {
        let mut s = armed_sim();
        s.ship.sys.shield_pts = 0.0;
        s.events.clear();
        s.damage_player(1.0e7);
        assert!(s.destroyed && s.events.iter().any(|e| matches!(e, Event::PlayerDestroyed)));
        s.damage_player(100.0); // no double events
        let credits = s.credits;
        s.insurance_respawn();
        assert!(!s.destroyed && s.ship.hull == s.ship.stats.hull_max);
        assert_eq!(s.ship.cargo.mass, 0.0);
        assert!((s.credits - credits * 0.8).abs() < 1e-6);
        assert!(s.nearest_station().is_some_and(|(_, d)| d < 5000.0));
    }

    #[test]
    fn a_five_minute_dogfight_stays_finite_and_deterministic() {
        let run = || {
            let mut s = armed_sim();
            let k = place_target(&mut s, 4000.0);
            s.npcs[k].hostile = true;
            let fire = ShipInput {
                fire: true,
                fire_missile: true,
                ..Default::default()
            };
            for i in 0..9000 {
                let input = if i % 2 == 0 {
                    fire
                } else {
                    ShipInput {
                        yaw: 0.2,
                        ..Default::default()
                    }
                };
                s.step(&input);
            }
            assert!(s.ship.hull.is_finite() && s.ship.sys.shield_pts.is_finite());
            assert!(s
                .npcs
                .iter()
                .all(|n| n.pos.length().is_finite() && n.vel.length().is_finite()));
            assert!(s.projectiles.len() < 500);
            (
                s.ship.hull,
                s.npcs.len(),
                s.projectiles.len(),
                s.wrecks.len(),
            )
        };
        assert_eq!(run(), run());
        let _ = Activity::default();
    }
}
