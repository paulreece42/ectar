use crate::compression;
use crate::error::{EctarError, Result};
use crate::io::streaming_shard_writer::StreamingShardWriter;
use reed_solomon_erasure::galois_8::ReedSolomon;
use std::io::Write;
use std::path::PathBuf;

/// A writer that creates size-limited compressed chunks and applies erasure coding
/// in a streaming fashion, writing shards directly without intermediate chunk files
pub struct StreamingErasureChunkingWriter {
    output_base: String,
    chunk_size: u64,
    compression_level: i32,
    data_shards: usize,
    parity_shards: usize,
    current_chunk: usize,
    bytes_in_current_chunk: u64,
    // Zstd encoder that writes to an internal buffer
    current_encoder: Option<zstd::stream::write::Encoder<'static, Vec<u8>>>,
    chunks_created: Vec<ChunkInfo>,
}

#[derive(Debug, Clone)]
pub struct ChunkInfo {
    pub chunk_number: usize,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub shard_size: u64,
}

impl StreamingErasureChunkingWriter {
    pub fn new(
        output_base: PathBuf,
        chunk_size: u64,
        compression_level: i32,
        data_shards: usize,
        parity_shards: usize,
    ) -> Self {
        Self {
            output_base: output_base.to_string_lossy().to_string(),
            chunk_size,
            compression_level,
            data_shards,
            parity_shards,
            current_chunk: 0,
            bytes_in_current_chunk: 0,
            current_encoder: None,
            chunks_created: Vec::new(),
        }
    }

    /// Start a new chunk with fresh compression
    fn start_new_chunk(&mut self) -> Result<()> {
        // Finish current chunk if exists
        if self.current_encoder.is_some() {
            self.finish_current_chunk()?;
        } else {
            // First chunk - start at 1
            self.current_chunk = 1;
        }

        self.bytes_in_current_chunk = 0;

        // Create encoder that writes to a new Vec
        let buffer = Vec::new();
        let encoder = compression::create_encoder(
            buffer,
            self.compression_level,
        )?;
        self.current_encoder = Some(encoder);

        log::debug!("Started chunk {}", self.current_chunk);

        Ok(())
    }

    /// Finish the current chunk: compress, encode with erasure coding, and write shards
    fn finish_current_chunk(&mut self) -> Result<()> {
        if let Some(encoder) = self.current_encoder.take() {
            // Finish compression and get the compressed data
            let chunk_buffer = encoder.finish()?;

            let compressed_size = chunk_buffer.len() as u64;
            let uncompressed_size = self.bytes_in_current_chunk;

            if compressed_size == 0 {
                return Ok(());
            }

            log::debug!(
                "Finishing chunk {} ({} bytes compressed from {} uncompressed)",
                self.current_chunk,
                compressed_size,
                uncompressed_size
            );

            // Apply erasure coding to the compressed chunk
            let shard_size = self.encode_and_write_shards(&chunk_buffer)?;

            self.chunks_created.push(ChunkInfo {
                chunk_number: self.current_chunk,
                compressed_size,
                uncompressed_size,
                shard_size,
            });

            // Increment chunk number for next chunk
            self.current_chunk += 1;
        }

        Ok(())
    }

    /// Apply Reed-Solomon erasure coding and write shards
    fn encode_and_write_shards(&self, chunk_data: &[u8]) -> Result<u64> {
        log::debug!(
            "Encoding chunk {} ({} bytes) into {} data + {} parity shards",
            self.current_chunk,
            chunk_data.len(),
            self.data_shards,
            self.parity_shards
        );

        // Calculate shard size (round up to ensure all data fits)
        let shard_size = (chunk_data.len() + self.data_shards - 1) / self.data_shards;

        // Create Reed-Solomon encoder
        let encoder = ReedSolomon::new(self.data_shards, self.parity_shards)
            .map_err(|e| EctarError::ErasureCoding(format!("Failed to create encoder: {:?}", e)))?;

        // Create shards - initialize all to shard_size with zeros
        let mut shards: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; self.data_shards + self.parity_shards];

        // Copy chunk data into data shards
        for (i, chunk) in chunk_data.chunks(shard_size).enumerate() {
            shards[i][..chunk.len()].copy_from_slice(chunk);
            // Remaining bytes are already zero-padded
        }

        // Encode to generate parity shards
        encoder
            .encode(&mut shards)
            .map_err(|e| EctarError::ErasureCoding(format!("Encoding failed: {:?}", e)))?;

        // Write shards using StreamingShardWriter
        let mut shard_writer = StreamingShardWriter::for_chunk(
            &self.output_base,
            self.current_chunk,
            self.data_shards + self.parity_shards,
        )?;

        shard_writer.write_shards(&shards)?;
        shard_writer.finish()?;

        log::info!(
            "Chunk {}: created {} shards (shard size: {} bytes)",
            self.current_chunk,
            shards.len(),
            shard_size
        );

        Ok(shard_size as u64)
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
        if self.current_encoder.is_some() && self.bytes_in_current_chunk > 0 {
            self.finish_current_chunk()?;
        }

        log::info!(
            "Created {} chunks with erasure coding, total uncompressed: {} bytes",
            self.chunks_created.len(),
            self.chunks_created
                .iter()
                .map(|c| c.uncompressed_size)
                .sum::<u64>()
        );

        Ok(self.chunks_created)
    }
}

impl Write for StreamingErasureChunkingWriter {
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
    fn test_streaming_erasure_chunking() {
        let temp_dir = TempDir::new().unwrap();
        let output_base = temp_dir.path().join("test");

        let mut writer = StreamingErasureChunkingWriter::new(
            output_base.clone(),
            1024, // 1KB chunk size
            3,    // compression level
            4,    // data shards
            2,    // parity shards
        );

        // Write some data
        let data = vec![42u8; 2048]; // 2KB of data
        writer.write_all(&data).unwrap();
        writer.flush().unwrap();

        let chunks = writer.finish().unwrap();

        // Should create 2 chunks
        assert_eq!(chunks.len(), 2);

        // Verify shard files were created for chunk 1
        for shard_idx in 0..6 {
            let shard_path = temp_dir.path().join(format!("test.c001.s{:02}", shard_idx));
            assert!(shard_path.exists(), "Shard {} should exist", shard_idx);
        }

        // Verify shard files were created for chunk 2
        for shard_idx in 0..6 {
            let shard_path = temp_dir.path().join(format!("test.c002.s{:02}", shard_idx));
            assert!(shard_path.exists(), "Shard {} should exist", shard_idx);
        }
    }
}
