// CyberManju OS — Share (cli/cmd/share.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: github.com, fileId.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	shareExpiry uint64
	auditLimit  int
	auditEntity string
)

var shareCmd = &cobra.Command{
	Use:   "share",
	Short: "Share links (revocable, expiring)",
}

var shareCreateCmd = &cobra.Command{
	Use:   "create <file-id>",
	Short: "Mint a share link",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("POST", "/api/share-links", map[string]any{
				"fileId": args[0], "expiresInHours": shareExpiry,
			})
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		raw, err := c.ShareCreate(args[0], shareExpiry)
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var shareListCmd = &cobra.Command{
	Use:   "list",
	Short: "List share links",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.ShareList()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var shareRevokeCmd = &cobra.Command{
	Use:   "revoke <id>",
	Short: "Revoke a share link",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if err := c.ShareRevoke(args[0]); err != nil {
			return err
		}
		ui.Success("share link %s revoked", args[0])
		return nil
	},
}

var auditCmd = &cobra.Command{
	Use:   "audit",
	Short: "Audit log (bare `cyb audit` lists recent entries)",
	RunE: func(cmd *cobra.Command, args []string) error {
		return auditListCmd.RunE(cmd, args)
	},
}

var auditListCmd = &cobra.Command{
	Use:   "list",
	Short: "Recent audit entries",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.AuditList(auditLimit, auditEntity)
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

func init() {
	shareCreateCmd.Flags().Uint64Var(&shareExpiry, "expires", 0, "expiry in hours (0 = never)")
	auditCmd.PersistentFlags().IntVar(&auditLimit, "limit", 50, "max entries")
	auditCmd.PersistentFlags().StringVar(&auditEntity, "entity", "", "filter by entity type")
	shareCmd.AddCommand(shareCreateCmd, shareListCmd, shareRevokeCmd)
	auditCmd.AddCommand(auditListCmd)
	rootCmd.AddCommand(shareCmd, auditCmd)
}
