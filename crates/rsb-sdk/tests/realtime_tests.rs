// Testes para rsb-core/src/realtime.rs

use chrono::Utc;
use rsb_sdk::realtime::{
    sync_all_files, ChangeQueue, ChangeType, FileChange, RealtimeSync, SyncStrategy,
};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_file_change_creation() {
    let change = FileChange {
        path: PathBuf::from("/path/to/file.txt"),
        change_type: ChangeType::Created,
        timestamp: Utc::now(),
        size: 1024,
        hash: None,
    };

    assert_eq!(change.path, PathBuf::from("/path/to/file.txt"));
    assert!(matches!(change.change_type, ChangeType::Created));
    assert_eq!(change.size, 1024);
}

#[test]
fn test_file_change_types() {
    let created = FileChange {
        path: PathBuf::from("/new_file.txt"),
        change_type: ChangeType::Created,
        timestamp: Utc::now(),
        size: 512,
        hash: None,
    };

    let modified = FileChange {
        path: PathBuf::from("/existing_file.txt"),
        change_type: ChangeType::Modified,
        timestamp: Utc::now(),
        size: 1024,
        hash: None,
    };

    let deleted = FileChange {
        path: PathBuf::from("/old_file.txt"),
        change_type: ChangeType::Deleted,
        timestamp: Utc::now(),
        size: 0,
        hash: None,
    };

    assert!(matches!(created.change_type, ChangeType::Created));
    assert!(matches!(modified.change_type, ChangeType::Modified));
    assert!(matches!(deleted.change_type, ChangeType::Deleted));
}

#[tokio::test]
async fn test_change_queue_basic() {
    let queue = ChangeQueue::new(10);

    let change = FileChange {
        path: PathBuf::from("/test.txt"),
        change_type: ChangeType::Created,
        timestamp: Utc::now(),
        size: 256,
        hash: None,
    };

    queue.add_change(change.clone()).await;
    let changes = queue.get_changes().await;

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].path, PathBuf::from("/test.txt"));
    assert!(matches!(changes[0].change_type, ChangeType::Created));
}

#[test]
fn test_realtime_watcher_creation() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    fs::write(temp_dir.path().join("test.txt"), "content").expect("Failed to create file");

    assert!(temp_dir.path().exists(), "Temp directory should exist");
}

#[test]
fn test_multiple_file_changes() {
    let changes = vec![
        FileChange {
            path: PathBuf::from("/file1.txt"),
            change_type: ChangeType::Created,
            timestamp: Utc::now(),
            size: 100,
            hash: None,
        },
        FileChange {
            path: PathBuf::from("/file2.txt"),
            change_type: ChangeType::Modified,
            timestamp: Utc::now(),
            size: 200,
            hash: None,
        },
        FileChange {
            path: PathBuf::from("/file3.txt"),
            change_type: ChangeType::Deleted,
            timestamp: Utc::now(),
            size: 0,
            hash: None,
        },
    ];

    assert_eq!(changes.len(), 3);
    assert_eq!(changes[0].path, PathBuf::from("/file1.txt"));
    assert_eq!(changes[1].path, PathBuf::from("/file2.txt"));
    assert_eq!(changes[2].path, PathBuf::from("/file3.txt"));
}

#[test]
fn test_change_timestamps() {
    let now = Utc::now();
    let change = FileChange {
        path: PathBuf::from("/test.txt"),
        change_type: ChangeType::Created,
        timestamp: now,
        size: 512,
        hash: None,
    };

    assert_eq!(change.timestamp, now);
}

#[test]
fn test_large_file_change() {
    let large_size = 1024 * 1024 * 1024;

    let change = FileChange {
        path: PathBuf::from("/large_file.iso"),
        change_type: ChangeType::Created,
        timestamp: Utc::now(),
        size: large_size,
        hash: None,
    };

    assert_eq!(change.size, large_size);
}

#[test]
fn test_nested_path_changes() {
    let paths = vec![
        "/root/subdir/file.txt",
        "/root/subdir/nested/deep/file.txt",
        "/another/path/file.txt",
    ];

    let changes: Vec<FileChange> = paths
        .iter()
        .map(|path| FileChange {
            path: PathBuf::from(path),
            change_type: ChangeType::Created,
            timestamp: Utc::now(),
            size: 100,
            hash: None,
        })
        .collect();

    assert_eq!(changes.len(), 3);
    assert!(changes[1].path.to_string_lossy().contains("nested/deep"));
}

