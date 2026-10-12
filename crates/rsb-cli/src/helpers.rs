use anyhow::{Result, anyhow};
use rsb_sdk::config::{self, Config};
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

/// Nomes de ficheiros .toml que nunca são perfis de backup.
const NON_PROFILE_TOML: &[&str] = &[
    "Cargo.toml",
    "rustfmt.toml",
    ".rustfmt.toml",
    "clippy.toml",
    ".clippy.toml",
    "rust-toolchain.toml",
    "deny.toml",
    "Dioxus.toml",
    "pyproject.toml",
];

/// Um .toml é candidato a perfil se não estiver na lista de exclusão e
/// tiver `source_path` + `destination_path` no topo.
fn is_profile_candidate(path: &Path) -> bool {
    if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("toml") {
        return false;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if NON_PROFILE_TOML
        .iter()
        .any(|n| n.eq_ignore_ascii_case(name))
    {
        return false;
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|c| c.parse::<toml::Table>().ok())
        .map(|t| t.contains_key("source_path") && t.contains_key("destination_path"))
        .unwrap_or(false)
}

/// Procura perfis .toml no diretório indicado (ordenados por nome).
pub fn find_profile_candidates(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_profile_candidate(p))
        .collect();
    found.sort();
    Ok(found)
}

/// Resolve o ficheiro de perfil: argumento explícito ou, em último caso,
/// auto-descoberta no diretório atual — só se existir exatamente UM perfil
/// válido (nunca Cargo.toml nem outros .toml de tooling).
pub fn resolve_profile_path(config: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(c) = config {
        return Ok(c);
    }
    let current = std::env::current_dir()?;
    let mut candidates = find_profile_candidates(&current)?;
    match candidates.len() {
        0 => Err(anyhow!(
            "No backup profile found in {}. Pass the profile .toml as the first argument \
             (create one with `rsb create-profile`)",
            current.display()
        )),
        1 => {
            let path = candidates.remove(0);
            eprintln!("ℹ️  Using profile auto-discovered: {}", path.display());
            Ok(path)
        }
        _ => Err(anyhow!(
            "Multiple backup profiles found in {}; specify one explicitly:\n{}",
            current.display(),
            candidates
                .iter()
                .map(|p| format!("  • {}", p.display()))
                .collect::<Vec<_>>()
                .join("\n")
        )),
    }
}

/// Profile for the profile-based commands (`backup`, `schedule`, `watch`):
/// the positional CONFIG, else the profile of the repository given with
/// `--backup <DIR>` (see [`find_profile_for_repository`]), else
/// auto-discovery in the current directory. `--backup` is never read as a
/// profile file.
pub fn resolve_command_profile(
    command: &str,
    config: Option<PathBuf>,
    repository: Option<PathBuf>,
) -> Result<PathBuf> {
    match (config, repository) {
        (Some(c), _) => Ok(c),
        (None, Some(repo)) => {
            let cwd = std::env::current_dir()?;
            let path = find_profile_for_repository(command, &repo, &cwd)?;
            eprintln!(
                "ℹ️  Using profile {} (its destination is {})",
                path.display(),
                repo.display()
            );
            Ok(path)
        }
        (None, None) => resolve_profile_path(None),
    }
}

/// Checks that `--backup <DIR>` names an existing repository directory and
/// returns it canonicalized. A file (e.g. a profile `.toml`, which `--backup`
/// used to accept) is refused with a hint to pass it as CONFIG instead.
pub fn check_repository_dir(command: &str, repo: &Path) -> Result<PathBuf> {
    if repo.is_file() {
        return Err(anyhow!(
            "--backup expects the repository directory, but {} is a file. \
             To use a profile, pass it as the first argument: rsb {} {}",
            repo.display(),
            command,
            repo.display()
        ));
    }
    if !repo.is_dir() {
        return Err(anyhow!(
            "Repository directory not found: {}",
            repo.display()
        ));
    }
    Ok(repo.canonicalize()?)
}

