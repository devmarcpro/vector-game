//! Military physics and strategy, on top of the weapons of `modules.ron` and the wars of
//! `politics.rs` (no parallel system): kinetic energy and dispersion, beam divergence and
//! absorption, passive thermal detection, abstract fleet battles (Lanchester square law fed by
//! the same weapon and hull data), blockades and espionage rolls. Everything is a pure function of
//! its inputs and of hashes of (seed, coordinates): never the real clock.

use crate::planet::unit;
use crate::politics::{self, Cell, WAR_PERIOD};
use crate::Sim;
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Vec3};
use vector_data::GameData;
use vector_world::galaxy::Galaxy;

/// Joules that make one point of damage (a 350-point hit is 3.5 MJ).
pub const DAMAGE_J: f64 = 1.0e4;
/// Irradiance (W/m^2) a passive thermal sensor can pick out of the background.
pub const THERMAL_THRESHOLD: f64 = 5.0e-6;
/// Range (m) under which an active sensor sees a ship whatever its heat.
pub const ACTIVE_RANGE: f64 = 5_000.0;

/// Kinetic energy of a projectile, J.
pub fn kinetic_energy_j(mass_kg: f64, speed: f64) -> f64 {
    0.5 * mass_kg * speed * speed
}

/// Damage points delivered by `joules` of energy.
pub fn damage_from_energy(joules: f64) -> f64 {
    joules / DAMAGE_J
}

/// Radius (m) of the disc in which a weapon of `spread_mrad` puts its shots at `dist` metres.
pub fn miss_radius(spread_mrad: f64, dist: f64) -> f64 {
    spread_mrad * 1e-3 * dist
}

/// Chance that a shot lands on a target of `radius` metres at `dist` metres (shots spread
/// uniformly over the dispersion disc).
pub fn hit_chance(spread_mrad: f64, dist: f64, radius: f64) -> f64 {
    let r = miss_radius(spread_mrad, dist);
    if r <= radius {
        1.0
    } else {
        (radius / r).powi(2)
    }
}

/// Radius (m) of a beam's spot at `dist`: the aperture plus diffraction, 1.22 lambda / D.
pub fn beam_spot_radius(wavelength_nm: f64, aperture: f64, dist: f64) -> f64 {
    aperture / 2.0 + 1.22 * wavelength_nm * 1e-9 / aperture * dist
}

/// Share of the beam power per unit area left at `dist` (1 at the muzzle, then falling as the
/// spot grows), before any absorption.
pub fn beam_focus(wavelength_nm: f64, aperture: f64, dist: f64) -> f64 {
    let s = beam_spot_radius(wavelength_nm, aperture, dist);
    ((aperture / 2.0) / s).powi(2)
}

/// Fraction of a beam that crosses `path` metres of air of `density` kg/m^3. Lasers lose little,
/// particle beams (wavelength under 100 nm: charged or neutral particles) scatter far more.
pub fn atmospheric_transmission(wavelength_nm: f64, density: f64, path: f64) -> f64 {
    let k = if wavelength_nm < 100.0 {
        2.0e-3
    } else {
        2.0e-4
    };
    (-k * density * path).exp()
}

/// Damage per second a beam really delivers at `dist` (focus and absorption).
pub fn beam_dps(base_dps: f64, wavelength_nm: f64, aperture: f64, dist: f64, density: f64) -> f64 {
    base_dps
        * beam_focus(wavelength_nm, aperture, dist)
        * atmospheric_transmission(wavelength_nm, density, dist)
}

/// Range (m) at which a passive sensor picks up a source of `power_kw` (inverse square law),
/// never under the active sensor range.
pub fn passive_detection_range(power_kw: f64) -> f64 {
    let p = power_kw.max(0.0) * 1000.0;
    (p / (4.0 * std::f64::consts::PI * THERMAL_THRESHOLD))
        .sqrt()
        .max(ACTIVE_RANGE)
}

/// Mean damage per second of the weapons of the data (what a typical gun does).
pub(crate) fn mean_weapon_dps(data: &GameData) -> f64 {
    let w: Vec<f64> = data
        .modules
        .iter()
        .filter(|m| matches!(m.weapon.as_str(), "beam" | "kinetic" | "missile"))
        .map(|m| {
            if m.weapon == "beam" {
                m.damage
            } else {
                m.damage * m.rate
            }
        })
        .collect();
    w.iter().sum::<f64>() / w.len().max(1) as f64
}

/// Rating of the reference warship of the data: geometric mean of its firepower (three
/// hardpoints of the mean weapon) and of the mean hull's structure.
pub fn ship_rating(data: &GameData) -> f64 {
    let hp = data.hulls.iter().map(|h| h.hp).sum::<f64>() / data.hulls.len().max(1) as f64;
    (3.0 * mean_weapon_dps(data) * hp).sqrt()
}

/// Result of an abstract battle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Battle {
    pub a_wins: bool,
    /// Fraction of each side's strength left at the end (0 for the loser).
    pub a_left: f64,
    pub b_left: f64,
}

/// Lanchester square law between two strengths, with a deterministic fog of war of +-10 % per
/// side drawn from `key`.
pub fn resolve_battle(a: f64, b: f64, seed: u64, key: i64) -> Battle {
    let ea = a * (0.9 + 0.2 * unit(hash_coord(seed ^ 0xA7, key, 1, 3)));
    let eb = b * (0.9 + 0.2 * unit(hash_coord(seed ^ 0xA7, key, 2, 3)));
    let left = |w: f64, l: f64| {
        if w <= 0.0 {
            0.0
        } else {
            ((w * w - l * l).max(0.0)).sqrt() / w
        }
    };
    if ea >= eb {
        Battle {
            a_wins: true,
            a_left: left(ea, eb),
            b_left: 0.0,
        }
    } else {
        Battle {
            a_wins: false,
            a_left: 0.0,
            b_left: left(eb, ea),
        }
    }
}

/// Consecutive wars between `c` and `nb` before period `m` (at most 10): how long supply lines
/// have been stretched.
fn war_age(data: &GameData, galaxy: &Galaxy, seed: u64, c: Cell, nb: Cell, m: i64) -> u32 {
    let (fc, fnb) = (
        politics::faction_of(data, galaxy, c),
        politics::faction_of(data, galaxy, nb),
    );
    (1..=10)
        .take_while(|i| politics::war_in_period(data, seed, c, nb, fc, fnb, m - i))
        .count() as u32
}

/// Fighting strength of a sector's fleet in period `m`: the number of hulls, the rating of the
/// reference warship, and supply (fuel and ammunition) that falls with the age of the war,
/// slower for the defender who fights close to home.
pub fn fleet_strength(
    data: &GameData,
    seed: u64,
    c: Cell,
    m: i64,
    age: u32,
    defender: bool,
) -> f64 {
    let n = 4.0 + (16.0 * unit(hash_coord(seed ^ 0xF1EE7, politics::key(c), m / 6, 5))).floor();
    let decay = if defender { 0.05 } else { 0.2 };
    n * ship_rating(data) / (1.0 + decay * f64::from(age))
}

/// One front of a war seen from sector `c`.
#[derive(Clone, Debug, PartialEq)]
pub struct Front {
    pub foe: Cell,
    /// Number of previous consecutive war periods.
    pub age: u32,
    pub own_strength: f64,
    pub foe_strength: f64,
    pub battle: Battle,
    pub own_doctrine: Doctrine,
    pub foe_doctrine: Doctrine,
}

impl Front {
    /// Whether `c` wins this front.
    pub fn own_wins(&self) -> bool {
        self.battle.a_wins
    }
}

/// The fronts of sector `c` at time `t`, one per neighbour at war with it. The sector with the
/// smaller key is the aggressor, the other defends; the battle is the same on both sides.
pub fn fronts(data: &GameData, galaxy: &Galaxy, seed: u64, c: Cell, t: f64) -> Vec<Front> {
    fronts_with(data, galaxy, seed, c, t, &[])
}

/// `fronts` with the sabotage and the hired fleets of the player (`fx`) applied to the sectors
/// they touch; both sides of a front read the same effects, so they still agree.
pub fn fronts_with(
    data: &GameData,
    galaxy: &Galaxy,
    seed: u64,
    c: Cell,
    t: f64,
    fx: &[FrontEffect],
) -> Vec<Front> {
    let m = (t / WAR_PERIOD).floor() as i64;
    let fc = politics::faction_of(data, galaxy, c);
    let mut out = Vec::new();
    for nb in politics::neighbours(c) {
        let fnb = politics::faction_of(data, galaxy, nb);
        if !politics::war_in_period(data, seed, c, nb, fc, fnb, m) {
            continue;
        }
        let age = war_age(data, galaxy, seed, c, nb, m);
        let c_attacks = politics::key(c) < politics::key(nb);
        let (dc, dn) = (faction_doctrine(data, fc), faction_doctrine(data, fnb));
        let sc = apply_effects(
            fleet_strength(data, seed, c, m, dc.supply_age(age), !c_attacks)
                * dc.strength_mult(!c_attacks),
            c,
            m,
            fx,
        );
        let sn = apply_effects(
            fleet_strength(data, seed, nb, m, dn.supply_age(age), c_attacks)
                * dn.strength_mult(c_attacks),
            nb,
            m,
            fx,
        );
        // Resolve once, in the frame of the smaller key, so both sides agree.
        let (lo, hi) = (
            politics::key(c).min(politics::key(nb)),
            politics::key(c).max(politics::key(nb)),
        );
        let key = lo ^ hi.rotate_left(13) ^ m;
        let battle = if c_attacks {
            resolve_battle(sc, sn, seed, key)
        } else {
            let b = resolve_battle(sn, sc, seed, key);
            Battle {
                a_wins: !b.a_wins,
                a_left: b.b_left,
                b_left: b.a_left,
            }
        };
        out.push(Front {
            foe: nb,
            age,
            own_strength: sc,
            foe_strength: sn,
            battle,
            own_doctrine: dc,
            foe_doctrine: dn,
        });
    }
    out
}

/// A sector is besieged when it is at war and loses a front: the victor's fleet cuts the lanes.
pub fn is_besieged(data: &GameData, galaxy: &Galaxy, seed: u64, c: Cell, t: f64) -> bool {
    fronts(data, galaxy, seed, c, t)
        .iter()
        .any(|f| !f.own_wins())
}

/// Price rise of every good in a blockaded sector.
pub const BLOCKADE_MARKUP: f64 = 0.3;

/// Outcome of a spy mission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpyResult {
    Success,
    Failure,
    /// The agent is caught: the target knows who sent them.
    Caught,
}

/// A mission rolled from hashes: odds grow with the agent's skill (0..1) and fall with the
/// target's counter-intelligence (0..1).
pub fn spy_mission(seed: u64, agent_skill: f64, counter_intel: f64, key: i64) -> SpyResult {
    let p_ok = (0.25 + 0.6 * agent_skill - 0.5 * counter_intel).clamp(0.02, 0.95);
    let roll = unit(hash_coord(seed ^ 0x59, key, 1, 7));
    if roll < p_ok {
        SpyResult::Success
    } else if unit(hash_coord(seed ^ 0x59, key, 2, 7)) < 0.3 + 0.5 * counter_intel {
        SpyResult::Caught
    } else {
        SpyResult::Failure
    }
}

// ---- hit zones: shields, reactive and ablative armour, hull, modules, crew -----------------

/// Energy (J) at which a hit punches through the bulkheads with 63 % of its force: a 300-point
/// kinetic slug (3 MJ) does, a beam that trickles a few points per tick does not.
pub const PEN_J: f64 = 3.0e6;
/// Share of a fully penetrating hit that reaches the cabins.
pub const CREW_MAX: f64 = 0.2;
/// Share of a beam's heat that an ablative layer carries away when it boils off.
pub const ABLATION_VS_BEAM: f64 = 0.5;
/// Cap on what reactive plates can cancel of a kinetic hit.
pub const REACTIVE_CAP: f64 = 0.75;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitKind {
    /// A slug, a missile, a mine: a chunk of energy delivered at once.
    Kinetic,
    /// A beam or a blast of heat: energy trickling in.
    Beam,
}

/// What the armour of a ship is made of, summed over its plates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Armour {
    /// Total armour rating (each point cuts a quarter off the damage).
    pub value: f64,
    /// Share (0..1) of the rating that is ablative.
    pub ablative: f64,
    /// Share of a kinetic hit the reactive plates cancel (0..`REACTIVE_CAP`).
    pub reactive: f64,
}

/// Where the energy of one hit goes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HitZones {
    pub shield: f64,
    pub hull: f64,
    pub modules: f64,
    pub crew: f64,
    /// Armour structure burnt away (ablation) and damage cancelled by reactive plates.
    pub ablated: f64,
    pub cancelled: f64,
}

impl HitZones {
    pub fn total_through(&self) -> f64 {
        self.hull + self.modules + self.crew
    }
}

