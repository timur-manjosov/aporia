//! The birthday problem.
//!
//! The simulation assigns uniformly random days one person at a time and
//! stops at the first repeated day. A reusable table of seen days is cleared
//! by undoing only the entries that were set, so a trial costs `O(n)` and
//! allocates nothing.

use rand::RngExt;

use crate::engine::{Experiment, Kind, Quantity, Sample};
use crate::error::Result;
use crate::params::ensure_in;
use crate::rng::SimRng;

/// Validated parameters of the birthday problem.
///
/// ```
/// use aporia::experiments::Birthday;
/// let b = Birthday::new(23, 365)?;
/// assert!((b.exact() - 0.507297).abs() < 1e-6);
/// assert_eq!(Birthday::smallest_majority(365), 23);
/// # Ok::<(), aporia::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Birthday {
    people: u64,
    days: u64,
}

impl Birthday {
    /// Largest number of days; the simulation keeps one flag per day.
    pub const MAX_DAYS: u64 = 10_000_000;
    /// Largest number of people.
    pub const MAX_PEOPLE: u64 = 1_000_000_000;

    /// Validates `1 ≤ people ≤ MAX_PEOPLE` and `1 ≤ days ≤ MAX_DAYS`.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidParameter`] otherwise.
    pub fn new(people: u64, days: u64) -> Result<Self> {
        Ok(Self {
            people: ensure_in("people", people, 1..=Self::MAX_PEOPLE)?,
            days: ensure_in("days", days, 1..=Self::MAX_DAYS)?,
        })
    }

    /// Exact `P(at least two of n people share a day)`.
    ///
    /// The complement is `∏_{i<n} (1 − i/d)`. The product is summed in log
    /// space with `ln_1p` and mapped back with `expm1`, which keeps full
    /// relative precision when the probability is tiny. For `n > d` the
    /// pigeonhole principle gives 1; the guard also avoids `ln_1p(−1) = −∞`.
    #[must_use]
    pub fn exact(&self) -> f64 {
        exact_probability(self.people, self.days)
    }

    /// The smallest group size with collision probability at least 1/2.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // n ≤ days + 1 and days ≤ MAX_DAYS < 2^53, exact
    pub fn smallest_majority(days: u64) -> u64 {
        let d = days as f64;
        let mut log_none = 0.0;
        let mut n = 1;
        // P(n) is increasing and reaches 1 at n = d + 1, so the loop ends.
        while n <= days && -f64::exp_m1(log_none) < 0.5 {
            log_none += f64::ln_1p(-(n as f64) / d);
            n += 1;
        }
        n
    }
}

#[allow(clippy::cast_precision_loss)] // both arguments are below 2^53 by validation
fn exact_probability(people: u64, days: u64) -> f64 {
    if people > days {
        return 1.0;
    }
    let d = days as f64;
    let log_none: f64 = (0..people).map(|i| f64::ln_1p(-(i as f64) / d)).sum();
    -f64::exp_m1(log_none)
}

impl Default for Birthday {
    fn default() -> Self {
        Self {
            people: 23,
            days: 365,
        }
    }
}

struct Shared {
    params: Birthday,
    seen: Vec<bool>,
    touched: Vec<usize>,
}

impl Quantity for Shared {
    fn label(&self) -> &'static str {
        "P(shared birthday)"
    }

    fn kind(&self) -> Kind {
        Kind::Binary
    }

    fn exact(&self) -> Option<f64> {
        Some(self.params.exact())
    }

    fn draw(&mut self, rng: &mut SimRng) -> Result<Sample> {
        let mut shared = false;
        for _ in 0..self.params.people {
            #[allow(clippy::cast_possible_truncation)] // days ≤ MAX_DAYS fits in usize
            let day = rng.random_range(0..self.params.days) as usize;
            // day < days == seen.len()
            if self.seen[day] {
                shared = true;
                break;
            }
            self.seen[day] = true;
            self.touched.push(day);
        }
        for &day in &self.touched {
            self.seen[day] = false;
        }
        self.touched.clear();
        Ok(Sample::Hit(shared))
    }
}

impl Experiment for Birthday {
    fn id(&self) -> &'static str {
        "birthday"
    }

    fn title(&self) -> &'static str {
        "Birthday problem"
    }

    fn intuition(&self) -> &'static str {
        "For an even chance of a shared birthday, about half as many people as days are needed."
    }

    fn lesson(&self) -> String {
        "A shared birthday needs only one matching pair, and the number of pairs, n(n-1)/2, \
         grows quadratically: 23 people already form 253 pairs."
            .to_owned()
    }

    fn default_trials(&self) -> u64 {
        1_000_000
    }

    fn parameters(&self) -> Vec<(&'static str, String)> {
        vec![
            ("people", self.people.to_string()),
            ("days", self.days.to_string()),
        ]
    }

    fn quantities(&self) -> Vec<Box<dyn Quantity>> {
        #[allow(clippy::cast_possible_truncation)] // days ≤ MAX_DAYS fits in usize
        let days = self.days as usize;
        let touched = days.min(usize::try_from(self.people).unwrap_or(usize::MAX));
        vec![Box::new(Shared {
            params: *self,
            seen: vec![false; days],
            touched: Vec::with_capacity(touched),
        })]
    }

    fn notes(&self) -> Vec<String> {
        let pairs = u128::from(self.people) * u128::from(self.people - 1) / 2;
        vec![
            format!(
                "Smallest group with P >= 1/2 for {} days: {} people (naive guess ceil(d/2) = {}).",
                self.days,
                Self::smallest_majority(self.days),
                self.days.div_ceil(2)
            ),
            format!("Pairs among {} people: {pairs}.", self.people),
        ]
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // the pigeonhole and empty cases are exact
mod tests {
    use super::*;
    use crate::experiments::testing::assert_simulation_matches_exact;
    use proptest::prelude::*;

    #[test]
    fn oracles() {
        for (n, expected) in [(23, 0.507_297), (22, 0.475_695), (50, 0.970_374)] {
            let p = Birthday::new(n, 365).unwrap().exact();
            assert!((p - expected).abs() < 1e-6, "n={n}: {p}");
        }
        assert_eq!(Birthday::smallest_majority(365), 23);
        assert_eq!(365u64.div_ceil(2), 183);
    }

    #[test]
    fn pigeonhole_and_edges() {
        assert_eq!(Birthday::new(366, 365).unwrap().exact(), 1.0);
        assert_eq!(Birthday::new(2, 1).unwrap().exact(), 1.0);
        assert_eq!(Birthday::new(1, 365).unwrap().exact(), 0.0);
        let full = Birthday::new(365, 365).unwrap().exact();
        assert!(full.is_finite() && full > 0.999_999);
        assert_eq!(Birthday::smallest_majority(1), 2);
        assert_eq!(Birthday::smallest_majority(2), 2);
        assert!(Birthday::new(0, 365).is_err());
        assert!(Birthday::new(10, 0).is_err());
    }

    #[test]
    fn simulation_matches_exact() {
        for (n, d) in [(23, 365), (5, 10), (40, 365), (11, 10), (1, 5)] {
            assert_simulation_matches_exact(&Birthday::new(n, d).unwrap(), 20_000);
        }
    }

    proptest! {
        #[test]
        fn probability_in_unit_interval_and_monotone(n in 1u64..2000, d in 1u64..2000) {
            let p = Birthday::new(n, d).unwrap().exact();
            let q = Birthday::new(n + 1, d).unwrap().exact();
            prop_assert!((0.0..=1.0).contains(&p));
            prop_assert!(p <= q);
        }
    }
}
