//! Livestock and breeding: domestic species (those flagged `domestic` in the species table) can
//! be bought at a station, kept aboard, fed with rations, bred and harvested. Each animal has four
//! traits written as two alleles each (Mendel: a parent hands one allele of each trait over, with
//! rare mutations), so a careful breeder improves a herd over generations: more yield per animal,
//! quicker maturity, hardiness against hunger, docility (fewer escapes... of the herd's upkeep).
use crate::bourse::DAY;
use crate::{Event, Sim};
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng};

/// Traits, by index in `Animal::genes`.
pub const TRAITS: [&str; 4] = ["RENDEMENT", "CROISSANCE", "RUSTICITE", "DOCILITE"];
/// Largest herd the ship's pens hold.
pub const HERD_MAX: usize = 24;
/// Days from birth to adulthood for an average animal.
pub const MATURITY_DAYS: f64 = 20.0;
/// Days between two litters of the same female.
pub const BREED_COOLDOWN: f64 = 30.0;
/// Rations an average animal eats per day, per kg of its body weight to the 3/4 power.
pub const FEED_RATE: f64 = 0.02;
/// Days an average animal survives without food.
pub const STARVE_DAYS: f64 = 8.0;
/// Chance that an allele handed over is changed by one step.
pub const MUTATION: f64 = 0.02;
/// Allele range.
pub const ALLELE_MAX: u8 = 4;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Animal {
    pub species: usize,
    pub male: bool,
    pub born: f64,
    pub genes: [[u8; 2]; 4],
    /// Days without food so far.
    pub hunger: f64,
    /// Last time this female gave birth.
    pub last_birth: f64,
    /// Generation (0 for animals bought).
    pub gen: u32,
}

impl Animal {
    /// Value of a trait in [0, 1].
    pub fn trait_value(&self, t: usize) -> f64 {
        f64::from(self.genes[t][0] + self.genes[t][1]) / f64::from(2 * ALLELE_MAX)
    }

    pub fn age_days(&self, now: f64) -> f64 {
        (now - self.born) / DAY
    }

    /// Adult when old enough; fast growers mature sooner (from 12 to 28 days).
    pub fn adult(&self, now: f64) -> bool {
        self.age_days(now) >= MATURITY_DAYS * (1.4 - 0.8 * self.trait_value(1))
    }

    /// Multiplier on what the species yields when harvested (0.5 to 1.5).
    pub fn yield_factor(&self) -> f64 {
        0.5 + self.trait_value(0)
    }

    /// Average of the four traits, the "quality" of the animal.
    pub fn quality(&self) -> f64 {
        (0..4).map(|t| self.trait_value(t)).sum::<f64>() / 4.0
    }
}

/// An animal of a given species made by chance (the stock of a station dealer).
fn random_animal(seed: u64, species: usize, now: f64, n: i64) -> Animal {
    let mut rng = Rng::new(hash_coord(seed ^ 0x11FE, species as i64, n, now as i64));
    let mut genes = [[0u8; 2]; 4];
    for g in &mut genes {
        for a in g.iter_mut() {
            *a = (rng.f64() * f64::from(ALLELE_MAX + 1)) as u8;
        }
    }
    Animal {
        species,
        male: rng.f64() < 0.5,
        born: now - MATURITY_DAYS * 2.0 * DAY,
        genes,
        hunger: 0.0,
        last_birth: f64::MIN,
        gen: 0,
    }
}

/// The offspring of two parents, genes recombined and rarely mutated.
pub fn offspring(a: &Animal, b: &Animal, now: f64, rng: &mut Rng) -> Animal {
    let mut genes = [[0u8; 2]; 4];
    for (t, gene) in genes.iter_mut().enumerate() {
        for (k, parent) in [a, b].into_iter().enumerate() {
            let mut allele = parent.genes[t][(rng.f64() < 0.5) as usize];
            if rng.f64() < MUTATION {
                allele = if rng.f64() < 0.5 {
                    allele.saturating_sub(1)
                } else {
                    (allele + 1).min(ALLELE_MAX)
                };
            }
            gene[k] = allele;
        }
    }
    Animal {
        species: a.species,
        male: rng.f64() < 0.5,
        born: now,
        genes,
        hunger: 0.0,
        last_birth: f64::MIN,
        gen: a.gen.max(b.gen) + 1,
    }
}

impl Sim {
    /// Indices of the domestic species that can be bought.
    pub fn domestic_species(&self) -> Vec<usize> {
        (0..self.data.species.len())
            .filter(|&i| self.data.species[i].domestic && !self.data.species[i].is_producer())
            .collect()
    }

