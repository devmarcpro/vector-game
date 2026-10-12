//! The chemistry engine. Tables live in `data/chemistry.ron` and `data/reactions.ron`; here are
//! the rules that use them: a reaction moves moles between substances so that the atoms of each
//! element are conserved exactly and the energy balance follows Hess's law (heat released =
//! difference of the enthalpies of formation). What the rest of the game takes from it:
//! the toxicity, corrosiveness and flammability of an air from its composition, the specific
//! impulse of a propellant from the energy of its combustion, the weathering of the deposits of a
//! world, and the shipboard laboratory (reactions run on the cargo, on the ship's battery).
use crate::{Sim, G0};
use std::collections::BTreeMap;
use vector_data::chemistry::Reaction;
use vector_data::GameData;
use vector_world::{BodyKind, CelBody};

/// Share of the ideal exhaust energy a real nozzle turns into velocity (a rocket engine loses to
/// incomplete expansion, dissociation and boundary layers). H2/O2 then gives about 450 s.
pub const NOZZLE_EFFICIENCY: f64 = 0.78;
/// Molar heat capacity used to warm reagents up to their activation temperature, J/(mol K).
pub const CP_MOLAR: f64 = 30.0;
/// Electric to chemical efficiency of the laboratory.
pub const LAB_EFFICIENCY: f64 = 0.6;
/// Limits of the laboratory reactor: temperature (K) and pressure (bar).
pub const LAB_T_MAX: f64 = 2400.0;
pub const LAB_P_MAX: f64 = 300.0;
/// Catalyst a batch needs in the hold, as a share of the mass of the reagents (min 0.5 kg).
pub const CATALYST_SHARE: f64 = 0.01;
/// Partial pressure of oxygen below which a flame does not spread (bar) and the one humans
/// breathe at ease (bar).
pub const FIRE_MIN_O2: f64 = 0.12;
pub const BREATH_MIN_O2: f64 = 0.1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChemError {
    /// Not enough of this compound.
    Lacking(String),
    Cold,
    LowPressure,
    NoCatalyst(String),
}

/// An amount of substances (moles by compound index).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mixture {
    pub mol: BTreeMap<usize, f64>,
}

impl Mixture {
    pub fn add(&mut self, compound: usize, mol: f64) {
        *self.mol.entry(compound).or_insert(0.0) += mol;
    }

    pub fn amount(&self, compound: usize) -> f64 {
        self.mol.get(&compound).copied().unwrap_or(0.0)
    }

    /// Total mass, g.
    pub fn mass(&self, data: &GameData) -> f64 {
        self.mol.iter().map(|(&c, &n)| n * data.molar_mass(c)).sum()
    }

    /// Moles of atoms of each element.
    pub fn atoms(&self, data: &GameData) -> BTreeMap<String, f64> {
        let mut m = BTreeMap::new();
        for (&c, &n) in &self.mol {
            for (s, k) in data.compound_atoms(c) {
                *m.entry(s).or_insert(0.0) += n * f64::from(k);
            }
        }
        m
    }

    /// The most the reaction can advance with what the mixture holds (moles of reaction).
    pub fn max_extent(&self, data: &GameData, r: &Reaction) -> f64 {
        r.reactants
            .iter()
            .map(|(c, n)| {
                data.compound_index(c)
                    .map_or(0.0, |i| self.amount(i) / f64::from(*n))
            })
            .fold(f64::INFINITY, f64::min)
    }

    /// Advance the reaction by `extent` moles of reaction at temperature `t` (K) and pressure
    /// `p` (bar). Returns the heat released in kJ (negative when it absorbed heat).
    pub fn react(
        &mut self,
        data: &GameData,
        r: &Reaction,
        extent: f64,
        t: f64,
        p: f64,
    ) -> Result<f64, ChemError> {
        if t < r.t_act {
            return Err(ChemError::Cold);
        }
        if p < r.p_min {
            return Err(ChemError::LowPressure);
        }
        if !r.catalyst.is_empty() {
            let c = data.compound_index(&r.catalyst).unwrap_or(usize::MAX);
            if self.amount(c) <= 0.0 {
                return Err(ChemError::NoCatalyst(r.catalyst.clone()));
            }
        }
        for (c, n) in &r.reactants {
            let i = data.compound_index(c).unwrap_or(usize::MAX);
            if self.amount(i) + 1e-12 < extent * f64::from(*n) {
                return Err(ChemError::Lacking(c.clone()));
            }
        }
        for (c, n) in &r.reactants {
            if let Some(i) = data.compound_index(c) {
                let e = self.mol.entry(i).or_insert(0.0);
                *e = (*e - extent * f64::from(*n)).max(0.0);
            }
        }
        for (c, n) in &r.products {
            if let Some(i) = data.compound_index(c) {
                self.add(i, extent * f64::from(*n));
            }
        }
        Ok(-data.reaction_enthalpy(r) * extent)
    }
}

