use crate::compression;
use crate::erasure::decoder;
use crate::error::{EctarError, Result};
use crate::index::format::ArchiveIndex;
use crate::io::shard_reader;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub struct ArchiveExtractor {
    shard_pattern: String,
    output_dir: PathBuf,
    verify_checksums: bool,
    partial: bool,
    file_filters: Vec<String>,
    exclude_patterns: Vec<String>,
    strip_components: usize,
}

impl ArchiveExtractor {
    pub fn new(shard_pattern: String, output_dir: Option<PathBuf>) -> Self {
        Self {
            shard_pattern,
            output_dir: output_dir.unwrap_or_else(|| PathBuf::from(".")),
            verify_checksums: true,
            partial: false,
            file_filters: Vec::new(),
            exclude_patterns: Vec::new(),
            strip_components: 0,
        }
    }

    pub fn verify_checksums(mut self, verify: bool) -> Self {
        self.verify_checksums = verify;
        self
    }

    pub fn partial(mut self, partial: bool) -> Self {
        self.partial = partial;
        self
    }

    pub fn file_filters(mut self, filters: Vec<String>) -> Self {
        self.file_filters = filters;
        self
    }

    pub fn exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.exclude_patterns = patterns;
        self
    }

    pub fn strip_components(mut self, n: usize) -> Self {
        self.strip_components = n;
        self
    }

    pub fn extract(&self) -> Result<ExtractionMetadata> {
        log::info!("Extracting archive from pattern: {}", self.shard_pattern);

        // Try to read index file (optional)
        let index_opt = match shard_reader::find_index_file(&self.shard_pattern) {
            Some(index_path) => {
                log::info!("Found index file: {}", index_path.display());
                match self.read_index(&index_path) {
                    Ok(index) => {
                        log::info!("Archive: {}", index.archive_name);
                        log::info!("  Data shards: {}", index.parameters.data_shards);
                        log::info!("  Parity shards: {}", index.parameters.parity_shards);
                        log::info!("  Chunks: {}", index.chunks.len());
                        log::info!("  Files: {}", index.files.len());
                        Some(index)
                    }
                    Err(e) => {
                        log::warn!("Failed to read index file: {}", e);
                        None
                    }
                }
            }
            None => {
                log::warn!("No index file found - will extract from shard headers only");
                log::warn!("File filtering and metadata will not be available");
                None
            }
        };

        // Extract using index if available, otherwise extract from shards only
        if let Some(index) = index_opt {
            self.extract_with_index(index)
        } else {
            self.extract_from_shards_only()
        }
    }

    /// Extract archive using index file (full functionality)
    fn extract_with_index(&self, index: ArchiveIndex) -> Result<ExtractionMetadata> {

        // Discover available shards
        let shards_by_chunk = shard_reader::discover_shards(&self.shard_pattern)?;

        // Create temporary directory for reconstructed chunks
        let temp_dir = TempDir::new()?;

        // Reconstruct each chunk
        let mut chunks_recovered = 0;
        let mut chunks_failed = Vec::new();

        for chunk_info in &index.chunks {
            let chunk_num = chunk_info.chunk_number;

            match shards_by_chunk.get(&chunk_num) {
                Some(shards) => {
                    if shards.len() < index.parameters.data_shards {
                        log::error!(
                            "Chunk {}: insufficient shards ({}/{})",
                            chunk_num,
                            shards.len(),
                            index.parameters.data_shards
                        );
                        chunks_failed.push(chunk_num);
                        continue;
                    }

                    // Reconstruct chunk
                    let chunk_path = temp_dir.path().join(format!("chunk{:03}.tar.zst", chunk_num));

                    match decoder::decode_chunk(
                        shards.clone(),
                        index.parameters.data_shards,
                        index.parameters.parity_shards,
                        &chunk_path,
                        Some(chunk_info.compressed_size),
                    ) {
                        Ok(_) => {
                            log::info!("Chunk {} reconstructed successfully", chunk_num);
                            chunks_recovered += 1;
                        }
                        Err(e) => {
                            log::error!("Failed to reconstruct chunk {}: {}", chunk_num, e);
                            chunks_failed.push(chunk_num);
                        }
                    }
                }
                None => {
                    log::error!("Chunk {}: no shards found", chunk_num);
                    chunks_failed.push(chunk_num);
                }
            }
        }

        if chunks_recovered == 0 {
            if self.partial {
                // In partial mode, return success with zero files extracted
                log::warn!("No chunks could be recovered (partial mode)");
                return Ok(ExtractionMetadata {
                    chunks_total: index.chunks.len(),
                    chunks_recovered: 0,
                    chunks_failed: chunks_failed.len(),
                    files_extracted: 0,
                });
            }
            return Err(EctarError::ErasureCoding(
                "No chunks could be recovered".to_string(),
            ));
        }

        if !chunks_failed.is_empty() && !self.partial {
            return Err(EctarError::ErasureCoding(format!(
                "Failed to recover {} chunks: {:?}",
                chunks_failed.len(),
                chunks_failed
            )));
        }

        log::info!(
            "Recovered {}/{} chunks",
            chunks_recovered,
            index.chunks.len()
        );

        // Concatenate and extract tar stream from all reconstructed chunks
        log::info!("Extracting files from reconstructed archive...");

        let files_extracted = self.extract_all_chunks(&temp_dir, &index, &chunks_failed, self.partial)?;

        Ok(ExtractionMetadata {
            chunks_total: index.chunks.len(),
            chunks_recovered,
            chunks_failed: chunks_failed.len(),
            files_extracted,
        })
    }

    /// Extract archive from shards only (no index file)
    /// Uses zfec headers from shards to determine parameters
    fn extract_from_shards_only(&self) -> Result<ExtractionMetadata> {
        // Discover available shards
        let shards_by_chunk = shard_reader::discover_shards(&self.shard_pattern)?;

        if shards_by_chunk.is_empty() {
            return Err(EctarError::ErasureCoding(
                "No shards found".to_string(),
            ));
        }

        log::info!("Found {} chunks from shard files", shards_by_chunk.len());

        // Read zfec header from first available shard to get k, m parameters
        let (data_shards, parity_shards) = {
            let first_chunk_shards = shards_by_chunk.values().next()
                .ok_or_else(|| EctarError::ErasureCoding("No shards available".to_string()))?;

            if first_chunk_shards.is_empty() {
                return Err(EctarError::ErasureCoding("No shards in first chunk".to_string()));
            }

            // Check for zfec header
            let first_shard = &first_chunk_shards[0];
            if let Some(ref header) = first_shard.header {
                let k = header.k as usize;
                let m = header.m as usize;
                log::info!("Detected erasure coding parameters from zfec header: k={}, m={}", k, m);
                log::info!("Note: Padding info from headers will be used to trim reconstructed chunks");
                (k, m - k)
            } else {
                return Err(EctarError::InvalidHeader(
                    "No zfec header found in shards - cannot extract without index file".to_string(),
                ));
            }
        };

        // Create temporary directory for reconstructed chunks
        let temp_dir = TempDir::new()?;

        // Reconstruct each chunk
        let mut chunks_recovered = 0;
        let mut chunks_failed = Vec::new();
        let chunks_total = shards_by_chunk.len();

        // Sort chunk numbers for consistent ordering
        let mut chunk_numbers: Vec<usize> = shards_by_chunk.keys().copied().collect();
        chunk_numbers.sort();

        for chunk_num in &chunk_numbers {
            match shards_by_chunk.get(chunk_num) {
                Some(shards) => {
                    if shards.len() < data_shards {
                        log::error!(
                            "Chunk {}: insufficient shards ({}/{})",
                            chunk_num,
                            shards.len(),
                            data_shards
                        );
                        chunks_failed.push(*chunk_num);
                        continue;
                    }

                    // Calculate compressed_size from zfec header padlen
                    let compressed_size = if let Some(ref header) = shards[0].header {
                        // shard_size * data_shards - padlen = actual compressed size
                        let shard_size = shards[0].data.len();
                        let total_size = shard_size * data_shards;
                        let actual_size = total_size - header.padlen;
                        log::debug!(
                            "Chunk {}: calculated compressed_size={} (shard_size={}, padlen={})",
                            chunk_num, actual_size, shard_size, header.padlen
                        );
                        Some(actual_size as u64)
                    } else {
                        None
                    };

                    // Reconstruct chunk
                    let chunk_path = temp_dir.path().join(format!("chunk{:03}.tar.zst", chunk_num));

                    match decoder::decode_chunk(
                        shards.clone(),
                        data_shards,
                        parity_shards,
                        &chunk_path,
                        compressed_size,
                    ) {
                        Ok(_) => {
                            log::info!("Chunk {} reconstructed successfully", chunk_num);
                            chunks_recovered += 1;
                        }
                        Err(e) => {
                            log::error!("Failed to reconstruct chunk {}: {}", chunk_num, e);
                            chunks_failed.push(*chunk_num);
                        }
                    }
                }
                None => {
                    log::error!("Chunk {}: no shards found", chunk_num);
                    chunks_failed.push(*chunk_num);
                }
            }
        }

        if chunks_recovered == 0 {
            return Err(EctarError::ErasureCoding(
                "No chunks could be recovered".to_string(),
            ));
        }

        log::info!(
            "Recovered {}/{} chunks",
            chunks_recovered,
            chunks_total
        );

        // Extract all chunks without index (no file filtering available)
        log::info!("Extracting files from reconstructed archive (no filtering)...");

        let files_extracted = self.extract_chunks_no_index(&temp_dir, &chunk_numbers, &chunks_failed)?;

        Ok(ExtractionMetadata {
            chunks_total,
            chunks_recovered,
            chunks_failed: chunks_failed.len(),
            files_extracted,
        })
    }

    fn read_index(&self, index_path: &Path) -> Result<ArchiveIndex> {
        let index_file = File::open(index_path)?;
        let mut decoder = compression::create_decoder(index_file)?;

        let mut json = String::new();
        std::io::Read::read_to_string(&mut decoder, &mut json)?;

        let index: ArchiveIndex = serde_json::from_str(&json)?;

        Ok(index)
    }

    fn extract_all_chunks(
        &self,
        temp_dir: &TempDir,
        index: &ArchiveIndex,
        chunks_failed: &[usize],
        partial: bool,
    ) -> Result<usize> {

        // Ensure output directory exists
        std::fs::create_dir_all(&self.output_dir)?;

        // Create a temporary file to hold the concatenated decompressed tar stream
        let concat_path = temp_dir.path().join("combined.tar");
        let mut concat_file = File::create(&concat_path)?;

        // Decompress and concatenate all chunks in order
        // Sort by chunk number to ensure correct ordering
        let mut chunk_numbers: Vec<usize> = index.chunks.iter()
            .map(|c| c.chunk_number)
            .collect();
        chunk_numbers.sort();

        for chunk_num in chunk_numbers {
            if chunks_failed.contains(&chunk_num) {
                log::warn!("Skipping failed chunk {} during extraction", chunk_num);
                continue;
            }

            let chunk_path = temp_dir.path().join(format!("chunk{:03}.tar.zst", chunk_num));

            if !chunk_path.exists() {
                continue;
            }

            log::debug!("Decompressing chunk {}...", chunk_num);

            // Decompress chunk and append to concatenated tar
            let chunk_file = File::open(&chunk_path)?;
            let mut decoder = compression::create_decoder(chunk_file)?;

            std::io::copy(&mut decoder, &mut concat_file)?;
        }

        concat_file.flush()?;
        drop(concat_file);

        // Extract from the concatenated tar file
        log::info!("Extracting tar archive...");
        let concat_file = File::open(&concat_path)?;
        let mut archive = tar::Archive::new(concat_file);

        // Unpack and count entries
        let mut file_count = 0;

        let entries_result = archive.entries();
        if let Err(e) = entries_result {
            if partial {
                log::warn!("Failed to read tar entries (partial mode): {}", e);
                return Ok(file_count);
            } else {
                return Err(EctarError::Tar(format!("Failed to read tar entries: {}", e)));
            }
        }

        for entry in entries_result.unwrap() {
            let mut entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    if partial {
                        log::warn!("Failed to read tar entry (partial mode): {}", e);
                        break; // Stop processing entries when we hit corruption
                    } else {
                        return Err(EctarError::Tar(format!("Failed to read entry: {}", e)));
                    }
                }
            };

            let path = match entry.path() {
                Ok(p) => p.to_path_buf(),
                Err(e) => {
                    if partial {
                        log::warn!("Failed to read entry path (partial mode): {}", e);
                        continue;
                    } else {
                        return Err(EctarError::Tar(format!("Failed to read entry path: {}", e)));
                    }
                }
            };

            let path_str = path.to_string_lossy();

            // Check file filters (if specified, only extract matching files)
            if !self.file_filters.is_empty() {
                let matches = self.file_filters.iter().any(|f| {
                    path_str.contains(f) || glob::Pattern::new(f)
                        .map(|p| p.matches(&path_str))
                        .unwrap_or(false)
                });
                if !matches {
                    log::debug!("Skipping {} (not in file filter)", path.display());
                    continue;
                }
            }

            // Check exclude patterns
            if self.exclude_patterns.iter().any(|p| {
                path_str.contains(p) || glob::Pattern::new(p)
                    .map(|pat| pat.matches(&path_str))
                    .unwrap_or(false)
            }) {
                log::debug!("Skipping {} (excluded)", path.display());
                continue;
            }

            // Apply strip_components
            let stripped_path = if self.strip_components > 0 {
                let components: Vec<_> = path.components().collect();
                if components.len() <= self.strip_components {
                    log::debug!("Skipping {} (not enough path components to strip)", path.display());
                    continue;
                }
                components[self.strip_components..].iter().collect::<PathBuf>()
            } else {
                path.clone()
            };

            log::debug!("Extracting: {} -> {}", path.display(), stripped_path.display());

            let output_path = self.output_dir.join(&stripped_path);

            // Create parent directories if needed
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            if let Err(e) = entry.unpack(&output_path) {
                if partial {
                    log::warn!("Failed to unpack {} (partial mode): {}", path.display(), e);
                    continue;
                } else {
                    return Err(EctarError::Tar(format!("Failed to unpack {}: {}", path.display(), e)));
                }
            }

            file_count += 1;
        }

        log::info!("Extracted {} entries", file_count);

        Ok(file_count)
    }

    /// Extract chunks without using index (no file filtering, simpler extraction)
    fn extract_chunks_no_index(
        &self,
        temp_dir: &TempDir,
        chunk_numbers: &[usize],
        chunks_failed: &[usize],
    ) -> Result<usize> {
        // Ensure output directory exists
        std::fs::create_dir_all(&self.output_dir)?;

        // Create a temporary file to hold the concatenated decompressed tar stream
        let concat_path = temp_dir.path().join("combined.tar");
        let mut concat_file = File::create(&concat_path)?;

        // Decompress and concatenate all chunks in order
        for chunk_num in chunk_numbers {
            if chunks_failed.contains(chunk_num) {
                log::warn!("Skipping failed chunk {} during extraction", chunk_num);
                continue;
            }

            let chunk_path = temp_dir.path().join(format!("chunk{:03}.tar.zst", chunk_num));

            if !chunk_path.exists() {
                continue;
            }

            log::debug!("Decompressing chunk {}...", chunk_num);

            // Decompress chunk and append to concatenated tar
            let chunk_file = File::open(&chunk_path)?;
            let mut decoder = compression::create_decoder(chunk_file)?;

            std::io::copy(&mut decoder, &mut concat_file)?;
        }

        concat_file.flush()?;
        drop(concat_file);

        // Extract from the concatenated tar file
        log::info!("Extracting tar archive...");
        let concat_file = File::open(&concat_path)?;
        let mut archive = tar::Archive::new(concat_file);

        // Unpack all entries (no filtering)
        let mut file_count = 0;

        for entry in archive.entries()? {
            let mut entry = entry.map_err(|e| EctarError::Tar(e.to_string()))?;

            let path = entry.path()
                .map_err(|e| EctarError::Tar(e.to_string()))?
                .to_path_buf();

            log::debug!("Extracting: {}", path.display());

            let output_path = self.output_dir.join(&path);

            // Create parent directories if needed
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            entry.unpack(&output_path)
                .map_err(|e| EctarError::Tar(format!("Failed to unpack {}: {}", path.display(), e)))?;

            file_count += 1;
        }

        log::info!("Extracted {} entries", file_count);

        Ok(file_count)
    }
}

