// CyberManju OS — Todos API (server-side todos, §4.9 agent bridge).
//
// The same list per owner, shared by the agent tools (`todo_add` /
// `todo_list` / `todo_done`) and the Archive TASKS tab. Ownership is enforced
// at the route layer: non-admin callers only ever see and mutate their own
// `owner_id` (P0-10 posture — guessable ids must not cross user boundaries).
//
// Contract:
//   - GET /api/todos → `TodoRow[]` (owner-scoped unless admin)
//   - POST /api/todos { title } → row (`invalid:` on blank titles)
//   - PUT /api/todos/{id} { done } → row or `not_found:`
//   - DELETE /api/todos/{id} → `{ removed: true }` or `not_found:`
// Roles: Authenticated throughout (owner-scoped; admins see all).

use cybermanju_db::Database;
use cybermanju_types::schema::TodoRow;

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn respond<T: serde::Serialize>(result: Result<T, String>, origin: Option<&str>) -> String {
    match result {
        Ok(value) => crate::json_ok(&value, origin),
        Err(message) => {
            let status = if message.starts_with("not_found:") {
                404
            } else {
                400
            };
            crate::json_error(status, &message, origin)
        }
    }
}

fn parse<T: for<'de> serde::Deserialize<'de>>(body: &str) -> Result<T, String> {
    serde_json::from_str(body).map_err(|e| format!("invalid: bad JSON body: {e}"))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    pub title: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleRequest {
    pub done: bool,
}

/// Create a todo owned by `owner`.
pub fn create(db: &Database, owner: &str, req: &CreateRequest) -> Result<TodoRow, String> {
    let title = req.title.trim().to_string();
    if title.is_empty() {
        return Err("invalid: todo title is required".to_string());
    }
    if title.len() > 500 {
        return Err("invalid: todo title exceeds 500 chars".to_string());
    }
    let now = now_rfc3339();
    let row = TodoRow {
        id: format!("todo-{}", uuid::Uuid::new_v4()),
        title,
        done: false,
        created_at: now,
        done_at: None,
        owner_id: owner.to_string(),
        source: "local".to_string(),
    };
    db.save_todo(&row).map_err(|e| e.to_string())?;
    Ok(row)
}

/// Read one todo, enforcing the owner scope (admins bypass).
fn scoped(db: &Database, id: &str, owner: &str, is_admin: bool) -> Result<TodoRow, String> {
    let row = db
        .list_todos(None)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| format!("not_found: no todo `{id}`"))?;
    if !is_admin && row.owner_id != owner {
        return Err(format!("not_found: no todo `{id}`"));
    }
    Ok(row)
}

/// Dispatch `/api/todos/*`. `owner`/`is_admin` come from verified claims
/// (`claims_owner` in lib.rs); untrusted callers arrive with an empty id
/// and see nothing.
pub fn route(
    db: &Database,
    method: &str,
    path_segments: &[&str],
    body: &str,
    origin: Option<&str>,
    owner: &str,
    is_admin: bool,
) -> Option<String> {
    let response = match path_segments {
        ["api", "todos"] if method == "GET" => respond(
            db.list_todos(if is_admin { None } else { Some(owner) })
                .map_err(|e| e.to_string()),
            origin,
        ),

        ["api", "todos"] if method == "POST" => match parse::<CreateRequest>(body) {
            Ok(req) => respond(create(db, owner, &req), origin),
            Err(e) => respond::<serde_json::Value>(Err(e), origin),
        },

        ["api", "todos", id] if method == "PUT" => match parse::<ToggleRequest>(body) {
            Ok(req) => match scoped(db, id, owner, is_admin) {
                Ok(mut row) => {
                    row.done = req.done;
                    row.done_at = if req.done { Some(now_rfc3339()) } else { None };
                    match db.save_todo(&row).map_err(|e| e.to_string()) {
                        Ok(()) => respond(Ok(row), origin),
                        Err(e) => respond::<TodoRow>(Err(e), origin),
                    }
                }
                Err(e) => respond::<TodoRow>(Err(e), origin),
            },
            Err(e) => respond::<TodoRow>(Err(e), origin),
        },

        ["api", "todos", id] if method == "DELETE" => match scoped(db, id, owner, is_admin) {
            Ok(_) => match db.remove_todo(id) {
                Ok(true) => respond(Ok(serde_json::json!({ "removed": true })), origin),
                Ok(false) => {
                    respond::<serde_json::Value>(Err(format!("not_found: no todo `{id}`")), origin)
                }
                Err(e) => respond::<serde_json::Value>(Err(e.to_string()), origin),
            },
            Err(e) => respond::<serde_json::Value>(Err(e), origin),
        },

        _ => return None,
    };
    Some(response)
}
