use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use argon2::{Algorithm, Argon2, Params, Version};
use chrono::{Duration, NaiveDate, Utc};
use rand::{RngCore, rngs::OsRng};
use rusqlite::{Connection, DatabaseName, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration as StdDuration, Instant},
};
use tauri::{Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_notification::NotificationExt;
use zeroize::{Zeroize, Zeroizing};

const SCHEMA_VERSION: i64 = 5;
const VAULT_KDF_MEMORY_KIB: u32 = 65_536;
const VAULT_KDF_ITERATIONS: u32 = 3;
const VAULT_KDF_PARALLELISM: u32 = 1;
const VAULT_KDF_FORMAT_VERSION: u32 = 1;
const VAULT_AUTO_LOCK_AFTER: StdDuration = StdDuration::from_secs(5 * 60);
const VAULT_CLIPBOARD_CLEAR_AFTER: StdDuration = StdDuration::from_secs(30);
const VAULT_VERIFIER: &[u8] = b"DevHUD vault verifier v1";
const VAULT_VERIFIER_AAD: &[u8] = b"devhud-vault-verifier-v1";

pub struct AppDatabase {
    connection: Mutex<Connection>,
}

#[derive(Default)]
pub struct VaultSession {
    key: Mutex<Option<Zeroizing<[u8; 32]>>>,
    last_activity: Mutex<Option<Instant>>,
}

impl VaultSession {
    fn unlock(&self, key: Zeroizing<[u8; 32]>) -> Result<(), String> {
        *self
            .key
            .lock()
            .map_err(|_| "The vault is temporarily unavailable".to_string())? = Some(key);
        *self
            .last_activity
            .lock()
            .map_err(|_| "The vault is temporarily unavailable".to_string())? =
            Some(Instant::now());
        Ok(())
    }

    fn lock(&self) -> Result<(), String> {
        self.key
            .lock()
            .map_err(|_| "The vault is temporarily unavailable".to_string())?
            .take();
        *self
            .last_activity
            .lock()
            .map_err(|_| "The vault is temporarily unavailable".to_string())? = None;
        Ok(())
    }

    fn is_unlocked(&self) -> Result<bool, String> {
        let guard = self
            .key
            .lock()
            .map_err(|_| "The vault lock is poisoned".to_string())?;
        Ok(guard.is_some())
    }

    fn touch(&self) -> Result<(), String> {
        *self
            .last_activity
            .lock()
            .map_err(|_| "The vault is temporarily unavailable".to_string())? =
            Some(Instant::now());
        Ok(())
    }

    fn lock_if_idle(&self) {
        let idle = self
            .last_activity
            .lock()
            .ok()
            .and_then(|activity| *activity)
            .is_some_and(|activity| activity.elapsed() >= VAULT_AUTO_LOCK_AFTER);
        if idle {
            if let Err(error) = self.lock() {
                eprintln!("Could not auto-lock vault: {error}");
            }
        }
    }

    fn with_key<T>(
        &self,
        operation: impl FnOnce(&[u8; 32]) -> Result<T, String>,
    ) -> Result<T, String> {
        let key = self
            .key
            .lock()
            .map_err(|_| "The vault is temporarily unavailable".to_string())?;
        let key = key
            .as_ref()
            .ok_or_else(|| "Unlock the vault before accessing credentials".to_string())?;
        self.touch()?;
        operation(key)
    }
}

impl AppDatabase {
    fn open(path: &Path) -> Result<Self, String> {
        let connection =
            Connection::open(path).map_err(|_| "Could not open the local database".to_string())?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "Could not configure the local database".to_string())?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(|_| "Could not configure the local database".to_string())?;
        let current_version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|_| "Could not check the database version".to_string())?;
        if current_version > 0 && current_version < SCHEMA_VERSION {
            let backups_dir = path
                .parent()
                .ok_or_else(|| "Could not locate the database backup directory".to_string())?
                .join("backups");
            fs::create_dir_all(&backups_dir)
                .map_err(|_| "Could not prepare a pre-migration backup".to_string())?;
            let backup_id = format!("backup-{}", uuid::Uuid::new_v4());
            let backup_path = backups_dir.join(format!("{backup_id}.sqlite3"));
            create_database_backup(&connection, &backup_path)?;
        }
        migrate(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    #[cfg(test)]
    fn in_memory() -> Result<Self, String> {
        let connection = Connection::open_in_memory()
            .map_err(|_| "Could not open the test database".to_string())?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|_| "Could not configure the test database".to_string())?;
        migrate(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

fn migrate(connection: &Connection) -> Result<(), String> {
    let mut version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|_| "Could not check the database version".to_string())?;
    if version > SCHEMA_VERSION {
        return Err("This database was created by a newer version of DevHUD".to_string());
    }
    if version < 1 {
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| "Could not start the database migration".to_string())?;
        transaction
            .execute_batch(
                "CREATE TABLE sprints (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL CHECK(length(trim(name)) BETWEEN 1 AND 120),
                goal TEXT NOT NULL DEFAULT '',
                start_at TEXT NOT NULL,
                end_at TEXT NOT NULL,
                status TEXT NOT NULL CHECK(status IN ('active', 'archived')),
                created_at TEXT NOT NULL,
                archived_at TEXT
            );
            CREATE UNIQUE INDEX one_active_sprint ON sprints(status) WHERE status = 'active';

            CREATE TABLE work_items (
                id TEXT PRIMARY KEY NOT NULL,
                sprint_id TEXT REFERENCES sprints(id) ON DELETE SET NULL,
                kind TEXT NOT NULL CHECK(kind IN ('task', 'production_note', 'idea', 'blocker')),
                title TEXT NOT NULL CHECK(length(trim(title)) BETWEEN 1 AND 500),
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL CHECK(status IN ('todo', 'in_progress', 'blocked', 'done', 'cancelled')),
                priority TEXT NOT NULL CHECK(priority IN ('low', 'normal', 'high', 'critical')),
                due_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                started_at TEXT,
                completed_at TEXT,
                source_item_id TEXT REFERENCES work_items(id) ON DELETE SET NULL
            );
            CREATE INDEX work_items_sprint_status ON work_items(sprint_id, status);
            CREATE INDEX work_items_due_at ON work_items(due_at) WHERE due_at IS NOT NULL;

            CREATE TABLE reminders (
                id TEXT PRIMARY KEY NOT NULL,
                work_item_id TEXT REFERENCES work_items(id) ON DELETE SET NULL,
                title TEXT NOT NULL CHECK(length(trim(title)) BETWEEN 1 AND 500),
                trigger_at TEXT NOT NULL,
                timezone TEXT NOT NULL,
                repeat_rule TEXT,
                enabled INTEGER NOT NULL CHECK(enabled IN (0, 1))
            );
            PRAGMA user_version = 1;",
            )
            .map_err(|_| "Could not apply the database migration".to_string())?;
        transaction
            .commit()
            .map_err(|_| "Could not finish the database migration".to_string())?;
        version = 1;
    }

    if version < 2 {
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| "Could not start the reminder migration".to_string())?;
        transaction
            .execute_batch(
                "CREATE TABLE reminder_occurrences (
                    id TEXT PRIMARY KEY NOT NULL,
                    reminder_id TEXT NOT NULL REFERENCES reminders(id) ON DELETE CASCADE,
                    scheduled_at TEXT NOT NULL,
                    snoozed_until TEXT,
                    delivered_at TEXT,
                    dismissed_at TEXT,
                    completed_at TEXT,
                    UNIQUE(reminder_id, scheduled_at)
                );
                CREATE INDEX reminder_occurrences_pending
                    ON reminder_occurrences(scheduled_at)
                    WHERE delivered_at IS NULL AND dismissed_at IS NULL AND completed_at IS NULL;
                PRAGMA user_version = 2;",
            )
            .map_err(|_| "Could not apply the reminder migration".to_string())?;
        transaction
            .commit()
            .map_err(|_| "Could not finish the reminder migration".to_string())?;
        version = 2;
    }

    if version < 3 {
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| "Could not start the history migration".to_string())?;
        transaction
            .execute_batch(
                "CREATE TABLE sprint_archives (
                    sprint_id TEXT PRIMARY KEY NOT NULL REFERENCES sprints(id) ON DELETE RESTRICT,
                    snapshot_json TEXT NOT NULL,
                    archived_at TEXT NOT NULL,
                    schema_version INTEGER NOT NULL
                );
                CREATE UNIQUE INDEX unique_sprint_carryover
                    ON work_items(source_item_id, sprint_id)
                    WHERE source_item_id IS NOT NULL;
                PRAGMA user_version = 3;",
            )
            .map_err(|_| "Could not apply the history migration".to_string())?;
        transaction
            .commit()
            .map_err(|_| "Could not finish the history migration".to_string())?;
        version = 3;
    }

    if version < 4 {
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| "Could not start the vault migration".to_string())?;
        transaction
            .execute_batch(
                "CREATE TABLE vault_config (
                    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                    salt BLOB NOT NULL,
                    memory_kib INTEGER NOT NULL,
                    iterations INTEGER NOT NULL,
                    parallelism INTEGER NOT NULL,
                    verifier_nonce BLOB NOT NULL,
                    verifier_cipher BLOB NOT NULL
                );
                CREATE TABLE vault_items (
                    id TEXT PRIMARY KEY NOT NULL,
                    nonce BLOB NOT NULL,
                    ciphertext BLOB NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                PRAGMA user_version = 4;",
            )
            .map_err(|_| "Could not apply the vault migration".to_string())?;
        transaction
            .commit()
            .map_err(|_| "Could not finish the vault migration".to_string())?;
    }

    if version < 5 {
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| "Could not start the vault format migration".to_string())?;
        transaction
            .execute_batch(
                "ALTER TABLE vault_config ADD COLUMN kdf_format_version INTEGER NOT NULL DEFAULT 1;
                 ALTER TABLE vault_config ADD COLUMN kdf_algorithm TEXT NOT NULL DEFAULT 'argon2id';
                 ALTER TABLE vault_config ADD COLUMN argon_version INTEGER NOT NULL DEFAULT 19;
                 PRAGMA user_version = 5;",
            )
            .map_err(|_| "Could not apply the vault format migration".to_string())?;
        transaction
            .commit()
            .map_err(|_| "Could not finish the vault format migration".to_string())?;
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemStatus {
    Todo,
    InProgress,
    Blocked,
    Done,
    Cancelled,
}

