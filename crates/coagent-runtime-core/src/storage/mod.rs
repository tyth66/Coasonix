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
pub struct RuntimeStepRecord {
    pub id: i64,
    pub task_id: String,
    pub request_id: Option<String>,
    pub operation: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEventRecord {
    pub id: i64,
    pub task_id: String,
    pub request_id: Option<String>,
    pub step_id: Option<i64>,
    pub task_sequence: i64,
    pub event_type: String,
    pub payload_json: String,
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
        let database_path = agent_dir.join("coagent.sqlite");

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

    pub fn schema_validation_expected_schemas(
        &self,
        task_id: &str,
        request_id: &str,
    ) -> Result<Vec<String>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT expected_schema FROM schema_validation_results
             WHERE task_id = ?1 AND request_id = ?2
             ORDER BY id",
        )?;
        let rows = statement.query_map(params![task_id, request_id], |row| row.get(0))?;
        rows.collect::<Result<Vec<String>, _>>()
            .map_err(StoreError::from)
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
            "INSERT INTO task_state (task_id, state, agent_calls)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(task_id) DO UPDATE SET
                state = excluded.state,
                agent_calls = excluded.agent_calls",
            params![
                state.task_id(),
                task_state_to_str(state.value()),
                state.agent_calls() as i64
            ],
        )?;
        Ok(())
    }

    pub fn load_task_state(&self, task_id: &str) -> Result<TaskState, StoreError> {
        let row = self
            .connection
            .query_row(
                "SELECT state, agent_calls FROM task_state WHERE task_id = ?1",
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

    /// List all non-terminal tasks with their current state and step summary.
    pub fn list_active_tasks(&self) -> Result<Vec<serde_json::Value>, StoreError> {
        let mut stmt = self.connection.prepare(
            "SELECT t.task_id, ts.state, ts.agent_calls,
                    (SELECT COUNT(*) FROM runtime_steps rs WHERE rs.task_id = t.task_id) as step_count,
                    (SELECT MAX(rs.created_at_ms) FROM runtime_steps rs WHERE rs.task_id = t.task_id) as last_step_ms
             FROM tasks t
             JOIN task_state ts ON t.task_id = ts.task_id
             WHERE ts.state NOT IN ('completed', 'failed', 'cancelled')
             ORDER BY last_step_ms DESC NULLS LAST
             LIMIT 50"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "task_id": row.get::<_, String>(0)?,
                "state": row.get::<_, String>(1)?,
                "agent_calls": row.get::<_, i64>(2)?,
                "step_count": row.get::<_, i64>(3)?,
                "last_step_ms": row.get::<_, Option<i64>>(4)?,
            }))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    /// Load a summary of a task: its state plus recent decisions and events.
    pub fn task_summary(&self, task_id: &str) -> Result<Option<serde_json::Value>, StoreError> {
        let state = match self.load_task_state(task_id) {
            Ok(s) => s,
            Err(StoreError::TaskStateNotFound(_)) => return Ok(None),
            Err(e) => return Err(e),
        };

        // Get recent runtime decisions
        let mut dec_stmt = self.connection.prepare(
            "SELECT decision, reasons_json, operation FROM runtime_decisions
             WHERE task_id = ?1 ORDER BY id DESC LIMIT 5"
        )?;
        let decisions: Vec<serde_json::Value> = dec_stmt
            .query_map(params![task_id], |row| {
                Ok(serde_json::json!({
                    "decision": row.get::<_, String>(0)?,
                    "reasons": row.get::<_, String>(1)?,
                    "operation": row.get::<_, String>(2)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(serde_json::json!({
            "task_id": task_id,
            "state": task_state_to_str(state.value()),
            "agent_calls": state.agent_calls(),
            "decisions": decisions,
        })))
    }

    /// Export the full history of a task: state, decisions, events, steps, attempts, and schema validations.
    pub fn export_task(&self, task_id: &str) -> Result<Option<serde_json::Value>, StoreError> {
        let state = match self.load_task_state(task_id) {
            Ok(s) => s,
            Err(StoreError::TaskStateNotFound(_)) => return Ok(None),
            Err(e) => return Err(e),
        };

        // Decisions
        let mut dec_stmt = self.connection.prepare(
            "SELECT decision, reasons_json, operation, request_id FROM runtime_decisions
             WHERE task_id = ?1 ORDER BY id"
        )?;
        let decisions: Vec<serde_json::Value> = dec_stmt
            .query_map(params![task_id], |row| {
                Ok(serde_json::json!({
                    "decision": row.get::<_, String>(0)?,
                    "reasons": row.get::<_, String>(1)?,
                    "operation": row.get::<_, String>(2)?,
                    "request_id": row.get::<_, Option<String>>(3)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Events
        let events = self.runtime_events(task_id)?;
        let events_json: Vec<serde_json::Value> = events.iter().map(|e| {
            serde_json::json!({
                "event_type": e.event_type,
                "task_sequence": e.task_sequence,
                "step_id": e.step_id,
                "payload_json": e.payload_json,
            })
        }).collect();

        // Steps
        let mut step_stmt = self.connection.prepare(
            "SELECT id, request_id, operation, state FROM runtime_steps
             WHERE task_id = ?1 ORDER BY id"
        )?;
        let steps: Vec<serde_json::Value> = step_stmt
            .query_map(params![task_id], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, i64>(0)?,
                    "request_id": row.get::<_, Option<String>>(1)?,
                    "operation": row.get::<_, String>(2)?,
                    "state": row.get::<_, String>(3)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Attempts
        let mut att_stmt = self.connection.prepare(
            "SELECT id, request_id, operation, backend_id, attempt_number, state, error_json
             FROM operation_attempts WHERE task_id = ?1 ORDER BY id"
        )?;
        let attempts: Vec<serde_json::Value> = att_stmt
            .query_map(params![task_id], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, i64>(0)?,
                    "request_id": row.get::<_, Option<String>>(1)?,
                    "operation": row.get::<_, String>(2)?,
                    "backend_id": row.get::<_, String>(3)?,
                    "attempt_number": row.get::<_, i64>(4)?,
                    "state": row.get::<_, String>(5)?,
                    "error_json": row.get::<_, Option<String>>(6)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(serde_json::json!({
            "schema_version": "coagent_export_v1",
            "task_id": task_id,
            "state": task_state_to_str(state.value()),
            "agent_calls": state.agent_calls(),
            "retry_count": state.retry_count(),
            "decisions": decisions,
            "steps": steps,
            "attempts": attempts,
            "events": events_json,
        })))
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

    pub fn start_runtime_step(
        &self,
        task_id: &str,
        request_id: Option<&str>,
        operation: &str,
    ) -> Result<RuntimeStepRecord, StoreError> {
        self.connection.execute(
            "INSERT INTO tasks (task_id) VALUES (?1)
             ON CONFLICT(task_id) DO NOTHING",
            params![task_id],
        )?;
        self.connection.execute(
            "INSERT INTO runtime_steps (task_id, request_id, operation, state)
             VALUES (?1, ?2, ?3, 'running')",
            params![task_id, request_id, operation],
        )?;
        let id = self.connection.last_insert_rowid();
        self.runtime_step(id)
    }

    pub fn finish_runtime_step(&self, step_id: i64, state: &str) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE runtime_steps SET state = ?1 WHERE id = ?2",
            params![state, step_id],
        )?;
        Ok(())
    }

    pub fn runtime_step(&self, step_id: i64) -> Result<RuntimeStepRecord, StoreError> {
        Ok(self.connection.query_row(
            "SELECT id, task_id, request_id, operation, state FROM runtime_steps WHERE id = ?1",
            params![step_id],
            runtime_step_from_row,
        )?)
    }

    pub fn runtime_step_for_request(
        &self,
        task_id: &str,
        request_id: Option<&str>,
        operation: &str,
    ) -> Result<Option<RuntimeStepRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT id, task_id, request_id, operation, state
                 FROM runtime_steps
                 WHERE task_id = ?1
                   AND ((request_id IS NULL AND ?2 IS NULL) OR request_id = ?2)
                   AND operation = ?3
                 ORDER BY id DESC
                 LIMIT 1",
                params![task_id, request_id, operation],
                runtime_step_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn write_runtime_event(
        &self,
        task_id: &str,
        request_id: Option<&str>,
        step_id: Option<i64>,
        event_type: &str,
        payload_json: &str,
    ) -> Result<RuntimeEventRecord, StoreError> {
        let next_sequence = self.next_runtime_event_sequence(task_id)?;
        self.connection.execute(
            "INSERT INTO runtime_events
             (task_id, request_id, step_id, task_sequence, event_type, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                task_id,
                request_id,
                step_id,
                next_sequence,
                event_type,
                payload_json
            ],
        )?;
        Ok(RuntimeEventRecord {
            id: self.connection.last_insert_rowid(),
            task_id: task_id.to_string(),
            request_id: request_id.map(str::to_string),
            step_id,
            task_sequence: next_sequence,
            event_type: event_type.to_string(),
            payload_json: payload_json.to_string(),
        })
    }

    pub fn runtime_events(&self, task_id: &str) -> Result<Vec<RuntimeEventRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, request_id, step_id, task_sequence, event_type, payload_json
             FROM runtime_events
             WHERE task_id = ?1
             ORDER BY task_sequence",
        )?;
        let rows = statement.query_map(params![task_id], runtime_event_from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Get the next attempt number for an operation (1-based, max+1).
    pub fn next_attempt_number(
        &self,
        task_id: &str,
        request_id: Option<&str>,
        operation: &str,
    ) -> Result<u32, StoreError> {
        let current: i64 = self.connection.query_row(
            "SELECT COALESCE(MAX(attempt_number), 0) FROM operation_attempts
             WHERE task_id = ?1
               AND ((request_id IS NULL AND ?2 IS NULL) OR request_id = ?2)
               AND operation = ?3",
            params![task_id, request_id, operation],
            |row| row.get(0),
        )?;
        Ok((current + 1) as u32)
    }

    /// Record a new operation attempt.
    pub fn record_attempt(
        &self,
        task_id: &str,
        request_id: Option<&str>,
        operation: &str,
        backend_id: &str,
        attempt_number: u32,
    ) -> Result<i64, StoreError> {
        self.connection.execute(
            "INSERT INTO tasks (task_id) VALUES (?1)
             ON CONFLICT(task_id) DO NOTHING",
            params![task_id],
        )?;
        self.connection.execute(
            "INSERT INTO operation_attempts
             (task_id, request_id, operation, backend_id, attempt_number, state, started_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6)",
            params![
                task_id,
                request_id,
                operation,
                backend_id,
                attempt_number,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64,
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Mark an attempt as succeeded.
    pub fn complete_attempt(&self, attempt_id: i64) -> Result<(), StoreError> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        self.connection.execute(
            "UPDATE operation_attempts SET state = 'succeeded', finished_at_ms = ?1 WHERE id = ?2",
            params![now_ms, attempt_id],
        )?;
        Ok(())
    }

    /// Mark an attempt as failed with an error.
    pub fn fail_attempt(&self, attempt_id: i64, error: &str) -> Result<(), StoreError> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        self.connection.execute(
            "UPDATE operation_attempts SET state = 'failed', error_json = ?1, finished_at_ms = ?2 WHERE id = ?3",
            params![error, now_ms, attempt_id],
        )?;
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

    fn next_runtime_event_sequence(&self, task_id: &str) -> Result<i64, StoreError> {
        let current: i64 = self.connection.query_row(
            "SELECT COALESCE(MAX(task_sequence), 0) FROM runtime_events WHERE task_id = ?1",
            params![task_id],
            |row| row.get(0),
        )?;
        Ok(current + 1)
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

fn runtime_step_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RuntimeStepRecord> {
    Ok(RuntimeStepRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        request_id: row.get(2)?,
        operation: row.get(3)?,
        state: row.get(4)?,
    })
}

fn runtime_event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RuntimeEventRecord> {
    Ok(RuntimeEventRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        request_id: row.get(2)?,
        step_id: row.get(3)?,
        task_sequence: row.get(4)?,
        event_type: row.get(5)?,
        payload_json: row.get(6)?,
    })
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
        TaskStateValue::Queued => "queued",
        TaskStateValue::Running => "running",
        TaskStateValue::Blocked => "blocked",
        TaskStateValue::WaitingApproval => "waiting_approval",
        TaskStateValue::Retrying => "retrying",
        TaskStateValue::PartiallyCompleted => "partially_completed",
        TaskStateValue::Completed => "completed",
        TaskStateValue::Failed => "failed",
        TaskStateValue::Cancelled => "cancelled",
    }
}

fn task_state_from_str(value: &str) -> Result<TaskStateValue, StoreError> {
    match value {
        "queued" => Ok(TaskStateValue::Queued),
        "running" => Ok(TaskStateValue::Running),
        "blocked" => Ok(TaskStateValue::Blocked),
        "waiting_approval" => Ok(TaskStateValue::WaitingApproval),
        "retrying" => Ok(TaskStateValue::Retrying),
        "partially_completed" => Ok(TaskStateValue::PartiallyCompleted),
        "completed" => Ok(TaskStateValue::Completed),
        "failed" => Ok(TaskStateValue::Failed),
        "cancelled" => Ok(TaskStateValue::Cancelled),
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
            agent_calls INTEGER NOT NULL DEFAULT 0
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
        "runtime_steps",
        "CREATE TABLE IF NOT EXISTS runtime_steps (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE RESTRICT,
            request_id TEXT,
            operation TEXT NOT NULL,
            state TEXT NOT NULL CHECK(length(state) > 0),
            created_at_ms INTEGER NOT NULL DEFAULT 0
        );",
    ),
    (
        "runtime_events",
        "CREATE TABLE IF NOT EXISTS runtime_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE RESTRICT,
            request_id TEXT,
            step_id INTEGER REFERENCES runtime_steps(id),
            task_sequence INTEGER NOT NULL,
            event_type TEXT NOT NULL CHECK(length(event_type) > 0),
            payload_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL DEFAULT 0,
            UNIQUE(task_id, task_sequence)
        );
        CREATE TRIGGER IF NOT EXISTS runtime_events_no_update
        BEFORE UPDATE ON runtime_events
        BEGIN
            SELECT RAISE(ABORT, 'runtime_events are append-only');
        END;
        CREATE TRIGGER IF NOT EXISTS runtime_events_no_delete
        BEFORE DELETE ON runtime_events
        BEGIN
            SELECT RAISE(ABORT, 'runtime_events are append-only');
        END;",
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
        "operation_attempts",
        "CREATE TABLE IF NOT EXISTS operation_attempts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE RESTRICT,
            request_id TEXT,
            operation TEXT NOT NULL,
            backend_id TEXT NOT NULL,
            attempt_number INTEGER NOT NULL DEFAULT 1,
            state TEXT NOT NULL CHECK(length(state) > 0),
            error_json TEXT,
            started_at_ms INTEGER NOT NULL DEFAULT 0,
            finished_at_ms INTEGER
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
