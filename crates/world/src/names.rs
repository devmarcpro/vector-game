//! Procedural names from syllables.
use vector_core::Rng;

const START: [&str; 24] = [
    "Ka", "Ro", "Ven", "Ta", "Zi", "Ur", "Mel", "Dor", "An", "Sei", "Lu", "Ax", "Or", "Ith", "El",
    "Vas", "Nor", "Qui", "Ba", "Sha", "Tor", "Eri", "Cal", "Hy",
];
const MID: [&str; 14] = [
    "ra", "ne", "li", "do", "mi", "sa", "ko", "ve", "thu", "ri", "ga", "no", "ul", "ax",
];
const END: [&str; 14] = [
    "n", "s", "x", "th", "ra", "ia", "on", "is", "a", "um", "or", "eth", "ar", "us",
];

pub fn pick<'a, T>(rng: &mut Rng, list: &'a [T]) -> &'a T {
    &list[(rng.next_u64() % list.len() as u64) as usize]
}

/// Pronounceable name of 2-3 syllables.
pub fn star_name(rng: &mut Rng) -> String {
    let mut s = String::from(*pick(rng, &START));
    if rng.f64() < 0.55 {
        s.push_str(pick(rng, &MID));
    }
    s.push_str(pick(rng, &END));
    s
}

pub fn roman(n: usize) -> String {
    const R: [&str; 12] = [
        "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII",
    ];
    R.get(n)
        .map_or_else(|| format!("{}", n + 1), |s| (*s).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_are_deterministic_and_nonempty() {
        let a = star_name(&mut Rng::new(7));
        assert_eq!(a, star_name(&mut Rng::new(7)));
        assert!(a.len() >= 3);
        assert_eq!(roman(3), "IV");
    }
}
