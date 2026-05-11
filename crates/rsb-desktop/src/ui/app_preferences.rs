use serde::{Deserialize, Serialize};
use std::fs;

use crate::ui::i18n::{Language, Theme};
use rsb_sdk::utils::ensure_directory_exists;

/// Estrutura para armazenar preferências globais do aplicativo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppPreferences {
    pub language: String,
    pub theme: String,
    pub exclude_patterns: String,
    pub encrypt_patterns: String,
    pub backup_mode: String,
    pub pause_on_low_battery: String,
    pub pause_on_high_cpu: String,
    pub compression_level: String,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            language: "pt".to_string(),
            theme: "system".to_string(),
            exclude_patterns: "*.tmp\nnode_modules\n.git".to_string(),
            encrypt_patterns: "*.pdf\n*.docx\nprivate/".to_string(),
            backup_mode: "incremental".to_string(),
            pause_on_low_battery: "20".to_string(),
            pause_on_high_cpu: "90".to_string(),
            compression_level: "3".to_string(),
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

    /// Converter Language para string
    pub fn language_to_string(lang: Language) -> String {
        match lang {
            Language::En => "en".to_string(),
            Language::Pt => "pt".to_string(),
        }
    }

    /// Converter string para Language
    pub fn string_to_language(s: &str) -> Language {
        match s {
            "en" => Language::En,
            _ => Language::Pt,
        }
    }

    /// Converter Theme para string
    pub fn theme_to_string(theme: Theme) -> String {
        match theme {
            Theme::Light => "light".to_string(),
            Theme::Dark => "dark".to_string(),
            Theme::System => "system".to_string(),
        }
    }

    /// Converter string para Theme
    pub fn string_to_theme(s: &str) -> Theme {
        match s {
            "light" => Theme::Light,
            "dark" => Theme::Dark,
            _ => Theme::System,
        }
    }
}
