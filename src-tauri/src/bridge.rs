use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::thread;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tauri::Manager;

use crate::AppState;
use crate::PUBLIC_CV_FILENAME;
use crate::domain::BrowserInspection;

const MAX_MESSAGE_BYTES: u64 = 1_048_576;

pub fn start(app: tauri::AppHandle, socket_path: PathBuf) -> Result<(), String> {
    if socket_path.exists() {
        fs::remove_file(&socket_path).map_err(|error| error.to_string())?;
    }
    let listener = UnixListener::bind(&socket_path).map_err(|error| error.to_string())?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    thread::Builder::new()
        .name("here-for-work-browser-bridge".to_string())
        .spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut bytes = Vec::new();
                let read_result = (&mut stream)
                    .take(MAX_MESSAGE_BYTES + 1)
                    .read_to_end(&mut bytes);
                let response = match read_result {
                    Ok(_) if bytes.len() as u64 <= MAX_MESSAGE_BYTES => {
                        handle_message(&app, &bytes)
                    }
                    Ok(_) => {
                        json!({ "protocolVersion": 1, "ok": false, "error": "message_too_large" })
                    }
                    Err(_) => json!({ "protocolVersion": 1, "ok": false, "error": "read_failed" }),
                };
                let _ = stream.write_all(response.to_string().as_bytes());
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn handle_message(app: &tauri::AppHandle, bytes: &[u8]) -> Value {
    let Ok(message) = serde_json::from_slice::<Value>(bytes) else {
        return json!({ "protocolVersion": 1, "ok": false, "error": "invalid_json" });
    };
    if message.get("protocolVersion").and_then(Value::as_u64) != Some(1) {
        return json!({ "protocolVersion": 1, "ok": false, "error": "unsupported_protocol" });
    }
    let Some(extension_id) = message.get("extensionId").and_then(Value::as_str) else {
        return json!({ "protocolVersion": 1, "ok": false, "error": "missing_extension_id" });
    };
    let Some(installation_id) = message.get("installationId").and_then(Value::as_str) else {
        return json!({ "protocolVersion": 1, "ok": false, "error": "missing_installation_id" });
    };
    if uuid::Uuid::parse_str(installation_id).is_err() {
        return json!({ "protocolVersion": 1, "ok": false, "error": "invalid_installation_id" });
    }
    if extension_id.len() != 32 || !extension_id.bytes().all(|byte| matches!(byte, b'a'..=b'p')) {
        return json!({ "protocolVersion": 1, "ok": false, "error": "invalid_extension_id" });
    }
    let state = app.state::<AppState>();
    let Ok(mut store) = state.store.lock() else {
        return json!({ "protocolVersion": 1, "ok": false, "error": "store_unavailable" });
    };
    let approved = match (
        store.approved_extension_id(),
        store.approved_installation_id(),
    ) {
        (Ok(Some(approved_extension)), Ok(Some(approved_installation)))
            if approved_extension == extension_id && approved_installation == installation_id =>
        {
            true
        }
        (Ok(_), Ok(_)) => false,
        (Err(_), _) | (_, Err(_)) => {
            return json!({ "protocolVersion": 1, "ok": false, "error": "store_unavailable" });
        }
    };
    if !approved {
        if message.get("type").and_then(Value::as_str) == Some("hello") {
            let _ = store.set_pending_browser_identity(extension_id, installation_id);
            return json!({ "protocolVersion": 1, "ok": false, "type": "pairing_required" });
        }
        return json!({ "protocolVersion": 1, "ok": false, "error": "extension_not_approved" });
    }
    let _ = store.record_browser_connected();
    match message.get("type").and_then(Value::as_str) {
        Some("hello") => json!({ "protocolVersion": 1, "ok": true, "type": "hello_ack" }),
        Some("poll") => match store.claim_browser_command(Utc::now(), Duration::seconds(30)) {
            Ok(Some(mut command)) => match materialize_browser_payload(
                &state.adapter.career_ops_root,
                &mut command.payload,
            ) {
                Ok(()) => json!({
                    "protocolVersion": 1,
                    "ok": true,
                    "type": "command",
                    "commandId": command.command_id,
                    "sessionId": command.session_id,
                    "commandType": command.command_type,
                    "driverLeaseId": command.driver_lease_id,
                    "payload": command.payload,
                }),
                Err(error) => {
                    let _ = store.fail_browser_command(&command.command_id, error);
                    json!({ "protocolVersion": 1, "ok": false, "error": error })
                }
            },
            Ok(None) => json!({ "protocolVersion": 1, "ok": true, "type": "idle" }),
            Err(_) => json!({ "protocolVersion": 1, "ok": false, "error": "store_unavailable" }),
        },
        Some("command_result") => handle_command_result(app, &mut store, &message),
        _ => json!({ "protocolVersion": 1, "ok": false, "error": "unsupported_message" }),
    }
}

fn materialize_browser_payload(root: &Path, payload: &mut Value) -> Result<(), &'static str> {
    const MAX_CV_PDF_BYTES: usize = 600_000;

    let Some(descriptor) = payload
        .as_object_mut()
        .and_then(|object| object.remove("cvUpload"))
    else {
        return Ok(());
    };
    let object = descriptor
        .as_object()
        .ok_or("invalid_cv_upload_descriptor")?;
    let field_id = object
        .get("fieldId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 500)
        .ok_or("invalid_cv_upload_descriptor")?;
    let relative_path = object
        .get("relativePath")
        .and_then(Value::as_str)
        .ok_or("invalid_cv_upload_descriptor")?;
    let expected_hash = object
        .get("sha256")
        .and_then(Value::as_str)
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
        .ok_or("invalid_cv_upload_descriptor")?;
    if object.get("mimeType").and_then(Value::as_str) != Some("application/pdf")
        || object.get("classification").and_then(Value::as_str) != Some("safe_verified")
    {
        return Err("invalid_cv_upload_descriptor");
    }
    let path = crate::checked_career_ops_artifact(root, relative_path)
        .map_err(|_| "cv_upload_artifact_unavailable")?;
    let bytes = fs::read(path).map_err(|_| "cv_upload_artifact_unavailable")?;
    if bytes.is_empty() || bytes.len() > MAX_CV_PDF_BYTES {
        return Err("cv_upload_artifact_size_invalid");
    }
    let actual_hash = format!("{:x}", Sha256::digest(&bytes));
    if actual_hash != expected_hash {
        return Err("cv_upload_artifact_changed");
    }
    let upload = json!({
        "fieldId": field_id,
        "fileName": PUBLIC_CV_FILENAME,
        "mimeType": "application/pdf",
        "contentBase64": STANDARD.encode(bytes),
        "sha256": expected_hash,
        "classification": "safe_verified"
    });
    payload
        .as_object_mut()
        .ok_or("invalid_cv_upload_descriptor")?
        .insert("uploads".to_string(), json!([upload]));
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FormSnapshot {
    protocol_version: u32,
    ats: String,
    url: String,
    title: String,
    fields: Vec<FormField>,
    flow: FormFlow,
    fingerprint: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FormFlow {
    disposition: String,
    issues: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FormField {
    id: String,
    label: String,
    control: String,
    input_type: String,
    input_mode: Option<String>,
    required: bool,
    options: Vec<String>,
    language: Option<String>,
    max_length: Option<u32>,
    max_words: Option<u32>,
    min_sentences: Option<u32>,
    max_sentences: Option<u32>,
    classification: String,
    reason: String,
}

fn handle_command_result(
    _app: &tauri::AppHandle,
    store: &mut crate::store::Store,
    message: &Value,
) -> Value {
    let Some(command_id) = message.get("commandId").and_then(Value::as_str) else {
        return json!({ "protocolVersion": 1, "ok": false, "error": "missing_command_id" });
    };
    let Some(session_id) = message.get("sessionId").and_then(Value::as_str) else {
        return json!({ "protocolVersion": 1, "ok": false, "error": "missing_session_id" });
    };
    let Some(command_type) = message.get("commandType").and_then(Value::as_str) else {
        return json!({ "protocolVersion": 1, "ok": false, "error": "missing_command_type" });
    };
    let driver_lease_id = match message.get("driverLeaseId") {
        Some(Value::String(value)) => Some(value.as_str()),
        Some(Value::Null) => None,
        _ => {
            return json!({ "protocolVersion": 1, "ok": false, "error": "missing_driver_lease_id" });
        }
    };
    match store.browser_command_result_matches(
        command_id,
        session_id,
        command_type,
        driver_lease_id,
    ) {
        Ok(true) => {}
        Ok(false) => {
            return json!({ "protocolVersion": 1, "ok": false, "error": "stale_or_mismatched_driver_lease" });
        }
        Err(_) => return json!({ "protocolVersion": 1, "ok": false, "error": "store_unavailable" }),
    }
    let status = message.get("status").and_then(Value::as_str);
    if status == Some("failed") {
        let error_code = message
            .get("error")
            .and_then(Value::as_str)
            .map(normalize_error_code)
            .unwrap_or_else(|| "extension_command_failed".to_string());
        let result = if command_type == "focus_review" {
            store.fail_focus_review(command_id, &error_code)
        } else {
            store.fail_browser_command(command_id, &error_code)
        };
        return match result {
            Ok(_) => json!({ "protocolVersion": 1, "ok": true, "type": "result_ack" }),
            Err(_) => {
                json!({ "protocolVersion": 1, "ok": false, "error": "invalid_command_state" })
            }
        };
    }
    if status != Some("completed") {
        return json!({ "protocolVersion": 1, "ok": false, "error": "invalid_command_status" });
    }
    let result = message.get("result").cloned().unwrap_or_else(|| json!({}));
    match command_type {
        "inspect_request" => {
            if result.get("ok").and_then(Value::as_bool) != Some(true) {
                let error = result
                    .get("error")
                    .and_then(Value::as_str)
                    .map(normalize_error_code)
                    .unwrap_or_else(|| "inspection_failed".to_string());
                return match store.fail_browser_command(command_id, &error) {
                    Ok(_) => json!({ "protocolVersion": 1, "ok": true, "type": "result_ack" }),
                    Err(_) => {
                        json!({ "protocolVersion": 1, "ok": false, "error": "invalid_command_state" })
                    }
                };
            }
            let Some(snapshot_value) = result.get("snapshot").cloned() else {
                return json!({ "protocolVersion": 1, "ok": false, "error": "missing_form_snapshot" });
            };
            let snapshot = match validate_form_snapshot(snapshot_value.clone()) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    let _ = store.fail_browser_command(command_id, error);
                    return json!({ "protocolVersion": 1, "ok": false, "error": error });
                }
            };
            let safe_field_count = snapshot
                .fields
                .iter()
                .filter(|field| field.classification == "safe_verified")
                .count();
            let needs_user_count = snapshot.fields.len().saturating_sub(safe_field_count);
            let inspection = BrowserInspection {
                ats: snapshot.ats,
                page_title: snapshot.title,
                page_url: snapshot.url,
                snapshot_fingerprint: snapshot.fingerprint,
                fields: snapshot_value["fields"].clone(),
                flow_disposition: snapshot.flow.disposition,
                flow_issues: json!(snapshot.flow.issues),
                safe_field_count,
                needs_user_count,
            };
            match store.complete_browser_inspection(command_id, &inspection) {
                Ok(_) => json!({ "protocolVersion": 1, "ok": true, "type": "result_ack" }),
                Err(_) => {
                    json!({ "protocolVersion": 1, "ok": false, "error": "invalid_command_state" })
                }
            }
        }
        "release_for_review" => {
            if result.get("ok").and_then(Value::as_bool) != Some(true) {
                return match store.fail_browser_command(command_id, "release_failed") {
                    Ok(_) => json!({ "protocolVersion": 1, "ok": true, "type": "result_ack" }),
                    Err(_) => {
                        json!({ "protocolVersion": 1, "ok": false, "error": "invalid_command_state" })
                    }
                };
            }
            match store.complete_browser_release(command_id) {
                Ok(_) => json!({ "protocolVersion": 1, "ok": true, "type": "result_ack" }),
                Err(_) => {
                    json!({ "protocolVersion": 1, "ok": false, "error": "invalid_command_state" })
                }
            }
        }
        "focus_review" => {
            if result.get("ok").and_then(Value::as_bool) != Some(true) {
                return match store.fail_focus_review(command_id, "focus_review_failed") {
                    Ok(_) => json!({ "protocolVersion": 1, "ok": true, "type": "result_ack" }),
                    Err(_) => {
                        json!({ "protocolVersion": 1, "ok": false, "error": "invalid_command_state" })
                    }
                };
            }
            match store.complete_focus_review(command_id) {
                Ok(_) => json!({ "protocolVersion": 1, "ok": true, "type": "result_ack" }),
                Err(_) => {
                    json!({ "protocolVersion": 1, "ok": false, "error": "invalid_command_state" })
                }
            }
        }
        "fill_plan" => {
            if result.get("ok").and_then(Value::as_bool) != Some(true) {
                return match store.fail_browser_command(command_id, "fill_failed") {
                    Ok(_) => json!({ "protocolVersion": 1, "ok": true, "type": "result_ack" }),
                    Err(_) => {
                        json!({ "protocolVersion": 1, "ok": false, "error": "invalid_command_state" })
                    }
                };
            }
            let Some(results) = result.get("results") else {
                return json!({ "protocolVersion": 1, "ok": false, "error": "missing_fill_results" });
            };
            if !valid_fill_results(results) {
                let _ = store.fail_browser_command(command_id, "invalid_fill_results");
                return json!({ "protocolVersion": 1, "ok": false, "error": "invalid_fill_results" });
            }
            match store.complete_browser_fill(command_id, results) {
                Ok(_) => json!({ "protocolVersion": 1, "ok": true, "type": "result_ack" }),
                Err(_) => {
                    json!({ "protocolVersion": 1, "ok": false, "error": "invalid_command_state" })
                }
            }
        }
        _ => json!({ "protocolVersion": 1, "ok": false, "error": "unsupported_command_result" }),
    }
}

fn valid_fill_results(value: &Value) -> bool {
    let Some(items) = value.as_array() else {
        return false;
    };
    if items.len() > 300 {
        return false;
    }
    items.iter().all(|item| {
        let Some(object) = item.as_object() else {
            return false;
        };
        let valid_reason = match object.get("reason") {
            Some(Value::Null) => true,
            Some(Value::String(value)) => value.len() <= 1_000,
            _ => false,
        };
        object.len() == 6
            && object
                .get("fieldId")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty() && value.len() <= 500)
            && object
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|value| matches!(value, "verified" | "skipped" | "failed"))
            && valid_reason
            && object
                .get("reasonCode")
                .and_then(Value::as_str)
                .is_some_and(|value| {
                    matches!(
                        value,
                        "verified"
                            | "user_file_preserved"
                            | "unsafe_instruction"
                            | "control_missing"
                            | "control_hidden"
                            | "control_replaced"
                            | "control_ambiguous"
                            | "unsupported_control"
                            | "unsupported_option"
                            | "write_failed"
                            | "readback_mismatch"
                            | "attachment_invalid"
                            | "attachment_failed"
                    )
                })
            && object.get("mutated").and_then(Value::as_bool).is_some()
            && match object.get("readBackSha256") {
                Some(Value::Null) => true,
                Some(Value::String(value)) => {
                    value.len() == 64
                        && value
                            .bytes()
                            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                }
                _ => false,
            }
    })
}

