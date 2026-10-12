//! The works of a colony, four tabs of the colony screen: industry (I), farming and food (G),
//! the city (V) and logistics (L). They show what `vector_sim::colony_works` computes: what holds
//! each building back, wear and quality, the fields and the diet, density, sanitation, pollution
//! and crime, the convoys on the road. Charts are drawn with characters.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use crate::ui::bar;
use vector_render::Framebuffer;
use vector_sim::colony::Colony;
use vector_sim::colony_works::{limit_label, spark, HIST_LEN, LIMIT_NONE, LIMIT_OFF};
use winit::keyboard::KeyCode;

pub const TABS: [(&str, KeyCode, &str); 4] = [
    ("INDUSTRIE", KeyCode::KeyI, "I"),
    ("AGRICULTURE", KeyCode::KeyG, "G"),
    ("VILLE", KeyCode::KeyV, "V"),
    ("LOGISTIQUE", KeyCode::KeyL, "L"),
];

/// Row of the first line of a tab.
const TOP: usize = 5;

fn pct(v: f64) -> String {
    format!("{:>3.0}%", v * 100.0)
}

fn level(v: f64, good_high: bool) -> u32 {
    let v = if good_high { 1.0 - v } else { v };
    if v < 0.3 {
        GREEN
    } else if v < 0.6 {
        AMBER
    } else {
        RED
    }
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let Some(c) = g.sim.colonies.get(g.col_idx) else {
        return;
    };
    // The tab bar: clickable words.
    let mut x = 1;
    for (i, (name, key, k)) in TABS.iter().enumerate() {
        let label = format!("[{k}] {name}");
        let on = usize::from(g.col_tab) == i + 1;
        g.hit_cells(x, TOP - 1, label.len(), 1, Act::Key(*key), None);
        fb.text(x, TOP - 1, &label, if on { AMBER } else { GREEN });
        x += label.len() + 2;
    }
    fb.text(x, TOP - 1, "(MEME TOUCHE OU ECHAP : RETOUR)", DIM);
    match g.col_tab {
        1 => industry(fb, g, c),
        2 => farming(fb, g, c),
        3 => city(fb, g, c),
        _ => logistics(fb, g, c),
    }
}

fn history(
    fb: &mut Framebuffer,
    row: usize,
    label: &str,
    c: &Colony,
    idx: usize,
    range: (f32, f32),
    color: u32,
) {
    let h: Vec<f32> = c.works.hist.iter().map(|s| s[idx]).collect();
    let pad = HIST_LEN - h.len().min(HIST_LEN);
    fb.text(
        1,
        row,
        &format!(
            "{label:<22} {}{}",
            " ".repeat(pad),
            spark(&h, range.0, range.1)
        ),
        color,
    );
}

