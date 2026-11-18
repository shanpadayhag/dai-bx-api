use crate::features::email::entities::account_email::escape_html;
use crate::features::email::entities::outgoing_email::OutgoingEmail;

pub const FORGOT_PASSWORD_PATH: &str = "/forgot-password";

const ALERT_SUBJECT: &str = "New sign-in to your DaiBX account";
const NEW_BROWSER_OPENING: &str =
    "Your DaiBX account was just signed in to from a browser it hasn’t seen before.";
const RECOVERY_CODE_OPENING: &str =
    "Your DaiBX account was just signed in to with a recovery code.";
const IF_IT_WAS_YOU: &str = "If this was you, you don’t need to do anything.";
const IF_IT_WAS_NOT_YOU: &str = "If it wasn’t you, reset your password now. That signs out every device. Then replace your authenticator in Settings.";
const RESET_BUTTON_LABEL: &str = "Reset my password";
const NEVER_ASKS: &str =
    "DaiBX never asks for your password or your codes by email, message or phone.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignInAlertEmail<'a> {
    pub recipient: &'a str,
    pub device_label: &'a str,
    pub signed_in_at_label: &'a str,
    pub used_recovery_code: bool,
}

impl SignInAlertEmail<'_> {
    pub fn compose(&self, forgot_password_link: &str) -> OutgoingEmail {
        let opening = if self.used_recovery_code {
            RECOVERY_CODE_OPENING
        } else {
            NEW_BROWSER_OPENING
        };
        OutgoingEmail {
            recipient: self.recipient.to_owned(),
            subject: ALERT_SUBJECT.to_owned(),
            text_body: format!(
                "{opening}\n\nDevice: {}\nTime: {}\n\n{IF_IT_WAS_YOU}\n\n{IF_IT_WAS_NOT_YOU}\n\n{forgot_password_link}\n\n{NEVER_ASKS}\n",
                self.device_label, self.signed_in_at_label,
            ),
            html_body: format!(
                "<!doctype html><html><body style=\"font-family:system-ui,sans-serif;color:#0c1324;line-height:1.5\">\
                 <p>{}</p>\
                 <p><strong>Device:</strong> {}<br><strong>Time:</strong> {}</p>\
                 <p>{}</p>\
                 <p>{}</p>\
                 <p><a href=\"{}\" style=\"display:inline-block;padding:10px 16px;border-radius:10px;background:#2456f0;color:#ffffff;text-decoration:none\">{}</a></p>\
                 <p style=\"color:#566079\">{}</p>\
                 </body></html>",
                escape_html(opening),
                escape_html(self.device_label),
                escape_html(self.signed_in_at_label),
                escape_html(IF_IT_WAS_YOU),
                escape_html(IF_IT_WAS_NOT_YOU),
                escape_html(forgot_password_link),
                escape_html(RESET_BUTTON_LABEL),
                escape_html(NEVER_ASKS),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINK: &str = "https://dai-bx.vercel.app/forgot-password";

    fn alert_email(used_recovery_code: bool) -> SignInAlertEmail<'static> {
        SignInAlertEmail {
            recipient: "you@example.com",
            device_label: "Chrome on macOS",
            signed_in_at_label: "Wed, 18 Nov 2026, 9:41 PM (Asia/Manila)",
            used_recovery_code,
        }
    }

    #[test]
    fn says_what_signed_in_when_and_what_to_do_in_both_bodies() {
        let outgoing_email = alert_email(false).compose(LINK);

        assert_eq!(outgoing_email.recipient, "you@example.com");
        assert_eq!(outgoing_email.subject, ALERT_SUBJECT);
        for email_body in [&outgoing_email.text_body, &outgoing_email.html_body] {
            assert!(email_body.contains("Chrome on macOS"));
            assert!(email_body.contains("Wed, 18 Nov 2026, 9:41 PM (Asia/Manila)"));
            assert!(email_body.contains(LINK));
            assert!(email_body.contains("never asks for your password or your codes"));
            assert!(email_body.contains("a browser it hasn"));
        }
    }

    #[test]
    fn says_when_a_recovery_code_was_used() {
        let outgoing_email = alert_email(true).compose(LINK);

        assert!(outgoing_email.text_body.contains("with a recovery code"));
        assert!(!outgoing_email.text_body.contains("a browser it hasn"));
    }

    #[test]
    fn escapes_values_in_the_html_body() {
        let outgoing_email = SignInAlertEmail {
            device_label: "<b>Chrome</b>",
            ..alert_email(false)
        }
        .compose(LINK);

        assert!(outgoing_email.html_body.contains("&lt;b&gt;Chrome"));
    }
}
