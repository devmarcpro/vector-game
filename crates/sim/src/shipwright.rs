//! The ship designer: parts on a grid of cells (`data/ship_parts.ron`) make a blueprint; the
//! physics of the whole (mass, centre of mass, inertia, torque of off-axis engines, delta-v, heat,
//! power, landing, flight in air) is computed from it, and a hull (the same `Hull` the fleet and
//! the ship screen already use) is derived from it. Nothing here touches the simulation state.
use crate::G0;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vector_data::ship::{CELL_M, PORT_NX, PORT_NY, PORT_NZ, PORT_PX, PORT_PY, PORT_PZ};
use vector_data::{GameData, Hull, SlotType};

/// The grid: x in 0..GRID_X (tail to nose), y in -GRID_Y..=GRID_Y (port to starboard), z in
/// 0..GRID_Z (down to up).
pub const GRID_X: i8 = 16;
pub const GRID_Y: i8 = 5;
pub const GRID_Z: i8 = 6;
pub const MAX_CELLS: usize = 64;
/// Wage of one person-day of work at a yard (credits).
pub const LABOR_DAY_CR: f64 = 400.0;
/// Share of the materials a ship gives back when taken apart.
pub const RECYCLE: f64 = 0.6;
/// Power of a ship above which it must carry a radiator to be safe (see `analyze`).
pub const MIN_DV: f64 = 1000.0;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub part: String,
    pub x: i8,
    pub y: i8,
    pub z: i8,
}

/// A module fitted in a slot of the part at (x, y, z).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fit {
    pub x: i8,
    pub y: i8,
    pub z: i8,
    pub slot: u8,
    pub module: String,
}

/// A ship plan: what a player designs, saves, shares, finds or buys.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Blueprint {
    pub id: u32,
    pub name: String,
    /// "joueur", "achete" or "trouve".
    pub origin: String,
    pub cells: Vec<Cell>,
    pub fits: Vec<Fit>,
}

/// One slot of the hull a blueprint makes, in the order of `Hull::slots`.
#[derive(Clone, Debug)]
pub struct SlotRef {
    pub x: i8,
    pub y: i8,
    pub z: i8,
    pub slot: u8,
    pub ty: SlotType,
    pub size: u8,
}

fn port_bit(d: (i8, i8, i8)) -> u8 {
    match d {
        (1, 0, 0) => PORT_PX,
        (-1, 0, 0) => PORT_NX,
        (0, 1, 0) => PORT_PY,
        (0, -1, 0) => PORT_NY,
        (0, 0, 1) => PORT_PZ,
        _ => PORT_NZ,
    }
}

const DIRS: [(i8, i8, i8); 6] = [
    (1, 0, 0),
    (-1, 0, 0),
    (0, 1, 0),
    (0, -1, 0),
    (0, 0, 1),
    (0, 0, -1),
];

/// Centre of a cell in metres: x, y, z of the ship frame.
pub fn centre(x: i8, y: i8, z: i8) -> [f64; 3] {
    [
        (f64::from(x) + 0.5) * CELL_M,
        f64::from(y) * CELL_M,
        (f64::from(z) + 0.5) * CELL_M,
    ]
}

impl Blueprint {
    pub fn new(id: u32, name: &str) -> Self {
        Blueprint {
            id,
            name: name.to_string(),
            origin: "joueur".to_string(),
            cells: Vec::new(),
            fits: Vec::new(),
        }
    }

    pub fn cell_at(&self, x: i8, y: i8, z: i8) -> Option<usize> {
        self.cells.iter().position(|c| (c.x, c.y, c.z) == (x, y, z))
    }

    fn sort(&mut self) {
        self.cells.sort_by_key(|c| (c.x, c.y, c.z));
    }

