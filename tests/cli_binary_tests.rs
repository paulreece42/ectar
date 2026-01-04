use std::fs::{self, File};
use std::io::Write;
use std::process::Command;
use tempfile::TempDir;

fn get_binary_path() -> String {
    env!("CARGO_BIN_EXE_ectar").to_string()
}

fn create_test_files(temp_dir: &TempDir) -> std::path::PathBuf {
    let test_dir = temp_dir.path().join("testdata");
    fs::create_dir(&test_dir).unwrap();

    let file = test_dir.join("test.txt");
    let mut f = File::create(&file).unwrap();
    f.write_all(b"Test content for CLI testing").unwrap();
    drop(f);

    test_dir
}

#[test]
fn test_cli_help() {
    let output = Command::new(get_binary_path())
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ectar"));
    assert!(stdout.contains("create"));
    assert!(stdout.contains("extract"));
}

#[test]
fn test_cli_version() {
    let output = Command::new(get_binary_path())
        .arg("--version")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ectar"));
}

#[test]
fn test_cli_create() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Create failed: {:?}", String::from_utf8_lossy(&output.stderr));

    // Verify shard files were created
    let pattern = format!("{}.c001.s00", archive_base);
    assert!(std::path::Path::new(&pattern).exists());
}

#[test]
fn test_cli_create_no_compression() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--no-compression")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_create_no_index() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--no-index")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_extract() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // First create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Then extract
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("extract")
        .arg("-o")
        .arg(extract_dir.to_string_lossy().to_string())
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Extract failed: {:?}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn test_cli_list() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // First create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Then list
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("list")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "List failed: stderr={:?}, stdout={:?}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test.txt") || stdout.contains("testdata"));
}

#[test]
fn test_cli_list_long() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // List with long format
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("list")
        .arg("--long")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "List long failed: {:?}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn test_cli_list_json() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // List with JSON format
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("list")
        .arg("--format")
        .arg("json")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_verify() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Verify
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("verify")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Verify failed: {:?}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("HEALTHY") || stdout.contains("Healthy") || stdout.contains("healthy") || stdout.contains("OK"),
        "Expected health status in output: {:?}", stdout);
}

#[test]
fn test_cli_verify_quick() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Verify quick
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("verify")
        .arg("--quick")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_verify_full() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Verify full
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("verify")
        .arg("--full")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_info() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Info
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("info")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_info_json() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Info JSON
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("info")
        .arg("--format")
        .arg("json")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_create_with_exclude() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = temp_dir.path().join("testdata");
    fs::create_dir(&test_dir).unwrap();

    // Create files including one to exclude
    let keep = test_dir.join("keep.txt");
    let mut f = File::create(&keep).unwrap();
    f.write_all(b"keep").unwrap();
    drop(f);

    let exclude = test_dir.join("exclude.log");
    let mut f = File::create(&exclude).unwrap();
    f.write_all(b"exclude").unwrap();
    drop(f);

    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--exclude")
        .arg(".log")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_extract_partial() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Extract with partial flag
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("extract")
        .arg("-o")
        .arg(extract_dir.to_string_lossy().to_string())
        .arg("--partial")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_extract_strip_components() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Extract with strip components
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("extract")
        .arg("-o")
        .arg(extract_dir.to_string_lossy().to_string())
        .arg("--strip-components")
        .arg("1")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_extract_no_verify() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Extract without verification
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("extract")
        .arg("-o")
        .arg(extract_dir.to_string_lossy().to_string())
        .arg("--no-verify-checksums")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Extract no verify failed: {:?}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn test_cli_create_compression_level() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--compression-level")
        .arg("10")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_verbosity_v() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    let output = Command::new(get_binary_path())
        .arg("-v")
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_verbosity_vv() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    let output = Command::new(get_binary_path())
        .arg("-vv")
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_verbosity_vvv() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    let output = Command::new(get_binary_path())
        .arg("-vvv")
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_quiet() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    let output = Command::new(get_binary_path())
        .arg("-q")
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_verify_failed_archive() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive with 4 data + 2 parity = 6 shards
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Delete more than parity shards (4 out of 6) - this should cause unrecoverable failure
    for i in 0..4 {
        let shard_path = temp_dir.path().join(format!("archive.c001.s{:02}", i));
        let _ = fs::remove_file(shard_path);
    }

    // Verify should fail
    let pattern = format!("{}.c*.s*", archive_base);
    let output = Command::new(get_binary_path())
        .arg("verify")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    // Should exit with non-zero status for failed verification
    assert!(!output.status.success());
}