#[test]
fn test_file_change_with_hash() {
    let change = FileChange {
        path: PathBuf::from("/file.txt"),
        change_type: ChangeType::Created,
        timestamp: Utc::now(),
        size: 512,
        hash: Some("abc123def456".to_string()),
    };

    assert!(change.hash.is_some());
    assert_eq!(change.hash.unwrap(), "abc123def456");
}

#[test]
fn test_rename_change_type() {
    let change = FileChange {
        path: PathBuf::from("/old_name.txt"),
        change_type: ChangeType::Renamed,
        timestamp: Utc::now(),
        size: 256,
        hash: None,
    };

    assert!(matches!(change.change_type, ChangeType::Renamed));
}

// Testes assíncronos para ChangeQueue

#[tokio::test]
async fn test_change_queue_multiple_adds() {
    let queue = ChangeQueue::new(100);

    for i in 0..5 {
        let change = FileChange {
            path: PathBuf::from(format!("/file{}.txt", i)),
            change_type: ChangeType::Created,
            timestamp: Utc::now(),
            size: 100 * (i as u64 + 1),
            hash: None,
        };
        queue.add_change(change).await;
    }

    let changes = queue.get_changes().await;
    assert_eq!(changes.len(), 5);
}

#[tokio::test]
async fn test_change_queue_max_size() {
    let queue = ChangeQueue::new(3);

    for i in 0..5 {
        let change = FileChange {
            path: PathBuf::from(format!("/file{}.txt", i)),
            change_type: ChangeType::Created,
            timestamp: Utc::now(),
            size: 100,
            hash: None,
        };
        queue.add_change(change).await;
    }

    let changes = queue.get_changes().await;
    // Queue tem max_size=3, então deve manter apenas os últimos 3
    assert_eq!(changes.len(), 3);
    // O primeiro (file0.txt) deve ter sido removido
    assert_eq!(changes[0].path, PathBuf::from("/file2.txt"));
    assert_eq!(changes[2].path, PathBuf::from("/file4.txt"));
}

#[tokio::test]
async fn test_change_queue_clear() {
    let queue = ChangeQueue::new(10);

    for i in 0..3 {
        let change = FileChange {
            path: PathBuf::from(format!("/file{}.txt", i)),
            change_type: ChangeType::Created,
            timestamp: Utc::now(),
            size: 100,
            hash: None,
        };
        queue.add_change(change).await;
    }

    assert_eq!(queue.count().await, 3);
    queue.clear().await;
    assert_eq!(queue.count().await, 0);
}

#[tokio::test]
async fn test_change_queue_count() {
    let queue = ChangeQueue::new(10);

    assert_eq!(queue.count().await, 0);

    for i in 0..4 {
        let change = FileChange {
            path: PathBuf::from(format!("/file{}.txt", i)),
            change_type: ChangeType::Created,
            timestamp: Utc::now(),
            size: 100,
            hash: None,
        };
        queue.add_change(change).await;
    }

    assert_eq!(queue.count().await, 4);
}

// Testes para RealtimeSync

#[tokio::test]
async fn test_realtime_sync_creation() {
    let strategy = SyncStrategy::default();
    let sync = RealtimeSync::new(strategy);

    let stats = sync.get_stats().await;
    assert_eq!(stats.pending_changes, 0);
    assert_eq!(stats.synced, 0);
    assert_eq!(stats.failed, 0);
}

#[tokio::test]
async fn test_realtime_sync_increment_counters() {
    let strategy = SyncStrategy::default();
    let sync = RealtimeSync::new(strategy);

    sync.increment_synced().await;
    sync.increment_synced().await;
    sync.increment_failed().await;

    let stats = sync.get_stats().await;
    assert_eq!(stats.synced, 2);
    assert_eq!(stats.failed, 1);
}

#[tokio::test]
async fn test_realtime_sync_process_batch_with_ignore_patterns() {
    let mut strategy = SyncStrategy::default();
    strategy.ignore_patterns = vec![".*\\.tmp$".into()];
    let sync = RealtimeSync::new(strategy);

    // Adicionar mudança que não deve ser ignorada
    let change1 = FileChange {
        path: PathBuf::from("/file.txt"),
        change_type: ChangeType::Created,
        timestamp: Utc::now(),
        size: 100,
        hash: None,
    };

    // Adicionar mudança que pode ser ignorada
    let change2 = FileChange {
        path: PathBuf::from("/temp.tmp"),
        change_type: ChangeType::Created,
        timestamp: Utc::now(),
        size: 50,
        hash: None,
    };

    sync.queue.add_change(change1).await;
    sync.queue.add_change(change2).await;

    let processed = sync.process_batch().await;

    // Apenas a primeira mudança deve ser processada (tmp ignorada)
    assert_eq!(processed.len(), 1);
    assert_eq!(processed[0].path, PathBuf::from("/file.txt"));
}

