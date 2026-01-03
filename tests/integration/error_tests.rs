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

    builder.create(&[test_file]).unwrap();

    // Delete index file
    let index_file = temp_dir.path().join("archive.index.zst");
    fs::remove_file(index_file).unwrap();

    // Extract should fail
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir));
    let result = extractor.extract();

    assert!(result.is_err());
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
