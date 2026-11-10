//! Command-line definitions and dispatch.
//!
//! clap reports malformed input (unknown flags, non-numeric values, values
//! outside a fixed set) and exits with status 2. Values that parse but are
//! outside an experiment's domain, such as `--doors 2`, are rejected by the
//! library constructors and reported as errors with status 1.

use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};

use aporia::engine::{self, Experiment, RunConfig};
use aporia::experiments::{self, Bertrand, Birthday, Braess, Host, MontyHall, Parrondo, Penney};
use aporia::rng::DEFAULT_SEED;
use aporia::{Confidence, Error, Result, Sequence, Trials, render, svg};

/// Number of log-spaced checkpoints in a convergence plot.
const PLOT_POINTS: u32 = 60;

/// A Monte-Carlo laboratory for results where intuition fails.
#[derive(Debug, Parser)]
#[command(name = "aporia", version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List the experiments.
    List,
    /// Run every experiment with its default parameters, then print a summary.
    All {
        #[command(flatten)]
        common: Common,
    },
    /// Monty Hall problem with n doors, k of them opened by the host.
    MontyHall {
        /// Number of doors (3 to 1000).
        #[arg(long, default_value_t = 3)]
        doors: u32,
        /// Number of doors the host opens (1 to doors - 2).
        #[arg(long, default_value_t = 1)]
        opened: u32,
        /// Whether the host knows where the car is.
        #[arg(long, value_enum, default_value_t = HostArg::Knowing)]
        host: HostArg,
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        plot: Plot,
    },
    /// Birthday problem: probability of a shared birthday.
    Birthday {
        /// Number of people.
        #[arg(long, default_value_t = 23)]
        people: u64,
        /// Number of equally likely days.
        #[arg(long, default_value_t = 365)]
        days: u64,
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        plot: Plot,
    },
    /// Bertrand's chord paradox: three ways to draw a random chord.
    Bertrand {
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        plot: Plot,
    },
    /// Penney's game: which coin pattern appears first?
    Penney {
        /// Pattern of the first player, e.g. HHT; also prints the best response.
        #[arg(long)]
        first: Option<String>,
        /// Pattern of the second player, same length as the first.
        #[arg(long, default_value = "THH")]
        second: String,
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        plot: Plot,
    },
    /// Parrondo's paradox: two losing games that win when mixed.
    Parrondo {
        /// Rounds per game.
        #[arg(long, default_value_t = 1000)]
        rounds: u32,
        /// Bias against the player, in [0, 0.1).
        #[arg(long, default_value_t = 0.005)]
        epsilon: f64,
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        plot: Plot,
    },
    /// Braess's paradox: a free road that slows everybody down.
    Braess {
        /// Number of drivers (at least 2).
        #[arg(long, default_value_t = 4000)]
        drivers: u32,
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        plot: Plot,
    },
}

#[derive(Debug, Args)]
struct Common {
    /// Trials per quantity [default: depends on the experiment].
    #[arg(long)]
    trials: Option<u64>,
    /// Master seed, decimal or hexadecimal with 0x prefix.
    #[arg(long, default_value_t = DEFAULT_SEED, value_parser = parse_seed)]
    seed: u64,
    /// Confidence level of the intervals, in percent.
    #[arg(long, value_enum, default_value_t = Level::P95)]
    confidence: Level,
}