impl WorkItemStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemPriority {
    Low,
    Normal,
    High,
    Critical,
}

impl WorkItemPriority {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemKind {
    Task,
    ProductionNote,
    Idea,
    Blocker,
}

impl WorkItemKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::ProductionNote => "production_note",
            Self::Idea => "idea",
            Self::Blocker => "blocker",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Sprint {
    pub id: String,
    pub name: String,
    pub goal: String,
    pub start_at: String,
    pub end_at: String,
    pub status: String,
    pub created_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub id: String,
    pub sprint_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub due_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub source_item_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewWorkItem {
    pub sprint_id: Option<String>,
    pub kind: WorkItemKind,
    pub title: String,
    pub description: String,
    pub priority: WorkItemPriority,
    pub due_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: String,
    pub work_item_id: Option<String>,
    pub title: String,
    pub trigger_at: String,
    pub timezone: String,
    pub repeat_rule: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewReminder {
    pub work_item_id: Option<String>,
    pub title: String,
    pub trigger_at: String,
    pub timezone: String,
}

#[derive(Debug, Deserialize, Serialize, Zeroize)]
#[zeroize(drop)]
#[serde(rename_all = "camelCase")]
struct VaultPayload {
    kind: String,
    title: String,
    url: String,
    username: String,
    password: String,
    notes: String,
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewVaultItem {
    kind: String,
    title: String,
    url: String,
    username: String,
    password: String,
    notes: String,
    tags: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VaultSearchResult {
    id: String,
    kind: String,
    title: String,
    url: String,
    username: String,
    tags: Vec<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum VaultField {
    Username,
    Password,
    Notes,
}

fn derive_vault_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, String> {
    let parameters = Params::new(
        VAULT_KDF_MEMORY_KIB,
        VAULT_KDF_ITERATIONS,
        VAULT_KDF_PARALLELISM,
        Some(32),
    )
    .map_err(|_| "Could not configure vault key derivation".to_string())?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, parameters);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| "Could not derive the vault key".to_string())?;
    Ok(key)
}

fn encrypt_vault_bytes(
    key: &[u8; 32],
    nonce: &[u8; 12],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| "Could not initialize vault encryption".to_string())?;
    cipher
        .encrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| "Could not encrypt vault data".to_string())
}

fn decrypt_vault_bytes(
    key: &[u8; 32],
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Zeroizing<Vec<u8>>, String> {
    if nonce.len() != 12 {
        return Err("Vault data has an invalid encryption nonce".to_string());
    }
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| "Could not initialize vault encryption".to_string())?;
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| "Vault authentication failed".to_string())
}

fn validate_vault_text(value: &str, field: &str, max_length: usize) -> Result<(), String> {
    if value.chars().count() > max_length {
        return Err(format!("{field} must be {max_length} characters or fewer"));
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedSprint {
    pub sprint: Sprint,
    pub work_items: Vec<WorkItem>,
    pub archived_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SprintSummary {
    pub sprint: Sprint,
    pub completed: Vec<String>,
    pub in_progress: Vec<String>,
    pub blocked: Vec<String>,
    pub remaining: Vec<String>,
    pub carried_over: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub id: String,
    pub created_at: String,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    pub backup: BackupInfo,
    pub sprints: u64,
    pub work_items: u64,
    pub reminders: u64,
    pub integrity_ok: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub restored_from: String,
    pub safety_backup: BackupInfo,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductivityExport {
    format_version: u32,
    exported_at: String,
    sprints: Vec<Sprint>,
    work_items: Vec<WorkItem>,
    reminders: Vec<Reminder>,
}

fn backups_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|_| "Could not locate DevHUD's data directory".to_string())?
        .join("backups");
    fs::create_dir_all(&path).map_err(|_| "Could not prepare the backup directory".to_string())?;
    Ok(path)
}

fn backup_path(directory: &Path, id: &str) -> Result<PathBuf, String> {
    let uuid = id
        .strip_prefix("backup-")
        .ok_or_else(|| "Invalid backup identifier".to_string())?;
    uuid::Uuid::parse_str(uuid).map_err(|_| "Invalid backup identifier".to_string())?;
    Ok(directory.join(format!("{id}.sqlite3")))
}

fn create_database_backup(connection: &Connection, path: &Path) -> Result<(), String> {
    connection
        .backup(DatabaseName::Main, path, None)
        .map_err(|_| "Could not create the SQLite backup".to_string())
}

fn restore_database(source: &Connection, destination: &mut Connection) -> Result<(), String> {
    let backup = rusqlite::backup::Backup::new(source, destination)
        .map_err(|_| "Could not prepare the database restore".to_string())?;
    backup
        .run_to_completion(5, StdDuration::from_millis(250), None)
        .map_err(|_| "Could not restore the local database".to_string())?;
    drop(backup);
    destination
        .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
        .map_err(|_| "Could not configure the restored database".to_string())
}

fn backup_info(id: String, path: &Path) -> Result<BackupInfo, String> {
    let metadata =
        fs::metadata(path).map_err(|_| "Could not inspect the backup file".to_string())?;
    let modified = metadata
        .modified()
        .map_err(|_| "Could not read the backup timestamp".to_string())?;
    let created_at: chrono::DateTime<Utc> = modified.into();
    Ok(BackupInfo {
        id,
        created_at: created_at.to_rfc3339(),
        size_bytes: metadata.len(),
    })
}

fn validate_backup_connection(connection: &Connection) -> Result<(), String> {
    let result: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|_| "The backup database could not be checked".to_string())?;
    if result != "ok" {
        return Err("This backup failed SQLite's integrity check".to_string());
    }
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|_| "The backup database version could not be read".to_string())?;
    if version < 1 || version > SCHEMA_VERSION {
        return Err("This backup uses an unsupported DevHUD database version".to_string());
    }
    Ok(())
}

fn table_count(connection: &Connection, table: &str) -> Result<u64, String> {
    let query = match table {
        "sprints" => "SELECT count(*) FROM sprints",
        "work_items" => "SELECT count(*) FROM work_items",
        "reminders" => "SELECT count(*) FROM reminders",
        _ => return Err("Unsupported backup data type".to_string()),
    };
    connection
        .query_row(query, [], |row| row.get(0))
        .map_err(|_| "The backup is missing expected DevHUD data".to_string())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReminderOccurrence {
    pub occurrence_id: String,
    pub reminder_id: String,
    pub work_item_id: Option<String>,
    pub title: String,
    pub scheduled_at: String,
}

fn read_work_item(row: &Row<'_>) -> rusqlite::Result<WorkItem> {
    Ok(WorkItem {
        id: row.get(0)?,
        sprint_id: row.get(1)?,
        kind: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        status: row.get(5)?,
        priority: row.get(6)?,
        due_at: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        started_at: row.get(10)?,
        completed_at: row.get(11)?,
        source_item_id: row.get(12)?,
    })
}

fn validate_text(value: &str, field: &str, max_length: usize) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{field} is required"));
    }
    if value.chars().count() > max_length {
        return Err(format!("{field} must be {max_length} characters or fewer"));
    }
    Ok(value.to_string())
}

fn validate_date(value: &str, field: &str) -> Result<String, String> {
    let value = validate_text(value, field, 10)?;
    NaiveDate::parse_from_str(&value, "%Y-%m-%d")
        .map_err(|_| format!("{field} must be a valid calendar date"))?;
    Ok(value)
}

#[tauri::command]
fn get_sprints(database: tauri::State<'_, AppDatabase>) -> Result<Vec<Sprint>, String> {
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT id, name, goal, start_at, end_at, status, created_at, archived_at
             FROM sprints ORDER BY status = 'active' DESC, start_at DESC",
        )
        .map_err(|_| "Could not load sprints".to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(Sprint {
                id: row.get(0)?,
                name: row.get(1)?,
                goal: row.get(2)?,
                start_at: row.get(3)?,
                end_at: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
                archived_at: row.get(7)?,
            })
        })
        .map_err(|_| "Could not load sprints".to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Could not read sprint data".to_string())
}

