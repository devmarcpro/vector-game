//! The works of a colony: what makes it an industrial, agricultural and urban place rather than a
//! bag of numbers. Everything here extends `colony.rs` (one colony, one set of buildings and
//! stocks, one tick) and is driven by data: `buildings.ron` carries the capacities, `crops.ron`
//! the agronomy of the domestic producers of `species.ron`, `techs.ron` the abilities unlocked.
//!
//! * Industry: wear and upkeep (spare parts, workshops), tooling (tools in stock), automation
//!   that replaces workers, a quality index (norms, metrology, skill) whose defects go to the
//!   waste heap, and for each building the factor that limits it (inputs, power, staff, ...).
//! * Farming: soil, climate of the city site (temperature through the seasons, humidity, rain),
//!   irrigation, fertiliser, rotation of families, greenhouses, spoilage and cold stores,
//!   cooking and a three-axis nutrition of the people that follows what is actually grown.
//! * City: density, sanitation and waste (a closed cycle with compost and sorting), pollution,
//!   crime born of unemployment, crowding, pollution and poverty, transit that gets workers to
//!   work, convoys with real delays, orbital lifts, sieges that cut the lanes.
use crate::colony::{BuildingInst, Colony};
use crate::crew::DAY;
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng};
use vector_data::{GameData, CAPACITIES};

/// Plants per hectare used to turn the growth rate and yield of a species into kg per day.
pub const FARM_STAND: f64 = 260.0;
/// Days a crop stays in the fields before the rotation may change it.
pub const SEASON_DAYS: f64 = 40.0;
/// Fertiliser a hectare takes per day, kg, and water of the irrigation (open) and of the
/// greenhouses, kg per hectare and day.
pub const FERT_KG_HA_DAY: f64 = 0.12;
pub const IRRIG_WATER: f64 = 60.0;
pub const SHELTER_WATER: f64 = 30.0;
/// Share of the food that spoils in a day without any cold.
pub const SPOIL_RATE: f64 = 0.006;
/// Household waste, kg per person and day.
pub const WASTE_PER_PERSON: f64 = 0.25;
/// Tools a worker wears out per day, kg.
pub const TOOLS_PER_JOB: f64 = 0.01;
/// Spare parts that bring a building back by one percent of health, kg.
pub const PARTS_PER_PERCENT: f64 = 0.8;
/// Buildings one workshop keeps in order.
pub const WORKSHOP_SERVES: f64 = 8.0;
/// Persons per hectare above which a district is too dense.
pub const DENSE: f64 = 250.0;
/// Profile (energy, protein, vitamins) of food whose origin is unknown (bought, imported).
pub const DEFAULT_MIX: [f64; 3] = [0.7, 0.35, 0.35];

/// What stops a building from working at full rate.
pub const LIMIT_NONE: u8 = 0;
pub const LIMIT_INPUT: u8 = 1;
pub const LIMIT_POWER: u8 = 2;
pub const LIMIT_STAFF: u8 = 3;
pub const LIMIT_SPACE: u8 = 4;
pub const LIMIT_WORN: u8 = 5;
pub const LIMIT_TOOLS: u8 = 6;
pub const LIMIT_OFF: u8 = 7;

pub fn limit_label(l: u8) -> &'static str {
    match l {
        LIMIT_INPUT => "INTRANTS",
        LIMIT_POWER => "ENERGIE",
        LIMIT_STAFF => "PERSONNEL",
        LIMIT_SPACE => "STOCKAGE",
        LIMIT_WORN => "USURE",
        LIMIT_TOOLS => "OUTILLAGE",
        LIMIT_OFF => "ARRET",
        _ => "-",
    }
}

/// Per-building operating state, rewritten every tick.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Ops {
    /// The factor that holds the building back (`LIMIT_*`).
    pub limit: u8,
    /// Share of its rated throughput it used last tick, 0..1.
    pub load: f32,
}

/// Where a colony stands: what its fields get from the sky and the ground. Computed from the
/// surface map of the world when the colony is founded (or first met).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Env {
    pub ready: bool,
    /// Fertility of the biome of the city, 0..1.
    pub fertility: f64,
    /// Humidity of the city site, 0..1.
    pub hum: f64,
    /// Annual mean temperature, K, half-amplitude of the seasons and phase of the year (0..1) of
    /// the warmest day.
    pub t_mean: f64,
    pub t_amp: f64,
    pub t_peak: f64,
    /// Length of the year, s.
    pub year: f64,
    /// A sea within reach (algae, fish).
    pub coast: bool,
    /// Open air fit for open fields (pressure at least a fifth of a bar).
    pub air: bool,
}

impl Default for Env {
    fn default() -> Env {
        Env {
            ready: false,
            fertility: 0.1,
            hum: 0.3,
            t_mean: 288.0,
            t_amp: 0.0,
            t_peak: 0.25,
            year: 3.15e7,
            coast: false,
            air: false,
        }
    }
}

impl Env {
    /// Outdoor temperature at game time `t`, K.
    pub fn temp_at(&self, t: f64) -> f64 {
        let phase = (t / self.year.max(1.0)).fract();
        self.t_mean + self.t_amp * (std::f64::consts::TAU * (phase - self.t_peak)).cos()
    }
}

/// Goods on their way to a colony.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shipment {
    pub res: usize,
    pub kg: f64,
    /// Game time of arrival, s.
    pub due: f64,
    /// Colony that sent it.
    pub from: u32,
}

/// What the fields did over the last tick (display and tests).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FarmStat {
    pub ha: f64,
    pub yield_day: f64,
    pub temp_f: f64,
    pub water_f: f64,
    pub soil_f: f64,
    pub light: f64,
    pub irrigated: f64,
    pub fert_ratio: f64,
}

/// Everything the works of a colony remember.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Works {
    pub env: Env,
    /// Fertility of the soil now, 1 = good ground (see `Env::fertility` for its ceiling).
    pub soil: f64,
    /// Crop in the fields (index into `GameData::crops`) and the one before; when it was sown.
    pub crop: usize,
    pub prev_crop: usize,
    pub planted: f64,
    /// Let the rotation choose the crops (needs the `crop_rotation` capacity).
    pub rotate: bool,
    /// Nutrition (energy, protein, vitamins) of the food in store, and the score of the people.
    pub mix: [f64; 3],
    pub nutrition: f64,
    /// Food in store after the last tick (what is above it came from elsewhere).
    pub food_track: f64,
    pub farm: FarmStat,
    pub spoiled: f64,
    /// Waste left in the streets, kg; pollution and crime, 0..1.
    pub litter: f64,
    pub pollution: f64,
    pub crime: f64,
    /// Emission units of the day (see `emission`): what the works put in the air, derived each
    /// tick and handed to the climate of the world (`Terra::emit_co2`). Not saved.
    #[serde(skip)]
    pub emitted: f64,
    /// Goodwill earned or lost with the faction of the sector, not yet handed over (see
    /// `links::colony_relations_tick`). Not saved.
    #[serde(skip)]
    pub rel_acc: f32,
    /// Food that went through the port since the sim last looked: imported (+) or exported (-),
    /// kg, for the sim to move in the stock of the market of the station nearby (see
    /// `links::settle_station_food`). Not saved.
    #[serde(skip)]
    pub trade_flow: f64,
    /// Quality index of what the colony makes (0..1) and the share lost to defects.
    pub quality: f64,
    pub defect: f64,
    /// Share of the tools needed that were found, of the buildings kept in order, and of the
    /// labour that machines replace.
    pub tool_cover: f64,
    pub maint_cover: f64,
    pub automation: f64,
    /// Share of the commute that the transit carries, sanitation coverage, density (people per
    /// hectare), built area (hectares) and unemployment.
    pub mobility: f64,
    pub sanit_cover: f64,
    pub density: f64,
    pub area_ha: f64,
    pub unemployment: f64,
    /// Resource that held most buildings back, and how many.
    pub short_res: Option<(usize, u32)>,
    pub inbound: Vec<Shipment>,
    /// War: the lanes are cut (0..1), set by the sim from the front line.
    pub siege: f64,
    /// Rain of the day (0..1) and heat or dust (0..1), from the weather of the city.
    pub rain: f64,
    pub harsh: f64,
    /// Local price of food relative to its base value, set by the sim.
    pub food_price: f64,
    /// Harvest of the year in the sector (1 = a normal one, see `macroeco::harvest`), set by the
    /// sim: the open fields suffer the drought that makes bread dear in the markets. Derived,
    /// not saved.
    #[serde(skip, default = "one_year")]
    pub harvest: f64,
    /// Credits earned selling surplus food, spent on imports, since the founding.
    pub sold: f64,
    pub bought: f64,
    /// Day of the last history sample, and the history: (food days, pollution, crime,
    /// nutrition, power satisfaction, productivity).
    pub hist_day: i64,
    /// Day of the last pass of the weather, the war and the prices (see `apply_works_world`).
    pub world_day: i64,
    pub hist: Vec<[f32; 6]>,
    /// Last day a theft was told, a lack of parts was told.
    pub told: [i64; 4],
}

impl Default for Works {
    fn default() -> Works {
        Works {
            env: Env::default(),
            soil: 0.5,
            crop: usize::MAX,
            prev_crop: usize::MAX,
            planted: 0.0,
            rotate: true,
            mix: DEFAULT_MIX,
            nutrition: 0.8,
            food_track: 0.0,
            farm: FarmStat::default(),
            spoiled: 0.0,
            litter: 0.0,
            pollution: 0.0,
            crime: 0.05,
            emitted: 0.0,
            rel_acc: 0.0,
            trade_flow: 0.0,
            quality: 0.5,
            defect: 0.1,
            tool_cover: 1.0,
            maint_cover: 0.35,
            automation: 0.0,
            mobility: 1.0,
            sanit_cover: 0.15,
            density: 0.0,
            area_ha: 0.0,
            unemployment: 0.0,
            short_res: None,
            inbound: Vec::new(),
            siege: 0.0,
            rain: 0.0,
            harsh: 0.0,
            food_price: 1.0,
            harvest: 1.0,
            sold: 0.0,
            bought: 0.0,
            hist_day: i64::MIN,
            world_day: i64::MIN,
            hist: Vec::new(),
            told: [i64::MIN; 4],
        }
    }
}

fn one_year() -> f64 {
    1.0
}

/// Days of history kept for the charts.
pub const HIST_LEN: usize = 60;

pub fn cap_bit(id: &str) -> u32 {
    CAPACITIES
        .iter()
        .position(|c| *c == id)
        .map_or(0, |i| 1u32 << i)
}

