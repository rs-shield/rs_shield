use dioxus::prelude::*;
use rsb_sdk::core::{ChatIntegration, EmailConfig};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ui::{
    app::AppConfig,
    i18n::{get_texts, tr},
};
/// Configurações de integrações salvas localmente
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationConfig {
    pub email: Option<EmailIntegrationConfig>,
    pub slack: Option<SlackIntegrationConfig>,
    pub telegram: Option<TelegramIntegrationConfig>,
    pub discord: Option<DiscordIntegrationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailIntegrationConfig {
    pub enabled: bool,
    pub smtp_server: String,
    pub smtp_port: u16,
    pub sender_email: String,
    pub sender_password: String,
    pub recipient_email: String,
    pub use_tls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackIntegrationConfig {
    pub enabled: bool,
    pub webhook_url: String,
    pub mention_user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramIntegrationConfig {
    pub enabled: bool,
    pub bot_token: String,
    pub chat_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordIntegrationConfig {
    pub enabled: bool,
    pub webhook_url: String,
}

impl IntegrationConfig {
    /// The one file the Integrations screen saves and every operation
    /// screen reads: `~/.rs-shield/integrations.json`. (Operations used to
    /// look next to the profile or the backup folder instead, so configured
    /// notifications were never sent.)
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".rs-shield")
            .join("integrations.json")
    }

    /// Loads [`Self::default_path`] (empty configuration if missing).
    pub fn load_default() -> Self {
        Self::load_from(&Self::default_path())
    }

    /// Loads the configuration file at `path` (empty if missing/invalid).
    pub fn load_from(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                tracing::warn!("Ignoring invalid {}: {}", path.display(), e);
                Self::empty()
            }),
            Err(_) => Self::empty(),
        }
    }

    fn empty() -> Self {
        Self {
            email: None,
            slack: None,
            telegram: None,
            discord: None,
        }
    }

    /// Saves to `path` (mode 0600: it holds SMTP passwords and webhooks).
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            rsb_sdk::config::create_dir_secure(dir)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        rsb_sdk::config::write_file_secure(path, json.as_bytes())
    }

    /// Converter para EmailConfig do SDK
    pub fn to_email_config(&self) -> Option<EmailConfig> {
        self.email
            .as_ref()
            .filter(|e| e.enabled)
            .map(|e| EmailConfig {
                smtp_server: e.smtp_server.clone(),
                smtp_port: e.smtp_port,
                sender_email: e.sender_email.clone(),
                sender_password: e.sender_password.clone(),
                recipient_email: e.recipient_email.clone(),
                use_tls: e.use_tls,
            })
    }

    /// Converter para ChatIntegrations do SDK
    pub fn to_chat_integrations(&self) -> Vec<ChatIntegration> {
        let mut integrations = Vec::new();

        if let Some(slack) = &self.slack
            && slack.enabled
        {
            integrations.push(ChatIntegration::Slack {
                webhook_url: slack.webhook_url.clone(),
                mention_user: slack.mention_user.clone(),
            });
        }

        if let Some(telegram) = &self.telegram
            && telegram.enabled
        {
            integrations.push(ChatIntegration::Telegram {
                bot_token: telegram.bot_token.clone(),
                chat_id: telegram.chat_id.clone(),
            });
        }

        if let Some(discord) = &self.discord
            && discord.enabled
        {
            integrations.push(ChatIntegration::Discord {
                webhook_url: discord.webhook_url.clone(),
            });
        }

        integrations
    }
}

