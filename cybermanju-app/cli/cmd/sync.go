// CyberManju OS — Sync (cli/cmd/sync.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: fmt, strings, time, github.com, local, id.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"fmt"
	"strings"
	"time"

	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/client"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/util"
)

var (
	syncFiles  []string
	syncWait   bool
	syncPrefix string
)

var syncCmd = &cobra.Command{
	Use:   "sync",
	Short: "Provider sync: configs, runs, moves between providers",
}

var syncConfigsCmd = &cobra.Command{
	Use:   "configs",
	Short: "List sync configs (providers)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("GET", "/api/sync/configs", nil)
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		cfgs, err := c.SyncConfigs()
		if err != nil {
			return err
		}
		if len(cfgs) == 0 {
			ui.Info("no providers configured — see `cyb sync create --help`")
			return nil
		}
		var rows [][]string
		for _, g := range cfgs {
			rows = append(rows, []string{g.ID, g.BackendType, nameOf(g.Name), fmt.Sprint(g.Enabled)})
		}
		ui.Table([]string{"ID", "BACKEND", "NAME", "ENABLED"}, rows)
		return nil
	},
}

func nameOf(s *string) string {
	if s == nil {
		return ""
	}
	return *s
}

var (
	syncCreateBackend string
	syncCreateName    string
	syncCreateBase    string
	syncCreateRepo    string
	syncCreateBranch  string
	syncCreateFolder  string
	syncCreateToken   string
)

var syncCreateCmd = &cobra.Command{
	Use:   "create --backend <local|github|gitlab|googleDrive>",
	Short: "Add a provider (token optional — use `cyb oauth launch` after)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		backend := strings.TrimSpace(syncCreateBackend)
		switch backend {
		case "local", "github", "gitlab", "googleDrive":
		default:
			return fmt.Errorf("unsupported: backend must be local|github|gitlab|googleDrive (got %q)", backend)
		}
		cfg := map[string]any{
			"id": "", "backendType": backend, "enabled": true,
			"name": syncCreateName, "basePath": syncCreateBase,
			"repoName": syncCreateRepo, "branch": syncCreateBranch,
			"folderId": syncCreateFolder, "token": syncCreateToken,
			"autoSync": false, "compressBeforeUpload": true,
			"createPreviews": false, "deleteRawAfterSync": false,
			"maxConcurrentUploads": 4, "encryptBeforeUpload": true,
		}
		// Absent beats "": several handlers treat an explicit empty string
		// as a real value (e.g. paths), while missing keys take defaults.
		// "id" stays: the server mints one when it arrives empty.
		for k, v := range cfg {
			if k == "id" {
				continue
			}
			if s, ok := v.(string); ok && s == "" {
				delete(cfg, k)
			}
		}
		out, err := c.SyncCreate(cfg)
		if err != nil {
			return err
		}
		ui.Success("provider %s (%s) → %s", nameOf(out.Name), out.BackendType, out.ID)
		if syncCreateToken == "" && backend != "local" {
			ui.Info("no token stored — run: cyb oauth launch <provider> --config %s", out.ID)
		}
		return nil
	},
}

var syncRmCmd = &cobra.Command{
	Use:   "rm <config-id>",
	Short: "Delete a sync config (admin)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if !confirmDanger(fmt.Sprintf("delete sync config %s?", args[0])) {
			return fmt.Errorf("aborted")
		}
		if err := c.SyncDelete(args[0]); err != nil {
			return err
		}
		ui.Success("deleted %s", args[0])
		return nil
	},
}

var syncStartCmd = &cobra.Command{
	Use:   "start <config-id>",
	Short: "Start an async sync job (202 + poll)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		resp, err := c.SyncStart(args[0], syncFiles)
		if err != nil {
			return err
		}
		ui.Success("sync job %s started (202 accepted)", resp.JobID)
		if !syncWait {
			ui.Hint("poll with %s", ui.Chip("cyb sync job "+resp.JobID))
			return nil
		}
		return waitSyncJob(c, resp.JobID)
	},
}

// waitSyncJob polls a sync job until it leaves an active state.
func waitSyncJob(c *client.Client, id string) error {
	for i := 0; i < 300; i++ {
		j, err := c.SyncJob(id)
		if err != nil {
			return err
		}
		switch strings.ToLower(j.Status) {
		case "completed", "done":
			ui.Success("sync job %s completed", id)
			return nil
		case "error", "failed", "cancelled":
			msg := j.Status
			if j.Error != nil && *j.Error != "" {
				msg = *j.Error
			}
			return fmt.Errorf("sync job %s ended: %s", id, msg)
		}
		if i%15 == 0 {
			ui.Hint("job %s: %s…", id, j.Status)
		}
		time.Sleep(2 * time.Second)
	}
	return fmt.Errorf("timed out waiting for job %s (10 min)", id)
}