/// Wear of a category of building, health per day at full use.
pub fn wear_rate(category: &str) -> f64 {
    match category {
        "housing" => 0.0004,
        "power" => 0.0008,
        "extraction" => 0.003,
        "industry" => 0.002,
        "agriculture" => 0.0016,
        "storage" => 0.0004,
        "infrastructure" => 0.0008,
        "urbanism" => 0.0012,
        "spaceport" => 0.0008,
        "terraform" => 0.002,
        "defense" => 0.0006,
        _ => 0.001,
    }
}

/// Pollution a building of a category emits per day at full use (before its own `pollution`).
pub fn emission(category: &str, burns: bool) -> f64 {
    match category {
        "extraction" => 0.6,
        "industry" => 1.0,
        "power" if burns => 0.7,
        "spaceport" => 0.5,
        "agriculture" => 0.1,
        _ => 0.0,
    }
}

/// Hectares a building occupies in the city (the fields of a farm are not city).
pub fn footprint(d: &vector_data::Building) -> f64 {
    if d.housing > 0.0 {
        (d.housing / (80.0 * (1.0 + d.housing / 400.0))).max(0.5)
    } else {
        0.35 + d.storage / 150_000.0
    }
}

/// Temperature window (K) a crop can grow in: the union of its wild habitats.
pub fn crop_window(data: &GameData, crop: usize) -> (f64, f64) {
    let sp = &data.species[data.species_index(&data.crops[crop].species).unwrap_or(0)];
    let mut w = (f64::MAX, f64::MIN);
    for b in &sp.habitat {
        if let Some(i) = data.biome_index(b) {
            w.0 = w.0.min(data.biomes[i].t.0);
            w.1 = w.1.max(data.biomes[i].t.1.min(400.0));
        }
    }
    if w.0 > w.1 {
        (270.0, 310.0)
    } else {
        w
    }
}

/// Growth factor of a crop at temperature `t` (K): 1 in the middle of its window, 0.4 at its
/// edges, nothing 8 K beyond.
pub fn temp_factor(window: (f64, f64), t: f64) -> f64 {
    let (lo, hi) = window;
    let mid = (lo + hi) / 2.0;
    let half = ((hi - lo) / 2.0).max(1.0);
    if t < lo - 8.0 || t > hi + 8.0 {
        0.0
    } else if t < lo {
        0.4 * (t - (lo - 8.0)) / 8.0
    } else if t > hi {
        0.4 * ((hi + 8.0) - t) / 8.0
    } else {
        1.0 - 0.6 * ((t - mid).abs() / half)
    }
}

/// Does the crop live only in the sea (algae)?
fn sea_crop(data: &GameData, crop: usize) -> bool {
    let sp = &data.species[data.species_index(&data.crops[crop].species).unwrap_or(0)];
    sp.habitat
        .iter()
        .all(|b| b.starts_with("ocean") || b == "mer_gelee")
}

/// Kg of food per hectare and day of a crop in perfect conditions.
pub fn crop_yield(data: &GameData, crop: usize) -> f64 {
    let sp = &data.species[data.species_index(&data.crops[crop].species).unwrap_or(0)];
    let food = sp
        .yields
        .iter()
        .find(|(r, _)| r == "food")
        .map_or(0.0, |(_, k)| *k);
    FARM_STAND * sp.growth * food
}

/// Convoy delay in days for `kg` over a leg, and the largest load one dispatch can take.
/// `same_body`: both colonies are on the same world (roads); otherwise they are in the same
/// system and the goods go up and down (an orbital lift on either end makes it quick). `dist_au`
/// is the distance between the two worlds.
pub fn convoy_delay(same_body: bool, dist_au: f64, kg: f64, lift: f64, elevator: bool) -> f64 {
    let loading = kg / lift.max(500.0);
    let leg = if same_body {
        0.4 + 0.3 * (kg / 20_000.0).min(2.0)
    } else {
        2.0 + 6.0 * dist_au * if elevator { 0.7 } else { 1.0 }
    };
    (0.3 + loading + leg) * if elevator { 0.6 } else { 1.0 }
}

impl Colony {
    pub fn has_cap(&self, id: &str) -> bool {
        self.caps & cap_bit(id) != 0
    }

