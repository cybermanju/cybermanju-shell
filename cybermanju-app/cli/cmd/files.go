// CyberManju OS — Files (cli/cmd/files.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: fmt, github.com.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"fmt"

	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/util"
)

var (
	filesAll    bool
	filesParent string
)

var filesCmd = &cobra.Command{
	Use:   "files",
	Short: "Vault files: list, inspect, organize",
}

var filesLsCmd = &cobra.Command{
	Use:   "ls [parent-id]",
	Short: "List files (default: vault root)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		parent := filesParent
		if len(args) > 0 {
			parent = args[0]
		}
		if flagJSON {
			raw, err := c.Raw("GET", "/api/files", nil)
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		nodes, err := c.Files()
		if err != nil {
			return err
		}
		var rows [][]string
		for _, n := range nodes {
			pid := ""
			if n.ParentID != nil {
				pid = *n.ParentID
			}
			if !filesAll && pid != parent {
				continue
			}
			enc := ""
			if n.Encrypted {
				enc = ui.Flag("encrypted")
			}
			rows = append(rows, []string{n.ID, n.Name, n.FileType, util.FormatBytes(n.SizeBytes), enc})
		}
		if len(rows) == 0 {
			ui.Info("no files here")
			return nil
		}
		ui.Table([]string{"ID", "NAME", "TYPE", "SIZE", "FLAGS"}, rows)
		return nil
	},
}

// confirmDanger asks for an explicit yes (or --yes) before destruction.

var filesInfoCmd = &cobra.Command{
	Use:   "info <file-id>",
	Short: "Show a file row",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("GET", "/api/files/"+args[0], nil)
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		n, err := c.File(args[0])
		if err != nil {
			return err
		}
		parent := ""
		if n.ParentID != nil {
			parent = *n.ParentID
		}
		ui.KV(
			[2]string{"id", n.ID},
			[2]string{"name", n.Name},
			[2]string{"type", n.FileType},
			[2]string{"parent", parent},
			[2]string{"size", util.FormatBytes(n.SizeBytes)},
			[2]string{"encrypted", fmt.Sprint(n.Encrypted)},
			[2]string{"tags", fmt.Sprint(n.Tags)},
			[2]string{"modified", n.ModifiedAt},
		)
		return nil
	},
}

var filesMkdirCmd = &cobra.Command{
	Use:   "mkdir <name>",
	Short: "Create a folder",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		n, err := c.Mkdir(args[0], filesParent)
		if err != nil {
			return err
		}
		ui.Success("folder %q → %s", n.Name, n.ID)
		return nil
	},
}

var filesRenameCmd = &cobra.Command{
	Use:   "rename <file-id> <new-name>",
	Short: "Rename a file",
	Args:  cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if err := c.Rename(args[0], args[1]); err != nil {
			return err
		}
		ui.Success("renamed %s → %q", args[0], args[1])
		return nil
	},
}

var filesTrashCmd = &cobra.Command{
	Use:   "trash <file-id>",
	Short: "Move a file to trash",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if err := c.TrashFile(args[0]); err != nil {
			return err
		}
		ui.Success("trashed %s", args[0])
		return nil
	},
}

var filesTrashLsCmd = &cobra.Command{
	Use:   "trash-ls",
	Short: "List trashed files",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("GET", "/api/trash", nil)
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		items, err := c.TrashList()
		if err != nil {
			return err
		}
		if len(items) == 0 {
			ui.Info("trash is empty")
			return nil
		}
		var rows [][]string
		for _, it := range items {
			rows = append(rows, []string{it.ID, it.Name})
		}
		ui.Table([]string{"ID", "NAME"}, rows)
		return nil
	},
}

var filesRestoreCmd = &cobra.Command{
	Use:   "restore <file-id>",
	Short: "Restore a file from trash",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if err := c.TrashRestore(args[0]); err != nil {
			return err
		}
		ui.Success("restored %s", args[0])
		return nil
	},
}

var filesEmptyTrashCmd = &cobra.Command{
	Use:   "empty-trash",
	Short: "Permanently empty the trash",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if !confirmDanger("permanently delete everything in trash?") {
			return fmt.Errorf("aborted")
		}
		if err := c.TrashEmpty(); err != nil {
			return err
		}
		ui.Success("trash emptied")
		return nil
	},
}

func init() {
	filesLsCmd.Flags().BoolVar(&filesAll, "all", false, "list the whole vault, not just one level")
	filesLsCmd.Flags().StringVar(&filesParent, "parent", "", "parent id (default: root)")
	filesMkdirCmd.Flags().StringVar(&filesParent, "parent", "", "parent id (default: root)")
	filesCmd.AddCommand(filesLsCmd, filesInfoCmd, filesMkdirCmd, filesRenameCmd,
		filesTrashCmd, filesTrashLsCmd, filesRestoreCmd, filesEmptyTrashCmd)
	rootCmd.AddCommand(filesCmd)
}
