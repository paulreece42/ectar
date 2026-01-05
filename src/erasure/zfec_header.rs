use crate::error::{EctarError, Result};

/// Zfec-compatible header for shard files
///
/// This header makes ectar shards compatible with the zunfec tool
/// from the zfec/tahoe-lafs project. The header is 2-4 bytes and
/// contains the erasure coding parameters.
///
/// Header format (variable length, big-endian):
/// - m-1 (total shares): 8 bits
/// - k-1 (required shares): log_ceil(m, 2) bits
/// - padlen (padding bytes): log_ceil(k, 2) bits
/// - sharenum (shard index): log_ceil(m, 2) bits
///
/// Header size: 2, 3, or 4 bytes depending on parameters
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZfecHeader {
    /// Number of data shards required for reconstruction (k)
    pub k: u8,
    /// Total number of shards (data + parity) (m)
    pub m: u8,
    /// This shard's index (0 to m-1)
    pub sharenum: u8,
    /// Number of padding bytes in the last block
    pub padlen: usize,
}

impl ZfecHeader {
    /// Create a new zfec header
    pub fn new(k: u8, m: u8, sharenum: u8, padlen: usize) -> Result<Self> {
        if k == 0 || m == 0 {
            return Err(EctarError::InvalidParameters(
                "k and m must be non-zero".to_string(),
            ));
        }
        if k > m {
            return Err(EctarError::InvalidParameters(
                "k must be <= m".to_string(),
            ));
        }
        if sharenum >= m {
            return Err(EctarError::InvalidParameters(
                format!("sharenum {} must be < m {}", sharenum, m),
            ));
        }

        // Validate padlen fits in allocated bits
        let pad_bits = log2_ceil(k as usize);
        let max_padlen = (1usize << pad_bits) - 1;
        if padlen > max_padlen {
            return Err(EctarError::InvalidParameters(
                format!("padlen {} exceeds maximum {} for k={} ({} bits)",
                        padlen, max_padlen, k, pad_bits),
            ));
        }

        Ok(Self {
            k,
            m,
            sharenum,
            padlen,
        })
    }

    /// Calculate the header size in bytes for given m
    pub fn size(m: u8) -> usize {
        let k_bits = log2_ceil(m as usize);
        let pad_bits = log2_ceil(m as usize); // Conservative: use m for k upper bound
        let sharenum_bits = log2_ceil(m as usize);

        let total_bits = 8 + k_bits + pad_bits + sharenum_bits;
        (total_bits + 7) / 8 // ceiling division
    }

    /// Encode this header into bytes (zfec format)
    pub fn encode(&self) -> Vec<u8> {
        let k_bits = log2_ceil(self.m as usize);
        let pad_bits = log2_ceil(self.k as usize);
        let sharenum_bits = log2_ceil(self.m as usize);

        let total_bits = 8 + k_bits + pad_bits + sharenum_bits;
        let num_bytes = (total_bits + 7) / 8;

        // Build the packed value (left to right: m-1, k-1, padlen, sharenum)
        let mut value: u32 = 0;
        let mut shift = total_bits;

        // m-1 (8 bits)
        shift -= 8;
        value |= ((self.m - 1) as u32) << shift;

        // k-1 (k_bits)
        shift -= k_bits;
        value |= ((self.k - 1) as u32) << shift;

        // padlen (pad_bits)
        shift -= pad_bits;
        value |= (self.padlen as u32) << shift;

        // sharenum (sharenum_bits)
        value |= self.sharenum as u32;

        // Serialize as big-endian, MSB-aligned
        match num_bytes {
            2 => (value as u16).to_be_bytes().to_vec(),
            3 => {
                // Left-shift to align to MSB of 3 bytes (24 bits)
                let shift_amount = (num_bytes * 8) - total_bits;
                let aligned_value = value << shift_amount;
                let bytes = aligned_value.to_be_bytes();
                bytes[1..].to_vec() // skip first byte of u32
            }
            4 => value.to_be_bytes().to_vec(),
            _ => unreachable!("Header size must be 2, 3, or 4 bytes"),
        }
    }

    /// Decode a zfec header from bytes
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 2 || bytes.len() > 4 {
            return Err(EctarError::InvalidHeader(format!(
                "Invalid zfec header size: {} bytes (expected 2-4)",
                bytes.len()
            )));
        }

