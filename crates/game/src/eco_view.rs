//! The tabs of the brokerage screen (D at a station) that belong to the economy: insurance with
//! its real premium, securities (bonds, funds, options), sales (auction, flea market, ads),
//! the regular lines of cargo ships and the economic board of the jurisdictions around, with
//! charts drawn in characters. Everything is a button: click, or use the arrows and Enter.
use crate::game::Game;
use crate::hud::{AMBER, DIM, GREEN, RED, UP, WHITE};
use crate::mouse::Act;
use std::cell::RefCell;
use vector_render::Framebuffer;
use vector_sim::bourse::DAY;
use vector_sim::lanes::LineView;
use vector_sim::macroeco::Board;

type BoardCache = Option<((i64, [i64; 3]), Board)>;
type LanesCache = Option<(i64, Vec<LineView>, Vec<(String, f64)>)>;
type JurisdictionCache = Option<(i64, Vec<(String, [i64; 3])>)>;

/// Cached figures of the economy tabs: they are computed once a day (or when the choice changes),
/// not every frame.
#[derive(Default)]
pub struct EcoUi {
    board: RefCell<BoardCache>,
    lanes: RefCell<LanesCache>,
    jurisdictions: RefCell<JurisdictionCache>,
}

fn money(v: f64) -> String {
    if v.abs() >= 1.0e6 {
        format!("{:.2}M", v / 1.0e6)
    } else if v.abs() >= 1.0e4 {
        format!("{:.1}K", v / 1.0e3)
    } else {
        format!("{v:.0}")
    }
}

/// A clickable text; returns the column after it.
fn button(fb: &mut Framebuffer, g: &Game, x: usize, y: usize, label: &str, id: usize) -> usize {
    let hov = g.hit_cells(x, y, label.len(), 1, Act::Call(Game::click_eco, id), None);
    fb.text(x, y, label, if hov { WHITE } else { GREEN });
    x + label.len() + 2
}

/// A line chart in characters: `h` rows, `w` columns, the extremes written at the left.
pub fn chart(fb: &mut Framebuffer, x: usize, y: usize, w: usize, h: usize, series: &[f64], c: u32) {
    if series.len() < 2 || w < 4 || h < 2 {
        return;
    }
    let (lo, hi) = series
        .iter()
        .fold((f64::MAX, f64::MIN), |(l, hh), &v| (l.min(v), hh.max(v)));
    let span = (hi - lo).max(1e-12);
    let label = |v: f64| {
        if v.abs() >= 100.0 {
            format!("{v:>6.0}")
        } else {
            format!("{v:>6.2}")
        }
    };
    fb.text(x, y, &label(hi), DIM);
    fb.text(x, y + h - 1, &label(lo), DIM);
    let left = x + 7;
    let cols = w.saturating_sub(7).max(2);
    for k in 0..cols {
        let f = k as f64 * (series.len() - 1) as f64 / (cols - 1) as f64;
        let i = f.floor() as usize;
        let j = (i + 1).min(series.len() - 1);
        let v = series[i] + (series[j] - series[i]) * (f - i as f64);
        let row = ((hi - v) / span * (h - 1) as f64).round() as usize;
        for r in 0..h {
            let ch = if r == row {
                "*"
            } else if r == h - 1 {
                "."
            } else {
                " "
            };
            if ch != " " {
                fb.text(left + k, y + r, ch, if r == row { c } else { DIM });
            }
        }
    }
}

fn gauge(v: f64, width: usize) -> String {
    let n = ((v.clamp(0.0, 1.0)) * width as f64).round() as usize;
    format!("[{}{}]", "=".repeat(n), " ".repeat(width - n))
}

impl Game {
    /// Number of rows the arrows move through on each economy tab.
    pub fn eco_rows(&self) -> usize {
        match self.trade_ui.tab {
            5 | 6 => self.data.resources.len(),
            7 => self.eco_lines().len(),
            8 => self.eco_jurisdictions().len(),
            _ => 0,
        }
    }

