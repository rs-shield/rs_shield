use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::ui::{app::AppConfig, i18n::get_texts};
use rsb_sdk::credentials::credentials_manager::{CredentialsManager, S3Credentials};
use rsb_sdk::credentials::SecureString;
use rsb_sdk::utils::ensure_directory_exists;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct S3Config {
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    // NOTE: access_key and secret_key are never saved in TOML for security!
    // They use environment variables, keyring, or an encrypted file.
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub source_path: String,
    pub destination_path: String,
    pub exclude_patterns: Vec<String>,
    pub encryption_key: Option<String>,
    pub backup_mode: String,
    pub s3: Option<S3Config>,
    pub encrypt_patterns: Option<Vec<String>>,
    pub pause_on_low_battery: Option<u8>,
    pub pause_on_high_cpu: Option<u8>,
    pub compression_level: Option<u8>,
}

#[component]
pub fn CreateProfileScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());

    let mut profile_name = use_signal(String::new);
    let mut source_path = use_signal(PathBuf::new);
    let mut dest_path = use_signal(PathBuf::new);
    let mut exclude_patterns = use_signal(|| "*.tmp\nnode_modules\n.git".to_string());
    let mut message = use_signal(String::new);

    // S3 Signals
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
        let name = profile_name();
        let source = source_path();
        let dest = dest_path();
        let _excludes = exclude_patterns();

        if name.is_empty() {
            message.set(texts.profile_name_required.to_string());
            return;
        }

        if source.as_os_str().is_empty() {
            message.set(texts.select_source_dir.to_string());
            return;
        }

        if dest.as_os_str().is_empty() {
            message.set(texts.select_dest_dir.to_string());
            return;
        }

        let s3_config = if use_s3() {
            // Nunca salvar credenciais no TOML por segurança!
            Some(S3Config {
                bucket: if s3_bucket().is_empty() {
                    None
                } else {
                    Some(s3_bucket())
                }, // Never save credentials in TOML for security!
                region: if s3_region().is_empty() {
                    None
                } else {
                    Some(s3_region())
                }, // access_key and secret_key are managed via:
                endpoint: if s3_endpoint().is_empty() {
                    None
                } else {
                    Some(s3_endpoint())
                }, // 1. Environment variables (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY)
                   // 2. Encrypted file (~/.rs-shield/s3_credentials.enc)
                   // 3. System keyring
            })
        } else {
            None
        };

        is_loading.set(true);
        is_success.set(false);

        // Capture credentials if provided
        let access_key = s3_access_key().clone();
        let secret_key = s3_secret_key().clone();
        let excludes = exclude_patterns();

        tracing::debug!("=== Form submitted ===");
        tracing::debug!("s3_use: {}", use_s3());
        tracing::debug!("access_key length: {}", access_key.len());
        tracing::debug!("secret_key length: {}", secret_key.len());

        spawn(async move {
            match create_profile_async(
                &name, &source, &dest, &excludes, s3_config, access_key, secret_key,
            )
            .await
            {
                Ok(_) => {
                    let success_msg = texts.create_profile_success.replace("{0}", &name);
                    message.set(success_msg);
                    is_success.set(true);
                    profile_name.set(String::new());
                    source_path.set(PathBuf::new());
                    dest_path.set(PathBuf::new());
                }
                Err(e) => {
                    let error_msg = texts.error_creating_profile.replace("{0}", &e);
                    message.set(error_msg);
                }
            }
            is_loading.set(false);
        });
    };

    let profile_saved_msg =
        use_memo(move || texts.profile_saved_as.replace("{0}", &profile_name()));

    rsx! {
        div { class: "card",
            h2 { class: "page-title", "{texts.create_profile_title}" }

            div { class: "form-group",
                label { class: "label-text", "{texts.profile_name_label}" }
                input {
                    class: "input-field",
                    r#type: "text",
                    placeholder: "E.g.: documents_backup",
                    value: "{profile_name}",
                    oninput: move |evt| {
                        profile_name.set(evt.value());
                        message.set(String::new());
                    }
                }
                p { class: "hint", "{profile_saved_msg()}" }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.source_dir_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{texts.select_source_folder}",
                        value: "{source_path.read().to_string_lossy()}",
                        readonly: true
                    }
                    button {
                        class: "btn-icon",
                        onclick: handle_select_source,
                        disabled: is_loading(),
                        "📂"
                    }
                }
                p { class: "hint", "{texts.source_dir_hint}" }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.dest_dir_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{texts.select_dest_folder}",
                        value: "{dest_path.read().to_string_lossy()}",
                        readonly: true
                    }
                    button {
                        class: "btn-icon",
                        onclick: handle_select_dest,
                        disabled: is_loading(),
                        "📂"
                    }
                }
                p { class: "hint", "{texts.dest_dir_hint}" }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.exclude_patterns_label}" }
                textarea {
                    class: "input-field min-h-[120px] font-mono text-sm",
                    placeholder: "One pattern per line:\n*.tmp\nnode_modules\n.git\n.DS_Store\n__pycache__",
                    value: "{exclude_patterns}",
                    oninput: move |evt| exclude_patterns.set(evt.value())
                }
                p { class: "hint",
                    "{texts.exclude_patterns_hint}\n"
                    "- \"*.ext\" for wildcards (e.g., *.tmp, *.log)\n"
                    "- \".hidden\" for hidden files (e.g., .git, .DS_Store)\n"
                    "- \"folder\" for entire folders (e.g., node_modules, __pycache__)"
                }
            }

            div { class: "card-section mt-4 border-t border-slate-200 dark:border-slate-700 pt-4",
                div {
                    class: "flex items-center justify-between cursor-pointer select-none",
                    onclick: move |_| use_s3.set(!use_s3()),
                    h3 { class: "section-title mb-0", "{texts.s3_config_optional}" }
                    div { class: if use_s3() { "text-indigo-600 transform rotate-180 transition-transform" } else { "text-slate-400 transition-transform" },
                        "▼"
                    }
                }

                if use_s3() {
                    div { class: "mt-4 space-y-4 animate-fade-in",
                        div { class: "form-group",
                            label { class: "label-text", "Bucket Name" }
                            input {
                                class: "input-field", r#type: "text", placeholder: "e.g., my-backup-bucket",
                                value: "{s3_bucket}", oninput: move |evt| s3_bucket.set(evt.value())
                            }
                        }
                        div { class: "grid grid-cols-2 gap-4",
                            div { class: "form-group",
                                label { class: "label-text", "Region" }
                                input {
                                    class: "input-field", r#type: "text", placeholder: "e.g., us-east-1",
                                    value: "{s3_region}", oninput: move |evt| s3_region.set(evt.value())
                                }
                            }
                            div { class: "form-group",
                                label { class: "label-text", "Endpoint (Optional)" }
                                input {
                                    class: "input-field", r#type: "text", placeholder: "e.g., https://s3.amazonaws.com",
                                    value: "{s3_endpoint}", oninput: move |evt| s3_endpoint.set(evt.value())
                                }
                            }
                        }
                        div { class: "form-group",
                            label { class: "label-text", "Access Key ID" }
                            input {
                                class: "input-field", r#type: "text", placeholder: "AWS_ACCESS_KEY_ID",
                                value: "{s3_access_key}", oninput: move |evt| s3_access_key.set(evt.value())
                            }
                        }
                        div { class: "form-group",
                            label { class: "label-text", "Secret Access Key" }
                            input {
                                class: "input-field", r#type: "password", placeholder: "AWS_SECRET_ACCESS_KEY",
                                value: "{s3_secret_key}", oninput: move |evt| s3_secret_key.set(evt.value())
                            }
                            p { class: "hint text-amber-600 dark:text-amber-400",
                                "⚠️ Note: Credentials will be saved in the configuration file. For greater security, use environment variables or IAM roles if possible."
                            }
                        }
                    }
                } else {
                    p { class: "hint mt-2",
                        "Click to expand if you want to configure remote backup for AWS S3, MinIO, Cloudflare R2, etc."
                    }
                }
            }

            if !message().is_empty() {
                div {
                    class: "alert",
                    class: if is_success() { "alert-success" } else { "alert-error" },
                    "{message}"
                }
            }

            button {
                class: "btn-primary",
                class: if is_loading() { "btn-disabled" } else { "" },
                onclick: handle_create,
                disabled: is_loading() || profile_name().is_empty(),
                if is_loading() {
                    "⏳ Creating profile..."
                } else {
                    "✨ Create Profile"
                }
            }

            div { class: "card-section",
                h3 { class: "section-title", "{texts.next_steps_title}" }
                p { class: "hint", "{texts.next_steps_hint}" }
            }
        }
    }
}

