// CyberManju OS — Theme test (cli/internal/ui/theme_test.go).
// Role: CLI terminal UI.
// Exports: TestGradientKeepsText, TestCharmThemeIsPink, TestBanner.
// Integrates: strings, testing, github.com.
// Contracts: none special (pure logic; errors bubble to caller).
package ui

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/colorprofile"
)

func TestGradientKeepsText(t *testing.T) {
	lipgloss.Writer.Profile = colorprofile.TrueColor
	// Strip ANSI: every rune must survive exactly once in order.
	got := Gradient("cyb", PinkHot, RedHot)
	plain := stripANSI(got)
	if plain != "cyb" {
		t.Fatalf("gradient lost text: %q", plain)
	}
	if !strings.Contains(got, "\x1b[") {
		t.Fatal("expected ANSI color codes")
	}
	if Gradient("", PinkHot, RedHot) != "" {
		t.Fatal("empty in, empty out")
	}
	// Bad colors degrade to plain text, never garbage.
	if Gradient("hi", "nope", "alsono") != "hi" {
		t.Fatal("bad colors should pass text through")
	}
}

func stripANSI(s string) string {
	var b strings.Builder
	inEsc := false
	for i := 0; i < len(s); i++ {
		if s[i] == 0x1b && i+1 < len(s) && s[i+1] == '[' {
			inEsc = true
			i++
			continue
		}
		if inEsc {
			if (s[i] >= 'a' && s[i] <= 'z') || (s[i] >= 'A' && s[i] <= 'Z') {
				inEsc = false
			}
			continue
		}
		b.WriteByte(s[i])
	}
	return b.String()
}

func TestCharmThemeIsPink(t *testing.T) {
	lipgloss.Writer.Profile = colorprofile.ANSI
	th := CharmTheme()
	if th == nil {
		t.Fatal("nil theme")
	}
	probe := th.Theme(true).Focused.Title.Render("x")
	if !strings.Contains(probe, "\x1b[") {
		t.Fatal("focused title must carry color")
	}
}

func TestBanner(t *testing.T) {
	b := Banner("v0.2.0")
	if !strings.Contains(stripANSI(b), "cybermanju") {
		t.Fatalf("banner lost its name: %q", b)
	}
}
