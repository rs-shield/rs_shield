//! `--backup <DIR>` on `backup`, `prune`, `schedule` and `watch` names the
//! repository directory (as for restore/verify/snapshots), never the profile
//! file. Running the commands needs a login, so this covers argument parsing,
//! profile resolution and the generated schedule.

use clap::Parser;
use rsb_cli::command::main_cmd::Commands;
use rsb_cli::helpers::{
    find_profile_for_repository, profile_uses_relative_paths, render_schedule,
    resolve_command_profile,
};
use rsb_sdk::config::Config;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

fn parse(args: &[&str]) -> Result<Commands, clap::Error> {
    let mut argv = vec!["rsb"];
    argv.extend_from_slice(args);
    Cli::try_parse_from(argv).map(|c| c.command)
}

fn config_and_backup(cmd: Commands) -> (Option<PathBuf>, Option<PathBuf>) {
    match cmd {
        Commands::Backup { config, backup, .. }
        | Commands::Prune { config, backup, .. }
        | Commands::Schedule { config, backup, .. }
        | Commands::Watch { config, backup, .. } => (config, backup),
        _ => panic!("unexpected command"),
    }
}

/// A local repository (snapshots/ + data/) and its working directory.
struct Fixture {
    _tmp: tempfile::TempDir,
    cwd: PathBuf,
    repo: PathBuf,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().join("work");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&cwd).unwrap();
    fs::create_dir_all(repo.join("snapshots")).unwrap();
    fs::create_dir_all(repo.join("data")).unwrap();
    Fixture {
        _tmp: tmp,
        cwd,
        repo,
    }
}

fn write_profile(dir: &Path, name: &str, dest: &Path, extra: &str) -> PathBuf {
    let path = dir.join(format!("{}.toml", name));
    // toml basic strings: escape Windows backslashes.
    let dest = dest.to_string_lossy().replace('\\', "\\\\");
    fs::write(
        &path,
        format!(
            "source_path = \"src\"\ndestination_path = \"{}\"\n{}",
            dest, extra
        ),
    )
    .unwrap();
    path
}

#[test]
fn backup_flag_is_a_directory_and_conflicts_with_config() {
    for cmd in ["backup", "prune", "schedule"] {
        let (config, backup) = config_and_backup(parse(&[cmd, "--backup", "/mnt/repo"]).unwrap());
        assert_eq!(config, None, "{}", cmd);
        assert_eq!(backup, Some(PathBuf::from("/mnt/repo")), "{}", cmd);
        assert!(
            parse(&[cmd, "p.toml", "-b", "/mnt/repo"]).is_err(),
            "{}: CONFIG and --backup together must be rejected",
            cmd
        );
    }
    let (_, backup) =
        config_and_backup(parse(&["watch", "-b", "/mnt/repo", "--sync-to", "/tmp/s"]).unwrap());
    assert_eq!(backup, Some(PathBuf::from("/mnt/repo")));
    assert!(parse(&["watch", "p.toml", "-b", "/mnt/repo", "-s", "/tmp/s"]).is_err());
}

#[test]
fn repository_resolves_to_the_profile_that_backs_up_to_it() {
    let f = fixture();
    let profile = write_profile(&f.cwd, "docs", &f.repo, "");
    write_profile(&f.cwd, "other", &f.cwd.join("elsewhere"), "");
    fs::create_dir_all(f.cwd.join("elsewhere")).unwrap();

    let found = find_profile_for_repository("backup", &f.repo, &f.cwd).unwrap();
    assert_eq!(
        found.canonicalize().unwrap(),
        profile.canonicalize().unwrap()
    );
}

#[test]
fn relative_portable_destination_is_resolved_against_the_working_dir() {
    let f = fixture();
    // As written by `create-profile --portable` run from `work/`.
    let profile = write_profile(&f.cwd, "portable", Path::new("../repo"), "");
    let found = find_profile_for_repository("watch", &f.repo, &f.cwd).unwrap();
    assert_eq!(
        found.canonicalize().unwrap(),
        profile.canonicalize().unwrap()
    );
}

#[test]
fn profile_stored_inside_the_repository_is_found() {
    let f = fixture();
    let profile = write_profile(&f.repo, "inside", &f.repo, "");
    let found = find_profile_for_repository("backup", &f.repo, &f.cwd).unwrap();
    assert_eq!(
        found.canonicalize().unwrap(),
        profile.canonicalize().unwrap()
    );
}

#[test]
fn no_matching_profile_is_an_error_with_guidance() {
    let f = fixture();
    // Only an S3 profile whose local fallback is the repository: not a match.
    write_profile(&f.cwd, "cloud", &f.repo, "s3_bucket = \"my-backups\"\n");
    let err = find_profile_for_repository("backup", &f.repo, &f.cwd)
        .unwrap_err()
        .to_string();
    assert!(err.contains("No profile found for repository"), "{}", err);
    assert!(err.contains("rsb backup <PROFILE.toml>"), "{}", err);
    assert!(err.contains("rsb create-profile"), "{}", err);
}

