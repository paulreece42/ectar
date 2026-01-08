#[cfg(test)]
mod tests {
    use ectar::io::streaming_shard_writer::ShardOutput;
    use ectar::io::tape::TapeShardOutput;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_tape_shard_output_creation() {
        // Create a temporary file to simulate a tape device
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path();

        let output = TapeShardOutput::new(temp_path, 512).unwrap();
        assert_eq!(output.block_size(), 512);
        assert_eq!(output.current_position(), 0);
    }

    #[test]
    fn test_block_aligned_writing() {
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path();
        let mut output = TapeShardOutput::new(temp_path, 4).unwrap(); // Small block size for testing

        // Write partial block - should be buffered
        assert_eq!(output.write(&[1, 2]).unwrap(), 2);
        assert_eq!(output.current_position(), 0); // Position doesn't advance until block is written

        // Write enough to complete the block
        assert_eq!(output.write(&[3, 4]).unwrap(), 2);
        assert_eq!(output.current_position(), 4); // Now position advances

        // Check data was written correctly
        let data = std::fs::read(temp_path).unwrap();
        assert_eq!(&data[..4], &[1, 2, 3, 4]);
    }

    #[test]
    fn test_partial_block_flush() {
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path();
        let mut output = TapeShardOutput::new(temp_path, 4).unwrap();

        // Write partial block
        assert_eq!(output.write(&[1]).unwrap(), 1);

        // Flush should pad to block boundary
        output.flush().unwrap();
        assert_eq!(output.current_position(), 4); // Full block written

        // Check data was written with padding
        let data = std::fs::read(temp_path).unwrap();
        assert_eq!(&data[..4], &[1, 0, 0, 0]); // Padded with zeros
    }

    #[test]
    fn test_shard_output_finish() {
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path();
        let mut output = TapeShardOutput::new(temp_path, 4).unwrap();

        // Write some data
        output.write_all(&[1, 2, 3, 4]).unwrap();

        // Finish should return total bytes written
        let total_written = output.finish().unwrap();
        assert_eq!(total_written, 4);
    }

    #[test]
    fn test_tape_device_error_handling() {
        // Test with invalid device path
        let invalid_path = std::path::Path::new("/dev/nonexistent_tape");
        let result = TapeShardOutput::new(invalid_path, 512);
        assert!(result.is_err());
    }
}
