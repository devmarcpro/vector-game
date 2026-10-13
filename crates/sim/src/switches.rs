//! The switches of the console: the shield generators on and off, the floodlights, the fine
//! trim of the thrust. Each one is a real command of the ship, the same one whatever pressed it
//! (a key, a toggle switch of the cockpit, a menu line). None of it is saved: a loaded ship
//! starts with its shield on, its lights off and the trim centred, like the gear and the SAS.
use crate::Sim;

/// Electrical load of the floodlights (kW): they draw from the same bus as everything else.
pub const LIGHTS_POWER_KW: f64 = 40.0;
/// Heat and light (kW) the floodlights add to what a passive sensor sees: a lit ship can be
/// found from afar, which is the price of seeing the ground of a dark world.
pub const LIGHTS_EMISSION_KW: f64 = 400.0;
/// The trim goes from -TRIM_MAX to +TRIM_MAX notches of one percent of the thrust.
pub const TRIM_MAX: i8 = 10;

impl Sim {
    /// Whether the ship carries at least one shield generator (working or not).
    pub fn shield_installed(&self) -> bool {
        self.ship
            .design
            .modules(&self.data)
            .any(|(_, m)| m.shield > 0.0)
    }

    /// Switch the shield generators on or off. Off: they draw no power and protect nothing (the
    /// charge is gone at once); on: they draw again and the shield recharges as usual. Returns
    /// the new state; a ship without generator refuses.
    pub fn toggle_shield(&mut self) -> Result<bool, &'static str> {
        if !self.shield_installed() {
            return Err("aucun generateur de bouclier installe");
        }
        self.shield_on = !self.shield_on;
        if !self.shield_on {
            self.ship.sys.shield_pts = 0.0;
            self.effective.shield_cap = 0.0;
        }
        self.ship.sys.shield_off = !self.shield_on;
        Ok(self.shield_on)
    }

    /// Switch the floodlights on or off; returns the new state.
    pub fn toggle_lights(&mut self) -> bool {
        self.lights_on = !self.lights_on;
        self.lights_on
    }

    /// Move the thrust trim by `delta` notches (clamped); returns the new notch.
    pub fn trim_step(&mut self, delta: i8) -> i8 {
        self.trim = (self.trim + delta).clamp(-TRIM_MAX, TRIM_MAX);
        self.trim
    }

    /// Put the trim back to the middle.
    pub fn trim_reset(&mut self) {
        self.trim = 0;
    }

    /// The thrust the throttle lever and the trim ask for together, 0 to 1.
    pub fn effective_throttle(&self) -> f64 {
        (self.throttle + f64::from(self.trim) * 0.01).clamp(0.0, 1.0)
    }

    /// What the switches ask of the power bus this step (called by `run_systems`).
    pub(crate) fn sync_switches(&mut self) {
        self.ship.sys.shield_off = !self.shield_on;
        self.ship.sys.extra_kw = if self.lights_on { LIGHTS_POWER_KW } else { 0.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use crate::test_support::data;
    use crate::ShipInput;

    fn sim() -> Sim {
        new_sim(5, data())
    }

    /// A frigate with a fusion reactor, a torch, batteries and a shield generator, in flight.
    fn shielded() -> Sim {
        let mut s = sim();
        s.undock();
        let d = std::sync::Arc::clone(&s.data);
        let hull = d.hull_index("frigate").expect("hull");
        s.ship.design = crate::design::ShipDesign::empty(&d, hull);
        for id in [
            "fusion_mk3",
            "torch_mk3",
            "battery_mk3",
            "radiator_mk4",
            "lifesup_mk1",
            "tank_mk3",
            "shield_mk1",
        ] {
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
        s.step(&ShipInput::default());
        s
    }

    #[test]
    fn the_shield_switch_cuts_the_draw_and_the_protection() {
        let mut s = shielded();
        assert!(s.shield_installed());
        assert!(s.effective.shield_cap > 0.0, "the generator works at first");
        s.ship.sys.shield_pts = s.effective.shield_cap;
        let load_on = {
            s.step(&ShipInput::default());
            s.ship.sys.demand_kw
        };
        assert_eq!(s.toggle_shield(), Ok(false));
        assert_eq!(s.ship.sys.shield_pts, 0.0, "the charge goes at once");
        for _ in 0..5 {
            s.step(&ShipInput::default());
        }
        assert_eq!(s.effective.shield_cap, 0.0);
        assert_eq!(s.ship.sys.shield_pts, 0.0, "it does not recharge while off");
        assert!(
            s.ship.sys.demand_kw < load_on,
            "{} < {load_on}: off, the generator draws nothing",
            s.ship.sys.demand_kw
        );
        // A blow lands on the hull alone.
        let h0 = s.ship.hull;
        s.damage_player(50.0);
        assert!(s.ship.hull < h0, "no shield, the hull takes it");
        // Back on: it draws again and the charge comes back.
        assert_eq!(s.toggle_shield(), Ok(true));
        for _ in 0..600 {
            s.step(&ShipInput::default());
        }
        assert!(s.effective.shield_cap > 0.0 && s.ship.sys.shield_pts > 0.0);
        assert!(s.ship.sys.demand_kw >= load_on - 1.0);
    }

    #[test]
    fn a_ship_without_generator_refuses_the_shield_switch() {
        let mut s = sim();
        let d = std::sync::Arc::clone(&s.data);
        let hull = s.ship.design.hull;
        s.ship.design = crate::design::ShipDesign::empty(&d, hull);
        assert!(!s.shield_installed());
        assert!(s.toggle_shield().is_err());
        assert!(s.shield_on, "the state does not move");
    }

    #[test]
    fn the_lights_cost_power_and_shout() {
        let mut s = sim();
        s.undock();
        s.step(&ShipInput::default());
        let (quiet, range, demand) = (s.emission_kw(), s.detection_range(), s.ship.sys.demand_kw);
        assert!(!s.lights_on, "off by default: a quiet ship");
        assert!(s.toggle_lights());
        s.step(&ShipInput::default());
        assert!(
            s.emission_kw() >= quiet + LIGHTS_EMISSION_KW - 1e-9,
            "lit, the ship gives off more"
        );
        assert!(
            s.detection_range() > range,
            "pirates find a lit ship from further"
        );
        assert!(
            s.ship.sys.demand_kw >= demand + LIGHTS_POWER_KW - 1e-6,
            "{} vs {demand}",
            s.ship.sys.demand_kw
        );
        // A lit ship cannot make an ambush from a distance where a dark one could.
        let k = s
            .npcs
            .iter()
            .position(|n| n.role == crate::npc::NpcRole::Trader)
            .unwrap_or(0);
        s.npcs[k].provoked = false;
        s.npcs[k].disabled = false;
        s.npcs[k].state = crate::npc::NpcState::Orbit {
            around: 0,
            phase: 0.0,
        };
        let dark_range =
            crate::arms::passive_detection_range(s.quiet_signature_kw() - LIGHTS_EMISSION_KW);
        s.npcs[k].pos = s.ship.pos + vector_core::Vec3::new(0.0, 0.0, dark_range + 5_000.0);
        assert!(!s.is_ambush(k), "the lights give the ship away");
        assert!(!s.toggle_lights());
        assert!(s.is_ambush(k), "dark again, the ship is a shadow");
    }

    #[test]
    fn the_trim_adds_to_the_throttle_and_stays_in_range() {
        let mut s = sim();
        s.set_throttle(0.5);
        assert_eq!(s.trim, 0);
        assert_eq!(s.trim_step(5), 5);
        assert!((s.effective_throttle() - 0.55).abs() < 1e-12);
        assert_eq!(s.trim_step(100), TRIM_MAX);
        assert_eq!(s.trim_step(-100), -TRIM_MAX);
        assert!((s.effective_throttle() - 0.40).abs() < 1e-12);
        s.trim_reset();
        assert!((s.effective_throttle() - 0.5).abs() < 1e-12);
        s.set_throttle(1.0);
        s.trim_step(4);
        assert_eq!(s.effective_throttle(), 1.0, "never above full");
    }

    #[test]
    fn the_trim_changes_the_thrust_the_engines_get() {
        let mut s = sim();
        s.undock();
        s.set_throttle(0.5);
        let fwd = ShipInput {
            fwd: 1.0,
            ..Default::default()
        };
        let v0 = s.ship.vel;
        for _ in 0..50 {
            s.step(&fwd);
        }
        let plain = (s.ship.vel - v0).length();
        let mut t = sim();
        t.undock();
        t.set_throttle(0.5);
        t.trim_step(10);
        let v0 = t.ship.vel;
        for _ in 0..50 {
            t.step(&fwd);
        }
        let trimmed = (t.ship.vel - v0).length();
        assert!(trimmed > plain, "{trimmed} > {plain}");
    }
}
