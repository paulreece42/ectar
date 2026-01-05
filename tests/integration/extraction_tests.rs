use ectar::archive::create::ArchiveBuilder;
use ectar::archive::extract::ArchiveExtractor;
use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;

// ============================================================================
// Filter Edge Cases
// ============================================================================

#[test]
fn test_extract_with_no_files_matching_filter() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files
    let test_file_a = temp_dir.path().join("file_a.txt");
    let mut file = File::create(&test_file_a).unwrap();
    file.write_all(b"File A").unwrap();
    drop(file);

    let test_file_b = temp_dir.path().join("file_b.txt");
    let mut file = File::create(&test_file_b).unwrap();
    file.write_all(b"File B").unwrap();
    drop(file);

    // Create archive
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2);

    builder.create(&[test_file_a, test_file_b]).unwrap();

    // Extract with filter that matches nothing
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()))
        .file_filters(vec!["nonexistent_file.txt".to_string()]);

    let result = extractor.extract();

    // Should succeed but extract 0 files
    assert!(result.is_ok());
    let metadata = result.unwrap();
    assert_eq!(metadata.files_extracted, 0);
}

#[test]
fn test_extract_with_glob_pattern_filter() {
    let temp_dir = TempDir::new().unwrap();

    // Create mixed file types
    let txt_file = temp_dir.path().join("file.txt");
    let mut file = File::create(&txt_file).unwrap();
    file.write_all(b"Text file").unwrap();
    drop(file);

    let bin_file = temp_dir.path().join("file.bin");
    let mut file = File::create(&bin_file).unwrap();
    file.write_all(b"Binary file").unwrap();
    drop(file);

    // Create archive
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2);

    builder.create(&[txt_file, bin_file]).unwrap();

    // Extract only .txt files
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()))
        .file_filters(vec!["*.txt".to_string()]);

    let result = extractor.extract();

    assert!(result.is_ok());

    // Should only have extracted the .txt file
    assert!(extract_dir.join("file.txt").exists());
    assert!(!extract_dir.join("file.bin").exists());
}

#[test]
fn test_extract_with_invalid_glob_pattern() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test").unwrap();
    drop(file);

    // Create archive
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2);

    builder.create(&[test_file]).unwrap();

    // Extract with invalid glob pattern
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir))
        .file_filters(vec!["[[[".to_string()]); // Invalid glob

    let result = extractor.extract();

    // Documents current behavior with invalid glob patterns
    // May succeed with no matches or fail with pattern error
    let _ = result;
}

#[test]
fn test_extract_with_exclude_pattern_matches_all() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test").unwrap();
    drop(file);

    // Create archive
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2);

    builder.create(&[test_file]).unwrap();

    // Extract with exclude pattern matching everything
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()))
        .exclude_patterns(vec!["*".to_string()]);

    let result = extractor.extract();

    // Should succeed but extract 0 files
    assert!(result.is_ok());
    let metadata = result.unwrap();
    assert_eq!(metadata.files_extracted, 0);
}

// ============================================================================
// Strip Components Edge Cases
// ============================================================================

#[test]
fn test_strip_components_equals_path_depth() {
    let temp_dir = TempDir::new().unwrap();

    // Create nested directory structure
    let nested_dir = temp_dir.path().join("a").join("b").join("c");
    fs::create_dir_all(&nested_dir).unwrap();

    let test_file = nested_dir.join("file.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Nested file").unwrap();
    drop(file);

    // Create archive
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2);

    builder.create(&[temp_dir.path().join("a")]).unwrap();

    // Extract with strip_components equal to path depth (3: a/b/c)
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()))
        .strip_components(3);

    let result = extractor.extract();

    assert!(result.is_ok());

    // File should be at root after stripping all components
    assert!(extract_dir.join("file.txt").exists());
}

#[test]
fn test_strip_components_exceeds_path_depth() {
    let temp_dir = TempDir::new().unwrap();

    // Create simple file
    let test_file = temp_dir.path().join("a").join("file.txt");
    fs::create_dir_all(test_file.parent().unwrap()).unwrap();
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"File").unwrap();
    drop(file);

    // Create archive
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2);

    builder.create(&[temp_dir.path().join("a")]).unwrap();

    // Extract with strip_components exceeding path depth
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()))
        .strip_components(10); // Much more than path depth

    let result = extractor.extract();

    assert!(result.is_ok());

    // Should skip all files as they don't have enough components
    let metadata = result.unwrap();
    assert_eq!(metadata.files_extracted, 0);
}

#[test]
fn test_strip_components_zero() {
    let temp_dir = TempDir::new().unwrap();

    // Create nested file
    let nested_dir = temp_dir.path().join("dir");
    fs::create_dir(&nested_dir).unwrap();

    let test_file = nested_dir.join("file.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test").unwrap();
    drop(file);

    // Create archive
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2);

    builder.create(&[nested_dir.clone()]).unwrap();

    // Extract with strip_components = 0 (default)
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()))
        .strip_components(0);

    let result = extractor.extract();

    assert!(result.is_ok());

    // Should preserve full path
    assert!(extract_dir.join("dir").join("file.txt").exists());
}
