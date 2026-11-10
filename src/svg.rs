//! A minimal hand-written SVG line plot of running estimates.
//!
//! One figure type only: the running estimate of each quantity against the
//! number of trials on a logarithmic axis, with a shaded confidence band and
//! a dashed line at the exact value. Colours are from the Okabe-Ito palette,
//! which stays distinguishable under the common forms of colour blindness;
//! the background is opaque so the figure reads on light and dark pages.

use std::fmt::Write as _;

use crate::engine::Series;
use crate::stats::Confidence;

const WIDTH: f64 = 800.0;
const HEIGHT: f64 = 480.0;
const LEFT: f64 = 72.0;
const RIGHT: f64 = 24.0;
const TOP: f64 = 74.0;
const BOTTOM: f64 = 64.0;
/// Okabe-Ito blue, vermillion, bluish green, reddish purple, orange, sky blue.
const PALETTE: [&str; 6] = [
    "#0072B2", "#D55E00", "#009E73", "#CC79A7", "#E69F00", "#56B4E9",
];
const INK: &str = "#222222";
const GRID: &str = "#DDDDDD";

/// Escapes the five XML special characters.
///
/// ```
/// assert_eq!(aporia::svg::escape("P(a < b) & \"c\""), "P(a &lt; b) &amp; &quot;c&quot;");
/// ```
#[must_use]
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Linear map from a data interval onto a pixel interval.
#[derive(Clone, Copy)]
struct Scale {
    d0: f64,
    d1: f64,
    p0: f64,
    p1: f64,
}

impl Scale {
    fn new(d0: f64, d1: f64, p0: f64, p1: f64) -> Self {
        // A degenerate domain would divide by zero; widen it symmetrically.
        let (d0, d1) = if (d1 - d0).abs() < 1e-12 {
            (d0 - 0.5, d1 + 0.5)
        } else {
            (d0, d1)
        };
        Self { d0, d1, p0, p1 }
    }

    fn map(self, x: f64) -> f64 {
        self.p0 + (x - self.d0) / (self.d1 - self.d0) * (self.p1 - self.p0)
    }
}

/// Tick positions at multiples of 1, 2 or 5 times a power of ten.
fn nice_ticks(lo: f64, hi: f64) -> Vec<f64> {
    let raw = (hi - lo) / 6.0;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|m| m * mag)
        .find(|&s| s >= raw)
        .unwrap_or(10.0 * mag);
    let mut ticks = Vec::new();
    let mut t = (lo / step).ceil() * step;
    while t <= hi + step * 1e-9 {
        // Snap values like 0.30000000000000004 so labels print cleanly.
        ticks.push((t / step).round() * step);
        t += step;
    }
    ticks
}

fn decimals(step: f64) -> usize {
    if step >= 1.0 {
        0
    } else {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // step < 1 so -log10(step) is positive and small
        let d = (-step.log10()).ceil() as usize;
        d
    }
}