/// Componente de tela de integrações
#[component]
pub fn IntegrationScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut config = use_signal(IntegrationConfig::load_default);
    let mut active_tab = use_signal(|| "email");
    let mut status_msg = use_signal(String::new);
    let mut show_status = use_signal(|| false);
    let mut testing = use_signal(|| false);
    let mut test_results = use_signal(String::new);

    let handle_save_config = move |_| {
        if let Ok(()) = config().save_to(&IntegrationConfig::default_path()) {
            status_msg.set(texts.integ_save_ok.to_string());
            show_status.set(true);

            // Auto-hide após 3 segundos
            spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                show_status.set(false);
            });
        } else {
            status_msg.set(texts.integ_save_error.to_string());
            show_status.set(true);
        }
    };

    let handle_test_config = move |_| {
        testing.set(true);
        test_results.set(texts.integ_testing_status.to_string());

        let current_config = config();
        spawn(async move {
            let mut manager = rsb_sdk::core::NotificationManager::new();

            // Adicionar configurações habilitadas
            if let Some(email_cfg) = current_config.to_email_config() {
                manager.set_email_config(email_cfg);
            }

            for chat_integration in current_config.to_chat_integrations() {
                manager.add_chat_integration(chat_integration);
            }

            // Testar
            match manager.test().await {
                Ok(results) => {
                    let mut result_text = format!("{}\n", texts.integ_tests_done);

                    if results.email_success {
                        result_text.push_str(&format!("{}\n", texts.integ_email_sent_ok));
                    } else if let Some(err) = &results.email_error {
                        result_text.push_str(&format!("❌ Email: {}\n", err));
                    }

                    for success in &results.chat_success {
                        result_text
                            .push_str(&format!("{}\n", tr(texts.integ_chat_sent_ok, &[success])));
                    }

                    for (channel, err) in &results.chat_errors {
                        result_text.push_str(&format!("❌ {}: {}\n", channel, err));
                    }

                    test_results.set(result_text);
                }
                Err(e) => {
                    test_results.set(tr(texts.integ_tests_error, &[&e]));
                }
            }

            testing.set(false);
        });
    };

    rsx! {
        div { class: "flex flex-col h-full gap-4 p-4 bg-slate-50 dark:bg-slate-900",
            // Header
            div {
                h1 { class: "text-xl font-bold text-slate-900 dark:text-slate-100 mb-2", "{texts.integ_title}" }
                p { class: "text-sm text-slate-600 dark:text-slate-400",
                    "{texts.integ_subtitle}"
                }
            }

            // Status Message
            if show_status() {
                div { class: "p-4 rounded-lg bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800",
                    p { class: "text-green-800 dark:text-green-300", "{status_msg}" }
                }
            }

            // Tabs
            div { class: "tabs tabs-bordered",
                button {
                    class: if active_tab() == "email" { "tab tab-active" } else { "tab" },
                    onclick: move |_| active_tab.set("email"),
                    "📧 Email"
                }
                button {
                    class: if active_tab() == "slack" { "tab tab-active" } else { "tab" },
                    onclick: move |_| active_tab.set("slack"),
                    "🔵 Slack"
                }
                button {
                    class: if active_tab() == "telegram" { "tab tab-active" } else { "tab" },
                    onclick: move |_| active_tab.set("telegram"),
                    "✈️ Telegram"
                }
                button {
                    class: if active_tab() == "discord" { "tab tab-active" } else { "tab" },
                    onclick: move |_| active_tab.set("discord"),
                    "🟣 Discord"
                }
            }

            // Tab Content
            div { class: "flex-1 overflow-y-auto",
                if active_tab() == "email" {
                    EmailForm { config, on_change: move |email_cfg| {
                        let mut new_config = config();
                        new_config.email = Some(email_cfg);
                        config.set(new_config);
                    } }
                }
                if active_tab() == "slack" {
                    SlackForm { config, on_change: move |slack_cfg| {
                        let mut new_config = config();
                        new_config.slack = Some(slack_cfg);
                        config.set(new_config);
                    } }
                }
                if active_tab() == "telegram" {
                    TelegramForm { config, on_change: move |telegram_cfg| {
                        let mut new_config = config();
                        new_config.telegram = Some(telegram_cfg);
                        config.set(new_config);
                    } }
                }
                if active_tab() == "discord" {
                    DiscordForm { config, on_change: move |discord_cfg| {
                        let mut new_config = config();
                        new_config.discord = Some(discord_cfg);
                        config.set(new_config);
                    } }
                }
            }

            // Save Button
            div { class: "flex flex-col gap-3 pt-3 border-t border-slate-200 dark:border-slate-700",
                button {
                    class: "btn btn-primary flex-1 text-sm py-2",
                    onclick: handle_save_config,
                    "{texts.integ_save_button}"
                }
                button {
                    class: "btn btn-secondary flex-1 text-sm py-2",
                    onclick: handle_test_config,
                    disabled: testing(),
                    if testing() {
                        "{texts.integ_testing_button}"
                    } else {
                        "{texts.integ_test_button}"
                    }
                }
            }

            // Test Results
            if !test_results().is_empty() {
                div { class: "p-4 rounded-lg bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800",
                    p { class: "text-blue-800 dark:text-blue-300 whitespace-pre-wrap", "{test_results}" }
                }
            }
        }
    }
}