fn validate_form_snapshot(value: Value) -> Result<FormSnapshot, &'static str> {
    let snapshot: FormSnapshot =
        serde_json::from_value(value).map_err(|_| "invalid_form_snapshot")?;
    if snapshot.protocol_version != 1
        || !matches!(
            snapshot.ats.as_str(),
            "ashby" | "greenhouse" | "lever" | "generic"
        )
        || !allowed_form_url(&snapshot.url, &snapshot.ats)
        || snapshot.title.len() > 500
        || snapshot.fields.len() > 300
        || !matches!(
            snapshot.flow.disposition.as_str(),
            "fillable" | "fallback_eligible" | "human_handoff"
        )
        || snapshot.flow.issues.len() > 8
        || snapshot.flow.issues.iter().any(|issue| {
            !matches!(
                issue.as_str(),
                "authentication_required"
                    | "captcha_or_antibot"
                    | "custom_widget"
                    | "embedded_frame"
                    | "modal_form"
                    | "multi_step_form"
                    | "no_compatible_fields"
            )
        })
        || (snapshot.flow.disposition == "fillable" && !snapshot.flow.issues.is_empty())
        || (snapshot.fields.is_empty() && snapshot.flow.disposition == "fillable")
        || (snapshot.flow.disposition != "fillable" && snapshot.flow.issues.is_empty())
        || (snapshot.flow.disposition == "human_handoff"
            && !snapshot.flow.issues.iter().any(|issue| {
                matches!(
                    issue.as_str(),
                    "authentication_required" | "captcha_or_antibot"
                )
            }))
        || snapshot.fingerprint.len() != 64
        || !snapshot
            .fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("invalid_form_snapshot");
    }
    let mut field_ids = std::collections::BTreeSet::new();
    for field in &snapshot.fields {
        if field.id.is_empty()
            || !field_ids.insert(field.id.as_str())
            || field.id.len() > 500
            || field.label.len() > 2_000
            || field.input_type.len() > 100
            || field.reason.len() > 1_000
            || field.options.len() > 200
            || field.options.iter().any(|option| option.len() > 1_000)
            || !matches!(
                field.control.as_str(),
                "input" | "textarea" | "select" | "unsupported"
            )
            || !matches!(
                field.classification.as_str(),
                "safe_verified"
                    | "grounded_narrative"
                    | "compensation"
                    | "sensitive"
                    | "unknown"
                    | "unsupported"
                    | "unverifiable"
            )
            || field
                .input_mode
                .as_ref()
                .is_some_and(|value| value.len() > 50)
            || field
                .language
                .as_ref()
                .is_some_and(|value| value.len() > 35)
            || field
                .max_length
                .is_some_and(|value| value == 0 || value > 12_000)
            || field
                .max_words
                .is_some_and(|value| value == 0 || value > 3_000)
            || field
                .min_sentences
                .is_some_and(|value| value == 0 || value > 50)
            || field
                .max_sentences
                .is_some_and(|value| value == 0 || value > 50)
            || matches!((field.min_sentences, field.max_sentences), (Some(minimum), Some(maximum)) if minimum > maximum)
        {
            return Err("invalid_form_snapshot");
        }
        let _ = field.required;
    }
    Ok(snapshot)
}

