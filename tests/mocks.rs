#[cfg(test)]
pub mod mocks {
    use std::io::{Read, Result, Seek, Write};

    /// Mock tape device that simulates tape drive behavior for testing
    pub struct MockTapeDevice {
        pub data: Vec<u8>,
        pub position: u64,
        pub block_size: usize,
        pub simulate_errors: bool,
        pub error_at_position: Option<u64>,
        pub write_buffer: Vec<u8>,
    }

    impl MockTapeDevice {
        /// Create a new mock tape device with the specified block size
        pub fn new(block_size: usize) -> Self {
            Self {
                data: Vec::new(),
                position: 0,
                block_size,
                simulate_errors: false,
                error_at_position: None,
                write_buffer: Vec::new(),
            }
        }

        /// Enable error simulation at a specific position
        pub fn simulate_error_at(&mut self, position: u64) {
            self.simulate_errors = true;
            self.error_at_position = Some(position);
        }

        /// Check if an operation should fail due to simulated error
        fn should_fail(&self) -> bool {
            self.simulate_errors
                && self
                    .error_at_position
                    .map_or(false, |pos| self.position >= pos)
        }

        /// Ensure data vector is large enough for the current position
        fn ensure_capacity(&mut self, required_len: usize) {
            if required_len > self.data.len() {
                self.data.resize(required_len, 0);
            }
        }
    }

    impl Write for MockTapeDevice {
        fn write(&mut self, buf: &[u8]) -> Result<usize> {
            if self.should_fail() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Simulated tape error",
                ));
            }

            // Tape devices write in fixed block sizes, so buffer until we have complete blocks
            self.write_buffer.extend_from_slice(buf);

            let mut bytes_written = 0;
            while self.write_buffer.len() >= self.block_size {
                // Write a complete block
                let block: Vec<u8> = self.write_buffer[..self.block_size].to_vec();
                let start_pos = self.position as usize;
                let end_pos = start_pos + self.block_size;

                self.ensure_capacity(end_pos);
                self.data[start_pos..end_pos].copy_from_slice(&block);

                self.write_buffer.drain(..self.block_size);
                self.position += self.block_size as u64;
                bytes_written += self.block_size;
            }

            Ok(buf.len()) // Return the number of bytes accepted (buffered)
        }

        fn flush(&mut self) -> Result<()> {
            if self.should_fail() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Simulated tape flush error",
                ));
            }

            // If there's a partial block remaining, pad it to block boundary
            if !self.write_buffer.is_empty() {
                let padding_needed = self.block_size - self.write_buffer.len();
                self.write_buffer.resize(self.block_size, 0); // Pad with zeros

                let start_pos = self.position as usize;
                let end_pos = start_pos + self.block_size;
                self.ensure_capacity(end_pos);
                self.data[start_pos..end_pos].copy_from_slice(&self.write_buffer);

                self.position += self.block_size as u64;
                self.write_buffer.clear();
            }

            Ok(())
        }
    }

    impl Read for MockTapeDevice {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
            if self.should_fail() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Simulated tape read error",
                ));
            }

            let start_pos = self.position as usize;
            if start_pos >= self.data.len() {
                return Ok(0); // EOF
            }

            let available = self.data.len() - start_pos;
            let to_read = available.min(buf.len());

            // Read only complete blocks (tape behavior)
            let block_aligned_read = (to_read / self.block_size) * self.block_size;
            let actual_read = if block_aligned_read > 0 {
                block_aligned_read
            } else if to_read >= self.block_size {
                self.block_size
            } else {
                0 // Don't read partial blocks
            };

            if actual_read > 0 {
                buf[..actual_read].copy_from_slice(&self.data[start_pos..start_pos + actual_read]);
                self.position += actual_read as u64;
                Ok(actual_read)
            } else {
                Ok(0) // No complete block available
            }
        }
    }

    impl Seek for MockTapeDevice {
        fn seek(&mut self, pos: std::io::SeekFrom) -> Result<u64> {
            if self.should_fail() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Simulated tape seek error",
                ));
            }

            let new_pos = match pos {
                std::io::SeekFrom::Start(offset) => offset as i64,
                std::io::SeekFrom::End(offset) => self.data.len() as i64 + offset,
                std::io::SeekFrom::Current(offset) => self.position as i64 + offset,
            };

            if new_pos < 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid seek position",
                ));
            }

            self.position = new_pos as u64;
            Ok(self.position)
        }
    }

    #[test]
    fn test_mock_tape_device_creation() {
        let device = MockTapeDevice::new(512);
        assert_eq!(device.block_size, 512);
        assert_eq!(device.position, 0);
        assert!(device.data.is_empty());
        assert!(!device.simulate_errors);
    }

    #[test]
    fn test_mock_write_operations() {
        let mut device = MockTapeDevice::new(4); // Small block size for testing

        // Write partial block - should be buffered
        assert_eq!(device.write(&[1, 2]).unwrap(), 2);
        assert_eq!(device.position, 0); // Position doesn't advance until block is written
        assert_eq!(device.write_buffer.len(), 2);

        // Write enough to complete the block
        assert_eq!(device.write(&[3, 4]).unwrap(), 2);
        assert_eq!(device.position, 4); // Now position advances
        assert_eq!(device.write_buffer.len(), 0);

        // Check data was written correctly
        assert_eq!(&device.data[..4], &[1, 2, 3, 4]);

        // Test flush with partial block
        assert_eq!(device.write(&[5]).unwrap(), 1);
        device.flush().unwrap();
        assert_eq!(device.position, 8); // Another full block written with padding
        assert_eq!(&device.data[4..8], &[5, 0, 0, 0]); // Padded with zeros
    }

    #[test]
    fn test_mock_error_simulation() {
        let mut device = MockTapeDevice::new(4);

        // Enable error simulation at position 2
        device.simulate_error_at(2);

        // First write should succeed
        assert_eq!(device.write(&[1, 2, 3, 4]).unwrap(), 4);

        // Second write should fail (position is now 4 >= 2)
        assert!(device.write(&[5, 6, 7, 8]).is_err());

        // Seek should also fail
        assert!(device.seek(std::io::SeekFrom::Start(10)).is_err());

        // Read should fail
        let mut buf = [0; 4];
        assert!(device.read(&mut buf).is_err());
    }
}
