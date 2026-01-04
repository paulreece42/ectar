use crate::error::{EctarError, Result};

/// Parse a human-readable byte size string (e.g., "1GB", "100MB") into bytes
pub fn parse_byte_size(s: &str) -> Result<u64> {
    let s = s.trim().to_uppercase();

    let (num_str, multiplier) = if let Some(stripped) = s.strip_suffix("TB") {
        (stripped, 1024u64.pow(4))
    } else if let Some(stripped) = s.strip_suffix("GB") {
        (stripped, 1024u64.pow(3))
    } else if let Some(stripped) = s.strip_suffix("MB") {
        (stripped, 1024u64.pow(2))
    } else if let Some(stripped) = s.strip_suffix("KB") {
        (stripped, 1024)
    } else if let Some(stripped) = s.strip_suffix("B") {
        (stripped, 1)
    } else {
        (&s[..], 1)
    };

    let num: u64 = num_str
        .trim()
        .parse()
        .map_err(|_| EctarError::InvalidParameters(format!("Invalid byte size: {}", s)))?;

    Ok(num * multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_byte_size() {
        assert_eq!(parse_byte_size("100").unwrap(), 100);
        assert_eq!(parse_byte_size("1KB").unwrap(), 1024);
        assert_eq!(parse_byte_size("1MB").unwrap(), 1024 * 1024);
        assert_eq!(parse_byte_size("1GB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_byte_size("1TB").unwrap(), 1024u64 * 1024 * 1024 * 1024);
        assert_eq!(parse_byte_size("100mb").unwrap(), 100 * 1024 * 1024);
    }

    #[test]
    fn test_parse_byte_size_with_b_suffix() {
        assert_eq!(parse_byte_size("100B").unwrap(), 100);
        assert_eq!(parse_byte_size("50b").unwrap(), 50);
    }

    #[test]
    fn test_parse_byte_size_with_whitespace() {
        assert_eq!(parse_byte_size("  100  ").unwrap(), 100);
        assert_eq!(parse_byte_size("  1 KB").unwrap(), 1024);
    }

    #[test]
    fn test_parse_byte_size_invalid() {
        assert!(parse_byte_size("abc").is_err());
        assert!(parse_byte_size("GB").is_err());
        assert!(parse_byte_size("100XB").is_err());
    }
}
