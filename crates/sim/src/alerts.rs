//! Unified alerts: one priority list of everything that asks for the attention of the player.
//! This reads the existing systems and invents no state (nothing is saved): hull, fuel, heat,
//! power, radiation, structure, antimatter, impact, the crew, the bank, the law, the colonies,
//! the sector around, the mind of the AI and the orders of the yard. Alerts only inform: the game
//! never forces an answer (sandbox). The dashboard and the counter in the top bar draw this list.
use crate::bourse::DAY;
use crate::design::{T_DAMAGE, T_REF};
use crate::navigation::STRUCTURE_LIMIT_G;
use crate::Sim;

/// Gravity of an alert, most serious first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Critical => "CRITIQUE",
            Severity::Warning => "ATTENTION",
            Severity::Info => "INFO",
        }
    }
}

/// What an alert is about: the dashboard maps it to the screen that deals with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topic {
    Hull,
    Modules,
    Fuel,
    Heat,
    Power,
    Radiation,
    Structure,
    Antimatter,
    Collision,
    Health,
    Disease,
    Wounds,
    Needs,
    Pay,
    Debt,
    Insurance,
    Law,
    Tribute,
    Surprise,
    Contract,
    Colony,
    Epidemic,
    War,
    Malware,
    Yard,
    Cryo,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Alert {
    pub sev: Severity,
    pub topic: Topic,
    pub text: String,
}

// ---- thresholds (the tests pin every one of them) ---------------------------------------------

/// Hull share under which the ship is in danger, and under which it is worn.
pub const HULL_CRITICAL: f64 = 0.25;
pub const HULL_WARNING: f64 = 0.5;
/// Module health under which a module counts as broken.
pub const MODULE_BROKEN: f64 = 0.3;
/// Fuel share (in flight) under which the tank is nearly dry, and low.
pub const FUEL_CRITICAL: f64 = 0.1;
pub const FUEL_WARNING: f64 = 0.25;
/// Hull temperature (K) at which the heat becomes a warning (halfway to the damage threshold).
pub const HEAT_WARNING_K: f64 = (T_REF + T_DAMAGE) / 2.0;
/// Battery share under which a power deficit is critical.
pub const BATTERY_CRITICAL: f64 = 0.1;
/// Dose rate aboard (Gy per hour) that warns and that is critical; accumulated dose (Gy) too.
pub const DOSE_RATE_WARNING: f64 = 0.05;
pub const DOSE_RATE_CRITICAL: f64 = 0.5;
pub const DOSE_TOTAL_WARNING: f64 = 1.0;
pub const DOSE_TOTAL_CRITICAL: f64 = 4.0;
/// Share of the structural limit (in g) that warns.
pub const G_WARNING_SHARE: f64 = 0.5;
/// Seconds before an impact on a world under which the alert is critical, and the speed (m/s)
/// under which nobody calls it an impact.
pub const IMPACT_SECONDS: f64 = 90.0;
pub const IMPACT_MIN_SPEED: f64 = 15.0;
/// Person's health that is critical and that warns; need (hunger, thirst) that warns.
pub const HEALTH_CRITICAL: f32 = 0.3;
pub const HEALTH_WARNING: f32 = 0.6;
pub const NEED_WARNING: f32 = 0.8;
/// Blood (share of the normal) that makes a person critical; wound severity called grave.
pub const BLOOD_CRITICAL: f32 = 0.55;
pub const WOUND_GRAVE: f32 = 0.5;
/// Share of the credit ceiling that warns about the debt.
pub const DEBT_WARNING: f64 = 0.8;
/// Days before a contract (or a policy) expires that warn, then inform.
pub const EXPIRY_WARNING_DAYS: f64 = 3.0;
pub const EXPIRY_INFO_DAYS: f64 = 10.0;
/// Notoriety that makes the player wanted (a warning, before the warrant of `law`).
pub const NOTORIETY_WARNING: f32 = 1.0;
/// Colony: unrest above, supplies under, share of sick people above which it warns.
pub const COLONY_UNREST: f64 = 0.5;
pub const COLONY_SUPPLY: f64 = 0.6;
pub const COLONY_SICK: f64 = 0.05;
/// Prevalence of an outbreak in the sector that informs, and that warns.
pub const EPIDEMIC_INFO: f64 = 0.001;
pub const EPIDEMIC_WARNING: f64 = 0.05;
/// Malware share that warns and that is critical.
pub const MALWARE_WARNING: f64 = 0.3;
pub const MALWARE_CRITICAL: f64 = 0.6;

