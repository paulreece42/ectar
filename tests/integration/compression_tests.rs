use ectar::archive::create::ArchiveBuilder;
use ectar::archive::extract::ArchiveExtractor;
use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;

// ============================================================================
// Compression & Decompression Error Tests
// ============================================================================

#[test]
fn test_decompress_invalid_zstd_data() {
    use std::path::PathBuf;

    let temp_dir = TempDir::new().unwrap();

    // Create a fake .zst file with random/invalid data
    let fake_zst = temp_dir.path().join("fake.zst");
    let mut file = File::create(&fake_zst).unwrap();
    file.write_all(b"This is not valid zstd compressed data!")
        .unwrap();
    drop(file);

    // Attempting to decompress should fail
    let result = zstd::decode_all(std::fs::read(&fake_zst).unwrap().as_slice());

    // Should fail with decompression error
    assert!(result.is_err());
}

#[test]
fn test_decompress_truncated_zstd_stream() {
    let temp_dir = TempDir::new().unwrap();

    // Create valid compressed data
    let original_data = b"Test data for compression";
    let compressed = zstd::encode_all(&original_data[..], 3).unwrap();

    // Truncate to 50%
    let truncated = &compressed[..compressed.len() / 2];

    // Write truncated data
    let truncated_file = temp_dir.path().join("truncated.zst");
    let mut file = File::create(&truncated_file).unwrap();
    file.write_all(truncated).unwrap();
    drop(file);

    // Attempting to decompress should fail
    let result = zstd::decode_all(std::fs::read(&truncated_file).unwrap().as_slice());

    // Should fail with decompression error
    assert!(result.is_err());
}

#[test]
fn test_no_compression_mode() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data for no compression").unwrap();
    drop(file);

    // Create archive with no compression
    let archive_base = temp_dir
        .path()
        .join("archive")
        .to_string_lossy()
        .to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .no_compression(true);

    let result = builder.create(&[test_file]);

    // Should succeed
    assert!(result.is_ok());

    // Extract to verify
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    let extract_result = extractor.extract();

    assert!(extract_result.is_ok());

    // Verify extracted file
    let extracted_file = extract_dir.join("test.txt");
    assert!(extracted_file.exists());
    let content = fs::read_to_string(extracted_file).unwrap();
    assert_eq!(content, "Test data for no compression");
}
