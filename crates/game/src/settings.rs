//! Player settings (graphics, sound), stored as simple `key=value` lines.
use std::path::Path;

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
        }
    }
}

impl Settings {
    pub const COUNT: usize = 6;

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
            _ => {}
        }
    }

    pub fn to_text(&self) -> String {
        format!(
            "scanlines={}\nglow={}\nscheme={}\nvolume={}\ntutorial={}\nhints={}\n",
            u8::from(self.scanlines),
            u8::from(self.glow),
            self.scheme,
            self.volume,
            u8::from(self.tutorial),
            u8::from(self.hints)
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
            let base = Settings::default();
            let mut a = base.clone();
            let mut b = base.clone();
            a.adjust(i, true);
            b.adjust(i, false);
            assert!(a != base || b != base, "setting {i}");
        }
    }
}