/// Finds the profile whose backups go to the local repository `repo`.
///
/// Profiles never live inside the repository (`create-profile`, also with
/// `--portable`, writes `<name>.toml` to the current directory and keeps no
/// copy in the repository), so the candidates are the profile `.toml` files
/// in `repo` itself and in `cwd`. A candidate matches when it has no S3
/// bucket and its `destination_path`, resolved the way a backup resolves it
/// (`~`/`$VAR` expanded, relative paths against `cwd`), is `repo`. Exactly
/// one match is required: none or several is an error explaining what to
/// pass instead, so `--backup` can never pick a profile that backs up
/// somewhere else.
pub fn find_profile_for_repository(command: &str, repo: &Path, cwd: &Path) -> Result<PathBuf> {
    let repo_canon = check_repository_dir(command, repo)?;
    let mut dirs = vec![repo_canon.clone()];
    if let Ok(cwd_canon) = cwd.canonicalize()
        && cwd_canon != repo_canon
    {
        dirs.push(cwd_canon);
    }
    let mut matches = Vec::new();
    for dir in &dirs {
        for profile in find_profile_candidates(dir).unwrap_or_default() {
            if profile_targets_repository(&profile, &repo_canon, cwd) {
                matches.push(profile);
            }
        }
    }
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(anyhow!(
            "No profile found for repository {repo}.\n\
             `rsb {cmd}` needs a profile (source folder, exclusions, encryption), and no profile \
             in {repo} or in the current directory ({cwd}) backs up to {repo}.\n  \
             • pass the profile as the first argument: rsb {cmd} <PROFILE.toml>\n  \
             • or create one: rsb create-profile --name <NAME> --source <SOURCE> --dest {repo}",
            repo = repo.display(),
            cmd = command,
            cwd = cwd.display()
        )),
        _ => Err(anyhow!(
            "Several profiles back up to {}; pass one as the first argument (rsb {} <PROFILE.toml>):\n{}",
            repo.display(),
            command,
            matches
                .iter()
                .map(|p| format!("  • {}", p.display()))
                .collect::<Vec<_>>()
                .join("\n")
        )),
    }
}

/// Does the profile at `profile` back up to the local repository
/// `repo_canon` (already canonicalized)?
fn profile_targets_repository(profile: &Path, repo_canon: &Path, cwd: &Path) -> bool {
    let Ok(cfg) = fs::read_to_string(profile)
        .map_err(anyhow::Error::from)
        .and_then(|c| toml::from_str::<Config>(&c).map_err(anyhow::Error::from))
    else {
        return false;
    };
    let has_s3 =
        cfg.s3.as_ref().and_then(|s| s.bucket.as_ref()).is_some() || cfg.s3_bucket.is_some();
    if has_s3 || cfg.destination_path.trim().is_empty() || is_s3_url(&cfg.destination_path) {
        return false;
    }
    let dest = rsb_sdk::utils::expand_path(&cfg.destination_path);
    let dest = if dest.is_relative() {
        cwd.join(dest)
    } else {
        dest
    };
    dest.canonicalize().is_ok_and(|d| d == repo_canon)
}

/// Does the profile use relative source/destination paths (as written by
/// `create-profile --portable`)? They are resolved against the working
/// directory when the backup runs.
pub fn profile_uses_relative_paths(cfg: &Config) -> bool {
    let relative = |p: &str| !p.trim().is_empty() && !is_s3_url(p) && Path::new(p).is_relative();
    relative(&cfg.source_path) || relative(&cfg.destination_path)
}

/// Text printed by `rsb schedule`: a cron line or a systemd service that
/// runs `<exe> backup "<profile>" --storage <s3|local>` (matching what the
/// profile configures), with the encryption key read from `env_file` (mode
/// 0600) — never `--key` on the command line, where `ps`, the crontab and
/// the journal would show it. `profile` should be absolute. With `workdir`
/// (see [`profile_uses_relative_paths`]) the job first changes to that
/// directory, since cron/systemd do not start in the directory the
/// profile's relative paths were written for.
pub fn render_schedule(
    format: &str,
    exe: &Path,
    profile: &Path,
    cfg: &Config,
    env_file: &Path,
    workdir: Option<&Path>,
) -> Result<String> {
    let has_s3 =
        cfg.s3.as_ref().and_then(|s| s.bucket.as_ref()).is_some() || cfg.s3_bucket.is_some();
    let storage = if has_s3 { "s3" } else { "local" };
    let command = format!(
        "\"{}\" backup \"{}\" --storage {}",
        exe.display(),
        profile.display(),
        storage
    );
    let env_file = env_file.display();
    let key_env = config::ENCRYPTION_KEY_ENV;
    match format {
        "cron" => Ok(format!(
            "# 1) Store the encryption key in a file readable only by you:\n\
             #    (umask 077 && printf '{key_env}=%s\\n' 'YOUR_KEY' > \"{env_file}\")\n\
             # 2) Add this line to your crontab (crontab -e):\n\
             0 3 * * * {cd}set -a && . \"{env_file}\" && set +a && {command}\n",
            cd = workdir
                .map(|d| format!("cd \"{}\" && ", d.display()))
                .unwrap_or_default()
        )),
        "systemd" => Ok(format!(
            "# 1) Create {env_file} with mode 0600 containing:\n\
             #    {key_env}=YOUR_KEY\n\
             # 2) Example rsb-backup.service:\n\
             [Service]\nType=oneshot\n{wd}EnvironmentFile={env_file}\nExecStart={command}\n",
            wd = workdir
                .map(|d| format!("WorkingDirectory={}\n", d.display()))
                .unwrap_or_default()
        )),
        _ => Err(anyhow!("Unknown format. Use 'cron' or 'systemd'.")),
    }
}

