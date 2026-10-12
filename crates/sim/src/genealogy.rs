//! Genealogy and genetics (E3). Everybody belongs to a family tree that is a pure function of their
//! identity `Kin { fam, k }`: the k-th child of family `fam`. A family's parents are two members
//! of other families (derived from its key), its size is 2 to 5, even ranks are men and odd ranks
//! women. Ancestors, siblings, uncles and cousins therefore exist, with names, ages and genomes,
//! without being stored. Only families created during play (a recruited cousin, a birth aboard)
//! are written to a small registry that overrides the derivation.
//! Genomes are recombined from the parents' (Mendel: each parent hands over one allele of each
//! gene, at random), with rare mutations. The generation is written in the high byte of a family
//! key (founders are generation 0 and get random alleles), so a person's genome, name and culture
//! are the same whoever asks and from wherever.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vector_core::{hash_coord, mix64, Rng};
use vector_data::GameData;

/// Generation of the people met at the start of the game; their ancestors go back to generation 0.
pub const ADULT_GENERATION: u32 = 6;
/// Chance that an allele handed to a child mutates (becomes a random one).
pub const MUTATION: f64 = 1.0 / 200.0;

/// Identity of a person: the `k`-th child of family `fam` (even `k`: man, odd: woman).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Kin {
    pub fam: u64,
    pub k: u8,
}

impl Kin {
    pub fn male(self) -> bool {
        self.k.is_multiple_of(2)
    }

    fn h(self, salt: u64) -> u64 {
        hash_coord(self.fam ^ salt, i64::from(self.k), 0x4B1, 0)
    }
}

/// Families created during play: family key -> (father, mother, number of children).
pub type Families = BTreeMap<u64, (Kin, Kin, u8)>;

/// Generation of a family: 0 for the founders, the parents' plus one below.
pub fn generation(fam: u64) -> u32 {
    (fam >> 56) as u32
}

/// A family key of the given generation.
pub fn make_fam(generation: u32, h: u64) -> u64 {
    (u64::from(generation) << 56) | (h & 0x00FF_FFFF_FFFF_FFFF)
}

use vector_core::unit;

pub fn children_count(fams: &Families, fam: u64) -> u8 {
    fams.get(&fam)
        .map_or_else(|| 2 + (mix64(fam ^ 0xC417) % 4) as u8, |f| f.2)
}

/// Father and mother of a person.
pub fn parents(fams: &Families, kin: Kin) -> (Kin, Kin) {
    if let Some(&(f, m, _)) = fams.get(&kin.fam) {
        return (f, m);
    }
    let g = generation(kin.fam).saturating_sub(1);
    let (ff, fm) = (
        make_fam(g, mix64(kin.fam ^ 0xFA)),
        make_fam(g, mix64(kin.fam ^ 0xAA)),
    );
    let (nf, nm) = (children_count(fams, ff), children_count(fams, fm));
    (
        Kin {
            fam: ff,
            k: 2 * (mix64(kin.fam ^ 0x1D) % u64::from(nf.div_ceil(2))) as u8,
        },
        Kin {
            fam: fm,
            k: 1 + 2 * (mix64(kin.fam ^ 0x2D) % u64::from(nm / 2)) as u8,
        },
    )
}

/// The ancestors by generation: `[0]` is the person, `[1]` the parents, `[2]` the grandparents...
pub fn ancestors(fams: &Families, kin: Kin, gens: usize) -> Vec<Vec<Kin>> {
    let mut out = vec![vec![kin]];
    for g in 0..gens {
        let next: Vec<Kin> = out[g]
            .iter()
            .flat_map(|&k| {
                let (f, m) = parents(fams, k);
                [f, m]
            })
            .collect();
        out.push(next);
    }
    out
}

/// Brothers and sisters.
pub fn siblings(fams: &Families, kin: Kin) -> Vec<Kin> {
    (0..children_count(fams, kin.fam))
        .filter(|&k| k != kin.k)
        .map(|k| Kin { fam: kin.fam, k })
        .collect()
}

// ---- genomes ------------------------------------------------------------------------------