#[derive(Debug, Args)]
struct Plot {
    /// Write a convergence plot to this SVG file.
    #[arg(long, value_name = "PATH")]
    svg: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Level {
    #[value(name = "90")]
    P90,
    #[value(name = "95")]
    P95,
    #[value(name = "99")]
    P99,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum HostArg {
    Knowing,
    Ignorant,
}

fn parse_seed(s: &str) -> std::result::Result<u64, String> {
    let parsed = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => s.parse(),
    };
    parsed.map_err(|e| e.to_string())
}

impl Common {
    fn config(&self, experiment: &dyn Experiment) -> Result<RunConfig> {
        Ok(RunConfig {
            trials: Trials::new(self.trials.unwrap_or(experiment.default_trials()))?,
            seed: self.seed,
            confidence: match self.confidence {
                Level::P90 => Confidence::P90,
                Level::P95 => Confidence::P95,
                Level::P99 => Confidence::P99,
            },
        })
    }
}

/// Executes the parsed command, writing the report to `out`.
pub fn execute(cli: Cli, out: &mut impl Write) -> Result<()> {
    let (experiment, common, plot): (Box<dyn Experiment>, Common, Plot) = match cli.command {
        Command::List => return list(out),
        Command::All { common } => return all(&common, out),
        Command::MontyHall {
            doors,
            opened,
            host,
            common,
            plot,
        } => {
            let host = match host {
                HostArg::Knowing => Host::Knowing,
                HostArg::Ignorant => Host::Ignorant,
            };
            (Box::new(MontyHall::new(doors, opened, host)?), common, plot)
        }
        Command::Birthday {
            people,
            days,
            common,
            plot,
        } => (Box::new(Birthday::new(people, days)?), common, plot),
        Command::Bertrand { common, plot } => (Box::new(Bertrand), common, plot),
        Command::Penney {
            first,
            second,
            common,
            plot,
        } => {
            let second: Sequence = second.parse()?;
            let game = match first {
                Some(first) => Penney::new(first.parse()?, second)?.with_best_response(),
                None => Penney::new("HHT".parse()?, second)?,
            };
            (Box::new(game), common, plot)
        }
        Command::Parrondo {
            rounds,
            epsilon,
            common,
            plot,
        } => (Box::new(Parrondo::new(rounds, epsilon)?), common, plot),
        Command::Braess {
            drivers,
            common,
            plot,
        } => (Box::new(Braess::new(drivers)?), common, plot),
    };
    single(experiment.as_ref(), &common, plot.svg.as_deref(), out)
}

fn list(out: &mut impl Write) -> Result<()> {
    let text = experiments::all()
        .iter()
        .fold(String::new(), |mut text, e| {
            let _ = writeln!(text, "{:<12}{}", e.id(), e.title());
            text
        });
    emit(out, &text)
}

fn all(common: &Common, out: &mut impl Write) -> Result<()> {
    let mut reports = Vec::new();
    let mut text = String::new();
    for experiment in experiments::all() {
        let report = engine::run(experiment.as_ref(), &common.config(experiment.as_ref())?)?;
        text.push_str(&render::report(&report));
        text.push('\n');
        reports.push(report);
    }
    // All experiments share the confidence level, so the first config is representative.
    let confidence = reports
        .first()
        .map_or(Confidence::default(), |r| r.config.confidence);
    text.push_str(&render::summary(&reports, confidence));
    emit(out, &text)
}

fn single(
    experiment: &dyn Experiment,
    common: &Common,
    svg_path: Option<&Path>,
    out: &mut impl Write,
) -> Result<()> {
    let config = common.config(experiment)?;
    let Some(path) = svg_path else {
        let report = engine::run(experiment, &config)?;
        return emit(out, &render::report(&report));
    };
    if !experiment.supports_plot() {
        return Err(Error::Unsupported(format!(
            "{} does not support --svg: its quantities are equilibrium outcomes, not running averages \
             worth plotting",
            experiment.id()
        )));
    }
    let checkpoints = engine::log_checkpoints(config.trials.get(), PLOT_POINTS);
    let (report, series) = engine::run_with_checkpoints(experiment, &config, &checkpoints)?;
    let params: Vec<String> = report
        .parameters
        .iter()
        .map(|(k, v)| format!("{k} = {v}"))
        .collect();
    let title = if params.is_empty() {
        report.title.to_owned()
    } else {
        format!("{} ({})", report.title, params.join(", "))
    };
    let figure = svg::convergence_plot(&title, &series, config.confidence);
    std::fs::write(path, figure).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    emit(out, &render::report(&report))
}

fn emit(out: &mut impl Write, text: &str) -> Result<()> {
    out.write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|source| Error::Io {
            path: PathBuf::from("<stdout>"),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn seeds_parse_in_decimal_and_hex() {
        assert_eq!(parse_seed("42"), Ok(42));
        assert_eq!(parse_seed("0x41504F524941"), Ok(DEFAULT_SEED));
        assert!(parse_seed("0xZZ").is_err());
        assert!(parse_seed("-1").is_err());
    }

    #[test]
    fn cli_definition_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn broken_writer_is_an_io_error() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("closed"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert!(matches!(emit(&mut Broken, "x"), Err(Error::Io { .. })));
    }
}
