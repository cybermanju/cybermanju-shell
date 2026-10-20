// CyberManju OS — Client (cli/internal/client/client.go).
// Package client speaks the CyberManju OS dashboard REST API
// (the same surface the Docker/web dashboard serves on :3456).
// The wire is camelCase JSON; errors carry the AGENT-1 machine prefixes
// (auth: / rate_limited: / not_found: / unsupported: / too_large: /
// integrity: / network: / disk_full: / conflict:) which map to CLI hints.
package client

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// Client is a dashboard connection.
type Client struct {
	BaseURL string
	Token   string
	HTTP    *http.Client
}

// New builds a client for server (bare host accepted) with token.
func New(server, token string, timeout time.Duration) *Client {
	if timeout <= 0 {
		timeout = 15 * time.Second
	}
	return &Client{
		BaseURL: strings.TrimRight(strings.TrimSpace(server), "/"),
		Token:   strings.TrimSpace(token),
		HTTP:    &http.Client{Timeout: timeout},
	}
}

// APIError is a classified server failure with a human hint.
type APIError struct {
	Status  int
	Message string
	Hint    string
}

func (e *APIError) Error() string {
	if e.Hint != "" {
		return fmt.Sprintf("%s — %s", e.Message, e.Hint)
	}
	return e.Message
}

// HintFor maps an AGENT-1 prefix / HTTP status to CLI guidance.
func HintFor(message string, status int) string {
	switch {
	case strings.HasPrefix(message, "auth:"):
		return "check credentials (`cyb login`) and provider tokens"
	case strings.HasPrefix(message, "rate_limited:"):
		return "back off and retry in a bit"
	case strings.HasPrefix(message, "not_found:"):
		return "check the id / path and retry"
	case strings.HasPrefix(message, "unsupported:"):
		return "needs a dashboard/server feature this build lacks"
	case strings.HasPrefix(message, "too_large:"):
		return "over the content cap (content API: 1 MiB)"
	case strings.HasPrefix(message, "integrity:"):
		return "hash mismatch — re-fetch and retry"
	case strings.HasPrefix(message, "network:"):
		return "provider network issue — retry, then check the provider status"
	case strings.HasPrefix(message, "disk_full:"):
		return "add capacity (`cyb disk create`) or resize a disk"
	case strings.HasPrefix(message, "conflict:"):
		return "resolve the conflict and retry"
	}
	if status == http.StatusUnauthorized {
		return "unauthorized — run `cyb login` (or `cyb setup` on first run)"
	}
	return ""
}

func failure(status int, body []byte) *APIError {
	msg := strings.TrimSpace(string(body))
	var parsed map[string]any
	if err := json.Unmarshal(body, &parsed); err == nil {
		for _, key := range []string{"message", "error"} {
			if v, ok := parsed[key].(string); ok && strings.TrimSpace(v) != "" {
				msg = strings.TrimSpace(v)
				break
			}
		}
	}
	if msg == "" {
		msg = fmt.Sprintf("HTTP %d", status)
	}
	return &APIError{Status: status, Message: msg, Hint: HintFor(msg, status)}
}

// Raw performs the request and returns the raw body (2xx only).
func (c *Client) Raw(method, path string, body any) ([]byte, error) {
	var reader io.Reader
	if body != nil {
		raw, err := json.Marshal(body)
		if err != nil {
			return nil, err
		}
		reader = bytes.NewReader(raw)
	}
	req, err := http.NewRequest(method, c.BaseURL+path, reader)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")
	if c.Token != "" {
		req.Header.Set("Authorization", "Bearer "+c.Token)
	}
	res, err := c.HTTP.Do(req)
	if err != nil {
		return nil, &APIError{Message: fmt.Sprintf("network: cannot reach %s%s: %v", c.BaseURL, path, err), Hint: HintFor("network:", 0)}
	}
	defer res.Body.Close()
	data, err := io.ReadAll(io.LimitReader(res.Body, 32<<20))
	if err != nil {
		return nil, err
	}
	if res.StatusCode < 200 || res.StatusCode >= 300 {
		return nil, failure(res.StatusCode, data)
	}
	return data, nil
}