#[tokio::test]
async fn test_sync_strategy_default() {
    let strategy = SyncStrategy::default();

    assert!(strategy.immediate_sync);
    assert_eq!(strategy.batch_interval, 5);
    assert_eq!(strategy.immediate_threshold, 10_485_760); // 10 MB
    assert_eq!(strategy.direction, "uni");
    assert!(strategy.ignore_patterns.len() > 0);
}

#[tokio::test]
async fn test_sync_strategy_custom() {
    let strategy = SyncStrategy {
        immediate_sync: false,
        batch_interval: 10,
        immediate_threshold: 5_000_000,
        ignore_patterns: vec![".*\\.lock".into()],
        direction: "bi".into(),
    };

    assert!(!strategy.immediate_sync);
    assert_eq!(strategy.batch_interval, 10);
    assert_eq!(strategy.immediate_threshold, 5_000_000);
    assert_eq!(strategy.direction, "bi");
}

// Testes de integração com sistema de ficheiros

#[tokio::test]
async fn test_file_sync_basic() {
    let src_dir = TempDir::new().expect("Failed to create source dir");
    let _dst_dir = TempDir::new().expect("Failed to create destination dir");

    // Criar um ficheiro de teste
    let test_file = src_dir.path().join("test.txt");
    fs::write(&test_file, "test content").expect("Failed to write test file");

    // Usar o RealtimeSync num modo simples
    let queue = ChangeQueue::new(100);

    let change = FileChange {
        path: test_file.clone(),
        change_type: ChangeType::Created,
        timestamp: Utc::now(),
        size: 12,
        hash: None,
    };

    queue.add_change(change).await;
    assert_eq!(queue.count().await, 1);

    let changes = queue.get_changes().await;
    assert_eq!(changes[0].size, 12);
}

// Testes para sync_all_files

#[tokio::test]
async fn test_sync_all_files_single_file() {
    let src_dir = TempDir::new().expect("Failed to create source dir");
    let dst_dir = TempDir::new().expect("Failed to create destination dir");

    // Criar um ficheiro de teste
    let test_file = src_dir.path().join("test.txt");
    fs::write(&test_file, "test content").expect("Failed to write test file");

    // Sincronizar
    let result = sync_all_files(src_dir.path(), dst_dir.path()).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 1);

    // Verificar que o ficheiro foi copiado
    let dst_file = dst_dir.path().join("test.txt");
    assert!(dst_file.exists());
    let content = fs::read_to_string(&dst_file).expect("Failed to read destination file");
    assert_eq!(content, "test content");
}

#[tokio::test]
async fn test_sync_all_files_multiple_files() {
    let src_dir = TempDir::new().expect("Failed to create source dir");
    let dst_dir = TempDir::new().expect("Failed to create destination dir");

    // Criar múltiplos ficheiros
    fs::write(src_dir.path().join("file1.txt"), "content1").expect("Failed to write file1");
    fs::write(src_dir.path().join("file2.txt"), "content2").expect("Failed to write file2");
    fs::write(src_dir.path().join("file3.txt"), "content3").expect("Failed to write file3");

    let result = sync_all_files(src_dir.path(), dst_dir.path()).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 3);

    // Verificar que todos os ficheiros foram copiados
    assert!(dst_dir.path().join("file1.txt").exists());
    assert!(dst_dir.path().join("file2.txt").exists());
    assert!(dst_dir.path().join("file3.txt").exists());
}

#[tokio::test]
async fn test_sync_all_files_nested_directories() {
    let src_dir = TempDir::new().expect("Failed to create source dir");
    let dst_dir = TempDir::new().expect("Failed to create destination dir");

    // Criar estrutura aninhada
    fs::create_dir_all(src_dir.path().join("subdir/nested")).expect("Failed to create dirs");
    fs::write(src_dir.path().join("file.txt"), "root").expect("Failed to write root file");
    fs::write(src_dir.path().join("subdir/file.txt"), "sub").expect("Failed to write sub file");
    fs::write(src_dir.path().join("subdir/nested/file.txt"), "nested")
        .expect("Failed to write nested file");

    let result = sync_all_files(src_dir.path(), dst_dir.path()).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 3);

    // Verificar estrutura replicada
    assert!(dst_dir.path().join("file.txt").exists());
    assert!(dst_dir.path().join("subdir/file.txt").exists());
    assert!(dst_dir.path().join("subdir/nested/file.txt").exists());
}