/// Resolve config path from either --config, --backup, or auto-discover
/// Returns: (config_path, is_portable_mode)
pub fn resolve_config_or_backup(
    config: &Option<PathBuf>,
    backup: &Option<PathBuf>,
) -> Result<(PathBuf, bool)> {
    // If --backup provided, use it directly (portable mode)
    if let Some(backup_path) = backup {
        if backup_path.exists() {
            return Ok((backup_path.clone(), true));
        } else {
            return Err(anyhow!(
                "❌ Error: Backup path not found: {}",
                backup_path.display()
            ));
        }
    }

    // If --config provided, use it
    if let Some(config_path) = config {
        if config_path.exists() {
            return Ok((config_path.clone(), false));
        } else {
            return Err(anyhow!(
                "❌ Error: Configuration file not found: {}",
                config_path.display()
            ));
        }
    }

    resolve_profile_path(None).map(|p| (p, false))
}

/// Extract destination path from config.toml
pub fn extract_destination_from_config(config_path: &PathBuf) -> Result<PathBuf> {
    let config_content = fs::read_to_string(config_path)
        .map_err(|e| anyhow!("Failed to read config: {:?} - {}", config_path, e))?;

    let config_table: toml::Table = toml::from_str(&config_content)
        .map_err(|e| anyhow!("Failed to parse config.toml: {}", e))?;

    let dest_val = config_table
        .get("destination_path")
        .or_else(|| config_table.get("destination"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("'destination_path' not found in config.toml"))?;

    Ok(PathBuf::from(dest_val))
}

/// Is `dest` an `s3://` URL (scheme compared case-insensitively)?
pub fn is_s3_url(dest: &str) -> bool {
    dest.get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("s3://"))
}

/// Parses an `s3://bucket` destination and returns the bucket name, or
/// `None` when `dest` is not an S3 URL (a local path). The bucket name is
/// validated with [`validate_s3_bucket_name`]. Objects are always stored at
/// the bucket root, so a key prefix (`s3://bucket/some/prefix`) is rejected
/// instead of being silently ignored.
pub fn parse_s3_destination(dest: &str) -> Result<Option<String>> {
    if !is_s3_url(dest) {
        return Ok(None);
    }
    let rest = &dest[5..];
    let (bucket, prefix) = match rest.split_once('/') {
        Some((bucket, prefix)) => (bucket, prefix.trim_matches('/')),
        None => (rest, ""),
    };
    if bucket.is_empty() {
        return Err(anyhow!(
            "Invalid S3 destination '{}': missing bucket name (expected s3://<bucket>)",
            dest
        ));
    }
    if !prefix.is_empty() {
        return Err(anyhow!(
            "Invalid S3 destination '{}': key prefixes are not supported (backups are \
             stored at the root of the bucket). Use s3://{} or a dedicated bucket.",
            dest,
            bucket
        ));
    }
    validate_s3_bucket_name(bucket)?;
    Ok(Some(bucket.to_string()))
}

/// Where `create-profile` stores a backup, from `--dest` and `--s3-bucket`.
#[derive(Debug, PartialEq, Eq)]
pub struct ProfileDestination {
    /// Value for `destination_path`: the local repository directory, or
    /// `s3://<bucket>` for an S3 destination (never used as a local path).
    pub destination: PathBuf,
    /// S3 bucket for the profile's `s3` / `s3_bucket` fields.
    pub s3_bucket: Option<String>,
}

