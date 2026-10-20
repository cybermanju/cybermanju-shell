// CyberManju OS — Theme (cli/internal/ui/theme.go).
// Role: CLI terminal UI — the CyberManju cyberpunk-neon face: one Lipgloss
// palette (hot pink into signal red on near-black, neon cyan + hazard
// yellow accents), gradient banner, Huh form theme, and the shared style
// atoms (badges, chips, keycaps, panels, lists) every command renders with.
// Exports: palette consts, style vars (Title/OK/Err/Warn/Dim/Accent/Panel/
// Number/Subtle/BadgeCheck/BadgeCross/Bullet), Gradient, CharmTheme,
// NewForm, Logo, Rule, Splash, Badge, Flag, Chip, Key, List, PanelTitled,
// NextSteps, StatusDot.
// Integrates: fmt, strings, charm.land (lipgloss/v2, huh/v2).
// Contracts: pure rendering — safe on pipes; builders emit full-fidelity
// ANSI and every print boundary downsamples to the active colorprofile
// (NoTTY when --no-color).
package ui

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/huh/v2"
	"charm.land/lipgloss/v2"
)

// CyberManju Charm palette — hot pink into signal red on near-black,
// with cyberpunk neon cyan + hazard yellow accents.
const (
	PinkHot  = "#FF5FA2"
	PinkDeep = "#C9184A"
	RedHot   = "#FF2222"
	RedDeep  = "#8D0801"
	CyanNeon = "#00F0FF"
	YellowHz = "#FFE600"
	IceWhite = "#FFF0F5"
	DimGray  = "#8A8A93"
)

// Gradient blends text per-rune from colorA to colorB (hex "#RRGGBB").
// Pure function — safe everywhere, including pipes.
func Gradient(text, colorA, colorB string) string {
	runes := []rune(text)
	if len(runes) == 0 {
		return ""
	}
	ra, ga, ba, errA := hexRGB(colorA)
	rb, gb, bb, errB := hexRGB(colorB)
	if errA != nil || errB != nil {
		return text
	}
	var b strings.Builder
	for i, r := range runes {
		t := 0.0
		if len(runes) > 1 {
			t = float64(i) / float64(len(runes)-1)
		}
		c := lipgloss.Color(fmt.Sprintf("#%02X%02X%02X",
			lerp(ra, rb, t), lerp(ga, gb, t), lerp(ba, bb, t)))
		b.WriteString(lipgloss.NewStyle().Foreground(c).Render(string(r)))
	}
	return b.String()
}

func lerp(a, b uint8, t float64) uint8 {
	return uint8(float64(a)*(1-t) + float64(b)*t)
}

func hexRGB(s string) (uint8, uint8, uint8, error) {
	var r, g, b uint8
	_, err := fmt.Sscanf(strings.TrimPrefix(s, "#"), "%02X%02X%02X", &r, &g, &b)
	if err != nil {
		_, err = fmt.Sscanf(strings.TrimPrefix(s, "#"), "%02x%02x%02x", &r, &g, &b)
	}
	return r, g, b, err
}

// CharmTheme is the Huh form theme: pink focus, red accents, adapting to
// the terminal background at runtime. The CyberManju terminal is dark, so
// the dark Charm base applies there.
func CharmTheme() huh.Theme {
	return huh.ThemeFunc(func(isDark bool) *huh.Styles {
		t := huh.ThemeCharm(isDark)
		pink := lipgloss.Color(PinkHot)
		red := lipgloss.Color(RedHot)
		t.Focused.Title = t.Focused.Title.Foreground(pink).Bold(true)
		t.Focused.Description = t.Focused.Description.Foreground(pink)
		t.Focused.Base = t.Focused.Base.BorderForeground(pink)
		t.Focused.SelectSelector = t.Focused.SelectSelector.Foreground(red)
		t.Focused.NextIndicator = t.Focused.NextIndicator.Foreground(red)
		t.Focused.PrevIndicator = t.Focused.PrevIndicator.Foreground(red)
		t.Focused.Option = t.Focused.Option.Foreground(lipgloss.Color(IceWhite))
		t.Focused.MultiSelectSelector = t.Focused.MultiSelectSelector.Foreground(red)
		return t
	})
}

// NewForm builds a Huh form in the CyberManju Charm theme, pinned to the
// active color profile (--no-color strips form chrome too).
func NewForm(groups ...*huh.Group) *huh.Form {
	return huh.NewForm(groups...).
		WithTheme(CharmTheme()).
		WithShowHelp(false).
		WithProgramOptions(tea.WithColorProfile(ColorProfile()))
}

