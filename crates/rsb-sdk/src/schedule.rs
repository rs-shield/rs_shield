//! Scheduled backups: the cron line / systemd service printed by
//! `rsb schedule` and shown by the desktop Schedule screen (one
//! implementation, so both give the same job).

use crate::config::{self, Config};
use crate::error::RsbError;
use std::path::{Path, PathBuf};

fn is_s3_url(p: &str) -> bool {
    p.get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("s3://"))
}

/// Does the profile use relative source/destination paths (as written by
/// `create-profile --portable`)? They are resolved against the working
/// directory when the backup runs.
pub fn profile_uses_relative_paths(cfg: &Config) -> bool {
    let relative = |p: &str| !p.trim().is_empty() && !is_s3_url(p) && Path::new(p).is_relative();
    relative(&cfg.source_path) || relative(&cfg.destination_path)
}

/// File the scheduled job reads the encryption key from
/// (`~/.rs-shield/rsb-backup.env`, mode 0600).
pub fn default_env_file() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".rs-shield").join("rsb-backup.env"))
        .unwrap_or_else(|| PathBuf::from("rsb-backup.env"))
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
) -> Result<String, RsbError> {
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
        _ => Err(RsbError::InvalidInput(
            "Unknown format. Use 'cron' or 'systemd'.".to_string(),
        )),
    }
}