/// Split `dmg` points (1 point = `DAMAGE_J` joules) over the zones of a ship: the shield first,
/// then reactive plates (against kinetic hits only), the armour rating, and whatever is left
/// goes to the cabins in proportion to how deep the hit's energy penetrates, the rest to the
/// hull and to the modules.
pub fn split_hit(dmg: f64, kind: HitKind, shield_pts: f64, armour: &Armour) -> HitZones {
    let mut z = HitZones::default();
    if dmg <= 0.0 {
        return z;
    }
    z.shield = shield_pts.max(0.0).min(dmg);
    let mut rest = dmg - z.shield;
    if kind == HitKind::Kinetic {
        z.cancelled = rest * armour.reactive.clamp(0.0, REACTIVE_CAP);
        rest -= z.cancelled;
    }
    if kind == HitKind::Beam {
        rest *= 1.0 - ABLATION_VS_BEAM * armour.ablative.clamp(0.0, 1.0);
    }
    let before = rest;
    rest /= 1.0 + 0.25 * armour.value.max(0.0);
    z.ablated = (before - rest) * armour.ablative.clamp(0.0, 1.0);
    if rest <= 0.0 {
        return z;
    }
    let pen = 1.0 - (-(rest * DAMAGE_J) / PEN_J).exp();
    z.crew = rest * CREW_MAX * pen;
    let body = rest - z.crew;
    z.hull = body * crate::combat::STRUCTURE_SHARE;
    z.modules = body * (1.0 - crate::combat::STRUCTURE_SHARE);
    z
}

// ---- decoys, seekers, active and passive sensors ------------------------------------------

/// A thermal flare: a seeker locks on the hottest source it sees.
#[derive(Clone, Copy, Debug)]
pub struct Decoy {
    pub pos: Vec3,
    pub vel: Vec3,
    pub heat_kw: f64,
    pub until: f64,
}

/// How long a flare burns (s).
pub const DECOY_LIFE: f64 = 14.0;
/// Distance (m) at which a missile blows up on a flare.
pub const DECOY_FUSE: f64 = 90.0;
/// Radiated power an active sensor adds to the ship's signature, per kW of sensor power: a
/// radar shouts, and anyone with a receiver hears it from far.
pub const ACTIVE_EMISSION: f64 = 8.0;
/// How much harder the first strike of an ambush lands.
pub const AMBUSH_MULT: f64 = 3.0;

/// Irradiance a seeker sees from `kw` at `dist` metres (inverse square law).
pub fn irradiance(kw: f64, dist: f64) -> f64 {
    kw.max(0.0) / (dist * dist).max(1.0)
}

/// What a heat-seeking missile at `from` locks on: `None` for the ship, `Some(i)` for decoy `i`
/// when it outshines the ship (a hot flare near the missile beats a cold ship far away).
pub fn seeker_pick(from: Vec3, ship_pos: Vec3, ship_kw: f64, decoys: &[Decoy]) -> Option<usize> {
    let mut best = irradiance(ship_kw, (ship_pos - from).length());
    let mut pick = None;
    for (i, d) in decoys.iter().enumerate() {
        let e = irradiance(d.heat_kw, (d.pos - from).length());
        if e > best {
            best = e;
            pick = Some(i);
        }
    }
    pick
}

/// Heat (kW) a non-player ship gives off: its engines and, if it fights, its guns.
pub fn npc_thermal_kw(role: crate::npc::NpcRole, level: u8) -> f64 {
    let s = crate::combat::npc_stats(role, level);
    60.0 + 0.3 * s.dps + 25.0 * f64::from(level)
}

/// How a ship is seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Contact {
    /// Beyond every sensor.
    Lost,
    /// Heard by the passive sensors alone: silent, but only the hottest are seen.
    Passive,
    /// Painted by the active sensors: far and sure, but the ship now shouts its position.
    Active,
}

impl Contact {
    pub fn label(self) -> &'static str {
        match self {
            Contact::Lost => "AVEUGLE",
            Contact::Passive => "PASSIF",
            Contact::Active => "ACTIF",
        }
    }
}

// ---- doctrine -------------------------------------------------------------------------------

/// How a fleet fights: the same doctrine drives the war of a faction and the raids of pirates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Doctrine {
    /// Strikes first and hard, breaks off late, but suffers when it is the one at home.
    Aggressive,
    /// Holds its ground: strong at home, weak when it attacks.
    Defensive,
    /// Hit and run: no great strength but it lives off short supply lines and flees early.
    Harassment,
}

impl Doctrine {
    pub fn label(self) -> &'static str {
        match self {
            Doctrine::Aggressive => "AGRESSIVE",
            Doctrine::Defensive => "DEFENSIVE",
            Doctrine::Harassment => "HARCELEMENT",
        }
    }

    /// Multiplier on a fleet's strength in a battle, depending on who attacks.
    pub fn strength_mult(self, defender: bool) -> f64 {
        match (self, defender) {
            (Doctrine::Aggressive, false) => 1.2,
            (Doctrine::Aggressive, true) => 0.95,
            (Doctrine::Defensive, false) => 0.85,
            (Doctrine::Defensive, true) => 1.25,
            (Doctrine::Harassment, _) => 1.0,
        }
    }

    /// Years of war the supply lines feel: raiders carry little and need little.
    pub fn supply_age(self, age: u32) -> u32 {
        if self == Doctrine::Harassment {
            age / 2
        } else {
            age
        }
    }

    /// What the doctrine means, with its numbers.
    pub fn text(self) -> String {
        format!(
            "Force attaquant {:.2} / defendant {:.2} ; lignes de ravitaillement {} ; rompt le combat sous {:.0} % de coque.",
            self.strength_mult(false),
            self.strength_mult(true),
            if self == Doctrine::Harassment {
                "deux fois plus longues"
            } else {
                "normales"
            },
            self.flee_below() * 100.0
        )
    }

    /// Hull fraction under which a ship of this doctrine breaks off.
    pub fn flee_below(self) -> f64 {
        match self {
            Doctrine::Aggressive => 0.12,
            Doctrine::Defensive => 0.3,
            Doctrine::Harassment => 0.5,
        }
    }
}

/// The doctrine of a faction: from how aggressive its mores are.
pub fn faction_doctrine(data: &GameData, faction: Option<usize>) -> Doctrine {
    match faction
        .and_then(|f| data.factions.get(f))
        .map(|f| f.aggression)
    {
        Some(a) if a >= 0.55 => Doctrine::Aggressive,
        Some(a) if a < 0.3 => Doctrine::Defensive,
        Some(_) => Doctrine::Harassment,
        None => Doctrine::Harassment,
    }
}

/// The doctrine of a pirate band (by the id of the ship that leads it).
pub fn pirate_doctrine(seed: u64, id: u32) -> Doctrine {
    match (unit(hash_coord(seed ^ 0xD0C7, i64::from(id), 3, 11)) * 3.0) as u32 {
        0 => Doctrine::Aggressive,
        1 => Doctrine::Defensive,
        _ => Doctrine::Harassment,
    }
}

/// A change to the strength of the fleet of a sector during one war period: sabotage (`mult`
/// under 1), mercenaries hired (`add`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrontEffect {
    pub cell: Cell,
    pub period: i64,
    pub mult: f64,
    pub add: f64,
}

fn apply_effects(strength: f64, c: Cell, m: i64, fx: &[FrontEffect]) -> f64 {
    let mut s = strength;
    for e in fx.iter().filter(|e| e.cell == c && e.period == m) {
        s *= e.mult;
    }
    for e in fx.iter().filter(|e| e.cell == c && e.period == m) {
        s += e.add;
    }
    s.max(0.0)
}

impl Sim {
    /// Heat the ship gives off (kW): half the power its powered modules draw, plus the flash of
    /// the weapons that fired within their last cooldown. A ship with everything cut is cold.
    pub fn thermal_signature_kw(&self) -> f64 {
        let data = &self.data;
        let mut kw = 0.0;
        for (slot, m) in self.ship.design.modules(data) {
            if self.ship.sys.powered[slot] && m.power < 0.0 {
                kw += -m.power * 0.5;
            }
            if m.flash > 0.0 && self.weapon_cd.get(slot).copied().unwrap_or(0.0) > 0.0 {
                kw += m.flash;
            }
        }
        kw
    }

    /// Distance (m) at which passive sensors see this ship.
    pub fn detection_range(&self) -> f64 {
        passive_detection_range(self.emission_kw())
    }

    /// Plant a false rumour that will be heard like any news (the player cannot tell it apart).
    pub fn plant_rumor(&mut self, tag: u8, dist_ly: f64, text: String) {
        let t = self.time;
        self.news_push(t, tag, dist_ly, text);
    }
}

// ---- espionage, hired fleets, treaties, war crimes, sieges: the persistent state ------------

pub const AGENT_COST: f64 = 4000.0;
pub const INFILTRATE_COST: f64 = 1500.0;
/// How far from the current sector an agent can be slipped (sectors).
pub const SPY_REACH: i64 = 2;
pub const INTEL_DAYS: f64 = 30.0;
pub const DRONE_INTEL_DAYS: f64 = 10.0;
/// Days a ceasefire holds, and the crimes of war after which a people refuses to talk.
pub const PEACE_DAYS: f64 = 120.0;
pub const CRIMES_REFUSE: usize = 3;
/// Standing (old -100..100 scale) under which a people is at war with the player.
pub const WAR_STANDING: f32 = -60.0;
/// Mercenary contracts: days, and credits a day per enemy hull.
pub const MERC_DAYS: f64 = 30.0;
pub const MERC_PAY_PER_HULL: f64 = 800.0;
/// Orbital batteries: damage per gun and second, reach (m).
pub const BATTERY_DPS: f64 = 25.0;
pub const BATTERY_RANGE: f64 = 60_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState {
    /// At home, ready to be sent.
    Idle,
    /// Under cover in a rival sector.
    Infiltrated,
    /// Held by the counter-intelligence of the target.
    Captured,
    /// Cover blown: lost.
    Burned,
}

impl AgentState {
    pub fn label(self) -> &'static str {
        match self {
            AgentState::Idle => "AU REPOS",
            AgentState::Infiltrated => "INFILTRE",
            AgentState::Captured => "CAPTURE",
            AgentState::Burned => "GRILLE",
        }
    }
}

/// A spy of the player, kept from one mission to the next.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Agent {
    pub id: u32,
    pub name: String,
    pub skill: f64,
    pub state: AgentState,
    pub cell: Cell,
    /// 1 = unsuspected, 0 = blown.
    pub cover: f64,
    pub ops: u32,
}

/// What an infiltrated agent can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpyOp {
    /// Weaken the fleet of the sector for the war period.
    Sabotage,
    /// Copy the plans of the local arsenals: research points.
    StealPlans,
    /// Listen: learn the real fleets and doctrines of the fronts.
    Wiretap,
    /// Spread a false rumour about the people that holds the sector.
    Rumor,
}

pub const SPY_OPS: [SpyOp; 4] = [
    SpyOp::Sabotage,
    SpyOp::StealPlans,
    SpyOp::Wiretap,
    SpyOp::Rumor,
];

impl SpyOp {
    pub fn label(self) -> &'static str {
        match self {
            SpyOp::Sabotage => "SABOTAGE",
            SpyOp::StealPlans => "VOL DE PLANS",
            SpyOp::Wiretap => "ECOUTES",
            SpyOp::Rumor => "RUMEUR",
        }
    }
    /// What the operation does, with its price and the cover it burns.
    pub fn text(self) -> String {
        let what = match self {
            SpyOp::Sabotage => {
                "affaiblit la flotte du secteur pour la periode de guerre (jusqu'a -30 %)"
            }
            SpyOp::StealPlans => "copie les plans des arsenaux : de 150 a 500 points de recherche",
            SpyOp::Wiretap => {
                "ecoute les etats-majors : revele forces et doctrines des fronts pendant 30 jours"
            }
            SpyOp::Rumor => {
                "repand une fausse nouvelle de defaite, entendue comme n'importe quelle autre"
            }
        };
        format!(
            "Un agent infiltre {what}. Cout {:.0} CR ; use {:.0} % de sa couverture ; echec ou capture selon son talent et la contre-espionnage du secteur.",
            self.cost(),
            self.cover_loss() * 100.0
        )
    }

    pub fn cost(self) -> f64 {
        match self {
            SpyOp::Sabotage => 5000.0,
            SpyOp::StealPlans => 3000.0,
            SpyOp::Wiretap => 800.0,
            SpyOp::Rumor => 600.0,
        }
    }
    /// Cover an attempt uses up, whatever its outcome.
    pub fn cover_loss(self) -> f64 {
        match self {
            SpyOp::Sabotage => 0.5,
            SpyOp::StealPlans => 0.35,
            SpyOp::Wiretap => 0.1,
            SpyOp::Rumor => 0.2,
        }
    }
}

/// Counter-intelligence of a sector (0..1): warlike peoples watch their own more closely.
pub fn counter_intel(data: &GameData, galaxy: &Galaxy, seed: u64, c: Cell) -> f64 {
    let a = politics::faction_of(data, galaxy, c)
        .and_then(|f| data.factions.get(f))
        .map_or(0.1, |f| f.aggression);
    (0.15 + 0.5 * a + 0.1 * unit(hash_coord(seed ^ 0xC1, politics::key(c), 5, 5))).clamp(0.0, 0.95)
}

/// A hired fleet: ships of the player fighting for a sector for a while.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Merc {
    pub id: u32,
    pub employer: Cell,
    pub foe: Cell,
    pub faction: Option<usize>,
    pub ships: Vec<u32>,
    pub pay_day: f64,
    pub starts: f64,
    pub ends: f64,
}

