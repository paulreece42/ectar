use crate::erasure::ZfecHeader;
use crate::error::{EctarError, Result};
use crate::io::tape::TapeShardOutput;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Trait for shard output destinations (files, tape drives, network, etc.)
pub trait ShardOutput: Write + Send {
    fn finish(&mut self) -> Result<u64>;
}

/// File-based shard output
pub struct FileShardOutput {
    file: File,
    bytes_written: u64,
}

impl FileShardOutput {
    pub fn new(path: PathBuf) -> Result<Self> {
        let file = File::create(&path)?;
        log::debug!("Created shard output file: {}", path.display());
        Ok(Self {
            file,
            bytes_written: 0,
        })
    }
}

impl Write for FileShardOutput {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.file.write(buf)?;
        self.bytes_written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

impl ShardOutput for FileShardOutput {
    fn finish(&mut self) -> Result<u64> {
        self.file.flush()?;
        Ok(self.bytes_written)
    }
}

/// Manages multiple shard outputs for parallel writing
pub struct StreamingShardWriter {
    outputs: Vec<Box<dyn ShardOutput>>,
    current_chunk: usize,
    /// Erasure coding parameters (k, m) if zfec headers should be written
    ec_params: Option<(u8, u8)>,
    /// Padding length for zfec headers
    padlen: usize,
    /// Whether headers have been written for this chunk
    headers_written: bool,
    /// Whether this is a tape-based writer
    is_tape_mode: bool,
    /// Starting positions for each shard on tape (shard_num, device_index, start_position)
    tape_shard_positions: Vec<(usize, usize, u64)>,
}

impl StreamingShardWriter {
    /// Create a new streaming shard writer
    pub fn new() -> Self {
        Self {
            outputs: Vec::new(),
            current_chunk: 0,
            ec_params: None,
            padlen: 0,
            headers_written: false,
            is_tape_mode: false,
            tape_shard_positions: Vec::new(),
        }
    }

    /// Create with file-based outputs for a given chunk
    pub fn for_chunk(output_base: &str, chunk_number: usize, num_shards: usize) -> Result<Self> {
        let mut outputs: Vec<Box<dyn ShardOutput>> = Vec::new();

        for shard_idx in 0..num_shards {
            let shard_path = format_shard_path(output_base, chunk_number, shard_idx);
            let output = FileShardOutput::new(shard_path)?;
            outputs.push(Box::new(output));
        }

        Ok(Self {
            outputs,
            current_chunk: chunk_number,
            ec_params: None,
            padlen: 0,
            headers_written: false,
            is_tape_mode: false,
            tape_shard_positions: Vec::new(),
        })
    }

    /// Create with file-based outputs and zfec headers enabled
    pub fn for_chunk_with_headers(
        output_base: &str,
        chunk_number: usize,
        data_shards: u8,
        total_shards: u8,
        padlen: usize,
    ) -> Result<Self> {
        let mut outputs: Vec<Box<dyn ShardOutput>> = Vec::new();

        for shard_idx in 0..total_shards as usize {
            let shard_path = format_shard_path(output_base, chunk_number, shard_idx);
            let output = FileShardOutput::new(shard_path)?;
            outputs.push(Box::new(output));
        }

        Ok(Self {
            outputs,
            current_chunk: chunk_number,
            ec_params: Some((data_shards, total_shards)),
            padlen,
            headers_written: false,
            is_tape_mode: false,
            tape_shard_positions: Vec::new(),
        })
    }

