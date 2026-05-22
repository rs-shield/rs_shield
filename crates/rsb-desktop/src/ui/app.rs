use std::sync::Arc;

use dioxus::prelude::*;
use dioxus_desktop::{LogicalSize, use_window};
use tokio::sync::Mutex;
use rsb_sdk::operation::operations::OperationsManager;
use rsb_sdk::metrics::system::{
        SystemMetrics, format_bytes_gb, format_percentage_bg, format_percentage_color,
        get_system_metrics,
    };

use crate::ui::{
    app_preferences::AppPreferences,
    backup_screen::BackupScreen,
    config_screen::ConfigScreen,
    create_profile_screen::CreateProfileScreen,
    fido2_manager_view::Fido2ManagerView,
    i18n::{Language, Theme, get_texts},
    integrations_screen::IntegrationScreen,
    login_screen::LoginScreen,
    profile_manager_screen::ProfileManagerScreen,
    prune_screen::PruneScreen,
    realtime_sync_screen::RealtimeSyncScreen,
    restore_screen::RestoreScreen,
    schedule_screen::ScheduleScreen,
    shared::TabButton,
    verify_screen::VerifyScreen,
};

#[derive(Clone, Copy, PartialEq)]
pub enum ActiveTab {
    CreateProfile,
    ProfileManager,
    Backup,
    Restore,
    Verify,
    Prune,
    Schedule,
    RealtimeSync,
    Fido2Manager,
    Integrations,
    Config,
}

#[derive(Clone, Copy)]
pub struct AppConfig {
    pub exclude_patterns: Signal<String>,
    pub s3_bucket: Signal<String>,
    pub s3_region: Signal<String>,
    pub s3_endpoint: Signal<String>,
    pub s3_access_key: Signal<String>,
    pub s3_secret_key: Signal<String>,
    pub backup_mode: Signal<String>,
    pub language: Signal<Language>,
    pub encrypt_patterns: Signal<String>,
    pub theme: Signal<Theme>,
    pub pause_on_low_battery: Signal<String>,
    pub pause_on_high_cpu: Signal<String>,
    pub compression_level: Signal<String>,
}

impl AppConfig {
    pub fn exclude_patterns(&self) -> String {
        (self.exclude_patterns)()
    }
    pub fn s3_bucket(&self) -> String {
        (self.s3_bucket)()
    }
    pub fn s3_region(&self) -> String {
        (self.s3_region)()
    }
    pub fn s3_endpoint(&self) -> String {
        (self.s3_endpoint)()
    }
    pub fn s3_access_key(&self) -> String {
        (self.s3_access_key)()
    }
    pub fn s3_secret_key(&self) -> String {
        (self.s3_secret_key)()
    }
    pub fn backup_mode(&self) -> String {
        (self.backup_mode)()
    }
    pub fn language(&self) -> Language {
        (self.language)()
    }
    pub fn theme(&self) -> Theme {
        (self.theme)()
    }
    pub fn encrypt_patterns(&self) -> String {
        (self.encrypt_patterns)()
    }
    pub fn pause_on_low_battery(&self) -> String {
        (self.pause_on_low_battery)()
    }
    pub fn pause_on_high_cpu(&self) -> String {
        (self.pause_on_high_cpu)()
    }
    pub fn compression_level(&self) -> String {
        (self.compression_level)()
    }
}

