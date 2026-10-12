use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::ui::components::{
    Button, ButtonSize, ButtonVariant, Card, CardVariant, Input, InputVariant,
};
use crate::ui::{app::AppConfig, i18n::get_texts};
use rsb_sdk::credentials::SecureString;
use rsb_sdk::credentials::credentials_manager::{CredentialsManager, S3Credentials};
use rsb_sdk::utils::ensure_directory_exists;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct S3Config {
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
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
            Some(S3Config {
                bucket: if s3_bucket().is_empty() {
                    None
                } else {
                    Some(s3_bucket())
                },
                region: if s3_region().is_empty() {
                    None
                } else {
                    Some(s3_region())
                },
                endpoint: if s3_endpoint().is_empty() {
                    None
                } else {
                    Some(s3_endpoint())
                },
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
        use_memo(move || format!("Profile will be saved as: {}.toml", profile_name()));

    rsx! {
        div { class: "flex flex-col gap-4 p-4 bg-slate-50 dark:bg-slate-900",
            // Header
            div { class: "space-y-1",
                h1 { class: "text-lg font-semibold text-slate-900 dark:text-slate-100", "➕ Create Profile" }
                p { class: "text-sm text-slate-600 dark:text-slate-400", "Set up a new backup profile with your preferences" }
            }

            // Main form card
            Card { variant: CardVariant::Elevated,
                div { class: "space-y-4",
                    // Profile name
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

                    // Source directory
                    div { class: "space-y-2",
                        label { class: "text-sm font-semibold text-slate-700 dark:text-slate-300",
                            "{texts.source_dir_label}"
                        }
                        div { class: "flex gap-2",
                            input {
                                class: "flex-1 px-4 py-2 text-sm rounded-md border border-slate-300 dark:border-slate-600 bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-300 cursor-not-allowed",
                                r#type: "text",
                                placeholder: "Select folder",
                                value: "{source_path.read().to_string_lossy()}",
                                readonly: true
                            }
                            Button {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::SM,
                                disabled: is_loading(),
                                onclick: handle_select_source,
                                "📂 Select"
                            }
                        }
                        p { class: "text-xs text-slate-600 dark:text-slate-400",
                            "{texts.source_dir_hint}"
                        }
                    }

                    // Destination directory
                    div { class: "space-y-2",
                        label { class: "text-sm font-semibold text-slate-700 dark:text-slate-300",
                            "{texts.dest_dir_label}"
                        }
                        div { class: "flex gap-2",
                            input {
                                class: "flex-1 px-4 py-2 text-sm rounded-md border border-slate-300 dark:border-slate-600 bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-300 cursor-not-allowed",
                                r#type: "text",
                                placeholder: "Select folder",
                                value: "{dest_path.read().to_string_lossy()}",
                                readonly: true
                            }
                            Button {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::SM,
                                disabled: is_loading(),
                                onclick: handle_select_dest,
                                "📂 Select"
                            }
                        }
                        p { class: "text-xs text-slate-600 dark:text-slate-400",
                            "{texts.dest_dir_hint}"
                        }
                    }

                    // Exclude patterns (textarea)
                    div { class: "space-y-2",
                        label { class: "text-sm font-semibold text-slate-700 dark:text-slate-300",
                            "{texts.exclude_patterns_label}"
                        }
                        textarea {
                            class: "w-full px-3 py-2.5 text-sm rounded-md border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 text-slate-900 dark:text-slate-100 font-mono focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500 outline-none transition-all duration-150 resize-none",
                            placeholder: "*.tmp\nnode_modules\n.git\n.DS_Store\n__pycache__",
                            value: "{exclude_patterns}",
                            oninput: move |evt: Event<FormData>| exclude_patterns.set(evt.value()),
                            rows: "8"
                        }
                        p { class: "text-xs text-slate-600 dark:text-slate-400",
                            "One pattern per line. Wildcards: *.ext, Hidden: .folder, Folders: node_modules"
                        }
                    }
                }
            }

            // S3 Configuration (collapsible)
            Card { variant: CardVariant::Outlined,
                div { class: "space-y-4",
                    div {
                        class: "flex items-center justify-between cursor-pointer select-none hover:opacity-75 transition-opacity",
                        onclick: move |_| use_s3.set(!use_s3()),
                        h3 { class: "text-lg font-semibold text-slate-900 dark:text-slate-100", "☁️ S3 Configuration (Optional)" }
                        div {
                            class: if use_s3() { "text-indigo-600 dark:text-indigo-400 transform rotate-180 transition-transform" } else { "text-slate-400 dark:text-slate-500 transition-transform" },
                            "▼"
                        }
                    }

                    if use_s3() {
                        div { class: "space-y-4 pt-4 border-t border-slate-200 dark:border-slate-700",
                            Input {
                                label: "Bucket Name".to_string(),
                                placeholder: "my-backup-bucket".to_string(),
                                value: "{s3_bucket}",
                                oninput: Some(EventHandler::new(move |evt: FormEvent| s3_bucket.set(evt.value()))),
                            }

                            div { class: "grid grid-cols-2 gap-4",
                                Input {
                                    label: "Region".to_string(),
                                    placeholder: "us-east-1".to_string(),
                                    value: "{s3_region}",
                                    oninput: Some(EventHandler::new(move |evt: FormEvent| s3_region.set(evt.value()))),
                                }
                                Input {
                                    label: "Endpoint (Optional)".to_string(),
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
                                helper_text: "⚠️ Credentials saved in config. Use environment variables for better security.".to_string(),
                                oninput: Some(EventHandler::new(move |evt: FormEvent| s3_secret_key.set(evt.value()))),
                            }
                        }
                    } else {
                        p { class: "text-sm text-slate-600 dark:text-slate-400 pt-2",
                            "Click to configure remote backup to AWS S3, MinIO, Cloudflare R2, or compatible services."
                        }
                    }
                }
            }

            // Status message
            if !message().is_empty() {
                div {
                    class: if is_success() {
                        "flex items-start gap-3 p-4 rounded-lg bg-emerald-50 dark:bg-emerald-900/20 border-l-4 border-emerald-500"
                    } else {
                        "flex items-start gap-3 p-4 rounded-lg bg-red-50 dark:bg-red-900/20 border-l-4 border-red-500"
                    },
                    span { class: "text-xl flex-shrink-0 mt-0.5", if is_success() { "✅" } else { "❌" } }
                    p { class: "text-sm", "{message}" }
                }
            }

            // Action buttons
            div { class: "flex flex-col sm:flex-row gap-3",
                Button {
                    variant: ButtonVariant::Primary,
                    size: ButtonSize::MD,
                    loading: is_loading(),
                    disabled: is_loading() || profile_name().is_empty() || source_path().as_os_str().is_empty(),
                    class: "flex-1".to_string(),
                    onclick: handle_create,
                    "✨ Create Profile"
                }
            }

            // Helper text
            Card { variant: CardVariant::Filled,
                div { class: "space-y-2",
                    h3 { class: "font-semibold text-slate-900 dark:text-slate-100", "📝 Next Steps" }
                    p { class: "text-sm text-slate-600 dark:text-slate-400",
                        "After creating this profile, you'll be able to:"
                    }
                    ul { class: "list-disc list-inside text-sm text-slate-600 dark:text-slate-400 space-y-1",
                        li { "Run backups manually or on schedule" }
                        li { "Monitor real-time synchronization" }
                        li { "Manage snapshots and recovery" }
                        li { "Configure advanced settings" }
                    }
                }
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
        //max_threads: None,
        // channel_buffer_size: 8192,  // ⚡ Default buffer size
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
