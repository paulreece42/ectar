use crate::compression;
use crate::error::{EctarError, Result};
use crate::index::format::ArchiveIndex;
use crate::io::shard_reader;
use std::fs::File;
use std::path::PathBuf;

pub struct ArchiveInfo {
    input: String,
    output_format: OutputFormat,
}

#[derive(Debug, Clone)]
pub enum OutputFormat {
    Text,
    Json,
}

impl ArchiveInfo {
    pub fn new(input: String) -> Self {
        Self {
            input,
            output_format: OutputFormat::Text,
        }
    }

    pub fn output_format(mut self, format: &str) -> Result<Self> {
        self.output_format = match format.to_lowercase().as_str() {
            "text" => OutputFormat::Text,
            "json" => OutputFormat::Json,
            _ => return Err(EctarError::InvalidParameters(format!(
                "Invalid output format: {}. Use text or json",
                format
            ))),
        };
        Ok(self)
    }

    pub fn show(&self) -> Result<()> {
        // Find and read index file
        let index_path = shard_reader::find_index_file(&self.input)
            .ok_or_else(|| EctarError::MissingIndex(PathBuf::from(&self.input)))?;

        let index = self.read_index(&index_path)?;

        // Display based on format
        match self.output_format {
            OutputFormat::Text => self.display_text(&index),
            OutputFormat::Json => self.display_json(&index),
        }

        Ok(())
    }

    fn read_index(&self, index_path: &PathBuf) -> Result<ArchiveIndex> {
        let index_file = File::open(index_path)?;
        let mut decoder = compression::create_decoder(index_file)?;

        let mut json = String::new();
        std::io::Read::read_to_string(&mut decoder, &mut json)?;

        let index: ArchiveIndex = serde_json::from_str(&json)?;
        Ok(index)
    }

    fn display_text(&self, index: &ArchiveIndex) {
        println!("Archive Information");
        println!("{}", "=".repeat(60));
        println!("Name:              {}", index.archive_name);
        println!("Created:           {}", index.created);
        println!("Tool Version:      {}", index.tool_version);
        println!("Index Version:     {}", index.version);
        println!();

        println!("Erasure Coding Parameters");
        println!("{}", "-".repeat(60));
        println!("Data Shards:       {}", index.parameters.data_shards);
        println!("Parity Shards:     {}", index.parameters.parity_shards);
        println!("Total Shards:      {}", index.parameters.data_shards + index.parameters.parity_shards);
        println!("Redundancy:        {:.1}%",
            (index.parameters.parity_shards as f64 / index.parameters.data_shards as f64) * 100.0);
        println!("Can Lose:          {} shards", index.parameters.parity_shards);

        if let Some(chunk_size) = index.parameters.chunk_size {
            println!("Chunk Size:        {} bytes ({:.2} MB)",
                chunk_size,
                chunk_size as f64 / (1024.0 * 1024.0)
            );
        }
        println!("Compression Level: {}", index.parameters.compression_level);
        println!();

        println!("Archive Statistics");
        println!("{}", "-".repeat(60));
        println!("Total Files:       {}", index.files.len());
        println!("Total Chunks:      {}", index.chunks.len());

        let total_uncompressed: u64 = index.chunks.iter().map(|c| c.uncompressed_size).sum();
        let total_compressed: u64 = index.chunks.iter().map(|c| c.compressed_size).sum();
        let total_shards_size: u64 = index.chunks.iter()
            .map(|c| c.shard_size * (index.parameters.data_shards + index.parameters.parity_shards) as u64)
            .sum();

        println!("Original Size:     {} bytes ({:.2} MB)",
            total_uncompressed,
            total_uncompressed as f64 / (1024.0 * 1024.0)
        );
        println!("Compressed Size:   {} bytes ({:.2} MB)",
            total_compressed,
            total_compressed as f64 / (1024.0 * 1024.0)
        );
        println!("Total Shard Size:  {} bytes ({:.2} MB)",
            total_shards_size,
            total_shards_size as f64 / (1024.0 * 1024.0)
        );

        if total_uncompressed > 0 {
            println!("Compression Ratio: {:.2}%",
                (total_compressed as f64 / total_uncompressed as f64) * 100.0
            );
            if total_compressed > 0 {
                println!("Storage Overhead:  {:.2}%",
                    ((total_shards_size as f64 / total_compressed as f64) - 1.0) * 100.0
                );
            }
        }
        println!();

        if !index.chunks.is_empty() {
            println!("Chunk Details");
            println!("{}", "-".repeat(60));
            println!("{:<8} {:<15} {:<15} {:<12}", "Chunk", "Uncompressed", "Compressed", "Shard Size");
            println!("{}", "-".repeat(60));
            for chunk in &index.chunks {
                println!("{:<8} {:<15} {:<15} {:<12}",
                    chunk.chunk_number,
                    format!("{} B", chunk.uncompressed_size),
                    format!("{} B", chunk.compressed_size),
                    format!("{} B", chunk.shard_size),
                );
            }
        }
    }

    fn display_json(&self, index: &ArchiveIndex) {
        let json = serde_json::to_string_pretty(&index).unwrap();
        println!("{}", json);
    }
}
