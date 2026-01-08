# LTO Tape Drive Support Implementation Plan

## Overview

Add support for writing Ectar shards directly to LTO tape drives for RAIT (Redundant Array of Inexpensive Tapes) functionality. This enables parallel writing to multiple tape drives and recovery from partial tape failures.

## Test-Driven Design Approach

Following TDD principles: Red-Green-Refactor cycle
1. Write failing tests first (Red)
2. Implement minimal code to pass tests (Green)
3. Refactor for clarity and performance (Refactor)

## Phase 1: Failing Tests (Red Phase)

### 1.1 Unit Tests for TapeShardOutput

**Test File:** `tests/tape_shard_output.rs`

```rust
#[cfg(test)]
mod tests {
    use std::io::Write;
    use ectar::io::tape::{TapeShardOutput, MockTapeDevice};

    #[test]
    fn test_tape_shard_output_creation() {
        // Should create TapeShardOutput with valid tape device
        // Expected: FAIL - TapeShardOutput doesn't exist yet
    }

    #[test]
    fn test_block_aligned_writing() {
        // Write data smaller than block size, then larger
        // Should buffer internally and write complete blocks
        // Expected: FAIL - no implementation
    }

    #[test]
    fn test_partial_block_flush() {
        // Write partial block and call flush()
        // Should pad to block boundary and write
        // Expected: FAIL - no TapeShardOutput::flush
    }

    #[test]
    fn test_shard_output_finish() {
        // Call finish() after writing
        // Should return total bytes written and finalize tape
        // Expected: FAIL - no ShardOutput impl
    }

    #[test]
    fn test_tape_device_error_handling() {
        // Simulate tape device write error
        // Should propagate error appropriately
        // Expected: FAIL - no error handling
    }
}
```

### 1.2 Integration Tests for RAIT Multi-Tape Writer

**Test File:** `tests/rait_shard_writer.rs`

```rust
#[cfg(test)]
mod tests {
    use ectar::io::rait::RaitShardWriter;
    use ectar::io::tape::MockTapeDevice;

    #[test]
    fn test_rait_writer_creation() {
        // Create RaitShardWriter with 3 mock tape devices
        // Should initialize all tape outputs
        // Expected: FAIL - RaitShardWriter doesn't exist
    }

    #[test]
    fn test_parallel_shard_writing() {
        // Write multiple shards to different tapes simultaneously
        // Should distribute one shard per tape drive
        // Expected: FAIL - no parallel writing logic
    }

    #[test]
    fn test_shard_boundary_tracking() {
        // Track positions of each shard on tape
        // Should maintain accurate position metadata
        // Expected: FAIL - no position tracking
    }

    #[test]
    fn test_partial_tape_failure_recovery() {
        // Simulate one tape drive failure during writing
        // Should continue with remaining tapes
        // Expected: FAIL - no error recovery
    }
}
```

### 1.3 Unit Tests for TapeShardReader

**Test File:** `tests/tape_shard_reader.rs`

```rust
#[cfg(test)]
mod tests {
    use ectar::io::tape::TapeShardReader;

    #[test]
    fn test_tape_shard_reader_creation() {
        // Create reader with tape device paths
        // Should parse shard positions from metadata
        // Expected: FAIL - TapeShardReader doesn't exist
    }

    #[test]
    fn test_seek_to_specific_shard() {
        // Seek to chunk 1, shard 2 position
        // Should position tape correctly
        // Expected: FAIL - no seek implementation
    }

    #[test]
    fn test_read_shard_from_position() {
        // Read shard data from current tape position
        // Should return correct shard bytes
        // Expected: FAIL - no read implementation
    }

    #[test]
    fn test_multi_tape_parallel_reading() {
        // Read from multiple tapes simultaneously
        // Should coordinate parallel reads
        // Expected: FAIL - no parallel reading
    }
}
```

### 1.4 CLI Integration Tests

**Test File:** `tests/cli_tape_support.rs`

```rust
#[cfg(test)]
mod tests {
    use assert_cmd::Command;

    #[test]
    fn test_create_with_tape_devices() {
        // ectar create --tape-devices /dev/tape0,/dev/tape1,/dev/tape2
        // Should accept tape device arguments
        // Expected: FAIL - no CLI options
    }

    #[test]
    fn test_extract_from_tape_devices() {
        // ectar extract --tape-devices /dev/tape0,/dev/tape1,/dev/tape2
        // Should read from specified tape devices
        // Expected: FAIL - no tape extract support
    }

    #[test]
    fn test_tape_block_size_parameter() {
        // ectar create --block-size 512KB --tape-devices ...
        // Should configure block size for tape writing
        // Expected: FAIL - no block-size option
    }
}
```

### 1.5 Mock Infrastructure

**Test File:** `tests/mocks.rs`

```rust
#[cfg(test)]
pub mod mocks {
    use std::io::{Read, Write, Seek};
    use std::collections::VecDeque;

    pub struct MockTapeDevice {
        pub data: Vec<u8>,
        pub position: u64,
        pub block_size: usize,
        pub simulate_errors: bool,
        pub error_at_position: Option<u64>,
    }

    impl MockTapeDevice {
        pub fn new(block_size: usize) -> Self {
            // Create mock tape device with configurable block size
            // Expected: FAIL - no mock implementation
        }
    }

    impl Write for MockTapeDevice {
        // Mock write implementation
        // Expected: FAIL - no Write impl
    }

    impl Read for MockTapeDevice {
        // Mock read implementation
        // Expected: FAIL - no Read impl
    }

    impl Seek for MockTapeDevice {
        // Mock seek implementation
        // Expected: FAIL - no Seek impl
    }
}
```

## Phase 2: Implementation (Green Phase)

Once all tests are written and failing:

1. **Implement TapeShardOutput** (`src/io/tape_shard_writer.rs`)
   - Basic struct with tape device handle
   - Block-aligned buffering logic
   - Write and flush implementations
   - ShardOutput trait implementation

2. **Implement RaitShardWriter** (`src/io/rait_shard_writer.rs`)
   - Multi-tape coordination
   - Parallel shard distribution
   - Position tracking and metadata

3. **Implement TapeShardReader** (`src/io/tape_shard_reader.rs`)
   - Position-based seeking
   - Shard reading from tape
   - Multi-tape parallel reading

4. **Add CLI Options** (`src/cli.rs`)
   - `--tape-devices` parameter
   - `--block-size` parameter
   - Update create/extract commands

5. **Mock Infrastructure** (`tests/mocks.rs`)
   - Complete MockTapeDevice implementation
   - Error simulation capabilities

## Phase 3: Refinement (Refactor Phase)

1. **Performance Optimization**
   - Optimize buffer sizes for tape throughput
   - Async I/O for parallel tape operations

2. **Error Handling Enhancement**
   - Comprehensive tape error recovery
   - Graceful degradation on tape failures

3. **Integration Testing**
   - End-to-end tests with mock tapes
   - CLI integration verification

4. **Documentation**
   - Update README with tape usage examples
   - API documentation for new modules

## Testing Strategy

- **Unit Tests**: Individual component behavior
- **Integration Tests**: Full pipeline with mocks
- **Manual Testing**: Real tape drives (when available)
- **Error Path Testing**: Simulate various tape failure scenarios

## Dependencies

- Add `nix` crate for ioctl calls (tape device control)
- Consider `tokio` for async tape operations if needed

## Risk Assessment

- **High Risk**: Tape device compatibility across different LTO generations
- **Medium Risk**: Block alignment and buffering logic
- **Low Risk**: CLI integration and mock testing infrastructure