fn al(sev: Severity, topic: Topic, text: impl Into<String>) -> Alert {
    Alert {
        sev,
        topic,
        text: text.into(),
    }
}

impl Sim {
    /// Seconds before the ship hits the world it falls toward, and which one: only when it is in
    /// free flight (not docked, not held next to a body), closing faster than `IMPACT_MIN_SPEED`.
    pub fn impact_eta(&self) -> Option<(usize, f64)> {
        if self.docked.is_some() || self.parked.is_some() {
            return None;
        }
        let i = self.ref_body?;
        let b = &self.bodies[i];
        if matches!(b.kind, vector_world::BodyKind::Station) || b.gm <= 0.0 {
            return None;
        }
        let r = self.ship.pos - b.pos;
        let d = r.length();
        if d <= 0.0 {
            return None;
        }
        let closing = -(self.ship.vel - b.vel).dot(r * (1.0 / d));
        let alt = (d - b.radius).max(0.0);
        (closing >= IMPACT_MIN_SPEED).then_some((i, alt / closing))
    }

    /// Everything that asks for attention now, most serious first (then in a stable order).
    pub fn alerts(&self) -> Vec<Alert> {
        let mut out: Vec<Alert> = Vec::new();
        self.ship_alerts(&mut out);
        self.crew_alerts(&mut out);
        self.money_alerts(&mut out);
        self.world_alerts(&mut out);
        out.sort_by_key(|a| a.sev);
        out
    }

