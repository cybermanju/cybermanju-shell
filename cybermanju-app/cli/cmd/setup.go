// CyberManju OS — Setup (cli/cmd/setup.go).
// Role: cybsh command for first-run backend and account setup.
// Exports: module root (see items below).
// Integrates: fmt, os, runtime, strings, huh, client, config, supa, ui.
// Contracts: preserve saved backend/session fields when updating the profile.
package cmd

import (
	"fmt"
	"os"
	"runtime"
	"strings"

	"charm.land/huh/v2"
	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/client"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/config"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/supa"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

type setupTarget string

const (
	setupTargetDashboard setupTarget = "remote"
	setupTargetLimited   setupTarget = "supabase"
)

var (
	setupUsername string
	setupPassword string
	setupDisplay  string
	setupLogin    bool
)

var setupCmd = &cobra.Command{
	Use:   "setup",
	Short: "Choose a backend, connect a dashboard, and save your profile",
	Long: `On Termux, choose a remote dashboard or CLI Limited (Supabase) before
entering connection details. Dashboard setup probes the API, then creates or
signs into an account and saves the profile (~/.config/cybermanju/cyb.json, 0600).

No local server yet? On a desktop, run:  cyb serve up`,
	RunE: func(cmd *cobra.Command, args []string) error {
		f := profile()
		nonInteractive := setupUsername != "" || setupLogin
		if !nonInteractive {
			ui.Emit(ui.Splash(cliVersion))
			fmt.Println()
		}

		// A previously selected CLI Limited backend is already the setup type;
		// do not send its Supabase project URL to the dashboard auth endpoint.
		if f.Backend.Type == config.BackendSupabase {
			return setupLimitedBackend(f)
		}

		target := setupTargetDashboard
		if shouldAskSetupTarget(runtime.GOOS, f, nonInteractive, flagServer) {
			var err error
			target, err = chooseTermuxSetupTarget()
			if err != nil {
				return err
			}
		}
		if target == setupTargetLimited {
			return setupLimitedBackend(f)
		}
		if runtime.GOOS == "android" && f.Backend.Type == "" {
			f.Backend.Type = config.BackendRemote
		}

		server := config.ResolveServer(flagServer, f)
		// A Termux remote dashboard must be supplied explicitly; localhost is
		// not a useful default when Docker and the native app are unavailable.
		if runtime.GOOS == "android" && strings.TrimSpace(flagServer) == "" &&
			strings.TrimSpace(f.Server) == "" && strings.TrimSpace(os.Getenv("CYBERMANJU_SERVER")) == "" {
			server = ""
		}

		var username, password, display string
		if nonInteractive {
			username, password, display = setupUsername, setupPassword, setupDisplay
			if username == "" || password == "" {
				return fmt.Errorf("non-interactive setup needs --username and --password")
			}
		} else {
			form := ui.NewForm(
				huh.NewGroup(
					huh.NewInput().
						Title("Dashboard URL").
						Value(&server).
						Validate(func(s string) error {
							if strings.TrimSpace(s) == "" {
								return fmt.Errorf("a dashboard URL is required")
							}
							return nil
						}),
					huh.NewInput().Title("Username").Value(&username),
					huh.NewInput().Title("Password").EchoMode(huh.EchoModePassword).Value(&password),
					huh.NewInput().Title("Display name (register only, optional)").Value(&display),
				),
			)
			if err := form.Run(); err != nil {
				return err
			}
		}

		c := client.New(server, "", flagTimeout)
		var st client.AuthStatus
		if err := ui.SpinWhile("Checking dashboard API…", func() error {
			var err error
			st, err = c.AuthStatus()
			return err
		}); err != nil {
			ui.Failure("no dashboard at %s", server)
			ui.Hint("start one with %s", ui.Chip("cyb serve up"))
			ui.Hint("or point elsewhere with %s", ui.Chip("cyb setup --server URL"))
			ui.Hint("if this is a Supabase project URL, rerun setup and choose CLI Limited")
			ui.Hint("(%v)", err)
			return fmt.Errorf("setup aborted")
		}

		mode := "login"
		if st.RegistrationOpen && !setupLogin {
			mode = "register"
		}
		if mode == "register" {
			if err := c.Register(username, password, display, ""); err != nil {
				return err
			}
			ui.Success("account %q created", username)
		} else {
			ui.Info("registration closed — signing in")
		}
		var login client.LoginResp
		if err := ui.SpinWhile("Signing in…", func() error {
			var err error
			login, err = c.Login(username, password)
			return err
		}); err != nil {
			return err
		}
		f.Server = server
		f.Token = login.Token
		f = f.WithExpiry(login.ExpiresIn)
		if err := config.Save(f); err != nil {
			return err
		}
		ui.Success("signed in as %s (%s) — profile saved", login.Username, login.Role)
		fmt.Println()
		ui.Emit(ui.NextSteps("next",
			[2]string{"cyb oauth launch google --config <id>", "connect a provider"},
			[2]string{"cyb disk create <config> 10G", "encrypted vault disk"},
			[2]string{`cyb sh "help"`, "the full cybsh verb list"},
		))
		return nil
	},
}

func setupTargetChoices(goos string) []setupTarget {
	if goos != "android" {
		return nil
	}
	return []setupTarget{setupTargetDashboard, setupTargetLimited}
}

func shouldAskSetupTarget(goos string, f config.File, nonInteractive bool, explicitServer string) bool {
	return goos == "android" && f.Backend.Type == "" && !nonInteractive && strings.TrimSpace(explicitServer) == ""
}

func chooseTermuxSetupTarget() (setupTarget, error) {
	target := setupTargetDashboard
	options := make([]huh.Option[setupTarget], 0, 2)
	for _, choice := range setupTargetChoices(runtime.GOOS) {
		switch choice {
		case setupTargetDashboard:
			options = append(options, huh.NewOption("Remote dashboard — full CLI", choice))
		case setupTargetLimited:
			options = append(options, huh.NewOption("CLI Limited — Supabase direct, read-only", choice))
		}
	}
	form := ui.NewForm(huh.NewGroup(
		huh.NewSelect[setupTarget]().
			Title("Setup type").
			Options(options...).
			Value(&target),
	))
	if err := form.Run(); err != nil {
		return "", err
	}
	return target, nil
}

func setupLimitedBackend(f config.File) error {
	brokerURL, brokerKey, source := "", "", ""
	if f.Supa != nil && f.Supa.URL != "" && f.Supa.Key != "" {
		brokerURL, brokerKey, source = f.Supa.URL, f.Supa.Key, "saved profile"
	} else if url, key, src := supa.BrokerPair("", ""); src == "baked into this build" {
		brokerURL, brokerKey, source = url, key, src
		ui.Info("using the broker baked into this CLI build")
	}
	if brokerURL == "" || brokerKey == "" {
		form := ui.NewForm(huh.NewGroup(
			huh.NewInput().Title("Supabase project URL").Value(&brokerURL),
			huh.NewInput().Title("Anon / publishable key").EchoMode(huh.EchoModePassword).Value(&brokerKey),
		))
		if err := form.Run(); err != nil {
			return err
		}
		if source == "" {
			source = "entered during setup"
		}
	}

	updated, err := withSupabaseBackend(f, brokerURL, brokerKey)
	if err != nil {
		return err
	}
	if err := config.Save(updated); err != nil {
		return err
	}
	ui.Success("setup complete — CLI Limited (Supabase direct, read-only; broker %s)", source)
	ui.Emit(ui.NextSteps("Supabase allowlist",
		[2]string{"http://127.0.0.1:54329/callback", "allow under Authentication → URL Configuration → Redirect URLs"},
		[2]string{"cyb supa login", "sign in through the broker"},
	))
	return nil
}

func withSupabaseBackend(f config.File, brokerURL, brokerKey string) (config.File, error) {
	brokerURL = strings.TrimSpace(brokerURL)
	brokerKey = strings.TrimSpace(brokerKey)
	if brokerURL == "" || brokerKey == "" {
		return f, fmt.Errorf("Supabase project URL and anon key are required")
	}
	f.Backend.Type = config.BackendSupabase
	if f.Supa == nil {
		f.Supa = &config.SupaSession{}
	}
	f.Supa.URL = brokerURL
	f.Supa.Key = brokerKey
	return f, nil
}

func init() {
	setupCmd.Flags().StringVar(&setupUsername, "username", "", "non-interactive username")
	setupCmd.Flags().StringVar(&setupPassword, "password", "", "non-interactive password")
	setupCmd.Flags().StringVar(&setupDisplay, "display-name", "", "display name for registration")
	setupCmd.Flags().BoolVar(&setupLogin, "login", false, "force login even when registration is open")
	rootCmd.AddCommand(setupCmd)
}
