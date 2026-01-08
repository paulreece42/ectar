#[cfg(test)]
mod tests {
    use ectar::io::tape_reader::TapeShardReader;
    use std::collections::HashMap;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_tape_shard_reader_creation() {
        // Create temporary files to simulate tape devices
        let temp_files: Vec<NamedTempFile> =
            (0..2).map(|_| NamedTempFile::new().unwrap()).collect();
        let tape_paths: Vec<(&str, &std::path::Path)> = vec![
            ("tape0", temp_files[0].path()),
            ("tape1", temp_files[1].path()),
        ];

        let shard_positions = HashMap::new();
        let reader = TapeShardReader::new(&tape_paths, shard_positions).unwrap();

        let device_names = reader.device_names();
        assert_eq!(device_names.len(), 2);
        assert!(device_names.contains(&"tape0"));
        assert!(device_names.contains(&"tape1"));
    }

    #[test]
    fn test_seek_to_specific_shard() {
        let temp_file = NamedTempFile::new().unwrap();
        let tape_paths = vec![("tape0", temp_file.path())];

        let mut shard_positions = HashMap::new();
        shard_positions.insert((0, 0), ("tape0".to_string(), 10));
        shard_positions.insert((0, 1), ("tape0".to_string(), 20));

        let mut reader = TapeShardReader::new(&tape_paths, shard_positions).unwrap();

        // Seek to chunk 0, shard 0 (position 10)
        reader.seek_to_shard(0, 0).unwrap();

        // Seek to chunk 0, shard 1 (position 20)
        reader.seek_to_shard(0, 1).unwrap();

        // Try to seek to non-existent shard
        assert!(reader.seek_to_shard(1, 0).is_err());
    }

    #[test]
    fn test_read_shard_from_position() {
        let temp_file = NamedTempFile::new().unwrap();

        // Write some test data to the file
        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .open(temp_file.path())
                .unwrap();
            file.write_all(&[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        }

        let tape_paths = vec![("tape0", temp_file.path())];
        let shard_positions = HashMap::new();

        let mut reader = TapeShardReader::new(&tape_paths, shard_positions).unwrap();

        // Read from beginning
        let data = reader.read_shard_data("tape0", 4).unwrap();
        assert_eq!(data, vec![1, 2, 3, 4]);

        // Read next chunk
        let data = reader.read_shard_data("tape0", 4).unwrap();
        assert_eq!(data, vec![5, 6, 7, 8]);

        // Try to read from non-existent device
        assert!(reader.read_shard_data("nonexistent", 4).is_err());
    }

    #[test]
    fn test_multi_tape_parallel_reading() {
        let temp_files: Vec<NamedTempFile> =
            (0..3).map(|_| NamedTempFile::new().unwrap()).collect();

        // Write different data to each tape
        for (i, temp_file) in temp_files.iter().enumerate() {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .open(temp_file.path())
                .unwrap();
            let data = vec![(i + 1) as u8; 10]; // tape0: [1,1,1,...], tape1: [2,2,2,...], etc.
            file.write_all(&data).unwrap();
        }

        let tape_paths: Vec<(&str, &std::path::Path)> = vec![
            ("tape0", temp_files[0].path()),
            ("tape1", temp_files[1].path()),
            ("tape2", temp_files[2].path()),
        ];

        let mut shard_positions = HashMap::new();
        shard_positions.insert((0, 0), ("tape0".to_string(), 0));
        shard_positions.insert((0, 1), ("tape1".to_string(), 0));
        shard_positions.insert((0, 2), ("tape2".to_string(), 0));

        let mut reader = TapeShardReader::new(&tape_paths, shard_positions).unwrap();

        // Read from all tapes simultaneously (simulating parallel access)
        let data0 = reader.read_shard(0, 0, 4).unwrap();
        let data1 = reader.read_shard(0, 1, 4).unwrap();
        let data2 = reader.read_shard(0, 2, 4).unwrap();

        assert_eq!(data0, vec![1, 1, 1, 1]);
        assert_eq!(data1, vec![2, 2, 2, 2]);
        assert_eq!(data2, vec![3, 3, 3, 3]);
    }
}
