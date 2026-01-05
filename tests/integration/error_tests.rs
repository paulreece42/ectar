use ectar::archive::create::ArchiveBuilder;
use ectar::archive::extract::ArchiveExtractor;
use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;

#[test]
fn test_extract_insufficient_shards() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data").unwrap();
    drop(file);

    // Create archive with 4 data + 2 parity shards
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(1024 * 1024));

    builder.create(&[test_file]).unwrap();

    // Delete too many shards (need 4 data shards, delete 3 leaving only 3)
    fs::remove_file(temp_dir.path().join("archive.c001.s00")).unwrap();
    fs::remove_file(temp_dir.path().join("archive.c001.s01")).unwrap();
    fs::remove_file(temp_dir.path().join("archive.c001.s02")).unwrap();

    // Extract should fail
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir));
    let result = extractor.extract();

    // Should fail due to insufficient shards
    assert!(result.is_err());
}

#[test]
fn test_extract_missing_index() {
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
        .parity_shards(2)
        .chunk_size(Some(1024 * 1024));

    builder.create(&[test_file.clone()]).unwrap();

    // Delete index file
    let index_file = temp_dir.path().join("archive.index.zst");
    fs::remove_file(index_file).unwrap();

    // Extract should now SUCCEED using zfec headers (emergency recovery mode)
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    let result = extractor.extract();

    assert!(result.is_ok(), "Extraction should succeed without index using zfec headers");

    // Verify extracted file matches original
    let extracted_file = extract_dir.join("test.txt");
    assert!(extracted_file.exists());
    let extracted_content = fs::read_to_string(extracted_file).unwrap();
    assert_eq!(extracted_content, "Test");
}

#[test]
fn test_create_invalid_shard_count() {
    let temp_dir = TempDir::new().unwrap();

    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test").unwrap();
    drop(file);

    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Data shards = 0 should fail
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(0)
        .parity_shards(2);

    let result = builder.create(&[test_file.clone()]);
    assert!(result.is_err());

    // Parity shards = 0 should fail
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(0);

    let result = builder.create(&[test_file.clone()]);
    assert!(result.is_err());

    // Total shards > 256 should fail
    let builder = ArchiveBuilder::new(archive_base)
        .data_shards(200)
        .parity_shards(100);

    let result = builder.create(&[test_file]);
    assert!(result.is_err());
}

#[test]
fn test_create_nonexistent_file() {
    let temp_dir = TempDir::new().unwrap();

    let nonexistent = temp_dir.path().join("nonexistent.txt");
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    let builder = ArchiveBuilder::new(archive_base)
        .data_shards(4)
        .parity_shards(2);

    let result = builder.create(&[nonexistent]);
    assert!(result.is_err());
}

#[test]
fn test_invalid_compression_level() {
    let temp_dir = TempDir::new().unwrap();

    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test").unwrap();
    drop(file);

    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Compression level 0 is invalid (min is 1)
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .compression_level(0);

    let result = builder.create(&[test_file.clone()]);
    assert!(result.is_err());

    // Compression level 23 is invalid (max is 22)
    let builder = ArchiveBuilder::new(archive_base)
        .data_shards(4)
        .parity_shards(2)
        .compression_level(23);

    let result = builder.create(&[test_file]);
    assert!(result.is_err());
}

#[test]
fn test_partial_extraction() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data for partial extraction").unwrap();
    drop(file);

    // Create archive
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(1024 * 1024));

    builder.create(&[test_file]).unwrap();

    // Delete too many shards
    fs::remove_file(temp_dir.path().join("archive.c001.s00")).unwrap();
    fs::remove_file(temp_dir.path().join("archive.c001.s01")).unwrap();
    fs::remove_file(temp_dir.path().join("archive.c001.s02")).unwrap();

    // Extract without partial flag should fail
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern.clone(), Some(extract_dir.clone()));
    let result = extractor.extract();
    assert!(result.is_err());

    // Extract with partial flag should succeed (but with no files since we can't recover the chunk)
    fs::remove_dir_all(&extract_dir).unwrap();
    fs::create_dir(&extract_dir).unwrap();

    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir))
        .partial(true);
    let result = extractor.extract();

    // With partial mode, it should succeed even if no chunks recovered
    assert!(result.is_ok());
    let metadata = result.unwrap();
    assert_eq!(metadata.chunks_failed, 1);
}

