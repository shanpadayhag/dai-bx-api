use data_encoding::BASE64URL_NOPAD;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::core::security::random_bytes::generate_random_bytes;

pub const OPAQUE_TOKEN_BYTE_COUNT: usize = 32;

pub type TokenHash = [u8; 32];

pub fn generate_opaque_token() -> Zeroizing<String> {
    let token_bytes = Zeroizing::new(generate_random_bytes::<OPAQUE_TOKEN_BYTE_COUNT>());
    Zeroizing::new(BASE64URL_NOPAD.encode(token_bytes.as_slice()))
}

pub fn hash_opaque_token(opaque_token: &str) -> TokenHash {
    Sha256::digest(opaque_token.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_32_random_bytes_as_unpadded_base64url() {
        let opaque_token = generate_opaque_token();

        assert_eq!(opaque_token.len(), 43);
        let decoded_token = BASE64URL_NOPAD
            .decode(opaque_token.as_bytes())
            .expect("token should be base64url");
        assert_eq!(decoded_token.len(), OPAQUE_TOKEN_BYTE_COUNT);
    }

    #[test]
    fn generates_a_different_token_each_time() {
        assert_ne!(*generate_opaque_token(), *generate_opaque_token());
    }

    #[test]
    fn hashes_a_token_with_sha256() {
        assert_eq!(
            data_encoding::HEXLOWER.encode(&hash_opaque_token("abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