fn allowed_form_url(value: &str, family: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return false;
    }
    let is_public_host = match url.host() {
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
    };
    if !is_public_host {
        return false;
    }
    match family {
        "ashby" => host == "jobs.ashbyhq.com",
        "greenhouse" => matches!(
            host.as_str(),
            "boards.greenhouse.io" | "job-boards.greenhouse.io" | "job-boards.eu.greenhouse.io"
        ),
        "lever" => matches!(host.as_str(), "jobs.lever.co" | "jobs.eu.lever.co"),
        "generic" => true,
        _ => false,
    }
}

fn normalize_error_code(error: &str) -> String {
    let normalized = error.to_lowercase();
    if matches!(
        normalized.as_str(),
        "snapshot_mismatch"
            | "form_drift_before_fill"
            | "invalid_fill_plan_duplicate_field"
            | "verified_fill_required"
    ) {
        return normalized;
    }
    if normalized.contains("automation control") || normalized.contains("webdriver") {
        return "automation_marked_session".to_string();
    }
    if normalized.contains("no active") {
        return "no_active_application_page".to_string();
    }
    if normalized.contains("receiving end does not exist")
        || normalized.contains("could not establish connection")
        || normalized.contains("no longer available")
    {
        return "unsupported_or_unavailable_page".to_string();
    }
    if normalized.contains("message port closed") {
        return "extension_message_interrupted".to_string();
    }
    if normalized.contains("approved chrome profile could not open")
        || normalized.contains("approved chrome profile did not return")
    {
        return "application_tab_recovery_failed".to_string();
    }
    "extension_command_failed".to_string()
}

