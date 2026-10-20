// CyberManju OS — Users (cli/cmd/users.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: fmt, github.com.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"fmt"

	"charm.land/huh/v2"
	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	userPassword string
	userRole     string
)

var usersCmd = &cobra.Command{
	Use:   "users",
	Short: "User management (admin-gated)",
}

var usersListCmd = &cobra.Command{
	Use:   "list",
	Short: "List users (hashes redacted)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.Users()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var usersCreateCmd = &cobra.Command{
	Use:   "create <username>",
	Short: "Create a user (admin)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		pw := userPassword
		if pw == "" {
			form := ui.NewForm(huh.NewGroup(
				huh.NewInput().Title("Password").EchoMode(huh.EchoModePassword).Value(&pw),
			))
			if err := form.Run(); err != nil {
				return err
			}
		}
		raw, err := c.UserCreate(args[0], pw, userRole)
		if err != nil {
			return err
		}
		ui.Success("user %q created", args[0])
		return ui.PrintJSON(raw)
	},
}

var usersRmCmd = &cobra.Command{
	Use:   "rm <id>",
	Short: "Delete a user (admin)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if !confirmDanger(fmt.Sprintf("delete user %s?", args[0])) {
			return fmt.Errorf("aborted")
		}
		if err := c.UserDelete(args[0]); err != nil {
			return err
		}
		ui.Success("user %s deleted", args[0])
		return nil
	},
}

var usersRoleCmd = &cobra.Command{
	Use:   "role <id> <admin|user|viewer>",
	Short: "Set a user's role (admin)",
	Args:  cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.UserSetRole(args[0], args[1])
		if err != nil {
			return err
		}
		ui.Success("role → %s", args[1])
		return ui.PrintJSON(raw)
	},
}

var permsCmd = &cobra.Command{
	Use:   "perms",
	Short: "Per-file grants (RBAC)",
}

var permsGetCmd = &cobra.Command{
	Use:   "get <file-id>",
	Short: "Show permissions for a file",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.PermGet(args[0])
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var permsGrantCmd = &cobra.Command{
	Use:   "grant <user-id> <file-id> <access>",
	Short: "Grant access (viewer|user|admin …)",
	Args:  cobra.ExactArgs(3),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.PermGrant(args[0], args[1], args[2])
		if err != nil {
			return err
		}
		ui.Success("granted %s on %s to %s", args[2], args[1], args[0])
		return ui.PrintJSON(raw)
	},
}

var permsVerifyCmd = &cobra.Command{
	Use:   "verify <user-id> <file-id> <access>",
	Short: "Verify a user may access a file",
	Args:  cobra.ExactArgs(3),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.PermVerify(args[0], args[1], args[2])
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

func init() {
	usersCreateCmd.Flags().StringVar(&userPassword, "password", "", "password (prompted otherwise)")
	usersCreateCmd.Flags().StringVar(&userRole, "role", "user", "admin|user|viewer")
	usersCmd.AddCommand(usersListCmd, usersCreateCmd, usersRmCmd, usersRoleCmd)
	permsCmd.AddCommand(permsGetCmd, permsGrantCmd, permsVerifyCmd)
	rootCmd.AddCommand(usersCmd, permsCmd)
}
