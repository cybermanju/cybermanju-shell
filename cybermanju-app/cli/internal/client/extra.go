// CyberManju OS — Extra (cli/internal/client/extra.go).
// Role: CLI REST client.
// Exports: GetRetry, SyncConfigRaw, SyncTestMap, SyncRemoteFilesMap, Readyz, HealthRaw, DashboardStatusRaw, Search.
// Integrates: encoding, fmt, net, strings, time, config.
// Contracts: none special (pure logic; errors bubble to caller).
package client

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// This file covers the second API wave: search, secrets, cron, durability,
// groups, shares, audit, batch, versions, content, code, crypto, users,
// permissions, agent memory/MCP/keys, dashboard and sync extras.
// Shapes are decoded minimally (unknown fields ignored); --json prints raw.

// GetRetry performs GET with one backoff retry on transport failure.
// Only GET retries: POST/PUT/DELETE can have side effects.
func (c *Client) GetRetry(path string) ([]byte, error) {
	data, err := c.Raw(http.MethodGet, path, nil)
	if err == nil {
		return data, nil
	}
	if _, ok := err.(*APIError); ok {
		return nil, err
	}
	time.Sleep(400 * time.Millisecond)
	return c.Raw(http.MethodGet, path, nil)
}

// SyncConfigRaw fetches one stored config as an untyped map, preserving
// every server field (including ones this CLI doesn't model) for
// pass-through calls like test / remote-files / upload.
func (c *Client) SyncConfigRaw(id string) (map[string]any, error) {
	data, err := c.Raw(http.MethodGet, "/api/sync/configs", nil)
	if err != nil {
		return nil, err
	}
	var list []map[string]any
	if err := json.Unmarshal(data, &list); err != nil {
		return nil, fmt.Errorf("decode sync configs: %w", err)
	}
	for _, g := range list {
		if gid, _ := g["id"].(string); gid == id {
			return g, nil
		}
	}
	return nil, &APIError{Status: 404, Message: fmt.Sprintf("not_found: no sync config %q", id), Hint: HintFor("not_found:", 404)}
}

func (c *Client) SyncTestMap(cfg map[string]any) (SyncTestResp, error) {
	return decodeSyncTest(c, cfg)
}

func (c *Client) SyncRemoteFilesMap(cfg map[string]any, prefix string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/sync/remote-files", map[string]any{
		"config": cfg, "prefix": prefix,
	})
}

// ── health ────────────────────────────────────────────────────────

func (c *Client) Readyz() error {
	_, err := c.Raw(http.MethodGet, "/api/readyz", nil)
	return err
}

func (c *Client) HealthRaw() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/health", nil)
}

func (c *Client) DashboardStatusRaw() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/dashboard/status", nil)
}

// ── search ────────────────────────────────────────────────────────

func (c *Client) Search(query string, limit, offset int) ([]byte, error) {
	q := url.Values{}
	q.Set("q", query)
	if limit > 0 {
		q.Set("limit", fmt.Sprint(limit))
	}
	if offset > 0 {
		q.Set("offset", fmt.Sprint(offset))
	}
	return c.Raw(http.MethodGet, "/api/search?"+q.Encode(), nil)
}

func (c *Client) Suggest(prefix string, limit int) ([]byte, error) {
	q := url.Values{}
	q.Set("q", prefix)
	if limit > 0 {
		q.Set("limit", fmt.Sprint(limit))
	}
	return c.Raw(http.MethodGet, "/api/search/suggest?"+q.Encode(), nil)
}

func (c *Client) GeoFiles() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/geo-files", nil)
}

// SearchHit is a best-effort BM25 hit.
type SearchHit struct {
	FileID   string  `json:"fileId"`
	FileName string  `json:"fileName"`
	Score    float64 `json:"score"`
	Snippet  string  `json:"snippet"`
}

// ── secrets keystore ──────────────────────────────────────────────

// SecretMeta is the list-safe row (values never listed).
type SecretMeta struct {
	ID       string   `json:"id"`
	Kind     string   `json:"kind"`
	Title    string   `json:"title"`
	Username *string  `json:"username"`
	URL      *string  `json:"url"`
	Category *string  `json:"category"`
	Tags     []string `json:"tags"`
	Favorite bool     `json:"favorite"`
}

func (c *Client) Secrets() ([]SecretMeta, error) {
	data, err := c.Raw(http.MethodGet, "/api/secrets", nil)
	if err != nil {
		return nil, err
	}
	return DecodeList[SecretMeta](data)
}