    /// Create with tape-based outputs and zfec headers enabled
    /// Each shard is written to a different tape device
    pub fn for_tape_devices_with_headers(
        tape_devices: &[&Path],
        chunk_number: usize,
        data_shards: u8,
        total_shards: u8,
        padlen: usize,
        block_size: usize,
    ) -> Result<Self> {
        if tape_devices.len() != total_shards as usize {
            return Err(EctarError::InvalidParameters(format!(
                "Number of tape devices ({}) must equal total shards ({})",
                tape_devices.len(),
                total_shards
            )));
        }

        let mut outputs: Vec<Box<dyn ShardOutput>> = Vec::new();
        let mut tape_shard_positions = Vec::new();

        for (device_index, tape_path) in tape_devices.iter().enumerate() {
            let output = TapeShardOutput::new(tape_path, block_size)?;
            // Record starting position for this shard
            let start_position = output.current_position();
            tape_shard_positions.push((device_index, device_index, start_position));
            outputs.push(Box::new(output));
        }

        log::info!(
            "Created tape shard writer for chunk {} with {} devices (block size: {})",
            chunk_number,
            tape_devices.len(),
            block_size
        );

        Ok(Self {
            outputs,
            current_chunk: chunk_number,
            ec_params: Some((data_shards, total_shards)),
            padlen,
            headers_written: false,
            is_tape_mode: true,
            tape_shard_positions,
        })
    }

    /// Get tape shard positions (only valid for tape mode)
    /// Returns (shard_num, device_index, byte_position) for each shard
    pub fn get_tape_shard_positions(&self) -> Option<Vec<(usize, usize, u64)>> {
        if self.is_tape_mode {
            Some(self.tape_shard_positions.clone())
        } else {
            None
        }
    }

    /// Write shards in parallel (all shards from the same chunk)
    /// Each shard is written to its corresponding output
    /// If ec_params is set, writes zfec headers before shard data (once)
    pub fn write_shards(&mut self, shards: &[Vec<u8>]) -> Result<Vec<u64>> {
        if shards.len() != self.outputs.len() {
            return Err(EctarError::InvalidParameters(format!(
                "Shard count mismatch: expected {}, got {}",
                self.outputs.len(),
                shards.len()
            )));
        }

        // Write zfec headers if configured and not yet written
        if let Some((k, m)) = self.ec_params {
            if !self.headers_written {
                for (shard_idx, output) in self.outputs.iter_mut().enumerate() {
                    let header = ZfecHeader::new(k, m, shard_idx as u8, self.padlen)?;
                    let header_bytes = header.encode();
                    output.write_all(&header_bytes)?;
                    log::debug!(
                        "Wrote zfec header for shard {}: k={}, m={}, padlen={}",
                        shard_idx,
                        k,
                        m,
                        self.padlen
                    );
                }
                self.headers_written = true;
            }
        }

        let mut shard_sizes = Vec::new();

        // Write each shard to its output
        for (shard_data, output) in shards.iter().zip(self.outputs.iter_mut()) {
            output.write_all(shard_data)?;
            output.flush()?;
            shard_sizes.push(shard_data.len() as u64);
        }

        log::debug!(
            "Wrote {} shards for chunk {} ({} bytes each)",
            shards.len(),
            self.current_chunk,
            if !shard_sizes.is_empty() {
                shard_sizes[0]
            } else {
                0
            }
        );

        Ok(shard_sizes)
    }

    /// Finish writing and return bytes written per shard
    pub fn finish(mut self) -> Result<Vec<u64>> {
        let mut sizes = Vec::new();
        for output in self.outputs.iter_mut() {
            let size = output.finish()?;
            sizes.push(size);
        }
        Ok(sizes)
    }

