// CyberManju OS — Supa (cli/cmd/supa.go).
// Role: cybsh CLI command.
// Exports: module root (see items below).
// Integrates: fmt, time, github.com, absent, present, github, openURL.
// Contracts: none special (pure logic; errors bubble to caller).
package cmd

import (
	"fmt"
	"time"

	"github.com/spf13/cobra"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/client"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/config"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/supa"
	"github.com/cybermanju/cybermanju.github.io/cli/internal/ui"
)

var (
	supaProvider string
	supaPort     int
	supaInstance string
)

var supaCmd = &cobra.Command{
	Use:   "supa",
	Short: "CLI Limited — Supabase direct, read-only (no dashboard)",
	Long: `Broker-direct access without any dashboard: Supabase Auth in the
terminal, then read-only provider APIs (repos, files, projects).
Encrypted vault blobs stay opaque here — no decrypt/encrypt.`,
}

func supaClient() (*supa.Client, config.File, error) {
	f := profile()
	if f.Backend.TypeOrDefault() != config.BackendSupabase {
		return nil, f, fmt.Errorf("supa mode is not selected — `cyb backend use supabase` first")
	}
	if f.Supa == nil || f.Supa.URL == "" || f.Supa.Key == "" {
		return nil, f, fmt.Errorf("broker not configured — `cyb backend use supabase --url URL --key KEY`")
	}
	return supa.New(f.Supa.URL, f.Supa.Key), f, nil
}

var supaLoginCmd = &cobra.Command{
	Use:   "login",
	Short: "Sign in via the broker (browser PKCE loopback)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c, f, err := supaClient()
		if err != nil {
			return err
		}
		provider, err := client.OAuthProvider(supaProvider)
		if err != nil {
			return err
		}
		verifier, err := supa.NewVerifier()
		if err != nil {
			return err
		}
		redirect := supa.RedirectTo(supaPort)
		authURL := c.AuthorizeURL(provider, redirect, supa.Challenge(verifier))
		ui.Info("opening approval in your browser…")
		ui.Info("allowlisted callback must be: %s", redirect)
		if berr := openURL(authURL); berr != nil {
			ui.Info("could not open a browser — visit:\n%s", authURL)
		}
		var code string
		if err := ui.SpinWhile("Waiting for the browser callback…", func() error {
			var werr error
			code, werr = supa.WaitForCode(supaPort, 3*time.Minute)
			return werr
		}); err != nil {
			return err
		}
		sess, err := c.Exchange(code, verifier)
		if err != nil {
			return err
		}
		f.Supa.AccessToken = sess.AccessToken
		f.Supa.RefreshToken = sess.RefreshToken
		f.Supa.UserID = sess.UserID
		f.Supa.Email = sess.Email
		f.Supa.Provider = provider
		f.Supa.ProviderToken = sess.ProviderToken
		if sess.ExpiresIn > 0 {
			f.Supa.ExpiresAt = time.Now().Add(time.Duration(sess.ExpiresIn) * time.Second).UTC().Format(time.RFC3339)
		}
		if err := config.Save(f); err != nil {
			return err
		}
		ui.Success("signed in as %s (limited, read-only)", displayEmail(sess))
		if sess.ProviderToken == "" {
			ui.Info("no provider token in this session — repos need it (re-login after granting provider scopes)")
		}
		return nil
	},
}

func displayEmail(sess supa.Session) string {
	if sess.Email != "" {
		return sess.Email
	}
	return sess.UserID
}

var supaLogoutCmd = &cobra.Command{
	Use:   "logout",
	Short: "Forget the Supabase session (broker pair kept)",
	RunE: func(cmd *cobra.Command, args []string) error {
		f := profile()
		if f.Supa == nil {
			ui.Info("nothing stored")
			return nil
		}
		f.Supa.AccessToken = ""
		f.Supa.RefreshToken = ""
		f.Supa.ExpiresAt = ""
		f.Supa.UserID = ""
		f.Supa.Email = ""
		f.Supa.Provider = ""
		f.Supa.ProviderToken = ""
		if err := config.Save(f); err != nil {
			return err
		}
		ui.Success("supabase session cleared")
		return nil
	},
}

