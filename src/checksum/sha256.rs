use crate::error::Result;
use digest::Digest;
use sha2::Sha256;
use std::io::Read;

pub fn compute_checksum<R: Read>(mut reader: R) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("sha256:{:x}", result))
}

pub fn verify_checksum<R: Read>(reader: R, expected: &str) -> Result<bool> {
    let computed = compute_checksum(reader)?;
    Ok(computed == expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_compute_checksum_empty() {
        let data = b"";
        let checksum = compute_checksum(Cursor::new(data)).unwrap();
        // SHA256 of empty string
        assert_eq!(checksum, "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_compute_checksum_simple() {
        let data = b"Hello, World!";
        let checksum = compute_checksum(Cursor::new(data)).unwrap();
        // Known SHA256 of "Hello, World!"
        assert_eq!(checksum, "sha256:dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f");
    }

    #[test]
    fn test_compute_checksum_large_data() {
        // Test with data larger than the 8192 buffer
        let data = vec![42u8; 100000];
        let checksum = compute_checksum(Cursor::new(&data)).unwrap();

        // Verify it's a valid sha256 format
        assert!(checksum.starts_with("sha256:"));
        assert_eq!(checksum.len(), 71); // "sha256:" + 64 hex chars

        // Verify consistency
        let checksum2 = compute_checksum(Cursor::new(&data)).unwrap();
        assert_eq!(checksum, checksum2);
    }

    #[test]
    fn test_verify_checksum_valid() {
        let data = b"test data";
        let checksum = compute_checksum(Cursor::new(data)).unwrap();

        let is_valid = verify_checksum(Cursor::new(data), &checksum).unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_verify_checksum_invalid() {
        let data = b"test data";
        let wrong_checksum = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

        let is_valid = verify_checksum(Cursor::new(data), wrong_checksum).unwrap();
        assert!(!is_valid);
    }

    #[test]
    fn test_checksum_different_data() {
        let data1 = b"test data 1";
        let data2 = b"test data 2";

        let checksum1 = compute_checksum(Cursor::new(data1)).unwrap();
        let checksum2 = compute_checksum(Cursor::new(data2)).unwrap();

        assert_ne!(checksum1, checksum2);
    }

    #[test]
    fn test_checksum_deterministic() {
        let data = b"deterministic test";

        let checksum1 = compute_checksum(Cursor::new(data)).unwrap();
        let checksum2 = compute_checksum(Cursor::new(data)).unwrap();
        let checksum3 = compute_checksum(Cursor::new(data)).unwrap();

        assert_eq!(checksum1, checksum2);
        assert_eq!(checksum2, checksum3);
    }
}
