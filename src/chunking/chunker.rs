use crate::error::Result;
use std::io::{self, Write};

/// A writer that splits output into size-limited chunks
pub struct ChunkingWriter<W: Write> {
    chunk_size: u64,
    current_chunk: usize,
    bytes_in_chunk: u64,
    writer_factory: Box<dyn Fn(usize) -> Result<W>>,
    current_writer: Option<W>,
    chunks_created: Vec<ChunkMetadata>,
}

#[derive(Debug, Clone)]
pub struct ChunkMetadata {
    pub chunk_number: usize,
    pub size: u64,
}

impl<W: Write> ChunkingWriter<W> {
    /// Create a new chunking writer
    ///
    /// # Arguments
    /// * `chunk_size` - Maximum size of each chunk in bytes
    /// * `writer_factory` - Function that creates a new writer for each chunk
    pub fn new<F>(chunk_size: u64, writer_factory: F) -> Self
    where
        F: Fn(usize) -> Result<W> + 'static,
    {
        Self {
            chunk_size,
            current_chunk: 0,
            bytes_in_chunk: 0,
            writer_factory: Box::new(writer_factory),
            current_writer: None,
            chunks_created: Vec::new(),
        }
    }

    /// Start a new chunk
    fn start_new_chunk(&mut self) -> Result<()> {
        // Finish current chunk if exists
        if let Some(writer) = self.current_writer.take() {
            drop(writer); // Ensure writer is flushed and closed
            self.chunks_created.push(ChunkMetadata {
                chunk_number: self.current_chunk,
                size: self.bytes_in_chunk,
            });
        }

        // Start new chunk
        self.current_chunk += 1;
        self.bytes_in_chunk = 0;
        let writer = (self.writer_factory)(self.current_chunk)?;
        self.current_writer = Some(writer);

        Ok(())
    }

    /// Get metadata for all created chunks
    pub fn chunks(&self) -> &[ChunkMetadata] {
        &self.chunks_created
    }

    /// Get the current chunk number
    pub fn current_chunk_number(&self) -> usize {
        self.current_chunk
    }

    /// Finish writing and return chunk metadata
    pub fn finish(mut self) -> Result<Vec<ChunkMetadata>> {
        // Finish the last chunk
        if let Some(writer) = self.current_writer.take() {
            drop(writer);
            if self.bytes_in_chunk > 0 {
                self.chunks_created.push(ChunkMetadata {
                    chunk_number: self.current_chunk,
                    size: self.bytes_in_chunk,
                });
            }
        }

        Ok(self.chunks_created)
    }
}

impl<W: Write> Write for ChunkingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        // Start first chunk if needed
        if self.current_writer.is_none() {
            self.start_new_chunk()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        }

        let mut bytes_written = 0;

        while bytes_written < buf.len() {
            let remaining_in_chunk = self.chunk_size - self.bytes_in_chunk;
            let remaining_in_buf = buf.len() - bytes_written;
            let to_write = std::cmp::min(remaining_in_chunk as usize, remaining_in_buf);

            if to_write == 0 {
                // Current chunk is full, start a new one
                self.start_new_chunk()
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
                continue;
            }

            // Write to current chunk
            let writer = self.current_writer.as_mut().unwrap();
            let n = writer.write(&buf[bytes_written..bytes_written + to_write])?;

            bytes_written += n;
            self.bytes_in_chunk += n as u64;
        }

        Ok(bytes_written)
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(writer) = &mut self.current_writer {
            writer.flush()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_chunking_writer() {
        let mut buffers: Vec<Vec<u8>> = Vec::new();

        {
            let mut chunker = ChunkingWriter::new(100, |chunk_num| {
                Ok(Cursor::new(Vec::new()))
            });

            // Write 250 bytes (should create 3 chunks: 100, 100, 50)
            let data = vec![0u8; 250];
            chunker.write_all(&data).unwrap();

            let chunks = chunker.finish().unwrap();
            assert_eq!(chunks.len(), 3);
            assert_eq!(chunks[0].size, 100);
            assert_eq!(chunks[1].size, 100);
            assert_eq!(chunks[2].size, 50);
        }
    }

    #[test]
    fn test_single_chunk() {
        let mut chunker = ChunkingWriter::new(1000, |_| Ok(Cursor::new(Vec::new())));

        chunker.write_all(b"Hello, World!").unwrap();

        let chunks = chunker.finish().unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].size, 13);
    }
}
