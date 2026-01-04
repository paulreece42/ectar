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
    use crate::erasure::encoder;
    use std::fs::File;
    use std::io::Write as IoWrite;
    use tempfile::{NamedTempFile, TempDir};

    fn create_test_shards(temp_dir: &TempDir, data: &[u8], data_shards: usize, parity_shards: usize) -> Vec<ShardData> {
        // Create a temporary chunk file
        let mut chunk_file = NamedTempFile::new().unwrap();
        chunk_file.write_all(data).unwrap();
        chunk_file.flush().unwrap();

        let chunk_path = chunk_file.path().to_path_buf();
        let output_base = temp_dir.path().join("test").to_string_lossy().to_string();

        // Encode the chunk
        let shard_infos = encoder::encode_chunk(&chunk_path, &output_base, 1, data_shards, parity_shards).unwrap();

        // Load shards from files
        shard_infos
            .iter()
            .map(|info| ShardData::from_file(&info.path).unwrap())
            .collect()
    }

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

    #[test]
    fn test_parse_shard_filename_invalid_format() {
        // Missing .c marker
        assert!(parse_shard_filename("backup.s05").is_err());

        // Missing .s marker
        assert!(parse_shard_filename("backup.c001").is_err());

        // .s before .c (invalid order)
        assert!(parse_shard_filename("backup.s05.c001").is_err());

        // Non-numeric chunk number
        assert!(parse_shard_filename("backup.cabc.s05").is_err());

        // Non-numeric shard number
        assert!(parse_shard_filename("backup.c001.sxy").is_err());
    }

    #[test]
    fn test_decode_chunk_success() {
        let temp_dir = TempDir::new().unwrap();
        let test_data = b"Hello, World! This is test data for decoding.";
        let shards = create_test_shards(&temp_dir, test_data, 4, 2);

        let output_path = temp_dir.path().join("decoded.bin");
        let bytes_written = decode_chunk(
            shards,
            4,
            2,
            &output_path,
            Some(test_data.len() as u64),
        ).unwrap();

        assert_eq!(bytes_written, test_data.len() as u64);

        // Verify decoded content
        let decoded = std::fs::read(&output_path).unwrap();
        assert_eq!(decoded, test_data);
    }

    #[test]
    fn test_decode_chunk_with_missing_shard() {
        let temp_dir = TempDir::new().unwrap();
        let test_data = b"Hello, World! This is test data for recovery.";
        let mut shards = create_test_shards(&temp_dir, test_data, 4, 2);

        // Remove one data shard (still recoverable with parity)
        shards.remove(0);
        assert_eq!(shards.len(), 5); // 3 data + 2 parity = still recoverable

        let output_path = temp_dir.path().join("decoded.bin");
        let bytes_written = decode_chunk(
            shards,
            4,
            2,
            &output_path,
            Some(test_data.len() as u64),
        ).unwrap();

        assert_eq!(bytes_written, test_data.len() as u64);

        // Verify decoded content
        let decoded = std::fs::read(&output_path).unwrap();
        assert_eq!(decoded, test_data);
    }

    #[test]
    fn test_decode_chunk_insufficient_shards() {
        let temp_dir = TempDir::new().unwrap();
        let test_data = b"Test data";
        let mut shards = create_test_shards(&temp_dir, test_data, 4, 2);

        // Remove too many shards (need at least 4 data shards to recover)
        shards.truncate(3); // Only 3 shards left

        let output_path = temp_dir.path().join("decoded.bin");
        let result = decode_chunk(
            shards,
            4,
            2,
            &output_path,
            Some(test_data.len() as u64),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_decode_chunk_empty_shards() {
        let output_path = PathBuf::from("/tmp/decoded.bin");
        let result = decode_chunk(
            vec![],
            4,
            2,
            &output_path,
            None,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_decode_chunk_without_expected_size() {
        let temp_dir = TempDir::new().unwrap();
        let test_data = b"Hello, World!";
        let shards = create_test_shards(&temp_dir, test_data, 4, 2);

        let output_path = temp_dir.path().join("decoded.bin");
        let bytes_written = decode_chunk(
            shards,
            4,
            2,
            &output_path,
            None, // No expected size - will include padding
        ).unwrap();

        // Without expected size, output may include padding
        assert!(bytes_written >= test_data.len() as u64);
    }

    #[test]
    fn test_shard_data_from_file() {
        let temp_dir = TempDir::new().unwrap();

        // Create a shard file with proper naming
        let shard_path = temp_dir.path().join("test.c001.s02");
        let mut file = File::create(&shard_path).unwrap();
        file.write_all(b"shard data content").unwrap();
        drop(file);

        let shard = ShardData::from_file(&shard_path).unwrap();
        assert_eq!(shard.chunk_number, 1);
        assert_eq!(shard.shard_number, 2);
        assert_eq!(shard.data, b"shard data content");
    }

    #[test]
    fn test_shard_data_from_file_invalid_name() {
        let temp_dir = TempDir::new().unwrap();

        // Create a file with invalid naming
        let shard_path = temp_dir.path().join("invalid_name.bin");
        let mut file = File::create(&shard_path).unwrap();
        file.write_all(b"data").unwrap();
        drop(file);

        let result = ShardData::from_file(&shard_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_shard_data_fields() {
        let shard = ShardData {
            chunk_number: 5,
            shard_number: 10,
            data: vec![1, 2, 3, 4, 5],
        };

        assert_eq!(shard.chunk_number, 5);
        assert_eq!(shard.shard_number, 10);
        assert_eq!(shard.data, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_decode_chunk_large_data() {
        let temp_dir = TempDir::new().unwrap();
        let test_data = vec![42u8; 1024 * 100]; // 100KB
        let shards = create_test_shards(&temp_dir, &test_data, 10, 5);

        let output_path = temp_dir.path().join("decoded.bin");
        let bytes_written = decode_chunk(
            shards,
            10,
            5,
            &output_path,
            Some(test_data.len() as u64),
        ).unwrap();

        assert_eq!(bytes_written, test_data.len() as u64);

        // Verify decoded content
        let decoded = std::fs::read(&output_path).unwrap();
        assert_eq!(decoded, test_data);
    }

    #[test]
    fn test_decode_with_multiple_missing_shards() {
        let temp_dir = TempDir::new().unwrap();
        let test_data = b"Test data for multiple shard recovery";
        let mut shards = create_test_shards(&temp_dir, test_data, 4, 2);

        // Remove 2 shards (still recoverable with 4 remaining, need min 4)
        shards.remove(0);
        shards.remove(0);
        assert_eq!(shards.len(), 4); // Exactly at minimum

        let output_path = temp_dir.path().join("decoded.bin");
        let bytes_written = decode_chunk(
            shards,
            4,
            2,
            &output_path,
            Some(test_data.len() as u64),
        ).unwrap();

        assert_eq!(bytes_written, test_data.len() as u64);

        // Verify content
        let decoded = std::fs::read(&output_path).unwrap();
        assert_eq!(decoded.as_slice(), test_data.as_slice());
    }
}
