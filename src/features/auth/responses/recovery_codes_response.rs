use serde::Serialize;
use zeroize::Zeroizing;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCodesResponse<'a> {
    pub recovery_codes: Vec<&'a str>,
}

impl<'a> RecoveryCodesResponse<'a> {
    pub fn from_codes(recovery_codes: &'a [Zeroizing<String>]) -> Self {
        Self {
            recovery_codes: recovery_codes
                .iter()
                .map(|recovery_code| recovery_code.as_str())
                .collect(),
        }
    }
}