fn industry(fb: &mut Framebuffer, g: &Game, c: &Colony) {
    let w = &c.works;
    let mut row = TOP + 1;
    fb.text(
        1,
        row,
        &format!(
            "QUALITE {} {}  DEFAUTS {}  OUTILLAGE {}  ENTRETIEN {}  AUTOMATISATION {}  EMPLOIS {:.0}/{:.0}  PRODUCTIVITE {}",
            bar(w.quality, 8),
            pct(w.quality),
            pct(w.defect),
            pct(w.tool_cover),
            pct(w.maint_cover),
            pct(w.automation),
            c.stats.jobs,
            c.stats.workers,
            pct(c.stats.prod_mult)
        ),
        GREEN,
    );
    row += 1;
    let short = w.short_res.map_or("AUCUN".to_string(), |(r, n)| {
        format!(
            "{} ({n} BATIMENTS A L'ARRET)",
            g.data.resources[r].name.to_uppercase()
        )
    });
    fb.text(
        1,
        row,
        &format!(
            "GOULOT D'APPROVISIONNEMENT : {short}   NORMES {}  METROLOGIE {}  ROBOTIQUE {}",
            on_off(c.has_cap("norms")),
            on_off(c.has_cap("precision_tooling")),
            on_off(c.has_cap("robotics"))
        ),
        if w.short_res.is_some() { RED } else { DIM },
    );
    row += 2;
    fb.text(
        1,
        row,
        "  BATIMENT                    ETAT  CHARGE      USURE  LIMITE      RECETTE",
        DIM,
    );
    row += 1;
    let rows = 26usize;
    let n = c.buildings.len();
    let start = g
        .tab_sel
        .saturating_sub(rows / 2)
        .min(n.saturating_sub(rows));
    for k in start..(start + rows).min(n) {
        let b = &c.buildings[k];
        let d = &g.data.buildings[b.def];
        let sel = k == g.tab_sel;
        let state = if !b.built() {
            format!("{:>3.0}%", b.progress * 100.0)
        } else if b.enabled {
            " ON ".to_string()
        } else {
            "OFF".to_string()
        };
        let recipe = if d.recipes.is_empty() {
            String::new()
        } else {
            g.data
                .recipe_index(&d.recipes[b.recipe % d.recipes.len()])
                .map_or(String::new(), |i| g.data.recipes[i].name.clone())
        };
        let limit = if b.built() {
            limit_label(b.ops.limit)
        } else {
            "-"
        };
        let col = if sel {
            AMBER
        } else if !b.built() || b.ops.limit == LIMIT_OFF {
            DIM
        } else if b.ops.limit == LIMIT_NONE {
            GREEN
        } else {
            RED
        };
        g.hit_cells(0, row, 90, 1, Act::Call(Game::click_tab_row, k), None);
        fb.text(
            1,
            row,
            &format!(
                "{}{:<26.26} {:>4} {} {:>4.0}%  {:<10} {:<24.24}",
                if sel { ">" } else { " " },
                d.name,
                state,
                bar(f64::from(b.ops.load), 6),
                (1.0 - b.health) * 100.0,
                limit,
                recipe
            ),
            col,
        );
        row += 1;
    }
    row = fb.rows().saturating_sub(10);
    fb.text(1, row, "HISTORIQUE (60 JOURS)", AMBER);
    history(fb, row + 1, "PRODUCTIVITE", c, 5, (0.0, 1.2), GREEN);
    history(fb, row + 2, "ENERGIE SERVIE", c, 4, (0.0, 1.0), GREEN);
    fb.text(
        1,
        fb.rows() - 4,
        "HAUT/BAS CHOIX  ESPACE ON/OFF  R RECETTE  LIMITES : INTRANTS = STOCK MANQUANT, ENERGIE, PERSONNEL, USURE, OUTILLAGE",
        DIM,
    );
}

fn on_off(b: bool) -> &'static str {
    if b {
        "OUI"
    } else {
        "NON"
    }
}