func (c *Client) SecretSave(req map[string]any) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/secrets", req)
}

func (c *Client) SecretGet(id string) ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/secrets/"+id, nil)
}

func (c *Client) SecretUpdate(id string, req map[string]any) ([]byte, error) {
	return c.Raw(http.MethodPut, "/api/secrets/"+id, req)
}

func (c *Client) SecretDelete(id string) error {
	return c.Do(http.MethodDelete, "/api/secrets/"+id, nil, nil)
}

func (c *Client) SecretReveal(id string) (string, error) {
	var out struct {
		Value string `json:"value"`
	}
	if err := c.Do(http.MethodGet, "/api/secrets/"+id+"/reveal", nil, &out); err != nil {
		return "", err
	}
	return out.Value, nil
}

// ── scheduler ─────────────────────────────────────────────────────

// ScheduleRow is a cron row (best-effort fields).
type ScheduleRow struct {
	ID          string  `json:"id"`
	Path        string  `json:"path"`
	Expr        string  `json:"expr"`
	Description *string `json:"description"`
	Enabled     bool    `json:"enabled"`
	RunOnBoot   bool    `json:"runOnBoot"`
	NextFireAt  *string `json:"nextFireAt"`
}

func (c *Client) CronList() ([]ScheduleRow, error) {
	data, err := c.Raw(http.MethodGet, "/api/cron", nil)
	if err != nil {
		return nil, err
	}
	return DecodeList[ScheduleRow](data)
}

func (c *Client) CronSave(req map[string]any) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/cron", req)
}

func (c *Client) CronUpdate(id string, req map[string]any) ([]byte, error) {
	return c.Raw(http.MethodPut, "/api/cron/"+id, req)
}

func (c *Client) CronDelete(id string) error {
	return c.Do(http.MethodDelete, "/api/cron/"+id, nil, nil)
}

func (c *Client) CronEnable(id string, enabled bool) error {
	verb := "disable"
	if enabled {
		verb = "enable"
	}
	return c.Do(http.MethodPost, "/api/cron/"+id+"/"+verb, map[string]any{}, nil)
}

func (c *Client) CronRun(id string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/cron/"+id+"/run", map[string]any{})
}

func (c *Client) CronHistory(id string) ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/cron/"+id+"/runs", nil)
}

// ── durability ────────────────────────────────────────────────────

func (c *Client) RepairStatus() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/repair/status", nil)
}

func (c *Client) RepairTasks() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/repair/tasks", nil)
}

func (c *Client) RepairHealth() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/repair/health", nil)
}

func (c *Client) RepairRun(findings []string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/repair/run", map[string]any{"findings": findings})
}

func (c *Client) RepairRebuild() ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/repair/rebuild", map[string]any{})
}

func (c *Client) RepairGC(dryRun bool, graceSecs *uint64) ([]byte, error) {
	req := map[string]any{"dryRun": dryRun}
	if graceSecs != nil {
		req["graceSecs"] = *graceSecs
	}
	return c.Raw(http.MethodPost, "/api/repair/gc", req)
}

func (c *Client) ScrubRun() ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/scrub/run", map[string]any{})
}

func (c *Client) ScrubRuns() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/scrub/runs", nil)
}

func (c *Client) LeaseAcquire(holder, scope string, ttlSecs uint64) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/lease/acquire", map[string]any{
		"holder": holder, "scope": scope, "ttlSecs": ttlSecs,
	})
}

func (c *Client) LeaseRelease(holder, scope string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/lease/release", map[string]any{
		"holder": holder, "scope": scope,
	})
}

func (c *Client) LeaseStatus(scope string) ([]byte, error) {
	path := "/api/lease/status"
	if scope != "" {
		path += "/" + scope
	}
	return c.Raw(http.MethodGet, path, nil)
}

// ── groups: collections / accounts / loose / faces ───────────────

func (c *Client) Collections() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/collections", nil)
}

func (c *Client) CollectionCreate(name, collectionType, color, description string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/collections", map[string]any{
		"name": name, "collectionType": collectionType,
		"color": color, "description": description,
	})
}

func (c *Client) CollectionAddItem(id, fileID, note string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/collections/"+id+"/items", map[string]any{
		"fileId": fileID, "note": note,
	})
}

