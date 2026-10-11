//! Economy sanity report: value added by recipes, module price outliers, resource value range.
use vector_data::{find_data_dir, GameData};

fn main() {
    let dir = find_data_dir().expect("data/ not found");
    let d = GameData::load(&dir).expect("data");
    let value = |id: &str| d.resource_index(id).map_or(0.0, |i| d.resources[i].value);
    let mut bad = 0;
    println!("== recipes: output value / (input value + energy) ==");
    let mut ratios = Vec::new();
    // Two thresholds. Value added per batch is only a problem if one building can farm it: what
    // caps an exploit is throughput, so the profit per building-day is what gets flagged.
    // Recycling loses value on purpose (disposal, not trade) and is listed apart.
    const MAX_PROFIT_PER_DAY: f64 = 20_000.0;
    const MIN_RATIO: f64 = 0.5;
    let mut recycling = Vec::new();
    for r in d.recipes.iter().filter(|r| r.kind == "process") {
        let vin: f64 = r.inputs.iter().map(|(i, k)| value(i) * k).sum();
        let vout: f64 = r.outputs.iter().map(|(i, k)| value(i) * k).sum();
        let energy = r.energy_kwh * 0.1;
        let ratio = vout / (vin + energy).max(1e-9);
        let per_day = (vout - vin - energy) / (r.hours / 24.0).max(1e-9);
        ratios.push((ratio, r.id.as_str()));
        if r.id.starts_with("recycle_") {
            recycling.push((ratio, r.id.as_str()));
        } else if ratio < MIN_RATIO {
            bad += 1;
            println!(
                "  LOSS     {:<28} in {vin:>9.1} out {vout:>9.1} energy {energy:>7.1} ratio {ratio:.2}",
                r.id
            );
        } else if ratio > 6.0 && per_day > MAX_PROFIT_PER_DAY {
            bad += 1;
            println!(
                "  EXPLOIT? {:<28} ratio {ratio:.2}, {per_day:.0} cr per building-day",
                r.id
            );
        }
    }
    for (ratio, id) in &recycling {
        println!("  recycling (loss by design) {id:<24} ratio {ratio:.2}");
    }    ratios.sort_by(|a, b| a.0.total_cmp(&b.0));
    if let (Some(lo), Some(hi)) = (ratios.first(), ratios.last()) {
        println!("  range {:.2} ({}) .. {:.2} ({})", lo.0, lo.1, hi.0, hi.1);
    }
    println!("== modules: price per kg outliers ==");
    let mean: f64 = d
        .modules
        .iter()
        .map(|m| m.cost / m.mass.max(1.0))
        .sum::<f64>()
        / d.modules.len() as f64;
    for m in &d.modules {
        let ppk = m.cost / m.mass.max(1.0);
        if ppk > mean * 12.0 || m.cost <= 0.0 {
            bad += 1;
            println!(
                "  OUTLIER {:<24} {:>9.0} cr {:>8.0} kg ({ppk:.1} cr/kg, mean {mean:.1})",
                m.id, m.cost, m.mass
            );
        }
    }
    println!("== resources: value range ==");
    let (mut lo, mut hi) = (("", f64::MAX), ("", 0.0f64));
    for r in &d.resources {
        if r.value < lo.1 {
            lo = (r.id.as_str(), r.value);
        }
        if r.value > hi.1 {
            hi = (r.id.as_str(), r.value);
        }
        if r.value <= 0.0 {
            bad += 1;
            println!("  FREE {}", r.id);
        }
    }
    println!(
        "  cheapest {} ({:.3} cr/kg), dearest {} ({:.1} cr/kg)",
        lo.0, lo.1, hi.0, hi.1
    );
    println!("== hulls ==");
    for h in &d.hulls {
        println!(
            "  {:<22} {:>10.0} cr {:>9.0} kg ({} slots)",
            h.id,
            h.cost,
            h.mass,
            h.slots.len()
        );
    }
    println!("{bad} balance warning(s)");
}
