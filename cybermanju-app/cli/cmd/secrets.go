// CyberManju OS — Secrets (cli/cmd/secrets.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: fmt, github.com, id, username, category, notes.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"fmt"

	"charm.land/huh/v2"
	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/client"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	secretKind     string
	secretUsername string
	secretURL      string
	secretCategory string
	secretTags     []string
	secretNotes    string
	secretFavorite bool
	secretValue    string
	secretTitle    string
)

var secretsCmd = &cobra.Command{
	Use:   "secrets",
	Short: "Sealed keystore (values never listed, only revealed)",
}

var secretsListCmd = &cobra.Command{
	Use:   "list",
	Short: "List secret metadata (no values)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("GET", "/api/secrets", nil)
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		rows, err := c.Secrets()
		if err != nil {
			return err
		}
		if len(rows) == 0 {
			ui.Info("no secrets stored")
			return nil
		}
		var out [][]string
		for _, s := range rows {
			out = append(out, []string{s.ID, s.Kind, s.Title})
		}
		ui.Table([]string{"ID", "KIND", "TITLE"}, out)
		return nil
	},
}

var secretsSaveCmd = &cobra.Command{
	Use:   "save <title>",
	Short: "Create (or upsert with --id) a sealed secret",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		value := secretValue
		if value == "" {
			form := ui.NewForm(huh.NewGroup(
				huh.NewInput().Title("Secret value").EchoMode(huh.EchoModePassword).Value(&value),
			))
			if err := form.Run(); err != nil {
				return err
			}
		}
		raw, err := c.SecretSave(map[string]any{
			"id": secretID, "kind": secretKind, "title": args[0],
			"username": secretUsername, "url": secretURL,
			"category": secretCategory, "tags": orEmptyList(secretTags),
			"notes": secretNotes, "favorite": secretFavorite,
			"value": value,
		})
		if err != nil {
			return err
		}
		ui.Success("secret sealed (id %s)", client.JSONField(raw, "id"))
		return nil
	},
}

var secretID string

// orEmptyList keeps JSON arrays as arrays (the server rejects null).
func orEmptyList(tags []string) []string {
	if tags == nil {
		return []string{}
	}
	return tags
}

var secretsGetCmd = &cobra.Command{
	Use:   "get <id>",
	Short: "Show secret metadata (never the value)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.SecretGet(args[0])
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var secretsRevealCmd = &cobra.Command{
	Use:   "reveal <id>",
	Short: "Print the plaintext value (audited server-side)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		value, err := c.SecretReveal(args[0])
		if err != nil {
			return err
		}
		fmt.Println(value)
		return nil
	},
}

var secretsRmCmd = &cobra.Command{
	Use:   "rm <id>",
	Short: "Delete a secret",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if !confirmDanger(fmt.Sprintf("delete secret %s?", args[0])) {
			return fmt.Errorf("aborted")
		}
		if err := c.SecretDelete(args[0]); err != nil {
			return err
		}
		ui.Success("secret %s deleted", args[0])
		return nil
	},
}

func init() {
	secretsSaveCmd.Flags().StringVar(&secretID, "id", "", "upsert an existing id")
	secretsSaveCmd.Flags().StringVar(&secretKind, "kind", "login", "login|apiKey|note|…")
	secretsSaveCmd.Flags().StringVar(&secretUsername, "username", "", "login username")
	secretsSaveCmd.Flags().StringVar(&secretURL, "url", "", "associated URL")
	secretsSaveCmd.Flags().StringVar(&secretCategory, "category", "", "category")
	secretsSaveCmd.Flags().StringSliceVar(&secretTags, "tags", nil, "comma-separated tags")
	secretsSaveCmd.Flags().StringVar(&secretNotes, "notes", "", "notes")
	secretsSaveCmd.Flags().BoolVar(&secretFavorite, "favorite", false, "mark favorite")
	secretsSaveCmd.Flags().StringVar(&secretValue, "value", "", "value (prompted securely otherwise)")
	secretsCmd.AddCommand(secretsListCmd, secretsSaveCmd, secretsGetCmd, secretsRevealCmd, secretsRmCmd)
	rootCmd.AddCommand(secretsCmd)
}