#[tauri::command]
fn create_sprint(
    name: String,
    goal: String,
    start_at: String,
    end_at: String,
    database: tauri::State<'_, AppDatabase>,
) -> Result<Sprint, String> {
    let name = validate_text(&name, "Sprint name", 120)?;
    let goal = goal.trim().to_string();
    if goal.chars().count() > 2_000 {
        return Err("Sprint goal must be 2000 characters or fewer".to_string());
    }
    let start_at = validate_date(&start_at, "Start date")?;
    let end_at = validate_date(&end_at, "End date")?;
    if start_at > end_at {
        return Err("Sprint end date must be on or after its start date".to_string());
    }

    let sprint = Sprint {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        goal,
        start_at,
        end_at,
        status: "active".to_string(),
        created_at: Utc::now().to_rfc3339(),
        archived_at: None,
    };
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    connection
        .execute(
            "INSERT INTO sprints (id, name, goal, start_at, end_at, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6)",
            params![
                sprint.id,
                sprint.name,
                sprint.goal,
                sprint.start_at,
                sprint.end_at,
                sprint.created_at
            ],
        )
        .map_err(|_| {
            "Could not create sprint. Archive the current sprint before creating another."
                .to_string()
        })?;
    Ok(sprint)
}

#[tauri::command]
fn archive_sprint(id: String, database: tauri::State<'_, AppDatabase>) -> Result<(), String> {
    let mut connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    archive_sprint_record(&mut connection, &id)
}

fn archive_sprint_record(connection: &mut Connection, id: &str) -> Result<(), String> {
    let transaction = connection
        .transaction()
        .map_err(|_| "Could not start archiving sprint".to_string())?;
    let sprint = transaction
        .query_row(
            "SELECT id, name, goal, start_at, end_at, status, created_at, archived_at
             FROM sprints WHERE id = ?1 AND status = 'active'",
            params![id],
            |row| {
                Ok(Sprint {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    goal: row.get(2)?,
                    start_at: row.get(3)?,
                    end_at: row.get(4)?,
                    status: row.get(5)?,
                    created_at: row.get(6)?,
                    archived_at: row.get(7)?,
                })
            },
        )
        .map_err(|_| "The active sprint could not be found".to_string())?;
    let items = {
        let mut statement = transaction
            .prepare(
                "SELECT id, sprint_id, kind, title, description, status, priority, due_at,
                        created_at, updated_at, started_at, completed_at, source_item_id
                 FROM work_items WHERE sprint_id = ?1 ORDER BY created_at",
            )
            .map_err(|_| "Could not prepare sprint archive".to_string())?;
        let rows = statement
            .query_map(params![id], read_work_item)
            .map_err(|_| "Could not load sprint work for archive".to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Could not read sprint work for archive".to_string())?
    };
    let archived_at = Utc::now().to_rfc3339();
    let snapshot = ArchivedSprint {
        sprint: Sprint {
            status: "archived".to_string(),
            archived_at: Some(archived_at.clone()),
            ..sprint
        },
        work_items: items,
        archived_at: archived_at.clone(),
    };
    let snapshot_json = serde_json::to_string(&snapshot)
        .map_err(|_| "Could not prepare the sprint archive".to_string())?;
    transaction
        .execute(
            "INSERT INTO sprint_archives (sprint_id, snapshot_json, archived_at, schema_version)
             VALUES (?1, ?2, ?3, 1)",
            params![id, snapshot_json, archived_at],
        )
        .map_err(|_| "Could not preserve the sprint snapshot".to_string())?;
    transaction
        .execute(
            "UPDATE sprints SET status = 'archived', archived_at = ?1
             WHERE id = ?2 AND status = 'active'",
            params![archived_at, id],
        )
        .map_err(|_| "Could not archive sprint".to_string())?;
    transaction
        .commit()
        .map_err(|_| "Could not finish archiving sprint".to_string())
}

#[tauri::command]
fn search_sprint_history(
    query: String,
    database: tauri::State<'_, AppDatabase>,
) -> Result<Vec<ArchivedSprint>, String> {
    let query = query.trim();
    if query.chars().count() > 200 {
        return Err("Search query must be 200 characters or fewer".to_string());
    }
    let pattern = format!("%{query}%");
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT a.snapshot_json FROM sprint_archives a
             JOIN sprints s ON s.id = a.sprint_id
             WHERE ?1 = ''
                OR s.name LIKE ?2 COLLATE NOCASE
                OR s.goal LIKE ?2 COLLATE NOCASE
                OR a.snapshot_json LIKE ?2 COLLATE NOCASE
             ORDER BY a.archived_at DESC",
        )
        .map_err(|_| "Could not prepare sprint history search".to_string())?;
    let rows = statement
        .query_map(params![query, pattern], |row| row.get::<_, String>(0))
        .map_err(|_| "Could not search sprint history".to_string())?;
    rows.map(|row| {
        let json = row.map_err(|_| "Could not read sprint history".to_string())?;
        serde_json::from_str(&json).map_err(|_| "A sprint archive could not be read".to_string())
    })
    .collect()
}

#[tauri::command]
fn carry_over_work_item(
    source_item_id: String,
    database: tauri::State<'_, AppDatabase>,
) -> Result<WorkItem, String> {
    let mut connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|_| "Could not start carrying over this item".to_string())?;
    let source = transaction
        .query_row(
            "SELECT id, kind, title, description, priority, due_at
             FROM work_items WHERE id = ?1 AND status NOT IN ('done', 'cancelled')",
            params![source_item_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .map_err(|_| "The unfinished source item could not be found".to_string())?;
    let target_sprint: Option<String> = transaction
        .query_row(
            "SELECT id FROM sprints WHERE status = 'active'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "Could not find the active sprint".to_string())?;
    let Some(target_sprint) = target_sprint else {
        return Err("Create an active sprint before carrying over work".to_string());
    };
    let existing: Option<String> = transaction
        .query_row(
            "SELECT id FROM work_items WHERE source_item_id = ?1 AND sprint_id = ?2",
            params![source.0, target_sprint],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "Could not check for existing carry-over".to_string())?;
    if existing.is_some() {
        return Err("This item has already been carried into the active sprint".to_string());
    }
    let now = Utc::now().to_rfc3339();
    let carried = WorkItem {
        id: uuid::Uuid::new_v4().to_string(),
        sprint_id: Some(target_sprint),
        kind: source.1,
        title: source.2,
        description: source.3,
        status: "todo".to_string(),
        priority: source.4,
        due_at: source.5,
        created_at: now.clone(),
        updated_at: now,
        started_at: None,
        completed_at: None,
        source_item_id: Some(source.0),
    };
    transaction
        .execute(
            "INSERT INTO work_items
             (id, sprint_id, kind, title, description, status, priority, due_at,
              created_at, updated_at, source_item_id)
             VALUES (?1, ?2, ?3, ?4, ?5, 'todo', ?6, ?7, ?8, ?9, ?10)",
            params![
                carried.id,
                carried.sprint_id,
                carried.kind,
                carried.title,
                carried.description,
                carried.priority,
                carried.due_at,
                carried.created_at,
                carried.updated_at,
                carried.source_item_id
            ],
        )
        .map_err(|_| "Could not carry over this work item".to_string())?;
    transaction
        .commit()
        .map_err(|_| "Could not finish carrying over this work item".to_string())?;
    Ok(carried)
}

#[tauri::command]
fn get_sprint_summary(
    sprint_id: String,
    database: tauri::State<'_, AppDatabase>,
) -> Result<SprintSummary, String> {
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let snapshot: Option<String> = connection
        .query_row(
            "SELECT snapshot_json FROM sprint_archives WHERE sprint_id = ?1",
            params![sprint_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "Could not load the sprint summary".to_string())?;
    let archived = if let Some(snapshot) = snapshot {
        serde_json::from_str::<ArchivedSprint>(&snapshot)
            .map_err(|_| "The sprint archive could not be read".to_string())?
    } else {
        let sprint = connection
            .query_row(
                "SELECT id, name, goal, start_at, end_at, status, created_at, archived_at
                 FROM sprints WHERE id = ?1",
                params![sprint_id],
                |row| {
                    Ok(Sprint {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        goal: row.get(2)?,
                        start_at: row.get(3)?,
                        end_at: row.get(4)?,
                        status: row.get(5)?,
                        created_at: row.get(6)?,
                        archived_at: row.get(7)?,
                    })
                },
            )
            .map_err(|_| "The sprint could not be found".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT id, sprint_id, kind, title, description, status, priority, due_at,
                        created_at, updated_at, started_at, completed_at, source_item_id
                 FROM work_items WHERE sprint_id = ?1",
            )
            .map_err(|_| "Could not load sprint work".to_string())?;
        let rows = statement
            .query_map(params![sprint_id], read_work_item)
            .map_err(|_| "Could not load sprint work".to_string())?;
        ArchivedSprint {
            sprint,
            work_items: rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "Could not read sprint work".to_string())?,
            archived_at: String::new(),
        }
    };
    let titles = |predicate: fn(&WorkItem) -> bool| {
        archived
            .work_items
            .iter()
            .filter(|item| predicate(item))
            .map(|item| item.title.clone())
            .collect::<Vec<_>>()
    };
    Ok(SprintSummary {
        sprint: archived.sprint,
        completed: titles(|item| item.status == "done"),
        in_progress: titles(|item| item.status == "in_progress"),
        blocked: titles(|item| item.status == "blocked" || item.kind == "blocker"),
        remaining: titles(|item| {
            item.status != "done"
                && item.status != "cancelled"
                && item.status != "blocked"
                && item.kind != "blocker"
        }),
        carried_over: titles(|item| item.source_item_id.is_some()),
    })
}

#[tauri::command]
fn create_backup(
    app: tauri::AppHandle,
    database: tauri::State<'_, AppDatabase>,
) -> Result<BackupInfo, String> {
    let directory = backups_directory(&app)?;
    let id = format!("backup-{}", uuid::Uuid::new_v4());
    let path = backup_path(&directory, &id)?;
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    create_database_backup(&connection, &path)?;
    backup_info(id, &path)
}

#[tauri::command]
fn list_backups(app: tauri::AppHandle) -> Result<Vec<BackupInfo>, String> {
    let directory = backups_directory(&app)?;
    let entries =
        fs::read_dir(&directory).map_err(|_| "Could not read the backup list".to_string())?;
    let mut backups = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| "Could not read the backup list".to_string())?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(id) = file_name.strip_suffix(".sqlite3") else {
            continue;
        };
        if !id.starts_with("backup-") || backup_path(&directory, id).is_err() {
            continue;
        }
        backups.push(backup_info(id.to_string(), &path)?);
    }
    backups.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(backups)
}