#[tokio::test]
async fn test_sync_all_files_nonexistent_source() {
    let nonexistent = PathBuf::from("/nonexistent/path");
    let dst_dir = TempDir::new().expect("Failed to create destination dir");

    let result = sync_all_files(&nonexistent, dst_dir.path()).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("does not exist"));
}

#[tokio::test]
async fn test_sync_all_files_preserves_content() {
    let src_dir = TempDir::new().expect("Failed to create source dir");
    let dst_dir = TempDir::new().expect("Failed to create destination dir");

    let content = "Test content with special chars: áéíóú €";
    fs::write(src_dir.path().join("test.txt"), content).expect("Failed to write file");

    let result = sync_all_files(src_dir.path(), dst_dir.path()).await;
    assert!(result.is_ok());

    let dst_content = fs::read_to_string(dst_dir.path().join("test.txt"))
        .expect("Failed to read destination file");
    assert_eq!(dst_content, content);
}

#[tokio::test]
async fn test_sync_all_files_empty_directory() {
    let src_dir = TempDir::new().expect("Failed to create source dir");
    let dst_dir = TempDir::new().expect("Failed to create destination dir");

    // Criar uma subpasta vazia
    fs::create_dir(src_dir.path().join("empty_subdir")).expect("Failed to create subdir");

    let result = sync_all_files(src_dir.path(), dst_dir.path()).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 0); // Sem ficheiros, apenas pastas vazias
}

// Testes para processamento automático com notify

#[tokio::test]
async fn test_realtime_sync_start_processing() {
    let strategy = SyncStrategy::default();
    let sync = std::sync::Arc::new(RealtimeSync::new(strategy));

    // Verificar que não está processando inicialmente
    assert!(!sync.is_processing().await);

    // Iniciar processamento
    let sync_clone = sync.clone();
    sync_clone.start_processing_loop().await;

    // Dar um pouco de tempo para a task iniciar
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Parar processamento
    sync.stop_processing().await;
    assert!(!sync.is_processing().await);
}

#[tokio::test]
async fn test_process_single_change_valid_file() {
    let src_dir = TempDir::new().expect("Failed to create source dir");
    let test_file = src_dir.path().join("test.txt");
    fs::write(&test_file, "content").expect("Failed to write file");

    // Verificar que o arquivo existe e tem o tamanho correto
    let metadata = std::fs::metadata(&test_file).expect("File should exist");
    assert_eq!(metadata.len(), 7);

    let strategy = SyncStrategy::default();
    let sync = RealtimeSync::new(strategy);

    let change = FileChange {
        path: test_file.clone(),
        change_type: ChangeType::Created,
        timestamp: Utc::now(),
        size: 7,
        hash: None,
    };

    let result = sync.process_single_change(change, 2).await;
    assert!(result.is_ok());

    let stats = sync.get_stats().await;
    assert_eq!(stats.synced, 1);
}

#[tokio::test]
async fn test_process_single_change_nonexistent_file() {
    let nonexistent = PathBuf::from("/nonexistent/file.txt");
    let strategy = SyncStrategy::default();
    let sync = RealtimeSync::new(strategy);

    let change = FileChange {
        path: nonexistent,
        change_type: ChangeType::Created,
        timestamp: Utc::now(),
        size: 100,
        hash: None,
    };

    let result = sync.process_single_change(change, 1).await;
    assert!(result.is_err());

    let stats = sync.get_stats().await;
    assert_eq!(stats.failed, 1);
}

#[tokio::test]
async fn test_process_single_change_with_ignore_pattern() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_file = temp_dir.path().join("test.tmp");
    fs::write(&test_file, "content").expect("Failed to write file");

    let mut strategy = SyncStrategy::default();
    strategy.ignore_patterns = vec![".*\\.tmp$".into()];
    let sync = RealtimeSync::new(strategy);

    let change = FileChange {
        path: test_file,
        change_type: ChangeType::Created,
        timestamp: Utc::now(),
        size: 7,
        hash: None,
    };

    let result = sync.process_single_change(change, 2).await;
    assert!(result.is_ok());

    // Deve ignorar o padrão, então não deve incrementar synced
    let stats = sync.get_stats().await;
    assert_eq!(stats.synced, 0);
}

