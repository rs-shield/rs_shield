use crate::ui::app::AppConfig;
use crate::ui::components::loading_state::{LoadingOverlay, LoadingStyle};
use crate::ui::components::{
    Button, ButtonSize, ButtonVariant, Card, CardVariant, Input, InputState, InputVariant,
};
use crate::ui::error_handler::format_user_error;
use crate::ui::i18n::get_texts;
use axum::response::Html;
use dioxus::prelude::*;
use rsb_sdk::credentials::Fido2Manager;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Message when the identifier has no FIDO2 credential. Keys are
/// registered with the CLI, so a first-time user is told how.
pub fn login_not_found_message(id: &str, no_credentials: bool) -> String {
    if no_credentials {
        "❌ No FIDO2 security key is registered on this computer. Register one first with `rsb auth register`, then sign in with its identifier.".to_string()
    } else {
        format!(
            "❌ Identifier '{}' not found. Use the identifier you registered (`rsb auth list` shows them).",
            id
        )
    }
}

#[component]
pub fn LoginScreen(on_login: EventHandler<String>) -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let fido2_manager_arc = use_context::<Arc<Mutex<Fido2Manager>>>();

    let mut user_id = use_signal(String::new);
    let mut error_msg = use_signal(String::new);
    let mut is_authenticating = use_signal(|| false);
    let mut show_recovery_input = use_signal(|| false);
    let mut recovery_code = use_signal(String::new);

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
            let (has_cred, no_credentials, load_error) = {
                let mut mgr = fido2_manager_arc_clone.lock().await;
                // A credentials file that cannot be read must not look like
                // "identifier not found".
                let load_error = Fido2Manager::default_storage_path()
                    .and_then(|path| mgr.load_from_file(&path).map_err(|e| e.to_string()))
                    .err();
                (
                    mgr.has_credential(&id),
                    mgr.list_credentials().is_empty(),
                    load_error,
                )
            };

            if let Some(e) = load_error {
                error_msg.set(format!("❌ Could not read the FIDO2 credentials: {}", e));
                is_authenticating.set(false);
                return;
            }
            if !has_cred {
                error_msg.set(login_not_found_message(&id, no_credentials));
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
                // O código só fica consumido se for persistido; caso contrário
                // continuaria reutilizável, por isso o login falha.
                let saved = Fido2Manager::default_storage_path()
                    .map_err(|e| e.to_string())
                    .and_then(|path| mgr.save_to_file(&path).map_err(|e| e.to_string()));
                match saved {
                    Ok(()) => on_login.call(id),
                    Err(e) => error_msg.set(format!(
                        "❌ Não foi possível registar o uso do código de recuperação: {}",
                        e
                    )),
                }
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

        // Centered layout with smooth transition
        div { class: "flex flex-col items-center justify-center min-h-screen py-12 px-4",
            // Card - Login container
            Card {
                variant: CardVariant::Elevated,
                class: "w-full max-w-md",

                // Header with icon
                div { class: "text-center mb-8",
                    div { class: "text-6xl mb-4 block animate-bounce", "🔐" }
                    h1 { class: "text-3xl font-bold text-slate-900 dark:text-white", "{texts.login_title}" }
                    p { class: "text-sm text-slate-600 dark:text-slate-400 mt-2", "{texts.auth_required_msg}" }
                }

                // Form
                div { class: "space-y-4",
                    // User identifier input
                    Input {
                        variant: InputVariant::Text,
                        state: if is_authenticating() { InputState::Disabled } else { InputState::Default },
                        label: texts.user_identifier.to_string(),
                        placeholder: "admin@rsb".to_string(),
                        value: "{user_id}",
                        helper_text: "Your unique identifier".to_string(),
                        disabled: is_authenticating(),
                        oninput: Some(EventHandler::new(move |evt: FormEvent| user_id.set(evt.value()))),
                    }

                    // Recovery code input (conditional) — mascarado contra shoulder-surfing.
                    if show_recovery_input() {
                        Input {
                            variant: InputVariant::Password,
                            state: if is_authenticating() { InputState::Disabled } else { InputState::Default },
                            label: texts.recovery_codes_label.to_string(),
                            placeholder: "XXXX-XXXX".to_string(),
                            value: "{recovery_code}",
                            helper_text: "Enter your backup recovery code".to_string(),
                            disabled: is_authenticating(),
                            oninput: Some(EventHandler::new(move |evt: FormEvent| recovery_code.set(evt.value()))),
                        }
                    }

                    // Error message (conditional)
                    if !error_msg().is_empty() {
                        div { class: "flex items-start gap-3 p-4 bg-red-50 dark:bg-red-900/20 border-l-4 border-red-500 rounded-r-lg",
                            span { class: "text-xl flex-shrink-0 mt-0.5", "⚠️" }
                            div { class: "flex-1",
                                p { class: "text-sm text-red-800 dark:text-red-300", "{error_msg}" }
                            }
                        }
                    }

                    // Action buttons
                    div { class: "flex gap-3 pt-2",
                        if show_recovery_input() {
                            Button {
                                variant: ButtonVariant::Warning,
                                size: ButtonSize::LG,
                                disabled: is_authenticating(),
                                loading: is_authenticating(),
                                class: "flex-1".to_string(),
                                onclick: handle_recovery_login,
                                "Verify Code"
                            }
                        } else {
                            Button {
                                variant: ButtonVariant::Primary,
                                size: ButtonSize::LG,
                                disabled: is_authenticating(),
                                loading: is_authenticating(),
                                class: "flex-1".to_string(),
                                onclick: handle_login,
                                "{texts.login_button}"
                            }
                        }
                    }

                    // Toggle link
                    div { class: "text-center pt-4 border-t border-slate-200 dark:border-slate-700",
                        button {
                            class: "text-sm font-medium text-indigo-600 dark:text-indigo-400 hover:text-indigo-700 dark:hover:text-indigo-300 transition-colors duration-150 cursor-pointer",
                            onclick: move |_| show_recovery_input.toggle(),
                            if show_recovery_input() { "← Back to FIDO2" } else { "Use recovery code →" }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::login_not_found_message;

    #[test]
    fn first_run_message_explains_registration() {
        assert!(login_not_found_message("me", true).contains("rsb auth register"));
        assert!(login_not_found_message("me", false).contains("'me'"));
    }
}
