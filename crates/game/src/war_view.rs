//! War page of the factions screen (X, then G): the fronts and sieges around the ship, what the
//! selected people thinks of the player (war, ceasefire, reparations, crimes of war), the
//! mercenary contracts of the sector, and the spies of the player with their operations. Plus
//! the squadron panel of the fleet screen (F3): orders, ammunition, prizes.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED};
use crate::mouse::Act;
use vector_render::Framebuffer;
use vector_sim::arms::{self, AgentState, SPY_OPS};
use vector_sim::fleet::{FleetOrder, ORDERS};

const WHITE: u32 = 0x00FF_FFFF;

/// Codes of `click_war`: recruit, next agent, next target sector, infiltrate, the four
/// operations (4..8), withdraw, drone, peace, then contract `k` = 20 + k.
pub const W_RECRUIT: usize = 0;
pub const W_AGENT: usize = 1;
pub const W_CELL: usize = 2;
pub const W_INFILTRATE: usize = 3;
pub const W_OP: usize = 4;
pub const W_WITHDRAW: usize = 8;
pub const W_DRONE: usize = 9;
pub const W_PEACE: usize = 10;
pub const W_CONTRACT: usize = 20;

impl Game {
    /// Open the war page: the sectors with a war in reach are listed once.
    pub fn open_war_page(&mut self) {
        self.war_cells = self.sim.war_cells();
        self.war_target = 0;
        self.war_agent = 0;
        self.war_page = true;
    }

    /// Everything the buttons and keys of the war page do.
    pub fn click_war(&mut self, code: usize) {
        let agent_id = self.sim.military.agents.get(self.war_agent).map(|a| a.id);
        let cell = self.war_cells.get(self.war_target).copied();
        let res: Result<String, &'static str> = match code {
            W_RECRUIT => self
                .sim
                .recruit_agent()
                .map(|_| "AGENT RECRUTE".to_string()),
            W_AGENT => {
                let n = self.sim.military.agents.len().max(1);
                self.war_agent = (self.war_agent + 1) % n;
                return;
            }
            W_CELL => {
                let n = self.war_cells.len().max(1);
                self.war_target = (self.war_target + 1) % n;
                return;
            }
            W_INFILTRATE => match (agent_id, cell) {
                (Some(a), Some(c)) => self.sim.infiltrate(a, c),
                (None, _) => Err("aucun agent"),
                _ => Err("aucun secteur en guerre a portee"),
            },
            c if (W_OP..W_OP + 4).contains(&c) => match agent_id {
                Some(a) => self.sim.spy_op(a, SPY_OPS[c - W_OP]),
                None => Err("aucun agent"),
            },
            W_WITHDRAW => match agent_id {
                Some(a) => self
                    .sim
                    .withdraw_agent(a)
                    .map(|()| "AGENT RAPPELE".to_string()),
                None => Err("aucun agent"),
            },
            W_DRONE => match cell {
                Some(c) => self
                    .sim
                    .launch_recon_drone(c)
                    .map(|n| format!("DRONE : {n} FRONT(S) RELEVE(S)")),
                None => Err("aucun secteur en guerre a portee"),
            },
            W_PEACE => self
                .sim
                .sue_for_peace(self.fac_sel)
                .map(|p| format!("CESSEZ-LE-FEU SIGNE : {p:.0} CR DE REPARATIONS")),
            c if c >= W_CONTRACT => {
                let ids: Vec<u32> = self
                    .sim
                    .fleet
                    .iter()
                    .filter(|s| {
                        s.order == FleetOrder::Trade
                            && s.route.is_none()
                            && arms::hardpoints(&self.data, s.hull) > 0.0
                            && !s.prize
                    })
                    .map(|s| s.id)
                    .collect();
                self.sim
                    .hire_out(c - W_CONTRACT, &ids)
                    .map(|_| format!("{} VAISSEAU(X) ENGAGE(S)", ids.len()))
            }
            _ => return,
        };
        match res {
            Ok(m) => self.say(m.to_uppercase()),
            Err(e) => self.say(e.to_uppercase()),
        }
        self.war_cells = self.sim.war_cells();
        self.war_target = self.war_target.min(self.war_cells.len().saturating_sub(1));
    }

    pub fn war_key(&mut self, code: winit::keyboard::KeyCode) -> bool {
        use winit::keyboard::KeyCode as K;
        let c = match code {
            K::KeyR => W_RECRUIT,
            K::KeyA => W_AGENT,
            K::KeyC => W_CELL,
            K::KeyI => W_INFILTRATE,
            K::Digit1 => W_OP,
            K::Digit2 => W_OP + 1,
            K::Digit3 => W_OP + 2,
            K::Digit4 => W_OP + 3,
            K::KeyW => W_WITHDRAW,
            K::KeyD => W_DRONE,
            K::KeyP => W_PEACE,
            K::KeyM => W_CONTRACT,
            K::ArrowRight => {
                self.fac_sel = (self.fac_sel + 1) % self.data.factions.len().max(1);
                return true;
            }
            K::ArrowLeft => {
                let n = self.data.factions.len().max(1);
                self.fac_sel = (self.fac_sel + n - 1) % n;
                return true;
            }
            _ => return false,
        };
        self.click_war(c);
        true
    }
}