fn farming(fb: &mut Framebuffer, g: &Game, c: &Colony) {
    let w = &c.works;
    let data = &g.data;
    let now = g.sim.time;
    let t = w.env.temp_at(now);
    let mut row = TOP + 1;
    let crop_name = data.crops.get(w.crop).map_or("AUCUNE".to_string(), |cr| {
        let sp = data
            .species_index(&cr.species)
            .map_or("?", |i| data.species[i].name.as_str());
        format!("{} ({})", sp.to_uppercase(), cr.family.to_uppercase())
    });
    fb.text(
        1,
        row,
        &format!(
            "SOL {} {}  FERTILITE DU BIOME {}  HUMIDITE {}  TEMPERATURE {:.0} K ({:+.0} C)  AIR LIBRE {}  COTE {}",
            bar((w.soil / 1.2).min(1.0), 8),
            pct(w.soil),
            pct(w.env.fertility),
            pct(w.env.hum),
            t,
            t - 273.15,
            on_off(w.env.air),
            on_off(w.env.coast)
        ),
        GREEN,
    );
    row += 1;
    fb.text(
        1,
        row,
        &format!(
            "CULTURE : {crop_name}  SEMEE IL Y A {:.0} J  ROTATION {} (TECHNOLOGIE {})   [C] CHANGER DE CULTURE   [R] ROTATION",
            ((now - w.planted) / 86_400.0).max(0.0),
            on_off(w.rotate),
            on_off(c.has_cap("crop_rotation")),
        ),
        AMBER,
    );
    g.hit_cells(1, row, 120, 1, Act::Key(KeyCode::KeyC), None);
    row += 2;
    let f = &w.farm;
    fb.text(
        1,
        row,
        &format!(
            "CHAMPS {:.0} HA  RENDEMENT {:.0} KG/J   CLIMAT {}  EAU {}  SOL {}  LUMIERE {}  IRRIGATION {}  ENGRAIS {}",
            f.ha,
            f.yield_day,
            pct(f.temp_f),
            pct(f.water_f),
            pct(f.soil_f),
            pct(f.light),
            pct(f.irrigated),
            pct(f.fert_ratio)
        ),
        if f.ha > 0.0 { GREEN } else { DIM },
    );
    row += 1;
    let fi = data.resource_index("food");
    let pi = data.resource_index("preserves");
    let food = fi.map_or(0.0, |i| c.storage[i]);
    let daily = (c.pop * 0.6).max(1.0);
    let days = food / daily;
    let state = if c.stats.food_sat < 0.6 {
        ("FAMINE", RED)
    } else if c.stats.food_sat < 0.95 {
        ("DISETTE", AMBER)
    } else if days > 90.0 {
        ("SURPLUS", AMBER)
    } else {
        ("SUFFISANT", GREEN)
    };
    fb.text(
        1,
        row,
        &format!(
            "VIVRES {:.0} KG = {:.0} JOURS  CONSERVES {:.0} KG  PERTE PAR AVARIE {:.1} KG/J  VIVRES SERVIS {}   {}",
            food,
            days,
            pi.map_or(0.0, |i| c.storage[i]),
            w.spoiled,
            pct(c.stats.food_sat),
            state.0
        ),
        state.1,
    );
    row += 2;
    fb.text(1, row, "NUTRITION DU PEUPLE", AMBER);
    row += 1;
    for (i, name) in ["ENERGIE", "PROTEINES", "VITAMINES"].iter().enumerate() {
        let need = [0.6, 0.4, 0.35][i];
        fb.text(
            1,
            row,
            &format!(
                "{name:<10} {} {}",
                bar((w.mix[i] / need).min(1.0), 12),
                pct((w.mix[i] / need).min(1.0))
            ),
            level((w.mix[i] / need).min(1.0), true),
        );
        row += 1;
    }
    fb.text(
        1,
        row,
        &format!(
            "SCORE {} {}  (CUISINES : MEILLEURE ASSIMILATION)",
            bar(w.nutrition, 12),
            pct(w.nutrition)
        ),
        level(w.nutrition, true),
    );
    row += 2;
    fb.text(
        1,
        row,
        "CULTURE                          FAMILLE   CLIMAT NOW  KG/HA/J   EAU   NUTRITION E/P/V",
        DIM,
    );
    row += 1;
    for (i, cr) in data.crops.iter().enumerate() {
        let sp = data
            .species_index(&cr.species)
            .map_or("?", |k| data.species[k].name.as_str());
        let ft = vector_sim::colony_works::temp_factor(
            vector_sim::colony_works::crop_window(data, i),
            t,
        );
        let cur = i == w.crop;
        fb.text(
            1,
            row,
            &format!(
                "{}{:<30.30} {:<9.9} {:>8}   {:>6.1}  {:>4.1}   {:.1}/{:.1}/{:.1}",
                if cur { ">" } else { " " },
                sp.to_uppercase(),
                cr.family.to_uppercase(),
                pct(ft),
                vector_sim::colony_works::crop_yield(data, i),
                cr.water,
                cr.nutrition.0,
                cr.nutrition.1,
                cr.nutrition.2
            ),
            if cur {
                AMBER
            } else if ft > 0.3 {
                GREEN
            } else {
                DIM
            },
        );
        row += 1;
    }
    let base = fb.rows().saturating_sub(10);
    fb.text(1, base, "HISTORIQUE (60 JOURS)", AMBER);
    history(fb, base + 1, "JOURS DE VIVRES", c, 0, (0.0, 120.0), GREEN);
    history(fb, base + 2, "NUTRITION", c, 3, (0.0, 1.0), GREEN);
}

