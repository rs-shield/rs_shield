use serde::{Deserialize, Serialize};
use std::fs;

use crate::ui::i18n::{Language, Theme};
use rsb_sdk::utils::ensure_directory_exists;

/// Estrutura para armazenar preferências globais do aplicativo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppPreferences {
    pub language: Language,
    pub theme: Theme,
    pub exclude_patterns: String,
    pub encrypt_patterns: String,
    pub backup_mode: String,
    pub pause_on_low_battery: u8,
    pub pause_on_high_cpu: u8,
    pub compression_level: u8,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            language: Language::Pt,
            theme: Theme::System,
            exclude_patterns: rsb_sdk::config::DEFAULT_EXCLUDE_PATTERNS.join("\n"),
            encrypt_patterns: "*.pdf\n*.docx\nprivate/".to_string(),
            backup_mode: "incremental".to_string(),
            pause_on_low_battery: 20,
            pause_on_high_cpu: 90,
            compression_level: 3,
        }
    }
}

impl AppPreferences {
    /// Obter o caminho do arquivo de preferências
    fn get_preferences_path() -> std::path::PathBuf {
        if let Some(home) = dirs::home_dir() {
            home.join(".rs-shield").join("app_preferences.json")
        } else {
            std::path::PathBuf::from(".rs-shield/app_preferences.json")
        }
    }

    /// Carregar preferências do arquivo
    pub fn load() -> Self {
        let path = Self::get_preferences_path();

        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(prefs) = serde_json::from_str::<AppPreferences>(&content) {
                return prefs;
            }
        }

        Self::default()
    }

    /// Salvar preferências no arquivo
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::get_preferences_path();

        // Criar diretório se não existir usando função centralizada
        if let Some(parent) = path.parent() {
            let parent_str = parent.to_string_lossy().to_string();
            ensure_directory_exists(&parent_str).map_err(|e| std::io::Error::other(e))?;
        }

        let json_str = serde_json::to_string_pretty(self)?;
        fs::write(&path, json_str)?;

        Ok(())
    }
}