    /// Active (built, on, sound) buildings with their definitions.
    pub(crate) fn active_defs<'a>(
        &'a self,
        data: &'a GameData,
    ) -> impl Iterator<Item = (usize, &'a BuildingInst, &'a vector_data::Building)> + 'a {
        self.buildings
            .iter()
            .enumerate()
            .filter(|(_, b)| b.built() && b.enabled && b.health > 0.2)
            .map(|(i, b)| (i, b, &data.buildings[b.def]))
    }

    /// Built area of the city, hectares.
    pub fn area_ha(&self, data: &GameData) -> f64 {
        self.buildings
            .iter()
            .filter(|b| b.built())
            .map(|b| footprint(&data.buildings[b.def]))
            .sum()
    }

    /// Share of the labour the machines replace: the automation of the hubs and cells, capped
    /// by what the technology allows (half without `automation`, none without anything).
    pub fn automation_level(&self, data: &GameData) -> f64 {
        let raw: f64 = self
            .active_defs(data)
            .map(|(_, b, d)| d.automation * b.health)
            .sum();
        let cap = if self.has_cap("robotics") {
            0.8
        } else if self.has_cap("automation") {
            0.55
        } else {
            0.2
        };
        raw.min(cap)
    }

    /// Quality index of the production, 0..1, from the tooling, the skill of the workers, the
    /// machines and the standards offices.
    pub fn quality_index(&self, data: &GameData, tool_ok: f64, skill: f64, auto: f64) -> f64 {
        let industrial = self
            .active_defs(data)
            .filter(|(_, _, d)| !d.recipes.is_empty())
            .count() as f64;
        let office: f64 = self
            .active_defs(data)
            .map(|(_, b, d)| d.quality * b.health)
            .sum();
        let norms = if self.has_cap("norms") { 1.0 } else { 0.5 };
        let office_cover = if industrial > 0.0 {
            (office * norms / (industrial * 0.1).max(0.5)).min(1.0)
        } else {
            0.0
        };
        let tools = if self.has_cap("precision_tooling") {
            1.0
        } else {
            0.7
        };
        (0.30
            + 0.20 * tool_ok * tools
            + 0.20 * skill
            + 0.15 * (auto / 0.8).min(1.0)
            + 0.25 * office_cover)
            .clamp(0.0, 1.0)
    }

    /// Choose the crop to sow: the most promising for the temperature now, tiring the soil less,
    /// not of the family just harvested, filling the gaps of the diet.
    pub fn pick_crop(&self, data: &GameData, now: f64, shelter: f64) -> Option<usize> {
        if data.crops.is_empty() {
            return None;
        }
        let t = self.works.env.temp_at(now);
        let prev_family = data
            .crops
            .get(self.works.prev_crop)
            .map(|c| c.family.as_str());
        let cur_family = data.crops.get(self.works.crop).map(|c| c.family.as_str());
        let mix = self.works.mix;
        let gap = [
            (0.6 - mix[0]).max(0.0),
            (0.4 - mix[1]).max(0.0),
            (0.35 - mix[2]).max(0.0),
        ];
        let mut best: Option<(usize, f64)> = None;
        for (i, c) in data.crops.iter().enumerate() {
            if sea_crop(data, i) && !self.works.env.coast {
                continue;
            }
            let open = temp_factor(crop_window(data, i), t);
            // Fields under glass feel less of the weather.
            let ft = open + shelter * (1.0 - open);
            if ft <= 0.02 {
                continue;
            }
            let rain = (self.works.env.hum * 1.15 / c.water).clamp(0.2, 1.0);
            let water = rain + shelter * (1.0 - rain);
            let mut score = crop_yield(data, i) * ft * water;
            if Some(c.family.as_str()) == cur_family || Some(c.family.as_str()) == prev_family {
                score *= 0.5;
            }
            if c.draw < 0.0 && self.works.soil < 0.7 {
                score *= 1.6; // legumes feed a tired soil
            }
            let fill = c.nutrition.0 * gap[0] + c.nutrition.1 * gap[1] + c.nutrition.2 * gap[2];
            score *= 1.0 + 1.5 * fill;
            if best.is_none_or(|(_, s)| score > s) {
                best = Some((i, score));
            }
        }
        best.map(|b| b.0)
    }

    /// The fields: soil, climate, water, fertiliser, rotation and the harvest. Returns the kg of
    /// food harvested. `power_sat` is the share of the load the grid serves, `prod_mult` the
    /// productivity of the labour.
    pub(crate) fn farm_tick(
        &mut self,
        data: &GameData,
        days: f64,
        now: f64,
        power_sat: f64,
        prod_mult: f64,
    ) -> f64 {
        let farms: Vec<(f64, f64, f64)> = self
            .active_defs(data)
            .filter(|(_, _, d)| d.farm > 0.0)
            .map(|(_, b, d)| (d.farm * b.health, d.shelter, d.irrigation))
            .collect();
        let ha_total: f64 = farms.iter().map(|f| f.0).sum();
        self.works.farm = FarmStat::default();
        if ha_total <= 0.0 || data.crops.is_empty() {
            return 0.0;
        }
        // Sowing: the first crop, then the rotation each season if the technology is there.
        let shelter = farms.iter().map(|f| f.0 * f.1).sum::<f64>() / ha_total;
        let invalid = self.works.crop >= data.crops.len();
        let due = now - self.works.planted >= SEASON_DAYS * DAY;
        if invalid || due {
            let rotating = self.works.rotate && self.has_cap("crop_rotation");
            let next = if invalid || rotating {
                self.pick_crop(data, now, shelter)
            } else {
                Some(self.works.crop)
            };
            if let Some(c) = next {
                // Sowing the family just harvested again is what tires the soil.
                if !invalid {
                    self.works.prev_crop = self.works.crop;
                }
                self.works.crop = c;
                self.works.planted = now;
            }
        }
        let crop = self.works.crop;
        if crop >= data.crops.len() {
            return 0.0;
        }
        let c = &data.crops[crop];
        let env = self.works.env.clone();
        let t = env.temp_at(now);
        let ft_open = temp_factor(crop_window(data, crop), t);
        // Light: the sun at the colony after its sky; domes and halls borrow it.
        let light = (self.flux * crate::colony::solar_transmittance(self.pressure))
            .sqrt()
            .clamp(0.05, 1.0);
        let open_ha: f64 = farms.iter().filter(|f| f.1 < 0.5).map(|f| f.0).sum();
        let shelter_ha: f64 = farms.iter().filter(|f| f.1 >= 0.5).map(|f| f.0).sum();
        // Water: rain and humidity, then the irrigation the colony can pump.
        let irrig_ha: f64 = self
            .active_defs(data)
            .map(|(_, b, d)| d.irrigation * b.health)
            .sum::<f64>()
            * if self.has_cap("irrigation") { 1.0 } else { 0.0 };
        let irrig_share = if open_ha > 0.0 {
            (irrig_ha / open_ha).min(1.0)
        } else {
            0.0
        };
        let water_need_kg =
            (irrig_share * open_ha * IRRIG_WATER + shelter_ha * SHELTER_WATER) * days;
        let water_got = self.take_stock(data, "water", water_need_kg);
        let water_ratio = if water_need_kg > 0.0 {
            water_got / water_need_kg
        } else {
            1.0
        };
        let rain_supply = env.hum * 1.15 + 0.4 * self.works.rain;
        let supply = rain_supply + irrig_share * water_ratio * (1.4 - rain_supply).max(0.0);
        let w_open = (supply / c.water).clamp(0.0, 1.0);
        // Fertiliser: what the fields take keeps the soil, twice as much with the chemistry.
        let fert_need = FERT_KG_HA_DAY * ha_total * days;
        let fert_got = self.take_stock(data, "fertilizer", fert_need);
        let fert_ratio = if fert_need > 0.0 {
            fert_got / fert_need
        } else {
            0.0
        };
        let fert_eff = fert_ratio
            * if self.has_cap("synthetic_fertilizer") {
                1.0
            } else {
                0.6
            };
        // Soil: the crop draws on it (a family repeated twice as much), rest and fertiliser feed it.
        let repeated = data
            .crops
            .get(self.works.prev_crop)
            .is_some_and(|p| p.family == c.family);
        let soil_max = (0.35 + 0.85 * env.fertility).clamp(0.3, 1.2);
        let draw = c.draw * if repeated && c.draw > 0.0 { 2.0 } else { 1.0 };
        let regen = 0.0015 + 0.006 * fert_eff;
        let d_soil = if draw >= 0.0 {
            -draw * (1.0 - 0.75 * fert_eff) + regen * ((soil_max - self.works.soil) / soil_max)
        } else {
            -draw + regen * ((soil_max - self.works.soil) / soil_max)
        };
        self.works.soil = (self.works.soil + d_soil * days).clamp(0.05, soil_max + 0.25);
        let soil_f = self.works.soil.clamp(0.05, 1.2);
        // Harvest.
        let base = crop_yield(data, crop);
        let harsh = 1.0 - 0.3 * self.works.harsh;
        let smog = 1.0 - 0.25 * self.works.pollution;
        let pests = 1.0 - self.pest.clamp(0.0, 0.5);
        let mut per_day = 0.0;
        let power = power_sat.clamp(0.0, 1.0);
        for (ha, shelter, _) in &farms {
            let air_ok = env.air || *shelter >= 0.5;
            if !air_ok || (sea_crop(data, crop) && !env.coast && *shelter < 0.5) {
                continue;
            }
            let f_t = ft_open + shelter * (1.0 - ft_open);
            let w = w_open
                + shelter * (1.0 - w_open) * if shelter_ha > 0.0 { water_ratio } else { 1.0 };
            let s = soil_f + shelter * (0.9 - soil_f).max(0.0);
            let l = if *shelter >= 0.5 {
                light.max(0.5)
            } else {
                light
            };
            let rot = if self.has_cap("crop_rotation") && !repeated {
                1.1
            } else {
                1.0
            };
            // The sector's year of drought or plenty reaches the open fields only.
            let year = 1.0 + (self.works.harvest - 1.0) * (1.0 - shelter.clamp(0.0, 1.0));
            per_day += ha
                * base
                * year
                * f_t
                * w
                * s
                * l
                * rot
                * harsh
                * smog
                * pests
                * if *shelter >= 0.5 { power } else { 1.0 };
        }
        let kg = (per_day * days * prod_mult.clamp(0.0, 1.5)).min(self.free_space());
        self.works.farm = FarmStat {
            ha: ha_total,
            yield_day: per_day,
            temp_f: ft_open,
            water_f: w_open,
            soil_f,
            light,
            irrigated: irrig_share,
            fert_ratio,
        };
        if kg > 0.0 {
            if let Some(fi) = data.resource_index("food") {
                let stock = self.storage[fi];
                let p = [c.nutrition.0, c.nutrition.1, c.nutrition.2];
                for (m, pv) in self.works.mix.iter_mut().zip(p) {
                    *m = (*m * stock + pv * kg) / (stock + kg);
                }
                self.storage[fi] += kg;
            }
        }
        kg
    }

    /// Take up to `kg` of resource `id` from the stores; returns what was taken.
    pub(crate) fn take_stock(&mut self, data: &GameData, id: &str, kg: f64) -> f64 {
        match data.resource_index(id) {
            Some(i) if kg > 0.0 => {
                let got = self.storage[i].min(kg);
                self.storage[i] -= got;
                got
            }
            _ => 0.0,
        }
    }

    /// Let the food spoil (cold stores slow it down) and the waste pile up. Returns the kg lost.
    pub(crate) fn spoil_tick(
        &mut self,
        data: &GameData,
        days: f64,
        now: f64,
        power_sat: f64,
    ) -> f64 {
        let Some(fi) = data.resource_index("food") else {
            return 0.0;
        };
        let stock = self.storage[fi];
        if stock <= 0.0 {
            self.works.spoiled = 0.0;
            return 0.0;
        }
        let t = self.works.env.temp_at(now);
        let heat = 0.5 + 0.5 * ((t - 273.0) / 40.0).clamp(0.0, 2.0);
        let cold: f64 = self
            .active_defs(data)
            .map(|(_, b, d)| d.preserve * b.health)
            .sum::<f64>()
            * power_sat.clamp(0.0, 1.0);
        let cover = (cold / stock.max(1.0)).min(1.0);
        let rate = SPOIL_RATE * heat * (1.0 - 0.9 * cover);
        let lost = stock * (1.0 - (-rate * days).exp());
        self.storage[fi] -= lost;
        self.works.spoiled = if days > 0.0 { lost / days } else { 0.0 };
        lost
    }

    /// Put waste where it goes: collected into the stores (a landfill that fills up), or left in
    /// the streets. Returns nothing; `works.litter` and the stock of waste move.
    pub(crate) fn dump_waste(&mut self, data: &GameData, kg: f64, collected: f64) {
        let Some(wi) = data.resource_index("waste") else {
            return;
        };
        let into_store = kg * collected;
        // The landfill takes a quarter of the stores at most.
        let room = (self.stats.storage_cap.max(5000.0) * 0.25 - self.storage[wi])
            .max(0.0)
            .min(self.free_space());
        let put = into_store.min(room);
        self.storage[wi] += put;
        self.works.litter += kg - put;
    }

    /// Daily record for the charts.
    fn record_history(&mut self, now: f64, food_days: f64) {
        let day = (now / DAY).floor() as i64;
        if day == self.works.hist_day {
            return;
        }
        self.works.hist_day = day;
        let w = &self.works;
        let s = &self.stats;
        let sample = [
            food_days.min(200.0) as f32,
            w.pollution as f32,
            w.crime as f32,
            w.nutrition as f32,
            s.power_sat as f32,
            s.prod_mult as f32,
        ];
        self.works.hist.push(sample);
        if self.works.hist.len() > HIST_LEN {
            self.works.hist.remove(0);
        }
    }
}

/// Inputs of the city tick the colony computes before the others.
pub(crate) struct CityIn {
    pub days: f64,
    pub now: f64,
    pub vital_sat: f64,
    pub sec: f64,
    pub happiness: f64,
}

/// City results the main tick feeds into the mood, the health and the epidemics.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CityOut {
    pub mood: f64,
    pub deaths: f64,
    pub crowd: f64,
    pub income_mult: f64,
}

impl Colony {
    /// Sanitation, waste, pollution, density, crime and the siege: the city side of the tick.
    pub(crate) fn city_tick(&mut self, data: &GameData, x: &CityIn, extra_waste: f64) -> CityOut {
        let days = x.days;
        // Density and area.
        let area = self.area_ha(data).max(1.0);
        let density = self.pop / area;
        self.works.area_ha = area;
        self.works.density = density;
        let thr = if self.has_cap("district_planning") {
            DENSE * 1.6
        } else {
            DENSE
        };
        let excess = ((density - thr) / thr).clamp(0.0, 1.0);
        // Sanitation and the waste heap.
        let power_ok = x.vital_sat.min(1.0);
        let sanit: f64 = self
            .active_defs(data)
            .map(|(_, b, d)| d.sanitation * b.health)
            .sum::<f64>()
            * power_ok;
        let cover = if self.pop > 0.0 {
            (sanit / self.pop).min(1.0)
        } else {
            1.0
        };
        let eff = if self.has_cap("sanitation") {
            cover
        } else {
            cover * 0.7
        };
        let collected = 0.15 + 0.85 * eff;
        self.works.sanit_cover = collected;
        let waste = self.pop * WASTE_PER_PERSON * days + extra_waste;
        self.dump_waste(data, waste, collected);
        // Litter washes away slowly; what stays is dirt and disease.
        self.works.litter = (self.works.litter * (1.0 - 0.015 * days).max(0.0)).max(0.0);
        // Pollution: industry and fuel burn emit; scrubbers, sanitation and the wind clean.
        let mut emitted = 0.0;
        let mut cleaned = 0.0;
        for (_, b, d) in self.active_defs(data) {
            let load = if d.recipes.is_empty() {
                1.0
            } else {
                f64::from(b.ops.load).max(0.2)
            };
            emitted += emission(&d.category, !d.burns.is_empty()) * load * b.health;
            if d.pollution > 0.0 {
                emitted += d.pollution * load;
            } else {
                cleaned += -d.pollution
                    * if self.has_cap("air_scrubbing") {
                        1.0
                    } else {
                        0.6
                    };
            }
        }
        self.works.emitted = emitted;
        let air = 0.4 + 0.6 * self.pressure.clamp(0.0, 1.0);
        let target =
            ((emitted * 0.03 + self.works.litter / 8000.0 - cleaned * 0.04) / air).clamp(0.0, 1.0);
        self.works.pollution += (target - self.works.pollution) * (1.0 - (-days / 12.0).exp());
        self.works.pollution = self.works.pollution.clamp(0.0, 1.0);
        // Crime: unemployment, hunger of the mood, crowding and dirt breed it; police and a
        // strict order hold it back.
        let (_, _, sec_h) = self.policy_level(data, "security");
        let martial = if self.policy_value(data, "security") >= 2 {
            0.7
        } else {
            1.0
        };
        let poverty = (1.0 - x.happiness).clamp(0.0, 1.0);
        let crime_target = ((0.03
            + 0.30 * self.works.unemployment
            + 0.25 * poverty * 0.6
            + 0.15 * self.works.pollution
            + 0.20 * excess
            + 0.10 * self.unrest
            - 0.35 * x.sec
            - sec_h.max(0.0) * 0.0)
            * martial)
            .clamp(0.0, 1.0);
        self.works.crime += (crime_target - self.works.crime) * (1.0 - (-days / 12.0).exp());
        self.works.crime = self.works.crime.clamp(0.0, 1.0);
        let day = (x.now / DAY).floor() as i64;
        if self.works.crime > 0.35 && self.treasury > 100.0 && self.works.told[0] != day {
            let mut rng = Rng::new(hash_coord(self.system_seed, i64::from(self.id), day, 61));
            if rng.f64() < self.works.crime * 0.05 * days.max(0.25) * 4.0 {
                let stolen = (self.treasury * 0.04 * rng.range(0.5, 1.5)).round();
                self.treasury -= stolen;
                self.works.told[0] = day;
                self.log_line(
                    x.now,
                    format!("Criminalite : un braquage emporte {stolen:.0} CR de la caisse."),
                );
            }
        }
        // Siege: shortages, fear, and shells on the buildings the turrets do not cover.
        let defense: f64 = self
            .active_defs(data)
            .map(|(_, b, d)| d.defense * b.health)
            .sum();
        let def_cover = (defense / (self.pop / 400.0 + 1.0)).min(1.0);
        let mut mood = -0.2
            * self.works.pollution
            * if self.has_cap("district_planning") {
                0.7
            } else {
                1.0
            }
            - 0.12 * self.works.crime
            - 0.05 * excess
            - 0.08 * self.works.siege * (1.0 - def_cover);
        if self.works.siege > 0.0 && !self.buildings.is_empty() {
            let mut rng = Rng::new(hash_coord(self.system_seed, i64::from(self.id), day, 63));
            let k = (rng.next_u64() % self.buildings.len() as u64) as usize;
            self.buildings[k].health = (self.buildings[k].health
                - 0.006 * days * self.works.siege * (1.0 - def_cover))
                .max(0.0);
            if rng.f64() < 0.05 && self.works.told[2] != day {
                self.works.told[2] = day;
                self.log_line(x.now, "Siege : des obus frappent la ville.");
            }
        }
        mood += 0.0;
        let crowd_epi =
            (0.15 * excess + 0.2 * (1.0 - collected) + 0.15 * self.works.pollution).min(0.5);
        CityOut {
            mood,
            deaths: self.pop
                * (0.00015 * self.works.pollution
                    + 0.00008 * (1.0 - collected) * (1.0 - collected))
                * days,
            crowd: crowd_epi,
            income_mult: 1.0 - 0.4 * self.works.crime,
        }
    }

