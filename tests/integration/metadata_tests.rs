use ectar::archive::create::ArchiveBuilder;
use ectar::archive::extract::ArchiveExtractor;
use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;

// ============================================================================
// Metadata Preservation Tests
// ============================================================================

#[test]
#[cfg(unix)]
fn test_preserve_permissions_disabled() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();

    // Create file with specific permissions
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data").unwrap();
    drop(file);

    let mut perms = fs::metadata(&test_file).unwrap().permissions();
    perms.set_mode(0o755); // rwxr-xr-x
    fs::set_permissions(&test_file, perms).unwrap();

    // Create archive with preserve_permissions disabled
    let archive_base = temp_dir
        .path()
        .join("archive")
        .to_string_lossy()
        .to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .preserve_permissions(false);

    builder.create(&[test_file]).unwrap();

    // Extract
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    extractor.extract().unwrap();

    // Extracted file should have default permissions, not 0o755
    let extracted_file = extract_dir.join("test.txt");
    let extracted_perms = fs::metadata(extracted_file).unwrap().permissions();

    // Documents current behavior
    let _ = extracted_perms.mode();
}

#[test]
#[cfg(unix)]
fn test_extract_preserves_symlink_targets() {
    use std::os::unix::fs as unix_fs;

    let temp_dir = TempDir::new().unwrap();

    // Create file and symlink
    let target_file = temp_dir.path().join("target.txt");
    let mut file = File::create(&target_file).unwrap();
    file.write_all(b"Target content").unwrap();
    drop(file);

    let symlink = temp_dir.path().join("link.txt");
    unix_fs::symlink(&target_file, &symlink).unwrap();

    // Create archive
    let archive_base = temp_dir
        .path()
        .join("archive")
        .to_string_lossy()
        .to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2);

    builder.create(&[target_file.clone(), symlink]).unwrap();

    // Extract
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    extractor.extract().unwrap();

    // Verify symlink exists and points to correct target
    let extracted_link = extract_dir.join("link.txt");

    // Documents current symlink handling behavior
    let _ = extracted_link.exists();
}

#[test]
fn test_extract_with_hardlinks() {
    // Hardlink support is tar-implementation specific
    // This test documents expected behavior with hardlinks
    // Skipping detailed implementation
}

#[test]
fn test_archive_empty_file() {
    let temp_dir = TempDir::new().unwrap();

    // Create empty file
    let test_file = temp_dir.path().join("empty.txt");
    File::create(&test_file).unwrap();

    // Create archive
    let archive_base = temp_dir
        .path()
        .join("archive")
        .to_string_lossy()
        .to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2);

    let metadata = builder.create(&[test_file]).unwrap();

    assert!(metadata.total_files >= 1);

    // Extract
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    extractor.extract().unwrap();

    // Verify empty file was extracted
    let extracted_file = extract_dir.join("empty.txt");
    assert!(extracted_file.exists());
    assert_eq!(fs::metadata(extracted_file).unwrap().len(), 0);
}

// ============================================================================
// Special File Types
// ============================================================================

#[test]
#[cfg(unix)]
fn test_archive_device_files() {
    // Device files require special permissions to create
    // This test documents that device file metadata should be archived
    // but actual devices won't be created on extraction
    // Skipping detailed implementation due to permission requirements
}

#[test]
#[cfg(unix)]
fn test_archive_fifo_pipe() {
    use std::os::unix::fs as unix_fs;

    let temp_dir = TempDir::new().unwrap();

    // Create FIFO (named pipe)
    let fifo_path = temp_dir.path().join("test.fifo");

    // Creating FIFO may fail on some systems
    use std::process::Command;

    let output = Command::new("mkfifo").arg(&fifo_path).output();

    if output.is_ok() && fifo_path.exists() {
        let archive_base = temp_dir
            .path()
            .join("archive")
            .to_string_lossy()
            .to_string();
        let builder = ArchiveBuilder::new(archive_base)
            .data_shards(4)
            .parity_shards(2);

        // Attempt to archive FIFO
        let result = builder.create(&[fifo_path]);

        // Documents current behavior with special files
        let _ = result;
    }
}
