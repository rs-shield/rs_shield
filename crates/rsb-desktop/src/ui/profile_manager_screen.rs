use dioxus::prelude::*;
use std::fs;
use std::path::PathBuf;

use crate::ui::app::AppConfig;
use crate::ui::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, CardVariant, EmptyState,
};
use crate::ui::error_handler::format_user_error;
use crate::ui::i18n::{Translations, get_texts, tr};

/// Estrutura simplificada de um perfil
#[derive(Debug, Clone)]
pub struct ProfileEntry {
    pub name: String,
    pub path: PathBuf,
    pub created: ProfileAge,
}

/// Age of a profile file, rendered in the current language.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProfileAge {
    Days(u64),
    Recent,
    Unknown,
}

impl ProfileAge {
    pub fn label(self, t: &Translations) -> String {
        match self {
            ProfileAge::Days(d) => tr(t.pm_days_ago, &[&d]),
            ProfileAge::Recent => t.pm_recently.to_string(),
            ProfileAge::Unknown => t.pm_unknown.to_string(),
        }
    }
}

pub struct ProfileManager;

impl ProfileManager {
    fn profiles_dir() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".rsb-desktop")
    }

    pub fn list_profiles() -> Vec<ProfileEntry> {
        let profiles_dir = Self::profiles_dir();
        let mut profiles = Vec::new();

        if !profiles_dir.exists() {
            return profiles;
        }

        if let Ok(entries) = fs::read_dir(&profiles_dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata()
                    && metadata.is_file()
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
                            ProfileAge::Days(duration.as_secs() / 86400)
                        } else {
                            ProfileAge::Recent
                        }
                    } else {
                        ProfileAge::Unknown
                    };

                    profiles.push(ProfileEntry {
                        name,
                        path,
                        created,
                    });
                }
            }
        }

        // Sort by name
        profiles.sort_by(|a, b| a.name.cmp(&b.name));
        profiles
    }

    pub fn delete_profile(t: &Translations, profile_path: &PathBuf) -> Result<(), String> {
        fs::remove_file(profile_path).map_err(|e| tr(t.pm_error_deleting, &[&e]))
    }
}

/// Profile Management Screen
#[component]
pub fn ProfileManagerScreen(mut active_tab: Signal<crate::ui::app::ActiveTab>) -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut profiles = use_signal(ProfileManager::list_profiles);
    let mut status_msg = use_signal(String::new);
    let mut show_status = use_signal(|| false);
    let mut status_type = use_signal(|| "success"); // "success" or "error"
    let mut to_delete = use_signal(|| Option::<ProfileEntry>::None);

    let mut handle_delete = move |profile_name: String, profile_path: PathBuf| {
        match ProfileManager::delete_profile(texts, &profile_path) {
            Ok(()) => {
                status_msg.set(tr(texts.pm_deleted, &[&profile_name]));
                status_type.set("success");
                to_delete.set(None);
                profiles.set(ProfileManager::list_profiles());
            }
            Err(e) => {
                status_msg.set(format_user_error(texts, e, "profile"));
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
    let (status_bg_class, status_icon) = if status_type() == "success" {
        (
            "bg-emerald-50 dark:bg-emerald-900/20 border-emerald-500",
            "✅",
        )
    } else {
        ("bg-red-50 dark:bg-red-900/20 border-red-500", "❌")
    };
    rsx! {
        div { class: "flex flex-col h-full gap-4 p-4 bg-slate-50 dark:bg-slate-900",
            // Header
            div { class: "space-y-2",
                h1 { class: "text-2xl font-bold text-slate-900 dark:text-slate-100", "{texts.pm_title}" }
                p { class: "text-slate-600 dark:text-slate-400", "{texts.pm_subtitle}" }
            }

            // Status message
            if show_status() {
                div {
                    class: format!("flex items-start gap-3 p-4 rounded-lg border-l-4 {}", status_bg_class),

                    span {
                        class: "text-xl flex-shrink-0",
                        "{status_icon}"
                    }

                    p {
                        class: "text-sm text-slate-700 dark:text-slate-300",
                        "{status_msg}"
                    }
                }
            }

            // Profiles list or empty state
            if profiles().is_empty() {
                Card { variant: CardVariant::Outlined, class: "flex-1".to_string(),
                    EmptyState {
                        icon: "📭".to_string(),
                        title: texts.pm_empty_title.to_string(),
                        description: texts.pm_empty_description.to_string(),
                        action_label: None,
                        on_action: None,
                    }
                }
            } else {
                div { class: "grid gap-4 flex-1 overflow-y-auto",
                    for profile in profiles() {
                        Card { variant: CardVariant::Elevated,
                            div { class: "flex items-start justify-between gap-4",
                                div { class: "flex-1 space-y-2",
                                    div { class: "flex items-center gap-2",
                                        span { class: "text-base", "📦" }
                                        h3 { class: "text-base font-semibold text-slate-900 dark:text-slate-100", "{profile.name}" }
                                    }
                                    p { class: "text-xs text-slate-600 dark:text-slate-400", "{texts.pm_path_label} {profile.path.display()}" }
                                    Badge { variant: BadgeVariant::Neutral, "{profile.created.label(texts)}" }
                                }
                                Button {
                                    variant: ButtonVariant::Danger,
                                    size: ButtonSize::SM,
                                    onclick: move |_| to_delete.set(Some(profile.clone())),
                                    "{texts.pm_delete}"
                                }
                            }
                        }
                    }
                }
            }

            // Delete confirmation modal
            if let Some(profile) = to_delete() {
                div { class: "fixed inset-0 bg-black/20 dark:bg-black/50 backdrop-blur-sm flex items-center justify-center z-50",
                    Card { variant: CardVariant::Elevated, class: "max-w-md w-full mx-4".to_string(),
                        div { class: "space-y-4",
                            div { class: "space-y-2",
                                h2 { class: "text-xl font-bold text-slate-900 dark:text-slate-100 flex items-center gap-2",
                                    span { "⚠️" }
                                    "{texts.pm_confirm_title}"
                                }
                                p { class: "text-slate-600 dark:text-slate-400",
                                    "{tr(texts.pm_confirm_message, &[&profile.name])}"
                                }
                            }

                            div { class: "flex flex-col sm:flex-row gap-3",
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    size: ButtonSize::MD,
                                    class: "flex-1".to_string(),
                                    onclick: move |_| to_delete.set(None),
                                    "{texts.pm_cancel}"
                                }
                                Button {
                                    variant: ButtonVariant::Danger,
                                    size: ButtonSize::MD,
                                    class: "flex-1".to_string(),
                                    onclick: move |_| handle_delete(profile.name.clone(), profile.path.clone()),
                                    "{texts.pm_delete}"
                                }
                            }
                        }
                    }
                }
            }

            // Footer actions
            div { class: "flex flex-col sm:flex-row gap-3 pt-4 border-t border-slate-200 dark:border-slate-700",
                Button {
                    variant: ButtonVariant::Success,
                    size: ButtonSize::MD,
                    class: "flex-1".to_string(),
                    onclick: move |_| active_tab.set(crate::ui::app::ActiveTab::CreateProfile),
                    "{texts.pm_create_new}"
                }
                Button {
                    variant: ButtonVariant::Secondary,
                    size: ButtonSize::MD,
                    class: "flex-1".to_string(),
                    onclick: move |_| profiles.set(ProfileManager::list_profiles()),
                    "{texts.pm_refresh}"
                }
            }
        }
    }
}