/// Two alleles (paternal, maternal) of each gene, 0..3: `genome[2 * gene]`, `genome[2 * gene + 1]`.
pub type Genome = Vec<u8>;

/// Shift one random allele of one random gene by one step (a mutation). Returns the gene and the
/// change actually made (+1 or -1).
pub fn mutate(data: &GameData, genome: &mut Genome, rng: &mut Rng) -> (usize, i8) {
    let gene = (rng.next_u64() % data.genes.len() as u64) as usize;
    let slot = 2 * gene + (rng.next_u64() % 2) as usize;
    let mut dir: i8 = if rng.next_u64().is_multiple_of(2) {
        1
    } else {
        -1
    };
    let cur = genome[slot] as i8;
    if !(0..=3).contains(&(cur + dir)) {
        dir = -dir;
    }
    genome[slot] = (cur + dir) as u8;
    (gene, dir)
}

fn founder_genome(data: &GameData, kin: Kin) -> Genome {
    let mut g = Vec::with_capacity(data.genes.len() * 2);
    for (i, gene) in data.genes.iter().enumerate() {
        for side in 0..2u64 {
            let h = kin.h(0x6E4E ^ (i as u64) << 8 ^ side);
            // Harmful alleles of the predispositions are rarer (about 1 in 10).
            let a = if gene.is_predisposition() {
                if unit(h) < 0.10 {
                    0
                } else {
                    1 + (mix64(h) % 3) as u8
                }
            } else {
                (h % 4) as u8
            };
            g.push(a);
        }
    }
    g
}

/// The genome of a person: random for founders (generation 0), else recombined from the
/// parents' genomes.
pub fn genome(fams: &Families, data: &GameData, kin: Kin) -> Genome {
    if generation(kin.fam) == 0 {
        return founder_genome(data, kin);
    }
    let (f, m) = parents(fams, kin);
    child_genome(data, kin, &genome(fams, data, f), &genome(fams, data, m))
}

/// The genome `kin` gets from parents with genomes `gf` and `gm` (Mendel plus rare mutations).
pub fn child_genome(data: &GameData, kin: Kin, gf: &[u8], gm: &[u8]) -> Genome {
    let mut g = Vec::with_capacity(gf.len());
    for i in 0..data.genes.len() {
        for (side, parent) in [(1u64, gf), (2u64, gm)] {
            let h = kin.h(0x3E40 ^ (i as u64) << 8 ^ side);
            let mut a = parent[2 * i + (h & 1) as usize];
            if unit(mix64(h)) < MUTATION {
                a = (mix64(h ^ 0x77) % 4) as u8;
            }
            g.push(a);
        }
    }
    g
}

/// Sum of a trait key over the genes of a genome: weight x (2 x expression - 1) for additive genes.
pub fn gene_effect(data: &GameData, genome: &[u8], key: &str) -> f64 {
    data.genes
        .iter()
        .enumerate()
        .filter(|(_, g)| !g.is_predisposition())
        .map(|(i, g)| {
            let w: f64 = g
                .effects
                .iter()
                .filter(|(k, _)| k == key)
                .map(|(_, w)| w)
                .sum();
            w * (2.0 * vector_data::Gene::expression(genome[2 * i], genome[2 * i + 1]) - 1.0)
        })
        .sum()
}

/// Chance multiplier of a disease for this genome (product over the predispositions linked to it).
pub fn disease_risk(data: &GameData, genome: &[u8], disease_id: &str) -> f64 {
    data.genes
        .iter()
        .enumerate()
        .filter(|(_, g)| g.disease == disease_id)
        .map(|(i, g)| g.risk(genome[2 * i], genome[2 * i + 1]))
        .product()
}

/// How alike two genomes are, 0..1 (1 minus the mean difference of the genes' expressions).
pub fn similarity(a: &[u8], b: &[u8]) -> f64 {
    let n = a.len() / 2;
    let diff: f64 = (0..n)
        .map(|i| {
            let e = |g: &[u8]| f64::from(g[2 * i] + g[2 * i + 1]) / 6.0;
            (e(a) - e(b)).abs()
        })
        .sum();
    1.0 - diff / n.max(1) as f64
}

