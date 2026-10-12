//! Exploration on foot (E5 + E9, one system): every place of the game, a planet's surface as much
//! as a station, a wreck or a cave, is explored turn by turn on floors drawn as characters. The
//! ship's AI sends a squad of the crew (and a rover on open ground); the squad moves, fights and
//! carries together. A *surface* floor is the ground of a planet cell (rocks, ponds, plants, the
//! animals and the sites of its biome, under its air and climate); a *dungeon* floor is a place
//! drawn from `data/rogue_themes.ron`: rooms and corridors, creatures, loot and features. A site of
//! a surface leads down into a dungeon, and stairs lead back up: the run is a stack of places.
//! Floors are a pure function of (place seed, visit number, depth). Whatever is carried out is
//! added to the hold; whoever falls stays down.
use crate::{ShipInput, Sim};
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng};
use vector_data::{GameData, RogueTheme, ROGUE_FEATURES, ROGUE_TERMINALS};
use vector_world::surface::{Surface, H as SURF_H, W as SURF_W};
use vector_world::{AnomalyKind, BodyKind};

pub const T_WALL: u8 = 0;
pub const T_FLOOR: u8 = 1;
/// Stairs up (on a surface: where the ship set the squad down).
pub const T_UP: u8 = 2;
pub const T_DOWN: u8 = 3;
/// Water: cannot be crossed, does not hide anything.
pub const T_WATER: u8 = 4;
/// A site of a surface (a ruin, a wreck, a cave...): searched once, and a way down.
pub const T_SITE: u8 = 5;
/// First feature tile; feature `k` of `ROGUE_FEATURES` is `T_FEATURE + k`.
pub const T_FEATURE: u8 = 6;
/// First terminal tile; terminal `k` of `ROGUE_TERMINALS` is `T_TERMINAL + k`.
pub const T_TERMINAL: u8 = T_FEATURE + 16;
/// Metres of ground one tile of a surface floor stands for (the same in the 3D view).
pub const TILE_M: f64 = 3.0;
/// Water thicker than this under its freezing point carries a squad: ice, not a thin skin.
pub const ICE_WALKS_BELOW: f64 = 262.0;

/// How far the squad sees, and seconds one turn takes in a dungeon and on a surface.
pub const VIEW_RADIUS: i32 = 9;
pub const TURN_SECONDS: f64 = 5.0;
pub const SURFACE_TURN_SECONDS: f64 = 60.0;
pub const MAX_PARTY: usize = 4;
/// Firearms: kilograms of ammunition per round, the cap of the recoil, and how fast it dies.
pub const ROUND_KG: f64 = 0.02;
pub const MAX_RECOIL: f64 = 0.5;
pub const RECOIL_DECAY: f64 = 0.6;
/// Grenades: the stock resource, the explosive reaction of its charge (see `chemistry.rs`), the
/// mass of that charge, damage points per MJ released, blast radius and throwing range (cells),
/// and the noise (cells) that wakes the sleepers.
pub const GRENADE_RESOURCE: &str = "grenades";
pub const GRENADE_REACTION: &str = "ammonium_nitrate_blast";
pub const GRENADE_CHARGE_KG: f64 = 0.4;
pub const BLAST_DMG_PER_MJ: f64 = 14.0;
pub const BLAST_RADIUS: i32 = 2;
pub const GRENADE_RANGE: i32 = 7;
pub const GRENADE_NOISE: f64 = 14.0;

/// Chance that a round hits a target `dist` cells away with `recoil` shaking the aim.
pub fn fire_accuracy(dist: f64, recoil: f64) -> f64 {
    (0.95 - 0.06 * dist - recoil).clamp(0.1, 0.95)
}
/// Distance within which a body can be entered (m): stations, anomalies, wrecks.
pub const ENTER_RANGE: f64 = 150_000.0;
/// What one person and the rover can carry, kg; tiles per move with the rover.
pub const LOOT_PER_MEMBER: f64 = 60.0;
pub const LOOT_VEHICLE: f64 = 250.0;
pub const VEHICLE_SPEED: i32 = 2;
/// How far from a planet the ship may be to land a squad, in body radii.
pub const MAX_RANGE_RADII: f64 = 30.0;
/// The drone costs one unit of robotics, flies this many turns and reaches this far (cells).
pub const DRONE_COST: f64 = 1.0;
pub const DRONE_TURNS: u32 = 30;
pub const DRONE_REACH: i32 = 3;
/// Monsters that never move, and the one that plays dead until you touch it.
const STATIONARY: [&str; 2] = ["tourelle", "reine_ruche"];
const SLEEPER: &str = "mimique";

/// A creature or machine on a floor, with everything it needs to act (so that the animals of a
/// planet and the guardians of a ruin are the same thing to the engine).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mon {
    pub name: String,
    pub glyph: char,
    pub x: i32,
    pub y: i32,
    pub hp: f64,
    pub attack: f64,
    pub defense: f64,
    pub sight: i32,
    /// Attacks on sight; otherwise it wanders and flees.
    pub hunts: bool,
    pub stationary: bool,
    /// Plays dead until touched.
    pub sleeper: bool,
    pub awake: bool,
    pub boss: bool,
    /// (resource, min kg, max kg) dropped when it dies.
    pub drops: Vec<(usize, f64, f64)>,
    /// A person of a social place: bumping into one talks to it. `crew` is a member of the
    /// ship's crew walking about the ship, `lines` what a passer-by says.
    pub friendly: bool,
    pub crew: Option<u32>,
    pub lines: Vec<String>,
    /// The species of an animal of a planet (tame ones can be caught for the pens).
    pub species: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub x: i32,
    pub y: i32,
    pub res: usize,
    pub kg: f64,
    /// Gathered (plants): the tools of the squad find more.
    pub plant: bool,
}

/// A site of interest on a surface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SiteInst {
    pub def: usize,
    pub x: i32,
    pub y: i32,
    pub used: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Floor {
    pub tiles: Vec<u8>,
    pub seen: Vec<bool>,
    pub monsters: Vec<Mon>,
    pub items: Vec<Item>,
    pub sites: Vec<SiteInst>,
    pub start: (i32, i32),
    /// What each cell is made of (index in `data.materials`).
    #[serde(default)]
    pub mats: Vec<u8>,
    /// Cells the squad built (its property): the same material as any other, plus this mark.
    #[serde(default)]
    pub owned: Vec<bool>,
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
    /// Turns left of a blessing (damage x1.5) and of the drone sent ahead.
    pub buff: u32,
    pub drone_turns: u32,
    pub boss_dead: bool,
    /// A planet surface: the body, the name of the place, the rover, air (bar) and temperature (K).
    pub body: Option<usize>,
    /// The cell of the map of the world where the squad landed.
    pub land: Option<(usize, usize)>,
    pub place: String,
    pub vehicle: bool,
    pub env: Option<(f64, f64)>,
    /// Seconds one turn takes.
    pub turn_seconds: f64,
    /// The place this one was entered from (a surface above a dungeon).
    pub parent: Option<Box<Rogue>>,
    /// A social place (station, colony, the ship): terminals and people instead of monsters.
    pub social: bool,
    /// The terminal the squad just used (index in `ROGUE_TERMINALS`), for the screen to open.
    pub terminal: Option<usize>,
    /// Rounds left in the magazine of each member who has fired: (crew index, rounds).
    #[serde(default)]
    pub mags: Vec<(usize, u32)>,
    /// How much the last shots shake the aim of the next ones (0..`MAX_RECOIL`).
    #[serde(default)]
    pub recoil: f64,
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

    /// The material of the cell (index in `data.materials`), when the floor has them.
    pub fn mat(&self, x: i32, y: i32) -> Option<usize> {
        if !self.inside(x, y) {
            return None;
        }
        self.floor()
            .mats
            .get(self.idx(x, y))
            .map(|&m| usize::from(m))
    }

    /// Whether the squad built this cell (its property, in the map and in first person alike).
    pub fn owned(&self, x: i32, y: i32) -> bool {
        self.inside(x, y)
            && self
                .floor()
                .owned
                .get(self.idx(x, y))
                .copied()
                .unwrap_or(false)
    }

    /// Cells of this floor the squad built.
    pub fn built_count(&self) -> usize {
        self.floor().owned.iter().filter(|b| **b).count()
    }

    pub fn tile(&self, x: i32, y: i32) -> u8 {
        if self.inside(x, y) {
            self.floor().tiles[self.idx(x, y)]
        } else {
            T_WALL
        }
    }

    /// Whether the squad could stand on this tile (ignoring who is there).
    pub fn walkable(&self, x: i32, y: i32) -> bool {
        !matches!(self.tile(x, y), T_WALL | T_WATER)
    }

    fn say(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 60 {
            self.log.remove(0);
        }
    }

    /// A surface floor of a landing (the root of a run, on a planet).
    pub fn on_surface(&self) -> bool {
        self.body.is_some() && self.cur == 0
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

    /// What this place carries so far, kg (the places above hold their own).
    pub fn loot_mass(&self) -> f64 {
        self.loot.iter().map(|l| l.1).sum::<f64>() + 0.0
    }

    /// Kg of loot the squad can carry here: a pack each, and the rover on open ground.
    pub fn carry_cap(&self, members_up: usize) -> f64 {
        members_up as f64 * LOOT_PER_MEMBER
            + if self.vehicle && self.on_surface() {
                LOOT_VEHICLE
            } else {
                0.0
            }
    }

    /// Everything carried in the places above this one too.
    fn loot_above(&self) -> f64 {
        self.parent
            .as_ref()
            .map_or(0.0, |p| p.loot_mass() + p.loot_above())
    }
}

/// What kind of entry a target is.
#[derive(Clone, Debug, PartialEq)]
pub enum TargetKind {
    /// A dungeon of a theme.
    Place,
    /// A landing on the surface of this body.
    Landing(usize),
    /// The ship itself, walked from room to room.
    Ship,
}

/// Where a place can be entered from: a label, the theme and the seed of the place.
#[derive(Clone, Debug, PartialEq)]
pub struct RogueTarget {
    pub label: String,
    pub theme: usize,
    pub seed: u64,
    pub kind: TargetKind,
}

/// A monster of the table, placed.
fn mon_from_def(
    data: &GameData,
    k: usize,
    x: i32,
    y: i32,
    scale: f64,
    awake: bool,
    boss: bool,
) -> Mon {
    let d = &data.rogue_monsters[k];
    let drops = data
        .resource_index(&d.loot.0)
        .map(|r| (r, d.loot.1, d.loot.2))
        .into_iter()
        .collect();
    Mon {
        name: d.name.clone(),
        glyph: d.glyph.chars().next().unwrap_or('?'),
        x,
        y,
        hp: d.hp * scale,
        attack: d.attack,
        defense: d.defense,
        sight: d.sight,
        hunts: true,
        stationary: STATIONARY.contains(&d.id.as_str()),
        sleeper: d.id == SLEEPER,
        awake,
        boss,
        drops,
        friendly: false,
        crew: None,
        lines: Vec::new(),
        species: None,
    }
}

/// An animal of a planet, placed.
fn mon_from_species(data: &GameData, sp: usize, x: i32, y: i32) -> Mon {
    let s = &data.species[sp];
    let hunts = s.danger > 0.0 && !s.domestic;
    let letter = s
        .name
        .chars()
        .find(char::is_ascii_alphabetic)
        .unwrap_or('a');
    Mon {
        name: s.name.clone(),
        glyph: if hunts {
            letter.to_ascii_uppercase()
        } else {
            letter.to_ascii_lowercase()
        },
        x,
        y,
        hp: 1.0 + s.body.cbrt() * 1.5,
        attack: 0.275 * s.danger,
        defense: 0.0,
        sight: 8,
        hunts,
        stationary: false,
        sleeper: false,
        awake: !hunts,
        boss: false,
        drops: s
            .yields
            .iter()
            .filter_map(|(r, kg)| data.resource_index(r).map(|i| (i, *kg, *kg)))
            .collect(),
        friendly: false,
        crew: None,
        lines: Vec::new(),
        species: Some(sp),
    }
}

/// Where and when an animal is met: what the biology of its species makes of the place.
struct Ambient {
    temp: f64,
    night: bool,
    world: u64,
    radiation: f64,
    biome: usize,
    now: f64,
}

/// An animal of a planet as its population has become (size of the local variant) and as it
/// lives (asleep in the cold season if it hibernates, asleep by day if it is nocturnal).
fn mon_adapted(data: &GameData, sp: usize, x: i32, y: i32, amb: &Ambient) -> Mon {
    use crate::biosphere::{behavior_of, variant_now};
    let mut m = mon_from_species(data, sp, x, y);
    let v = variant_now(data, amb.world, amb.radiation, sp, amb.biome, amb.now);
    let f = v.mass_factor();
    m.hp *= f.cbrt();
    m.attack *= f.cbrt();
    for d in &mut m.drops {
        d.1 *= f;
        d.2 *= f;
    }
    if v.speciated() {
        m.name = v.name(data);
    }
    if behavior_of(data, &data.species[sp]).asleep(amb.temp, amb.night) {
        m.sleeper = true;
        m.awake = false;
    }
    m
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

/// What the physics of a world does to what the ground holds (see `Sim::dig_mods`): metals follow
/// the density, volcanic minerals the activity of the inside, volatiles the cold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DigMods {
    pub metal: f64,
    pub volcanic: f64,
    pub cold: f64,
    pub depth: f64,
}

impl DigMods {
    pub const NEUTRAL: DigMods = DigMods {
        metal: 1.0,
        volcanic: 1.0,
        cold: 1.0,
        depth: 1.0,
    };

    /// Multiplier of the yield of a resource (its category and id) here.
    pub fn factor(&self, category: &str, id: &str) -> f64 {
        let by_kind = match category {
            "metal" | "precious" | "nuclear" => self.metal,
            "volatile" => self.cold,
            _ => 1.0,
        };
        let by_id = match id {
            "sulfur" | "phosphorus" | "rare_earths" | "diamond" => self.volcanic,
            "water" => self.cold,
            _ => 1.0,
        };
        by_kind * by_id * self.depth
    }
}

/// Animals keep off slippery and burning ground.
fn animal_avoids(data: &GameData, m: usize) -> bool {
    let m = &data.materials[m];
    m.slick >= 0.3 || m.harm > 0.0
}

/// The theme a site of a surface leads down into.
pub fn site_theme(site_id: &str) -> &'static str {
    match site_id {
        "epave" => "epave",
        "ruine" => "ruines",
        "grotte" => "grotte",
        "campement" => "depot",
        "source" => "grotte_cristal",
        _ => "laboratoire",
    }
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

type Room = (i32, i32, i32, i32);

fn room_center(r: &Room) -> (i32, i32) {
    (r.0 + r.2 / 2, r.1 + r.3 / 2)
}

/// Carve up to `n` rooms (smaller ones when `small`) and link them in a chain with corridors:
/// every floor cell is reachable from the first room. The common ground of dungeons and of the
/// social places.
fn carve_rooms(w: usize, h: usize, n: usize, rng: &mut Rng, small: bool) -> (Vec<u8>, Vec<Room>) {
    let mut tiles = vec![T_WALL; w * h];
    let mut rooms: Vec<Room> = Vec::new();
    for _ in 0..1500 {
        if rooms.len() >= n {
            break;
        }
        let (rw, rh) = if small {
            (4 + (rng.f64() * 4.0) as i32, 3 + (rng.f64() * 3.0) as i32)
        } else {
            (5 + (rng.f64() * 8.0) as i32, 4 + (rng.f64() * 5.0) as i32)
        };
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
    for k in 1..rooms.len() {
        let (ax, ay) = room_center(&rooms[k - 1]);
        let (bx, by) = room_center(&rooms[k]);
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
    (tiles, rooms)
}

/// A floor of a social place: an entrance, a terminal in the middle of each of the other rooms
/// (in a shuffled order), and people about. `funcs` are indices in `ROGUE_TERMINALS`; `crew` the
/// members of the ship's crew to put in the rooms (the ship only).
pub fn generate_social(
    data: &GameData,
    theme: usize,
    seed: u64,
    funcs: &[usize],
    crew: &[(u32, String)],
) -> Floor {
    let t = &data.rogue_themes[theme];
    let (w, h) = t.size;
    let mut rng = Rng::new(hash_coord(seed, 0, 11, 23));
    let n_rooms = (funcs.len() + 1).max(4);
    let (mut tiles, rooms) = carve_rooms(w, h, n_rooms, &mut rng, true);
    let start = room_center(&rooms[0]);
    tiles[start.1 as usize * w + start.0 as usize] = T_UP;
    // Shuffle the functions over the other rooms.
    let mut order: Vec<usize> = funcs.to_vec();
    for k in (1..order.len()).rev() {
        order.swap(k, (rng.f64() * (k + 1) as f64) as usize % (k + 1));
    }
    for (room, f) in rooms.iter().skip(1).zip(&order) {
        let c = room_center(room);
        tiles[c.1 as usize * w + c.0 as usize] = T_TERMINAL + *f as u8;
    }
    let mut monsters: Vec<Mon> = Vec::new();
    let free = |tiles: &Vec<u8>, monsters: &Vec<Mon>, c: (i32, i32)| {
        tiles[c.1 as usize * w + c.0 as usize] == T_FLOOR
            && !monsters.iter().any(|m| (m.x, m.y) == c)
    };
    let person =
        |name: String, glyph: char, x: i32, y: i32, crew: Option<u32>, lines: Vec<String>| Mon {
            name,
            glyph,
            x,
            y,
            hp: 5.0,
            attack: 0.0,
            defense: 0.0,
            sight: 6,
            hunts: false,
            stationary: false,
            sleeper: false,
            awake: true,
            boss: false,
            drops: Vec::new(),
            friendly: true,
            crew,
            lines,
            species: None,
        };
    let place = |monsters: &mut Vec<Mon>, rng: &mut Rng, who: Option<(u32, &String)>| {
        for _ in 0..40 {
            let room = &rooms[(rng.f64() * rooms.len() as f64) as usize % rooms.len()];
            let c = (
                room.0 + (rng.f64() * f64::from(room.2)) as i32,
                room.1 + (rng.f64() * f64::from(room.3)) as i32,
            );
            if (c != start) && free(&tiles, monsters, c) {
                let m = match who {
                    Some((id, name)) => person(
                        name.clone(),
                        name.chars()
                            .find(|c| c.is_ascii_alphabetic())
                            .unwrap_or('c')
                            .to_ascii_uppercase(),
                        c.0,
                        c.1,
                        Some(id),
                        Vec::new(),
                    ),
                    None => person("Passant".to_string(), '@', c.0, c.1, None, t.lines.clone()),
                };
                monsters.push(m);
                return;
            }
        }
    };
    if crew.is_empty() {
        for _ in 0..(rooms.len() * 2).min(16) {
            place(&mut monsters, &mut rng, None);
        }
    } else {
        for (id, name) in crew {
            place(&mut monsters, &mut rng, Some((*id, name)));
        }
    }
    let mats = paint(data, &tiles, w, t, seed, None);
    Floor {
        seen: vec![false; w * h],
        tiles,
        monsters,
        items: Vec::new(),
        sites: Vec::new(),
        start,
        mats,
        owned: Vec::new(),
    }
}

/// Tiles of a room of the player's ship: 5 by 3 of floor, a wall of one tile around.
pub const SHIP_ROOM_W: usize = 6;
pub const SHIP_ROOM_H: usize = 4;

/// The walk-through floor of a ship built from a plan: one room per walkable part of its main
/// deck, doors between neighbouring rooms, the entrance on the bridge, the terminals in the rooms
/// that match them (engineering terminal in engineering, hold in the hold...) and the crew in the
/// cabins. Returns the floor and its size in tiles.
pub fn generate_ship_interior(
    data: &GameData,
    theme: usize,
    seed: u64,
    plan: &crate::shipwright::Blueprint,
    funcs: &[usize],
    crew: &[(u32, String)],
) -> (Floor, (usize, usize)) {
    let rooms = plan.interior(data);
    let (minx, maxx) = rooms.iter().fold((i32::MAX, i32::MIN), |a, r| {
        (a.0.min(i32::from(r.0)), a.1.max(i32::from(r.0)))
    });
    let (miny, maxy) = rooms.iter().fold((i32::MAX, i32::MIN), |a, r| {
        (a.0.min(i32::from(r.1)), a.1.max(i32::from(r.1)))
    });
    let (nx, ny) = if rooms.is_empty() {
        (1, 1)
    } else {
        ((maxx - minx + 1) as usize, (maxy - miny + 1) as usize)
    };
    let (w, h) = (nx * SHIP_ROOM_W + 1, ny * SHIP_ROOM_H + 1);
    let mut tiles = vec![T_WALL; w * h];
    let origin = |x: i8, y: i8| {
        (
            (i32::from(x) - minx) as usize * SHIP_ROOM_W + 1,
            (i32::from(y) - miny) as usize * SHIP_ROOM_H + 1,
        )
    };
    for (x, y, _) in &rooms {
        let (ox, oy) = origin(*x, *y);
        for dy in 0..SHIP_ROOM_H - 1 {
            for dx in 0..SHIP_ROOM_W - 1 {
                tiles[(oy + dy) * w + ox + dx] = T_FLOOR;
            }
        }
        // Doors towards the neighbours in front and to starboard (the other side is theirs).
        if rooms.iter().any(|r| r.0 == x + 1 && r.1 == *y) {
            tiles[(oy + 1) * w + ox + SHIP_ROOM_W - 1] = T_FLOOR;
        }
        if rooms.iter().any(|r| r.0 == *x && r.1 == y + 1) {
            tiles[(oy + SHIP_ROOM_H - 1) * w + ox + 2] = T_FLOOR;
        }
    }
    let mut rng = Rng::new(hash_coord(seed, 0, 29, 31));
    let find =
        |kind: &str| -> Vec<usize> { (0..rooms.len()).filter(|&i| rooms[i].2 == kind).collect() };
    let start = rooms
        .iter()
        .position(|r| r.2 == "passerelle")
        .or(if rooms.is_empty() { None } else { Some(0) })
        .map_or((1, 1), |i| {
            let (ox, oy) = origin(rooms[i].0, rooms[i].1);
            (ox + 2, oy + 1)
        });
    tiles[start.1 * w + start.0] = T_UP;
    // Terminals: each in a room of its kind when there is one, else in any other room; never on
    // the entrance tile nor on another terminal.
    let wanted = |id: &str| -> &'static str {
        match id {
            "soute" => "soute",
            "ingenierie" => "ingenierie",
            "carte" => "passerelle",
            "elevage" => "hangar",
            _ => "cabines",
        }
    };
    let mut used = std::collections::BTreeSet::new();
    used.insert(start);
    for &f in funcs {
        let preferred = find(wanted(ROGUE_TERMINALS[f].0));
        let mut order: Vec<usize> = preferred.clone();
        // Which of the preferred rooms comes first is the seed's choice.
        if order.len() > 1 {
            let k = (rng.f64() * order.len() as f64) as usize % order.len();
            order.rotate_left(k);
        }
        order.extend((0..rooms.len()).filter(|i| !preferred.contains(i)));
        'rooms: for i in order {
            let (ox, oy) = origin(rooms[i].0, rooms[i].1);
            for (dx, dy) in [(2, 1), (1, 2), (3, 2), (4, 1), (1, 1)] {
                let c = (ox + dx, oy + dy);
                if !used.contains(&c) {
                    used.insert(c);
                    tiles[c.1 * w + c.0] = T_TERMINAL + f as u8;
                    break 'rooms;
                }
            }
        }
    }
    // The crew: in the cabins by preference.
    let mut monsters: Vec<Mon> = Vec::new();
    let homes: Vec<usize> = {
        let mut v = find("cabines");
        let rest: Vec<usize> = (0..rooms.len()).filter(|i| !v.contains(i)).collect();
        v.extend(rest);
        v
    };
    for (k, (id, name)) in crew.iter().enumerate() {
        if homes.is_empty() {
            break;
        }
        let room = homes[k % homes.len()];
        let (ox, oy) = origin(rooms[room].0, rooms[room].1);
        for _ in 0..30 {
            let c = (
                ox + (rng.f64() * (SHIP_ROOM_W - 1) as f64) as usize,
                oy + (rng.f64() * (SHIP_ROOM_H - 1) as f64) as usize,
            );
            let free = tiles[c.1 * w + c.0] == T_FLOOR
                && !monsters
                    .iter()
                    .any(|m| (m.x, m.y) == (c.0 as i32, c.1 as i32));
            if free {
                monsters.push(Mon {
                    name: name.clone(),
                    glyph: name
                        .chars()
                        .find(|c| c.is_ascii_alphabetic())
                        .unwrap_or('c')
                        .to_ascii_uppercase(),
                    x: c.0 as i32,
                    y: c.1 as i32,
                    hp: 5.0,
                    attack: 0.0,
                    defense: 0.0,
                    sight: 6,
                    hunts: false,
                    stationary: false,
                    sleeper: false,
                    awake: true,
                    boss: false,
                    drops: Vec::new(),
                    friendly: true,
                    crew: Some(*id),
                    lines: Vec::new(),
                    species: None,
                });
                break;
            }
        }
    }
    let t = &data.rogue_themes[theme];
    let mats = paint(data, &tiles, w, t, seed, None);
    (
        Floor {
            seen: vec![false; w * h],
            tiles,
            monsters,
            items: Vec::new(),
            sites: Vec::new(),
            start: (start.0 as i32, start.1 as i32),
            mats,
            owned: Vec::new(),
        },
        (w, h),
    )
}

/// A hard freeze turns the water of a surface floor into ice one can walk on (a thin freeze only
/// makes the water "eau gelee", which still does not carry anyone).
pub fn freeze_over(data: &GameData, tiles: &mut [u8], mats: &mut [u8], temp: f64) {
    if temp >= ICE_WALKS_BELOW {
        return;
    }
    if let Some(ice) = data.material_index("banquise") {
        for (t, m) in tiles.iter_mut().zip(mats.iter_mut()) {
            if *t == T_WATER {
                *t = T_FLOOR;
                *m = ice as u8;
            }
        }
    }
}

/// What the cells of a floor are made of: the floor and wall materials of the place, or on a
/// surface the ground the biome is made of (patches of the materials that make it up, snow and
/// ice where it is cold) and the liquid its temperature allows.
fn paint(
    data: &GameData,
    tiles: &[u8],
    w: usize,
    theme: &RogueTheme,
    seed: u64,
    ground: Option<(&str, f64)>,
) -> Vec<u8> {
    let find = |id: &str| data.material_index(id).unwrap_or(0);
    let (floor, wall) = (find(&theme.floor_mat), find(&theme.wall_mat));
    let of_biome = |kind: &str, biome: &str| -> Vec<(usize, f64)> {
        data.materials
            .iter()
            .enumerate()
            .filter(|(_, m)| m.kind == kind)
            .filter_map(|(i, m)| {
                m.ground_of
                    .iter()
                    .find(|(b, _)| b == biome)
                    .map(|(_, wt)| (i, *wt))
            })
            .collect()
    };
    let pick = |list: &[(usize, f64)], n: f64, fallback: usize| -> usize {
        let total: f64 = list.iter().map(|(_, wt)| wt).sum();
        let mut x = n * total;
        for (i, wt) in list {
            if x < *wt {
                return *i;
            }
            x -= wt;
        }
        list.last().map_or(fallback, |(i, _)| *i)
    };
    let (floors, walls) = match ground {
        Some((b, _)) => (of_biome("floor", b), of_biome("wall", b)),
        None => (Vec::new(), Vec::new()),
    };
    let (snow, ice) = (find("neige"), find("glace"));
    let liquid = match ground {
        Some((_, t)) if t < 273.0 => find("eau_glacee"),
        Some((_, t)) if t > 1200.0 => find("lave"),
        _ => find("eau"),
    };
    tiles
        .iter()
        .enumerate()
        .map(|(i, &t)| {
            let (x, y) = ((i % w) as f64, (i / w) as f64);
            let m = if t == T_WATER {
                liquid
            } else if t == T_WALL {
                if walls.is_empty() {
                    wall
                } else {
                    pick(&walls, vnoise(seed ^ 0x3B17, x / 8.0, y / 8.0), wall)
                }
            } else if floors.is_empty() {
                floor
            } else {
                let cold = ground.is_some_and(|(_, tk)| tk < 268.0);
                let frost = vnoise(seed ^ 0x77A1, x / 7.0, y / 7.0);
                if cold && frost > 0.88 {
                    ice
                } else if cold && frost > 0.55 {
                    snow
                } else {
                    pick(&floors, vnoise(seed ^ 0x6D41, x / 7.0, y / 7.0), floor)
                }
            };
            m as u8
        })
        .collect()
}

/// Build floor `depth` of a place.
pub fn generate_floor(data: &GameData, theme: usize, seed: u64, depth: u32) -> Floor {
    let t = &data.rogue_themes[theme];
    let (w, h) = t.size;
    let last = depth + 1 >= t.floors;
    let mut rng = Rng::new(hash_coord(seed, i64::from(depth), 11, 22));
    let n_rooms = t.rooms.0 + (rng.f64() * (t.rooms.1 - t.rooms.0 + 1) as f64) as usize;
    let (mut tiles, rooms) = carve_rooms(w, h, n_rooms, &mut rng, false);
    let center = room_center;
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
        let awake = rng.f64() < 0.35;
        monsters.push(mon_from_def(data, k, c.0, c.1, scale, awake, false));
    }
    // Vault guardians.
    for c in vaults {
        let k = pick_weighted(&pool, &mut rng);
        let spot = [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .map(|d| (c.0 + d.0, c.1 + d.1))
            .find(|s| free(&tiles, *s) && !monsters.iter().any(|m| (m.x, m.y) == *s));
        if let Some(s) = spot {
            monsters.push(mon_from_def(data, k, s.0, s.1, scale * 1.5, false, false));
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
        monsters.push(mon_from_def(
            data,
            strongest,
            c.0,
            c.1,
            scale * 2.5,
            false,
            true,
        ));
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
                        plant: false,
                    });
                }
            }
        }
    }
    let mats = paint(
        data,
        &tiles,
        w,
        t,
        hash_coord(seed, i64::from(depth), 5, 91),
        None,
    );
    Floor {
        seen: vec![false; w * h],
        tiles,
        monsters,
        items,
        sites: Vec::new(),
        start,
        mats,
        owned: Vec::new(),
    }
}

