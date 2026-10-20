// CyberManju OS — Supa (cli/internal/supa/supa.go).
// Package supa speaks Supabase Auth + provider read APIs directly — the
// limited, dashboard-less backend. READ-ONLY by construction: every
// provider call here is a GET. Encrypted vault blobs stay opaque in this
// mode (no decrypt/encrypt); full power needs a dashboard backend.
package supa

import (
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// Client is a Supabase project connection (URL + anon key, public by design).
type Client struct {
	URL  string
	Key  string
	HTTP *http.Client
}

// New builds a client (10s timeout default).
func New(rawURL, key string) *Client {
	return &Client{
		URL:  strings.TrimRight(strings.TrimSpace(rawURL), "/"),
		Key:  strings.TrimSpace(key),
		HTTP: &http.Client{Timeout: 30 * time.Second},
	}
}

// BakedURL/BakedKey are stamped by CI ldflags from the repo secrets
// (VITE_SUPABASE_URL / VITE_SUPABASE_ANON_KEY) — the same broker every
// desktop and Android CLI build bakes. Empty in dev builds; Settings paste applies.
var BakedURL, BakedKey string

// BrokerPair resolves the broker: explicit flags > baked pair > "".
// Source reports where the pair came from for honest UI.
func BrokerPair(flagURL, flagKey string) (url, key, source string) {
	if flagURL != "" && flagKey != "" {
		return flagURL, flagKey, "flags"
	}
	if BakedURL != "" && BakedKey != "" {
		return BakedURL, BakedKey, "baked into this build"
	}
	return "", "", "none"
}

// Session is a Supabase Auth session (provider tokens included when the
// broker returns them for the OAuth grant).
type Session struct {
	AccessToken     string `json:"access_token"`
	RefreshToken    string `json:"refresh_token"`
	ExpiresIn       int64  `json:"expires_in"`
	UserID          string
	Email           string
	ProviderToken   string `json:"provider_token"`
	ProviderRefresh string `json:"provider_refresh_token"`
}

// NewVerifier mints a PKCE code_verifier (43–128 URL-safe chars).
func NewVerifier() (string, error) {
	raw := make([]byte, 48)
	if _, err := rand.Read(raw); err != nil {
		return "", err
	}
	return base64.RawURLEncoding.EncodeToString(raw), nil
}

// Challenge derives the S256 code_challenge (RFC 7636).
func Challenge(verifier string) string {
	sum := sha256.Sum256([]byte(verifier))
	return base64.RawURLEncoding.EncodeToString(sum[:])
}

// AuthorizeURL builds the browser URL for a provider grant.
func (c *Client) AuthorizeURL(provider, redirectTo, challenge string) string {
	q := url.Values{}
	q.Set("provider", provider)
	q.Set("redirect_to", redirectTo)
	q.Set("code_challenge", challenge)
	q.Set("code_challenge_method", "s256")
	return c.URL + "/auth/v1/authorize?" + q.Encode()
}

// RedirectTo is the loopback callback the user allowlists once in the
// Supabase dashboard (Authentication → URL Configuration → Redirect URLs).
func RedirectTo(port int) string {
	return fmt.Sprintf("http://127.0.0.1:%d/callback", port)
}

// WaitForCode serves one loopback callback and returns the auth code
// (or the provider's error). Times out after timeout.
func WaitForCode(port int, timeout time.Duration) (string, error) {
	type result struct {
		code string
		err  error
	}
	out := make(chan result, 1)
	mux := http.NewServeMux()
	srv := &http.Server{Addr: fmt.Sprintf("127.0.0.1:%d", port), Handler: mux}
	mux.HandleFunc("/callback", func(w http.ResponseWriter, r *http.Request) {
		q := r.URL.Query()
		if e := q.Get("error_description"); e != "" {
			out <- result{"", fmt.Errorf("%s", e)}
		} else if e := q.Get("error"); e != "" {
			out <- result{"", fmt.Errorf("provider refused: %s", e)}
		} else if code := q.Get("code"); code != "" {
			out <- result{code, nil}
		} else {
			out <- result{"", fmt.Errorf("callback carried no code")}
		}
		_, _ = w.Write([]byte("<h1>Signed in — return to your terminal.</h1>"))
	})
	go func() {
		ln, err := net.Listen("tcp", srv.Addr)
		if err != nil {
			out <- result{"", fmt.Errorf("loopback %s busy — is another login running? (%v)", srv.Addr, err)}
			return
		}
		_ = srv.Serve(ln)
	}()
	select {
	case r := <-out:
		_ = srv.Close()
		return r.code, r.err
	case <-time.After(timeout):
		_ = srv.Close()
		return "", fmt.Errorf("timed out waiting for the browser callback")
	}
}

func (c *Client) doAuth(method, path string, body any, out any) error {
	var reader io.Reader
	if body != nil {
		raw, err := json.Marshal(body)
		if err != nil {
			return err
		}
		reader = strings.NewReader(string(raw))
	}
	req, err := http.NewRequest(method, c.URL+path, reader)
	if err != nil {
		return err
	}
	req.Header.Set("apikey", c.Key)
	req.Header.Set("Content-Type", "application/json")
	res, err := c.HTTP.Do(req)
	if err != nil {
		return fmt.Errorf("network: cannot reach %s: %w", c.URL, err)
	}
	defer res.Body.Close()
	data, err := io.ReadAll(io.LimitReader(res.Body, 1<<20))
	if err != nil {
		return err
	}
	if res.StatusCode < 200 || res.StatusCode >= 300 {
		return fmt.Errorf("supabase %d: %s", res.StatusCode, strings.TrimSpace(string(data)))
	}
	if out == nil {
		return nil
	}
	var wrapped struct {
		Session
		User struct {
			ID    string `json:"id"`
			Email string `json:"email"`
		} `json:"user"`
	}
	if err := json.Unmarshal(data, &wrapped); err != nil {
		return fmt.Errorf("decode auth response: %w", err)
	}
	wrapped.Session.UserID = wrapped.User.ID
	wrapped.Session.Email = wrapped.User.Email
	*out.(*Session) = wrapped.Session
	return nil
}

// Exchange swaps a PKCE code for a session.
func (c *Client) Exchange(code, verifier string) (Session, error) {
	var s Session
	err := c.doAuth("POST", "/auth/v1/token?grant_type=pkce", map[string]string{
		"auth_code": code, "code_verifier": verifier,
	}, &s)
	return s, err
}

// Refresh rotates an expired access token.
func (c *Client) Refresh(refreshToken string) (Session, error) {
	var s Session
	err := c.doAuth("POST", "/auth/v1/token?grant_type=refresh_token", map[string]string{
		"refresh_token": refreshToken,
	}, &s)
	return s, err
}

// Me validates the session and returns the user.
func (c *Client) Me(accessToken string) (Session, error) {
	req, err := http.NewRequest("GET", c.URL+"/auth/v1/user", nil)
	if err != nil {
		return Session{}, err
	}
	req.Header.Set("apikey", c.Key)
	req.Header.Set("Authorization", "Bearer "+accessToken)
	res, err := c.HTTP.Do(req)
	if err != nil {
		return Session{}, fmt.Errorf("network: cannot reach %s: %w", c.URL, err)
	}
	defer res.Body.Close()
	data, _ := io.ReadAll(io.LimitReader(res.Body, 1<<20))
	if res.StatusCode != 200 {
		return Session{}, fmt.Errorf("auth: session invalid (%d)", res.StatusCode)
	}
	var u struct {
		ID    string `json:"id"`
		Email string `json:"email"`
	}
	if err := json.Unmarshal(data, &u); err != nil {
		return Session{}, err
	}
	return Session{AccessToken: accessToken, UserID: u.ID, Email: u.Email}, nil
}

// ── read-only provider APIs (GET only) ────────────────────────────

func getJSON(url, token string, out any) error {
	req, err := http.NewRequest("GET", url, nil)
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", "Bearer "+token)
	req.Header.Set("Accept", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		return fmt.Errorf("network: %w", err)
	}
	defer res.Body.Close()
	data, err := io.ReadAll(io.LimitReader(res.Body, 8<<20))
	if err != nil {
		return err
	}
	if res.StatusCode < 200 || res.StatusCode >= 300 {
		return fmt.Errorf("provider %d: %s", res.StatusCode, strings.TrimSpace(string(data)))
	}
	return json.Unmarshal(data, out)
}

// GitHubRepo is a best-effort repository row.
type GitHubRepo struct {
	FullName    string `json:"full_name"`
	Private     bool   `json:"private"`
	DefaultBr   string `json:"default_branch"`
	Description string `json:"description"`
}

// GitHubRepos lists the user's repositories (read-only).
func GitHubRepos(token string) ([]GitHubRepo, error) {
	var out []GitHubRepo
	err := getJSON("https://api.github.com/user/repos?per_page=100&sort=updated", token, &out)
	return out, err
}

// DriveFile is a best-effort Drive row.
type DriveFile struct {
	ID       string `json:"id"`
	Name     string `json:"name"`
	MimeType string `json:"mimeType"`
	Size     string `json:"size"`
}

// DriveFiles lists Drive files visible to the grant (read-only).
func DriveFiles(token string) ([]DriveFile, error) {
	var wrapped struct {
		Files []DriveFile `json:"files"`
	}
	err := getJSON("https://www.googleapis.com/drive/v3/files?pageSize=100&fields=files(id,name,mimeType,size)&orderBy=modifiedTime%20desc", token, &wrapped)
	return wrapped.Files, err
}

// GitLabProject is a best-effort project row.
type GitLabProject struct {
	ID                int64  `json:"id"`
	PathWithNamespace string `json:"path_with_namespace"`
	Visibility        string `json:"visibility"`
}

// GitLabProjects lists member projects (read-only).
func GitLabProjects(instance, token string) ([]GitLabProject, error) {
	var out []GitLabProject
	err := getJSON(strings.TrimRight(instance, "/")+"/api/v4/projects?membership=true&per_page=100&order_by=last_activity_at", token, &out)
	return out, err
}
