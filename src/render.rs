//! Plain-text rendering of reports.
//!
//! All numbers use a fixed number of decimals, so that output is stable and
//! columns line up: probabilities six, real-valued quantities four.

use std::fmt::Write as _;

use crate::engine::{Kind, QuantityReport, Report};
use crate::stats::Confidence;

use Align::{Left, Right};

const WIDTH: usize = 78;
const INDENT: &str = "  ";
const KEY_WIDTH: usize = 12;

/// Column alignment.
#[derive(Clone, Copy)]
enum Align {
    Left,
    Right,
}

/// Formats `x` with the number of decimals used for quantities of `kind`.
///
/// ```
/// use aporia::engine::Kind;
/// assert_eq!(aporia::render::number(1.0 / 3.0, Kind::Binary), "0.333333");
/// assert_eq!(aporia::render::number(-9.21811, Kind::Real), "-9.2181");
/// ```
#[must_use]
pub fn number(x: f64, kind: Kind) -> String {
    match kind {
        Kind::Binary => format!("{x:.6}"),
        Kind::Real => format!("{x:.4}"),
    }
}

/// The full text block for one experiment.
#[must_use]
pub fn report(r: &Report) -> String {
    let mut out = String::new();
    let heading = format!("{} ({})", r.title, r.id);
    let _ = writeln!(out, "{heading}\n{}\n", "=".repeat(heading.chars().count()));
    keyed(&mut out, "Intuition", r.intuition);
    if !r.parameters.is_empty() {
        let params: Vec<String> = r
            .parameters
            .iter()
            .map(|(k, v)| format!("{k} = {v}"))
            .collect();
        keyed(&mut out, "Parameters", &params.join(", "));
    }
    keyed(
        &mut out,
        "Run",
        &format!(
            "{} trials per quantity, seed {}, {} intervals",
            r.config.trials.get(),
            r.config.seed,
            r.config.confidence
        ),
    );
    out.push('\n');

    let ci = format!("{} CI", r.config.confidence);
    let mut rows = vec![row(&[
        "quantity",
        "exact",
        "estimate",
        &ci,
        "accepted/attempted",
        "inside",
    ])];
    rows.extend(r.quantities.iter().map(|q| {
        vec![
            q.label.clone(),
            exact(q),
            number(q.estimate.point, q.kind),
            interval(q),
            format!("{}/{}", q.accepted, q.attempted),
            inside(q).to_owned(),
        ]
    }));
    table(&mut out, &rows, &[Left, Right, Right, Right, Right, Left]);
    out.push('\n');
    for note in &r.notes {
        keyed(&mut out, "Exact", note);
    }
    keyed(&mut out, "Lesson", &r.lesson);
    out
}

/// One summary table over several experiments, with a note on chance misses.
#[must_use]
pub fn summary(reports: &[Report], confidence: Confidence) -> String {
    let mut out = String::from("Summary\n=======\n\n");
    let ci = format!("{confidence} CI");
    let mut rows = vec![row(&[
        "experiment",
        "quantity",
        "exact",
        "estimate",
        &ci,
        "inside",
    ])];
    let (mut checked, mut hits) = (0, 0);
    for r in reports {
        for q in &r.quantities {
            if let Some(i) = q.inside() {
                checked += 1;
                hits += usize::from(i);
            }
            rows.push(vec![
                r.id.to_owned(),
                q.label.clone(),
                exact(q),
                number(q.estimate.point, q.kind),
                interval(q),
                inside(q).to_owned(),
            ]);
        }
    }
    table(&mut out, &rows, &[Left, Left, Right, Right, Right, Left]);
    let one_in = match confidence {
        Confidence::P90 => "ten",
        Confidence::P95 => "twenty",
        Confidence::P99 => "a hundred",
    };
    let _ = writeln!(
        out,
        "\n{INDENT}{hits} of {checked} exact values lie inside their {confidence} intervals."
    );
    wrap(
        &mut out,
        INDENT,
        INDENT,
        &format!(
            "With {confidence} intervals roughly one comparison in {one_in} misses by chance, \
             so an occasional \"no\" is expected and is not a bug."
        ),
    );
    out
}

fn row(cells: &[&str]) -> Vec<String> {
    cells.iter().map(|&c| c.to_owned()).collect()
}

fn exact(q: &QuantityReport) -> String {
    q.exact
        .map_or_else(|| "-".to_owned(), |x| number(x, q.kind))
}

fn interval(q: &QuantityReport) -> String {
    format!(
        "[{}, {}]",
        number(q.estimate.lower, q.kind),
        number(q.estimate.upper, q.kind)
    )
}

fn inside(q: &QuantityReport) -> &'static str {
    match q.inside() {
        Some(true) => "yes",
        Some(false) => "no",
        None => "-",
    }
}

fn table(out: &mut String, rows: &[Vec<String>], align: &[Align]) {
    let cols = align.len();
    let widths: Vec<usize> = (0..cols)
        .map(|c| {
            rows.iter()
                .filter_map(|r| r.get(c))
                .map(|s| s.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    for r in rows {
        let mut line = String::from(INDENT);
        for ((cell, &w), a) in r.iter().zip(&widths).zip(align) {
            let _ = match a {
                Align::Left => write!(line, "{cell:<w$}  "),
                Align::Right => write!(line, "{cell:>w$}  "),
            };
        }
        let _ = writeln!(out, "{}", line.trim_end());
    }
}

fn keyed(out: &mut String, key: &str, text: &str) {
    let first = format!("{INDENT}{key:<KEY_WIDTH$}");
    let rest = " ".repeat(first.chars().count());
    wrap(out, &first, &rest, text);
}

/// Greedy word wrap at [`WIDTH`] columns with separate first-line and hanging indents.
fn wrap(out: &mut String, first: &str, rest: &str, text: &str) {
    let mut line = first.to_owned();
    let mut empty = true;
    for word in text.split_whitespace() {
        let fits = line.chars().count() + 1 + word.chars().count() <= WIDTH;
        if !empty && !fits {
            let _ = writeln!(out, "{line}");
            rest.clone_into(&mut line);
            empty = true;
        }
        if !empty {
            line.push(' ');
        }
        line.push_str(word);
        empty = false;
    }
    let _ = writeln!(out, "{}", line.trim_end());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_long_text_with_hanging_indent() {
        let mut out = String::new();
        keyed(&mut out, "Lesson", &"word ".repeat(40));
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| l.chars().count() <= WIDTH));
        assert!(lines[0].starts_with("  Lesson"));
        assert!(lines[1].starts_with(&" ".repeat(14)));
    }

    #[test]
    fn table_aligns_columns() {
        let mut out = String::new();
        let rows = vec![row(&["a", "1.0"]), row(&["long", "10.0"])];
        table(&mut out, &rows, &[Left, Right]);
        assert_eq!(out, "  a      1.0\n  long  10.0\n");
    }
}
