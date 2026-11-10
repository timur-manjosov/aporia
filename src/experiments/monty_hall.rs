//! The Monty Hall problem, generalised to `n` doors and `k` opened doors.
//!
//! The simulation plays the physical game: a car is placed, the player picks,
//! the host opens doors, the player stays or switches. With an ignorant host
//! the car is sometimes revealed; those games are rejected, which conditions
//! on the event the player actually observed.

use std::fmt;

use rand::RngExt;

use crate::engine::{Experiment, Kind, Quantity, Sample};
use crate::error::{Result, invalid};
use crate::params::ensure_in;
use crate::rng::SimRng;

/// What the host knows when opening doors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Host {
    /// Knows where the car is and never reveals it.
    #[default]
    Knowing,
    /// Opens doors at random; games in which the car is revealed are rejected.
    Ignorant,
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Knowing => "knowing",
            Self::Ignorant => "ignorant",
        })
    }
}

/// Validated parameters of a Monty Hall game.
///
/// ```
/// use aporia::experiments::{MontyHall, Host};
/// let g = MontyHall::new(3, 1, Host::Knowing)?;
/// assert!((g.exact_switch() - 2.0 / 3.0).abs() < 1e-15);
/// assert!(MontyHall::new(3, 2, Host::Knowing).is_err()); // must leave a door to switch to
/// # Ok::<(), aporia::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MontyHall {
    doors: u32,
    opened: u32,
    host: Host,
}

impl MontyHall {
    /// Largest supported number of doors.
    pub const MAX_DOORS: u32 = 1000;

    /// Validates `3 ≤ doors ≤ MAX_DOORS` and `1 ≤ opened ≤ doors − 2`.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidParameter`] otherwise.
    pub fn new(doors: u32, opened: u32, host: Host) -> Result<Self> {
        let doors = ensure_in("doors", doors, 3..=Self::MAX_DOORS)?;
        if !(1..=doors - 2).contains(&opened) {
            return Err(invalid(
                "opened",
                format!("must be between 1 and doors - 2 = {}", doors - 2),
            ));
        }
        Ok(Self {
            doors,
            opened,
            host,
        })
    }

    /// Exact `P(win | stay)`: the first pick is right with probability `1/n`.
    /// For the ignorant host, conditional on the car not being revealed, all
    /// `n − k` closed doors are symmetric, so it is `1/(n − k)`.
    #[must_use]
    pub fn exact_stay(&self) -> f64 {
        let (n, k) = (f64::from(self.doors), f64::from(self.opened));
        match self.host {
            Host::Knowing => 1.0 / n,
            Host::Ignorant => 1.0 / (n - k),
        }
    }

    /// Exact `P(win | switch)`. With a knowing host the car is behind one of
    /// the `n − k − 1` other closed doors with probability `(n − 1)/n`.
    #[must_use]
    pub fn exact_switch(&self) -> f64 {
        let (n, k) = (f64::from(self.doors), f64::from(self.opened));
        match self.host {
            Host::Knowing => (n - 1.0) / (n * (n - k - 1.0)),
            Host::Ignorant => 1.0 / (n - k),
        }
    }

    /// Probability that an ignorant host does not reveal the car: the car is
    /// behind the pick (`1/n`), or behind one of the `n − 1 − k` doors the host
    /// left closed among the `n − 1` others.
    #[must_use]
    pub fn acceptance(&self) -> f64 {
        let (n, k) = (f64::from(self.doors), f64::from(self.opened));
        match self.host {
            Host::Knowing => 1.0,
            Host::Ignorant => (n - k) / n,
        }
    }
}

impl Default for MontyHall {
    fn default() -> Self {
        Self {
            doors: 3,
            opened: 1,
            host: Host::Knowing,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strategy {
    Stay,
    Switch,
}

/// One strategy played against the configured host, with reusable buffers.
struct Game {
    params: MontyHall,
    strategy: Strategy,
    is_open: Vec<bool>,
    candidates: Vec<u32>,
}

impl Game {
    fn new(params: MontyHall, strategy: Strategy) -> Self {
        Self {
            params,
            strategy,
            is_open: vec![false; params.doors as usize],
            candidates: Vec::with_capacity(params.doors as usize),
        }
    }
}

impl Quantity for Game {
    fn label(&self) -> &'static str {
        match self.strategy {
            Strategy::Stay => "P(win | stay)",
            Strategy::Switch => "P(win | switch)",
        }
    }

    fn kind(&self) -> Kind {
        Kind::Binary
    }

    fn exact(&self) -> Option<f64> {
        Some(match self.strategy {
            Strategy::Stay => self.params.exact_stay(),
            Strategy::Switch => self.params.exact_switch(),
        })
    }

    fn draw(&mut self, rng: &mut SimRng) -> Result<Sample> {
        let MontyHall {
            doors,
            opened,
            host,
        } = self.params;
        let car = rng.random_range(0..doors);
        let pick = rng.random_range(0..doors);

        // The doors the host may open, then a partial Fisher-Yates shuffle
        // selects `opened` of them uniformly without replacement.
        self.candidates.clear();
        self.candidates
            .extend((0..doors).filter(|&d| d != pick && (host == Host::Ignorant || d != car)));
        let len = self.candidates.len();
        let k = opened as usize;
        // len ≥ k: the knowing host has n − 2 ≥ k candidates (n − 1 if pick = car).
        for i in 0..k {
            let j = rng.random_range(i..len);
            self.candidates.swap(i, j);
        }
        let shown = &self.candidates[..k];
        if host == Host::Ignorant && shown.contains(&car) {
            return Ok(Sample::Rejected);
        }

        let choice = match self.strategy {
            Strategy::Stay => pick,
            Strategy::Switch => {
                for &d in shown {
                    self.is_open[d as usize] = true; // d < doors == is_open.len()
                }
                // The player sees only which doors are open, never the car.
                let closed_others = doors - opened - 1;
                let r = rng.random_range(0..closed_others) as usize;
                let choice = (0..doors)
                    .filter(|&d| d != pick && !self.is_open[d as usize])
                    .nth(r)
                    .unwrap_or(pick); // unreachable: exactly `closed_others` doors pass the filter
                for &d in shown {
                    self.is_open[d as usize] = false;
                }
                choice
            }
        };
        Ok(Sample::Hit(choice == car))
    }
}

impl Experiment for MontyHall {
    fn id(&self) -> &'static str {
        "monty-hall"
    }

    fn title(&self) -> &'static str {
        "Monty Hall problem"
    }

