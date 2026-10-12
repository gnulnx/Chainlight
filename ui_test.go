package main

import (
	"errors"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
	"strings"
	"testing"
	"time"
)

func observed() Snapshot {
	return Snapshot{At: time.Now(), Chain: ChainInfo{Chain: "main", IBD: true, Progress: .2, Blocks: 1000, Headers: 900000}, Network: NetworkInfo{Active: true, Connections: 8}}
}
func TestRealHeaderPresyncState(t *testing.T) {
	s := observed()
	s.Chain.Blocks = 0
	s.Chain.Headers = 0
	s.Peers = []Peer{{PresyncedHeaders: 782000}}
	m := newModel(nil, "")
	m.current = s
	if phase(s) != "HEADER PRE-SYNC" {
		t.Fatal(phase(s))
	}
	v := m.View()
	if !strings.Contains(v, "782,000 headers examined") || !strings.Contains(v, "has not started") {
		t.Fatal(v)
	}
}
func TestTrafficRestartCannotUnderflow(t *testing.T) {
	m := newModel(nil, "")
	s := observed()
	s.Totals = NetTotals{Received: 10000, Sent: 2000}
	m0, _ := m.Update(snapshotMsg{s, nil})
	m = m0.(model)
	s.At = s.At.Add(2 * time.Second)
	s.Totals = NetTotals{Received: 14000, Sent: 3000}
	m0, _ = m.Update(snapshotMsg{s, nil})
	m = m0.(model)
	if m.down != 2000 || m.up != 500 {
		t.Fatalf("rates %v %v", m.down, m.up)
	}
	s.At = s.At.Add(2 * time.Second)
	s.Totals = NetTotals{Received: 1, Sent: 1}
	m0, _ = m.Update(snapshotMsg{s, nil})
	m = m0.(model)
	if m.down != 0 || m.up != 0 || len(m.history) != 1 {
		t.Fatalf("restart rates/history %v %v %d", m.down, m.up, len(m.history))
	}
}
func TestETARequiresMeasuredHistoryAndConnection(t *testing.T) {
	m := newModel(nil, "")
	m.current = observed()
	now := time.Now()
	m.history = []sample{{at: now, progress: .1}, {at: now.Add(time.Minute), progress: .2}}
	if strings.Contains(m.eta(), "remaining") {
		t.Fatal("premature ETA")
	}
	m.history[1].at = now.Add(2 * time.Minute)
	if !strings.Contains(m.eta(), "rate may change") {
		t.Fatal(m.eta())
	}
	m.err = errors.New("offline")
	if strings.Contains(m.eta(), "remaining") {
		t.Fatal("stale ETA")
	}
}
func TestViewsFitTerminal(t *testing.T) {
	for _, size := range []tea.WindowSizeMsg{{Width: 40, Height: 14}, {Width: 60, Height: 20}, {Width: 100, Height: 36}} {
		for tab := 0; tab < 3; tab++ {
			m := newModel(nil, "")
			m.current = observed()
			m.width = size.Width
			m.height = size.Height
			m.tab = tab
			for _, help := range []bool{false, true} {
				m.help = help
				v := m.View()
				lines := strings.Split(v, "\n")
				if len(lines) > size.Height {
					t.Fatalf("height %d > %d", len(lines), size.Height)
				}
				for _, line := range lines {
					if ansi.StringWidth(line) > size.Width {
						t.Fatalf("width %d > %d: %q", ansi.StringWidth(line), size.Width, line)
					}
				}
			}
		}
	}
}
func TestPeerTextCannotExecuteTerminalControls(t *testing.T) {
	m := newModel(nil, "")
	m.current = observed()
	m.tab = 1
	m.current.Peers = []Peer{{Address: "peer\x1b]52;c;Zm9v\a\r\n", Subversion: "/node/\x1b[2J\u202ereversed"}}
	v := m.View()
	for _, bad := range []string{"\x1b]52", "\x1b[2J", "\u202e", "\a", "\r"} {
		if strings.Contains(v, bad) {
			t.Fatalf("unsafe control %q in view", bad)
		}
	}
}
func TestLostConnectionMarksRetainedSnapshotStale(t *testing.T) {
	m := newModel(nil, "")
	m.current = observed()
	m.lastSuccess = m.current.At
	m.err = errors.New("offline")
	v := m.View()
	if !strings.Contains(v, "STALE DATA") || !strings.Contains(v, "Unavailable · connection lost") {
		t.Fatal(v)
	}
}
func TestPollingDoesNotOverlapAndQuitLeavesCoreAlone(t *testing.T) {
	m := newModel(nil, "")
	next, _ := m.Update(tickMsg(time.Now()))
	if !next.(model).busy {
		t.Fatal("lost running poll state")
	}
	next, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'q'}})
	if cmd == nil {
		t.Fatal("quit ignored")
	}
	if _, ok := cmd().(tea.QuitMsg); !ok {
		t.Fatal("quit didn't emit only TUI shutdown")
	}
	if !next.(model).busy {
		t.Fatal("quit mutated polling state")
	}
}
func TestMissingTrafficSampleCannotCreateFalseRateOnRecovery(t *testing.T) {
	m := newModel(nil, "")
	s := observed()
	s.Totals = NetTotals{Received: 10000, Sent: 2000}
	next, _ := m.Update(snapshotMsg{s, nil})
	m = next.(model)
	missing := s
	missing.At = s.At.Add(2 * time.Second)
	missing.Totals = NetTotals{}
	missing.Notes = []string{"getnettotals unavailable"}
	next, _ = m.Update(snapshotMsg{missing, nil})
	m = next.(model)
	if strings.Contains(m.View(), "0 B since node start") {
		t.Error("missing traffic displayed as zero received")
	}
	s.At = s.At.Add(4 * time.Second)
	s.Totals = NetTotals{Received: 14000, Sent: 3000}
	next, _ = m.Update(snapshotMsg{s, nil})
	m = next.(model)
	// Either retain the last valid baseline (1000 B/s) or establish a new one (0).
	// Treating missing counters as real zeros creates a false 7000 B/s surge.
	if m.down > 1000 {
		t.Fatalf("unavailable sample became zero baseline: %v B/s", m.down)
	}
}
func TestMissingMempoolDoesNotClaimEmptyMempool(t *testing.T) {
	m := newModel(nil, "")
	m.current = observed()
	m.current.Chain.IBD = false
	m.current.Notes = []string{"getmempoolinfo unavailable"}
	v := m.View()
	if strings.Contains(v, "0 transactions · 0 B") || strings.Contains(v, "0.0 sat/vB") {
		t.Fatal("missing mempool presented as measured empty/zero fee", v)
	}
}
func TestMissingUptimeDoesNotClaimFreshRestart(t *testing.T) {
	m := newModel(nil, "")
	m.tab = 2
	m.current = observed()
	m.current.Notes = []string{"uptime unavailable"}
	if strings.Contains(m.View(), "Core uptime         0s") {
		t.Fatal("unavailable uptime presented as measured fresh restart")
	}
}
func TestMissingPeerListDoesNotClaimNoPeers(t *testing.T) {
	m := newModel(nil, "")
	m.tab = 1
	m.current = observed()
	m.current.Notes = []string{"getpeerinfo unavailable"}
	if strings.Contains(m.View(), "Core will seek outbound connections") {
		t.Fatal("missing peer RPC implies no peer connections despite network info reporting eight")
	}
}
