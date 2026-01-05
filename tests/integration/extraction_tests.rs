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

// ============================================================================
// Zfec Header and Index-less Extraction Tests
// ============================================================================

#[test]
fn test_extract_without_index_file() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file with enough data to create multiple chunks
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data for extraction without index file").unwrap();
    drop(file);

    // Create archive with chunking
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(3)
        .parity_shards(2)
        .chunk_size(Some(1024)); // Small chunk size to create multiple chunks

    builder.create(&[test_file.clone()]).unwrap();

    // Delete the index file
    let index_path = format!("{}.index.zst", archive_base);
    fs::remove_file(&index_path).unwrap();

    // Extract without index
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));

    let result = extractor.extract();

    // Should succeed using zfec headers from shards
    assert!(result.is_ok());
    let metadata = result.unwrap();
    assert!(metadata.chunks_recovered > 0);
    assert_eq!(metadata.files_extracted, 1);

    // Verify extracted file matches original
    let extracted_file = extract_dir.join("test.txt");
    assert!(extracted_file.exists());
    let original_content = fs::read(&test_file).unwrap();
    let extracted_content = fs::read(&extracted_file).unwrap();
    assert_eq!(original_content, extracted_content);
}

#[test]
fn test_zfec_headers_present_in_shards() {
    use ectar::erasure::{ShardData, ZfecHeader};

    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data").unwrap();
    drop(file);

    // Create archive with chunking to ensure shards are created
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(1024)); // Force chunking

    builder.create(&[test_file]).unwrap();

    // Read first shard and verify header
    let shard_path = format!("{}.c001.s00", archive_base);
    let shard = ShardData::from_file(&std::path::PathBuf::from(&shard_path)).unwrap();

    // Verify header is present
    assert!(shard.header.is_some());

    let header = shard.header.unwrap();
    assert_eq!(header.k, 4);
    assert_eq!(header.m, 6); // 4 data + 2 parity
    assert_eq!(header.sharenum, 0);
    // padlen will vary based on data size
}

#[test]
fn test_extract_with_missing_shards_using_headers() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data for shard recovery").unwrap();
    drop(file);

    // Create archive with redundancy
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(3)
        .parity_shards(2);

    builder.create(&[test_file.clone()]).unwrap();

    // Delete some shards (but keep enough for recovery)
    fs::remove_file(format!("{}.c001.s00", archive_base)).unwrap();
    fs::remove_file(format!("{}.c001.s01", archive_base)).unwrap();

    // Delete index file
    fs::remove_file(format!("{}.index.zst", archive_base)).unwrap();

    // Extract with remaining shards (should work - have 3 out of 5, need 3)
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));

    let result = extractor.extract();

    // Should succeed using headers and Reed-Solomon recovery
    assert!(result.is_ok());
    let metadata = result.unwrap();
    assert_eq!(metadata.chunks_recovered, 1);
    assert_eq!(metadata.files_extracted, 1);

    // Verify extracted file matches original
    let extracted_file = extract_dir.join("test.txt");
    assert!(extracted_file.exists());
    let original_content = fs::read(&test_file).unwrap();
    let extracted_content = fs::read(&extracted_file).unwrap();
    assert_eq!(original_content, extracted_content);
}

#[test]
fn test_extract_without_index_insufficient_shards() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data").unwrap();
    drop(file);

    // Create archive
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2);

    builder.create(&[test_file]).unwrap();

    // Delete too many shards (need 4, delete 3, leaving only 3)
    fs::remove_file(format!("{}.c001.s00", archive_base)).unwrap();
    fs::remove_file(format!("{}.c001.s01", archive_base)).unwrap();
    fs::remove_file(format!("{}.c001.s02", archive_base)).unwrap();

    // Delete index
    fs::remove_file(format!("{}.index.zst", archive_base)).unwrap();

    // Extract should fail
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir));

    let result = extractor.extract();

    // Should fail - insufficient shards
    assert!(result.is_err());
}

#[test]
fn test_extract_multi_chunk_without_index() {
    let temp_dir = TempDir::new().unwrap();

    // Create multiple test files
    let file1 = temp_dir.path().join("file1.txt");
    let mut f = File::create(&file1).unwrap();
    f.write_all(&vec![b'A'; 2048]).unwrap();
    drop(f);

    let file2 = temp_dir.path().join("file2.txt");
    let mut f = File::create(&file2).unwrap();
    f.write_all(&vec![b'B'; 2048]).unwrap();
    drop(f);

    // Create archive with small chunks
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(3)
        .parity_shards(2)
        .chunk_size(Some(1024));

    builder.create(&[file1.clone(), file2.clone()]).unwrap();

    // Delete index
    fs::remove_file(format!("{}.index.zst", archive_base)).unwrap();

    // Extract all chunks without index
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));

    let result = extractor.extract();

    // Should successfully extract all chunks and files
    assert!(result.is_ok());
    let metadata = result.unwrap();
    assert!(metadata.chunks_recovered >= 2); // At least 2 chunks
    assert_eq!(metadata.files_extracted, 2);

    // Verify both files extracted correctly
    let extracted1 = extract_dir.join("file1.txt");
    let extracted2 = extract_dir.join("file2.txt");
    assert!(extracted1.exists());
    assert!(extracted2.exists());

    assert_eq!(fs::read(&file1).unwrap(), fs::read(&extracted1).unwrap());
    assert_eq!(fs::read(&file2).unwrap(), fs::read(&extracted2).unwrap());
}
