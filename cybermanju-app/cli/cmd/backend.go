// CyberManju OS — Backend (cli/cmd/backend.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: fmt, github.com, signed, expired, Allowlist, http:.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"fmt"

	"charm.land/huh/v2"
	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/client"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/config"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/supa"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	backendServer string
	backendPort   int
	backendURL    string
	backendKey    string
)

var backendCmd = &cobra.Command{
	Use:   "backend",
	Short: "Choose how cyb reaches a vault (remote|docker|native|supabase)",
	Long: `Backends:
  remote    dashboard URL (server flag / CYBERMANJU_SERVER / saved)
  docker    container managed by cyb (cyb serve up under the hood)
  native    installed CyberManju OS app (its localhost dashboard)
  CLI Limited (supabase)  broker-direct: read-only, no decrypt/encrypt`,
	RunE: func(cmd *cobra.Command, args []string) error {
		return backendStatusCmd.RunE(cmd, args)
	},
}

var backendStatusCmd = &cobra.Command{
	Use:   "status",
	Short: "Show the active backend and probe it",
	RunE: func(cmd *cobra.Command, args []string) error {
		f := profile()
		t := f.Backend.TypeOrDefault()
		ui.Header("◈ backend: %s", t)
		switch t {
		case config.BackendSupabase:
			if f.Supa == nil || f.Supa.URL == "" {
				ui.Info("broker not configured — cyb backend use supabase --url URL --key KEY")
				return nil
			}
			ui.KV(
				[2]string{"broker", f.Supa.URL},
				[2]string{"session", supaSessionState(f)},
			)
		default:
			server := config.ResolveDashboard(flagServer, f)
			ui.KV([2]string{"server", server})
			c := client.New(server, "", flagTimeout)
			if _, err := c.AuthStatus(); err != nil {
				ui.Failure("unreachable: %v", err)
				return fmt.Errorf("backend probe failed")
			}
			ui.Success("dashboard answers")
		}
		return nil
	},
}

func supaSessionState(f config.File) string {
	if f.Supa == nil || f.Supa.AccessToken == "" {
		return "signed out — cyb supa login"
	}
	if f.Supa.Expired() {
		return "expired — cyb supa login (refresh on next status)"
	}
	who := f.Supa.Email
	if who == "" {
		who = f.Supa.UserID
	}
	return "signed in as " + who
}

var backendUseCmd = &cobra.Command{
	Use:   "use <remote|docker|native|supabase>",
	Short: "Select and configure a backend",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		f := profile()
		switch args[0] {
		case config.BackendRemote:
			f.Backend.Type = config.BackendRemote
			if backendServer != "" {
				f.Server = backendServer
			}
			if err := config.Save(f); err != nil {
				return err
			}
			ui.Success("backend → remote (%s)", config.ResolveDashboard("", f))
		case config.BackendDocker:
			port := backendPort
			if port == 0 {
				port = 3456
			}
			if err := ensureDockerUp(port, "", ""); err != nil {
				return err
			}
			f.Backend.Type = config.BackendDocker
			f.Backend.DockerPort = port
			if err := config.Save(f); err != nil {
				return err
			}
			ui.Success("backend → docker (http://127.0.0.1:%d)", port)
		case config.BackendNative:
			port := backendPort
			if port == 0 {
				port = 3456
			}
			probe := client.New(fmt.Sprintf("http://127.0.0.1:%d", port), "", flagTimeout)
			if _, err := probe.AuthStatus(); err != nil {
				return fmt.Errorf("no CyberManju dashboard on 127.0.0.1:%d — is the native app (deb/appimage/dmg/apk) running? (%v)", port, err)
			}
			f.Backend.Type = config.BackendNative
			f.Backend.NativePort = port
			if err := config.Save(f); err != nil {
				return err
			}
			ui.Success("backend → native app (http://127.0.0.1:%d)", port)
		case config.BackendSupabase:
			brokerURL, brokerKey := backendURL, backendKey
			source := "paste"
			if brokerURL == "" || brokerKey == "" {
				if bURL, bKey, src := supa.BrokerPair("", ""); src == "baked into this build" {
					brokerURL, brokerKey, source = bURL, bKey, src
					ui.Info("using the broker baked into this build")
				} else {
					form := ui.NewForm(huh.NewGroup(
						huh.NewInput().Title("Supabase project URL").Value(&brokerURL),
						huh.NewInput().Title("Anon / publishable key").EchoMode(huh.EchoModePassword).Value(&brokerKey),
					))
					if err := form.Run(); err != nil {
						return err
					}
				}
			}
			if brokerURL == "" || brokerKey == "" {
				return fmt.Errorf("broker URL and key are required (same pair as Settings → OAuth broker)")
			}
			f.Backend.Type = config.BackendSupabase
			if f.Supa == nil {
				f.Supa = &config.SupaSession{}
			}
			f.Supa.URL = brokerURL
			f.Supa.Key = brokerKey
			if err := config.Save(f); err != nil {
				return err
			}
			ui.Success("backend → CLI Limited (Supabase direct, read-only; broker %s)", source)
			ui.Emit(ui.NextSteps("supabase allowlist",
				[2]string{"http://127.0.0.1:54329/callback", "allow under Authentication → URL Configuration → Redirect URLs"},
				[2]string{"cyb supa login", "sign in through the broker"},
			))
		default:
			return fmt.Errorf("unsupported: backend must be remote|docker|native|supabase (got %q)", args[0])
		}
		return nil
	},
}

func init() {
	backendUseCmd.Flags().StringVar(&backendServer, "server", "", "remote dashboard URL")
	backendUseCmd.Flags().IntVar(&backendPort, "port", 0, "loopback port (docker/native, default 3456)")
	backendUseCmd.Flags().StringVar(&backendURL, "url", "", "Supabase project URL")
	backendUseCmd.Flags().StringVar(&backendKey, "key", "", "Supabase anon key")
	backendCmd.AddCommand(backendStatusCmd, backendUseCmd)
	rootCmd.AddCommand(backendCmd)
}
