//! Shared, nonblocking mailbox cooldowns. A deferred request never reaches the network.
use crate::{NetError, Result};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const MAX_MAILBOXES: usize = 256;
const MAX_PAUSE: u64 = 24 * 60 * 60;
static PAUSES: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

pub(super) fn key(endpoint: &str, mailbox: &str) -> String {
    format!("{}\n{}", endpoint.trim_end_matches('/'), mailbox.to_ascii_lowercase())
}

fn deferred() -> NetError {
    NetError::Http {
        status: 429,
        body: "Microsoft 365 mailbox requests are paused; try again after the server cooldown.".into(),
    }
}

pub(super) fn check(key: &str) -> Result<()> {
    let pauses = PAUSES
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| NetError::Protocol("Cannot access Microsoft 365 request coordination".into()))?;
    if pauses.get(key).is_some_and(|until| *until > Instant::now()) {
        return Err(deferred());
    }
    Ok(())
}

pub(super) fn pause(key: &str, retry_after: Option<&str>) -> Result<()> {
    let now = Instant::now();
    let until = now.checked_add(delay(retry_after)).ok_or_else(deferred)?;
    let mut pauses = PAUSES
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| NetError::Protocol("Cannot access Microsoft 365 request coordination".into()))?;
    pauses.retain(|_, value| *value > now);
    if !pauses.contains_key(key) && pauses.len() >= MAX_MAILBOXES {
        return Err(NetError::TooLarge("Microsoft 365 mailbox cooldown registry".into()));
    }
    pauses.entry(key.into()).and_modify(|value| *value = (*value).max(until)).or_insert(until);
    Ok(())
}

// Graph documents Retry-After as seconds. Missing or malformed values receive a conservative pause.
fn delay(value: Option<&str>) -> Duration {
    let seconds = value
        .filter(|v| v.len() <= 20 && !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60);
    Duration::from_secs(seconds.clamp(1, MAX_PAUSE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_is_shared_and_scoped() {
        let first = key("https://fixture.invalid/v1.0/", "Shared@Example.net");
        let second = key("https://fixture.invalid/v1.0", "shared@example.net");
        pause(&first, Some("30")).unwrap();
        assert!(check(&second).is_err());
        assert!(check(&key("https://other.invalid/v1.0", "shared@example.net")).is_ok());
        assert!(check(&key("https://fixture.invalid/v1.0", "other@example.net")).is_ok());
    }

    #[test]
    fn retry_after_is_bounded() {
        assert_eq!(delay(Some("5")), Duration::from_secs(5));
        assert_eq!(delay(Some("0")), Duration::from_secs(1));
        for value in [None, Some("-1"), Some("tomorrow"), Some("999999999999999999999999")] {
            assert_eq!(delay(value), Duration::from_secs(60));
        }
        assert_eq!(delay(Some("18446744073709551615")), Duration::from_secs(MAX_PAUSE));
    }
}
