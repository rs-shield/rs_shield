use crate::ui::app::AppConfig;
use crate::ui::error_handler::format_user_error;
use crate::ui::i18n::get_texts;
use crate::ui::loading_state::{LoadingOverlay, LoadingStyle};
use axum::response::Html;
use dioxus::prelude::*;
use rsb_sdk::credentials::Fido2Manager;
use std::sync::Arc;
use tokio::sync::Mutex;

#[component]
pub fn LoginScreen(on_login: EventHandler<String>) -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let fido2_manager_arc = use_context::<Arc<Mutex<Fido2Manager>>>();

    let mut user_id = use_signal(|| String::new());
    let mut error_msg = use_signal(|| String::new());
    let mut is_authenticating = use_signal(|| false);
    let mut show_recovery_input = use_signal(|| false);
    let mut recovery_code = use_signal(|| String::new());

    let fido2_for_login = fido2_manager_arc.clone();
    let handle_login = move |_| {
        let id = user_id();
        if id.is_empty() {
            error_msg.set(format!("⚠️ {} is required.", texts.user_identifier));
            return;
        }

        is_authenticating.set(true);
        error_msg.set(String::new());

        let fido2_manager_arc_clone = fido2_for_login.clone();
        spawn(async move {
            let has_cred = {
                let mut mgr = fido2_manager_arc_clone.lock().await;

                if let Ok(path) = Fido2Manager::default_storage_path() {
                    let _ = mgr.load_from_file(&path);
                }

                mgr.has_credential(&id)
            };

            if !has_cred {
                error_msg.set(format!("❌ Identifier '{}' not found.", id));
                is_authenticating.set(false);
                return;
            }

            error_msg.set("🌐 Browser is opening for authentication...".into());
            let html_content = include_str!("../../../rsb-cli/src/assets/fido2_auth.html");

            let result = rsb_sdk::fido2::fido2_web::run_server(
                fido2_manager_arc_clone.clone(),
                Html(html_content),
            )
            .await;

            match result {
                Ok(_) => on_login.call(id),
                Err(e) => error_msg.set(format_user_error(e, "fido2")),
            }
            is_authenticating.set(false);
        });
    };

    let fido2_for_recovery = fido2_manager_arc.clone();
    let handle_recovery_login = move |_| {
        let id = user_id();
        let code = recovery_code();

        if id.is_empty() || code.is_empty() {
            error_msg.set("⚠️ Identifier and code are required.".into());
            return;
        }

        is_authenticating.set(true);
        let fido2_manager_arc_clone = fido2_for_recovery.clone();

        spawn(async move {
            let mut mgr = fido2_manager_arc_clone.lock().await;

            if mgr.verify_backup_code(&id, &code) {
                if let Ok(path) = Fido2Manager::default_storage_path() {
                    let _ = mgr.save_to_file(&path);
                }
                on_login.call(id);
            } else {
                error_msg.set("❌ Código de recuperação inválido ou já utilizado.".into());
            }
            is_authenticating.set(false);
        });
    };

    rsx! {
        LoadingOverlay {
            is_visible: is_authenticating(),
            style: LoadingStyle::Spinner,
            message: if show_recovery_input() {
                "Verifying recovery code...".to_string()
            } else {
                "Authenticating with FIDO2...".to_string()
            },
        }

        // Layout de centralização melhorado com transição de opacidade na entrada
        div { class: "flex flex-col items-center justify-center min-h-[80vh] py-12 px-4 transition-opacity duration-300 ease-in-out",
            // Cartão principal com sombras e bordas mais suaves
            div { class: "w-full max-w-md bg-white dark:bg-slate-800 rounded-2xl shadow-2xl border border-slate-100 dark:border-slate-700/50 p-8 sm:p-10 transform transition-all",

                // Cabeçalho refinado
                div { class: "text-center mb-8 select-none",
                    span { class: "text-5xl mb-4 block animate-bounce duration-1000", "🔐" }
                    h2 { class: "text-2xl sm:text-3xl font-extrabold text-slate-900 dark:text-white tracking-tight", "{texts.login_title}" }
                    p { class: "text-sm text-slate-500 dark:text-slate-400 mt-2 max-w-xs mx-auto leading-relaxed", "{texts.auth_required_msg}" }
                }

                div { class: "space-y-5",
                    // Grupo do Input do Utilizador
                    div { class: "form-group flex flex-col gap-1.5",
                        label { class: "label-text text-xs font-semibold uppercase tracking-wider text-slate-600 dark:text-slate-400", "{texts.user_identifier}" }
                        input {
                            class: "input-field w-full px-4 py-2.5 rounded-lg border border-slate-300 dark:border-slate-600 bg-transparent text-slate-900 dark:text-white focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500 outline-none transition-all duration-200 disabled:opacity-50 disabled:cursor-not-allowed",
                            r#type: "text",
                            placeholder: "ex: admin@rsb",
                            value: "{user_id}",
                            oninput: move |evt| user_id.set(evt.value()),
                            disabled: is_authenticating()
                        }
                    }

                    // Grupo do Input de Recuperação com transição suave nativa do Tailwind
                    if show_recovery_input() {
                        div { class: "form-group flex flex-col gap-1.5 transition-all duration-300 ease-out transform translate-y-0 opacity-100",
                            label { class: "label-text text-xs font-semibold uppercase tracking-wider text-slate-600 dark:text-slate-400", "{texts.recovery_codes_label}" }
                            input {
                                class: "input-field w-full px-4 py-2.5 rounded-lg border border-slate-300 dark:border-slate-600 bg-transparent text-slate-900 dark:text-white font-mono tracking-widest focus:ring-2 focus:ring-amber-500/20 focus:border-amber-500 outline-none transition-all duration-200 disabled:opacity-50 disabled:cursor-not-allowed",
                                r#type: "text",
                                placeholder: "XXXX-XXXX",
                                value: "{recovery_code}",
                                oninput: move |evt| recovery_code.set(evt.value()),
                                disabled: is_authenticating()
                            }
                        }
                    }

                    // Caixa de erro com melhor padding e contraste interno
                    if !error_msg().is_empty() {
                        div { class: "flex items-start gap-2.5 p-3.5 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900/50 rounded-lg text-red-600 dark:text-red-400 text-sm font-medium leading-5 animate-pulse",
                            span { class: "flex-shrink-0 select-none", "💡" }
                            div { class: "break-words w-full", "{error_msg}" }
                        }
                    }

                    // Botões de Ação com feedbacks táteis (hover/active/disabled)
                    if show_recovery_input() {
                         button {
                            class: "w-full btn-warning py-3 text-base font-semibold rounded-lg shadow-md hover:shadow-lg transform hover:-translate-y-0.5 active:translate-y-0 transition-all duration-150 disabled:opacity-50 disabled:pointer-events-none flex items-center justify-center gap-2",
                            onclick: handle_recovery_login,
                            disabled: is_authenticating(),
                            if is_authenticating() { "⏳ Verifying..." } else { "Enter with Code" }

                        }
                    } else {
                        button {
                            class: "w-full btn-primary py-3 text-base font-semibold rounded-lg shadow-md hover:shadow-lg transform hover:-translate-y-0.5 active:translate-y-0 transition-all duration-150 disabled:opacity-50 disabled:pointer-events-none flex items-center justify-center gap-2",
                            onclick: handle_login,
                            disabled: is_authenticating(),
                            if is_authenticating() {
                                span { class: "inline-block animate-spin", "⏳" }
                            }
                            if is_authenticating() { "Waiting for key..." } else { "{texts.login_button}" }
                        }
                    }

                    // Zona inferior de alternância (Link de alternância)
                    div { class: "text-center pt-2",
                        a {
                            class: "text-sm font-medium text-indigo-600 dark:text-indigo-400 hover:text-indigo-700 dark:hover:text-indigo-300 transition-colors duration-150 underline decoration-indigo-500/30 hover:decoration-indigo-500 underline-offset-4 cursor-pointer select-none",
                            onclick: move |_| show_recovery_input.toggle(),
                            if show_recovery_input() { "Go back to login" } else { "{texts.use_recovery_code_link}" }
                        }
                    }
                }
            }
        }
    }
}
