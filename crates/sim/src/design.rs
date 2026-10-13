//! Modular ship: design (hull + installed modules), derived stats, power bus, heat and damage.
use serde::{Deserialize, Serialize};
use vector_data::{GameData, Module};

/// Reference temperature (K) at which radiators deliver their rated power, and the cold floor.
pub const T_REF: f64 = 350.0;
pub const T_COLD: f64 = 150.0;
/// Above this the modules start to cook; above SCRAM the reactors shut down.
pub const T_DAMAGE: f64 = 650.0;
pub const T_SCRAM: f64 = 900.0;
/// Fraction of reactor output released as waste heat.
pub const REACTOR_WASTE: f64 = 0.6;
/// Specific isp of the RCS thrusters (s).
pub const RCS_ISP: f64 = 20_000.0;
/// Damage points absorbed per MJ of shield capacity.
pub const SHIELD_PTS_PER_MJ: f64 = 10.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShipDesign {
    pub hull: usize,
    pub slots: Vec<Option<usize>>,
}

impl ShipDesign {
    pub fn empty(data: &GameData, hull: usize) -> Self {
        ShipDesign {
            hull,
            slots: vec![None; data.hulls[hull].slots.len()],
        }
    }

    /// Install `module` into `slot`. Fails when the slot is taken, of the wrong type or too small.
    pub fn install(
        &mut self,
        data: &GameData,
        slot: usize,
        module: usize,
    ) -> Result<(), &'static str> {
        let (ty, size) = *data.hulls[self.hull]
            .slots
            .get(slot)
            .ok_or("emplacement inconnu")?;
        let m = &data.modules[module];
        if self.slots[slot].is_some() {
            return Err("emplacement occupe");
        }
        if m.slot != ty {
            return Err("type d'emplacement incompatible");
        }
        if m.size > size {
            return Err("module trop grand pour l'emplacement");
        }
        self.slots[slot] = Some(module);
        Ok(())
    }

    pub fn remove(&mut self, slot: usize) -> Option<usize> {
        self.slots.get_mut(slot).and_then(Option::take)
    }

    /// Install by module id into the first compatible free slot.
    pub fn install_first_fit(&mut self, data: &GameData, id: &str) -> Result<usize, &'static str> {
        let mi = data.module_index(id).ok_or("module inconnu")?;
        for s in 0..self.slots.len() {
            if self.install(data, s, mi).is_ok() {
                return Ok(s);
            }
        }
        Err("aucun emplacement compatible")
    }

    /// The starter ship: a small mining prospector.
    pub fn starter(data: &GameData) -> Self {
        let hull = data.hull_index("prospector").unwrap_or(0);
        let mut d = ShipDesign::empty(data, hull);
        for id in [
            "torch_mk2",
            "fusion_mk2",
            "gyro_mk1",
            "tank_mk2",
            "cargo_mk2",
            "lifesup_mk1",
            "battery_mk1",
            "habitat_mk1",
            "ftl_mk1",
            "radiator_mk2",
            "rcs_mk1",
            "laser_mk2",
        ] {
            // Ignore modules a custom data set may not offer; the ship is still valid.
            let _ = d.install_first_fit(data, id);
        }
        d
    }

    pub fn modules<'a>(
        &'a self,
        data: &'a GameData,
    ) -> impl Iterator<Item = (usize, &'a Module)> + 'a {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, m)| m.map(|m| (i, &data.modules[m])))
    }
}

/// Nominal numbers derived from the design and module health (before power allocation).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShipStats {
    /// Hull + installed modules, without cargo or propellant.
    pub mass_dry: f64,
    pub thrust: f64,
    pub isp: f64,
    pub fuel_cap: f64,
    pub cargo_kg: f64,
    pub cargo_m3: f64,
    pub battery_cap: f64,
    pub radiate_kw: f64,
    pub hull_max: f64,
    pub power_capacity: f64,
    pub power_peak: f64,
    pub mining_rate: f64,
    pub scan_range: f64,
    pub jump_range: f64,
    pub crew_cap: f64,
    pub shield: f64,
}

