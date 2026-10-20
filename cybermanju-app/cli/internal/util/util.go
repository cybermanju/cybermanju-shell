// CyberManju OS — Util (cli/internal/util/util.go).
// Package util holds small pure helpers (sizes, versions, asset names)
// shared by commands and unit-tested without a dashboard.
package util

import (
	"fmt"
	"strconv"
	"strings"
)

// ParseSize parses "512M", "10G", "1.5T" or bare bytes into bytes.
func ParseSize(s string) (uint64, error) {
	s = strings.TrimSpace(s)
	if s == "" {
		return 0, fmt.Errorf("empty size (try 512M, 10G)")
	}
	mult := uint64(1)
	num := s
	switch last := strings.ToUpper(s[len(s)-1:]); last {
	case "K":
		mult = 1 << 10
		num = s[:len(s)-1]
	case "M":
		mult = 1 << 20
		num = s[:len(s)-1]
	case "G":
		mult = 1 << 30
		num = s[:len(s)-1]
	case "T":
		mult = 1 << 40
		num = s[:len(s)-1]
	}
	f, err := strconv.ParseFloat(strings.TrimSpace(num), 64)
	if err != nil || f <= 0 {
		return 0, fmt.Errorf("bad size %q (try 512M, 10G)", s)
	}
	return uint64(f * float64(mult)), nil
}

// FormatBytes renders bytes as a human size.
func FormatBytes(b uint64) string {
	const unit = 1024
	if b < unit {
		return fmt.Sprintf("%dB", b)
	}
	div, exp := uint64(unit), 0
	for n := b / unit; n >= unit; n /= unit {
		div *= unit
		exp++
	}
	return fmt.Sprintf("%.1f%cB", float64(b)/float64(div), "KMGTPE"[exp])
}

// CompareVersions compares "v0.1.0"-style versions; >0 means a is newer.
func CompareVersions(a, b string) int {
	norm := func(v string) []int {
		v = strings.TrimPrefix(strings.TrimSpace(v), "v")
		v = strings.SplitN(v, "-", 2)[0]
		v = strings.SplitN(v, "+", 2)[0]
		var out []int
		for _, p := range strings.Split(v, ".") {
			n, _ := strconv.Atoi(strings.TrimSpace(p))
			out = append(out, n)
		}
		return out
	}
	pa, pb := norm(a), norm(b)
	for i := 0; i < len(pa) || i < len(pb); i++ {
		var x, y int
		if i < len(pa) {
			x = pa[i]
		}
		if i < len(pb) {
			y = pb[i]
		}
		if x != y {
			if x > y {
				return 1
			}
			return -1
		}
	}
	return 0
}

// AssetName is the release tarball for GOOS/GOARCH.
func AssetName(goos, goarch string) string {
	return fmt.Sprintf("cyb-%s-%s.tar.gz", goos, goarch)
}
