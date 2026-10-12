//! Cryogenisation: the crew (and colonists) put to sleep in caissons. One representation: the
//! state of a sleeper is the `Body` of `body.rs` (`cryo`, core temperature, ice, cryoprotectant,
//! the date it began); the former cryostasis of a wounded person in the infirmary is the same
//! thing in a worse caisson. Pods are ship modules (`data/modules.ron`, category "cryo"): their
//! capacity, reliability and service temperature come from the powered, healthy ones.
//!
//! * A sleeper eats, drinks, breathes and tires no more; ages at `body::bio_rate` of its core
//!   temperature; its wounds and diseases stand still; its pay falls to a retainer; but the
//!   radiation keeps adding up and nothing repairs it.
//! * Going under: the core cools by a lag; crossing -40..0 C grows ice unless the blood carries
//!   cryoprotectant (a resource, 1.5 kg a person). A caisson warmer than -110 C lets ice grow back.
//! * Power or a caisson lost: those over the capacity warm up, uncontrolled, and die of the thaw.
//! * Waking is a process of hours (`wake_hours`), then a roll whose odds rise with the years asleep,
//!   the ice and the age, and fall with the reliability of the caisson and the care. The waker
//!   forgets (skills), suffers (cryo sickness), finds the world changed (news since) and
//!   loved ones older.
//! * The player (the ship's AI) manages it: put to sleep, wake, scheduled wakes (a date, the
//!   arrival, a critical alert), an automatic plan (all asleep but N) and watch rotation.
use crate::bourse::DAY;
use crate::crew::Person;
use crate::{Event, Sim};
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng};
use vector_data::GameData;
use vector_world::galaxy::StarRef;

/// Cryoprotectant (kg) a sleeper needs for a clean vitrification, and per colonist.
pub const CPA_KG: f64 = 1.5;
pub const COLONIST_CPA_KG: f64 = 1.0;
/// Hours of the controlled rewarming (good care), and of an urgent one.
pub const WAKE_HOURS: f32 = 24.0;
pub const WAKE_HOURS_URGENT: f32 = 4.0;
/// Share of the salary kept paid to someone asleep (the retainer).
pub const RETAINER: f64 = 0.1;
/// What a powered infirmary (or the hospital of a station) offers when there is no caisson.
pub const INFIRMARY_TEMP: f32 = -60.0;
pub const INFIRMARY_REL: f64 = 0.8;
/// Room temperature a thawing sleeper drifts to, C.
pub const ROOM_C: f32 = 21.0;
/// Chance per day, at reliability 0, that a held sleeper's caisson fails.
pub const FAIL_PER_DAY: f64 = 0.006;
/// Spare parts (kg) per sleeper and day when nothing says otherwise (an infirmary).
pub const UPKEEP_DEFAULT: f64 = 0.002;
/// Someone with less loyalty than this, healthy, is put to sleep against their will (a crime):
/// morale lost by the victim and by the others, loyalty lost by everybody.
pub const UNWILLING_LOYALTY: f32 = 0.35;
pub const FORCED_MORALE_VICTIM: f32 = 0.3;
pub const FORCED_MORALE_OTHERS: f32 = 0.1;
pub const FORCED_LOYALTY: f32 = 0.1;
/// Added risk of a failed waking per Sv of dose taken asleep; the dose that kills in the cold.
pub const DOSE_RISK: f64 = 0.03;
pub const LETHAL_DOSE_SV: f32 = 8.0;
/// Days between checks of the alerts for the scheduled "critical alert" wake.
pub const ALERT_CHECK: f64 = 30.0;

/// Where a scheduled wake concerns.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Who {
    All,
    One(u32),
    /// The best watcher among the sleepers (pilot, medic, engineer first).
    Best,
}

/// When a scheduled wake fires.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum When {
    At(f64),
    /// The ship reaches a new star or docks.
    Arrival,
    /// Any critical alert (the wake is urgent).
    Alert,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WakeOrder {
    pub who: Who,
    pub when: When,
}

/// What the player has arranged for the sleepers (saved with the crew).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CryoPlan {
    pub orders: Vec<WakeOrder>,
    /// Colonists asleep in bulk (hold kg of "colonists" turned into sleepers): how many, since
    /// when, the ice they grew and the cryoprotectant they got.
    pub colonists: u32,
    pub colonists_since: f64,
    pub colonists_ice: f32,
    pub colonists_cpa: f32,
    /// Days between two rotations of the watch (0 = none), and when the last one was.
    pub rotation_days: f32,
    pub last_rotation: f64,
    /// People to put to sleep when a place frees (the other half of a rotation).
    pub pending_sleep: Vec<u32>,
    pub last_here: Option<StarRef>,
    pub was_docked: bool,
    pub last_check: f64,
    pub last_tick: f64,
    /// The number kept awake by the last automatic plan.
    pub keep_awake: u8,
}

/// What the ship offers the sleepers this step (set by the sim, never saved).
#[derive(Clone, Copy, Debug, Default)]
pub struct CryoCtx {
    /// Places of the powered pods, their mean reliability, temperature (C), upkeep (kg/day).
    pub cap: usize,
    pub rel: f64,
    pub temp: f64,
    pub upkeep: f64,
    /// Docked at a station: its hospital freezes anyone.
    pub hospital: bool,
}

impl CryoCtx {
    /// Places that hold a sleeper: the pods, one improvised place in a powered infirmary, all of
    /// them at a hospital.
    pub fn slots(&self, medbay: f64) -> usize {
        if self.hospital {
            usize::MAX
        } else {
            self.cap + usize::from(medbay > 0.0)
        }
    }
    /// The temperature (C) the next sleeper will be held at, given how many already sleep in pods.
    pub fn set_temp(&self, in_pods: usize) -> f32 {
        if in_pods < self.cap {
            self.temp as f32
        } else {
            INFIRMARY_TEMP
        }
    }
    pub fn reliability(&self) -> f64 {
        if self.cap > 0 {
            self.rel
        } else {
            INFIRMARY_REL
        }
    }
}

/// Someone who just came out (read by the sim to tell what changed in the world).
#[derive(Clone, Debug)]
pub struct Woken {
    pub id: u32,
    pub name: String,
    pub since: f64,
    pub years: f64,
}

/// Hours of a controlled rewarming: care shortens it, haste (an alert) cuts it hard.
pub fn wake_hours(medic: f64, medbay: f64, urgent: bool) -> f32 {
    if urgent {
        return WAKE_HOURS_URGENT;
    }
    let care = (0.04 * medic + 0.2 * medbay.min(1.0)).min(0.6);
    (WAKE_HOURS * (1.0 - care) as f32).max(8.0)
}

