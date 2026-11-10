//! Braess's paradox in a four-node road network.
//!
//! `N` drivers travel from S to T. The roads S→A and B→T take `flow/100`
//! minutes, A→T and S→B take 45 minutes, and the optional shortcut A→B is
//! free. All costs are kept as integers in hundredths of a minute, so "a
//! strictly better route exists" is an exact integer comparison.
//!
//! One trial assigns every driver a uniformly random route and then runs
//! asynchronous best-response dynamics: while some driver can strictly lower
//! its own travel time by switching route, one such move, chosen uniformly
//! at random among all improving (driver, route) pairs, is applied.
//!
//! Termination: this is a congestion game, so Rosenthal's potential
//! `Φ = Σ_edges Σ_{j=1}^{flow} cost_edge(j)` changes by exactly the mover's
//! change in cost. Every improving move therefore lowers `Φ` by at least one
//! hundredth, `Φ ≥ 0`, and the dynamics stop after at most `Φ(start)` moves,
//! in a state where no driver can improve: a pure Nash equilibrium. The
//! iteration cap is this bound, so reaching it would indicate a bug.
//!
//! Drivers on the same route are interchangeable, so the state is the vector
//! of route counts. Picking a uniformly random improving (driver, route) pair
//! amounts to weighting each improving route change by its number of drivers.

use std::fmt;

use rand::RngExt;

use crate::engine::{Experiment, Kind, Quantity, Sample};
use crate::error::{Error, Result};
use crate::params::ensure_in;
use crate::rng::SimRng;

/// Travel time of the constant roads A→T and S→B, in hundredths of a minute.
const CONSTANT_ROAD: u64 = 4500;

/// A route from S to T.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// S→A→T.
    Sat,
    /// S→B→T.
    Sbt,
    /// S→A→B→T, using the shortcut.
    Sabt,
}

impl Route {
    fn index(self) -> usize {
        self as usize
    }
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Sat => "S-A-T",
            Self::Sbt => "S-B-T",
            Self::Sabt => "S-A-B-T",
        })
    }
}

/// The road network, with or without the shortcut A→B.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    /// Roads S→A, A→T, S→B, B→T.
    WithoutShortcut,
    /// The same roads plus the free shortcut A→B.
    WithShortcut,
}

impl Network {
    /// The routes available in this network.
    #[must_use]
    pub fn routes(self) -> &'static [Route] {
        match self {
            Self::WithoutShortcut => &[Route::Sat, Route::Sbt],
            Self::WithShortcut => &[Route::Sat, Route::Sbt, Route::Sabt],
        }
    }
}

/// Number of drivers on each route, indexed by [`Route`] in declaration order.
pub type Counts = [u64; 3];

/// Travel time of `route` in hundredths of a minute when `counts` drivers use
/// the routes; the driver's own contribution is included.
///
/// ```
/// use aporia::experiments::braess::route_cost;
/// use aporia::experiments::Route;
/// // 4000 drivers all on S-A-B-T: 40 + 0 + 40 minutes.
/// assert_eq!(route_cost(&[0, 0, 4000], Route::Sabt), 8000);
/// ```
#[must_use]
pub fn route_cost(counts: &Counts, route: Route) -> u64 {
    let [via_a, via_b, shortcut] = *counts;
    let s_to_a = via_a + shortcut;
    let b_to_t = via_b + shortcut;
    match route {
        Route::Sat => s_to_a + CONSTANT_ROAD,
        Route::Sbt => CONSTANT_ROAD + b_to_t,
        Route::Sabt => s_to_a + b_to_t,
    }
}

/// The cost a driver on `from` would pay after switching alone to `to`.
fn deviation_cost(counts: &Counts, from: Route, to: Route) -> u64 {
    let mut after = *counts;
    after[from.index()] -= 1;
    after[to.index()] += 1;
    route_cost(&after, to)
}

/// A terminal state of best-response dynamics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Equilibrium {
    /// Drivers per route.
    pub counts: Counts,
    /// Improving moves made from the random start.
    pub steps: u64,
}

