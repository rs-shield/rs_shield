use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPasskey {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub passkey: Vec<u8>,
    pub counter: u32,
    pub created_at: String,
    pub last_used: Option<String>,
}