/// Size of a surface floor (cells).
pub const MAP_W: usize = 96;
pub const MAP_H: usize = 52;

impl Sim {
    // ---- where to go ---------------------------------------------------------------------

    /// Places that can be entered from where the ship is.
    pub fn rogue_targets(&self) -> Vec<RogueTarget> {
        let d = &self.data;
        let theme = |id: &str| d.rogue_theme_index(id).unwrap_or(0);
        let dim = self.multiverse.dimension > 0;
        let mut out = Vec::new();
        let mut add = |label: String, id: &str, seed: u64, portal: bool, kind: TargetKind| {
            let social = matches!(id, "station" | "colonie" | "vaisseau");
            let id = if dim && !portal && !social && kind == TargetKind::Place {
                "reve"
            } else {
                id
            };
            out.push(RogueTarget {
                label,
                theme: theme(id),
                seed,
                kind,
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
                // A station is walked only from its dock: it is not yours to wander into.
                BodyKind::Station if self.docked == Some(i) => {
                    add(
                        format!("Station : {name}"),
                        "station",
                        seed,
                        false,
                        TargetKind::Place,
                    );
                }
                BodyKind::Anomaly(AnomalyKind::Portal)
                    if dist <= crate::multiverse::PORTAL_RANGE =>
                {
                    add(
                        format!("Coeur du portail : {name}"),
                        "portail",
                        seed,
                        true,
                        TargetKind::Place,
                    );
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
                    add(
                        format!("{} : {name}", k.label()),
                        id,
                        seed,
                        false,
                        TargetKind::Place,
                    );
                }
                BodyKind::Asteroid(_) if dist <= b.radius * 3.0 + 30_000.0 => {
                    add(
                        format!("Galeries : {name}"),
                        "grotte",
                        seed,
                        false,
                        TargetKind::Place,
                    );
                }
                BodyKind::Planet(k) | BodyKind::Moon(k)
                    if !k.is_giant() && dist <= b.radius * MAX_RANGE_RADII + 1.0e6 =>
                {
                    add(
                        format!("Atterrir : {name}"),
                        "surface",
                        seed,
                        true,
                        TargetKind::Landing(i),
                    );
                }
                _ => {}
            }
        }
        if let Some(ci) = self.docked_colony() {
            let c = &self.colonies[ci];
            add(
                format!("Colonie : {}", c.name),
                "colonie",
                hash_coord(self.system.seed, c.id as i64, 4, 4),
                false,
                TargetKind::Place,
            );
        }
        add(
            "Parcourir le vaisseau".to_string(),
            "vaisseau",
            hash_coord(self.system.seed, 0, 99, 9),
            true,
            TargetKind::Ship,
        );
        for w in &self.wrecks {
            if (self.ship.pos - w.pos).length() <= 30_000.0 {
                let seed = hash_coord(self.system.seed, i64::from(w.id), 77, 5);
                add(
                    format!("Epave : {}", w.name),
                    "epave",
                    seed,
                    false,
                    TargetKind::Place,
                );
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
                p.alive && p.health > 0.3 && p.age >= 16.0
            })
            .collect();
        v.sort_by(|a, b| {
            let s = |i: usize| combat.map_or(0.0, |c| self.crew.members[i].skills[c]);
            s(*b).total_cmp(&s(*a))
        });
        v.truncate(MAX_PARTY);
        v
    }

    /// Whether a squad can land on `body` now.
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

    /// A land cell of `body` near the middle of its map (None for worlds with no surface).
    pub fn default_landing(&self, body: usize) -> Option<(usize, usize)> {
        let map = self.planet_map(body)?;
        let (mx, my) = (SURF_W as i32 / 2, SURF_H as i32 / 2);
        let mut cells: Vec<(usize, usize)> = (0..SURF_H)
            .flat_map(|y| (0..SURF_W).map(move |x| (x, y)))
            .filter(|&(x, y)| !map.surface.is_sea(x, y))
            .collect();
        cells.sort_by_key(|&(x, y)| (x as i32 - mx).abs() + 2 * (y as i32 - my).abs());
        cells.first().copied()
    }

    fn next_visit_seed(&mut self, base: u64) -> u64 {
        let visits = self
            .rogue_visits
            .iter()
            .find(|(s, _)| *s == base)
            .map_or(0, |(_, n)| *n);
        match self.rogue_visits.iter_mut().find(|(s, _)| *s == base) {
            Some((_, n)) => *n += 1,
            None => self.rogue_visits.push((base, 1)),
        }
        hash_coord(base, i64::from(visits), 3, 3)
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
        let party = if !party.is_empty() {
            party
        } else if target.kind == TargetKind::Ship {
            // Aboard, the AI walks as one member of the crew; the others are about the ship.
            self.rogue_squad().into_iter().take(1).collect()
        } else {
            self.rogue_squad()
        };
        if let TargetKind::Landing(body) = target.kind {
            let (cx, cy) = self.default_landing(body).ok_or("pas de terre ferme")?;
            return self.rogue_land(body, cx, cy, party, false);
        }
        let party = self.able(party);
        if party.is_empty() {
            return Err("personne n'est en etat de partir");
        }
        let seed = self.next_visit_seed(target.seed);
        if self.data.rogue_themes[target.theme].social.is_empty() && target.kind != TargetKind::Ship
        {
            self.rogue_begin(target.theme, seed, party);
        } else {
            self.rogue_begin_social(target.theme, seed, party);
        }
        Ok(())
    }

    /// Board the crippled target on foot: the squad crosses the airlock and fights through the
    /// corridors of the enemy ship (a floor of the theme `abordage`, its walls and decks of the
    /// materials of that hull). Clearing the garrison and walking back out takes the ship.
    pub fn board_on_foot(&mut self) -> Result<(), &'static str> {
        if self.rogue.is_some() {
            return Err("une exploration est deja en cours");
        }
        let k = self.target_npc.ok_or("aucune cible")?;
        let n = self.npcs.get(k).ok_or("cible inconnue")?;
        if !n.disabled {
            return Err("la cible n'est pas neutralisee");
        }
        if (n.pos - self.ship.pos).length() > crate::combat::BOARD_RANGE
            || (n.vel - self.ship.vel).length() > 60.0
        {
            return Err("trop loin ou trop rapide pour un abordage");
        }
        let theme = self
            .data
            .rogue_theme_index("abordage")
            .ok_or("pas de theme d'abordage")?;
        let party = self.able(self.rogue_squad());
        if party.is_empty() {
            return Err("personne n'est en etat de partir");
        }
        let b = crate::arms::Boarding {
            npc_id: n.id,
            name: n.name.clone(),
            role: n.role,
            faction: n.faction,
            level: n.level,
            hull_max: n.hull_max,
            cargo: n.cargo,
        };
        let seed = hash_coord(self.system.seed, i64::from(b.npc_id), 0xAB02, 7);
        let mut r = self.new_run(theme, seed, party);
        // The walls and the decks of the ship are what its builders used.
        let (wall, deck) = match b.role {
            crate::npc::NpcRole::Patrol => ("coque_blindee", "pont_blinde"),
            crate::npc::NpcRole::Pirate => ("coque_rouillee", "caillebotis"),
            _ => ("cloison", "tole"),
        };
        if let (Some(w), Some(d)) = (
            self.data.material_index(wall),
            self.data.material_index(deck),
        ) {
            let f = &mut r.floors[0];
            for i in 0..f.tiles.len().min(f.mats.len()) {
                match f.tiles[i] {
                    T_WALL => f.mats[i] = w as u8,
                    T_FLOOR => f.mats[i] = d as u8,
                    _ => {}
                }
            }
        }
        r.place = format!("ABORDAGE : {}", b.name);
        r.reveal();
        self.military.boarding = Some(b);
        self.rogue = Some(r);
        Ok(())
    }

    /// The ship the squad leaves, once the boarding walk is over: taken if the garrison is
    /// dead, otherwise the squad falls back and the ship drifts on, crippled.
    fn boarding_outcome(&mut self, r: &Rogue) -> Option<String> {
        let b = self.military.boarding.take()?;
        let cleared = r.floors[0]
            .monsters
            .iter()
            .all(|m| m.friendly || m.hp <= 0.0);
        if !cleared {
            return Some(format!(
                "{} N'EST PAS PRIS : LA GARNISON TIENT ENCORE",
                b.name
            ));
        }
        let k = self.npcs.iter().position(|n| n.id == b.npc_id)?;
        if !b.faction.is_none() && b.role != crate::npc::NpcRole::Pirate {
            self.change_reputation(b.faction, -20.0);
            self.mark_wanted_pub(b.faction, 10.0);
        }
        self.remove_npc_silent(k);
        self.capture_prize(&b);
        Some(format!(
            "{} EST A VOUS : DECIDEZ DU SORT DES PRISONNIERS (F3)",
            b.name
        ))
    }

    /// The terminals the ship's modules give it: the bridge, the crew's quarters and the time
    /// anchors always, the rest from what is installed.
    pub fn ship_terminals(&self) -> Vec<usize> {
        let mut ids: Vec<&str> = vec!["pont", "equipage", "temps"];
        for m in self.ship.design.slots.iter().flatten() {
            let def = &self.data.modules[*m];
            let f = match def.category.as_str() {
                "cargo" => Some("soute"),
                "reactor" | "engine" | "battery" => Some("ingenierie"),
                "scanner" | "ftl" => Some("carte"),
                _ if def.food > 0.0 => Some("elevage"),
                _ if def.medical > 0.0 => Some("infirmerie"),
                _ if def.science > 0.0 => Some("science"),
                _ => None,
            };
            if let Some(f) = f {
                if !ids.contains(&f) {
                    ids.push(f);
                }
            }
        }
        ids.iter()
            .filter_map(|id| ROGUE_TERMINALS.iter().position(|t| t.0 == *id))
            .collect()
    }

    /// Start a walk through a social place (a station, a colony, the ship) from the ship.
    pub fn rogue_begin_social(&mut self, theme: usize, seed: u64, party: Vec<usize>) {
        let ship = self.data.rogue_themes[theme].id == "vaisseau";
        let funcs: Vec<usize> = if ship {
            self.ship_terminals()
        } else {
            self.data.rogue_themes[theme]
                .social
                .iter()
                .filter_map(|f| ROGUE_TERMINALS.iter().position(|t| t.0 == f))
                .collect()
        };
        let crew: Vec<(u32, String)> = if ship {
            self.crew
                .members
                .iter()
                .enumerate()
                .filter(|(i, p)| p.alive && !party.contains(i))
                .map(|(_, p)| (p.id, p.name.clone()))
                .take(14)
                .collect()
        } else {
            Vec::new()
        };
        let plan = if ship {
            self.plan_of_hull(self.ship.design.hull).cloned()
        } else {
            None
        };
        let (floor, size) = match &plan {
            Some(bp) if !bp.interior(&self.data).is_empty() => {
                let (f, s) = generate_ship_interior(&self.data, theme, seed, bp, &funcs, &crew);
                (f, Some(s))
            }
            _ => (
                generate_social(&self.data, theme, seed, &funcs, &crew),
                None,
            ),
        };
        let mut r = self.new_run(theme, seed, party);
        if let Some((w, h)) = size {
            r.w = w;
            r.h = h;
        }
        r.social = true;
        let (x, y) = floor.start;
        r.floors = vec![floor];
        r.x = x;
        r.y = y;
        r.reveal();
        self.rogue = Some(r);
    }

    /// The members of `party` still able to go.
    fn able(&self, party: Vec<usize>) -> Vec<usize> {
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
    }