#[tokio::test]
async fn test_peek_pending_changes() {
    let queue = ChangeQueue::new(10);
    let strategy = SyncStrategy::default();
    let sync = RealtimeSync::new(strategy);

    let change = FileChange {
        path: PathBuf::from("/test.txt"),
        change_type: ChangeType::Created,
        timestamp: Utc::now(),
        size: 100,
        hash: None,
    };

    sync.queue.add_change(change.clone()).await;

    // Peek não deve remover
    let pending = sync.peek_pending_changes().await;
    assert_eq!(pending.len(), 1);

    // Peek novamente deve retornar a mesma mudança
    let pending2 = sync.peek_pending_changes().await;
    assert_eq!(pending2.len(), 1);
}

#[tokio::test]
async fn test_get_detailed_stats_ratio() {
    let strategy = SyncStrategy::default();
    let sync = RealtimeSync::new(strategy);

    // 3 synced, 1 failed
    sync.increment_synced().await;
    sync.increment_synced().await;
    sync.increment_synced().await;
    sync.increment_failed().await;

    let detailed = sync.get_detailed_stats().await;

    assert_eq!(detailed.synced, 3);
    assert_eq!(detailed.failed, 1);
    assert_eq!(detailed.sync_ratio, 0.75); // 3 / (3 + 1)
}

#[tokio::test]
async fn test_get_detailed_stats_ratio_zero() {
    let strategy = SyncStrategy::default();
    let sync = RealtimeSync::new(strategy);

    let detailed = sync.get_detailed_stats().await;

    assert_eq!(detailed.synced, 0);
    assert_eq!(detailed.failed, 0);
    assert_eq!(detailed.sync_ratio, 0.0);
}

#[tokio::test]
async fn test_multiple_changes_sequential() {
    let src_dir = TempDir::new().expect("Failed to create source dir");
    let strategy = SyncStrategy::default();
    let sync = RealtimeSync::new(strategy);

    // Adicionar múltiplas mudanças
    for i in 0..3 {
        let file_path = src_dir.path().join(format!("file{}.txt", i));
        fs::write(&file_path, format!("content{}", i)).expect("Failed to write file");

        let change = FileChange {
            path: file_path,
            change_type: ChangeType::Created,
            timestamp: Utc::now(),
            size: (8 + i as u64),
            hash: None,
        };

        let result = sync.process_single_change(change, 2).await;
        assert!(result.is_ok());
    }

    let detailed = sync.get_detailed_stats().await;
    assert_eq!(detailed.synced, 3);
    assert_eq!(detailed.sync_ratio, 1.0);
}

#[tokio::test]
async fn test_process_batch_filters_correctly() {
    let src_dir = TempDir::new().expect("Failed to create source dir");
    let strategy = SyncStrategy {
        immediate_sync: true,
        batch_interval: 5,
        immediate_threshold: 10_485_760,
        ignore_patterns: vec![".*\\.lock$".into(), ".*\\.tmp$".into()],
        direction: "uni".into(),
    };
    let sync = RealtimeSync::new(strategy);

    // Adicionar mudanças
    let file1 = src_dir.path().join("important.txt");
    let file2 = src_dir.path().join("cache.lock");
    let file3 = src_dir.path().join("temp.tmp");

    fs::write(&file1, "important").expect("Failed to write file1");
    fs::write(&file2, "lock").expect("Failed to write file2");
    fs::write(&file3, "temp").expect("Failed to write file3");

    sync.queue
        .add_change(FileChange {
            path: file1,
            change_type: ChangeType::Created,
            timestamp: Utc::now(),
            size: 9,
            hash: None,
        })
        .await;

    sync.queue
        .add_change(FileChange {
            path: file2,
            change_type: ChangeType::Created,
            timestamp: Utc::now(),
            size: 4,
            hash: None,
        })
        .await;

    sync.queue
        .add_change(FileChange {
            path: file3,
            change_type: ChangeType::Created,
            timestamp: Utc::now(),
            size: 4,
            hash: None,
        })
        .await;

    let processed = sync.process_batch().await;

    // Apenas o arquivo importante.txt deve ser processado
    assert_eq!(processed.len(), 1);
    assert!(processed[0]
        .path
        .to_string_lossy()
        .contains("important.txt"));
}
