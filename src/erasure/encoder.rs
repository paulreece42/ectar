use crate::error::{EctarError, Result};
use reed_solomon_erasure::galois_8::ReedSolomon;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

/// Encode a chunk file into k+m shards using Reed-Solomon erasure coding
pub fn encode_chunk(
    chunk_path: &PathBuf,
    output_base: &str,
    chunk_number: usize,
    data_shards: usize,
    parity_shards: usize,
) -> Result<Vec<ShardInfo>> {
    // Validate parameters
    if data_shards < 1 {
        return Err(EctarError::InvalidParameters(
            "Data shards must be at least 1".to_string(),
        ));
    }
    if parity_shards < 1 {
        return Err(EctarError::InvalidParameters(
            "Parity shards must be at least 1".to_string(),
        ));
    }
    if data_shards + parity_shards > 256 {
        return Err(EctarError::InvalidParameters(
            "Total shards cannot exceed 256".to_string(),
        ));
    }

    // Read the chunk file
    let mut chunk_file = File::open(chunk_path)?;
    let mut chunk_data = Vec::new();
    chunk_file.read_to_end(&mut chunk_data)?;

    log::debug!(
        "Encoding chunk {} ({} bytes) into {} data + {} parity shards",
        chunk_number,
        chunk_data.len(),
        data_shards,
        parity_shards
    );

    // Calculate shard size (round up to ensure all data fits)
    let shard_size = (chunk_data.len() + data_shards - 1) / data_shards;

    // Create Reed-Solomon encoder
    let encoder = ReedSolomon::new(data_shards, parity_shards)
        .map_err(|e| EctarError::ErasureCoding(format!("Failed to create encoder: {:?}", e)))?;

    // Create shards - initialize all to shard_size with zeros
    let mut shards: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; data_shards + parity_shards];

    // Copy chunk data into data shards
    for (i, chunk) in chunk_data.chunks(shard_size).enumerate() {
        shards[i][..chunk.len()].copy_from_slice(chunk);
        // Remaining bytes are already zero-padded
    }

    // Encode to generate parity shards
    encoder
        .encode(&mut shards)
        .map_err(|e| EctarError::ErasureCoding(format!("Encoding failed: {:?}", e)))?;

    // Write shards to files
    let mut shard_infos = Vec::new();

    for (shard_idx, shard_data) in shards.iter().enumerate() {
        let shard_path = format_shard_path(output_base, chunk_number, shard_idx);

        let mut shard_file = File::create(&shard_path)?;
        shard_file.write_all(shard_data)?;

        log::debug!(
            "Written shard {} to {} ({} bytes)",
            shard_idx,
            shard_path.display(),
            shard_data.len()
        );

        shard_infos.push(ShardInfo {
            chunk_number,
            shard_number: shard_idx,
            path: shard_path,
            size: shard_data.len() as u64,
            is_parity: shard_idx >= data_shards,
        });
    }

    log::info!(
        "Created {} shards for chunk {} (shard size: {} bytes)",
        shards.len(),
        chunk_number,
        shard_size
    );

    Ok(shard_infos)
}

/// Format a shard file path
pub fn format_shard_path(output_base: &str, chunk_number: usize, shard_number: usize) -> PathBuf {
    PathBuf::from(format!(
        "{}.c{:03}.s{:02}",
        output_base, chunk_number, shard_number
    ))
}

#[derive(Debug, Clone)]
pub struct ShardInfo {
    pub chunk_number: usize,
    pub shard_number: usize,
    pub path: PathBuf,
    pub size: u64,
    pub is_parity: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_encode_chunk() {
        // Create a temporary chunk file
        let mut chunk_file = NamedTempFile::new().unwrap();
        let test_data = b"Hello, World! This is test data for Reed-Solomon encoding.";
        chunk_file.write_all(test_data).unwrap();
        chunk_file.flush().unwrap();

        let chunk_path = chunk_file.path().to_path_buf();
        let temp_dir = tempfile::tempdir().unwrap();
        let output_base = temp_dir.path().join("test").to_string_lossy().to_string();

        // Encode with 4 data + 2 parity shards
        let shards = encode_chunk(&chunk_path, &output_base, 1, 4, 2).unwrap();

        assert_eq!(shards.len(), 6);
        assert_eq!(shards.iter().filter(|s| !s.is_parity).count(), 4);
        assert_eq!(shards.iter().filter(|s| s.is_parity).count(), 2);

        // Verify all shard files exist
        for shard in &shards {
            assert!(shard.path.exists());
        }
    }
}
