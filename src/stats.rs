//! Point estimates and confidence intervals.
//!
//! Binary quantities use the Wilson score interval, which keeps its nominal
//! coverage near 0 and 1 where the textbook Wald interval collapses to width
//! zero. Real-valued quantities use the normal interval `mean ± z·SE`; it
//! relies on the central limit theorem and is therefore a large-sample
//! approximation.

use std::fmt;

use crate::error::Result;

/// Two-sided confidence level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Confidence {
    /// 90 %.
    P90,
    /// 95 % (the default).
    #[default]
    P95,
    /// 99 %.
    P99,
}

impl Confidence {
    /// The standard normal quantile `z` with `P(|Z| ≤ z)` equal to the level.
    ///
    /// ```
    /// use aporia::Confidence;
    /// assert_eq!(Confidence::P95.z(), 1.959964);
    /// ```
    #[must_use]
    pub fn z(self) -> f64 {
        match self {
            Self::P90 => 1.644_854,
            Self::P95 => 1.959_964,
            Self::P99 => 2.575_829,
        }
    }

    /// The level in percent.
    #[must_use]
    pub fn percent(self) -> u32 {
        match self {
            Self::P90 => 90,
            Self::P95 => 95,
            Self::P99 => 99,
        }
    }

    /// Builds a level from a percentage.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidParameter`] unless `percent` is 90, 95 or 99.
    pub fn from_percent(percent: u32) -> Result<Self> {
        match percent {
            90 => Ok(Self::P90),
            95 => Ok(Self::P95),
            99 => Ok(Self::P99),
            _ => Err(crate::error::invalid("confidence", "must be 90, 95 or 99")),
        }
    }
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} %", self.percent())
    }
}

/// A point estimate with a two-sided confidence interval.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    /// The point estimate (sample proportion or sample mean).
    pub point: f64,
    /// Lower end of the interval.
    pub lower: f64,
    /// Upper end of the interval.
    pub upper: f64,
}

impl Estimate {
    /// Absolute slack used when testing whether a value lies in the interval.
    ///
    /// It makes degenerate intervals of width zero (every sample identical)
    /// robust against rounding in the last bit.
    pub const TOLERANCE: f64 = 1e-9;

    /// Whether `value` lies in the interval, up to [`Estimate::TOLERANCE`].
    ///
    /// ```
    /// use aporia::stats::Estimate;
    /// let e = Estimate { point: 80.0, lower: 80.0, upper: 80.0 };
    /// assert!(e.contains(80.0 + 1e-12));
    /// assert!(!e.contains(80.1));
    /// ```
    #[must_use]
    pub fn contains(&self, value: f64) -> bool {
        self.lower - Self::TOLERANCE <= value && value <= self.upper + Self::TOLERANCE
    }
}

/// Wilson score interval for `hits` successes in `n` trials.
///
/// ```
/// use aporia::stats::wilson;
/// let e = wilson(5, 10, 1.959964).unwrap();
/// assert!((e.lower - 0.2366).abs() < 1e-4 && (e.upper - 0.7634).abs() < 1e-4);
/// assert!(wilson(0, 0, 1.959964).is_none());
/// ```
///
/// Returns `None` if `n` is zero.
#[must_use]
pub fn wilson(hits: u64, n: u64, z: f64) -> Option<Estimate> {
    if n == 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)] // counts above 2^53 are far beyond any run length
    let (h, n) = (hits as f64, n as f64);
    let p = h / n;
    let z2 = z * z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let half = z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / denom;
    Some(Estimate {
        point: p,
        lower: (center - half).max(0.0),
        upper: (center + half).min(1.0),
    })
}

/// Counter of successes for a binary quantity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    /// Number of successes.
    pub hits: u64,
    /// Number of observations.
    pub n: u64,
}

impl Tally {
    /// Records one observation.
    pub fn push(&mut self, hit: bool) {
        self.hits += u64::from(hit);
        self.n += 1;
    }

    /// Sample proportion with Wilson interval; `None` if nothing was recorded.
    #[must_use]
    pub fn estimate(&self, confidence: Confidence) -> Option<Estimate> {
        wilson(self.hits, self.n, confidence.z())
    }
}

/// Numerically stable running mean and variance (Welford's algorithm).
///
/// The naive formula `E[X²] − E[X]²` cancels catastrophically when the mean is
/// large compared with the spread; Welford's update does not.
///
/// ```
/// use aporia::stats::RunningMean;
/// let mut m = RunningMean::default();
/// for x in [2.0, 4.0, 6.0] { m.push(x); }
/// assert_eq!(m.mean(), 4.0);
/// assert_eq!(m.sample_variance(), Some(4.0));
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RunningMean {
    n: u64,
    mean: f64,
    m2: f64,
}

impl RunningMean {
    /// Records one observation.
    pub fn push(&mut self, x: f64) {
        self.n += 1;
        let delta = x - self.mean;
        #[allow(clippy::cast_precision_loss)] // counts above 2^53 are far beyond any run length
        let n = self.n as f64;
        self.mean += delta / n;
        self.m2 += delta * (x - self.mean);
    }

