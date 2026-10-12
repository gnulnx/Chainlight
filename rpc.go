package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"time"
	"unicode"
)

type ChainInfo struct {
	Chain       string          `json:"chain"`
	Blocks      int64           `json:"blocks"`
	Headers     int64           `json:"headers"`
	Time        int64           `json:"time"`
	Progress    float64         `json:"verificationprogress"`
	IBD         bool            `json:"initialblockdownload"`
	Size        int64           `json:"size_on_disk"`
	Pruned      bool            `json:"pruned"`
	PruneTarget int64           `json:"prune_target_size"`
	Warnings    json.RawMessage `json:"warnings"`
}
type NetworkInfo struct {
	Version     int    `json:"version"`
	Subversion  string `json:"subversion"`
	Connections int    `json:"connections"`
	Inbound     int    `json:"connections_in"`
	Outbound    int    `json:"connections_out"`
	Active      bool   `json:"networkactive"`
}
type NetTotals struct {
	Received uint64 `json:"totalbytesrecv"`
	Sent     uint64 `json:"totalbytessent"`
}
type Peer struct {
	ID               int     `json:"id"`
	Address          string  `json:"addr"`
	Subversion       string  `json:"subver"`
	Inbound          bool    `json:"inbound"`
	Ping             float64 `json:"pingtime"`
	Received         uint64  `json:"bytesrecv"`
	Sent             uint64  `json:"bytessent"`
	Type             string  `json:"connection_type"`
	PresyncedHeaders int64   `json:"presynced_headers"`
	SyncedHeaders    int64   `json:"synced_headers"`
	SyncedBlocks     int64   `json:"synced_blocks"`
}
type MempoolInfo struct {
	Size   int64   `json:"size"`
	Bytes  int64   `json:"bytes"`
	Usage  int64   `json:"usage"`
	Max    int64   `json:"maxmempool"`
	MinFee float64 `json:"mempoolminfee"`
}
type Snapshot struct {
	At              time.Time   `json:"observed_at"`
	Chain           ChainInfo   `json:"chain"`
	Network         NetworkInfo `json:"network"`
	Totals          NetTotals   `json:"traffic"`
	Peers           []Peer      `json:"peers"`
	Mempool         MempoolInfo `json:"mempool"`
	DiskAvailable   uint64      `json:"disk_available_bytes"`
	DiskTotal       uint64      `json:"disk_total_bytes"`
	DataDirBytes    uint64      `json:"datadir_bytes"`
	MemoryAvailable uint64      `json:"memory_available_bytes"`
	Uptime          int64       `json:"uptime_seconds"`
	Notes           []string    `json:"notes,omitempty"`
}

func (s Snapshot) unavailable(method string) bool {
	for _, note := range s.Notes {
		if note == method+" unavailable" {
			return true
		}
	}
	return false
}

type Client struct {
	URL, DataDir   string
	HTTP           *http.Client
	resourceMu     sync.Mutex
	resourceAt     time.Time
	resourceBytes  uint64
	resourceFailed bool
}

