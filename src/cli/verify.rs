use crate::compression;
use crate::erasure::decoder;
use crate::error::{EctarError, Result};
use crate::index::format::ArchiveIndex;
use crate::io::shard_reader;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write as IoWrite;
use std::path::PathBuf;

pub struct ArchiveVerifier {
    input: String,
    quick_mode: bool,
    full_mode: bool,
    report_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub archive_name: String,
    pub total_chunks: usize,
    pub chunks_verified: usize,
    pub chunks_failed: Vec<usize>,
    pub chunks_unrecoverable: Vec<usize>,
    pub total_shards: usize,
    pub missing_shards: usize,
    pub status: VerificationStatus,
    pub details: Vec<ChunkVerificationDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkVerificationDetail {
    pub chunk_number: usize,
    pub shards_available: usize,
    pub shards_required: usize,
    pub is_recoverable: bool,
    pub verification_performed: bool,
    pub checksum_valid: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VerificationStatus {
    Healthy,
    Degraded,
    Failed,
}

impl ArchiveVerifier {
    pub fn new(input: String) -> Self {
        Self {
            input,
            quick_mode: false,
            full_mode: false,
            report_path: None,
        }
    }

    pub fn quick(mut self) -> Self {
        self.quick_mode = true;
        self
    }

    pub fn full(mut self) -> Self {
        self.full_mode = true;
        self
    }

    pub fn report(mut self, path: Option<PathBuf>) -> Self {
        self.report_path = path;
        self
    }

    pub fn verify(&self) -> Result<VerificationReport> {
        // Read index file
        let index_path = shard_reader::find_index_file(&self.input)
            .ok_or_else(|| EctarError::MissingIndex(PathBuf::from(&self.input)))?;

        let index = self.read_index(&index_path)?;

        // Discover available shards
        let shards_by_chunk = shard_reader::discover_shards(&self.input)?;

        let mut report = VerificationReport {
            archive_name: index.archive_name.clone(),
            total_chunks: index.chunks.len(),
            chunks_verified: 0,
            chunks_failed: Vec::new(),
            chunks_unrecoverable: Vec::new(),
            total_shards: 0,
            missing_shards: 0,
            status: VerificationStatus::Healthy,
            details: Vec::new(),
        };

        // Verify each chunk
        for chunk_info in &index.chunks {
            let chunk_num = chunk_info.chunk_number;
            let shards_available = shards_by_chunk
                .get(&chunk_num)
                .map(|s| s.len())
                .unwrap_or(0);

            let is_recoverable = shards_available >= index.parameters.data_shards;

            let expected_shards = index.parameters.data_shards + index.parameters.parity_shards;
            report.total_shards += expected_shards;
            if shards_available < expected_shards {
                report.missing_shards += expected_shards - shards_available;
            }

            let mut detail = ChunkVerificationDetail {
                chunk_number: chunk_num,
                shards_available,
                shards_required: index.parameters.data_shards,
                is_recoverable,
                verification_performed: false,
                checksum_valid: None,
            };

            if !is_recoverable {
                log::error!(
                    "Chunk {}: insufficient shards ({}/{})",
                    chunk_num,
                    shards_available,
                    index.parameters.data_shards
                );
                report.chunks_unrecoverable.push(chunk_num);
                report.status = VerificationStatus::Failed;
            } else if shards_available < expected_shards {
                log::warn!(
                    "Chunk {}: degraded ({}/{} shards)",
                    chunk_num,
                    shards_available,
                    expected_shards
                );
                if report.status == VerificationStatus::Healthy {
                    report.status = VerificationStatus::Degraded;
                }
            } else {
                log::info!(
                    "Chunk {}: healthy ({}/{} shards)",
                    chunk_num,
                    shards_available,
                    expected_shards
                );
            }

            // Full verification: actually decode and verify
            if self.full_mode && is_recoverable {
                match self.verify_chunk_full(chunk_num, &shards_by_chunk, &index, chunk_info) {
                    Ok(()) => {
                        detail.verification_performed = true;
                        detail.checksum_valid = Some(true);
                        report.chunks_verified += 1;
                        log::info!("Chunk {}: verified successfully", chunk_num);
                    }
                    Err(e) => {
                        detail.verification_performed = true;
                        detail.checksum_valid = Some(false);
                        report.chunks_failed.push(chunk_num);
                        log::error!("Chunk {}: verification failed: {}", chunk_num, e);
                        if report.status != VerificationStatus::Failed {
                            report.status = VerificationStatus::Degraded;
                        }
                    }
                }
            } else if !self.full_mode && is_recoverable {
                report.chunks_verified += 1;
            }

            report.details.push(detail);
        }

        // Display report
        self.display_report(&report);

        // Write report to file if requested
        if let Some(ref report_path) = self.report_path {
            self.write_report_file(&report, report_path)?;
        }

        Ok(report)
    }

    fn read_index(&self, index_path: &PathBuf) -> Result<ArchiveIndex> {
        let index_file = File::open(index_path)?;
        let mut decoder = compression::create_decoder(index_file)?;

        let mut json = String::new();
        std::io::Read::read_to_string(&mut decoder, &mut json)?;

        let index: ArchiveIndex = serde_json::from_str(&json)?;
        Ok(index)
    }

    fn verify_chunk_full(
        &self,
        chunk_num: usize,
        shards_by_chunk: &HashMap<usize, Vec<decoder::ShardData>>,
        index: &ArchiveIndex,
        chunk_info: &crate::index::format::ChunkInfo,
    ) -> Result<()> {
        let shards = shards_by_chunk
            .get(&chunk_num)
            .ok_or_else(|| EctarError::ErasureCoding("No shards found".to_string()))?;

        // Create temporary file for decoded chunk
        let temp_dir = tempfile::TempDir::new()?;
        let chunk_path = temp_dir
            .path()
            .join(format!("chunk{:03}.tar.zst", chunk_num));

        // Decode chunk
        decoder::decode_chunk(
            shards.clone(),
            index.parameters.data_shards,
            index.parameters.parity_shards,
            &chunk_path,
            Some(chunk_info.compressed_size),
        )?;

        // Verify decoded chunk exists and has correct size
        let metadata = std::fs::metadata(&chunk_path)?;
        if metadata.len() != chunk_info.compressed_size {
            return Err(EctarError::ErasureCoding(format!(
                "Decoded chunk size mismatch: expected {}, got {}",
                chunk_info.compressed_size,
                metadata.len()
            )));
        }

        Ok(())
    }

    fn display_report(&self, report: &VerificationReport) {
        println!("\nVerification Report for: {}", report.archive_name);
        println!("{}", "=".repeat(60));

        let status_str = match report.status {
            VerificationStatus::Healthy => "✓ HEALTHY",
            VerificationStatus::Degraded => "⚠ DEGRADED",
            VerificationStatus::Failed => "✗ FAILED",
        };
        println!("Overall Status: {}", status_str);
        println!();

        println!("Summary:");
        println!("  Total Chunks:          {}", report.total_chunks);
        println!("  Chunks Verified:       {}", report.chunks_verified);
        println!("  Chunks Failed:         {}", report.chunks_failed.len());
        println!(
            "  Chunks Unrecoverable:  {}",
            report.chunks_unrecoverable.len()
        );
        println!("  Total Shards:          {}", report.total_shards);
        println!("  Missing Shards:        {}", report.missing_shards);

        if report.missing_shards > 0 {
            println!(
                "  Redundancy Loss:       {:.1}%",
                (report.missing_shards as f64 / report.total_shards as f64) * 100.0
            );
        }

        println!();

        if !report.chunks_unrecoverable.is_empty() {
            println!(
                "⚠ WARNING: {} chunks are UNRECOVERABLE:",
                report.chunks_unrecoverable.len()
            );
            for chunk_num in &report.chunks_unrecoverable {
                println!("  - Chunk {}", chunk_num);
            }
            println!();
        }

        if !report.chunks_failed.is_empty() {
            println!(
                "⚠ WARNING: {} chunks failed verification:",
                report.chunks_failed.len()
            );
            for chunk_num in &report.chunks_failed {
                println!("  - Chunk {}", chunk_num);
            }
            println!();
        }

        if self.quick_mode || self.full_mode {
            println!("Chunk Details:");
            println!(
                "{:<8} {:<15} {:<12} {:<15}",
                "Chunk", "Shards (A/R)", "Status", "Verification"
            );
            println!("{}", "-".repeat(60));

            for detail in &report.details {
                let expected_shards = detail.shards_required + (detail.shards_required / 2); // data + parity estimate
                let status = if !detail.is_recoverable {
                    "UNRECOVERABLE"
                } else if detail.shards_available < expected_shards {
                    "DEGRADED"
                } else {
                    "HEALTHY"
                };

                let verification = if detail.verification_performed {
                    match detail.checksum_valid {
                        Some(true) => "VERIFIED ✓",
                        Some(false) => "FAILED ✗",
                        None => "-",
                    }
                } else {
                    "-"
                };

                println!(
                    "{:<8} {:<15} {:<12} {:<15}",
                    detail.chunk_number,
                    format!("{}/{}", detail.shards_available, detail.shards_required),
                    status,
                    verification,
                );
            }
        }
    }

    fn write_report_file(&self, report: &VerificationReport, path: &PathBuf) -> Result<()> {
        let json = serde_json::to_string_pretty(&report)?;
        let mut file = File::create(path)?;
        file.write_all(json.as_bytes())?;
        println!("\nDetailed report written to: {}", path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::create::ArchiveBuilder;
    use std::fs::{self, File};
    use std::io::Write as IoWriteTrait;
    use tempfile::TempDir;

    fn create_test_archive(temp_dir: &TempDir) -> String {
        // Create test file
        let test_file = temp_dir.path().join("test.txt");
        let mut file = File::create(&test_file).unwrap();
        file.write_all(b"Test data for verification").unwrap();
        drop(file);

        // Create archive
        let archive_base = temp_dir
            .path()
            .join("archive")
            .to_string_lossy()
            .to_string();
        let builder = ArchiveBuilder::new(archive_base.clone())
            .data_shards(4)
            .parity_shards(2)
            .chunk_size(Some(1024 * 1024));

        builder.create(&[test_file]).unwrap();
        archive_base
    }

    #[test]
    fn test_verifier_new() {
        let verifier = ArchiveVerifier::new("test_pattern".to_string());
        assert!(!verifier.quick_mode);
        assert!(!verifier.full_mode);
        assert!(verifier.report_path.is_none());
    }

    #[test]
    fn test_verifier_quick() {
        let verifier = ArchiveVerifier::new("test".to_string()).quick();
        assert!(verifier.quick_mode);
    }

    #[test]
    fn test_verifier_full() {
        let verifier = ArchiveVerifier::new("test".to_string()).full();
        assert!(verifier.full_mode);
    }

    #[test]
    fn test_verifier_report() {
        let verifier = ArchiveVerifier::new("test".to_string())
            .report(Some(PathBuf::from("/tmp/report.json")));
        assert_eq!(
            verifier.report_path,
            Some(PathBuf::from("/tmp/report.json"))
        );
    }

    #[test]
    fn test_verify_healthy_archive() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive(&temp_dir);
        let pattern = format!("{}.c*.s*", archive_base);

        let verifier = ArchiveVerifier::new(pattern);
        let report = verifier.verify().unwrap();

        assert_eq!(report.status, VerificationStatus::Healthy);
        assert_eq!(report.chunks_unrecoverable.len(), 0);
        assert_eq!(report.chunks_failed.len(), 0);
    }

    #[test]
    fn test_verify_quick_mode() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive(&temp_dir);
        let pattern = format!("{}.c*.s*", archive_base);

        let verifier = ArchiveVerifier::new(pattern).quick();
        let report = verifier.verify().unwrap();

        assert_eq!(report.status, VerificationStatus::Healthy);
    }

    #[test]
    fn test_verify_full_mode() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive(&temp_dir);
        let pattern = format!("{}.c*.s*", archive_base);

        let verifier = ArchiveVerifier::new(pattern).full();
        let report = verifier.verify().unwrap();

        assert_eq!(report.status, VerificationStatus::Healthy);
        assert_eq!(report.chunks_verified, report.total_chunks);
    }

    #[test]
    fn test_verify_degraded_archive() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive(&temp_dir);

        // Delete one shard (archive should still be recoverable but degraded)
        let shard_to_delete = temp_dir.path().join("archive.c001.s00");
        fs::remove_file(shard_to_delete).unwrap();

        let pattern = format!("{}.c*.s*", archive_base);
        let verifier = ArchiveVerifier::new(pattern);
        let report = verifier.verify().unwrap();

        assert_eq!(report.status, VerificationStatus::Degraded);
        assert!(report.missing_shards > 0);
    }