    /// Industry upkeep: wear from use, repairs by the crews and workshops from the spare parts
    /// in store. `load` of each building must have been set by the production loop.
    pub(crate) fn upkeep_tick(&mut self, data: &GameData, days: f64, now: f64, auto: f64) {
        let units = self
            .buildings
            .iter()
            .filter(|b| b.built() && data.buildings[b.def].category != "housing")
            .count() as f64;
        let workshops: f64 = self
            .active_defs(data)
            .map(|(_, b, d)| d.tooling * b.health)
            .sum();
        let mcover = if units > 0.0 {
            (workshops * WORKSHOP_SERVES / units).min(1.0)
        } else {
            1.0
        };
        let cover = (0.35 + 0.65 * mcover + 0.1 * auto).min(1.0);
        self.works.maint_cover = cover;
        let tool_gap = 1.0 - self.works.tool_cover;
        // Wear.
        for b in self.buildings.iter_mut().filter(|b| b.built()) {
            let d = &data.buildings[b.def];
            let load = if b.enabled {
                f64::from(b.ops.load).max(0.3)
            } else {
                0.15
            };
            let load = if d.recipes.is_empty() && b.enabled {
                1.0
            } else {
                load
            };
            let k = if d.wear > 0.0 { d.wear } else { 1.0 };
            let w = wear_rate(&d.category) * k * (0.4 + 0.6 * load) * (1.0 + 0.6 * tool_gap) * days;
            b.health = (b.health - w).max(0.0);
        }
        // Repairs, as far as the spare parts go.
        // Walls and roofs are mended with construction materials, machines with spare parts.
        let mut want: Vec<(usize, f64, bool)> = Vec::new();
        for (i, b) in self.buildings.iter().enumerate().filter(|(_, b)| b.built()) {
            if b.health < 0.999 {
                let structural = matches!(
                    data.buildings[b.def].category.as_str(),
                    "housing" | "storage" | "infrastructure" | "spaceport" | "defense"
                );
                want.push((i, (1.0 - b.health).min(0.012 * cover * days), structural));
            }
        }
        let need = |structural: bool| -> f64 {
            want.iter()
                .filter(|w| w.2 == structural)
                .map(|w| w.1)
                .sum::<f64>()
                * 100.0
        };
        let (need_parts, need_mat) = (need(false) * PARTS_PER_PERCENT, need(true) * 2.0);
        if need_parts + need_mat <= 0.0 {
            return;
        }
        let got_parts = self.take_stock(data, "spare_parts", need_parts);
        let got_mat = self.take_stock(data, "construction_materials", need_mat);
        let ratio_of = |got: f64, need: f64| if need > 0.0 { got / need } else { 0.0 };
        let (r_parts, r_mat) = (ratio_of(got_parts, need_parts), ratio_of(got_mat, need_mat));
        for (i, amount, structural) in want {
            let ratio = if structural { r_mat } else { r_parts };
            self.buildings[i].health = (self.buildings[i].health + amount * ratio).min(1.0);
        }
        let ratio = if need_parts > 0.0 { r_parts } else { r_mat };
        let worn = self
            .buildings
            .iter()
            .filter(|b| b.built() && b.health < 0.5)
            .count();
        let day = (now / DAY).floor() as i64;
        if ratio < 0.2 && worn > 0 && self.works.told[1] + 15 <= day {
            self.works.told[1] = day;
            self.log_line(
                now,
                format!("Entretien : faute de pieces detachees, {worn} batiments se degradent."),
            );
        }
    }

    /// Imports to avoid a famine and exports of a glut, at the local price. Needs the convoys of
    /// the road engineers, a port and open lanes. Returns nothing: stocks and the treasury move.
    pub(crate) fn trade_tick(&mut self, data: &GameData, days: f64, now: f64) {
        if !self.has_cap("convoys") || self.works.siege > 0.5 || !self.has_port(data) {
            return;
        }
        let Some(fi) = data.resource_index("food") else {
            return;
        };
        let daily = (self.pop * 0.6).max(1.0);
        let stock_days = self.storage[fi] / daily;
        // The price is the one of the sector (see `apply_works_world`); what goes through the port
        // is booked in `trade_flow`, and the sim moves it in the stock of the nearest market.
        let price = data.resources[fi].value * self.works.food_price.clamp(0.3, 6.0);
        if stock_days < 10.0 && self.treasury > 0.0 {
            let want = (10.0 - stock_days) * daily * (days / 2.0).min(1.0);
            let cost = price * 1.4;
            let kg = want
                .min(self.treasury * 0.5 / cost)
                .min(self.free_space())
                .min(daily * days * 3.0);
            if kg > 1.0 {
                self.works.trade_flow += kg;
                self.add_food_with_mix(fi, kg, DEFAULT_MIX);
                self.treasury -= kg * cost;
                self.works.bought += kg * cost;
                let day = (now / DAY).floor() as i64;
                if self.works.told[3] + 10 <= day {
                    self.works.told[3] = day;
                    self.log_line(
                        now,
                        format!("Importation de vivres : {kg:.0} kg pour eviter la disette."),
                    );
                }
            }
        } else if stock_days > 90.0 {
            let kg = (self.storage[fi] - 60.0 * daily) * 0.1 * days.min(1.0);
            if kg > 1.0 {
                self.works.trade_flow -= kg;
                self.storage[fi] -= kg;
                let gain = kg * price * 0.7;
                self.treasury += gain;
                self.works.sold += gain;
            }
        }
    }

    pub(crate) fn add_food_with_mix(&mut self, fi: usize, kg: f64, prof: [f64; 3]) {
        let stock = self.storage[fi];
        for (m, p) in self.works.mix.iter_mut().zip(prof) {
            *m = (*m * stock + p * kg) / (stock + kg).max(1e-9);
        }
        self.storage[fi] += kg;
    }

    /// Nutrition of the people: the profile of the food in store, cooked or not, as far as there
    /// is enough of it.
    pub(crate) fn nutrition_tick(&mut self, food_sat: f64, cook_cover: f64, days: f64) {
        let m = self.works.mix;
        let e = (m[0] / 0.6).min(1.0);
        let p = (m[1] / 0.4).min(1.0);
        let v = (m[2] / 0.35).min(1.0);
        let score = (0.4 * e + 0.3 * p + 0.3 * v) * (0.9 + 0.1 * cook_cover);
        let target = (score * food_sat.clamp(0.0, 1.0)).clamp(0.0, 1.0);
        self.works.nutrition += (target - self.works.nutrition) * (1.0 - (-days / 10.0).exp());
        self.works.nutrition = self.works.nutrition.clamp(0.0, 1.0);
    }

    /// Record the history and everything else the tick leaves for the screens.
    pub(crate) fn works_close(&mut self, data: &GameData, now: f64) {
        let daily = (self.pop * 0.6).max(1.0);
        let food_days = data
            .resource_index("food")
            .map_or(0.0, |i| self.storage[i] / daily);
        if let Some(fi) = data.resource_index("food") {
            self.works.food_track = self.storage[fi];
        }
        self.record_history(now, food_days);
    }

    /// Blend food that appeared in the stores from elsewhere (deposits, recipes, trade) into the
    /// nutrition mix at the default profile.
    pub(crate) fn absorb_foreign_food(&mut self, data: &GameData) {
        let Some(fi) = data.resource_index("food") else {
            return;
        };
        let extra = self.storage[fi] - self.works.food_track;
        if extra > 0.5 && self.works.food_track >= 0.0 {
            let stock = self.works.food_track.max(0.0);
            for (m, p) in self.works.mix.iter_mut().zip(DEFAULT_MIX) {
                *m = (*m * stock + p * extra) / (stock + extra);
            }
        }
    }

    /// Receive the convoys that arrived. What does not fit in the stores waits one more day.
    pub(crate) fn receive_convoys(&mut self, data: &GameData, now: f64) {
        if self.works.siege > 0.8 || self.works.inbound.is_empty() {
            return;
        }
        let mut waiting = Vec::new();
        let list = std::mem::take(&mut self.works.inbound);
        for mut s in list {
            if s.due > now {
                waiting.push(s);
                continue;
            }
            let put = s.kg.min(self.free_space());
            if put > 0.0 {
                self.storage[s.res] += put;
                if data.resources[s.res].id == "food" {
                    // Food from another colony keeps the default profile.
                    let st = self.storage[s.res] - put;
                    for (m, p) in self.works.mix.iter_mut().zip(DEFAULT_MIX) {
                        *m = (*m * st + p * put) / (st + put).max(1e-9);
                    }
                }
                self.log_line(
                    now,
                    format!(
                        "Convoi arrive : {put:.0} kg de {}.",
                        data.resources[s.res].name
                    ),
                );
            }
            s.kg -= put;
            if s.kg > 1.0 {
                s.due = now + DAY;
                waiting.push(s);
            }
        }
        self.works.inbound = waiting;
    }

