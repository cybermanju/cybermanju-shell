// CyberManju OS — Supa test (cli/internal/supa/supa_test.go).
// Role: CLI Supabase helper.
// Exports: TestChallengeRFC7636Vector, TestVerifierShape, TestAuthorizeURL, TestExchangeDecodesProviderToken, TestRedirectTo, TestBrokerPairOrder.
// Integrates: encoding, net, strings, testing, redirect_to=http%3A%2F%2F127.0.0.1%3A54329%2Fcallback, code_challenge=challenge123.
// Contracts: none special (pure logic; errors bubble to caller).
package supa

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestChallengeRFC7636Vector(t *testing.T) {
	// Appendix B of RFC 7636 (cross-checked with an independent SHA-256).
	verifier := "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
	if got := Challenge(verifier); got != "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM" {
		t.Fatalf("bad challenge: %s", got)
	}
}

func TestVerifierShape(t *testing.T) {
	v, err := NewVerifier()
	if err != nil {
		t.Fatal(err)
	}
	if len(v) < 43 || len(v) > 128 {
		t.Fatalf("bad length %d", len(v))
	}
	for _, c := range v {
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '-' || c == '_') {
			t.Fatalf("bad char %q", c)
		}
	}
}

func TestAuthorizeURL(t *testing.T) {
	c := New("https://broker.example.test/", "anon-key")
	u := c.AuthorizeURL("github", "http://127.0.0.1:54329/callback", "challenge123")
	for _, want := range []string{
		"/auth/v1/authorize?", "provider=github",
		"redirect_to=http%3A%2F%2F127.0.0.1%3A54329%2Fcallback",
		"code_challenge=challenge123", "code_challenge_method=s256",
	} {
		if !strings.Contains(u, want) {
			t.Fatalf("missing %q in %s", want, u)
		}
	}
}

func TestExchangeDecodesProviderToken(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("/auth/v1/token", func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("apikey") != "anon-key" {
			w.WriteHeader(http.StatusUnauthorized)
			return
		}
		_ = json.NewEncoder(w).Encode(map[string]any{
			"access_token": "acc", "refresh_token": "ref",
			"expires_in":     3600,
			"user":           map[string]any{"id": "u-1", "email": "me@x.test"},
			"provider_token": "gho_provider",
		})
	})
	srv := httptest.NewServer(mux)
	defer srv.Close()
	c := New(srv.URL, "anon-key")
	s, err := c.Exchange("code", "verifier")
	if err != nil {
		t.Fatal(err)
	}
	if s.AccessToken != "acc" || s.UserID != "u-1" || s.ProviderToken != "gho_provider" {
		t.Fatalf("bad session: %+v", s)
	}
}

func TestRedirectTo(t *testing.T) {
	if got := RedirectTo(54329); got != "http://127.0.0.1:54329/callback" {
		t.Fatalf("got %s", got)
	}
}

func TestBrokerPairOrder(t *testing.T) {
	// Flags win over everything.
	if _, _, src := BrokerPair("u", "k"); src != "flags" {
		t.Fatalf("src=%s", src)
	}
	// Baked pair applies when flags are absent.
	BakedURL, BakedKey = "https://baked.test", "baked-key"
	defer func() { BakedURL, BakedKey = "", "" }()
	u, k, src := BrokerPair("", "")
	if u != "https://baked.test" || k != "baked-key" || src != "baked into this build" {
		t.Fatalf("got %q %q %q", u, k, src)
	}
	// Nothing anywhere.
	BakedURL, BakedKey = "", ""
	if _, _, src := BrokerPair("", ""); src != "none" {
		t.Fatalf("src=%s", src)
	}
}
