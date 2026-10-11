//! Chemistry tables (`data/chemistry.ron`: elements and compounds, `data/reactions.ron`:
//! reactions). The energy of a reaction is never typed in: it is the difference of the standard
//! enthalpies of formation of the products and of the reactants (Hess's law), so a table cannot
//! contradict itself. Every reaction is checked to balance, element by element.
use crate::{DataError, GameData};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A chemical element.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Element {
    pub symbol: String,
    pub name: String,
    /// Atomic number.
    pub z: u32,
    /// Molar mass, g/mol.
    pub mass: f64,
    /// Pauling electronegativity (0 for the noble gases).
    pub en: f64,
    /// Melting and boiling points at 1 bar, K.
    pub melt: f64,
    pub boil: f64,
    /// Solar abundance, log10 of the atoms per 10^12 hydrogen atoms (the astronomers' scale).
    pub abund: f64,
    pub text: String,
}

/// A pure substance: an element in its standard form (H2, Fe) or a compound.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Compound {
    pub id: String,
    pub name: String,
    /// Molecular formula, e.g. "Fe2O3" (no parentheses).
    pub formula: String,
    /// Class: element, oxyde, volatil, acide, sel, hydrocarbure, alcool, explosif, organique,
    /// silicate.
    pub class: String,
    /// Standard enthalpy of formation, kJ/mol (0 for an element in its standard state).
    pub hf: f64,
    /// Melting and boiling (or sublimation) points at 1 bar, K.
    pub melt: f64,
    pub boil: f64,
    /// Resources (`resources.ron`) that are this substance; the first is the one a reaction makes.
    pub resources: Vec<String>,
    /// Toxic above this mole fraction in the air breathed (0 = not toxic).
    pub limit: f64,
    /// Corrosiveness of the substance on metal and hull, per unit of mole fraction (0 = inert).
    pub corrosive: f64,
    /// Auto-ignition temperature in air, K (0 = not a fuel) and lower flammability limit (mole
    /// fraction of the fuel in air below which a flame does not spread).
    pub ignite: f64,
    pub lel: f64,
    /// Feeds a fire (O2, peroxides, nitrates).
    pub oxidizer: bool,
    pub text: String,
}

impl Compound {
    /// Physical state at temperature `t` (K) and 1 bar.
    pub fn state(&self, t: f64) -> &'static str {
        if t < self.melt {
            "SOLIDE"
        } else if t < self.boil {
            "LIQUIDE"
        } else {
            "GAZ"
        }
    }
}

/// A chemical reaction: stoichiometric coefficients, what it takes to start it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Reaction {
    pub id: String,
    pub name: String,
    /// synthese, combustion, explosion, electrolyse, reduction, corrosion, decomposition.
    pub kind: String,
    pub reactants: Vec<(String, u32)>,
    pub products: Vec<(String, u32)>,
    /// Temperature the reactants must be brought to (K) and the pressure needed (bar).
    pub t_act: f64,
    pub p_min: f64,
    /// A compound that must be present but is not consumed (empty = none).
    pub catalyst: String,
    pub text: String,
}

/// Elements and compounds (`chemistry.ron`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Chemistry {
    pub elements: Vec<Element>,
    pub compounds: Vec<Compound>,
}

pub const COMPOUND_CLASSES: [&str; 11] = [
    "element",
    "oxyde",
    "volatil",
    "acide",
    "sel",
    "hydrocarbure",
    "alcool",
    "explosif",
    "organique",
    "silicate",
    "carburant",
];
pub const REACTION_KINDS: [&str; 7] = [
    "synthese",
    "combustion",
    "explosion",
    "electrolyse",
    "reduction",
    "corrosion",
    "decomposition",
];

