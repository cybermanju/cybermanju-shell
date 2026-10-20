// CyberManju OS — Config (cli/internal/config/config.go).
// Package config persists the CLI's connection profile: which dashboard to
// talk to and the JWT minted by `cyb login` / `cyb setup`. The file is
// created 0600 — the token is a bearer credential, never logged.
package config

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// File is the on-disk profile.
type File struct {
	Server    string `json:"server"`
	Token     string `json:"token,omitempty"`
	UpdatedAt string `json:"updatedAt,omitempty"`
	// ExpiresAt is the RFC3339 instant the JWT stops working (empty = unknown).
	ExpiresAt string `json:"expiresAt,omitempty"`
	// Backend selects how cyb reaches a vault (see Backend).
	Backend Backend `json:"backend,omitempty"`
	// Supa holds the Supabase-direct limited-mode session (0600 file).
	Supa *SupaSession `json:"supa,omitempty"`
}

// Backend types: remote dashboard, docker-managed, native app, or
// supabase-direct limited mode (read-only, no dashboard at all).
const (
	BackendRemote   = "remote"
	BackendDocker   = "docker"
	BackendNative   = "native"
	BackendSupabase = "supabase"
)

// Backend selects the vault backend.
type Backend struct {
	// Type is one of remote|docker|native|supabase (empty = remote).
	Type string `json:"type,omitempty"`
	// DockerPort / NativePort override the loopback dashboard port.
	DockerPort int `json:"dockerPort,omitempty"`
	NativePort int `json:"nativePort,omitempty"`
}

// SupaSession is a Supabase Auth session for limited direct mode.
// The provider token is a full OAuth credential: 0600 file, TLS only,
// never logged, cleared by `supa logout`.
type SupaSession struct {
	URL           string `json:"url"`
	Key           string `json:"key"`
	AccessToken   string `json:"accessToken,omitempty"`
	RefreshToken  string `json:"refreshToken,omitempty"`
	ExpiresAt     string `json:"expiresAt,omitempty"`
	UserID        string `json:"userId,omitempty"`
	Email         string `json:"email,omitempty"`
	Provider      string `json:"provider,omitempty"`
	ProviderToken string `json:"providerToken,omitempty"`
}

// TypeOrDefault normalizes an empty backend to remote.
func (b Backend) TypeOrDefault() string {
	if b.Type == "" {
		return BackendRemote
	}
	return b.Type
}

// DashboardPort returns the loopback port for docker/native backends.
func (b Backend) DashboardPort() int {
	switch b.TypeOrDefault() {
	case BackendDocker:
		if b.DockerPort > 0 {
			return b.DockerPort
		}
	case BackendNative:
		if b.NativePort > 0 {
			return b.NativePort
		}
	}
	return 3456
}

// DefaultServer is used when nothing else points at a dashboard.
const DefaultServer = "http://127.0.0.1:3456"

// Dir returns the platform config dir for the CLI.
func Dir() (string, error) {
	if xdg := os.Getenv("XDG_CONFIG_HOME"); xdg != "" {
		return filepath.Join(xdg, "cybermanju"), nil
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, ".config", "cybermanju"), nil
}

// Path returns the profile file path.
func Path() (string, error) {
	dir, err := Dir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "cyb.json"), nil
}

// HistoryPath returns the REPL history file path.
func HistoryPath() (string, error) {
	dir, err := Dir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "history"), nil
}

// Load reads the profile; a missing file is not an error.
func Load() (File, error) {
	var f File
	path, err := Path()
	if err != nil {
		return f, err
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return f, nil
		}
		return f, err
	}
	if err := json.Unmarshal(raw, &f); err != nil {
		return f, fmt.Errorf("parse %s: %w", path, err)
	}
	return f, nil
}

// Save writes the profile with 0600 permissions.
func Save(f File) error {
	path, err := Path()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	f.UpdatedAt = time.Now().UTC().Format(time.RFC3339)
	raw, err := json.MarshalIndent(f, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, append(raw, '\n'), 0o600)
}

// ResolveServer picks the dashboard URL: flag > env > file > default.
func ResolveServer(flag string, f File) string {
	if s := strings.TrimSpace(flag); s != "" {
		return strings.TrimRight(s, "/")
	}
	return resolveStored(f)
}

// ResolveDashboard honors the selected backend: an explicit flag always
// wins; supabase-direct mode has no dashboard (""); docker/native resolve
// to their loopback port; remote falls back to the stored chain.
func ResolveDashboard(flag string, f File) string {
	if s := strings.TrimSpace(flag); s != "" {
		return strings.TrimRight(s, "/")
	}
	switch f.Backend.TypeOrDefault() {
	case BackendSupabase:
		return ""
	case BackendDocker, BackendNative:
		return fmt.Sprintf("http://127.0.0.1:%d", f.Backend.DashboardPort())
	default:
		return resolveStored(f)
	}
}

func resolveStored(f File) string {
	if s := strings.TrimSpace(os.Getenv("CYBERMANJU_SERVER")); s != "" {
		return strings.TrimRight(s, "/")
	}
	if s := strings.TrimSpace(f.Server); s != "" {
		return strings.TrimRight(s, "/")
	}
	return DefaultServer
}

// ResolveToken picks the JWT: flag > env > file (empty = anonymous).
func ResolveToken(flag string, f File) string {
	if t := strings.TrimSpace(flag); t != "" {
		return t
	}
	if t := strings.TrimSpace(os.Getenv("CYBERMANJU_TOKEN")); t != "" {
		return t
	}
	return strings.TrimSpace(f.Token)
}

// Expired reports whether the Supabase access token is past expiry
// (unknown = false; callers refresh on 401).
func (s *SupaSession) Expired() bool {
	if s == nil || strings.TrimSpace(s.AccessToken) == "" || strings.TrimSpace(s.ExpiresAt) == "" {
		return false
	}
	t, err := time.Parse(time.RFC3339, s.ExpiresAt)
	if err != nil {
		return false
	}
	return time.Now().After(t)
}

// Expired reports whether the saved JWT is past its expiry (unknown = false).
func (f File) Expired() bool {
	if strings.TrimSpace(f.Token) == "" || strings.TrimSpace(f.ExpiresAt) == "" {
		return false
	}
	t, err := time.Parse(time.RFC3339, f.ExpiresAt)
	if err != nil {
		return false
	}
	return time.Now().After(t)
}

// WithExpiry stamps expiresIn seconds from now onto the profile.
func (f File) WithExpiry(expiresIn int64) File {
	if expiresIn > 0 {
		f.ExpiresAt = time.Now().Add(time.Duration(expiresIn) * time.Second).UTC().Format(time.RFC3339)
	}
	return f
}
