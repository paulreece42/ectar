use clap::{Parser, Subcommand};
use ectar::error::Result;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ectar")]
#[command(about = "Erasure-coded tar archive utility for long-term data preservation", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Suppress output
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new erasure-coded archive
    Create {
        /// Output archive base name (without extensions)
        #[arg(short, long)]
        output: String,

        /// Number of data shards (auto-configured when using tape devices)
        #[arg(long, default_value = "10")]
        data_shards: usize,

        /// Number of parity shards (auto-configured when using tape devices)
        #[arg(long, default_value = "5")]
        parity_shards: usize,

        /// Maximum chunk size (e.g., 1GB, 100MB)
        #[arg(long)]
        chunk_size: Option<String>,

        /// Zstd compression level (1-22)
        #[arg(long, default_value = "3")]
        compression_level: i32,

        /// Disable compression
        #[arg(long)]
        no_compression: bool,

        /// Don't generate index file
        #[arg(long)]
        no_index: bool,

        /// Exclude files matching pattern (can be repeated)
        #[arg(long)]
        exclude: Vec<String>,

        /// Follow symbolic links
        #[arg(long)]
        follow_symlinks: bool,

        /// Don't preserve file permissions
        #[arg(long)]
        no_preserve_permissions: bool,

        /// Show progress bar (not yet implemented)
        #[arg(long)]
        progress: bool,

        /// Disable progress bar (not yet implemented)
        #[arg(long)]
        no_progress: bool,

        /// Tape device paths for RAIT (Redundant Array of Inexpensive Tapes)
        #[arg(long)]
        tape_devices: Vec<String>,

        /// Tape block size (e.g., 512, 1KB, 4KB) - default 512 bytes
        #[arg(long, default_value = "512")]
        block_size: String,

        /// Files or directories to archive
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },

    /// Extract files from an archive
    Extract {
        /// Input shard pattern (e.g., "backup.tar.zst.c*.s*")
        #[arg(short, long)]
        input: String,

        /// Output directory
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Extract only specific files/paths
        #[arg(long)]
        files: Vec<String>,

        /// Exclude files matching pattern
        #[arg(long)]
        exclude: Vec<String>,

        /// Strip N leading path components
        #[arg(long)]
        strip_components: Option<usize>,

        /// Verify SHA256 checksums during extraction
        #[arg(long, default_value = "true")]
        verify_checksums: bool,

        /// Skip checksum verification
        #[arg(long)]
        no_verify_checksums: bool,

        /// Continue extraction even with errors
        #[arg(long)]
        force: bool,

        /// Extract recoverable chunks even if some are lost
        #[arg(long)]
        partial: bool,

        /// Show progress bar
        #[arg(long)]
        progress: bool,

        /// Tape device paths for reading RAIT archives
        #[arg(long)]
        tape_devices: Vec<String>,

        /// Tape block size (e.g., 512, 1KB, 4KB) - default 512 bytes
        #[arg(long, default_value = "512")]
        block_size: String,
    },

    /// List contents of an archive
    List {
        /// Input shard pattern or index file
        #[arg(short, long)]
        input: String,

        /// List only files matching pattern
        #[arg(long)]
        files: Option<String>,

        /// Long listing format with metadata
        #[arg(long)]
        long: bool,

        /// Output format: text, json, csv
        #[arg(long, default_value = "text")]
        format: String,
    },

    /// Verify archive integrity
    Verify {
        /// Input shard pattern
        #[arg(short, long)]
        input: String,

        /// Quick check (verify shard existence only)
        #[arg(long)]
        quick: bool,

        /// Full check (decode and verify checksums)
        #[arg(long)]
        full: bool,

        /// Write detailed report to file
        #[arg(long)]
        report: Option<PathBuf>,
    },

    /// Display archive metadata
    Info {
        /// Input shard pattern or index file
        #[arg(short, long)]
        input: String,

        /// Output format: text, json
        #[arg(long, default_value = "text")]
        format: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging based on verbosity
    let log_level = if cli.quiet {
        "error"
    } else {
        match cli.verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }
    };

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    log::info!("ectar version {}", env!("CARGO_PKG_VERSION"));

    match cli.command {
        Commands::Create {
            output,
            data_shards,
            parity_shards,
            chunk_size,
            compression_level,
            no_compression,
            no_index,
            exclude,
            follow_symlinks,
            no_preserve_permissions,
            progress: _,
            no_progress: _,
            tape_devices,
            block_size,
            paths,
        } => {
            use ectar::archive::create::ArchiveBuilder;
            use ectar::utils;

            // Parse chunk size if provided
            let chunk_size_bytes = if let Some(ref cs) = chunk_size {
                Some(utils::parse_byte_size(cs)?)
            } else {
                None
            };

            // Build the archive
            println!("Creating archive: {}", output);
            if let Some(cs) = chunk_size_bytes {
                println!("  Chunk size: {} bytes", cs);
            }

            let mut builder = ArchiveBuilder::new(output.clone())
                .data_shards(data_shards)
                .parity_shards(parity_shards)
                .chunk_size(chunk_size_bytes)
                .compression_level(compression_level)
                .no_compression(no_compression)
                .no_index(no_index)
                .exclude_patterns(exclude)
                .follow_symlinks(follow_symlinks)
                .preserve_permissions(!no_preserve_permissions)
                .tape_devices(tape_devices.clone())
                .block_size(utils::parse_byte_size(&block_size)? as usize);

            // Show tape configuration if tape devices are specified
            if !tape_devices.is_empty() {
                let configured_data = tape_devices.len().saturating_sub(1);
                let configured_parity = 1;
                println!(
                    "  Erasure coding: {} data + {} parity shards (auto-configured for tape RAIT)",
                    configured_data, configured_parity
                );
                println!(
                    "  Tape devices: {} ({})",
                    tape_devices.len(),
                    tape_devices.join(", ")
                );
            } else {
                println!("  Data shards: {}", data_shards);
                println!("  Parity shards: {}", parity_shards);
            }

            let metadata = builder.create(&paths)?;

            println!("Archive created successfully:");
            println!("  Total files: {}", metadata.total_files);
            println!("  Total size: {} bytes", metadata.total_size);
            println!("  Compressed size: {} bytes", metadata.compressed_size);
            if metadata.total_size > 0 {
                println!(
                    "  Compression ratio: {:.2}%",
                    (metadata.compressed_size as f64 / metadata.total_size as f64) * 100.0
                );
            }
        }

        Commands::Extract {
            input,
            output,
            files,
            exclude,
            strip_components,
            verify_checksums,
            no_verify_checksums,
            force: _,
            partial,
            progress: _,
            tape_devices,
            block_size,
        } => {
            use ectar::archive::extract::ArchiveExtractor;

            println!("Extracting archive from: {}", input);
            if let Some(ref o) = output {
                println!("  Output directory: {}", o.display());
            }

            let mut extractor = ArchiveExtractor::new(input.clone(), output)
                .verify_checksums(verify_checksums && !no_verify_checksums)
                .partial(partial)
                .file_filters(files)
                .exclude_patterns(exclude)
                .tape_devices(tape_devices.clone());

            if let Some(n) = strip_components {
                extractor = extractor.strip_components(n);
            }

            // Set block size for tape mode
            if !tape_devices.is_empty() {
                let parsed_size = ectar::utils::parse_byte_size(&block_size)? as usize;
                extractor = extractor.block_size(parsed_size);
            }

            // Show tape mode info
            if !tape_devices.is_empty() {
                println!("  Tape mode: {} devices", tape_devices.len());
                println!("  Devices: {}", tape_devices.join(", "));
            }

            let metadata = extractor.extract()?;

            println!("\nExtraction complete:");
            println!(
                "  Chunks recovered: {}/{}",
                metadata.chunks_recovered, metadata.chunks_total
            );
            if metadata.chunks_failed > 0 {
                println!("  Chunks failed: {}", metadata.chunks_failed);
            }
            println!("  Files extracted: {}", metadata.files_extracted);

            if metadata.chunks_failed > 0 && partial {
                log::warn!(
                    "Partial extraction: {} chunks could not be recovered",
                    metadata.chunks_failed
                );
            }
        }

        Commands::List {
            input,
            files,
            long,
            format,
        } => {
            use ectar::archive::list::ArchiveLister;

            let lister = ArchiveLister::new(input)
                .filter(files)
                .long_format(long)
                .output_format(&format)?;

            lister.list()?;
        }

        Commands::Verify {
            input,
            quick,
            full,
            report,
        } => {
            use ectar::cli::verify::ArchiveVerifier;

            let mut verifier = ArchiveVerifier::new(input);

            if quick {
                verifier = verifier.quick();
            }
            if full {
                verifier = verifier.full();
            }
            verifier = verifier.report(report);

            let verification_report = verifier.verify()?;

            // Exit with error code if verification failed
            if verification_report.status == ectar::cli::verify::VerificationStatus::Failed {
                std::process::exit(1);
            }
        }

        Commands::Info { input, format } => {
            use ectar::cli::info::ArchiveInfo;

            let info = ArchiveInfo::new(input).output_format(&format)?;

            info.show()?;
        }
    }

    Ok(())
}
