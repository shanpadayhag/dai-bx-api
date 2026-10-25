use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::core::security::crockford_code::{generate_crockford_code, normalize_crockford_code};

pub const INVITE_CODE_SYMBOL_COUNT: usize = 16;
pub const INVITE_VALIDITY_SECONDS: i64 = 7 * 24 * 60 * 60;

pub struct CreatedInvite {
    pub invite_code: Zeroizing<String>,
    pub expires_at_utc: String,
}

pub fn generate_invite_code() -> Zeroizing<String> {
    generate_crockford_code(INVITE_CODE_SYMBOL_COUNT)
}

pub fn normalize_invite_code(entered_code: &str) -> Option<Zeroizing<String>> {
    normalize_crockford_code(entered_code, INVITE_CODE_SYMBOL_COUNT)
}

pub fn hash_invite_code(normalized_code: &str) -> [u8; 32] {
    Sha256::digest(normalized_code.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_sixteen_symbols_in_groups_of_four() {
        let invite_code = generate_invite_code();

        assert_eq!(invite_code.len(), 19);
        assert_eq!(invite_code.matches('-').count(), 3);
    }

    #[test]
    fn hashes_the_same_code_the_same_however_it_was_typed() {
        let normalized_typed_code =
            normalize_invite_code("k7qm 2xrd 9ftb o0il").expect("typed code should normalize");
        let normalized_pasted_code =
            normalize_invite_code("K7QM-2XRD-9FTB-0011").expect("pasted code should normalize");

        assert_eq!(
            hash_invite_code(&normalized_typed_code),
            hash_invite_code(&normalized_pasted_code)
        );
    }
}
