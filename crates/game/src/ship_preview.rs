//! Colours of the parts of a plan in the designer's grid and lists. The 3D preview of a plan is no
//! longer drawn here: the designer shows the real model of the ship (`ship_model::blueprint_view`),
//! the same as the outside view (audit B5).

/// Colour of a part by its kind (the data says which kind it is).
pub fn part_color(kind: &str) -> u32 {
    match kind {
        "cockpit" => 0x00E0_A020,
        "hull" => 0x0070_A0C0,
        "deck" => 0x0040_C060,
        "hold" => 0x00B0_8040,
        "bay" => 0x0090_60C0,
        "engine" => 0x00D0_5040,
        "nacelle" => 0x00E0_7030,
        "wing" => 0x0050_C0D0,
        "gear" => 0x00C0_C0C0,
        _ => 0x0080_80A0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_of_part_has_its_own_colour() {
        let kinds = [
            "cockpit", "hull", "deck", "hold", "bay", "engine", "nacelle", "wing", "gear",
        ];
        let mut seen = std::collections::HashSet::new();
        for k in kinds {
            assert!(seen.insert(part_color(k)), "{k}");
        }
        assert!(!seen.contains(&part_color("unknown")));
    }
}
