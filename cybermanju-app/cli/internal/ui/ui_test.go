// CyberManju OS — Ui test (cli/internal/ui/ui_test.go).
// Role: CLI terminal UI tests — table, JSON tint, markdown, chips.
// Exports: TestRenderTable*, TestRenderJSON*, TestRenderMarkdown*,
// TestIsMarkdown, TestGlamourStyle, TestTermWidthBounds, TestChromeAtoms.
// Integrates: testing, strings, lipgloss, colorprofile.
// Contracts: profile-sensitive tests pin lipgloss.Writer.Profile first
// (tests share the process-global renderer); width tests only assume the
// 40..100 TermWidth clamp, so they hold on any terminal and in CI logs.
package ui

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/colorprofile"
)

func TestRenderTableHasHeaderAndRule(t *testing.T) {
	lipgloss.Writer.Profile = colorprofile.TrueColor
	out := stripANSI(RenderTable(
		[]string{"ID", "NAME"},
		[][]string{{"ab12", "notes.md"}, {"cd34", "photos"}},
	))
	lines := strings.Split(out, "\n")
	if len(lines) != 4 {
		t.Fatalf("want header + rule + 2 rows, got %d lines:\n%s", len(lines), out)
	}
	if !strings.Contains(lines[0], "ID") || !strings.Contains(lines[0], "NAME") {
		t.Fatalf("header lost columns:\n%s", out)
	}
	if !strings.Contains(lines[1], "─") {
		t.Fatalf("missing neon header rule:\n%s", out)
	}
	if !strings.Contains(out, "notes.md") || !strings.Contains(out, "photos") {
		t.Fatalf("lost cell text:\n%s", out)
	}
}

func TestRenderTableCarriesColor(t *testing.T) {
	// v2 builders always emit full-fidelity ANSI; the profile only gates
	// what Downsample (and the lipgloss writers) let through to output.
	lipgloss.Writer.Profile = colorprofile.TrueColor
	if got := RenderTable([]string{"A"}, [][]string{{"b"}}); !strings.Contains(got, "\x1b[") {
		t.Fatal("expected ANSI styling in the builder")
	}
	lipgloss.Writer.Profile = colorprofile.NoTTY
	plain := Downsample(RenderTable([]string{"A"}, [][]string{{"b"}}))
	if strings.Contains(plain, "\x1b[") {
		t.Fatalf("NoTTY profile must strip styling at print, got %q", plain)
	}
	if stripANSI(plain) != "A\n─\nb" {
		t.Fatalf("stripped table wrong: %q", stripANSI(plain))
	}
}

func TestRenderTableTruncatesOverflow(t *testing.T) {
	lipgloss.Writer.Profile = colorprofile.Ascii
	long := strings.Repeat("x", 300)
	out := RenderTable([]string{"NAME"}, [][]string{{long}})
	if strings.Contains(out, long) {
		t.Fatal("300-cell column must truncate to the terminal width")
	}
	if !strings.Contains(out, "…") {
		t.Fatalf("truncation must show an ellipsis:\n%s", out)
	}
	for _, line := range strings.Split(out, "\n") {
		if lipgloss.Width(line) > 100 {
			t.Fatalf("line exceeds the width clamp: %q", line)
		}
	}
}

func TestRenderJSONPrettyAndTinted(t *testing.T) {
	lipgloss.Writer.Profile = colorprofile.TrueColor
	raw := []byte(`{"id":"ab12","size":1234,"ok":true,"meta":null,"tags":["a","b"]}`)
	out, ok := RenderJSON(raw)
	if !ok {
		t.Fatal("valid JSON must render")
	}
	for _, want := range []string{`"id"`, `"ab12"`, "1234", "true", "null", "\n"} {
		if !strings.Contains(out, want) {
			t.Fatalf("render lost %q:\n%s", want, out)
		}
	}
	if !strings.Contains(out, "\x1b[") {
		t.Fatal("expected syntax tint in the builder")
	}
	lipgloss.Writer.Profile = colorprofile.NoTTY
	if plain := Downsample(out); strings.Contains(plain, "\x1b[") {
		t.Fatalf("NoTTY profile must strip tint at print: %q", plain)
	}
}

func TestRenderJSONShapes(t *testing.T) {
	lipgloss.Writer.Profile = colorprofile.Ascii
	for _, raw := range []string{`[]`, `{}`, `[1,{"a":[true,null]}]`, `"str"`, `42`} {
		if out, ok := RenderJSON([]byte(raw)); !ok || out == "" {
			t.Fatalf("must render %s (ok=%v)", raw, ok)
		}
	}
	if _, ok := RenderJSON([]byte(`not json`)); ok {
		t.Fatal("non-JSON must report ok=false")
	}
	if _, ok := RenderJSON([]byte(`{"a":1} trailing`)); ok {
		t.Fatal("trailing garbage must report ok=false")
	}
}

