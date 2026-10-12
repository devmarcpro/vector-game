//! Two station screens: the livestock pens (L: buy, breed, harvest, with the genes drawn as
//! bars) and the casino (A: roulette and a machine, stake and game chosen by clickable words).
use crate::game::{Game, Screen};
use crate::hub::word;
use crate::hud::{AMBER, DIM, DOWN, GREEN, UP, WHITE};
use crate::mouse::Act;
use vector_render::Framebuffer;
use vector_sim::casino::{self, Game as Bet};
use winit::keyboard::KeyCode;

const STAKES: [f64; 6] = [10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0];

/// State of both screens.
#[derive(Default)]
pub struct FarmUi {
    pub animal: usize,
    /// The livestock screen of a station's dealer (buy and sell) or of the ship's pens (breed
    /// and slaughter): animals are kept aboard, bought and sold at stations.
    pub dealer: bool,
    pub species: usize,
    pub game: usize,
    pub number: u32,
    pub stake: usize,
    pub last: Option<casino::Round>,
}

impl FarmUi {
    fn bet(&self) -> Bet {
        match self.game {
            0 => Bet::Red,
            1 => Bet::Black,
            2 => Bet::Number(self.number),
            _ => Bet::Machine,
        }
    }
}

fn bar(v: f64, w: usize) -> String {
    let n = (v * w as f64).round() as usize;
    format!("{}{}", "#".repeat(n.min(w)), ".".repeat(w - n.min(w)))
}

impl Game {
    pub fn click_game(&mut self, i: usize) {
        self.farm.game = i;
    }

    pub fn click_animal(&mut self, i: usize) {
        self.farm.animal = i;
    }

    pub fn livestock_key(&mut self, code: KeyCode) {
        let n = self.sim.herd.len();
        let list = self.sim.domestic_species();
        let r: Result<String, &str> = match code {
            KeyCode::ArrowUp if n > 0 => {
                self.farm.animal = (self.farm.animal + n - 1) % n;
                return;
            }
            KeyCode::ArrowDown if n > 0 => {
                self.farm.animal = (self.farm.animal + 1) % n;
                return;
            }
            KeyCode::ArrowLeft if !list.is_empty() => {
                self.farm.species = (self.farm.species + list.len() - 1) % list.len();
                return;
            }
            KeyCode::ArrowRight if !list.is_empty() => {
                self.farm.species = (self.farm.species + 1) % list.len();
                return;
            }
            KeyCode::KeyB | KeyCode::KeyS if !self.farm.dealer => {
                Err("ACHAT ET VENTE CHEZ UN ELEVEUR, DANS UNE STATION")
            }
            KeyCode::KeyP | KeyCode::KeyH if self.farm.dealer => {
                Err("LES ENCLOS SONT A BORD DE VOTRE VAISSEAU")
            }
            KeyCode::KeyS => self
                .sim
                .sell_animal(self.farm.animal)
                .map(|c| format!("ANIMAL VENDU POUR {c:.0} CR")),
            KeyCode::KeyB if !list.is_empty() => {
                let s = list[self.farm.species % list.len()];
                self.sim
                    .buy_animal(s)
                    .map(|c| format!("ANIMAL ACHETE POUR {c:.0} CR"))
            }
            KeyCode::KeyP => {
                let i = self.farm.animal;
                match self.sim.best_partner(i) {
                    Some(j) => self.sim.breed(i, j).map(|k| {
                        self.farm.animal = k;
                        "NAISSANCE !".to_string()
                    }),
                    None => Err("aucun partenaire disponible"),
                }
            }
            KeyCode::KeyH => self.sim.harvest_animal(self.farm.animal).map(|got| {
                let kg: f64 = got.iter().map(|(_, k)| k).sum();
                format!("ABATTU : {kg:.1} KG EN SOUTE")
            }),
            KeyCode::Escape | KeyCode::KeyL => {
                self.screen = Screen::Station;
                return;
            }
            _ => return,
        };
        let n = self.sim.herd.len();
        self.farm.animal = self.farm.animal.min(n.saturating_sub(1));
        match r {
            Ok(m) => self.say(m),
            Err(e) => self.say(e.to_uppercase()),
        }
    }

