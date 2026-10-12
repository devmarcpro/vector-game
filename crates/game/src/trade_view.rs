//! The brokerage screen (D at a station): rumours, arbitrage, cartels, tenders, insurance,
//! securities, sales, regular lines and the economic board (the last five live in `eco_view`),
//! nine tabs chosen with the arrows or by clicking their names.
use crate::game::{Game, MarketTab};
use crate::hud::{AMBER, DIM, GREEN, UP, WHITE};
use crate::mouse::Act;
use vector_render::Framebuffer;
use vector_sim::bourse::DAY;
use winit::keyboard::KeyCode;

pub const TABS: [&str; 9] = [
    "RUMEURS",
    "ARBITRAGE",
    "CARTELS",
    "APPELS D'OFFRES",
    "ASSURANCE",
    "TITRES",
    "VENTES",
    "LIGNES",
    "ECONOMIE",
];

#[derive(Default)]
pub struct TradeUi {
    pub tab: usize,
    pub sel: usize,
    /// Bid as a percentage of the ceiling (steps of 5, default 60).
    pub bid: usize,
    /// Caches of the economy tabs.
    pub eco: crate::eco_view::EcoUi,
}

impl Game {
    pub fn click_trade_tab(&mut self, i: usize) {
        self.trade_ui.tab = i;
        self.trade_ui.sel = 0;
    }

    pub fn click_trade_row(&mut self, i: usize) {
        self.trade_ui.sel = i;
    }

    fn trade_rows(&self) -> usize {
        match self.trade_ui.tab {
            2 => self.data.cartels.len(),
            3 => self.sim.offered_tenders().len(),
            5..=8 => self.eco_rows(),
            _ => 0,
        }
    }

    pub fn trade_key(&mut self, code: KeyCode) {
        let rows = self.trade_rows();
        match code {
            KeyCode::ArrowLeft => {
                self.click_trade_tab((self.trade_ui.tab + TABS.len() - 1) % TABS.len())
            }
            KeyCode::ArrowRight | KeyCode::Tab => {
                self.click_trade_tab((self.trade_ui.tab + 1) % TABS.len())
            }
            KeyCode::PageUp if rows > 0 => self.trade_ui.sel = self.trade_ui.sel.saturating_sub(10),
            KeyCode::PageDown if rows > 0 => {
                self.trade_ui.sel = (self.trade_ui.sel + 10).min(rows - 1)
            }
            KeyCode::ArrowUp if rows > 0 => {
                self.trade_ui.sel = (self.trade_ui.sel + rows - 1) % rows;
            }
            KeyCode::ArrowDown if rows > 0 => self.trade_ui.sel = (self.trade_ui.sel + 1) % rows,
            KeyCode::Equal | KeyCode::NumpadAdd => {
                self.trade_ui.bid = (self.trade_ui.bid.max(1) + 1).min(30);
            }
            KeyCode::Minus | KeyCode::NumpadSubtract => {
                self.trade_ui.bid = self.trade_ui.bid.max(2) - 1;
            }
            KeyCode::Enter | KeyCode::Space => self.trade_action(),
            KeyCode::Escape | KeyCode::KeyD => self.market_tab = MarketTab::Open,
            _ => {}
        }
    }

    pub fn bid_fraction(&self) -> f64 {
        0.05 * self.trade_ui.bid.max(1) as f64
    }

    fn trade_action(&mut self) {
        let sel = self.trade_ui.sel;
        let r: Result<String, &str> = match self.trade_ui.tab {
            2 => {
                if self.sim.trade.seats.contains(&sel) {
                    self.sim
                        .leave_cartel(sel)
                        .map(|()| "SIEGE CEDE".to_string())
                } else {
                    self.sim
                        .join_cartel(sel)
                        .map(|()| "SIEGE ACHETE".to_string())
                }
            }
            3 => {
                let offers = self.sim.offered_tenders();
                match offers.get(sel) {
                    Some(o) => {
                        let price = o.ceiling * self.bid_fraction();
                        self.sim
                            .bid_tender(o.id, price)
                            .map(|()| format!("MARCHE REMPORTE A {price:.1} CR/KG : VOIR CONTRATS"))
                    }
                    None => Err("aucune offre choisie"),
                }
            }
            4..=8 => {
                self.eco_enter();
                return;
            }
            _ => return,
        };
        match r {
            Ok(m) => self.say(m),
            Err(e) => self.say(e.to_uppercase()),
        }
    }
}

pub fn draw(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let ui = &g.trade_ui;
    fb.text(
        1,
        1,
        &format!("== COURTAGE : {} ==", TABS[ui.tab.min(TABS.len() - 1)]),
        AMBER,
    );
    fb.text(18, 1, &format!("CREDITS {:.0}", s.credits), GREEN);
    // The tabs are the words of the bar of the market screen (`ui::market_tab_bar`).
    match ui.tab {
        0 => draw_rumours(fb, g),
        1 => draw_arbitrage(fb, g),
        2 => draw_cartels(fb, g),
        3 => draw_tenders(fb, g),
        4 => crate::eco_view::draw_insurance(fb, g),
        5 => crate::eco_view::draw_titres(fb, g),
        6 => crate::eco_view::draw_ventes(fb, g),
        7 => crate::eco_view::draw_lignes(fb, g),
        _ => crate::eco_view::draw_economie(fb, g),
    }
    for (i, m) in g.messages.iter().rev().take(1).enumerate() {
        fb.text(1, 36 + i, m, AMBER);
    }
}

