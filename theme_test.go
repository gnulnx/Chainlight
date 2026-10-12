package main

import (
	"github.com/charmbracelet/lipgloss"
	"os"
	"path/filepath"
	"testing"
)

func TestThemeReloadsAndRejectsControlSequences(t *testing.T) {
	p := filepath.Join(t.TempDir(), "colors.toml")
	write := func(s string) {
		t.Helper()
		if e := os.WriteFile(p, []byte(s), 0600); e != nil {
			t.Fatal(e)
		}
	}
	write("accent = '#abcdef'\nforeground = '#123456'\n")
	v := loadPalette(p)
	if v.Accent != lipgloss.Color("#abcdef") || v.Text != lipgloss.Color("#123456") {
		t.Fatal(v)
	}
	write("accent = '#fedcba'\nforeground = '\x1b]52;c;attack'\n")
	v = loadPalette(p)
	if v.Accent != lipgloss.Color("#fedcba") || v.Text != loadPalette("").Text {
		t.Fatal(v)
	}
}
func TestMissingThemeUsesTerminalPalette(t *testing.T) {
	if loadPalette(filepath.Join(t.TempDir(), "absent")) != loadPalette("") {
		t.Fatal("missing theme changed defaults")
	}
}
