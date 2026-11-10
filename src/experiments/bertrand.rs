//! Bertrand's chord paradox.
//!
//! "Choose a random chord of the unit circle. What is the probability that it
//! is longer than the side √3 of the inscribed equilateral triangle?" Three
//! reasonable ways to choose a chord give three different answers, and each
//! answer is exactly right for its model. Every sampler below constructs an
//! actual chord (two endpoints on the circle); the event is then tested on
//! the squared length, so no square root enters the comparison.

use std::f64::consts::TAU;

use rand::RngExt;

use crate::engine::{Experiment, Kind, Quantity, Sample};
use crate::error::Result;
use crate::rng::SimRng;

/// A point in the plane.
pub type Point = (f64, f64);

/// A chord of the unit circle, given by its endpoints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chord {
    /// First endpoint.
    pub a: Point,
    /// Second endpoint.
    pub b: Point,
}

impl Chord {
    /// Squared length.
    ///
    /// ```
    /// use aporia::experiments::Chord;
    /// let diameter = Chord { a: (-1.0, 0.0), b: (1.0, 0.0) };
    /// assert_eq!(diameter.len_sq(), 4.0);
    /// ```
    #[must_use]
    pub fn len_sq(&self) -> f64 {
        (self.a.0 - self.b.0).powi(2) + (self.a.1 - self.b.1).powi(2)
    }

    /// Whether the chord is longer than the side of the inscribed equilateral triangle.
    #[must_use]
    pub fn is_long(&self) -> bool {
        self.len_sq() > 3.0
    }

    /// The chord through `midpoint` perpendicular to the radius through it.
    ///
    /// For the centre itself every diameter qualifies; the horizontal one is
    /// returned. That event has probability zero under all samplers here.
    #[must_use]
    pub fn from_midpoint(midpoint: Point) -> Self {
        let (x, y) = midpoint;
        let r2 = x * x + y * y;
        let half = (1.0 - r2).max(0.0).sqrt();
        let (ux, uy) = if r2 > 0.0 {
            let r = r2.sqrt();
            (-y / r, x / r)
        } else {
            (1.0, 0.0)
        };
        Self {
            a: (x - half * ux, y - half * uy),
            b: (x + half * ux, y + half * uy),
        }
    }
}

/// How a "random chord" is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Two independent uniform points on the circle.
    Endpoints,
    /// A uniform direction, and a chord perpendicular to it at a distance
    /// from the centre that is uniform in `[0, 1]`.
    Radial,
    /// A midpoint uniform over the area of the disk.
    Midpoint,
}

impl Method {
    /// All methods in presentation order.
    pub const ALL: [Self; 3] = [Self::Endpoints, Self::Radial, Self::Midpoint];

    /// Samples one chord according to this method.
    pub fn sample(self, rng: &mut SimRng) -> Chord {
        match self {
            Self::Endpoints => {
                let (s, t): (f64, f64) = (rng.random_range(0.0..TAU), rng.random_range(0.0..TAU));
                Chord {
                    a: (s.cos(), s.sin()),
                    b: (t.cos(), t.sin()),
                }
            }
            Self::Radial => {
                let phi: f64 = rng.random_range(0.0..TAU);
                let dist: f64 = rng.random_range(0.0..=1.0);
                Chord::from_midpoint((dist * phi.cos(), dist * phi.sin()))
            }
            Self::Midpoint => loop {
                // Rejection from the enclosing square yields a point uniform
                // in area; accepting with probability π/4 per attempt.
                let x: f64 = rng.random_range(-1.0..=1.0);
                let y: f64 = rng.random_range(-1.0..=1.0);
                if x * x + y * y <= 1.0 {
                    break Chord::from_midpoint((x, y));
                }
            },
        }
    }

