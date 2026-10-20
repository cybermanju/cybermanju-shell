// CyberManju OS — Oauth (cli/cmd/oauth.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: fmt, github.com.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"fmt"

	"charm.land/huh/v2"
	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/client"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var oauthConfig string

var oauthCmd = &cobra.Command{
	Use:   "oauth",
	Short: "Provider OAuth (browser launch + verify)",
}

var oauthLaunchCmd = &cobra.Command{
	Use:   "launch <github|google|gitlab>",
	Short: "Open the provider approval, then verify the connection",
	Long: `Mints a PKCE authorize URL from the dashboard, opens your browser,
and verifies the stored credentials afterwards. The dashboard exchanges
the code itself and seals it (0600) — cyb never sees the secret.`,
	Args: cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if oauthConfig == "" {
			return fmt.Errorf("a sync config is required: --config <id> (see `cyb sync configs`)")
		}
		provider, err := client.OAuthProvider(args[0])
		if err != nil {
			return err
		}
		got, serr := c.OAuthStart(provider, oauthConfig)
		if serr != nil {
			return serr
		}
		ui.Info("opening approval in your browser…")
		if berr := openURL(got.AuthorizeURL); berr != nil {
			ui.Info("could not open a browser — visit:\n%s", got.AuthorizeURL)
		}
		var done bool
		form := ui.NewForm(huh.NewGroup(
			huh.NewConfirm().
				Title("Approve at the provider, then confirm here").
				Value(&done),
		))
		if err := form.Run(); err != nil {
			return err
		}
		if !done {
			return fmt.Errorf("aborted — no credentials stored")
		}
		g, cerr := c.SyncConfigRaw(oauthConfig)
		if cerr != nil {
			return cerr
		}
		out, terr := c.SyncTestMap(g)
		if terr != nil {
			return fmt.Errorf("approval done but verification failed: %w", terr)
		}
		if out.OK || out.Message == "" {
			ui.Success("provider connected and verified")
			return nil
		}
		return fmt.Errorf("verification said: %s", out.Message)
	},
}

var oauthStatusCmd = &cobra.Command{
	Use:   "status <config-id>",
	Short: "Probe a provider connection",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		g, err := c.SyncConfigRaw(args[0])
		if err != nil {
			return err
		}
		out, terr := c.SyncTestMap(g)
		if terr != nil {
			return terr
		}
		if out.OK || out.Message == "" {
			ui.Success("provider %s reachable", args[0])
			return nil
		}
		return fmt.Errorf("provider %s: %s", args[0], out.Message)
	},
}

func init() {
	oauthLaunchCmd.Flags().StringVar(&oauthConfig, "config", "", "sync config id to connect (required)")
	oauthCmd.AddCommand(oauthLaunchCmd, oauthStatusCmd)
	rootCmd.AddCommand(oauthCmd)
}
