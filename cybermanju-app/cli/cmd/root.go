// CyberManju OS — Root (cli/cmd/root.go).
// Package cmd is the `cyb` command tree.
package cmd

import (
	"fmt"
	"strings"
	"time"

	"charm.land/huh/v2"
	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/client"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/config"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	flagServer  string
	flagToken   string
	flagJSON    bool
	flagNoColor bool
	flagTimeout time.Duration
	flagYes     bool

	cliVersion string
	cliCommit  string
)

// Execute runs the CLI (version/commit stamped by CI ldflags).
func Execute(version, commit string) error {
	cliVersion, cliCommit = version, commit
	return rootCmd.Execute()
}

var rootCmd = &cobra.Command{
	Use:   "cyb",
	Short: "Universal CyberManju OS terminal",
	Long: `cyb — files, disks, sync, OAuth, cybsh and the AI agent from any terminal.

Talk to a dashboard (local Docker, desktop app, or remote server).
Start here:  cyb setup

Docs:  3D wiki https://cybermanju.github.io/wiki/ · agents: https://cybermanju.github.io/llms.txt`,
	SilenceUsage:  true,
	SilenceErrors: true,
	PersistentPreRun: func(cmd *cobra.Command, args []string) {
		if flagNoColor {
			ui.NoColor()
		}
	},
	Run: func(cmd *cobra.Command, args []string) {
		_ = cmd.Help()
	},
}

func init() {
	rootCmd.PersistentFlags().StringVarP(&flagServer, "server", "s", "", "dashboard URL (env CYBERMANJU_SERVER, else saved)")
	rootCmd.PersistentFlags().StringVar(&flagToken, "token", "", "JWT (env CYBERMANJU_TOKEN, else saved)")
	rootCmd.PersistentFlags().BoolVar(&flagJSON, "json", false, "print raw JSON responses")
	rootCmd.PersistentFlags().BoolVar(&flagNoColor, "no-color", false, "disable Lipgloss styling")
	rootCmd.PersistentFlags().DurationVar(&flagTimeout, "timeout", 15*time.Second, "HTTP timeout")
	rootCmd.PersistentFlags().BoolVarP(&flagYes, "yes", "y", false, "skip confirmations")
	rootCmd.AddCommand(versionCmd)
	installHelp()
}

var versionCmd = &cobra.Command{
	Use:   "version",
	Short: "Print the CLI version",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Printf("%s  %s %s\n",
			ui.Downsample(ui.Gradient("◈ cyb", ui.PinkHot, ui.RedHot)),
			ui.Downsample(ui.Accent.Render("v"+strings.TrimPrefix(cliVersion, "v"))),
			ui.Downsample(ui.Dim.Render("(commit "+cliCommit+")")))
	},
}

// profile loads the saved profile.
func profile() config.File {
	f, err := config.Load()
	if err != nil {
		ui.Failure("cannot read profile: %v", err)
	}
	return f
}

// mustClient builds a client for the selected backend. In supabase-direct
// limited mode there is no dashboard (empty base URL) — requireAuth refuses
// dashboard commands with guidance instead of dialing nowhere.
func mustClient() *client.Client {
	f := profile()
	return client.New(
		config.ResolveDashboard(flagServer, f),
		config.ResolveToken(flagToken, f),
		flagTimeout,
	)
}

// requireAuth errors when no (or an expired) JWT is configured, or when the
// selected backend is dashboard-less.
func requireAuth(c *client.Client) error {
	if c.BaseURL == "" {
		return fmt.Errorf("supabase limited mode: this command needs a dashboard — `cyb backend use docker|native|remote` (read-only here: `cyb supa repos`)")
	}
	if c.Token == "" {
		return fmt.Errorf("not logged in — run `cyb login` (server %s)", c.BaseURL)
	}
	// Flag/env tokens have unknown age; only the saved profile expires loudly.
	if flagToken == "" && profile().Expired() {
		return fmt.Errorf("session expired — run `cyb login` (server %s)", c.BaseURL)
	}
	return nil
}

// serverOf reports the effective dashboard URL (for messages).
func serverOf(c *client.Client) string { return c.BaseURL }

// printRaw prints raw JSON when --json is set.
func printRaw(raw []byte) error {
	if flagJSON {
		return ui.PrintJSON(raw)
	}
	return nil
}

// confirmDanger asks for an explicit yes (or --yes) before destruction.
func confirmDanger(question string) bool {
	if flagYes {
		return true
	}
	var ok bool
	form := ui.NewForm(huh.NewGroup(
		huh.NewConfirm().Title(question).Value(&ok),
	))
	if err := form.Run(); err != nil {
		return false
	}
	return ok
}