// Do performs the request and decodes the JSON body into out.
func (c *Client) Do(method, path string, body, out any) error {
	data, err := c.Raw(method, path, body)
	if err != nil {
		return err
	}
	if out == nil {
		return nil
	}
	if len(bytes.TrimSpace(data)) == 0 {
		return nil
	}
	if err := json.Unmarshal(data, out); err != nil {
		return fmt.Errorf("decode %s %s: %w", method, path, err)
	}
	return nil
}

// DecodeList tolerates both bare arrays and {files|items|rows:[...]} wraps.
func DecodeList[T any](data []byte) ([]T, error) {
	var bare []T
	if err := json.Unmarshal(data, &bare); err == nil {
		return bare, nil
	}
	var wrapped map[string]json.RawMessage
	if err := json.Unmarshal(data, &wrapped); err != nil {
		return nil, fmt.Errorf("decode list: %s", strings.TrimSpace(string(data)))
	}
	for _, key := range []string{"files", "items", "rows", "configs", "disks", "sessions", "jobs"} {
		if raw, ok := wrapped[key]; ok {
			var list []T
			if err := json.Unmarshal(raw, &list); err == nil {
				return list, nil
			}
		}
	}
	return nil, fmt.Errorf("decode list: unexpected shape: %s", strings.TrimSpace(string(data)))
}

// ── auth ──────────────────────────────────────────────────────────

// AuthStatus is GET /api/auth/status.
type AuthStatus struct {
	RegistrationOpen bool `json:"registrationOpen"`
}

// LoginResp is POST /api/users/login.
type LoginResp struct {
	UserID      string `json:"userId"`
	Username    string `json:"username"`
	Role        string `json:"role"`
	DisplayName string `json:"displayName"`
	Token       string `json:"token"`
	TokenType   string `json:"tokenType"`
	ExpiresIn   int64  `json:"expiresIn"`
}

func (c *Client) AuthStatus() (AuthStatus, error) {
	var out AuthStatus
	err := c.Do(http.MethodGet, "/api/auth/status", nil, &out)
	return out, err
}

func (c *Client) Register(username, password, displayName, role string) error {
	return c.Do(http.MethodPost, "/api/users/register", map[string]any{
		"username": username, "password": password,
		"displayName": displayName, "role": role,
	}, nil)
}

func (c *Client) Login(username, password string) (LoginResp, error) {
	var out LoginResp
	err := c.Do(http.MethodPost, "/api/users/login", map[string]any{
		"username": username, "password": password,
	}, &out)
	return out, err
}

func (c *Client) Logout() error {
	return c.Do(http.MethodPost, "/api/auth/logout", map[string]any{}, nil)
}

// ── files ─────────────────────────────────────────────────────────

// FileNode is the vault file row (camelCase).
type FileNode struct {
	ID         string   `json:"id"`
	Name       string   `json:"name"`
	FileType   string   `json:"fileType"`
	ParentID   *string  `json:"parentId"`
	SizeBytes  uint64   `json:"sizeBytes"`
	Tags       []string `json:"tags"`
	Encrypted  bool     `json:"encrypted"`
	CreatedAt  string   `json:"createdAt"`
	ModifiedAt string   `json:"modifiedAt"`
}

func (c *Client) Files() ([]FileNode, error) {
	data, err := c.Raw(http.MethodGet, "/api/files", nil)
	if err != nil {
		return nil, err
	}
	return DecodeList[FileNode](data)
}

func (c *Client) File(id string) (FileNode, error) {
	var out FileNode
	err := c.Do(http.MethodGet, "/api/files/"+id, nil, &out)
	return out, err
}

func (c *Client) Mkdir(name, parentID string) (FileNode, error) {
	var out FileNode
	err := c.Do(http.MethodPost, "/api/files/folder", map[string]any{
		"name": name, "parentId": parentID,
	}, &out)
	return out, err
}