/// Odds (0..0.97) that waking someone fails: the caisson (reliability), the years asleep, the
/// ice, the age, the haste; the care of a medic and an infirmary lowers them.
pub fn wake_death_chance(
    years: f64,
    rel: f64,
    ice: f32,
    age: f32,
    medic: f64,
    medbay: f64,
    urgent: bool,
) -> f64 {
    let base = 0.01 + (1.0 - rel.clamp(0.0, 1.0)) * 0.8;
    let duration = 0.04 * (1.0 + years.max(0.0)).ln();
    let ice = 0.6 * f64::from(ice.max(0.0));
    let old = 0.003 * f64::from((age - 50.0).max(0.0));
    let care = (1.0 - (0.04 * medic + 0.15 * medbay.min(1.0))).max(0.35);
    let haste = if urgent { 1.5 } else { 1.0 };
    ((base + duration + ice + old) * care * haste).clamp(0.0, 0.97)
}

/// The odds that waking `p` fails: `wake_death_chance` plus the radiation they took asleep
/// (the dose does not mend in the cold: 3 % per Sv).
pub fn wake_risk(p: &Person, years: f64, rel: f64, medic: f64, medbay: f64, urgent: bool) -> f64 {
    (wake_death_chance(years, rel, p.body.cryo_ice, p.age, medic, medbay, urgent)
        + DOSE_RISK * f64::from(p.body.acute_dose))
    .min(0.97)
}

/// Share (0..0.5) of every skill forgotten after `years` asleep with `ice` damage.
pub fn memory_loss(years: f64, ice: f32) -> f32 {
    (0.05 * (1.0 + 2.0 * years.max(0.0)).ln() as f32 + 0.3 * ice.max(0.0)).clamp(0.0, 0.5)
}

/// Chance per day that a held sleeper's caisson fails; four times worse without spare parts and
/// worse again once ice has damaged the body.
pub fn failure_per_day(rel: f64, maintained: bool, ice: f32) -> f64 {
    FAIL_PER_DAY
        * (1.0 - rel.clamp(0.0, 1.0))
        * if maintained { 1.0 } else { 4.0 }
        * (1.0 + f64::from(ice.max(0.0)))
}

/// How well a person serves as a watcher: the pilot, the medic and the engineer first, then the
/// best skill.
pub fn watch_score(data: &GameData, p: &Person) -> f64 {
    let role = match data.roles[p.role].id.as_str() {
        "pilot" => 30.0,
        "medic" => 25.0,
        "engineer" => 20.0,
        _ => 0.0,
    };
    role + f64::from(p.best_skill().1) + f64::from(p.health)
}

/// A person's years asleep at `now`.
pub fn years_asleep(p: &Person, now: f64) -> f64 {
    ((now - p.body.cryo_since).max(0.0)) / (365.25 * DAY)
}

fn lasted(years: f64) -> String {
    if years >= 1.0 {
        format!("{years:.1} ans")
    } else if years * 365.25 < 1.0 {
        format!("{:.0} heures", years * 365.25 * 24.0)
    } else {
        format!("{:.0} jours", years * 365.25)
    }
}

/// Everything a sleeper's step needs from the crew layer.
pub struct SleepCx {
    pub held: bool,
    pub hours: f32,
    pub days: f64,
    pub now: f64,
    pub seed: u64,
    pub rad_sv_day: f64,
    pub rel: f64,
    pub maintained: bool,
    pub medic_skill: f64,
    pub medbay: f64,
}