pub struct ExtractionMetadata {
    pub chunks_total: usize,
    pub chunks_recovered: usize,
    pub chunks_failed: usize,
    pub files_extracted: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::create::ArchiveBuilder;
    use std::fs::{self, File};
    use std::io::Write as IoWriteTrait;
    use tempfile::TempDir;

    fn create_test_archive(temp_dir: &TempDir, content: &[u8]) -> String {
        let test_file = temp_dir.path().join("test.txt");
        let mut file = File::create(&test_file).unwrap();
        file.write_all(content).unwrap();
        drop(file);

        let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
        let builder = ArchiveBuilder::new(archive_base.clone())
            .data_shards(4)
            .parity_shards(2)
            .chunk_size(Some(1024 * 1024));

        builder.create(&[test_file]).unwrap();
        archive_base
    }

    fn create_multi_file_archive(temp_dir: &TempDir) -> String {
        let test_dir = temp_dir.path().join("testdata");
        fs::create_dir(&test_dir).unwrap();

        for i in 1..=3 {
            let file = test_dir.join(format!("file{}.txt", i));
            let mut f = File::create(&file).unwrap();
            f.write_all(format!("Content of file {}", i).as_bytes()).unwrap();
            drop(f);
        }

        let subdir = test_dir.join("subdir");
        fs::create_dir(&subdir).unwrap();
        let subfile = subdir.join("nested.txt");
        let mut f = File::create(&subfile).unwrap();
        f.write_all(b"Nested file content").unwrap();
        drop(f);

        let archive_base = temp_dir.path().join("archive").to_string_lossy().to_string();
        let builder = ArchiveBuilder::new(archive_base.clone())
            .data_shards(4)
            .parity_shards(2)
            .chunk_size(Some(1024 * 1024));

        builder.create(&[test_dir]).unwrap();
        archive_base
    }