impl Equilibrium {
    /// Mean travel time over all drivers, in minutes.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // totals stay far below 2^53
    pub fn mean_time(&self) -> f64 {
        let total: u64 = [Route::Sat, Route::Sbt, Route::Sabt]
            .iter()
            .map(|&r| self.counts[r.index()] * route_cost(&self.counts, r))
            .sum();
        let drivers: u64 = self.counts.iter().sum();
        total as f64 / (drivers as f64 * 100.0)
    }
}

/// Validated number of drivers.
///
/// ```
/// use aporia::experiments::{Braess, Network};
/// let b = Braess::new(4000)?;
/// assert_eq!(b.exact_mean_time(Network::WithoutShortcut), Some(65.0));
/// assert_eq!(b.exact_mean_time(Network::WithShortcut), Some(80.0));
/// # Ok::<(), aporia::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Braess {
    drivers: u32,
}

impl Braess {
    /// Largest number of drivers.
    pub const MAX_DRIVERS: u32 = 1_000_000;

    /// Validates `2 ≤ drivers ≤ MAX_DRIVERS`.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidParameter`] otherwise.
    pub fn new(drivers: u32) -> Result<Self> {
        Ok(Self {
            drivers: ensure_in("drivers", drivers, 2..=Self::MAX_DRIVERS)?,
        })
    }

    /// Exact equilibrium mean travel time in minutes, where it is known in closed form.
    ///
    /// - Without the shortcut and even `N`: a driver gains by switching iff
    ///   its route carries at least two more drivers than the other, so the
    ///   only equilibrium is the even split, at `N/200 + 45` minutes.
    /// - With the shortcut and `N ≤ 4498`: a driver on S-A-T (or S-B-T)
    ///   gains by switching to S-A-B-T iff the other congestible road carries
    ///   at most 4498 drivers, which always holds; so in equilibrium all use
    ///   S-A-B-T, at `2N/100 = N/50` minutes. No one leaves, because S-A-T
    ///   would cost `N/100 + 45 ≥ N/50`.
    ///
    /// Otherwise `None`.
    #[must_use]
    pub fn exact_mean_time(&self, network: Network) -> Option<f64> {
        let n = f64::from(self.drivers);
        match network {
            Network::WithoutShortcut => self.drivers.is_multiple_of(2).then(|| n / 200.0 + 45.0),
            Network::WithShortcut => (self.drivers <= 4498).then(|| n / 50.0),
        }
    }

    /// Upper bound on Rosenthal's potential, hence on the number of improving moves.
    fn step_cap(self) -> u64 {
        let n = u64::from(self.drivers);
        // Two congestible roads with Σ_{j=1}^{n} j each, two constant roads with n · 4500 each.
        n * (n + 1) + 2 * CONSTANT_ROAD * n
    }

    /// Runs one trial: random routes, then best-response dynamics to equilibrium.
    ///
    /// # Errors
    /// Returns [`Error::IterationCap`] if the potential bound is exceeded,
    /// which the argument in the module documentation rules out.
    pub fn equilibrium(&self, network: Network, rng: &mut SimRng) -> Result<Equilibrium> {
        let routes = network.routes();
        let mut counts: Counts = [0; 3];
        for _ in 0..self.drivers {
            // The index is drawn from 0..routes.len().
            let r = routes[rng.random_range(0..routes.len())];
            counts[r.index()] += 1;
        }
        let cap = self.step_cap();
        let mut steps = 0;
        // At most 3 · 2 route changes; a fixed array avoids allocation.
        let mut moves = [(Route::Sat, Route::Sat, 0u64); 6];
        loop {
            let mut len = 0;
            let mut total = 0;
            for &from in routes {
                let drivers = counts[from.index()];
                if drivers == 0 {
                    continue;
                }
                let current = route_cost(&counts, from);
                for &to in routes {
                    if to != from && deviation_cost(&counts, from, to) < current {
                        // len < 6: at most 3 · 2 ordered route pairs exist.
                        moves[len] = (from, to, drivers);
                        len += 1;
                        total += drivers;
                    }
                }
            }
            if total == 0 {
                return Ok(Equilibrium { counts, steps });
            }
            if steps >= cap {
                return Err(Error::IterationCap {
                    procedure: "best-response dynamics",
                    cap,
                });
            }
            let mut u = rng.random_range(0..total);
            for &(from, to, weight) in &moves[..len] {
                if u < weight {
                    counts[from.index()] -= 1;
                    counts[to.index()] += 1;
                    break;
                }
                u -= weight;
            }
            steps += 1;
        }
    }
}

