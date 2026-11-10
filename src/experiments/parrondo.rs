//! Parrondo's paradox.
//!
//! Game A is a slightly unfair coin. Game B uses a bad coin when the capital
//! is a multiple of three and a good coin otherwise. Each game loses on its
//! own, yet alternating between them wins. The simulation plays the games
//! round by round; the exact expectation propagates the distribution of the
//! capital modulo three, which is all that the win probabilities depend on.

use std::fmt;

use rand::RngExt;

use crate::engine::{Experiment, Kind, Quantity, Sample};
use crate::error::{Result, invalid};
use crate::params::ensure_in;
use crate::rng::SimRng;

/// Which game is played in each round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Always game A.
    A,
    /// Always game B.
    B,
    /// A or B with probability 1/2 each, independently every round.
    Random,
    /// The deterministic sequence A, A, B, B, A, A, B, B, …
    Aabb,
}

impl Policy {
    /// All policies in presentation order.
    pub const ALL: [Self; 4] = [Self::A, Self::B, Self::Random, Self::Aabb];

    fn label(self) -> &'static str {
        match self {
            Self::A => "E[X_T] | A only",
            Self::B => "E[X_T] | B only",
            Self::Random => "E[X_T] | random",
            Self::Aabb => "E[X_T] | AABB",
        }
    }
}

impl fmt::Display for Policy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::A => "A",
            Self::B => "B",
            Self::Random => "random",
            Self::Aabb => "AABB",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Game {
    A,
    B,
}

/// Validated parameters of Parrondo's games.
///
/// ```
/// use aporia::experiments::{Parrondo, Policy};
/// let p = Parrondo::new(1000, 0.005)?;
/// assert!((p.exact_mean(Policy::A) + 10.0).abs() < 1e-9);
/// assert!(p.exact_mean(Policy::Random) > 0.0);
/// # Ok::<(), aporia::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parrondo {
    rounds: u32,
    epsilon: f64,
}

impl Parrondo {
    /// Largest number of rounds.
    pub const MAX_ROUNDS: u32 = 1_000_000;

    /// Validates `1 ≤ rounds ≤ MAX_ROUNDS` and `0 ≤ epsilon < 0.1`.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidParameter`] otherwise, including for `NaN`.
    pub fn new(rounds: u32, epsilon: f64) -> Result<Self> {
        let rounds = ensure_in("rounds", rounds, 1..=Self::MAX_ROUNDS)?;
        if !(0.0..0.1).contains(&epsilon) {
            return Err(invalid("epsilon", "must lie in [0, 0.1)"));
        }
        Ok(Self { rounds, epsilon })
    }

    /// Win probability of `game` at capital residue `r = capital mod 3`.
    fn win(&self, game: Game, r: usize) -> f64 {
        match (game, r) {
            (Game::A, _) => 0.5 - self.epsilon,
            (Game::B, 0) => 0.1 - self.epsilon,
            (Game::B, _) => 0.75 - self.epsilon,
        }
    }

    /// Win probability in round `t` at residue `r`, averaged over the
    /// policy's choice of game.
    fn policy_win(&self, policy: Policy, t: u32, r: usize) -> f64 {
        match policy {
            Policy::A => self.win(Game::A, r),
            Policy::B => self.win(Game::B, r),
            Policy::Random => 0.5 * (self.win(Game::A, r) + self.win(Game::B, r)),
            Policy::Aabb => self.win(aabb(t), r),
        }
    }

