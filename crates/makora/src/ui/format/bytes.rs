use super::number::{js_round, to_fixed};

/// A download size: `1.23 GB`, or whole megabytes below a gigabyte.
pub fn format_bytes(bytes: u64) -> String {
    let bytes = bytes as f64;
    if bytes >= 1e9 {
        format!("{} GB", to_fixed(bytes / 1e9, 2))
    } else {
        format!("{} MB", js_round(bytes / 1e6))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn megabytes_round_to_whole() {
        assert_eq!(format_bytes(0), "0 MB");
        assert_eq!(format_bytes(499_999), "0 MB");
        assert_eq!(format_bytes(500_000), "1 MB");
        assert_eq!(format_bytes(42_400_000), "42 MB");
        assert_eq!(format_bytes(999_999_999), "1000 MB");
    }

    #[test]
    fn gigabytes_have_two_decimals() {
        assert_eq!(format_bytes(1_000_000_000), "1.00 GB");
        assert_eq!(format_bytes(4_368_439_808), "4.37 GB");
        assert_eq!(format_bytes(12_340_000_000), "12.34 GB");
    }
}
