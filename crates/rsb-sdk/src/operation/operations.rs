use crate::utils::ensure_directory_exists;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub id: String,
    pub operation_type: String,
    pub timestamp: String,
    pub duration_secs: u64,
    pub status: String,
    pub files_processed: usize,
    pub files_skipped: usize,
    pub files_with_errors: usize,
    pub backup_size: String, // "1.5 GB", etc
    pub source_path: Option<String>,
    pub destination_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OperationsHistory {
    pub operations: Vec<Operation>,
}

impl OperationsHistory {
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    pub fn add_operation(&mut self, operation: Operation) {
        self.operations.push(operation);
        self.sort_by_timestamp();
    }

    fn sort_by_timestamp(&mut self) {
        self.operations
            .sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    }

    pub fn get_operations_count(&self, op_type: &str) -> usize {
        self.operations
            .iter()
            .filter(|op| op.operation_type == op_type)
            .count()
    }

    pub fn get_successful_operations_count(&self, op_type: &str) -> usize {
        self.operations
            .iter()
            .filter(|op| op.operation_type == op_type && op.status == "Sucesso")
            .count()
    }

    pub fn get_last_operation_time(&self, op_type: Option<&str>) -> String {
        match op_type {
            Some(op_type) => self
                .operations
                .iter()
                .find(|op| op.operation_type == op_type)
                .map(|op| format_relative_time(&op.timestamp))
                .unwrap_or_else(|| "Nunca".to_string()),
            None => self
                .operations
                .first()
                .map(|op| format_relative_time(&op.timestamp))
                .unwrap_or_else(|| "Nunca".to_string()),
        }
    }

    pub fn get_total_operations(&self) -> usize {
        self.operations.len()
    }

    pub fn get_recent_operations(&self, limit: usize) -> Vec<Operation> {
        self.operations.iter().take(limit).cloned().collect()
    }
}

pub struct OperationsManager {
    history_file: PathBuf,
    history: OperationsHistory,
}

impl OperationsManager {
    pub fn new() -> Self {
        let history_file = get_history_file_path();
        let history = Self::load_history(&history_file);

        Self {
            history_file,
            history,
        }
    }

    pub fn load_history(history_file: &PathBuf) -> OperationsHistory {
        match fs::read_to_string(history_file) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(history) => history,
                Err(_) => OperationsHistory::new(),
            },
            Err(_) => OperationsHistory::new(),
        }
    }

    pub fn add_operation(&mut self, operation: Operation) -> Result<(), String> {
        self.history.add_operation(operation);
        self.save_history()
    }

    pub fn save_history(&self) -> Result<(), String> {
        match serde_json::to_string_pretty(&self.history) {
            Ok(json) => match fs::write(&self.history_file, json) {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("Erro ao salvar histórico: {}", e)),
            },
            Err(e) => Err(format!("Erro ao serializar histórico: {}", e)),
        }
    }

    pub fn get_history(&self) -> &OperationsHistory {
        &self.history
    }
}

pub fn get_history_file_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".rsb-desktop").join("operations_history.json")
}

pub fn ensure_history_directory() -> Result<(), String> {
    let dir_path = get_history_file_path().parent().unwrap().to_path_buf();
    let dir_str = dir_path.to_string_lossy().to_string();
    ensure_directory_exists(&dir_str)?;
    Ok(())
}

pub fn create_operation(
    operation_type: String,
    status: String,
    files_processed: usize,
    files_skipped: usize,
    files_with_errors: usize,
    duration_secs: u64,
    backup_size: String,
    source_path: Option<String>,
    destination_path: Option<String>,
) -> Operation {
    Operation {
        id: format!("{}", chrono::Utc::now().timestamp_millis()),
        operation_type,
        timestamp: chrono::Local::now().to_rfc3339(),
        duration_secs,
        status,
        files_processed,
        files_skipped,
        files_with_errors,
        backup_size,
        source_path,
        destination_path,
    }
}

fn format_relative_time(iso_timestamp: &str) -> String {
    match DateTime::parse_from_rfc3339(iso_timestamp) {
        Ok(dt) => {
            let local_time: DateTime<Local> = dt.with_timezone(&Local);
            let now = Local::now();
            let duration = now.signed_duration_since(local_time);

            if duration.num_seconds() < 60 {
                "Agora".to_string()
            } else if duration.num_minutes() < 60 {
                format!("Há {} min", duration.num_minutes())
            } else if duration.num_hours() < 24 {
                format!("Há {} h", duration.num_hours())
            } else if duration.num_days() < 7 {
                format!("Há {} dias", duration.num_days())
            } else {
                local_time.format("%d/%m/%Y").to_string()
            }
        }
        Err(_) => "Desconhecido".to_string(),
    }
}
