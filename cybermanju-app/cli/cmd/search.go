// CyberManju OS — Search (cli/cmd/search.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: encoding, fmt, github.com.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"encoding/json"
	"fmt"

	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/client"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	searchLimit  int
	searchOffset int
)

var searchCmd = &cobra.Command{
	Use:   "search <query>",
	Short: "BM25 search over filename + content + tags",
	Args:  cobra.MinimumNArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.Search(joinArgs(args), searchLimit, searchOffset)
		if err != nil {
			return err
		}
		if flagJSON {
			return ui.PrintJSON(raw)
		}
		hits, derr := client.DecodeList[client.SearchHit](raw)
		if derr != nil {
			return ui.PrintJSON(raw)
		}
		if len(hits) == 0 {
			ui.Info("no hits")
			return nil
		}
		var rows [][]string
		for _, h := range hits {
			rows = append(rows, []string{h.FileID, h.FileName, ui.Number.Render(fmt.Sprintf("%.2f", h.Score)), truncate(h.Snippet, 72)})
		}
		ui.Table([]string{"FILE", "NAME", "SCORE", "SNIPPET"}, rows)
		return nil
	},
}

func joinArgs(args []string) string {
	out := ""
	for i, a := range args {
		if i > 0 {
			out += " "
		}
		out += a
	}
	return out
}

func truncate(s string, n int) string {
	r := []rune(s)
	for i, c := range r {
		if c == '\n' || c == '\r' || c == '\t' {
			r[i] = ' '
		}
	}
	if len(r) > n {
		return string(r[:n]) + "…"
	}
	return string(r)
}

var suggestCmd = &cobra.Command{
	Use:   "suggest <prefix>",
	Short: "Autocomplete suggestions",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.Suggest(args[0], searchLimit)
		if err != nil {
			return err
		}
		return ui.PrintJSON(raw)
	},
}

var geoCmd = &cobra.Command{
	Use:   "geo",
	Short: "Files with GPS coordinates",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		if err := requireAuth(c); err != nil {
			return err
		}
		raw, err := c.GeoFiles()
		if err != nil {
			return err
		}
		if flagJSON {
			return ui.PrintJSON(raw)
		}
		var items []struct {
			ID     string   `json:"id"`
			Name   string   `json:"name"`
			GpsLat *float64 `json:"gpsLat"`
			GpsLon *float64 `json:"gpsLon"`
		}
		if err := json.Unmarshal(raw, &items); err != nil {
			return ui.PrintJSON(raw)
		}
		var rows [][]string
		for _, it := range items {
			lat, lon := "", ""
			if it.GpsLat != nil {
				lat = fmt.Sprintf("%.5f", *it.GpsLat)
			}
			if it.GpsLon != nil {
				lon = fmt.Sprintf("%.5f", *it.GpsLon)
			}
			rows = append(rows, []string{it.ID, it.Name, lat, lon})
		}
		if len(rows) == 0 {
			ui.Info("no files carry GPS coordinates")
			return nil
		}
		ui.Table([]string{"ID", "NAME", "LAT", "LON"}, rows)
		return nil
	},
}

func init() {
	searchCmd.Flags().IntVar(&searchLimit, "limit", 20, "max hits")
	searchCmd.Flags().IntVar(&searchOffset, "offset", 0, "hit offset")
	suggestCmd.Flags().IntVar(&searchLimit, "limit", 10, "max suggestions")
	rootCmd.AddCommand(searchCmd, suggestCmd, geoCmd)
}
