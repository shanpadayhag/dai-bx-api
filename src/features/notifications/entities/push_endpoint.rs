use reqwest::Url;

pub const MAX_PUSH_ENDPOINT_LENGTH: usize = 1000;

const EXACT_PUSH_HOSTS: [&str; 2] = ["fcm.googleapis.com", "updates.push.services.mozilla.com"];
const PUSH_HOST_SUFFIXES: [&str; 2] = [".push.apple.com", ".notify.windows.com"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushEndpoint(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the push endpoint must be an https address on a known push service")]
pub struct InvalidPushEndpoint;

impl PushEndpoint {
    pub fn parse(endpoint_text: &str) -> Result<Self, InvalidPushEndpoint> {
        if endpoint_text.len() > MAX_PUSH_ENDPOINT_LENGTH {
            return Err(InvalidPushEndpoint);
        }
        let endpoint_url = Url::parse(endpoint_text).map_err(|_| InvalidPushEndpoint)?;
        let is_plain_https = endpoint_url.scheme() == "https"
            && endpoint_url.username().is_empty()
            && endpoint_url.password().is_none()
            && endpoint_url.port().is_none();
        let is_known_push_host = endpoint_url.domain().is_some_and(is_known_push_host);
        if !is_plain_https || !is_known_push_host {
            return Err(InvalidPushEndpoint);
        }
        Ok(Self(endpoint_text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_known_push_host(endpoint_host: &str) -> bool {
    EXACT_PUSH_HOSTS.contains(&endpoint_host)
        || PUSH_HOST_SUFFIXES.iter().any(|host_suffix| {
            endpoint_host
                .strip_suffix(host_suffix)
                .is_some_and(|host_label| !host_label.is_empty() && !host_label.ends_with('.'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_each_push_service() {
        for endpoint_text in [
            "https://fcm.googleapis.com/fcm/send/abc123:APA91b",
            "https://web.push.apple.com/QGuQyavXutnMH2mdyRqFk2",
            "https://updates.push.services.mozilla.com/wpush/v2/gAAAAA",
            "https://wns2-par02p.notify.windows.com/w/?token=BQYAAA",
        ] {
            assert!(
                PushEndpoint::parse(endpoint_text).is_ok(),
                "{endpoint_text} should be accepted"
            );
        }
    }

    #[test]
    fn rejects_plain_http_ip_literals_logins_and_ports() {
        for endpoint_text in [
            "http://fcm.googleapis.com/fcm/send/abc",
            "https://142.250.0.1/fcm/send/abc",
            "https://[::1]/push",
            "https://user:secret@fcm.googleapis.com/fcm/send/abc",
            "https://fcm.googleapis.com:8443/fcm/send/abc",
            "not a url",
        ] {
            assert_eq!(
                PushEndpoint::parse(endpoint_text),
                Err(InvalidPushEndpoint),
                "{endpoint_text} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_look_alike_hosts() {
        for endpoint_text in [
            "https://push.apple.com.evil.example/abc",
            "https://evilfcm.googleapis.com/fcm/send/abc",
            "https://evilpush.apple.com/abc",
            "https://push.apple.com/abc",
            "https://localhost/push",
            "https://169.254.169.254/latest/meta-data",
        ] {
            assert_eq!(
                PushEndpoint::parse(endpoint_text),
                Err(InvalidPushEndpoint),
                "{endpoint_text} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_an_endpoint_longer_than_the_limit() {
        let long_endpoint = format!(
            "https://fcm.googleapis.com/fcm/send/{}",
            "a".repeat(MAX_PUSH_ENDPOINT_LENGTH)
        );

        assert_eq!(
            PushEndpoint::parse(&long_endpoint),
            Err(InvalidPushEndpoint)
        );
    }
}