/// A contract the sector under war proposes.
#[derive(Clone, Debug, PartialEq)]
pub struct MercOffer {
    pub employer: Cell,
    pub foe: Cell,
    pub faction: Option<usize>,
    pub pay_day: f64,
    pub days: f64,
    pub own_strength: f64,
    pub foe_strength: f64,
}

/// A ceasefire with a people.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Treaty {
    pub faction: usize,
    pub until: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WarCrime {
    pub time: f64,
    pub faction: Option<usize>,
    pub what: String,
}

/// The ship the squad is fighting aboard (kept while the walk lasts).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Boarding {
    pub npc_id: u32,
    pub name: String,
    pub role: crate::npc::NpcRole,
    pub faction: Option<usize>,
    pub level: u8,
    pub hull_max: f64,
    pub cargo: Option<(usize, f64)>,
}

/// A sector that loses a front of a war: the victor's fleet cuts its lanes.
#[derive(Clone, Debug, PartialEq)]
pub struct Siege {
    pub cell: Cell,
    pub faction: Option<usize>,
    pub besieger: Option<usize>,
    pub foe: Cell,
    pub age: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Military {
    pub agents: Vec<Agent>,
    pub next_agent: u32,
    pub mercs: Vec<Merc>,
    pub next_merc: u32,
    pub treaties: Vec<Treaty>,
    pub crimes: Vec<WarCrime>,
    /// Value (credits) of the ships of each people the player destroyed: what peace costs.
    pub destroyed: Vec<(usize, f64)>,
    pub effects: Vec<FrontEffect>,
    /// Sectors the player knows the fleets of, until when.
    pub intel: Vec<(Cell, f64)>,
    pub boarding: Option<Boarding>,
    pub rumors: u32,
    /// Ambushes sprung by the player, and the time of the last one.
    pub ambushes: u32,
    pub last_ambush: f64,
    pub last_day: i64,
    pub last_battery_day: i64,
    /// Sieges around the ship (refreshed daily, not saved).
    #[serde(skip)]
    pub sieges: Vec<Siege>,
    #[serde(skip)]
    pub siege_key: Option<(i64, Cell)>,
}

impl Military {
    pub fn crimes_against(&self, faction: usize) -> usize {
        self.crimes
            .iter()
            .filter(|c| c.faction == Some(faction))
            .count()
    }
    pub fn destroyed_value(&self, faction: usize) -> f64 {
        self.destroyed
            .iter()
            .find(|d| d.0 == faction)
            .map_or(0.0, |d| d.1)
    }
    pub fn knows(&self, c: Cell, t: f64) -> bool {
        self.intel.iter().any(|i| i.0 == c && i.1 > t)
    }
}

/// An orbital defence: how many guns a sector keeps in orbit, and what they do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Battery {
    pub guns: u32,
    pub dps: f64,
    pub range: f64,
}

/// The batteries of a sector at time `t`: 2 to 9 guns, halved while the sector is besieged
/// (the victor has already smashed half of them).
pub fn orbital_battery(data: &GameData, galaxy: &Galaxy, seed: u64, c: Cell, t: f64) -> Battery {
    let mut guns = 2 + (7.0 * unit(hash_coord(seed ^ 0xBA77, politics::key(c), 9, 9))) as u32;
    if is_besieged(data, galaxy, seed, c, t) {
        guns = guns.div_ceil(2);
    }
    Battery {
        guns,
        dps: BATTERY_DPS,
        range: BATTERY_RANGE,
    }
}

/// Fighting value of a hull used as a warship, in units of the reference warship of the data:
/// firepower follows its hardpoints, endurance its structure.
pub fn warship_rating(data: &GameData, hull: usize) -> f64 {
    let h = &data.hulls[hull];
    let points: f64 = h
        .slots
        .iter()
        .filter(|(s, _)| matches!(s, vector_data::SlotType::Hardpoint))
        .map(|(_, n)| f64::from(*n))
        .sum();
    let rating = (points.max(0.5) * mean_weapon_dps(data) * h.hp).sqrt();
    rating / ship_rating(data)
}

/// Hardpoints of a hull.
pub fn hardpoints(data: &GameData, hull: usize) -> f64 {
    data.hulls[hull]
        .slots
        .iter()
        .filter(|(s, _)| matches!(s, vector_data::SlotType::Hardpoint))
        .map(|(_, n)| f64::from(*n))
        .sum()
}

impl Sim {
    // ---- hit zones ------------------------------------------------------------------------

    /// What the plates of the ship are made of.
    pub fn armour_profile(&self) -> Armour {
        let data = &self.data;
        let mut a = Armour {
            value: self.effective.armor,
            ..Armour::default()
        };
        let (mut total, mut abl) = (0.0, 0.0);
        for (slot, m) in self.ship.design.modules(data) {
            if m.category != "armor" || self.ship.sys.health[slot] <= 0.0 {
                continue;
            }
            total += m.armor;
            abl += m.armor * m.ablative;
            a.reactive += 0.5 * m.reactive * f64::from(m.size) * self.ship.sys.health[slot];
        }
        a.ablative = if total > 0.0 { abl / total } else { 0.0 };
        a.reactive = a.reactive.min(REACTIVE_CAP);
        a
    }

    /// Burn away `amount` structure points of the ablative plates.
    pub(crate) fn ablate_armour(&mut self, amount: f64) {
        if amount <= 0.0 {
            return;
        }
        let data = std::sync::Arc::clone(&self.data);
        let plates: Vec<usize> = self
            .ship
            .design
            .modules(&data)
            .filter(|(_, m)| m.category == "armor" && m.ablative > 0.0)
            .map(|(i, _)| i)
            .collect();
        for &i in &plates {
            let m = &data.modules[self.ship.design.slots[i].unwrap_or(0)];
            let h = &mut self.ship.sys.health[i];
            *h = (*h - amount / plates.len() as f64 / m.hp.max(1.0)).max(0.0);
        }
    }

    // ---- ambush ---------------------------------------------------------------------------

    /// Heat of the ship's own machinery, without the flash of a gun that has just fired.
    pub fn quiet_signature_kw(&self) -> f64 {
        self.ship
            .design
            .modules(&self.data)
            .filter(|(i, m)| self.ship.sys.powered[*i] && m.power < 0.0)
            .map(|(_, m)| -m.power * 0.5)
            .sum::<f64>()
            + if self.active_sensors {
                self.sensor_power_kw() * ACTIVE_EMISSION
            } else {
                0.0
            }
    }

    /// Whether a blow on NPC `k` is an ambush: the target is not on alert, was not provoked,
    /// and sits beyond the distance at which it could feel the ship's heat. A ship that has cut
    /// its machinery is a cold shadow; the first strike on an unwary target lands `AMBUSH_MULT`
    /// times harder.
    pub fn is_ambush(&self, k: usize) -> bool {
        let Some(n) = self.npcs.get(k) else {
            return false;
        };
        !matches!(n.state, crate::npc::NpcState::Combat { .. })
            && !n.provoked
            && !n.disabled
            && (n.pos - self.ship.pos).length() > passive_detection_range(self.quiet_signature_kw())
    }

    pub(crate) fn note_ambush(&mut self) {
        self.military.ambushes += 1;
        let t = self.time;
        if t - self.military.last_ambush > 60.0 || self.military.ambushes == 1 {
            self.news_push(
                t,
                1,
                0.0,
                "Embuscade : un vaisseau froid a frappe le premier.".to_string(),
            );
        }
        self.military.last_ambush = t;
    }

    // ---- decoys and sensors ---------------------------------------------------------------

