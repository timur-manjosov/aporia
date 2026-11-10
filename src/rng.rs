//! Reproducible random-number streams.
//!
//! Every quantity of every experiment draws from its own `ChaCha8` stream. The
//! key is derived from the master seed; the 64-bit stream number is the
//! FNV-1a hash of `"<experiment-id>/<quantity-label>"`. Because the stream
//! depends only on these strings, adding an experiment or a quantity never
//! changes the numbers produced by existing ones.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// The random-number generator used by all simulations.
pub type SimRng = ChaCha8Rng;

/// Default master seed: the ASCII bytes of `"APORIA"` read as a big-endian integer.
pub const DEFAULT_SEED: u64 = 0x4150_4F52_4941;

/// FNV-1a, 64-bit variant.
///
/// Used instead of `std::hash::DefaultHasher`, whose output is explicitly not
/// stable across Rust releases.
///
/// ```
/// // Reference value of the FNV-1a specification for the input "a".
/// assert_eq!(aporia::rng::fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
/// ```
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes
        .iter()
        .fold(OFFSET, |h, &b| (h ^ u64::from(b)).wrapping_mul(PRIME))
}

/// The stream for one quantity of one experiment.
///
/// ```
/// use aporia::rng::stream;
/// use rand::RngExt;
/// let a: u64 = stream(1, "birthday", "P(shared)").random();
/// let b: u64 = stream(1, "birthday", "P(shared)").random();
/// let c: u64 = stream(2, "birthday", "P(shared)").random();
/// assert_eq!(a, b);
/// assert_ne!(a, c);
/// ```
#[must_use]
pub fn stream(master_seed: u64, experiment: &str, quantity: &str) -> SimRng {
    let key = [experiment.as_bytes(), b"/", quantity.as_bytes()].concat();
    let mut rng = SimRng::seed_from_u64(master_seed);
    rng.set_stream(fnv1a64(&key));
    rng
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngExt;

    #[test]
    fn fnv_reference_values() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn default_seed_spells_aporia() {
        assert_eq!(&DEFAULT_SEED.to_be_bytes()[2..], b"APORIA");
    }

    #[test]
    fn streams_differ_by_label() {
        let a: u64 = stream(7, "x", "a").random();
        let b: u64 = stream(7, "x", "b").random();
        assert_ne!(a, b);
    }
}
