// CyberManju OS — Groups (cli/cmd/groups.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: fmt, github.com.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"fmt"

	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/client"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	collType   string
	collColor  string
	collDesc   string
	collNote   string
	acctType   string
	acctPath   string
	acctColor  string
	looseColor string
)

var collectionsCmd = &cobra.Command{
	Use:   "collections",
	Short: "Curated file sets",
}

var collectionsListCmd = &cobra.Command{
	Use:   "list",
	Short: "List collections",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.Collections()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var collectionsCreateCmd = &cobra.Command{
	Use:   "create <name>",
	Short: "Create a collection",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.CollectionCreate(args[0], collType, collColor, collDesc)
		if err != nil {
			return err
		}
		ui.Success("collection created (id %s)", client.JSONField(raw, "id"))
		return nil
	},
}

var collectionsAddCmd = &cobra.Command{
	Use:   "add <collection-id> <file-id>",
	Short: "Add a file to a collection",
	Args:  cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if _, err := c.CollectionAddItem(args[0], args[1], collNote); err != nil {
			return err
		}
		ui.Success("added %s to %s", args[1], args[0])
		return nil
	},
}

var collectionsRmCmd = &cobra.Command{
	Use:   "rm <collection-id> <file-id>",
	Short: "Remove a file from a collection",
	Args:  cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if err := c.CollectionRemoveItem(args[0], args[1]); err != nil {
			return err
		}
		ui.Success("removed %s from %s", args[1], args[0])
		return nil
	},
}

var collectionsItemsCmd = &cobra.Command{
	Use:   "items",
	Short: "List all collection items",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.CollectionItems()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var accountsCmd = &cobra.Command{
	Use:   "accounts",
	Short: "Storage accounts (vault roots)",
}

var accountsListCmd = &cobra.Command{
	Use:   "list",
	Short: "List accounts",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.Accounts()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var accountsCreateCmd = &cobra.Command{
	Use:   "create <name>",
	Short: "Create an account",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.AccountCreate(args[0], acctType, acctPath, acctColor)
		if err != nil {
			return err
		}
		ui.Success("account created (id %s)", client.JSONField(raw, "id"))
		return nil
	},
}

var accountsSwitchCmd = &cobra.Command{
	Use:   "switch <id>",
	Short: "Switch the active account",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if _, err := c.AccountSwitch(args[0]); err != nil {
			return err
		}
		ui.Success("switched to %s", args[0])
		return nil
	},
}

var accountsRmCmd = &cobra.Command{
	Use:   "rm <id>",
	Short: "Delete an account",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if !confirmDanger(fmt.Sprintf("delete account %s?", args[0])) {
			return fmt.Errorf("aborted")
		}
		if err := c.AccountDelete(args[0]); err != nil {
			return err
		}
		ui.Success("account %s deleted", args[0])
		return nil
	},
}

var looseCmd = &cobra.Command{
	Use:   "loose",
	Short: "Loose file groups",
}

var looseListCmd = &cobra.Command{
	Use:   "list",
	Short: "List loose groups",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.LooseGroups()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var looseCreateCmd = &cobra.Command{
	Use:   "create <name>",
	Short: "Create a loose group",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.LooseGroupCreate(args[0], looseColor)
		if err != nil {
			return err
		}
		ui.Success("group created (id %s)", client.JSONField(raw, "id"))
		return nil
	},
}

var looseAddCmd = &cobra.Command{
	Use:   "add <group-id> <file-id>",
	Short: "Add a file to a loose group",
	Args:  cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if _, err := c.LooseGroupAdd(args[0], args[1]); err != nil {
			return err
		}
		ui.Success("added %s to %s", args[1], args[0])
		return nil
	},
}

var facesCmd = &cobra.Command{
	Use:   "faces",
	Short: "Face groups + detection (engine always reported)",
}

var facesGroupsCmd = &cobra.Command{
	Use:   "groups",
	Short: "List face groups",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.FaceGroups()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var facesDetectCmd = &cobra.Command{
	Use:   "detect <file-id>",
	Short: "Detect faces on a file (onnx/heuristic-v2/none)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		var out []byte
		if err := ui.SpinWhile("Detecting faces…", func() error {
			raw, err := c.FacesDetect(args[0])
			if err != nil {
				return err
			}
			out = raw
			return nil
		}); err != nil {
			return err
		}
		return ui.PrintJSON(out)
	},
}

var facesBatchCmd = &cobra.Command{
	Use:   "detect-batch",
	Short: "Batch detect + adaptive recluster",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		var out []byte
		if err := ui.SpinWhile("Clustering faces…", func() error {
			raw, err := c.FacesDetectBatch()
			if err != nil {
				return err
			}
			out = raw
			return nil
		}); err != nil {
			return err
		}
		return ui.PrintJSON(out)
	},
}

func init() {
	collectionsCreateCmd.Flags().StringVar(&collType, "type", "", "collection type")
	collectionsCreateCmd.Flags().StringVar(&collColor, "color", "", "color")
	collectionsCreateCmd.Flags().StringVar(&collDesc, "desc", "", "description")
	collectionsAddCmd.Flags().StringVar(&collNote, "note", "", "note on the item")
	accountsCreateCmd.Flags().StringVar(&acctType, "type", "", "account type")
	accountsCreateCmd.Flags().StringVar(&acctPath, "path", "", "root path")
	accountsCreateCmd.Flags().StringVar(&acctColor, "color", "", "color")
	looseCreateCmd.Flags().StringVar(&looseColor, "color", "", "color")
	collectionsCmd.AddCommand(collectionsListCmd, collectionsCreateCmd, collectionsAddCmd,
		collectionsRmCmd, collectionsItemsCmd)
	accountsCmd.AddCommand(accountsListCmd, accountsCreateCmd, accountsSwitchCmd, accountsRmCmd)
	looseCmd.AddCommand(looseListCmd, looseCreateCmd, looseAddCmd)
	facesCmd.AddCommand(facesGroupsCmd, facesDetectCmd, facesBatchCmd)
	rootCmd.AddCommand(collectionsCmd, accountsCmd, looseCmd, facesCmd)
}
