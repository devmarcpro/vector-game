//! Codex entries of the geology of worlds and of astronomy. The text comes from
//! `data/geology.ron` and `data/cosmos.ron`; every number is drawn from the generators of
//! `vector_world` themselves (a sample world, a census of stars, the constants of the rules), so
//! the encyclopedia cannot drift from the game.
use std::sync::OnceLock;
use vector_data::GameData;
use vector_world::cosmos::{self, FeatureKind};
use vector_world::geology::{Boundary, DepositKind, GeoParams, Rock, VolcanoKind};
use vector_world::orbit::{AU, G, M_SUN};
use vector_world::physics::Activity;
use vector_world::stellar::{self, StarStage};
use vector_world::surface::{generate_with, Surface, H, W};
use vector_world::{peek_star, PlanetKind, StarHeader};

/// Metres per unit of relative elevation (same scale as the surface screen).
const RELIEF_M: f64 = 8000.0;

fn params(age: f64) -> GeoParams {
    GeoParams {
        activity: Activity::Plates,
        age_gyr: age,
        pressure: 1.0,
    }
}

/// The world the figures are quoted on: a temperate world with plates, as old as the Earth.
fn sample() -> &'static Surface {
    static S: OnceLock<Surface> = OnceLock::new();
    S.get_or_init(|| generate_with(1, PlanetKind::Terran, 285.0, &params(4.5)).expect("solid"))
}

/// Mean steepness of the land of a world of the given age (sum of |dh| between neighbours).
fn roughness(age: f64) -> f64 {
    let s = generate_with(1, PlanetKind::Terran, 285.0, &params(age)).expect("solid");
    let mut sum = 0.0;
    for y in 0..H {
        for x in 0..W - 1 {
            let (a, b) = (s.elev[Surface::idx(x, y)], s.elev[Surface::idx(x + 1, y)]);
            if a > 0.0 && b > 0.0 {
                sum += f64::from((a - b).abs());
            }
        }
    }
    sum * RELIEF_M / (W * H) as f64
}

fn census() -> &'static Vec<StarHeader> {
    static C: OnceLock<Vec<StarHeader>> = OnceLock::new();
    C.get_or_init(|| {
        (0..6000u64)
            .map(|s| peek_star(s.wrapping_mul(0x9E37_79B9)))
            .collect()
    })
}

fn pct(n: usize, of: usize) -> f64 {
    100.0 * n as f64 / of.max(1) as f64
}