impl Default for Braess {
    fn default() -> Self {
        Self { drivers: 4000 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Measure {
    MeanTime(Network),
    Steps,
}

struct Traffic {
    params: Braess,
    measure: Measure,
}

impl Quantity for Traffic {
    fn label(&self) -> &'static str {
        match self.measure {
            Measure::MeanTime(Network::WithoutShortcut) => "mean minutes, no shortcut",
            Measure::MeanTime(Network::WithShortcut) => "mean minutes, shortcut",
            Measure::Steps => "improving moves, shortcut",
        }
    }

    fn kind(&self) -> Kind {
        Kind::Real
    }

    fn exact(&self) -> Option<f64> {
        match self.measure {
            Measure::MeanTime(network) => self.params.exact_mean_time(network),
            Measure::Steps => None,
        }
    }

    fn draw(&mut self, rng: &mut SimRng) -> Result<Sample> {
        Ok(Sample::Value(match self.measure {
            Measure::MeanTime(network) => self.params.equilibrium(network, rng)?.mean_time(),
            #[allow(clippy::cast_precision_loss)] // steps ≤ step_cap < 2^53
            Measure::Steps => self.params.equilibrium(Network::WithShortcut, rng)?.steps as f64,
        }))
    }
}

impl Experiment for Braess {
    fn id(&self) -> &'static str {
        "braess"
    }

    fn title(&self) -> &'static str {
        "Braess's paradox"
    }

    fn intuition(&self) -> &'static str {
        "Adding a free road cannot make everybody's trip longer."
    }

    fn lesson(&self) -> String {
        "Equilibrium is not optimum. Each switch to the shortcut is individually rational, \
         but it loads the roads S-A and B-T that everybody shares. Between 3001 and 4498 \
         drivers the new equilibrium is strictly worse for every driver; below 3000 the \
         shortcut helps."
            .to_owned()
    }

    fn default_trials(&self) -> u64 {
        200
    }

    fn parameters(&self) -> Vec<(&'static str, String)> {
        vec![("drivers", self.drivers.to_string())]
    }

    fn quantities(&self) -> Vec<Box<dyn Quantity>> {
        [
            Measure::MeanTime(Network::WithoutShortcut),
            Measure::MeanTime(Network::WithShortcut),
            Measure::Steps,
        ]
        .into_iter()
        .map(|measure| {
            Box::new(Traffic {
                params: *self,
                measure,
            }) as Box<dyn Quantity>
        })
        .collect()
    }

    fn notes(&self) -> Vec<String> {
        let without = self.exact_mean_time(Network::WithoutShortcut);
        let with = self.exact_mean_time(Network::WithShortcut);
        let mut notes = Vec::new();
        if with.is_some() {
            notes.push(format!(
                "With the shortcut, a driver leaving S-A-B-T for S-A-T or S-B-T would pay \
                 N/100+45 = {:.4} minutes instead of N/50 = {:.4}.",
                f64::from(self.drivers) / 100.0 + 45.0,
                f64::from(self.drivers) / 50.0
            ));
        }
        if let (Some(a), Some(b)) = (without, with) {
            notes.push(format!(
                "The shortcut changes every driver's equilibrium time from {a:.4} to {b:.4} \
                 minutes ({:+.2} %).",
                100.0 * (b - a) / a
            ));
        }
        notes
    }

    fn supports_plot(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::cast_possible_truncation)] // exact multiples of 1/100; small counts
mod tests {
    use super::*;
    use crate::experiments::testing::assert_simulation_matches_exact;
    use crate::rng::stream;

