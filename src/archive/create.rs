use crate::checksum;
use crate::compression;
use crate::error::{EctarError, Result};
use crate::index::format::{ArchiveIndex, ArchiveParameters, ChunkInfo, FileEntry, FileType};
use chrono::Utc;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct ArchiveBuilder {
    output_base: String,
    data_shards: usize,
    parity_shards: usize,
    chunk_size: Option<u64>,
    compression_level: i32,
    no_compression: bool,
    no_index: bool,
    exclude_patterns: Vec<String>,
    follow_symlinks: bool,
    preserve_permissions: bool,
}

impl ArchiveBuilder {
    pub fn new(output_base: String) -> Self {
        Self {
            output_base,
            data_shards: 10,
            parity_shards: 5,
            chunk_size: None,
            compression_level: compression::zstd::DEFAULT_COMPRESSION_LEVEL,
            no_compression: false,
            no_index: false,
            exclude_patterns: Vec::new(),
            follow_symlinks: false,
            preserve_permissions: true,
        }
    }

    pub fn data_shards(mut self, n: usize) -> Self {
        self.data_shards = n;
        self
    }

    pub fn parity_shards(mut self, n: usize) -> Self {
        self.parity_shards = n;
        self
    }

    pub fn chunk_size(mut self, size: Option<u64>) -> Self {
        self.chunk_size = size;
        self
    }

    pub fn compression_level(mut self, level: i32) -> Self {
        self.compression_level = level;
        self
    }

    pub fn no_compression(mut self, no_comp: bool) -> Self {
        self.no_compression = no_comp;
        self
    }

    pub fn no_index(mut self, no_idx: bool) -> Self {
        self.no_index = no_idx;
        self
    }