    fn eco_lines(&self) -> Vec<LineView> {
        let day = (self.sim.time / DAY).floor() as i64;
        let mut c = self.trade_ui.eco.lanes.borrow_mut();
        if c.as_ref().is_none_or(|(d, _, _)| *d != day) {
            let hubs: Vec<(String, f64)> = self
                .sim
                .galaxy
                .nearby(self.sim.here_pos, 25.0)
                .into_iter()
                .filter(|s| s.distance_to(self.sim.here_pos) > 0.5)
                .filter(|s| {
                    !vector_sim::lanes::lines_at(&self.data, &self.sim.galaxy, s.pos).is_empty()
                })
                .take(4)
                .map(|s| {
                    (
                        s.header.name.to_uppercase(),
                        s.distance_to(self.sim.here_pos),
                    )
                })
                .collect();
            *c = Some((day, self.sim.lines_here(), hubs));
        }
        c.as_ref().map(|(_, v, _)| v.clone()).unwrap_or_default()
    }

    fn eco_jurisdictions(&self) -> Vec<(String, [i64; 3])> {
        let day = (self.sim.time / DAY).floor() as i64;
        let mut c = self.trade_ui.eco.jurisdictions.borrow_mut();
        if c.as_ref().is_none_or(|(d, _)| *d != day) {
            *c = Some((day, self.sim.jurisdictions()));
        }
        c.as_ref().map(|(_, v)| v.clone()).unwrap_or_default()
    }

    fn eco_board(&self, cell: [i64; 3]) -> Board {
        let day = (self.sim.time / DAY).floor() as i64;
        let res = self.bourse_sel.min(self.data.resources.len() - 1);
        let mut c = self.trade_ui.eco.board.borrow_mut();
        if c.as_ref()
            .is_none_or(|(k, b)| *k != (day, cell) || b.price_res != res)
        {
            *c = Some(((day, cell), self.sim.eco_board(cell, res)));
        }
        c.as_ref().map(|(_, b)| b.clone()).expect("board")
    }

    /// Run the action of a button of the economy tabs.
    pub fn click_eco(&mut self, id: usize) {
        let res = self.trade_ui.sel.min(self.data.resources.len() - 1);
        let r: Result<String, &str> = match id {
            100 | 101 => {
                let amount = if id == 100 { 1000.0 } else { 5000.0 };
                self.sim
                    .buy_bond(amount)
                    .map(|u| format!("OBLIGATIONS ACHETEES : {u:.0} UNITES DE MONNAIE LOCALE"))
            }
            110..=119 => self
                .sim
                .sell_bond(id - 110)
                .map(|c| format!("OBLIGATION VENDUE {c:.0} CR")),
            120..=125 => {
                let k = (id - 120) / 2;
                if (id - 120).is_multiple_of(2) {
                    self.sim
                        .buy_fund(k, 1000.0)
                        .map(|u| format!("{u:.2} PARTS ACHETEES"))
                } else {
                    self.sim
                        .sell_fund(k)
                        .map(|c| format!("PARTS VENDUES {c:.0} CR"))
                }
            }
            130 | 131 => self
                .sim
                .buy_option(res, 100.0, 30.0, id == 130)
                .map(|_| "OPTION ACHETEE (PRIME PAYEE, PERTE LIMITEE A LA PRIME)".to_string()),
            200..=203 => {
                let st = self.sim.docked_station();
                match st {
                    Ok(st) => {
                        let lots = self.sim.auction_lots(st, self.sim.time);
                        match lots.get(id - 200) {
                            Some(l) => {
                                let bid = (l.value * self.bid_fraction()).round();
                                self.sim.place_bid(id - 200, bid).map(|()| {
                                    format!(
                                        "MISE DE {bid:.0} CR PLACEE (BLOQUEE JUSQU'A LA CLOTURE)"
                                    )
                                })
                            }
                            None => Err("lot inconnu"),
                        }
                    }
                    Err(_) => Err("non amarre"),
                }
            }
            210..=215 => self.sim.flea_buy(id - 210).map(|(kg, fake)| {
                if fake {
                    format!("CONTREFACON : {kg:.1} KG DE VRAI SEULEMENT")
                } else {
                    format!("ACHETE : {kg:.0} KG")
                }
            }),
            220..=225 => self.sim.flea_appraise(id - 220).map(|fake| {
                if fake {
                    "EXPERTISE : CONTREFACON !".to_string()
                } else {
                    "EXPERTISE : LOT AUTHENTIQUE".to_string()
                }
            }),
            230 => {
                let kg = self.sim.ship.cargo.kg[res];
                self.sim
                    .flea_sell(res, kg)
                    .map(|c| format!("VENDU AUX PUCES POUR {c:.0} CR"))
            }
            240 => self.sim.run_ad(res, 2000.0).map(|s| {
                format!(
                    "CAMPAGNE LANCEE : DEMANDE +{:.1} % ICI POUR 20 JOURS",
                    s * 100.0
                )
            }),
            250..=254 => self
                .sim
                .hire_guide(id - 250)
                .map(|c| format!("GUIDE ENGAGE POUR {c:.0} CR")),
            300..=304 => {
                let lines = self.eco_lines();
                let sel = self.trade_ui.sel.min(lines.len().saturating_sub(1));
                match lines
                    .get(sel)
                    .and_then(|lv| lv.convoys.get(id - 300).map(|c| (lv, c)))
                {
                    Some((lv, c)) => self
                        .sim
                        .finance_convoy(lv.line.id, c.k, 1000.0)
                        .map(|()| "CONVOI FINANCE : 1000 CR EN JEU".to_string()),
                    None => Err("convoi inconnu"),
                }
            }
            400..=408 => {
                self.trade_ui.sel = id - 400;
                return;
            }
            500 | 501 => self
                .sim
                .buy_insurance(id == 501)
                .map(|p| format!("POLICE SOUSCRITE POUR {p:.0} CR")),
            502 => {
                let best = (0..self.data.resources.len()).max_by(|a, b| {
                    let v = |r: usize| self.sim.ship.cargo.kg[r] * self.data.resources[r].value;
                    v(*a).total_cmp(&v(*b))
                });
                match best {
                    Some(r) => {
                        let kg = self.sim.ship.cargo.kg[r];
                        self.sim
                            .declare_loss(r, kg)
                            .map(|p| format!("SINISTRE INDEMNISE : {p:.0} CR"))
                    }
                    None => Err("rien en soute"),
                }
            }
            _ => return,
        };
        match r {
            Ok(m) => self.say(m),
            Err(e) => self.say(e.to_uppercase()),
        }
        self.trade_ui.eco.board.borrow_mut().take();
        self.trade_ui.eco.lanes.borrow_mut().take();
    }