    /// Number of observations.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.n
    }

    /// Sample mean (0 if empty).
    #[must_use]
    pub fn mean(&self) -> f64 {
        self.mean
    }

    /// Unbiased sample variance; `None` with fewer than two observations.
    #[must_use]
    pub fn sample_variance(&self) -> Option<f64> {
        #[allow(clippy::cast_precision_loss)] // counts above 2^53 are far beyond any run length
        (self.n >= 2).then(|| self.m2 / (self.n - 1) as f64)
    }

    /// Sample mean with normal (CLT) interval `mean ± z·s/√n`.
    ///
    /// Returns `None` with fewer than two observations, because the standard
    /// error is then undefined.
    #[must_use]
    pub fn estimate(&self, confidence: Confidence) -> Option<Estimate> {
        let var = self.sample_variance()?;
        #[allow(clippy::cast_precision_loss)] // counts above 2^53 are far beyond any run length
        let se = (var / self.n as f64).sqrt();
        let half = confidence.z() * se;
        Some(Estimate {
            point: self.mean,
            lower: self.mean - half,
            upper: self.mean + half,
        })
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::cast_precision_loss)] // exact equality is the property under test; test counts are small
mod tests {
    use super::*;
    use crate::rng::stream;
    use proptest::prelude::*;
    use rand::RngExt;

    const Z95: f64 = 1.959_964;

    #[test]
    fn wilson_zero_successes() {
        let e = wilson(0, 10, Z95).unwrap();
        assert_eq!(e.lower, 0.0);
        let expected = Z95 * Z95 / (10.0 + Z95 * Z95);
        assert!((e.upper - expected).abs() < 1e-12);
        assert!((e.upper - 0.2775).abs() < 1e-4);
    }

    #[test]
    fn wilson_half() {
        let e = wilson(5, 10, Z95).unwrap();
        assert!((e.lower - 0.2366).abs() < 1e-4);
        assert!((e.upper - 0.7634).abs() < 1e-4);
        assert_eq!(e.point, 0.5);
    }

    #[test]
    fn wilson_needs_data() {
        assert!(wilson(0, 0, Z95).is_none());
        assert!(Tally::default().estimate(Confidence::P95).is_none());
    }

    #[test]
    fn welford_matches_two_pass_with_large_offset() {
        let mut rng = stream(1, "test", "welford");
        let data: Vec<f64> = (0..10_000).map(|_| 1e9 + rng.random::<f64>()).collect();
        let mut m = RunningMean::default();
        for &x in &data {
            m.push(x);
        }
        let n = data.len() as f64;
        let mean = data.iter().sum::<f64>() / n;
        let var = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
        // One ulp at 1e9 is about 1.2e-7; the naive sum accumulates a few hundred.
        assert!((m.mean() - mean).abs() < 1e-5, "{} vs {mean}", m.mean());
        // Uniform(0,1) has variance 1/12; the relative agreement must be tight.
        let v = m.sample_variance().unwrap();
        assert!((v - var).abs() / var < 1e-6, "{v} vs {var}");
        assert!((v - 1.0 / 12.0).abs() < 0.005);
    }

    #[test]
    fn running_mean_needs_two_samples() {
        let mut m = RunningMean::default();
        m.push(1.0);
        assert!(m.estimate(Confidence::P95).is_none());
        m.push(1.0);
        let e = m.estimate(Confidence::P95).unwrap();
        assert_eq!((e.lower, e.upper), (1.0, 1.0));
    }

    #[test]
    fn wilson_empirical_coverage() {
        let (p, n, reps) = (0.3, 200, 20_000);
        let mut rng = stream(2, "test", "coverage");
        let mut covered = 0;
        for _ in 0..reps {
            let hits = (0..n).filter(|_| rng.random_bool(p)).count() as u64;
            if wilson(hits, n, Z95).unwrap().contains(p) {
                covered += 1;
            }
        }
        let coverage = f64::from(covered) / f64::from(reps);
        assert!((0.93..=0.97).contains(&coverage), "coverage {coverage}");
    }

    #[test]
    fn confidence_levels() {
        for pct in [90, 95, 99] {
            assert_eq!(Confidence::from_percent(pct).unwrap().percent(), pct);
        }
        assert!(Confidence::from_percent(80).is_err());
    }

    proptest! {
        #[test]
        fn wilson_contains_point_and_is_in_unit_interval(n in 1u64..100_000, frac in 0.0f64..=1.0) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
            let hits = ((n as f64) * frac).floor() as u64;
            for c in [Confidence::P90, Confidence::P95, Confidence::P99] {
                let e = wilson(hits, n, c.z()).unwrap();
                prop_assert!(0.0 <= e.lower && e.lower <= e.point && e.point <= e.upper && e.upper <= 1.0);
            }
        }
    }
}
