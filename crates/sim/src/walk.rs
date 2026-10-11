//! Walking expeditions (E5): the ship's AI takes control of crew members (and a rover) on foot on
//! a small piece of a planet, in a turn-based roguelike view. The ground, the plants and the
//! animals come from the biome of the chosen surface cell and the body's seed, so the same cell
//! always gives the same place. The expedition itself is not saved: it is over when the party
//! comes back (loot goes into the hold) or dies.
use crate::{ShipInput, Sim};
use vector_core::{hash_coord, Rng};
use vector_world::surface::Surface;

pub const MAP_W: i32 = 96;
pub const MAP_H: i32 = 52;
/// Game time of one turn, seconds.
pub const TURN_SECONDS: f64 = 60.0;
/// The drone costs one unit of robotics, flies this many turns and reaches this far (cells).
pub const DRONE_COST: f64 = 1.0;
pub const DRONE_TURNS: u32 = 30;
pub const DRONE_REACH: i32 = 3;
/// What one person and the rover can carry, kg.
pub const LOOT_PER_MEMBER: f64 = 30.0;
pub const LOOT_VEHICLE: f64 = 250.0;
/// Tiles per move with the rover.
pub const VEHICLE_SPEED: i32 = 2;
/// How far from a planet the ship may be to send an expedition, in body radii.
pub const MAX_RANGE_RADII: f64 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    Ground,
    Rock,
    Water,
    /// Where the ship set the party down (and where it is picked up).
    Landing,
}

#[derive(Clone, Debug)]
pub struct Creature {
    pub species: usize,
    pub x: i32,
    pub y: i32,
    pub hp: f64,
    pub alive: bool,
}

/// A site of interest on the map.
#[derive(Clone, Debug)]
pub struct SiteInst {
    pub def: usize,
    pub x: i32,
    pub y: i32,
    pub used: bool,
}

#[derive(Clone, Debug)]
pub struct Plant {
    pub species: usize,
    pub x: i32,
    pub y: i32,
    pub taken: bool,
}

/// One expedition in progress.
#[derive(Clone, Debug)]
pub struct Walk {
    pub body: usize,
    pub biome: usize,
    pub seed: u64,
    pub tiles: Vec<Tile>,
    pub plants: Vec<Plant>,
    pub creatures: Vec<Creature>,
    pub sites: Vec<SiteInst>,
    /// Air pressure (bar) and temperature (K) of the place: without a suit they hurt.
    pub pressure: f64,
    pub temp: f64,
    /// Indices into the crew, and where each one stands.
    pub party: Vec<usize>,
    pub pos: Vec<(i32, i32)>,
    pub active: usize,
    pub vehicle: bool,
    /// (resource, kg) picked up so far.
    pub loot: Vec<(usize, f64)>,
    pub turn: u32,
    pub log: Vec<String>,
    pub over: bool,
    /// Turns left of the drone the AI sent ahead (it gathers plants within reach on its own).
    pub drone_turns: u32,
}

fn vnoise(seed: u64, x: f64, y: f64) -> f64 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let (fx, fy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let c = |dx: i64, dy: i64| {
        (hash_coord(seed, ix as i64 + dx, iy as i64 + dy, 5) >> 11) as f64 / (1u64 << 53) as f64
    };
    let a = c(0, 0) + (c(1, 0) - c(0, 0)) * fx;
    let b = c(0, 1) + (c(1, 1) - c(0, 1)) * fx;
    a + (b - a) * fy
}

/// Damage per turn of a place without a suit's protection: (damage after armour, reason). A
/// thin atmosphere (below 0.05 bar) and extreme temperatures hurt; armour cuts it by 1.5 x its
/// defence (a full exosuit stops almost everything).
pub fn environment_damage(pressure: f64, temp: f64, defense: f64) -> (f64, &'static str) {
    let mut dmg: f64 = 0.0;
    let mut why = "";
    if pressure < 0.05 {
        dmg += 0.03;
        why = "L'air manque";
    }
    let excess = if temp < 250.0 {
        250.0 - temp
    } else if temp > 330.0 {
        temp - 330.0
    } else {
        0.0
    };
    if excess > 0.0 {
        dmg += 0.004 * (excess / 10.0).min(10.0);
        why = if temp < 250.0 {
            "Le froid mord"
        } else {
            "La chaleur brule"
        };
    }
    (dmg * (1.0 - (1.5 * defense).min(0.95)), why)
}

impl Walk {
    pub fn idx(x: i32, y: i32) -> usize {
        (y * MAP_W + x) as usize
    }

    pub fn inside(x: i32, y: i32) -> bool {
        (0..MAP_W).contains(&x) && (0..MAP_H).contains(&y)
    }

    pub fn tile(&self, x: i32, y: i32) -> Tile {
        if Self::inside(x, y) {
            self.tiles[Self::idx(x, y)]
        } else {
            Tile::Rock
        }
    }

    pub fn landing() -> (i32, i32) {
        (MAP_W / 2, MAP_H / 2)
    }

    pub fn creature_at(&self, x: i32, y: i32) -> Option<usize> {
        self.creatures
            .iter()
            .position(|c| c.alive && c.x == x && c.y == y)
    }

