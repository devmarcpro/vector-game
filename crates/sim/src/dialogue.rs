//! Radio conversations with other ships (data: `data/dialogues.ron`). The ship's AI speaks, the
//! other captain answers in a tone that follows the mores of their faction; options depend on
//! the situation (reputation, cargo, credits, their values) and their effects touch the world.
use crate::civ::CIV_SUBJECT;
use crate::crew::DAY;
use crate::Sim;
use vector_core::{hash_coord, Rng};
use vector_data::dialogue::{parse_conds, parse_effect, Cond, Effect};

/// What the screen shows of a node.
#[derive(Clone, Debug)]
pub struct DialogueView {
    pub speaker: String,
    pub faction: String,
    /// Ship type or "CIVILISATION".
    pub role: &'static str,
    /// Their goodwill towards you.
    pub rep: f32,
    pub tone: &'static str,
    pub text: String,
    /// (label, available).
    pub options: Vec<(String, bool)>,
}

/// Bribe offered to a patrol, credits, and the fine when it is refused.
pub const BRIBE: f64 = 200.0;
pub const BRIBE_FINE: f64 = 300.0;

/// Whoever the AI talks to, seen the same way: a ship, or the people of a world.
struct Speaker {
    name: String,
    faction: String,
    role: &'static str,
    hostile: bool,
    cargo: String,
    /// Mores on the ethos axes.
    profile: Vec<f64>,
    rep: f32,
}

impl Sim {
    fn speaker(&self, subject: usize) -> Option<Speaker> {
        if subject >= CIV_SUBJECT {
            let body = subject - CIV_SUBJECT;
            let c = self.civilisation(body)?;
            return Some(Speaker {
                name: c.name,
                faction: self.data.civilisations[c.kind].name.clone(),
                role: "CIVILISATION",
                hostile: false,
                cargo: "rien".to_string(),
                profile: c.profile,
                rep: self.civ_reputation(body),
            });
        }
        let n = self.npcs.get(subject)?;
        Some(Speaker {
            name: n.name.clone(),
            faction: n.faction.map_or_else(
                || "les indépendants".to_string(),
                |f| self.data.factions[f].name.clone(),
            ),
            role: n.role.label(),
            hostile: n.hostile,
            cargo: n.cargo.map_or_else(
                || "rien".to_string(),
                |(r, _)| self.data.resources[r].name.to_lowercase(),
            ),
            profile: self
                .data
                .ethos
                .profile(n.faction.map(|f| self.data.factions[f].id.as_str())),
            rep: self.reputation_with(n.faction),
        })
    }

    fn mores_of(&self, sp: &Speaker, axis: &str) -> f64 {
        self.data
            .ethos
            .axis(axis)
            .and_then(|i| sp.profile.get(i))
            .copied()
            .unwrap_or(0.0)
    }

