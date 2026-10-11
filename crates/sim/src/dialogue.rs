//! Radio conversations with other ships (data: `data/dialogues.ron`). The ship's AI speaks, the
//! other captain answers in a tone that follows the mores of their faction; options depend on
//! the situation (reputation, cargo, credits, their values) and their effects touch the world.
use crate::crew::DAY;
use crate::Sim;
use vector_core::{hash_coord, Rng};
use vector_data::dialogue::{parse_conds, parse_effect, Cond, Effect};

/// What the screen shows of a node.
#[derive(Clone, Debug)]
pub struct DialogueView {
    pub speaker: String,
    pub faction: String,
    pub tone: &'static str,
    pub text: String,
    /// (label, available).
    pub options: Vec<(String, bool)>,
}

/// Bribe offered to a patrol, credits, and the fine when it is refused.
pub const BRIBE: f64 = 200.0;
pub const BRIBE_FINE: f64 = 300.0;

impl Sim {
    /// Tone of voice of an NPC: the strongest of authority (formal), openness (warm) and
    /// xenophobia (wary) in the values of their faction, or neutral.
    pub fn npc_tone(&self, npc: usize) -> &'static str {
        let Some(n) = self.npcs.get(npc) else {
            return "neutre";
        };
        let fid = n.faction.map(|f| self.data.factions[f].id.as_str());
        let profile = self.data.ethos.profile(fid);
        let get = |a: &str| {
            self.data
                .ethos
                .axis(a)
                .and_then(|i| profile.get(i))
                .copied()
                .unwrap_or(0.0)
        };
        let (aut, ouv) = (get("autorite"), get("ouverture"));
        let cands = [("formel", aut), ("cordial", ouv), ("mefiant", -ouv)];
        cands
            .iter()
            .filter(|(_, v)| *v >= 0.3)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map_or("neutre", |(t, _)| *t)
    }

    /// First node of the conversation with this NPC.
    pub fn dialogue_start(&self, npc: usize) -> Option<String> {
        let n = self.npcs.get(npc)?;
        self.data
            .dialogues
            .start_for(n.role.label())
            .map(str::to_string)
    }

    fn cond_holds(&self, npc: usize, c: &Cond) -> bool {
        let n = &self.npcs[npc];
        match c {
            Cond::Rep(cmp, v) => cmp.test(f64::from(self.reputation_with(n.faction)), *v),
            Cond::Credits(cmp, v) => cmp.test(self.credits, *v),
            Cond::Cargo(r, cmp, v) => self
                .data
                .resource_index(r)
                .is_some_and(|i| cmp.test(self.ship.cargo.kg[i], *v)),
            Cond::Ethos(a, cmp, v) => {
                let profile = self
                    .data
                    .ethos
                    .profile(n.faction.map(|f| self.data.factions[f].id.as_str()));
                cmp.test(
                    self.data
                        .ethos
                        .axis(a)
                        .and_then(|i| profile.get(i))
                        .copied()
                        .unwrap_or(0.0),
                    *v,
                )
            }
            Cond::Role(r) => n.role.label() == r,
            Cond::Hostile(h) => n.hostile == *h,
        }
    }

    fn option_available(&self, npc: usize, cond: &str) -> bool {
        parse_conds(cond)
            .map(|cs| cs.iter().all(|c| self.cond_holds(npc, c)))
            .unwrap_or(false)
    }

    /// The text and options of a node, for this NPC; `result` fills `{result}`.
    pub fn dialogue_view(&self, npc: usize, node: &str, result: &str) -> Option<DialogueView> {
        let n = self.npcs.get(npc)?;
        let nd = self.data.dialogues.node(node)?;
        let tone = self.npc_tone(npc);
        let raw = nd
            .text
            .iter()
            .find(|(t, _)| t == tone)
            .or_else(|| nd.text.iter().find(|(t, _)| t == "neutre"))
            .map(|(_, x)| x.as_str())?;
        let faction = n.faction.map_or_else(
            || "les indépendants".to_string(),
            |f| self.data.factions[f].name.clone(),
        );
        let cargo = n.cargo.map_or_else(
            || "rien".to_string(),
            |(r, _)| self.data.resources[r].name.to_lowercase(),
        );
        let text = raw
            .replace("{name}", &n.name)
            .replace("{faction}", &faction)
            .replace("{system}", &self.system.name)
            .replace("{cargo}", &cargo)
            .replace("{result}", result);
        Some(DialogueView {
            speaker: n.name.clone(),
            faction,
            tone,
            text,
            options: nd
                .options
                .iter()
                .map(|o| (o.label.clone(), self.option_available(npc, &o.cond)))
                .collect(),
        })
    }

    /// Choose option `opt` of `node`: apply its effects, return the next node ("end" closes the
    /// conversation, the start node when the option says "back") and the text of what happened.
    pub fn dialogue_choose(
        &mut self,
        npc: usize,
        node: &str,
        opt: usize,
    ) -> Result<(String, String), &'static str> {
        if npc >= self.npcs.len() {
            return Err("plus de contact");
        }
        let nd = self
            .data
            .dialogues
            .node(node)
            .ok_or("noeud inconnu")?
            .clone();
        let o = nd.options.get(opt).ok_or("option inconnue")?.clone();
        if !self.option_available(npc, &o.cond) {
            return Err("option indisponible");
        }
        let mut result = String::new();
        for e in &o.effects {
            let eff = parse_effect(e).map_err(|_| "effet invalide")?;
            if let Some(text) = self.apply_dialogue_effect(npc, &eff)? {
                result = text;
            }
        }
        let next = match o.next.as_str() {
            "back" => self.dialogue_start(npc).unwrap_or_else(|| "end".into()),
            other => other.to_string(),
        };
        Ok((next, result))
    }

    fn apply_dialogue_effect(
        &mut self,
        npc: usize,
        eff: &Effect,
    ) -> Result<Option<String>, &'static str> {
        let faction = self.npcs[npc].faction;
        Ok(match eff {
            Effect::Rep(x) => {
                self.change_reputation(faction, *x as f32);
                None
            }
            Effect::Credits(x) => {
                if self.credits + x < 0.0 {
                    return Err("fonds insuffisants");
                }
                self.credits += x;
                None
            }
            Effect::Give(res, kg) => {
                let data = std::sync::Arc::clone(&self.data);
                let r = data.resource_index(res).ok_or("ressource inconnue")?;
                if self.ship.cargo.kg[r] < *kg {
                    return Err("pas assez en soute");
                }
                self.ship.cargo.remove(&data, r, *kg);
                None
            }
            Effect::News => Some(self.radio_news()),
            Effect::Region => Some(self.radio_region()),
            Effect::Tip => Some(self.radio_tip()),
            Effect::Peace => Some(match self.pay_tribute() {
                Ok(a) => format!("Tribut de {a:.0} CR payé. Les pirates s'éloignent."),
                Err(e) => format!("Rien à payer : {e}."),
            }),
            Effect::Hostile => {
                self.npcs[npc].hostile = true;
                None
            }
            Effect::Bluff => Some(self.bluff(npc)),
            Effect::Bribe => Some(self.bribe(npc)),
        })
    }

    fn radio_news(&self) -> String {
        match self.market_news(30.0).first() {
            Some(ev) => {
                let d = &self.data.world_events[ev.def];
                format!("{} : {}", d.name, d.text)
            }
            None => "Rien de spécial dans la région, les affaires suivent leur cours.".to_string(),
        }
    }

    fn radio_tip(&self) -> String {
        let Some(ev) = self.market_news(30.0).into_iter().next() else {
            return "Aucun mouvement notable ; les prix tiennent.".to_string();
        };
        let d = &self.data.world_events[ev.def];
        match d.effects.first() {
            Some((k, m)) => {
                let what = match k.strip_prefix("cat:") {
                    Some(c) => format!("les marchandises de type {c}"),
                    None => self
                        .data
                        .resource_index(k)
                        .map_or_else(|| k.clone(), |i| self.data.resources[i].name.clone()),
                };
                format!(
                    "À cause de « {} », {} {} (x{:.2}).",
                    d.name,
                    what.to_lowercase(),
                    if *m > 1.0 {
                        "s'arrachent"
                    } else {
                        "se bradent"
                    },
                    m
                )
            }
            None => "Aucun mouvement notable ; les prix tiennent.".to_string(),
        }
    }

    fn radio_region(&self) -> String {
        let here = self.here;
        let near = self
            .galaxy
            .nearby(self.here_pos, 40.0)
            .into_iter()
            .filter(|e| e.id != here)
            .min_by(|a, b| {
                a.distance_to(self.here_pos)
                    .total_cmp(&b.distance_to(self.here_pos))
            });
        match near {
            Some(e) => {
                let owner = self
                    .galaxy
                    .territory(e.pos, self.data.factions.len())
                    .map_or("sans maître".to_string(), |f| {
                        format!("territoire de {}", self.data.factions[f].name)
                    });
                format!(
                    "L'étoile {} brille à {:.1} années-lumière, {}.",
                    e.header.name,
                    e.distance_to(self.here_pos),
                    owner
                )
            }
            None => "Personne n'ose s'aventurer plus loin d'ici.".to_string(),
        }
    }

    /// A pirate bluff: the better the crew's leader, the likelier the pirates give up.
    fn bluff(&mut self, npc: usize) -> String {
        let lead = self.data.skill_index("commandement");
        let best = lead.map_or(0.0, |s| {
            self.crew
                .members
                .iter()
                .filter(|p| p.alive)
                .map(|p| f64::from(p.skills[s]))
                .fold(0.0, f64::max)
        });
        let chance = 0.25 + 0.06 * best;
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            i64::from(self.npcs[npc].id),
            self.time as i64,
            23,
        ));
        if rng.f64() < chance {
            self.tribute_due = None;
            self.end_combat(npc);
            self.npcs[npc].last_scan = self.time + 2.0 * DAY;
            "Les pirates hésitent... puis rompent le contact.".to_string()
        } else {
            self.npcs[npc].hostile = true;
            "Ils éclatent de rire : vous bluffez. Ils ouvrent le feu !".to_string()
        }
    }

    /// Slip a patrol a bribe: honourable factions are offended and fine you.
    fn bribe(&mut self, npc: usize) -> String {
        let faction = self.npcs[npc].faction;
        let honour = self
            .data
            .ethos
            .profile(faction.map(|f| self.data.factions[f].id.as_str()))
            .get(self.data.ethos.axis("honneur").unwrap_or(usize::MAX))
            .copied()
            .unwrap_or(0.0);
        if honour > 0.3 {
            self.credits = (self.credits - BRIBE_FINE).max(0.0);
            self.change_reputation(faction, -0.2);
            format!("Tentative de corruption ! Amende de {BRIBE_FINE:.0} CR et rapport au commandement.")
        } else {
            self.credits = (self.credits - BRIBE).max(0.0);
            self.change_reputation(faction, -0.05);
            "L'agent empoche le pot-de-vin et abrège le contrôle.".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::NpcRole;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn sim() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let d = Arc::new(GameData::load(&dir).expect("data loads"));
        let mut s = new_sim(11, d);
        s.spawn_traffic();
        s.credits = 5_000.0;
        s
    }

    fn npc_of(s: &Sim, role: NpcRole) -> usize {
        s.npcs
            .iter()
            .position(|n| n.role == role)
            .unwrap_or_else(|| panic!("a {role:?} in the system"))
    }

    fn set_faction(s: &mut Sim, npc: usize, id: &str) {
        let f = s
            .data
            .factions
            .iter()
            .position(|f| f.id == id)
            .expect("faction");
        s.npcs[npc].faction = Some(f);
    }

    #[test]
    fn tone_follows_the_mores_of_the_faction() {
        let mut s = sim();
        let k = npc_of(&s, NpcRole::Trader);
        set_faction(&mut s, k, "hegemonie");
        assert_eq!(
            s.npc_tone(k),
            "formel",
            "an authoritarian faction is formal"
        );
        set_faction(&mut s, k, "concorde");
        assert_eq!(s.npc_tone(k), "cordial", "an open faction is warm");
        s.npcs[k].faction = None;
        assert_eq!(s.npc_tone(k), "neutre");
        // The same node reads differently.
        let start = s.dialogue_start(k).expect("start");
        let neutral = s.dialogue_view(k, &start, "").expect("view").text;
        set_faction(&mut s, k, "hegemonie");
        let formal = s.dialogue_view(k, &start, "").expect("view").text;
        assert_ne!(neutral, formal);
        assert!(formal.contains("Identifiez-vous"));
    }

    #[test]
    fn options_depend_on_cargo_and_gifts_buy_goodwill_and_cost_goods() {
        let mut s = sim();
        let k = npc_of(&s, NpcRole::Trader);
        set_faction(&mut s, k, "concorde");
        let start = s.dialogue_start(k).expect("start");
        let gift = s
            .data
            .dialogues
            .node(&start)
            .unwrap()
            .options
            .iter()
            .position(|o| o.label.contains("vivres"))
            .expect("gift option");
        let d = Arc::clone(&s.data);
        let food = d.resource_index("food").unwrap();
        let have = s.ship.cargo.kg[food];
        s.ship.cargo.remove(&d, food, have);
        let view = s.dialogue_view(k, &start, "").unwrap();
        assert!(!view.options[gift].1, "no food, no gift");
        assert!(s.dialogue_choose(k, &start, gift).is_err());
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        s.ship.cargo.add(&d, food, 100.0);
        assert!(s.dialogue_view(k, &start, "").unwrap().options[gift].1);
        let rep = s.reputation_with(s.npcs[k].faction);
        let (next, _) = s.dialogue_choose(k, &start, gift).expect("gift");
        assert_eq!(next, "gift_reply");
        assert!(s.reputation_with(s.npcs[k].faction) > rep);
        assert!((s.ship.cargo.kg[food] - 80.0).abs() < 1e-6);
    }

    #[test]
    fn bribes_work_on_the_corruptible_and_offend_the_honourable() {
        let mut s = sim();
        let k = npc_of(&s, NpcRole::Patrol);
        let start = s.dialogue_start(k).expect("start");
        let bribe = s
            .data
            .dialogues
            .node(&start)
            .unwrap()
            .options
            .iter()
            .position(|o| o.effects.iter().any(|e| e == "bribe"))
            .expect("bribe option");
        // "ligue" is not honourable (-0.3), "hegemonie" is (0.7).
        set_faction(&mut s, k, "ligue");
        let credits = s.credits;
        let (_, text) = s.dialogue_choose(k, &start, bribe).expect("bribe");
        assert_eq!(s.credits, credits - BRIBE, "{text}");
        set_faction(&mut s, k, "hegemonie");
        let rep = s.reputation_with(s.npcs[k].faction);
        let credits = s.credits;
        let (_, text) = s.dialogue_choose(k, &start, bribe).expect("bribe");
        assert_eq!(s.credits, credits - BRIBE_FINE, "{text}");
        assert!(s.reputation_with(s.npcs[k].faction) < rep - 0.15);
    }

    #[test]
    fn pirates_can_be_paid_or_provoked_and_every_start_node_exists() {
        let mut s = sim();
        for role in [NpcRole::Trader, NpcRole::Miner, NpcRole::Patrol] {
            let k = npc_of(&s, role);
            let start = s.dialogue_start(k).expect("start for every role");
            assert!(s.dialogue_view(k, &start, "").is_some(), "{role:?}");
        }
        let k = npc_of(&s, NpcRole::Pirate);
        let start = s.dialogue_start(k).expect("pirate start");
        let nd = s.data.dialogues.node(&start).unwrap().clone();
        let insult = nd
            .options
            .iter()
            .position(|o| o.label == "Insulter")
            .unwrap();
        s.npcs[k].hostile = false;
        s.dialogue_choose(k, &start, insult).expect("insult");
        assert!(s.npcs[k].hostile, "insulting pirates makes them hostile");
        // With nobody asking for tribute, "pay" is harmless and says so.
        let pay = nd
            .options
            .iter()
            .position(|o| o.label.contains("tribut"))
            .unwrap();
        s.tribute_due = None;
        let (next, text) = s.dialogue_choose(k, &start, pay).expect("pay");
        assert_eq!(next, "pirate_paid");
        assert!(text.contains("Rien à payer"), "{text}");
    }
}