impl ProfileDestination {
    /// S3 profiles back up to the bucket: no local directory is created
    /// (with `--storage local` the backup creates its directory itself).
    pub fn is_s3(&self) -> bool {
        self.s3_bucket.is_some()
    }
}

/// Resolves `create-profile --dest` / `--s3-bucket`: `--dest s3://bucket`
/// selects S3 storage (same as `--s3-bucket bucket`); both must name the
/// same bucket when given together. Bucket names are validated.
pub fn plan_profile_destination(
    dest: &Path,
    s3_bucket: Option<&str>,
) -> Result<ProfileDestination> {
    let dest_str = dest.to_string_lossy();
    let from_url = parse_s3_destination(&dest_str)?;
    let s3_bucket = s3_bucket.map(str::trim).filter(|b| !b.is_empty());
    if let (Some(url_bucket), Some(flag_bucket)) = (from_url.as_deref(), s3_bucket)
        && url_bucket != flag_bucket
    {
        return Err(anyhow!(
            "--dest {} and --s3-bucket {} name different buckets",
            dest_str,
            flag_bucket
        ));
    }
    if let Some(b) = s3_bucket {
        validate_s3_bucket_name(b)?;
    }
    Ok(match from_url {
        Some(bucket) => ProfileDestination {
            destination: PathBuf::from(format!("s3://{}", bucket)),
            s3_bucket: Some(bucket),
        },
        None => ProfileDestination {
            destination: dest.to_path_buf(),
            s3_bucket: s3_bucket.map(str::to_string),
        },
    })
}

/// Checks an S3 bucket name against the S3 naming rules: 3-63 characters,
/// only lowercase letters, digits, `.` and `-`, starting and ending with a
/// letter or digit, no `..`, and not formatted like an IPv4 address.
pub fn validate_s3_bucket_name(name: &str) -> Result<()> {
    let invalid = |why: &str| {
        Err(anyhow!(
            "Invalid S3 bucket name '{}': {} (allowed: 3-63 characters, lowercase letters, \
             digits, '.' and '-', starting and ending with a letter or digit)",
            name,
            why
        ))
    };
    if !(3..=63).contains(&name.len()) {
        return invalid("must be 3-63 characters long");
    }
    if let Some(c) = name
        .chars()
        .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '.' || *c == '-'))
    {
        return invalid(&format!("character {:?} is not allowed", c));
    }
    let alnum = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric());
    if !alnum(name.chars().next()) || !alnum(name.chars().last()) {
        return invalid("must start and end with a letter or digit");
    }
    if name.contains("..") {
        return invalid("must not contain two adjacent periods");
    }
    if name.parse::<std::net::Ipv4Addr>().is_ok() {
        return invalid("must not be formatted as an IP address");
    }
    Ok(())
}

/// Avisa que um segredo passado como argumento fica visível em `ps`/histórico.
pub fn warn_secret_arg(flag: &str) {
    eprintln!(
        "⚠️  WARNING: secrets passed via {} are visible to other users (ps) and stored in shell history. \
         Prefer env {} / OS keyring / interactive prompt.",
        flag,
        config::ENCRYPTION_KEY_ENV
    );
}