    pub fn casino_key(&mut self, code: KeyCode) {
        let f = &mut self.farm;
        match code {
            KeyCode::ArrowLeft => f.game = (f.game + 3) % 4,
            KeyCode::ArrowRight => f.game = (f.game + 1) % 4,
            KeyCode::ArrowUp => f.stake = (f.stake + 1).min(STAKES.len() - 1),
            KeyCode::ArrowDown => f.stake = f.stake.saturating_sub(1),
            KeyCode::Equal | KeyCode::NumpadAdd => f.number = (f.number + 1) % casino::SLOTS,
            KeyCode::Minus | KeyCode::NumpadSubtract => {
                f.number = (f.number + casino::SLOTS - 1) % casino::SLOTS;
            }
            KeyCode::Enter | KeyCode::Space => {
                let bet = f.bet();
                match self.sim.gamble(bet, STAKES[self.farm.stake]) {
                    Ok(r) => {
                        let net = r.payout - r.stake;
                        self.farm.last = Some(r);
                        if net > 0.0 {
                            self.say(format!("GAGNE {net:.0} CR"));
                        } else {
                            self.say(format!("PERDU {:.0} CR", -net));
                        }
                    }
                    Err(e) => self.say(e.to_uppercase()),
                }
            }
            KeyCode::Escape | KeyCode::KeyA => self.screen = Screen::Station,
            _ => {}
        }
    }
}

pub fn draw_livestock(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    let dealer = g.farm.dealer;
    fb.text(
        1,
        1,
        if dealer {
            "== MARCHE AUX BETES (STATION) =="
        } else {
            "== ENCLOS DU VAISSEAU =="
        },
        AMBER,
    );
    fb.text(
        20,
        1,
        &format!(
            "CREDITS {:.0}   ENCLOS {}/{}   RATIONS {:.0} KG",
            s.credits,
            s.herd.len(),
            vector_sim::livestock::HERD_MAX,
            s.data
                .resource_index("food")
                .map_or(0.0, |r| s.ship.cargo.kg[r])
        ),
        DIM,
    );
    let list = s.domestic_species();
    if let (true, Some(&sp)) = (dealer, list.get(g.farm.species % list.len().max(1))) {
        fb.text(
            1,
            3,
            &format!(
                "A VENDRE : < {} >  {:.0} CR  (GAUCHE/DROITE)",
                s.data.species[sp].name.to_uppercase(),
                s.animal_price(sp)
            ),
            GREEN,
        );
    }
    fb.text(
        1,
        5,
        "N  ESPECE              SEXE AGE  ADULTE  QUALITE   RDT CRO RUS DOC  GEN",
        DIM,
    );
    for (i, a) in s.herd.iter().enumerate() {
        let sel = i == g.farm.animal;
        let hov = g.hit_cells(1, 6 + i, 76, 1, Act::Call(Game::click_animal, i), None);
        let col = if sel {
            AMBER
        } else if hov {
            WHITE
        } else if a.hunger > 0.0 {
            DOWN
        } else {
            GREEN
        };
        fb.text(
            1,
            6 + i,
            &format!(
                "{}{:<2}{:<18.18} {}  {:>4.0}  {:<6}  {}  ",
                if sel { ">" } else { " " },
                i,
                s.data.species[a.species].name.to_uppercase(),
                if a.male { "M" } else { "F" },
                a.age_days(s.time),
                if a.adult(s.time) { "OUI" } else { "NON" },
                bar(a.quality(), 8),
            ),
            col,
        );
        for t in 0..4 {
            let v = a.trait_value(t);
            fb.text(
                49 + t * 4,
                6 + i,
                &format!("{:>3.0}", v * 100.0),
                if v > 0.65 {
                    UP
                } else if v < 0.35 {
                    DOWN
                } else {
                    DIM
                },
            );
        }
        fb.text(66, 6 + i, &format!("{:>3}", a.gen), DIM);
    }
    if s.herd.is_empty() {
        fb.text(
            3,
            7,
            "AUCUN ANIMAL. ACHETEZ-EN (B) : UN MALE ET UNE FEMELLE.",
            DIM,
        );
    }
    let row = 6 + vector_sim::livestock::HERD_MAX + 1;
    let mut x = 1;
    if dealer {
        x = word(fb, g, x, row, "[B] ACHETER L'ESPECE CHOISIE", KeyCode::KeyB);
        word(fb, g, x, row, "[S] VENDRE L'ANIMAL CHOISI", KeyCode::KeyS);
    } else {
        x = word(
            fb,
            g,
            x,
            row,
            "[P] REPRODUIRE (MEILLEUR PARTENAIRE)",
            KeyCode::KeyP,
        );
        word(fb, g, x, row, "[H] ABATTRE", KeyCode::KeyH);
    }
    fb.text(
        1,
        row + 2,
        "RDT RENDEMENT A L'ABATTAGE  CRO MATURITE  RUS RESISTANCE A LA FAIM  DOC DOCILITE",
        DIM,
    );
    fb.text(
        1,
        row + 3,
        "CHAQUE ANIMAL MANGE DES RATIONS CHAQUE JOUR ; SANS RATIONS IL MEURT DE FAIM.",
        DIM,
    );
    for (i, m) in g.messages.iter().rev().take(1).enumerate() {
        fb.text(1, row + 4 + i, m, AMBER);
    }
}