    /// Exact `E[X_T]`, the expected capital after `T` rounds.
    ///
    /// Let `π_t` be the distribution of the capital modulo 3 before round `t`
    /// and `p_t(r)` the win probability there. Each round adds
    /// `Σ_r π_t(r) (2 p_t(r) − 1)` to the expectation and moves mass from `r`
    /// to `r + 1` with probability `p_t(r)` and to `r − 1` otherwise.
    #[must_use]
    pub fn exact_mean(&self, policy: Policy) -> f64 {
        let mut dist = [1.0, 0.0, 0.0];
        let mut mean = 0.0;
        for t in 0..self.rounds {
            let mut next = [0.0; 3];
            for (r, &mass) in dist.iter().enumerate() {
                let p = self.policy_win(policy, t, r);
                mean += mass * (2.0 * p - 1.0);
                next[(r + 1) % 3] += mass * p;
                next[(r + 2) % 3] += mass * (1.0 - p);
            }
            dist = next;
        }
        mean
    }

    /// Stationary distribution of the capital modulo 3 under a time-homogeneous
    /// policy; `None` for the periodic [`Policy::Aabb`].
    ///
    /// For a walk on the 3-cycle that steps up with probability `p_r` at
    /// residue `r`, the balance equations are solved by
    /// `π_r ∝ 1 − p_{r+1} + p_{r+1} p_{r+2}` (indices mod 3).
    #[must_use]
    pub fn stationary(&self, policy: Policy) -> Option<[f64; 3]> {
        if policy == Policy::Aabb {
            return None;
        }
        let p = [0, 1, 2].map(|r| self.policy_win(policy, 0, r));
        let w = [0, 1, 2].map(|r| 1.0 - p[(r + 1) % 3] + p[(r + 1) % 3] * p[(r + 2) % 3]);
        let total: f64 = w.iter().sum();
        Some(w.map(|x| x / total))
    }

    /// Asymptotic drift per round, `Σ_r π_r (2 p_r − 1)` under the stationary
    /// distribution; `None` for [`Policy::Aabb`].
    #[must_use]
    pub fn drift(&self, policy: Policy) -> Option<f64> {
        let pi = self.stationary(policy)?;
        Some(
            (0..3)
                .map(|r| pi[r] * (2.0 * self.policy_win(policy, 0, r) - 1.0))
                .sum(),
        )
    }
}

impl Default for Parrondo {
    fn default() -> Self {
        Self {
            rounds: 1000,
            epsilon: 0.005,
        }
    }
}

fn aabb(t: u32) -> Game {
    if t % 4 < 2 { Game::A } else { Game::B }
}

struct Capital {
    params: Parrondo,
    policy: Policy,
}

impl Quantity for Capital {
    fn label(&self) -> &'static str {
        self.policy.label()
    }

    fn kind(&self) -> Kind {
        Kind::Real
    }

    fn exact(&self) -> Option<f64> {
        Some(self.params.exact_mean(self.policy))
    }

    fn draw(&mut self, rng: &mut SimRng) -> Result<Sample> {
        let mut capital: i64 = 0;
        for t in 0..self.params.rounds {
            let game = match self.policy {
                Policy::A => Game::A,
                Policy::B => Game::B,
                Policy::Random => {
                    if rng.random::<bool>() {
                        Game::A
                    } else {
                        Game::B
                    }
                }
                Policy::Aabb => aabb(t),
            };
            // rem_euclid keeps the residue in 0..3 for negative capital.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // value in 0..3
            let r = capital.rem_euclid(3) as usize;
            let p = self.params.win(game, r);
            capital += if rng.random::<f64>() < p { 1 } else { -1 };
        }
        #[allow(clippy::cast_precision_loss)] // |capital| ≤ MAX_ROUNDS
        Ok(Sample::Value(capital as f64))
    }
}