pub fn geology_entry(data: &GameData, i: usize) -> Vec<String> {
    let g = &data.geology[i];
    let mut out = vec![format!("{}  [{}]", g.name, g.group), g.text.clone()];
    let s = sample();
    let cells = W * H;
    let land = s.elev.iter().filter(|&&e| e >= 0.0).count();
    match g.group.as_str() {
        "limite" => {
            if let Some(b) = Boundary::ALL.iter().find(|b| b.id() == g.id) {
                let on: Vec<usize> = (0..cells)
                    .filter(|&k| s.geo.boundary_at(k % W, k / W) == Some(*b))
                    .collect();
                let mean = on.iter().map(|&k| f64::from(s.elev[k])).sum::<f64>()
                    / on.len().max(1) as f64;
                out.push(format!(
                    "Sur un monde type ({} plaques) : {:.1} % de la surface, altitude moyenne {:.0} m",
                    s.geo.plates.len(),
                    pct(on.len(), cells),
                    mean * RELIEF_M
                ));
            }
        }
        "roche" => {
            if let Some(r) = Rock::ALL.iter().find(|r| r.id() == g.id) {
                let n = (0..cells)
                    .filter(|&k| s.elev[k] >= 0.0 && s.geo.rock_at(k % W, k / W) == *r)
                    .count();
                out.push(format!(
                    "Sur un monde type : {:.0} % des terres émergées",
                    pct(n, land)
                ));
            }
        }
        "gisement" => {
            let Some(kind) = DepositKind::from_id(&g.id) else {
                return out;
            };
            let parts: Vec<String> = g
                .yields
                .iter()
                .map(|(r, share)| {
                    format!(
                        "{} {:.0} %",
                        data.resource_index(r)
                            .map_or(r.as_str(), |k| data.resources[k].name.as_str()),
                        share * 100.0
                    )
                })
                .collect();
            out.push(format!("Contient : {}", parts.join(", ")));
            let n = s.geo.deposits.iter().filter(|d| d.kind == kind).count();
            out.push(format!(
                "Sur un monde type : {n} gisements, richesse moyenne {:.0} %. Fonder une colonie à moins de {:.0} cases en enrichit le sol.",
                100.0
                    * s.geo
                        .deposits
                        .iter()
                        .filter(|d| d.kind == kind)
                        .map(|d| f64::from(d.richness))
                        .sum::<f64>()
                    / n.max(1) as f64,
                vector_sim::geo::SITE_REACH_CELLS
            ));
        }
        "volcan" => {
            if let Some(k) = VolcanoKind::ALL.iter().find(|k| k.id() == g.id) {
                let (act, ext) = (
                    s.geo.volcanoes.iter().filter(|v| v.kind == *k && v.active).count(),
                    s.geo.volcanoes.iter().filter(|v| v.kind == *k && !v.active).count(),
                );
                out.push(format!(
                    "Sur un monde type : {act} actifs, {ext} éteints. Chaque éruption du monde frappe un volcan actif."
                ));
            }
        }
        "erosion" => match g.id.as_str() {
            "hydraulic" => out.push(format!(
                "Pente moyenne des terres : {:.0} m par case à 0,1 milliard d'années, {:.0} m à 4,5 milliards, {:.0} m à 9 milliards.",
                roughness(0.1),
                roughness(4.5),
                roughness(9.0)
            )),
            "wind" => out.push(format!(
                "Plein effet à partir de {:.2} bar de pression ; nul sur un monde sans air.",
                vector_world::geology::WIND_MIN_BAR
            )),
            _ => out.push(format!(
                "Agit au-delà de {:.0} degrés de latitude.",
                vector_world::geology::ICE_LATITUDE.to_degrees()
            )),
        },
        _ => {
            let (hi, lo) = (
                s.elev.iter().copied().fold(f32::MIN, f32::max),
                s.elev.iter().copied().fold(f32::MAX, f32::min),
            );
            out.push(format!(
                "Sur un monde type : plus haut sommet {:.0} m, fosse la plus profonde {:.0} m.",
                f64::from(hi) * RELIEF_M,
                -f64::from(lo) * RELIEF_M
            ));
        }
    }
    out
}

