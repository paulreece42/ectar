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
            _ => {
                return Err(EctarError::InvalidParameters(format!(
                    "Invalid output format: {}. Use text, json, or csv",
                    format
                )))
            }
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
            index
                .files
                .iter()
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
        path.contains(pattern)
            || glob::Pattern::new(pattern)
                .map(|p| p.matches(path))
                .unwrap_or(false)
    }

    fn display_text(&self, files: &[&FileEntry], index: &ArchiveIndex) {
        if self.long_format {
            println!("Archive: {}", index.archive_name);
            println!("Created: {}", index.created);
            println!("Files: {}", files.len());
            println!();
            println!(
                "{:<10} {:<12} {:<8} {:<10} {}",
                "Type", "Size", "Chunk", "Mode", "Path"
            );
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
                    format!(
                        "{}-{}",
                        spans.first().unwrap_or(&0),
                        spans.last().unwrap_or(&0)
                    )
                } else {
                    file.chunk.to_string()
                };

                println!(
                    "{:<10} {:<12} {:<8} {:<10o} {}",
                    file_type, size_str, chunks_info, file.mode, file.path
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

            println!(
                "{},{},{},{},{},{},{}",
                file.path, file_type, file.size, file.chunk, file.mode, file.mtime, checksum
            );
        }
    }
}

pub struct ListMetadata {
    pub total_files: usize,
    pub total_size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::create::ArchiveBuilder;
    use std::fs::{self, File};
    use std::io::Write as IoWriteTrait;
    use tempfile::TempDir;

    fn create_test_archive_with_files(temp_dir: &TempDir) -> String {
        let test_dir = temp_dir.path().join("testdata");
        fs::create_dir(&test_dir).unwrap();

        // Create multiple files
        let file1 = test_dir.join("file1.txt");
        let mut f = File::create(&file1).unwrap();
        f.write_all(b"Content of file 1").unwrap();
        drop(f);

        let file2 = test_dir.join("file2.txt");
        let mut f = File::create(&file2).unwrap();
        f.write_all(b"Content of file 2").unwrap();
        drop(f);

        let subdir = test_dir.join("subdir");
        fs::create_dir(&subdir).unwrap();

        let file3 = subdir.join("file3.txt");
        let mut f = File::create(&file3).unwrap();
        f.write_all(b"Content of file 3 in subdir").unwrap();
        drop(f);

        let archive_base = temp_dir
            .path()
            .join("archive")
            .to_string_lossy()
            .to_string();
        let builder = ArchiveBuilder::new(archive_base.clone())
            .data_shards(4)
            .parity_shards(2)
            .chunk_size(Some(1024 * 1024));

        builder.create(&[test_dir]).unwrap();
        archive_base
    }

    #[test]
    fn test_archive_lister_new() {
        let lister = ArchiveLister::new("test_pattern".to_string());
        assert_eq!(lister.input, "test_pattern");
        assert!(lister.filter_pattern.is_none());
        assert!(!lister.long_format);
        assert!(matches!(lister.output_format, OutputFormat::Text));
    }

    #[test]
    fn test_filter() {
        let lister = ArchiveLister::new("test".to_string()).filter(Some("*.txt".to_string()));
        assert_eq!(lister.filter_pattern, Some("*.txt".to_string()));
    }

    #[test]
    fn test_long_format() {
        let lister = ArchiveLister::new("test".to_string()).long_format(true);
        assert!(lister.long_format);
    }

    #[test]
    fn test_output_format_text() {
        let lister = ArchiveLister::new("test".to_string())
            .output_format("text")
            .unwrap();
        assert!(matches!(lister.output_format, OutputFormat::Text));
    }

    #[test]
    fn test_output_format_json() {
        let lister = ArchiveLister::new("test".to_string())
            .output_format("json")
            .unwrap();
        assert!(matches!(lister.output_format, OutputFormat::Json));
    }

    #[test]
    fn test_output_format_csv() {
        let lister = ArchiveLister::new("test".to_string())
            .output_format("csv")
            .unwrap();
        assert!(matches!(lister.output_format, OutputFormat::Csv));
    }