pub fn socket_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("browser-bridge.sock")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn cv_bytes_are_materialized_only_for_the_transient_browser_response() {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        use sha2::{Digest, Sha256};

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("career-ops");
        let relative = "output/role/cv/tailored/v001/cv.pdf";
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = b"%PDF-1 verified";
        std::fs::write(&path, bytes).unwrap();
        let hash = format!("{:x}", Sha256::digest(bytes));
        let mut payload = json!({
            "plan": { "protocolVersion": 1, "snapshotFingerprint": "a".repeat(64), "instructions": [] },
            "cvUpload": {
                "fieldId": "resume",
                "relativePath": relative,
                "sha256": hash,
                "fileName": "Leonardo_Gomez_Frontend_Engineer.pdf",
                "mimeType": "application/pdf",
                "classification": "safe_verified"
            }
        });

        super::materialize_browser_payload(&root, &mut payload).unwrap();

        assert!(payload.get("cvUpload").is_none());
        assert_eq!(payload["uploads"][0]["fieldId"], "resume");
        assert_eq!(
            payload["uploads"][0]["contentBase64"],
            STANDARD.encode(bytes)
        );
        assert!(!payload.to_string().contains(relative));
    }

    #[test]
    fn form_snapshot_accepts_known_and_generic_public_https_hosts() {
        let valid = json!({
            "protocolVersion": 1,
            "ats": "ashby",
            "url": "https://jobs.ashbyhq.com/acme/role",
            "title": "Frontend Engineer",
            "fields": [{
                "id": "email",
                "label": "Email",
                "control": "input",
                "inputType": "email",
                "required": true,
                "options": [],
                "classification": "safe_verified",
                "reason": "Verified profile fact."
            }],
            "flow": { "disposition": "fillable", "issues": [] },
            "fingerprint": "a".repeat(64)
        });
        assert!(super::validate_form_snapshot(valid.clone()).is_ok());

        let mut wrong_host = valid.clone();
        wrong_host["url"] = json!("https://careers.example.com/acme/role");
        assert!(super::validate_form_snapshot(wrong_host).is_err());

        let mut generic = valid.clone();
        generic["ats"] = json!("generic");
        generic["url"] = json!("https://careers.example.com/acme/role");
        assert!(super::validate_form_snapshot(generic.clone()).is_ok());
        generic["url"] = json!("https://localhost/application");
        assert!(super::validate_form_snapshot(generic).is_err());

        let mut empty_form = valid.clone();
        empty_form["fields"] = json!([]);
        empty_form["flow"] =
            json!({ "disposition": "fallback_eligible", "issues": ["no_compatible_fields"] });
        assert!(super::validate_form_snapshot(empty_form).is_ok());

        let mut unknown_classification = valid;
        unknown_classification["fields"][0]["classification"] = json!("auto_submit");
        assert!(super::validate_form_snapshot(unknown_classification).is_err());

        let grounded = json!({
            "protocolVersion": 1,
            "ats": "generic",
            "url": "https://careers.example.com/acme/role",
            "title": "Frontend Engineer",
            "fields": [{
                "id": "story",
                "label": "Tell us about a front-end problem you solved",
                "control": "textarea",
                "inputType": "textarea",
                "required": true,
                "options": [],
                "language": "en",
                "maxLength": 600,
                "maxWords": 100,
                "minSentences": 2,
                "maxSentences": 3,
                "classification": "grounded_narrative",
                "reason": "Grounded draft pending review."
            }],
            "flow": { "disposition": "fillable", "issues": [] },
            "fingerprint": "b".repeat(64)
        });
        assert!(super::validate_form_snapshot(grounded).is_ok());
    }

    #[test]
    fn bridge_error_codes_are_bounded_and_non_sensitive() {
        assert_eq!(
            super::normalize_error_code("No active application page."),
            "no_active_application_page"
        );
        assert_eq!(
            super::normalize_error_code("https://private.example.test/sensitive"),
            "extension_command_failed"
        );
        assert_eq!(
            super::normalize_error_code(
                "The approved Chrome profile did not return the expected application tab."
            ),
            "application_tab_recovery_failed"
        );
    }

    #[test]
    fn fill_results_require_one_typed_readback_envelope_per_entry() {
        let verified = json!([{
            "fieldId": "email",
            "status": "verified",
            "reasonCode": "verified",
            "reason": null,
            "mutated": true,
            "readBackSha256": "a".repeat(64)
        }]);
        assert!(super::valid_fill_results(&verified));

        let mut missing_reason_code = verified.clone();
        missing_reason_code[0]
            .as_object_mut()
            .unwrap()
            .remove("reasonCode");
        assert!(!super::valid_fill_results(&missing_reason_code));

        let mut uppercase_hash = verified.clone();
        uppercase_hash[0]["readBackSha256"] = json!("A".repeat(64));
        assert!(!super::valid_fill_results(&uppercase_hash));

        let duplicate_field_results = json!([verified[0].clone(), verified[0].clone()]);
        assert!(super::valid_fill_results(&duplicate_field_results));
        // Exact field identity and cardinality are checked against the stored plan in Store.
    }
}
