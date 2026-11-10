//! Penney's game.
//!
//! Two players choose distinct coin-flip patterns of equal length; a fair
//! coin is flipped until one of the patterns appears. The second player can
//! always choose a pattern that beats the first, because "beats" is not
//! transitive.
//!
//! The simulation keeps the last `L` flips in a rolling bit window, so a flip
//! costs one shift and two compares and nothing is allocated. A trial lasts
//! about as long as it takes for a fixed pattern to appear, which for
//! patterns of length `L` is of order `2^L` flips; long patterns are therefore
//! slow to simulate.

use rand::RngExt;

use crate::engine::{Experiment, Kind, Quantity, Sample};
use crate::error::{Result, invalid};
use crate::params::Sequence;
use crate::rng::SimRng;

/// Conway's leading number `c(x, y) = Σ_{k=1}^{L} 2^(k−1) · [last k symbols
/// of x = first k symbols of y]` for patterns of equal length `L`.
///
/// ```
/// use aporia::Sequence;
/// use aporia::experiments::penney::leading_number;
/// let hhh: Sequence = "HHH".parse()?;
/// assert_eq!(leading_number(hhh, hhh), 7);
/// # Ok::<(), aporia::Error>(())
/// ```
#[must_use]
pub fn leading_number(x: Sequence, y: Sequence) -> u64 {
    let len = x.len().min(y.len());
    (1..=len)
        .filter(|&k| {
            let suffix = x.bits() & ((1 << k) - 1);
            let prefix = y.bits() >> (y.len() - k);
            suffix == prefix
        })
        .map(|k| 1u64 << (k - 1))
        .sum()
}

/// Exact probability that `second` appears before `first`, by Conway's
/// formula: with `a = c(A,A) − c(A,B)` and `b = c(B,B) − c(B,A)` the odds
/// for B against A are `a : b`.
///
/// Both differences are positive for distinct patterns of equal length:
/// `c(A,A)` contains the term `2^(L−1)`, while `c(A,B)` is at most
/// `2^(L−1) − 1` because the full-length term would require `A = B`.
#[must_use]
#[allow(clippy::cast_precision_loss)] // a, b < 2^17, exact in f64
pub fn second_wins(first: Sequence, second: Sequence) -> f64 {
    let a = leading_number(first, first) - leading_number(first, second);
    let b = leading_number(second, second) - leading_number(second, first);
    a as f64 / (a + b) as f64
}

/// The pattern that maximises the second player's winning probability
/// against `first`, found by exhaustive search over all `2^L` patterns with
/// the exact formula. Ties go to the pattern that comes first in
/// lexicographic order (`H < T`). For `L = 1` the only reply is the other symbol.
///
/// ```
/// use aporia::experiments::penney::best_response;
/// let (reply, p) = best_response("HHT".parse()?);
/// assert_eq!(reply.to_string(), "THH");
/// assert_eq!(p, 0.75);
/// # Ok::<(), aporia::Error>(())
/// ```
#[must_use]
pub fn best_response(first: Sequence) -> (Sequence, f64) {
    let len = first.len();
    let mut best = (first, -1.0);
    for bits in 0..(1u32 << len) {
        if bits == first.bits() {
            continue;
        }
        // bits < 2^len and len is valid, so construction cannot fail.
        let Ok(candidate) = Sequence::from_bits(bits, len) else {
            continue;
        };
        let p = second_wins(first, candidate);
        if p > best.1 {
            best = (candidate, p);
        }
    }
    best
}

/// Validated parameters of Penney's game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Penney {
    first: Sequence,
    second: Sequence,
    show_best_response: bool,
}