/// Nome do perfil (usado como conta no keyring) a partir do caminho do .toml.
pub fn profile_name(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

/// Obtém a chave de cifra: `--key` (com aviso) → env RSB_ENCRYPTION_KEY →
/// keyring do perfil → chave legada no TOML → prompt (só em TTY).
/// Se `required` e nenhuma chave estiver disponível, devolve erro em vez de
/// prosseguir em claro.
pub fn obtain_encryption_key(
    cli_key: Option<String>,
    profile: Option<&str>,
    cfg: Option<&Config>,
    required: bool,
) -> Result<Option<String>> {
    if cli_key.is_some() {
        warn_secret_arg("--key");
    }
    if let Some(k) = config::resolve_encryption_key(cli_key, profile, cfg) {
        return Ok(Some(k));
    }
    if !required {
        return Ok(None);
    }
    if std::io::stdin().is_terminal() {
        let prompt = match profile {
            Some(p) => format!("Enter encryption key for profile '{}': ", p),
            None => "Enter encryption key: ".to_string(),
        };
        let key = rpassword::prompt_password(prompt)?;
        if key.is_empty() {
            return Err(anyhow!("Encryption key cannot be empty"));
        }
        return Ok(Some(key));
    }
    Err(anyhow!(
        "This profile has encryption enabled but no key is available. \
         Set {} (e.g. via a 0600 EnvironmentFile), store the key in the OS keyring \
         (rsb create-profile --encrypt), or run interactively. Refusing to back up unencrypted.",
        config::ENCRYPTION_KEY_ENV
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_toml_is_never_a_profile() {
        let dir = tempfile::tempdir().unwrap();
        let cargo = dir.path().join("Cargo.toml");
        fs::write(&cargo, "source_path = \"a\"\ndestination_path = \"b\"\n").unwrap();
        assert!(!is_profile_candidate(&cargo));
        assert!(find_profile_candidates(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn only_profile_tomls_are_candidates() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("other.toml"), "name = \"x\"\n").unwrap();
        fs::write(
            dir.path().join("b.toml"),
            "source_path = \"a\"\ndestination_path = \"b\"\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("a.toml"),
            "source_path = \"a\"\ndestination_path = \"b\"\n",
        )
        .unwrap();
        let found = find_profile_candidates(dir.path()).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found[0].ends_with("a.toml"));
    }

    #[test]
    fn s3_destination_is_parsed() {
        assert_eq!(
            parse_s3_destination("s3://my-backups").unwrap().as_deref(),
            Some("my-backups")
        );
        assert_eq!(
            parse_s3_destination("S3://my.backups.2026/")
                .unwrap()
                .as_deref(),
            Some("my.backups.2026")
        );
        // Local paths (also Windows ones) are not S3 destinations.
        for local in [
            "/backup/docs",
            "backup",
            "C:\\backup",
            "s3",
            "s3:/x",
            "./s3://x",
        ] {
            assert_eq!(parse_s3_destination(local).unwrap(), None, "{}", local);
        }
    }

    #[test]
    fn s3_destination_rejects_prefix_and_bad_names() {
        let err = parse_s3_destination("s3://bucket/daily/").unwrap_err();
        assert!(
            err.to_string().contains("prefixes are not supported"),
            "{}",
            err
        );
        assert!(parse_s3_destination("s3://").is_err());
        assert!(parse_s3_destination("s3:///x").is_err());
        assert!(parse_s3_destination("s3://My_Bucket").is_err());
    }

    #[test]
    fn profile_destination_from_s3_url_creates_no_local_dir() {
        let d = plan_profile_destination(Path::new("s3://my-backups"), None).unwrap();
        assert!(d.is_s3());
        assert_eq!(d.s3_bucket.as_deref(), Some("my-backups"));
        assert_eq!(d.destination, PathBuf::from("s3://my-backups"));

        // Same bucket given twice is fine; different buckets are not.
        let d = plan_profile_destination(Path::new("s3://b1b"), Some("b1b")).unwrap();
        assert_eq!(d.s3_bucket.as_deref(), Some("b1b"));
        assert!(plan_profile_destination(Path::new("s3://b1b"), Some("b2b")).is_err());

        // --s3-bucket with a local --dest: S3 profile, local path kept.
        let d = plan_profile_destination(Path::new("/backup/x"), Some("my-backups")).unwrap();
        assert!(d.is_s3());
        assert_eq!(d.destination, PathBuf::from("/backup/x"));
        assert!(plan_profile_destination(Path::new("/backup/x"), Some("Bad_Bucket")).is_err());

        let d = plan_profile_destination(Path::new("/backup/x"), None).unwrap();
        assert!(!d.is_s3());
        assert_eq!(d.destination, PathBuf::from("/backup/x"));
    }

    #[test]
    fn bucket_names_follow_s3_rules() {
        for ok in ["abc", "my-backups", "a.b-c.1", &"a".repeat(63)] {
            assert!(validate_s3_bucket_name(ok).is_ok(), "{}", ok);
        }
        for bad in [
            "ab",
            &"a".repeat(64),
            "MyBucket",
            "my_bucket",
            "-bucket",
            "bucket-",
            ".bucket",
            "my..bucket",
            "192.168.1.1",
            "bucket name",
        ] {
            assert!(validate_s3_bucket_name(bad).is_err(), "{}", bad);
        }
    }

    #[test]
    fn explicit_path_wins() {
        let p = PathBuf::from("/tmp/explicit.toml");
        assert_eq!(resolve_profile_path(Some(p.clone())).unwrap(), p);
    }
}
