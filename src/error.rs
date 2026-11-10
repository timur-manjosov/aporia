//! The error type shared by the library and the command-line interface.

use std::path::PathBuf;

/// Everything that can go wrong in Aporia.
///
/// Parameter errors are raised by constructors, so a value that exists is valid
/// ("parse, don't validate"). Simulation errors are typed instead of surfacing
/// as `NaN` or a panic.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A parameter value lies outside its admissible range.
    #[error("invalid value for {name}: {reason}")]
    InvalidParameter {
        /// Name of the parameter as the user knows it.
        name: &'static str,
        /// Human-readable description of the admissible range.
        reason: String,
    },

    /// Too few accepted samples to form an estimate with an interval.
    #[error(
        "quantity '{quantity}' has {accepted} accepted samples, at least {required} are needed"
    )]
    InsufficientSamples {
        /// Label of the quantity.
        quantity: String,
        /// Number of samples that were accepted.
        accepted: u64,
        /// Minimum number required.
        required: u64,
    },

    /// An iterative procedure exceeded its iteration cap.
    #[error("{procedure} did not terminate within {cap} steps")]
    IterationCap {
        /// What was iterating.
        procedure: &'static str,
        /// The cap that was hit.
        cap: u64,
    },

    /// A requested feature is not available for this experiment.
    #[error("{0}")]
    Unsupported(String),

    /// An experiment implementation broke an invariant of the engine. This is a bug.
    #[error("internal error: {0}")]
    Internal(String),

    /// A file could not be written.
    #[error("cannot write {path}: {source}")]
    Io {
        /// The file that was being written.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
}

/// Convenience alias used throughout the crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Builds an [`Error::InvalidParameter`].
pub(crate) fn invalid(name: &'static str, reason: impl Into<String>) -> Error {
    Error::InvalidParameter {
        name,
        reason: reason.into(),
    }
}
