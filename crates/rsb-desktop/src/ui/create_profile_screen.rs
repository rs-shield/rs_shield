use dioxus::prelude::*;
use rsb_sdk::config::{self, Config, S3Config};
use std::path::{Path, PathBuf};

use crate::ui::components::{
    Button, ButtonSize, ButtonVariant, Card, CardVariant, Input, InputVariant,
};
use crate::ui::{
    app::AppConfig,
    i18n::{Translations, get_texts, tr},
    s3_settings,
};

/// Fields of the Create Profile form.
#[derive(Debug, Clone, Default)]
pub struct ProfileForm {
    pub name: String,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub exclude_patterns: String,
    pub encrypt: bool,
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
    pub s3_endpoint: Option<String>,
}

/// Directory of the profiles listed by "Manage profiles" (`~/.rsb-desktop`).
pub fn profiles_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rsb-desktop")
}

/// A profile name is also its file name and its OS keyring account: only
/// letters, digits, `-`, `_` and `.` (no path separators, no leading dot),
/// so it can never write outside the profiles folder.
pub fn validate_profile_name(t: &Translations, name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        Err(tr(t.cp_invalid_profile_name, &[&name]))
    }
}

/// Builds the profile like `rsb create-profile` does (same defaults; the
/// key is never part of it). With an S3 bucket and no destination, the
/// destination is `s3://<bucket>`.
pub fn build_profile(t: &Translations, form: &ProfileForm) -> Result<Config, String> {
    validate_profile_name(t, &form.name)?;
    if form.source.as_os_str().is_empty() {
        return Err(t.cp_select_source_folder.to_string());
    }
    let bucket = form
        .s3_bucket
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty());
    let destination = match (form.destination.as_os_str().is_empty(), bucket) {
        (false, _) => form.destination.to_string_lossy().into_owned(),
        (true, Some(b)) => format!("s3://{}", b),
        (true, None) => {
            return Err(t.cp_select_dest_folder_or_s3.to_string());
        }
    };
    let exclude_patterns: Vec<String> = form
        .exclude_patterns
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let region = form.s3_region.clone().filter(|r| !r.trim().is_empty());
    let endpoint = form.s3_endpoint.clone().filter(|e| !e.trim().is_empty());
    Ok(Config {
        source_path: form.source.to_string_lossy().into_owned(),
        destination_path: destination,
        exclude_patterns: if exclude_patterns.is_empty() {
            Config::default().exclude_patterns
        } else {
            exclude_patterns
        },
        backup_mode: "incremental".to_string(),
        s3_bucket: bucket.map(str::to_string),
        s3_region: bucket.and(region.clone()),
        s3_endpoint: bucket.and(endpoint.clone()),
        s3: bucket.map(|b| S3Config {
            bucket: Some(b.to_string()),
            region,
            endpoint,
            access_key: None,
            secret_key: None,
        }),
        encrypt_patterns: form.encrypt.then(|| vec!["*".to_string()]),
        pause_on_low_battery: Some(20),
        pause_on_high_cpu: Some(65),
        compression_level: Some(3),
        ..Config::default()
    })
}

/// Writes `<dir>/<name>.toml` (mode 0600, never overwriting an existing
/// profile) and returns its path.
pub fn write_profile(
    t: &Translations,
    dir: &Path,
    name: &str,
    cfg: &Config,
) -> Result<PathBuf, String> {
    validate_profile_name(t, name)?;
    config::create_dir_secure(dir).map_err(|e| tr(t.cp_cannot_create, &[&dir.display(), &e]))?;
    let path = dir.join(format!("{}.toml", name));
    if path.exists() {
        return Err(tr(t.cp_profile_exists, &[&name, &path.display()]));
    }
    config::write_config_secure(&path, cfg)
        .map_err(|e| tr(t.cp_cannot_write, &[&path.display(), &e]))?;
    Ok(path)
}

