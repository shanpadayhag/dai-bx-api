use std::time::{SystemTime, UNIX_EPOCH};

pub fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the system clock is after 1970")
        .as_secs()
}