#[test]
fn test_extract_no_shards() {
    let temp_dir = TempDir::new().unwrap();

    // Try to extract from non-existent shards
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}/nonexistent.c*.s*", temp_dir.path().display());
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir));
    let result = extractor.extract();

    // Should fail - no shards found
    assert!(result.is_err());
}

// ============================================================================
// Boundary Condition Tests - Shard Counts
// ============================================================================

#[test]
fn test_create_with_exactly_256_total_shards() {
    let temp_dir = TempDir::new().unwrap();

    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data").unwrap();
    drop(file);

    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // 200 data + 56 parity = 256 (exactly at the limit)
    let builder = ArchiveBuilder::new(archive_base)
        .data_shards(200)
        .parity_shards(56);

    let result = builder.create(&[test_file]);

    // Should succeed at exactly 256 total shards
    assert!(result.is_ok());
}

#[test]
fn test_create_with_257_total_shards() {
    let temp_dir = TempDir::new().unwrap();

    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data").unwrap();
    drop(file);

    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // 200 data + 57 parity = 257 (over the limit)
    let builder = ArchiveBuilder::new(archive_base)
        .data_shards(200)
        .parity_shards(57);

    let result = builder.create(&[test_file]);

    // Should fail when exceeding 256 total shards
    assert!(result.is_err());
}

#[test]
fn test_extract_with_exactly_minimum_shards() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data for minimum shard recovery").unwrap();
    drop(file);

    // Create archive with 4 data + 2 parity shards
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(1024 * 1024));

    builder.create(&[test_file]).unwrap();

    // Delete exactly 2 shards, leaving exactly 4 (the minimum needed)
    fs::remove_file(temp_dir.path().join("archive.c001.s00")).unwrap();
    fs::remove_file(temp_dir.path().join("archive.c001.s01")).unwrap();

    // Extract should succeed with exactly the minimum shards
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    let result = extractor.extract();

    // Should succeed with exactly 4 shards available (minimum required)
    assert!(result.is_ok());

    // Verify extracted file exists
    let extracted_file = extract_dir.join("test.txt");
    assert!(extracted_file.exists());
}

#[test]
fn test_extract_with_one_less_than_minimum() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data").unwrap();
    drop(file);

    // Create archive with 4 data + 2 parity shards
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(1024 * 1024));

    builder.create(&[test_file]).unwrap();

    // Delete 3 shards, leaving only 3 (one less than minimum of 4)
    fs::remove_file(temp_dir.path().join("archive.c001.s00")).unwrap();
    fs::remove_file(temp_dir.path().join("archive.c001.s01")).unwrap();
    fs::remove_file(temp_dir.path().join("archive.c001.s02")).unwrap();

    // Extract should fail
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir));
    let result = extractor.extract();

    // Should fail with one less than minimum shards
    assert!(result.is_err());
}

// ============================================================================
// Boundary Condition Tests - Compression Levels
// ============================================================================

#[test]
fn test_compression_level_exactly_1() {
    let temp_dir = TempDir::new().unwrap();

    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data for compression").unwrap();
    drop(file);

    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Compression level 1 is minimum valid
    let builder = ArchiveBuilder::new(archive_base)
        .data_shards(4)
        .parity_shards(2)
        .compression_level(1);

    let result = builder.create(&[test_file]);

    // Should succeed at minimum valid compression level
    assert!(result.is_ok());
}

