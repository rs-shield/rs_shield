use crate::ui::app::AppConfig;
use crate::ui::i18n::get_texts;
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

    let handle_login = move |_| {
        let id = user_id();
        if id.is_empty() {
            error_msg.set(format!("⚠️ {} é obrigatório.", texts.user_identifier));
            return;
        }

        is_authenticating.set(true);
        error_msg.set(String::new());

        let fido2_manager_arc_clone = fido2_manager_arc.clone();
        spawn(async move {
            let mut mgr = fido2_manager_arc_clone.lock().await;

            if !mgr.has_credential(&id) {
                error_msg.set(format!(
                    "❌ Usuário '{}' não encontrado. Registre-o primeiro.",
                    id
                ));
                is_authenticating.set(false);
                return;
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            if true {
                on_login.call(id);
            } else {
                error_msg.set("❌ Falha na autenticação FIDO2".into());
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

                    if !error_msg().is_empty() {
                        div { class: "p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded text-red-600 text-sm",
                            "{error_msg}"
                        }
                    }

                    button {
                        class: "w-full btn-primary py-3 text-lg flex items-center justify-center gap-2",
                        onclick: handle_login,
                        disabled: is_authenticating(),
                        if is_authenticating() { "⏳ Aguardando Chave..." } else { "{texts.login_button}" }
                    }
                }
            }
        }
    }
}