    /// Enter on an economy tab: its main action.
    pub fn eco_enter(&mut self) {
        let id = match self.trade_ui.tab {
            4 => 500,
            5 => 100,
            6 => 200,
            7 => 300,
            _ => return,
        };
        self.click_eco(id);
    }
}

pub fn draw_insurance(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    use vector_sim::trading::{COVER, INSURANCE_DAYS, LOADING};
    let risk = s.risk_here();
    fb.text(
        1,
        5,
        &format!(
            "PRIME = VALEUR x RISQUE REEL x {LOADING:.1} (MARGE DE L'ASSUREUR) x VOTRE DOSSIER ; {INSURANCE_DAYS:.0} JOURS ; REMBOURSE {:.0} % A LA DESTRUCTION.",
            COVER * 100.0
        ),
        DIM,
    );
    fb.text(
        1,
        7,
        "RISQUES ICI (CHANCE PAR MOIS DE PERDRE VAISSEAU OU SOUTE)",
        AMBER,
    );
    let rows = [
        ("ZONE (AGRESSIVITE DU PEUPLE)", risk.zone, false),
        ("PIRATES", risk.pirates, false),
        ("GUERRE", f64::from(u8::from(risk.war)), risk.war),
        ("BLOCUS", f64::from(u8::from(risk.siege)), risk.siege),
        (
            "EPIDEMIE (PORTS FERMES)",
            (risk.sick / 0.1).clamp(0.0, 1.0),
            risk.sick > 0.02,
        ),
        (
            "CATASTROPHE",
            f64::from(u8::from(risk.catastrophe)),
            risk.catastrophe,
        ),
    ];
    for (i, (name, v, hot)) in rows.iter().enumerate() {
        fb.text(
            3,
            8 + i,
            &format!("{name:<30} {} {:>3.0} %", gauge(*v, 20), v * 100.0),
            if *hot { RED } else { GREEN },
        );
    }
    let q = s.insurance_quote(false);
    let qh = s.insurance_quote(true);
    fb.text(
        1,
        15,
        &format!(
            "RISQUE TOTAL {:.1} % PAR MOIS   DOSSIER x{:.2} ({} SINISTRES SUR L'AN, {} FRAUDES)",
            q.odds * 100.0,
            q.history,
            s.recent_claims(),
            s.trade.frauds
        ),
        AMBER,
    );
    fb.text(
        1,
        17,
        &format!(
            "SOUTE {} CR  ->  PRIME {} CR       SOUTE + COQUE ({} CR)  ->  PRIME {} CR",
            money(q.cargo),
            money(q.premium),
            money(qh.hull),
            money(qh.premium)
        ),
        GREEN,
    );
    let x = button(fb, g, 1, 19, "[ASSURER LA SOUTE]", 500);
    let x = button(fb, g, x, 19, "[ASSURER SOUTE ET COQUE]", 501);
    button(fb, g, x, 19, "[DECLARER LA SOUTE PERDUE]", 502);
    if s.insured() {
        fb.text(
            1,
            21,
            &format!(
                "POLICE ACTIVE : {:.1} JOURS RESTANTS, SOUTE COUVERTE {} CR, COQUE {} CR",
                (s.trade.insured_until - s.time) / DAY,
                money(s.trade.covered),
                money(s.trade.hull_covered)
            ),
            UP,
        );
    } else {
        fb.text(1, 21, "AUCUNE POLICE EN COURS.", RED);
    }
    fb.text(
        1,
        23,
        "UNE DECLARATION DE PERTE EST CONTROLEE : PLUS DE SINISTRES AU DOSSIER ET MOINS DE DANGER ICI = PLUS D'ENQUETES.",
        DIM,
    );
    fb.text(
        1,
        24,
        "UNE FRAUDE ANNULE LA POLICE, COUTE TROIS MOIS DE PRIME, UNE AMENDE ET HAUSSE DURABLEMENT VOTRE PRIME.",
        DIM,
    );
}

