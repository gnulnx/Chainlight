package main

import (
	"context"
	"fmt"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"math"
	"strings"
	"time"
)

type sample struct {
	at       time.Time
	progress float64
	received uint64
}
type snapshotMsg struct {
	s   Snapshot
	err error
}
type tickMsg time.Time
type model struct {
	client             *Client
	themeFile          string
	current            Snapshot
	err                error
	width, height, tab int
	busy, help         bool
	history            []sample
	down, up           float64
	lastSuccess        time.Time
}

func newModel(c *Client, theme string) model {
	return model{client: c, themeFile: theme, width: 100, height: 36, busy: true}
}
func (m model) poll() tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 12*time.Second)
		defer cancel()
		s, e := m.client.snapshot(ctx)
		return snapshotMsg{s, e}
	}
}
func tick() tea.Cmd { return tea.Tick(2*time.Second, func(t time.Time) tea.Msg { return tickMsg(t) }) }
func (m model) Init() tea.Cmd {
	return tea.Batch(m.poll(), tick(), tea.SetWindowTitle("Chainlight — Bitcoin Node"))
}
func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch v := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = v.Width
		m.height = v.Height
	case tea.KeyMsg:
		switch v.String() {
		case "q", "ctrl+c":
			return m, tea.Quit
		case "?":
			m.help = !m.help
		case "esc":
			m.help = false
		case "tab", "right", "l":
			m.tab = (m.tab + 1) % 3
		case "shift+tab", "left", "h":
			m.tab = (m.tab + 2) % 3
		case "1":
			m.tab = 0
		case "2":
			m.tab = 1
		case "3":
			m.tab = 2
		case "r":
			if !m.busy {
				m.busy = true
				return m, m.poll()
			}
		}
	case tickMsg:
		if !m.busy {
			m.busy = true
			return m, tea.Batch(m.poll(), tick())
		}
		return m, tick()
	case snapshotMsg:
		m.busy = false
		m.err = v.err
		if v.err == nil {
			elapsed := v.s.At.Sub(m.current.At).Seconds()
			// Counter reset after Core restart is a new baseline, never an underflow.
			m.down = 0
			m.up = 0
			if !m.current.At.IsZero() && elapsed > 0 && !m.current.unavailable("getnettotals") && !v.s.unavailable("getnettotals") && v.s.Totals.Received >= m.current.Totals.Received && v.s.Totals.Sent >= m.current.Totals.Sent {
				m.down = float64(v.s.Totals.Received-m.current.Totals.Received) / elapsed
				m.up = float64(v.s.Totals.Sent-m.current.Totals.Sent) / elapsed
			} else {
				m.history = nil
			}
			m.current = v.s
			m.lastSuccess = v.s.At
			m.history = append(m.history, sample{v.s.At, v.s.Chain.Progress, v.s.Totals.Received})
			if len(m.history) > 150 {
				m.history = m.history[len(m.history)-150:]
			}
		}
	}
	return m, nil
}
func humanBytes(n uint64) string {
	v := float64(n)
	units := []string{"B", "KiB", "MiB", "GiB", "TiB"}
	i := 0
	for v >= 1024 && i < len(units)-1 {
		v /= 1024
		i++
	}
	if i == 0 {
		return fmt.Sprintf("%.0f %s", v, units[i])
	}
	return fmt.Sprintf("%.1f %s", v, units[i])
}
func number(n int64) string {
	s := fmt.Sprintf("%d", n)
	for i := len(s) - 3; i > 0; i -= 3 {
		s = s[:i] + "," + s[i:]
	}
	return s
}
func date(t int64) string {
	if t <= 0 {
		return "Waiting for blocks"
	}
	return time.Unix(t, 0).UTC().Format("02 Jan 2006 · 15:04 UTC")
}
func duration(n int64) string {
	if n < 60 {
		return fmt.Sprintf("%ds", n)
	}
	if n < 3600 {
		return fmt.Sprintf("%dm %ds", n/60, n%60)
	}
	return fmt.Sprintf("%dh %dm", n/3600, n%3600/60)
}
func maxPresync(s Snapshot) int64 {
	var n int64
	for _, p := range s.Peers {
		if p.PresyncedHeaders > n {
			n = p.PresyncedHeaders
		}
	}
	return n
}
func phase(s Snapshot) string {
	if s.At.IsZero() {
		return "CONNECTING"
	}
	if !s.Network.Active {
		return "NETWORK DISABLED"
	}
	if !s.Chain.IBD {
		return "LIVE NODE"
	}
	if s.Chain.Headers == 0 && maxPresync(s) > 0 {
		return "HEADER PRE-SYNC"
	}
	if s.Chain.Blocks == 0 {
		return "DISCOVERING HEADERS"
	}
	return "VERIFYING BLOCKS"
}
func (m model) eta() string {
	if m.err != nil || !m.current.Chain.IBD || len(m.history) < 2 {
		return "Measuring verification rate…"
	}
	first, last := m.history[0], m.history[len(m.history)-1]
	secs := last.at.Sub(first.at).Seconds()
	delta := last.progress - first.progress
	if secs < 120 || delta <= 0 {
		return "Measuring verification rate…"
	}
	remaining := (1 - last.progress) * secs / delta
	if remaining <= 0 || math.IsInf(remaining, 0) || remaining > 90*86400 {
		return "Estimate not yet reliable"
	}
	return "~" + duration(int64(remaining)) + " remaining · rate may change"
}
func progressBar(value float64, width int, p palette) string {
	value = math.Max(0, math.Min(1, value))
	fill := int(value * float64(width))
	return lipgloss.NewStyle().Foreground(p.Accent).Render(strings.Repeat("━", fill)) + lipgloss.NewStyle().Foreground(p.Muted).Render(strings.Repeat("─", width-fill))
}
func spark(history []sample, width int) string {
	if len(history) < 2 {
		return "Collecting network activity…"
	}
	rates := make([]float64, 0, len(history)-1)
	var peak float64
	for i := 1; i < len(history); i++ {
		d := history[i].at.Sub(history[i-1].at).Seconds()
		r := 0.0
		if d > 0 && history[i].received >= history[i-1].received {
			r = float64(history[i].received-history[i-1].received) / d
		}
		rates = append(rates, r)
		if r > peak {
			peak = r
		}
	}
	if len(rates) > width {
		rates = rates[len(rates)-width:]
	}
	chars := []rune("▁▂▃▄▅▆▇█")
	var b strings.Builder
	for _, r := range rates {
		i := 0
		if peak > 0 {
			i = int(r / peak * 7)
		}
		b.WriteRune(chars[i])
	}
	return b.String()
}
func (m model) View() string {
	p := loadPalette(m.themeFile)
	accent := lipgloss.NewStyle().Foreground(p.Accent).Bold(true)
	muted := lipgloss.NewStyle().Foreground(p.Muted)
	text := lipgloss.NewStyle().Foreground(p.Text)
	good := lipgloss.NewStyle().Foreground(p.Green)
	warn := lipgloss.NewStyle().Foreground(p.Yellow)
	w := m.width - 4
	if w > 110 {
		w = 110
	}
	if w < 24 {
		w = 24
	}
	if m.width < 40 || m.height < 14 {
		return "Chainlight\nEnlarge terminal to at least 40 × 14.\nq quit"
	}
	compact := m.height < 30
	var out []string
	add := func(s string) { out = append(out, s) }
	add(accent.Render("₿  CHAINLIGHT") + muted.Render("   Your chain. Your verification."))
	status := phase(m.current)
	if m.err != nil {
		status = "WAITING FOR CORE"
		if !m.current.At.IsZero() {
			status = "CONNECTION LOST · STALE DATA"
		}
	}
	statusColor := warn
	if status == "LIVE NODE" {
		statusColor = good
	}
	add(statusColor.Render("● "+status) + muted.Render("   "+safe(m.current.Chain.Chain)))
	add("")
	var tabs []string
	for i, t := range []string{"1 Overview", "2 Peers", "3 Storage"} {
		if m.tab == i {
			tabs = append(tabs, accent.Underline(true).Render(t))
		} else {
			tabs = append(tabs, muted.Render(t))
		}
	}
	add(strings.Join(tabs, "     "))
	add(muted.Render(strings.Repeat("─", w)))
	if m.err != nil {
		add(warn.Render(safe(m.err.Error())))
		if !m.lastSuccess.IsZero() {
			add(muted.Render("Last successful observation: " + m.lastSuccess.Format("15:04:05") + " · " + duration(int64(time.Since(m.lastSuccess).Seconds())) + " ago"))
		}
		add("")
	}
	s := m.current
	if m.help {
		add(accent.Render("KEYBOARD"))
		add("Tab / ← → / h l    Switch pages")
		add("1 2 3               Jump to a page")
		add("r                   Refresh now")
		add("? / Escape          Show / close help")
		add("q / Ctrl+C          Quit dashboard (Core keeps running)")
		add("")
		add(accent.Render("WHAT SYNC MEANS"))
		add("Headers establish the best-work chain; blocks are downloaded")
		add("and verified locally. Verification % estimates work completed,")
		add("not bytes downloaded. Pruning removes older verified blocks.")
		add("A new header pre-sync can advance before Core reports headers.")
	} else if s.At.IsZero() {
		add(accent.Render("GETTING YOUR NODE READY"))
		add("Connecting to Bitcoin Core’s local RPC interface.")
		add("The dashboard retries automatically every two seconds.")
		add("")
		add(muted.Render("Start the node: systemctl --user start bitcoin-node"))
		add(muted.Render("Check startup: journalctl --user -u bitcoin-node -n 30"))
	} else {
		switch m.tab {
		case 0:
			if s.Chain.IBD {
				add(accent.Render("BUILDING YOUR INDEPENDENTLY VERIFIED BITCOIN CHAIN"))
				if !compact {
					add("")
				}
				if s.Chain.Headers == 0 && maxPresync(s) > 0 {
					add(text.Render("Header pre-sync   ") + accent.Render(number(maxPresync(s))+" headers examined"))
					add(muted.Render("Core is discovering chain history before accepting headers."))
					add(muted.Render("Block verification has not started yet."))
				} else {
					add(text.Render("Verification   ") + accent.Render(fmt.Sprintf("%.2f%%", s.Chain.Progress*100)))
					add(progressBar(s.Chain.Progress, w-2, p))
					add(muted.Render(m.eta()))
				}
				if compact {
					add("Blocks " + accent.Render(number(s.Chain.Blocks)) + " · Headers " + number(s.Chain.Headers))
				} else {
					add("")
					add("Verified blocks     " + accent.Render(number(s.Chain.Blocks)))
					add("Validated headers   " + number(s.Chain.Headers))
				}
				add("Chain history       " + date(s.Chain.Time))
			} else {
				add(good.Bold(true).Render("INDEPENDENT VERIFICATION · UP TO DATE"))
				add("")
				add("Block height        " + accent.Render(number(s.Chain.Blocks)))
				add("Latest block        " + date(s.Chain.Time))
				if s.unavailable("getmempoolinfo") {
					add("Mempool             Unavailable")
					add("Minimum fee         Unavailable")
				} else {
					add("Mempool             " + number(s.Mempool.Size) + " transactions · " + humanBytes(uint64(s.Mempool.Bytes)))
					add(fmt.Sprintf("Minimum fee         %.1f sat/vB", s.Mempool.MinFee*1e5))
				}
			}
			if !compact {
				add("")
			}
			add(accent.Render("NETWORK"))
			add(fmt.Sprintf("Peers               %d connected · %d outbound · %d inbound", s.Network.Connections, s.Network.Outbound, s.Network.Inbound))
			rate := "Measuring…"
			if len(m.history) > 1 {
				rate = humanBytes(uint64(m.down)) + "/s ↓   " + humanBytes(uint64(m.up)) + "/s ↑"
			}
			if s.unavailable("getnettotals") {
				rate = "Unavailable"
			}
			if m.err != nil {
				rate = "Unavailable · connection lost"
			}
			add("Transfer            " + rate)
			if s.unavailable("getnettotals") {
				add("Received by Core    Unavailable")
			} else {
				add("Received by Core    " + humanBytes(s.Totals.Received) + " since node start")
				add(accent.Render(spark(m.history, w-2)))
			}
			if !compact {
				add("")
			}
			mode := "Archival"
			if s.Chain.Pruned {
				mode = "Pruned"
			}
			add(muted.Render(mode + " · " + humanBytes(s.DataDirBytes) + " databases · " + humanBytes(s.DiskAvailable) + " disk available"))
		case 1:
			add(accent.Render("CONNECTED PEERS"))
			add(muted.Render("Peer addresses are network endpoints; hide this page when sharing."))
			add("")
			if s.unavailable("getpeerinfo") {
				add(fmt.Sprintf("Peer details unavailable · %d peers connected", s.Network.Connections))
			} else if len(s.Peers) == 0 {
				add("No peers reported. Core will seek outbound connections.")
			}
			for i, peer := range s.Peers {
				if i >= m.height-15 {
					add(muted.Render(fmt.Sprintf("… %d more peers", len(s.Peers)-i)))
					break
				}
				direction := "OUT"
				if peer.Inbound {
					direction = "IN "
				}
				ping := "—"
				if peer.Ping > 0 {
					ping = fmt.Sprintf("%.0f ms", peer.Ping*1000)
				}
				add(fmt.Sprintf("%s  %-35s %8s  %s", direction, short(safe(peer.Address), 35), ping, short(safe(peer.Subversion), 24)))
			}
		case 2:
			add(accent.Render("STORAGE & RESOURCES"))
			add("")
			mode := "Archival · keeps historic blocks"
			if s.Chain.Pruned {
				mode = "Pruned · verifies all blocks, retains recent history"
			}
			add("Mode                " + mode)
			add("Block & undo files  " + humanBytes(uint64(s.Chain.Size)))
			add("Core databases      " + humanBytes(s.DataDirBytes))
			add("Filesystem free     " + humanBytes(s.DiskAvailable))
			add("Filesystem total    " + humanBytes(s.DiskTotal))
			if s.DiskTotal > 0 {
				add(progressBar(1-float64(s.DiskAvailable)/float64(s.DiskTotal), w-2, p))
			}
			add("")
			if s.Chain.Pruned {
				add("Block prune target  " + humanBytes(uint64(s.Chain.PruneTarget)))
				add(muted.Render("Target applies to block files; chainstate needs extra space."))
			}
			add("Available RAM       " + humanBytes(s.MemoryAvailable))
			if s.unavailable("uptime") {
				add("Core uptime         Unavailable")
			} else {
				add("Core uptime         " + duration(s.Uptime))
			}
			add("Core version        " + safe(s.Network.Subversion))
			add("")
			add(muted.Render("Closing the dashboard leaves your node running."))
		}
	}
	if len(s.Notes) > 0 {
		add(warn.Render(strings.Join(s.Notes, " · ")))
	}
	if len(s.Chain.Warnings) > 2 && string(s.Chain.Warnings) != "null" && string(s.Chain.Warnings) != "[]" {
		add(warn.Render("Core warnings: " + safe(string(s.Chain.Warnings))))
	}
	if !compact {
		add("")
	}
	add(muted.Render("← → / Tab pages    r refresh    ? help    q quit"))
	if !s.At.IsZero() {
		add(muted.Render("Observed " + s.At.Format("15:04:05") + " · refresh every 2s · chainlight " + version))
	}
	// Bound every line and the view to the real terminal dimensions.
	for i, line := range out {
		out[i] = lipgloss.NewStyle().MaxWidth(w).Render(line)
	}
	limit := m.height - 2
	if len(out) > limit {
		footer := out[len(out)-2:]
		out = append(out[:limit-2], footer...)
	}
	return lipgloss.NewStyle().Padding(1, 2).Render(strings.Join(out, "\n"))
}
func short(s string, n int) string {
	r := []rune(s)
	if len(r) > n {
		return string(r[:n-1]) + "…"
	}
	return s
}