    pub fn carry_capacity(&self) -> f64 {
        self.party.len() as f64 * LOOT_PER_MEMBER + if self.vehicle { LOOT_VEHICLE } else { 0.0 }
    }

    pub fn loot_mass(&self) -> f64 {
        self.loot.iter().map(|(_, kg)| kg).sum()
    }

    fn free(&self, x: i32, y: i32) -> bool {
        matches!(self.tile(x, y), Tile::Ground | Tile::Landing)
            && self.creature_at(x, y).is_none()
            && !self.pos.contains(&(x, y))
    }

    fn say(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 60 {
            self.log.remove(0);
        }
    }

    /// Whether member `i` of the party can still act.
    pub fn up(&self, sim: &Sim, i: usize) -> bool {
        sim.crew
            .members
            .get(self.party[i])
            .is_some_and(|p| p.alive && p.health > 0.0)
    }

    pub fn active_member(&self) -> usize {
        self.party[self.active]
    }
}

impl Sim {
    /// Whether an expedition can leave for `body` now.
    pub fn expedition_possible(&self, body: usize) -> Result<(), &'static str> {
        let b = self.bodies.get(body).ok_or("corps inconnu")?;
        self.planet_map(body).ok_or("pas de surface solide")?;
        if (self.ship.pos - b.pos).length() > b.radius * MAX_RANGE_RADII + 1.0e6 {
            return Err("approchez-vous de la planete");
        }
        if self.crew.alive().count() == 0 {
            return Err("personne a bord");
        }
        Ok(())
    }

    /// Land `party` (crew indices) on the cell (`cx`, `cy`) of `body`.
    pub fn walk_start(
        &self,
        body: usize,
        cx: usize,
        cy: usize,
        party: Vec<usize>,
        vehicle: bool,
    ) -> Result<Walk, &'static str> {
        self.expedition_possible(body)?;
        let map = self.planet_map(body).ok_or("pas de surface solide")?;
        if map.surface.is_sea(cx, cy) {
            return Err("pas de terre ferme ici");
        }
        let party: Vec<usize> = party
            .into_iter()
            .filter(|&i| {
                self.crew
                    .members
                    .get(i)
                    .is_some_and(|p| p.alive && p.health > 0.0)
            })
            .collect();
        if party.is_empty() {
            return Err("aucun equipier valide");
        }
        let biome = map.biome_at(cx, cy);
        let seed = hash_coord(map.seed, cx as i64, cy as i64, 77);
        let b = &self.data.biomes[biome];
        let hum = f64::from(map.surface.hum[Surface::idx(cx, cy)]);
        // Ground: rocks and, in wet places, ponds.
        let mut tiles = vec![Tile::Ground; (MAP_W * MAP_H) as usize];
        for y in 0..MAP_H {
            for x in 0..MAP_W {
                let n = vnoise(seed, f64::from(x) / 6.0, f64::from(y) / 6.0);
                let w = vnoise(seed ^ 0xA5A5, f64::from(x) / 9.0, f64::from(y) / 9.0);
                tiles[Walk::idx(x, y)] = if n > 0.80 {
                    Tile::Rock
                } else if hum > 0.45 && w < 0.22 {
                    Tile::Water
                } else {
                    Tile::Ground
                };
            }
        }
        let (lx, ly) = Walk::landing();
        for dy in -2..=2 {
            for dx in -2..=2 {
                tiles[Walk::idx(lx + dx, ly + dy)] = Tile::Ground;
            }
        }
        tiles[Walk::idx(lx, ly)] = Tile::Landing;
        let mut w = Walk {
            body,
            biome,
            seed,
            tiles,
            plants: Vec::new(),
            creatures: Vec::new(),
            sites: Vec::new(),
            pressure: map.pressure,
            temp: self.temp_now(&map, cx, cy),
            pos: vec![(lx, ly); party.len()],
            party,
            active: 0,
            vehicle,
            loot: Vec::new(),
            turn: 0,
            drone_turns: 0,
            log: Vec::new(),
            over: false,
        };
        // Life: the producers and animals of the biome.
        let mut rng = Rng::new(seed ^ 0x11FE);
        let flora: Vec<usize> = (0..self.data.species.len())
            .filter(|&i| {
                let s = &self.data.species[i];
                s.is_producer() && s.body >= 0.04 && s.habitat.contains(&b.id)
            })
            .collect();
        let fauna: Vec<usize> = (0..self.data.species.len())
            .filter(|&i| {
                let s = &self.data.species[i];
                !s.is_producer() && s.body >= 0.01 && s.habitat.contains(&b.id)
            })
            .collect();
        let n_plants = if flora.is_empty() {
            0
        } else {
            ((MAP_W * MAP_H) as f64 * (0.01 + 0.06 * b.fertility)) as usize
        };
        for _ in 0..n_plants {
            let (x, y) = (
                (rng.next_u64() % MAP_W as u64) as i32,
                (rng.next_u64() % MAP_H as u64) as i32,
            );
            if w.free(x, y) && !w.plants.iter().any(|p| p.x == x && p.y == y) {
                let sp = flora[(rng.next_u64() % flora.len() as u64) as usize];
                w.plants.push(Plant {
                    species: sp,
                    x,
                    y,
                    taken: false,
                });
            }
        }
        let n_fauna = if fauna.is_empty() {
            0
        } else {
            8 + (rng.next_u64() % 8) as usize
        };
        for _ in 0..n_fauna {
            let (x, y) = (
                (rng.next_u64() % MAP_W as u64) as i32,
                (rng.next_u64() % MAP_H as u64) as i32,
            );
            // Not on top of the landing party.
            if w.free(x, y) && (x - lx).abs() + (y - ly).abs() > 8 {
                let sp = fauna[(rng.next_u64() % fauna.len() as u64) as usize];
                let hp = 1.0 + self.data.species[sp].body.cbrt() * 1.5;
                w.creatures.push(Creature {
                    species: sp,
                    x,
                    y,
                    hp,
                    alive: true,
                });
            }
        }
        // Sites of interest: 2 to 4, by weight, away from the landing.
        let total: f64 = self.data.sites.iter().map(|s| s.weight).sum();
        for _ in 0..(2 + rng.next_u64() % 3) {
            let (x, y) = (
                (rng.next_u64() % MAP_W as u64) as i32,
                (rng.next_u64() % MAP_H as u64) as i32,
            );
            if !w.free(x, y) || (x - lx).abs() + (y - ly).abs() < 10 {
                continue;
            }
            if w.sites.iter().any(|s| s.x == x && s.y == y)
                || w.plants.iter().any(|p| p.x == x && p.y == y)
            {
                continue;
            }
            let mut pick = rng.f64() * total;
            let mut def = 0;
            for (i, s) in self.data.sites.iter().enumerate() {
                if pick < s.weight {
                    def = i;
                    break;
                }
                pick -= s.weight;
            }
            w.sites.push(SiteInst {
                def,
                x,
                y,
                used: false,
            });
        }
        let names: Vec<String> = w
            .party
            .iter()
            .map(|&i| self.crew.members[i].name.clone())
            .collect();
        w.say(format!(
            "Debarquement en {} : {}{}.",
            b.name.to_lowercase(),
            names.join(", "),
            if vehicle { ", avec le rover" } else { "" }
        ));
        Ok(w)
    }

    /// Move the active member one step (or `VEHICLE_SPEED` with the rover); walking into an
    /// animal attacks it. Ends the turn.
    pub fn walk_step(&mut self, w: &mut Walk, dx: i32, dy: i32) {
        if w.over || !w.up(self, w.active) {
            return;
        }
        let me = w.active;
        let (mut x, mut y) = w.pos[me];
        let steps = if w.vehicle { VEHICLE_SPEED } else { 1 };
        for _ in 0..steps {
            let (nx, ny) = (x + dx, y + dy);
            if let Some(c) = w.creature_at(nx, ny) {
                self.walk_attack(w, c);
                break;
            }
            if !w.free(nx, ny) {
                break;
            }
            x = nx;
            y = ny;
        }
        w.pos[me] = (x, y);
        self.walk_end_turn(w);
    }

    pub(crate) fn walk_attack(&mut self, w: &mut Walk, c: usize) {
        let me = w.party[w.active];
        let skill = self
            .data
            .skill_index("combat")
            .map_or(0.0, |s| f64::from(self.crew.members[me].skills[s]));
        let dmg = 0.8 + 0.3 * skill + self.crew.members[me].gear_stat(&self.data, "attack");
        let sp = w.creatures[c].species;
        w.creatures[c].hp -= dmg;
        let name = self.data.species[sp].name.to_lowercase();
        if w.creatures[c].hp <= 0.0 {
            w.creatures[c].alive = false;
            let yields = self.data.species[sp].yields.clone();
            w.say(format!("Vous abattez : {name}."));
            for (res, kg) in yields {
                self.walk_loot(w, &res, kg);
            }
        } else {
            w.say(format!("Vous frappez : {name} ({dmg:.1} degats)."));
        }
    }

    fn walk_loot(&mut self, w: &mut Walk, res: &str, kg: f64) {
        let Some(r) = self.data.resource_index(res) else {
            return;
        };
        let room = (w.carry_capacity() - w.loot_mass()).max(0.0);
        let take = kg.min(room);
        if take < kg {
            w.say("Trop lourd : une partie est abandonnee sur place.");
        }
        if take > 0.0 {
            match w.loot.iter_mut().find(|(i, _)| *i == r) {
                Some((_, v)) => *v += take,
                None => w.loot.push((r, take)),
            }
        }
    }

    /// Gather the plant under or next to the active member, or search a site next to them.
    pub fn walk_gather(&mut self, w: &mut Walk) {
        if w.over || !w.up(self, w.active) {
            return;
        }
        let (x, y) = w.pos[w.active];
        if let Some(k) = w
            .sites
            .iter()
            .position(|s| !s.used && (s.x - x).abs() <= 1 && (s.y - y).abs() <= 1)
        {
            self.walk_search_site(w, k);
            return;
        }
        let found = w
            .plants
            .iter()
            .position(|p| !p.taken && (p.x - x).abs() <= 1 && (p.y - y).abs() <= 1);
        match found {
            Some(k) => {
                w.plants[k].taken = true;
                let sp = w.plants[k].species;
                let yields = self.data.species[sp].yields.clone();
                w.say(format!(
                    "Recolte : {}.",
                    self.data.species[sp].name.to_lowercase()
                ));
                let bonus =
                    1.0 + self.crew.members[w.party[w.active]].gear_stat(&self.data, "gather");
                for (res, kg) in yields {
                    self.walk_loot(w, &res, kg * bonus);
                }
                self.walk_end_turn(w);
            }
            None => w.say("Rien a recolter ici."),
        }
    }

    fn walk_search_site(&mut self, w: &mut Walk, k: usize) {
        w.sites[k].used = true;
        let data = std::sync::Arc::clone(&self.data);
        let site = &data.sites[w.sites[k].def];
        w.say(format!("{} : {}", site.name, site.text));
        let bonus = 1.0 + self.crew.members[w.party[w.active]].gear_stat(&self.data, "gather");
        for (res, kg) in &site.loot {
            self.walk_loot(w, res, *kg * bonus);
        }
        if site.heal {
            for &m in &w.party {
                let p = &mut self.crew.members[m];
                if p.alive {
                    p.health = (p.health + 0.3).min(1.0);
                    p.fatigue = (p.fatigue - 0.5).max(0.0);
                    for o in p.organs.iter_mut() {
                        if *o > 0.0 {
                            *o = (*o + 0.2).min(1.0);
                        }
                    }
                }
            }
            w.say("Toute l'equipe se repose et se soigne.");
        }
        let mut rng = Rng::new(hash_coord(w.seed, i64::from(w.turn), k as i64, 63));
        if site.danger > 0.0 && rng.f64() < site.danger {
            let who = w.party[w.active];
            let dmg = 0.2 + 0.3 * rng.f64();
            self.walk_hurt(w, who, dmg, "Un eboulement ou une embuscade");
        }
        self.walk_end_turn(w);
    }

    /// Control the next member who can still act.
    pub fn walk_switch(&mut self, w: &mut Walk) {
        let n = w.party.len();
        for k in 1..=n {
            let i = (w.active + k) % n;
            if w.up(self, i) {
                w.active = i;
                let name = self.crew.members[w.party[i]].name.clone();
                w.say(format!("L'IA prend le controle de {name}."));
                return;
            }
        }
    }

    /// Call everybody back to the ship: the loot goes into the hold, the time of the walk back
    /// passes. Ends the expedition.
    pub fn walk_return(&mut self, w: &mut Walk) {
        if w.over {
            return;
        }
        let (lx, ly) = Walk::landing();
        let far = (0..w.party.len())
            .filter(|&i| w.up(self, i))
            .map(|i| (w.pos[i].0 - lx).abs().max((w.pos[i].1 - ly).abs()))
            .max()
            .unwrap_or(0);
        self.advance(f64::from(far) * TURN_SECONDS, &ShipInput::default());
        let data = std::sync::Arc::clone(&self.data);
        let mut left = Vec::new();
        for (r, kg) in std::mem::take(&mut w.loot) {
            let got = self.ship.cargo.add(&data, r, kg);
            if got < kg {
                left.push(data.resources[r].name.clone());
            }
        }
        if !left.is_empty() {
            w.say(format!("Soute pleine : abandonne {}.", left.join(", ")));
        }
        w.say("Retour au vaisseau.");
        w.over = true;
    }

    /// The AI sends a drone (one unit of robotics): for DRONE_TURNS turns it gathers, on its own,
    /// the plants within DRONE_REACH cells of the active member, at half yield.
    pub fn walk_deploy_drone(&mut self, w: &mut Walk) -> Result<(), &'static str> {
        if w.over {
            return Err("l'expedition est terminee");
        }
        if w.drone_turns > 0 {
            return Err("un drone est deja en vol");
        }
        let r = self
            .data
            .resource_index("robotics")
            .ok_or("pas de robotique")?;
        if self.ship.cargo.kg[r] < DRONE_COST {
            return Err("il faut une unite de robotique dans la soute");
        }
        let data = std::sync::Arc::clone(&self.data);
        self.ship.cargo.remove(&data, r, DRONE_COST);
        w.drone_turns = DRONE_TURNS;
        w.say("Un drone est lance : il recolte autour de l'equipe.");
        self.walk_end_turn(w);
        Ok(())
    }

    fn walk_drone_gather(&mut self, w: &mut Walk) {
        let (x, y) = w.pos[w.active];
        let Some(k) = w.plants.iter().position(|p| {
            !p.taken && (p.x - x).abs() <= DRONE_REACH && (p.y - y).abs() <= DRONE_REACH
        }) else {
            return;
        };
        w.plants[k].taken = true;
        let sp = w.plants[k].species;
        let yields = self.data.species[sp].yields.clone();
        for (res, kg) in yields {
            self.walk_loot(w, &res, kg * 0.5);
        }
    }

    fn walk_end_turn(&mut self, w: &mut Walk) {
        w.turn += 1;
        if w.drone_turns > 0 {
            w.drone_turns -= 1;
            self.walk_drone_gather(w);
            if w.drone_turns == 0 {
                w.say("Le drone rentre, batteries a plat.");
            }
        }
        self.advance(TURN_SECONDS, &ShipInput::default());
        let mut rng = Rng::new(hash_coord(w.seed, i64::from(w.turn), 3, 41));
        for c in 0..w.creatures.len() {
            if !w.creatures[c].alive {
                continue;
            }
            let (cx, cy) = (w.creatures[c].x, w.creatures[c].y);
            let sp = &self.data.species[w.creatures[c].species];
            // Nearest living member.
            let near = (0..w.party.len())
                .filter(|&i| w.up(self, i))
                .map(|i| {
                    let (px, py) = w.pos[i];
                    ((px - cx).abs().max((py - cy).abs()), i, px, py)
                })
                .min_by_key(|t| t.0);
            let hunts = sp.danger > 0.0 && !sp.domestic;
            let mut target = (cx, cy);
            match near {
                Some((d, i, _, _)) if hunts && d <= 1 => {
                    let dmg = sp.danger * (0.15 + 0.25 * rng.f64());
                    let who = w.party[i];
                    let what = sp.name.to_lowercase();
                    self.walk_hurt(w, who, dmg, &what);
                    continue;
                }
                Some((d, _, px, py)) if hunts && d <= 8 => {
                    target = (cx + (px - cx).signum(), cy + (py - cy).signum());
                }
                Some((d, _, px, py)) if !hunts && !sp.domestic && d <= 3 => {
                    target = (cx - (px - cx).signum(), cy - (py - cy).signum());
                }
                _ => {
                    if rng.f64() < 0.3 && !sp.domestic {
                        target = (
                            cx + (rng.next_u64() % 3) as i32 - 1,
                            cy + (rng.next_u64() % 3) as i32 - 1,
                        );
                    }
                }
            }
            if target != (cx, cy) && w.free(target.0, target.1) {
                w.creatures[c].x = target.0;
                w.creatures[c].y = target.1;
            }
        }
        // The air and the weather: a suit (any armour) protects, in proportion to its defence.
        for i in 0..w.party.len() {
            if !w.up(self, i) {
                continue;
            }
            let who = w.party[i];
            let defense = self.crew.members[who].gear_stat(&self.data, "defense");
            let (dmg, why) = environment_damage(w.pressure, w.temp, defense);
            if dmg > 0.0 {
                self.walk_hurt_raw(w, who, dmg, why);
            }
        }
        if (0..w.party.len()).all(|i| !w.up(self, i)) {
            w.say("Toute l'equipe est hors de combat.");
            w.over = true;
        } else if !w.up(self, w.active) {
            self.walk_switch(w);
        }
    }

    /// Damage that no armour stops (thin air, cold): applied as it is.
    fn walk_hurt_raw(&mut self, w: &mut Walk, who: usize, dmg: f64, by: &str) {
        let defense = self.crew.members[who].gear_stat(&self.data, "defense");
        // `walk_hurt` takes the armour off again: put it back so the net result is `dmg`.
        let gross = dmg / (1.0 - defense.min(0.9));
        self.walk_hurt(w, who, gross, by);
    }

    /// Hurt crew member `who`: health and one organ (a vital one at 0 kills).
    pub(crate) fn walk_hurt(&mut self, w: &mut Walk, who: usize, dmg: f64, by: &str) {
        let n_org = self.data.organs.len();
        let pick =
            (hash_coord(w.seed, i64::from(w.turn), who as i64, 57) % n_org.max(1) as u64) as usize;
        let organ = self.data.organs[pick].name.to_lowercase();
        let vital = self.data.organs[pick].vital;
        let dmg = dmg
            * (1.0
                - self.crew.members[who]
                    .gear_stat(&self.data, "defense")
                    .min(0.9));
        let p = &mut self.crew.members[who];
        p.organs.resize(n_org, 1.0);
        p.health = (p.health - dmg as f32).max(0.0);
        p.organs[pick] = (p.organs[pick] - (dmg * 0.8) as f32).max(0.0);
        let name = p.name.clone();
        let dead = p.health <= 0.0 || (vital && p.organs[pick] <= 0.0);
        if dead {
            p.health = 0.0;
        }
        w.say(format!("{by} blesse {name} ({organ}, -{dmg:.2})."));
        if dead {
            w.say(format!("{name} s'effondre."));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn data() -> Arc<GameData> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        Arc::new(GameData::load(&dir).expect("data loads"))
    }

    /// A sim parked above a solid world with a cell that has both plants and animals.
    fn setup() -> (Sim, usize, usize, usize) {
        for seed in 1..60u64 {
            let mut s = new_sim(seed, data());
            for body in 0..s.bodies.len() {
                let Some(map) = s.planet_map(body) else {
                    continue;
                };
                for cy in (4..vector_world::surface::H).step_by(6) {
                    for cx in (0..vector_world::surface::W).step_by(9) {
                        if map.surface.is_sea(cx, cy) {
                            continue;
                        }
                        s.ship.pos = s.bodies[body].pos
                            - vector_core::Vec3::new(0.0, 0.0, s.bodies[body].radius * 3.0);
                        s.ship.vel = s.bodies[body].vel;
                        s.update_ref();
                        if let Ok(w) = s.walk_start(body, cx, cy, vec![0], false) {
                            if w.plants.len() > 5 && w.creatures.len() > 3 {
                                return (s, body, cx, cy);
                            }
                        }
                    }
                }
            }
        }
        panic!("no living world found in the first seeds");
    }

    #[test]
    fn the_same_cell_always_gives_the_same_place_and_the_party_lands_together() {
        let (s, body, cx, cy) = setup();
        let a = s.walk_start(body, cx, cy, vec![0], false).unwrap();
        let b = s.walk_start(body, cx, cy, vec![0], false).unwrap();
        assert_eq!(a.tiles, b.tiles);
        assert_eq!(a.plants.len(), b.plants.len());
        assert_eq!(
            a.creatures.iter().map(|c| (c.x, c.y)).collect::<Vec<_>>(),
            b.creatures.iter().map(|c| (c.x, c.y)).collect::<Vec<_>>()
        );
        assert_eq!(a.pos, vec![Walk::landing()]);
        assert_eq!(a.tile(Walk::landing().0, Walk::landing().1), Tile::Landing);
        let c = s.walk_start(body, (cx + 1) % 64, cy, vec![0], false);
        if let Ok(c) = c {
            assert_ne!(a.seed, c.seed, "another cell, another place");
        }
    }

    #[test]
    fn walking_is_blocked_by_rocks_and_water_and_time_passes() {
        let (mut s, body, cx, cy) = setup();
        let mut w = s.walk_start(body, cx, cy, vec![0], false).unwrap();
        let (lx, ly) = Walk::landing();
        // Wall off the east side and try to walk into it.
        w.tiles[Walk::idx(lx + 1, ly)] = Tile::Rock;
        let t0 = s.time;
        s.walk_step(&mut w, 1, 0);
        assert_eq!(w.pos[0], (lx, ly), "a rock blocks");
        assert!(s.time > t0, "a turn passes even when blocked");
        w.tiles[Walk::idx(lx - 1, ly)] = Tile::Ground;
        w.creatures
            .retain(|c| (c.x - lx + 1).abs() > 1 || (c.y - ly).abs() > 1);
        s.walk_step(&mut w, -1, 0);
        assert_eq!(w.pos[0], (lx - 1, ly));
        // The rover goes twice as far.
        let mut v = s.walk_start(body, cx, cy, vec![0], true).unwrap();
        for dx in 1..=2 {
            v.tiles[Walk::idx(lx + dx, ly)] = Tile::Ground;
            v.creatures.retain(|c| c.x != lx + dx);
        }
        s.walk_step(&mut v, 1, 0);
        assert_eq!(v.pos[0], (lx + 2, ly), "two tiles with the rover");
    }

    #[test]
    fn gathering_fills_the_pack_up_to_its_limit_and_returning_fills_the_hold() {
        let (mut s, body, cx, cy) = setup();
        let mut w = s.walk_start(body, cx, cy, vec![0], false).unwrap();
        // Put a plant next to the member and take it.
        let sp = w.plants[0].species;
        w.plants[0].x = w.pos[0].0 + 1;
        w.plants[0].y = w.pos[0].1;
        w.plants[0].taken = false;
        s.walk_gather(&mut w);
        assert!(w.plants[0].taken);
        let expected: f64 = s.data.species[sp].yields.iter().map(|(_, k)| k).sum();
        assert!(w.loot_mass() > 0.0 && w.loot_mass() <= expected + 1e-9);
        // The pack never holds more than the capacity.
        s.walk_loot(&mut w, "food", 1.0e6);
        assert!(w.loot_mass() <= w.carry_capacity() + 1e-9);
        let before: f64 = s.ship.cargo.mass;
        let carried = w.loot_mass();
        s.ship.cargo.cap_mass = 1.0e9;
        s.ship.cargo.cap_vol = 1.0e9;
        s.walk_return(&mut w);
        assert!(w.over && w.loot.is_empty());
        assert!(
            s.ship.cargo.mass >= before + carried - 1e-6,
            "the loot is in the hold"
        );
    }

    #[test]
    fn bumping_into_an_animal_fights_it_and_predators_hurt_organs_and_can_kill() {
        let (mut s, body, cx, cy) = setup();
        let mut w = s.walk_start(body, cx, cy, vec![0], false).unwrap();
        let (lx, ly) = Walk::landing();
        // A weak animal on the east: bumping kills it eventually and drops its yields.
        let k = 0;
        w.creatures[k].x = lx + 1;
        w.creatures[k].y = ly;
        w.creatures[k].hp = 0.5;
        w.creatures[k].alive = true;
        let sp = w.creatures[k].species;
        s.walk_step(&mut w, 1, 0);
        assert!(!w.creatures[k].alive, "a strong enough blow kills it");
        let has_yield = s.data.species[sp].yields.iter().any(|(r, _)| {
            s.data
                .resource_index(r)
                .is_some_and(|i| w.loot.iter().any(|(l, _)| *l == i))
        });
        assert!(has_yield || s.data.species[sp].yields.is_empty());
        // A terrible predator next to the member wounds it, damaging an organ too.
        let fierce = (0..s.data.species.len())
            .find(|&i| {
                !s.data.species[i].is_producer()
                    && s.data.species[i].danger > 0.0
                    && !s.data.species[i].domestic
            })
            .expect("a dangerous species");
        let victim = w.party[0];
        let before: Vec<f32> = s.crew.members[victim].organs.clone();
        let h0 = s.crew.members[victim].health;
        w.creatures.push(Creature {
            species: fierce,
            x: lx + 1,
            y: ly + 1,
            hp: 50.0,
            alive: true,
        });
        for _ in 0..40 {
            if w.over {
                break;
            }
            s.walk_end_turn(&mut w);
        }
        let hurt = s.crew.members[victim].health < h0;
        let organ_lost = s.crew.members[victim]
            .organs
            .iter()
            .zip(&before)
            .any(|(a, b)| a < b);
        assert!(
            hurt && organ_lost,
            "the predator wounds the member and an organ"
        );
        assert!(w.log.iter().any(|l| l.contains("blesse")));
    }

    #[test]
    fn sites_are_placed_far_from_the_landing_and_searched_once_for_loot_and_rest() {
        let (mut s, body, cx, cy) = setup();
        let mut w = s.walk_start(body, cx, cy, vec![0], false).unwrap();
        let (lx, ly) = Walk::landing();
        assert!(w.sites.len() <= 4);
        assert!(w
            .sites
            .iter()
            .all(|t| (t.x - lx).abs() + (t.y - ly).abs() >= 10));
        // Put a loot site and a hot spring next to the member.
        let loot = s
            .data
            .sites
            .iter()
            .position(|d| !d.loot.is_empty() && !d.heal && d.danger < 0.1)
            .unwrap();
        let spring = s.data.sites.iter().position(|d| d.heal).unwrap();
        let (x, y) = w.pos[0];
        w.sites.clear();
        w.sites.push(SiteInst {
            def: loot,
            x: x + 1,
            y,
            used: false,
        });
        s.walk_gather(&mut w);
        assert!(w.sites[0].used);
        assert!(w.loot_mass() > 0.0, "the site gave something");
        let after = w.loot_mass();
        s.walk_gather(&mut w);
        assert_eq!(w.loot_mass(), after, "a site is searched once");
        // The spring mends wounds.
        let me = w.party[0];
        s.crew.members[me].health = 0.3;
        let lungs = s.data.organ_index("poumons").unwrap();
        s.crew.members[me].organs[lungs] = 0.4;
        w.sites.push(SiteInst {
            def: spring,
            x: x - 1,
            y,
            used: false,
        });
        s.walk_gather(&mut w);
        assert!(s.crew.members[me].health > 0.5);
        assert!(s.crew.members[me].organs[lungs] > 0.55);
    }

    #[test]
    fn weapons_hit_harder_armour_stops_damage_and_tools_find_more() {
        let (mut s, body, cx, cy) = setup();
        let d = Arc::clone(&s.data);
        let rifle = d.gear_index("fusil_plasma").unwrap();
        let suit = d.gear_index("exosquelette").unwrap();
        let pick = d.gear_index("pioche").unwrap();

        // A blow: the damage done to the same animal depends on the weapon.
        let blow = |s: &mut Sim, armed: bool| -> f64 {
            let mut w = s.walk_start(body, cx, cy, vec![0], false).unwrap();
            s.crew.members[0].gear = vec![None; 3];
            if armed {
                s.crew.members[0].gear[0] = Some(rifle);
            }
            let (lx, ly) = Walk::landing();
            w.creatures[0].x = lx + 1;
            w.creatures[0].y = ly;
            w.creatures[0].hp = 1000.0;
            w.creatures[0].alive = true;
            s.walk_attack(&mut w, 0);
            1000.0 - w.creatures[0].hp
        };
        let bare = blow(&mut s, false);
        let armed = blow(&mut s, true);
        assert!(
            armed > bare + d.gear[rifle].attack - 1e-9,
            "{armed} vs {bare}"
        );

        // A wound: the same hit costs less health in armour.
        let wound = |s: &mut Sim, armoured: bool| -> f32 {
            let mut w = s.walk_start(body, cx, cy, vec![0], false).unwrap();
            s.crew.members[0].gear = vec![None; 3];
            s.crew.members[0].health = 1.0;
            if armoured {
                s.crew.members[0].gear[1] = Some(suit);
            }
            s.walk_hurt(&mut w, 0, 0.4, "test");
            1.0 - s.crew.members[0].health
        };
        let naked = wound(&mut s, false);
        let dressed = wound(&mut s, true);
        assert!((naked - 0.4).abs() < 1e-3);
        assert!(dressed < naked * 0.6, "{dressed} vs {naked}");

        // A harvest: the tool adds its share.
        let harvest = |s: &mut Sim, tool: bool| -> f64 {
            let mut w = s.walk_start(body, cx, cy, vec![0], false).unwrap();
            s.crew.members[0].gear = vec![None; 3];
            if tool {
                s.crew.members[0].gear[2] = Some(pick);
            }
            w.plants[0].x = w.pos[0].0 + 1;
            w.plants[0].y = w.pos[0].1;
            w.plants[0].taken = false;
            w.sites.clear();
            s.walk_gather(&mut w);
            w.loot_mass()
        };
        let plain = harvest(&mut s, false);
        let tooled = harvest(&mut s, true);
        assert!(plain > 0.0);
        assert!(
            (tooled / plain - (1.0 + d.gear[pick].gather)).abs() < 1e-6,
            "{tooled} / {plain}"
        );
    }

    #[test]
    fn switching_control_skips_the_fallen() {
        let (mut s, body, cx, cy) = setup();
        let d = Arc::clone(&s.data);
        let sib = {
            let me = s.crew.members[0].kin;
            let sib = crate::genealogy::siblings(&s.crew.families, me)[0];
            crate::crew::person_from_kin(&d, &s.crew.families, sib, 99, None, None)
        };
        s.crew.hire(&d, sib, 2, 0.0);
        let mut w = s.walk_start(body, cx, cy, vec![0, 1], false).unwrap();
        assert_eq!(w.active, 0);
        s.walk_switch(&mut w);
        assert_eq!(w.active, 1, "the AI moves to the other one");
        s.crew.members[0].health = 0.0;
        s.walk_switch(&mut w);
        assert_eq!(w.active, 1, "nobody else to switch to");
        s.crew.members[1].health = 0.0;
        s.walk_end_turn(&mut w);
        assert!(w.over, "everybody down ends the expedition");
    }

    #[test]
    fn thin_air_and_extreme_temperatures_hurt_unless_a_suit_is_worn() {
        // The formula: nothing in a mild place, damage in vacuum, armour cuts it.
        assert_eq!(environment_damage(1.0, 290.0, 0.0).0, 0.0);
        let bare = environment_damage(0.0, 290.0, 0.0).0;
        let suited = environment_damage(0.0, 290.0, 0.35).0;
        assert!(bare > 0.0 && suited < bare * 0.6);
        assert!(
            environment_damage(0.0, 290.0, 0.6).0 < bare * 0.15,
            "an exosuit nearly stops it"
        );
        assert!(environment_damage(1.0, 150.0, 0.0).0 > 0.0, "cold hurts");
        assert!(environment_damage(1.0, 420.0, 0.0).0 > 0.0, "heat hurts");
        // In a walk on a vacuum world the member loses health turn after turn.
        let (mut s, body, cx, cy) = setup();
        let mut w = s.walk_start(body, cx, cy, vec![0], false).unwrap();
        w.pressure = 0.0;
        s.crew.members[0].gear = vec![None; 3];
        s.crew.members[0].health = 1.0;
        for _ in 0..5 {
            s.walk_end_turn(&mut w);
        }
        let bare_health = s.crew.members[0].health;
        assert!(bare_health < 0.95, "{bare_health}");
        // With a suit the same turns cost much less.
        let suit = s.data.gear_index("exosquelette").unwrap();
        let mut w2 = s.walk_start(body, cx, cy, vec![0], false).unwrap();
        w2.pressure = 0.0;
        s.crew.members[0].gear[1] = Some(suit);
        s.crew.members[0].health = 1.0;
        for _ in 0..5 {
            s.walk_end_turn(&mut w2);
        }
        assert!(
            s.crew.members[0].health > bare_health + 0.05,
            "{}",
            s.crew.members[0].health
        );
    }

    #[test]
    fn a_drone_gathers_plants_on_its_own_for_a_while() {
        let (mut s, body, cx, cy) = setup();
        let mut w = s.walk_start(body, cx, cy, vec![0], false).unwrap();
        let r = s.data.resource_index("robotics").unwrap();
        s.ship.cargo.kg[r] = 0.0;
        assert!(
            s.walk_deploy_drone(&mut w).is_err(),
            "no robotics, no drone"
        );
        s.ship.cargo.kg[r] = 3.0;
        // Put plants all around the active member.
        let (x, y) = w.pos[w.active];
        let species = w.plants.first().map_or(0, |p| p.species);
        for k in 0..12 {
            w.plants.push(Plant {
                species,
                x: x + k % 3 - 1,
                y: y + k / 3 - 1,
                taken: false,
            });
        }
        let before = w.plants.iter().filter(|p| p.taken).count();
        s.walk_deploy_drone(&mut w).expect("launched");
        assert_eq!(w.drone_turns, DRONE_TURNS - 1);
        assert!(s.ship.cargo.kg[r] < 3.0, "a unit of robotics is spent");
        assert!(s.walk_deploy_drone(&mut w).is_err(), "one at a time");
        for _ in 0..8 {
            s.walk_step(&mut w, 0, 0);
        }
        assert!(
            w.plants.iter().filter(|p| p.taken).count() >= before + 5,
            "it gathered"
        );
        for _ in 0..40 {
            s.walk_step(&mut w, 0, 0);
        }
        assert_eq!(w.drone_turns, 0, "its batteries ran out");
    }
}
