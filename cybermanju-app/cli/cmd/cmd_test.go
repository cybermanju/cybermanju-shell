// CyberManju OS — Cmd test (cli/cmd/cmd_test.go).
// Role: CLI command wiring tests — flags newcomers rely on must exist.
// Exports: TestFilesCatRenderFlag, TestContentName,
// TestFilesCatRenderEndToEnd, TestRootHelpAtlasComplete.
// Integrates: testing, httptest, cmd package internals.
// Contracts: flag shape + pure helpers + fake-dashboard e2e only — no real
// network, no real profile.
package cmd

import (
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/colorprofile"
)

func TestFilesCatRenderFlag(t *testing.T) {
	f := filesCatCmd.Flags().Lookup("render")
	if f == nil {
		t.Fatal("files cat must expose --render")
	}
	if f.Shorthand != "r" {
		t.Fatalf("render shorthand must be -r, got %q", f.Shorthand)
	}
	if f.DefValue != "false" {
		t.Fatalf("render must default off, got %q", f.DefValue)
	}
}

func TestContentName(t *testing.T) {
	if got := contentName([]byte(`{"name":"notes.md","content":"# hi"}`)); got != "notes.md" {
		t.Fatalf("want notes.md, got %q", got)
	}
	if got := contentName([]byte(`not json`)); got != "" {
		t.Fatalf("unexpected shapes must yield empty, got %q", got)
	}
}

// TestFilesCatRenderEndToEnd serves a fake dashboard and asserts `cat -r`
// glamour-renders the content instead of dumping the JSON envelope.
func TestFilesCatRenderEndToEnd(t *testing.T) {
	lipgloss.Writer.Profile = colorprofile.Ascii
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/files/note1/content" {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"fileId":"note1","name":"notes.md","content":"# hello vault","sizeBytes":13}`)
	}))
	defer srv.Close()

	oldServer, oldToken, oldRender := flagServer, flagToken, filesCatRender
	flagServer, flagToken, filesCatRender = srv.URL, "tok", true
	defer func() { flagServer, flagToken, filesCatRender = oldServer, oldToken, oldRender }()

	out := captureStdout(t, func() {
		if err := filesCatCmd.RunE(filesCatCmd, []string{"note1"}); err != nil {
			t.Fatalf("cat -r failed: %v", err)
		}
	})
	if !strings.Contains(out, "hello vault") {
		t.Fatalf("rendered output lost the heading: %q", out)
	}
	if strings.Contains(out, `"content"`) {
		t.Fatalf("rendered output leaked the JSON envelope: %q", out)
	}
}

// TestRootHelpAtlasComplete guards the neon atlas: every visible top-level
// command must appear in a section (no silent "more" fallback), every
// section must render, and a leaf page must show its flags.
func TestRootHelpAtlasComplete(t *testing.T) {
	// Mirror production: Execute() lazily adds the completion command.
	rootCmd.InitDefaultCompletionCmd()
	out := captureStdout(t, func() { renderRootHelp(rootCmd, nil) })
	for _, sec := range helpSections {
		if !strings.Contains(out, sec.title) {
			t.Fatalf("atlas lost section %q", sec.title)
		}
		for _, name := range sec.cmds {
			if !strings.Contains(out, name) {
				t.Fatalf("atlas lost command %q", name)
			}
		}
	}
	if strings.Contains(out, "◈ more") {
		t.Fatal("atlas grew a 'more' section — map the new command into helpSections")
	}
	for _, sub := range rootCmd.Commands() {
		if sub.Hidden || sub.Deprecated != "" || sub.Name() == "help" || sub.Name() == "completion" {
			continue
		}
		if !strings.Contains(out, sub.Name()) {
			t.Fatalf("atlas hides top-level command %q", sub.Name())
		}
	}
	leaf := captureStdout(t, func() { renderRootHelp(filesCatCmd, nil) })
	if !strings.Contains(leaf, "--render") || !strings.Contains(leaf, "cyb files cat") {
		t.Fatalf("leaf help must show usage + flags, got:\n%s", leaf)
	}
}

// captureStdout swaps os.Stdout for the duration of fn (ui prints there).
func captureStdout(t *testing.T, fn func()) string {
	t.Helper()
	old := os.Stdout
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	os.Stdout = w
	defer func() { os.Stdout = old }()
	fn()
	_ = w.Close()
	raw, err := io.ReadAll(r)
	if err != nil {
		t.Fatal(err)
	}
	return string(raw)
}
