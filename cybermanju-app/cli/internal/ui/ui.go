// CyberManju OS — Ui (cli/internal/ui/ui.go).
// Package ui is the Charm v2 face of `cyb`: one Lipgloss cyberpunk-neon
// theme (hot pink into signal red, neon cyan + hazard yellow accents), a
// gradient banner, Bubble Tea spinner, gradient progress downloads, neon
// tables with header rules and zebra rows, syntax-tinted JSON, and
// width-aware Glamour markdown shared by every command. Builders emit
// full-fidelity ANSI; every print boundary downsamples to the active
// colorprofile (NoTTY when --no-color), so stdout stays pipe-clean and
// honest. Interactive programs (spinner, Huh forms, REPL) are pinned to
// the same profile via tea.WithColorProfile.
package ui

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/muesli/reflow/truncate"
	"golang.org/x/term"

	"charm.land/bubbles/v2/progress"
	"charm.land/bubbles/v2/spinner"
	tea "charm.land/bubbletea/v2"
	"charm.land/glamour/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/colorprofile"
)

var (
	// Title renders command headers in hot pink.
	Title = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(PinkHot))
	// OK renders success lines in hot pink.
	OK = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(PinkHot))
	// Err renders failure lines in signal red.
	Err = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(RedHot))
	// Warn renders caution lines in deep pink.
	Warn = lipgloss.NewStyle().Foreground(lipgloss.Color("#FF7AB8"))
	// Dim renders secondary text.
	Dim = lipgloss.NewStyle().Foreground(lipgloss.Color(DimGray))
	// Accent renders ids, prompts and highlights in hot pink.
	Accent = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(PinkHot))
	// Panel boxes notes and next steps in a pink rounded frame.
	Panel = lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color(PinkHot)).
		Padding(0, 1)
)

// NoColor disables all styling (for pipes and --no-color) by pinning the
// Lipgloss writer to the NoTTY profile, which strips every escape sequence.
// (The Ascii profile keeps non-color SGR like bold — not plain enough.)
func NoColor() {
	lipgloss.Writer.Profile = colorprofile.NoTTY
}

// ColorProfile reports the active Lipgloss output profile (tests pin it;
// production auto-detects from the terminal).
func ColorProfile() colorprofile.Profile {
	return lipgloss.Writer.Profile
}

// Banner is the `cyb` mark in a pink → red gradient.
func Banner(version string) string {
	return Gradient("◈ cybermanju", PinkHot, RedHot) + " " + Dim.Render("v"+strings.TrimPrefix(version, "v"))
}

// Success prints a pink check line.
func Success(format string, args ...any) {
	fmt.Fprintln(os.Stdout, Downsample(OK.Render("✓ ")+fmt.Sprintf(format, args...)))
}

// Failure prints a red cross line.
func Failure(format string, args ...any) {
	fmt.Fprintln(os.Stderr, Downsample(Err.Render("✗ ")+fmt.Sprintf(format, args...)))
}

// Info prints a dim informational line.
func Info(format string, args ...any) {
	fmt.Fprintln(os.Stdout, Downsample(Dim.Render("• ")+fmt.Sprintf(format, args...)))
}

// Header prints a neon section title.
func Header(format string, args ...any) {
	fmt.Fprintln(os.Stdout, Downsample(Title.Render("◈ "+fmt.Sprintf(format, args...))))
}

// Hint prints a dim next-step line to stderr so stdout stays pipe-clean.
// Wrap `cyb …` references in Chip for scanability.
func Hint(format string, args ...any) {
	fmt.Fprintln(os.Stderr, Downsample(Dim.Render("→  ")+fmt.Sprintf(format, args...)))
}

// Emit prints one pre-styled line to stdout, downsampled to the active
// color profile.
func Emit(s string) {
	fmt.Fprintln(os.Stdout, Downsample(s))
}

// Table renders neon columns: pink header row over a neon rule,
// ice-white cells, faint zebra striping, terminal-width truncation.
func Table(headers []string, rows [][]string) {
	Emit(RenderTable(headers, rows))
}