#[test]
fn several_matching_profiles_are_an_error() {
    let f = fixture();
    write_profile(&f.cwd, "a", &f.repo, "");
    write_profile(&f.cwd, "b", &f.repo, "");
    let err = find_profile_for_repository("backup", &f.repo, &f.cwd)
        .unwrap_err()
        .to_string();
    assert!(err.contains("Several profiles"), "{}", err);
    assert!(err.contains("a.toml") && err.contains("b.toml"), "{}", err);
}

#[test]
fn a_profile_file_passed_as_backup_is_refused() {
    let f = fixture();
    let profile = write_profile(&f.cwd, "docs", &f.repo, "");
    let err = find_profile_for_repository("schedule", &profile, &f.cwd)
        .unwrap_err()
        .to_string();
    assert!(err.contains("expects the repository directory"), "{}", err);
    assert!(err.contains("rsb schedule"), "{}", err);

    let err = find_profile_for_repository("backup", &f.cwd.join("missing"), &f.cwd)
        .unwrap_err()
        .to_string();
    assert!(err.contains("not found"), "{}", err);
}

#[test]
fn positional_config_wins_and_is_used_as_is() {
    let p = PathBuf::from("explicit.toml");
    assert_eq!(
        resolve_command_profile("backup", Some(p.clone()), None).unwrap(),
        p
    );
}

#[test]
fn schedule_from_repository_runs_backup_with_the_profile() {
    let f = fixture();
    let profile = write_profile(&f.cwd, "docs", &f.repo, "");
    let found = find_profile_for_repository("schedule", &f.repo, &f.cwd)
        .unwrap()
        .canonicalize()
        .unwrap();
    let cfg: Config = toml::from_str(&fs::read_to_string(&profile).unwrap()).unwrap();
    let exe = Path::new("/usr/local/bin/rsb");
    let env = Path::new("/home/u/.rs-shield/rsb-backup.env");

    let cron = render_schedule("cron", exe, &found, &cfg, env, None).unwrap();
    let line = cron
        .lines()
        .find(|l| l.starts_with("0 3 * * *"))
        .expect("cron line");
    let expected = format!(
        "\"{}\" backup \"{}\" --storage local",
        exe.display(),
        found.display()
    );
    assert!(line.ends_with(&expected), "{}", line);
    // The repository directory itself is never what `backup` is given.
    assert!(
        !line.contains(&format!(
            "backup \"{}\"",
            f.repo.canonicalize().unwrap().display()
        )),
        "{}",
        line
    );
    assert!(!line.contains("--key"), "{}", line);

    let unit = render_schedule("systemd", exe, &found, &cfg, env, None).unwrap();
    assert!(
        unit.lines().any(|l| l == format!("ExecStart={}", expected)),
        "{}",
        unit
    );
    assert!(render_schedule("launchd", exe, &found, &cfg, env, None).is_err());
}

#[test]
fn schedule_uses_s3_storage_for_s3_profiles() {
    let cfg = Config {
        s3_bucket: Some("my-backups".into()),
        ..Config::new("src", "s3://my-backups")
    };
    let cron = render_schedule(
        "cron",
        Path::new("rsb"),
        Path::new("/p/cloud.toml"),
        &cfg,
        Path::new("/e"),
        None,
    )
    .unwrap();
    assert!(
        cron.contains("backup \"/p/cloud.toml\" --storage s3"),
        "{}",
        cron
    );
}

#[test]
fn schedule_for_a_repository_without_profile_errors() {
    let f = fixture();
    assert!(resolve_command_profile("schedule", None, Some(f.repo.clone())).is_err());
}

#[test]
fn schedule_of_a_portable_profile_runs_from_its_working_dir() {
    let f = fixture();
    let profile = write_profile(&f.cwd, "portable", Path::new("../repo"), "");
    let cfg: Config = toml::from_str(&fs::read_to_string(&profile).unwrap()).unwrap();
    assert!(profile_uses_relative_paths(&cfg));
    // Absolute on every platform ("/src" has no drive letter on Windows).
    let abs_src = f.cwd.join("src");
    let abs_src = abs_src.to_str().unwrap();
    let abs_repo = f.repo.to_str().unwrap();
    assert!(!profile_uses_relative_paths(&Config::new(
        abs_src, abs_repo
    )));
    assert!(!profile_uses_relative_paths(&Config::new(
        abs_src,
        "s3://bucket"
    )));

    let exe = Path::new("rsb");
    let env = Path::new("/e");
    let cron = render_schedule("cron", exe, &profile, &cfg, env, Some(&f.cwd)).unwrap();
    let line = cron.lines().find(|l| l.starts_with("0 3")).unwrap();
    assert!(
        line.starts_with(&format!("0 3 * * * cd \"{}\" && ", f.cwd.display())),
        "{}",
        line
    );
    let unit = render_schedule("systemd", exe, &profile, &cfg, env, Some(&f.cwd)).unwrap();
    assert!(
        unit.lines()
            .any(|l| l == format!("WorkingDirectory={}", f.cwd.display())),
        "{}",
        unit
    );
}
