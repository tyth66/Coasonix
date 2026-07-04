use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use thiserror::Error;

use crate::{
    policy::RuntimeDecisionValue,
    state::{TaskState, TaskStateValue},
};

#[derive(Debug)]
pub struct RuntimeStore {
    database_path: PathBuf,
    connection: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEventInput {
    pub task_id: String,
    pub event_type: String,
    pub summary: String,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEventRecord {
    pub id: i64,
    pub task_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDecisionRecord {
    pub task_id: String,
    pub request_id: Option<String>,
    pub operation: String,
    pub decision: RuntimeDecisionValue,
    pub reasons: Vec<String>,
    pub command_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaValidationRecord {
    pub task_id: String,
    pub request_id: Option<String>,
    pub expected_schema: String,
    pub valid: bool,
    pub errors_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheMetadata {
    pub cache_key: String,
    pub payload_hash: String,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration failed: {0}")]
    MigrationFailed(String),
    #[error("audit events are append-only")]
    AppendOnlyViolation,
    #[error("task state not found: {0}")]
    TaskStateNotFound(String),
    #[error("invalid task state value: {0}")]
    InvalidTaskState(String),
}

impl RuntimeStore {
    pub fn initialize(repo_root: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::initialize_inner(repo_root.as_ref(), None)
    }

    pub fn initialize_with_extra_migration(
        repo_root: impl AsRef<Path>,
        extra_migration: &str,
    ) -> Result<Self, StoreError> {
        Self::initialize_inner(repo_root.as_ref(), Some(extra_migration))
    }

    fn initialize_inner(
        repo_root: &Path,
        extra_migration: Option<&str>,
    ) -> Result<Self, StoreError> {
        let agent_dir = repo_root.join(".agent");
        fs::create_dir_all(&agent_dir)?;
        let database_path = agent_dir.join("coasonix.sqlite");

        match Self::open_and_migrate(&database_path, extra_migration) {
            Ok(connection) => Ok(Self {
                database_path,
                connection,
            }),
            Err(error) => {
                let _ = fs::remove_file(&database_path);
                Err(error)
            }
        }
    }

    fn open_and_migrate(
        database_path: &Path,
        extra_migration: Option<&str>,
    ) -> Result<Connection, StoreError> {
        let mut connection = Connection::open(database_path)?;
        connection.set_transaction_behavior(TransactionBehavior::Immediate);
        apply_pragmas(&connection)?;
        run_migrations(&connection, extra_migration)?;
        Ok(connection)
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn foreign_keys_enabled(&self) -> Result<bool, StoreError> {
        Ok(self
            .connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))?
            == 1)
    }

    pub fn journal_mode(&self) -> Result<String, StoreError> {
        Ok(self
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))?
            .to_lowercase())
    }

    pub fn synchronous_level(&self) -> Result<i64, StoreError> {
        Ok(self
            .connection
            .query_row("PRAGMA synchronous", [], |row| row.get::<_, i64>(0))?)
    }

    pub fn busy_timeout_ms(&self) -> Result<i64, StoreError> {
        Ok(self
            .connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get::<_, i64>(0))?)
    }

    pub fn migration_tables(&self) -> Result<Vec<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT name FROM runtime_metadata WHERE key LIKE 'migration.%' ORDER BY value",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn write_audit_event(
        &self,
        event: &AuditEventInput,
    ) -> Result<AuditEventRecord, StoreError> {
        let next_sequence = self.next_task_sequence(&event.task_id)?;
        self.connection.execute(
            "INSERT INTO audit_events (task_id, task_sequence, event_type, summary, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                event.task_id,
                next_sequence,
                event.event_type,
                event.summary,
                event.payload_json
            ],
        )?;
        Ok(AuditEventRecord {
            id: self.connection.last_insert_rowid(),
            task_sequence: next_sequence,
        })
    }

    pub fn try_update_audit_summary(&self, id: i64, summary: &str) -> Result<(), StoreError> {
        match self.connection.execute(
            "UPDATE audit_events SET summary = ?1 WHERE id = ?2",
            params![summary, id],
        ) {
            Ok(_) => Ok(()),
            Err(error) if is_append_only_violation(&error) => Err(StoreError::AppendOnlyViolation),
            Err(error) => Err(StoreError::Sqlite(error)),
        }
    }

    pub fn try_delete_audit_event(&self, id: i64) -> Result<(), StoreError> {
        match self
            .connection
            .execute("DELETE FROM audit_events WHERE id = ?1", params![id])
        {
            Ok(_) => Ok(()),
            Err(error) if is_append_only_violation(&error) => Err(StoreError::AppendOnlyViolation),
            Err(error) => Err(StoreError::Sqlite(error)),
        }
    }

    pub fn commit_runtime_decision_with_audit(
        &self,
        decision: &RuntimeDecisionRecord,
        audit: &AuditEventInput,
    ) -> Result<AuditEventRecord, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let next_sequence = next_task_sequence(&transaction, &audit.task_id)?;
        transaction.execute(
            "INSERT INTO audit_events (task_id, task_sequence, event_type, summary, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                audit.task_id,
                next_sequence,
                audit.event_type,
                audit.summary,
                audit.payload_json
            ],
        )?;
        let audit_id = transaction.last_insert_rowid();
        insert_runtime_decision(&transaction, decision, Some(audit_id))?;
        transaction.commit()?;
        Ok(AuditEventRecord {
            id: audit_id,
            task_sequence: next_sequence,
        })
    }

    pub fn commit_schema_validation_with_audit(
        &self,
        validation: &SchemaValidationRecord,
        audit: &AuditEventInput,
    ) -> Result<AuditEventRecord, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let next_sequence = next_task_sequence(&transaction, &audit.task_id)?;
        transaction.execute(
            "INSERT INTO audit_events (task_id, task_sequence, event_type, summary, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                audit.task_id,
                next_sequence,
                audit.event_type,
                audit.summary,
                audit.payload_json
            ],
        )?;
        let audit_id = transaction.last_insert_rowid();
        transaction.execute(
            "INSERT INTO schema_validation_results
             (task_id, request_id, expected_schema, valid, errors_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                validation.task_id,
                validation.request_id,
                validation.expected_schema,
                i64::from(validation.valid),
                validation.errors_json
            ],
        )?;
        transaction.commit()?;
        Ok(AuditEventRecord {
            id: audit_id,
            task_sequence: next_sequence,
        })
    }

    pub fn schema_validation_count(
        &self,
        task_id: &str,
        request_id: &str,
    ) -> Result<i64, StoreError> {
        Ok(self.connection.query_row(
            "SELECT COUNT(*) FROM schema_validation_results
             WHERE task_id = ?1 AND request_id = ?2",
            params![task_id, request_id],
            |row| row.get(0),
        )?)
    }

    pub fn runtime_decision_audit_event_id(
        &self,
        task_id: &str,
        request_id: &str,
    ) -> Result<Option<i64>, StoreError> {
        self.connection
            .query_row(
                "SELECT audit_event_id FROM runtime_decisions
                 WHERE task_id = ?1 AND request_id = ?2
                 ORDER BY id DESC LIMIT 1",
                params![task_id, request_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()
            .map(|value| value.flatten())
            .map_err(StoreError::from)
    }

    pub fn runtime_decision_count(
        &self,
        task_id: &str,
        decision: RuntimeDecisionValue,
    ) -> Result<i64, StoreError> {
        Ok(self.connection.query_row(
            "SELECT COUNT(*) FROM runtime_decisions WHERE task_id = ?1 AND decision = ?2",
            params![task_id, runtime_decision_to_str(decision)],
            |row| row.get(0),
        )?)
    }

    pub fn upsert_task_state(&self, state: &TaskState) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO tasks (task_id) VALUES (?1)
             ON CONFLICT(task_id) DO NOTHING",
            params![state.task_id()],
        )?;
        self.connection.execute(
            "INSERT INTO task_state (task_id, state, reasonix_calls)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(task_id) DO UPDATE SET
                state = excluded.state,
                reasonix_calls = excluded.reasonix_calls",
            params![
                state.task_id(),
                task_state_to_str(state.value()),
                state.reasonix_calls() as i64
            ],
        )?;
        Ok(())
    }

    pub fn load_task_state(&self, task_id: &str) -> Result<TaskState, StoreError> {
        let row = self
            .connection
            .query_row(
                "SELECT state, reasonix_calls FROM task_state WHERE task_id = ?1",
                params![task_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
            .ok_or_else(|| StoreError::TaskStateNotFound(task_id.to_string()))?;

        Ok(TaskState::restore(
            task_id,
            task_state_from_str(&row.0)?,
            row.1 as u32,
        ))
    }

    pub fn transition_state_with_audit(
        &self,
        task_id: &str,
        next: TaskStateValue,
        audit: &AuditEventInput,
    ) -> Result<(), StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "UPDATE task_state SET state = ?1 WHERE task_id = ?2",
            params![task_state_to_str(next), task_id],
        )?;
        let next_sequence = next_task_sequence(&transaction, task_id)?;
        transaction.execute(
            "INSERT INTO audit_events (task_id, task_sequence, event_type, summary, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                audit.task_id,
                next_sequence,
                audit.event_type,
                audit.summary,
                audit.payload_json
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn insert_lock(
        &self,
        lock_id: &str,
        task_id: &str,
        acquired_at_ms: i64,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO locks (lock_id, task_id, acquired_at_ms) VALUES (?1, ?2, ?3)",
            params![lock_id, task_id, acquired_at_ms],
        )?;
        Ok(())
    }

    pub fn stale_locks(&self, now_ms: i64, stale_after_ms: i64) -> Result<Vec<String>, StoreError> {
        let cutoff = now_ms - stale_after_ms;
        let mut statement = self
            .connection
            .prepare("SELECT lock_id FROM locks WHERE acquired_at_ms <= ?1 ORDER BY lock_id")?;
        let rows = statement.query_map(params![cutoff], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn record_cache_metadata(&self, metadata: &CacheMetadata) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO cache_entries (cache_key, payload_hash, reuse_enabled, corrupted)
             VALUES (?1, ?2, 0, 0)
             ON CONFLICT(cache_key) DO UPDATE SET
                payload_hash = excluded.payload_hash,
                reuse_enabled = 0,
                corrupted = 0",
            params![metadata.cache_key, metadata.payload_hash],
        )?;
        Ok(())
    }

    pub fn cache_entry_count(&self) -> Result<i64, StoreError> {
        Ok(self
            .connection
            .query_row("SELECT COUNT(*) FROM cache_entries", [], |row| row.get(0))?)
    }

    pub fn cache_reuse_allowed(
        &self,
        cache_key: &str,
        actual_payload_hash: &str,
    ) -> Result<bool, StoreError> {
        let row = self
            .connection
            .query_row(
                "SELECT payload_hash, reuse_enabled FROM cache_entries WHERE cache_key = ?1",
                params![cache_key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;

        let Some((expected_hash, reuse_enabled)) = row else {
            return Ok(false);
        };

        if expected_hash != actual_payload_hash {
            self.connection.execute(
                "UPDATE cache_entries SET corrupted = 1 WHERE cache_key = ?1",
                params![cache_key],
            )?;
            return Ok(false);
        }

        Ok(reuse_enabled == 1)
    }

    fn next_task_sequence(&self, task_id: &str) -> Result<i64, StoreError> {
        next_task_sequence(&self.connection, task_id)
    }
}

fn apply_pragmas(connection: &Connection) -> Result<(), StoreError> {
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    connection.busy_timeout(std::time::Duration::from_millis(5000))?;
    Ok(())
}

fn run_migrations(
    connection: &Connection,
    extra_migration: Option<&str>,
) -> Result<(), StoreError> {
    let result = (|| -> Result<(), rusqlite::Error> {
        for (index, (name, sql)) in MIGRATIONS.iter().enumerate() {
            connection.execute_batch(sql)?;
            connection.execute(
                "INSERT OR REPLACE INTO runtime_metadata (key, name, value)
                 VALUES (?1, ?2, ?3)",
                params![
                    format!("migration.{:02}", index + 1),
                    *name,
                    (index + 1) as i64
                ],
            )?;
        }
        if let Some(extra) = extra_migration {
            connection.execute_batch(extra)?;
        }
        Ok(())
    })();

    result.map_err(|error| StoreError::MigrationFailed(error.to_string()))
}

fn next_task_sequence(connection: &Connection, task_id: &str) -> Result<i64, StoreError> {
    let current: i64 = connection.query_row(
        "SELECT COALESCE(MAX(task_sequence), 0) FROM audit_events WHERE task_id = ?1",
        params![task_id],
        |row| row.get(0),
    )?;
    Ok(current + 1)
}

fn insert_runtime_decision(
    connection: &Connection,
    decision: &RuntimeDecisionRecord,
    audit_event_id: Option<i64>,
) -> Result<(), StoreError> {
    connection.execute(
        "INSERT INTO runtime_decisions
         (task_id, request_id, operation, decision, reasons_json, command_hash, audit_event_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            decision.task_id,
            decision.request_id,
            decision.operation,
            runtime_decision_to_str(decision.decision),
            serde_json::to_string(&decision.reasons)
                .map_err(|error| StoreError::MigrationFailed(error.to_string()))?,
            decision.command_hash,
            audit_event_id
        ],
    )?;
    Ok(())
}

fn is_append_only_violation(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(_, Some(message))
            if message.contains("audit_events are append-only")
    )
}

fn task_state_to_str(value: TaskStateValue) -> &'static str {
    match value {
        TaskStateValue::Created => "created",
        TaskStateValue::Running => "running",
        TaskStateValue::Completed => "completed",
        TaskStateValue::Failed => "failed",
    }
}

