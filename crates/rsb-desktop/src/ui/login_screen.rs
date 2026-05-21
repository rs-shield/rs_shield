use crate::ui::app::AppConfig;
use crate::ui::error_handler::format_user_error;
use crate::ui::i18n::get_texts;
use dioxus::prelude::*;
use rsb_sdk::credentials::Fido2Manager;
use std::sync::Arc;
use tokio::sync::Mutex;
use axum::response::Html;


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
            error_msg.set(format!("⚠️ {} é obrigatório.", texts.user_identifier));
            return;
        }

        is_authenticating.set(true);
        error_msg.set(String::new());

        let fido2_manager_arc_clone = fido2_for_login.clone();
        spawn(async move {
            let has_cred = {
                let mut mgr = fido2_manager_arc_clone.lock().await;

                // Carregar do disco para garantir que temos os dados mais recentes
                if let Ok(path) = Fido2Manager::default_storage_path() {
                    let _ = mgr.load_from_file(&path);
                }

                mgr.has_credential(&id)
            }; // O cadeado (lock) é liberado aqui ao sair do escopo

            if !has_cred {
                error_msg.set(format!(
                    "❌ Identificador '{}' não encontrado.",
                    id
                ));
                is_authenticating.set(false);
                return;
            }

            error_msg.set("🌐 Abrindo navegador para autenticação...".into());
            let html_content = include_str!("../../../rsb-cli/src/assets/fido2_auth.html");
            
            let result = rsb_sdk::fido2::fido2_web::run_server(fido2_manager_arc_clone.clone(), Html(html_content)).await;
            
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
            error_msg.set("⚠️ Identificador e código são necessários.".into());
            return;
        }

        is_authenticating.set(true);
        let fido2_manager_arc_clone = fido2_for_recovery.clone();

        spawn(async move {
            let mut mgr = fido2_manager_arc_clone.lock().await;
            
            // Tenta validar o código real usando o Fido2Manager
            if mgr.verify_backup_code(&id, &code) {
                // Se o código for válido, salvamos a alteração (o código foi consumido)
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
        div { class: "flex flex-col items-center justify-center min-h-[400px] py-12",
            div { class: "w-full max-w-md bg-white dark:bg-slate-800 rounded-xl shadow-2xl border border-slate-200 dark:border-slate-700 p-8",
                div { class: "text-center mb-8",
                    span { class: "text-5xl mb-4 block", "🔐" }
                    h2 { class: "text-2xl font-bold text-slate-900 dark:text-white", "{texts.login_title}" }
                    p { class: "text-slate-500 dark:text-slate-400 mt-2", "{texts.auth_required_msg}" }
                }

                div { class: "space-y-4",
                    div { class: "form-group",
                        label { class: "label-text", "{texts.user_identifier}" }
                        input {
                            class: "input-field",
                            r#type: "text",
                            placeholder: "ex: admin@rsb",
                            value: "{user_id}",
                            oninput: move |evt| user_id.set(evt.value()),
                            disabled: is_authenticating()
                        }
                    }

                    if show_recovery_input() {
                        div { class: "form-group animate-fade-in",
                            label { class: "label-text", "{texts.recovery_codes_label}" }
                            input {
                                class: "input-field font-mono",
                                r#type: "text",
                                placeholder: "XXXX-XXXX",
                                value: "{recovery_code}",
                                oninput: move |evt| recovery_code.set(evt.value()),
                                disabled: is_authenticating()
                            }
                        }
                    }

                    if !error_msg().is_empty() {
                        div { class: "p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded text-red-600 text-sm",
                            "{error_msg}"
                        }
                    }

                    if show_recovery_input() {
                         button {
                            class: "w-full btn-warning py-3 text-lg",
                            onclick: handle_recovery_login,
                            disabled: is_authenticating(),
                            if is_authenticating() { "⏳ Verificando..." } else { "Entrar com Código" }
                        }
                    } else {
                        button {
                            class: "w-full btn-primary py-3 text-lg flex items-center justify-center gap-2",
                            onclick: handle_login,
                            disabled: is_authenticating(),
                            if is_authenticating() { "⏳ Aguardando Chave..." } else { "{texts.login_button}" }
                        }
                    }

                    div { class: "text-center mt-4",
                        a { 
                            class: "text-sm text-indigo-600 hover:underline cursor-pointer",
                            onclick: move |_| show_recovery_input.toggle(),
                            if show_recovery_input() { "Voltar para FIDO2" } else { "{texts.use_recovery_code_link}" }
                        }
                    }
                }
            }
        }
    }
}
