package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	tea "github.com/charmbracelet/bubbletea"
	"os"
	"path/filepath"
	"time"
)

const version = "0.1.1"

func main() {
	home, _ := os.UserHomeDir()
	datadir := flag.String("datadir", filepath.Join(home, ".bitcoin"), "Bitcoin Core mainnet data directory")
	endpoint := flag.String("rpc-url", "http://127.0.0.1:8332", "Loopback RPC endpoint (remote nodes require SSH tunnel)")
	theme := flag.String("theme-file", "", "Optional Omarchy colors.toml (reloaded on refresh)")
	jsonOut := flag.Bool("snapshot", false, "Print one live JSON snapshot and exit")
	render := flag.Bool("render", false, "Print one live dashboard and exit")
	width := flag.Int("width", 100, "Width for --render")
	showVersion := flag.Bool("version", false, "Print version")
	flag.Parse()
	if *showVersion {
		fmt.Println("chainlight " + version)
		return
	}
	c, err := newClient(*endpoint, *datadir)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	m := newModel(c, *theme)
	if *jsonOut || *render {
		ctx, cancel := context.WithTimeout(context.Background(), 12*time.Second)
		defer cancel()
		s, e := c.snapshot(ctx)
		if e != nil {
			fmt.Fprintln(os.Stderr, e)
			os.Exit(1)
		}
		if *jsonOut {
			enc := json.NewEncoder(os.Stdout)
			enc.SetIndent("", "  ")
			if e = enc.Encode(s); e != nil {
				os.Exit(1)
			}
			return
		}
		m.current = s
		m.width = *width
		m.height = 36
		fmt.Println(m.View())
		return
	}
	if _, err = tea.NewProgram(m, tea.WithAltScreen()).Run(); err != nil {
		fmt.Fprintln(os.Stderr, "Dashboard:", err)
		os.Exit(1)
	}
}