    fn ship_alerts(&self, out: &mut Vec<Alert>) {
        use Severity::*;
        let (ship, d) = (&self.ship, &self.data);
        let hull = ship.hull / ship.stats.hull_max.max(1.0);
        if hull < HULL_CRITICAL {
            out.push(al(
                Critical,
                Topic::Hull,
                format!("COQUE A {:.0} % : RUPTURE PROCHE", hull * 100.0),
            ));
        } else if hull < HULL_WARNING {
            out.push(al(
                Warning,
                Topic::Hull,
                format!("COQUE ENDOMMAGEE : {:.0} %", hull * 100.0),
            ));
        }
        let broken: Vec<String> = ship
            .design
            .modules(d)
            .filter(|(i, _)| ship.sys.health[*i] < MODULE_BROKEN)
            .map(|(_, m)| m.name.to_uppercase())
            .collect();
        if !broken.is_empty() {
            out.push(al(
                Warning,
                Topic::Modules,
                format!(
                    "{} MODULE(S) HORS D'USAGE : {}",
                    broken.len(),
                    broken.join(", ")
                ),
            ));
        }
        let fuel = ship.fuel / ship.stats.fuel_cap.max(1.0);
        if self.docked.is_none() {
            if fuel < FUEL_CRITICAL && self.parked.is_none() {
                out.push(al(
                    Critical,
                    Topic::Fuel,
                    format!("RESERVOIR PRESQUE VIDE : {:.0} %", fuel * 100.0),
                ));
            } else if fuel < FUEL_WARNING {
                out.push(al(
                    Warning,
                    Topic::Fuel,
                    format!("CARBURANT BAS : {:.0} %", fuel * 100.0),
                ));
            }
        }
        let sys = &ship.sys;
        if sys.overheating || sys.temp_k >= T_DAMAGE {
            out.push(al(
                Critical,
                Topic::Heat,
                format!("SURCHAUFFE : {:.0} K, LES MODULES SOUFFRENT", sys.temp_k),
            ));
        } else if sys.temp_k >= HEAT_WARNING_K {
            out.push(al(
                Warning,
                Topic::Heat,
                format!("CHALEUR ELEVEE : {:.0} K", sys.temp_k),
            ));
        }
        let battery = sys.battery_mj / ship.stats.battery_cap.max(1.0);
        if sys.demand_kw > sys.supply_kw + 1e-9 {
            if battery < BATTERY_CRITICAL {
                out.push(al(
                    Critical,
                    Topic::Power,
                    "DEFICIT D'ENERGIE ET BATTERIE VIDE".to_string(),
                ));
            } else {
                out.push(al(
                    Warning,
                    Topic::Power,
                    format!(
                        "DEFICIT D'ENERGIE : {:.0} KW DEMANDES, {:.0} KW",
                        sys.demand_kw, sys.supply_kw
                    ),
                ));
            }
        }
        let rate = self.dose_rate_aboard();
        if rate >= DOSE_RATE_CRITICAL || sys.dose_gy >= DOSE_TOTAL_CRITICAL {
            out.push(al(
                Critical,
                Topic::Radiation,
                format!(
                    "RAYONNEMENT : {:.1} MGY/H, DOSE {:.2} GY",
                    rate * 1e3,
                    sys.dose_gy
                ),
            ));
        } else if rate >= DOSE_RATE_WARNING || sys.dose_gy >= DOSE_TOTAL_WARNING {
            out.push(al(
                Warning,
                Topic::Radiation,
                format!(
                    "RAYONNEMENT : {:.1} MGY/H, DOSE {:.2} GY",
                    rate * 1e3,
                    sys.dose_gy
                ),
            ));
        }
        let g = self.structural_load_g();
        if g >= STRUCTURE_LIMIT_G {
            out.push(al(
                Critical,
                Topic::Structure,
                format!("CHARGE {g:.0} G : LA STRUCTURE CEDE"),
            ));
        } else if g >= STRUCTURE_LIMIT_G * G_WARNING_SHARE {
            out.push(al(
                Warning,
                Topic::Structure,
                format!("CHARGE {g:.0} G SUR LA STRUCTURE"),
            ));
        }
        if self.antimatter_stored() > 0.0 && !self.trap_powered() {
            out.push(al(
                Critical,
                Topic::Antimatter,
                "PIEGE A ANTIMATIERE COUPE : FUITE".to_string(),
            ));
        }
        if let Some((b, t)) = self.impact_eta() {
            if t <= IMPACT_SECONDS {
                out.push(al(
                    Critical,
                    Topic::Collision,
                    format!(
                        "IMPACT SUR {} DANS {:.0} S",
                        self.bodies[b].name.to_uppercase(),
                        t
                    ),
                ));
            }
        }
    }

