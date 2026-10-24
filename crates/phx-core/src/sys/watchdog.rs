use std::collections::VecDeque;
use std::time::Duration;

/// What the watchdog says about a long install.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Keep going.
    Continue,
    /// Progressing, but too slowly to be worth waiting for.
    TooSlow,
    /// No progress at all for too long.
    Stalled,
}

/// Clock-free policy over cumulative bytes-read samples, so it is testable.
#[derive(Clone, Debug)]
pub struct Watchdog {
    /// No verdict before this.
    pub grace: Duration,
    /// Rate is measured over this trailing window.
    pub window: Duration,
    /// Bytes/s; below this the window counts as slow.
    pub min_rate: f64,
    /// Continuously slow for this long: too slow.
    pub slow_for: Duration,
    /// No progress at all for this long: stalled.
    pub stall_for: Duration,
    samples: VecDeque<(Duration, u64)>,
    slow_since: Option<Duration>,
    last_move: Duration,
    last_bytes: u64,
}

impl Default for Watchdog {
    // Patient with slow mirrors and long compiles; 10 kB/s once took ~5 h for a JDK.
    fn default() -> Self {
        Self {
            grace: Duration::from_secs(45),
            window: Duration::from_secs(120),
            min_rate: 32.0 * 1024.0,
            slow_for: Duration::from_secs(300),
            stall_for: Duration::from_secs(180),
            samples: VecDeque::new(),
            slow_since: None,
            last_move: Duration::ZERO,
            last_bytes: 0,
        }
    }
}

impl Watchdog {
    /// Records cumulative `bytes` at elapsed time `t`; samples arrive in order.
    pub fn observe(&mut self, t: Duration, bytes: u64) -> Verdict {
        self.samples.push_back((t, bytes));
        if bytes > self.last_bytes {
            self.last_bytes = bytes;
            self.last_move = t;
        }
        // Keep one sample at or before the window start so the rate spans it.
        while self.samples.len() > 2 && self.samples.get(1).is_some_and(|s| s.0 + self.window <= t)
        {
            self.samples.pop_front();
        }
        if t < self.grace {
            return Verdict::Continue;
        }
        if t.saturating_sub(self.last_move) >= self.stall_for {
            return Verdict::Stalled;
        }
        let Some(&(t0, b0)) = self.samples.front() else {
            return Verdict::Continue;
        };
        if t.saturating_sub(t0) < self.window {
            return Verdict::Continue;
        }
        #[allow(clippy::cast_precision_loss, reason = "byte counts far below 2^52")]
        let rate = bytes.saturating_sub(b0) as f64 / t.saturating_sub(t0).as_secs_f64();
        if rate >= self.min_rate {
            self.slow_since = None;
            return Verdict::Continue;
        }
        let since = *self.slow_since.get_or_insert(t);
        if t.saturating_sub(since) >= self.slow_for {
            Verdict::TooSlow
        } else {
            Verdict::Continue
        }
    }

    /// Progress can't be measured at `t` (part of the install runs as root): this time
    /// counts neither as a stall nor as slow, and the rate window starts afresh after it.
    pub fn unmeasured(&mut self, t: Duration, bytes: u64) {
        self.last_move = t;
        self.last_bytes = self.last_bytes.max(bytes);
        self.slow_since = None;
        self.samples.clear();
        self.samples.push_back((t, self.last_bytes));
    }

    /// Current trailing-window rate in bytes/s.
    #[must_use]
    pub fn rate(&self) -> f64 {
        match (self.samples.front(), self.samples.back()) {
            (Some(a), Some(b)) if b.0 > a.0 => {
                #[allow(clippy::cast_precision_loss, reason = "byte counts far below 2^52")]
                let bytes = b.1.saturating_sub(a.1) as f64;
                bytes / b.0.saturating_sub(a.0).as_secs_f64()
            }
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Verdict, Watchdog};
    use crate::sys::{WatchError, interactive_watched};
    use std::time::Duration;

    fn run(total_s: u64, rate: impl Fn(u64) -> f64) -> (Verdict, u64) {
        let mut w = Watchdog::default();
        let mut bytes = 0.0_f64;
        let mut t = 0;
        while t <= total_s {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "test byte counts"
            )]
            let v = w.observe(Duration::from_secs(t), bytes as u64);
            if v != Verdict::Continue {
                return (v, t);
            }
            bytes += rate(t) * 5.0;
            t += 5;
        }
        (Verdict::Continue, total_s)
    }

    #[test]
    fn time_it_cannot_measure_is_never_a_stall() {
        let mut w = Watchdog::default();
        let mut t = 0;
        while t <= 1800 {
            if (600..=1200).contains(&t) {
                w.unmeasured(Duration::from_secs(t), 0);
            } else {
                assert_eq!(
                    w.observe(Duration::from_secs(t), t * 100_000),
                    Verdict::Continue,
                    "t={t}"
                );
            }
            t += 5;
        }
        let mut w = Watchdog::default();
        w.unmeasured(Duration::from_secs(600), 0);
        assert_eq!(
            w.observe(Duration::from_secs(700), 0),
            Verdict::Continue,
            "a quiet tick right after root work is fine"
        );
        assert_eq!(
            w.observe(Duration::from_secs(785), 0),
            Verdict::Stalled,
            "a real stall afterwards is still caught"
        );
    }

    #[test]
    fn watchdog_policy_matches_go() {
        type Rate = Box<dyn Fn(u64) -> f64>;
        let kb = 1024.0;
        let cases: [(&str, Rate, Verdict, u64); 6] = [
            (
                "fast download finishes untouched",
                Box::new(move |_| 2000.0 * kb),
                Verdict::Continue,
                0,
            ),
            (
                "the real JDK case: 10 kB/s",
                Box::new(move |_| 10.0 * kb),
                Verdict::TooSlow,
                480,
            ),
            (
                "dead connection",
                Box::new(move |t| if t < 60 { 500.0 * kb } else { 0.0 }),
                Verdict::Stalled,
                300,
            ),
            (
                "slow start then recovers",
                Box::new(move |t| if t < 180 { 5.0 * kb } else { 800.0 * kb }),
                Verdict::Continue,
                0,
            ),
            (
                "bursty compile",
                Box::new(move |t| if t % 60 < 10 { 3000.0 * kb } else { 0.0 }),
                Verdict::Continue,
                0,
            ),
            (
                "silent during grace",
                Box::new(move |t| if t < 40 { 0.0 } else { 1000.0 * kb }),
                Verdict::Continue,
                0,
            ),
        ];
        for (name, rate, want, before) in cases {
            let (v, at) = run(1800, rate);
            assert_eq!(v, want, "{name}: verdict at {at}s");
            if before > 0 {
                assert!(at <= before, "{name}: came at {at}s, want by {before}s");
            }
        }
    }

    #[test]
    fn stalled_command_is_deferred_and_its_tree_stopped() {
        let w = Watchdog {
            grace: Duration::ZERO,
            stall_for: Duration::from_millis(300),
            ..Watchdog::default()
        };
        let start = std::time::Instant::now();
        let r = interactive_watched(
            w,
            &["sh", "-c", "sleep 30 & sleep 30"],
            Duration::from_millis(100),
        );
        assert!(
            matches!(
                r,
                Err(WatchError::Deferred {
                    verdict: Verdict::Stalled,
                    ..
                })
            ),
            "{r:?}"
        );
        assert!(start.elapsed() < Duration::from_secs(5), "stopped promptly");
    }
}
