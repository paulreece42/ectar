use crate::error::{EctarError, Result};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

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
}

impl StreamingShardWriter {
    /// Create a new streaming shard writer
    pub fn new() -> Self {
        Self {
            outputs: Vec::new(),
            current_chunk: 0,
        }
    }

    /// Create with file-based outputs for a given chunk
    pub fn for_chunk(
        output_base: &str,
        chunk_number: usize,
        num_shards: usize,
    ) -> Result<Self> {
        let mut outputs: Vec<Box<dyn ShardOutput>> = Vec::new();

        for shard_idx in 0..num_shards {
            let shard_path = format_shard_path(output_base, chunk_number, shard_idx);
            let output = FileShardOutput::new(shard_path)?;
            outputs.push(Box::new(output));
        }

        Ok(Self {
            outputs,
            current_chunk: chunk_number,
        })
    }

    /// Write shards in parallel (all shards from the same chunk)
    /// Each shard is written to its corresponding output
    pub fn write_shards(&mut self, shards: &[Vec<u8>]) -> Result<Vec<u64>> {
        if shards.len() != self.outputs.len() {
            return Err(EctarError::InvalidParameters(format!(
                "Shard count mismatch: expected {}, got {}",
                self.outputs.len(),
                shards.len()
            )));
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
            if !shard_sizes.is_empty() { shard_sizes[0] } else { 0 }
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
        let shards = vec![
            vec![1u8; 100],
            vec![2u8; 100],
            vec![3u8; 100],
        ];

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
}