impl Experiment for Parrondo {
    fn id(&self) -> &'static str {
        "parrondo"
    }

    fn title(&self) -> &'static str {
        "Parrondo's paradox"
    }

    fn intuition(&self) -> &'static str {
        "Mixing two losing games cannot produce a winning game."
    }

    fn lesson(&self) -> String {
        "Game B loses because its bad coin is used too often: under B alone the capital \
         is a multiple of three in more than one round in three. Mixing in A, which ignores \
         the capital, changes the stationary distribution over residues; the bad coin is \
         used less often, and the drift, a re-weighted average of the coins, turns positive."
            .to_owned()
    }

    fn default_trials(&self) -> u64 {
        100_000
    }

    fn parameters(&self) -> Vec<(&'static str, String)> {
        vec![
            ("rounds", self.rounds.to_string()),
            ("epsilon", format!("{:.4}", self.epsilon)),
        ]
    }

    fn quantities(&self) -> Vec<Box<dyn Quantity>> {
        Policy::ALL
            .iter()
            .map(|&policy| {
                Box::new(Capital {
                    params: *self,
                    policy,
                }) as Box<dyn Quantity>
            })
            .collect()
    }

    fn notes(&self) -> Vec<String> {
        let mut drift = Vec::new();
        let mut share = Vec::new();
        for policy in [Policy::A, Policy::B, Policy::Random] {
            if let (Some(d), Some([at_zero, ..])) = (self.drift(policy), self.stationary(policy)) {
                drift.push(format!("{policy} {d:+.6}"));
                share.push(format!("{policy} {at_zero:.6}"));
            }
        }
        vec![
            format!(
                "Asymptotic drift per round (stationary distribution of capital mod 3): {}.",
                drift.join(", ")
            ),
            format!(
                "Stationary share of rounds at capital = 0 (mod 3), where B uses its bad coin: \
                 {}.",
                share.join(", ")
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiments::testing::assert_simulation_matches_exact;

    #[test]
    fn oracles_expected_capital() {
        let p = Parrondo::default();
        for (policy, expected) in [
            (Policy::A, -10.0),
            (Policy::B, -9.2181),
            (Policy::Random, 15.4211),
            (Policy::Aabb, 14.5772),
        ] {
            let m = p.exact_mean(policy);
            assert!((m - expected).abs() < 1e-3, "{policy}: {m}");
        }
    }

    #[test]
    fn oracles_drift() {
        let p = Parrondo::default();
        for (policy, expected) in [
            (Policy::A, -0.010_000),
            (Policy::B, -0.008_695),
            (Policy::Random, 0.015_704),
        ] {
            let d = p.drift(policy).unwrap();
            assert!((d - expected).abs() < 1e-5, "{policy}: {d}");
        }
        assert!(p.drift(Policy::Aabb).is_none());
    }

    #[test]
    fn stationary_distribution_is_invariant() {
        let p = Parrondo::new(10, 0.02).unwrap();
        for policy in [Policy::A, Policy::B, Policy::Random] {
            let pi = p.stationary(policy).unwrap();
            let mut next = [0.0; 3];
            for r in 0..3 {
                let w = p.policy_win(policy, 0, r);
                next[(r + 1) % 3] += pi[r] * w;
                next[(r + 2) % 3] += pi[r] * (1.0 - w);
            }
            for r in 0..3 {
                assert!((next[r] - pi[r]).abs() < 1e-15);
            }
        }
    }

    #[test]
    fn long_run_mean_approaches_drift() {
        let p = Parrondo::new(100_000, 0.005).unwrap();
        for policy in [Policy::B, Policy::Random] {
            let per_round = p.exact_mean(policy) / 100_000.0;
            assert!((per_round - p.drift(policy).unwrap()).abs() < 1e-4);
        }
    }

    #[test]
    fn invalid_parameters() {
        assert!(Parrondo::new(0, 0.005).is_err());
        assert!(Parrondo::new(10, 0.1).is_err());
        assert!(Parrondo::new(10, -0.001).is_err());
        assert!(Parrondo::new(10, f64::NAN).is_err());
        assert!(Parrondo::new(10, 0.0).is_ok());
    }

    #[test]
    fn simulation_matches_exact() {
        assert_simulation_matches_exact(&Parrondo::new(200, 0.005).unwrap(), 4000);
        assert_simulation_matches_exact(&Parrondo::new(7, 0.05).unwrap(), 4000);
    }
}
