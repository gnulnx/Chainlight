use crate::{
    metrics::{GraphKind, Metrics},
    types::Snapshot,
    utils::number,
};
use chrono::{DateTime, Utc};
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct NodeEvent {
    pub at: DateTime<Utc>,
    pub kind: &'static str,
    pub detail: String,
}

#[derive(Default)]
pub struct App {
    pub current: Option<Snapshot>,
    pub error: Option<String>,
    pub metrics: Metrics,
    pub tab: usize,
    pub help: bool,
    pub graph: GraphKind,
    pub show_upload: bool,
    pub events: VecDeque<NodeEvent>,
    pub progress_bar: bool,
}

impl App {
    pub fn stale(&self) -> bool {
        self.error.is_some()
            || self
                .current
                .as_ref()
                .is_some_and(|s| (Utc::now() - s.observed_at).num_seconds() > 15)
    }

    pub fn apply(&mut self, result: Result<Snapshot, String>) {
        match result {
            Ok(s) => {
                self.observe(&s);
                self.metrics.update(&s);
                self.current = Some(s);
                self.error = None;
            }
            Err(message) => {
                if self.error.is_none() {
                    self.record(
                        Utc::now(),
                        "CONNECTION LOST",
                        "Holding the last observation; retrying".into(),
                    );
                }
                self.metrics.pause();
                self.error = Some(message);
            }
        }
    }
    fn record(&mut self, at: DateTime<Utc>, kind: &'static str, detail: String) {
        if kind == "BLOCKS CHECKED" && self.events.back().is_some_and(|e| e.kind == kind) {
            self.events.pop_back();
        }
        self.events.push_back(NodeEvent { at, kind, detail });
        while self.events.len() > 32 {
            self.events.pop_front();
        }
    }
    fn observe(&mut self, s: &Snapshot) {
        if self.error.is_some() {
            self.record(
                s.observed_at,
                "RECONNECTED",
                "Local Bitcoin Core RPC available".into(),
            );
        }
        let Some(old) = self.current.clone() else {
            self.record(
                s.observed_at,
                "NODE CONNECTED",
                format!(
                    "{} · verified tip #{}",
                    s.chain.chain,
                    number(s.chain.blocks)
                ),
            );
            return;
        };
        if matches!((old.uptime_seconds, s.uptime_seconds), (Some(a), Some(b)) if b < a) {
            self.record(s.observed_at, "CORE RESTARTED", "Node uptime reset".into());
        } else if s.chain.blocks > old.chain.blocks {
            self.record(
                s.observed_at,
                "BLOCKS CHECKED",
                format!(
                    "Verified {} more blocks · now #{}",
                    number(s.chain.blocks - old.chain.blocks),
                    number(s.chain.blocks)
                ),
            );
        } else if s.chain.blocks < old.chain.blocks
            || (!old.chain.bestblockhash.is_empty()
                && !s.chain.bestblockhash.is_empty()
                && old.chain.bestblockhash != s.chain.bestblockhash)
        {
            self.record(
                s.observed_at,
                "CHAIN TIP CHANGED",
                format!("Verified tip #{}", number(s.chain.blocks)),
            );
        }
        if old.chain.initialblockdownload && !s.chain.initialblockdownload {
            self.record(
                s.observed_at,
                "SYNC COMPLETE",
                "Core has left initial block download".into(),
            );
        }
        if old.network.networkactive != s.network.networkactive {
            self.record(
                s.observed_at,
                "NETWORK",
                if s.network.networkactive {
                    "Enabled"
                } else {
                    "Disabled"
                }
                .into(),
            );
        }
        if let (Some(before), Some(after)) = (&old.peers, &s.peers) {
            let added = after
                .iter()
                .filter(|p| !before.iter().any(|b| p.id == b.id))
                .collect::<Vec<_>>();
            for peer in added.iter().take(3) {
                let direction = if peer.inbound { "inbound" } else { "outbound" };
                let network = if peer.network.is_empty() {
                    "network unspecified"
                } else {
                    &peer.network
                };
                self.record(
                    s.observed_at,
                    "PEER CONNECTED",
                    format!("{direction} · {network}"),
                );
            }
            if added.len() > 3 {
                self.record(
                    s.observed_at,
                    "PEERS CONNECTED",
                    format!("{} additional connections", added.len() - 3),
                );
            }
            let removed = before
                .iter()
                .filter(|p| !after.iter().any(|b| p.id == b.id))
                .count();
            if removed > 0 {
                self.record(
                    s.observed_at,
                    "PEER DISCONNECTED",
                    format!("{removed} connection(s) closed"),
                );
            }
        }
        if !s.chain.initialblockdownload
            && old.chain.blocks == s.chain.blocks
            && let (Some(before), Some(after)) = (&old.mempool, &s.mempool)
            && before.size != after.size
        {
            self.record(
                s.observed_at,
                "MEMPOOL",
                format!(
                    "{} transactions · count change {:+}",
                    number(after.size),
                    i128::from(after.size) - i128::from(before.size)
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_only_reports_observed_changes_and_deduplicates_outages() {
        let mut app = App::default();
        let mut s = Snapshot::default();
        s.chain.blocks = 100;
        s.chain.initialblockdownload = true;
        s.peers = Some(vec![]);
        app.apply(Ok(s.clone()));
        app.apply(Ok(s.clone()));
        assert_eq!(app.events.len(), 1);
        s.chain.blocks = 125;
        s.chain.initialblockdownload = false;
        s.peers = Some(vec![crate::types::Peer {
            id: 7,
            network: "ipv6".into(),
            ..Default::default()
        }]);
        app.apply(Ok(s.clone()));
        assert!(
            app.events
                .iter()
                .any(|e| e.kind == "BLOCKS CHECKED" && e.detail.contains("25 more blocks"))
        );
        assert_eq!(
            app.events
                .iter()
                .filter(|e| e.kind == "SYNC COMPLETE")
                .count(),
            1
        );
        assert!(
            app.events
                .iter()
                .any(|e| e.kind == "PEER CONNECTED" && e.detail == "outbound · ipv6")
        );
        s.peers = None;
        app.apply(Ok(s.clone()));
        assert!(!app.events.iter().any(|e| e.kind == "PEER DISCONNECTED"));
        app.apply(Err("offline".into()));
        app.apply(Err("offline".into()));
        assert_eq!(
            app.events
                .iter()
                .filter(|e| e.kind == "CONNECTION LOST")
                .count(),
            1
        );
        app.apply(Ok(s.clone()));
        assert_eq!(app.events.back().unwrap().kind, "RECONNECTED");
        for _ in 0..100 {
            s.chain.blocks += 1;
            app.apply(Ok(s.clone()));
        }
        assert!(app.events.len() < 10);
        assert_eq!(app.events.back().unwrap().kind, "BLOCKS CHECKED");
    }

    #[test]
    fn failures_retain_previous_data_until_a_valid_observation_recovers() {
        let mut app = App::default();
        app.apply(Ok(Snapshot::default()));
        app.apply(Err("Core is busy or unavailable".to_string()));
        assert!(app.current.is_some());
        assert!(app.error.is_some());
        app.apply(Ok(Snapshot::default()));
        assert!(app.error.is_none());
    }
}