pub fn draw_casino(fb: &mut Framebuffer, g: &Game) {
    let s = &g.sim;
    fb.text(1, 1, "== CASINO ==", AMBER);
    fb.text(16, 1, &format!("CREDITS {:.0}", s.credits), GREEN);
    fb.text(
        1,
        3,
        "LA MAISON GARDE TOUJOURS UN AVANTAGE. JOUEZ POUR LE PLAISIR.",
        DIM,
    );
    let f = &g.farm;
    let names = ["ROUGE", "NOIR", "NUMERO", "MACHINE A SOUS"];
    let mut x = 1;
    for (i, n) in names.iter().enumerate() {
        let hov = g.hit_cells(x, 6, n.len(), 1, Act::Call(Game::click_game, i), None);
        fb.text(
            x,
            6,
            n,
            if i == f.game {
                AMBER
            } else if hov {
                WHITE
            } else {
                GREEN
            },
        );
        x += n.len() + 3;
    }
    if f.game == 2 {
        fb.text(
            1,
            8,
            &format!("NUMERO {} (+/- POUR CHANGER)  GAIN x36", f.number),
            GREEN,
        );
    } else if f.game == 3 {
        fb.text(
            1,
            8,
            "TROIS SYMBOLES IDENTIQUES x20   DEUX IDENTIQUES x0,9",
            GREEN,
        );
    } else {
        fb.text(1, 8, "GAIN x2 (LE 0 VERT FAIT GAGNER LA MAISON)", GREEN);
    }
    fb.text(
        1,
        10,
        &format!("MISE {:.0} CR (HAUT/BAS)", STAKES[f.stake]),
        AMBER,
    );
    let x = word(fb, g, 1, 12, "[ENTREE] JOUER", KeyCode::Enter);
    word(fb, g, x, 12, "[A] QUITTER", KeyCode::Escape);
    if let Some(r) = &f.last {
        let net = r.payout - r.stake;
        let result = if matches!(r.game, Bet::Machine) {
            let sym = |n: u32| ["%", "$", "#", "&", "*", "@"][n as usize % 6];
            format!(
                "[ {} ][ {} ][ {} ]",
                sym(r.reels[0]),
                sym(r.reels[1]),
                sym(r.reels[2])
            )
        } else {
            format!(
                "LA BILLE S'ARRETE SUR {} {}",
                r.wheel,
                if r.wheel == 0 {
                    "VERT"
                } else if casino::is_red(r.wheel) {
                    "ROUGE"
                } else {
                    "NOIR"
                }
            )
        };
        fb.text_big(8, 170, &result, 2, WHITE);
        fb.text(
            1,
            22,
            &if net > 0.0 {
                format!("GAGNE {net:.0} CR")
            } else {
                format!("PERDU {:.0} CR", -net)
            },
            if net > 0.0 { UP } else { DOWN },
        );
    }
}
