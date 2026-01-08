use crate::erasure::decoder::ShardData;
use crate::error::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Discover and read shard files from a pattern
pub fn discover_shards(pattern: &str) -> Result<HashMap<usize, Vec<ShardData>>> {
    // Expand glob pattern to find shard files
    let paths = glob::glob(pattern).map_err(|e| {
        crate::error::EctarError::InvalidParameters(format!("Invalid pattern: {}", e))
    })?;

    let mut shards_by_chunk: HashMap<usize, Vec<ShardData>> = HashMap::new();

    for path_result in paths {
        let path = path_result.map_err(|e| {
            crate::error::EctarError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;

        // Read shard file
        match ShardData::from_file(&path) {
            Ok(shard) => {
                log::debug!(
                    "Found shard: chunk {}, shard {} at {}",
                    shard.chunk_number,
                    shard.shard_number,
                    path.display()
                );
                shards_by_chunk
                    .entry(shard.chunk_number)
                    .or_insert_with(Vec::new)
                    .push(shard);
            }
            Err(e) => {
                log::warn!("Skipping invalid shard file {}: {}", path.display(), e);
            }
        }
    }

    log::info!("Discovered {} chunks with shards", shards_by_chunk.len());

    for (chunk_num, shards) in &shards_by_chunk {
        log::info!("  Chunk {}: {} shards available", chunk_num, shards.len());
    }

    Ok(shards_by_chunk)
}

/// Find an index file from a shard pattern
pub fn find_index_file(shard_pattern: &str) -> Option<PathBuf> {
    // Try to extract base name from pattern
    // Pattern might be like "backup.c*.s*" or "/path/to/backup.c*.s*"
    let base = shard_pattern
        .replace(".c*", "")
        .replace(".s*", "")
        .replace("*", "");

    let index_path = PathBuf::from(format!("{}.index.zst", base));

    if index_path.exists() {
        Some(index_path)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_find_index_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().join("test-archive");
        let index_path = temp_dir.path().join("test-archive.index.zst");

        // Create the index file
        File::create(&index_path).unwrap();

        // Test finding it
        let pattern = format!("{}.c*.s*", base_path.to_string_lossy());
        let found = find_index_file(&pattern);

        assert!(found.is_some());
        assert_eq!(found.unwrap(), index_path);
    }

    #[test]
    fn test_find_index_file_not_exists() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().join("nonexistent");

        let pattern = format!("{}.c*.s*", base_path.to_string_lossy());
        let found = find_index_file(&pattern);

        assert!(found.is_none());
    }

    #[test]
    fn test_find_index_file_pattern_variations() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().join("archive");
        let index_path = temp_dir.path().join("archive.index.zst");

        File::create(&index_path).unwrap();

        // Test various pattern formats
        let patterns = vec![
            format!("{}.c*.s*", base_path.to_string_lossy()),
            format!("{}*", base_path.to_string_lossy()),
        ];

        for pattern in patterns {
            let found = find_index_file(&pattern);
            assert!(found.is_some(), "Pattern {} should find index", pattern);
        }
    }

    #[test]
    fn test_discover_shards_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path().join("test");

        // Create test shards with simple content
        // Shard format: chunk number in filename determines organization
        for chunk in 1..=2 {
            for shard in 0..3 {
                let shard_path = temp_dir
                    .path()
                    .join(format!("test.c{:03}.s{:02}", chunk, shard));
                let mut file = File::create(&shard_path).unwrap();
                // Write some test data
                file.write_all(&[chunk as u8; 100]).unwrap();
            }
        }

        // Discover shards
        let pattern = format!("{}.c*.s*", base.to_string_lossy());
        let shards = discover_shards(&pattern).unwrap();

        // Should find 2 chunks
        assert_eq!(shards.len(), 2);

        // Each chunk should have 3 shards
        assert_eq!(shards.get(&1).unwrap().len(), 3);
        assert_eq!(shards.get(&2).unwrap().len(), 3);
    }

    #[test]
    fn test_discover_shards_empty_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let pattern = format!("{}/nonexistent*.shard", temp_dir.path().to_string_lossy());

        let shards = discover_shards(&pattern).unwrap();
        assert_eq!(shards.len(), 0);
    }

    #[test]
    fn test_discover_shards_invalid_pattern() {
        // Test with a pattern that glob won't accept
        let result = discover_shards("[[[invalid");
        assert!(result.is_err());
    }
}
