//! Personal equipment: a weapon, an armour and a tool per person, taken from the hold (where the
//! items are trade goods) and put back there when removed or replaced.
use crate::Sim;

impl Sim {
    fn gear_item(&self, gear: usize) -> Option<usize> {
        let g = self.data.gear.get(gear)?;
        self.data.resource_index(&format!("gear_{}", g.id))
    }

    /// Equip `gear` (an index into `GameData::gear`) on crew member `member`; the item comes from
    /// the hold and what was worn in that slot goes back there.
    pub fn equip(&mut self, member: usize, gear: usize) -> Result<String, &'static str> {
        let data = std::sync::Arc::clone(&self.data);
        let p = self.crew.members.get(member).ok_or("membre inconnu")?;
        if !p.alive {
            return Err("le membre est mort");
        }
        let def = data.gear.get(gear).ok_or("equipement inconnu")?;
        let item = self.gear_item(gear).ok_or("piece inconnue")?;
        if self.ship.cargo.kg[item] < 1.0 {
            return Err("l'objet n'est pas en soute");
        }
        let slot = def.slot_index();
        let old = p.gear.get(slot).copied().flatten();
        self.ship.cargo.remove(&data, item, 1.0);
        if let Some(o) = old {
            // The old one goes back to the hold (there is room: the new one just left it).
            if let Some(oi) = self.gear_item(o) {
                self.ship.cargo.add(&data, oi, 1.0);
            }
        }
        let p = &mut self.crew.members[member];
        p.gear.resize(3, None);
        p.gear[slot] = Some(gear);
        Ok(format!("{} : {} equipe", p.name, def.name))
    }

    /// Take off whatever `member` wears in `slot` (0 weapon, 1 armour, 2 tool).
    pub fn unequip(&mut self, member: usize, slot: usize) -> Result<String, &'static str> {
        let data = std::sync::Arc::clone(&self.data);
        let p = self.crew.members.get(member).ok_or("membre inconnu")?;
        let g = p
            .gear
            .get(slot)
            .copied()
            .flatten()
            .ok_or("rien a retirer")?;
        let item = self.gear_item(g).ok_or("piece inconnue")?;
        if self.ship.cargo.add(&data, item, 1.0) < 1.0 {
            return Err("soute pleine");
        }
        let p = &mut self.crew.members[member];
        p.gear[slot] = None;
        Ok(format!("{} : {} retire", p.name, data.gear[g].name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn sim() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let mut s = new_sim(5, Arc::new(GameData::load(&dir).expect("data loads")));
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        s
    }

    fn give(s: &mut Sim, id: &str) {
        let d = Arc::clone(&s.data);
        let r = d.resource_index(&format!("gear_{id}")).expect("item");
        s.ship.cargo.add(&d, r, 1.0);
    }

    #[test]
    fn equipping_takes_the_item_from_the_hold_and_swapping_gives_the_old_one_back() {
        let mut s = sim();
        let d = Arc::clone(&s.data);
        let knife = d.gear_index("couteau").unwrap();
        let rifle = d.gear_index("fusil").unwrap();
        assert!(s.equip(0, knife).is_err(), "not in the hold");
        give(&mut s, "couteau");
        s.equip(0, knife).expect("equip");
        let (ki, ri) = (
            d.resource_index("gear_couteau").unwrap(),
            d.resource_index("gear_fusil").unwrap(),
        );
        assert!(s.ship.cargo.kg[ki] < 0.5);
        assert_eq!(s.crew.members[0].gear[0], Some(knife));
        let base = s.crew.members[0].gear_stat(&d, "attack");
        assert!((base - d.gear[knife].attack).abs() < 1e-9);
        // The rifle replaces the knife in the same slot; the knife is back in the hold.
        give(&mut s, "fusil");
        s.equip(0, rifle).expect("swap");
        assert_eq!(s.crew.members[0].gear[0], Some(rifle));
        assert!(s.ship.cargo.kg[ki] >= 0.99 && s.ship.cargo.kg[ri] < 0.5);
        assert!(s.crew.members[0].gear_stat(&d, "attack") > base);
        // Three different slots add up.
        give(&mut s, "exosquelette");
        give(&mut s, "pioche");
        s.equip(0, d.gear_index("exosquelette").unwrap()).unwrap();
        s.equip(0, d.gear_index("pioche").unwrap()).unwrap();
        assert!(s.crew.members[0].gear_stat(&d, "defense") >= 0.5);
        assert!(s.crew.members[0].gear_stat(&d, "gather") >= 0.4);
        // Taking off puts it back in the hold.
        s.unequip(0, 1).expect("take off");
        assert!(s.crew.members[0].gear[1].is_none());
        assert!(s.unequip(0, 1).is_err(), "nothing left in that slot");
    }
}
