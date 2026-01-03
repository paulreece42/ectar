use crate::compression;
use crate::error::Result;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

/// A writer that creates size-limited compressed chunks
/// Each chunk has independent compression for recovery purposes
pub struct CompressedChunkingWriter {
    output_base: PathBuf,
    chunk_size: u64,
    compression_level: i32,
    current_chunk: usize,
    bytes_in_current_chunk: u64,
    current_encoder: Option<zstd::stream::write::Encoder<'static, File>>,
    chunks_created: Vec<ChunkInfo>,
}

#[derive(Debug, Clone)]
pub struct ChunkInfo {
    pub chunk_number: usize,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
}

impl CompressedChunkingWriter {
    pub fn new(output_base: PathBuf, chunk_size: u64, compression_level: i32) -> Self {
        Self {
            output_base,
            chunk_size,
            compression_level,
            current_chunk: 0,
            bytes_in_current_chunk: 0,
            current_encoder: None,
            chunks_created: Vec::new(),
        }
    }

    /// Start a new chunk with fresh compression
    fn start_new_chunk(&mut self) -> Result<()> {
        // Finish current chunk if exists
        if let Some(encoder) = self.current_encoder.take() {
            let file = encoder.finish()?;
            let compressed_size = file.metadata()?.len();

            self.chunks_created.push(ChunkInfo {
                chunk_number: self.current_chunk,
                compressed_size,
                uncompressed_size: self.bytes_in_current_chunk,
            });

            // Only increment chunk number after finishing a chunk
            self.current_chunk += 1;
        } else {
            // First chunk - start at 1
            self.current_chunk = 1;
        }

        self.bytes_in_current_chunk = 0;

        let chunk_path = self.get_chunk_path(self.current_chunk);
        let file = File::create(&chunk_path)?;
        let encoder = compression::create_encoder(file, self.compression_level)?;
        self.current_encoder = Some(encoder);

        log::debug!(
            "Started chunk {} at {}",
            self.current_chunk,
            chunk_path.display()
        );

        Ok(())
    }

    /// Get the file path for a chunk number
    fn get_chunk_path(&self, chunk_number: usize) -> PathBuf {
        let mut path = self.output_base.clone();
        let filename = format!(
            "{}.c{:03}.tar.zst",
            path.file_name().unwrap().to_string_lossy(),
            chunk_number
        );
        path.set_file_name(filename);
        path
    }

    /// Get metadata for all created chunks
    pub fn chunks(&self) -> &[ChunkInfo] {
        &self.chunks_created
    }

    /// Get the current chunk number
    pub fn current_chunk_number(&self) -> usize {
        // If no chunk has been started yet, return 1 (first chunk)
        if self.current_chunk == 0 {
            1
        } else {
            self.current_chunk
        }
    }

    /// Finish writing and return chunk metadata
    pub fn finish(mut self) -> Result<Vec<ChunkInfo>> {
        // Finish the last chunk
        if let Some(encoder) = self.current_encoder.take() {
            let file = encoder.finish()?;
            let compressed_size = file.metadata()?.len();

            if self.bytes_in_current_chunk > 0 {
                self.chunks_created.push(ChunkInfo {
                    chunk_number: self.current_chunk,
                    compressed_size,
                    uncompressed_size: self.bytes_in_current_chunk,
                });
            }
        }

        log::info!(
            "Created {} chunks, total uncompressed: {} bytes",
            self.chunks_created.len(),
            self.chunks_created
                .iter()
                .map(|c| c.uncompressed_size)
                .sum::<u64>()
        );

        Ok(self.chunks_created)
    }
}