    /// Price of an animal of species `s` at a station (credits).
    pub fn animal_price(&self, s: usize) -> f64 {
        let sp = &self.data.species[s];
        let worth: f64 = sp
            .yields
            .iter()
            .filter_map(|(r, kg)| {
                self.data
                    .resource_index(r)
                    .map(|i| self.data.resources[i].value * kg)
            })
            .sum();
        (worth * 6.0 + sp.body * 2.0).max(20.0)
    }

    /// Buy one animal of species `s` (a station dealer sells them).
    pub fn buy_animal(&mut self, s: usize) -> Result<f64, &'static str> {
        self.docked
            .ok_or("non amarre : l'eleveur est a la station")?;
        if !self.domestic_species().contains(&s) {
            return Err("espece non domestique");
        }
        if self.herd.len() >= HERD_MAX {
            return Err("enclos pleins");
        }
        let price = self.animal_price(s);
        if price > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= price;
        let n = self.herd_counter;
        self.herd_counter += 1;
        self.herd
            .push(random_animal(self.galaxy.seed, s, self.time, n));
        Ok(price)
    }

    /// Sell animal `i` to the dealer of the station (less than a new one costs: he must live).
    pub fn sell_animal(&mut self, i: usize) -> Result<f64, &'static str> {
        self.docked
            .ok_or("non amarre : l'eleveur est a la station")?;
        let a = self.herd.get(i).ok_or("animal inconnu")?;
        let pay = self.animal_price(a.species) * (0.35 + 0.4 * a.quality());
        self.herd.remove(i);
        self.credits += pay;
        Ok(pay)
    }

    /// Whether animal `i` could breed with animal `j`.
    pub fn can_breed(&self, i: usize, j: usize) -> Result<(), &'static str> {
        let (Some(a), Some(b)) = (self.herd.get(i), self.herd.get(j)) else {
            return Err("animal inconnu");
        };
        if i == j || a.species != b.species {
            return Err("pas de la meme espece");
        }
        if a.male == b.male {
            return Err("il faut un male et une femelle");
        }
        if !a.adult(self.time) || !b.adult(self.time) {
            return Err("trop jeunes");
        }
        let f = if a.male { b } else { a };
        if (self.time - f.last_birth) / DAY < BREED_COOLDOWN {
            return Err("la femelle a mis bas recemment");
        }
        if self.herd.len() >= HERD_MAX {
            return Err("enclos pleins");
        }
        if f.hunger > 0.0 {
            return Err("la femelle a faim");
        }
        Ok(())
    }

    /// Breed `i` with `j`; the young one joins the herd. Returns its index.
    pub fn breed(&mut self, i: usize, j: usize) -> Result<usize, &'static str> {
        self.can_breed(i, j)?;
        let mut rng = Rng::new(hash_coord(
            self.galaxy.seed ^ 0xB4EE,
            i as i64,
            j as i64,
            self.time as i64,
        ));
        let young = offspring(&self.herd[i], &self.herd[j], self.time, &mut rng);
        let mother = if self.herd[i].male { j } else { i };
        self.herd[mother].last_birth = self.time;
        self.herd.push(young);
        Ok(self.herd.len() - 1)
    }

    /// The best partner for animal `i` right now (highest quality), if any.
    pub fn best_partner(&self, i: usize) -> Option<usize> {
        (0..self.herd.len())
            .filter(|&j| self.can_breed(i, j).is_ok())
            .max_by(|&a, &b| {
                self.herd[a]
                    .quality()
                    .total_cmp(&self.herd[b].quality())
                    .then(b.cmp(&a))
            })
    }

    /// Slaughter / harvest animal `i`: its yields (scaled by its yield trait) go to the hold.
    /// Returns the list of (resource, kg) put aboard.
    pub fn harvest_animal(&mut self, i: usize) -> Result<Vec<(usize, f64)>, &'static str> {
        let a = self.herd.get(i).ok_or("animal inconnu")?;
        if !a.adult(self.time) {
            return Err("trop jeune pour etre abattu");
        }
        let factor = a.yield_factor();
        let sp = a.species;
        let data = std::sync::Arc::clone(&self.data);
        let mut got = Vec::new();
        for (r, kg) in &data.species[sp].yields {
            if let Some(ri) = data.resource_index(r) {
                let put = self.ship.cargo.add(&data, ri, kg * factor);
                got.push((ri, put));
            }
        }
        self.herd.remove(i);
        Ok(got)
    }

    /// Feed the herd, once a day: rations are eaten from the hold, hunger kills.
    pub(crate) fn livestock_tick(&mut self) {
        let day = (self.time / DAY).floor() as i64;
        let last = self.last_livestock_day;
        self.last_livestock_day = day;
        if last == i64::MIN || day <= last || self.herd.is_empty() {
            return;
        }
        let data = std::sync::Arc::clone(&self.data);
        let Some(food) = data.resource_index("food") else {
            return;
        };
        let mut dead = Vec::new();
        for _ in last..day.min(last + 3) {
            for (k, a) in self.herd.iter_mut().enumerate() {
                let need = FEED_RATE * data.species[a.species].body.max(0.1).powf(0.75);
                if self.ship.cargo.remove(&data, food, need) >= need - 1e-9 {
                    a.hunger = 0.0;
                } else {
                    a.hunger += 1.0;
                    if a.hunger > STARVE_DAYS * (0.5 + a.trait_value(2)) && !dead.contains(&k) {
                        dead.push(k);
                    }
                }
            }
        }
        if !dead.is_empty() {
            let n = dead.len();
            for k in dead.into_iter().rev() {
                self.herd.remove(k);
            }
            self.events.push(Event::MarketNews(format!(
                "Elevage : {n} animal(aux) mort(s) de faim, il manque des rations."
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn docked() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let mut s = new_sim(5, Arc::new(GameData::load(&dir).expect("data loads")));
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.credits = 1.0e6;
        s.time = 100.0 * DAY;
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        s
    }

    /// A pair of adults of a domestic species, one of each sex.
    fn pair(s: &mut Sim) -> (usize, usize) {
        let sp = s.domestic_species()[0];
        s.herd.clear();
        for male in [true, false] {
            let mut a = random_animal(s.galaxy.seed, sp, s.time, i64::from(male));
            a.male = male;
            s.herd.push(a);
        }
        (0, 1)
    }

    #[test]
    fn there_are_domestic_animals_and_buying_costs_credits_and_a_pen() {
        let mut s = docked();
        let list = s.domestic_species();
        assert!(list.len() >= 3, "{} domestic animals", list.len());
        let c = s.credits;
        let price = s.buy_animal(list[0]).expect("buy");
        assert!((c - s.credits - price).abs() < 1e-9 && price > 0.0);
        assert_eq!(s.herd.len(), 1);
        let wild = (0..s.data.species.len())
            .find(|i| !s.data.species[*i].domestic)
            .unwrap();
        assert!(s.buy_animal(wild).is_err(), "wild species cannot be bought");
        s.credits = 1.0;
        assert!(s.buy_animal(list[0]).is_err(), "no money");
        s.credits = 1.0e6;
        while s.herd.len() < HERD_MAX {
            s.buy_animal(list[0]).unwrap();
        }
        assert!(s.buy_animal(list[0]).is_err(), "pens full");
        s.undock();
        s.herd.pop();
        assert!(s.buy_animal(list[0]).is_err(), "no dealer in space");
    }

    #[test]
    fn the_dealer_buys_animals_for_less_than_he_sells_them() {
        let mut s = docked();
        let sp = s.domestic_species()[0];
        let price = s.buy_animal(sp).unwrap();
        let c = s.credits;
        let got = s.sell_animal(0).expect("sold");
        assert!(got > 0.0 && got < price, "{got} vs {price}");
        assert_eq!(s.credits, c + got);
        assert!(s.herd.is_empty());
        assert!(s.sell_animal(0).is_err());
        s.undock();
        s.herd.push(crate::livestock::Animal {
            species: sp,
            male: true,
            born: 0.0,
            genes: [[2; 2]; 4],
            hunger: 0.0,
            last_birth: f64::MIN,
            gen: 0,
        });
        assert!(s.sell_animal(0).is_err(), "no dealer in space");
    }

    #[test]
    fn breeding_needs_an_adult_pair_and_a_rested_mother() {
        let mut s = docked();
        let (a, b) = pair(&mut s);
        assert!(s.can_breed(a, a).is_err());
        s.herd[b].male = true;
        assert!(s.can_breed(a, b).is_err(), "two males");
        s.herd[b].male = false;
        s.herd[b].species = (s.herd[b].species + 1) % s.data.species.len();
        assert!(s.can_breed(a, b).is_err(), "two species");
        s.herd[b].species = s.herd[a].species;
        s.herd[b].born = s.time;
        assert!(s.can_breed(a, b).is_err(), "too young");
        s.herd[b].born = s.time - 60.0 * DAY;
        let young = s.breed(a, b).expect("breeds");
        assert_eq!(s.herd.len(), 3);
        assert_eq!(s.herd[young].gen, 1);
        assert_eq!(s.herd[young].born, s.time);
        assert!(s.can_breed(a, b).is_err(), "the mother needs a month");
        s.time += (BREED_COOLDOWN + 1.0) * DAY;
        assert!(s.can_breed(a, b).is_ok());
        assert_eq!(s.best_partner(a), Some(b));
    }

    #[test]
    fn offspring_inherit_one_allele_from_each_parent() {
        let mut s = docked();
        let (a, b) = pair(&mut s);
        s.herd[a].genes = [[0, 0]; 4];
        s.herd[b].genes = [[4, 4]; 4];
        let mut rng = Rng::new(7);
        let mut mutated = 0;
        for _ in 0..400 {
            let c = offspring(&s.herd[a], &s.herd[b], 0.0, &mut rng);
            for t in 0..4 {
                // Without mutation: always one 0 and one 4.
                if c.genes[t][0] != 0 || c.genes[t][1] != 4 {
                    mutated += 1;
                }
                assert!(c.genes[t].iter().all(|g| *g <= ALLELE_MAX));
            }
        }
        // 3200 alleles at 2 %: some mutations, far from all.
        assert!(mutated > 3 && mutated < 100, "{mutated} mutated of 1600");
    }

    #[test]
    fn selective_breeding_improves_the_herd() {
        let mut s = docked();
        let (a, _) = pair(&mut s);
        let sp = s.herd[a].species;
        let mut rng = Rng::new(3);
        let mut pool: Vec<Animal> = (0..16)
            .map(|n| {
                let mut x = random_animal(s.galaxy.seed, sp, s.time, n);
                x.male = n % 2 == 0;
                x
            })
            .collect();
        let start = pool.iter().map(Animal::quality).sum::<f64>() / pool.len() as f64;
        for _ in 0..12 {
            // Keep the best half of each sex, breed them.
            pool.sort_by(|x, y| y.quality().total_cmp(&x.quality()));
            let males: Vec<Animal> = pool.iter().filter(|x| x.male).take(4).cloned().collect();
            let females: Vec<Animal> = pool.iter().filter(|x| !x.male).take(4).cloned().collect();
            pool.clear();
            for i in 0..16 {
                pool.push(offspring(
                    &males[i % males.len()],
                    &females[(i / 2) % females.len()],
                    0.0,
                    &mut rng,
                ));
            }
        }
        let end = pool.iter().map(Animal::quality).sum::<f64>() / pool.len() as f64;
        assert!(end > start + 0.1, "quality {start:.2} -> {end:.2}");
    }

    #[test]
    fn harvest_gives_yield_scaled_by_the_trait_and_young_animals_are_spared() {
        let mut s = docked();
        let (a, b) = pair(&mut s);
        s.herd[a].genes[0] = [0, 0];
        s.herd[b].genes[0] = [4, 4];
        let sp = s.herd[a].species;
        let (res, kg) = {
            let (r, kg) = &s.data.species[sp].yields[0];
            (s.data.resource_index(r).unwrap(), *kg)
        };
        let before = s.ship.cargo.kg[res];
        let low = s.harvest_animal(a).expect("harvest");
        let got_low = s.ship.cargo.kg[res] - before;
        assert!(
            (got_low - kg * 0.5).abs() < 1e-9,
            "{got_low} vs {}",
            kg * 0.5
        );
        assert!(low.iter().any(|(r, _)| *r == res));
        let before = s.ship.cargo.kg[res];
        s.harvest_animal(0).expect("the other");
        assert!((s.ship.cargo.kg[res] - before - kg * 1.5).abs() < 1e-9);
        assert!(s.herd.is_empty());
        let young = {
            let mut y = random_animal(s.galaxy.seed, sp, s.time, 9);
            y.born = s.time;
            y
        };
        s.herd.push(young);
        assert!(s.harvest_animal(0).is_err());
    }

    #[test]
    fn the_herd_eats_rations_and_starves_without() {
        let mut s = docked();
        pair(&mut s);
        let food = s.data.resource_index("food").unwrap();
        s.ship.cargo.kg[food] = 1000.0;
        s.ship.cargo.mass = 1000.0 * s.data.resources[food].density.min(1.0);
        s.livestock_tick();
        s.time += DAY;
        s.livestock_tick();
        assert!(s.ship.cargo.kg[food] < 1000.0, "the herd ate");
        assert_eq!(s.herd.len(), 2);
        // No more rations: hunger grows and the animals die.
        s.ship.cargo.kg[food] = 0.0;
        for _ in 0..30 {
            s.time += DAY;
            s.livestock_tick();
        }
        assert!(s.herd.is_empty(), "{} animals left", s.herd.len());
        assert!(s
            .events
            .iter()
            .any(|e| matches!(e, Event::MarketNews(t) if t.starts_with("Elevage"))));
    }
}