func (c *Client) Rename(id, newName string) error {
	return c.Do(http.MethodPost, "/api/files/"+id+"/rename", map[string]any{
		"newName": newName,
	}, nil)
}

func (c *Client) TrashFile(id string) error {
	return c.Do(http.MethodDelete, "/api/files/"+id, nil, nil)
}

// TrashItem is a best-effort trashed-file row (unknown fields ignored).
type TrashItem struct {
	ID   string `json:"id"`
	Name string `json:"name"`
}

func (c *Client) TrashList() ([]TrashItem, error) {
	data, err := c.Raw(http.MethodGet, "/api/trash", nil)
	if err != nil {
		return nil, err
	}
	return DecodeList[TrashItem](data)
}

func (c *Client) TrashRestore(id string) error {
	return c.Do(http.MethodPost, "/api/trash/"+id+"/restore", map[string]any{}, nil)
}

func (c *Client) TrashEmpty() error {
	return c.Do(http.MethodDelete, "/api/trash", nil, nil)
}

// ── sync ──────────────────────────────────────────────────────────

// SyncConfig is the provider config row (token never serialized to clients).
type SyncConfig struct {
	ID          string  `json:"id"`
	BackendType string  `json:"backendType"`
	Enabled     bool    `json:"enabled"`
	Name        *string `json:"name"`
	BasePath    *string `json:"basePath"`
	RepoName    *string `json:"repoName"`
	Branch      *string `json:"branch"`
	FolderID    *string `json:"folderId"`
	AutoSync    bool    `json:"autoSync"`
}

func strptr(s string) *string {
	if s == "" {
		return nil
	}
	return &s
}

// NewSyncConfig builds a create payload for backend with the common knobs.
func NewSyncConfig(backend, name, basePath, repo, branch, folder, token string) map[string]any {
	return map[string]any{
		"id": "", "backendType": backend, "enabled": true,
		"name": strptr(name), "basePath": strptr(basePath),
		"repoName": strptr(repo), "branch": strptr(branch),
		"folderId": strptr(folder), "token": strptr(token),
		"autoSync": false, "compressBeforeUpload": true,
		"createPreviews": false, "deleteRawAfterSync": false,
		"maxConcurrentUploads": 4, "encryptBeforeUpload": true,
	}
}

func (c *Client) SyncConfigs() ([]SyncConfig, error) {
	data, err := c.Raw(http.MethodGet, "/api/sync/configs", nil)
	if err != nil {
		return nil, err
	}
	return DecodeList[SyncConfig](data)
}

func (c *Client) SyncCreate(cfg map[string]any) (SyncConfig, error) {
	var out SyncConfig
	err := c.Do(http.MethodPost, "/api/sync/configs", map[string]any{"config": cfg}, &out)
	return out, err
}

func (c *Client) SyncDelete(id string) error {
	return c.Do(http.MethodDelete, "/api/sync/configs/"+id, nil, nil)
}

// StartJobResp is POST /api/sync/start → 202.
type StartJobResp struct {
	JobID string `json:"jobId"`
}

func (c *Client) SyncStart(configID string, fileIDs []string) (StartJobResp, error) {
	var out StartJobResp
	if fileIDs == nil {
		fileIDs = []string{}
	}
	err := c.Do(http.MethodPost, "/api/sync/start", map[string]any{
		"configId": configID, "fileIds": fileIDs,
	}, &out)
	if err == nil && out.JobID == "" {
		return out, fmt.Errorf("sync start returned no job id")
	}
	return out, err
}

func (c *Client) SyncCancel() error {
	return c.Do(http.MethodPost, "/api/sync/cancel", map[string]any{}, nil)
}

// SyncJob is a best-effort job snapshot (unknown fields ignored).
type SyncJob struct {
	JobID    string         `json:"jobId"`
	ConfigID string         `json:"configId"`
	Status   string         `json:"status"`
	Progress map[string]any `json:"progress"`
	Error    *string        `json:"error"`
}