pub fn draw_titres(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let sec = s.local_sector();
    fb.text(
        1,
        5,
        &format!(
            "OBLIGATIONS D'ETAT : {} A 6 MOIS, RENDEMENT {:.1} % PAR AN (TAUX {:.1} %, DETTE {:.0} %). DEFAUT PARTIEL EN CAS DE KRACH PROFOND.",
            vector_sim::macroeco::currency_name(&g.data, sec.faction),
            sec.sovereign_yield() * 100.0,
            sec.policy_rate * 100.0,
            sec.debt_ratio * 100.0
        ),
        DIM,
    );
    let x = button(fb, g, 1, 6, "[ACHETER 1000 CR]", 100);
    button(fb, g, x, 6, "[ACHETER 5000 CR]", 101);
    for (i, b) in s.trade.folio.bonds.iter().take(4).enumerate() {
        fb.text(
            3,
            7 + i,
            &format!(
                "OBLIGATION {}  ECHEANCE DANS {:>4.0} J  VALEUR {:>8} CR (PAYE {})",
                i + 1,
                (b.maturity - s.time) / DAY,
                money(s.bond_market_value(b)),
                money(b.paid)
            ),
            GREEN,
        );
        button(fb, g, 80, 7 + i, "[VENDRE]", 110 + i);
    }
    fb.text(1, 12, "FONDS (FRAIS 0,5 % + 0,8 % PAR AN)", AMBER);
    for (k, (name, text)) in vector_sim::securities::FUNDS.iter().enumerate() {
        let price = s.fund_price(k, s.time);
        let units = s.fund_units(k);
        fb.text(
            3,
            13 + k * 2,
            &format!(
                "{name:<16} PART {price:>7.2}   DETENU {units:>8.2} = {:>8} CR",
                money(units * price)
            ),
            GREEN,
        );
        fb.text(5, 14 + k * 2, text, DIM);
        let x = button(fb, g, 80, 13 + k * 2, "[ACHETER 1000 CR]", 120 + 2 * k);
        button(fb, g, x, 13 + k * 2, "[VENDRE]", 121 + 2 * k);
    }
    let res = g.trade_ui.sel.min(g.data.resources.len() - 1);
    let r = &g.data.resources[res];
    let price = s.reference_price(s.here_pos, s.time, res);
    let prem = vector_sim::securities::option_premium_per_kg(r.value, price, 30.0) * 100.0;
    fb.text(
        1,
        18,
        &format!(
            "OPTIONS SUR {} (FLECHES : CHOISIR) PRIX {price:.2} CR/KG ; 100 KG A 30 J = PRIME {prem:.0} CR",
            r.name.to_uppercase()
        ),
        AMBER,
    );
    let x = button(fb, g, 1, 19, "[OPTION D'ACHAT : GAIN SI ELLE MONTE]", 130);
    button(fb, g, x, 19, "[OPTION DE VENTE : GAIN SI ELLE BAISSE]", 131);
    for (i, o) in s.trade.folio.options.iter().take(4).enumerate() {
        fb.text(
            3,
            20 + i,
            &format!(
                "{} {:<18.18} {:>5.0} KG  STRIKE {:>7.2}  ECHEANCE {:>4.0} J  VALEUR {:>7} CR",
                if o.call { "ACHAT" } else { "VENTE" },
                g.data.resources[o.res].name.to_uppercase(),
                o.kg,
                o.strike,
                (o.expiry - s.time) / DAY,
                money(s.option_value(o))
            ),
            GREEN,
        );
    }
    fb.text(
        1,
        25,
        &format!(
            "CONTRATS A TERME OUVERTS (BOURSE, B / N) : {}   VALEUR NETTE DU PORTEFEUILLE : {} CR   PATRIMOINE : {} CR",
            s.futures.len(),
            money(s.portfolio_value()),
            money(s.net_worth())
        ),
        DIM,
    );
}

