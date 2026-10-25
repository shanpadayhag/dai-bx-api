use std::collections::HashSet;
use std::sync::LazyLock;

use data_encoding::{HEXLOWER, HEXUPPER};
use sha1::{Digest, Sha1};
use sha2::Sha256;

pub const MIN_PASSWORD_CHARACTERS: usize = 15;
pub const MAX_PASSWORD_CHARACTERS: usize = 128;
const SERVICE_NAME_WORD: &str = "daibx";
const MIN_PERSONAL_WORD_CHARACTERS: usize = 4;

static COMMON_PASSWORD_SHA256S: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    include_str!("common_password_sha256s.txt")
        .lines()
        .map(str::trim)
        .filter(|password_digest| !password_digest.is_empty())
        .collect()
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordRejection {
    TooShort,
    TooLong,
    Common,
    ContainsPersonalWord,
    Breached,
}

#[derive(Debug, Clone, Copy)]
pub struct PasswordContext<'a> {
    pub person_name: &'a str,
    pub email: &'a str,
}

pub struct NewPassword<'a> {
    password: &'a str,
}

impl<'a> NewPassword<'a> {
    pub fn new(password: &'a str) -> Self {
        Self { password }
    }

    pub fn check_local_rules(
        &self,
        password_context: PasswordContext<'_>,
    ) -> Result<(), PasswordRejection> {
        let password_character_count = self.password.chars().count();
        if password_character_count < MIN_PASSWORD_CHARACTERS {
            return Err(PasswordRejection::TooShort);
        }
        if password_character_count > MAX_PASSWORD_CHARACTERS {
            return Err(PasswordRejection::TooLong);
        }
        let lowercase_password = self.password.to_lowercase();
        let lowercase_password_sha256 =
            HEXLOWER.encode(&Sha256::digest(lowercase_password.as_bytes()));
        if COMMON_PASSWORD_SHA256S.contains(lowercase_password_sha256.as_str()) {
            return Err(PasswordRejection::Common);
        }
        if personal_words(password_context)
            .any(|personal_word| lowercase_password.contains(&personal_word))
        {
            return Err(PasswordRejection::ContainsPersonalWord);
        }
        Ok(())
    }

    pub fn sha1_hex(&self) -> String {
        HEXUPPER.encode(&Sha1::digest(self.password.as_bytes()))
    }
}

fn personal_words(password_context: PasswordContext<'_>) -> impl Iterator<Item = String> + '_ {
    std::iter::once(SERVICE_NAME_WORD.to_owned()).chain(
        [password_context.person_name, password_context.email]
            .into_iter()
            .flat_map(|personal_text| {
                personal_text.split(|text_character: char| !text_character.is_alphanumeric())
            })
            .filter(|personal_word| personal_word.chars().count() >= MIN_PERSONAL_WORD_CHARACTERS)
            .map(str::to_lowercase),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTEXT: PasswordContext<'static> = PasswordContext {
        person_name: "Shan Padayhag",
        email: "shan.p@example.com",
    };

    fn check(password: &str) -> Result<(), PasswordRejection> {
        NewPassword::new(password).check_local_rules(CONTEXT)
    }

    #[test]
    fn needs_at_least_15_characters_counted_as_characters_not_bytes() {
        assert_eq!(check("fourteen chars"), Err(PasswordRejection::TooShort));
        assert_eq!(check("fifteen chars!!"), Ok(()));
        assert_eq!(check("ñöüçåøæß€£¥©®™"), Err(PasswordRejection::TooShort));
        assert_eq!(check("ñöüçåøæß€£¥©®™✓"), Ok(()));
    }

    #[test]
    fn allows_up_to_128_characters_of_anything_and_refuses_more() {
        let unicode_password = "密码✓ é ".repeat(32);

        assert_eq!(unicode_password.chars().count(), 192);
        assert_eq!(
            check(&unicode_password.chars().take(128).collect::<String>()),
            Ok(())
        );
        assert_eq!(check(&unicode_password), Err(PasswordRejection::TooLong));
    }

    #[test]
    fn has_no_composition_rules() {
        assert_eq!(check("all lowercase words here"), Ok(()));
        assert_eq!(check("                    "), Ok(()));
    }

    #[test]
    fn refuses_a_common_password_in_any_case() {
        assert_eq!(check("passwordpassword"), Err(PasswordRejection::Common));
        assert_eq!(
            check("CorrectHorseBatteryStaple"),
            Err(PasswordRejection::Common)
        );
        assert_eq!(
            check("QWERTYUIOP1234567890"),
            Err(PasswordRejection::Common)
        );
        assert_eq!(check("worldofwarcraft"), Err(PasswordRejection::Common));
    }

    #[test]
    fn bundles_only_sha256_digests_of_the_common_passwords() {
        assert!(COMMON_PASSWORD_SHA256S.len() > 300);
        assert!(COMMON_PASSWORD_SHA256S.iter().all(|password_digest| {
            password_digest.len() == 64
                && password_digest
                    .bytes()
                    .all(|digest_byte| digest_byte.is_ascii_hexdigit())
        }));
    }

    #[test]
    fn refuses_the_service_name_and_the_persons_own_words() {
        assert_eq!(
            check("my daibx login phrase"),
            Err(PasswordRejection::ContainsPersonalWord)
        );
        assert_eq!(
            check("hello shan how are you"),
            Err(PasswordRejection::ContainsPersonalWord)
        );
        assert_eq!(
            check("PADAYHAG forever and ever"),
            Err(PasswordRejection::ContainsPersonalWord)
        );
        assert_eq!(
            check("my example domain phrase"),
            Err(PasswordRejection::ContainsPersonalWord)
        );
    }

    #[test]
    fn ignores_personal_words_shorter_than_four_characters() {
        let short_context = PasswordContext {
            person_name: "Al Yu",
            email: "al@ab.io",
        };

        assert_eq!(
            NewPassword::new("al yu ab io are short words").check_local_rules(short_context),
            Ok(())
        );
    }

    #[test]
    fn hashes_with_sha1_in_uppercase_hex_for_the_breach_lookup() {
        assert_eq!(
            NewPassword::new("password").sha1_hex(),
            "5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8"
        );
    }
}