    fn crew_alerts(&self, out: &mut Vec<Alert>) {
        use Severity::*;
        let (mut sick, mut grave, mut needy) = (0, 0, 0);
        for p in self.crew.alive() {
            let b = &p.body;
            if p.health < HEALTH_CRITICAL || b.blood < BLOOD_CRITICAL {
                out.push(al(
                    Critical,
                    Topic::Health,
                    format!(
                        "{} EN DETRESSE : SANTE {:.0} %",
                        p.name.to_uppercase(),
                        p.health * 100.0
                    ),
                ));
            } else if p.health < HEALTH_WARNING {
                out.push(al(
                    Warning,
                    Topic::Health,
                    format!(
                        "{} AFFAIBLI(E) : SANTE {:.0} %",
                        p.name.to_uppercase(),
                        p.health * 100.0
                    ),
                ));
            }
            if p.disease.is_some() {
                sick += 1;
            }
            if b.wounds.iter().any(|w| w.sev >= WOUND_GRAVE) {
                grave += 1;
            }
            if p.hunger >= NEED_WARNING || p.thirst >= NEED_WARNING {
                needy += 1;
            }
        }
        let st = self.cryo_status();
        if st.sleepers > st.held {
            out.push(al(
                Critical,
                Topic::Cryo,
                format!(
                    "{} DORMEUR(S) SANS CAISSON ALIMENTE : ILS SE RECHAUFFENT",
                    st.sleepers - st.held
                ),
            ));
        }
        if st.sleepers > 0 || st.colonists > 0 {
            let spare = self.data.resource_index("spare_parts");
            if st.pods > 0 && spare.is_some_and(|r| self.ship.cargo.kg[r] < 0.5) {
                out.push(al(
                    Warning,
                    Topic::Cryo,
                    "CAISSONS CRYO SANS PIECES DETACHEES : LES PANNES SE MULTIPLIENT".to_string(),
                ));
            }
        }
        if sick > 0 {
            out.push(al(
                Warning,
                Topic::Disease,
                format!("{sick} PERSONNE(S) MALADE(S) A BORD"),
            ));
        }
        if grave > 0 {
            out.push(al(
                Warning,
                Topic::Wounds,
                format!("{grave} BLESSE(S) GRAVE(S)"),
            ));
        }
        if needy > 0 {
            out.push(al(
                Warning,
                Topic::Needs,
                format!("{needy} PERSONNE(S) AVEC FAIM OU SOIF"),
            ));
        }
        if self.crew.unpaid {
            out.push(al(
                Warning,
                Topic::Pay,
                "SALAIRES IMPAYES : LE MORAL SE DEGRADE".to_string(),
            ));
        }
    }

    fn money_alerts(&self, out: &mut Vec<Alert>) {
        use Severity::*;
        if self.bank.loan > 0.0 {
            if self.bank.overdue_days > 0 {
                out.push(al(
                    Critical,
                    Topic::Debt,
                    format!(
                        "PRET EN RETARD DEPUIS {} J : {:.0} CR DUS",
                        self.bank.overdue_days, self.bank.loan
                    ),
                ));
            } else if self.bank.loan >= DEBT_WARNING * self.credit_limit() {
                out.push(al(
                    Warning,
                    Topic::Debt,
                    format!("DETTE {:.0} CR PRES DU PLAFOND DE CREDIT", self.bank.loan),
                ));
            }
        }
        let left = (self.trade.insured_until - self.time) / DAY;
        if left > 0.0 && left <= EXPIRY_WARNING_DAYS {
            out.push(al(
                Info,
                Topic::Insurance,
                format!("L'ASSURANCE EXPIRE DANS {left:.1} J"),
            ));
        }
        if let Some((_, c)) = self.tribute_due {
            out.push(al(
                Warning,
                Topic::Tribute,
                format!("TRIBUT EXIGE PAR DES PIRATES : {c:.0} CR"),
            ));
        }
    }

