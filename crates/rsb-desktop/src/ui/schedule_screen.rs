use dioxus::prelude::*;
use std::path::{Path, PathBuf};

use crate::ui::{app::AppConfig, i18n::get_texts, profile_loader::load_profile};

/// The CLI the scheduled job runs: `rsb-cli` installed next to the desktop
/// app if present, otherwise `rsb-cli` from the PATH.
pub fn cli_executable() -> PathBuf {
    let name = format!("rsb-cli{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&name)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

/// The job for `profile`, generated exactly like `rsb schedule`: the
/// profile must load, the key is read from a 0600 env file (never put on
/// the command line), and `--storage` matches the profile.
pub fn schedule_text(profile: &Path, format: &str, exe: &Path) -> Result<String, String> {
    let loaded = load_profile(profile)?;
    let abs = std::fs::canonicalize(profile)
        .map_err(|e| format!("Cannot resolve {}: {}", profile.display(), e))?;
    // Relative (portable) paths: run from the profile's folder (cron and
    // systemd do not start in any particular directory).
    let workdir = rsb_sdk::schedule::profile_uses_relative_paths(&loaded.config)
        .then(|| abs.parent().map(Path::to_path_buf))
        .flatten();
    rsb_sdk::schedule::render_schedule(
        format,
        exe,
        &abs,
        &loaded.config,
        &rsb_sdk::schedule::default_env_file(),
        workdir.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[component]
pub fn ScheduleScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());

    let mut config_path = use_signal(PathBuf::new);
    let mut format = use_signal(|| "cron".to_string());
    let mut output_command = use_signal(String::new);
    let mut status_message = use_signal(String::new);

    let handle_generate = move |_| {
        let cfg_path = config_path();
        if cfg_path.as_os_str().is_empty() {
            status_message.set(texts.please_select_config_file.to_string());
            return;
        }
        match schedule_text(&cfg_path, &format(), &cli_executable()) {
            Ok(text) => {
                output_command.set(text);
                status_message.set(
                    "✅ Job generated. Install it as described below (nothing is scheduled automatically)."
                        .to_string(),
                );
            }
            Err(e) => {
                output_command.set(String::new());
                status_message.set(format!("❌ {}", e));
            }
        }
    };

    rsx! {
        div { class: "card",
            h2 { class: "page-title", "{texts.schedule_title}" }

            div { class: "form-group",
                label { class: "label-text", "{texts.config_file_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{texts.select_config_file_hint}",
                        value: "{config_path.read().to_string_lossy()}",
                        readonly: true
                    }
                    button {
                        class: "btn-icon",
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(h) = rfd::AsyncFileDialog::new()
                                    .add_filter("TOML", &["toml"])
                                    .pick_file()
                                    .await
                                {
                                    config_path.set(h.path().to_path_buf());
                                    status_message.set(String::new());
                                    output_command.set(String::new());
                                }
                            });
                        },
                        "📂"
                    }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.schedule_format_label}" }
                select {
                    class: "select-field",
                    value: "{format}",
                    onchange: move |evt| {
                        format.set(evt.value());
                        output_command.set(String::new());
                    },
                    option { value: "cron", "⏰ Cron (Linux/macOS)" }
                    option { value: "systemd", "🐧 Systemd Service (Linux)" }
                }
                p { class: "hint",
                    "Same output as `rsb schedule <profile> --format <cron|systemd>`. The encryption key is read from ~/.rs-shield/rsb-backup.env (mode 0600), never written into the job."
                }
            }

            if !status_message().is_empty() {
                div {
                    class: "alert",
                    class: if status_message().starts_with("✅") { "alert-success" } else { "alert-error" },
                    "{status_message}"
                }
            }

            div { class: "flex gap-3",
                button {
                    class: "btn-primary flex-1",
                    onclick: handle_generate,
                    disabled: config_path().as_os_str().is_empty(),
                    "🔧 Generate Job"
                }
            }

            div { class: "form-group",
                label { class: "label-text", "📝 Scheduled Job" }
                textarea {
                    class: "textarea-field",
                    readonly: true,
                    value: "{output_command}",
                    style: "height: 160px; font-family: monospace; font-size: 0.85rem;"
                }
            }

            div { class: "info-box",
                h4 { class: "font-semibold mb-2", if format() == "cron" { "Cron Instructions:" } else { "Systemd Instructions:" }}
                if format() == "cron" {
                    p { class: "text-xs mb-2", "1. Create the key file as shown in the first comment line" }
                    p { class: "text-xs mb-2", "2. Open your crontab: crontab -e" }
                    p { class: "text-xs", "3. Paste the last line and save" }
                } else {
                    p { class: "text-xs mb-2", "1. Create the key file as shown in the comment" }
                    p { class: "text-xs mb-2", "2. Save the service as ~/.config/systemd/user/rsb-backup.service (or /etc/systemd/system/)" }
                    p { class: "text-xs mb-2", "3. Add an rsb-backup.timer with OnCalendar=daily" }
                    p { class: "text-xs", "4. Run: systemctl --user enable --now rsb-backup.timer" }
                }
                p { class: "text-xs mt-3 text-slate-600 dark:text-slate-400",
                    "ℹ️ {texts.schedule_hint}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_matches_the_cli_and_never_contains_a_key() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("docs.toml");
        std::fs::write(
            &profile,
            "source_path = \"/src\"\ndestination_path = \"/repo\"\nencrypt_patterns = [\"*\"]\n",
        )
        .unwrap();
        let exe = Path::new("/opt/rsb/rsb-cli");
        let cron = schedule_text(&profile, "cron", exe).unwrap();
        assert!(cron.contains("--storage local"));
        assert!(cron.contains("\"/opt/rsb/rsb-cli\" backup"));
        assert!(cron.contains("RSB_ENCRYPTION_KEY"));
        assert!(!cron.contains("--key"));
        assert!(!cron.contains("cd \""), "absolute profile: no workdir");
        let unit = schedule_text(&profile, "systemd", exe).unwrap();
        assert!(unit.contains("EnvironmentFile="));
        assert!(schedule_text(&profile, "launchd", exe).is_err());
    }

    #[test]
    fn invalid_or_portable_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.toml");
        std::fs::write(&bad, "nope").unwrap();
        assert!(schedule_text(&bad, "cron", Path::new("rsb-cli")).is_err());

        let portable = dir.path().join("p.toml");
        std::fs::write(
            &portable,
            "source_path = \"src\"\ndestination_path = \"s3://b\"\n[s3]\nbucket = \"b\"\n",
        )
        .unwrap();
        let cron = schedule_text(&portable, "cron", Path::new("rsb-cli")).unwrap();
        assert!(cron.contains("--storage s3"));
        assert!(cron.contains("cd \""), "relative paths need the workdir");
    }
}
