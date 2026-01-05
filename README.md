# Ectar - Erasure-Coded Tar Archive Utility

**Ectar** is a command-line utility for creating and extracting tar archives with Reed-Solomon erasure coding, designed for long-term data preservation on degraded media (tapes, failing drives, etc.).

## Disclaimer

I vibe-coded this in a few hours in early January 2026, for my own personal use, experimentation, and learning

As I find bugs, I'll fix them, as I have time

It has **not** been heavily tested, in production deployments

Data storage can be extremely tricky, full of tiny gotchas and strange edge cases that you will not discover until you are, for example,
attempting to restore petabytes of data, after a real-world failure, 10+ years down the road

I strongly suggest working with an experienced vendor, if doing this in production environments.

## Features

- **Erasure Coding**: Uses Reed-Solomon encoding to create k+m shards per chunk
- **Resilient Recovery**: Can recover data even when up to m shards are lost or corrupted
- **Size-Limited Chunking**: Splits archives into manageable chunks (e.g., 1GB each)
- **Streaming Pipeline**: Single-pass archive creation with parallel shard output
- **Zstd Compression**: Multi-threaded compression with configurable levels (1-22)
- **Independent Chunk Recovery**: Each chunk can be recovered independently
- **Comprehensive Indexing**: Searchable compressed JSON index with file metadata and checksums
- **Partial Extraction**: Recover what's possible even when some chunks are lost
- **Long-Term Preservation**: Self-describing formats (tar, zstd, JSON) readable 50+ years from now

## Installation

```bash
cargo build --release
sudo cp target/release/ectar /usr/local/bin/
```

## Usage

### Create an Archive

```bash
# Basic archive with default settings (10 data + 5 parity shards)
ectar create --output backup /path/to/data

# Custom erasure coding parameters
ectar create --output backup \
  --data-shards 10 \
  --parity-shards 5 \
  /path/to/data

# Chunked archive (1GB chunks)
ectar create --output backup \
  --chunk-size 1GB \
  --data-shards 8 \
  --parity-shards 4 \
  /path/to/data

# High compression for text files
ectar create --output logs \
  --compression-level 19 \
  --data-shards 6 \
  --parity-shards 3 \
  /var/log

# No compression (for pre-compressed data)
ectar create --output media \
  --no-compression \
  --chunk-size 500MB \
  /path/to/videos
```

**Output Files:**
- `backup.c001.s00` through `.s14` (chunk 1: 10 data + 5 parity shards)
- `backup.c002.s00` through `.s14` (chunk 2, if chunked)
- `backup.index.zst` (compressed JSON index)

### Extract an Archive

```bash
# Extract full archive
ectar extract --input "backup.c*.s*" --output /restore

# Extract specific files (glob patterns supported)
ectar extract \
  --input "backup.c*.s*" \
  --files "*.pdf" \
  --output /restore

# Exclude certain files
ectar extract \
  --input "backup.c*.s*" \
  --exclude "*.tmp" \
  --output /restore

# Strip leading path components
ectar extract \
  --input "backup.c*.s*" \
  --strip-components 2 \
  --output /restore

# Partial recovery (some chunks missing)
ectar extract \
  --input "backup.c*.s*" \
  --partial \
  --output /partial-restore
```

### List Archive Contents

```bash
# List all files
ectar list --input "backup.c*.s*"

# Long listing with metadata
ectar list --input "backup.c*.s*" --long

# JSON output for scripting
ectar list --input "backup.c*.s*" --format json

# CSV output
ectar list --input "backup.c*.s*" --format csv

# Filter by pattern
ectar list --input "backup.c*.s*" --files "*.pdf"
```

### Verify Archive

```bash
# Quick verification (check shard existence)
ectar verify --input "backup.c*.s*" --quick

# Full verification (decode and verify checksums)
ectar verify \
  --input "backup.c*.s*" \
  --full \
  --report verify-report.json
```

### Show Archive Info

```bash
# Display archive metadata
ectar info --input "backup.c*.s*"

# JSON output
ectar info --input "backup.c*.s*" --format json
```

## File Naming Convention

```
<basename>.c<chunk>.s<shard>
```

- `<basename>`: User-specified archive name
- `.c<chunk>`: Chunk number (001, 002, ..., 999)
- `.s<shard>`: Shard number (00, 01, ..., 99)

**Examples:**
- `backup.c001.s00` - Chunk 1, shard 0 (first data shard)
- `backup.c001.s09` - Chunk 1, shard 9 (last data shard with k=10)
- `backup.c001.s10` - Chunk 1, shard 10 (first parity shard)
- `backup.c001.s14` - Chunk 1, shard 14 (last parity shard with m=5)
- `backup.index.zst` - Compressed JSON index

## How Erasure Coding Works

With `--data-shards=10` and `--parity-shards=5`:

1. Each chunk is split into 10 data shards
2. 5 parity shards are generated using Reed-Solomon encoding
3. Total of 15 shards are created per chunk
4. You can lose **any 5 shards** and still recover the complete chunk
5. Storage overhead: 1.5x (15 shards / 10 data shards)

**Recovery Scenarios:**
- All 15 shards present → Direct extraction
- 10-14 shards present → Full recovery via Reed-Solomon
- <10 shards present → Chunk unrecoverable (but other chunks may be fine with `--partial`)

## Index File Format

The index is a zstd-compressed JSON file containing:

