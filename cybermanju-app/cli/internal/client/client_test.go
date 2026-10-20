// CyberManju OS — Client test (cli/internal/client/client_test.go).
// Role: CLI REST client.
// Exports: TestAuthHeaderAndCamelDecode, TestErrorMapping, TestUnauthorizedHint, TestOAuthStartDecode, TestExecPassthrough, TestOAuthProviderSlugs, TestJobTerminal, TestJSONField.
// Integrates: encoding, net, strings, testing, time, id.
// Contracts: none special (pure logic; errors bubble to caller).
package client

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func testServer(t *testing.T, mux *http.ServeMux) (*Client, func()) {
	t.Helper()
	srv := httptest.NewServer(mux)
	return New(srv.URL, "sekret", time.Second*5), srv.Close
}

func TestAuthHeaderAndCamelDecode(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("/api/files", func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer sekret" {
			w.WriteHeader(http.StatusUnauthorized)
			_ = json.NewEncoder(w).Encode(map[string]string{"message": "auth: missing"})
			return
		}
		_ = json.NewEncoder(w).Encode([]map[string]any{{
			"id": "f1", "name": "vault", "fileType": "folder",
			"sizeBytes": 0, "encrypted": false, "tags": []string{},
		}})
	})
	c, done := testServer(t, mux)
	defer done()
	nodes, err := c.Files()
	if err != nil {
		t.Fatal(err)
	}
	if len(nodes) != 1 || nodes[0].ID != "f1" || nodes[0].FileType != "folder" {
		t.Fatalf("bad decode: %+v", nodes)
	}
}

func TestErrorMapping(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("/api/disk/detach", func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusConflict)
		_, _ = w.Write([]byte(`{"message":"conflict: disk busy in volume"}`))
	})
	c, done := testServer(t, mux)
	defer done()
	err := c.DiskDetach("d1")
	apiErr, ok := err.(*APIError)
	if !ok {
		t.Fatalf("expected *APIError, got %T (%v)", err, err)
	}
	if apiErr.Status != 409 || !strings.Contains(apiErr.Hint, "conflict") {
		t.Fatalf("bad mapping: %+v", apiErr)
	}
}

func TestUnauthorizedHint(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("/api/sync/configs", func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
		_, _ = w.Write([]byte(`HTTP 401 Unauthorized`))
	})
	c, done := testServer(t, mux)
	defer done()
	c.Token = ""
	_, err := c.SyncConfigs()
	if err == nil || !strings.Contains(err.Error(), "cyb login") {
		t.Fatalf("expected login hint, got %v", err)
	}
}

func TestOAuthStartDecode(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("/api/sync/oauth/google/start", func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Query().Get("configId") != "c1" {
			t.Errorf("configId not forwarded: %s", r.URL.RawQuery)
		}
		_ = json.NewEncoder(w).Encode(map[string]any{
			"authorizeUrl": "https://accounts.google.com/o/oauth2/auth?x=1",
			"state":        "s", "expiresIn": 600,
		})
	})
	c, done := testServer(t, mux)
	defer done()
	got, err := c.OAuthStart("google", "c1")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(got.AuthorizeURL, "https://") || got.ExpiresIn != 600 {
		t.Fatalf("bad decode: %+v", got)
	}
}

func TestExecPassthrough(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("/api/os/exec", func(w http.ResponseWriter, r *http.Request) {
		var body map[string]string
		_ = json.NewDecoder(r.Body).Decode(&body)
		_ = json.NewEncoder(w).Encode(map[string]any{
			"ok": true, "line": body["line"], "output": "42", "prompt": "cybsh> ",
		})
	})
	c, done := testServer(t, mux)
	defer done()
	res, err := c.Exec("echo 42")
	if err != nil || !res.OK || res.Output != "42" {
		t.Fatalf("bad exec: %+v %v", res, err)
	}
}

func TestOAuthProviderSlugs(t *testing.T) {
	for in, want := range map[string]string{
		"github": "github", "google": "google", "drive": "google",
		"googleDrive": "google", "gitlab": "gitlab",
	} {
		got, err := OAuthProvider(in)
		if err != nil || got != want {
			t.Fatalf("OAuthProvider(%q) = %q, %v", in, got, err)
		}
	}
	if _, err := OAuthProvider("dropbox"); err == nil {
		t.Fatal("expected error for dropbox")
	}
}

func TestJobTerminal(t *testing.T) {
	for _, s := range []string{"completed", "failed", "error", "cancelled", "aborted", "denied", "timeout"} {
		if !(JobSnapshot{Status: s}.Terminal()) {
			t.Fatalf("%s should be terminal", s)
		}
	}
	for _, s := range []string{"running", "pending", "awaitingApproval", ""} {
		if (JobSnapshot{Status: s}.Terminal()) {
			t.Fatalf("%s should be active", s)
		}
	}
}

func TestJSONField(t *testing.T) {
	raw := []byte(`{"id":"abc-123","name":"x"}`)
	if got := JSONField(raw, "id"); got != "abc-123" {
		t.Fatalf("got %q", got)
	}
	if got := JSONField([]byte(`{}`), "id"); got != "" {
		t.Fatalf("got %q", got)
	}
}

func TestDecodeListWrapped(t *testing.T) {
	raw := []byte(`{"files":[{"id":"a"},{"id":"b"}]}`)
	type item struct {
		ID string `json:"id"`
	}
	list, err := DecodeList[item](raw)
	if err != nil || len(list) != 2 || list[1].ID != "b" {
		t.Fatalf("got %+v %v", list, err)
	}
}
