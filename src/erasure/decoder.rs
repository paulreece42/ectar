use crate::error::{EctarError, Result};
use reed_solomon_erasure::galois_8::ReedSolomon;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

/// Reconstruct a chunk from available shards using Reed-Solomon decoding
pub fn decode_chunk(
    available_shards: Vec<ShardData>,
    data_shards: usize,
    parity_shards: usize,
    output_path: &PathBuf,
    expected_size: Option<u64>,
) -> Result<u64> {
    let total_shards = data_shards + parity_shards;

    log::info!(
        "Reconstructing chunk from {} available shards (need {}, have {})",
        available_shards.len(),
        data_shards,
        total_shards
    );

    // Validate we have enough shards
    if available_shards.len() < data_shards {
        return Err(EctarError::InsufficientShards {
            chunk: available_shards.get(0).map(|s| s.chunk_number).unwrap_or(0),
            needed: data_shards,
            available: available_shards.len(),
        });
    }

    // Determine shard size (all shards should be the same size)
    let shard_size = if let Some(first) = available_shards.first() {
        first.data.len()
    } else {
        return Err(EctarError::ErasureCoding(
            "No shards available".to_string(),
        ));
    };

    // Create Reed-Solomon decoder
    let decoder = ReedSolomon::new(data_shards, parity_shards)
        .map_err(|e| EctarError::ErasureCoding(format!("Failed to create decoder: {:?}", e)))?;

    // Create shard array with None for missing shards
    let mut shards: Vec<Option<Vec<u8>>> = vec![None; total_shards];

    // Fill in available shards
    for shard in available_shards {
        if shard.shard_number < total_shards {
            shards[shard.shard_number] = Some(shard.data);
        }
    }

    // Reconstruct missing shards
    decoder
        .reconstruct(&mut shards)
        .map_err(|e| EctarError::ErasureCoding(format!("Reconstruction failed: {:?}", e)))?;

    // Combine data shards to get original chunk
    let mut reconstructed = Vec::new();
    for i in 0..data_shards {
        if let Some(ref shard) = shards[i] {
            reconstructed.extend_from_slice(shard);
        } else {
            return Err(EctarError::ErasureCoding(
                "Missing data shard after reconstruction".to_string(),
            ));
        }
    }

    // Trim to expected size if provided (removes zero-padding)
    if let Some(expected) = expected_size {
        let original_len = reconstructed.len();
        if original_len > expected as usize {
            reconstructed.truncate(expected as usize);
            log::debug!(
                "Trimmed reconstructed chunk from {} to {} bytes",
                original_len,
                expected
            );
        }
    }

    // Write reconstructed chunk to file
    let mut output_file = File::create(output_path)?;
    output_file.write_all(&reconstructed)?;

    let bytes_written = reconstructed.len() as u64;

    log::info!(
        "Successfully reconstructed chunk to {} ({} bytes)",
        output_path.display(),
        bytes_written
    );

    Ok(bytes_written)
}

#[derive(Debug, Clone)]
pub struct ShardData {
    pub chunk_number: usize,
    pub shard_number: usize,
    pub data: Vec<u8>,
}

impl ShardData {
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        // Parse shard filename to extract chunk and shard numbers
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| EctarError::InvalidShardFile(path.clone()))?;

        let (chunk_number, shard_number) = parse_shard_filename(filename)?;

        // Read shard data
        let mut file = File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        Ok(ShardData {
            chunk_number,
            shard_number,
            data,
        })
    }
}

/// Parse a shard filename like "backup.c001.s05" into (chunk_number, shard_number)
fn parse_shard_filename(filename: &str) -> Result<(usize, usize)> {
    // Find .c and .s markers
    let c_pos = filename
        .find(".c")
        .ok_or_else(|| EctarError::InvalidShardFile(PathBuf::from(filename)))?;
    let s_pos = filename
        .rfind(".s")
        .ok_or_else(|| EctarError::InvalidShardFile(PathBuf::from(filename)))?;

    if s_pos <= c_pos {
        return Err(EctarError::InvalidShardFile(PathBuf::from(filename)));
    }

    // Extract chunk number (after .c, before .s)
    let chunk_str = &filename[c_pos + 2..s_pos];
    let chunk_number: usize = chunk_str
        .parse()
        .map_err(|_| EctarError::InvalidShardFile(PathBuf::from(filename)))?;

    // Extract shard number (after .s)
    let shard_str = &filename[s_pos + 2..];
    let shard_number: usize = shard_str
        .parse()
        .map_err(|_| EctarError::InvalidShardFile(PathBuf::from(filename)))?;

    Ok((chunk_number, shard_number))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_shard_filename() {
        assert_eq!(
            parse_shard_filename("backup.c001.s05").unwrap(),
            (1, 5)
        );
        assert_eq!(
            parse_shard_filename("archive.c042.s12").unwrap(),
            (42, 12)
        );
        assert_eq!(
            parse_shard_filename("test.c999.s99").unwrap(),
            (999, 99)
        );

        // Invalid filenames
        assert!(parse_shard_filename("invalid").is_err());
        assert!(parse_shard_filename("backup.tar.zst").is_err());
    }
}
