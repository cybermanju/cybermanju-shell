// CyberManju OS — Auth (cli/cmd/auth.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: fmt, github.com.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"fmt"

	"charm.land/huh/v2"
	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/config"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	loginUsername string
	loginPassword string
)

var loginCmd = &cobra.Command{
	Use:   "login",
	Short: "Sign in and save the JWT",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		username, password := loginUsername, loginPassword
		if username == "" {
			form := ui.NewForm(huh.NewGroup(
				huh.NewInput().Title("Username").Value(&username),
				huh.NewInput().Title("Password").EchoMode(huh.EchoModePassword).Value(&password),
			))
			if err := form.Run(); err != nil {
				return err
			}
		}
		resp, err := c.Login(username, password)
		if err != nil {
			return err
		}
		f := profile()
		f.Server = c.BaseURL
		f.Token = resp.Token
		f = f.WithExpiry(resp.ExpiresIn)
		if err := config.Save(f); err != nil {
			return err
		}
		ui.Success("signed in as %s (%s)", resp.Username, resp.Role)
		return nil
	},
}

var logoutCmd = &cobra.Command{
	Use:   "logout",
	Short: "Revoke the session and forget the JWT",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if c.Token == "" {
			ui.Info("no saved session")
			return nil
		}
		if err := c.Logout(); err != nil {
			fmt.Fprintf(cmd.ErrOrStderr(), "server logout failed (clearing locally anyway): %v\n", err)
		}
		f := profile()
		f.Token = ""
		if err := config.Save(f); err != nil {
			return err
		}
		ui.Success("logged out")
		return nil
	},
}

func init() {
	loginCmd.Flags().StringVar(&loginUsername, "username", "", "non-interactive username")
	loginCmd.Flags().StringVar(&loginPassword, "password", "", "non-interactive password")
	rootCmd.AddCommand(loginCmd, logoutCmd)
}
