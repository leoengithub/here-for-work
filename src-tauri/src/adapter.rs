use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    CareerOpsCapabilityManifest, EvaluationResultRead, HistoryRecord, QueueFilters,
};

#[derive(Debug, Clone)]
pub struct AdapterConfig {
    pub node_path: PathBuf,
    pub script_path: PathBuf,
    pub career_ops_root: PathBuf,
    pub tracker_index_path: PathBuf,
    pub staging_path: PathBuf,
    pub claude_path: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("career-ops adapter could not start: {0}")]
    Start(#[from] std::io::Error),
    #[error("career-ops adapter timed out")]
    Timeout,
    #[error("career-ops adapter exited with status {0}: {1}")]
    Exit(i32, String),
    #[error("career-ops adapter returned invalid data: {0}")]
    InvalidData(String),
    #[error("career-ops adapter rejected the request at {stage}: {code}: {message}")]
    Rejected {
        code: String,
        stage: String,
        retry_policy: String,
        message: String,
    },
}

#[derive(Debug, Deserialize)]
struct ResponseEnvelope {
    id: String,
    ok: bool,
    result: Option<Value>,
    error: Option<ResponseError>,
}

#[derive(Debug, Deserialize)]
struct ResponseError {
    #[serde(default = "default_adapter_error_code")]
    code: String,
    #[serde(default)]
    stage: Option<String>,
    #[serde(default, rename = "retryPolicy")]
    retry_policy: Option<String>,
    message: String,
}

fn default_adapter_error_code() -> String {
    "adapter_error".to_string()
}

