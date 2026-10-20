// CyberManju OS — Ai (cli/cmd/ai.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: encoding, fmt, strings, time, github.com.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/client"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

func mustMarshal(v any) []byte {
	raw, err := json.Marshal(v)
	if err != nil {
		return []byte(fmt.Sprintf("%v", v))
	}
	return raw
}

var (
	aiSession  string
	aiFollow   bool
	aiWait     time.Duration
	aiAnswer   string
	aiDeny     bool
	aiRemember bool
)

var aiCmd = &cobra.Command{
	Use:   "ai",
	Short: "Native AI agent: configs, prompts, jobs, approvals",
}

var aiProvidersCmd = &cobra.Command{
	Use:   "providers",
	Short: "List provider presets",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.AgentProvidersRaw()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var aiConfigsCmd = &cobra.Command{
	Use:   "configs",
	Short: "List agent configs (keys never come back, only hasKey)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("GET", "/api/agent/configs", nil)
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		cfgs, err := c.AgentConfigs()
		if err != nil {
			return err
		}
		if len(cfgs) == 0 {
			ui.Info("no agent configs — create one in the desktop Accounts/AI panel first")
			return nil
		}
		var rows [][]string
		for _, g := range cfgs {
			rows = append(rows, []string{g.ID, g.Name, g.ProviderID, g.Model, fmt.Sprint(g.HasKey)})
		}
		ui.Table([]string{"ID", "NAME", "PROVIDER", "MODEL", "HASKEY"}, rows)
		return nil
	},
}

var aiPromptCmd = &cobra.Command{
	Use:   "prompt <config-id> <text...>",
	Short: "Start an agent run (202 + optional follow)",
	Args:  cobra.MinimumNArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		prompt := strings.Join(args[1:], " ")
		job, err := c.AgentPrompt(args[0], aiSession, prompt)
		if err != nil {
			return err
		}
		ui.Success("agent job %s started (session %s)", job.JobID, job.SessionID)
		if !aiFollow {
			ui.Hint("poll with %s", ui.Chip("cyb ai status "+job.JobID))
			return nil
		}
		return followAgentJob(c, job.JobID)
	},
}

// followAgentJob polls an agent job, rendering the result as markdown.
func followAgentJob(c *client.Client, id string) error {
	ui.Hint("following job %s (max %s)", id, aiWait)
	deadline := time.Now().Add(aiWait)
	for {
		j, err := c.AgentJob(id)
		if err != nil {
			return err
		}
		if j.Terminal() {
			printAgentResult(j)
			if j.Error != nil && *j.Error != "" {
				return fmt.Errorf("job %s ended: %s", id, *j.Error)
			}
			return nil
		}
		if j.Pending != nil {
			fmt.Println(ui.Downsample(ui.Warn.Render(fmt.Sprintf("job %s parks an approval: %s — %s", id, j.Pending.Tool, j.Pending.Summary))))
			ui.Info("answer with: cyb ai approve %s [--answer TEXT|--deny]", id)
			return nil
		}
		if time.Now().After(deadline) {
			ui.Info("still %s after %s — keep polling: cyb ai status %s", j.Status, aiWait, id)
			return nil
		}
		time.Sleep(2 * time.Second)
	}
}

func printAgentResult(j client.JobSnapshot) {
	ui.KV(
		[2]string{"job", j.JobID},
		[2]string{"status", j.Status},
		[2]string{"turns", fmt.Sprintf("%d/%d", j.TurnsUsed, j.MaxTurns)},
		[2]string{"tokens", fmt.Sprintf("in %d / out %d", j.Usage.InputTokens, j.Usage.OutputTokens)},
	)
	switch r := j.Result.(type) {
	case nil:
	case string:
		fmt.Println(ui.RenderMarkdown(r))
	default:
		_ = ui.PrintJSON(mustMarshal(r))
	}
}

var aiStatusCmd = &cobra.Command{
	Use:   "status <job-id> [--follow]",
	Short: "Show (and optionally follow) an agent job",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("GET", "/api/agent/jobs/"+args[0], nil)
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		if aiFollow {
			return followAgentJob(c, args[0])
		}
		j, err := c.AgentJob(args[0])
		if err != nil {
			return err
		}
		printAgentResult(j)
		return nil
	},
}

var aiSessionsCmd = &cobra.Command{
	Use:   "sessions",
	Short: "List agent sessions (transcripts)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.AgentSessionsRaw()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var aiAbortCmd = &cobra.Command{
	Use:   "abort <job-id>",
	Short: "Abort a running agent job",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if err := c.AgentAbort(args[0]); err != nil {
			return err
		}
		ui.Success("job %s aborted", args[0])
		return nil
	},
}

var aiApproveCmd = &cobra.Command{
	Use:   "approve <job-id>",
	Short: "Approve (or deny) a parked permission ask",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		approved := !aiDeny
		if err := c.AgentApprove(args[0], aiAnswer, approved, aiRemember); err != nil {
			return err
		}
		if approved {
			ui.Success("job %s approved", args[0])
		} else {
			ui.Success("job %s denied", args[0])
		}
		return nil
	},
}

func init() {
	aiPromptCmd.Flags().StringVar(&aiSession, "session", "", "continue a session (default: new)")
	aiPromptCmd.Flags().BoolVar(&aiFollow, "follow", false, "poll until the job finishes")
	aiPromptCmd.Flags().DurationVar(&aiWait, "wait", 5*time.Minute, "max follow time")
	aiStatusCmd.Flags().BoolVar(&aiFollow, "follow", false, "poll until the job finishes")
	aiStatusCmd.Flags().DurationVar(&aiWait, "wait", 5*time.Minute, "max follow time")
	aiApproveCmd.Flags().StringVar(&aiAnswer, "answer", "", "answer to a parked question")
	aiApproveCmd.Flags().BoolVar(&aiDeny, "deny", false, "deny instead of approve")
	aiApproveCmd.Flags().BoolVar(&aiRemember, "remember", false, "persist an allow-rule (admin REST only)")
	aiCmd.AddCommand(aiProvidersCmd, aiConfigsCmd, aiPromptCmd, aiStatusCmd,
		aiSessionsCmd, aiAbortCmd, aiApproveCmd)
	rootCmd.AddCommand(aiCmd)
}
