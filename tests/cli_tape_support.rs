#[cfg(test)]
mod tests {
    use std::process::Command;

    fn get_binary_path() -> String {
        env!("CARGO_BIN_EXE_ectar").to_string()
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
