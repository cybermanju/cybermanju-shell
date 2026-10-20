// CyberManju OS — Sh (cli/cmd/sh.go).
// Role: cybsh CLI command.
// Exports: replModel, Init, Update, View.
// Integrates: bufio, fmt, os, strings, github.com, golang.org.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"bufio"
	"fmt"
	"os"
	"strings"

	"github.com/spf13/cobra"
	"golang.org/x/term"

	"charm.land/bubbles/v2/textinput"
	"charm.land/bubbles/v2/viewport"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/client"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/config"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	shDry  bool
	shLint bool
	shFmt  bool
)

var shCmd = &cobra.Command{
	Use:   "sh <cybsh line...>",
	Short: "Run one cybsh line on the dashboard (same shell everywhere)",
	Long: `Examples:
  cyb sh "disk list"  |  cyb sh "sync move <file> <a> <b>"  |  cyb sh "ai ask \"hi\""
Full verb list: cyb sh "help" · 3D wiki https://cybermanju.github.io/wiki/`,
	Args: cobra.MinimumNArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		line := strings.Join(args, " ")
		if flagJSON {
			raw, err := c.Raw("POST", "/api/os/exec", map[string]any{"line": line})
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		res, err := c.Exec(line)
		if err != nil {
			return err
		}
		if res.Output != "" {
			fmt.Println(res.Output)
		}
		if !res.OK {
			msg := res.Error
			if msg == "" {
				msg = "shell error"
			}
			return fmt.Errorf("%s", msg)
		}
		return nil
	},
}

var runCmd = &cobra.Command{
	Use:   "run <file.cybsh> [-- args...]",
	Short: "Run a .cybsh script (dry/lint/fmt supported)",
	Args:  cobra.MinimumNArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		file := args[0]
		rest := args[1:]
		// `--` separator: everything after it becomes script argv.
		var argv []string
		for i, a := range rest {
			if a == "--" {
				argv = rest[i+1:]
				rest = rest[:i]
				break
			}
		}
		_ = rest
		line := "run " + quoteArg(file)
		if shDry {
			line += " --dry"
		}
		if shLint {
			line += " --lint"
		}
		if shFmt {
			line += " --fmt"
		}
		if flagJSON {
			line += " --json"
		}
		if len(argv) > 0 {
			quoted := make([]string, len(argv))
			for i, a := range argv {
				quoted[i] = quoteArg(a)
			}
			line += " -- " + strings.Join(quoted, " ")
		}
		if flagJSON {
			raw, err := c.Raw("POST", "/api/os/exec", map[string]any{"line": line})
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		res, err := c.Exec(line)
		if err != nil {
			return err
		}
		if res.Output != "" {
			fmt.Println(res.Output)
		}
		if !res.OK {
			msg := res.Error
			if msg == "" {
				msg = "script failed"
			}
			return fmt.Errorf("%s", msg)
		}
		return nil
	},
}

func quoteArg(s string) string {
	if s == "" || strings.ContainsAny(s, " \t\"'\\") {
		return "\"" + strings.ReplaceAll(s, "\"", "\\\"") + "\""
	}
	return s
}

func init() {
	runCmd.Flags().BoolVar(&shDry, "dry", false, "parse and budget without executing")
	runCmd.Flags().BoolVar(&shLint, "lint", false, "lint only")
	runCmd.Flags().BoolVar(&shFmt, "fmt", false, "print the formatted script")
	rootCmd.AddCommand(shCmd, runCmd, replCmd)
}

// ── REPL ──────────────────────────────────────────────────────────

// replModel is the Bubble Tea REPL: viewport scrollback + textinput line.
type replModel struct {
	client  *client.Client
	view    viewport.Model
	input   textinput.Model
	history []string
	histIdx int
	lines   []string
	ready   bool
}

func newReplModel(c *client.Client, history []string) replModel {
	ti := textinput.New()
	ti.Placeholder = "disk list  |  sync move <file> <a> <b>  |  ai ask \"hi\""
	ti.Prompt = "cybsh> "
	styles := textinput.DefaultDarkStyles()
	styles.Focused.Prompt = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color(ui.PinkHot))
	styles.Focused.Text = lipgloss.NewStyle().Foreground(lipgloss.Color(ui.IceWhite))
	ti.SetStyles(styles)
	ti.Focus()
	return replModel{client: c, input: ti, history: history, histIdx: len(history)}
}

func (m replModel) Init() tea.Cmd { return textinput.Blink }

