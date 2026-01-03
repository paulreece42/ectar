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