func (c *Client) SyncJob(id string) (SyncJob, error) {
	var out SyncJob
	err := c.Do(http.MethodGet, "/api/sync/jobs/"+id, nil, &out)
	return out, err
}

// SyncMoveResp is POST /api/sync/move.
type SyncMoveResp struct {
	FileID       string `json:"fileId"`
	FromConfigID string `json:"fromConfigId"`
	ToConfigID   string `json:"toConfigId"`
	RemotePath   string `json:"remotePath"`
	Bytes        uint64 `json:"bytes"`
	Noop         bool   `json:"noop"`
}

func (c *Client) SyncMove(fileID, from, to string) (SyncMoveResp, error) {
	var out SyncMoveResp
	err := c.Do(http.MethodPost, "/api/sync/move", map[string]any{
		"fileId": fileID, "fromConfigId": from, "toConfigId": to,
	}, &out)
	return out, err
}

// SyncTestResp is POST /api/sync/test (shape-tolerant).
type SyncTestResp struct {
	OK      bool   `json:"ok"`
	Message string `json:"message"`
}

func (c *Client) SyncTest(cfg SyncConfig) (SyncTestResp, error) {
	return decodeSyncTest(c, cfg)
}

// decodeSyncTest tolerates bool | {ok,message} | string shapes.
func decodeSyncTest(c *Client, cfg any) (SyncTestResp, error) {
	var out SyncTestResp
	data, err := c.Raw(http.MethodPost, "/api/sync/test", map[string]any{"config": cfg})
	if err != nil {
		return out, err
	}
	var b bool
	if jerr := json.Unmarshal(data, &b); jerr == nil {
		out.OK = b
		return out, nil
	}
	if jerr := json.Unmarshal(data, &out); jerr != nil {
		var s string
		if serr := json.Unmarshal(data, &s); serr == nil {
			out.Message = s
			return out, nil
		}
		return out, fmt.Errorf("decode sync test: %s", strings.TrimSpace(string(data)))
	}
	if out.Message == "" {
		out.Message = strings.TrimSpace(string(data))
	}
	return out, nil
}

// Raw endpoints with evolving shapes — callers decode or --json them.
func (c *Client) SyncStatusRaw() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/sync/status", nil)
}

func (c *Client) SyncProgressRaw() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/sync/progress", nil)
}

func (c *Client) SyncRemoteFiles(cfg SyncConfig, prefix string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/sync/remote-files", map[string]any{
		"config": cfg, "prefix": prefix,
	})
}

func (c *Client) SyncUsage(id string) ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/sync/usage/"+id, nil)
}

// ── disks ─────────────────────────────────────────────────────────

// DiskRow is the catalog disk row (camelCase).
type DiskRow struct {
	ID            string `json:"id"`
	Name          string `json:"name"`
	Provider      string `json:"provider"`
	ConfigID      string `json:"configId"`
	VolumeUUID    string `json:"volumeUuid"`
	CapacityBytes uint64 `json:"capacityBytes"`
	BlockSize     uint32 `json:"blockSize"`
	UsedBytes     uint64 `json:"usedBytes"`
	State         string `json:"state"`
	Health        string `json:"health"`
	ContainerPath string `json:"containerPath"`
	HoldsKeys     bool   `json:"holdsKeys"`
	CreatedAt     string `json:"createdAt"`
	UpdatedAt     string `json:"updatedAt"`
}

// DiskCheck is POST /api/disk/check.
type DiskCheck struct {
	DiskID            string   `json:"diskId"`
	Blocks            int      `json:"blocks"`
	ContainerBlocks   int      `json:"containerBlocks"`
	Orphans           int      `json:"orphans"`
	PendingCheckpoint int      `json:"pendingCheckpoint"`
	Locked            bool     `json:"locked"`
	OK                bool     `json:"ok"`
	Problems          []string `json:"problems"`
}

