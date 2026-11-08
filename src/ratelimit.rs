use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// Tiny fixed-window limiter keyed by string (ip + action). Good enough for login/unlock/register.
#[derive(Default)]
pub struct RateLimiter {
    inner: Mutex<HashMap<String, (u32, Instant)>>,
}

impl RateLimiter {
    pub fn check(&self, key: &str, max: u32, window: Duration) -> bool {
        let mut map = self.inner.lock().unwrap();
        let now = Instant::now();
        if map.len() > 10_000 {
            map.retain(|_, (_, start)| now.duration_since(*start) < window);
        }
        let entry = map.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(entry.1) >= window {
            *entry = (0, now);
        }
        entry.0 += 1;
        entry.0 <= max
    }
}
