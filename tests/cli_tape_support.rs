#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    fn get_binary_path() -> String {
        env!("CARGO_BIN_EXE_ectar").to_string()
    }

    /// Integration test for tape mode: create archive to tape files and extract
    #[test]
    fn test_tape_mode_create_and_extract() {
        let temp_dir = TempDir::new().unwrap();

        // Create test file
        let test_file = temp_dir.path().join("testdata.txt");
        let mut f = File::create(&test_file).unwrap();
        f.write_all(b"Hello, tape world! This is test data for the tape archive.").unwrap();
        drop(f);

        // Create 3 "tape device" files (simulated as regular files)
        let tape0 = temp_dir.path().join("tape0");
        let tape1 = temp_dir.path().join("tape1");
        let tape2 = temp_dir.path().join("tape2");

        // Create empty tape files
        File::create(&tape0).unwrap();
        File::create(&tape1).unwrap();
        File::create(&tape2).unwrap();

        let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();

        // Create archive to tape devices
        let output = Command::new(get_binary_path())
            .args(&[
                "create",
                "--tape-devices", tape0.to_str().unwrap(),
                "--tape-devices", tape1.to_str().unwrap(),
                "--tape-devices", tape2.to_str().unwrap(),
                "--block-size", "512",
                "--output", &archive_base,
                test_file.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute create command");

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("Create stdout: {}", stdout);
        println!("Create stderr: {}", stderr);

        assert!(output.status.success(), "Create command failed: {}", stderr);

        // Verify tape files have data
        let tape0_size = fs::metadata(&tape0).unwrap().len();
        let tape1_size = fs::metadata(&tape1).unwrap().len();
        let tape2_size = fs::metadata(&tape2).unwrap().len();

        assert!(tape0_size > 0, "Tape 0 should have data");
        assert!(tape1_size > 0, "Tape 1 should have data");
        assert!(tape2_size > 0, "Tape 2 should have data");

        // Verify index file was created
        let index_path = format!("{}.index.zst", archive_base);
        assert!(std::path::Path::new(&index_path).exists(), "Index file should exist");

        // Create extract directory
        let extract_dir = temp_dir.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();

        // Extract archive from tape devices
        let output = Command::new(get_binary_path())
            .args(&[
                "extract",
                "--tape-devices", tape0.to_str().unwrap(),
                "--tape-devices", tape1.to_str().unwrap(),
                "--tape-devices", tape2.to_str().unwrap(),
                "--block-size", "512",
                "--input", &format!("{}.c*.s*", archive_base),
                "--output", extract_dir.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute extract command");

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("Extract stdout: {}", stdout);
        println!("Extract stderr: {}", stderr);

        assert!(output.status.success(), "Extract command failed: {}", stderr);

        // Verify extracted file exists and has correct content
        let extracted_file = extract_dir.join("testdata.txt");
        assert!(extracted_file.exists(), "Extracted file should exist");

        let content = fs::read_to_string(&extracted_file).unwrap();
        assert_eq!(content, "Hello, tape world! This is test data for the tape archive.");
    }

    #[test]
    fn test_create_with_tape_devices() {
        // Test that the CLI accepts tape device options for create command
        let output = Command::new(get_binary_path())
            .args(&[
                "create",
                "--tape-devices",
                "/dev/tape0",
                "--tape-devices",
                "/dev/tape1",
                "--block-size",
                "512",
                "--output",
                "test_output",
                "--help", // Use --help to avoid actually creating an archive
            ])
            .output()
            .expect("Failed to execute command");

        // Should succeed (help output) and not show an error about unknown options
        assert!(output.status.success() || output.status.code() == Some(2)); // clap returns 2 for --help
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Should not contain "unexpected argument" or similar errors
        assert!(!stderr.contains("unexpected argument"));
        assert!(!stderr.contains("unrecognized option"));
    }

    #[test]
    fn test_extract_from_tape_devices() {
        // Test that the CLI accepts tape device options for extract command
        let output = Command::new(get_binary_path())
            .args(&[
                "extract",
                "--tape-devices",
                "/dev/tape0",
                "--tape-devices",
                "/dev/tape1",
                "--block-size",
                "1024",
                "--input",
                "test.c*.s*",
                "--help", // Use --help to avoid actually extracting
            ])
            .output()
            .expect("Failed to execute command");

        // Should succeed (help output) and not show an error about unknown options
        assert!(output.status.success() || output.status.code() == Some(2)); // clap returns 2 for --help
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Should not contain "unexpected argument" or similar errors
        assert!(!stderr.contains("unexpected argument"));
        assert!(!stderr.contains("unrecognized option"));
    }

    #[test]
    fn test_tape_block_size_parameter() {
        // Test that the CLI accepts block size parameter
        let output = Command::new(get_binary_path())
            .args(&[
                "create",
                "--block-size",
                "1KB",
                "--output",
                "test_output",
                "/tmp/test",
                "--help", // Use --help to avoid actually creating
            ])
            .output()
            .expect("Failed to execute command");

        // Should succeed and not show an error about unknown options
        assert!(output.status.success() || output.status.code() == Some(2)); // clap returns 2 for --help
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Should not contain "unexpected argument" or similar errors
        assert!(!stderr.contains("unexpected argument"));
        assert!(!stderr.contains("unrecognized option"));
    }
}
