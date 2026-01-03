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

        /// Number of data shards
        #[arg(long, default_value = "10")]
        data_shards: usize,

        /// Number of parity shards
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

        /// Show progress bar
        #[arg(long)]
        progress: bool,

        /// Disable progress bar
        #[arg(long)]
        no_progress: bool,

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
            progress,
            no_progress,
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
            let mut builder = ArchiveBuilder::new(output.clone())
                .data_shards(data_shards)
                .parity_shards(parity_shards)
                .chunk_size(chunk_size_bytes)
                .compression_level(compression_level)
                .no_compression(no_compression)
                .no_index(no_index)
                .exclude_patterns(exclude)
                .follow_symlinks(follow_symlinks)
                .preserve_permissions(!no_preserve_permissions);

            println!("Creating archive: {}", output);
            if let Some(cs) = chunk_size_bytes {
                println!("  Chunk size: {} bytes", cs);
            }

            let metadata = builder.create(&paths)?;

            println!("Archive created successfully:");
            println!("  Total files: {}", metadata.total_files);
            println!("  Total size: {} bytes", metadata.total_size);
            println!("  Compressed size: {} bytes", metadata.compressed_size);
            println!(
                "  Compression ratio: {:.2}%",
                (metadata.compressed_size as f64 / metadata.total_size as f64) * 100.0
            );
        }

        Commands::Extract {
            input,
            output,
            files,
            exclude,
            strip_components,
            verify_checksums,
            no_verify_checksums,
            force,
            partial,
            progress,
        } => {
            use ectar::archive::extract::ArchiveExtractor;

            println!("Extracting archive from: {}", input);
            if let Some(ref o) = output {
                println!("  Output directory: {}", o.display());
            }

            let extractor = ArchiveExtractor::new(input.clone(), output)
                .verify_checksums(verify_checksums && !no_verify_checksums)
                .partial(partial);

            let metadata = extractor.extract()?;

            println!("\nExtraction complete:");
            println!("  Chunks recovered: {}/{}", metadata.chunks_recovered, metadata.chunks_total);
            if metadata.chunks_failed > 0 {
                println!("  Chunks failed: {}", metadata.chunks_failed);
            }
            println!("  Files extracted: {}", metadata.files_extracted);

            if metadata.chunks_failed > 0 && partial {
                log::warn!("Partial extraction: {} chunks could not be recovered", metadata.chunks_failed);
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

            let info = ArchiveInfo::new(input)
                .output_format(&format)?;

            info.show()?;
        }
    }

    Ok(())
}
