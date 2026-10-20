// CyberManju OS — Main (cli/main.go).
// Command cyb is the universal CyberManju OS terminal: files, disks, sync,
// OAuth, cybsh and the AI agent over the dashboard REST API, with a Charm
// (Lipgloss/Bubbles/Huh/Glamour) face.
package main

import (
	"os"

	"github.com/cybermanju/cybermanju.github.io/cli/cmd"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	// Stamped by CI: go build -ldflags "-X main.version=vX.Y.Z -X main.commit=SHA".
	version = "dev"
	commit  = "none"
)

func main() {
	if err := cmd.Execute(version, commit); err != nil {
		ui.Failure("%v", err)
		os.Exit(1)
	}
}
