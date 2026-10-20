// CyberManju OS — Sync extra (cli/cmd/sync_extra.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: encoding, fmt, os, github.com, backendType, token.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"encoding/base64"
	"fmt"
	"os"

	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	syncRestoreFile   string
	syncRestoreRemote string
	syncRestoreDest   string
	repoName          string
	repoPrivate       bool
	repoDesc          string
	repoBranch        string
	repoBasePath      string
	repoToken         string
)

var syncRunsCmd = &cobra.Command{
	Use:   "runs",
	Short: "Finished sync run history",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.SyncRuns()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var syncRestoreCmd = &cobra.Command{
	Use:   "restore <config-id>",
	Short: "Restore a file from a provider",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		var out []byte
		if err := ui.SpinWhile("Restoring…", func() error {
			raw, err := c.SyncRestore(args[0], syncRestoreFile, syncRestoreRemote, syncRestoreDest)
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

var syncRemoteRmCmd = &cobra.Command{
	Use:   "remote-rm <config-id> <remote-path>",
	Short: "Delete a remote object",
	Args:  cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if !confirmDanger(fmt.Sprintf("delete remote %s on %s?", args[1], args[0])) {
			return fmt.Errorf("aborted")
		}
		if err := c.SyncRemoteDelete(args[0], args[1]); err != nil {
			return err
		}
		ui.Success("remote object deleted")
		return nil
	},
}

var syncRepoCmd = &cobra.Command{
	Use:   "create-repo <backend> <config-id>",
	Short: "Create the remote repo (github|gitlab|google)",
	Args:  cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.SyncCreateRepo(map[string]any{
			"backendType": args[0], "configId": args[1],
			"token": repoToken, "name": repoName,
			"private": repoPrivate, "description": repoDesc,
			"branch": repoBranch, "basePath": repoBasePath,
		})
		if err != nil {
			return err
		}
		ui.Success("remote repo ready")
		return ui.PrintJSON(raw)
	},
}

var syncUploadCmd = &cobra.Command{
	Use:   "upload <config-id> <local-file> <remote-path>",
	Short: "Upload a local file to a provider",
	Args:  cobra.ExactArgs(3),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		cfgMap, err := c.SyncConfigRaw(args[0])
		if err != nil {
			return err
		}
		data, err := os.ReadFile(args[1])
		if err != nil {
			return err
		}
		var out []byte
		if err := ui.SpinWhile("Uploading…", func() error {
			raw, err := c.SyncUpload(cfgMap, args[2], base64.StdEncoding.EncodeToString(data))
			if err != nil {
				return err
			}
			out = raw
			return nil
		}); err != nil {
			return err
		}
		ui.Success("uploaded %s → %s", args[1], args[2])
		return ui.PrintJSON(out)
	},
}

func init() {
	syncRestoreCmd.Flags().StringVar(&syncRestoreFile, "file", "", "vault file id")
	syncRestoreCmd.Flags().StringVar(&syncRestoreRemote, "remote", "", "remote path")
	syncRestoreCmd.Flags().StringVar(&syncRestoreDest, "dest", "", "local destination path")
	syncRepoCmd.Flags().StringVar(&repoName, "name", "", "repo name")
	syncRepoCmd.Flags().BoolVar(&repoPrivate, "private", true, "private repo")
	syncRepoCmd.Flags().StringVar(&repoDesc, "desc", "", "repo description")
	syncRepoCmd.Flags().StringVar(&repoBranch, "branch", "", "branch")
	syncRepoCmd.Flags().StringVar(&repoBasePath, "base-path", "", "base path")
	syncRepoCmd.Flags().StringVar(&repoToken, "token", "", "provider token (else stored/OAuth)")
	syncCmd.AddCommand(syncRunsCmd, syncRestoreCmd, syncRemoteRmCmd, syncRepoCmd, syncUploadCmd)
}