fn button(
    fb: &mut Framebuffer,
    g: &Game,
    col: usize,
    row: usize,
    _w: usize,
    label: &str,
    code: usize,
) {
    let w = label.chars().count() + 2;
    let rc = (
        col as i32 * 8,
        row as i32 * 10,
        (col + w) as i32 * 8,
        row as i32 * 10 + 11,
    );
    crate::proto_view::button(fb, g, rc, label, Act::Call(Game::click_war, code));
}

pub fn draw_war(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let t = s.time;
    fb.text(1, 1, "GUERRE ET RENSEIGNEMENT", AMBER);
    fb.text(40, 1, "[G] RETOUR AUX FACTIONS", DIM);
    fb.text(
        1,
        2,
        "FRONTS, BLOCUS, PAIX, CONTRATS DE MERCENAIRES ET AGENTS SECRETS (SOUS-ONGLET DES FACTIONS)",
        DIM,
    );
    let here = s.current_cell();
    fb.text(1, 4, &format!("SECTEUR : {}", s.cell_label(here)), GREEN);
    let batt = arms::orbital_battery(&g.data, &s.galaxy, s.galaxy.seed, here, t);
    fb.text(
        1,
        5,
        &format!(
            "BATTERIES ORBITALES : {} CANONS, {:.0} KM DE PORTEE{}",
            batt.guns,
            batt.range / 1000.0,
            if s.here_besieged().is_some() {
                "   BLOCUS EN COURS : PRIX +30 %"
            } else {
                ""
            }
        ),
        if s.here_besieged().is_some() {
            RED
        } else {
            GREEN
        },
    );
    // Fronts of the sector: what the player knows (own sector always, the foe only if spied).
    let fx = &s.military.effects;
    let fronts = arms::fronts_with(&g.data, &s.galaxy, s.galaxy.seed, here, t, fx);
    if fronts.is_empty() {
        fb.text(1, 7, "AUCUN FRONT : LE SECTEUR EST EN PAIX", DIM);
    }
    for (k, f) in fronts.iter().take(3).enumerate() {
        let known = s.military.knows(f.foe, t);
        let foe = if known {
            format!("{:.0} ({})", f.foe_strength, f.foe_doctrine.label())
        } else {
            "? (ESPIONNEZ POUR LE SAVOIR)".to_string()
        };
        fb.text(
            1,
            7 + k * 3,
            &format!(
                "FRONT CONTRE {}   {}",
                s.cell_label(f.foe).chars().take(40).collect::<String>(),
                if f.own_wins() {
                    "NOUS TENONS"
                } else {
                    "NOUS PLIONS"
                }
            ),
            if f.own_wins() { GREEN } else { RED },
        );
        fb.text(
            3,
            8 + k * 3,
            &format!("NOUS {:.0} ({})", f.own_strength, f.own_doctrine.label()),
            GREEN,
        );
        fb.text(
            3,
            9 + k * 3,
            &format!(
                "EUX {foe}   GUERRE EN COURS DEPUIS {} PERIODE(S)",
                f.age + 1
            ),
            if known { GREEN } else { DIM },
        );
    }
    fb.text(1, 17, "BLOCUS A PORTEE", AMBER);
    if s.military.sieges.is_empty() {
        fb.text(3, 18, "AUCUN SECTEUR VOISIN N'EST ASSIEGE", DIM);
    }
    for (k, sg) in s.military.sieges.iter().take(4).enumerate() {
        fb.text(
            3,
            18 + k,
            &format!(
                "{} ASSIEGE PAR {}",
                s.cell_label(sg.cell).chars().take(40).collect::<String>(),
                sg.besieger
                    .map_or("UNE FLOTTE SANS DRAPEAU".to_string(), |f| g.data.factions
                        [f]
                        .name
                        .to_uppercase()),
            ),
            RED,
        );
    }
    // The people selected: war, peace, crimes.
    let x = 100;
    let f = g.fac_sel.min(g.data.factions.len().saturating_sub(1));
    let fac = &g.data.factions[f];
    fb.text(
        x,
        4,
        &format!("PEUPLE : {}   (GAUCHE/DROITE)", fac.name.to_uppercase()),
        fac.color,
    );
    let status = if s.ceasefire_with(f) {
        ("CESSEZ-LE-FEU EN VIGUEUR", GREEN)
    } else if s.at_war_with(f) {
        ("EN GUERRE CONTRE VOUS", RED)
    } else {
        ("PAS DE GUERRE", DIM)
    };
    fb.text(x, 5, status.0, status.1);
    fb.text(
        x,
        6,
        &format!(
            "DOCTRINE {}   VAISSEAUX DETRUITS {:.0} CR   CRIMES DE GUERRE {}",
            arms::faction_doctrine(&g.data, Some(f)).label(),
            s.military.destroyed_value(f),
            s.military.crimes_against(f)
        ),
        GREEN,
    );
    if s.at_war_with(f) {
        button(
            fb,
            g,
            x,
            7,
            44,
            &format!(
                "DEMANDER LA PAIX : {:.0} CR (P)",
                s.reparations_due(f).max(2000.0)
            ),
            W_PEACE,
        );
    }
    for (k, c) in s.military.crimes.iter().rev().take(3).enumerate() {
        fb.text(x, 9 + k, &format!("CRIME : {}", c.what.to_uppercase()), RED);
    }
    // Mercenary contracts.
    fb.text(x, 13, "CONTRATS DE MERCENAIRES (A QUAI)", AMBER);
    let offers = s.merc_offers();
    if offers.is_empty() || s.docked_station().is_err() {
        fb.text(
            x,
            14,
            "AUCUN : IL FAUT UN SECTEUR EN GUERRE ET UN QUAI",
            DIM,
        );
    }
    if s.docked_station().is_ok() {
        for (k, o) in offers.iter().take(2).enumerate() {
            button(
                fb,
                g,
                x,
                14 + k,
                60,
                &format!(
                    "ENGAGER LES NAVIRES LIBRES : {:.0} CR/J, {:.0} J (M)",
                    o.pay_day, o.days
                ),
                W_CONTRACT + k,
            );
        }
    }
    for (k, m) in s.military.mercs.iter().take(2).enumerate() {
        fb.text(
            x,
            16 + k,
            &format!(
                "CONTRAT : {} NAVIRES, FIN DANS {:.0} J",
                m.ships.len(),
                ((m.ends - t) / 86_400.0).max(0.0)
            ),
            GREEN,
        );
    }
    // Spies.
    let y0 = 25;
    fb.text(1, y0, "AGENTS SECRETS", AMBER);
    button(
        fb,
        g,
        20,
        y0,
        34,
        &format!("RECRUTER {:.0} CR (R)", arms::AGENT_COST),
        W_RECRUIT,
    );
    if s.military.agents.is_empty() {
        fb.text(3, y0 + 1, "AUCUN AGENT : RECRUTEZ-EN UN A QUAI", DIM);
    }
    let sel = g.war_agent.min(s.military.agents.len().saturating_sub(1));
    for (k, a) in s.military.agents.iter().enumerate().take(6) {
        let col = match a.state {
            AgentState::Infiltrated => GREEN,
            AgentState::Idle => AMBER,
            _ => RED,
        };
        let r = g.hit_cells(0, y0 + 1 + k, 80, 1, Act::Call(Game::click_agent, k), None);
        fb.text(
            1,
            y0 + 1 + k,
            &format!(
                "{}{:<22.22} TALENT {:>3.0}%  {:<9} COUVERTURE {:>3.0}%  MISSIONS {:>2}  {}",
                if k == sel { ">" } else { " " },
                a.name,
                a.skill * 100.0,
                a.state.label(),
                a.cover * 100.0,
                a.ops,
                if a.state == AgentState::Infiltrated {
                    s.cell_label(a.cell).chars().take(30).collect::<String>()
                } else {
                    String::new()
                }
            ),
            if r { WHITE } else { col },
        );
    }
    let ty = y0 + 8;
    let target = g.war_cells.get(g.war_target).copied();
    button(
        fb,
        g,
        1,
        ty,
        70,
        &format!(
            "SECTEUR VISE (C) : {}",
            target.map_or("AUCUN SECTEUR EN GUERRE A PORTEE".to_string(), |c| s
                .cell_label(c)
                .chars()
                .take(44)
                .collect())
        ),
        W_CELL,
    );
    button(
        fb,
        g,
        1,
        ty + 1,
        22,
        &format!("INFILTRER (I) {:.0}", arms::INFILTRATE_COST),
        W_INFILTRATE,
    );
    button(fb, g, 25, ty + 1, 22, "RAPPELER (W)", W_WITHDRAW);
    button(
        fb,
        g,
        49,
        ty + 1,
        34,
        "DRONE DE RECONNAISSANCE (D)",
        W_DRONE,
    );
    button(fb, g, 1, ty + 2, 20, "AGENT SUIVANT (A)", W_AGENT);
    for (k, op) in SPY_OPS.iter().enumerate() {
        button(
            fb,
            g,
            1 + k * 26,
            ty + 4,
            25,
            &format!("{} {:.0} ({})", op.label(), op.cost(), k + 1),
            W_OP + k,
        );
    }
    fb.text(
        1,
        fb.rows() - 3,
        "R RECRUTER  A AGENT  C SECTEUR  I INFILTRER  1-4 OPERATION  W RAPPELER  D DRONE  P PAIX  M CONTRAT  G/ECHAP RETOUR",
        DIM,
    );
}