    /// Fire one thermal flare from the first powered launcher that has a round.
    pub fn launch_decoy(&mut self) -> Result<(), &'static str> {
        let data = std::sync::Arc::clone(&self.data);
        let slot = self
            .ship
            .design
            .modules(&data)
            .find(|(i, m)| {
                m.weapon == "decoy"
                    && self.ship.sys.powered[*i]
                    && self.ship.sys.health[*i] > 0.0
                    && self.weapon_cd.get(*i).copied().unwrap_or(0.0) <= 0.0
            })
            .map(|(i, _)| i)
            .ok_or("aucun lance-leurres pret")?;
        let m = &data.modules[self.ship.design.slots[slot].unwrap_or(0)];
        let ammo = data.resource_index(&m.ammo).ok_or("munition inconnue")?;
        if self.ship.cargo.kg[ammo] + 1e-9 < m.ammo_use {
            return Err("plus de leurres en soute");
        }
        if !self.rules.infinite_resources {
            self.ship.cargo.remove(&data, ammo, m.ammo_use);
        }
        if let Some(cd) = self.weapon_cd.get_mut(slot) {
            *cd = 1.0 / m.rate.max(0.01);
        }
        let h = hash_coord(
            self.system.seed,
            (self.time * 100.0) as i64,
            slot as i64,
            0xDEC0,
        );
        let side = self.ship.rot.col(0) * if h & 1 == 0 { 1.0 } else { -1.0 };
        self.decoys.push(Decoy {
            pos: self.ship.pos + side * (SHIP_RADIUS_M + 20.0),
            vel: self.ship.vel + side * 40.0,
            heat_kw: m.decoy,
            until: self.time + DECOY_LIFE,
        });
        if self.decoys.len() > 24 {
            self.decoys.remove(0);
        }
        Ok(())
    }

    pub(crate) fn decoys_tick(&mut self, dt: f64) {
        let now = self.time;
        self.decoys.retain(|d| d.until > now);
        for d in &mut self.decoys {
            d.pos = d.pos + d.vel * dt;
            // The flare cools as it burns out.
            d.heat_kw *= 1.0 - 0.02 * dt;
        }
    }

    /// Switch the active sensors on or off (they need an installed, powered radar).
    pub fn toggle_active_sensors(&mut self) -> Result<bool, &'static str> {
        if !self.active_sensors && self.sensor_power_kw() <= 0.0 {
            return Err("aucun radar actif installe");
        }
        self.active_sensors = !self.active_sensors;
        Ok(self.active_sensors)
    }

    /// Power (kW) of the powered active sensors installed.
    pub fn sensor_power_kw(&self) -> f64 {
        self.ship
            .design
            .modules(&self.data)
            .filter(|(i, m)| {
                m.sensor > 0.0 && self.ship.sys.powered[*i] && self.ship.sys.health[*i] > 0.0
            })
            .map(|(_, m)| m.sensor)
            .sum()
    }

    /// Reach (m) of the active sensors, 0 when off.
    pub fn active_reach(&self) -> f64 {
        if !self.active_sensors {
            return 0.0;
        }
        self.ship
            .design
            .modules(&self.data)
            .filter(|(i, m)| {
                m.sensor > 0.0 && self.ship.sys.powered[*i] && self.ship.sys.health[*i] > 0.0
            })
            .map(|(_, m)| m.scan_range)
            .fold(0.0, f64::max)
    }

    /// Heat and radio noise the ship gives off: with the radar on, it shouts.
    pub fn emission_kw(&self) -> f64 {
        let radar = if self.active_sensors {
            self.sensor_power_kw() * ACTIVE_EMISSION
        } else {
            0.0
        };
        self.thermal_signature_kw() + radar
    }

    /// How the player's sensors hold NPC `k`.
    pub fn contact(&self, k: usize) -> Contact {
        let Some(n) = self.npcs.get(k) else {
            return Contact::Lost;
        };
        let d = (n.pos - self.ship.pos).length();
        if d <= passive_detection_range(npc_thermal_kw(n.role, n.level)) {
            Contact::Passive
        } else if d <= self.active_reach() {
            Contact::Active
        } else {
            Contact::Lost
        }
    }

    // ---- espionage --------------------------------------------------------------------------

    pub fn current_cell(&self) -> Cell {
        politics::cell_of(self.here_pos)
    }

    /// Short label of a sector: its people and its coordinates.
    pub fn cell_label(&self, c: Cell) -> String {
        let who = politics::faction_of(&self.data, &self.galaxy, c)
            .map_or("ESPACE LIBRE".to_string(), |f| {
                self.data.factions[f].name.to_uppercase()
            });
        format!("{who} [{} {} {}]", c[0], c[1], c[2])
    }

    fn day_index(&self) -> i64 {
        (self.time / crate::crew::DAY).floor() as i64
    }

    /// Hire a spy at a station (a person of the local culture, with a hidden talent).
    pub fn recruit_agent(&mut self) -> Result<u32, &'static str> {
        self.docked_station()?;
        if self.credits < AGENT_COST {
            return Err("fonds insuffisants");
        }
        self.credits -= AGENT_COST;
        let id = self.military.next_agent + 1;
        self.military.next_agent = id;
        let h = hash_coord(self.galaxy.seed, i64::from(id), self.day_index(), 0xA6E7);
        let culture = vector_world::names::culture_id(h);
        let name = format!(
            "{} {}",
            vector_world::names::given_name(culture, h & 1 == 0, h >> 3),
            vector_world::names::surname(culture, h >> 11)
        );
        self.military.agents.push(Agent {
            id,
            name,
            skill: 0.2 + 0.6 * unit(h ^ 0x5C177),
            state: AgentState::Idle,
            cell: [0; 3],
            cover: 1.0,
            ops: 0,
        });
        Ok(id)
    }

    fn agent_index(&self, id: u32) -> Result<usize, &'static str> {
        self.military
            .agents
            .iter()
            .position(|a| a.id == id)
            .ok_or("agent inconnu")
    }

    fn spy_key(&self, a: &Agent, salt: i64) -> i64 {
        hash_coord(
            self.galaxy.seed,
            i64::from(a.id),
            i64::from(a.ops) + salt,
            self.day_index(),
        ) as i64
    }

    /// Slip agent `id` into sector `cell`.
    pub fn infiltrate(&mut self, id: u32, cell: Cell) -> Result<String, &'static str> {
        let i = self.agent_index(id)?;
        if self.military.agents[i].state != AgentState::Idle {
            return Err("l'agent n'est pas disponible");
        }
        if !politics::cells_around(self.current_cell(), SPY_REACH).contains(&cell) {
            return Err("secteur hors de portee de vos reseaux");
        }
        if self.credits < INFILTRATE_COST {
            return Err("fonds insuffisants");
        }
        self.credits -= INFILTRATE_COST;
        let a = self.military.agents[i].clone();
        let ci = counter_intel(&self.data, &self.galaxy, self.galaxy.seed, cell);
        let key = self.spy_key(&a, 0);
        let label = self.cell_label(cell);
        let people = politics::faction_of(&self.data, &self.galaxy, cell);
        let res = spy_mission(self.galaxy.seed, a.skill, ci, key);
        let ag = &mut self.military.agents[i];
        ag.ops += 1;
        match res {
            SpyResult::Success => {
                ag.state = AgentState::Infiltrated;
                ag.cell = cell;
                ag.cover = 1.0;
                Ok(format!("{} S'EST INFILTRE : {label}", ag.name))
            }
            SpyResult::Failure => Ok(format!("{} A RENONCE : TROP DE CONTROLES", ag.name)),
            SpyResult::Caught => {
                ag.state = AgentState::Captured;
                let name = ag.name.clone();
                self.change_reputation(people, -15.0);
                let t = self.time;
                self.news_push(
                    t,
                    6,
                    0.0,
                    format!("Un espion a ete arrete a la frontiere de {label} : il parle de vous."),
                );
                Ok(format!("{name} A ETE CAPTURE ({label} SAIT QUI L'ENVOIE)"))
            }
        }
    }

    /// Bring an agent home (the cover is lost, the man is safe).
    pub fn withdraw_agent(&mut self, id: u32) -> Result<(), &'static str> {
        let i = self.agent_index(id)?;
        let a = &mut self.military.agents[i];
        if a.state != AgentState::Infiltrated {
            return Err("l'agent n'est pas sur le terrain");
        }
        a.state = AgentState::Idle;
        Ok(())
    }

    /// An operation of an infiltrated agent. The roll uses the agent's skill, the counter
    /// intelligence of the sector and the cover left; every attempt burns some cover.
    pub fn spy_op(&mut self, id: u32, op: SpyOp) -> Result<String, &'static str> {
        let i = self.agent_index(id)?;
        let a = self.military.agents[i].clone();
        if a.state != AgentState::Infiltrated {
            return Err("l'agent n'est pas infiltre");
        }
        if self.credits < op.cost() {
            return Err("fonds insuffisants");
        }
        self.credits -= op.cost();
        let ci = counter_intel(&self.data, &self.galaxy, self.galaxy.seed, a.cell);
        let key = self.spy_key(&a, 17);
        // A thin cover raises the odds of being seen.
        let res = spy_mission(self.galaxy.seed, a.skill * (0.4 + 0.6 * a.cover), ci, key);
        let people = politics::faction_of(&self.data, &self.galaxy, a.cell);
        let label = self.cell_label(a.cell);
        let now = self.time;
        {
            let ag = &mut self.military.agents[i];
            ag.ops += 1;
            ag.cover = (ag.cover - op.cover_loss()).max(0.0);
        }
        match res {
            SpyResult::Caught => {
                self.military.agents[i].state = AgentState::Captured;
                self.change_reputation(people, -15.0);
                self.news_push(
                    now,
                    6,
                    0.0,
                    format!(
                        "Un agent etranger a ete pris sur le fait a {label} : {}.",
                        op.label().to_lowercase()
                    ),
                );
                return Ok(format!("{} : AGENT CAPTURE", op.label()));
            }
            SpyResult::Failure => return Ok(format!("{} : ECHEC, L'OPERATION AVORTE", op.label())),
            SpyResult::Success => {}
        }
        let m = (now / WAR_PERIOD).floor() as i64;
        let msg = match op {
            SpyOp::Sabotage => {
                let mult = 1.0 - 0.3 * (0.5 + 0.5 * a.skill);
                self.military.effects.push(FrontEffect {
                    cell: a.cell,
                    period: m,
                    mult,
                    add: 0.0,
                });
                self.military.sieges.clear();
                self.military.siege_key = None;
                format!(
                    "SABOTAGE REUSSI : LA FLOTTE DE {label} PERD {:.0} %",
                    (1.0 - mult) * 100.0
                )
            }
            SpyOp::StealPlans => {
                let pts = 150.0 + 350.0 * a.skill;
                self.research_pool += pts;
                self.news_push(
                    now,
                    5,
                    0.0,
                    format!("Des plans d'armement ont disparu des arsenaux de {label}."),
                );
                format!("PLANS VOLES : +{pts:.0} POINTS DE RECHERCHE")
            }
            SpyOp::Wiretap => {
                self.military
                    .intel
                    .push((a.cell, now + INTEL_DAYS * crate::crew::DAY));
                let n = self.report_fronts(a.cell);
                format!("ECOUTES : {n} FRONT(S) DANS LES COMMUNICATIONS DE {label}")
            }
            SpyOp::Rumor => {
                self.military.rumors += 1;
                let who = people.map_or("une flotte sans drapeau".to_string(), |f| {
                    format!("la flotte de {}", self.data.factions[f].name)
                });
                let ly = {
                    let c = politics::cell_center(a.cell);
                    ((c[0] - self.here_pos[0]).powi(2)
                        + (c[1] - self.here_pos[1]).powi(2)
                        + (c[2] - self.here_pos[2]).powi(2))
                    .sqrt()
                };
                self.plant_rumor(
                    1,
                    ly,
                    format!("On murmure que {who} aurait subi de lourdes pertes."),
                );
                "RUMEUR LANCEE : LA FAUSSE NOUVELLE COURT".to_string()
            }
        };
        Ok(msg)
    }

    /// The sectors within reach of the networks that are at war, nearest first.
    pub fn war_cells(&self) -> Vec<Cell> {
        let here = self.current_cell();
        let mut v: Vec<Cell> = politics::cells_around(here, SPY_REACH)
            .into_iter()
            .filter(|c| {
                !fronts_with(
                    &self.data,
                    &self.galaxy,
                    self.galaxy.seed,
                    *c,
                    self.time,
                    &self.military.effects,
                )
                .is_empty()
            })
            .collect();
        v.sort_by_key(|c| (c[0] - here[0]).abs() + (c[1] - here[1]).abs() + (c[2] - here[2]).abs());
        v
    }

    /// Put the real fronts of `cell` in the journal; returns how many there are.
    fn report_fronts(&mut self, cell: Cell) -> usize {
        let t = self.time;
        let fx = self.military.effects.clone();
        let fs = fronts_with(&self.data, &self.galaxy, self.galaxy.seed, cell, t, &fx);
        let label = self.cell_label(cell);
        for f in &fs {
            let foe = self.cell_label(f.foe);
            self.news_push(
                t,
                1,
                0.0,
                format!(
                    "Renseignement : {label} ({}) {:.0} contre {foe} ({}) {:.0}, front vieux de {} periodes.",
                    f.own_doctrine.label().to_lowercase(),
                    f.own_strength,
                    f.foe_doctrine.label().to_lowercase(),
                    f.foe_strength,
                    f.age
                ),
            );
        }
        fs.len()
    }

    /// Send a reconnaissance drone (one unit of robotics) over a sector next to the ship.
    pub fn launch_recon_drone(&mut self, cell: Cell) -> Result<usize, &'static str> {
        if !politics::cells_around(self.current_cell(), 1).contains(&cell) {
            return Err("hors de portee du drone");
        }
        let robotics = self
            .data
            .resource_index("robotics")
            .ok_or("pas de robotique")?;
        if self.ship.cargo.kg[robotics] < 1.0 {
            return Err("il faut une unite de robotique en soute");
        }
        let data = std::sync::Arc::clone(&self.data);
        self.ship.cargo.remove(&data, robotics, 1.0);
        let until = self.time + DRONE_INTEL_DAYS * crate::crew::DAY;
        self.military.intel.push((cell, until));
        Ok(self.report_fronts(cell))
    }

    // ---- sieges and the war on the map ------------------------------------------------------

    /// Recompute the sieges around the ship (cheap enough once a day).
    pub fn refresh_sieges(&mut self) {
        let t = self.time;
        let here = self.current_cell();
        let m = (t / WAR_PERIOD).floor() as i64;
        let mut out = Vec::new();
        for c in politics::cells_around(here, 2) {
            for f in fronts_with(
                &self.data,
                &self.galaxy,
                self.galaxy.seed,
                c,
                t,
                &self.military.effects,
            ) {
                if !f.own_wins() {
                    out.push(Siege {
                        cell: c,
                        faction: politics::faction_of(&self.data, &self.galaxy, c),
                        besieger: politics::faction_of(&self.data, &self.galaxy, f.foe),
                        foe: f.foe,
                        age: f.age,
                    });
                }
            }
        }
        // Tell the journal about each siege seen for the first time this period.
        let fresh = self.military.siege_key.is_none_or(|k| k.0 != m);
        if fresh {
            for s in out.iter().filter(|s| s.cell == here) {
                let by = s.besieger.map_or("une flotte inconnue".to_string(), |f| {
                    format!("la flotte de {}", self.data.factions[f].name)
                });
                let label = self.cell_label(s.cell);
                self.news_push(
                    t,
                    1,
                    0.0,
                    format!("Blocus de {label} : {by} coupe les routes ; les prix montent."),
                );
            }
        }
        self.military.sieges = out;
        self.military.siege_key = Some((m, here));
    }

    /// Whether the sector the ship is in is under siege.
    pub fn here_besieged(&self) -> Option<&Siege> {
        let here = self.current_cell();
        self.military.sieges.iter().find(|s| s.cell == here)
    }

    // ---- hired fleets -----------------------------------------------------------------------

    /// What the sector the ship is docked in proposes: it must be at war.
    pub fn merc_offers(&self) -> Vec<MercOffer> {
        let here = self.current_cell();
        let t = self.time;
        let unit_rating = ship_rating(&self.data);
        fronts_with(
            &self.data,
            &self.galaxy,
            self.galaxy.seed,
            here,
            t,
            &self.military.effects,
        )
        .into_iter()
        .map(|f| MercOffer {
            employer: here,
            foe: f.foe,
            faction: politics::faction_of(&self.data, &self.galaxy, here),
            pay_day: MERC_PAY_PER_HULL * (f.foe_strength / unit_rating).max(1.0),
            days: MERC_DAYS,
            own_strength: f.own_strength,
            foe_strength: f.foe_strength,
        })
        .collect()
    }

    /// Hire the ships (ids of `fleet`) out for the offer `k`.
    pub fn hire_out(&mut self, k: usize, ships: &[u32]) -> Result<u32, &'static str> {
        self.docked_station()?;
        let offer = self
            .merc_offers()
            .into_iter()
            .nth(k)
            .ok_or("aucune offre")?;
        if ships.is_empty() {
            return Err("aucun vaisseau designe");
        }
        let mut add = 0.0;
        for id in ships {
            let s = self
                .fleet
                .iter()
                .find(|s| s.id == *id)
                .ok_or("vaisseau inconnu")?;
            if s.order != crate::fleet::FleetOrder::Trade || s.route.is_some() {
                return Err("un vaisseau est deja affecte");
            }
            if !matches!(s.state, crate::fleet::FleetState::Idle) {
                return Err("un vaisseau est en mission");
            }
            add += warship_rating(&self.data, s.hull) * s.condition * s.ammo;
        }
        for s in self.fleet.iter_mut().filter(|s| ships.contains(&s.id)) {
            s.order = crate::fleet::FleetOrder::Mercenary;
        }
        let t = self.time;
        let id = self.military.next_merc + 1;
        self.military.next_merc = id;
        let unit_rating = ship_rating(&self.data);
        let ends = t + offer.days * crate::crew::DAY;
        // The hired hulls count in every war period the contract touches.
        for m in [
            (t / WAR_PERIOD).floor() as i64,
            ((ends - 1.0) / WAR_PERIOD).floor() as i64,
        ] {
            if !self.military.effects.iter().any(|e| {
                e.cell == offer.employer
                    && e.period == m
                    && e.add == add * unit_rating
                    && e.mult == 1.0
            }) {
                self.military.effects.push(FrontEffect {
                    cell: offer.employer,
                    period: m,
                    mult: 1.0,
                    add: add * unit_rating,
                });
            }
        }
        self.military.mercs.push(Merc {
            id,
            employer: offer.employer,
            foe: offer.foe,
            faction: offer.faction,
            ships: ships.to_vec(),
            pay_day: offer.pay_day,
            starts: t,
            ends,
        });
        self.military.sieges.clear();
        self.military.siege_key = None;
        Ok(id)
    }

    /// Settle every contract that has run its course: pay, losses, standing.
    fn settle_mercs(&mut self) {
        let now = self.time;
        let due: Vec<usize> = (0..self.military.mercs.len())
            .filter(|&i| self.military.mercs[i].ends <= now)
            .collect();
        for i in due.into_iter().rev() {
            let c = self.military.mercs.remove(i);
            let fx = self.military.effects.clone();
            let front = fronts_with(
                &self.data,
                &self.galaxy,
                self.galaxy.seed,
                c.employer,
                c.ends - 1.0,
                &fx,
            )
            .into_iter()
            .find(|f| f.foe == c.foe);
            // No front left means the war ended on its own: the hulls come home whole.
            let (won, left) = front.map_or((true, 1.0), |f| (f.own_wins(), f.battle.a_left));
            let days = (c.ends - c.starts) / crate::crew::DAY;
            let pay = c.pay_day * days * if won { 1.5 } else { 0.5 };
            self.credits += pay;
            let mut lost = 0;
            for id in &c.ships {
                let roll = unit(hash_coord(
                    self.galaxy.seed ^ 0x3E7C,
                    i64::from(c.id),
                    i64::from(*id),
                    3,
                ));
                let death = if won { (1.0 - left) * 0.8 } else { 0.65 };
                if let Some(p) = self.fleet.iter().position(|s| s.id == *id) {
                    if roll < death {
                        self.fleet.remove(p);
                        lost += 1;
                    } else {
                        let s = &mut self.fleet[p];
                        s.order = crate::fleet::FleetOrder::Trade;
                        s.condition *= if won { 0.4 + 0.6 * left } else { 0.3 };
                        s.ammo *= 0.4;
                    }
                }
            }
            if let Some(f) = c.faction {
                self.change_reputation(Some(f), if won { 12.0 } else { -4.0 });
            }
            let label = self.cell_label(c.employer);
            self.news_push(
                now,
                1,
                0.0,
                format!(
                    "Vos mercenaires au service de {label} : {} ({pay:.0} CR, {lost} vaisseau(x) perdu(s)).",
                    if won { "victoire" } else { "defaite" }
                ),
            );
        }
    }

    // ---- war and peace ----------------------------------------------------------------------

    /// Whether the people is at war with the player.
    pub fn at_war_with(&self, faction: usize) -> bool {
        self.reputation_with(Some(faction)) <= WAR_STANDING
    }

    pub fn ceasefire_with(&self, faction: usize) -> bool {
        self.military
            .treaties
            .iter()
            .any(|t| t.faction == faction && t.until > self.time)
    }

    /// What a people asks to end the war: the ships destroyed, and a heavy sum per crime.
    pub fn reparations_due(&self, faction: usize) -> f64 {
        0.6 * self.military.destroyed_value(faction)
            + 8000.0 * self.military.crimes_against(faction) as f64
            + self.fines_owed(faction)
    }

    /// Pay the reparations and sign a ceasefire.
    pub fn sue_for_peace(&mut self, faction: usize) -> Result<f64, &'static str> {
        if faction >= self.data.factions.len() {
            return Err("peuple inconnu");
        }
        if !self.at_war_with(faction) {
            return Err("ce peuple n'est pas en guerre contre vous");
        }
        if self.military.crimes_against(faction) >= CRIMES_REFUSE {
            return Err("trop de crimes de guerre : ils refusent de negocier");
        }
        let due = self.reparations_due(faction);
        let price = due.max(2000.0);
        if self.credits < price {
            return Err("fonds insuffisants pour les reparations");
        }
        self.credits -= price;
        self.military.destroyed.retain(|d| d.0 != faction);
        self.military.crimes.retain(|c| c.faction != Some(faction));
        let i = self.record_index(faction);
        self.trade.record[i].owed = 0.0;
        self.change_reputation(Some(faction), 55.0);
        self.military.treaties.retain(|t| t.faction != faction);
        self.military.treaties.push(Treaty {
            faction,
            until: self.time + PEACE_DAYS * crate::crew::DAY,
        });
        let t = self.time;
        let name = self.data.factions[faction].name.clone();
        self.news_push(
            t,
            1,
            0.0,
            format!("Cessez-le-feu signe avec {name} : {price:.0} CR de reparations."),
        );
        Ok(price)
    }

    /// Fire on a people the player signed a ceasefire with: the treaty is torn up.
    pub(crate) fn treaty_violation(&mut self, faction: usize) {
        if !self.ceasefire_with(faction) {
            return;
        }
        self.military.treaties.retain(|t| t.faction != faction);
        self.change_reputation(Some(faction), -40.0);
        let t = self.time;
        let name = self.data.factions[faction].name.clone();
        self.news_push(
            t,
            1,
            0.0,
            format!("Le cessez-le-feu avec {name} est rompu : vous avez tire les premiers."),
        );
    }

    /// A ship of the people destroyed by the player (what peace will cost).
    pub(crate) fn note_destroyed(&mut self, faction: Option<usize>, hull_max: f64) {
        let Some(f) = faction else { return };
        let v = hull_max * 40.0;
        match self.military.destroyed.iter_mut().find(|d| d.0 == f) {
            Some(d) => d.1 += v,
            None => self.military.destroyed.push((f, v)),
        }
    }

    /// A crime of war (finishing a crippled ship, shooting prisoners): recorded, fined by the
    /// law of the people it is done to (`crime_de_guerre`), and held against the player.
    pub fn record_war_crime(&mut self, faction: Option<usize>, what: &str) {
        let t = self.time;
        self.military.crimes.push(WarCrime {
            time: t,
            faction,
            what: what.to_string(),
        });
        if self.military.crimes.len() > 40 {
            self.military.crimes.remove(0);
        }
        self.change_reputation(faction, -12.0);
        self.offend(faction, "crime_de_guerre");
        self.news_push(t, 6, 0.0, format!("Crime de guerre signale : {what}."));
    }

    // ---- planetary defence -------------------------------------------------------------------

    /// The batteries of the sector fire on a ship its people hunts, near their station.
    pub(crate) fn battery_tick(&mut self, dt: f64) {
        if self.docked.is_some() || self.faction.is_none() || !self.is_wanted() {
            return;
        }
        let Some(station) = self
            .bodies
            .iter()
            .filter(|b| matches!(b.kind, vector_world::BodyKind::Station))
            .map(|b| b.pos)
            .min_by(|a, b| {
                (*a - self.ship.pos)
                    .length()
                    .total_cmp(&(*b - self.ship.pos).length())
            })
        else {
            return;
        };
        let d = (station - self.ship.pos).length();
        if d > BATTERY_RANGE {
            return;
        }
        let bat = orbital_battery(
            &self.data,
            &self.galaxy,
            self.galaxy.seed,
            self.current_cell(),
            self.time,
        );
        // The beam spreads with distance like any other.
        let dps = f64::from(bat.guns) * beam_dps(bat.dps, 1064.0, 1.0, d, 0.0);
        self.enemy_beams.push((station, self.ship.pos));
        self.damage_player_hit(dps * dt, HitKind::Beam);
        let day = self.day_index();
        if self.military.last_battery_day != day + 1 {
            self.military.last_battery_day = day + 1;
            let t = self.time;
            self.news_push(
                t,
                1,
                0.0,
                "Les batteries orbitales ouvrent le feu sur votre vaisseau.".to_string(),
            );
        }
    }

    // ---- the daily round --------------------------------------------------------------------

    pub(crate) fn military_tick(&mut self) {
        let day = self.day_index();
        // `last_day` holds the day plus one: zero means the round never ran.
        if day + 1 == self.military.last_day {
            return;
        }
        let days = if self.military.last_day == 0 {
            1
        } else {
            (day + 1 - self.military.last_day).clamp(1, 30)
        };
        self.military.last_day = day + 1;
        let t = self.time;
        // Agents under cover wear thin; those who are blown are lost.
        let mut burnt = Vec::new();
        for _ in 0..days {
            for a in self
                .military
                .agents
                .iter_mut()
                .filter(|a| a.state == AgentState::Infiltrated)
            {
                let ci = counter_intel(&self.data, &self.galaxy, self.galaxy.seed, a.cell);
                a.cover -= (0.02 + 0.05 * ci - 0.03 * a.skill).max(0.005);
                if a.cover <= 0.0 {
                    a.state = AgentState::Burned;
                    burnt.push((a.name.clone(), a.cell));
                }
            }
        }
        for (name, cell) in burnt {
            let label = self.cell_label(cell);
            let people = politics::faction_of(&self.data, &self.galaxy, cell);
            self.change_reputation(people, -5.0);
            self.news_push(t, 6, 0.0, format!("{name} a ete demasque a {label}."));
        }
        let m = (t / WAR_PERIOD).floor() as i64;
        self.military.effects.retain(|e| e.period >= m - 1);
        self.military.intel.retain(|i| i.1 > t);
        self.military.treaties.retain(|x| x.until > t);
        self.settle_mercs();
        self.refresh_sieges();
        self.prize_tick(days as f64);
    }
}

