use crate::ui::{
    app::AppConfig,
    i18n::get_texts,
    loading_state::{LoadingOverlay, LoadingStyle},
};
use dioxus::prelude::*;

#[derive(Clone, Debug)]
struct DiagnosticInfo {
    category: String,
    status: DiagnosticStatus,
    message: String,
    details: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DiagnosticStatus {
    Healthy,
    Warning,
    Critical,
}

impl DiagnosticStatus {
    fn color(&self) -> &'static str {
        match self {
            DiagnosticStatus::Healthy => "emerald",
            DiagnosticStatus::Warning => "yellow",
            DiagnosticStatus::Critical => "red",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            DiagnosticStatus::Healthy => "✅",
            DiagnosticStatus::Warning => "⚠️",
            DiagnosticStatus::Critical => "❌",
        }
    }
}

#[component]
pub fn DiagnosticsScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());

    let mut diagnostics = use_signal(Vec::<DiagnosticInfo>::new);
    let mut is_loading = use_signal(|| false);
    let mut last_run = use_signal(|| Option::<String>::None);

    let mut run_diagnostics = move |_| {
        is_loading.set(true);
        spawn(async move {
            // TODO: Implement actual diagnostics
            // This should check:
            // - Backup storage availability
            // - Configuration validity
            // - Encryption key status
            // - FIDO2 device status
            // - Disk space
            // - Network connectivity
            // - Database integrity
            // - Last backup status

            let mut diags = vec![
                DiagnosticInfo {
                    category: "Storage".to_string(),
                    status: DiagnosticStatus::Healthy,
                    message: "Disk space available: 250 GB".to_string(),
                    details: Some("Backup directory: ~/backups".to_string()),
                },
                DiagnosticInfo {
                    category: "Configuration".to_string(),
                    status: DiagnosticStatus::Healthy,
                    message: "Configuration file is valid".to_string(),
                    details: Some("Default profile loaded successfully".to_string()),
                },
                DiagnosticInfo {
                    category: "Encryption".to_string(),
                    status: DiagnosticStatus::Healthy,
                    message: "Encryption key available".to_string(),
                    details: Some("Algorithm: AES-256-GCM".to_string()),
                },
                DiagnosticInfo {
                    category: "FIDO2".to_string(),
                    status: DiagnosticStatus::Warning,
                    message: "No FIDO2 key configured".to_string(),
                    details: Some("Consider adding a key for biometric authentication".to_string()),
                },
                DiagnosticInfo {
                    category: "Last Backup".to_string(),
                    status: DiagnosticStatus::Healthy,
                    message: "Backup completed successfully".to_string(),
                    details: Some("Time: 2 hours ago | 1,234 files | 12 GB".to_string()),
                },
            ];

            diagnostics.set(diags);
            last_run.set(Some("Now".to_string()));
            is_loading.set(false);
        });
    };

    use_effect(move || {
        run_diagnostics(());
    });

    rsx! {
        div { class: "space-y-6",
            // Loading overlay
            LoadingOverlay {
                is_visible: is_loading(),
                style: LoadingStyle::Spinner,
                message: texts.diagnostics_running.to_string(),
            }

            // Header
            div { class: "flex justify-between items-center",
                h2 { class: "text-2xl font-bold text-slate-900 dark:text-white", "🔧 {texts.diagnostics_title}" }
                button {
                    class: "btn-primary py-2 px-4 flex items-center gap-2",
                    onclick: move |_| run_diagnostics(()),
                    disabled: is_loading(),
                    if is_loading() { "{texts.diagnostics_running}" } else { "{texts.diagnostics_run}" }
                }
            }

            // Last run info
            if let Some(run_time) = last_run.read().as_ref() {
                div { class: "text-xs text-slate-500 dark:text-slate-400", "Last run: {run_time}" }
            }

            // Diagnostics summary
            div { class: "grid grid-cols-3 gap-4",
                {
                    let healthy = diagnostics.read().iter().filter(|d| d.status == DiagnosticStatus::Healthy).count();
                    rsx! {
                        div { class: "p-4 bg-emerald-50 dark:bg-emerald-900/20 border border-emerald-200 dark:border-emerald-800/50 rounded-lg",
                            p { class: "text-sm font-semibold text-emerald-900 dark:text-emerald-100", "{texts.diagnostics_healthy}" }
                            p { class: "text-2xl font-bold text-emerald-700 dark:text-emerald-400 mt-2", "{healthy}" }
                        }
                    }
                }

                {
                    let warnings = diagnostics.read().iter().filter(|d| d.status == DiagnosticStatus::Warning).count();
                    rsx! {
                        div { class: "p-4 bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800/50 rounded-lg",
                            p { class: "text-sm font-semibold text-yellow-900 dark:text-yellow-100", "{texts.diagnostics_warnings}" }
                            p { class: "text-2xl font-bold text-yellow-700 dark:text-yellow-400 mt-2", "{warnings}" }
                        }
                    }
                }

                {
                    let critical = diagnostics.read().iter().filter(|d| d.status == DiagnosticStatus::Critical).count();
                    rsx! {
                        div { class: "p-4 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800/50 rounded-lg",
                            p { class: "text-sm font-semibold text-red-900 dark:text-red-100", "{texts.diagnostics_critical}" }
                            p { class: "text-2xl font-bold text-red-700 dark:text-red-400 mt-2", "{critical}" }
                        }
                    }
                }
            }

            // Diagnostics details
            div { class: "space-y-4",
                for diag in diagnostics.read().iter() {
                    {
                        let color = diag.status.color();
                        let icon = diag.status.icon();
                        let category = diag.category.clone();
                        let message = diag.message.clone();
                        let details = diag.details.clone();

                        rsx! {
                            div {
                                key: "{category}",
                                class: "p-4 border rounded-lg transition-all",
                                class: if diag.status == DiagnosticStatus::Healthy {
                                    "bg-emerald-50 dark:bg-emerald-900/20 border-emerald-200 dark:border-emerald-800/50"
                                } else if diag.status == DiagnosticStatus::Warning {
                                    "bg-yellow-50 dark:bg-yellow-900/20 border-yellow-200 dark:border-yellow-800/50"
                                } else {
                                    "bg-red-50 dark:bg-red-900/20 border-red-200 dark:border-red-800/50"
                                },
                                div { class: "flex items-start gap-3",
                                    span { class: "text-2xl flex-shrink-0 mt-1", "{icon}" }
                                    div { class: "flex-1",
                                        h3 { class: "font-semibold text-sm", class: if diag.status == DiagnosticStatus::Healthy {
                                            "text-emerald-900 dark:text-emerald-100"
                                        } else if diag.status == DiagnosticStatus::Warning {
                                            "text-yellow-900 dark:text-yellow-100"
                                        } else {
                                            "text-red-900 dark:text-red-100"
                                        }, "{category}" }
                                        p { class: "text-sm mt-1", class: if diag.status == DiagnosticStatus::Healthy {
                                            "text-emerald-700 dark:text-emerald-200"
                                        } else if diag.status == DiagnosticStatus::Warning {
                                            "text-yellow-700 dark:text-yellow-200"
                                        } else {
                                            "text-red-700 dark:text-red-200"
                                        }, "{message}" }
                                        if let Some(detail) = details {
                                            p { class: "text-xs mt-2 opacity-75", class: if diag.status == DiagnosticStatus::Healthy {
                                                "text-emerald-600 dark:text-emerald-300"
                                            } else if diag.status == DiagnosticStatus::Warning {
                                                "text-yellow-600 dark:text-yellow-300"
                                            } else {
                                                "text-red-600 dark:text-red-300"
                                            }, "{detail}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Recommendations
            div { class: "p-4 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800/50 rounded-lg",
                h3 { class: "font-semibold text-sm text-blue-900 dark:text-blue-100 mb-2", "💡 Recomendações" }
                ul { class: "text-xs text-blue-800 dark:text-blue-200 space-y-2 list-disc pl-5",
                    li { "Configure uma chave FIDO2 para autenticação mais segura" }
                    li { "Revise o agendamento de backups para garantir proteção regular" }
                    li { "Realize um backup completo a cada mês para melhor recuperação" }
                }
            }
        }
    }
}