    /// Lifting capacity of the colony (kg per day): roads, depots and lifts that are running.
    pub fn lift_capacity(&self, data: &GameData) -> f64 {
        let mut c: f64 = self
            .active_defs(data)
            .map(|(_, b, d)| d.lift * b.health)
            .sum();
        if self.has_port(data) {
            c += 10_000.0;
        }
        c
    }

    /// Is a lift of the orbital kind running (a cable to orbit)?
    pub fn has_elevator(&self, data: &GameData) -> bool {
        self.active_defs(data).any(|(_, _, d)| d.lift >= 100_000.0)
    }
}

/// Short text of a bar of `n` cells for a value of 0..1 (charts).
pub fn spark(values: &[f32], lo: f32, hi: f32) -> String {
    const LEVELS: [char; 8] = ['_', '.', ':', '-', '=', '+', '*', '#'];
    values
        .iter()
        .map(|v| {
            let k = ((v - lo) / (hi - lo).max(1e-6)).clamp(0.0, 1.0);
            LEVELS[((k * 7.0).round() as usize).min(7)]
        })
        .collect()
}

impl crate::Sim {
    /// Capacities the research has unlocked (all of them when the technology rule is off).
    pub fn capacity_mask(&self) -> u32 {
        if !self.rules.tech_locks {
            return (1u32 << CAPACITIES.len()) - 1;
        }
        let mut m = 0;
        for (t, tech) in self.data.techs.iter().enumerate() {
            if self.researched.get(t).copied().unwrap_or(false) {
                for (k, id) in &tech.unlocks {
                    if k == "capacity" {
                        m |= cap_bit(id);
                    }
                }
            }
        }
        m
    }

    /// Work out the climate of the city of colony `ci` from the surface map of its world.
    pub(crate) fn colony_env_init(&mut self, ci: usize) {
        use vector_world::surface::{Surface, H, W};
        let pressure = self.colonies[ci].pressure;
        let mut env = Env {
            ready: true,
            air: pressure >= 0.2,
            ..Env::default()
        };
        if let Some((map, (cx, cy))) = self.colony_world(ci) {
            let i = Surface::idx(cx, cy);
            let temps: Vec<f64> = (0..8)
                .map(|k| {
                    vector_world::climate::seasonal_temp(&map.surface, cx, cy, f64::from(k) / 8.0)
                })
                .collect();
            let (lo, hi) = temps
                .iter()
                .fold((f64::MAX, f64::MIN), |a, t| (a.0.min(*t), a.1.max(*t)));
            let peak = temps
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map_or(0, |p| p.0);
            env.t_mean = temps.iter().sum::<f64>() / 8.0;
            env.t_amp = (hi - lo) / 2.0;
            env.t_peak = peak as f64 / 8.0;
            env.hum = f64::from(map.surface.hum[i]).clamp(0.0, 1.0);
            env.fertility = self
                .data
                .biomes
                .get(map.biome_at(cx, cy))
                .map_or(0.0, |b| b.fertility);
            env.coast = (0..=10usize).any(|dy| {
                (0..=10usize).any(|dx| {
                    let (x, y) = (
                        (cx + W + dx - 5) % W,
                        (cy + dy).saturating_sub(5).min(H - 1),
                    );
                    map.surface.is_sea(x, y)
                })
            });
            env.year = if self.colonies[ci].system_seed == self.system.seed {
                self.year_length(map.body)
            } else {
                3.15e7
            };
        }
        let c = &mut self.colonies[ci];
        let soil_max = (0.35 + 0.85 * env.fertility).clamp(0.3, 1.2);
        c.works.soil = soil_max * 0.9;
        c.works.env = env;
    }

    /// Weather, war and prices of the place, once per colony tick.
    pub(crate) fn apply_works_world(&mut self, ci: usize) {
        use crate::planet::Weather;
        let (star, now) = (self.colonies[ci].star, self.time);
        // Weather changes daily, war and prices are looked at every few days.
        let day = (now / DAY).floor() as i64;
        if self.colonies[ci].works.world_day == day {
            return;
        }
        let slow = day.rem_euclid(4) == i64::from(self.colonies[ci].id) % 4
            || self.colonies[ci].works.world_day == i64::MIN;
        let elapsed = if self.colonies[ci].works.world_day == i64::MIN {
            0.0
        } else {
            (day - self.colonies[ci].works.world_day).clamp(0, 400) as f64
        };
        self.colonies[ci].works.world_day = day;
        self.colony_relations_tick(ci, elapsed);
        let (mut rain, mut harsh) = (0.0, 0.0);
        if let Some((map, (cx, cy))) = self.colony_world(ci) {
            let (w, sev) = self.weather_at(&map, cx, cy);
            match w {
                Weather::Rain => rain = 0.6 + 0.4 * sev,
                Weather::Storm => {
                    rain = 1.0;
                    harsh = 0.5 * sev;
                }
                Weather::Heat | Weather::Dust => harsh = 0.4 + 0.6 * sev,
                Weather::Blizzard => harsh = 0.6 * sev,
                _ => {}
            }
        }
        if !slow {
            let c = &mut self.colonies[ci];
            c.works.rain = rain;
            c.works.harsh = harsh;
            return;
        }
        let pos = self
            .visited
            .get(&star)
            .copied()
            .or_else(|| self.galaxy.star(star).map(|s| s.pos))
            .unwrap_or(self.here_pos);
        let turmoil =
            crate::politics::turmoil(&self.data, &self.galaxy, self.galaxy.seed, pos, now);
        let food = self.data.resource_index("food");
        let factors = crate::bourse::world_factors(&self.data, self.galaxy.seed, pos, now);
        let c = &mut self.colonies[ci];
        c.works.rain = rain;
        c.works.harsh = harsh;
        c.works.siege = if turmoil.besieged { 1.0 } else { 0.0 };
        c.works.food_price = food.map_or(1.0, |f| factors[f]);
        c.works.harvest =
            crate::macroeco::harvest(self.galaxy.seed, crate::politics::cell_of(pos), now);
    }

