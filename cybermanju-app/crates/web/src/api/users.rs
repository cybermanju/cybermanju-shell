// CyberManju OS — User management (shared by Tauri IPC and REST)
//
// <<< AGENT-3 IDENTITY: registration modes, pinned Argon2id parameters and a
// single legacy-hash migration path shared by the HTTP and Tauri transports. >>>

use crate::security;
use cybermanju_db::Database;
use cybermanju_types::schema::User;
use redb::ReadableTable;

/// OWASP Argon2id recommendations (m=19456 KiB, t=2, p=1, 32-byte tag).
/// Previously the crate defaults were used implicitly, which makes hashes
/// non-portable between `argon2` releases.
const ARGON2_M_COST: u32 = 19_456;
const ARGON2_T_COST: u32 = 2;
const ARGON2_P_COST: u32 = 1;
const ARGON2_OUTPUT_LEN: usize = 32;

/// How a caller reached the registration endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationMode {
    /// Public `POST /api/users/register` — bootstrap only, never admin.
    Bootstrap,
    /// `POST /api/users` behind an existing admin session.
    AdminCreated,
    /// Desktop Tauri IPC — a trusted local process, roles unrestricted.
    LocalIpc,
}

/// Successful [`authenticate`] result.
#[derive(Debug)]
pub struct AuthOutcome {
    /// The user as stored after any opportunistic hash upgrade.
    pub user: User,
    /// `true` when a legacy BLAKE3 hash was migrated to argon2id.
    pub upgraded: bool,
}

// ─── Registration ─────────────────────────────────────────────────────

/// Register a new user (also used by the `register_user` Tauri command).
///
/// The caller must hold the database write lock. `mode` decides both whether
/// registration is open at all and whether the requested role may be `admin`.
pub fn register(
    db: &Database,
    username: String,
    password: String,
    display_name: Option<String>,
    role: Option<String>,
    mode: RegistrationMode,
) -> Result<User, String> {
    let username = username.trim().to_string();
    security::validate_username(&username)?;
    if let Some(display) = display_name.as_deref() {
        security::validate_display_name(display)?;
    }
    security::validate_password(&password)?;

    let role = resolve_role(db, role.as_deref(), mode)?;

    if find_by_username(db, &username)?.is_some() {
        return Err(format!("Username '{}' already exists", username));
    }

    let password_hash = argon2_hash(&password)?;

    let user_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let user = User {
        id: user_id.clone(),
        username,
        password_hash,
        display_name,
        role,
        is_active: true,
        created_at: now.clone(),
        updated_at: now,
    };

    let serialized = serde_json::to_string(&user).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_users_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(user_id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(user)
}

/// Decide the role for a new account, enforcing the bootstrap gate.
fn resolve_role(
    db: &Database,
    requested: Option<&str>,
    mode: RegistrationMode,
) -> Result<String, String> {
    match mode {
        RegistrationMode::Bootstrap => {
            if !registration_open(db)? {
                return Err("Registration is closed: users already exist".to_string());
            }
            match requested.map(str::trim).filter(|r| !r.is_empty()) {
                None => Ok("user".to_string()),
                Some("admin") => Err("Registration cannot grant the admin role".to_string()),
                Some(r) if security::VALID_ROLES.contains(&r) => Ok(r.to_string()),
                Some(r) => Err(format!("Invalid role: {r}. Must be admin, user, or viewer")),
            }
        }
        RegistrationMode::AdminCreated | RegistrationMode::LocalIpc => {
            let role = requested
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .unwrap_or("user")
                .to_string();
            security::validate_role(&role)?;
            Ok(role)
        }
    }
}

/// Whether the public registration endpoint may create an account right now.
///
/// Open only while the user table is empty, or when an operator explicitly
/// opts in with `CYBERMANJU_ALLOW_REGISTRATION=1`.
pub fn registration_open(db: &Database) -> Result<bool, String> {
    if let Ok(flag) = std::env::var("CYBERMANJU_ALLOW_REGISTRATION") {
        let flag = flag.trim();
        if flag == "1" || flag.eq_ignore_ascii_case("true") || flag.eq_ignore_ascii_case("yes") {
            return Ok(true);
        }
    }
    Ok(count_users(db)? == 0)
}

/// Number of user records currently stored.
pub fn count_users(db: &Database) -> Result<usize, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_users_table())
        .map_err(|e| e.to_string())?;
    let mut count = 0usize;
    for entry in table.iter().map_err(|e| e.to_string())? {
        if entry.is_ok() {
            count += 1;
        }
    }
    Ok(count)
}

