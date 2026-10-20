// CyberManju OS — Disks (cli/cmd/disks.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: fmt, os, github.com.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"fmt"
	"os"

	"charm.land/huh/v2"
	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/util"
)

var (
	diskPassphrase     string
	diskPassphraseFile string
	diskContainerPath  string
)

// passphrase resolves the disk passphrase: flag > file > interactive prompt.
// It is never logged.
func passphrase(cmd *cobra.Command) (string, error) {
	if diskPassphrase != "" {
		return diskPassphrase, nil
	}
	if diskPassphraseFile != "" {
		raw, err := os.ReadFile(diskPassphraseFile)
		if err != nil {
			return "", err
		}
		return string(raw), nil
	}
	var pw string
	form := ui.NewForm(huh.NewGroup(
		huh.NewInput().Title("Disk passphrase").EchoMode(huh.EchoModePassword).Value(&pw),
	))
	if err := form.Run(); err != nil {
		return "", err
	}
	if pw == "" {
		return "", fmt.Errorf("empty passphrase")
	}
	return pw, nil
}

func passphraseFlags(cmd *cobra.Command) {
	cmd.Flags().StringVar(&diskPassphrase, "passphrase", "", "disk passphrase (env CYB_PASSPHRASE is safer than argv)")
	cmd.Flags().StringVar(&diskPassphraseFile, "passphrase-file", "", "read the passphrase from a file")
}

var diskCmd = &cobra.Command{
	Use:   "disk",
	Short: "Encrypted vault disks (create/attach/keys) + volume",
}

var diskListCmd = &cobra.Command{
	Use:   "list",
	Short: "List disks",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("GET", "/api/disk/list", nil)
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		disks, err := c.Disks()
		if err != nil {
			return err
		}
		if len(disks) == 0 {
			ui.Info("no disks — create one: cyb disk create <config> 10G")
			return nil
		}
		var rows [][]string
		for _, d := range disks {
			keys := ""
			if d.HoldsKeys {
				keys = ui.Flag("key-holder")
			}
			rows = append(rows, []string{d.ID, d.Provider, d.State, d.Health,
				util.FormatBytes(d.UsedBytes) + "/" + util.FormatBytes(d.CapacityBytes), keys})
		}
		ui.Table([]string{"ID", "PROVIDER", "STATE", "HEALTH", "USED/CAP", "FLAGS"}, rows)
		return nil
	},
}

var diskCreateCmd = &cobra.Command{
	Use:   "create <config-id> <size>",
	Short: "Create an encrypted disk (Argon2id + ChaCha20Poly1305)",
	Long: `Size like 512M, 10G. The passphrase seals the container and is never
stored — lose it and the disk is cryptographically gone.`,
	Args: cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		size, err := util.ParseSize(args[1])
		if err != nil {
			return err
		}
		pw, err := passphrase(cmd)
		if err != nil {
			return err
		}
		disk, err := c.DiskCreate(args[0], size, pw, diskContainerPath)
		if err != nil {
			return err
		}
		ui.Success("disk %s (%s on %s) attached", disk.ID, util.FormatBytes(disk.CapacityBytes), disk.Provider)
		return nil
	},
}

var diskAttachCmd = &cobra.Command{
	Use:   "attach <disk-id>",
	Short: "Attach a disk (unlock with passphrase)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		pw, err := passphrase(cmd)
		if err != nil {
			return err
		}
		if err := c.DiskAttach(args[0], pw); err != nil {
			return err
		}
		ui.Success("disk %s attached", args[0])
		return nil
	},
}

var diskDetachCmd = &cobra.Command{
	Use:   "detach <disk-id>",
	Short: "Detach a disk (drops the in-memory key)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if err := c.DiskDetach(args[0]); err != nil {
			return err
		}
		ui.Success("disk %s detached", args[0])
		return nil
	},
}

var diskResizeCmd = &cobra.Command{
	Use:   "resize <disk-id> <size>",
	Short: "Resize an attached disk (grow; shrink refuses past used slots)",
	Args:  cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		size, err := util.ParseSize(args[1])
		if err != nil {
			return err
		}
		if err := c.DiskResize(args[0], size); err != nil {
			return err
		}
		ui.Success("disk %s resized to %s", args[0], util.FormatBytes(size))
		return nil
	},
}

var diskCheckCmd = &cobra.Command{
	Use:   "check <disk-id>",
	Short: "Verify container vs catalog (orphans, locks, checkpoints)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("POST", "/api/disk/check", map[string]any{"id": args[0]})
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		rep, err := c.DiskCheck(args[0])
		if err != nil {
			return err
		}
		ui.KV(
			[2]string{"disk", rep.DiskID},
			[2]string{"ok", fmt.Sprint(rep.OK)},
			[2]string{"blocks", fmt.Sprint(rep.Blocks)},
			[2]string{"containerBlocks", fmt.Sprint(rep.ContainerBlocks)},
			[2]string{"orphans", fmt.Sprint(rep.Orphans)},
			[2]string{"pendingCheckpoint", fmt.Sprint(rep.PendingCheckpoint)},
			[2]string{"locked", fmt.Sprint(rep.Locked)},
		)
		for _, p := range rep.Problems {
			ui.Failure("%s", p)
		}
		if !rep.OK {
			return fmt.Errorf("disk check failed")
		}
		return nil
	},
}

var diskDfCmd = &cobra.Command{
	Use:   "df",
	Short: "Merged volume usage (attached disks only)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("GET", "/api/volume/df", nil)
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		df, err := c.VolumeDF()
		if err != nil {
			return err
		}
		ui.KV(
			[2]string{"total", util.FormatBytes(df.TotalBytes)},
			[2]string{"used", util.FormatBytes(df.UsedBytes)},
			[2]string{"free", util.FormatBytes(df.FreeBytes)},
			[2]string{"disks", fmt.Sprint(df.DiskCount)},
		)
		var rows [][]string
		for _, d := range df.Disks {
			rows = append(rows, []string{d.ID, d.Provider, util.FormatBytes(d.Used) + "/" + util.FormatBytes(d.Size), d.Health})
		}
		if len(rows) > 0 {
			ui.Table([]string{"ID", "PROVIDER", "USED/SIZE", "HEALTH"}, rows)
		}
		return nil
	},
}

var diskKeyHolderCmd = &cobra.Command{
	Use:   "key-holder <disk-id>",
	Short: "Mark this disk the key holder (single-holder policy)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if err := c.DiskKeyHolder(args[0]); err != nil {
			return err
		}
		ui.Success("disk %s is now the key holder", args[0])
		return nil
	},
}

var diskDestroyCmd = &cobra.Command{
	Use:   "destroy <disk-id>",
	Short: "Destroy a disk (catalog + container; remote payload left for GC)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if !confirmDanger(fmt.Sprintf("destroy disk %s? The passphrase cannot recover it.", args[0])) {
			return fmt.Errorf("aborted")
		}
		if err := c.DiskDestroy(args[0]); err != nil {
			return err
		}
		ui.Success("disk %s destroyed", args[0])
		return nil
	},
}

func init() {
	passphraseFlags(diskCreateCmd)
	passphraseFlags(diskAttachCmd)
	diskCreateCmd.Flags().StringVar(&diskContainerPath, "container-path", "", "custom .cybermanju path (default: data dir)")
	diskCmd.AddCommand(diskListCmd, diskCreateCmd, diskAttachCmd, diskDetachCmd,
		diskResizeCmd, diskCheckCmd, diskDfCmd, diskKeyHolderCmd, diskDestroyCmd)
	rootCmd.AddCommand(diskCmd)
}
