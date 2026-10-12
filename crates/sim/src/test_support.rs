//! Helpers shared by the tests of the simulation (audit B18): the data tables loaded once, a
//! fresh copy when a test edits them, and a ship docked at the nearest station.
use crate::scene::new_sim;
use crate::Sim;
use std::sync::{Arc, OnceLock};
use vector_data::GameData;

/// A fresh copy of the game data (for a test that edits it).
pub fn load() -> GameData {
    GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
        .expect("data")
}

/// The game data, loaded once for all the tests of the process.
pub fn data() -> Arc<GameData> {
    static D: OnceLock<Arc<GameData>> = OnceLock::new();
    Arc::clone(D.get_or_init(|| Arc::new(load())))
}

/// A new game docked at the nearest station with a million credits.
pub fn docked_sim() -> Sim {
    let mut s = new_sim(5, data());
    let (st, _) = s.nearest_station().expect("station");
    s.spawn_near(st, s.bodies[st].radius + 200.0);
    s.try_dock().expect("dock");
    s.credits = 1.0e6;
    s
}

#[test]
fn the_shared_data_is_loaded_once_and_a_fresh_copy_is_independent() {
    assert!(Arc::ptr_eq(&data(), &data()));
    let mut a = load();
    a.recipes.clear();
    assert!(!data().recipes.is_empty());
    assert!(docked_sim().docked.is_some());
}