/// Provision (or promote) the operator-declared admin account.
///
/// Registration never mints an admin, so Docker/headless deployments need an
/// out-of-band way to create the first one: `CYBERMANJU_ADMIN_USERNAME` plus
/// `CYBERMANJU_ADMIN_PASSWORD`. Returns the created/updated user, or `None`
/// when neither variable is set.
pub fn ensure_admin_provisioned(
    db: &Database,
    username: &str,
    password: &str,
) -> Result<Option<User>, String> {
    let username = username.trim();
    if username.is_empty() || password.is_empty() {
        return Ok(None);
    }
    match find_by_username(db, username)? {
        Some(mut existing) => {
            let mut changed = false;
            if existing.role != "admin" {
                existing.role = "admin".to_string();
                changed = true;
            }
            // Reset the credential when it no longer verifies, so the
            // operator can rotate a locked-out bootstrap account.
            if !argon2_verify(password, &existing.password_hash).unwrap_or(false) {
                existing.password_hash = argon2_hash(password)?;
                changed = true;
            }
            if changed {
                existing.updated_at = chrono::Utc::now().to_rfc3339();
                store_user(db, &existing)?;
            }
            Ok(Some(existing))
        }
        None => {
            let user = register(
                db,
                username.to_string(),
                password.to_string(),
                None,
                Some("admin".to_string()),
                RegistrationMode::LocalIpc,
            )?;
            Ok(Some(user))
        }
    }
}

// ─── Authentication ───────────────────────────────────────────────────

/// Verify a username/password pair.
///
/// Legacy unsalted BLAKE3 hashes (written by the original desktop build) are
/// accepted once and transparently rewritten as argon2id, so both transports
/// behave identically instead of the web path returning HTTP 500.
///
/// All failures return the same message so a caller cannot distinguish
/// "unknown user" from "wrong password".
pub fn authenticate(db: &Database, username: &str, password: &str) -> Result<AuthOutcome, String> {
    let mut user =
        find_by_username(db, username.trim())?.ok_or_else(|| "Invalid credentials".to_string())?;

    if !user.is_active {
        return Err("User account is deactivated".to_string());
    }
    if password.is_empty() {
        return Err("Invalid credentials".to_string());
    }

    let mut upgraded = false;
    let valid = if user.password_hash.starts_with("$argon2") {
        argon2_verify(password, &user.password_hash)?
    } else if is_legacy_blake3(&user.password_hash) {
        let digest = blake3::hash(password.as_bytes()).to_hex().to_string();
        let matched = constant_time_eq(digest.as_bytes(), user.password_hash.as_bytes());
        if matched {
            let new_hash = argon2_hash(password)?;
            user.password_hash = new_hash.clone();
            write_password_hash(db, &user.id, &new_hash)?;
            upgraded = true;
        }
        matched
    } else {
        false
    };

    if !valid {
        return Err("Invalid credentials".to_string());
    }

    Ok(AuthOutcome { user, upgraded })
}

/// A legacy hash is a bare 64-char lowercase hex digest.
fn is_legacy_blake3(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Persist only the password-hash column of an existing user.
fn write_password_hash(db: &Database, user_id: &str, password_hash: &str) -> Result<(), String> {
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_users_table())
            .map_err(|e| e.to_string())?;
        let raw = match table.get(user_id).map_err(|e| e.to_string())? {
            Some(v) => v.value().to_string(),
            None => return Err(format!("User not found: {}", user_id)),
        };
        let mut user: User = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        user.password_hash = password_hash.to_string();
        user.updated_at = chrono::Utc::now().to_rfc3339();
        table
            .insert(
                user_id,
                serde_json::to_string(&user)
                    .map_err(|e| e.to_string())?
                    .as_str(),
            )
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

// ─── Maintenance ──────────────────────────────────────────────────────

/// Write an entire user record back to the store.
fn store_user(db: &Database, user: &User) -> Result<(), String> {
    let serialized = serde_json::to_string(user).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_users_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(user.id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Delete a user by ID.
pub fn delete(db: &Database, user_id: &str) -> Result<bool, String> {
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_users_table())
            .map_err(|e| e.to_string())?;
        let removed = table.remove(user_id).map_err(|e| e.to_string())?.is_some();
        if !removed {
            return Err(format!("User not found: {}", user_id));
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(true)
}

/// Update a user's role.
pub fn update_role(db: &Database, user_id: &str, role: String) -> Result<User, String> {
    security::validate_role(&role)?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    let user = {
        let table = tx
            .open_table(Database::get_users_table())
            .map_err(|e| e.to_string())?;
        let existing = table.get(user_id).map_err(|e| e.to_string())?;
        match existing {
            Some(v) => {
                let mut user: User = serde_json::from_str(v.value()).map_err(|e| e.to_string())?;
                user.role = role;
                user.updated_at = chrono::Utc::now().to_rfc3339();
                user
            }
            None => return Err(format!("User not found: {}", user_id)),
        }
    };
    {
        let mut table = tx
            .open_table(Database::get_users_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(
                user_id,
                serde_json::to_string(&user)
                    .map_err(|e| e.to_string())?
                    .as_str(),
            )
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(user)
}

/// Look up a user by username.
pub fn find_by_username(db: &Database, username: &str) -> Result<Option<User>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_users_table())
        .map_err(|e| e.to_string())?;
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        let user: User = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        if user.username == username {
            return Ok(Some(user));
        }
    }
    Ok(None)
}

/// Reject tokens whose account was deleted, disabled, renamed, or changed role.
/// The token's username and role are identity claims, not durable authority;
/// callers should invoke this after signature/expiry verification.
pub fn validate_token_identity(db: &Database, claims: &security::Claims) -> Result<(), String> {
    let tx = db
        .begin_read()
        .map_err(|_| "auth: account identity unavailable".to_string())?;
    let users = tx
        .open_table(Database::get_users_table())
        .map_err(|_| "auth: account identity unavailable".to_string())?;
    let raw = users
        .get(claims.user_id.as_str())
        .map_err(|_| "auth: account identity unavailable".to_string())?
        .ok_or_else(|| "auth: account no longer exists".to_string())?;
    let user: User = serde_json::from_str(raw.value())
        .map_err(|_| "auth: account identity is invalid".to_string())?;

    if user.id != claims.user_id || user.username != claims.sub {
        return Err("auth: token identity no longer matches account".to_string());
    }
    if !user.is_active {
        return Err("auth: account is inactive".to_string());
    }
    if user.role != claims.role {
        return Err("auth: account role has changed".to_string());
    }
    Ok(())
}

// ─── Password hashing ─────────────────────────────────────────────────

/// Argon2id password hashing with pinned parameters.
pub fn argon2_hash(password: &str) -> Result<String, String> {
    use argon2::password_hash::{PasswordHasher, SaltString};

    let salt = SaltString::generate(&mut rand_core::OsRng);
    argon2_instance()?
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("Argon2 hash error: {}", e))
}

/// Argon2id password verification.
pub fn argon2_verify(password: &str, hash: &str) -> Result<bool, String> {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};

    if !hash.starts_with("$argon2") {
        return Ok(false);
    }
    let parsed = PasswordHash::new(hash).map_err(|e| format!("Invalid hash format: {}", e))?;
    match argon2_instance()?.verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(format!("Argon2 verify error: {}", e)),
    }
}