impl Game {
    pub fn click_agent(&mut self, k: usize) {
        self.war_agent = k;
    }

    /// Squadron controls of the fleet screen: order of the selected ship (O), resupply (R),
    /// repairs (V), judgement of a prize crew (1 to 4).
    pub fn squad_click(&mut self, code: usize) {
        let i = self.ship_sel2;
        let Some(ship) = self.sim.fleet.get(i) else {
            self.say("AUCUN VAISSEAU");
            return;
        };
        let (id, order) = (ship.id, ship.order);
        let res: Result<String, &'static str> = match code {
            0 => {
                let cur = ORDERS.iter().position(|o| *o == order).unwrap_or(0);
                let next = ORDERS[(cur + 1) % ORDERS.len()];
                self.sim
                    .set_order(i, next)
                    .map(|()| format!("ORDRE : {}", next.label()))
            }
            1 => self
                .sim
                .resupply_escorts()
                .map(|kg| format!("ESCADRE RAVITAILLEE : {kg:.0} KG DE MUNITIONS")),
            2 => self
                .sim
                .repair_squadron()
                .map(|c| format!("ESCADRE REPAREE : {c:.0} CR")),
            3 => self
                .sim
                .prize_enlist(id)
                .map(|()| "EQUIPAGE ENROLE".to_string()),
            4 => self
                .sim
                .prize_release(id)
                .map(|()| "PRISONNIERS LIBERES".to_string()),
            5 => self
                .sim
                .prize_ransom(id)
                .map(|c| format!("RANCON : {c:.0} CR")),
            6 => self
                .sim
                .prize_execute(id)
                .map(|()| "PRISONNIERS EXECUTES : CRIME DE GUERRE".to_string()),
            _ => return,
        };
        match res {
            Ok(m) => self.say(m),
            Err(e) => self.say(e.to_uppercase()),
        }
    }
}