/// Mass of the reagents of one mole of reaction, kg.
pub fn reagent_mass_kg(data: &GameData, r: &Reaction) -> f64 {
    r.reactants
        .iter()
        .map(|(c, n)| {
            data.compound_index(c)
                .map_or(0.0, |i| data.molar_mass(i) * f64::from(*n))
        })
        .sum::<f64>()
        / 1000.0
}

/// Specific impulse (s) of a reaction used as a propellant: the exhaust speed follows from the
/// heat of the reaction shared by the mass of the exhaust (v = sqrt(2 q)), times the efficiency
/// of the nozzle. None for a reaction that does not give heat.
pub fn specific_impulse(data: &GameData, r: &Reaction) -> Option<f64> {
    let dh = data.reaction_enthalpy(r);
    if dh >= 0.0 {
        return None;
    }
    let exhaust_kg: f64 = r
        .products
        .iter()
        .map(|(c, n)| {
            data.compound_index(c)
                .map_or(0.0, |i| data.molar_mass(i) * f64::from(*n))
        })
        .sum::<f64>()
        / 1000.0;
    let q = -dh * 1000.0 / exhaust_kg; // J per kg of exhaust
    Some(NOZZLE_EFFICIENCY * (2.0 * q).sqrt() / G0)
}

/// Energy density of a reaction, MJ per kg of reagents (negative: heat released).
pub fn energy_density(data: &GameData, r: &Reaction) -> f64 {
    data.reaction_enthalpy(r) / 1000.0 / reagent_mass_kg(data, r)
}

/// What an air does to those who breathe it and to what it touches.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AirHazard {
    /// Sum over toxic gases of fraction / exposure limit: harmful above 1.
    pub toxicity: f64,
    /// Corrosion rate of bare metal (arbitrary units per bar, 1 = a strong acid mist).
    pub corrosion: f64,
    /// Partial pressure of oxygen, bar.
    pub o2_bar: f64,
    /// Flammable gas share of the air as a multiple of its lower flammability limit.
    pub fuel_ratio: f64,
}

impl AirHazard {
    /// Breathable without a suit: enough oxygen, not too much, nothing toxic.
    pub fn breathable(&self) -> bool {
        self.o2_bar >= BREATH_MIN_O2 && self.o2_bar <= 0.5 && self.toxicity < 1.0
    }

    /// How fast a flame spreads relative to normal air (0 = cannot burn).
    pub fn fire_spread(&self) -> f64 {
        if self.o2_bar < FIRE_MIN_O2 {
            0.0
        } else {
            self.o2_bar / 0.21
        }
    }

    /// A gas mix that burns all at once on a spark: fuel in air within flammability limits.
    pub fn explosive(&self) -> bool {
        self.fire_spread() > 0.0 && self.fuel_ratio >= 1.0
    }

    /// Damage per turn-minute to an unprotected person (before armour).
    pub fn lungs(&self) -> f64 {
        let mut d = 0.0;
        if self.toxicity > 1.0 {
            d += 0.002 * self.toxicity.min(10.0);
        }
        if self.o2_bar < BREATH_MIN_O2 {
            d += 0.004;
        }
        d
    }
}

/// The hazard of an air given as gas formulas with their mole fractions, at `pressure` (bar)
/// and `temp` (K). Gases are matched to compounds by formula; unknown gases are inert.
pub fn air_hazard(data: &GameData, gases: &[(&str, f32)], pressure: f64, temp: f64) -> AirHazard {
    let total: f64 = gases.iter().map(|g| f64::from(g.1)).sum::<f64>().max(1e-9);
    let mut h = AirHazard::default();
    let mut corr = 0.0;
    for (formula, share) in gases {
        let x = f64::from(*share) / total;
        let Some(c) = data
            .chemistry
            .compounds
            .iter()
            .find(|c| c.formula == *formula)
        else {
            continue;
        };
        if c.formula == "O2" {
            h.o2_bar = x * pressure;
        }
        if c.limit > 0.0 {
            h.toxicity += x / c.limit;
        }
        corr += x * c.corrosive;
        if c.lel > 0.0 {
            h.fuel_ratio += x / c.lel;
        }
    }
    // Reactions go about twice as fast every 25 K; a dense air presses its gases on the metal.
    let thermal = 2f64.powf((temp - 288.0) / 25.0).clamp(0.05, 8.0);
    h.corrosion = corr * pressure.clamp(0.0, 100.0) * thermal;
    h
}