/// What became of a sleeper in a step.
pub enum Verdict {
    Sleeps,
    Died(&'static str),
    /// Woke: the line for the journal.
    Woke(String),
}

fn sat(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// One step of a sleeper: the body (cold, ice, dose), slow ageing, a caisson that may fail, and
/// the end of the rewarming (the roll of the waking).
pub fn tick_sleeper(p: &mut Person, cx: &SleepCx) -> Verdict {
    let waking_before = p.body.cryo_wake > 0.0;
    let out = p.body.sleep_step(cx.hours, cx.held, ROOM_C, cx.rad_sv_day);
    p.age += (cx.days / 365.25 * crate::body::bio_rate(p.body.core_temp)) as f32;
    if out.dose > 0.0 {
        p.instability = sat(p.instability + 0.02 * out.dose);
    }
    if p.body.acute_dose >= LETHAL_DOSE_SV {
        return Verdict::Died("irradiation");
    }
    if p.body.cryo_ice >= 1.0 {
        return Verdict::Died(if cx.held {
            "cristaux de glace"
        } else {
            "decongelation"
        });
    }
    if cx.held && !waking_before {
        let mut r = Rng::new(hash_coord(cx.seed, i64::from(p.id), cx.now as i64, 83));
        if r.f64() < failure_per_day(cx.rel, cx.maintained, p.body.cryo_ice) * cx.days {
            return Verdict::Died("defaillance du caisson");
        }
    }
    if !out.waking_done {
        return Verdict::Sleeps;
    }
    // The end of the rewarming: the roll.
    let years = years_asleep(p, cx.now);
    let urgent = p.body.cryo_urgent;
    let chance = wake_risk(p, years, cx.rel, cx.medic_skill, cx.medbay, urgent);
    let mut r = Rng::new(hash_coord(cx.seed, i64::from(p.id), cx.now as i64, 89));
    if r.f64() < chance {
        return Verdict::Died("reanimation ratee");
    }
    let ice = p.body.cryo_ice;
    p.body.thaw(cx.now);
    let loss = memory_loss(years, ice);
    for s in &mut p.skills {
        *s *= 1.0 - loss;
    }
    p.health = sat(p.health * (1.0 - 0.5 * ice)).max(0.2);
    p.body.cryo_sick = sat(0.3 + 0.5 * ice + 0.1 * (years as f32).min(5.0) / 5.0);
    p.body.cryo_ice = 0.0;
    let decay = (0.05 * years).min(0.5) as f32;
    for rel in &mut p.relations {
        rel.1 = (f32::from(rel.1) * (1.0 - decay)) as i16;
    }
    p.morale = sat(p.morale - (0.08 * years).min(0.4) as f32);
    if years > 1.0 {
        p.stress = sat(p.stress + 0.1);
    }
    let hint = if loss > 0.02 {
        format!(", {:.0} % de ses acquis oublies", loss * 100.0)
    } else {
        String::new()
    };
    Verdict::Woke(format!(
        "{} sort de cryo apres {}{hint}.",
        p.name,
        lasted(years)
    ))
}

/// A summary of the cryo system now.
#[derive(Clone, Copy, Debug, Default)]
pub struct CryoStatus {
    /// Places of the powered pods, of the pods that exist at all (powered or not).
    pub pods: usize,
    pub pods_installed: usize,
    pub sleepers: usize,
    pub held: usize,
    pub waking: usize,
    pub colonists: u32,
    pub free: usize,
    pub rel: f64,
    pub temp: f64,
    pub hospital: bool,
    pub infirmary: bool,
}

impl Sim {
    /// What the ship offers the sleepers now.
    pub fn cryo_ctx_now(&self) -> CryoCtx {
        let e = &self.effective;
        let cap = e.cryo_cap.floor().max(0.0) as usize;
        let upkeep = {
            let (mut w, mut s) = (0.0, 0.0);
            for (_, m) in self.ship.design.modules(&self.data) {
                if m.cryo > 0.0 {
                    w += m.cryo;
                    s += m.cryo * m.cryo_upkeep;
                }
            }
            if w > 0.0 {
                s / w
            } else {
                UPKEEP_DEFAULT
            }
        };
        CryoCtx {
            cap,
            rel: e.cryo_rel,
            temp: e.cryo_temp,
            upkeep,
            hospital: self.docked.is_some(),
        }
    }

    pub fn cryo_status(&self) -> CryoStatus {
        let ctx = self.cryo_ctx_now();
        let slots = ctx.slots(self.effective.medbay);
        let sleepers: Vec<&Person> = self
            .crew
            .members
            .iter()
            .filter(|p| p.alive && p.body.cryo)
            .collect();
        let held = sleepers.len().min(slots);
        let pods_installed: usize = self
            .ship
            .design
            .modules(&self.data)
            .map(|(_, m)| m.cryo as usize)
            .sum();
        let taken = (sleepers.len() + self.crew.cryo.colonists as usize).min(ctx.cap);
        CryoStatus {
            pods: ctx.cap,
            pods_installed,
            sleepers: sleepers.len(),
            held,
            waking: sleepers.iter().filter(|p| p.body.cryo_wake > 0.0).count(),
            colonists: self.crew.cryo.colonists,
            free: if ctx.hospital {
                usize::MAX
            } else {
                (ctx.cap + usize::from(self.effective.medbay > 0.0))
                    .saturating_sub(sleepers.len() + self.crew.cryo.colonists as usize)
            },
            rel: ctx.reliability(),
            temp: f64::from(ctx.set_temp(taken.min(ctx.cap.saturating_sub(1)))),
            hospital: ctx.hospital,
            infirmary: self.effective.medbay > 0.0,
        }
    }

    /// Free places for one more sleeper (pods, then the infirmary; unlimited at a hospital).
    pub fn cryo_free_slots(&self) -> usize {
        self.cryo_status().free
    }

    fn medic_skill_now(&self) -> f64 {
        let Some(med) = self.data.skill_index("medecine") else {
            return 0.0;
        };
        self.crew
            .members
            .iter()
            .filter(|p| {
                p.alive
                    && !p.body.cryo
                    && !p.body.unconscious()
                    && self.data.roles[p.role].id == "medic"
            })
            .map(|p| f64::from(p.skills[med]))
            .fold(0.0, f64::max)
    }

    /// Put a member to sleep (a caisson with room, the powered infirmary, or the hospital of the
    /// station): the core cools over hours, the blood takes cryoprotectant from the hold, wounds
    /// and diseases stand still. Someone who does not want it, healthy, is a crime (`cryo_forcee`).
    pub fn cryo_put(&mut self, member: usize) -> Result<String, &'static str> {
        let p = self.crew.members.get(member).ok_or("membre inconnu")?;
        if !p.alive {
            return Err("le patient est mort");
        }
        if p.body.cryo {
            return Err("deja en cryostase");
        }
        let st = self.cryo_status();
        if st.free == 0 {
            return Err(if st.pods_installed > 0 {
                "plus de place en caisson (ou panne d'energie)"
            } else {
                "pas de caisson cryo ni d'infirmerie alimentee"
            });
        }
        let unwilling = p.loyalty < UNWILLING_LOYALTY && !p.body.is_hurt() && p.disease.is_none();
        let ctx = self.cryo_ctx_now();
        let in_pods = self
            .crew
            .members
            .iter()
            .filter(|q| q.alive && q.body.cryo)
            .count()
            + self.crew.cryo.colonists as usize;
        let set = ctx.set_temp(in_pods);
        let data = std::sync::Arc::clone(&self.data);
        let got = data
            .resource_index("cryoprotectant")
            .map_or(0.0, |r| self.ship.cargo.remove(&data, r, CPA_KG));
        let cpa = (got / CPA_KG) as f32;
        let now = self.time;
        let name = {
            let p = &mut self.crew.members[member];
            p.body.cryo = true;
            p.body.cryo_since = now;
            p.body.cryo_set = set;
            p.body.cryo_cpa = cpa;
            p.body.cryo_wake = 0.0;
            p.body.cryo_ice = p.body.cryo_ice.max(0.0);
            p.name.clone()
        };
        let mut report = format!("{name} entre en cryostase ({set:.0} C)");
        if cpa < 0.99 {
            report.push_str(&format!(
                ", cryoprotecteur {:.0} % : risque de cristaux",
                cpa * 100.0
            ));
        }
        if unwilling {
            self.offend(None, "cryo_forcee");
            for (i, q) in self.crew.members.iter_mut().enumerate() {
                if q.alive {
                    q.morale = sat(q.morale
                        - if i == member {
                            FORCED_MORALE_VICTIM
                        } else {
                            FORCED_MORALE_OTHERS
                        });
                    q.loyalty = sat(q.loyalty - FORCED_LOYALTY);
                }
            }
            self.crew.members[member].body.trauma =
                sat(self.crew.members[member].body.trauma + 0.3);
            report.push_str(" (CONTRE SON GRE)");
        } else {
            self.offend(None, "cryo_interdite");
        }
        self.crew.log_event(now, report.clone());
        Ok(report)
    }

    /// The old name of `cryo_put` (the infirmary's cryostasis).
    pub fn cryo_start(&mut self, member: usize) -> Result<String, &'static str> {
        self.cryo_put(member)
    }