// RenderTable is the pure Table builder (stdout-free, unit-testable).
func RenderTable(headers []string, rows [][]string) string {
	widths := make([]int, len(headers))
	for i, h := range headers {
		widths[i] = lipgloss.Width(h)
	}
	for _, row := range rows {
		for i, cell := range row {
			if i < len(widths) {
				if w := lipgloss.Width(cell); w > widths[i] {
					widths[i] = w
				}
			}
		}
	}
	fitWidths(widths)
	pad := func(s string, w int) string {
		// truncate.StringWithTail counts the tail inside the limit, so
		// only truncate cells that actually overflow (exact fits stay whole).
		if lipgloss.Width(s) > w {
			s = truncate.StringWithTail(s, uint(max(1, w)), "…")
		}
		return s + strings.Repeat(" ", max(0, w-lipgloss.Width(s)+1))
	}
	headStyle := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(PinkHot))
	ruleStyle := lipgloss.NewStyle().Foreground(lipgloss.Color(PinkDeep))
	zebra := lipgloss.NewStyle().Faint(true)
	var out strings.Builder
	var head, rule strings.Builder
	for i, h := range headers {
		head.WriteString(pad(headStyle.Render(h), widths[i]))
		rule.WriteString(pad(ruleStyle.Render(strings.Repeat("─", widths[i])), widths[i]))
	}
	out.WriteString(strings.TrimRight(head.String(), " ") + "\n")
	out.WriteString(strings.TrimRight(rule.String(), " ") + "\n")
	for ri, row := range rows {
		var line strings.Builder
		for i := range headers {
			cell := ""
			if i < len(row) {
				cell = row[i]
			}
			if ri%2 == 1 {
				cell = zebra.Render(cell)
			}
			line.WriteString(pad(cell, widths[i]))
		}
		out.WriteString(strings.TrimRight(line.String(), " "))
		if ri < len(rows)-1 {
			out.WriteString("\n")
		}
	}
	return out.String()
}

// fitWidths shrinks the widest columns (never below 8 cells) until the
// table fits the terminal.
func fitWidths(widths []int) {
	total := func() int {
		n := 0
		for _, w := range widths {
			n += w + 1
		}
		return n
	}
	for total() > TermWidth() {
		best, bi := 8, -1
		for i, w := range widths {
			if w > best {
				best, bi = w, i
			}
		}
		if bi < 0 {
			return
		}
		widths[bi]--
	}
}

// KV prints key: value rows.
func KV(pairs ...[2]string) {
	w := 0
	for _, p := range pairs {
		if lipgloss.Width(p[0]) > w {
			w = lipgloss.Width(p[0])
		}
	}
	keyStyle := lipgloss.NewStyle().Foreground(lipgloss.Color(PinkHot))
	for _, p := range pairs {
		Emit(fmt.Sprintf("%s  %s", keyStyle.Render(p[0]+strings.Repeat(" ", max(0, w-lipgloss.Width(p[0])))), p[1]))
	}
}

// JSON tint atoms: pink keys, ice strings, cyan numbers, yellow bools.
var (
	jsonKey   = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(PinkHot))
	jsonStr   = lipgloss.NewStyle().Foreground(lipgloss.Color(IceWhite))
	jsonNum   = lipgloss.NewStyle().Foreground(lipgloss.Color(CyanNeon))
	jsonBool  = lipgloss.NewStyle().Foreground(lipgloss.Color(YellowHz))
	jsonNull  = lipgloss.NewStyle().Faint(true).Foreground(lipgloss.Color(DimGray))
	jsonDelim = lipgloss.NewStyle().Foreground(lipgloss.Color(PinkDeep))
	jsonPunct = lipgloss.NewStyle().Foreground(lipgloss.Color(DimGray))
)

// PrintJSON pretty-prints raw bytes for --json, syntax-tinted on color
// terminals (plain pretty JSON when --no-color). Non-JSON passes through.
func PrintJSON(raw []byte) error {
	if s, ok := RenderJSON(raw); ok {
		fmt.Println(Downsample(s))
		return nil
	}
	fmt.Println(strings.TrimSpace(string(raw)))
	return nil
}

// RenderJSON is the pure JSON renderer (stdout-free, unit-testable):
// pretty-prints and tints; ok=false when raw is not a JSON value.
func RenderJSON(raw []byte) (out string, ok bool) {
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.UseNumber()
	var b strings.Builder
	if err := writeJSONValue(dec, &b, 0); err != nil {
		return "", false
	}
	if _, err := dec.Token(); err != io.EOF {
		return "", false
	}
	return b.String(), true
}

func writeJSONIndent(b *strings.Builder, indent int) {
	b.WriteString(strings.Repeat("  ", indent))
}