    pub fn exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.exclude_patterns = patterns;
        self
    }

    pub fn follow_symlinks(mut self, follow: bool) -> Self {
        self.follow_symlinks = follow;
        self
    }

    pub fn preserve_permissions(mut self, preserve: bool) -> Self {
        self.preserve_permissions = preserve;
        self
    }

    /// Validate parameters before creating archive
    pub fn validate(&self) -> Result<()> {
        if self.data_shards < 1 {
            return Err(EctarError::InvalidParameters(
                "Data shards must be at least 1".to_string(),
            ));
        }

        if self.parity_shards < 1 {
            return Err(EctarError::InvalidParameters(
                "Parity shards must be at least 1".to_string(),
            ));
        }

        if self.data_shards + self.parity_shards > 256 {
            return Err(EctarError::InvalidParameters(
                "Total shards (data + parity) cannot exceed 256".to_string(),
            ));
        }

        if !self.no_compression {
            compression::zstd::validate_compression_level(self.compression_level)?;
        }

        Ok(())
    }

    /// Create the archive from the given paths
    pub fn create(&self, paths: &[PathBuf]) -> Result<ArchiveMetadata> {
        self.validate()?;

        log::info!("Creating archive: {}", self.output_base);
        log::info!("  Data shards: {}", self.data_shards);
        log::info!("  Parity shards: {}", self.parity_shards);
        log::info!("  Compression level: {}", self.compression_level);
        if let Some(cs) = self.chunk_size {
            log::info!("  Chunk size: {} bytes", cs);
        }
        log::info!("  Paths: {} items", paths.len());

        // Collect all files to archive
        let files_to_archive = self.collect_files(paths)?;
        log::info!("Collected {} files to archive", files_to_archive.len());

        // Choose between chunked and non-chunked creation
        if let Some(chunk_size) = self.chunk_size {
            self.create_chunked(paths, &files_to_archive, chunk_size)
        } else {
            self.create_single(paths, &files_to_archive)
        }
    }

    /// Create a non-chunked archive
    fn create_single(&self, paths: &[PathBuf], files_to_archive: &[PathBuf]) -> Result<ArchiveMetadata> {
        let archive_path = format!("{}.tar.zst", self.output_base);
        let mut file_entries = Vec::new();

        // Create tar builder
        let output_file = File::create(&archive_path)?;

        if self.no_compression {
            let mut writer = output_file;
            let mut tar_builder = tar::Builder::new(&mut writer);
            self.add_files_to_tar(&mut tar_builder, &files_to_archive, paths, &mut file_entries, 1)?;
            tar_builder.finish()?;
        } else {
            let mut encoder = compression::zstd::create_encoder(
                output_file,
                self.compression_level,
            )?;
            {
                let mut tar_builder = tar::Builder::new(&mut encoder);
                self.add_files_to_tar(&mut tar_builder, &files_to_archive, paths, &mut file_entries, 1)?;
                tar_builder.finish()?;
            }
            encoder.finish()?;
        }

        // Get final file size
        let final_size = std::fs::metadata(&archive_path)?.len();

        // Create index if requested
        if !self.no_index {
            let chunks_info = vec![crate::chunking::ChunkInfo {
                chunk_number: 1,
                compressed_size: final_size,
                uncompressed_size: file_entries.iter().map(|e| e.size).sum(),
            }];
            self.create_index(&file_entries, &chunks_info)?;
        }

        Ok(ArchiveMetadata {
            total_files: files_to_archive.len(),
            total_size: file_entries.iter().map(|e| e.size).sum(),
            compressed_size: final_size,
            chunks: 1,
        })
    }

    /// Create a chunked archive with independent compression per chunk
    fn create_chunked(&self, paths: &[PathBuf], files_to_archive: &[PathBuf], chunk_size: u64) -> Result<ArchiveMetadata> {
        use crate::chunking::StreamingErasureChunkingWriter;

        let mut file_entries = Vec::new();

        // Create streaming erasure chunking writer
        let output_base = PathBuf::from(&self.output_base);
        let mut chunking_writer = StreamingErasureChunkingWriter::new(
            output_base,
            chunk_size,
            self.compression_level,
            self.data_shards,
            self.parity_shards,
        )
        .no_compression(self.no_compression);

        // Determine base path for making relative paths
        let base_path = if paths.len() == 1 && paths[0].is_dir() {
            paths[0].parent().unwrap_or(&paths[0])
        } else {
            Path::new("")
        };

        // Create tar builder on top of chunking writer and add files
        {
            let mut tar_builder = tar::Builder::new(&mut chunking_writer);

            // Add files to tar archive, tracking which chunk each file is in
            for file_path in files_to_archive {
                // Get chunk number before adding file
                let chunk_number = tar_builder.get_ref().current_chunk_number();

                log::debug!("Adding file to chunk {}: {}", chunk_number, file_path.display());

                let metadata = std::fs::symlink_metadata(file_path)?;
                let file_type = self.classify_file_type(&metadata);

                // Make path relative for tar (tar requires relative paths)
                let tar_path = if base_path.as_os_str().is_empty() {
                    // No base path - use just the filename to ensure relative path
                    file_path.file_name()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| file_path.clone())
                } else {
                    file_path.strip_prefix(base_path).unwrap_or(file_path).to_path_buf()
                };

                // Add to tar
                if metadata.is_file() {
                    let mut file = File::open(file_path)?;
                    tar_builder.append_file(&tar_path, &mut file)?;
                } else if metadata.is_dir() {
                    tar_builder.append_dir(&tar_path, file_path)?;
                } else if metadata.is_symlink() {
                    let mut header = tar::Header::new_gnu();
                    header.set_metadata(&metadata);
                    header.set_entry_type(tar::EntryType::Symlink);
                    let target = std::fs::read_link(file_path)?;
                    header.set_link_name(&target)?;
                    header.set_size(0);
                    header.set_cksum();
                    tar_builder.append_data(&mut header, &tar_path, std::io::empty())?;
                }

                // Flush to ensure we get accurate chunk tracking
                tar_builder.get_mut().flush()?;

                // Get chunk number after writing (file might have crossed chunk boundary)
                let final_chunk = tar_builder.get_ref().current_chunk_number();

                // Compute checksum for regular files
                let checksum = if metadata.is_file() {
                    let file = File::open(file_path)?;
                    Some(checksum::sha256::compute_checksum(file)?)
                } else {
                    None
                };

                // Create file entry for index
                let entry = FileEntry {
                    path: tar_path.to_string_lossy().to_string(),
                    chunk: chunk_number,
                    offset: 0,
                    size: metadata.len(),
                    compressed_size: None,
                    checksum,
                    mode: Self::get_file_mode(&metadata),
                    mtime: Self::get_mtime(&metadata),
                    uid: Self::get_uid(&metadata),
                    gid: Self::get_gid(&metadata),
                    user: None,
                    group: None,
                    entry_type: file_type,
                    target: if metadata.is_symlink() {
                        Some(std::fs::read_link(file_path)?.to_string_lossy().to_string())
                    } else {
                        None
                    },
                    spans_chunks: if final_chunk != chunk_number {
                        Some((chunk_number..=final_chunk).collect())
                    } else {
                        None
                    },
                };

                file_entries.push(entry);
            }

            tar_builder.finish()?;
        }

        // Finish chunking and get chunk metadata (shards already written!)
        let chunks_info = chunking_writer.finish()?;

        log::info!("Created {} chunks with {} shards each", chunks_info.len(), self.data_shards + self.parity_shards);

        // Create index if requested
        if !self.no_index {
            self.create_index_from_streaming(&file_entries, &chunks_info)?;
        }

        let total_uncompressed: u64 = chunks_info.iter().map(|c| c.uncompressed_size).sum();
        // Total shard size = sum of (shard_size * number of shards) for each chunk
        let total_shard_size: u64 = chunks_info.iter()
            .map(|c| c.shard_size * (self.data_shards + self.parity_shards) as u64)
            .sum();

        Ok(ArchiveMetadata {
            total_files: files_to_archive.len(),
            total_size: total_uncompressed,
            compressed_size: total_shard_size, // Report shard size instead of compressed
            chunks: chunks_info.len(),
        })
    }

    /// Add files to tar archive (non-chunked)
    fn add_files_to_tar<W: Write>(
        &self,
        tar_builder: &mut tar::Builder<W>,
        files_to_archive: &[PathBuf],
        paths: &[PathBuf],
        file_entries: &mut Vec<FileEntry>,
        chunk_number: usize,
    ) -> Result<()> {
        // Determine base path for making relative paths
        let base_path = if paths.len() == 1 && paths[0].is_dir() {
            paths[0].parent().unwrap_or(&paths[0])
        } else {
            Path::new("")
        };

        // Add files to tar archive
        for file_path in files_to_archive {
            log::debug!("Adding file: {}", file_path.display());

            let metadata = std::fs::symlink_metadata(file_path)?;
            let file_type = self.classify_file_type(&metadata);

            // Make path relative for tar (tar requires relative paths)
            let tar_path = if base_path.as_os_str().is_empty() {
                // No base path - use just the filename to ensure relative path
                file_path.file_name()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| file_path.clone())
            } else {
                file_path.strip_prefix(base_path).unwrap_or(file_path).to_path_buf()
            };

            // Add to tar
            if metadata.is_file() {
                let mut file = File::open(file_path)?;
                tar_builder.append_file(&tar_path, &mut file)?;
            } else if metadata.is_dir() {
                tar_builder.append_dir(&tar_path, file_path)?;
            } else if metadata.is_symlink() {
                // For symlinks, we need to use append_path with proper header
                let mut header = tar::Header::new_gnu();
                header.set_metadata(&metadata);
                header.set_entry_type(tar::EntryType::Symlink);
                let target = std::fs::read_link(file_path)?;
                header.set_link_name(&target)?;
                header.set_size(0);
                header.set_cksum();
                tar_builder.append_data(&mut header, &tar_path, std::io::empty())?;
            }

            // Compute checksum for regular files
            let checksum = if metadata.is_file() {
                let file = File::open(file_path)?;
                Some(checksum::sha256::compute_checksum(file)?)
            } else {
                None
            };

            // Create file entry for index
            let entry = FileEntry {
                path: tar_path.to_string_lossy().to_string(),
                chunk: chunk_number,
                offset: 0, // TODO: Track actual offset
                size: metadata.len(),
                compressed_size: None,
                checksum,
                mode: Self::get_file_mode(&metadata),
                mtime: Self::get_mtime(&metadata),
                uid: Self::get_uid(&metadata),
                gid: Self::get_gid(&metadata),
                user: None,
                group: None,
                entry_type: file_type,
                target: if metadata.is_symlink() {
                    Some(std::fs::read_link(file_path)?.to_string_lossy().to_string())
                } else {
                    None
                },
                spans_chunks: None,
            };

            file_entries.push(entry);
        }

        Ok(())
    }

    /// Collect all files to be archived
    fn collect_files(&self, paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for path in paths {
            if !path.exists() {
                return Err(EctarError::Io(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Path not found: {}", path.display()),
                )));
            }

            if path.is_file() {
                if !self.is_excluded(path) {
                    files.push(path.clone());
                }
            } else if path.is_dir() {
                let walker = WalkDir::new(path)
                    .follow_links(self.follow_symlinks)
                    .into_iter()
                    .filter_entry(|e| !self.is_excluded(e.path()));

                for entry in walker {
                    let entry = entry.map_err(|e| {
                        EctarError::Io(io::Error::new(io::ErrorKind::Other, e.to_string()))
                    })?;

                    if !self.is_excluded(entry.path()) {
                        files.push(entry.path().to_path_buf());
                    }
                }
            }
        }

        Ok(files)
    }

    /// Check if a path should be excluded
    fn is_excluded(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        for pattern in &self.exclude_patterns {
            if path_str.contains(pattern) {
                return true;
            }
        }

        false
    }

    /// Classify file type from metadata
    fn classify_file_type(&self, metadata: &std::fs::Metadata) -> FileType {
        use std::os::unix::fs::FileTypeExt;

        let file_type = metadata.file_type();

        if file_type.is_file() {
            FileType::File
        } else if file_type.is_dir() {
            FileType::Directory
        } else if file_type.is_symlink() {
            FileType::Symlink
        } else {
            FileType::Other
        }
    }

    /// Get file mode from metadata
    #[cfg(unix)]
    fn get_file_mode(metadata: &std::fs::Metadata) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode()
    }

    #[cfg(not(unix))]
    fn get_file_mode(_metadata: &std::fs::Metadata) -> u32 {
        0o644 // Default permissions on non-Unix
    }

    /// Get modification time from metadata
    fn get_mtime(metadata: &std::fs::Metadata) -> chrono::DateTime<Utc> {
        metadata
            .modified()
            .ok()
            .and_then(|t| chrono::DateTime::from_timestamp(
                t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64,
                0,
            ))
            .unwrap_or_else(Utc::now)
    }

    /// Get UID from metadata
    #[cfg(unix)]
    fn get_uid(metadata: &std::fs::Metadata) -> Option<u64> {
        use std::os::unix::fs::MetadataExt;
        Some(metadata.uid() as u64)
    }

    #[cfg(not(unix))]
    fn get_uid(_metadata: &std::fs::Metadata) -> Option<u64> {
        None
    }

    /// Get GID from metadata
    #[cfg(unix)]
    fn get_gid(metadata: &std::fs::Metadata) -> Option<u64> {
        use std::os::unix::fs::MetadataExt;
        Some(metadata.gid() as u64)
    }

    #[cfg(not(unix))]
    fn get_gid(_metadata: &std::fs::Metadata) -> Option<u64> {
        None
    }

    /// Create the index file (for non-chunked archives)
    fn create_index(&self, file_entries: &[FileEntry], chunks_info: &[crate::chunking::ChunkInfo]) -> Result<()> {
        // Convert chunking::ChunkInfo to index::format::ChunkInfo
        let chunks = chunks_info
            .iter()
            .map(|c| ChunkInfo {
                chunk_number: c.chunk_number,
                compressed_size: c.compressed_size,
                uncompressed_size: c.uncompressed_size,
                shard_size: 0, // Non-chunked archives don't use erasure coding
                checksum: String::new(),
            })
            .collect();

        self.write_index(file_entries, chunks)
    }

    /// Create the index file from streaming chunk info (per-chunk shard sizes)
    fn create_index_from_streaming(
        &self,
        file_entries: &[FileEntry],
        chunks_info: &[crate::chunking::streaming_erasure_chunker::ChunkInfo],
    ) -> Result<()> {
        // Convert streaming ChunkInfo to index::format::ChunkInfo with per-chunk shard sizes
        let chunks = chunks_info
            .iter()
            .map(|c| ChunkInfo {
                chunk_number: c.chunk_number,
                compressed_size: c.compressed_size,
                uncompressed_size: c.uncompressed_size,
                shard_size: c.shard_size,
                checksum: String::new(), // TODO: Compute chunk checksum
            })
            .collect();

        self.write_index(file_entries, chunks)
    }

    /// Write the index file
    fn write_index(
        &self,
        file_entries: &[FileEntry],
        chunks: Vec<ChunkInfo>,
    ) -> Result<()> {

        let index = ArchiveIndex {
            version: "1.0".to_string(),
            created: Utc::now(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            archive_name: self.output_base.clone(),
            parameters: ArchiveParameters {
                data_shards: self.data_shards,
                parity_shards: self.parity_shards,
                chunk_size: self.chunk_size,
                compression_level: self.compression_level,
            },
            chunks,
            files: file_entries.to_vec(),
        };

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&index)?;

        // Write compressed index
        let index_path = format!("{}.index.zst", self.output_base);
        let index_file = File::create(&index_path)?;
        compression::compress(json.as_bytes(), index_file, 19)?;

        log::info!("Created index file: {}", index_path);

        Ok(())
    }
}

pub struct ArchiveMetadata {
    pub total_files: usize,
    pub total_size: u64,
    pub compressed_size: u64,
    pub chunks: usize,
}
