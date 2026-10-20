// CyberManju OS — Durability (cli/cmd/durability.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: fmt, github.com.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"fmt"

	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	gcDryRun   bool
	gcGrace    uint64
	leaseScope string
	leaseTTL   uint64
)

var repairCmd = &cobra.Command{
	Use:   "repair",
	Short: "Durability: verify, repair, rebuild, garbage-collect",
}

var repairStatusCmd = &cobra.Command{
	Use:   "status",
	Short: "Repair queue status",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.RepairStatus()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var repairTasksCmd = &cobra.Command{
	Use:   "tasks",
	Short: "Repair task table",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.RepairTasks()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var repairHealthCmd = &cobra.Command{
	Use:   "health",
	Short: "Volume health rollup",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.RepairHealth()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var repairRunCmd = &cobra.Command{
	Use:   "run",
	Short: "Drain the finding queue (off-thread repair)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		var out []byte
		if err := ui.SpinWhile("Repairing…", func() error {
			raw, err := c.RepairRun(nil)
			if err != nil {
				return err
			}
			out = raw
			return nil
		}); err != nil {
			return err
		}
		if flagJSON {
			return ui.PrintJSON(out)
		}
		ui.Success("repair pass started")
		return ui.PrintJSON(out)
	},
}

var repairRebuildCmd = &cobra.Command{
	Use:   "rebuild",
	Short: "Rebuild the catalog from providers",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if !confirmDanger("rebuild the block catalog from providers?") {
			return fmt.Errorf("aborted")
		}
		raw, err := c.RepairRebuild()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var repairGCCmd = &cobra.Command{
	Use:   "gc",
	Short: "Sweep unreferenced chunks (--dry-run to preview)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if !gcDryRun && !confirmDanger("garbage-collect unreferenced chunks?") {
			return fmt.Errorf("aborted")
		}
		var grace *uint64
		if cmd.Flags().Changed("grace") {
			grace = &gcGrace
		}
		var out []byte
		if err := ui.SpinWhile("Collecting…", func() error {
			raw, err := c.RepairGC(gcDryRun, grace)
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

var scrubCmd = &cobra.Command{
	Use:   "scrub",
	Short: "Verification passes over stored chunks",
}

var scrubRunCmd = &cobra.Command{
	Use:   "run",
	Short: "Start one verification pass (findings queue for repair)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		var out []byte
		if err := ui.SpinWhile("Scrubbing…", func() error {
			raw, err := c.ScrubRun()
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

var scrubRunsCmd = &cobra.Command{
	Use:   "runs",
	Short: "Recent scrub passes",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.ScrubRuns()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var leaseCmd = &cobra.Command{
	Use:   "lease",
	Short: "Single-writer leases (acquire/release/status)",
}

var leaseAcquireCmd = &cobra.Command{
	Use:   "acquire <holder>",
	Short: "Acquire the write lease",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.LeaseAcquire(args[0], leaseScope, leaseTTL)
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var leaseReleaseCmd = &cobra.Command{
	Use:   "release <holder>",
	Short: "Release the write lease",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.LeaseRelease(args[0], leaseScope)
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var leaseStatusCmd = &cobra.Command{
	Use:   "status",
	Short: "Lease status (optional --scope)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.LeaseStatus(leaseScope)
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

func init() {
	repairGCCmd.Flags().BoolVar(&gcDryRun, "dry-run", false, "preview without sweeping")
	repairGCCmd.Flags().Uint64Var(&gcGrace, "grace", 0, "grace period seconds")
	leaseAcquireCmd.Flags().StringVar(&leaseScope, "scope", "volume", "lease scope")
	leaseAcquireCmd.Flags().Uint64Var(&leaseTTL, "ttl", 60, "lease TTL seconds")
	leaseReleaseCmd.Flags().StringVar(&leaseScope, "scope", "volume", "lease scope")
	leaseStatusCmd.Flags().StringVar(&leaseScope, "scope", "", "lease scope (default: all)")
	repairCmd.AddCommand(repairStatusCmd, repairTasksCmd, repairHealthCmd,
		repairRunCmd, repairRebuildCmd, repairGCCmd)
	scrubCmd.AddCommand(scrubRunCmd, scrubRunsCmd)
	leaseCmd.AddCommand(leaseAcquireCmd, leaseReleaseCmd, leaseStatusCmd)
	rootCmd.AddCommand(repairCmd, scrubCmd, leaseCmd)
}