    fn world_alerts(&self, out: &mut Vec<Alert>) {
        use Severity::*;
        let d = &self.data;
        if let Some(f) = self.faction {
            let name = d.factions[f].name.to_uppercase();
            if self.has_warrant(f) {
                out.push(al(
                    Critical,
                    Topic::Law,
                    format!("MANDAT D'ARRET CHEZ {name} : LES QUAIS SONT FERMES"),
                ));
            } else if self.notoriety_in(f) >= NOTORIETY_WARNING {
                out.push(al(
                    Warning,
                    Topic::Law,
                    format!(
                        "RECHERCHE PAR {name} : NOTORIETE {:.1}",
                        self.notoriety_in(f)
                    ),
                ));
            }
            let owed = self.fines_owed(f);
            if owed > 0.0 {
                out.push(al(
                    Warning,
                    Topic::Law,
                    format!("AMENDES IMPAYEES CHEZ {name} : {owed:.0} CR"),
                ));
            }
        }
        let waiting = self.surprises.pending.len();
        if waiting > 0 {
            out.push(al(
                Info,
                Topic::Surprise,
                format!("{waiting} SURPRISE(S) ATTENDENT UNE REPONSE (J)"),
            ));
        }
        for c in &self.contracts {
            let days = (c.expires - self.time) / DAY;
            let what = if c.pax > 0 {
                format!("PASSAGERS POUR {}", c.dest_name.to_uppercase())
            } else {
                format!("LIVRAISON A {}", c.dest_name.to_uppercase())
            };
            if days <= EXPIRY_WARNING_DAYS {
                out.push(al(
                    Warning,
                    Topic::Contract,
                    format!("CONTRAT URGENT, {what} : {:.1} J", days.max(0.0)),
                ));
            } else if days <= EXPIRY_INFO_DAYS {
                out.push(al(
                    Info,
                    Topic::Contract,
                    format!("ECHEANCE DE CONTRAT, {what} : {days:.0} J"),
                ));
            }
        }
        for col in &self.colonies {
            let st = &col.stats;
            let n = col.name.to_uppercase();
            if col.unrest >= COLONY_UNREST {
                out.push(al(
                    Warning,
                    Topic::Colony,
                    format!("{n} : AGITATION {:.0} %", col.unrest * 100.0),
                ));
            }
            if st.power_sat < COLONY_SUPPLY
                || st.food_sat < COLONY_SUPPLY
                || st.water_sat < COLONY_SUPPLY
            {
                out.push(al(
                    Warning,
                    Topic::Colony,
                    format!(
                        "{n} : PENURIE (ENERGIE {:.0} %, VIVRES {:.0} %, EAU {:.0} %)",
                        st.power_sat * 100.0,
                        st.food_sat * 100.0,
                        st.water_sat * 100.0
                    ),
                ));
            }
            if col.debt_days > 0.0 {
                out.push(al(
                    Warning,
                    Topic::Colony,
                    format!("{n} : TRESORERIE A SEC DEPUIS {:.0} J", col.debt_days),
                ));
            }
            if col.pop > 0.0 && col.sick() / col.pop >= COLONY_SICK {
                out.push(al(
                    Warning,
                    Topic::Epidemic,
                    format!(
                        "{n} : EPIDEMIE, {:.0} % DE MALADES",
                        100.0 * col.sick() / col.pop
                    ),
                ));
            }
        }
        for o in &self.yard_orders {
            if o.ready <= self.time {
                out.push(al(
                    Info,
                    Topic::Yard,
                    format!("COMMANDE DE CHANTIER PRETE : {}", o.name.to_uppercase()),
                ));
            }
        }
        if self.mind.malware >= MALWARE_CRITICAL {
            out.push(al(
                Critical,
                Topic::Malware,
                format!(
                    "LOGICIEL MALVEILLANT : {:.0} % DE L'ESPRIT",
                    self.mind.malware * 100.0
                ),
            ));
        } else if self.mind.malware >= MALWARE_WARNING {
            out.push(al(
                Warning,
                Topic::Malware,
                format!(
                    "LOGICIEL MALVEILLANT : {:.0} % DE L'ESPRIT",
                    self.mind.malware * 100.0
                ),
            ));
        }
        // The sector around: war, siege, outbreaks.
        let cell = crate::politics::cell_of(self.here_pos);
        if self.local_sector().at_war {
            let besieged =
                crate::arms::is_besieged(d, &self.galaxy, self.galaxy.seed, cell, self.time);
            if besieged {
                out.push(al(
                    Warning,
                    Topic::War,
                    "SECTEUR EN GUERRE ET BLOQUE : PRIX MAJORES".to_string(),
                ));
            } else {
                out.push(al(Info, Topic::War, "GUERRE DANS LE SECTEUR".to_string()));
            }
        }
        let worst = crate::epidemic::prevalence_at(d, self.galaxy.seed, self.here_pos, self.time)
            .into_iter()
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((k, i)) = worst {
            let name = d.pathogens[k].name.to_uppercase();
            if i >= EPIDEMIC_WARNING {
                out.push(al(
                    Warning,
                    Topic::Epidemic,
                    format!(
                        "EPIDEMIE DANS LE SECTEUR : {name}, {:.0} % DE CONTAGIEUX",
                        i * 100.0
                    ),
                ));
            } else if i >= EPIDEMIC_INFO {
                out.push(al(
                    Info,
                    Topic::Epidemic,
                    format!("FOYER DE {name} DANS LE SECTEUR"),
                ));
            }
        }
    }