func newClient(endpoint, datadir string) (*Client, error) {
	u, err := url.Parse(endpoint)
	if err != nil || u.Scheme != "http" || u.User != nil || u.RawQuery != "" || u.Fragment != "" || (u.Path != "" && u.Path != "/") {
		return nil, errors.New("RPC URL must be a loopback HTTP endpoint without credentials, query, or wallet path")
	}
	// Remote nodes use an SSH tunnel; never send the cookie to a remote host.
	if u.Hostname() != "127.0.0.1" && u.Hostname() != "::1" {
		return nil, errors.New("RPC must use 127.0.0.1 or [::1]; use an SSH tunnel for remote nodes")
	}
	transport := http.DefaultTransport.(*http.Transport).Clone()
	transport.Proxy = nil
	return &Client{URL: endpoint, DataDir: datadir, HTTP: &http.Client{Timeout: 3 * time.Second, Transport: transport, CheckRedirect: func(_ *http.Request, _ []*http.Request) error { return http.ErrUseLastResponse }}}, nil
}
func (c *Client) call(ctx context.Context, method string, target any) error {
	cookie, err := os.ReadFile(filepath.Join(c.DataDir, ".cookie"))
	if err != nil {
		return errors.New("Waiting for Bitcoin Core — RPC cookie is not available")
	}
	user, pass, ok := strings.Cut(strings.TrimSpace(string(cookie)), ":")
	if !ok || user == "" || pass == "" {
		return errors.New("Bitcoin Core RPC cookie is invalid")
	}
	body, _ := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": "chainlight", "method": method, "params": []any{}})
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.URL, bytes.NewReader(body))
	if err != nil {
		return errors.New("Invalid RPC request")
	}
	req.Header.Set("Content-Type", "application/json")
	req.SetBasicAuth(user, pass)
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return errors.New("Bitcoin Core RPC is unreachable — retrying")
	}
	defer resp.Body.Close()
	if resp.StatusCode == http.StatusUnauthorized {
		return errors.New("Bitcoin Core RPC authentication failed — retrying with current cookie")
	}
	if resp.StatusCode != http.StatusOK && resp.StatusCode != http.StatusServiceUnavailable {
		return fmt.Errorf("Bitcoin Core RPC returned HTTP %d", resp.StatusCode)
	}
	var envelope struct {
		Result json.RawMessage `json:"result"`
		Error  *struct {
			Code    int    `json:"code"`
			Message string `json:"message"`
		} `json:"error"`
	}
	if err = json.NewDecoder(io.LimitReader(resp.Body, 4<<20)).Decode(&envelope); err != nil {
		return errors.New("Bitcoin Core returned an invalid RPC response")
	}
	if envelope.Error != nil {
		if envelope.Error.Code == -28 {
			return errors.New("Bitcoin Core is starting — loading block databases")
		}
		// Never echo an arbitrary server message: a substituted local endpoint
		// could reflect the authentication cookie into visible error text.
		return fmt.Errorf("%s: RPC error %d", method, envelope.Error.Code)
	}
	if len(envelope.Result) == 0 || string(envelope.Result) == "null" {
		return fmt.Errorf("%s: missing RPC result", method)
	}
	if err := json.Unmarshal(envelope.Result, target); err != nil {
		return fmt.Errorf("%s: invalid RPC result", method)
	}
	return nil
}
func (c *Client) snapshot(ctx context.Context) (Snapshot, error) {
	s := Snapshot{}
	if err := c.call(ctx, "getblockchaininfo", &s.Chain); err != nil {
		return s, err
	}
	if err := c.call(ctx, "getnetworkinfo", &s.Network); err != nil {
		return s, err
	}
	for _, r := range []struct {
		method string
		target any
	}{{"getnettotals", &s.Totals}, {"getpeerinfo", &s.Peers}, {"getmempoolinfo", &s.Mempool}, {"uptime", &s.Uptime}} {
		if err := c.call(ctx, r.method, r.target); err != nil {
			s.Notes = append(s.Notes, r.method+" unavailable")
		}
	}
	s.At = time.Now()
	var fs syscall.Statfs_t
	if syscall.Statfs(c.DataDir, &fs) == nil {
		s.DiskAvailable = uint64(fs.Bavail) * uint64(fs.Bsize)
		s.DiskTotal = uint64(fs.Blocks) * uint64(fs.Bsize)
	}
	// Avoid repeatedly walking LevelDB files on a busy HDD. Account allocated
	// blocks without reading contents, at most once per 30 seconds.
	c.resourceMu.Lock()
	if time.Since(c.resourceAt) >= 30*time.Second {
		c.resourceBytes = 0
		c.resourceFailed = false
		for _, dir := range []string{"blocks", "chainstate", "indexes"} {
			err := filepath.Walk(filepath.Join(c.DataDir, dir), func(_ string, info os.FileInfo, e error) error {
				if ctx.Err() != nil {
					return ctx.Err()
				}
				if e != nil {
					if !os.IsNotExist(e) {
						c.resourceFailed = true
					}
					return nil
				}
				if info.Mode().IsRegular() {
					if st, ok := info.Sys().(*syscall.Stat_t); ok {
						c.resourceBytes += uint64(st.Blocks) * 512
					}
				}
				return nil
			})
			if err != nil {
				c.resourceFailed = true
			}
		}
		c.resourceAt = time.Now()
	}
	s.DataDirBytes = c.resourceBytes
	if c.resourceFailed {
		s.Notes = append(s.Notes, "Database disk usage incomplete")
	}
	c.resourceMu.Unlock()
	if data, e := os.ReadFile("/proc/meminfo"); e == nil {
		for _, line := range strings.Split(string(data), "\n") {
			if strings.HasPrefix(line, "MemAvailable:") {
				var kb uint64
				fmt.Sscanf(line, "MemAvailable: %d kB", &kb)
				s.MemoryAvailable = kb * 1024
			}
		}
	}
	return s, nil
}
func safe(s string) string {
	// All RPC strings (including peer-supplied subversions) are untrusted text.
	return strings.Map(func(r rune) rune {
		if unicode.IsControl(r) || r == 0x7f || (r >= 0x202a && r <= 0x202e) || (r >= 0x2066 && r <= 0x2069) {
			return -1
		}
		return r
	}, s)
}