func (c *Client) CollectionRemoveItem(id, fileID string) error {
	return c.Do(http.MethodDelete, "/api/collections/"+id+"/items/"+fileID, nil, nil)
}

func (c *Client) CollectionItems() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/collection-items", nil)
}

func (c *Client) Accounts() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/accounts", nil)
}

func (c *Client) AccountCreate(name, accountType, path, color string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/accounts", map[string]any{
		"name": name, "accountType": accountType, "path": path, "color": color,
	})
}

func (c *Client) AccountSwitch(id string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/accounts/"+id+"/switch", map[string]any{})
}

func (c *Client) AccountDelete(id string) error {
	return c.Do(http.MethodDelete, "/api/accounts/"+id, nil, nil)
}

func (c *Client) LooseGroups() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/loose-groups", nil)
}

func (c *Client) LooseGroupCreate(name, color string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/loose-groups", map[string]any{
		"name": name, "color": color,
	})
}

func (c *Client) LooseGroupAdd(groupID, fileID string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/loose-groups/"+groupID+"/files", map[string]any{
		"fileId": fileID,
	})
}

func (c *Client) FaceGroups() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/face-groups", nil)
}

func (c *Client) FacesDetect(fileID string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/faces/detect", map[string]any{"fileId": fileID})
}

func (c *Client) FacesDetectBatch() ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/faces/detect-batch", map[string]any{})
}

// ── shares + audit ────────────────────────────────────────────────

func (c *Client) ShareCreate(fileID string, expiresHours uint64) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/share-links", map[string]any{
		"fileId": fileID, "expiresInHours": expiresHours,
	})
}

func (c *Client) ShareList() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/share-links", nil)
}

func (c *Client) ShareRevoke(id string) error {
	return c.Do(http.MethodDelete, "/api/share-links/"+id, nil, nil)
}

func (c *Client) AuditList(limit int, entityType string) ([]byte, error) {
	q := url.Values{}
	if limit > 0 {
		q.Set("limit", fmt.Sprint(limit))
	}
	if entityType != "" {
		q.Set("entityType", entityType)
	}
	path := "/api/audit"
	if enc := q.Encode(); enc != "" {
		path += "?" + enc
	}
	return c.Raw(http.MethodGet, path, nil)
}

// ── batch ─────────────────────────────────────────────────────────

func (c *Client) BatchDelete(fileIDs []string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/batch/delete", map[string]any{"fileIds": fileIDs})
}

func (c *Client) BatchEncrypt(fileIDs []string, algorithm string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/batch/encrypt", map[string]any{
		"fileIds": fileIDs, "algorithm": algorithm,
	})
}

func (c *Client) BatchCompress(fileIDs []string, layer string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/batch/compress", map[string]any{
		"fileIds": fileIDs, "layer": layer,
	})
}

// ── versions + content + code ─────────────────────────────────────

func (c *Client) FileVersions(id string) ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/files/"+id+"/versions", nil)
}

func (c *Client) FileVersionCreate(id string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/files/"+id+"/versions", map[string]any{})
}

func (c *Client) FileVersionRevert(id, versionID string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/files/"+id+"/versions/"+versionID+"/revert", map[string]any{})
}

func (c *Client) VersionsSnapshotAll() ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/versions/snapshot-all", map[string]any{})
}

func (c *Client) FileContent(id string) ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/files/"+id+"/content", nil)
}

func (c *Client) FileContentWrite(id, content string) ([]byte, error) {
	return c.Raw(http.MethodPut, "/api/files/"+id+"/content", map[string]any{"content": content})
}

func (c *Client) CodeParse(fileName, content string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/code/parse", map[string]any{
		"fileName": fileName, "content": content,
	})
}

// ── crypto ────────────────────────────────────────────────────────

func (c *Client) CryptoStatus() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/encryption/status", nil)
}

func (c *Client) CryptoKeys() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/encryption/keys", nil)
}

// ── users + permissions ───────────────────────────────────────────

func (c *Client) Users() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/users", nil)
}

func (c *Client) UserCreate(username, password, role string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/users", map[string]any{
		"username": username, "password": password, "role": role,
	})
}

func (c *Client) UserDelete(id string) error {
	return c.Do(http.MethodDelete, "/api/users/"+id, nil, nil)
}

func (c *Client) UserSetRole(id, role string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/users/"+id+"/role", map[string]any{"role": role})
}

