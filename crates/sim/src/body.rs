//! The body of a person: localised wounds, blood, oxygen, core temperature, pain, trauma, drugs,
//! permanent disabilities, cryostasis. What hurts follows from the physics of the place (pressure,
//! air, temperature, gravity, acceleration, radiation); what heals follows from care (a medic's
//! skill, the infirmary, food, rest, drugs). The crew layer (`crew.rs`) feeds it every tick and
//! applies its result to organs, health, stress and needs; `treat.rs` holds the actions of the
//! player (first aid, drugs, surgery, resuscitation, cryostasis).
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng};
use vector_data::GameData;

pub const NORMAL_CORE: f32 = 37.0;
/// Blood volume (share of normal) under which a person bleeds to death.
pub const BLOOD_FATAL: f32 = 0.3;
/// Standard gravity, m/s^2.
pub const G0: f64 = 9.806_65;
/// Overdose threshold of the drug pressure.
pub const OVERDOSE: f32 = 1.6;

/// A localised injury on one part of the body.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wound {
    /// Index into `GameData::wounds` / `GameData::body_parts`.
    pub kind: u16,
    pub part: u8,
    /// 0..1, the wound closes at 0.
    pub sev: f32,
    pub peak: f32,
    /// Quality 0..1 of the dressing, splint or suture (first aid, surgery).
    pub dressed: f32,
    /// Infection load 0..1.
    pub infection: f32,
}

/// A drug in the blood.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrugState {
    pub drug: u16,
    pub level: f32,
    pub tolerance: f32,
    pub dependence: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Body {
    /// Blood volume, 1 = normal.
    pub blood: f32,
    /// Oxygenation of the brain, 1 = normal.
    pub oxygen: f32,
    /// Core temperature, Celsius.
    pub core_temp: f32,
    /// Felt pain 0..1 (after the painkillers).
    pub pain: f32,
    /// Psychological trauma 0..1.
    pub trauma: f32,
    /// Bone density 1 = normal (falls in low gravity).
    pub bone: f32,
    /// Recent radiation dose (Sv, mends slowly) and the dose of a lifetime.
    pub acute_dose: f32,
    pub total_dose: f32,
    /// Dose received since the last step (from the ship's radiation hazard), taken by the step.
    pub pending_dose: f32,
    pub wounds: Vec<Wound>,
    pub drugs: Vec<DrugState>,
    /// Permanent disability 0..1 per body part.
    pub disability: Vec<f32>,
    /// In cryostasis (see `cryo`): the body is cooled to the service temperature of its caisson.
    pub cryo: bool,
    /// Cryoprotectant loaded 0..1 (against ice), ice damage 0..1 (death at 1), hours of
    /// controlled rewarming left (0 = not waking), the date the sleep began, the date of the last
    /// thaw, the cryo sickness 0..1 that follows a waking, and the temperature (C) the caisson holds.
    pub cryo_cpa: f32,
    pub cryo_ice: f32,
    pub cryo_wake: f32,
    pub cryo_since: f64,
    pub cryo_last: f64,
    pub cryo_sick: f32,
    pub cryo_set: f32,
    /// The rewarming was an emergency one (cut short: riskier).
    pub cryo_urgent: bool,
    /// Date of the last resuscitation.
    pub revived_at: f64,
}

impl Default for Body {
    fn default() -> Body {
        Body {
            blood: 1.0,
            oxygen: 1.0,
            core_temp: NORMAL_CORE,
            pain: 0.0,
            trauma: 0.0,
            bone: 1.0,
            acute_dose: 0.0,
            total_dose: 0.0,
            pending_dose: 0.0,
            wounds: Vec::new(),
            drugs: Vec::new(),
            disability: Vec::new(),
            cryo: false,
            cryo_cpa: 0.0,
            cryo_ice: 0.0,
            cryo_wake: 0.0,
            cryo_since: 0.0,
            cryo_last: -1.0e18,
            cryo_sick: 0.0,
            cryo_set: -130.0,
            cryo_urgent: false,
            revived_at: -1.0e18,
        }
    }
}

/// What a person is exposed to where they are (the cabin, or the air of a world on foot).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Exposure {
    pub pressure_kpa: f64,
    pub o2_frac: f64,
    pub co2_kpa: f64,
    pub temp_k: f64,
    /// Gravity in g.
    pub gravity_g: f64,
    /// Sustained acceleration of the ship in g.
    pub accel_g: f64,
    /// Radiation, Sv per day.
    pub rad_sv_day: f64,
    /// Protection of the clothing against the cold and the heat, 0..1.
    pub insulation: f64,
}

impl Exposure {
    /// The air of a ship that works: one atmosphere, 21 % oxygen, a comfortable 21 C.
    pub const CABIN: Exposure = Exposure {
        pressure_kpa: 101.3,
        o2_frac: 0.21,
        co2_kpa: 0.04,
        temp_k: 294.0,
        gravity_g: 1.0,
        accel_g: 0.0,
        rad_sv_day: 0.0,
        insulation: 0.0,
    };
}

impl Default for Exposure {
    fn default() -> Exposure {
        Exposure::CABIN
    }
}

/// The state of the care around a person during a step.
#[derive(Clone, Copy, Debug, Default)]
pub struct Care {
    /// Medicine skill 0..10 of the best medic on duty (0 without).
    pub medic_skill: f64,
    /// Power of the infirmary.
    pub medbay: f64,
    pub hunger: f32,
    pub thirst: f32,
    pub fatigue: f32,
    pub age: f32,
    /// Integrity 0..1 of the liver (clears drugs).
    pub liver: f32,
}

/// What a step does to the rest of the person.
#[derive(Clone, Debug, Default)]
pub struct StepOut {
    /// (organ index, integrity lost).
    pub organ_damage: Vec<(usize, f32)>,
    /// Health lost during the step.
    pub health_loss: f32,
    /// Added to the stress target (pain, trauma, fear) and taken off it (calming drugs).
    pub stress_add: f32,
    /// Fatigue gained per hour (heavy gravity, drugs) or lost (stimulants, negative).
    pub fatigue_rate: f32,
    /// Multiplier of hunger: the cold burns food.
    pub hunger_mult: f32,
    pub radiation_sick: bool,
    /// Dose (Sv) absorbed during the step, after the protection of drugs.
    pub dose: f32,
    pub sepsis: bool,
    pub unconscious: bool,
    pub death: Option<&'static str>,
    pub notes: Vec<String>,
}

