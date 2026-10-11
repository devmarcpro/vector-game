//! Health tracking of a colony (H in the colony screen): the epidemics running in it, their
//! compartments (susceptible, exposed, sick, immune), the effective reproduction number under the
//! colony's measures, what runs in the sector around and aboard the ship, and the journal.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::ui::bar;
use vector_render::Framebuffer;

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let Some(c) = g.sim.colonies.get(g.col_idx) else {
        return;
    };
    let data = &g.data;
    let mut row = 5usize;
    let level = data
        .policy_index("quarantine")
        .and_then(|p| {
            data.policies[p]
                .levels
                .get(c.policies[p])
                .map(|l| l.0.clone())
        })
        .unwrap_or_default();
    let stock = |id: &str| data.resource_index(id).map_or(0.0, |r| c.storage[r]);
    fb.text(
        1,
        row,
        &format!(
            "SUIVI SANITAIRE  QUARANTAINE : {}  MEDICAMENTS {:.0} KG  VACCINS {:.0} KG  (H : RETOUR)",
            level.to_uppercase(),
            stock("medicine"),
            stock("vaccine")
        ),
        AMBER,
    );
    row += 2;
    fb.text(
        1,
        row,
        "AGENT                      ORIGINE   SAINS  INCUBATION  MALADES  IMMUNISES  R EFFECTIF  TRAVAIL PERDU",
        DIM,
    );
    row += 1;
    if c.outbreaks.iter().all(|o| !o.active() && o.r < 0.01) {
        fb.text(1, row, "AUCUNE EPIDEMIE : LA COLONIE EST SAINE.", GREEN);
        row += 1;
    }
    for o in &c.outbreaks {
        let p = &data.pathogens[o.pathogen];
        let col = if o.i > 0.1 {
            RED
        } else if o.active() {
            AMBER
        } else {
            DIM
        };
        fb.text(
            1,
            row,
            &format!(
                "{:<26.26} {:<9.9} {:>4.0}%  {:>9.1}%  {:>3.0} {:>4.0}%  {:>8.0}%  {:>10.2}  {:>9.0}%",
                p.name.to_uppercase(),
                p.origin.to_uppercase(),
                o.s * 100.0,
                o.e * 100.0,
                o.i * c.pop,
                o.i * 100.0,
                o.r * 100.0,
                c.r_effective(data, o),
                (p.prod_hit * (o.i / 0.1).min(3.0)).min(0.8) * 100.0
            ),
            col,
        );
        row += 1;
    }
    row += 1;
    fb.text(
        1,
        row,
        &format!(
            "MORAL {}  PEUR {:.0}%  PRODUCTION PERDUE {:.0}%  MALADES {:.0}",
            bar(c.happiness, 8),
            c.fear() * 100.0,
            c.labor_loss(data) * 100.0,
            c.sick()
        ),
        GREEN,
    );
    row += 2;
    // The sector around.
    let pos = g.sim.galaxy.star(c.star).map_or(g.sim.here_pos, |e| e.pos);
    let around = vector_sim::epidemic::prevalence_at(data, g.sim.galaxy.seed, pos, g.sim.time);
    fb.text(1, row, "DANS LE SECTEUR :", AMBER);
    if around.is_empty() {
        fb.text(19, row, "RIEN A SIGNALER", GREEN);
    }
    for (k, i) in &around {
        row += 1;
        fb.text(
            3,
            row,
            &format!(
                "{} : {:.1}% DE LA POPULATION MALADE",
                data.pathogens[*k].name.to_uppercase(),
                i * 100.0
            ),
            if *i > 0.05 { RED } else { DIM },
        );
    }
    row += 2;
    // The ship.
    let aboard: Vec<String> = g
        .sim
        .crew
        .alive()
        .filter_map(|p| {
            p.disease
                .and_then(|(d, _)| data.pathogen_of_disease(d))
                .map(|k| format!("{} ({})", p.name, data.pathogens[k].name))
        })
        .collect();
    fb.text(
        1,
        row,
        &format!(
            "A BORD : {}",
            if aboard.is_empty() {
                "PERSONNE N'EST ATTEINT".to_string()
            } else {
                aboard.join(", ").to_uppercase()
            }
        ),
        if aboard.is_empty() { GREEN } else { RED },
    );
    row += 2;
    fb.text(1, row, "JOURNAL :", AMBER);
    for (_, line) in c.log.iter().rev().take(8) {
        row += 1;
        fb.text(3, row, &line.to_uppercase(), DIM);
    }
}
