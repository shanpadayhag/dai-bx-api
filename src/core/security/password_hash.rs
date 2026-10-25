use std::sync::LazyLock;

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

use crate::core::security::opaque_token::generate_opaque_token;

pub const ARGON2_MEMORY_KIB: u32 = 19_456;
pub const ARGON2_ITERATIONS: u32 = 2;
pub const ARGON2_PARALLELISM: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordVerification {
    pub is_valid: bool,
    pub needs_rehash: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PasswordHashError {
    #[error("password hashing failed: {0}")]
    Hashing(String),
    #[error("stored password hash is malformed: {0}")]
    MalformedStoredHash(String),
    #[error(transparent)]
    BlockingTask(#[from] tokio::task::JoinError),
}

static DUMMY_PASSWORD_HASH: LazyLock<String> = LazyLock::new(|| {
    hash_password_blocking(&generate_opaque_token())
        .expect("hashing a random dummy password must succeed")
});

pub async fn hash_password(password: Zeroizing<String>) -> Result<String, PasswordHashError> {
    tokio::task::spawn_blocking(move || hash_password_blocking(&password)).await?
}

pub async fn verify_password(
    password: Zeroizing<String>,
    stored_password_hash: String,
) -> Result<PasswordVerification, PasswordHashError> {
    tokio::task::spawn_blocking(move || verify_password_blocking(&password, &stored_password_hash))
        .await?
}

pub async fn verify_against_dummy_hash(password: Zeroizing<String>) {
    let dummy_verification = tokio::task::spawn_blocking(move || {
        verify_password_blocking(&password, &DUMMY_PASSWORD_HASH)
    })
    .await;
    if let Ok(Err(dummy_hash_error)) = dummy_verification {
        tracing::error!(error = %dummy_hash_error, "dummy password verification failed");
    }
}

fn current_argon2() -> Argon2<'static> {
    let current_params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        None,
    )
    .expect("the Argon2id settings must be valid");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, current_params)
}

fn hash_password_blocking(password: &str) -> Result<String, PasswordHashError> {
    current_argon2()
        .hash_password(password.as_bytes())
        .map(|password_hash| password_hash.to_string())
        .map_err(|hashing_error| PasswordHashError::Hashing(hashing_error.to_string()))
}

fn verify_password_blocking(
    password: &str,
    stored_password_hash: &str,
) -> Result<PasswordVerification, PasswordHashError> {
    let parsed_password_hash = PasswordHash::new(stored_password_hash)
        .map_err(|parse_error| PasswordHashError::MalformedStoredHash(parse_error.to_string()))?;
    let is_valid = current_argon2()
        .verify_password(password.as_bytes(), &parsed_password_hash)
        .is_ok();
    Ok(PasswordVerification {
        is_valid,
        needs_rehash: !uses_current_settings(&parsed_password_hash),
    })
}

fn uses_current_settings(parsed_password_hash: &PasswordHash) -> bool {
    let is_argon2id = parsed_password_hash.algorithm == Algorithm::Argon2id.ident();
    let is_current_version = parsed_password_hash.version == Some(Version::V0x13.into());
    let has_current_params = Params::try_from(parsed_password_hash).is_ok_and(|stored_params| {
        stored_params.m_cost() == ARGON2_MEMORY_KIB
            && stored_params.t_cost() == ARGON2_ITERATIONS
            && stored_params.p_cost() == ARGON2_PARALLELISM
    });
    is_argon2id && is_current_version && has_current_params
}

#[cfg(test)]
mod tests {
    use super::*;

    fn password(password_text: &str) -> Zeroizing<String> {
        Zeroizing::new(password_text.to_owned())
    }

    fn hash_with_settings(password_text: &str, argon2_params: argon2::Params) -> String {
        use argon2::password_hash::PasswordHasher;

        argon2::Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2_params,
        )
        .hash_password(password_text.as_bytes())
        .expect("hashing with older settings should succeed")
        .to_string()
    }

    #[tokio::test]
    async fn hashes_with_argon2id_at_the_owasp_settings() {
        let stored_password_hash = hash_password(password("correct horse battery staple"))
            .await
            .expect("hashing should succeed");

        assert!(stored_password_hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
    }

    #[tokio::test]
    async fn verifies_the_right_password_and_refuses_a_wrong_one() {
        let stored_password_hash = hash_password(password("correct horse battery staple"))
            .await
            .expect("hashing should succeed");

        let right_verification = verify_password(
            password("correct horse battery staple"),
            stored_password_hash.clone(),
        )
        .await
        .expect("verification should run");
        let wrong_verification = verify_password(
            password("Correct horse battery staple"),
            stored_password_hash,
        )
        .await
        .expect("verification should run");

        assert!(right_verification.is_valid);
        assert!(!right_verification.needs_rehash);
        assert!(!wrong_verification.is_valid);
    }

    #[tokio::test]
    async fn verifies_a_long_unicode_password_exactly_as_entered() {
        let unicode_password = "pässwörd ✓ 密码 ".repeat(8);
        let stored_password_hash = hash_password(password(&unicode_password))
            .await
            .expect("hashing should succeed");

        let exact_verification =
            verify_password(password(&unicode_password), stored_password_hash.clone())
                .await
                .expect("verification should run");
        let trimmed_verification =
            verify_password(password(unicode_password.trim_end()), stored_password_hash)
                .await
                .expect("verification should run");

        assert!(exact_verification.is_valid);
        assert!(!trimmed_verification.is_valid);
    }

    #[tokio::test]
    async fn flags_a_valid_hash_made_with_older_settings_for_rehashing() {
        let outdated_password_hash = hash_with_settings(
            "correct horse battery staple",
            argon2::Params::new(8_192, 1, 1, None).expect("older settings should be valid"),
        );

        let outdated_verification = verify_password(
            password("correct horse battery staple"),
            outdated_password_hash,
        )
        .await
        .expect("verification should run");

        assert!(outdated_verification.is_valid);
        assert!(outdated_verification.needs_rehash);
    }

    #[tokio::test]
    async fn reports_a_malformed_stored_hash_as_an_error() {
        let verification_result =
            verify_password(password("password"), String::from("not-a-phc-string")).await;

        assert!(matches!(
            verification_result,
            Err(PasswordHashError::MalformedStoredHash(_))
        ));
    }

    #[tokio::test]
    async fn runs_a_full_verification_against_the_dummy_hash() {
        verify_against_dummy_hash(password("anything at all")).await;
    }
}
