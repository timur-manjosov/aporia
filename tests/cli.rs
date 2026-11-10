//! Integration tests through the compiled binary.

use std::path::PathBuf;
use std::process::{Command, Output};

fn aporia(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aporia"))
        .args(args)
        .output()
        .expect("binary runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("UTF-8 output")
}

/// A fresh path inside Cargo's target directory for this test.
fn temp_file(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    path
}

#[test]
fn same_arguments_give_identical_output() {
    let args = ["birthday", "--trials", "5000"];
    let (a, b) = (aporia(&args), aporia(&args));
    assert!(a.status.success());
    assert_eq!(a.stdout, b.stdout);
}

#[test]
fn different_seed_changes_output() {
    let a = aporia(&["bertrand", "--trials", "5000", "--seed", "1"]);
    let b = aporia(&["bertrand", "--trials", "5000", "--seed", "2"]);
    assert_ne!(a.stdout, b.stdout);
}

#[test]
fn hexadecimal_seed_equals_decimal() {
    let a = aporia(&["penney", "--trials", "2000", "--seed", "0x10"]);
    let b = aporia(&["penney", "--trials", "2000", "--seed", "16"]);
    assert_eq!(a.stdout, b.stdout);
}

#[test]
fn list_names_all_experiments() {
    let out = aporia(&["list"]);
    assert!(out.status.success());
    let text = stdout(&out);
    for id in [
        "monty-hall",
        "birthday",
        "bertrand",
        "penney",
        "parrondo",
        "braess",
    ] {
        assert!(text.contains(id), "{id} missing");
    }
}

#[test]
fn all_smoke_test() {
    let out = aporia(&["all", "--trials", "2000"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = stdout(&out);
    assert!(text.contains("Summary"));
    assert!(text.contains("2000 trials per quantity"));
    assert!(text.contains("misses by chance"));
    assert!(!text.contains("NaN"));
}

#[test]
fn usage_errors_exit_with_2() {
    for args in [
        &["no-such-experiment"][..],
        &["birthday", "--people", "many"],
        &["bertrand", "--confidence", "80"],
        &["monty-hall", "--host", "lazy"],
        &["all", "--svg", "x.svg"],
    ] {
        let out = aporia(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
}

#[test]
fn domain_errors_exit_with_1_and_one_line() {
    for args in [
        &["monty-hall", "--doors", "2"][..],
        &["monty-hall", "--doors", "5", "--opened", "4"],
        &["birthday", "--days", "0"],
        &["penney", "--first", "HHT", "--second", "HT"],
        &["penney", "--first", "HHX"],
        &["parrondo", "--epsilon", "0.2"],
        &["braess", "--drivers", "1"],
        &["bertrand", "--trials", "0"],
    ] {
        let out = aporia(args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(err.lines().count(), 1, "{args:?}: {err}");
        assert!(err.starts_with("error: "));
        assert!(!err.contains("panicked"));
    }
}

#[test]
fn svg_is_written_and_clean() {
    let path = temp_file("monty.svg");
    let out = aporia(&[
        "monty-hall",
        "--host",
        "ignorant",
        "--trials",
        "3000",
        "--svg",
        path.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    let svg = std::fs::read_to_string(&path).unwrap();
    assert!(svg.starts_with("<svg"));
    assert!(!svg.contains("NaN"));
    assert!(svg.contains("P(win | switch)"));
}

#[test]
fn braess_svg_fails_cleanly() {
    let path = temp_file("braess.svg");
    let out = aporia(&["braess", "--trials", "2", "--svg", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--svg"));
    assert!(!path.exists());
}

#[test]
fn unwritable_svg_path_is_an_io_error() {
    let path = temp_file("missing-dir/plot.svg");
    let out = aporia(&[
        "bertrand",
        "--trials",
        "100",
        "--svg",
        path.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("error: cannot write"));
}

#[test]
fn penney_first_prints_best_response() {
    let out = aporia(&["penney", "--first", "HHH", "--trials", "1000"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Best response to HHH: THH"));
    let default = aporia(&["penney", "--trials", "1000"]);
    assert!(!stdout(&default).contains("Best response"));
}

#[test]
fn ignorant_host_reports_rejections() {
    let out = aporia(&["monty-hall", "--host", "ignorant", "--trials", "3000"]);
    let text = stdout(&out);
    // About two thirds of the 3000 games are accepted.
    let line = text.lines().find(|l| l.contains("P(win | stay)")).unwrap();
    let ratio = line
        .split_whitespace()
        .find(|w| w.ends_with("/3000"))
        .unwrap();
    let accepted: u32 = ratio.trim_end_matches("/3000").parse().unwrap();
    assert!((1800..2200).contains(&accepted), "{accepted}");
}