var supaStatusCmd = &cobra.Command{
	Use:   "status",
	Short: "Validate the broker session (refreshes once on expiry)",
	RunE: func(cmd *cobra.Command, args []string) error {
		c, f, err := supaClient()
		if err != nil {
			return err
		}
		if f.Supa.AccessToken == "" {
			ui.Info("signed out — cyb supa login")
			return nil
		}
		me, merr := c.Me(f.Supa.AccessToken)
		if merr != nil && f.Supa.RefreshToken != "" {
			ui.Info("access token stale — refreshing…")
			if sess, rerr := c.Refresh(f.Supa.RefreshToken); rerr == nil {
				f.Supa.AccessToken = sess.AccessToken
				f.Supa.RefreshToken = sess.RefreshToken
				if sess.ExpiresIn > 0 {
					f.Supa.ExpiresAt = time.Now().Add(time.Duration(sess.ExpiresIn) * time.Second).UTC().Format(time.RFC3339)
				}
				_ = config.Save(f)
				me, merr = c.Me(f.Supa.AccessToken)
			} else {
				return fmt.Errorf("session invalid and refresh failed — cyb supa login (%v)", rerr)
			}
		}
		if merr != nil {
			return fmt.Errorf("session invalid — cyb supa login (%v)", merr)
		}
		who := me.Email
		if who == "" {
			who = me.UserID
		}
		ui.Success("broker session valid (%s)", who)
		ui.KV(
			[2]string{"provider", f.Supa.Provider},
			[2]string{"provider token", present(f.Supa.ProviderToken)},
			[2]string{"expires", f.Supa.ExpiresAt},
		)
		return nil
	},
}

func present(s string) string {
	if s == "" {
		return "absent"
	}
	return "present (hidden)"
}

var supaReposCmd = &cobra.Command{
	Use:   "repos",
	Short: "List provider repos/files/projects (read-only)",
	RunE: func(cmd *cobra.Command, args []string) error {
		_, f, err := supaClient()
		if err != nil {
			return err
		}
		provider := supaProvider
		if provider == "" {
			provider = f.Supa.Provider
		}
		if provider == "" {
			provider = "github"
		}
		slug, err := client.OAuthProvider(provider)
		if err != nil {
			return err
		}
		token := f.Supa.ProviderToken
		if token == "" {
			return fmt.Errorf("no provider token in this session — cyb supa login (grant provider scopes)")
		}
		switch slug {
		case "github":
			repos, err := supa.GitHubRepos(token)
			if err != nil {
				return err
			}
			var rows [][]string
			for _, r := range repos {
				rows = append(rows, []string{r.FullName, r.DefaultBr, fmt.Sprint(r.Private), truncate(r.Description, 48)})
			}
			if len(rows) == 0 {
				ui.Info("no repos visible to this token")
				return nil
			}
			ui.Table([]string{"REPO", "BRANCH", "PRIVATE", "DESCRIPTION"}, rows)
		case "google":
			files, err := supa.DriveFiles(token)
			if err != nil {
				return err
			}
			var rows [][]string
			for _, d := range files {
				rows = append(rows, []string{d.ID, d.Name, d.MimeType, d.Size})
			}
			if len(rows) == 0 {
				ui.Info("no drive files visible to this token")
				return nil
			}
			ui.Table([]string{"ID", "NAME", "MIME", "SIZE"}, rows)
		case "gitlab":
			projs, err := supa.GitLabProjects(supaInstance, token)
			if err != nil {
				return err
			}
			var rows [][]string
			for _, p := range projs {
				rows = append(rows, []string{fmt.Sprint(p.ID), p.PathWithNamespace, p.Visibility})
			}
			if len(rows) == 0 {
				ui.Info("no projects visible to this token")
				return nil
			}
			ui.Table([]string{"ID", "PROJECT", "VISIBILITY"}, rows)
		}
		return nil
	},
}

func init() {
	supaLoginCmd.Flags().StringVar(&supaProvider, "provider", "github", "github|google|gitlab")
	supaLoginCmd.Flags().IntVar(&supaPort, "port", 54329, "loopback callback port (allowlist this URL)")
	supaReposCmd.Flags().StringVar(&supaProvider, "provider", "", "github|google|gitlab (default: login provider)")
	supaReposCmd.Flags().StringVar(&supaInstance, "instance", "https://gitlab.com", "GitLab instance")
	supaCmd.AddCommand(supaLoginCmd, supaLogoutCmd, supaStatusCmd, supaReposCmd)
	rootCmd.AddCommand(supaCmd)
}
