//! Reading formats of the views: one style for a duration, an amount, a price and a mass
//! (audit B10). Capitals, a space between the figure and the unit.

/// A duration in seconds: S, MIN, H, J (days) or ANS.
pub fn duration(s: f64) -> String {
    let a = s.abs();
    let (v, unit) = if a < 60.0 {
        (s, "S")
    } else if a < 3_600.0 {
        (s / 60.0, "MIN")
    } else if a < 86_400.0 {
        (s / 3_600.0, "H")
    } else if a < 400.0 * 86_400.0 {
        (s / 86_400.0, "J")
    } else {
        (s / vector_core::units::YEAR, "ANS")
    };
    if v.abs() >= 100.0 || unit == "S" {
        format!("{v:.0} {unit}")
    } else {
        format!("{v:.1} {unit}")
    }
}

/// A span of days.
pub fn days(d: f64) -> String {
    duration(d * 86_400.0)
}

/// An amount of money: plain under 10 000, then K and M.
pub fn amount(v: f64) -> String {
    if v.abs() >= 1.0e6 {
        format!("{:.2}M", v / 1.0e6)
    } else if v.abs() >= 1.0e4 {
        format!("{:.1}K", v / 1.0e3)
    } else {
        format!("{v:.0}")
    }
}

/// An amount of credits.
pub fn credits(v: f64) -> String {
    format!("{} CR", amount(v))
}

/// A unit price: more decimals when it is small.
pub fn price(p: f64) -> String {
    if p >= 1000.0 {
        format!("{p:.0}")
    } else if p >= 10.0 {
        format!("{p:.1}")
    } else {
        format!("{p:.2}")
    }
}

/// A mass in kg as a short string (kg, t, kt, Mt, Gt).
pub fn mass(kg: f64) -> String {
    if kg < 1.0e3 {
        format!("{kg:.0} kg")
    } else if kg < 1.0e6 {
        format!("{:.0} t", kg / 1.0e3)
    } else if kg < 1.0e9 {
        format!("{:.0} kt", kg / 1.0e6)
    } else if kg < 1.0e12 {
        format!("{:.0} Mt", kg / 1.0e9)
    } else {
        format!("{:.0} Gt", kg / 1.0e12)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_duration_style_for_every_screen() {
        assert_eq!(duration(45.0), "45 S");
        assert_eq!(duration(3.2 * 86_400.0), "3.2 J");
        assert_eq!(days(3.2), duration(3.2 * 86_400.0));
        assert_eq!(days(0.5), "12.0 H");
        assert_eq!(duration(2.0 * 31_557_600.0), "2.0 ANS");
        // The screens' own names are views of these functions.
        assert_eq!(
            crate::hud::fmt_time(3.2 * 86_400.0),
            duration(3.2 * 86_400.0)
        );
        assert_eq!(crate::cryo_view::span(3.2), days(3.2));
        assert_eq!(credits(12_345.0), "12.3K CR");
        assert_eq!(price(5.0), "5.00");
        assert_eq!(mass(2_500.0), "3 t");
    }
}
