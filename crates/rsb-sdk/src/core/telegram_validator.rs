use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

/// Response from Telegram getMe method
#[derive(Debug, Deserialize)]
pub struct TelegramGetMeResponse {
    pub ok: bool,
    pub result: Option<TelegramBot>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TelegramBot {
    pub id: u64,
    pub is_bot: bool,
    pub first_name: String,
    pub username: Option<String>,
}

/// Response from Telegram getUpdates
#[derive(Debug, Deserialize)]
pub struct TelegramUpdatesResponse {
    pub ok: bool,
    pub result: Option<Vec<TelegramUpdate>>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TelegramUpdate {
    pub update_id: u64,
    pub message: Option<TelegramMessage>,
}

#[derive(Debug, Deserialize)]
pub struct TelegramMessage {
    pub message_id: u64,
    pub chat: TelegramChat,
}

#[derive(Debug, Deserialize)]
pub struct TelegramChat {
    pub id: i64,
    pub r#type: String,
}

/// Response from Telegram sendMessage
#[derive(Debug, Deserialize)]
pub struct TelegramSendResponse {
    pub ok: bool,
    pub result: Option<serde_json::Value>,
    pub error_code: Option<u32>,
    pub description: Option<String>,
}

/// Validate Telegram bot configuration
pub async fn validate_telegram_token(bot_token: &str) -> Result<TelegramBot, String> {
    if bot_token.is_empty() {
        return Err("Bot token is empty".to_string());
    }

    let url = format!("https://api.telegram.org/bot{}/getMe", bot_token);
    let client = Client::new();

    match client.get(&url).send().await {
        Ok(response) => {
            match response.json::<TelegramGetMeResponse>().await {
                Ok(data) => {
                    if data.ok {
                        if let Some(bot) = data.result {
                            info!(
                                "✅ Telegram bot validated: {} (@{})",
                                bot.first_name,
                                bot.username.as_deref().unwrap_or("unknown")
                            );
                            Ok(bot)
                        } else {
                            Err("Invalid response format from Telegram".to_string())
                        }
                    } else {
                        let desc = data.description.unwrap_or_default();
                        error!("❌ Telegram validation failed: {}", desc);
                        Err(format!(
                            "Telegram returned error: {}",
                            desc
                        ))
                    }
                }
                Err(e) => {
                    error!("❌ Failed to parse Telegram response: {}", e);
                    Err(format!("Failed to parse Telegram response: {}", e))
                }
            }
        }
        Err(e) => {
            error!("❌ Failed to connect to Telegram: {}", e);
            Err(format!("Connection error: {}", e))
        }
    }
}

/// Get chat ID from Telegram by reading recent messages
pub async fn get_telegram_chat_id(bot_token: &str) -> Result<Vec<(i64, String)>, String> {
    if bot_token.is_empty() {
        return Err("Bot token is empty".to_string());
    }

    let url = format!("https://api.telegram.org/bot{}/getUpdates", bot_token);
    let client = Client::new();

    match client.get(&url).send().await {
        Ok(response) => {
            match response.json::<TelegramUpdatesResponse>().await {
                Ok(data) => {
                    if data.ok {
                        if let Some(updates) = data.result {
                            let mut chat_ids = Vec::new();
                            for update in updates {
                                if let Some(message) = update.message {
                                    let chat_type = message.chat.r#type.clone();
                                    chat_ids.push((message.chat.id, chat_type));
                                }
                            }

                            if chat_ids.is_empty() {
                                Err("No messages found. Send /start to the bot first.".to_string())
                            } else {
                                info!("✅ Found {} chat(s)", chat_ids.len());
                                Ok(chat_ids)
                            }
                        } else {
                            Err("No updates available. Send /start to the bot first.".to_string())
                        }
                    } else {
                        let desc = data.description.unwrap_or_default();
                        Err(format!("Telegram error: {}", desc))
                    }
                }
                Err(e) => Err(format!("Failed to parse response: {}", e)),
            }
        }
        Err(e) => Err(format!("Connection error: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telegram_response_parsing() {
        let json = r#"{
            "ok": true,
            "result": {
                "id": 123456789,
                "is_bot": true,
                "first_name": "MyBot",
                "username": "my_test_bot"
            }
        }"#;

        let response: TelegramGetMeResponse = serde_json::from_str(json).unwrap();
        assert!(response.ok);
        assert!(response.result.is_some());
    }

    #[test]
    fn test_telegram_error_response_parsing() {
        let json = r#"{
            "ok": false,
            "error_code": 401,
            "description": "Unauthorized"
        }"#;

        let response: TelegramGetMeResponse = serde_json::from_str(json).unwrap();
        assert!(!response.ok);
    }
}
