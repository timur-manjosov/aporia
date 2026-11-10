//! Aporia: a Monte-Carlo laboratory for results where intuition fails.
//!
//! Each experiment simulates the *mechanism* of a counter-intuitive result,
//! compares the estimate with an exactly derived value that is computed by
//! separate code, and reports the estimate with a confidence interval.
//!
//! ```
//! use aporia::{Confidence, RunConfig, Trials, run};
//! use aporia::experiments::{Host, MontyHall};
//!
//! let game = MontyHall::new(3, 1, Host::Knowing)?;
//! let config = RunConfig {
//!     trials: Trials::new(20_000)?,
//!     seed: aporia::rng::DEFAULT_SEED,
//!     confidence: Confidence::P95,
//! };
//! let report = run(&game, &config)?;
//! let switch = &report.quantities[1];
//! assert_eq!(switch.label, "P(win | switch)");
//! assert!(switch.estimate.point > 0.6); // exact: 2/3
//! print!("{}", aporia::render::report(&report));
//! # Ok::<(), aporia::Error>(())
//! ```
//!
//! The same seed and arguments always produce the same numbers: every
//! quantity draws from its own stream, see [`rng`].

pub mod engine;
pub mod error;
pub mod experiments;
pub mod params;
pub mod render;
pub mod rng;
pub mod stats;

pub use engine::{Experiment, Quantity, Report, RunConfig, Sample, run, run_with_checkpoints};
pub use error::{Error, Result};
pub use params::{Sequence, Trials};
pub use stats::{Confidence, Estimate};
