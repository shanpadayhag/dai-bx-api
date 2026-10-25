use subtle::ConstantTimeEq;

pub fn bytes_match(left_bytes: &[u8], right_bytes: &[u8]) -> bool {
    left_bytes.ct_eq(right_bytes).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_equal_bytes_only() {
        assert!(bytes_match(b"same", b"same"));
        assert!(!bytes_match(b"same", b"sane"));
        assert!(!bytes_match(b"same", b"same-but-longer"));
        assert!(bytes_match(b"", b""));
    }
}