/// The squadron panel at the bottom of the fleet screen (nothing when the fleet is empty).
pub fn draw_squad(fb: &mut Framebuffer, g: &Game, y: usize) {
    let s = &g.sim;
    if s.fleet.is_empty() {
        return;
    }
    let count = |o: FleetOrder| s.squadron(o).count();
    fb.text(
        1,
        y,
        &format!(
            "ESCADRE  DEFENDRE {}  SUIVRE {}  ATTAQUER {}  PATROUILLER {}  MERCENAIRES {}   ECRAN {:.0} %",
            count(FleetOrder::Defend),
            count(FleetOrder::Follow),
            count(FleetOrder::Attack),
            count(FleetOrder::Patrol),
            count(FleetOrder::Mercenary),
            s.escort_screen() * 100.0
        ),
        AMBER,
    );
    let Some(ship) = s.fleet.get(g.ship_sel2.min(s.fleet.len() - 1)) else {
        return;
    };
    fb.text(
        1,
        y + 1,
        &format!(
            "{}: ORDRE {}  CONDITION {:.0} %  MUNITIONS {:.0} %{}",
            ship.name
                .to_uppercase()
                .chars()
                .take(24)
                .collect::<String>(),
            ship.order.label(),
            ship.condition * 100.0,
            ship.ammo * 100.0,
            if ship.prize {
                format!(
                    "  PRISE : EQUIPAGE {} DONT {} PRISONNIERS, LOYAUTE {:.0} %",
                    ship.crew,
                    ship.prisoners,
                    ship.loyalty * 100.0
                )
            } else {
                String::new()
            }
        ),
        GREEN,
    );
    squad_button(fb, g, 1, y + 2, 24, "ORDRE SUIVANT (O)", 0);
    squad_button(fb, g, 27, y + 2, 22, "RAVITAILLER (R)", 1);
    squad_button(fb, g, 51, y + 2, 22, "REPARER (V)", 2);
    if ship.prize && ship.prisoners > 0 {
        squad_button(fb, g, 1, y + 3, 16, "ENROLER (1)", 3);
        squad_button(fb, g, 19, y + 3, 16, "LIBERER (2)", 4);
        squad_button(fb, g, 37, y + 3, 16, "RANCON (3)", 5);
        squad_button(fb, g, 55, y + 3, 20, "EXECUTER (4)", 6);
    }
}

fn squad_button(
    fb: &mut Framebuffer,
    g: &Game,
    col: usize,
    row: usize,
    _w: usize,
    label: &str,
    code: usize,
) {
    let w = label.chars().count() + 2;
    let rc = (
        col as i32 * 8,
        row as i32 * 10,
        (col + w) as i32 * 8,
        row as i32 * 10 + 11,
    );
    crate::proto_view::button(fb, g, rc, label, Act::Call(Game::squad_click, code));
}
