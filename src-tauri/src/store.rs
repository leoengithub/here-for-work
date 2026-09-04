use std::collections::HashSet;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::ptr;

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Europe::Madrid;
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    ActivityEntry, AdapterEffectContext, AdapterRoleContext, BrowserAnswerCommitWork,
    BrowserAnswerWork, BrowserCommand, BrowserInspection, BrowserSessionSummary, CvFallbackSetting,
    DashboardState, DiscoveryCursor, DiscoveryDataset, DiscoveryFinding, DiscoveryRun,
    DiscoveryRunDiagnostic, DiscoveryRunEvidenceDiagnostic, DiscoveryRunFinding,
    DiscoveryRunFindingDiagnostic, DiscoveryRunImportResult, DiscoveryRunMatchScore,
    EvaluationResultRead, EvaluationSyncRole, HistoryRecord, ImportResult, LeasedRun,
    OutcomeNotification, PreQueueRecovery, PreQueueRoleSummary, PreparationCleanupWork,
    PreparationEvaluationIdentity, PreparationSummary, PreparationWork, QueueEvaluationSummary,
    QueueFilters, QueueGroup, ReconcileResult, RestorePreflight, RoleSummary, RunSummary,
    ScheduledRun, SourceScheduleSummary, StuckPreparationCandidate, StuckPreparationReset,
};

const SCHEMA_VERSION: i64 = 25;
const MAX_RUN_ATTEMPTS: i64 = 3;
const MAX_BROWSER_COMMAND_ATTEMPTS: i64 = 3;
const MAX_EVALUATION_SYNC_ATTEMPTS: i64 = 3;
const MAX_ACTIVE_PREPARATIONS: i64 = 2;
const EVALUATION_LEASE_SECONDS: i64 = 120;
const EVALUATION_EXECUTOR_LEASE_SECONDS: i64 = 3_600;
const RUN_STEPS: [&str; 3] = ["discover", "reconcile", "notify"];
const MAX_DISCOVERY_RUN_BYTES: usize = 2_000_000;
const MAX_DISCOVERY_FINDINGS: usize = 1_000;
const MAX_DISCOVERY_EVIDENCE_PER_FINDING: usize = 32;
const MAX_DISCOVERY_ISSUES: usize = 100;
const MAX_DISCOVERY_DIAGNOSTICS_BYTES: usize = 500_000;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("dataset is not valid JSON: {0}")]
    InvalidDataset(#[from] serde_json::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported dataset schema version {0}")]
    UnsupportedDataset(u32),
    #[error("invalid discovery run: {0}")]
    InvalidDiscoveryRun(String),
    #[error("invalid queue group in operational store: {0}")]
    InvalidQueueGroup(String),
    #[error("unknown discovery source: {0}")]
    UnknownSource(String),
    #[error("discovery source {0} is staged; the existing external workflow still owns execution")]
    SourceNotReady(String),
    #[error("invalid run transition: {0}")]
    #[cfg_attr(not(test), allow(dead_code))]
    InvalidRunTransition(String),
    #[error("invalid browser transition: {0}")]
    InvalidBrowserTransition(String),
    #[error("invalid adapter effect: {0}")]
    InvalidAdapterEffect(String),
    #[error("invalid preparation: {0}")]
    InvalidPreparation(String),
}

pub struct Store {
    connection: Connection,
}

pub struct PreparationCompletion<'a> {
    pub tracker_id: i64,
    pub report_path: &'a str,
    pub report_hash: &'a str,
    pub cv_pdf_path: &'a str,
    pub cv_pdf_hash: &'a str,
    pub cv_source: &'a str,
}

impl Store {
    fn table_exists(&self, name: &str) -> Result<bool, StoreError> {
        let count = self.connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count == 1)
    }

    fn column_exists(&self, table: &str, column: &str) -> Result<bool, StoreError> {
        let pragma = format!("PRAGMA table_info({table})");
        let mut statement = self.connection.prepare(&pragma)?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            if row.get::<_, String>(1)? == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn current_preparation_evaluation(
        &self,
        role_id: &str,
    ) -> Result<PreparationEvaluationIdentity, StoreError> {
        self.connection
            .query_row(
                "SELECT receipt.tracker_id, receipt.report_path, receipt.report_hash,
                        receipt.upstream_revision, receipt.compatibility_fingerprint
                   FROM evaluation_sync evaluation
                   JOIN evaluation_receipts receipt
                     ON receipt.receipt_key = evaluation.current_receipt_key
                  WHERE evaluation.role_id = ?1
                    AND evaluation.state IN ('ready', 'needs_decision')",
                [role_id],
                |row| {
                    Ok(PreparationEvaluationIdentity {
                        tracker_id: row.get(0)?,
                        report_path: row.get(1)?,
                        report_sha256: row.get(2)?,
                        upstream_revision: row.get(3)?,
                        evaluation_compatibility_fingerprint: row.get(4)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::InvalidPreparation(
                    "the role is awaiting a current canonical career-ops evaluation".to_string(),
                )
            })
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        let should_consider_backup = path
            .metadata()
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false);
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        if should_consider_backup {
            backup_before_migration(&connection, path)?;
        }
        let mut store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<(), StoreError> {
        self.connection.execute_batch(
            "BEGIN IMMEDIATE;
             CREATE TABLE IF NOT EXISTS schema_meta (
               version INTEGER NOT NULL
             );
             INSERT INTO schema_meta(version)
               SELECT 0 WHERE NOT EXISTS (SELECT 1 FROM schema_meta);
             CREATE TABLE IF NOT EXISTS settings (
               key TEXT PRIMARY KEY,
               value TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS roles (
               id TEXT PRIMARY KEY,
               normalized_key TEXT NOT NULL UNIQUE,
               company TEXT NOT NULL,
               title TEXT NOT NULL,
               location TEXT NOT NULL,
               queue_group TEXT NOT NULL,
               eligibility_summary TEXT NOT NULL,
               uncertainty TEXT,
               application_url TEXT,
               preparation_state TEXT NOT NULL DEFAULT 'not_started',
               discovered_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS source_occurrences (
               id TEXT PRIMARY KEY,
               role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
               source_id TEXT NOT NULL,
               source TEXT NOT NULL,
               source_role_id TEXT NOT NULL,
               payload_hash TEXT NOT NULL,
               discovered_at TEXT NOT NULL,
               UNIQUE(source_id, source_role_id)
             );
             CREATE TABLE IF NOT EXISTS activity (
               id TEXT PRIMARY KEY,
               kind TEXT NOT NULL,
               message TEXT NOT NULL,
               occurred_at TEXT NOT NULL
             );
             COMMIT;",
        )?;

        let mut version: i64 =
            self.connection
                .query_row("SELECT version FROM schema_meta LIMIT 1", [], |row| {
                    row.get(0)
                })?;
        if version < 1 {
            self.connection
                .execute("UPDATE schema_meta SET version = 1", [])?;
            version = 1;
        }
        if version < 2 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE roles ADD COLUMN canonical_tracker_id INTEGER;
                 ALTER TABLE roles ADD COLUMN canonical_status TEXT;
                 ALTER TABLE roles ADD COLUMN canonical_date TEXT;
                 UPDATE schema_meta SET version = 2;
                 COMMIT;",
            )?;
            version = 2;
        }
        if version < 3 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE IF NOT EXISTS source_schedules (
                   source_id TEXT PRIMARY KEY,
                   display_name TEXT NOT NULL,
                   timezone TEXT NOT NULL,
                   schedule_hours TEXT NOT NULL,
                   last_successful_at TEXT,
                   enabled INTEGER NOT NULL DEFAULT 1
                 );
                 CREATE TABLE IF NOT EXISTS runs (
                   id TEXT PRIMARY KEY,
                   source_id TEXT NOT NULL REFERENCES source_schedules(source_id),
                   kind TEXT NOT NULL,
                   coverage_start TEXT NOT NULL,
                   coverage_end TEXT NOT NULL,
                   status TEXT NOT NULL,
                   attempt INTEGER NOT NULL DEFAULT 0,
                   lease_expires_at TEXT,
                   error_class TEXT,
                   created_at TEXT NOT NULL,
                   updated_at TEXT NOT NULL,
                   dedupe_key TEXT NOT NULL UNIQUE
                 );
                 CREATE TABLE IF NOT EXISTS run_steps (
                   id TEXT PRIMARY KEY,
                   run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
                   name TEXT NOT NULL,
                   status TEXT NOT NULL,
                   attempt INTEGER NOT NULL DEFAULT 0,
                   input_hash TEXT,
                   output_hash TEXT,
                   error_class TEXT,
                   updated_at TEXT NOT NULL,
                   UNIQUE(run_id, name)
                 );
                 CREATE TABLE IF NOT EXISTS notification_outbox (
                   id TEXT PRIMARY KEY,
                   dedupe_key TEXT NOT NULL UNIQUE,
                   title TEXT NOT NULL,
                   body TEXT NOT NULL,
                   status TEXT NOT NULL DEFAULT 'pending',
                   attempts INTEGER NOT NULL DEFAULT 0,
                   next_attempt_at TEXT NOT NULL,
                   created_at TEXT NOT NULL
                 );
                 INSERT OR IGNORE INTO source_schedules(source_id, display_name, timezone, schedule_hours, last_successful_at)
                   VALUES ('frontend-role-scan', 'Frontend Role Scan', 'Europe/Madrid', '8', '2026-08-29T06:00:00Z');
                 INSERT OR IGNORE INTO source_schedules(source_id, display_name, timezone, schedule_hours, last_successful_at)
                   VALUES ('eu-job-radar', 'EU Job Radar', 'Europe/Madrid', '9,13,18', '2026-08-29T06:00:00Z');
                 UPDATE schema_meta SET version = 3;
                 COMMIT;",
            )?;
            version = 3;
        }
        if version < 4 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE source_schedules ADD COLUMN execution_mode TEXT NOT NULL DEFAULT 'staged';
                 UPDATE runs
                    SET status = 'action_required',
                        error_class = 'source_adapter_not_configured',
                        lease_expires_at = NULL,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                  WHERE status IN ('queued', 'retryable', 'running');
                 INSERT OR IGNORE INTO run_steps(id, run_id, name, status, attempt, error_class, updated_at)
                   SELECT lower(hex(randomblob(16))), id, 'discover', 'action_required', 0,
                          'source_adapter_not_configured', updated_at FROM runs;
                 INSERT OR IGNORE INTO run_steps(id, run_id, name, status, attempt, error_class, updated_at)
                   SELECT lower(hex(randomblob(16))), id, 'reconcile', 'blocked', 0,
                          'source_adapter_not_configured', updated_at FROM runs;
                 INSERT OR IGNORE INTO run_steps(id, run_id, name, status, attempt, error_class, updated_at)
                   SELECT lower(hex(randomblob(16))), id, 'notify', 'blocked', 0,
                          'source_adapter_not_configured', updated_at FROM runs;
                 UPDATE schema_meta SET version = 4;
                 COMMIT;",
            )?;
            version = 4;
        }
        if version < 5 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE IF NOT EXISTS browser_sessions (
                   id TEXT PRIMARY KEY,
                   purpose TEXT NOT NULL,
                   role_id TEXT REFERENCES roles(id),
                   status TEXT NOT NULL,
                   ats TEXT,
                   page_title TEXT,
                   page_url TEXT,
                   snapshot_fingerprint TEXT,
                   fields_json TEXT,
                   field_count INTEGER NOT NULL DEFAULT 0,
                   safe_field_count INTEGER NOT NULL DEFAULT 0,
                   needs_user_count INTEGER NOT NULL DEFAULT 0,
                   error_code TEXT,
                   created_at TEXT NOT NULL,
                   updated_at TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS browser_commands (
                   id TEXT PRIMARY KEY,
                   session_id TEXT NOT NULL REFERENCES browser_sessions(id) ON DELETE CASCADE,
                   command_type TEXT NOT NULL,
                   payload_json TEXT NOT NULL,
                   status TEXT NOT NULL,
                   attempt INTEGER NOT NULL DEFAULT 0,
                   lease_expires_at TEXT,
                   error_code TEXT,
                   created_at TEXT NOT NULL,
                   updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS browser_commands_claimable
                   ON browser_commands(status, created_at);
                 INSERT INTO activity(id, kind, message, occurred_at)
                   SELECT lower(hex(randomblob(16))), 'schedule',
                          'Earlier queued catch-up entries were reclassified as preserved windows; the existing external workflows still own execution.',
                          strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    WHERE EXISTS (
                      SELECT 1 FROM runs
                       WHERE status = 'action_required'
                         AND error_class = 'source_adapter_not_configured'
                    )
                      AND NOT EXISTS (
                        SELECT 1 FROM activity
                         WHERE message = 'Earlier queued catch-up entries were reclassified as preserved windows; the existing external workflows still own execution.'
                      );
                 UPDATE schema_meta SET version = 5;
                 COMMIT;",
            )?;
            version = 5;
        }
        if version < 6 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE roles ADD COLUMN review_state TEXT NOT NULL DEFAULT 'unviewed';
                 ALTER TABLE roles ADD COLUMN canonical_visibility_override INTEGER NOT NULL DEFAULT 0;
                 CREATE TABLE IF NOT EXISTS adapter_effects (
                   idempotency_key TEXT PRIMARY KEY,
                   role_id TEXT NOT NULL REFERENCES roles(id),
                   operation TEXT NOT NULL,
                   status TEXT NOT NULL,
                   parent_effect_key TEXT,
                   tracker_id INTEGER,
                   error_class TEXT,
                   created_at TEXT NOT NULL,
                   updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS adapter_effects_role
                   ON adapter_effects(role_id, operation, updated_at);
                 UPDATE schema_meta SET version = 6;
                 COMMIT;",
            )?;
            version = 6;
        }
        if version < 7 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE IF NOT EXISTS preparation_jobs (
                   id TEXT PRIMARY KEY,
                   role_id TEXT NOT NULL REFERENCES roles(id),
                   provider TEXT NOT NULL,
                   status TEXT NOT NULL,
                   step TEXT NOT NULL,
                   attempt INTEGER NOT NULL DEFAULT 0,
                   context_hash TEXT,
                   tracker_id INTEGER,
                   report_path TEXT,
                   report_hash TEXT,
                   cv_pdf_path TEXT,
                   cv_pdf_hash TEXT,
                   error_class TEXT,
                   created_at TEXT NOT NULL,
                   updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS preparation_jobs_role
                   ON preparation_jobs(role_id, updated_at);
                 UPDATE preparation_jobs
                    SET status = 'action_required', error_class = 'app_interrupted',
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                  WHERE status IN ('queued', 'preparing');
                 UPDATE roles SET preparation_state = 'failed'
                  WHERE id IN (
                    SELECT role_id FROM preparation_jobs WHERE status = 'action_required'
                  );
                 UPDATE schema_meta SET version = 7;
                 COMMIT;",
            )?;
            version = 7;
        }
        if version < 8 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE browser_sessions ADD COLUMN preparation_id TEXT REFERENCES preparation_jobs(id);
                 ALTER TABLE browser_sessions ADD COLUMN provider TEXT;
                 ALTER TABLE browser_sessions ADD COLUMN answers_context_hash TEXT;
                 ALTER TABLE browser_sessions ADD COLUMN review_items_json TEXT;
                 ALTER TABLE browser_sessions ADD COLUMN fill_results_json TEXT;
                 UPDATE browser_sessions
                    SET status = 'action_required', error_code = 'app_interrupted',
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                  WHERE status IN ('drafting_answers', 'answering', 'filling');
                 CREATE INDEX IF NOT EXISTS browser_sessions_preparation
                   ON browser_sessions(preparation_id, updated_at);
                 UPDATE schema_meta SET version = 8;
                 COMMIT;",
            )?;
            version = 8;
        }
        if version < 9 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE browser_sessions ADD COLUMN answers_report_hash TEXT;
                 UPDATE browser_sessions
                    SET status = 'action_required', error_code = 'answer_persistence_interrupted',
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                  WHERE status IN ('persisting_answers', 'saving_answers');
                 UPDATE schema_meta SET version = 9;
                 COMMIT;",
            )?;
            version = 9;
        }
        if version < 10 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE preparation_jobs ADD COLUMN resolved_application_url TEXT;
                 UPDATE schema_meta SET version = 10;
                 COMMIT;",
            )?;
            version = 10;
        }
        if version < 11 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE roles ADD COLUMN legitimacy TEXT;
                 UPDATE schema_meta SET version = 11;
                 COMMIT;",
            )?;
            version = 11;
        }
        if version < 12 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE INDEX IF NOT EXISTS preparation_jobs_claimable
                   ON preparation_jobs(status, created_at, id);
                 UPDATE schema_meta SET version = 12;
                 COMMIT;",
            )?;
            version = 12;
        }
        if version < 13 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE source_occurrences ADD COLUMN posted_at TEXT;
                 UPDATE schema_meta SET version = 13;
                 COMMIT;",
            )?;
            version = 13;
        }
        if version < 14 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE preparation_jobs ADD COLUMN error_stage TEXT;
                 ALTER TABLE preparation_jobs ADD COLUMN error_detail TEXT;
                 ALTER TABLE notification_outbox ADD COLUMN event_kind TEXT NOT NULL DEFAULT 'discovery';
                 ALTER TABLE notification_outbox ADD COLUMN role_id TEXT;
                 ALTER TABLE notification_outbox ADD COLUMN preparation_id TEXT;
                 ALTER TABLE notification_outbox ADD COLUMN browser_session_id TEXT;
                 ALTER TABLE notification_outbox ADD COLUMN action_kind TEXT;
                 ALTER TABLE notification_outbox ADD COLUMN action_label TEXT;
                 ALTER TABLE notification_outbox ADD COLUMN delivered_via TEXT;
                 ALTER TABLE notification_outbox ADD COLUMN delivered_at TEXT;
                 ALTER TABLE notification_outbox ADD COLUMN last_error TEXT;
                 CREATE INDEX IF NOT EXISTS notification_outbox_delivery
                   ON notification_outbox(status, event_kind, created_at);
                 UPDATE schema_meta SET version = 14;
                 COMMIT;",
            )?;
            version = 14;
        }
        if version < 15 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE preparation_jobs ADD COLUMN retry_policy TEXT;
                 UPDATE schema_meta SET version = 15;
                 COMMIT;",
            )?;
            version = 15;
        }
        if version < 16 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE preparation_jobs ADD COLUMN cv_source TEXT;
                 UPDATE schema_meta SET version = 16;
                 COMMIT;",
            )?;
            version = 16;
        }
        if version < 17 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE IF NOT EXISTS evaluation_receipts (
                   receipt_key TEXT PRIMARY KEY,
                   role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
                   tracker_id INTEGER NOT NULL,
                   report_path TEXT NOT NULL,
                   report_hash TEXT NOT NULL,
                   upstream_revision TEXT NOT NULL,
                   compatibility_fingerprint TEXT NOT NULL,
                   source_identity_hash TEXT NOT NULL,
                   native_score REAL NOT NULL,
                   final_decision TEXT NOT NULL,
                   legitimacy TEXT NOT NULL,
                   strengths_json TEXT NOT NULL,
                   blockers_json TEXT NOT NULL,
                   gaps_json TEXT NOT NULL,
                   compensation TEXT,
                   authorization_confidence TEXT NOT NULL,
                   authorization_question TEXT NOT NULL,
                   material_uncertainty_json TEXT NOT NULL,
                   created_at TEXT NOT NULL,
                   UNIQUE(role_id, tracker_id, report_hash, upstream_revision, compatibility_fingerprint, source_identity_hash)
                 );
                 CREATE TABLE IF NOT EXISTS evaluation_sync (
                   role_id TEXT PRIMARY KEY REFERENCES roles(id) ON DELETE CASCADE,
                   state TEXT NOT NULL,
                   reason TEXT NOT NULL,
                   attempt INTEGER NOT NULL DEFAULT 0,
                   input_hash TEXT,
                   current_receipt_key TEXT REFERENCES evaluation_receipts(receipt_key),
                   lease_expires_at TEXT,
                   created_at TEXT NOT NULL,
                   updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS evaluation_sync_claimable
                   ON evaluation_sync(state, lease_expires_at, updated_at);
                 INSERT OR IGNORE INTO evaluation_sync(
                   role_id, state, reason, created_at, updated_at
                 )
                 SELECT id, 'awaiting_evaluation', 'evaluation_receipt_required',
                        updated_at, updated_at
                   FROM roles;
                 UPDATE schema_meta SET version = 17;
                 COMMIT;",
            )?;
            version = 17;
        }
        if version < 18 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE evaluation_receipts ADD COLUMN risk_level TEXT;
                 UPDATE evaluation_sync
                    SET state = 'needs_attention',
                        reason = 'canonical_evaluation_requires_refresh',
                        current_receipt_key = NULL,
                        lease_expires_at = NULL,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                  WHERE current_receipt_key IN (
                    SELECT receipt_key FROM evaluation_receipts WHERE risk_level IS NULL
                  );
                 UPDATE schema_meta SET version = 18;
                 COMMIT;",
            )?;
            version = 18;
        }
        if version < 19 {
            if !self.table_exists("browser_sessions")? {
                self.connection.execute_batch(
                    "BEGIN IMMEDIATE;
                     CREATE TABLE browser_sessions (
                       id TEXT PRIMARY KEY,
                       purpose TEXT NOT NULL,
                       role_id TEXT REFERENCES roles(id),
                       status TEXT NOT NULL,
                       ats TEXT,
                       page_title TEXT,
                       page_url TEXT,
                       snapshot_fingerprint TEXT,
                       fields_json TEXT,
                       field_count INTEGER NOT NULL DEFAULT 0,
                       safe_field_count INTEGER NOT NULL DEFAULT 0,
                       needs_user_count INTEGER NOT NULL DEFAULT 0,
                       error_code TEXT,
                       created_at TEXT NOT NULL,
                       updated_at TEXT NOT NULL,
                       preparation_id TEXT REFERENCES preparation_jobs(id),
                       provider TEXT,
                       answers_context_hash TEXT,
                       review_items_json TEXT,
                       fill_results_json TEXT,
                       answers_report_hash TEXT,
                       driver_owner TEXT,
                       driver_lease_id TEXT,
                       driver_lease_state TEXT NOT NULL DEFAULT 'none',
                       fallback_eligible INTEGER NOT NULL DEFAULT 0,
                       handoff_reason TEXT
                     );
                     CREATE UNIQUE INDEX IF NOT EXISTS browser_sessions_driver_lease
                       ON browser_sessions(driver_lease_id) WHERE driver_lease_id IS NOT NULL;
                     COMMIT;",
                )?;
            }
            if !self.column_exists("browser_sessions", "driver_owner")? {
                self.connection.execute(
                    "ALTER TABLE browser_sessions ADD COLUMN driver_owner TEXT",
                    [],
                )?;
            }
            if !self.column_exists("browser_sessions", "driver_lease_id")? {
                self.connection.execute(
                    "ALTER TABLE browser_sessions ADD COLUMN driver_lease_id TEXT",
                    [],
                )?;
            }
            if !self.column_exists("browser_sessions", "driver_lease_state")? {
                self.connection.execute(
                    "ALTER TABLE browser_sessions ADD COLUMN driver_lease_state TEXT NOT NULL DEFAULT 'none'",
                    [],
                )?;
            }
            if !self.column_exists("browser_sessions", "fallback_eligible")? {
                self.connection.execute(
                    "ALTER TABLE browser_sessions ADD COLUMN fallback_eligible INTEGER NOT NULL DEFAULT 0",
                    [],
                )?;
            }
            if !self.column_exists("browser_sessions", "handoff_reason")? {
                self.connection.execute(
                    "ALTER TABLE browser_sessions ADD COLUMN handoff_reason TEXT",
                    [],
                )?;
            }
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 UPDATE browser_sessions
                    SET driver_owner = 'extension',
                        driver_lease_id = lower(
                          hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' || substr(hex(randomblob(2)), 2) ||
                          '-' || substr('89ab', abs(random()) % 4 + 1, 1) || substr(hex(randomblob(2)), 2) ||
                          '-' || hex(randomblob(6))
                        ),
                        driver_lease_state = CASE
                          WHEN status IN ('review_required', 'submitted_tracking_pending', 'applied_recorded') THEN 'released'
                          WHEN status = 'action_required' THEN 'human_handoff'
                          ELSE 'held'
                        END
                  WHERE purpose = 'application';
                 CREATE UNIQUE INDEX IF NOT EXISTS browser_sessions_driver_lease
                   ON browser_sessions(driver_lease_id) WHERE driver_lease_id IS NOT NULL;
                 UPDATE schema_meta SET version = 19;
                 COMMIT;",
            )?;
            version = 19;
        }
        if version < 20 {
            let roles_have_canonical_status = self.connection.query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('roles')
                  WHERE name = 'canonical_status'",
                [],
                |row| row.get::<_, bool>(0),
            )?;
            let evaluation_sync_exists = self.connection.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master
                  WHERE type = 'table' AND name = 'evaluation_sync'",
                [],
                |row| row.get::<_, bool>(0),
            )?;
            let transaction = self.connection.transaction()?;
            if roles_have_canonical_status {
                transaction.execute(
                    "UPDATE roles SET canonical_status = CASE
                        WHEN LOWER(TRIM(COALESCE(canonical_status, ''))) = 'applied'
                          THEN 'Applied'
                        WHEN LOWER(TRIM(COALESCE(canonical_status, ''))) = 'discarded'
                          THEN 'Discarded'
                        ELSE canonical_status END
                      WHERE LOWER(TRIM(COALESCE(canonical_status, '')))
                        IN ('applied', 'discarded')",
                    [],
                )?;
            }
            if roles_have_canonical_status && evaluation_sync_exists {
                transaction.execute(
                    "UPDATE evaluation_sync
                        SET state = 'terminal', reason = 'canonical_terminal',
                            current_receipt_key = NULL, input_hash = NULL,
                            lease_expires_at = NULL,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                      WHERE role_id IN (
                        SELECT id FROM roles
                         WHERE canonical_status IN ('Applied', 'Discarded')
                      )",
                    [],
                )?;
            }
            transaction.execute("UPDATE schema_meta SET version = 20", [])?;
            transaction.commit()?;
            version = 20;
        }
        if version < 21 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE IF NOT EXISTS discovery_runs (
                   source_id TEXT NOT NULL,
                   run_id TEXT NOT NULL,
                   window_id TEXT NOT NULL,
                   supersedes_run_id TEXT,
                   coverage_start TEXT NOT NULL,
                   coverage_end TEXT NOT NULL,
                   timezone TEXT NOT NULL,
                   generated_at TEXT NOT NULL,
                   status TEXT NOT NULL,
                   digest TEXT NOT NULL,
                   finding_count INTEGER NOT NULL,
                   imported_at TEXT NOT NULL,
                   PRIMARY KEY (source_id, run_id)
                 );
                 CREATE TABLE IF NOT EXISTS discovery_cursors (
                   source_id TEXT PRIMARY KEY,
                   window_id TEXT NOT NULL,
                   run_id TEXT NOT NULL,
                   coverage_end TEXT NOT NULL,
                   updated_at TEXT NOT NULL
                 );
                 UPDATE schema_meta SET version = 21;
                 COMMIT;",
            )?;
            version = 21;
        }
        if version < 22 {
            self.connection.execute_batch("BEGIN IMMEDIATE;")?;
            let has_diagnostics_column: i64 = self.connection.query_row(
                "SELECT COUNT(*) FROM pragma_table_info('discovery_runs')
                  WHERE name = 'diagnostics_json'",
                [],
                |row| row.get(0),
            )?;
            if has_diagnostics_column == 0 {
                self.connection.execute(
                    "ALTER TABLE discovery_runs ADD COLUMN diagnostics_json TEXT NOT NULL DEFAULT '{}'",
                    [],
                )?;
            }
            let has_cursor_column: i64 = self.connection.query_row(
                "SELECT COUNT(*) FROM pragma_table_info('discovery_runs')
                  WHERE name = 'cursor_advanced'",
                [],
                |row| row.get(0),
            )?;
            if has_cursor_column == 0 {
                self.connection.execute(
                    "ALTER TABLE discovery_runs ADD COLUMN cursor_advanced INTEGER NOT NULL DEFAULT 0",
                    [],
                )?;
            }
            self.connection.execute_batch(
                "CREATE TABLE IF NOT EXISTS discovery_findings (
                   source_id TEXT NOT NULL,
                   finding_id TEXT NOT NULL,
                   source_role_id TEXT NOT NULL,
                   normalized_key TEXT NOT NULL,
                   last_run_id TEXT NOT NULL,
                   payload_hash TEXT NOT NULL,
                   PRIMARY KEY (source_id, finding_id),
                   UNIQUE (source_id, source_role_id)
                 );",
            )?;
            self.connection
                .execute("UPDATE schema_meta SET version = 22", [])?;
            self.connection.execute_batch("COMMIT;")?;
            version = 22;
        }
        if version < 23 {
            let roles_have_canonical_status = self.connection.query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('roles')
                  WHERE name = 'canonical_status'",
                [],
                |row| row.get::<_, bool>(0),
            )?;
            let evaluation_sync_exists = self.connection.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master
                  WHERE type = 'table' AND name = 'evaluation_sync'",
                [],
                |row| row.get::<_, bool>(0),
            )?;
            let transaction = self.connection.transaction()?;
            if roles_have_canonical_status {
                transaction.execute(
                    "UPDATE roles SET canonical_status = CASE
                        WHEN LOWER(TRIM(COALESCE(canonical_status, ''))) = 'rejected'
                          THEN 'Rejected'
                        ELSE canonical_status END
                      WHERE LOWER(TRIM(COALESCE(canonical_status, ''))) = 'rejected'",
                    [],
                )?;
            }
            if roles_have_canonical_status && evaluation_sync_exists {
                transaction.execute(
                    "UPDATE evaluation_sync
                        SET state = 'terminal', reason = 'canonical_terminal',
                            current_receipt_key = NULL, input_hash = NULL,
                            lease_expires_at = NULL,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                      WHERE role_id IN (
                        SELECT id FROM roles
                         WHERE canonical_status = 'Rejected'
                      )",
                    [],
                )?;
            }
            transaction.execute("UPDATE schema_meta SET version = 23", [])?;
            transaction.commit()?;
            version = 23;
        }
        if version < 24 {
            // Repair false capability holds that still point at a durable receipt,
            // supersede non-actionable staged catch-up placeholders, and clear
            // stuck prep on roles restored to Queue eligibility.
            let evaluation_sync_exists = self.connection.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master
                  WHERE type = 'table' AND name = 'evaluation_sync'",
                [],
                |row| row.get::<_, bool>(0),
            )?;
            let receipts_exist = self.connection.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master
                  WHERE type = 'table' AND name = 'evaluation_receipts'",
                [],
                |row| row.get::<_, bool>(0),
            )?;
            let runs_exist = self.connection.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master
                  WHERE type = 'table' AND name = 'runs'",
                [],
                |row| row.get::<_, bool>(0),
            )?;
            let preparation_jobs_exist = self.connection.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master
                  WHERE type = 'table' AND name = 'preparation_jobs'",
                [],
                |row| row.get::<_, bool>(0),
            )?;
            let transaction = self.connection.transaction()?;
            if evaluation_sync_exists && receipts_exist {
                transaction.execute(
                    "UPDATE evaluation_sync
                        SET state = CASE
                              WHEN receipt.final_decision = 'Skip'
                                OR receipt.legitimacy = 'Suspicious'
                                THEN 'hidden'
                              WHEN receipt.final_decision = 'Research first'
                                OR COALESCE(json_array_length(receipt.blockers_json), 0) > 0
                                OR COALESCE(json_array_length(receipt.gaps_json), 0) > 0
                                OR COALESCE(
                                     json_array_length(
                                       json_extract(
                                         receipt.material_uncertainty_json,
                                         '$.notEvaluatedRiskSignals'
                                       )
                                     ),
                                     0
                                   ) > 0
                                THEN 'needs_decision'
                              ELSE 'ready'
                            END,
                            reason = CASE
                              WHEN receipt.final_decision = 'Skip'
                                OR receipt.legitimacy = 'Suspicious'
                                THEN 'canonical_evaluation_not_viable'
                              ELSE 'canonical_evaluation_verified'
                            END,
                            lease_expires_at = NULL,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                       FROM evaluation_receipts AS receipt
                      WHERE evaluation_sync.current_receipt_key = receipt.receipt_key
                        AND evaluation_sync.state = 'needs_attention'
                        AND evaluation_sync.reason = 'evaluation_result_capability_unavailable'",
                    [],
                )?;
            }
            if runs_exist {
                transaction.execute(
                    "UPDATE runs
                        SET status = 'cancelled',
                            error_class = 'source_adapter_not_configured_superseded',
                            lease_expires_at = NULL,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                      WHERE status = 'action_required'
                        AND error_class = 'source_adapter_not_configured'
                        AND kind = 'catch_up'",
                    [],
                )?;
                transaction.execute(
                    "UPDATE run_steps
                        SET status = 'cancelled',
                            error_class = 'source_adapter_not_configured_superseded',
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                      WHERE status IN ('action_required', 'blocked')
                        AND run_id IN (
                          SELECT id FROM runs
                           WHERE status = 'cancelled'
                             AND error_class = 'source_adapter_not_configured_superseded'
                        )",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO activity(id, kind, message, occurred_at)
                     SELECT lower(hex(randomblob(16))), 'schedule',
                            'Superseded obsolete staged catch-up placeholders that had no recovery action.',
                            strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                      WHERE EXISTS (
                        SELECT 1 FROM runs
                         WHERE status = 'cancelled'
                           AND error_class = 'source_adapter_not_configured_superseded'
                      )
                        AND NOT EXISTS (
                          SELECT 1 FROM activity
                           WHERE message = 'Superseded obsolete staged catch-up placeholders that had no recovery action.'
                        )",
                    [],
                )?;
            }
            if evaluation_sync_exists && preparation_jobs_exist {
                // Mirror Q7=B force cleanup for roles restored to Queue eligibility.
                transaction.execute(
                    "UPDATE preparation_jobs
                        SET status = 'cancelled', step = 'cancelled',
                            error_class = NULL, error_stage = NULL, error_detail = NULL,
                            retry_policy = NULL,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                      WHERE status = 'action_required'
                        AND role_id IN (
                          SELECT evaluation.role_id
                            FROM evaluation_sync evaluation
                           WHERE evaluation.state IN ('ready', 'needs_decision')
                             AND evaluation.reason = 'canonical_evaluation_verified'
                             AND evaluation.current_receipt_key IS NOT NULL
                        )",
                    [],
                )?;
                transaction.execute(
                    "UPDATE roles
                        SET preparation_state = 'not_started',
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                      WHERE preparation_state IN ('failed', 'queued', 'preparing')
                        AND id IN (
                          SELECT evaluation.role_id
                            FROM evaluation_sync evaluation
                           WHERE evaluation.state IN ('ready', 'needs_decision')
                             AND evaluation.reason = 'canonical_evaluation_verified'
                             AND evaluation.current_receipt_key IS NOT NULL
                        )",
                    [],
                )?;
                transaction.execute(
                    "UPDATE notification_outbox
                        SET status = 'expired', last_error = 'stuck_preparation_cleanup'
                      WHERE status IN ('pending', 'delivering')
                        AND event_kind = 'preparation_failed'
                        AND role_id IN (
                          SELECT evaluation.role_id
                            FROM evaluation_sync evaluation
                           WHERE evaluation.state IN ('ready', 'needs_decision')
                             AND evaluation.reason = 'canonical_evaluation_verified'
                             AND evaluation.current_receipt_key IS NOT NULL
                        )",
                    [],
                )?;
            }
            transaction.execute("UPDATE schema_meta SET version = 24", [])?;
            transaction.commit()?;
            version = 24;
        }
        if version < 25 {
            // Restore roles whose durable receipts were cleared because they
            // carried the composed executor fingerprint instead of the read probe.
            let evaluation_sync_exists = self.connection.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master
                  WHERE type = 'table' AND name = 'evaluation_sync'",
                [],
                |row| row.get::<_, bool>(0),
            )?;
            let receipts_exist = self.connection.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master
                  WHERE type = 'table' AND name = 'evaluation_receipts'",
                [],
                |row| row.get::<_, bool>(0),
            )?;
            let transaction = self.connection.transaction()?;
            if evaluation_sync_exists && receipts_exist {
                transaction.execute(
                    "UPDATE evaluation_sync
                        SET current_receipt_key = (
                              SELECT receipt.receipt_key
                                FROM evaluation_receipts AS receipt
                               WHERE receipt.role_id = evaluation_sync.role_id
                               ORDER BY receipt.created_at DESC, receipt.receipt_key DESC
                               LIMIT 1
                            ),
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                      WHERE state = 'needs_attention'
                        AND reason = 'evaluation_compatibility_changed'
                        AND current_receipt_key IS NULL
                        AND EXISTS (
                          SELECT 1 FROM evaluation_receipts AS receipt
                           WHERE receipt.role_id = evaluation_sync.role_id
                        )",
                    [],
                )?;
                transaction.execute(
                    "UPDATE evaluation_sync
                        SET state = CASE
                              WHEN receipt.final_decision = 'Skip'
                                OR receipt.legitimacy = 'Suspicious'
                                THEN 'hidden'
                              WHEN receipt.final_decision = 'Research first'
                                OR COALESCE(json_array_length(receipt.blockers_json), 0) > 0
                                OR COALESCE(json_array_length(receipt.gaps_json), 0) > 0
                                OR COALESCE(
                                     json_array_length(
                                       json_extract(
                                         receipt.material_uncertainty_json,
                                         '$.notEvaluatedRiskSignals'
                                       )
                                     ),
                                     0
                                   ) > 0
                                THEN 'needs_decision'
                              ELSE 'ready'
                            END,
                            reason = CASE
                              WHEN receipt.final_decision = 'Skip'
                                OR receipt.legitimacy = 'Suspicious'
                                THEN 'canonical_evaluation_not_viable'
                              ELSE 'canonical_evaluation_verified'
                            END,
                            lease_expires_at = NULL,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                       FROM evaluation_receipts AS receipt
                      WHERE evaluation_sync.current_receipt_key = receipt.receipt_key
                        AND evaluation_sync.state = 'needs_attention'
                        AND evaluation_sync.reason IN (
                          'evaluation_result_capability_unavailable',
                          'evaluation_compatibility_changed'
                        )",
                    [],
                )?;
            }
            transaction.execute("UPDATE schema_meta SET version = 25", [])?;
            transaction.commit()?;
            version = 25;
        }
        if version != SCHEMA_VERSION {
            return Err(StoreError::Database(rusqlite::Error::InvalidQuery));
        }
        Ok(())
    }

    pub fn import_dataset(&mut self, payload: &str) -> Result<ImportResult, StoreError> {
        let dataset: DiscoveryDataset = serde_json::from_str(payload)?;
        if dataset.schema_version != 1 {
            return Err(StoreError::UnsupportedDataset(dataset.schema_version));
        }

        let transaction = self.connection.transaction()?;
        let mut result = ImportResult::default();

        for finding in &dataset.findings {
            let finding_result =
                reconcile_discovery_finding(&transaction, finding, &Utc::now().to_rfc3339())?;
            result.imported += finding_result.imported;
            result.updated += finding_result.updated;
            result.unchanged += finding_result.unchanged;
        }

        transaction.execute(
            "INSERT INTO settings(key, value, updated_at) VALUES ('last_successful_discovery_at', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![dataset.generated_at, Utc::now().to_rfc3339()],
        )?;
        transaction.execute(
            "INSERT INTO activity(id, kind, message, occurred_at) VALUES (?1, 'import', ?2, ?3)",
            params![
                Uuid::new_v4().to_string(),
                format!(
                    "Discovery snapshot reconciled: {} new, {} updated, {} unchanged.",
                    result.imported, result.updated, result.unchanged
                ),
                Utc::now().to_rfc3339(),
            ],
        )?;
        transaction.commit()?;
        Ok(result)
    }

    /// Ingest one immutable, digest-sealed discovery-run artifact.
    ///
    /// This path deliberately does not execute a source. It records the typed
    /// producer attempt, reconciles findings only for completed runs, and keeps
    /// a source-specific successful-coverage cursor separate from the legacy
    /// snapshot importer and scheduler authority.
    pub fn import_discovery_run(
        &mut self,
        payload: &str,
    ) -> Result<DiscoveryRunImportResult, StoreError> {
        if payload.len() > MAX_DISCOVERY_RUN_BYTES {
            return Err(StoreError::InvalidDiscoveryRun(
                "discovery run exceeds the maximum payload size".to_string(),
            ));
        }
        let raw: serde_json::Value = serde_json::from_str(payload)?;
        validate_discovery_run_raw_bounds(&raw)?;
        let run: DiscoveryRun = serde_json::from_value(raw.clone())?;
        validate_discovery_run(&run)?;
        let digest = discovery_run_digest_value(&raw)?;
        if run.integrity.digest != digest {
            return Err(StoreError::InvalidDiscoveryRun(
                "integrity.digest does not match the canonical payload".to_string(),
            ));
        }

        let transaction = self.connection.transaction()?;
        let source_exists = transaction.query_row(
            "SELECT COUNT(*) > 0 FROM source_schedules WHERE source_id = ?1",
            [&run.source.source_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !source_exists {
            return Err(StoreError::UnknownSource(run.source.source_id.clone()));
        }

        let existing = transaction
            .query_row(
                "SELECT digest FROM discovery_runs WHERE source_id = ?1 AND run_id = ?2",
                params![run.source.source_id, run.run_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(existing_digest) = existing {
            if existing_digest == digest {
                transaction.commit()?;
                return Ok(DiscoveryRunImportResult {
                    replayed: true,
                    ..Default::default()
                });
            }
            return Err(StoreError::InvalidDiscoveryRun(
                "run identity was reused with a different digest".to_string(),
            ));
        }

        let previous_cursor = transaction
            .query_row(
                "SELECT coverage_end FROM discovery_cursors WHERE source_id = ?1",
                [&run.source.source_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let coverage_end = DateTime::parse_from_rfc3339(&run.coverage.window_end)
            .map_err(|_| StoreError::InvalidDiscoveryRun("invalid coverage end".to_string()))?
            .with_timezone(&Utc);
        let coverage_is_newer = previous_cursor
            .as_deref()
            .map(|value| {
                DateTime::parse_from_rfc3339(value)
                    .map(|cursor| coverage_end > cursor.with_timezone(&Utc))
                    .unwrap_or(false)
            })
            .unwrap_or(true);

        if let Some(supersedes_run_id) = &run.supersedes_run_id {
            let superseded = transaction
                .query_row(
                    "SELECT window_id, coverage_start, coverage_end, timezone, status
                       FROM discovery_runs
                      WHERE source_id = ?1 AND run_id = ?2",
                    params![run.source.source_id, supersedes_run_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
                .optional()?;
            let Some((window_id, coverage_start, coverage_end, timezone, status)) = superseded
            else {
                return Err(StoreError::InvalidDiscoveryRun(
                    "supersedesRunId does not reference a known attempt for this source"
                        .to_string(),
                ));
            };
            if window_id != run.window_id
                || coverage_start != run.coverage.window_start
                || coverage_end != run.coverage.window_end
                || timezone != run.coverage.timezone
            {
                return Err(StoreError::InvalidDiscoveryRun(
                    "retry coverage does not exactly match the superseded attempt".to_string(),
                ));
            }
            if status == "completed" {
                return Err(StoreError::InvalidDiscoveryRun(
                    "supersedesRunId must reference the latest partial or failed attempt"
                        .to_string(),
                ));
            }
            let latest_attempt: String = transaction.query_row(
                "SELECT run_id FROM discovery_runs
                  WHERE source_id = ?1 AND window_id = ?2
                    AND coverage_start = ?3 AND coverage_end = ?4 AND timezone = ?5
                  ORDER BY imported_at DESC, rowid DESC LIMIT 1",
                params![
                    run.source.source_id,
                    run.window_id,
                    run.coverage.window_start,
                    run.coverage.window_end,
                    run.coverage.timezone,
                ],
                |row| row.get(0),
            )?;
            if latest_attempt != *supersedes_run_id {
                return Err(StoreError::InvalidDiscoveryRun(
                    "supersedesRunId must reference the latest attempt for the window".to_string(),
                ));
            }
        } else {
            let existing_attempt: Option<String> = transaction
                .query_row(
                    "SELECT run_id FROM discovery_runs
                      WHERE source_id = ?1 AND window_id = ?2
                        AND coverage_start = ?3 AND coverage_end = ?4 AND timezone = ?5
                      LIMIT 1",
                    params![
                        run.source.source_id,
                        run.window_id,
                        run.coverage.window_start,
                        run.coverage.window_end,
                        run.coverage.timezone,
                    ],
                    |row| row.get(0),
                )
                .optional()?;
            if existing_attempt.is_some() {
                return Err(StoreError::InvalidDiscoveryRun(
                    "a repeated attempt for this window must set supersedesRunId".to_string(),
                ));
            }
        }

        if run.status == "completed" {
            validate_discovery_finding_identities(&transaction, &run)?;
        }

        let now = Utc::now().to_rfc3339();
        let initial_diagnostics = discovery_run_diagnostics(&run, &now, false)?;
        transaction.execute(
            "INSERT INTO discovery_runs(
               source_id, run_id, window_id, supersedes_run_id, coverage_start,
               coverage_end, timezone, generated_at, status, digest, finding_count,
               imported_at, diagnostics_json, cursor_advanced
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                run.source.source_id,
                run.run_id,
                run.window_id,
                run.supersedes_run_id,
                run.coverage.window_start,
                run.coverage.window_end,
                run.coverage.timezone,
                run.generated_at,
                run.status,
                digest,
                run.findings.len() as i64,
                now,
                initial_diagnostics,
                false,
            ],
        )?;

        let mut result = DiscoveryRunImportResult {
            recorded: true,
            ..Default::default()
        };
        if run.status == "completed" && coverage_is_newer {
            for finding in &run.findings {
                let legacy = discovery_run_finding_to_legacy(finding);
                let finding_result = reconcile_typed_discovery_finding(
                    &transaction,
                    finding,
                    &legacy,
                    &run.run_id,
                    &now,
                )?;
                result.imported += finding_result.imported;
                result.updated += finding_result.updated;
                result.unchanged += finding_result.unchanged;
            }

            transaction.execute(
                "INSERT INTO discovery_cursors(
                       source_id, window_id, run_id, coverage_end, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(source_id) DO UPDATE SET
                       window_id = excluded.window_id,
                       run_id = excluded.run_id,
                       coverage_end = excluded.coverage_end,
                       updated_at = excluded.updated_at",
                params![
                    run.source.source_id,
                    run.window_id,
                    run.run_id,
                    run.coverage.window_end,
                    now,
                ],
            )?;
            result.cursor_advanced = true;
            let diagnostics = discovery_run_diagnostics(&run, &now, true)?;
            transaction.execute(
                "UPDATE discovery_runs SET diagnostics_json = ?1, cursor_advanced = 1
                  WHERE source_id = ?2 AND run_id = ?3",
                params![diagnostics, run.source.source_id, run.run_id],
            )?;
        }

        transaction.execute(
            "INSERT INTO activity(id, kind, message, occurred_at)
             VALUES (?1, 'import', ?2, ?3)",
            params![
                Uuid::new_v4().to_string(),
                format!(
                    "Discovery run {} recorded: {} new, {} updated, {} unchanged{}.",
                    run.run_id,
                    result.imported,
                    result.updated,
                    result.unchanged,
                    if result.cursor_advanced {
                        ", cursor advanced"
                    } else {
                        ""
                    }
                ),
                now,
            ],
        )?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn set_background_enabled(&mut self, enabled: bool) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO settings(key, value, updated_at) VALUES ('background_enabled', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![if enabled { "true" } else { "false" }, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn evaluation_sync_roles(&self) -> Result<Vec<EvaluationSyncRole>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT r.id, r.company, r.title, r.application_url, r.canonical_status,
                    r.canonical_tracker_id,
                    r.normalized_key,
                    COALESCE((
                      SELECT group_concat(ordered.identity, '|') FROM (
                        SELECT source_id || ':' || source_role_id || ':' || payload_hash AS identity
                          FROM source_occurrences WHERE role_id = r.id
                         ORDER BY source_id, source_role_id
                      ) ordered
                    ), '')
               FROM roles r
              ORDER BY r.id",
        )?;
        statement
            .query_map([], |row| {
                let normalized_key = row.get::<_, String>(6)?;
                let occurrences = row.get::<_, String>(7)?;
                Ok(EvaluationSyncRole {
                    role_id: row.get(0)?,
                    company: row.get(1)?,
                    title: row.get(2)?,
                    application_url: row.get(3)?,
                    canonical_status: row.get(4)?,
                    canonical_tracker_id: row.get(5)?,
                    source_identity_hash: format!(
                        "{:x}",
                        Sha256::digest(format!("{normalized_key}\n{occurrences}"))
                    ),
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn recover_expired_evaluation_syncs(&mut self) -> Result<usize, StoreError> {
        let now = Utc::now().to_rfc3339();
        self.connection
            .execute(
                "UPDATE evaluation_sync
                    SET state = 'awaiting_evaluation', reason = 'evaluation_sync_lease_expired',
                        lease_expires_at = NULL, updated_at = ?1
                  WHERE state = 'syncing' AND lease_expires_at < ?1",
                [&now],
            )
            .map_err(StoreError::from)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn invalidate_evaluation_compatibility(
        &mut self,
        upstream_revision: Option<&str>,
        compatibility_fingerprint: Option<&str>,
    ) -> Result<usize, StoreError> {
        self.invalidate_evaluation_compatibility_with_alternates(
            upstream_revision,
            compatibility_fingerprint,
            None,
        )
    }

    /// Invalidate receipts that no longer match the active evaluation probes.
    ///
    /// Accepts the result-read fingerprint plus an optional executor/composed
    /// alternate so HFW-composed A–G receipts are not wiped when only the read
    /// probe fingerprint differs.
    pub fn invalidate_evaluation_compatibility_with_alternates(
        &mut self,
        upstream_revision: Option<&str>,
        compatibility_fingerprint: Option<&str>,
        alternate_compatibility_fingerprint: Option<&str>,
    ) -> Result<usize, StoreError> {
        let (Some(upstream_revision), Some(compatibility_fingerprint)) =
            (upstream_revision, compatibility_fingerprint)
        else {
            return Ok(0);
        };
        let now = Utc::now().to_rfc3339();
        let alternate = alternate_compatibility_fingerprint
            .filter(|value| !value.is_empty() && *value != compatibility_fingerprint);
        self.connection
            .execute(
                "UPDATE evaluation_sync
                    SET state = 'needs_attention', reason = 'evaluation_compatibility_changed',
                        current_receipt_key = NULL, input_hash = NULL,
                        lease_expires_at = NULL, updated_at = ?1
                  WHERE current_receipt_key IS NOT NULL
                    AND NOT EXISTS (
                      SELECT 1 FROM evaluation_receipts receipt
                       WHERE receipt.receipt_key = evaluation_sync.current_receipt_key
                         AND receipt.upstream_revision = ?2
                         AND (
                           receipt.compatibility_fingerprint = ?3
                           OR (
                             ?4 IS NOT NULL
                             AND receipt.compatibility_fingerprint = ?4
                           )
                         )
                    )",
                params![now, upstream_revision, compatibility_fingerprint, alternate],
            )
            .map_err(StoreError::from)
    }

    pub fn mark_evaluation_sync_unavailable(
        &mut self,
        role_id: &str,
        reason: &str,
    ) -> Result<(), StoreError> {
        let reason = sanitize_evaluation_reason(reason);
        // Do not demote roles that already hold a durable verified receipt. A
        // transient read-capability outage must not yank them from Queue again.
        self.connection.execute(
            "UPDATE evaluation_sync
                SET state = 'needs_attention', reason = ?1,
                    attempt = 0, lease_expires_at = NULL, updated_at = ?2
              WHERE role_id = ?3
                AND state <> 'terminal'
                AND NOT (
                  state IN ('ready', 'needs_decision', 'hidden')
                  AND current_receipt_key IS NOT NULL
                )",
            params![reason, Utc::now().to_rfc3339(), role_id],
        )?;
        Ok(())
    }

    /// User-triggered Retry history sync must re-attempt globally recoverable
    /// holds even after the automatic attempt budget is exhausted.
    pub fn reset_exhausted_global_reconcile_attempts(&mut self) -> Result<usize, StoreError> {
        let now = Utc::now().to_rfc3339();
        let held = {
            let mut statement = self.connection.prepare(
                "SELECT role_id, reason FROM evaluation_sync WHERE state = 'needs_attention'",
            )?;
            statement
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut changed = 0usize;
        for (role_id, reason) in held {
            if !PreQueueRecovery::is_global_reconcile_reason(&reason) {
                continue;
            }
            changed += self.connection.execute(
                "UPDATE evaluation_sync
                    SET attempt = 0, input_hash = NULL, lease_expires_at = NULL, updated_at = ?1
                  WHERE role_id = ?2 AND state = 'needs_attention'",
                params![now, role_id],
            )?;
        }
        Ok(changed)
    }

    pub fn claim_evaluation_sync(
        &mut self,
        role_id: &str,
        input_hash: &str,
    ) -> Result<bool, StoreError> {
        self.claim_evaluation_sync_with_lease(role_id, input_hash, EVALUATION_LEASE_SECONDS)
    }

    pub fn claim_evaluation_sync_with_lease(
        &mut self,
        role_id: &str,
        input_hash: &str,
        lease_seconds: i64,
    ) -> Result<bool, StoreError> {
        let transaction = self.connection.transaction()?;
        let now = Utc::now();
        let current = transaction
            .query_row(
                "SELECT state, input_hash, lease_expires_at, attempt
                   FROM evaluation_sync WHERE role_id = ?1",
                [role_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((state, stored_hash, lease, attempt)) = current else {
            return Err(StoreError::InvalidPreparation(
                "role has no evaluation lifecycle".to_string(),
            ));
        };
        if state == "terminal"
            || matches!(state.as_str(), "ready" | "needs_decision" | "hidden")
                && stored_hash.as_deref() == Some(input_hash)
            || state == "needs_attention"
                && stored_hash.as_deref() == Some(input_hash)
                && attempt >= MAX_EVALUATION_SYNC_ATTEMPTS
        {
            return Ok(false);
        }
        if state == "syncing"
            && lease
                .as_deref()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .is_some_and(|value| value.with_timezone(&Utc) >= now)
        {
            return Ok(false);
        }
        transaction.execute(
            "UPDATE evaluation_sync
                SET state = 'syncing', reason = 'evaluation_result_read_pending',
                    attempt = CASE WHEN input_hash = ?1 THEN attempt + 1 ELSE 1 END,
                    input_hash = ?1,
                    lease_expires_at = ?2, updated_at = ?3
              WHERE role_id = ?4",
            params![
                input_hash,
                (now + Duration::seconds(lease_seconds.max(1))).to_rfc3339(),
                now.to_rfc3339(),
                role_id
            ],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn hold_evaluation(
        &mut self,
        role_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<(), StoreError> {
        if !matches!(
            state,
            "awaiting_evaluation" | "needs_attention" | "terminal"
        ) {
            return Err(StoreError::InvalidPreparation(
                "invalid evaluation hold state".to_string(),
            ));
        }
        let reason = sanitize_evaluation_reason(reason);
        self.connection.execute(
            "UPDATE evaluation_sync
                SET state = ?1, reason = ?2, current_receipt_key = NULL,
                    lease_expires_at = NULL, updated_at = ?3
              WHERE role_id = ?4",
            params![state, reason, Utc::now().to_rfc3339(), role_id],
        )?;
        Ok(())
    }

    pub fn complete_evaluation_sync(
        &mut self,
        role: &EvaluationSyncRole,
        input_hash: &str,
        result: &EvaluationResultRead,
    ) -> Result<bool, StoreError> {
        if result.canonical.tracker_id < 1
            || result.canonical.score != result.evaluation.score
            || !(1.0..=5.0).contains(&result.evaluation.score)
            || !company_matches(&role.company, &result.role.company)
            || !title_matches(&role.title, &result.role.title)
        {
            return Err(StoreError::InvalidPreparation(
                "evaluation result identity or native score does not match the role".to_string(),
            ));
        }
        let transaction = self.connection.transaction()?;
        let leased = transaction.query_row(
            "SELECT COUNT(*) FROM evaluation_sync
              WHERE role_id = ?1 AND state = 'syncing' AND input_hash = ?2",
            params![role.role_id, input_hash],
            |row| row.get::<_, i64>(0),
        )?;
        if leased != 1 {
            return Err(StoreError::InvalidPreparation(
                "evaluation sync lease is no longer current".to_string(),
            ));
        }
        let canonical_status = transaction
            .query_row(
                "SELECT canonical_status FROM roles WHERE id = ?1",
                [&role.role_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        if canonical_status
            .as_deref()
            .is_some_and(is_terminal_canonical_status)
        {
            transaction.execute(
                "UPDATE evaluation_sync
                    SET state = 'terminal', reason = 'canonical_terminal',
                        current_receipt_key = NULL, input_hash = NULL,
                        lease_expires_at = NULL, updated_at = ?1
                  WHERE role_id = ?2",
                params![Utc::now().to_rfc3339(), role.role_id],
            )?;
            transaction.commit()?;
            return Ok(false);
        }
        let needs_decision = result.evaluation.final_decision == "Research first"
            || result.evaluation.confidence != "High"
            || !result.evaluation.blockers.is_empty()
            || !result.evaluation.gaps.is_empty()
            || !result
                .evaluation
                .material_uncertainty
                .not_evaluated_risk_signals
                .is_empty();
        let state = if result.evaluation.final_decision == "Skip"
            || result.evaluation.legitimacy_tier == "Suspicious"
        {
            "hidden"
        } else if needs_decision {
            "needs_decision"
        } else {
            "ready"
        };
        let strengths = result
            .evaluation
            .strengths
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>();
        let blockers = result
            .evaluation
            .blockers
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>();
        let gaps = result
            .evaluation
            .gaps
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>();
        let material_uncertainty = serde_json::json!({
            "confidence": result.evaluation.material_uncertainty.confidence,
            "authorizationQuestion": result.evaluation.material_uncertainty.authorization_question,
            "notEvaluatedRiskSignals": result.evaluation.material_uncertainty.not_evaluated_risk_signals,
        });
        let receipt_identity = serde_json::json!({
            "roleId": role.role_id,
            "trackerId": result.canonical.tracker_id,
            "report": result.report,
            "upstreamRevision": result.upstream_revision,
            "compatibilityFingerprint": result.compatibility_fingerprint,
            "sourceIdentityHash": role.source_identity_hash,
        });
        let receipt_key = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&receipt_identity)?)
        );
        let now = Utc::now().to_rfc3339();
        transaction.execute(
            "INSERT INTO evaluation_receipts(
               receipt_key, role_id, tracker_id, report_path, report_hash,
               upstream_revision, compatibility_fingerprint, source_identity_hash,
               native_score, final_decision, legitimacy, risk_level, strengths_json,
               blockers_json, gaps_json, compensation, authorization_confidence,
               authorization_question, material_uncertainty_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                       ?14, ?15, ?16, ?17, ?18, ?19, ?20)
             ON CONFLICT(receipt_key) DO UPDATE SET risk_level = excluded.risk_level
               WHERE evaluation_receipts.risk_level IS NULL",
            params![
                receipt_key,
                role.role_id,
                result.canonical.tracker_id,
                result.report.path,
                result.report.sha256,
                result.upstream_revision,
                result.compatibility_fingerprint,
                role.source_identity_hash,
                result.evaluation.score,
                result.evaluation.final_decision,
                result.evaluation.legitimacy_tier,
                result.evaluation.risk_level,
                serde_json::to_string(&strengths)?,
                serde_json::to_string(&blockers)?,
                serde_json::to_string(&gaps)?,
                result.evaluation.compensation.advertised,
                result.evaluation.authorization.confidence,
                result.evaluation.authorization.question,
                material_uncertainty.to_string(),
                now,
            ],
        )?;
        transaction.execute(
            "UPDATE evaluation_sync
                SET state = ?1, reason = ?2, current_receipt_key = ?3,
                    lease_expires_at = NULL, updated_at = ?4
              WHERE role_id = ?5 AND state = 'syncing' AND input_hash = ?6",
            params![
                state,
                if state == "hidden" {
                    "canonical_evaluation_not_viable"
                } else {
                    "canonical_evaluation_verified"
                },
                receipt_key,
                now,
                role.role_id,
                input_hash,
            ],
        )?;
        transaction.execute(
            "UPDATE roles SET canonical_tracker_id = ?1, canonical_status = ?2, updated_at = ?3
              WHERE id = ?4",
            params![
                result.canonical.tracker_id,
                result.canonical.status,
                now,
                role.role_id
            ],
        )?;
        transaction.commit()?;
        Ok(matches!(state, "ready" | "needs_decision"))
    }

    pub fn background_enabled(&self) -> Result<bool, StoreError> {
        Ok(setting(&self.connection, "background_enabled")?.as_deref() == Some("true"))
    }

    pub fn queue_filters(&self) -> Result<QueueFilters, StoreError> {
        let Some(value) = setting(&self.connection, "queue_filters")? else {
            return Ok(QueueFilters::default());
        };
        serde_json::from_str(&value).map_err(StoreError::InvalidDataset)
    }

    pub fn queue_filters_configured(&self) -> Result<bool, StoreError> {
        Ok(setting(&self.connection, "queue_filters")?.is_some())
    }

    pub fn set_queue_filters(&mut self, filters: &QueueFilters) -> Result<(), StoreError> {
        validate_queue_filters(filters)?;
        self.connection.execute(
            "INSERT INTO settings(key, value, updated_at) VALUES ('queue_filters', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![serde_json::to_string(filters)?, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn cv_fallback_setting(&self) -> Result<CvFallbackSetting, StoreError> {
        let Some(value) = setting(&self.connection, "user_reviewed_cv_fallback")? else {
            return Ok(CvFallbackSetting::default());
        };
        serde_json::from_str(&value).map_err(StoreError::InvalidDataset)
    }

    pub fn set_cv_fallback_setting(
        &mut self,
        setting: &CvFallbackSetting,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO settings(key, value, updated_at) VALUES ('user_reviewed_cv_fallback', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![serde_json::to_string(setting)?, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn set_adapter_ready(&mut self, ready: bool) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO settings(key, value, updated_at) VALUES ('adapter_ready', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![if ready { "true" } else { "false" }, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn approved_extension_id(&self) -> Result<Option<String>, StoreError> {
        Ok(setting(&self.connection, "approved_extension_id")?)
    }

    pub fn pending_extension_id(&self) -> Result<Option<String>, StoreError> {
        Ok(setting(&self.connection, "pending_extension_id")?)
    }

    pub fn approved_installation_id(&self) -> Result<Option<String>, StoreError> {
        Ok(setting(&self.connection, "approved_installation_id")?)
    }

    pub fn pending_installation_id(&self) -> Result<Option<String>, StoreError> {
        Ok(setting(&self.connection, "pending_installation_id")?)
    }

    pub fn selected_chrome_profile(&self) -> Result<Option<String>, StoreError> {
        Ok(setting(&self.connection, "selected_chrome_profile")?)
    }

    pub fn set_pending_browser_identity(
        &mut self,
        extension_id: &str,
        installation_id: &str,
    ) -> Result<(), StoreError> {
        if self.pending_extension_id()?.as_deref() == Some(extension_id)
            && self.pending_installation_id()?.as_deref() == Some(installation_id)
        {
            return Ok(());
        }
        let now = Utc::now().to_rfc3339();
        for (key, value) in [
            ("pending_extension_id", extension_id),
            ("pending_installation_id", installation_id),
        ] {
            self.connection.execute(
                "INSERT INTO settings(key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![key, value, now],
            )?;
        }
        self.connection.execute(
            "INSERT INTO activity(id, kind, message, occurred_at) VALUES (?1, 'browser', 'A browser extension requested pairing.', ?2)",
            params![Uuid::new_v4().to_string(), now],
        )?;
        Ok(())
    }

    pub fn configure_browser(
        &mut self,
        extension_id: &str,
        installation_id: &str,
        profile_id: &str,
    ) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        let now = Utc::now().to_rfc3339();
        for (key, value) in [
            ("approved_extension_id", extension_id),
            ("approved_installation_id", installation_id),
            ("selected_chrome_profile", profile_id),
        ] {
            transaction.execute(
                "INSERT INTO settings(key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![key, value, now],
            )?;
        }
        transaction.execute(
            "DELETE FROM settings WHERE key IN ('pending_extension_id', 'pending_installation_id')",
            [],
        )?;
        transaction.execute(
            "INSERT INTO activity(id, kind, message, occurred_at) VALUES (?1, 'browser', 'The selected Chrome profile and extension were connected.', ?2)",
            params![Uuid::new_v4().to_string(), now],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn record_browser_connected(&mut self) -> Result<(), StoreError> {
        let now = Utc::now();
        if let Some(previous) = self.browser_last_connected_at()?
            && let Ok(previous) = chrono::DateTime::parse_from_rfc3339(&previous)
            && now.signed_duration_since(previous.with_timezone(&Utc)) < Duration::seconds(30)
        {
            return Ok(());
        }
        self.connection.execute(
            "INSERT INTO settings(key, value, updated_at) VALUES ('browser_last_connected_at', ?1, ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            [now.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn browser_last_connected_at(&self) -> Result<Option<String>, StoreError> {
        Ok(setting(&self.connection, "browser_last_connected_at")?)
    }

    pub fn queue_browser_connection_check(&mut self) -> Result<BrowserSessionSummary, StoreError> {
        if self.approved_extension_id()?.is_none() || self.approved_installation_id()?.is_none() {
            return Err(StoreError::InvalidBrowserTransition(
                "connect an approved Chrome extension before checking a page".to_string(),
            ));
        }
        let existing = self
            .connection
            .query_row(
                "SELECT id FROM browser_sessions
                  WHERE purpose = 'connection_check'
                    AND status IN ('waiting_for_extension', 'inspecting', 'releasing')
                  ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return self.browser_session(&id);
        }
        let transaction = self.connection.transaction()?;
        let now = Utc::now().to_rfc3339();
        let session_id = Uuid::new_v4().to_string();
        transaction.execute(
            "INSERT INTO browser_sessions(
               id, purpose, status, created_at, updated_at
             ) VALUES (?1, 'connection_check', 'waiting_for_extension', ?2, ?2)",
            params![session_id, now],
        )?;
        insert_browser_command(
            &transaction,
            &session_id,
            "inspect_request",
            &serde_json::json!({}),
            &now,
        )?;
        transaction.execute(
            "INSERT INTO activity(id, kind, message, occurred_at)
             VALUES (?1, 'browser', 'Waiting for the selected Chrome extension to inspect the active HTTPS application page.', ?2)",
            params![Uuid::new_v4().to_string(), now],
        )?;
        transaction.commit()?;
        self.browser_session(&session_id)
    }

    pub fn queue_application_session(
        &mut self,
        preparation_id: &str,
    ) -> Result<BrowserSessionSummary, StoreError> {
        if self.approved_extension_id()?.is_none() || self.approved_installation_id()?.is_none() {
            return Err(StoreError::InvalidBrowserTransition(
                "connect an approved Chrome profile before continuing".to_string(),
            ));
        }
        let preparation = self
            .connection
            .query_row(
                "SELECT p.role_id, p.provider, p.report_path,
                        COALESCE(p.resolved_application_url, r.application_url),
                        r.canonical_status
               FROM preparation_jobs p JOIN roles r ON r.id = p.role_id
              WHERE p.id = ?1 AND p.status = 'completed'",
                [preparation_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::InvalidBrowserTransition(
                    "the preparation is missing or not completed".to_string(),
                )
            })?;
        if preparation
            .4
            .as_deref()
            .is_some_and(is_terminal_canonical_status)
        {
            return Err(StoreError::InvalidBrowserTransition(
                "the role already has a terminal canonical outcome".to_string(),
            ));
        }
        if preparation.2.as_deref().unwrap_or_default().is_empty() {
            return Err(StoreError::InvalidBrowserTransition(
                "the preparation has no canonical report".to_string(),
            ));
        }
        let url = preparation.3.ok_or_else(|| {
            StoreError::InvalidBrowserTransition("the role has no application URL".to_string())
        })?;
        let form_url = application_form_url(&url)?;
        let existing = self
            .connection
            .query_row(
                "SELECT id FROM browser_sessions
              WHERE preparation_id = ?1 AND status IN (
                    'waiting_for_extension', 'inspecting', 'drafting_answers', 'answering',
                    'filling', 'persisting_answers', 'saving_answers', 'releasing',
                    'submitted_tracking_pending', 'applied_recorded'
              )
              ORDER BY created_at DESC LIMIT 1",
                [preparation_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return self.browser_session(&id);
        }
        let transaction = self.connection.transaction()?;
        let now = Utc::now().to_rfc3339();
        let session_id = Uuid::new_v4().to_string();
        let driver_lease_id = Uuid::new_v4().to_string();
        transaction.execute(
            "INSERT INTO browser_sessions(
               id, purpose, role_id, preparation_id, provider, status, page_url,
               driver_owner, driver_lease_id, driver_lease_state, created_at, updated_at
             ) VALUES (?1, 'application', ?2, ?3, ?4, 'waiting_for_extension', ?5,
                       'extension', ?6, 'held', ?7, ?7)",
            params![
                session_id,
                preparation.0,
                preparation_id,
                preparation.1,
                form_url,
                driver_lease_id,
                now
            ],
        )?;
        insert_browser_command(
            &transaction,
            &session_id,
            "inspect_request",
            &serde_json::json!({ "expectedUrl": form_url }),
            &now,
        )?;
        transaction.commit()?;
        self.browser_session(&session_id)
    }

    pub fn has_review_required_application_session(
        &self,
        preparation_id: &str,
    ) -> Result<bool, StoreError> {
        let count = self.connection.query_row(
            "SELECT COUNT(*) FROM browser_sessions
              WHERE preparation_id = ?1 AND purpose = 'application' AND status = 'review_required'",
            [preparation_id],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count > 0)
    }

    pub fn has_active_application_session(&self, preparation_id: &str) -> Result<bool, StoreError> {
        let count = self.connection.query_row(
            "SELECT COUNT(*) FROM browser_sessions
              WHERE preparation_id = ?1 AND purpose = 'application'
                AND status IN (
                  'waiting_for_extension', 'inspecting', 'drafting_answers', 'answering',
                  'filling', 'persisting_answers', 'saving_answers', 'releasing'
                )",
            [preparation_id],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count > 0)
    }

    pub fn fail_browser_session_start(
        &mut self,
        session_id: &str,
        error_code: &str,
    ) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "UPDATE browser_sessions SET status = 'action_required', error_code = ?1,
                    updated_at = ?2 WHERE id = ?3 AND status = 'waiting_for_extension'",
            params![error_code, Utc::now().to_rfc3339(), session_id],
        )?;
        transaction.execute(
            "UPDATE browser_commands SET status = 'permanent', error_code = ?1,
                    lease_expires_at = NULL, updated_at = ?2
              WHERE session_id = ?3 AND status = 'pending'",
            params![error_code, Utc::now().to_rfc3339(), session_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn queue_browser_failure_notification(
        &mut self,
        session_id: &str,
        stage: &str,
        detail: &str,
    ) -> Result<(), StoreError> {
        let detail = sanitize_preparation_error(detail);
        let notification = self
            .connection
            .query_row(
                "SELECT b.role_id, b.preparation_id, r.company, r.title
                   FROM browser_sessions b JOIN roles r ON r.id = b.role_id
                  WHERE b.id = ?1 AND b.purpose = 'application'
                    AND b.status = 'action_required'",
                [session_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((role_id, preparation_id, company, title)) = notification else {
            return Ok(());
        };
        self.connection.execute(
            "INSERT OR IGNORE INTO notification_outbox(
               id, dedupe_key, title, body, status, attempts, next_attempt_at, created_at,
               event_kind, role_id, preparation_id, browser_session_id,
               action_kind, action_label
             ) VALUES (?1, ?2, 'Preparation failed', ?3, 'pending', 0, ?4, ?4,
                       'preparation_failed', ?5, ?6, ?7, 'view_details', 'View details')",
            params![
                Uuid::new_v4().to_string(),
                format!("browser-session:{session_id}:preparation-failed"),
                format!(
                    "{title} at {company}. {}: {detail}",
                    preparation_stage_label(stage)
                ),
                Utc::now().to_rfc3339(),
                role_id,
                preparation_id,
                session_id,
            ],
        )?;
        Ok(())
    }

    pub fn browser_sessions(&self) -> Result<Vec<BrowserSessionSummary>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, purpose, role_id, preparation_id, status, ats, page_title, page_url,
                    snapshot_fingerprint, field_count, safe_field_count,
                    needs_user_count, error_code, review_items_json, fill_results_json, updated_at
               FROM browser_sessions ORDER BY updated_at DESC LIMIT 20",
        )?;
        statement
            .query_map([], browser_session_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn browser_command_result_matches(
        &self,
        command_id: &str,
        session_id: &str,
        command_type: &str,
        driver_lease_id: Option<&str>,
    ) -> Result<bool, StoreError> {
        let matches = self.connection.query_row(
            "SELECT COUNT(*) FROM browser_commands c
               JOIN browser_sessions b ON b.id = c.session_id
              WHERE c.id = ?1 AND c.status = 'leased' AND c.session_id = ?2
                AND c.command_type = ?3
                AND ((b.driver_lease_id IS NULL AND ?4 IS NULL) OR b.driver_lease_id = ?4)",
            params![command_id, session_id, command_type, driver_lease_id],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(matches == 1)
    }

    pub fn claim_browser_command(
        &mut self,
        now: DateTime<Utc>,
        lease_duration: Duration,
    ) -> Result<Option<BrowserCommand>, StoreError> {
        let transaction = self.connection.transaction()?;
        let now_text = now.to_rfc3339();
        transaction.execute(
            "UPDATE browser_commands
                SET status = CASE WHEN attempt >= ?1 THEN 'permanent' ELSE 'pending' END,
                    error_code = 'lease_expired', lease_expires_at = NULL, updated_at = ?2
              WHERE status = 'leased' AND lease_expires_at < ?2",
            params![MAX_BROWSER_COMMAND_ATTEMPTS, now_text],
        )?;
        transaction.execute(
            "UPDATE browser_sessions SET status = 'action_required', error_code = 'extension_command_expired', updated_at = ?1
              WHERE id IN (
                SELECT session_id FROM browser_commands
                 WHERE status = 'permanent' AND error_code = 'lease_expired'
                   AND command_type != 'focus_review'
              )",
            [now_text.clone()],
        )?;
        transaction.execute(
            "UPDATE browser_sessions
                SET driver_lease_state = CASE
                      WHEN EXISTS (SELECT 1 FROM browser_commands c WHERE c.session_id = browser_sessions.id
                                   AND c.status = 'permanent' AND c.error_code = 'lease_expired'
                                   AND c.command_type != 'inspect_request') THEN 'human_handoff'
                      WHEN EXISTS (SELECT 1 FROM browser_commands c WHERE c.session_id = browser_sessions.id
                                   AND c.status = 'permanent' AND c.error_code = 'lease_expired'
                                   AND c.command_type = 'inspect_request') THEN 'released'
                      ELSE 'human_handoff' END,
                    fallback_eligible = CASE
                      WHEN NOT EXISTS (SELECT 1 FROM browser_commands c WHERE c.session_id = browser_sessions.id
                                       AND c.status = 'permanent' AND c.error_code = 'lease_expired'
                                       AND c.command_type != 'inspect_request')
                       AND EXISTS (SELECT 1 FROM browser_commands c WHERE c.session_id = browser_sessions.id
                                   AND c.status = 'permanent' AND c.error_code = 'lease_expired'
                                   AND c.command_type = 'inspect_request') THEN 1 ELSE 0 END,
                    handoff_reason = 'extension_command_expired'
              WHERE purpose = 'application' AND status = 'action_required'
                AND error_code = 'extension_command_expired'",
            [],
        )?;
        let candidate = transaction
            .query_row(
                "SELECT c.id, c.session_id, c.command_type, c.payload_json, s.driver_lease_id
                   FROM browser_commands c
                   JOIN browser_sessions s ON s.id = c.session_id
                   LEFT JOIN roles r ON r.id = s.role_id
                  WHERE c.status = 'pending' AND c.attempt < ?1
                    AND (s.purpose != 'application'
                      OR LOWER(TRIM(COALESCE(r.canonical_status, '')))
                        NOT IN ('applied', 'discarded', 'rejected'))
                    AND (
                      (c.command_type = 'focus_review' AND s.status = 'review_required'
                        AND s.driver_lease_state = 'released')
                      OR
                      (s.driver_lease_state IN ('held', 'none') AND s.id = (
                        SELECT active.id FROM browser_sessions active
                         WHERE active.purpose = 'application'
                           AND NOT EXISTS (
                             SELECT 1 FROM roles active_role
                              WHERE active_role.id = active.role_id
                                AND LOWER(TRIM(COALESCE(active_role.canonical_status, '')))
                                  IN ('applied', 'discarded', 'rejected')
                           )
                           AND active.status IN (
                             'waiting_for_extension', 'inspecting', 'drafting_answers',
                             'answering', 'filling', 'persisting_answers',
                             'saving_answers', 'releasing'
                           )
                         ORDER BY active.created_at, active.rowid LIMIT 1
                      ))
                      OR (
                        s.purpose != 'application'
                        AND NOT EXISTS (
                          SELECT 1 FROM browser_sessions active
                           WHERE active.purpose = 'application'
                             AND NOT EXISTS (
                               SELECT 1 FROM roles active_role
                                WHERE active_role.id = active.role_id
                                  AND LOWER(TRIM(COALESCE(active_role.canonical_status, '')))
                                    IN ('applied', 'discarded', 'rejected')
                             )
                             AND active.status IN (
                               'waiting_for_extension', 'inspecting', 'drafting_answers',
                               'answering', 'filling', 'persisting_answers',
                               'saving_answers', 'releasing'
                             )
                        )
                      )
                    )
                  ORDER BY c.created_at, c.rowid LIMIT 1",
                [MAX_BROWSER_COMMAND_ATTEMPTS],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((command_id, session_id, command_type, payload_json, driver_lease_id)) = candidate
        else {
            transaction.commit()?;
            return Ok(None);
        };
        let lease_expires_at = now + lease_duration;
        transaction.execute(
            "UPDATE browser_commands
                SET status = 'leased', attempt = attempt + 1, lease_expires_at = ?1,
                    error_code = NULL, updated_at = ?2
              WHERE id = ?3 AND status = 'pending'",
            params![lease_expires_at.to_rfc3339(), now_text, command_id],
        )?;
        let session_status = match command_type.as_str() {
            "release_for_review" => "releasing",
            "fill_plan" => "filling",
            "focus_review" => "review_required",
            _ => "inspecting",
        };
        transaction.execute(
            "UPDATE browser_sessions SET status = ?1, error_code = NULL, updated_at = ?2 WHERE id = ?3",
            params![session_status, now_text, session_id],
        )?;
        transaction.commit()?;
        let payload = serde_json::from_str(&payload_json).map_err(|error| {
            StoreError::InvalidBrowserTransition(format!(
                "stored browser payload is invalid: {error}"
            ))
        })?;
        Ok(Some(BrowserCommand {
            command_id,
            session_id,
            command_type,
            driver_lease_id,
            payload,
        }))
    }

    pub fn recover_stalled_browser_commands(
        &mut self,
        now: DateTime<Utc>,
        pending_timeout: Duration,
    ) -> Result<Vec<String>, StoreError> {
        let transaction = self.connection.transaction()?;
        let now_text = now.to_rfc3339();
        transaction.execute(
            "UPDATE browser_commands
                SET status = CASE WHEN attempt >= ?1 THEN 'permanent' ELSE 'pending' END,
                    error_code = 'lease_expired', lease_expires_at = NULL, updated_at = ?2
              WHERE status = 'leased' AND lease_expires_at < ?2",
            params![MAX_BROWSER_COMMAND_ATTEMPTS, now_text],
        )?;
        transaction.execute(
            "UPDATE browser_commands
                SET status = 'permanent', error_code = 'extension_handshake_timeout',
                    lease_expires_at = NULL, updated_at = ?1
              WHERE status = 'pending' AND command_type != 'focus_review' AND updated_at < ?2",
            params![(now).to_rfc3339(), (now - pending_timeout).to_rfc3339()],
        )?;
        transaction.execute(
            "UPDATE browser_sessions
                SET status = 'action_required',
                    error_code = CASE
                      WHEN EXISTS (
                        SELECT 1 FROM browser_commands c
                         WHERE c.session_id = browser_sessions.id
                           AND c.status = 'permanent'
                           AND c.error_code = 'extension_handshake_timeout'
                      ) THEN 'extension_handshake_timeout'
                      ELSE 'extension_command_expired'
                    END,
                    updated_at = ?1
              WHERE id IN (
                SELECT session_id FROM browser_commands
                 WHERE status = 'permanent'
                   AND error_code IN ('lease_expired', 'extension_handshake_timeout')
                   AND command_type != 'focus_review'
              )
                AND status != 'action_required'",
            [now_text.clone()],
        )?;
        transaction.execute(
            "UPDATE browser_sessions
                SET driver_lease_state = CASE
                      WHEN EXISTS (SELECT 1 FROM browser_commands c WHERE c.session_id = browser_sessions.id
                                   AND c.status = 'permanent' AND c.command_type != 'inspect_request')
                        THEN 'human_handoff'
                      WHEN EXISTS (SELECT 1 FROM browser_commands c WHERE c.session_id = browser_sessions.id
                                   AND c.status = 'permanent' AND c.command_type = 'inspect_request')
                        THEN 'released' ELSE 'human_handoff' END,
                    fallback_eligible = CASE
                      WHEN NOT EXISTS (SELECT 1 FROM browser_commands c WHERE c.session_id = browser_sessions.id
                                       AND c.status = 'permanent' AND c.command_type != 'inspect_request')
                       AND EXISTS (SELECT 1 FROM browser_commands c WHERE c.session_id = browser_sessions.id
                                   AND c.status = 'permanent' AND c.command_type = 'inspect_request')
                        THEN 1 ELSE 0 END,
                    handoff_reason = error_code
              WHERE purpose = 'application' AND status = 'action_required'
                AND error_code IN ('extension_command_expired', 'extension_handshake_timeout')",
            [],
        )?;
        let session_ids = {
            let mut statement = transaction.prepare(
                "SELECT DISTINCT b.id
                   FROM browser_sessions b
                   JOIN browser_commands c ON c.session_id = b.id
                  WHERE b.status = 'action_required' AND b.updated_at = ?1
                    AND c.status = 'permanent'
                    AND c.error_code IN ('lease_expired', 'extension_handshake_timeout')",
            )?;
            statement
                .query_map([now_text], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        transaction.commit()?;
        Ok(session_ids)
    }

    pub fn complete_browser_inspection(
        &mut self,
        command_id: &str,
        inspection: &BrowserInspection,
    ) -> Result<BrowserSessionSummary, StoreError> {
        let transaction = self.connection.transaction()?;
        let session_id = leased_browser_command(&transaction, command_id, "inspect_request")?;
        let now = Utc::now().to_rfc3339();
        transaction.execute(
            "UPDATE browser_commands
                SET status = 'completed', lease_expires_at = NULL, error_code = NULL, updated_at = ?1
              WHERE id = ?2",
            params![now, command_id],
        )?;
        let (purpose, expected_url) = transaction.query_row(
            "SELECT purpose, page_url FROM browser_sessions WHERE id = ?1",
            [&session_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )?;
        if purpose == "application"
            && expected_url
                .as_deref()
                .is_none_or(|expected| !application_pages_match(expected, &inspection.page_url))
        {
            return Err(StoreError::InvalidBrowserTransition(
                "inspected page does not match the prepared role".to_string(),
            ));
        }
        let no_compatible_fields = inspection.fields.as_array().is_none_or(|fields| {
            !fields.iter().any(|field| {
                field.get("control").and_then(serde_json::Value::as_str) != Some("unsupported")
            })
        });
        let flow_issues = inspection
            .flow_issues
            .as_array()
            .cloned()
            .unwrap_or_default();
        let active_human_blocker = flow_issues.iter().any(|issue| {
            matches!(
                issue.as_str(),
                Some("authentication_required" | "active_antibot_challenge")
            )
        });
        let structural_blocker = flow_issues.iter().any(|issue| {
            matches!(
                issue.as_str(),
                Some("embedded_frame" | "modal_form" | "multi_step_form")
            )
        });
        let fallback_flow = inspection.flow_disposition == "fallback_eligible";
        let blocked = purpose == "application"
            && (no_compatible_fields
                || fallback_flow
                || structural_blocker
                || active_human_blocker);
        let fallback_eligible = blocked && fallback_flow;
        let handoff_reason = if no_compatible_fields {
            Some("no_compatible_fields".to_string())
        } else {
            inspection
                .flow_issues
                .as_array()
                .and_then(|issues| issues.first())
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        let next_status = if blocked {
            "action_required"
        } else if purpose == "application" {
            "drafting_answers"
        } else {
            "releasing"
        };
        transaction.execute(
            "UPDATE browser_sessions
                SET status = ?1, ats = ?2, page_title = ?3, page_url = ?4,
                    snapshot_fingerprint = ?5, fields_json = ?6, field_count = ?7,
                    safe_field_count = ?8, needs_user_count = ?9, error_code = ?10,
                    driver_lease_state = CASE WHEN ?11 THEN 'released'
                                              WHEN ?12 THEN 'human_handoff'
                                              ELSE driver_lease_state END,
                    fallback_eligible = ?11, handoff_reason = ?13, updated_at = ?14
              WHERE id = ?15",
            params![
                next_status,
                inspection.ats,
                inspection.page_title,
                inspection.page_url,
                inspection.snapshot_fingerprint,
                inspection.fields.to_string(),
                inspection.fields.as_array().map_or(0, Vec::len) as i64,
                inspection.safe_field_count as i64,
                inspection.needs_user_count as i64,
                if blocked {
                    handoff_reason.as_deref()
                } else {
                    None
                },
                fallback_eligible,
                blocked && !fallback_eligible,
                handoff_reason,
                now,
                session_id
            ],
        )?;
        if purpose == "connection_check" {
            insert_browser_command(
                &transaction,
                &session_id,
                "release_for_review",
                &serde_json::json!({ "expectedUrl": inspection.page_url, "connectionCheck": true }),
                &now,
            )?;
        }
        transaction.commit()?;
        self.browser_session(&session_id)
    }

    pub fn claim_answer_work(&mut self) -> Result<Option<BrowserAnswerWork>, StoreError> {
        let transaction = self.connection.transaction()?;
        let candidate = transaction
            .query_row(
                "SELECT b.id, b.preparation_id, b.provider, p.report_path, p.cv_pdf_path,
                    p.cv_pdf_hash, b.ats, b.page_url, b.page_title, b.fields_json,
                    b.snapshot_fingerprint
               FROM browser_sessions b JOIN preparation_jobs p ON p.id = b.preparation_id
              WHERE b.status = 'drafting_answers'
              ORDER BY b.updated_at LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, String>(10)?,
                    ))
                },
            )
            .optional()?;
        let Some(candidate) = candidate else {
            transaction.commit()?;
            return Ok(None);
        };
        let now = Utc::now().to_rfc3339();
        let changed = transaction.execute(
            "UPDATE browser_sessions SET status = 'answering', error_code = NULL, updated_at = ?1
              WHERE id = ?2 AND status = 'drafting_answers'",
            params![now, candidate.0],
        )?;
        if changed != 1 {
            transaction.commit()?;
            return Ok(None);
        }
        transaction.commit()?;
        let fields: serde_json::Value = serde_json::from_str(&candidate.9).map_err(|error| {
            StoreError::InvalidBrowserTransition(format!("stored form fields are invalid: {error}"))
        })?;
        Ok(Some(BrowserAnswerWork {
            session_id: candidate.0,
            preparation_id: candidate.1,
            provider: candidate.2,
            report_path: candidate.3,
            cv_pdf_path: candidate.4,
            cv_pdf_hash: candidate.5,
            snapshot: serde_json::json!({
                "protocolVersion": 1,
                "ats": candidate.6,
                "url": candidate.7,
                "title": candidate.8,
                "fields": fields,
                "fingerprint": candidate.10,
            }),
            snapshot_fingerprint: candidate.10,
        }))
    }

    pub fn complete_answer_work(
        &mut self,
        session_id: &str,
        context_hash: &str,
        fill_plan: &serde_json::Value,
        review_items: &serde_json::Value,
        cv_upload: Option<&serde_json::Value>,
    ) -> Result<BrowserSessionSummary, StoreError> {
        let transaction = self.connection.transaction()?;
        let fingerprint = transaction.query_row(
            "SELECT snapshot_fingerprint FROM browser_sessions WHERE id = ?1 AND status = 'answering'",
            [session_id],
            |row| row.get::<_, String>(0),
        ).optional()?.ok_or_else(|| StoreError::InvalidBrowserTransition(
            "answer session is missing or no longer active".to_string(),
        ))?;
        if fill_plan
            .get("snapshotFingerprint")
            .and_then(serde_json::Value::as_str)
            != Some(fingerprint.as_str())
        {
            return Err(StoreError::InvalidBrowserTransition(
                "fill plan does not match the inspected form".to_string(),
            ));
        }
        let instructions = fill_plan
            .get("instructions")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                StoreError::InvalidBrowserTransition(
                    "fill plan is missing instructions".to_string(),
                )
            })?;
        let now = Utc::now().to_rfc3339();
        let has_browser_work = !instructions.is_empty() || cv_upload.is_some();
        let next_status = if has_browser_work {
            "filling"
        } else {
            "action_required"
        };
        let empty_fill_results = serde_json::json!([]);
        transaction.execute(
            "UPDATE browser_sessions SET status = ?1, answers_context_hash = ?2,
                    review_items_json = ?3, fill_results_json = ?4,
                    error_code = ?5,
                    driver_lease_state = CASE WHEN ?6 THEN driver_lease_state ELSE 'human_handoff' END,
                    fallback_eligible = 0,
                    handoff_reason = CASE WHEN ?6 THEN handoff_reason ELSE 'manual_completion_required' END,
                    updated_at = ?7 WHERE id = ?8",
            params![
                next_status,
                context_hash,
                review_items.to_string(),
                if has_browser_work {
                    None
                } else {
                    Some(empty_fill_results.to_string())
                },
                if has_browser_work { None } else { Some("manual_completion_required") },
                has_browser_work,
                now,
                session_id
            ],
        )?;
        if has_browser_work {
            let mut payload = serde_json::json!({ "plan": fill_plan });
            if let Some(upload) = cv_upload {
                payload["cvUpload"] = upload.clone();
            }
            insert_browser_command(&transaction, session_id, "fill_plan", &payload, &now)?;
        }
        transaction.commit()?;
        self.browser_session(session_id)
    }

    pub fn fail_answer_work(
        &mut self,
        session_id: &str,
        error_code: &str,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE browser_sessions SET status = 'action_required', error_code = ?1, updated_at = ?2
              WHERE id = ?3 AND status = 'answering'",
            params![error_code, Utc::now().to_rfc3339(), session_id],
        )?;
        Ok(())
    }

    pub fn complete_browser_fill(
        &mut self,
        command_id: &str,
        results: &serde_json::Value,
    ) -> Result<BrowserSessionSummary, StoreError> {
        let transaction = self.connection.transaction()?;
        let session_id = leased_browser_command(&transaction, command_id, "fill_plan")?;
        let (payload_json, fields_json) = transaction.query_row(
            "SELECT c.payload_json, b.fields_json
               FROM browser_commands c JOIN browser_sessions b ON b.id = c.session_id
              WHERE c.id = ?1",
            [command_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?;
        let payload: serde_json::Value = serde_json::from_str(&payload_json).map_err(|error| {
            StoreError::InvalidBrowserTransition(format!(
                "stored browser payload is invalid: {error}"
            ))
        })?;
        let results = normalized_browser_fill_results(results, &payload)?;
        let fields: serde_json::Value = serde_json::from_str(&fields_json).map_err(|error| {
            StoreError::InvalidBrowserTransition(format!("stored form fields are invalid: {error}"))
        })?;
        let required_ids = fields
            .as_array()
            .into_iter()
            .flatten()
            .filter(|field| {
                field.get("required").and_then(serde_json::Value::as_bool) == Some(true)
            })
            .filter_map(|field| field.get("id").and_then(serde_json::Value::as_str))
            .collect::<std::collections::BTreeSet<_>>();
        let upload_id = payload
            .pointer("/cvUpload/fieldId")
            .and_then(serde_json::Value::as_str);
        let required_failed = results.as_array().is_some_and(|items| {
            items.iter().any(|item| {
                let field_id = item.get("fieldId").and_then(serde_json::Value::as_str);
                let required =
                    field_id.is_some_and(|id| required_ids.contains(id) || upload_id == Some(id));
                required
                    && item.get("status").and_then(serde_json::Value::as_str) != Some("verified")
                    && item.get("reasonCode").and_then(serde_json::Value::as_str)
                        != Some("user_file_preserved")
            })
        });
        let now = Utc::now().to_rfc3339();
        transaction.execute(
            "UPDATE browser_commands SET status = 'completed', lease_expires_at = NULL,
                    error_code = NULL, updated_at = ?1 WHERE id = ?2",
            params![now, command_id],
        )?;
        transaction.execute(
            "UPDATE browser_sessions SET status = ?1, fill_results_json = ?2,
                    error_code = ?3,
                    driver_lease_state = CASE WHEN ?4 THEN 'human_handoff' ELSE driver_lease_state END,
                    fallback_eligible = 0,
                    handoff_reason = CASE WHEN ?4 THEN 'required_fill_readback_failed' ELSE handoff_reason END,
                    updated_at = ?5 WHERE id = ?6",
            params![
                if required_failed { "action_required" } else { "persisting_answers" },
                results.to_string(),
                if required_failed { Some("required_fill_readback_failed") } else { None },
                required_failed,
                now,
                session_id
            ],
        )?;
        transaction.commit()?;
        self.browser_session(&session_id)
    }

    pub fn claim_answer_commit_work(
        &mut self,
    ) -> Result<Option<BrowserAnswerCommitWork>, StoreError> {
        let transaction = self.connection.transaction()?;
        let candidate = transaction
            .query_row(
                "SELECT b.id, b.preparation_id, p.report_path, p.cv_pdf_path,
                        b.answers_context_hash, b.review_items_json, b.fill_results_json
                   FROM browser_sessions b JOIN preparation_jobs p ON p.id = b.preparation_id
                  WHERE b.status = 'persisting_answers'
                  ORDER BY b.updated_at LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .optional()?;
        let Some(candidate) = candidate else {
            transaction.commit()?;
            return Ok(None);
        };
        let now = Utc::now().to_rfc3339();
        let changed = transaction.execute(
            "UPDATE browser_sessions SET status = 'saving_answers', error_code = NULL,
                    updated_at = ?1 WHERE id = ?2 AND status = 'persisting_answers'",
            params![now, candidate.0],
        )?;
        if changed != 1 {
            transaction.commit()?;
            return Ok(None);
        }
        transaction.commit()?;
        let review_items = serde_json::from_str(&candidate.5).map_err(|error| {
            StoreError::InvalidBrowserTransition(format!(
                "stored review items are invalid: {error}"
            ))
        })?;
        let fill_results = serde_json::from_str(&candidate.6).map_err(|error| {
            StoreError::InvalidBrowserTransition(format!(
                "stored fill results are invalid: {error}"
            ))
        })?;
        Ok(Some(BrowserAnswerCommitWork {
            session_id: candidate.0,
            preparation_id: candidate.1,
            report_path: candidate.2,
            cv_pdf_path: candidate.3,
            context_hash: candidate.4,
            review_items,
            fill_results,
        }))
    }

    pub fn complete_answer_commit(
        &mut self,
        session_id: &str,
        context_hash: &str,
        report_hash: &str,
    ) -> Result<BrowserSessionSummary, StoreError> {
        let transaction = self.connection.transaction()?;
        let (stored_hash, expected_url) = transaction
            .query_row(
                "SELECT answers_context_hash, page_url FROM browser_sessions
                  WHERE id = ?1 AND status = 'saving_answers'",
                [session_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::InvalidBrowserTransition(
                    "answer persistence session is missing or no longer active".to_string(),
                )
            })?;
        if stored_hash != context_hash || report_hash.len() != 64 {
            return Err(StoreError::InvalidBrowserTransition(
                "persisted answers do not match this form session".to_string(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        transaction.execute(
            "UPDATE browser_sessions SET status = 'releasing', answers_report_hash = ?1,
                    error_code = NULL, updated_at = ?2 WHERE id = ?3",
            params![report_hash, now, session_id],
        )?;
        insert_browser_command(
            &transaction,
            session_id,
            "release_for_review",
            &serde_json::json!({ "expectedUrl": expected_url }),
            &now,
        )?;
        transaction.commit()?;
        self.browser_session(session_id)
    }

    pub fn fail_answer_commit(
        &mut self,
        session_id: &str,
        error_code: &str,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE browser_sessions SET status = 'action_required', error_code = ?1,
                    updated_at = ?2 WHERE id = ?3 AND status = 'saving_answers'",
            params![error_code, Utc::now().to_rfc3339(), session_id],
        )?;
        Ok(())
    }

    pub fn complete_browser_release(
        &mut self,
        command_id: &str,
    ) -> Result<BrowserSessionSummary, StoreError> {
        let transaction = self.connection.transaction()?;
        let session_id = leased_browser_command(&transaction, command_id, "release_for_review")?;
        let (purpose, role_id, preparation_id, company, title) = transaction.query_row(
            "SELECT b.purpose, b.role_id, b.preparation_id, r.company, r.title
               FROM browser_sessions b LEFT JOIN roles r ON r.id = b.role_id
              WHERE b.id = ?1",
            [&session_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )?;
        let next_status = if purpose == "connection_check" {
            "connection_verified"
        } else {
            "review_required"
        };
        let now = Utc::now().to_rfc3339();
        transaction.execute(
            "UPDATE browser_commands
                SET status = 'completed', lease_expires_at = NULL, error_code = NULL, updated_at = ?1
              WHERE id = ?2",
            params![now, command_id],
        )?;
        transaction.execute(
            "UPDATE browser_sessions SET status = ?1, error_code = NULL,
                    driver_lease_state = CASE WHEN purpose = 'application' THEN 'released' ELSE driver_lease_state END,
                    fallback_eligible = 0, handoff_reason = NULL, updated_at = ?2 WHERE id = ?3",
            params![next_status, now, session_id],
        )?;
        if purpose == "connection_check" {
            transaction.execute(
                "INSERT INTO activity(id, kind, message, occurred_at)
                 VALUES (?1, 'browser', 'The extension inspected an HTTPS application page and released it without filling or finalizing anything.', ?2)",
                params![Uuid::new_v4().to_string(), now],
            )?;
        } else if let (Some(role_id), Some(preparation_id), Some(company), Some(title)) =
            (role_id, preparation_id, company, title)
        {
            transaction.execute(
                "INSERT OR IGNORE INTO notification_outbox(
                   id, dedupe_key, title, body, status, attempts, next_attempt_at, created_at,
                   event_kind, role_id, preparation_id, browser_session_id,
                   action_kind, action_label
                 ) VALUES (?1, ?2, 'Application ready for review', ?3, 'pending', 0, ?4, ?4,
                           'application_ready', ?5, ?6, ?7, 'review_form', 'Review form')",
                params![
                    Uuid::new_v4().to_string(),
                    format!("browser-session:{session_id}:review-required"),
                    format!(
                        "{title} at {company}. The live form is released in Chrome. Only you can submit it."
                    ),
                    now,
                    role_id,
                    preparation_id,
                    session_id,
                ],
            )?;
        }
        transaction.commit()?;
        self.browser_session(&session_id)
    }

    pub fn queue_focus_review(
        &mut self,
        session_id: &str,
    ) -> Result<BrowserSessionSummary, StoreError> {
        let expected_url = self
            .connection
            .query_row(
                "SELECT page_url FROM browser_sessions
                  WHERE id = ?1 AND purpose = 'application' AND status = 'review_required'",
                [session_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::InvalidBrowserTransition(
                    "the application form is not released for review".to_string(),
                )
            })?;
        let transaction = self.connection.transaction()?;
        let now = Utc::now().to_rfc3339();
        insert_browser_command(
            &transaction,
            session_id,
            "focus_review",
            &serde_json::json!({ "expectedUrl": expected_url }),
            &now,
        )?;
        transaction.commit()?;
        self.browser_session(session_id)
    }

    pub fn complete_focus_review(
        &mut self,
        command_id: &str,
    ) -> Result<BrowserSessionSummary, StoreError> {
        let transaction = self.connection.transaction()?;
        let session_id = leased_browser_command(&transaction, command_id, "focus_review")?;
        let now = Utc::now().to_rfc3339();
        transaction.execute(
            "UPDATE browser_commands
                SET status = 'completed', lease_expires_at = NULL, error_code = NULL, updated_at = ?1
              WHERE id = ?2",
            params![now, command_id],
        )?;
        transaction.commit()?;
        self.browser_session(&session_id)
    }

    pub fn fail_focus_review(
        &mut self,
        command_id: &str,
        error_code: &str,
    ) -> Result<BrowserSessionSummary, StoreError> {
        let transaction = self.connection.transaction()?;
        let session_id = leased_browser_command(&transaction, command_id, "focus_review")?;
        transaction.execute(
            "UPDATE browser_commands
                SET status = 'failed', lease_expires_at = NULL, error_code = ?1, updated_at = ?2
              WHERE id = ?3",
            params![error_code, Utc::now().to_rfc3339(), command_id],
        )?;
        transaction.commit()?;
        self.browser_session(&session_id)
    }

    pub fn fail_browser_command(
        &mut self,
        command_id: &str,
        error_code: &str,
    ) -> Result<BrowserSessionSummary, StoreError> {
        let transaction = self.connection.transaction()?;
        let (session_id, attempt, command_type) = transaction.query_row(
            "SELECT session_id, attempt, command_type FROM browser_commands WHERE id = ?1 AND status = 'leased'",
            [command_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )?;
        let command_status = if attempt >= MAX_BROWSER_COMMAND_ATTEMPTS {
            "permanent"
        } else {
            "failed"
        };
        let now = Utc::now().to_rfc3339();
        let fallback_eligible = browser_failure_is_fallback_eligible(&command_type, error_code);
        transaction.execute(
            "UPDATE browser_commands SET status = ?1, lease_expires_at = NULL,
                    error_code = ?2, updated_at = ?3 WHERE id = ?4",
            params![command_status, error_code, now, command_id],
        )?;
        transaction.execute(
            "UPDATE browser_sessions SET status = 'action_required', error_code = ?1,
                    driver_lease_state = CASE
                      WHEN purpose != 'application' THEN driver_lease_state
                      WHEN ?2 THEN 'released' ELSE 'human_handoff' END,
                    fallback_eligible = CASE WHEN purpose = 'application' AND ?2 THEN 1 ELSE 0 END,
                    handoff_reason = ?1, updated_at = ?3 WHERE id = ?4",
            params![error_code, fallback_eligible, now, session_id],
        )?;
        let application = transaction
            .query_row(
                "SELECT b.role_id, b.preparation_id, r.company, r.title
                   FROM browser_sessions b JOIN roles r ON r.id = b.role_id
                  WHERE b.id = ?1 AND b.purpose = 'application'",
                [&session_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        if let Some((role_id, preparation_id, company, title)) = application {
            transaction.execute(
                "INSERT OR IGNORE INTO notification_outbox(
                   id, dedupe_key, title, body, status, attempts, next_attempt_at, created_at,
                   event_kind, role_id, preparation_id, browser_session_id,
                   action_kind, action_label
                 ) VALUES (?1, ?2, 'Preparation failed', ?3, 'pending', 0, ?4, ?4,
                           'preparation_failed', ?5, ?6, ?7, 'view_details', 'View details')",
                params![
                    Uuid::new_v4().to_string(),
                    format!("browser-session:{session_id}:preparation-failed"),
                    format!(
                        "{title} at {company}. {}: {}",
                        preparation_stage_label(&command_type),
                        sanitize_preparation_error(error_code)
                    ),
                    now,
                    role_id,
                    preparation_id,
                    session_id,
                ],
            )?;
        }
        transaction.commit()?;
        self.browser_session(&session_id)
    }

    pub fn retry_browser_session(
        &mut self,
        session_id: &str,
    ) -> Result<BrowserSessionSummary, StoreError> {
        let transaction = self.connection.transaction()?;
        let (status, error_code, driver_lease_state) = transaction
            .query_row(
                "SELECT status, error_code, driver_lease_state FROM browser_sessions WHERE id = ?1",
                [session_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::InvalidBrowserTransition("browser session not found".to_string())
            })?;
        if status != "action_required" {
            return Err(StoreError::InvalidBrowserTransition(
                "only an action-required browser session can be retried".to_string(),
            ));
        }
        if driver_lease_state == "human_handoff" {
            return Err(StoreError::InvalidBrowserTransition(
                "this browser session may have changed the live form and requires human review before another driver can run".to_string(),
            ));
        }
        if matches!(
            error_code.as_deref(),
            Some("answer_persistence_failed" | "answer_persistence_interrupted")
        ) {
            let repair = transaction
                .query_row(
                    "SELECT b.fill_results_json, c.payload_json
                       FROM browser_sessions b
                       JOIN browser_commands c ON c.session_id = b.id
                      WHERE b.id = ?1 AND c.command_type = 'fill_plan'
                      ORDER BY c.created_at DESC, c.rowid DESC LIMIT 1",
                    [session_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            if let Some((results_json, payload_json)) = repair {
                let results: serde_json::Value =
                    serde_json::from_str(&results_json).map_err(|error| {
                        StoreError::InvalidBrowserTransition(format!(
                            "stored fill results are invalid: {error}"
                        ))
                    })?;
                let payload: serde_json::Value =
                    serde_json::from_str(&payload_json).map_err(|error| {
                        StoreError::InvalidBrowserTransition(format!(
                            "stored browser payload is invalid: {error}"
                        ))
                    })?;
                let repaired = normalized_browser_fill_results(&results, &payload)?;
                transaction.execute(
                    "UPDATE browser_sessions SET fill_results_json = ?1 WHERE id = ?2",
                    params![repaired.to_string(), session_id],
                )?;
            }
            transaction.execute(
                "UPDATE browser_sessions SET status = 'persisting_answers', error_code = NULL,
                        updated_at = ?1 WHERE id = ?2",
                params![Utc::now().to_rfc3339(), session_id],
            )?;
            transaction.commit()?;
            return self.browser_session(session_id);
        }
        let failed_command = transaction
            .query_row(
                "SELECT command_type, payload_json FROM browser_commands
              WHERE session_id = ?1 AND status IN ('failed', 'permanent')
              ORDER BY updated_at DESC LIMIT 1",
                [session_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        if failed_command.is_none() {
            transaction.execute(
                "UPDATE browser_sessions SET status = 'drafting_answers', error_code = NULL,
                        updated_at = ?1 WHERE id = ?2",
                params![Utc::now().to_rfc3339(), session_id],
            )?;
            transaction.commit()?;
            return self.browser_session(session_id);
        }
        let (command_type, payload_json) = failed_command.expect("checked above");
        let payload: serde_json::Value = serde_json::from_str(&payload_json).map_err(|error| {
            StoreError::InvalidBrowserTransition(format!(
                "stored browser payload is invalid: {error}"
            ))
        })?;
        let now = Utc::now().to_rfc3339();
        insert_browser_command(&transaction, session_id, &command_type, &payload, &now)?;
        let next_status = match command_type.as_str() {
            "release_for_review" => "releasing",
            "fill_plan" => "filling",
            _ => "waiting_for_extension",
        };
        transaction.execute(
            "UPDATE browser_sessions SET status = ?1, error_code = NULL,
                    driver_owner = CASE WHEN purpose = 'application' THEN 'extension' ELSE driver_owner END,
                    driver_lease_id = CASE WHEN purpose = 'application' THEN ?2 ELSE driver_lease_id END,
                    driver_lease_state = CASE WHEN purpose = 'application' THEN 'held' ELSE driver_lease_state END,
                    fallback_eligible = 0, handoff_reason = NULL, updated_at = ?3 WHERE id = ?4",
            params![next_status, Uuid::new_v4().to_string(), now, session_id],
        )?;
        transaction.commit()?;
        self.browser_session(session_id)
    }

    fn browser_session(&self, session_id: &str) -> Result<BrowserSessionSummary, StoreError> {
        self.connection
            .query_row(
                "SELECT id, purpose, role_id, preparation_id, status, ats, page_title, page_url,
                        snapshot_fingerprint, field_count, safe_field_count,
                        needs_user_count, error_code, review_items_json, fill_results_json, updated_at
                   FROM browser_sessions WHERE id = ?1",
                [session_id],
                browser_session_from_row,
            )
            .map_err(StoreError::from)
    }

    pub fn integrity_check(&self) -> Result<String, StoreError> {
        self.connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(StoreError::from)
    }

    pub fn backup_to(&self, path: &Path) -> Result<(), StoreError> {
        self.connection.backup(rusqlite::MAIN_DB, path, None)?;
        Ok(())
    }

    pub fn redacted_diagnostics(&self) -> Result<serde_json::Value, StoreError> {
        let role_count = self
            .connection
            .query_row("SELECT COUNT(*) FROM roles", [], |row| row.get::<_, i64>(0))?;
        let occurrence_count =
            self.connection
                .query_row("SELECT COUNT(*) FROM source_occurrences", [], |row| {
                    row.get::<_, i64>(0)
                })?;
        let handled_count = self.connection.query_row(
            "SELECT COUNT(*) FROM roles WHERE canonical_tracker_id IS NOT NULL",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        let mut status_statement = self
            .connection
            .prepare("SELECT status, COUNT(*) FROM runs GROUP BY status ORDER BY status")?;
        let run_statuses = status_statement
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "status": row.get::<_, String>(0)?,
                    "count": row.get::<_, i64>(1)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut source_statement = self.connection.prepare(
            "SELECT source_id, timezone, schedule_hours, execution_mode,
                    last_successful_at
               FROM source_schedules WHERE enabled = 1 ORDER BY source_id",
        )?;
        let sources = source_statement
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "sourceId": row.get::<_, String>(0)?,
                    "timezone": row.get::<_, String>(1)?,
                    "scheduleHours": row.get::<_, String>(2)?,
                    "executionMode": row.get::<_, String>(3)?,
                    "lastSuccessfulAt": row.get::<_, Option<String>>(4)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let browser_session_count =
            self.connection
                .query_row("SELECT COUNT(*) FROM browser_sessions", [], |row| {
                    row.get::<_, i64>(0)
                })?;
        let mut browser_status_statement = self.connection.prepare(
            "SELECT status, COUNT(*) FROM browser_sessions GROUP BY status ORDER BY status",
        )?;
        let browser_statuses = browser_status_statement
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "status": row.get::<_, String>(0)?,
                    "count": row.get::<_, i64>(1)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut preparation_status_statement = self.connection.prepare(
            "SELECT status, COUNT(*) FROM preparation_jobs GROUP BY status ORDER BY status",
        )?;
        let preparation_statuses = preparation_status_statement
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "status": row.get::<_, String>(0)?,
                    "count": row.get::<_, i64>(1)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let discovery_runs = self.discovery_run_diagnostics()?;
        let discovery_cursors = self.discovery_cursors()?;
        Ok(serde_json::json!({
            "operationalSchemaVersion": SCHEMA_VERSION,
            "counts": {
                "roles": role_count,
                "sourceOccurrences": occurrence_count,
                "handledRoles": handled_count,
            },
            "runStatuses": run_statuses,
            "browser": {
                "sessionCount": browser_session_count,
                "statuses": browser_statuses,
            },
            "preparationStatuses": preparation_statuses,
            "sources": sources,
            "discoveryRuns": discovery_runs,
            "discoveryCursors": discovery_cursors,
            "redaction": {
                "roleDetails": "omitted",
                "applicationUrls": "omitted",
                "activityMessages": "omitted",
                "providerOutput": "omitted",
                "artifactPathsAndHashes": "omitted",
                "browserProfile": "omitted",
                "browserPageUrls": "omitted",
                "browserPageTitles": "omitted",
                "formFields": "omitted"
            }
        }))
    }

    pub fn restore_preflight(path: &Path) -> Result<RestorePreflight, StoreError> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let integrity =
            connection.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))?;
        let schema_version =
            connection.query_row("SELECT version FROM schema_meta LIMIT 1", [], |row| {
                row.get::<_, i64>(0)
            })?;
        let role_count =
            connection.query_row("SELECT COUNT(*) FROM roles", [], |row| row.get::<_, i64>(0))?;
        let run_count =
            connection.query_row("SELECT COUNT(*) FROM runs", [], |row| row.get::<_, i64>(0))?;
        Ok(RestorePreflight {
            path: path.display().to_string(),
            integrity,
            schema_version,
            role_count,
            run_count,
        })
    }

    fn retire_active_work_for_applied_role(
        transaction: &Transaction<'_>,
        role_id: &str,
        now: &str,
    ) -> Result<(), StoreError> {
        let is_applied = transaction.query_row(
            "SELECT LOWER(TRIM(COALESCE(canonical_status, ''))) = 'applied'
               FROM roles WHERE id = ?1",
            [role_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !is_applied {
            return Ok(());
        }

        transaction.execute(
            "UPDATE preparation_jobs
                SET status = 'cancelled', step = 'cancelled',
                    error_class = NULL, error_stage = NULL, error_detail = NULL,
                    retry_policy = NULL, updated_at = ?1
              WHERE role_id = ?2 AND status IN ('queued', 'preparing')",
            params![now, role_id],
        )?;
        transaction.execute(
            "UPDATE browser_commands
                SET status = 'permanent', error_code = 'canonical_terminal',
                    lease_expires_at = NULL, updated_at = ?1
              WHERE status IN ('pending', 'leased')
                AND session_id IN (
                  SELECT id FROM browser_sessions
                   WHERE purpose = 'application' AND role_id = ?2
                )",
            params![now, role_id],
        )?;
        transaction.execute(
            "UPDATE browser_sessions
                SET status = 'applied_recorded', error_code = NULL,
                    driver_lease_state = 'released', fallback_eligible = 0,
                    handoff_reason = 'canonical_terminal', updated_at = ?1
              WHERE purpose = 'application' AND role_id = ?2
                AND status != 'applied_recorded'",
            params![now, role_id],
        )?;
        transaction.execute(
            "UPDATE roles
                SET preparation_state = CASE
                      WHEN EXISTS (
                        SELECT 1 FROM preparation_jobs
                         WHERE role_id = ?2 AND status = 'completed'
                      ) THEN 'prepared' ELSE 'not_started' END,
                    updated_at = ?1
              WHERE id = ?2
                AND preparation_state != CASE
                      WHEN EXISTS (
                        SELECT 1 FROM preparation_jobs
                         WHERE role_id = ?2 AND status = 'completed'
                      ) THEN 'prepared' ELSE 'not_started' END",
            params![now, role_id],
        )?;
        Ok(())
    }

    pub fn reconcile_history(
        &mut self,
        records: &[HistoryRecord],
    ) -> Result<ReconcileResult, StoreError> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "UPDATE roles SET canonical_status = CASE
                WHEN LOWER(TRIM(COALESCE(canonical_status, ''))) = 'applied' THEN 'Applied'
                WHEN LOWER(TRIM(COALESCE(canonical_status, ''))) = 'discarded' THEN 'Discarded'
                WHEN LOWER(TRIM(COALESCE(canonical_status, ''))) = 'rejected' THEN 'Rejected'
                ELSE canonical_status END
              WHERE LOWER(TRIM(COALESCE(canonical_status, '')))
                IN ('applied', 'discarded', 'rejected')",
            [],
        )?;
        let cleared = transaction.execute(
            "UPDATE roles SET canonical_tracker_id = NULL, canonical_status = NULL, canonical_date = NULL
             WHERE canonical_tracker_id IS NOT NULL
               AND COALESCE(canonical_status, '') != 'Applied'",
            [],
        )?;
        let mut role_statement = transaction
            .prepare("SELECT id, company, title, application_url, canonical_status FROM roles")?;
        let roles = role_statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(role_statement);

        let mut matched = 0;
        for (role_id, company, title, application_url, current_status) in roles {
            let record =
                deterministic_history_match(records, &company, &title, application_url.as_deref());
            // Applied is a monotonic terminal outcome. History reads are intentionally
            // bounded, so no matching record may replace its terminal identity or date.
            if current_status.as_deref().is_some_and(is_applied_status) {
                Self::retire_active_work_for_applied_role(
                    &transaction,
                    &role_id,
                    &Utc::now().to_rfc3339(),
                )?;
                if record.is_some() {
                    matched += 1;
                }
                continue;
            }
            if let Some(record) = record {
                let record_status = if is_applied_status(&record.status) {
                    "Applied"
                } else if is_discarded_status(&record.status) {
                    "Discarded"
                } else if is_rejected_status(&record.status) {
                    "Rejected"
                } else {
                    record.status.as_str()
                };
                transaction.execute(
                    "UPDATE roles SET canonical_tracker_id = ?1, canonical_status = ?2, canonical_date = ?3,
                       canonical_visibility_override = CASE
                         WHEN canonical_visibility_override = 1 AND ?2 = 'Evaluated' THEN 1 ELSE 0 END,
                       updated_at = ?4 WHERE id = ?5",
                    params![
                        record.id,
                        record_status,
                        record.date,
                        Utc::now().to_rfc3339(),
                        role_id
                    ],
                )?;
                transaction.execute(
                    "UPDATE evaluation_sync
                        SET state = CASE
                              WHEN ?1 IN ('Applied', 'Discarded', 'Rejected') THEN 'terminal'
                              WHEN state = 'terminal' THEN 'awaiting_evaluation'
                              ELSE state END,
                            reason = CASE
                              WHEN ?1 IN ('Applied', 'Discarded', 'Rejected') THEN 'canonical_terminal'
                              WHEN state = 'terminal' THEN 'canonical_evaluation_requires_refresh'
                              ELSE reason END,
                            current_receipt_key = CASE
                              WHEN ?1 IN ('Applied', 'Discarded', 'Rejected') OR state = 'terminal'
                              THEN NULL ELSE current_receipt_key END,
                            input_hash = CASE WHEN state = 'terminal' THEN NULL ELSE input_hash END,
                            lease_expires_at = CASE
                              WHEN ?1 IN ('Applied', 'Discarded', 'Rejected') OR state = 'terminal'
                              THEN NULL ELSE lease_expires_at END,
                            updated_at = ?2
                      WHERE role_id = ?3",
                    params![record_status, Utc::now().to_rfc3339(), role_id],
                )?;
                if record_status == "Applied" {
                    Self::retire_active_work_for_applied_role(
                        &transaction,
                        &role_id,
                        &Utc::now().to_rfc3339(),
                    )?;
                }
                matched += 1;
            }
        }
        transaction.execute(
            "UPDATE roles SET canonical_visibility_override = 0
              WHERE canonical_tracker_id IS NULL
                AND COALESCE(canonical_status, '') != 'Applied'",
            [],
        )?;
        transaction.execute(
            "UPDATE evaluation_sync
                SET state = 'awaiting_evaluation', reason = 'canonical_evaluation_missing',
                    current_receipt_key = NULL, input_hash = NULL,
                    lease_expires_at = NULL, updated_at = ?1
              WHERE role_id IN (
                SELECT id FROM roles
                 WHERE canonical_tracker_id IS NULL
                   AND COALESCE(canonical_status, '') != 'Applied'
              )",
            [Utc::now().to_rfc3339()],
        )?;
        transaction.execute(
            "UPDATE evaluation_sync
                SET state = 'terminal', reason = 'canonical_terminal',
                    current_receipt_key = NULL, input_hash = NULL,
                    lease_expires_at = NULL, updated_at = ?1
              WHERE role_id IN (
                SELECT id FROM roles
                 WHERE canonical_status IN ('Applied', 'Discarded', 'Rejected')
              ) AND state != 'terminal'",
            [Utc::now().to_rfc3339()],
        )?;
        transaction.execute(
            "INSERT INTO activity(id, kind, message, occurred_at) VALUES (?1, 'history', ?2, ?3)",
            params![
                Uuid::new_v4().to_string(),
                format!(
                    "Canonical history reconciled: {matched} queue roles matched against {} career-ops records.",
                    records.len()
                ),
                Utc::now().to_rfc3339(),
            ],
        )?;
        transaction.execute(
            "INSERT INTO settings(key, value, updated_at) VALUES ('last_history_reconcile_at', ?1, ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            [Utc::now().to_rfc3339()],
        )?;
        transaction.commit()?;
        Ok(ReconcileResult {
            matched,
            cleared,
            unmatched: records.len().saturating_sub(matched),
        })
    }

    pub fn begin_discard_effect(
        &mut self,
        role_id: &str,
    ) -> Result<AdapterEffectContext, StoreError> {
        if self.canonical_role_is_applied(role_id)? {
            return Err(StoreError::InvalidAdapterEffect(
                "an Applied role cannot be discarded".to_string(),
            ));
        }
        self.begin_adapter_effect(role_id, "role.discard", None, None)
    }

    fn canonical_role_is_applied(&self, role_id: &str) -> Result<bool, StoreError> {
        self.connection
            .query_row(
                "SELECT LOWER(TRIM(COALESCE(canonical_status, ''))) = 'applied'
                   FROM roles WHERE id = ?1",
                [role_id],
                |row| row.get::<_, bool>(0),
            )
            .optional()?
            .ok_or_else(|| StoreError::InvalidAdapterEffect("role was not found".to_string()))
    }

    fn canonical_role_is_terminal(&self, role_id: &str) -> Result<bool, StoreError> {
        self.connection
            .query_row(
                "SELECT LOWER(TRIM(COALESCE(canonical_status, '')))
                        IN ('applied', 'discarded', 'rejected')
                   FROM roles WHERE id = ?1",
                [role_id],
                |row| row.get::<_, bool>(0),
            )
            .optional()?
            .ok_or_else(|| StoreError::InvalidPreparation("role was not found".to_string()))
    }

    pub fn begin_applied_effect_for_session(
        &mut self,
        session_id: &str,
    ) -> Result<(String, AdapterEffectContext), StoreError> {
        let (role_id, tracker_id, session_status, canonical_status) = self
            .connection
            .query_row(
                "SELECT b.role_id, r.canonical_tracker_id, b.status, r.canonical_status
               FROM browser_sessions b JOIN roles r ON r.id = b.role_id
              WHERE b.id = ?1 AND b.purpose = 'application'
                AND b.status IN ('review_required', 'submitted_tracking_pending', 'applied_recorded')",
                [session_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::InvalidAdapterEffect(
                    "application is not waiting for outcome confirmation".to_string(),
                )
            })?;
        let tracker_id = tracker_id.ok_or_else(|| {
            StoreError::InvalidAdapterEffect("canonical tracker row is missing".to_string())
        })?;
        let role_is_applied = canonical_status.as_deref().is_some_and(is_applied_status);
        if session_status == "applied_recorded" {
            if !role_is_applied {
                return Err(StoreError::InvalidAdapterEffect(
                    "recorded application is missing its canonical Applied outcome".to_string(),
                ));
            }
            let completed = self
                .connection
                .query_row(
                    "SELECT idempotency_key, parent_effect_key, tracker_id
                       FROM adapter_effects
                      WHERE role_id = ?1 AND operation = 'application.applied.confirm'
                        AND status = 'completed' AND tracker_id = ?2
                      ORDER BY updated_at DESC LIMIT 1",
                    params![role_id, tracker_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<i64>>(2)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| {
                    StoreError::InvalidAdapterEffect(
                        "recorded application is missing its completed canonical effect"
                            .to_string(),
                    )
                })?;
            return Ok((
                role_id.clone(),
                AdapterEffectContext {
                    idempotency_key: completed.0,
                    role: self.adapter_role_context(&role_id)?,
                    parent_effect_key: completed.1,
                    tracker_id: completed.2,
                },
            ));
        }
        if role_is_applied {
            return Err(StoreError::InvalidAdapterEffect(
                "this role is already recorded as Applied".to_string(),
            ));
        }
        let discard_in_progress = self.connection.query_row(
            "SELECT COUNT(*) FROM adapter_effects
              WHERE role_id = ?1 AND operation = 'role.discard' AND status = 'pending'",
            [&role_id],
            |row| row.get::<_, i64>(0),
        )?;
        if discard_in_progress > 0 {
            return Err(StoreError::InvalidAdapterEffect(
                "application dismissal is already in progress".to_string(),
            ));
        }
        let effect = self.begin_adapter_effect(
            &role_id,
            "application.applied.confirm",
            None,
            Some(tracker_id),
        )?;
        Ok((role_id, effect))
    }

    pub fn begin_applied_effect_for_role(
        &mut self,
        role_id: &str,
    ) -> Result<AdapterEffectContext, StoreError> {
        let (tracker_id, canonical_status) = self
            .connection
            .query_row(
                "SELECT canonical_tracker_id, canonical_status FROM roles WHERE id = ?1",
                [role_id],
                |row| {
                    Ok((
                        row.get::<_, Option<i64>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::InvalidAdapterEffect("role not found".to_string()))?;
        let tracker_id = tracker_id.ok_or_else(|| {
            StoreError::InvalidAdapterEffect("canonical tracker row is missing".to_string())
        })?;
        let role_is_applied = canonical_status.as_deref().is_some_and(is_applied_status);
        if role_is_applied {
            let completed = self
                .connection
                .query_row(
                    "SELECT idempotency_key, parent_effect_key, tracker_id
                       FROM adapter_effects
                      WHERE role_id = ?1 AND operation = 'application.applied.confirm'
                        AND status = 'completed' AND tracker_id = ?2
                      ORDER BY updated_at DESC LIMIT 1",
                    params![role_id, tracker_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<i64>>(2)?,
                        ))
                    },
                )
                .optional()?;
            if let Some(completed) = completed {
                return Ok(AdapterEffectContext {
                    idempotency_key: completed.0,
                    role: self.adapter_role_context(role_id)?,
                    parent_effect_key: completed.1,
                    tracker_id: completed.2,
                });
            }
            return Err(StoreError::InvalidAdapterEffect(
                "this role is already recorded as Applied".to_string(),
            ));
        }
        if canonical_status
            .as_deref()
            .is_some_and(|status| is_discarded_status(status) || is_rejected_status(status))
        {
            return Err(StoreError::InvalidAdapterEffect(
                "the role already has a terminal canonical outcome".to_string(),
            ));
        }
        let discard_in_progress = self.connection.query_row(
            "SELECT COUNT(*) FROM adapter_effects
              WHERE role_id = ?1 AND operation = 'role.discard' AND status = 'pending'",
            [role_id],
            |row| row.get::<_, i64>(0),
        )?;
        if discard_in_progress > 0 {
            return Err(StoreError::InvalidAdapterEffect(
                "application dismissal is already in progress".to_string(),
            ));
        }
        let latest_prep_status: Option<String> = self
            .connection
            .query_row(
                "SELECT status FROM preparation_jobs
                  WHERE role_id = ?1 AND status != 'cancelled'
                  ORDER BY updated_at DESC, id DESC LIMIT 1",
                [role_id],
                |row| row.get(0),
            )
            .optional()?;
        match latest_prep_status.as_deref() {
            Some("queued") | Some("preparing") => {
                return Err(StoreError::InvalidAdapterEffect(
                    "Cancel active work first".to_string(),
                ));
            }
            Some("action_required") | Some("completed") => {}
            _ => {
                return Err(StoreError::InvalidAdapterEffect(
                    "role is not an idle Applications preparation".to_string(),
                ));
            }
        }
        let blocking_session_status: Option<String> = self
            .connection
            .query_row(
                "SELECT status FROM browser_sessions
                  WHERE purpose = 'application' AND role_id = ?1
                    AND status IN (
                      'waiting_for_extension', 'inspecting', 'drafting_answers', 'answering',
                      'filling', 'persisting_answers', 'saving_answers', 'releasing',
                      'connection_verified', 'review_required', 'submitted_tracking_pending'
                    )
                  ORDER BY updated_at DESC, id DESC LIMIT 1",
                [role_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(status) = blocking_session_status.as_deref() {
            if status == "review_required" || status == "submitted_tracking_pending" {
                return Err(StoreError::InvalidAdapterEffect(
                    "confirm the open application session instead".to_string(),
                ));
            }
            return Err(StoreError::InvalidAdapterEffect(
                "Cancel active work first".to_string(),
            ));
        }
        self.begin_adapter_effect(
            role_id,
            "application.applied.confirm",
            None,
            Some(tracker_id),
        )
    }

    pub fn complete_applied_effect_for_role(
        &mut self,
        role_id: &str,
        idempotency_key: &str,
        tracker_id: i64,
        canonical_status: &str,
    ) -> Result<(), StoreError> {
        if canonical_status != "Applied" {
            return Err(StoreError::InvalidAdapterEffect(
                "applied effect returned a non-Applied status".to_string(),
            ));
        }
        let confirmed_at = Utc::now();
        let confirmed_at_text = confirmed_at.to_rfc3339();
        let confirmation_date = confirmed_at.with_timezone(&Madrid).date_naive().to_string();
        let transaction = self.connection.transaction()?;
        let changed = transaction.execute(
            "UPDATE adapter_effects SET status = 'completed', tracker_id = ?1,
                    error_class = NULL, updated_at = ?2
              WHERE idempotency_key = ?3 AND role_id = ?4 AND status = 'pending'",
            params![tracker_id, confirmed_at_text, idempotency_key, role_id],
        )?;
        if changed != 1 {
            let already_completed_at = transaction
                .query_row(
                    "SELECT e.updated_at
                   FROM adapter_effects e
                   JOIN roles r ON r.id = e.role_id
                  WHERE e.idempotency_key = ?1 AND e.role_id = ?2
                    AND e.operation = 'application.applied.confirm'
                    AND e.status = 'completed' AND e.tracker_id = ?3
                    AND LOWER(TRIM(COALESCE(r.canonical_status, ''))) = 'applied'",
                    params![idempotency_key, role_id, tracker_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if let Some(completed_at) = already_completed_at {
                let completed_date = DateTime::parse_from_rfc3339(&completed_at)
                    .map(|value| value.with_timezone(&Madrid).date_naive().to_string())
                    .unwrap_or(confirmation_date);
                transaction.execute(
                    "UPDATE roles SET canonical_status = 'Applied',
                                      canonical_date = COALESCE(canonical_date, ?1)
                      WHERE id = ?2
                        AND LOWER(TRIM(COALESCE(canonical_status, ''))) = 'applied'",
                    params![completed_date, role_id],
                )?;
                transaction.commit()?;
                return Ok(());
            }
            return Err(StoreError::InvalidAdapterEffect(
                "applied effect is missing or no longer pending".to_string(),
            ));
        }
        let had_completed_prep: bool = transaction.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM preparation_jobs
                WHERE role_id = ?1 AND status = 'completed'
             )",
            [role_id],
            |row| row.get(0),
        )?;
        transaction.execute(
            "UPDATE roles SET canonical_tracker_id = ?1, canonical_status = 'Applied',
                    canonical_date = CASE
                      WHEN LOWER(TRIM(COALESCE(canonical_status, ''))) = 'applied'
                        THEN COALESCE(canonical_date, ?2)
                      ELSE ?2 END,
                    canonical_visibility_override = 0,
                    updated_at = ?3 WHERE id = ?4",
            params![tracker_id, confirmation_date, confirmed_at_text, role_id],
        )?;
        transaction.execute(
            "UPDATE evaluation_sync
                SET state = 'terminal', reason = 'canonical_terminal',
                    current_receipt_key = NULL, input_hash = NULL,
                    lease_expires_at = NULL, updated_at = ?1
              WHERE role_id = ?2",
            params![confirmed_at_text, role_id],
        )?;
        transaction.execute(
            "UPDATE browser_commands
                SET status = 'permanent', error_code = 'canonical_terminal',
                    lease_expires_at = NULL, updated_at = ?1
              WHERE status IN ('pending', 'leased')
                AND session_id IN (
                  SELECT id FROM browser_sessions
                   WHERE purpose = 'application' AND role_id = ?2
                )",
            params![confirmed_at_text, role_id],
        )?;
        transaction.execute(
            "UPDATE browser_sessions
                SET status = 'applied_recorded', error_code = NULL,
                    driver_lease_state = 'released', fallback_eligible = 0,
                    handoff_reason = 'canonical_terminal', updated_at = ?1
              WHERE purpose = 'application' AND role_id = ?2
                AND status != 'applied_recorded'",
            params![confirmed_at_text, role_id],
        )?;
        transaction.execute(
            "UPDATE preparation_jobs
                SET status = 'cancelled', step = 'cancelled',
                    error_class = NULL, error_stage = NULL, error_detail = NULL,
                    retry_policy = NULL, updated_at = ?1
              WHERE role_id = ?2 AND status != 'cancelled'",
            params![confirmed_at_text, role_id],
        )?;
        transaction.execute(
            "UPDATE roles SET preparation_state = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                if had_completed_prep {
                    "prepared"
                } else {
                    "not_started"
                },
                confirmed_at_text,
                role_id
            ],
        )?;
        transaction.execute(
            "INSERT INTO activity(id, kind, message, occurred_at)
             VALUES (?1, 'application', 'The user confirmed an external application; career-ops recorded the canonical Applied outcome.', ?2)",
            params![Uuid::new_v4().to_string(), confirmed_at_text],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn begin_preparation(
        &mut self,
        role_id: &str,
        provider: &str,
    ) -> Result<PreparationWork, StoreError> {
        if !matches!(provider, "codex" | "claude") {
            return Err(StoreError::InvalidPreparation(
                "provider must be codex or claude".to_string(),
            ));
        }
        if self.canonical_role_is_terminal(role_id)? {
            return Err(StoreError::InvalidPreparation(
                "the role already has a terminal canonical outcome".to_string(),
            ));
        }
        let evaluation = self.current_preparation_evaluation(role_id)?;
        let role = self.adapter_role_context(role_id)?;
        if role.application_url.is_none() {
            return Err(StoreError::InvalidPreparation(
                "the role has no application URL".to_string(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection.transaction()?;
        let role_is_terminal = transaction.query_row(
            "SELECT LOWER(TRIM(COALESCE(canonical_status, '')))
                    IN ('applied', 'discarded', 'rejected')
               FROM roles WHERE id = ?1",
            [role_id],
            |row| row.get::<_, bool>(0),
        )?;
        if role_is_terminal {
            return Err(StoreError::InvalidPreparation(
                "the role already has a terminal canonical outcome".to_string(),
            ));
        }
        let existing = transaction
            .query_row(
                "SELECT id, provider, retry_policy FROM preparation_jobs
                  WHERE role_id = ?1 AND status = 'action_required'
                  ORDER BY updated_at DESC LIMIT 1",
                [role_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        if let Some((id, stored_provider, retry_policy)) = existing {
            let requires_fresh_preparation = matches!(
                retry_policy.as_deref(),
                Some("fresh_preparation_provider_run") | Some("fresh_preparation_id")
            );
            if requires_fresh_preparation {
                transaction.execute(
                    "UPDATE preparation_jobs
                        SET status = 'cancelled', step = 'cancelled',
                            error_class = NULL, error_stage = NULL, error_detail = NULL,
                            retry_policy = NULL, updated_at = ?1
                      WHERE id = ?2 AND status = 'action_required'",
                    params![now, id],
                )?;
                // Fall through to insert a new preparation id so recover cannot
                // reuse a poisoned provider-result from the failed job.
            } else {
                if stored_provider != provider {
                    return Err(StoreError::InvalidPreparation(format!(
                        "retry must use the original {stored_provider} provider"
                    )));
                }
                transaction.execute(
                    "UPDATE preparation_jobs
                        SET status = 'queued', step = 'queued',
                            error_class = NULL, error_stage = NULL, error_detail = NULL,
                            retry_policy = NULL,
                            updated_at = ?1
                      WHERE id = ?2",
                    params![now, id],
                )?;
                transaction.execute(
                    "UPDATE roles SET preparation_state = 'queued', updated_at = ?1 WHERE id = ?2",
                    params![now, role_id],
                )?;
                transaction.commit()?;
                return Ok(PreparationWork {
                    id,
                    role_id: role_id.to_string(),
                    provider: stored_provider,
                    role,
                    evaluation,
                });
            }
        }
        let already_active: bool = transaction.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM preparation_jobs
                WHERE role_id = ?1 AND status IN ('queued', 'preparing', 'completed')
             )",
            [role_id],
            |row| row.get(0),
        )?;
        if already_active {
            return Err(StoreError::InvalidPreparation(
                "this role already has an active or completed preparation".to_string(),
            ));
        }
        let id = Uuid::new_v4().to_string();
        transaction.execute(
            "INSERT INTO preparation_jobs(
               id, role_id, provider, status, step, attempt, created_at, updated_at
             ) VALUES (?1, ?2, ?3, 'queued', 'queued', 0, ?4, ?4)",
            params![id, role_id, provider, now],
        )?;
        transaction.execute(
            "UPDATE roles SET preparation_state = 'queued', review_state = 'viewed', updated_at = ?1
              WHERE id = ?2",
            params![now, role_id],
        )?;
        transaction.commit()?;
        Ok(PreparationWork {
            id,
            role_id: role_id.to_string(),
            provider: provider.to_string(),
            role,
            evaluation,
        })
    }

    pub fn claim_preparation_work(&mut self) -> Result<Option<PreparationWork>, StoreError> {
        let transaction = self.connection.transaction()?;
        let candidate = transaction
            .query_row(
                "SELECT p.id, p.provider, p.role_id, r.company, r.title, r.location, r.application_url
                  FROM preparation_jobs p
                  JOIN roles r ON r.id = p.role_id
                  WHERE p.status = 'queued'
                    AND LOWER(TRIM(COALESCE(r.canonical_status, '')))
                      NOT IN ('applied', 'discarded', 'rejected')
                    AND (
                      SELECT COUNT(*) FROM preparation_jobs active
                      JOIN roles active_role ON active_role.id = active.role_id
                      WHERE active.status = 'preparing'
                        AND LOWER(TRIM(COALESCE(active_role.canonical_status, '')))
                          NOT IN ('applied', 'discarded', 'rejected')
                    ) < ?1
                  ORDER BY p.created_at, p.rowid LIMIT 1",
                [MAX_ACTIVE_PREPARATIONS],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        AdapterRoleContext {
                            company: row.get(3)?,
                            title: row.get(4)?,
                            location: row.get(5)?,
                            application_url: row.get(6)?,
                        },
                    ))
                },
            )
            .optional()?;
        let Some((id, provider, role_id, role)) = candidate else {
            transaction.commit()?;
            return Ok(None);
        };
        let now = Utc::now().to_rfc3339();
        let changed = transaction.execute(
            "UPDATE preparation_jobs
                SET status = 'preparing', step = 'preparing_report', attempt = attempt + 1,
                    error_class = NULL, error_stage = NULL, error_detail = NULL,
                    retry_policy = NULL, updated_at = ?1
              WHERE id = ?2 AND status = 'queued'",
            params![now, id],
        )?;
        if changed != 1 {
            transaction.commit()?;
            return Ok(None);
        }
        transaction.execute(
            "UPDATE roles SET preparation_state = 'preparing', updated_at = ?1 WHERE id = ?2",
            params![now, role_id],
        )?;
        transaction.commit()?;
        let evaluation = self.current_preparation_evaluation(&role_id)?;
        Ok(Some(PreparationWork {
            id,
            role_id,
            provider,
            role,
            evaluation,
        }))
    }

    pub fn recover_interrupted_preparations(&mut self) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        let now = Utc::now().to_rfc3339();
        transaction.execute(
            "UPDATE roles SET preparation_state = 'queued', updated_at = ?1
              WHERE id IN (SELECT role_id FROM preparation_jobs WHERE status = 'queued')",
            [now.clone()],
        )?;
        transaction.execute(
            "UPDATE roles SET preparation_state = 'failed', updated_at = ?1
              WHERE id IN (SELECT role_id FROM preparation_jobs WHERE status = 'preparing')",
            [now.clone()],
        )?;
        transaction.execute(
            "UPDATE preparation_jobs
                SET status = 'action_required', error_class = 'app_interrupted',
                    error_stage = step,
                    error_detail = 'HereForWork quit while this preparation was still running.',
                    updated_at = ?1
              WHERE status = 'preparing'",
            [now],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn next_preparation_for_browser_handoff(&self) -> Result<Option<String>, StoreError> {
        self.connection
            .query_row(
                "SELECT p.id
                   FROM preparation_jobs p
                   JOIN roles r ON r.id = p.role_id
                  WHERE p.status = 'completed'
                    AND LOWER(TRIM(COALESCE(r.canonical_status, '')))
                      NOT IN ('applied', 'discarded', 'rejected')
                    AND NOT EXISTS (
                      SELECT 1 FROM browser_sessions b WHERE b.preparation_id = p.id
                    )
                    AND NOT EXISTS (
                      SELECT 1 FROM preparation_jobs earlier
                      JOIN roles earlier_role ON earlier_role.id = earlier.role_id
                       WHERE earlier.rowid < p.rowid
                         AND earlier.status IN ('queued', 'preparing')
                         AND LOWER(TRIM(COALESCE(earlier_role.canonical_status, '')))
                           NOT IN ('applied', 'discarded', 'rejected')
                    )
                  ORDER BY p.rowid LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn cancel_inactive_preparation_for_role(
        &mut self,
        role_id: &str,
    ) -> Result<bool, StoreError> {
        let transaction = self.connection.transaction()?;
        let preparation = transaction
            .query_row(
                "SELECT id, status, error_class FROM preparation_jobs
                  WHERE role_id = ?1
                  ORDER BY updated_at DESC, id DESC LIMIT 1",
                [role_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((preparation_id, status, error_class)) = preparation else {
            transaction.commit()?;
            return Ok(false);
        };
        if status == "cancelled" {
            transaction.commit()?;
            return Ok(true);
        }
        let cancellable = matches!(status.as_str(), "queued" | "preparing")
            || (status == "action_required" && error_class.as_deref() == Some("app_interrupted"));
        if !cancellable {
            transaction.commit()?;
            return Ok(false);
        }
        let now = Utc::now().to_rfc3339();
        transaction.execute(
            "UPDATE preparation_jobs SET status = 'cancelled', step = 'cancelled',
                    error_class = NULL, error_stage = NULL, error_detail = NULL,
                    retry_policy = NULL, updated_at = ?1 WHERE id = ?2",
            params![now, preparation_id],
        )?;
        transaction.execute(
            "UPDATE roles SET preparation_state = 'not_started', updated_at = ?1 WHERE id = ?2",
            params![now, role_id],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    /// List roles eligible for Q7=B one-shot stuck-preparation cleanup.
    ///
    /// Includes failed / `action_required` preparation that lacks
    /// `evaluation_sync` in (`ready`, `needs_decision`), plus any
    /// `force_role_ids` that still have blocking failed prep.
    /// Never includes Applied / Discarded terminal canonical roles.
    pub fn list_stuck_preparation_cleanup_candidates(
        &self,
        force_role_ids: &[&str],
    ) -> Result<Vec<StuckPreparationCandidate>, StoreError> {
        let force: HashSet<&str> = force_role_ids.iter().copied().collect();
        let mut statement = self.connection.prepare(
            "SELECT r.id, r.company, r.title, r.preparation_state, r.canonical_status,
                    evaluation.state,
                    latest.id, latest.status, latest.error_class
               FROM roles r
               LEFT JOIN evaluation_sync evaluation ON evaluation.role_id = r.id
               LEFT JOIN preparation_jobs latest ON latest.id = (
                 SELECT candidate.id FROM preparation_jobs candidate
                  WHERE candidate.role_id = r.id AND candidate.status != 'cancelled'
                  ORDER BY candidate.updated_at DESC, candidate.id DESC LIMIT 1
               )
              WHERE LOWER(TRIM(COALESCE(r.canonical_status, '')))
                      NOT IN ('applied', 'discarded')
                AND (
                      r.preparation_state = 'failed'
                      OR latest.status = 'action_required'
                    )
              ORDER BY r.company, r.title, r.id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut candidates = Vec::new();
        for (
            role_id,
            company,
            title,
            preparation_state,
            canonical_status,
            evaluation_sync_state,
            preparation_id,
            preparation_status,
            error_class,
        ) in rows
        {
            let eval = evaluation_sync_state.as_deref().unwrap_or("");
            let force_selected = force.contains(role_id.as_str());
            let zombie = !matches!(eval, "ready" | "needs_decision");
            if !force_selected && !zombie {
                continue;
            }
            candidates.push(StuckPreparationCandidate {
                role_id,
                company,
                title,
                preparation_state,
                canonical_status,
                evaluation_sync_state,
                preparation_id,
                preparation_status,
                error_class,
                selection_reason: if force_selected {
                    "force_role".to_string()
                } else {
                    "zombie_failed_prep".to_string()
                },
            });
        }
        Ok(candidates)
    }

    /// Clear blocking failed / `action_required` preparation for a role so
    /// Prepare can start fresh. Preserves evaluation receipts, evaluation_sync,
    /// and canonical tracker fields. Refuses Applied / Discarded.
    pub fn reset_stuck_preparation_for_role(
        &mut self,
        role_id: &str,
    ) -> Result<StuckPreparationReset, StoreError> {
        let transaction = self.connection.transaction()?;
        let role = transaction
            .query_row(
                "SELECT company, title, preparation_state, canonical_status
                   FROM roles WHERE id = ?1",
                [role_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::InvalidPreparation("role was not found".to_string()))?;
        let (company, title, preparation_state_before, canonical_status) = role;
        if canonical_status
            .as_deref()
            .is_some_and(|status| matches!(status.trim().to_ascii_lowercase().as_str(), "applied" | "discarded"))
        {
            return Err(StoreError::InvalidPreparation(
                "refusing to reset preparation for an Applied or Discarded role".to_string(),
            ));
        }

        let mut job_statement = transaction.prepare(
            "SELECT id FROM preparation_jobs
              WHERE role_id = ?1 AND status = 'action_required'
              ORDER BY updated_at DESC, id DESC",
        )?;
        let preparation_ids = job_statement
            .query_map([role_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(job_statement);

        let now = Utc::now().to_rfc3339();
        let mut cancelled_preparation_ids = Vec::new();
        for preparation_id in &preparation_ids {
            let cancelled = transaction.execute(
                "UPDATE preparation_jobs SET status = 'cancelled', step = 'cancelled',
                        error_class = NULL, error_stage = NULL, error_detail = NULL,
                        retry_policy = NULL, updated_at = ?1
                  WHERE id = ?2 AND status = 'action_required'",
                params![now, preparation_id],
            )?;
            if cancelled == 1 {
                cancelled_preparation_ids.push(preparation_id.clone());
            }
        }

        let needs_state_reset = preparation_state_before != "not_started";
        if needs_state_reset {
            transaction.execute(
                "UPDATE roles SET preparation_state = 'not_started', updated_at = ?1 WHERE id = ?2",
                params![now, role_id],
            )?;
        }

        let expired_notification_count = if !cancelled_preparation_ids.is_empty() || needs_state_reset
        {
            transaction.execute(
                "UPDATE notification_outbox
                    SET status = 'expired', last_error = 'stuck_preparation_cleanup'
                  WHERE status IN ('pending', 'delivering')
                    AND event_kind = 'preparation_failed'
                    AND role_id = ?1",
                [role_id],
            )?
        } else {
            0
        };

        let changed = needs_state_reset || !cancelled_preparation_ids.is_empty();
        transaction.commit()?;
        Ok(StuckPreparationReset {
            role_id: role_id.to_string(),
            company,
            title,
            preparation_state_before,
            preparation_state_after: "not_started".to_string(),
            cancelled_preparation_ids,
            expired_notification_count: expired_notification_count as usize,
            changed,
        })
    }

    /// Apply Q7=B cleanup for all current candidates (including force roles).
    pub fn reset_stuck_preparations(
        &mut self,
        force_role_ids: &[&str],
    ) -> Result<Vec<StuckPreparationReset>, StoreError> {
        let candidates = self.list_stuck_preparation_cleanup_candidates(force_role_ids)?;
        let mut results = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            results.push(self.reset_stuck_preparation_for_role(&candidate.role_id)?);
        }
        Ok(results)
    }

    pub fn preparation_artifact_paths(
        &self,
        preparation_id: &str,
    ) -> Result<(String, String), StoreError> {
        self.connection
            .query_row(
                "SELECT report_path, cv_pdf_path FROM preparation_jobs
                  WHERE id = ?1 AND status = 'completed'",
                [preparation_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::InvalidPreparation(
                    "completed preparation artifacts were not found".to_string(),
                )
            })
    }

    pub fn preparation_detail(
        &self,
        preparation_id: &str,
    ) -> Result<crate::domain::PreparationDetail, StoreError> {
        self.connection
            .query_row(
                "SELECT p.id, p.role_id, r.company, r.title, p.provider, p.status, p.step,
                        p.error_class, p.error_stage, p.error_detail, p.retry_policy,
                        p.report_path, p.cv_pdf_path, p.cv_source
                   FROM preparation_jobs p JOIN roles r ON r.id = p.role_id
                  WHERE p.id = ?1 AND p.status != 'cancelled'",
                [preparation_id],
                |row| {
                    Ok(crate::domain::PreparationDetail {
                        preparation_id: row.get(0)?,
                        role_id: row.get(1)?,
                        company: row.get(2)?,
                        title: row.get(3)?,
                        provider: row.get(4)?,
                        status: row.get(5)?,
                        stage: row.get::<_, Option<String>>(8)?.unwrap_or(row.get(6)?),
                        error_class: row.get(7)?,
                        error_detail: row.get(9)?,
                        retry_policy: row.get(10)?,
                        report_markdown: None,
                        report_path: row.get(11)?,
                        cv_pdf_path: row.get(12)?,
                        cv_source: row.get(13)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::InvalidPreparation("preparation not found".to_string()))
    }

    pub fn record_preparation_context(
        &mut self,
        preparation_id: &str,
        context_hash: &str,
        resolved_application_url: &str,
    ) -> Result<(), StoreError> {
        let resolved_application_url = application_form_url(resolved_application_url)?;
        let changed = self.connection.execute(
            "UPDATE preparation_jobs
                SET context_hash = ?1, resolved_application_url = ?2,
                    step = 'preparing_cv', updated_at = ?3
              WHERE id = ?4 AND status = 'preparing'",
            params![
                context_hash,
                resolved_application_url,
                Utc::now().to_rfc3339(),
                preparation_id
            ],
        )?;
        if changed != 1 {
            return Err(StoreError::InvalidPreparation(
                "preparation is missing or no longer active".to_string(),
            ));
        }
        Ok(())
    }

    pub fn complete_preparation(
        &mut self,
        preparation_id: &str,
        completion: &PreparationCompletion<'_>,
    ) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection.transaction()?;
        let (role_id, role_is_terminal) = transaction
            .query_row(
                "SELECT p.role_id,
                        LOWER(TRIM(COALESCE(r.canonical_status, '')))
                          IN ('applied', 'discarded', 'rejected')
                   FROM preparation_jobs p JOIN roles r ON r.id = p.role_id
                  WHERE p.id = ?1 AND p.status = 'preparing'",
                [preparation_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::InvalidPreparation(
                    "preparation is missing or no longer active".to_string(),
                )
            })?;
        if role_is_terminal {
            return Err(StoreError::InvalidPreparation(
                "the role already has a terminal canonical outcome".to_string(),
            ));
        }
        transaction.execute(
            "UPDATE preparation_jobs
                SET status = 'completed', step = 'prepared', tracker_id = ?1,
                    report_path = ?2, report_hash = ?3, cv_pdf_path = ?4, cv_pdf_hash = ?5,
                    error_class = NULL, error_stage = NULL, error_detail = NULL,
                    retry_policy = NULL, cv_source = ?6, updated_at = ?7
              WHERE id = ?8",
            params![
                completion.tracker_id,
                completion.report_path,
                completion.report_hash,
                completion.cv_pdf_path,
                completion.cv_pdf_hash,
                completion.cv_source,
                now,
                preparation_id
            ],
        )?;
        transaction.execute(
            "UPDATE roles
                SET preparation_state = 'prepared', canonical_tracker_id = ?1,
                    canonical_status = 'Evaluated', canonical_visibility_override = 0,
                    updated_at = ?2
              WHERE id = ?3",
            params![completion.tracker_id, now, role_id],
        )?;
        transaction.execute(
            "INSERT INTO activity(id, kind, message, occurred_at)
             VALUES (?1, 'preparation', ?2, ?3)",
            params![
                Uuid::new_v4().to_string(),
                if completion.cv_source == "user_reviewed_fallback" {
                    "career-ops prepared a report; PDF rendering failed, so HereForWork used the configured user-reviewed CV without tailoring it."
                } else {
                    "career-ops prepared a report and fact-checked tailored CV."
                },
                now
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn begin_preparation_cleanup(
        &mut self,
        preparation_id: &str,
    ) -> Result<PreparationCleanupWork, StoreError> {
        let (role_id, preparation_status, report_path, cv_pdf_path, canonical_status) = self
            .connection
            .query_row(
                "SELECT p.role_id, p.status, p.report_path, p.cv_pdf_path, r.canonical_status
                   FROM preparation_jobs p JOIN roles r ON r.id = p.role_id
                  WHERE p.id = ?1",
                [preparation_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::InvalidPreparation("application preparation was not found".to_string())
            })?;
        if canonical_status.as_deref().is_some_and(is_applied_status) {
            return Err(StoreError::InvalidPreparation(
                "an Applied application cannot be dismissed".to_string(),
            ));
        }
        let latest_application_status = self
            .connection
            .query_row(
                "SELECT status FROM browser_sessions
                  WHERE preparation_id = ?1 AND purpose = 'application'
                  ORDER BY updated_at DESC, id DESC LIMIT 1",
                [preparation_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let dismissible = matches!(
            (
                preparation_status.as_str(),
                latest_application_status.as_deref(),
            ),
            (
                "action_required",
                None | Some("action_required") | Some("review_required")
            ) | (
                "completed",
                Some("action_required") | Some("review_required")
            )
        );
        if !dismissible {
            return Err(StoreError::InvalidPreparation(
                "only a failed preparation or an application ready for review can be dismissed"
                    .to_string(),
            ));
        }
        let applied_effect_count = self.connection.query_row(
            "SELECT COUNT(*) FROM adapter_effects
              WHERE role_id = ?1 AND operation = 'application.applied.confirm'
                AND status = 'pending'",
            [&role_id],
            |row| row.get::<_, i64>(0),
        )?;
        if applied_effect_count > 0 {
            return Err(StoreError::InvalidPreparation(
                "application tracking is already in progress".to_string(),
            ));
        }
        let effect = self.begin_adapter_effect(&role_id, "role.discard", None, None)?;
        Ok(PreparationCleanupWork {
            preparation_id: preparation_id.to_string(),
            role_id,
            report_path,
            cv_pdf_path,
            effect,
        })
    }

    pub fn fail_preparation_cleanup(
        &mut self,
        preparation_id: &str,
        idempotency_key: &str,
        error_class: &str,
    ) -> Result<(), StoreError> {
        self.fail_adapter_effect(idempotency_key, error_class)?;
        self.connection.execute(
            "UPDATE preparation_jobs SET status = 'action_required', step = 'undo_cleanup',
                    error_class = ?1, updated_at = ?2 WHERE id = ?3",
            params![error_class, Utc::now().to_rfc3339(), preparation_id],
        )?;
        Ok(())
    }

    pub fn complete_preparation_cleanup(
        &mut self,
        work: &PreparationCleanupWork,
        tracker_id: i64,
        canonical_status: &str,
    ) -> Result<(), StoreError> {
        if canonical_status != "Discarded" {
            return Err(StoreError::InvalidAdapterEffect(
                "preparation cleanup returned a non-Discarded status".to_string(),
            ));
        }
        let transaction = self.connection.transaction()?;
        let now = Utc::now().to_rfc3339();
        let role_is_applied = transaction.query_row(
            "SELECT LOWER(TRIM(COALESCE(canonical_status, ''))) = 'applied'
               FROM roles WHERE id = ?1",
            [&work.role_id],
            |row| row.get::<_, bool>(0),
        )?;
        if role_is_applied {
            return Err(StoreError::InvalidAdapterEffect(
                "an Applied application cannot be dismissed".to_string(),
            ));
        }
        let changed = transaction.execute(
            "UPDATE adapter_effects SET status = 'completed', tracker_id = ?1,
                    error_class = NULL, updated_at = ?2
              WHERE idempotency_key = ?3 AND role_id = ?4 AND status = 'pending'",
            params![tracker_id, now, &work.effect.idempotency_key, &work.role_id],
        )?;
        if changed != 1 {
            return Err(StoreError::InvalidAdapterEffect(
                "preparation cleanup effect is missing or no longer pending".to_string(),
            ));
        }
        transaction.execute(
            "DELETE FROM browser_commands WHERE session_id IN (
                SELECT id FROM browser_sessions WHERE preparation_id = ?1
             )",
            [&work.preparation_id],
        )?;
        transaction.execute(
            "DELETE FROM browser_sessions WHERE preparation_id = ?1",
            [&work.preparation_id],
        )?;
        transaction.execute(
            "DELETE FROM notification_outbox WHERE preparation_id = ?1",
            [&work.preparation_id],
        )?;
        transaction.execute(
            "DELETE FROM preparation_jobs WHERE id = ?1",
            [&work.preparation_id],
        )?;
        transaction.execute(
            "UPDATE roles SET preparation_state = 'not_started', review_state = 'dismissed',
                    canonical_tracker_id = ?1, canonical_status = 'Discarded',
                    canonical_visibility_override = 0, updated_at = ?2 WHERE id = ?3",
            params![tracker_id, now, &work.role_id],
        )?;
        transaction.execute(
            "INSERT INTO activity(id, kind, message, occurred_at)
             VALUES (?1, 'preparation', 'Preparation was discarded and its generated artifacts were deleted.', ?2)",
            params![Uuid::new_v4().to_string(), now],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn fail_preparation(
        &mut self,
        preparation_id: &str,
        error_class: &str,
        error_stage: &str,
        error_detail: &str,
    ) -> Result<(), StoreError> {
        self.fail_preparation_with_policy(
            preparation_id,
            error_class,
            error_stage,
            error_detail,
            "retry_same_preparation",
        )
    }

    pub fn fail_preparation_with_policy(
        &mut self,
        preparation_id: &str,
        error_class: &str,
        error_stage: &str,
        error_detail: &str,
        retry_policy: &str,
    ) -> Result<(), StoreError> {
        let error_detail = sanitize_preparation_error(error_detail);
        let retry_policy = sanitize_retry_policy(retry_policy);
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection.transaction()?;
        let preparation = transaction
            .query_row(
                "SELECT p.role_id, p.attempt, r.company, r.title
                   FROM preparation_jobs p JOIN roles r ON r.id = p.role_id
                  WHERE p.id = ?1 AND p.status = 'preparing'",
                [preparation_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        if let Some((role_id, attempt, company, title)) = preparation {
            transaction.execute(
                "UPDATE preparation_jobs
                    SET status = 'action_required', error_class = ?1, error_stage = ?2,
                        error_detail = ?3, retry_policy = ?4, updated_at = ?5
                  WHERE id = ?6",
                params![
                    error_class,
                    error_stage,
                    error_detail,
                    retry_policy,
                    now,
                    preparation_id
                ],
            )?;
            transaction.execute(
                "UPDATE roles SET preparation_state = 'failed', updated_at = ?1 WHERE id = ?2",
                params![now, role_id],
            )?;
            transaction.execute(
                "INSERT OR IGNORE INTO notification_outbox(
                   id, dedupe_key, title, body, status, attempts, next_attempt_at, created_at,
                   event_kind, role_id, preparation_id, action_kind, action_label
                 ) VALUES (?1, ?2, 'Preparation failed', ?3, 'pending', 0, ?4, ?4,
                           'preparation_failed', ?5, ?6, 'view_details', 'View details')",
                params![
                    Uuid::new_v4().to_string(),
                    format!("preparation:{preparation_id}:attempt:{attempt}:failed"),
                    format!(
                        "{title} at {company}. {}: {error_detail}",
                        preparation_stage_label(error_stage)
                    ),
                    now,
                    role_id,
                    preparation_id,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn cancel_preparation(&mut self, preparation_id: &str) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection.transaction()?;
        let role_id = transaction
            .query_row(
                "SELECT role_id FROM preparation_jobs WHERE id = ?1 AND status = 'preparing'",
                [preparation_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(role_id) = role_id {
            transaction.execute(
                "UPDATE preparation_jobs SET status = 'cancelled', step = 'cancelled',
                        error_class = NULL, updated_at = ?1 WHERE id = ?2",
                params![now, preparation_id],
            )?;
            transaction.execute(
                "UPDATE roles SET preparation_state = 'not_started', updated_at = ?1 WHERE id = ?2",
                params![now, role_id],
            )?;
            transaction.execute(
                "INSERT INTO activity(id, kind, message, occurred_at)
                 VALUES (?1, 'preparation', 'Application preparation was cancelled before canonical artifacts were committed.', ?2)",
                params![Uuid::new_v4().to_string(), now],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn begin_undo_discard_effect(
        &mut self,
        role_id: &str,
    ) -> Result<AdapterEffectContext, StoreError> {
        if self.canonical_role_is_applied(role_id)? {
            return Err(StoreError::InvalidAdapterEffect(
                "an Applied role cannot undo a previous dismissal".to_string(),
            ));
        }
        let parent = self
            .connection
            .query_row(
                "SELECT idempotency_key, tracker_id FROM adapter_effects
                  WHERE role_id = ?1 AND operation = 'role.discard' AND status = 'completed'
                  ORDER BY updated_at DESC LIMIT 1",
                [role_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::InvalidAdapterEffect(
                    "no completed HereForWork dismissal is available to undo".to_string(),
                )
            })?;
        let tracker_id = parent.1.ok_or_else(|| {
            StoreError::InvalidAdapterEffect(
                "the completed dismissal has no canonical tracker id".to_string(),
            )
        })?;
        self.begin_adapter_effect(
            role_id,
            "role.discard.undo",
            Some(parent.0),
            Some(tracker_id),
        )
    }

    fn begin_adapter_effect(
        &mut self,
        role_id: &str,
        operation: &str,
        parent_effect_key: Option<String>,
        tracker_id: Option<i64>,
    ) -> Result<AdapterEffectContext, StoreError> {
        let role = self.adapter_role_context(role_id)?;
        let existing = self
            .connection
            .query_row(
                "SELECT idempotency_key, parent_effect_key, tracker_id
                   FROM adapter_effects
                  WHERE role_id = ?1 AND operation = ?2
                    AND status IN ('pending', 'action_required')
                  ORDER BY updated_at DESC LIMIT 1",
                params![role_id, operation],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                },
            )
            .optional()?;
        if let Some((idempotency_key, stored_parent, stored_tracker_id)) = existing {
            self.connection.execute(
                "UPDATE adapter_effects SET status = 'pending', error_class = NULL, updated_at = ?1
                  WHERE idempotency_key = ?2",
                params![Utc::now().to_rfc3339(), idempotency_key],
            )?;
            return Ok(AdapterEffectContext {
                idempotency_key,
                role,
                parent_effect_key: stored_parent,
                tracker_id: stored_tracker_id,
            });
        }
        let idempotency_key = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO adapter_effects(
               idempotency_key, role_id, operation, status, parent_effect_key,
               tracker_id, created_at, updated_at
             ) VALUES (?1, ?2, ?3, 'pending', ?4, ?5, ?6, ?6)",
            params![
                idempotency_key,
                role_id,
                operation,
                parent_effect_key,
                tracker_id,
                now
            ],
        )?;
        Ok(AdapterEffectContext {
            idempotency_key,
            role,
            parent_effect_key,
            tracker_id,
        })
    }

    pub fn complete_discard_effect(
        &mut self,
        role_id: &str,
        idempotency_key: &str,
        tracker_id: i64,
        canonical_status: &str,
    ) -> Result<(), StoreError> {
        if canonical_status != "Discarded" {
            return Err(StoreError::InvalidAdapterEffect(
                "discard effect returned a non-Discarded status".to_string(),
            ));
        }
        self.complete_adapter_effect(
            role_id,
            idempotency_key,
            tracker_id,
            canonical_status,
            "dismissed",
            false,
        )
    }

    pub fn complete_undo_discard_effect(
        &mut self,
        role_id: &str,
        idempotency_key: &str,
        tracker_id: i64,
        canonical_status: &str,
    ) -> Result<(), StoreError> {
        if canonical_status != "Evaluated" {
            return Err(StoreError::InvalidAdapterEffect(
                "undo effect returned a non-Evaluated status".to_string(),
            ));
        }
        self.complete_adapter_effect(
            role_id,
            idempotency_key,
            tracker_id,
            canonical_status,
            "viewed",
            true,
        )
    }

    pub fn complete_applied_effect(
        &mut self,
        session_id: &str,
        role_id: &str,
        idempotency_key: &str,
        tracker_id: i64,
        canonical_status: &str,
    ) -> Result<(), StoreError> {
        if canonical_status != "Applied" {
            return Err(StoreError::InvalidAdapterEffect(
                "applied effect returned a non-Applied status".to_string(),
            ));
        }
        let confirmed_at = Utc::now();
        let confirmed_at_text = confirmed_at.to_rfc3339();
        let confirmation_date = confirmed_at.with_timezone(&Madrid).date_naive().to_string();
        let transaction = self.connection.transaction()?;
        let changed = transaction.execute(
            "UPDATE adapter_effects SET status = 'completed', tracker_id = ?1,
                    error_class = NULL, updated_at = ?2
              WHERE idempotency_key = ?3 AND role_id = ?4 AND status = 'pending'",
            params![tracker_id, confirmed_at_text, idempotency_key, role_id],
        )?;
        if changed != 1 {
            let already_completed_at = transaction
                .query_row(
                    "SELECT e.updated_at
                   FROM adapter_effects e
                   JOIN roles r ON r.id = e.role_id
                   JOIN browser_sessions b ON b.role_id = r.id
                  WHERE e.idempotency_key = ?1 AND e.role_id = ?2
                    AND e.operation = 'application.applied.confirm'
                    AND e.status = 'completed' AND e.tracker_id = ?3
                    AND LOWER(TRIM(COALESCE(r.canonical_status, ''))) = 'applied'
                    AND b.id = ?4 AND b.status = 'applied_recorded'",
                    params![idempotency_key, role_id, tracker_id, session_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if let Some(completed_at) = already_completed_at {
                let completed_date = DateTime::parse_from_rfc3339(&completed_at)
                    .map(|value| value.with_timezone(&Madrid).date_naive().to_string())
                    .unwrap_or(confirmation_date);
                transaction.execute(
                    "UPDATE roles SET canonical_status = 'Applied',
                                      canonical_date = COALESCE(canonical_date, ?1)
                      WHERE id = ?2
                        AND LOWER(TRIM(COALESCE(canonical_status, ''))) = 'applied'",
                    params![completed_date, role_id],
                )?;
                transaction.commit()?;
                return Ok(());
            }
            return Err(StoreError::InvalidAdapterEffect(
                "applied effect is missing or no longer pending".to_string(),
            ));
        }
        transaction.execute(
            "UPDATE roles SET canonical_tracker_id = ?1, canonical_status = 'Applied',
                    canonical_date = CASE
                      WHEN LOWER(TRIM(COALESCE(canonical_status, ''))) = 'applied'
                        THEN COALESCE(canonical_date, ?2)
                      ELSE ?2 END,
                    canonical_visibility_override = 0,
                    updated_at = ?3 WHERE id = ?4",
            params![tracker_id, confirmation_date, confirmed_at_text, role_id],
        )?;
        transaction.execute(
            "UPDATE evaluation_sync
                SET state = 'terminal', reason = 'canonical_terminal',
                    current_receipt_key = NULL, input_hash = NULL,
                    lease_expires_at = NULL, updated_at = ?1
              WHERE role_id = ?2",
            params![confirmed_at_text, role_id],
        )?;
        transaction.execute(
            "UPDATE browser_sessions SET status = 'applied_recorded', error_code = NULL,
                    updated_at = ?1 WHERE id = ?2
                    AND status IN ('review_required', 'submitted_tracking_pending')",
            params![confirmed_at_text, session_id],
        )?;
        transaction.execute(
            "INSERT INTO activity(id, kind, message, occurred_at)
             VALUES (?1, 'application', 'The user confirmed submission; career-ops recorded the canonical Applied outcome.', ?2)",
            params![Uuid::new_v4().to_string(), confirmed_at_text],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn mark_submitted_tracking_pending(&mut self, session_id: &str) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE browser_sessions SET status = 'submitted_tracking_pending',
                    error_code = 'canonical_write_failed', updated_at = ?1
              WHERE id = ?2 AND status IN ('review_required', 'submitted_tracking_pending')",
            params![Utc::now().to_rfc3339(), session_id],
        )?;
        Ok(())
    }

    fn complete_adapter_effect(
        &mut self,
        role_id: &str,
        idempotency_key: &str,
        tracker_id: i64,
        canonical_status: &str,
        review_state: &str,
        visibility_override: bool,
    ) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        let role_is_applied = transaction.query_row(
            "SELECT LOWER(TRIM(COALESCE(canonical_status, ''))) = 'applied'
               FROM roles WHERE id = ?1",
            [role_id],
            |row| row.get::<_, bool>(0),
        )?;
        if role_is_applied {
            return Err(StoreError::InvalidAdapterEffect(
                "an Applied role cannot accept another canonical outcome".to_string(),
            ));
        }
        let changed = transaction.execute(
            "UPDATE adapter_effects
                SET status = 'completed', tracker_id = ?1, error_class = NULL, updated_at = ?2
              WHERE idempotency_key = ?3 AND role_id = ?4 AND status = 'pending'",
            params![
                tracker_id,
                Utc::now().to_rfc3339(),
                idempotency_key,
                role_id
            ],
        )?;
        if changed != 1 {
            return Err(StoreError::InvalidAdapterEffect(
                "adapter effect is missing or no longer pending".to_string(),
            ));
        }
        transaction.execute(
            "UPDATE roles
                SET canonical_tracker_id = ?1, canonical_status = ?2, review_state = ?3,
                    canonical_visibility_override = ?4, updated_at = ?5
              WHERE id = ?6",
            params![
                tracker_id,
                canonical_status,
                review_state,
                i64::from(visibility_override),
                Utc::now().to_rfc3339(),
                role_id
            ],
        )?;
        transaction.execute(
            "INSERT INTO activity(id, kind, message, occurred_at) VALUES (?1, 'decision', ?2, ?3)",
            params![
                Uuid::new_v4().to_string(),
                if visibility_override {
                    "A dismissal was undone and the role returned to the review queue."
                } else {
                    "A role was recorded as Discarded in canonical career-ops history."
                },
                Utc::now().to_rfc3339()
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn fail_adapter_effect(
        &mut self,
        idempotency_key: &str,
        error_class: &str,
    ) -> Result<(), StoreError> {
        self.connection.execute(
            "UPDATE adapter_effects SET status = 'action_required', error_class = ?1, updated_at = ?2
              WHERE idempotency_key = ?3 AND status = 'pending'",
            params![error_class, Utc::now().to_rfc3339(), idempotency_key],
        )?;
        Ok(())
    }

    fn adapter_role_context(&self, role_id: &str) -> Result<AdapterRoleContext, StoreError> {
        self.connection
            .query_row(
                "SELECT company, title, location, application_url FROM roles WHERE id = ?1",
                [role_id],
                |row| {
                    Ok(AdapterRoleContext {
                        company: row.get(0)?,
                        title: row.get(1)?,
                        location: row.get(2)?,
                        application_url: row.get(3)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::InvalidAdapterEffect("role not found".to_string()))
    }

    pub fn reconcile_due_runs(
        &mut self,
        now: DateTime<Utc>,
    ) -> Result<Vec<ScheduledRun>, StoreError> {
        self.recover_expired_leases(now)?;
        let mut statement = self.connection.prepare(
            "SELECT source_id, schedule_hours, last_successful_at, execution_mode
               FROM source_schedules WHERE enabled = 1",
        )?;
        let schedules = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let mut created = Vec::new();
        for (source_id, hours, last_successful_at, execution_mode) in schedules {
            let cursor = last_successful_at
                .as_deref()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .map(|value| value.with_timezone(&Utc))
                .unwrap_or(now - Duration::days(1));
            let due = missed_nominal_times(cursor, now, &hours);
            let Some(coverage_end) = due.last().copied() else {
                continue;
            };
            let status = if execution_mode == "active" {
                "queued"
            } else {
                "action_required"
            };
            let run = ScheduledRun {
                id: Uuid::new_v4().to_string(),
                source_id: source_id.clone(),
                coverage_start: cursor.to_rfc3339(),
                coverage_end: coverage_end.to_rfc3339(),
                status: status.to_string(),
            };
            let inserted = self.connection.execute(
                "INSERT OR IGNORE INTO runs(
                   id, source_id, kind, coverage_start, coverage_end, status, error_class,
                   created_at, updated_at, dedupe_key
                 ) VALUES (?1, ?2, 'catch_up', ?3, ?4, ?5, ?6, ?7, ?7, ?8)",
                params![
                    run.id,
                    run.source_id,
                    run.coverage_start,
                    run.coverage_end,
                    status,
                    if status == "action_required" {
                        Some("source_adapter_not_configured")
                    } else {
                        None
                    },
                    now.to_rfc3339(),
                    format!("{}:{}", source_id, coverage_end.to_rfc3339())
                ],
            )?;
            if inserted > 0 {
                insert_run_steps(&self.connection, &run.id, status, &now.to_rfc3339())?;
                self.connection.execute(
                    "INSERT INTO activity(id, kind, message, occurred_at) VALUES (?1, 'schedule', ?2, ?3)",
                    params![
                        Uuid::new_v4().to_string(),
                        if status == "queued" {
                            format!("Queued one consolidated catch-up for {source_id}.")
                        } else {
                            format!(
                                "Preserved a missed window for {source_id}; execution remains with the existing external workflow."
                            )
                        },
                        now.to_rfc3339()
                    ],
                )?;
                created.push(run);
            }
        }
        Ok(created)
    }

    pub fn queue_manual_run(&mut self, source_id: &str) -> Result<ScheduledRun, StoreError> {
        let execution_mode = self
            .connection
            .query_row(
                "SELECT execution_mode FROM source_schedules WHERE source_id = ?1 AND enabled = 1",
                [source_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| StoreError::UnknownSource(source_id.to_string()))?;
        if execution_mode != "active" {
            return Err(StoreError::SourceNotReady(source_id.to_string()));
        }
        let now = Utc::now();
        let run = ScheduledRun {
            id: Uuid::new_v4().to_string(),
            source_id: source_id.to_string(),
            coverage_start: now.to_rfc3339(),
            coverage_end: now.to_rfc3339(),
            status: "queued".to_string(),
        };
        self.connection.execute(
            "INSERT INTO runs(id, source_id, kind, coverage_start, coverage_end, status, created_at, updated_at, dedupe_key)
             VALUES (?1, ?2, 'manual', ?3, ?4, 'queued', ?5, ?5, ?6)",
            params![
                run.id,
                run.source_id,
                run.coverage_start,
                run.coverage_end,
                now.to_rfc3339(),
                format!("manual:{}", run.id)
            ],
        )?;
        insert_run_steps(&self.connection, &run.id, "queued", &now.to_rfc3339())?;
        Ok(run)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_source_execution_mode(
        &mut self,
        source_id: &str,
        execution_mode: &str,
    ) -> Result<(), StoreError> {
        if !matches!(execution_mode, "staged" | "active") {
            return Err(StoreError::InvalidRunTransition(format!(
                "unsupported source execution mode {execution_mode}"
            )));
        }
        let changed = self.connection.execute(
            "UPDATE source_schedules SET execution_mode = ?1 WHERE source_id = ?2",
            params![execution_mode, source_id],
        )?;
        if changed == 0 {
            return Err(StoreError::UnknownSource(source_id.to_string()));
        }
        if execution_mode == "active" {
            let now = Utc::now().to_rfc3339();
            self.connection.execute(
                "UPDATE runs SET status = 'queued', error_class = NULL, updated_at = ?1
                  WHERE source_id = ?2 AND status = 'action_required'
                    AND error_class = 'source_adapter_not_configured'",
                params![now, source_id],
            )?;
            self.connection.execute(
                "UPDATE run_steps SET status = 'pending', error_class = NULL, updated_at = ?1
                  WHERE name = 'discover' AND status = 'action_required'
                    AND run_id IN (SELECT id FROM runs WHERE source_id = ?2 AND status = 'queued')",
                params![now, source_id],
            )?;
        }
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn claim_next_run(
        &mut self,
        now: DateTime<Utc>,
        lease_duration: Duration,
    ) -> Result<Option<LeasedRun>, StoreError> {
        self.recover_expired_leases(now)?;
        let transaction = self.connection.transaction()?;
        let candidate = transaction
            .query_row(
                "SELECT r.id, r.source_id, r.coverage_start, r.coverage_end, r.attempt
                   FROM runs r
                   JOIN source_schedules s ON s.source_id = r.source_id
                  WHERE r.status IN ('queued', 'retryable')
                    AND s.execution_mode = 'active'
                    AND EXISTS (
                      SELECT 1 FROM run_steps rs
                       WHERE rs.run_id = r.id
                         AND rs.status IN ('pending', 'retryable')
                         AND rs.attempt < ?1
                    )
                  ORDER BY r.created_at, r.id
                  LIMIT 1",
                [MAX_RUN_ATTEMPTS],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, source_id, coverage_start, coverage_end, attempt)) = candidate else {
            transaction.commit()?;
            return Ok(None);
        };
        let step_name = transaction
            .query_row(
                "SELECT name FROM run_steps
                  WHERE run_id = ?1 AND status IN ('pending', 'retryable')
                  ORDER BY CASE name WHEN 'discover' THEN 0 WHEN 'reconcile' THEN 1 ELSE 2 END
                  LIMIT 1",
                [&id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::InvalidRunTransition(format!("run {id} has no claimable step"))
            })?;
        let next_attempt = attempt + 1;
        let lease_expires_at = now + lease_duration;
        transaction.execute(
            "UPDATE runs SET status = 'running', attempt = ?1, lease_expires_at = ?2,
                    error_class = NULL, updated_at = ?3 WHERE id = ?4",
            params![
                next_attempt,
                lease_expires_at.to_rfc3339(),
                now.to_rfc3339(),
                id
            ],
        )?;
        transaction.execute(
            "UPDATE run_steps SET status = 'running', attempt = attempt + 1,
                    error_class = NULL, updated_at = ?1 WHERE run_id = ?2 AND name = ?3",
            params![now.to_rfc3339(), id, step_name],
        )?;
        transaction.commit()?;
        Ok(Some(LeasedRun {
            id,
            source_id,
            coverage_start,
            coverage_end,
            attempt: next_attempt,
            step_name,
            lease_expires_at: lease_expires_at.to_rfc3339(),
        }))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn complete_run_step(
        &mut self,
        run_id: &str,
        step_name: &str,
        input_hash: Option<&str>,
        output_hash: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        let changed = transaction.execute(
            "UPDATE run_steps SET status = 'completed', input_hash = ?1, output_hash = ?2,
                    error_class = NULL, updated_at = ?3
              WHERE run_id = ?4 AND name = ?5 AND status = 'running'",
            params![input_hash, output_hash, now.to_rfc3339(), run_id, step_name],
        )?;
        if changed == 0 {
            return Err(StoreError::InvalidRunTransition(format!(
                "step {step_name} on run {run_id} is not running"
            )));
        }
        let next_step = match step_name {
            "discover" => Some("reconcile"),
            "reconcile" => Some("notify"),
            "notify" => None,
            _ => {
                return Err(StoreError::InvalidRunTransition(format!(
                    "unknown run step {step_name}"
                )));
            }
        };
        if let Some(next_step) = next_step {
            transaction.execute(
                "UPDATE run_steps SET status = 'pending', error_class = NULL, updated_at = ?1
                  WHERE run_id = ?2 AND name = ?3 AND status = 'blocked'",
                params![now.to_rfc3339(), run_id, next_step],
            )?;
            transaction.execute(
                "UPDATE runs SET status = 'queued', lease_expires_at = NULL, updated_at = ?1
                  WHERE id = ?2",
                params![now.to_rfc3339(), run_id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn fail_run_step(
        &mut self,
        run_id: &str,
        step_name: &str,
        requested_class: &str,
        now: DateTime<Utc>,
    ) -> Result<String, StoreError> {
        if !matches!(
            requested_class,
            "retryable" | "action_required" | "permanent"
        ) {
            return Err(StoreError::InvalidRunTransition(format!(
                "unsupported failure class {requested_class}"
            )));
        }
        let transaction = self.connection.transaction()?;
        let step_attempt = transaction.query_row(
            "SELECT rs.attempt FROM run_steps rs
               JOIN runs r ON r.id = rs.run_id
              WHERE rs.run_id = ?1 AND rs.name = ?2
                AND rs.status = 'running' AND r.status = 'running'",
            params![run_id, step_name],
            |row| row.get::<_, i64>(0),
        )?;
        let effective_class = if requested_class == "retryable" && step_attempt >= MAX_RUN_ATTEMPTS
        {
            "permanent"
        } else {
            requested_class
        };
        let changed = transaction.execute(
            "UPDATE run_steps SET status = ?1, error_class = ?1, updated_at = ?2
              WHERE run_id = ?3 AND name = ?4 AND status = 'running'",
            params![effective_class, now.to_rfc3339(), run_id, step_name],
        )?;
        if changed == 0 {
            return Err(StoreError::InvalidRunTransition(format!(
                "step {step_name} on run {run_id} is not running"
            )));
        }
        transaction.execute(
            "UPDATE runs SET status = ?1, error_class = ?1, lease_expires_at = NULL,
                    updated_at = ?2 WHERE id = ?3",
            params![effective_class, now.to_rfc3339(), run_id],
        )?;
        transaction.commit()?;
        Ok(effective_class.to_string())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn complete_run(
        &mut self,
        run_id: &str,
        new_viable_roles: usize,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        let (source_id, coverage_end, incomplete_steps) = transaction.query_row(
            "SELECT r.source_id, r.coverage_end,
                    (SELECT COUNT(*) FROM run_steps rs WHERE rs.run_id = r.id AND rs.status != 'completed')
               FROM runs r WHERE r.id = ?1",
            [run_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )?;
        if incomplete_steps != 0 {
            return Err(StoreError::InvalidRunTransition(format!(
                "run {run_id} still has {incomplete_steps} incomplete steps"
            )));
        }
        transaction.execute(
            "UPDATE runs SET status = 'completed', error_class = NULL, lease_expires_at = NULL,
                    updated_at = ?1 WHERE id = ?2",
            params![now.to_rfc3339(), run_id],
        )?;
        transaction.execute(
            "UPDATE source_schedules SET last_successful_at = ?1 WHERE source_id = ?2",
            params![coverage_end, source_id],
        )?;
        if new_viable_roles > 0 {
            transaction.execute(
                "INSERT OR IGNORE INTO notification_outbox(
                   id, dedupe_key, title, body, status, attempts, next_attempt_at, created_at
                 ) VALUES (?1, ?2, 'New roles to review', ?3, 'pending', 0, ?4, ?4)",
                params![
                    Uuid::new_v4().to_string(),
                    format!("discovery-completed:{run_id}"),
                    format!("{new_viable_roles} viable role(s) are ready for review."),
                    now.to_rfc3339()
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn expire_undelivered_outcome_notifications(&mut self) -> Result<usize, StoreError> {
        self.connection
            .execute(
                "UPDATE notification_outbox
                    SET status = 'expired', last_error = 'app_restarted_before_delivery'
                  WHERE status IN ('pending', 'delivering')
                    AND event_kind IN ('preparation_failed', 'application_ready')",
                [],
            )
            .map_err(StoreError::from)
    }

    pub fn take_in_app_outcome_notifications(
        &mut self,
        limit: usize,
    ) -> Result<Vec<OutcomeNotification>, StoreError> {
        let transaction = self.connection.transaction()?;
        let mut statement = transaction.prepare(
            "SELECT id, event_kind, title, body, action_kind, action_label,
                    role_id, preparation_id, browser_session_id, created_at
               FROM notification_outbox
              WHERE status = 'pending'
                AND event_kind IN ('preparation_failed', 'application_ready')
              ORDER BY created_at, rowid LIMIT ?1",
        )?;
        let notifications = statement
            .query_map([limit.clamp(1, 20) as i64], outcome_notification_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let now = Utc::now().to_rfc3339();
        for notification in &notifications {
            transaction.execute(
                "UPDATE notification_outbox
                    SET status = 'delivered', delivered_via = 'in_app', delivered_at = ?1,
                        attempts = attempts + 1, last_error = NULL
                  WHERE id = ?2 AND status = 'pending'",
                params![now, notification.id],
            )?;
        }
        transaction.commit()?;
        Ok(notifications)
    }

    pub fn claim_native_outcome_notification(
        &mut self,
    ) -> Result<Option<OutcomeNotification>, StoreError> {
        let transaction = self.connection.transaction()?;
        let notification = transaction
            .query_row(
                "SELECT id, event_kind, title, body, action_kind, action_label,
                        role_id, preparation_id, browser_session_id, created_at
                   FROM notification_outbox
                  WHERE status = 'pending'
                    AND event_kind IN ('preparation_failed', 'application_ready')
                  ORDER BY created_at, rowid LIMIT 1",
                [],
                outcome_notification_from_row,
            )
            .optional()?;
        let Some(notification) = notification else {
            transaction.commit()?;
            return Ok(None);
        };
        let changed = transaction.execute(
            "UPDATE notification_outbox
                SET status = 'delivering', delivered_via = 'native', attempts = attempts + 1
              WHERE id = ?1 AND status = 'pending'",
            [&notification.id],
        )?;
        transaction.commit()?;
        Ok((changed == 1).then_some(notification))
    }

    pub fn finish_native_outcome_notification(
        &mut self,
        notification_id: &str,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        let now = Utc::now().to_rfc3339();
        let (status, delivered_at, last_error) = match error {
            Some(error) => ("failed", None, Some(sanitize_preparation_error(error))),
            None => ("delivered", Some(now.clone()), None),
        };
        self.connection.execute(
            "UPDATE notification_outbox
                SET status = ?1, delivered_at = ?2, last_error = ?3
              WHERE id = ?4 AND status = 'delivering'",
            params![status, delivered_at, last_error, notification_id],
        )?;
        Ok(())
    }

    fn recover_expired_leases(&mut self, now: DateTime<Utc>) -> Result<usize, StoreError> {
        let transaction = self.connection.transaction()?;
        let recovered_steps = transaction.execute(
            "UPDATE run_steps SET status = 'retryable', error_class = 'lease_expired', updated_at = ?1
              WHERE status = 'running' AND run_id IN (
                SELECT id FROM runs WHERE status = 'running' AND lease_expires_at < ?1
              )",
            [now.to_rfc3339()],
        )?;
        transaction.execute(
            "UPDATE runs
                SET status = CASE
                      WHEN EXISTS (
                        SELECT 1 FROM run_steps rs
                         WHERE rs.run_id = runs.id AND rs.status = 'retryable'
                           AND rs.attempt >= ?1
                      ) THEN 'permanent'
                      ELSE 'retryable'
                    END,
                    error_class = 'lease_expired', lease_expires_at = NULL, updated_at = ?2
              WHERE status = 'running' AND lease_expires_at < ?2",
            params![MAX_RUN_ATTEMPTS, now.to_rfc3339()],
        )?;
        transaction.commit()?;
        Ok(recovered_steps)
    }

    pub fn dashboard(&self) -> Result<DashboardState, StoreError> {
        let queue_filters = self.queue_filters()?;
        let mut roles_statement = self.connection.prepare(
            "SELECT
               r.id, r.company, r.title, r.location,
               COALESCE(MIN(s.source), 'Unknown'), COUNT(s.id),
               CASE WHEN evaluation.state = 'needs_decision' THEN 'needs_decision'
                    ELSE r.queue_group END,
               r.eligibility_summary, r.uncertainty,
               CASE WHEN COUNT(DISTINCT s.posted_at) = 1 THEN MIN(s.posted_at) ELSE NULL END,
               r.discovered_at, r.application_url, r.preparation_state,
               r.canonical_tracker_id, r.canonical_status,
               receipt.native_score, receipt.legitimacy, receipt.risk_level,
               receipt.strengths_json, receipt.blockers_json,
               receipt.gaps_json, receipt.compensation,
               receipt.authorization_confidence, receipt.authorization_question,
               receipt.material_uncertainty_json
             FROM roles r
             LEFT JOIN source_occurrences s ON s.role_id = r.id
             JOIN evaluation_sync evaluation ON evaluation.role_id = r.id
             JOIN evaluation_receipts receipt
               ON receipt.receipt_key = evaluation.current_receipt_key
             WHERE evaluation.state IN ('ready', 'needs_decision')
               AND COALESCE(r.canonical_status, '') NOT IN ('Applied', 'Discarded', 'Rejected')
               AND COALESCE(r.legitimacy, '') <> 'suspicious'
               AND r.preparation_state = 'not_started'
             GROUP BY r.id
             ORDER BY
               CASE r.queue_group
                 WHEN 'strong_match' THEN 0
                 WHEN 'other_new' THEN 1
                 ELSE 2
               END,
               r.discovered_at DESC,
               r.company COLLATE NOCASE",
        )?;
        let mut roles = roles_statement
            .query_map([], |row| {
                let queue_group: String = row.get(6)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    queue_group,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, Option<i64>>(13)?,
                    row.get::<_, Option<String>>(14)?,
                    row.get::<_, Option<f64>>(15)?,
                    row.get::<_, Option<String>>(16)?,
                    row.get::<_, Option<String>>(17)?,
                    row.get::<_, Option<String>>(18)?,
                    row.get::<_, Option<String>>(19)?,
                    row.get::<_, Option<String>>(20)?,
                    row.get::<_, Option<String>>(21)?,
                    row.get::<_, Option<String>>(22)?,
                    row.get::<_, Option<String>>(23)?,
                    row.get::<_, Option<String>>(24)?,
                ))
            })?
            .map(role_summary_from_tuple)
            .collect::<Result<Vec<_>, StoreError>>()?;
        roles.retain(|role| role_matches_queue_filters(role, &queue_filters));

        let mut dismissed_statement = self.connection.prepare(
            "SELECT
               r.id, r.company, r.title, r.location,
               COALESCE(MIN(s.source), 'Unknown'), COUNT(s.id),
               r.queue_group, r.eligibility_summary, r.uncertainty,
               CASE WHEN COUNT(DISTINCT s.posted_at) = 1 THEN MIN(s.posted_at) ELSE NULL END,
               r.discovered_at, r.application_url, r.preparation_state,
               r.canonical_tracker_id, r.canonical_status,
               receipt.native_score, receipt.legitimacy, receipt.risk_level,
               receipt.strengths_json, receipt.blockers_json,
               receipt.gaps_json, receipt.compensation,
               receipt.authorization_confidence, receipt.authorization_question,
               receipt.material_uncertainty_json
             FROM roles r
             LEFT JOIN source_occurrences s ON s.role_id = r.id
             LEFT JOIN evaluation_sync evaluation ON evaluation.role_id = r.id
             LEFT JOIN evaluation_receipts receipt
               ON receipt.receipt_key = evaluation.current_receipt_key
             WHERE r.review_state = 'dismissed' AND r.canonical_status = 'Discarded'
             GROUP BY r.id
             ORDER BY r.updated_at DESC LIMIT 5",
        )?;
        let recently_dismissed = dismissed_statement
            .query_map([], |row| {
                let queue_group: String = row.get(6)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    queue_group,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, Option<i64>>(13)?,
                    row.get::<_, Option<String>>(14)?,
                    row.get::<_, Option<f64>>(15)?,
                    row.get::<_, Option<String>>(16)?,
                    row.get::<_, Option<String>>(17)?,
                    row.get::<_, Option<String>>(18)?,
                    row.get::<_, Option<String>>(19)?,
                    row.get::<_, Option<String>>(20)?,
                    row.get::<_, Option<String>>(21)?,
                    row.get::<_, Option<String>>(22)?,
                    row.get::<_, Option<String>>(23)?,
                    row.get::<_, Option<String>>(24)?,
                ))
            })?
            .map(role_summary_from_tuple)
            .collect::<Result<Vec<_>, StoreError>>()?;

        let mut preparation_statement = self.connection.prepare(
            "SELECT p.id, p.role_id, r.company, r.title, p.provider, p.status, p.step,
                    p.attempt, p.report_path, p.cv_pdf_path, p.cv_source, p.error_class,
                    p.error_stage, p.error_detail, p.retry_policy, p.updated_at,
                    EXISTS (
                      SELECT 1 FROM adapter_effects e
                       WHERE e.role_id = p.role_id
                         AND e.operation = 'application.applied.confirm'
                         AND e.status IN ('pending', 'action_required')
                    ) AS applied_tracking_pending
               FROM preparation_jobs p
               JOIN roles r ON r.id = p.role_id
              WHERE p.status != 'cancelled'
                AND p.id = (
                  SELECT latest.id FROM preparation_jobs latest
                   WHERE latest.role_id = p.role_id AND latest.status != 'cancelled'
                   ORDER BY latest.updated_at DESC, latest.id DESC LIMIT 1
                )
              ORDER BY p.updated_at DESC LIMIT 30",
        )?;
        let preparations = preparation_statement
            .query_map([], |row| {
                Ok(PreparationSummary {
                    id: row.get(0)?,
                    role_id: row.get(1)?,
                    company: row.get(2)?,
                    title: row.get(3)?,
                    provider: row.get(4)?,
                    status: row.get(5)?,
                    step: row.get(6)?,
                    attempt: row.get(7)?,
                    report_path: row.get(8)?,
                    cv_pdf_path: row.get(9)?,
                    cv_source: row.get(10)?,
                    error_class: row.get(11)?,
                    error_stage: row.get(12)?,
                    error_detail: row.get(13)?,
                    retry_policy: row.get(14)?,
                    updated_at: row.get(15)?,
                    applied_tracking_pending: row.get(16)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut activity_statement = self.connection.prepare(
            "SELECT id, kind, message, occurred_at FROM activity ORDER BY occurred_at DESC LIMIT 30",
        )?;
        let activity = activity_statement
            .query_map([], |row| {
                Ok(ActivityEntry {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    message: row.get(2)?,
                    occurred_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let last_successful_discovery_at =
            setting(&self.connection, "last_successful_discovery_at")?;
        let background_enabled =
            setting(&self.connection, "background_enabled")?.as_deref() == Some("true");
        let adapter_ready = setting(&self.connection, "adapter_ready")?.as_deref() == Some("true");
        let handled_count = self.connection.query_row(
            "SELECT COUNT(*) FROM roles r
               LEFT JOIN evaluation_sync evaluation ON evaluation.role_id = r.id
              WHERE r.canonical_status IN ('Applied', 'Discarded', 'Rejected')
                 OR evaluation.state = 'hidden'",
            [],
            |row| row.get(0),
        )?;
        let mut pre_queue_statement = self.connection.prepare(
            "SELECT r.id, r.company, r.title, evaluation.state, evaluation.reason,
                    evaluation.attempt, evaluation.updated_at
               FROM evaluation_sync evaluation
               JOIN roles r ON r.id = evaluation.role_id
              WHERE evaluation.state IN ('awaiting_evaluation', 'needs_attention', 'syncing')
                AND COALESCE(r.canonical_status, '') NOT IN ('Applied', 'Discarded', 'Rejected')
              ORDER BY evaluation.updated_at DESC, r.company COLLATE NOCASE",
        )?;
        let pre_queue_roles = pre_queue_statement
            .query_map([], |row| {
                let state = row.get::<_, String>(3)?;
                let reason = row.get::<_, String>(4)?;
                Ok(PreQueueRoleSummary {
                    role_id: row.get(0)?,
                    company: row.get(1)?,
                    title: row.get(2)?,
                    state: state.clone(),
                    recovery: PreQueueRecovery::for_state_and_reason(&state, &reason),
                    reason,
                    attempt: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let pending_run_count = self.connection.query_row(
            "SELECT COUNT(*) FROM runs WHERE status IN ('queued', 'retryable', 'running')",
            [],
            |row| row.get(0),
        )?;
        let action_required_run_count = self.connection.query_row(
            "SELECT COUNT(*) FROM runs
              WHERE status = 'action_required'
                AND COALESCE(error_class, '') != 'source_adapter_not_configured'",
            [],
            |row| row.get(0),
        )?;
        let mut sources_statement = self.connection.prepare(
            "SELECT s.source_id, s.display_name, s.timezone, s.schedule_hours,
                    s.execution_mode, s.last_successful_at,
                    COUNT(CASE
                      WHEN r.status = 'action_required'
                       AND COALESCE(r.error_class, '') != 'source_adapter_not_configured'
                      THEN 1 END)
               FROM source_schedules s
               LEFT JOIN runs r ON r.source_id = s.source_id
              WHERE s.enabled = 1
              GROUP BY s.source_id
              ORDER BY s.display_name COLLATE NOCASE",
        )?;
        let sources = sources_statement
            .query_map([], |row| {
                Ok(SourceScheduleSummary {
                    source_id: row.get(0)?,
                    display_name: row.get(1)?,
                    timezone: row.get(2)?,
                    schedule_hours: row.get(3)?,
                    execution_mode: row.get(4)?,
                    last_successful_at: row.get(5)?,
                    action_required_count: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut runs_statement = self.connection.prepare(
            "SELECT id, source_id, kind, coverage_start, coverage_end, status, attempt,
                    error_class, updated_at
               FROM runs ORDER BY updated_at DESC, created_at DESC LIMIT 12",
        )?;
        let recent_runs = runs_statement
            .query_map([], |row| {
                Ok(RunSummary {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    kind: row.get(2)?,
                    coverage_start: row.get(3)?,
                    coverage_end: row.get(4)?,
                    status: row.get(5)?,
                    attempt: row.get(6)?,
                    error_class: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let discovery_runs = self.discovery_run_diagnostics()?;
        let discovery_cursors = self.discovery_cursors()?;

        Ok(DashboardState {
            roles,
            pre_queue_roles,
            recently_dismissed,
            preparations,
            activity,
            queue_filters,
            last_successful_discovery_at,
            background_enabled,
            adapter_status: if adapter_ready {
                "ready"
            } else {
                "not_configured"
            }
            .to_string(),
            handled_count,
            pending_run_count,
            action_required_run_count,
            sources,
            recent_runs,
            discovery_runs,
            discovery_cursors,
        })
    }

    pub fn discovery_cursors(&self) -> Result<Vec<DiscoveryCursor>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT source_id, window_id, run_id, coverage_end, updated_at
               FROM discovery_cursors ORDER BY source_id",
        )?;
        statement
            .query_map([], |row| {
                Ok(DiscoveryCursor {
                    source_id: row.get(0)?,
                    window_id: row.get(1)?,
                    run_id: row.get(2)?,
                    coverage_end: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn discovery_run_diagnostics(&self) -> Result<Vec<DiscoveryRunDiagnostic>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT diagnostics_json FROM discovery_runs
               WHERE diagnostics_json != '{}'
               ORDER BY imported_at DESC LIMIT 30",
        )?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|value| {
                let value = value?;
                if value.len() > MAX_DISCOVERY_DIAGNOSTICS_BYTES {
                    return Err(StoreError::InvalidDiscoveryRun(
                        "stored discovery diagnostics exceed the maximum size".to_string(),
                    ));
                }
                serde_json::from_str(&value)
                    .map_err(|error| StoreError::InvalidDiscoveryRun(error.to_string()))
            })
            .collect()
    }
}

fn application_form_url(value: &str) -> Result<String, StoreError> {
    let mut url = url::Url::parse(value).map_err(|_| {
        StoreError::InvalidBrowserTransition("the role application URL is invalid".to_string())
    })?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || !is_public_application_host(&url)
    {
        return Err(StoreError::InvalidBrowserTransition(
            "the role application URL must use public HTTPS without embedded credentials"
                .to_string(),
        ));
    }
    url.set_fragment(None);
    if url.host_str() == Some("jobs.ashbyhq.com") {
        let path = url.path().trim_end_matches('/').to_string();
        if !path.ends_with("/application") {
            url.set_path(&format!("{path}/application"));
        }
    }
    Ok(url.to_string())
}

fn is_public_application_host(url: &url::Url) -> bool {
    let Some(hostname) = url.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };
    if hostname == "localhost" || hostname.ends_with(".localhost") || hostname.ends_with(".local") {
        return false;
    }
    match url.host() {
        Some(url::Host::Ipv4(ip)) => {
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_multicast()
        }
        Some(url::Host::Ipv6(ip)) => {
            !ip.is_loopback()
                && !ip.is_unique_local()
                && !ip.is_unicast_link_local()
                && !ip.is_unspecified()
                && !ip.is_multicast()
        }
        Some(url::Host::Domain(_)) => true,
        None => false,
    }
}

fn application_pages_match(expected: &str, inspected: &str) -> bool {
    let (Ok(mut expected), Ok(mut inspected)) =
        (url::Url::parse(expected), url::Url::parse(inspected))
    else {
        return false;
    };
    expected.set_fragment(None);
    expected.set_query(None);
    inspected.set_fragment(None);
    inspected.set_query(None);
    let expected_path = expected.path().trim_end_matches('/');
    let inspected_path = inspected.path().trim_end_matches('/');
    expected.scheme() == inspected.scheme()
        && expected.host_str() == inspected.host_str()
        && expected.port_or_known_default() == inspected.port_or_known_default()
        && expected_path == inspected_path
}

fn backup_before_migration(connection: &Connection, path: &Path) -> Result<(), StoreError> {
    let has_schema_meta = connection.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_meta'
         )",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if !has_schema_meta {
        return Ok(());
    }
    let version = connection.query_row("SELECT version FROM schema_meta LIMIT 1", [], |row| {
        row.get::<_, i64>(0)
    })?;
    if version >= SCHEMA_VERSION {
        return Ok(());
    }
    let backup_directory = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("migration-backups");
    std::fs::create_dir_all(&backup_directory)?;
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("here-for-work");
    let timestamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let backup_path: PathBuf = backup_directory.join(format!(
        "{stem}-pre-v{SCHEMA_VERSION}-from-v{version}-{timestamp}.sqlite3"
    ));
    connection.backup(rusqlite::MAIN_DB, backup_path, None)?;
    Ok(())
}

fn insert_run_steps(
    connection: &Connection,
    run_id: &str,
    run_status: &str,
    now: &str,
) -> Result<(), StoreError> {
    for (index, step_name) in RUN_STEPS.iter().enumerate() {
        let status = if index == 0 {
            if run_status == "action_required" {
                "action_required"
            } else {
                "pending"
            }
        } else {
            "blocked"
        };
        let error_class = if run_status == "action_required" {
            Some("source_adapter_not_configured")
        } else {
            None
        };
        connection.execute(
            "INSERT OR IGNORE INTO run_steps(
               id, run_id, name, status, attempt, error_class, updated_at
             ) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
            params![
                Uuid::new_v4().to_string(),
                run_id,
                step_name,
                status,
                error_class,
                now
            ],
        )?;
    }
    Ok(())
}

fn discovery_run_finding_to_legacy(finding: &DiscoveryRunFinding) -> DiscoveryFinding {
    DiscoveryFinding {
        source_id: finding.source_id.clone(),
        source: finding.source.clone(),
        source_role_id: finding.source_role_id.clone(),
        company: finding.company.clone(),
        title: finding.title.clone(),
        location: finding.location.clone(),
        discovered_at: finding.discovered_at.clone(),
        posted_at: finding.posted_at.clone(),
        application_url: finding.application_url.clone(),
        normalized_key: finding.normalized_key.clone(),
        queue_group: finding.queue_group.clone(),
        eligibility_summary: finding.eligibility_summary.clone(),
        uncertainty: finding.uncertainty.clone(),
        legitimacy: finding.legitimacy.clone(),
    }
}

fn valid_identifier_with_bounds(value: &str, minimum: usize, maximum: usize) -> bool {
    value.chars().count() >= minimum
        && value.chars().count() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn valid_text(value: &str, minimum: usize, maximum: usize) -> bool {
    let length = value.len();
    (minimum..=maximum).contains(&length)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_https(value: Option<&str>) -> bool {
    value
        .map(|value| {
            valid_text(value, 1, 2_000)
                && url::Url::parse(value)
                    .map(|parsed| parsed.scheme() == "https" && parsed.host_str().is_some())
                    .unwrap_or(false)
        })
        .unwrap_or(true)
}

fn valid_datetime(value: &str) -> bool {
    // Keep this deliberately narrower than chrono's RFC3339 parser. It mirrors
    // the executable contract: ASCII extended ISO date-time, uppercase T/Z,
    // an optional 1-9 digit fraction, and an explicit timezone.
    if value.len() > 100 || !value.is_ascii() {
        return false;
    }
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..10].iter().all(u8::is_ascii_digit)
        || !bytes[11..13].iter().all(u8::is_ascii_digit)
        || !bytes[14..16].iter().all(u8::is_ascii_digit)
        || !bytes[17..19].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    if NaiveDate::parse_from_str(&value[..10], "%Y-%m-%d").is_err() {
        return false;
    }
    let hours = value[11..13].parse::<u32>().ok();
    let minutes = value[14..16].parse::<u32>().ok();
    let seconds = value[17..19].parse::<u32>().ok();
    let (Some(hours), Some(minutes), Some(seconds)) = (hours, minutes, seconds) else {
        return false;
    };
    if NaiveTime::from_hms_opt(hours, minutes, seconds).is_none() {
        return false;
    }

    let mut timezone_start = 19;
    if bytes.get(timezone_start) == Some(&b'.') {
        timezone_start += 1;
        let fraction_start = timezone_start;
        while bytes
            .get(timezone_start)
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            timezone_start += 1;
        }
        if !(1..=9).contains(&(timezone_start - fraction_start)) {
            return false;
        }
    }
    let timezone = &bytes[timezone_start..];
    let valid_timezone = timezone == b"Z"
        || (timezone.len() == 6
            && matches!(timezone[0], b'+' | b'-')
            && timezone[3] == b':'
            && timezone[1..3].iter().all(|byte| byte.is_ascii_digit())
            && timezone[4..6].iter().all(|byte| byte.is_ascii_digit()));
    if !valid_timezone {
        return false;
    }
    if timezone != b"Z" {
        let offset_hours = (timezone[1] - b'0') as u32 * 10 + (timezone[2] - b'0') as u32;
        let offset_minutes = (timezone[4] - b'0') as u32 * 10 + (timezone[5] - b'0') as u32;
        if offset_hours > 23 || offset_minutes > 59 {
            return false;
        }
    }
    DateTime::parse_from_rfc3339(value).is_ok()
}

fn valid_date_or_datetime(value: &str) -> bool {
    (value.len() == 10 && NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok())
        || valid_datetime(value)
}

fn validate_discovery_run_raw_bounds(raw: &serde_json::Value) -> Result<(), StoreError> {
    let object = raw.as_object().ok_or_else(|| {
        StoreError::InvalidDiscoveryRun("run must serialize as an object".to_string())
    })?;
    let findings = object
        .get("findings")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| StoreError::InvalidDiscoveryRun("findings must be an array".to_string()))?;
    if findings.len() > MAX_DISCOVERY_FINDINGS {
        return Err(StoreError::InvalidDiscoveryRun(
            "discovery run exceeds the bounded findings, evidence, or issue limits".to_string(),
        ));
    }
    let issues = object
        .get("issues")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| StoreError::InvalidDiscoveryRun("issues must be an array".to_string()))?;
    if issues.len() > MAX_DISCOVERY_ISSUES {
        return Err(StoreError::InvalidDiscoveryRun(
            "discovery run exceeds the bounded findings, evidence, or issue limits".to_string(),
        ));
    }
    for finding in findings {
        if let Some(evidence) = finding
            .as_object()
            .and_then(|value| value.get("evidence"))
            .and_then(serde_json::Value::as_array)
        {
            if evidence.len() > MAX_DISCOVERY_EVIDENCE_PER_FINDING {
                return Err(StoreError::InvalidDiscoveryRun(
                    "discovery run exceeds the bounded findings, evidence, or issue limits"
                        .to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_discovery_run(run: &DiscoveryRun) -> Result<(), StoreError> {
    if run.findings.len() > MAX_DISCOVERY_FINDINGS
        || run.issues.len() > MAX_DISCOVERY_ISSUES
        || run
            .findings
            .iter()
            .any(|finding| finding.evidence.len() > MAX_DISCOVERY_EVIDENCE_PER_FINDING)
    {
        return Err(StoreError::InvalidDiscoveryRun(
            "discovery run exceeds the bounded findings, evidence, or issue limits".to_string(),
        ));
    }
    if run.contract != "hereforwork.discovery-run" || run.schema_version != 1 {
        return Err(StoreError::InvalidDiscoveryRun(
            "unsupported contract or schemaVersion".to_string(),
        ));
    }
    if !valid_identifier_with_bounds(&run.window_id, 8, 128)
        || !valid_identifier_with_bounds(&run.run_id, 8, 128)
    {
        return Err(StoreError::InvalidDiscoveryRun(
            "windowId and runId must be valid identifiers".to_string(),
        ));
    }
    if run
        .supersedes_run_id
        .as_deref()
        .is_some_and(|value| !valid_identifier_with_bounds(value, 8, 128) || value == run.run_id)
    {
        return Err(StoreError::InvalidDiscoveryRun(
            "supersedesRunId must be a different valid identifier".to_string(),
        ));
    }
    if !valid_identifier_with_bounds(&run.source.source_id, 1, 100)
        || !valid_text(&run.source.display_name, 1, 200)
        || !valid_text(&run.source.producer, 1, 100)
        || !valid_text(&run.source.producer_version, 1, 200)
        || !valid_text(&run.coverage.timezone, 1, 100)
    {
        return Err(StoreError::InvalidDiscoveryRun(
            "source and coverage metadata must not be empty".to_string(),
        ));
    }
    if !valid_identifier_with_bounds(&run.source.source_id, 1, 100)
        || !valid_datetime(&run.coverage.window_start)
        || !valid_datetime(&run.coverage.window_end)
        || !valid_datetime(&run.generated_at)
    {
        return Err(StoreError::InvalidDiscoveryRun(
            "sourceId and coverage timestamps are invalid".to_string(),
        ));
    }
    let start = DateTime::parse_from_rfc3339(&run.coverage.window_start)
        .map_err(|_| StoreError::InvalidDiscoveryRun("invalid windowStart".to_string()))?;
    let end = DateTime::parse_from_rfc3339(&run.coverage.window_end)
        .map_err(|_| StoreError::InvalidDiscoveryRun("invalid windowEnd".to_string()))?;
    let generated = DateTime::parse_from_rfc3339(&run.generated_at)
        .map_err(|_| StoreError::InvalidDiscoveryRun("invalid generatedAt".to_string()))?;
    if start > end || generated < end {
        return Err(StoreError::InvalidDiscoveryRun(
            "coverage timestamps are not chronologically valid".to_string(),
        ));
    }
    if !matches!(run.status.as_str(), "completed" | "partial" | "failed")
        || (run.status != "completed" && run.issues.is_empty())
        || (run.status == "failed" && !run.findings.is_empty())
    {
        return Err(StoreError::InvalidDiscoveryRun(
            "status, issues, and findings do not satisfy the contract".to_string(),
        ));
    }
    if run.integrity.algorithm != "sha256"
        || run.integrity.canonicalization != "hfw-discovery-run-v1"
        || run.integrity.coverage != "all_top_level_fields_except_integrity"
        || !valid_sha256(&run.integrity.digest)
    {
        return Err(StoreError::InvalidDiscoveryRun(
            "integrity metadata is invalid".to_string(),
        ));
    }

    let mut finding_ids = HashSet::new();
    let mut source_role_ids = HashSet::new();
    for finding in &run.findings {
        if !finding_ids.insert(finding.finding_id.clone())
            || !source_role_ids.insert(finding.source_role_id.clone())
            || !valid_identifier_with_bounds(&finding.finding_id, 1, 200)
            || !valid_identifier_with_bounds(&finding.source_id, 1, 100)
            || finding.source_id != run.source.source_id
            || finding.source != run.source.display_name
            || !valid_text(&finding.source, 1, 200)
            || !valid_text(&finding.company, 1, 500)
            || !valid_text(&finding.title, 1, 500)
            || !valid_text(&finding.location, 1, 500)
            || !valid_text(&finding.source_role_id, 1, 500)
            || !valid_text(&finding.normalized_key, 3, 500)
            || !valid_text(&finding.eligibility_summary, 1, 2000)
            || finding
                .uncertainty
                .as_deref()
                .is_some_and(|value| !valid_text(value, 1, 2000))
            || !valid_datetime(&finding.discovered_at)
            || finding
                .posted_at
                .as_deref()
                .is_some_and(|value| !valid_date_or_datetime(value))
            || !valid_https(finding.application_url.as_deref())
        {
            return Err(StoreError::InvalidDiscoveryRun(
                "finding identity, source, or required field is invalid".to_string(),
            ));
        }
        let mut evidence_ids = HashSet::new();
        if finding.evidence.is_empty() {
            return Err(StoreError::InvalidDiscoveryRun(
                "every finding requires evidence".to_string(),
            ));
        }
        for evidence in &finding.evidence {
            if !evidence_ids.insert(evidence.evidence_id.clone())
                || !valid_identifier_with_bounds(&evidence.evidence_id, 1, 200)
                || !matches!(
                    evidence.kind.as_str(),
                    "source_listing" | "company_page" | "authorization" | "legitimacy" | "other"
                )
                || !valid_text(&evidence.reference, 1, 2000)
                || !valid_datetime(&evidence.observed_at)
                || !valid_https(evidence.url.as_deref())
                || evidence
                    .summary
                    .as_deref()
                    .is_some_and(|value| !valid_text(value, 1, 4000))
                || evidence
                    .content_sha256
                    .as_deref()
                    .is_some_and(|value| !valid_sha256(value))
            {
                return Err(StoreError::InvalidDiscoveryRun(
                    "finding evidence is invalid or duplicated".to_string(),
                ));
            }
        }
        match &finding.match_score {
            DiscoveryRunMatchScore::Scored {
                scale,
                value,
                authority,
                source_version,
                scored_at,
            } if scale == "career_ops_1_to_5"
                && value.is_finite()
                && (1.0..=5.0).contains(value)
                && authority == "career-ops"
                && valid_text(source_version, 1, 200)
                && valid_datetime(scored_at) => {}
            DiscoveryRunMatchScore::NotScored {
                reason,
                authority,
                source_version,
                checked_at,
            } if matches!(
                reason.as_str(),
                "not_evaluated" | "unavailable" | "deferred" | "insufficient_evidence"
            ) && authority == "career-ops"
                && valid_text(source_version, 1, 200)
                && valid_datetime(checked_at) => {}
            _ => {
                return Err(StoreError::InvalidDiscoveryRun(
                    "finding matchScore is invalid".to_string(),
                ));
            }
        }
    }
    let mut issue_ids = HashSet::new();
    for issue in &run.issues {
        if !issue_ids.insert(issue.issue_id.clone())
            || !valid_identifier_with_bounds(&issue.issue_id, 1, 200)
            || !valid_text(&issue.code, 1, 200)
            || !valid_text(&issue.message, 1, 2000)
        {
            return Err(StoreError::InvalidDiscoveryRun(
                "run issue is invalid or duplicated".to_string(),
            ));
        }
    }
    Ok(())
}

fn canonical_json(value: &serde_json::Value) -> Result<String, StoreError> {
    match value {
        serde_json::Value::Array(items) => Ok(format!(
            "[{}]",
            items
                .iter()
                .map(canonical_json)
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        )),
        serde_json::Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
            Ok(format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|key| {
                        canonical_json(&object[key]).map(|value| {
                            format!(
                                "{}:{}",
                                serde_json::to_string(key).expect("JSON key serialization"),
                                value
                            )
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .join(",")
            ))
        }
        serde_json::Value::Number(number) => canonical_json_number(number),
        _ => Ok(serde_json::to_string(value).expect("JSON scalar serialization")),
    }
}

fn canonical_json_number(number: &serde_json::Number) -> Result<String, StoreError> {
    let lexical = number.to_string();
    let c_lexical = CString::new(lexical.as_bytes()).map_err(|_| {
        StoreError::InvalidDiscoveryRun("canonical JSON number is not finite".to_string())
    })?;
    let mut end = ptr::null_mut();
    // serde_json and Rust's f64 parser can round a boundary decimal differently
    // from V8. libc::strtod is used here solely as the cross-runtime decimal-to-
    // binary64 conversion; it is required to consume the complete JSON number.
    let value = unsafe { libc::strtod(c_lexical.as_ptr(), &mut end) };
    let consumed = if end.is_null() {
        None
    } else {
        Some(unsafe { end.offset_from(c_lexical.as_ptr()) as usize })
    };
    if consumed != Some(lexical.len()) || !value.is_finite() {
        return Err(StoreError::InvalidDiscoveryRun(
            "canonical JSON number is not finite".to_string(),
        ));
    }
    if value == 0.0 {
        return Ok("{\"$hfwCanonicalNumberV1\":\"0000000000000000\"}".to_string());
    }
    Ok(format!(
        "{{\"$hfwCanonicalNumberV1\":\"{:016x}\"}}",
        value.to_bits()
    ))
}

fn discovery_run_digest_value(raw: &serde_json::Value) -> Result<String, StoreError> {
    let mut value = raw.clone();
    let object = value.as_object_mut().ok_or_else(|| {
        StoreError::InvalidDiscoveryRun("run must serialize as an object".to_string())
    })?;
    object.remove("integrity");
    if let Some(findings) = object
        .get_mut("findings")
        .and_then(serde_json::Value::as_array_mut)
    {
        findings.sort_by(|left, right| {
            left.get("findingId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .as_bytes()
                .cmp(
                    right
                        .get("findingId")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .as_bytes(),
                )
        });
        for finding in findings {
            if let Some(evidence) = finding
                .get_mut("evidence")
                .and_then(serde_json::Value::as_array_mut)
            {
                evidence.sort_by(|left, right| {
                    left.get("evidenceId")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .as_bytes()
                        .cmp(
                            right
                                .get("evidenceId")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default()
                                .as_bytes(),
                        )
                });
            }
        }
    }
    if let Some(issues) = object
        .get_mut("issues")
        .and_then(serde_json::Value::as_array_mut)
    {
        issues.sort_by(|left, right| {
            left.get("issueId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .as_bytes()
                .cmp(
                    right
                        .get("issueId")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .as_bytes(),
                )
        });
    }
    Ok(format!(
        "{:x}",
        Sha256::digest(canonical_json(&value)?.as_bytes())
    ))
}

fn discovery_run_diagnostics(
    run: &DiscoveryRun,
    imported_at: &str,
    cursor_advanced: bool,
) -> Result<String, StoreError> {
    let diagnostics = DiscoveryRunDiagnostic {
        source_id: run.source.source_id.clone(),
        source_display_name: run.source.display_name.clone(),
        producer: run.source.producer.clone(),
        producer_version: run.source.producer_version.clone(),
        run_id: run.run_id.clone(),
        window_id: run.window_id.clone(),
        supersedes_run_id: run.supersedes_run_id.clone(),
        coverage_start: run.coverage.window_start.clone(),
        coverage_end: run.coverage.window_end.clone(),
        timezone: run.coverage.timezone.clone(),
        generated_at: run.generated_at.clone(),
        status: run.status.clone(),
        digest: run.integrity.digest.clone(),
        finding_count: run.findings.len(),
        imported_at: imported_at.to_string(),
        cursor_advanced,
        issues: run.issues.clone(),
        findings: run
            .findings
            .iter()
            .map(|finding| DiscoveryRunFindingDiagnostic {
                finding_id: finding.finding_id.clone(),
                source_role_id: finding.source_role_id.clone(),
                evidence: finding
                    .evidence
                    .iter()
                    .map(|evidence| DiscoveryRunEvidenceDiagnostic {
                        evidence_id: evidence.evidence_id.clone(),
                        kind: evidence.kind.clone(),
                        observed_at: evidence.observed_at.clone(),
                        url: evidence.url.clone(),
                        content_sha256: evidence.content_sha256.clone(),
                    })
                    .collect(),
            })
            .collect(),
    };
    let serialized = serde_json::to_string(&diagnostics)
        .map_err(|error| StoreError::InvalidDiscoveryRun(error.to_string()))?;
    if serialized.len() > MAX_DISCOVERY_DIAGNOSTICS_BYTES {
        return Err(StoreError::InvalidDiscoveryRun(
            "discovery run diagnostics exceed the maximum size".to_string(),
        ));
    }
    Ok(serialized)
}

fn validate_discovery_finding_identities(
    transaction: &Transaction<'_>,
    run: &DiscoveryRun,
) -> Result<(), StoreError> {
    for finding in &run.findings {
        let prior = transaction
            .query_row(
                "SELECT source_role_id, normalized_key
                   FROM discovery_findings
                  WHERE source_id = ?1 AND finding_id = ?2",
                params![run.source.source_id, finding.finding_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        if let Some((_, normalized_key)) = prior {
            if normalized_key != finding.normalized_key {
                return Err(StoreError::InvalidDiscoveryRun(
                    "findingId was reused for a different normalized identity".to_string(),
                ));
            }
        }
        let claimed_by = transaction
            .query_row(
                "SELECT finding_id FROM discovery_findings
                  WHERE source_id = ?1 AND source_role_id = ?2 AND finding_id != ?3",
                params![
                    run.source.source_id,
                    finding.source_role_id,
                    finding.finding_id
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if claimed_by.is_some() {
            return Err(StoreError::InvalidDiscoveryRun(
                "sourceRoleId is already claimed by a different findingId".to_string(),
            ));
        }
    }
    Ok(())
}

fn reconcile_typed_discovery_finding(
    transaction: &Transaction<'_>,
    finding: &DiscoveryRunFinding,
    legacy: &DiscoveryFinding,
    run_id: &str,
    now: &str,
) -> Result<ImportResult, StoreError> {
    let previous_source_role = transaction
        .query_row(
            "SELECT source_role_id FROM discovery_findings
               WHERE source_id = ?1 AND finding_id = ?2",
            params![finding.source_id, finding.finding_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let mut result = reconcile_discovery_finding(transaction, legacy, now)?;
    if previous_source_role.as_deref() != Some(finding.source_role_id.as_str()) {
        if let Some(previous_source_role) = previous_source_role {
            let references: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM discovery_findings
                  WHERE source_id = ?1 AND source_role_id = ?2 AND finding_id != ?3",
                params![finding.source_id, previous_source_role, finding.finding_id],
                |row| row.get(0),
            )?;
            if references == 0 {
                transaction.execute(
                    "DELETE FROM source_occurrences
                      WHERE source_id = ?1 AND source_role_id = ?2",
                    params![finding.source_id, previous_source_role],
                )?;
            }
            result = ImportResult {
                imported: 0,
                updated: 1,
                unchanged: 0,
            };
        }
    }
    transaction.execute(
        "INSERT INTO discovery_findings(
           source_id, finding_id, source_role_id, normalized_key, last_run_id, payload_hash
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(source_id, finding_id) DO UPDATE SET
           source_role_id = excluded.source_role_id,
           normalized_key = excluded.normalized_key,
           last_run_id = excluded.last_run_id,
           payload_hash = excluded.payload_hash",
        params![
            finding.source_id,
            finding.finding_id,
            finding.source_role_id,
            finding.normalized_key,
            run_id,
            hash_finding(legacy),
        ],
    )?;
    Ok(result)
}

fn reconcile_discovery_finding(
    transaction: &Transaction<'_>,
    finding: &DiscoveryFinding,
    now: &str,
) -> Result<ImportResult, StoreError> {
    let existing_role_id = transaction
        .query_row(
            "SELECT id FROM roles WHERE normalized_key = ?1",
            [&finding.normalized_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let role_id = existing_role_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let payload_hash = hash_finding(finding);
    let existing_hash = transaction
        .query_row(
            "SELECT payload_hash FROM source_occurrences
               WHERE source_id = ?1 AND source_role_id = ?2",
            params![finding.source_id, finding.source_role_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let mut result = ImportResult::default();
    match existing_hash.as_deref() {
        None => result.imported = 1,
        Some(hash) if hash == payload_hash => result.unchanged = 1,
        Some(_) => result.updated = 1,
    }

    upsert_role(transaction, &role_id, finding)?;
    transaction.execute(
        "INSERT INTO source_occurrences(
           id, role_id, source_id, source, source_role_id, payload_hash,
           discovered_at, posted_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(source_id, source_role_id) DO UPDATE SET
           role_id = excluded.role_id,
           source = excluded.source,
           payload_hash = excluded.payload_hash,
           discovered_at = excluded.discovered_at,
           posted_at = excluded.posted_at",
        params![
            Uuid::new_v4().to_string(),
            role_id,
            finding.source_id,
            finding.source,
            finding.source_role_id,
            payload_hash,
            finding.discovered_at,
            finding.posted_at,
        ],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO evaluation_sync(
           role_id, state, reason, created_at, updated_at
         ) VALUES (?1, 'awaiting_evaluation', 'evaluation_receipt_required', ?2, ?2)",
        params![role_id, now],
    )?;
    if existing_hash.as_deref() != Some(payload_hash.as_str()) {
        transaction.execute(
            "UPDATE evaluation_sync
                SET state = CASE
                      WHEN (SELECT canonical_status FROM roles WHERE id = ?1)
                           IN ('Applied', 'Discarded', 'Rejected') THEN 'terminal'
                      ELSE 'awaiting_evaluation'
                    END,
                    reason = CASE
                      WHEN (SELECT canonical_status FROM roles WHERE id = ?1)
                           IN ('Applied', 'Discarded', 'Rejected') THEN 'canonical_terminal'
                      ELSE 'source_identity_changed'
                    END,
                    input_hash = NULL, current_receipt_key = NULL,
                    lease_expires_at = NULL, updated_at = ?2
              WHERE role_id = ?1",
            params![role_id, now],
        )?;
    }
    Ok(result)
}

fn upsert_role(
    transaction: &Transaction<'_>,
    role_id: &str,
    finding: &DiscoveryFinding,
) -> Result<(), rusqlite::Error> {
    transaction.execute(
        "INSERT INTO roles (
           id, normalized_key, company, title, location, queue_group,
           eligibility_summary, uncertainty, legitimacy, application_url, discovered_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(normalized_key) DO UPDATE SET
           company = excluded.company,
           title = excluded.title,
           location = excluded.location,
           queue_group = excluded.queue_group,
           eligibility_summary = excluded.eligibility_summary,
           uncertainty = excluded.uncertainty,
           legitimacy = excluded.legitimacy,
           application_url = COALESCE(excluded.application_url, roles.application_url),
           discovered_at = MIN(roles.discovered_at, excluded.discovered_at),
           updated_at = excluded.updated_at",
        params![
            role_id,
            finding.normalized_key,
            finding.company,
            finding.title,
            finding.location,
            finding.queue_group.as_str(),
            finding.eligibility_summary,
            finding.uncertainty,
            finding.legitimacy.as_ref().map(|value| value.as_str()),
            finding.application_url,
            finding.discovered_at,
            Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(())
}

fn hash_finding(finding: &DiscoveryFinding) -> String {
    let encoded = serde_json::to_vec(finding).expect("serializing a validated finding cannot fail");
    format!("{:x}", Sha256::digest(encoded))
}

fn setting(connection: &Connection, key: &str) -> Result<Option<String>, rusqlite::Error> {
    connection
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()
}

fn insert_browser_command(
    transaction: &Transaction<'_>,
    session_id: &str,
    command_type: &str,
    payload: &serde_json::Value,
    now: &str,
) -> Result<(), rusqlite::Error> {
    transaction.execute(
        "INSERT INTO browser_commands(
           id, session_id, command_type, payload_json, status, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?5)",
        params![
            Uuid::new_v4().to_string(),
            session_id,
            command_type,
            payload.to_string(),
            now
        ],
    )?;
    Ok(())
}

fn normalized_browser_fill_results(
    results: &serde_json::Value,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, StoreError> {
    let items = results.as_array().ok_or_else(|| {
        StoreError::InvalidBrowserTransition("browser fill results are not an array".to_string())
    })?;
    let instructions = payload
        .pointer("/plan/instructions")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            StoreError::InvalidBrowserTransition("stored fill plan has no instructions".to_string())
        })?;
    let mut expected = std::collections::BTreeMap::<String, String>::new();
    for instruction in instructions {
        let field_id = instruction
            .get("fieldId")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                StoreError::InvalidBrowserTransition(
                    "stored fill plan has an invalid field id".to_string(),
                )
            })?;
        let value = instruction
            .get("value")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                StoreError::InvalidBrowserTransition(
                    "stored fill plan has an invalid value".to_string(),
                )
            })?;
        if expected
            .insert(field_id.to_string(), hash_browser_readback(value))
            .is_some()
        {
            return Err(StoreError::InvalidBrowserTransition(
                "stored fill plan contains a duplicate field id".to_string(),
            ));
        }
    }
    if let Some(upload) = payload.get("cvUpload") {
        let field_id = upload
            .get("fieldId")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                StoreError::InvalidBrowserTransition(
                    "stored CV upload has an invalid field id".to_string(),
                )
            })?;
        let hash = upload
            .get("sha256")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                StoreError::InvalidBrowserTransition(
                    "stored CV upload has an invalid hash".to_string(),
                )
            })?;
        expected.insert(field_id.to_string(), hash.to_string());
    }
    let mut normalized = Vec::<serde_json::Value>::with_capacity(items.len());
    let mut seen = std::collections::BTreeSet::<String>::new();
    for item in items {
        let field_id = item
            .get("fieldId")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                StoreError::InvalidBrowserTransition(
                    "browser fill result is missing its field id".to_string(),
                )
            })?;
        if !expected.contains_key(field_id) {
            return Err(StoreError::InvalidBrowserTransition(
                "browser fill results contain an unplanned field id".to_string(),
            ));
        }
        if !seen.insert(field_id.to_string()) {
            return Err(StoreError::InvalidBrowserTransition(
                "browser fill results contain a duplicate field id".to_string(),
            ));
        }
        if item.get("status").and_then(serde_json::Value::as_str) == Some("verified")
            && item
                .get("readBackSha256")
                .and_then(serde_json::Value::as_str)
                != expected.get(field_id).map(String::as_str)
        {
            return Err(StoreError::InvalidBrowserTransition(
                "browser fill read-back hash does not match the planned value".to_string(),
            ));
        }
        normalized.push(item.clone());
    }
    if seen.len() != expected.len() || expected.keys().any(|field_id| !seen.contains(field_id)) {
        return Err(StoreError::InvalidBrowserTransition(
            "browser fill results do not cover every planned field exactly once".to_string(),
        ));
    }
    Ok(serde_json::Value::Array(normalized))
}

fn hash_browser_readback(value: &str) -> String {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    format!("{:x}", Sha256::digest(normalized.trim().as_bytes()))
}

fn browser_failure_is_fallback_eligible(command_type: &str, error_code: &str) -> bool {
    match command_type {
        "inspect_request" => matches!(
            error_code,
            "extension_handshake_timeout"
                | "extension_command_expired"
                | "no_active_application_page"
                | "unsupported_or_unavailable_page"
                | "extension_message_interrupted"
                | "application_tab_recovery_failed"
                | "inspection_failed"
                | "invalid_form_snapshot"
        ),
        "fill_plan" => matches!(
            error_code,
            "snapshot_mismatch" | "form_drift_before_fill" | "invalid_fill_plan_duplicate_field"
        ),
        _ => false,
    }
}

fn leased_browser_command(
    transaction: &Transaction<'_>,
    command_id: &str,
    expected_type: &str,
) -> Result<String, StoreError> {
    let command = transaction
        .query_row(
            "SELECT session_id, command_type FROM browser_commands
              WHERE id = ?1 AND status = 'leased'",
            [command_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((session_id, command_type)) = command else {
        return Err(StoreError::InvalidBrowserTransition(
            "browser command is missing or no longer leased".to_string(),
        ));
    };
    if command_type != expected_type {
        return Err(StoreError::InvalidBrowserTransition(format!(
            "browser command type {command_type} does not match {expected_type}"
        )));
    }
    Ok(session_id)
}

fn outcome_notification_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OutcomeNotification> {
    Ok(OutcomeNotification {
        id: row.get(0)?,
        event_kind: row.get(1)?,
        title: row.get(2)?,
        body: row.get(3)?,
        action_kind: row.get(4)?,
        action_label: row.get(5)?,
        role_id: row.get(6)?,
        preparation_id: row.get(7)?,
        browser_session_id: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn sanitize_preparation_error(value: &str) -> String {
    let mut sanitized = value
        .split_whitespace()
        .map(|token| {
            if token.starts_with("http://") || token.starts_with("https://") {
                "[external URL]".to_string()
            } else if token.starts_with('/') || token.contains("/Users/") {
                "[local path]".to_string()
            } else if token.contains('@')
                && token.chars().all(|character| {
                    character.is_ascii_alphanumeric() || ".@_+-".contains(character)
                })
            {
                "[email]".to_string()
            } else if token.len() >= 32
                && token.chars().all(|character| character.is_ascii_hexdigit())
            {
                "[context id]".to_string()
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    sanitized = sanitized.chars().take(320).collect();
    if sanitized.is_empty() {
        "Preparation stopped without a usable error message.".to_string()
    } else {
        sanitized
    }
}

fn sanitize_evaluation_reason(value: &str) -> String {
    let normalized = value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(240)
        .collect::<String>();
    if normalized.is_empty() {
        "evaluation_result_unavailable".to_string()
    } else {
        normalized
    }
}

fn sanitize_retry_policy(value: &str) -> &'static str {
    match value {
        "retry_same_preparation" => "retry_same_preparation",
        "repair_runtime_then_retry" => "repair_runtime_then_retry",
        "fresh_preparation_provider_run" => "fresh_preparation_provider_run",
        "fresh_preparation_id" => "fresh_preparation_id",
        "manual_repair_required" => "manual_repair_required",
        _ => "manual_repair_required",
    }
}

fn preparation_stage_label(stage: &str) -> String {
    let mut label = stage.replace(['_', '.'], " ");
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
}

fn browser_session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BrowserSessionSummary> {
    Ok(BrowserSessionSummary {
        id: row.get(0)?,
        purpose: row.get(1)?,
        role_id: row.get(2)?,
        preparation_id: row.get(3)?,
        status: row.get(4)?,
        ats: row.get(5)?,
        page_title: row.get(6)?,
        page_url: row.get(7)?,
        snapshot_fingerprint: row.get(8)?,
        field_count: row.get(9)?,
        safe_field_count: row.get(10)?,
        needs_user_count: row.get(11)?,
        error_code: row.get(12)?,
        review_items: row
            .get::<_, Option<String>>(13)?
            .and_then(|value| serde_json::from_str(&value).ok()),
        fill_results: row
            .get::<_, Option<String>>(14)?
            .and_then(|value| serde_json::from_str(&value).ok()),
        updated_at: row.get(15)?,
    })
}

type RoleSummaryTuple = (
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    String,
    Option<i64>,
    Option<String>,
    Option<f64>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn role_summary_from_tuple(
    result: Result<RoleSummaryTuple, rusqlite::Error>,
) -> Result<RoleSummary, StoreError> {
    let (
        id,
        company,
        title,
        location,
        source,
        source_count,
        queue_group,
        eligibility_summary,
        uncertainty,
        posted_at,
        discovered_at,
        application_url,
        preparation_state,
        canonical_tracker_id,
        canonical_status,
        native_score,
        legitimacy,
        risk_level,
        strengths_json,
        blockers_json,
        gaps_json,
        compensation,
        authorization_confidence,
        authorization_question,
        material_uncertainty_json,
    ) = result?;
    let parsed_group = QueueGroup::parse(&queue_group)
        .ok_or_else(|| StoreError::InvalidQueueGroup(queue_group.clone()))?;
    Ok(RoleSummary {
        id,
        company,
        title,
        location,
        source,
        source_count,
        queue_group: parsed_group,
        eligibility_summary,
        uncertainty,
        posted_at,
        discovered_at,
        application_url,
        preparation_state,
        canonical_tracker_id,
        canonical_status,
        evaluation: match (
            native_score,
            legitimacy,
            risk_level,
            strengths_json,
            blockers_json,
            gaps_json,
            authorization_confidence,
            authorization_question,
            material_uncertainty_json,
        ) {
            (
                Some(native_score),
                Some(legitimacy),
                Some(risk_level),
                Some(strengths),
                Some(blockers),
                Some(gaps),
                Some(authorization_confidence),
                Some(authorization_question),
                Some(material_uncertainty),
            ) => Some(QueueEvaluationSummary {
                native_score,
                legitimacy,
                risk_level,
                strengths: serde_json::from_str(&strengths).map_err(StoreError::InvalidDataset)?,
                blockers: serde_json::from_str(&blockers).map_err(StoreError::InvalidDataset)?,
                gaps: serde_json::from_str(&gaps).map_err(StoreError::InvalidDataset)?,
                compensation,
                authorization_confidence,
                authorization_question,
                material_uncertainty: serde_json::from_str(&material_uncertainty)
                    .map_err(StoreError::InvalidDataset)?,
            }),
            _ => None,
        },
    })
}

fn validate_queue_filters(filters: &QueueFilters) -> Result<(), StoreError> {
    for (label, values) in [
        ("role families", &filters.role_families),
        ("seniority", &filters.seniority),
        ("locations", &filters.locations),
    ] {
        if values.len() > 24
            || values
                .iter()
                .any(|value| value.trim().is_empty() || value.chars().count() > 120)
        {
            return Err(StoreError::InvalidPreparation(format!(
                "queue filter {label} must contain at most 24 concise values"
            )));
        }
    }
    Ok(())
}

fn normalized_filter_value(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_filter_phrase(haystack: &str, needle: &str) -> bool {
    !needle.is_empty() && format!(" {haystack} ").contains(&format!(" {needle} "))
}

fn role_matches_queue_filters(role: &RoleSummary, filters: &QueueFilters) -> bool {
    let trust_evidence = normalized_filter_value(&format!(
        "{} {}",
        role.eligibility_summary,
        role.uncertainty.as_deref().unwrap_or_default()
    ));
    if [
        "suspicious",
        "possible scam",
        "suspected scam",
        "job could not be verified",
        "employer could not be verified",
        "company could not be verified",
        "possible impersonation",
        "suspected impersonation",
        "fraudulent listing",
        "legitimacy blocker",
    ]
    .iter()
    .any(|marker| contains_filter_phrase(&trust_evidence, marker))
    {
        return false;
    }

    let title = normalized_filter_value(&role.title);
    if !filters.role_families.is_empty() {
        let ignored = ["engineer", "engineering", "developer", "software", "web"];
        let matches_family = filters.role_families.iter().any(|family| {
            let normalized = normalized_filter_value(family);
            let distinctive = normalized
                .split_whitespace()
                .filter(|token| !ignored.contains(token))
                .collect::<Vec<_>>();
            distinctive.is_empty()
                || distinctive
                    .iter()
                    .any(|token| contains_filter_phrase(&title, token))
        });
        if !matches_family {
            return false;
        }
    }

    let allowed_seniority = filters
        .seniority
        .iter()
        .map(|value| normalized_filter_value(value))
        .collect::<Vec<_>>();
    let explicitly_disallowed = [
        (
            ["intern", "internship", "graduate", "junior", "entry level"].as_slice(),
            ["intern", "graduate", "junior", "entry"].as_slice(),
        ),
        (
            ["mid level", "middle", " l2 ", " l3 "].as_slice(),
            ["mid", "middle", "l2", "l3"].as_slice(),
        ),
    ]
    .iter()
    .any(|(title_markers, allowed_markers)| {
        title_markers
            .iter()
            .any(|marker| contains_filter_phrase(&title, marker))
            && !allowed_seniority.iter().any(|allowed| {
                allowed_markers
                    .iter()
                    .any(|marker| contains_filter_phrase(allowed, marker))
            })
    });
    if explicitly_disallowed {
        return false;
    }

    let location = normalized_filter_value(&role.location);
    let is_remote = ["remote", "worldwide", "home based"]
        .iter()
        .any(|marker| contains_filter_phrase(&location, marker));
    let remote_restricted_elsewhere = [
        "us only",
        "united states",
        "usa only",
        "canada only",
        "north america only",
        "latin america only",
        "apac only",
        "australia only",
        "new zealand only",
        "india only",
    ]
    .iter()
    .any(|marker| contains_filter_phrase(&location, marker));
    let matches_location = filters.locations.is_empty()
        || filters
            .locations
            .iter()
            .map(|value| normalized_filter_value(value))
            .any(|allowed| contains_filter_phrase(&location, &allowed));
    if !(matches_location || filters.remote_allowed && is_remote && !remote_restricted_elsewhere) {
        return false;
    }

    if filters.require_authorization_path {
        let evidence = normalized_filter_value(&format!(
            "{} {}",
            role.eligibility_summary,
            role.uncertainty.as_deref().unwrap_or_default()
        ));
        if [
            "no sponsorship",
            "cannot sponsor",
            "visa sponsorship is not available",
            "must already be authorized",
            "must be legally authorized",
            "must be eligible to work",
            "must have existing work authorization",
            "without sponsorship",
            "unable to sponsor",
            "do not sponsor",
            "valid work permit required",
            "right to work required",
            "us citizen",
            "citizenship required",
        ]
        .iter()
        .any(|marker| contains_filter_phrase(&evidence, marker))
        {
            return false;
        }
    }
    true
}

fn normalized_words(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| {
            !word.is_empty()
                && !matches!(
                    *word,
                    "remote"
                        | "hybrid"
                        | "onsite"
                        | "barcelona"
                        | "madrid"
                        | "spain"
                        | "eu"
                        | "europe"
                )
        })
        .map(ToOwned::to_owned)
        .collect()
}

fn company_matches(discovered: &str, canonical: &str) -> bool {
    let discovered = normalized_words(discovered).join(" ");
    let canonical = normalized_words(canonical).join(" ");
    discovered == canonical
        || (discovered.len() >= 4 && canonical.contains(&discovered))
        || (canonical.len() >= 4 && discovered.contains(&canonical))
}

fn deterministic_history_match<'a>(
    records: &'a [HistoryRecord],
    company: &str,
    title: &str,
    application_url: Option<&str>,
) -> Option<&'a HistoryRecord> {
    if let Some(application_url) = application_url {
        let exact_url = records
            .iter()
            .filter(|record| tracker_notes_contain_exact_url(&record.notes, application_url))
            .collect::<Vec<_>>();
        if exact_url.len() == 1 {
            return exact_url.into_iter().next();
        }
        if exact_url.len() > 1 {
            return None;
        }
    }
    let fallback = records
        .iter()
        .filter(|record| company_matches(company, &record.company))
        .filter(|record| title_matches(title, &record.role))
        .collect::<Vec<_>>();
    (fallback.len() == 1).then(|| fallback[0])
}

fn tracker_notes_contain_exact_url(notes: &str, expected: &str) -> bool {
    let expected = normalized_history_url(expected);
    notes
        .split_whitespace()
        .map(normalized_history_url)
        .any(|candidate| candidate == expected)
}

fn normalized_history_url(value: &str) -> &str {
    value
        .trim_end_matches(['.', ',', ';', ')', ']', '}'])
        .trim_end_matches('/')
}

fn is_applied_status(status: &str) -> bool {
    status.trim().eq_ignore_ascii_case("Applied")
}

fn is_discarded_status(status: &str) -> bool {
    status.trim().eq_ignore_ascii_case("Discarded")
}

fn is_rejected_status(status: &str) -> bool {
    status.trim().eq_ignore_ascii_case("Rejected")
}

fn is_terminal_canonical_status(status: &str) -> bool {
    is_applied_status(status) || is_discarded_status(status) || is_rejected_status(status)
}

fn title_matches(discovered: &str, canonical: &str) -> bool {
    let discovered = normalized_words(discovered);
    let canonical = normalized_words(canonical);
    discovered == canonical
        || (discovered.len() >= 3 && contains_word_sequence(&canonical, &discovered))
        || (canonical.len() >= 3 && contains_word_sequence(&discovered, &canonical))
}

fn contains_word_sequence(haystack: &[String], needle: &[String]) -> bool {
    needle.len() <= haystack.len()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn missed_nominal_times(
    cursor: DateTime<Utc>,
    now: DateTime<Utc>,
    hours: &str,
) -> Vec<DateTime<Utc>> {
    let hours = hours
        .split(',')
        .filter_map(|value| value.parse::<u32>().ok())
        .filter(|hour| *hour < 24)
        .collect::<Vec<_>>();
    let local_cursor = cursor.with_timezone(&Madrid);
    let local_now = now.with_timezone(&Madrid);
    let mut date = local_cursor.date_naive();
    let mut due = Vec::new();
    while date <= local_now.date_naive() && due.len() < 64 {
        for hour in &hours {
            let Some(time) = NaiveTime::from_hms_opt(*hour, 0, 0) else {
                continue;
            };
            let local = date.and_time(time);
            if let Some(nominal) = Madrid.from_local_datetime(&local).single() {
                let nominal = nominal.with_timezone(&Utc);
                if nominal > cursor && nominal <= now {
                    due.push(nominal);
                }
            }
        }
        date = match date.succ_opt() {
            Some(next) => next,
            None => break,
        };
    }
    due.sort_unstable();
    due
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_DISCOVERY_FINDINGS, MAX_DISCOVERY_RUN_BYTES, MAX_EVALUATION_SYNC_ATTEMPTS,
        PreparationCompletion, QueueFilters, Store, hash_browser_readback, valid_datetime,
        valid_text, validate_discovery_run_raw_bounds,
    };
    use crate::domain::{
        BrowserSessionSummary, EvaluationResultRead, HistoryRecord, ImportResult, PreparationWork,
        QueueGroup,
    };
    use rusqlite::params;
    use serde_json::Value;
    use sha2::{Digest, Sha256};

    const DATASET: &str = r#"{
      "schemaVersion": 1,
      "generatedAt": "2026-08-30T12:00:00+02:00",
      "findings": [
        {
          "sourceId": "source-a",
          "source": "Example source",
          "sourceRoleId": "role-1",
          "company": "Northstar Tools",
          "title": "Frontend Engineer",
          "location": "Remote, Europe",
          "discoveredAt": "2026-08-30T09:00:00+02:00",
          "postedAt": "2026-08-28",
          "applicationUrl": "https://example.test/jobs/1",
          "normalizedKey": "northstar-tools/frontend-engineer",
          "queueGroup": "needs_decision",
          "eligibilitySummary": "Remote scope appears compatible but authorization is not verified.",
          "uncertainty": "Confirm whether Spain is an eligible hiring location."
        }
      ]
    }"#;

    fn test_evaluation(
        role: &crate::domain::EvaluationSyncRole,
        final_decision: &str,
        legitimacy: &str,
        confidence: &str,
    ) -> EvaluationResultRead {
        let report_hash = format!("{:x}", sha2::Sha256::digest(role.role_id.as_bytes()));
        serde_json::from_value(serde_json::json!({
            "contract": "hereforwork.career-ops-evaluation-result",
            "schemaVersion": 1,
            "upstreamRevision": "d".repeat(40),
            "compatibilityFingerprint": "c".repeat(64),
            "report": { "path": format!("reports/{}.md", role.role_id), "sha256": report_hash },
            "role": { "company": role.company, "title": role.title },
            "canonical": { "trackerId": 42, "status": "Evaluated", "score": 4.2, "reportPath": format!("reports/{}.md", role.role_id) },
            "evaluation": {
                "score": 4.2,
                "finalDecision": final_decision,
                "legitimacyTier": legitimacy,
                "archetype": "Frontend Engineer",
                "nextAction": "Review the evaluated role.",
                "strengths": ["Strong source-backed frontend match."],
                "blockers": [],
                "gaps": [],
                "compensation": { "advertised": null },
                "authorization": {
                    "confidence": "interesting",
                    "evidence": ["The source supports an eligible route."],
                    "scope": "job-specific",
                    "engagementMechanism": "employee_payroll",
                    "question": "Confirm the employing entity.",
                    "legacyWorkAuth": "unstated"
                },
                "riskLevel": "Low",
                "confidence": confidence,
                "riskSummary": {
                    "legitimacy": if legitimacy == "Suspicious" { "suspicious" } else { "high_confidence" },
                    "classification": "clear",
                    "culture": "pass",
                    "interviewRedflags": "none",
                    "aiInfra": "consistent",
                    "aiScreeningDisclosure": "no_match"
                },
                "materialUncertainty": {
                    "confidence": confidence,
                    "authorizationQuestion": "Confirm the employing entity.",
                    "notEvaluatedRiskSignals": []
                }
            }
        }))
        .unwrap()
    }

    fn allow_all_test_roles(store: &mut Store) {
        for role in store.evaluation_sync_roles().unwrap() {
            let input_hash = format!("test:{}", role.source_identity_hash);
            if !store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
            {
                continue;
            }
            let evaluation = test_evaluation(&role, "Apply", "High Confidence", "High");
            store
                .complete_evaluation_sync(&role, &input_hash, &evaluation)
                .unwrap();
        }
    }

    fn import_evaluated(store: &mut Store, payload: &str) -> ImportResult {
        let result = store.import_dataset(payload).unwrap();
        allow_all_test_roles(store);
        result
    }

    fn sealed_discovery_run(mut value: Value) -> String {
        value["integrity"]["digest"] = Value::String(String::new());
        let mut run = value;
        let digest = super::discovery_run_digest_value(&run).unwrap();
        run.as_object_mut().unwrap().insert(
            "integrity".to_string(),
            serde_json::json!({
                "algorithm": "sha256",
                "canonicalization": "hfw-discovery-run-v1",
                "coverage": "all_top_level_fields_except_integrity",
                "digest": digest,
            }),
        );
        serde_json::to_string(&run).unwrap()
    }

    fn discovery_fixture() -> Value {
        serde_json::from_str(include_str!("../../examples/discovery-run.example.json")).unwrap()
    }

    fn queue_and_claim(store: &mut Store, role_id: &str, provider: &str) -> PreparationWork {
        let queued = store.begin_preparation(role_id, provider).unwrap();
        let claimed = store.claim_preparation_work().unwrap().unwrap();
        assert_eq!(claimed.id, queued.id);
        claimed
    }

    fn completed_preparation(store: &mut Store) -> PreparationWork {
        import_evaluated(store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let preparation = queue_and_claim(store, &role_id, "codex");
        store
            .record_preparation_context(
                &preparation.id,
                &"a".repeat(64),
                "https://example.test/jobs/1",
            )
            .unwrap();
        store
            .complete_preparation(
                &preparation.id,
                &PreparationCompletion {
                    tracker_id: 42,
                    report_path: "reports/042-example.md",
                    report_hash: &"b".repeat(64),
                    cv_pdf_path: "output/042-example/cv.pdf",
                    cv_pdf_hash: &"c".repeat(64),
                    cv_source: "tailored_generated",
                },
            )
            .unwrap();
        preparation
    }

    fn queue_completed_application_session(store: &mut Store) -> BrowserSessionSummary {
        let preparation = completed_preparation(store);
        store
            .configure_browser(
                "abcdefghijklmnopabcdefghijklmnop",
                "019d0000-0000-7000-8000-000000000001",
                "Profile 1",
            )
            .unwrap();
        store.queue_application_session(&preparation.id).unwrap()
    }

    #[test]
    fn importing_the_same_snapshot_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();

        let first = import_evaluated(&mut store, DATASET);
        let second = import_evaluated(&mut store, DATASET);

        assert_eq!(first.imported, 1);
        assert_eq!(second.unchanged, 1);
        let roles = store.dashboard().unwrap().roles;
        assert_eq!(roles.len(), 1);
        assert_eq!(roles[0].posted_at.as_deref(), Some("2026-08-28"));
    }

    #[test]
    fn discovery_run_first_import_and_replay_are_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let fixture_digest = super::discovery_run_digest_value(&discovery_fixture()).unwrap();
        assert_eq!(
            fixture_digest,
            "68aecb6b79fd151e18edc3e062e287cba33e9374cd545035b945fff93bd4e054"
        );
        let mut fixture = discovery_fixture();
        fixture.as_object_mut().unwrap().remove("supersedesRunId");
        let payload = sealed_discovery_run(fixture);

        let first = store.import_discovery_run(&payload).unwrap();
        assert_eq!(first.imported, 2);
        assert!(first.recorded);
        assert!(first.cursor_advanced);
        assert!(!first.replayed);
        let replay = store.import_discovery_run(&payload).unwrap();
        assert_eq!(
            replay,
            crate::domain::DiscoveryRunImportResult {
                replayed: true,
                ..Default::default()
            }
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM discovery_runs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM discovery_cursors", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM roles", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(store.integrity_check().unwrap(), "ok");
        let replay_roles = store
            .connection
            .query_row("SELECT COUNT(*) FROM roles", [], |row| row.get::<_, i64>(0))
            .unwrap();
        assert_eq!(replay_roles, 2);
    }

    #[test]
    fn discovery_run_digest_conflict_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let mut fixture = discovery_fixture();
        fixture.as_object_mut().unwrap().remove("supersedesRunId");
        let payload = sealed_discovery_run(fixture.clone());
        store.import_discovery_run(&payload).unwrap();

        fixture["findings"][0]["title"] = Value::String("Changed title".to_string());
        let conflict = sealed_discovery_run(fixture);
        let error = store.import_discovery_run(&conflict).unwrap_err();
        assert!(error.to_string().contains("different digest"));
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM discovery_runs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM discovery_cursors", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn discovery_run_accepts_any_finite_native_score_in_range() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let mut fixture = discovery_fixture();
        fixture.as_object_mut().unwrap().remove("supersedesRunId");
        fixture["findings"][0]["matchScore"]["value"] =
            Value::Number(serde_json::Number::from_f64(3.6495739800251394).unwrap());
        fixture["findings"][1]["matchScore"] = fixture["findings"][0]["matchScore"].clone();
        fixture["findings"][1]["matchScore"]["value"] =
            Value::Number(serde_json::Number::from_f64(4.25).unwrap());
        let payload = sealed_discovery_run(fixture);
        let result = store.import_discovery_run(&payload).unwrap();
        assert_eq!(result.imported, 2);
    }

    #[test]
    fn discovery_run_requires_finding_source_to_match_run_display_name() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let mut fixture = discovery_fixture();
        fixture.as_object_mut().unwrap().remove("supersedesRunId");
        fixture["findings"][0]["source"] = Value::String("Untrusted source label".to_string());
        let error = store
            .import_discovery_run(&sealed_discovery_run(fixture))
            .unwrap_err();
        assert!(error.to_string().contains("finding identity"));
        assert_eq!(store.discovery_run_diagnostics().unwrap().len(), 0);
    }

    #[test]
    fn discovery_run_timestamps_match_strict_contract_shape() {
        for value in [
            "2026-09-01 13:00:00+02:00",
            "2026-09-01t13:00:00+02:00",
            "2026-09-01T13:00:00z",
            "2026-09-01T13:00:60+02:00",
            "2026-09-01T13:00:00.1234567890+02:00",
        ] {
            assert!(
                !valid_datetime(value),
                "accepted invalid timestamp: {value}"
            );
        }
        assert!(!valid_datetime(&"2026-09-01T13:00:00+02:00".repeat(6)));
        for value in [
            "2026-09-01T13:00:00+02:00",
            "2026-09-01T13:00:00.123456789+02:00",
            "2026-09-01T11:00:00Z",
        ] {
            assert!(valid_datetime(value), "rejected valid timestamp: {value}");
        }
        let overlong = format!("2026-09-01T13:00:00.{}+02:00", "1".repeat(76));
        assert!(!valid_datetime(&overlong));
    }

    #[test]
    fn canonical_json_number_formatting_matches_shared_parity_fixtures() {
        let fixtures: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../contracts/discovery-run-number-parity.json"
        ))
        .unwrap();
        for fixture in fixtures {
            let value = serde_json::json!({ "value": fixture["value"] });
            let expected = format!(
                "{{\"value\":{{\"$hfwCanonicalNumberV1\":\"{}\"}}}}",
                fixture["expected"].as_str().unwrap()
            );
            assert_eq!(super::canonical_json(&value).unwrap(), expected);
        }
    }

    #[test]
    fn career_ops_score_digest_parity_covers_randomized_valid_f64_values() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../contracts/discovery-run-score-precision.json"
        ))
        .unwrap();
        let mut state = fixture["seed"].as_u64().unwrap() as u32;
        let mut aggregate = Sha256::new();
        for _ in 0..fixture["count"].as_u64().unwrap() {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let candidate = 1.0 + (state as f64 / 4_294_967_296.0) * 4.0;
            assert!((1.0..=5.0).contains(&candidate));
            let value = serde_json::json!({ "value": candidate });
            let digest = format!(
                "{:x}",
                Sha256::digest(super::canonical_json(&value).unwrap().as_bytes())
            );
            aggregate.update(digest.as_bytes());
        }
        assert_eq!(
            format!("{:x}", aggregate.finalize()),
            fixture["aggregateSha256"].as_str().unwrap()
        );
    }

    #[test]
    fn discovery_run_text_limits_count_utf8_bytes_for_emoji() {
        assert!(valid_text(&"😀".repeat(50), 1, 200));
        assert!(!valid_text(&"😀".repeat(51), 1, 200));
    }

    #[test]
    fn stable_finding_identity_reconciles_a_source_role_change() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let mut first = discovery_fixture();
        first.as_object_mut().unwrap().remove("supersedesRunId");
        store
            .import_discovery_run(&sealed_discovery_run(first))
            .unwrap();

        let mut next = discovery_fixture();
        next.as_object_mut().unwrap().remove("supersedesRunId");
        next["runId"] = Value::String("01990df0-4d80-7ab0-b4f1-7d83a11b2c99".to_string());
        next["windowId"] = Value::String("01990de0-5790-74ea-9cee-0ec9cc19dc99".to_string());
        next["coverage"]["windowStart"] = Value::String("2026-09-01T13:00:00+02:00".to_string());
        next["coverage"]["windowEnd"] = Value::String("2026-09-01T14:00:00+02:00".to_string());
        next["generatedAt"] = Value::String("2026-09-01T14:05:00+02:00".to_string());
        next["findings"][0]["sourceRoleId"] = Value::String("example-001-reissued".to_string());
        let result = store
            .import_discovery_run(&sealed_discovery_run(next))
            .unwrap();
        assert_eq!(result.updated, 1);
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM source_occurrences
                      WHERE source_id = 'eu-job-radar' AND source_role_id = 'example-001'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM discovery_findings
                      WHERE source_id = 'eu-job-radar' AND finding_id = 'eu-job-radar:example-001'
                        AND source_role_id = 'example-001-reissued'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn stable_finding_identity_conflict_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let mut first = discovery_fixture();
        first.as_object_mut().unwrap().remove("supersedesRunId");
        store
            .import_discovery_run(&sealed_discovery_run(first))
            .unwrap();

        let mut conflict = discovery_fixture();
        conflict.as_object_mut().unwrap().remove("supersedesRunId");
        conflict["runId"] = Value::String("01990df0-4d80-7ab0-b4f1-7d83a11b2c99".to_string());
        conflict["windowId"] = Value::String("01990de0-5790-74ea-9cee-0ec9cc19dc99".to_string());
        conflict["coverage"]["windowStart"] =
            Value::String("2026-09-01T13:00:00+02:00".to_string());
        conflict["coverage"]["windowEnd"] = Value::String("2026-09-01T14:00:00+02:00".to_string());
        conflict["generatedAt"] = Value::String("2026-09-01T14:05:00+02:00".to_string());
        conflict["findings"][0]["normalizedKey"] = Value::String("different-identity".to_string());
        let error = store
            .import_discovery_run(&sealed_discovery_run(conflict))
            .unwrap_err();
        assert!(error.to_string().contains("findingId"));
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM discovery_findings", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }

    #[test]
    fn failed_discovery_run_records_diagnostics_without_advancing_cursor() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let mut fixture = discovery_fixture();
        let object = fixture.as_object_mut().unwrap();
        object.remove("supersedesRunId");
        object.insert("status".to_string(), Value::String("partial".to_string()));
        object.insert(
            "issues".to_string(),
            serde_json::json!([{
                "issueId": "source-timeout",
                "code": "source_timeout",
                "message": "Synthetic source timeout",
                "retryable": true
            }]),
        );
        object.insert("findings".to_string(), Value::Array(Vec::new()));
        let payload = sealed_discovery_run(fixture);

        let result = store.import_discovery_run(&payload).unwrap();
        assert!(result.recorded);
        assert!(!result.cursor_advanced);
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM discovery_cursors", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT status FROM discovery_runs", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "partial"
        );
        let diagnostics = store.discovery_run_diagnostics().unwrap();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].status, "partial");
        assert_eq!(diagnostics[0].issues[0].code, "source_timeout");
        assert!(diagnostics[0].findings.is_empty());
        assert!(store.discovery_cursors().unwrap().is_empty());
        let dashboard = store.dashboard().unwrap();
        assert_eq!(dashboard.discovery_runs.len(), 1);
        assert_eq!(dashboard.discovery_runs[0].issues[0].code, "source_timeout");
        let redacted = store.redacted_diagnostics().unwrap();
        assert_eq!(redacted["discoveryRuns"][0]["status"], "partial");
        assert_eq!(redacted["discoveryCursors"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn invalid_retry_coverage_rolls_back_and_preserves_cursor() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let mut first = discovery_fixture();
        first.as_object_mut().unwrap().remove("supersedesRunId");
        let first_run = sealed_discovery_run(first);
        store.import_discovery_run(&first_run).unwrap();

        let mut retry = discovery_fixture();
        retry["runId"] = Value::String("01990df0-4d80-7ab0-b4f1-7d83a11b2c99".to_string());
        retry["coverage"]["windowEnd"] = Value::String("2026-09-01T14:00:00+02:00".to_string());
        retry["generatedAt"] = Value::String("2026-09-01T14:05:00+02:00".to_string());
        retry["supersedesRunId"] =
            Value::String("01990df0-4d80-7ab0-b4f1-7d83a11b2c00".to_string());
        let error = store
            .import_discovery_run(&sealed_discovery_run(retry))
            .unwrap_err();
        assert!(error.to_string().contains("coverage"));
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM discovery_runs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT coverage_end FROM discovery_cursors", [], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap(),
            "2026-09-01T13:00:00+02:00"
        );
    }

    #[test]
    fn older_completed_run_is_recorded_without_reconciling_or_rewinding_cursor() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let mut newer = discovery_fixture();
        newer.as_object_mut().unwrap().remove("supersedesRunId");
        newer["runId"] = Value::String("01990df0-4d80-7ab0-b4f1-7d83a11b2c99".to_string());
        newer["windowId"] = Value::String("01990de0-5790-74ea-9cee-0ec9cc19dc99".to_string());
        newer["coverage"]["windowStart"] = Value::String("2026-09-01T13:00:00+02:00".to_string());
        newer["coverage"]["windowEnd"] = Value::String("2026-09-01T14:00:00+02:00".to_string());
        newer["generatedAt"] = Value::String("2026-09-01T14:05:00+02:00".to_string());
        store
            .import_discovery_run(&sealed_discovery_run(newer))
            .unwrap();
        let before = store
            .connection
            .query_row(
                "SELECT r.title, o.source_role_id FROM source_occurrences o
                  JOIN roles r ON r.id = o.role_id
                  WHERE o.source_id = 'eu-job-radar' AND o.source_role_id = 'example-001'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap();

        let mut older = discovery_fixture();
        older.as_object_mut().unwrap().remove("supersedesRunId");
        older["runId"] = Value::String("01990df0-4d80-7ab0-b4f1-7d83a11b2c88".to_string());
        older["findings"][0]["title"] = Value::String("Should not overwrite".to_string());
        older["findings"][0]["sourceRoleId"] = Value::String("older-id".to_string());
        let result = store
            .import_discovery_run(&sealed_discovery_run(older))
            .unwrap();
        assert!(result.recorded);
        assert!(!result.cursor_advanced);
        assert_eq!(result.imported + result.updated + result.unchanged, 0);
        let after = store
            .connection
            .query_row(
                "SELECT r.title, o.source_role_id FROM source_occurrences o
                  JOIN roles r ON r.id = o.role_id
                  WHERE o.source_id = 'eu-job-radar' AND o.source_role_id = 'example-001'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(
            store.discovery_cursors().unwrap()[0].coverage_end,
            "2026-09-01T14:00:00+02:00"
        );
    }

    #[test]
    fn retry_lineage_allows_one_partial_retry_and_rejects_completed_or_old_attempts() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let mut partial = discovery_fixture();
        partial.as_object_mut().unwrap().remove("supersedesRunId");
        partial["status"] = Value::String("partial".to_string());
        partial["findings"] = Value::Array(Vec::new());
        partial["issues"] = serde_json::json!([{
            "issueId": "timeout", "code": "timeout", "message": "temporary", "retryable": true
        }]);
        let partial_run_id = partial["runId"].as_str().unwrap().to_string();
        store
            .import_discovery_run(&sealed_discovery_run(partial))
            .unwrap();

        let mut completed = discovery_fixture();
        completed["runId"] = Value::String("01990df0-4d80-7ab0-b4f1-7d83a11b2c99".to_string());
        completed["supersedesRunId"] = Value::String(partial_run_id.clone());
        store
            .import_discovery_run(&sealed_discovery_run(completed))
            .unwrap();

        let mut invalid = discovery_fixture();
        invalid["runId"] = Value::String("01990df0-4d80-7ab0-b4f1-7d83a11b2c88".to_string());
        invalid["supersedesRunId"] = Value::String(partial_run_id);
        let error = store
            .import_discovery_run(&sealed_discovery_run(invalid))
            .unwrap_err();
        assert!(error.to_string().contains("latest attempt"));
        let mut bypass = discovery_fixture();
        bypass.as_object_mut().unwrap().remove("supersedesRunId");
        bypass["runId"] = Value::String("01990df0-4d80-7ab0-b4f1-7d83a11b2c77".to_string());
        let error = store
            .import_discovery_run(&sealed_discovery_run(bypass))
            .unwrap_err();
        assert!(error.to_string().contains("must set supersedesRunId"));
        assert_eq!(store.discovery_run_diagnostics().unwrap().len(), 2);
    }

    #[test]
    fn discovery_run_input_and_collection_limits_fail_before_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let error = store
            .import_discovery_run(&"{".repeat(MAX_DISCOVERY_RUN_BYTES + 1))
            .unwrap_err();
        assert!(error.to_string().contains("maximum payload size"));

        let mut fixture = discovery_fixture();
        fixture["findings"] = Value::Array(
            std::iter::repeat_n(fixture["findings"][0].clone(), MAX_DISCOVERY_FINDINGS + 1)
                .collect(),
        );
        let error = store
            .import_discovery_run(&sealed_discovery_run(fixture))
            .unwrap_err();
        assert!(error.to_string().contains("bounded findings"));
        assert_eq!(store.discovery_run_diagnostics().unwrap().len(), 0);
    }

    #[test]
    fn raw_cardinality_is_checked_before_typed_deserialization() {
        let mut fixture = discovery_fixture();
        fixture["findings"] = Value::Array(vec![Value::Null; MAX_DISCOVERY_FINDINGS + 1]);
        let error = validate_discovery_run_raw_bounds(&fixture).unwrap_err();
        assert!(error.to_string().contains("bounded findings"));

        let mut evidence_fixture = discovery_fixture();
        evidence_fixture["findings"][0]["evidence"] = Value::Array(vec![
            Value::Null;
            super::MAX_DISCOVERY_EVIDENCE_PER_FINDING
                + 1
        ]);
        let error = validate_discovery_run_raw_bounds(&evidence_fixture).unwrap_err();
        assert!(error.to_string().contains("bounded findings"));

        let mut issue_fixture = discovery_fixture();
        issue_fixture["issues"] = Value::Array(vec![Value::Null; super::MAX_DISCOVERY_ISSUES + 1]);
        let error = validate_discovery_run_raw_bounds(&issue_fixture).unwrap_err();
        assert!(error.to_string().contains("bounded findings"));
    }

    #[test]
    fn failed_import_transaction_rolls_back_without_advancing_cursor() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        store
            .connection
            .execute_batch(
                "CREATE TRIGGER fail_discovery_activity
                   BEFORE INSERT ON activity
                   WHEN NEW.kind = 'import'
                   BEGIN
                     SELECT RAISE(ABORT, 'forced import failure');
                   END;",
            )
            .unwrap();
        let mut fixture = discovery_fixture();
        fixture.as_object_mut().unwrap().remove("supersedesRunId");
        let error = store
            .import_discovery_run(&sealed_discovery_run(fixture))
            .unwrap_err();
        assert!(error.to_string().contains("forced import failure"));
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM discovery_runs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM discovery_cursors", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(store.integrity_check().unwrap(), "ok");
    }

    #[test]
    fn evaluated_queue_role_with_receipt_and_url_can_begin_preparation() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role = store.dashboard().unwrap().roles[0].clone();
        assert_eq!(role.preparation_state, "not_started");
        assert!(role.application_url.is_some());
        assert!(role.evaluation.is_some());

        let queued = store.begin_preparation(&role.id, "codex").unwrap();
        let dashboard = store.dashboard().unwrap();
        assert!(dashboard.roles.iter().all(|item| item.id != role.id));
        assert_eq!(dashboard.preparations[0].id, queued.id);
        assert_eq!(dashboard.preparations[0].status, "queued");
    }

    #[test]
    fn imported_role_is_held_before_queue_and_cannot_prepare_without_a_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();

        store.import_dataset(DATASET).unwrap();

        let dashboard = store.dashboard().unwrap();
        assert!(dashboard.roles.is_empty());
        assert_eq!(dashboard.pre_queue_roles.len(), 1);
        assert_eq!(dashboard.pre_queue_roles[0].state, "awaiting_evaluation");
        assert_eq!(
            dashboard.pre_queue_roles[0].recovery.scope,
            crate::domain::PreQueueRecoveryScope::None
        );
        assert_eq!(dashboard.handled_count, 0);
        let error = store
            .begin_preparation(&dashboard.pre_queue_roles[0].role_id, "codex")
            .unwrap_err();
        assert!(error.to_string().contains("awaiting a current canonical"));
    }

    #[test]
    fn canonical_receipt_promotes_once_and_persists_only_the_bounded_projection() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("test.sqlite3");
        let mut store = Store::open(&database).unwrap();
        store.import_dataset(DATASET).unwrap();
        let role = store.evaluation_sync_roles().unwrap().remove(0);
        let input_hash = format!("receipt:{}", role.source_identity_hash);
        assert!(
            store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );
        let evaluation = test_evaluation(&role, "Apply", "High Confidence", "High");
        assert!(
            store
                .complete_evaluation_sync(&role, &input_hash, &evaluation)
                .unwrap()
        );
        assert!(
            !store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );

        let dashboard = store.dashboard().unwrap();
        assert_eq!(dashboard.roles.len(), 1);
        let projection = dashboard.roles[0].evaluation.as_ref().unwrap();
        assert_eq!(projection.native_score, 4.2);
        assert_eq!(projection.legitimacy, "High Confidence");
        assert_eq!(projection.risk_level, "Low");
        assert_eq!(
            projection.strengths,
            ["Strong source-backed frontend match."]
        );
        assert_eq!(dashboard.handled_count, 0);
        let receipts: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM evaluation_receipts", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(receipts, 1);

        store
            .connection
            .execute("UPDATE evaluation_receipts SET risk_level = NULL", [])
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE evaluation_sync
                    SET state = 'needs_attention', reason = 'canonical_evaluation_requires_refresh',
                        current_receipt_key = NULL
                  WHERE role_id = ?1",
                [&role.role_id],
            )
            .unwrap();
        assert!(
            store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );
        assert!(
            store
                .complete_evaluation_sync(&role, &input_hash, &evaluation)
                .unwrap()
        );
        assert_eq!(
            store.dashboard().unwrap().roles[0]
                .evaluation
                .as_ref()
                .unwrap()
                .risk_level,
            "Low"
        );
        let receipts_after_refresh: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM evaluation_receipts", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(receipts_after_refresh, 1);
        drop(store);

        let restarted = Store::open(&database).unwrap();
        assert_eq!(restarted.dashboard().unwrap().roles.len(), 1);
    }

    #[test]
    fn evaluated_human_decision_stays_in_queue_while_skip_and_suspicious_are_handled() {
        for (decision, legitimacy, expected_roles, expected_handled) in [
            ("Research first", "High Confidence", 1, 0),
            ("Skip", "High Confidence", 0, 1),
            ("Apply", "Suspicious", 0, 1),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
            store.import_dataset(DATASET).unwrap();
            let role = store.evaluation_sync_roles().unwrap().remove(0);
            let input_hash = format!(
                "receipt:{}:{decision}:{legitimacy}",
                role.source_identity_hash
            );
            assert!(
                store
                    .claim_evaluation_sync(&role.role_id, &input_hash)
                    .unwrap()
            );
            let evaluation = test_evaluation(&role, decision, legitimacy, "High");
            store
                .complete_evaluation_sync(&role, &input_hash, &evaluation)
                .unwrap();

            let dashboard = store.dashboard().unwrap();
            assert_eq!(dashboard.roles.len(), expected_roles);
            assert_eq!(dashboard.handled_count, expected_handled);
            if decision == "Research first" {
                assert_eq!(dashboard.roles[0].queue_group, QueueGroup::NeedsDecision);
                assert!(store.begin_preparation(&role.role_id, "codex").is_ok());
            }
        }
    }

    #[test]
    fn transient_probe_unavailability_preserves_and_recovers_the_current_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        store.import_dataset(DATASET).unwrap();
        let role = store.evaluation_sync_roles().unwrap().remove(0);
        let input_hash = format!("probe:{}", role.source_identity_hash);
        assert!(
            store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );
        let evaluation = test_evaluation(&role, "Apply", "High Confidence", "High");
        store
            .complete_evaluation_sync(&role, &input_hash, &evaluation)
            .unwrap();
        let receipt_before = store
            .connection
            .query_row(
                "SELECT current_receipt_key FROM evaluation_sync WHERE role_id = ?1",
                [&role.role_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .unwrap();
        assert!(receipt_before.is_some());

        assert_eq!(
            store
                .invalidate_evaluation_compatibility(None, None)
                .unwrap(),
            0
        );
        store
            .mark_evaluation_sync_unavailable(
                &role.role_id,
                "evaluation_result_capability_unavailable",
            )
            .unwrap();
        let receipt_during_outage = store
            .connection
            .query_row(
                "SELECT current_receipt_key FROM evaluation_sync WHERE role_id = ?1",
                [&role.role_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .unwrap();
        assert_eq!(receipt_during_outage, receipt_before);
        let state_during_outage: String = store
            .connection
            .query_row(
                "SELECT state FROM evaluation_sync WHERE role_id = ?1",
                [&role.role_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            matches!(state_during_outage.as_str(), "ready" | "needs_decision"),
            "verified receipt must stay queue-eligible during a transient read outage"
        );
        assert_eq!(store.dashboard().unwrap().roles.len(), 1);

        assert!(
            !store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );
        assert_eq!(store.dashboard().unwrap().roles.len(), 1);
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM evaluation_receipts", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn capability_outage_still_holds_roles_without_a_verified_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        store.import_dataset(DATASET).unwrap();
        let role = store.evaluation_sync_roles().unwrap().remove(0);

        store
            .mark_evaluation_sync_unavailable(
                &role.role_id,
                "evaluation_result_capability_unavailable",
            )
            .unwrap();
        let (state, reason, receipt): (String, String, Option<String>) = store
            .connection
            .query_row(
                "SELECT state, reason, current_receipt_key FROM evaluation_sync WHERE role_id = ?1",
                [&role.role_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(state, "needs_attention");
        assert_eq!(reason, "evaluation_result_capability_unavailable");
        assert!(receipt.is_none());
        assert!(store.dashboard().unwrap().roles.is_empty());
    }

    #[test]
    fn schema_v24_rehydrates_capability_unavailable_roles_with_receipts() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.sqlite3");
        let mut store = Store::open(&path).unwrap();
        store.import_dataset(DATASET).unwrap();
        let role = store.evaluation_sync_roles().unwrap().remove(0);
        let input_hash = format!("rehydrate:{}", role.source_identity_hash);
        assert!(
            store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );
        let evaluation = test_evaluation(&role, "Research first", "High Confidence", "High");
        store
            .complete_evaluation_sync(&role, &input_hash, &evaluation)
            .unwrap();
        assert_eq!(store.dashboard().unwrap().roles.len(), 1);

        store
            .connection
            .execute(
                "UPDATE evaluation_sync
                    SET state = 'needs_attention',
                        reason = 'evaluation_result_capability_unavailable',
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                  WHERE role_id = ?1",
                [&role.role_id],
            )
            .unwrap();
        store
            .connection
            .execute(
                "INSERT INTO runs(
                   id, source_id, kind, coverage_start, coverage_end, status, error_class,
                   created_at, updated_at, dedupe_key
                 ) VALUES (
                   'catch-up-obsolete', 'frontend-role-scan', 'catch_up',
                   '2026-08-30T00:00:00Z', '2026-08-30T08:00:00Z',
                   'action_required', 'source_adapter_not_configured',
                   '2026-08-30T08:00:00Z', '2026-08-30T08:00:00Z',
                   'frontend-role-scan:2026-08-30T08:00:00Z'
                 )",
                [],
            )
            .unwrap();
        store
            .connection
            .execute("UPDATE schema_meta SET version = 23", [])
            .unwrap();
        drop(store);

        let store = Store::open(&path).unwrap();
        let (state, reason): (String, String) = store
            .connection
            .query_row(
                "SELECT state, reason FROM evaluation_sync WHERE role_id = ?1",
                [&role.role_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, "needs_decision");
        assert_eq!(reason, "canonical_evaluation_verified");
        assert_eq!(store.dashboard().unwrap().roles.len(), 1);
        assert_eq!(store.dashboard().unwrap().action_required_run_count, 0);
        let run_status: String = store
            .connection
            .query_row(
                "SELECT status FROM runs WHERE id = 'catch-up-obsolete'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(run_status, "cancelled");
        let schema: i64 = store
            .connection
            .query_row("SELECT version FROM schema_meta", [], |row| row.get(0))
            .unwrap();
        assert_eq!(schema, 25);
    }

    #[test]
    fn schema_v25_restores_executor_fingerprint_receipts_cleared_by_read_probe() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.sqlite3");
        let mut store = Store::open(&path).unwrap();
        store.import_dataset(DATASET).unwrap();
        let role = store.evaluation_sync_roles().unwrap().remove(0);
        let input_hash = format!("exec-fp:{}", role.source_identity_hash);
        assert!(
            store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );
        let evaluation = test_evaluation(&role, "Research first", "High Confidence", "High");
        store
            .complete_evaluation_sync(&role, &input_hash, &evaluation)
            .unwrap();
        let receipt_key: String = store
            .connection
            .query_row(
                "SELECT current_receipt_key FROM evaluation_sync WHERE role_id = ?1",
                [&role.role_id],
                |row| row.get(0),
            )
            .unwrap();
        // Simulate an HFW-composed receipt that recorded the executor fingerprint.
        let executor_fingerprint = "f".repeat(64);
        store
            .connection
            .execute(
                "UPDATE evaluation_receipts
                    SET compatibility_fingerprint = ?1
                  WHERE receipt_key = ?2",
                params![executor_fingerprint, &receipt_key],
            )
            .unwrap();
        let stored_fp: String = store
            .connection
            .query_row(
                "SELECT compatibility_fingerprint FROM evaluation_receipts WHERE receipt_key = ?1",
                [&receipt_key],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_fp, executor_fingerprint);
        assert_eq!(
            store
                .invalidate_evaluation_compatibility_with_alternates(
                    Some(&"d".repeat(40)),
                    Some(&"e".repeat(64)),
                    Some(executor_fingerprint.as_str()),
                )
                .unwrap(),
            0
        );
        store
            .connection
            .execute(
                "UPDATE evaluation_sync
                    SET state = 'needs_attention',
                        reason = 'evaluation_compatibility_changed',
                        current_receipt_key = NULL,
                        input_hash = NULL
                  WHERE role_id = ?1",
                [&role.role_id],
            )
            .unwrap();
        store
            .connection
            .execute("UPDATE schema_meta SET version = 24", [])
            .unwrap();
        drop(store);

        let store = Store::open(&path).unwrap();
        let (state, reason, pointer): (String, String, Option<String>) = store
            .connection
            .query_row(
                "SELECT state, reason, current_receipt_key FROM evaluation_sync WHERE role_id = ?1",
                [&role.role_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(state, "needs_decision");
        assert_eq!(reason, "canonical_evaluation_verified");
        assert_eq!(pointer.as_deref(), Some(receipt_key.as_str()));
        let schema: i64 = store
            .connection
            .query_row("SELECT version FROM schema_meta", [], |row| row.get(0))
            .unwrap();
        assert_eq!(schema, 25);
    }

    #[test]
    fn legacy_machine_summary_receipt_promotes_after_history_outage() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        store.import_dataset(DATASET).unwrap();
        let role = store.evaluation_sync_roles().unwrap().remove(0);
        // The adapter's legacy Machine Summary projection is normalized before
        // it reaches the store; a prior history outage must not make that
        // recoverable receipt permanently unclaimable.
        let input_hash = format!("legacy-machine-summary-v1:{}", role.source_identity_hash);

        store
            .mark_evaluation_sync_unavailable(&role.role_id, "canonical_history_unavailable")
            .unwrap();
        assert!(store.dashboard().unwrap().roles.is_empty());
        assert!(
            store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );

        let evaluation = test_evaluation(&role, "Research first", "High Confidence", "High");
        store
            .complete_evaluation_sync(&role, &input_hash, &evaluation)
            .unwrap();
        assert_eq!(store.dashboard().unwrap().roles.len(), 1);
    }

    #[test]
    fn transient_read_failures_retry_the_same_input_with_a_bounded_attempt_budget() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        store.import_dataset(DATASET).unwrap();
        let role = store.evaluation_sync_roles().unwrap().remove(0);
        let input_hash = format!("retry:{}", role.source_identity_hash);

        for expected_attempt in 1..=MAX_EVALUATION_SYNC_ATTEMPTS {
            assert!(
                store
                    .claim_evaluation_sync(&role.role_id, &input_hash)
                    .unwrap()
            );
            store
                .hold_evaluation(
                    &role.role_id,
                    "needs_attention",
                    "evaluation_result_invalid_or_stale",
                )
                .unwrap();
            let attempt = store
                .connection
                .query_row(
                    "SELECT attempt FROM evaluation_sync WHERE role_id = ?1",
                    [&role.role_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap();
            assert_eq!(attempt, expected_attempt);
        }
        assert!(
            !store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );
        assert!(
            store
                .claim_evaluation_sync(&role.role_id, "changed-input")
                .unwrap()
        );
        let reset_attempt = store
            .connection
            .query_row(
                "SELECT attempt FROM evaluation_sync WHERE role_id = ?1",
                [&role.role_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(reset_attempt, 1);
    }

    #[test]
    fn user_retry_resets_exhausted_global_reconcile_attempt_budget() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        store.import_dataset(DATASET).unwrap();
        let role = store.evaluation_sync_roles().unwrap().remove(0);
        let input_hash = format!("retry:{}", role.source_identity_hash);

        for _ in 0..MAX_EVALUATION_SYNC_ATTEMPTS {
            assert!(
                store
                    .claim_evaluation_sync(&role.role_id, &input_hash)
                    .unwrap()
            );
            store
                .hold_evaluation(
                    &role.role_id,
                    "needs_attention",
                    "evaluation_result_invalid_or_stale",
                )
                .unwrap();
        }
        assert!(
            !store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );

        assert_eq!(
            store.reset_exhausted_global_reconcile_attempts().unwrap(),
            1
        );
        assert!(
            store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );
        let reset = store
            .connection
            .query_row(
                "SELECT state, attempt, input_hash FROM evaluation_sync WHERE role_id = ?1",
                [&role.role_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(reset.0, "syncing");
        assert_eq!(reset.1, 1);
        assert_eq!(reset.2.as_deref(), Some(input_hash.as_str()));
    }

    #[test]
    fn user_retry_does_not_reset_repair_career_ops_holds() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        store.import_dataset(DATASET).unwrap();
        let role = store.evaluation_sync_roles().unwrap().remove(0);
        let input_hash = format!("skip:{}", role.source_identity_hash);
        assert!(
            store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );
        store
            .hold_evaluation(
                &role.role_id,
                "needs_attention",
                "canonical_status_not_evaluated",
            )
            .unwrap();

        assert_eq!(
            store.reset_exhausted_global_reconcile_attempts().unwrap(),
            0
        );
        let held = store
            .connection
            .query_row(
                "SELECT reason, attempt, input_hash FROM evaluation_sync WHERE role_id = ?1",
                [&role.role_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(held.0, "canonical_status_not_evaluated");
        assert_eq!(held.1, 1);
        assert_eq!(held.2.as_deref(), Some(input_hash.as_str()));
    }

    #[test]
    fn changed_source_or_compatibility_invalidates_the_current_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        assert_eq!(store.dashboard().unwrap().roles.len(), 1);

        let changed = DATASET.replace("Remote, Europe", "Remote, Spain");
        store.import_dataset(&changed).unwrap();
        let held = store.dashboard().unwrap();
        assert!(held.roles.is_empty());
        assert_eq!(held.pre_queue_roles[0].reason, "source_identity_changed");
        assert_eq!(
            held.pre_queue_roles[0].recovery.scope,
            crate::domain::PreQueueRecoveryScope::None
        );

        allow_all_test_roles(&mut store);
        assert_eq!(store.dashboard().unwrap().roles.len(), 1);
        assert_eq!(
            store
                .invalidate_evaluation_compatibility(Some(&"d".repeat(40)), Some(&"e".repeat(64)))
                .unwrap(),
            1
        );
        let invalidated = store.dashboard().unwrap();
        assert!(invalidated.roles.is_empty());
        assert_eq!(
            invalidated.pre_queue_roles[0].reason,
            "evaluation_compatibility_changed"
        );
        assert_eq!(
            invalidated.pre_queue_roles[0].recovery.scope,
            crate::domain::PreQueueRecoveryScope::GlobalReconcile
        );
    }

    #[test]
    fn expired_evaluation_lease_is_recoverable_without_duplicate_receipts() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        store.import_dataset(DATASET).unwrap();
        let role = store.evaluation_sync_roles().unwrap().remove(0);
        let input_hash = format!("receipt:{}", role.source_identity_hash);
        assert!(
            store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );
        store
            .connection
            .execute(
                "UPDATE evaluation_sync SET lease_expires_at = '2020-01-01T00:00:00Z' WHERE role_id = ?1",
                [&role.role_id],
            )
            .unwrap();
        assert_eq!(store.recover_expired_evaluation_syncs().unwrap(), 1);
        assert!(
            store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );
        let evaluation = test_evaluation(&role, "Apply", "High Confidence", "High");
        store
            .complete_evaluation_sync(&role, &input_hash, &evaluation)
            .unwrap();
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM evaluation_receipts", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn ambiguous_history_fallback_does_not_bind_a_canonical_evaluation() {
        let record = |id| HistoryRecord {
            id,
            date: "2026-09-02".to_string(),
            company: "Northstar Tools".to_string(),
            role: "Frontend Engineer".to_string(),
            score: "4.2/5".to_string(),
            status: "Evaluated".to_string(),
            pdf: "yes".to_string(),
            report: "[report](reports/example.md)".to_string(),
            notes: String::new(),
        };
        assert!(
            super::deterministic_history_match(
                &[record(1), record(2)],
                "Northstar Tools",
                "Frontend Engineer",
                None,
            )
            .is_none()
        );
    }

    #[test]
    fn reimporting_an_occurrence_without_a_publication_date_clears_it() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let without_posted_at = DATASET.replace("          \"postedAt\": \"2026-08-28\",\n", "");

        let result = import_evaluated(&mut store, &without_posted_at);

        assert_eq!(result.updated, 1);
        assert!(store.dashboard().unwrap().roles[0].posted_at.is_none());
    }

    #[test]
    fn conflicting_source_publication_dates_are_omitted_from_the_role() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let second_source = DATASET
            .replace("source-a", "source-b")
            .replace("role-1", "role-2")
            .replace("2026-08-28", "2026-08-27");

        import_evaluated(&mut store, &second_source);

        let dashboard = store.dashboard().unwrap();
        let role = &dashboard.roles[0];
        assert_eq!(role.source_count, 2);
        assert!(role.posted_at.is_none());
    }

    #[test]
    fn queue_filters_apply_to_existing_roles_and_future_imports() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        store
            .set_queue_filters(&QueueFilters {
                role_families: vec!["Frontend".to_string()],
                seniority: vec!["Senior".to_string()],
                locations: vec!["Europe".to_string()],
                remote_allowed: true,
                require_authorization_path: true,
            })
            .unwrap();
        assert_eq!(store.dashboard().unwrap().roles.len(), 1);

        let future_backend = DATASET
            .replace("role-1", "role-2")
            .replace("Northstar Tools", "Backend Works")
            .replace("Frontend Engineer", "Backend Engineer")
            .replace(
                "northstar-tools/frontend-engineer",
                "backend-works/backend-engineer",
            );
        import_evaluated(&mut store, &future_backend);
        let dashboard = store.dashboard().unwrap();
        assert_eq!(dashboard.roles.len(), 1);
        assert_eq!(dashboard.roles[0].title, "Frontend Engineer");

        store
            .set_queue_filters(&QueueFilters {
                role_families: vec!["Backend".to_string()],
                seniority: vec!["Senior".to_string()],
                locations: vec!["Europe".to_string()],
                remote_allowed: true,
                require_authorization_path: true,
            })
            .unwrap();
        let dashboard = store.dashboard().unwrap();
        assert_eq!(dashboard.roles.len(), 1);
        assert_eq!(dashboard.roles[0].title, "Backend Engineer");
    }

    #[test]
    fn queue_filters_hide_explicit_authorization_conflicts() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let conflict = DATASET.replace(
            "Remote scope appears compatible but authorization is not verified.",
            "Candidates must already be authorized; no sponsorship is available.",
        );
        import_evaluated(&mut store, &conflict);
        store
            .set_queue_filters(&QueueFilters {
                role_families: vec!["Frontend".to_string()],
                seniority: vec!["Senior".to_string()],
                locations: vec!["Europe".to_string()],
                remote_allowed: true,
                require_authorization_path: true,
            })
            .unwrap();

        assert!(store.dashboard().unwrap().roles.is_empty());
    }

    #[test]
    fn suspicious_discovery_findings_never_reach_the_queue() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let suspicious = DATASET.replace(
            "\"uncertainty\": \"Confirm whether Spain is an eligible hiring location.\"",
            "\"uncertainty\": \"Company identity could not be verified.\",\n          \"legitimacy\": \"suspicious\"",
        );

        import_evaluated(&mut store, &suspicious);

        assert!(store.dashboard().unwrap().roles.is_empty());
    }

    #[test]
    fn legacy_suspicious_evidence_is_hidden_without_a_typed_legitimacy_value() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let suspicious = DATASET.replace(
            "Confirm whether Spain is an eligible hiring location.",
            "Possible impersonation: employer could not be verified.",
        );

        import_evaluated(&mut store, &suspicious);

        assert!(store.dashboard().unwrap().roles.is_empty());
    }

    #[test]
    fn queue_filters_use_whole_phrases_and_reject_remote_only_regions() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let internationalization = DATASET
            .replace("Frontend Engineer", "Internationalization Engineer")
            .replace(
                "northstar-tools/frontend-engineer",
                "northstar-tools/internationalization-engineer",
            );
        import_evaluated(&mut store, &internationalization);
        store
            .set_queue_filters(&QueueFilters {
                role_families: Vec::new(),
                seniority: vec!["Senior".to_string()],
                locations: vec!["Europe".to_string()],
                remote_allowed: true,
                require_authorization_path: true,
            })
            .unwrap();
        assert_eq!(store.dashboard().unwrap().roles.len(), 1);

        let remote_us = DATASET
            .replace("role-1", "role-2")
            .replace("Northstar Tools", "US Remote Co")
            .replace("Remote, Europe", "Remote, United States")
            .replace(
                "northstar-tools/frontend-engineer",
                "us-remote-co/frontend-engineer",
            );
        import_evaluated(&mut store, &remote_us);
        assert_eq!(store.dashboard().unwrap().roles.len(), 1);
    }

    #[test]
    fn preparation_retries_the_same_work_and_persists_only_artifact_references() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();

        let queued = store.begin_preparation(&role_id, "codex").unwrap();
        let queued_dashboard = store.dashboard().unwrap();
        assert!(queued_dashboard.roles.is_empty());
        assert_eq!(queued_dashboard.preparations[0].status, "queued");
        assert_eq!(queued_dashboard.preparations[0].attempt, 0);
        let first = store.claim_preparation_work().unwrap().unwrap();
        assert_eq!(first.id, queued.id);
        store
            .record_preparation_context(
                &first.id,
                &"a".repeat(64),
                "https://apply.example.test/application/1",
            )
            .unwrap();
        store
            .fail_preparation(
                &first.id,
                "provider_failed",
                "provider.invoke",
                "Provider failed.",
            )
            .unwrap();
        let failed = store.dashboard().unwrap();
        assert_eq!(failed.preparations[0].status, "action_required");
        assert_eq!(
            failed.preparations[0].error_class.as_deref(),
            Some("provider_failed")
        );

        let queued_retry = store.begin_preparation(&role_id, "codex").unwrap();
        assert_eq!(queued_retry.id, first.id);
        let retry = store.claim_preparation_work().unwrap().unwrap();
        assert_eq!(retry.id, first.id);
        store
            .record_preparation_context(
                &retry.id,
                &"a".repeat(64),
                "https://apply.example.test/application/1",
            )
            .unwrap();
        store
            .complete_preparation(
                &retry.id,
                &PreparationCompletion {
                    tracker_id: 42,
                    report_path: "reports/042-example.md",
                    report_hash: &"b".repeat(64),
                    cv_pdf_path: "output/042-example/cv.pdf",
                    cv_pdf_hash: &"c".repeat(64),
                    cv_source: "tailored_generated",
                },
            )
            .unwrap();
        let completed = store.dashboard().unwrap();
        assert!(completed.roles.is_empty());
        assert_eq!(completed.preparations[0].status, "completed");
        assert_eq!(completed.preparations[0].attempt, 2);
        assert_eq!(
            completed.preparations[0].cv_pdf_path.as_deref(),
            Some("output/042-example/cv.pdf")
        );
    }

    #[test]
    fn fresh_preparation_provider_run_starts_a_new_preparation_id() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let first = queue_and_claim(&mut store, &role_id, "codex");
        store
            .fail_preparation_with_policy(
                &first.id,
                "cv_fact_check_failed",
                "stage.fact_verification",
                "CV fact check failed — unsupported metric-like claims: 8 years",
                "fresh_preparation_provider_run",
            )
            .unwrap();

        let restarted = store.begin_preparation(&role_id, "codex").unwrap();
        assert_ne!(restarted.id, first.id);
        let failed_status: String = store
            .connection
            .query_row(
                "SELECT status FROM preparation_jobs WHERE id = ?1",
                [&first.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(failed_status, "cancelled");
        let claimed = store.claim_preparation_work().unwrap().unwrap();
        assert_eq!(claimed.id, restarted.id);
        assert_ne!(claimed.id, first.id);
        let dashboard = store.dashboard().unwrap();
        assert_eq!(dashboard.preparations.len(), 1);
        assert_eq!(dashboard.preparations[0].id, restarted.id);
        assert_eq!(dashboard.preparations[0].status, "preparing");
        assert!(dashboard.preparations.iter().all(|item| item.id != first.id));
    }

    #[test]
    fn fresh_preparation_id_policy_also_starts_a_new_preparation_id() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let first = queue_and_claim(&mut store, &role_id, "codex");
        store
            .fail_preparation_with_policy(
                &first.id,
                "context_changed",
                "commit.validate",
                "Context changed before commit.",
                "fresh_preparation_id",
            )
            .unwrap();

        let restarted = store.begin_preparation(&role_id, "codex").unwrap();
        assert_ne!(restarted.id, first.id);
    }

    #[test]
    fn cancelled_preparation_returns_role_to_not_started_without_artifacts() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let work = store.begin_preparation(&role_id, "codex").unwrap();

        assert!(
            store
                .cancel_inactive_preparation_for_role(&role_id)
                .unwrap()
        );
        assert!(
            store
                .cancel_inactive_preparation_for_role(&role_id)
                .unwrap()
        );

        let dashboard = store.dashboard().unwrap();
        assert_eq!(dashboard.roles[0].preparation_state, "not_started");
        assert!(dashboard.preparations.is_empty());
        let restarted = store.begin_preparation(&role_id, "claude").unwrap();
        assert_ne!(restarted.id, work.id);
    }

    #[test]
    fn interrupted_preparation_can_return_to_queue_without_canonical_effect() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        store.begin_preparation(&role_id, "codex").unwrap();
        store.claim_preparation_work().unwrap().unwrap();
        store.recover_interrupted_preparations().unwrap();

        let interrupted = &store.dashboard().unwrap().preparations[0];
        assert_eq!(interrupted.status, "action_required");
        assert_eq!(interrupted.error_class.as_deref(), Some("app_interrupted"));
        assert!(
            store
                .cancel_inactive_preparation_for_role(&role_id)
                .unwrap()
        );
        assert!(
            store
                .cancel_inactive_preparation_for_role(&role_id)
                .unwrap()
        );

        let dashboard = store.dashboard().unwrap();
        assert!(dashboard.preparations.is_empty());
        assert_eq!(dashboard.roles[0].preparation_state, "not_started");
        let canonical_effects: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM adapter_effects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(canonical_effects, 0);
    }

    #[test]
    fn ordinary_preparation_failure_cannot_be_cancelled_as_interrupted_work() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let work = queue_and_claim(&mut store, &role_id, "codex");
        store
            .fail_preparation(
                &work.id,
                "artifact_commit_failed",
                "preparation.result.commit",
                "The artifact could not be committed.",
            )
            .unwrap();

        assert!(
            !store
                .cancel_inactive_preparation_for_role(&role_id)
                .unwrap()
        );
        assert_eq!(
            store.dashboard().unwrap().preparations[0].status,
            "action_required"
        );
    }

    #[test]
    fn stuck_preparation_cleanup_resets_zombie_failed_prep_without_touching_evaluation() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let work = queue_and_claim(&mut store, &role_id, "codex");
        store
            .fail_preparation(
                &work.id,
                "artifact_commit_failed",
                "preparation.result.commit",
                "The artifact could not be committed.",
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE evaluation_sync SET state = 'hidden', reason = 'canonical_evaluation_not_viable'
                  WHERE role_id = ?1",
                [&role_id],
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_status = 'Evaluated', canonical_tracker_id = 99
                  WHERE id = ?1",
                [&role_id],
            )
            .unwrap();

        let candidates = store
            .list_stuck_preparation_cleanup_candidates(&[])
            .unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].role_id, role_id);
        assert_eq!(candidates[0].selection_reason, "zombie_failed_prep");

        let reset = store.reset_stuck_preparation_for_role(&role_id).unwrap();
        assert!(reset.changed);
        assert_eq!(reset.preparation_state_after, "not_started");
        assert_eq!(reset.cancelled_preparation_ids, vec![work.id.clone()]);

        let preparation_state: String = store
            .connection
            .query_row(
                "SELECT preparation_state FROM roles WHERE id = ?1",
                [&role_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(preparation_state, "not_started");
        assert!(store.dashboard().unwrap().preparations.is_empty());
        let (eval_state, tracker_id, canonical_status): (String, i64, String) = store
            .connection
            .query_row(
                "SELECT evaluation.state, r.canonical_tracker_id, r.canonical_status
                   FROM roles r
                   JOIN evaluation_sync evaluation ON evaluation.role_id = r.id
                  WHERE r.id = ?1",
                [&role_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(eval_state, "hidden");
        assert_eq!(tracker_id, 99);
        assert_eq!(canonical_status, "Evaluated");

        let again = store.reset_stuck_preparation_for_role(&role_id).unwrap();
        assert!(!again.changed);
        assert!(again.cancelled_preparation_ids.is_empty());
        assert!(
            store
                .list_stuck_preparation_cleanup_candidates(&[])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn stuck_preparation_cleanup_force_includes_needs_decision_failures() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        store
            .connection
            .execute(
                "UPDATE evaluation_sync SET state = 'needs_decision', reason = 'canonical_evaluation_verified'
                  WHERE role_id = ?1",
                [&role_id],
            )
            .unwrap();
        let work = queue_and_claim(&mut store, &role_id, "codex");
        store
            .fail_preparation(
                &work.id,
                "artifact_inspection_unavailable",
                "artifacts.inspect.v1",
                "Artifact inspection is unavailable.",
            )
            .unwrap();

        assert!(
            store
                .list_stuck_preparation_cleanup_candidates(&[])
                .unwrap()
                .is_empty(),
            "needs_decision failures are not zombies"
        );
        let forced = store
            .list_stuck_preparation_cleanup_candidates(&[&role_id])
            .unwrap();
        assert_eq!(forced.len(), 1);
        assert_eq!(forced[0].selection_reason, "force_role");

        let results = store.reset_stuck_preparations(&[&role_id]).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].changed);
        assert_eq!(
            store.dashboard().unwrap().roles[0].preparation_state,
            "not_started"
        );
        let eval_state: String = store
            .connection
            .query_row(
                "SELECT state FROM evaluation_sync WHERE role_id = ?1",
                [&role_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(eval_state, "needs_decision");
    }

    #[test]
    fn stuck_preparation_cleanup_refuses_applied_and_leaves_discarded_history() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let work = queue_and_claim(&mut store, &role_id, "codex");
        store
            .fail_preparation(
                &work.id,
                "canonical_writer_failed",
                "application.applied.confirm",
                "Canonical write failed.",
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_status = 'Applied', preparation_state = 'not_started'
                  WHERE id = ?1",
                [&role_id],
            )
            .unwrap();

        assert!(
            store
                .list_stuck_preparation_cleanup_candidates(&[&role_id])
                .unwrap()
                .is_empty()
        );
        let err = store.reset_stuck_preparation_for_role(&role_id).unwrap_err();
        assert!(
            err.to_string()
                .contains("Applied or Discarded"),
            "{err}"
        );
        let status: String = store
            .connection
            .query_row(
                "SELECT status FROM preparation_jobs WHERE id = ?1",
                [&work.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "action_required");
    }

    #[test]
    fn preparation_queue_runs_at_most_two_jobs_and_preserves_fifo_order() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        for index in 2..=3 {
            let dataset = DATASET
                .replace("role-1", &format!("role-{index}"))
                .replace("Northstar Tools", &format!("Company {index}"))
                .replace(
                    "northstar-tools/frontend-engineer",
                    &format!("company-{index}/frontend-engineer"),
                );
            import_evaluated(&mut store, &dataset);
        }
        let role_ids = store
            .dashboard()
            .unwrap()
            .roles
            .into_iter()
            .map(|role| role.id)
            .collect::<Vec<_>>();
        for role_id in &role_ids {
            store.begin_preparation(role_id, "codex").unwrap();
        }

        let first = store.claim_preparation_work().unwrap().unwrap();
        let second = store.claim_preparation_work().unwrap().unwrap();
        assert!(store.claim_preparation_work().unwrap().is_none());
        assert_ne!(first.id, second.id);

        store
            .complete_preparation(
                &second.id,
                &PreparationCompletion {
                    tracker_id: 42,
                    report_path: "reports/042-example.md",
                    report_hash: &"b".repeat(64),
                    cv_pdf_path: "output/042-example/cv.pdf",
                    cv_pdf_hash: &"c".repeat(64),
                    cv_source: "tailored_generated",
                },
            )
            .unwrap();
        assert!(
            store
                .next_preparation_for_browser_handoff()
                .unwrap()
                .is_none()
        );

        store
            .fail_preparation(
                &first.id,
                "provider_failed",
                "provider.invoke",
                "Provider failed.",
            )
            .unwrap();
        assert_eq!(
            store.next_preparation_for_browser_handoff().unwrap(),
            Some(second.id.clone())
        );
        let third = store.claim_preparation_work().unwrap().unwrap();
        assert_ne!(third.id, first.id);
        assert_ne!(third.id, second.id);
        assert_eq!(third.provider, "codex");
    }

    #[test]
    fn browser_handoff_skips_the_oldest_normalized_terminal_preparation() {
        for terminal_status in [" Applied ", " dIsCaRdEd ", " rEjEcTeD "] {
            let directory = tempfile::tempdir().unwrap();
            let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
            let first = completed_preparation(&mut store);
            let second_dataset = DATASET
                .replace("role-1", "role-2")
                .replace("Northstar Tools", "Second Company")
                .replace(
                    "northstar-tools/frontend-engineer",
                    "second-company/frontend-engineer",
                )
                .replace("https://example.test/jobs/1", "https://example.test/jobs/2");
            import_evaluated(&mut store, &second_dataset);
            let second_role_id = store
                .dashboard()
                .unwrap()
                .roles
                .into_iter()
                .find(|role| role.id != first.role_id)
                .unwrap()
                .id;
            let second = queue_and_claim(&mut store, &second_role_id, "codex");
            store
                .record_preparation_context(
                    &second.id,
                    &"d".repeat(64),
                    "https://example.test/jobs/2",
                )
                .unwrap();
            store
                .complete_preparation(
                    &second.id,
                    &PreparationCompletion {
                        tracker_id: 43,
                        report_path: "reports/043-example.md",
                        report_hash: &"e".repeat(64),
                        cv_pdf_path: "output/043-example/cv.pdf",
                        cv_pdf_hash: &"f".repeat(64),
                        cv_source: "tailored_generated",
                    },
                )
                .unwrap();
            store
                .connection
                .execute(
                    "UPDATE roles SET canonical_status = ?1 WHERE id = ?2",
                    rusqlite::params![terminal_status, first.role_id],
                )
                .unwrap();

            assert_eq!(
                store.next_preparation_for_browser_handoff().unwrap(),
                Some(second.id)
            );
        }
    }

    #[test]
    fn restart_preserves_queued_work_and_marks_only_interrupted_active_work() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let second_dataset = DATASET
            .replace("role-1", "role-2")
            .replace("Northstar Tools", "Second Company")
            .replace(
                "northstar-tools/frontend-engineer",
                "second-company/frontend-engineer",
            );
        import_evaluated(&mut store, &second_dataset);
        let role_ids = store
            .dashboard()
            .unwrap()
            .roles
            .into_iter()
            .map(|role| role.id)
            .collect::<Vec<_>>();
        for role_id in &role_ids {
            store.begin_preparation(role_id, "codex").unwrap();
        }
        let interrupted = store.claim_preparation_work().unwrap().unwrap();

        store.recover_interrupted_preparations().unwrap();

        let dashboard = store.dashboard().unwrap();
        let interrupted_summary = dashboard
            .preparations
            .iter()
            .find(|preparation| preparation.id == interrupted.id)
            .unwrap();
        assert_eq!(interrupted_summary.status, "action_required");
        assert_eq!(
            interrupted_summary.error_class.as_deref(),
            Some("app_interrupted")
        );
        assert_eq!(
            dashboard
                .preparations
                .iter()
                .filter(|preparation| preparation.status == "queued")
                .count(),
            1
        );
        assert!(store.claim_preparation_work().unwrap().is_some());
    }

    #[test]
    fn background_setting_round_trips() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();

        store.set_background_enabled(true).unwrap();

        assert!(store.dashboard().unwrap().background_enabled);
    }

    #[test]
    fn reviewed_cv_fallback_setting_round_trips_without_entering_dashboard_data() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let setting = crate::domain::CvFallbackSetting {
            path: Some("/private/example/reviewed.pdf".to_string()),
            sha256: Some("a".repeat(64)),
        };

        store.set_cv_fallback_setting(&setting).unwrap();

        let stored = store.cv_fallback_setting().unwrap();
        assert_eq!(stored.path, setting.path);
        assert_eq!(stored.sha256, setting.sha256);
        let dashboard = serde_json::to_value(store.dashboard().unwrap()).unwrap();
        assert!(!dashboard.to_string().contains("reviewed.pdf"));
    }

    #[test]
    fn evaluated_history_match_remains_queueable_only_with_its_current_receipt() {
        use crate::domain::HistoryRecord;

        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let records = vec![HistoryRecord {
            id: 42,
            date: "2026-08-30".to_string(),
            company: "Northstar Tools".to_string(),
            role: "Frontend Engineer (Remote Europe)".to_string(),
            score: "4.2/5".to_string(),
            status: "Evaluated".to_string(),
            pdf: "❌".to_string(),
            report: "—".to_string(),
            notes: String::new(),
        }];

        let result = store.reconcile_history(&records).unwrap();

        assert_eq!(result.matched, 1);
        assert_eq!(store.dashboard().unwrap().roles.len(), 1);
        assert_eq!(store.dashboard().unwrap().handled_count, 0);
    }

    #[test]
    fn history_reconciliation_accepts_a_technology_suffix() {
        assert!(super::title_matches(
            "Senior Product Engineer, Frontend",
            "Senior Product Engineer, Frontend (React, TypeScript)"
        ));
    }

    #[test]
    fn canonical_dismiss_and_undo_keep_queue_visibility_recoverable() {
        use crate::domain::HistoryRecord;

        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let discard = store.begin_discard_effect(&role_id).unwrap();
        store
            .complete_discard_effect(&role_id, &discard.idempotency_key, 42, "Discarded")
            .unwrap();
        let dismissed = store.dashboard().unwrap();
        assert!(dismissed.roles.is_empty());
        assert_eq!(dismissed.recently_dismissed.len(), 1);

        let undo = store.begin_undo_discard_effect(&role_id).unwrap();
        assert_eq!(
            undo.parent_effect_key.as_deref(),
            Some(discard.idempotency_key.as_str())
        );
        store
            .complete_undo_discard_effect(&role_id, &undo.idempotency_key, 42, "Evaluated")
            .unwrap();
        assert_eq!(store.dashboard().unwrap().roles.len(), 1);

        let mut canonical = HistoryRecord {
            id: 42,
            date: "2026-08-30".to_string(),
            company: "Northstar Tools".to_string(),
            role: "Frontend Engineer".to_string(),
            score: "N/A".to_string(),
            status: "Evaluated".to_string(),
            pdf: "❌".to_string(),
            report: "—".to_string(),
            notes: String::new(),
        };
        store
            .reconcile_history(std::slice::from_ref(&canonical))
            .unwrap();
        assert_eq!(store.dashboard().unwrap().roles.len(), 1);

        canonical.status = "Applied".to_string();
        store.reconcile_history(&[canonical]).unwrap();
        assert!(store.dashboard().unwrap().roles.is_empty());
    }

    #[test]
    fn applied_role_cannot_begin_undo_or_create_an_external_writer_effect() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let discard = store.begin_discard_effect(&role_id).unwrap();
        store
            .complete_discard_effect(&role_id, &discard.idempotency_key, 42, "Discarded")
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_status = ' Applied ' WHERE id = ?1",
                [&role_id],
            )
            .unwrap();

        let error = store.begin_undo_discard_effect(&role_id).unwrap_err();
        assert!(error.to_string().contains("Applied role"));
        let effects: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM adapter_effects
                  WHERE role_id = ?1 AND operation = 'role.discard.undo'",
                [&role_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(effects, 0);
        let discard_state: String = store
            .connection
            .query_row(
                "SELECT status FROM adapter_effects WHERE idempotency_key = ?1",
                [&discard.idempotency_key],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(discard_state, "completed");
    }

    #[test]
    fn application_dismissal_accepts_failed_work_without_committed_artifacts_and_reuses_its_effect()
    {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let preparation = queue_and_claim(&mut store, &role_id, "codex");
        store
            .fail_preparation(
                &preparation.id,
                "provider_failed",
                "provider.invoke",
                "Provider failed.",
            )
            .unwrap();

        let first = store.begin_preparation_cleanup(&preparation.id).unwrap();
        assert!(first.report_path.is_none());
        assert!(first.cv_pdf_path.is_none());
        store
            .fail_preparation_cleanup(
                &preparation.id,
                &first.effect.idempotency_key,
                "canonical_writer_failed",
            )
            .unwrap();

        let preserved = store.dashboard().unwrap();
        assert_eq!(preserved.preparations.len(), 1);
        assert_eq!(preserved.preparations[0].status, "action_required");
        let retry = store.begin_preparation_cleanup(&preparation.id).unwrap();
        assert_eq!(retry.effect.idempotency_key, first.effect.idempotency_key);
    }

    #[test]
    fn failed_application_dismissal_preserves_committed_artifact_references_and_browser_state() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let session = queue_completed_application_session(&mut store);
        let preparation_id = session.preparation_id.as_deref().unwrap();
        store
            .connection
            .execute(
                "UPDATE browser_sessions SET status = 'review_required' WHERE id = ?1",
                [&session.id],
            )
            .unwrap();

        let work = store.begin_preparation_cleanup(preparation_id).unwrap();
        assert!(work.report_path.is_some());
        assert!(work.cv_pdf_path.is_some());
        store
            .fail_preparation_cleanup(
                preparation_id,
                &work.effect.idempotency_key,
                "canonical_writer_failed",
            )
            .unwrap();

        let dashboard = store.dashboard().unwrap();
        let preserved = dashboard
            .preparations
            .iter()
            .find(|preparation| preparation.id == preparation_id)
            .unwrap();
        assert_eq!(preserved.status, "action_required");
        assert_eq!(preserved.report_path, work.report_path);
        assert_eq!(preserved.cv_pdf_path, work.cv_pdf_path);
        let preserved_sessions = store.browser_sessions().unwrap();
        assert_eq!(preserved_sessions.len(), 1);
        assert_eq!(preserved_sessions[0].id, session.id);
        assert_eq!(preserved_sessions[0].status, "review_required");
    }

    #[test]
    fn application_dismissal_is_rejected_outside_failed_and_ready_for_review_states() {
        for browser_status in [
            "waiting_for_extension",
            "inspecting",
            "filling",
            "submitted_tracking_pending",
            "applied_recorded",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
            let session = queue_completed_application_session(&mut store);
            store
                .connection
                .execute(
                    "UPDATE browser_sessions SET status = ?1 WHERE id = ?2",
                    rusqlite::params![browser_status, &session.id],
                )
                .unwrap();

            let error = store
                .begin_preparation_cleanup(session.preparation_id.as_deref().unwrap())
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("only a failed preparation or an application ready for review")
            );
            assert_eq!(store.dashboard().unwrap().preparations.len(), 1);
            assert_eq!(store.browser_sessions().unwrap().len(), 1);
        }

        let directory = tempfile::tempdir().unwrap();
        let mut queued_store = Store::open(directory.path().join("queued.sqlite3")).unwrap();
        import_evaluated(&mut queued_store, DATASET);
        let role_id = queued_store.dashboard().unwrap().roles[0].id.clone();
        let queued = queued_store.begin_preparation(&role_id, "codex").unwrap();
        assert!(
            queued_store
                .begin_preparation_cleanup(&queued.id)
                .unwrap_err()
                .to_string()
                .contains("only a failed preparation or an application ready for review")
        );
    }

    #[test]
    fn completed_application_dismissal_removes_only_its_local_work_after_canonical_success() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let session = queue_completed_application_session(&mut store);
        let preparation_id = session.preparation_id.clone().unwrap();
        store
            .connection
            .execute(
                "UPDATE browser_sessions SET status = 'review_required' WHERE id = ?1",
                [&session.id],
            )
            .unwrap();
        store
            .connection
            .execute(
                "INSERT INTO notification_outbox(
                   id, dedupe_key, title, body, status, attempts, next_attempt_at, created_at,
                   event_kind, role_id, preparation_id
                 ) VALUES ('dismiss-test', 'dismiss-test', 'Ready', 'Ready', 'pending', 0, ?1, ?1,
                           'application_ready', ?2, ?3)",
                rusqlite::params![
                    chrono::Utc::now().to_rfc3339(),
                    &session.role_id,
                    &preparation_id
                ],
            )
            .unwrap();

        let work = store.begin_preparation_cleanup(&preparation_id).unwrap();
        store
            .complete_preparation_cleanup(&work, 42, "Discarded")
            .unwrap();

        let dashboard = store.dashboard().unwrap();
        assert!(dashboard.preparations.is_empty());
        assert!(dashboard.roles.is_empty());
        assert_eq!(dashboard.recently_dismissed.len(), 1);
        assert!(store.browser_sessions().unwrap().is_empty());
        let notifications: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM notification_outbox WHERE preparation_id = ?1",
                [&preparation_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(notifications, 0);
    }

    #[test]
    fn application_dismissal_and_applied_tracking_cannot_start_together() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let session = queue_completed_application_session(&mut store);
        let preparation_id = session.preparation_id.clone().unwrap();
        store
            .connection
            .execute(
                "UPDATE browser_sessions SET status = 'review_required' WHERE id = ?1",
                [&session.id],
            )
            .unwrap();

        let cleanup = store.begin_preparation_cleanup(&preparation_id).unwrap();
        assert!(
            store
                .begin_applied_effect_for_session(&session.id)
                .unwrap_err()
                .to_string()
                .contains("dismissal is already in progress")
        );
        store
            .fail_preparation_cleanup(
                &preparation_id,
                &cleanup.effect.idempotency_key,
                "canonical_writer_failed",
            )
            .unwrap();
        let (_, applied) = store.begin_applied_effect_for_session(&session.id).unwrap();
        assert_eq!(applied.tracker_id, Some(42));
    }

    #[test]
    fn applied_outcome_survives_bounded_and_stale_history_reconciliation() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let session = queue_completed_application_session(&mut store);
        store
            .connection
            .execute(
                "UPDATE browser_sessions SET status = 'review_required' WHERE id = ?1",
                [&session.id],
            )
            .unwrap();
        let (role_id, effect) = store.begin_applied_effect_for_session(&session.id).unwrap();
        store
            .complete_applied_effect(
                &session.id,
                &role_id,
                &effect.idempotency_key,
                42,
                "Applied",
            )
            .unwrap();
        // Simulate an Applied row written by an older HereForWork build before
        // completion also normalized the evaluation lifecycle to terminal.
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_date = '2026-09-02' WHERE id = ?1",
                [&role_id],
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE evaluation_sync SET state = 'ready', reason = 'evaluation_current'
                  WHERE role_id = ?1",
                [&role_id],
            )
            .unwrap();

        let bounded_snapshot = (0..2_000)
            .map(|index| HistoryRecord {
                id: 1_000 + index,
                date: "2026-09-03".to_string(),
                company: format!("Unrelated company {index}"),
                role: format!("Unrelated role {index}"),
                score: "N/A".to_string(),
                status: "Evaluated".to_string(),
                pdf: "—".to_string(),
                report: "—".to_string(),
                notes: String::new(),
            })
            .collect::<Vec<_>>();
        let first = store.reconcile_history(&bounded_snapshot).unwrap();
        assert_eq!(first.cleared, 0);
        assert_eq!(first.matched, 0);

        let stale_matching_record = HistoryRecord {
            id: 42,
            date: "2026-08-30".to_string(),
            company: "Northstar Tools".to_string(),
            role: "Frontend Engineer".to_string(),
            score: "4.2/5".to_string(),
            status: "Evaluated".to_string(),
            pdf: "✅".to_string(),
            report: "reports/042-example.md".to_string(),
            notes: String::new(),
        };
        let second = store.reconcile_history(&[stale_matching_record]).unwrap();
        assert_eq!(second.cleared, 0);
        assert_eq!(second.matched, 1);
        let stale_applied_record = HistoryRecord {
            id: 99,
            date: "2026-08-01".to_string(),
            company: "Northstar Tools".to_string(),
            role: "Frontend Engineer".to_string(),
            score: "4.2/5".to_string(),
            status: " applied ".to_string(),
            pdf: "✅".to_string(),
            report: "reports/099-stale.md".to_string(),
            notes: String::new(),
        };
        let third = store.reconcile_history(&[stale_applied_record]).unwrap();
        assert_eq!(third.cleared, 0);
        assert_eq!(third.matched, 1);
        store.reconcile_history(&[]).unwrap();

        let canonical = store
            .connection
            .query_row(
                "SELECT canonical_tracker_id, canonical_status, canonical_date
                   FROM roles WHERE id = ?1",
                [&role_id],
                |row| {
                    Ok((
                        row.get::<_, Option<i64>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            canonical,
            (
                Some(42),
                Some("Applied".to_string()),
                Some("2026-09-02".to_string())
            )
        );
        let evaluation_state: (String, String) = store
            .connection
            .query_row(
                "SELECT state, reason FROM evaluation_sync WHERE role_id = ?1",
                [&role_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            evaluation_state,
            ("terminal".to_string(), "canonical_terminal".to_string())
        );
        assert!(store.dashboard().unwrap().roles.is_empty());
    }

    #[test]
    fn newly_reconciled_applied_retires_only_active_role_work_and_preserves_evidence() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let completed = completed_preparation(&mut store);
        store
            .configure_browser(
                "abcdefghijklmnopabcdefghijklmnop",
                "019d0000-0000-7000-8000-000000000001",
                "Profile 1",
            )
            .unwrap();
        let session = store.queue_application_session(&completed.id).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for (id, status) in [
            ("queued-evidence", "queued"),
            ("preparing-evidence", "preparing"),
        ] {
            store
                .connection
                .execute(
                    "INSERT INTO preparation_jobs(
                       id, role_id, provider, status, step, attempt, context_hash, tracker_id,
                       report_path, report_hash, cv_pdf_path, cv_pdf_hash, error_class,
                       created_at, updated_at, error_stage, error_detail, retry_policy,
                       cv_source, resolved_application_url
                     ) VALUES (
                       ?1, ?2, 'codex', ?3, ?3, 2, 'context-evidence', 77,
                       'reports/preserve.md', 'report-hash-evidence',
                       'output/preserve.pdf', 'cv-hash-evidence', 'old-error',
                       ?4, ?4, 'old-stage', 'old-detail', 'retryable',
                       'tailored_generated', 'https://example.test/jobs/1'
                     )",
                    rusqlite::params![id, completed.role_id, status, now],
                )
                .unwrap();
        }
        store
            .connection
            .execute(
                "UPDATE roles SET preparation_state = 'preparing' WHERE id = ?1",
                [&completed.role_id],
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE browser_sessions
                    SET status = 'action_required', error_code = 'old-session-error',
                        page_title = 'Preserved title', fill_results_json = '[{\"kept\":true}]',
                        driver_lease_state = 'human_handoff', fallback_eligible = 1,
                        handoff_reason = 'old-handoff'
                  WHERE id = ?1",
                [&session.id],
            )
            .unwrap();
        store
            .connection
            .execute(
                "INSERT INTO browser_commands(
                   id, session_id, command_type, payload_json, status, attempt,
                   lease_expires_at, error_code, created_at, updated_at
                 ) VALUES (
                   'leased-evidence', ?1, 'fill_plan', '{\"keep\":\"leased\"}',
                   'leased', 2, '2099-01-01T00:00:00Z', NULL, ?2, ?2
                 )",
                rusqlite::params![session.id, now],
            )
            .unwrap();
        store
            .connection
            .execute(
                "INSERT INTO browser_commands(
                   id, session_id, command_type, payload_json, status, attempt,
                   lease_expires_at, error_code, created_at, updated_at
                 ) VALUES (
                   'completed-evidence', ?1, 'release_for_review',
                   '{\"keep\":\"completed\"}', 'completed', 1, NULL,
                   'completed-marker', ?2, ?2
                 )",
                rusqlite::params![session.id, now],
            )
            .unwrap();
        let row_counts_before: (i64, i64, i64) = store
            .connection
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM preparation_jobs WHERE role_id = ?1),
                   (SELECT COUNT(*) FROM browser_sessions WHERE role_id = ?1),
                   (SELECT COUNT(*) FROM browser_commands WHERE session_id = ?2)",
                rusqlite::params![completed.role_id, session.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let applied = HistoryRecord {
            id: 42,
            date: "2026-09-03".to_string(),
            company: "Northstar Tools".to_string(),
            role: "Frontend Engineer".to_string(),
            score: "4.2/5".to_string(),
            status: " aPpLiEd ".to_string(),
            pdf: "✅".to_string(),
            report: "reports/042-example.md".to_string(),
            notes: String::new(),
        };

        store
            .reconcile_history(std::slice::from_ref(&applied))
            .unwrap();

        let preserved_preparations: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM preparation_jobs
                  WHERE id IN ('queued-evidence', 'preparing-evidence')
                    AND status = 'cancelled' AND step = 'cancelled'
                    AND error_class IS NULL AND error_stage IS NULL
                    AND error_detail IS NULL AND retry_policy IS NULL
                    AND report_path = 'reports/preserve.md'
                    AND report_hash = 'report-hash-evidence'
                    AND cv_pdf_path = 'output/preserve.pdf'
                    AND cv_pdf_hash = 'cv-hash-evidence'
                    AND context_hash = 'context-evidence'
                    AND tracker_id = 77 AND attempt = 2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(preserved_preparations, 2);
        let preserved_completed_preparation: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM preparation_jobs
                  WHERE id = ?1 AND status = 'completed'
                    AND report_path = 'reports/042-example.md'
                    AND cv_pdf_path = 'output/042-example/cv.pdf'",
                [&completed.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(preserved_completed_preparation, 1);
        let retired_commands: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM browser_commands
                  WHERE session_id = ?1 AND status = 'permanent'
                    AND error_code = 'canonical_terminal' AND lease_expires_at IS NULL
                    AND (
                      (command_type = 'inspect_request' AND payload_json LIKE '%expectedUrl%'
                        AND attempt = 0)
                      OR (id = 'leased-evidence' AND payload_json = '{\"keep\":\"leased\"}'
                        AND attempt = 2)
                    )",
                [&session.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retired_commands, 2);
        let preserved_completed_command: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM browser_commands
                  WHERE id = 'completed-evidence' AND status = 'completed'
                    AND error_code = 'completed-marker'
                    AND payload_json = '{\"keep\":\"completed\"}' AND attempt = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(preserved_completed_command, 1);
        let retired_session_and_role: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM browser_sessions b JOIN roles r ON r.id = b.role_id
                  WHERE b.id = ?1 AND b.status = 'applied_recorded'
                    AND b.error_code IS NULL AND b.driver_lease_state = 'released'
                    AND b.fallback_eligible = 0 AND b.handoff_reason = 'canonical_terminal'
                    AND b.page_title = 'Preserved title'
                    AND b.fill_results_json = '[{\"kept\":true}]'
                    AND r.canonical_status = 'Applied' AND r.preparation_state = 'prepared'",
                [&session.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retired_session_and_role, 1);
        let scoped_updated_at: (String, String, String, String) = store
            .connection
            .query_row(
                "SELECT
                   (SELECT updated_at FROM preparation_jobs WHERE id = 'queued-evidence'),
                   (SELECT updated_at FROM browser_commands WHERE id = 'leased-evidence'),
                   (SELECT updated_at FROM browser_sessions WHERE id = ?1),
                   (SELECT updated_at FROM roles WHERE id = ?2)",
                rusqlite::params![session.id, completed.role_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();

        store.reconcile_history(&[applied]).unwrap();

        let row_counts_after: (i64, i64, i64) = store
            .connection
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM preparation_jobs WHERE role_id = ?1),
                   (SELECT COUNT(*) FROM browser_sessions WHERE role_id = ?1),
                   (SELECT COUNT(*) FROM browser_commands WHERE session_id = ?2)",
                rusqlite::params![completed.role_id, session.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(row_counts_after, row_counts_before);
        let repeated_updated_at: (String, String, String, String) = store
            .connection
            .query_row(
                "SELECT
                   (SELECT updated_at FROM preparation_jobs WHERE id = 'queued-evidence'),
                   (SELECT updated_at FROM browser_commands WHERE id = 'leased-evidence'),
                   (SELECT updated_at FROM browser_sessions WHERE id = ?1),
                   (SELECT updated_at FROM roles WHERE id = ?2)",
                rusqlite::params![session.id, completed.role_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(repeated_updated_at, scoped_updated_at);
    }

    #[test]
    fn omitted_already_applied_role_retires_queued_work_without_completed_artifacts() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let queued = store.begin_preparation(&role_id, "codex").unwrap();
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_status = ' aPpLiEd ' WHERE id = ?1",
                [&role_id],
            )
            .unwrap();

        store.reconcile_history(&[]).unwrap();

        let state: (String, String, String, String) = store
            .connection
            .query_row(
                "SELECT r.canonical_status, r.preparation_state, p.status, p.step
                   FROM roles r JOIN preparation_jobs p ON p.role_id = r.id
                  WHERE r.id = ?1 AND p.id = ?2",
                rusqlite::params![role_id, queued.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            state,
            (
                "Applied".to_string(),
                "not_started".to_string(),
                "cancelled".to_string(),
                "cancelled".to_string()
            )
        );
    }

    #[test]
    fn nonterminal_history_reconciliation_does_not_retire_active_work() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let completed = completed_preparation(&mut store);
        store
            .configure_browser(
                "abcdefghijklmnopabcdefghijklmnop",
                "019d0000-0000-7000-8000-000000000001",
                "Profile 1",
            )
            .unwrap();
        let session = store.queue_application_session(&completed.id).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        store
            .connection
            .execute(
                "INSERT INTO preparation_jobs(
                   id, role_id, provider, status, step, attempt, created_at, updated_at
                 ) VALUES ('nonterminal-queued', ?1, 'codex', 'queued', 'queued', 0, ?2, ?2)",
                rusqlite::params![completed.role_id, now],
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE roles SET preparation_state = 'queued' WHERE id = ?1",
                [&completed.role_id],
            )
            .unwrap();
        let evaluated = HistoryRecord {
            id: 42,
            date: "2026-09-03".to_string(),
            company: "Northstar Tools".to_string(),
            role: "Frontend Engineer".to_string(),
            score: "4.2/5".to_string(),
            status: "Evaluated".to_string(),
            pdf: "✅".to_string(),
            report: "reports/042-example.md".to_string(),
            notes: String::new(),
        };

        store.reconcile_history(&[evaluated]).unwrap();

        let state: (String, String, String, String, String) = store
            .connection
            .query_row(
                "SELECT r.canonical_status, r.preparation_state, p.status, b.status, c.status
                   FROM roles r
                   JOIN preparation_jobs p ON p.role_id = r.id AND p.id = 'nonterminal-queued'
                   JOIN browser_sessions b ON b.role_id = r.id AND b.id = ?2
                   JOIN browser_commands c ON c.session_id = b.id
                  WHERE r.id = ?1",
                rusqlite::params![completed.role_id, session.id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            state,
            (
                "Evaluated".to_string(),
                "queued".to_string(),
                "queued".to_string(),
                "waiting_for_extension".to_string(),
                "pending".to_string()
            )
        );
    }

    #[test]
    fn missing_nonterminal_history_still_requires_fresh_canonical_evaluation() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let evaluated = HistoryRecord {
            id: 42,
            date: "2026-08-30".to_string(),
            company: "Northstar Tools".to_string(),
            role: "Frontend Engineer".to_string(),
            score: "4.2/5".to_string(),
            status: "Evaluated".to_string(),
            pdf: "✅".to_string(),
            report: "reports/042-example.md".to_string(),
            notes: String::new(),
        };
        store.reconcile_history(&[evaluated]).unwrap();

        let result = store.reconcile_history(&[]).unwrap();

        assert_eq!(result.cleared, 1);
        let state: (Option<String>, String, String) = store
            .connection
            .query_row(
                "SELECT r.canonical_status, e.state, e.reason
                   FROM roles r JOIN evaluation_sync e ON e.role_id = r.id
                  WHERE r.id = ?1",
                [&role_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            state,
            (
                None,
                "awaiting_evaluation".to_string(),
                "canonical_evaluation_missing".to_string()
            )
        );
    }

    #[test]
    fn preparation_start_and_retry_refuse_a_terminal_role() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_status = ' Applied ' WHERE id = ?1",
                [&role_id],
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE evaluation_sync SET state = 'terminal', reason = 'canonical_terminal'
                  WHERE role_id = ?1",
                [&role_id],
            )
            .unwrap();

        let error = store.begin_preparation(&role_id, "codex").unwrap_err();
        assert!(error.to_string().contains("terminal canonical outcome"));
        let job_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM preparation_jobs", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(job_count, 0);

        store
            .connection
            .execute(
                "UPDATE roles SET canonical_status = 'Evaluated' WHERE id = ?1",
                [&role_id],
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE evaluation_sync SET state = 'ready', reason = 'canonical_evaluation_verified'
                  WHERE role_id = ?1",
                [&role_id],
            )
            .unwrap();
        let preparation = queue_and_claim(&mut store, &role_id, "codex");
        store
            .fail_preparation(
                &preparation.id,
                "provider_failed",
                "preparing_report",
                "test failure",
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_status = 'Discarded' WHERE id = ?1",
                [&role_id],
            )
            .unwrap();

        let error = store.begin_preparation(&role_id, "codex").unwrap_err();
        assert!(error.to_string().contains("terminal canonical outcome"));
        let status: String = store
            .connection
            .query_row(
                "SELECT status FROM preparation_jobs WHERE id = ?1",
                [&preparation.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "action_required");
    }

    #[test]
    fn discarded_history_variants_are_normalized_and_terminal() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let discarded = HistoryRecord {
            id: 42,
            date: "2026-09-03".to_string(),
            company: "Northstar Tools".to_string(),
            role: "Frontend Engineer".to_string(),
            score: "4.2/5".to_string(),
            status: " dIsCaRdEd ".to_string(),
            pdf: "—".to_string(),
            report: "reports/042-example.md".to_string(),
            notes: String::new(),
        };

        store.reconcile_history(&[discarded]).unwrap();

        let state: (String, String) = store
            .connection
            .query_row(
                "SELECT r.canonical_status, e.state
                   FROM roles r JOIN evaluation_sync e ON e.role_id = r.id
                  WHERE r.id = ?1",
                [&role_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, ("Discarded".to_string(), "terminal".to_string()));
    }

    #[test]
    fn rejected_history_variants_are_normalized_and_terminal() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let rejected = HistoryRecord {
            id: 42,
            date: "2026-09-03".to_string(),
            company: "Northstar Tools".to_string(),
            role: "Frontend Engineer".to_string(),
            score: "4.2/5".to_string(),
            status: " rEjEcTeD ".to_string(),
            pdf: "—".to_string(),
            report: "reports/042-example.md".to_string(),
            notes: String::new(),
        };

        store.reconcile_history(&[rejected]).unwrap();

        let state: (String, String, String) = store
            .connection
            .query_row(
                "SELECT r.canonical_status, e.state, e.reason
                   FROM roles r JOIN evaluation_sync e ON e.role_id = r.id
                  WHERE r.id = ?1",
                [&role_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            state,
            (
                "Rejected".to_string(),
                "terminal".to_string(),
                "canonical_terminal".to_string()
            )
        );
        let dashboard = store.dashboard().unwrap();
        assert!(dashboard.roles.is_empty());
        assert!(
            !dashboard
                .pre_queue_roles
                .iter()
                .any(|role| role.role_id == role_id)
        );
    }

    #[test]
    fn rejected_roles_leave_needs_attention_and_count_as_handled() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_status = 'Rejected' WHERE id = ?1",
                [&role_id],
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE evaluation_sync
                    SET state = 'needs_attention', reason = 'canonical_status_not_evaluated'
                  WHERE role_id = ?1",
                [&role_id],
            )
            .unwrap();

        store
            .hold_evaluation(&role_id, "terminal", "canonical_terminal")
            .unwrap();

        let dashboard = store.dashboard().unwrap();
        assert!(
            !dashboard
                .pre_queue_roles
                .iter()
                .any(|role| role.role_id == role_id)
        );
        assert!(dashboard.roles.is_empty());
        assert_eq!(dashboard.handled_count, 1);
        let state: (String, String) = store
            .connection
            .query_row(
                "SELECT state, reason FROM evaluation_sync WHERE role_id = ?1",
                [&role_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            state,
            ("terminal".to_string(), "canonical_terminal".to_string())
        );
    }

    #[test]
    fn schema_v23_terminalizes_existing_rejected_roles() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.sqlite3");
        {
            let mut store = Store::open(&path).unwrap();
            import_evaluated(&mut store, DATASET);
            let role_id = store.dashboard().unwrap().roles[0].id.clone();
            store
                .connection
                .execute(
                    "UPDATE roles SET canonical_status = ' rejected ' WHERE id = ?1",
                    [&role_id],
                )
                .unwrap();
            store
                .connection
                .execute(
                    "UPDATE evaluation_sync
                        SET state = 'needs_attention', reason = 'canonical_status_not_evaluated'
                      WHERE role_id = ?1",
                    [&role_id],
                )
                .unwrap();
            store
                .connection
                .execute("UPDATE schema_meta SET version = 22", [])
                .unwrap();
        }

        let store = Store::open(&path).unwrap();
        let state: (String, String, String, i64) = store
            .connection
            .query_row(
                "SELECT r.canonical_status, e.state, e.reason, m.version
                   FROM roles r
                   JOIN evaluation_sync e ON e.role_id = r.id
                   CROSS JOIN schema_meta m",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            state,
            (
                "Rejected".to_string(),
                "terminal".to_string(),
                "canonical_terminal".to_string(),
                23
            )
        );
        assert!(store.dashboard().unwrap().pre_queue_roles.is_empty());
    }

    #[test]
    fn stale_evaluation_completion_cannot_overwrite_applied() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        store.import_dataset(DATASET).unwrap();
        let role = store.evaluation_sync_roles().unwrap().remove(0);
        let input_hash = format!("test:{}", role.source_identity_hash);
        assert!(
            store
                .claim_evaluation_sync(&role.role_id, &input_hash)
                .unwrap()
        );
        let evaluation = test_evaluation(&role, "Apply", "High Confidence", "High");
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_tracker_id = 99, canonical_status = 'Applied',
                        canonical_date = '2026-09-03' WHERE id = ?1",
                [&role.role_id],
            )
            .unwrap();

        assert!(
            !store
                .complete_evaluation_sync(&role, &input_hash, &evaluation)
                .unwrap()
        );

        let state: (i64, String, String) = store
            .connection
            .query_row(
                "SELECT r.canonical_tracker_id, r.canonical_status, e.state
                   FROM roles r JOIN evaluation_sync e ON e.role_id = r.id
                  WHERE r.id = ?1",
                [&role.role_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(state, (99, "Applied".to_string(), "terminal".to_string()));
        let receipt_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM evaluation_receipts WHERE role_id = ?1",
                [&role.role_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(receipt_count, 0);
    }

    #[test]
    fn stale_preparation_completion_is_rejected_after_applied() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let preparation = queue_and_claim(&mut store, &role_id, "codex");
        store
            .record_preparation_context(
                &preparation.id,
                &"a".repeat(64),
                "https://example.test/jobs/1",
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_tracker_id = 99, canonical_status = 'Applied',
                        canonical_date = '2026-09-03' WHERE id = ?1",
                [&role_id],
            )
            .unwrap();

        let error = store
            .complete_preparation(
                &preparation.id,
                &PreparationCompletion {
                    tracker_id: 42,
                    report_path: "reports/042-example.md",
                    report_hash: &"b".repeat(64),
                    cv_pdf_path: "output/042-example/cv.pdf",
                    cv_pdf_hash: &"c".repeat(64),
                    cv_source: "tailored_generated",
                },
            )
            .unwrap_err();
        assert!(error.to_string().contains("terminal canonical outcome"));

        let state: (i64, String, String, String) = store
            .connection
            .query_row(
                "SELECT r.canonical_tracker_id, r.canonical_status, r.preparation_state, p.status
                   FROM roles r JOIN preparation_jobs p ON p.role_id = r.id
                  WHERE r.id = ?1",
                [&role_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            state,
            (
                99,
                "Applied".to_string(),
                "preparing".to_string(),
                "preparing".to_string()
            )
        );
    }

    #[test]
    fn terminal_role_cannot_queue_an_application_session() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let preparation = completed_preparation(&mut store);
        store
            .configure_browser(
                "abcdefghijklmnopabcdefghijklmnop",
                "019d0000-0000-7000-8000-000000000001",
                "Profile 1",
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_status = ' Applied ' WHERE id = ?1",
                [&preparation.role_id],
            )
            .unwrap();

        let error = store
            .queue_application_session(&preparation.id)
            .unwrap_err();

        assert!(error.to_string().contains("terminal canonical outcome"));
        assert!(store.browser_sessions().unwrap().is_empty());
    }

    #[test]
    fn terminal_roles_are_defensively_excluded_from_preparation_and_browser_claims() {
        for terminal_status in [" Applied ", " discarded ", " rejected "] {
            let directory = tempfile::tempdir().unwrap();
            let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
            let session = queue_completed_application_session(&mut store);
            let preparation_id = session.preparation_id.as_deref().unwrap();
            let role_id = session.role_id.as_deref().unwrap();
            store
                .connection
                .execute(
                    "UPDATE preparation_jobs SET status = 'queued', step = 'queued', attempt = 0
                      WHERE id = ?1",
                    [preparation_id],
                )
                .unwrap();
            store
                .connection
                .execute(
                    "UPDATE roles SET canonical_status = ?1, preparation_state = 'queued'
                      WHERE id = ?2",
                    rusqlite::params![terminal_status, role_id],
                )
                .unwrap();

            assert!(store.claim_preparation_work().unwrap().is_none());
            assert!(
                store
                    .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
                    .unwrap()
                    .is_none()
            );
            let preparation: (String, i64) = store
                .connection
                .query_row(
                    "SELECT status, attempt FROM preparation_jobs WHERE id = ?1",
                    [preparation_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(preparation, ("queued".to_string(), 0));
            let browser_command: (String, i64) = store
                .connection
                .query_row(
                    "SELECT status, attempt FROM browser_commands WHERE session_id = ?1",
                    [&session.id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(browser_command, ("pending".to_string(), 0));
        }
    }

    #[test]
    fn nonterminal_roles_remain_eligible_for_preparation_and_browser_claims() {
        let preparation_directory = tempfile::tempdir().unwrap();
        let mut preparation_store =
            Store::open(preparation_directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut preparation_store, DATASET);
        let role_id = preparation_store.dashboard().unwrap().roles[0].id.clone();
        preparation_store
            .begin_preparation(&role_id, "codex")
            .unwrap();
        assert!(
            preparation_store
                .claim_preparation_work()
                .unwrap()
                .is_some()
        );

        let browser_directory = tempfile::tempdir().unwrap();
        let mut browser_store = Store::open(browser_directory.path().join("test.sqlite3")).unwrap();
        queue_completed_application_session(&mut browser_store);
        assert!(
            browser_store
                .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn discard_start_and_completion_both_refuse_applied() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let effect = store.begin_discard_effect(&role_id).unwrap();
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_tracker_id = 42, canonical_status = ' aPpLiEd '
                  WHERE id = ?1",
                [&role_id],
            )
            .unwrap();

        assert!(
            store
                .complete_discard_effect(&role_id, &effect.idempotency_key, 42, "Discarded")
                .unwrap_err()
                .to_string()
                .contains("Applied role")
        );
        assert!(
            store
                .begin_discard_effect(&role_id)
                .unwrap_err()
                .to_string()
                .contains("Applied role")
        );
        let status: String = store
            .connection
            .query_row(
                "SELECT canonical_status FROM roles WHERE id = ?1",
                [&role_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, " aPpLiEd ");
    }

    #[test]
    fn preparation_cleanup_start_and_completion_both_refuse_applied() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let preparation = completed_preparation(&mut store);
        store
            .configure_browser(
                "abcdefghijklmnopabcdefghijklmnop",
                "019d0000-0000-7000-8000-000000000001",
                "Profile 1",
            )
            .unwrap();
        let session = store.queue_application_session(&preparation.id).unwrap();
        store
            .connection
            .execute(
                "UPDATE browser_sessions SET status = 'review_required' WHERE id = ?1",
                [&session.id],
            )
            .unwrap();
        let cleanup = store.begin_preparation_cleanup(&preparation.id).unwrap();
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_status = 'Applied' WHERE id = ?1",
                [&preparation.role_id],
            )
            .unwrap();

        assert!(
            store
                .complete_preparation_cleanup(&cleanup, 42, "Discarded")
                .unwrap_err()
                .to_string()
                .contains("Applied application")
        );
        assert!(
            store
                .begin_preparation_cleanup(&preparation.id)
                .unwrap_err()
                .to_string()
                .contains("Applied application")
        );
        assert!(store.preparation_detail(&preparation.id).is_ok());
    }

    #[test]
    fn stale_second_session_cannot_start_another_applied_effect() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let first_session = queue_completed_application_session(&mut store);
        store
            .connection
            .execute(
                "UPDATE browser_sessions SET status = 'review_required' WHERE id = ?1",
                [&first_session.id],
            )
            .unwrap();
        let preparation_id = first_session.preparation_id.as_deref().unwrap();
        let stale_session = store.queue_application_session(preparation_id).unwrap();
        store
            .connection
            .execute(
                "UPDATE browser_sessions SET status = 'review_required' WHERE id = ?1",
                [&stale_session.id],
            )
            .unwrap();
        let (role_id, effect) = store
            .begin_applied_effect_for_session(&first_session.id)
            .unwrap();
        store
            .complete_applied_effect(
                &first_session.id,
                &role_id,
                &effect.idempotency_key,
                42,
                "Applied",
            )
            .unwrap();

        assert!(
            store
                .begin_applied_effect_for_session(&stale_session.id)
                .unwrap_err()
                .to_string()
                .contains("already recorded as Applied")
        );
        let effect_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM adapter_effects
                  WHERE role_id = ?1 AND operation = 'application.applied.confirm'",
                [&role_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(effect_count, 1);
    }

    #[test]
    fn schema_20_normalizes_legacy_applied_status_and_lifecycle() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("test.sqlite3");
        let mut store = Store::open(&database).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_tracker_id = 42, canonical_status = '  aPpLiEd  '
                  WHERE id = ?1",
                [&role_id],
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE evaluation_sync SET state = 'ready', reason = 'evaluation_current'
                  WHERE role_id = ?1",
                [&role_id],
            )
            .unwrap();
        store
            .connection
            .execute("UPDATE schema_meta SET version = 19", [])
            .unwrap();
        drop(store);

        let reopened = Store::open(&database).unwrap();
        let state: (String, String, String, i64) = reopened
            .connection
            .query_row(
                "SELECT r.canonical_status, e.state, e.reason, m.version
                   FROM roles r JOIN evaluation_sync e ON e.role_id = r.id
                   CROSS JOIN schema_meta m WHERE r.id = ?1",
                [&role_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            state,
            (
                "Applied".to_string(),
                "terminal".to_string(),
                "canonical_terminal".to_string(),
                23
            )
        );
    }

    #[test]
    fn applied_confirmation_is_effectively_idempotent_and_tracking_retry_never_queues_browser_work()
    {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let session = queue_completed_application_session(&mut store);
        store
            .connection
            .execute(
                "UPDATE browser_sessions SET status = 'review_required' WHERE id = ?1",
                [&session.id],
            )
            .unwrap();
        let browser_commands_before: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM browser_commands", [], |row| {
                row.get(0)
            })
            .unwrap();

        let (role_id, first) = store.begin_applied_effect_for_session(&session.id).unwrap();
        store
            .fail_adapter_effect(&first.idempotency_key, "canonical_write_failed")
            .unwrap();
        store.mark_submitted_tracking_pending(&session.id).unwrap();
        let (retry_role_id, retry) = store.begin_applied_effect_for_session(&session.id).unwrap();
        assert_eq!(retry_role_id, role_id);
        assert_eq!(retry.idempotency_key, first.idempotency_key);
        store
            .complete_applied_effect(&session.id, &role_id, &retry.idempotency_key, 42, "Applied")
            .unwrap();

        let (first_canonical_date, completed_at): (Option<String>, String) = store
            .connection
            .query_row(
                "SELECT r.canonical_date, e.updated_at
                   FROM roles r JOIN adapter_effects e ON e.role_id = r.id
                  WHERE r.id = ?1 AND e.idempotency_key = ?2",
                rusqlite::params![role_id, retry.idempotency_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let expected_date = chrono::DateTime::parse_from_rfc3339(&completed_at)
            .unwrap()
            .with_timezone(&chrono_tz::Europe::Madrid)
            .date_naive()
            .to_string();
        assert_eq!(
            first_canonical_date.as_deref(),
            Some(expected_date.as_str())
        );

        let browser_commands_after: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM browser_commands", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(browser_commands_after, browser_commands_before);
        assert_eq!(
            store.browser_session(&session.id).unwrap().status,
            "applied_recorded"
        );
        let (duplicate_role_id, duplicate) =
            store.begin_applied_effect_for_session(&session.id).unwrap();
        assert_eq!(duplicate_role_id, role_id);
        assert_eq!(duplicate.idempotency_key, retry.idempotency_key);
        store
            .connection
            .execute(
                "UPDATE roles SET canonical_date = NULL WHERE id = ?1",
                [&role_id],
            )
            .unwrap();
        store
            .complete_applied_effect(
                &session.id,
                &role_id,
                &duplicate.idempotency_key,
                42,
                "Applied",
            )
            .unwrap();
        let repaired_date: Option<String> = store
            .connection
            .query_row(
                "SELECT canonical_date FROM roles WHERE id = ?1",
                [&role_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(repaired_date.as_deref(), Some(expected_date.as_str()));
        let completed_effects: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM adapter_effects
                  WHERE role_id = ?1 AND operation = 'application.applied.confirm'
                    AND status = 'completed'",
                [&role_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(completed_effects, 1);
        let canonical_status: String = store
            .connection
            .query_row(
                "SELECT canonical_status FROM roles WHERE id = ?1",
                [&role_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(canonical_status, "Applied");
    }

    #[test]
    fn external_applied_rejects_active_preparation() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        store.begin_preparation(&role_id, "codex").unwrap();
        let err = store.begin_applied_effect_for_role(&role_id).unwrap_err();
        assert!(err.to_string().contains("Cancel active work first"));
    }

    #[test]
    fn external_applied_rejects_outcome_confirmable_browser_session() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let session = queue_completed_application_session(&mut store);
        store
            .connection
            .execute(
                "UPDATE browser_sessions SET status = 'review_required' WHERE id = ?1",
                [&session.id],
            )
            .unwrap();
        let err = store
            .begin_applied_effect_for_role(session.role_id.as_deref().unwrap())
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("confirm the open application session"));
    }

    #[test]
    fn external_applied_on_failed_prep_records_applied_and_leaves_applications() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let first = queue_and_claim(&mut store, &role_id, "codex");
        store
            .fail_preparation_with_policy(
                &first.id,
                "cv_fact_check_failed",
                "stage.fact_verification",
                "CV fact check failed — unsupported metric-like claims: 8 years",
                "fresh_preparation_provider_run",
            )
            .unwrap();

        let effect = store.begin_applied_effect_for_role(&role_id).unwrap();
        store
            .complete_applied_effect_for_role(&role_id, &effect.idempotency_key, 42, "Applied")
            .unwrap();

        let dashboard = store.dashboard().unwrap();
        assert!(dashboard.preparations.iter().all(|p| p.role_id != role_id));
        let status: String = store
            .connection
            .query_row(
                "SELECT canonical_status FROM roles WHERE id = ?1",
                [&role_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "Applied");
        let prep_status: String = store
            .connection
            .query_row(
                "SELECT status FROM preparation_jobs WHERE id = ?1",
                [&first.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(prep_status, "cancelled");
    }

    #[test]
    fn external_applied_tracking_failure_sets_pending_flag_for_retry() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let first = queue_and_claim(&mut store, &role_id, "codex");
        store
            .fail_preparation(&first.id, "provider_failed", "provider.invoke", "Provider failed.")
            .unwrap();
        let effect = store.begin_applied_effect_for_role(&role_id).unwrap();
        store
            .fail_adapter_effect(&effect.idempotency_key, "canonical_write_failed")
            .unwrap();
        let row = store
            .dashboard()
            .unwrap()
            .preparations
            .into_iter()
            .find(|p| p.role_id == role_id)
            .unwrap();
        assert!(row.applied_tracking_pending);
        let retry = store.begin_applied_effect_for_role(&role_id).unwrap();
        assert_eq!(retry.idempotency_key, effect.idempotency_key);
    }

    #[test]
    fn missed_windows_consolidate_into_one_idempotent_run() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-08-30T19:00:00+02:00")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let first = store.reconcile_due_runs(now).unwrap();
        let second = store.reconcile_due_runs(now).unwrap();

        assert_eq!(first.len(), 2);
        assert!(second.is_empty());
        let dashboard = store.dashboard().unwrap();
        assert_eq!(dashboard.pending_run_count, 0);
        // Staged `source_adapter_not_configured` catch-ups are preserved windows,
        // not actionable Queue blockers.
        assert_eq!(dashboard.action_required_run_count, 0);
        assert!(
            dashboard
                .recent_runs
                .iter()
                .all(|run| run.error_class.as_deref() == Some("source_adapter_not_configured"))
        );
    }

    #[test]
    fn staged_source_refuses_manual_execution() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();

        let error = store.queue_manual_run("frontend-role-scan").unwrap_err();

        assert!(error.to_string().contains("existing external workflow"));
    }

    #[test]
    fn active_run_leases_steps_and_advances_cursor_only_after_completion() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        store
            .set_source_execution_mode("frontend-role-scan", "active")
            .unwrap();
        let run = store.queue_manual_run("frontend-role-scan").unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-08-30T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        for expected_step in ["discover", "reconcile", "notify"] {
            let lease = store
                .claim_next_run(now, chrono::Duration::minutes(5))
                .unwrap()
                .unwrap();
            assert_eq!(lease.id, run.id);
            assert_eq!(lease.step_name, expected_step);
            store
                .complete_run_step(
                    &run.id,
                    expected_step,
                    Some("input-hash"),
                    Some("output-hash"),
                    now,
                )
                .unwrap();
        }
        store.complete_run(&run.id, 2, now).unwrap();

        let dashboard = store.dashboard().unwrap();
        assert_eq!(dashboard.pending_run_count, 0);
        assert_eq!(dashboard.recent_runs[0].status, "completed");
        let pending_notifications: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM notification_outbox WHERE status = 'pending'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pending_notifications, 1);
    }

    #[test]
    fn retryable_step_stops_after_three_attempts() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        store
            .set_source_execution_mode("frontend-role-scan", "active")
            .unwrap();
        let run = store.queue_manual_run("frontend-role-scan").unwrap();
        let now = chrono::Utc::now();

        for expected_class in ["retryable", "retryable", "permanent"] {
            let lease = store
                .claim_next_run(now, chrono::Duration::minutes(5))
                .unwrap()
                .unwrap();
            let effective = store
                .fail_run_step(&run.id, &lease.step_name, "retryable", now)
                .unwrap();
            assert_eq!(effective, expected_class);
        }

        assert!(
            store
                .claim_next_run(now, chrono::Duration::minutes(5))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store.dashboard().unwrap().recent_runs[0].status,
            "permanent"
        );
    }

    #[test]
    fn browser_connection_check_inspects_then_releases_without_a_fill_command() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        store
            .configure_browser(
                "abcdefghijklmnopabcdefghijklmnop",
                "019d0000-0000-7000-8000-000000000001",
                "Profile 1",
            )
            .unwrap();

        let queued = store.queue_browser_connection_check().unwrap();
        assert_eq!(queued.status, "waiting_for_extension");
        let inspect = store
            .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
            .unwrap()
            .unwrap();
        assert_eq!(inspect.command_type, "inspect_request");
        let fields = serde_json::json!([
            { "id": "email", "classification": "safe_verified" },
            { "id": "auth", "classification": "sensitive" }
        ]);
        let inspection = crate::domain::BrowserInspection {
            ats: "ashby".to_string(),
            page_title: "Example role".to_string(),
            page_url: "https://jobs.ashbyhq.com/acme/role".to_string(),
            snapshot_fingerprint: "a".repeat(64),
            fields,
            flow_disposition: "fillable".to_string(),
            flow_issues: serde_json::json!([]),
            safe_field_count: 1,
            needs_user_count: 1,
        };
        let inspected = store
            .complete_browser_inspection(&inspect.command_id, &inspection)
            .unwrap();
        assert_eq!(inspected.status, "releasing");

        let release = store
            .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
            .unwrap()
            .unwrap();
        assert_eq!(release.command_type, "release_for_review");
        assert_eq!(release.payload["expectedUrl"], inspection.page_url);
        let verified = store.complete_browser_release(&release.command_id).unwrap();
        assert_eq!(verified.status, "connection_verified");
        assert_eq!(verified.field_count, 2);
        let fill_commands: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM browser_commands WHERE command_type = 'fill_plan'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fill_commands, 0);
    }

    #[test]
    fn browser_lane_finishes_or_fails_one_application_before_leasing_the_next() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let second_dataset = DATASET
            .replace("role-1", "role-2")
            .replace("Northstar Tools", "Second Company")
            .replace(
                "northstar-tools/frontend-engineer",
                "second-company/frontend-engineer",
            )
            .replace("https://example.test/jobs/1", "https://example.test/jobs/2");
        import_evaluated(&mut store, &second_dataset);
        let role_ids = store
            .dashboard()
            .unwrap()
            .roles
            .into_iter()
            .map(|role| role.id)
            .collect::<Vec<_>>();
        let mut preparations = Vec::new();
        for (index, role_id) in role_ids.iter().enumerate() {
            let preparation = queue_and_claim(&mut store, role_id, "codex");
            store
                .record_preparation_context(
                    &preparation.id,
                    &"a".repeat(64),
                    &format!("https://example.test/jobs/{}", index + 1),
                )
                .unwrap();
            store
                .complete_preparation(
                    &preparation.id,
                    &PreparationCompletion {
                        tracker_id: 40 + index as i64,
                        report_path: &format!("reports/{}-example.md", index + 1),
                        report_hash: &"b".repeat(64),
                        cv_pdf_path: &format!("output/{}/cv.pdf", index + 1),
                        cv_pdf_hash: &"c".repeat(64),
                        cv_source: "tailored_generated",
                    },
                )
                .unwrap();
            preparations.push(preparation);
        }
        store
            .configure_browser(
                "abcdefghijklmnopabcdefghijklmnop",
                "019d0000-0000-7000-8000-000000000001",
                "Profile 1",
            )
            .unwrap();
        let first_session = store
            .queue_application_session(&preparations[0].id)
            .unwrap();
        let second_session = store
            .queue_application_session(&preparations[1].id)
            .unwrap();

        let first_command = store
            .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
            .unwrap()
            .unwrap();
        assert_eq!(first_command.session_id, first_session.id);
        assert!(
            store
                .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
                .unwrap()
                .is_none()
        );

        store
            .fail_browser_command(&first_command.command_id, "inspection_failed")
            .unwrap();
        let second_command = store
            .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
            .unwrap()
            .unwrap();
        assert_eq!(second_command.session_id, second_session.id);
    }

    #[test]
    fn application_session_with_no_compatible_fields_releases_driver_for_future_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let preparation = queue_and_claim(&mut store, &role_id, "codex");
        store
            .record_preparation_context(
                &preparation.id,
                &"a".repeat(64),
                "https://example.test/jobs/1",
            )
            .unwrap();
        store
            .complete_preparation(
                &preparation.id,
                &PreparationCompletion {
                    tracker_id: 42,
                    report_path: "reports/042-example.md",
                    report_hash: &"b".repeat(64),
                    cv_pdf_path: "output/042-example/cv.pdf",
                    cv_pdf_hash: &"c".repeat(64),
                    cv_source: "tailored_generated",
                },
            )
            .unwrap();
        store
            .configure_browser(
                "abcdefghijklmnopabcdefghijklmnop",
                "019d0000-0000-7000-8000-000000000001",
                "Profile 1",
            )
            .unwrap();

        store.queue_application_session(&preparation.id).unwrap();
        let inspect = store
            .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
            .unwrap()
            .unwrap();
        let inspected = store
            .complete_browser_inspection(
                &inspect.command_id,
                &crate::domain::BrowserInspection {
                    ats: "generic".to_string(),
                    page_title: "Custom application".to_string(),
                    page_url: "https://example.test/jobs/1".to_string(),
                    snapshot_fingerprint: "f".repeat(64),
                    fields: serde_json::json!([]),
                    flow_disposition: "fallback_eligible".to_string(),
                    flow_issues: serde_json::json!(["no_compatible_fields"]),
                    safe_field_count: 0,
                    needs_user_count: 0,
                },
            )
            .unwrap();
        assert_eq!(inspected.status, "action_required");
        let lease_state: (String, i64, Option<String>) = store.connection.query_row(
            "SELECT driver_lease_state, fallback_eligible, handoff_reason FROM browser_sessions WHERE id = ?1",
            [&inspected.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).unwrap();
        assert_eq!(
            lease_state,
            (
                "released".to_string(),
                1,
                Some("no_compatible_fields".to_string())
            )
        );
        assert!(
            store
                .take_in_app_outcome_notifications(5)
                .unwrap()
                .is_empty()
        );

        assert!(
            store
                .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
                .unwrap()
                .is_none()
        );
        assert!(store.claim_answer_work().unwrap().is_none());
        assert!(
            store
                .take_in_app_outcome_notifications(5)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn custom_widget_keeps_extension_lease_while_safe_native_fields_continue() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let session = queue_completed_application_session(&mut store);
        let inspect = store
            .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
            .unwrap()
            .unwrap();

        let inspected = store
            .complete_browser_inspection(
                &inspect.command_id,
                &crate::domain::BrowserInspection {
                    ats: "generic".to_string(),
                    page_title: "Custom application".to_string(),
                    page_url: "https://example.test/jobs/1".to_string(),
                    snapshot_fingerprint: "f".repeat(64),
                    fields: serde_json::json!([
                        { "id": "email", "control": "input", "required": true, "classification": "safe_verified" },
                        { "id": "country", "control": "unsupported", "required": true, "classification": "unsupported" }
                    ]),
                    flow_disposition: "human_handoff".to_string(),
                    flow_issues: serde_json::json!(["custom_widget"]),
                    safe_field_count: 1,
                    needs_user_count: 1,
                },
            )
            .unwrap();

        assert_eq!(inspected.status, "drafting_answers");
        let lease_state: (String, i64, Option<String>) = store
            .connection
            .query_row(
                "SELECT driver_lease_state, fallback_eligible, handoff_reason FROM browser_sessions WHERE id = ?1",
                [&session.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            lease_state,
            ("held".to_string(), 0, Some("custom_widget".to_string()))
        );
    }

    #[test]
    fn preparation_failure_is_sanitized_deduped_and_routed_once() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let preparation = queue_and_claim(&mut store, &role_id, "codex");
        let detail = "Failed at /Users/private/cv.md for person@example.test https://private.test/token abcdefabcdefabcdefabcdefabcdefabcdef";
        store
            .fail_preparation_with_policy(
                &preparation.id,
                "context_changed",
                "commit.validate",
                detail,
                "fresh_preparation_id",
            )
            .unwrap();
        store
            .fail_preparation_with_policy(
                &preparation.id,
                "context_changed",
                "commit.validate",
                detail,
                "fresh_preparation_id",
            )
            .unwrap();

        let summary = &store.dashboard().unwrap().preparations[0];
        assert_eq!(summary.error_class.as_deref(), Some("context_changed"));
        assert_eq!(summary.error_stage.as_deref(), Some("commit.validate"));
        assert_eq!(
            summary.retry_policy.as_deref(),
            Some("fresh_preparation_id")
        );
        let sanitized = summary.error_detail.as_deref().unwrap();
        assert!(sanitized.contains("[local path]"));
        assert!(sanitized.contains("[email]"));
        assert!(sanitized.contains("[external URL]"));
        assert!(sanitized.contains("[context id]"));
        assert!(!sanitized.contains("private.test"));

        let notifications = store.take_in_app_outcome_notifications(5).unwrap();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].event_kind, "preparation_failed");
        assert_eq!(notifications[0].action_kind, "view_details");
        assert_eq!(notifications[0].preparation_id, preparation.id);
        assert!(
            store
                .take_in_app_outcome_notifications(5)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn cancelled_preparations_do_not_queue_failure_outcomes() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let first = queue_and_claim(&mut store, &role_id, "codex");
        store.cancel_preparation(&first.id).unwrap();
        let second = queue_and_claim(&mut store, &role_id, "claude");
        store.cancel_preparation(&second.id).unwrap();
        let outcome_count: i64 = store.connection.query_row(
            "SELECT COUNT(*) FROM notification_outbox WHERE event_kind IN ('preparation_failed', 'application_ready')",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(outcome_count, 0);
    }

    #[test]
    fn native_delivery_failure_and_restart_expiry_do_not_replay_outcomes() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("test.sqlite3");
        let mut store = Store::open(&database).unwrap();
        import_evaluated(&mut store, DATASET);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let first = queue_and_claim(&mut store, &role_id, "codex");
        store
            .fail_preparation(
                &first.id,
                "provider_failed",
                "provider.invoke",
                "Provider failed.",
            )
            .unwrap();
        let native = store.claim_native_outcome_notification().unwrap().unwrap();
        store
            .finish_native_outcome_notification(&native.id, Some("permission denied"))
            .unwrap();
        assert!(
            store
                .take_in_app_outcome_notifications(5)
                .unwrap()
                .is_empty()
        );

        let retried = store.begin_preparation(&role_id, "codex").unwrap();
        let claimed = store.claim_preparation_work().unwrap().unwrap();
        assert_eq!(claimed.id, retried.id);
        store
            .fail_preparation(
                &claimed.id,
                "provider_failed",
                "provider.invoke",
                "Failed again.",
            )
            .unwrap();
        drop(store);

        let mut reopened = Store::open(&database).unwrap();
        assert_eq!(
            reopened.expire_undelivered_outcome_notifications().unwrap(),
            1
        );
        assert!(
            reopened
                .take_in_app_outcome_notifications(5)
                .unwrap()
                .is_empty()
        );
        assert!(
            reopened
                .claim_native_outcome_notification()
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn application_session_drafts_after_live_inspection_and_releases_after_verified_fill() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let dataset = DATASET.replace(
            "https://example.test/jobs/1",
            "https://jobs.ashbyhq.com/northstar/frontend",
        );
        import_evaluated(&mut store, &dataset);
        let role_id = store.dashboard().unwrap().roles[0].id.clone();
        let preparation = queue_and_claim(&mut store, &role_id, "codex");
        store
            .record_preparation_context(
                &preparation.id,
                &"a".repeat(64),
                "https://jobs.ashbyhq.com/northstar/frontend/application",
            )
            .unwrap();
        store
            .complete_preparation(
                &preparation.id,
                &PreparationCompletion {
                    tracker_id: 42,
                    report_path: "reports/042-example.md",
                    report_hash: &"b".repeat(64),
                    cv_pdf_path: "output/042-example/cv.pdf",
                    cv_pdf_hash: &"c".repeat(64),
                    cv_source: "tailored_generated",
                },
            )
            .unwrap();
        store
            .configure_browser(
                "abcdefghijklmnopabcdefghijklmnop",
                "019d0000-0000-7000-8000-000000000001",
                "Profile 1",
            )
            .unwrap();

        let session = store.queue_application_session(&preparation.id).unwrap();
        assert_eq!(session.status, "waiting_for_extension");
        let inspect = store
            .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
            .unwrap()
            .unwrap();
        assert_eq!(
            inspect.payload["expectedUrl"],
            "https://jobs.ashbyhq.com/northstar/frontend/application"
        );
        let inspection = crate::domain::BrowserInspection {
            ats: "ashby".to_string(),
            page_title: "Frontend Engineer".to_string(),
            page_url: "https://jobs.ashbyhq.com/northstar/frontend/application".to_string(),
            snapshot_fingerprint: "d".repeat(64),
            fields: serde_json::json!([
                {
                    "id": "email", "label": "Email", "control": "input", "inputType": "email",
                    "required": true, "options": [], "classification": "safe_verified", "reason": "verified"
                },
                {
                    "id": "story", "label": "Tell us about a front-end problem you solved", "control": "textarea", "inputType": "textarea",
                    "required": true, "options": [], "language": "en", "maxLength": 600, "maxWords": 100, "minSentences": 2, "maxSentences": 3,
                    "classification": "grounded_narrative", "reason": "grounded draft"
                },
                {
                    "id": "salary", "label": "Expected annual salary (EUR)", "control": "input", "inputType": "number", "inputMode": "numeric",
                    "required": true, "options": [], "classification": "compensation", "reason": "canonical preference"
                }
            ]),
            flow_disposition: "fillable".to_string(),
            flow_issues: serde_json::json!([]),
            safe_field_count: 1,
            needs_user_count: 2,
        };
        let inspected = store
            .complete_browser_inspection(&inspect.command_id, &inspection)
            .unwrap();
        assert_eq!(inspected.status, "drafting_answers");
        assert!(
            store
                .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
                .unwrap()
                .is_none()
        );

        let work = store.claim_answer_work().unwrap().unwrap();
        assert_eq!(work.snapshot_fingerprint, "d".repeat(64));
        let fill_plan = serde_json::json!({
            "protocolVersion": 1,
            "snapshotFingerprint": "d".repeat(64),
            "instructions": [
                { "fieldId": "email", "value": "verified@example.test", "classification": "safe_verified" },
                { "fieldId": "story", "value": "A grounded first sentence. A grounded second sentence.", "classification": "grounded_draft" },
                { "fieldId": "salary", "value": "52000", "classification": "canonical_preference" }
            ]
        });
        let review_items = serde_json::json!([
            {
                "fieldId": "email", "label": "Email", "decision": "fill", "answer": "verified@example.test", "provenance": ["config/profile.yml:email"]
            },
            {
                "fieldId": "story", "label": "Tell us about a front-end problem you solved", "decision": "fill_draft", "answer": "A grounded first sentence. A grounded second sentence.",
                "provenance": ["cv.md"], "draftPolicy": { "language": "en", "maxLength": 600, "maxWords": 100, "minSentences": 2, "maxSentences": 3 }
            },
            {
                "fieldId": "salary", "label": "Expected annual salary (EUR)", "decision": "fill_preference", "answer": "52000",
                "provenance": ["config/profile.yml:compensation.application_answer"]
            }
        ]);
        let cv_upload = serde_json::json!({
            "fieldId": "resume",
            "relativePath": "output/042-example/cv.pdf",
            "sha256": "c".repeat(64),
            "fileName": "Leonardo_Gomez_Frontend_Engineer.pdf",
            "mimeType": "application/pdf",
            "classification": "safe_verified"
        });
        store
            .complete_answer_work(
                &session.id,
                &"e".repeat(64),
                &fill_plan,
                &review_items,
                Some(&cv_upload),
            )
            .unwrap();
        let fill = store
            .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
            .unwrap()
            .unwrap();
        assert_eq!(fill.command_type, "fill_plan");
        assert_eq!(fill.payload["cvUpload"]["fieldId"], "resume");
        assert!(fill.payload.get("uploads").is_none());
        assert!(fill.driver_lease_id.is_some());
        assert!(
            store
                .browser_command_result_matches(
                    &fill.command_id,
                    &fill.session_id,
                    &fill.command_type,
                    fill.driver_lease_id.as_deref(),
                )
                .unwrap()
        );
        assert!(
            !store
                .browser_command_result_matches(
                    &fill.command_id,
                    &fill.session_id,
                    &fill.command_type,
                    Some("00000000-0000-4000-8000-000000000000"),
                )
                .unwrap()
        );
        assert!(store.complete_browser_fill(
            &fill.command_id,
            &serde_json::json!([
                { "fieldId": "email", "status": "verified", "reasonCode": "verified", "reason": null, "mutated": true, "readBackSha256": hash_browser_readback("verified@example.test") }
            ]),
        ).is_err());
        assert!(store.complete_browser_fill(
            &fill.command_id,
            &serde_json::json!([
                { "fieldId": "email", "status": "verified", "reasonCode": "verified", "reason": null, "mutated": true, "readBackSha256": hash_browser_readback("verified@example.test") },
                { "fieldId": "email", "status": "verified", "reasonCode": "verified", "reason": null, "mutated": true, "readBackSha256": hash_browser_readback("verified@example.test") },
                { "fieldId": "story", "status": "verified", "reasonCode": "verified", "reason": null, "mutated": true, "readBackSha256": hash_browser_readback("A grounded first sentence. A grounded second sentence.") },
                { "fieldId": "salary", "status": "verified", "reasonCode": "verified", "reason": null, "mutated": true, "readBackSha256": hash_browser_readback("52000") },
                { "fieldId": "resume", "status": "verified", "reasonCode": "verified", "reason": null, "mutated": true, "readBackSha256": "c".repeat(64) }
            ]),
        ).is_err());
        store
            .complete_browser_fill(
                &fill.command_id,
                &serde_json::json!([
                    { "fieldId": "email", "status": "verified", "reasonCode": "verified", "reason": null, "mutated": true, "readBackSha256": hash_browser_readback("verified@example.test") },
                    { "fieldId": "story", "status": "verified", "reasonCode": "verified", "reason": null, "mutated": true, "readBackSha256": hash_browser_readback("A grounded first sentence. A grounded second sentence.") },
                    { "fieldId": "salary", "status": "verified", "reasonCode": "verified", "reason": null, "mutated": true, "readBackSha256": hash_browser_readback("52000") },
                    { "fieldId": "resume", "status": "verified", "reasonCode": "verified", "reason": null, "mutated": true, "readBackSha256": "c".repeat(64) }
                ]),
            )
            .unwrap();
        assert_eq!(
            store.browser_session(&session.id).unwrap().status,
            "persisting_answers"
        );
        assert!(
            store
                .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
                .unwrap()
                .is_none()
        );
        let commit_work = store.claim_answer_commit_work().unwrap().unwrap();
        assert_eq!(commit_work.preparation_id, preparation.id);
        assert_eq!(commit_work.report_path, "reports/042-example.md");
        assert_eq!(commit_work.cv_pdf_path, "output/042-example/cv.pdf");
        assert_eq!(commit_work.review_items[0]["decision"], "fill");
        assert_eq!(commit_work.review_items[1]["decision"], "fill_draft");
        assert_eq!(commit_work.review_items[1]["provenance"][0], "cv.md");
        assert_eq!(commit_work.review_items[2]["decision"], "fill_preference");
        assert_eq!(commit_work.fill_results[0]["status"], "verified");
        assert_eq!(commit_work.fill_results.as_array().unwrap().len(), 4);
        store
            .connection
            .execute(
                "INSERT INTO browser_commands(
                   id, session_id, command_type, payload_json, status, attempt,
                   error_code, created_at, updated_at
                 ) VALUES ('historical-inspect-failure', ?1, 'inspect_request', '{}',
                           'failed', 1, 'application_tab_recovery_failed', ?2, ?2)",
                rusqlite::params![&session.id, chrono::Utc::now().to_rfc3339()],
            )
            .unwrap();
        store
            .fail_answer_commit(&session.id, "answer_persistence_failed")
            .unwrap();
        assert_eq!(
            store.retry_browser_session(&session.id).unwrap().status,
            "persisting_answers"
        );
        let retried_commit = store.claim_answer_commit_work().unwrap().unwrap();
        assert_eq!(retried_commit.context_hash, "e".repeat(64));
        store
            .complete_answer_commit(&session.id, &"e".repeat(64), &"f".repeat(64))
            .unwrap();
        let release = store
            .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
            .unwrap()
            .unwrap();
        assert_eq!(release.command_type, "release_for_review");
        assert_eq!(
            release.payload["expectedUrl"],
            "https://jobs.ashbyhq.com/northstar/frontend/application"
        );
        let review = store.complete_browser_release(&release.command_id).unwrap();
        assert_eq!(review.status, "review_required");
        assert_eq!(review.fill_results.unwrap()[0]["status"], "verified");
        assert!(
            store
                .has_review_required_application_session(&preparation.id)
                .unwrap()
        );
        let refill = store.queue_application_session(&preparation.id).unwrap();
        assert_ne!(refill.id, session.id);
        assert_eq!(refill.status, "waiting_for_extension");
        assert!(
            store
                .has_active_application_session(&preparation.id)
                .unwrap()
        );
        let refill_inspect = store
            .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
            .unwrap()
            .unwrap();
        assert_eq!(refill_inspect.command_type, "inspect_request");
        let refill_inspection = crate::domain::BrowserInspection {
            ats: "ashby".to_string(),
            page_title: "Frontend Engineer".to_string(),
            page_url: "https://jobs.ashbyhq.com/northstar/frontend/application".to_string(),
            snapshot_fingerprint: "9".repeat(64),
            fields: serde_json::json!([{
                "id": "email", "label": "Email", "control": "input", "inputType": "email",
                "required": true, "options": [], "classification": "safe_verified", "reason": "verified"
            }]),
            flow_disposition: "fillable".to_string(),
            flow_issues: serde_json::json!([]),
            safe_field_count: 1,
            needs_user_count: 0,
        };
        let reinspected = store
            .complete_browser_inspection(&refill_inspect.command_id, &refill_inspection)
            .unwrap();
        assert_eq!(reinspected.status, "drafting_answers");
        assert_eq!(reinspected.snapshot_fingerprint, Some("9".repeat(64)));
        let refill_work = store.claim_answer_work().unwrap().unwrap();
        let refill_plan = serde_json::json!({
            "protocolVersion": 1,
            "snapshotFingerprint": "9".repeat(64),
            "instructions": [{ "fieldId": "email", "value": "verified@example.test", "classification": "safe_verified" }]
        });
        store
            .complete_answer_work(
                &refill_work.session_id,
                &"8".repeat(64),
                &refill_plan,
                &serde_json::json!([]),
                None,
            )
            .unwrap();
        let refill_command = store
            .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
            .unwrap()
            .unwrap();
        let handoff = store
            .complete_browser_fill(
                &refill_command.command_id,
                &serde_json::json!([{
                    "fieldId": "email", "status": "failed", "reasonCode": "readback_mismatch",
                    "reason": "Settled read-back did not match the planned value.", "mutated": true,
                    "readBackSha256": hash_browser_readback("framework rewrite")
                }]),
            )
            .unwrap();
        assert_eq!(handoff.status, "action_required");
        assert_eq!(
            handoff.error_code.as_deref(),
            Some("required_fill_readback_failed")
        );
        assert!(store.retry_browser_session(&handoff.id).is_err());
        store.mark_submitted_tracking_pending(&session.id).unwrap();
        assert_eq!(
            store.browser_session(&session.id).unwrap().status,
            "submitted_tracking_pending"
        );
        let (role_id, effect) = store.begin_applied_effect_for_session(&session.id).unwrap();
        assert_eq!(effect.tracker_id, Some(42));
        store
            .fail_adapter_effect(&effect.idempotency_key, "canonical_write_failed")
            .unwrap();
        store.mark_submitted_tracking_pending(&session.id).unwrap();
        let (retry_role_id, retry_effect) =
            store.begin_applied_effect_for_session(&session.id).unwrap();
        assert_eq!(retry_role_id, role_id);
        assert_eq!(retry_effect.idempotency_key, effect.idempotency_key);
        store
            .complete_applied_effect(
                &session.id,
                &role_id,
                &retry_effect.idempotency_key,
                42,
                "Applied",
            )
            .unwrap();
        assert_eq!(
            store.browser_session(&session.id).unwrap().status,
            "applied_recorded"
        );
        let forbidden: i64 = store.connection.query_row(
            "SELECT COUNT(*) FROM browser_commands WHERE command_type NOT IN ('inspect_request', 'fill_plan', 'release_for_review')",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(forbidden, 0);
    }

    #[test]
    fn failed_browser_command_can_be_retried_without_duplicate_sessions() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        store
            .configure_browser(
                "abcdefghijklmnopabcdefghijklmnop",
                "019d0000-0000-7000-8000-000000000001",
                "Profile 1",
            )
            .unwrap();
        let session = store.queue_browser_connection_check().unwrap();
        let command = store
            .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
            .unwrap()
            .unwrap();
        let failed = store
            .fail_browser_command(&command.command_id, "no_active_application_page")
            .unwrap();
        assert_eq!(failed.status, "action_required");

        let retried = store.retry_browser_session(&session.id).unwrap();
        assert_eq!(retried.status, "waiting_for_extension");
        assert_eq!(store.browser_sessions().unwrap().len(), 1);
        let retry_command = store
            .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
            .unwrap()
            .unwrap();
        assert_eq!(retry_command.command_type, "inspect_request");
    }

    #[test]
    fn delayed_extension_handshake_expires_boundedly_and_retries_the_same_session() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let session = queue_completed_application_session(&mut store);
        let stalled = store
            .recover_stalled_browser_commands(
                chrono::Utc::now() + chrono::Duration::seconds(16),
                chrono::Duration::seconds(15),
            )
            .unwrap();

        assert_eq!(stalled, vec![session.id.clone()]);
        assert_eq!(
            store
                .browser_session(&session.id)
                .unwrap()
                .error_code
                .as_deref(),
            Some("extension_handshake_timeout")
        );
        store
            .queue_browser_failure_notification(
                &session.id,
                "browser.extension",
                "The approved extension did not connect in time.",
            )
            .unwrap();
        let retried = store.retry_browser_session(&session.id).unwrap();
        assert_eq!(retried.id, session.id);
        assert_eq!(retried.status, "waiting_for_extension");
        assert_eq!(store.browser_sessions().unwrap().len(), 1);
    }

    #[test]
    fn expired_command_ack_releases_after_three_bounded_leases() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let session = queue_completed_application_session(&mut store);
        let mut now = chrono::Utc::now();

        for attempt in 1..=3 {
            let command = store
                .claim_browser_command(now, chrono::Duration::seconds(30))
                .unwrap()
                .unwrap();
            assert_eq!(command.session_id, session.id);
            now += chrono::Duration::seconds(31);
            let expired = store
                .recover_stalled_browser_commands(now, chrono::Duration::seconds(15))
                .unwrap();
            if attempt < 3 {
                assert!(expired.is_empty());
            } else {
                assert_eq!(expired, vec![session.id.clone()]);
            }
        }
        assert_eq!(
            store
                .browser_session(&session.id)
                .unwrap()
                .error_code
                .as_deref(),
            Some("extension_command_expired")
        );
    }

    #[test]
    fn retry_failures_do_not_duplicate_the_session_or_failure_notification() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        let session = queue_completed_application_session(&mut store);

        for _ in 0..2 {
            let command = store
                .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
                .unwrap()
                .unwrap();
            store
                .fail_browser_command(&command.command_id, "application_tab_recovery_failed")
                .unwrap();
            if store.browser_session(&session.id).unwrap().status == "action_required" {
                let _ = store.retry_browser_session(&session.id).unwrap();
            }
        }

        assert_eq!(store.browser_sessions().unwrap().len(), 1);
        let failures: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM notification_outbox
                  WHERE browser_session_id = ?1 AND event_kind = 'preparation_failed'",
                [&session.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(failures, 1);
    }

    #[test]
    fn only_proven_pre_fill_failures_release_the_driver_for_future_retry() {
        for code in [
            "snapshot_mismatch",
            "form_drift_before_fill",
            "invalid_fill_plan_duplicate_field",
        ] {
            assert!(super::browser_failure_is_fallback_eligible(
                "fill_plan",
                code
            ));
        }
        for code in [
            "fill_restart_uncertain",
            "readback_mismatch",
            "extension_command_failed",
        ] {
            assert!(!super::browser_failure_is_fallback_eligible(
                "fill_plan",
                code
            ));
        }
    }

    #[test]
    fn diagnostics_omit_browser_page_and_field_data() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
        store
            .configure_browser(
                "abcdefghijklmnopabcdefghijklmnop",
                "019d0000-0000-7000-8000-000000000001",
                "Private Profile",
            )
            .unwrap();
        store.queue_browser_connection_check().unwrap();
        let command = store
            .claim_browser_command(chrono::Utc::now(), chrono::Duration::seconds(30))
            .unwrap()
            .unwrap();
        let inspection = crate::domain::BrowserInspection {
            ats: "ashby".to_string(),
            page_title: "Secret Employer Role".to_string(),
            page_url: "https://jobs.ashbyhq.com/private/secret-role".to_string(),
            snapshot_fingerprint: "b".repeat(64),
            fields: serde_json::json!([{
                "id": "private-answer",
                "label": "Private question",
                "classification": "sensitive"
            }]),
            flow_disposition: "fillable".to_string(),
            flow_issues: serde_json::json!([]),
            safe_field_count: 0,
            needs_user_count: 1,
        };
        store
            .complete_browser_inspection(&command.command_id, &inspection)
            .unwrap();

        let diagnostics = store.redacted_diagnostics().unwrap().to_string();
        assert!(diagnostics.contains("\"sessionCount\":1"));
        assert!(!diagnostics.contains("Secret Employer"));
        assert!(!diagnostics.contains("secret-role"));
        assert!(!diagnostics.contains("Private question"));
        assert!(!diagnostics.contains("Private Profile"));
    }

    #[test]
    fn migration_creates_a_pre_migration_backup() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("test.sqlite3");
        {
            let connection = rusqlite::Connection::open(&database_path).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE schema_meta(version INTEGER NOT NULL);
                     INSERT INTO schema_meta(version) VALUES (3);
                     CREATE TABLE source_schedules(
                       source_id TEXT PRIMARY KEY, display_name TEXT NOT NULL,
                       timezone TEXT NOT NULL, schedule_hours TEXT NOT NULL,
                       last_successful_at TEXT, enabled INTEGER NOT NULL DEFAULT 1
                     );
                     CREATE TABLE runs(
                       id TEXT PRIMARY KEY, source_id TEXT NOT NULL, kind TEXT NOT NULL,
                       coverage_start TEXT NOT NULL, coverage_end TEXT NOT NULL,
                       status TEXT NOT NULL, attempt INTEGER NOT NULL DEFAULT 0,
                       lease_expires_at TEXT, error_class TEXT, created_at TEXT NOT NULL,
                       updated_at TEXT NOT NULL, dedupe_key TEXT NOT NULL UNIQUE
                     );
                     CREATE TABLE run_steps(
                       id TEXT PRIMARY KEY, run_id TEXT NOT NULL, name TEXT NOT NULL,
                       status TEXT NOT NULL, attempt INTEGER NOT NULL DEFAULT 0,
                       input_hash TEXT, output_hash TEXT, error_class TEXT,
                       updated_at TEXT NOT NULL, UNIQUE(run_id, name)
                     );
                     CREATE TABLE notification_outbox(
                       id TEXT PRIMARY KEY, dedupe_key TEXT NOT NULL UNIQUE,
                       title TEXT NOT NULL, body TEXT NOT NULL,
                       status TEXT NOT NULL DEFAULT 'pending', attempts INTEGER NOT NULL DEFAULT 0,
                       next_attempt_at TEXT NOT NULL, created_at TEXT NOT NULL
                     );",
                )
                .unwrap();
        }

        let _store = Store::open(&database_path).unwrap();

        let backup_count = std::fs::read_dir(directory.path().join("migration-backups"))
            .unwrap()
            .count();
        assert_eq!(backup_count, 1);
    }

    #[test]
    fn version_17_receipts_are_held_until_canonical_risk_is_refreshed() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("test.sqlite3");
        {
            let connection = rusqlite::Connection::open(&database_path).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE schema_meta(version INTEGER NOT NULL);
                     INSERT INTO schema_meta(version) VALUES (17);
                     CREATE TABLE evaluation_receipts(
                       receipt_key TEXT PRIMARY KEY, role_id TEXT NOT NULL,
                       tracker_id INTEGER NOT NULL, report_path TEXT NOT NULL,
                       report_hash TEXT NOT NULL, upstream_revision TEXT NOT NULL,
                       compatibility_fingerprint TEXT NOT NULL,
                       source_identity_hash TEXT NOT NULL, native_score REAL NOT NULL,
                       final_decision TEXT NOT NULL, legitimacy TEXT NOT NULL,
                       strengths_json TEXT NOT NULL, blockers_json TEXT NOT NULL,
                       gaps_json TEXT NOT NULL, compensation TEXT,
                       authorization_confidence TEXT NOT NULL,
                       authorization_question TEXT NOT NULL,
                       material_uncertainty_json TEXT NOT NULL, created_at TEXT NOT NULL
                     );
                     CREATE TABLE evaluation_sync(
                       role_id TEXT PRIMARY KEY, state TEXT NOT NULL, reason TEXT NOT NULL,
                       attempt INTEGER NOT NULL DEFAULT 0, input_hash TEXT,
                       current_receipt_key TEXT, lease_expires_at TEXT,
                       created_at TEXT NOT NULL, updated_at TEXT NOT NULL
                     );
                     CREATE TABLE browser_sessions(
                       id TEXT PRIMARY KEY, purpose TEXT NOT NULL, role_id TEXT,
                       status TEXT NOT NULL, ats TEXT, page_title TEXT, page_url TEXT,
                       snapshot_fingerprint TEXT, fields_json TEXT,
                       field_count INTEGER NOT NULL DEFAULT 0,
                       safe_field_count INTEGER NOT NULL DEFAULT 0,
                       needs_user_count INTEGER NOT NULL DEFAULT 0, error_code TEXT,
                       created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
                       preparation_id TEXT, provider TEXT, answers_context_hash TEXT,
                       review_items_json TEXT, fill_results_json TEXT, answers_report_hash TEXT
                     );
                     INSERT INTO evaluation_receipts VALUES (
                       'receipt-1', 'role-1', 42, 'reports/42.md', 'hash', 'revision',
                       'fingerprint', 'source-hash', 4.2, 'Apply', 'High Confidence',
                       '[]', '[]', '[]', NULL, 'interesting', 'Confirm employing entity',
                       '{}', '2026-09-01T12:00:00Z'
                     );
                     INSERT INTO evaluation_sync VALUES (
                       'role-1', 'ready', 'canonical_evaluation_verified', 1, 'input-hash',
                       'receipt-1', NULL, '2026-09-01T12:00:00Z', '2026-09-01T12:00:00Z'
                     );",
                )
                .unwrap();
        }

        let store = Store::open(&database_path).unwrap();
        let (state, reason, receipt): (String, String, Option<String>) = store
            .connection
            .query_row(
                "SELECT state, reason, current_receipt_key FROM evaluation_sync WHERE role_id = 'role-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(state, "needs_attention");
        assert_eq!(reason, "canonical_evaluation_requires_refresh");
        assert_eq!(receipt, None);
    }
}