pub fn draw_ventes(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let Ok(st) = s.docked_station() else {
        fb.text(1, 5, "NON AMARRE : LES VENTES SONT A LA STATION.", RED);
        return;
    };
    fb.text(
        1,
        5,
        &format!(
            "ENCHERES DE LA SEMAINE (SECOND PRIX : VOUS DONNEZ LE MAXIMUM, VOUS PAYEZ 5 % DE PLUS QUE LE MEILLEUR RIVAL). MISE : {:.0} % DE LA VALEUR (+/-)",
            g.bid_fraction() * 100.0
        ),
        DIM,
    );
    for (i, l) in s.auction_lots(st, s.time).iter().enumerate() {
        let open = s.time < l.close;
        let placed = s
            .trade
            .dealings
            .bids
            .iter()
            .any(|b| b.week == l.week && b.index == i && b.station == st);
        fb.text(
            3,
            6 + i,
            &format!(
                "LOT {} {:>5.0} KG {:<16.16} VALEUR {:>7} RESERVE {:>7} RIVAUX {} {}",
                i + 1,
                l.kg,
                g.data.resources[l.res].name.to_uppercase(),
                money(l.value),
                money(l.reserve),
                l.rivals.len(),
                if open {
                    format!("CLOT DANS {:.1} J", (l.close - s.time) / DAY)
                } else {
                    "CLOS".to_string()
                }
            ),
            if placed { UP } else { GREEN },
        );
        if open && !placed {
            button(
                fb,
                g,
                92,
                6 + i,
                &format!("[MISER {}]", money(l.value * g.bid_fraction())),
                200 + i,
            );
        } else if placed {
            fb.text(92, 6 + i, "[MISE PLACEE]", UP);
        }
    }
    let rate = vector_sim::auction::counterfeit_rate(
        &g.data,
        &s.galaxy,
        s.galaxy.seed,
        s.here_pos,
        s.time,
    );
    fb.text(
        1,
        11,
        &format!(
            "MARCHE AUX PUCES D'AUJOURD'HUI ({:.0} % DES BIENS DE LA REGION SONT DES COPIES : LES MARQUES EN SOUFFRENT)",
            rate * 100.0
        ),
        AMBER,
    );
    for (row, l) in s.flea_lots().iter().enumerate() {
        let known = s.flea_known(l.index);
        let market = s.reference_price(s.here_pos, s.time, l.res) * l.kg;
        fb.text(
            3,
            12 + row,
            &format!(
                "{:>3.0} KG {:<16.16} {:>6} CR ({:>+4.0} % / MARCHE) {}",
                l.kg,
                g.data.resources[l.res].name.to_uppercase(),
                money(l.price),
                (l.price / market.max(1e-9) - 1.0) * 100.0,
                if known && l.fake {
                    "CONTREFACON"
                } else if known {
                    "AUTHENTIQUE"
                } else {
                    ""
                }
            ),
            if known && l.fake { RED } else { GREEN },
        );
        let x = button(fb, g, 80, 12 + row, "[EXPERTISE]", 220 + l.index);
        button(fb, g, x, 12 + row, "[ACHETER]", 210 + l.index);
    }
    let res = g.trade_ui.sel.min(g.data.resources.len() - 1);
    let r = &g.data.resources[res];
    fb.text(
        1,
        19,
        &format!(
            "BIEN CHOISI (FLECHES) : {}   EN SOUTE {:.0} KG   AUX PUCES : {:.0} % DU PRIX, SANS TAXE",
            r.name.to_uppercase(),
            s.ship.cargo.kg[res],
            vector_sim::auction::FLEA_SELL * 100.0
        ),
        AMBER,
    );
    let x = button(fb, g, 1, 20, "[VENDRE TOUT AUX PUCES]", 230);
    button(fb, g, x, 20, "[CAMPAGNE DE PUBLICITE 2000 CR]", 240);
    let ads = s
        .trade
        .dealings
        .ads
        .iter()
        .filter(|a| a.end > s.time)
        .count();
    fb.text(
        1,
        22,
        &format!(
            "PUBLICITE : {ads} CAMPAGNE(S) EN COURS (DEMANDE LOCALE + PRIX DU BIEN) ; MARQUE DE VOTRE MAISON {:.0} % (+{:.1} % SUR VOS PRIX)",
            s.brand_at(s.time) * 100.0,
            s.brand_at(s.time) * 4.0
        ),
        DIM,
    );
    for (n, (i, c)) in s
        .contracts
        .iter()
        .enumerate()
        .filter(|(_, c)| c.pax > 0)
        .take(3)
        .enumerate()
    {
        let row = 27 + n;
        fb.text(
            3,
            row,
            &format!(
                "PASSAGERS : {} {} ({} ESCALES RESTANTES){}",
                c.pax,
                vector_sim::economy::PAX_KINDS[usize::from(c.pax_kind.min(5))].0,
                c.legs.len(),
                if c.guide { "  GUIDE ENGAGE" } else { "" }
            ),
            GREEN,
        );
        if !c.guide && i < 5 {
            button(fb, g, 80, row, "[ENGAGER UN GUIDE]", 250 + i);
        }
    }
    for (i, b) in s.trade.dealings.bids.iter().take(3).enumerate() {
        fb.text(
            3,
            24 + i,
            &format!(
                "MISE EN COURS : {:.0} KG {} A {} CR MAX, CLOTURE DANS {:.1} J",
                b.kg,
                g.data.resources[b.res].name.to_uppercase(),
                money(b.max_bid),
                (b.close - s.time) / DAY
            ),
            UP,
        );
    }
}