func writeJSONValue(dec *json.Decoder, b *strings.Builder, indent int) error {
	tok, err := dec.Token()
	if err != nil {
		return err
	}
	switch t := tok.(type) {
	case json.Delim:
		switch t {
		case '{':
			b.WriteString(jsonDelim.Render("{"))
			if !dec.More() {
				if ct, err := dec.Token(); err != nil {
					return err
				} else if d, isDelim := ct.(json.Delim); !isDelim || d != '}' {
					return fmt.Errorf("ui: unterminated object")
				}
				b.WriteString(jsonDelim.Render("}"))
				return nil
			}
			b.WriteString("\n")
			for dec.More() {
				writeJSONIndent(b, indent+1)
				kt, err := dec.Token()
				if err != nil {
					return err
				}
				ks, isStr := kt.(string)
				if !isStr {
					return fmt.Errorf("ui: object key is not a string")
				}
				b.WriteString(jsonKey.Render(strconv.Quote(ks)))
				b.WriteString(jsonPunct.Render(": "))
				if err := writeJSONValue(dec, b, indent+1); err != nil {
					return err
				}
				if dec.More() {
					b.WriteString(jsonPunct.Render(","))
				}
				b.WriteString("\n")
			}
			if ct, err := dec.Token(); err != nil {
				return err
			} else if d, isDelim := ct.(json.Delim); !isDelim || d != '}' {
				return fmt.Errorf("ui: unterminated object")
			}
			writeJSONIndent(b, indent)
			b.WriteString(jsonDelim.Render("}"))
			return nil
		case '[':
			b.WriteString(jsonDelim.Render("["))
			if !dec.More() {
				if ct, err := dec.Token(); err != nil {
					return err
				} else if d, isDelim := ct.(json.Delim); !isDelim || d != ']' {
					return fmt.Errorf("ui: unterminated array")
				}
				b.WriteString(jsonDelim.Render("]"))
				return nil
			}
			b.WriteString("\n")
			for dec.More() {
				writeJSONIndent(b, indent+1)
				if err := writeJSONValue(dec, b, indent+1); err != nil {
					return err
				}
				if dec.More() {
					b.WriteString(jsonPunct.Render(","))
				}
				b.WriteString("\n")
			}
			if ct, err := dec.Token(); err != nil {
				return err
			} else if d, isDelim := ct.(json.Delim); !isDelim || d != ']' {
				return fmt.Errorf("ui: unterminated array")
			}
			writeJSONIndent(b, indent)
			b.WriteString(jsonDelim.Render("]"))
			return nil
		default:
			return fmt.Errorf("ui: unexpected delimiter %v", t)
		}
	case string:
		b.WriteString(jsonStr.Render(strconv.Quote(t)))
	case json.Number:
		b.WriteString(jsonNum.Render(t.String()))
	case bool:
		b.WriteString(jsonBool.Render(strconv.FormatBool(t)))
	case nil:
		b.WriteString(jsonNull.Render("null"))
	default:
		return fmt.Errorf("ui: unexpected token %T", tok)
	}
	return nil
}

// TermWidth reports the stdout terminal width clamped to 40..100
// (80 when stdout is not a terminal, e.g. pipes and CI logs).
func TermWidth() int {
	if w, _, err := term.GetSize(int(os.Stdout.Fd())); err == nil && w > 0 {
		return min(max(w, 40), 100)
	}
	return 80
}

// GlamourStyle picks the markdown stylesheet for the active color profile:
// ascii when color is off, the cyberpunk pink style on color terminals.
func GlamourStyle() string {
	if ColorProfile() <= colorprofile.Ascii {
		return "ascii"
	}
	return "pink"
}

// IsMarkdown reports whether name looks like a markdown document.
func IsMarkdown(name string) bool {
	lower := strings.ToLower(name)
	for _, ext := range []string{".md", ".markdown", ".mdown", ".mkd", ".mdx"} {
		if strings.HasSuffix(lower, ext) {
			return true
		}
	}
	return false
}

// Downsample re-emits styled text for the active color profile (strips ANSI
// entirely when --no-color).
func Downsample(s string) string {
	return lipgloss.Sprint(s)
}

