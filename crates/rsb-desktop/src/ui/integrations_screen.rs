use dioxus::prelude::*;
use rsb_sdk::utils::ensure_directory_exists;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
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
    /// Carregar configurações do arquivo de profile
    pub fn load(profile_path: &Path) -> Self {
        let config_dir = match profile_path.parent() {
            Some(p) => p,
            None => std::path::Path::new("."),
        };
        let config_path = config_dir.join("integrations.json");

        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                if let Ok(config) = serde_json::from_str(&content) {
                    return config;
                }
            }
        }

        Self {
            email: None,
            slack: None,
            telegram: None,
            discord: None,
        }
    }

    /// Salvar configurações no arquivo de profile
    pub fn save(&self, profile_path: &Path) -> std::io::Result<()> {
        let config_dir = match profile_path.parent() {
            Some(p) => p,
            None => std::path::Path::new("."),
        };
        let config_path = config_dir.join("integrations.json");

        // Criar diretório se não existir
        if let Some(dir_str) = config_dir.to_str() {
            let _ = ensure_directory_exists(dir_str);
        }

        let json = serde_json::to_string_pretty(self)?;
        fs::write(config_path, json)?;
        Ok(())
    }
}

/// Componente de tela de integrações
#[component]
pub fn IntegrationScreen() -> Element {
    // Expandir ~ para o caminho real do home
    let profile_path = if let Some(home) = dirs::home_dir() {
        home.join(".rs-shield").join("default.toml")
    } else {
        std::path::PathBuf::from("~/.rs-shield/default.toml")
    };

    let mut config = use_signal(|| IntegrationConfig::load(&profile_path));
    let mut active_tab = use_signal(|| "email");
    let mut status_msg = use_signal(String::new);
    let mut show_status = use_signal(|| false);

    let handle_save_config = move |_| {
        if let Ok(()) = config().save(&profile_path) {
            status_msg.set("✅ Configurações salvas com sucesso!".to_string());
            show_status.set(true);

            // Auto-hide após 3 segundos
            spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                show_status.set(false);
            });
        } else {
            status_msg.set("❌ Erro ao salvar configurações".to_string());
            show_status.set(true);
        }
    };

    rsx! {
        div { class: "flex flex-col h-full gap-6 p-6 bg-slate-50 dark:bg-slate-900",
            // Header
            div {
                h1 { class: "text-3xl font-bold text-slate-900 dark:text-slate-100 mb-2", "⚙️ Integrações" }
                p { class: "text-slate-600 dark:text-slate-400",
                    "Configure notificações por Email, Slack, Telegram e Discord"
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
            div { class: "flex gap-4 pt-4 border-t border-slate-200 dark:border-slate-700",
                button {
                    class: "btn btn-primary flex-1",
                    onclick: handle_save_config,
                    "💾 Salvar Configurações"
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
        div { class: "space-y-6 p-6 bg-white dark:bg-slate-800 rounded-lg border border-slate-200 dark:border-slate-700",
            // Enable Toggle
            div { class: "flex items-center justify-between",
                label { class: "text-lg font-semibold text-slate-700 dark:text-slate-300", "Ativar Email" }
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
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "Servidor SMTP" }
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
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "Porta SMTP" }
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
                        "Geralmente 587 (TLS) ou 465 (SSL)"
                    }
                }

                // Email do Remetente
                div {
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "Email do Remetente" }
                    input {
                        r#type: "email",
                        placeholder: "seu_email@gmail.com",
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
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "Senha/Token de App" }
                    input {
                        r#type: "password",
                        placeholder: "Senha ou App Password",
                        class: "input input-bordered w-full",
                        value: sender_password(),
                        onchange: move |e| {
                            sender_password.set(e.value());
                            handle_update(e);
                        }
                    }
                    p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1",
                        "Gmail: Use App Password (não a senha da conta)"
                    }
                }

                // Email do Destinatário
                div {
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "Email do Destinatário" }
                    input {
                        r#type: "email",
                        placeholder: "destino@example.com",
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
                    label { class: "font-medium text-slate-700 dark:text-slate-300", "Usar TLS" }
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
    div { class: "space-y-6 p-6 bg-white dark:bg-slate-800 rounded-lg border border-slate-200 dark:border-slate-700",
        // Enable Toggle
        div { class: "flex items-center justify-between",
            label { class: "text-lg font-semibold text-slate-700 dark:text-slate-300", "Ativar Slack" }
            input {
                r#type: "checkbox",
                class: "checkbox",
                checked: enabled(),
                onchange: handle_enable_change
            }
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
                    "Obtenha em: api.slack.com → Incoming Webhooks"
                }
            }

            // Mention User (Optional)
            div {
                label { class: "label font-medium text-slate-700 dark:text-slate-300", "Mencionar Usuário (Opcional)" }
                input {
                    r#type: "text",
                    placeholder: "U123456 ou nome_usuario",
                    class: "input input-bordered w-full",
                    value: mention_user(),
                    onchange: move |e| {
                        mention_user.set(e.value());
                        handle_update(e);
                    }
                }
                p { class: "text-xs text-slate-500 dark:text-slate-400 mt-1",
                    "ID do usuário ou @username"
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
        div { class: "space-y-6 p-6 bg-white dark:bg-slate-800 rounded-lg border border-slate-200 dark:border-slate-700",
            // Enable Toggle
            div { class: "flex items-center justify-between",
                label { class: "text-lg font-semibold text-slate-700 dark:text-slate-300", "Ativar Telegram" }
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
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "Token do Bot" }
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
                        "Obtenha com @BotFather no Telegram"
                    }
                }

                // Chat ID
                div {
                    label { class: "label font-medium text-slate-700 dark:text-slate-300", "ID do Chat" }
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
                        "Envie /start ao bot e acesse api.telegram.org/botTOKEN/getUpdates"
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
        div { class: "space-y-6 p-6 bg-white dark:bg-slate-800 rounded-lg border border-slate-200 dark:border-slate-700",
            // Enable Toggle
            div { class: "flex items-center justify-between",
                label { class: "text-lg font-semibold text-slate-700 dark:text-slate-300", "Ativar Discord" }
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
                        "Clique direito no canal → Editar → Integrações → Webhooks"
                    }
                }
            }
        }
    }
}
