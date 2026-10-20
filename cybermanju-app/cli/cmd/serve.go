// CyberManju OS — Serve (cli/cmd/serve.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: fmt, os, os/exec, filepath, runtime, strings, github.com, --name.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/client"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

const (
	serveImage = "ghcr.io/cybermanju/cybermanju-os:latest"
	serveName  = "cybermanju-os"
)

var (
	servePort   int
	serveData   string
	serveImageF string
)

var serveCmd = &cobra.Command{
	Use:   "serve",
	Short: "Run the dashboard in Docker (up/status/stop)",
}

var serveUpCmd = &cobra.Command{
	Use:   "up",
	Short: "Start the dashboard container",
	RunE: func(cmd *cobra.Command, args []string) error {
		if err := ensureDockerUp(servePort, serveData, serveImageF); err != nil {
			return err
		}
		ui.Success("dashboard up → http://127.0.0.1:%d", servePort)
		ui.Hint("next: %s", ui.Chip(fmt.Sprintf("cyb setup --server http://127.0.0.1:%d", servePort)))
		return nil
	},
}

// ensureDockerUp pulls and (re)starts the dashboard container. Shared by
// `serve up` and `backend use docker`.
func ensureDockerUp(port int, dataDir, image string) error {
	if runtime.GOOS == "android" {
		return fmt.Errorf("unsupported: Docker is unavailable in Android/Termux; use remote or CLI Limited")
	}
	if _, err := exec.LookPath("docker"); err != nil {
		return fmt.Errorf("docker not found — install Docker or point cyb at a dashboard with --server")
	}
	data := dataDir
	if data == "" {
		home, _ := os.UserHomeDir()
		data = filepath.Join(home, ".local", "share", "cybermanju-os", "data")
	}
	if err := os.MkdirAll(data, 0o700); err != nil {
		return err
	}
	if image == "" {
		image = serveImage
	}
	ui.Info("pulling %s…", image)
	if out, err := exec.Command("docker", "pull", image).CombinedOutput(); err != nil {
		return fmt.Errorf("docker pull failed: %v\n%s", err, out)
	}
	// Fresh container each up (idempotent): stop+rm a stale one first.
	exec.Command("docker", "rm", "-f", serveName).Run()
	run := exec.Command("docker", "run", "-d",
		"--name", serveName,
		"-p", fmt.Sprintf("127.0.0.1:%d:3456", port),
		"-v", data+":/data",
		image)
	if out, err := run.CombinedOutput(); err != nil {
		return fmt.Errorf("docker run failed: %v\n%s", err, out)
	}
	ui.Success("dashboard container up (data: %s)", data)
	return nil
}

var serveStatusCmd = &cobra.Command{
	Use:   "status",
	Short: "Check whether a dashboard answers",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		var st client.AuthStatus
		if err := ui.SpinWhile("Probing "+c.BaseURL+"…", func() error {
			var err error
			st, err = c.AuthStatus()
			return err
		}); err != nil {
			ui.Failure("no dashboard at %s", c.BaseURL)
			return fmt.Errorf("unreachable")
		}
		ui.Success("dashboard reachable at %s", c.BaseURL)
		ui.KV(
			[2]string{"registration", map[bool]string{true: "open (first run)", false: "closed"}[st.RegistrationOpen]},
		)
		if runtime.GOOS != "android" {
			if out, err := exec.Command("docker", "ps", "--filter", "name="+serveName, "--format", "{{.Status}}").Output(); err == nil {
				if s := strings.TrimSpace(string(out)); s != "" {
					ui.Info("container: %s", s)
				}
			}
		}
		return nil
	},
}

var serveStopCmd = &cobra.Command{
	Use:   "stop",
	Short: "Stop and remove the dashboard container",
	RunE: func(cmd *cobra.Command, args []string) error {
		if runtime.GOOS == "android" {
			return fmt.Errorf("unsupported: Docker is unavailable in Android/Termux; use remote or CLI Limited")
		}
		if out, err := exec.Command("docker", "rm", "-f", serveName).CombinedOutput(); err != nil {
			return fmt.Errorf("docker rm failed: %v\n%s", err, out)
		}
		ui.Success("dashboard container stopped")
		return nil
	},
}

func init() {
	serveUpCmd.Flags().IntVar(&servePort, "port", 3456, "host port to publish")
	serveUpCmd.Flags().StringVar(&serveData, "data", "", "host data dir (default ~/.local/share/cybermanju-os/data)")
	serveUpCmd.Flags().StringVar(&serveImageF, "image", "", "image to run (default "+serveImage+")")
	serveCmd.AddCommand(serveUpCmd, serveStatusCmd, serveStopCmd)
	rootCmd.AddCommand(serveCmd)
}