#[tauri::command]
fn preview_backup(app: tauri::AppHandle, id: String) -> Result<BackupPreview, String> {
    let directory = backups_directory(&app)?;
    let path = backup_path(&directory, &id)?;
    let connection = Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| "This backup file could not be opened".to_string())?;
    validate_backup_connection(&connection)?;
    Ok(BackupPreview {
        backup: backup_info(id, &path)?,
        sprints: table_count(&connection, "sprints")?,
        work_items: table_count(&connection, "work_items")?,
        reminders: table_count(&connection, "reminders")?,
        integrity_ok: true,
    })
}

#[tauri::command]
fn restore_backup(
    app: tauri::AppHandle,
    id: String,
    database: tauri::State<'_, AppDatabase>,
) -> Result<RestoreResult, String> {
    let directory = backups_directory(&app)?;
    let source_path = backup_path(&directory, &id)?;
    let restored_connection = Connection::open(&source_path)
        .map_err(|_| "This backup file could not be opened".to_string())?;
    validate_backup_connection(&restored_connection)?;
    migrate(&restored_connection)?;
    let preview = BackupPreview {
        backup: backup_info(id.clone(), &source_path)?,
        sprints: table_count(&restored_connection, "sprints")?,
        work_items: table_count(&restored_connection, "work_items")?,
        reminders: table_count(&restored_connection, "reminders")?,
        integrity_ok: true,
    };

    let mut current = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let safety_id = format!("backup-{}", uuid::Uuid::new_v4());
    let safety_path = backup_path(&directory, &safety_id)?;
    create_database_backup(&current, &safety_path)?;
    let safety_backup = backup_info(safety_id, &safety_path)?;
    restore_database(&restored_connection, &mut current)?;
    drop(current);
    Ok(RestoreResult {
        restored_from: format!(
            "{} sprints, {} work items, {} reminders",
            preview.sprints, preview.work_items, preview.reminders
        ),
        safety_backup,
    })
}

#[tauri::command]
fn export_productivity_data(database: tauri::State<'_, AppDatabase>) -> Result<String, String> {
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let sprints = {
        let mut statement = connection
            .prepare(
                "SELECT id, name, goal, start_at, end_at, status, created_at, archived_at
                 FROM sprints ORDER BY start_at",
            )
            .map_err(|_| "Could not prepare productivity export".to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok(Sprint {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    goal: row.get(2)?,
                    start_at: row.get(3)?,
                    end_at: row.get(4)?,
                    status: row.get(5)?,
                    created_at: row.get(6)?,
                    archived_at: row.get(7)?,
                })
            })
            .map_err(|_| "Could not read sprints for export".to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Could not read sprints for export".to_string())?
    };
    let work_items = {
        let mut statement = connection
            .prepare(
                "SELECT id, sprint_id, kind, title, description, status, priority, due_at,
                        created_at, updated_at, started_at, completed_at, source_item_id
                 FROM work_items ORDER BY created_at",
            )
            .map_err(|_| "Could not prepare productivity export".to_string())?;
        let rows = statement
            .query_map([], read_work_item)
            .map_err(|_| "Could not read work items for export".to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Could not read work items for export".to_string())?
    };
    let reminders = {
        let mut statement = connection
            .prepare(
                "SELECT id, work_item_id, title, trigger_at, timezone, repeat_rule, enabled
                 FROM reminders ORDER BY trigger_at",
            )
            .map_err(|_| "Could not prepare productivity export".to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok(Reminder {
                    id: row.get(0)?,
                    work_item_id: row.get(1)?,
                    title: row.get(2)?,
                    trigger_at: row.get(3)?,
                    timezone: row.get(4)?,
                    repeat_rule: row.get(5)?,
                    enabled: row.get(6)?,
                })
            })
            .map_err(|_| "Could not read reminders for export".to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Could not read reminders for export".to_string())?
    };
    serde_json::to_string_pretty(&ProductivityExport {
        format_version: 1,
        exported_at: Utc::now().to_rfc3339(),
        sprints,
        work_items,
        reminders,
    })
    .map_err(|_| "Could not generate the productivity export".to_string())
}

#[tauri::command]
fn get_work_items(
    sprint_id: Option<String>,
    database: tauri::State<'_, AppDatabase>,
) -> Result<Vec<WorkItem>, String> {
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT id, sprint_id, kind, title, description, status, priority, due_at,
                    created_at, updated_at, started_at, completed_at, source_item_id
             FROM work_items
             WHERE (?1 IS NULL OR sprint_id = ?1)
             ORDER BY
               CASE status WHEN 'in_progress' THEN 0 WHEN 'blocked' THEN 1 ELSE 2 END,
               due_at IS NULL, due_at, created_at DESC",
        )
        .map_err(|_| "Could not load work items".to_string())?;
    let rows = statement
        .query_map(params![sprint_id], read_work_item)
        .map_err(|_| "Could not load work items".to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Could not read work item data".to_string())
}

#[tauri::command]
fn add_work_item(
    item: NewWorkItem,
    database: tauri::State<'_, AppDatabase>,
) -> Result<WorkItem, String> {
    let title = validate_text(&item.title, "Title", 500)?;
    let description = item.description.trim().to_string();
    if description.chars().count() > 10_000 {
        return Err("Description must be 10000 characters or fewer".to_string());
    }
    let now = Utc::now().to_rfc3339();
    let work_item = WorkItem {
        id: uuid::Uuid::new_v4().to_string(),
        sprint_id: item.sprint_id,
        kind: item.kind.as_str().to_string(),
        title,
        description,
        status: if item.kind == WorkItemKind::Blocker {
            "blocked".to_string()
        } else {
            "todo".to_string()
        },
        priority: item.priority.as_str().to_string(),
        due_at: item.due_at,
        created_at: now.clone(),
        updated_at: now,
        started_at: None,
        completed_at: None,
        source_item_id: None,
    };
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    connection
        .execute(
            "INSERT INTO work_items
             (id, sprint_id, kind, title, description, status, priority, due_at,
              created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                work_item.id,
                work_item.sprint_id,
                work_item.kind,
                work_item.title,
                work_item.description,
                work_item.status,
                work_item.priority,
                work_item.due_at,
                work_item.created_at,
                work_item.updated_at
            ],
        )
        .map_err(|_| {
            "Could not save this item. Check that the selected sprint still exists.".to_string()
        })?;
    Ok(work_item)
}

#[tauri::command]
fn set_work_item_status(
    id: String,
    status: WorkItemStatus,
    database: tauri::State<'_, AppDatabase>,
) -> Result<WorkItem, String> {
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let now = Utc::now().to_rfc3339();
    let changed = connection
        .execute(
            "UPDATE work_items
             SET status = ?1,
                 updated_at = ?2,
                 started_at = CASE WHEN ?1 = 'in_progress' AND started_at IS NULL THEN ?2 ELSE started_at END,
                 completed_at = CASE WHEN ?1 = 'done' THEN ?2 WHEN ?1 != 'done' THEN NULL ELSE completed_at END
             WHERE id = ?3",
            params![status.as_str(), now, id],
        )
        .map_err(|_| "Could not update item status".to_string())?;
    if changed == 0 {
        return Err("The item could not be found".to_string());
    }
    connection
        .query_row(
            "SELECT id, sprint_id, kind, title, description, status, priority, due_at,
                    created_at, updated_at, started_at, completed_at, source_item_id
             FROM work_items WHERE id = ?1",
            params![id],
            read_work_item,
        )
        .map_err(|_| "Could not reload the updated item".to_string())
}

#[tauri::command]
fn delete_work_item(id: String, database: tauri::State<'_, AppDatabase>) -> Result<(), String> {
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let changed = connection
        .execute("DELETE FROM work_items WHERE id = ?1", params![id])
        .map_err(|_| "Could not delete item".to_string())?;
    if changed == 0 {
        return Err("The item could not be found".to_string());
    }
    Ok(())
}

#[tauri::command]
fn create_reminder(
    reminder: NewReminder,
    database: tauri::State<'_, AppDatabase>,
) -> Result<Reminder, String> {
    let title = validate_text(&reminder.title, "Reminder title", 500)?;
    let trigger_at = chrono::DateTime::parse_from_rfc3339(&reminder.trigger_at)
        .map_err(|_| "Reminder time must be a valid ISO date and time".to_string())?
        .with_timezone(&Utc)
        .to_rfc3339();
    let timezone = validate_text(&reminder.timezone, "Timezone", 100)?;
    let reminder = Reminder {
        id: uuid::Uuid::new_v4().to_string(),
        work_item_id: reminder.work_item_id,
        title,
        trigger_at,
        timezone,
        repeat_rule: None,
        enabled: true,
    };
    let mut connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|_| "Could not start saving the reminder".to_string())?;
    transaction
        .execute(
            "INSERT INTO reminders
             (id, work_item_id, title, trigger_at, timezone, repeat_rule, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                reminder.id,
                reminder.work_item_id,
                reminder.title,
                reminder.trigger_at,
                reminder.timezone,
                reminder.repeat_rule,
                reminder.enabled
            ],
        )
        .map_err(|_| {
            "Could not save reminder. Check that its linked item still exists.".to_string()
        })?;
    transaction
        .execute(
            "INSERT INTO reminder_occurrences (id, reminder_id, scheduled_at)
             VALUES (?1, ?2, ?3)",
            params![
                uuid::Uuid::new_v4().to_string(),
                reminder.id,
                reminder.trigger_at
            ],
        )
        .map_err(|_| "Could not schedule this reminder".to_string())?;
    transaction
        .commit()
        .map_err(|_| "Could not finish saving the reminder".to_string())?;
    Ok(reminder)
}

