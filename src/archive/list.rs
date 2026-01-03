use crate::compression;
use crate::error::{EctarError, Result};
use crate::index::format::{ArchiveIndex, FileEntry, FileType};
use crate::io::shard_reader;
use std::fs::File;
use std::path::PathBuf;

pub struct ArchiveLister {
    input: String,
    filter_pattern: Option<String>,
    long_format: bool,
    output_format: OutputFormat,
}

#[derive(Debug, Clone)]
pub enum OutputFormat {
    Text,
    Json,
    Csv,
}

impl ArchiveLister {
    pub fn new(input: String) -> Self {
        Self {
            input,
            filter_pattern: None,
            long_format: false,
            output_format: OutputFormat::Text,
        }
    }

    pub fn filter(mut self, pattern: Option<String>) -> Self {
        self.filter_pattern = pattern;
        self
    }

    pub fn long_format(mut self, long: bool) -> Self {
        self.long_format = long;
        self
    }

    pub fn output_format(mut self, format: &str) -> Result<Self> {
        self.output_format = match format.to_lowercase().as_str() {
            "text" => OutputFormat::Text,
            "json" => OutputFormat::Json,
            "csv" => OutputFormat::Csv,
            _ => return Err(EctarError::InvalidParameters(format!(
                "Invalid output format: {}. Use text, json, or csv",
                format
            ))),
        };
        Ok(self)
    }

    pub fn list(&self) -> Result<ListMetadata> {
        // Find and read index file
        let index_path = shard_reader::find_index_file(&self.input)
            .ok_or_else(|| EctarError::MissingIndex(PathBuf::from(&self.input)))?;

        let index = self.read_index(&index_path)?;

        // Filter files if pattern provided
        let files: Vec<&FileEntry> = if let Some(ref pattern) = self.filter_pattern {
            index.files.iter()
                .filter(|f| self.matches_pattern(&f.path, pattern))
                .collect()
        } else {
            index.files.iter().collect()
        };

        // Display based on format
        match self.output_format {
            OutputFormat::Text => self.display_text(&files, &index),
            OutputFormat::Json => self.display_json(&files),
            OutputFormat::Csv => self.display_csv(&files),
        }

        Ok(ListMetadata {
            total_files: files.len(),
            total_size: files.iter().map(|f| f.size).sum(),
        })
    }

    fn read_index(&self, index_path: &PathBuf) -> Result<ArchiveIndex> {
        let index_file = File::open(index_path)?;
        let mut decoder = compression::create_decoder(index_file)?;

        let mut json = String::new();
        std::io::Read::read_to_string(&mut decoder, &mut json)?;

        let index: ArchiveIndex = serde_json::from_str(&json)?;
        Ok(index)
    }

    fn matches_pattern(&self, path: &str, pattern: &str) -> bool {
        // Simple glob-style matching
        path.contains(pattern) || glob::Pattern::new(pattern)
            .map(|p| p.matches(path))
            .unwrap_or(false)
    }

    fn display_text(&self, files: &[&FileEntry], index: &ArchiveIndex) {
        if self.long_format {
            println!("Archive: {}", index.archive_name);
            println!("Created: {}", index.created);
            println!("Files: {}", files.len());
            println!();
            println!("{:<10} {:<12} {:<8} {:<10} {}", "Type", "Size", "Chunk", "Mode", "Path");
            println!("{}", "-".repeat(80));

            for file in files {
                let file_type = match file.entry_type {
                    FileType::File => "file",
                    FileType::Directory => "dir",
                    FileType::Symlink => "symlink",
                    FileType::Hardlink => "hardlink",
                    FileType::Other => "other",
                };

                let size_str = if file.size > 1024 * 1024 {
                    format!("{:.2}MB", file.size as f64 / (1024.0 * 1024.0))
                } else if file.size > 1024 {
                    format!("{:.2}KB", file.size as f64 / 1024.0)
                } else {
                    format!("{}B", file.size)
                };

                let chunks_info = if let Some(ref spans) = file.spans_chunks {
                    format!("{}-{}", spans.first().unwrap_or(&0), spans.last().unwrap_or(&0))
                } else {
                    file.chunk.to_string()
                };

                println!("{:<10} {:<12} {:<8} {:<10o} {}",
                    file_type,
                    size_str,
                    chunks_info,
                    file.mode,
                    file.path
                );
            }
        } else {
            for file in files {
                println!("{}", file.path);
            }
        }
    }

    fn display_json(&self, files: &[&FileEntry]) {
        let json = serde_json::to_string_pretty(&files).unwrap();
        println!("{}", json);
    }

    fn display_csv(&self, files: &[&FileEntry]) {
        println!("path,type,size,chunk,mode,mtime,checksum");
        for file in files {
            let file_type = match file.entry_type {
                FileType::File => "file",
                FileType::Directory => "directory",
                FileType::Symlink => "symlink",
                FileType::Hardlink => "hardlink",
                FileType::Other => "other",
            };

            let checksum = file.checksum.as_ref().map(|s| s.as_str()).unwrap_or("");

            println!("{},{},{},{},{},{},{}",
                file.path,
                file_type,
                file.size,
                file.chunk,
                file.mode,
                file.mtime,
                checksum
            );
        }
    }
}

pub struct ListMetadata {
    pub total_files: usize,
    pub total_size: u64,
}
