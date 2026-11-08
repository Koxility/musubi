//! Rate-Limiter, der sich an die vom Server gemeldeten Grenzen anpasst.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::{Instant, sleep};

const WINDOW: Duration = Duration::from_secs(60);
/// AniList hat zusätzlich einen Burst-Limiter.
const MIN_SPACING: Duration = Duration::from_millis(250);
/// Live-Wert zum Zeitpunkt der Entwicklung. Wird von `X-RateLimit-Limit` überschrieben.
const INITIAL_LIMIT: u32 = 30;

struct State {
    limit: u32,
    sent: VecDeque<Instant>,
    blocked_until: Option<Instant>,
}

pub struct RateLimiter {
    state: Mutex<State>,
    blocked: watch::Sender<Option<Duration>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        let (blocked, _) = watch::channel(None);
        Self {
            state: Mutex::new(State {
                limit: INITIAL_LIMIT,
                sent: VecDeque::new(),
                blocked_until: None,
            }),
            blocked,
        }
    }

    /// Meldet die Wartezeit, sobald der Server gedrosselt hat.
    pub fn subscribe(&self) -> watch::Receiver<Option<Duration>> {
        self.blocked.subscribe()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn try_acquire(&self, now: Instant) -> Option<Duration> {
        let mut state = self.lock();
        if let Some(until) = state.blocked_until {
            if until > now {
                return Some(until - now);
            }
            state.blocked_until = None;
        }
        while state
            .sent
            .front()
            .is_some_and(|t| now.duration_since(*t) >= WINDOW)
        {
            state.sent.pop_front();
        }
        if state.sent.len() as u32 >= state.limit {
            let oldest = *state.sent.front()?;
            return Some(WINDOW.saturating_sub(now.duration_since(oldest)));
        }
        if let Some(last) = state.sent.back() {
            let since = now.duration_since(*last);
            if since < MIN_SPACING {
                return Some(MIN_SPACING - since);
            }
        }
        state.sent.push_back(now);
        None
    }

    /// Wartet, bis eine Anfrage gesendet werden darf.
    pub async fn acquire(&self) {
        while let Some(wait) = self.try_acquire(Instant::now()) {
            sleep(wait).await;
        }
    }

    pub fn observe_limit(&self, limit: u32) {
        if limit > 0 {
            self.lock().limit = limit;
        }
    }

    pub fn block_for(&self, wait: Duration) {
        self.lock().blocked_until = Some(Instant::now() + wait);
        self.blocked.send_replace(Some(wait));
    }

    pub fn clear_notice(&self) {
        if self.blocked.borrow().is_some() {
            self.blocked.send_replace(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn waits_for_window_once_limit_is_reached() {
        let limiter = RateLimiter::new();
        limiter.observe_limit(2);
        let start = Instant::now();
        limiter.acquire().await;
        limiter.acquire().await;
        assert!(start.elapsed() < Duration::from_secs(1));
        limiter.acquire().await;
        assert!(start.elapsed() >= WINDOW);
    }

    #[tokio::test(start_paused = true)]
    async fn honours_server_block() {
        let limiter = RateLimiter::new();
        let notices = limiter.subscribe();
        limiter.block_for(Duration::from_secs(30));
        assert_eq!(*notices.borrow(), Some(Duration::from_secs(30)));
        let start = Instant::now();
        limiter.acquire().await;
        assert!(start.elapsed() >= Duration::from_secs(30));
    }

    #[tokio::test(start_paused = true)]
    async fn spaces_requests_against_burst_limit() {
        let limiter = RateLimiter::new();
        let start = Instant::now();
        limiter.acquire().await;
        limiter.acquire().await;
        assert!(start.elapsed() >= MIN_SPACING);
    }
}
