package main

import (
	"bufio"
	"fmt"
	"github.com/charmbracelet/lipgloss"
	"math"
	"os"
	"regexp"
	"strconv"
	"strings"
)

type palette struct{ Accent, Text, Muted, Green, Yellow, Red lipgloss.TerminalColor }

func loadPalette(path string) palette {
	p := palette{lipgloss.Color("4"), lipgloss.Color("7"), lipgloss.Color("8"), lipgloss.Color("2"), lipgloss.Color("3"), lipgloss.Color("1")}
	if path == "" {
		return p
	}
	f, e := os.Open(path)
	if e != nil {
		return p
	}
	defer f.Close()
	valid := regexp.MustCompile(`^#[0-9a-fA-F]{6}$`)
	colors := map[string]*lipgloss.TerminalColor{"accent": &p.Accent, "foreground": &p.Text, "muted": &p.Muted, "green": &p.Green, "yellow": &p.Yellow, "red": &p.Red}
	var bg, fg, dim string
	s := bufio.NewScanner(f)
	for s.Scan() {
		k, v, ok := strings.Cut(s.Text(), "=")
		if !ok {
			continue
		}
		v = strings.Trim(strings.TrimSpace(v), "\"'")
		if valid.MatchString(v) {
			switch strings.TrimSpace(k) {
			case "background":
				bg = v
			case "foreground":
				fg = v
			case "muted":
				dim = v
			}
		}
		if dest, ok := colors[strings.TrimSpace(k)]; ok && valid.MatchString(v) {
			*dest = lipgloss.Color(v)
		}
	}
	if bg != "" && fg != "" && dim != "" {
		p.Muted = lipgloss.Color(readableMuted(bg, fg, dim))
	}
	return p
}
func rgb(hex string) [3]float64 {
	n, _ := strconv.ParseUint(strings.TrimPrefix(hex, "#"), 16, 32)
	return [3]float64{float64(n >> 16), float64((n >> 8) & 255), float64(n & 255)}
}
func luminance(c [3]float64) float64 {
	for i, x := range c {
		x /= 255
		if x <= .04045 {
			c[i] = x / 12.92
		} else {
			c[i] = math.Pow((x+.055)/1.055, 2.4)
		}
	}
	return c[0]*.2126 + c[1]*.7152 + c[2]*.0722
}
func contrast(a, b [3]float64) float64 {
	x, y := luminance(a), luminance(b)
	if x < y {
		x, y = y, x
	}
	return (x + .05) / (y + .05)
}
func readableMuted(bg, fg, dim string) string {
	b, f, d := rgb(bg), rgb(fg), rgb(dim)
	for step := 0; step <= 20; step++ {
		t := float64(step) / 20
		var c [3]float64
		for i := range c {
			c[i] = math.Round(d[i] + (f[i]-d[i])*t)
		}
		if contrast(b, c) >= 4.5 || step == 20 {
			return fmt.Sprintf("#%02x%02x%02x", int(c[0]), int(c[1]), int(c[2]))
		}
	}
	return fg
}
