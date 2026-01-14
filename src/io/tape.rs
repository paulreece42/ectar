use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use crate::error::Result;
use crate::io::streaming_shard_writer::ShardOutput;

/// TapeShardOutput implements ShardOutput for tape devices
/// It handles block-aligned writing and buffering for tape I/O
pub struct TapeShardOutput {
    tape_device: File,
    current_position: u64,
    bytes_written: u64,
    block_size: usize,
    write_buffer: Vec<u8>,
}

impl TapeShardOutput {
    /// Create a new TapeShardOutput for the given tape device path
    pub fn new(device_path: &Path, block_size: usize) -> Result<Self> {
        use std::io::{Seek, SeekFrom};

        // Open in append mode to preserve data from previous chunks
        let mut tape_device = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .append(true)
            .create(true)
            .open(device_path)?;

        // Get current position (end of file due to append mode)
        let current_position = tape_device.seek(SeekFrom::End(0))?;

        Ok(Self {
            tape_device,
            current_position,
            bytes_written: 0,
            block_size,
            write_buffer: Vec::new(),
        })
    }

    /// Get the current position on the tape
    pub fn current_position(&self) -> u64 {
        self.current_position
    }

    /// Get the block size used for tape I/O
    pub fn block_size(&self) -> usize {
        self.block_size
    }
}

impl Write for TapeShardOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Buffer data until we have complete tape blocks
        self.write_buffer.extend_from_slice(buf);

        let mut bytes_processed = 0;
        while self.write_buffer.len() >= self.block_size {
            // Write complete blocks to tape
            let block = &self.write_buffer[..self.block_size];
            self.tape_device.write_all(block)?;

            self.write_buffer.drain(..self.block_size);
            bytes_processed += self.block_size;
            self.current_position += self.block_size as u64;
            self.bytes_written += self.block_size as u64;
        }

        Ok(buf.len()) // Return the number of bytes accepted (buffered)
    }

    fn flush(&mut self) -> io::Result<()> {
        // Write remaining partial block with padding if needed
        if !self.write_buffer.is_empty() {
            // Pad to block boundary with zeros
            let padding_needed = self.block_size - self.write_buffer.len();
            self.write_buffer.resize(self.block_size, 0);

            self.tape_device.write_all(&self.write_buffer)?;
            self.tape_device.flush()?;

            self.current_position += self.block_size as u64;
            self.bytes_written += self.block_size as u64;
            self.write_buffer.clear();
        } else {
            self.tape_device.flush()?;
        }

        Ok(())
    }
}

impl ShardOutput for TapeShardOutput {
    fn finish(&mut self) -> Result<u64> {
        // Flush any remaining buffered data
        self.flush()?;
        Ok(self.bytes_written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::NamedTempFile;

    #[test]
    fn test_tape_shard_output_creation() {
        // Create a temporary file to simulate a tape device
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path();

        let output = TapeShardOutput::new(temp_path, 512).unwrap();
        assert_eq!(output.block_size, 512);
        assert_eq!(output.current_position(), 0);
        assert_eq!(output.bytes_written, 0);
    }

    #[test]
    fn test_block_aligned_writing() {
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path();
        let mut output = TapeShardOutput::new(temp_path, 4).unwrap(); // Small block size for testing

        // Write partial block - should be buffered
        assert_eq!(output.write(&[1, 2]).unwrap(), 2);
        assert_eq!(output.current_position(), 0); // Position doesn't advance until block is written
        assert_eq!(output.bytes_written, 0);

        // Write enough to complete the block
        assert_eq!(output.write(&[3, 4]).unwrap(), 2);
        assert_eq!(output.current_position(), 4); // Now position advances
        assert_eq!(output.bytes_written, 4);

        // Check data was written correctly
        let mut file = std::fs::File::open(temp_path).unwrap();
        let mut buffer = [0; 4];
        file.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, &[1, 2, 3, 4]);
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
        assert_eq!(output.bytes_written, 4);

        // Check data was written with padding
        let mut file = std::fs::File::open(temp_path).unwrap();
        let mut buffer = [0; 4];
        file.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, &[1, 0, 0, 0]); // Padded with zeros
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