impl ShipStats {
    pub fn compute(data: &GameData, design: &ShipDesign, health: &[f64]) -> ShipStats {
        let h = &data.hulls[design.hull];
        let mut s = ShipStats {
            mass_dry: h.mass,
            hull_max: h.hp,
            ..Default::default()
        };
        let mut thrust_isp = 0.0;
        for (i, m) in design.modules(data) {
            let k = health.get(i).copied().unwrap_or(1.0);
            s.mass_dry += m.mass;
            s.thrust += m.thrust * k;
            thrust_isp += m.thrust * k * m.isp;
            s.fuel_cap += m.fuel;
            s.cargo_kg += m.cargo_kg * k;
            s.cargo_m3 += m.cargo_m3 * k;
            s.battery_cap += m.battery;
            s.radiate_kw += m.radiate * k;
            s.hull_max += if m.category == "armor" { m.hp } else { 0.0 };
            if m.power > 0.0 {
                s.power_capacity += m.power * k;
            } else {
                s.power_peak += -m.power;
            }
            s.mining_rate += m.mining_rate * k;
            s.scan_range = f64::max(s.scan_range, m.scan_range * k);
            s.jump_range = f64::max(s.jump_range, m.jump_range * if k > 0.0 { 1.0 } else { 0.0 });
            s.crew_cap += m.crew_cap;
            s.shield += m.shield * k;
        }
        s.isp = if s.thrust > 0.0 {
            thrust_isp / s.thrust
        } else {
            0.0
        };
        s
    }
}

/// What the pilot/AI is asking of the systems this step.
#[derive(Clone, Copy, Debug, Default)]
pub struct Activity {
    pub throttle: f64,
    pub mining: bool,
    pub rcs_use: f64,
    pub jumping: bool,
    pub firing: bool,
    /// Heat (kW) arriving from outside: sunlight on the hull, the air of an entry.
    pub ext_heat_kw: f64,
}

/// Capabilities actually available this step after power allocation.
#[derive(Clone, Debug, Default)]
pub struct Effective {
    pub thrust: f64,
    pub isp: f64,
    pub rcs_n: f64,
    pub max_rate: f64,
    pub ang_accel: f64,
    pub mining_rate: f64,
    pub mining_range: f64,
    pub life_ok: bool,
    /// Powered scanner reach (m), 0 without a scanner.
    pub scan_range: f64,
    /// Light years per jump available right now (needs a powered FTL drive).
    pub jump_range: f64,
    /// Number of people the powered life support can sustain.
    pub life_capacity: f64,
    /// Shield capacity in damage points (powered shield modules).
    pub shield_cap: f64,
    /// Sum of the armour rating of healthy plating.
    pub armor: f64,
    /// kg of food per day grown by powered greenhouses.
    pub food_per_day: f64,
    pub medbay: f64,
    pub workshop: f64,
    /// Sleepers the powered cryo modules can hold, and their mean reliability and temperature (C),
    /// weighted by capacity (see `cryo`).
    pub cryo_cap: f64,
    pub cryo_rel: f64,
    pub cryo_temp: f64,
}

/// Per-ship dynamic systems state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Systems {
    pub health: Vec<f64>,
    pub powered: Vec<bool>,
    pub battery_mj: f64,
    pub temp_k: f64,
    pub supply_kw: f64,
    pub demand_kw: f64,
    pub overheating: bool,
    /// Current shield charge (damage points) and the time of the last hit.
    pub shield_pts: f64,
    pub last_hit: f64,
    /// The shield generators are switched off at the console (no draw, no protection). Not saved.
    #[serde(skip)]
    pub shield_off: bool,
    /// Extra electrical load (kW) asked by the console this step: the floodlights. Not saved.
    #[serde(skip)]
    pub extra_kw: f64,
}

impl Systems {
    pub fn new(design: &ShipDesign, battery_mj: f64) -> Self {
        let n = design.slots.len();
        Systems {
            health: vec![1.0; n],
            powered: vec![false; n],
            battery_mj,
            temp_k: 290.0,
            supply_kw: 0.0,
            demand_kw: 0.0,
            overheating: false,
            shield_pts: 0.0,
            last_hit: -1.0e9,
            shield_off: false,
            extra_kw: 0.0,
        }
    }

