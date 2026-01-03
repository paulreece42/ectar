pub mod zstd;

// Re-export commonly used functions
pub use zstd::{compress, create_decoder, create_encoder, decompress};