    #[test]
    fn test_extractor_new() {
        let extractor = ArchiveExtractor::new("pattern".to_string(), None);
        assert_eq!(extractor.shard_pattern, "pattern");
        assert_eq!(extractor.output_dir, PathBuf::from("."));
        assert!(extractor.verify_checksums);
        assert!(!extractor.partial);
        assert!(extractor.file_filters.is_empty());
        assert!(extractor.exclude_patterns.is_empty());
        assert_eq!(extractor.strip_components, 0);
    }

    #[test]
    fn test_extractor_with_output_dir() {
        let extractor = ArchiveExtractor::new("pattern".to_string(), Some(PathBuf::from("/output")));
        assert_eq!(extractor.output_dir, PathBuf::from("/output"));
    }

    #[test]
    fn test_verify_checksums() {
        let extractor = ArchiveExtractor::new("pattern".to_string(), None)
            .verify_checksums(false);
        assert!(!extractor.verify_checksums);
    }

    #[test]
    fn test_partial() {
        let extractor = ArchiveExtractor::new("pattern".to_string(), None)
            .partial(true);
        assert!(extractor.partial);
    }

    #[test]
    fn test_file_filters() {
        let extractor = ArchiveExtractor::new("pattern".to_string(), None)
            .file_filters(vec!["*.txt".to_string()]);
        assert_eq!(extractor.file_filters.len(), 1);
    }