pub fn draw_lignes(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    fb.text(
        1,
        5,
        "LIGNES REGULIERES : DES CONVOIS DE CARGOS PNJ PARTENT A HEURE FIXE, VOLENT DES JOURS, SUBISSENT PIRATES, GUERRE, BLOCUS ET QUARANTAINE ;",
        DIM,
    );
    fb.text(
        1,
        6,
        "LES MARCHANDISES SONT REELLEMENT EN TRANSIT : LES PRIX DES DEUX BOUTS SUIVENT LES STOCKS. VOUS POUVEZ FINANCER UN CONVOI.",
        DIM,
    );
    let lines = g.eco_lines();
    if lines.is_empty() {
        fb.text(
            3,
            8,
            "AUCUNE LIGNE REGULIERE A CE SYSTEME (1 ETOILE SUR 3 EST UNE PLAQUE TOURNANTE).",
            RED,
        );
        if let Some((_, _, hubs)) = g.trade_ui.eco.lanes.borrow().as_ref() {
            for (i, (name, d)) in hubs.iter().enumerate() {
                fb.text(
                    3,
                    10 + i,
                    &format!("PLAQUE TOURNANTE PROCHE : {name} A {d:.1} AL"),
                    GREEN,
                );
            }
        }
        return;
    }
    let sel = g.trade_ui.sel.min(lines.len() - 1);
    for (i, lv) in lines.iter().take(6).enumerate() {
        let l = &lv.line;
        let col = if i == sel { AMBER } else { GREEN };
        let hov = g.hit_cells(1, 8 + i, 100, 1, Act::Call(Game::click_eco, 400 + i), None);
        fb.text(
            1,
            8 + i,
            &format!(
                "{}{:<14.14} > {:<14.14} {:<14.14} {:>4.1} AL  TOUS LES {:>2.0} J  {:>3.2} AL/J  MARGE {:>+5.1} %",
                if i == sel { ">" } else { " " },
                l.a_name.to_uppercase(),
                l.b_name.to_uppercase(),
                g.data.resources[l.res].name.to_uppercase(),
                l.dist_ly,
                l.period,
                l.speed,
                s.convoy_margin(l) * 100.0
            ),
            if hov && i != sel { WHITE } else { col },
        );
    }
    let lv = &lines[sel];
    let l = &lv.line;
    let ledger = vector_sim::lanes::ledger(&g.data, &s.galaxy, l, s.time);
    fb.text(
        1,
        15,
        &format!(
            "BILAN DE LA LIGNE : EXPEDIE {} KG = EN VOL {} + LIVRE {} + PERDU {}   (CONSERVE)",
            money(ledger.shipped),
            money(ledger.in_transit),
            money(ledger.delivered),
            money(ledger.lost)
        ),
        AMBER,
    );
    let fa = s.galaxy.territory(l.a, g.data.factions.len());
    let fb_ = s.galaxy.territory(l.b, g.data.factions.len());
    fb.text(
        1,
        16,
        &format!(
            "PORTS {:.1} % + {:.1} %   DOUANES A L'ARRIVEE {:.1} %   COURTIER {:.0} %",
            vector_sim::lanes::port_fee(&g.data, fa) * 100.0,
            vector_sim::lanes::port_fee(&g.data, fb_) * 100.0,
            vector_sim::lanes::tariff(&g.data, fb_) * 100.0,
            s.broker_fee() * 100.0
        ),
        DIM,
    );
    fb.text(
        1,
        18,
        "DEPART       ARRIVEE      CHARGE   PERDU   RETARD        ETAT",
        AMBER,
    );
    for (k, c) in lv.convoys.iter().enumerate() {
        use vector_sim::lanes::Phase;
        let phase = c.phase(s.time);
        let state = match phase {
            Phase::Waiting => "AU QUAI".to_string(),
            Phase::Transit => format!(
                "{} {:.0} %",
                gauge(c.progress(s.time), 10),
                c.progress(s.time) * 100.0
            ),
            Phase::Arrived => "ARRIVE".to_string(),
        };
        fb.text(
            1,
            19 + k,
            &format!(
                "J{:>+6.1}    J{:>+6.1}    {:>5.0} T  {:>4.0} T  {:>3.1} J {:<11} {}",
                (c.depart - s.time) / DAY,
                (c.arrive - s.time) / DAY,
                c.loaded / 1000.0,
                c.lost / 1000.0,
                c.delay_days,
                c.cause.label(),
                state
            ),
            if c.lost > 0.0 { RED } else { GREEN },
        );
        if phase == Phase::Waiting && s.docked.is_some() {
            button(fb, g, 92, 19 + k, "[FINANCER 1000 CR]", 300 + k);
        }
    }
    for (i, st) in s.trade.lanes.stakes.iter().take(3).enumerate() {
        fb.text(
            3,
            26 + i,
            &format!(
                "CONVOI FINANCE : {} ({} CR)",
                st.label.to_uppercase(),
                money(st.amount)
            ),
            UP,
        );
    }
}

