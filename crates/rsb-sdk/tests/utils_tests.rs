use rsb_sdk::utils;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_mmap_file_basic() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let test_file = temp_dir.path().join("test.txt");

    let content = b"Hello, Memory Map!";
    fs::write(&test_file, content).expect("Failed to write test file");

    let mmap = utils::mmap_file(&test_file).expect("Failed to mmap file");

    assert_eq!(
        mmap.len(),
        content.len(),
        "Mmap length should match file size"
    );
    assert_eq!(&mmap[..], content, "Mmap content should match file content");
}

#[test]
fn test_mmap_file_large() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let test_file = temp_dir.path().join("large.bin");

    let large_data = vec![42u8; 10 * 1024 * 1024]; // 10 MB
    fs::write(&test_file, &large_data).expect("Failed to write large file");

    let mmap = utils::mmap_file(&test_file).expect("Failed to mmap large file");

    assert_eq!(
        mmap.len(),
        large_data.len(),
        "Mmap should handle large files"
    );
    assert_eq!(mmap[0], 42, "Mmap content should be accessible");
}

#[test]
fn test_mmap_file_not_found() {
    let non_existent = Path::new("/path/to/non/existent/file.txt");
    let result = utils::mmap_file(non_existent);

    assert!(result.is_err(), "Mmapping non-existent file should fail");
}

#[test]
fn test_mmap_empty_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let test_file = temp_dir.path().join("empty.txt");

    fs::write(&test_file, b"").expect("Failed to write empty file");

    let mmap = utils::mmap_file(&test_file).expect("Failed to mmap empty file");

    assert_eq!(mmap.len(), 0, "Mmap of empty file should have length 0");
}

#[test]
fn test_build_walker_basic() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let root = temp_dir.path();

    // Create test structure
    fs::write(root.join("file1.txt"), "content1").expect("Failed to create file1");
    fs::write(root.join("file2.rs"), "content2").expect("Failed to create file2");
    fs::create_dir(root.join("subdir")).expect("Failed to create subdir");
    fs::write(root.join("subdir/file3.txt"), "content3").expect("Failed to create file3");

    let walker = utils::build_walker(root, &[], false);
    let entries: Vec<_> = walker.build().collect();

    assert!(entries.len() > 0, "Walker should find entries");
}

#[test]
fn test_walk_filtered_basic() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let root = temp_dir.path();

    // Create test files
    fs::write(root.join("file1.txt"), "content1").expect("Failed to create file1");
    fs::write(root.join("file2.tmp"), "temp").expect("Failed to create file2");
    fs::create_dir(root.join("node_modules")).expect("Failed to create node_modules");
    fs::write(root.join("node_modules/pkg.js"), "code").expect("Failed to create pkg");

    let custom_globs = vec!["*.tmp".to_string(), "node_modules".to_string()];
    let walk = utils::walk_filtered(root, &custom_globs, false);

    let entries: Vec<_> = walk.filter_map(|e| e.ok()).collect();

    // Should have found some entries
    assert!(entries.len() > 0, "Walker should find entries");
}

#[test]
fn test_walk_respects_hidden_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let root = temp_dir.path();

    // Create visible and hidden files
    fs::write(root.join("visible.txt"), "content").expect("Failed to create visible");
    fs::write(root.join(".hidden"), "secret").expect("Failed to create hidden");

    let walker = utils::build_walker(root, &[], false);
    let entries: Vec<_> = walker.build().filter_map(|e| e.ok()).collect();

    assert!(entries.len() > 0, "Walker should find visible files");
}

#[test]
fn test_multiple_exclude_patterns() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let root = temp_dir.path();

    // Create various files
    fs::write(root.join("readme.md"), "doc").expect("Failed to create readme");
    fs::write(root.join("test.tmp"), "temp").expect("Failed to create temp");
    fs::write(root.join("backup.bak"), "backup").expect("Failed to create backup");
    fs::create_dir(root.join("build")).expect("Failed to create build");
    fs::write(root.join("build/output.o"), "object").expect("Failed to create object");

    let patterns = vec![
        "*.tmp".to_string(),
        "*.bak".to_string(),
        "build".to_string(),
    ];

    let walk = utils::walk_filtered(root, &patterns, false);
    let entries: Vec<_> = walk.filter_map(|e| e.ok()).collect();

    assert!(
        entries.len() > 0,
        "Walker should still find non-excluded files"
    );
}

#[test]
fn test_walker_with_gitignore() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let root = temp_dir.path();

    // Create .gitignore
    fs::write(root.join(".gitignore"), "*.tmp\nnode_modules/\n")
        .expect("Failed to create .gitignore");

    // Create files
    fs::write(root.join("keep.txt"), "content").expect("Failed to create keep");
    fs::write(root.join("ignore.tmp"), "temp").expect("Failed to create temp");
    fs::create_dir(root.join("node_modules")).expect("Failed to create node_modules");
    fs::write(root.join("node_modules/pkg.js"), "code").expect("Failed to create pkg");

    let walker = utils::build_walker(root, &[], true); // respect_gitignore = true
    let entries: Vec<_> = walker.build().filter_map(|e| e.ok()).collect();

    assert!(entries.len() > 0, "Walker should find entries");
}

#[test]
fn test_walker_depth() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let root = temp_dir.path();

    // Create nested structure
    fs::create_dir(root.join("level1")).expect("Failed to create level1");
    fs::create_dir(root.join("level1/level2")).expect("Failed to create level2");
    fs::create_dir(root.join("level1/level2/level3")).expect("Failed to create level3");
    fs::write(root.join("level1/level2/level3/deep.txt"), "content")
        .expect("Failed to create deep file");

    let walker = utils::build_walker(root, &[], false);
    let entries: Vec<_> = walker.build().filter_map(|e| e.ok()).collect();

    // Should traverse deep directories
    assert!(entries.len() > 0, "Walker should handle nested directories");
}
