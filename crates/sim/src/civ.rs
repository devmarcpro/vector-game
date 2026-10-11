//! Native civilisations (E4): living worlds with enough habitable land may hold one. Which one,
//! its name, mores and size come from the world's seed (nothing is stored); only what the player
//! did to its goodwill is saved.
use crate::relations::Entity;
use crate::Sim;
use vector_core::{hash_coord, Rng};
use vector_world::surface::{H, W};

/// A conversation with a world's people is held as a subject numbered `CIV_SUBJECT + body`.
pub const CIV_SUBJECT: usize = 1_000_000;
/// Share of habitable worlds that carry a civilisation.
pub const CIV_CHANCE: f64 = 0.35;
/// Average habitability of the land needed.
pub const MIN_HABIT: f64 = 0.10;

#[derive(Clone, Debug)]
pub struct Civ {
    /// Index into `GameData::civilisations`.
    pub kind: usize,
    pub name: String,
    /// Mores on the axes of `ethos.ron`: the kind's, shifted a little by this world.
    pub profile: Vec<f64>,
    pub tech: u8,
    pub population: f64,
}

impl Sim {
    pub(crate) fn civ_key(&self, body: usize) -> u64 {
        hash_coord(self.system.seed, body as i64, 0, 71)
    }

    /// The civilisation of `body`, if any (solid worlds only).
    pub fn civilisation(&self, body: usize) -> Option<Civ> {
        let map = self.planet_map(body)?;
        let data = &self.data;
        // Habitable land: the mean of `habit` over a sample of the surface.
        let (mut sum, mut n) = (0.0, 0.0);
        for y in (0..H).step_by(6) {
            for x in (0..W).step_by(6) {
                if !map.surface.is_sea(x, y) {
                    sum += data.biomes[map.biome_at(x, y)].habit;
                }
                n += 1.0;
            }
        }
        let habit = sum / n;
        if habit < MIN_HABIT {
            return None;
        }
        let mut rng = Rng::new(self.civ_key(body));
        if rng.f64() > CIV_CHANCE {
            return None;
        }
        let kind = (rng.next_u64() % data.civilisations.len() as u64) as usize;
        let k = &data.civilisations[kind];
        let mut name = String::new();
        for i in 0..2 + (rng.next_u64() % 2) as usize {
            let syl = &k.syllables[(rng.next_u64() % k.syllables.len() as u64) as usize];
            if i == 0 {
                let mut cs = syl.chars();
                name.extend(cs.next().into_iter().flat_map(char::to_uppercase));
                name.push_str(cs.as_str());
            } else {
                name.push_str(syl);
            }
        }
        let profile = k
            .profile
            .iter()
            .map(|v| (v + (rng.f64() - 0.5) * 0.5).clamp(-1.0, 1.0))
            .collect();
        Some(Civ {
            kind,
            name,
            profile,
            tech: k.tech,
            population: 1.0e5 * (1.0 + 400.0 * habit * rng.f64()),
        })
    }

    /// Goodwill of the people of `body` toward the player (a view of the relations registry).
    pub fn civ_reputation(&self, body: usize) -> f32 {
        self.rel(Entity::civ(self.civ_key(body)))
    }

    /// Change the goodwill of the people of `body`; the territory it lies in feels it a little.
    pub fn change_civ_reputation(&mut self, body: usize, delta: f32) {
        let e = Entity::civ(self.civ_key(body));
        if !self.relations.labels.iter().any(|(k, _)| *k == e) {
            if let Some(c) = self.civilisation(body) {
                self.relations.label(e, &c.name);
            }
        }
        self.relate(e, delta);
    }

    /// Whether the ship is near enough to a world to talk to its people.
    pub fn can_hail_world(&self, body: usize) -> Result<(), &'static str> {
        let b = self.bodies.get(body).ok_or("corps inconnu")?;
        if (self.ship.pos - b.pos).length() > b.radius * crate::rogue::MAX_RANGE_RADII + 1.0e6 {
            return Err("trop loin pour une transmission");
        }
        if self.civilisation(body).is_none() {
            return Err("aucune reponse : monde sans civilisation");
        }
        Ok(())
    }

    /// The first world of the system that holds a civilisation (for tests and scenes).
    pub fn first_civilised_world(&self) -> Option<usize> {
        (0..self.bodies.len()).find(|&i| self.civilisation(i).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn sim(seed: u64) -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        new_sim(seed, Arc::new(GameData::load(&dir).expect("data loads")))
    }

    #[test]
    fn civilisations_come_from_the_seed_and_only_on_habitable_land() {
        let mut found = 0;
        let mut barren = 0;
        for seed in 1..40u64 {
            let s = sim(seed);
            for b in 0..s.bodies.len() {
                let a = s.civilisation(b);
                let again = s.civilisation(b);
                assert_eq!(
                    a.as_ref().map(|c| (c.kind, c.name.clone())),
                    again.as_ref().map(|c| (c.kind, c.name.clone())),
                    "pure function of the world"
                );
                let is_none = a.is_none();
                match a {
                    Some(c) => {
                        found += 1;
                        assert!(
                            !c.name.is_empty() && c.name.chars().next().unwrap().is_uppercase()
                        );
                        assert_eq!(c.profile.len(), s.data.ethos.axes.len());
                        assert!(c.profile.iter().all(|v| (-1.0..=1.0).contains(v)));
                        assert!(c.population > 1.0e5);
                    }
                    None => barren += 1,
                }
                // Giants and stars never hold one.
                if matches!(
                    s.bodies[b].kind,
                    vector_world::BodyKind::Star(_) | vector_world::BodyKind::Station
                ) {
                    assert!(is_none);
                }
            }
        }
        assert!(
            found >= 3 && barren > found,
            "found {found}, barren {barren}"
        );
    }

    #[test]
    fn goodwill_is_remembered_per_world_and_clamped() {
        let mut s = sim(7);
        let b = 1;
        assert_eq!(s.civ_reputation(b), 0.0);
        s.change_civ_reputation(b, 0.4);
        s.change_civ_reputation(b, 0.9);
        assert_eq!(s.civ_reputation(b), 1.0);
        s.change_civ_reputation(b, -3.0);
        assert_eq!(s.civ_reputation(b), -1.0);
        assert_eq!(
            s.civ_reputation(b + 1),
            0.0,
            "another world, another memory"
        );
    }
}