fn sat(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// Relaxation factor of a first order process of time constant `tau` hours over `hours`.
fn relax(hours: f32, tau: f32) -> f32 {
    1.0 - (-hours / tau).exp()
}

impl Body {
    pub fn new(data: &GameData) -> Body {
        Body {
            disability: vec![0.0; data.body_parts.len()],
            ..Body::default()
        }
    }

    fn ensure(&mut self, data: &GameData) {
        self.disability.resize(data.body_parts.len(), 0.0);
    }

    /// Sum of an effect of the drugs in the blood, weakened by tolerance.
    pub fn drug_effect(&self, data: &GameData, key: &str) -> f32 {
        self.drugs
            .iter()
            .map(|d| {
                d.level
                    * (1.0 - d.tolerance.min(0.9))
                    * data.drugs[d.drug as usize].effect(key) as f32
            })
            .sum()
    }

    /// Raw level times the effect, without tolerance (what the body must clear: toxic effects).
    fn drug_load(&self, data: &GameData, key: &str) -> f32 {
        self.drugs
            .iter()
            .map(|d| d.level * data.drugs[d.drug as usize].effect(key) as f32)
            .sum()
    }

    /// Strength of the drug pressure on the body: levels weighted by the interactions of the
    /// drugs present together, and by how badly the liver clears them.
    pub fn overdose_pressure(&self, data: &GameData, liver: f32) -> f32 {
        let mut total = 0.0;
        for d in &self.drugs {
            let def = &data.drugs[d.drug as usize];
            let mut w = 1.0f32;
            for (other, f) in &def.interactions {
                if let Some(o) = self
                    .drugs
                    .iter()
                    .find(|o| data.drugs[o.drug as usize].id == *other)
                {
                    w += *f as f32 * o.level;
                }
            }
            total += d.level * w;
        }
        total / (0.5 + 0.5 * liver.clamp(0.05, 1.0))
    }

    /// Add severity to a wound of this kind on this part (merging with the one already there).
    /// Returns the organ of the part that is hurt too, with the damage.
    pub fn add_wound(
        &mut self,
        data: &GameData,
        kind: &str,
        part: usize,
        sev: f32,
    ) -> Option<(usize, f32)> {
        self.ensure(data);
        let k = data.wound_index(kind)?;
        let sev = sev.clamp(0.0, 1.0);
        if sev <= 0.0 || part >= data.body_parts.len() {
            return None;
        }
        match self
            .wounds
            .iter_mut()
            .find(|w| w.kind as usize == k && w.part as usize == part)
        {
            Some(w) => {
                w.sev = 1.0 - (1.0 - w.sev) * (1.0 - sev);
                w.peak = w.peak.max(w.sev);
            }
            None => self.wounds.push(Wound {
                kind: k as u16,
                part: part as u8,
                sev,
                peak: sev,
                dressed: 0.0,
                infection: 0.0,
            }),
        }
        if sev > 0.3 {
            self.trauma = sat(self.trauma + 0.1 * sev);
        }
        // The organs under the wound take part of the blow.
        let organs = &data.body_parts[part].organs;
        if organs.is_empty() {
            return None;
        }
        let pick = (hash_coord(k as u64, part as i64, (sev * 1000.0) as i64, 91)
            % organs.len() as u64) as usize;
        let organ = data.organ_index(&organs[pick])?;
        let hurts_organ = data.wounds[k].internal || data.wounds[k].bleed >= 0.05;
        hurts_organ.then_some((organ, sev * 0.35))
    }

    /// A wound of a kind at a random place (the kind's parts, weighted by `hit`).
    pub fn strike(
        &mut self,
        data: &GameData,
        kind: &str,
        sev: f32,
        pick: u64,
    ) -> Option<(usize, f32)> {
        let k = data.wound_index(kind)?;
        let allowed = &data.wounds[k].parts;
        let parts: Vec<(usize, f64)> = data
            .body_parts
            .iter()
            .enumerate()
            .filter(|(_, p)| allowed.is_empty() || allowed.contains(&p.id))
            .map(|(i, p)| (i, p.hit))
            .collect();
        let total: f64 = parts.iter().map(|x| x.1).sum();
        if total <= 0.0 {
            return None;
        }
        let mut r = (pick % 10_000) as f64 / 10_000.0 * total;
        let mut chosen = parts[0].0;
        for (i, w) in &parts {
            if r < *w {
                chosen = *i;
                break;
            }
            r -= w;
        }
        self.add_wound(data, kind, chosen, sev)
    }

    /// A sudden loss of pressure from `from_kpa` to `to_kpa`: lungs and ears. A drop of less than
    /// a third is harmless; the injury grows with the ratio.
    pub fn rapid_decompression(
        &mut self,
        data: &GameData,
        from_kpa: f64,
        to_kpa: f64,
    ) -> Vec<(usize, f32)> {
        let ratio = (1.0 - to_kpa.max(0.0) / from_kpa.max(1.0)).clamp(0.0, 1.0);
        let sev = (((ratio - 0.3) / 0.7).clamp(0.0, 1.0)).powf(1.2) as f32;
        let mut hurt = Vec::new();
        if sev > 0.0 {
            for (name, k) in [("torse", 1.0), ("tete", 0.5)] {
                if let Some(p) = data.part_index(name) {
                    hurt.extend(self.add_wound(data, "barotraumatisme", p, sev * k));
                }
            }
        }
        hurt
    }

    /// The impact of a fall of `height` metres under `gravity` (m/s^2); `protection` 0..1 is the
    /// share of the energy absorbed by a suit. Energy per kilogram is g h.
    pub fn fall(
        &mut self,
        data: &GameData,
        height: f64,
        gravity: f64,
        protection: f64,
        pick: u64,
    ) -> Vec<(usize, f32)> {
        let energy = gravity * height * (1.0 - protection.clamp(0.0, 0.95));
        let sev = ((energy - G0 * 1.5) / (G0 * 8.0)).clamp(0.0, 1.5) as f32;
        let mut hurt = Vec::new();
        if sev <= 0.0 {
            return hurt;
        }
        let brittle = 1.0 + 2.0 * (1.0 - self.bone);
        hurt.extend(self.strike(data, "contusion", sev.min(1.0), pick));
        if sev * brittle > 0.35 {
            let legs = ["jambe_g", "jambe_d"][(pick >> 7) as usize % 2];
            if let Some(p) = data.part_index(legs) {
                hurt.extend(self.add_wound(data, "fracture_os", p, (sev * brittle * 0.8).min(1.0)));
            }
        }
        if sev > 0.8 {
            hurt.extend(self.strike(data, "hemorragie_interne", sev - 0.5, pick >> 3));
            hurt.extend(self.strike(data, "traumatisme_crane", sev - 0.6, pick >> 5));
        }
        hurt
    }

    /// Take a dose of a drug: the level rises, tolerance and dependence grow.
    pub fn take_drug(&mut self, data: &GameData, drug: usize) {
        let def = &data.drugs[drug];
        match self.drugs.iter_mut().find(|d| d.drug as usize == drug) {
            Some(d) => {
                d.level += 1.0;
                d.tolerance = sat(d.tolerance + def.tolerance as f32);
                d.dependence = sat(d.dependence + def.dependence as f32);
            }
            None => self.drugs.push(DrugState {
                drug: drug as u16,
                level: 1.0,
                tolerance: sat(def.tolerance as f32),
                dependence: sat(def.dependence as f32),
            }),
        }
        let blood = def.effect("blood_now") as f32;
        if blood > 0.0 {
            self.blood = (self.blood + blood).min(1.0);
        }
    }

    /// First aid: dress the wounds that need it, with the quality of the skill (0..10) of who
    /// does it. Internal wounds cannot be dressed. Returns the number of wounds treated.
    pub fn dress_wounds(&mut self, data: &GameData, skill: f64) -> usize {
        let q = (0.35 + 0.06 * skill).clamp(0.0, 0.95) as f32;
        let mut n = 0;
        for w in &mut self.wounds {
            let k = &data.wounds[w.kind as usize];
            if !k.internal && w.dressed < q {
                w.dressed = q;
                n += 1;
            }
        }
        n
    }

    /// The wounds a doctor can see: everything external; the internal ones with skill 3 or an
    /// infirmary (its scanner). The severity read is noisy without the infirmary.
    pub fn diagnose(&self, data: &GameData, skill: f64, medbay: f64) -> (Vec<(usize, f32)>, usize) {
        let mut seen = Vec::new();
        let mut hidden = 0;
        for (i, w) in self.wounds.iter().enumerate() {
            let k = &data.wounds[w.kind as usize];
            if k.internal && skill < 3.0 && medbay <= 0.0 {
                hidden += 1;
                continue;
            }
            let noise = if medbay > 0.0 {
                0.0
            } else {
                (0.3 - 0.025 * skill).max(0.03)
            };
            let r = (hash_coord(i as u64, w.kind as i64, (w.sev * 100.0) as i64, 17) % 2001) as f64
                / 1000.0
                - 1.0;
            seen.push((i, sat(w.sev + (r * noise) as f32)));
        }
        (seen, hidden)
    }

    /// What wounds and disabilities take off a trait key (organs are counted elsewhere).
    pub fn trait_effect(&self, data: &GameData, key: &str) -> f64 {
        let mut sum = 0.0;
        for (i, p) in data.body_parts.iter().enumerate() {
            let e: f64 = p
                .effects
                .iter()
                .filter(|(k, _)| k == key)
                .map(|(_, v)| *v)
                .sum();
            if e == 0.0 {
                continue;
            }
            let d = f64::from(self.disability.get(i).copied().unwrap_or(0.0));
            let hurt: f64 = self
                .wounds
                .iter()
                .filter(|w| w.part as usize == i)
                .map(|w| f64::from(w.sev) * 0.8)
                .sum();
            sum += e * (d + hurt.min(1.0));
        }
        if key == "work_speed" {
            sum -= 0.35 * f64::from(self.pain);
            if self.blood < 0.7 {
                sum -= 0.6 * f64::from(0.7 - self.blood);
            }
            if self.core_temp < 35.0 {
                sum -= 0.05 * f64::from(35.0 - self.core_temp);
            }
        }
        sum
    }

    pub fn unconscious(&self) -> bool {
        self.oxygen < 0.4 || self.blood < 0.45 || self.core_temp < 29.0
    }

    pub fn is_hurt(&self) -> bool {
        !self.wounds.is_empty()
            || self.blood < 0.95
            || self.core_temp < 35.5
            || self.core_temp > 38.5
            || self.acute_dose > 0.2
    }

    /// Worst permanent disability.
    pub fn disabled(&self) -> bool {
        self.disability.iter().any(|d| *d > 0.1)
    }

    /// Advance the body by `hours` in `env` with the care around it. `key` is a deterministic
    /// date-like number for the random draws.
    pub fn step(
        &mut self,
        data: &GameData,
        env: &Exposure,
        care: &Care,
        hours: f32,
        seed: u64,
        key: i64,
    ) -> StepOut {
        let mut out = StepOut {
            hunger_mult: 1.0,
            ..StepOut::default()
        };
        self.ensure(data);
        if self.cryo {
            // A sleeper is stepped by `sleep_step` (caisson, ice, dose); this is the bare fallback.
            self.oxygen = 1.0;
            return out;
        }
        let mut rng = Rng::new(hash_coord(seed, key, 0xB0D1, 71));
        let organ_of = |id: &str| data.organ_index(id);
        let organ_hit = |o: Option<usize>, dmg: f32, out: &mut StepOut| {
            if let (Some(o), true) = (o, dmg > 0.0) {
                out.organ_damage.push((o, dmg));
            }
        };

        // Breathing: partial pressure of oxygen, blackouts under acceleration, CO2, blood.
        let ppo2 = env.pressure_kpa * env.o2_frac;
        let mut target = (((ppo2 - 3.0) / 13.0).clamp(0.0, 1.0)) as f32;
        target *= 1.0 - ((env.accel_g - 4.5) / 5.0).clamp(0.0, 0.9) as f32;
        let co2_tox = ((env.co2_kpa - 4.0) / 6.0).clamp(0.0, 1.0) as f32;
        target *= 1.0 - 0.5 * co2_tox;
        target *= 1.0 - self.drug_load(data, "resp").min(0.8);
        target *= (self.blood / 0.75).min(1.0);
        self.oxygen += (target - self.oxygen) * relax(hours, 0.15);
        self.oxygen = sat(self.oxygen);
        if self.oxygen < 0.6 {
            organ_hit(
                organ_of("cerveau"),
                10.0 * (0.6 - self.oxygen) * hours,
                &mut out,
            );
        }
        if self.oxygen < 0.3 {
            organ_hit(
                organ_of("coeur"),
                4.0 * (0.3 - self.oxygen) * hours,
                &mut out,
            );
        }
        organ_hit(organ_of("poumons"), 0.1 * co2_tox * hours, &mut out);

        // Low pressure: fluids boil under 10 kPa, the lungs and ears tear under 50.
        let lowp = ((50.0 - env.pressure_kpa) / 50.0).clamp(0.0, 1.0) as f32;
        let boil = ((10.0 - env.pressure_kpa) / 10.0).clamp(0.0, 1.0) as f32;
        let baro = (lowp.powi(3) * 1.5 + boil * 4.0) * hours;
        if baro > 0.0 {
            if let Some(t) = data.part_index("torse") {
                if let Some((o, d)) = self.add_wound(data, "barotraumatisme", t, baro) {
                    organ_hit(Some(o), d * 0.5, &mut out);
                }
            }
            organ_hit(organ_of("poumons"), 0.5 * baro, &mut out);
        }

        // Temperature.
        let amb_c = env.temp_k - 273.15;
        let eff_c = (37.0 - (37.0 - amb_c) * (1.0 - 0.9 * env.insulation.clamp(0.0, 1.0))) as f32;
        let delta = if eff_c < 15.0 {
            eff_c - 15.0
        } else if eff_c > 28.0 {
            eff_c - 28.0
        } else {
            0.0
        };
        let target_core = (NORMAL_CORE + 0.45 * delta).clamp(20.0, 46.0);
        self.core_temp += (target_core - self.core_temp) * relax(hours, 2.5);
        if eff_c < 15.0 {
            out.hunger_mult = (1.0 + 0.03 * (15.0 - eff_c)).min(2.0);
        }
        if self.core_temp < 35.0 {
            organ_hit(
                organ_of("coeur"),
                0.03 * (35.0 - self.core_temp) * hours,
                &mut out,
            );
            out.stress_add += 0.1;
        }
        if self.core_temp > 40.0 {
            organ_hit(
                organ_of("cerveau"),
                0.05 * (self.core_temp - 40.0) * hours,
                &mut out,
            );
            organ_hit(
                organ_of("reins"),
                0.05 * (self.core_temp - 40.0) * hours,
                &mut out,
            );
            out.stress_add += 0.1;
        }
        if eff_c < -5.0 {
            let rate = 0.05 * ((-5.0 - eff_c) / 10.0).min(3.0) * hours;
            for (i, p) in data.body_parts.iter().enumerate() {
                if p.limb {
                    self.add_wound(data, "gelure", i, rate);
                }
            }
        }
        if amb_c > 60.0 {
            let rate = (0.1 * ((eff_c - 60.0) / 40.0).max(0.0) * hours).min(1.0);
            for name in ["bras_g", "bras_d", "torse"] {
                if let Some(i) = data.part_index(name) {
                    self.add_wound(data, "brulure_plaie", i, rate);
                }
            }
        }

        // Gravity and acceleration.
        let g = env.gravity_g as f32;
        if g < 0.5 {
            self.bone = sat(self.bone - 4.0e-5 * (0.5 - g) / 0.5 * hours);
        } else if g >= 0.8 {
            self.bone = sat(self.bone + 2.0e-5 * hours);
        }
        if g > 1.3 {
            out.fatigue_rate += 0.02 * (g - 1.3);
        }
        let brittle = 1.0 + 3.0 * (1.0 - self.bone);
        if g > 2.5 {
            for name in ["jambe_g", "jambe_d"] {
                if let Some(i) = data.part_index(name) {
                    self.add_wound(data, "contusion", i, 0.05 * (g - 2.5) * hours);
                }
            }
        }
        let a = env.accel_g as f32;
        if let Some(t) = data.part_index("torse") {
            if a > 6.0 {
                self.add_wound(data, "contusion", t, 0.05 * (a - 6.0) * hours);
            }
            if a > 9.0 {
                if let Some((o, d)) =
                    self.add_wound(data, "hemorragie_interne", t, 0.08 * (a - 9.0) * hours)
                {
                    organ_hit(Some(o), d, &mut out);
                }
            }
            if a > 12.0 {
                self.add_wound(data, "fracture_os", t, 0.2 * (a - 12.0) * brittle * hours);
            }
        }

        // Radiation: a dose in Sv, mended slowly; the sickness above 1 Sv, death far above.
        let protect = self.drug_effect(data, "rad").min(0.9) as f64;
        let received = (env.rad_sv_day / 24.0 * f64::from(hours)) as f32 + self.pending_dose;
        self.pending_dose = 0.0;
        let dose = received * (1.0 - protect as f32);
        self.acute_dose += dose;
        self.total_dose += dose;
        self.acute_dose *= 0.5f32.powf(hours / (20.0 * 24.0));
        out.dose = dose;
        // The cells that divide fast pay first: damage follows the dose received.
        if dose > 0.0 {
            for (name, sens) in [
                ("estomac", 0.15),
                ("peau", 0.15),
                ("foie", 0.08),
                ("reins", 0.08),
            ] {
                organ_hit(organ_of(name), dose * sens, &mut out);
            }
            if self.acute_dose > 4.0 {
                out.health_loss += 0.08 * dose;
            }
        }
        if env.rad_sv_day > 3.0 {
            if let Some(t) = data.part_index("torse") {
                self.add_wound(data, "brulure_rayons_loc", t, dose);
            }
        }
        out.radiation_sick = self.acute_dose > 1.0;

        // Wounds: bleeding, pain, infection, healing.
        let clot = self.drug_effect(data, "clot").clamp(0.0, 0.9);
        let fight = self.drug_effect(data, "infection").clamp(0.0, 1.0);
        let heal_drug = self.drug_effect(data, "heal").max(0.0);
        let hunger_bad = care.hunger.max(care.thirst);
        let medic = (care.medic_skill / 10.0).clamp(0.0, 1.0) as f32;
        let age_f = 1.0 - 0.4 * ((care.age - 40.0) / 40.0).clamp(0.0, 1.0);
        let base_heal = (1.0 + 0.6 * medic + 0.3 * care.medbay.min(1.0) as f32)
            * (1.0 - 0.5 * hunger_bad)
            * (1.0 - 0.3 * care.fatigue)
            * age_f
            * (1.0 + 0.8 * heal_drug);
        let mut pain = 0.0f32;
        let mut bleed = 0.0f32;
        let mut done: Vec<usize> = Vec::new();
        for (i, w) in self.wounds.iter_mut().enumerate() {
            let k = &data.wounds[w.kind as usize];
            let dressing = w.dressed;
            bleed += k.bleed as f32 * w.sev * (1.0 - 0.9 * dressing) * (1.0 - clot);
            pain += k.pain as f32 * w.sev;
            // Infection of an open wound.
            if k.infect > 0.0 && w.sev > 0.05 && w.infection <= 0.0 {
                let p = k.infect as f32 / 24.0
                    * hours
                    * (1.0 + hunger_bad)
                    * (1.0 - 0.85 * dressing)
                    * (1.0 - fight);
                if rng.f64() < f64::from(p) {
                    w.infection = 0.05;
                    out.notes.push(format!(
                        "{} s'infecte",
                        data.wounds[w.kind as usize].name.to_lowercase()
                    ));
                }
            }
            if w.infection > 0.0 {
                let immune = 0.01 + 0.03 * (1.0 - hunger_bad) + 0.1 * dressing * 0.5;
                let grow = 0.05 * (1.0 + hunger_bad);
                w.infection = sat(w.infection + (grow - immune - 0.25 * fight) * hours);
                w.sev = sat(w.sev + w.infection * 0.02 * hours);
                if w.infection > 0.8 {
                    out.sepsis = true;
                }
            }
            // Healing: a wound that needs a dressing mends slowly without it.
            let mut f = base_heal;
            if k.dress {
                f *= 0.4 + 0.6 * dressing;
            }
            if w.infection > 0.0 {
                f *= 0.2;
            }
            if k.id == "fracture_os" {
                f *= self.bone.max(0.3);
            }
            w.sev -= hours / 24.0 / k.heal_days as f32 * f;
            if w.sev <= 0.005 {
                done.push(i);
            }
        }
        for &i in done.iter().rev() {
            let w = self.wounds.remove(i);
            let k = &data.wounds[w.kind as usize];
            // A badly hurt wound that was not treated leaves a permanent disability.
            let left = if k.dress && w.dressed < 0.5 {
                0.4 * w.peak
            } else if w.peak > 0.9 {
                0.1
            } else {
                0.0
            };
            if left > 0.05 && (w.part as usize) < self.disability.len() {
                let d = &mut self.disability[w.part as usize];
                *d = sat(*d + left);
                out.notes.push(format!(
                    "{} laisse une sequelle ({})",
                    k.name.to_lowercase(),
                    data.body_parts[w.part as usize].name.to_lowercase()
                ));
            }
        }
        self.blood = sat(self.blood - bleed * hours);
        // The body makes blood again when fed and watered.
        if care.thirst < 0.6 && care.hunger < 0.8 && bleed < 0.001 {
            self.blood = sat(self.blood + 0.003 * hours);
        }
        if self.blood < 0.6 {
            out.health_loss += 0.1 * (0.6 - self.blood) * hours;
        }
        if self.blood <= BLOOD_FATAL {
            out.death = Some("hemorragie");
        }

        // Drugs: clearance by the liver, tolerance, dependence, withdrawal, overdose.
        let clear = 0.7 + 0.3 * care.liver.clamp(0.0, 1.0);
        let mut withdrawal = 0.0f32;
        for d in &mut self.drugs {
            let def = &data.drugs[d.drug as usize];
            d.level *= 0.5f32.powf(hours * clear / def.half_life_h as f32);
            d.tolerance *= 0.5f32.powf(hours / (24.0 * 14.0));
            if d.level < 0.15 && d.dependence > 0.2 {
                withdrawal += d.dependence * (1.0 - d.level / 0.15);
            }
            d.dependence *= 0.5f32.powf(hours / (24.0 * 60.0));
        }
        let liver_load = self.drug_load(data, "liver");
        organ_hit(organ_of("foie"), liver_load * hours / 24.0, &mut out);
        let pressure = self.overdose_pressure(data, care.liver);
        if pressure > OVERDOSE {
            let over = pressure - OVERDOSE;
            organ_hit(organ_of("foie"), 0.1 * over * hours, &mut out);
            organ_hit(organ_of("reins"), 0.05 * over * hours, &mut out);
            if pressure > 2.4 {
                out.health_loss += 0.2 * (pressure - 2.4) * hours;
            }
            out.notes.push("surdose".into());
        }
        self.drugs
            .retain(|d| d.level > 0.01 || d.dependence > 0.01 || d.tolerance > 0.01);

        // Pain, mind.
        let relief = self.drug_effect(data, "pain");
        self.pain = sat(pain + 0.5 * withdrawal - relief).min(1.0);
        if self.pain > 0.7 {
            self.trauma = sat(self.trauma + 0.01 * hours);
        }
        self.trauma = sat(self.trauma - 0.0005 * hours * (1.0 + medic));
        let calm = self.drug_effect(data, "stress");
        out.stress_add += 0.35 * self.pain + 0.3 * self.trauma + 0.4 * withdrawal - calm;
        out.fatigue_rate -= self.drug_effect(data, "fatigue") * 0.2;
        out.unconscious = self.unconscious();

        // Deaths of the body itself.
        if self.core_temp <= 24.0 {
            out.death.get_or_insert("hypothermie");
        }
        if self.core_temp >= 44.0 {
            out.death.get_or_insert("coup de chaleur");
        }
        if self.acute_dose >= 8.0 {
            out.death.get_or_insert("irradiation");
        }
        out
    }

    /// Wake from cryostasis: the body is rewarmed.
    pub fn thaw(&mut self, now: f64) {
        self.cryo = false;
        self.core_temp = NORMAL_CORE;
        self.cryo_wake = 0.0;
        self.cryo_urgent = false;
        self.cryo_last = now;
        self.cryo_cpa = 0.0;
    }

    /// One step of a sleeper. `held`: a powered caisson keeps it at its set temperature, else it
    /// drifts to `ambient` (C). The core cools (or rewarms) by first order lags; crossing the band
    /// of freezing water (-40..0 C) grows ice crystals unless the blood carries cryoprotectant, and
    /// much more when nothing controls the crossing. A caisson that is too warm to vitrify lets
    /// the ice grow back slowly. Radiation keeps adding up: frozen cells mend nothing.
    pub fn sleep_step(
        &mut self,
        hours: f32,
        held: bool,
        ambient: f32,
        rad_sv_day: f64,
    ) -> SleepOut {
        let mut out = SleepOut::default();
        let before = self.core_temp;
        let waking = self.cryo_wake > 0.0;
        if waking {
            let k = (hours / self.cryo_wake.max(hours)).min(1.0);
            self.core_temp += (NORMAL_CORE - self.core_temp) * k;
            self.cryo_wake = (self.cryo_wake - hours).max(0.0);
            if self.cryo_wake <= 0.0 {
                self.core_temp = NORMAL_CORE;
                out.waking_done = true;
            }
        } else {
            let target = if held { self.cryo_set } else { ambient };
            let tau = if target < self.core_temp {
                SLEEP_COOL_TAU
            } else {
                SLEEP_WARM_TAU
            };
            self.core_temp += (target - self.core_temp) * relax(hours, tau);
        }
        let controlled = held || waking;
        let crossed = band_crossed(before, self.core_temp);
        if crossed > 0.0 {
            let open = if controlled { 1.0 } else { UNCONTROLLED_ICE };
            let hurt = crossed * ICE_CROSSING * (1.0 - ICE_PROTECTION * self.cryo_cpa) * open;
            self.cryo_ice += hurt;
            out.ice = hurt;
        }
        if held && !waking && self.core_temp > RECRYSTALLISE_BELOW {
            // Too warm to be glass: the ice grows back (per day).
            let warm = ((self.core_temp - RECRYSTALLISE_BELOW) / 110.0).clamp(0.0, 1.0);
            let hurt = RECRYSTALLISE * warm * (1.0 - ICE_PROTECTION * self.cryo_cpa) * hours / 24.0;
            self.cryo_ice += hurt;
            out.ice += hurt;
        }
        if !held && !waking && self.core_temp > 8.0 {
            // Thawed without care: the tissues die in hours.
            let hurt = UNCONTROLLED_THAW * hours;
            self.cryo_ice += hurt;
            out.ice += hurt;
        }
        self.cryo_ice = self.cryo_ice.min(1.5);
        // Radiation: the dose adds up and nothing repairs it while frozen.
        let received = (rad_sv_day / 24.0 * f64::from(hours)) as f32 + self.pending_dose;
        self.pending_dose = 0.0;
        self.acute_dose += received;
        self.total_dose += received;
        out.dose = received;
        self.oxygen = 1.0;
        out
    }
}

/// What a sleeper's step reports.
#[derive(Clone, Copy, Debug, Default)]
pub struct SleepOut {
    pub ice: f32,
    pub dose: f32,
    pub waking_done: bool,
}

/// Time constants (hours) of the cooling and of the drift toward the room.
pub const SLEEP_COOL_TAU: f32 = 2.0;
pub const SLEEP_WARM_TAU: f32 = 3.0;
/// Ice grown by one full crossing of the freezing band without protection, share saved by a full
/// load of cryoprotectant, and the multiplier when the crossing is not controlled.
pub const ICE_CROSSING: f32 = 0.45;
pub const ICE_PROTECTION: f32 = 0.92;
pub const UNCONTROLLED_ICE: f32 = 2.0;
/// Above this temperature (C) a held sleeper regrows ice, per day at the worst.
pub const RECRYSTALLISE_BELOW: f32 = -110.0;
pub const RECRYSTALLISE: f32 = 0.004;
/// Ice per hour of an uncontrolled thaw above 8 C.
pub const UNCONTROLLED_THAW: f32 = 0.06;

/// Share (0..1) of the freezing band (-40..0 C) crossed when the core goes from `a` to `b`.
pub fn band_crossed(a: f32, b: f32) -> f32 {
    let (lo, hi) = (a.min(b), a.max(b));
    ((hi.min(0.0) - lo.max(-40.0)) / 40.0).clamp(0.0, 1.0)
}

/// How fast a body ages at core temperature `t` (C): 1 warm, about a thousandth in a caisson at
/// -80 C, a five thousandth deeper (the floor).
pub fn bio_rate(t: f32) -> f64 {
    10f64.powf((f64::from(t) - 30.0) / 40.0).clamp(0.0002, 1.0)
}

/// Wear of the organs with age, integrity per day (the heart and the brain mend slowest and
/// feel it first).
pub fn aging_wear(age: f32) -> f32 {
    0.000_02 * (age - 55.0).max(0.0).powf(1.5)
}

/// Chance of dying of old age during `days` (a Gompertz law); `health_base` is the genetic
/// and trait bonus of the person: each 0.1 is like being a year younger... ten times.
pub fn natural_death_chance(age: f32, health_base: f64, days: f64) -> f64 {
    let a = f64::from(age) - 10.0 * health_base;
    let per_year = (0.000_2 * (0.1 * (a - 20.0)).exp()).min(5.0);
    1.0 - (-per_year * days / 365.25).exp()
}

/// Chance that a resuscitation works: the skill of the medic, the infirmary, and how long the
/// heart has been stopped.
pub fn revive_chance(skill: f64, medbay: f64, minutes: f64) -> f64 {
    let base = 0.15 + 0.06 * skill + 0.2 * medbay.min(1.0);
    (base * (-minutes / 12.0).exp()).clamp(0.0, 0.95)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameData {
        GameData::load(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data"))
            .expect("data loads")
    }

    fn run(
        b: &mut Body,
        d: &GameData,
        env: &Exposure,
        care: &Care,
        hours: f32,
        n: usize,
    ) -> StepOut {
        let mut last = StepOut::default();
        let mut organs = vec![1.0f32; d.organs.len()];
        for k in 0..n {
            last = b.step(d, env, care, hours, 5, k as i64);
            for (o, dmg) in &last.organ_damage {
                organs[*o] = (organs[*o] - dmg).max(0.0);
            }
        }
        last
    }

    fn care() -> Care {
        Care {
            liver: 1.0,
            age: 30.0,
            ..Care::default()
        }
    }

    fn wound_sev(b: &Body, d: &GameData, kind: &str) -> f32 {
        let k = d.wound_index(kind).expect("kind");
        b.wounds
            .iter()
            .filter(|w| w.kind as usize == k)
            .map(|w| w.sev)
            .sum()
    }

    #[test]
    fn a_healthy_body_in_the_cabin_stays_unhurt() {
        let d = data();
        let mut b = Body::new(&d);
        run(&mut b, &d, &Exposure::CABIN, &care(), 1.0, 200);
        assert!(b.wounds.is_empty() && b.blood > 0.99 && b.oxygen > 0.99);
        assert!((b.core_temp - NORMAL_CORE).abs() < 0.2);
    }

    #[test]
    fn decompression_hurts_more_at_lower_pressure() {
        let d = data();
        let sev = |to: f64| {
            let mut b = Body::new(&d);
            b.rapid_decompression(&d, 101.3, to);
            wound_sev(&b, &d, "barotraumatisme")
        };
        assert_eq!(sev(90.0), 0.0, "a small drop is harmless");
        assert!(sev(40.0) > 0.0);
        assert!(sev(0.0) > sev(40.0) + 0.2, "{} {}", sev(0.0), sev(40.0));
        // Staying in a thin air wounds too, more the thinner it is.
        let stay = |p: f64| {
            let mut b = Body::new(&d);
            let env = Exposure {
                pressure_kpa: p,
                o2_frac: 1.0,
                ..Exposure::CABIN
            };
            run(&mut b, &d, &env, &care(), 0.1, 10);
            wound_sev(&b, &d, "barotraumatisme")
        };
        assert!(stay(5.0) > stay(30.0) && stay(30.0) > stay(80.0));
    }

    #[test]
    fn thin_air_starves_the_brain_and_a_full_tank_saves() {
        let d = data();
        let brain = d.organ_index("cerveau").expect("brain");
        let loss = |env: Exposure| {
            let mut b = Body::new(&d);
            let mut total = 0.0;
            for k in 0..6 {
                let o = b.step(&d, &env, &care(), 0.1, 5, k);
                total += o
                    .organ_damage
                    .iter()
                    .filter(|(i, _)| *i == brain)
                    .map(|(_, v)| v)
                    .sum::<f32>();
            }
            (b.oxygen, total)
        };
        let (ox_thin, loss_thin) = loss(Exposure {
            pressure_kpa: 30.0,
            ..Exposure::CABIN
        });
        let (ox_ok, loss_ok) = loss(Exposure::CABIN);
        assert!(ox_thin < 0.6 && loss_thin > 0.1, "{ox_thin} {loss_thin}");
        assert!(ox_ok > 0.99 && loss_ok == 0.0);
        // The same thin air with pure oxygen (a suit) is breathable: the partial pressure counts.
        let (ox_suit, _) = loss(Exposure {
            pressure_kpa: 30.0,
            o2_frac: 1.0,
            ..Exposure::CABIN
        });
        assert!(ox_suit > 0.99);
    }

    #[test]
    fn cold_chills_the_core_freezes_limbs_and_burns_food() {
        let d = data();
        let cold = Exposure {
            temp_k: 253.0,
            ..Exposure::CABIN
        };
        let mut bare = Body::new(&d);
        let out = run(&mut bare, &d, &cold, &care(), 0.5, 12);
        assert!(bare.core_temp < 34.0, "{}", bare.core_temp);
        assert!(
            out.hunger_mult > 1.4,
            "the cold burns food: {}",
            out.hunger_mult
        );
        assert!(wound_sev(&bare, &d, "gelure") > 0.0);
        let mut suited = Body::new(&d);
        let warm = Exposure {
            insulation: 0.75,
            ..cold
        };
        run(&mut suited, &d, &warm, &care(), 0.5, 12);
        assert!(suited.core_temp > bare.core_temp + 2.0);
        assert!(wound_sev(&suited, &d, "gelure") < wound_sev(&bare, &d, "gelure"));
        // The heat: a hot cabin gives a heat stroke.
        let mut hot = Body::new(&d);
        let out = run(
            &mut hot,
            &d,
            &Exposure {
                temp_k: 330.0,
                ..Exposure::CABIN
            },
            &care(),
            0.5,
            12,
        );
        assert!(hot.core_temp > 40.0 && out.hunger_mult == 1.0);
    }

    #[test]
    fn heavy_acceleration_blacks_out_and_breaks_but_low_gravity_thins_the_bones() {
        let d = data();
        let mut b = Body::new(&d);
        let hard = Exposure {
            accel_g: 14.0,
            ..Exposure::CABIN
        };
        run(&mut b, &d, &hard, &care(), 0.1, 10);
        assert!(b.unconscious(), "oxygen {}", b.oxygen);
        assert!(wound_sev(&b, &d, "hemorragie_interne") > 0.0);
        assert!(wound_sev(&b, &d, "fracture_os") > 0.0);
        let mut easy = Body::new(&d);
        run(
            &mut easy,
            &d,
            &Exposure {
                accel_g: 2.0,
                ..Exposure::CABIN
            },
            &care(),
            0.1,
            10,
        );
        assert!(easy.wounds.is_empty() && !easy.unconscious());
        let mut weightless = Body::new(&d);
        run(
            &mut weightless,
            &d,
            &Exposure {
                gravity_g: 0.0,
                ..Exposure::CABIN
            },
            &care(),
            24.0,
            60,
        );
        assert!(weightless.bone < 0.95, "{}", weightless.bone);
        // A brittle skeleton breaks on a fall that a sound one survives.
        let h = 3.0;
        let mut sound = Body::new(&d);
        sound.fall(&d, h, 9.81, 0.0, 3);
        let mut brittle = Body::new(&d);
        brittle.bone = 0.5;
        brittle.fall(&d, h, 9.81, 0.0, 3);
        assert!(wound_sev(&brittle, &d, "fracture_os") > wound_sev(&sound, &d, "fracture_os"));
        // The same fall hurts less in low gravity.
        let mut moon = Body::new(&d);
        moon.fall(&d, 8.0, 1.6, 0.0, 3);
        let mut earth = Body::new(&d);
        earth.fall(&d, 8.0, 9.81, 0.0, 3);
        assert!(earth.wounds.len() > moon.wounds.len());
    }

    #[test]
    fn bleeding_drains_blood_and_dressing_and_clotting_stop_it() {
        let d = data();
        let c = d.part_index("bras_g").expect("arm");
        let bleed = |dress: bool, clot: bool| {
            let mut b = Body::new(&d);
            b.add_wound(&d, "coupure", c, 0.9);
            if dress {
                b.dress_wounds(&d, 6.0);
            }
            if clot {
                b.take_drug(&d, d.drug_index("coagulant").expect("clot"));
            }
            run(&mut b, &d, &Exposure::CABIN, &care(), 0.25, 10);
            b.blood
        };
        let open = bleed(false, false);
        assert!(open < 0.9, "an open cut bleeds: {open}");
        assert!(bleed(true, false) > open + 0.05);
        assert!(bleed(false, true) > open + 0.02);
        // Without any care a haemorrhage kills.
        let mut b = Body::new(&d);
        b.add_wound(
            &d,
            "hemorragie_interne",
            d.part_index("torse").expect("t"),
            1.0,
        );
        let out = run(&mut b, &d, &Exposure::CABIN, &care(), 0.25, 24);
        assert_eq!(out.death, Some("hemorragie"), "blood {}", b.blood);
    }

    #[test]
    fn a_medic_and_food_heal_faster_and_dressing_prevents_infection_and_sequelae() {
        let d = data();
        let leg = d.part_index("jambe_g").expect("leg");
        let remaining = |medic: f64, hunger: f32, dress: bool| {
            let mut b = Body::new(&d);
            b.add_wound(&d, "fracture_os", leg, 0.8);
            if dress {
                b.dress_wounds(&d, medic);
            }
            let care = Care {
                medic_skill: medic,
                hunger,
                ..care()
            };
            run(&mut b, &d, &Exposure::CABIN, &care, 12.0, 30);
            wound_sev(&b, &d, "fracture_os")
        };
        let alone = remaining(0.0, 0.1, true);
        let doctor = remaining(8.0, 0.1, true);
        let starving = remaining(0.0, 0.95, true);
        let undressed = remaining(0.0, 0.1, false);
        assert!(doctor < alone, "medic: {doctor} < {alone}");
        assert!(alone < starving, "food: {alone} < {starving}");
        assert!(alone < undressed, "splint: {alone} < {undressed}");
        // A severe fracture left alone ends with a permanent disability; a splinted one does not.
        let end = |dress: bool| {
            let mut b = Body::new(&d);
            b.add_wound(&d, "fracture_os", leg, 0.95);
            if dress {
                b.dress_wounds(&d, 7.0);
            }
            let care = Care {
                medic_skill: 7.0,
                ..care()
            };
            run(&mut b, &d, &Exposure::CABIN, &care, 24.0, 150);
            (b.wounds.len(), b.disability[leg])
        };
        let (n_free, lame) = end(false);
        let (n_splint, ok) = end(true);
        assert_eq!((n_free, n_splint), (0, 0), "both healed in the end");
        assert!(lame > 0.2 && ok < 0.15, "{lame} {ok}");
        // The disability costs work speed.
        let mut b = Body::new(&d);
        b.disability[leg] = lame;
        assert!(b.trait_effect(&d, "work_speed") < -0.02);
    }

    #[test]
    fn antibiotics_beat_an_infection_that_otherwise_wins() {
        let d = data();
        let arm = d.part_index("bras_d").expect("arm");
        let end = |drug: bool| {
            let mut b = Body::new(&d);
            b.add_wound(&d, "coupure", arm, 0.4);
            b.wounds[0].infection = 0.3;
            b.wounds[0].dressed = 0.3;
            let care = Care {
                hunger: 0.7,
                ..care()
            };
            for k in 0..40 {
                if drug && k % 16 == 0 {
                    b.take_drug(&d, d.drug_index("antibiotique").expect("abx"));
                }
                b.step(&d, &Exposure::CABIN, &care, 0.5, 5, k);
            }
            b.wounds.first().map_or(0.0, |w| w.infection)
        };
        assert!(end(false) > 0.5, "untreated: {}", end(false));
        assert!(end(true) < 0.1, "treated: {}", end(true));
    }

    #[test]
    fn drugs_tolerance_dependence_withdrawal_and_interactions() {
        let d = data();
        let opioid = d.drug_index("opioide").expect("o");
        let sedative = d.drug_index("sedatif").expect("s");
        let liver = 1.0;
        let mut b = Body::new(&d);
        b.take_drug(&d, opioid);
        let first = b.drug_effect(&d, "pain");
        // Taking it again and again raises tolerance and dependence.
        let mut habit = Body::new(&d);
        for _ in 0..10 {
            habit.take_drug(&d, opioid);
            habit.drugs[0].level = 1.0;
        }
        assert!(
            habit.drug_effect(&d, "pain") < first,
            "tolerance weakens it"
        );
        assert!(habit.drugs[0].dependence > 0.5);
        // When it wears off, the addict suffers: pain without any wound.
        habit.drugs[0].level = 0.0;
        habit.step(&d, &Exposure::CABIN, &care(), 0.1, 5, 1);
        assert!(
            habit.pain > 0.2 && habit.pain > b.pain,
            "withdrawal {}",
            habit.pain
        );
        // Opioid and sedative together weigh far more than each, and a weak liver worse still.
        let mut mix = Body::new(&d);
        mix.take_drug(&d, opioid);
        let alone = mix.overdose_pressure(&d, liver);
        mix.take_drug(&d, sedative);
        mix.take_drug(&d, opioid);
        let both = mix.overdose_pressure(&d, liver);
        assert!(both > 2.0 * alone && both > OVERDOSE, "{alone} {both}");
        assert!(mix.overdose_pressure(&d, 0.2) > both);
        // The overdose depresses breathing and hurts the liver.
        let mut hits = 0.0;
        let mut ox = 1.0;
        for k in 0..6 {
            let o = mix.step(&d, &Exposure::CABIN, &care(), 0.2, 5, k);
            ox = mix.oxygen;
            hits += o.organ_damage.iter().map(|x| x.1).sum::<f32>();
        }
        assert!(ox < 0.9 && hits > 0.0, "{ox} {hits}");
    }

    #[test]
    fn radiation_sickens_and_a_protective_drug_helps() {
        let d = data();
        let storm = Exposure {
            rad_sv_day: 24.0,
            ..Exposure::CABIN
        };
        let dose = |drug: bool| {
            let mut b = Body::new(&d);
            if drug {
                b.take_drug(&d, d.drug_index("antiradiation").expect("rad"));
            }
            let out = run(&mut b, &d, &storm, &care(), 0.5, 4);
            (b.acute_dose, out.radiation_sick)
        };
        let (bare, sick) = dose(false);
        let (guarded, _) = dose(true);
        assert!(sick && bare > 1.5, "{bare}");
        assert!(guarded < bare * 0.8);
    }

    #[test]
    fn old_age_kills_and_the_young_are_spared() {
        let years = |age: f32| {
            let mut n = 0;
            for s in 0..500u64 {
                let mut r = Rng::new(s);
                if r.f64() < natural_death_chance(age, 0.0, 365.25) {
                    n += 1;
                }
            }
            n
        };
        assert!(years(25.0) < 3 && years(60.0) < 15);
        assert!(years(90.0) > 60, "{}", years(90.0));
        assert!(natural_death_chance(80.0, 0.3, 365.0) < natural_death_chance(80.0, 0.0, 365.0));
        assert!(aging_wear(30.0) == 0.0 && aging_wear(85.0) > 0.002);
    }

    #[test]
    fn resuscitation_depends_on_skill_the_infirmary_and_time() {
        assert!(revive_chance(8.0, 1.0, 1.0) > revive_chance(1.0, 0.0, 1.0));
        assert!(revive_chance(8.0, 1.0, 30.0) < revive_chance(8.0, 1.0, 2.0));
    }

    #[test]
    fn diagnosis_hides_internal_wounds_from_the_unskilled() {
        let d = data();
        let mut b = Body::new(&d);
        b.add_wound(&d, "coupure", 2, 0.5);
        b.add_wound(&d, "hemorragie_interne", 1, 0.5);
        let (seen, hidden) = b.diagnose(&d, 0.0, 0.0);
        assert_eq!((seen.len(), hidden), (1, 1));
        let (seen, hidden) = b.diagnose(&d, 5.0, 0.0);
        assert_eq!((seen.len(), hidden), (2, 0));
        let (seen, _) = b.diagnose(&d, 0.0, 1.0);
        assert_eq!(seen.len(), 2, "the infirmary scans");
        assert!((seen[0].1 - 0.5).abs() < 1e-6, "exact with the scanner");
    }
}