fn city(fb: &mut Framebuffer, g: &Game, c: &Colony) {
    let w = &c.works;
    let data = &g.data;
    let mut row = TOP + 1;
    fb.text(
        1,
        row,
        &format!(
            "POPULATION {:.0}  SURFACE BATIE {:.0} HA  DENSITE {:.0} HAB/HA {}  CHOMAGE {}  MOBILITE {}",
            c.pop,
            w.area_ha,
            w.density,
            if w.density > 250.0 { "(TROP DENSE)" } else { "" },
            pct(w.unemployment),
            pct(w.mobility)
        ),
        GREEN,
    );
    row += 2;
    fb.text(1, row, "QUARTIERS", AMBER);
    row += 1;
    fb.text(
        1,
        row,
        "QUARTIER             BATIMENTS  EMPLOIS  PUISSANCE KW  SURFACE HA",
        DIM,
    );
    row += 1;
    let groups: [(&str, &[&str]); 7] = [
        ("LOGEMENT", &["housing"]),
        ("INDUSTRIE", &["industry", "extraction"]),
        ("AGRICOLE", &["agriculture"]),
        ("SERVICES", &["service", "science", "defense", "terraform"]),
        ("ENERGIE", &["power"]),
        (
            "INFRASTRUCTURE",
            &["infrastructure", "spaceport", "storage"],
        ),
        ("ASSAINISSEMENT", &["urbanism"]),
    ];
    for (name, cats) in groups {
        let (mut n, mut jobs, mut kw, mut ha) = (0, 0.0, 0.0, 0.0);
        for b in c.buildings.iter().filter(|b| b.built()) {
            let d = &data.buildings[b.def];
            if cats.contains(&d.category.as_str()) {
                n += 1;
                jobs += d.workers;
                kw += d.power;
                ha += vector_sim::colony_works::footprint(d);
            }
        }
        fb.text(
            1,
            row,
            &format!("{name:<20} {n:>9}  {jobs:>7.0}  {kw:>12.0}  {ha:>10.1}"),
            if n > 0 { GREEN } else { DIM },
        );
        row += 1;
    }
    row += 1;
    let waste = data.resource_index("waste").map_or(0.0, |i| c.storage[i]);
    let lines: [(&str, f64, bool, String); 4] = [
        (
            "ASSAINISSEMENT",
            w.sanit_cover,
            true,
            format!(
                "{} DES DECHETS COLLECTES  DECHARGE {waste:.0} KG",
                pct(w.sanit_cover)
            ),
        ),
        (
            "POLLUTION",
            w.pollution,
            false,
            format!(
                "{}  DECHETS DANS LES RUES {:.0} KG",
                pct(w.pollution),
                w.litter
            ),
        ),
        (
            "CRIMINALITE",
            w.crime,
            false,
            format!(
                "{}  (CHOMAGE, DENSITE, POLLUTION, PAUVRETE ; POLICE ET ORDRE LA FREINENT)",
                pct(w.crime)
            ),
        ),
        (
            "SIEGE",
            w.siege,
            false,
            if w.siege > 0.0 {
                "VOIES COUPEES : PAS DE CONVOIS NI D'IMPORTS".to_string()
            } else {
                "AUCUN".to_string()
            },
        ),
    ];
    for (name, v, good_high, text) in lines {
        fb.text(
            1,
            row,
            &format!("{name:<15} {} {text}", bar(v, 12)),
            level(v, good_high),
        );
        row += 1;
    }
    row += 1;
    fb.text(
        1,
        row,
        &format!(
            "TECHNOLOGIES : ASSAINISSEMENT {}  RECYCLAGE {}  TRANSPORTS DE MASSE {}  URBANISME {}  EPURATION DE L'AIR {}",
            on_off(c.has_cap("sanitation")),
            on_off(c.has_cap("recycling")),
            on_off(c.has_cap("mass_transit")),
            on_off(c.has_cap("district_planning")),
            on_off(c.has_cap("air_scrubbing"))
        ),
        DIM,
    );
    let base = fb.rows().saturating_sub(10);
    fb.text(1, base, "HISTORIQUE (60 JOURS)", AMBER);
    history(fb, base + 1, "POLLUTION", c, 1, (0.0, 1.0), AMBER);
    history(fb, base + 2, "CRIMINALITE", c, 2, (0.0, 1.0), RED);
}

