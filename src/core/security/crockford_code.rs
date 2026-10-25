use zeroize::Zeroizing;

const CROCKFORD_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const BITS_PER_SYMBOL: usize = 5;
const SYMBOL_GROUP_SIZE: usize = 4;

pub fn generate_crockford_code(symbol_count: usize) -> Zeroizing<String> {
    let mut random_bytes = Zeroizing::new(vec![0_u8; (symbol_count * BITS_PER_SYMBOL).div_ceil(8)]);
    getrandom::fill(&mut random_bytes).expect("operating system randomness must be available");
    let mut generated_code = Zeroizing::new(String::with_capacity(
        symbol_count + symbol_count / SYMBOL_GROUP_SIZE,
    ));
    for symbol_index in 0..symbol_count {
        if symbol_index > 0 && symbol_index % SYMBOL_GROUP_SIZE == 0 {
            generated_code.push('-');
        }
        let alphabet_index = read_bits(
            &random_bytes,
            symbol_index * BITS_PER_SYMBOL,
            BITS_PER_SYMBOL,
        );
        generated_code.push(char::from(CROCKFORD_ALPHABET[alphabet_index]));
    }
    generated_code
}

pub fn normalize_crockford_code(
    entered_code: &str,
    symbol_count: usize,
) -> Option<Zeroizing<String>> {
    let mut normalized_code = Zeroizing::new(String::with_capacity(symbol_count));
    for entered_symbol in entered_code.chars() {
        if entered_symbol == '-' || entered_symbol.is_whitespace() {
            continue;
        }
        let canonical_symbol = match entered_symbol.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            uppercase_symbol => uppercase_symbol,
        };
        if !canonical_symbol.is_ascii() || !CROCKFORD_ALPHABET.contains(&(canonical_symbol as u8)) {
            return None;
        }
        normalized_code.push(canonical_symbol);
    }
    (normalized_code.len() == symbol_count).then_some(normalized_code)
}

fn read_bits(source_bytes: &[u8], first_bit_index: usize, bit_count: usize) -> usize {
    (first_bit_index..first_bit_index + bit_count).fold(0, |read_value, bit_index| {
        let source_bit = (source_bytes[bit_index / 8] >> (7 - bit_index % 8)) & 1;
        (read_value << 1) | usize::from(source_bit)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const CROCKFORD_SYMBOLS: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

    #[test]
    fn groups_any_length_in_fours_with_only_crockford_symbols() {
        for (symbol_count, expected_shape) in [(12, "XXXX-XXXX-XXXX"), (16, "XXXX-XXXX-XXXX-XXXX")]
        {
            let generated_code = generate_crockford_code(symbol_count);

            assert_eq!(generated_code.len(), expected_shape.len());
            assert!(
                generated_code.chars().zip(expected_shape.chars()).all(
                    |(code_symbol, shape_symbol)| (shape_symbol == '-') == (code_symbol == '-')
                )
            );
            assert!(
                generated_code
                    .chars()
                    .filter(|code_symbol| *code_symbol != '-')
                    .all(|code_symbol| CROCKFORD_SYMBOLS.contains(code_symbol))
            );
        }
    }

    #[test]
    fn uses_every_symbol_over_many_codes() {
        let seen_symbols: HashSet<char> = (0..200)
            .flat_map(|_| generate_crockford_code(16).chars().collect::<Vec<_>>())
            .filter(|code_symbol| *code_symbol != '-')
            .collect();

        assert_eq!(seen_symbols.len(), CROCKFORD_SYMBOLS.len());
    }

    #[test]
    fn normalizes_case_separators_and_aliases_to_the_given_length() {
        assert_eq!(
            normalize_crockford_code(" k7qm-2xrd 9ftb-o0il ", 16)
                .as_deref()
                .map(String::as_str),
            Some("K7QM2XRD9FTB0011")
        );
        assert!(normalize_crockford_code("K7QM-2XRD-9FTB", 16).is_none());
        assert!(normalize_crockford_code("K7QM-2XRD-9FTU-0000", 16).is_none());
    }

    #[test]
    fn normalizes_a_generated_code_back_to_itself_without_hyphens() {
        let generated_code = generate_crockford_code(16);

        assert_eq!(
            normalize_crockford_code(&generated_code, 16)
                .as_deref()
                .map(String::as_str),
            Some(generated_code.replace('-', "").as_str())
        );
    }
}