    /// Put a part on a free cell of the grid.
    pub fn place(
        &mut self,
        data: &GameData,
        part: &str,
        x: i8,
        y: i8,
        z: i8,
    ) -> Result<(), &'static str> {
        data.ship_part_index(part).ok_or("piece inconnue")?;
        if !(0..GRID_X).contains(&x)
            || !(-GRID_Y..=GRID_Y).contains(&y)
            || !(0..GRID_Z).contains(&z)
        {
            return Err("hors de la grille");
        }
        if self.cell_at(x, y, z).is_some() {
            return Err("case occupee");
        }
        if self.cells.len() >= MAX_CELLS {
            return Err("trop de pieces");
        }
        self.cells.push(Cell {
            part: part.to_string(),
            x,
            y,
            z,
        });
        self.sort();
        Ok(())
    }

    /// Take the part of a cell away, with the modules fitted in it.
    pub fn remove(&mut self, x: i8, y: i8, z: i8) -> Option<String> {
        let i = self.cell_at(x, y, z)?;
        self.fits.retain(|f| (f.x, f.y, f.z) != (x, y, z));
        Some(self.cells.remove(i).part)
    }

    /// The slots of the hull this plan makes, cell after cell.
    pub fn slots(&self, data: &GameData) -> Vec<SlotRef> {
        let mut v = Vec::new();
        for c in &self.cells {
            let Some(p) = data.ship_part_index(&c.part).map(|i| &data.ship_parts[i]) else {
                continue;
            };
            for (k, (ty, size)) in p.slots.iter().enumerate() {
                v.push(SlotRef {
                    x: c.x,
                    y: c.y,
                    z: c.z,
                    slot: k as u8,
                    ty: *ty,
                    size: *size,
                });
            }
        }
        v
    }

    pub fn fit_at(&self, x: i8, y: i8, z: i8, slot: u8) -> Option<&Fit> {
        self.fits
            .iter()
            .find(|f| (f.x, f.y, f.z, f.slot) == (x, y, z, slot))
    }

    /// Fit a module in a slot of a part (same rules as `ShipDesign::install`).
    pub fn fit(
        &mut self,
        data: &GameData,
        x: i8,
        y: i8,
        z: i8,
        slot: u8,
        module: &str,
    ) -> Result<(), &'static str> {
        let ci = self.cell_at(x, y, z).ok_or("case vide")?;
        let part = &data.ship_parts[data
            .ship_part_index(&self.cells[ci].part)
            .ok_or("piece inconnue")?];
        let (ty, size) = *part
            .slots
            .get(usize::from(slot))
            .ok_or("emplacement inconnu")?;
        let m = &data.modules[data.module_index(module).ok_or("module inconnu")?];
        if self.fit_at(x, y, z, slot).is_some() {
            return Err("emplacement occupe");
        }
        if m.slot != ty {
            return Err("type d'emplacement incompatible");
        }
        if m.size > size {
            return Err("module trop grand pour l'emplacement");
        }
        self.fits.push(Fit {
            x,
            y,
            z,
            slot,
            module: module.to_string(),
        });
        Ok(())
    }

    pub fn unfit(&mut self, x: i8, y: i8, z: i8, slot: u8) -> Option<String> {
        let i = self
            .fits
            .iter()
            .position(|f| (f.x, f.y, f.z, f.slot) == (x, y, z, slot))?;
        Some(self.fits.remove(i).module)
    }

    /// Copy of the plan reflected through the plane y = 0 (cells and fits).
    pub fn mirror_y(&mut self) {
        for c in &mut self.cells {
            c.y = -c.y;
        }
        for f in &mut self.fits {
            f.y = -f.y;
        }
        self.sort();
    }

    /// The plan as text (RON), to save to a file or to share.
    pub fn to_text(&self) -> String {
        vector_data::ship::to_text(self)
    }

    /// Read a plan from text and check it names real parts and modules that fit.
    pub fn from_text(data: &GameData, text: &str) -> Result<Blueprint, String> {
        let mut bp: Blueprint =
            vector_data::ship::from_text(text).map_err(|e| format!("plan illisible: {e}"))?;
        let fits = std::mem::take(&mut bp.fits);
        let cells = std::mem::take(&mut bp.cells);
        for c in cells {
            bp.place(data, &c.part, c.x, c.y, c.z)
                .map_err(|e| format!("plan invalide: {e}"))?;
        }
        for f in fits {
            bp.fit(data, f.x, f.y, f.z, f.slot, &f.module)
                .map_err(|e| format!("plan invalide: {e}"))?;
        }
        Ok(bp)
    }

    /// Whether the part of cell `a` is joined to the part of the neighbouring cell `b`.
    fn joined(&self, data: &GameData, a: usize, d: (i8, i8, i8)) -> Option<usize> {
        let ca = &self.cells[a];
        let b = self.cell_at(ca.x + d.0, ca.y + d.1, ca.z + d.2)?;
        let pa = &data.ship_parts[data.ship_part_index(&ca.part)?];
        let pb = &data.ship_parts[data.ship_part_index(&self.cells[b].part)?];
        let back = (-d.0, -d.1, -d.2);
        (pa.ports & port_bit(d) != 0 && pb.ports & port_bit(back) != 0).then_some(b)
    }

    /// Cells reachable from the first cockpit through matching ports.
    pub fn connected(&self, data: &GameData) -> Vec<bool> {
        let mut seen = vec![false; self.cells.len()];
        let Some(root) = self.cells.iter().position(|c| {
            data.ship_part_index(&c.part)
                .is_some_and(|i| data.ship_parts[i].kind == "cockpit")
        }) else {
            return seen;
        };
        let mut stack = vec![root];
        seen[root] = true;
        while let Some(a) = stack.pop() {
            for d in DIRS {
                if let Some(b) = self.joined(data, a, d) {
                    if !seen[b] {
                        seen[b] = true;
                        stack.push(b);
                    }
                }
            }
        }
        seen
    }

    /// The rooms of the inside of the ship: the deck (z) with most walkable cells, as
    /// (x, y, room kind).
    pub fn interior(&self, data: &GameData) -> Vec<(i8, i8, String)> {
        let room_of = |c: &Cell| {
            data.ship_part_index(&c.part)
                .map(|i| data.ship_parts[i].room.clone())
                .unwrap_or_default()
        };
        let mut per_deck: BTreeMap<i8, usize> = BTreeMap::new();
        for c in &self.cells {
            if !room_of(c).is_empty() {
                *per_deck.entry(c.z).or_default() += 1;
            }
        }
        // The first deck wins a tie: the lowest one, nearest the landing gear.
        let mut best: Option<(i8, usize)> = None;
        for (z, n) in per_deck {
            if best.is_none_or(|(_, bn)| n > bn) {
                best = Some((z, n));
            }
        }
        let Some((deck, _)) = best else {
            return Vec::new();
        };
        self.cells
            .iter()
            .filter(|c| c.z == deck)
            .filter_map(|c| {
                let r = room_of(c);
                (!r.is_empty()).then_some((c.x, c.y, r))
            })
            .collect()
    }
}