func (m replModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmds []tea.Cmd
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		border := lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(lipgloss.Color(ui.PinkDeep))
		// Clamp for postage-stamp terminals — bubbles misbehaves at ≤0.
		vw, vh, iw := max(20, msg.Width-2), max(6, msg.Height-4), max(10, msg.Width-10)
		if !m.ready {
			m.view = viewport.New(
				viewport.WithWidth(vw),
				viewport.WithHeight(vh),
			)
			m.view.Style = border
			m.ready = true
		} else {
			m.view.SetWidth(vw)
			m.view.SetHeight(vh)
		}
		m.input.SetWidth(iw)
	case tea.KeyPressMsg:
		switch msg.String() {
		case "ctrl+c", "esc":
			return m, tea.Quit
		case "enter":
			line := strings.TrimSpace(m.input.Value())
			m.input.Reset()
			m.histIdx = len(m.history)
			if line == "" {
				return m, nil
			}
			m.history = append(m.history, line)
			appendHistoryLine(line)
			if line == ":quit" || line == ":exit" || line == ":q" {
				return m, tea.Quit
			}
			if line == ":clear" {
				m.lines = nil
				m.view.SetContent("")
				return m, nil
			}
			if line == ":help" {
				m.emit("  " + ui.Key(":help") + ui.Dim.Render(" this help   ") +
					ui.Key(":clear") + ui.Dim.Render(" clear screen   ") +
					ui.Key(":quit") + ui.Dim.Render(" leave"))
				return m, nil
			}
			res, err := m.client.Exec(line)
			if err != nil {
				m.emit(ui.Err.Render("  " + err.Error()))
				return m, nil
			}
			if res.Output != "" {
				for _, l := range strings.Split(strings.TrimRight(res.Output, "\n"), "\n") {
					m.emit("  " + l)
				}
			}
			if !res.OK && res.Error != "" {
				m.emit(ui.Err.Render("  " + res.Error))
			}
			return m, nil
		case "up":
			if m.histIdx > 0 {
				m.histIdx--
				m.input.SetValue(m.history[m.histIdx])
			}
			return m, nil
		case "down":
			if m.histIdx < len(m.history) {
				m.histIdx++
				if m.histIdx == len(m.history) {
					m.input.Reset()
				} else {
					m.input.SetValue(m.history[m.histIdx])
				}
			}
			return m, nil
		}
	}
	var cmd tea.Cmd
	m.input, cmd = m.input.Update(msg)
	cmds = append(cmds, cmd)
	if m.ready {
		var vcmd tea.Cmd
		m.view, vcmd = m.view.Update(msg)
		cmds = append(cmds, vcmd)
	}
	return m, tea.Batch(cmds...)
}

func (m *replModel) emit(s string) {
	m.lines = append(m.lines, s)
	if len(m.lines) > 500 {
		m.lines = m.lines[len(m.lines)-500:]
	}
	m.view.SetContent(lipgloss.JoinVertical(lipgloss.Left, m.lines...))
	m.view.GotoBottom()
}

func (m replModel) View() tea.View {
	if !m.ready {
		return tea.NewView(ui.Dim.Render("warming up the neon…"))
	}
	help := "  " + ui.Key(":help") + ui.Dim.Render(" commands  ") +
		ui.Key("↑/↓") + ui.Dim.Render(" history  ") +
		ui.Key("esc") + ui.Dim.Render(" quits")
	return tea.NewView(m.view.View() + "\n" + m.input.View() + "\n" + help)
}

var histFile string

func appendHistoryLine(line string) {
	if histFile == "" {
		return
	}
	f, err := os.OpenFile(histFile, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o600)
	if err != nil {
		return
	}
	defer f.Close()
	fmt.Fprintln(f, line)
}

func loadHistory() []string {
	var out []string
	path, err := config.HistoryPath()
	if err != nil {
		return out
	}
	histFile = path
	raw, err := os.ReadFile(path)
	if err != nil {
		return out
	}
	for _, l := range strings.Split(string(raw), "\n") {
		if strings.TrimSpace(l) != "" {
			out = append(out, l)
		}
	}
	return out
}

var replCmd = &cobra.Command{
	Use:   "repl",
	Short: "Interactive cybsh (Bubble Tea UI, history kept, esc quits)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		history := loadHistory()
		if !term.IsTerminal(int(os.Stdin.Fd())) {
			return plainRepl(c, history)
		}
		p := tea.NewProgram(newReplModel(c, history),
			tea.WithColorProfile(ui.ColorProfile()),
		)
		_, err := p.Run()
		return err
	},
}

// plainRepl is the pipe-friendly fallback (no TTY).
func plainRepl(c *client.Client, history []string) error {
	ui.Emit(ui.Banner(cliVersion) + "  " + ui.Dim.Render("cybsh REPL — :quit to leave"))
	in := bufio.NewScanner(os.Stdin)
	in.Buffer(make([]byte, 1024*1024), 1024*1024)
	for {
		fmt.Print(ui.Downsample(ui.Accent.Render("cybsh> ")))
		if !in.Scan() {
			fmt.Println()
			return nil
		}
		line := strings.TrimSpace(in.Text())
		if line == "" {
			continue
		}
		if line == ":quit" || line == ":exit" || line == ":q" {
			return nil
		}
		appendHistoryLine(line)
		res, err := c.Exec(line)
		if err != nil {
			ui.Failure("%v", err)
			continue
		}
		if res.Output != "" {
			fmt.Println(res.Output)
		}
		if !res.OK && res.Error != "" {
			ui.Failure("%s", res.Error)
		}
	}
}
