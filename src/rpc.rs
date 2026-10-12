use crate::types::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::Utc;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use url::{Host, Url};

#[derive(Clone, Default)]
struct Resources {
    available: Option<u64>,
    total: Option<u64>,
    allocated: Option<u64>,
    memory: Option<u64>,
}

pub struct Client {
    endpoint: String,
    datadir: PathBuf,
    agent: ureq::Agent,
    resources: Resources,
    resources_at: Option<Instant>,
}
impl Client {
    pub fn new(endpoint: &str, datadir: PathBuf) -> Result<Self, String> {
        if endpoint.split("//").nth(1).is_some_and(|authority| {
            authority
                .split('/')
                .next()
                .is_some_and(|host| host.contains('@'))
        }) {
            return Err("RPC URL must not contain credentials".into());
        }
        let url = Url::parse(endpoint).map_err(|_| "Invalid RPC URL".to_string())?;
        if url.scheme() != "http"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
            || !matches!(url.host(), Some(Host::Ipv4(ip)) if ip == std::net::Ipv4Addr::LOCALHOST)
                && !matches!(url.host(), Some(Host::Ipv6(ip)) if ip == std::net::Ipv6Addr::LOCALHOST)
        {
            return Err("RPC requires a numeric loopback HTTP URL without credentials, query, fragment, or wallet path; use an SSH tunnel for remote nodes".into());
        }
        let config = ureq::Agent::config_builder()
            .proxy(None)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(8)))
            .build();
        Ok(Self {
            endpoint: url.to_string(),
            datadir,
            agent: config.into(),
            resources: Resources::default(),
            resources_at: None,
        })
    }

    fn call<T: DeserializeOwned>(&self, method: &str, deadline: Instant) -> Result<T, String> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("Node observation deadline exceeded".into());
        }
        let cookie = fs::read_to_string(self.datadir.join(".cookie"))
            .map_err(|_| "Waiting for Bitcoin Core — RPC cookie is not available".to_string())?;
        let cookie = cookie.trim();
        let (user, pass) = cookie
            .split_once(':')
            .filter(|(u, p)| !u.is_empty() && !p.is_empty())
            .ok_or_else(|| "Bitcoin Core RPC cookie is invalid".to_string())?;
        if user.chars().chain(pass.chars()).any(char::is_control) {
            return Err("Bitcoin Core RPC cookie is invalid".into());
        }
        let auth = format!("Basic {}", STANDARD.encode(format!("{user}:{pass}")));
        let mut response = self
            .agent
            .post(&self.endpoint)
            .header("Authorization", &auth)
            .config()
            .timeout_global(Some(remaining.min(Duration::from_secs(8))))
            .build()
            .send_json(json!({"jsonrpc":"2.0","id":"chainlight","method":method,"params":[]}))
            .map_err(|_| "Bitcoin Core RPC is unreachable — retrying".to_string())?;
        let status = response.status().as_u16();
        if status == 401 {
            return Err(
                "Bitcoin Core RPC authentication failed — retrying with current cookie".into(),
            );
        }
        if status != 200 && status != 503 {
            return Err(format!("Bitcoin Core RPC returned HTTP {status}"));
        }
        let body = response
            .body_mut()
            .with_config()
            .limit(4 * 1024 * 1024)
            .read_to_vec()
            .map_err(|_| {
                "Bitcoin Core returned an invalid or oversized RPC response".to_string()
            })?;
        let envelope: Value = serde_json::from_slice(&body)
            .map_err(|_| "Bitcoin Core returned an invalid RPC response".to_string())?;
        if !envelope.is_object()
            || envelope.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || envelope.get("id").and_then(Value::as_str) != Some("chainlight")
        {
            return Err("Bitcoin Core returned an invalid RPC envelope".into());
        }
        if let Some(error) = envelope.get("error").filter(|v| !v.is_null()) {
            if envelope.get("result").is_some_and(|v| !v.is_null())
                || !error.get("message").is_some_and(Value::is_string)
            {
                return Err("Bitcoin Core returned an invalid RPC error".into());
            }
            let code = error
                .get("code")
                .and_then(Value::as_i64)
                .ok_or_else(|| "Bitcoin Core returned an invalid RPC error".to_string())?;
            if code == -28 {
                return Err("Bitcoin Core is starting — loading block databases".into());
            }
            return Err(format!("{method}: RPC error {code}"));
        }
        if status != 200 {
            return Err(format!("Bitcoin Core RPC returned HTTP {status}"));
        }
        let result = envelope
            .get("result")
            .filter(|v| !v.is_null())
            .ok_or_else(|| format!("{method}: missing RPC result"))?;
        validate_result(method, result).map_err(|_| format!("{method}: invalid RPC result"))?;
        serde_json::from_value(result.clone()).map_err(|_| format!("{method}: invalid RPC result"))
    }

    pub fn snapshot(&mut self) -> Result<Snapshot, String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let chain = self.call("getblockchaininfo", deadline)?;
        let network = self.call("getnetworkinfo", deadline)?;
        let mut notes = Vec::new();
        let traffic = self.optional("getnettotals", deadline, &mut notes);
        let peers = self.optional("getpeerinfo", deadline, &mut notes);
        let mempool = self.optional("getmempoolinfo", deadline, &mut notes);
        let uptime_seconds = self.optional("uptime", deadline, &mut notes);
        if self
            .resources_at
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(30))
        {
            self.resources = sample_resources(&self.datadir, deadline);
            self.resources_at = Some(Instant::now());
        }
        Ok(Snapshot {
            observed_at: Utc::now(),
            chain,
            network,
            traffic,
            peers,
            mempool,
            disk_available_bytes: self.resources.available,
            disk_total_bytes: self.resources.total,
            datadir_bytes: self.resources.allocated,
            memory_available_bytes: self.resources.memory,
            uptime_seconds,
            notes,
        })
    }
    fn optional<T: DeserializeOwned>(
        &self,
        method: &str,
        deadline: Instant,
        notes: &mut Vec<String>,
    ) -> Option<T> {
        match self.call(method, deadline) {
            Ok(value) => Some(value),
            Err(_) => {
                notes.push(format!("{method} unavailable"));
                None
            }
        }
    }
}
fn validate_result(method: &str, value: &Value) -> Result<(), ()> {
    let keys: &[&str] = match method {
        "getblockchaininfo" => &[
            "chain",
            "blocks",
            "headers",
            "time",
            "verificationprogress",
            "initialblockdownload",
            "size_on_disk",
            "pruned",
        ],
        "getnetworkinfo" => &[
            "version",
            "subversion",
            "connections",
            "connections_in",
            "connections_out",
            "networkactive",
        ],
        "getnettotals" => &["totalbytesrecv", "totalbytessent"],
        "getmempoolinfo" => &["size", "bytes", "usage", "maxmempool", "mempoolminfee"],
        "getpeerinfo" => {
            return if value.as_array().is_some_and(|peers| {
                peers.iter().all(|peer| {
                    [
                        "id",
                        "addr",
                        "subver",
                        "inbound",
                        "bytesrecv",
                        "bytessent",
                        "connection_type",
                    ]
                    .iter()
                    .all(|key| peer.get(key).is_some_and(|v| !v.is_null()))
                })
            }) {
                Ok(())
            } else {
                Err(())
            };
        }
        "uptime" => {
            return if value.as_u64().is_some() {
                Ok(())
            } else {
                Err(())
            };
        }
        _ => return Err(()),
    };
    if !value.is_object()
        || keys
            .iter()
            .any(|key| value.get(key).is_none_or(Value::is_null))
    {
        return Err(());
    }
    if method == "getblockchaininfo"
        && !value["verificationprogress"]
            .as_f64()
            .is_some_and(|v| v.is_finite() && (0.0..=1.0).contains(&v))
    {
        return Err(());
    }
    Ok(())
}
fn sample_resources(datadir: &Path, deadline: Instant) -> Resources {
    let mut resources = Resources::default();
    if let Ok(stat) = nix::sys::statvfs::statvfs(datadir) {
        resources.available = stat.blocks_available().checked_mul(stat.fragment_size());
        resources.total = stat.blocks().checked_mul(stat.fragment_size());
    }
    #[cfg(unix)]
    {
        let mut bytes = 0_u64;
        let mut okay = true;
        for name in ["blocks", "chainstate", "indexes"] {
            let path = datadir.join(name);
            match allocated_bytes(&path, deadline) {
                Ok(n) => match bytes.checked_add(n) {
                    Some(n) => bytes = n,
                    None => okay = false,
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => okay = false,
            }
        }
        if okay {
            resources.allocated = Some(bytes);
        }
    }
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        resources.memory = meminfo.lines().find_map(|line| {
            let rest = line.strip_prefix("MemAvailable:")?;
            rest.split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()?
                .checked_mul(1024)
        });
    }
    resources
}
#[cfg(unix)]
fn allocated_bytes(path: &Path, deadline: Instant) -> std::io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    if Instant::now() >= deadline {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "resource deadline",
        ));
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_symlink() {
        return Ok(0);
    }
    if metadata.is_dir() {
        fs::read_dir(path)?.try_fold(0_u64, |total, entry| {
            let n = allocated_bytes(&entry?.path(), deadline)?;
            total
                .checked_add(n)
                .ok_or_else(|| std::io::Error::other("allocation overflow"))
        })
    } else {
        metadata
            .blocks()
            .checked_mul(512)
            .ok_or_else(|| std::io::Error::other("allocation overflow"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
    };
    use tempfile::TempDir;

    fn envelope(result: Value) -> String {
        json!({"jsonrpc":"2.0","id":"chainlight","result":result}).to_string()
    }
    fn error(code: i64, message: &str) -> String {
        json!({"jsonrpc":"2.0","id":"chainlight","error":{"code":code,"message":message}})
            .to_string()
    }
    struct Server {
        url: String,
        requests: mpsc::Receiver<(String, String)>,
        handle: thread::JoinHandle<()>,
    }
    fn server(responses: Vec<(u16, String)>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                let mut authorization = String::new();
                let mut length = 0;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some((key, value)) = line.split_once(':') {
                        if key.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse::<usize>().unwrap();
                        }
                        if key.eq_ignore_ascii_case("authorization") {
                            authorization = value.trim().to_string();
                        }
                    }
                }
                let mut request = vec![0; length];
                reader.read_exact(&mut request).unwrap();
                tx.send((authorization, String::from_utf8(request).unwrap()))
                    .unwrap();
                write!(stream,"HTTP/1.1 {status} Status\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        });
        Server {
            url,
            requests: rx,
            handle,
        }
    }
    fn fixture(srv: &Server) -> (Client, TempDir) {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".cookie"), "__cookie__:secret-one\n").unwrap();
        (
            Client::new(&srv.url, dir.path().to_path_buf()).unwrap(),
            dir,
        )
    }
    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(20)
    }
    fn core_results() -> Vec<(u16, String)> {
        vec![
            (
                200,
                envelope(
                    json!({"chain":"main","blocks":200,"headers":971126,"time":1231469665,"verificationprogress":0.0001,"initialblockdownload":true,"size_on_disk":1000000,"pruned":true,"prune_target_size":21474836480_u64}),
                ),
            ),
            (
                200,
                envelope(
                    json!({"version":310100,"subversion":"/Satoshi:31.1.0/","connections":8,"connections_in":0,"connections_out":8,"networkactive":true}),
                ),
            ),
        ]
    }
    #[test]
    fn cookie_rotation_is_used_on_next_request() {
        let srv = server(vec![(200, envelope(json!(20))), (200, envelope(json!(21)))]);
        let (c, dir) = fixture(&srv);
        assert_eq!(c.call::<u64>("uptime", deadline()).unwrap(), 20);
        fs::write(dir.path().join(".cookie"), "__cookie__:secret-two").unwrap();
        assert_eq!(c.call::<u64>("uptime", deadline()).unwrap(), 21);
        let first = srv.requests.recv().unwrap().0;
        let second = srv.requests.recv().unwrap().0;
        assert_eq!(
            first,
            format!("Basic {}", STANDARD.encode("__cookie__:secret-one"))
        );
        assert_eq!(
            second,
            format!("Basic {}", STANDARD.encode("__cookie__:secret-two"))
        );
        srv.handle.join().unwrap();
    }
    #[test]
    fn failure_messages_never_reflect_credentials() {
        for (status, body, expected) in [
            (401, "__cookie__:secret-one".into(), "authentication failed"),
            (503, error(-28, "secret-one"), "loading block databases"),
            (200, error(-1, "__cookie__:secret-one"), "RPC error -1"),
            (200, "not-json secret-one".into(), "invalid RPC response"),
            (200, envelope(json!({})), "invalid RPC result"),
        ] {
            let srv = server(vec![(status, body)]);
            let (c, _dir) = fixture(&srv);
            let err = c.call::<u64>("uptime", deadline()).unwrap_err();
            assert!(err.contains(expected), "{err}");
            assert!(!err.contains("secret-one") && !err.contains("__cookie__"));
            srv.handle.join().unwrap();
        }
    }
    #[test]
    fn invalid_envelopes_are_rejected() {
        for body in [
            json!({"result":1}),
            json!({"jsonrpc":"2.0","id":"other","result":1}),
            json!({"jsonrpc":"2.0","id":"chainlight"}),
            json!({"jsonrpc":"2.0","id":"chainlight","result":null}),
            json!({"jsonrpc":"2.0","id":"chainlight","error":{"code":"-1"}}),
        ] {
            let srv = server(vec![(200, body.to_string())]);
            let (c, _dir) = fixture(&srv);
            assert!(c.call::<u64>("uptime", deadline()).is_err());
            srv.handle.join().unwrap();
        }
    }
    #[test]
    fn optional_failures_are_absent_and_snapshot_only_reads() {
        let mut responses = core_results();
        responses.extend((0..4).map(|_| (200, error(-32601, "Method unavailable"))));
        let srv = server(responses);
        let (mut c, _dir) = fixture(&srv);
        let s = c.snapshot().unwrap();
        assert_eq!(s.chain.blocks, 200);
        assert!(
            s.traffic.is_none()
                && s.peers.is_none()
                && s.mempool.is_none()
                && s.uptime_seconds.is_none()
        );
        assert_eq!(s.notes.len(), 4);
        let methods: Vec<String> = (0..6)
            .map(|_| {
                let body = srv.requests.recv().unwrap().1;
                let v: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(v["params"], json!([]));
                v["method"].as_str().unwrap().into()
            })
            .collect();
        assert_eq!(
            methods,
            vec![
                "getblockchaininfo",
                "getnetworkinfo",
                "getnettotals",
                "getpeerinfo",
                "getmempoolinfo",
                "uptime"
            ]
        );
        srv.handle.join().unwrap();
    }
    #[test]
    fn valid_snapshot_preserves_real_zero_and_presync_values() {
        let mut responses = core_results();
        responses.extend([(200,envelope(json!({"totalbytesrecv":0,"totalbytessent":0}))),(200,envelope(json!([{"id":0,"addr":"127.0.0.1:8333","subver":"/Satoshi:31.1.0/","inbound":false,"bytesrecv":100,"bytessent":50,"connection_type":"outbound-full-relay","presynced_headers":782000}]))),(200,envelope(json!({"size":0,"bytes":0,"usage":0,"maxmempool":100000000,"mempoolminfee":0.000001}))),(200,envelope(json!(0)))]);
        let srv = server(responses);
        let (mut c, _dir) = fixture(&srv);
        let s = c.snapshot().unwrap();
        assert_eq!(s.traffic.unwrap().received, 0);
        assert_eq!(s.peers.unwrap()[0].presynced_headers, 782000);
        assert_eq!(s.uptime_seconds, Some(0));
        assert!(s.notes.is_empty());
        srv.handle.join().unwrap();
    }
    #[test]
    fn unsafe_urls_and_proxy_are_blocked() {
        for url in [
            "http://example.com:8332",
            "http://localhost:8332",
            "https://127.0.0.1:8332",
            "http://user:secret@127.0.0.1:8332",
            "http://@127.0.0.1:8332",
            "http://127.0.0.1:8332/wallet/a",
            "http://127.0.0.1:8332?token=secret",
            "http://127.0.0.1:8332#secret",
        ] {
            assert!(Client::new(url, PathBuf::new()).is_err(), "{url}");
        }
        let c = Client::new("http://[::1]:8332", PathBuf::new()).unwrap();
        assert!(c.agent.config().proxy().is_none());
        assert_eq!(c.agent.config().max_redirects(), 0);
    }
    #[test]
    fn redirect_does_not_forward_credentials() {
        let dest = TcpListener::bind("127.0.0.1:0").unwrap();
        dest.set_nonblocking(true).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let location = format!("http://{}", dest.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0; 4096];
            assert!(stream.read(&mut buf).unwrap() > 0);
            write!(stream,"HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".cookie"), "__cookie__:secret-one").unwrap();
        let c = Client::new(&url, dir.path().to_path_buf()).unwrap();
        assert!(c.call::<u64>("uptime", deadline()).is_err());
        handle.join().unwrap();
        assert_eq!(
            dest.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    #[test]
    fn missing_cookie_and_expired_deadline_fail_without_network() {
        let c = Client::new(
            "http://127.0.0.1:1",
            TempDir::new().unwrap().path().to_path_buf(),
        )
        .unwrap();
        assert!(
            c.call::<u64>("uptime", deadline())
                .unwrap_err()
                .contains("cookie is not available")
        );
        assert!(
            c.call::<u64>("uptime", Instant::now())
                .unwrap_err()
                .contains("deadline")
        );
    }
    #[cfg(unix)]
    #[test]
    fn resource_scan_counts_allocated_blocks_and_skips_symlinks() {
        use std::os::unix::fs::{MetadataExt, symlink};
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("blocks")).unwrap();
        let file = dir.path().join("blocks/blk00000.dat");
        fs::write(&file, vec![0_u8; 8192]).unwrap();
        symlink(dir.path(), dir.path().join("blocks/loop")).unwrap();
        let resources = sample_resources(dir.path(), deadline());
        assert_eq!(
            resources.allocated,
            Some(fs::metadata(file).unwrap().blocks() * 512)
        );
        assert!(resources.available.is_some());
    }
    #[test]
    fn malformed_required_result_cannot_become_default_mainnet_state() {
        let srv = server(vec![(200, envelope(json!({})))]);
        let (mut c, _dir) = fixture(&srv);
        assert!(c.snapshot().unwrap_err().contains("invalid RPC result"));
        srv.handle.join().unwrap();
    }
    #[test]
    fn filesystem_resources_are_cached_between_refreshes() {
        let mut responses = core_results();
        responses.extend((0..4).map(|_| (200, error(-32601, "unavailable"))));
        let mut second = responses.clone();
        responses.append(&mut second);
        let srv = server(responses);
        let (mut c, dir) = fixture(&srv);
        fs::create_dir(dir.path().join("blocks")).unwrap();
        let file = dir.path().join("blocks/blk00000.dat");
        fs::write(&file, vec![1_u8; 8192]).unwrap();
        let first = c.snapshot().unwrap();
        fs::write(&file, vec![1_u8; 65536]).unwrap();
        let second = c.snapshot().unwrap();
        assert_eq!(first.datadir_bytes, second.datadir_bytes);
        assert!(first.datadir_bytes.is_some());
        srv.handle.join().unwrap();
    }
}
