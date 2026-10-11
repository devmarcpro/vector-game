//! Names of everything in the universe, from the grammar in `data/names.ron`.
//!
//! One source for every name: stars, planets and moons (by nature), stations, anomalies,
//! nebulae, clusters, sectors, galaxies, cities, rivers, mountains, companies, ships, pirates,
//! cartels, religions, and the people of each culture. Everything is a pure function of a hash
//! (never of a clock), so the same seed always gives the same names. The grammar is embedded
//! in the binary and validated the first time it is used (and by the tests).
use serde::Deserialize;
use std::collections::BTreeSet;
use std::sync::OnceLock;
use vector_core::{mix64, Rng};

const GRAMMAR: &str = include_str!("../../../data/names.ron");

const GREEK: [&str; 24] = [
    "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta", "Theta", "Iota", "Kappa",
    "Lambda", "Mu", "Nu", "Xi", "Omicron", "Pi", "Rho", "Sigma", "Tau", "Upsilon", "Phi", "Chi",
    "Psi", "Omega",
];

/// Characters the 5x7 font can draw (plain ASCII plus the French accents it composes).
const ACCENTED: &str = "éèêëàâäîïôöùûüçÉÈÊËÀÂÄÎÏÔÖÙÛÜÇ";

/// Tokens that need no list.
const TOKENS: [&str; 17] = [
    "root", "root2", "gr", "rom", "n", "n3", "n4", "cat", "l", "idx", "idx_l", "host", "label",
    "given", "surname", "person", "w",
];