    fn intuition(&self) -> &'static str {
        "Two doors remain, so it is 50/50."
    }

    fn lesson(&self) -> String {
        match self.host {
            Host::Knowing => "The information is in the host's knowledge, not in the revealed \
                goat. A knowing host never opens the car, so the probability that the first \
                pick was wrong is concentrated on the doors the host left closed."
                .to_owned(),
            Host::Ignorant => "With an ignorant host it really is 50/50 (in general 1/(n-k) \
                per closed door): a goat revealed by chance carries no information about \
                which closed door hides the car. The information was in the knowledge, \
                not in the goat."
                .to_owned(),
        }
    }

    fn default_trials(&self) -> u64 {
        1_000_000
    }

    fn parameters(&self) -> Vec<(&'static str, String)> {
        vec![
            ("doors", self.doors.to_string()),
            ("opened", self.opened.to_string()),
            ("host", self.host.to_string()),
        ]
    }

    fn quantities(&self) -> Vec<Box<dyn Quantity>> {
        vec![
            Box::new(Game::new(*self, Strategy::Stay)),
            Box::new(Game::new(*self, Strategy::Switch)),
        ]
    }

    fn notes(&self) -> Vec<String> {
        match self.host {
            Host::Knowing => vec![format!(
                "Switching wins (n-1)/(n-k-1) = {:.6} times as often as staying.",
                self.exact_switch() / self.exact_stay()
            )],
            Host::Ignorant => vec![format!(
                "Probability that the host does not reveal the car (acceptance rate): \
                 (n-k)/n = {:.6}.",
                self.acceptance()
            )],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RunConfig;
    use crate::experiments::testing::{assert_simulation_matches_exact, config};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    #[test]
    fn oracles() {
        let g = MontyHall::new(3, 1, Host::Knowing).unwrap();
        assert!(close(g.exact_stay(), 1.0 / 3.0) && close(g.exact_switch(), 2.0 / 3.0));
        let g = MontyHall::new(3, 1, Host::Ignorant).unwrap();
        assert!(close(g.exact_stay(), 0.5) && close(g.exact_switch(), 0.5));
        assert!(close(g.acceptance(), 2.0 / 3.0));
        let g = MontyHall::new(5, 2, Host::Knowing).unwrap();
        assert!(close(g.exact_stay(), 0.2) && close(g.exact_switch(), 0.4));
        let g = MontyHall::new(5, 2, Host::Ignorant).unwrap();
        assert!(close(g.exact_stay(), 1.0 / 3.0) && close(g.exact_switch(), 1.0 / 3.0));
        let g = MontyHall::new(100, 98, Host::Knowing).unwrap();
        assert!(close(g.exact_stay(), 0.01) && close(g.exact_switch(), 0.99));
    }

    #[test]
    fn invalid_parameters() {
        assert!(MontyHall::new(2, 1, Host::Knowing).is_err());
        assert!(MontyHall::new(1001, 1, Host::Knowing).is_err());
        assert!(MontyHall::new(5, 0, Host::Knowing).is_err());
        assert!(MontyHall::new(5, 4, Host::Ignorant).is_err());
        assert!(MontyHall::new(1000, 998, Host::Knowing).is_ok());
    }

    #[test]
    fn simulation_matches_exact() {
        for (n, k) in [(3, 1), (5, 2), (10, 8), (100, 98)] {
            for host in [Host::Knowing, Host::Ignorant] {
                let g = MontyHall::new(n, k, host).unwrap();
                assert_simulation_matches_exact(&g, 20_000);
            }
        }
    }

    #[test]
    fn ignorant_acceptance_rate() {
        for (n, k) in [(3, 1), (5, 2), (10, 3)] {
            let g = MontyHall::new(n, k, Host::Ignorant).unwrap();
            let cfg: RunConfig = config(40_000);
            let r = crate::run(&g, &cfg).unwrap();
            let p = g.acceptance();
            for q in &r.quantities {
                #[allow(clippy::cast_precision_loss)] // small counts
                let rate = q.accepted as f64 / q.attempted as f64;
                // z = 4 binomial bound on the acceptance proportion.
                let tol = 4.0 * (p * (1.0 - p) / 40_000.0).sqrt();
                assert!((rate - p).abs() < tol, "n={n} k={k}: {rate} vs {p}");
            }
        }
    }

    #[test]
    fn knowing_host_never_rejects() {
        let r = crate::run(&MontyHall::default(), &config(1000)).unwrap();
        assert!(r.quantities.iter().all(|q| q.accepted == q.attempted));
    }
}