```json
{
  "version": "1.0",
  "created": "2026-01-04T12:00:00Z",
  "tool_version": "0.1.0",
  "archive_name": "backup",
  "parameters": {
    "data_shards": 10,
    "parity_shards": 5,
    "chunk_size": 1073741824,
    "compression_level": 3
  },
  "chunks": [
    {
      "chunk_number": 1,
      "compressed_size": 1048576,
      "uncompressed_size": 2097152,
      "shard_size": 104858
    }
  ],
  "files": [
    {
      "path": "docs/report.pdf",
      "chunk": 1,
      "offset": 0,
      "size": 1048576,
      "checksum": "sha256:abc123...",
      "mode": 33188,
      "mtime": "2026-01-02T15:30:00Z",
      "entry_type": "file"
    }
  ]
}
```

### Searching the Index

```bash
# Find all PDF files
zstdcat backup.index.zst | jq '.files[] | select(.path | endswith(".pdf"))'

# Find files larger than 100MB
zstdcat backup.index.zst | jq '.files[] | select(.size > 104857600)'

# Find files in specific chunk
zstdcat backup.index.zst | jq '.files[] | select(.chunk == 3)'

# Simple grep
zstdcat backup.index.zst | grep "report.pdf"
```

## Architecture

### Data Flow (Creation)
```
Files → Tar Stream → Zstd Compress → Chunk → Reed-Solomon Encode → Shards
                                              ↓
                                         Index (compressed JSON)
```

The streaming pipeline processes data in a single pass, writing shards directly without intermediate files.

### Data Flow (Extraction)
```
Shards (≥k per chunk) → Reed-Solomon Decode → Zstd Decompress → Tar Extract → Files
```

## Long-Term Recovery

Ectar is designed for data recovery 50+ years in the future:

1. **Self-describing formats**: Standard tar, zstd, and JSON formats
2. **Human-readable index**: Decompressed index is plain JSON
3. **Manual recovery possible**: Data shards contain actual compressed tar data
4. **No proprietary formats**: All components use widely-documented open formats

## Extraction using standard open-source tools

We all know how frusturating it is to find an abandoned FOSS project that has created files which
will no longer easily compile, 10+ years after it is written

That's why I made sure the resultant files are extractable using (currently) commonly available FOSS tools on most Linux distros

You will need:

- zstd
- GNU tar
- zfec python library

Then, use the included un-ec.py utility to create a standard .tar.zst file, easily extractable with modern GNU tar

### Step 1: Get chunk sizes from the index

First, decompress and read the index file to get the `compressed_size` for each chunk:

```bash
zstd -d -c backup.index.zst | python3 -m json.tool
```

Look for the `chunks` array, which contains entries like:
```json
{
  "chunk_number": 1,
  "compressed_size": 1048177,
  "uncompressed_size": 1048576,
  "shard_size": 349393
}
```

The `compressed_size` is the exact size you need for the `--size` parameter.

### Step 2: Reconstruct each chunk

For each chunk, use un-ec.py with the `--size` parameter to remove Reed-Solomon padding:

```bash
# Install zfec if needed
pip install zfec

# Reconstruct chunk 1 (using shards 0, 1, 2 with k=3, n=5)
python un-ec.py -k 3 -n 5 --size 1048177 -o chunk001.zst \
    backup.c001.s00 backup.c001.s01 backup.c001.s02 --indices 0 1 2
```

**Important:** The `--size` parameter is required for valid output. Without it, Reed-Solomon padding bytes will corrupt the compressed data stream, causing zstd to fail with "unknown header".

### Step 3: Decompress and concatenate chunks

```bash
# Decompress each chunk
zstd -d chunk001.zst
zstd -d chunk002.zst
# ... repeat for all chunks

# Concatenate into a single tar file
cat chunk001 chunk002 chunk003 > combined.tar

# Extract with GNU tar
tar -xf combined.tar
```

### Complete example with multiple chunks

```bash
# Read the index to get chunk sizes
zstd -d -c backup.index.zst | python3 -c "
import json, sys
idx = json.load(sys.stdin)
for c in idx['chunks']:
    print(f\"Chunk {c['chunk_number']:03d}: compressed_size={c['compressed_size']}\")
"

# Reconstruct each chunk (example with k=3, n=5)
python un-ec.py -k 3 -n 5 --size 1048177 -o chunk001.zst backup.c001.s0* --indices 0 1 2
python un-ec.py -k 3 -n 5 --size 1048606 -o chunk002.zst backup.c002.s0* --indices 0 1 2
python un-ec.py -k 3 -n 5 --size 1048606 -o chunk003.zst backup.c003.s0* --indices 0 1 2

# Decompress and concatenate
for f in chunk*.zst; do zstd -d "$f"; done
cat chunk001 chunk002 chunk003 > combined.tar

# Extract
tar -xf combined.tar
```

## Development Status

**Implemented:**
- [x] Archive creation with streaming pipeline
- [x] Zstd compression (multi-threaded)
- [x] Size-limited chunking
- [x] Reed-Solomon erasure coding
- [x] Index generation with file metadata
- [x] Archive extraction with Reed-Solomon recovery
- [x] Partial extraction mode
- [x] File filtering and exclusion
- [x] List command with multiple output formats
- [x] Verify command (quick and full modes)
- [x] Info command
- [x] Comprehensive test suite (219 tests, 90.72% coverage)

**Todo:**
- [ ] Remove requirement for index to be present to restore
- [ ] Add read/write directly to multiple LTO tapes
- [ ] Possibly create mbuffer-like command using EC to/from LTO tapes (or separate project)

**Known Issues:**
See [BUGS.md](BUGS.md) for known issues and planned improvements.

## Contributing

Contributions are welcome! Please see CONTRIBUTING.md for guidelines.

## License

- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)


## Acknowledgments

- Uses [reed-solomon-erasure](https://github.com/rust-rse/reed-solomon-erasure) for erasure coding
- Built with [clap](https://github.com/clap-rs/clap) for CLI parsing
- Compression via [zstd-rs](https://github.com/gyscos/zstd-rs)
