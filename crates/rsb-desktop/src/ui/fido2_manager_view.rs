use dioxus::prelude::*;
use std::sync::Arc;
use axum::response::Html;
use tokio::sync::Mutex;

// SDK Imports (adjust names according to your real rsb_sdk structure)
use rsb_sdk::credentials::Fido2Manager;

// Internal project imports
use crate::ui::app::AppConfig;
use crate::ui::i18n::get_texts;

#[component]
pub fn Fido2ManagerView() -> Element {
    let mut app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    
    let mut user_id = use_signal(|| String::new());
    let mut credentials = use_signal(|| Vec::new());
    let mut message = use_signal(|| String::new());
    let mut recovery_codes = use_signal(|| Vec::<String>::new());
    let fido2_manager_arc = use_context::<Arc<Mutex<Fido2Manager>>>();

    let fido2_manager_for_effect = fido2_manager_arc.clone();
    use_effect(move || {
        let mgr_arc = fido2_manager_for_effect.clone();
        spawn(async move {
            let mgr = mgr_arc.lock().await;
            credentials.set(mgr.list_credentials());
        });
    });

    let fido2_manager_for_register = fido2_manager_arc.clone();
    let on_register = move |_| {
        let mgr_arc = fido2_manager_for_register.clone();

        spawn(async move {
            let id = user_id.read().clone();

            if id.is_empty() {
                message.set(format!("⚠️ {} {}", texts.user_identifier, "is required"));
                return;
            }

            message.set("🌐 Abrindo navegador para registo FIDO2...".into());
            
            // Iniciamos o servidor web que lida com a interação de hardware no navegador
            let html_content = include_str!("../../../rsb-cli/src/assets/fido2_auth.html");
            if let Err(e) = rsb_sdk::fido2::fido2_web::run_server(mgr_arc, Html(html_content)).await {
                message.set(format!("❌ Erro ao iniciar servidor: {}", e));
            }
        });
    };

    let on_generate_codes = move |_| {
        let mgr_arc = fido2_manager_arc.clone();
        spawn(async move {
            let mut mgr = mgr_arc.lock().await;
            let current_user_id = user_id.read().clone();

            if current_user_id.is_empty() {
                message.set(format!("⚠️ {} {}", texts.user_identifier, "is required to generate recovery codes."));
                return;
            }

            match mgr.generate_backup_codes(&current_user_id) {
                Ok(codes) => {
                    recovery_codes.set(codes);
                    message.set(format!("✅ {}", texts.recovery_codes_generated_success));
                }
                Err(e) => message.set(format!("❌ Failed to generate recovery codes: {}", e)),
            }
        });
    };

    rsx! {
        div { class: "p-6 bg-white rounded-lg shadow-sm border border-gray-200",
            h2 { class: "text-2xl font-bold mb-4 text-gray-800", "🛡️ {texts.manage_fido2_keys_title}" }

            div { class: "mb-6 flex gap-2",
                input {
                    class: "border border-gray-300 p-2 rounded flex-1 focus:ring-2 focus:ring-blue-500 outline-none",
                    placeholder: "{texts.user_identifier} (e.g., admin@rsb)",
                    value: "{user_id}",
                    oninput: move |evt| user_id.set(evt.value())
                }
                button {
                    class: "bg-blue-600 text-white px-4 py-2 rounded font-semibold hover:bg-blue-700 transition-colors",
                    onclick: on_register,
                    "{texts.add_new_fido2_key_button}"
                }
            }

            if !message.read().is_empty() {
                div { class: "mb-4 p-3 bg-blue-50 border-l-4 border-blue-500 text-blue-700 text-sm", "{message}" }
            }

            h3 { class: "font-bold text-gray-700 mb-3", "Trusted Devices ({credentials.read().len()})" }
            ul { class: "bg-gray-50 rounded divide-y divide-gray-200",
                for cred in credentials.read().iter() {
                    li { key: "{cred.user_id}", class: "p-4 flex justify-between items-center hover:bg-gray-100 transition-colors",
                        div {
                            p { class: "font-semibold text-gray-900", "{cred.user_id}" }
                            p { class: "text-xs text-gray-500", "Registered at: {cred.created_at}" }
                        }
                        div { class: "flex items-center gap-2",
                             span { class: "px-2 py-1 text-xs font-medium bg-green-100 text-green-700 rounded-full", "Active" }
                             if let Some(last) = &cred.last_used {
                                 span { class: "text-xs text-gray-400", "Last use: {last}" }
                             }
                        }
                    }
                }
            }

            // Seção de Códigos de Recuperação
            div { class: "mt-8 pt-6 border-t border-gray-100",
                h3 { class: "font-bold text-gray-700 mb-3", "{texts.recovery_codes_label}" }
                
                if recovery_codes.read().is_empty() {
                    button {
                        class: "btn-warning",
                        onclick: on_generate_codes,
                        "{texts.generate_recovery_codes_button}"
                    }
                } else {
                    div { class: "bg-gray-50 p-4 rounded border border-dashed border-gray-300",
                        p { class: "text-xs text-red-600 mb-3 font-bold", "⚠️ Guarde estes códigos em local seguro. Eles só serão exibidos uma vez." }
                        div { class: "grid grid-cols-2 gap-2",
                            for code in recovery_codes.read().iter() {
                                code { class: "bg-white p-2 rounded border border-gray-200 text-center font-mono text-sm", "{code}" }
                            }
                        }
                        button {
                            class: "mt-4 text-xs text-blue-600 hover:underline cursor-pointer",
                            onclick: move |_| recovery_codes.set(Vec::new()),
                            "Concluído"
                        }
                    }
                }
            }
        }
    }
}
