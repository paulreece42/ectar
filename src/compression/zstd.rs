use crate::error::{EctarError, Result};
use std::io::{Read, Write};
use zstd::stream::{read::Decoder, write::Encoder};

/// Zstd compression level (1-22)
pub const DEFAULT_COMPRESSION_LEVEL: i32 = 3;
pub const MAX_COMPRESSION_LEVEL: i32 = 22;
pub const MIN_COMPRESSION_LEVEL: i32 = 1;

/// Compress data from reader to writer using zstd
pub fn compress<R: Read, W: Write>(reader: R, writer: W, level: i32) -> Result<u64> {
    let level = validate_compression_level(level)?;

    let mut encoder = Encoder::new(writer, level)
        .map_err(|e| EctarError::Compression(format!("Failed to create encoder: {}", e)))?;

    encoder
        .multithread(num_cpus::get() as u32)
        .map_err(|e| EctarError::Compression(format!("Failed to enable multithreading: {}", e)))?;

    let bytes_written = std::io::copy(&mut std::io::BufReader::new(reader), &mut encoder)
        .map_err(|e| EctarError::Compression(format!("Compression failed: {}", e)))?;

    encoder
        .finish()
        .map_err(|e| EctarError::Compression(format!("Failed to finish compression: {}", e)))?;

    Ok(bytes_written)
}

/// Decompress data from reader to writer using zstd
pub fn decompress<R: Read, W: Write>(reader: R, writer: W) -> Result<u64> {
    let mut decoder = Decoder::new(reader)
        .map_err(|e| EctarError::Decompression(format!("Failed to create decoder: {}", e)))?;

    let bytes_written = std::io::copy(&mut decoder, &mut std::io::BufWriter::new(writer))
        .map_err(|e| EctarError::Decompression(format!("Decompression failed: {}", e)))?;

    Ok(bytes_written)
}

/// Validate compression level is within acceptable range
pub fn validate_compression_level(level: i32) -> Result<i32> {
    if level < MIN_COMPRESSION_LEVEL || level > MAX_COMPRESSION_LEVEL {
        return Err(EctarError::InvalidParameters(format!(
            "Compression level must be between {} and {}, got {}",
            MIN_COMPRESSION_LEVEL, MAX_COMPRESSION_LEVEL, level
        )));
    }
    Ok(level)
}

/// Create a zstd encoder that can be written to
pub fn create_encoder<W: Write>(writer: W, level: i32) -> Result<Encoder<'static, W>> {
    let level = validate_compression_level(level)?;

    let mut encoder = Encoder::new(writer, level)
        .map_err(|e| EctarError::Compression(format!("Failed to create encoder: {}", e)))?;

    encoder
        .multithread(num_cpus::get() as u32)
        .map_err(|e| EctarError::Compression(format!("Failed to enable multithreading: {}", e)))?;

    Ok(encoder)
}

/// Create a zstd decoder that can be read from
pub fn create_decoder<R: Read>(reader: R) -> Result<Decoder<'static, std::io::BufReader<R>>> {
    Decoder::new(reader)
        .map_err(|e| EctarError::Decompression(format!("Failed to create decoder: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_compress_decompress() {
        let data = b"Hello, World! This is a test of zstd compression.".repeat(100);
        let mut compressed = Vec::new();

        // Compress
        compress(Cursor::new(&data), &mut compressed, 3).unwrap();

        // Verify compression actually reduced size
        assert!(compressed.len() < data.len());

        // Decompress
        let mut decompressed = Vec::new();
        decompress(Cursor::new(&compressed), &mut decompressed).unwrap();

        // Verify data matches
        assert_eq!(data, decompressed);
    }

    #[test]
    fn test_compression_levels() {
        let data = b"Test data for compression level testing".repeat(50);

        // Test various compression levels
        for level in [1, 3, 10, 19, 22] {
            let mut compressed = Vec::new();
            compress(Cursor::new(&data), &mut compressed, level).unwrap();

            let mut decompressed = Vec::new();
            decompress(Cursor::new(&compressed), &mut decompressed).unwrap();

            assert_eq!(data, decompressed);
        }
    }

    #[test]
    fn test_invalid_compression_level() {
        assert!(validate_compression_level(0).is_err());
        assert!(validate_compression_level(23).is_err());
        assert!(validate_compression_level(-1).is_err());
        assert!(validate_compression_level(100).is_err());
    }
}