fn logistics(fb: &mut Framebuffer, g: &Game, c: &Colony) {
    let data = &g.data;
    let mut row = TOP + 1;
    let lift = c.lift_capacity(data);
    fb.text(
        1,
        row,
        &format!(
            "CAPACITE DE CONVOI {:.0} KG/J  ASTROPORT {}  ASCENSEUR ORBITAL {}  CONVOIS (TECHNOLOGIE) {}  PRIX DES VIVRES x{:.2}  IMPORTS {:.0} CR  EXPORTS {:.0} CR",
            lift,
            on_off(c.has_port(data)),
            on_off(c.has_elevator(data)),
            on_off(c.has_cap("convoys")),
            c.works.food_price,
            c.works.bought,
            c.works.sold
        ),
        GREEN,
    );
    row += 2;
    fb.text(1, row, "CONVOIS EN ROUTE VERS CETTE COLONIE", AMBER);
    row += 1;
    if c.works.inbound.is_empty() {
        fb.text(3, row, "AUCUN", DIM);
        row += 1;
    }
    for s in c.works.inbound.iter().take(8) {
        let from = g
            .sim
            .colonies
            .iter()
            .find(|o| o.id == s.from)
            .map_or("?".to_string(), |o| o.name.to_uppercase());
        fb.text(
            3,
            row,
            &format!(
                "{:>7.0} KG DE {:<24.24} DE {:<20.20} ARRIVEE DANS {:>5.1} J",
                s.kg,
                data.resources[s.res].name.to_uppercase(),
                from,
                ((s.due - g.sim.time) / 86_400.0).max(0.0)
            ),
            GREEN,
        );
        row += 1;
    }
    // Convoys that left this colony and are still on the road.
    let outbound: Vec<(String, &vector_sim::colony_works::Shipment)> = g
        .sim
        .colonies
        .iter()
        .flat_map(|o| {
            o.works
                .inbound
                .iter()
                .filter(|s| s.from == c.id)
                .map(move |s| (o.name.to_uppercase(), s))
        })
        .collect();
    row += 1;
    fb.text(1, row, "CONVOIS PARTIS DE CETTE COLONIE", AMBER);
    row += 1;
    if outbound.is_empty() {
        fb.text(3, row, "AUCUN", DIM);
        row += 1;
    }
    for (to, s) in outbound.iter().take(8) {
        fb.text(
            3,
            row,
            &format!(
                "{:>7.0} KG DE {:<24.24} VERS {:<20.20} ARRIVEE DANS {:>5.1} J",
                s.kg,
                data.resources[s.res].name.to_uppercase(),
                to,
                ((s.due - g.sim.time) / 86_400.0).max(0.0)
            ),
            GREEN,
        );
        row += 1;
    }
    row += 1;
    fb.text(
        1,
        row,
        "ENVOYER UN CONVOI  (GAUCHE/DROITE : DESTINATION, HAUT/BAS : MARCHANDISE)",
        AMBER,
    );
    row += 1;
    let dests: Vec<usize> = g
        .sim
        .colonies
        .iter()
        .enumerate()
        .filter(|(i, o)| *i != g.col_idx && o.system_seed == c.system_seed)
        .map(|(i, _)| i)
        .collect();
    let dest = dests.get(g.ship_to % dests.len().max(1)).copied();
    let items: Vec<usize> = (0..data.resources.len())
        .filter(|&i| c.storage[i] > 0.5)
        .collect();
    let res = items.get(g.ship_res % items.len().max(1)).copied();
    match (dest, res) {
        (Some(d), Some(r)) => {
            let to = &g.sim.colonies[d];
            let same = to.body == c.body;
            fb.text(
                3,
                row,
                &format!(
                    "VERS {} ({})   MARCHANDISE : {} ({:.0} KG EN STOCK)",
                    to.name.to_uppercase(),
                    if same {
                        "MEME MONDE, PAR ROUTE"
                    } else {
                        "AUTRE MONDE, PAR NAVETTE"
                    },
                    data.resources[r].name.to_uppercase(),
                    c.storage[r]
                ),
                GREEN,
            );
            row += 1;
            let mut x = 3;
            for (i, kg) in [500.0, 2000.0, 10_000.0].iter().enumerate() {
                let label = format!("[ENVOYER {kg:.0} KG]");
                g.hit_cells(x, row, label.len(), 1, Act::Call(Game::click_ship, i), None);
                fb.text(x, row, &label, AMBER);
                x += label.len() + 2;
            }
            let label = "[TOUT ENVOYER]";
            g.hit_cells(x, row, label.len(), 1, Act::Call(Game::click_ship, 3), None);
            fb.text(x, row, label, AMBER);
            row += 1;
            // The delay a load would take.
            let kg = c.storage[r].min(2000.0);
            let au = if same { 0.0 } else { 1.0 };
            let days = vector_sim::colony_works::convoy_delay(
                same,
                au,
                kg,
                lift.max(2000.0),
                c.has_elevator(data) || to.has_elevator(data),
            );
            fb.text(
                3,
                row,
                &format!("DELAI ESTIME POUR {kg:.0} KG : {days:.1} JOURS"),
                DIM,
            );
        }
        (None, _) => {
            fb.text(3, row, "AUCUNE AUTRE COLONIE DANS CE SYSTEME : LA FLOTTE DELEGUEE (F3) ASSURE LES LIAISONS LOINTAINES", DIM);
        }
        _ => {
            fb.text(3, row, "RIEN EN STOCK A ENVOYER", DIM);
        }
    }
}
