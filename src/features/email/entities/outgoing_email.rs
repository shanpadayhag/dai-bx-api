#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingEmail {
    pub recipient: String,
    pub subject: String,
    pub text_body: String,
    pub html_body: String,
}