    /// Rated radiating power (kW) of a ship: the hull itself plus its radiator modules. One law for
    /// the live ship, the shipyard report and the infrared signature.
    pub fn rated_radiation_kw(mass_dry: f64, radiate_kw: f64) -> f64 {
        30.0 + 0.004 * mass_dry + radiate_kw
    }

    /// Radiated power (kW) at temperature `t` for a rated `rated_kw`.
    pub fn radiated(rated_kw: f64, t: f64) -> f64 {
        let num = t.powi(4) - T_COLD.powi(4);
        let den = T_REF.powi(4) - T_COLD.powi(4);
        rated_kw * num / den
    }

    /// Advance power bus, heat and overheating damage by `dt`; return what is available.
    /// `solar` is the stellar flux relative to 1 AU at the ship's position.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        data: &GameData,
        design: &ShipDesign,
        stats: &ShipStats,
        mass_total: f64,
        act: &Activity,
        solar: f64,
        dt: f64,
    ) -> Effective {
        let hull = &data.hulls[design.hull];
        let scram = self.temp_k > T_SCRAM;
        let mut supply = 0.0;
        let mut consumers: Vec<(usize, f64, u8)> = Vec::new();
        for (i, m) in design.modules(data) {
            let k = self.health[i];
            if k <= 0.0 {
                self.powered[i] = false;
                continue;
            }
            if m.power > 0.0 {
                let flux = if m.category == "solar" { solar } else { 1.0 };
                if !(scram && m.category == "reactor") {
                    supply += m.power * k * flux;
                }
                self.powered[i] = true;
            } else if m.power < 0.0 {
                let load = match m.category.as_str() {
                    "engine" => act.throttle.clamp(0.0, 1.0),
                    "mining" => f64::from(u8::from(act.mining)),
                    "ftl" => f64::from(u8::from(act.jumping)),
                    "weapon" => f64::from(u8::from(act.firing)),
                    // A shield switched off at the console draws nothing (and protects nothing).
                    "shield" if self.shield_off => 0.0,
                    "gyro" | "rcs" => 0.2 + 0.8 * act.rcs_use.clamp(0.0, 1.0),
                    _ => 1.0,
                };
                if load > 0.0 {
                    consumers.push((i, -m.power * load, m.priority));
                } else {
                    self.powered[i] = true; // idle but ready
                }
            } else {
                self.powered[i] = true; // passive modules
            }
        }
        let mut demand: f64 = consumers.iter().map(|c| c.1).sum::<f64>() + self.extra_kw;
        let batt_avail_kw = self.battery_mj * 1000.0 / dt;
        if demand > supply + batt_avail_kw {
            // Shed lowest-priority consumers first (priority 3 before 0).
            consumers.sort_by(|a, b| b.2.cmp(&a.2).then(b.0.cmp(&a.0)));
            let mut kept: Vec<(usize, f64, u8)> = Vec::new();
            let mut left = demand;
            for c in &consumers {
                if left > supply + batt_avail_kw && c.2 > 0 {
                    self.powered[c.0] = false;
                    left -= c.1;
                } else {
                    kept.push(*c);
                }
            }
            // Critical loads that still do not fit run on whatever is left, unpowered ones off.
            if left > supply + batt_avail_kw {
                for c in &kept {
                    self.powered[c.0] = false;
                }
                left = 0.0;
            } else {
                for c in &kept {
                    self.powered[c.0] = true;
                }
            }
            demand = left;
        } else {
            for c in &consumers {
                self.powered[c.0] = true;
            }
        }
        // Battery: discharge for any deficit, charge from any surplus.
        let net = supply - demand;
        let produced_for_demand = if net >= 0.0 { demand } else { supply };
        if net < 0.0 {
            self.battery_mj = (self.battery_mj + net * dt / 1000.0).max(0.0);
        } else {
            let room_kw = (stats.battery_cap - self.battery_mj).max(0.0) * 1000.0 / dt;
            let charge = net.min(room_kw);
            self.battery_mj += charge * dt / 1000.0 * crate::colony::BATTERY_EFFICIENCY;
        }
        self.supply_kw = supply;
        self.demand_kw = demand;

        // Heat: reactors dump a fraction of what they produce; every consumed kW ends up as heat.
        let reactor_out = produced_for_demand.min(supply);
        let heat_in = reactor_out * REACTOR_WASTE + demand + act.ext_heat_kw;
        let rated = Self::rated_radiation_kw(stats.mass_dry, stats.radiate_kw);
        let c_kj_per_k = (mass_total * 0.5).max(1.0);
        // Sub-step the explicit integration: the thermal time constant can be ~100 s.
        let n = (dt / 5.0).ceil().max(1.0);
        let h = dt / n;
        for _ in 0..(n as usize) {
            let rad = Self::radiated(rated, self.temp_k);
            self.temp_k = (self.temp_k + (heat_in - rad) * h / c_kj_per_k).max(T_COLD);
        }
        self.overheating = self.temp_k > T_DAMAGE;
        if self.overheating {
            let rate = 0.004 * (self.temp_k - T_DAMAGE) / 100.0 * dt;
            for (i, _) in design.modules(data) {
                self.health[i] = (self.health[i] - rate).max(0.0);
            }
        }

        // Effective capabilities.
        let mut e = Effective {
            rcs_n: hull.rcs,
            max_rate: hull.max_rate,
            ang_accel: hull.ang_accel,
            ..Default::default()
        };
        let (mut thrust_isp, mut agility, mut life) = (0.0, 0.0, 0.0);
        for (i, m) in design.modules(data) {
            if !self.powered[i] {
                continue;
            }
            let k = self.health[i];
            e.thrust += m.thrust * k;
            thrust_isp += m.thrust * k * m.isp;
            e.rcs_n += m.rcs * k;
            agility += m.agility * k;
            e.mining_rate += m.mining_rate * k;
            e.mining_range = f64::max(e.mining_range, m.mining_range);
            life += m.life_support * k;
            // Greenhouses live on the light of the star; hydroponics and tanks on current.
            let light = if m.category == "greenhouse" {
                solar.sqrt().clamp(0.05, 1.0)
            } else {
                1.0
            };
            e.food_per_day += m.food * k * light;
            e.medbay += m.medical * k;
            if m.cryo > 0.0 {
                let c = m.cryo * k;
                e.cryo_cap += c;
                e.cryo_rel += c * m.reliability * k.sqrt();
                e.cryo_temp += c * m.cryo_temp;
            }
            e.workshop += m.repair * k;
            if !self.shield_off {
                e.shield_cap += m.shield * k * SHIELD_PTS_PER_MJ;
            }
            e.armor += m.armor * k;
            e.scan_range = f64::max(e.scan_range, m.scan_range * k);
            if k > 0.5 {
                e.jump_range = f64::max(e.jump_range, m.jump_range);
            }
        }
        if e.cryo_cap > 0.0 {
            e.cryo_rel /= e.cryo_cap;
            e.cryo_temp /= e.cryo_cap;
        }
        e.isp = if e.thrust > 0.0 {
            thrust_isp / e.thrust
        } else {
            0.0
        };
        e.max_rate *= 1.0 + agility;
        e.ang_accel *= 1.0 + agility;
        e.life_ok = life > 0.0;
        e.life_capacity = life;
        e
    }

    /// Spread `amount` of damage over installed modules, starting from the one chosen by `pick`.
    pub fn damage_modules(&mut self, data: &GameData, design: &ShipDesign, amount: f64, pick: u64) {
        let ids: Vec<usize> = design.modules(data).map(|(i, _)| i).collect();
        if ids.is_empty() {
            return;
        }
        let first = ids[(pick % ids.len() as u64) as usize];
        let m = &data.modules[design.slots[first].unwrap_or(0)];
        self.health[first] = (self.health[first] - amount / m.hp).max(0.0);
        // A fifth of the energy rattles a neighbour.
        let second = ids[((pick >> 17) % ids.len() as u64) as usize];
        let m2 = &data.modules[design.slots[second].unwrap_or(0)];
        self.health[second] = (self.health[second] - amount * 0.2 / m2.hp).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameData {
        crate::test_support::load()
    }

    fn setup(d: &GameData) -> (ShipDesign, ShipStats, Systems) {
        let design = ShipDesign::starter(d);
        let sys = Systems::new(&design, 0.0);
        let stats = ShipStats::compute(d, &design, &sys.health);
        (design, stats, sys)
    }

    #[test]
    fn starter_ship_matches_design_targets() {
        let d = data();
        let (design, st, _) = setup(&d);
        assert!(design.slots.iter().filter(|s| s.is_some()).count() >= 9);
        assert!((st.fuel_cap - 8000.0).abs() < 1.0 && (st.cargo_kg - 30000.0).abs() < 1.0);
        assert!(
            (st.thrust - 1.4e6).abs() < 1.0 && st.mass_dry > 15_000.0 && st.mass_dry < 25_000.0,
            "{}",
            st.mass_dry
        );
        assert!(st.power_capacity >= 15_000.0);
    }

    #[test]
    fn slot_rules_are_enforced() {
        let d = data();
        let mut des = ShipDesign::empty(&d, d.hull_index("shuttle").expect("hull"));
        let big = d.module_index("fusion_mk4").expect("m");
        assert!(des.install(&d, 0, big).is_err(), "size 4 into size 1 slot");
        let tank = d.module_index("tank_mk1").expect("m");
        assert!(
            des.install(&d, 0, tank).is_err(),
            "internal module in core slot"
        );
        assert!(des.install(&d, 2, tank).is_ok());
        assert!(des.install(&d, 2, tank).is_err(), "slot already used");
        assert_eq!(des.remove(2), Some(tank));
    }

    #[test]
    fn heat_reaches_equilibrium_and_radiators_matter() {
        let d = data();
        let (design, stats, mut sys) = setup(&d);
        sys.battery_mj = stats.battery_cap;
        let act = Activity {
            throttle: 1.0,
            mining: true,
            rcs_use: 0.0,
            jumping: false,
            firing: false,
            ext_heat_kw: 0.0,
        };
        for _ in 0..20_000 {
            sys.step(&d, &design, &stats, 30_000.0, &act, 1.0, 0.5);
        }
        assert!(
            sys.temp_k > 330.0 && sys.temp_k < T_DAMAGE,
            "{}",
            sys.temp_k
        );
        // Strip the radiators: the same load now cooks the ship.
        let mut bare = design.clone();
        for s in 0..bare.slots.len() {
            if bare.slots[s].is_some_and(|m| d.modules[m].category == "radiator") {
                bare.remove(s);
            }
        }
        let st2 = ShipStats::compute(&d, &bare, &sys.health);
        let mut hot = Systems::new(&bare, st2.battery_cap);
        let mut peak: f64 = 0.0;
        for _ in 0..20_000 {
            hot.step(&d, &bare, &st2, 30_000.0, &act, 1.0, 0.5);
            peak = peak.max(hot.temp_k);
        }
        assert!(peak > sys.temp_k + 100.0, "peak {peak}");
        assert!(
            hot.health.iter().any(|&h| h < 1.0),
            "overheating must hurt modules"
        );
    }

    #[test]
    fn greenhouses_follow_the_light_of_the_star_and_hydroponics_do_not() {
        let d = data();
        let food = |module: &str, solar: f64| {
            let mut des = ShipDesign::empty(&d, d.hull_index("prospector").expect("hull"));
            for id in ["fission_mk1", module, "lifesup_mk1"] {
                des.install_first_fit(&d, id).expect("fit");
            }
            let mut sys = Systems::new(&des, 0.0);
            let stats = ShipStats::compute(&d, &des, &sys.health);
            let act = Activity {
                throttle: 0.0,
                mining: false,
                rcs_use: 0.0,
                jumping: false,
                firing: false,
                ext_heat_kw: 0.0,
            };
            sys.step(&d, &des, &stats, 30_000.0, &act, solar, 0.1)
                .food_per_day
        };
        let (near, far) = (food("greenhouse_mk1", 1.0), food("greenhouse_mk1", 0.04));
        assert!(near > 0.0 && far < near * 0.3, "{near} {far}");
        let (hn, hf) = (food("hydroponics_mk1", 1.0), food("hydroponics_mk1", 0.04));
        assert!(hn > 0.0 && (hn - hf).abs() < 1e-9, "{hn} {hf}");
    }

    #[test]
    fn batteries_lose_a_tenth_of_what_they_are_charged_with_like_the_colony_banks() {
        let d = data();
        let (design, stats, mut sys) = setup(&d);
        sys.battery_mj = 0.0;
        let act = Activity {
            throttle: 0.0,
            mining: false,
            rcs_use: 0.0,
            jumping: false,
            firing: false,
            ext_heat_kw: 0.0,
        };
        sys.step(&d, &design, &stats, 30_000.0, &act, 1.0, 1.0);
        let surplus = sys.supply_kw - sys.demand_kw;
        assert!(surplus > 0.0, "{surplus}");
        let want = surplus / 1000.0 * crate::colony::BATTERY_EFFICIENCY;
        assert!(
            (sys.battery_mj - want.min(stats.battery_cap)).abs() < 1e-6,
            "{} vs {want}",
            sys.battery_mj
        );
    }

    #[test]
    fn power_shedding_cuts_low_priority_first() {
        let d = data();
        let design = {
            let mut des = ShipDesign::empty(&d, d.hull_index("prospector").expect("hull"));
            for id in ["fission_mk1", "torch_mk2", "laser_mk2", "lifesup_mk1"] {
                des.install_first_fit(&d, id).expect("fit");
            }
            des
        };
        let mut sys = Systems::new(&design, 0.0);
        let stats = ShipStats::compute(&d, &design, &sys.health);
        // 2 MW supply; torch wants 6 MW, laser 3 MW, life support 90 kW.
        let act = Activity {
            throttle: 1.0,
            mining: true,
            rcs_use: 0.0,
            jumping: false,
            firing: false,
            ext_heat_kw: 0.0,
        };
        let eff = sys.step(&d, &design, &stats, 30_000.0, &act, 1.0, 0.1);
        let cat = |c: &str| {
            design
                .modules(&d)
                .find(|(_, m)| m.category == c)
                .map(|(i, _)| i)
                .expect("module")
        };
        assert!(
            sys.powered[cat("life_support")],
            "critical load stays powered"
        );
        assert!(!sys.powered[cat("mining")], "lowest priority is shed first");
        assert!(
            !sys.powered[cat("engine")] && eff.thrust == 0.0,
            "engine cannot run on 2 MW"
        );
        assert!(eff.life_ok);
    }

    #[test]
    fn battery_bridges_a_deficit_then_runs_dry() {
        let d = data();
        let (design, stats, mut sys) = setup(&d);
        sys.battery_mj = stats.battery_cap;
        // Kill the reactor: only the battery remains.
        for (i, m) in design.modules(&d) {
            if m.category == "reactor" {
                sys.health[i] = 0.0;
            }
        }
        let act = Activity {
            throttle: 0.0,
            ..Default::default()
        };
        let b0 = sys.battery_mj;
        sys.step(&d, &design, &stats, 30_000.0, &act, 1.0, 1.0);
        assert!(sys.battery_mj < b0, "battery discharges");
        for _ in 0..100_000 {
            sys.step(&d, &design, &stats, 30_000.0, &act, 1.0, 1.0);
        }
        assert!(sys.battery_mj < 0.1);
        let eff = sys.step(&d, &design, &stats, 30_000.0, &act, 1.0, 1.0);
        assert!(!eff.life_ok, "no power, no life support");
    }

    #[test]
    fn damage_reduces_module_capability() {
        let d = data();
        let (design, mut stats, mut sys) = setup(&d);
        let before = stats.thrust;
        for (i, m) in design.modules(&d) {
            if m.category == "engine" {
                sys.health[i] = 0.4;
            }
        }
        stats = ShipStats::compute(&d, &design, &sys.health);
        assert!((stats.thrust - before * 0.4).abs() < 1.0);
        sys.damage_modules(&d, &design, 200.0, 12345);
        assert!(sys.health.iter().any(|&h| h < 1.0));
    }
}
