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
}

impl ArchiveExtractor {
    pub fn new(shard_pattern: String, output_dir: Option<PathBuf>) -> Self {
        Self {
            shard_pattern,
            output_dir: output_dir.unwrap_or_else(|| PathBuf::from(".")),
            verify_checksums: true,
            partial: false,
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

    pub fn extract(&self) -> Result<ExtractionMetadata> {
        log::info!("Extracting archive from pattern: {}", self.shard_pattern);

        // Read index file
        let index_path = shard_reader::find_index_file(&self.shard_pattern)
            .ok_or_else(|| EctarError::MissingIndex(PathBuf::from(&self.shard_pattern)))?;

        log::info!("Found index file: {}", index_path.display());

        let index = self.read_index(&index_path)?;

        log::info!("Archive: {}", index.archive_name);
        log::info!("  Data shards: {}", index.parameters.data_shards);
        log::info!("  Parity shards: {}", index.parameters.parity_shards);
        log::info!("  Chunks: {}", index.chunks.len());
        log::info!("  Files: {}", index.files.len());

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
        for chunk_num in 1..=index.chunks.len() {
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

            log::debug!("Extracting: {}", path.display());

            let output_path = self.output_dir.join(&path);
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
}

pub struct ExtractionMetadata {
    pub chunks_total: usize,
    pub chunks_recovered: usize,
    pub chunks_failed: usize,
    pub files_extracted: usize,
}
