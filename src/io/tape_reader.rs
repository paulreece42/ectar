use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek};
use std::path::Path;

use crate::error::Result;

/// TapeShardReader manages reading shards from multiple tape devices
/// with position-based seeking for recovery operations
pub struct TapeShardReader {
    tape_devices: HashMap<String, File>,
    shard_positions: HashMap<(usize, usize), (String, u64)>, // (chunk_num, shard_num) -> (device_name, position)
}

impl TapeShardReader {
    /// Create a new TapeShardReader with the specified tape device paths
    /// and pre-computed shard positions
    pub fn new(
        tape_paths: &[(&str, &Path)],
        shard_positions: HashMap<(usize, usize), (String, u64)>,
    ) -> Result<Self> {
        let mut tape_devices = HashMap::new();

        for (device_name, device_path) in tape_paths {
            let device = std::fs::OpenOptions::new().read(true).open(device_path)?;
            tape_devices.insert(device_name.to_string(), device);
        }

        Ok(Self {
            tape_devices,
            shard_positions,
        })
    }

    /// Seek to the position of a specific shard on its tape device
    pub fn seek_to_shard(&mut self, chunk_num: usize, shard_num: usize) -> Result<()> {
        if let Some((device_name, position)) = self.shard_positions.get(&(chunk_num, shard_num)) {
            if let Some(device) = self.tape_devices.get_mut(device_name) {
                device.seek(std::io::SeekFrom::Start(*position))?;
                Ok(())
            } else {
                Err(crate::error::EctarError::InvalidParameters(format!(
                    "Tape device '{}' not found",
                    device_name
                )))
            }
        } else {
            Err(crate::error::EctarError::InvalidParameters(format!(
                "Position not found for chunk {}, shard {}",
                chunk_num, shard_num
            )))
        }
    }

    /// Read shard data from the current tape position
    /// Assumes seek_to_shard() was called first
    pub fn read_shard_data(&mut self, device_name: &str, expected_size: usize) -> Result<Vec<u8>> {
        if let Some(device) = self.tape_devices.get_mut(device_name) {
            let mut buffer = vec![0; expected_size];
            let bytes_read = device.read(&mut buffer)?;

            if bytes_read != expected_size {
                return Err(crate::error::EctarError::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    format!(
                        "Expected to read {} bytes, got {}",
                        expected_size, bytes_read
                    ),
                )));
            }

            Ok(buffer)
        } else {
            Err(crate::error::EctarError::InvalidParameters(format!(
                "Tape device '{}' not found",
                device_name
            )))
        }
    }

    /// Read a specific shard by chunk and shard number
    pub fn read_shard(
        &mut self,
        chunk_num: usize,
        shard_num: usize,
        expected_size: usize,
    ) -> Result<Vec<u8>> {
        if let Some((device_name, _)) = self.shard_positions.get(&(chunk_num, shard_num)) {
            let device_name = device_name.clone();
            self.seek_to_shard(chunk_num, shard_num)?;
            self.read_shard_data(&device_name, expected_size)
        } else {
            Err(crate::error::EctarError::InvalidParameters(format!(
                "Position not found for chunk {}, shard {}",
                chunk_num, shard_num
            )))
        }
    }

    /// Get all available tape device names
    pub fn device_names(&self) -> Vec<&str> {
        self.tape_devices.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a specific shard position is known
    pub fn has_shard_position(&self, chunk_num: usize, shard_num: usize) -> bool {
        self.shard_positions.contains_key(&(chunk_num, shard_num))
    }

    /// Get the position of a specific shard
    pub fn get_shard_position(&self, chunk_num: usize, shard_num: usize) -> Option<&(String, u64)> {
        self.shard_positions.get(&(chunk_num, shard_num))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_read_shard_data() {
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
    fn test_read_shard() {
        let temp_file = NamedTempFile::new().unwrap();

        // Write test data with known positions
        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .open(temp_file.path())
                .unwrap();
            file.write_all(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap(); // Padding to position 10
            file.write_all(&[1, 2, 3, 4]).unwrap(); // Shard at position 10
            file.write_all(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap(); // Padding to position 20
            file.write_all(&[5, 6, 7, 8]).unwrap(); // Shard at position 20
        }

        let tape_paths = vec![("tape0", temp_file.path())];

        let mut shard_positions = HashMap::new();
        shard_positions.insert((0, 0), ("tape0".to_string(), 10));
        shard_positions.insert((0, 1), ("tape0".to_string(), 24));

        let mut reader = TapeShardReader::new(&tape_paths, shard_positions).unwrap();

        // Read shard 0
        let data = reader.read_shard(0, 0, 4).unwrap();
        assert_eq!(data, vec![1, 2, 3, 4]);

        // Read shard 1
        let data = reader.read_shard(0, 1, 4).unwrap();
        assert_eq!(data, vec![5, 6, 7, 8]);

        // Try to read non-existent shard
        assert!(reader.read_shard(1, 0, 4).is_err());
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