/// Syllable generator: start + optional mid + end.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Voice {
    #[serde(default)]
    pub id: String,
    pub start: Vec<String>,
    pub mid: Vec<String>,
    pub end: Vec<String>,
    pub mid_chance: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WordList {
    pub id: String,
    pub list: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Pattern {
    pub kind: String,
    pub voice: String,
    pub forms: Vec<(String, u32)>,
}

/// The names of one culture: people, and the flavour of its places.
#[derive(Clone, Debug, Deserialize)]
pub struct CultureNames {
    pub id: String,
    pub place: Voice,
    pub words: Vec<WordList>,
    pub first_start: Vec<String>,
    pub first_mid: Vec<String>,
    pub first_mid_chance: f64,
    pub first_end_m: Vec<String>,
    pub first_end_f: Vec<String>,
    pub last_start: Vec<String>,
    pub last_mid: Vec<String>,
    pub last_mid_chance: f64,
    pub last_end: Vec<String>,
    pub particles: Vec<String>,
    pub particle_chance: f64,
    pub nick_m: Vec<String>,
    pub nick_f: Vec<String>,
    pub title_m: Vec<String>,
    pub title_f: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NameBook {
    pub voices: Vec<Voice>,
    pub words: Vec<WordList>,
    pub patterns: Vec<Pattern>,
    pub cultures: Vec<CultureNames>,
}

/// Every kind of place or thing the grammar must be able to name.
pub const KINDS: [&str; 26] = [
    "star",
    "planet_dead",
    "planet_living",
    "planet_giant",
    "moon_dead",
    "moon_named",
    "station",
    "anomaly",
    "nebula",
    "cluster",
    "sector",
    "galaxy",
    "city",
    "colony",
    "river",
    "mountain",
    "water",
    "region",
    "company",
    "ship_civil_m",
    "ship_civil_f",
    "ship_pirate_m",
    "ship_pirate_f",
    "cartel",
    "religion",
    "faction",
];

fn char_ok(c: char) -> bool {
    c.is_ascii_alphanumeric() || " -'.".contains(c) || ACCENTED.contains(c)
}

fn text_ok(s: &str) -> bool {
    !s.is_empty() && s.chars().all(char_ok)
}

impl NameBook {
    /// Parse and validate a grammar.
    pub fn parse(src: &str) -> Result<NameBook, String> {
        let book: NameBook = ron::from_str(src).map_err(|e| format!("names.ron: {e}"))?;
        book.validate()?;
        Ok(book)
    }

    fn validate(&self) -> Result<(), String> {
        let bad = |m: String| Err(format!("names.ron: {m}"));
        let check_list = |what: &str, l: &[String], needed: bool| -> Result<(), String> {
            if needed && l.is_empty() {
                return Err(format!("names.ron: liste vide : {what}"));
            }
            for s in l {
                if !text_ok(s) {
                    return Err(format!(
                        "names.ron: texte non affichable dans {what} : {s:?}"
                    ));
                }
            }
            Ok(())
        };
        let check_voice = |v: &Voice| -> Result<(), String> {
            check_list(&format!("voix {} debut", v.id), &v.start, true)?;
            check_list(&format!("voix {} milieu", v.id), &v.mid, true)?;
            check_list(&format!("voix {} fin", v.id), &v.end, true)?;
            if !(0.0..=1.0).contains(&v.mid_chance) {
                return Err(format!("names.ron: mid_chance hors de 0..1 ({})", v.id));
            }
            Ok(())
        };
        let mut ids = BTreeSet::new();
        for v in &self.voices {
            if !ids.insert(v.id.clone()) {
                return bad(format!("voix en double : {}", v.id));
            }
            check_voice(v)?;
        }
        let mut words = BTreeSet::new();
        for w in &self.words {
            if !words.insert(w.id.as_str()) {
                return bad(format!("liste en double : {}", w.id));
            }
            check_list(&format!("liste {}", w.id), &w.list, true)?;
        }
        let mut kinds = BTreeSet::new();
        for p in &self.patterns {
            if !kinds.insert(p.kind.as_str()) {
                return bad(format!("modele en double : {}", p.kind));
            }
            if !ids.contains(&p.voice) {
                return bad(format!("modele {} : voix inconnue {}", p.kind, p.voice));
            }
            if p.forms.is_empty() || p.forms.iter().all(|f| f.1 == 0) {
                return bad(format!("modele {} sans forme utilisable", p.kind));
            }
            for (form, weight) in &p.forms {
                if *weight == 0 {
                    return bad(format!("modele {} : poids nul pour {form:?}", p.kind));
                }
                // Braces are grammar, not text: only the literal parts must be drawable.
                if !strip_tokens(form).chars().all(char_ok) {
                    return bad(format!("modele {} : texte non affichable {form:?}", p.kind));
                }
                for tok in tokens_of(form) {
                    let (name, arg) = tok.split_once(':').unwrap_or((tok.as_str(), ""));
                    if !TOKENS.contains(&name) {
                        return bad(format!("modele {} : jeton inconnu {{{tok}}}", p.kind));
                    }
                    if name == "w" && !words.contains(arg) {
                        return bad(format!("modele {} : liste inconnue {arg}", p.kind));
                    }
                }
            }
        }
        for k in KINDS {
            if !kinds.contains(k) {
                return bad(format!("modele manquant : {k}"));
            }
        }
        if self.cultures.is_empty() {
            return bad("aucune culture".into());
        }
        let mut cids = BTreeSet::new();
        for c in &self.cultures {
            if !cids.insert(c.id.as_str()) {
                return bad(format!("culture en double : {}", c.id));
            }
            let n = |s: &str| format!("culture {} : {s}", c.id);
            check_voice(&c.place)?;
            for w in &c.words {
                if !words.contains(w.id.as_str()) {
                    return bad(n(&format!("liste inconnue {}", w.id)));
                }
                check_list(&n(&w.id), &w.list, true)?;
            }
            for (what, l, needed) in [
                ("first_start", &c.first_start, true),
                ("first_mid", &c.first_mid, true),
                ("first_end_m", &c.first_end_m, true),
                ("first_end_f", &c.first_end_f, true),
                ("last_start", &c.last_start, true),
                ("last_mid", &c.last_mid, true),
                ("last_end", &c.last_end, true),
                ("particles", &c.particles, false),
                ("nick_m", &c.nick_m, true),
                ("nick_f", &c.nick_f, true),
                ("title_m", &c.title_m, true),
                ("title_f", &c.title_f, true),
            ] {
                check_list(&n(what), l, needed)?;
            }
            for p in [c.first_mid_chance, c.last_mid_chance, c.particle_chance] {
                if !(0.0..=1.0).contains(&p) {
                    return bad(n("probabilite hors de 0..1"));
                }
            }
            if c.particle_chance > 0.0 && c.particles.is_empty() {
                return bad(n("particle_chance sans particules"));
            }
        }
        Ok(())
    }

    pub fn culture(&self, id: &str) -> Option<&CultureNames> {
        self.cultures.iter().find(|c| c.id == id)
    }

    fn voice(&self, id: &str) -> &Voice {
        self.voices
            .iter()
            .find(|v| v.id == id)
            .unwrap_or(&self.voices[0])
    }

    fn words_of<'a>(&'a self, id: &str, culture: Option<&'a CultureNames>) -> &'a [String] {
        culture
            .and_then(|c| c.words.iter().find(|w| w.id == id))
            .or_else(|| self.words.iter().find(|w| w.id == id))
            .map_or(&[][..], |w| &w.list)
    }
}

/// The embedded, validated grammar.
pub fn book() -> &'static NameBook {
    static BOOK: OnceLock<NameBook> = OnceLock::new();
    BOOK.get_or_init(|| NameBook::parse(GRAMMAR).unwrap_or_else(|e| panic!("{e}")))
}

fn tokens_of(form: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = form;
    while let Some(a) = rest.find('{') {
        let Some(b) = rest[a..].find('}') else { break };
        out.push(rest[a + 1..a + b].to_string());
        rest = &rest[a + b + 1..];
    }
    out
}

fn strip_tokens(form: &str) -> String {
    let mut out = String::new();
    let mut depth = false;
    for c in form.chars() {
        match c {
            '{' => depth = true,
            '}' => depth = false,
            _ if !depth => out.push(c),
            _ => {}
        }
    }
    out
}

pub fn pick<'a, T>(rng: &mut Rng, list: &'a [T]) -> &'a T {
    &list[(rng.next_u64() % list.len() as u64) as usize]
}