/// Everything that follows from a blueprint.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub cells: usize,
    pub mass_structure: f64,
    pub mass_modules: f64,
    pub mass_dry: f64,
    pub fuel_kg: f64,
    pub mass_wet: f64,
    /// Centre of mass (m) with empty and full tanks.
    pub com_dry: [f64; 3],
    pub com_wet: [f64; 3],
    /// Extent of the ship (m): length, width, height.
    pub extent: [f64; 3],
    /// Moments of inertia about the centre of mass (kg m2), full tanks: roll, pitch, yaw.
    pub inertia: [f64; 3],
    pub thrust: f64,
    pub isp: f64,
    /// Torque (N m) of the engines at full thrust about the centre of mass (pitch, yaw).
    pub parasitic: [f64; 2],
    /// Torque of the attitude thrusters (N m) and the angular acceleration that gives (rad/s2).
    pub rcs_force: f64,
    pub rcs_torque: f64,
    pub ang_accel: f64,
    pub dv_empty: f64,
    pub dv_loaded: f64,
    pub twr: f64,
    pub power_supply: f64,
    pub power_peak: f64,
    /// Temperature (K) the ship settles at with everything running.
    pub temp_eq: f64,
    pub hp: f64,
    pub armor: f64,
    pub berths: u32,
    pub crew_cap: f64,
    pub life_cap: f64,
    pub hab_m3: f64,
    pub volume: f64,
    pub cargo_kg: f64,
    pub cargo_m3: f64,
    pub hangar: f64,
    pub lift_area: f64,
    /// Position (m) of the centre of pressure and the margin of stability (share of the length,
    /// positive = the nose follows the wind).
    pub cp_x: f64,
    pub aero_margin: f64,
    pub aero_skew: f64,
    pub wing_loading: f64,
    pub gears: usize,
    pub gear_support_kg: f64,
    pub gear_stable: bool,
    /// Materials the build consumes (resource index, kg), person-days, credits of modules.
    pub materials: Vec<(usize, f64)>,
    pub labor: f64,
    pub modules_cost: f64,
    /// Value of the ship at market prices (materials + labour + modules).
    pub value: f64,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Report {
    pub fn buildable(&self) -> bool {
        self.errors.is_empty()
    }
}

/// What the ship does on a world with surface gravity `g` (m/s2).
#[derive(Clone, Debug)]
pub struct LandingCheck {
    pub can_land: bool,
    pub can_take_off: bool,
    pub notes: Vec<String>,
}

pub fn landing(r: &Report, g: f64) -> LandingCheck {
    let mut notes = Vec::new();
    let weight_n = r.mass_wet * g;
    let mut can_land = true;
    if r.gears == 0 {
        notes.push("aucun train d'atterrissage".to_string());
        can_land = false;
    } else if !r.gear_stable {
        notes.push("trains mal disposes: le centre de masse sort de l'appui".to_string());
        can_land = false;
    }
    if r.gear_support_kg * G0 < weight_n {
        notes.push(format!(
            "trains trop faibles: {:.1} g pour {:.1} g de pesanteur",
            r.gear_support_kg / r.mass_wet.max(1.0),
            g / G0
        ));
        can_land = false;
    }
    let can_take_off = r.thrust > weight_n;
    if !can_take_off {
        notes.push(format!(
            "poussee insuffisante pour redecoller ({:.2} g)",
            r.thrust / weight_n.max(1.0)
        ));
    }
    LandingCheck {
        can_land,
        can_take_off,
        notes,
    }
}

fn add(a: &mut [f64; 3], k: f64, v: [f64; 3]) {
    for i in 0..3 {
        a[i] += k * v[i];
    }
}

/// Moments of inertia about `com` of point masses (position, mass) plus the cubes they sit in.
fn inertia_of(items: &[([f64; 3], f64, bool)], com: [f64; 3]) -> [f64; 3] {
    let mut i = [0.0; 3];
    for (p, m, cube) in items {
        let d = [p[0] - com[0], p[1] - com[1], p[2] - com[2]];
        let own = if *cube {
            m * CELL_M * CELL_M / 6.0
        } else {
            0.0
        };
        i[0] += m * (d[1] * d[1] + d[2] * d[2]) + own;
        i[1] += m * (d[0] * d[0] + d[2] * d[2]) + own;
        i[2] += m * (d[0] * d[0] + d[1] * d[1]) + own;
    }
    i
}

