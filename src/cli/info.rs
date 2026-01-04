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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::create::ArchiveBuilder;
    use std::fs::File;
    use std::io::Write as IoWriteTrait;
    use tempfile::TempDir;

    fn create_test_archive(temp_dir: &TempDir) -> String {
        let test_file = temp_dir.path().join("test.txt");
        let mut file = File::create(&test_file).unwrap();
        file.write_all(b"Test data for info display").unwrap();
        drop(file);

        let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
        let builder = ArchiveBuilder::new(archive_base.clone())
            .data_shards(4)
            .parity_shards(2)
            .chunk_size(Some(1024 * 1024));

        builder.create(&[test_file]).unwrap();
        archive_base
    }

    #[test]
    fn test_archive_info_new() {
        let info = ArchiveInfo::new("test_pattern".to_string());
        assert_eq!(info.input, "test_pattern");
        assert!(matches!(info.output_format, OutputFormat::Text));
    }

    #[test]
    fn test_output_format_text() {
        let info = ArchiveInfo::new("test".to_string())
            .output_format("text")
            .unwrap();
        assert!(matches!(info.output_format, OutputFormat::Text));
    }

    #[test]
    fn test_output_format_json() {
        let info = ArchiveInfo::new("test".to_string())
            .output_format("json")
            .unwrap();
        assert!(matches!(info.output_format, OutputFormat::Json));
    }

    #[test]
    fn test_output_format_case_insensitive() {
        let info = ArchiveInfo::new("test".to_string())
            .output_format("JSON")
            .unwrap();
        assert!(matches!(info.output_format, OutputFormat::Json));

        let info = ArchiveInfo::new("test".to_string())
            .output_format("Text")
            .unwrap();
        assert!(matches!(info.output_format, OutputFormat::Text));
    }

    #[test]
    fn test_output_format_invalid() {
        let result = ArchiveInfo::new("test".to_string())
            .output_format("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_show_text_format() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive(&temp_dir);
        let pattern = format!("{}.c*.s*", archive_base);

        let info = ArchiveInfo::new(pattern)
            .output_format("text")
            .unwrap();
        let result = info.show();
        assert!(result.is_ok());
    }

    #[test]
    fn test_show_json_format() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive(&temp_dir);
        let pattern = format!("{}.c*.s*", archive_base);

        let info = ArchiveInfo::new(pattern)
            .output_format("json")
            .unwrap();
        let result = info.show();
        assert!(result.is_ok());
    }

    #[test]
    fn test_show_missing_index() {
        let temp_dir = TempDir::new().unwrap();
        let pattern = temp_dir.path().join("nonexistent.c*.s*").to_string_lossy().to_string();

        let info = ArchiveInfo::new(pattern);
        let result = info.show();
        assert!(result.is_err());
    }

    #[test]
    fn test_output_format_debug() {
        let format = OutputFormat::Text;
        assert_eq!(format!("{:?}", format), "Text");

        let format = OutputFormat::Json;
        assert_eq!(format!("{:?}", format), "Json");
    }
}
