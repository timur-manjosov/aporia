//! Aporia: a Monte-Carlo laboratory for results where intuition fails.
//!
//! Each experiment simulates the *mechanism* of a counter-intuitive result,
//! compares the estimate with an exactly derived value that is computed by
//! separate code, and reports the estimate with a confidence interval.

pub mod engine;
pub mod error;
pub mod params;
pub mod render;
pub mod rng;
pub mod stats;

pub use engine::{Experiment, Quantity, Report, RunConfig, Sample, run, run_with_checkpoints};
pub use error::{Error, Result};
pub use params::{Sequence, Trials};
pub use stats::{Confidence, Estimate};
