use dioxus::prelude::*;
use rsb_sdk::core::{get_telegram_chat_id, validate_telegram_token};

use crate::ui::{
    app::AppConfig,
    i18n::{get_texts, tr},
};

#[component]
pub fn TelegramValidator(bot_token: String) -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut validation_result = use_signal(String::new);
    let mut chat_ids = use_signal(Vec::new);
    let mut is_validating = use_signal(|| false);
    let mut show_validator = use_signal(|| false);
    let bot_token_clone = bot_token.clone();

    let handle_validate = move |_| {
        if bot_token.is_empty() {
            validation_result.set(texts.tg_enter_token.to_string());
            return;
        }

        is_validating.set(true);
        validation_result.set(texts.tg_validating_status.to_string());
        let token = bot_token.clone();

        spawn(async move {
            match validate_telegram_token(&token).await {
                Ok(bot) => {
                    validation_result.set(format!(
                        "{}\nID: {}",
                        tr(
                            texts.tg_bot_valid,
                            &[
                                &bot.first_name,
                                &bot.username.as_deref().unwrap_or(texts.tg_no_username),
                            ]
                        ),
                        bot.id
                    ));

                    // Try to get chat IDs
                    match get_telegram_chat_id(&token).await {
                        Ok(ids) => {
                            chat_ids.set(ids.clone());
                            let mut chat_text =
                                format!("\n\n{}\n", tr(texts.tg_chats_found, &[&ids.len()]));
                            for (id, chat_type) in ids {
                                chat_text
                                    .push_str(&format!("  • Chat ID: {} ({})\n", id, chat_type));
                            }
                            validation_result.set(validation_result().clone() + &chat_text);
                        }
                        Err(e) => {
                            validation_result.set(format!(
                                "{}\n\n{}\n\n{}",
                                validation_result(),
                                tr(texts.tg_chat_ids_error, &[&e]),
                                texts.tg_chat_ids_solution
                            ));
                        }
                    }
                }
                Err(e) => {
                    validation_result.set(format!("{}\n{}", texts.tg_validation_failed, e));
                }
            }

            is_validating.set(false);
        });
    };

    let ids_list = chat_ids();

    rsx! {
        div { class: "space-y-4",
            button {
                class: "btn btn-sm btn-outline",
                onclick: move |_| show_validator.toggle(),
                if show_validator() { "{texts.tg_hide_validator}" } else { "{texts.tg_show_validator}" }
            }

            if show_validator() {
                div { class: "p-4 rounded-lg bg-slate-100 dark:bg-slate-700 space-y-3",
                    button {
                        class: "btn btn-sm btn-primary w-full",
                        onclick: handle_validate,
                        disabled: is_validating() || bot_token_clone.is_empty(),
                        if is_validating() { "{texts.tg_validating_button}" } else { "{texts.tg_validate_button}" }
                    }

                    if !validation_result().is_empty() {
                        div { class: "p-3 rounded bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600",
                            p { class: "text-sm whitespace-pre-wrap font-mono", "{validation_result}" }
                        }
                    }

                    if !ids_list.is_empty() {
                        div { class: "p-3 rounded bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800",
                            p { class: "text-sm font-semibold text-green-900 dark:text-green-300 mb-2", "{texts.tg_available_chat_ids}" }
                            div { class: "space-y-1",
                                { ids_list.iter().map(|(id, chat_type)| {
                                    let id_val = *id;
                                    let chat_type_val = chat_type.clone();
                                    rsx! {
                                        button {
                                            class: "btn btn-xs btn-ghost w-full justify-start",
                                            onclick: move |_| {
                                                // Copy to clipboard simulation
                                                validation_result.set(tr(texts.tg_selected_chat, &[&id_val]));
                                            },
                                            "{id_val} ({chat_type_val})"
                                        }
                                    }
                                })}
                            }
                        }
                    }
                }
            }
        }
    }
}