#[test]
fn test_compression_level_exactly_22() {
    let temp_dir = TempDir::new().unwrap();

    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data for compression").unwrap();
    drop(file);

    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Compression level 22 is maximum valid
    let builder = ArchiveBuilder::new(archive_base)
        .data_shards(4)
        .parity_shards(2)
        .compression_level(22);

    let result = builder.create(&[test_file]);

    // Should succeed at maximum valid compression level
    assert!(result.is_ok());
}

#[test]
fn test_compression_level_negative() {
    let temp_dir = TempDir::new().unwrap();

    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test").unwrap();
    drop(file);

    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

    // Negative compression level should fail
    let builder = ArchiveBuilder::new(archive_base)
        .data_shards(4)
        .parity_shards(2)
        .compression_level(-5);

    let result = builder.create(&[test_file]);

    // Should fail with negative compression level
    assert!(result.is_err());
}

// ============================================================================
// Partial Extraction & Recovery Edge Cases
// ============================================================================

#[test]
fn test_partial_extraction_with_some_chunks_failed() {
    let temp_dir = TempDir::new().unwrap();

    // Create multiple files to generate multiple chunks
    let file1 = temp_dir.path().join("file1.txt");
    let mut f = File::create(&file1).unwrap();
    f.write_all(&vec![b'A'; 10000]).unwrap();
    drop(f);

    let file2 = temp_dir.path().join("file2.txt");
    let mut f = File::create(&file2).unwrap();
    f.write_all(&vec![b'B'; 10000]).unwrap();
    drop(f);

    let file3 = temp_dir.path().join("file3.txt");
    let mut f = File::create(&file3).unwrap();
    f.write_all(&vec![b'C'; 10000]).unwrap();
    drop(f);

    // Create archive with small chunk size to generate multiple chunks
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(5000)); // Small chunks

    let metadata = builder.create(&[file1, file2, file3]).unwrap();

    // Only proceed if we have multiple chunks
    if metadata.chunks > 1 {
        // Delete shards from the second chunk only
        fs::remove_file(temp_dir.path().join("archive.c002.s00")).ok();
        fs::remove_file(temp_dir.path().join("archive.c002.s01")).ok();
        fs::remove_file(temp_dir.path().join("archive.c002.s02")).ok();

        // Extract with partial mode
        let extract_dir = temp_dir.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();

        let pattern = format!("{}.c*.s*", archive_base);
        let extractor = ArchiveExtractor::new(pattern, Some(extract_dir))
            .partial(true);

        let result = extractor.extract();

        // Should succeed and extract files from recoverable chunks
        assert!(result.is_ok());
        let extract_metadata = result.unwrap();

        // Should have at least one failed chunk
        assert!(extract_metadata.chunks_failed > 0);
    }
}

#[test]
fn test_partial_extraction_corrupted_tar_stream() {
    // This test would require corrupting the middle of reconstructed data
    // which is complex to set up. Skipping detailed implementation.
    // Documents that partial mode should handle mid-stream tar corruption
}

#[test]
fn test_partial_vs_non_partial_behavior() {
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

    // Delete too many shards
    fs::remove_file(temp_dir.path().join("archive.c001.s00")).unwrap();
    fs::remove_file(temp_dir.path().join("archive.c001.s01")).unwrap();
    fs::remove_file(temp_dir.path().join("archive.c001.s02")).unwrap();

    // Try non-partial extraction - should fail
    let extract_dir1 = temp_dir.path().join("extract1");
    fs::create_dir(&extract_dir1).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern.clone(), Some(extract_dir1));
    let result1 = extractor.extract();

    assert!(result1.is_err());

    // Try partial extraction - should succeed
    let extract_dir2 = temp_dir.path().join("extract2");
    fs::create_dir(&extract_dir2).unwrap();

    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir2))
        .partial(true);
    let result2 = extractor.extract();

    assert!(result2.is_ok());
}