    #[test]
    fn test_output_format_case_insensitive() {
        let lister = ArchiveLister::new("test".to_string())
            .output_format("JSON")
            .unwrap();
        assert!(matches!(lister.output_format, OutputFormat::Json));

        let lister = ArchiveLister::new("test".to_string())
            .output_format("CSV")
            .unwrap();
        assert!(matches!(lister.output_format, OutputFormat::Csv));
    }

    #[test]
    fn test_output_format_invalid() {
        let result = ArchiveLister::new("test".to_string()).output_format("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_text_format() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive_with_files(&temp_dir);
        let pattern = format!("{}.c*.s*", archive_base);

        let lister = ArchiveLister::new(pattern).output_format("text").unwrap();
        let result = lister.list();
        assert!(result.is_ok());
        let metadata = result.unwrap();
        assert!(metadata.total_files >= 3); // At least 3 files + directories
    }

    #[test]
    fn test_list_json_format() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive_with_files(&temp_dir);
        let pattern = format!("{}.c*.s*", archive_base);

        let lister = ArchiveLister::new(pattern).output_format("json").unwrap();
        let result = lister.list();
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_csv_format() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive_with_files(&temp_dir);
        let pattern = format!("{}.c*.s*", archive_base);

        let lister = ArchiveLister::new(pattern).output_format("csv").unwrap();
        let result = lister.list();
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_long_format() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive_with_files(&temp_dir);
        let pattern = format!("{}.c*.s*", archive_base);

        let lister = ArchiveLister::new(pattern).long_format(true);
        let result = lister.list();
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_with_filter() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive_with_files(&temp_dir);
        let pattern = format!("{}.c*.s*", archive_base);

        let lister = ArchiveLister::new(pattern).filter(Some("file1".to_string()));
        let result = lister.list();
        assert!(result.is_ok());
        let metadata = result.unwrap();
        assert_eq!(metadata.total_files, 1); // Only file1.txt should match
    }

    #[test]
    fn test_list_with_glob_filter() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive_with_files(&temp_dir);
        let pattern = format!("{}.c*.s*", archive_base);

        let lister = ArchiveLister::new(pattern).filter(Some("*.txt".to_string()));
        let result = lister.list();
        assert!(result.is_ok());
        let metadata = result.unwrap();
        assert!(metadata.total_files >= 3); // All .txt files
    }

    #[test]
    fn test_list_missing_index() {
        let temp_dir = TempDir::new().unwrap();
        let pattern = temp_dir
            .path()
            .join("nonexistent.c*.s*")
            .to_string_lossy()
            .to_string();

        let lister = ArchiveLister::new(pattern);
        let result = lister.list();
        assert!(result.is_err());
    }

    #[test]
    fn test_list_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive_with_files(&temp_dir);
        let pattern = format!("{}.c*.s*", archive_base);

        let lister = ArchiveLister::new(pattern);
        let result = lister.list();
        assert!(result.is_ok());
        let metadata = result.unwrap();
        assert!(metadata.total_files > 0);
        assert!(metadata.total_size > 0);
    }

    #[test]
    fn test_output_format_debug() {
        let format = OutputFormat::Text;
        assert_eq!(format!("{:?}", format), "Text");

        let format = OutputFormat::Json;
        assert_eq!(format!("{:?}", format), "Json");

        let format = OutputFormat::Csv;
        assert_eq!(format!("{:?}", format), "Csv");
    }

    #[test]
    fn test_matches_pattern_simple() {
        let lister = ArchiveLister::new("test".to_string());
        assert!(lister.matches_pattern("file.txt", "file"));
        assert!(lister.matches_pattern("file.txt", ".txt"));
        assert!(!lister.matches_pattern("file.txt", "other"));
    }

    #[test]
    fn test_matches_pattern_glob() {
        let lister = ArchiveLister::new("test".to_string());
        assert!(lister.matches_pattern("file.txt", "*.txt"));
        assert!(lister.matches_pattern("path/to/file.txt", "**/file.txt"));
        assert!(!lister.matches_pattern("file.txt", "*.rs"));
    }
}
