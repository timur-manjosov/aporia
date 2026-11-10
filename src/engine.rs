//! The experiment abstraction and the Monte-Carlo driver.
//!
//! An [`Experiment`] is a set of [`Quantity`] values. A quantity simulates the
//! mechanism of the experiment once per [`Quantity::draw`]; its exact value,
//! if known, is computed by separate code and only used for comparison.

use crate::error::{Error, Result};
use crate::params::Trials;
use crate::rng::{SimRng, stream};
use crate::stats::{Confidence, Estimate, RunningMean, Tally};

/// The outcome of one simulated trial.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sample {
    /// A binary outcome: did the event occur?
    Hit(bool),
    /// A real-valued outcome.
    Value(f64),
    /// The trial is discarded because it falls outside the conditioning event.
    Rejected,
}

/// Whether a quantity is a probability or an expectation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Estimated by a proportion with a Wilson interval.
    Binary,
    /// Estimated by a mean with a normal interval.
    Real,
}

/// One simulated quantity.
///
/// Implementations may keep scratch buffers in `self`, so that a trial does
/// not allocate; [`Experiment::quantities`] hands out fresh instances.
pub trait Quantity {
    /// Stable label; it also selects the random stream, so changing it changes the output.
    fn label(&self) -> &'static str;
    /// Whether [`Quantity::draw`] yields [`Sample::Hit`] or [`Sample::Value`].
    fn kind(&self) -> Kind;
    /// The analytically derived value, if one is known for these parameters.
    fn exact(&self) -> Option<f64>;
    /// Simulates one trial of the mechanism.
    ///
    /// # Errors
    /// Implementations may fail, for example when an iteration cap is hit.
    fn draw(&mut self, rng: &mut SimRng) -> Result<Sample>;
}

/// A named collection of quantities with its narrative.
pub trait Experiment {
    /// Command-line name, also part of every random-stream key.
    fn id(&self) -> &'static str;
    /// Human-readable title.
    fn title(&self) -> &'static str;
    /// The claim intuition makes.
    fn intuition(&self) -> &'static str;
    /// What the experiment shows instead.
    fn lesson(&self) -> String;
    /// Number of trials used when none is given.
    fn default_trials(&self) -> u64;
    /// Parameter names and values, for display.
    fn parameters(&self) -> Vec<(&'static str, String)>;
    /// Fresh instances of all quantities.
    fn quantities(&self) -> Vec<Box<dyn Quantity>>;
    /// Further exactly computed facts, one line each.
    fn notes(&self) -> Vec<String>;
    /// Whether a convergence plot makes sense for this experiment.
    fn supports_plot(&self) -> bool {
        true
    }
}

/// Settings shared by all quantities of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunConfig {
    /// Trials attempted per quantity.
    pub trials: Trials,
    /// Master seed.
    pub seed: u64,
    /// Confidence level of all intervals.
    pub confidence: Confidence,
}

/// Simulation result for one quantity.
#[derive(Debug, Clone, PartialEq)]
pub struct QuantityReport {
    /// The quantity's label.
    pub label: String,
    /// Binary or real-valued.
    pub kind: Kind,
    /// Exact value, if known.
    pub exact: Option<f64>,
    /// Point estimate and interval from the accepted samples.
    pub estimate: Estimate,
    /// Trials simulated.
    pub attempted: u64,
    /// Trials not rejected.
    pub accepted: u64,
}

impl QuantityReport {
    /// Whether the exact value lies in the confidence interval.
    #[must_use]
    pub fn inside(&self) -> Option<bool> {
        self.exact.map(|x| self.estimate.contains(x))
    }
}

/// Everything shown for one experiment.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// Experiment id.
    pub id: &'static str,
    /// Title.
    pub title: &'static str,
    /// The intuitive claim.
    pub intuition: &'static str,
    /// The lesson.
    pub lesson: String,
    /// Parameters as name/value pairs.
    pub parameters: Vec<(&'static str, String)>,
    /// One entry per quantity, in declaration order.
    pub quantities: Vec<QuantityReport>,
    /// Extra exact lines.
    pub notes: Vec<String>,
    /// Settings of the run.
    pub config: RunConfig,
}

/// Running estimate of one quantity after a given number of trials.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Checkpoint {
    /// Trials attempted so far.
    pub trials: u64,
    /// Estimate so far; `None` while too few samples have been accepted.
    pub estimate: Option<Estimate>,
}

/// The convergence path of one quantity.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    /// The quantity's label.
    pub label: String,
    /// Exact value, if known.
    pub exact: Option<f64>,
    /// Checkpoints in increasing order of trials.
    pub points: Vec<Checkpoint>,
}

