// CyberManju OS — Ai extra (cli/cmd/ai_extra.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: encoding, fmt, os, github.com, configId.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"encoding/json"
	"fmt"
	"os"

	"charm.land/huh/v2"
	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/client"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	aiNewTitle   string
	aiImportFile string
	aiMemTopK    int
	aiMemText    string
	aiMemSession string
	aiKeyValue   string
)

var aiSessionGetCmd = &cobra.Command{
	Use:   "session <id>",
	Short: "Show a transcript",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.AgentSessionGet(args[0])
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var aiSessionNewCmd = &cobra.Command{
	Use:   "new <config-id>",
	Short: "Open a session (transcript)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("POST", "/api/agent/sessions", map[string]any{
				"configId": args[0], "title": aiNewTitle,
			})
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		raw, err := c.AgentSessionCreate(args[0], aiNewTitle)
		if err != nil {
			return err
		}
		ui.Success("session opened (id %s)", client.JSONField(raw, "id"))
		return nil
	},
}

var aiSessionRmCmd = &cobra.Command{
	Use:   "forget <id>",
	Short: "Delete a transcript",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if !confirmDanger(fmt.Sprintf("delete transcript %s?", args[0])) {
			return fmt.Errorf("aborted")
		}
		if err := c.AgentSessionDelete(args[0]); err != nil {
			return err
		}
		ui.Success("transcript %s deleted", args[0])
		return nil
	},
}

var aiSessionImportCmd = &cobra.Command{
	Use:   "import <file.json>",
	Short: "Import a transcript (re-keyed server-side)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := os.ReadFile(args[0])
		if err != nil {
			return err
		}
		var session any
		if jerr := json.Unmarshal(raw, &session); jerr != nil {
			return jerr
		}
		out, err := c.Raw("POST", "/api/agent/sessions/import", map[string]any{"session": session})
		if err != nil {
			return err
		}
		ui.Success("transcript imported")
		return ui.PrintJSON(out)
	},
}

var aiInitCmd = &cobra.Command{
	Use:   "init <config-id>",
	Short: "Init handshake for a config (detached job)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.AgentInit(args[0])
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var aiCompactCmd = &cobra.Command{
	Use:   "compact <session-id> <config-id>",
	Short: "Summarize into a fresh session (old kept)",
	Args:  cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		var out []byte
		if err := ui.SpinWhile("Compacting…", func() error {
			raw, err := c.AgentCompact(args[0], args[1])
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

var aiKeyCmd = &cobra.Command{
	Use:   "key <config-id>",
	Short: "Seal a provider API key (never echoed back)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		key := aiKeyValue
		if key == "" {
			form := ui.NewForm(huh.NewGroup(
				huh.NewInput().Title("Provider API key").EchoMode(huh.EchoModePassword).Value(&key),
			))
			if err := form.Run(); err != nil {
				return err
			}
		}
		if err := c.AgentKeySeal(args[0], key); err != nil {
			return err
		}
		ui.Success("key sealed for %s", args[0])
		return nil
	},
}

var aiModelsCmd = &cobra.Command{
	Use:   "models <config-id>",
	Short: "Refresh the model list from the provider",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		var out []byte
		if err := ui.SpinWhile("Refreshing models…", func() error {
			raw, err := c.AgentModels(args[0])
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

var aiMCPToolsCmd = &cobra.Command{
	Use:   "mcp <config-id>",
	Short: "Discover MCP tools for a config",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.AgentMCPTools(args[0])
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var aiMCPAddCmd = &cobra.Command{
	Use:   "mcp-add <config-id> <name> <server>",
	Short: "Attach an MCP server (admin)",
	Args:  cobra.ExactArgs(3),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.AgentMCPAdd(args[0], args[1], args[2])
		if err != nil {
			return err
		}
		ui.Success("MCP server %q attached", args[1])
		return ui.PrintJSON(raw)
	},
}

var aiMCPRemoveCmd = &cobra.Command{
	Use:   "mcp-rm <config-id> <name>",
	Short: "Detach an MCP server",
	Args:  cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if err := c.AgentMCPRemove(args[0], args[1]); err != nil {
			return err
		}
		ui.Success("MCP server %q detached", args[1])
		return nil
	},
}

var aiMemCmd = &cobra.Command{
	Use:   "mem",
	Short: "Semantic memory (transcripts + recall)",
}

var aiMemListCmd = &cobra.Command{
	Use:   "list",
	Short: "List semantic memories",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("GET", "/api/agent/memories", nil)
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		rows, err := c.AgentMemories(aiMemConfig)
		if err != nil {
			return err
		}
		if len(rows) == 0 {
			ui.Info("no memories")
			return nil
		}
		var out [][]string
		for _, m := range rows {
			out = append(out, []string{m.ID, m.ConfigID, truncate(m.Text, 64)})
		}
		ui.Table([]string{"ID", "CONFIG", "TEXT"}, out)
		return nil
	},
}

var aiMemConfig string

var aiMemAddCmd = &cobra.Command{
	Use:   "add <text...>",
	Short: "Store a semantic memory (permission-gated)",
	Args:  cobra.MinimumNArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		text := aiMemText
		if text == "" {
			text = joinArgs(args)
		}
		raw, err := c.AgentMemoryStore(aiMemConfig, aiMemSession, text)
		if err != nil {
			return err
		}
		ui.Success("memory stored")
		return ui.PrintJSON(raw)
	},
}

var aiMemRecallCmd = &cobra.Command{
	Use:   "recall <query...>",
	Short: "Recall memories by query",
	Args:  cobra.MinimumNArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.AgentMemoryRecall(aiMemConfig, joinArgs(args), aiMemTopK)
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var aiMemRmCmd = &cobra.Command{
	Use:   "rm <id>",
	Short: "Delete a memory",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if err := c.AgentMemoryDelete(args[0]); err != nil {
			return err
		}
		ui.Success("memory %s deleted", args[0])
		return nil
	},
}

var aiMemExportCmd = &cobra.Command{
	Use:   "export",
	Short: "Export memories as markdown",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.AgentMemoryExport(aiMemConfig)
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

func init() {
	aiSessionNewCmd.Flags().StringVar(&aiNewTitle, "title", "", "session title")
	aiMemListCmd.Flags().StringVar(&aiMemConfig, "config", "", "filter by config")
	aiMemAddCmd.Flags().StringVar(&aiMemConfig, "config", "", "config id (required)")
	aiMemAddCmd.Flags().StringVar(&aiMemSession, "session", "", "session id")
	aiMemAddCmd.Flags().StringVar(&aiMemText, "text", "", "memory text (else positional args)")
	aiMemRecallCmd.Flags().StringVar(&aiMemConfig, "config", "", "config id")
	aiMemRecallCmd.Flags().IntVar(&aiMemTopK, "top", 5, "top-K hits")
	aiMemExportCmd.Flags().StringVar(&aiMemConfig, "config", "", "filter by config")
	aiKeyCmd.Flags().StringVar(&aiKeyValue, "key", "", "API key (prompted securely otherwise)")
	aiMemCmd.AddCommand(aiMemListCmd, aiMemAddCmd, aiMemRecallCmd, aiMemRmCmd, aiMemExportCmd)
	aiCmd.AddCommand(aiSessionGetCmd, aiSessionNewCmd, aiSessionRmCmd,
		aiSessionImportCmd, aiInitCmd, aiCompactCmd, aiKeyCmd, aiModelsCmd,
		aiMCPToolsCmd, aiMCPAddCmd, aiMCPRemoveCmd, aiMemCmd)
}