// ---- names, culture, life span ------------------------------------------------------------

/// Culture of a person: that of the father (85 %) or of the mother, back to the founders.
pub fn culture_of(fams: &Families, data: &GameData, kin: Kin) -> usize {
    let mut k = kin;
    while generation(k.fam) > 0 {
        let (f, m) = parents(fams, k);
        k = if unit(k.h(0xC018)) < 0.85 { f } else { m };
    }
    (k.h(0xC017) % data.cultures.len() as u64) as usize
}

/// Family name, handed down from father to child, from the founder of the male line.
pub fn surname(fams: &Families, data: &GameData, kin: Kin) -> String {
    let mut k = kin;
    while generation(k.fam) > 0 {
        k = parents(fams, k).0;
    }
    let c = &data.cultures[culture_of(fams, data, k)];
    vector_world::names::surname(&c.id, k.h(0x5A1))
}

pub fn given_name(fams: &Families, data: &GameData, kin: Kin) -> String {
    let c = &data.cultures[culture_of(fams, data, kin)];
    vector_world::names::given_name(&c.id, !kin.male(), kin.h(0x6A1))
}

pub fn full_name(fams: &Families, data: &GameData, kin: Kin) -> String {
    format!(
        "{} {}",
        given_name(fams, data, kin),
        surname(fams, data, kin)
    )
}

/// Age at death, years: 62 to 91, moved by the longevity genes.
pub fn lifespan(fams: &Families, data: &GameData, kin: Kin) -> f64 {
    let g = genome(fams, data, kin);
    62.0 + 29.0 * unit(kin.h(0x11FE)) + gene_effect(data, &g, "lifespan")
}

// ---- relationships ------------------------------------------------------------------------

/// What `b` is to `a`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    /// An ancestor `n` generations up (1 = parent).
    Ancestor(u8),
    /// A descendant `n` generations down (1 = child).
    Descendant(u8),
    Sibling,
    /// A sibling of an ancestor `n` generations up (1 = uncle or aunt).
    UncleAunt(u8),
    /// A descendant of a sibling, `n` generations down (1 = nephew or niece).
    NephewNiece(u8),
    Cousin {
        degree: u8,
        removed: u8,
    },
}

impl Relation {
    /// French label for a relative of the given sex.
    pub fn label(self, male: bool) -> String {
        let (m, f) = match self {
            Relation::Ancestor(1) => ("père", "mère"),
            Relation::Ancestor(2) => ("grand-père", "grand-mère"),
            Relation::Ancestor(3) => ("arrière-grand-père", "arrière-grand-mère"),
            Relation::Ancestor(_) => ("aïeul", "aïeule"),
            Relation::Descendant(1) => ("fils", "fille"),
            Relation::Descendant(2) => ("petit-fils", "petite-fille"),
            Relation::Descendant(_) => ("descendant", "descendante"),
            Relation::Sibling => ("frère", "sœur"),
            Relation::UncleAunt(1) => ("oncle", "tante"),
            Relation::UncleAunt(_) => ("grand-oncle", "grand-tante"),
            Relation::NephewNiece(1) => ("neveu", "nièce"),
            Relation::NephewNiece(_) => ("petit-neveu", "petite-nièce"),
            Relation::Cousin {
                degree: 1,
                removed: 0,
            } => ("cousin germain", "cousine germaine"),
            Relation::Cousin { degree: 1, .. } => ("cousin éloigné", "cousine éloignée"),
            Relation::Cousin { degree: 2, .. } => {
                ("cousin issu de germain", "cousine issue de germain")
            }
            Relation::Cousin { .. } => ("cousin lointain", "cousine lointaine"),
        };
        (if male { m } else { f }).to_string()
    }
}