pub fn App() -> Element {
    let mut active_tab = use_signal(|| ActiveTab::Backup);
    let mut authenticated_user = use_signal(|| Option::<String>::None);

    let mut backup_count = use_signal(|| 0usize);
    let mut restore_count = use_signal(|| 0usize);
    let mut verify_count = use_signal(|| 0usize);
    let mut prune_count = use_signal(|| 0usize);
    let mut total_operations = use_signal(|| 0usize);
    let mut last_operation_time = use_signal(|| String::from("Nunca"));

    let mut system_metrics = use_signal(|| SystemMetrics {
        cpu_usage: 0.0,
        memory_usage: 0.0,
        memory_total_gb: 0.0,
        memory_used_gb: 0.0,
        disk_usage: 0.0,
        disk_total_gb: 0.0,
        disk_used_gb: 0.0,
        disk_free_gb: 0.0,
    });

    use_effect(move || {
        spawn(async move {
            let _ = rsb_sdk::operation::operations::ensure_history_directory();
            let manager = OperationsManager::new();
            let history = manager.get_history();

            backup_count.set(history.get_operations_count("Backup"));
            restore_count.set(history.get_operations_count("Restore"));
            verify_count.set(history.get_operations_count("Verify"));
            prune_count.set(history.get_operations_count("Prune"));
            total_operations.set(history.get_total_operations());
            last_operation_time.set(history.get_last_operation_time(None));
        });
    });

    use_effect(move || {
        spawn(async move {
            loop {
                let metrics = get_system_metrics();
                system_metrics.set(metrics);
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        });
    });

    let prefs = AppPreferences::load();

    let app_config = AppConfig {
        exclude_patterns: use_signal(|| prefs.exclude_patterns.clone()),
        s3_bucket: use_signal(String::new),
        s3_region: use_signal(String::new),
        s3_endpoint: use_signal(String::new),
        s3_access_key: use_signal(String::new),
        s3_secret_key: use_signal(String::new),
        backup_mode: use_signal(|| prefs.backup_mode.clone()),
        language: use_signal(|| prefs.language),
        theme: use_signal(|| prefs.theme),
        encrypt_patterns: use_signal(|| prefs.encrypt_patterns.clone()),
        pause_on_low_battery: use_signal(|| prefs.pause_on_low_battery.to_string()),
        pause_on_high_cpu: use_signal(|| prefs.pause_on_high_cpu.to_string()),
        compression_level: use_signal(|| prefs.compression_level.to_string()),
    };
    use_context_provider(|| app_config);

    use_effect(move || {
        let prefs = AppPreferences {
            language: app_config.language(),
            theme: app_config.theme(),
            exclude_patterns: app_config.exclude_patterns(),
            encrypt_patterns: app_config.encrypt_patterns(),
            backup_mode: app_config.backup_mode(),
            pause_on_low_battery: app_config.pause_on_low_battery().parse().unwrap_or(20),
            pause_on_high_cpu: app_config.pause_on_high_cpu().parse().unwrap_or(90),
            compression_level: app_config.compression_level().parse().unwrap_or(3),
        };

        let _ = prefs.save();
    });

    let texts = get_texts(*app_config.language.read());

    let window = use_window();
    let mut started = use_signal(|| false);

    let fido2_manager_arc = use_context_provider(|| {
        let origin = "http://localhost:3000";
        let rp_id = "localhost";
        let mut mgr = rsb_sdk::credentials::Fido2Manager::new(origin, rp_id, "RSB Shield Desktop")
            .expect("Failed to init FIDO2");

        if let Ok(path) = rsb_sdk::credentials::Fido2Manager::default_storage_path() {
            if path.exists() {
                let _ = mgr.load_from_file(&path);
            }
        }
        Arc::new(Mutex::new(mgr))
    });

    let is_logged_in = authenticated_user.read().is_some();

    use_context_provider(|| authenticated_user);

    let mut logout = move |_| {
        authenticated_user.set(None);
        active_tab.set(ActiveTab::Backup);
    };

    use_effect(move || {
        if !started() {
            window.set_inner_size(LogicalSize::new(1280.0, 900.0));
            started.set(true);
        }
    });

    let theme_class = use_memo(move || match app_config.theme() {
        Theme::Dark => "dark",
        Theme::Light => "",
        Theme::System => "",
    });

    rsx! {
        style { "{include_str!(\"./styles.css\")}" }


        div { class: "flex h-screen w-screen overflow-hidden bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100 {theme_class} transition-colors duration-200 antialiased",



            aside { class: "w-72 bg-white dark:bg-slate-900 border-r border-slate-200 dark:border-slate-800 flex flex-col p-6 flex-shrink-0 z-10 shadow-sm",


                div { class: "flex items-center gap-3 pb-6 mb-2 border-b border-slate-100 dark:border-slate-800 select-none",
                    span { class: "text-2xl filter drop-shadow-sm flex-shrink-0", "🛡️" }
                    div { class: "flex flex-col min-w-0",
                        span { class: "text-base font-bold tracking-tight text-slate-900 dark:text-white truncate", "RSB Shield" }
                        span { class: "text-[11px] text-slate-400 dark:text-slate-500 font-medium truncate", "Secure Backup Solutions" }
                    }
                }

                if is_logged_in {
                    Fragment {

                        nav { class: "space-y-1 flex-1 overflow-y-auto pt-4 pr-1 -mr-2 custom-scrollbar",

                            h5 { class: "text-[10px] font-bold text-slate-400 dark:text-slate-500 uppercase tracking-wider px-3 mb-2", "Gestão" }
                            TabButton { label: "Criar Perfil".to_string(), icon: "📝", active: *active_tab.read() == ActiveTab::CreateProfile, onclick: move |_| active_tab.set(ActiveTab::CreateProfile) }
                            TabButton { label: "Gerenciar Perfis".to_string(), icon: "📋", active: *active_tab.read() == ActiveTab::ProfileManager, onclick: move |_| active_tab.set(ActiveTab::ProfileManager) }

                            h5 { class: "text-[10px] font-bold text-slate-400 dark:text-slate-500 uppercase tracking-wider px-3 mt-6 mb-2", "Operações" }
                            TabButton { label: texts.nav_backup.to_string(), icon: "📦", active: *active_tab.read() == ActiveTab::Backup, onclick: move |_| active_tab.set(ActiveTab::Backup) }
                            TabButton { label: texts.nav_restore.to_string(), icon: "🔄", active: *active_tab.read() == ActiveTab::Restore, onclick: move |_| active_tab.set(ActiveTab::Restore) }
                            TabButton { label: texts.nav_verify.to_string(), icon: "🔍", active: *active_tab.read() == ActiveTab::Verify, onclick: move |_| active_tab.set(ActiveTab::Verify) }
                            TabButton { label: texts.nav_prune.to_string(), icon: "✂️", active: *active_tab.read() == ActiveTab::Prune, onclick: move |_| active_tab.set(ActiveTab::Prune) }
                            TabButton { label: "Real-Time Sync".to_string(), icon: "💾", active: *active_tab.read() == ActiveTab::RealtimeSync, onclick: move |_| active_tab.set(ActiveTab::RealtimeSync) }

                            h5 { class: "text-[10px] font-bold text-slate-400 dark:text-slate-500 uppercase tracking-wider px-3 mt-6 mb-2", "Sistema" }
                            TabButton { label: texts.nav_schedule.to_string(), icon: "🕒", active: *active_tab.read() == ActiveTab::Schedule, onclick: move |_| active_tab.set(ActiveTab::Schedule) }
                            TabButton { label: texts.nav_fido2.to_string(), icon: "🔑", active: *active_tab.read() == ActiveTab::Fido2Manager, onclick: move |_| active_tab.set(ActiveTab::Fido2Manager) }
                            TabButton { label: "Integrações".to_string(), icon: "🔗", active: *active_tab.read() == ActiveTab::Integrations, onclick: move |_| active_tab.set(ActiveTab::Integrations) }
                            TabButton { label: texts.nav_config.to_string(), icon: "⚙️", active: *active_tab.read() == ActiveTab::Config, onclick: move |_| active_tab.set(ActiveTab::Config) }
                        }


                        div { class: "mt-auto pt-4 border-t border-slate-100 dark:border-slate-800",
                            div { class: "flex items-center gap-3 px-3 py-3 mb-3 bg-slate-50 dark:bg-slate-800/40 rounded-xl border border-slate-200/60 dark:border-slate-800/60",
                                div { class: "w-8 h-8 rounded-full bg-indigo-50 dark:bg-indigo-950 flex items-center justify-center text-sm border border-indigo-100 dark:border-indigo-900/30 flex-shrink-0", "👤" }
                                if let Some(user) = authenticated_user.read().as_ref() {
                                    div { class: "flex-1 min-w-0",
                                        p { class: "text-xs font-semibold text-slate-900 dark:text-slate-200 truncate", "{user}" }
                                        p { class: "text-[10px] font-medium text-emerald-600 dark:text-emerald-400 flex items-center gap-1",
                                            span { class: "w-1 h-1 rounded-full bg-emerald-500 inline-block animate-pulse" }
                                            "Sessão Ativa"
                                        }
                                    }
                                }
                            }
                            button {
                                class: "w-full flex items-center justify-center gap-2 px-3 py-2.5 text-xs font-semibold text-red-600 hover:text-white bg-white dark:bg-slate-900 hover:bg-red-600 dark:hover:bg-red-600 rounded-xl border border-red-200 dark:border-red-900/40 transition-all duration-150 active:scale-[0.98]",
                                onclick: logout,
                                span { "🚪" }
                                span { "{texts.logout_button}" }
                            }
                        }
                    }
                } else {
                    div { class: "flex-1 flex flex-col items-center justify-center text-center p-4 bg-slate-50 dark:bg-slate-800/20 rounded-2xl border border-dashed border-slate-200 dark:border-slate-700 my-4",
                        span { class: "text-3xl mb-3 opacity-60", "🔒" }
                        p { class: "text-xs font-medium text-slate-400 dark:text-slate-500 px-2", "{texts.auth_required_msg}" }
                    }
                }
            }



            main { class: "flex flex-1 overflow-hidden bg-slate-50 dark:bg-slate-950",
                div { class: "flex-1 overflow-y-auto px-10 py-8 custom-scrollbar",
                    div { class: "max-w-4xl mx-auto",
                        if is_logged_in {

                            div { class: "mb-6",
                                h1 { class: "text-2xl font-extrabold tracking-tight text-slate-900 dark:text-white", "{texts.control_panel_title}" }
                                p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1", "{texts.control_panel_subtitle}" }
                            }

                            div { class: "bg-white dark:bg-slate-900 rounded-2xl border border-slate-200/80 dark:border-slate-800 p-6 shadow-sm",
                                match *active_tab.read() {
                                    ActiveTab::CreateProfile => rsx! { CreateProfileScreen {} },
                                    ActiveTab::ProfileManager => rsx! { ProfileManagerScreen { active_tab } },
                                    ActiveTab::Backup => rsx! { BackupScreen {} },
                                    ActiveTab::Restore => rsx! { RestoreScreen {} },
                                    ActiveTab::Verify => rsx! { VerifyScreen {} },
                                    ActiveTab::Prune => rsx! { PruneScreen {} },
                                    ActiveTab::RealtimeSync => rsx! { RealtimeSyncScreen {} },
                                    ActiveTab::Schedule => rsx! { ScheduleScreen {} },
                                    ActiveTab::Fido2Manager => rsx! { Fido2ManagerView {} },
                                    ActiveTab::Integrations => rsx! { IntegrationScreen {} },
                                    ActiveTab::Config => rsx! { ConfigScreen {} },
                                }
                            }
                        } else {
                            div { class: "flex items-center justify-center min-h-[75vh]",
                                LoginScreen { on_login: move |user: String| authenticated_user.set(Some(user)) }
                            }
                        }
                    }
                }


                aside { class: "w-80 bg-white dark:bg-slate-900 border-l border-slate-200 dark:border-slate-800 flex flex-col overflow-hidden flex-shrink-0 shadow-sm",


                    div { class: "p-6 border-b border-slate-100 dark:border-slate-800",
                        h3 { class: "text-xs font-bold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-1", "{texts.reports_title}" }
                        p { class: "text-sm font-semibold text-slate-800 dark:text-slate-200 flex items-center gap-2",
                            span { class: "w-2 h-2 rounded-full bg-emerald-500 inline-block animate-pulse" }
                            "{texts.real_time_label}"
                        }
                    }


                    div { class: "flex-1 overflow-y-auto p-6 space-y-5 custom-scrollbar",


                        div { class: "bg-slate-50/70 dark:bg-slate-800/30 border border-slate-200/60 dark:border-slate-800/60 rounded-xl p-4",
                            h4 { class: "font-bold text-[11px] text-slate-400 dark:text-slate-500 uppercase tracking-wider mb-4 flex items-center gap-2",
                                span { "💻" }
                                "{texts.system_title}"
                            }


                            div { class: "mb-4",
                                div { class: "flex justify-between items-center mb-1.5",
                                    span { class: "text-xs font-medium text-slate-600 dark:text-slate-400", "Utilização CPU" }
                                    span { class: "text-xs font-bold font-mono {format_percentage_color(system_metrics().cpu_usage)}", "{system_metrics().cpu_usage:.0}%" }
                                }
                                div { class: "w-full bg-slate-200 dark:bg-slate-800 rounded-full h-1.5 overflow-hidden",
                                    div {
                                        class: "h-full rounded-full bg-gradient-to-r from-blue-500 to-indigo-500 transition-all duration-300",
                                        style: "width: {system_metrics().cpu_usage}%"
                                    }
                                }
                            }


                            div { class: "mb-4",
                                title: "{texts.used_label}: {format_bytes_gb(system_metrics().memory_used_gb)} / {texts.total_label}: {format_bytes_gb(system_metrics().memory_total_gb)}",
                                div { class: "flex flex-col mb-1.5 gap-0.5",
                                    div { class: "flex justify-between items-center",
                                        span { class: "text-xs font-medium text-slate-600 dark:text-slate-400", "Memória RAM" }
                                        span { class: "text-xs font-bold font-mono {format_percentage_color(system_metrics().memory_usage)}", "{system_metrics().memory_usage:.0}%" }
                                    }
                                    span { class: "text-[10px] text-slate-400 font-mono text-right", "({system_metrics().memory_used_gb:.1}/{system_metrics().memory_total_gb:.1} GB)" }
                                }
                                div { class: "w-full bg-slate-200 dark:bg-slate-800 rounded-full h-1.5 overflow-hidden",
                                    div {
                                        class: "h-full rounded-full bg-gradient-to-r from-purple-500 to-pink-500 transition-all duration-300",
                                        style: "width: {system_metrics().memory_usage}%"
                                    }
                                }
                            }


                            div {
                                title: "{texts.used_label}: {format_bytes_gb(system_metrics().disk_used_gb)} / {texts.total_label}: {format_bytes_gb(system_metrics().disk_total_gb)}",
                                div { class: "flex flex-col mb-1.5 gap-0.5",
                                    div { class: "flex justify-between items-center",
                                        span { class: "text-xs font-medium text-slate-600 dark:text-slate-400", "Disco Local" }
                                        span { class: "text-xs font-bold font-mono {format_percentage_color(system_metrics().disk_usage)}", "{system_metrics().disk_usage:.0}%" }
                                    }
                                    span { class: "text-[10px] text-slate-400 font-mono text-right", "({system_metrics().disk_used_gb:.1}/{system_metrics().disk_total_gb:.1} GB)" }
                                }
                                div { class: "w-full bg-slate-200 dark:bg-slate-800 rounded-full h-1.5 overflow-hidden",
                                    div {
                                        class: "h-full rounded-full bg-gradient-to-r from-amber-500 to-orange-500 transition-all duration-300",
                                        style: "width: {system_metrics().disk_usage}%"
                                    }
                                }
                            }
                        }


                        div { class: "bg-white dark:bg-slate-900 border border-slate-200/80 dark:border-slate-800 rounded-xl p-4 shadow-sm",
                            h4 { class: "font-bold text-[11px] text-emerald-600 dark:text-emerald-500 uppercase tracking-wider mb-3 flex items-center gap-2",
                                span { "📈" }
                                "{texts.activity_title}"
                            }
                            div { class: "space-y-2.5",
                                div { class: "flex justify-between items-center bg-slate-50 dark:bg-slate-800/40 p-2 rounded-lg border border-slate-100 dark:border-slate-800/40",
                                    span { class: "text-xs font-medium text-slate-500 dark:text-slate-400", "{texts.total_ops_label}" }
                                    span { class: "text-xs font-bold text-slate-900 dark:text-slate-200 font-mono bg-white dark:bg-slate-800 px-2 py-0.5 rounded border border-slate-200/40 dark:border-slate-700/40 shadow-2xs", "{total_operations}" }
                                }
                                div { class: "flex justify-between items-center px-1",
                                    span { class: "text-xs font-medium text-slate-500 dark:text-slate-400", "{texts.last_op_label}" }
                                    span { class: "text-xs font-semibold text-emerald-600 dark:text-emerald-400 font-mono", "{last_operation_time}" }
                                }
                            }
                        }


                        div { class: "bg-white dark:bg-slate-900 border border-slate-200/80 dark:border-slate-800 rounded-xl p-4 shadow-sm",
                            h4 { class: "font-bold text-[11px] text-indigo-600 dark:text-indigo-500 uppercase tracking-wider mb-3 flex items-center gap-2",
                                span { "🗃️" }
                                "{texts.stats_title}"
                            }
                            div { class: "grid grid-cols-2 gap-2.5",
                                div { class: "bg-slate-50 dark:bg-slate-800/50 rounded-lg p-2.5 border border-slate-100 dark:border-slate-800/40",
                                    p { class: "text-[10px] text-slate-400 dark:text-slate-500 font-bold truncate uppercase tracking-wider", "{texts.backups_count_label}" }
                                    p { class: "text-base font-bold text-indigo-600 dark:text-indigo-400 mt-0.5 font-mono", "{backup_count}" }
                                }
                                div { class: "bg-slate-50 dark:bg-slate-800/50 rounded-lg p-2.5 border border-slate-100 dark:border-slate-800/40",
                                    p { class: "text-[10px] text-slate-400 dark:text-slate-500 font-bold truncate uppercase tracking-wider", "{texts.restores_count_label}" }
                                    p { class: "text-base font-bold text-purple-600 dark:text-purple-400 mt-0.5 font-mono", "{restore_count}" }
                                }
                                div { class: "bg-slate-50 dark:bg-slate-800/50 rounded-lg p-2.5 border border-slate-100 dark:border-slate-800/40",
                                    p { class: "text-[10px] text-slate-400 dark:text-slate-500 font-bold truncate uppercase tracking-wider", "{texts.verifies_count_label}" }
                                    p { class: "text-base font-bold text-blue-600 dark:text-blue-400 mt-0.5 font-mono", "{verify_count}" }
                                }
                                div { class: "bg-slate-50 dark:bg-slate-800/50 rounded-lg p-2.5 border border-slate-100 dark:border-slate-800/40",
                                    p { class: "text-[10px] text-slate-400 dark:text-slate-500 font-bold truncate uppercase tracking-wider", "{texts.prunes_count_label}" }
                                    p { class: "text-base font-bold text-orange-600 dark:text-orange-400 mt-0.5 font-mono", "{prune_count}" }
                                }
                            }
                        }


                        div { class: "bg-emerald-50/50 dark:bg-emerald-950/20 border border-emerald-100 dark:border-emerald-900/60 rounded-xl p-4 flex items-center gap-3",
                            span { class: "text-sm w-6 h-6 rounded-full bg-white dark:bg-emerald-900 flex items-center justify-center shadow-2xs text-emerald-500 flex-shrink-0 font-bold", "✓" }
                            div { class: "min-w-0",
                                h5 { class: "font-bold text-xs text-emerald-950 dark:text-emerald-300 uppercase tracking-wide", "{texts.alerts_title}" }
                                p { class: "text-xs text-slate-600 dark:text-emerald-400/80 truncate font-medium", "{texts.system_ok_msg}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