fn task_state_from_str(value: &str) -> Result<TaskStateValue, StoreError> {
    match value {
        "created" => Ok(TaskStateValue::Created),
        "running" => Ok(TaskStateValue::Running),
        "completed" => Ok(TaskStateValue::Completed),
        "failed" => Ok(TaskStateValue::Failed),
        _ => Err(StoreError::InvalidTaskState(value.to_string())),
    }
}

fn runtime_decision_to_str(value: RuntimeDecisionValue) -> &'static str {
    match value {
        RuntimeDecisionValue::Allow => "allow",
        RuntimeDecisionValue::Deny => "deny",
        RuntimeDecisionValue::RequireApproval => "require_approval",
        RuntimeDecisionValue::RetryableError => "retryable_error",
        RuntimeDecisionValue::FatalError => "fatal_error",
    }
}

const MIGRATIONS: &[(&str, &str)] = &[
    (
        "runtime_metadata",
        "CREATE TABLE IF NOT EXISTS runtime_metadata (
            key TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            value INTEGER NOT NULL
        );",
    ),
    (
        "tasks",
        "CREATE TABLE IF NOT EXISTS tasks (
            task_id TEXT PRIMARY KEY,
            created_at_ms INTEGER NOT NULL DEFAULT 0
        );",
    ),
    (
        "audit_events",
        "CREATE TABLE IF NOT EXISTS audit_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            task_sequence INTEGER NOT NULL,
            event_type TEXT NOT NULL CHECK(length(event_type) > 0),
            summary TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL DEFAULT 0,
            UNIQUE(task_id, task_sequence)
        );
        CREATE TRIGGER IF NOT EXISTS audit_events_no_update
        BEFORE UPDATE ON audit_events
        BEGIN
            SELECT RAISE(ABORT, 'audit_events are append-only');
        END;
        CREATE TRIGGER IF NOT EXISTS audit_events_no_delete
        BEFORE DELETE ON audit_events
        BEGIN
            SELECT RAISE(ABORT, 'audit_events are append-only');
        END;",
    ),
    (
        "task_state",
        "CREATE TABLE IF NOT EXISTS task_state (
            task_id TEXT PRIMARY KEY REFERENCES tasks(task_id) ON DELETE RESTRICT,
            state TEXT NOT NULL,
            reasonix_calls INTEGER NOT NULL DEFAULT 0
        );",
    ),
    (
        "runtime_decisions",
        "CREATE TABLE IF NOT EXISTS runtime_decisions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            request_id TEXT,
            operation TEXT NOT NULL,
            decision TEXT NOT NULL,
            reasons_json TEXT NOT NULL,
            command_hash TEXT,
            audit_event_id INTEGER REFERENCES audit_events(id)
        );",
    ),
    (
        "schema_validation_results",
        "CREATE TABLE IF NOT EXISTS schema_validation_results (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            request_id TEXT,
            expected_schema TEXT NOT NULL,
            valid INTEGER NOT NULL,
            errors_json TEXT NOT NULL
        );",
    ),
    (
        "policy_evaluation_results",
        "CREATE TABLE IF NOT EXISTS policy_evaluation_results (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            request_id TEXT,
            decision TEXT NOT NULL,
            reasons_json TEXT NOT NULL
        );",
    ),
    (
        "locks",
        "CREATE TABLE IF NOT EXISTS locks (
            lock_id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            acquired_at_ms INTEGER NOT NULL
        );",
    ),
    (
        "artifacts",
        "CREATE TABLE IF NOT EXISTS artifacts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            path TEXT NOT NULL,
            hash TEXT
        );",
    ),
    (
        "cache_entries",
        "CREATE TABLE IF NOT EXISTS cache_entries (
            cache_key TEXT PRIMARY KEY,
            payload_hash TEXT NOT NULL,
            reuse_enabled INTEGER NOT NULL DEFAULT 0,
            corrupted INTEGER NOT NULL DEFAULT 0
        );",
    ),
];