    /// The exact probability of a long chord under this method.
    ///
    /// - Endpoints: fix the first endpoint; the chord is long iff the second
    ///   lies on the opposite third of the circle: 1/3.
    /// - Radial: long iff the distance from the centre is below 1/2: 1/2.
    /// - Midpoint: long iff the midpoint lies in the disk of radius 1/2,
    ///   which has a quarter of the area: 1/4.
    ///
    /// ```
    /// use aporia::experiments::Method;
    /// assert_eq!(Method::Midpoint.exact(), 0.25);
    /// ```
    #[must_use]
    pub fn exact(self) -> f64 {
        match self {
            Self::Endpoints => 1.0 / 3.0,
            Self::Radial => 0.5,
            Self::Midpoint => 0.25,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Endpoints => "P(long | endpoints)",
            Self::Radial => "P(long | radial)",
            Self::Midpoint => "P(long | midpoint)",
        }
    }
}

/// Bertrand's paradox; it has no parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bertrand;

struct LongChord(Method);

impl Quantity for LongChord {
    fn label(&self) -> &'static str {
        self.0.label()
    }

    fn kind(&self) -> Kind {
        Kind::Binary
    }

    fn exact(&self) -> Option<f64> {
        Some(self.0.exact())
    }

    fn draw(&mut self, rng: &mut SimRng) -> Result<Sample> {
        Ok(Sample::Hit(self.0.sample(rng).is_long()))
    }
}

impl Experiment for Bertrand {
    fn id(&self) -> &'static str {
        "bertrand"
    }

    fn title(&self) -> &'static str {
        "Bertrand's chord paradox"
    }

    fn intuition(&self) -> &'static str {
        "A random chord is a random chord; there is one answer."
    }

    fn lesson(&self) -> String {
        "\"Random chord\" does not define a probability space. The question becomes a \
         well-posed problem only once the sampling mechanism is specified, and each of the \
         three answers is exactly right for its own mechanism."
            .to_owned()
    }

    fn default_trials(&self) -> u64 {
        1_000_000
    }

    fn parameters(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }

    fn quantities(&self) -> Vec<Box<dyn Quantity>> {
        Method::ALL
            .iter()
            .map(|&m| Box::new(LongChord(m)) as Box<dyn Quantity>)
            .collect()
    }

    fn notes(&self) -> Vec<String> {
        vec![
            "A chord is long when its squared length exceeds 3, the squared side of the \
              inscribed equilateral triangle."
                .to_owned(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiments::testing::{Z, assert_simulation_matches_exact};
    use crate::rng::stream;
    use crate::stats::Tally;

    #[test]
    fn simulation_matches_exact() {
        assert_simulation_matches_exact(&Bertrand, 40_000);
    }

    #[test]
    fn chords_have_endpoints_on_the_circle() {
        let mut rng = stream(3, "test", "bertrand");
        for m in Method::ALL {
            for _ in 0..1000 {
                let c = m.sample(&mut rng);
                for (x, y) in [c.a, c.b] {
                    assert!((x * x + y * y - 1.0).abs() < 1e-12);
                }
            }
        }
    }

    /// A lesson, kept as a test: choosing the midpoint's *radius* uniformly in
    /// `[0, 1]` is not the same as choosing the midpoint uniformly in the disk.
    /// The area element is `r dr dθ`, so a uniform radius over-weights the
    /// centre; the probability of a long chord becomes `P(r < 1/2) = 1/2`
    /// (the radial answer) instead of `1/4`.
    #[test]
    fn uniform_radius_is_the_jacobian_mistake() {
        let mut rng = stream(4, "test", "jacobian");
        let mut tally = Tally::default();
        let n = 40_000;
        for _ in 0..n {
            let r: f64 = rng.random_range(0.0..=1.0);
            let theta: f64 = rng.random_range(0.0..TAU);
            tally.push(Chord::from_midpoint((r * theta.cos(), r * theta.sin())).is_long());
        }
        let e = crate::stats::wilson(tally.hits, tally.n, Z).unwrap();
        assert!(e.contains(0.5));
        assert!(!e.contains(0.25));
    }
}