/// Runs every quantity of `experiment`.
///
/// # Errors
/// Fails if a quantity fails to draw or accepts too few samples.
pub fn run(experiment: &dyn Experiment, config: &RunConfig) -> Result<Report> {
    run_with_checkpoints(experiment, config, &[]).map(|(report, _)| report)
}

/// Like [`run`], but also records the running estimate at the given trial
/// counts, which must be strictly increasing. Checkpoints beyond the trial
/// count are ignored.
///
/// # Errors
/// Fails like [`run`], or if `checkpoints` is not strictly increasing.
pub fn run_with_checkpoints(
    experiment: &dyn Experiment,
    config: &RunConfig,
    checkpoints: &[u64],
) -> Result<(Report, Vec<Series>)> {
    if checkpoints.windows(2).any(|w| w[0] >= w[1]) {
        return Err(crate::error::invalid(
            "checkpoints",
            "must be strictly increasing",
        ));
    }
    let mut quantities = Vec::new();
    let mut series = Vec::new();
    for mut quantity in experiment.quantities() {
        let mut rng = stream(config.seed, experiment.id(), quantity.label());
        let (report, points) = simulate(quantity.as_mut(), &mut rng, config, checkpoints)?;
        series.push(Series {
            label: report.label.clone(),
            exact: report.exact,
            points,
        });
        quantities.push(report);
    }
    let report = Report {
        id: experiment.id(),
        title: experiment.title(),
        intuition: experiment.intuition(),
        lesson: experiment.lesson(),
        parameters: experiment.parameters(),
        quantities,
        notes: experiment.notes(),
        config: *config,
    };
    Ok((report, series))
}

enum Accumulator {
    Binary(Tally),
    Real(RunningMean),
}

impl Accumulator {
    fn push(&mut self, sample: Sample, label: &str) -> Result<()> {
        match (self, sample) {
            (Self::Binary(t), Sample::Hit(h)) => t.push(h),
            (Self::Real(m), Sample::Value(x)) => m.push(x),
            (_, Sample::Rejected) => {}
            _ => {
                return Err(Error::Internal(format!(
                    "quantity '{label}' returned a sample of the wrong kind"
                )));
            }
        }
        Ok(())
    }

    fn accepted(&self) -> u64 {
        match self {
            Self::Binary(t) => t.n,
            Self::Real(m) => m.count(),
        }
    }

    fn estimate(&self, confidence: Confidence) -> Option<Estimate> {
        match self {
            Self::Binary(t) => t.estimate(confidence),
            Self::Real(m) => m.estimate(confidence),
        }
    }

    fn required(&self) -> u64 {
        match self {
            Self::Binary(_) => 1,
            Self::Real(_) => 2,
        }
    }
}

fn simulate(
    quantity: &mut dyn Quantity,
    rng: &mut SimRng,
    config: &RunConfig,
    checkpoints: &[u64],
) -> Result<(QuantityReport, Vec<Checkpoint>)> {
    let label = quantity.label();
    let mut acc = match quantity.kind() {
        Kind::Binary => Accumulator::Binary(Tally::default()),
        Kind::Real => Accumulator::Real(RunningMean::default()),
    };
    let trials = config.trials.get();
    let mut points = Vec::new();
    let mut pending = checkpoints
        .iter()
        .copied()
        .filter(|&c| c <= trials)
        .peekable();
    for t in 1..=trials {
        acc.push(quantity.draw(rng)?, label)?;
        if pending.next_if_eq(&t).is_some() {
            points.push(Checkpoint {
                trials: t,
                estimate: acc.estimate(config.confidence),
            });
        }
    }
    let estimate = acc
        .estimate(config.confidence)
        .ok_or_else(|| Error::InsufficientSamples {
            quantity: label.to_owned(),
            accepted: acc.accepted(),
            required: acc.required(),
        })?;
    let report = QuantityReport {
        label: label.to_owned(),
        kind: quantity.kind(),
        exact: quantity.exact(),
        estimate,
        attempted: trials,
        accepted: acc.accepted(),
    };
    Ok((report, points))
}

