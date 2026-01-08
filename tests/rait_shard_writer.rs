#[cfg(test)]
mod tests {
    use ectar::io::rait::RaitShardWriter;
    use tempfile::NamedTempFile;

    #[test]
    fn test_rait_writer_creation() {
        // Create temporary files to simulate tape devices
        let temp_files: Vec<NamedTempFile> =
            (0..3).map(|_| NamedTempFile::new().unwrap()).collect();
        let tape_paths: Vec<&std::path::Path> = temp_files.iter().map(|f| f.path()).collect();

        let writer = RaitShardWriter::new(&tape_paths, 512).unwrap();
        assert_eq!(writer.num_tapes(), 3);
        assert_eq!(writer.total_shards(), 3);
    }

    #[test]
    fn test_parallel_shard_writing() {
        let temp_files: Vec<NamedTempFile> =
            (0..3).map(|_| NamedTempFile::new().unwrap()).collect();
        let tape_paths: Vec<&std::path::Path> = temp_files.iter().map(|f| f.path()).collect();

        let mut writer = RaitShardWriter::new(&tape_paths, 4).unwrap();

        // Create test shard data
        let shards = vec![
            vec![1, 2, 3, 4],    // Shard 0
            vec![5, 6, 7, 8],    // Shard 1
            vec![9, 10, 11, 12], // Shard 2
        ];

        // Write shards
        let sizes = writer.write_shards(&shards).unwrap();
        assert_eq!(sizes, vec![4, 4, 4]);

        // Check that data was written to the correct tapes
        for (i, temp_file) in temp_files.iter().enumerate() {
            let data = std::fs::read(temp_file.path()).unwrap();
            assert_eq!(data.len(), 4);
            // Each tape should have received one shard
            let expected_shard = &shards[i];
            assert_eq!(&data[..4], expected_shard.as_slice());
        }
    }

    #[test]
    fn test_shard_boundary_tracking() {
        let temp_files: Vec<NamedTempFile> =
            (0..2).map(|_| NamedTempFile::new().unwrap()).collect();
        let tape_paths: Vec<&std::path::Path> = temp_files.iter().map(|f| f.path()).collect();

        let mut writer = RaitShardWriter::new(&tape_paths, 4).unwrap();

        // Write first set of shards
        let shards1 = vec![vec![1, 2, 3, 4], vec![5, 6, 7, 8]];
        writer.write_shards(&shards1).unwrap();

        // Check positions are recorded
        assert_eq!(writer.get_shard_position(0, 0), Some(0));
        assert_eq!(writer.get_shard_position(0, 1), Some(0));

        // Write second set of shards
        let shards2 = vec![vec![13, 14, 15, 16], vec![17, 18, 19, 20]];
        writer.write_shards(&shards2).unwrap();

        // Check positions for second chunk
        assert_eq!(writer.get_shard_position(1, 0), Some(4)); // After first 4-byte block
        assert_eq!(writer.get_shard_position(1, 1), Some(4));
    }

    #[test]
    fn test_partial_tape_failure_recovery() {
        // Test with insufficient tape devices
        let temp_files: Vec<NamedTempFile> =
            (0..2).map(|_| NamedTempFile::new().unwrap()).collect();
        let tape_paths: Vec<&std::path::Path> = temp_files.iter().map(|f| f.path()).collect();

        let mut writer = RaitShardWriter::new(&tape_paths, 4).unwrap();

        // Try to write 3 shards to 2 tapes - should fail
        let shards = vec![vec![1, 2, 3, 4], vec![5, 6, 7, 8], vec![9, 10, 11, 12]];
        assert!(writer.write_shards(&shards).is_err());
    }
}
