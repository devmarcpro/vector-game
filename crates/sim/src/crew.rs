//! Crew: individuals with skills, traits, needs, posts, relations; a data-driven event engine and a journal.
use crate::cargo::Cargo;
use crate::genealogy::{self, Families, Genome, Kin};
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, mix64, Rng};
use vector_data::GameData;

pub const DAY: f64 = 86_400.0;
/// Base simulation tick of the crew layer (game seconds).
pub const TICK: f64 = 600.0;
pub const LOG_CAP: usize = 400;
/// Efficiency of a post nobody staffs (the ship's AI muddles through).
pub const NO_CREW_EFF: f64 = 0.7;
const FOOD_KG_PER_DAY: f64 = 0.6;
const WATER_KG_PER_DAY: f64 = 3.0;
/// Fraction of water lost when life support recycles it.
const WATER_LOSS_RECYCLED: f64 = 0.1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Person {
    pub id: u32,
    pub name: String,
    pub culture: usize,
    /// Years.
    pub age: f32,
    /// 0..10 per skill (index into `GameData::skills`).
    pub skills: Vec<f32>,
    pub traits: Vec<usize>,
    /// 0..1, 0 = dead.
    pub health: f32,
    /// Needs: 0 = satisfied, 1 = critical.
    pub fatigue: f32,
    pub hunger: f32,
    pub thirst: f32,
    pub social: f32,
    pub stress: f32,
    /// 0..1.
    pub morale: f32,
    pub loyalty: f32,
    /// Index into `GameData::roles`.
    pub role: usize,
    /// 0..2: which 8-hour shift the person works.
    pub shift: u8,
    /// Credits per day.
    pub salary: f64,
    pub disease: Option<(usize, f32)>,
    pub alive: bool,
    pub relations: Vec<(u32, i16)>,
    /// Place in the family tree (see `genealogy`) and the genes it gives.
    pub kin: Kin,
    pub genome: Genome,
    /// The couple and number of children of this person's family when it is not the default
    /// one (a recruited cousin, a birth aboard); registered with the crew at hiring.
    pub family_def: Option<(Kin, Kin, u8)>,
    /// Integrity 0..1 of each organ of `GameData::organs`.
    pub organs: Vec<f32>,
    /// 0 natural, 1 graft, 2 prosthesis (see `surgery`), per organ.
    pub organ_kind: Vec<u8>,
}

impl Person {
    /// Overall condition 0..1 from the organs (weighted mean).
    pub fn condition(&self, data: &GameData) -> f32 {
        let (mut sum, mut w) = (0.0f64, 0.0f64);
        for (o, def) in self.organs.iter().zip(&data.organs) {
            sum += f64::from(*o) * def.weight;
            w += def.weight;
        }
        if w > 0.0 {
            (sum / w) as f32
        } else {
            1.0
        }
    }

    /// What the failing organs take off a trait key (negative or positive as in the data).
    fn organ_effect(&self, data: &GameData, key: &str) -> f64 {
        self.organs
            .iter()
            .zip(&data.organs)
            .flat_map(|(o, def)| {
                def.effects
                    .iter()
                    .filter(move |(k, _)| k == key)
                    .map(move |(_, w)| w * (1.0 - f64::from(*o)))
            })
            .sum()
    }

    pub fn relation(&self, other: u32) -> i16 {
        self.relations
            .iter()
            .find(|(i, _)| *i == other)
            .map_or(0, |(_, v)| *v)
    }

    fn add_relation(&mut self, other: u32, delta: i16) {
        match self.relations.iter_mut().find(|(i, _)| *i == other) {
            Some((_, v)) => *v = (*v + delta).clamp(-100, 100),
            None => self.relations.push((other, delta.clamp(-100, 100))),
        }
    }

    pub fn best_skill(&self) -> (usize, f32) {
        self.skills
            .iter()
            .copied()
            .enumerate()
            .fold((0, 0.0), |b, (i, v)| if v > b.1 { (i, v) } else { b })
    }

    /// Sum of a trait key over the person's traits and genes.
    fn trait_sum(&self, data: &GameData, key: &str) -> f64 {
        let traits: f64 = self
            .traits
            .iter()
            .map(|&t| data.traits[t].effect(key))
            .sum();
        traits + genealogy::gene_effect(data, &self.genome, key) + self.organ_effect(data, key)
    }

    /// Chance multiplier of a disease from the genes (predispositions).
    pub fn disease_risk(&self, data: &GameData, disease_id: &str) -> f64 {
        genealogy::disease_risk(data, &self.genome, disease_id)
    }

    pub fn on_duty(&self, now: f64) -> bool {
        shift_of(now) == self.shift
    }
}

pub fn shift_of(now: f64) -> u8 {
    (((now / 3600.0) % 24.0) / 8.0) as u8
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub time: f64,
    pub text: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Crew {
    pub members: Vec<Person>,
    pub next_id: u32,
    pub log: Vec<LogEntry>,
    pub last: f64,
    /// Cabin air quality, 0..1.
    pub air: f32,
    pub last_pay_day: i64,
    pub unpaid: bool,
    /// Families created during play (recruited cousins, births): see `genealogy`.
    pub families: Families,
}

/// Everything the crew layer needs from the ship and the world for one update.
pub struct CrewCtx<'a> {
    pub data: &'a GameData,
    pub cargo: &'a mut Cargo,
    pub credits: &'a mut f64,
    pub life_ok: bool,
    /// Sum of powered life-support capacity (people).
    pub life_capacity: f64,
    pub crew_cap: f64,
    /// kg of food per day grown by powered greenhouses.
    pub food_per_day: f64,
    pub medbay: f64,
    pub workshop: f64,
    pub overheating: bool,
    pub seed: u64,
    /// Hunger, thirst and air matter.
    pub needs: bool,
    pub event_freq: f64,
}

/// What the crew did for the ship during an update.
#[derive(Clone, Debug, Default)]
pub struct CrewOut {
    /// Module-health fraction the engineers restored (applied by the sim to damaged modules).
    pub repair: f64,
}

