// CyberManju OS — Accounts (crates/web/src/api/accounts.rs).
// Role: REST dashboard (:3456): routes, auth, security headers.
// Exports: list, create, switch, delete.
// Integrates: cybermanju_db, cybermanju_types, redb.
// Contracts: none special (pure logic; errors bubble to caller).
// CyberManju OS — Account switching (shared by Tauri IPC and REST)

use cybermanju_db::Database;
use cybermanju_types::schema::Account;
use redb::ReadableTable;

/// List all accounts from the database.
pub fn list(db: &Database) -> Result<Vec<Account>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_accounts_table())
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        let account: Account = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        results.push(account);
    }

    Ok(results)
}

/// Create a new account. The first account becomes active automatically.
pub fn create(
    db: &Database,
    name: String,
    account_type: String,
    path: Option<String>,
    color: Option<String>,
) -> Result<Account, String> {
    let account_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let existing_table = tx_read
        .open_table(Database::get_accounts_table())
        .map_err(|e| e.to_string())?;
    let has_existing = existing_table
        .iter()
        .map_err(|e| e.to_string())?
        .next()
        .is_some();
    drop(tx_read);

    let account = Account {
        id: account_id.clone(),
        name,
        account_type,
        path,
        color: color.unwrap_or_else(|| "#6366f1".to_string()),
        is_active: !has_existing,
        created_at: now.clone(),
        updated_at: now,
    };

    let serialized = serde_json::to_string(&account).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_accounts_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(account_id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(account)
}

/// Switch the active account. Sets `is_active=true` on the target account
/// and `is_active=false` on every other account.
pub fn switch(db: &Database, account_id: &str) -> Result<Account, String> {
    let now = chrono::Utc::now().to_rfc3339();

    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let read_table = tx_read
        .open_table(Database::get_accounts_table())
        .map_err(|e| e.to_string())?;

    let mut accounts: Vec<(String, Account)> = Vec::new();
    let mut target_found = false;

    for entry in read_table.iter().map_err(|e| e.to_string())? {
        let (key, value) = entry.map_err(|e| e.to_string())?;
        let mut account: Account =
            serde_json::from_str(value.value()).map_err(|e| e.to_string())?;

        if key.value() == account_id {
            account.is_active = true;
            target_found = true;
        } else {
            account.is_active = false;
        }
        account.updated_at = now.clone();
        accounts.push((key.value().to_string(), account));
    }
    drop(tx_read);

    if !target_found {
        return Err(format!("Account not found: {}", account_id));
    }

    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_accounts_table())
            .map_err(|e| e.to_string())?;

        for (id, account) in &accounts {
            let serialized = serde_json::to_string(account).map_err(|e| e.to_string())?;
            table
                .insert(id.as_str(), serialized.as_str())
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;

    accounts
        .into_iter()
        .find(|(id, _)| id.as_str() == account_id)
        .map(|(_, a)| a)
        .ok_or_else(|| format!("Account not found after write: {}", account_id))
}

/// Delete an account by its ID. Cannot delete the active account.
pub fn delete(db: &Database, account_id: &str) -> Result<bool, String> {
    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let read_table = tx_read
        .open_table(Database::get_accounts_table())
        .map_err(|e| e.to_string())?;
    let value = read_table
        .get(account_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Account not found: {}", account_id))?;
    let account: Account = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;

    if account.is_active {
        return Err(
            "Cannot delete the active account. Switch to another account first.".to_string(),
        );
    }
    drop(tx_read);

    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_accounts_table())
            .map_err(|e| e.to_string())?;
        let removed = table
            .remove(account_id)
            .map_err(|e| e.to_string())?
            .is_some();

        if !removed {
            return Err(format!("Account not found during deletion: {}", account_id));
        }
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(true)
}