/// Atoms of a formula such as "C8H18" or "Fe2O3" as (symbol, count), in order of appearance.
pub fn parse_formula(f: &str) -> Option<Vec<(String, u32)>> {
    let mut out: Vec<(String, u32)> = Vec::new();
    let b: Vec<char> = f.chars().collect();
    let mut i = 0;
    if b.is_empty() {
        return None;
    }
    while i < b.len() {
        if !b[i].is_ascii_uppercase() {
            return None;
        }
        let mut sym = b[i].to_string();
        i += 1;
        while i < b.len() && b[i].is_ascii_lowercase() {
            sym.push(b[i]);
            i += 1;
        }
        let mut n = 0u32;
        let mut digits = false;
        while i < b.len() && b[i].is_ascii_digit() {
            n = n * 10 + b[i].to_digit(10)?;
            digits = true;
            i += 1;
        }
        let n = if digits { n } else { 1 };
        if n == 0 {
            return None;
        }
        match out.iter_mut().find(|(s, _)| *s == sym) {
            Some(e) => e.1 += n,
            None => out.push((sym, n)),
        }
    }
    Some(out)
}

impl GameData {
    pub fn element_index(&self, symbol: &str) -> Option<usize> {
        self.chemistry
            .elements
            .iter()
            .position(|e| e.symbol == symbol)
    }
    pub fn compound_index(&self, id: &str) -> Option<usize> {
        self.chemistry.compounds.iter().position(|c| c.id == id)
    }
    pub fn reaction_index(&self, id: &str) -> Option<usize> {
        self.reactions.iter().position(|r| r.id == id)
    }

    /// Atoms of compound `c` as (element symbol, count).
    pub fn compound_atoms(&self, c: usize) -> Vec<(String, u32)> {
        parse_formula(&self.chemistry.compounds[c].formula).unwrap_or_default()
    }

    /// Molar mass of compound `c`, g/mol, from its formula.
    pub fn molar_mass(&self, c: usize) -> f64 {
        self.compound_atoms(c)
            .iter()
            .map(|(s, n)| {
                self.element_index(s)
                    .map_or(0.0, |e| self.chemistry.elements[e].mass)
                    * f64::from(*n)
            })
            .sum()
    }

    /// The compound a resource is made of, if the resource is a pure substance.
    pub fn compound_of_resource(&self, res: &str) -> Option<usize> {
        self.chemistry
            .compounds
            .iter()
            .position(|c| c.resources.iter().any(|r| r == res))
    }

    /// Mass fraction of each element in a resource (None when it is not a pure substance).
    pub fn resource_elements(&self, res: &str) -> Option<Vec<(String, f64)>> {
        let c = self.compound_of_resource(res)?;
        let total = self.molar_mass(c);
        Some(
            self.compound_atoms(c)
                .into_iter()
                .map(|(s, n)| {
                    let m = self
                        .element_index(&s)
                        .map_or(0.0, |e| self.chemistry.elements[e].mass);
                    (s, m * f64::from(n) / total)
                })
                .collect(),
        )
    }

    /// Enthalpy of reaction, kJ per mole of reaction (negative: gives heat), by Hess's law.
    pub fn reaction_enthalpy(&self, r: &Reaction) -> f64 {
        let side = |l: &[(String, u32)]| -> f64 {
            l.iter()
                .map(|(c, n)| {
                    self.compound_index(c)
                        .map_or(0.0, |i| self.chemistry.compounds[i].hf)
                        * f64::from(*n)
                })
                .sum()
        };
        side(&r.products) - side(&r.reactants)
    }

    /// Atoms of each element on a side of a reaction.
    pub fn side_atoms(&self, l: &[(String, u32)]) -> BTreeMap<String, u64> {
        let mut m = BTreeMap::new();
        for (c, n) in l {
            if let Some(i) = self.compound_index(c) {
                for (s, k) in self.compound_atoms(i) {
                    *m.entry(s).or_insert(0) += u64::from(k) * u64::from(*n);
                }
            }
        }
        m
    }

