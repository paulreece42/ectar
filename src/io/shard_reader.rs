use crate::erasure::decoder::ShardData;
use crate::error::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Discover and read shard files from a pattern
pub fn discover_shards(pattern: &str) -> Result<HashMap<usize, Vec<ShardData>>> {
    // Expand glob pattern to find shard files
    let paths = glob::glob(pattern)
        .map_err(|e| crate::error::EctarError::InvalidParameters(format!("Invalid pattern: {}", e)))?;

    let mut shards_by_chunk: HashMap<usize, Vec<ShardData>> = HashMap::new();

    for path_result in paths {
        let path = path_result
            .map_err(|e| crate::error::EctarError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

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

    log::info!(
        "Discovered {} chunks with shards",
        shards_by_chunk.len()
    );

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