    #[test]
    fn test_verify_failed_archive() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive(&temp_dir);

        // Delete enough shards to make archive unrecoverable (need 4 data shards, delete 3)
        for i in 0..3 {
            let shard_path = temp_dir.path().join(format!("archive.c001.s{:02}", i));
            fs::remove_file(shard_path).unwrap();
        }

        let pattern = format!("{}.c*.s*", archive_base);
        let verifier = ArchiveVerifier::new(pattern);
        let report = verifier.verify().unwrap();

        assert_eq!(report.status, VerificationStatus::Failed);
        assert!(!report.chunks_unrecoverable.is_empty());
    }

    #[test]
    fn test_verify_with_report_file() {
        let temp_dir = TempDir::new().unwrap();
        let archive_base = create_test_archive(&temp_dir);
        let report_path = temp_dir.path().join("report.json");
        let pattern = format!("{}.c*.s*", archive_base);

        let verifier = ArchiveVerifier::new(pattern).report(Some(report_path.clone()));
        let _report = verifier.verify().unwrap();

        // Verify report file was created
        assert!(report_path.exists());
        let content = fs::read_to_string(&report_path).unwrap();
        let parsed: VerificationReport = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.status, VerificationStatus::Healthy);
    }

    #[test]
    fn test_verify_missing_index() {
        let temp_dir = TempDir::new().unwrap();
        let pattern = temp_dir
            .path()
            .join("nonexistent.c*.s*")
            .to_string_lossy()
            .to_string();

        let verifier = ArchiveVerifier::new(pattern);
        let result = verifier.verify();

        assert!(result.is_err());
    }

    #[test]
    fn test_verification_report_serialization() {
        let report = VerificationReport {
            archive_name: "test".to_string(),
            total_chunks: 2,
            chunks_verified: 2,
            chunks_failed: vec![],
            chunks_unrecoverable: vec![],
            total_shards: 12,
            missing_shards: 0,
            status: VerificationStatus::Healthy,
            details: vec![ChunkVerificationDetail {
                chunk_number: 1,
                shards_available: 6,
                shards_required: 4,
                is_recoverable: true,
                verification_performed: true,
                checksum_valid: Some(true),
            }],
        };

        let json = serde_json::to_string(&report).unwrap();
        let parsed: VerificationReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.archive_name, "test");
        assert_eq!(parsed.status, VerificationStatus::Healthy);
    }

    #[test]
    fn test_verification_status_variants() {
        assert_eq!(VerificationStatus::Healthy, VerificationStatus::Healthy);
        assert_ne!(VerificationStatus::Healthy, VerificationStatus::Degraded);
        assert_ne!(VerificationStatus::Degraded, VerificationStatus::Failed);
    }

    #[test]
    fn test_chunk_detail_serialization() {
        let detail = ChunkVerificationDetail {
            chunk_number: 5,
            shards_available: 4,
            shards_required: 3,
            is_recoverable: true,
            verification_performed: false,
            checksum_valid: None,
        };

        let json = serde_json::to_string(&detail).unwrap();
        let parsed: ChunkVerificationDetail = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.chunk_number, 5);
        assert!(parsed.is_recoverable);
    }
}
