use dioxus::prelude::*;
use rsb_sdk::core::{validate_telegram_token, get_telegram_chat_id};

#[component]
pub fn TelegramValidator(
    bot_token: String,
) -> Element {
    let mut validation_result = use_signal(String::new);
    let mut chat_ids = use_signal(Vec::new());
    let mut is_validating = use_signal(|| false);
    let mut show_validator = use_signal(|| false);

    let handle_validate = move |_| {
        if bot_token.is_empty() {
            validation_result.set("❌ Please enter a bot token".to_string());
            return;
        }

        is_validating.set(true);
        validation_result.set("🔄 Validating bot token...".to_string());
        let token = bot_token.clone();

        spawn(async move {
            match validate_telegram_token(&token).await {
                Ok(bot) => {
                    validation_result.set(format!(
                        "✅ Bot válido: {} (@{})\nID: {}",
                        bot.first_name,
                        bot.username.as_deref().unwrap_or("sem username"),
                        bot.id
                    ));

                    // Try to get chat IDs
                    match get_telegram_chat_id(&token).await {
                        Ok(ids) => {
                            chat_ids.set(ids.clone());
                            let mut chat_text = format!("\n\n✅ Found {} chat(s):\n", ids.len());
                            for (id, chat_type) in ids {
                                chat_text.push_str(&format!("  • Chat ID: {} ({})\n", id, chat_type));
                            }
                            validation_result.set(validation_result().clone() + &chat_text);
                        }
                        Err(e) => {
                            validation_result.set(format!(
                                "{}\n\n⚠️ Could not get chat IDs: {}\n\nSolution: Send /start to your bot and try again",
                                validation_result(),
                                e
                            ));
                        }
                    }
                }
                Err(e) => {
                    validation_result.set(format!("❌ Validation failed:\n{}", e));
                }
            }

            is_validating.set(false);
        });
    };

    rsx! {
        div { class: "space-y-4",
            button {
                class: "btn btn-sm btn-outline",
                onclick: move |_| show_validator.toggle(),
                if show_validator() { "Hide Validator" } else { "🧪 Validate Bot Token" }
            }

            if show_validator() {
                div { class: "p-4 rounded-lg bg-slate-100 dark:bg-slate-700 space-y-3",
                    button {
                        class: "btn btn-sm btn-primary w-full",
                        onclick: handle_validate,
                        disabled: is_validating() || bot_token.is_empty(),
                        if is_validating() { "⏳ Validating..." } else { "✓ Validate Token" }
                    }

                    if !validation_result().is_empty() {
                        div { class: "p-3 rounded bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600",
                            p { class: "text-sm whitespace-pre-wrap font-mono", "{validation_result}" }
                        }
                    }

                    if !chat_ids().is_empty() {
                        div { class: "p-3 rounded bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800",
                            p { class: "text-sm font-semibold text-green-900 dark:text-green-300 mb-2", "📋 Available Chat IDs:" }
                            div { class: "space-y-1",
                                {chat_ids().iter().map(|(id, chat_type)| {
                                    rsx! {
                                        button {
                                            class: "btn btn-xs btn-ghost w-full justify-start",
                                            onclick: move |_| {
                                                // Copy to clipboard simulation
                                                validation_result.set(format!("Selected: {}", id));
                                            },
                                            "{id} ({chat_type})"
                                        }
                                    }
                                }).collect::<Vec<_>>()}
                            }
                        }
                    }
                }
            }
        }
    }
}
