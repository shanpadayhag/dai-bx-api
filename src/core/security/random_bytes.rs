pub fn generate_random_bytes<const BYTE_COUNT: usize>() -> [u8; BYTE_COUNT] {
    let mut random_bytes = [0_u8; BYTE_COUNT];
    getrandom::fill(&mut random_bytes).expect("operating system randomness must be available");
    random_bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_different_bytes_each_time() {
        assert_ne!(generate_random_bytes::<32>(), generate_random_bytes::<32>());
    }
}
