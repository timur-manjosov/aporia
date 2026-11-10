//! The six experiments.

mod bertrand;
mod birthday;
mod monty_hall;

pub use bertrand::{Bertrand, Chord, Method};
pub use birthday::Birthday;
pub use monty_hall::{Host, MontyHall};

use crate::engine::Experiment;

/// Every experiment with its default parameters, in presentation order.
///
/// ```
/// let ids: Vec<_> = aporia::experiments::all().iter().map(|e| e.id()).collect();
/// assert_eq!(ids, ["monty-hall", "birthday", "bertrand"]);
/// ```
#[must_use]
pub fn all() -> Vec<Box<dyn Experiment>> {
    vec![
        Box::new(MontyHall::default()),
        Box::new(Birthday::default()),
        Box::new(Bertrand),
    ]
}

/// Shared helpers for the simulation-versus-exact tests.
///
/// The tests use a `z = 4` interval rather than the 95 % interval of the
/// command line. At a fixed seed the outcome is deterministic either way, but
/// a harmless refactoring that changes the order of random draws produces a
/// different sample path; with `z = 4` such a change fails the test with
/// probability about 6e-5 per comparison instead of 5 %.
#[cfg(test)]
pub(crate) mod testing {
    use crate::engine::{Experiment, Kind, RunConfig, Sample};
    use crate::params::Trials;
    use crate::rng::stream;
    use crate::stats::{Confidence, RunningMean, Tally, wilson};

    pub const Z: f64 = 4.0;

    pub fn config(trials: u64) -> RunConfig {
        RunConfig {
            trials: Trials::new(trials).unwrap(),
            seed: crate::rng::DEFAULT_SEED,
            confidence: Confidence::P95,
        }
    }

    /// Asserts that every exact value lies in the `z = 4` interval of its simulation.
    pub fn assert_simulation_matches_exact(experiment: &dyn Experiment, trials: u64) {
        for mut q in experiment.quantities() {
            let Some(exact) = q.exact() else { continue };
            let mut rng = stream(crate::rng::DEFAULT_SEED, experiment.id(), q.label());
            let (mut tally, mut mean) = (Tally::default(), RunningMean::default());
            for _ in 0..trials {
                match q.draw(&mut rng).unwrap() {
                    Sample::Hit(h) => tally.push(h),
                    Sample::Value(x) => mean.push(x),
                    Sample::Rejected => {}
                }
            }
            let (lo, hi) = match q.kind() {
                Kind::Binary => {
                    let e = wilson(tally.hits, tally.n, Z).unwrap();
                    (e.lower, e.upper)
                }
                Kind::Real => {
                    #[allow(clippy::cast_precision_loss)] // small counts
                    let se = (mean.sample_variance().unwrap() / mean.count() as f64).sqrt();
                    (mean.mean() - Z * se, mean.mean() + Z * se)
                }
            };
            assert!(
                lo - 1e-9 <= exact && exact <= hi + 1e-9,
                "{} / {}: exact {exact} outside [{lo}, {hi}]",
                experiment.id(),
                q.label()
            );
        }
    }
}