    /// Chemistry tables: known elements and resources, balanced reactions, energy that agrees
    /// with the kind of reaction.
    pub(crate) fn validate_chemistry(&self) -> Result<(), DataError> {
        let bad = |s: String| Err(DataError::Invalid(s));
        let ch = &self.chemistry;
        let mut seen = std::collections::HashSet::new();
        for e in &ch.elements {
            if !seen.insert(e.symbol.as_str()) {
                return bad(format!("element {} en double", e.symbol));
            }
            if e.mass <= 0.0 || e.z == 0 || e.en < 0.0 || e.melt <= 0.0 || e.boil < e.melt {
                return bad(format!("element {}: valeurs invalides", e.symbol));
            }
        }
        let mut seen = std::collections::HashSet::new();
        let mut owner: BTreeMap<&str, &str> = BTreeMap::new();
        for c in &ch.compounds {
            if !seen.insert(c.id.as_str()) {
                return bad(format!("compose {} en double", c.id));
            }
            let Some(atoms) = parse_formula(&c.formula) else {
                return bad(format!("compose {}: formule {} illisible", c.id, c.formula));
            };
            for (s, _) in &atoms {
                if self.element_index(s).is_none() {
                    return bad(format!("compose {}: element {s} inconnu", c.id));
                }
            }
            if !COMPOUND_CLASSES.contains(&c.class.as_str()) {
                return bad(format!("compose {}: classe {} inconnue", c.id, c.class));
            }
            if c.melt <= 0.0 || c.boil < c.melt || !c.hf.is_finite() {
                return bad(format!("compose {}: valeurs invalides", c.id));
            }
            if c.limit < 0.0 || c.corrosive < 0.0 || c.ignite < 0.0 || c.lel < 0.0 || c.lel > 1.0 {
                return bad(format!("compose {}: danger hors bornes", c.id));
            }
            if (c.ignite > 0.0) != (c.lel > 0.0) {
                return bad(format!("compose {}: ignite et lel vont ensemble", c.id));
            }
            // A pure element in its standard state has no enthalpy of formation.
            if c.class == "element" && c.hf != 0.0 {
                return bad(format!("compose {}: un element a hf = 0", c.id));
            }
            for r in &c.resources {
                if self.resource_index(r).is_none() {
                    return bad(format!("compose {}: ressource {r} inconnue", c.id));
                }
                if let Some(o) = owner.insert(r.as_str(), c.id.as_str()) {
                    return bad(format!("ressource {r} dans {o} et {}", c.id));
                }
            }
            if c.text.is_empty() {
                return bad(format!("compose {}: texte manquant", c.id));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for r in &self.reactions {
            if !seen.insert(r.id.as_str()) {
                return bad(format!("reaction {} en double", r.id));
            }
            if !REACTION_KINDS.contains(&r.kind.as_str()) {
                return bad(format!("reaction {}: genre {} inconnu", r.id, r.kind));
            }
            if r.reactants.is_empty() || r.products.is_empty() {
                return bad(format!("reaction {}: un cote est vide", r.id));
            }
            for (c, n) in r.reactants.iter().chain(&r.products) {
                if *n == 0 || self.compound_index(c).is_none() {
                    return bad(format!("reaction {}: compose {c} invalide", r.id));
                }
            }
            if !r.catalyst.is_empty() && self.compound_index(&r.catalyst).is_none() {
                return bad(format!(
                    "reaction {}: catalyseur {} inconnu",
                    r.id, r.catalyst
                ));
            }
            if !(1.0..=6000.0).contains(&r.t_act) || r.p_min <= 0.0 || r.p_min > 1000.0 {
                return bad(format!("reaction {}: conditions hors bornes", r.id));
            }
            if self.side_atoms(&r.reactants) != self.side_atoms(&r.products) {
                return bad(format!("reaction {}: atomes non conserves", r.id));
            }
            let dh = self.reaction_enthalpy(r);
            let heats = ["combustion", "explosion"].contains(&r.kind.as_str());
            if (heats && dh >= 0.0) || (r.kind == "electrolyse" && dh <= 0.0) {
                return bad(format!(
                    "reaction {}: enthalpie {dh:.0} incoherente avec {}",
                    r.id, r.kind
                ));
            }
            if r.text.is_empty() {
                return bad(format!("reaction {}: texte manquant", r.id));
            }
        }
        Ok(())
    }
}