#[tauri::command]
fn get_reminders(database: tauri::State<'_, AppDatabase>) -> Result<Vec<Reminder>, String> {
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT id, work_item_id, title, trigger_at, timezone, repeat_rule, enabled
             FROM reminders ORDER BY trigger_at",
        )
        .map_err(|_| "Could not load reminders".to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(Reminder {
                id: row.get(0)?,
                work_item_id: row.get(1)?,
                title: row.get(2)?,
                trigger_at: row.get(3)?,
                timezone: row.get(4)?,
                repeat_rule: row.get(5)?,
                enabled: row.get(6)?,
            })
        })
        .map_err(|_| "Could not load reminders".to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Could not read reminder data".to_string())
}

fn due_reminders(database: &AppDatabase) -> Result<Vec<ReminderOccurrence>, String> {
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT o.id, r.id, r.work_item_id, r.title, o.scheduled_at
             FROM reminder_occurrences o
             JOIN reminders r ON r.id = o.reminder_id
             WHERE r.enabled = 1
               AND o.delivered_at IS NULL
               AND o.dismissed_at IS NULL
               AND o.completed_at IS NULL
               AND COALESCE(o.snoozed_until, o.scheduled_at) <= ?1
             ORDER BY COALESCE(o.snoozed_until, o.scheduled_at)",
        )
        .map_err(|_| "Could not check due reminders".to_string())?;
    let rows = statement
        .query_map(params![Utc::now().to_rfc3339()], |row| {
            Ok(ReminderOccurrence {
                occurrence_id: row.get(0)?,
                reminder_id: row.get(1)?,
                work_item_id: row.get(2)?,
                title: row.get(3)?,
                scheduled_at: row.get(4)?,
            })
        })
        .map_err(|_| "Could not check due reminders".to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Could not read due reminders".to_string())
}

fn mark_reminder_delivered(database: &AppDatabase, occurrence_id: &str) -> Result<bool, String> {
    let mut connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|_| "Could not update reminder state".to_string())?;
    let changed = transaction
        .execute(
            "UPDATE reminder_occurrences
             SET delivered_at = ?1
             WHERE id = ?2 AND delivered_at IS NULL AND dismissed_at IS NULL AND completed_at IS NULL",
            params![Utc::now().to_rfc3339(), occurrence_id],
        )
        .map_err(|_| "Could not update reminder state".to_string())?;
    if changed == 0 {
        return Ok(false);
    }
    transaction
        .execute(
            "UPDATE reminders SET enabled = 0 WHERE id = (
                SELECT reminder_id FROM reminder_occurrences WHERE id = ?1
             )",
            params![occurrence_id],
        )
        .map_err(|_| "Could not finish updating reminder state".to_string())?;
    transaction
        .commit()
        .map_err(|_| "Could not finish updating reminder state".to_string())?;
    Ok(true)
}

#[tauri::command]
fn get_due_reminders(
    database: tauri::State<'_, AppDatabase>,
) -> Result<Vec<ReminderOccurrence>, String> {
    due_reminders(&database)
}

#[tauri::command]
fn snooze_reminder(
    occurrence_id: String,
    minutes: u32,
    database: tauri::State<'_, AppDatabase>,
) -> Result<(), String> {
    if !matches!(minutes, 5 | 15 | 60) {
        return Err("Choose a snooze interval of 5, 15, or 60 minutes".to_string());
    }
    let snoozed_until = (Utc::now() + Duration::minutes(i64::from(minutes))).to_rfc3339();
    let mut connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|_| "Could not start snoozing reminder".to_string())?;
    let changed = transaction
        .execute(
            "UPDATE reminder_occurrences
             SET snoozed_until = ?1, delivered_at = NULL
             WHERE id = ?2 AND dismissed_at IS NULL AND completed_at IS NULL",
            params![snoozed_until, occurrence_id],
        )
        .map_err(|_| "Could not snooze reminder".to_string())?;
    if changed == 0 {
        return Err("This reminder is no longer available to snooze".to_string());
    }
    transaction
        .execute(
            "UPDATE reminders SET enabled = 1 WHERE id = (
                SELECT reminder_id FROM reminder_occurrences WHERE id = ?1
             )",
            params![occurrence_id],
        )
        .map_err(|_| "Could not finish snoozing reminder".to_string())?;
    transaction
        .commit()
        .map_err(|_| "Could not finish snoozing reminder".to_string())
}

#[tauri::command]
fn dismiss_reminder(
    occurrence_id: String,
    database: tauri::State<'_, AppDatabase>,
) -> Result<(), String> {
    let mut connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|_| "Could not update reminder state".to_string())?;
    let changed = transaction
        .execute(
            "UPDATE reminder_occurrences SET dismissed_at = ?1
             WHERE id = ?2 AND dismissed_at IS NULL AND completed_at IS NULL",
            params![Utc::now().to_rfc3339(), occurrence_id],
        )
        .map_err(|_| "Could not dismiss reminder".to_string())?;
    if changed == 0 {
        return Err("This reminder is no longer available to dismiss".to_string());
    }
    transaction
        .execute(
            "UPDATE reminders SET enabled = 0 WHERE id = (
                SELECT reminder_id FROM reminder_occurrences WHERE id = ?1
             )",
            params![occurrence_id],
        )
        .map_err(|_| "Could not finish dismissing reminder".to_string())?;
    transaction
        .commit()
        .map_err(|_| "Could not finish dismissing reminder".to_string())
}

#[tauri::command]
fn cancel_reminder(id: String, database: tauri::State<'_, AppDatabase>) -> Result<(), String> {
    let mut connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|_| "Could not update reminder state".to_string())?;
    let changed = transaction
        .execute(
            "UPDATE reminders SET enabled = 0 WHERE id = ?1 AND enabled = 1",
            params![id],
        )
        .map_err(|_| "Could not cancel reminder".to_string())?;
    if changed == 0 {
        return Err("This reminder is no longer active".to_string());
    }
    transaction
        .execute(
            "UPDATE reminder_occurrences SET dismissed_at = ?1
             WHERE reminder_id = ?2 AND dismissed_at IS NULL AND completed_at IS NULL",
            params![Utc::now().to_rfc3339(), id],
        )
        .map_err(|_| "Could not finish cancelling reminder".to_string())?;
    transaction
        .commit()
        .map_err(|_| "Could not finish cancelling reminder".to_string())
}

#[tauri::command]
fn vault_status(database: tauri::State<'_, AppDatabase>) -> Result<bool, String> {
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    connection
        .query_row("SELECT 1 FROM vault_config WHERE singleton = 1", [], |_| {
            Ok(())
        })
        .optional()
        .map(|config| config.is_some())
        .map_err(|_| "Could not check the vault status".to_string())
}