// Logo is the cyberpunk mark: gradient blades over the wordmark.
func Logo() string {
	blades := Gradient("/// CYBERMANJU_OS v1", CyanNeon, PinkHot)
	word := Gradient("◈ cyb", PinkHot, RedHot)
	return blades + "\n" + word
}

// Rule renders a deterministic neon divider (pink/cyan segments).
func Rule(width int) string {
	if width < 8 {
		width = 8
	}
	if width > 80 {
		width = 80
	}
	pink := lipgloss.NewStyle().Foreground(lipgloss.Color(PinkHot)).Render(strings.Repeat("━", width/2))
	cyan := lipgloss.NewStyle().Foreground(lipgloss.Color(CyanNeon)).Render(strings.Repeat("━", width-width/2))
	return pink + cyan
}

// Splash is the setup/boot header: logo + tagline + rule.
func Splash(version string) string {
	return Logo() + "  " + Dim.Render("v"+strings.TrimPrefix(version, "v")) + "\n" +
		Dim.Render("your vault · your cloud · your terminal") + "\n" +
		Rule(48)
}

// ── Style atoms ─────────────────────────────────────────────────────

// Number renders numeric/measured values in neon cyan.
var Number = lipgloss.NewStyle().Foreground(lipgloss.Color(CyanNeon))

// Subtle renders faint secondary text one step below Dim.
var Subtle = lipgloss.NewStyle().Foreground(lipgloss.Color("#55555E"))

// BadgeCheck / BadgeCross are the ✓ / ✗ status badges.
var (
	BadgeCheck = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(CyanNeon))
	BadgeCross = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(RedHot))
)

// Badge renders a status badge: ok → cyan ✓, bad → red ✗.
func Badge(ok bool) string {
	if ok {
		return BadgeCheck.Render("✓")
	}
	return BadgeCross.Render("✗")
}

// Flag renders small status flags (encrypted, enabled, …) as pink chips.
func Flag(text string) string {
	return lipgloss.NewStyle().
		Foreground(lipgloss.Color(PinkHot)).
		Render("‹" + text + "›")
}

// Chip renders a `cyb …` command reference as an inline pink chip so prose
// never reads as plain text.
func Chip(text string) string {
	return lipgloss.NewStyle().
		Foreground(lipgloss.Color(PinkHot)).
		Render("`" + text + "`")
}

// Key renders a keybinding (enter, esc, ↑/↓) as a neon keycap.
func Key(text string) string {
	return lipgloss.NewStyle().
		Foreground(lipgloss.Color(CyanNeon)).
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(CyanNeon)).
		Padding(0, 1).
		Render(text)
}

// Bullet is the cyan neon list marker.
var Bullet = lipgloss.NewStyle().Foreground(lipgloss.Color(CyanNeon)).Render("▸")

// List renders bullet lines: cyan ▸ markers, ice-white text.
func List(items ...string) string {
	var b strings.Builder
	for i, it := range items {
		if i > 0 {
			b.WriteString("\n")
		}
		b.WriteString(Bullet + " " + it)
	}
	return b.String()
}

// PanelTitled boxes body under a pink title row (notes, next steps,
// approvals): rounded frame, title in the top border line.
func PanelTitled(title, body string) string {
	head := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(PinkHot)).Render("◈ " + title)
	frame := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(PinkHot)).
		Padding(0, 1)
	return head + "\n" + frame.Render(body)
}

// NextSteps renders a titled panel whose rows are `command — what it does`
// pairs; commands are underlined pink chips for scanability.
func NextSteps(title string, steps ...[2]string) string {
	var b strings.Builder
	for i, s := range steps {
		if i > 0 {
			b.WriteString("\n")
		}
		cmd := lipgloss.NewStyle().
			Bold(true).
			Underline(true).
			Foreground(lipgloss.Color(PinkHot)).
			Render(s[0])
		b.WriteString(cmd + "  " + Dim.Render(s[1]))
	}
	return PanelTitled(title, b.String())
}

// StatusDot renders a ● in ok (cyan) or bad (red) neon.
func StatusDot(ok bool) string {
	if ok {
		return lipgloss.NewStyle().Foreground(lipgloss.Color(CyanNeon)).Render("●")
	}
	return lipgloss.NewStyle().Foreground(lipgloss.Color(RedHot)).Render("●")
}