impl Penney {
    /// Validates that the patterns are distinct and of equal length.
    ///
    /// ```
    /// use aporia::experiments::Penney;
    /// assert!(Penney::new("HHT".parse()?, "THH".parse()?).is_ok());
    /// assert!(Penney::new("HHT".parse()?, "HHT".parse()?).is_err());
    /// assert!(Penney::new("HH".parse()?, "THH".parse()?).is_err());
    /// # Ok::<(), aporia::Error>(())
    /// ```
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidParameter`] otherwise.
    pub fn new(first: Sequence, second: Sequence) -> Result<Self> {
        if first.len() != second.len() {
            return Err(invalid("second", "must have the same length as first"));
        }
        if first == second {
            return Err(invalid("second", "must differ from first"));
        }
        Ok(Self {
            first,
            second,
            show_best_response: false,
        })
    }

    /// Also report the exact best response to the first pattern.
    #[must_use]
    pub fn with_best_response(mut self) -> Self {
        self.show_best_response = true;
        self
    }

    /// Exact probability that the second pattern appears first.
    #[must_use]
    pub fn exact(&self) -> f64 {
        second_wins(self.first, self.second)
    }
}

impl Default for Penney {
    fn default() -> Self {
        Self {
            first: Sequence::from_bits_const(0b001, 3),  // HHT
            second: Sequence::from_bits_const(0b100, 3), // THH
            show_best_response: false,
        }
    }
}

struct Race(Penney);

impl Quantity for Race {
    fn label(&self) -> &'static str {
        "P(second wins)"
    }

    fn kind(&self) -> Kind {
        Kind::Binary
    }

    fn exact(&self) -> Option<f64> {
        Some(self.0.exact())
    }

    fn draw(&mut self, rng: &mut SimRng) -> Result<Sample> {
        let (first, second) = (self.0.first, self.0.second);
        let mask = first.mask();
        // Before L flips have been made no pattern can be complete, so the
        // first L − 1 flips only fill the window.
        let mut window = 0u32;
        for _ in 1..first.len() {
            window = (window << 1) | u32::from(rng.random::<bool>());
        }
        loop {
            window = ((window << 1) | u32::from(rng.random::<bool>())) & mask;
            // Equal lengths and distinct patterns: at most one can match.
            if window == first.bits() {
                return Ok(Sample::Hit(false));
            }
            if window == second.bits() {
                return Ok(Sample::Hit(true));
            }
        }
    }
}

impl Experiment for Penney {
    fn id(&self) -> &'static str {
        "penney"
    }

    fn title(&self) -> &'static str {
        "Penney's game"
    }

    fn intuition(&self) -> &'static str {
        "A fair coin makes it a fair game, and some pattern must be the best one to pick."
    }

    fn lesson(&self) -> String {
        "\"Beats\" is not transitive: every pattern of length three or more has a pattern \
         that beats it, so the player who chooses second can always counter. The overlaps \
         of a pattern with itself and with its rival, measured by Conway's leading numbers, \
         decide the race."
            .to_owned()
    }

    fn default_trials(&self) -> u64 {
        1_000_000
    }

    fn parameters(&self) -> Vec<(&'static str, String)> {
        vec![
            ("first", self.first.to_string()),
            ("second", self.second.to_string()),
        ]
    }

    fn quantities(&self) -> Vec<Box<dyn Quantity>> {
        vec![Box::new(Race(*self))]
    }

    fn notes(&self) -> Vec<String> {
        let (a, b) = (self.first, self.second);
        let mut notes = vec![format!(
            "Leading numbers: c(A,A) = {}, c(A,B) = {}, c(B,B) = {}, c(B,A) = {}; \
             odds for the second pattern {}:{}.",
            leading_number(a, a),
            leading_number(a, b),
            leading_number(b, b),
            leading_number(b, a),
            leading_number(a, a) - leading_number(a, b),
            leading_number(b, b) - leading_number(b, a),
        )];
        let cycle: Vec<Sequence> = [0b100, 0b001, 0b011, 0b110]
            .iter()
            .filter_map(|&bits| Sequence::from_bits(bits, 3).ok())
            .collect();
        let links: Vec<String> = cycle
            .iter()
            .zip(cycle.iter().cycle().skip(1))
            .map(|(&winner, &loser)| {
                format!("{winner} beats {loser} ({:.6})", second_wins(loser, winner))
            })
            .collect();
        notes.push(format!("Non-transitive cycle: {}.", links.join(", ")));
        if self.show_best_response {
            let (reply, p) = best_response(self.first);
            notes.push(format!(
                "Best response to {}: {reply}, winning with probability {p:.6}.",
                self.first
            ));
        }
        notes
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // the oracle values are exact dyadic fractions
mod tests {
    use super::*;
    use crate::experiments::testing::assert_simulation_matches_exact;

    fn seq(s: &str) -> Sequence {
        s.parse().unwrap()
    }

    #[test]
    fn oracles() {
        let cases = [
            ("HHH", "THH", 7.0 / 8.0),
            ("HHT", "THH", 3.0 / 4.0),
            ("HTH", "HHT", 2.0 / 3.0),
            ("HTT", "HHT", 2.0 / 3.0),
            ("THH", "TTH", 2.0 / 3.0),
            ("THT", "TTH", 2.0 / 3.0),
            ("TTH", "HTT", 3.0 / 4.0),
            ("TTT", "HTT", 7.0 / 8.0),
        ];
        for (a, b, p) in cases {
            assert!(
                (second_wins(seq(a), seq(b)) - p).abs() < 1e-15,
                "{a} vs {b}"
            );
        }
    }

    #[test]
    fn non_transitive_cycle() {
        let beats = |w: &str, l: &str| second_wins(seq(l), seq(w));
        assert_eq!(beats("THH", "HHT"), 0.75);
        assert!((beats("HHT", "HTT") - 2.0 / 3.0).abs() < 1e-15);
        assert_eq!(beats("HTT", "TTH"), 0.75);
        assert!((beats("TTH", "THH") - 2.0 / 3.0).abs() < 1e-15);
    }

    /// Independent oracle: the probability that `b` appears before `a`,
    /// from the absorbing Markov chain whose states are the last `L − 1`
    /// flips (or fewer at the start), solved by Gaussian elimination.
    #[allow(clippy::many_single_char_names, clippy::needless_range_loop)] // linear-algebra notation
    fn markov_second_wins(a: &str, b: &str) -> f64 {
        let l = a.len();
        // All strings of length 0..l over {H, T}, indexed by position in `states`.
        let mut states: Vec<String> = vec![String::new()];
        for len in 1..l {
            for bits in 0..(1u32 << len) {
                states.push(
                    (0..len)
                        .rev()
                        .map(|i| if bits >> i & 1 == 0 { 'H' } else { 'T' })
                        .collect(),
                );
            }
        }
        let index = |s: &str| states.iter().position(|t| t == s).unwrap();
        let n = states.len();
        // x_s = Σ_flip 1/2 · (1 if b completes, 0 if a completes, x_next otherwise)
        let mut m = vec![vec![0.0_f64; n + 1]; n];
        for (i, s) in states.iter().enumerate() {
            m[i][i] += 1.0;
            for flip in ['H', 'T'] {
                let w = format!("{s}{flip}");
                match (w == b, w == a) {
                    (true, _) => m[i][n] += 0.5,
                    (_, true) => {}
                    _ => {
                        let next = if w.len() == l { &w[1..] } else { &w[..] };
                        m[i][index(next)] -= 0.5;
                    }
                }
            }
        }
        for col in 0..n {
            let pivot = (col..n)
                .max_by(|&p, &q| m[p][col].abs().total_cmp(&m[q][col].abs()))
                .unwrap();
            m.swap(col, pivot);
            for r in 0..n {
                if r != col {
                    let f = m[r][col] / m[col][col];
                    for c in col..=n {
                        m[r][c] -= f * m[col][c];
                    }
                }
            }
        }
        m[0][n] / m[0][0]
    }

    #[test]
    fn conway_agrees_with_markov_chain_on_all_short_pairs() {
        let mut pairs = 0;
        for len in 1..=4u32 {
            for x in 0..(1u32 << len) {
                for y in 0..(1u32 << len) {
                    if x == y {
                        continue;
                    }
                    let (a, b) = (
                        Sequence::from_bits(x, len).unwrap(),
                        Sequence::from_bits(y, len).unwrap(),
                    );
                    let markov = markov_second_wins(&a.to_string(), &b.to_string());
                    assert!((second_wins(a, b) - markov).abs() < 1e-12, "{a} vs {b}");
                    pairs += 1;
                }
            }
        }
        assert_eq!(pairs, 310);
    }

    #[test]
    fn best_responses() {
        assert_eq!(best_response(seq("HHH")).0, seq("THH"));
        assert_eq!(best_response(seq("HHT")).0, seq("THH"));
        assert_eq!(best_response(seq("H")), (seq("T"), 0.5));
        let (_, p) = best_response(seq("HTHTHTHTHTHTHTHT"));
        assert!(p > 0.5);
    }

    #[test]
    fn simulation_matches_exact() {
        for (a, b) in [
            ("HHT", "THH"),
            ("HHH", "THH"),
            ("HTH", "HHT"),
            ("H", "T"),
            ("HTHH", "TTHT"),
        ] {
            assert_simulation_matches_exact(&Penney::new(seq(a), seq(b)).unwrap(), 20_000);
        }
    }

    #[test]
    fn default_is_hht_vs_thh() {
        let p = Penney::default();
        assert_eq!((p.first, p.second), (seq("HHT"), seq("THH")));
    }
}