        // Convert bytes to u32 (big-endian)
        let value: u32 = match bytes.len() {
            2 => u16::from_be_bytes([bytes[0], bytes[1]]) as u32,
            3 => u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]),
            4 => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            _ => unreachable!(),
        };

        let total_bytes = bytes.len();

        // Extract m from first 8 bits (MSB-aligned in the byte array)
        let m_minus_1 = ((value >> (total_bytes * 8 - 8)) & 0xFF) as u8;
        let m = m_minus_1.checked_add(1)
            .ok_or_else(|| EctarError::InvalidHeader(
                "m value overflow (m-1 = 255)".to_string()
            ))?;

        if m == 0 {
            return Err(EctarError::InvalidHeader(
                "Invalid m value: 0".to_string()
            ));
        }

        // Calculate bit widths
        let k_bits = log2_ceil(m as usize);
        let sharenum_bits = log2_ceil(m as usize);

        // Extract k-1 from next k_bits
        let k_shift = total_bytes * 8 - 8 - k_bits;
        let k_mask = (1u32 << k_bits) - 1;
        let k_minus_1 = ((value >> k_shift) & k_mask) as u8;
        let k = k_minus_1.checked_add(1)
            .ok_or_else(|| EctarError::InvalidHeader(
                "k value overflow".to_string()
            ))?;

        if k == 0 || k > m {
            return Err(EctarError::InvalidHeader(format!(
                "Invalid k value: {} (m={})",
                k, m
            )));
        }

        // Now we know k, calculate pad_bits and validate header size
        let pad_bits = log2_ceil(k as usize);
        let expected_total_bits = 8 + k_bits + pad_bits + sharenum_bits;
        let expected_bytes = (expected_total_bits + 7) / 8;

        if expected_bytes != bytes.len() {
            return Err(EctarError::InvalidHeader(format!(
                "Header size mismatch: expected {} bytes for m={}, k={}, got {}",
                expected_bytes, m, k, bytes.len()
            )));
        }

        // Calculate the padding bits (unused bits in the header)
        let padding_bits = (total_bytes * 8) - expected_total_bits;

        // Extract padlen (after accounting for padding)
        let padlen_shift = sharenum_bits + padding_bits;
        let pad_mask = (1u32 << pad_bits) - 1;
        let padlen = ((value >> padlen_shift) & pad_mask) as usize;

        // Extract sharenum from the lowest bits (after padding)
        let sharenum_mask = (1u32 << sharenum_bits) - 1;
        let sharenum = ((value >> padding_bits) & sharenum_mask) as u8;

        if sharenum >= m {
            return Err(EctarError::InvalidHeader(format!(
                "Invalid sharenum: {} >= m {}",
                sharenum, m
            )));
        }

        Ok(Self {
            k,
            m,
            sharenum,
            padlen,
        })
    }

    /// Try to decode a zfec header, returning None if not a valid header
    /// This is used for backward compatibility detection
    pub fn try_decode(bytes: &[u8]) -> Option<Self> {
        Self::decode(bytes).ok()
    }
}

/// Calculate ceiling of log2(n)
/// Returns the number of bits needed to represent values 0..n-1
fn log2_ceil(n: usize) -> usize {
    if n <= 1 {
        return 0;
    }
    let mut bits = 0;
    let mut value = n - 1;
    while value > 0 {
        bits += 1;
        value >>= 1;
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log2_ceil() {
        assert_eq!(log2_ceil(1), 0);
        assert_eq!(log2_ceil(2), 1);
        assert_eq!(log2_ceil(3), 2);
        assert_eq!(log2_ceil(4), 2);
        assert_eq!(log2_ceil(5), 3);
        assert_eq!(log2_ceil(8), 3);
        assert_eq!(log2_ceil(16), 4);
        assert_eq!(log2_ceil(256), 8);
    }

    #[test]
    fn test_header_size() {
        assert_eq!(ZfecHeader::size(3), 2); // 8 + 2 + 2 + 2 = 14 bits -> 2 bytes
        assert_eq!(ZfecHeader::size(16), 3); // 8 + 4 + 4 + 4 = 20 bits -> 3 bytes
        assert_eq!(ZfecHeader::size(255), 4); // 8 + 8 + 8 + 8 = 32 bits -> 4 bytes
    }

    #[test]
    fn test_encode_decode_simple() {
        // k=3 allows pad_bits=2, max_padlen=3
        let header = ZfecHeader::new(3, 5, 2, 2).unwrap();
        let encoded = header.encode();
        let decoded = ZfecHeader::decode(&encoded).unwrap();

        assert_eq!(header, decoded);
    }

    #[test]
    fn test_encode_decode_ectar_params() {
        // Typical ectar parameters
        // k=10 allows pad_bits=4, max_padlen=15
        let header = ZfecHeader::new(10, 15, 7, 9).unwrap();
        let encoded = header.encode();
        let decoded = ZfecHeader::decode(&encoded).unwrap();

        assert_eq!(header.k, decoded.k);
        assert_eq!(header.m, decoded.m);
        assert_eq!(header.sharenum, decoded.sharenum);
        assert_eq!(header.padlen, decoded.padlen);
    }

    #[test]
    fn test_encode_decode_max_params() {
        // Maximum parameters (m=255, since it's u8)
        // k=200 allows pad_bits=8, max_padlen=255
        let header = ZfecHeader::new(200, 255, 199, 199).unwrap();
        let encoded = header.encode();
        assert_eq!(encoded.len(), 4); // Should be 4 bytes

        let decoded = ZfecHeader::decode(&encoded).unwrap();
        assert_eq!(header, decoded);
    }

    #[test]
    fn test_invalid_parameters() {
        assert!(ZfecHeader::new(0, 5, 0, 0).is_err()); // k = 0
        assert!(ZfecHeader::new(5, 0, 0, 0).is_err()); // m = 0
        assert!(ZfecHeader::new(10, 5, 0, 0).is_err()); // k > m
        assert!(ZfecHeader::new(5, 10, 10, 0).is_err()); // sharenum >= m

        // Invalid padlen (exceeds bit allocation)
        assert!(ZfecHeader::new(3, 5, 0, 7).is_err()); // k=3 allows max padlen=3, not 7
        assert!(ZfecHeader::new(10, 15, 0, 42).is_err()); // k=10 allows max padlen=15, not 42
        assert!(ZfecHeader::new(200, 255, 0, 1023).is_err()); // k=200 allows max padlen=255, not 1023
    }

    #[test]
    fn test_try_decode_invalid() {
        // Random bytes that aren't a valid header
        let invalid = vec![0xFF, 0xFF, 0xFF, 0xFF];
        assert!(ZfecHeader::try_decode(&invalid).is_none());

        // Too short
        assert!(ZfecHeader::try_decode(&[0x00]).is_none());

        // Too long
        assert!(ZfecHeader::try_decode(&[0x00, 0x00, 0x00, 0x00, 0x00]).is_none());
    }
}