    /// A dungeon run on `theme` with this seed and squad (the squad must not be empty).
    fn new_run(&self, theme: usize, seed: u64, party: Vec<usize>) -> Rogue {
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
            drone_turns: 0,
            boss_dead: false,
            body: None,
            land: None,
            place: t.name.clone(),
            vehicle: false,
            env: None,
            turn_seconds: TURN_SECONDS,
            parent: None,
            social: false,
            terminal: None,
            mags: Vec::new(),
            recoil: 0.0,
        };
        r.say(format!("{} : {}", t.name, t.text));
        r.reveal();
        r
    }

    /// Start a dungeon run from the ship.
    pub fn rogue_begin(&mut self, theme: usize, seed: u64, party: Vec<usize>) {
        let r = self.new_run(theme, seed, party);
        self.rogue = Some(r);
    }

    /// Land `party` (crew indices) on the cell (`cx`, `cy`) of `body`, with or without the rover.
    pub fn rogue_land(
        &mut self,
        body: usize,
        cx: usize,
        cy: usize,
        party: Vec<usize>,
        vehicle: bool,
    ) -> Result<(), &'static str> {
        if self.rogue.is_some() {
            return Err("une exploration est deja en cours");
        }
        self.expedition_possible(body)?;
        let map = self.planet_map(body).ok_or("pas de surface solide")?;
        if map.surface.is_sea(cx, cy) || map.surface.is_lake(cx, cy) {
            return Err("pas de terre ferme ici");
        }
        let party = self.able(party);
        if party.is_empty() {
            return Err("aucun equipier valide");
        }
        let biome = map.biome_at(cx, cy);
        let seed = hash_coord(map.seed, cx as i64, cy as i64, 77);
        let b = self.data.biomes[biome].clone();
        let hum = f64::from(map.surface.hum[Surface::idx(cx, cy)]);
        let (w, h) = (MAP_W as i32, MAP_H as i32);
        let at = |x: i32, y: i32| (y * w + x) as usize;
        let (lx, ly) = (w / 2, h / 2);
        // Ground: rocks, ponds in wet places, the river of the cell (the same line the 3D view
        // draws, from the map) and the shore where a sea or a lake lies next to the cell.
        let radius = self.bodies[body].radius;
        let cell_n = radius * std::f64::consts::PI / SURF_H as f64;
        let cell_e = (radius * Surface::lat(cy).cos().abs().max(0.05) * std::f64::consts::TAU
            / SURF_W as f64)
            .max(1.0);
        let wet_next = |dx: isize, dy: isize| {
            let (nx, ny) = (
                (cx as isize + dx).rem_euclid(SURF_W as isize) as usize,
                cy as isize + dy,
            );
            (0..SURF_H as isize).contains(&ny)
                && (map.surface.is_sea(nx, ny as usize) || map.surface.is_lake(nx, ny as usize))
        };
        let shores = [
            wet_next(1, 0),
            wet_next(-1, 0),
            wet_next(0, -1),
            wet_next(0, 1),
        ];
        let mut tiles = vec![T_FLOOR; MAP_W * MAP_H];
        for y in 0..h {
            for x in 0..w {
                let n = vnoise(seed, f64::from(x) / 6.0, f64::from(y) / 6.0);
                let wv = vnoise(seed ^ 0xA5A5, f64::from(x) / 9.0, f64::from(y) / 9.0);
                let (fx, fy) = (
                    cx as f64 + f64::from(x - lx) * TILE_M / cell_e,
                    cy as f64 + f64::from(y - ly) * TILE_M / cell_n,
                );
                let river = map
                    .surface
                    .hydro
                    .river_near(fx, fy)
                    .is_some_and(|(d, rel)| {
                        let meander =
                            (vnoise(seed ^ 0x71BE, f64::from(x) / 9.0, f64::from(y) / 9.0) - 0.5)
                                * 20.0;
                        d * cell_n + meander < vector_world::hydro::river_half_width_m(rel)
                    });
                // Distance of the tile from the edge of the grid facing each neighbour.
                let edge = [w - 1 - x, x, y, h - 1 - y];
                let shore = (0..4).any(|k| {
                    shores[k]
                        && f64::from(edge[k])
                            < 2.0
                                + 8.0
                                    * vnoise(
                                        seed ^ (0x5408 + k as u64),
                                        f64::from(x) / 5.0,
                                        f64::from(y) / 5.0,
                                    )
                });
                tiles[at(x, y)] = if n > 0.80 && !river && !shore {
                    T_WALL
                } else if river || shore || (hum > 0.45 && wv < 0.22) {
                    T_WATER
                } else {
                    T_FLOOR
                };
            }
        }
        for dy in -2..=2 {
            for dx in -2..=2 {
                tiles[at(lx + dx, ly + dy)] = T_FLOOR;
            }
        }
        tiles[at(lx, ly)] = T_UP;
        // What each cell is made of comes first: plants root where the matter lets them, animals
        // keep off ice and fire.
        let surface = self.data.rogue_theme_index("surface").unwrap_or(0);
        let temp_here = self.temp_now(&map, cx, cy);
        let mut mats = paint(
            &self.data,
            &tiles,
            MAP_W,
            &self.data.rogue_themes[surface],
            seed,
            Some((&b.id, temp_here)),
        );
        freeze_over(&self.data, &mut tiles, &mut mats, temp_here);
        let fertile = |x: i32, y: i32| self.data.materials[usize::from(mats[at(x, y)])].fertility;
        let avoided = |x: i32, y: i32| animal_avoids(&self.data, usize::from(mats[at(x, y)]));
        let mut monsters: Vec<Mon> = Vec::new();
        let mut items: Vec<Item> = Vec::new();
        let mut sites: Vec<SiteInst> = Vec::new();
        let busy = |tiles: &Vec<u8>, monsters: &Vec<Mon>, items: &Vec<Item>, x: i32, y: i32| {
            tiles[at(x, y)] != T_FLOOR
                || monsters.iter().any(|m| (m.x, m.y) == (x, y))
                || items.iter().any(|i| (i.x, i.y) == (x, y))
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
            if !busy(&tiles, &monsters, &items, x, y) && rng.f64() < fertile(x, y) {
                let sp = flora[(rng.next_u64() % flora.len() as u64) as usize];
                for (res, kg) in &self.data.species[sp].yields {
                    if let Some(r) = self.data.resource_index(res) {
                        items.push(Item {
                            x,
                            y,
                            res: r,
                            kg: *kg,
                            plant: true,
                        });
                    }
                }
            }
        }
        let n_fauna = if fauna.is_empty() {
            0
        } else {
            8 + (rng.next_u64() % 8) as usize
        };
        let amb = Ambient {
            temp: self.temp_now(&map, cx, cy),
            night: self.is_night(&map),
            world: map.seed,
            radiation: crate::biosphere::surface_radiation(map.pressure),
            biome,
            now: self.time,
        };
        // The trophic pyramid: grazers are met more often than the omnivores and the
        // carnivores that live off them.
        let weight = |sp: usize| match self.data.species[sp].trophic.as_str() {
            "herbivore" => 4.0,
            "omnivore" => 2.0,
            _ => 1.0,
        };
        let total_w: f64 = fauna.iter().map(|&s| weight(s)).sum();
        for _ in 0..n_fauna {
            let (x, y) = (
                (rng.next_u64() % MAP_W as u64) as i32,
                (rng.next_u64() % MAP_H as u64) as i32,
            );
            let mut pick = rng.f64() * total_w;
            let sp = fauna
                .iter()
                .copied()
                .find(|&s| {
                    pick -= weight(s);
                    pick < 0.0
                })
                .unwrap_or(fauna[0]);
            // Not on top of the landing party.
            if !busy(&tiles, &monsters, &items, x, y)
                && !avoided(x, y)
                && (x - lx).abs() + (y - ly).abs() > 8
            {
                let first = mon_adapted(&self.data, sp, x, y, &amb);
                // Herds, packs and swarms come in groups, side by side.
                let group = crate::biosphere::behavior_of(&self.data, &self.data.species[sp])
                    .group_on_grid();
                monsters.push(first.clone());
                let mut placed = 1;
                for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1), (1, 1), (-1, -1)] {
                    let (gx, gy) = (x + dx, y + dy);
                    if placed < group
                        && gx >= 0
                        && gy >= 0
                        && gx < w
                        && gy < h
                        && !busy(&tiles, &monsters, &items, gx, gy)
                        && !avoided(gx, gy)
                        && (gx - lx).abs() + (gy - ly).abs() > 8
                    {
                        let mut mate = first.clone();
                        (mate.x, mate.y) = (gx, gy);
                        monsters.push(mate);
                        placed += 1;
                    }
                }
            }
        }
        // Sites of interest: 2 to 4, by weight, away from the landing.
        let total: f64 = self.data.sites.iter().map(|s| s.weight).sum();
        for _ in 0..(2 + rng.next_u64() % 3) {
            let (x, y) = (
                (rng.next_u64() % MAP_W as u64) as i32,
                (rng.next_u64() % MAP_H as u64) as i32,
            );
            if busy(&tiles, &monsters, &items, x, y) || (x - lx).abs() + (y - ly).abs() < 10 {
                continue;
            }
            if sites.iter().any(|s| s.x == x && s.y == y) {
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
            tiles[at(x, y)] = T_SITE;
            sites.push(SiteInst {
                def,
                x,
                y,
                used: false,
            });
        }
        let floor = Floor {
            seen: vec![false; MAP_W * MAP_H],
            tiles,
            monsters,
            items,
            sites,
            start: (lx, ly),
            mats,
            owned: Vec::new(),
        };
        let names: Vec<String> = party
            .iter()
            .map(|&i| self.crew.members[i].name.clone())
            .collect();
        let mut r = Rogue {
            theme: surface,
            seed,
            w: MAP_W,
            h: MAP_H,
            floors: vec![floor],
            cur: 0,
            party,
            x: lx,
            y: ly,
            active: 0,
            loot: Vec::new(),
            turn: 0,
            log: Vec::new(),
            over: false,
            kills: 0,
            buff: 0,
            drone_turns: 0,
            boss_dead: false,
            body: Some(body),
            land: Some((cx, cy)),
            place: b.name.clone(),
            vehicle,
            env: Some((map.pressure, temp_here)),
            turn_seconds: SURFACE_TURN_SECONDS,
            parent: None,
            social: false,
            terminal: None,
            mags: Vec::new(),
            recoil: 0.0,
        };
        r.say(format!(
            "Debarquement en {} : {}{}.",
            b.name.to_lowercase(),
            names.join(", "),
            if vehicle { ", avec le rover" } else { "" }
        ));
        r.reveal();
        self.rogue = Some(r);
        Ok(())
    }

    // ---- the squad -------------------------------------------------------------------------

    /// The squad on a planet breathes its air and feels its cold: tell the body model what each
    /// member is exposed to during the next turn (a sealed suit gives its own air and pressure).
    fn expose_party(&mut self, r: &mut Rogue, seconds: f64) {
        let (Some((pressure, temp)), true) = (r.env, r.on_surface()) else {
            return;
        };
        let (o2, co2, grav) =
            r.body
                .and_then(|b| self.system.bodies.get(b))
                .map_or((0.21, 0.0, 1.0), |b| {
                    let f = |gas: &str| {
                        b.atmosphere
                            .iter()
                            .find(|(n, _)| *n == gas)
                            .map_or(0.0, |(_, v)| f64::from(*v))
                    };
                    (f("O2"), f("CO2"), b.gravity / crate::body::G0)
                });
        let mut list = Vec::new();
        for who in self.party_up(r) {
            let p = &self.crew.members[who];
            let defense = p.gear_stat(&self.data, "defense");
            let mut e = crate::body::Exposure {
                pressure_kpa: pressure * 100.0,
                o2_frac: o2,
                co2_kpa: co2 * pressure * 100.0,
                temp_k: temp,
                gravity_g: grav,
                accel_g: 0.0,
                rad_sv_day: 0.0,
                insulation: defense.clamp(0.0, 0.95),
            };
            if defense >= 0.15 {
                // A sealed suit: its own pressure and oxygen, whatever the air outside.
                e.pressure_kpa = e.pressure_kpa.max(40.0);
                e.o2_frac = if e.pressure_kpa > 100.0 {
                    o2.max(0.21)
                } else {
                    1.0
                };
                e.co2_kpa = 0.0;
            }
            if e.pressure_kpa * e.o2_frac < 16.0 && r.turn.is_multiple_of(10) {
                let name = p.name.clone();
                r.say(format!(
                    "{name} suffoque : l'air d'ici ne se respire pas (il faut une combinaison etanche)."
                ));
            }
            list.push((p.id, e));
        }
        self.crew.away = list;
        self.crew.away_until = self.time + seconds + 2.0 * crate::crew::TICK;
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

    /// Kg more the squad can pick up: the hold and the packs both have a limit.
    fn loot_room(&self, r: &Rogue) -> f64 {
        let carried = r.loot_mass();
        let hold = self.ship.cargo.cap_mass - self.ship.cargo.mass - carried - r.loot_above();
        let pack = r.carry_cap(self.party_up(r).len()) - carried;
        hold.min(pack).max(0.0)
    }

    /// Put `kg` of `res` in the pack as far as there is room; returns what was taken.
    fn rogue_gain(&mut self, r: &mut Rogue, res: usize, kg: f64) -> f64 {
        let take = kg.min(self.loot_room(r));
        if take < kg - 1e-9 {
            r.say("Trop lourd : une partie est abandonnee sur place.");
        }
        if take > 0.0 {
            match r.loot.iter_mut().find(|l| l.0 == res) {
                Some(l) => l.1 += take,
                None => r.loot.push((res, take)),
            }
        }
        take
    }

    /// What the tools of the lead add to a harvest.
    fn gather_bonus(&self, r: &Rogue) -> f64 {
        let up = self.party_up(r);
        let lead = up.get(r.active.min(up.len().saturating_sub(1)));
        1.0 + lead.map_or(0.0, |&i| {
            self.crew.members[i].gear_stat(&self.data, "gather")
        })
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
        // The blow leaves a localised wound: cut, bruise, break, burn or head injury.
        if dmg >= 0.04 {
            let roll = hash_coord(r.seed, i64::from(r.turn), who as i64, 59) % 100;
            let kind = match roll {
                0..=44 => "coupure",
                45..=69 => "contusion",
                70..=84 => "fracture_os",
                85..=94 => "brulure_plaie",
                _ => "traumatisme_crane",
            };
            let hd = std::sync::Arc::clone(&self.data);
            let pick = hash_coord(r.seed, i64::from(r.turn), who as i64, 61);
            self.crew.members[who]
                .body
                .strike(&hd, kind, (dmg * 1.5).clamp(0.1, 1.0) as f32, pick);
        }
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

    // ---- firearms: magazines, recoil, reloading, grenades ------------------------------------

    /// The firearm a member carries (a weapon with a magazine), if any.
    fn firearm(&self, who: usize) -> Option<&vector_data::Gear> {
        self.crew
            .members
            .get(who)?
            .gear
            .iter()
            .flatten()
            .filter_map(|&g| self.data.gear.get(g))
            .find(|g| g.mag > 0)
    }

    /// Rounds left in the magazine of a member (a full one until the first shot of the run).
    pub fn rogue_rounds(&self, r: &Rogue, who: usize) -> u32 {
        let full = self.firearm(who).map_or(0, |g| g.mag);
        r.mags
            .iter()
            .find(|m| m.0 == who)
            .map_or(full, |m| m.1.min(full))
    }

    /// Rounds in the magazines of the squad and their capacity, if anyone carries a firearm.
    pub fn rogue_ammo_status(&self, r: &Rogue) -> Option<(u32, u32)> {
        let guns: Vec<usize> = self
            .party_up(r)
            .into_iter()
            .filter(|&w| self.firearm(w).is_some())
            .collect();
        if guns.is_empty() {
            return None;
        }
        let rounds = guns.iter().map(|&w| self.rogue_rounds(r, w)).sum();
        let cap = guns
            .iter()
            .filter_map(|&w| self.firearm(w).map(|g| g.mag))
            .sum();
        Some((rounds, cap))
    }

    fn set_rounds(r: &mut Rogue, who: usize, n: u32) {
        match r.mags.iter_mut().find(|m| m.0 == who) {
            Some(m) => m.1 = n,
            None => r.mags.push((who, n)),
        }
    }

    /// The nearest hostile creature in view within `range` cells and with a clear line.
    fn rogue_nearest_foe(&self, r: &Rogue, range: i32) -> Option<usize> {
        let vis = r.visible();
        r.floor()
            .monsters
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                !m.friendly
                    && m.hunts
                    && vis[r.idx(m.x, m.y)]
                    && (m.x - r.x).abs().max((m.y - r.y).abs()) <= range
                    && r.line_clear(r.x, r.y, m.x, m.y)
            })
            .min_by_key(|(_, m)| (m.x - r.x).abs().max((m.y - r.y).abs()))
            .map(|(k, _)| k)
    }

    /// Wake the sleepers within `noise` cells of the squad.
    fn rogue_noise(r: &mut Rogue, noise: f64) {
        let (px, py) = (r.x, r.y);
        for o in &mut r.floor_mut().monsters {
            if !o.awake && f64::from((o.x - px).abs().max((o.y - py).abs())) <= noise {
                o.awake = true;
            }
        }
    }

    /// The squad's firearms shoot the nearest hostile in view: one round each, a hit chance
    /// that falls with the distance and with the recoil of the shots before, a louder noise
    /// than a blade. Empty magazines click.
    pub fn rogue_fire(&mut self) -> Result<String, &'static str> {
        let mut r = self.rogue.take().ok_or("pas d'exploration en cours")?;
        let res = self.rogue_fire_inner(&mut r);
        self.rogue = Some(r);
        res
    }

    fn rogue_fire_inner(&mut self, r: &mut Rogue) -> Result<String, &'static str> {
        if r.over {
            return Err("l'exploration est terminee");
        }
        let up = self.party_up(r);
        let shooters: Vec<usize> = up
            .iter()
            .copied()
            .filter(|&w| self.firearm(w).is_some())
            .collect();
        if shooters.is_empty() {
            return Err("aucune arme a feu equipee");
        }
        let range = shooters
            .iter()
            .filter_map(|&w| self.firearm(w).map(|g| g.range))
            .max()
            .unwrap_or(0);
        let k = self
            .rogue_nearest_foe(r, range)
            .ok_or("aucune cible a portee")?;
        let (tx, ty) = (r.floor().monsters[k].x, r.floor().monsters[k].y);
        let dist = f64::from((tx - r.x).abs().max((ty - r.y).abs()));
        let mut rng = Rng::new(hash_coord(r.seed, i64::from(r.turn), 13, 47));
        let lead = r.active.min(up.len().saturating_sub(1));
        let mut dealt = 0.0;
        let (mut shots, mut clicks) = (0, 0);
        let mut noise: f64 = 0.0;
        for &who in &shooters {
            let rounds = self.rogue_rounds(r, who);
            let g = self.firearm(who).expect("a shooter").clone();
            if rounds == 0 || g.range < dist as i32 {
                clicks += u32::from(rounds == 0);
                continue;
            }
            Self::set_rounds(r, who, rounds - 1);
            shots += 1;
            noise = noise.max(g.noise);
            let accuracy = fire_accuracy(dist, r.recoil);
            r.recoil = (r.recoil + g.recoil).min(MAX_RECOIL);
            if rng.f64() < accuracy {
                let who_is_lead = up.get(lead) == Some(&who);
                dealt += self.rogue_blow(who, &mut rng) * 1.5 * if who_is_lead { 1.0 } else { 0.7 };
            }
        }
        if shots == 0 {
            if clicks > 0 {
                r.say("Clic : chargeur vide.");
                self.rogue_end_turn(r);
                return Err("chargeur vide : rechargez");
            }
            return Err("cible hors de portee");
        }
        Self::rogue_noise(r, noise);
        let name = r.floor().monsters[k].name.to_lowercase();
        if dealt <= 0.0 {
            r.say(format!("Vous tirez sur {name} : manque."));
            self.rogue_end_turn(r);
            return Ok(format!("{shots} COUP(S) : MANQUE"));
        }
        let defense = r.floor().monsters[k].defense;
        let dmg = dealt * (1.0 - defense) * if r.buff > 0 { 1.5 } else { 1.0 };
        self.rogue_wound_monster(r, k, dmg, &mut rng, "touchez");
        self.rogue_end_turn(r);
        Ok(format!("{shots} COUP(S) : {dmg:.1} DEGATS"))
    }

    /// Reload every firearm of the squad from the ammunition of the hold (a turn).
    pub fn rogue_reload(&mut self) -> Result<String, &'static str> {
        let mut r = self.rogue.take().ok_or("pas d'exploration en cours")?;
        let res = (|| {
            if r.over {
                return Err("l'exploration est terminee");
            }
            let data = std::sync::Arc::clone(&self.data);
            let mut loaded = 0;
            let mut empty_hold = false;
            let mut any_gun = false;
            for who in self.party_up(&r) {
                let Some(g) = self.firearm(who).cloned() else {
                    continue;
                };
                any_gun = true;
                let have = self.rogue_rounds(&r, who);
                let missing = g.mag.saturating_sub(have);
                if missing == 0 {
                    continue;
                }
                let Some(res) = data.resource_index(&g.ammo) else {
                    continue;
                };
                let take = (f64::from(missing) * ROUND_KG).min(self.ship.cargo.kg[res]);
                if take < ROUND_KG {
                    empty_hold = true;
                    continue;
                }
                let n = (take / ROUND_KG).floor() as u32;
                self.ship.cargo.remove(&data, res, f64::from(n) * ROUND_KG);
                Self::set_rounds(&mut r, who, have + n);
                loaded += n;
            }
            if !any_gun {
                return Err("aucune arme a feu equipee");
            }
            if loaded == 0 {
                return Err(if empty_hold {
                    "plus de munitions en soute"
                } else {
                    "les chargeurs sont pleins"
                });
            }
            r.say(format!("Rechargement : {loaded} coups."));
            self.rogue_end_turn(&mut r);
            Ok(format!("RECHARGEMENT : {loaded} COUPS"))
        })();
        self.rogue = Some(r);
        res
    }

    /// Damage (points) of a charge of `kg` of the explosive reaction `reaction` of the
    /// chemistry: the heat it releases (Hess' law, per kg of reagents) times a fixed share.
    pub fn blast_damage(&self, reaction: &str, kg: f64) -> Option<f64> {
        let r = self.data.reactions.iter().find(|r| r.id == reaction)?;
        let mj_per_kg = -crate::chemistry::energy_density(&self.data, r);
        (mj_per_kg > 0.0).then_some(mj_per_kg * kg * BLAST_DMG_PER_MJ)
    }

    /// Throw a grenade at the nearest hostile in view: the blast follows the chemistry of the
    /// charge, falls off with the distance from the centre, stops at walls, and hurts the squad
    /// too when it is thrown too close. A grenade makes a noise that wakes everything near.
    pub fn rogue_grenade(&mut self) -> Result<String, &'static str> {
        let mut r = self.rogue.take().ok_or("pas d'exploration en cours")?;
        let res = self.rogue_grenade_inner(&mut r);
        self.rogue = Some(r);
        res
    }

    fn rogue_grenade_inner(&mut self, r: &mut Rogue) -> Result<String, &'static str> {
        if r.over {
            return Err("l'exploration est terminee");
        }
        let stock = self
            .data
            .resource_index(GRENADE_RESOURCE)
            .ok_or("pas de grenades dans ce monde")?;
        if self.ship.cargo.kg[stock] < 1.0 {
            return Err("aucune grenade en soute");
        }
        let k = self
            .rogue_nearest_foe(r, GRENADE_RANGE)
            .ok_or("aucune cible a portee")?;
        let (cx, cy) = (r.floor().monsters[k].x, r.floor().monsters[k].y);
        let base = self
            .blast_damage(GRENADE_REACTION, GRENADE_CHARGE_KG)
            .ok_or("charge inconnue")?;
        let data = std::sync::Arc::clone(&self.data);
        self.ship.cargo.remove(&data, stock, 1.0);
        let mut rng = Rng::new(hash_coord(r.seed, i64::from(r.turn), 17, 53));
        let falloff = |d: i32| 1.0 / (1.0 + f64::from(d * d));
        // The creatures first, from the highest index down so removals shift nothing.
        let mut hit: Vec<(usize, f64)> = r
            .floor()
            .monsters
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                let d = (m.x - cx).abs().max((m.y - cy).abs());
                d <= BLAST_RADIUS && r.line_clear(cx, cy, m.x, m.y)
            })
            .map(|(i, m)| {
                let d = (m.x - cx).abs().max((m.y - cy).abs());
                (i, base * falloff(d) * (1.0 - m.defense))
            })
            .collect();
        hit.sort_by_key(|h| std::cmp::Reverse(h.0));
        let mut total = 0.0;
        for (i, d) in &hit {
            if r.floor().monsters[*i].friendly {
                continue;
            }
            total += d;
            self.rogue_wound_monster(r, *i, *d, &mut rng, "soufflez");
        }
        // Then the squad, if it stands in the blast.
        let mut own = 0;
        for who in self.party_up(r) {
            let d = (r.x - cx).abs().max((r.y - cy).abs());
            if d <= BLAST_RADIUS && r.line_clear(cx, cy, r.x, r.y) {
                let p = &mut self.crew.members[who];
                p.health = (p.health - (0.1 * base * falloff(d) / 6.0) as f32).max(0.0);
                own += 1;
            }
        }
        Self::rogue_noise(r, GRENADE_NOISE);
        r.say(format!(
            "La grenade explose : {total:.1} degats{}.",
            if own > 0 {
                ", l'equipe est touchee"
            } else {
                ""
            }
        ));
        self.rogue_end_turn(r);
        Ok(format!("EXPLOSION : {total:.1} DEGATS"))
    }

    /// Hurt creature `k` for `dmg`; if it falls, count the kill and drop its loot.
    fn rogue_wound_monster(
        &mut self,
        r: &mut Rogue,
        k: usize,
        dmg: f64,
        rng: &mut Rng,
        verb: &str,
    ) {
        let name = r.floor().monsters[k].name.clone();
        let m = &mut r.floor_mut().monsters[k];
        m.hp -= dmg;
        m.awake = true;
        if m.hp > 0.0 {
            r.say(format!("Vous {verb} {} ({dmg:.1}).", name.to_lowercase()));
            return;
        }
        let m = r.floor_mut().monsters.remove(k);
        r.kills += 1;
        self.kill_relations(r.body, m.hunts, m.boss);
        r.say(format!("{name} est abattu."));
        let mult = if m.boss { 3.0 } else { 1.0 } * (1.0 + 0.3 * r.depth() as f64);
        for (res, lo, hi) in &m.drops {
            let kg = (lo + (hi - lo) * rng.f64()) * mult;
            r.floor_mut().items.push(Item {
                x: m.x,
                y: m.y,
                res: *res,
                kg,
                plant: false,
            });
        }
        if m.boss {
            r.boss_dead = true;
            self.research_pool += 100.0;
            r.say("Le maitre des lieux est tombe : on en tirera des enseignements.");
        }
    }

    // ---- moving ----------------------------------------------------------------------------

    /// One move of the squad: a step (two with the rover on open ground), or an attack when
    /// something stands there.
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
        if !r.walkable(r.x + dx, r.y + dy) {
            return Err(if r.tile(r.x + dx, r.y + dy) == T_WATER {
                "de l'eau"
            } else {
                "un mur"
            });
        }
        let steps = if r.vehicle && r.on_surface() {
            VEHICLE_SPEED
        } else {
            1
        };
        let mut moved = false;
        for _ in 0..steps {
            let (nx, ny) = (r.x + dx, r.y + dy);
            if let Some(k) = r.monster_at(nx, ny) {
                if r.floor().monsters[k].friendly {
                    self.rogue_talk(r, k);
                } else {
                    self.rogue_attack(r, k);
                }
                break;
            }
            if !r.walkable(nx, ny) {
                break;
            }
            r.x = nx;
            r.y = ny;
            moved = true;
            self.rogue_arrive(r);
        }
        if moved {
            self.rogue_slide(r, dx, dy);
        }
        self.rogue_end_turn(r);
        Ok(())
    }

    /// Slippery ground (ice) carries the squad one cell further, sometimes two or three, until
    /// something stops it; a rover grips better.
    fn rogue_slide(&mut self, r: &mut Rogue, dx: i32, dy: i32) {
        let grip = if r.vehicle && r.on_surface() {
            0.4
        } else {
            1.0
        };
        for n in 0..3 {
            let slick = r
                .mat(r.x, r.y)
                .map_or(0.0, |m| self.data.materials[m].slick)
                * grip;
            if slick <= 0.0 || r.over {
                return;
            }
            let mut rng = Rng::new(hash_coord(
                r.seed,
                i64::from(r.turn),
                i64::from(r.x) * 131 + i64::from(r.y),
                83 + n,
            ));
            let (nx, ny) = (r.x + dx, r.y + dy);
            if rng.f64() >= slick || !r.walkable(nx, ny) || r.monster_at(nx, ny).is_some() {
                return;
            }
            r.x = nx;
            r.y = ny;
            r.say("Vous derapez sur la glace.");
            self.rogue_arrive(r);
        }
    }

    /// Talk to a person: a member of the crew tells how they feel, a passer-by a line of the
    /// place, sometimes a rumour of the exchange (the same rumours as the brokerage).
    fn rogue_talk(&mut self, r: &mut Rogue, k: usize) {
        let m = r.floor().monsters[k].clone();
        let mut rng = Rng::new(hash_coord(r.seed, i64::from(r.turn), k as i64, 71));
        let text = if let Some(id) = m.crew {
            match self.crew.members.iter().find(|p| p.id == id) {
                None => "...".to_string(),
                Some(p) => {
                    let mut worries: Vec<(f32, &str)> = vec![
                        (p.hunger, "J'ai faim."),
                        (p.thirst, "J'ai soif."),
                        (p.fatigue, "Je tombe de sommeil."),
                        (p.stress, "Je suis a cran, ces temps-ci."),
                        (p.social, "On ne se parle plus assez, ici."),
                    ];
                    worries.sort_by(|a, b| b.0.total_cmp(&a.0));
                    let role = self.data.roles[p.role].name.to_lowercase();
                    let mood = if self.is_pregnant(p.id) {
                        "J'attends un enfant.".to_string()
                    } else if worries[0].0 > 0.6 {
                        worries[0].1.to_string()
                    } else if p.morale > 0.7 {
                        "Tout va bien, merci de demander.".to_string()
                    } else {
                        "On fait aller.".to_string()
                    };
                    format!("{mood} ({role}, sante {:.0} %)", p.health * 100.0)
                }
            }
        } else {
            let rumours = self.rumours();
            if !rumours.is_empty() && rng.f64() < 0.35 {
                let ru = &rumours[(rng.f64() * rumours.len() as f64) as usize % rumours.len()];
                let name = self.data.world_events[ru.def].name.to_lowercase();
                let days = ((ru.start - self.time) / 86_400.0).max(0.0);
                format!("On dit que \"{name}\" arrive dans {days:.0} jours.")
            } else if m.lines.is_empty() {
                "...".to_string()
            } else {
                m.lines[(rng.f64() * m.lines.len() as f64) as usize % m.lines.len()].clone()
            }
        };
        r.say(format!("{} : {text}", m.name));
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
        let defense = r.floor().monsters[k].defense;
        let dmg = total * (1.0 - defense);
        // A loud weapon wakes the sleepers around it (the gear's `noise` radius, in cells).
        let noise = up
            .iter()
            .map(|&who| self.crew.members[who].gear_stat(&self.data, "noise"))
            .fold(0.0, f64::max);
        Self::rogue_noise(r, noise);
        self.rogue_wound_monster(r, k, dmg, &mut rng, "frappez");
    }

    /// The squad is on a new tile: pick up what lies there, search what lies there, trip what
    /// lies there.
    fn rogue_arrive(&mut self, r: &mut Rogue) {
        let (x, y) = (r.x, r.y);
        while let Some(k) = r.floor().items.iter().position(|i| i.x == x && i.y == y) {
            let it = r.floor().items[k].clone();
            if self.loot_room(r) <= 0.01 {
                r.say("Vous ne pouvez plus rien porter : vous laissez le butin.");
                break;
            }
            let bonus = if it.plant { self.gather_bonus(r) } else { 1.0 };
            let taken = self.rogue_gain(r, it.res, it.kg * bonus) / bonus;
            let name = self.data.resources[it.res].name.to_lowercase();
            if taken > 0.0 {
                r.say(format!("Vous ramassez {taken:.1} kg de {name}."));
            }
            if taken < it.kg - 1e-9 {
                r.floor_mut().items[k].kg -= taken;
                break;
            }
            r.floor_mut().items.remove(k);
        }
        if let Some(k) = r
            .floor()
            .sites
            .iter()
            .position(|s| s.x == x && s.y == y && !s.used)
        {
            self.rogue_search_site(r, k);
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

    /// Search a site of a surface once: loot, a rest, sometimes a mishap.
    fn rogue_search_site(&mut self, r: &mut Rogue, k: usize) {
        r.floor_mut().sites[k].used = true;
        let data = std::sync::Arc::clone(&self.data);
        let site = &data.sites[r.floor().sites[k].def];
        r.say(format!("{} : {}", site.name, site.text));
        let bonus = self.gather_bonus(r);
        for (res, kg) in &site.loot {
            if let Some(i) = data.resource_index(res) {
                self.rogue_gain(r, i, *kg * bonus);
            }
        }
        // Ruins and wrecks hold the things of the sector's history; terminals hold ciphers.
        let dig_key = hash_coord(
            r.seed,
            r.floor().sites[k].x as i64,
            r.floor().sites[k].y as i64,
            71,
        );
        for a in self.dig_site(&site.id, dig_key) {
            let line = crate::archive::artefact_line(self, a);
            r.say(format!("Trouvaille : {line}"));
        }
        if site.heal {
            for m in self.party_up(r) {
                let p = &mut self.crew.members[m];
                p.health = (p.health + 0.3).min(1.0);
                p.fatigue = (p.fatigue - 0.5).max(0.0);
                for o in p.organs.iter_mut() {
                    if *o > 0.0 {
                        *o = (*o + 0.2).min(1.0);
                    }
                }
            }
            r.say("Toute l'equipe se repose et se soigne.");
        }
        let mut rng = Rng::new(hash_coord(r.seed, i64::from(r.turn), k as i64, 63));
        if site.danger > 0.0 && rng.f64() < site.danger {
            let up = self.party_up(r);
            if let Some(&who) = up.get(r.active.min(up.len().saturating_sub(1))) {
                let dmg = 0.2 + 0.3 * rng.f64();
                self.rogue_hurt(r, who, dmg, "Un eboulement ou une embuscade");
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
                if rng.f64() < 0.35 {
                    let strength = (2.0 + 6.0 * rng.f64()).round() * depth_mult.round().max(1.0);
                    self.add_cipher(strength, "ARCHIVE CHIFFREE");
                    r.say("Le terminal garde aussi une archive chiffree : elle ira dans votre memoire.");
                }
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
                        self.rogue_gain(r, ri, kg);
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

    // ---- matter: digging and building ----------------------------------------------------------

    /// Health lost this turn to the matter the squad stands on or next to.
    fn cell_harm(&self, r: &Rogue) -> f64 {
        let mats = &self.data.materials;
        let here = r.mat(r.x, r.y).map_or(0.0, |m| mats[m].harm);
        let near = (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
            .filter_map(|(dx, dy)| r.mat(r.x + dx, r.y + dy))
            .filter(|&m| mats[m].kind == "liquid")
            .map(|m| mats[m].harm / 3.0)
            .fold(0.0, f64::max);
        here.max(near)
    }

    /// What the physics of the world the squad is on does to the matter it digs: dense worlds are
    /// rich in metals, active insides in volcanic minerals, cold surfaces in ice and volatiles, and
    /// deeper floors of a mine are richer.
    pub fn dig_mods(&self, r: &Rogue) -> DigMods {
        let mut root = r;
        while let Some(p) = root.parent.as_deref() {
            root = p;
        }
        let mut m = DigMods::NEUTRAL;
        if let Some(body) = root.body {
            if let Some(map) = self.planet_map(body) {
                let ph = self.system.physics(body, f64::from(map.surface.tilt));
                m.metal = (ph.density / 5514.0).clamp(0.4, 2.0);
                m.volcanic = match ph.activity {
                    vector_world::physics::Activity::Dead => 0.3,
                    vector_world::physics::Activity::Stagnant => 0.7,
                    vector_world::physics::Activity::Plates
                    | vector_world::physics::Activity::Fluid => 1.0,
                    vector_world::physics::Activity::Volcanic => 2.5,
                };
            }
            if r.on_surface() {
                let t = root.env.map_or(288.0, |e| e.1);
                m.cold = if t < 273.0 {
                    (1.0 + (273.0 - t) / 100.0).min(3.0)
                } else if t > 330.0 {
                    0.1
                } else {
                    0.3
                };
            }
        }
        m.depth = 1.0 + 0.25 * r.cur as f64;
        m
    }

    /// The lead's tool: the extra share a pick or a scanner gives to a harvest.
    fn lead_tool(&self, r: &Rogue) -> f64 {
        self.gather_bonus(r) - 1.0
    }

    /// Break a wall or dig the ground of the cell next to the squad: it takes turns (more in hard
    /// matter, fewer with a pick), gives what the matter holds as the world's physics allow, and
    /// lays bare the floor under it. Returns what was found.
    pub fn rogue_dig(&mut self, dx: i32, dy: i32) -> Result<String, &'static str> {
        let mut r = self.rogue.take().ok_or("pas d'exploration en cours")?;
        let res = self.rogue_dig_inner(&mut r, dx.signum(), dy.signum());
        self.rogue = Some(r);
        res
    }

    fn rogue_dig_inner(&mut self, r: &mut Rogue, dx: i32, dy: i32) -> Result<String, &'static str> {
        if r.over {
            return Err("l'exploration est terminee");
        }
        if (dx, dy) == (0, 0) {
            return Err("choisissez une direction");
        }
        let (tx, ty) = (r.x + dx, r.y + dy);
        if !r.inside(tx, ty) || !matches!(r.tile(tx, ty), T_WALL | T_FLOOR) {
            return Err("rien a creuser ici");
        }
        if r.monster_at(tx, ty).is_some() {
            return Err("la place est occupee");
        }
        let mi = r.mat(tx, ty).ok_or("rien a creuser ici")?;
        let data = std::sync::Arc::clone(&self.data);
        let m = &data.materials[mi];
        if m.kind == "liquid" || m.under.is_empty() {
            return Err("rien a creuser ici");
        }
        let tool = self.lead_tool(r);
        if m.hardness >= 0.75 && tool < 0.25 {
            return Err("trop dur : il faut un outil de fouille");
        }
        let under = data.material_index(&m.under).ok_or("rien a creuser ici")?;
        let i = r.idx(tx, ty);
        let mods = self.dig_mods(r);
        let mut rng = Rng::new(hash_coord(
            r.seed,
            i64::from(tx),
            i64::from(ty),
            91 + r.cur as i64,
        ));
        let bonus = 1.0 + tool;
        let turns = 1 + (m.hardness * 6.0 / (1.0 + 2.0 * tool)).round() as u32;
        let mut found: Vec<String> = Vec::new();
        for (res, kg, rarity) in &m.yields {
            let Some(ri) = data.resource_index(res) else {
                continue;
            };
            let f = mods.factor(&data.resources[ri].category, res);
            if rng.f64() >= (rarity * f).min(1.0) {
                continue;
            }
            let amount = kg * f * (0.6 + 0.8 * rng.f64()) * bonus;
            let taken = self.rogue_gain(r, ri, amount);
            if taken > 0.0 {
                found.push(format!(
                    "{taken:.1} kg de {}",
                    data.resources[ri].name.to_lowercase()
                ));
            }
        }
        let f = r.floor_mut();
        f.tiles[i] = T_FLOOR;
        f.mats[i] = under as u8;
        let what = if found.is_empty() {
            "rien d'utile".to_string()
        } else {
            found.join(", ")
        };
        let msg = format!("{} creuse ({turns} tours) : {what}", m.name);
        r.say(format!("{msg}."));
        for _ in 0..turns {
            if r.over {
                break;
            }
            self.rogue_end_turn(r);
        }
        Ok(msg)
    }

    /// Build a wall (`wall`) or a floor of the squad's own at the cell next to it, out of what it
    /// carries: the first material of that kind that has a cost in `materials.ron`. A floor can
    /// also be laid over water (a bridge). The cell becomes the squad's property.
    pub fn rogue_build(&mut self, dx: i32, dy: i32, wall: bool) -> Result<String, &'static str> {
        let mut r = self.rogue.take().ok_or("pas d'exploration en cours")?;
        let res = self.rogue_build_inner(&mut r, dx.signum(), dy.signum(), wall);
        self.rogue = Some(r);
        res
    }

    fn rogue_build_inner(
        &mut self,
        r: &mut Rogue,
        dx: i32,
        dy: i32,
        wall: bool,
    ) -> Result<String, &'static str> {
        if r.over {
            return Err("l'exploration est terminee");
        }
        if (dx, dy) == (0, 0) {
            return Err("choisissez une direction");
        }
        let (tx, ty) = (r.x + dx, r.y + dy);
        let t = r.tile(tx, ty);
        if !r.inside(tx, ty) || !(t == T_FLOOR || (!wall && t == T_WATER)) {
            return Err("on ne peut pas batir ici");
        }
        if r.monster_at(tx, ty).is_some() || r.floor().items.iter().any(|i| (i.x, i.y) == (tx, ty))
        {
            return Err("la place est occupee");
        }
        let kind = if wall { "wall" } else { "floor" };
        let data = std::sync::Arc::clone(&self.data);
        let mi = data
            .materials
            .iter()
            .position(|m| m.kind == kind && !m.cost.is_empty())
            .ok_or("rien a batir")?;
        let m = &data.materials[mi];
        if r.mat(tx, ty) == Some(mi) {
            return Err("c'est deja bati");
        }
        for (res, kg) in &m.cost {
            let ri = data.resource_index(res).ok_or("matiere inconnue")?;
            if r.loot.iter().find(|l| l.0 == ri).map_or(0.0, |l| l.1) < *kg - 1e-9 {
                return Err("il manque des materiaux dans le sac");
            }
        }
        for (res, kg) in &m.cost {
            let ri = data.resource_index(res).ok_or("matiere inconnue")?;
            if let Some(l) = r.loot.iter_mut().find(|l| l.0 == ri) {
                l.1 -= kg;
            }
        }
        r.loot.retain(|l| l.1 > 1e-9);
        let i = r.idx(tx, ty);
        let (w, h) = (r.w, r.h);
        let f = r.floor_mut();
        f.tiles[i] = if wall { T_WALL } else { T_FLOOR };
        f.mats[i] = mi as u8;
        f.owned.resize(w * h, false);
        f.owned[i] = true;
        let cost: Vec<String> = m
            .cost
            .iter()
            .filter_map(|(res, kg)| {
                data.resource_index(res)
                    .map(|ri| format!("{kg:.0} kg de {}", data.resources[ri].name.to_lowercase()))
            })
            .collect();
        let msg = format!("{} bati ({})", m.name, cost.join(", "));
        r.say(format!("{msg}."));
        self.rogue_end_turn(r);
        Ok(msg)
    }

    /// The AI sends a drone (one unit of robotics): for DRONE_TURNS turns it picks up, on its own
    /// and at half yield, what lies within DRONE_REACH cells of the squad.
    pub fn rogue_deploy_drone(&mut self) -> Result<(), &'static str> {
        let mut r = self.rogue.take().ok_or("pas d'exploration en cours")?;
        let res = self.rogue_drone_inner(&mut r);
        self.rogue = Some(r);
        res
    }

    fn rogue_drone_inner(&mut self, r: &mut Rogue) -> Result<(), &'static str> {
        if r.over {
            return Err("l'exploration est terminee");
        }
        if r.drone_turns > 0 {
            return Err("un drone est deja en vol");
        }
        let res = self
            .data
            .resource_index("robotics")
            .ok_or("pas de robotique")?;
        if self.ship.cargo.kg[res] < DRONE_COST {
            return Err("il faut une unite de robotique dans la soute");
        }
        let data = std::sync::Arc::clone(&self.data);
        self.ship.cargo.remove(&data, res, DRONE_COST);
        r.drone_turns = DRONE_TURNS;
        r.say("Un drone est lance : il ramasse autour de l'equipe.");
        self.rogue_end_turn(r);
        Ok(())
    }

    fn rogue_drone_gather(&mut self, r: &mut Rogue) {
        let (x, y) = (r.x, r.y);
        let Some(k) = r
            .floor()
            .items
            .iter()
            .position(|i| (i.x - x).abs() <= DRONE_REACH && (i.y - y).abs() <= DRONE_REACH)
        else {
            return;
        };
        let it = r.floor_mut().items.remove(k);
        let bonus = if it.plant { self.gather_bonus(r) } else { 1.0 };
        self.rogue_gain(r, it.res, it.kg * 0.5 * bonus);
    }

    // ---- the world answers -----------------------------------------------------------------

    /// Monsters move and strike, hazards and the air bite, the clock runs.
    fn rogue_end_turn(&mut self, r: &mut Rogue) {
        r.turn += 1;
        r.buff = r.buff.saturating_sub(1);
        r.recoil *= RECOIL_DECAY;
        if r.drone_turns > 0 {
            r.drone_turns -= 1;
            self.rogue_drone_gather(r);
            if r.drone_turns == 0 {
                r.say("Le drone rentre, batteries a plat.");
            }
        }
        // The ground slows or speeds the squad (mud, snow and sand cost time, ice saves it); a
        // rover feels it less.
        let slow = r.mat(r.x, r.y).map_or(1.0, |m| self.data.materials[m].slow);
        let slow = if r.vehicle && r.on_surface() {
            1.0 + (slow - 1.0) * 0.4
        } else {
            slow
        };
        self.expose_party(r, r.turn_seconds * slow);
        self.advance(r.turn_seconds * slow, &ShipInput::default());
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
        // The matter underfoot burns (sulphur fumes), and a liquid like lava scorches what is next to it.
        let harm = self.cell_harm(r);
        if harm > 0.0 {
            for who in self.party_up(r) {
                let defense = self.crew.members[who]
                    .gear_stat(&self.data, "defense")
                    .min(0.9);
                let p = &mut self.crew.members[who];
                p.health = (p.health - (harm * (1.0 - defense)) as f32).max(0.0);
            }
            if r.turn.is_multiple_of(5) {
                r.say("La matiere ici brule.");
            }
        }
        // The air and the climate of a surface: a suit protects in proportion to its defence, and
        // warm clothes in the cold.
        if let (Some((pressure, temp)), true) = (r.env, r.on_surface()) {
            // What the air is made of (chemistry): poison and lack of oxygen.
            let lungs = r
                .body
                .and_then(|b| self.air_hazard_of(b))
                .map_or(0.0, |a| a.lungs());
            for who in self.party_up(r) {
                let p = &self.crew.members[who];
                let defense = p.gear_stat(&self.data, "defense")
                    + if temp < 250.0 {
                        p.gear_stat(&self.data, "warmth")
                    } else {
                        0.0
                    };
                let (mut dmg, mut why) = environment_damage(pressure, temp, defense);
                if lungs > 0.0 {
                    dmg += lungs * (1.0 - (1.5 * defense).min(0.95));
                    if why.is_empty() {
                        why = "L'air est irrespirable";
                    }
                }
                if dmg > 0.0 {
                    let p = &mut self.crew.members[who];
                    p.health = (p.health - (dmg * r.turn_seconds / 60.0) as f32).max(0.0);
                    if r.turn.is_multiple_of(10) {
                        r.say(format!("{why}."));
                    }
                }
            }
        }
        let mut rng = Rng::new(hash_coord(r.seed, i64::from(r.turn), r.cur as i64, 41));
        let (px, py) = (r.x, r.y);
        let data = std::sync::Arc::clone(&self.data);
        for k in 0..r.floor().monsters.len() {
            let (mx, my, hunts, attack, sight, awake, stationary, sleeper, friendly) = {
                let m = &r.floor().monsters[k];
                (
                    m.x,
                    m.y,
                    m.hunts,
                    m.attack,
                    m.sight,
                    m.awake,
                    m.stationary,
                    m.sleeper,
                    m.friendly,
                )
            };
            let name = r.floor().monsters[k].name.clone();
            let dist = (mx - px).abs().max((my - py).abs());
            let sees = dist <= sight && r.line_clear(mx, my, px, py);
            if !awake {
                // Sleepers wake when you come close; the mimic only when touched.
                let wake = if sleeper {
                    dist <= 1
                } else {
                    sees && dist <= sight / 2 + 1
                };
                if wake {
                    r.floor_mut().monsters[k].awake = true;
                    if sees {
                        r.say(format!("{name} vous remarque."));
                    }
                }
                continue;
            }
            let animal = r.floor().monsters[k].species.is_some();
            let free = |r: &Rogue, x: i32, y: i32| {
                r.walkable(x, y)
                    && r.monster_at(x, y).is_none()
                    && (x, y) != (px, py)
                    && !(animal && r.mat(x, y).is_some_and(|m| animal_avoids(&data, m)))
            };
            if hunts {
                if dist <= 1 {
                    let up = self.party_up(r);
                    if up.is_empty() {
                        break;
                    }
                    let who = up[(rng.f64() * up.len() as f64) as usize % up.len()];
                    let dmg = attack * (0.6 + 0.8 * rng.f64()) * (1.0 + 0.1 * r.depth() as f64);
                    self.rogue_hurt(r, who, dmg, &name);
                } else if sees && !stationary {
                    let (sx, sy) = ((px - mx).signum(), (py - my).signum());
                    for (ax, ay) in [(sx, sy), (sx, 0), (0, sy)] {
                        let (tx, ty) = (mx + ax, my + ay);
                        if (ax, ay) != (0, 0) && free(r, tx, ty) {
                            let mm = &mut r.floor_mut().monsters[k];
                            mm.x = tx;
                            mm.y = ty;
                            break;
                        }
                    }
                }
            } else if !stationary {
                // Prey flees from the squad and otherwise wanders a little.
                let target = if dist <= 3 && !friendly {
                    (mx - (px - mx).signum(), my - (py - my).signum())
                } else if rng.f64() < 0.3 {
                    (
                        mx + (rng.next_u64() % 3) as i32 - 1,
                        my + (rng.next_u64() % 3) as i32 - 1,
                    )
                } else {
                    (mx, my)
                };
                if target != (mx, my) && free(r, target.0, target.1) {
                    let mm = &mut r.floor_mut().monsters[k];
                    mm.x = target.0;
                    mm.y = target.1;
                }
            }
        }
        let up = self.party_up(r);
        if up.is_empty() {
            r.over = true;
            r.loot.clear();
            r.say("Plus personne ne se releve. L'exploration est perdue.");
        } else if r.active >= up.len() {
            r.active = 0;
        }
        r.reveal();
    }

    /// Catch alive the tame animal next to the squad for the pens of the ship (the livestock):
    /// only species that can be kept, only if there is room in the pens.
    pub fn rogue_capture(&mut self) -> Result<String, &'static str> {
        let mut r = self.rogue.take().ok_or("pas d'exploration en cours")?;
        let res = self.rogue_capture_inner(&mut r);
        self.rogue = Some(r);
        res
    }

    fn rogue_capture_inner(&mut self, r: &mut Rogue) -> Result<String, &'static str> {
        if r.over {
            return Err("l'exploration est terminee");
        }
        let near = (0..r.floor().monsters.len()).find(|&k| {
            let m = &r.floor().monsters[k];
            m.species.is_some() && (m.x - r.x).abs().max((m.y - r.y).abs()) <= 1
        });
        let k = near.ok_or("aucune bete a portee")?;
        let sp = r.floor().monsters[k].species.expect("checked");
        if !self.domestic_species().contains(&sp) {
            return Err("cette bete ne se laisse pas apprivoiser");
        }
        if self.herd.len() >= crate::livestock::HERD_MAX {
            return Err("enclos pleins");
        }
        let name = r.floor().monsters[k].name.clone();
        let n = self.herd_counter;
        self.herd_counter += 1;
        let mut a = crate::livestock::random_animal(
            hash_coord(self.galaxy.seed, r.seed as i64, i64::from(r.turn), 5),
            sp,
            self.time,
            n,
        );
        a.born = self.time - crate::bourse::DAY * 30.0;
        self.herd.push(a);
        r.floor_mut().monsters.remove(k);
        r.say(format!(
            "Vous capturez : {}. Elle ira dans les enclos.",
            name.to_lowercase()
        ));
        self.rogue_end_turn(r);
        Ok(format!("{} CAPTURE", name.to_uppercase()))
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

    // ---- stairs and the way out --------------------------------------------------------------

    /// Use what is under the squad: stairs down or up, a site (down into its dungeon), the
    /// entrance (back to the place above, or to the ship).
    pub fn rogue_stairs(&mut self) -> Result<String, &'static str> {
        let mut r = self.rogue.take().ok_or("pas d'exploration en cours")?;
        if r.over {
            self.rogue = Some(r);
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
                self.rogue = Some(r);
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
                self.rogue = Some(r);
                Ok(msg)
            }
            T_UP if r.parent.is_some() => {
                // Back up to the surface, with what was found.
                let mut parent = *r.parent.take().expect("checked");
                for (res, kg) in std::mem::take(&mut r.loot) {
                    match parent.loot.iter_mut().find(|l| l.0 == res) {
                        Some(l) => l.1 += kg,
                        None => parent.loot.push((res, kg)),
                    }
                }
                parent.kills += r.kills;
                parent.turn += r.turn;
                parent.boss_dead |= r.boss_dead;
                parent.say("Retour a la surface.");
                parent.reveal();
                self.rogue = Some(parent);
                Ok("RETOUR A LA SURFACE".to_string())
            }
            T_UP => {
                let msg = self.rogue_leave(&mut r);
                self.rogue = Some(r);
                Ok(msg)
            }
            t if t >= T_TERMINAL && ((t - T_TERMINAL) as usize) < ROGUE_TERMINALS.len() => {
                let idx = (t - T_TERMINAL) as usize;
                r.terminal = Some(idx);
                let label = ROGUE_TERMINALS[idx].1.to_string();
                r.say(format!("Borne : {}.", label.to_lowercase()));
                self.rogue = Some(r);
                Ok(label)
            }
            T_SITE => {
                let k = r
                    .floor()
                    .sites
                    .iter()
                    .position(|s| (s.x, s.y) == (r.x, r.y))
                    .ok_or("aucun site ici");
                let Ok(k) = k else {
                    self.rogue = Some(r);
                    return Err("aucun site ici");
                };
                let id = self.data.sites[r.floor().sites[k].def].id.clone();
                let Some(theme) = self.data.rogue_theme_index(site_theme(&id)) else {
                    self.rogue = Some(r);
                    return Err("lieu inconnu");
                };
                let seed = self.next_visit_seed(hash_coord(r.seed, k as i64, 91, 17));
                let mut child = self.new_run(theme, seed, r.party.clone());
                child.active = r.active;
                let name = child.place.clone();
                child.parent = Some(Box::new(r));
                self.rogue = Some(child);
                Ok(format!("VOUS DESCENDEZ : {}", name.to_uppercase()))
            }
            _ => {
                self.rogue = Some(r);
                Err("pas d'escalier ici")
            }
        }
    }

    /// Back to the ship with the loot (the squad stands on the entrance).
    fn rogue_leave(&mut self, r: &mut Rogue) -> String {
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
        let taken = self.boarding_outcome(r);
        let mut msg = format!("RETOUR AU VAISSEAU : {kg_total:.0} KG RAPPORTES");
        if let Some(t) = taken {
            msg = format!("{msg} - {t}");
        }
        if !left.is_empty() {
            msg = format!("{msg} (SOUTE PLEINE : {} LAISSE)", left.join(", "));
        }
        r.say(msg.clone());
        msg
    }

    /// Call everybody back to the ship from anywhere on a surface: the time of the walk back
    /// passes, the loot goes into the hold, the exploration ends.
    pub fn rogue_recall(&mut self) -> Result<String, &'static str> {
        let mut r = self.rogue.take().ok_or("pas d'exploration en cours")?;
        if r.over || !r.on_surface() || r.parent.is_some() {
            self.rogue = Some(r);
            return Err("on ne rappelle l'equipe que depuis la surface");
        }
        let (lx, ly) = r.floor().start;
        let far = (r.x - lx).abs().max((r.y - ly).abs());
        self.advance(f64::from(far) * r.turn_seconds, &ShipInput::default());
        let msg = self.rogue_leave(&mut r);
        self.rogue = Some(r);
        Ok(msg)
    }

    /// Close the exploration screen once it is over.
    pub fn rogue_close(&mut self) {
        if self.rogue.as_ref().is_some_and(|r| r.over) {
            self.rogue = None;
            // A squad lost aboard takes no ship.
            self.military.boarding = None;
        }
    }

    /// Walk toward (tx, ty) over the cells already seen, up to `max` steps, stopping when an
    /// awake monster comes into view or something happens.
    pub fn rogue_travel(&mut self, tx: i32, ty: i32, max: usize) -> Result<usize, &'static str> {
        let path = {
            let r = self.rogue.as_ref().ok_or("pas d'exploration en cours")?;
            if !r.inside(tx, ty) || !r.floor().seen[r.idx(tx, ty)] || !r.walkable(tx, ty) {
                return Err("destination inconnue");
            }
            shortest_path(r, (r.x, r.y), (tx, ty))
        };
        let path = path.ok_or("pas de chemin")?;
        let mut done = 0;
        for (nx, ny) in path.into_iter().take(max) {
            let (dx, dy, before) = {
                let r = self.rogue.as_ref().unwrap();
                (nx - r.x, ny - r.y, (r.log.len(), self.party_up(r).len()))
            };
            self.rogue_step(dx, dy)?;
            done += 1;
            let r = self.rogue.as_ref().unwrap();
            let vis = r.visible();
            let danger = r.floor().monsters.iter().any(|m| {
                m.hunts
                    && vis[r.idx(m.x, m.y)]
                    && (m.awake || (m.x - r.x).abs().max((m.y - r.y).abs()) <= 3)
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
                if seen[i] || !r.walkable(nx, ny) || !r.floor().seen[i] {
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

    /// Cells reachable from `start` over walkable tiles (sites and features included).
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
                    if !seen[i] && !matches!(f.tiles[i], T_WALL | T_WATER) {
                        seen[i] = true;
                        q.push_back((nx, ny));
                    }
                }
            }
        }
        seen
    }

    /// A dungeon run on `theme` with a fresh squad and a roomy hold.
    fn run_on(theme: &str, seed: u64) -> Sim {
        let mut s = new_sim(5, data());
        s.ship.cargo.cap_mass = 1.0e7;
        let t = s.data.rogue_theme_index(theme).unwrap();
        let party = s.rogue_squad();
        assert!(!party.is_empty());
        s.rogue_begin(t, seed, party);
        s
    }

    fn mon(s: &Sim, id: &str, x: i32, y: i32, hp: f64, awake: bool) -> Mon {
        let k = s.data.rogue_monster_index(id).unwrap();
        mon_from_def(&s.data, k, x, y, 1.0, awake, false).with_hp(hp)
    }

    impl Mon {
        fn with_hp(mut self, hp: f64) -> Mon {
            self.hp = hp;
            self
        }
    }

    /// A sim parked above a land cell with plants and animals: (sim, body, cx, cy).
    fn landing_site() -> (Sim, usize, usize, usize) {
        for seed in 1..60u64 {
            let mut s = new_sim(seed, data());
            for body in 0..s.bodies.len() {
                let Some(map) = s.planet_map(body) else {
                    continue;
                };
                // Chemistry: the squad lands unsuited, so the air must be breathable.
                if s.air_hazard_of(body).is_some_and(|a| !a.breathable()) {
                    continue;
                }
                for cy in (4..SURF_H).step_by(6) {
                    for cx in (0..SURF_W).step_by(9) {
                        if map.surface.is_sea(cx, cy) {
                            continue;
                        }
                        s.ship.pos = s.bodies[body].pos
                            - vector_core::Vec3::new(0.0, 0.0, s.bodies[body].radius * 3.0);
                        s.ship.vel = s.bodies[body].vel;
                        s.update_ref();
                        if s.rogue_land(body, cx, cy, vec![0], false).is_ok() {
                            let r = s.rogue.take().unwrap();
                            let plants = r.floor().items.iter().filter(|i| i.plant).count();
                            if plants > 5 && r.floor().monsters.len() > 3 {
                                s.ship.cargo.cap_mass = 1.0e7;
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
    fn every_floor_of_every_place_is_connected_deterministic_and_populated() {
        let d = data();
        for (t, th) in d.rogue_themes.iter().enumerate() {
            if th.id == "surface" || th.id == "vaisseau" || !th.social.is_empty() {
                continue;
            }
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
                        assert!(m.hp > 0.0 && !m.drops.is_empty());
                    }
                    let mut spots: Vec<_> = f.monsters.iter().map(|m| (m.x, m.y)).collect();
                    spots.sort_unstable();
                    spots.dedup();
                    assert_eq!(spots.len(), f.monsters.len(), "monsters stack");
                    for it in &f.items {
                        assert!(f.tiles[at(it.x, it.y)] != T_WALL && it.kg > 0.0 && !it.plant);
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
        let mut s = run_on("station", 4);
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
        let mut s = run_on("station", 4);
        s.crew.members[0].health = 1.0;
        let (px, py) = {
            let r = s.rogue.as_ref().unwrap();
            (r.x, r.y)
        };
        let (x, y) = [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .map(|d| (px + d.0, py + d.1))
            .find(|&(x, y)| s.rogue.as_ref().unwrap().walkable(x, y))
            .unwrap();
        let rat = mon(&s, "rat", x, y, 0.5, true);
        {
            let r = s.rogue.as_mut().unwrap();
            r.floors[0].monsters.clear();
            r.floors[0].monsters.push(rat);
        }
        s.rogue_step(x - px, y - py).expect("attacks");
        let r = s.rogue.as_ref().unwrap();
        assert_eq!((r.x, r.y), (px, py), "the squad stays put when it strikes");
        assert_eq!(r.kills, 1, "a rat does not survive a squad");
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
    fn a_full_hold_or_full_packs_leave_the_loot_behind() {
        let mut s = run_on("station", 4);
        s.ship.cargo.cap_mass = s.ship.cargo.mass; // no room in the hold
        let r = s.rogue.as_mut().unwrap();
        let (x, y) = (r.x + 1, r.y);
        let (x, y) = if r.walkable(x, y) {
            (x, y)
        } else {
            (r.x - 1, r.y)
        };
        r.floors[0].monsters.clear();
        r.floors[0].items.clear();
        r.floors[0].items.push(Item {
            x,
            y,
            res: 0,
            kg: 50.0,
            plant: false,
        });
        let (dx, dy) = (x - r.x, y - r.y);
        s.rogue_step(dx, dy).unwrap();
        let r = s.rogue.as_ref().unwrap();
        assert!(r.loot.is_empty(), "nothing taken");
        assert_eq!(r.floor().items.len(), 1, "the pile stays");
        // Roomy hold, but a pack holds only so much.
        let mut s = run_on("station", 4);
        let members = s.rogue.as_ref().unwrap().party.len();
        let r = s.rogue.as_mut().unwrap();
        r.floors[0].monsters.clear();
        r.floors[0].items.clear();
        let (x, y) = if r.walkable(r.x + 1, r.y) {
            (r.x + 1, r.y)
        } else {
            (r.x - 1, r.y)
        };
        r.floors[0].items.push(Item {
            x,
            y,
            res: 0,
            kg: 1.0e6,
            plant: false,
        });
        let (dx, dy) = (x - r.x, y - r.y);
        s.rogue_step(dx, dy).unwrap();
        let r = s.rogue.as_ref().unwrap();
        assert!(
            (r.loot_mass() - LOOT_PER_MEMBER * members as f64).abs() < 1e-6,
            "{} kg in {members} packs",
            r.loot_mass()
        );
    }

    #[test]
    fn monsters_hunt_the_squad_and_hurt_it_until_it_falls() {
        let mut s = run_on("base_pirate", 9);
        let (cx, cy) = {
            let r = s.rogue.as_ref().unwrap();
            [(2, 0), (-2, 0), (0, 2), (0, -2)]
                .into_iter()
                .map(|d| (r.x + d.0, r.y + d.1))
                .find(|&(x, y)| r.walkable(x, y) && r.line_clear(r.x, r.y, x, y))
                .expect("a free cell near the start")
        };
        let chef = mon(&s, "chef_pirate", cx, cy, 1.0e6, true);
        {
            let r = s.rogue.as_mut().unwrap();
            r.floors[0].monsters.clear();
            r.floors[0].monsters.push(chef);
        }
        let before: f32 = s.crew.members.iter().map(|p| p.health).sum();
        for _ in 0..4 {
            s.rogue_wait().unwrap();
        }
        let r = s.rogue.as_ref().unwrap();
        let m = &r.floor().monsters[0];
        assert!(
            (m.x - r.x).abs().max((m.y - r.y).abs()) <= 1,
            "it closed in"
        );
        let after: f32 = s.crew.members.iter().map(|p| p.health).sum();
        assert!(after < before, "and struck");
        s.rogue.as_mut().unwrap().loot.push((0, 10.0));
        for _ in 0..400 {
            if s.rogue.as_ref().unwrap().over {
                break;
            }
            let _ = s.rogue_wait();
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
        let mut s = run_on("laboratoire", 3);
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
        r.loot.clear();
        // Several caches: some of the table's goods always make it into the packs.
        let loot0 = r.loot_mass();
        s.rogue_feature(&mut r, "cache");
        let loot1 = r.loot_mass();
        assert!(loot1 > loot0);
        s.rogue_feature(&mut r, "vault");
        assert!(r.loot_mass() > loot1, "a vault pays more");
        let (ox, oy) = (r.x, r.y);
        let mut moved = false;
        for turn in 0..30 {
            r.turn = turn;
            s.rogue_feature(&mut r, "portal");
            moved |= (r.x, r.y) != (ox, oy);
        }
        assert!(moved, "portals teleport");
        assert!(r.walkable(r.x, r.y));
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
        let mut s = run_on("epave", 6);
        let food = s.data.resource_index("food").unwrap();
        s.rogue.as_mut().unwrap().loot.push((food, 12.0));
        assert!(
            s.rogue_stairs().unwrap().starts_with("RETOUR"),
            "out at the entrance"
        );
        assert!(s.rogue.as_ref().unwrap().over);
        let mut s = run_on("epave", 6);
        let (dx, dy) = {
            let r = s.rogue.as_ref().unwrap();
            let i = r.floor().tiles.iter().position(|t| *t == T_DOWN).unwrap();
            ((i % r.w) as i32, (i / r.w) as i32)
        };
        {
            let r = s.rogue.as_mut().unwrap();
            r.floors[0].monsters.clear();
            r.x = dx;
            r.y = dy;
        }
        assert_eq!(s.rogue_stairs().expect("down"), "Etage 2.");
        {
            let r = s.rogue.as_ref().unwrap();
            assert_eq!(r.cur, 1);
            assert_eq!(r.floors.len(), 2);
            assert_eq!(r.tile(r.x, r.y), T_UP);
        }
        s.rogue.as_mut().unwrap().floors[0].items.clear();
        assert_eq!(s.rogue_stairs().expect("up"), "Etage 1.");
        let r = s.rogue.as_ref().unwrap();
        assert_eq!((r.x, r.y), (dx, dy), "back on the stairs down");
        assert!(
            r.floors[0].items.is_empty(),
            "the floor above is as it was left"
        );
        let mut s = run_on("epave", 6);
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
    fn leaving_brings_the_loot_aboard() {
        let mut s = run_on("depot", 2);
        let steel = s.data.resource_index("steel").unwrap();
        let before = s.ship.cargo.kg[steel];
        s.rogue.as_mut().unwrap().loot.push((steel, 25.0));
        let msg = s.rogue_stairs().unwrap();
        assert!(msg.contains("25"), "{msg}");
        assert!((s.ship.cargo.kg[steel] - before - 25.0).abs() < 1e-9);
    }

    #[test]
    fn a_hazardous_place_hurts_and_a_clean_one_does_not() {
        let mut s = run_on("reacteur", 5);
        s.rogue.as_mut().unwrap().floors[0].monsters.clear();
        for p in &mut s.crew.members {
            p.health = 1.0;
        }
        for _ in 0..40 {
            s.rogue_wait().unwrap();
        }
        let hurt: f32 = s.crew.members.iter().map(|p| 1.0 - p.health).sum();
        assert!(hurt > 0.0, "radiation burns");
        let mut t = run_on("station", 5);
        t.rogue.as_mut().unwrap().floors[0].monsters.clear();
        for _ in 0..40 {
            t.rogue_wait().unwrap();
        }
        let safe: f32 = t.crew.members.iter().map(|p| 1.0 - p.health).sum();
        assert!(safe < hurt, "a clean station is gentler");
    }

    #[test]
    fn travel_walks_the_known_map_and_stops_when_danger_shows() {
        let mut s = run_on("station", 4);
        s.rogue.as_mut().unwrap().floors[0].monsters.clear();
        let far = |s: &Sim| {
            let r = s.rogue.as_ref().unwrap();
            let mut best = (r.x, r.y);
            let mut d = 0;
            for y in 0..r.h as i32 {
                for x in 0..r.w as i32 {
                    if r.floor().seen[r.idx(x, y)] && r.walkable(x, y) {
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
        let (tx, ty) = far(&s);
        assert!(s.rogue_travel(tx, ty, 50).expect("travels") >= 1);
        let (ux, uy) = {
            let r = s.rogue.as_ref().unwrap();
            (0..r.w as i32)
                .flat_map(|x| (0..r.h as i32).map(move |y| (x, y)))
                .find(|&(x, y)| !r.floor().seen[r.idx(x, y)])
                .unwrap()
        };
        assert!(
            s.rogue_travel(ux, uy, 5).is_err(),
            "unknown cells are refused"
        );
        // A hunter in view stops the walk quickly.
        let mut s = run_on("base_pirate", 9);
        let (px, py) = {
            let r = s.rogue.as_ref().unwrap();
            (0..r.w as i32)
                .flat_map(|x| (0..r.h as i32).map(move |y| (x, y)))
                .filter(|&(x, y)| r.tile(x, y) == T_FLOOR && r.floor().seen[r.idx(x, y)])
                .max_by_key(|&(x, y)| (x - r.x).abs().max((y - r.y).abs()))
                .unwrap()
        };
        let pirate = mon(&s, "pirate", px, py, 7.0, true);
        {
            let r = s.rogue.as_mut().unwrap();
            r.floors[0].monsters.clear();
            r.floors[0].monsters.push(pirate);
        }
        if let Ok(n) = s.rogue_travel(px, py, 30) {
            assert!(n <= 30);
        }
    }

    #[test]
    fn places_to_explore_follow_what_is_near_and_dimensions_dream() {
        let mut s = new_sim(5, data());
        let (st, _) = s.nearest_station().expect("station");
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        // A station is not yours to wander into: only from its dock.
        assert!(
            !s.rogue_targets()
                .iter()
                .any(|t| s.data.rogue_themes[t.theme].id == "station"),
            "not docked, no walk"
        );
        s.try_dock().unwrap();
        let targets = s.rogue_targets();
        assert!(
            targets
                .iter()
                .any(|t| s.data.rogue_themes[t.theme].id == "station"),
            "{targets:?}"
        );
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
            assert!(
                ts.iter().any(|x| d.rogue_themes[x.theme].id == id),
                "{kind:?} leads to {id}"
            );
        }
        // In a dimension every place turns to a dream (a landing stays a landing).
        let mut t = new_sim(5, Arc::clone(&d));
        t.multiverse.dimension = 1;
        let (st, _) = t.nearest_station().unwrap();
        t.spawn_near(st, t.bodies[st].radius + 200.0);
        assert!(t
            .rogue_targets()
            .iter()
            .filter(|x| x.kind == TargetKind::Place && d.rogue_themes[x.theme].social.is_empty())
            .all(|x| d.rogue_themes[x.theme].id == "reve"));
        // Entering: one at a time, and a place changes between visits.
        let mut s = new_sim(5, data());
        let (st, _) = s.nearest_station().unwrap();
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        let i = s
            .rogue_targets()
            .iter()
            .position(|t| {
                t.kind == TargetKind::Place && s.data.rogue_themes[t.theme].social.is_empty()
            })
            .unwrap_or_else(|| {
                // Nothing wild near the station: use a made-up place.
                s.bodies.push(crate::Body {
                    name: "Epave".into(),
                    kind: BodyKind::Anomaly(AnomalyKind::DerelictShip),
                    pos: s.ship.pos,
                    vel: vector_core::Vec3::ZERO,
                    radius: 100.0,
                    gm: 0.0,
                });
                s.rogue_targets().len() - 2
            });
        s.rogue_enter(i, vec![]).expect("goes");
        assert!(s.rogue_enter(i, vec![]).is_err(), "one at a time");
        let first = s.rogue.as_ref().unwrap().floors[0].tiles.clone();
        s.rogue.as_mut().unwrap().over = true;
        s.rogue_close();
        s.rogue_enter(i, vec![]).unwrap();
        assert_ne!(
            s.rogue.as_ref().unwrap().floors[0].tiles,
            first,
            "a new visit, a new layout"
        );
    }

    #[test]
    fn every_cell_is_made_of_a_material_of_its_own_kind() {
        let d = data();
        let kind_of = |t: u8| match t {
            T_WALL => "wall",
            T_WATER => "liquid",
            _ => "floor",
        };
        let mut seen_kinds = std::collections::HashSet::new();
        for theme in &d.rogue_themes {
            if theme.id == "surface" {
                continue;
            }
            let s = run_on(&theme.id, 9);
            let r = s.rogue.as_ref().unwrap();
            for f in &r.floors {
                assert_eq!(f.mats.len(), f.tiles.len(), "{}", theme.id);
                for (t, m) in f.tiles.iter().zip(&f.mats) {
                    let mat = &d.materials[usize::from(*m)];
                    assert_eq!(mat.kind, kind_of(*t), "{}: {}", theme.id, mat.id);
                    seen_kinds.insert(mat.id.clone());
                }
            }
        }
        // The built places do not all share one material: stone, alloy, wax, crystal...
        assert!(seen_kinds.len() >= 10, "{seen_kinds:?}");
        let (mut s, body, cx, cy) = landing_site();
        s.rogue_land(body, cx, cy, vec![0], false).unwrap();
        let r = s.rogue.as_ref().unwrap();
        let f = r.floor();
        let ground: std::collections::HashSet<usize> = f
            .tiles
            .iter()
            .zip(&f.mats)
            .filter(|(t, _)| **t != T_WALL && **t != T_WATER)
            .map(|(_, m)| usize::from(*m))
            .collect();
        assert!(ground.len() >= 2, "a landing is not one uniform ground");
        for (t, m) in f.tiles.iter().zip(&f.mats) {
            assert_eq!(d.materials[usize::from(*m)].kind, kind_of(*t));
        }
    }

    /// A land cell of a world with rivers: (sim, body, cx, cy) with the number of tiles of water of
    /// the floor it makes, for a cell on a river and for a dry cell far from any water.
    fn water_tiles(sim: &mut Sim, body: usize, cx: usize, cy: usize) -> Option<usize> {
        sim.rogue = None;
        sim.ship.pos =
            sim.bodies[body].pos - vector_core::Vec3::new(0.0, 0.0, sim.bodies[body].radius * 3.0);
        sim.ship.vel = sim.bodies[body].vel;
        sim.update_ref();
        sim.rogue_land(body, cx, cy, vec![0], false).ok()?;
        let r = sim.rogue.take()?;
        Some(r.floor().tiles.iter().filter(|&&t| t == T_WATER).count())
    }

    #[test]
    fn a_river_cell_has_a_river_on_its_grid_and_a_dry_cell_has_none() {
        for seed in 1..80u64 {
            let mut s = new_sim(seed, data());
            for body in 0..s.bodies.len() {
                let Some(map) = s.planet_map(body) else {
                    continue;
                };
                let h = &map.surface.hydro;
                let near_water = |x: usize, y: usize| {
                    (-3i32..=3).any(|dy| {
                        (-3i32..=3).any(|dx| {
                            let (xx, yy) = (
                                (x as i32 + dx).rem_euclid(SURF_W as i32) as usize,
                                (y as i32 + dy).clamp(0, SURF_H as i32 - 1) as usize,
                            );
                            map.surface.is_water(xx, yy)
                        })
                    })
                };
                let dry = (4..SURF_H - 4)
                    .flat_map(|y| (0..SURF_W).map(move |x| (x, y)))
                    .find(|&(x, y)| {
                        !near_water(x, y)
                            && map.surface.hum[Surface::idx(x, y)] < 0.4
                            && !map.surface.is_sea(x, y)
                    });
                let river = (4..SURF_H - 4)
                    .flat_map(|y| (0..SURF_W).map(move |x| (x, y)))
                    .find(|&(x, y)| {
                        h.river[Surface::idx(x, y)]
                            && map.surface.hum[Surface::idx(x, y)] < 0.45
                            && s.temp_now(&map, x, y) > 276.0
                            && !(-1i32..=1).any(|d| {
                                map.surface
                                    .is_sea((x as i32 + d).rem_euclid(SURF_W as i32) as usize, y)
                                    || map.surface.is_sea(x, (y as i32 + d) as usize)
                            })
                    });
                let (Some(dry), Some(river)) = (dry, river) else {
                    continue;
                };
                let Some(w_river) = water_tiles(&mut s, body, river.0, river.1) else {
                    continue;
                };
                let Some(w_dry) = water_tiles(&mut s, body, dry.0, dry.1) else {
                    continue;
                };
                assert_eq!(w_dry, 0, "no water, no river: a dry cell has none");
                assert!(
                    w_river >= 20,
                    "the river shows on the grid: {w_river} tiles"
                );
                return;
            }
        }
        panic!("no world with both a dry cell and a dry-climate river found");
    }

    #[test]
    fn a_hard_freeze_makes_ice_one_can_walk_on_and_a_thin_one_does_not() {
        let d = data();
        let surface = d.rogue_theme_index("surface").unwrap();
        let t = &d.rogue_themes[surface];
        let mut tiles = vec![T_FLOOR; MAP_W * MAP_H];
        tiles[7] = T_WATER;
        tiles[8] = T_WATER;
        // Just under the freezing point: still water that cannot be crossed.
        let mut mats = paint(&d, &tiles, MAP_W, t, 5, Some(("desert", 270.0)));
        let mut thin = tiles.clone();
        freeze_over(&d, &mut thin, &mut mats, 270.0);
        assert_eq!(thin[7], T_WATER);
        // Deep freeze: the same cells become ice floor.
        let mut mats = paint(&d, &tiles, MAP_W, t, 5, Some(("desert", 240.0)));
        let mut hard = tiles.clone();
        freeze_over(&d, &mut hard, &mut mats, 240.0);
        assert_eq!((hard[7], hard[8]), (T_FLOOR, T_FLOOR));
        let ice = d.material_index("banquise").unwrap() as u8;
        assert_eq!((mats[7], mats[8]), (ice, ice));
        assert_eq!(d.materials[usize::from(mats[7])].kind, "floor");
    }

    #[test]
    fn lakes_and_seas_are_not_landed_on() {
        let mut s = new_sim(3, data());
        for body in 0..s.bodies.len() {
            let Some(map) = s.planet_map(body) else {
                continue;
            };
            for y in 0..SURF_H {
                for x in 0..SURF_W {
                    if map.surface.is_lake(x, y) {
                        assert!(s.rogue_land(body, x, y, vec![0], false).is_err());
                        return;
                    }
                }
            }
        }
    }

    #[test]
    fn cold_makes_snow_and_ice_and_frozen_ponds() {
        let d = data();
        let surface = d.rogue_theme_index("surface").unwrap();
        let t = &d.rogue_themes[surface];
        let (w, h) = (MAP_W, MAP_H);
        let mut tiles = vec![T_FLOOR; w * h];
        tiles[5] = T_WATER;
        let count = |temp: f64, id: &str| {
            let m = paint(&d, &tiles, w, t, 33, Some(("desert", temp)));
            let k = d.material_index(id).unwrap() as u8;
            m.iter().filter(|&&x| x == k).count()
        };
        assert!(count(240.0, "neige") > 100, "cold ground gets snow");
        assert_eq!(count(300.0, "neige"), 0, "warm ground does not");
        assert_eq!(count(240.0, "eau_glacee"), 1);
        assert_eq!(count(300.0, "eau"), 1);
        assert_eq!(count(1500.0, "lave"), 1);
    }

    #[test]
    fn mud_costs_time_and_ice_saves_it() {
        let (mut s, body, cx, cy) = landing_site();
        s.rogue_land(body, cx, cy, vec![0], false).unwrap();
        let spent = |s: &mut Sim, id: &str| {
            let k = s.data.material_index(id).unwrap() as u8;
            let r = s.rogue.as_mut().unwrap();
            let (x, y) = (r.x, r.y);
            let i = r.idx(x, y);
            r.floors[0].mats[i] = k;
            let t0 = s.time;
            s.rogue_wait().unwrap();
            s.time - t0
        };
        let mud = spent(&mut s, "boue");
        let ice = spent(&mut s, "glace");
        let plain = spent(&mut s, "herbe");
        assert!(
            mud > plain * 1.8 && ice < plain * 0.9,
            "{mud} {plain} {ice}"
        );
    }

    #[test]
    fn a_run_survives_a_save() {
        let mut s = run_on("ruines", 8);
        s.rogue_wait().unwrap();
        let bytes = s.save_bytes().unwrap();
        let t = Sim::load_bytes(&bytes, Arc::clone(&s.data)).unwrap();
        let (a, b) = (s.rogue.as_ref().unwrap(), t.rogue.as_ref().unwrap());
        assert_eq!(a.turn, b.turn);
        assert_eq!(a.floors[0].tiles, b.floors[0].tiles);
        assert_eq!((a.x, a.y), (b.x, b.y));
        assert_eq!(a.floors[0].monsters, b.floors[0].monsters);
    }

    // ---- surfaces ---------------------------------------------------------------------------

    #[test]
    fn the_same_cell_always_gives_the_same_place_and_the_party_lands_together() {
        let (mut s, body, cx, cy) = landing_site();
        s.rogue_land(body, cx, cy, vec![0], false).unwrap();
        let a = s.rogue.take().unwrap();
        s.rogue_land(body, cx, cy, vec![0], false).unwrap();
        let b = s.rogue.take().unwrap();
        assert_eq!(a.floors[0].tiles, b.floors[0].tiles);
        assert_eq!(a.floors[0].items, b.floors[0].items);
        assert_eq!(
            a.floors[0]
                .monsters
                .iter()
                .map(|m| (m.x, m.y))
                .collect::<Vec<_>>(),
            b.floors[0]
                .monsters
                .iter()
                .map(|m| (m.x, m.y))
                .collect::<Vec<_>>()
        );
        assert_eq!((a.x, a.y), (MAP_W as i32 / 2, MAP_H as i32 / 2));
        assert_eq!(a.tile(a.x, a.y), T_UP);
        assert_eq!(a.turn_seconds, SURFACE_TURN_SECONDS);
        assert!(a.env.is_some() && a.on_surface());
        s.rogue_land(body, (cx + 1) % 64, cy, vec![0], false).ok();
        if let Some(c) = s.rogue.take() {
            assert_ne!(a.seed, c.seed, "another cell, another place");
        }
    }

    #[test]
    fn rocks_and_water_block_time_passes_and_the_rover_goes_twice_as_far() {
        let (mut s, body, cx, cy) = landing_site();
        s.rogue_land(body, cx, cy, vec![0], false).unwrap();
        let (lx, ly) = {
            let r = s.rogue.as_ref().unwrap();
            (r.x, r.y)
        };
        {
            let r = s.rogue.as_mut().unwrap();
            r.floors[0].monsters.clear();
            let i = r.idx(lx + 1, ly);
            r.floors[0].tiles[i] = T_WALL;
            let i = r.idx(lx, ly + 1);
            r.floors[0].tiles[i] = T_WATER;
        }
        let t0 = s.time;
        assert!(s.rogue_step(1, 0).is_err(), "a rock blocks");
        assert!(s.rogue_step(0, 1).is_err(), "water blocks");
        assert_eq!(s.time, t0, "a refused move costs nothing");
        s.rogue_step(-1, 0).unwrap();
        assert_eq!(s.rogue.as_ref().unwrap().x, lx - 1);
        assert!(s.time > t0, "a step takes a turn of the clock");
        // The rover goes twice as far.
        s.rogue = None;
        s.rogue_land(body, cx, cy, vec![0], true).unwrap();
        {
            let r = s.rogue.as_mut().unwrap();
            r.floors[0].monsters.clear();
            r.floors[0].items.clear();
            for dx in 1..=2 {
                let i = r.idx(lx + dx, ly);
                r.floors[0].tiles[i] = T_FLOOR;
            }
        }
        s.rogue_step(1, 0).unwrap();
        assert_eq!(
            s.rogue.as_ref().unwrap().x,
            lx + 2,
            "two tiles with the rover"
        );
    }

    #[test]
    fn plants_are_gathered_on_the_way_up_to_the_packs_limit_and_recall_fills_the_hold() {
        let (mut s, body, cx, cy) = landing_site();
        s.rogue_land(body, cx, cy, vec![0], false).unwrap();
        let (lx, ly) = {
            let r = s.rogue.as_mut().unwrap();
            r.floors[0].monsters.clear();
            r.floors[0].items.clear();
            let (x, y) = (r.x, r.y);
            let i = r.idx(x + 1, y);
            r.floors[0].tiles[i] = T_FLOOR;
            (x, y)
        };
        let food = s.data.resource_index("food").unwrap();
        s.rogue.as_mut().unwrap().floors[0].items.push(Item {
            x: lx + 1,
            y: ly,
            res: food,
            kg: 5.0,
            plant: true,
        });
        s.rogue_step(1, 0).unwrap();
        let r = s.rogue.as_ref().unwrap();
        assert!((r.loot_mass() - 5.0).abs() < 1e-9, "{}", r.loot_mass());
        // The pack never holds more than the capacity.
        let mut rr = s.rogue.take().unwrap();
        s.rogue_gain(&mut rr, food, 1.0e6);
        assert!(rr.loot_mass() <= rr.carry_cap(1) + 1e-9);
        s.rogue = Some(rr);
        let carried = s.rogue.as_ref().unwrap().loot_mass();
        let (before_mass, before_t) = (s.ship.cargo.mass, s.time);
        let msg = s.rogue_recall().unwrap();
        assert!(msg.starts_with("RETOUR"), "{msg}");
        assert!(s.rogue.as_ref().unwrap().over && s.rogue.as_ref().unwrap().loot.is_empty());
        assert!(
            s.ship.cargo.mass >= before_mass + carried - 1e-6,
            "the loot is in the hold"
        );
        assert!(s.time > before_t, "the walk back takes time");
    }

    #[test]
    fn animals_are_fought_for_their_yields_and_predators_hurt_organs() {
        let (mut s, body, cx, cy) = landing_site();
        s.rogue_land(body, cx, cy, vec![0], false).unwrap();
        let (lx, ly) = {
            let r = s.rogue.as_ref().unwrap();
            (r.x, r.y)
        };
        // A weak animal on the east: bumping kills it and it drops its yields.
        let weak = (0..s.data.species.len())
            .find(|&i| {
                !s.data.species[i].is_producer()
                    && !s.data.species[i].yields.is_empty()
                    && s.data.species[i].danger == 0.0
            })
            .unwrap();
        let mut a = mon_from_species(&s.data, weak, lx + 1, ly);
        a.hp = 0.5;
        {
            let r = s.rogue.as_mut().unwrap();
            r.floors[0].monsters.clear();
            r.floors[0].items.clear();
            let i = r.idx(lx + 1, ly);
            r.floors[0].tiles[i] = T_FLOOR;
            r.floors[0].monsters.push(a);
        }
        s.rogue_step(1, 0).unwrap();
        let r = s.rogue.as_ref().unwrap();
        assert_eq!(r.kills, 1);
        assert!(!r.floor().items.is_empty(), "its yields lie where it fell");
        // A terrible predator wounds a member and an organ.
        let fierce = (0..s.data.species.len())
            .find(|&i| {
                !s.data.species[i].is_producer()
                    && s.data.species[i].danger > 0.0
                    && !s.data.species[i].domestic
            })
            .expect("a dangerous species");
        let victim = s.rogue.as_ref().unwrap().party[0];
        let before: Vec<f32> = s.crew.members[victim].organs.clone();
        let h0 = s.crew.members[victim].health;
        let mut p = mon_from_species(&s.data, fierce, lx + 1, ly + 1);
        p.hp = 1.0e6;
        p.awake = true;
        {
            let r = s.rogue.as_mut().unwrap();
            r.floors[0].monsters.clear();
            r.floors[0].monsters.push(p);
        }
        for _ in 0..40 {
            if s.rogue.as_ref().unwrap().over {
                break;
            }
            let _ = s.rogue_wait();
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
        assert!(s
            .rogue
            .as_ref()
            .unwrap()
            .log
            .iter()
            .any(|l| l.contains("blesse")));
    }

    #[test]
    fn sites_are_far_from_the_landing_searched_once_and_lead_down() {
        let (mut s, body, cx, cy) = landing_site();
        s.rogue_land(body, cx, cy, vec![0], false).unwrap();
        let (lx, ly) = {
            let r = s.rogue.as_ref().unwrap();
            (r.x, r.y)
        };
        {
            let r = s.rogue.as_ref().unwrap();
            assert!(r.floor().sites.len() <= 4);
            assert!(r
                .floor()
                .sites
                .iter()
                .all(|t| (t.x - lx).abs() + (t.y - ly).abs() >= 10));
        }
        let loot = s
            .data
            .sites
            .iter()
            .position(|d| !d.loot.is_empty() && !d.heal && d.danger < 0.1)
            .unwrap();
        let spring = s.data.sites.iter().position(|d| d.heal).unwrap();
        {
            let r = s.rogue.as_mut().unwrap();
            r.floors[0].monsters.clear();
            r.floors[0].items.clear();
            r.floors[0].sites.clear();
            for (k, dx) in [(loot, 1), (spring, -1)] {
                let i = r.idx(lx + dx, ly);
                r.floors[0].tiles[i] = T_SITE;
                r.floors[0].sites.push(SiteInst {
                    def: k,
                    x: lx + dx,
                    y: ly,
                    used: false,
                });
            }
        }
        s.rogue_step(1, 0).unwrap();
        let r = s.rogue.as_ref().unwrap();
        assert!(r.floor().sites[0].used);
        assert!(r.loot_mass() > 0.0, "the site gave something");
        let after = r.loot_mass();
        s.rogue_step(-1, 0).unwrap();
        s.rogue_step(1, 0).unwrap();
        assert_eq!(
            s.rogue.as_ref().unwrap().loot_mass(),
            after,
            "a site is searched once"
        );
        // The spring mends wounds.
        let me = s.rogue.as_ref().unwrap().party[0];
        s.crew.members[me].health = 0.3;
        let lungs = s.data.organ_index("poumons").unwrap();
        s.crew.members[me].organs[lungs] = 0.4;
        s.rogue_step(-1, 0).unwrap();
        s.rogue_step(-1, 0).unwrap();
        assert!(s.crew.members[me].health > 0.5);
        assert!(s.crew.members[me].organs[lungs] > 0.55);
        // A site is also a way down into a dungeon, and the stairs lead back up with the loot.
        {
            let r = s.rogue.as_mut().unwrap();
            let k = r.floor().sites.iter().position(|t| t.def == loot).unwrap();
            let (x, y) = (r.floor().sites[k].x, r.floor().sites[k].y);
            r.x = x;
            r.y = y;
        }
        let msg = s.rogue_stairs().expect("descends");
        assert!(msg.starts_with("VOUS DESCENDEZ"), "{msg}");
        {
            let r = s.rogue.as_ref().unwrap();
            assert!(r.parent.is_some() && !r.on_surface());
            assert_eq!(r.turn_seconds, TURN_SECONDS);
        }
        s.rogue.as_mut().unwrap().loot.push((0, 7.0));
        let msg = s.rogue_stairs().expect("climbs back");
        assert_eq!(msg, "RETOUR A LA SURFACE");
        let r = s.rogue.as_ref().unwrap();
        assert!(r.on_surface() && r.parent.is_none());
        assert!(
            r.loot.iter().any(|l| l.0 == 0 && l.1 >= 7.0),
            "what was found comes up"
        );
    }

    #[test]
    fn a_kill_in_the_field_changes_the_standing_with_the_territory() {
        let (mut s, body, cx, cy) = landing_site();
        s.faction = Some(0);
        s.rogue = None;
        s.rogue_land(body, cx, cy, vec![0], false).unwrap();
        let (lx, ly) = {
            let r = s.rogue.as_ref().unwrap();
            (r.x, r.y)
        };
        let prey = (0..s.data.species.len())
            .find(|&i| !s.data.species[i].is_producer())
            .unwrap();
        let mut a = mon_from_species(&s.data, prey, lx + 1, ly);
        a.hp = 0.001;
        a.hunts = true;
        let mut r = s.rogue.take().unwrap();
        r.floors[0].monsters.clear();
        r.floors[0].monsters.push(a);
        let rep = s.reputation_with(Some(0));
        s.rogue_attack(&mut r, 0);
        assert_eq!(r.kills, 1);
        assert!(s.reputation_with(Some(0)) > rep, "the kill was thanked");
        s.rogue = Some(r);
    }

    #[test]
    fn weapons_hit_harder_armour_stops_damage_and_tools_find_more() {
        let (mut s, body, cx, cy) = landing_site();
        let d = Arc::clone(&s.data);
        let rifle = d.gear_index("fusil_plasma").unwrap();
        let suit = d.gear_index("exosquelette").unwrap();
        let pick = d.gear_index("pioche").unwrap();
        let food = d.resource_index("food").unwrap();
        // A blow: the damage done to the same animal depends on the weapon.
        let blow = |s: &mut Sim, armed: bool| -> f64 {
            s.rogue = None;
            s.rogue_land(body, cx, cy, vec![0], false).unwrap();
            s.crew.members[0].gear = vec![None; 3];
            if armed {
                s.crew.members[0].gear[0] = Some(rifle);
            }
            let (lx, ly) = {
                let r = s.rogue.as_ref().unwrap();
                (r.x, r.y)
            };
            let prey = (0..s.data.species.len())
                .find(|&i| !s.data.species[i].is_producer())
                .unwrap();
            let mut a = mon_from_species(&s.data, prey, lx + 1, ly);
            a.hp = 1000.0;
            let mut r = s.rogue.take().unwrap();
            r.floors[0].monsters.clear();
            r.floors[0].monsters.push(a);
            s.rogue_attack(&mut r, 0);
            let done = 1000.0 - r.floors[0].monsters[0].hp;
            s.rogue = Some(r);
            done
        };
        let bare = blow(&mut s, false);
        let armed = blow(&mut s, true);
        // A shot is heard: a sleeper six cells away wakes up for the rifle, not for the knife.
        let sleeper_wakes = |s: &mut Sim, weapon: Option<usize>| -> bool {
            s.rogue = None;
            s.rogue_land(body, cx, cy, vec![0], false).unwrap();
            s.crew.members[0].gear = vec![None; 3];
            s.crew.members[0].gear[0] = weapon;
            let (lx, ly) = {
                let r = s.rogue.as_ref().unwrap();
                (r.x, r.y)
            };
            let prey = (0..s.data.species.len())
                .find(|&i| !s.data.species[i].is_producer())
                .unwrap();
            let mut a = mon_from_species(&s.data, prey, lx + 1, ly);
            a.hp = 1000.0;
            let mut far = mon_from_species(&s.data, prey, lx + 6, ly);
            far.awake = false;
            let mut r = s.rogue.take().unwrap();
            r.floors[0].monsters.clear();
            r.floors[0].monsters.push(a);
            r.floors[0].monsters.push(far);
            s.rogue_attack(&mut r, 0);
            let woke = r.floors[0].monsters[1].awake;
            s.rogue = Some(r);
            woke
        };
        let knife = d.gear_index("couteau").unwrap();
        let needle = d.gear_index("fusil").unwrap();
        assert!(!sleeper_wakes(&mut s, Some(knife)), "a knife is quiet");
        assert!(
            sleeper_wakes(&mut s, Some(needle)),
            "a needle rifle is loud"
        );
        assert!(
            armed > bare + d.gear[rifle].attack - 1e-9,
            "{armed} vs {bare}"
        );
        // A wound: the same hit costs less health in armour.
        let wound = |s: &mut Sim, armoured: bool| -> f32 {
            s.rogue = None;
            s.rogue_land(body, cx, cy, vec![0], false).unwrap();
            s.crew.members[0].gear = vec![None; 3];
            s.crew.members[0].health = 1.0;
            if armoured {
                s.crew.members[0].gear[1] = Some(suit);
            }
            let mut r = s.rogue.take().unwrap();
            s.rogue_hurt(&mut r, 0, 0.4, "test");
            s.rogue = Some(r);
            1.0 - s.crew.members[0].health
        };
        let naked = wound(&mut s, false);
        let dressed = wound(&mut s, true);
        assert!((naked - 0.4).abs() < 1e-3);
        assert!(dressed < naked * 0.6, "{dressed} vs {naked}");
        // A harvest: the tool adds its share.
        let harvest = |s: &mut Sim, tool: bool| -> f64 {
            s.rogue = None;
            s.rogue_land(body, cx, cy, vec![0], false).unwrap();
            s.crew.members[0].gear = vec![None; 3];
            if tool {
                s.crew.members[0].gear[2] = Some(pick);
            }
            let (lx, ly) = {
                let r = s.rogue.as_mut().unwrap();
                r.floors[0].monsters.clear();
                r.floors[0].items.clear();
                r.floors[0].sites.clear();
                (r.x, r.y)
            };
            s.rogue.as_mut().unwrap().floors[0].items.push(Item {
                x: lx + 1,
                y: ly,
                res: food,
                kg: 2.0,
                plant: true,
            });
            let mut r = s.rogue.take().unwrap();
            let i = r.idx(lx + 1, ly);
            r.floors[0].tiles[i] = T_FLOOR;
            s.rogue = Some(r);
            s.rogue_step(1, 0).unwrap();
            s.rogue.as_ref().unwrap().loot_mass()
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
    fn leadership_changes_among_the_standing_and_a_fallen_squad_ends_the_run() {
        let (mut s, body, cx, cy) = landing_site();
        let d = Arc::clone(&s.data);
        let sib = {
            let me = s.crew.members[0].kin;
            let sib = crate::genealogy::siblings(&s.crew.families, me)[0];
            crate::crew::person_from_kin(&d, &s.crew.families, sib, 99, None, None)
        };
        s.crew.hire(&d, sib, 2, 0.0);
        s.rogue_land(body, cx, cy, vec![0, 1], false).unwrap();
        assert_eq!(s.rogue.as_ref().unwrap().active, 0);
        s.rogue_switch();
        assert_eq!(
            s.rogue.as_ref().unwrap().active,
            1,
            "the AI moves to the other one"
        );
        s.crew.members[0].health = 0.0;
        s.rogue_wait().unwrap();
        assert!(
            s.rogue.as_ref().unwrap().active < 1,
            "only one is left standing"
        );
        s.crew.members[1].health = 0.0;
        s.rogue_wait().unwrap();
        assert!(
            s.rogue.as_ref().unwrap().over,
            "everybody down ends the expedition"
        );
    }

    #[test]
    fn thin_air_and_extreme_temperatures_hurt_unless_a_suit_is_worn() {
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
        let (mut s, body, cx, cy) = landing_site();
        let turns = |s: &mut Sim, suit: bool| -> f32 {
            s.rogue = None;
            s.rogue_land(body, cx, cy, vec![0], false).unwrap();
            s.rogue.as_mut().unwrap().env = Some((0.0, 290.0));
            s.rogue.as_mut().unwrap().floors[0].monsters.clear();
            s.crew.members[0].gear = vec![None; 3];
            if suit {
                s.crew.members[0].gear[1] = s.data.gear_index("exosquelette");
            }
            s.crew.members[0].health = 1.0;
            for _ in 0..5 {
                s.rogue_wait().unwrap();
            }
            s.crew.members[0].health
        };
        let bare = turns(&mut s, false);
        assert!(bare < 0.95, "{bare}");
        let suited = turns(&mut s, true);
        assert!(suited > bare + 0.05, "{suited} vs {bare}");
    }

    #[test]
    fn a_drone_picks_things_up_on_its_own_for_a_while() {
        let (mut s, body, cx, cy) = landing_site();
        s.rogue_land(body, cx, cy, vec![0], false).unwrap();
        // The squad is suited: the air of a world is rarely breathable.
        s.crew.members[0].gear[1] = s.data.gear_index("exosquelette");
        let r = s.data.resource_index("robotics").unwrap();
        s.ship.cargo.kg[r] = 0.0;
        assert!(s.rogue_deploy_drone().is_err(), "no robotics, no drone");
        s.ship.cargo.kg[r] = 3.0;
        let food = s.data.resource_index("food").unwrap();
        let (x, y) = {
            let rr = s.rogue.as_mut().unwrap();
            rr.floors[0].monsters.clear();
            (rr.x, rr.y)
        };
        for k in 0..12 {
            s.rogue.as_mut().unwrap().floors[0].items.push(Item {
                x: x + k % 3 - 1,
                y: y + 1 + k / 3 % 2,
                res: food,
                kg: 2.0,
                plant: true,
            });
        }
        let before = s.rogue.as_ref().unwrap().floor().items.len();
        s.rogue_deploy_drone().expect("launched");
        assert!(s.rogue.as_ref().unwrap().drone_turns >= DRONE_TURNS - 1);
        assert!(s.ship.cargo.kg[r] < 3.0, "a unit of robotics is spent");
        assert!(s.rogue_deploy_drone().is_err(), "one at a time");
        for _ in 0..8 {
            s.rogue_wait().unwrap();
        }
        let rr = s.rogue.as_ref().unwrap();
        assert!(rr.floor().items.len() + 5 <= before, "it picked up");
        assert!(rr.loot_mass() > 0.0);
        for _ in 0..40 {
            s.rogue_wait().unwrap();
        }
        assert_eq!(
            s.rogue.as_ref().unwrap().drone_turns,
            0,
            "its batteries ran out"
        );
    }

    // ---- social places ----------------------------------------------------------------------

    #[test]
    fn social_places_hold_every_terminal_once_and_people_not_monsters() {
        let d = data();
        for id in ["station", "colonie"] {
            let t = d.rogue_theme_index(id).unwrap();
            let th = &d.rogue_themes[t];
            let funcs: Vec<usize> = th
                .social
                .iter()
                .map(|f| ROGUE_TERMINALS.iter().position(|x| x.0 == f).unwrap())
                .collect();
            for seed in 0..8u64 {
                let f = generate_social(&d, t, seed, &funcs, &[]);
                assert_eq!(
                    f.tiles,
                    generate_social(&d, t, seed, &funcs, &[]).tiles,
                    "deterministic"
                );
                let (w, h) = th.size;
                let reach = reachable(&f, w, h, f.start);
                for (i, tile) in f.tiles.iter().enumerate() {
                    if *tile != T_WALL {
                        assert!(reach[i], "{id}: cell {i} cut off");
                    }
                }
                for k in &funcs {
                    let n = f
                        .tiles
                        .iter()
                        .filter(|t| **t == T_TERMINAL + *k as u8)
                        .count();
                    assert_eq!(
                        n, 1,
                        "{id}: terminal {} appears once",
                        ROGUE_TERMINALS[*k].0
                    );
                }
                assert_eq!(f.tiles[f.start.1 as usize * w + f.start.0 as usize], T_UP);
                assert!(f.monsters.len() >= 6, "{id}: people about");
                assert!(f
                    .monsters
                    .iter()
                    .all(|m| m.friendly && !m.hunts && m.attack == 0.0));
            }
        }
    }

    #[test]
    fn the_ship_is_drawn_from_its_modules_and_its_crew_walks_in_it() {
        let mut s = new_sim(5, data());
        // The crew: the squad goes out, the others are in the rooms.
        let d = Arc::clone(&s.data);
        for k in 0..3 {
            let p = crate::crew::generate_person(&d, 900 + k, None);
            s.crew.hire(&d, p, 1, 0.0);
        }
        let funcs = s.ship_terminals();
        let names: Vec<&str> = funcs.iter().map(|k| ROGUE_TERMINALS[*k].0).collect();
        for needed in ["pont", "equipage", "temps"] {
            assert!(names.contains(&needed), "{needed} in {names:?}");
        }
        assert!(names.contains(&"soute"), "the ship has a hold: {names:?}");
        let mut dup = names.clone();
        dup.sort_unstable();
        dup.dedup();
        assert_eq!(dup.len(), names.len(), "each room once");
        let i = s
            .rogue_targets()
            .iter()
            .position(|t| t.kind == TargetKind::Ship)
            .expect("the ship can always be walked");
        s.rogue_enter(i, vec![0]).expect("goes");
        let r = s.rogue.as_ref().unwrap();
        assert!(r.social && r.party == vec![0]);
        let aboard: Vec<u32> = r.floor().monsters.iter().filter_map(|m| m.crew).collect();
        assert_eq!(
            aboard.len(),
            s.crew.members.len() - 1,
            "everybody else is aboard"
        );
        assert!(!aboard.contains(&s.crew.members[0].id));
        for k in &funcs {
            assert_eq!(
                r.floor()
                    .tiles
                    .iter()
                    .filter(|t| **t == T_TERMINAL + *k as u8)
                    .count(),
                1
            );
        }
        // Without a medbay the ship has no infirmary; with one it does.
        let medbay = s
            .data
            .modules
            .iter()
            .position(|m| m.medical > 0.0)
            .expect("a medical module");
        let slot = s
            .ship
            .design
            .slots
            .iter()
            .position(|m| m.is_none())
            .or_else(|| s.ship.design.slots.iter().position(|m| m.is_some()))
            .unwrap();
        s.ship.design.slots[slot] = Some(medbay);
        let with = s.ship_terminals();
        assert!(with.iter().any(|k| ROGUE_TERMINALS[*k].0 == "infirmerie"));
    }

    #[test]
    fn people_talk_crew_tell_how_they_feel_and_terminals_ask_for_their_screen() {
        let mut s = new_sim(5, data());
        let d = Arc::clone(&s.data);
        let mut p = crate::crew::generate_person(&d, 901, None);
        p.hunger = 0.95;
        let k = s.crew.hire(&d, p, 1, 0.0);
        let hungry = s.crew.members[k].id;
        let i = s
            .rogue_targets()
            .iter()
            .position(|t| t.kind == TargetKind::Ship)
            .unwrap();
        s.rogue_enter(i, vec![0]).unwrap();
        // Put the hungry one right next to the squad and bump into them.
        {
            let r = s.rogue.as_mut().unwrap();
            let (x, y) = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .map(|d| (r.x + d.0, r.y + d.1))
                .find(|&(x, y)| r.walkable(x, y))
                .unwrap();
            let m = r.floors[0]
                .monsters
                .iter_mut()
                .find(|m| m.crew == Some(hungry))
                .unwrap();
            m.x = x;
            m.y = y;
        }
        let (px, py) = {
            let r = s.rogue.as_ref().unwrap();
            (r.x, r.y)
        };
        let (mx, my) = {
            let r = s.rogue.as_ref().unwrap();
            let m = r
                .floor()
                .monsters
                .iter()
                .find(|m| m.crew == Some(hungry))
                .unwrap();
            (m.x, m.y)
        };
        s.rogue_step(mx - px, my - py).unwrap();
        let r = s.rogue.as_ref().unwrap();
        assert_eq!((r.x, r.y), (px, py), "talking does not move the squad");
        assert!(
            r.log.iter().any(|l| l.contains("faim")),
            "the hungry one says so: {:?}",
            r.log.last()
        );
        // A terminal under the squad sets the request for its screen.
        let k = ROGUE_TERMINALS.iter().position(|t| t.0 == "pont").unwrap();
        {
            let r = s.rogue.as_mut().unwrap();
            let (x, y) = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .map(|d| (r.x + d.0, r.y + d.1))
                .find(|&(x, y)| r.walkable(x, y))
                .unwrap();
            r.x = x;
            r.y = y;
            let i = r.idx(x, y);
            r.floors[0].tiles[i] = T_TERMINAL + k as u8;
        }
        let label = s.rogue_stairs().unwrap();
        assert_eq!(label, "PONT DE COMMANDE");
        assert_eq!(s.rogue.as_ref().unwrap().terminal, Some(k));
        // The squad leaves by the airlock like from anywhere.
        {
            let r = s.rogue.as_mut().unwrap();
            let (x, y) = r.floor().start;
            r.x = x;
            r.y = y;
        }
        assert!(s.rogue_stairs().unwrap().starts_with("RETOUR"));
    }

    #[test]
    fn a_station_walk_is_social_and_its_passers_by_relay_rumours() {
        let mut s = new_sim(5, data());
        let (st, _) = s.nearest_station().unwrap();
        s.spawn_near(st, s.bodies[st].radius + 200.0);
        s.try_dock().unwrap();
        let i = s
            .rogue_targets()
            .iter()
            .position(|t| s.data.rogue_themes[t.theme].id == "station")
            .unwrap();
        s.rogue_enter(i, vec![]).unwrap();
        let r = s.rogue.as_ref().unwrap();
        assert!(r.social);
        // Every passer-by, bumped into many times at many dates, says something; some say a rumour.
        let mut said = std::collections::BTreeSet::new();
        for turn in 0..400 {
            s.time = f64::from(turn) * 43_200.0;
            let mut r = s.rogue.take().unwrap();
            r.turn = turn;
            s.rogue_talk(&mut r, 0);
            said.insert(r.log.last().unwrap().clone());
            s.rogue = Some(r);
        }
        assert!(said.len() >= 4, "{} different things said", said.len());
        assert!(
            said.iter().any(|l| l.contains("arrive dans")),
            "some rumours of the exchange are relayed: {said:?}"
        );
    }

    #[test]
    fn a_tame_animal_of_a_surface_can_be_caught_for_the_pens() {
        let (mut s, body, cx, cy) = landing_site();
        s.rogue_land(body, cx, cy, vec![0], false).unwrap();
        let tame = s.domestic_species()[0];
        let wild = (0..s.data.species.len())
            .find(|&i| {
                !s.data.species[i].is_producer()
                    && !s.data.species[i].domestic
                    && s.data.species[i].danger == 0.0
            })
            .expect("a wild harmless species");
        let (lx, ly) = {
            let r = s.rogue.as_mut().unwrap();
            r.floors[0].monsters.clear();
            (r.x, r.y)
        };
        assert!(s.rogue_capture().is_err(), "nothing to catch");
        let m = mon_from_species(&s.data, wild, lx + 1, ly);
        s.rogue.as_mut().unwrap().floors[0].monsters.push(m);
        assert!(s.rogue_capture().is_err(), "a wild beast will not be tamed");
        s.rogue.as_mut().unwrap().floors[0].monsters.clear();
        let m = mon_from_species(&s.data, tame, lx + 1, ly);
        s.rogue.as_mut().unwrap().floors[0].monsters.push(m);
        let herd = s.herd.len();
        s.rogue_capture().expect("caught");
        assert_eq!(s.herd.len(), herd + 1);
        assert_eq!(s.herd.last().unwrap().species, tame);
        assert!(s.rogue.as_ref().unwrap().floor().monsters.is_empty());
        // Full pens refuse.
        let m = mon_from_species(&s.data, tame, lx + 1, ly);
        s.rogue.as_mut().unwrap().floors[0].monsters.push(m);
        while s.herd.len() < crate::livestock::HERD_MAX {
            let a = s.herd[0].clone();
            s.herd.push(a);
        }
        assert!(s.rogue_capture().is_err());
    }

    // ---- matter: digging, building, sliding, burning ----------------------------------------

    fn mat_id(s: &Sim, id: &str) -> u8 {
        s.data.material_index(id).unwrap() as u8
    }

    /// A landing swept clean: an open field of bare regolith around the squad, nobody else.
    fn bare_field() -> Sim {
        let (mut s, body, cx, cy) = landing_site();
        s.rogue_land(body, cx, cy, vec![0], false).unwrap();
        s.crew.members[0].gear = vec![None; 3];
        let reg = mat_id(&s, "regolithe");
        let mut r = s.rogue.take().unwrap();
        let (w, h) = (r.w, r.h);
        let f = &mut r.floors[0];
        f.monsters.clear();
        f.items.clear();
        f.sites.clear();
        for i in 0..w * h {
            if f.tiles[i] != T_UP {
                f.tiles[i] = T_FLOOR;
                f.mats[i] = reg;
            }
        }
        r.env = Some((1.0, 288.0));
        s.rogue = Some(r);
        s
    }

    fn put(s: &mut Sim, x: i32, y: i32, tile: u8, mat: &str) {
        let k = mat_id(s, mat);
        let r = s.rogue.as_mut().unwrap();
        let i = r.idx(x, y);
        r.floors[0].tiles[i] = tile;
        r.floors[0].mats[i] = k;
    }

    #[test]
    fn digging_gives_what_the_matter_holds_and_lays_the_floor_bare() {
        let mut s = bare_field();
        let (x, y) = {
            let r = s.rogue.as_ref().unwrap();
            (r.x, r.y)
        };
        // Eight dug cells of rock: iron and silica come out, each cell becomes bedrock floor.
        for dy in -1..=1 {
            for dx in -1..=1 {
                if (dx, dy) != (0, 0) {
                    put(&mut s, x + dx, y + dy, T_WALL, "roche");
                }
            }
        }
        let iron = s.data.resource_index("iron").unwrap();
        let silica = s.data.resource_index("silica").unwrap();
        for (dx, dy) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (-1, -1),
            (1, -1),
            (-1, 1),
        ] {
            let msg = s.rogue_dig(dx, dy).expect("rock can be dug");
            let r = s.rogue.as_ref().unwrap();
            assert!(msg.contains("Roche"), "{msg}");
            assert_eq!(r.tile(x + dx, y + dy), T_FLOOR, "a dug wall is walkable");
            assert_eq!(
                s.data.materials[r.mat(x + dx, y + dy).unwrap()].id,
                "roche_nue",
                "the floor under shows"
            );
        }
        let r = s.rogue.as_ref().unwrap();
        let got = |res: usize| r.loot.iter().find(|l| l.0 == res).map_or(0.0, |l| l.1);
        assert!(got(silica) > 3.0, "silica {}", got(silica));
        assert!(got(iron) > 2.0, "iron {}", got(iron));
        // Something that is not there cannot be dug: water, bedrock, a monster's cell.
        let mut s = bare_field();
        let (x, y) = {
            let r = s.rogue.as_ref().unwrap();
            (r.x, r.y)
        };
        put(&mut s, x + 1, y, T_WATER, "eau");
        assert!(s.rogue_dig(1, 0).is_err());
        assert!(s.rogue_dig(0, 1).is_ok(), "the ground can be dug too");
        assert!(s.rogue_dig(0, 1).is_err(), "bedrock is the end");
        assert!(s.rogue_dig(0, 0).is_err());
    }

    #[test]
    fn hard_matter_needs_a_tool_and_takes_longer() {
        let mut s = bare_field();
        let (x, y) = {
            let r = s.rogue.as_ref().unwrap();
            (r.x, r.y)
        };
        put(&mut s, x + 1, y, T_WALL, "granit");
        put(&mut s, x - 1, y, T_WALL, "bosquet");
        assert!(
            s.rogue_dig(1, 0).is_err(),
            "bare hands do not break granite"
        );
        let t0 = s.rogue.as_ref().unwrap().turn;
        s.rogue_dig(-1, 0).expect("a thicket gives way");
        let soft = s.rogue.as_ref().unwrap().turn - t0;
        let pick = s.data.gear_index("pioche").unwrap();
        s.crew.members[0].gear[2] = Some(pick);
        let t1 = s.rogue.as_ref().unwrap().turn;
        s.rogue_dig(1, 0).expect("a pick breaks granite");
        let hard = s.rogue.as_ref().unwrap().turn - t1;
        assert!(hard > soft, "granite {hard} turns vs thicket {soft}");
        // The tool shortens the work on the same matter.
        put(&mut s, x + 1, y, T_WALL, "granit");
        let t2 = s.rogue.as_ref().unwrap().turn;
        s.rogue_dig(1, 0).unwrap();
        let with = s.rogue.as_ref().unwrap().turn - t2;
        s.crew.members[0].gear[2] = None;
        put(&mut s, x + 1, y, T_WALL, "gres");
        put(&mut s, x, y + 1, T_WALL, "gres");
        let t3 = s.rogue.as_ref().unwrap().turn;
        s.rogue_dig(1, 0).unwrap();
        let bare = s.rogue.as_ref().unwrap().turn - t3;
        s.crew.members[0].gear[2] = Some(pick);
        let t4 = s.rogue.as_ref().unwrap().turn;
        s.rogue_dig(0, 1).unwrap();
        let tooled = s.rogue.as_ref().unwrap().turn - t4;
        assert!(tooled < bare, "pick {tooled} vs hands {bare}");
        assert!(with >= tooled);
    }

    #[test]
    fn the_physics_of_the_world_scales_what_the_ground_holds() {
        let s = bare_field();
        let r = s.rogue.as_ref().unwrap();
        let body = r.body.unwrap();
        let tilt = f64::from(s.planet_map(body).unwrap().surface.tilt);
        let ph = s.system.physics(body, tilt);
        let m = s.dig_mods(r);
        assert!((m.metal - (ph.density / 5514.0).clamp(0.4, 2.0)).abs() < 1e-9);
        // The factor reads the category and the id of the resource.
        let dense = DigMods {
            metal: 2.0,
            ..DigMods::NEUTRAL
        };
        assert_eq!(dense.factor("metal", "iron"), 2.0);
        assert_eq!(dense.factor("precious", "gold"), 2.0);
        assert_eq!(dense.factor("mineral", "silica"), 1.0);
        let hot = DigMods {
            volcanic: 2.5,
            ..DigMods::NEUTRAL
        };
        assert_eq!(hot.factor("mineral", "sulfur"), 2.5);
        assert_eq!(hot.factor("metal", "iron"), 1.0);
        let icy = DigMods {
            cold: 3.0,
            ..DigMods::NEUTRAL
        };
        assert_eq!(icy.factor("volatile", "ammonia"), 3.0);
        assert_eq!(icy.factor("volatile", "water_ice"), 3.0);
        assert_eq!(icy.factor("metal", "iron"), 1.0);
        // The same cell dug on a cold surface gives more ice than on a warm one.
        let ice_kg = |temp: f64| -> f64 {
            let mut s = bare_field();
            s.rogue.as_mut().unwrap().env = Some((1.0, temp));
            let (x, y) = {
                let r = s.rogue.as_ref().unwrap();
                (r.x, r.y)
            };
            let wi = s.data.resource_index("water_ice").unwrap();
            let mut total = 0.0;
            for k in 1..=5 {
                put(&mut s, x + 1, y, T_WALL, "glace_dure");
                let pick = s.data.gear_index("pioche").unwrap();
                s.crew.members[0].gear[2] = Some(pick);
                let r = s.rogue.as_mut().unwrap();
                r.seed ^= k;
                s.rogue_dig(1, 0).unwrap();
                let r = s.rogue.as_ref().unwrap();
                total = r.loot.iter().find(|l| l.0 == wi).map_or(0.0, |l| l.1);
            }
            total
        };
        assert!(
            ice_kg(150.0) > ice_kg(300.0) * 2.0,
            "cold worlds hold more ice"
        );
        // A deeper floor of a mine is richer.
        let mut deep = s.rogue.as_ref().unwrap().clone();
        deep.cur = 0;
        let shallow = s.dig_mods(&deep).depth;
        assert_eq!(shallow, 1.0);
        let mut below = deep.clone();
        below.floors.push(below.floors[0].clone());
        below.cur = 1;
        assert!(s.dig_mods(&below).depth > shallow);
    }

    #[test]
    fn ice_makes_the_squad_slide_and_the_rover_grips() {
        let walk = |mat: &str, rover: bool| -> i32 {
            let mut s = bare_field();
            let (x, y) = {
                let r = s.rogue.as_ref().unwrap();
                (r.x, r.y)
            };
            for dx in 0..40 {
                put(&mut s, x + dx, y, T_FLOOR, mat);
            }
            s.rogue.as_mut().unwrap().vehicle = rover;
            for _ in 0..8 {
                let _ = s.rogue_step(1, 0);
            }
            s.rogue.as_ref().unwrap().x - x
        };
        assert_eq!(walk("regolithe", false), 8, "plain ground: one cell a step");
        assert!(walk("glace", false) > 8, "ice carries further");
    }

    #[test]
    fn caustic_ground_and_lava_hurt_unprotected_squads() {
        let health_after = |mat: &str, neighbour: Option<&str>, armour: bool| -> f32 {
            let mut s = bare_field();
            let (x, y) = {
                let r = s.rogue.as_ref().unwrap();
                (r.x, r.y)
            };
            put(&mut s, x, y, T_FLOOR, mat);
            if let Some(n) = neighbour {
                put(&mut s, x + 1, y, T_WATER, n);
            }
            if armour {
                let suit = s.data.gear_index("exosquelette").unwrap();
                s.crew.members[0].gear[1] = Some(suit);
            }
            s.crew.members[0].health = 1.0;
            for _ in 0..20 {
                s.rogue_step(0, 0).unwrap();
            }
            s.crew.members[0].health
        };
        let safe = health_after("regolithe", None, false);
        let fumes = health_after("soufre", None, false);
        let scorched = health_after("regolithe", Some("lave"), false);
        let water = health_after("regolithe", Some("eau"), false);
        assert!(fumes < safe - 0.05, "{fumes} vs {safe}");
        assert!(scorched < safe - 0.1, "{scorched} vs {safe}");
        assert!((water - safe).abs() < 1e-6, "water does not burn");
        assert!(
            health_after("soufre", None, true) > fumes + 0.03,
            "armour stops part of it"
        );
    }

    #[test]
    fn warm_clothes_stop_the_cold() {
        let (mut s, body, cx, cy) = landing_site();
        let cold = |s: &mut Sim, kit: Option<&str>| -> f32 {
            s.rogue = None;
            s.rogue_land(body, cx, cy, vec![0], false).unwrap();
            s.crew.members[0].gear = vec![None; 3];
            if let Some(k) = kit {
                s.crew.members[0].gear[1] = Some(s.data.gear_index(k).unwrap());
            }
            s.crew.members[0].health = 1.0;
            let r = s.rogue.as_mut().unwrap();
            r.env = Some((1.0, 150.0));
            r.floors[0].monsters.clear();
            for _ in 0..10 {
                s.rogue_step(0, 0).unwrap();
            }
            1.0 - s.crew.members[0].health
        };
        let bare = cold(&mut s, None);
        let wrapped = cold(&mut s, Some("combi_renforcee"));
        assert!(bare > 0.0);
        assert!(wrapped < bare * 0.6, "{wrapped} vs {bare}");
        // Warmth counts on top of the suit's own defence: more than that defence alone would stop.
        let g = &s.data.gear[s.data.gear_index("combi_renforcee").unwrap()];
        assert!(g.warmth > 0.0);
        let by_defence = environment_damage(1.0, 150.0, g.defense).0 as f32 * 10.0;
        assert!(wrapped < by_defence * 0.5, "{wrapped} vs {by_defence}");
    }

    #[test]
    fn the_squad_builds_walls_floors_and_bridges_that_are_its_own() {
        let mut s = bare_field();
        let (x, y) = {
            let r = s.rogue.as_ref().unwrap();
            (r.x, r.y)
        };
        let reg = s.data.resource_index("regolith").unwrap();
        let iron = s.data.resource_index("iron").unwrap();
        // Nothing carried, nothing built.
        assert!(s.rogue_build(1, 0, true).is_err());
        s.rogue.as_mut().unwrap().loot = vec![(reg, 25.0), (iron, 7.0)];
        let msg = s.rogue_build(1, 0, true).expect("a wall of cut stone");
        assert!(msg.contains("Pierre"), "{msg}");
        {
            let r = s.rogue.as_ref().unwrap();
            assert_eq!(r.tile(x + 1, y), T_WALL);
            assert_eq!(
                s.data.materials[r.mat(x + 1, y).unwrap()].id,
                "pierre_taillee"
            );
            assert!(r.owned(x + 1, y) && !r.owned(x - 1, y));
            assert_eq!(r.built_count(), 1);
            let left = r.loot.iter().find(|l| l.0 == reg).unwrap().1;
            assert!((left - 15.0).abs() < 1e-6, "the stone was paid: {left}");
        }
        // A built wall is a wall: it stops the squad, and it can be dug out again.
        assert!(s.rogue_step(1, 0).is_err());
        assert!(s.rogue_build(1, 0, true).is_err(), "already built");
        s.rogue.as_mut().unwrap().loot = vec![(iron, 7.0)];
        assert!(
            s.rogue_build(-1, 0, true).is_err(),
            "no stone left: the wall is refused"
        );
        // A floor of sheet metal over water is a bridge.
        put(&mut s, x, y + 1, T_WATER, "eau");
        assert!(s.rogue_step(0, 1).is_err(), "water stops the squad");
        s.rogue_build(0, 1, false).expect("a floor over the water");
        {
            let r = s.rogue.as_ref().unwrap();
            assert_eq!(r.tile(x, y + 1), T_FLOOR);
            assert_eq!(s.data.materials[r.mat(x, y + 1).unwrap()].id, "tole");
            assert!(r.owned(x, y + 1));
        }
        s.rogue_step(0, 1).expect("the bridge holds");
        // Walls cannot be built over water, nor on a creature.
        s.rogue.as_mut().unwrap().loot = vec![(reg, 25.0), (iron, 7.0)];
        let (nx, ny) = {
            let r = s.rogue.as_ref().unwrap();
            (r.x, r.y)
        };
        put(&mut s, nx + 1, ny, T_WATER, "eau");
        assert!(s.rogue_build(1, 0, true).is_err());
        // Digging out what was built gives its materials back.
        put(&mut s, nx - 1, ny, T_WALL, "pierre_taillee");
        s.crew.members[0].gear[2] = s.data.gear_index("pioche");
        s.rogue_dig(-1, 0).expect("cut stone breaks");
        assert!(s.rogue.as_ref().unwrap().loot.iter().any(|l| l.0 == reg));
    }

    #[test]
    fn plants_take_root_only_where_the_matter_lets_them_and_animals_keep_off_ice() {
        let (mut s, body, cx, cy) = landing_site();
        s.rogue_land(body, cx, cy, vec![0], false).unwrap();
        let d = std::sync::Arc::clone(&s.data);
        let r = s.rogue.as_ref().unwrap();
        let f = r.floor();
        for it in f.items.iter().filter(|i| i.plant) {
            let m = &d.materials[r.mat(it.x, it.y).unwrap()];
            assert!(m.fertility > 0.0, "a plant on {}", m.id);
        }
        for m in f.monsters.iter().filter(|m| m.species.is_some()) {
            let mat = r.mat(m.x, m.y).unwrap();
            assert!(
                !animal_avoids(&d, mat),
                "an animal on {}",
                d.materials[mat].id
            );
        }
        // They keep off it while they walk too: a field of ice with one dry cell, a herd on it.
        let mut s = bare_field();
        let (x, y) = {
            let r = s.rogue.as_ref().unwrap();
            (r.x, r.y)
        };
        let prey = (0..s.data.species.len())
            .find(|&i| !s.data.species[i].is_producer())
            .unwrap();
        for dx in -6..=6 {
            for dy in -6..=6 {
                put(&mut s, x + dx, y + dy, T_FLOOR, "regolithe");
            }
        }
        for dx in 3..=6 {
            for dy in -6..=6 {
                put(&mut s, x + dx, y + dy, T_FLOOR, "glace");
            }
        }
        let mut a = mon_from_species(&s.data, prey, x + 2, y);
        a.hunts = false;
        a.awake = true;
        s.rogue.as_mut().unwrap().floors[0].monsters.push(a);
        let ice = mat_id(&s, "glace");
        for _ in 0..60 {
            s.rogue_step(0, 0).unwrap();
            let r = s.rogue.as_ref().unwrap();
            for m in &r.floor().monsters {
                assert_ne!(
                    r.floor().mats[r.idx(m.x, m.y)],
                    ice,
                    "the animal stepped on ice at ({}, {})",
                    m.x,
                    m.y
                );
            }
        }
    }

    #[test]
    fn every_material_that_gives_or_costs_something_is_consistent() {
        let d = data();
        for m in &d.materials {
            for (res, kg, rarity) in &m.yields {
                assert!(d.resource_index(res).is_some() && *kg > 0.0 && *rarity > 0.0);
            }
            if m.kind == "wall" && !m.yields.is_empty() {
                assert!(!m.under.is_empty(), "{} gives but cannot be dug", m.id);
            }
            if !m.cost.is_empty() {
                // What can be built can be dug out for at least part of its price.
                assert!(!m.yields.is_empty(), "{} is built but gives nothing", m.id);
            }
        }
        // Every kind of resource the physics modulates is obtainable somewhere.
        for cat in ["metal", "precious", "volatile"] {
            assert!(
                d.materials.iter().any(|m| m
                    .yields
                    .iter()
                    .any(|(r, _, _)| d.resources[d.resource_index(r).unwrap()].category == cat)),
                "no material gives a {cat}"
            );
        }
        // The data refuses a broken material.
        let mut bad = (*d).clone();
        bad.materials[0].under = "nulle_part".to_string();
        assert!(bad.validate().is_err());
        let mut bad = (*d).clone();
        bad.materials[0]
            .yields
            .push(("pas_une_ressource".to_string(), 1.0, 0.5));
        assert!(bad.validate().is_err());
    }

    #[test]
    fn animals_live_by_their_biology_asleep_in_the_cold_and_sized_by_their_variant() {
        let d = data();
        let elk = d.species_index("elan_des_neiges").expect("species");
        let biome = d.biome_index("taiga").expect("biome");
        let amb = |temp: f64| Ambient {
            temp,
            night: true,
            world: 11,
            radiation: 0.0,
            biome,
            now: 0.0,
        };
        let warm = mon_adapted(&d, elk, 3, 3, &amb(285.0));
        let cold = mon_adapted(&d, elk, 3, 3, &amb(250.0));
        assert!(!warm.sleeper, "not asleep in the warm season");
        assert!(cold.sleeper && !cold.awake, "hibernating in the cold");
        // The local variant sets the size: hit points follow the cube root of the mass factor.
        let base = mon_from_species(&d, elk, 3, 3);
        let v = crate::biosphere::variant_now(&d, 11, 0.0, elk, biome, 0.0);
        assert!((warm.hp - base.hp * v.mass_factor().cbrt()).abs() < 1e-9);
        // A variant diverged enough carries its own name.
        let named = (0..300u64)
            .map(|w| Ambient {
                world: w,
                ..amb(285.0)
            })
            .find(|a| crate::biosphere::variant_now(&d, a.world, 0.0, elk, biome, 0.0).speciated())
            .expect("some world has a diverged population");
        assert!(mon_adapted(&d, elk, 0, 0, &named).name.contains("variante"));
    }

    // ---- firearms and grenades ----------------------------------------------------------------

    /// Open an arena around the squad (floor from the squad to fourteen cells east, seven rows).
    fn arena(s: &mut Sim) -> (i32, i32) {
        let r = s.rogue.as_mut().unwrap();
        let (x0, y0) = (r.x, r.y);
        for dy in -3..=3 {
            for dx in -1..=14 {
                let (x, y) = (x0 + dx, y0 + dy);
                if r.inside(x, y) && r.tile(x, y) != T_UP {
                    let i = r.idx(x, y);
                    r.floor_mut().tiles[i] = T_FLOOR;
                }
            }
        }
        r.floor_mut().monsters.clear();
        r.floor_mut().items.clear();
        (x0, y0)
    }

    fn equip(s: &mut Sim, gear: &str) {
        let g = s.data.gear_index(gear).unwrap();
        let who = s.rogue.as_ref().unwrap().party[0];
        s.crew.members[who].gear[0] = Some(g);
    }

    #[test]
    fn accuracy_falls_with_distance_and_recoil() {
        assert!(fire_accuracy(2.0, 0.0) > fire_accuracy(8.0, 0.0));
        assert!(fire_accuracy(4.0, 0.0) > fire_accuracy(4.0, 0.4));
        assert!(fire_accuracy(20.0, 0.5) >= 0.1 && fire_accuracy(0.0, 0.0) <= 0.95);
    }

    #[test]
    fn firearms_have_magazines_recoil_noise_and_reload_from_the_hold() {
        let mut s = run_on("epave", 11);
        let (x0, y0) = arena(&mut s);
        let who = s.rogue.as_ref().unwrap().party[0];
        assert!(s.rogue_fire().is_err(), "no firearm yet");
        equip(&mut s, "fusil");
        assert_eq!(s.rogue_rounds(s.rogue.as_ref().unwrap(), who), 12);
        assert!(
            s.rogue_fire().unwrap_err().contains("cible"),
            "nothing to shoot at"
        );
        {
            let foe = mon(&s, "rat", x0 + 4, y0, 1.0e6, true);
            let sleeper = mon(&s, "rat", x0 + 3, y0 + 3, 5.0, false);
            let r = s.rogue.as_mut().unwrap();
            r.floor_mut().monsters.extend([foe, sleeper]);
        }
        let hp0 = s.rogue.as_ref().unwrap().floor().monsters[0].hp;
        s.rogue_fire().expect("fire");
        {
            let r = s.rogue.as_ref().unwrap();
            assert_eq!(s.rogue_rounds(r, who), 11);
            assert!(r.recoil > 0.0 && r.recoil <= MAX_RECOIL);
            assert!(
                r.floor().monsters[1].awake,
                "the shot wakes the sleeper nearby"
            );
        }
        let mut hit = s.rogue.as_ref().unwrap().floor().monsters[0].hp < hp0;
        for _ in 0..11 {
            let _ = s.rogue_fire();
            hit |= s.rogue.as_ref().unwrap().floor().monsters[0].hp < hp0;
        }
        assert!(hit, "twelve shots at four cells land at least once");
        let r = s.rogue.as_ref().unwrap();
        assert_eq!(s.rogue_rounds(r, who), 0);
        assert!(r.recoil <= MAX_RECOIL);
        assert!(s.rogue_fire().unwrap_err().contains("chargeur"));
        // The hold has no ammunition: nothing to load. With some, the magazine fills.
        assert!(s.rogue_reload().unwrap_err().contains("munitions"));
        let slugs = s.data.resource_index("ammo_slugs").unwrap();
        let data = std::sync::Arc::clone(&s.data);
        s.ship.cargo.add(&data, slugs, 10.0);
        let before = s.ship.cargo.kg[slugs];
        s.rogue_reload().expect("reload");
        let r = s.rogue.as_ref().unwrap();
        assert_eq!(s.rogue_rounds(r, who), 12);
        assert!((before - s.ship.cargo.kg[slugs] - 12.0 * ROUND_KG).abs() < 1e-9);
        assert!(s.rogue_reload().unwrap_err().contains("pleins"));
        // Out of reach: a target beyond the range of the gun is not shot at.
        s.rogue.as_mut().unwrap().floor_mut().monsters.clear();
        let far = mon(&s, "rat", x0 + 13, y0, 5.0, true);
        s.rogue.as_mut().unwrap().floor_mut().monsters.push(far);
        assert!(s.rogue_fire().unwrap_err().contains("cible"));
    }

    #[test]
    fn a_grenade_blast_follows_the_chemistry_and_falls_off_with_the_distance() {
        let mut s = run_on("epave", 12);
        let an = s.blast_damage(GRENADE_REACTION, GRENADE_CHARGE_KG).unwrap();
        let nitro = s
            .blast_damage("nitroglycerin_blast", GRENADE_CHARGE_KG)
            .unwrap();
        assert!(an > 3.0 && an < 60.0, "{an}");
        assert!(nitro > an, "nitroglycerin is the stronger explosive");
        let (x0, y0) = arena(&mut s);
        assert!(s.rogue_grenade().unwrap_err().contains("grenade"));
        let g = s.data.resource_index(GRENADE_RESOURCE).unwrap();
        let data = std::sync::Arc::clone(&s.data);
        s.ship.cargo.add(&data, g, 3.0);
        {
            // The target 5 cells east, one at 1 and one at 2 cells from it, one behind a wall,
            // and one asleep far away.
            let ms = [
                mon(&s, "rat", x0 + 5, y0, 1.0e6, true),
                mon(&s, "rat", x0 + 5, y0 + 1, 1.0e6, true),
                mon(&s, "rat", x0 + 7, y0, 1.0e6, true),
                mon(&s, "rat", x0 + 5, y0 + 3, 1.0e6, true),
                mon(&s, "rat", x0 + 6, y0 - 3, 1.0e6, false),
            ];
            let r = s.rogue.as_mut().unwrap();
            // A wall at (x0+5, y0+2) shields the one at y0+3 from the centre.
            let i = r.idx(x0 + 5, y0 + 2);
            r.floor_mut().tiles[i] = T_WALL;
            r.floor_mut().monsters.extend(ms);
        }
        s.rogue_grenade().expect("throw");
        let r = s.rogue.as_ref().unwrap();
        let lost = |k: usize| 1.0e6 - r.floor().monsters[k].hp;
        assert!(
            (lost(0) - an * (1.0 - r.floor().monsters[0].defense)).abs() < 1e-6,
            "centre"
        );
        assert!(lost(1) < lost(0) * 0.6 && lost(1) > 0.0, "one cell away");
        assert!(lost(2) < lost(1) && lost(2) > 0.0, "two cells away");
        assert_eq!(lost(3), 0.0, "the wall stops the blast");
        assert!(r.floor().monsters[4].awake, "the bang wakes the sleeper");
        assert!((s.ship.cargo.kg[g] - 2.0).abs() < 1e-9);
        // Thrown at a target next to the squad, it hurts the squad too.
        let who = s.rogue.as_ref().unwrap().party[0];
        let h0 = s.crew.members[who].health;
        s.rogue.as_mut().unwrap().floor_mut().monsters.clear();
        let near = mon(&s, "rat", x0 + 1, y0, 1.0e6, true);
        s.rogue.as_mut().unwrap().floor_mut().monsters.push(near);
        s.rogue_grenade().expect("throw close");
        assert!(s.crew.members[who].health < h0, "the squad is in the blast");
    }
}
