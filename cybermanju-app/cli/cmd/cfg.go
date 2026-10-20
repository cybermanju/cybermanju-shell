// CyberManju OS — Cfg (cli/cmd/cfg.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: github.com.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/config"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var configCmd = &cobra.Command{
	Use:   "config",
	Short: "Show the saved profile",
	RunE: func(cmd *cobra.Command, args []string) error {
		f := profile()
		path, _ := config.Path()
		token := "unset"
		if config.ResolveToken(flagToken, f) != "" {
			token = "set (hidden)"
		}
		ui.KV(
			[2]string{"file", path},
			[2]string{"server", config.ResolveServer(flagServer, f)},
			[2]string{"token", token},
		)
		return nil
	},
}

var configServerCmd = &cobra.Command{
	Use:   "set-server <url>",
	Short: "Save the dashboard URL",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		f := profile()
		f.Server = args[0]
		if err := config.Save(f); err != nil {
			return err
		}
		ui.Success("server → %s", f.Server)
		return nil
	},
}

func init() {
	configCmd.AddCommand(configServerCmd)
	rootCmd.AddCommand(configCmd)
}
