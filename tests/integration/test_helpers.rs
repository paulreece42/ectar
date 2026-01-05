use ectar::archive::create::ArchiveBuilder;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Corrupts a file by XORing bytes at the specified position
pub fn corrupt_file_bytes(path: &Path, position: usize, byte_count: usize) -> std::io::Result<()> {
    let mut file = File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    drop(file);

    if position + byte_count > data.len() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Position + byte_count exceeds file size",
        ));
    }

    // XOR bytes to corrupt them
    for i in position..position + byte_count {
        data[i] ^= 0xFF;
    }

    let mut file = File::create(path)?;
    file.write_all(&data)?;
    Ok(())
}

/// Creates a minimal archive for testing and returns the archive base path
pub fn create_minimal_archive(temp_dir: &TempDir) -> Result<String, Box<dyn std::error::Error>> {
    let test_file = temp_dir.path().join("test.txt");
    let mut file = File::create(&test_file)?;
    file.write_all(b"Test data")?;
    drop(file);

    let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
    let builder = ArchiveBuilder::new(archive_base.clone())
        .data_shards(4)
        .parity_shards(2)
        .chunk_size(Some(1024 * 1024));

    builder.create(&[test_file])?;
    Ok(archive_base)
}

/// Corruption types for index files
pub enum IndexCorruption {
    TruncateJson,
    InvalidSyntax,
    MissingField,
    InvalidVersion,
}

/// Corrupts the index JSON file in various ways
pub fn corrupt_index_json(index_path: &Path, corruption_type: IndexCorruption) -> std::io::Result<()> {
    // Read and decompress the index
    let compressed_data = fs::read(index_path)?;
    let decompressed_data = zstd::decode_all(&compressed_data[..])?;
    let mut json_str = String::from_utf8(decompressed_data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    // Apply corruption
    match corruption_type {
        IndexCorruption::TruncateJson => {
            // Truncate JSON to 50% of its size
            json_str.truncate(json_str.len() / 2);
        }
        IndexCorruption::InvalidSyntax => {
            // Remove closing brace to make invalid JSON
            if let Some(pos) = json_str.rfind('}') {
                json_str.truncate(pos);
            }
        }
        IndexCorruption::MissingField => {
            // Remove the "chunks" field
            if let Some(start) = json_str.find("\"chunks\"") {
                if let Some(end) = json_str[start..].find(']') {
                    json_str.replace_range(start..start + end + 1, "\"chunks\":[]");
                }
            }
        }
        IndexCorruption::InvalidVersion => {
            // Change version to an invalid value
            json_str = json_str.replace("\"1.0\"", "\"999.0\"");
        }
    }

    // Recompress and write back
    let compressed = zstd::encode_all(json_str.as_bytes(), 3)?;
    let mut file = File::create(index_path)?;
    file.write_all(&compressed)?;
    Ok(())
}

/// Deletes random shards from an archive
pub fn delete_random_shards(base_path: &Path, count: usize) -> Vec<PathBuf> {
    let mut deleted = Vec::new();
    let parent = base_path.parent().unwrap_or(base_path);
    let base_name = base_path.file_name().unwrap().to_string_lossy();

    // Find all shard files
    let pattern = format!("{}/{}.c*.s*", parent.display(), base_name);
    if let Ok(entries) = glob::glob(&pattern) {
        let shards: Vec<PathBuf> = entries.filter_map(Result::ok).collect();

        for (i, shard) in shards.iter().take(count).enumerate() {
            if let Ok(()) = fs::remove_file(shard) {
                deleted.push(shard.clone());
            }
            if i + 1 >= count {
                break;
            }
        }
    }

    deleted
}
