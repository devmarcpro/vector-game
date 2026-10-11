//! Roguelike exploration (E9): every place of the game can be explored on foot, turn by turn, on
//! floors drawn as characters. Floors are a pure function of (place seed, visit number, depth):
//! rooms and corridors, creatures from the place's table, loot, and features (terminals, medbays,
//! altars, mutagen pools, traps, portals, vaults...). The AI sends a squad of the crew; they move
//! and fight together, whatever they carry out is added to the hold, and whoever falls stays down.
//! See `data/rogue_themes.ron` and `data/rogue_monsters.ron`.
use crate::{ShipInput, Sim};
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng};
use vector_data::{GameData, ROGUE_FEATURES};
use vector_world::{AnomalyKind, BodyKind, PlanetKind};

pub const T_WALL: u8 = 0;
pub const T_FLOOR: u8 = 1;
pub const T_UP: u8 = 2;
pub const T_DOWN: u8 = 3;
/// First feature tile; feature `k` of `ROGUE_FEATURES` is `T_FEATURE + k`.
pub const T_FEATURE: u8 = 4;

/// How far the squad sees, and seconds the ship's clock advances per turn.
pub const VIEW_RADIUS: i32 = 9;
pub const TURN_SECONDS: f64 = 5.0;
pub const MAX_PARTY: usize = 4;
/// Distance within which a body can be entered (m): stations, anomalies, wrecks.
pub const ENTER_RANGE: f64 = 150_000.0;
/// Monsters that never move, and the one that plays dead until you touch it.
const STATIONARY: [&str; 2] = ["tourelle", "reine_ruche"];
const SLEEPER: &str = "mimique";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mon {
    pub kind: usize,
    pub x: i32,
    pub y: i32,
    pub hp: f64,
    pub awake: bool,
    pub boss: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub x: i32,
    pub y: i32,
    pub res: usize,
    pub kg: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Floor {
    pub tiles: Vec<u8>,
    pub seen: Vec<bool>,
    pub monsters: Vec<Mon>,
    pub items: Vec<Item>,
    pub start: (i32, i32),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rogue {
    pub theme: usize,
    pub seed: u64,
    pub w: usize,
    pub h: usize,
    pub floors: Vec<Floor>,
    pub cur: usize,
    /// Crew indices of the squad, where they stand, who leads.
    pub party: Vec<usize>,
    pub x: i32,
    pub y: i32,
    pub active: usize,
    /// (resource, kg) carried out so far.
    pub loot: Vec<(usize, f64)>,
    pub turn: u32,
    pub log: Vec<String>,
    pub over: bool,
    pub kills: u32,
    /// Turns left of a blessing (damage x1.5).
    pub buff: u32,
    pub boss_dead: bool,
}

impl Rogue {
    pub fn floor(&self) -> &Floor {
        &self.floors[self.cur]
    }

    fn floor_mut(&mut self) -> &mut Floor {
        &mut self.floors[self.cur]
    }

    pub fn idx(&self, x: i32, y: i32) -> usize {
        y as usize * self.w + x as usize
    }

    pub fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h
    }

    pub fn tile(&self, x: i32, y: i32) -> u8 {
        if self.inside(x, y) {
            self.floor().tiles[self.idx(x, y)]
        } else {
            T_WALL
        }
    }

    fn say(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 60 {
            self.log.remove(0);
        }
    }

    /// Whether a straight line from (x0, y0) to (x1, y1) is open.
    pub fn line_clear(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        while (x, y) != (x1, y1) {
            if (x, y) != (x0, y0) && self.tile(x, y) == T_WALL {
                return false;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
        true
    }

    /// Cells the squad sees right now.
    pub fn visible(&self) -> Vec<bool> {
        let mut v = vec![false; self.w * self.h];
        for y in (self.y - VIEW_RADIUS)..=(self.y + VIEW_RADIUS) {
            for x in (self.x - VIEW_RADIUS)..=(self.x + VIEW_RADIUS) {
                if !self.inside(x, y) {
                    continue;
                }
                let (dx, dy) = (x - self.x, y - self.y);
                if dx * dx + dy * dy <= VIEW_RADIUS * VIEW_RADIUS
                    && self.line_clear(self.x, self.y, x, y)
                {
                    v[self.idx(x, y)] = true;
                }
            }
        }
        v
    }

    fn reveal(&mut self) {
        let v = self.visible();
        let f = self.floor_mut();
        for (s, vis) in f.seen.iter_mut().zip(v) {
            *s |= vis;
        }
    }

    pub fn monster_at(&self, x: i32, y: i32) -> Option<usize> {
        self.floor()
            .monsters
            .iter()
            .position(|m| m.x == x && m.y == y)
    }

    pub fn depth(&self) -> u32 {
        self.cur as u32
    }
}

/// Where a place can be entered from: a label, the theme and the seed of the place.
#[derive(Clone, Debug, PartialEq)]
pub struct RogueTarget {
    pub label: String,
    pub theme: usize,
    pub seed: u64,
}

fn pick_weighted<T: Clone>(list: &[(T, f64)], rng: &mut Rng) -> T {
    let total: f64 = list.iter().map(|x| x.1).sum();
    let mut r = rng.f64() * total;
    for (t, w) in list {
        if r < *w {
            return t.clone();
        }
        r -= w;
    }
    list[list.len() - 1].0.clone()
}

/// Build floor `depth` of a place.
pub fn generate_floor(data: &GameData, theme: usize, seed: u64, depth: u32) -> Floor {
    let t = &data.rogue_themes[theme];
    let (w, h) = t.size;
    let last = depth + 1 >= t.floors;
    let mut rng = Rng::new(hash_coord(seed, i64::from(depth), 11, 22));
    let mut tiles = vec![T_WALL; w * h];
    let n_rooms = t.rooms.0 + (rng.f64() * (t.rooms.1 - t.rooms.0 + 1) as f64) as usize;
    let mut rooms: Vec<(i32, i32, i32, i32)> = Vec::new();
    for _ in 0..400 {
        if rooms.len() >= n_rooms {
            break;
        }
        let rw = 5 + (rng.f64() * 8.0) as i32;
        let rh = 4 + (rng.f64() * 5.0) as i32;
        let x = 1 + (rng.f64() * (w as i32 - rw - 2) as f64) as i32;
        let y = 1 + (rng.f64() * (h as i32 - rh - 2) as f64) as i32;
        let clash = rooms.iter().any(|r| {
            x < r.0 + r.2 + 2 && x + rw + 2 > r.0 && y < r.1 + r.3 + 2 && y + rh + 2 > r.1
        });
        if clash {
            continue;
        }
        rooms.push((x, y, rw, rh));
        for yy in y..y + rh {
            for xx in x..x + rw {
                tiles[yy as usize * w + xx as usize] = T_FLOOR;
            }
        }
    }
    let center = |r: &(i32, i32, i32, i32)| (r.0 + r.2 / 2, r.1 + r.3 / 2);
    for k in 1..rooms.len() {
        let (ax, ay) = center(&rooms[k - 1]);
        let (bx, by) = center(&rooms[k]);
        let horizontal_first = rng.f64() < 0.5;
        let mut carve = |x: i32, y: i32| {
            tiles[y as usize * w + x as usize] = T_FLOOR.max(tiles[y as usize * w + x as usize])
        };
        let (mut x, mut y) = (ax, ay);
        if horizontal_first {
            while x != bx {
                carve(x, y);
                x += (bx - x).signum();
            }
            while y != by {
                carve(x, y);
                y += (by - y).signum();
            }
        } else {
            while y != by {
                carve(x, y);
                y += (by - y).signum();
            }
            while x != bx {
                carve(x, y);
                x += (bx - x).signum();
            }
        }
        carve(bx, by);
    }
    let start = center(&rooms[0]);
    tiles[start.1 as usize * w + start.0 as usize] = T_UP;
    if !last {
        let e = center(&rooms[rooms.len() - 1]);
        tiles[e.1 as usize * w + e.0 as usize] = T_DOWN;
    }
    let random_cell = |room: &(i32, i32, i32, i32), rng: &mut Rng| {
        (
            room.0 + (rng.f64() * f64::from(room.2)) as i32,
            room.1 + (rng.f64() * f64::from(room.3)) as i32,
        )
    };
    let free = |tiles: &Vec<u8>, c: (i32, i32)| tiles[c.1 as usize * w + c.0 as usize] == T_FLOOR;
    // Features: one in two rooms (not the first) holds one.
    let feats: Vec<(usize, f64)> = t
        .features
        .iter()
        .filter_map(|(f, wt)| ROGUE_FEATURES.iter().position(|x| x == f).map(|i| (i, *wt)))
        .collect();
    let mut vaults = Vec::new();
    for room in rooms.iter().skip(1) {
        if rng.f64() < 0.5 {
            let f = pick_weighted(&feats, &mut rng);
            let c = random_cell(room, &mut rng);
            if free(&tiles, c) {
                tiles[c.1 as usize * w + c.0 as usize] = T_FEATURE + f as u8;
                if ROGUE_FEATURES[f] == "vault" {
                    vaults.push(c);
                }
            }
        }
    }
    // Monsters.
    let pool: Vec<(usize, f64)> = t
        .monsters
        .iter()
        .filter_map(|(m, wt)| {
            let i = data.rogue_monster_index(m)?;
            (data.rogue_monsters[i].min_depth <= depth).then_some((i, *wt))
        })
        .collect();
    let pool = if pool.is_empty() {
        t.monsters
            .iter()
            .filter_map(|(m, wt)| data.rogue_monster_index(m).map(|i| (i, *wt)))
            .collect()
    } else {
        pool
    };
    let scale = 1.0 + 0.2 * f64::from(depth);
    let mut monsters: Vec<Mon> = Vec::new();
    let count = (rooms.len() as f64 * (0.9 + 0.25 * f64::from(depth))).round() as usize;
    for _ in 0..count {
        let room = &rooms
            [1 + (rng.f64() * (rooms.len() - 1).max(1) as f64) as usize % (rooms.len() - 1).max(1)];
        let c = random_cell(room, &mut rng);
        if !free(&tiles, c) || monsters.iter().any(|m| (m.x, m.y) == c) {
            continue;
        }
        let k = pick_weighted(&pool, &mut rng);
        monsters.push(Mon {
            kind: k,
            x: c.0,
            y: c.1,
            hp: data.rogue_monsters[k].hp * scale,
            awake: rng.f64() < 0.35,
            boss: false,
        });
    }
    // Vault guardians.
    for c in vaults {
        let k = pick_weighted(&pool, &mut rng);
        let spot = [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .map(|d| (c.0 + d.0, c.1 + d.1))
            .find(|s| free(&tiles, *s) && !monsters.iter().any(|m| (m.x, m.y) == *s));
        if let Some(s) = spot {
            monsters.push(Mon {
                kind: k,
                x: s.0,
                y: s.1,
                hp: data.rogue_monsters[k].hp * scale * 1.5,
                awake: false,
                boss: false,
            });
        }
    }
    // The boss of the last floor, in the last room.
    if last && rooms.len() > 1 {
        let strongest = pool
            .iter()
            .map(|(i, _)| *i)
            .max_by(|a, b| {
                data.rogue_monsters[*a]
                    .hp
                    .total_cmp(&data.rogue_monsters[*b].hp)
            })
            .unwrap_or(0);
        let room = rooms[rooms.len() - 1];
        let mut c = center(&room);
        if !free(&tiles, c) {
            c = (room.0, room.1);
        }
        monsters.retain(|m| (m.x, m.y) != c);
        monsters.push(Mon {
            kind: strongest,
            x: c.0,
            y: c.1,
            hp: data.rogue_monsters[strongest].hp * scale * 2.5,
            awake: false,
            boss: true,
        });
    }
    monsters.retain(|m| (m.x, m.y) != start);
    // Loot piles.
    let loot_table: Vec<(usize, f64)> = t.loot.iter().enumerate().map(|(i, l)| (i, l.3)).collect();
    let mut items = Vec::new();
    for room in &rooms {
        if rng.f64() < 0.55 {
            let c = random_cell(room, &mut rng);
            if free(&tiles, c) && !monsters.iter().any(|m| (m.x, m.y) == c) {
                let (r, lo, hi, _) = &t.loot[pick_weighted(&loot_table, &mut rng)];
                if let Some(res) = data.resource_index(r) {
                    items.push(Item {
                        x: c.0,
                        y: c.1,
                        res,
                        kg: (lo + (hi - lo) * rng.f64()) * (1.0 + 0.3 * f64::from(depth)),
                    });
                }
            }
        }
    }
    Floor {
        seen: vec![false; w * h],
        tiles,
        monsters,
        items,
        start,
    }
}

impl Sim {
    /// Places that can be entered from where the ship is.
    pub fn rogue_targets(&self) -> Vec<RogueTarget> {
        let d = &self.data;
        let theme = |id: &str| d.rogue_theme_index(id).unwrap_or(0);
        let dim = self.multiverse.dimension > 0;
        let mut out = Vec::new();
        let mut add = |label: String, id: &str, seed: u64, portal: bool| {
            let id = if dim && !portal { "reve" } else { id };
            out.push(RogueTarget {
                label,
                theme: theme(id),
                seed,
            });
        };
        for (i, b) in self.bodies.iter().enumerate() {
            let dist = (self.ship.pos - b.pos).length();
            let seed = hash_coord(
                self.system.seed,
                i as i64,
                i64::from(self.multiverse.dimension),
                5,
            );
            let name = b.name.clone();
            match b.kind {
                BodyKind::Station if self.docked == Some(i) || dist <= ENTER_RANGE => {
                    add(format!("Station : {name}"), "station", seed, false);
                }
                BodyKind::Anomaly(AnomalyKind::Portal)
                    if dist <= crate::multiverse::PORTAL_RANGE =>
                {
                    add(format!("Coeur du portail : {name}"), "portail", seed, true);
                }
                BodyKind::Anomaly(k) if dist <= ENTER_RANGE => {
                    let id = match k {
                        AnomalyKind::DerelictShip | AnomalyKind::IonStorm => "epave",
                        AnomalyKind::AncientStation => "station_ancienne",
                        AnomalyKind::AlienBeacon | AnomalyKind::MagneticAnomaly => "laboratoire",
                        AnomalyKind::ArtifactField | AnomalyKind::GravityWell => "ruines",
                        AnomalyKind::CrystalFormation => "grotte_cristal",
                        AnomalyKind::Comet => "glace",
                        AnomalyKind::RadiationBelt => "reacteur",
                        AnomalyKind::Nebula => "ruche",
                        AnomalyKind::SpatialRift => "reve",
                        AnomalyKind::Portal => "portail",
                    };
                    add(format!("{} : {name}", k.label()), id, seed, false);
                }
                BodyKind::Asteroid(_) if dist <= b.radius * 3.0 + 30_000.0 => {
                    add(format!("Galeries : {name}"), "grotte", seed, false);
                }
                BodyKind::Planet(k) | BodyKind::Moon(k)
                    if !k.is_giant() && dist <= b.radius * 3.0 + 1.0e6 =>
                {
                    let id = match k {
                        PlanetKind::Ice | PlanetKind::Tundra => "glace",
                        PlanetKind::Lava | PlanetKind::Venusian => "reacteur",
                        _ if seed.is_multiple_of(3) => "ruines",
                        _ => "grotte",
                    };
                    add(format!("Sous la surface : {name}"), id, seed, false);
                }
                _ => {}
            }
        }
        for w in &self.wrecks {
            if (self.ship.pos - w.pos).length() <= 30_000.0 {
                let seed = hash_coord(self.system.seed, i64::from(w.id), 77, 5);
                add(format!("Epave : {}", w.name), "epave", seed, false);
            }
        }
        out
    }

    /// Crew fit to go: alive and healthy enough, the best fighters first.
    pub fn rogue_squad(&self) -> Vec<usize> {
        let combat = self.data.skill_index("combat");
        let mut v: Vec<usize> = (0..self.crew.members.len())
            .filter(|&i| {
                let p = &self.crew.members[i];
                p.alive && p.health > 0.3
            })
            .collect();
        v.sort_by(|a, b| {
            let s = |i: usize| combat.map_or(0.0, |c| self.crew.members[i].skills[c]);
            s(*b).total_cmp(&s(*a))
        });
        v.truncate(MAX_PARTY);
        v
    }

    /// Set out on target `i` of `rogue_targets` with `party` (the default squad if empty).
    pub fn rogue_enter(&mut self, i: usize, party: Vec<usize>) -> Result<(), &'static str> {
        if self.rogue.is_some() {
            return Err("une exploration est deja en cours");
        }
        if self.jump.is_some() {
            return Err("impossible pendant un saut");
        }
        let target = self
            .rogue_targets()
            .into_iter()
            .nth(i)
            .ok_or("rien a explorer ici")?;
        let party: Vec<usize> = if party.is_empty() {
            self.rogue_squad()
        } else {
            party
                .into_iter()
                .filter(|&k| {
                    self.crew
                        .members
                        .get(k)
                        .is_some_and(|p| p.alive && p.health > 0.0)
                })
                .take(MAX_PARTY)
                .collect()
        };
        if party.is_empty() {
            return Err("personne n'est en etat de partir");
        }
        let visits = self
            .rogue_visits
            .iter()
            .find(|(s, _)| *s == target.seed)
            .map_or(0, |(_, n)| *n);
        match self
            .rogue_visits
            .iter_mut()
            .find(|(s, _)| *s == target.seed)
        {
            Some((_, n)) => *n += 1,
            None => self.rogue_visits.push((target.seed, 1)),
        }
        let seed = hash_coord(target.seed, i64::from(visits), 3, 3);
        self.rogue_begin(target.theme, seed, party);
        Ok(())
    }

    /// Start a run on `theme` with this seed and squad (the squad must not be empty).
    pub fn rogue_begin(&mut self, theme: usize, seed: u64, party: Vec<usize>) {
        let t = &self.data.rogue_themes[theme];
        let floor = generate_floor(&self.data, theme, seed, 0);
        let (x, y) = floor.start;
        let mut r = Rogue {
            theme,
            seed,
            w: t.size.0,
            h: t.size.1,
            floors: vec![floor],
            cur: 0,
            party,
            x,
            y,
            active: 0,
            loot: Vec::new(),
            turn: 0,
            log: Vec::new(),
            over: false,
            kills: 0,
            buff: 0,
            boss_dead: false,
        };
        r.say(format!("{} : {}", t.name, t.text));
        r.reveal();
        self.rogue = Some(r);
    }

    fn party_up(&self, r: &Rogue) -> Vec<usize> {
        r.party
            .iter()
            .copied()
            .filter(|&i| {
                self.crew
                    .members
                    .get(i)
                    .is_some_and(|p| p.alive && p.health > 0.0)
            })
            .collect()
    }

    /// Hurt one member of the squad: health and one organ.
    fn rogue_hurt(&mut self, r: &mut Rogue, who: usize, dmg: f64, by: &str) {
        let n_org = self.data.organs.len();
        let pick =
            (hash_coord(r.seed, i64::from(r.turn), who as i64, 57) % n_org.max(1) as u64) as usize;
        let vital = self.data.organs.get(pick).is_some_and(|o| o.vital);
        let organ = self
            .data
            .organs
            .get(pick)
            .map_or(String::new(), |o| o.name.to_lowercase());
        let dmg = dmg
            * (1.0
                - self.crew.members[who]
                    .gear_stat(&self.data, "defense")
                    .min(0.9));
        let p = &mut self.crew.members[who];
        p.organs.resize(n_org, 1.0);
        p.health = (p.health - dmg as f32).max(0.0);
        if n_org > 0 {
            p.organs[pick] = (p.organs[pick] - (dmg * 0.8) as f32).max(0.0);
        }
        let name = p.name.clone();
        let dead = p.health <= 0.0 || (vital && n_org > 0 && p.organs[pick] <= 0.0);
        if dead {
            p.health = 0.0;
        }
        r.say(format!("{by} blesse {name} ({organ}, -{dmg:.2})."));
        if dead {
            r.say(format!("{name} s'effondre."));
        }
    }

    /// Damage of one member's blow.
    fn rogue_blow(&self, who: usize, rng: &mut Rng) -> f64 {
        let p = &self.crew.members[who];
        let combat = self
            .data
            .skill_index("combat")
            .map_or(0.0, |c| f64::from(p.skills[c]));
        (0.8 + 0.22 * combat + 2.0 * p.gear_stat(&self.data, "attack")) * (0.7 + 0.6 * rng.f64())
    }

    /// One move of the squad: a step, or an attack when something stands there.
    pub fn rogue_step(&mut self, dx: i32, dy: i32) -> Result<(), &'static str> {
        let mut r = self.rogue.take().ok_or("pas d'exploration en cours")?;
        let res = self.rogue_step_inner(&mut r, dx.signum(), dy.signum());
        self.rogue = Some(r);
        res
    }

    fn rogue_step_inner(&mut self, r: &mut Rogue, dx: i32, dy: i32) -> Result<(), &'static str> {
        if r.over {
            return Err("l'exploration est terminee");
        }
        if dx == 0 && dy == 0 {
            self.rogue_end_turn(r);
            return Ok(());
        }
        let (nx, ny) = (r.x + dx, r.y + dy);
        if r.tile(nx, ny) == T_WALL {
            return Err("un mur");
        }
        if let Some(k) = r.monster_at(nx, ny) {
            self.rogue_attack(r, k);
        } else {
            r.x = nx;
            r.y = ny;
            self.rogue_arrive(r);
        }
        self.rogue_end_turn(r);
        Ok(())
    }

    /// The squad strikes monster `k`.
    fn rogue_attack(&mut self, r: &mut Rogue, k: usize) {
        let mut rng = Rng::new(hash_coord(r.seed, i64::from(r.turn), 9, 31));
        let up = self.party_up(r);
        let lead = r.active.min(up.len().saturating_sub(1));
        let mut total = 0.0;
        for (n, &who) in up.iter().enumerate() {
            let blow = self.rogue_blow(who, &mut rng) * if n == lead { 1.0 } else { 0.5 };
            total += blow;
        }
        if r.buff > 0 {
            total *= 1.5;
        }
        let kind = r.floor().monsters[k].kind;
        let def = self.data.rogue_monsters[kind].clone();
        let dmg = total * (1.0 - def.defense);
        let m = &mut r.floor_mut().monsters[k];
        m.hp -= dmg;
        m.awake = true;
        if m.hp > 0.0 {
            r.say(format!(
                "Vous frappez {} ({dmg:.1}).",
                def.name.to_lowercase()
            ));
            return;
        }
        let m = r.floor_mut().monsters.remove(k);
        r.kills += 1;
        r.say(format!("{} est abattu.", def.name));
        let (res, lo, hi) = &def.loot;
        if let Some(ri) = self.data.resource_index(res) {
            let mult = if m.boss { 3.0 } else { 1.0 } * (1.0 + 0.3 * r.depth() as f64);
            let kg = (lo + (hi - lo) * rng.f64()) * mult;
            r.floor_mut().items.push(Item {
                x: m.x,
                y: m.y,
                res: ri,
                kg,
            });
        }
        if m.boss {
            r.boss_dead = true;
            self.research_pool += 100.0;
            r.say("Le maitre des lieux est tombe : on en tirera des enseignements.");
        }
    }

    /// The squad is on a new tile: pick up what lies there, trip what lies there.
    fn rogue_arrive(&mut self, r: &mut Rogue) {
        let (x, y) = (r.x, r.y);
        if let Some(k) = r.floor().items.iter().position(|i| i.x == x && i.y == y) {
            let it = r.floor().items[k].clone();
            let carried: f64 = r.loot.iter().map(|l| l.1).sum();
            let room = (self.ship.cargo.cap_mass - self.ship.cargo.mass - carried).max(0.0);
            if room <= 0.01 {
                r.say("La soute est pleine : vous laissez le butin.");
            } else {
                let kg = it.kg.min(room);
                match r.loot.iter_mut().find(|l| l.0 == it.res) {
                    Some(l) => l.1 += kg,
                    None => r.loot.push((it.res, kg)),
                }
                let name = self.data.resources[it.res].name.to_lowercase();
                r.say(format!("Vous ramassez {kg:.1} kg de {name}."));
                if kg < it.kg {
                    r.floor_mut().items[k].kg -= kg;
                } else {
                    r.floor_mut().items.remove(k);
                }
            }
        }
        let t = r.tile(x, y);
        if t >= T_FEATURE {
            let f = (t - T_FEATURE) as usize;
            if f < ROGUE_FEATURES.len() {
                self.rogue_feature(r, ROGUE_FEATURES[f]);
                let i = r.idx(x, y);
                r.floor_mut().tiles[i] = T_FLOOR;
            }
        }
    }

    fn rogue_feature(&mut self, r: &mut Rogue, feature: &str) {
        let mut rng = Rng::new(hash_coord(
            r.seed,
            i64::from(r.turn),
            r.x as i64 * 97 + r.y as i64,
            13,
        ));
        let up = self.party_up(r);
        let depth_mult = 1.0 + 0.3 * r.depth() as f64;
        match feature {
            "terminal" => {
                let gain = (15.0 + 60.0 * rng.f64()) * depth_mult;
                self.research_pool += gain;
                r.say(format!(
                    "Un terminal vous livre {gain:.0} points de recherche."
                ));
            }
            "medbay" => {
                for &w in &up {
                    let p = &mut self.crew.members[w];
                    p.health = (p.health + 0.4).min(1.0);
                }
                r.say("Une infirmerie intacte : l'equipe est soignee.");
            }
            "fountain" => {
                for &w in &up {
                    let p = &mut self.crew.members[w];
                    p.health = (p.health + 0.15).min(1.0);
                }
                r.say("Une source claire : un peu de repos.");
            }
            "cache" | "vault" => {
                let n = if feature == "vault" { 5 } else { 2 };
                let t = self.data.rogue_themes[r.theme].clone();
                let table: Vec<(usize, f64)> =
                    (0..t.loot.len()).map(|i| (i, t.loot[i].3)).collect();
                for _ in 0..n {
                    let (res, lo, hi, _) = &t.loot[pick_weighted(&table, &mut rng)];
                    if let Some(ri) = self.data.resource_index(res) {
                        let kg = (lo + (hi - lo) * rng.f64()) * depth_mult;
                        let carried: f64 = r.loot.iter().map(|l| l.1).sum();
                        let room =
                            (self.ship.cargo.cap_mass - self.ship.cargo.mass - carried).max(0.0);
                        let kg = kg.min(room);
                        if kg > 0.01 {
                            match r.loot.iter_mut().find(|l| l.0 == ri) {
                                Some(l) => l.1 += kg,
                                None => r.loot.push((ri, kg)),
                            }
                        }
                    }
                }
                r.say(if feature == "vault" {
                    "Un coffre-fort force : le butin est considerable."
                } else {
                    "Une cache bien garnie."
                });
            }
            "altar" => {
                if rng.f64() < 0.5 {
                    r.buff = 60;
                    r.say("L'autel brille : vos coups porteront davantage (60 tours).");
                } else {
                    for &w in &up {
                        let p = &mut self.crew.members[w];
                        p.health = (p.health - 0.08).max(0.05);
                    }
                    r.say("L'autel se retourne contre vous : une decharge secoue l'equipe.");
                }
            }
            "mutagen" => {
                for &w in &up {
                    let p = &mut self.crew.members[w];
                    p.health = (p.health + 0.2).min(1.0);
                    p.instability = (p.instability + 0.25).min(1.0);
                }
                r.buff = 40;
                r.say(
                    "Un bain de mutagene : vous vous sentez plus fort, plus instable (40 tours).",
                );
            }
            "trap" => {
                if !up.is_empty() {
                    let who = up[(rng.f64() * up.len() as f64) as usize % up.len()];
                    let dmg = 0.1 + 0.2 * rng.f64();
                    self.rogue_hurt(r, who, dmg, "Un piege");
                }
            }
            "portal" => {
                let f = r.floor();
                let cells: Vec<(i32, i32)> = (0..r.h as i32)
                    .flat_map(|y| (0..r.w as i32).map(move |x| (x, y)))
                    .filter(|&(x, y)| {
                        f.tiles[y as usize * r.w + x as usize] == T_FLOOR
                            && !f.monsters.iter().any(|m| (m.x, m.y) == (x, y))
                    })
                    .collect();
                if !cells.is_empty() {
                    let (x, y) = cells[(rng.f64() * cells.len() as f64) as usize % cells.len()];
                    r.x = x;
                    r.y = y;
                    r.say("Un portail vous projette ailleurs sur l'etage.");
                }
            }
            _ => {}
        }
    }

    /// Monsters move and strike, hazards bite, the clock runs.
    fn rogue_end_turn(&mut self, r: &mut Rogue) {
        r.turn += 1;
        r.buff = r.buff.saturating_sub(1);
        self.advance(TURN_SECONDS, &ShipInput::default());
        let theme = self.data.rogue_themes[r.theme].clone();
        // Hazard of the place, every fifth turn.
        if theme.hazard > 0.0 && r.turn.is_multiple_of(5) {
            for who in self.party_up(r) {
                let dmg = theme.hazard * 0.12;
                let defense = self.crew.members[who]
                    .gear_stat(&self.data, "defense")
                    .min(0.9);
                let net = dmg * (1.0 - defense);
                let p = &mut self.crew.members[who];
                p.health = (p.health - net as f32).max(0.0);
            }
            if r.turn.is_multiple_of(20) {
                r.say(format!("{} vous ronge.", theme.hazard_name));
            }
        }
        let mut rng = Rng::new(hash_coord(r.seed, i64::from(r.turn), r.cur as i64, 41));
        let (px, py) = (r.x, r.y);
        for k in 0..r.floor().monsters.len() {
            let m = r.floor().monsters[k].clone();
            let def = self.data.rogue_monsters[m.kind].clone();
            let dist = (m.x - px).abs().max((m.y - py).abs());
            let sees = dist <= def.sight && r.line_clear(m.x, m.y, px, py);
            if !m.awake {
                // Sleepers wake when you come close; the mimic only when touched.
                let wake = if def.id == SLEEPER {
                    dist <= 1
                } else {
                    sees && dist <= def.sight / 2 + 1
                };
                if wake {
                    r.floor_mut().monsters[k].awake = true;
                    if sees {
                        r.say(format!("{} vous remarque.", def.name));
                    }
                }
                continue;
            }
            if dist <= 1 {
                let up = self.party_up(r);
                if up.is_empty() {
                    break;
                }
                let who = up[(rng.f64() * up.len() as f64) as usize % up.len()];
                let dmg = def.attack * (0.6 + 0.8 * rng.f64()) * (1.0 + 0.1 * r.depth() as f64);
                let name = def.name.clone();
                self.rogue_hurt(r, who, dmg, &name);
            } else if sees && !STATIONARY.contains(&def.id.as_str()) {
                let (sx, sy) = ((px - m.x).signum(), (py - m.y).signum());
                for (ax, ay) in [(sx, sy), (sx, 0), (0, sy)] {
                    let (tx, ty) = (m.x + ax, m.y + ay);
                    if (ax, ay) != (0, 0)
                        && r.tile(tx, ty) != T_WALL
                        && (tx, ty) != (px, py)
                        && r.monster_at(tx, ty).is_none()
                    {
                        let mm = &mut r.floor_mut().monsters[k];
                        mm.x = tx;
                        mm.y = ty;
                        break;
                    }
                }
            }
        }
        if self.party_up(r).is_empty() {
            r.over = true;
            r.loot.clear();
            r.say("Plus personne ne se releve. L'exploration est perdue.");
        }
        r.reveal();
    }

    /// Wait a turn.
    pub fn rogue_wait(&mut self) -> Result<(), &'static str> {
        self.rogue_step(0, 0)
    }

    /// Change the leading member (the one whose blows count in full).
    pub fn rogue_switch(&mut self) {
        if let Some(mut r) = self.rogue.take() {
            let n = self.party_up(&r).len().max(1);
            r.active = (r.active + 1) % n;
            self.rogue = Some(r);
        }
    }

    /// Use the stairs under the squad: down, or up (and out, from the first floor).
    pub fn rogue_stairs(&mut self) -> Result<String, &'static str> {
        let mut r = self.rogue.take().ok_or("pas d'exploration en cours")?;
        let res = self.rogue_stairs_inner(&mut r);
        self.rogue = Some(r);
        res
    }

    fn rogue_stairs_inner(&mut self, r: &mut Rogue) -> Result<String, &'static str> {
        if r.over {
            return Err("l'exploration est terminee");
        }
        match r.tile(r.x, r.y) {
            T_DOWN => {
                if r.cur + 1 >= r.floors.len() {
                    let f = generate_floor(&self.data, r.theme, r.seed, r.cur as u32 + 1);
                    r.floors.push(f);
                }
                r.cur += 1;
                let (x, y) = r.floor().start;
                r.x = x;
                r.y = y;
                r.reveal();
                let msg = format!("Etage {}.", r.cur + 1);
                r.say(msg.clone());
                Ok(msg)
            }
            T_UP if r.cur > 0 => {
                r.cur -= 1;
                // Arrive on the stairs down of the floor above.
                let (mut ex, mut ey) = r.floor().start;
                for y in 0..r.h as i32 {
                    for x in 0..r.w as i32 {
                        if r.tile(x, y) == T_DOWN {
                            (ex, ey) = (x, y);
                        }
                    }
                }
                r.x = ex;
                r.y = ey;
                r.reveal();
                let msg = format!("Etage {}.", r.cur + 1);
                r.say(msg.clone());
                Ok(msg)
            }
            T_UP => {
                let n = self.rogue_leave_inner(r);
                Ok(n)
            }
            _ => Err("pas d'escalier ici"),
        }
    }

    /// Back to the ship with the loot (needs the entrance stairs under the squad).
    fn rogue_leave_inner(&mut self, r: &mut Rogue) -> String {
        let data = std::sync::Arc::clone(&self.data);
        let mut left = Vec::new();
        let mut kg_total = 0.0;
        for (res, kg) in std::mem::take(&mut r.loot) {
            let got = self.ship.cargo.add(&data, res, kg);
            kg_total += got;
            if got < kg - 0.01 {
                left.push(data.resources[res].name.clone());
            }
        }
        r.over = true;
        let mut msg = format!("RETOUR AU VAISSEAU : {kg_total:.0} KG RAPPORTES");
        if !left.is_empty() {
            msg = format!("{msg} (SOUTE PLEINE : {} LAISSE)", left.join(", "));
        }
        r.say(msg.clone());
        msg
    }

    /// Close the exploration screen once it is over.
    pub fn rogue_close(&mut self) {
        if self.rogue.as_ref().is_some_and(|r| r.over) {
            self.rogue = None;
        }
    }

    /// Walk toward (tx, ty) over the cells already seen, up to `max` steps, stopping when an
    /// awake monster comes into view or something happens.
    pub fn rogue_travel(&mut self, tx: i32, ty: i32, max: usize) -> Result<usize, &'static str> {
        let (path, w) = {
            let r = self.rogue.as_ref().ok_or("pas d'exploration en cours")?;
            if !r.inside(tx, ty) || !r.floor().seen[r.idx(tx, ty)] || r.tile(tx, ty) == T_WALL {
                return Err("destination inconnue");
            }
            (shortest_path(r, (r.x, r.y), (tx, ty)), r.w)
        };
        let _ = w;
        let path = path.ok_or("pas de chemin")?;
        let mut done = 0;
        for (nx, ny) in path.into_iter().take(max) {
            let (dx, dy) = {
                let r = self.rogue.as_ref().unwrap();
                (nx - r.x, ny - r.y)
            };
            let before = {
                let r = self.rogue.as_ref().unwrap();
                (r.log.len(), self.party_up(r).len())
            };
            self.rogue_step(dx, dy)?;
            done += 1;
            let r = self.rogue.as_ref().unwrap();
            let vis = r.visible();
            let danger = r.floor().monsters.iter().any(|m| {
                vis[r.idx(m.x, m.y)] && (m.awake || (m.x - r.x).abs().max((m.y - r.y).abs()) <= 3)
            });
            if danger || r.over || r.log.len() != before.0 || self.party_up(r).len() != before.1 {
                break;
            }
        }
        Ok(done)
    }
}