fn draw_rumours(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    fb.text(
        1,
        5,
        "ON MURMURE A LA STATION... (CERTAINES RUMEURS SONT FAUSSES)",
        DIM,
    );
    let list = s.rumours();
    if list.is_empty() {
        fb.text(3, 7, "RIEN DE NEUF A L'HORIZON.", DIM);
    }
    for (i, r) in list.iter().take(14).enumerate() {
        let def = &s.data.world_events[r.def];
        let eta = (r.start - s.time) / DAY;
        fb.text(
            1,
            7 + i * 2,
            &format!(
                "DANS {eta:>4.1} J  {:<34.34}  SOURCE {:>3.0} %",
                def.name.to_uppercase(),
                r.confidence * 100.0
            ),
            if r.confidence > 0.7 { UP } else { GREEN },
        );
        fb.text(5, 8 + i * 2, &def.text, DIM);
    }
}

fn draw_arbitrage(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    fb.text(
        1,
        5,
        "ACHETEZ ICI, REVENDEZ LA-BAS : LES PLUS GROS ECARTS DE PRIX AUTOUR DE VOUS",
        DIM,
    );
    fb.text(
        1,
        6,
        "BIEN            DESTINATION                 DIST   ICI     LA-BAS  MARGE",
        DIM,
    );
    for (i, a) in s.arbitrage(60.0, 18).iter().enumerate() {
        fb.text(
            1,
            7 + i,
            &format!(
                "{:<15.15} {:<26.26} {:>4.0}  {:>7.1} {:>7.1}  {:>+4.0}%",
                s.data.resources[a.res].name.to_uppercase(),
                a.star.to_uppercase(),
                a.dist_ly,
                a.here,
                a.there,
                a.margin() * 100.0
            ),
            if a.margin() > 0.3 { UP } else { GREEN },
        );
    }
}

fn draw_cartels(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    fb.text(
        1,
        5,
        &format!(
            "UN SIEGE COUTE {:.0} CR ; IL RAPPORTE CHAQUE MOIS QUAND LE CARTEL TIENT LES PRIX ICI. RISQUE : ENQUETE ANTITRUST.",
            vector_sim::trading::SEAT_PRICE
        ),
        DIM,
    );
    for (c, k) in s.data.cartels.iter().enumerate() {
        let active = s.cartel_here(c);
        let seat = s.trade.seats.contains(&c);
        let sel = c == g.trade_ui.sel;
        let hov = g.hit_cells(
            1,
            7 + c * 3,
            90,
            1,
            Act::Call(Game::click_trade_row, c),
            None,
        );
        fb.text(
            1,
            7 + c * 3,
            &format!(
                "{}{:<32.32} {}  {}",
                if sel { ">" } else { " " },
                k.name.to_uppercase(),
                if active { "ACTIF ICI " } else { "INACTIF   " },
                if seat { "[VOTRE SIEGE]" } else { "" }
            ),
            if sel {
                AMBER
            } else if hov {
                WHITE
            } else if active {
                UP
            } else {
                GREEN
            },
        );
        fb.text(
            3,
            8 + c * 3,
            &format!("+{:.0} % SUR : {}", k.markup * 100.0, k.goods.join(", ")),
            DIM,
        );
    }
    fb.text(1, 26, "[ENTREE] ACHETER / CEDER LE SIEGE", GREEN);
}

fn draw_tenders(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    fb.text(
        1,
        5,
        &format!(
            "MISE : {:.0} % DU PLAFOND (+/-). LE MOINS CHER GAGNE ; LIVRAISON SOUS {:.0} JOURS OU PENALITE.",
            g.bid_fraction() * 100.0,
            vector_sim::trading::TENDER_DAYS
        ),
        DIM,
    );
    let offers = s.offered_tenders();
    fb.text(1, 7, "OFFERTS PAR CETTE STATION", AMBER);
    for (i, o) in offers.iter().enumerate() {
        let sel = i == g.trade_ui.sel;
        let hov = g.hit_cells(1, 8 + i, 80, 1, Act::Call(Game::click_trade_row, i), None);
        fb.text(
            1,
            8 + i,
            &format!(
                "{}{:<16.16} {:>6.0} KG   PLAFOND {:>7.1} CR/KG   VOTRE MISE {:>7.1}",
                if sel { ">" } else { " " },
                s.data.resources[o.res].name.to_uppercase(),
                o.kg,
                o.ceiling,
                o.ceiling * g.bid_fraction()
            ),
            if sel {
                AMBER
            } else if hov {
                WHITE
            } else {
                GREEN
            },
        );
    }
    fb.text(
        1,
        8 + offers.len().max(1) + 1,
        "UN MARCHE REMPORTE DEVIENT UN CONTRAT : IL SE LIVRE ET SE SUIT DANS CONTRATS.",
        DIM,
    );
    fb.text(1, 30, "[ENTREE] MISER", GREEN);
}
