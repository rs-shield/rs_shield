//! Argument parsing of `rsb restore --files/--date` (the restore itself is
//! covered by the rsb-sdk tests; running the binary needs a login).

use clap::Parser;
use rsb_cli::command::main_cmd::Commands;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

fn parse(args: &[&str]) -> Result<Commands, clap::Error> {
    let mut argv = vec!["rsb", "restore"];
    argv.extend_from_slice(args);
    Cli::try_parse_from(argv).map(|c| c.command)
}

fn restore_fields(cmd: Commands) -> (Vec<String>, Option<String>, Option<String>) {
    match cmd {
        Commands::Restore {
            files,
            date,
            snapshot,
            ..
        } => (files, date, snapshot),
        _ => panic!("not a restore command"),
    }
}

#[test]
fn files_is_repeatable_and_takes_several_values() {
    let (files, date, _) = restore_fields(
        parse(&[
            "cfg.toml", "-t", "out", "--files", "docs/", "**/*.txt", "-i", "a b.txt",
        ])
        .unwrap(),
    );
    assert_eq!(files, ["docs/", "**/*.txt", "a b.txt"]);
    assert!(date.is_none());

    let (files, _, _) = restore_fields(parse(&["cfg.toml", "-t", "out"]).unwrap());
    assert!(files.is_empty());
}

#[test]
fn date_is_accepted_and_conflicts_with_snapshot() {
    let (_, date, snapshot) =
        restore_fields(parse(&["-b", "repo", "-t", "out", "--date", "2026-10-01 12:00"]).unwrap());
    assert_eq!(date.as_deref(), Some("2026-10-01 12:00"));
    assert!(snapshot.is_none());

    let err = parse(&[
        "-b",
        "repo",
        "-t",
        "out",
        "--date",
        "2026-10-01",
        "--snapshot",
        "2026-10-01T080000Z",
    ])
    .err()
    .expect("--date and --snapshot must conflict");
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

/// `--files` arrives byte for byte (no normalization by the parser), and the
/// selector built from it matches the other Unicode normalization form:
/// typed NFC selects a macOS (NFD) manifest path and vice versa.
#[test]
fn files_match_across_unicode_normalization_forms() {
    const NFC: &str = "fotos/ver\u{e3}o/caf\u{e9}.txt";
    const NFD: &str = "fotos/vera\u{303}o/cafe\u{301}.txt";
    for (typed, stored) in [(NFC, NFD), (NFD, NFC)] {
        let (files, _, _) =
            restore_fields(parse(&["cfg.toml", "-t", "out", "--files", typed]).unwrap());
        assert_eq!(files, [typed]);
        let selector = rsb_sdk::core::FileSelector::new(&files).unwrap();
        assert!(selector.matches(stored), "{typed:?} must select {stored:?}");
        let picked = selector
            .select(vec![(std::path::PathBuf::from(stored), ())], "snap")
            .unwrap();
        assert_eq!(
            picked[0].0,
            std::path::PathBuf::from(stored),
            "stored name kept"
        );
    }
}
