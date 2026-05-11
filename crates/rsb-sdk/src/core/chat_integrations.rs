use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

/// Configuration for chat integrations (Slack, Telegram, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum ChatIntegration {
    Slack {
        webhook_url: String,          // Slack channel Webhook URL
        mention_user: Option<String>, // ID or @username to mention
    },
    Telegram {
        bot_token: String, // Telegram bot token
        chat_id: String,   // Chat/group ID
    },
    Discord {
        webhook_url: String, // Discord Webhook URL
    },
}

/// Formatted message for sending
#[derive(Debug, Clone, Serialize)]
struct SlackMessage {
    text: String,
    blocks: Option<Vec<SlackBlock>>,
}

#[derive(Debug, Clone, Serialize)]
struct SlackBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<SlackText>,
    fields: Option<Vec<SlackText>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    accessory: Option<SlackAccessory>,
}

#[derive(Debug, Clone, Serialize)]
struct SlackText {
    #[serde(rename = "type")]
    text_type: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    emoji: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct SlackAccessory {
    #[serde(rename = "type")]
    accessory_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<String>,
    alt_text: Option<String>,
}

/// Mensagem para Telegram
#[derive(Debug, Clone, Serialize)]
struct TelegramMessage {
    chat_id: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parse_mode: Option<String>,
}

/// Mensagem para Discord
#[derive(Debug, Clone, Serialize)]
struct DiscordMessage {
    content: String,
    embeds: Option<Vec<DiscordEmbed>>,
}

#[derive(Debug, Clone, Serialize)]
struct DiscordEmbed {
    title: String,
    description: String,
    color: u32,
    fields: Option<Vec<DiscordField>>,
    timestamp: String,
}

#[derive(Debug, Clone, Serialize)]
struct DiscordField {
    name: String,
    value: String,
    inline: bool,
}

/// Send notification via chat integration
#[allow(dead_code)]
pub async fn send_chat_notification(
    integration: &ChatIntegration,
    title: &str,
    message: &str,
    notification_type: &str, // "success", "error", "warning", "info"
) -> Result<(), Box<dyn std::error::Error>> {
    match integration {
        ChatIntegration::Slack {
            webhook_url,
            mention_user,
        } => {
            send_slack_notification(
                webhook_url,
                title,
                message,
                notification_type,
                mention_user.as_deref(),
            )
            .await
        }
        ChatIntegration::Telegram { bot_token, chat_id } => {
            send_telegram_notification(bot_token, chat_id, title, message, notification_type).await
        }
        ChatIntegration::Discord { webhook_url } => {
            send_discord_notification(webhook_url, title, message, notification_type).await
        }
    }
}

/// Send notification to Slack
async fn send_slack_notification(
    webhook_url: &str,
    title: &str,
    message: &str,
    notification_type: &str,
    mention_user: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let emoji = match notification_type {
        "success" => "✅",
        "error" => "❌",
        "warning" => "⚠️",
        _ => "ℹ️",
    };

    let mention = mention_user
        .map(|u| format!("<@{}> ", u))
        .unwrap_or_default();

    let text = if !mention.is_empty() {
        format!("{}{}{}: {}", mention, emoji, title, message)
    } else {
        format!("{}{}: {}", emoji, title, message)
    };

    let slack_msg = SlackMessage {
        text: text.clone(),
        blocks: Some(vec![
            SlackBlock {
                block_type: "divider".to_string(),
                text: None,
                fields: None,
                accessory: None,
            },
            SlackBlock {
                block_type: "section".to_string(),
                text: Some(SlackText {
                    text_type: "mrkdwn".to_string(),
                    text: format!("{}*{}*\n{}", emoji, title, message),
                    emoji: None,
                }),
                fields: None,
                accessory: None,
            },
        ]),
    };

    let client = Client::new();
    match client.post(webhook_url).json(&slack_msg).send().await {
        Ok(response) => {
            if response.status().is_success() {
                info!(
                    "✅ [Slack] Notification sent successfully to {}",
                    mention_user.unwrap_or("user")
                );
                Ok(())
            } else {
                error!("❌ [Slack] Error: {}", response.status());
                Err("Slack notification failed".into())
            }
        }
        Err(e) => {
            error!("❌ [Slack] Connection error: {}", e);
            Err(Box::new(e))
        }
    }
}

/// Send notification to Telegram
async fn send_telegram_notification(
    bot_token: &str,
    chat_id: &str,
    title: &str,
    message: &str,
    notification_type: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let emoji = match notification_type {
        "success" => "✅",
        "error" => "❌",
        "warning" => "⚠️",
        _ => "ℹ️",
    };

    let text = format!("*{}* {}\n\n{}", emoji, title, message);

    let telegram_msg = TelegramMessage {
        chat_id: chat_id.to_string(),
        text,
        parse_mode: Some("Markdown".to_string()),
    };

    let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
    let client = Client::new();

    match client.post(&url).json(&telegram_msg).send().await {
        Ok(response) => {
            if response.status().is_success() {
                info!("✅ [Telegram] Notification sent successfully");
                Ok(())
            } else {
                error!("❌ [Telegram] Error: {}", response.status());
                Err("Telegram notification failed".into())
            }
        }
        Err(e) => {
            error!("❌ [Telegram] Connection error: {}", e);
            Err(Box::new(e))
        }
    }
}

/// Send notification to Discord
async fn send_discord_notification(
    webhook_url: &str,
    title: &str,
    message: &str,
    notification_type: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (color, emoji) = match notification_type {
        "success" => (0x10b981u32, "✅"),
        "error" => (0xef4444u32, "❌"),
        "warning" => (0xf59e0bu32, "⚠️"),
        _ => (0x3b82f6u32, "ℹ️"),
    };

    let discord_msg = DiscordMessage {
        content: format!("{} **{}**: {}", emoji, title, message),
        embeds: Some(vec![DiscordEmbed {
            title: format!("{} {}", emoji, title),
            description: message.to_string(),
            color,
            fields: Some(vec![
                DiscordField {
                    name: "Color".to_string(),
                    value: format!("#{:06x}", color),
                    inline: true,
                },
                DiscordField {
                    name: "Timestamp".to_string(),
                    value: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    inline: true,
                },
                DiscordField {
                    name: "Type".to_string(),
                    value: notification_type.to_string(),
                    inline: true,
                },
            ]),
            timestamp: chrono::Local::now().to_rfc3339(),
        }]),
    };

    let client = Client::new();
    match client.post(webhook_url).json(&discord_msg).send().await {
        Ok(response) => {
            if response.status().is_success() {
                let color_str = format!("#{:06x}", color);
                info!(
                    "✅ [Discord] Notification sent successfully with color {}",
                    color_str
                );
                Ok(())
            } else {
                error!("❌ [Discord] Error: {}", response.status());
                Err("Discord notification failed".into())
            }
        }
        Err(e) => {
            error!("❌ [Discord] Connection error: {}", e);
            Err(Box::new(e))
        }
    }
}

/// Blocking version for sending notifications
#[allow(dead_code)]
pub fn send_chat_notification_blocking(
    integration: &ChatIntegration,
    title: &str,
    message: &str,
    notification_type: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(send_chat_notification(
        integration,
        title,
        message,
        notification_type,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slack_config() {
        let slack = ChatIntegration::Slack {
            webhook_url: "https://hooks.slack.com/services/...".to_string(),
            mention_user: Some("U123456".to_string()),
        };
        matches!(slack, ChatIntegration::Slack { .. });
    }

    #[test]
    fn test_telegram_config() {
        let telegram = ChatIntegration::Telegram {
            bot_token: "123:ABC".to_string(),
            chat_id: "-1001234567890".to_string(),
        };
        matches!(telegram, ChatIntegration::Telegram { .. });
    }

    #[test]
    fn test_discord_config() {
        let discord = ChatIntegration::Discord {
            webhook_url: "https://discord.com/api/webhooks/...".to_string(),
        };
        matches!(discord, ChatIntegration::Discord { .. });
    }
}