/// Email Configuration Form
#[component]
fn EmailForm(
    config: Signal<IntegrationConfig>,
    on_change: EventHandler<EmailIntegrationConfig>,
) -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let email_config = config().email.unwrap_or(EmailIntegrationConfig {
        enabled: false,
        smtp_server: "smtp.gmail.com".to_string(),
        smtp_port: 587,
        sender_email: String::new(),
        sender_password: String::new(),
        recipient_email: String::new(),
        use_tls: true,
    });

    let mut enabled = use_signal(|| email_config.enabled);
    let mut smtp_server = use_signal(|| email_config.smtp_server.clone());
    let mut smtp_port = use_signal(|| email_config.smtp_port.to_string());
    let mut sender_email = use_signal(|| email_config.sender_email.clone());
    let mut sender_password = use_signal(|| email_config.sender_password.clone());
    let mut recipient_email = use_signal(|| email_config.recipient_email.clone());
    let mut use_tls = use_signal(|| email_config.use_tls);

    let handle_update = move |_| {
        let port = smtp_port().parse().unwrap_or(587);
        on_change.call(EmailIntegrationConfig {
            enabled: enabled(),
            smtp_server: smtp_server(),
            smtp_port: port,
            sender_email: sender_email(),
            sender_password: sender_password(),
            recipient_email: recipient_email(),
            use_tls: use_tls(),
        });
    };

    let handle_enable_change = move |_| {
        enabled.toggle();
        let port = smtp_port().parse().unwrap_or(587);
        on_change.call(EmailIntegrationConfig {
            enabled: enabled(),
            smtp_server: smtp_server(),
            smtp_port: port,
            sender_email: sender_email(),
            sender_password: sender_password(),
            recipient_email: recipient_email(),
            use_tls: use_tls(),
        });
    };

    rsx! {
        div { class: "space-y-4 p-4 bg-white dark:bg-slate-800 rounded-lg border border-slate-200 dark:border-slate-700",
            // Enable Toggle
            div { class: "flex items-center justify-between",
                label { class: "text-lg font-semibold text-slate-700 dark:text-slate-300", "{texts.integ_enable_email}" }
                    label { class: "text-base font-semibold text-slate-700 dark:text-slate-300", "{texts.integ_enable_email}" }
                input {
                    r#type: "checkbox",
                    class: "checkbox",
                    checked: enabled(),
                    onchange: handle_enable_change
                }
            }

            if enabled() {
                // Servidor SMTP
                div {
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "{texts.integ_smtp_server}" }
                    input {
                        r#type: "text",
                        placeholder: "smtp.gmail.com",
                        class: "input input-bordered w-full",
                        value: smtp_server(),
                        onchange: move |e| {
                            smtp_server.set(e.value());
                            handle_update(e);
                        }
                    }
                    p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1",
                        "Gmail: smtp.gmail.com, Outlook: smtp-mail.outlook.com"
                    }
                }

                // Porta SMTP
                div {
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "{texts.integ_smtp_port}" }
                    input {
                        r#type: "number",
                        placeholder: "587",
                        class: "input input-bordered w-full",
                        value: smtp_port(),
                        onchange: move |e| {
                            smtp_port.set(e.value());
                            handle_update(e);
                        }
                    }
                    p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1",
                        "{texts.integ_smtp_port_hint}"
                    }
                }

                // Email do Remetente
                div {
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "{texts.integ_sender_email}" }
                    input {
                        r#type: "email",
                        placeholder: "{texts.integ_sender_email_placeholder}",
                        class: "input input-bordered w-full",
                        value: sender_email(),
                        onchange: move |e| {
                            sender_email.set(e.value());
                            handle_update(e);
                        }
                    }
                }

                // Senha/Token
                div {
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "{texts.integ_app_password}" }
                    input {
                        r#type: "password",
                        placeholder: "{texts.integ_app_password_placeholder}",
                        class: "input input-bordered w-full",
                        value: sender_password(),
                        onchange: move |e| {
                            sender_password.set(e.value());
                            handle_update(e);
                        }
                    }
                    p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1",
                        "{texts.integ_app_password_hint}"
                    }
                }

                // Email do Destinatário
                div {
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "{texts.integ_recipient_email}" }
                    input {
                        r#type: "email",
                        placeholder: "{texts.integ_recipient_email_placeholder}",
                        class: "input input-bordered w-full",
                        value: recipient_email(),
                        onchange: move |e| {
                            recipient_email.set(e.value());
                            handle_update(e);
                        }
                    }
                }

                // Use TLS
                div { class: "flex items-center justify-between",
                    label { class: "font-medium text-slate-700 dark:text-slate-300", "{texts.integ_use_tls}" }
                    input {
                        r#type: "checkbox",
                        class: "checkbox",
                        checked: use_tls(),
                        onchange: move |_| {
                            use_tls.toggle();
                            let port = smtp_port().parse().unwrap_or(587);
                            on_change.call(EmailIntegrationConfig {
                                enabled: enabled(),
                                smtp_server: smtp_server(),
                                smtp_port: port,
                                sender_email: sender_email(),
                                sender_password: sender_password(),
                                recipient_email: recipient_email(),
                                use_tls: use_tls(),
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Slack Configuration Form
#[component]
fn SlackForm(
    config: Signal<IntegrationConfig>,
    on_change: EventHandler<SlackIntegrationConfig>,
) -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let slack_config = config().slack.unwrap_or(SlackIntegrationConfig {
        enabled: false,
        webhook_url: String::new(),
        mention_user: None,
    });

    let mut enabled = use_signal(|| slack_config.enabled);
    let mut webhook_url = use_signal(|| slack_config.webhook_url.clone());
    let mut mention_user = use_signal(|| slack_config.mention_user.clone().unwrap_or_default());

    let handle_update = move |_| {
        on_change.call(SlackIntegrationConfig {
            enabled: enabled(),
            webhook_url: webhook_url(),
            mention_user: if mention_user().is_empty() {
                None
            } else {
                Some(mention_user())
            },
        });
    };

    let handle_enable_change = move |_| {
        enabled.toggle();
        on_change.call(SlackIntegrationConfig {
            enabled: enabled(),
            webhook_url: webhook_url(),
            mention_user: if mention_user().is_empty() {
                None
            } else {
                Some(mention_user())
            },
        });
    };

    rsx! {
    div { class: "space-y-4 p-4 bg-white dark:bg-slate-800 rounded-lg border border-slate-200 dark:border-slate-700",
        // Enable Toggle
        div { class: "flex items-center justify-between",
            label { class: "text-lg font-semibold text-slate-700 dark:text-slate-300", "{texts.integ_enable_slack}" }
            input {
                r#type: "checkbox",
                class: "checkbox",
                checked: enabled(),
                onchange: handle_enable_change
            }
        }

            if enabled() {
                // Webhook URL
                div {
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "Webhook URL" }
                    input {
                        r#type: "text",
                        placeholder: "https://hooks.slack.com/services/T.../B.../...",
                        class: "input input-bordered w-full",
                        value: webhook_url(),
                        onchange: move |e| {
                            webhook_url.set(e.value());
                            handle_update(e);
                        }
                    }
                    p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1",
                        "{texts.integ_slack_webhook_hint}"
                    }
                }

                // Mention User (Optional)
                div {
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "{texts.integ_slack_mention_user}" }
                    input {
                        r#type: "text",
                        placeholder: "{texts.integ_slack_mention_placeholder}",
                        class: "input input-bordered w-full",
                        value: mention_user(),
                        onchange: move |e| {
                            mention_user.set(e.value());
                            handle_update(e);
                        }
                    }
                    p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1",
                        "{texts.integ_slack_mention_hint}"
                    }
                }
            }
        }
    }
}

