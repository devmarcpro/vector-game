//! Data schemas and loading/validation of `data/*.ron`. Content lives in data, not code.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub name: String,
    pub category: String,
    /// kg/m3
    pub density: f64,
    /// credits per kg (base price)
    pub value: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AsteroidClass {
    pub id: String,
    pub name: String,
    pub weight: f64,
    pub density: f64,
    /// (resource id, mass fraction)
    pub composition: Vec<(String, f64)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotType {
    #[default]
    Core,
    Internal,
    External,
    Hardpoint,
}

impl SlotType {
    pub fn label(self) -> &'static str {
        match self {
            SlotType::Core => "NOYAU",
            SlotType::Internal => "INTERNE",
            SlotType::External => "EXTERNE",
            SlotType::Hardpoint => "ARMEMENT",
        }
    }
}

/// A ship module. Unused stats default to zero so data files only list what matters.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Module {
    pub id: String,
    pub name: String,
    pub category: String,
    pub slot: SlotType,
    pub size: u8,
    pub mass: f64,
    pub hp: f64,
    pub cost: f64,
    /// kW: positive = produced (reactor/solar), negative = consumed at full load.
    pub power: f64,
    pub priority: u8,
    pub thrust: f64,
    pub isp: f64,
    pub fuel: f64,
    pub cargo_kg: f64,
    pub cargo_m3: f64,
    /// MJ
    pub battery: f64,
    /// kW dissipated at reference temperature
    pub radiate: f64,
    pub mining_rate: f64,
    pub mining_range: f64,
    pub life_support: f64,
    pub crew_cap: f64,
    pub medical: f64,
    pub science: f64,
    pub repair: f64,
    pub food: f64,
    pub scan_range: f64,
    pub armor: f64,
    pub agility: f64,
    pub rcs: f64,
    pub shield: f64,
    pub dock: f64,
    pub hangar: f64,
    pub armory: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hull {
    pub id: String,
    pub name: String,
    pub class: String,
    pub mass: f64,
    pub hp: f64,
    pub cost: f64,
    pub rcs: f64,
    pub max_rate: f64,
    pub ang_accel: f64,
    pub slots: Vec<(SlotType, u8)>,
}

#[derive(Debug)]
pub enum DataError {
    Io(PathBuf, std::io::Error),
    Parse(PathBuf, String),
    Invalid(String),
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataError::Io(p, e) => write!(f, "{}: {e}", p.display()),
            DataError::Parse(p, e) => write!(f, "{}: {e}", p.display()),
            DataError::Invalid(s) => write!(f, "donnees invalides: {s}"),
        }
    }
}
impl std::error::Error for DataError {}

#[derive(Clone, Debug, Default)]
pub struct GameData {
    pub resources: Vec<Resource>,
    pub asteroids: Vec<AsteroidClass>,
    pub modules: Vec<Module>,
    pub hulls: Vec<Hull>,
}

fn read<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T, DataError> {
    let text = std::fs::read_to_string(path).map_err(|e| DataError::Io(path.to_path_buf(), e))?;
    ron::from_str(&text).map_err(|e| DataError::Parse(path.to_path_buf(), e.to_string()))
}

impl GameData {
    pub fn load(dir: &Path) -> Result<GameData, DataError> {
        let d = GameData {
            resources: read(&dir.join("resources.ron"))?,
            asteroids: read(&dir.join("worldgen/asteroids.ron"))?,
            modules: read(&dir.join("modules.ron"))?,
            hulls: read(&dir.join("hulls.ron"))?,
        };
        d.validate()?;
        Ok(d)
    }

    pub fn module_index(&self, id: &str) -> Option<usize> {
        self.modules.iter().position(|m| m.id == id)
    }
    pub fn hull_index(&self, id: &str) -> Option<usize> {
        self.hulls.iter().position(|h| h.id == id)
    }

    pub fn resource_index(&self, id: &str) -> Option<usize> {
        self.resources.iter().position(|r| r.id == id)
    }

    /// Fails on duplicate ids, unknown references and out-of-range values.
    pub fn validate(&self) -> Result<(), DataError> {
        let bad = |s: String| Err(DataError::Invalid(s));
        let mut ids = BTreeSet::new();
        for r in &self.resources {
            if !ids.insert(r.id.as_str()) {
                return bad(format!("ressource en double: {}", r.id));
            }
            if r.density <= 0.0 || r.value < 0.0 {
                return bad(format!("valeurs hors bornes: {}", r.id));
            }
        }
        let mut cls = BTreeSet::new();
        for a in &self.asteroids {
            if !cls.insert(a.id.as_str()) {
                return bad(format!("classe d'asteroide en double: {}", a.id));
            }
            let mut sum = 0.0;
            for (rid, f) in &a.composition {
                if !ids.contains(rid.as_str()) {
                    return bad(format!(
                        "asteroide {} reference une ressource inconnue: {rid}",
                        a.id
                    ));
                }
                if !(0.0..=1.0).contains(f) {
                    return bad(format!("fraction hors bornes dans {}", a.id));
                }
                sum += f;
            }
            if !(0.99..=1.01).contains(&sum) {
                return bad(format!("composition de {} somme a {sum}", a.id));
            }
        }
        let mut mids = BTreeSet::new();
        for m in &self.modules {
            if !mids.insert(m.id.as_str()) {
                return bad(format!("module en double: {}", m.id));
            }
            if !(1..=4).contains(&m.size) || m.mass <= 0.0 || m.hp <= 0.0 || m.cost < 0.0 {
                return bad(format!("module invalide: {}", m.id));
            }
            let produces = matches!(m.category.as_str(), "reactor" | "solar");
            if produces != (m.power > 0.0) && m.power != 0.0 {
                return bad(format!("signe de puissance incoherent: {}", m.id));
            }
        }
        let mut hids = BTreeSet::new();
        for h in &self.hulls {
            if !hids.insert(h.id.as_str()) {
                return bad(format!("coque en double: {}", h.id));
            }
            if h.slots.is_empty()
                || h.slots.iter().any(|(_, s)| !(1..=4).contains(s))
                || h.mass <= 0.0
                || h.hp <= 0.0
            {
                return bad(format!("coque invalide: {}", h.id));
            }
        }
        Ok(())
    }
}

/// Locate the `data/` directory: in the cwd or next to the executable, searching upward.
pub fn find_data_dir() -> Option<PathBuf> {
    let starts = [
        std::env::current_dir().ok(),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf)),
    ];
    for s in starts.into_iter().flatten() {
        let mut p = Some(s.as_path());
        while let Some(d) = p {
            if d.join("data/resources.ron").exists() {
                return Some(d.join("data"));
            }
            p = d.parent();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")
    }

    #[test]
    fn shipped_data_is_valid() {
        let d = GameData::load(&dir()).expect("data loads");
        assert!(d.resources.len() >= 40, "{}", d.resources.len());
        assert!(d.asteroids.len() >= 4);
        assert!(d.modules.len() >= 60, "{}", d.modules.len());
        assert!(d.hulls.len() >= 15);
    }

    #[test]
    fn validation_catches_errors() {
        let mut d = GameData::load(&dir()).expect("data loads");
        d.asteroids[0].composition.push(("unobtainium".into(), 0.0));
        assert!(d.validate().is_err());
        let mut d = GameData::load(&dir()).expect("data loads");
        let dup = d.resources[0].clone();
        d.resources.push(dup);
        assert!(d.validate().is_err());
    }
}