impl AdapterError {
    pub fn preparation_failure(&self, fallback_stage: &str) -> (String, String, String, String) {
        match self {
            Self::Rejected {
                code,
                stage,
                retry_policy,
                message,
            } => (
                code.clone(),
                stage.clone(),
                message.clone(),
                retry_policy.clone(),
            ),
            _ => (
                "adapter_error".to_string(),
                fallback_stage.to_string(),
                self.to_string(),
                "retry_same_preparation".to_string(),
            ),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistorySnapshot {
    records: Vec<HistoryRecord>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationResultReadInput {
    pub report_path: String,
    pub report_sha256: String,
    pub tracker_id: i64,
    pub compatibility_fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalRoleInput {
    pub idempotency_key: String,
    pub event_date: String,
    pub company: String,
    pub title: String,
    pub location: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalEffect {
    pub idempotency_key: String,
    pub tracker_id: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationRoleInput {
    pub preparation_id: String,
    pub company: String,
    pub title: String,
    pub location: String,
    pub url: String,
    pub tracker_id: i64,
    pub report_path: String,
    pub report_sha256: String,
    pub upstream_revision: String,
    pub evaluation_compatibility_fingerprint: String,
    pub artifact_compatibility_fingerprint: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationContext {
    pub preparation_id: String,
    pub context_hash: String,
    pub prompt: String,
    pub job: Value,
    pub canonical_evaluation: Value,
    pub evaluation_gate: PreparationEvaluationGate,
    pub artifact_plan: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationEvaluationGate {
    pub score: f64,
    pub legitimacy: String,
    pub authorization_confidence: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecoveredPreparationResult {
    outcome: String,
    result: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactReference {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationArtifacts {
    pub report: ArtifactReference,
    pub cv_html: ArtifactReference,
    pub cv_pdf: ArtifactReference,
    pub cv_changes: ArtifactReference,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationCvProvenance {
    pub source: String,
    pub tailored: bool,
    pub source_sha256: Option<String>,
    pub render_recovery: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationWarning {
    pub code: String,
    pub stage: String,
    pub recovered_by: String,
    pub detail: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationCommit {
    pub preparation_id: String,
    pub context_hash: String,
    pub tracker_id: i64,
    pub artifacts: PreparationArtifacts,
    pub cv_provenance: PreparationCvProvenance,
    #[serde(default)]
    pub warnings: Vec<PreparationWarning>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerContext {
    pub preparation_id: String,
    pub context_hash: String,
    pub prompt: String,
    pub snapshot_fingerprint: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatedAnswers {
    pub preparation_id: String,
    pub context_hash: String,
    pub fill_plan: Value,
    pub review_items: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommittedAnswers {
    pub preparation_id: String,
    pub context_hash: String,
    pub report: ArtifactReference,
}

#[derive(Debug, Deserialize)]
struct CanonicalEffectResult {
    outcome: String,
    effect: CanonicalEffect,
}

#[derive(Debug, Clone)]
pub struct AdapterHealth {
    pub ready: bool,
    pub capabilities: CareerOpsCapabilityManifest,
}

impl AdapterConfig {
    pub fn request(&self, operation: &str, input: Value) -> Result<Value, AdapterError> {
        let timeout = if operation == "evaluation.full_ag.run.v1" {
            Duration::from_secs(3_600)
        } else if operation.starts_with("preparation.") || operation.starts_with("answers.") {
            Duration::from_secs(240)
        } else {
            Duration::from_secs(15)
        };
        self.request_with_timeout(operation, input, timeout)
    }

    fn request_with_timeout(
        &self,
        operation: &str,
        input: Value,
        timeout: Duration,
    ) -> Result<Value, AdapterError> {
        self.request_with_timeout_and_environment(operation, input, timeout, None)
    }

    fn request_with_timeout_and_environment(
        &self,
        operation: &str,
        input: Value,
        timeout: Duration,
        fallback_configuration: Option<&Value>,
    ) -> Result<Value, AdapterError> {
        let mut command = Command::new(&self.node_path);
        command
            .arg(&self.script_path)
            .current_dir(&self.career_ops_root)
            .env("HFW_CAREER_OPS_ROOT", &self.career_ops_root)
            .env("HFW_CAREER_OPS_INDEX", &self.tracker_index_path)
            .env("HFW_CAREER_OPS_STAGING", &self.staging_path)
            .env(
                "PATH",
                adapter_child_path(std::env::var_os("PATH"), self.claude_path.as_deref()),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(configuration) = fallback_configuration {
            command.env("HFW_USER_REVIEWED_CV_FALLBACK", configuration.to_string());
        }
        let mut child = command.spawn()?;

        let request_id = Uuid::new_v4().to_string();
        let request = json!({
            "id": request_id,
            "protocolVersion": 1,
            "operation": operation,
            "input": input,
        });
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(request.to_string().as_bytes())?;
            stdin.write_all(b"\n")?;
        }

        let stdout = child.stdout.take().expect("piped stdout is available");
        let stderr = child.stderr.take().expect("piped stderr is available");
        let stdout_reader = thread::spawn(move || read_all(stdout));
        let stderr_reader = thread::spawn(move || read_all(stderr));
        let deadline = Instant::now() + timeout;

        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AdapterError::Timeout);
            }
            thread::sleep(Duration::from_millis(20));
        };

        let stdout = stdout_reader
            .join()
            .map_err(|_| AdapterError::InvalidData("stdout reader failed".to_string()))??;
        let stderr = stderr_reader
            .join()
            .map_err(|_| AdapterError::InvalidData("stderr reader failed".to_string()))??;
        if !status.success() {
            return Err(AdapterError::Exit(
                status.code().unwrap_or(-1),
                String::from_utf8_lossy(&stderr).trim().to_string(),
            ));
        }

        let response: ResponseEnvelope = serde_json::from_slice(&stdout)
            .map_err(|error| AdapterError::InvalidData(error.to_string()))?;
        if response.id != request_id {
            return Err(AdapterError::InvalidData(
                "response id does not match the request".to_string(),
            ));
        }
        if !response.ok {
            let error = response.error.unwrap_or(ResponseError {
                code: "adapter_error".to_string(),
                stage: None,
                retry_policy: None,
                message: "unknown adapter error".to_string(),
            });
            return Err(AdapterError::Rejected {
                code: error.code,
                stage: error.stage.unwrap_or_else(|| operation.to_string()),
                retry_policy: error
                    .retry_policy
                    .unwrap_or_else(|| "retry_same_preparation".to_string()),
                message: error.message,
            });
        }
        response
            .result
            .ok_or_else(|| AdapterError::InvalidData("response result is missing".to_string()))
    }

    pub fn capabilities(&self) -> Result<CareerOpsCapabilityManifest, AdapterError> {
        let result = self.request("capabilities.get", json!({}))?;
        let manifest: CareerOpsCapabilityManifest = serde_json::from_value(result)
            .map_err(|error| AdapterError::InvalidData(error.to_string()))?;
        manifest.validate().map_err(AdapterError::InvalidData)?;
        Ok(manifest)
    }

    pub fn health(
        &self,
        fallback_configuration: Option<&Value>,
    ) -> Result<AdapterHealth, AdapterError> {
        let capabilities_before = self.capabilities()?;
        let result = self.request_with_timeout_and_environment(
            "health.check",
            json!({}),
            Duration::from_secs(15),
            fallback_configuration,
        )?;
        let capabilities = self.capabilities()?;
        if capabilities_before.upstream_revision != capabilities.upstream_revision {
            return Err(AdapterError::InvalidData(
                "career-ops revision changed during compatibility checks".to_string(),
            ));
        }
        let ready = result
            .get("ready")
            .and_then(Value::as_bool)
            .ok_or_else(|| {
                AdapterError::InvalidData("health result is missing ready".to_string())
            })?;
        Ok(AdapterHealth {
            ready,
            capabilities,
        })
    }

    pub fn history_snapshot(&self) -> Result<Vec<HistoryRecord>, AdapterError> {
        let result = self.request("history.snapshot", json!({ "limit": 2000 }))?;
        let snapshot: HistorySnapshot = serde_json::from_value(result)
            .map_err(|error| AdapterError::InvalidData(error.to_string()))?;
        Ok(snapshot.records)
    }

    pub fn evaluation_result_read(
        &self,
        input: &EvaluationResultReadInput,
    ) -> Result<EvaluationResultRead, AdapterError> {
        let result = self.request(
            "evaluation.result.read.v1",
            serde_json::to_value(input).expect("evaluation result input serializes"),
        )?;
        serde_json::from_value(result).map_err(|error| AdapterError::InvalidData(error.to_string()))
    }

    pub fn evaluation_full_ag_run(
        &self,
        url: &str,
        company: &str,
        title: &str,
        compatibility_fingerprint: &str,
    ) -> Result<EvaluationResultRead, AdapterError> {
        let result = self.request(
            "evaluation.full_ag.run.v1",
            json!({
                "url": url,
                "company": company,
                "title": title,
                "compatibilityFingerprint": compatibility_fingerprint,
                "source": "HereForWork",
            }),
        )?;
        serde_json::from_value(result).map_err(|error| AdapterError::InvalidData(error.to_string()))
    }

    pub fn evaluation_result_input(
        &self,
        record: &HistoryRecord,
        compatibility_fingerprint: &str,
    ) -> Result<EvaluationResultReadInput, AdapterError> {
        let link = strict_tracker_report_link(&record.report)?;
        let root = std::fs::canonicalize(&self.career_ops_root).map_err(|error| {
            AdapterError::InvalidData(format!("career-ops root is unavailable: {error}"))
        })?;
        let tracker = if self.career_ops_root.join("data/applications.md").is_file() {
            self.career_ops_root.join("data/applications.md")
        } else {
            self.career_ops_root.join("applications.md")
        };
        let tracker = std::fs::canonicalize(tracker).map_err(|error| {
            AdapterError::InvalidData(format!("canonical tracker is unavailable: {error}"))
        })?;
        ensure_inside_root(&root, &tracker, "canonical tracker")?;
        let report = std::fs::canonicalize(
            tracker
                .parent()
                .ok_or_else(|| {
                    AdapterError::InvalidData("canonical tracker has no parent".to_string())
                })?
                .join(link),
        )
        .map_err(|error| {
            AdapterError::InvalidData(format!(
                "canonical evaluation report is unavailable: {error}"
            ))
        })?;
        ensure_inside_root(&root, &report, "canonical evaluation report")?;
        let metadata = std::fs::metadata(&report).map_err(|error| {
            AdapterError::InvalidData(format!(
                "canonical evaluation report is unavailable: {error}"
            ))
        })?;
        if !metadata.is_file() || metadata.len() > 500_000 {
            return Err(AdapterError::InvalidData(
                "canonical evaluation report is not a bounded regular file".to_string(),
            ));
        }
        let bytes = std::fs::read(&report)?;
        let relative = report
            .strip_prefix(&root)
            .map_err(|_| {
                AdapterError::InvalidData(
                    "canonical evaluation report escaped career-ops".to_string(),
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        Ok(EvaluationResultReadInput {
            report_path: relative,
            report_sha256: format!("{:x}", Sha256::digest(bytes)),
            tracker_id: record.id,
            compatibility_fingerprint: compatibility_fingerprint.to_string(),
        })
    }

    pub fn queue_filter_defaults(&self) -> Result<QueueFilters, AdapterError> {
        let result = self.request("profile.queue_filters.get", json!({}))?;
        serde_json::from_value(result).map_err(|error| AdapterError::InvalidData(error.to_string()))
    }

    pub fn preparation_context(
        &self,
        input: &PreparationRoleInput,
    ) -> Result<PreparationContext, AdapterError> {
        let result = self.request(
            "preparation.context.get",
            serde_json::to_value(input).expect("preparation role input serializes"),
        )?;
        serde_json::from_value(result).map_err(|error| AdapterError::InvalidData(error.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn commit_preparation(
        &self,
        input: &PreparationRoleInput,
        event_date: &str,
        job: Value,
        result: Value,
        fallback_configuration: Option<&Value>,
        canonical_evaluation: Value,
        artifact_plan: Value,
    ) -> Result<PreparationCommit, AdapterError> {
        // Adapter allowlist for preparation.result.commit is narrower than
        // PreparationRoleInput (used by preparation.context.get). Do not dump
        // tracker/report/fingerprint fields into the commit payload.
        let payload = json!({
            "preparationId": input.preparation_id,
            "eventDate": event_date,
            "company": input.company,
            "title": input.title,
            "location": input.location,
            "url": input.url,
            "job": job,
            "result": result,
            "canonicalEvaluation": canonical_evaluation,
            "artifactPlan": artifact_plan,
        });
        let value = self.request_with_timeout_and_environment(
            "preparation.result.commit",
            payload,
            Duration::from_secs(240),
            fallback_configuration,
        )?;
        serde_json::from_value(value).map_err(|error| AdapterError::InvalidData(error.to_string()))
    }

    pub fn recover_preparation_result(
        &self,
        preparation_id: &str,
        context_hash: &str,
    ) -> Result<Option<Value>, AdapterError> {
        let value = self.request(
            "preparation.result.recover",
            json!({
                "preparationId": preparation_id,
                "contextHash": context_hash,
            }),
        )?;
        let recovered: RecoveredPreparationResult = serde_json::from_value(value)
            .map_err(|error| AdapterError::InvalidData(error.to_string()))?;
        match recovered.outcome.as_str() {
            "completed" => recovered.result.map(Some).ok_or_else(|| {
                AdapterError::InvalidData("recovered preparation result is missing".to_string())
            }),
            "missing" if recovered.result.is_none() => Ok(None),
            _ => Err(AdapterError::InvalidData(
                "recovered preparation result has an invalid outcome".to_string(),
            )),
        }
    }

    pub fn delete_preparation_artifacts(
        &self,
        preparation_id: &str,
        report_path: Option<&str>,
        cv_pdf_path: Option<&str>,
    ) -> Result<(), AdapterError> {
        let result = self.request(
            "preparation.artifacts.delete",
            json!({
                "preparationId": preparation_id,
                "reportPath": report_path,
                "cvPdfPath": cv_pdf_path,
            }),
        )?;
        if result.get("outcome").and_then(Value::as_str) != Some("completed") {
            return Err(AdapterError::InvalidData(
                "artifact cleanup result is incomplete".to_string(),
            ));
        }
        Ok(())
    }

    pub fn answer_context(
        &self,
        preparation_id: &str,
        report_path: &str,
        snapshot: Value,
    ) -> Result<AnswerContext, AdapterError> {
        let value = self.request(
            "answers.context.get",
            json!({
                "preparationId": preparation_id,
                "reportPath": report_path,
                "snapshot": snapshot,
            }),
        )?;
        serde_json::from_value(value).map_err(|error| AdapterError::InvalidData(error.to_string()))
    }

    pub fn validate_answers(
        &self,
        preparation_id: &str,
        report_path: &str,
        snapshot: Value,
        result: Value,
    ) -> Result<ValidatedAnswers, AdapterError> {
        let value = self.request(
            "answers.result.validate",
            json!({
                "preparationId": preparation_id,
                "reportPath": report_path,
                "snapshot": snapshot,
                "result": result,
            }),
        )?;
        serde_json::from_value(value).map_err(|error| AdapterError::InvalidData(error.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn commit_answers(
        &self,
        preparation_id: &str,
        report_path: &str,
        cv_pdf_path: &str,
        context_hash: &str,
        review_items: Value,
        fill_results: Value,
        event_date: &str,
    ) -> Result<CommittedAnswers, AdapterError> {
        let value = self.request(
            "answers.result.commit",
            json!({
                "preparationId": preparation_id,
                "reportPath": report_path,
                "cvPdfPath": cv_pdf_path,
                "contextHash": context_hash,
                "reviewItems": review_items,
                "fillResults": fill_results,
                "eventDate": event_date,
            }),
        )?;
        serde_json::from_value(value).map_err(|error| AdapterError::InvalidData(error.to_string()))
    }

    pub fn discard_role(
        &self,
        input: &CanonicalRoleInput,
    ) -> Result<CanonicalEffect, AdapterError> {
        self.canonical_effect(
            "role.discard",
            serde_json::to_value(input).expect("canonical role input serializes"),
        )
    }

    pub fn confirm_applied(
        &self,
        input: &CanonicalRoleInput,
        tracker_id: i64,
    ) -> Result<CanonicalEffect, AdapterError> {
        let mut payload = serde_json::to_value(input)
            .expect("canonical role input serializes")
            .as_object()
            .cloned()
            .expect("canonical role input is an object");
        payload.insert("trackerId".to_string(), Value::Number(tracker_id.into()));
        payload.insert("userConfirmed".to_string(), Value::Bool(true));
        self.canonical_effect("application.applied.confirm", Value::Object(payload))
    }

    pub fn undo_discard(
        &self,
        idempotency_key: &str,
        discard_effect_key: &str,
        tracker_id: i64,
        event_date: &str,
    ) -> Result<CanonicalEffect, AdapterError> {
        self.canonical_effect(
            "role.discard.undo",
            json!({
                "idempotencyKey": idempotency_key,
                "discardEffectKey": discard_effect_key,
                "trackerId": tracker_id,
                "eventDate": event_date,
            }),
        )
    }

    fn canonical_effect(
        &self,
        operation: &str,
        input: Value,
    ) -> Result<CanonicalEffect, AdapterError> {
        let result = self.request(operation, input)?;
        let result: CanonicalEffectResult = serde_json::from_value(result)
            .map_err(|error| AdapterError::InvalidData(error.to_string()))?;
        if result.outcome != "completed" || result.effect.idempotency_key.is_empty() {
            return Err(AdapterError::InvalidData(
                "canonical effect did not complete".to_string(),
            ));
        }
        Ok(result.effect)
    }
}

fn strict_tracker_report_link(value: &str) -> Result<&str, AdapterError> {
    let value = value.trim();
    let marker = value.find("](").ok_or_else(|| {
        AdapterError::InvalidData("canonical tracker report link is missing".to_string())
    })?;
    if !value.starts_with('[')
        || !value.ends_with(')')
        || value[marker + 2..value.len() - 1].contains(['(', ')'])
    {
        return Err(AdapterError::InvalidData(
            "canonical tracker report link is ambiguous".to_string(),
        ));
    }
    let link = &value[marker + 2..value.len() - 1];
    if link.trim().is_empty() || Path::new(link).is_absolute() {
        return Err(AdapterError::InvalidData(
            "canonical tracker report link must be relative".to_string(),
        ));
    }
    Ok(link)
}

fn ensure_inside_root(root: &Path, candidate: &Path, label: &str) -> Result<(), AdapterError> {
    if candidate == root || !candidate.starts_with(root) {
        return Err(AdapterError::InvalidData(format!(
            "{label} must stay inside the career-ops root"
        )));
    }
    Ok(())
}

fn read_all(mut reader: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn adapter_child_path(existing: Option<OsString>, claude_path: Option<&Path>) -> OsString {
    let mut paths = Vec::new();
    if let Some(parent) = claude_path.and_then(Path::parent) {
        if !parent.as_os_str().is_empty() {
            paths.push(parent.to_path_buf());
        }
    }
    if let Some(existing) = existing {
        for path in std::env::split_paths(&existing) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    std::env::join_paths(paths).unwrap_or_else(|_| OsString::from("/usr/bin:/bin"))
}

pub fn discover_executable(home: &std::path::Path, name: &str) -> Option<PathBuf> {
    let mut candidates: HashMap<&str, Vec<PathBuf>> = HashMap::new();
    candidates.insert(
        "node",
        vec![
            home.join(".nvm/versions/node/v22.18.0/bin/node"),
            PathBuf::from("/opt/homebrew/bin/node"),
            PathBuf::from("/usr/local/bin/node"),
        ],
    );
    candidates.insert(
        "codex",
        vec![
            home.join(".nvm/versions/node/v22.18.0/bin/codex"),
            home.join(".local/bin/codex"),
        ],
    );
    candidates.insert(
        "claude",
        vec![
            home.join(".local/bin/claude"),
            home.join(".nvm/versions/node/v22.18.0/bin/claude"),
        ],
    );
    candidates
        .remove(name)
        .unwrap_or_default()
        .into_iter()
        .find(|path| path.is_file())
}

#[cfg(test)]
mod evaluation_pointer_tests {
    use super::{AdapterConfig, HistoryRecord, PreparationRoleInput};
    use crate::provider::bind_context_hash;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};

    fn record(report: &str) -> HistoryRecord {
        HistoryRecord {
            id: 17,
            date: "2026-09-02".to_string(),
            company: "Example".to_string(),
            role: "Frontend Engineer".to_string(),
            score: "4.2/5".to_string(),
            status: "Evaluated".to_string(),
            pdf: "yes".to_string(),
            report: report.to_string(),
            notes: String::new(),
        }
    }

    #[test]
    fn evaluation_input_hashes_the_tracker_relative_report_without_writing_upstream() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join("data")).unwrap();
        std::fs::create_dir_all(directory.path().join("reports")).unwrap();
        std::fs::write(directory.path().join("data/applications.md"), "tracker").unwrap();
        let bytes = b"# report";
        std::fs::write(directory.path().join("reports/017.md"), bytes).unwrap();
        let adapter = AdapterConfig {
            node_path: "node".into(),
            script_path: "adapter.mjs".into(),
            career_ops_root: directory.path().into(),
            tracker_index_path: directory.path().join("data/applications.db"),
            staging_path: directory.path().join("outside-staging"),
            claude_path: None,
        };

        let input = adapter
            .evaluation_result_input(&record("[017](../reports/017.md)"), &"a".repeat(64))
            .unwrap();

        assert_eq!(input.report_path, "reports/017.md");
        assert_eq!(input.report_sha256, format!("{:x}", Sha256::digest(bytes)));
        assert_eq!(input.tracker_id, 17);
        assert_eq!(
            std::fs::read_to_string(directory.path().join("data/applications.md")).unwrap(),
            "tracker"
        );
    }

    #[test]
    fn evaluation_input_rejects_a_tracker_report_escape() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join("data")).unwrap();
        std::fs::create_dir_all(directory.path().join("reports")).unwrap();
        std::fs::write(directory.path().join("data/applications.md"), "tracker").unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), directory.path().join("reports/escape.md"))
            .unwrap();
        let adapter = AdapterConfig {
            node_path: "node".into(),
            script_path: "adapter.mjs".into(),
            career_ops_root: directory.path().into(),
            tracker_index_path: directory.path().join("data/applications.db"),
            staging_path: directory.path().join("outside-staging"),
            claude_path: None,
        };
        assert!(
            adapter
                .evaluation_result_input(
                    &record("[outside](../reports/escape.md)"),
                    &"a".repeat(64),
                )
                .is_err()
        );
    }

    #[test]
    fn v2_full_cv_flows_from_context_through_binding_to_commit() {
        let directory = tempfile::tempdir().unwrap();
        let log = directory.path().join("requests.ndjson");
        let script = directory.path().join("adapter.sh");
        let context_hash = "a".repeat(64);
        let script_body = format!(
            r#"#!/bin/sh
request=$(cat)
printf '%s\n' "$request" >> '{}'
id=$(printf '%s' "$request" | sed -E 's/.*"id":"([^"]+)".*/\1/')
if printf '%s' "$request" | grep -q 'preparation.context.get'; then
  printf '{{"id":"%s","ok":true,"result":{{"preparationId":"55555555-5555-4555-8555-555555555555","contextHash":"{}","prompt":"cv only","job":{{"url":"https://jobs.example.test/role"}},"canonicalEvaluation":{{}},"evaluationGate":{{"score":4.2,"legitimacy":"High Confidence","authorizationConfidence":"investigate"}},"artifactPlan":{{"cv":{{"action":"refresh","scope":"full_cv"}}}}}}}}\n' "$id"
else
  printf '{{"id":"%s","ok":true,"result":{{"preparationId":"55555555-5555-4555-8555-555555555555","contextHash":"{}","trackerId":42,"artifacts":{{"report":{{"path":"reports/042.md","sha256":"{}"}},"cvHtml":{{"path":"output/042/cv/tailored/v001/cv.html","sha256":"{}"}},"cvPdf":{{"path":"output/042/cv/tailored/v001/cv.pdf","sha256":"{}"}},"cvChanges":{{"path":"output/042/cv/tailored/v001/changes.md","sha256":"{}"}}}},"cvProvenance":{{"source":"tailored_generated","tailored":true,"sourceSha256":null,"renderRecovery":null}},"warnings":[]}}}}\n' "$id"
fi
"#,
            log.display(),
            context_hash,
            context_hash,
            "b".repeat(64),
            "c".repeat(64),
            "d".repeat(64),
            "e".repeat(64),
        );
        std::fs::write(&script, script_body).unwrap();
        let adapter = AdapterConfig {
            node_path: "/bin/sh".into(),
            script_path: script,
            career_ops_root: directory.path().into(),
            tracker_index_path: directory.path().join("applications.db"),
            staging_path: directory.path().join("staging"),
            claude_path: None,
        };
        let input = PreparationRoleInput {
            preparation_id: "55555555-5555-4555-8555-555555555555".to_string(),
            company: "Example".to_string(),
            title: "Frontend Engineer".to_string(),
            location: "Remote".to_string(),
            url: "https://jobs.example.test/role".to_string(),
            tracker_id: 42,
            report_path: "reports/042.md".to_string(),
            report_sha256: "b".repeat(64),
            upstream_revision: "c".repeat(40),
            evaluation_compatibility_fingerprint: "d".repeat(64),
            artifact_compatibility_fingerprint: "e".repeat(64),
        };

        let context = adapter.preparation_context(&input).unwrap();
        let provider_result = bind_context_hash(
            json!({
                "contractVersion": 2,
                "contextHash": "f".repeat(64),
                "cvPayload": { "page_format": "a4" },
                "cvChangesMarkdown": "Verified source-backed reorder."
            }),
            &context.context_hash,
        )
        .unwrap();
        let committed = adapter
            .commit_preparation(
                &input,
                "2026-09-03",
                context.job,
                provider_result,
                None,
                context.canonical_evaluation,
                context.artifact_plan,
            )
            .unwrap();

        assert_eq!(committed.context_hash, context_hash);
        let requests = std::fs::read_to_string(log).unwrap();
        assert!(requests.contains("preparation.context.get"));
        assert!(requests.contains("preparation.result.commit"));
        assert!(requests.contains(r#""contractVersion":2"#));
        assert!(requests.contains(&format!(r#""contextHash":"{}""#, context_hash)));

        let commit_line = requests
            .lines()
            .find(|line| line.contains("preparation.result.commit"))
            .expect("commit request was logged");
        let commit_request: Value = serde_json::from_str(commit_line).unwrap();
        let commit_input = commit_request
            .get("input")
            .and_then(Value::as_object)
            .expect("commit request has input object");
        let allowed = [
            "preparationId",
            "eventDate",
            "company",
            "title",
            "location",
            "url",
            "job",
            "result",
            "canonicalEvaluation",
            "artifactPlan",
        ];
        for key in commit_input.keys() {
            assert!(
                allowed.contains(&key.as_str()),
                "preparation.result.commit must not send unknown field `{key}`"
            );
        }
        for key in allowed {
            assert!(
                commit_input.contains_key(key),
                "preparation.result.commit missing required field `{key}`"
            );
        }
        for forbidden in [
            "trackerId",
            "reportPath",
            "reportSha256",
            "upstreamRevision",
            "evaluationCompatibilityFingerprint",
            "artifactCompatibilityFingerprint",
        ] {
            assert!(
                !commit_input.contains_key(forbidden),
                "preparation.result.commit must not include context-only field `{forbidden}`"
            );
        }
    }
}

#[cfg(test)]
mod adapter_spawn_path_tests {
    use super::{AdapterConfig, adapter_child_path};
    use serde_json::json;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    #[test]
    fn adapter_child_path_prepends_discovered_claude_directory() {
        let path = adapter_child_path(
            Some(OsString::from("/usr/bin:/bin")),
            Some(Path::new("/Users/example/.local/bin/claude")),
        );
        let entries = std::env::split_paths(&path).collect::<Vec<_>>();

        assert_eq!(
            entries.first().map(PathBuf::as_path),
            Some(Path::new("/Users/example/.local/bin"))
        );
        assert!(entries.contains(&PathBuf::from("/usr/bin")));
        assert!(entries.contains(&PathBuf::from("/bin")));
    }

    #[test]
    fn adapter_spawn_env_includes_discovered_claude_directory() {
        let directory = tempfile::tempdir().unwrap();
        let claude_dir = directory.path().join("discovered-bin");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let claude_path = claude_dir.join("claude");
        std::fs::write(&claude_path, "#!/bin/sh\n").unwrap();
        let path_log = directory.path().join("child-path.txt");
        let script = directory.path().join("adapter.sh");
        std::fs::write(
            &script,
            format!(
                r#"#!/bin/sh
request=$(cat)
printf '%s\n' "$PATH" > '{}'
id=$(printf '%s' "$request" | sed -E 's/.*"id":"([^"]+)".*/\1/')
printf '{{"id":"%s","ok":true,"result":{{"ok":true}}}}\n' "$id"
"#,
                path_log.display()
            ),
        )
        .unwrap();
        let adapter = AdapterConfig {
            node_path: "/bin/sh".into(),
            script_path: script,
            career_ops_root: directory.path().into(),
            tracker_index_path: directory.path().join("applications.db"),
            staging_path: directory.path().join("staging"),
            claude_path: Some(claude_path),
        };

        adapter.request("capabilities.get", json!({})).unwrap();
        let spawned_path = std::fs::read_to_string(&path_log).unwrap();
        let entries = std::env::split_paths(spawned_path.trim()).collect::<Vec<_>>();

        assert!(
            entries.iter().any(|entry| entry == &claude_dir),
            "adapter spawn PATH must include the discovered claude directory, got {}",
            spawned_path.trim()
        );
    }
}