/// Radius (m) the decoys leave the ship at.
const SHIP_RADIUS_M: f64 = crate::SHIP_RADIUS;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    #[test]
    fn kinetic_energy_matches_the_damage_of_the_data() {
        let d = data();
        assert!((kinetic_energy_j(2.0, 100.0) - 10_000.0).abs() < 1e-9);
        let guns: Vec<_> = d.modules.iter().filter(|m| m.weapon == "kinetic").collect();
        assert!(guns.len() >= 7);
        for m in guns {
            let dmg = damage_from_energy(kinetic_energy_j(m.proj_mass, m.speed));
            assert!(
                (dmg / m.damage - 1.0).abs() < 0.12,
                "{}: {dmg:.0} vs {}",
                m.id,
                m.damage
            );
        }
    }

    #[test]
    fn dispersion_grows_with_range_and_lowers_the_hit_chance() {
        assert!(miss_radius(1.5, 20_000.0) > 2.0 * miss_radius(1.5, 9_000.0));
        assert_eq!(hit_chance(1.0, 100.0, 15.0), 1.0);
        let near = hit_chance(4.0, 5_000.0, 15.0);
        let far = hit_chance(4.0, 20_000.0, 15.0);
        assert!(far < near && far < 0.05, "{near} {far}");
        // A tighter gun lands more at the same range.
        assert!(hit_chance(0.4, 50_000.0, 15.0) > hit_chance(1.5, 50_000.0, 15.0));
    }

    #[test]
    fn beams_diverge_and_air_absorbs_them() {
        let (wl, ap) = (1064.0, 0.25);
        assert!((beam_focus(wl, ap, 0.0) - 1.0).abs() < 1e-12);
        let d1 = beam_focus(wl, ap, 5_000.0);
        let d2 = beam_focus(wl, ap, 50_000.0);
        assert!(d2 < d1 && d1 < 1.0);
        // A shorter wavelength diverges less.
        assert!(beam_focus(500.0, ap, 50_000.0) > d2);
        let vac = beam_dps(60.0, wl, ap, 3_000.0, 0.0);
        let air = beam_dps(60.0, wl, ap, 3_000.0, 1.2);
        assert!(air < vac && vac > 0.0);
        // Particle beams scatter in air far more than lasers.
        assert!(
            atmospheric_transmission(10.0, 1.2, 1000.0)
                < atmospheric_transmission(1064.0, 1.2, 1000.0)
        );
    }

    #[test]
    fn a_hot_ship_is_seen_from_further_than_a_cold_one() {
        let cold = passive_detection_range(0.0);
        let warm = passive_detection_range(100.0);
        let blazing = passive_detection_range(2_500.0);
        assert_eq!(cold, ACTIVE_RANGE);
        assert!(warm > cold && blazing > 4.0 * warm - 1.0);
        // Inverse square: four times the power doubles the range.
        let r1 = passive_detection_range(400.0);
        let r2 = passive_detection_range(1_600.0);
        assert!((r2 / r1 - 2.0).abs() < 1e-9);
    }

    #[test]
    fn every_firing_weapon_carries_heat_and_a_spread() {
        let d = data();
        for m in d
            .modules
            .iter()
            .filter(|m| matches!(m.weapon.as_str(), "kinetic" | "beam"))
        {
            assert!(m.flash > 0.0, "{} has no flash", m.id);
        }
    }

    #[test]
    fn battles_are_deterministic_and_the_stronger_fleet_wins() {
        let a = resolve_battle(1000.0, 400.0, 7, 99);
        assert_eq!(a, resolve_battle(1000.0, 400.0, 7, 99));
        assert!(a.a_wins && a.b_left == 0.0 && a.a_left > 0.5);
        let b = resolve_battle(300.0, 900.0, 7, 99);
        assert!(!b.a_wins && b.a_left == 0.0);
        // Square law: twice the force leaves more than 85 % of the force, not 50 %.
        assert!(resolve_battle(2000.0, 1000.0, 7, 5).a_left > 0.8);
    }

    #[test]
    fn wars_have_fronts_that_both_sides_agree_on_and_the_loser_is_besieged() {
        let d = data();
        let g = Galaxy::new(7);
        let mut found = None;
        'o: for c in politics::cells_around([0, 0, 0], 2) {
            for m in 1..300 {
                let t = m as f64 * WAR_PERIOD + 1.0;
                let f = fronts(&d, &g, 7, c, t);
                if let Some(f) = f.first() {
                    found = Some((c, t, f.clone()));
                    break 'o;
                }
            }
        }
        let (c, t, f) = found.expect("a war near the origin");
        let back = fronts(&d, &g, 7, f.foe, t);
        let mirror = back.iter().find(|x| x.foe == c).expect("seen from the foe");
        assert_ne!(f.own_wins(), mirror.own_wins(), "one victor per front");
        assert_eq!(f.age, mirror.age);
        assert_eq!(f, fronts(&d, &g, 7, c, t)[0], "deterministic");
        let loser = if f.own_wins() { f.foe } else { c };
        assert!(is_besieged(&d, &g, 7, loser, t));
        // Without any war there is no front and no siege.
        let calm = politics::cells_around([0, 0, 0], 2)
            .into_iter()
            .find(|&c| fronts(&d, &g, 7, c, 0.5).is_empty())
            .expect("a peaceful sector");
        assert!(!is_besieged(&d, &g, 7, calm, 0.5));
    }

    #[test]
    fn supply_lines_stretch_with_the_age_of_the_war() {
        let d = data();
        let fresh = fleet_strength(&d, 7, [0, 0, 0], 4, 0, false);
        let old = fleet_strength(&d, 7, [0, 0, 0], 4, 8, false);
        let old_home = fleet_strength(&d, 7, [0, 0, 0], 4, 8, true);
        assert!(old < fresh && old < old_home);
    }

    #[test]
    fn skilled_agents_succeed_more_and_counter_intelligence_catches_them() {
        let rate = |skill: f64, ci: f64, res: SpyResult| {
            (0..2000)
                .filter(|&k| spy_mission(7, skill, ci, k) == res)
                .count()
        };
        assert!(rate(0.9, 0.1, SpyResult::Success) > rate(0.1, 0.1, SpyResult::Success) + 300);
        assert!(rate(0.5, 0.9, SpyResult::Success) < rate(0.5, 0.0, SpyResult::Success));
        assert!(rate(0.5, 0.9, SpyResult::Caught) > rate(0.5, 0.0, SpyResult::Caught));
        assert_eq!(spy_mission(7, 0.5, 0.5, 3), spy_mission(7, 0.5, 0.5, 3));
    }

    // ---- zones, decoys, doctrine, espionage, mercenaries, peace, boarding, escorts -------------

    use crate::combat::{Owner, ProjKind, Projectile};
    use crate::npc::{AiMode, NpcRole, NpcState};
    use crate::ShipInput;
    use vector_world::{generate_system, BodyKind};

    /// A frontier system with a belt and a station, the ship in deep space and fitted with
    /// `extra` modules on a frigate hull.
    fn sim_with(extra: &[&str]) -> Sim {
        let g = Galaxy::new(8);
        let e = g
            .nearby([0.0; 3], 150.0)
            .into_iter()
            .find(|e| {
                let s = generate_system(e.seed);
                g.territory(e.pos, 8).is_none()
                    && s.bodies
                        .iter()
                        .any(|b| matches!(b.kind, BodyKind::Asteroid(_)))
                    && s.bodies.iter().any(|b| b.kind == BodyKind::Station)
            })
            .expect("frontier system with a belt");
        let mut s = Sim::new_in_galaxy(g, &e, data());
        s.reveal_all();
        s.ship.pos = vector_core::Vec3::new(4.0e11, 0.0, 0.0);
        s.ship.vel = vector_core::Vec3::ZERO;
        s.update_ref();
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        let d = Arc::clone(&s.data);
        let hull = d.hull_index("frigate").expect("hull");
        s.ship.design = crate::design::ShipDesign::empty(&d, hull);
        for id in [
            "fusion_mk3",
            "torch_mk3",
            "battery_mk3",
            "radiator_mk4",
            "lifesup_mk1",
            "cargo_mk3",
            "tank_mk3",
        ]
        .iter()
        .chain(extra.iter())
        {
            s.ship
                .design
                .install_first_fit(&d, id)
                .unwrap_or_else(|e| panic!("{id}: {e}"));
        }
        s.ship.sys = crate::design::Systems::new(&s.ship.design, 0.0);
        s.ship.refresh_stats(&d);
        s.ship.hull = s.ship.stats.hull_max;
        s.ship.fuel = s.ship.stats.fuel_cap;
        s.ship.sys.battery_mj = s.ship.stats.battery_cap;
        s.weapon_cd = vec![0.0; s.ship.design.slots.len()];
        s.credits = 1.0e7;
        for k in 0..2 {
            let sk = d.skill_index("ingenierie").unwrap();
            let p = crate::crew::generate_person(&d, vector_core::mix64(77 + k), Some(sk));
            let role = crate::crew::Crew::role_index(&d, "engineer");
            s.crew.hire(&d, p, role, 0.0);
        }
        // One step so that the modules are powered.
        s.step(&ShipInput::default());
        s
    }

    fn dock(s: &mut Sim) {
        s.docked = s.nearest_station().map(|x| x.0);
        assert!(s.docked_station().is_ok());
    }

    fn add(s: &mut Sim, res: &str, kg: f64) {
        let d = Arc::clone(&s.data);
        s.ship
            .cargo
            .add(&d, d.resource_index(res).expect("resource"), kg);
    }

    /// A sector and a time at which it is at war, the ship placed in that sector.
    fn war_sector(s: &mut Sim) -> (Cell, f64) {
        for c in politics::cells_around([0, 0, 0], 2) {
            for m in 1..300 {
                let t = m as f64 * WAR_PERIOD + 1.0;
                if !fronts(&s.data, &s.galaxy, s.galaxy.seed, c, t).is_empty() {
                    s.here_pos = politics::cell_center(c);
                    s.time = t;
                    return (c, t);
                }
            }
        }
        panic!("no war near the origin");
    }

    #[test]
    fn reactive_plates_stop_slugs_and_ablative_plates_stop_beams() {
        let plain = Armour {
            value: 2.0,
            ..Armour::default()
        };
        let react = Armour {
            reactive: 0.5,
            ..plain
        };
        let abl = Armour {
            ablative: 0.6,
            ..plain
        };
        let slug = |a: &Armour| split_hit(300.0, HitKind::Kinetic, 0.0, a);
        assert!(slug(&react).total_through() < 0.55 * slug(&plain).total_through());
        assert!(slug(&react).cancelled > 100.0);
        // Reactive plates do nothing against a beam; ablative ones do.
        let beam = |a: &Armour| split_hit(6.0, HitKind::Beam, 0.0, a);
        assert_eq!(beam(&react).total_through(), beam(&plain).total_through());
        assert!(beam(&abl).total_through() < 0.8 * beam(&plain).total_through());
        // The ablative layer burns away in both cases; a plain one never does.
        assert!(slug(&abl).ablated > 0.0 && beam(&abl).ablated > 0.0);
        assert_eq!(slug(&plain).ablated, 0.0);
    }

    #[test]
    fn deep_hits_reach_the_cabins_and_shields_come_first() {
        let none = Armour::default();
        let big = split_hit(400.0, HitKind::Kinetic, 0.0, &none);
        let tick = split_hit(4.0, HitKind::Beam, 0.0, &none);
        // Nothing is lost on a bare hull.
        assert!((big.total_through() - 400.0).abs() < 1e-9);
        assert!(big.crew / 400.0 > 5.0 * tick.crew / 4.0, "{big:?} {tick:?}");
        assert!((big.hull / (big.hull + big.modules) - 0.6).abs() < 1e-9);
        let sh = split_hit(100.0, HitKind::Kinetic, 60.0, &none);
        assert_eq!(sh.shield, 60.0);
        assert!((sh.total_through() - 40.0).abs() < 1e-9);
    }

    #[test]
    fn plates_are_read_from_the_installed_modules_and_burn_in_the_fight() {
        let mut r = sim_with(&["armor_react_mk1"]);
        assert!(r.armour_profile().reactive > 0.0 && r.armour_profile().ablative == 0.0);
        r.damage_player(300.0);
        assert!(
            r.last_zones.cancelled > 0.0,
            "reactive plates fire on slugs"
        );
        assert!(r.last_zones.crew > 0.0, "a slug reaches the cabins");
        let mut s = sim_with(&["armor_abl_mk1"]);
        let a = s.armour_profile();
        assert!(a.ablative > 0.5 && a.reactive == 0.0, "{a:?}");
        let plate = s
            .ship
            .design
            .modules(&s.data)
            .find(|(_, m)| m.id == "armor_abl_mk1")
            .map(|(i, _)| i)
            .unwrap();
        let before = s.ship.sys.health[plate];
        s.damage_player_hit(200.0, HitKind::Beam);
        assert!(s.last_zones.ablated > 0.0);
        assert!(s.ship.sys.health[plate] < before);
    }

    #[test]
    fn a_hotter_flare_draws_the_seeker_and_the_missile_dies_on_it() {
        let ship = vector_core::Vec3::ZERO;
        let flare = Decoy {
            pos: vector_core::Vec3::new(2000.0, 0.0, 0.0),
            vel: vector_core::Vec3::ZERO,
            heat_kw: 3000.0,
            until: 1e9,
        };
        let from = vector_core::Vec3::new(0.0, 0.0, 6000.0);
        assert_eq!(seeker_pick(from, ship, 100.0, &[flare]), Some(0));
        // A cold flare is ignored, and a flare far behind the ship too.
        let cold = Decoy {
            heat_kw: 1.0,
            ..flare
        };
        assert_eq!(seeker_pick(from, ship, 100.0, &[cold]), None);
        let far = Decoy {
            pos: vector_core::Vec3::new(0.0, 0.0, 60_000.0),
            ..flare
        };
        assert_eq!(seeker_pick(from, ship, 3000.0, &[far]), None);

        // In the sim: the same missile hits the ship without a flare and the flare with one.
        let run = |with_flare: bool| {
            let mut s = sim_with(&["decoy_mk1"]);
            add(&mut s, "decoys", 5.0);
            s.npcs.clear();
            if with_flare {
                s.launch_decoy().expect("launch");
                assert_eq!(s.decoys.len(), 1);
                assert!(s.launch_decoy().is_err(), "the launcher reloads");
                // Put the flare between the missile and the ship.
                s.decoys[0].pos = s.ship.pos + vector_core::Vec3::new(0.0, 1500.0, 2500.0);
                s.decoys[0].vel = s.ship.vel;
                s.decoys[0].until = s.time + 60.0;
            }
            let h0 = s.ship.hull;
            s.projectiles.push(Projectile {
                pos: s.ship.pos + vector_core::Vec3::new(0.0, 1500.0, 6000.0),
                vel: s.ship.vel + vector_core::Vec3::new(0.0, 0.0, -300.0),
                damage: 600.0,
                life: 60.0,
                owner: Owner::Npc(1),
                kind: ProjKind::Missile {
                    target: None,
                    turn: 3.0,
                    top_speed: 700.0,
                },
            });
            for _ in 0..900 {
                s.step(&ShipInput::default());
            }
            (h0 - s.ship.hull, s.decoys.len())
        };
        let (bare, _) = run(false);
        let (lured, left) = run(true);
        assert!(bare > 100.0, "the missile must hit a bare ship: {bare}");
        assert!(lured < 1.0 && left == 0, "{lured} {left}");
    }

    #[test]
    fn the_radar_sees_further_and_shouts() {
        let mut s = sim_with(&["sensor_active_mk1"]);
        assert!(s.sensor_power_kw() > 0.0);
        let quiet = s.emission_kw();
        let range = s.detection_range();
        assert!(s.toggle_active_sensors().unwrap());
        assert!(s.emission_kw() > quiet + 1000.0);
        assert!(s.detection_range() > range, "the radar lights the ship up");
        s.toggle_active_sensors().unwrap();
        assert!(!s.active_sensors);
        // A ship without a radar cannot switch one on.
        assert!(sim_with(&[]).toggle_active_sensors().is_err());

        // Contact classes: a hostile beyond the passive range is lost, seen with the radar.
        let k = s
            .npcs
            .iter()
            .position(|n| n.role == NpcRole::Pirate)
            .unwrap();
        s.npcs[k].level = 1;
        let passive = passive_detection_range(npc_thermal_kw(NpcRole::Pirate, 1));
        s.toggle_active_sensors().unwrap();
        let reach = s.active_reach();
        assert!(reach > passive, "{reach} {passive}");
        let far = (passive + reach) / 2.0;
        s.npcs[k].pos = s.ship.pos + vector_core::Vec3::new(0.0, 0.0, far);
        assert_eq!(s.contact(k), Contact::Active);
        s.toggle_active_sensors().unwrap();
        assert_eq!(s.contact(k), Contact::Lost);
        s.npcs[k].pos = s.ship.pos + vector_core::Vec3::new(0.0, 0.0, passive * 0.5);
        assert_eq!(s.contact(k), Contact::Passive);
    }

    #[test]
    fn hostile_crews_loose_missiles_that_the_ship_can_fool() {
        let mut s = sim_with(&[]);
        let k = s
            .npcs
            .iter()
            .position(|n| n.role == NpcRole::Pirate)
            .unwrap();
        {
            let n = &mut s.npcs[k];
            n.level = 3;
            n.hostile = true;
            n.disabled = false;
            n.hull = 1.0e6;
            n.hull_max = 1.0e6;
            n.pos = s.ship.pos + vector_core::Vec3::new(0.0, 0.0, 8000.0);
            n.vel = s.ship.vel;
            n.state = NpcState::Combat {
                mode: AiMode::Pursue,
            };
        }
        let mut launched = false;
        for _ in 0..600 {
            s.step(&ShipInput::default());
            launched |= s.projectiles.iter().any(|p| {
                matches!(p.owner, Owner::Npc(_)) && matches!(p.kind, ProjKind::Missile { .. })
            });
        }
        assert!(launched, "a level-3 hostile must fire missiles");
    }

    #[test]
    fn doctrines_shape_the_strength_of_a_fleet_and_the_will_to_fight() {
        assert!(
            Doctrine::Aggressive.strength_mult(false) > Doctrine::Aggressive.strength_mult(true)
        );
        assert!(Doctrine::Defensive.strength_mult(true) > Doctrine::Defensive.strength_mult(false));
        assert_eq!(Doctrine::Harassment.supply_age(8), 4);
        assert_eq!(Doctrine::Aggressive.supply_age(8), 8);
        assert!(Doctrine::Harassment.flee_below() > Doctrine::Defensive.flee_below());
        assert!(Doctrine::Defensive.flee_below() > Doctrine::Aggressive.flee_below());
        let seen: std::collections::BTreeSet<_> =
            (0..60).map(|id| pirate_doctrine(7, id).label()).collect();
        assert_eq!(seen.len(), 3, "pirates come in all three doctrines");
        let d = data();
        let kinds: std::collections::BTreeSet<_> = (0..d.factions.len())
            .map(|f| faction_doctrine(&d, Some(f)).label())
            .collect();
        assert!(kinds.len() >= 2, "factions differ: {kinds:?}");
    }

    #[test]
    fn sabotage_and_hired_hulls_move_the_front_for_both_sides() {
        let d = data();
        let g = Galaxy::new(7);
        let mut found = None;
        'o: for c in politics::cells_around([0, 0, 0], 2) {
            for m in 1..300 {
                let t = m as f64 * WAR_PERIOD + 1.0;
                if let Some(f) = fronts(&d, &g, 7, c, t).first() {
                    found = Some((c, t, f.clone()));
                    break 'o;
                }
            }
        }
        let (c, t, f) = found.expect("a war");
        let m = (t / WAR_PERIOD).floor() as i64;
        let weak = [FrontEffect {
            cell: c,
            period: m,
            mult: 0.4,
            add: 0.0,
        }];
        let f2 = fronts_with(&d, &g, 7, c, t, &weak)
            .into_iter()
            .find(|x| x.foe == f.foe)
            .unwrap();
        assert!((f2.own_strength - 0.4 * f.own_strength).abs() < 1e-6);
        // The other side reads the same sabotage: it sees the foe weakened, and the same victor.
        let back = fronts_with(&d, &g, 7, f.foe, t, &weak)
            .into_iter()
            .find(|x| x.foe == c)
            .unwrap();
        assert!((back.foe_strength - f2.own_strength).abs() < 1e-6);
        assert_ne!(f2.own_wins(), back.own_wins());
        // Doctrines are those of the peoples, and swap sides.
        assert_eq!(
            f.own_doctrine,
            faction_doctrine(&d, politics::faction_of(&d, &g, c))
        );
        assert_eq!(back.foe_doctrine, f.own_doctrine);
        // Enough hired hulls turn a lost front into a won one.
        let big = [FrontEffect {
            cell: c,
            period: m,
            mult: 1.0,
            add: 1000.0 * ship_rating(&d),
        }];
        let f3 = fronts_with(&d, &g, 7, c, t, &big)
            .into_iter()
            .find(|x| x.foe == f.foe)
            .unwrap();
        assert!(f3.own_wins());
        // Effects of another period do nothing.
        let other = [FrontEffect {
            period: m + 5,
            ..big[0]
        }];
        assert_eq!(fronts_with(&d, &g, 7, c, t, &other)[0], f);
    }

    /// Run `op` with fresh infiltrated agents until one succeeds; returns its message.
    fn succeed(s: &mut Sim, cell: Cell, op: SpyOp) -> String {
        for k in 0..120 {
            let id = 5000 + s.military.agents.len() as u32;
            s.military.agents.push(Agent {
                id,
                name: format!("T{k}"),
                skill: 0.9,
                state: AgentState::Infiltrated,
                cell,
                cover: 1.0,
                ops: k,
            });
            let m = s.spy_op(id, op).expect("op");
            let ok = match op {
                SpyOp::Sabotage => m.starts_with("SABOTAGE REUSSI"),
                SpyOp::StealPlans => m.starts_with("PLANS VOLES"),
                SpyOp::Wiretap => m.contains("FRONT(S)"),
                SpyOp::Rumor => m.starts_with("RUMEUR LANCEE"),
            };
            if ok {
                return m;
            }
        }
        panic!("no success in 120 tries");
    }

    #[test]
    fn spies_are_recruited_infiltrated_and_used() {
        let mut s = sim_with(&[]);
        assert!(s.recruit_agent().is_err(), "needs a station");
        dock(&mut s);
        let (c, _t) = war_sector(&mut s);
        let id = s.recruit_agent().expect("hire");
        assert_eq!(s.military.agents[0].state, AgentState::Idle);
        assert!(
            s.spy_op(id, SpyOp::Wiretap).is_err(),
            "not on the field yet"
        );
        assert!(
            s.infiltrate(id, [40, 40, 40]).is_err(),
            "out of reach of the networks"
        );
        let credits = s.credits;
        s.infiltrate(id, c).expect("infiltrate");
        assert!(s.credits < credits);
        // Several agents: at least one slips in, and is then under cover.
        let mut ids = vec![id];
        for _ in 0..25 {
            ids.push(s.recruit_agent().unwrap());
        }
        for &a in &ids {
            let _ = s.infiltrate(a, c);
        }
        assert!(
            s.military
                .agents
                .iter()
                .any(|a| a.state == AgentState::Infiltrated && a.cell == c),
            "someone got in"
        );

        // Wiretap: the real fronts reach the journal, and the sector is known for a month.
        succeed(&mut s, c, SpyOp::Wiretap);
        assert!(s.news.iter().any(|n| n.text.starts_with("Renseignement")));
        assert!(s.military.knows(c, s.time));
        // Stolen plans are research points.
        let pool = s.research_pool;
        succeed(&mut s, c, SpyOp::StealPlans);
        assert!(s.research_pool > pool + 100.0);
        // Sabotage weakens the fleet of the sector, as seen by `fronts_with`.
        succeed(&mut s, c, SpyOp::Sabotage);
        let plain = fronts(&s.data, &s.galaxy, s.galaxy.seed, c, s.time)[0].own_strength;
        let hit = fronts_with(
            &s.data,
            &s.galaxy,
            s.galaxy.seed,
            c,
            s.time,
            &s.military.effects,
        )[0]
        .own_strength;
        assert!(hit < plain);
        // The rumour is heard like any news.
        succeed(&mut s, c, SpyOp::Rumor);
        assert!(s.news.iter().any(|n| n.text.starts_with("On murmure")));
        assert!(s.military.rumors >= 1);
    }

    #[test]
    fn cover_wears_thin_and_a_blown_agent_is_lost() {
        let mut s = sim_with(&[]);
        let (c, _) = war_sector(&mut s);
        s.military.agents.push(Agent {
            id: 1,
            name: "X".into(),
            skill: 0.0,
            state: AgentState::Infiltrated,
            cell: c,
            cover: 0.3,
            ops: 0,
        });
        for d in 0..60 {
            s.time += crate::crew::DAY;
            s.military_tick();
            if s.military.agents[0].state != AgentState::Infiltrated {
                assert!(d < 40, "a clumsy agent does not last long");
                break;
            }
        }
        assert_eq!(s.military.agents[0].state, AgentState::Burned);
        assert!(s.news.iter().any(|n| n.text.contains("demasque")));
        // Agents captured on a mission cannot work again.
        s.military.agents[0].state = AgentState::Captured;
        assert!(s.spy_op(1, SpyOp::Wiretap).is_err());
    }

    #[test]
    fn a_recon_drone_costs_robotics_and_reveals_a_front() {
        let mut s = sim_with(&[]);
        let (c, _) = war_sector(&mut s);
        let cell = politics::cells_around(c, 1)
            .into_iter()
            .find(|x| !fronts(&s.data, &s.galaxy, s.galaxy.seed, *x, s.time).is_empty())
            .unwrap();
        assert!(s.launch_recon_drone(cell).is_err(), "needs robotics");
        add(&mut s, "robotics", 3.0);
        let n = s.launch_recon_drone(cell).expect("drone");
        assert!(n >= 1 && s.military.knows(cell, s.time));
        assert!(s.launch_recon_drone([30, 30, 30]).is_err());
    }

    #[test]
    fn hired_ships_fight_for_a_war_and_come_back_paid() {
        let mut s = sim_with(&[]);
        dock(&mut s);
        let (c, t) = war_sector(&mut s);
        let frigate = s.data.hull_index("frigate").unwrap();
        s.buy_fleet_ship_free(frigate);
        s.buy_fleet_ship_free(frigate);
        let ids: Vec<u32> = s.fleet.iter().map(|f| f.id).collect();
        let offers = s.merc_offers();
        assert!(!offers.is_empty() && offers[0].pay_day > 0.0);
        assert!(s.hire_out(0, &[]).is_err());
        let before = fronts_with(&s.data, &s.galaxy, s.galaxy.seed, c, t, &s.military.effects)[0]
            .own_strength;
        s.hire_out(0, &ids).expect("hire out");
        let after = fronts_with(&s.data, &s.galaxy, s.galaxy.seed, c, t, &s.military.effects)[0]
            .own_strength;
        assert!(after > before, "hired hulls add to the line");
        assert!(s
            .fleet
            .iter()
            .all(|f| f.order == crate::fleet::FleetOrder::Mercenary));
        assert!(s.hire_out(0, &ids).is_err(), "already under contract");
        assert!(s.set_order(0, crate::fleet::FleetOrder::Defend).is_err());
        let credits = s.credits;
        s.time += (MERC_DAYS + 1.0) * crate::crew::DAY;
        s.military_tick();
        assert!(s.military.mercs.is_empty());
        assert!(s.credits > credits, "the contract is paid");
        assert!(s.news.iter().any(|n| n.text.starts_with("Vos mercenaires")));
        assert!(s
            .fleet
            .iter()
            .all(|f| f.order == crate::fleet::FleetOrder::Trade));
    }

    #[test]
    fn peace_has_a_price_a_ceasefire_and_a_limit() {
        let mut s = sim_with(&[]);
        let f = 2;
        assert!(s.sue_for_peace(f).is_err(), "no war, no peace");
        s.change_reputation(Some(f), -90.0);
        assert!(s.at_war_with(f));
        s.military.destroyed.push((f, 50_000.0));
        s.military.crimes.push(WarCrime {
            time: 0.0,
            faction: Some(f),
            what: "x".into(),
        });
        let due = s.reparations_due(f);
        assert!(due >= 30_000.0 + 8000.0);
        s.credits = due - 1.0;
        assert!(s.sue_for_peace(f).is_err(), "cannot pay");
        s.credits = due + 5000.0;
        let paid = s.sue_for_peace(f).expect("peace");
        assert!((paid - due).abs() < 1.0);
        assert!(!s.at_war_with(f) && s.ceasefire_with(f));
        assert_eq!(s.military.destroyed_value(f), 0.0);
        assert_eq!(s.military.crimes_against(f), 0);
        // Firing first tears the treaty up and the standing falls again.
        let rep = s.reputation_with(Some(f));
        s.treaty_violation(f);
        assert!(!s.ceasefire_with(f) && s.reputation_with(Some(f)) < rep - 20.0);
        // Too many crimes of war: they will not talk.
        s.change_reputation(Some(f), -90.0);
        for _ in 0..CRIMES_REFUSE {
            s.military.crimes.push(WarCrime {
                time: 0.0,
                faction: Some(f),
                what: "x".into(),
            });
        }
        assert!(s.sue_for_peace(f).unwrap_err().contains("crimes"));
    }

    #[test]
    fn finishing_a_crippled_ship_is_a_crime_of_war_and_is_remembered() {
        let mut s = sim_with(&[]);
        let k = s.npcs.iter().position(|n| n.role == NpcRole::Trader);
        let k = k.unwrap_or_else(|| {
            let n = s
                .npcs
                .iter()
                .position(|n| n.role != NpcRole::Pirate)
                .expect("a civil ship");
            n
        });
        s.npcs[k].faction = Some(1);
        s.npcs[k].role = NpcRole::Trader;
        s.npcs[k].disabled = true;
        let hull = s.npcs[k].hull_max;
        s.damage_npc(k, 1.0e9);
        assert_eq!(s.military.crimes_against(1), 1);
        assert!(s.military.destroyed_value(1) >= hull * 40.0 - 1.0);
        assert!(s.news.iter().any(|n| n.text.starts_with("Crime de guerre")));
        // A pirate is fair game.
        let p = s
            .npcs
            .iter()
            .position(|n| n.role == NpcRole::Pirate)
            .unwrap();
        s.npcs[p].disabled = true;
        s.damage_npc(p, 1.0e9);
        assert_eq!(s.military.crimes.len(), 1);
    }

    #[test]
    fn batteries_fire_on_a_hunted_ship_near_the_station_only() {
        let mut s = sim_with(&[]);
        s.faction = Some(0);
        let st = s.nearest_station().unwrap().0;
        let sp = s.bodies[st].pos;
        s.ship.pos = sp + vector_core::Vec3::new(20_000.0, 0.0, 0.0);
        let h0 = s.ship.hull;
        s.battery_tick(1.0);
        assert_eq!(s.ship.hull, h0, "an innocent ship is left alone");
        s.change_reputation(Some(0), -90.0);
        assert!(s.is_wanted());
        s.battery_tick(1.0);
        assert!(s.ship.hull < h0, "the guns open up");
        assert!(s
            .news
            .iter()
            .any(|n| n.text.contains("batteries orbitales")));
        // Out of range: nothing.
        s.ship.pos = sp + vector_core::Vec3::new(400_000.0, 0.0, 0.0);
        let h1 = s.ship.hull;
        s.battery_tick(1.0);
        assert_eq!(s.ship.hull, h1);
        // More guns in a sector that is not besieged.
        let d = data();
        let g = Galaxy::new(7);
        let sizes: Vec<u32> = politics::cells_around([0, 0, 0], 2)
            .into_iter()
            .map(|c| orbital_battery(&d, &g, 7, c, 0.5).guns)
            .collect();
        assert!(sizes.iter().all(|&n| (1..=9).contains(&n)));
        assert!(sizes.iter().min() != sizes.iter().max(), "sectors differ");
    }

    #[test]
    fn sieges_are_seen_and_reported() {
        let mut s = sim_with(&[]);
        let d = Arc::clone(&s.data);
        // Find a time and place where the sector the ship is in loses a front.
        let mut ok = false;
        'o: for c in politics::cells_around([0, 0, 0], 2) {
            for m in 1..400 {
                let t = m as f64 * WAR_PERIOD + 1.0;
                if is_besieged(&d, &s.galaxy, s.galaxy.seed, c, t) {
                    s.here_pos = politics::cell_center(c);
                    s.time = t;
                    ok = true;
                    break 'o;
                }
            }
        }
        assert!(ok, "a besieged sector exists");
        s.refresh_sieges();
        let siege = s
            .here_besieged()
            .expect("the ship's sector is besieged")
            .clone();
        assert_eq!(siege.cell, s.current_cell());
        assert!(s.military.sieges.iter().all(|x| x.foe != x.cell));
        assert!(s.news.iter().any(|n| n.text.starts_with("Blocus de")));
        // A sector at peace is not.
        let calm = politics::cells_around(s.current_cell(), 2)
            .into_iter()
            .find(|c| fronts(&d, &s.galaxy, s.galaxy.seed, *c, s.time).is_empty())
            .unwrap();
        s.here_pos = politics::cell_center(calm);
        s.refresh_sieges();
        assert!(s.here_besieged().is_none());
    }

    #[test]
    fn escorts_screen_the_ship_shoot_and_run_out_of_ammunition() {
        use crate::fleet::FleetOrder;
        let mut s = sim_with(&[]);
        let frigate = s.data.hull_index("frigate").unwrap();
        for _ in 0..3 {
            s.buy_fleet_ship_free(frigate);
        }
        assert!(
            s.set_order(0, FleetOrder::Defend).is_err(),
            "orders are given at a station"
        );
        dock(&mut s);
        s.set_order(0, FleetOrder::Defend).unwrap();
        s.set_order(1, FleetOrder::Attack).unwrap();
        s.set_order(2, FleetOrder::Follow).unwrap();
        let screen = s.escort_screen();
        assert!((screen - (crate::fleet::DEFEND_SHARE + crate::fleet::FOLLOW_SHARE)).abs() < 1e-9);
        // Warships cost double the upkeep, and a ship in the squadron cannot take a route.
        assert_eq!(FleetOrder::Attack.upkeep_mult(), 2.0);
        s.create_route(
            crate::fleet::Endpoint::Colony(1),
            crate::fleet::Endpoint::Colony(2),
            0,
            10.0,
        )
        .unwrap();
        assert!(s.assign_route(0, 0).is_err());
        s.docked = None;

        // Hits are shared: the ship takes less, the defenders take the rest.
        s.ship.hull = s.ship.stats.hull_max;
        let hp = s.ship.hull;
        s.damage_player_hit(100.0, HitKind::Beam);
        let lost = hp - s.ship.hull;
        let mut bare = sim_with(&[]);
        bare.damage_player_hit(100.0, HitKind::Beam);
        let lost_bare = bare.ship.stats.hull_max - bare.ship.hull;
        assert!(lost < lost_bare * 0.9, "{lost} {lost_bare}");
        assert!(s.fleet[0].condition < 1.0 && s.fleet[2].condition < 1.0);
        // Pounded long enough the screen dies, and the journal says so.
        for _ in 0..400 {
            s.damage_player_hit(5000.0, HitKind::Beam);
            s.ship.hull = s.ship.stats.hull_max;
            s.destroyed = false;
        }
        assert!(s.fleet.iter().all(|f| f.order != FleetOrder::Defend));
        assert!(s.news.iter().any(|n| n.text.contains("est perdu")));

        // The attacker fires on the target of the player and burns ammunition doing so.
        let mut s = sim_with(&[]);
        s.buy_fleet_ship_free(frigate);
        dock(&mut s);
        s.set_order(0, FleetOrder::Attack).unwrap();
        s.docked = None;
        let k = s
            .npcs
            .iter()
            .position(|n| n.role == NpcRole::Pirate)
            .unwrap();
        s.npcs[k].pos = s.ship.pos + vector_core::Vec3::new(0.0, 0.0, 5000.0);
        s.npcs[k].hostile = true;
        s.npcs[k].hull = 1.0e5;
        s.npcs[k].hull_max = 1.0e5;
        s.target_npc = Some(k);
        let (h0, a0) = (s.npcs[k].hull, s.fleet[0].ammo);
        s.escort_tick(10.0);
        assert!(s.npcs[k].hull < h0 && s.fleet[0].ammo < a0);
        // Empty guns do not fire; the hold refills them.
        s.fleet[0].ammo = 0.0;
        let h1 = s.npcs[k].hull;
        s.escort_tick(10.0);
        assert_eq!(s.npcs[k].hull, h1);
        assert!(s.resupply_escorts().is_err(), "no ammunition in the hold");
        add(&mut s, "ammo_slugs", 1000.0);
        let used = s.resupply_escorts().expect("resupply");
        assert!((used - crate::fleet::RESUPPLY_KG).abs() < 1e-6 && s.fleet[0].ammo > 0.99);
        // A patrol engages the nearest hostile without being told.
        s.set_order(0, FleetOrder::Trade).unwrap_err();
        s.docked = s.nearest_station().map(|x| x.0);
        s.set_order(0, FleetOrder::Patrol).unwrap();
        s.docked = None;
        s.target_npc = None;
        let h2 = s.npcs[k].hull;
        s.escort_tick(10.0);
        assert!(s.npcs[k].hull < h2);
    }

    #[test]
    fn a_boarding_on_foot_takes_the_ship_and_its_crew_must_be_judged() {
        let mut s = sim_with(&[]);
        let k = s
            .npcs
            .iter()
            .position(|n| n.role == NpcRole::Pirate)
            .unwrap();
        let id = s.npcs[k].id;
        assert!(s.board_on_foot().is_err(), "no target");
        s.target_npc = Some(k);
        assert!(s.board_on_foot().is_err(), "not neutralised");
        {
            let n = &mut s.npcs[k];
            n.disabled = true;
            n.pos = s.ship.pos + vector_core::Vec3::new(0.0, 0.0, 200.0);
            n.vel = s.ship.vel;
        }
        s.board_on_foot().expect("board");
        assert!(s.board_on_foot().is_err(), "already aboard");
        {
            let r = s.rogue.as_ref().unwrap();
            assert_eq!(s.data.rogue_themes[r.theme].id, "abordage");
            // The walls are the hull's: a pirate ship is rusted plate.
            let rust = s.data.material_index("coque_rouillee").unwrap();
            let f = r.floor();
            let (wx, wy) = (0..r.h as i32)
                .flat_map(|y| (0..r.w as i32).map(move |x| (x, y)))
                .find(|&(x, y)| r.tile(x, y) == crate::rogue::T_WALL)
                .unwrap();
            assert_eq!(f.mats[r.idx(wx, wy)] as usize, rust);
            assert!(!f.monsters.is_empty(), "the crew holds the corridors");
        }
        // Falling back with the garrison alive takes nothing.
        let msg = s.rogue_stairs().expect("leave");
        assert!(msg.contains("GARNISON"), "{msg}");
        assert!(s.fleet.is_empty() && s.npcs.iter().any(|n| n.id == id));
        s.rogue_close();
        // Again, and this time the garrison is dead.
        s.board_on_foot().expect("board again");
        s.rogue.as_mut().unwrap().floors[0].monsters.clear();
        let msg = s.rogue_stairs().expect("leave");
        assert!(msg.contains("EST A VOUS"), "{msg}");
        assert!(s.npcs.iter().all(|n| n.id != id), "the ship left the sky");
        assert_eq!(s.fleet.len(), 1);
        let prize = s.fleet[0].clone();
        assert!(prize.prize && prize.prisoners > 0 && prize.condition < 1.0);
        assert!(s.military.boarding.is_none());

        // Judging the crew: each way works once and has its price.
        let pid = prize.id;
        let credits = s.credits;
        let sum = s.prize_ransom(pid).unwrap();
        assert!(sum > 0.0 && s.credits == credits + sum);
        assert!(s.prize_ransom(pid).is_err(), "no prisoners left");
        s.fleet[0].prisoners = 4;
        s.fleet[0].origin = Some(1);
        s.prize_execute(pid).unwrap();
        assert_eq!(s.military.crimes_against(1), 1);
        s.fleet[0].prisoners = 4;
        let l = s.fleet[0].loyalty;
        let up = s.fleet[0].upkeep;
        s.prize_enlist(pid).unwrap();
        assert!(s.fleet[0].loyalty > l && s.fleet[0].upkeep > up);
        // Left alone with prisoners aboard, a prize crew mutinies.
        s.fleet[0].prisoners = 4;
        s.fleet[0].loyalty = 0.1;
        for _ in 0..10 {
            s.prize_tick(1.0);
        }
        assert!(s.fleet.is_empty());
        assert!(s.news.iter().any(|n| n.text.starts_with("Mutinerie")));
    }

    #[test]
    fn a_cold_ship_strikes_first_and_an_unwary_target_pays_triple() {
        let mut s = sim_with(&[]);
        let k = s
            .npcs
            .iter()
            .position(|n| n.role == NpcRole::Trader)
            .unwrap_or(0);
        let far = passive_detection_range(s.quiet_signature_kw()) + 20_000.0;
        let hit = |s: &mut Sim, aware: bool| {
            s.npcs[k].hull_max = 1.0e6;
            s.npcs[k].hull = 1.0e6;
            s.npcs[k].shield = 0.0;
            s.npcs[k].provoked = false;
            s.npcs[k].disabled = false;
            s.npcs[k].pos = s.ship.pos + vector_core::Vec3::new(0.0, 0.0, far);
            s.npcs[k].state = if aware {
                NpcState::Combat { mode: AiMode::Flee }
            } else {
                NpcState::Orbit {
                    around: 0,
                    phase: 0.0,
                }
            };
            let before = s.npcs[k].hull;
            s.damage_npc(k, 100.0);
            before - s.npcs[k].hull
        };
        // Unwary and unseen: the blow is tripled and the journal says so.
        let base = hit(&mut s, true);
        assert_eq!(base, 100.0);
        let amb = hit(&mut s, false);
        assert!((amb - 100.0 * AMBUSH_MULT).abs() < 1e-9, "{amb}");
        assert_eq!(s.military.ambushes, 1);
        assert!(s.news.iter().any(|n| n.text.starts_with("Embuscade")));
        // A blazing ship is felt from afar: no ambush.
        s.npcs[k].provoked = false;
        let hot = s.quiet_signature_kw();
        assert!(hot > 0.0);
        s.npcs[k].pos = s.ship.pos + vector_core::Vec3::new(0.0, 0.0, 1000.0);
        s.npcs[k].state = NpcState::Orbit {
            around: 0,
            phase: 0.0,
        };
        assert!(!s.is_ambush(k), "too close to be unseen");
        // Cut the machinery: the ship goes cold and the same distance becomes an ambush.
        for p in s.ship.sys.powered.iter_mut() {
            *p = false;
        }
        assert_eq!(s.quiet_signature_kw(), 0.0);
        s.npcs[k].pos = s.ship.pos + vector_core::Vec3::new(0.0, 0.0, ACTIVE_RANGE + 1000.0);
        assert!(
            s.is_ambush(k),
            "a cold ship is a shadow beyond five kilometres"
        );
        // A target already on alert is never ambushed.
        s.npcs[k].state = NpcState::Combat {
            mode: AiMode::Pursue,
        };
        assert!(!s.is_ambush(k));
    }
}
