use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EctarError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Tar error: {0}")]
    Tar(String),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("Decompression error: {0}")]
    Decompression(String),

    #[error("Erasure coding error: {0}")]
    ErasureCoding(String),

    #[error("Insufficient shards for chunk {chunk}: need {needed}, have {available}")]
    InsufficientShards {
        chunk: usize,
        needed: usize,
        available: usize,
    },

    #[error("Corrupt shard: {shard} (checksum mismatch)")]
    CorruptShard { shard: String },

    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),

    #[error("Missing index file: {0}")]
    MissingIndex(PathBuf),

    #[error("Checksum mismatch for file: {file}")]
    ChecksumMismatch { file: String },

    #[error("Invalid shard file: {0}")]
    InvalidShardFile(PathBuf),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("File not found in archive: {0}")]
    FileNotFound(String),

    #[error("Invalid chunk size: {0}")]
    InvalidChunkSize(String),
}

impl From<serde_json::Error> for EctarError {
    fn from(err: serde_json::Error) -> Self {
        EctarError::Serialization(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, EctarError>;