pub fn roman(n: usize) -> String {
    const R: [&str; 12] = [
        "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII",
    ];
    R.get(n)
        .map_or_else(|| format!("{}", n + 1), |s| (*s).to_string())
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

fn syllables(v: &Voice, rng: &mut Rng) -> String {
    let mut s = pick(rng, &v.start).clone();
    if rng.f64() < v.mid_chance {
        s.push_str(pick(rng, &v.mid));
    }
    s.push_str(pick(rng, &v.end));
    cap(&s)
}

/// "de Alpha" -> "d'Alpha", "le Aube" -> "l'Aube" (also "la"), before a vowel or a silent h.
fn elide(s: &str) -> String {
    let mut out = String::new();
    let mut words = s.split(' ').peekable();
    let mut first = true;
    while let Some(w) = words.next() {
        if !first && !out.ends_with('\'') {
            out.push(' ');
        }
        first = false;
        let lower = w.to_lowercase();
        let next_vowel = words.peek().is_some_and(|n| {
            n.chars()
                .next()
                .is_some_and(|c| "aeiouyhAEIOUYHéèêëàâäîïôöùûüÉÈÊËÀÂÄÎÏÔÖÙÛÜ".contains(c))
        });
        if next_vowel && matches!(lower.as_str(), "de" | "le" | "la") {
            out.push_str(
                match (w.starts_with(|c: char| c.is_uppercase()), lower.as_str()) {
                    (false, "de") => "d'",
                    (true, "de") => "D'",
                    (false, _) => "l'",
                    (true, _) => "L'",
                },
            );
        } else {
            out.push_str(w);
        }
    }
    out
}

/// What to name and in which context. Build with [`Spec::new`], then call [`Spec::name`].
#[derive(Clone, Copy, Debug)]
pub struct Spec<'a> {
    pub kind: &'a str,
    /// Culture id: its place voice, words and people are used (the first culture if unknown).
    pub culture: Option<&'a str>,
    /// Name of the parent object (star of a planet, planet of a moon or station).
    pub host: &'a str,
    /// Index among siblings (0 = first).
    pub idx: usize,
    /// What the object is (anomaly kind label).
    pub label: &'a str,
}