async fn create_profile_async(
    name: &str,
    source: &std::path::Path,
    dest: &std::path::Path,
    excludes_str: &str,
    s3_config: Option<S3Config>,
    access_key: String,
    secret_key: String,
) -> Result<(), String> {
    // Parse exclude patterns from textarea (one per line)
    let exclude_patterns: Vec<String> = excludes_str
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let config = Config {
        source_path: source.to_string_lossy().into_owned(),
        destination_path: dest.to_string_lossy().into_owned(),
        exclude_patterns,
        encryption_key: None,
        backup_mode: "incremental".to_string(),
        s3: s3_config.clone(),
        encrypt_patterns: None,
        pause_on_low_battery: Some(20),
        pause_on_high_cpu: Some(20),
        compression_level: Some(3),
    };

    let toml_str =
        toml::to_string(&config).map_err(|e| format!("Error serializing TOML: {}", e))?;

    let filename = format!("{}.toml", name);

    // Try to save in the configuration directory (~/.rsb-desktop/)
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        let config_dir_path = format!("{}/.rsb-desktop", home_str);

        // Use centralized function to create directory
        let config_dir = ensure_directory_exists(&config_dir_path)
            .map_err(|e| format!("Error creating configuration directory: {}", e))?;

        let config_path = config_dir.join(&filename);

        match fs::write(&config_path, &toml_str) {
            Ok(_) => {
                tracing::info!("Profile saved at: {:?}", config_path);

                // Debug: check values
                tracing::debug!("=== Debug: Salvando credenciais S3 ===");
                tracing::debug!("s3_config.is_some(): {}", s3_config.is_some());
                tracing::debug!("access_key.is_empty(): {}", access_key.is_empty());
                tracing::debug!("secret_key.is_empty(): {}", secret_key.is_empty());

                // Save S3 credentials securely if provided
                if s3_config.is_some() && !access_key.is_empty() && !secret_key.is_empty() {
                    tracing::info!("🔐 Starting S3 credential save...");

                    let credentials = S3Credentials {
                        access_key: SecureString::new(access_key.clone()),
                        secret_key: SecureString::new(secret_key.clone()),
                        session_token: None,
                    };

                    let cred_dir_path = format!("{}/.rs-shield", home_str);
                    // Ensure credentials directory exists
                    let _ = ensure_directory_exists(&cred_dir_path);

                    let cred_file = home.join(".rs-shield").join("s3_credentials.enc");
                    let cred_file_str = cred_file.to_string_lossy().to_string();
                    tracing::info!("Caminho das credenciais: {}", cred_file_str);

                    match CredentialsManager::save_encrypted(&cred_file_str, &credentials, false) {
                        Ok(_) => {
                            tracing::info!(
                                "✅ S3 credentials saved securely at: {}",
                                cred_file_str
                            );
                        }
                        Err(e) => {
                            tracing::error!("❌ Failed to save credentials: {}", e);
                        }
                    }
                } else {
                    tracing::warn!("⚠️  S3 credentials NOT saved - conditions not met:");
                    if s3_config.is_none() {
                        tracing::warn!("   - S3 config not provided");
                    }
                    if access_key.is_empty() {
                        tracing::warn!("   - Access key is empty");
                    }
                    if secret_key.is_empty() {
                        tracing::warn!("   - Secret key is empty");
                    }
                }

                Ok(())
            }
            Err(e) => {
                tracing::error!("❌ Error saving to ~/.rsb-desktop/: {}", e);
                Err(format!("Error writing TOML file: {}", e))
            }
        }
    } else {
        // If it can't get home, fail clearly
        let error_msg = "❌ CRITICAL ERROR: Could not determine the user's HOME directory. Check environment variables (HOME/USERPROFILE).";
        tracing::error!("{}", error_msg);
        Err(error_msg.to_string())
    }
}
