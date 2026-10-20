// CyberManju OS — Setup test (cli/cmd/setup_test.go).
// Role: protect platform-specific first-run setup routing.
// Exports: TestSetupTargetChoices, TestShouldAskSetupTarget, TestWithSupabaseBackend.
// Integrates: testing, internal/config.
// Contracts: Termux exposes only supported backend choices; saved profile state is preserved.
package cmd

import (
	"reflect"
	"testing"

	"github.com/cybermanju/cybermanju.github.io/cli/internal/config"
)

func TestSetupTargetChoices(t *testing.T) {
	want := []setupTarget{setupTargetDashboard, setupTargetLimited}
	if got := setupTargetChoices("android"); !reflect.DeepEqual(got, want) {
		t.Fatalf("android setup choices = %v, want %v", got, want)
	}
	if got := setupTargetChoices("linux"); len(got) != 0 {
		t.Fatalf("linux should keep the installer's existing backend menu; got %v", got)
	}
}

func TestShouldAskSetupTarget(t *testing.T) {
	if !shouldAskSetupTarget("android", config.File{}, false, "") {
		t.Fatal("fresh interactive Termux setup must ask for a setup type")
	}
	if shouldAskSetupTarget("linux", config.File{}, false, "") {
		t.Fatal("desktop setup must not duplicate the installer's backend menu")
	}
	if shouldAskSetupTarget("android", config.File{Backend: config.Backend{Type: config.BackendRemote}}, false, "") {
		t.Fatal("an already selected backend must not prompt again")
	}
	if shouldAskSetupTarget("android", config.File{}, true, "") {
		t.Fatal("non-interactive setup must not open a selector")
	}
	if shouldAskSetupTarget("android", config.File{}, false, "https://dashboard.example") {
		t.Fatal("an explicit dashboard URL must imply the dashboard setup path")
	}
}

func TestWithSupabaseBackendPreservesProfile(t *testing.T) {
	original := config.File{
		Server: "https://dashboard.example",
		Token:  "dashboard-session",
		Backend: config.Backend{
			Type: config.BackendRemote,
		},
		Supa: &config.SupaSession{AccessToken: "existing-supa-session"},
	}
	got, err := withSupabaseBackend(original, " https://project.supabase.co/ ", " anon-key ")
	if err != nil {
		t.Fatal(err)
	}
	if got.Backend.Type != config.BackendSupabase {
		t.Fatalf("backend = %q, want %q", got.Backend.Type, config.BackendSupabase)
	}
	if got.Server != original.Server || got.Token != original.Token {
		t.Fatalf("dashboard profile fields were lost: %#v", got)
	}
	if got.Supa == nil || got.Supa.URL != "https://project.supabase.co/" || got.Supa.Key != "anon-key" {
		t.Fatalf("Supabase pair not stored correctly: %#v", got.Supa)
	}
	if got.Supa.AccessToken != "existing-supa-session" {
		t.Fatalf("existing Supabase session was lost: %#v", got.Supa)
	}
}

func TestWithSupabaseBackendRequiresCompletePair(t *testing.T) {
	for _, tc := range []struct{ url, key string }{{"", "anon"}, {"https://project.supabase.co", ""}, {" ", "anon"}} {
		if _, err := withSupabaseBackend(config.File{}, tc.url, tc.key); err == nil {
			t.Fatalf("withSupabaseBackend(%q, %q) should fail", tc.url, tc.key)
		}
	}
}
