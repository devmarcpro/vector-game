//! The black market: where a good is banned (by the mores or the contraband list of the people
//! whose station you are at) a contact trades it anyway, at a premium, away from the customs.
//! Every deal may end in a raid: the goods and a fine are lost and the people's trust with them.
//! It is a second door on the SAME physical stock of the station as the open market (`market.rs`):
//! the contact sells only what the station holds, and what you sell him adds to it; the price
//! is the station's own (scarcity included) with the premium and the risk of the back door.
use crate::relations::Entity;
use crate::{Event, Sim};
use vector_core::{hash_coord, Rng};

/// What the contact pays you, relative to the reference price.
pub const SELL_FACTOR: f64 = 1.35;
/// What the contact asks of you, relative to the reference price.
pub const BUY_FACTOR: f64 = 1.9;
/// Chance that a deal ends in a raid.
pub const RAID_CHANCE: f64 = 0.12;
pub const RAID_FINE: f64 = 600.0;

/// Trust of a contact grows with every clean deal and falls with a raid; it improves the terms.
pub const TRUST_STEP: f32 = 0.04;
pub const TRUST_RAID: f32 = 0.3;
/// Reputation lost with the local people at each clean deal (old -100..100 scale).
pub const DEAL_STAIN: f32 = 0.3;

impl Sim {
    /// Trust (0..1) of the contact of the people whose station this is.
    pub fn contact_trust(&self) -> f32 {
        self.faction
            .map_or(0.0, |f| self.rel(Entity::contact(f)).clamp(0.0, 1.0))
    }

    fn set_trust(&mut self, delta: f32) {
        let Some(f) = self.faction else { return };
        self.relate_alone(Entity::contact(f), delta, false);
    }

    /// A clean deal is still smuggling: the authorities like you a little less, and so do their
    /// friends; their enemies, who care nothing for their laws, a little more.
    fn stain(&mut self) {
        self.change_reputation(self.faction, -DEAL_STAIN);
    }

    /// (price asked per reference unit, price paid, chance of a raid) with the contact's trust.
    pub fn black_terms(&self) -> (f64, f64, f64) {
        let t = f64::from(self.contact_trust());
        (
            BUY_FACTOR - 0.3 * t,
            SELL_FACTOR + 0.1 * t,
            RAID_CHANCE * (1.0 - 0.7 * t),
        )
    }

