//! Player settings (graphics, sound), stored as simple `key=value` lines.
use std::path::Path;
use winit::keyboard::KeyCode;

/// Languages of the interface (only French for now; the option is here for the translations).
pub const LANGS: [&str; 1] = ["FRANCAIS"];

pub const SCHEMES: [&str; 5] = ["COULEUR", "VERT PHOSPHORE", "AMBRE", "BLANC", "BLEU"];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub scanlines: bool,
    pub glow: bool,
    /// Index into `SCHEMES` (0 = the game's own colours).
    pub scheme: usize,
    /// 0..=10
    pub volume: u8,
    /// Guided tutorial on new games.
    pub tutorial: bool,
    /// Permanent command bar on the last screen row.
    pub hints: bool,
    /// AZERTY keyboard (default) or QWERTY: the keys are read by position and named by label.
    pub azerty: bool,
    /// Index into `LANGS`.
    pub lang: usize,
    /// Menu column of the cockpit (clickable menus and ship controls).
    pub flight_menu: bool,
    /// Scrolling banner of the news along the bottom of the screen.
    pub ticker: bool,
    /// Limiting magnitude of the sky: 5.0 + 0.5 * this (0..=6).
    pub sky_mag: u8,
    /// Galactic band, constellations and their names in the sky.
    pub sky_band: bool,
    pub sky_const: bool,
    pub sky_names: bool,
    /// Style de l'interface : NOUVEAU (true, defaut) ou CLASSIQUE.
    pub ui_new: bool,
    /// Arrangement of the cockpit: 0 A, 1 B, 2 C, 3 the old HUD (see `cockpit::Disposition`).
    pub cockpit: u8,
    /// Arrangement C: the console is unfolded.
    pub cockpit_open: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            scanlines: false,
            glow: false,
            scheme: 0,
            volume: 6,
            tutorial: true,
            hints: true,
            azerty: true,
            lang: 0,
            flight_menu: true,
            ticker: true,
            sky_mag: 3,
            sky_band: true,
            sky_const: true,
            sky_names: true,
            ui_new: !cfg!(test), // the tests of clicks read the classic cell layout
            cockpit: 0,
            cockpit_open: false,
        }
    }
}

impl Settings {
    pub const COUNT: usize = 16;

    /// Limiting magnitude of the sky.
    pub fn sky_mag_limit(&self) -> f64 {
        5.0 + 0.5 * f64::from(self.sky_mag.min(6))
    }

    pub fn describe(&self) -> Vec<(&'static str, String)> {
        let yn = |b: bool| if b { "OUI" } else { "NON" }.to_string();
        vec![
            ("Lignes de balayage (CRT)", yn(self.scanlines)),
            ("Halo lumineux", yn(self.glow)),
            (
                "Palette",
                SCHEMES[self.scheme.min(SCHEMES.len() - 1)].to_string(),
            ),
            ("Volume sonore", format!("{}/10", self.volume)),
            ("Tutoriel guide", yn(self.tutorial)),
            ("Barre des commandes", yn(self.hints)),
            (
                "Clavier",
                if self.azerty { "AZERTY" } else { "QWERTY" }.to_string(),
            ),
            ("Langue", LANGS[self.lang.min(LANGS.len() - 1)].to_string()),
            ("Menu de pilotage (cockpit)", yn(self.flight_menu)),
            ("Bandeau d'informations", yn(self.ticker)),
            (
                "Ciel : magnitude limite",
                format!("{:.1}", self.sky_mag_limit()),
            ),
            ("Ciel : bande galactique", yn(self.sky_band)),
            ("Ciel : constellations", yn(self.sky_const)),
            ("Ciel : noms des constellations", yn(self.sky_names)),
            (
                "Style de l'interface",
                if self.ui_new { "NOUVEAU" } else { "CLASSIQUE" }.to_string(),
            ),
            (
                "Disposition du cockpit",
                crate::cockpit::Disposition::from_index(self.cockpit)
                    .label()
                    .to_string(),
            ),
        ]
    }

    pub fn adjust(&mut self, i: usize, up: bool) {
        let cycle = |v: usize, n: usize| if up { (v + 1) % n } else { (v + n - 1) % n };
        match i {
            0 => self.scanlines = !self.scanlines,
            1 => self.glow = !self.glow,
            2 => self.scheme = cycle(self.scheme, SCHEMES.len()),
            3 => {
                self.volume = if up {
                    (self.volume + 1).min(10)
                } else {
                    self.volume.saturating_sub(1)
                }
            }
            4 => self.tutorial = !self.tutorial,
            5 => self.hints = !self.hints,
            6 => self.azerty = !self.azerty,
            7 => self.lang = cycle(self.lang, LANGS.len()),
            8 => self.flight_menu = !self.flight_menu,
            9 => self.ticker = !self.ticker,
            10 => {
                self.sky_mag = if up {
                    (self.sky_mag + 1).min(6)
                } else {
                    self.sky_mag.saturating_sub(1)
                }
            }
            11 => self.sky_band = !self.sky_band,
            12 => self.sky_const = !self.sky_const,
            13 => self.sky_names = !self.sky_names,
            14 => self.ui_new = !self.ui_new,
            15 => {
                let n = crate::cockpit::Disposition::ALL.len();
                let i = usize::from(self.cockpit.min(3));
                self.cockpit = (if up { (i + 1) % n } else { (i + n - 1) % n }) as u8;
            }
            _ => {}
        }
    }