    /// Send `kg` of resource `res` from colony `from` to colony `to` by convoy: roads on the same
    /// world, shuttles or an orbital lift in the same system. The goods leave now and arrive after
    /// the real delay. Returns the delay in days.
    pub fn colony_dispatch(
        &mut self,
        from: usize,
        to: usize,
        res: usize,
        kg: f64,
    ) -> Result<f64, &'static str> {
        if from == to || from >= self.colonies.len() || to >= self.colonies.len() {
            return Err("colonies invalides");
        }
        let data = std::sync::Arc::clone(&self.data);
        if res >= data.resources.len() || kg < 1.0 {
            return Err("quantite invalide");
        }
        let (a, b) = (&self.colonies[from], &self.colonies[to]);
        if a.system_seed != b.system_seed {
            return Err("autre systeme : utilisez la flotte deleguee (F3)");
        }
        if !a.has_cap("convoys") {
            return Err("convois : technologie du genie routier requise");
        }
        if a.works.siege > 0.5 || b.works.siege > 0.5 {
            return Err("les voies sont coupees par le siege");
        }
        let lift = a.lift_capacity(&data).max(2000.0);
        let max_load = lift * 3.0;
        if kg > max_load + 1e-6 {
            return Err("charge trop lourde pour les convois de cette colonie");
        }
        if a.storage[res] + 1e-6 < kg {
            return Err("stock insuffisant");
        }
        let same_body = a.body == b.body;
        let dist_au = if same_body {
            0.0
        } else {
            let pa = self.system_body_pos(a.system_seed, a.body);
            let pb = self.system_body_pos(b.system_seed, b.body);
            (pa - pb).length() / vector_world::orbit::AU
        };
        let elevator = a.has_elevator(&data) || b.has_elevator(&data);
        let days = convoy_delay(same_body, dist_au, kg, lift, elevator);
        let (id, now) = (a.id, self.time);
        let name = data.resources[res].name.clone();
        let dest = self.colonies[to].name.clone();
        self.colonies[from].storage[res] -= kg;
        self.colonies[from].log_line(
            now,
            format!("Convoi parti vers {dest} : {kg:.0} kg de {name} ({days:.1} j)."),
        );
        self.colonies[to].works.inbound.push(Shipment {
            res,
            kg,
            due: now + days * DAY,
            from: id,
        });
        Ok(days)
    }

    /// Where body `body` of the system with this seed is now, m (current system only; elsewhere
    /// the bodies are taken on their orbit at time zero, enough for a delay).
    fn system_body_pos(&self, seed: u64, body: usize) -> vector_core::Vec3 {
        if seed == self.system.seed {
            self.bodies
                .get(body)
                .map_or(vector_core::Vec3::default(), |b| b.pos)
        } else {
            vector_world::generate_system(seed)
                .bodies
                .get(body)
                .and_then(|b| {
                    b.orbit
                        .as_ref()
                        .map(|o| vector_core::Vec3::new(o.a, 0.0, 0.0))
                })
                .unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::tests::founded;
    use std::sync::Arc;

    const ALL: u32 = (1 << 16) - 1;

    fn put(c: &mut Colony, data: &GameData, id: &str, n: usize) {
        let def = data.building_index(id).expect(id);
        for _ in 0..n {
            c.buildings.push(BuildingInst {
                def,
                progress: 1.0,
                enabled: true,
                recipe: 0,
                health: 1.0,
                ops: Ops::default(),
            });
        }
    }

    fn stock(c: &mut Colony, data: &GameData, id: &str, kg: f64) {
        c.storage[data.resource_index(id).expect(id)] += kg;
    }

    fn held(c: &Colony, data: &GameData, id: &str) -> f64 {
        c.storage[data.resource_index(id).expect(id)]
    }

    fn env(fertility: f64, hum: f64, t_mean: f64, air: bool) -> Env {
        Env {
            ready: true,
            fertility,
            hum,
            t_mean,
            t_amp: 0.0,
            t_peak: 0.25,
            year: 3.0e7,
            coast: false,
            air,
        }
    }

    fn colony(caps: u32, e: Env) -> (Arc<GameData>, Colony) {
        let (s, ci) = founded();
        let mut c = s.colonies[ci].clone();
        c.caps = caps;
        c.works.env = e;
        c.works.soil = 0.7;
        c.pop = 100.0;
        c.treasury = 1.0e6;
        (Arc::clone(&s.data), c)
    }

    fn run(c: &mut Colony, data: &GameData, days: f64) {
        let mut left = days;
        while left > 0.0 {
            let step = left.min(40.0);
            let t = c.last_tick + step * DAY;
            c.update(data, t);
            left -= step;
        }
    }

    fn health_of(c: &Colony, data: &GameData, id: &str) -> f64 {
        c.buildings
            .iter()
            .find(|b| data.buildings[b.def].id == id)
            .expect(id)
            .health
    }

    #[test]
    fn unmaintained_buildings_wear_out_and_a_workshop_with_spare_parts_keeps_them() {
        let (data, mut a) = colony(0, Env::default());
        put(&mut a, &data, "smelter", 1);
        let mut b = a.clone();
        put(&mut b, &data, "workshop", 2);
        stock(&mut b, &data, "spare_parts", 5000.0);
        run(&mut a, &data, 300.0);
        run(&mut b, &data, 300.0);
        let (ha, hb) = (
            health_of(&a, &data, "smelter"),
            health_of(&b, &data, "smelter"),
        );
        assert!(ha < 0.7, "a smelter left alone wears out: {ha}");
        assert!(hb > ha + 0.25, "workshops and parts keep it: {hb} vs {ha}");
        assert!(
            held(&b, &data, "spare_parts") < 5000.0 - 10.0,
            "repairs use spare parts"
        );
        // Walls are mended with construction materials, not spare parts.
        let (data, mut c) = colony(0, Env::default());
        let dome_before = health_of(&c, &data, "habitat_dome");
        run(&mut c, &data, 400.0);
        let worn = health_of(&c, &data, "habitat_dome");
        assert!(worn < dome_before - 0.05, "{worn}");
        stock(&mut c, &data, "construction_materials", 5000.0);
        run(&mut c, &data, 200.0);
        assert!(
            health_of(&c, &data, "habitat_dome") > worn,
            "walls are mended"
        );
        assert!(held(&c, &data, "construction_materials") < 5000.0);
    }

    #[test]
    fn norms_metrology_and_tools_raise_quality_and_the_defects_go_to_the_waste_heap() {
        let (data, mut a) = colony(ALL, Env::default());
        put(&mut a, &data, "solar_farm", 6);
        put(&mut a, &data, "residential_block", 2);
        a.pop = 400.0;
        stock(&mut a, &data, "water", 100_000.0);
        stock(&mut a, &data, "food", 100_000.0);
        put(&mut a, &data, "smelter", 1);
        stock(&mut a, &data, "iron", 20_000.0);
        stock(&mut a, &data, "carbon", 2_000.0);
        let mut b = a.clone();
        put(&mut b, &data, "metrology_lab", 1);
        put(&mut b, &data, "standards_office", 1);
        stock(&mut b, &data, "tools", 500.0);
        // A colony without the technology gets less from the same offices.
        let mut raw = b.clone();
        raw.caps = 0;
        run(&mut a, &data, 20.0);
        run(&mut b, &data, 20.0);
        run(&mut raw, &data, 20.0);
        assert!(
            b.works.quality > a.works.quality + 0.15,
            "{} {}",
            b.works.quality,
            a.works.quality
        );
        assert!(
            b.works.quality > raw.works.quality,
            "norms need their technology"
        );
        assert!(b.works.defect < a.works.defect);
        let waste = |c: &Colony| held(c, &data, "waste") + c.works.litter;
        assert!(waste(&a) > waste(&b), "{} {}", waste(&a), waste(&b));
        assert!(
            held(&b, &data, "steel") > held(&a, &data, "steel"),
            "good tools and fewer defects: more steel"
        );
    }

    #[test]
    fn mass_is_conserved_when_the_defects_become_waste() {
        let (data, mut c) = colony(ALL, Env::default());
        put(&mut c, &data, "solar_farm", 6);
        put(&mut c, &data, "smelter", 1);
        stock(&mut c, &data, "iron", 5_000.0);
        stock(&mut c, &data, "carbon", 500.0);
        c.pop = 3.0;
        c.treasury = -1.0;
        c.works.litter = 0.0;
        let iron = held(&c, &data, "iron");
        run(&mut c, &data, 6.0);
        let used = iron - held(&c, &data, "iron");
        let steel = held(&c, &data, "steel");
        let waste = held(&c, &data, "waste") + c.works.litter;
        assert!(steel > 20.0, "the smelter ran: {steel}");
        // What the furnace gave is steel and the waste of its defects (and the town's few kilos).
        assert!(
            steel + waste <= used + 3.0 * 0.25 * 6.0 + 1e-6,
            "mass appeared: {used} iron -> {steel} steel + {waste} waste"
        );
        assert!(waste > 1.0, "defects left waste");
    }

    #[test]
    fn automation_replaces_workers_as_far_as_the_technology_allows() {
        let (data, mut a) = colony(ALL, Env::default());
        put(&mut a, &data, "smelter", 2);
        let mut b = a.clone();
        put(&mut b, &data, "automation_hub", 2);
        let mut c = b.clone();
        c.caps = 0;
        for x in [&mut a, &mut b, &mut c] {
            run(x, &data, 6.0);
        }
        assert!(b.works.automation > 0.45, "{}", b.works.automation);
        assert!(
            b.stats.jobs < a.stats.jobs,
            "{} vs {}",
            b.stats.jobs,
            a.stats.jobs
        );
        assert!(
            c.works.automation <= 0.2 + 1e-9,
            "no technology, little automation"
        );
        assert!(c.stats.jobs > b.stats.jobs);
        // Robots go further.
        let mut r = b.clone();
        put(&mut r, &data, "robot_cell", 2);
        run(&mut r, &data, 6.0);
        assert!(r.works.automation > b.works.automation);
    }

    #[test]
    fn the_bottleneck_of_a_chain_is_named() {
        let (data, mut c) = colony(ALL, Env::default());
        put(&mut c, &data, "smelter", 1);
        run(&mut c, &data, 6.0);
        let b = c
            .buildings
            .iter()
            .find(|b| data.buildings[b.def].id == "smelter")
            .expect("smelter");
        assert_eq!(b.ops.limit, LIMIT_INPUT, "{:?}", b.ops);
        let (res, n) = c.works.short_res.expect("a shortage");
        assert!(n >= 1 && ["iron", "carbon"].contains(&data.resources[res].id.as_str()));
        // Fed, it works and the inputs no longer hold it back.
        stock(&mut c, &data, "iron", 50_000.0);
        stock(&mut c, &data, "carbon", 5_000.0);
        stock(&mut c, &data, "tools", 500.0);
        run(&mut c, &data, 6.0);
        let b = c
            .buildings
            .iter()
            .find(|b| data.buildings[b.def].id == "smelter")
            .expect("smelter");
        assert_ne!(b.ops.limit, LIMIT_INPUT);
        assert!(b.ops.load > 0.1, "{:?}", b.ops);
        assert_eq!(limit_label(LIMIT_INPUT), "INTRANTS");
    }

    #[test]
    fn soil_climate_and_air_decide_what_the_fields_give() {
        let yield_of = |e: Env, building: &str| {
            let (data, mut c) = colony(ALL, e);
            put(&mut c, &data, building, 2);
            run(&mut c, &data, 6.0);
            c.works.farm.yield_day
        };
        let good = yield_of(env(0.8, 0.6, 288.0, true), "farmland");
        let desert = yield_of(env(0.05, 0.05, 300.0, true), "farmland");
        let frozen = yield_of(env(0.8, 0.6, 230.0, true), "farmland");
        let airless = yield_of(env(0.8, 0.6, 288.0, false), "farmland");
        let hall = yield_of(env(0.8, 0.6, 288.0, false), "greenhouse_hall");
        let hall_frozen = yield_of(env(0.8, 0.6, 230.0, true), "greenhouse_hall");
        assert!(good > 100.0, "{good}");
        assert!(desert < good * 0.5, "{desert} vs {good}");
        assert_eq!(frozen, 0.0, "nothing grows in the open at 230 K");
        assert_eq!(airless, 0.0, "no open fields without air");
        assert!(hall > 20.0, "a greenhouse grows without air: {hall}");
        assert!(hall_frozen > 0.0, "a greenhouse softens the frost");
    }

    #[test]
    fn link_harvest_to_fields_a_bad_year_of_the_sector_hits_the_open_fields_only() {
        let yield_of = |harvest: f64, building: &str| {
            let (data, mut c) = colony(ALL, env(0.8, 0.6, 288.0, true));
            c.works.harvest = harvest;
            put(&mut c, &data, building, 2);
            run(&mut c, &data, 6.0);
            c.works.farm.yield_day
        };
        let (normal, drought, bumper) = (
            yield_of(1.0, "farmland"),
            yield_of(0.6, "farmland"),
            yield_of(1.3, "farmland"),
        );
        assert!(normal > 100.0, "{normal}");
        assert!(
            (drought / normal - 0.6).abs() < 0.05,
            "{drought} vs {normal}"
        );
        assert!(bumper > normal * 1.2, "{bumper} vs {normal}");
        let (hall, hall_dry) = (
            yield_of(1.0, "greenhouse_hall"),
            yield_of(0.6, "greenhouse_hall"),
        );
        assert!(
            hall_dry / hall > 0.9 && hall_dry / hall > drought / normal + 0.25,
            "a greenhouse barely feels the drought: {hall_dry} vs {hall}"
        );
    }

    #[test]
    fn the_seasons_move_the_harvest() {
        let (data, mut c) = colony(ALL, env(0.8, 0.6, 288.0, true));
        c.works.env.t_amp = 22.0;
        c.works.env.t_peak = 0.25;
        put(&mut c, &data, "farmland", 2);
        // Midsummer, then midwinter.
        c.last_tick = 0.25 * c.works.env.year;
        run(&mut c, &data, 1.0);
        let summer = c.works.farm.yield_day;
        c.last_tick = 0.75 * c.works.env.year;
        c.works.planted = c.last_tick;
        run(&mut c, &data, 1.0);
        let winter = c.works.farm.yield_day;
        assert!(summer > winter + 20.0, "{summer} vs {winter}");
        let y = c.works.env.year;
        assert!(c.works.env.temp_at(0.25 * y) > c.works.env.temp_at(0.75 * y));
    }

    #[test]
    fn rotation_spares_the_soil_that_monoculture_exhausts() {
        let e = env(0.5, 0.6, 288.0, true);
        let soil_after = |caps: u32| {
            let (data, mut c) = colony(caps, e.clone());
            put(&mut c, &data, "farmland", 2);
            c.works.soil = 0.7;
            run(&mut c, &data, 480.0);
            c.works.soil
        };
        let rotated = soil_after(ALL);
        let mono = soil_after(0);
        assert!(rotated > mono + 0.2, "{rotated} vs {mono}");
        // The rotation really changes the crop from one season to the next.
        let (data, mut c) = colony(ALL, e);
        put(&mut c, &data, "farmland", 2);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..8 {
            run(&mut c, &data, 41.0);
            seen.insert(c.works.crop);
        }
        assert!(seen.len() >= 3, "{seen:?}");
    }

    #[test]
    fn irrigation_and_fertiliser_raise_the_yield_and_feed_the_soil() {
        let dry = env(0.6, 0.15, 288.0, true);
        let (data, mut a) = colony(ALL, dry);
        put(&mut a, &data, "farmland", 2);
        let mut b = a.clone();
        put(&mut b, &data, "irrigation_net", 2);
        stock(&mut b, &data, "water", 50_000.0);
        run(&mut a, &data, 6.0);
        run(&mut b, &data, 6.0);
        assert!(
            b.works.farm.yield_day > a.works.farm.yield_day * 1.5,
            "{} {}",
            b.works.farm.yield_day,
            a.works.farm.yield_day
        );
        assert!(b.works.farm.irrigated > 0.9);
        assert!(held(&b, &data, "water") < 50_000.0, "the pumps use water");
        // Without the technology the network is a heap of pipes.
        let mut raw = b.clone();
        raw.caps = 0;
        run(&mut raw, &data, 6.0);
        assert!(raw.works.farm.irrigated == 0.0);
        // Fertiliser keeps the soil up.
        let (data, mut poor) = colony(ALL, env(0.6, 0.6, 288.0, true));
        put(&mut poor, &data, "farmland", 2);
        let mut rich = poor.clone();
        stock(&mut rich, &data, "fertilizer", 5_000.0);
        run(&mut poor, &data, 200.0);
        run(&mut rich, &data, 200.0);
        assert!(
            rich.works.soil > poor.works.soil + 0.1,
            "{} {}",
            rich.works.soil,
            poor.works.soil
        );
        assert!(held(&rich, &data, "fertilizer") < 5_000.0);
    }

    #[test]
    fn food_spoils_less_in_a_cold_store_and_the_spoilage_is_waste() {
        let (data, mut a) = colony(ALL, env(0.5, 0.5, 300.0, true));
        a.pop = 0.0;
        stock(&mut a, &data, "food", 20_000.0);
        let mut b = a.clone();
        put(&mut b, &data, "cold_store", 1);
        stock(&mut b, &data, "preserves", 100.0);
        run(&mut a, &data, 10.0);
        run(&mut b, &data, 10.0);
        assert!(a.works.spoiled > 20.0, "{}", a.works.spoiled);
        assert!(
            b.works.spoiled < a.works.spoiled * 0.5,
            "{} vs {}",
            b.works.spoiled,
            a.works.spoiled
        );
        assert!(
            held(&a, &data, "waste") + a.works.litter > held(&b, &data, "waste") + b.works.litter
        );
        assert!(
            held(&b, &data, "preserves") > 99.0,
            "preserves do not spoil"
        );
    }

    #[test]
    fn preserves_feed_the_people_when_the_rations_run_out() {
        let (data, mut a) = colony(ALL, Env::default());
        stock(&mut a, &data, "water", 20_000.0);
        stock(&mut a, &data, "consumer_goods", 500.0);
        let mut b = a.clone();
        stock(&mut b, &data, "preserves", 5_000.0);
        a.caps = 0;
        b.caps = 0;
        run(&mut a, &data, 6.0);
        run(&mut b, &data, 6.0);
        assert!(a.stats.food_sat < 0.1, "{}", a.stats.food_sat);
        assert!(b.stats.food_sat > 0.95, "{}", b.stats.food_sat);
    }

    #[test]
    fn nutrition_follows_the_variety_of_what_is_grown_and_the_kitchens() {
        let (data, mut a) = colony(ALL, Env::default());
        stock(&mut a, &data, "food", 30_000.0);
        stock(&mut a, &data, "water", 20_000.0);
        stock(&mut a, &data, "consumer_goods", 500.0);
        let mut b = a.clone();
        let mut c = a.clone();
        a.works.mix = [0.9, 0.1, 0.05]; // grain only
        b.works.mix = [0.6, 0.45, 0.45]; // a varied table
        c.works.mix = a.works.mix;
        put(&mut c, &data, "canteen", 1);
        for x in [&mut a, &mut b, &mut c] {
            x.works.food_track = 30_000.0;
            run(x, &data, 80.0);
        }
        assert!(
            a.works.nutrition < b.works.nutrition - 0.2,
            "{} {}",
            a.works.nutrition,
            b.works.nutrition
        );
        assert!(c.works.nutrition > a.works.nutrition, "kitchens help");
        assert!(a.stats.food_sat > 0.99, "fed, but badly");
        // A harvest changes the mix of the stock to what was grown.
        let (data, mut f) = colony(ALL, env(0.8, 0.6, 288.0, true));
        put(&mut f, &data, "farmland", 2);
        f.works.mix = [0.9, 0.1, 0.05];
        run(&mut f, &data, 30.0);
        let crop = &data.crops[f.works.crop];
        assert!(
            f.works.mix[2] != 0.05,
            "{:?} {:?}",
            f.works.mix,
            crop.nutrition
        );
    }

    #[test]
    fn waste_is_collected_or_left_to_foul_the_streets_and_compost_closes_the_cycle() {
        let (data, mut a) = colony(ALL, Env::default());
        let mut b = a.clone();
        put(&mut b, &data, "sewage_plant", 1);
        let mut c = b.clone();
        put(&mut c, &data, "composter", 1);
        for x in [&mut a, &mut b, &mut c] {
            stock(x, &data, "water", 50_000.0);
            stock(x, &data, "food", 20_000.0);
            run(x, &data, 60.0);
        }
        assert!(
            a.works.litter > b.works.litter + 300.0,
            "{} {}",
            a.works.litter,
            b.works.litter
        );
        assert!(
            a.works.pollution > b.works.pollution,
            "{} {}",
            a.works.pollution,
            b.works.pollution
        );
        assert!(b.works.sanit_cover > a.works.sanit_cover);
        assert!(held(&b, &data, "waste") > 100.0, "the heap fills");
        assert!(
            held(&c, &data, "waste") < held(&b, &data, "waste"),
            "compost eats the heap"
        );
        assert!(
            held(&c, &data, "fertilizer") > 10.0,
            "and gives fertiliser back"
        );
        // The landfill is bounded: the rest stays in the streets.
        let (data, mut d) = colony(ALL, Env::default());
        d.dump_waste(&data, 1.0e6, 1.0);
        assert!(held(&d, &data, "waste") <= d.stats.storage_cap.max(5000.0) * 0.25 + 1.0);
        assert!(d.works.litter > 1.0e5);
    }

    #[test]
    fn scrubbers_clean_the_air_that_industry_fouls_and_the_mood_follows() {
        let (data, mut a) = colony(ALL, Env::default());
        put(&mut a, &data, "smelter", 4);
        put(&mut a, &data, "chemical_plant", 3);
        for id in [
            "iron",
            "carbon",
            "tools",
            "ammonia",
            "phosphorus",
            "food",
            "water",
        ] {
            stock(&mut a, &data, id, 100_000.0);
        }
        let mut b = a.clone();
        put(&mut b, &data, "air_scrubber", 4);
        run(&mut a, &data, 120.0);
        run(&mut b, &data, 120.0);
        assert!(a.works.pollution > 0.15, "{}", a.works.pollution);
        assert!(
            b.works.pollution < a.works.pollution * 0.6,
            "{} {}",
            b.works.pollution,
            a.works.pollution
        );
    }

    #[test]
    fn crime_follows_unemployment_and_the_police_hold_it_down() {
        let (data, mut a) = colony(ALL, Env::default());
        a.pop = 100.0;
        let mut b = a.clone();
        put(&mut b, &data, "smelter", 3);
        let mut c = a.clone();
        put(&mut c, &data, "police_station", 3);
        for x in [&mut a, &mut b, &mut c] {
            stock(x, &data, "food", 30_000.0);
            stock(x, &data, "water", 30_000.0);
            stock(x, &data, "consumer_goods", 1_000.0);
            run(x, &data, 90.0);
        }
        assert!(
            a.works.unemployment > b.works.unemployment + 0.2,
            "{} {}",
            a.works.unemployment,
            b.works.unemployment
        );
        assert!(
            a.works.crime > b.works.crime,
            "{} {}",
            a.works.crime,
            b.works.crime
        );
        assert!(
            a.works.crime > c.works.crime,
            "{} {}",
            a.works.crime,
            c.works.crime
        );
        // Crime empties the tills.
        let (data, mut d) = colony(ALL, Env::default());
        d.works.crime = 1.0;
        stock(&mut d, &data, "food", 30_000.0);
        stock(&mut d, &data, "water", 30_000.0);
        run(&mut d, &data, 40.0);
        assert!(
            d.log.iter().any(|(_, l)| l.contains("braquage")),
            "a heist was told"
        );
    }

    #[test]
    fn transit_gets_the_workers_to_the_workplaces_of_a_big_city() {
        let (data, mut a) = colony(ALL, Env::default());
        put(&mut a, &data, "residential_block", 12);
        put(&mut a, &data, "warehouse", 12);
        put(&mut a, &data, "smelter", 4);
        a.pop = 800.0;
        let mut b = a.clone();
        put(&mut b, &data, "metro_line", 1);
        put(&mut b, &data, "tram_line", 1);
        for x in [&mut a, &mut b] {
            stock(x, &data, "food", 200_000.0);
            stock(x, &data, "water", 200_000.0);
            run(x, &data, 6.0);
        }
        assert!(a.works.area_ha > 20.0, "{}", a.works.area_ha);
        assert!(a.works.mobility < 0.9, "{}", a.works.mobility);
        assert!(
            b.works.mobility > a.works.mobility,
            "{} {}",
            b.works.mobility,
            a.works.mobility
        );
    }

    #[test]
    fn dense_districts_are_measured_and_planning_tolerates_more() {
        let (data, mut a) = colony(0, Env::default());
        a.buildings.clear();
        put(&mut a, &data, "arcology", 1);
        a.pop = 1500.0;
        stock(&mut a, &data, "food", 500_000.0);
        stock(&mut a, &data, "water", 500_000.0);
        let mut b = a.clone();
        b.caps = ALL;
        run(&mut a, &data, 6.0);
        run(&mut b, &data, 6.0);
        assert!(a.works.density > DENSE, "{}", a.works.density);
        // The same density is below the threshold of a planned city.
        assert!(a.works.density < DENSE * 1.6 || b.works.density < DENSE * 1.6);
        assert!(a.works.area_ha > 1.0);
    }

    #[test]
    fn convoys_take_real_time_depend_on_the_lift_and_stop_under_siege() {
        // The delay law.
        let road = convoy_delay(true, 0.0, 10_000.0, 6_000.0, false);
        let shuttle = convoy_delay(false, 1.0, 10_000.0, 10_000.0, false);
        let lift = convoy_delay(false, 1.0, 10_000.0, 400_000.0, true);
        assert!(road < shuttle, "{road} {shuttle}");
        assert!(lift < shuttle * 0.8, "{lift} {shuttle}");
        assert!(
            convoy_delay(true, 0.0, 60_000.0, 6_000.0, false) > road,
            "a bigger load takes longer"
        );
        // In a colony pair.
        let (mut s, ci) = founded();
        s.rules.tech_locks = false;
        let mut other = s.colonies[ci].clone();
        other.id += 100;
        other.name = "Autre".into();
        other.body = if other.body == 1 { 2 } else { 1 };
        other.storage.iter_mut().for_each(|v| *v = 0.0);
        s.colonies.push(other);
        let to = s.colonies.len() - 1;
        let data = Arc::clone(&s.data);
        let food = data.resource_index("food").expect("food");
        s.colonies[ci].caps = ALL;
        s.colonies[ci].storage[food] = 8000.0;
        let days = s.colony_dispatch(ci, to, food, 3000.0).expect("dispatch");
        assert!(days > 0.5, "{days}");
        assert!(
            (s.colonies[ci].storage[food] - 5000.0).abs() < 1e-6,
            "the goods left"
        );
        s.colonies[to].treasury = -1.0; // no imports to blur the picture
        assert!(s.colonies[to].storage[food] < 1.0, "and are not there yet");
        s.time += (days - 0.2) * DAY;
        s.colonies[to].works.siege = 0.0; // the sector may be at war: not this test's business
        s.update_colonies();
        assert!(s.colonies[to].storage[food] < 1.0, "still on the road");
        s.time += 1.0 * DAY;
        s.colonies[to].works.siege = 0.0;
        s.update_colonies();
        assert!(
            s.colonies[to].storage[food] > 1500.0,
            "arrived: {}",
            s.colonies[to].storage[food]
        );
        assert!(s.colonies[to].works.inbound.is_empty());
        // Errors: too much, the same colony, another system.
        assert!(s.colony_dispatch(ci, to, food, 1.0e7).is_err());
        assert!(s.colony_dispatch(ci, ci, food, 10.0).is_err());
        s.colonies[to].system_seed ^= 1;
        assert!(
            s.colony_dispatch(ci, to, food, 10.0).is_err(),
            "another system goes by fleet"
        );
    }

    #[test]
    fn a_siege_stops_the_convoys_and_shells_the_town_unless_turrets_cover_it() {
        let (data, mut a) = colony(ALL, Env::default());
        let food = data.resource_index("food").expect("food");
        a.works.inbound.push(Shipment {
            res: food,
            kg: 500.0,
            due: a.last_tick,
            from: 99,
        });
        a.works.siege = 1.0;
        let before = a.storage[food];
        run(&mut a, &data, 3.0);
        assert!(
            a.works.inbound.len() == 1 && a.storage[food] <= before + 1.0,
            "nothing gets through"
        );
        a.works.siege = 0.0;
        run(&mut a, &data, 2.0);
        assert!(a.works.inbound.is_empty(), "the road reopens");
        // Shells.
        let (data, mut besieged) = colony(ALL, Env::default());
        put(&mut besieged, &data, "habitat_dome", 6);
        let mut guarded = besieged.clone();
        put(&mut guarded, &data, "defense_turret", 4);
        let mut calm = besieged.clone();
        besieged.works.siege = 1.0;
        guarded.works.siege = 1.0;
        for x in [&mut besieged, &mut guarded, &mut calm] {
            run(x, &data, 80.0);
        }
        let mean = |c: &Colony| {
            c.buildings.iter().map(|b| b.health).sum::<f64>() / c.buildings.len() as f64
        };
        assert!(
            mean(&besieged) < mean(&calm) - 0.03,
            "{} {}",
            mean(&besieged),
            mean(&calm)
        );
        assert!(mean(&guarded) > mean(&besieged), "turrets cover the town");
    }

    #[test]
    fn trade_buys_food_against_famine_and_sells_a_glut_at_the_local_price() {
        let (data, mut a) = colony(ALL, Env::default());
        stock(&mut a, &data, "water", 50_000.0);
        stock(&mut a, &data, "consumer_goods", 500.0);
        let mut raw = a.clone();
        raw.caps = 0;
        run(&mut a, &data, 12.0);
        run(&mut raw, &data, 12.0);
        assert!(a.works.bought > 0.0, "the colony imported food");
        assert!(
            a.stats.food_sat > raw.stats.food_sat,
            "{} {}",
            a.stats.food_sat,
            raw.stats.food_sat
        );
        // Dear food costs more.
        let (data, mut cheap) = colony(ALL, Env::default());
        stock(&mut cheap, &data, "water", 50_000.0);
        stock(&mut cheap, &data, "consumer_goods", 500.0);
        let mut dear = cheap.clone();
        dear.works.food_price = 3.0;
        run(&mut cheap, &data, 12.0);
        run(&mut dear, &data, 12.0);
        assert!(
            dear.works.bought > cheap.works.bought,
            "{} {}",
            dear.works.bought,
            cheap.works.bought
        );
        // A glut is sold.
        let (data, mut g) = colony(ALL, Env::default());
        stock(&mut g, &data, "food", 200_000.0);
        stock(&mut g, &data, "water", 50_000.0);
        stock(&mut g, &data, "consumer_goods", 500.0);
        run(&mut g, &data, 10.0);
        assert!(g.works.sold > 0.0, "the surplus was sold");
        // Not under siege.
        let (data, mut s) = colony(ALL, Env::default());
        s.works.siege = 1.0;
        stock(&mut s, &data, "water", 50_000.0);
        run(&mut s, &data, 12.0);
        assert!(s.works.bought == 0.0, "the lanes are cut");
    }

    #[test]
    fn a_technology_waits_for_its_discovery_and_capacities_follow_research() {
        let (mut s, _) = founded();
        s.rules.tech_locks = true;
        let data = Arc::clone(&s.data);
        let t = data.tech_index("precision_tooling").expect("tech");
        for (i, tech) in data.techs.iter().enumerate() {
            s.researched[i] = tech.id != "precision_tooling"
                && tech.id != "orbital_lift_engineering"
                && tech.id != "agronomy"
                && tech.id != "industrial_robotics";
        }
        assert!(!s.tech_available(t), "tungsten is missing");
        assert_eq!(s.missing_findings(t), vec!["resource:tungsten".to_string()]);
        assert!(s.start_research(t).is_err());
        let w = data.resource_index("tungsten").expect("tungsten");
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        s.ship.cargo.add(&data, w, 5.0);
        s.tick_research(1.0);
        assert!(s.findings.contains("resource:tungsten"));
        assert!(s.tech_available(t) && s.start_research(t).is_ok());
        // An anomaly and a resource both needed.
        let o = data.tech_index("orbital_lift_engineering").expect("tech");
        assert!(s.missing_findings(o).len() == 2);
        s.findings.insert("anomaly:gravity_well".into());
        assert_eq!(
            s.missing_findings(o),
            vec!["resource:nanomaterials".to_string()]
        );
        // Capacities come with the research.
        assert_eq!(s.capacity_mask() & cap_bit("crop_rotation"), 0);
        let a = data.tech_index("agronomy").expect("tech");
        s.researched[a] = true;
        assert_ne!(s.capacity_mask() & cap_bit("crop_rotation"), 0);
        s.rules.tech_locks = false;
        assert_eq!(s.capacity_mask(), ALL);
        // Studying a ruin's tool is a finding too.
        let r = data.tech_index("industrial_robotics").expect("tech");
        assert_eq!(s.missing_findings(r), vec!["artefact:outil".to_string()]);
    }

    #[test]
    fn wrecks_and_colony_studies_are_findings() {
        let (mut s, ci) = founded();
        let data = Arc::clone(&s.data);
        let re = data.tech_index("reverse_engineering").expect("tech");
        assert_eq!(s.missing_findings(re), vec!["wreck:salvaged".to_string()]);
        s.wrecks.push(crate::combat::Wreck {
            id: 1,
            name: "TEST".into(),
            pos: s.ship.pos,
            vel: s.ship.vel,
            cargo: vec![(0, 10.0)],
            born: 0.0,
        });
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        s.salvage().expect("salvage");
        assert!(s.missing_findings(re).is_empty());
        let up = data.tech_index("urban_planning").expect("tech");
        assert!(s.missing_findings(up).contains(&"study:colony".to_string()));
        s.colonies[ci].life.discoveries = 1;
        s.tick_research(1.0);
        assert!(s.findings.contains("study:colony"));
    }

    #[test]
    fn the_charts_keep_a_daily_history() {
        let (data, mut c) = colony(ALL, Env::default());
        stock(&mut c, &data, "food", 5_000.0);
        run(&mut c, &data, 30.0);
        assert!(
            c.works.hist.len() >= 25 && c.works.hist.len() <= HIST_LEN,
            "{}",
            c.works.hist.len()
        );
        run(&mut c, &data, 120.0);
        assert!(c.works.hist.len() <= HIST_LEN);
        let line = spark(&[0.0, 0.5, 1.0], 0.0, 1.0);
        assert_eq!(line.chars().count(), 3);
        assert!(line.starts_with('_') && line.ends_with('#'));
    }

    #[test]
    fn crops_are_domestic_producers_of_the_species_table_with_enough_families() {
        let data = founded().0.data.clone();
        assert!(data.crops.len() >= 10);
        for (i, c) in data.crops.iter().enumerate() {
            let sp = &data.species[data.species_index(&c.species).expect("species")];
            assert!(sp.is_producer() && sp.domestic);
            let w = crop_window(&data, i);
            assert!(w.0 < w.1, "{}", c.species);
            assert!(crop_yield(&data, i) > 3.0, "{}", c.species);
        }
        // The window shapes the growth.
        let w = (280.0, 300.0);
        assert!(temp_factor(w, 290.0) > temp_factor(w, 282.0));
        assert!(temp_factor(w, 282.0) > temp_factor(w, 276.0));
        assert_eq!(temp_factor(w, 260.0), 0.0);
        assert_eq!(temp_factor(w, 320.0), 0.0);
        // Sea crops need a coast.
        let algae = data
            .crops
            .iter()
            .position(|c| c.species == "algues_marines")
            .expect("algae");
        assert!(sea_crop(&data, algae));
    }
}