/// The physics of a blueprint and every reason it should not be built or will fly badly.
pub fn analyze(data: &GameData, bp: &Blueprint) -> Report {
    let mut r = Report {
        cells: bp.cells.len(),
        ..Default::default()
    };
    let mut mass_items: Vec<([f64; 3], f64, bool)> = Vec::new();
    let mut fuel_items: Vec<([f64; 3], f64)> = Vec::new();
    let mut mats: BTreeMap<usize, f64> = BTreeMap::new();
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    let (mut aero_sum, mut aero_x, mut aero_y) = (0.0, 0.0, 0.0);
    let mut cockpits = 0;
    let mut rcs_parts = 0.0;
    let mut gears: Vec<(f64, f64, f64)> = Vec::new();
    let mut min_z = i8::MAX;
    for c in &bp.cells {
        min_z = min_z.min(c.z);
    }
    for c in &bp.cells {
        let Some(pi) = data.ship_part_index(&c.part) else {
            r.errors.push(format!("piece inconnue: {}", c.part));
            continue;
        };
        let p = &data.ship_parts[pi];
        let at = centre(c.x, c.y, c.z);
        r.mass_structure += p.mass;
        mass_items.push((at, p.mass, true));
        for k in 0..3 {
            lo[k] = lo[k].min(at[k] - CELL_M / 2.0);
            hi[k] = hi[k].max(at[k] + CELL_M / 2.0);
        }
        r.hp += p.hp;
        r.berths += p.berths;
        r.hab_m3 += p.hab_m3;
        r.volume += p.volume;
        r.lift_area += p.lift_area;
        r.labor += p.labor;
        rcs_parts += p.rcs;
        aero_sum += p.aero_area;
        aero_x += p.aero_area * at[0];
        aero_y += p.aero_area * at[1];
        if p.kind == "cockpit" {
            cockpits += 1;
        }
        if p.kind == "gear" {
            gears.push((at[0], at[1], p.support_kg));
        }
        for (res, kg) in &p.materials {
            if let Some(ri) = data.resource_index(res) {
                *mats.entry(ri).or_default() += kg;
            }
        }
    }
    r.rcs_force = rcs_parts;
    // Modules.
    let (mut thrust_isp, mut engines) = (0.0, Vec::new());
    let mut rcs_modules = 0.0;
    for f in &bp.fits {
        let Some(mi) = data.module_index(&f.module) else {
            r.errors.push(format!("module inconnu: {}", f.module));
            continue;
        };
        if bp.cell_at(f.x, f.y, f.z).is_none() {
            r.errors.push(format!("module {} sans piece", f.module));
            continue;
        }
        let m = &data.modules[mi];
        let at = centre(f.x, f.y, f.z);
        r.mass_modules += m.mass;
        mass_items.push((at, m.mass, false));
        if m.fuel > 0.0 {
            r.fuel_kg += m.fuel;
            fuel_items.push((at, m.fuel));
        }
        r.cargo_kg += m.cargo_kg;
        r.cargo_m3 += m.cargo_m3;
        r.crew_cap += m.crew_cap;
        r.life_cap += m.life_support;
        r.hangar += m.hangar;
        r.hp += if m.category == "armor" { m.hp } else { 0.0 };
        r.armor += m.armor;
        r.modules_cost += m.cost;
        rcs_modules += m.rcs;
        if m.power > 0.0 {
            r.power_supply += m.power;
        } else {
            r.power_peak += -m.power;
        }
        if m.thrust > 0.0 {
            r.thrust += m.thrust;
            thrust_isp += m.thrust * m.isp;
            engines.push((at, m.thrust));
        }
    }
    r.isp = if r.thrust > 0.0 {
        thrust_isp / r.thrust
    } else {
        0.0
    };
    r.mass_dry = r.mass_structure + r.mass_modules;
    r.mass_wet = r.mass_dry + r.fuel_kg;
    // Centre of mass with empty and full tanks.
    let com = |extra: &[([f64; 3], f64)]| {
        let mut s = [0.0; 3];
        let mut m = 0.0;
        for (p, k, _) in &mass_items {
            add(&mut s, *k, *p);
            m += k;
        }
        for (p, k) in extra {
            add(&mut s, *k, *p);
            m += k;
        }
        if m > 0.0 {
            [s[0] / m, s[1] / m, s[2] / m]
        } else {
            [0.0; 3]
        }
    };
    r.com_dry = com(&[]);
    r.com_wet = com(&fuel_items);
    if !bp.cells.is_empty() {
        r.extent = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    }
    let mut full_items = mass_items.clone();
    full_items.extend(fuel_items.iter().map(|(p, k)| (*p, *k, false)));
    r.inertia = inertia_of(&full_items, r.com_wet);
    // Engines push along +x from where they are: the torque about the centre of mass.
    for (at, f) in &engines {
        r.parasitic[0] += (at[2] - r.com_wet[2]) * f;
        r.parasitic[1] += -(at[1] - r.com_wet[1]) * f;
    }
    // Attitude thrusters: the lever arm is half the length of the ship (at least a cell).
    let arm = (r.extent[0] / 2.0).max(CELL_M / 2.0);
    let rcs_total = rcs_parts + rcs_modules;
    r.rcs_torque = rcs_total * arm;
    let i_turn = 0.5 * (r.inertia[1] + r.inertia[2]);
    r.ang_accel = if i_turn > 0.0 {
        r.rcs_torque / i_turn
    } else {
        0.0
    };
    // Delta-v (Tsiolkovsky) and thrust to weight.
    let dv = |m_dry: f64| {
        if r.isp > 0.0 && r.fuel_kg > 0.0 && m_dry > 0.0 {
            r.isp * G0 * ((m_dry + r.fuel_kg) / m_dry).ln()
        } else {
            0.0
        }
    };
    r.dv_empty = dv(r.mass_dry);
    r.dv_loaded = dv(r.mass_dry + r.cargo_kg);
    r.twr = if r.mass_wet > 0.0 {
        r.thrust / (r.mass_wet * G0)
    } else {
        0.0
    };
    // Heat: the ship settles where the radiators shed what the reactors and loads release.
    let heat_in = r.power_supply.min(r.power_peak) * crate::design::REACTOR_WASTE + r.power_peak;
    let radiate: f64 = bp
        .fits
        .iter()
        .filter_map(|f| data.module_index(&f.module))
        .map(|mi| data.modules[mi].radiate)
        .sum();
    let rated = 30.0 + 0.004 * r.mass_dry + radiate;
    let (tc, tr) = (crate::design::T_COLD, crate::design::T_REF);
    r.temp_eq = (tc.powi(4) + heat_in / rated * (tr.powi(4) - tc.powi(4))).powf(0.25);
    // Flight in air: the centre of pressure against the centre of mass.
    if aero_sum > 0.0 {
        r.cp_x = aero_x / aero_sum;
        r.aero_margin = if r.extent[0] > 0.0 {
            (r.com_wet[0] - r.cp_x) / r.extent[0]
        } else {
            0.0
        };
        r.aero_skew = (aero_y / aero_sum - r.com_wet[1]).abs();
    }
    r.wing_loading = if r.lift_area > 0.0 {
        r.mass_wet / r.lift_area
    } else {
        f64::INFINITY
    };
    // Landing: only the legs on the lowest layer touch the ground; the centre of mass must be
    // inside the polygon they make.
    let low: Vec<&(f64, f64, f64)> = gears
        .iter()
        .filter(|g| {
            bp.cells
                .iter()
                .filter(|c| centre(c.x, c.y, c.z)[0] == g.0 && centre(c.x, c.y, c.z)[1] == g.1)
                .map(|c| c.z)
                .min()
                == Some(min_z)
        })
        .collect();
    r.gears = low.len();
    r.gear_support_kg = low.iter().map(|g| g.2).sum();
    let pts: Vec<(f64, f64)> = low.iter().map(|g| (g.0, g.1)).collect();
    r.gear_stable = inside_hull(&pts, (r.com_wet[0], r.com_wet[1]), 0.3);
    // Cost.
    r.materials = mats.into_iter().collect();
    let mat_value: f64 = r
        .materials
        .iter()
        .map(|(i, kg)| data.resources[*i].value * kg)
        .sum();
    r.value = mat_value + r.labor * LABOR_DAY_CR + r.modules_cost;
    verdict(data, bp, &mut r, cockpits);
    r
}