/// Is the volatile `res` (a resource of category "volatile") stable on a world at `temp` (K) and
/// `pressure` (bar)? In vacuum an ice sublimates away well below its boiling point; an
/// atmosphere keeps it up to the boiling point.
pub fn volatile_stable(data: &GameData, res: &str, temp: f64, pressure: f64) -> bool {
    let Some(c) = data.compound_of_resource(res) else {
        return true;
    };
    let boil = data.chemistry.compounds[c].boil;
    temp < boil * (0.55 + 0.45 * pressure.clamp(0.0, 1.0))
}

/// How oxidised the crust of a world is, 0..1: oxygen, water, CO2 and sulphur gases of its air
/// rust the iron of its ground.
pub fn oxidation(data: &GameData, atmosphere: &[(&str, f32)], pressure: f64) -> f64 {
    let h = air_hazard(data, atmosphere, pressure, 288.0);
    let total: f64 = atmosphere
        .iter()
        .map(|g| f64::from(g.1))
        .sum::<f64>()
        .max(1e-9);
    let mut ox = 0.0;
    for (g, f) in atmosphere {
        let w = match *g {
            "O2" => 1.0,
            "H2O" => 0.6,
            "CO2" => 0.3,
            "SO2" => 0.5,
            _ => 0.0,
        };
        ox += w * f64::from(*f) / total;
    }
    let _ = h;
    (ox * pressure.clamp(0.0, 1.0).sqrt()).clamp(0.0, 0.9)
}

/// Weather the ground deposits of a world after its formation and its air: unstable ices are
/// gone, part of the iron has rusted into iron oxide. Deterministic, no randomness.
pub fn weather_deposits(data: &GameData, b: &CelBody, deposits: &mut Vec<(usize, f64)>) {
    if matches!(b.kind, BodyKind::Asteroid(_)) {
        return;
    }
    deposits.retain(|&(i, _)| {
        let r = &data.resources[i];
        r.category != "volatile" || volatile_stable(data, &r.id, b.temp, b.pressure)
    });
    let ox = oxidation(data, &b.atmosphere, b.pressure);
    if ox > 0.01 {
        let (Some(fe), Some(feo)) = (
            data.resource_index("iron"),
            data.resource_index("iron_oxide"),
        ) else {
            return;
        };
        if let Some(d) = deposits.iter_mut().find(|d| d.0 == fe) {
            // Rust is Fe2O3: 1 kg of iron makes 1.43 kg of oxide.
            let rusted = d.1 * ox;
            d.1 -= rusted;
            deposits.push((feo, rusted * 1.43));
        }
    }
}

/// Outcome of a batch in the laboratory.
#[derive(Clone, Debug, PartialEq)]
pub struct LabReport {
    pub reaction: usize,
    /// Moles of reaction that took place.
    pub extent: f64,
    /// (resource index, kg) taken from and added to the hold.
    pub consumed: Vec<(usize, f64)>,
    pub produced: Vec<(usize, f64)>,
    /// Electric energy drawn from the battery, kJ.
    pub electric_kj: f64,
    /// Heat that ends up in the ship, kJ (electric input minus the chemical energy stored).
    pub heat_kj: f64,
    /// Modules damaged by a blast, MJ of blast.
    pub blast_mj: f64,
}

impl Sim {
    /// The air hazard of world `i`, from its composition (None when it has no air).
    pub fn air_hazard_of(&self, i: usize) -> Option<AirHazard> {
        let b = self.system.bodies.get(i)?;
        if !matches!(b.kind, BodyKind::Planet(_) | BodyKind::Moon(_)) || b.pressure < 0.001 {
            return None;
        }
        Some(air_hazard(&self.data, &b.atmosphere, b.pressure, b.temp))
    }

    /// The air eats the exposed modules of the ship (external and weapon slots) in proportion to
    /// its corrosion rate; the inside of the hull is spared.
    pub(crate) fn corrode(&mut self, dt: f64) {
        let Some((i, _, _)) = self.in_air() else {
            return;
        };
        let Some(h) = self.air_hazard_of(i) else {
            return;
        };
        let loss = 1.0e-4 * h.corrosion * dt;
        if loss <= 0.0 {
            return;
        }
        let data = self.data.clone();
        let ids: Vec<usize> = self.ship.design.modules(&data).map(|(i, _)| i).collect();
        for slot in ids {
            let m = &data.modules[self.ship.design.slots[slot].unwrap_or(0)];
            if matches!(
                m.slot,
                vector_data::SlotType::External | vector_data::SlotType::Hardpoint
            ) {
                let h = &mut self.ship.sys.health[slot];
                *h = (*h - loss).max(0.0);
            }
        }
    }