// DiskUsage is one attached disk inside VolumeDF.
type DiskUsage struct {
	ID       string `json:"id"`
	Provider string `json:"provider"`
	Size     uint64 `json:"size"`
	Used     uint64 `json:"used"`
	Free     uint64 `json:"free"`
	Health   string `json:"health"`
}

// VolumeDF is GET /api/volume/df (attached disks only).
type VolumeDF struct {
	TotalBytes uint64      `json:"totalBytes"`
	UsedBytes  uint64      `json:"usedBytes"`
	FreeBytes  uint64      `json:"freeBytes"`
	DiskCount  int         `json:"diskCount"`
	Disks      []DiskUsage `json:"disks"`
}

func (c *Client) Disks() ([]DiskRow, error) {
	data, err := c.Raw(http.MethodGet, "/api/disk/list", nil)
	if err != nil {
		return nil, err
	}
	return DecodeList[DiskRow](data)
}

func (c *Client) Disk(id string) (DiskRow, error) {
	var out DiskRow
	err := c.Do(http.MethodGet, "/api/disk/"+id, nil, &out)
	return out, err
}

func (c *Client) DiskCreate(configID string, sizeBytes uint64, passphrase, containerPath string) (DiskRow, error) {
	var out DiskRow
	req := map[string]any{
		"configId": configID, "sizeBytes": sizeBytes,
		"passphrase": passphrase,
	}
	// The server defaults absent fields; an explicit "" would win over the
	// default and fail (e.g. "cannot write ''").
	if strings.TrimSpace(containerPath) != "" {
		req["containerPath"] = containerPath
	}
	err := c.Do(http.MethodPost, "/api/disk/create", req, &out)
	return out, err
}

func (c *Client) DiskAttach(id, passphrase string) error {
	return c.Do(http.MethodPost, "/api/disk/attach", map[string]any{
		"id": id, "passphrase": passphrase,
	}, nil)
}

func (c *Client) DiskDetach(id string) error {
	return c.Do(http.MethodPost, "/api/disk/detach", map[string]any{"id": id}, nil)
}

func (c *Client) DiskResize(id string, sizeBytes uint64) error {
	return c.Do(http.MethodPost, "/api/disk/resize", map[string]any{
		"id": id, "sizeBytes": sizeBytes,
	}, nil)
}

func (c *Client) DiskDestroy(id string) error {
	return c.Do(http.MethodPost, "/api/disk/destroy", map[string]any{"id": id}, nil)
}

func (c *Client) DiskCheck(id string) (DiskCheck, error) {
	var out DiskCheck
	err := c.Do(http.MethodPost, "/api/disk/check", map[string]any{"id": id}, &out)
	return out, err
}

func (c *Client) DiskKeyHolder(id string) error {
	return c.Do(http.MethodPost, "/api/disk/key-holder", map[string]any{"id": id}, nil)
}

func (c *Client) VolumeDF() (VolumeDF, error) {
	var out VolumeDF
	err := c.Do(http.MethodGet, "/api/volume/df", nil, &out)
	return out, err
}

// ── oauth ─────────────────────────────────────────────────────────

// OAuthStart is GET /api/sync/oauth/{provider}/start.
type OAuthStart struct {
	AuthorizeURL string `json:"authorizeUrl"`
	State        string `json:"state"`
	ExpiresIn    uint64 `json:"expiresIn"`
}

// OAuthProvider maps CLI-facing names to server slugs.
func OAuthProvider(name string) (string, error) {
	switch strings.ToLower(strings.TrimSpace(name)) {
	case "github":
		return "github", nil
	case "google", "googledrive", "google-drive", "drive":
		return "google", nil
	case "gitlab":
		return "gitlab", nil
	}
	return "", fmt.Errorf("unsupported: unknown OAuth provider %q (github|google|gitlab)", name)
}

func (c *Client) OAuthStart(provider, configID string) (OAuthStart, error) {
	var out OAuthStart
	err := c.Do(http.MethodGet, "/api/sync/oauth/"+provider+"/start?configId="+configID, nil, &out)
	return out, err
}

