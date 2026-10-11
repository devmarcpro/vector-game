# PROMPT — Session 2 de VECTOR (à coller au début d'une nouvelle session)

Tu reprends le projet VECTOR, un bac à sable spatial solo en Rust (moteur 100 % maison : rasteriseur de lignes CPU, police bitmap 5x7, simulation déterministe à 30 Hz, galaxie infinie par hachage de la graine). Dépôt : https://github.com/devmarcpro/vector-game (branche master), dossier de travail local : `C:\Users\ciryl\jeu espace`.

## Lis d'abord, dans cet ordre
1. `README.md`, puis `docs/PROGRESS.md`, `docs/DECISIONS.md`, `docs/ACCEPTANCE.md`.
2. `docs/ROADMAP.md` : le plan validé (étape 0 de consolidation, puis E1 à E6).
3. `docs/SIM_MODEL.md`, `docs/CONTENT.md`, `docs/CONTROLS.md`.
Ne reconstruis rien : M0-M10 sont terminés (147 tests verts, clippy propre, 100 ans de simulation stables, `dist/VECTOR.zip`).

## Contraintes permanentes
- Autonomie : ne me pose pas de questions sauf vrai blocage ; consigne les décisions dans `docs/DECISIONS.md` et l'avancement dans `docs/PROGRESS.md`. Avant chaque grosse étape, montre-moi un court plan détaillé, puis exécute.
- Dépendances : winit, softbuffer, serde, ron, postcard, cpal uniquement, sauf justification écrite.
- Architecture : données dans `data/*.ron` validées au chargement ; génération déterministe par hachage ; sim / rendu / UI séparés ; sauvegardes = graine + deltas (format `VECT`, version à incrémenter) ; chaque règle ou système nouveau est testé ; chaque nouvel élément de contenu a une entrée de codex générée depuis les données ; texte en capitales (la police est en cours d'accentuation, voir étape 0).
- Vérification sans voir la fenêtre : tests, `--snapshot` (PNG) et `--script` (scripts dans `scripts/`, rejoués par `cargo test`). Avant de livrer une étape : `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, puis commit et push. Messages de commit terminés par la ligne de co-auteur fournie par l'environnement.
- Quirks Windows connus : cargo est dans `%USERPROFILE%\.cargo\bin` (à ajouter au PATH en PowerShell) ; les apostrophes cassent les heredocs bash (utiliser l'outil Write ou des scripts) ; `cargo fmt` reformate le code donc les patches par ancre échouent (relire le texte réel avant d'éditer) ; relire un fichier avant de l'éditer.
- Sois factuel : si un test échoue ou qu'une étape est partielle, dis-le.

## Mission de cette session
Fais d'abord l'étape 0 de consolidation (jouabilité réelle par scripts, équilibrage, performance du vol libre, accents, finition des sauvegardes). Puis enchaîne E1 (planètes détaillées), E2 (faune et flore), E3 (organes, cybernétique, mutations), E4 (dialogues et civilisations), E5 (vue roguelike en quadrillage à la Dwarf Fortress : intérieur du vaisseau, stations, villes, ruines, surfaces), E6 (idées annexes), en les découpant en jalons testables comme M0-M10. Pour chaque jalon : plan, implémentation, tests, captures, docs, commit/push.

Commence par me confirmer en trois lignes que tu as lu la documentation et l'état du dépôt (`git log`, `cargo test`), puis donne-moi le plan détaillé de l'étape 0.