pub fn cosmos_entry(data: &GameData, i: usize) -> Vec<String> {
    let c = &data.cosmos[i];
    let mut out = vec![format!("{}  [{}]", c.name, c.group), c.text.clone()];
    let stars = census();
    if let Some(stage) = StarStage::ALL.iter().find(|s| s.id() == c.id) {
        let of: Vec<&StarHeader> = stars.iter().filter(|h| h.stage == *stage).collect();
        let n = of.len().max(1) as f64;
        out.push(format!(
            "{:.2} % des étoiles de la galaxie. Typique : {:.2} masses solaires, {:.3} rayon solaire, {:.0} K.",
            pct(of.len(), stars.len()),
            of.iter().map(|h| h.mass_sun).sum::<f64>() / n,
            of.iter().map(|h| h.radius_sun).sum::<f64>() / n,
            of.iter().map(|h| h.teff).sum::<f64>() / n
        ));
        return out;
    }
    if let Some(kind) = FeatureKind::ALL.iter().find(|k| k.id() == c.id) {
        let (r0, r1) = kind.radius_range();
        out.push(format!(
            "Rayon {r0:.0} à {r1:.0} al. Au cœur : dose {:.1} Gy/h (décroît comme le carré de la distance au cœur), capteurs réduits de {:.0} %, intérêt pour la recherche +{:.0} %.",
            kind.radiation(),
            kind.opacity() * 100.0,
            kind.interest() * 100.0
        ));
        return out;
    }
    if let Some(a) = data.asteroids.iter().find(|a| a.id == c.link) {
        let mut comp = a.composition.clone();
        comp.sort_by(|x, y| y.1.total_cmp(&x.1));
        let parts: Vec<String> = comp
            .iter()
            .take(6)
            .map(|(r, f)| {
                format!(
                    "{} {:.1} %",
                    data.resource_index(r)
                        .map_or(r.as_str(), |k| data.resources[k].name.as_str()),
                    f * 100.0
                )
            })
            .collect();
        out.push(format!(
            "Densité {:.0} kg/m3. Composition : {}",
            a.density,
            parts.join(", ")
        ));
        return out;
    }
    match c.id.as_str() {
        "pulsar" => out.push(format!(
            "Période {:.3} à {:.1} s. Dose à 1 UA : {:.2} Gy/h, à 0,1 UA : {:.0} Gy/h.",
            stellar::PULSAR_PERIOD_S.0,
            stellar::PULSAR_PERIOD_S.1,
            stellar::PULSAR_DOSE_AU,
            stellar::PULSAR_DOSE_AU * 100.0
        )),
        "magnetar" => out.push(format!(
            "Dose à 1 UA : {:.1} Gy/h (x{:.0} un pulsar ordinaire).",
            stellar::PULSAR_DOSE_AU * stellar::MAGNETAR_FACTOR,
            stellar::MAGNETAR_FACTOR
        )),
        "variable_star" => out.push(format!(
            "Géante rouge : {:.0} à {:.0} jours, +/- {:.0} à {:.0} % de lumière. Supergéante : {:.0} à {:.0} jours, +/- {:.0} à {:.0} %.",
            stellar::MIRA_PERIOD_DAYS.0,
            stellar::MIRA_PERIOD_DAYS.1,
            stellar::MIRA_AMPLITUDE.0 * 100.0,
            stellar::MIRA_AMPLITUDE.1 * 100.0,
            stellar::CEPHEID_PERIOD_DAYS.0,
            stellar::CEPHEID_PERIOD_DAYS.1,
            stellar::CEPHEID_AMPLITUDE.0 * 100.0,
            stellar::CEPHEID_AMPLITUDE.1 * 100.0
        )),
        "binary_star" => out.push(format!(
            "Le compagnon orbite à au moins {:.0} UA et {:.0} fois la dernière planète.",
            stellar::COMPANION_MIN_AU,
            stellar::COMPANION_SEPARATION_FACTOR
        )),
        "gravitational_lens" => {
            let gm = 10.0 * M_SUN * G;
            let focus = 1.0e10 * gm / (299_792_458.0f64).powi(2);
            out.push(format!(
                "Pour un trou noir de 10 masses solaires : foyer à {:.0} UA, capteurs x3 au foyer (x1 loin de là).",
                focus / AU
            ));
        }
        "comet_tail" => out.push(format!(
            "Excentricité {:.2} à {:.2}. Inactive au-delà de {:.0} UA (fois la racine de la luminosité).",
            stellar::COMET_ECC.0,
            stellar::COMET_ECC.1,
            stellar::COMET_REACH_AU
        )),
        "ring" => out.push(format!(
            "De {:.2} rayons de la planète à sa limite de Roche pour la glace : {:.2} R (rho_planète / {:.0})^(1/3). Dix blocs de glace minables par anneau.",
            stellar::RING_INNER_RADII,
            stellar::ROCHE_FACTOR,
            stellar::ICE_DENSITY
        )),
        "trojans" => out.push(format!(
            "A {:.0} degrés devant et derrière la géante : six roches de type D à chaque point.",
            stellar::TROJAN_ANGLE.to_degrees()
        )),
        "supermassive_black_hole" => out.push(format!(
            "{:.1e} masses solaires, à {:.0} al de chez vous. Dose à 1 000 al : {:.0} Gy/h, nulle au-delà de 3 000 al.",
            cosmos::CENTRE_MASS_SUN,
            vector_world::galaxy::Galaxy::centre_distance([0.0; 3]),
            cosmos::CENTRE_DOSE_1000LY
        )),
        "quasar" => {
            let q = vector_world::galaxy::Galaxy::new(1).quasars();
            out.push(format!(
                "{} sur le ciel profond, décalage vers le rouge de {:.1} à {:.1}.",
                q.len(),
                q.iter().map(|x| x.redshift).fold(f64::MAX, f64::min),
                q.iter().map(|x| x.redshift).fold(0.0, f64::max)
            ));
        }
        "gravitational_waves" => out.push(format!(
            "Un événement tous les {:.0} jours en moyenne dans la galaxie ; entendu au-dessus d'une déformation de {:.0e}.",
            1.0 / cosmos::GW_RATE_PER_DAY,
            cosmos::GW_THRESHOLD
        )),
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use vector_world::AstType;

    fn data() -> GameData {
        GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
            .expect("data")
    }

    #[test]
    fn every_kind_the_code_knows_has_an_entry_and_every_entry_has_numbers() {
        let d = data();
        for id in Boundary::ALL.map(Boundary::id) {
            assert!(d.geo_def(id).is_some_and(|g| g.group == "limite"), "{id}");
        }
        for id in Rock::ALL.map(Rock::id) {
            assert!(d.geo_def(id).is_some_and(|g| g.group == "roche"), "{id}");
        }
        for id in VolcanoKind::ALL.map(VolcanoKind::id) {
            assert!(d.geo_def(id).is_some_and(|g| g.group == "volcan"), "{id}");
        }
        for k in DepositKind::ALL {
            let g = d.geo_def(k.id()).expect("deposit entry");
            assert!(g.group == "gisement" && !g.yields.is_empty(), "{}", k.id());
        }
        for id in StarStage::ALL.map(StarStage::id) {
            assert!(
                d.cosmos_def(id).is_some_and(|c| c.group == "etoile"),
                "{id}"
            );
        }
        for k in FeatureKind::ALL {
            assert!(
                d.cosmos_def(k.id()).is_some_and(|c| c.group == "milieu"),
                "{}",
                k.id()
            );
        }
        for a in AstType::ALL {
            let c = d
                .cosmos_def(&format!("asteroid_{}", a.id().to_lowercase()))
                .expect("asteroid");
            assert_eq!(c.link, a.id());
            assert!(
                d.asteroids.iter().any(|x| x.id == a.id()),
                "{} has a composition",
                a.id()
            );
        }
        // The real generators feed every line: each entry quotes at least one number.
        for i in 0..d.geology.len() {
            let e = geology_entry(&d, i);
            assert!(e.len() >= 3, "{}: {e:?}", d.geology[i].id);
            assert!(e
                .iter()
                .skip(2)
                .any(|l| l.chars().any(|c| c.is_ascii_digit())));
        }
        for i in 0..d.cosmos.len() {
            let e = cosmos_entry(&d, i);
            assert!(e.len() >= 3, "{}: {e:?}", d.cosmos[i].id);
            assert!(e
                .iter()
                .skip(2)
                .any(|l| l.chars().any(|c| c.is_ascii_digit())));
        }
    }

    #[test]
    fn the_figures_come_from_the_generators_not_from_the_text() {
        let d = data();
        // The entry for convergent boundaries quotes the share of the sample map they cover.
        let k = d
            .geology
            .iter()
            .position(|g| g.id == "convergent")
            .expect("entry");
        let line = geology_entry(&d, k).remove(2);
        let s = sample();
        let on = (0..W * H)
            .filter(|&i| s.geo.boundary_at(i % W, i / W) == Some(Boundary::Convergent))
            .count();
        assert!(line.contains(&format!("{:.1} %", pct(on, W * H))), "{line}");
        // The pulsar entry quotes the dose constant of the rules.
        let p = d
            .cosmos
            .iter()
            .position(|c| c.id == "pulsar")
            .expect("entry");
        let line = cosmos_entry(&d, p).remove(2);
        assert!(
            line.contains(&format!("{:.2} Gy/h", stellar::PULSAR_DOSE_AU))
                && line.contains(&format!("{:.0} Gy/h", stellar::PULSAR_DOSE_AU * 100.0)),
            "{line}"
        );
    }
}
