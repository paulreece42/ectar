use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveIndex {
    pub version: String,
    pub created: DateTime<Utc>,
    pub tool_version: String,
    pub archive_name: String,
    pub parameters: ArchiveParameters,
    pub chunks: Vec<ChunkInfo>,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveParameters {
    pub data_shards: usize,
    pub parity_shards: usize,
    pub chunk_size: Option<u64>,
    pub compression_level: i32,
    /// Tape devices used for RAIT mode (None for file-based storage)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tape_devices: Option<Vec<String>>,
    /// Block size used for tape writes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkInfo {
    pub chunk_number: usize,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub shard_size: u64,
    pub checksum: String,
    /// Tape shard positions: shard_num -> (device_index, byte_position)
    /// Only present for tape-based archives
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tape_shard_positions: Option<Vec<TapeShardPosition>>,
}

/// Position info for a shard on a tape device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TapeShardPosition {
    pub shard_num: usize,
    pub device_index: usize,
    pub position: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub chunk: usize,
    pub offset: u64,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    pub mode: u32,
    pub mtime: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gid: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub entry_type: FileType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spans_chunks: Option<Vec<usize>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    File,
    Directory,
    Symlink,
    Hardlink,
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_archive_parameters_serialization() {
        let params = ArchiveParameters {
            data_shards: 10,
            parity_shards: 5,
            chunk_size: Some(1024 * 1024),
            compression_level: 3,
            tape_devices: None,
            block_size: None,
        };

        let json = serde_json::to_string(&params).unwrap();
        let deserialized: ArchiveParameters = serde_json::from_str(&json).unwrap();

        assert_eq!(params.data_shards, deserialized.data_shards);
        assert_eq!(params.parity_shards, deserialized.parity_shards);
        assert_eq!(params.chunk_size, deserialized.chunk_size);
        assert_eq!(params.compression_level, deserialized.compression_level);
    }

    #[test]
    fn test_chunk_info_serialization() {
        let chunk = ChunkInfo {
            chunk_number: 1,
            compressed_size: 5000,
            uncompressed_size: 10000,
            shard_size: 500,
            checksum: "sha256:abc123".to_string(),
            tape_shard_positions: None,
        };

        let json = serde_json::to_string(&chunk).unwrap();
        let deserialized: ChunkInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(chunk.chunk_number, deserialized.chunk_number);
        assert_eq!(chunk.compressed_size, deserialized.compressed_size);
        assert_eq!(chunk.uncompressed_size, deserialized.uncompressed_size);
        assert_eq!(chunk.shard_size, deserialized.shard_size);
        assert_eq!(chunk.checksum, deserialized.checksum);
    }

    #[test]
    fn test_file_entry_serialization() {
        let now = Utc::now();
        let entry = FileEntry {
            path: "test/file.txt".to_string(),
            chunk: 1,
            offset: 0,
            size: 1024,
            compressed_size: Some(512),
            checksum: Some("sha256:test".to_string()),
            mode: 0o644,
            mtime: now,
            uid: Some(1000),
            gid: Some(1000),
            user: Some("testuser".to_string()),
            group: Some("testgroup".to_string()),
            entry_type: FileType::File,
            target: None,
            spans_chunks: None,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: FileEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.path, deserialized.path);
        assert_eq!(entry.chunk, deserialized.chunk);
        assert_eq!(entry.size, deserialized.size);
        assert_eq!(entry.mode, deserialized.mode);
    }

    #[test]
    fn test_file_entry_optional_fields() {
        let now = Utc::now();
        let entry = FileEntry {
            path: "test.txt".to_string(),
            chunk: 1,
            offset: 0,
            size: 100,
            compressed_size: None,
            checksum: None,
            mode: 0o644,
            mtime: now,
            uid: None,
            gid: None,
            user: None,
            group: None,
            entry_type: FileType::File,
            target: None,
            spans_chunks: None,
        };

        let json = serde_json::to_string(&entry).unwrap();

        // Verify optional fields are not in JSON when None
        assert!(!json.contains("\"compressed_size\""));
        assert!(!json.contains("\"checksum\""));
        assert!(!json.contains("\"uid\""));
        assert!(!json.contains("\"gid\""));
        assert!(!json.contains("\"user\""));
        assert!(!json.contains("\"group\""));
        assert!(!json.contains("\"target\""));
        assert!(!json.contains("\"spans_chunks\""));
    }

    #[test]
    fn test_file_type_serialization() {
        let types = vec![
            (FileType::File, "\"file\""),
            (FileType::Directory, "\"directory\""),
            (FileType::Symlink, "\"symlink\""),
            (FileType::Hardlink, "\"hardlink\""),
            (FileType::Other, "\"other\""),
        ];

        for (file_type, expected_json) in types {
            let json = serde_json::to_string(&file_type).unwrap();
            assert_eq!(json, expected_json);

            let deserialized: FileType = serde_json::from_str(&json).unwrap();
            assert_eq!(file_type, deserialized);
        }
    }

    #[test]
    fn test_archive_index_serialization() {
        let now = Utc::now();
        let index = ArchiveIndex {
            version: "1.0".to_string(),
            created: now,
            tool_version: "0.1.0".to_string(),
            archive_name: "test-archive".to_string(),
            parameters: ArchiveParameters {
                data_shards: 4,
                parity_shards: 2,
                chunk_size: Some(1024),
                compression_level: 3,
                tape_devices: None,
                block_size: None,
            },
            chunks: vec![ChunkInfo {
                chunk_number: 1,
                compressed_size: 500,
                uncompressed_size: 1000,
                shard_size: 100,
                checksum: "test".to_string(),
                tape_shard_positions: None,
            }],
            files: vec![FileEntry {
                path: "file.txt".to_string(),
                chunk: 1,
                offset: 0,
                size: 100,
                compressed_size: None,
                checksum: None,
                mode: 0o644,
                mtime: now,
                uid: None,
                gid: None,
                user: None,
                group: None,
                entry_type: FileType::File,
                target: None,
                spans_chunks: None,
            }],
        };

        let json = serde_json::to_string_pretty(&index).unwrap();
        let deserialized: ArchiveIndex = serde_json::from_str(&json).unwrap();

        assert_eq!(index.version, deserialized.version);
        assert_eq!(index.archive_name, deserialized.archive_name);
        assert_eq!(index.chunks.len(), deserialized.chunks.len());
        assert_eq!(index.files.len(), deserialized.files.len());
    }

    #[test]
    fn test_file_spanning_chunks() {
        let now = Utc::now();
        let entry = FileEntry {
            path: "large-file.bin".to_string(),
            chunk: 1,
            offset: 0,
            size: 1000000,
            compressed_size: None,
            checksum: None,
            mode: 0o644,
            mtime: now,
            uid: None,
            gid: None,
            user: None,
            group: None,
            entry_type: FileType::File,
            target: None,
            spans_chunks: Some(vec![1, 2, 3]),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: FileEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.spans_chunks, deserialized.spans_chunks);
        assert_eq!(deserialized.spans_chunks.unwrap(), vec![1, 2, 3]);
    }
}