    /// Mid price of the open market of the station where you are docked (credits per kg): the
    /// base on which the contact puts his premium.
    fn black_base_price(&mut self, res: usize) -> Result<f64, &'static str> {
        let st = self.docked_station()?;
        let data = std::sync::Arc::clone(&self.data);
        Ok(self.market(st).price(&data, res))
    }

    /// Whether the good is banned where you are docked (so a contact deals it).
    pub fn black_market_for(&self, res: usize) -> Result<(), &'static str> {
        self.docked_station()?;
        let f = self
            .faction
            .ok_or("espace libre : ici tout se vend au grand jour")?;
        if self.banned_goods(f).contains(&res) {
            Ok(())
        } else {
            Err("ici ce produit est libre : pas besoin du marche noir")
        }
    }

    fn raid(&mut self, res: usize, kg: f64) -> &'static str {
        let fine = self.credits.clamp(0.0, RAID_FINE);
        self.credits -= fine;
        self.change_reputation(self.faction, -0.25);
        self.offend(self.faction, "contrebande");
        self.set_trust(-TRUST_RAID);
        let name = self.data.resources[res].name.to_lowercase();
        self.events.push(Event::MarketNews(format!(
            "Descente de police : {kg:.0} kg de {name} saisis, amende de {fine:.0} CR."
        )));
        "descente de police"
    }

    fn deal_raided(&mut self, res: usize, kg: f64) -> bool {
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            res as i64,
            self.time as i64,
            (kg * 10.0) as i64,
        ));
        rng.f64() < self.black_terms().2
    }

    /// Sell `kg` kg of a banned good to the contact; returns what was paid.
    pub fn black_sell(&mut self, res: usize, kg: f64) -> Result<f64, &'static str> {
        self.black_market_for(res)?;
        let kg = kg.min(self.ship.cargo.kg[res]);
        if kg <= 0.0 {
            return Err("rien a vendre");
        }
        let data = std::sync::Arc::clone(&self.data);
        self.ship.cargo.remove(&data, res, kg);
        if self.deal_raided(res, kg) {
            return Err(self.raid(res, kg));
        }
        let pay = self.black_base_price(res)? * kg * self.black_terms().1;
        let st = self.docked_station()?;
        self.market(st).stock[res] += kg;
        self.credits += pay;
        self.bank.dirty += pay;
        self.set_trust(TRUST_STEP);
        self.stain();
        Ok(pay)
    }

    /// Buy `kg` kg of a banned good from the contact; returns what it cost.
    pub fn black_buy(&mut self, res: usize, kg: f64) -> Result<f64, &'static str> {
        self.black_market_for(res)?;
        let price = self.black_base_price(res)? * self.black_terms().0;
        let st = self.docked_station()?;
        let room = self.ship.cargo.cap_mass - self.ship.cargo.mass;
        let held = self.market(st).stock[res];
        let kg = kg.min(self.credits / price).min(room).min(held);
        if kg <= 0.0 {
            return Err("achat impossible (credits, soute ou stock du contact)");
        }
        self.credits -= price * kg;
        // The goods leave the shelves of the station, seized or not.
        self.market(st).stock[res] -= kg;
        if self.deal_raided(res, kg) {
            return Err(self.raid(res, kg));
        }
        let data = std::sync::Arc::clone(&self.data);
        let got = self.ship.cargo.add(&data, res, kg);
        // Whatever the hold could not take is paid back, and goes back on the shelves.
        self.credits += price * (kg - got);
        self.market(st).stock[res] += kg - got;
        self.set_trust(TRUST_STEP);
        self.stain();
        Ok(price * got)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn docked(faction: &str) -> (Sim, usize) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let mut s = new_sim(5, Arc::new(GameData::load(&dir).expect("data loads")));
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().expect("dock");
        s.credits = 100_000.0;
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        let f = s
            .data
            .factions
            .iter()
            .position(|x| x.id == faction)
            .expect("faction");
        s.faction = Some(f);
        let banned = s.banned_goods(f);
        (s, *banned.first().expect("a banned good"))
    }

    #[test]
    fn only_banned_goods_have_a_black_market_and_it_pays_a_premium() {
        let (mut s, res) = docked("verdant");
        let d = Arc::clone(&s.data);
        let free = (0..d.resources.len())
            .find(|&r| !s.banned_goods(s.faction.unwrap()).contains(&r))
            .unwrap();
        assert!(
            s.black_market_for(free).is_err(),
            "free goods go to the open market"
        );
        assert!(s.black_market_for(res).is_ok());
        s.faction = None;
        assert!(
            s.black_market_for(res).is_err(),
            "no black market in free space"
        );
        s.faction = Some(d.factions.iter().position(|x| x.id == "verdant").unwrap());
        // Over many deals: most pass at the premium, some end in a raid.
        let (mut ok, mut raided) = (0, 0);
        for t in 0..200 {
            s.time = f64::from(t) * 3600.0;
            s.credits = 1.0e6;
            let st = s.docked_station().unwrap();
            let data = Arc::clone(&s.data);
            let reference = s.market(st).price(&data, res);
            let credits = s.credits;
            let factor = s.black_terms().0;
            match s.black_buy(res, 10.0) {
                Ok(cost) => {
                    ok += 1;
                    assert!((cost - reference * factor * 10.0).abs() < 1e-6);
                    assert!((credits - s.credits - cost).abs() < 1e-6);
                    let pay = s.black_sell(res, 10.0);
                    if let Ok(p) = pay {
                        assert!(
                            p > cost * 0.6 && p < cost,
                            "{p} vs {cost}: premium both ways, net loss on a round trip"
                        );
                    }
                }
                Err(e) => {
                    assert_eq!(e, "descente de police");
                    raided += 1;
                }
            }
            s.ship.cargo.kg[res] = 0.0;
            s.ship.cargo.mass = 0.0;
            s.ship.cargo.volume = 0.0;
        }
        let rate = f64::from(raided) / f64::from(ok + raided);
        assert!(
            raided > 0 && (rate - RAID_CHANCE).abs() < 0.08,
            "raid rate {rate}"
        );
    }

    #[test]
    fn the_black_market_is_a_second_door_on_the_stock_of_the_station() {
        // Audit B15: one physical stock; the contact sells what the station holds and what he
        // buys goes on its shelves, at the price the station itself asks.
        let (mut s, res) = docked("verdant");
        let st = s.docked_station().unwrap();
        let data = Arc::clone(&s.data);
        s.ship.cargo.cap_mass = 1.0e9;
        s.ship.cargo.cap_vol = 1.0e9;
        s.credits = 1.0e12;
        // He cannot sell what the station has not got.
        s.market(st).stock[res] = 0.0;
        s.market(st).target[res] = 0.0;
        s.ship.cargo.kg[res] = 0.0;
        assert!(s.black_buy(res, 10.0).is_err(), "nothing on the shelves");
        // With stock he sells, and the stock goes down by what he sold (a raid takes it too).
        s.market(st).stock[res] = 500.0;
        s.market(st).target[res] = 500.0;
        let mut taken = 0.0;
        for t in 0..30 {
            s.time += 60.0 * f64::from(t % 2 + 1);
            let before = s.market(st).stock[res];
            let had = s.ship.cargo.kg[res];
            if s.black_buy(res, 20.0).is_ok() {
                taken += s.ship.cargo.kg[res] - had;
                assert!(s.market(st).stock[res] < before + 1e-9);
            }
        }
        assert!(taken > 0.0);
        // The shelves are the same ones the open market reads: the price rises with the shortage.
        let reference = s.market(st).reference[res];
        s.market(st).stock[res] = 0.2 * reference;
        let dear = s.market(st).price(&data, res);
        s.market(st).stock[res] = 0.8 * reference;
        assert!(s.market(st).price(&data, res) < dear);
        // What he buys from you is added to them.
        s.ship.cargo.add(&data, res, 30.0);
        let mut sold = false;
        for t in 0..30 {
            s.time += 1000.0 * f64::from(t);
            let before = s.market(st).stock[res];
            if s.black_sell(res, 10.0).is_ok() {
                let after = s.market(st).stock[res];
                assert!((after - before - 10.0).abs() < 1e-6, "{before} -> {after}");
                sold = true;
                break;
            }
        }
        assert!(sold);
    }

    #[test]
    fn a_clean_deal_builds_the_contact_and_stains_the_standing_with_echoes_on_rivals() {
        let (mut s, res) = docked("verdant");
        let f = s.faction.unwrap();
        let foe = s.data.factions.iter().position(|x| x.id == "hegemonie");
        let enemies: Vec<usize> = s.data.factions[f]
            .enemies
            .iter()
            .filter_map(|id| s.data.factions.iter().position(|x| x.id == *id))
            .collect();
        s.ship.cargo.add(&std::sync::Arc::clone(&s.data), res, 50.0);
        let (trust, rep) = (s.contact_trust(), s.reputation_with(Some(f)));
        let enemy_before: Vec<f32> = enemies
            .iter()
            .map(|e| s.reputation_with(Some(*e)))
            .collect();
        let mut done = false;
        for t in 0..40 {
            s.time += f64::from(t) * 977.0;
            if s.black_sell(res, 10.0).is_ok() {
                done = true;
                break;
            }
        }
        assert!(done, "a deal went through");
        assert!(s.contact_trust() > trust, "the contact trusts you more");
        assert!(
            s.reputation_with(Some(f)) < rep,
            "but the authorities like you less"
        );
        if !enemies.is_empty() {
            let grew = enemies
                .iter()
                .zip(&enemy_before)
                .any(|(e, b)| s.reputation_with(Some(*e)) > *b);
            assert!(grew, "their enemies take note kindly {foe:?}");
        }
    }

    #[test]
    fn a_raid_costs_a_fine_the_goods_and_reputation() {
        let (mut s, res) = docked("verdant");
        let f = s.faction;
        let rep = s.reputation_with(f);
        s.credits = 5_000.0;
        let before = s.credits;
        let msg = s.raid(res, 25.0);
        assert_eq!(msg, "descente de police");
        assert_eq!(s.credits, before - RAID_FINE);
        assert!(s.reputation_with(f) < rep - 0.2);
        assert!(s
            .events
            .iter()
            .any(|e| matches!(e, Event::MarketNews(t) if t.starts_with("Descente"))));
    }

    #[test]
    fn contacts_trust_grows_with_clean_deals_improves_terms_and_falls_with_raids() {
        let (mut s, res) = docked("verdant");
        assert_eq!(s.contact_trust(), 0.0);
        let (b0, s0, r0) = s.black_terms();
        let mut clean = 0;
        for t in 0..60 {
            s.time = f64::from(t) * 7200.0;
            s.credits = 1.0e6;
            s.ship.cargo.kg[res] = 0.0;
            if s.black_buy(res, 5.0).is_ok() {
                clean += 1;
            }
        }
        assert!(clean > 20);
        let trust = s.contact_trust();
        assert!(trust > 0.15, "trust {trust}");
        let (b1, s1, r1) = s.black_terms();
        assert!(b1 < b0 && s1 > s0 && r1 < r0, "better terms for a friend");
        assert!(b1 > s1, "but a round trip never pays");
        // A raid costs trust.
        let before = s.contact_trust();
        s.raid(res, 5.0);
        assert!((before - s.contact_trust() - TRUST_RAID).abs() < 1e-6 || s.contact_trust() == 0.0);
        // Another people's contact knows nothing of you.
        s.faction = s.data.factions.iter().position(|x| x.id == "concorde");
        assert_eq!(s.contact_trust(), 0.0);
    }
}