/// Whether `p` lies inside the convex hull of `pts` by at least `margin` metres (needs three
/// points that are not in a line).
fn inside_hull(pts: &[(f64, f64)], p: (f64, f64), margin: f64) -> bool {
    if pts.len() < 3 {
        return false;
    }
    // Monotone chain.
    let mut s: Vec<(f64, f64)> = pts.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    s.dedup();
    if s.len() < 3 {
        return false;
    }
    let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    let mut h: Vec<(f64, f64)> = Vec::new();
    for pass in 0..2 {
        let start = h.len();
        let it: Vec<(f64, f64)> = if pass == 0 {
            s.clone()
        } else {
            s.iter().rev().copied().collect()
        };
        for q in it {
            while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], q) <= 1e-9 {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    if h.len() < 3 {
        return false;
    }
    (0..h.len()).all(|i| {
        let (a, b) = (h[i], h[(i + 1) % h.len()]);
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        len > 0.0 && cross(a, b, p) / len >= margin
    })
}

fn verdict(data: &GameData, bp: &Blueprint, r: &mut Report, cockpits: usize) {
    if bp.cells.is_empty() {
        r.errors.push("plan vide".to_string());
        return;
    }
    if cockpits == 0 {
        r.errors
            .push("pas de noyau d'IA ni de passerelle".to_string());
    }
    let seen = bp.connected(data);
    let loose = seen.iter().filter(|s| !**s).count();
    if cockpits > 0 && loose > 0 {
        r.errors.push(format!(
            "{loose} piece(s) non reliee(s): les raccords ne correspondent pas"
        ));
    }
    if r.thrust <= 0.0 {
        r.warnings
            .push("aucun moteur: le vaisseau ne bougera pas".to_string());
    } else if r.twr < 1.0 {
        r.warnings.push(format!(
            "poussee {:.2} g: ne decolle d'aucune planete a 1 g",
            r.twr
        ));
    }
    if r.thrust > 0.0 && r.fuel_kg <= 0.0 {
        r.warnings.push("aucun reservoir".to_string());
    } else if r.thrust > 0.0 && r.dv_empty < MIN_DV {
        r.warnings.push(format!(
            "delta-v {:.0} m/s seulement (sous {MIN_DV:.0})",
            r.dv_empty
        ));
    }
    if r.power_supply <= 0.0 && r.power_peak > 0.0 {
        r.warnings.push("aucune source d'energie".to_string());
    } else if r.power_peak > r.power_supply * 1.0 + 1e-9 {
        r.warnings.push(format!(
            "pic de {:.0} kW pour {:.0} kW produits: des modules s'eteindront",
            r.power_peak, r.power_supply
        ));
    }
    if r.temp_eq > crate::design::T_DAMAGE {
        r.warnings.push(format!(
            "surchauffe: {:.0} K a pleine charge, il faut des radiateurs",
            r.temp_eq
        ));
    }
    let people = f64::from(r.berths) + r.crew_cap;
    if people > 0.0 && r.life_cap < people {
        r.warnings.push(format!(
            "support de vie pour {:.0} personnes, {:.0} a bord",
            r.life_cap, people
        ));
    }
    if r.thrust > 0.0 && r.rcs_torque > 0.0 {
        let par = (r.parasitic[0].powi(2) + r.parasitic[1].powi(2)).sqrt();
        if par > r.rcs_torque {
            r.warnings.push(
                "couple parasite des moteurs superieur a la manoeuvre: ingouvernable".to_string(),
            );
        } else if par > 0.3 * r.rcs_torque {
            r.warnings.push(format!(
                "moteurs desaxes: {:.0}% de la manoeuvre sert a tenir le cap",
                100.0 * par / r.rcs_torque
            ));
        }
    } else if r.thrust > 0.0 {
        r.warnings.push("aucun propulseur d'attitude".to_string());
    }
    if r.gears == 0 {
        r.warnings
            .push("sans train: ne peut pas se poser".to_string());
    } else if !r.gear_stable {
        r.warnings
            .push("trains en ligne ou hors du centre de masse: bascule au sol".to_string());
    }
    if r.lift_area > 0.0 && r.aero_margin < 0.02 {
        r.warnings.push(
            "instable en atmosphere: le centre de poussee est devant le centre de masse"
                .to_string(),
        );
    } else if r.lift_area <= 0.0 && r.aero_margin < 0.0 {
        r.warnings
            .push("instable en atmosphere: il tournera de l'arriere en entrant".to_string());
    }
    // A ship that cannot even carry its own weight is not a ship.
    if r.hp < 0.0 {
        r.errors.push("structure negative".to_string());
    }
}

/// The hull a blueprint makes, for the fleet and the ship screen: structure, slots, thrusters and
/// the agility its inertia allows (less what the off-axis engines waste).
pub fn hull_of(data: &GameData, bp: &Blueprint, r: &Report) -> Hull {
    let slots: Vec<(SlotType, u8)> = bp.slots(data).iter().map(|s| (s.ty, s.size)).collect();
    let par = (r.parasitic[0].powi(2) + r.parasitic[1].powi(2)).sqrt();
    let trim = if r.rcs_torque > 0.0 {
        (1.0 - par / r.rcs_torque).clamp(0.1, 1.0)
    } else {
        1.0
    };
    // Same calibration as the hand-made hulls: the turn rate is three quarters of the acceleration.
    let ang = (r.ang_accel * trim).clamp(0.2, 5.0);
    let class = if r.hangar > 0.0 || slots.iter().any(|s| s.0 == SlotType::Hardpoint) {
        "combat"
    } else if r.berths >= 20 {
        "colonial"
    } else if r.cargo_kg >= 30_000.0 {
        "cargo"
    } else {
        "sur mesure"
    };
    Hull {
        id: format!("custom_{}", bp.id),
        name: bp.name.clone(),
        class: class.to_string(),
        mass: r.mass_structure.max(1.0),
        hp: r.hp.max(1.0),
        cost: r.value - r.modules_cost,
        rcs: r.rcs_force.max(1000.0),
        max_rate: (0.75 * ang).clamp(0.3, 3.5),
        ang_accel: ang,
        slots,
    }
}

/// Bring the loadout of a plan into a design of the hull it made (`hull` index in the data).
pub fn design_of(data: &GameData, bp: &Blueprint, hull: usize) -> crate::design::ShipDesign {
    let mut d = crate::design::ShipDesign::empty(data, hull);
    for (i, s) in bp.slots(data).iter().enumerate() {
        if let Some(f) = bp.fit_at(s.x, s.y, s.z, s.slot) {
            if let Some(mi) = data.module_index(&f.module) {
                let _ = d.install(data, i, mi);
            }
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn data() -> GameData {
        GameData::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")).expect("data")
    }

    fn plan_of(d: &GameData, id: &str) -> Blueprint {
        let def = d.ship_plans.iter().find(|p| p.id == id).expect("plan");
        let mut bp = Blueprint::new(1, &def.name);
        for (p, x, y, z) in &def.cells {
            bp.place(d, p, *x, *y, *z).expect("place");
        }
        for (x, y, z, s, m) in &def.loadout {
            bp.fit(d, *x, *y, *z, *s, m).expect("fit");
        }
        bp
    }

    fn part_mass(d: &GameData, id: &str) -> f64 {
        d.ship_parts[d.ship_part_index(id).expect("part")].mass
    }

    #[test]
    fn every_plan_on_sale_is_buildable() {
        let d = data();
        for def in &d.ship_plans {
            let bp = plan_of(&d, &def.id);
            let r = analyze(&d, &bp);
            println!(
                "{}: m={:.0} dry, {:.0} wet, com=({:.1},{:.1},{:.1}) dv={:.0} twr={:.2} par={:?} rcs={:.0} gears={} stable={} T={:.0} margin={:.3} val={:.0} warn={:?} err={:?}",
                def.id, r.mass_dry, r.mass_wet, r.com_wet[0], r.com_wet[1], r.com_wet[2],
                r.dv_empty, r.twr, r.parasitic, r.rcs_torque, r.gears, r.gear_stable, r.temp_eq,
                r.aero_margin, r.value, r.warnings, r.errors
            );
            assert!(r.buildable(), "{}: {:?}", def.id, r.errors);
            assert!(r.thrust > 0.0 && r.dv_empty > MIN_DV, "{}", def.id);
            assert!(r.gear_stable, "{}: legs around the centre of mass", def.id);
            assert!(
                !r.warnings.iter().any(|w| w.contains("support de vie")),
                "{}",
                def.id
            );
        }
    }

    #[test]
    fn mass_is_the_sum_of_parts_and_modules() {
        let d = data();
        let bp = plan_of(&d, "courier_x");
        let r = analyze(&d, &bp);
        let parts: f64 = bp.cells.iter().map(|c| part_mass(&d, &c.part)).sum();
        let mods: f64 = bp
            .fits
            .iter()
            .map(|f| d.modules[d.module_index(&f.module).expect("m")].mass)
            .sum();
        assert!((r.mass_structure - parts).abs() < 1e-6);
        assert!((r.mass_modules - mods).abs() < 1e-6);
        assert!((r.mass_wet - (parts + mods + r.fuel_kg)).abs() < 1e-6);
        let hull = hull_of(&d, &bp, &r);
        assert!(
            (hull.mass - parts).abs() < 1e-6,
            "the hull carries the structure mass"
        );
        assert_eq!(hull.slots.len(), bp.slots(&d).len());
    }

    #[test]
    fn centre_of_mass_on_known_cases() {
        let d = data();
        // Two equal frames side by side: the middle of the pair.
        let mut bp = Blueprint::new(1, "t");
        bp.place(&d, "hull_frame", 0, 0, 0).expect("p");
        bp.place(&d, "hull_frame", 1, 0, 0).expect("p");
        let r = analyze(&d, &bp);
        assert!((r.com_dry[0] - 6.0).abs() < 1e-9, "{}", r.com_dry[0]);
        assert!(r.com_dry[1].abs() < 1e-9 && (r.com_dry[2] - 3.0).abs() < 1e-9);
        // A heavy block pulls it: weighted mean of the cell centres 3 m and 9 m.
        let mut bp = Blueprint::new(1, "t");
        bp.place(&d, "hull_frame", 0, 0, 0).expect("p");
        bp.place(&d, "hull_armor", 1, 0, 0).expect("p");
        let (a, b) = (part_mass(&d, "hull_frame"), part_mass(&d, "hull_armor"));
        let r = analyze(&d, &bp);
        assert!((r.com_dry[0] - (a * 3.0 + b * 9.0) / (a + b)).abs() < 1e-9);
    }

    #[test]
    fn inertia_of_two_masses_matches_the_formula() {
        let d = data();
        let mut bp = Blueprint::new(1, "t");
        bp.place(&d, "hull_frame", 0, 0, 0).expect("p");
        bp.place(&d, "hull_frame", 1, 0, 0).expect("p");
        let r = analyze(&d, &bp);
        let m = part_mass(&d, "hull_frame");
        // Each frame is 3 m from the centre along x, plus its own cube.
        let own = m * CELL_M * CELL_M / 6.0;
        assert!((r.inertia[1] - 2.0 * (m * 9.0 + own)).abs() < 1e-6, "pitch");
        assert!(
            (r.inertia[0] - 2.0 * own).abs() < 1e-6,
            "roll about the long axis"
        );
    }

    #[test]
    fn an_off_axis_engine_makes_a_parasitic_torque_and_a_centred_one_does_not() {
        let d = data();
        // A frame with an engine on a nacelle to starboard.
        let mut bp = Blueprint::new(1, "t");
        bp.place(&d, "hull_frame", 0, 0, 0).expect("p");
        bp.place(&d, "nacelle_pod", 0, 1, 0).expect("p");
        bp.fit(&d, 0, 1, 0, 0, "torch_mk2").expect("fit");
        let r = analyze(&d, &bp);
        let f = d.modules[d.module_index("torch_mk2").expect("m")].thrust;
        let arm = 6.0 - r.com_wet[1];
        assert!(
            (r.parasitic[1] + arm * f).abs() < 1e-3,
            "yaw torque = -r_y F: {:?}",
            r.parasitic
        );
        assert!(r.parasitic[1].abs() > 1.0e6);
        // The same engine in the middle of a symmetric stack: the centre of mass is on its line.
        let mut bp = Blueprint::new(1, "t");
        bp.place(&d, "mount_main", 0, 0, 0).expect("p");
        bp.fit(&d, 0, 0, 0, 0, "torch_mk2").expect("fit");
        let r = analyze(&d, &bp);
        assert!(
            r.parasitic[0].abs() < 1e-6 && r.parasitic[1].abs() < 1e-6,
            "{:?}",
            r.parasitic
        );
        // Two mirrored nacelles cancel.
        let mut bp = Blueprint::new(1, "t");
        bp.place(&d, "hull_frame", 0, 0, 0).expect("p");
        bp.place(&d, "nacelle_pod", 0, 1, 0).expect("p");
        bp.place(&d, "nacelle_pod", 0, -1, 0).expect("p");
        bp.fit(&d, 0, 1, 0, 0, "torch_mk2").expect("fit");
        bp.fit(&d, 0, -1, 0, 0, "torch_mk2").expect("fit");
        let r = analyze(&d, &bp);
        assert!(r.parasitic[1].abs() < 1e-3, "{:?}", r.parasitic);
    }

    #[test]
    fn off_axis_thrust_costs_the_hull_its_agility() {
        let d = data();
        let mut a = Blueprint::new(1, "t");
        a.place(&d, "hull_frame", 0, 0, 0).expect("p");
        a.place(&d, "nacelle_pod", 0, 1, 0).expect("p");
        a.place(&d, "rcs_block", 1, 0, 0).expect("p");
        a.fit(&d, 0, 1, 0, 0, "torch_mk2").expect("fit");
        let mut b = a.clone();
        b.unfit(0, 1, 0, 0);
        b.fit(&d, 0, 1, 0, 1, "radiator_mk1").expect("fit");
        let (ra, rb) = (analyze(&d, &a), analyze(&d, &b));
        assert!(ra.parasitic[1].abs() > 0.0 && rb.parasitic[1] == 0.0);
        let (ha, hb) = (hull_of(&d, &a, &ra), hull_of(&d, &b, &rb));
        assert!(
            ha.ang_accel < hb.ang_accel,
            "{} vs {}",
            ha.ang_accel,
            hb.ang_accel
        );
    }

    #[test]
    fn delta_v_is_tsiolkovsky() {
        let d = data();
        let mut bp = Blueprint::new(1, "t");
        bp.place(&d, "mount_main", 0, 0, 0).expect("p");
        bp.place(&d, "hold_tankage", 1, 0, 0).expect("p");
        bp.fit(&d, 0, 0, 0, 0, "torch_mk2").expect("fit");
        bp.fit(&d, 1, 0, 0, 0, "tank_mk4").expect("fit");
        let r = analyze(&d, &bp);
        let isp = d.modules[d.module_index("torch_mk2").expect("m")].isp;
        let m1 = r.mass_dry;
        let m0 = m1 + 32_000.0;
        assert!((r.dv_empty - isp * G0 * (m0 / m1).ln()).abs() < 1e-6);
        // Cargo weighs on it.
        let mut with = bp.clone();
        with.fit(&d, 1, 0, 0, 1, "cargo_mk4").expect("fit");
        let rw = analyze(&d, &with);
        assert!(rw.dv_loaded < rw.dv_empty);
        // No tank, no delta-v.
        let mut dry = Blueprint::new(1, "t");
        dry.place(&d, "mount_main", 0, 0, 0).expect("p");
        dry.fit(&d, 0, 0, 0, 0, "torch_mk2").expect("fit");
        assert_eq!(analyze(&d, &dry).dv_empty, 0.0);
    }

    #[test]
    fn the_validation_refuses_what_cannot_be_built() {
        let d = data();
        // No cockpit.
        let mut bp = Blueprint::new(1, "t");
        bp.place(&d, "hull_frame", 0, 0, 0).expect("p");
        assert!(!analyze(&d, &bp).buildable());
        // Empty plan.
        assert!(!analyze(&d, &Blueprint::new(1, "t")).buildable());
        // A part joined face to a face with no port: the nose cone only joins behind.
        let mut bp = Blueprint::new(1, "t");
        bp.place(&d, "core_cockpit", 0, 0, 0).expect("p");
        bp.place(&d, "hull_nose", 1, 0, 0).expect("p");
        assert!(analyze(&d, &bp).buildable());
        bp.place(&d, "hull_nose", 0, 1, 0).expect("p");
        let r = analyze(&d, &bp);
        assert!(
            !r.buildable(),
            "a nose cone cannot hang on a side: {:?}",
            r.errors
        );
        // Grid and collisions.
        assert!(bp.place(&d, "hull_frame", 0, 0, 0).is_err());
        assert!(bp.place(&d, "hull_frame", GRID_X, 0, 0).is_err());
        assert!(bp.place(&d, "hull_frame", 0, 0, -1).is_err());
        // Module slots.
        let mut ok = Blueprint::new(1, "t");
        ok.place(&d, "core_cockpit", 0, 0, 0).expect("p");
        assert!(
            ok.fit(&d, 0, 0, 0, 0, "tank_mk1").is_err(),
            "internal module in a core slot"
        );
        assert!(ok.fit(&d, 0, 0, 0, 0, "torch_mk4").is_err(), "too big");
        assert!(ok.fit(&d, 0, 0, 0, 0, "torch_mk2").is_ok());
        assert!(ok.fit(&d, 0, 0, 0, 0, "torch_mk1").is_err(), "taken");
    }

    #[test]
    fn warnings_follow_the_numbers() {
        let d = data();
        let mut bp = Blueprint::new(1, "t");
        bp.place(&d, "core_cockpit", 0, 0, 0).expect("p");
        let r = analyze(&d, &bp);
        assert!(r.warnings.iter().any(|w| w.contains("aucun moteur")));
        assert!(r.warnings.iter().any(|w| w.contains("sans train")));
        // A weak engine under heavy armour cannot lift off a heavy world.
        let mut bp = Blueprint::new(1, "t");
        bp.place(&d, "mount_light", 0, 0, 0).expect("p");
        bp.place(&d, "hull_armor", 1, 0, 0).expect("p");
        bp.place(&d, "hull_armor", 2, 0, 0).expect("p");
        bp.place(&d, "core_cockpit", 3, 0, 0).expect("p");
        bp.fit(&d, 0, 0, 0, 0, "chem_mk1").expect("fit");
        let r = analyze(&d, &bp);
        assert!(r.twr > 0.0);
        let g = landing(&r, 9.81 * 20.0);
        assert!(
            !g.can_take_off,
            "weak engine on a heavy world: {:?}",
            g.notes
        );
    }

    #[test]
    fn landing_needs_legs_around_the_centre_of_mass() {
        let d = data();
        let bp = plan_of(&d, "lander_m");
        let r = analyze(&d, &bp);
        assert!(r.gear_stable, "the lander stands: com {:?}", r.com_wet);
        assert!(landing(&r, 3.7).can_land);
        // Take the front leg away: two skids in a line cannot hold it.
        let mut two = bp.clone();
        two.remove(1, 0, 0);
        let r2 = analyze(&d, &two);
        assert!(!r2.gear_stable);
        assert!(!landing(&r2, 3.7).can_land);
        // Weak legs under a heavy ship.
        assert!(!landing(&r, 9.81 * 400.0).can_land, "crushed under 400 g");
    }

    #[test]
    fn inside_the_support_polygon() {
        let tri = [(0.0, 0.0), (10.0, 0.0), (5.0, 10.0)];
        assert!(inside_hull(&tri, (5.0, 3.0), 0.3));
        assert!(!inside_hull(&tri, (9.5, 8.0), 0.3));
        assert!(!inside_hull(&tri, (5.0, 0.1), 0.3), "too near an edge");
        assert!(
            !inside_hull(&[(0.0, 0.0), (5.0, 0.0), (10.0, 0.0)], (5.0, 0.0), 0.0),
            "a line"
        );
    }

    #[test]
    fn wings_and_the_centre_of_pressure() {
        let d = data();
        let r = analyze(&d, &plan_of(&d, "lander_m"));
        assert!(r.lift_area > 0.0 && r.wing_loading.is_finite());
        let c = analyze(&d, &plan_of(&d, "courier_x"));
        assert!(c.wing_loading.is_infinite(), "no wing: no lift");
        // The warning follows the sign of the margin.
        let mut bp = Blueprint::new(1, "t");
        bp.place(&d, "hull_armor", 0, 0, 0).expect("p");
        bp.place(&d, "hull_truss", 1, 0, 0).expect("p");
        bp.place(&d, "hull_truss", 2, 0, 0).expect("p");
        let r = analyze(&d, &bp);
        assert!(r.warnings.iter().any(|w| w.contains("instable")) == (r.aero_margin < 0.0));
    }

    #[test]
    fn cost_is_the_sum_of_materials_labour_and_modules() {
        let d = data();
        let bp = plan_of(&d, "courier_x");
        let r = analyze(&d, &bp);
        let (mut mats, mut labor) = (0.0, 0.0);
        for c in &bp.cells {
            let p = &d.ship_parts[d.ship_part_index(&c.part).expect("part")];
            labor += p.labor;
            for (res, kg) in &p.materials {
                mats += kg * d.resources[d.resource_index(res).expect("res")].value;
            }
        }
        let mods: f64 = bp
            .fits
            .iter()
            .map(|f| d.modules[d.module_index(&f.module).expect("m")].cost)
            .sum();
        assert!((r.value - (mats + labor * LABOR_DAY_CR + mods)).abs() < 1e-6);
        assert!((r.labor - labor).abs() < 1e-9);
        let total_kg: f64 = r.materials.iter().map(|(_, k)| k).sum();
        let want: f64 = bp
            .cells
            .iter()
            .map(|c| {
                d.ship_parts[d.ship_part_index(&c.part).expect("part")]
                    .materials
                    .iter()
                    .map(|m| m.1)
                    .sum::<f64>()
            })
            .sum();
        assert!((total_kg - want).abs() < 1e-6);
    }

    #[test]
    fn blueprints_round_trip_through_text() {
        let d = data();
        let bp = plan_of(&d, "hauler_x");
        let text = bp.to_text();
        let back = Blueprint::from_text(&d, &text).expect("read");
        assert_eq!(back, bp);
        assert!(Blueprint::from_text(&d, "pas du texte").is_err());
        // A plan naming a missing part is refused.
        let bad = text.replace("hold_large", "hold_inexistante");
        assert!(Blueprint::from_text(&d, &bad).is_err());
    }

    #[test]
    fn the_interior_follows_the_design_and_is_deterministic() {
        let d = data();
        let bp = plan_of(&d, "courier_x");
        let a = bp.interior(&d);
        assert_eq!(a, bp.interior(&d));
        // One room per walkable part of the hull deck, none for trusses, legs or nothing at all.
        assert_eq!(a.len(), 7);
        assert!(a.iter().any(|r| r.2 == "passerelle") && a.iter().any(|r| r.2 == "soute"));
        assert!(Blueprint::new(1, "t").interior(&d).is_empty());
    }
}