impl Write for CompressedChunkingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        // Start first chunk if needed
        if self.current_encoder.is_none() {
            self.start_new_chunk()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        }

        let mut bytes_written = 0;

        while bytes_written < buf.len() {
            let remaining_in_chunk = self.chunk_size - self.bytes_in_current_chunk;
            let remaining_in_buf = buf.len() - bytes_written;
            let to_write = std::cmp::min(remaining_in_chunk as usize, remaining_in_buf);

            if to_write == 0 {
                // Current chunk is full, start a new one
                self.start_new_chunk()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
                continue;
            }

            // Write to current chunk's encoder
            let encoder = self.current_encoder.as_mut().unwrap();
            let n = encoder.write(&buf[bytes_written..bytes_written + to_write])?;

            bytes_written += n;
            self.bytes_in_current_chunk += n as u64;
        }

        Ok(bytes_written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(encoder) = &mut self.current_encoder {
            encoder.flush()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_single_chunk() {
        let temp_dir = TempDir::new().unwrap();
        let output_base = temp_dir.path().join("test");

        let mut writer = CompressedChunkingWriter::new(
            output_base.clone(),
            1024, // 1KB chunks
            3,
        );

        // Write less than chunk size
        let data = vec![42u8; 500];
        writer.write_all(&data).unwrap();
        writer.flush().unwrap();

        let chunks = writer.finish().unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_number, 1);
        assert_eq!(chunks[0].uncompressed_size, 500);

        // Verify chunk file exists
        let chunk_path = temp_dir.path().join("test.c001.tar.zst");
        assert!(chunk_path.exists());
    }

    #[test]
    fn test_multiple_chunks() {
        let temp_dir = TempDir::new().unwrap();
        let output_base = temp_dir.path().join("test");

        let mut writer = CompressedChunkingWriter::new(
            output_base.clone(),
            1024, // 1KB chunks
            3,
        );

        // Write 2.5KB of data
        let data = vec![42u8; 2560];
        writer.write_all(&data).unwrap();
        writer.flush().unwrap();

        let chunks = writer.finish().unwrap();

        // Should create 3 chunks
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].uncompressed_size, 1024);
        assert_eq!(chunks[1].uncompressed_size, 1024);
        assert_eq!(chunks[2].uncompressed_size, 512);

        // Verify chunk files exist
        assert!(temp_dir.path().join("test.c001.tar.zst").exists());
        assert!(temp_dir.path().join("test.c002.tar.zst").exists());
        assert!(temp_dir.path().join("test.c003.tar.zst").exists());
    }

    #[test]
    fn test_exact_chunk_boundary() {
        let temp_dir = TempDir::new().unwrap();
        let output_base = temp_dir.path().join("test");

        let mut writer = CompressedChunkingWriter::new(
            output_base,
            1024,
            3,
        );

        // Write exactly 2KB
        let data = vec![99u8; 2048];
        writer.write_all(&data).unwrap();

        let chunks = writer.finish().unwrap();

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].uncompressed_size, 1024);
        assert_eq!(chunks[1].uncompressed_size, 1024);
    }

    #[test]
    fn test_empty_write() {
        let temp_dir = TempDir::new().unwrap();
        let output_base = temp_dir.path().join("test");

        let mut writer = CompressedChunkingWriter::new(
            output_base,
            1024,
            3,
        );

        let chunks = writer.finish().unwrap();
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_current_chunk_number() {
        let temp_dir = TempDir::new().unwrap();
        let output_base = temp_dir.path().join("test");

        let mut writer = CompressedChunkingWriter::new(
            output_base,
            100,
            3,
        );

        // Before writing, should be 1
        assert_eq!(writer.current_chunk_number(), 1);

        // Write some data
        writer.write_all(&[1u8; 50]).unwrap();
        assert_eq!(writer.current_chunk_number(), 1);

        // Write more to trigger new chunk
        writer.write_all(&[2u8; 100]).unwrap();
        assert_eq!(writer.current_chunk_number(), 2);
    }

    #[test]
    fn test_compression_reduces_size() {
        let temp_dir = TempDir::new().unwrap();
        let output_base = temp_dir.path().join("test");

        let mut writer = CompressedChunkingWriter::new(
            output_base,
            10000,
            3,
        );

        // Write highly compressible data
        let data = vec![0u8; 5000];
        writer.write_all(&data).unwrap();

        let chunks = writer.finish().unwrap();

        assert_eq!(chunks.len(), 1);
        // Compressed size should be significantly smaller than uncompressed
        assert!(chunks[0].compressed_size < chunks[0].uncompressed_size);
    }
}