/// Build the pinned Argon2id instance used for every password operation.
fn argon2_instance() -> Result<argon2::Argon2<'static>, String> {
    use argon2::{Algorithm, Argon2, Params, Version};

    let params = Params::new(
        ARGON2_M_COST,
        ARGON2_T_COST,
        ARGON2_P_COST,
        Some(ARGON2_OUTPUT_LEN),
    )
    .map_err(|e| format!("Invalid Argon2 parameters: {}", e))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Database, tempfile::TempDir, User, security::Claims) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Database::new(dir.path().join("identity.redb").to_str().unwrap()).unwrap();
        let user = User {
            id: "identity-user".to_string(),
            username: "alice".to_string(),
            password_hash: String::new(),
            display_name: None,
            role: "admin".to_string(),
            is_active: true,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx.open_table(Database::get_users_table()).unwrap();
            table
                .insert(
                    user.id.as_str(),
                    serde_json::to_string(&user).unwrap().as_str(),
                )
                .unwrap();
        }
        tx.commit().unwrap();
        let claims = security::Claims {
            sub: user.username.clone(),
            user_id: user.id.clone(),
            role: user.role.clone(),
            iat: 1,
            exp: u64::MAX,
            jti: "identity-test".to_string(),
        };
        (db, dir, user, claims)
    }

    fn save(db: &Database, user: &User) {
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx.open_table(Database::get_users_table()).unwrap();
            table
                .insert(
                    user.id.as_str(),
                    serde_json::to_string(user).unwrap().as_str(),
                )
                .unwrap();
        }
        tx.commit().unwrap();
    }

    #[test]
    fn token_identity_matches_current_active_user() {
        let (db, _dir, _, claims) = fixture();
        assert!(validate_token_identity(&db, &claims).is_ok());
    }

    #[test]
    fn role_rename_deactivation_and_deletion_reject_old_identity() {
        let (db, _dir, mut user, claims) = fixture();

        user.role = "user".to_string();
        save(&db, &user);
        assert!(validate_token_identity(&db, &claims)
            .unwrap_err()
            .contains("role"));

        user.role = claims.role.clone();
        user.username = "alice-renamed".to_string();
        save(&db, &user);
        assert!(validate_token_identity(&db, &claims)
            .unwrap_err()
            .contains("identity"));

        user.username = claims.sub.clone();
        user.is_active = false;
        save(&db, &user);
        assert!(validate_token_identity(&db, &claims)
            .unwrap_err()
            .contains("inactive"));

        let tx = db.begin_write().unwrap();
        {
            let mut table = tx.open_table(Database::get_users_table()).unwrap();
            table.remove(user.id.as_str()).unwrap();
        }
        tx.commit().unwrap();
        assert!(validate_token_identity(&db, &claims)
            .unwrap_err()
            .contains("exists"));
    }
}
