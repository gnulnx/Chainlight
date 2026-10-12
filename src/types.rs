use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ChainInfo {
    pub chain: String,
    pub blocks: u64,
    pub headers: u64,
    pub time: i64,
    pub verificationprogress: f64,
    pub initialblockdownload: bool,
    pub size_on_disk: u64,
    pub pruned: bool,
    pub prune_target_size: u64,
    pub warnings: serde_json::Value,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct NetworkInfo {
    pub version: u64,
    pub subversion: String,
    pub connections: usize,
    pub connections_in: usize,
    pub connections_out: usize,
    pub networkactive: bool,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Traffic {
    #[serde(rename = "totalbytesrecv")]
    pub received: u64,
    #[serde(rename = "totalbytessent")]
    pub sent: u64,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Peer {
    pub id: i64,
    pub addr: String,
    pub subver: String,
    pub inbound: bool,
    pub pingtime: f64,
    pub bytesrecv: u64,
    pub bytessent: u64,
    pub connection_type: String,
    pub presynced_headers: i64,
    pub synced_headers: i64,
    pub synced_blocks: i64,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Mempool {
    pub size: u64,
    pub bytes: u64,
    pub usage: u64,
    pub maxmempool: u64,
    pub mempoolminfee: f64,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Snapshot {
    pub observed_at: DateTime<Utc>,
    pub chain: ChainInfo,
    pub network: NetworkInfo,
    pub traffic: Option<Traffic>,
    pub peers: Option<Vec<Peer>>,
    pub mempool: Option<Mempool>,
    pub disk_available_bytes: Option<u64>,
    pub disk_total_bytes: Option<u64>,
    pub datadir_bytes: Option<u64>,
    pub memory_available_bytes: Option<u64>,
    pub uptime_seconds: Option<u64>,
    #[serde(default)]
    pub notes: Vec<String>,
}