func (c *Client) PermGet(fileID string) ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/permissions/"+fileID, nil)
}

func (c *Client) PermGrant(userID, fileID, access string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/permissions", map[string]any{
		"userId": userID, "fileId": fileID, "access": access,
	})
}

func (c *Client) PermVerify(userID, fileID, requiredAccess string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/permissions/verify", map[string]any{
		"userId": userID, "fileId": fileID, "requiredAccess": requiredAccess,
	})
}

// ── agent extras ──────────────────────────────────────────────────

func (c *Client) AgentSessionGet(id string) ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/agent/sessions/"+id, nil)
}

func (c *Client) AgentInit(configID string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/agent/init", map[string]any{"configId": configID})
}

func (c *Client) AgentCompact(sessionID, configID string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/agent/sessions/"+sessionID+"/compact", map[string]any{
		"configId": configID,
	})
}

func (c *Client) AgentKeySeal(configID, apiKey string) error {
	return c.Do(http.MethodPut, "/api/agent/configs/"+configID+"/key", map[string]any{
		"apiKey": apiKey,
	}, nil)
}

func (c *Client) AgentModels(configID string) ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/agent/configs/"+configID+"/models", nil)
}

func (c *Client) AgentMCPTools(configID string) ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/agent/configs/"+configID+"/mcp/tools", nil)
}

func (c *Client) AgentMCPAdd(configID, name, server string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/agent/configs/"+configID+"/mcp", map[string]any{
		"name": name, "server": server,
	})
}

func (c *Client) AgentMCPRemove(configID, name string) error {
	return c.Do(http.MethodDelete, "/api/agent/configs/"+configID+"/mcp/"+name, nil, nil)
}

// AgentMemory is a semantic memory row (embedding never leaves the server).
type AgentMemory struct {
	ID       string `json:"id"`
	ConfigID string `json:"configId"`
	Text     string `json:"text"`
}

func (c *Client) AgentMemories(configID string) ([]AgentMemory, error) {
	path := "/api/agent/memories"
	if configID != "" {
		path += "?configId=" + url.QueryEscape(configID)
	}
	data, err := c.Raw(http.MethodGet, path, nil)
	if err != nil {
		return nil, err
	}
	return DecodeList[AgentMemory](data)
}

func (c *Client) AgentMemoryStore(configID, sessionID, text string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/agent/memories", map[string]any{
		"configId": configID, "sessionId": sessionID, "text": text,
	})
}

func (c *Client) AgentMemoryDelete(id string) error {
	return c.Do(http.MethodDelete, "/api/agent/memories/"+id, nil, nil)
}

func (c *Client) AgentMemoryRecall(configID, query string, topK int) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/agent/memories/recall", map[string]any{
		"configId": configID, "query": query, "topK": topK,
	})
}

func (c *Client) AgentMemoryExport(configID string) ([]byte, error) {
	path := "/api/agent/memories/export"
	if configID != "" {
		path += "?configId=" + url.QueryEscape(configID)
	}
	return c.Raw(http.MethodGet, path, nil)
}

// ── sync extras ───────────────────────────────────────────────────

func (c *Client) SyncRuns() ([]byte, error) {
	return c.Raw(http.MethodGet, "/api/sync/runs", nil)
}

func (c *Client) SyncRestore(configID, fileID, remotePath, destPath string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/sync/restore", map[string]any{
		"configId": configID, "fileId": fileID,
		"remotePath": remotePath, "destPath": destPath,
	})
}

func (c *Client) SyncRemoteDelete(configID, remotePath string) error {
	return c.Do(http.MethodDelete, "/api/sync/remote", map[string]any{
		"configId": configID, "remotePath": remotePath,
	}, nil)
}

func (c *Client) SyncCreateRepo(req map[string]any) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/sync/create-repo", req)
}

func (c *Client) SyncUpload(config map[string]any, remotePath, contentBase64 string) ([]byte, error) {
	return c.Raw(http.MethodPost, "/api/sync/upload", map[string]any{
		"config": config, "remotePath": remotePath, "contentBase64": contentBase64,
	})
}

// JSONField extracts a string field from raw JSON (best effort).
func JSONField(raw []byte, key string) string {
	var m map[string]any
	if err := json.Unmarshal(raw, &m); err != nil {
		return ""
	}
	s, _ := m[strings.ToLower(key)].(string)
	if s == "" {
		s, _ = m[key].(string)
	}
	return s
}