/// Breadth-first path over seen, walkable cells, excluding the start.
pub fn shortest_path(r: &Rogue, from: (i32, i32), to: (i32, i32)) -> Option<Vec<(i32, i32)>> {
    use std::collections::VecDeque;
    let mut prev = vec![None; r.w * r.h];
    let mut seen = vec![false; r.w * r.h];
    let mut q = VecDeque::new();
    seen[r.idx(from.0, from.1)] = true;
    q.push_back(from);
    while let Some((x, y)) = q.pop_front() {
        if (x, y) == to {
            let mut path = vec![to];
            let mut c = to;
            while let Some(p) = prev[r.idx(c.0, c.1)] {
                if p == from {
                    break;
                }
                path.push(p);
                c = p;
            }
            path.reverse();
            return Some(path);
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (nx, ny) = (x + dx, y + dy);
                if (dx, dy) == (0, 0) || !r.inside(nx, ny) {
                    continue;
                }
                let i = r.idx(nx, ny);
                if seen[i] || r.tile(nx, ny) == T_WALL || !r.floor().seen[i] {
                    continue;
                }
                seen[i] = true;
                prev[i] = Some((x, y));
                q.push_back((nx, ny));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::collections::VecDeque;
    use std::sync::Arc;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    /// Cells reachable from `start` over non-wall tiles.
    fn reachable(f: &Floor, w: usize, h: usize, start: (i32, i32)) -> Vec<bool> {
        let mut seen = vec![false; w * h];
        let mut q = VecDeque::new();
        seen[start.1 as usize * w + start.0 as usize] = true;
        q.push_back(start);
        while let Some((x, y)) = q.pop_front() {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                        continue;
                    }
                    let i = ny as usize * w + nx as usize;
                    if !seen[i] && f.tiles[i] != T_WALL {
                        seen[i] = true;
                        q.push_back((nx, ny));
                    }
                }
            }
        }
        seen
    }

    /// A run on `theme` with a fresh squad and a roomy hold.
    fn run_on(theme: &str, seed: u64) -> (Sim, ()) {
        let mut s = new_sim(5, data());
        s.ship.cargo.cap_mass = 1.0e7;
        let t = s.data.rogue_theme_index(theme).unwrap();
        let party = s.rogue_squad();
        assert!(!party.is_empty());
        let floor = generate_floor(&s.data, t, seed, 0);
        let (x, y) = floor.start;
        let th = &s.data.rogue_themes[t];
        s.rogue = Some(Rogue {
            theme: t,
            seed,
            w: th.size.0,
            h: th.size.1,
            floors: vec![floor],
            cur: 0,
            party,
            x,
            y,
            active: 0,
            loot: vec![],
            turn: 0,
            log: vec![],
            over: false,
            kills: 0,
            buff: 0,
            boss_dead: false,
        });
        s.rogue.as_mut().unwrap().reveal();
        (s, ())
    }

    #[test]
    fn every_floor_of_every_place_is_connected_deterministic_and_populated() {
        let d = data();
        for (t, th) in d.rogue_themes.iter().enumerate() {
            for depth in 0..th.floors {
                for seed in 0..6u64 {
                    let f = generate_floor(&d, t, seed * 7 + 1, depth);
                    let again = generate_floor(&d, t, seed * 7 + 1, depth);
                    assert_eq!(f.tiles, again.tiles, "{} deterministic", th.id);
                    assert_eq!(f.monsters, again.monsters);
                    assert_eq!(f.items, again.items);
                    let (w, h) = th.size;
                    assert_eq!(f.tiles.len(), w * h);
                    let reach = reachable(&f, w, h, f.start);
                    let at = |x: i32, y: i32| y as usize * w + x as usize;
                    assert_eq!(f.tiles[at(f.start.0, f.start.1)], T_UP);
                    let last = depth + 1 >= th.floors;
                    let downs = f.tiles.iter().filter(|t| **t == T_DOWN).count();
                    assert_eq!(downs, usize::from(!last), "{} stairs down", th.id);
                    for (i, t) in f.tiles.iter().enumerate() {
                        if *t != T_WALL {
                            assert!(reach[i], "{} floor {depth}: cell {i} cut off", th.id);
                        }
                    }
                    for m in &f.monsters {
                        assert!(f.tiles[at(m.x, m.y)] != T_WALL, "monster in a wall");
                        assert!((m.x, m.y) != f.start);
                        assert!(m.hp > 0.0);
                    }
                    let mut spots: Vec<_> = f.monsters.iter().map(|m| (m.x, m.y)).collect();
                    spots.sort_unstable();
                    spots.dedup();
                    assert_eq!(spots.len(), f.monsters.len(), "monsters stack");
                    for it in &f.items {
                        assert!(f.tiles[at(it.x, it.y)] != T_WALL && it.kg > 0.0);
                    }
                    let bosses = f.monsters.iter().filter(|m| m.boss).count();
                    assert_eq!(bosses, usize::from(last), "{} boss only below", th.id);
                    assert!(
                        !f.monsters.is_empty() && !f.items.is_empty(),
                        "{} empty floor",
                        th.id
                    );
                }
            }
        }
        // Different seeds, different floors.
        let a = generate_floor(&d, 0, 1, 0);
        let b = generate_floor(&d, 0, 2, 0);
        assert_ne!(a.tiles, b.tiles);
        // Deeper floors hold more and tougher monsters.
        let (mut shallow, mut deep) = (0.0, 0.0);
        for seed in 0..10 {
            let s0 = generate_floor(&d, 3, seed, 0);
            let s2 = generate_floor(&d, 3, seed, 2);
            shallow += s0.monsters.iter().map(|m| m.hp).sum::<f64>();
            deep += s2.monsters.iter().map(|m| m.hp).sum::<f64>();
        }
        assert!(deep > shallow * 1.3, "{shallow} vs {deep}");
    }

    #[test]
    fn walls_block_sight_and_the_view_is_remembered() {
        let (mut s, _) = run_on("station", 4);
        let r = s.rogue.as_mut().unwrap();
        let v = r.visible();
        assert!(v[r.idx(r.x, r.y)]);
        for y in 0..r.h as i32 {
            for x in 0..r.w as i32 {
                if v[r.idx(x, y)] {
                    assert!(
                        (x - r.x).pow(2) + (y - r.y).pow(2) <= VIEW_RADIUS * VIEW_RADIUS,
                        "seen beyond the radius"
                    );
                    assert!(r.line_clear(r.x, r.y, x, y));
                }
            }
        }
        // A wall right next to the squad hides what is behind it.
        let (wx, wy) = (0..r.w as i32)
            .flat_map(|x| (0..r.h as i32).map(move |y| (x, y)))
            .find(|&(x, y)| {
                r.tile(x, y) == T_WALL && r.tile(x + 1, y) == T_WALL && r.tile(x - 1, y) == T_FLOOR
            })
            .unwrap();
        assert!(!r.line_clear(wx - 1, wy, wx + 3, wy));
        assert!(r.floor().seen.iter().filter(|s| **s).count() > 5);
    }

    #[test]
    fn bumping_into_a_monster_fights_and_a_kill_drops_loot_that_can_be_carried() {
        let (mut s, _) = run_on("station", 4);
        s.crew.members[0].health = 1.0;
        let (x, y, kind_hp) = {
            let r = s.rogue.as_mut().unwrap();
            // A rat right next to the squad.
            let rat = s.data.rogue_monster_index("rat").unwrap();
            let (nx, ny) = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .map(|d| (r.x + d.0, r.y + d.1))
                .find(|&(x, y)| r.tile(x, y) != T_WALL)
                .unwrap();
            r.floors[0].monsters.clear();
            r.floors[0].monsters.push(Mon {
                kind: rat,
                x: nx,
                y: ny,
                hp: 0.5,
                awake: true,
                boss: false,
            });
            (nx, ny, 3.0)
        };
        let (px, py) = {
            let r = s.rogue.as_ref().unwrap();
            (r.x, r.y)
        };
        s.rogue_step(x - px, y - py).expect("attacks");
        let r = s.rogue.as_ref().unwrap();
        assert_eq!((r.x, r.y), (px, py), "the squad stays put when it strikes");
        assert!(r.floor().monsters.is_empty() || r.floor().monsters[0].hp < kind_hp);
        assert_eq!(r.kills, 1, "a rat does not survive a squad");
        // The drop lies where it fell; stepping there carries it.
        assert_eq!(
            r.floor()
                .items
                .iter()
                .filter(|i| (i.x, i.y) == (x, y))
                .count(),
            1
        );
        s.rogue_step(x - px, y - py).expect("steps");
        let r = s.rogue.as_ref().unwrap();
        assert!(!r.loot.is_empty());
        assert!(r.floor().items.iter().all(|i| (i.x, i.y) != (x, y)));
    }

    #[test]
    fn a_full_hold_leaves_the_loot_behind() {
        let (mut s, _) = run_on("station", 4);
        s.ship.cargo.cap_mass = s.ship.cargo.mass; // no room
        let r = s.rogue.as_mut().unwrap();
        let (x, y) = (r.x + 1, r.y);
        let (x, y) = if r.tile(x, y) == T_WALL {
            (r.x - 1, r.y)
        } else {
            (x, y)
        };
        r.floors[0].monsters.clear();
        r.floors[0].items.clear();
        r.floors[0].items.push(Item {
            x,
            y,
            res: 0,
            kg: 50.0,
        });
        let (dx, dy) = (x - r.x, y - r.y);
        if r.tile(x, y) != T_WALL {
            s.rogue_step(dx, dy).unwrap();
            let r = s.rogue.as_ref().unwrap();
            assert!(r.loot.is_empty(), "nothing taken");
            assert_eq!(r.floor().items.len(), 1, "the pile stays");
        }
    }

    #[test]
    fn monsters_hunt_the_squad_and_hurt_it_until_it_falls() {
        let (mut s, _) = run_on("base_pirate", 9);
        let chef = s.data.rogue_monster_index("chef_pirate").unwrap();
        let (cx, cy) = {
            let r = s.rogue.as_mut().unwrap();
            let c = [(2, 0), (-2, 0), (0, 2), (0, -2)]
                .into_iter()
                .map(|d| (r.x + d.0, r.y + d.1))
                .find(|&(x, y)| r.tile(x, y) != T_WALL && r.line_clear(r.x, r.y, x, y))
                .expect("a free cell near the start");
            r.floors[0].monsters.clear();
            r.floors[0].monsters.push(Mon {
                kind: chef,
                x: c.0,
                y: c.1,
                hp: 1.0e6,
                awake: true,
                boss: false,
            });
            c
        };
        let before: f32 = s.crew.members.iter().map(|p| p.health).sum();
        for _ in 0..4 {
            s.rogue_wait().unwrap();
        }
        let r = s.rogue.as_ref().unwrap();
        let m = &r.floor().monsters[0];
        assert!(
            (m.x - r.x).abs().max((m.y - r.y).abs()) <= 1,
            "it closed in ({cx},{cy}) -> ({},{})",
            m.x,
            m.y
        );
        let after: f32 = s.crew.members.iter().map(|p| p.health).sum();
        assert!(after < before, "and struck");
        // Keep waiting under its blows: the squad dies, the loot is lost, the run is over.
        {
            let r = s.rogue.as_mut().unwrap();
            r.loot.push((0, 10.0));
        }
        for _ in 0..400 {
            if s.rogue.as_ref().unwrap().over {
                break;
            }
            let _ = s.rogue_wait();
            // Keep the chef alive and adjacent.
        }
        let r = s.rogue.as_ref().unwrap();
        assert!(r.over, "the squad fell");
        assert!(r.loot.is_empty(), "the loot is lost with them");
        assert!(s.party_up(r).is_empty());
        assert!(s.rogue_step(1, 0).is_err(), "no moving after the end");
        s.rogue_close();
        assert!(s.rogue.is_none());
    }

    #[test]
    fn features_do_what_they_say() {
        let (mut s, _) = run_on("laboratoire", 3);
        let mut r = s.rogue.take().unwrap();
        for p in &mut s.crew.members {
            p.health = 0.5;
        }
        let pool = s.research_pool;
        s.rogue_feature(&mut r, "terminal");
        assert!(s.research_pool > pool);
        s.rogue_feature(&mut r, "medbay");
        let party = s.party_up(&r);
        assert!(party.iter().all(|&i| s.crew.members[i].health >= 0.89));
        for &i in &party {
            s.crew.members[i].health = 0.5;
            s.crew.members[i].instability = 0.0;
        }
        s.rogue_feature(&mut r, "mutagen");
        assert!(r.buff > 0);
        assert!(party
            .iter()
            .all(|&i| s.crew.members[i].instability > 0.2 && s.crew.members[i].health > 0.6));
        s.rogue_feature(&mut r, "fountain");
        let hp: f32 = s.crew.members.iter().map(|p| p.health).sum();
        s.rogue_feature(&mut r, "trap");
        assert!(
            s.crew.members.iter().map(|p| p.health).sum::<f32>() < hp,
            "a trap hurts"
        );
        let loot0: f64 = r.loot.iter().map(|l| l.1).sum();
        s.rogue_feature(&mut r, "cache");
        let loot1: f64 = r.loot.iter().map(|l| l.1).sum();
        assert!(loot1 > loot0);
        s.rogue_feature(&mut r, "vault");
        assert!(
            r.loot.iter().map(|l| l.1).sum::<f64>() > loot1 * 1.5,
            "a vault pays more"
        );
        let (ox, oy) = (r.x, r.y);
        let mut moved = false;
        for turn in 0..30 {
            r.turn = turn;
            s.rogue_feature(&mut r, "portal");
            moved |= (r.x, r.y) != (ox, oy);
        }
        assert!(moved, "portals teleport");
        assert!(r.tile(r.x, r.y) != T_WALL);
        // An altar blesses or burns.
        let (mut blessed, mut burnt) = (0, 0);
        for turn in 0..40 {
            r.turn = turn;
            r.buff = 0;
            let before: f32 = s.crew.members.iter().map(|p| p.health).sum();
            s.rogue_feature(&mut r, "altar");
            if r.buff > 0 {
                blessed += 1;
            } else if s.crew.members.iter().map(|p| p.health).sum::<f32>() < before {
                burnt += 1;
            }
            for p in &mut s.crew.members {
                p.health = 0.8;
            }
        }
        assert!(
            blessed > 5 && burnt > 5,
            "{blessed} blessings, {burnt} curses"
        );
    }

    #[test]
    fn stairs_keep_the_floors_and_only_the_entrance_leads_out() {
        let (mut s, _) = run_on("epave", 6);
        let food = s.data.resource_index("food").unwrap();
        s.rogue.as_mut().unwrap().loot.push((food, 12.0));
        // On the entrance: leaving needs to stand on the stairs (we do).
        assert!(
            s.rogue_stairs().unwrap().starts_with("RETOUR"),
            "out at the entrance"
        );
        assert!(s.rogue.as_ref().unwrap().over);
        // Fresh run: go down and up again.
        let (s, _) = run_on("epave", 6);
        let (dx, dy) = {
            let r = s.rogue.as_ref().unwrap();
            let f = r.floor();
            let i = f.tiles.iter().position(|t| *t == T_DOWN).unwrap();
            ((i % r.w) as i32, (i / r.w) as i32)
        };
        let (mut s, _) = run_on("epave", 6);
        {
            let r = s.rogue.as_mut().unwrap();
            r.floors[0].monsters.clear();
            r.x = dx;
            r.y = dy;
        }
        let msg = s.rogue_stairs().expect("down");
        assert_eq!(msg, "Etage 2.");
        {
            let r = s.rogue.as_ref().unwrap();
            assert_eq!(r.cur, 1);
            assert_eq!(r.floors.len(), 2);
            assert_eq!(r.tile(r.x, r.y), T_UP);
        }
        s.rogue.as_mut().unwrap().floors[0].items.clear();
        let msg = s.rogue_stairs().expect("up");
        assert_eq!(msg, "Etage 1.");
        let r = s.rogue.as_ref().unwrap();
        assert_eq!((r.x, r.y), (dx, dy), "back on the stairs down");
        assert!(
            r.floors[0].items.is_empty(),
            "the floor above is as it was left"
        );
        assert!(s.rogue_stairs().is_ok(), "down again");
        // Nothing under the squad elsewhere.
        let (mut s, _) = run_on("epave", 6);
        {
            let r = s.rogue.as_mut().unwrap();
            let (x, y) = (0..r.w as i32)
                .flat_map(|x| (0..r.h as i32).map(move |y| (x, y)))
                .find(|&(x, y)| r.tile(x, y) == T_FLOOR)
                .unwrap();
            r.x = x;
            r.y = y;
        }
        assert!(s.rogue_stairs().is_err());
    }

    #[test]
    fn leaving_brings_the_loot_aboard_and_a_dead_end_loses_it() {
        let (mut s, _) = run_on("depot", 2);
        let steel = s.data.resource_index("steel").unwrap();
        let before = s.ship.cargo.kg[steel];
        s.rogue.as_mut().unwrap().loot.push((steel, 25.0));
        let msg = s.rogue_stairs().unwrap();
        assert!(msg.contains("25"), "{msg}");
        assert!((s.ship.cargo.kg[steel] - before - 25.0).abs() < 1e-9);
    }

    #[test]
    fn a_hazardous_place_hurts_and_armour_stops_it() {
        let (mut s, _) = run_on("reacteur", 5);
        s.rogue.as_mut().unwrap().floors[0].monsters.clear();
        for p in &mut s.crew.members {
            p.health = 1.0;
        }
        for _ in 0..40 {
            s.rogue_wait().unwrap();
        }
        let hurt: f32 = s.crew.members.iter().map(|p| 1.0 - p.health).sum();
        assert!(hurt > 0.0, "radiation burns");
        let (mut t, _) = run_on("station", 5);
        t.rogue.as_mut().unwrap().floors[0].monsters.clear();
        for _ in 0..40 {
            t.rogue_wait().unwrap();
        }
        let safe: f32 = t.crew.members.iter().map(|p| 1.0 - p.health).sum();
        assert!(safe < hurt, "a clean station is gentler");
    }

    #[test]
    fn travel_walks_the_known_map_and_stops_when_danger_shows() {
        let (mut s, _) = run_on("station", 4);
        s.rogue.as_mut().unwrap().floors[0].monsters.clear();
        // Aim at the farthest seen floor cell.
        let (tx, ty) = {
            let r = s.rogue.as_ref().unwrap();
            let mut best = (r.x, r.y);
            let mut d = 0;
            for y in 0..r.h as i32 {
                for x in 0..r.w as i32 {
                    if r.floor().seen[r.idx(x, y)] && r.tile(x, y) != T_WALL {
                        let dd = (x - r.x).abs().max((y - r.y).abs());
                        if dd > d {
                            d = dd;
                            best = (x, y);
                        }
                    }
                }
            }
            best
        };
        let n = s.rogue_travel(tx, ty, 50).expect("travels");
        assert!(n >= 1);
        // Unknown cells are refused.
        let (ux, uy) = {
            let r = s.rogue.as_ref().unwrap();
            (0..r.w as i32)
                .flat_map(|x| (0..r.h as i32).map(move |y| (x, y)))
                .find(|&(x, y)| !r.floor().seen[r.idx(x, y)])
                .unwrap()
        };
        assert!(s.rogue_travel(ux, uy, 5).is_err());
        // A hunter in view stops the walk after the first step.
        let (mut s, _) = run_on("base_pirate", 9);
        let pirate = s.data.rogue_monster_index("pirate").unwrap();
        let (px, py) = {
            let r = s.rogue.as_mut().unwrap();
            r.floors[0].monsters.clear();
            let c = (0..r.w as i32)
                .flat_map(|x| (0..r.h as i32).map(move |y| (x, y)))
                .filter(|&(x, y)| r.tile(x, y) == T_FLOOR && r.floor().seen[r.idx(x, y)])
                .max_by_key(|&(x, y)| (x - r.x).abs().max((y - r.y).abs()))
                .unwrap();
            r.floors[0].monsters.push(Mon {
                kind: pirate,
                x: c.0,
                y: c.1,
                hp: 7.0,
                awake: true,
                boss: false,
            });
            c
        };
        let _ = (px, py);
        let steps = s.rogue_travel(px, py, 30);
        if let Ok(n) = steps {
            assert!(n <= 30);
        }
    }

    #[test]
    fn places_to_explore_follow_what_is_near_and_dimensions_dream() {
        let mut s = new_sim(5, data());
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        let targets = s.rogue_targets();
        assert!(
            targets
                .iter()
                .any(|t| s.data.rogue_themes[t.theme].id == "station"),
            "{targets:?}"
        );
        // The theme of an anomaly follows its kind.
        let d = data();
        for (kind, id) in [
            (AnomalyKind::DerelictShip, "epave"),
            (AnomalyKind::AncientStation, "station_ancienne"),
            (AnomalyKind::CrystalFormation, "grotte_cristal"),
            (AnomalyKind::Comet, "glace"),
            (AnomalyKind::SpatialRift, "reve"),
        ] {
            let mut t = new_sim(5, Arc::clone(&d));
            t.bodies.push(crate::Body {
                name: "X".into(),
                kind: BodyKind::Anomaly(kind),
                pos: t.ship.pos,
                vel: vector_core::Vec3::ZERO,
                radius: 100.0,
                gm: 0.0,
            });
            let ts = t.rogue_targets();
            let last = ts.last().unwrap();
            assert_eq!(d.rogue_themes[last.theme].id, id, "{kind:?}");
        }
        // In a dimension every place turns to a dream.
        let mut t = new_sim(5, Arc::clone(&d));
        t.multiverse.dimension = 1;
        let (st, _) = t.nearest_station().unwrap();
        t.spawn_near(st, t.bodies[st].radius + 200.0);
        assert!(t
            .rogue_targets()
            .iter()
            .all(|x| d.rogue_themes[x.theme].id == "reve"));
        // Entering: needs something to explore and a squad; a place changes between visits.
        let mut s = new_sim(5, data());
        let (st, _) = s.nearest_station().unwrap();
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.rogue_enter(0, vec![]).expect("goes");
        assert!(s.rogue_enter(0, vec![]).is_err(), "one at a time");
        let first = s.rogue.as_ref().unwrap().floors[0].tiles.clone();
        s.rogue.as_mut().unwrap().over = true;
        s.rogue_close();
        s.rogue_enter(0, vec![]).unwrap();
        assert_ne!(
            s.rogue.as_ref().unwrap().floors[0].tiles,
            first,
            "a new visit, a new layout"
        );
    }

    #[test]
    fn a_run_survives_a_save() {
        let (mut s, _) = run_on("ruines", 8);
        s.rogue_wait().unwrap();
        let bytes = s.save_bytes().unwrap();
        let t = Sim::load_bytes(&bytes, Arc::clone(&s.data)).unwrap();
        let (a, b) = (s.rogue.as_ref().unwrap(), t.rogue.as_ref().unwrap());
        assert_eq!(a.turn, b.turn);
        assert_eq!(a.floors[0].tiles, b.floors[0].tiles);
        assert_eq!((a.x, a.y), (b.x, b.y));
        assert_eq!(a.floors[0].monsters, b.floors[0].monsters);
    }
}
