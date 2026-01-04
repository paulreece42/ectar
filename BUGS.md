# Known Bugs and Issues

## Critical

- [ ] **Division by zero with empty archives** - `src/main.rs:242`

  If `total_size` is 0 (empty archive), the compression ratio calculation causes division by zero.
  ```rust
  (metadata.compressed_size as f64 / metadata.total_size as f64) * 100.0
  ```
  Same issue exists in `src/cli/info.rs:119-122`.

- [ ] **Log message shows wrong value after truncation** - `src/erasure/decoder.rs:77-81`

  The debug log reports the size after truncation for both values, making it misleading.
  ```rust
  reconstructed.truncate(expected as usize);
  log::debug!(
      "Trimmed reconstructed chunk from {} to {} bytes",
      reconstructed.len(),  // Already truncated!
      expected
  );
  ```

- [ ] **Chunk extraction loop assumes contiguous numbering** - `src/archive/extract.rs:169`

  ```rust
  for chunk_num in 1..=index.chunks.len()
  ```
  This assumes chunks are numbered 1 through N consecutively. If chunks have gaps (e.g., chunk 2 is missing from index), this will try to process non-existent chunks.

## Medium

- [ ] **Unused CLI flags for Extract command** - `src/main.rs:91-99`

  The following flags are parsed but never used:
  - `--files` (extract specific files)
  - `--exclude` (exclude patterns)
  - `--strip-components` (strip path prefix)
  - `--progress` (show progress bar)

- [ ] **Shard size stored only from first chunk** - `src/archive/create.rs:298-302`

  Only the first chunk's shard size is recorded in the index. If chunks have different compressed sizes, they will have different shard sizes, but only one is stored.

- [ ] **Chunk checksums never computed** - `src/archive/create.rs:538-539`

  The chunk checksum field is always empty:
  ```rust
  checksum: String::new(), // TODO: Compute chunk checksum
  ```

- [ ] **`--no-compression` flag ignored in chunked mode** - `src/archive/create.rs:185`

  The `create_chunked()` function always applies compression regardless of `self.no_compression`.

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