/// What `b` is to `a`, looking up to 4 generations up in both trees; None if unrelated within that.
pub fn relationship(fams: &Families, a: Kin, b: Kin) -> Option<Relation> {
    if a == b {
        return None;
    }
    if a.fam == b.fam {
        return Some(Relation::Sibling);
    }
    let (aa, ab) = (ancestors(fams, a, 4), ancestors(fams, b, 4));
    // Search by total distance; "shared family but different people" counts one generation more
    // (siblings of the common ancestor rather than the ancestor itself).
    let mut best: Option<(usize, usize, usize)> = None;
    for (i, gen_a) in aa.iter().enumerate() {
        for (j, gen_b) in ab.iter().enumerate() {
            for &x in gen_a {
                for &y in gen_b {
                    let eff = if x == y {
                        Some((i, j))
                    } else if x.fam == y.fam {
                        Some((i + 1, j + 1))
                    } else {
                        None
                    };
                    if let Some((ei, ej)) = eff {
                        if best.is_none_or(|(s, _, _)| ei + ej < s) {
                            best = Some((ei + ej, ei, ej));
                        }
                    }
                }
            }
        }
    }
    let (_, ei, ej) = best?;
    let g = |n: usize| n as u8;
    Some(match (ei, ej) {
        (_, 0) => Relation::Ancestor(g(ei)),
        (0, _) => Relation::Descendant(g(ej)),
        (1, 1) => Relation::Sibling,
        (1, _) => Relation::NephewNiece(g(ej - 1)),
        (_, 1) => Relation::UncleAunt(g(ei - 1)),
        _ => Relation::Cousin {
            degree: g(ei.min(ej) - 1),
            removed: g(ei.abs_diff(ej)),
        },
    })
}

// ---- families created in play -------------------------------------------------------------

/// The key of the family of a couple: a pure function of the two, one generation below the later.
pub fn family_key(father: Kin, mother: Kin) -> u64 {
    let g = generation(father.fam).max(generation(mother.fam)) + 1;
    make_fam(
        g,
        hash_coord(
            father.fam ^ mother.fam.rotate_left(17),
            i64::from(father.k),
            i64::from(mother.k),
            0xFA3,
        ),
    )
}

/// Register the family of a couple and return its key. `children` is 1 to 5.
pub fn new_family(fams: &mut Families, father: Kin, mother: Kin, children: u8) -> u64 {
    let fam = family_key(father, mother);
    fams.insert(fam, (father, mother, children.max(1)));
    fam
}

/// A person who marries into the tree: a founder-like member of a fresh family.
pub fn spouse_of(partner: Kin, salt: u64) -> Kin {
    Kin {
        fam: make_fam(
            generation(partner.fam),
            mix64(partner.fam ^ salt ^ u64::from(partner.k) << 40),
        ),
        k: if partner.male() { 1 } else { 0 },
    }
}

/// The family of an uncle or aunt `u`: (key, father, mother, number of children). The spouse
/// and the size follow from `u` alone, so a cousin recruited later is the one the pedigree shows.
pub fn uncle_family(u: Kin) -> (u64, Kin, Kin, u8) {
    let s = spouse_of(u, 0xC0B5);
    let (father, mother) = if u.male() { (u, s) } else { (s, u) };
    let n = 2 + (mix64(u.fam ^ u64::from(u.k) ^ 0xC0C0) % 3) as u8;
    (family_key(father, mother), father, mother, n)
}

/// A family to register: key, father, mother and number of children.
pub type FamilyDef = (u64, Kin, Kin, u8);

/// A sibling or a cousin of `anchor`, chosen by `pick`. Returns the relative and, for a cousin,
/// the family to register so that the relationship is real.
pub fn relative_of(fams: &Families, anchor: Kin, pick: u64) -> Option<(Kin, Option<FamilyDef>)> {
    let sibs = siblings(fams, anchor);
    if pick.is_multiple_of(2) && !sibs.is_empty() {
        return Some((sibs[(pick / 2 % sibs.len() as u64) as usize], None));
    }
    // A cousin: a child of one of the anchor's uncles or aunts.
    let (f, m) = parents(fams, anchor);
    let parent = if (pick / 2).is_multiple_of(2) { f } else { m };
    let uncles = siblings(fams, parent);
    if uncles.is_empty() {
        return sibs.first().map(|&s| (s, None));
    }
    let u = uncles[(pick / 4 % uncles.len() as u64) as usize];
    let (fam, father, mother, n) = uncle_family(u);
    let k = (mix64(pick ^ 0xA11) % u64::from(n)) as u8;
    Some((Kin { fam, k }, Some((fam, father, mother, n))))
}