    /// The key a physical key stands for under the chosen layout: the game names its keys by the
    /// letter printed on them, so on an AZERTY keyboard the keys at the QWERTY positions of A, Q,
    /// W, Z and M are read as Q, A, Z, W and M.
    pub fn translate(&self, code: KeyCode) -> KeyCode {
        if !self.azerty {
            return code;
        }
        match code {
            KeyCode::KeyQ => KeyCode::KeyA,
            KeyCode::KeyA => KeyCode::KeyQ,
            KeyCode::KeyW => KeyCode::KeyZ,
            KeyCode::KeyZ => KeyCode::KeyW,
            KeyCode::Semicolon => KeyCode::KeyM,
            KeyCode::KeyM => KeyCode::Comma,
            other => other,
        }
    }

    /// Flight rotation keys: pitch up and down, yaw right and left, roll left and right.
    pub fn move_keys(&self) -> [KeyCode; 6] {
        if self.azerty {
            [
                KeyCode::KeyZ,
                KeyCode::KeyS,
                KeyCode::KeyD,
                KeyCode::KeyQ,
                KeyCode::KeyA,
                KeyCode::KeyE,
            ]
        } else {
            [
                KeyCode::KeyW,
                KeyCode::KeyS,
                KeyCode::KeyD,
                KeyCode::KeyA,
                KeyCode::KeyQ,
                KeyCode::KeyE,
            ]
        }
    }

    /// Rewrite the key names of a help text for the chosen layout (texts are written in QWERTY).
    pub fn localize(&self, text: &str) -> String {
        if !self.azerty {
            return text.to_string();
        }
        text.replace(
            "W/S tangage, A/D lacet, Q/E roulis",
            "Z/S tangage, Q/D lacet, A/E roulis",
        )
        .replace("W/S A/D Q/E", "Z/S Q/D A/E")
        .replace("WASD/QE", "ZQSD/AE")
        .replace("Z tribut", "Entree : tribut (radio)")
    }

    pub fn to_text(&self) -> String {
        format!(
            "scanlines={}\nglow={}\nscheme={}\nvolume={}\ntutorial={}\nhints={}\nazerty={}\nlang={}\nmenu={}\nticker={}\nskymag={}\nskyband={}\nskyconst={}\nskynames={}\nuistyle={}\ncockpit={}\ncockpitopen={}\n",
            u8::from(self.scanlines),
            u8::from(self.glow),
            self.scheme,
            self.volume,
            u8::from(self.tutorial),
            u8::from(self.hints),
            u8::from(self.azerty),
            self.lang,
            u8::from(self.flight_menu),
            u8::from(self.ticker),
            self.sky_mag,
            u8::from(self.sky_band),
            u8::from(self.sky_const),
            u8::from(self.sky_names),
            u8::from(self.ui_new),
            self.cockpit,
            u8::from(self.cockpit_open)
        )
    }

    pub fn from_text(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let Ok(n) = v.trim().parse::<usize>() else {
                continue;
            };
            match k.trim() {
                "scanlines" => s.scanlines = n != 0,
                "glow" => s.glow = n != 0,
                "scheme" => s.scheme = n.min(SCHEMES.len() - 1),
                "tutorial" => s.tutorial = n != 0,
                "hints" => s.hints = n != 0,
                "azerty" => s.azerty = n != 0,
                "menu" => s.flight_menu = n != 0,
                "ticker" => s.ticker = n != 0,
                "skymag" => s.sky_mag = n.min(6) as u8,
                "skyband" => s.sky_band = n != 0,
                "skyconst" => s.sky_const = n != 0,
                "skynames" => s.sky_names = n != 0,
                "uistyle" => s.ui_new = n != 0,
                "cockpit" => s.cockpit = n.min(3) as u8,
                "cockpitopen" => s.cockpit_open = n != 0,
                "lang" => s.lang = n.min(LANGS.len() - 1),
                "volume" => s.volume = n.min(10) as u8,
                _ => {}
            }
        }
        s
    }

    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path)
            .map(|t| Settings::from_text(&t))
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, self.to_text());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_tolerance() {
        let mut s = Settings::default();
        s.adjust(0, true);
        s.adjust(2, true);
        s.adjust(3, true);
        assert_eq!(Settings::from_text(&s.to_text()), s);
        // Garbage lines and out-of-range values are ignored or clamped.
        let t = Settings::from_text("nonsense\nscheme=99\nvolume=500\nglow=x\n");
        assert_eq!(t.scheme, SCHEMES.len() - 1);
        assert_eq!(t.volume, 10);
        assert!(!t.glow);
        assert_eq!(Settings::from_text("").describe().len(), Settings::COUNT);
    }

    #[test]
    fn every_setting_adjusts() {
        for i in 0..Settings::COUNT {
            if i == 7 && LANGS.len() < 2 {
                // One language so far: the option exists for the translations to come.
                continue;
            }
            let base = Settings::default();
            let mut a = base.clone();
            let mut b = base.clone();
            a.adjust(i, true);
            b.adjust(i, false);
            assert!(a != base || b != base, "setting {i}");
        }
    }
}