var syncJobCmd = &cobra.Command{
	Use:   "job <job-id>",
	Short: "Show a sync job",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("GET", "/api/sync/jobs/"+args[0], nil)
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		j, err := c.SyncJob(args[0])
		if err != nil {
			return err
		}
		errStr := ""
		if j.Error != nil {
			errStr = *j.Error
		}
		ui.KV(
			[2]string{"job", j.JobID},
			[2]string{"config", j.ConfigID},
			[2]string{"status", j.Status},
			[2]string{"error", errStr},
		)
		return nil
	},
}

var syncStatusCmd = &cobra.Command{
	Use:   "status",
	Short: "Overall sync status",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.SyncStatusRaw()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var syncProgressCmd = &cobra.Command{
	Use:   "progress",
	Short: "Live sync progress",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.SyncProgressRaw()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var syncCancelCmd = &cobra.Command{
	Use:   "cancel",
	Short: "Cancel the running sync",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if err := c.SyncCancel(); err != nil {
			return err
		}
		ui.Success("sync cancelled")
		return nil
	},
}

var syncMoveCmd = &cobra.Command{
	Use:   "move <file-id> <from-config> <to-config>",
	Short: "Single-copy relocate a file between providers",
	Long: `Download from A, upload to B as-is, verify BLAKE3, delete from A.
Same provider twice is a no-op; striped files refuse with unsupported:.`,
	Args: cobra.ExactArgs(3),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		resp, err := c.SyncMove(args[0], args[1], args[2])
		if err != nil {
			return err
		}
		if resp.Noop {
			ui.Info("noop — already on %s", args[2])
			return nil
		}
		ui.Success("moved %s (%s) %s → %s", resp.FileID, util.FormatBytes(resp.Bytes), resp.FromConfigID, resp.ToConfigID)
		return nil
	},
}

var syncTestCmd = &cobra.Command{
	Use:   "test <config-id>",
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
		} else {
			ui.Failure("provider %s: %s", args[0], out.Message)
			return fmt.Errorf("probe failed")
		}
		return nil
	},
}

var syncRemoteCmd = &cobra.Command{
	Use:   "remote <config-id>",
	Short: "List remote files on a provider",
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
		raw, rerr := c.SyncRemoteFilesMap(g, syncPrefix)
		if rerr != nil {
			return rerr
		}
		return ui.PrintJSON(raw)
	},
}

var syncUsageCmd = &cobra.Command{
	Use:   "usage <config-id>",
	Short: "Provider quota usage",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.SyncUsage(args[0])
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

func init() {
	syncStartCmd.Flags().StringSliceVar(&syncFiles, "files", nil, "limit to file ids (repeatable/comma-separated)")
	syncStartCmd.Flags().BoolVar(&syncWait, "wait", false, "poll until the job finishes")
	syncRemoteCmd.Flags().StringVar(&syncPrefix, "prefix", "", "remote path prefix")
	syncCreateCmd.Flags().StringVar(&syncCreateBackend, "backend", "", "local|github|gitlab|googleDrive (required)")
	syncCreateCmd.Flags().StringVar(&syncCreateName, "name", "", "display name")
	syncCreateCmd.Flags().StringVar(&syncCreateBase, "base-path", "", "local base path")
	syncCreateCmd.Flags().StringVar(&syncCreateRepo, "repo", "", "owner/repo (github) or project id (gitlab)")
	syncCreateCmd.Flags().StringVar(&syncCreateBranch, "branch", "", "branch (default main)")
	syncCreateCmd.Flags().StringVar(&syncCreateFolder, "folder", "", "drive folder id")
	syncCreateCmd.Flags().StringVar(&syncCreateToken, "token", "", "PAT (else use cyb oauth launch)")
	_ = syncCreateCmd.MarkFlagRequired("backend")
	syncCmd.AddCommand(syncConfigsCmd, syncCreateCmd, syncRmCmd, syncStartCmd,
		syncJobCmd, syncStatusCmd, syncProgressCmd, syncCancelCmd,
		syncMoveCmd, syncTestCmd, syncRemoteCmd, syncUsageCmd)
	rootCmd.AddCommand(syncCmd)
}
