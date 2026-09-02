use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde::Serialize;
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{CareerOpsCapabilityManifest, HistoryRecord, QueueFilters};

#[derive(Debug, Clone)]
pub struct AdapterConfig {
    pub node_path: PathBuf,
    pub script_path: PathBuf,
    pub career_ops_root: PathBuf,
    pub tracker_index_path: PathBuf,
    pub staging_path: PathBuf,
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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationContext {
    pub preparation_id: String,
    pub context_hash: String,
    pub prompt: String,
    pub job: Value,
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
        let timeout = if operation.starts_with("preparation.") || operation.starts_with("answers.")
        {
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

    pub fn commit_preparation(
        &self,
        input: &PreparationRoleInput,
        event_date: &str,
        job: Value,
        result: Value,
        fallback_configuration: Option<&Value>,
    ) -> Result<PreparationCommit, AdapterError> {
        let mut payload = serde_json::to_value(input)
            .expect("preparation role input serializes")
            .as_object()
            .cloned()
            .expect("preparation input is an object");
        payload.insert(
            "eventDate".to_string(),
            Value::String(event_date.to_string()),
        );
        payload.insert("job".to_string(), job);
        payload.insert("result".to_string(), result);
        let value = self.request_with_timeout_and_environment(
            "preparation.result.commit",
            Value::Object(payload),
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

fn read_all(mut reader: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
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