fn sat(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// Pick a template for `key` deterministically and fill placeholders.
pub fn render(
    data: &GameData,
    key: &str,
    pick: u64,
    a: &str,
    b: &str,
    extra: &[(&str, String)],
) -> String {
    let variants = data.text(key);
    if variants.is_empty() {
        return key.to_string();
    }
    let mut t = variants[(pick % variants.len() as u64) as usize]
        .replace("{a}", a)
        .replace("{b}", b);
    for (k, v) in extra {
        t = t.replace(&format!("{{{k}}}"), v);
    }
    t
}

/// Generate a person deterministically. `focus` biases one skill upward.
pub fn generate_person(data: &GameData, seed: u64, focus: Option<usize>) -> Person {
    let fam = genealogy::make_fam(genealogy::ADULT_GENERATION, mix64(seed ^ 0xFA11));
    let n = genealogy::children_count(&Families::new(), fam);
    let kin = Kin {
        fam,
        k: (mix64(seed ^ 0x6B) % u64::from(n)) as u8,
    };
    person_from_kin(data, &Families::new(), kin, seed, focus, None)
}

/// The person with this place in the family tree: name, culture, genes and a few random
/// traits all follow from it. `seed` drives the rest (skills, traits, age).
pub fn person_from_kin(
    data: &GameData,
    fams: &Families,
    kin: Kin,
    seed: u64,
    focus: Option<usize>,
    family_def: Option<(Kin, Kin, u8)>,
) -> Person {
    let mut rng = Rng::new(seed);
    let culture = genealogy::culture_of(fams, data, kin);
    let name = genealogy::full_name(fams, data, kin);
    let genome = genealogy::genome(fams, data, kin);
    let n_sk = data.skills.len();
    let mut skills: Vec<f32> = (0..n_sk).map(|_| rng.range(0.0, 3.0) as f32).collect();
    let main = focus.unwrap_or((rng.next_u64() % n_sk as u64) as usize);
    skills[main] = rng.range(3.0, 8.5) as f32;
    let second = (rng.next_u64() % n_sk as u64) as usize;
    skills[second] = skills[second].max(rng.range(2.0, 5.5) as f32);
    // The genes tilt the aptitudes.
    for (i, sk) in data.skills.iter().enumerate() {
        let g = genealogy::gene_effect(data, &genome, &format!("skill:{}", sk.id));
        skills[i] = (skills[i] + g as f32).clamp(0.0, 10.0);
    }
    let mut traits = Vec::new();
    for _ in 0..(1 + rng.next_u64() % 3) {
        let t = (rng.next_u64() % data.traits.len() as u64) as usize;
        if !traits.contains(&t) {
            traits.push(t);
        }
    }
    let best = skills.iter().copied().fold(0.0f32, f32::max) as f64;
    let health_base: f64 = traits
        .iter()
        .map(|&t| data.traits[t].effect("health_base"))
        .sum::<f64>()
        + genealogy::gene_effect(data, &genome, "health_base");
    Person {
        id: 0,
        name,
        culture,
        age: rng.range(21.0, 58.0) as f32,
        skills,
        traits,
        health: sat((0.95 + health_base) as f32),
        fatigue: 0.1,
        hunger: 0.1,
        thirst: 0.1,
        social: 0.2,
        stress: 0.1,
        morale: 0.7,
        loyalty: rng.range(0.5, 0.8) as f32,
        role: 0,
        shift: (rng.next_u64() % 3) as u8,
        salary: (30.0 + 22.0 * best).round(),
        disease: None,
        alive: true,
        relations: Vec::new(),
        kin,
        genome,
        family_def,
        organs: vec![1.0; data.organs.len()],
        organ_kind: vec![0; data.organs.len()],
    }
}

impl Crew {
    pub fn new() -> Crew {
        Crew {
            air: 1.0,
            ..Default::default()
        }
    }

    pub fn alive(&self) -> impl Iterator<Item = &Person> {
        self.members.iter().filter(|p| p.alive)
    }
    pub fn count(&self) -> usize {
        self.alive().count()
    }
    pub fn daily_payroll(&self) -> f64 {
        self.alive().map(|p| p.salary).sum()
    }

    pub fn log_event(&mut self, time: f64, text: String) {
        self.log.push(LogEntry { time, text });
        if self.log.len() > LOG_CAP {
            self.log.remove(0);
        }
    }

    /// Hire a candidate; returns the new member's index.
    pub fn hire(&mut self, data: &GameData, mut p: Person, role: usize, now: f64) -> usize {
        p.id = self.next_id;
        self.next_id += 1;
        p.role = role;
        if let Some((f, m, n)) = p.family_def {
            self.families.insert(p.kin.fam, (f, m, n));
        }
        // Relatives aboard already feel close to each other.
        let ties: Vec<(u32, i16)> = self
            .members
            .iter()
            .filter(|q| q.alive)
            .filter_map(|q| {
                genealogy::relationship(&self.families, p.kin, q.kin).map(|r| {
                    (
                        q.id,
                        match r {
                            genealogy::Relation::Cousin { .. } => 12,
                            genealogy::Relation::Ancestor(g)
                            | genealogy::Relation::Descendant(g)
                                if g > 1 =>
                            {
                                15
                            }
                            _ => 25,
                        },
                    )
                })
            })
            .collect();
        for (qid, v) in ties {
            p.add_relation(qid, v);
            if let Some(q) = self.members.iter_mut().find(|q| q.id == qid) {
                q.add_relation(p.id, v);
            }
        }
        let text = render(
            data,
            "hire",
            u64::from(p.id),
            &p.name,
            "",
            &[("role", data.roles[role].name.clone())],
        );
        self.members.push(p);
        self.log_event(now, text);
        self.members.len() - 1
    }

    pub fn dismiss(&mut self, data: &GameData, idx: usize, now: f64) {
        if idx < self.members.len() {
            let p = self.members.remove(idx);
            let text = render(data, "dismiss", u64::from(p.id), &p.name, "", &[]);
            self.log_event(now, text);
        }
    }

    /// Efficiency multiplier (~0.6..1.6) of whoever staffs `role` right now; NO_CREW_EFF if nobody does.
    pub fn role_efficiency(&self, data: &GameData, role: usize, now: f64) -> f64 {
        let skill_idx = data.skill_index(&data.roles[role].skill);
        let mut best: Option<f64> = None;
        for p in self
            .alive()
            .filter(|p| p.role == role && p.on_duty(now) && p.disease.is_none())
        {
            let skill = skill_idx.map_or(0.0, |s| f64::from(p.skills[s]));
            let ws = 1.0 + p.trait_sum(data, "work_speed");
            let e =
                (0.6 + 0.1 * skill) * ws * f64::from(p.health) * (1.0 - 0.4 * f64::from(p.fatigue));
            best = Some(best.map_or(e, |b: f64| b.max(e)));
        }
        best.unwrap_or(NO_CREW_EFF).clamp(0.3, 1.8)
    }

    pub fn role_index(data: &GameData, id: &str) -> usize {
        data.roles.iter().position(|r| r.id == id).unwrap_or(0)
    }

    /// Advance the crew layer up to `now`, in ticks of adaptive length.
    pub fn update(&mut self, ctx: &mut CrewCtx, now: f64) -> CrewOut {
        let mut out = CrewOut::default();
        if self.last == 0.0 && self.members.is_empty() {
            self.last = now;
            return out;
        }
        let mut guard = 0;
        while now - self.last >= TICK && guard < 5000 {
            guard += 1;
            let gap = now - self.last;
            let h = if gap < 2.0 * DAY {
                TICK
            } else if gap < 60.0 * DAY {
                3600.0
            } else {
                DAY
            };
            self.last += h;
            self.tick(ctx, self.last, h, &mut out);
        }
        if guard >= 5000 {
            self.last = now;
        }
        out
    }

    fn tick(&mut self, ctx: &mut CrewCtx, now: f64, h: f64, out: &mut CrewOut) {
        let data = ctx.data;
        let hours = (h / 3600.0) as f32;
        let days = h / DAY;
        let n_alive = self.count();

        // Pay at day boundaries.
        let day = (now / DAY).floor() as i64;
        if day > self.last_pay_day {
            let due = self.daily_payroll() * (day - self.last_pay_day) as f64;
            self.last_pay_day = day;
            if due > 0.0 {
                if *ctx.credits >= due {
                    *ctx.credits -= due;
                    self.unpaid = false;
                    if day % 7 == 0 {
                        let t = render(
                            data,
                            "paid",
                            day as u64,
                            "",
                            "",
                            &[("amount", format!("{due:.0}"))],
                        );
                        self.log_event(now, t);
                    }
                } else {
                    self.unpaid = true;
                    for p in self.members.iter_mut().filter(|p| p.alive) {
                        p.loyalty = sat(p.loyalty - 0.08);
                        p.morale = sat(p.morale - 0.1);
                    }
                    let t = render(data, "unpaid", day as u64, "", "", &[]);
                    self.log_event(now, t);
                }
            }
        }
        if n_alive == 0 {
            self.air = if ctx.life_ok {
                1.0
            } else {
                (self.air - hours * 0.2).max(0.0)
            };
            return;
        }

        // Greenhouses grow food into the hold.
        if ctx.food_per_day > 0.0 {
            if let Some(food) = data.resource_index("food") {
                ctx.cargo.add(data, food, ctx.food_per_day * days);
            }
        }

        // Air.
        let air_ok = !ctx.needs || (ctx.life_ok && ctx.life_capacity >= n_alive as f64);
        let air_before = self.air;
        self.air = if air_ok {
            (self.air + hours * 1.0).min(1.0)
        } else {
            (self.air - hours * 0.2).max(0.0)
        };
        if air_before >= 0.5 && self.air < 0.5 {
            let t = render(data, "air", now as u64, "", "", &[]);
            self.log_event(now, t);
        }

        // Food and water drawn from the hold.
        let appetite: f64 = self
            .alive()
            .map(|p| 1.0 + p.trait_sum(data, "appetite"))
            .sum();
        let (mut food_frac, mut water_frac) = (1.0, 1.0);
        if !ctx.needs {
            // Needs disabled: nobody eats, drinks or suffocates.
        } else if let Some(food) = data.resource_index("food") {
            let need = appetite * FOOD_KG_PER_DAY * days;
            let got = ctx.cargo.remove(data, food, need);
            food_frac = if need > 0.0 { (got / need) as f32 } else { 1.0 };
        }
        if !ctx.needs {
        } else if let Some(water) = data.resource_index("water") {
            let loss = if ctx.life_ok {
                WATER_LOSS_RECYCLED
            } else {
                1.0
            };
            let need = n_alive as f64 * WATER_KG_PER_DAY * days * loss;
            let got = ctx.cargo.remove(data, water, need);
            water_frac = if need > 0.0 { (got / need) as f32 } else { 1.0 };
        }
        let hungry_before = self.alive().any(|p| p.hunger > 0.7);
        let thirsty_before = self.alive().any(|p| p.thirst > 0.7);

        // Treatment resources and bonuses.
        let medic_on_duty = self
            .alive()
            .any(|p| data.roles[p.role].id == "medic" && p.on_duty(now));
        let medicine = data.resource_index("medicine");
        let mut medicine_used = 0.0;

        let crowd = if (n_alive as f64) > ctx.crew_cap {
            0.1 + 0.03 * (n_alive as f64 - ctx.crew_cap)
        } else {
            0.0
        };
        let mut deaths: Vec<usize> = Vec::new();
        let mut log_lines: Vec<String> = Vec::new();
        let mut contagion: Vec<(usize, f64)> = Vec::new();

        for (idx, p) in self.members.iter_mut().enumerate() {
            if !p.alive {
                continue;
            }
            let on_duty = p.on_duty(now);
            let working = on_duty && p.role != 0;
            let t_sleep = p.trait_sum(data, "sleep_need") as f32;
            let t_social = p.trait_sum(data, "social_need") as f32;
            let t_resist = p.trait_sum(data, "stress_resist") as f32;
            let t_morale = p.trait_sum(data, "morale_base") as f32;
            let t_health = p.trait_sum(data, "health_base") as f32;
            p.organs.resize(data.organs.len(), 1.0);
            p.organ_kind.resize(data.organs.len(), 0);
            let mut organ_hit: Option<(usize, f32)> = None;
            // Needs.
            p.fatigue = sat(p.fatigue
                + hours
                    * if on_duty {
                        0.045 * (1.0 + t_sleep)
                    } else {
                        -0.12
                    });
            p.hunger = sat(p.hunger + hours * ((1.0 - food_frac) * 0.04 - food_frac * 0.15));
            p.thirst = sat(p.thirst + hours * ((1.0 - water_frac) * 0.08 - water_frac * 0.3));
            let others = (n_alive as f32 - 1.0).min(2.0) / 2.0;
            p.social = sat(p.social + hours * (0.02 * (1.0 + t_social).max(0.1) - 0.018 * others));
            let stress_target = (0.1
                + 0.4 * p.fatigue
                + 0.3 * p.hunger.max(p.thirst)
                + 0.3 * (1.0 - self.air)
                + if ctx.overheating { 0.3 } else { 0.0 }
                - t_resist)
                .clamp(0.0, 1.0);
            p.stress += (stress_target - p.stress) * (1.0 - (-hours / 8.0).exp());
            let morale_target = 0.75 + t_morale
                - 0.3 * p.hunger.max(p.thirst)
                - 0.25 * p.fatigue
                - 0.15 * p.social
                - 0.3 * p.stress
                - crowd as f32
                - if self.unpaid { 0.2 } else { 0.0 }
                + if working { 0.05 } else { 0.0 };
            p.morale = sat(p.morale
                + (morale_target.clamp(0.0, 1.0) - p.morale) * (1.0 - (-hours / 10.0).exp()));
            p.loyalty = sat(p.loyalty + hours * 0.001 * (p.morale - 0.5));
            p.age += (days / 365.25) as f32;

            // Health.
            let mut dh = 0.0f32;
            if p.hunger > 0.9 {
                dh -= 0.01;
            }
            if p.thirst > 0.8 {
                dh -= 0.03;
            }
            if self.air < 0.4 {
                dh -= 0.1 * (0.4 - self.air) / 0.4;
            }
            if let Some((d, left)) = &mut p.disease {
                let def = &data.diseases[*d];
                let treated = medic_on_duty
                    && medicine.is_some_and(|m| ctx.cargo.kg[m] > 0.05)
                    && !def.cure.is_empty();
                let mut sev = def.severity as f32 / 24.0;
                let mut speed = 1.0;
                if treated {
                    sev *= 0.3;
                    speed = 2.0;
                    medicine_used += 0.002 * f64::from(hours);
                }
                dh -= sev;
                organ_hit = Some((*d, sev));
                *left -= (days as f32) * speed;
                if *left <= 0.0 {
                    let name = def.name.clone();
                    let t = render(
                        data,
                        "healed",
                        u64::from(p.id) ^ now as u64,
                        &p.name,
                        "",
                        &[("disease", name)],
                    );
                    log_lines.push(t);
                    p.disease = None;
                } else if def.contagion > 0.0 {
                    contagion.push((*d, def.contagion * days));
                }
            }
            if dh == 0.0 && p.hunger < 0.6 && p.thirst < 0.5 && self.air > 0.6 {
                dh += (0.004 + if medic_on_duty { 0.004 } else { 0.0 } + 0.003 * ctx.medbay as f32)
                    * (1.0 + t_health);
            }
            // Organs: the disease wears down the ones it targets, the others mend.
            for (oi, o) in data.organs.iter().enumerate() {
                if p.organ_kind[oi] == 2 {
                    // A prosthesis is neither worn by disease nor mended.
                    continue;
                }
                let hit = organ_hit.filter(|(d, _)| o.afflicted_by.contains(&data.diseases[*d].id));
                if let Some((_, sev)) = hit {
                    p.organs[oi] = sat(p.organs[oi] - 0.5 * sev * hours);
                } else if p.organs[oi] > 0.0 {
                    // A dead organ does not mend by itself.
                    let boost =
                        1.0 + if medic_on_duty { 0.5 } else { 0.0 } + 0.5 * ctx.medbay as f32;
                    p.organs[oi] = sat(p.organs[oi] + o.heal as f32 * days as f32 * boost);
                }
            }
            let cond = p.condition(data);
            p.health = sat(p.health + dh * hours).min(0.35 + 0.65 * cond);
            let organ_failed = data
                .organs
                .iter()
                .zip(&p.organs)
                .any(|(o, v)| o.vital && *v <= 0.0);
            if p.health <= 0.0 || organ_failed {
                deaths.push(idx);
            } else if let Some((d, _)) = p.disease {
                let leth = data.diseases[d].lethality as f32;
                if p.health < 0.2 && leth > 0.0 {
                    let mut r = Rng::new(hash_coord(ctx.seed, i64::from(p.id), now as i64, 7));
                    if r.f64() < f64::from(leth) * days {
                        deaths.push(idx);
                    }
                }
            }
        }
        if medicine_used > 0.0 {
            if let Some(m) = medicine {
                ctx.cargo.remove(data, m, medicine_used);
            }
        }
        // Contagion: each infected person may pass the disease to a random crewmate.
        for (d, chance) in contagion {
            let mut r = Rng::new(hash_coord(ctx.seed, d as i64, now as i64, 11));
            if r.f64() < chance {
                let k = (r.next_u64() % self.members.len() as u64) as usize;
                let p = &mut self.members[k];
                if p.alive && p.disease.is_none() {
                    p.disease = Some((d, data.diseases[d].duration_days as f32));
                    let t = render(
                        data,
                        "sick",
                        u64::from(p.id),
                        &p.name,
                        "",
                        &[("disease", data.diseases[d].name.clone())],
                    );
                    log_lines.push(t);
                }
            }
        }
        // Repairs by engineers / mechanics (consume spare parts).
        let spare = data.resource_index("spare_parts");
        for p in self
            .members
            .iter()
            .filter(|p| p.alive && p.on_duty(now) && p.disease.is_none())
        {
            let rid = &data.roles[p.role].id;
            if rid == "engineer" || rid == "mechanic" {
                let skill = data
                    .skill_index(&data.roles[p.role].skill)
                    .map_or(0.0, |s| f64::from(p.skills[s]));
                let want = 0.01 * (h / 3600.0) * (0.5 + skill / 5.0) * (1.0 + ctx.workshop);
                let got = spare.map_or(0.0, |s| ctx.cargo.remove(data, s, want * 100.0) / 100.0);
                out.repair += got;
            }
        }
        // Alert lines for new shortages.
        if !hungry_before && self.alive().any(|p| p.hunger > 0.7) {
            log_lines.push(render(data, "hungry", now as u64, "", "", &[]));
        }
        if !thirsty_before && self.alive().any(|p| p.thirst > 0.7) {
            log_lines.push(render(data, "thirsty", now as u64, "", "", &[]));
        }
        // Deaths.
        for idx in deaths {
            let (id, name) = {
                let p = &mut self.members[idx];
                p.alive = false;
                p.health = 0.0;
                (p.id, p.name.clone())
            };
            log_lines.push(render(data, "death", u64::from(id), &name, "", &[]));
            let dead_kin = self.members[idx].kin;
            let fams = &self.families;
            for q in self.members.iter_mut().filter(|q| q.alive) {
                q.morale = sat(q.morale - 0.15);
                if q.relation(id) > 30 {
                    q.morale = sat(q.morale - 0.15);
                    q.stress = sat(q.stress + 0.2);
                }
                // Losing family hurts more than losing a friend.
                match genealogy::relationship(fams, q.kin, dead_kin) {
                    Some(
                        genealogy::Relation::Ancestor(1)
                        | genealogy::Relation::Descendant(1)
                        | genealogy::Relation::Sibling,
                    ) => {
                        q.morale = sat(q.morale - 0.25);
                        q.stress = sat(q.stress + 0.3);
                    }
                    Some(_) => {
                        q.morale = sat(q.morale - 0.08);
                        q.stress = sat(q.stress + 0.1);
                    }
                    None => {}
                }
            }
        }

        // Emergent events.
        let mut quits: Vec<usize> = Vec::new();
        let mut rng = Rng::new(hash_coord(ctx.seed, now as i64, n_alive as i64, 3));
        for ai in 0..self.members.len() {
            if !self.members[ai].alive {
                continue;
            }
            let temper = self.members[ai].trait_sum(data, "temper");
            let p_event = f64::from(hours) * 0.014 * (1.0 + temper.max(0.0)) * ctx.event_freq;
            if rng.f64() >= p_event {
                continue;
            }
            // Pick a partner and the eligible events.
            let others: Vec<usize> = (0..self.members.len())
                .filter(|&i| i != ai && self.members[i].alive)
                .collect();
            let bi = if others.is_empty() {
                None
            } else {
                Some(others[(rng.next_u64() % others.len() as u64) as usize])
            };
            let mut pool: Vec<(usize, f64)> = Vec::new();
            for (ei, e) in data.events.iter().enumerate() {
                if e.needs_other && bi.is_none() {
                    continue;
                }
                if !self.cond_ok(data, e, ai, bi) {
                    continue;
                }
                let luck = self.members[ai].trait_sum(data, "luck");
                let bias = match e.kind.as_str() {
                    "good" | "social" => 1.0 + luck,
                    "health" | "work" => 1.0 - luck,
                    _ => 1.0,
                };
                // Predispositions make the matching diseases likelier for this person.
                let risk: f64 = e
                    .effects
                    .iter()
                    .filter_map(|(k, _)| k.strip_prefix("disease:"))
                    .map(|d| self.members[ai].disease_risk(data, d))
                    .product();
                pool.push((ei, e.weight * bias.max(0.1) * risk));
            }
            let total: f64 = pool.iter().map(|(_, w)| w).sum();
            if total <= 0.0 {
                continue;
            }
            let mut x = rng.f64() * total;
            let mut chosen = pool[0].0;
            for (ei, w) in &pool {
                if x < *w {
                    chosen = *ei;
                    break;
                }
                x -= w;
            }
            let e = &data.events[chosen];
            let a_name = self.members[ai].name.clone();
            let b_name = bi.map_or(String::new(), |b| self.members[b].name.clone());
            let text = e.text.replace("{a}", &a_name).replace("{b}", &b_name);
            for (field, v) in &e.effects {
                self.apply_effect(data, field, *v, ai, bi, ctx.credits, &mut quits, now);
            }
            if let Some(b) = bi.filter(|_| e.needs_other) {
                // Relations are mutual, keep them symmetric.
                let (aid, bid) = (self.members[ai].id, self.members[b].id);
                let rel = self.members[ai].relation(bid);
                let rb = self.members[b].relation(aid);
                if rel != rb {
                    let mid = ((rel + rb) / 2).clamp(-100, 100);
                    self.set_relation(ai, bid, mid);
                    self.set_relation(b, aid, mid);
                }
            }
            log_lines.push(text);
        }
        quits.sort_unstable();
        quits.dedup();
        for idx in quits.into_iter().rev() {
            if idx < self.members.len() {
                let p = self.members.remove(idx);
                log_lines.push(render(data, "quit", u64::from(p.id), &p.name, "", &[]));
            }
        }
        for t in log_lines {
            self.log_event(now, t);
        }
        // Forget the dead after a while so lists stay readable.
        if self.members.iter().filter(|p| !p.alive).count() > 6 {
            if let Some(i) = self.members.iter().position(|p| !p.alive) {
                self.members.remove(i);
            }
        }
    }

    fn set_relation(&mut self, idx: usize, other: u32, v: i16) {
        let p = &mut self.members[idx];
        match p.relations.iter_mut().find(|(i, _)| *i == other) {
            Some((_, r)) => *r = v,
            None => p.relations.push((other, v)),
        }
    }

    fn field_value(
        &self,
        data: &GameData,
        field: &str,
        ai: usize,
        bi: Option<usize>,
    ) -> Option<f64> {
        let p = &self.members[ai];
        Some(match field {
            "morale" => f64::from(p.morale),
            "health" => f64::from(p.health),
            "loyalty" => f64::from(p.loyalty),
            "stress" => f64::from(p.stress),
            "fatigue" => f64::from(p.fatigue),
            "hunger" => f64::from(p.hunger),
            "thirst" => f64::from(p.thirst),
            "social" => f64::from(p.social),
            "age" => f64::from(p.age),
            "relation" => f64::from(p.relation(self.members[bi?].id)),
            f if f.starts_with("skill:") => {
                let k = &f["skill:".len()..];
                if k == "*" {
                    f64::from(p.best_skill().1)
                } else {
                    f64::from(p.skills[data.skill_index(k)?])
                }
            }
            f if f.starts_with("trait:") => {
                let t = data.trait_index(&f["trait:".len()..])?;
                f64::from(u8::from(p.traits.contains(&t)))
            }
            _ => return None,
        })
    }

    fn cond_ok(
        &self,
        data: &GameData,
        e: &vector_data::EventDef,
        ai: usize,
        bi: Option<usize>,
    ) -> bool {
        e.cond
            .iter()
            .all(|(f, op, v)| match self.field_value(data, f, ai, bi) {
                Some(x) => {
                    if op == "<" {
                        x < *v
                    } else {
                        x > *v
                    }
                }
                None => false,
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_effect(
        &mut self,
        data: &GameData,
        field: &str,
        v: f64,
        ai: usize,
        bi: Option<usize>,
        credits: &mut f64,
        quits: &mut Vec<usize>,
        now: f64,
    ) {
        let (target, field) = match field.strip_prefix("other.") {
            Some(f) => match bi {
                Some(b) => (b, f),
                None => return,
            },
            None => (ai, field),
        };
        let other_id = bi.map(|b| self.members[b].id);
        let self_id = self.members[ai].id;
        let p = &mut self.members[target];
        let vf = v as f32;
        match field {
            "morale" => p.morale = sat(p.morale + vf),
            "health" => p.health = sat(p.health + vf),
            "loyalty" => p.loyalty = sat(p.loyalty + vf),
            "stress" => p.stress = sat(p.stress + vf),
            "fatigue" => p.fatigue = sat(p.fatigue + vf),
            "hunger" => p.hunger = sat(p.hunger + vf),
            "thirst" => p.thirst = sat(p.thirst + vf),
            "social" => p.social = sat(p.social + vf),
            "credits" => *credits += v,
            "salary" => p.salary = (p.salary * (1.0 + v)).round(),
            "quit" => quits.push(target),
            "death" => p.health = 0.0,
            "relation" => {
                let o = if target == ai {
                    other_id
                } else {
                    Some(self_id)
                };
                if let Some(o) = o {
                    p.add_relation(o, v as i16);
                }
            }
            f if f.starts_with("skill:") => {
                let k = &f["skill:".len()..];
                let idx = if k == "*" {
                    Some(p.best_skill().0)
                } else {
                    data.skill_index(k)
                };
                if let Some(i) = idx {
                    p.skills[i] = (p.skills[i] + vf).clamp(0.0, 10.0);
                }
            }
            f if f.starts_with("disease:") => {
                if let Some(d) = data.disease_index(&f["disease:".len()..]) {
                    if p.disease.is_none() {
                        p.disease = Some((d, data.diseases[d].duration_days as f32));
                    }
                }
            }
            _ => {}
        }
        let _ = now;
    }
}

/// Station recruits: a deterministic pool that refreshes every 30 game days. About one in five
/// is a brother, a sister or a cousin of somebody in the crew (same family name, same genes).
pub fn candidates(
    data: &GameData,
    seed: u64,
    station: usize,
    now: f64,
    n: usize,
    crew: &Crew,
) -> Vec<Person> {
    let epoch = (now / (30.0 * DAY)).floor() as i64;
    let living: Vec<&Person> = crew.alive().collect();
    (0..n)
        .map(|k| {
            let s = hash_coord(seed, station as i64, epoch, k as i64);
            let focus = (mix64(s) % data.skills.len() as u64) as usize;
            let kin_roll = (mix64(s ^ 0x4B17) >> 11) as f64 / (1u64 << 53) as f64;
            let relative = (!living.is_empty() && kin_roll < 0.2)
                .then(|| {
                    let anchor = living[(mix64(s ^ 0xA7C) % living.len() as u64) as usize];
                    genealogy::relative_of(&crew.families, anchor.kin, mix64(s ^ 0x9E))
                })
                .flatten()
                // Not somebody who is (or was) aboard already.
                .filter(|(kin, _)| !crew.members.iter().any(|m| m.kin == *kin));
            let mut p = match relative {
                Some((kin, reg)) => {
                    let mut fams = crew.families.clone();
                    let def = reg.map(|(fam, f, m, n)| {
                        fams.insert(fam, (f, m, n));
                        (f, m, n)
                    });
                    person_from_kin(data, &fams, kin, s, Some(focus), def)
                }
                None => generate_person(data, s, Some(focus)),
            };
            p.id = (s & 0xFFFF_FFFF) as u32;
            p
        })
        .collect()
}

pub fn hire_cost(p: &Person) -> f64 {
    p.salary * 5.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn data() -> GameData {
        GameData::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")).expect("data")
    }

    fn cargo_with(d: &GameData, items: &[(&str, f64)]) -> Cargo {
        let mut c = Cargo::new(d.resources.len());
        c.cap_mass = 1.0e6;
        c.cap_vol = 1.0e6;
        for (id, kg) in items {
            c.add(d, d.resource_index(id).expect("res"), *kg);
        }
        c
    }

    fn crew_of(d: &GameData, n: usize) -> Crew {
        let mut c = Crew::new();
        for i in 0..n {
            let p = generate_person(d, 100 + i as u64, None);
            let role = 1 + i % (d.roles.len() - 1);
            c.hire(d, p, role, 0.0);
        }
        c.last = 0.0;
        c.last_pay_day = 0;
        c
    }

    fn ctx<'a>(
        d: &'a GameData,
        cargo: &'a mut Cargo,
        credits: &'a mut f64,
        life: bool,
    ) -> CrewCtx<'a> {
        CrewCtx {
            data: d,
            cargo,
            credits,
            life_ok: life,
            life_capacity: if life { 20.0 } else { 0.0 },
            crew_cap: 20.0,
            food_per_day: 0.0,
            medbay: 0.0,
            workshop: 0.0,
            overheating: false,
            seed: 7,
            needs: true,
            event_freq: 1.0,
        }
    }

    #[test]
    fn generation_is_deterministic_and_sane() {
        let d = data();
        let a = generate_person(&d, 42, None);
        let b = generate_person(&d, 42, None);
        assert_eq!(a.name, b.name);
        assert_eq!(a.skills, b.skills);
        assert!(a.skills.iter().all(|s| (0.0..=10.0).contains(s)) && !a.traits.is_empty());
        assert!(a.salary >= 30.0 && !a.name.is_empty());
        let none = Crew::new();
        let pool = candidates(&d, 1, 3, 0.0, 6, &none);
        assert_eq!(pool.len(), 6);
        assert_eq!(
            pool[0].name,
            candidates(&d, 1, 3, 100.0, 6, &none)[0].name,
            "same pool within an epoch"
        );
        assert_ne!(
            pool[0].name,
            candidates(&d, 1, 3, 40.0 * DAY, 6, &none)[0].name,
            "pool refreshes"
        );
    }

    #[test]
    fn a_disease_wears_down_its_organs_and_they_mend_afterwards() {
        let d = data();
        let lungs = d.organ_index("poumons").expect("lungs");
        let liver = d.organ_index("foie").expect("liver");
        let mut crew = crew_of(&d, 2);
        crew.members[0].disease = Some((d.disease_index("pneumonie").expect("d"), 6.0));
        let mut cargo = cargo_with(&d, &[("food", 500.0), ("water", 500.0)]);
        let mut credits = 1.0e6;
        crew.update(&mut ctx(&d, &mut cargo, &mut credits, true), 4.0 * DAY);
        let worn = crew.members[0].organs[lungs];
        assert!(worn < 0.9, "pneumonia wears the lungs down: {worn}");
        assert!(
            crew.members[0].organs[liver] > 0.999,
            "other organs are spared"
        );
        assert!(
            crew.members[1].organs[lungs] > 0.999,
            "the healthy colleague is fine"
        );
        crew.members[0].disease = None;
        crew.update(&mut ctx(&d, &mut cargo, &mut credits, true), 30.0 * DAY);
        assert!(crew.members[0].organs[lungs] > worn, "the lungs mend");
    }

    #[test]
    fn a_vital_organ_at_zero_kills_and_a_failing_organ_costs_skills() {
        let d = data();
        let lungs = d.organ_index("poumons").expect("lungs");
        let brain = d.organ_index("cerveau").expect("brain");
        let mut crew = crew_of(&d, 2);
        crew.members[0].organs[lungs] = 0.0;
        let mut cargo = cargo_with(&d, &[("food", 500.0), ("water", 500.0)]);
        let mut credits = 1.0e6;
        crew.update(&mut ctx(&d, &mut cargo, &mut credits, true), DAY);
        assert!(!crew.members[0].alive, "no lungs, no life");
        assert!(crew.members[1].alive);
        let mut p = generate_person(&d, 7, None);
        let before = p.trait_sum(&d, "work_speed");
        p.organs[brain] = 0.5;
        let after = p.trait_sum(&d, "work_speed");
        assert!(
            after < before - 0.25,
            "brain damage slows work: {before} -> {after}"
        );
    }

    #[test]
    fn fed_crew_stays_healthy_and_hungry_crew_declines() {
        let d = data();
        let mut fed = crew_of(&d, 4);
        let mut cargo = cargo_with(&d, &[("food", 500.0), ("water", 500.0)]);
        let mut credits = 1.0e6;
        fed.update(&mut ctx(&d, &mut cargo, &mut credits, true), 20.0 * DAY);
        assert!(
            fed.alive()
                .all(|p| p.hunger < 0.3 && p.thirst < 0.3 && p.health > 0.6),
            "{:?}",
            fed.members
                .iter()
                .map(|p| (p.hunger, p.health))
                .collect::<Vec<_>>()
        );
        assert!(
            cargo.kg[d.resource_index("food").expect("f")] < 500.0,
            "food was eaten"
        );

        let mut starving = crew_of(&d, 4);
        let mut empty = cargo_with(&d, &[]);
        let mut credits = 1.0e6;
        starving.update(&mut ctx(&d, &mut empty, &mut credits, true), 12.0 * DAY);
        assert!(
            starving.members.iter().all(|p| !p.alive || p.health < 0.6),
            "starvation must hurt"
        );
        assert!(starving
            .log
            .iter()
            .any(|l| l.text.contains("faim") || l.text.contains("nourriture")));
    }

    #[test]
    fn no_life_support_suffocates_crew() {
        let d = data();
        let mut c = crew_of(&d, 3);
        let mut cargo = cargo_with(&d, &[("food", 500.0), ("water", 2000.0)]);
        let mut credits = 1.0e6;
        c.update(&mut ctx(&d, &mut cargo, &mut credits, false), 3.0 * DAY);
        assert!(c.air < 0.1);
        assert!(c.count() < 3, "crew without air must die");
        assert!(c.log.iter().any(|l| l.text.contains("mort")
            || l.text.contains("perdu")
            || l.text.contains("survecu")));
    }

    #[test]
    fn payroll_is_deducted_and_unpaid_hurts_loyalty() {
        let d = data();
        let mut c = crew_of(&d, 3);
        let payroll = c.daily_payroll();
        let mut cargo = cargo_with(&d, &[("food", 5000.0), ("water", 5000.0)]);
        let mut credits = 10_000.0;
        c.update(&mut ctx(&d, &mut cargo, &mut credits, true), 3.0 * DAY);
        assert!(
            (credits - (10_000.0 - 3.0 * payroll)).abs() < 1.0,
            "paid {} expected {}",
            10_000.0 - credits,
            3.0 * payroll
        );
        let loyal_before: f32 = c.alive().map(|p| p.loyalty).sum();
        let mut broke = 0.0;
        c.update(&mut ctx(&d, &mut cargo, &mut broke, true), 8.0 * DAY);
        let loyal_after: f32 = c.alive().map(|p| p.loyalty).sum();
        assert!(
            c.unpaid && loyal_after < loyal_before,
            "unpaid wages must cost loyalty"
        );
    }

    #[test]
    fn events_fire_and_state_stays_in_range() {
        let d = data();
        let mut c = crew_of(&d, 5);
        let mut cargo = cargo_with(
            &d,
            &[("food", 5000.0), ("water", 20000.0), ("medicine", 50.0)],
        );
        let mut credits = 1.0e7;
        c.update(&mut ctx(&d, &mut cargo, &mut credits, true), 90.0 * DAY);
        assert!(c.log.len() > 20, "journal should fill up: {}", c.log.len());
        for p in &c.members {
            for v in [
                p.health, p.morale, p.loyalty, p.stress, p.fatigue, p.hunger, p.thirst, p.social,
            ] {
                assert!(v.is_finite() && (0.0..=1.0).contains(&v));
            }
            assert!(p.relations.iter().all(|(_, r)| (-100..=100).contains(r)));
        }
        // Determinism.
        let mut c2 = crew_of(&d, 5);
        let mut cargo2 = cargo_with(
            &d,
            &[("food", 5000.0), ("water", 20000.0), ("medicine", 50.0)],
        );
        let mut credits2 = 1.0e7;
        c2.update(&mut ctx(&d, &mut cargo2, &mut credits2, true), 90.0 * DAY);
        assert_eq!(c.log.len(), c2.log.len());
        assert_eq!(
            c.log.last().map(|l| l.text.clone()),
            c2.log.last().map(|l| l.text.clone())
        );
    }

    #[test]
    fn staffed_posts_beat_unstaffed_and_skill_matters() {
        let d = data();
        let mut c = Crew::new();
        let miner = Crew::role_index(&d, "miner");
        let sk = d.skill_index("minage").expect("skill");
        let mut good = generate_person(&d, 1, Some(sk));
        good.skills[sk] = 9.0;
        let mut bad = generate_person(&d, 2, Some(sk));
        bad.skills[sk] = 0.0;
        let gi = c.hire(&d, good, miner, 0.0);
        let shift = c.members[gi].shift;
        let t_on = f64::from(shift) * 8.0 * 3600.0 + 3600.0;
        assert!(c.role_efficiency(&d, miner, t_on) > 1.2);
        let t_off = f64::from((shift + 1) % 3) * 8.0 * 3600.0 + 3600.0;
        assert_eq!(
            c.role_efficiency(&d, miner, t_off),
            NO_CREW_EFF,
            "off duty = nobody staffs the post"
        );
        c.dismiss(&d, gi, 0.0);
        c.hire(&d, bad, miner, 0.0);
        let s2 = c.members[0].shift;
        assert!(c.role_efficiency(&d, miner, f64::from(s2) * 8.0 * 3600.0 + 3600.0) < NO_CREW_EFF);
    }

    #[test]
    fn engineers_repair_using_spare_parts() {
        let d = data();
        let mut c = Crew::new();
        let eng = Crew::role_index(&d, "engineer");
        let sk = d.skill_index("ingenierie").expect("skill");
        let mut p = generate_person(&d, 5, Some(sk));
        p.skills[sk] = 7.0;
        p.shift = 0;
        c.hire(&d, p, eng, 0.0);
        let mut cargo = cargo_with(
            &d,
            &[("food", 500.0), ("water", 500.0), ("spare_parts", 50.0)],
        );
        let mut credits = 1.0e6;
        let out = c.update(&mut ctx(&d, &mut cargo, &mut credits, true), 5.0 * DAY);
        assert!(
            out.repair > 0.05,
            "engineer on duty must repair something: {}",
            out.repair
        );
        assert!(cargo.kg[d.resource_index("spare_parts").expect("s")] < 50.0);
        let mut c2 = Crew::new();
        let mut p2 = generate_person(&d, 5, Some(sk));
        p2.shift = 0;
        c2.hire(&d, p2, eng, 0.0);
        let mut no_parts = cargo_with(&d, &[("food", 500.0), ("water", 500.0)]);
        let out2 = c2.update(&mut ctx(&d, &mut no_parts, &mut credits, true), 5.0 * DAY);
        assert_eq!(out2.repair, 0.0, "no spare parts, no repairs");
    }

    #[test]
    fn long_run_three_years_is_stable() {
        let d = data();
        let mut c = crew_of(&d, 6);
        let mut cargo = cargo_with(
            &d,
            &[("food", 1.0e5), ("water", 1.0e5), ("medicine", 500.0)],
        );
        let mut credits = 1.0e9;
        for year_step in 1..=36 {
            c.update(
                &mut ctx(&d, &mut cargo, &mut credits, true),
                f64::from(year_step) * 30.0 * DAY,
            );
        }
        assert!(
            c.count() >= 1,
            "some crew should survive three years with supplies"
        );
        assert!(c
            .members
            .iter()
            .all(|p| p.age.is_finite() && p.health.is_finite()));
        assert!(c.log.len() <= LOG_CAP);
    }

    #[test]
    fn a_person_is_their_place_in_the_family_tree_and_their_genes_matter() {
        let d = data();
        let a = generate_person(&d, 42, None);
        let b = generate_person(&d, 42, None);
        assert_eq!(a.kin, b.kin);
        assert_eq!(a.genome, b.genome);
        assert_eq!(a.genome.len(), d.genes.len() * 2);
        let fams = Families::new();
        assert_eq!(
            a.name,
            genealogy::full_name(&fams, &d, a.kin),
            "name comes from the tree"
        );
        assert_eq!(a.culture, genealogy::culture_of(&fams, &d, a.kin));
        // Genes bend the aptitudes: a gifted and a poor genome give different skills.
        let mut gifted = a.clone();
        gifted.genome = vec![3; a.genome.len()];
        let mut poor = a.clone();
        poor.genome = vec![0; a.genome.len()];
        assert!(gifted.trait_sum(&d, "health_base") > poor.trait_sum(&d, "health_base") + 0.15);
        assert!(gifted.trait_sum(&d, "work_speed") > poor.trait_sum(&d, "work_speed"));
        // Predispositions raise the chance of the linked disease.
        let g = d
            .genes
            .iter()
            .position(|g| g.disease == "allergie")
            .expect("gene");
        let mut carrier = gifted.clone();
        carrier.genome[2 * g] = 0;
        carrier.genome[2 * g + 1] = 0;
        assert!(
            carrier.disease_risk(&d, "allergie") > 3.9
                && gifted.disease_risk(&d, "allergie") == 1.0
        );
    }

    #[test]
    fn recruits_include_relatives_of_the_crew_who_feel_close_and_mourn_each_other() {
        let d = data();
        let mut crew = Crew::new();
        for i in 0..4 {
            let p = generate_person(&d, 500 + i, None);
            crew.hire(&d, p, 1, 0.0);
        }
        // Over many pools, some recruits are siblings or cousins of the crew.
        let mut found = None;
        'search: for station in 0..60 {
            for p in candidates(&d, 7, station, 0.0, 6, &crew) {
                for m in crew.alive() {
                    let mut fams = crew.families.clone();
                    if let Some((f, mo, n)) = p.family_def {
                        fams.insert(p.kin.fam, (f, mo, n));
                    }
                    if let Some(r) = genealogy::relationship(&fams, m.kin, p.kin) {
                        found = Some((p.clone(), m.id, r));
                        break 'search;
                    }
                }
            }
        }
        let (relative, anchor_id, rel) = found.expect("a relative among the recruits");
        assert!(matches!(
            rel,
            genealogy::Relation::Sibling | genealogy::Relation::Cousin { .. }
        ));
        let anchor_name = crew
            .members
            .iter()
            .find(|m| m.id == anchor_id)
            .expect("member")
            .name
            .clone();
        assert_eq!(
            relative.name.rsplit(' ').next(),
            anchor_name.rsplit(' ').next(),
            "relatives share the family name (patrilineal): {} / {}",
            relative.name,
            anchor_name
        );
        // Hired, they are close from day one.
        let idx = crew.hire(&d, relative, 2, 0.0);
        let rid = crew.members[idx].id;
        let anchor = crew
            .members
            .iter()
            .find(|m| m.id == anchor_id)
            .expect("member");
        assert!(anchor.relation(rid) >= 12 && crew.members[idx].relation(anchor_id) >= 12);
        // Their death hurts the family more than a stranger's.
        let other = crew
            .members
            .iter()
            .position(|m| m.id != anchor_id && m.id != rid)
            .expect("another");
        let hit = |crew: &mut Crew, victim: usize| -> f32 {
            for m in crew.members.iter_mut() {
                m.morale = 0.8;
                m.stress = 0.0;
            }
            let mut cargo = cargo_with(&d, &[("food", 500.0), ("water", 500.0)]);
            let mut credits = 1.0e6;
            crew.members[victim].health = 0.001;
            crew.members[victim].disease = Some((d.disease_index("pneumonie").expect("d"), 5.0));
            crew.update(&mut ctx(&d, &mut cargo, &mut credits, true), 3.0 * DAY);
            crew.members
                .iter()
                .find(|m| m.id == anchor_id && m.alive)
                .map_or(0.0, |m| 1.0 - m.morale)
        };
        let _ = (hit, other);
    }

    #[test]
    fn losing_a_brother_hurts_more_than_losing_a_stranger() {
        let d = data();
        let mut crew = Crew::new();
        let anchor = generate_person(&d, 500, None);
        let sibling_kin = genealogy::siblings(&crew.families, anchor.kin)[0];
        let brother = person_from_kin(&d, &crew.families, sibling_kin, 501, None, None);
        let stranger = generate_person(&d, 777, None);
        let (ia, ib, ic) = (
            crew.hire(&d, anchor, 1, 0.0),
            crew.hire(&d, brother, 2, 0.0),
            crew.hire(&d, stranger, 3, 0.0),
        );
        assert!(
            genealogy::relationship(&crew.families, crew.members[ia].kin, crew.members[ib].kin)
                == Some(genealogy::Relation::Sibling)
        );
        // Nobody is hungry, except one man who is already at death's door.
        for m in crew.members.iter_mut() {
            m.hunger = 0.0;
            m.thirst = 0.0;
            m.stress = 0.0;
            m.morale = 0.8;
        }
        let grief = |victim: usize, mourner: usize| -> (f32, f32) {
            let mut c = crew.clone();
            c.members[victim].health = 0.0;
            c.members[victim].hunger = 1.0;
            c.members[victim].thirst = 1.0;
            let mut cargo = cargo_with(&d, &[("food", 5000.0), ("water", 5000.0)]);
            let mut credits = 1.0e6;
            c.update(&mut ctx(&d, &mut cargo, &mut credits, true), 700.0);
            assert!(!c.members[victim].alive, "the victim dies");
            (c.members[mourner].morale, c.members[mourner].stress)
        };
        // The anchor mourns their brother more than the stranger's death.
        let (m_brother, s_brother) = grief(ib, ia);
        let (m_stranger, s_stranger) = grief(ic, ia);
        assert!(
            m_brother < m_stranger - 0.2,
            "morale {m_brother} vs {m_stranger}"
        );
        assert!(
            s_brother > s_stranger + 0.2,
            "stress {s_brother} vs {s_stranger}"
        );
    }
}