    fn tone_of(&self, sp: &Speaker) -> &'static str {
        let (aut, ouv) = (
            self.mores_of(sp, "autorite"),
            self.mores_of(sp, "ouverture"),
        );
        let cands = [("formel", aut), ("cordial", ouv), ("mefiant", -ouv)];
        cands
            .iter()
            .filter(|(_, v)| *v >= 0.3)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map_or("neutre", |(t, _)| *t)
    }

    /// Tone of voice of whoever is spoken to: the strongest of authority (formal), openness
    /// (warm) and xenophobia (wary) in their mores, or neutral.
    pub fn npc_tone(&self, subject: usize) -> &'static str {
        self.speaker(subject)
            .map_or("neutre", |sp| self.tone_of(&sp))
    }

    /// First node of the conversation with this subject.
    pub fn dialogue_start(&self, subject: usize) -> Option<String> {
        let sp = self.speaker(subject)?;
        self.data.dialogues.start_for(sp.role).map(str::to_string)
    }

    fn cond_holds(&self, sp: &Speaker, c: &Cond) -> bool {
        match c {
            Cond::Rep(cmp, v) => cmp.test(f64::from(sp.rep), *v),
            Cond::Credits(cmp, v) => cmp.test(self.credits, *v),
            Cond::Cargo(r, cmp, v) => self
                .data
                .resource_index(r)
                .is_some_and(|i| cmp.test(self.ship.cargo.kg[i], *v)),
            Cond::Ethos(a, cmp, v) => cmp.test(self.mores_of(sp, a), *v),
            Cond::Role(r) => sp.role == r,
            Cond::Hostile(h) => sp.hostile == *h,
        }
    }

    fn option_available(&self, sp: &Speaker, cond: &str) -> bool {
        parse_conds(cond)
            .map(|cs| cs.iter().all(|c| self.cond_holds(sp, c)))
            .unwrap_or(false)
    }

    /// The text and options of a node, for this subject; `result` fills `{result}`.
    pub fn dialogue_view(&self, subject: usize, node: &str, result: &str) -> Option<DialogueView> {
        let sp = self.speaker(subject)?;
        let nd = self.data.dialogues.node(node)?;
        let tone = self.tone_of(&sp);
        let raw = nd
            .text
            .iter()
            .find(|(t, _)| t == tone)
            .or_else(|| nd.text.iter().find(|(t, _)| t == "neutre"))
            .map(|(_, x)| x.as_str())?;
        let text = raw
            .replace("{name}", &sp.name)
            .replace("{faction}", &sp.faction)
            .replace("{system}", &self.system.name)
            .replace("{cargo}", &sp.cargo)
            .replace("{result}", result);
        Some(DialogueView {
            speaker: sp.name.clone(),
            faction: sp.faction.clone(),
            role: sp.role,
            rep: sp.rep,
            tone,
            text,
            options: nd
                .options
                .iter()
                .map(|o| (o.label.clone(), self.option_available(&sp, &o.cond)))
                .collect(),
        })
    }

    /// Choose option `opt` of `node`: apply its effects, return the next node ("end" closes the
    /// conversation, the start node when the option says "back") and the text of what happened.
    pub fn dialogue_choose(
        &mut self,
        subject: usize,
        node: &str,
        opt: usize,
    ) -> Result<(String, String), &'static str> {
        let sp = self.speaker(subject).ok_or("plus de contact")?;
        let nd = self
            .data
            .dialogues
            .node(node)
            .ok_or("noeud inconnu")?
            .clone();
        let o = nd.options.get(opt).ok_or("option inconnue")?.clone();
        if !self.option_available(&sp, &o.cond) {
            return Err("option indisponible");
        }
        let mut result = String::new();
        for e in &o.effects {
            let eff = parse_effect(e).map_err(|_| "effet invalide")?;
            if let Some(text) = self.apply_dialogue_effect(subject, &eff)? {
                result = text;
            }
        }
        let next = match o.next.as_str() {
            "back" => self.dialogue_start(subject).unwrap_or_else(|| "end".into()),
            other => other.to_string(),
        };
        Ok((next, result))
    }

    pub(crate) fn apply_dialogue_effect(
        &mut self,
        subject: usize,
        eff: &Effect,
    ) -> Result<Option<String>, &'static str> {
        let civ_body = (subject >= CIV_SUBJECT).then(|| subject - CIV_SUBJECT);
        // Ship-only effects do nothing when the people of a world are the ones talking.
        let npc = civ_body.is_none().then_some(subject);
        Ok(match eff {
            Effect::Rep(x) => {
                match civ_body {
                    Some(b) => self.change_civ_reputation(b, *x as f32),
                    None => {
                        let faction = self.npcs[subject].faction;
                        self.change_reputation(faction, *x as f32);
                    }
                }
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
            Effect::Ore => Some(self.radio_ore()),
            Effect::Peace => Some(match self.pay_tribute() {
                Ok(a) => format!("Tribut de {a:.0} CR payé. Les pirates s'éloignent."),
                Err(e) => format!("Rien à payer : {e}."),
            }),
            Effect::Hostile => {
                if let Some(n) = npc {
                    self.npcs[n].hostile = true;
                }
                None
            }
            Effect::Bluff => npc.map(|n| self.bluff(n)),
            Effect::Bribe => npc.map(|n| self.bribe(n)),
            Effect::Mores => civ_body.map(|b| self.describe_mores(b)),
            Effect::CivGift => civ_body.map(|b| self.civ_gift(b)),
        })
    }

    /// What the AI learns of the mores of a world's people: the axes on which they are extreme.
    fn describe_mores(&self, body: usize) -> String {
        let Some(c) = self.civilisation(body) else {
            return "Aucune réponse.".to_string();
        };
        let mut words: Vec<(f64, String)> = Vec::new();
        for (a, v) in self.data.ethos.axes.iter().zip(&c.profile) {
            if v.abs() >= 0.4 {
                words.push((
                    v.abs(),
                    if *v > 0.0 {
                        a.high.clone()
                    } else {
                        a.low.clone()
                    },
                ));
            }
        }
        words.sort_by(|a, b| b.0.total_cmp(&a.0));
        let list: Vec<String> = words.into_iter().take(4).map(|(_, w)| w).collect();
        format!(
            "{} Ils sont {}.",
            self.data.civilisations[c.kind].plan,
            if list.is_empty() {
                "difficiles à cerner".to_string()
            } else {
                list.join(", ")
            }
        )
    }

    /// A friendly civilisation gives a share of what it makes; it costs it some goodwill.
    fn civ_gift(&mut self, body: usize) -> String {
        let Some(c) = self.civilisation(body) else {
            return "Aucune réponse.".to_string();
        };
        let offers = &self.data.civilisations[c.kind].offers;
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            body as i64,
            (self.time / DAY) as i64,
            29,
        ));
        let pick = &offers[(rng.next_u64() % offers.len() as u64) as usize];
        let data = std::sync::Arc::clone(&self.data);
        let Some(r) = data.resource_index(pick) else {
            return "Ils n'ont rien à offrir.".to_string();
        };
        let kg = 12.0 * f64::from(c.tech);
        let got = self.ship.cargo.add(&data, r, kg);
        self.change_civ_reputation(body, -0.1);
        if got <= 0.0 {
            "Votre soute est pleine : ils reprennent leur présent.".to_string()
        } else {
            format!(
                "Ils vous offrent {got:.0} kg de {}.",
                data.resources[r].name.to_lowercase()
            )
        }
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

    /// The richest asteroid of the system by the value of what it holds.
    fn radio_ore(&self) -> String {
        let mut best: Option<(f64, usize)> = None;
        for (i, b) in self.system.bodies.iter().enumerate() {
            let vector_world::BodyKind::Asteroid(class) = b.kind else {
                continue;
            };
            let Some(cls) = self.data.asteroids.iter().find(|a| a.id == class.id()) else {
                continue;
            };
            let value: f64 = cls
                .composition
                .iter()
                .filter_map(|(r, f)| {
                    self.data
                        .resource_index(r)
                        .map(|ri| f * self.data.resources[ri].value)
                })
                .sum();
            if best.is_none_or(|(v, _)| value > v) {
                best = Some((value, i));
            }
        }
        match best {
            Some((v, i)) => {
                let b = &self.system.bodies[i];
                format!(
                    "Le rocher {} est le plus riche d'ici : environ {v:.1} CR le kilo de minerai.",
                    b.name
                )
            }
            None => "Il n'y a pas d'astéroïde digne de ce nom par ici.".to_string(),
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

    /// A sim parked above a world with people, and that world's subject number.
    fn civilised() -> (Sim, usize) {
        for seed in 1..60u64 {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
            let d = Arc::new(GameData::load(&dir).expect("data loads"));
            let mut s = new_sim(seed, d);
            if let Some(b) = s.first_civilised_world() {
                s.ship.pos =
                    s.bodies[b].pos - vector_core::Vec3::new(0.0, 0.0, s.bodies[b].radius * 3.0);
                s.credits = 5_000.0;
                s.ship.cargo.cap_mass = 1.0e6;
                s.ship.cargo.cap_vol = 1.0e6;
                return (s, crate::civ::CIV_SUBJECT + b);
            }
        }
        panic!("no civilisation in the first seeds");
    }

    #[test]
    fn the_people_of_a_world_answer_in_their_own_tone_and_remember_how_they_were_treated() {
        let (mut s, subject) = civilised();
        let body = subject - crate::civ::CIV_SUBJECT;
        assert!(s.can_hail_world(body).is_ok());
        let start = s
            .dialogue_start(subject)
            .expect("a civilisation has a start node");
        assert_eq!(start, "civ_start");
        let view = s.dialogue_view(subject, &start, "").expect("view");
        assert_eq!(view.role, "CIVILISATION");
        let civ = s.civilisation(body).unwrap();
        assert!(view.text.contains(&civ.name), "{}", view.text);
        // Learning their mores tells their plan of life.
        let dd = Arc::clone(&s.data);
        let idx = move |label: &str| {
            dd.dialogues
                .node("civ_start")
                .unwrap()
                .options
                .iter()
                .position(|o| o.label.contains(label))
                .unwrap()
        };
        let (_, mores) = s
            .dialogue_choose(subject, &start, idx("mœurs"))
            .expect("mores");
        assert!(
            mores.contains(&s.data.civilisations[civ.kind].plan),
            "{mores}"
        );
        // A present is refused until they like you.
        assert!(!s.dialogue_view(subject, &start, "").unwrap().options[idx("présent")].1);
        let d = Arc::clone(&s.data);
        let food = d.resource_index("food").unwrap();
        s.ship.cargo.add(&d, food, 100.0);
        let rep0 = s.civ_reputation(body);
        s.dialogue_choose(subject, &start, idx("Offrir"))
            .expect("gift");
        assert!(s.civ_reputation(body) > rep0 + 0.1);
        assert!(s.dialogue_view(subject, &start, "").unwrap().options[idx("présent")].1);
        let mass = s.ship.cargo.mass;
        let (next, text) = s
            .dialogue_choose(subject, &start, idx("présent"))
            .expect("present");
        assert_eq!(next, "civ_gift");
        assert!(text.starts_with("Ils vous offrent"), "{text}");
        assert!(s.ship.cargo.mass > mass, "they gave something");
        // Threats cost goodwill, and goodwill is remembered.
        let before = s.civ_reputation(body);
        s.dialogue_choose(subject, &start, idx("menacer"))
            .expect("threat");
        assert!(s.civ_reputation(body) < before - 0.25);
        // Ship-only effects do nothing to a people (no panic on a world subject).
        assert!(s
            .apply_dialogue_effect(subject, &vector_data::dialogue::Effect::Bribe)
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_world_too_far_or_without_people_does_not_answer() {
        let (mut s, subject) = civilised();
        let body = subject - crate::civ::CIV_SUBJECT;
        s.ship.pos = s.bodies[body].pos
            + vector_core::Vec3::new(0.0, 0.0, s.bodies[body].radius * 500.0 + 1.0e9);
        assert!(s.can_hail_world(body).unwrap_err().contains("loin"));
        let empty = (0..s.bodies.len())
            .find(|&i| s.civilisation(i).is_none())
            .unwrap();
        s.ship.pos =
            s.bodies[empty].pos - vector_core::Vec3::new(0.0, 0.0, s.bodies[empty].radius * 3.0);
        assert!(s.can_hail_world(empty).is_err());
    }

    #[test]
    fn miners_know_where_the_best_rock_is() {
        let mut s = sim();
        let k = npc_of(&s, NpcRole::Miner);
        let start = s.dialogue_start(k).expect("start");
        let ore = s
            .data
            .dialogues
            .node(&start)
            .unwrap()
            .options
            .iter()
            .position(|o| o.effects.iter().any(|e| e == "ore"))
            .expect("a question about ore");
        let (next, text) = s.dialogue_choose(k, &start, ore).expect("ask");
        assert_eq!(next, "tip_reply");
        assert!(
            text.contains("le plus riche") || text.contains("astéroïde"),
            "{text}"
        );
    }
}
