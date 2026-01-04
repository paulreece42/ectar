use ectar::archive::create::ArchiveBuilder;
use ectar::archive::extract::ArchiveExtractor;
use ectar::archive::list::ArchiveLister;
use ectar::cli::info::ArchiveInfo;
use ectar::cli::verify::ArchiveVerifier;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

fn create_test_archive(temp_dir: &TempDir) -> String {
    let test_dir = temp_dir.path().join("testdata");
    fs::create_dir(&test_dir).unwrap();

    let file1 = test_dir.join("file1.txt");
    let mut f = File::create(&file1).unwrap();
    f.write_all(b"Content of file 1").unwrap();
    drop(f);

    let file2 = test_dir.join("file2.txt");
    let mut f = File::create(&file2).unwrap();
    f.write_all(b"Content of file 2").unwrap();
    drop(f);

    let subdir = test_dir.join("subdir");
    fs::create_dir(&subdir).unwrap();

    let file3 = subdir.join("file3.txt");
    let mut f = File::create(&file3).unwrap();
    f.write_all(b"Content of file 3 in subdir").unwrap();
    drop(f);

    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(1024 * 1024));

    builder.create(&[test_dir]).unwrap();
    archive_base
}

#[test]
fn test_full_workflow_create_list_extract() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_test_archive(&temp_dir);
    let pattern = format!("{}.c*.s*", archive_base);

    // List the archive
    let lister = ArchiveLister::new(pattern.clone());
    let list_result = lister.list().unwrap();
    assert!(list_result.total_files >= 3);

    // Show info
    let info = ArchiveInfo::new(pattern.clone());
    info.show().unwrap();

    // Verify
    let verifier = ArchiveVerifier::new(pattern.clone());
    let verify_result = verifier.verify().unwrap();
    assert_eq!(verify_result.status, ectar::cli::verify::VerificationStatus::Healthy);

    // Extract
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    let extract_result = extractor.extract().unwrap();
    assert!(extract_result.files_extracted >= 3);

    // Verify extracted files
    let extracted_file = extract_dir.join("testdata").join("file1.txt");
    assert!(extracted_file.exists());
    let content = fs::read_to_string(extracted_file).unwrap();
    assert_eq!(content, "Content of file 1");
}

#[test]
fn test_create_with_different_shard_configs() {
    let temp_dir = TempDir::new().unwrap();

    // Create test file
    let test_file = temp_dir.path().join("test.txt");
    let mut f = File::create(&test_file).unwrap();
    f.write_all(b"Test content for different configs").unwrap();
    drop(f);

    // Test with different shard configurations
    for (data, parity) in [(4, 2), (6, 3), (10, 5)] {
        let archive_base = temp_dir.path().join(format!("archive_{}_{}", data, parity))
            .to_string_lossy().to_string();

        let builder = ArchiveBuilder::new(archive_base.clone())
            .data_shards(data)
            .parity_shards(parity)
            .chunk_size(Some(1024 * 1024));

        let metadata = builder.create(&[test_file.clone()]).unwrap();
        assert_eq!(metadata.total_files, 1);

        // Verify shards were created
        let pattern = format!("{}.c*.s*", archive_base);
        let verifier = ArchiveVerifier::new(pattern);
        let report = verifier.verify().unwrap();
        assert_eq!(report.status, ectar::cli::verify::VerificationStatus::Healthy);
    }
}

#[test]
fn test_recovery_with_missing_shards() {
    let temp_dir = TempDir::new().unwrap();

    let test_file = temp_dir.path().join("test.txt");
    let mut f = File::create(&test_file).unwrap();
    let test_content = "Recovery test content";
    f.write_all(test_content.as_bytes()).unwrap();
    drop(f);

    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(1024 * 1024));

    builder.create(&[test_file]).unwrap();

    // Delete up to 2 shards (should still be recoverable)
    for i in 0..2 {
        let shard_path = temp_dir.path().join(format!("archive.c001.s{:02}", i));
        let _ = fs::remove_file(shard_path);
    }

    let pattern = format!("{}.c*.s*", archive_base);

    // Verify shows degraded
    let verifier = ArchiveVerifier::new(pattern.clone());
    let report = verifier.verify().unwrap();
    assert_eq!(report.status, ectar::cli::verify::VerificationStatus::Degraded);

    // Extract should still work
    let extract_dir = temp_dir.path().join("extract");
    fs::create_dir(&extract_dir).unwrap();

    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
    let result = extractor.extract().unwrap();
    assert_eq!(result.files_extracted, 1);

    // Verify content
    let extracted_file = extract_dir.join("test.txt");
    assert!(extracted_file.exists());
    let content = fs::read_to_string(extracted_file).unwrap();
    assert_eq!(content, test_content);
}

#[test]
fn test_list_with_various_formats() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_test_archive(&temp_dir);
    let pattern = format!("{}.c*.s*", archive_base);

    // Test text format
    let lister = ArchiveLister::new(pattern.clone())
        .output_format("text").unwrap();
    lister.list().unwrap();

    // Test JSON format
    let lister = ArchiveLister::new(pattern.clone())
        .output_format("json").unwrap();
    lister.list().unwrap();

    // Test CSV format
    let lister = ArchiveLister::new(pattern.clone())
        .output_format("csv").unwrap();
    lister.list().unwrap();

    // Test long format
    let lister = ArchiveLister::new(pattern)
        .long_format(true);
    lister.list().unwrap();
}

#[test]
fn test_verify_modes() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_test_archive(&temp_dir);
    let pattern = format!("{}.c*.s*", archive_base);

    // Quick mode
    let verifier = ArchiveVerifier::new(pattern.clone()).quick();
    let report = verifier.verify().unwrap();
    assert_eq!(report.status, ectar::cli::verify::VerificationStatus::Healthy);

    // Full mode
    let verifier = ArchiveVerifier::new(pattern.clone()).full();
    let report = verifier.verify().unwrap();
    assert_eq!(report.status, ectar::cli::verify::VerificationStatus::Healthy);
    assert!(report.chunks_verified > 0);

    // With report file
    let report_path = temp_dir.path().join("report.json");
    let verifier = ArchiveVerifier::new(pattern)
        .report(Some(report_path.clone()));
    verifier.verify().unwrap();
    assert!(report_path.exists());
}

#[test]
fn test_info_formats() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_test_archive(&temp_dir);
    let pattern = format!("{}.c*.s*", archive_base);

    // Text format
    let info = ArchiveInfo::new(pattern.clone())
        .output_format("text").unwrap();
    info.show().unwrap();

    // JSON format
    let info = ArchiveInfo::new(pattern)
        .output_format("json").unwrap();
    info.show().unwrap();
}

#[test]
fn test_extract_with_filters() {
    let temp_dir = TempDir::new().unwrap();
    let archive_base = create_test_archive(&temp_dir);
    let pattern = format!("{}.c*.s*", archive_base);

    // Extract with file filter
    let extract_dir = temp_dir.path().join("extract_filtered");
    fs::create_dir(&extract_dir).unwrap();

    let extractor = ArchiveExtractor::new(pattern.clone(), Some(extract_dir.clone()))
        .file_filters(vec!["file1".to_string()]);
    let result = extractor.extract().unwrap();
    assert!(result.files_extracted >= 1);

    // Extract with strip components
    let extract_dir2 = temp_dir.path().join("extract_stripped");
    fs::create_dir(&extract_dir2).unwrap();

    let extractor = ArchiveExtractor::new(pattern, Some(extract_dir2.clone()))
        .strip_components(1);
    let result = extractor.extract().unwrap();
    assert!(result.files_extracted >= 1);
}