/// Telegram Configuration Form
#[component]
fn TelegramForm(
    config: Signal<IntegrationConfig>,
    on_change: EventHandler<TelegramIntegrationConfig>,
) -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let telegram_config = config().telegram.unwrap_or(TelegramIntegrationConfig {
        enabled: false,
        bot_token: String::new(),
        chat_id: String::new(),
    });

    let mut enabled = use_signal(|| telegram_config.enabled);
    let mut bot_token = use_signal(|| telegram_config.bot_token.clone());
    let mut chat_id = use_signal(|| telegram_config.chat_id.clone());

    let handle_update = move |_| {
        on_change.call(TelegramIntegrationConfig {
            enabled: enabled(),
            bot_token: bot_token(),
            chat_id: chat_id(),
        });
    };

    let handle_enable_change = move |_| {
        enabled.toggle();
        on_change.call(TelegramIntegrationConfig {
            enabled: enabled(),
            bot_token: bot_token(),
            chat_id: chat_id(),
        });
    };

    rsx! {
        div { class: "space-y-4 p-4 bg-white dark:bg-slate-800 rounded-lg border border-slate-200 dark:border-slate-700",
            // Enable Toggle
            div { class: "flex items-center justify-between",
                label { class: "text-lg font-semibold text-slate-700 dark:text-slate-300", "{texts.integ_enable_telegram}" }
                input {
                    r#type: "checkbox",
                    class: "checkbox",
                    checked: enabled(),
                    onchange: handle_enable_change
                }
            }

            if enabled() {
                // Bot Token
                div {
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "{texts.integ_tg_bot_token}" }
                    input {
                        r#type: "password",
                        placeholder: "123456789:ABCdefGHIjklmnoPQRstuvWXYZ",
                        class: "input input-bordered w-full",
                        value: bot_token(),
                        onchange: move |e| {
                            bot_token.set(e.value());
                            handle_update(e);
                        }
                    }
                    p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1",
                        "{texts.integ_tg_bot_token_hint}"
                    }
                }

                // Chat ID
                div {
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "{texts.integ_tg_chat_id}" }
                    input {
                        r#type: "text",
                        placeholder: "-1001234567890",
                        class: "input input-bordered w-full",
                        value: chat_id(),
                        onchange: move |e| {
                            chat_id.set(e.value());
                            handle_update(e);
                        }
                    }
                    p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1",
                        "{texts.integ_tg_chat_id_hint}"
                    }
                }

                // Help section
                div { class: "p-4 rounded-lg bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800",
                    p { class: "text-sm font-semibold text-blue-900 dark:text-blue-300 mb-2", "{texts.integ_tg_howto_title}" }
                    ul { class: "text-xs text-blue-800 dark:text-blue-400 space-y-1 list-disc list-inside",
                        li { "{texts.integ_tg_howto_1}" }
                        li { "{texts.integ_tg_howto_2}" }
                        li { "{texts.integ_tg_howto_3}" }
                        li { "{texts.integ_tg_howto_4}" }
                        li { "{texts.integ_tg_howto_5}" }
                        li { "{texts.integ_tg_howto_6}" }
                    }
                }

                // Troubleshooting section
                div { class: "p-4 rounded-lg bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800",
                    p { class: "text-sm font-semibold text-yellow-900 dark:text-yellow-300 mb-2", "{texts.integ_tg_troubleshooting_title}" }
                    ul { class: "text-xs text-yellow-800 dark:text-yellow-400 space-y-1 list-disc list-inside",
                        li { "{texts.integ_tg_trouble_1}" }
                        li { "{texts.integ_tg_trouble_2}" }
                        li { "{texts.integ_tg_trouble_3}" }
                        li { "{texts.integ_tg_trouble_4}" }
                    }
                }
            }
        }
    }
}