/// Renders the convergence plot of `series` as a standalone SVG document.
///
/// ```
/// use aporia::engine::{Checkpoint, Series};
/// use aporia::stats::{Confidence, Estimate};
/// let s = Series {
///     label: "P(hit)".into(),
///     exact: Some(0.5),
///     points: vec![
///         Checkpoint { trials: 10, estimate: Some(Estimate { point: 0.4, lower: 0.2, upper: 0.7 }) },
///         Checkpoint { trials: 100, estimate: Some(Estimate { point: 0.52, lower: 0.42, upper: 0.61 }) },
///     ],
/// };
/// let svg = aporia::svg::convergence_plot("Coin", &[s], Confidence::P95);
/// assert!(svg.starts_with("<svg") && !svg.contains("NaN"));
/// ```
#[must_use]
pub fn convergence_plot(title: &str, series: &[Series], confidence: Confidence) -> String {
    let points = || {
        series
            .iter()
            .flat_map(|s| s.points.iter())
            .filter_map(|c| c.estimate.map(|e| (c.trials, e)))
            .filter(|(_, e)| e.lower.is_finite() && e.upper.is_finite())
    };
    let (mut x_min, mut x_max) = (u64::MAX, 1);
    let (mut y_min, mut y_max) = (f64::INFINITY, f64::NEG_INFINITY);
    for (t, e) in points() {
        x_min = x_min.min(t);
        x_max = x_max.max(t);
        y_min = y_min.min(e.lower);
        y_max = y_max.max(e.upper);
    }
    for x in series.iter().filter_map(|s| s.exact) {
        y_min = y_min.min(x);
        y_max = y_max.max(x);
    }
    if !y_min.is_finite() || !y_max.is_finite() {
        (y_min, y_max) = (0.0, 1.0);
    }
    let x_min = x_min.min(x_max);
    let pad = 0.05 * (y_max - y_min).max(1e-9);
    let ticks = nice_ticks(y_min - pad, y_max + pad);
    let y_lo = ticks.first().copied().unwrap_or(y_min).min(y_min - pad);
    let y_hi = ticks.last().copied().unwrap_or(y_max).max(y_max + pad);
    #[allow(clippy::cast_precision_loss)] // only used for pixel positions
    let xs = Scale::new(
        (x_min as f64).log10(),
        (x_max as f64).log10(),
        LEFT,
        WIDTH - RIGHT,
    );
    let ys = Scale::new(y_lo, y_hi, HEIGHT - BOTTOM, TOP);

    let mut svg = String::new();
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}" font-family="Helvetica, Arial, sans-serif" font-size="13" fill="{INK}">"#
    );
    let _ = writeln!(
        svg,
        r##"<rect width="{WIDTH}" height="{HEIGHT}" fill="#FFFFFF"/>"##
    );
    let _ = writeln!(
        svg,
        r#"<text x="{LEFT}" y="28" font-size="17" font-weight="bold">{}</text>"#,
        escape(title)
    );

    axes(&mut svg, xs, ys, &ticks, (x_min, x_max), confidence);
    curves(&mut svg, xs, ys, series);
    svg.push_str("</svg>\n");
    svg
}

/// Title-independent furniture: grid, tick labels, frame, axis labels.
fn axes(
    svg: &mut String,
    xs: Scale,
    ys: Scale,
    ticks: &[f64],
    (x_min, x_max): (u64, u64),
    confidence: Confidence,
) {
    #[allow(clippy::cast_precision_loss)] // only used for pixel positions
    let px = |t: u64| xs.map((t as f64).log10());
    // Horizontal grid and y labels.
    let step = if ticks.len() > 1 {
        ticks[1] - ticks[0]
    } else {
        1.0
    };
    let dec = decimals(step);
    for &t in ticks {
        let y = ys.map(t);
        let _ = writeln!(
            svg,
            r#"<line x1="{LEFT}" y1="{y:.2}" x2="{:.2}" y2="{y:.2}" stroke="{GRID}"/>"#,
            WIDTH - RIGHT
        );
        let _ = writeln!(
            svg,
            r#"<text x="{:.2}" y="{:.2}" text-anchor="end">{t:.dec$}</text>"#,
            LEFT - 8.0,
            y + 4.0
        );
    }
    // Vertical grid at powers of ten, labelled 10^k.
    let mut p = 1u64;
    for k in 0..20 {
        if p >= x_min && p <= x_max {
            let x = px(p);
            let _ = writeln!(
                svg,
                r#"<line x1="{x:.2}" y1="{TOP}" x2="{x:.2}" y2="{:.2}" stroke="{GRID}"/>"#,
                HEIGHT - BOTTOM
            );
            let _ = writeln!(
                svg,
                r#"<text x="{x:.2}" y="{:.2}" text-anchor="middle">10<tspan dy="-6" font-size="10">{k}</tspan></text>"#,
                HEIGHT - BOTTOM + 20.0
            );
        }
        match p.checked_mul(10) {
            Some(next) => p = next,
            None => break,
        }
    }
    let _ = writeln!(
        svg,
        r#"<rect x="{LEFT}" y="{TOP}" width="{:.2}" height="{:.2}" fill="none" stroke="{INK}"/>"#,
        WIDTH - LEFT - RIGHT,
        HEIGHT - TOP - BOTTOM
    );
    let _ = writeln!(
        svg,
        r#"<text x="{:.2}" y="{:.2}" text-anchor="middle">trials (logarithmic scale)</text>"#,
        LEFT + (WIDTH - LEFT - RIGHT) / 2.0,
        HEIGHT - 18.0
    );
    let _ = writeln!(
        svg,
        r#"<text transform="translate(18 {:.2}) rotate(-90)" text-anchor="middle">running estimate</text>"#,
        TOP + (HEIGHT - TOP - BOTTOM) / 2.0
    );
    let _ = writeln!(
        svg,
        r#"<text x="{:.2}" y="{:.2}" text-anchor="end" font-size="11">shaded: {} confidence band; dashed: exact value</text>"#,
        WIDTH - RIGHT,
        HEIGHT - 4.0,
        escape(&confidence.to_string())
    );
}

