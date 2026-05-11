use crate::ui::operations::{create_operation, OperationsManager};

pub fn record_backup_operation(
    status: String,
    files_processed: usize,
    duration_secs: u64,
    backup_size: String,
    source: String,
    destination: String,
) -> Result<(), String> {
    let mut manager = OperationsManager::new();

    let operation = create_operation(
        "Backup".to_string(),
        status,
        files_processed,
        0, // files_skipped
        0, // files_with_errors
        duration_secs,
        backup_size,
        Some(source),
        Some(destination),
    );

    manager.add_operation(operation)
}

pub fn record_restore_operation(
    status: String,
    files_processed: usize,
    duration_secs: u64,
    source: String,
    destination: String,
) -> Result<(), String> {
    let mut manager = OperationsManager::new();

    let operation = create_operation(
        "Restore".to_string(),
        status,
        files_processed,
        0,
        0,
        duration_secs,
        "N/A".to_string(),
        Some(source),
        Some(destination),
    );

    manager.add_operation(operation)
}

pub fn record_verify_operation(
    status: String,
    files_processed: usize,
    files_with_errors: usize,
    duration_secs: u64,
    backup_path: String,
) -> Result<(), String> {
    let mut manager = OperationsManager::new();

    let operation = create_operation(
        "Verify".to_string(),
        status,
        files_processed,
        0,
        files_with_errors,
        duration_secs,
        "N/A".to_string(),
        Some(backup_path),
        None,
    );

    manager.add_operation(operation)
}

pub fn record_prune_operation(
    status: String,
    files_removed: usize,
    duration_secs: u64,
    space_freed: String,
    backup_path: String,
) -> Result<(), String> {
    let mut manager = OperationsManager::new();

    let operation = create_operation(
        "Prune".to_string(),
        status,
        files_removed,
        0,
        0,
        duration_secs,
        format!("Liberado: {}", space_freed),
        Some(backup_path),
        None,
    );

    manager.add_operation(operation)
}
pub async fn record_schedule_operation(
    success: bool,
    error: Option<String>,
    duration_secs: Option<u64>,
) -> Result<(), String> {
    let mut manager = OperationsManager::new();

    let status = if success {
        "success".to_string()
    } else {
        format!("failed: {:?}", error)
    };
    let duration = duration_secs.unwrap_or(0);

    let operation = create_operation(
        "Schedule".to_string(),
        status,
        0,
        0,
        if error.is_some() { 1 } else { 0 },
        duration,
        "N/A".to_string(),
        None,
        None,
    );

    manager.add_operation(operation)
}