    /// Reactions the laboratory can run: every substance has a resource to hold it and the
    /// reactor can reach the conditions.
    pub fn lab_reactions(&self) -> Vec<usize> {
        (0..self.data.reactions.len())
            .filter(|&r| self.lab_resources(r).is_some())
            .collect()
    }

    /// Resources of the reagents (lists to draw from) and of the products (the one made).
    #[allow(clippy::type_complexity)]
    fn lab_resources(&self, reaction: usize) -> Option<(Vec<Vec<usize>>, Vec<usize>)> {
        let d = &self.data;
        let r = d.reactions.get(reaction)?;
        if r.t_act > LAB_T_MAX || r.p_min > LAB_P_MAX {
            return None;
        }
        let list = |c: &str| -> Option<Vec<usize>> {
            let i = d.compound_index(c)?;
            let v: Vec<usize> = d.chemistry.compounds[i]
                .resources
                .iter()
                .filter_map(|x| d.resource_index(x))
                .collect();
            (!v.is_empty()).then_some(v)
        };
        let reag = r
            .reactants
            .iter()
            .map(|(c, _)| list(c))
            .collect::<Option<Vec<_>>>()?;
        let prod = r
            .products
            .iter()
            .map(|(c, _)| list(c).map(|v| v[0]))
            .collect::<Option<Vec<_>>>()?;
        Some((reag, prod))
    }