/// Discord Configuration Form
#[component]
fn DiscordForm(
    config: Signal<IntegrationConfig>,
    on_change: EventHandler<DiscordIntegrationConfig>,
) -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let discord_config = config().discord.unwrap_or(DiscordIntegrationConfig {
        enabled: false,
        webhook_url: String::new(),
    });

    let mut enabled = use_signal(|| discord_config.enabled);
    let mut webhook_url = use_signal(|| discord_config.webhook_url.clone());

    let handle_update = move |_| {
        on_change.call(DiscordIntegrationConfig {
            enabled: enabled(),
            webhook_url: webhook_url(),
        });
    };

    let handle_enable_change = move |_| {
        enabled.toggle();
        on_change.call(DiscordIntegrationConfig {
            enabled: enabled(),
            webhook_url: webhook_url(),
        });
    };

    rsx! {
        div { class: "space-y-4 p-4 bg-white dark:bg-slate-800 rounded-lg border border-slate-200 dark:border-slate-700",
            // Enable Toggle
            div { class: "flex items-center justify-between",
                label { class: "text-lg font-semibold text-slate-700 dark:text-slate-300", "{texts.integ_enable_discord}" }
                input {
                    r#type: "checkbox",
                    class: "checkbox",
                    checked: enabled(),
                    onchange: handle_enable_change
                }
            }

            if enabled() {
                // Webhook URL
                div {
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "Webhook URL" }
                    input {
                        r#type: "text",
                        placeholder: "https://discord.com/api/webhooks/123456789/ABC...",
                        class: "input input-bordered w-full",
                        value: webhook_url(),
                        onchange: move |e| {
                            webhook_url.set(e.value());
                            handle_update(e);
                        }
                    }
                    p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1",
                        "{texts.integ_discord_webhook_hint}"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_config_is_loaded_back_from_the_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("integrations.json");
        let mut cfg = IntegrationConfig::load_from(&path);
        assert!(cfg.to_chat_integrations().is_empty());
        cfg.discord = Some(DiscordIntegrationConfig {
            enabled: true,
            webhook_url: "https://discord.example/hook".into(),
        });
        cfg.save_to(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert_eq!(
            IntegrationConfig::load_from(&path)
                .to_chat_integrations()
                .len(),
            1
        );
        assert!(IntegrationConfig::default_path().ends_with(".rs-shield/integrations.json"));
    }
}
