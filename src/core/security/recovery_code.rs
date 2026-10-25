use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::core::security::constant_time::bytes_match;
use crate::core::security::crockford_code::{generate_crockford_code, normalize_crockford_code};
use crate::core::security::random_bytes::generate_random_bytes;

pub const RECOVERY_CODE_COUNT: usize = 10;
pub const RECOVERY_CODE_SYMBOL_COUNT: usize = 12;

pub struct RecoveryCodeHash {
    pub salt: [u8; 16],
    pub code_hash: [u8; 32],
}

pub fn generate_recovery_code() -> Zeroizing<String> {
    generate_crockford_code(RECOVERY_CODE_SYMBOL_COUNT)
}

pub fn normalize_recovery_code(entered_code: &str) -> Option<Zeroizing<String>> {
    normalize_crockford_code(entered_code, RECOVERY_CODE_SYMBOL_COUNT)
}

pub fn hash_recovery_code(normalized_code: &str) -> RecoveryCodeHash {
    let salt = generate_random_bytes::<16>();
    RecoveryCodeHash {
        salt,
        code_hash: hash_with_salt(normalized_code, &salt),
    }
}

pub fn recovery_code_matches(normalized_code: &str, salt: &[u8], code_hash: &[u8]) -> bool {
    bytes_match(&hash_with_salt(normalized_code, salt), code_hash)
}

fn hash_with_salt(normalized_code: &str, salt: &[u8]) -> [u8; 32] {
    Sha256::new()
        .chain_update(salt)
        .chain_update(normalized_code.as_bytes())
        .finalize()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CROCKFORD_SYMBOLS: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

    #[test]
    fn generates_twelve_crockford_symbols_in_groups_of_four() {
        let recovery_code = generate_recovery_code();
        let symbol_groups: Vec<&str> = recovery_code.split('-').collect();

        assert_eq!(symbol_groups.len(), 3);
        assert!(
            symbol_groups
                .iter()
                .all(|symbol_group| symbol_group.len() == 4)
        );
        assert!(
            recovery_code
                .chars()
                .filter(|symbol| *symbol != '-')
                .all(|symbol| CROCKFORD_SYMBOLS.contains(symbol))
        );
    }

    #[test]
    fn generates_a_different_code_each_time() {
        assert_ne!(*generate_recovery_code(), *generate_recovery_code());
    }

    #[test]
    fn normalizes_case_separators_and_crockford_aliases() {
        let normalized_code =
            normalize_recovery_code(" k7qm-2xrd 9ftb ").expect("valid code should normalize");
        assert_eq!(normalized_code.as_str(), "K7QM2XRD9FTB");

        let aliased_code = normalize_recovery_code("o0il-o0il-o0il").expect("aliases should map");
        assert_eq!(aliased_code.as_str(), "001100110011");
    }

    #[test]
    fn refuses_codes_of_the_wrong_length_or_with_foreign_symbols() {
        assert!(normalize_recovery_code("K7QM-2XRD").is_none());
        assert!(normalize_recovery_code("K7QM-2XRD-9FTB-0").is_none());
        assert!(normalize_recovery_code("K7QM-2XRD-9FTU").is_none());
        assert!(normalize_recovery_code("K7QM-2XRD-9FT!").is_none());
    }

    #[test]
    fn matches_a_code_only_with_its_own_salt_and_hash() {
        let first_code_hash = hash_recovery_code("K7QM2XRD9FTB");
        let second_code_hash = hash_recovery_code("K7QM2XRD9FTB");

        assert_ne!(first_code_hash.salt, second_code_hash.salt);
        assert!(recovery_code_matches(
            "K7QM2XRD9FTB",
            &first_code_hash.salt,
            &first_code_hash.code_hash
        ));
        assert!(!recovery_code_matches(
            "K7QM2XRD9FTC",
            &first_code_hash.salt,
            &first_code_hash.code_hash
        ));
        assert!(!recovery_code_matches(
            "K7QM2XRD9FTB",
            &second_code_hash.salt,
            &first_code_hash.code_hash
        ));
    }

    #[test]
    fn a_generated_code_normalizes_back_to_itself_without_hyphens() {
        let recovery_code = generate_recovery_code();
        let normalized_code =
            normalize_recovery_code(&recovery_code).expect("generated code should normalize");

        assert_eq!(normalized_code.as_str(), recovery_code.replace('-', ""));
    }
}
