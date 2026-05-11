use dioxus::prelude::*;
use std::fs;
use std::path::PathBuf;

/// Estrutura simplificada de um perfil
#[derive(Debug, Clone)]
pub struct ProfileEntry {
    pub name: String,
    pub path: PathBuf,
    pub created: String,
}

/// Profile Manager - Basic Operations
pub struct ProfileManager;

impl ProfileManager {
    /// Default profiles directory
    fn profiles_dir() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".rsb-desktop")
    }

    /// List all existing profiles
    pub fn list_profiles() -> Vec<ProfileEntry> {
        let profiles_dir = Self::profiles_dir();
        let mut profiles = Vec::new();

        if !profiles_dir.exists() {
            return profiles;
        }

        if let Ok(entries) = fs::read_dir(&profiles_dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file()
                        && entry.path().extension().is_some_and(|ext| ext == "toml")
                    {
                        let file_name = entry.file_name();
                        let name = file_name
                            .to_string_lossy()
                            .trim_end_matches(".toml")
                            .to_string();
                        let path = entry.path();

                        // Get modification date
                        let created = if let Ok(modified) = metadata.modified() {
                            if let Ok(duration) = modified.elapsed() {
                                format!("{} days ago", duration.as_secs() / 86400)
                            } else {
                                "Recently".to_string()
                            }
                        } else {
                            "Unknown".to_string()
                        };

                        profiles.push(ProfileEntry {
                            name,
                            path,
                            created,
                        });
                    }
                }
            }
        }

        // Sort by name
        profiles.sort_by(|a, b| a.name.cmp(&b.name));
        profiles
    }

    /// Delete a profile
    pub fn delete_profile(profile_path: &PathBuf) -> Result<(), String> {
        fs::remove_file(profile_path).map_err(|e| format!("Error deleting profile: {}", e))
    }
}

/// Profile Management Screen
#[component]
pub fn ProfileManagerScreen(mut active_tab: Signal<crate::ui::app::ActiveTab>) -> Element {
    let mut profiles = use_signal(ProfileManager::list_profiles);
    let mut status_msg = use_signal(String::new);
    let mut show_status = use_signal(|| false);
    let mut status_type = use_signal(|| "success"); // "success" or "error"
    let mut to_delete = use_signal(|| Option::<ProfileEntry>::None);

    let mut handle_delete = move |profile_name: String, profile_path: PathBuf| {
        match ProfileManager::delete_profile(&profile_path) {
            Ok(()) => {
                status_msg.set(format!(
                    "✅ Profile '{}' deleted successfully!",
                    profile_name
                ));
                status_type.set("success");
                to_delete.set(None);
                profiles.set(ProfileManager::list_profiles());
            }
            Err(e) => {
                status_msg.set(format!("❌ Error: {}", e));
                status_type.set("error");
            }
        }
        show_status.set(true);

        // Auto-hide after 3 seconds
        spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            show_status.set(false);
        });
    };

    let (bg_class, text_class) = if status_type() == "success" {
        (
            "bg-green-50 dark:bg-green-900/20 border-green-200 dark:border-green-800",
            "text-green-800 dark:text-green-300",
        )
    } else {
        (
            "bg-red-50 dark:bg-red-900/20 border-red-200 dark:border-red-800",
            "text-red-800 dark:text-red-300",
        )
    };

    rsx! {
        div { class: "flex flex-col h-full gap-6 p-6 bg-slate-50 dark:bg-slate-900",
            // Header
            div {
                h1 { class: "text-3xl font-bold text-slate-900 dark:text-slate-100 mb-2", "📋 Profile Management" }
                p { class: "text-slate-600 dark:text-slate-400",
                    "View and manage your backup profiles"
                }
            }

            // Status Message
            if show_status() {
                div { class: "p-4 rounded-lg {bg_class} border {text_class}",
                    p { "{status_msg}" }
                }
            }

            // Perfis List
            if profiles().is_empty() {
                div { class: "flex flex-col items-center justify-center flex-1 gap-4",
                    div { class: "text-6xl", "📭" }
                    p { class: "text-xl font-semibold text-slate-600 dark:text-slate-400", "No profiles created" }
                    p { class: "text-slate-500 dark:text-slate-500", "Create your first profile in the 'Create Profile' tab" }
                }
            } else {
                div { class: "grid gap-4 flex-1 overflow-y-auto",
                    for profile in profiles() {
                        div { class: "p-4 bg-white dark:bg-slate-800 rounded-lg border border-slate-200 dark:border-slate-700 shadow-sm",
                            div { class: "flex items-start justify-between gap-4",
                                div { class: "flex-1",
                                    h3 { class: "text-lg font-semibold text-slate-900 dark:text-slate-100", "📦 {profile.name}" }
                                    p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1", "Path: {profile.path.display()}" }
                                    p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1", "{profile.created}" }
                                }
                                button {
                                    class: "btn btn-sm btn-error",
                                    onclick: move |_| to_delete.set(Some(profile.clone())),
                                    "🗑️ Delete"
                                }
                            }
                        }
                    }
                }
            }

            // Delete Confirmation Modal
            if let Some(profile) = to_delete() {
                    div { class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
                        div { class: "bg-white dark:bg-slate-800 rounded-lg p-6 max-w-md shadow-xl",
                            h2 { class: "text-xl font-bold text-slate-900 dark:text-slate-100 mb-4",
                                "⚠️ Confirm Deletion"
                            }
                            p { class: "text-slate-600 dark:text-slate-400 mb-4",
                                "Are you sure you want to delete the profile \"{profile.name}\"? This action cannot be undone."
                            }
                            div { class: "flex gap-3 justify-end",
                                button {
                                    class: "btn btn-outline",
                                    onclick: move |_| to_delete.set(None),
                                    "Cancel"
                                }
                                button {
                                    class: "btn btn-error",
                                    onclick: move |_| handle_delete(profile.name.clone(), profile.path.clone()),
                                    "🗑️ Delete Permanently"
                                }
                            }
                        }
                    }
            }

            // Action Buttons
            div { class: "flex gap-4 pt-4 border-t border-slate-200 dark:border-slate-700",
                button {
                    class: "btn btn-success flex-1",
                    onclick: move |_| active_tab.set(crate::ui::app::ActiveTab::CreateProfile),
                    "➕ Create New Profile"
                }
                button {
                    class: "btn btn-primary flex-1",
                    onclick: move |_| profiles.set(ProfileManager::list_profiles()),
                    "🔄 Refresh"
                }
            }
        }
    }
}
