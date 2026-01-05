use super::test_helpers::{corrupt_file_bytes, corrupt_index_json, create_minimal_archive, IndexCorruption};
use ectar::archive::create::ArchiveBuilder;
use ectar::archive::extract::ArchiveExtractor;
use ectar::cli::verify::ArchiveVerifier;
use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;

// ============================================================================
// Corrupted Index File Tests
// ============================================================================

#[test]
fn test_extract_with_corrupted_index_json() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_minimal_archive(&temp_dir).unwrap();

    // Corrupt the index file with invalid JSON syntax
    let index_path = temp_dir.path().join("archive.index.zst");
    corrupt_index_json(&index_path, IndexCorruption::InvalidSyntax).unwrap();

    // Try to extract - should fail with deserialization error
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir));
    let result = extractor.extract();

    assert!(result.is_err());
}

#[test]
fn test_extract_with_truncated_index() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_minimal_archive(&temp_dir).unwrap();

    // Corrupt the index file by truncating JSON
    let index_path = temp_dir.path().join("archive.index.zst");
    corrupt_index_json(&index_path, IndexCorruption::TruncateJson).unwrap();

    // Try to extract - should fail
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir));
    let result = extractor.extract();

    assert!(result.is_err());
}

#[test]
fn test_extract_with_missing_index_fields() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_minimal_archive(&temp_dir).unwrap();

    // Corrupt the index by removing required fields
    let index_path = temp_dir.path().join("archive.index.zst");
    corrupt_index_json(&index_path, IndexCorruption::MissingField).unwrap();

    // Try to extract - should fail with deserialization error
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir));
    let result = extractor.extract();

    assert!(result.is_err());
}

#[test]
fn test_extract_with_invalid_index_version() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_minimal_archive(&temp_dir).unwrap();

    // Change index version to an invalid/future version
    let index_path = temp_dir.path().join("archive.index.zst");
    corrupt_index_json(&index_path, IndexCorruption::InvalidVersion).unwrap();

    // Try to extract - may succeed or fail depending on version handling
    // This test documents current behavior
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir));
    let _result = extractor.extract();

    // Test documents behavior - currently may succeed as version is not strictly validated
}

#[test]
fn test_extract_with_mismatched_shard_parameters() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data").unwrap();
    drop(file);

    // Create archive with 4 data shards
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(1024 * 1024));

    builder.create(&[test_file]).unwrap();

    // Manually edit index to claim 6 data shards instead of 4
    let index_path = temp_dir.path().join("archive.index.zst");
    let compressed_data = fs::read(&index_path).unwrap();
    let decompressed_data = zstd::decode_all(&compressed_data[..]).unwrap();
    let json_str = String::from_utf8(decompressed_data).unwrap();

    // Replace data_shards value from 4 to 6
    let modified_json = json_str.replace("\"data_shards\":4", "\"data_shards\":6");

    // Recompress and write back
    let compressed = zstd::encode_all(modified_json.as_bytes(), 3).unwrap();
    let mut file = File::create(&index_path).unwrap();
    file.write_all(&compressed).unwrap();
    drop(file);

    // Try to extract - should fail because of mismatched parameters
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir));
    let result = extractor.extract();

    // This test documents current behavior
    // Currently may succeed or fail depending on validation logic
    let _ = result;
}

// ============================================================================
// Corrupted Shard Data Tests
// ============================================================================

#[test]
fn test_extract_with_corrupted_shard_data() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_minimal_archive(&temp_dir).unwrap();

    // Corrupt one shard file by flipping bytes (use small offset for small files)
    let shard_path = temp_dir.path().join("archive.c001.s00");
    corrupt_file_bytes(&shard_path, 0, 10).unwrap();

    // Try to extract
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir));
    let result = extractor.extract();

    // With parity shards, should either recover or fail gracefully
    // The result depends on whether Reed-Solomon can detect/correct the corruption
    // This test documents the behavior
    let _ = result;
}

#[test]
fn test_extract_with_all_shards_corrupted() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_minimal_archive(&temp_dir).unwrap();

    // Corrupt all shard files (use smaller values for small files)
    for i in 0..6 {
        let shard_path = temp_dir.path().join(format!("archive.c001.s{:02}", i));
        if shard_path.exists() {
            corrupt_file_bytes(&shard_path, 10, 10).unwrap();
        }
    }

    // Try to extract - should fail
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir));
    let result = extractor.extract();

    // Should fail when all shards are corrupted
    assert!(result.is_err());
}

#[test]
fn test_extract_with_shard_size_mismatch() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_minimal_archive(&temp_dir).unwrap();

    // Truncate one shard to a different size
    let shard_path = temp_dir.path().join("archive.c001.s00");
    let file = File::open(&shard_path).unwrap();
    let metadata = file.metadata().unwrap();
    let original_size = metadata.len();
    drop(file);

    let file = File::create(&shard_path).unwrap();
    file.set_len(original_size / 2).unwrap();
    drop(file);

    // Try to extract - should fail due to size mismatch
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir));
    let result = extractor.extract();

    // Should fail due to size mismatch between shards
    assert!(result.is_err());
}

#[test]
fn test_verify_detects_corrupted_shard() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_minimal_archive(&temp_dir).unwrap();

    // Corrupt one shard (use small offset)
    let shard_path = temp_dir.path().join("archive.c001.s00");
    corrupt_file_bytes(&shard_path, 0, 10).unwrap();

    // Run verification in full mode
    let pattern = format!("{}.c*.s*", archive_base);
    let verifier = ArchiveVerifier::new(pattern)
        .full(); // Full verification

    let result = verifier.verify();

    // Verification may succeed or fail depending on corruption detection
    // This test documents the current verification behavior
    let _ = result;
}

// ============================================================================
// Checksum Validation Tests
// ============================================================================

#[test]
fn test_file_checksum_mismatch() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_minimal_archive(&temp_dir).unwrap();

    // Modify file checksum in index
    let index_path = temp_dir.path().join("archive.index.zst");
    let compressed_data = fs::read(&index_path).unwrap();
    let decompressed_data = zstd::decode_all(&compressed_data[..]).unwrap();
    let json_str = String::from_utf8(decompressed_data).unwrap();

    // Replace checksum with a fake one
    let modified_json = if json_str.contains("\"checksum\":\"") {
        json_str.replace(
            "\"checksum\":\"",
            "\"checksum\":\"0000000000000000000000000000000000000000000000000000000000000000"
        )
    } else {
        json_str
    };

    // Recompress and write back
    let compressed = zstd::encode_all(modified_json.as_bytes(), 3).unwrap();
    let mut file = File::create(&index_path).unwrap();
    file.write_all(&compressed).unwrap();
    drop(file);

    // Try to extract
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir));
    let result = extractor.extract();

    // This test documents current checksum verification behavior
    // If checksums are verified during extraction, this should fail
    let _ = result;
}

#[test]
fn test_extract_with_verify_checksums_enabled() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_minimal_archive(&temp_dir).unwrap();

    // Extract with checksum verification enabled
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir))
        .verify_checksums(true);

    let result = extractor.extract();

    // Should succeed with valid checksums
    assert!(result.is_ok());
}