    /// Independent brute force: expand the counts into one route per driver,
    /// compute edge flows from explicit edge lists, and try every unilateral
    /// deviation of every driver.
    fn is_nash(counts: &Counts, network: Network) -> bool {
        const EDGES: [&[usize]; 3] = [&[0, 1], &[2, 3], &[0, 4, 3]]; // SA AT | SB BT | SA AB BT
        let edge_cost = |e: usize, flow: u64| match e {
            0 | 3 => flow,
            1 | 2 => 4500,
            _ => 0,
        };
        let drivers: Vec<usize> = (0..3)
            .flat_map(|r| std::iter::repeat_n(r, counts[r] as usize))
            .collect();
        let mut flow = [0u64; 5];
        for &r in &drivers {
            for &e in EDGES[r] {
                flow[e] += 1;
            }
        }
        let cost = |flow: &[u64; 5], r: usize| -> u64 {
            EDGES[r].iter().map(|&e| edge_cost(e, flow[e])).sum()
        };
        let allowed = match network {
            Network::WithoutShortcut => 2,
            Network::WithShortcut => 3,
        };
        drivers.iter().all(|&r| {
            let current = cost(&flow, r);
            (0..allowed).filter(|&s| s != r).all(|s| {
                let mut f = flow;
                for &e in EDGES[r] {
                    f[e] -= 1;
                }
                for &e in EDGES[s] {
                    f[e] += 1;
                }
                cost(&f, s) >= current
            })
        })
    }

    #[test]
    fn dynamics_end_in_nash_equilibrium() {
        let mut rng = stream(5, "test", "braess");
        for n in [2, 3, 7, 100, 1001, 3000, 4000, 4499, 4500, 5000, 9001] {
            let b = Braess::new(n).unwrap();
            for network in [Network::WithoutShortcut, Network::WithShortcut] {
                for _ in 0..3 {
                    let eq = b.equilibrium(network, &mut rng).unwrap();
                    assert!(is_nash(&eq.counts, network), "n={n} {network:?} {eq:?}");
                    assert_eq!(eq.counts.iter().sum::<u64>(), u64::from(n));
                    if network == Network::WithoutShortcut {
                        assert_eq!(eq.counts[2], 0);
                    }
                }
            }
        }
    }

    #[test]
    fn oracles() {
        for (n, without, with) in [(4000, 65.0, 80.0), (3000, 60.0, 60.0), (2000, 55.0, 40.0)] {
            let b = Braess::new(n).unwrap();
            assert_eq!(b.exact_mean_time(Network::WithoutShortcut), Some(without));
            assert_eq!(b.exact_mean_time(Network::WithShortcut), Some(with));
            // The equilibrium is unique, so every trial reproduces it exactly.
            let mut rng = stream(6, "test", "braess-oracle");
            for (network, expected) in [
                (Network::WithoutShortcut, without),
                (Network::WithShortcut, with),
            ] {
                let t = b.equilibrium(network, &mut rng).unwrap().mean_time();
                assert!((t - expected).abs() < 1e-9);
            }
        }
        let b = Braess::new(5000).unwrap();
        assert_eq!(b.exact_mean_time(Network::WithShortcut), None);
        assert_eq!(b.exact_mean_time(Network::WithoutShortcut), Some(70.0));
        assert_eq!(
            Braess::new(4001)
                .unwrap()
                .exact_mean_time(Network::WithoutShortcut),
            None
        );
    }

    #[test]
    fn simulation_matches_exact_with_degenerate_intervals() {
        assert_simulation_matches_exact(&Braess::new(4000).unwrap(), 20);
        let r = crate::run(&Braess::default(), &crate::experiments::testing::config(20)).unwrap();
        for q in &r.quantities[..2] {
            assert_eq!(q.estimate.lower, q.estimate.upper);
            assert_eq!(q.inside(), Some(true));
        }
        assert_eq!(r.quantities[2].inside(), None);
    }

    #[test]
    fn invalid_parameters() {
        assert!(Braess::new(1).is_err());
        assert!(Braess::new(Braess::MAX_DRIVERS + 1).is_err());
    }
}
