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
        };
        d.validate()?;
        Ok(d)
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
