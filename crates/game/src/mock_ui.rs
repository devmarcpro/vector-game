//! Scenes de capture `--snapshot` : les ecrans du style NOUVEAU (`ui_<nom>`) et CLASSIQUE
//! (`ui_classic_<nom>`) avec les memes donnees, (le cockpit n'est plus une maquette : voir `cockpit/`). La boite a outils graphique est dans `theme`.
#![allow(clippy::too_many_arguments)]

use crate::game::Game;
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
}

pub fn resolve(name: &str) -> Option<Spec> {
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
    None
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenes_resolve() {
        assert!(resolve("ui_colony").is_some_and(|s| s.new_style == Some(true)));
        assert!(resolve("ui_classic_hub").is_some_and(|s| s.new_style == Some(false)));
        assert!(resolve("ui_nothing").is_none());
        assert!(resolve("colony").is_none());
    }
}
