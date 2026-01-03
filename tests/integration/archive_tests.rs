use ectar::archive::create::ArchiveBuilder;
use ectar::archive::extract::ArchiveExtractor;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_create_and_extract_single_file() {
    let temp_dir = TempDir::new().unwrap();

    // Create a test directory
    let test_data_dir = temp_dir.path().join("data");
    fs::create_dir(&test_data_dir).unwrap();

    // Create a test file in the directory
    let test_file = test_data_dir.join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Hello, World!").unwrap();
    drop(file);

    // Create archive
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(1024 * 1024));

    // Pass the directory to create
    let metadata = builder.create(&[test_data_dir.clone()]).unwrap();
    assert!(metadata.total_files >= 1);

    // Extract archive
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    let extract_metadata = extractor.extract().unwrap();

    assert!(extract_metadata.files_extracted >= 1);
    assert_eq!(extract_metadata.chunks_recovered, 1);

    // Verify extracted file
    let extracted_file = extract_dir.join("data").join("test.txt");
    assert!(extracted_file.exists());
    let content = fs::read_to_string(extracted_file).unwrap();
    assert_eq!(content, "Hello, World!");
}

#[test]
fn test_create_and_extract_directory() {
    let temp_dir = TempDir::new().unwrap();

    // Create a directory structure
    let test_dir = temp_dir.path().join("testdata");
    fs::create_dir(&test_dir).unwrap();

    let file1 = test_dir.join("file1.txt");
    let mut f = File::create(&file1).unwrap();
    f.write_all(b"File 1 content").unwrap();
    drop(f);

    let subdir = test_dir.join("subdir");
    fs::create_dir(&subdir).unwrap();

    let file2 = subdir.join("file2.txt");
    let mut f = File::create(&file2).unwrap();
    f.write_all(b"File 2 content").unwrap();
    drop(f);

    // Create archive
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(1024 * 1024));

    let metadata = builder.create(&[test_dir.clone()]).unwrap();
    assert!(metadata.total_files >= 2); // At least 2 files plus directory entries

    // Extract archive
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    extractor.extract().unwrap();

    // Verify extracted files
    let extracted_file1 = extract_dir.join("testdata").join("file1.txt");
    assert!(extracted_file1.exists());
    assert_eq!(fs::read_to_string(extracted_file1).unwrap(), "File 1 content");

    let extracted_file2 = extract_dir.join("testdata").join("subdir").join("file2.txt");
    assert!(extracted_file2.exists());
    assert_eq!(fs::read_to_string(extracted_file2).unwrap(), "File 2 content");
}

#[test]
fn test_large_file_spanning_chunks() {
    let temp_dir = TempDir::new().unwrap();

    // Create a file larger than chunk size
    let test_file = temp_dir.path().join("large.bin");
    let mut file = File::create(&test_file).unwrap();
    // Write 150KB (will span multiple 50KB chunks)
    let data = vec![42u8; 150 * 1024];
    file.write_all(&data).unwrap();
    drop(file);

    // Create archive with small chunks
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(6)
        .parity_shards(3)
        .chunk_size(Some(50 * 1024)); // 50KB chunks

    let metadata = builder.create(&[test_file.clone()]).unwrap();
    assert!(metadata.chunks > 1, "Should create multiple chunks");

    // Extract archive
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    let extract_metadata = extractor.extract().unwrap();

    assert!(extract_metadata.chunks_recovered > 1);

    // Verify extracted file
    let extracted_file = extract_dir.join(test_file.file_name().unwrap());
    assert!(extracted_file.exists());
    let extracted_data = fs::read(extracted_file).unwrap();
    assert_eq!(extracted_data.len(), data.len());
    assert_eq!(extracted_data, data);
}

#[test]
fn test_extract_with_missing_shards() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file).unwrap();
    file.write_all(b"Test data for recovery").unwrap();
    drop(file);

    // Create archive
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(1024 * 1024));

    builder.create(&[test_file.clone()]).unwrap();

    // Delete one shard (should still be recoverable with 4 data + 2 parity)
    let shard_to_delete = temp_dir.path().join("archive.c001.s00");
    fs::remove_file(shard_to_delete).unwrap();

    // Extract archive
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    let result = extractor.extract();

    // Should succeed with Reed-Solomon recovery
    assert!(result.is_ok());

    // Verify extracted file
    let extracted_file = extract_dir.join(test_file.file_name().unwrap());
    assert!(extracted_file.exists());
    assert_eq!(fs::read_to_string(extracted_file).unwrap(), "Test data for recovery");
}

#[test]
fn test_empty_directory() {
    let temp_dir = TempDir::new().unwrap();

    // Create empty directory
    let empty_dir = temp_dir.path().join("empty");
    fs::create_dir(&empty_dir).unwrap();

    // Create archive
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(1024 * 1024));

    let metadata = builder.create(&[empty_dir.clone()]).unwrap();
    assert!(metadata.total_files >= 1); // At least the directory entry

    // Extract archive
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    extractor.extract().unwrap();

    // Verify extracted directory exists
    let extracted_dir = extract_dir.join("empty");
    assert!(extracted_dir.exists());
    assert!(extracted_dir.is_dir());
}

#[test]
fn test_multiple_files_same_directory() {
    let temp_dir = TempDir::new().unwrap();

    // Create multiple files
    for i in 1..=5 {
        let file_path = temp_dir.path().join(format!("file{}.txt", i));
        let mut file = File::create(&file_path).unwrap();
        write!(file, "Content of file {}", i).unwrap();
    }

    // Create archive
    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(1024 * 1024));

    let files: Vec<PathBuf> = (1..=5)
        .map(|i| temp_dir.path().join(format!("file{}.txt", i)))
        .collect();

    let metadata = builder.create(&files).unwrap();
    assert_eq!(metadata.total_files, 5);

    // Extract archive
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let pattern = format!("{}.c*.s*", archive_base);
    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    let extract_metadata = extractor.extract().unwrap();

    assert_eq!(extract_metadata.files_extracted, 5);

    // Verify all files
    for i in 1..=5 {
        let extracted_file = extract_dir.join(format!("file{}.txt", i));
        assert!(extracted_file.exists());
        let content = fs::read_to_string(extracted_file).unwrap();
        assert_eq!(content, format!("Content of file {}", i));
    }
}