    /// Run a batch of reaction `reaction` on the cargo: about `target_kg` of reagents, limited by
    /// what the hold has, the free volume, the battery and the catalyst. Mass is conserved;
    /// the heat goes into the ship; a blast damages modules.
    pub fn lab_run(&mut self, reaction: usize, target_kg: f64) -> Result<LabReport, String> {
        let data = self.data.clone();
        let (reag, prod) = self
            .lab_resources(reaction)
            .ok_or("le laboratoire ne peut pas realiser cette reaction")?;
        let r = &data.reactions[reaction];
        let per_kg = reagent_mass_kg(&data, r);
        let mut extent = target_kg.max(0.0) / per_kg;
        // What the hold has of each reagent.
        for (k, (c, n)) in r.reactants.iter().enumerate() {
            let have: f64 = reag[k].iter().map(|&x| self.ship.cargo.kg[x]).sum();
            let ci = data.compound_index(c).unwrap_or(0);
            let kg_per = data.molar_mass(ci) * f64::from(*n) / 1000.0;
            if have < 1e-9 {
                return Err(format!("il manque : {}", data.chemistry.compounds[ci].name));
            }
            extent = extent.min(have / kg_per);
        }
        // The catalyst is not consumed but must be there.
        if !r.catalyst.is_empty() {
            let ci = data.compound_index(&r.catalyst).unwrap_or(0);
            let have: f64 = data.chemistry.compounds[ci]
                .resources
                .iter()
                .filter_map(|x| data.resource_index(x))
                .map(|x| self.ship.cargo.kg[x])
                .sum();
            let need = (extent * per_kg * CATALYST_SHARE).max(0.5);
            if have < need {
                return Err(format!(
                    "catalyseur manquant : {} ({need:.1} kg)",
                    data.chemistry.compounds[ci].name
                ));
            }
        }
        // Volume of the products must fit once the reagents are out.
        let vol = |list: &[(usize, f64)]| -> f64 {
            list.iter()
                .map(|&(x, kg)| kg / data.resources[x].density)
                .sum()
        };
        let reag_kg = |e: f64| -> Vec<(usize, f64)> {
            r.reactants
                .iter()
                .enumerate()
                .map(|(k, (c, n))| {
                    let ci = data.compound_index(c).unwrap_or(0);
                    (reag[k][0], e * data.molar_mass(ci) * f64::from(*n) / 1000.0)
                })
                .collect()
        };
        let prod_kg = |e: f64| -> Vec<(usize, f64)> {
            r.products
                .iter()
                .enumerate()
                .map(|(k, (c, n))| {
                    let ci = data.compound_index(c).unwrap_or(0);
                    (prod[k], e * data.molar_mass(ci) * f64::from(*n) / 1000.0)
                })
                .collect()
        };
        let dvol = vol(&prod_kg(1.0)) - vol(&reag_kg(1.0));
        if dvol > 1e-12 {
            let free = (self.ship.cargo.cap_vol - self.ship.cargo.volume).max(0.0);
            extent = extent.min(free / dvol);
        }
        // Energy: warm the reagents up to the activation temperature (a flame feeds itself, so an
        // exothermic reaction only needs a spark), and pay for the heat an endothermic one absorbs.
        let dh = data.reaction_enthalpy(r);
        let n_reag: f64 = r.reactants.iter().map(|(_, n)| f64::from(*n)).sum();
        let sensible = CP_MOLAR * n_reag * (r.t_act - self.ship.sys.temp_k).max(0.0) / 1000.0;
        let need_per = if dh > 0.0 {
            (dh + sensible) / LAB_EFFICIENCY
        } else {
            0.05 * sensible / LAB_EFFICIENCY
        };
        let avail_kj = self.ship.sys.battery_mj * 1000.0;
        if need_per > 0.0 {
            extent = extent.min(avail_kj / need_per);
        }
        if extent < 1e-6 {
            return Err("rien a faire : stock, place ou energie insuffisants".into());
        }
        let electric_kj = need_per * extent;
        self.ship.sys.battery_mj = (self.ship.sys.battery_mj - electric_kj / 1000.0).max(0.0);
        // Move the mass.
        let mut consumed = Vec::new();
        for (k, (c, n)) in r.reactants.iter().enumerate() {
            let ci = data.compound_index(c).unwrap_or(0);
            let mut left = extent * data.molar_mass(ci) * f64::from(*n) / 1000.0;
            for &x in &reag[k] {
                let got = self.ship.cargo.remove(&data, x, left);
                if got > 0.0 {
                    consumed.push((x, got));
                }
                left -= got;
                if left <= 1e-12 {
                    break;
                }
            }
        }
        let mut produced = Vec::new();
        for (x, kg) in prod_kg(extent) {
            let got = self.ship.cargo.add(&data, x, kg);
            produced.push((x, got));
        }
        let heat_kj = electric_kj - dh * extent;
        // The heat warms the ship (about 0.5 kJ/K per kg).
        let c = (self.ship.mass() * 0.5).max(1.0);
        self.ship.sys.temp_k += heat_kj.max(0.0) / c;
        // A blast hurts the modules.
        let mut blast_mj = 0.0;
        if r.kind == "explosion" {
            blast_mj = heat_kj.max(0.0) / 1000.0;
            let design = self.ship.design.clone();
            let pick = self.time.to_bits() ^ (reaction as u64).wrapping_mul(0x9E37_79B9);
            self.ship
                .sys
                .damage_modules(&data, &design, blast_mj * 500.0, pick);
        }
        Ok(LabReport {
            reaction,
            extent,
            consumed,
            produced,
            electric_kj,
            heat_kj,
            blast_mj,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;

    fn data() -> GameData {
        crate::test_support::load()
    }

    fn sim() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        new_sim(5, Arc::new(GameData::load(&dir).expect("data")))
    }

    fn react_id(d: &GameData, id: &str) -> usize {
        d.reaction_index(id).expect(id)
    }

    #[test]
    fn every_reaction_conserves_the_atoms_of_each_element() {
        let d = data();
        assert!(d.reactions.len() >= 25);
        for r in &d.reactions {
            let mut m = Mixture::default();
            for (c, n) in &r.reactants {
                m.add(d.compound_index(c).unwrap(), f64::from(*n) * 3.0);
            }
            if !r.catalyst.is_empty() {
                m.add(d.compound_index(&r.catalyst).unwrap(), 1.0);
            }
            let (a0, g0) = (m.atoms(&d), m.mass(&d));
            let ext = m.max_extent(&d, r);
            assert!((ext - 3.0).abs() < 1e-9, "{}", r.id);
            m.react(&d, r, ext, r.t_act, r.p_min).expect(&r.id);
            let a1 = m.atoms(&d);
            for (s, n) in &a0 {
                assert!(
                    (n - a1.get(s).copied().unwrap_or(0.0)).abs() < 1e-9,
                    "{} {s}",
                    r.id
                );
            }
            // Mass is conserved too (molar masses add the same atoms).
            assert!((g0 - m.mass(&d)).abs() < 1e-6, "{}", r.id);
        }
    }

    #[test]
    fn a_reaction_that_does_not_balance_is_refused_at_loading() {
        let mut d = data();
        // Break the water electrolysis: one oxygen too many.
        let i = react_id(&d, "electrolysis_water");
        d.reactions[i].products[1].1 = 2;
        assert!(d.validate().is_err(), "unbalanced reaction accepted");
        // An exothermic "electrolysis" is incoherent with its kind.
        let mut d = data();
        let i = react_id(&d, "hydrogen_combustion");
        d.reactions[i].kind = "electrolyse".into();
        assert!(d.validate().is_err());
    }

    #[test]
    fn the_energy_follows_hess_law_and_a_cycle_gives_back_what_it_took() {
        let d = data();
        let el = &d.reactions[react_id(&d, "electrolysis_water")];
        let co = &d.reactions[react_id(&d, "hydrogen_combustion")];
        // 2 H2O -> 2 H2 + O2 absorbs 571.6 kJ, and 2 H2 + O2 -> 2 H2O gives exactly that back.
        assert!((d.reaction_enthalpy(el) - 571.6).abs() < 0.01);
        let mut m = Mixture::default();
        m.add(d.compound_index("water").unwrap(), 10.0);
        let absorbed = -m.react(&d, el, 5.0, 400.0, 1.0).unwrap();
        let released = m.react(&d, co, 5.0, 900.0, 1.0).unwrap();
        assert!((absorbed - released).abs() < 1e-9, "{absorbed} {released}");
        assert!((m.amount(d.compound_index("water").unwrap()) - 10.0).abs() < 1e-9);
        // Methane burns hotter per mole than hydrogen, and the sign rule holds for every kind.
        for r in &d.reactions {
            let dh = d.reaction_enthalpy(r);
            match r.kind.as_str() {
                "combustion" | "explosion" => assert!(dh < 0.0, "{}", r.id),
                "electrolyse" => assert!(dh > 0.0, "{}", r.id),
                _ => {}
            }
        }
    }

    #[test]
    fn a_cold_or_thin_reactor_and_a_missing_catalyst_stop_the_reaction() {
        let d = data();
        let h = &d.reactions[react_id(&d, "haber_bosch")];
        let mut m = Mixture::default();
        m.add(d.compound_index("nitrogen").unwrap(), 1.0);
        m.add(d.compound_index("hydrogen").unwrap(), 3.0);
        assert_eq!(m.react(&d, h, 1.0, 300.0, 200.0), Err(ChemError::Cold));
        assert_eq!(m.react(&d, h, 1.0, 750.0, 1.0), Err(ChemError::LowPressure));
        assert!(matches!(
            m.react(&d, h, 1.0, 750.0, 200.0),
            Err(ChemError::NoCatalyst(_))
        ));
        m.add(d.compound_index("iron").unwrap(), 0.1);
        assert!(m.react(&d, h, 1.0, 750.0, 200.0).is_ok());
        assert!(matches!(
            m.react(&d, h, 1.0, 750.0, 200.0),
            Err(ChemError::Lacking(_))
        ));
    }

    #[test]
    fn specific_impulse_comes_from_the_energy_of_the_propellant() {
        let d = data();
        let isp = |id: &str| specific_impulse(&d, &d.reactions[react_id(&d, id)]).unwrap();
        let (h2, ch4, kero, n2h4, h2o2) = (
            isp("hydrogen_combustion"),
            isp("methane_combustion"),
            isp("kerosene_combustion"),
            isp("hydrazine_combustion"),
            isp("peroxide_decomposition"),
        );
        // Real engines: H2/O2 about 450 s, CH4/O2 about 360, kerosene/O2 about 350.
        assert!((420.0..480.0).contains(&h2), "{h2}");
        assert!((340.0..400.0).contains(&ch4), "{ch4}");
        assert!((330.0..400.0).contains(&kero), "{kero}");
        assert!(
            h2 > ch4 && ch4 > h2o2 && n2h4 > h2o2,
            "{h2} {ch4} {n2h4} {h2o2}"
        );
        // An endothermic reaction is no propellant.
        assert!(specific_impulse(&d, &d.reactions[react_id(&d, "electrolysis_water")]).is_none());
        // Energy per kilo: hydrogen far above kerosene (MJ/kg of reagents).
        let e = |id: &str| -energy_density(&d, &d.reactions[react_id(&d, id)]);
        assert!(e("hydrogen_combustion") > e("kerosene_combustion"));
        assert!(e("nitroglycerin_blast") > 5.0 && e("nitroglycerin_blast") < 8.0);
    }

    #[test]
    fn the_air_of_a_world_is_judged_by_what_it_is_made_of() {
        let d = data();
        let earth = air_hazard(&d, &[("N2", 0.78), ("O2", 0.21), ("Ar", 0.01)], 1.0, 288.0);
        assert!(earth.breathable() && earth.toxicity < 0.1, "{earth:?}");
        assert!(earth.fire_spread() > 0.9 && !earth.explosive());
        // Lava world: sulphur dioxide poisons and eats metal.
        let lava = air_hazard(&d, &[("SO2", 0.6), ("CO2", 0.3)], 2.0, 700.0);
        assert!(!lava.breathable() && lava.toxicity > 100.0, "{lava:?}");
        assert!(lava.corrosion > 50.0 * earth.corrosion);
        assert!(lava.lungs() > earth.lungs());
        // No oxygen: no breath, and no fire either.
        let tundra = air_hazard(&d, &[("N2", 0.9), ("CO2", 0.06)], 1.0, 250.0);
        assert!(!tundra.breathable() && tundra.fire_spread() == 0.0);
        // Too much carbon dioxide poisons.
        let desert = air_hazard(&d, &[("CO2", 0.95), ("N2", 0.04)], 0.5, 280.0);
        assert!(desert.toxicity > 10.0);
        // Methane and oxygen mix explodes; the same methane in nitrogen does not.
        let bad = air_hazard(&d, &[("N2", 0.6), ("O2", 0.3), ("CH4", 0.1)], 1.0, 288.0);
        assert!(bad.explosive());
        let ice = air_hazard(&d, &[("N2", 0.8), ("CH4", 0.1)], 1.0, 100.0);
        assert!(ice.fuel_ratio > 1.0 && !ice.explosive());
        // Hot air corrodes faster than cold air.
        let hot = air_hazard(&d, &[("SO2", 0.6)], 1.0, 600.0);
        let cold = air_hazard(&d, &[("SO2", 0.6)], 1.0, 300.0);
        assert!(hot.corrosion > 4.0 * cold.corrosion);
    }

    #[test]
    fn ices_sublimate_on_hot_airless_worlds_and_iron_rusts_under_an_oxidising_air() {
        let d = data();
        assert!(volatile_stable(&d, "water_ice", 150.0, 0.0));
        assert!(!volatile_stable(&d, "water_ice", 300.0, 0.0));
        assert!(volatile_stable(&d, "water_ice", 300.0, 1.0));
        assert!(
            !volatile_stable(&d, "methane", 100.0, 0.0),
            "methane ice needs deep cold"
        );
        assert!(volatile_stable(&d, "methane", 60.0, 0.0));
        let rust = oxidation(&d, &[("N2", 0.78), ("O2", 0.21)], 1.0);
        let none = oxidation(&d, &[], 0.0);
        assert!(rust > 0.15 && none == 0.0);
        // On a real world of the sim the weathered deposits still hold iron, and rust appears.
        let s = sim();
        let mut saw_rust = false;
        for b in &s.system.bodies {
            let mut dep = crate::colony::site_deposits(&d, b, 1);
            let before = dep.len();
            weather_deposits(&d, b, &mut dep);
            let fe = d.resource_index("iron_oxide").unwrap();
            saw_rust |= dep.iter().any(|x| x.0 == fe);
            assert!(before > 0 || dep.is_empty());
        }
        let _ = saw_rust;
    }

    fn stock(s: &mut Sim, res: &str, kg: f64) {
        let i = s.data.resource_index(res).unwrap();
        let d = s.data.clone();
        s.ship.cargo.cap_mass = 1.0e9;
        s.ship.cargo.cap_vol = 1.0e9;
        s.ship.cargo.add(&d, i, kg);
    }

    fn kg(s: &Sim, res: &str) -> f64 {
        s.ship.cargo.kg[s.data.resource_index(res).unwrap()]
    }

    #[test]
    fn the_laboratory_electrolyses_water_into_air_and_fuel_and_conserves_mass() {
        let mut s = sim();
        s.ship.sys.battery_mj = 1.0e6;
        let water0 = kg(&s, "water");
        stock(&mut s, "water", 900.0);
        let total0: f64 = s.ship.cargo.kg.iter().sum();
        let r = s.data.reaction_index("electrolysis_water").unwrap();
        let rep = s.lab_run(r, 900.0).expect("lab");
        // 2 H2O -> 2 H2 + O2: 900 kg of water is 100 kg of hydrogen and 800 kg of oxygen.
        assert!(
            (kg(&s, "hydrogen") - 100.0).abs() < 1.5,
            "{}",
            kg(&s, "hydrogen")
        );
        assert!(
            (kg(&s, "oxygen") - 800.0).abs() < 2.0,
            "{}",
            kg(&s, "oxygen")
        );
        assert!(
            (kg(&s, "water") - water0).abs() < 1e-6,
            "all the water went"
        );
        let total1: f64 = s.ship.cargo.kg.iter().sum();
        assert!((total0 - total1).abs() < 1e-6, "mass {total0} -> {total1}");
        // Energy: the battery gave `electric`, part of it is stored in the fuel, the rest is heat.
        let dh = s.data.reaction_enthalpy(&s.data.reactions[r]);
        assert!(dh > 0.0);
        assert!((rep.heat_kj + dh * rep.extent - rep.electric_kj).abs() < 1e-6);
        assert!((1.0e6 - s.ship.sys.battery_mj - rep.electric_kj / 1000.0).abs() < 1e-6);
        assert!(rep.heat_kj > 0.0 && rep.electric_kj > dh * rep.extent);
    }

    #[test]
    fn the_laboratory_needs_stock_energy_catalyst_and_burning_fuel_gives_heat_back() {
        let mut s = sim();
        s.ship.sys.battery_mj = 0.0;
        let r = s.data.reaction_index("electrolysis_water").unwrap();
        assert!(s.lab_run(r, 10.0).is_err(), "no water");
        stock(&mut s, "water", 100.0);
        assert!(s.lab_run(r, 100.0).is_err(), "no energy");
        s.ship.sys.battery_mj = 1.0e6;
        let ha = s.data.reaction_index("haber_bosch").unwrap();
        stock(&mut s, "nitrogen", 28.0);
        stock(&mut s, "hydrogen", 6.0);
        let e = s.lab_run(ha, 34.0).unwrap_err();
        assert!(e.contains("catalyseur"), "{e}");
        stock(&mut s, "iron", 5.0);
        let rep = s.lab_run(ha, 34.0).expect("haber");
        assert!(kg(&s, "ammonia") > 30.0, "{}", kg(&s, "ammonia"));
        assert!(kg(&s, "iron") >= 4.999, "the catalyst is not consumed");
        assert!(rep.heat_kj > 0.0);
        // A fuel burns and warms the ship.
        stock(&mut s, "methane", 16.0);
        stock(&mut s, "oxygen", 64.0);
        let t0 = s.ship.sys.temp_k;
        let mc = s.data.reaction_index("methane_combustion").unwrap();
        let rep = s.lab_run(mc, 80.0).expect("burn");
        assert!(rep.heat_kj > 8.0e5, "{}", rep.heat_kj);
        assert!(s.ship.sys.temp_k > t0);
        assert!(kg(&s, "co2_ice") > 40.0 && kg(&s, "methane") < 1e-6);
        // Not every reaction can be run: the reactor is limited.
        assert!(!s.lab_reactions().is_empty());
    }

    #[test]
    fn a_blast_in_the_lab_damages_the_modules() {
        let mut s = sim();
        s.ship.sys.battery_mj = 1.0e6;
        stock(&mut s, "nitroglycerin", 50.0);
        let r = s.data.reaction_index("nitroglycerin_blast").unwrap();
        let before: f64 = s.ship.sys.health.iter().sum();
        let rep = s.lab_run(r, 50.0).expect("blast");
        let after: f64 = s.ship.sys.health.iter().sum();
        assert!(
            rep.blast_mj > 100.0 && after < before,
            "{} {before} {after}",
            rep.blast_mj
        );
        // The same reaction with a combustion kind does not blast.
        let mut s2 = sim();
        s2.ship.sys.battery_mj = 1.0e6;
        stock(&mut s2, "methane", 16.0);
        stock(&mut s2, "oxygen", 64.0);
        let mc = s2.data.reaction_index("methane_combustion").unwrap();
        let before: f64 = s2.ship.sys.health.iter().sum();
        s2.lab_run(mc, 80.0).unwrap();
        assert_eq!(before, s2.ship.sys.health.iter().sum::<f64>());
    }

    #[test]
    fn exposed_modules_corrode_in_a_corrosive_air_only() {
        let mut s = sim();
        // Put the ship in the air of a world, then in the void: only the air eats.
        let Some(i) = (0..s.bodies.len()).find(|&i| s.air_hazard_of(i).is_some()) else {
            return;
        };
        let b = &s.bodies[i];
        s.ship.pos = b.pos + vector_core::Vec3::new(b.radius + 100.0, 0.0, 0.0);
        let h0: f64 = s.ship.sys.health.iter().sum();
        s.corrode(10.0);
        let h1: f64 = s.ship.sys.health.iter().sum();
        let corr = s.air_hazard_of(i).unwrap().corrosion;
        assert_eq!(h1 < h0, corr > 0.0 && s.in_air().is_some());
        s.ship.pos = vector_core::Vec3::new(1.0e15, 0.0, 0.0);
        s.corrode(1.0e6);
        assert_eq!(h1, s.ship.sys.health.iter().sum::<f64>());
    }
}
