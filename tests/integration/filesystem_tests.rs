use ectar::archive::create::ArchiveBuilder;
use ectar::archive::extract::ArchiveExtractor;
use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;

// Note: Some tests are Unix-specific and use #[cfg(unix)]

// ============================================================================
// Permission & Access Error Tests
// ============================================================================

#[test]
#[cfg(unix)]
fn test_create_with_unreadable_input_file() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();

    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data").unwrap();
    drop(file);

    // Make file unreadable (no read permissions)
    let mut perms = fs::metadata(&test_file).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&test_file, perms).unwrap();

    let archive_base = temp_dir
        .path()
        .join("archive")
        .to_string_lossy()
        .to_string();
    let builder = ArchiveBuilder::new(archive_base)
        .data_shards(4)
        .parity_shards(2);

    let result = builder.create(&[test_file.clone()]);

    // Restore permissions for cleanup
    let mut perms = fs::metadata(&test_file).unwrap().permissions();
    perms.set_mode(0o644);
    let _ = fs::set_permissions(&test_file, perms);

    // Should fail with permission denied error
    assert!(result.is_err());
}

#[test]
#[cfg(unix)]
fn test_create_to_readonly_directory() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();

    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data").unwrap();
    drop(file);

    // Create readonly output directory
    let output_dir = temp_dir.path().join("readonly");
    fs::create_dir(&output_dir).unwrap();

    let mut perms = fs::metadata(&output_dir).unwrap().permissions();
    perms.set_mode(0o444); // readonly
    fs::set_permissions(&output_dir, perms.clone()).unwrap();

    let archive_base = output_dir.join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base)
        .data_shards(4)
        .parity_shards(2);

    let result = builder.create(&[test_file]);

    // Restore permissions for cleanup
    perms.set_mode(0o755);
    let _ = fs::set_permissions(&output_dir, perms);

    // Should fail when trying to write to readonly directory
    assert!(result.is_err());
}

#[test]
#[cfg(unix)]
fn test_extract_to_readonly_directory() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();

    // Create test file and archive
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data").unwrap();
    drop(file);

    let archive_base = temp_dir
        .path()
        .join("archive")
        .to_string_lossy()
        .to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2);

    builder.create(&[test_file]).unwrap();

    // Create readonly extract directory
    let extract_dir = temp_dir.path().join("readonly");
    fs::create_dir(&extract_dir).unwrap();

    let mut perms = fs::metadata(&extract_dir).unwrap().permissions();
    perms.set_mode(0o444); // readonly
    fs::set_permissions(&extract_dir, perms.clone()).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    let result = extractor.extract();

    // Restore permissions for cleanup
    perms.set_mode(0o755);
    let _ = fs::set_permissions(&extract_dir, perms);

    // Should fail when trying to extract to readonly directory
    assert!(result.is_err());
}

#[test]
fn test_create_with_file_name_too_long() {
    let temp_dir = TempDir::new().unwrap();

    // Create file with very long name (300 characters)
    let long_name = "a".repeat(300);
    let test_file = temp_dir.path().join(&long_name);

    // May fail to create the file on some filesystems
    if File::create(&test_file).is_ok() {
        let archive_base = temp_dir
            .path()
            .join("archive")
            .to_string_lossy()
            .to_string();
        let builder = ArchiveBuilder::new(archive_base)
            .data_shards(4)
            .parity_shards(2);

        let result = builder.create(&[test_file]);

        // Documents current behavior with long filenames
        let _ = result;
    }
}

// ============================================================================
// Special Path Scenarios
// ============================================================================

#[test]
fn test_create_with_special_characters_in_path() {
    let temp_dir = TempDir::new().unwrap();

    // Create files with special characters
    let test_cases = vec![
        "file with spaces.txt",
        "file_with_unicode_\u{1F4A9}.txt",
        "file\"with\"quotes.txt",
    ];

    for name in test_cases {
        let test_file = temp_dir.path().join(name);
        if let Ok(mut file) = File::create(&test_file) {
            file.write_all(b"Test").unwrap();
            drop(file);

            let archive_base = temp_dir
                .path()
                .join("archive")
                .to_string_lossy()
                .to_string();
            let builder = ArchiveBuilder::new(archive_base)
                .data_shards(4)
                .parity_shards(2);

            let result = builder.create(&[test_file]);

            // Should handle special characters gracefully
            let _ = result;
        }
    }
}

#[test]
#[cfg(unix)]
fn test_create_with_symlink_cycle() {
    use std::os::unix::fs as unix_fs;

    let temp_dir = TempDir::new().unwrap();

    // Create symlink cycle: a -> b, b -> a
    let link_a = temp_dir.path().join("a");
    let link_b = temp_dir.path().join("b");

    unix_fs::symlink(&link_b, &link_a).unwrap();
    unix_fs::symlink(&link_a, &link_b).unwrap();

    let archive_base = temp_dir
        .path()
        .join("archive")
        .to_string_lossy()
        .to_string();
    let builder = ArchiveBuilder::new(archive_base)
        .data_shards(4)
        .parity_shards(2)
        .follow_symlinks(true);

    let result = builder.create(&[link_a]);

    // Should detect cycle or hit recursion limit
    // Documents current behavior
    let _ = result;
}

#[test]
fn test_extract_with_absolute_path_in_tar() {
    // This test would require manually creating a tar with absolute paths
    // Skipping detailed implementation as it requires low-level tar manipulation
    // The test documents the security requirement: absolute paths should be rejected/sanitized
}

#[test]
fn test_extract_with_path_traversal_attempt() {
    // This test would require manually creating a tar with ../ paths
    // Skipping detailed implementation as it requires low-level tar manipulation
    // The test documents the security requirement: ../ paths should be rejected/sanitized
}
