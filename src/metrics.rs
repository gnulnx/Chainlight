use std::collections::VecDeque;

use chrono::{DateTime, Utc};

use crate::{types::Snapshot, utils::duration};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GraphKind {
    #[default]
    Bandwidth,
    Blocks,
    Transactions,
}
impl GraphKind {
    pub fn next(self) -> Self {
        match self {
            Self::Bandwidth => Self::Blocks,
            Self::Blocks => Self::Transactions,
            Self::Transactions => Self::Bandwidth,
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Bandwidth => "BANDWIDTH",
            Self::Blocks => "BLOCKS / SEC",
            Self::Transactions => "VERIFIED TX / SEC",
        }
    }
}
#[derive(Clone, Debug)]
pub struct RateSample {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub download: Option<f64>,
    pub upload: Option<f64>,
    pub blocks: Option<f64>,
    pub transactions: Option<f64>,
}
impl RateSample {
    pub fn value(&self, kind: GraphKind) -> Option<f64> {
        match kind {
            GraphKind::Bandwidth => self.download,
            GraphKind::Blocks => self.blocks,
            GraphKind::Transactions => self.transactions,
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct Metrics {
    pub down: Option<f64>,
    pub up: Option<f64>,
    pub samples: [VecDeque<RateSample>; 3],
    pub blocks_per_second: Option<f64>,
    pub transactions_per_second: Option<f64>,
    chain_previous: Option<ChainBaseline>,
    previous: Option<(DateTime<Utc>, u64, u64, Option<u64>)>,
    progress: VecDeque<(DateTime<Utc>, f64)>,
}
#[derive(Clone, Copy, Debug)]
struct ChainBaseline {
    at: DateTime<Utc>,
    blocks: u64,
    transactions: Option<u64>,
    uptime: Option<u64>,
}

impl Metrics {
    pub fn pause(&mut self) {
        self.down = None;
        self.up = None;
        self.previous = None;
        self.chain_previous = None;
        self.blocks_per_second = None;
        self.transactions_per_second = None;
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
                } else if restart || traffic.received < received || traffic.sent < sent {
                    for history in &mut self.samples {
                        history.clear();
                    }
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

        self.blocks_per_second = None;
        self.transactions_per_second = None;
        if let Some(ChainBaseline {
            at,
            blocks,
            transactions: tx,
            uptime,
        }) = self.chain_previous
        {
            let seconds = (s.observed_at - at).num_milliseconds() as f64 / 1000.0;
            let restart = matches!((uptime, s.uptime_seconds), (Some(a), Some(b)) if b < a);
            if restart
                || s.chain.blocks < blocks
                || matches!((tx, s.verified_transactions), (Some(a), Some(b)) if b < a)
            {
                for history in &mut self.samples {
                    history.clear();
                }
            } else if seconds > 0.0 && seconds <= 15.0 {
                self.blocks_per_second = Some((s.chain.blocks - blocks) as f64 / seconds);
                self.transactions_per_second = tx
                    .zip(s.verified_transactions)
                    .map(|(a, b)| (b - a) as f64 / seconds);
                let sample = RateSample {
                    start: at,
                    end: s.observed_at,
                    download: self.down,
                    upload: self.up,
                    blocks: self.blocks_per_second,
                    transactions: self.transactions_per_second,
                };
                for kind in [
                    GraphKind::Bandwidth,
                    GraphKind::Blocks,
                    GraphKind::Transactions,
                ] {
                    if sample.value(kind).is_some() {
                        let history = &mut self.samples[kind as usize];
                        history.push_back(sample.clone());
                        while history
                            .front()
                            .is_some_and(|first| (sample.end - first.end).num_seconds() > 120)
                            || history.len() > 1024
                        {
                            history.pop_front();
                        }
                    }
                }
            }
        }
        self.chain_previous = Some(ChainBaseline {
            at: s.observed_at,
            blocks: s.chain.blocks,
            transactions: s.verified_transactions,
            uptime: s.uptime_seconds,
        });

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
        self.eta_seconds()
            .map(|remaining| format!("~{} remaining · rate may change", duration(remaining)))
    }

    pub fn eta_seconds(&self) -> Option<u64> {
        let (start, p0) = self.progress.front()?;
        let (end, p1) = self.progress.back()?;
        let seconds = (*end - *start).num_milliseconds() as f64 / 1000.0;
        if seconds < 120.0 || p1 <= p0 {
            return None;
        }
        let remaining = (1.0 - p1) * seconds / (p1 - p0);
        if remaining.is_finite() && remaining > 0.0 && remaining < 90.0 * 86400.0 {
            Some(remaining as u64)
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

    fn history(metrics: &Metrics) -> Vec<u64> {
        metrics.samples[0]
            .iter()
            .filter_map(|s| s.download)
            .map(|n| n as u64)
            .collect()
    }

    #[test]
    fn missing_sample_never_becomes_a_zero_counter_baseline() {
        let mut metrics = Metrics::default();
        metrics.update(&sample(0, Some(10_000), 0.1));
        metrics.update(&sample(2, Some(14_000), 0.1));
        assert_eq!(metrics.down, Some(2000.0));
        metrics.update(&sample(4, None, 0.1));
        assert_eq!(metrics.down, None);
        assert_eq!(history(&metrics), vec![2000]);
        metrics.update(&sample(6, Some(20_000), 0.1));
        assert_eq!(metrics.down, None);
        metrics.update(&sample(8, Some(24_000), 0.1));
        assert_eq!(metrics.down, Some(2000.0));
        assert_eq!(history(&metrics), vec![2000, 2000]);
    }

    #[test]
    fn gaps_hold_history_and_real_idle_samples_append_zero() {
        let mut metrics = Metrics::default();
        metrics.update(&sample(0, Some(10_000), 0.1));
        metrics.update(&sample(2, Some(14_000), 0.1));
        metrics.update(&sample(30, Some(50_000), 0.1));
        assert_eq!(metrics.down, None);
        assert_eq!(history(&metrics), vec![2000]);
        metrics.pause();
        metrics.update(&sample(32, Some(54_000), 0.1));
        assert_eq!(metrics.down, None);
        metrics.update(&sample(34, Some(54_000), 0.1));
        assert_eq!(metrics.down, Some(0.0));
        assert_eq!(history(&metrics), vec![2000, 0]);
    }

    #[test]
    fn restart_or_clock_rollback_resets_measurements() {
        let mut metrics = Metrics::default();
        metrics.update(&sample(0, Some(10_000), 0.1));
        metrics.update(&sample(2, Some(14_000), 0.1));
        metrics.update(&sample(4, Some(1), 0.1));
        assert_eq!(metrics.down, None);
        assert!(history(&metrics).is_empty());
        metrics.update(&sample(3, Some(20_000), 0.1));
        assert_eq!(metrics.down, None);
    }

    #[test]
    fn chain_rates_use_verified_counts_and_missing_transactions_stay_unknown() {
        let mut metrics = Metrics::default();
        let mut s = sample(0, Some(10_000), 0.1);
        s.chain.blocks = 100;
        s.verified_transactions = Some(1000);
        metrics.update(&s);
        s.observed_at += chrono::Duration::seconds(2);
        s.chain.blocks = 106;
        s.verified_transactions = Some(1200);
        metrics.update(&s);
        assert_eq!(metrics.blocks_per_second, Some(3.0));
        assert_eq!(metrics.transactions_per_second, Some(100.0));
        s.observed_at += chrono::Duration::seconds(2);
        s.verified_transactions = None;
        metrics.update(&s);
        assert_eq!(metrics.transactions_per_second, None);
        assert_eq!(metrics.samples[2].len(), 1);
        s.observed_at += chrono::Duration::seconds(2);
        s.verified_transactions = Some(1400);
        metrics.update(&s);
        assert_eq!(metrics.transactions_per_second, None);
        s.observed_at += chrono::Duration::seconds(2);
        metrics.update(&s);
        assert_eq!(metrics.transactions_per_second, Some(0.0));
        s.observed_at += chrono::Duration::seconds(2);
        s.chain.blocks = 105;
        s.verified_transactions = Some(1300);
        metrics.update(&s);
        assert_eq!(metrics.blocks_per_second, None);
        assert_eq!(metrics.transactions_per_second, None);
        assert!(metrics.samples.iter().all(|history| history.is_empty()));
    }

    #[test]
    fn long_partial_outage_holds_bandwidth_while_validation_keeps_updating() {
        let mut metrics = Metrics::default();
        metrics.update(&sample(0, Some(100), 0.1));
        metrics.update(&sample(2, Some(500), 0.1));
        let bandwidth = history(&metrics);
        for i in 2..200 {
            metrics.update(&sample(i * 2, None, 0.1));
        }
        assert_eq!(history(&metrics), bandwidth);
        assert!(metrics.samples[1].len() <= 61);
        assert_eq!(metrics.down, None);
        metrics.update(&sample(400, Some(10000), 0.1));
        assert_eq!(metrics.down, None);
        metrics.update(&sample(402, Some(10400), 0.1));
        assert_eq!(metrics.down, Some(200.0));
        assert_eq!(metrics.samples[0].len(), 1);
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