// ── cybsh ─────────────────────────────────────────────────────────

// ExecResp is POST /api/os/exec (HTTP 200 even on shell error).
type ExecResp struct {
	OK     bool   `json:"ok"`
	Line   string `json:"line"`
	Output string `json:"output"`
	Error  string `json:"error"`
	Prompt string `json:"prompt"`
}

func (c *Client) Exec(line string) (ExecResp, error) {
	var out ExecResp
	err := c.Do(http.MethodPost, "/api/os/exec", map[string]any{"line": line}, &out)
	return out, err
}

// ── ai agent ───────────────────────────────────────────────────────

// AgentConfigView is the config row (keys never come back, only hasKey).
type AgentConfigView struct {
	ID         string `json:"id"`
	Name       string `json:"name"`
	ProviderID string `json:"providerId"`
	Model      string `json:"model"`
	HasKey     bool   `json:"hasKey"`
}

func (c *Client) AgentConfigs() ([]AgentConfigView, error) {
	data, err := c.Raw(http.MethodGet, "/api/agent/configs", nil)
	if err != nil {
		return nil, err
	}
	return DecodeList[AgentConfigView](data)
}

func (c *Client) AgentProvidersRaw() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/agent/providers", nil)
}

func (c *Client) AgentSessionsRaw() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/agent/sessions", nil)
}

func (c *Client) AgentSessionCreate(configID, title string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/agent/sessions", map[string]any{
		"configId": configID, "title": title,
	})
}

func (c *Client) AgentSessionDelete(id string) error {
	return c.Do(http.MethodDelete, "/api/agent/sessions/"+id, nil, nil)
}

// TokenUsage counts a job's spend.
type TokenUsage struct {
	InputTokens  int `json:"inputTokens"`
	OutputTokens int `json:"outputTokens"`
}

// PendingApproval parks an ask for ≤10 min, then auto-denies.
type PendingApproval struct {
	Tool    string `json:"tool"`
	Summary string `json:"summary"`
	Input   any    `json:"input"`
}

// JobSnapshot is POST /api/agent/prompt → 202 and GET /api/agent/jobs/{id}.
type JobSnapshot struct {
	JobID     string           `json:"jobId"`
	SessionID string           `json:"sessionId"`
	ConfigID  string           `json:"configId"`
	Status    string           `json:"status"`
	TurnsUsed int              `json:"turnsUsed"`
	MaxTurns  int              `json:"maxTurns"`
	Usage     TokenUsage       `json:"usage"`
	Result    any              `json:"result"`
	Error     *string          `json:"error"`
	Pending   *PendingApproval `json:"pending"`
}

// Terminal reports whether the job will not move again.
func (j JobSnapshot) Terminal() bool {
	switch strings.ToLower(j.Status) {
	case "completed", "failed", "error", "cancelled", "aborted", "denied", "timeout":
		return true
	}
	return false
}

func (c *Client) AgentPrompt(configID, sessionID, prompt string) (JobSnapshot, error) {
	var out JobSnapshot
	err := c.Do(http.MethodPost, "/api/agent/prompt", map[string]any{
		"configId": configID, "sessionId": sessionID, "prompt": prompt,
	}, &out)
	return out, err
}

func (c *Client) AgentJob(id string) (JobSnapshot, error) {
	var out JobSnapshot
	err := c.Do(http.MethodGet, "/api/agent/jobs/"+id, nil, &out)
	return out, err
}

func (c *Client) AgentAbort(id string) error {
	return c.Do(http.MethodPost, "/api/agent/jobs/"+id+"/abort", map[string]any{}, nil)
}

func (c *Client) AgentApprove(id, answer string, approved, remember bool) error {
	return c.Do(http.MethodPost, "/api/agent/jobs/"+id+"/approve", map[string]any{
		"approved": approved, "answer": answer, "remember": remember,
	}, nil)
}
