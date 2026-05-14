use std::sync::Arc;

use dioxus::prelude::*;
use dioxus_desktop::{LogicalSize, use_window};
use tokio::sync::Mutex;

use crate::ui::{
    app_preferences::AppPreferences,
    backup_screen::BackupScreen,
    config_screen::ConfigScreen,
    create_profile_screen::CreateProfileScreen,
    fido2_manager_view::Fido2ManagerView,
    i18n::{Language, Theme, get_texts},
    integrations_screen::IntegrationScreen,
    login_screen::LoginScreen,
    metrics::{
        SystemMetrics, format_bytes_gb, format_percentage_bg, format_percentage_color,
        get_system_metrics,
    },
    operations::OperationsManager,
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
            let _ = crate::ui::operations::ensure_history_directory();
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

    // Carregar preferências salvas
    let prefs = AppPreferences::load();

    let app_config = AppConfig {
        exclude_patterns: use_signal(|| prefs.exclude_patterns.clone()),
        s3_bucket: use_signal(String::new),
        s3_region: use_signal(String::new),
        s3_endpoint: use_signal(String::new),
        s3_access_key: use_signal(String::new),
        s3_secret_key: use_signal(String::new),
        backup_mode: use_signal(|| prefs.backup_mode.clone()),
        language: use_signal(|| AppPreferences::string_to_language(&prefs.language)),
        theme: use_signal(|| AppPreferences::string_to_theme(&prefs.theme)),
        encrypt_patterns: use_signal(|| prefs.encrypt_patterns.clone()),
        pause_on_low_battery: use_signal(|| prefs.pause_on_low_battery.clone()),
        pause_on_high_cpu: use_signal(|| prefs.pause_on_high_cpu.clone()),
        compression_level: use_signal(|| prefs.compression_level.clone()),
    };
    use_context_provider(|| app_config);

    // Auto-salvar preferências quando mudam
    use_effect(move || {
        let prefs = AppPreferences {
            language: AppPreferences::language_to_string(app_config.language()),
            theme: AppPreferences::theme_to_string(app_config.theme()),
            exclude_patterns: app_config.exclude_patterns(),
            encrypt_patterns: app_config.encrypt_patterns(),
            backup_mode: app_config.backup_mode(),
            pause_on_low_battery: app_config.pause_on_low_battery(),
            pause_on_high_cpu: app_config.pause_on_high_cpu(),
            compression_level: app_config.compression_level(),
        };

        let _ = prefs.save();
    });

    let texts = get_texts(*app_config.language.read());

    let window = use_window();
    let mut started = use_signal(|| false);

    // Inicializa Fido2Manager uma única vez e o fornece como contexto
    let fido2_manager_arc = use_context_provider(|| {
        let origin = "http://localhost:3000"; // Ou a origem apropriada para o desktop
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

    // Se não estiver logado, forçamos a aba de login (virtualmente) ou mostramos o LoginScreen
    let is_logged_in = authenticated_user.read().is_some();

    // Função de Logout
    let mut logout = move |_| {
        authenticated_user.set(None);
        active_tab.set(ActiveTab::Backup);
    };

    use_effect(move || {
        if !started() {
            window.set_inner_size(LogicalSize::new(1200, 700));
            started.set(true);
        }
    });

    let theme_class = use_memo(move || match app_config.theme() {
        Theme::Dark => "dark",
        Theme::Light => "",
        Theme::System => "", // Handled by media query
    });

    rsx! {
        style { {include_str!("./styles.css")} }

        div { class: "flex h-screen w-screen overflow-hidden bg-slate-50 dark:bg-slate-900 {theme_class}",
            aside { class: "w-64 bg-white dark:bg-slate-800 border-r border-slate-200 dark:border-slate-700 flex flex-col p-6 flex-shrink-0 shadow-lg",
                div { class: "brand",
                    span { style: "font-size: 1.75rem;", "🛡️" }
                    span { "RSB Shield" }
                }
                if is_logged_in {
                    nav { class: "space-y-1 flex-1",
                        TabButton { label: "Criar Perfil".to_string(), icon: "📝", active: *active_tab.read() == ActiveTab::CreateProfile, onclick: move |_| active_tab.set(ActiveTab::CreateProfile) }
                        TabButton { label: "Gerenciar Perfis".to_string(), icon: "📋", active: *active_tab.read() == ActiveTab::ProfileManager, onclick: move |_| active_tab.set(ActiveTab::ProfileManager) }
                        TabButton { label: texts.nav_backup.to_string(), icon: "📦", active: *active_tab.read() == ActiveTab::Backup, onclick: move |_| active_tab.set(ActiveTab::Backup) }
                        TabButton { label: texts.nav_restore.to_string(), icon: "🔄", active: *active_tab.read() == ActiveTab::Restore, onclick: move |_| active_tab.set(ActiveTab::Restore) }
                        TabButton { label: texts.nav_verify.to_string(), icon: "🔍", active: *active_tab.read() == ActiveTab::Verify, onclick: move |_| active_tab.set(ActiveTab::Verify) }
                        TabButton { label: texts.nav_prune.to_string(), icon: "✂️", active: *active_tab.read() == ActiveTab::Prune, onclick: move |_| active_tab.set(ActiveTab::Prune) }
                        TabButton { label: "Real-Time Sync".to_string(), icon: "💾", active: *active_tab.read() == ActiveTab::RealtimeSync, onclick: move |_| active_tab.set(ActiveTab::RealtimeSync) }
                        TabButton { label: texts.nav_schedule.to_string(), icon: "🕒", active: *active_tab.read() == ActiveTab::Schedule, onclick: move |_| active_tab.set(ActiveTab::Schedule) }
                        TabButton { label: texts.nav_fido2.to_string(), icon: "🔑", active: *active_tab.read() == ActiveTab::Fido2Manager, onclick: move |_| active_tab.set(ActiveTab::Fido2Manager) }
                        TabButton { label: "Integrações".to_string(), icon: "🔗", active: *active_tab.read() == ActiveTab::Integrations, onclick: move |_| active_tab.set(ActiveTab::Integrations) }
                        TabButton { label: texts.nav_config.to_string(), icon: "⚙️", active: *active_tab.read() == ActiveTab::Config, onclick: move |_| active_tab.set(ActiveTab::Config) }
                    }
                    div { class: "mt-auto pt-4 border-t border-slate-200 dark:border-slate-700",
                        div { class: "flex items-center gap-2 px-2 py-3 mb-2",
                            div { class: "w-8 h-8 rounded-full bg-indigo-100 dark:bg-indigo-900/30 flex items-center justify-center text-indigo-600", "👤" }
                            div { class: "flex-1 overflow-hidden",
                                p { class: "text-xs font-bold truncate dark:text-white", "{authenticated_user.read().as_ref().unwrap()}" }
                                p { class: "text-[10px] text-slate-500", " Authenticated" }
                            }
                        }
                        button {
                            class: "w-full flex items-center gap-2 px-3 py-2 text-sm text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg transition-colors",
                            onclick: logout,
                            span { "🚪" }
                            span { "{texts.logout_button}" }
                        }
                    }
                } else {
                    div { class: "flex-1 flex flex-col items-center justify-center text-center p-4",
                        span { class: "text-4xl mb-4", "🔒" }
                        p { class: "text-sm text-slate-500", "{texts.auth_required_msg}" }
                    }
                }
            }

            main { class: "flex flex-1 overflow-hidden bg-slate-50 dark:bg-slate-900",
                div { class: "flex-1 overflow-y-auto p-8",
                    div { class: "max-w-4xl",
                        if is_logged_in {
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
                        } else {
                            LoginScreen { on_login: move |user: String| authenticated_user.set(Some(user)) }

                        }
                    }
                }

                aside { class: "w-80 bg-white dark:bg-slate-800 border-l border-slate-200 dark:border-slate-700 flex flex-col overflow-hidden shadow-lg",
                    div { class: "p-6 border-b border-slate-200 dark:border-slate-700",
                        h3 { class: "text-lg font-bold text-slate-900 dark:text-white mb-1", "{texts.reports_title}" }
                        p { class: "text-sm text-slate-500 dark:text-slate-400", "{texts.real_time_label}" }
                    }

                    div { class: "flex-1 overflow-y-auto p-6 space-y-4",
                        div { class: "bg-gradient-to-br from-slate-50 to-slate-100 dark:from-slate-800 dark:to-slate-700 border border-slate-200 dark:border-slate-600 rounded-lg p-4 shadow-sm",
                            h4 { class: "font-bold text-slate-900 dark:text-white mb-3 text-sm", "{texts.system_title}" }

                            div { class: "flex items-center gap-2 mb-2",
                                span { class: "text-xs font-medium text-slate-600 dark:text-slate-300 w-8", "CPU" }
                                div { class: "flex-1 bg-slate-200 dark:bg-slate-600 rounded-full h-1.5 overflow-hidden",
                                    div {
                                        class: "h-full bg-gradient-to-r from-blue-400 to-blue-600 dark:from-blue-500 dark:to-blue-700",
                                        style: "width: {system_metrics().cpu_usage}%"
                                    }
                                }
                                span { class: "text-xs font-bold {format_percentage_color(system_metrics().cpu_usage)} text-right whitespace-nowrap", "{system_metrics().cpu_usage:.0}%" }
                            }

                            div { class: "flex items-center gap-2 mb-2",
                                title: "{texts.used_label}: {format_bytes_gb(system_metrics().memory_used_gb)} / {texts.total_label}: {format_bytes_gb(system_metrics().memory_total_gb)}",
                                span { class: "text-xs font-medium text-slate-600 dark:text-slate-300 w-8", "RAM" }
                                div { class: "flex-1 {format_percentage_bg(system_metrics().memory_usage)} rounded-full h-1.5 overflow-hidden",
                                    div {
                                        class: "h-full bg-gradient-to-r from-purple-400 to-purple-600 dark:from-purple-500 dark:to-purple-700",
                                        style: "width: {system_metrics().memory_usage}%"
                                    }
                                }
                                span { class: "text-xs font-bold {format_percentage_color(system_metrics().memory_usage)} text-right whitespace-nowrap", "{system_metrics().memory_usage:.0}% ({system_metrics().memory_used_gb:.1}/{system_metrics().memory_total_gb:.1} GB)" }
                            }

                            div { class: "flex items-center gap-2",
                                title: "{texts.used_label}: {format_bytes_gb(system_metrics().disk_used_gb)} / {texts.total_label}: {format_bytes_gb(system_metrics().disk_total_gb)}",
                                span { class: "text-xs font-medium text-slate-600 dark:text-slate-300 w-8", "DSK" }
                                div { class: "flex-1 {format_percentage_bg(system_metrics().disk_usage)} rounded-full h-1.5 overflow-hidden",
                                    div {
                                        class: "h-full bg-gradient-to-r from-amber-400 to-amber-600 dark:from-amber-500 dark:to-amber-700",
                                        style: "width: {system_metrics().disk_usage}%"
                                    }
                                }
                                span { class: "text-xs font-bold {format_percentage_color(system_metrics().disk_usage)} text-right whitespace-nowrap", "{system_metrics().disk_usage:.0}% ({system_metrics().disk_used_gb:.1}/{system_metrics().disk_total_gb:.1} GB)" }
                            }
                        }
                        div { class: "bg-gradient-to-br from-green-50 to-emerald-50 dark:from-green-900/20 dark:to-emerald-900/20 border border-green-200 dark:border-green-800 rounded-lg p-4 shadow-sm",
                            h4 { class: "font-bold text-green-900 dark:text-green-300 mb-3 text-sm", "{texts.activity_title}" }
                            div { class: "space-y-2",
                                div { class: "flex justify-between items-center",
                                    span { class: "text-xs text-green-700 dark:text-green-300", "{texts.total_ops_label}" }
                                    span { class: "text-sm font-bold text-green-900 dark:text-green-200 bg-white dark:bg-slate-700 px-2 py-1 rounded", "{total_operations}" }
                                }
                                div { class: "flex justify-between items-center",
                                    span { class: "text-xs text-green-700 dark:text-green-300", "{texts.last_op_label}" }
                                    span { class: "text-xs font-semibold text-green-600 dark:text-green-400", "{last_operation_time}" }
                                }
                            }
                        }

                        div { class: "bg-gradient-to-br from-indigo-50 to-purple-50 dark:from-indigo-900/20 dark:to-purple-900/20 border border-indigo-200 dark:border-indigo-800 rounded-lg p-4 shadow-sm",
                            h4 { class: "font-bold text-indigo-900 dark:text-indigo-300 mb-3 text-sm", "{texts.stats_title}" }
                            div { class: "grid grid-cols-2 gap-2",
                                div { class: "bg-white dark:bg-slate-700 rounded p-2",
                                    p { class: "text-xs text-slate-500 dark:text-slate-400 font-medium", "{texts.backups_count_label}" }
                                    p { class: "text-lg font-bold text-indigo-600 dark:text-indigo-400", "{backup_count}" }
                                }
                                div { class: "bg-white dark:bg-slate-700 rounded p-2",
                                    p { class: "text-xs text-slate-500 dark:text-slate-400 font-medium", "{texts.restores_count_label}" }
                                    p { class: "text-lg font-bold text-purple-600 dark:text-purple-400", "{restore_count}" }
                                }
                                div { class: "bg-white dark:bg-slate-700 rounded p-2",
                                    p { class: "text-xs text-slate-500 dark:text-slate-400 font-medium", "{texts.verifies_count_label}" }
                                    p { class: "text-lg font-bold text-blue-600 dark:text-blue-400", "{verify_count}" }
                                }
                                div { class: "bg-white dark:bg-slate-700 rounded p-2",
                                    p { class: "text-xs text-slate-500 dark:text-slate-400 font-medium", "{texts.prunes_count_label}" }
                                    p { class: "text-lg font-bold text-orange-600 dark:text-orange-400", "{prune_count}" }
                                }
                            }
                        }

                        div { class: "bg-gradient-to-br from-amber-50 to-orange-50 dark:from-amber-900/20 dark:to-orange-900/20 border border-amber-200 dark:border-amber-800 rounded-lg p-4 shadow-sm",
                            h4 { class: "font-bold text-amber-900 dark:text-amber-300 mb-2 text-sm", "{texts.alerts_title}" }
                            div { class: "flex items-start space-x-2",
                                span { class: "text-lg", "✓" }
                                p { class: "text-xs text-amber-700 dark:text-amber-300", "{texts.system_ok_msg}" }
                            }
                        }
                    }
                }
            }

        }
    }
}
