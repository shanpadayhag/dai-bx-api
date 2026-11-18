use tracing::Subscriber;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::util::SubscriberInitExt;

pub fn build_json_log_subscriber<LogWriter>(
    log_filter: EnvFilter,
    log_writer: LogWriter,
) -> impl Subscriber + Send + Sync + 'static
where
    LogWriter: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_ansi(false)
        .with_env_filter(log_filter)
        .with_writer(log_writer)
        .finish()
}

pub fn init_logging() {
    build_json_log_subscriber(EnvFilter::from_default_env(), std::io::stdout).init();
}

#[cfg(test)]
pub mod test_capture {
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    use tracing::subscriber::DefaultGuard;
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::fmt::MakeWriter;

    use super::build_json_log_subscriber;

    #[derive(Clone, Default)]
    pub struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

    impl CapturedLogs {
        pub fn text(&self) -> String {
            String::from_utf8(self.0.lock().expect("the log buffer").clone())
                .expect("logs should be UTF-8")
        }
    }

    impl Write for CapturedLogs {
        fn write(&mut self, log_bytes: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .expect("the log buffer")
                .extend_from_slice(log_bytes);
            Ok(log_bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'writer> MakeWriter<'writer> for CapturedLogs {
        type Writer = Self;

        fn make_writer(&'writer self) -> Self::Writer {
            self.clone()
        }
    }

    pub fn capture_logs() -> (CapturedLogs, DefaultGuard) {
        let captured_logs = CapturedLogs::default();
        let subscriber_guard = tracing::subscriber::set_default(build_json_log_subscriber(
            EnvFilter::new("info"),
            captured_logs.clone(),
        ));
        (captured_logs, subscriber_guard)
    }
}

#[cfg(test)]
mod tests {
    use super::test_capture::capture_logs;

    #[test]
    fn writes_one_json_line_with_a_utc_time_and_flat_fields() {
        let (captured_logs, _subscriber_guard) = capture_logs();

        tracing::info!(route = "/boards", status = 404_u16, "request refused");

        let log_text = captured_logs.text();
        assert_eq!(log_text.lines().count(), 1);
        let log_line: serde_json::Value =
            serde_json::from_str(log_text.trim()).expect("the line should be JSON");
        assert_eq!(log_line["level"], "INFO");
        assert_eq!(log_line["message"], "request refused");
        assert_eq!(log_line["route"], "/boards");
        assert_eq!(log_line["status"], 404);
        assert!(
            log_line["timestamp"]
                .as_str()
                .is_some_and(|logged_time| logged_time.ends_with('Z'))
        );
    }

    #[test]
    fn keeps_a_value_with_a_line_break_inside_one_entry() {
        let (captured_logs, _subscriber_guard) = capture_logs();

        tracing::info!(
            detail = "first\n{\"level\":\"ERROR\",\"message\":\"forged\"}",
            "logged"
        );

        let log_text = captured_logs.text();
        assert_eq!(log_text.lines().count(), 1);
        let log_line: serde_json::Value =
            serde_json::from_str(log_text.trim()).expect("the line should be JSON");
        assert_eq!(log_line["level"], "INFO");
        assert_eq!(log_line["message"], "logged");
    }

    #[test]
    fn colours_nothing() {
        let (captured_logs, _subscriber_guard) = capture_logs();

        tracing::error!("plain");

        assert!(!captured_logs.text().contains('\u{1b}'));
    }
}
