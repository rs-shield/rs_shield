#[test]
fn test_report_structure() {
    use rsb_sdk::report::ReportData;
    use std::time::Duration;

    let report = ReportData {
        operation: "Backup".to_string(),
        profile_path: "/path/to/profile".to_string(),
        timestamp: "2026-02-07T10:30:00".to_string(),
        duration: Duration::from_secs(120),
        mode: Some("incremental".to_string()),
        files_processed: 150,
        files_skipped: 25,
        files_with_errors: 2,
        total_files: 177,
        errors: vec![
            "File not found: /path/to/file1".to_string(),
            "Permission denied: /path/to/file2".to_string(),
        ],
        status: "Success".to_string(),
    };

    assert_eq!(report.operation, "Backup");
    assert_eq!(report.files_processed, 150);
    assert_eq!(report.files_skipped, 25);
    assert_eq!(report.files_with_errors, 2);
    assert_eq!(report.total_files, 177);
    assert_eq!(report.errors.len(), 2);
    assert_eq!(report.status, "Success");
    assert_eq!(report.duration.as_secs(), 120);
}

#[test]
fn test_report_no_errors() {
    use rsb_sdk::report::ReportData;
    use std::time::Duration;

    let report = ReportData {
        operation: "Verify".to_string(),
        profile_path: "/path/to/profile".to_string(),
        timestamp: "2026-02-07T10:35:00".to_string(),
        duration: Duration::from_secs(60),
        mode: Some("full".to_string()),
        files_processed: 500,
        files_skipped: 0,
        files_with_errors: 0,
        total_files: 500,
        errors: vec![],
        status: "Success".to_string(),
    };

    assert!(
        report.errors.is_empty(),
        "Report with no errors should have empty error vector"
    );
    assert_eq!(report.files_with_errors, 0);
}

#[test]
fn test_report_restore_operation() {
    use rsb_sdk::report::ReportData;
    use std::time::Duration;

    let report = ReportData {
        operation: "Restore".to_string(),
        profile_path: "/path/to/profile".to_string(),
        timestamp: "2026-02-07T11:00:00".to_string(),
        duration: Duration::from_secs(300),
        mode: None,
        files_processed: 200,
        files_skipped: 10,
        files_with_errors: 1,
        total_files: 211,
        errors: vec!["Failed to restore: /path/to/large/file".to_string()],
        status: "Completed with warnings".to_string(),
    };

    assert_eq!(report.operation, "Restore");
    assert_eq!(report.mode, None, "Restore operation may not have a mode");
    assert_eq!(report.status, "Completed with warnings");
}

#[test]
fn test_report_prune_operation() {
    use rsb_sdk::report::ReportData;
    use std::time::Duration;

    let report = ReportData {
        operation: "Prune".to_string(),
        profile_path: "/path/to/profile".to_string(),
        timestamp: "2026-02-07T11:30:00".to_string(),
        duration: Duration::from_secs(45),
        mode: None,
        files_processed: 50,
        files_skipped: 0,
        files_with_errors: 0,
        total_files: 50,
        errors: vec![],
        status: "Success".to_string(),
    };

    assert_eq!(report.operation, "Prune");
    // All files processed successfully
    assert_eq!(
        report.files_processed + report.files_skipped,
        report.total_files
    );
}

#[test]
fn test_report_with_duration() {
    use rsb_sdk::report::ReportData;
    use std::time::Duration;

    let duration = Duration::from_secs(3661); // 1 hour, 1 minute, 1 second
    let report = ReportData {
        operation: "Backup".to_string(),
        profile_path: "/path".to_string(),
        timestamp: "2026-02-07T12:00:00".to_string(),
        duration,
        mode: Some("full".to_string()),
        files_processed: 1000,
        files_skipped: 100,
        files_with_errors: 5,
        total_files: 1105,
        errors: vec![],
        status: "Success".to_string(),
    };

    assert_eq!(report.duration.as_secs(), 3661);
    assert!(
        report.duration.as_secs() > 3600,
        "Duration should be over 1 hour"
    );
}

#[test]
fn test_report_multiple_errors() {
    use rsb_sdk::report::ReportData;
    use std::time::Duration;

    let errors = vec![
        "Error 1: File locked".to_string(),
        "Error 2: Permission denied".to_string(),
        "Error 3: Disk full".to_string(),
        "Error 4: Invalid path".to_string(),
    ];

    let report = ReportData {
        operation: "Backup".to_string(),
        profile_path: "/path".to_string(),
        timestamp: "2026-02-07T12:30:00".to_string(),
        duration: Duration::from_secs(180),
        mode: Some("incremental".to_string()),
        files_processed: 500,
        files_skipped: 50,
        files_with_errors: 4,
        total_files: 554,
        errors: errors.clone(),
        status: "Failed".to_string(),
    };

    assert_eq!(report.errors.len(), 4);
    assert!(report.errors.contains(&"Error 1: File locked".to_string()));
    assert_eq!(report.files_with_errors, report.errors.len() as usize);
}

#[test]
fn test_report_timestamps() {
    use rsb_sdk::report::ReportData;
    use std::time::Duration;

    let timestamps = vec![
        "2026-02-07T08:00:00",
        "2026-02-07T12:30:45",
        "2026-02-07T23:59:59",
    ];

    for ts in timestamps {
        let report = ReportData {
            operation: "Backup".to_string(),
            profile_path: "/path".to_string(),
            timestamp: ts.to_string(),
            duration: Duration::from_secs(60),
            mode: None,
            files_processed: 100,
            files_skipped: 10,
            files_with_errors: 0,
            total_files: 110,
            errors: vec![],
            status: "Success".to_string(),
        };

        assert_eq!(report.timestamp, ts);
    }
}

#[test]
fn test_report_zero_files() {
    use rsb_sdk::report::ReportData;
    use std::time::Duration;

    let report = ReportData {
        operation: "Backup".to_string(),
        profile_path: "/empty/path".to_string(),
        timestamp: "2026-02-07T13:00:00".to_string(),
        duration: Duration::from_secs(5),
        mode: None,
        files_processed: 0,
        files_skipped: 0,
        files_with_errors: 0,
        total_files: 0,
        errors: vec![],
        status: "No files to process".to_string(),
    };

    assert_eq!(report.total_files, 0);
    assert_eq!(report.files_processed, 0);
    assert!(report.errors.is_empty());
}
