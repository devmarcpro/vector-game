Scripts de la session de jeu testeur (docs/JOURNAL_DE_TEST.md). Non executes par `cargo test`
(seuls les .txt de scripts/ le sont). Chaque scenario s'ecrit en deux morceaux :

  cat scripts/play/start.txt scripts/play/b4.txt > /tmp/run.txt
  target/release/vector-game.exe --script /tmp/run.txt

start*.txt : creation d'une partie par les menus (start = Voyageur, start_marchand, start_explo,
start_outlaw, start_colon, start_wreck = Naufrage). Les captures vont dans shots/ (ignore par git).
Commandes : voir l'en-tete de crates/game/src/script.rs.