func TestRenderMarkdownRendersHeading(t *testing.T) {
	lipgloss.Writer.Profile = colorprofile.TrueColor
	out := RenderMarkdown("# hello vault\n\nsome *text*")
	// Styles may split words into separate runs — assert on the plain text.
	if !strings.Contains(stripANSI(out), "hello vault") {
		t.Fatalf("lost heading text: %q", out)
	}
	if strings.Contains(stripANSI(out), "# hello vault") {
		t.Fatalf("raw markdown marker leaked through: %q", out)
	}
	if !strings.Contains(out, "\x1b[") {
		t.Fatal("expected ANSI styling on a color terminal")
	}
}

func TestRenderMarkdownNoColorIsPlain(t *testing.T) {
	lipgloss.Writer.Profile = colorprofile.NoTTY
	out := RenderMarkdown("# hello vault\n\n- one\n- two")
	if strings.Contains(out, "\x1b[") {
		t.Fatalf("Ascii profile must strip styling: %q", out)
	}
	for _, want := range []string{"hello vault", "one", "two"} {
		if !strings.Contains(out, want) {
			t.Fatalf("lost %q: %q", want, out)
		}
	}
}

func TestRenderMarkdownWrapsToWidth(t *testing.T) {
	lipgloss.Writer.Profile = colorprofile.Ascii
	para := strings.Repeat("neon ", 60)
	for _, line := range strings.Split(RenderMarkdown(para), "\n") {
		if lipgloss.Width(line) > 100 {
			t.Fatalf("line exceeds the width clamp: %q", line)
		}
	}
}

func TestRenderMarkdownHonorsEnvStyle(t *testing.T) {
	lipgloss.Writer.Profile = colorprofile.TrueColor
	t.Setenv("GLAMOUR_STYLE", "notty")
	out := RenderMarkdown("# hello")
	if !strings.Contains(out, "hello") {
		t.Fatalf("GLAMOUR_STYLE=notty lost text: %q", out)
	}
}

func TestRenderMarkdownBadEnvFallsBack(t *testing.T) {
	lipgloss.Writer.Profile = colorprofile.TrueColor
	t.Setenv("GLAMOUR_STYLE", "no-such-style")
	in := "# hello"
	if out := RenderMarkdown(in); out != in {
		t.Fatalf("bad GLAMOUR_STYLE must pass text through, got %q", out)
	}
}

func TestIsMarkdown(t *testing.T) {
	for _, name := range []string{"a.md", "A.MD", "doc.markdown", "x.mdown", "y.mkd", "z.mdx"} {
		if !IsMarkdown(name) {
			t.Fatalf("%s should count as markdown", name)
		}
	}
	for _, name := range []string{"a.txt", "photo.jpg", "md", "a.md.bak"} {
		if IsMarkdown(name) {
			t.Fatalf("%s should not count as markdown", name)
		}
	}
}

func TestGlamourStyle(t *testing.T) {
	lipgloss.Writer.Profile = colorprofile.Ascii
	if GlamourStyle() != "ascii" {
		t.Fatalf("no-color must use the ascii style, got %s", GlamourStyle())
	}
	lipgloss.Writer.Profile = colorprofile.TrueColor
	if GlamourStyle() != "pink" {
		t.Fatalf("color terminal must use the pink style, got %s", GlamourStyle())
	}
	lipgloss.Writer.Profile = colorprofile.NoTTY
	if GlamourStyle() != "ascii" {
		t.Fatalf("pipes must use the ascii style, got %s", GlamourStyle())
	}
}

func TestTermWidthBounds(t *testing.T) {
	w := TermWidth()
	if w < 40 || w > 100 {
		t.Fatalf("TermWidth out of clamp: %d", w)
	}
}

func TestChromeAtoms(t *testing.T) {
	lipgloss.Writer.Profile = colorprofile.TrueColor
	if !strings.Contains(Badge(true), "✓") || !strings.Contains(Badge(false), "✗") {
		t.Fatal("badges must carry ✓/✗")
	}
	if !strings.Contains(Chip("cyb files ls"), "cyb files ls") {
		t.Fatal("chip lost its command")
	}
	if !strings.Contains(stripANSI(PanelTitled("next", "body")), "next") {
		t.Fatal("titled panel lost its title")
	}
	steps := NextSteps("next", [2]string{"cyb sh \"help\"", "the verbs"})
	if plain := stripANSI(steps); !strings.Contains(plain, "cyb sh") || !strings.Contains(plain, "the verbs") {
		t.Fatalf("next-steps lost content:\n%s", plain)
	}
	if stripANSI(List("a", "b")) != "▸ a\n▸ b" {
		t.Fatalf("list markers wrong: %q", stripANSI(List("a", "b")))
	}
	if stripANSI(Flag("encrypted")) != "‹encrypted›" {
		t.Fatalf("flag chips wrong: %q", stripANSI(Flag("encrypted")))
	}
	if stripANSI(Number.Render("42")) != "42" {
		t.Fatal("number must keep its digits")
	}
	if !strings.Contains(StatusDot(true), "●") || !strings.Contains(StatusDot(false), "●") {
		t.Fatal("status dots must carry ●")
	}
	if !strings.Contains(stripANSI(Panel.Render("hi")), "hi") {
		t.Fatalf("panel must keep its body: %q", stripANSI(Panel.Render("hi")))
	}
}