// ---- the pedigree of a person --------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Relative {
    pub kin: Kin,
    pub relation: Relation,
    pub name: String,
    pub age: f64,
    pub alive: bool,
    /// Age at death if dead, else None.
    pub died_at: Option<f64>,
    pub culture: usize,
    /// How alike the genome is to the person's, 0..1.
    pub similarity: f64,
    /// The relative this one hangs from in the tree: a grandparent is reached through the child
    /// that is the person's parent, an uncle through the parent, a cousin through the uncle.
    pub via: Option<Kin>,
}

#[derive(Clone, Debug, Default)]
pub struct Pedigree {
    pub grandparents: Vec<Relative>,
    pub parents: Vec<Relative>,
    pub siblings: Vec<Relative>,
    pub uncles: Vec<Relative>,
    pub cousins: Vec<Relative>,
}

/// The close family of `me` (aged `age` years): grandparents, parents, siblings, uncles, aunts and
/// first cousins, with names, ages, whether they still live and how much they resemble `me`.
pub fn pedigree(fams: &Families, data: &GameData, me: Kin, age: f64) -> Pedigree {
    let my_genome = genome(fams, data, me);
    let mut tmp = fams.clone();
    let mut out = Pedigree::default();
    let make = |tmp: &Families, kin: Kin, age: f64, relation: Relation, via: Option<Kin>| {
        let span = lifespan(tmp, data, kin);
        Relative {
            kin,
            relation,
            name: full_name(tmp, data, kin),
            age: age.min(span),
            alive: age < span,
            died_at: (age >= span).then_some(span),
            culture: culture_of(tmp, data, kin),
            similarity: similarity(&my_genome, &genome(tmp, data, kin)),
            via,
        }
    };
    let (father, mother) = parents(fams, me);
    let ages = [
        age + 22.0 + (mix64(me.fam ^ 0xA6E1) % 14) as f64,
        age + 20.0 + (mix64(me.fam ^ 0xA6E2) % 12) as f64,
    ];
    for (i, &p) in [father, mother].iter().enumerate() {
        out.parents
            .push(make(&tmp, p, ages[i], Relation::Ancestor(1), None));
        let (gf, gm) = parents(fams, p);
        for (j, &gp) in [gf, gm].iter().enumerate() {
            let gage = ages[i] + 21.0 + (mix64(p.fam ^ 0x6A00 ^ j as u64) % 12) as f64;
            out.grandparents
                .push(make(&tmp, gp, gage, Relation::Ancestor(2), Some(p)));
        }
        // Uncles and aunts, and their children.
        for u in siblings(fams, p) {
            let uage = ages[i] + (f64::from(p.k) - f64::from(u.k)) * 2.0;
            out.uncles
                .push(make(&tmp, u, uage, Relation::UncleAunt(1), Some(p)));
            let (_, uf, um, n) = uncle_family(u);
            let cousin_fam = new_family(&mut tmp, uf, um, n);
            for k in 0..n {
                let c = Kin { fam: cousin_fam, k };
                let cage = (uage - 24.0 - f64::from(k) * 2.0).max(0.5);
                let rel = Relation::Cousin {
                    degree: 1,
                    removed: 0,
                };
                out.cousins.push(make(&tmp, c, cage, rel, Some(u)));
            }
        }
    }
    for s in siblings(fams, me) {
        let sage = (age - (f64::from(s.k) - f64::from(me.k)) * 2.0).max(0.5);
        out.siblings
            .push(make(&tmp, s, sage, Relation::Sibling, None));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameData {
        GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
            .expect("data")
    }

    fn kin(i: u64) -> Kin {
        let fam = make_fam(ADULT_GENERATION, mix64(i ^ 0xFA11));
        let n = children_count(&Families::new(), fam);
        Kin {
            fam,
            k: (mix64(i) % u64::from(n)) as u8,
        }
    }

    #[test]
    fn families_are_pure_have_two_to_five_children_and_proper_parents() {
        let fams = Families::new();
        for i in 0..300 {
            let a = kin(i);
            assert_eq!(parents(&fams, a), parents(&fams, a));
            let n = children_count(&fams, a.fam);
            assert!((2..=5).contains(&n) && a.k < n);
            let (f, m) = parents(&fams, a);
            assert!(f.male() && !m.male(), "father is a man, mother a woman");
            assert!(f.k < children_count(&fams, f.fam) && m.k < children_count(&fams, m.fam));
            // Brothers and sisters share both parents.
            for s in siblings(&fams, a) {
                assert_eq!(parents(&fams, s), (f, m));
            }
            assert_eq!(siblings(&fams, a).len() as u8, n - 1);
        }
    }

    #[test]
    fn children_receive_one_allele_of_each_gene_from_each_parent() {
        let (d, fams) = (data(), Families::new());
        let (mut total, mut foreign) = (0u32, 0u32);
        for i in 0..400 {
            let a = kin(i);
            let (f, m) = parents(&fams, a);
            let (g, gf, gm) = (
                genome(&fams, &d, a),
                genome(&fams, &d, f),
                genome(&fams, &d, m),
            );
            for gene in 0..d.genes.len() {
                total += 2;
                if ![gf[2 * gene], gf[2 * gene + 1]].contains(&g[2 * gene]) {
                    foreign += 1;
                }
                if ![gm[2 * gene], gm[2 * gene + 1]].contains(&g[2 * gene + 1]) {
                    foreign += 1;
                }
            }
        }
        let rate = f64::from(foreign) / f64::from(total);
        // Only mutations bring in an allele the parents do not have (and only 3 times out of 4).
        assert!(rate > 0.0004 && rate < 0.01, "mutation rate {rate}");
    }

    #[test]
    fn kin_resemble_each_other_more_than_strangers() {
        let (d, fams) = (data(), Families::new());
        let (mut sib, mut cousin, mut stranger, mut n) = (0.0, 0.0, 0.0, 0.0);
        for i in 0..250 {
            let a = kin(i);
            let ga = genome(&fams, &d, a);
            let s = siblings(&fams, a)[0];
            sib += similarity(&ga, &genome(&fams, &d, s));
            // A cousin through the registry.
            let (c, reg) = relative_of(&fams, a, 5 + 8 * i).expect("relative");
            let mut f2 = fams.clone();
            if let Some((fam, fa, mo, nn)) = reg {
                f2.insert(fam, (fa, mo, nn));
            }
            cousin += similarity(&ga, &genome(&f2, &d, c));
            stranger += similarity(&ga, &genome(&fams, &d, kin(10_000 + i)));
            n += 1.0;
        }
        let (sib, cousin, stranger) = (sib / n, cousin / n, stranger / n);
        assert!(
            sib > cousin && cousin > stranger,
            "{sib} {cousin} {stranger}"
        );
        assert!(
            sib > stranger + 0.05,
            "siblings are clearly closer: {sib} vs {stranger}"
        );
    }

    #[test]
    fn relationships_are_recognised_in_both_directions() {
        let (d, mut fams) = (data(), Families::new());
        let me = kin(7);
        let (f, m) = parents(&fams, me);
        let (gf, _) = parents(&fams, f);
        let sib = siblings(&fams, me)[0];
        assert_eq!(relationship(&fams, me, f), Some(Relation::Ancestor(1)));
        assert_eq!(relationship(&fams, f, me), Some(Relation::Descendant(1)));
        assert_eq!(relationship(&fams, me, gf), Some(Relation::Ancestor(2)));
        assert_eq!(relationship(&fams, me, sib), Some(Relation::Sibling));
        assert_eq!(relationship(&fams, me, kin(9_999)), None);
        // Uncles and aunts: siblings of a parent; their children are first cousins.
        let uncle = siblings(&fams, m)[0];
        assert_eq!(relationship(&fams, me, uncle), Some(Relation::UncleAunt(1)));
        assert_eq!(
            relationship(&fams, uncle, me),
            Some(Relation::NephewNiece(1))
        );
        let (cousin, reg) = relative_of(&fams, me, 7).expect("relative");
        if let Some((fam, fa, mo, n)) = reg {
            fams.insert(fam, (fa, mo, n));
            assert_eq!(
                relationship(&fams, me, cousin),
                Some(Relation::Cousin {
                    degree: 1,
                    removed: 0
                }),
                "{cousin:?}"
            );
            assert_eq!(
                relationship(&fams, cousin, me),
                Some(Relation::Cousin {
                    degree: 1,
                    removed: 0
                })
            );
        }
        // Labels follow the sex.
        assert_eq!(Relation::Ancestor(1).label(true), "père");
        assert_eq!(Relation::Ancestor(1).label(false), "mère");
        assert_eq!(
            Relation::Cousin {
                degree: 1,
                removed: 0
            }
            .label(false),
            "cousine germaine"
        );
        let _ = d;
    }

    #[test]
    fn surnames_pass_from_father_to_children_and_names_are_stable() {
        let (d, fams) = (data(), Families::new());
        for i in 0..80 {
            let a = kin(i);
            let (f, _) = parents(&fams, a);
            assert_eq!(surname(&fams, &d, a), surname(&fams, &d, f));
            for s in siblings(&fams, a) {
                assert_eq!(surname(&fams, &d, s), surname(&fams, &d, a));
            }
            assert_eq!(full_name(&fams, &d, a), full_name(&fams, &d, a));
        }
        let names: std::collections::BTreeSet<String> =
            (0..80).map(|i| full_name(&fams, &d, kin(i))).collect();
        assert!(names.len() > 60, "{} distinct names", names.len());
    }

    #[test]
    fn the_pedigree_lists_the_close_family_with_plausible_ages() {
        let (d, fams) = (data(), Families::new());
        let me = kin(21);
        let p = pedigree(&fams, &d, me, 35.0);
        assert_eq!(p.parents.len(), 2);
        assert_eq!(p.grandparents.len(), 4);
        assert_eq!(p.siblings.len(), siblings(&fams, me).len());
        assert!(!p.uncles.is_empty() && !p.cousins.is_empty());
        for parent in &p.parents {
            assert!(parent.age >= 35.0 + 18.0 || !parent.alive, "{}", parent.age);
        }
        for gp in &p.grandparents {
            assert!(gp.age > 55.0, "{}", gp.age);
        }
        // Everybody has a name, the right surname, and relatives resemble the person.
        let family_name = surname(&fams, &d, me);
        assert!(
            p.parents[0].name.ends_with(&family_name),
            "{} / {}",
            p.parents[0].name,
            family_name
        );
        assert!(p.siblings.iter().all(|s| s.name.ends_with(&family_name)));
        assert!(p
            .parents
            .iter()
            .all(|r| (0.5..=1.0).contains(&r.similarity)));
        // Dead relatives carry their age at death.
        for r in p.grandparents.iter().chain(&p.parents) {
            assert_eq!(r.alive, r.died_at.is_none());
        }
    }

    #[test]
    fn genes_move_traits_and_predispositions_move_disease_risk() {
        let d = data();
        // A person with the best alleles everywhere versus the worst.
        let best: Genome = vec![3; d.genes.len() * 2];
        let worst: Genome = vec![0; d.genes.len() * 2];
        assert!(gene_effect(&d, &best, "health_base") > 0.09);
        assert!(gene_effect(&d, &worst, "health_base") < -0.09);
        assert!(gene_effect(&d, &best, "lifespan") > 17.0);
        assert!(gene_effect(&d, &best, "skill:science") > 1.0);
        assert_eq!(gene_effect(&d, &best, "no_such_key"), 0.0);
        assert!((disease_risk(&d, &worst, "cancer_precoce") - 4.0).abs() < 1e-12);
        assert!((disease_risk(&d, &best, "cancer_precoce") - 1.0).abs() < 1e-12);
        let mut carrier = best.clone();
        let g = d
            .genes
            .iter()
            .position(|g| g.disease == "cancer_precoce")
            .expect("gene");
        carrier[2 * g] = 0;
        assert!((disease_risk(&d, &carrier, "cancer_precoce") - 1.5).abs() < 1e-12);
        assert_eq!(disease_risk(&d, &best, "grippe"), 1.0, "no gene, no change");
    }
}