/// About `count` log-spaced trial counts from `min(10, trials)` to `trials`,
/// strictly increasing and always ending at `trials`.
///
/// ```
/// let c = aporia::engine::log_checkpoints(1_000_000, 60);
/// assert_eq!(c.first(), Some(&10));
/// assert_eq!(c.last(), Some(&1_000_000));
/// assert!(c.windows(2).all(|w| w[0] < w[1]));
/// ```
#[must_use]
pub fn log_checkpoints(trials: u64, count: u32) -> Vec<u64> {
    let start = trials.min(10);
    #[allow(clippy::cast_precision_loss)] // only used to place plot points
    let (lo, hi) = ((start as f64).ln(), (trials as f64).ln());
    let steps = count.max(2) - 1;
    let mut out: Vec<u64> = (0..=steps)
        .map(|i| {
            let x = lo + (hi - lo) * f64::from(i) / f64::from(steps);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // x.exp() lies in [start, trials] up to rounding; clamp handles the rest
            let c = x.exp().round() as u64;
            c.clamp(start, trials)
        })
        .collect();
    out.dedup();
    if out.last() != Some(&trials) {
        out.push(trials);
    }
    out
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact equality is the property under test
mod tests {
    use super::*;
    use rand::RngExt;

    /// A biased coin and a uniform variable with rejection, to exercise the driver.
    struct Toy;
    struct Coin;
    struct Uniform;

    impl Quantity for Coin {
        fn label(&self) -> &'static str {
            "coin"
        }
        fn kind(&self) -> Kind {
            Kind::Binary
        }
        fn exact(&self) -> Option<f64> {
            Some(0.3)
        }
        fn draw(&mut self, rng: &mut SimRng) -> Result<Sample> {
            Ok(Sample::Hit(rng.random_bool(0.3)))
        }
    }

    impl Quantity for Uniform {
        fn label(&self) -> &'static str {
            "uniform | > 0.5"
        }
        fn kind(&self) -> Kind {
            Kind::Real
        }
        fn exact(&self) -> Option<f64> {
            Some(0.75)
        }
        fn draw(&mut self, rng: &mut SimRng) -> Result<Sample> {
            let u: f64 = rng.random();
            Ok(if u > 0.5 {
                Sample::Value(u)
            } else {
                Sample::Rejected
            })
        }
    }

    impl Experiment for Toy {
        fn id(&self) -> &'static str {
            "toy"
        }
        fn title(&self) -> &'static str {
            "Toy"
        }
        fn intuition(&self) -> &'static str {
            ""
        }
        fn lesson(&self) -> String {
            String::new()
        }
        fn default_trials(&self) -> u64 {
            1000
        }
        fn parameters(&self) -> Vec<(&'static str, String)> {
            Vec::new()
        }
        fn quantities(&self) -> Vec<Box<dyn Quantity>> {
            vec![Box::new(Coin), Box::new(Uniform)]
        }
        fn notes(&self) -> Vec<String> {
            Vec::new()
        }
    }

    fn config(trials: u64) -> RunConfig {
        RunConfig {
            trials: Trials::new(trials).unwrap(),
            seed: 11,
            confidence: Confidence::P95,
        }
    }

    #[test]
    fn last_checkpoint_equals_plain_run() {
        let cfg = config(5000);
        let plain = run(&Toy, &cfg).unwrap();
        let (report, series) =
            run_with_checkpoints(&Toy, &cfg, &log_checkpoints(5000, 60)).unwrap();
        assert_eq!(plain, report);
        for (s, q) in series.iter().zip(&plain.quantities) {
            let last = s.points.last().unwrap();
            assert_eq!(last.trials, 5000);
            assert_eq!(last.estimate, Some(q.estimate));
        }
    }

    #[test]
    fn rejection_is_counted_and_conditioning_works() {
        let r = run(&Toy, &config(20_000)).unwrap();
        let q = &r.quantities[1];
        assert_eq!(q.attempted, 20_000);
        assert!(q.accepted > 9_000 && q.accepted < 11_000);
        assert_eq!(q.inside(), Some(true));
        assert_eq!(r.quantities[0].inside(), Some(true));
    }

    #[test]
    fn too_few_accepted_is_an_error() {
        // With one trial the uniform quantity has at most one accepted sample.
        assert!(matches!(
            run(&Toy, &config(1)),
            Err(Error::InsufficientSamples { required: 2, .. })
        ));
    }

    #[test]
    fn checkpoints_must_increase() {
        assert!(run_with_checkpoints(&Toy, &config(10), &[5, 5]).is_err());
    }

    #[test]
    fn checkpoint_grid() {
        assert_eq!(log_checkpoints(1, 60), vec![1]);
        assert_eq!(log_checkpoints(7, 60), vec![7]);
        let c = log_checkpoints(200, 60);
        assert_eq!((c[0], *c.last().unwrap()), (10, 200));
        assert!(c.windows(2).all(|w| w[0] < w[1]));
        assert!(log_checkpoints(1_000_000, 60).len() >= 55);
    }
}