impl<'a> Spec<'a> {
    pub fn new(kind: &'a str) -> Spec<'a> {
        Spec {
            kind,
            culture: None,
            host: "",
            idx: 0,
            label: "",
        }
    }
    pub fn culture(mut self, c: Option<&'a str>) -> Self {
        self.culture = c;
        self
    }
    pub fn host(mut self, h: &'a str) -> Self {
        self.host = h;
        self
    }
    pub fn idx(mut self, i: usize) -> Self {
        self.idx = i;
        self
    }
    pub fn label(mut self, l: &'a str) -> Self {
        self.label = l;
        self
    }

    /// The name, a pure function of the spec and `h`.
    pub fn name(&self, h: u64) -> String {
        let b = book();
        let mut rng = Rng::new(mix64(h ^ 0x4E41_4D45));
        let Some(p) = b.patterns.iter().find(|p| p.kind == self.kind) else {
            return String::new();
        };
        let culture = self.culture.and_then(|c| b.culture(c));
        let total: u64 = p.forms.iter().map(|f| u64::from(f.1)).sum();
        let mut r = rng.next_u64() % total;
        let mut form = &p.forms[0].0;
        for f in &p.forms {
            if r < u64::from(f.1) {
                form = &f.0;
                break;
            }
            r -= u64::from(f.1);
        }
        let voice = culture.map_or_else(|| b.voice(&p.voice), |c| &c.place);
        elide(&self.expand(b, form, voice, culture, &mut rng))
    }

    fn expand(
        &self,
        b: &NameBook,
        form: &str,
        voice: &Voice,
        culture: Option<&CultureNames>,
        rng: &mut Rng,
    ) -> String {
        let people = culture.unwrap_or(&b.cultures[0]);
        let mut out = String::new();
        let mut rest = form;
        while let Some(a) = rest.find('{') {
            out.push_str(&rest[..a]);
            let end = rest[a..].find('}').map_or(rest.len(), |e| a + e);
            let tok = &rest[a + 1..end];
            rest = rest.get(end + 1..).unwrap_or("");
            let (name, arg) = tok.split_once(':').unwrap_or((tok, ""));
            match name {
                "root" | "root2" => out.push_str(&syllables(voice, rng)),
                "gr" => out.push_str(pick(rng, &GREEK)),
                "rom" => out.push_str(&roman((rng.next_u64() % 12) as usize)),
                "n" => out.push_str(&(1 + rng.next_u64() % 99).to_string()),
                "n3" => out.push_str(&(100 + rng.next_u64() % 900).to_string()),
                "n4" => out.push_str(&(1000 + rng.next_u64() % 9000).to_string()),
                "cat" => {
                    let a = (b'A' + (rng.next_u64() % 26) as u8) as char;
                    let c = (b'A' + (rng.next_u64() % 26) as u8) as char;
                    out.push_str(&format!("{a}{c}-{}", 1000 + rng.next_u64() % 9000));
                }
                "l" => out.push((b'A' + (rng.next_u64() % 26) as u8) as char),
                "idx" => out.push_str(&roman(self.idx)),
                "idx_l" => out.push((b'b' + (self.idx % 25) as u8) as char),
                "host" => out.push_str(self.host),
                "label" => out.push_str(self.label),
                "given" => out.push_str(&given_in(people, rng.next_u64() & 1 == 0, rng.next_u64())),
                "surname" => out.push_str(&surname_in(people, rng.next_u64())),
                "person" => {
                    let f = rng.next_u64() & 1 == 0;
                    out.push_str(&given_in(people, f, rng.next_u64()));
                    out.push(' ');
                    out.push_str(&surname_in(people, rng.next_u64()));
                }
                "w" => out.push_str(pick(rng, b.words_of(arg, culture))),
                _ => {}
            }
        }
        out.push_str(rest);
        out
    }
}

// ---- convenience wrappers ---------------------------------------------------------------------

/// Name of a star, from a hash.
pub fn star_name(h: u64) -> String {
    Spec::new("star").name(h)
}

fn culture_or_first<'a>(b: &'a NameBook, id: &str) -> &'a CultureNames {
    b.culture(id).unwrap_or(&b.cultures[0])
}

