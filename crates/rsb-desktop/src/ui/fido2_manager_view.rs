use axum::response::Html;
use dioxus::prelude::*;
use std::sync::Arc;
use tokio::sync::Mutex;

// SDK Imports (adjust names according to your real rsb_sdk structure)
use rsb_sdk::credentials::Fido2Manager;

// Internal project imports
use crate::ui::app::AppConfig;
use crate::ui::error_handler::format_user_error;
use crate::ui::i18n::get_texts;

#[component]
pub fn Fido2ManagerView() -> Element {
    let mut app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());

    // Tentar obter o user_id autenticado do contexto da app
    let authenticated_user = try_use_context::<Signal<Option<String>>>();
    let authenticated_user_value = authenticated_user.map(|u| u().clone()).flatten();

    // Inicializar user_id com o user autenticado se disponível
    let mut user_id = use_signal(|| authenticated_user_value.unwrap_or_default());
    let mut credentials = use_signal(|| Vec::new());
    let mut message = use_signal(|| String::new());
    let mut recovery_codes = use_signal(|| Vec::<String>::new());
    let mut show_backup_warning = use_signal(|| false);
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

            message.set("🌐 Open the browser to register FIDO2 key...".into());

            let html_content = include_str!("../../../rsb-cli/src/assets/fido2_auth.html");
            if let Err(e) = rsb_sdk::fido2::fido2_web::run_server(mgr_arc, Html(html_content)).await
            {
                message.set(format_user_error(e, "fido2"));
            }
        });
    };

    let on_generate_codes = move |_| {
        let mgr_arc = fido2_manager_arc.clone();
        spawn(async move {
            let mut mgr = mgr_arc.lock().await;
            let current_user_id = user_id.read().clone();

            if current_user_id.is_empty() {
                message.set(format!(
                    "⚠️ {} {}",
                    texts.user_identifier, "is required to generate recovery codes."
                ));
                return;
            }

            match mgr.generate_backup_codes(&current_user_id) {
                Ok(codes) => {
                    recovery_codes.set(codes);
                    message.set(format!("✅ {}", texts.recovery_codes_generated_success));
                }
                Err(e) => message.set(format_user_error(e, "fido2")),
            }
        });
    };

    rsx! {
        div { class: "p-6 bg-white rounded-lg shadow-sm border border-gray-200",
            h2 { class: "text-2xl font-bold mb-4 text-gray-800", "🛡️ {texts.manage_fido2_keys_title}" }

            div { class: "mb-6 flex gap-2",
                div { class: "flex-1",
                    label { class: "block text-sm font-medium text-gray-700 mb-1", "{texts.user_identifier}" }
                    input {
                        class: "w-full border border-gray-300 p-2 rounded focus:ring-2 focus:ring-blue-500 outline-none",
                        placeholder: "ex: admin@rsb",
                        value: "{user_id}",
                        oninput: move |evt| user_id.set(evt.value())
                    }
                }
                button {
                    class: "bg-blue-600 text-white px-4 py-2 rounded font-semibold hover:bg-blue-700 transition-colors h-fit mt-6",
                    onclick: on_register,
                    "{texts.add_new_fido2_key_button}"
                }
            }

            if !message.read().is_empty() {
                div {
                    class: "mb-4 p-3 bg-blue-50 border-l-4 border-blue-500 text-blue-700 text-sm",
                    "{message}"
                }
            }

            h3 { class: "font-bold text-gray-700 mb-3 mt-6", "📱 Trusted Devices ({credentials.read().len()})" }
            if credentials.read().is_empty() {
                div { class: "bg-gray-50 rounded p-4 text-gray-500 text-sm",
                    "No trusted devices registered yet. Register a FIDO2 key above."
                }
            } else {
                ul { class: "bg-gray-50 rounded divide-y divide-gray-200",
                    for cred in credentials.read().iter() {
                        li { key: "{cred.user_id}", class: "p-4 flex justify-between items-center hover:bg-gray-100 transition-colors",
                            div {
                                p { class: "font-semibold text-gray-900", "{cred.user_id}" }
                                p { class: "text-xs text-gray-500", "Registered at: {cred.created_at}" }
                            }
                            div { class: "flex items-center gap-2",
                                 span { class: "px-2 py-1 text-xs font-medium bg-green-100 text-green-700 rounded-full", "✓ Active" }
                                 if let Some(last) = &cred.last_used {
                                     span { class: "text-xs text-gray-400", "Last use: {last}" }
                                 }
                            }
                        }
                    }
                }
            }

            div { class: "mt-8 pt-6 border-t border-gray-100",
                h3 { class: "font-bold text-gray-700 mb-3", "🔐 {texts.recovery_codes_label}" }

                if recovery_codes.read().is_empty() {
                    div { class: "bg-amber-50 border border-amber-200 rounded p-4 mb-4",
                        p { class: "text-sm text-amber-800 font-semibold mb-2", "⚠️ Important" }
                        p { class: "text-sm text-amber-700",
                            "Generate recovery codes to access your account in case of FIDO2 key loss. "
                            "These codes are critical for security - store them in a secure location!"
                        }
                    }
                    button {
                        class: "bg-amber-600 text-white px-4 py-2 rounded font-semibold hover:bg-amber-700 transition-colors",
                        onclick: on_generate_codes,
                        "🔑 {texts.generate_recovery_codes_button}"
                    }
                } else {
                    div { class: "bg-red-50 border-l-4 border-red-500 p-4 mb-4 rounded",
                        p { class: "text-sm font-bold text-red-700 mb-2", "🔴 STORE THESE CODES IN A SECURE LOCATION" }
                        p { class: "text-xs text-red-600 mb-4",
                            " This is the only time these recovery codes will be displayed. "
                            "If you lose them, you'll need to generate new ones. "
                            "Write them down on paper, store them in a digital safe, or print them securely."
                        }

                        div { class: "bg-white border border-gray-300 rounded p-4 mb-4",
                            div { class: "text-center mb-4",
                                p { class: "text-xs text-gray-500 font-semibold uppercase", "Recovery Codes" }
                                p { class: "text-xs text-gray-400", "Write down each code carefully" }
                            }
                            div { class: "grid grid-cols-2 gap-3 mb-4",
                                for code in recovery_codes.read().iter() {
                                    div {
                                        class: "bg-gray-50 p-3 rounded border border-gray-200 font-mono text-sm text-center font-bold text-gray-800",
                                        "{code}"
                                    }
                                }
                            }
                        }

                        button {
                            class: "w-full bg-gray-200 hover:bg-gray-300 text-gray-800 px-4 py-2 rounded font-semibold transition-colors",
                            onclick: move |_| {
                                recovery_codes.set(Vec::new());
                                message.set("✅ Codes closed. Generate new ones when needed.".into());
                            },
                            "Completed - Close Codes"
                        }
                    }
                }
            }
        }
    }
}