#[tauri::command]
fn vault_setup(
    database: tauri::State<'_, AppDatabase>,
    session: tauri::State<'_, VaultSession>,
    password: String,
) -> Result<(), String> {
    let password = Zeroizing::new(password);
    if password.chars().count() < 12 {
        return Err("The master password must contain at least 12 characters".to_string());
    }
    if password.chars().count() > 1024 {
        return Err("The master password is too long".to_string());
    }
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let key = derive_vault_key(&password, &salt)?;
    let verifier_cipher = encrypt_vault_bytes(&*key, &nonce, VAULT_VERIFIER, VAULT_VERIFIER_AAD)?;
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| "Could not start vault setup".to_string())?;
    let configured: bool = transaction
        .query_row("SELECT EXISTS(SELECT 1 FROM vault_config)", [], |row| {
            row.get(0)
        })
        .map_err(|_| "Could not check vault setup".to_string())?;
    if configured {
        return Err("The vault has already been configured".to_string());
    }
    transaction
        .execute(
            "INSERT INTO vault_config
             (singleton, salt, memory_kib, iterations, parallelism, verifier_nonce, verifier_cipher,
              kdf_format_version, kdf_algorithm, argon_version)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, 'argon2id', 19)",
            params![
                salt.as_slice(),
                VAULT_KDF_MEMORY_KIB,
                VAULT_KDF_ITERATIONS,
                VAULT_KDF_PARALLELISM,
                nonce.as_slice(),
                verifier_cipher,
                VAULT_KDF_FORMAT_VERSION
            ],
        )
        .map_err(|_| "Could not save vault configuration".to_string())?;
    transaction
        .commit()
        .map_err(|_| "Could not finish vault setup".to_string())?;
    session.unlock(key)
}

#[tauri::command]
fn vault_unlock(
    database: tauri::State<'_, AppDatabase>,
    session: tauri::State<'_, VaultSession>,
    password: String,
) -> Result<(), String> {
    let password = Zeroizing::new(password);
    let (
        salt,
        memory_kib,
        iterations,
        parallelism,
        format_version,
        algorithm,
        argon_version,
        nonce,
        ciphertext,
    ): (Vec<u8>, u32, u32, u32, u32, String, u32, Vec<u8>, Vec<u8>) = {
        let connection = database
            .connection
            .lock()
            .map_err(|_| "The local database is temporarily unavailable".to_string())?;
        connection
            .query_row(
                "SELECT salt, memory_kib, iterations, parallelism, kdf_format_version,
                        kdf_algorithm, argon_version, verifier_nonce, verifier_cipher
                 FROM vault_config WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                    ))
                },
            )
            .map_err(|_| "The vault has not been configured".to_string())?
    };
    if memory_kib != VAULT_KDF_MEMORY_KIB
        || iterations != VAULT_KDF_ITERATIONS
        || parallelism != VAULT_KDF_PARALLELISM
        || format_version != VAULT_KDF_FORMAT_VERSION
        || algorithm != "argon2id"
        || argon_version != 19
    {
        return Err("This vault uses unsupported key-derivation parameters".to_string());
    }
    let key = derive_vault_key(&password, &salt)?;
    let verifier = decrypt_vault_bytes(&*key, &nonce, &ciphertext, VAULT_VERIFIER_AAD)
        .map_err(|_| "Incorrect master password".to_string())?;
    if verifier.as_slice() != VAULT_VERIFIER {
        return Err("Incorrect master password".to_string());
    }
    session.unlock(key)
}

#[tauri::command]
fn vault_lock(session: tauri::State<'_, VaultSession>) -> Result<(), String> {
    session.lock()
}

#[tauri::command]
fn vault_is_unlocked(session: tauri::State<'_, VaultSession>) -> Result<bool, String> {
    session.is_unlocked()
}