/// Id of a culture drawn from a hash (to flavour a place whose people are not otherwise known).
pub fn culture_id(h: u64) -> &'static str {
    let b = book();
    &b.cultures[(mix64(h ^ 0x6375_6C74) % b.cultures.len() as u64) as usize].id
}

// ---- people ---------------------------------------------------------------------------------

fn given_in(c: &CultureNames, female: bool, h: u64) -> String {
    let mut rng = Rng::new(mix64(h ^ 0x6976_656E));
    let mut s = pick(&mut rng, &c.first_start).clone();
    if rng.f64() < c.first_mid_chance {
        s.push_str(pick(&mut rng, &c.first_mid));
    }
    s.push_str(pick(
        &mut rng,
        if female {
            &c.first_end_f
        } else {
            &c.first_end_m
        },
    ));
    cap(&s)
}

fn surname_in(c: &CultureNames, h: u64) -> String {
    let mut rng = Rng::new(mix64(h ^ 0x7375_726E));
    let mut s = pick(&mut rng, &c.last_start).clone();
    if rng.f64() < c.last_mid_chance {
        s.push_str(pick(&mut rng, &c.last_mid));
    }
    s.push_str(pick(&mut rng, &c.last_end));
    let mut s = cap(&s);
    if !c.particles.is_empty() && rng.f64() < c.particle_chance {
        let p = pick(&mut rng, &c.particles);
        // "Saint-" glues to the name; the others are separate words (and elide before a vowel).
        s = if p.ends_with('-') {
            format!("{p}{s}")
        } else {
            elide(&format!("{p} {s}"))
        };
    }
    s
}

/// Given name of a person of `culture`; `female` picks the ending family.
pub fn given_name(culture: &str, female: bool, h: u64) -> String {
    given_in(culture_or_first(book(), culture), female, h)
}

/// Family name (with its particle, if any).
pub fn surname(culture: &str, h: u64) -> String {
    surname_in(culture_or_first(book(), culture), h)
}

/// Nickname ("le Rouge", "Pioche"), for those who have one.
pub fn nickname(culture: &str, female: bool, h: u64) -> String {
    let c = culture_or_first(book(), culture);
    let mut rng = Rng::new(mix64(h ^ 0x6E69_636B));
    pick(&mut rng, if female { &c.nick_f } else { &c.nick_m }).clone()
}

/// Honorific title ("Docteure", "Capitaine").
pub fn title(culture: &str, female: bool, h: u64) -> String {
    let c = culture_or_first(book(), culture);
    let mut rng = Rng::new(mix64(h ^ 0x7469_746C));
    pick(&mut rng, if female { &c.title_f } else { &c.title_m }).clone()
}

