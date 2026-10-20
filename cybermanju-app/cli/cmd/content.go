// CyberManju OS — Content (cli/cmd/content.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: fmt, os, github.com.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	batchAlgo  string
	batchLayer string
	// filesCatRender glamour-renders the content instead of dumping JSON.
	filesCatRender bool
)

var filesCatCmd = &cobra.Command{
	Use:   "cat <file-id>",
	Short: "Read managed file text (honest encrypted:/binary:/too_large: refusals)",
	Long: `Prints the content envelope as JSON. Pass -r/--render to glamour-render
markdown straight to the terminal instead:

  cyb files cat <file-id> -r`,
	Args: cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if flagJSON {
			raw, err := c.Raw("GET", "/api/files/"+args[0]+"/content", nil)
			if err != nil {
				return err
			}
			return ui.PrintJSON(raw)
		}
		raw, err := c.FileContent(args[0])
		if err != nil {
			return err
		}
		if !filesCatRender {
			if ui.IsMarkdown(contentName(raw)) {
				ui.Hint("markdown file — render it with %s", ui.Chip("cyb files cat "+args[0]+" -r"))
			}
			return ui.PrintJSON(raw)
		}
		var payload struct {
			Name    string `json:"name"`
			Content string `json:"content"`
		}
		if err := json.Unmarshal(raw, &payload); err != nil || payload.Content == "" {
			return ui.PrintJSON(raw)
		}
		fmt.Println(ui.RenderMarkdown(payload.Content))
		return nil
	},
}

// contentName best-effort extracts the file name from a content envelope
// (empty when the shape is unexpected — callers treat that as not-markdown).
func contentName(raw []byte) string {
	var payload struct {
		Name string `json:"name"`
	}
	if err := json.Unmarshal(raw, &payload); err != nil {
		return ""
	}
	return payload.Name
}

var filesWriteCmd = &cobra.Command{
	Use:   "write <file-id> (--text TEXT | --file PATH)",
	Short: "Overwrite managed text (version snapshot first, 1 MiB cap)",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		content := filesWriteText
		if filesWriteFile != "" {
			raw, err := os.ReadFile(filesWriteFile)
			if err != nil {
				return err
			}
			content = string(raw)
		}
		if content == "" {
			return fmt.Errorf("nothing to write — pass --text or --file")
		}
		out, err := c.FileContentWrite(args[0], content)
		if err != nil {
			return err
		}
		return ui.PrintJSON(out)
	},
}

var filesWriteText string
var filesWriteFile string

var filesVersionsCmd = &cobra.Command{
	Use:   "versions <file-id>",
	Short: "List version snapshots",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.FileVersions(args[0])
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var filesSnapshotCmd = &cobra.Command{
	Use:   "snapshot <file-id>",
	Short: "Create a version snapshot now",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.FileVersionCreate(args[0])
		if err != nil {
			return err
		}
		ui.Success("snapshot created")
		return ui.PrintJSON(raw)
	},
}

var filesRevertCmd = &cobra.Command{
	Use:   "revert <file-id> <version-id>",
	Short: "Revert to a version snapshot",
	Args:  cobra.ExactArgs(2),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if !confirmDanger(fmt.Sprintf("revert %s to version %s?", args[0], args[1])) {
			return fmt.Errorf("aborted")
		}
		raw, err := c.FileVersionRevert(args[0], args[1])
		if err != nil {
			return err
		}
		ui.Success("reverted")
		return ui.PrintJSON(raw)
	},
}

var filesSnapshotAllCmd = &cobra.Command{
	Use:   "snapshot-all",
	Short: "Snapshot every versioned file",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		var out []byte
		if err := ui.SpinWhile("Snapshotting…", func() error {
			raw, err := c.VersionsSnapshotAll()
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

var batchCmd = &cobra.Command{
	Use:   "batch",
	Short: "Bulk delete / encrypt / compress",
}

var batchDeleteCmd = &cobra.Command{
	Use:   "delete <file-id...>",
	Short: "Delete many files at once",
	Args:  cobra.MinimumNArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		if !confirmDanger(fmt.Sprintf("delete %d files?", len(args))) {
			return fmt.Errorf("aborted")
		}
		raw, err := c.BatchDelete(args)
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var batchEncryptCmd = &cobra.Command{
	Use:   "encrypt <file-id...>",
	Short: "Encrypt many files (--algo, default vault cipher)",
	Args:  cobra.MinimumNArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		var out []byte
		if err := ui.SpinWhile("Encrypting…", func() error {
			raw, err := c.BatchEncrypt(args, batchAlgo)
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

var batchCompressCmd = &cobra.Command{
	Use:   "compress <file-id...>",
	Short: "Compress many files (--layer lz4|zstd|brotli|cascade)",
	Args:  cobra.MinimumNArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		var out []byte
		if err := ui.SpinWhile("Compressing…", func() error {
			raw, err := c.BatchCompress(args, batchLayer)
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

var codeCmd = &cobra.Command{
	Use:   "code",
	Short: "Code intelligence (tree-sitter + heuristic, engine reported)",
}

var codeParseCmd = &cobra.Command{
	Use:   "parse <file> [--as name.ext]",
	Short: "Parse source text into symbols",
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
		name := codeAs
		if name == "" {
			name = args[0]
		}
		out, err := c.CodeParse(name, string(raw))
		if err != nil {
			return err
		}
		return ui.PrintJSON(out)
	},
}

var codeAs string

var cryptoCmd = &cobra.Command{
	Use:   "crypto",
	Short: "Crypto engine status + key inventory",
}

var cryptoStatusCmd = &cobra.Command{
	Use:   "status",
	Short: "Engine capabilities and supported algorithms",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.CryptoStatus()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var cryptoKeysCmd = &cobra.Command{
	Use:   "keys",
	Short: "List key inventory (ids only, never material)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.CryptoKeys()
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

func init() {
	filesCatCmd.Flags().BoolVarP(&filesCatRender, "render", "r", false, "glamour-render markdown to the terminal")
	filesWriteCmd.Flags().StringVar(&filesWriteText, "text", "", "inline text content")
	filesWriteCmd.Flags().StringVar(&filesWriteFile, "file", "", "read content from a local file")
	batchEncryptCmd.Flags().StringVar(&batchAlgo, "algo", "", "cipher (default vault cipher)")
	batchCompressCmd.Flags().StringVar(&batchLayer, "layer", "cascade", "lz4|zstd|brotli|cascade")
	codeParseCmd.Flags().StringVar(&codeAs, "as", "", "filename to report (for stdin-like paths)")
	filesCmd.AddCommand(filesCatCmd, filesWriteCmd, filesVersionsCmd,
		filesSnapshotCmd, filesRevertCmd, filesSnapshotAllCmd)
	batchCmd.AddCommand(batchDeleteCmd, batchEncryptCmd, batchCompressCmd)
	codeCmd.AddCommand(codeParseCmd)
	cryptoCmd.AddCommand(cryptoStatusCmd, cryptoKeysCmd)
	rootCmd.AddCommand(batchCmd, codeCmd, cryptoCmd)
}
