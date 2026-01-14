use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use crate::error::{EctarError, Result};
use crate::io::streaming_shard_writer::ShardOutput;
use crate::io::tape::TapeShardOutput;

/// RaitShardWriter manages writing shards across multiple tape devices
/// (RAIT - Redundant Array of Inexpensive Tapes)
pub struct RaitShardWriter {
    tape_outputs: Vec<TapeShardOutput>,
    tape_names: Vec<String>, // Device names for position tracking
    shard_positions: HashMap<(usize, usize), (String, u64)>, // (chunk_num, shard_num) -> (device_name, position)
    current_chunk: usize,
    total_shards: usize,
}

impl RaitShardWriter {
    /// Create a new RaitShardWriter with the specified tape device paths
    pub fn new(tape_paths: &[&Path], block_size: usize) -> Result<Self> {
        let mut tape_outputs = Vec::new();
        let mut tape_names = Vec::new();

        for tape_path in tape_paths {
            let output = TapeShardOutput::new(tape_path, block_size)?;
            tape_outputs.push(output);
            tape_names.push(tape_path.to_string_lossy().to_string());
        }

        Ok(Self {
            tape_outputs,
            tape_names,
            shard_positions: HashMap::new(),
            current_chunk: 0,
            total_shards: tape_paths.len(),
        })
    }

    /// Write multiple shards to different tape devices
    /// Each shard goes to a different tape drive
    pub fn write_shards(&mut self, shards: &[Vec<u8>]) -> Result<Vec<u64>> {
        if shards.len() != self.total_shards {
            return Err(EctarError::InvalidParameters(format!(
                "Expected {} shards, got {}",
                self.total_shards,
                shards.len()
            )));
        }

        let mut shard_sizes = Vec::new();

        for (shard_num, shard_data) in shards.iter().enumerate() {
            // Each shard goes to a different tape drive
            let tape_index = shard_num % self.tape_outputs.len();
            let tape_output = &mut self.tape_outputs[tape_index];
            let tape_name = self.tape_names[tape_index].clone();

            // Record the starting position of this shard with device name
            let start_position = tape_output.current_position();
            self.shard_positions
                .insert((self.current_chunk, shard_num), (tape_name, start_position));

            // Write the shard data
            tape_output.write_all(shard_data)?;
            tape_output.flush()?;

            let shard_size = shard_data.len() as u64;
            shard_sizes.push(shard_size);
        }

        self.current_chunk += 1;
        Ok(shard_sizes)
    }

    /// Get the position of a specific shard on its tape (device_name, position)
    pub fn get_shard_position(&self, chunk_num: usize, shard_num: usize) -> Option<&(String, u64)> {
        self.shard_positions.get(&(chunk_num, shard_num))
    }

    /// Get the number of tape devices
    pub fn num_tapes(&self) -> usize {
        self.tape_outputs.len()
    }

    /// Get the total number of shards per chunk
    pub fn total_shards(&self) -> usize {
        self.total_shards
    }

    /// Finish writing and return final positions with device names
    pub fn finish(mut self) -> Result<HashMap<(usize, usize), (String, u64)>> {
        // Ensure all tape outputs are finished
        for tape_output in &mut self.tape_outputs {
            tape_output.finish()?;
        }

        Ok(self.shard_positions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

        // Check positions are recorded (now returns (device_name, position))
        let pos0 = writer.get_shard_position(0, 0).unwrap();
        let pos1 = writer.get_shard_position(0, 1).unwrap();
        assert_eq!(pos0.1, 0);
        assert_eq!(pos1.1, 0);

        // Write second set of shards
        let shards2 = vec![vec![13, 14, 15, 16], vec![17, 18, 19, 20]];
        writer.write_shards(&shards2).unwrap();

        // Check positions for second chunk
        let pos2 = writer.get_shard_position(1, 0).unwrap();
        let pos3 = writer.get_shard_position(1, 1).unwrap();
        assert_eq!(pos2.1, 4); // After first 4-byte block
        assert_eq!(pos3.1, 4);
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

    #[test]
    fn test_rait_writer_finish() {
        let temp_files: Vec<NamedTempFile> =
            (0..2).map(|_| NamedTempFile::new().unwrap()).collect();
        let tape_paths: Vec<&std::path::Path> = temp_files.iter().map(|f| f.path()).collect();

        let mut writer = RaitShardWriter::new(&tape_paths, 4).unwrap();

        let shards = vec![vec![1, 2, 3, 4], vec![5, 6, 7, 8]];
        writer.write_shards(&shards).unwrap();

        let positions = writer.finish().unwrap();

        // Should return all recorded positions with device names
        assert_eq!(positions.len(), 2);
        let pos0 = positions.get(&(0, 0)).unwrap();
        let pos1 = positions.get(&(0, 1)).unwrap();
        assert_eq!(pos0.1, 0);
        assert_eq!(pos1.1, 0);
        // Device names should be the temp file paths
        assert!(!pos0.0.is_empty());
        assert!(!pos1.0.is_empty());
    }
}
