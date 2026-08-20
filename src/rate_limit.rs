//! Server-side fixed-window rate limiting for MCP requests.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct Window {
    started: Instant,
    requests: u32,
}

#[derive(Debug)]
pub struct RateLimiter {
    max_requests: u32,
    window: Duration,
    clients: Mutex<HashMap<String, Window>>,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window: Duration) -> Option<Self> {
        if max_requests == 0 || window.is_zero() {
            return None;
        }
        Some(Self {
            max_requests,
            window,
            clients: Mutex::new(HashMap::new()),
        })
    }

    pub fn allow(&self, client_id: &str) -> bool {
        let now = Instant::now();
        let mut clients = match self.clients.lock() {
            Ok(clients) => clients,
            Err(poisoned) => poisoned.into_inner(),
        };
        let entry = clients.entry(client_id.to_string()).or_insert(Window {
            started: now,
            requests: 0,
        });

        if now.duration_since(entry.started) >= self.window {
            entry.started = now;
            entry.requests = 0;
        }
        if entry.requests >= self.max_requests {
            return false;
        }
        entry.requests += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_limits() {
        assert!(RateLimiter::new(0, Duration::from_secs(1)).is_none());
        assert!(RateLimiter::new(1, Duration::ZERO).is_none());
    }

    #[test]
    fn limits_each_client_independently() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60)).expect("valid limiter");
        assert!(limiter.allow("client-a"));
        assert!(limiter.allow("client-a"));
        assert!(!limiter.allow("client-a"));
        assert!(limiter.allow("client-b"));
    }

    #[test]
    fn starts_a_new_window_after_expiry() {
        let limiter = RateLimiter::new(1, Duration::from_millis(1)).expect("valid limiter");
        assert!(limiter.allow("client-a"));
        assert!(!limiter.allow("client-a"));
        std::thread::sleep(Duration::from_millis(2));
        assert!(limiter.allow("client-a"));
    }
}
