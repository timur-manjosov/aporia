//! Validated parameter types shared by several modules.

use std::fmt;
use std::str::FromStr;

use crate::error::{Error, Result, invalid};

/// A number of Monte-Carlo trials, at least one.
///
/// ```
/// use aporia::Trials;
/// assert_eq!(Trials::new(1000)?.get(), 1000);
/// assert!(Trials::new(0).is_err());
/// # Ok::<(), aporia::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Trials(u64);

impl Trials {
    /// Accepts any positive count.
    ///
    /// # Errors
    /// Returns [`Error::InvalidParameter`] for zero.
    pub fn new(n: u64) -> Result<Self> {
        if n == 0 {
            return Err(invalid("trials", "must be at least 1"));
        }
        Ok(Self(n))
    }

    /// The count.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

/// A coin-flip pattern of length 1 to [`Sequence::MAX_LEN`], such as `HHT`.
///
/// Internally the pattern is a bit string, `H = 0` and `T = 1`, with the first
/// symbol in the most significant of `len` bits. This is the layout of a
/// rolling window that shifts left on every flip, so matching is one compare.
///
/// ```
/// use aporia::Sequence;
/// let s: Sequence = "HHT".parse()?;
/// assert_eq!(s.len(), 3);
/// assert_eq!(s.bits(), 0b001);
/// assert_eq!(s.to_string(), "HHT");
/// # Ok::<(), aporia::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sequence {
    bits: u32,
    len: u32,
}

impl Sequence {
    /// Longest supported pattern; the expected simulation cost grows like `2^len`.
    pub const MAX_LEN: u32 = 16;

    /// Builds a pattern from its bit representation.
    ///
    /// # Errors
    /// Returns [`Error::InvalidParameter`] if `len` is outside `1..=MAX_LEN` or
    /// `bits` does not fit into `len` bits.
    pub fn from_bits(bits: u32, len: u32) -> Result<Self> {
        if !(1..=Self::MAX_LEN).contains(&len) {
            return Err(invalid(
                "sequence",
                format!("length must be between 1 and {}", Self::MAX_LEN),
            ));
        }
        if bits >> len != 0 {
            return Err(invalid("sequence", "bits exceed the length"));
        }
        Ok(Self { bits, len })
    }

    /// Compile-time constructor for literals inside the crate.
    ///
    /// The caller guarantees `1 ≤ len ≤ MAX_LEN` and `bits < 2^len`; it is
    /// only used with constant arguments that satisfy this.
    pub(crate) const fn from_bits_const(bits: u32, len: u32) -> Self {
        Self { bits, len }
    }

    /// Bit pattern, first symbol most significant, `T = 1`.
    #[must_use]
    pub fn bits(self) -> u32 {
        self.bits
    }

    /// Number of symbols.
    #[must_use]
    #[allow(clippy::len_without_is_empty)] // a Sequence is never empty by construction
    pub fn len(self) -> u32 {
        self.len
    }

    /// Mask selecting the low `len` bits.
    #[must_use]
    pub fn mask(self) -> u32 {
        // len <= 16, so the shift cannot overflow.
        (1 << self.len) - 1
    }
}

impl FromStr for Sequence {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let len = u32::try_from(s.len())
            .ok()
            .filter(|l| (1..=Self::MAX_LEN).contains(l))
            .ok_or_else(|| {
                invalid(
                    "sequence",
                    format!("length must be between 1 and {}", Self::MAX_LEN),
                )
            })?;
        let mut bits = 0;
        for c in s.chars() {
            let bit = match c {
                'H' => 0,
                'T' => 1,
                _ => return Err(invalid("sequence", "use only the letters H and T")),
            };
            bits = (bits << 1) | bit;
        }
        Ok(Self { bits, len })
    }
}

impl fmt::Display for Sequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in (0..self.len).rev() {
            let symbol = if (self.bits >> i) & 1 == 0 { 'H' } else { 'T' };
            write!(f, "{symbol}")?;
        }
        Ok(())
    }
}

/// Checks that `value` lies in `range`, naming the parameter on failure.
pub(crate) fn ensure_in<T>(
    name: &'static str,
    value: T,
    range: std::ops::RangeInclusive<T>,
) -> Result<T>
where
    T: PartialOrd + fmt::Display,
{
    if range.contains(&value) {
        Ok(value)
    } else {
        Err(invalid(
            name,
            format!("{value} is outside {}..={}", range.start(), range.end()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn rejects_bad_sequences() {
        assert!("".parse::<Sequence>().is_err());
        assert!("HXT".parse::<Sequence>().is_err());
        assert!("HHHHHHHHHHHHHHHHH".parse::<Sequence>().is_err());
        assert!("hht".parse::<Sequence>().is_err());
        assert!(Sequence::from_bits(0b100, 2).is_err());
        assert!(Sequence::from_bits(0, 0).is_err());
    }

    #[test]
    fn bit_layout() {
        let s: Sequence = "THH".parse().unwrap();
        assert_eq!(s.bits(), 0b100);
        assert_eq!(s.mask(), 0b111);
    }

    proptest! {
        #[test]
        fn sequence_round_trip(s in "[HT]{1,16}") {
            let parsed: Sequence = s.parse().unwrap();
            prop_assert_eq!(parsed.to_string(), s);
        }

        #[test]
        fn bits_round_trip(len in 1u32..=16, raw in any::<u32>()) {
            let bits = raw & ((1 << len) - 1);
            let s = Sequence::from_bits(bits, len).unwrap();
            prop_assert_eq!(s.to_string().parse::<Sequence>().unwrap(), s);
        }
    }
}