fn decrypt_vault_item(
    session: &VaultSession,
    id: &str,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<VaultPayload, String> {
    session.with_key(|key| {
        let plaintext = decrypt_vault_bytes(key, nonce, ciphertext, id.as_bytes())?;
        serde_json::from_slice(&plaintext)
            .map_err(|_| "Vault item could not be decoded".to_string())
    })
}

#[tauri::command]
fn add_vault_item(
    database: tauri::State<'_, AppDatabase>,
    session: tauri::State<'_, VaultSession>,
    item: NewVaultItem,
) -> Result<String, String> {
    validate_vault_text(&item.kind, "Type", 40)?;
    validate_vault_text(&item.title, "Title", 200)?;
    validate_vault_text(&item.url, "URL", 2048)?;
    validate_vault_text(&item.username, "Username", 512)?;
    validate_vault_text(&item.password, "Password", 4096)?;
    validate_vault_text(&item.notes, "Notes", 10_000)?;
    if item.title.trim().is_empty() {
        return Err("Title is required".to_string());
    }
    if item.tags.len() > 30 || item.tags.iter().any(|tag| tag.chars().count() > 50) {
        return Err("Use at most 30 tags, each 50 characters or fewer".to_string());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let payload = VaultPayload {
        kind: item.kind,
        title: item.title,
        url: item.url,
        username: item.username,
        password: item.password,
        notes: item.notes,
        tags: item.tags,
    };
    let serialized = Zeroizing::new(
        serde_json::to_vec(&payload).map_err(|_| "Could not encode vault item".to_string())?,
    );
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext =
        session.with_key(|key| encrypt_vault_bytes(key, &nonce, &serialized, id.as_bytes()))?;
    drop(payload);
    let now = Utc::now().to_rfc3339();
    let connection = database
        .connection
        .lock()
        .map_err(|_| "The local database is temporarily unavailable".to_string())?;
    connection
        .execute(
            "INSERT INTO vault_items (id, nonce, ciphertext, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            params![id, nonce.as_slice(), ciphertext, now],
        )
        .map_err(|_| "Could not save the encrypted vault item".to_string())?;
    Ok(id)
}

#[tauri::command]
fn search_vault_items(
    database: tauri::State<'_, AppDatabase>,
    session: tauri::State<'_, VaultSession>,
    query: String,
) -> Result<Vec<VaultSearchResult>, String> {
    validate_vault_text(&query, "Search", 200)?;
    session.with_key(|_| Ok(()))?;
    let items: Vec<(String, Vec<u8>, Vec<u8>, String, String)> = {
        let connection = database
            .connection
            .lock()
            .map_err(|_| "The local database is temporarily unavailable".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT id, nonce, ciphertext, created_at, updated_at FROM vault_items
                 ORDER BY updated_at DESC",
            )
            .map_err(|_| "Could not prepare vault search".to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .map_err(|_| "Could not search vault items".to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Could not read vault items".to_string())?
    };
    let needle = query.to_lowercase();
    let mut results = Vec::new();
    for (id, nonce, ciphertext, created_at, updated_at) in items {
        let payload = decrypt_vault_item(session.inner(), &id, &nonce, &ciphertext)?;
        let searchable = format!(
            "{} {} {} {} {}",
            payload.kind,
            payload.title,
            payload.url,
            payload.username,
            payload.tags.join(" ")
        )
        .to_lowercase();
        if needle.is_empty() || searchable.contains(&needle) {
            results.push(VaultSearchResult {
                id,
                kind: payload.kind.clone(),
                title: payload.title.clone(),
                url: payload.url.clone(),
                username: payload.username.clone(),
                tags: payload.tags.clone(),
                created_at,
                updated_at,
            });
        }
    }
    Ok(results)
}

#[tauri::command]
fn delete_vault_item(
    database: tauri::State<'_, AppDatabase>,
    session: tauri::State<'_, VaultSession>,
    id: String,
) -> Result<bool, String> {
    uuid::Uuid::parse_str(&id).map_err(|_| "Invalid vault item identifier".to_string())?;
    session.with_key(|_| {
        let connection = database
            .connection
            .lock()
            .map_err(|_| "The local database is temporarily unavailable".to_string())?;
        connection
            .execute("DELETE FROM vault_items WHERE id = ?1", [id])
            .map(|changed| changed > 0)
            .map_err(|_| "Could not delete the vault item".to_string())
    })
}

#[tauri::command]
fn copy_vault_field(
    app: tauri::AppHandle,
    database: tauri::State<'_, AppDatabase>,
    session: tauri::State<'_, VaultSession>,
    id: String,
    field: VaultField,
) -> Result<(), String> {
    uuid::Uuid::parse_str(&id).map_err(|_| "Invalid vault item identifier".to_string())?;
    let (nonce, ciphertext): (Vec<u8>, Vec<u8>) = {
        let connection = database
            .connection
            .lock()
            .map_err(|_| "The local database is temporarily unavailable".to_string())?;
        connection
            .query_row(
                "SELECT nonce, ciphertext FROM vault_items WHERE id = ?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| "Vault item was not found".to_string())?
    };
    let mut payload = decrypt_vault_item(session.inner(), &id, &nonce, &ciphertext)?;
    let secret = Zeroizing::new(match field {
        VaultField::Username => payload.username.clone(),
        VaultField::Password => payload.password.clone(),
        VaultField::Notes => payload.notes.clone(),
    });
    payload.zeroize();
    app.clipboard()
        .write_secret(secret.as_str())
        .map_err(|_| "Could not copy the selected vault field".to_string())?;
    std::thread::spawn(move || {
        std::thread::sleep(VAULT_CLIPBOARD_CLEAR_AFTER);
        match app.clipboard().read_text() {
            Ok(current) => {
                let current = Zeroizing::new(current);
                if *current == *secret {
                    if let Err(error) = app.clipboard().clear() {
                        eprintln!("Could not clear the vault clipboard value: {error}");
                    }
                }
            }
            Err(error) => {
                eprintln!("Could not check whether the vault clipboard value is current: {error}")
            }
        }
    });
    Ok(())
}

fn start_reminder_scheduler(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        loop {
            if let Some(session) = app.try_state::<VaultSession>() {
                session.lock_if_idle();
            }
            if let Some(database) = app.try_state::<AppDatabase>() {
                match due_reminders(&database) {
                    Ok(reminders) => {
                        for reminder in reminders {
                            match app
                                .notification()
                                .builder()
                                .title("DevHUD reminder")
                                .body(&reminder.title)
                                .show()
                            {
                                Ok(()) => {
                                    match mark_reminder_delivered(
                                        &database,
                                        &reminder.occurrence_id,
                                    ) {
                                        Ok(true) => {
                                            if let Err(error) = app.emit("reminder-due", &reminder)
                                            {
                                                eprintln!(
                                                    "Could not notify the HUD about a due reminder: {error}"
                                                );
                                            }
                                        }
                                        Ok(false) => {}
                                        Err(error) => {
                                            eprintln!(
                                                "Could not persist reminder delivery: {error}"
                                            );
                                        }
                                    }
                                }
                                Err(error) => {
                                    eprintln!("Could not deliver a reminder notification: {error}");
                                    if let Err(emit_error) = app.emit(
                                    "reminder-delivery-error",
                                    "A reminder could not be delivered. Open DevHUD to review it.",
                                ) {
                                    eprintln!("Could not report reminder delivery failure: {emit_error}");
                                }
                                }
                            }
                        }
                    }
                    Err(error) => eprintln!("Could not check due reminders: {error}"),
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(15));
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            fs::create_dir_all(&app_data_dir)?;
            let database_path = app_data_dir.join("devhud.sqlite3");
            let database = AppDatabase::open(&database_path).map_err(std::io::Error::other)?;
            app.manage(database);
            app.manage(VaultSession::default());
            start_reminder_scheduler(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_sprints,
            create_sprint,
            archive_sprint,
            get_work_items,
            add_work_item,
            set_work_item_status,
            delete_work_item,
            create_reminder,
            get_reminders,
            search_sprint_history,
            carry_over_work_item,
            get_sprint_summary,
            create_backup,
            list_backups,
            preview_backup,
            restore_backup,
            export_productivity_data,
            get_due_reminders,
            snooze_reminder,
            dismiss_reminder,
            cancel_reminder,
            vault_status,
            vault_setup,
            vault_unlock,
            vault_lock,
            vault_is_unlocked,
            add_vault_item,
            search_vault_items,
            delete_vault_item,
            copy_vault_field
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_persists_sprints_and_work_item_statuses() {
        let database = AppDatabase::in_memory().expect("database should initialize");
        let connection = database
            .connection
            .lock()
            .expect("database should be available");
        connection
            .execute(
                "INSERT INTO sprints (id, name, goal, start_at, end_at, status, created_at)
                 VALUES ('sprint-1', 'Sprint 1', '', '2026-11-01', '2026-11-14', 'active', 'now')",
                [],
            )
            .expect("sprint should be inserted");
        connection
            .execute(
                "INSERT INTO work_items
                 (id, sprint_id, kind, title, status, priority, created_at, updated_at)
                 VALUES ('item-1', 'sprint-1', 'task', 'Test persistence', 'todo', 'normal', 'now', 'now')",
                [],
            )
            .expect("work item should be inserted");
        connection
            .execute(
                "UPDATE work_items SET status = 'done', completed_at = 'later' WHERE id = 'item-1'",
                [],
            )
            .expect("work item should be updated");

        let status: String = connection
            .query_row(
                "SELECT status FROM work_items WHERE id = 'item-1'",
                [],
                |row| row.get(0),
            )
            .expect("status should be queryable");
        assert_eq!(status, "done");
    }

    #[test]
    fn rejects_invalid_titles() {
        assert!(validate_text("  ", "Title", 500).is_err());
        assert!(validate_text(&"x".repeat(501), "Title", 500).is_err());
        assert_eq!(
            validate_text("  Keep this  ", "Title", 500).unwrap(),
            "Keep this"
        );
    }

    #[test]
    fn rejects_invalid_sprint_dates() {
        assert!(validate_date("2026-02-30", "Start date").is_err());
        assert_eq!(
            validate_date("2026-11-11", "Start date").unwrap(),
            "2026-11-11"
        );
    }

    #[test]
    fn vault_key_verifier_rejects_wrong_password_and_tampering() {
        let salt = [7u8; 16];
        let nonce = [9u8; 12];
        let key =
            derive_vault_key("correct horse battery staple", &salt).expect("key should derive");
        let encrypted = encrypt_vault_bytes(&key, &nonce, VAULT_VERIFIER, VAULT_VERIFIER_AAD)
            .expect("verifier should encrypt");
        let verified = decrypt_vault_bytes(&key, &nonce, &encrypted, VAULT_VERIFIER_AAD)
            .expect("verifier should decrypt");
        assert_eq!(verified.as_slice(), VAULT_VERIFIER);

        let wrong_key = derive_vault_key("incorrect password", &salt).expect("key should derive");
        assert!(decrypt_vault_bytes(&wrong_key, &nonce, &encrypted, VAULT_VERIFIER_AAD).is_err());
        let mut tampered = encrypted;
        tampered[0] ^= 0x80;
        assert!(decrypt_vault_bytes(&key, &nonce, &tampered, VAULT_VERIFIER_AAD).is_err());
    }

    #[test]
    fn vault_item_ciphertext_authenticates_record_id_and_hides_plaintext() {
        let key = derive_vault_key("correct horse battery staple", &[3u8; 16])
            .expect("key should derive");
        let id = uuid::Uuid::new_v4().to_string();
        let payload = VaultPayload {
            kind: "login".to_string(),
            title: "Work account".to_string(),
            url: "https://example.invalid".to_string(),
            username: "private-user".to_string(),
            password: "private-secret-9281".to_string(),
            notes: "recovery phrase".to_string(),
            tags: vec!["work".to_string()],
        };
        let plaintext =
            Zeroizing::new(serde_json::to_vec(&payload).expect("payload should serialize"));
        let nonce = [4u8; 12];
        let ciphertext = encrypt_vault_bytes(&key, &nonce, &plaintext, id.as_bytes())
            .expect("item should encrypt");
        assert!(
            !String::from_utf8_lossy(&ciphertext).contains("private-secret-9281"),
            "ciphertext must not expose the secret"
        );
        let decrypted = decrypt_vault_bytes(&key, &nonce, &ciphertext, id.as_bytes())
            .expect("item should decrypt");
        let decoded: VaultPayload =
            serde_json::from_slice(&decrypted).expect("payload should deserialize");
        assert_eq!(decoded.password, "private-secret-9281");
        assert!(decrypt_vault_bytes(&key, &nonce, &ciphertext, b"different-record").is_err());
    }

    #[test]
    fn vault_uses_fresh_nonces_and_requires_unlock_for_key_access() {
        let session = VaultSession::default();
        assert!(session.with_key(|_| Ok(())).is_err());
        let key = derive_vault_key("correct horse battery staple", &[5u8; 16])
            .expect("key should derive");
        session.unlock(key).expect("session should unlock");
        assert!(session.with_key(|_| Ok(())).is_ok());
        session.lock().expect("session should lock");
        assert!(session.with_key(|_| Ok(())).is_err());

        let mut first = [0u8; 12];
        let mut second = [0u8; 12];
        OsRng.fill_bytes(&mut first);
        OsRng.fill_bytes(&mut second);
        assert_ne!(first, second);
    }

    #[test]
    fn vault_format_migration_records_kdf_parameters() {
        let connection = Connection::open_in_memory().expect("database should initialize");
        connection
            .execute_batch(
                "CREATE TABLE vault_config (
                    singleton INTEGER PRIMARY KEY,
                    salt BLOB NOT NULL,
                    memory_kib INTEGER NOT NULL,
                    iterations INTEGER NOT NULL,
                    parallelism INTEGER NOT NULL,
                    verifier_nonce BLOB NOT NULL,
                    verifier_cipher BLOB NOT NULL
                );
                CREATE TABLE vault_items (
                    id TEXT PRIMARY KEY NOT NULL,
                    nonce BLOB NOT NULL,
                    ciphertext BLOB NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                PRAGMA user_version = 4;",
            )
            .expect("version 4 vault schema should be created");

        migrate(&connection).expect("vault format should upgrade");

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("version should be queryable");
        let columns: i64 = connection
            .query_row(
                "SELECT count(*) FROM pragma_table_info('vault_config')
                 WHERE name IN ('kdf_format_version', 'kdf_algorithm', 'argon_version')",
                [],
                |row| row.get(0),
            )
            .expect("KDF columns should be queryable");
        assert_eq!(version, 5);
        assert_eq!(columns, 3);
    }

    #[test]
    fn upgrades_existing_v1_database_without_recreating_base_tables() {
        let database = AppDatabase::in_memory().expect("database should initialize");
        let connection = database
            .connection
            .lock()
            .expect("database should be available");
        connection
            .execute_batch(
                "DROP TABLE sprint_archives;
                 DROP INDEX unique_sprint_carryover;
                 DROP TABLE reminder_occurrences;
                 DROP TABLE vault_items;
                 DROP TABLE vault_config;
                 PRAGMA user_version = 1;",
            )
            .expect("version 1 schema should be restored");

        migrate(&connection).expect("database should upgrade");

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("version should be queryable");
        let table_exists: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'reminder_occurrences'",
                [],
                |row| row.get(0),
            )
            .expect("occurrence table should be queryable");
        assert_eq!(version, 5);
        assert_eq!(table_exists, 1);
    }

    #[test]
    fn due_reminder_delivery_is_persisted_and_not_duplicated() {
        let database = AppDatabase::in_memory().expect("database should initialize");
        let connection = database
            .connection
            .lock()
            .expect("database should be available");
        let due_at = (Utc::now() - Duration::minutes(1)).to_rfc3339();
        connection
            .execute(
                "INSERT INTO reminders
                 (id, title, trigger_at, timezone, enabled)
                 VALUES ('reminder-1', 'Take a break', ?1, 'UTC', 1)",
                params![due_at],
            )
            .expect("reminder should be inserted");
        connection
            .execute(
                "INSERT INTO reminder_occurrences (id, reminder_id, scheduled_at)
                 VALUES ('occurrence-1', 'reminder-1', ?1)",
                params![due_at],
            )
            .expect("occurrence should be inserted");
        drop(connection);

        let due = due_reminders(&database).expect("due reminders should load");
        assert_eq!(due.len(), 1);
        assert!(
            mark_reminder_delivered(&database, "occurrence-1").expect("delivery should persist")
        );
        assert!(
            !mark_reminder_delivered(&database, "occurrence-1").expect("repeat should be ignored")
        );
        assert!(
            due_reminders(&database)
                .expect("delivered reminder should not be due")
                .is_empty()
        );
    }

    #[test]
    fn archiving_keeps_a_stable_snapshot_for_history_search() {
        let database = AppDatabase::in_memory().expect("database should initialize");
        let mut connection = database
            .connection
            .lock()
            .expect("database should be available");
        connection
            .execute(
                "INSERT INTO sprints (id, name, goal, start_at, end_at, status, created_at)
                 VALUES ('sprint-archive', 'Release sprint', 'Ship the build', '2026-11-01', '2026-11-14', 'active', 'now')",
                [],
            )
            .expect("sprint should be inserted");
        connection
            .execute(
                "INSERT INTO work_items
                 (id, sprint_id, kind, title, status, priority, created_at, updated_at)
                 VALUES ('archive-item', 'sprint-archive', 'task', 'Original title', 'todo', 'normal', 'now', 'now')",
                [],
            )
            .expect("work item should be inserted");

        archive_sprint_record(&mut connection, "sprint-archive").expect("sprint should archive");
        connection
            .execute(
                "UPDATE work_items SET title = 'Changed after archive' WHERE id = 'archive-item'",
                [],
            )
            .expect("live item should be editable after archive");

        let snapshot: String = connection
            .query_row(
                "SELECT snapshot_json FROM sprint_archives WHERE sprint_id = 'sprint-archive'",
                [],
                |row| row.get(0),
            )
            .expect("snapshot should be queryable");
        let archived: ArchivedSprint =
            serde_json::from_str(&snapshot).expect("snapshot should deserialize");
        assert_eq!(archived.work_items[0].title, "Original title");
        assert_eq!(archived.sprint.status, "archived");
    }

    #[test]
    fn repeated_carryovers_are_blocked_by_the_database_constraint() {
        let database = AppDatabase::in_memory().expect("database should initialize");
        let connection = database
            .connection
            .lock()
            .expect("database should be available");
        connection
            .execute(
                "INSERT INTO sprints (id, name, goal, start_at, end_at, status, created_at)
                 VALUES ('active-sprint', 'Current', '', '2026-11-01', '2026-11-14', 'active', 'now')",
                [],
            )
            .expect("sprint should be inserted");
        connection
            .execute(
                "INSERT INTO sprints (id, name, goal, start_at, end_at, status, created_at)
                 VALUES ('old-sprint', 'Old', '', '2026-10-01', '2026-10-14', 'archived', 'now')",
                [],
            )
            .expect("archived sprint should be inserted");
        connection
            .execute(
                "INSERT INTO work_items
                 (id, sprint_id, kind, title, status, priority, created_at, updated_at)
                 VALUES ('source-item', 'old-sprint', 'task', 'Carry this', 'blocked', 'normal', 'now', 'now')",
                [],
            )
            .expect("source item should be inserted");
        connection
            .execute(
                "INSERT INTO work_items
                 (id, sprint_id, source_item_id, kind, title, status, priority, created_at, updated_at)
                 VALUES ('carried-item', 'active-sprint', 'source-item', 'task', 'Carry this', 'todo', 'normal', 'now', 'now')",
                [],
            )
            .expect("carried item should be inserted");

        let duplicate = connection.execute(
            "INSERT INTO work_items
             (id, sprint_id, source_item_id, kind, title, status, priority, created_at, updated_at)
             VALUES ('duplicate-item', 'active-sprint', 'source-item', 'task', 'Carry this', 'todo', 'normal', 'now', 'now')",
            [],
        );
        assert!(duplicate.is_err());
    }

    #[test]
    fn sqlite_backup_is_a_valid_snapshot() {
        let database = AppDatabase::in_memory().expect("database should initialize");
        let backup_path = std::env::temp_dir().join(format!(
            "devhud-test-backup-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        {
            let connection = database
                .connection
                .lock()
                .expect("database should be available");
            connection
                .execute(
                    "INSERT INTO sprints (id, name, goal, start_at, end_at, status, created_at)
                     VALUES ('backup-sprint', 'Backup test', '', '2026-11-01', '2026-11-14', 'active', 'now')",
                    [],
                )
                .expect("sprint should be inserted");
            create_database_backup(&connection, &backup_path)
                .expect("database backup should succeed");
        }

        let backup = Connection::open(&backup_path).expect("backup database should open");
        validate_backup_connection(&backup).expect("backup should pass integrity checks");
        assert_eq!(table_count(&backup, "sprints").unwrap(), 1);
        drop(backup);
        fs::remove_file(backup_path).expect("temporary backup should be removed");
    }

    #[test]
    fn restore_keeps_writes_on_the_original_database_path() {
        let current_path = std::env::temp_dir().join(format!(
            "devhud-test-restore-current-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let mut current = Connection::open(&current_path).expect("current database should open");
        migrate(&current).expect("current database should initialize");
        current
            .execute(
                "INSERT INTO sprints (id, name, goal, start_at, end_at, status, created_at)
                 VALUES ('old-sprint', 'Old data', '', '2026-11-01', '2026-11-14', 'active', 'now')",
                [],
            )
            .expect("old sprint should be inserted");

        let source = Connection::open_in_memory().expect("backup database should open");
        migrate(&source).expect("backup database should initialize");
        source
            .execute(
                "INSERT INTO sprints (id, name, goal, start_at, end_at, status, created_at)
                 VALUES ('restored-sprint', 'Restored data', '', '2026-12-01', '2026-12-14', 'active', 'now')",
                [],
            )
            .expect("restored sprint should be inserted");

        restore_database(&source, &mut current).expect("database should restore");
        drop(current);

        let reopened =
            Connection::open(&current_path).expect("original database path should reopen");
        let restored_count: i64 = reopened
            .query_row(
                "SELECT count(*) FROM sprints WHERE id = 'restored-sprint'",
                [],
                |row| row.get(0),
            )
            .expect("restored data should be queryable");
        let old_count: i64 = reopened
            .query_row(
                "SELECT count(*) FROM sprints WHERE id = 'old-sprint'",
                [],
                |row| row.get(0),
            )
            .expect("old data should be queryable");
        assert_eq!(restored_count, 1);
        assert_eq!(old_count, 0);

        reopened
            .execute(
                "INSERT INTO work_items
                 (id, sprint_id, kind, title, status, priority, created_at, updated_at)
                 VALUES ('after-restore', 'restored-sprint', 'task', 'After restore', 'todo', 'normal', 'now', 'now')",
                [],
            )
            .expect("writes after restore should succeed");
        drop(reopened);

        let verification =
            Connection::open(&current_path).expect("original database path should still open");
        let after_restore_count: i64 = verification
            .query_row(
                "SELECT count(*) FROM work_items WHERE id = 'after-restore'",
                [],
                |row| row.get(0),
            )
            .expect("post-restore write should persist");
        assert_eq!(after_restore_count, 1);
        drop(verification);
        drop(source);
        fs::remove_file(current_path).expect("temporary database should be removed");
    }
}
