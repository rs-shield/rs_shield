//! Argument parsing of `rsb snapshots ...` (the commands themselves are
//! covered by the rsb-sdk `snapshots_tests`; running the binary needs a
//! login).

use clap::Parser;
use rsb_cli::command::main_cmd::Commands;
use rsb_cli::command::snapshot_cmd::SnapshotCommand;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

fn parse(args: &[&str]) -> Result<SnapshotCommand, clap::Error> {
    let mut argv = vec!["rsb", "snapshots"];
    argv.extend_from_slice(args);
    Cli::try_parse_from(argv).map(|c| match c.command {
        Commands::Snapshots(cmd) => cmd,
        _ => panic!("not a snapshots command"),
    })
}

#[test]
fn list_takes_a_profile_or_a_repository_and_json() {
    match parse(&["list", "--config", "p.toml", "--json"]).unwrap() {
        SnapshotCommand::List {
            config,
            backup,
            json,
            key,
        } => {
            assert_eq!(config.unwrap().to_str(), Some("p.toml"));
            assert!(backup.is_none() && key.is_none() && json);
        }
        _ => panic!("expected list"),
    }
    match parse(&["list", "-b", "repo"]).unwrap() {
        SnapshotCommand::List { backup, json, .. } => {
            assert_eq!(backup.unwrap().to_str(), Some("repo"));
            assert!(!json);
        }
        _ => panic!("expected list"),
    }
    // Profile-less: the one profile of the current directory is used.
    assert!(parse(&["list"]).is_ok());
}

#[test]
fn config_and_backup_conflict() {
    for sub in [
        &["list"][..],
        &["show", "2026-05-25T103000.000Z"],
        &["delete", "2026-05-25T103000.000Z"],
        &["diff", "a", "b"],
    ] {
        let mut args = sub.to_vec();
        args.extend(["-c", "p.toml", "-b", "repo"]);
        let err = parse(&args).err().expect("must conflict");
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }
}

#[test]
fn show_delete_and_diff_take_ids() {
    match parse(&["show", "2026-05-25T103000.000Z-1", "-b", "repo", "--json"]).unwrap() {
        SnapshotCommand::Show { id, json, .. } => {
            assert_eq!(id, "2026-05-25T103000.000Z-1");
            assert!(json);
        }
        _ => panic!("expected show"),
    }
    match parse(&["delete", "2026-05-25T103000.000Z"]).unwrap() {
        SnapshotCommand::Delete {
            id,
            dry_run,
            confirm,
            ..
        } => {
            assert_eq!(id, "2026-05-25T103000.000Z");
            // Parses, but execute() refuses without --confirm/--dry-run.
            assert!(!dry_run && !confirm);
        }
        _ => panic!("expected delete"),
    }
    match parse(&["diff", "a", "b", "-c", "p.toml"]).unwrap() {
        SnapshotCommand::Diff { from, to, .. } => {
            assert_eq!((from.as_str(), to.as_str()), ("a", "b"))
        }
        _ => panic!("expected diff"),
    }
    assert!(parse(&["show"]).is_err());
    assert!(parse(&["diff", "a"]).is_err());
    // delete takes no key on the command line (env, keyring or prompt).
    assert!(parse(&["delete", "x", "--key", "k"]).is_err());
}

#[test]
fn delete_takes_confirm_and_dry_run_like_prune() {
    match parse(&["delete", "x", "--confirm"]).unwrap() {
        SnapshotCommand::Delete {
            dry_run, confirm, ..
        } => assert!(confirm && !dry_run),
        _ => panic!("expected delete"),
    }
    for flag in ["--dry-run", "-d"] {
        match parse(&["delete", "x", flag, "-b", "repo"]).unwrap() {
            SnapshotCommand::Delete {
                dry_run, confirm, ..
            } => assert!(dry_run && !confirm),
            _ => panic!("expected delete"),
        }
    }
    use rsb_cli::command::snapshot_cmd::check_delete_confirmation;
    assert!(check_delete_confirmation(false, false).is_err());
    assert!(check_delete_confirmation(true, false).is_ok());
    assert!(check_delete_confirmation(false, true).is_ok());
}