/// Number of distinct (given name, surname) pairs of a culture, for either sex.
pub fn combinations(culture: &str) -> u64 {
    let c = culture_or_first(book(), culture);
    let first = |ends: usize| {
        let mids = if c.first_mid_chance > 0.0 {
            1 + c.first_mid.len()
        } else {
            1
        };
        (c.first_start.len() * mids * ends) as u64
    };
    let mids = if c.last_mid_chance > 0.0 {
        1 + c.last_mid.len()
    } else {
        1
    };
    let last = (c.last_start.len() * mids * c.last_end.len()) as u64;
    (first(c.first_end_m.len()) + first(c.first_end_f.len())) * last
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn the_embedded_grammar_is_valid() {
        let b = book();
        assert!(b.cultures.len() >= 6);
        assert!(b.patterns.len() >= KINDS.len() - 1);
    }

    #[test]
    fn a_broken_grammar_is_refused() {
        // Each of these must fail validation: the checks have teeth.
        let good = GRAMMAR;
        for (from, to) in [
            ("{gr} {root}", "{inconnu} {root}"),
            ("{w:titan}", "{w:nexiste_pas}"),
            ("\"Havre\"", "\"Hav\u{00F8}\""),
            ("mid_chance:0.55", "mid_chance:1.5"),
            (
                "(kind:\"star\", voice:\"stellar\"",
                "(kind:\"star\", voice:\"nulle\"",
            ),
            ("(kind:\"river\"", "(kind:\"riviere\""),
        ] {
            assert!(good.contains(from), "{from}");
            let broken = good.replacen(from, to, 1);
            assert!(NameBook::parse(&broken).is_err(), "{to} must be refused");
        }
    }

    #[test]
    fn names_are_deterministic_and_nonempty() {
        for kind in KINDS {
            let s = Spec::new(kind).host("Karon").idx(2).label("Epave");
            let a = s.name(7);
            assert_eq!(a, s.name(7), "{kind}");
            assert!(a.chars().count() >= 2, "{kind}: {a:?}");
        }
        assert_eq!(roman(3), "IV");
    }

    #[test]
    fn every_kind_gives_displayable_names_of_sane_length() {
        for kind in KINDS {
            for culture in [None, Some("terrane"), Some("belter"), Some("errant")] {
                for h in 0..400u64 {
                    let n = Spec::new(kind)
                        .culture(culture)
                        .host("Karon")
                        .idx((h % 5) as usize)
                        .label("Epave")
                        .name(h);
                    let len = n.chars().count();
                    assert!((2..=44).contains(&len), "{kind}: {n:?} ({len})");
                    assert!(n.chars().all(char_ok), "{kind}: {n:?}");
                    assert!(!n.contains("  ") && n.trim() == n, "{kind}: {n:?}");
                    assert!(!n.contains('{'), "{kind}: {n:?}");
                }
            }
        }
    }

    #[test]
    fn star_names_rarely_collide() {
        let set: HashSet<String> = (0..3000u64).map(star_name).collect();
        assert!(set.len() > 2600, "{} distinct of 3000", set.len());
    }

    #[test]
    fn people_are_varied_in_every_culture() {
        for c in &book().cultures {
            let full: HashSet<String> = (0..2000u64)
                .map(|h| {
                    format!(
                        "{} {}",
                        given_name(&c.id, h % 2 == 0, h * 3 + 1),
                        surname(&c.id, h * 7 + 2)
                    )
                })
                .collect();
            assert!(
                full.len() > 1850,
                "{}: {} distinct of 2000",
                c.id,
                full.len()
            );
            assert!(combinations(&c.id) > 50_000, "{}", c.id);
        }
    }

    #[test]
    fn the_sex_changes_the_ending() {
        let men: HashSet<String> = (0..300u64)
            .map(|h| given_name("terrane", false, h))
            .collect();
        let women: HashSet<String> = (0..300u64)
            .map(|h| given_name("terrane", true, h))
            .collect();
        // Endings are drawn from different lists: the sets differ a lot.
        let both = men.intersection(&women).count();
        assert!(both * 4 < men.len(), "{both} shared names");
    }

    #[test]
    fn cultures_sound_different() {
        // Same hashes, different cultures: mostly different names.
        let a: Vec<String> = (0..100u64).map(|h| surname("terrane", h)).collect();
        let b: Vec<String> = (0..100u64).map(|h| surname("jovien", h)).collect();
        let same = a.iter().zip(&b).filter(|(x, y)| x == y).count();
        assert!(same < 5, "{same}");
    }

    #[test]
    fn elision_follows_french() {
        assert_eq!(elide("Havre de Alpha"), "Havre d'Alpha");
        assert_eq!(elide("Le Aube"), "L'Aube");
        assert_eq!(elide("Cercle de Karon"), "Cercle de Karon");
        assert_eq!(elide("La Étoile"), "L'Étoile");
    }

    #[test]
    fn dead_and_living_planets_are_named_differently() {
        let dead: Vec<String> = (0..200u64)
            .map(|h| Spec::new("planet_dead").host("Karon").idx(1).name(h))
            .collect();
        let living: Vec<String> = (0..200u64)
            .map(|h| Spec::new("planet_living").host("Karon").idx(1).name(h))
            .collect();
        let with_host = |v: &[String]| v.iter().filter(|n| n.contains("Karon")).count();
        assert!(with_host(&dead) > 100, "{}", with_host(&dead));
        assert!(with_host(&living) < 60, "{}", with_host(&living));
    }
}