// RenderMarkdown glamour-renders markdown for the terminal: word-wrapped to
// the terminal width, styled per the color profile (GLAMOUR_STYLE wins when
// set), colors downsampled via Lipgloss. Unrenderable input passes through.
func RenderMarkdown(text string) string {
	opts := []glamour.TermRendererOption{glamour.WithWordWrap(TermWidth())}
	if os.Getenv("GLAMOUR_STYLE") != "" {
		opts = append(opts, glamour.WithEnvironmentConfig())
	} else {
		opts = append(opts, glamour.WithStandardStyle(GlamourStyle()))
	}
	r, err := glamour.NewTermRenderer(opts...)
	if err != nil {
		return text
	}
	out, err := r.Render(text)
	if err != nil {
		return text
	}
	return Downsample(strings.TrimRight(out, "\n"))
}

// isTTY reports whether stderr is an interactive terminal.
func isTTY() bool {
	return term.IsTerminal(int(os.Stderr.Fd()))
}

// ── Bubble Tea spinner ────────────────────────────────────────────

type spinDoneMsg struct{ err error }

type spinModel struct {
	spinner spinner.Model
	msg     string
	frame   int
	done    chan error
}

func newSpinModel(msg string, done chan error) spinModel {
	s := spinner.New(
		spinner.WithSpinner(spinner.Dot),
		spinner.WithStyle(lipgloss.NewStyle().Foreground(lipgloss.Color(PinkHot))),
	)
	return spinModel{spinner: s, msg: msg, done: done}
}

// pinkFrames pulses the message dot between pink and red shapes.
var pinkFrames = []string{"◈", "◉", "⬢", "⬣"}

func (m spinModel) Init() tea.Cmd {
	return tea.Batch(
		m.spinner.Tick,
		func() tea.Msg { return spinDoneMsg{<-m.done} },
	)
}

func (m spinModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case spinDoneMsg:
		return m, tea.Quit
	case spinner.TickMsg:
		var cmd tea.Cmd
		m.spinner, cmd = m.spinner.Update(msg)
		m.frame++
		return m, cmd
	}
	return m, nil
}

func (m spinModel) View() tea.View {
	dot := Gradient(pinkFrames[m.frame%len(pinkFrames)], PinkHot, RedHot)
	return tea.NewView(fmt.Sprintf("%s %s  %s", m.spinner.View(), dot, m.msg))
}

// SpinWhile runs fn under a hot-pink Bubble Tea spinner (plain fallback
// when stderr is not a TTY, e.g. pipes and CI logs).
func SpinWhile(msg string, fn func() error) error {
	done := make(chan error, 1)
	go func() { done <- fn() }()
	if !isTTY() {
		err := <-done
		return err
	}
	p := tea.NewProgram(newSpinModel(msg, done),
		tea.WithOutput(os.Stderr),
		tea.WithColorProfile(ColorProfile()),
	)
	if _, err := p.Run(); err != nil {
		return <-done
	}
	return <-done
}

// ── Progress download ─────────────────────────────────────────────

type progressReader struct {
	r     io.Reader
	n     int64
	total int64
	bar   progress.Model
	last  time.Time
}

func (p *progressReader) Read(b []byte) (int, error) {
	n, err := p.r.Read(b)
	p.n += int64(n)
	if p.total > 0 && time.Since(p.last) > 80*time.Millisecond {
		p.last = time.Now()
		fmt.Fprintf(os.Stderr, "\r%s", Downsample(p.bar.ViewAs(float64(p.n)/float64(p.total))))
	}
	return n, err
}

// Download fetches url to dest with a pink → red progress bar (plain copy
// when not a TTY). Returns bytes written.
func Download(url, dest string) (int64, error) {
	res, err := http.Get(url) //nolint:gosec // release artifact download
	if err != nil {
		return -1, err
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		return -1, fmt.Errorf("network: download %d", res.StatusCode)
	}
	f, err := os.Create(dest)
	if err != nil {
		return -1, err
	}
	defer f.Close()
	total := res.ContentLength
	if !isTTY() || total <= 0 {
		n, err := io.Copy(f, res.Body)
		return n, err
	}
	bar := progress.New(
		progress.WithColors(lipgloss.Color(PinkHot), lipgloss.Color(RedHot)),
		progress.WithWidth(40),
	)
	pr := &progressReader{r: res.Body, total: total, bar: bar, last: time.Now()}
	n, err := io.Copy(f, pr)
	fmt.Fprintf(os.Stderr, "\r%s\n", Downsample(bar.ViewAs(1)))
	return n, err
}
