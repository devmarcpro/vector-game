//! Scenes de capture `--snapshot` : les ecrans du style NOUVEAU (`ui_<nom>`) et CLASSIQUE
//! (`ui_classic_<nom>`) avec les memes donnees, plus les maquettes de cockpit (a retirer avec
//! `mock_cockpit`, qui est a `wt-cockpit`). La boite a outils graphique est dans `theme`.
#![allow(clippy::too_many_arguments)]

use crate::game::Game;
pub use crate::theme::*;
use vector_render::Framebuffer;

#[derive(Clone, Copy)]
pub struct Spec {
    pub base: &'static str,
    pub kind: Kind,
    /// Style de l'interface force pour la capture (None : celui des options).
    pub new_style: Option<bool>,
    /// True : maquette de cockpit ; false : l'ecran tel qu'il est (`now_<nom>`).
    pub mock: bool,
    pub ui: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ui,
    Cockpit(crate::mock_cockpit::Layout),
    CockpitLand(crate::mock_cockpit::Layout),
}

pub fn resolve(name: &str) -> Option<Spec> {
    use crate::mock_cockpit::Layout::{COpen, A, B, C};
    if let Some(r) = name.strip_prefix("ui_") {
        let (new_style, r) = match r.strip_prefix("classic_") {
            Some(r) => (false, r),
            None => (true, r),
        };
        let base = match r {
            "colony" => "city",
            "hub" => "hub_dock",
            "crew" | "crew_med" => "medicine",
            "surface" => "surface",
            "station" => "station",
            "dash" => "dash_dock",
            _ => return None,
        };
        let ui = [
            "colony", "hub", "crew", "crew_med", "surface", "station", "dash",
        ]
        .into_iter()
        .find(|k| *k == r)?;
        return Some(Spec {
            base,
            kind: Kind::Ui,
            new_style: Some(new_style),
            mock: false,
            ui,
        });
    }
    let (mock, rest) = match name.strip_prefix("mock_") {
        Some(r) => (true, r),
        None => (false, name.strip_prefix("now_")?),
    };
    let (base, kind) = match rest {
        "cockpit" | "cockpit_a" => ("cockpit", Kind::Cockpit(A)),
        "cockpit_b" => ("cockpit", Kind::Cockpit(B)),
        "cockpit_c" => ("cockpit", Kind::Cockpit(C)),
        "cockpit_c_open" => ("cockpit", Kind::Cockpit(COpen)),
        "cockpit_land" | "cockpit_land_a" => ("land_descent", Kind::CockpitLand(A)),
        "cockpit_land_b" => ("land_descent", Kind::CockpitLand(B)),
        "cockpit_land_c" => ("land_descent", Kind::CockpitLand(C)),
        "cockpit_land_c_open" => ("land_descent", Kind::CockpitLand(COpen)),
        "land" => ("land_descent", Kind::CockpitLand(A)),
        _ => return None,
    };
    Some(Spec {
        base,
        kind,
        new_style: None,
        mock,
        ui: "",
    })
}

/// Adjust the game after the scene of `spec.base` has set it up.
pub fn after_setup(g: &mut Game, spec: &Spec) {
    g.tut_step = None;
    match spec.kind {
        Kind::Ui => {
            crate::ui_new::scene_setup(g, spec.ui);
            if let Some(n) = spec.new_style {
                g.settings.ui_new = n;
            }
        }
        Kind::Cockpit(_) => crate::mock_cockpit::setup_free(g),
        Kind::CockpitLand(_) => crate::mock_cockpit::setup_land(g),
    }
}

/// Draw the frame of the spec.
pub fn render(fb: &mut Framebuffer, g: &mut Game, spec: &Spec) {
    if !spec.mock {
        crate::ui_new::refresh(g);
        crate::draw_frame(fb, g);
        return;
    }
    g.begin_frame();
    fb.clear();
    if let Kind::Cockpit(l) | Kind::CockpitLand(l) = spec.kind {
        crate::mock_cockpit::draw(fb, g, l);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenes_resolve() {
        assert!(resolve("ui_colony").is_some_and(|s| s.new_style == Some(true)));
        assert!(resolve("ui_classic_hub").is_some_and(|s| s.new_style == Some(false)));
        assert!(resolve("ui_nothing").is_none());
        assert!(resolve("mock_cockpit_b").is_some());
        assert!(resolve("colony").is_none());
    }
}
