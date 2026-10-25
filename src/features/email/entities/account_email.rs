use crate::features::email::entities::outgoing_email::OutgoingEmail;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountEmailKind {
    VerifyEmail,
    PasswordReset,
    ConfirmEmailChange,
}

struct EmailWording {
    subject: &'static str,
    purpose: &'static str,
    button_label: &'static str,
    expiry: &'static str,
    if_not_requested: &'static str,
}

impl AccountEmailKind {
    pub fn link_path(&self) -> &'static str {
        match self {
            AccountEmailKind::VerifyEmail => "/verify-email",
            AccountEmailKind::PasswordReset => "/reset-password",
            AccountEmailKind::ConfirmEmailChange => "/confirm-email",
        }
    }

    pub fn compose(&self, recipient: &str, action_link: &str) -> OutgoingEmail {
        let email_wording = self.wording();
        OutgoingEmail {
            recipient: recipient.to_owned(),
            subject: email_wording.subject.to_owned(),
            text_body: format!(
                "{}\n\n{action_link}\n\nThe link works once and expires in {}. {}\n",
                email_wording.purpose, email_wording.expiry, email_wording.if_not_requested,
            ),
            html_body: format!(
                "<!doctype html><html><body style=\"font-family:system-ui,sans-serif;color:#0c1324;line-height:1.5\">\
                 <p>{}</p>\
                 <p><a href=\"{}\" style=\"display:inline-block;padding:10px 16px;border-radius:10px;background:#2456f0;color:#ffffff;text-decoration:none\">{}</a></p>\
                 <p style=\"color:#566079\">The link works once and expires in {}. {}</p>\
                 </body></html>",
                escape_html(email_wording.purpose),
                escape_html(action_link),
                escape_html(email_wording.button_label),
                escape_html(email_wording.expiry),
                escape_html(email_wording.if_not_requested),
            ),
        }
    }

    fn wording(&self) -> EmailWording {
        match self {
            AccountEmailKind::VerifyEmail => EmailWording {
                subject: "Confirm your email for DaiBX",
                purpose: "Open this link to confirm your email address and finish setting up DaiBX.",
                button_label: "Confirm my email",
                expiry: "24 hours",
                if_not_requested: "If you didn’t sign up for DaiBX, you can ignore this email.",
            },
            AccountEmailKind::PasswordReset => EmailWording {
                subject: "Reset your DaiBX password",
                purpose: "Open this link to choose a new password for DaiBX. You’ll still need your authenticator to sign in afterwards.",
                button_label: "Choose a new password",
                expiry: "30 minutes",
                if_not_requested: "If you didn’t ask for this, you can ignore this email and your password stays the same.",
            },
            AccountEmailKind::ConfirmEmailChange => EmailWording {
                subject: "Confirm your new email for DaiBX",
                purpose: "Open this link to make this address your DaiBX sign-in email.",
                button_label: "Use this email",
                expiry: "24 hours",
                if_not_requested: "If you didn’t ask for this, you can ignore this email and nothing changes.",
            },
        }
    }
}

fn escape_html(plain_text: &str) -> String {
    plain_text
        .chars()
        .map(|text_character| match text_character {
            '&' => String::from("&amp;"),
            '<' => String::from("&lt;"),
            '>' => String::from("&gt;"),
            '"' => String::from("&quot;"),
            '\'' => String::from("&#39;"),
            other_character => other_character.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINK: &str = "https://dai-bx-web.vercel.app/verify-email?token=abc_DEF-123&x=1";

    #[test]
    fn points_each_kind_at_its_own_page() {
        assert_eq!(AccountEmailKind::VerifyEmail.link_path(), "/verify-email");
        assert_eq!(
            AccountEmailKind::PasswordReset.link_path(),
            "/reset-password"
        );
        assert_eq!(
            AccountEmailKind::ConfirmEmailChange.link_path(),
            "/confirm-email"
        );
    }

    #[test]
    fn gives_each_kind_its_own_subject_and_expiry() {
        for (email_kind, expected_subject, expected_expiry) in [
            (
                AccountEmailKind::VerifyEmail,
                "Confirm your email for DaiBX",
                "24 hours",
            ),
            (
                AccountEmailKind::PasswordReset,
                "Reset your DaiBX password",
                "30 minutes",
            ),
            (
                AccountEmailKind::ConfirmEmailChange,
                "Confirm your new email for DaiBX",
                "24 hours",
            ),
        ] {
            let outgoing_email = email_kind.compose("you@example.com", LINK);

            assert_eq!(outgoing_email.subject, expected_subject);
            assert_eq!(outgoing_email.recipient, "you@example.com");
            assert!(outgoing_email.text_body.contains(expected_expiry));
            assert!(outgoing_email.html_body.contains(expected_expiry));
        }
    }

    #[test]
    fn puts_the_link_in_the_text_body_as_is() {
        let outgoing_email = AccountEmailKind::PasswordReset.compose("you@example.com", LINK);

        assert!(outgoing_email.text_body.contains(LINK));
    }

    #[test]
    fn escapes_the_link_in_the_html_body() {
        let outgoing_email = AccountEmailKind::VerifyEmail.compose("you@example.com", LINK);

        assert!(outgoing_email.html_body.contains(
            "href=\"https://dai-bx-web.vercel.app/verify-email?token=abc_DEF-123&amp;x=1\""
        ));
        assert!(!outgoing_email.html_body.contains("&x=1"));
    }

    #[test]
    fn tells_people_who_did_not_ask_that_they_can_ignore_it() {
        for email_kind in [
            AccountEmailKind::VerifyEmail,
            AccountEmailKind::PasswordReset,
            AccountEmailKind::ConfirmEmailChange,
        ] {
            assert!(
                email_kind
                    .compose("you@example.com", LINK)
                    .text_body
                    .contains("ignore this email")
            );
        }
    }
}
