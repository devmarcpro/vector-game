//! Validates the data files and checks them against the acceptance thresholds.
use vector_data::{find_data_dir, GameData};

fn main() {
    let Some(dir) = find_data_dir() else {
        eprintln!("data/ directory not found");
        std::process::exit(2);
    };
    let d = match GameData::load(&dir) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("INVALID DATA: {e:?}");
            std::process::exit(1);
        }
    };
    let variants: usize = d.texts.iter().map(|(_, v)| v.len()).sum();
    let weapons = d.modules.iter().filter(|m| !m.weapon.is_empty()).count();
    let rows: Vec<(&str, usize, usize)> = vec![
        ("resources", d.resources.len(), 40),
        ("recipes", d.recipes.len(), 100),
        ("modules (non-weapon)", d.modules.len() - weapons, 60),
        ("weapons", weapons, 20),
        ("hulls", d.hulls.len(), 15),
        ("buildings", d.buildings.len(), 25),
        ("technologies", d.techs.len(), 60),
        ("factions", d.factions.len(), 6),
        ("skills + traits", d.skills.len() + d.traits.len(), 40),
        ("diseases", d.diseases.len(), 30),
        ("crew events", d.events.len(), 150),
        ("text templates", variants, 300),
        ("planet types", vector_world::PlanetKind::ALL.len(), 12),
        ("star classes", vector_world::StarClass::ALL.len(), 8),
        ("anomalies", d.anomalies_def.len(), 10),
    ];
    let mut ok = true;
    for (name, have, need) in rows {
        let good = have >= need;
        ok &= good;
        println!(
            "{:<22} {:>5} / {:<5} {}",
            name,
            have,
            need,
            if good { "ok" } else { "MISSING" }
        );
    }
    let orphan_recipes: Vec<&str> = d
        .recipes
        .iter()
        .filter(|r| r.kind != "extract" && !d.buildings.iter().any(|b| b.recipes.contains(&r.id)))
        .map(|r| r.id.as_str())
        .collect();
    if !orphan_recipes.is_empty() {
        println!("recipes no building can run: {}", orphan_recipes.join(", "));
        ok = false;
    }
    println!("{}", if ok { "DATA OK" } else { "DATA INCOMPLETE" });
    std::process::exit(i32::from(!ok));
}
