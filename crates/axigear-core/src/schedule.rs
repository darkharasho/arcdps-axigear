//! When to poll. Comps: map change or every 10 min, backoff 1→2→5→10 min.
//! API: map/character change or every 5 min, backoff 30s→1→2→5 min.

use std::time::{Duration, Instant};

const fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

pub const COMP_INTERVAL: Duration = secs(600);
pub const COMP_BACKOFF: [Duration; 4] = [secs(60), secs(120), secs(300), secs(600)];
pub const API_INTERVAL: Duration = secs(300);
pub const API_BACKOFF: [Duration; 4] = [secs(30), secs(60), secs(120), secs(300)];
pub const MANUAL_REFRESH: Duration = secs(30);

#[derive(Debug, Clone)]
pub struct Poller {
    interval: Duration,
    backoff: &'static [Duration],
    last_ok: Option<Instant>,
    retry_at: Option<Instant>,
    streak: usize,
    pending: bool,
    stopped: bool,
}

impl Poller {
    pub fn new(interval: Duration, backoff: &'static [Duration]) -> Self {
        Poller { interval, backoff, last_ok: None, retry_at: None, streak: 0, pending: false, stopped: false }
    }

    /// First poll is due immediately; then on map change or after `interval`.
    /// A pending backoff always wins.
    pub fn due(&self, now: Instant) -> bool {
        if self.stopped {
            return false;
        }
        if let Some(t) = self.retry_at {
            return now >= t;
        }
        match self.last_ok {
            None => true,
            Some(t) => self.pending || now.saturating_duration_since(t) >= self.interval,
        }
    }

    pub fn note_map_change(&mut self) {
        self.pending = true;
    }

    pub fn success(&mut self, now: Instant) {
        self.last_ok = Some(now);
        self.retry_at = None;
        self.streak = 0;
        self.pending = false;
    }

    pub fn failure(&mut self, now: Instant) {
        let step = self.backoff[self.streak.min(self.backoff.len() - 1)];
        self.retry_at = Some(now + step);
        self.streak += 1;
        self.pending = false;
    }

    /// No more polls until `reset` (e.g. decrypt failure until the link changes).
    pub fn stop(&mut self) {
        self.stopped = true;
    }

    pub fn stopped(&self) -> bool {
        self.stopped
    }

    pub fn reset(&mut self) {
        *self = Poller::new(self.interval, self.backoff);
    }
}

#[derive(Debug, Clone)]
pub struct RateLimit {
    min: Duration,
    last: Option<Instant>,
}

impl RateLimit {
    pub fn new(min: Duration) -> Self {
        RateLimit { min, last: None }
    }

    pub fn try_acquire(&mut self, now: Instant) -> bool {
        if self.last.is_some_and(|t| now.saturating_duration_since(t) < self.min) {
            return false;
        }
        self.last = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_and_map_change() {
        let t0 = Instant::now();
        let mut p = Poller::new(COMP_INTERVAL, &COMP_BACKOFF);
        assert!(p.due(t0));
        p.success(t0);
        assert!(!p.due(t0 + secs(599)));
        assert!(p.due(t0 + secs(600)));
        p.note_map_change();
        assert!(p.due(t0 + secs(1)));
        p.success(t0 + secs(1));
        assert!(!p.due(t0 + secs(2)));
    }

    #[test]
    fn backoff_steps_cap_and_reset_on_success() {
        let t0 = Instant::now();
        let mut p = Poller::new(COMP_INTERVAL, &COMP_BACKOFF);
        let mut t = t0;
        for step in [60, 120, 300, 600, 600] {
            p.failure(t);
            assert!(!p.due(t + secs(step - 1)), "step {step}");
            assert!(p.due(t + secs(step)), "step {step}");
            t += secs(step);
        }
        p.success(t);
        p.failure(t);
        assert!(p.due(t + secs(60)));
    }

    #[test]
    fn backoff_beats_map_change_and_stop_halts() {
        let t0 = Instant::now();
        let mut p = Poller::new(API_INTERVAL, &API_BACKOFF);
        p.failure(t0);
        p.note_map_change();
        assert!(!p.due(t0 + secs(10)));
        p.stop();
        assert!(!p.due(t0 + secs(10_000)) && p.stopped());
        p.reset();
        assert!(p.due(t0));
    }

    #[test]
    fn manual_refresh_rate_limit() {
        let t0 = Instant::now();
        let mut r = RateLimit::new(MANUAL_REFRESH);
        assert!(r.try_acquire(t0));
        assert!(!r.try_acquire(t0 + secs(29)));
        assert!(r.try_acquire(t0 + secs(30)));
    }
}