    /// Get the number of outputs
    pub fn num_outputs(&self) -> usize {
        self.outputs.len()
    }
}

/// Format a shard file path
fn format_shard_path(output_base: &str, chunk_number: usize, shard_number: usize) -> PathBuf {
    PathBuf::from(format!(
        "{}.c{:03}.s{:02}",
        output_base, chunk_number, shard_number
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_streaming_shard_writer() {
        let temp_dir = TempDir::new().unwrap();
        let output_base = temp_dir.path().join("test").to_string_lossy().to_string();

        let mut writer = StreamingShardWriter::for_chunk(&output_base, 1, 3).unwrap();

        // Create test shards
        let shards = vec![vec![1u8; 100], vec![2u8; 100], vec![3u8; 100]];

        let sizes = writer.write_shards(&shards).unwrap();
        assert_eq!(sizes.len(), 3);
        assert_eq!(sizes[0], 100);

        let final_sizes = writer.finish().unwrap();
        assert_eq!(final_sizes.len(), 3);
        assert_eq!(final_sizes[0], 100);

        // Verify files were created
        assert!(format_shard_path(&output_base, 1, 0).exists());
        assert!(format_shard_path(&output_base, 1, 1).exists());
        assert!(format_shard_path(&output_base, 1, 2).exists());
    }

    #[test]
    fn test_streaming_shard_writer_new() {
        let writer = StreamingShardWriter::new();
        assert_eq!(writer.num_outputs(), 0);
    }

    #[test]
    fn test_write_shards_count_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let output_base = temp_dir.path().join("test").to_string_lossy().to_string();

        let mut writer = StreamingShardWriter::for_chunk(&output_base, 1, 3).unwrap();

        // Try to write wrong number of shards
        let shards = vec![
            vec![1u8; 100],
            vec![2u8; 100],
            // Missing third shard
        ];

        let result = writer.write_shards(&shards);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_shard_output() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test_shard.bin");

        let mut output = FileShardOutput::new(path.clone()).unwrap();

        // Write some data
        use std::io::Write;
        output.write_all(b"test data").unwrap();
        output.flush().unwrap();

        let bytes = output.finish().unwrap();
        assert_eq!(bytes, 9);

        // Verify file content
        let content = std::fs::read(&path).unwrap();
        assert_eq!(content, b"test data");
    }

    #[test]
    fn test_format_shard_path() {
        let path = format_shard_path("archive", 1, 5);
        assert_eq!(path, PathBuf::from("archive.c001.s05"));

        let path = format_shard_path("/path/to/backup", 42, 10);
        assert_eq!(path, PathBuf::from("/path/to/backup.c042.s10"));
    }

    #[test]
    fn test_num_outputs() {
        let temp_dir = TempDir::new().unwrap();
        let output_base = temp_dir.path().join("test").to_string_lossy().to_string();

        let writer = StreamingShardWriter::for_chunk(&output_base, 1, 5).unwrap();
        assert_eq!(writer.num_outputs(), 5);
    }

    #[test]
    fn test_write_with_zfec_headers() {
        let temp_dir = TempDir::new().unwrap();
        let output_base = temp_dir.path().join("test").to_string_lossy().to_string();

        // Create writer with zfec headers enabled (k=3, m=5, padlen=0)
        let mut writer = StreamingShardWriter::for_chunk_with_headers(
            &output_base,
            1,
            3, // data_shards
            5, // total_shards
            0, // padlen
        )
        .unwrap();

        // Create test shards
        let shards = vec![
            vec![1u8; 100],
            vec![2u8; 100],
            vec![3u8; 100],
            vec![4u8; 100],
            vec![5u8; 100],
        ];

        let _sizes = writer.write_shards(&shards).unwrap();
        let _final_sizes = writer.finish().unwrap();

        // Verify files were created and contain headers
        for shard_idx in 0..5 {
            let shard_path = format_shard_path(&output_base, 1, shard_idx);
            assert!(shard_path.exists());

            // Read the file and verify header + data
            let content = std::fs::read(&shard_path).unwrap();

            // Try different header sizes (2-4 bytes) to find the correct one
            let mut header = None;
            let mut actual_header_size = 0;

            for size in 2..=4 {
                if content.len() >= size {
                    if let Ok(h) = ZfecHeader::decode(&content[..size]) {
                        // Verify parameters match what we expect
                        if h.k == 3 && h.m == 5 && h.sharenum == shard_idx as u8 && h.padlen == 0 {
                            header = Some(h);
                            actual_header_size = size;
                            break;
                        }
                    }
                }
            }

            assert!(
                header.is_some(),
                "Failed to decode header for shard {}",
                shard_idx
            );
            let header = header.unwrap();

            // Verify header fields
            assert_eq!(header.k, 3);
            assert_eq!(header.m, 5);
            assert_eq!(header.sharenum, shard_idx as u8);
            assert_eq!(header.padlen, 0);

            // Verify shard data follows header
            assert_eq!(content.len(), actual_header_size + 100);
        }
    }
}
