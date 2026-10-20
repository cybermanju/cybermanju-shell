// CyberManju OS — Cron (cli/cmd/cron.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: fmt, github.com, id, description, runOnBoot.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"fmt"

	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/client"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	cronExpr        string
	cronDescription string
	cronEnabled     bool
	cronRunOnBoot   bool
	cronID          string
)

var cronCmd = &cobra.Command{
	Use:   "cron",
	Short: "Recurring .cybsh schedules (daemon runs them)",
}

var cronListCmd = &cobra.Command{
	Use:   "list",
	Short: "List schedules",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("GET", "/api/cron", nil)
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		rows, err := c.CronList()
		if err != nil {
			return err
		}
		if len(rows) == 0 {
			ui.Info("no schedules — add one: cyb cron add job.cybsh \"every 10m\"")
			return nil
		}
		var out [][]string
		for _, r := range rows {
			next := ""
			if r.NextFireAt != nil {
				next = *r.NextFireAt
			}
			out = append(out, []string{r.ID, r.Path, r.Expr, fmt.Sprint(r.Enabled), next})
		}
		ui.Table([]string{"ID", "SCRIPT", "SCHEDULE", "ENABLED", "NEXT"}, out)
		return nil
	},
}

var cronAddCmd = &cobra.Command{
	Use:   "add <file.cybsh> <schedule>",
	Short: "Schedule a script (cron expr, `every 10m`, or @daily)",
	Args:  cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.CronSave(map[string]any{
			"id": cronID, "path": args[0], "expr": args[1],
			"description": cronDescription, "enabled": cronEnabled,
			"runOnBoot": cronRunOnBoot,
		})
		if err != nil {
			return err
		}
		ui.Success("scheduled (id %s)", client.JSONField(raw, "id"))
		return nil
	},
}

var cronRmCmd = &cobra.Command{
	Use:   "rm <id>",
	Short: "Delete a schedule",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if !confirmDanger(fmt.Sprintf("delete schedule %s?", args[0])) {
			return fmt.Errorf("aborted")
		}
		if err := c.CronDelete(args[0]); err != nil {
			return err
		}
		ui.Success("schedule %s deleted", args[0])
		return nil
	},
}

var cronRunCmd = &cobra.Command{
	Use:   "run <id>",
	Short: "Execute a schedule now (synchronously)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		var out string
		if err := ui.SpinWhile("Running schedule…", func() error {
			raw, err := c.CronRun(args[0])
			if err != nil {
				return err
			}
			if flagJSON {
				return ui.PrintJSON(raw)
			}
			out = string(raw)
			return nil
		}); err != nil {
			return err
		}
		if !flagJSON {
			fmt.Println(out)
		}
		return nil
	},
}

var cronEnableCmd = &cobra.Command{
	Use:   "enable <id>",
	Short: "Enable a schedule",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if err := c.CronEnable(args[0], true); err != nil {
			return err
		}
		ui.Success("schedule %s enabled", args[0])
		return nil
	},
}

var cronDisableCmd = &cobra.Command{
	Use:   "disable <id>",
	Short: "Disable a schedule",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if err := c.CronEnable(args[0], false); err != nil {
			return err
		}
		ui.Success("schedule %s disabled", args[0])
		return nil
	},
}

var cronHistoryCmd = &cobra.Command{
	Use:   "history <id>",
	Short: "Recent runs of a schedule",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.CronHistory(args[0])
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

func init() {
	cronAddCmd.Flags().StringVar(&cronID, "id", "", "explicit schedule id (upsert)")
	cronAddCmd.Flags().StringVar(&cronDescription, "desc", "", "description")
	cronAddCmd.Flags().BoolVar(&cronEnabled, "enabled", true, "enable immediately")
	cronAddCmd.Flags().BoolVar(&cronRunOnBoot, "run-on-boot", false, "also run at daemon boot")
	cronCmd.AddCommand(cronListCmd, cronAddCmd, cronRmCmd, cronRunCmd,
		cronEnableCmd, cronDisableCmd, cronHistoryCmd)
	rootCmd.AddCommand(cronCmd)
}
