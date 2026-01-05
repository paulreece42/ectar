# Known Bugs and Issues

## Critical

- [x] **Index is required for extraction** - make index optional for extraction / recovery

  Shards now include zfec-compatible headers (2-4 bytes) that contain k, m, sharenum, and padlen.
  Extraction works without the index file using shard headers for emergency recovery.
  **Fixed:** Added ZfecHeader support and index-optional extraction mode.

- [x] **Division by zero with empty archives** - `src/main.rs:242`

  If `total_size` is 0 (empty archive), the compression ratio calculation causes division by zero.
  Same issue exists in `src/cli/info.rs:119-122`.
  **Fixed:** Added checks to avoid division by zero.

- [x] **Log message shows wrong value after truncation** - `src/erasure/decoder.rs:77-81`

  The debug log reports the size after truncation for both values, making it misleading.
  **Fixed:** Store original length before truncation.

- [x] **Chunk extraction loop assumes contiguous numbering** - `src/archive/extract.rs:169`

  This assumes chunks are numbered 1 through N consecutively. If chunks have gaps, this will try to process non-existent chunks.
  **Fixed:** Now iterates over actual chunk numbers from the index.

- [x] **Absolute paths passed to tar for single files** - `src/archive/create.rs`

  When archiving single files with absolute paths, tar would reject them.
  **Fixed:** Now uses just the filename when no base path is available.

## Medium

- [x] **Unused CLI flags for Extract command** - `src/main.rs:91-99`

  The following flags are parsed but never used:
  - `--files` (extract specific files)
  - `--exclude` (exclude patterns)
  - `--strip-components` (strip path prefix)

  **Fixed:** Implemented file filtering, exclude patterns, and strip_components in ArchiveExtractor.

- [x] **Shard size stored only from first chunk** - `src/archive/create.rs:298-302`

  Only the first chunk's shard size is recorded in the index. Different chunks may have different shard sizes.
  **Fixed:** Now stores per-chunk shard sizes in the index.

- [ ] **Chunk checksums never computed** - `src/archive/create.rs:538-539`

  The chunk checksum field is always empty:
  ```rust
  checksum: String::new(), // TODO: Compute chunk checksum
  ```

- [x] **`--no-compression` flag ignored in chunked mode** - `src/archive/create.rs:185`

  The `create_chunked()` function always applies compression regardless of `self.no_compression`.
  **Fixed:** StreamingErasureChunkingWriter now supports no_compression mode.

## Low

- [ ] **Progress bar flags ignored** - `src/main.rs:67-72, 203-204`

  Both `--progress` and `--no-progress` flags are parsed but never used.

- [ ] **Pattern matching is overly broad** - `src/archive/list.rs:95-99`

  The `matches_pattern()` function uses `path.contains(pattern)` before trying glob matching, which may match unintended files.

- [ ] **CSV output not properly escaped** - `src/archive/list.rs:154-176`

  File paths containing commas or quotes will produce malformed CSV output.

- [ ] **Verification shard estimate is imprecise** - `src/cli/verify.rs:270`

  ```rust
  let expected_shards = detail.shards_required + (detail.shards_required / 2);
  ```
  This hardcodes parity as 50% of data shards instead of using actual values.

- [ ] **Empty byte size input gives confusing error** - `src/utils.rs:18`

  Parsing an empty string like `""` or `"KB"` (no number) fails with a generic parse error instead of a clear message.

## Architecture Improvements

- [ ] **File offset always zero** - `src/archive/create.rs:389`

  The `offset` field in `FileEntry` is always 0. Either track actual offsets or remove the field.

- [ ] **No resume for interrupted archive creation**

  If archive creation is interrupted, there's no way to resume from where it left off.

- [ ] **Index file is not erasure-coded**

  The `.index.zst` file is a single point of failure. Consider applying erasure coding to the index as well, or embedding index data in shards.