/// Confidence bands, exact-value lines, running estimates and legend.
fn curves(svg: &mut String, xs: Scale, ys: Scale, series: &[Series]) {
    #[allow(clippy::cast_precision_loss)] // only used for pixel positions
    let px = |t: u64| xs.map((t as f64).log10());
    // Bands first, so that every line is drawn on top of every band.
    for (i, s) in series.iter().enumerate() {
        let colour = PALETTE[i % PALETTE.len()];
        let pts: Vec<_> = s
            .points
            .iter()
            .filter_map(|c| c.estimate.map(|e| (px(c.trials), e)))
            .collect();
        if pts.len() < 2 {
            continue;
        }
        let mut poly = String::new();
        for (x, e) in &pts {
            let _ = write!(poly, "{x:.2},{:.2} ", ys.map(e.upper));
        }
        for (x, e) in pts.iter().rev() {
            let _ = write!(poly, "{x:.2},{:.2} ", ys.map(e.lower));
        }
        let _ = writeln!(
            svg,
            r#"<polygon points="{}" fill="{colour}" fill-opacity="0.15" stroke="none"/>"#,
            poly.trim_end()
        );
    }
    for (i, s) in series.iter().enumerate() {
        let colour = PALETTE[i % PALETTE.len()];
        if let Some(x) = s.exact {
            let y = ys.map(x);
            let _ = writeln!(
                svg,
                r#"<line x1="{LEFT}" y1="{y:.2}" x2="{:.2}" y2="{y:.2}" stroke="{colour}" stroke-width="1.5" stroke-dasharray="6 4"/>"#,
                WIDTH - RIGHT
            );
        }
        let line: Vec<String> = s
            .points
            .iter()
            .filter_map(|c| c.estimate.map(|e| (px(c.trials), ys.map(e.point))))
            .map(|(x, y)| format!("{x:.2},{y:.2}"))
            .collect();
        let _ = writeln!(
            svg,
            r#"<polyline points="{}" fill="none" stroke="{colour}" stroke-width="2"/>"#,
            line.join(" ")
        );
        // Legend entries in one row below the title.
        #[allow(clippy::cast_precision_loss)] // at most a handful of series
        let lx = LEFT + 170.0 * i as f64;
        let _ = writeln!(
            svg,
            r#"<line x1="{lx:.2}" y1="52" x2="{:.2}" y2="52" stroke="{colour}" stroke-width="3"/><text x="{:.2}" y="56">{}</text>"#,
            lx + 22.0,
            lx + 28.0,
            escape(&s.label)
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Checkpoint, Series};
    use crate::stats::Estimate;

    fn series(exact: Option<f64>, n: usize) -> Series {
        Series {
            label: "a<b".into(),
            exact,
            points: (0..n)
                .map(|i| Checkpoint {
                    trials: 10u64.pow(u32::try_from(i).unwrap() + 1),
                    estimate: (i > 0).then_some(Estimate {
                        point: 0.5,
                        lower: 0.4,
                        upper: 0.6,
                    }),
                })
                .collect(),
        }
    }

    #[test]
    fn well_formed_and_escaped() {
        let svg = convergence_plot(
            "x & y",
            &[series(Some(0.5), 4), series(None, 3)],
            Confidence::P95,
        );
        assert!(svg.starts_with("<svg"));
        assert!(svg.trim_end().ends_with("</svg>"));
        assert!(svg.contains("x &amp; y") && svg.contains("a&lt;b"));
        assert!(!svg.contains("NaN") && !svg.contains("inf"));
        assert_eq!(svg.matches("<polygon").count(), 2);
        assert_eq!(svg.matches("stroke-dasharray").count(), 1);
    }

    #[test]
    fn degenerate_inputs_produce_no_nan() {
        for s in [vec![], vec![series(None, 1)], vec![series(Some(1.0), 2)]] {
            let svg = convergence_plot("t", &s, Confidence::P90);
            assert!(!svg.contains("NaN"), "{svg}");
        }
    }

    #[test]
    fn ticks_are_nice() {
        let t = nice_ticks(0.0, 1.0);
        assert_eq!(t.first(), Some(&0.0));
        assert_eq!(t.last(), Some(&1.0));
        assert!(t.len() >= 5);
    }
}
