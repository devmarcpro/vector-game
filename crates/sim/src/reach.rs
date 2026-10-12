//! Neighbourhood radii of the galaxy, named by use (one value per meaning; audit B12).

/// Region of a market and range of a price shock or of the quantum link, ly.
pub const REGION_LY: f64 = 60.0;
/// Local reach: a regular line, "your colonies" in a surprise condition, ly.
pub const LOCAL_LY: f64 = 30.0;
/// Side of a sector of power, ly.
pub const SECTOR_LY: f64 = 240.0;
/// A star is "seen" on the galactic map when this close to a visited system, ly.
pub const SEEN_LY: f64 = 15.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_radii_are_nested_and_shared() {
        assert!(SEEN_LY < LOCAL_LY && LOCAL_LY < REGION_LY && REGION_LY < SECTOR_LY);
        assert_eq!(crate::bourse::REGION_LY, REGION_LY);
        assert_eq!(crate::surprises::SHOCK_RADIUS_LY, REGION_LY);
        assert_eq!(crate::exotic_field::QCOMM_RANGE_LY, REGION_LY);
        assert_eq!(crate::lanes::REACH_LY, LOCAL_LY);
        assert_eq!(crate::surprises::COLONY_REACH_LY, LOCAL_LY);
        assert_eq!(crate::politics::SECTOR_LY, SECTOR_LY);
        assert_eq!(crate::voyage::SEEN_RADIUS_LY, SEEN_LY);
    }

    #[test]
    fn the_history_year_is_the_calendar_year() {
        assert_eq!(crate::history::YEAR, vector_core::units::YEAR);
    }
}