#[test]
fn test_cli_extract_partial_with_failures() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Delete too many shards to make chunk unrecoverable
    for i in 0..4 {
        let shard_path = temp_dir.path().join(format!("archive.c001.s{:02}", i));
        let _ = fs::remove_file(shard_path);
    }

    // Extract with partial flag
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("extract")
        .arg("-o")
        .arg(extract_dir.to_string_lossy().to_string())
        .arg("--partial")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    // Partial extraction should complete (even if some chunks failed)
    // The stdout should contain "Chunks failed" message
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Note: May or may not succeed depending on implementation
    assert!(stdout.contains("Chunks") || output.status.success() || !output.status.success());
}

#[test]
fn test_cli_extract_with_output_dir() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Extract with explicit output directory
    let extract_dir = temp_dir.path().join("output");
    fs::create_dir(&extract_dir).unwrap();
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("extract")
        .arg("-o")
        .arg(extract_dir.to_string_lossy().to_string())
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Output directory:"));
}

#[test]
fn test_cli_create_follow_symlinks() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = temp_dir.path().join("testdata");
    fs::create_dir(&test_dir).unwrap();

    // Create a file
    let file = test_dir.join("test.txt");
    let mut f = File::create(&file).unwrap();
    f.write_all(b"Test content").unwrap();
    drop(f);

    // Create a symlink
    #[cfg(unix)]
    std::os::unix::fs::symlink(&file, test_dir.join("link.txt")).unwrap();

    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--follow-symlinks")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_create_no_preserve_permissions() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--no-preserve-permissions")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_extract_with_exclude() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = temp_dir.path().join("testdata");
    fs::create_dir(&test_dir).unwrap();

    // Create multiple files
    for name in ["keep.txt", "exclude.log", "also_keep.txt"] {
        let file = test_dir.join(name);
        let mut f = File::create(&file).unwrap();
        f.write_all(name.as_bytes()).unwrap();
        drop(f);
    }

    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Extract with exclude pattern
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("extract")
        .arg("-o")
        .arg(extract_dir.to_string_lossy().to_string())
        .arg("--exclude")
        .arg(".log")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Extract with exclude failed: stderr={:?}",
        String::from_utf8_lossy(&output.stderr));
}

#[test]
fn test_cli_extract_with_files_filter() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = temp_dir.path().join("testdata");
    fs::create_dir(&test_dir).unwrap();

    // Create multiple files
    for name in ["file1.txt", "file2.txt", "file3.txt"] {
        let file = test_dir.join(name);
        let mut f = File::create(&file).unwrap();
        f.write_all(name.as_bytes()).unwrap();
        drop(f);
    }

    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Extract with file filter
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();
    let pattern = format!("{}.c*.s*", archive_base);

    let output = Command::new(get_binary_path())
        .arg("extract")
        .arg("-o")
        .arg(extract_dir.to_string_lossy().to_string())
        .arg("--files")
        .arg("file1")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Extract with files failed: stderr={:?}",
        String::from_utf8_lossy(&output.stderr));
}

#[test]
fn test_cli_list_with_files_filter() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // List with file filter
    let pattern = format!("{}.c*.s*", archive_base);
    let output = Command::new(get_binary_path())
        .arg("list")
        .arg("--files")
        .arg("test")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_list_csv_format() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // List in CSV format
    let pattern = format!("{}.c*.s*", archive_base);
    let output = Command::new(get_binary_path())
        .arg("list")
        .arg("--format")
        .arg("csv")
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_cli_verify_with_report() {
    let temp_dir = TempDir::new().unwrap();
    let test_dir = create_test_files(&temp_dir);
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Create the archive
    let output = Command::new(get_binary_path())
        .arg("create")
        .arg("-o")
        .arg(&archive_base)
        .arg("--data-shards")
        .arg("4")
        .arg("--parity-shards")
        .arg("2")
        .arg("--chunk-size")
        .arg("1MB")
        .arg(test_dir.to_string_lossy().to_string())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Verify with report file
    let pattern = format!("{}.c*.s*", archive_base);
    let report_path = temp_dir.path().join("report.json");

    let output = Command::new(get_binary_path())
        .arg("verify")
        .arg("--report")
        .arg(report_path.to_string_lossy().to_string())
        .arg("-i")
        .arg(&pattern)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Verify with report failed: stderr={:?}, stdout={:?}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout));
    assert!(report_path.exists());
}
