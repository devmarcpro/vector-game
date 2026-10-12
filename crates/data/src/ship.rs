//! Ship parts (sections of hull assembled on a grid of cells) and ready-made plans, from
//! `data/ship_parts.ron` and `data/ship_plans.ron`.
use crate::{DataError, GameData, SlotType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Edge of a grid cell, metres.
pub const CELL_M: f64 = 6.0;
/// Face bits of `ShipPart::ports`: +x (nose side), -x (tail side), +y, -y, +z (up), -z (down).
pub const PORT_PX: u8 = 1;
pub const PORT_NX: u8 = 2;
pub const PORT_PY: u8 = 4;
pub const PORT_NY: u8 = 8;
pub const PORT_PZ: u8 = 16;
pub const PORT_NZ: u8 = 32;
/// Kinds of part (the data says which one each part is).
pub const PART_KINDS: [&str; 10] = [
    "cockpit", "hull", "deck", "hold", "bay", "engine", "nacelle", "wing", "gear", "utility",
];
/// Kinds of room a part gives to the plan of the inside of the ship ("" = none, not walkable).
pub const ROOM_KINDS: [&str; 7] = [
    "",
    "passerelle",
    "cabines",
    "soute",
    "ingenierie",
    "hangar",
    "coursive",
];

/// One section of a ship: it fills one cell of the grid (6 m on a side).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ShipPart {
    pub id: String,
    pub name: String,
    pub kind: String,
    /// Dry mass, kg.
    pub mass: f64,
    /// Pressurised volume, m3, of which `hab_m3` can be lived in.
    pub volume: f64,
    pub hab_m3: f64,
    /// Structural strength (hit points added to the hull).
    pub hp: f64,
    /// Materials consumed by the build (resource id, kg).
    pub materials: Vec<(String, f64)>,
    /// Person-days of work.
    pub labor: f64,
    /// Faces that can be joined to a neighbour (bits `PORT_*`).
    pub ports: u8,
    /// Module slots the part brings to the hull.
    pub slots: Vec<(SlotType, u8)>,
    pub berths: u32,
    /// Area presented to the air (m2) and the part of it that lifts (m2).
    pub aero_area: f64,
    pub lift_area: f64,
    /// Load (kg) one landing leg can carry at 1 g.
    pub support_kg: f64,
    /// Thrust (N) of the attitude thrusters it carries.
    pub rcs: f64,
    /// Room it makes in the plan of the inside ("" = none).
    pub room: String,
    pub text: String,
}

/// A plan on offer (found or bought): parts on cells and the modules fitted in their slots.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ShipPlanDef {
    pub id: String,
    pub name: String,
    /// What the plan itself costs (credits).
    pub price: f64,
    pub cells: Vec<(String, i8, i8, i8)>,
    /// (x, y, z, slot of the part, module id)
    pub loadout: Vec<(i8, i8, i8, u8, String)>,
    pub text: String,
}

impl GameData {
    pub fn ship_part_index(&self, id: &str) -> Option<usize> {
        self.ship_parts.iter().position(|p| p.id == id)
    }

    /// Parts: unique ids, known kinds and resources, sane numbers; a cockpit, an engine mount and
    /// a landing gear must exist. Plans: known parts and modules.
    pub(crate) fn validate_ship(&self) -> Result<(), DataError> {
        let bad = |s: String| Err(DataError::Invalid(s));
        let mut seen = BTreeSet::new();
        for p in &self.ship_parts {
            if !seen.insert(p.id.as_str()) {
                return bad(format!("piece de vaisseau en double: {}", p.id));
            }
            if !PART_KINDS.contains(&p.kind.as_str()) {
                return bad(format!("piece {}: genre inconnu {}", p.id, p.kind));
            }
            if !ROOM_KINDS.contains(&p.room.as_str()) {
                return bad(format!("piece {}: salle inconnue {}", p.id, p.room));
            }
            if p.mass <= 0.0
                || p.hp <= 0.0
                || p.labor <= 0.0
                || p.volume < 0.0
                || p.hab_m3 < 0.0
                || p.hab_m3 > p.volume
                || p.ports == 0
                || p.ports > 63
                || p.materials.is_empty()
                || p.slots.iter().any(|(_, s)| !(1..=4).contains(s))
            {
                return bad(format!("piece invalide: {}", p.id));
            }
            for (r, kg) in &p.materials {
                if *kg <= 0.0 || self.resource_index(r).is_none() {
                    return bad(format!("piece {}: materiau inconnu {r}", p.id));
                }
            }
            if p.kind == "gear" && p.support_kg <= 0.0 {
                return bad(format!("piece {}: un train doit porter une charge", p.id));
            }
        }
        for kind in ["cockpit", "engine", "gear", "hull"] {
            if !self.ship_parts.iter().any(|p| p.kind == kind) {
                return bad(format!("aucune piece de genre {kind}"));
            }
        }
        let mut plans = BTreeSet::new();
        for pl in &self.ship_plans {
            if !plans.insert(pl.id.as_str()) {
                return bad(format!("plan en double: {}", pl.id));
            }
            if pl.cells.is_empty() || pl.price < 0.0 {
                return bad(format!("plan invalide: {}", pl.id));
            }
            for (part, ..) in &pl.cells {
                if self.ship_part_index(part).is_none() {
                    return bad(format!("plan {}: piece inconnue {part}", pl.id));
                }
            }
            for (.., module) in &pl.loadout {
                if self.module_index(module).is_none() {
                    return bad(format!("plan {}: module inconnu {module}", pl.id));
                }
            }
        }
        Ok(())
    }
}

/// A value as text (RON), for plans saved to a file and shared.
pub fn to_text<T: Serialize>(v: &T) -> String {
    ron::ser::to_string_pretty(v, ron::ser::PrettyConfig::default()).unwrap_or_default()
}

/// A value read back from text.
pub fn from_text<T: for<'a> Deserialize<'a>>(text: &str) -> Result<T, String> {
    ron::from_str(text).map_err(|e| e.to_string())
}