pub fn draw_economie(fb: &mut Framebuffer, g: &Game) {
    let list = g.eco_jurisdictions();
    if list.is_empty() {
        return;
    }
    let sel = g.trade_ui.sel.min(list.len() - 1);
    fb.text(1, 5, "JURIDICTIONS (CLIQUER)", AMBER);
    for (i, (name, cell)) in list.iter().enumerate() {
        let hov = g.hit_cells(1, 6 + i, 28, 1, Act::Call(Game::click_eco, 400 + i), None);
        let here = *cell == vector_sim::politics::cell_of(g.sim.here_pos);
        fb.text(
            1,
            6 + i,
            &format!(
                "{}{:<22.22}{}",
                if i == sel { ">" } else { " " },
                name,
                if here { " ICI" } else { "" }
            ),
            if i == sel {
                AMBER
            } else if hov {
                WHITE
            } else {
                GREEN
            },
        );
    }
    let b = g.eco_board(list[sel].1);
    let s = &b.sector;
    let x0 = 32;
    fb.text(
        x0,
        5,
        &format!("{}  SECTEUR {:?}", b.currency, b.cell),
        AMBER,
    );
    fb.text(
        x0,
        6,
        &format!(
            "PIB {:>7} CR/J (PRODUCTION {:>7} = {:.0} %)   RECOLTE {:.0} %   CHOMAGE {:.1} %",
            money(s.gdp),
            money(s.production),
            s.production / s.gdp.max(1.0) * 100.0,
            s.harvest * 100.0,
            s.unemployment * 100.0
        ),
        GREEN,
    );
    fb.text(
        x0,
        7,
        &format!(
            "INFLATION {:>+5.1} %   TAUX {:.1} %   DETTE {:.0} % DU PIB   1 UNITE = {:.2} CR   RENDEMENT ETAT {:.1} %",
            s.inflation * 100.0,
            s.policy_rate * 100.0,
            s.debt_ratio * 100.0,
            s.fx,
            s.sovereign_yield() * 100.0
        ),
        GREEN,
    );
    let mut flags = Vec::new();
    if s.crash > 0.02 {
        flags.push(format!("KRACH {:.0} %", s.crash * 100.0));
    }
    if s.bubble > 0.02 {
        flags.push(format!("BULLE {:.0} %", s.bubble * 100.0));
    }
    if s.at_war {
        flags.push("GUERRE".to_string());
    }
    if s.unrest > 0.05 {
        flags.push("TROUBLES".to_string());
    }
    fb.text(
        x0,
        8,
        &format!(
            "DOUANES {:.1} %   PORTS {:.1} %   TOURISME {:.0} % ({} ETOILES D'HOTEL)   {}",
            b.tariff * 100.0,
            b.port_fee * 100.0,
            b.tourism * 100.0,
            vector_sim::macroeco::hotel_stars(b.tourism, g.sim.system.seed, 0),
            flags.join(" ")
        ),
        if flags.is_empty() { DIM } else { RED },
    );
    // Output by industry, the three biggest.
    let total: f64 = b.by_industry.iter().sum::<f64>().max(1.0);
    let mut order: Vec<usize> = (0..b.by_industry.len()).collect();
    order.sort_by(|a, c| b.by_industry[*c].total_cmp(&b.by_industry[*a]));
    for (k, &i) in order.iter().take(4).enumerate() {
        fb.text(
            x0,
            10 + k,
            &format!(
                "{:<22.22} {} {:>6} CR/J",
                g.data.industries[i].name.to_uppercase(),
                gauge(b.by_industry[i] / total * 3.0, 14),
                money(b.by_industry[i])
            ),
            DIM,
        );
    }
    fb.text(
        1,
        15,
        "24 MOIS D'HISTOIRE (UN POINT = UN MOIS, CALCULE DEPUIS LA GRAINE)",
        AMBER,
    );
    fb.text(1, 16, "PIB (CR/J)", DIM);
    chart(fb, 1, 17, 62, 8, &b.gdp, GREEN);
    fb.text(70, 16, "INFLATION ANNUELLE", DIM);
    chart(fb, 70, 17, 62, 8, &b.inflation, AMBER);
    fb.text(1, 26, "CHANGE (CR PAR UNITE)", DIM);
    chart(fb, 1, 27, 62, 8, &b.fx, UP);
    fb.text(70, 26, "TAUX DIRECTEUR", DIM);
    chart(fb, 70, 27, 62, 8, &b.rate, RED);
    fb.text(
        1,
        38,
        &format!(
            "PRIX DE REFERENCE : {} (CR/KG, BIEN CHOISI A LA BOURSE)",
            g.data.resources[b.price_res].name.to_uppercase()
        ),
        DIM,
    );
    chart(fb, 1, 39, 62, 8, &b.price, WHITE);
}
