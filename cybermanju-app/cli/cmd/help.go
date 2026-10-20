// CyberManju OS — Help (cli/cmd/help.go).
// Role: styled `cyb --help` — gradient masthead, grouped command atlas,
// and tinted flags, all in the CyberManju neon face.
// Exports: installHelp.
// Integrates: fmt, strings, cobra, pflag, ui.
// Contracts: root help is the neon atlas, subcommand help is a neon page;
// hidden, deprecated and unmapped commands fall into the "more" section
// (sorted) so new commands can never vanish from help.
package cmd

import (
	"fmt"
	"sort"
	"strings"

	"github.com/spf13/cobra"
	"github.com/spf13/pflag"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

// helpSections buckets top-level commands into the neon atlas.
var helpSections = []struct {
	title string
	cmds  []string
}{
	{"vault", []string{"files", "batch", "code", "crypto"}},
	{"devices & sync", []string{"disk", "sync", "share", "audit", "repair", "scrub", "lease", "cron"}},
	{"shell everywhere", []string{"sh", "run", "repl"}},
	{"intelligence", []string{"ai", "search", "suggest", "geo"}},
	{"people & secrets", []string{"users", "perms", "collections", "accounts", "groups", "loose", "faces", "secrets", "oauth"}},
	{"machine", []string{"backend", "supa", "serve", "config"}},
	{"self care", []string{"setup", "login", "logout", "doctor", "update", "version", "completion"}},
}

// installHelp swaps the root help renderer for the neon atlas.
func installHelp() {
	rootCmd.SetHelpFunc(renderRootHelp)
}

// renderRootHelp prints the masthead atlas for `cyb --help` and a styled
// per-command page for everything below the root.
func renderRootHelp(c *cobra.Command, _ []string) {
	if c != rootCmd {
		renderCmdHelp(c)
		return
	}
	ui.Emit(ui.Splash(cliVersion))
	fmt.Println()
	fmt.Printf("  %s\n\n", ui.Downsample(ui.Dim.Render("files, disks, sync, OAuth, cybsh and the AI agent — one terminal.")))

	byName := map[string]*cobra.Command{}
	for _, sub := range c.Commands() {
		if sub.Hidden || sub.Deprecated != "" {
			continue
		}
		byName[sub.Name()] = sub
	}
	shown := map[string]bool{}
	nameWidth := 0
	for _, sub := range byName {
		if w := len(sub.Name()); w > nameWidth {
			nameWidth = w
		}
	}
	for _, sec := range helpSections {
		var rows [][]string
		for _, name := range sec.cmds {
			sub, ok := byName[name]
			if !ok {
				continue
			}
			rows = append(rows, []string{sub.Name(), sub.Short})
			shown[name] = true
		}
		if len(rows) == 0 {
			continue
		}
		fmt.Printf("  %s\n", ui.Downsample(ui.Title.Render("◈ "+sec.title)))
		for _, r := range rows {
			fmt.Printf("    %s  %s\n",
				ui.Downsample(ui.Accent.Render(r[0]+strings.Repeat(" ", max(0, nameWidth-len(r[0]))))),
				ui.Downsample(ui.Dim.Render(r[1])))
		}
		fmt.Println()
	}
	var rest [][]string
	for name, sub := range byName {
		if !shown[name] && name != "help" && name != "completion" {
			rest = append(rest, []string{name, sub.Short})
		}
	}
	sort.Slice(rest, func(i, j int) bool { return rest[i][0] < rest[j][0] })
	if len(rest) > 0 {
		fmt.Printf("  %s\n", ui.Downsample(ui.Title.Render("◈ more")))
		for _, r := range rest {
			fmt.Printf("    %s  %s\n",
				ui.Downsample(ui.Accent.Render(r[0]+strings.Repeat(" ", max(0, nameWidth-len(r[0]))))),
				ui.Downsample(ui.Dim.Render(r[1])))
		}
		fmt.Println()
	}
	fmt.Printf("  %s  %s\n", ui.Downsample(ui.Title.Render("◈ flags")), ui.Downsample(ui.Dim.Render("cyb <command> [flags]")))
	c.PersistentFlags().VisitAll(func(f *pflag.Flag) {
		name, usage := flagNameUsage(f)
		fmt.Printf("    %s  %s\n", ui.Downsample(ui.Chip(name)), ui.Downsample(ui.Dim.Render(usage)))
	})
	fmt.Println()
	fmt.Printf("  %s  %s\n", ui.Downsample(ui.Dim.Render("Start here:")), ui.Downsample(ui.Chip("cyb setup")))
}

// renderCmdHelp prints a neon help page for any non-root command.
func renderCmdHelp(c *cobra.Command) {
	fmt.Printf("%s  %s\n\n",
		ui.Downsample(ui.Gradient("◈ "+c.CommandPath(), ui.PinkHot, ui.RedHot)),
		ui.Downsample(ui.Dim.Render(c.Short)))
	if c.Long != "" {
		fmt.Printf("  %s\n\n", ui.Downsample(ui.Dim.Render(strings.TrimSpace(c.Long))))
	}
	fmt.Printf("  %s  %s\n\n", ui.Downsample(ui.Title.Render("usage")), ui.Downsample(ui.Chip(c.UseLine())))
	if len(c.Aliases) > 0 {
		fmt.Printf("  %s  %s\n\n", ui.Downsample(ui.Title.Render("aliases")), ui.Downsample(ui.Dim.Render(strings.Join(c.Aliases, ", "))))
	}
	if c.HasAvailableSubCommands() {
		fmt.Printf("  %s\n", ui.Downsample(ui.Title.Render("◈ commands")))
		w := 0
		for _, sub := range c.Commands() {
			if sub.Hidden || sub.Deprecated != "" {
				continue
			}
			if n := len(sub.Name()); n > w {
				w = n
			}
		}
		for _, sub := range c.Commands() {
			if sub.Hidden || sub.Deprecated != "" {
				continue
			}
			fmt.Printf("    %s  %s\n",
				ui.Downsample(ui.Accent.Render(sub.Name()+strings.Repeat(" ", max(0, w-len(sub.Name()))))),
				ui.Downsample(ui.Dim.Render(sub.Short)))
		}
		fmt.Println()
	}
	printFlagSet("flags", c.LocalFlags())
	printFlagSet("global flags", c.InheritedFlags())
}

// flagNameUsage renders one flag as --name/-s plus usage with its
// non-zero default appended, cobra-style.
func flagNameUsage(f *pflag.Flag) (string, string) {
	name := "--" + f.Name
	if f.Shorthand != "" {
		name += ", -" + f.Shorthand
	}
	usage := f.Usage
	if f.DefValue != "" && f.DefValue != "false" && f.DefValue != "0" {
		usage += " (default " + f.DefValue + ")"
	}
	return name, usage
}

// printFlagSet renders a titled flag list; empty sets print nothing.
// Non-zero defaults are shown, cobra-style.
func printFlagSet(title string, fs *pflag.FlagSet) {
	var rows [][2]string
	fs.VisitAll(func(f *pflag.Flag) {
		if f.Hidden {
			return
		}
		name, usage := flagNameUsage(f)
		rows = append(rows, [2]string{name, usage})
	})
	if len(rows) == 0 {
		return
	}
	fmt.Printf("  %s\n", ui.Downsample(ui.Title.Render("◈ "+title)))
	for _, r := range rows {
		fmt.Printf("    %s  %s\n", ui.Downsample(ui.Chip(r[0])), ui.Downsample(ui.Dim.Render(r[1])))
	}
	fmt.Println()
}
