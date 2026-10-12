use std::collections::VecDeque;

use chrono::{DateTime, Utc};

use crate::{types::Snapshot, utils::duration};

#[derive(Clone, Default, Debug)]
pub struct Metrics {
    pub down: Option<f64>,
    pub up: Option<f64>,
    pub rates: VecDeque<u64>,
    previous: Option<(DateTime<Utc>, u64, u64, Option<u64>)>,
    progress: VecDeque<(DateTime<Utc>, f64)>,
}

impl Metrics {
    pub fn pause(&mut self) {
        self.down = None;
        self.up = None;
        self.previous = None;
    }

    pub fn update(&mut self, s: &Snapshot) {
        self.down = None;
        self.up = None;
        if let Some(traffic) = &s.traffic {
            if let Some((at, received, sent, uptime)) = self.previous {
                let seconds = (s.observed_at - at).num_milliseconds() as f64 / 1000.0;
                let restart = matches!((uptime, s.uptime_seconds), (Some(a), Some(b)) if b < a);
                if seconds > 0.0
                    && seconds <= 15.0
                    && !restart
                    && traffic.received >= received
                    && traffic.sent >= sent
                {
                    let down = (traffic.received - received) as f64 / seconds;
                    self.down = Some(down);
                    self.up = Some((traffic.sent - sent) as f64 / seconds);
                    self.rates.push_back(down as u64);
                } else if restart || traffic.received < received || traffic.sent < sent {
                    self.rates.clear();
                }
            }
            self.previous = Some((
                s.observed_at,
                traffic.received,
                traffic.sent,
                s.uptime_seconds,
            ));
        } else {
            self.previous = None;
        }
        while self.rates.len() > 90 {
            self.rates.pop_front();
        }

        let p = s.chain.verificationprogress;
        if !s.chain.initialblockdownload || !p.is_finite() || !(0.0..=1.0).contains(&p) {
            self.progress.clear();
            return;
        }
        if let Some((at, old)) = self.progress.back()
            && (s.observed_at <= *at || (s.observed_at - *at).num_seconds() > 15 || p < *old)
        {
            self.progress.clear();
        }
        self.progress.push_back((s.observed_at, p));
        while self.progress.len() > 150 {
            self.progress.pop_front();
        }
    }

    pub fn eta(&self) -> Option<String> {
        let (start, p0) = self.progress.front()?;
        let (end, p1) = self.progress.back()?;
        let seconds = (*end - *start).num_milliseconds() as f64 / 1000.0;
        if seconds < 120.0 || p1 <= p0 {
            return None;
        }
        let remaining = (1.0 - p1) * seconds / (p1 - p0);
        if remaining.is_finite() && remaining > 0.0 && remaining < 90.0 * 86400.0 {
            Some(format!(
                "~{} remaining · rate may change",
                duration(remaining as u64)
            ))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChainInfo, Traffic};

    fn sample(seconds: i64, received: Option<u64>, progress: f64) -> Snapshot {
        Snapshot {
            observed_at: DateTime::from_timestamp(1_790_000_000 + seconds, 0).unwrap(),
            traffic: received.map(|received| Traffic {
                received,
                sent: 100,
            }),
            chain: ChainInfo {
                initialblockdownload: true,
                verificationprogress: progress,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn missing_sample_never_becomes_a_zero_counter_baseline() {
        let mut metrics = Metrics::default();
        metrics.update(&sample(0, Some(10_000), 0.1));
        metrics.update(&sample(2, Some(14_000), 0.1));
        assert_eq!(metrics.down, Some(2000.0));
        metrics.update(&sample(4, None, 0.1));
        assert_eq!(metrics.down, None);
        assert_eq!(metrics.rates, VecDeque::from([2000]));
        metrics.update(&sample(6, Some(20_000), 0.1));
        assert_eq!(metrics.down, None);
        metrics.update(&sample(8, Some(24_000), 0.1));
        assert_eq!(metrics.down, Some(2000.0));
        assert_eq!(metrics.rates, VecDeque::from([2000, 2000]));
    }

    #[test]
    fn gaps_hold_history_and_real_idle_samples_append_zero() {
        let mut metrics = Metrics::default();
        metrics.update(&sample(0, Some(10_000), 0.1));
        metrics.update(&sample(2, Some(14_000), 0.1));
        metrics.update(&sample(30, Some(50_000), 0.1));
        assert_eq!(metrics.down, None);
        assert_eq!(metrics.rates, VecDeque::from([2000]));
        metrics.pause();
        metrics.update(&sample(32, Some(54_000), 0.1));
        assert_eq!(metrics.down, None);
        metrics.update(&sample(34, Some(54_000), 0.1));
        assert_eq!(metrics.down, Some(0.0));
        assert_eq!(metrics.rates, VecDeque::from([2000, 0]));
    }

    #[test]
    fn restart_or_clock_rollback_resets_measurements() {
        let mut metrics = Metrics::default();
        metrics.update(&sample(0, Some(10_000), 0.1));
        metrics.update(&sample(2, Some(14_000), 0.1));
        metrics.update(&sample(4, Some(1), 0.1));
        assert_eq!(metrics.down, None);
        assert!(metrics.rates.is_empty());
        metrics.update(&sample(3, Some(20_000), 0.1));
        assert_eq!(metrics.down, None);
    }

    #[test]
    fn eta_requires_continuous_measurements_and_rejects_discontinuity() {
        let mut metrics = Metrics::default();
        for i in 0..60 {
            metrics.update(&sample(
                i * 2,
                Some(i as u64 * 10_000),
                0.1 + i as f64 * 0.001,
            ));
        }
        assert!(metrics.eta().is_none());
        metrics.update(&sample(120, Some(600_000), 0.16));
        assert!(metrics.eta().unwrap().contains("rate may change"));
        metrics.update(&sample(900, Some(900_000), 0.4));
        assert!(metrics.eta().is_none());
        metrics.update(&sample(902, Some(910_000), 0.01));
        assert!(metrics.eta().is_none());
    }
}