    /// Begin the controlled rewarming of a sleeper (hours later they are out, or lost).
    pub fn cryo_wake(&mut self, member: usize) -> Result<String, &'static str> {
        self.cryo_wake_with(member, false)
    }

    pub fn cryo_wake_with(&mut self, member: usize, urgent: bool) -> Result<String, &'static str> {
        let medic = self.medic_skill_now();
        let medbay = self.effective.medbay;
        let rel = self.cryo_ctx_now().reliability();
        let now = self.time;
        let p = self.crew.members.get_mut(member).ok_or("membre inconnu")?;
        if !p.alive {
            return Err("le patient est mort");
        }
        if !p.body.cryo {
            return Err("pas en cryostase");
        }
        if p.body.cryo_wake > 0.0 {
            return Err("deja en cours de reveil");
        }
        let hours = wake_hours(medic, medbay, urgent);
        p.body.cryo_wake = hours;
        p.body.cryo_urgent = urgent;
        let risk = wake_risk(p, years_asleep(p, now), rel, medic, medbay, urgent);
        let report = format!(
            "{} : rechauffement {} ({hours:.0} h, risque {:.0} %)",
            p.name,
            if urgent { "D'URGENCE" } else { "progressif" },
            risk * 100.0
        );
        self.crew.log_event(now, report.clone());
        Ok(report)
    }

    /// The odds that waking `member` now fails (for the screen).
    pub fn cryo_wake_risk(&self, member: usize) -> Option<f64> {
        let p = self.crew.members.get(member)?;
        if !p.alive || !p.body.cryo {
            return None;
        }
        Some(wake_risk(
            p,
            years_asleep(p, self.time),
            self.cryo_ctx_now().reliability(),
            self.medic_skill_now(),
            self.effective.medbay,
            false,
        ))
    }

    /// Schedule a wake.
    pub fn cryo_order(&mut self, who: Who, when: When) -> String {
        let t = match when {
            When::At(t) => format!("A LA DATE {:.1} J", t / DAY),
            When::Arrival => "A L'ARRIVEE".to_string(),
            When::Alert => "EN CAS D'ALERTE CRITIQUE".to_string(),
        };
        self.crew.cryo.orders.push(WakeOrder { who, when });
        format!("REVEIL PROGRAMME {t}")
    }

    pub fn cryo_clear_orders(&mut self) {
        self.crew.cryo.orders.clear();
    }

    pub fn cryo_set_rotation(&mut self, days: f32) {
        self.crew.cryo.rotation_days = days.max(0.0);
        self.crew.cryo.last_rotation = self.time;
    }

    /// Everyone to sleep but the `keep` best watchers, a wake of all at the arrival: for a long
    /// jump or transit. Returns the report.
    pub fn cryo_auto_plan(&mut self, keep: usize) -> Result<String, &'static str> {
        let data = std::sync::Arc::clone(&self.data);
        let mut awake: Vec<usize> = (0..self.crew.members.len())
            .filter(|&i| self.crew.members[i].alive && !self.crew.members[i].body.cryo)
            .collect();
        if awake.len() <= keep {
            return Err("personne a endormir");
        }
        if self.cryo_free_slots() == 0 {
            return Err("aucune place en caisson");
        }
        awake.sort_by(|&a, &b| {
            watch_score(&data, &self.crew.members[b])
                .total_cmp(&watch_score(&data, &self.crew.members[a]))
                .then(a.cmp(&b))
        });
        let (mut done, mut skipped) = (0, 0);
        for &i in awake.iter().skip(keep) {
            if self.cryo_put(i).is_ok() {
                done += 1;
            } else {
                skipped += 1;
            }
        }
        if done == 0 {
            return Err("aucune place en caisson");
        }
        self.crew.cryo.keep_awake = keep.min(255) as u8;
        self.crew.cryo.orders.retain(|o| o.when != When::Arrival);
        self.crew.cryo.orders.push(WakeOrder {
            who: Who::All,
            when: When::Arrival,
        });
        Ok(format!(
            "{done} ENDORMIS, {keep} VEILLEUR(S){} ; REVEIL A L'ARRIVEE",
            if skipped > 0 {
                format!(", {skipped} SANS PLACE")
            } else {
                String::new()
            }
        ))
    }

    /// Put hold colonists (kg of "colonists", 80 kg a head) to sleep in the free pods.
    pub fn cryo_load_colonists(&mut self, n: u32) -> Result<String, &'static str> {
        let data = std::sync::Arc::clone(&self.data);
        let col = data
            .resource_index("colonists")
            .ok_or("donnees incompletes")?;
        let st = self.cryo_status();
        let room = st.pods.saturating_sub(st.sleepers + st.colonists as usize);
        if n == 0 || room == 0 {
            return Err("plus de place en caisson");
        }
        let n = n.min(room as u32);
        let kg = f64::from(n) * crate::colony::COLONIST_KG;
        if self.ship.cargo.kg[col] + 1e-6 < kg {
            return Err("pas assez de colons en soute");
        }
        self.ship.cargo.remove(&data, col, kg);
        let need = f64::from(n) * COLONIST_CPA_KG;
        let got = data
            .resource_index("cryoprotectant")
            .map_or(0.0, |r| self.ship.cargo.remove(&data, r, need));
        let cpa = (got / need) as f32;
        let plan = &mut self.crew.cryo;
        let (old, new) = (f64::from(plan.colonists), f64::from(n));
        plan.colonists_since = (plan.colonists_since * old + self.time * new) / (old + new);
        plan.colonists_cpa =
            (plan.colonists_cpa * old as f32 + cpa * new as f32) / (old + new) as f32;
        plan.colonists += n;
        let text = format!(
            "{n} colons entrent en cryo (cryoprotecteur {:.0} %)",
            cpa * 100.0
        );
        self.crew.log_event(self.time, text.clone());
        Ok(text)
    }

    /// Wake the dormant colonists: how many come out alive (deterministic by the date).
    pub(crate) fn cryo_thaw_colonists(&mut self) -> u32 {
        let n = self.crew.cryo.colonists;
        if n == 0 {
            return 0;
        }
        let ctx = self.cryo_ctx_now();
        let years = ((self.time - self.crew.cryo.colonists_since) / (365.25 * DAY)).max(0.0);
        let chance = wake_death_chance(
            years,
            ctx.reliability(),
            self.crew.cryo.colonists_ice,
            30.0,
            self.medic_skill_now(),
            self.effective.medbay,
            false,
        );
        let mut alive = 0;
        for k in 0..n {
            let mut r = Rng::new(hash_coord(
                self.system.seed,
                i64::from(k),
                self.time as i64,
                97,
            ));
            if r.f64() >= chance {
                alive += 1;
            }
        }
        let plan = &mut self.crew.cryo;
        plan.colonists = 0;
        plan.colonists_ice = 0.0;
        let text = format!(
            "{alive} colons sur {n} sortent de cryo apres {}",
            lasted(years)
        );
        self.crew.log_event(self.time, text);
        alive
    }

    /// Wake the dormant colonists at one of our colonies (docked there, or parked above it):
    /// the survivors join its population.
    pub fn cryo_land_colonists(&mut self) -> Result<String, &'static str> {
        if self.crew.cryo.colonists == 0 {
            return Err("aucun colon endormi");
        }
        let ci = self
            .docked_colony()
            .or_else(|| self.parked.and_then(|(b, _)| self.colony_at(b)))
            .ok_or("il faut etre amarre ou pose a une de nos colonies")?;
        let alive = self.cryo_thaw_colonists();
        self.colonies[ci].pop += f64::from(alive);
        let name = self.colonies[ci].name.clone();
        let text = format!("{alive} colons reveilles : {name} grandit");
        self.events.push(Event::MarketNews(text.clone()));
        Ok(text)
    }

    /// Whoever is asleep and can be woken by an order of `who`.
    /// The navigation computer got the ship where it was going (end of a plan, arrival at a
    /// world): the wakes scheduled "on arrival" fire now, as for a new star or a docking.
    pub(crate) fn cryo_arrival_wake(&mut self) {
        let orders = std::mem::take(&mut self.crew.cryo.orders);
        let mut keep = Vec::new();
        for o in orders {
            if o.when != When::Arrival {
                keep.push(o);
                continue;
            }
            for i in self.cryo_pick(o.who) {
                let _ = self.cryo_wake_with(i, false);
            }
        }
        self.crew.cryo.orders = keep;
    }

    fn cryo_pick(&self, who: Who) -> Vec<usize> {
        let sleeping = |p: &Person| p.alive && p.body.cryo && p.body.cryo_wake <= 0.0;
        match who {
            Who::All => (0..self.crew.members.len())
                .filter(|&i| sleeping(&self.crew.members[i]))
                .collect(),
            Who::One(id) => self
                .crew
                .members
                .iter()
                .position(|p| p.id == id && sleeping(p))
                .into_iter()
                .collect(),
            Who::Best => (0..self.crew.members.len())
                .filter(|&i| sleeping(&self.crew.members[i]))
                .max_by(|&a, &b| {
                    watch_score(&self.data, &self.crew.members[a])
                        .total_cmp(&watch_score(&self.data, &self.crew.members[b]))
                        .then(b.cmp(&a))
                })
                .into_iter()
                .collect(),
        }
    }

    /// Once per step, after the crew layer: tell the awakened what changed in the world, fire the
    /// scheduled wakes, rotate the watch, and keep the colonists in their pods.
    pub(crate) fn cryo_tick(&mut self) {
        let now = self.time;
        // The world they come back to.
        let woken = std::mem::take(&mut self.crew.woken);
        for w in woken {
            let news: Vec<String> = self
                .news
                .iter()
                .rev()
                .filter(|n| n.time >= w.since)
                .take(3)
                .map(|n| n.text.clone())
                .collect();
            let mut text = format!("{} : {} d'absence", w.name, lasted(w.years));
            if w.years >= 0.5 {
                text.push_str(&format!(
                    ", les proches restes ont vieilli de {:.1} ans, la paie a ete retenue",
                    w.years
                ));
            }
            self.crew.log_event(now, text.clone());
            self.events.push(Event::MarketNews(text));
            if !news.is_empty() {
                let t = format!("NOUVELLES DEPUIS : {}", news.join(" / "));
                self.crew.log_event(now, t.clone());
                self.events.push(Event::MarketNews(t));
            }
        }
        // Arrival: a new star, or docking.
        let docked = self.docked.is_some();
        let arrived = (docked && !self.crew.cryo.was_docked)
            || self.crew.cryo.last_here.is_some_and(|h| h != self.here);
        self.crew.cryo.was_docked = docked;
        self.crew.cryo.last_here = Some(self.here);
        // Alerts are only read when an order waits for them.
        let mut alert = false;
        if self.crew.cryo.orders.iter().any(|o| o.when == When::Alert)
            && now - self.crew.cryo.last_check >= ALERT_CHECK
        {
            self.crew.cryo.last_check = now;
            alert = self
                .alerts()
                .iter()
                .any(|a| a.sev == crate::alerts::Severity::Critical);
        }
        let orders = std::mem::take(&mut self.crew.cryo.orders);
        let mut keep = Vec::new();
        for o in orders {
            let fire = match o.when {
                When::At(t) => now >= t,
                When::Arrival => arrived,
                When::Alert => alert,
            };
            if !fire {
                keep.push(o);
                continue;
            }
            let urgent = o.when == When::Alert;
            let who = self.cryo_pick(o.who);
            for i in who {
                let _ = self.cryo_wake_with(i, urgent);
            }
        }
        self.crew.cryo.orders = keep;
        // Rotation of the watch.
        let rot = self.crew.cryo.rotation_days;
        if rot > 0.0 && now - self.crew.cryo.last_rotation >= f64::from(rot) * DAY {
            self.crew.cryo.last_rotation = now;
            let rested = self
                .crew
                .members
                .iter()
                .enumerate()
                .filter(|(_, p)| p.alive && p.body.cryo && p.body.cryo_wake <= 0.0)
                .min_by(|a, b| a.1.body.cryo_since.total_cmp(&b.1.body.cryo_since))
                .map(|(i, _)| i);
            let tired = self
                .crew
                .members
                .iter()
                .enumerate()
                .filter(|(_, p)| p.alive && !p.body.cryo)
                .min_by(|a, b| a.1.body.cryo_last.total_cmp(&b.1.body.cryo_last))
                .map(|(_, p)| p.id);
            if let (Some(i), Some(id)) = (rested, tired) {
                if self.cryo_wake_with(i, false).is_ok() {
                    self.crew.cryo.pending_sleep.push(id);
                }
            }
        }
        // The other half of a rotation: the tired go under when a place is free.
        let pending = std::mem::take(&mut self.crew.cryo.pending_sleep);
        let mut still = Vec::new();
        for id in pending {
            let Some(i) = self
                .crew
                .members
                .iter()
                .position(|p| p.id == id && p.alive && !p.body.cryo)
            else {
                continue;
            };
            if self.cryo_free_slots() > 0 {
                let _ = self.cryo_put(i);
            } else {
                still.push(id);
            }
        }
        self.crew.cryo.pending_sleep = still;
        // Colonists: whoever does not fit in the powered pods is lost; the ice regrows in a warm pod.
        let dt = (now - self.crew.cryo.last_tick).clamp(0.0, 400.0 * DAY);
        self.crew.cryo.last_tick = now;
        if self.crew.cryo.colonists > 0 {
            let ctx = self.cryo_ctx_now();
            let sleepers = self
                .crew
                .members
                .iter()
                .filter(|p| p.alive && p.body.cryo)
                .count();
            let room = ctx.cap.saturating_sub(sleepers) as u32;
            if self.crew.cryo.colonists > room {
                let lost = self.crew.cryo.colonists - room;
                self.crew.cryo.colonists = room;
                let t = format!("PANNE DE CAISSONS : {lost} colons endormis perdus");
                self.crew.log_event(now, t.clone());
                self.events.push(Event::MarketNews(t));
            }
            if ctx.temp as f32 > crate::body::RECRYSTALLISE_BELOW {
                let warm =
                    ((ctx.temp as f32 - crate::body::RECRYSTALLISE_BELOW) / 110.0).clamp(0.0, 1.0);
                self.crew.cryo.colonists_ice += crate::body::RECRYSTALLISE
                    * warm
                    * (1.0 - crate::body::ICE_PROTECTION * self.crew.cryo.colonists_cpa)
                    * (dt / DAY) as f32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;

    fn sim() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let d = Arc::new(GameData::load(&dir).expect("data loads"));
        let mut s = new_sim(5, d);
        s.credits = 1.0e6;
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        // A powered bank of caissons (stats set by hand: the ship of the scene has none).
        s.effective.cryo_cap = 4.0;
        s.effective.cryo_rel = 0.97;
        s.effective.cryo_temp = -130.0;
        s.effective.medbay = 0.0;
        s.effective.life_ok = true;
        s.effective.life_capacity = 20.0;
        s.docked = None;
        s.parked = None;
        // A crew of four: the engineer of the scene, a pilot, a medic and a cook.
        let d = Arc::clone(&s.data);
        for (k, id) in ["pilot", "medic", "cook"].into_iter().enumerate() {
            let p = crate::crew::generate_person(&d, 0x51EE9 + k as u64, None);
            let role = crate::crew::Crew::role_index(&d, id);
            s.crew.hire(&d, p, role, 0.0);
        }
        s
    }

    fn give(s: &mut Sim, id: &str, kg: f64) {
        let d = Arc::clone(&s.data);
        let r = d.resource_index(id).expect("resource");
        s.ship.cargo.add(&d, r, kg);
    }

    /// Advance `days` of game time through the crew layer and the cryo tick (the effective
    /// capabilities are left as the test set them).
    fn run(s: &mut Sim, days: f64) {
        let mut left = days;
        while left > 0.0 {
            let d = left.min(1.0);
            s.time += d * DAY;
            s.update_crew();
            s.cryo_tick();
            left -= d;
        }
    }

    fn start(s: &mut Sim) {
        let t = s.time.max(1.0);
        s.time = t;
        s.crew.last = t;
        s.crew.cryo.last_tick = t;
    }

    #[test]
    fn a_sleeper_needs_no_food_water_or_air_and_ages_by_the_biological_factor() {
        let mut s = sim();
        start(&mut s);
        give(&mut s, "cryoprotectant", 20.0);
        s.cryo_put(0).expect("a pod is free");
        for _ in 0..8 {
            run(&mut s, 0.5);
        }
        // Cooling is done: the core is at the set temperature.
        let p = s.crew.members[0].clone();
        assert!(p.body.core_temp < -100.0, "{}", p.body.core_temp);
        // Now measure a long sleep: no hunger or thirst, slow ageing.
        let (a0, h0, t0) = (p.age, p.hunger, p.thirst);
        let years = 2.0;
        let rich = |s: &mut Sim| {
            let food = s.data.resource_index("food").expect("food");
            let water = s.data.resource_index("water").expect("water");
            s.ship.cargo.kg[food] = 1.0e5;
            s.ship.cargo.kg[water] = 1.0e5;
            (food, water)
        };
        let (food, _) = rich(&mut s);
        run(&mut s, years * 365.25);
        let q = &s.crew.members[0];
        assert!(q.alive, "it survives two years in a vitrifying pod");
        let aged = f64::from(q.age - a0);
        let factor = crate::body::bio_rate(q.body.core_temp);
        assert!(factor < 0.001, "{factor}");
        assert!(
            (aged - years * factor).abs() < 0.01 * years.max(aged),
            "aged {aged} for {years} y at x{factor}"
        );
        assert!((q.hunger - h0).abs() < 1e-6 && (q.thirst - t0).abs() < 1e-6);
        // The food drawn from the hold: the same month with the sleeper awake costs more.
        let month = |sleeps: bool| {
            let mut s = sim();
            start(&mut s);
            give(&mut s, "cryoprotectant", 5.0);
            if sleeps {
                s.cryo_put(0).expect("ok");
            }
            let (food, _) = rich(&mut s);
            run(&mut s, 30.0);
            1.0e5 - s.ship.cargo.kg[food]
        };
        let (awake, asleep) = (month(false), month(true));
        assert!(
            awake - asleep > 0.35 * 30.0,
            "the sleeper eats nothing: {awake} kg awake vs {asleep} kg"
        );
        let _ = food;
    }

    #[test]
    fn the_body_cools_by_a_lag_and_ice_depends_on_the_cryoprotectant() {
        let mut s = sim();
        start(&mut s);
        // Without cryoprotectant in the hold: the crossing of the freezing band grows ice.
        s.cryo_put(0).expect("ok");
        assert!(s.crew.members[0].body.cryo_cpa < 0.01);
        run(&mut s, 1.0);
        let bare = s.crew.members[0].body.cryo_ice;
        // With a full load: far less.
        give(&mut s, "cryoprotectant", 5.0);
        s.cryo_put(1).expect("ok");
        assert!(s.crew.members[1].body.cryo_cpa > 0.99);
        run(&mut s, 1.0);
        let loaded = s.crew.members[1].body.cryo_ice;
        assert!(bare > 0.3 && loaded < 0.1, "bare {bare} loaded {loaded}");
        // It was not instantaneous: a lag of hours (a mid temperature exists a few hours in).
        let mut b = crate::body::Body {
            cryo: true,
            cryo_set: -130.0,
            ..Default::default()
        };
        b.sleep_step(1.0, true, ROOM_C, 0.0);
        assert!(
            b.core_temp > -130.0 && b.core_temp < 37.0,
            "{}",
            b.core_temp
        );
        for _ in 0..11 {
            b.sleep_step(1.0, true, ROOM_C, 0.0);
        }
        assert!(b.core_temp < -125.0, "{}", b.core_temp);
    }

    #[test]
    fn a_warm_caisson_regrows_ice_a_deep_one_does_not() {
        let mut warm = crate::body::Body {
            cryo: true,
            core_temp: -80.0,
            cryo_set: -80.0,
            ..Default::default()
        };
        let mut deep = warm.clone();
        deep.core_temp = -196.0;
        deep.cryo_set = -196.0;
        for _ in 0..365 {
            warm.sleep_step(24.0, true, ROOM_C, 0.0);
            deep.sleep_step(24.0, true, ROOM_C, 0.0);
        }
        assert!(warm.cryo_ice > 0.2, "{}", warm.cryo_ice);
        assert!(deep.cryo_ice < 1e-6, "{}", deep.cryo_ice);
    }

    #[test]
    fn waking_mortality_rises_with_the_years_and_falls_with_reliability_and_care() {
        let c = |years, rel, medic, medbay| {
            wake_death_chance(years, rel, 0.1, 35.0, medic, medbay, false)
        };
        assert!(c(0.1, 0.97, 0.0, 0.0) < c(5.0, 0.97, 0.0, 0.0));
        assert!(c(5.0, 0.97, 0.0, 0.0) < c(50.0, 0.97, 0.0, 0.0));
        assert!(c(5.0, 0.995, 0.0, 0.0) < c(5.0, 0.90, 0.0, 0.0));
        assert!(c(5.0, 0.97, 8.0, 1.0) < c(5.0, 0.97, 0.0, 0.0));
        assert!(
            wake_death_chance(5.0, 0.97, 0.1, 35.0, 0.0, 0.0, true)
                > wake_death_chance(5.0, 0.97, 0.1, 35.0, 0.0, 0.0, false),
            "haste kills"
        );
        assert!(wake_death_chance(5.0, 0.97, 0.6, 35.0, 0.0, 0.0, false) > c(5.0, 0.97, 0.0, 0.0));
        assert!(wake_death_chance(5.0, 0.97, 0.1, 80.0, 0.0, 0.0, false) > c(5.0, 0.97, 0.0, 0.0));
        assert!(wake_hours(8.0, 1.0, false) < wake_hours(0.0, 0.0, false));
        assert!(wake_hours(0.0, 0.0, true) < wake_hours(8.0, 1.0, false));
    }

    #[test]
    fn waking_is_progressive_costs_memory_and_leaves_the_waker_sick() {
        let mut s = sim();
        start(&mut s);
        give(&mut s, "cryoprotectant", 5.0);
        s.cryo_put(0).expect("ok");
        run(&mut s, 3.0 * 365.25);
        let skill_before: f32 = s.crew.members[0].skills.iter().sum();
        s.effective.cryo_rel = 1.0;
        s.cryo_wake(0).expect("starts");
        assert!(s.cryo_wake(0).is_err(), "already waking");
        // Hours later: still cold, still asleep.
        run(&mut s, 0.25);
        let mid = s.crew.members[0].body.core_temp;
        assert!(
            s.crew.members[0].body.cryo && mid > -130.0 && mid < 36.0,
            "{mid}"
        );
        run(&mut s, 2.0);
        let p = &s.crew.members[0];
        if p.alive {
            assert!(!p.body.cryo && (p.body.core_temp - 37.0).abs() < 1.0);
            let after: f32 = p.skills.iter().sum();
            assert!(
                after < skill_before,
                "memory lost: {after} < {skill_before}"
            );
            assert!(p.body.cryo_sick > 0.2, "cryo sickness {}", p.body.cryo_sick);
        } else {
            // A rare loss on the roll: still a legal outcome, the log says why.
            assert!(s.crew.log.iter().any(|l| l.text.contains("mort")));
        }
    }

    #[test]
    fn a_power_cut_without_battery_kills_the_sleepers() {
        let mut s = sim();
        start(&mut s);
        give(&mut s, "cryoprotectant", 5.0);
        s.cryo_put(0).expect("ok");
        s.cryo_put(1).expect("ok");
        run(&mut s, 2.0);
        assert!(s.crew.members[0].alive && s.crew.members[1].alive);
        // Power lost: no pod is powered.
        s.effective.cryo_cap = 0.0;
        let st = s.cryo_status();
        assert_eq!(st.held, 0, "nothing holds them");
        run(&mut s, 3.0);
        assert!(
            !s.crew.members[0].alive && !s.crew.members[1].alive,
            "they die of the thaw"
        );
        assert!(s.crew.log.iter().any(|l| l.text.contains("decongelation")));
    }

    #[test]
    fn a_power_cut_with_the_caissons_back_in_time_saves_them() {
        let mut s = sim();
        start(&mut s);
        give(&mut s, "cryoprotectant", 5.0);
        s.cryo_put(0).expect("ok");
        run(&mut s, 2.0);
        s.effective.cryo_cap = 0.0;
        run(&mut s, 0.02); // half an hour
        s.effective.cryo_cap = 4.0;
        run(&mut s, 3.0);
        assert!(s.crew.members[0].alive, "a short cut is survived");
    }

    #[test]
    fn a_scheduled_wake_fires_at_its_date_and_not_before() {
        let mut s = sim();
        start(&mut s);
        give(&mut s, "cryoprotectant", 5.0);
        s.cryo_put(0).expect("ok");
        let at = s.time + 10.0 * DAY;
        s.cryo_order(Who::All, When::At(at));
        run(&mut s, 9.0);
        assert!(s.crew.members[0].body.cryo_wake <= 0.0, "not yet");
        run(&mut s, 1.5);
        assert!(
            s.crew.members[0].body.cryo_wake > 0.0 || !s.crew.members[0].body.cryo,
            "the wake has begun"
        );
        assert!(s.crew.cryo.orders.is_empty(), "the order is spent");
    }

    #[test]
    fn the_auto_plan_keeps_n_awake_and_wakes_all_at_the_arrival() {
        let mut s = sim();
        start(&mut s);
        give(&mut s, "cryoprotectant", 20.0);
        s.effective.cryo_cap = 12.0;
        let n = s.crew.members.len();
        assert!(n >= 3, "{n}");
        let msg = s.cryo_auto_plan(1).expect("plan");
        assert!(msg.contains("VEILLEUR"), "{msg}");
        let asleep = s.crew.members.iter().filter(|p| p.body.cryo).count();
        assert_eq!(asleep, n - 1);
        // The ship docks: the arrival wakes everybody.
        run(&mut s, 1.0);
        s.docked = Some(0);
        run(&mut s, 0.1);
        let waking = s
            .crew
            .members
            .iter()
            .filter(|p| p.body.cryo_wake > 0.0 || !p.body.cryo)
            .count();
        assert_eq!(waking, n, "all wake at the arrival");
    }

    #[test]
    fn the_watch_rotates_and_the_tired_go_under() {
        let mut s = sim();
        start(&mut s);
        give(&mut s, "cryoprotectant", 20.0);
        s.effective.cryo_cap = 12.0;
        s.cryo_auto_plan(1).expect("plan");
        s.crew.cryo.orders.clear();
        let watcher = s
            .crew
            .members
            .iter()
            .find(|p| !p.body.cryo)
            .map(|p| p.id)
            .expect("one is awake");
        s.cryo_set_rotation(5.0);
        run(&mut s, 5.5);
        run(&mut s, 3.0);
        let w = s.crew.members.iter().find(|p| p.id == watcher).expect("w");
        assert!(w.body.cryo, "the first watcher has gone to sleep");
        assert!(
            s.crew.members.iter().any(|p| !p.body.cryo && p.alive),
            "someone else keeps the watch"
        );
    }

    #[test]
    fn radiation_adds_up_asleep_and_the_dose_is_not_mended() {
        let mut s = sim();
        start(&mut s);
        give(&mut s, "cryoprotectant", 5.0);
        s.cryo_put(0).expect("ok");
        for _ in 0..20 {
            s.irradiate(0.5);
            run(&mut s, 1.0);
        }
        let d = s.crew.members[0].body.acute_dose;
        assert!(d > 5.0, "{d}");
        // An awake person with the same exposure would have mended (half-life 20 days).
        let mut awake = crate::body::Body::default();
        for _ in 0..20 {
            awake.acute_dose += 0.5;
            awake.acute_dose *= 0.5f32.powf(24.0 / 480.0);
        }
        assert!(awake.acute_dose < d, "{} < {d}", awake.acute_dose);
    }

    #[test]
    fn pay_falls_to_the_retainer_and_nobody_asleep_counts_as_a_mouth() {
        let mut s = sim();
        start(&mut s);
        give(&mut s, "cryoprotectant", 5.0);
        let full = s.crew.daily_payroll();
        let salary = s.crew.members[0].salary;
        s.cryo_put(0).expect("ok");
        let now = s.crew.daily_payroll();
        assert!(
            (full - now - salary * (1.0 - RETAINER)).abs() < 1e-6,
            "{full} {now}"
        );
    }

    #[test]
    fn colonists_sleep_in_bulk_and_swell_a_colony_less_the_dead() {
        let mut s = sim();
        start(&mut s);
        give(&mut s, "colonists", 80.0 * 10.0);
        give(&mut s, "cryoprotectant", 20.0);
        s.effective.cryo_cap = 12.0;
        s.cryo_load_colonists(10).expect("loaded");
        assert_eq!(s.crew.cryo.colonists, 10);
        assert!(s.cryo_load_colonists(10).is_err(), "no room left for more");
        // The power fails: those over the capacity are lost.
        s.effective.cryo_cap = 4.0;
        run(&mut s, 1.0);
        assert!(s.crew.cryo.colonists <= 4, "{}", s.crew.cryo.colonists);
    }

    #[test]
    fn a_sleeper_and_the_cryo_state_survive_saving() {
        let mut s = sim();
        start(&mut s);
        give(&mut s, "cryoprotectant", 5.0);
        s.cryo_put(0).expect("ok");
        s.cryo_order(Who::Best, When::Arrival);
        run(&mut s, 2.0);
        let bytes = s.save_bytes().expect("saves");
        let t = s.crew.members[0].body.clone();
        let d = Arc::clone(&s.data);
        let loaded = Sim::load_bytes(&bytes, d).expect("loads");
        let b = &loaded.crew.members[0].body;
        assert!(b.cryo && (b.core_temp - t.core_temp).abs() < 1e-4);
        assert!((b.cryo_ice - t.cryo_ice).abs() < 1e-6 && b.cryo_since == t.cryo_since);
        assert_eq!(loaded.crew.cryo.orders.len(), 1);
    }

    #[test]
    fn cryo_runs_are_deterministic() {
        let go = || {
            let mut s = sim();
            start(&mut s);
            give(&mut s, "cryoprotectant", 5.0);
            s.cryo_put(0).expect("ok");
            s.cryo_put(1).expect("ok");
            run(&mut s, 400.0);
            s.cryo_wake(0).expect("wake");
            s.cryo_wake(1).expect("wake");
            run(&mut s, 3.0);
            s.crew
                .members
                .iter()
                .map(|p| (p.alive, p.body.cryo, p.health.to_bits(), p.age.to_bits()))
                .collect::<Vec<_>>()
        };
        assert_eq!(go(), go());
    }

    #[test]
    fn freezing_the_unwilling_is_a_crime_and_costs_loyalty() {
        let mut s = sim();
        start(&mut s);
        give(&mut s, "cryoprotectant", 5.0);
        s.crew.members[1].loyalty = 0.1;
        let l0 = s.crew.members[0].loyalty;
        let r = s.cryo_put(1).expect("done");
        assert!(r.contains("CONTRE SON GRE"), "{r}");
        assert!(s.crew.members[0].loyalty < l0, "the others do not like it");
        // A consenting one is not.
        s.crew.members[0].loyalty = 0.9;
        let r = s.cryo_put(0).expect("done");
        assert!(!r.contains("CONTRE"), "{r}");
    }

    #[test]
    fn pods_of_the_design_give_the_capacity_and_damage_takes_it_away() {
        let mut s = sim();
        let d = Arc::clone(&s.data);
        let pod = d.module_index("cryo_pod_mk2").expect("pod");
        let hull = s.ship.design.hull;
        let slot = d.hulls[hull]
            .slots
            .iter()
            .position(|(t, sz)| *t == vector_data::SlotType::Internal && *sz >= 2)
            .expect("an internal slot");
        s.ship.design.slots[slot] = Some(pod);
        s.ship.refresh_stats(&d);
        s.advance(120.0, &crate::ShipInput::default());
        assert!(
            (s.effective.cryo_cap - 4.0).abs() < 1e-6,
            "{}",
            s.effective.cryo_cap
        );
        assert!((s.effective.cryo_temp + 130.0).abs() < 1e-6);
        assert!(s.effective.cryo_rel > 0.9);
        assert!(s.cryo_status().pods >= 4);
        // A damaged bank holds fewer.
        s.ship.sys.health[slot] = 0.5;
        s.advance(10.0, &crate::ShipInput::default());
        assert!(s.effective.cryo_cap < 3.0, "{}", s.effective.cryo_cap);
    }

    #[test]
    fn sleepers_without_power_raise_a_critical_alert() {
        use crate::alerts::{Severity, Topic};
        let mut s = sim();
        start(&mut s);
        give(&mut s, "cryoprotectant", 5.0);
        s.cryo_put(0).expect("ok");
        assert!(!s.alerts().iter().any(|a| a.topic == Topic::Cryo));
        s.effective.cryo_cap = 0.0;
        assert!(s
            .alerts()
            .iter()
            .any(|a| a.topic == Topic::Cryo && a.sev == Severity::Critical));
    }

    #[test]
    fn gestation_stands_still_while_the_mother_sleeps() {
        let mut s = sim();
        start(&mut s);
        give(&mut s, "cryoprotectant", 5.0);
        s.births_day = (s.time / DAY).floor() as i64;
        let (mid, fid) = (s.crew.members[0].id, s.crew.members[1].id);
        let due = s.time + 30.0 * DAY;
        s.crew.pregnancies.push((mid, fid, due));
        s.cryo_put(0).expect("ok");
        s.time += 10.0 * DAY;
        s.births_tick();
        let moved = s.crew.pregnancies[0].2;
        assert!(moved >= due + 10.0 * DAY - 1.0, "due {due} -> {moved}");
    }

    #[test]
    fn colonists_who_slept_for_ages_wake_fewer() {
        let wake = |years: f64| {
            let mut s = sim();
            start(&mut s);
            s.crew.cryo.colonists = 200;
            s.crew.cryo.colonists_since = s.time;
            s.time += years * 365.25 * DAY;
            s.cryo_thaw_colonists()
        };
        let (short, long) = (wake(1.0), wake(300.0));
        assert!(short > long, "{short} vs {long}");
        assert!(short > 150, "{short}");
    }

    #[test]
    fn free_caissons_add_berths_for_passengers() {
        let mut s = sim();
        let before = s.free_berths();
        s.effective.cryo_cap = 12.0;
        assert_eq!(s.free_berths(), before + 8, "4 were counted, 12 now");
        s.effective.cryo_cap = 0.0;
        assert_eq!(s.free_berths(), before - 4);
    }

    #[test]
    fn a_lethal_dose_kills_in_the_cold_and_any_dose_raises_the_wake_risk() {
        let mut s = sim();
        start(&mut s);
        give(&mut s, "cryoprotectant", 5.0);
        let p0 = s.crew.members[0].clone();
        let mut dosed = p0.clone();
        dosed.body.acute_dose = 2.0;
        assert!(
            wake_risk(&dosed, 1.0, 0.97, 0.0, 0.0, false)
                > wake_risk(&p0, 1.0, 0.97, 0.0, 0.0, false) + 0.05
        );
        s.cryo_put(0).expect("ok");
        s.crew.members[0].body.acute_dose = 9.0;
        run(&mut s, 1.0);
        assert!(!s.crew.members[0].alive, "dead of the dose, asleep");
        assert!(s.crew.log.iter().any(|l| l.text.contains("irradiation")));
    }
}