#[component]
pub fn CreateProfileScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());

    let mut profile_name = use_signal(String::new);
    let mut source_path = use_signal(PathBuf::new);
    let mut dest_path = use_signal(PathBuf::new);
    let mut exclude_patterns =
        use_signal(|| config::DEFAULT_EXCLUDE_PATTERNS.join("\n").to_string());
    let mut message = use_signal(String::new);

    let mut encrypt = use_signal(|| true);
    let mut password = use_signal(String::new);
    let mut password_confirm = use_signal(String::new);

    let mut use_s3 = use_signal(|| false);
    let mut s3_bucket = use_signal(String::new);
    let mut s3_region = use_signal(String::new);
    let mut s3_endpoint = use_signal(String::new);
    let mut s3_access_key = use_signal(String::new);
    let mut s3_secret_key = use_signal(String::new);

    let mut is_loading = use_signal(|| false);
    let mut is_success = use_signal(|| false);

    let handle_select_source = move |_| {
        spawn(async move {
            if let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await {
                source_path.set(folder.path().to_path_buf());
                message.set(String::new());
            }
        });
    };

    let handle_select_dest = move |_| {
        spawn(async move {
            if let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await {
                dest_path.set(folder.path().to_path_buf());
                message.set(String::new());
            }
        });
    };

    let handle_create = move |_| {
        is_success.set(false);
        let s3_on = use_s3();
        let form = ProfileForm {
            name: profile_name().trim().to_string(),
            source: source_path(),
            destination: dest_path(),
            exclude_patterns: exclude_patterns(),
            encrypt: encrypt(),
            s3_bucket: s3_on.then(|| s3_bucket.read().clone()),
            s3_region: s3_on.then(|| s3_region.read().clone()),
            s3_endpoint: s3_on.then(|| s3_endpoint.read().clone()),
        };
        let cfg = match build_profile(texts, &form) {
            Ok(c) => c,
            Err(e) => {
                message.set(format!("❌ {}", e));
                return;
            }
        };
        let pwd = password();
        if form.encrypt && pwd != password_confirm() {
            message.set(texts.cp_passwords_mismatch.to_string());
            return;
        }
        let access_key = s3_access_key();
        let secret_key = s3_secret_key();
        let save_creds = use_s3() && !access_key.is_empty() && !secret_key.is_empty();

        is_loading.set(true);
        spawn(async move {
            let name = form.name.clone();
            // File writes, keyring and the credentials KDF block: keep them
            // off the UI thread.
            let result = tokio::task::spawn_blocking(move || {
                let path = write_profile(texts, &profiles_dir(), &form.name, &cfg)?;
                let mut notes = vec![tr(texts.cp_profile_saved, &[&path.display()])];
                if form.encrypt {
                    if pwd.is_empty() {
                        notes.push(tr(
                            texts.cp_no_password_stored,
                            &[&config::ENCRYPTION_KEY_ENV],
                        ));
                    } else {
                        match config::store_encryption_key_in_keyring(&form.name, &pwd) {
                            Ok(()) => notes.push(texts.cp_key_stored_keyring.to_string()),
                            Err(e) => notes.push(tr(
                                texts.cp_key_not_saved,
                                &[&e, &config::ENCRYPTION_KEY_ENV],
                            )),
                        }
                    }
                }
                if save_creds {
                    match s3_settings::remember_credentials(&access_key, &secret_key) {
                        Ok(p) => notes.push(tr(texts.cp_s3_creds_saved, &[&p.display()])),
                        Err(e) => notes.push(tr(texts.cp_s3_creds_not_saved, &[&e])),
                    }
                }
                Ok::<_, String>(notes)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);

            match result {
                Ok(notes) => {
                    message.set(format!(
                        "{}\n{}",
                        texts.create_profile_success.replace("{0}", &name),
                        notes.join("\n")
                    ));
                    is_success.set(true);
                    profile_name.set(String::new());
                    source_path.set(PathBuf::new());
                    dest_path.set(PathBuf::new());
                    password.set(String::new());
                    password_confirm.set(String::new());
                    s3_secret_key.set(String::new());
                }
                Err(e) => message.set(texts.error_creating_profile.replace("{0}", &e)),
            }
            is_loading.set(false);
        });
    };

    let profile_saved_msg = use_memo(move || {
        tr(
            texts.cp_profile_will_be_saved,
            &[&profiles_dir()
                .join(format!("{}.toml", profile_name().trim()))
                .display()],
        )
    });

    rsx! {
        div { class: "flex flex-col gap-4 p-4 bg-slate-50 dark:bg-slate-900",
            div { class: "space-y-1",
                h1 { class: "text-lg font-semibold text-slate-900 dark:text-slate-100", "{texts.cp_title}" }
                p { class: "text-sm text-slate-600 dark:text-slate-400", "{texts.cp_subtitle}" }
            }

            Card { variant: CardVariant::Elevated,
                div { class: "space-y-4",
                    Input {
                        label: texts.profile_name_label.to_string(),
                        placeholder: "documents_backup".to_string(),
                        value: "{profile_name}",
                        helper_text: profile_saved_msg().to_string(),
                        oninput: Some(EventHandler::new(move |evt: FormEvent| {
                            profile_name.set(evt.value());
                            message.set(String::new());
                        })),
                    }

                    div { class: "space-y-2",
                        label { class: "text-sm font-semibold text-slate-700 dark:text-slate-300",
                            "{texts.source_dir_label}"
                        }
                        div { class: "flex gap-2",
                            input {
                                class: "flex-1 px-4 py-2 text-sm rounded-md border border-slate-300 dark:border-slate-600 bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-300 cursor-not-allowed",
                                r#type: "text",
                                placeholder: texts.cp_select_folder_placeholder,
                                value: "{source_path.read().to_string_lossy()}",
                                readonly: true
                            }
                            Button {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::SM,
                                disabled: is_loading(),
                                onclick: handle_select_source,
                                "{texts.cp_select_button}"
                            }
                        }
                        p { class: "text-xs text-slate-600 dark:text-slate-400",
                            "{texts.source_dir_hint}"
                        }
                    }

                    div { class: "space-y-2",
                        label { class: "text-sm font-semibold text-slate-700 dark:text-slate-300",
                            "{texts.dest_dir_label}"
                        }
                        div { class: "flex gap-2",
                            input {
                                class: "flex-1 px-4 py-2 text-sm rounded-md border border-slate-300 dark:border-slate-600 bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-300 cursor-not-allowed",
                                r#type: "text",
                                placeholder: texts.cp_select_folder_optional_s3,
                                value: "{dest_path.read().to_string_lossy()}",
                                readonly: true
                            }
                            Button {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::SM,
                                disabled: is_loading(),
                                onclick: handle_select_dest,
                                "{texts.cp_select_button}"
                            }
                        }
                        p { class: "text-xs text-slate-600 dark:text-slate-400",
                            "{texts.dest_dir_hint}"
                        }
                    }

                    div { class: "space-y-2",
                        label { class: "text-sm font-semibold text-slate-700 dark:text-slate-300",
                            "{texts.exclude_patterns_label}"
                        }
                        textarea {
                            class: "w-full px-3 py-2.5 text-sm rounded-md border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 text-slate-900 dark:text-slate-100 font-mono focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500 outline-none transition-all duration-150 resize-none",
                            value: "{exclude_patterns}",
                            oninput: move |evt: Event<FormData>| exclude_patterns.set(evt.value()),
                            rows: "6"
                        }
                        p { class: "text-xs text-slate-600 dark:text-slate-400",
                            "{texts.cp_exclude_help}"
                        }
                    }
                }
            }

            Card { variant: CardVariant::Outlined,
                div { class: "space-y-4",
                    label { class: "flex items-center gap-2 text-sm font-semibold text-slate-900 dark:text-slate-100",
                        input {
                            r#type: "checkbox",
                            checked: encrypt(),
                            onchange: move |_| encrypt.toggle()
                        }
                        "{texts.cp_encrypt_checkbox}"
                    }
                    if encrypt() {
                        Input {
                            variant: InputVariant::Password,
                            label: texts.cp_encryption_password.to_string(),
                            value: "{password}",
                            helper_text: texts.cp_encryption_password_hint.to_string(),
                            oninput: Some(EventHandler::new(move |evt: FormEvent| password.set(evt.value()))),
                        }
                        Input {
                            variant: InputVariant::Password,
                            label: texts.cp_confirm_password.to_string(),
                            value: "{password_confirm}",
                            oninput: Some(EventHandler::new(move |evt: FormEvent| password_confirm.set(evt.value()))),
                        }
                    }
                }
            }

            Card { variant: CardVariant::Outlined,
                div { class: "space-y-4",
                    div {
                        class: "flex items-center justify-between cursor-pointer select-none hover:opacity-75 transition-opacity",
                        onclick: move |_| use_s3.set(!use_s3()),
                        h3 { class: "text-lg font-semibold text-slate-900 dark:text-slate-100", "{texts.s3_config_optional}" }
                        div {
                            class: if use_s3() { "text-indigo-600 dark:text-indigo-400 transform rotate-180 transition-transform" } else { "text-slate-400 dark:text-slate-500 transition-transform" },
                            "▼"
                        }
                    }

                    if use_s3() {
                        div { class: "space-y-4 pt-4 border-t border-slate-200 dark:border-slate-700",
                            Input {
                                label: texts.bucket_label.to_string(),
                                placeholder: "my-backup-bucket".to_string(),
                                value: "{s3_bucket}",
                                oninput: Some(EventHandler::new(move |evt: FormEvent| s3_bucket.set(evt.value()))),
                            }

                            div { class: "grid grid-cols-2 gap-4",
                                Input {
                                    label: texts.cp_region.to_string(),
                                    placeholder: "us-east-1".to_string(),
                                    value: "{s3_region}",
                                    oninput: Some(EventHandler::new(move |evt: FormEvent| s3_region.set(evt.value()))),
                                }
                                Input {
                                    label: texts.cp_endpoint_optional.to_string(),
                                    placeholder: "https://s3.amazonaws.com".to_string(),
                                    value: "{s3_endpoint}",
                                    oninput: Some(EventHandler::new(move |evt: FormEvent| s3_endpoint.set(evt.value()))),
                                }
                            }

                            Input {
                                label: "Access Key ID".to_string(),
                                placeholder: "AKIA...".to_string(),
                                value: "{s3_access_key}",
                                oninput: Some(EventHandler::new(move |evt: FormEvent| s3_access_key.set(evt.value()))),
                            }

                            Input {
                                variant: InputVariant::Password,
                                label: "Secret Access Key".to_string(),
                                placeholder: "••••••••".to_string(),
                                value: "{s3_secret_key}",
                                helper_text: texts.cp_secret_key_hint.to_string(),
                                oninput: Some(EventHandler::new(move |evt: FormEvent| s3_secret_key.set(evt.value()))),
                            }
                        }
                    } else {
                        p { class: "text-sm text-slate-600 dark:text-slate-400 pt-2",
                            "{texts.cp_s3_click_hint}"
                        }
                    }
                }
            }

            if !message().is_empty() {
                div {
                    class: if is_success() {
                        "flex items-start gap-3 p-4 rounded-lg bg-emerald-50 dark:bg-emerald-900/20 border-l-4 border-emerald-500"
                    } else {
                        "flex items-start gap-3 p-4 rounded-lg bg-red-50 dark:bg-red-900/20 border-l-4 border-red-500"
                    },
                    span { class: "text-xl flex-shrink-0 mt-0.5", if is_success() { "✅" } else { "❌" } }
                    p { class: "text-sm whitespace-pre-wrap", "{message}" }
                }
            }

            div { class: "flex flex-col sm:flex-row gap-3",
                Button {
                    variant: ButtonVariant::Primary,
                    size: ButtonSize::MD,
                    loading: is_loading(),
                    disabled: is_loading() || profile_name().trim().is_empty() || source_path().as_os_str().is_empty(),
                    class: "flex-1".to_string(),
                    onclick: handle_create,
                    "{texts.cp_create_button}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::i18n::{Language, get_texts};

    fn t() -> &'static Translations {
        get_texts(Language::En)
    }

    fn form() -> ProfileForm {
        ProfileForm {
            name: "docs".into(),
            source: PathBuf::from("/src"),
            destination: PathBuf::from("/repo"),
            exclude_patterns: "*.tmp\n".into(),
            encrypt: true,
            ..Default::default()
        }
    }

    #[test]
    fn names_cannot_escape_the_profiles_folder() {
        for bad in ["", "../x", "a/b", "a\\b", ".hidden", "with space"] {
            assert!(validate_profile_name(t(), bad).is_err(), "{bad}");
        }
        for good in ["docs", "my-backup_2.v1"] {
            assert!(validate_profile_name(t(), good).is_ok(), "{good}");
        }
    }

    #[test]
    fn profile_matches_the_cli_and_has_no_secrets() {
        let cfg = build_profile(t(), &form()).unwrap();
        assert_eq!(cfg.encrypt_patterns, Some(vec!["*".to_string()]));
        assert_eq!(cfg.exclude_patterns, ["*.tmp"]);
        assert_eq!(cfg.pause_on_high_cpu, Some(65));
        assert!(cfg.encryption_key.is_none() && cfg.s3.is_none());

        let dir = tempfile::tempdir().unwrap();
        let path = write_profile(t(), dir.path(), "docs", &cfg).unwrap();
        let loaded = crate::ui::profile_loader::load_profile(&path, t()).unwrap();
        assert!(loaded.config.encryption_required());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        // Never overwrite an existing profile.
        assert!(write_profile(t(), dir.path(), "docs", &cfg).is_err());
    }

    #[test]
    fn s3_profile_needs_no_local_destination() {
        let mut f = form();
        f.destination = PathBuf::new();
        assert!(build_profile(t(), &f).is_err());
        f.s3_bucket = Some(" my-bucket ".into());
        f.s3_region = Some("eu-west-1".into());
        f.s3_endpoint = Some(String::new());
        let cfg = build_profile(t(), &f).unwrap();
        assert_eq!(cfg.destination_path, "s3://my-bucket");
        assert_eq!(cfg.s3_bucket.as_deref(), Some("my-bucket"));
        let s3 = cfg.s3.unwrap();
        assert_eq!(s3.region.as_deref(), Some("eu-west-1"));
        assert!(s3.endpoint.is_none() && s3.secret_key.is_none());
    }
}