    #[test]
    fn test_exclude_patterns() {
        let extractor = ArchiveExtractor::new("pattern".to_string(), None)
            .exclude_patterns(vec!["*.log".to_string()]);
        assert_eq!(extractor.exclude_patterns.len(), 1);
    }

    #[test]
    fn test_strip_components() {
        let extractor = ArchiveExtractor::new("pattern".to_string(), None)
            .strip_components(2);
        assert_eq!(extractor.strip_components, 2);
    }

    #[test]
    fn test_extract_basic() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive(&temp_dir, b"Test content");

        let extract_dir = temp_dir.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();

        let pattern = format!("{}.c*.s*", archive_base);
        let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()));
        let metadata = extractor.extract().unwrap();

        assert_eq!(metadata.chunks_recovered, 1);
        assert!(metadata.files_extracted >= 1);
    }

    #[test]
    fn test_extract_with_file_filter() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_multi_file_archive(&temp_dir);

        let extract_dir = temp_dir.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();

        let pattern = format!("{}.c*.s*", archive_base);
        let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()))
            .file_filters(vec!["file1".to_string()]);
        let metadata = extractor.extract().unwrap();

        assert!(metadata.files_extracted >= 1);
    }

    #[test]
    fn test_extract_with_exclude() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_multi_file_archive(&temp_dir);

        let extract_dir = temp_dir.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();

        let pattern = format!("{}.c*.s*", archive_base);
        let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()))
            .exclude_patterns(vec!["file1".to_string()]);
        let metadata = extractor.extract().unwrap();

        // Should have extracted some files, but not file1.txt
        assert!(metadata.files_extracted >= 1);
    }

    #[test]
    fn test_extract_with_strip_components() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_multi_file_archive(&temp_dir);

        let extract_dir = temp_dir.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();

        let pattern = format!("{}.c*.s*", archive_base);
        let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()))
            .strip_components(1);
        let metadata = extractor.extract().unwrap();

        assert!(metadata.files_extracted >= 1);
        // With strip_components=1, the "testdata" directory prefix should be stripped
    }

    #[test]
    fn test_extract_missing_index() {
        let temp_dir = TempDir::new().unwrap();
        let pattern = temp_dir.path().join("nonexistent.c*.s*").to_string_lossy().to_string();

        let extractor = ArchiveExtractor::new(pattern, None);
        let result = extractor.extract();
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_partial_mode_no_chunks() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive(&temp_dir, b"Test content");

        // Delete all shards to make archive unrecoverable
        for i in 0..6 {
            let shard_path = temp_dir.path().join(format!("archive.c001.s{:02}", i));
            let _ = fs::remove_file(shard_path);
        }

        let extract_dir = temp_dir.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();

        let pattern = format!("{}.c*.s*", archive_base);
        let extractor = ArchiveExtractor::new(pattern, Some(extract_dir))
            .partial(true);
        let metadata = extractor.extract().unwrap();

        // In partial mode, should succeed but with no files extracted
        assert_eq!(metadata.chunks_recovered, 0);
        assert_eq!(metadata.files_extracted, 0);
    }

    #[test]
    fn test_extract_no_chunks_recovered_error() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive(&temp_dir, b"Test content");

        // Delete all shards
        for i in 0..6 {
            let shard_path = temp_dir.path().join(format!("archive.c001.s{:02}", i));
            let _ = fs::remove_file(shard_path);
        }

        let extract_dir = temp_dir.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();

        let pattern = format!("{}.c*.s*", archive_base);
        let extractor = ArchiveExtractor::new(pattern, Some(extract_dir))
            .partial(false); // Not partial mode
        let result = extractor.extract();

        assert!(result.is_err());
    }

    #[test]
    fn test_extract_insufficient_shards_non_partial() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive(&temp_dir, b"Test content");

        // Delete 3 shards (need 4 data shards to recover)
        for i in 0..3 {
            let shard_path = temp_dir.path().join(format!("archive.c001.s{:02}", i));
            let _ = fs::remove_file(shard_path);
        }

        let extract_dir = temp_dir.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();

        let pattern = format!("{}.c*.s*", archive_base);
        let extractor = ArchiveExtractor::new(pattern, Some(extract_dir))
            .partial(false);
        let result = extractor.extract();

        assert!(result.is_err());
    }

    #[test]
    fn test_extract_glob_filter() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_multi_file_archive(&temp_dir);

        let extract_dir = temp_dir.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();

        let pattern = format!("{}.c*.s*", archive_base);
        let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()))
            .file_filters(vec!["*.txt".to_string()]);
        let metadata = extractor.extract().unwrap();

        assert!(metadata.files_extracted >= 1);
    }

    #[test]
    fn test_extract_glob_exclude() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_multi_file_archive(&temp_dir);

        let extract_dir = temp_dir.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();

        let pattern = format!("{}.c*.s*", archive_base);
        let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()))
            .exclude_patterns(vec!["nested*".to_string()]);
        let metadata = extractor.extract().unwrap();

        assert!(metadata.files_extracted >= 1);
    }

    #[test]
    fn test_extract_with_verify_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive(&temp_dir, b"Test content");

        let extract_dir = temp_dir.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();

        let pattern = format!("{}.c*.s*", archive_base);
        let extractor = ArchiveExtractor::new(pattern, Some(extract_dir.clone()))
            .verify_checksums(false);
        let metadata = extractor.extract().unwrap();

        assert!(metadata.files_extracted >= 1);
    }

    #[test]
    fn test_extraction_metadata_fields() {
        let metadata = ExtractionMetadata {
            chunks_total: 5,
            chunks_recovered: 4,
            chunks_failed: 1,
            files_extracted: 10,
        };

        assert_eq!(metadata.chunks_total, 5);
        assert_eq!(metadata.chunks_recovered, 4);
        assert_eq!(metadata.chunks_failed, 1);
        assert_eq!(metadata.files_extracted, 10);
    }
}
