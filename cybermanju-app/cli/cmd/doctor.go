// CyberManju OS — Doctor (cli/cmd/doctor.go).
// Role: cybsh CLI command.
// Exports: check.
// Integrates: fmt, github.com.
// Contracts: signals: engine.
package cmd

import (
	"fmt"
	"strings"

	"charm.land/lipgloss/v2"
	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/config"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/supa"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/util"
)

// doctorSupabase checks the dashboard-less limited backend.
func doctorSupabase() error {
	f := profile()
	ui.Header("system check — CLI Limited (Supabase direct, read-only)")
	ui.Emit(ui.Rule(48))
	var checks []check
	fail := func(name, detail string) { checks = append(checks, check{name, false, detail}) }
	pass := func(name, detail string) { checks = append(checks, check{name, true, detail}) }

	if f.Supa == nil || f.Supa.URL == "" {
		fail("broker configured", "cyb backend use supabase --url URL --key KEY")
		printChecks(checks)
		return fmt.Errorf("doctor: broker not configured")
	}
	pass("broker configured", f.Supa.URL)
	if f.Supa.AccessToken == "" {
		fail("broker session", "signed out — cyb supa login")
	} else {
		c := supa.New(f.Supa.URL, f.Supa.Key)
		if me, err := c.Me(f.Supa.AccessToken); err != nil {
			fail("broker session", err.Error()+" — cyb supa login")
		} else {
			who := me.Email
			if who == "" {
				who = me.UserID
			}
			pass("broker session", "valid ("+who+")")
		}
	}
	if f.Supa.ProviderToken == "" {
		fail("provider token", "absent — repos need it (re-login after granting scopes)")
	} else {
		pass("provider token", "present ("+f.Supa.Provider+")")
	}
	printChecks(checks)
	for _, ch := range checks {
		if !ch.ok {
			return fmt.Errorf("doctor: %d of %d checks failing", countFail(checks), len(checks))
		}
	}
	ui.Success("limited backend healthy — full power needs `cyb backend use docker|native|remote`")
	return nil
}

type check struct {
	name   string
	ok     bool
	detail string
}

var doctorCmd = &cobra.Command{
	Use:   "doctor",
	Short: "Health checklist: server, auth, crypto, disks, providers, agent",
	RunE: func(cmd *cobra.Command, args []string) error {
		c := mustClient()
		ui.Emit(ui.Splash(cliVersion))
		fmt.Println()
		if profile().Backend.TypeOrDefault() == config.BackendSupabase {
			return doctorSupabase()
		}
		ui.Header("system check — %s", c.BaseURL)
		ui.Emit(ui.Rule(48))

		var checks []check
		fail := func(name, detail string) { checks = append(checks, check{name, false, detail}) }
		pass := func(name, detail string) { checks = append(checks, check{name, true, detail}) }

		// 1. Reachability (public probes).
		if st, err := c.AuthStatus(); err != nil {
			fail("dashboard reachable", err.Error())
			printChecks(checks)
			return fmt.Errorf("doctor: dashboard unreachable")
		} else if st.RegistrationOpen {
			pass("dashboard reachable", "registration open (first run — cyb setup)")
		} else {
			pass("dashboard reachable", "registration closed")
		}
		if err := c.Readyz(); err != nil {
			fail("readiness (db + index + volume)", err.Error())
		} else {
			pass("readiness (db + index + volume)", "readyz 200")
		}

		// 2. Auth.
		if c.Token == "" {
			fail("auth session", "no JWT — run `cyb login`")
		} else if profile().Expired() && flagToken == "" {
			fail("auth session", "saved JWT expired — run `cyb login`")
		} else if _, err := c.Files(); err != nil {
			fail("auth session", err.Error())
		} else {
			pass("auth session", "JWT accepted")
		}

		// 3. Crypto engine.
		if _, err := c.CryptoStatus(); err != nil {
			fail("crypto engine", err.Error())
		} else {
			pass("crypto engine", "ChaCha20Poly1305 + PQC suites")
		}

		// 4. Disks.
		if df, err := c.VolumeDF(); err != nil {
			fail("vault volume", err.Error())
		} else if df.DiskCount == 0 {
			fail("vault volume", "no attached disks — cyb disk create <config> 10G")
		} else {
			pass("vault volume", fmt.Sprintf("%d disk(s), %s free", df.DiskCount, util.FormatBytes(df.FreeBytes)))
		}

		// 5. Providers.
		if cfgs, err := c.SyncConfigs(); err != nil {
			fail("sync providers", err.Error())
		} else if len(cfgs) == 0 {
			fail("sync providers", "none — cyb sync create --backend local …")
		} else {
			enabled := 0
			for _, g := range cfgs {
				if g.Enabled {
					enabled++
				}
			}
			pass("sync providers", fmt.Sprintf("%d configured (%d enabled)", len(cfgs), enabled))
		}

		// 6. Agent.
		if cfgs, err := c.AgentConfigs(); err != nil {
			fail("ai agent", err.Error())
		} else if len(cfgs) == 0 {
			fail("ai agent", "no configs — create one in the desktop AI panel")
		} else {
			keyed := 0
			for _, g := range cfgs {
				if g.HasKey {
					keyed++
				}
			}
			pass("ai agent", fmt.Sprintf("%d config(s), %d with keys", len(cfgs), keyed))
		}

		// 7. CLI version vs latest release (best effort).
		if rel, err := latestRelease(); err == nil && cliVersion != "dev" {
			switch util.CompareVersions(rel.Tag, cliVersion) {
			case 1:
				fail("cli update", fmt.Sprintf("%s → %s available (cyb update)", cliVersion, rel.Tag))
			default:
				pass("cli update", fmt.Sprintf("%s is current", cliVersion))
			}
		} else if cliVersion == "dev" {
			pass("cli update", "dev build — releases carry versioned binaries")
		}

		printChecks(checks)
		for _, ch := range checks {
			if !ch.ok {
				return fmt.Errorf("doctor: %d of %d checks failing", countFail(checks), len(checks))
			}
		}
		ui.Success("all %d checks green", len(checks))
		return nil
	},
}

func printChecks(checks []check) {
	w := 0
	for _, ch := range checks {
		if n := lipgloss.Width(ch.name); n > w {
			w = n
		}
	}
	for _, ch := range checks {
		mark := ui.Downsample(ui.Badge(ch.ok))
		detail := ch.detail
		if detail != "" {
			detail = " — " + detail
		}
		fmt.Printf("%s %s%s\n", mark, ch.name+strings.Repeat(" ", max(0, w-lipgloss.Width(ch.name))), ui.Downsample(ui.Dim.Render(detail)))
	}
}

func countFail(checks []check) int {
	n := 0
	for _, ch := range checks {
		if !ch.ok {
			n++
		}
	}
	return n
}

func init() {
	rootCmd.AddCommand(doctorCmd)
}