    /// (critical, warning, info) counts of the alerts.
    pub fn alert_counts(alerts: &[Alert]) -> (usize, usize, usize) {
        let n = |s| alerts.iter().filter(|a| a.sev == s).count();
        (
            n(Severity::Critical),
            n(Severity::Warning),
            n(Severity::Info),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn sim() -> Sim {
        let data = Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        );
        new_sim(7, data)
    }

    fn has(s: &Sim, sev: Severity, topic: Topic) -> bool {
        s.alerts().iter().any(|a| a.sev == sev && a.topic == topic)
    }

    fn none(s: &Sim, topic: Topic) -> bool {
        !s.alerts().iter().any(|a| a.topic == topic)
    }

    #[test]
    fn a_fresh_ship_has_nothing_serious() {
        let s = sim();
        let a = s.alerts();
        assert!(
            a.iter().all(|a| a.sev == Severity::Info),
            "fresh start: {a:?}"
        );
    }

    #[test]
    fn alerts_are_sorted_by_gravity() {
        let mut s = sim();
        s.ship.hull = 1.0; // critical
        s.crew.unpaid = true; // warning
        s.mind.malware = 0.7; // critical
        let a = s.alerts();
        assert!(a.len() >= 3);
        assert!(a.windows(2).all(|w| w[0].sev <= w[1].sev), "{a:?}");
        assert_eq!(a[0].sev, Severity::Critical);
        let (c, w, _) = Sim::alert_counts(&a);
        assert!(c >= 2 && w >= 1);
    }

    #[test]
    fn hull_thresholds() {
        let mut s = sim();
        let max = s.ship.stats.hull_max;
        s.ship.hull = max * (HULL_WARNING + 0.01);
        assert!(none(&s, Topic::Hull));
        s.ship.hull = max * (HULL_WARNING - 0.01);
        assert!(has(&s, Severity::Warning, Topic::Hull));
        s.ship.hull = max * (HULL_CRITICAL + 0.01);
        assert!(has(&s, Severity::Warning, Topic::Hull));
        s.ship.hull = max * (HULL_CRITICAL - 0.01);
        assert!(has(&s, Severity::Critical, Topic::Hull));
    }

    #[test]
    fn fuel_alerts_only_in_flight() {
        let mut s = sim();
        let cap = s.ship.stats.fuel_cap;
        s.docked = None;
        s.ship.fuel = cap * (FUEL_WARNING + 0.01);
        assert!(none(&s, Topic::Fuel));
        s.ship.fuel = cap * (FUEL_WARNING - 0.01);
        assert!(has(&s, Severity::Warning, Topic::Fuel));
        s.ship.fuel = cap * (FUEL_CRITICAL - 0.01);
        assert!(has(&s, Severity::Critical, Topic::Fuel));
        // Held on the ground, an empty tank is a worry, not a danger.
        s.parked = Some((0, vector_core::Vec3::ZERO));
        assert!(has(&s, Severity::Warning, Topic::Fuel));
        assert!(!has(&s, Severity::Critical, Topic::Fuel));
        s.parked = None;
        s.docked = Some(0);
        assert!(none(&s, Topic::Fuel), "a docked ship refuels at the market");
    }

    #[test]
    fn heat_thresholds() {
        let mut s = sim();
        s.ship.sys.temp_k = HEAT_WARNING_K - 1.0;
        assert!(none(&s, Topic::Heat));
        s.ship.sys.temp_k = HEAT_WARNING_K + 1.0;
        assert!(has(&s, Severity::Warning, Topic::Heat));
        s.ship.sys.temp_k = T_DAMAGE + 1.0;
        assert!(has(&s, Severity::Critical, Topic::Heat));
        s.ship.sys.temp_k = 300.0;
        s.ship.sys.overheating = true;
        assert!(has(&s, Severity::Critical, Topic::Heat));
    }

    #[test]
    fn power_deficit_is_critical_without_battery() {
        let mut s = sim();
        s.ship.sys.supply_kw = 10.0;
        s.ship.sys.demand_kw = 5.0;
        assert!(none(&s, Topic::Power));
        s.ship.sys.demand_kw = 20.0;
        s.ship.sys.battery_mj = s.ship.stats.battery_cap;
        assert!(has(&s, Severity::Warning, Topic::Power));
        s.ship.sys.battery_mj = 0.0;
        assert!(has(&s, Severity::Critical, Topic::Power));
    }

    #[test]
    fn accumulated_dose_thresholds() {
        let mut s = sim();
        s.ship.sys.dose_gy = DOSE_TOTAL_WARNING - 0.01;
        assert!(none(&s, Topic::Radiation));
        s.ship.sys.dose_gy = DOSE_TOTAL_WARNING + 0.01;
        assert!(has(&s, Severity::Warning, Topic::Radiation));
        s.ship.sys.dose_gy = DOSE_TOTAL_CRITICAL + 0.01;
        assert!(has(&s, Severity::Critical, Topic::Radiation));
    }

    #[test]
    fn structure_load_thresholds() {
        let mut s = sim();
        s.g_load = STRUCTURE_LIMIT_G * G_WARNING_SHARE - 1.0;
        assert!(none(&s, Topic::Structure));
        s.g_load = STRUCTURE_LIMIT_G * G_WARNING_SHARE + 1.0;
        assert!(has(&s, Severity::Warning, Topic::Structure));
        s.g_load = STRUCTURE_LIMIT_G + 1.0;
        assert!(has(&s, Severity::Critical, Topic::Structure));
    }

    #[test]
    fn antimatter_leaks_when_the_trap_is_off() {
        let mut s = sim();
        assert!(none(&s, Topic::Antimatter));
        let r = s.data.resource_index("antimatter").expect("antimatter");
        let data = Arc::clone(&s.data);
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        s.ship.cargo.add(&data, r, 0.01);
        // No containment module on the starter ship: the trap is not powered.
        assert!(!s.trap_powered());
        assert!(has(&s, Severity::Critical, Topic::Antimatter));
    }

    #[test]
    fn impact_needs_free_flight_and_a_closing_speed() {
        let mut s = sim();
        s.docked = None;
        s.parked = None;
        let b = s
            .bodies
            .iter()
            .position(|b| matches!(b.kind, vector_world::BodyKind::Planet(_)))
            .expect("planet");
        let dir = vector_core::Vec3::new(1.0, 0.0, 0.0);
        let alt = 10_000.0;
        s.ship.pos = s.bodies[b].pos + dir * (s.bodies[b].radius + alt);
        s.ref_body = Some(b);
        // Falling at 200 m/s: 50 s to impact.
        s.ship.vel = s.bodies[b].vel - dir * 200.0;
        let (i, t) = s.impact_eta().expect("falling");
        assert_eq!(i, b);
        assert!((t - 50.0).abs() < 1e-6, "{t}");
        assert!(has(&s, Severity::Critical, Topic::Collision));
        // Falling slowly enough that the impact is far: no alert.
        s.ship.vel = s.bodies[b].vel - dir * 20.0;
        assert!(none(&s, Topic::Collision), "500 s away");
        // Moving away or too slowly: no impact at all.
        s.ship.vel = s.bodies[b].vel + dir * 200.0;
        assert!(s.impact_eta().is_none());
        s.ship.vel = s.bodies[b].vel - dir * (IMPACT_MIN_SPEED - 1.0);
        assert!(s.impact_eta().is_none());
        // Docked: nothing.
        s.ship.vel = s.bodies[b].vel - dir * 200.0;
        s.docked = Some(0);
        assert!(none(&s, Topic::Collision));
    }

    #[test]
    fn crew_thresholds() {
        let mut s = sim();
        assert!(s.crew.count() > 0);
        let p = 0;
        s.crew.members[p].health = HEALTH_WARNING + 0.05;
        assert!(none(&s, Topic::Health));
        s.crew.members[p].health = HEALTH_WARNING - 0.05;
        assert!(has(&s, Severity::Warning, Topic::Health));
        s.crew.members[p].health = HEALTH_CRITICAL - 0.05;
        assert!(has(&s, Severity::Critical, Topic::Health));
        s.crew.members[p].health = 1.0;
        s.crew.members[p].body.blood = BLOOD_CRITICAL - 0.05;
        assert!(has(&s, Severity::Critical, Topic::Health));
        s.crew.members[p].body.blood = 1.0;
        s.crew.members[p].thirst = NEED_WARNING + 0.05;
        assert!(has(&s, Severity::Warning, Topic::Needs));
        s.crew.members[p].thirst = 0.0;
        s.crew.unpaid = true;
        assert!(has(&s, Severity::Warning, Topic::Pay));
        s.crew.unpaid = false;
        assert!(s.alerts().iter().all(|a| a.sev == Severity::Info));
    }

    #[test]
    fn debt_alerts() {
        let mut s = sim();
        s.bank.loan = 100.0;
        s.bank.overdue_days = 0;
        assert!(none(&s, Topic::Debt));
        s.bank.loan = s.credit_limit() * (DEBT_WARNING + 0.05);
        assert!(has(&s, Severity::Warning, Topic::Debt));
        s.bank.overdue_days = 2;
        assert!(has(&s, Severity::Critical, Topic::Debt));
    }

    #[test]
    fn contract_expiry_thresholds() {
        let mut s = sim();
        let c = crate::economy::Contract {
            id: 1,
            dest_star: s.here,
            dest_seed: 0,
            dest_body: 0,
            dest_name: "Port".into(),
            dest_system: "X".into(),
            dist_ly: 1.0,
            resource: 0,
            kg: 10.0,
            reward: 100.0,
            expires: s.time + (EXPIRY_INFO_DAYS + 1.0) * DAY,
            faction: None,
            pax: 0,
            pax_kind: 0,
            legs: Vec::new(),
            guide: false,
            origin: [0; 3],
        };
        s.contracts.push(c);
        assert!(none(&s, Topic::Contract));
        s.contracts[0].expires = s.time + (EXPIRY_INFO_DAYS - 1.0) * DAY;
        assert!(has(&s, Severity::Info, Topic::Contract));
        s.contracts[0].expires = s.time + (EXPIRY_WARNING_DAYS - 0.5) * DAY;
        assert!(has(&s, Severity::Warning, Topic::Contract));
    }

    #[test]
    fn law_and_mind_thresholds() {
        let mut s = sim();
        let f = s.faction.unwrap_or(0);
        s.faction = Some(f);
        s.trade.record.push(crate::law::Record {
            faction: f,
            notoriety: NOTORIETY_WARNING + 0.5,
            grave: 0.0,
            owed: 0.0,
            t: s.time,
        });
        assert!(has(&s, Severity::Warning, Topic::Law));
        s.trade.record[0].notoriety = crate::law::WARRANT + 0.5;
        assert!(has(&s, Severity::Critical, Topic::Law));
        s.mind.malware = MALWARE_WARNING + 0.01;
        assert!(has(&s, Severity::Warning, Topic::Malware));
        s.mind.malware = MALWARE_CRITICAL + 0.01;
        assert!(has(&s, Severity::Critical, Topic::Malware));
        s.mind.malware = MALWARE_WARNING - 0.05;
        assert!(none(&s, Topic::Malware));
    }
}
