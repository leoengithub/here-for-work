mod adapter;
mod bridge;
mod domain;
mod provider;
mod store;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use adapter::{AdapterConfig, CanonicalRoleInput, PreparationRoleInput, discover_executable};
use domain::{
    BrowserSessionSummary, BrowserSetup, CheckResult, ChromeProfile, DashboardState, ImportResult,
    IntegrationHealth, MaintenanceResult, PreparationDetail, PreparationWork, PrepareRoleOutcome,
    QueueFilters, ReconcileResult, RestorePreflight, ScheduledRun,
};
use sha2::Digest;
use store::Store;
use tauri::Manager;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

struct AppState {
    store: Mutex<Store>,
    adapter: AdapterConfig,
    codex_path: Option<PathBuf>,
    claude_path: Option<PathBuf>,
    provider_schema_path: PathBuf,
    preparation_schema_path: PathBuf,
    answer_schema_path: PathBuf,
    provider_sandbox_path: PathBuf,
    home_dir: PathBuf,
    extension_directory: PathBuf,
    native_host_path: PathBuf,
    app_data_dir: PathBuf,
    preparation_cancellations: Mutex<HashMap<String, Arc<AtomicBool>>>,
    canonical_write_lock: Mutex<()>,
}

struct PreparationCancellationRegistration<'a> {
    role_id: String,
    registrations: &'a Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl Drop for PreparationCancellationRegistration<'_> {
    fn drop(&mut self) {
        if let Ok(mut registrations) = self.registrations.lock() {
            registrations.remove(&self.role_id);
        }
    }
}

#[tauri::command]
fn get_dashboard(state: tauri::State<'_, AppState>) -> Result<DashboardState, String> {
    let store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store.dashboard().map_err(|error| error.to_string())
}

#[tauri::command]
fn import_dataset(
    payload: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<ImportResult, String> {
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    let result = store
        .import_dataset(&payload)
        .map_err(|error| error.to_string())?;
    drop(store);
    if result.imported > 0 {
        let _ = app
            .notification()
            .builder()
            .title("New roles to review")
            .body(format!(
                "{} new role(s) are ready in HereForWork.",
                result.imported
            ))
            .show();
    }
    Ok(result)
}

#[tauri::command]
fn set_background_enabled(
    enabled: bool,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<DashboardState, String> {
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store
        .set_background_enabled(enabled)
        .map_err(|error| error.to_string())?;
    if enabled {
        app.autolaunch()
            .enable()
            .map_err(|error| error.to_string())?;
    } else {
        app.autolaunch()
            .disable()
            .map_err(|error| error.to_string())?;
    }
    store.dashboard().map_err(|error| error.to_string())
}

#[tauri::command]
fn save_queue_filters(
    filters: QueueFilters,
    state: tauri::State<'_, AppState>,
) -> Result<DashboardState, String> {
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store
        .set_queue_filters(&filters)
        .map_err(|error| error.to_string())?;
    store.dashboard().map_err(|error| error.to_string())
}

#[tauri::command]
fn request_notification_permission(app: tauri::AppHandle) -> Result<bool, String> {
    let notifications = app.notification();
    if notifications
        .permission_state()
        .map_err(|error| error.to_string())?
        == tauri_plugin_notification::PermissionState::Granted
    {
        return Ok(true);
    }
    notifications
        .request_permission()
        .map(|permission| permission == tauri_plugin_notification::PermissionState::Granted)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn send_test_notification(app: tauri::AppHandle) -> Result<(), String> {
    app.notification()
        .builder()
        .title("HereForWork is ready")
        .body("Notifications can bring you back to the local review queue.")
        .show()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn queue_manual_discovery(
    source_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ScheduledRun, String> {
    if !matches!(source_id.as_str(), "frontend-role-scan" | "eu-job-radar") {
        return Err("Unknown discovery source".to_string());
    }
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store
        .queue_manual_run(&source_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn run_provider_probe(
    provider: provider::ProviderKind,
    state: tauri::State<'_, AppState>,
) -> Result<provider::ProviderProbeResult, String> {
    let executable = match provider {
        provider::ProviderKind::Codex => state.codex_path.as_ref(),
        provider::ProviderKind::Claude => state.claude_path.as_ref(),
    }
    .ok_or("Selected provider CLI is not configured")?;
    provider::probe(
        provider,
        executable,
        &state.provider_schema_path,
        &state.provider_sandbox_path,
    )
}

enum PreparationGate {
    Proceed,
    NeedsDecision(&'static str),
    Discard(&'static str),
}

fn preparation_gate(result: &serde_json::Value) -> Result<PreparationGate, String> {
    let score = result
        .get("score")
        .and_then(serde_json::Value::as_f64)
        .ok_or("Provider result omitted its match score")?;
    let legitimacy = result
        .get("legitimacy")
        .and_then(serde_json::Value::as_str)
        .ok_or("Provider result omitted legitimacy")?;
    let authorization = result
        .get("authorizationConfidence")
        .and_then(serde_json::Value::as_str)
        .ok_or("Provider result omitted authorization confidence")?;
    if authorization == "problem" {
        return Ok(PreparationGate::Discard(
            "Preparation stopped because career-ops confirmed an authorization conflict.",
        ));
    }
    if legitimacy == "Suspicious" {
        return Ok(PreparationGate::Discard(
            "Preparation stopped because career-ops found a legitimacy blocker.",
        ));
    }
    if score < 4.0 {
        return Ok(PreparationGate::NeedsDecision(
            "Preparation stopped before creating files because the verified match score is below 4.0.",
        ));
    }
    Ok(PreparationGate::Proceed)
}

#[tauri::command]
fn prepare_role(
    role_id: String,
    provider: provider::ProviderKind,
    state: tauri::State<'_, AppState>,
) -> Result<PrepareRoleOutcome, String> {
    let provider_name = match provider {
        provider::ProviderKind::Codex => "codex",
        provider::ProviderKind::Claude => "claude",
    };
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store
        .begin_preparation(&role_id, provider_name)
        .map_err(|error| error.to_string())?;
    let dashboard = store.dashboard().map_err(|error| error.to_string())?;
    Ok(PrepareRoleOutcome {
        dashboard,
        disposition: "queued".to_string(),
        message: "Preparation queued. You can keep working while HereForWork prepares this application in the background.".to_string(),
    })
}

fn process_preparation_work(app: &tauri::AppHandle, work: PreparationWork) -> Result<(), String> {
    let state = app.state::<AppState>();
    let provider = match work.provider.as_str() {
        "codex" => provider::ProviderKind::Codex,
        "claude" => provider::ProviderKind::Claude,
        _ => {
            if let Ok(mut store) = state.store.lock() {
                let _ = store.fail_preparation(&work.id, "invalid_provider");
            }
            return Err("Stored preparation provider is invalid".to_string());
        }
    };
    let provider_name = work.provider.as_str();
    let role_id = work.role_id.clone();
    let cancellation = Arc::new(AtomicBool::new(false));
    state
        .preparation_cancellations
        .lock()
        .map_err(|_| "Preparation cancellation lock was poisoned".to_string())?
        .insert(role_id.clone(), Arc::clone(&cancellation));
    let _cancellation_registration = PreparationCancellationRegistration {
        role_id: role_id.clone(),
        registrations: &state.preparation_cancellations,
    };
    let executable = match provider {
        provider::ProviderKind::Codex => state.codex_path.as_ref(),
        provider::ProviderKind::Claude => state.claude_path.as_ref(),
    };
    let url = work
        .role
        .application_url
        .clone()
        .ok_or("The role has no application URL")?;
    let input = PreparationRoleInput {
        preparation_id: work.id.clone(),
        company: work.role.company.clone(),
        title: work.role.title.clone(),
        location: work.role.location.clone(),
        url,
    };
    let context = match state.adapter.preparation_context(&input) {
        Ok(context) => context,
        Err(error) => {
            if let Ok(mut store) = state.store.lock() {
                let _ = store.fail_preparation(&work.id, "context_unavailable");
            }
            return Err(error.to_string());
        }
    };
    if context.preparation_id != work.id || context.context_hash.len() != 64 {
        if let Ok(mut store) = state.store.lock() {
            let _ = store.fail_preparation(&work.id, "invalid_context");
        }
        return Err("career-ops returned an invalid preparation context".to_string());
    }
    let resolved_application_url = context
        .job
        .get("url")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "career-ops returned no resolved application URL".to_string())?;
    {
        let mut store = state
            .store
            .lock()
            .map_err(|_| "Operational store lock was poisoned".to_string())?;
        store
            .record_preparation_context(&work.id, &context.context_hash, resolved_application_url)
            .map_err(|error| error.to_string())?;
    }
    if cancellation.load(Ordering::Relaxed) {
        if let Ok(mut store) = state.store.lock() {
            let _ = store.cancel_preparation(&work.id);
        }
        return Err("Preparation was cancelled.".to_string());
    }
    let result = match state
        .adapter
        .recover_preparation_result(&work.id, &context.context_hash)
    {
        Ok(Some(result)) => result,
        Ok(None) => {
            let Some(executable) = executable else {
                if let Ok(mut store) = state.store.lock() {
                    let _ = store.fail_preparation(&work.id, "provider_not_configured");
                }
                return Err(format!(
                    "The selected {provider_name} CLI is not configured"
                ));
            };
            match provider::invoke_structured_cancellable(
                provider,
                executable,
                &state.preparation_schema_path,
                &state.provider_sandbox_path,
                &context.prompt,
                std::time::Duration::from_secs(600),
                Some(&cancellation),
            ) {
                Ok(result) => match provider::bind_context_hash(result, &context.context_hash) {
                    Ok(result) => result,
                    Err(error) => {
                        if let Ok(mut store) = state.store.lock() {
                            let _ = store.fail_preparation(&work.id, "invalid_provider_result");
                        }
                        return Err(error);
                    }
                },
                Err(error) => {
                    if let Ok(mut store) = state.store.lock() {
                        if cancellation.load(Ordering::Relaxed) {
                            let _ = store.cancel_preparation(&work.id);
                        } else {
                            let _ = store.fail_preparation(&work.id, "provider_failed");
                        }
                    }
                    return Err(error);
                }
            }
        }
        Err(error) => {
            if let Ok(mut store) = state.store.lock() {
                let _ = store.fail_preparation(&work.id, "recovery_failed");
            }
            return Err(error.to_string());
        }
    };
    if cancellation.load(Ordering::Relaxed) {
        if let Ok(mut store) = state.store.lock() {
            let _ = store.cancel_preparation(&work.id);
        }
        return Err("Preparation was cancelled.".to_string());
    }
    if result
        .get("contextHash")
        .and_then(serde_json::Value::as_str)
        != Some(context.context_hash.as_str())
    {
        if let Ok(mut store) = state.store.lock() {
            let _ = store.fail_preparation(&work.id, "stale_provider_result");
        }
        return Err("Provider result does not match the current preparation context".to_string());
    }
    match preparation_gate(&result)? {
        PreparationGate::Proceed => {}
        PreparationGate::NeedsDecision(message) => {
            let mut store = state
                .store
                .lock()
                .map_err(|_| "Operational store lock was poisoned".to_string())?;
            store
                .hold_preparation_for_decision(&work.id, "preparation_gate_needs_decision")
                .map_err(|error| error.to_string())?;
            drop(store);
            let _ = app
                .notification()
                .builder()
                .title("Role needs your decision")
                .body(message)
                .show();
            return Ok(());
        }
        PreparationGate::Discard(message) => {
            {
                let mut store = state
                    .store
                    .lock()
                    .map_err(|_| "Operational store lock was poisoned".to_string())?;
                store
                    .cancel_preparation(&work.id)
                    .map_err(|error| error.to_string())?;
            }
            discard_role_internal(&role_id, &state)?;
            let _ = app
                .notification()
                .builder()
                .title("Role removed from the queue")
                .body(message)
                .show();
            return Ok(());
        }
    }
    let committed = {
        let _canonical_write = state
            .canonical_write_lock
            .lock()
            .map_err(|_| "Canonical writer lock was poisoned".to_string())?;
        match state
            .adapter
            .commit_preparation(&input, &madrid_today(), context.job, result)
        {
            Ok(committed) => committed,
            Err(error) => {
                if let Ok(mut store) = state.store.lock() {
                    let _ = store.fail_preparation(&work.id, "artifact_commit_failed");
                }
                return Err(error.to_string());
            }
        }
    };
    if committed.preparation_id != work.id || committed.context_hash != context.context_hash {
        if let Ok(mut store) = state.store.lock() {
            let _ = store.fail_preparation(&work.id, "commit_identity_mismatch");
        }
        return Err("career-ops committed a mismatched preparation".to_string());
    }
    for artifact in [
        &committed.artifacts.report,
        &committed.artifacts.cv_html,
        &committed.artifacts.cv_pdf,
        &committed.artifacts.cv_changes,
    ] {
        if artifact.path.is_empty()
            || artifact.sha256.len() != 64
            || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            if let Ok(mut store) = state.store.lock() {
                let _ = store.fail_preparation(&work.id, "invalid_artifact_reference");
            }
            return Err("career-ops returned an invalid artifact reference".to_string());
        }
    }
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store
        .complete_preparation(
            &work.id,
            committed.tracker_id,
            &committed.artifacts.report.path,
            &committed.artifacts.report.sha256,
            &committed.artifacts.cv_pdf.path,
            &committed.artifacts.cv_pdf.sha256,
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

const PREPARATION_WORKER_COUNT: usize = 2;

fn start_preparation_workers(app: tauri::AppHandle) -> Result<(), String> {
    for worker_index in 0..PREPARATION_WORKER_COUNT {
        let worker_app = app.clone();
        std::thread::Builder::new()
            .name(format!("here-for-work-preparation-worker-{worker_index}"))
            .spawn(move || loop {
                let work = {
                    let state = worker_app.state::<AppState>();
                    state
                        .store
                        .lock()
                        .ok()
                        .and_then(|mut store| store.claim_preparation_work().ok().flatten())
                };
                let Some(work) = work else {
                    std::thread::sleep(std::time::Duration::from_millis(350));
                    continue;
                };
                let preparation_id = work.id.clone();
                if process_preparation_work(&worker_app, work).is_err() {
                    if let Ok(mut store) = worker_app.state::<AppState>().store.lock() {
                        let _ = store.fail_preparation(&preparation_id, "preparation_failed");
                    }
                    let _ = worker_app
                        .notification()
                        .builder()
                        .title("Application preparation needs attention")
                        .body("One role stopped safely. Later queued roles will continue; open Applications for details or retry.")
                        .show();
                }
            })
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn start_browser_handoff_worker(app: tauri::AppHandle) -> Result<(), String> {
    std::thread::Builder::new()
        .name("here-for-work-browser-handoff-worker".to_string())
        .spawn(move || loop {
            let preparation_id = {
                let state = app.state::<AppState>();
                state
                    .store
                    .lock()
                    .ok()
                    .and_then(|store| store.next_preparation_for_browser_handoff().ok().flatten())
            };
            let Some(preparation_id) = preparation_id else {
                std::thread::sleep(std::time::Duration::from_millis(350));
                continue;
            };
            let state = app.state::<AppState>();
            if start_application_browser_session(&preparation_id, &state).is_err() {
                let _ = app
                    .notification()
                    .builder()
                    .title("Browser handoff needs attention")
                    .body("The application materials are ready, but its Chrome tab could not start. Later roles will continue.")
                    .show();
            }
        })
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn cancel_preparation(role_id: String, state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let active_cancellation = state
        .preparation_cancellations
        .lock()
        .map_err(|_| "Preparation cancellation lock was poisoned".to_string())?
        .get(&role_id)
        .cloned();
    if let Some(cancellation) = active_cancellation {
        cancellation.store(true, Ordering::Relaxed);
        return Ok(true);
    }
    state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?
        .cancel_queued_preparation_for_role(&role_id)
        .map_err(|error| error.to_string())
}

pub(crate) fn checked_career_ops_artifact(
    root: &std::path::Path,
    relative: &str,
) -> Result<PathBuf, String> {
    let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
    let candidate = canonical_root
        .join(relative)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !candidate.starts_with(&canonical_root) || !candidate.is_file() {
        return Err("Artifact is outside the configured career-ops output roots.".to_string());
    }
    Ok(candidate)
}

fn verified_cv_upload_descriptor(
    root: &std::path::Path,
    work: &crate::domain::BrowserAnswerWork,
) -> Result<Option<serde_json::Value>, String> {
    const MAX_CV_PDF_BYTES: u64 = 600_000;

    let eligible_fields = work
        .snapshot
        .get("fields")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|field| {
            field.get("control").and_then(serde_json::Value::as_str) == Some("input")
                && field.get("inputType").and_then(serde_json::Value::as_str) == Some("file")
                && field
                    .get("classification")
                    .and_then(serde_json::Value::as_str)
                    == Some("safe_verified")
        })
        .collect::<Vec<_>>();
    let [field] = eligible_fields.as_slice() else {
        return Ok(None);
    };
    let field_id = field
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "cv_upload_field_invalid".to_string())?;
    let path = checked_career_ops_artifact(root, &work.cv_pdf_path)
        .map_err(|_| "cv_upload_artifact_unavailable".to_string())?;
    let bytes = std::fs::read(path).map_err(|_| "cv_upload_artifact_unavailable".to_string())?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_CV_PDF_BYTES {
        return Err("cv_upload_artifact_size_invalid".to_string());
    }
    let actual_hash = format!("{:x}", sha2::Sha256::digest(&bytes));
    if actual_hash != work.cv_pdf_hash {
        return Err("cv_upload_artifact_changed".to_string());
    }
    Ok(Some(serde_json::json!({
        "fieldId": field_id,
        "relativePath": work.cv_pdf_path,
        "sha256": work.cv_pdf_hash,
        "fileName": "HereForWork-tailored-CV.pdf",
        "mimeType": "application/pdf",
        "classification": "safe_verified"
    })))
}

#[tauri::command]
fn get_preparation_detail(
    preparation_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<PreparationDetail, String> {
    let (report_path, cv_pdf_path) = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?
        .preparation_artifact_paths(&preparation_id)
        .map_err(|error| error.to_string())?;
    let report = checked_career_ops_artifact(&state.adapter.career_ops_root, &report_path)?;
    let _cv = checked_career_ops_artifact(&state.adapter.career_ops_root, &cv_pdf_path)?;
    if report.metadata().map_err(|error| error.to_string())?.len() > 500_000 {
        return Err("The career-ops report exceeds the 500 KB display limit.".to_string());
    }
    let report_markdown = std::fs::read_to_string(report).map_err(|error| error.to_string())?;
    Ok(PreparationDetail {
        preparation_id,
        report_markdown,
        report_path,
        cv_pdf_path,
    })
}

#[tauri::command]
fn open_preparation_artifact(
    preparation_id: String,
    artifact: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let (report_path, cv_pdf_path) = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?
        .preparation_artifact_paths(&preparation_id)
        .map_err(|error| error.to_string())?;
    let relative = match artifact.as_str() {
        "report" => report_path,
        "cv" => cv_pdf_path,
        _ => return Err("Artifact must be report or cv.".to_string()),
    };
    let path = checked_career_ops_artifact(&state.adapter.career_ops_root, &relative)?;
    std::process::Command::new("/usr/bin/open")
        .arg(path)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

fn chrome_profiles(home_dir: &std::path::Path) -> Vec<ChromeProfile> {
    let local_state = home_dir.join("Library/Application Support/Google/Chrome/Local State");
    let Ok(payload) = std::fs::read_to_string(local_state) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) else {
        return Vec::new();
    };
    let Some(cache) = value
        .pointer("/profile/info_cache")
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    let mut profiles = cache
        .iter()
        .map(|(id, details)| ChromeProfile {
            id: id.clone(),
            name: details
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(id)
                .to_string(),
        })
        .collect::<Vec<_>>();
    profiles.sort_by_key(|profile| profile.name.to_lowercase());
    profiles
}

#[tauri::command]
fn get_browser_setup(state: tauri::State<'_, AppState>) -> Result<BrowserSetup, String> {
    let store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    let native_manifest = state.home_dir.join(
        "Library/Application Support/Google/Chrome/NativeMessagingHosts/com.hereforwork.bridge.json",
    );
    Ok(BrowserSetup {
        profiles: chrome_profiles(&state.home_dir),
        selected_profile_id: store
            .selected_chrome_profile()
            .map_err(|error| error.to_string())?,
        approved_extension_id: store
            .approved_extension_id()
            .map_err(|error| error.to_string())?,
        pending_extension_id: store
            .pending_extension_id()
            .map_err(|error| error.to_string())?,
        approved_installation_id: store
            .approved_installation_id()
            .map_err(|error| error.to_string())?,
        pending_installation_id: store
            .pending_installation_id()
            .map_err(|error| error.to_string())?,
        extension_directory: state.extension_directory.display().to_string(),
        native_host_registered: native_manifest.is_file(),
        last_connected_at: store
            .browser_last_connected_at()
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
fn get_browser_sessions(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<BrowserSessionSummary>, String> {
    let store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store.browser_sessions().map_err(|error| error.to_string())
}

#[tauri::command]
fn start_browser_connection_check(
    state: tauri::State<'_, AppState>,
) -> Result<BrowserSessionSummary, String> {
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store
        .queue_browser_connection_check()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn retry_browser_session(
    session_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<BrowserSessionSummary, String> {
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store
        .retry_browser_session(&session_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn continue_in_browser(
    preparation_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<BrowserSessionSummary, String> {
    start_application_browser_session(&preparation_id, &state)
}

fn start_application_browser_session(
    preparation_id: &str,
    state: &AppState,
) -> Result<BrowserSessionSummary, String> {
    let (profile_id, session, reuse_existing_page) = {
        let mut store = state
            .store
            .lock()
            .map_err(|_| "Operational store lock was poisoned".to_string())?;
        let profile_id = store
            .selected_chrome_profile()
            .map_err(|error| error.to_string())?
            .ok_or("Select and connect a Chrome profile first")?;
        let reuse_existing_page = store
            .has_review_required_application_session(preparation_id)
            .map_err(|error| error.to_string())?;
        let session = store
            .queue_application_session(preparation_id)
            .map_err(|error| error.to_string())?;
        (profile_id, session, reuse_existing_page)
    };
    let url = session
        .page_url
        .as_deref()
        .ok_or("The application session has no URL")?;
    let chrome = PathBuf::from("/Applications/Google Chrome.app");
    if !chrome.is_dir() {
        if let Ok(mut store) = state.store.lock() {
            let _ = store.fail_browser_session_start(&session.id, "chrome_not_installed");
        }
        return Err("Google Chrome is not installed in /Applications.".to_string());
    }
    if !reuse_existing_page {
        if let Err(error) = background_chrome_command(&profile_id, url).spawn() {
            if let Ok(mut store) = state.store.lock() {
                let _ = store.fail_browser_session_start(&session.id, "chrome_launch_failed");
            }
            return Err(format!(
                "Could not open the selected Chrome profile: {error}"
            ));
        }
    }
    Ok(session)
}

fn background_chrome_command(profile_id: &str, url: &str) -> std::process::Command {
    let mut command = std::process::Command::new("/usr/bin/open");
    command
        .arg("-g")
        .arg("-a")
        .arg("Google Chrome")
        .arg("--args")
        .arg(format!("--profile-directory={profile_id}"))
        .arg("--new-tab")
        .arg(url);
    command
}

#[tauri::command]
fn confirm_application_applied(
    session_id: String,
    user_confirmed: bool,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<BrowserSessionSummary, String> {
    if !user_confirmed {
        return Err("Applied requires your explicit outcome confirmation.".to_string());
    }
    let (role_id, context) = {
        let mut store = state
            .store
            .lock()
            .map_err(|_| "Operational store lock was poisoned".to_string())?;
        store
            .begin_applied_effect_for_session(&session_id)
            .map_err(|error| error.to_string())?
    };
    let tracker_id = context
        .tracker_id
        .ok_or("Canonical tracker row is missing")?;
    let input = CanonicalRoleInput {
        idempotency_key: context.idempotency_key.clone(),
        event_date: madrid_today(),
        company: context.role.company,
        title: context.role.title,
        location: context.role.location,
        url: context.role.application_url,
    };
    let effect = {
        let _canonical_write = state
            .canonical_write_lock
            .lock()
            .map_err(|_| "Canonical writer lock was poisoned".to_string())?;
        match state.adapter.confirm_applied(&input, tracker_id) {
            Ok(effect) => effect,
            Err(error) => {
                if let Ok(mut store) = state.store.lock() {
                    let _ = store
                        .fail_adapter_effect(&context.idempotency_key, "canonical_write_failed");
                    let _ = store.mark_submitted_tracking_pending(&session_id);
                }
                let _ = app
                    .notification()
                    .builder()
                    .title("Tracking update pending")
                    .body("Your submission is not repeated. Open HereForWork to retry only the career-ops update.")
                    .show();
                return Err(error.to_string());
            }
        }
    };
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store
        .complete_applied_effect(
            &session_id,
            &role_id,
            &context.idempotency_key,
            effect.tracker_id,
            &effect.status,
        )
        .map_err(|error| error.to_string())?;
    store
        .browser_sessions()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|session| session.id == session_id)
        .ok_or("Application session disappeared".to_string())
}

fn start_answer_worker(app: tauri::AppHandle) -> Result<(), String> {
    std::thread::Builder::new()
        .name("here-for-work-answer-worker".to_string())
        .spawn(move || {
            loop {
                let commit_work = {
                    let state = app.state::<AppState>();
                    state
                        .store
                        .lock()
                        .ok()
                        .and_then(|mut store| store.claim_answer_commit_work().ok().flatten())
                };
                if let Some(work) = commit_work {
                    let outcome = (|| -> Result<(), String> {
                        let state = app.state::<AppState>();
                        let committed = {
                            let _canonical_write = state
                                .canonical_write_lock
                                .lock()
                                .map_err(|_| "answer_persistence_failed".to_string())?;
                            state.adapter.commit_answers(
                                &work.preparation_id,
                                &work.report_path,
                                &work.cv_pdf_path,
                                &work.context_hash,
                                work.review_items.clone(),
                                work.fill_results.clone(),
                                &madrid_today(),
                            )
                            .map_err(|_| "answer_persistence_failed".to_string())?
                        };
                        if committed.preparation_id != work.preparation_id
                            || committed.context_hash != work.context_hash
                            || committed.report.path != work.report_path
                        {
                            return Err("answer_persistence_failed".to_string());
                        }
                        let mut store = state
                            .store
                            .lock()
                            .map_err(|_| "store_unavailable".to_string())?;
                        store
                            .complete_answer_commit(
                                &work.session_id,
                                &committed.context_hash,
                                &committed.report.sha256,
                            )
                            .map_err(|_| "answer_persistence_failed".to_string())?;
                        Ok(())
                    })();
                    if let Err(error_code) = outcome {
                        if let Ok(mut store) = app.state::<AppState>().store.lock() {
                            let _ = store.fail_answer_commit(&work.session_id, &error_code);
                        }
                        let _ = app
                            .notification()
                            .builder()
                            .title("Application answers need attention")
                            .body("The verified answers were not saved to career-ops. The browser remains locked for review until you retry.")
                            .show();
                    }
                    continue;
                }
                let work = {
                    let state = app.state::<AppState>();
                    state
                        .store
                        .lock()
                        .ok()
                        .and_then(|mut store| store.claim_answer_work().ok().flatten())
                };
                let Some(work) = work else {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    continue;
                };
                let outcome = (|| -> Result<(), String> {
                    let state = app.state::<AppState>();
                    let provider = match work.provider.as_str() {
                        "codex" => provider::ProviderKind::Codex,
                        "claude" => provider::ProviderKind::Claude,
                        _ => return Err("invalid_provider".to_string()),
                    };
                    let executable = match provider {
                        provider::ProviderKind::Codex => state.codex_path.as_ref(),
                        provider::ProviderKind::Claude => state.claude_path.as_ref(),
                    }
                    .ok_or("provider_not_configured")?;
                    let context = state
                        .adapter
                        .answer_context(
                            &work.preparation_id,
                            &work.report_path,
                            work.snapshot.clone(),
                        )
                        .map_err(|_| "answer_context_failed".to_string())?;
                    if context.preparation_id != work.preparation_id
                        || context.snapshot_fingerprint != work.snapshot_fingerprint
                        || context.context_hash.len() != 64
                    {
                        return Err("invalid_answer_context".to_string());
                    }
                    let result = provider::invoke_structured(
                        provider,
                        executable,
                        &state.answer_schema_path,
                        &state.provider_sandbox_path,
                        &context.prompt,
                        std::time::Duration::from_secs(600),
                    )
                    .map_err(|_| "answer_provider_failed".to_string())?;
                    let result = provider::bind_context_hash(result, &context.context_hash)
                        .map_err(|_| "invalid_answer_provider_result".to_string())?;
                    if result
                        .get("contextHash")
                        .and_then(serde_json::Value::as_str)
                        != Some(context.context_hash.as_str())
                    {
                        return Err("stale_answer_result".to_string());
                    }
                    let cv_upload = verified_cv_upload_descriptor(
                        &state.adapter.career_ops_root,
                        &work,
                    )?;
                    let validated = state
                        .adapter
                        .validate_answers(
                            &work.preparation_id,
                            &work.report_path,
                            work.snapshot.clone(),
                            result,
                        )
                        .map_err(|_| "answer_validation_failed".to_string())?;
                    if validated.preparation_id != work.preparation_id
                        || validated.context_hash != context.context_hash
                    {
                        return Err("answer_identity_mismatch".to_string());
                    }
                    let mut store = state
                        .store
                        .lock()
                        .map_err(|_| "store_unavailable".to_string())?;
                    store
                        .complete_answer_work(
                            &work.session_id,
                            &validated.context_hash,
                            &validated.fill_plan,
                            &validated.review_items,
                            cv_upload.as_ref(),
                        )
                        .map_err(|_| "answer_commit_failed".to_string())?;
                    Ok(())
                })();
                if let Err(error_code) = outcome {
                    if let Ok(mut store) = app.state::<AppState>().store.lock() {
                        let _ = store.fail_answer_work(&work.session_id, &error_code);
                    }
                    let _ = app
                    .notification()
                    .builder()
                    .title("Application preparation needs attention")
                    .body("The live-form answer step stopped safely. Open HereForWork to retry it.")
                    .show();
                }
            }
        })
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn configure_browser_bridge(
    extension_id: String,
    installation_id: String,
    profile_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<BrowserSetup, String> {
    if extension_id.len() != 32 || !extension_id.bytes().all(|byte| matches!(byte, b'a'..=b'p')) {
        return Err("Extension ID must be the 32-letter ID shown by Chrome.".to_string());
    }
    if uuid::Uuid::parse_str(&installation_id).is_err() {
        return Err("Installation ID must be the UUID shown by the extension.".to_string());
    }
    if !chrome_profiles(&state.home_dir)
        .iter()
        .any(|profile| profile.id == profile_id)
    {
        return Err("Select a Chrome profile reported by this Mac.".to_string());
    }
    if !state.native_host_path.is_file() {
        return Err("The native messaging host binary is missing.".to_string());
    }
    let manifest_directory = state
        .home_dir
        .join("Library/Application Support/Google/Chrome/NativeMessagingHosts");
    std::fs::create_dir_all(&manifest_directory).map_err(|error| error.to_string())?;
    let manifest_path = manifest_directory.join("com.hereforwork.bridge.json");
    let manifest = serde_json::json!({
        "name": "com.hereforwork.bridge",
        "description": "HereForWork local browser bridge",
        "path": state.native_host_path,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{extension_id}/")]
    });
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store
        .configure_browser(&extension_id, &installation_id, &profile_id)
        .map_err(|error| error.to_string())?;
    drop(store);
    get_browser_setup(state)
}

#[tauri::command]
fn create_operational_backup(
    state: tauri::State<'_, AppState>,
) -> Result<MaintenanceResult, String> {
    let backup_directory = state.app_data_dir.join("backups");
    std::fs::create_dir_all(&backup_directory).map_err(|error| error.to_string())?;
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let path = backup_directory.join(format!("here-for-work-{timestamp}.sqlite3"));
    let store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    let integrity = store.integrity_check().map_err(|error| error.to_string())?;
    if integrity != "ok" {
        return Err(format!(
            "Backup refused because integrity_check returned {integrity}."
        ));
    }
    store.backup_to(&path).map_err(|error| error.to_string())?;
    Ok(MaintenanceResult {
        path: path.display().to_string(),
        integrity,
    })
}

#[tauri::command]
fn export_operational_summary(
    state: tauri::State<'_, AppState>,
) -> Result<MaintenanceResult, String> {
    let export_directory = state.app_data_dir.join("exports");
    std::fs::create_dir_all(&export_directory).map_err(|error| error.to_string())?;
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let path = export_directory.join(format!("here-for-work-summary-{timestamp}.json"));
    let store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    let integrity = store.integrity_check().map_err(|error| error.to_string())?;
    let dashboard = store.dashboard().map_err(|error| error.to_string())?;
    let payload = serde_json::json!({
        "schemaVersion": 1,
        "exportedAt": chrono::Utc::now().to_rfc3339(),
        "integrity": integrity,
        "dashboard": dashboard
    });
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&payload).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(MaintenanceResult {
        path: path.display().to_string(),
        integrity,
    })
}

#[tauri::command]
fn export_redacted_diagnostics(
    state: tauri::State<'_, AppState>,
) -> Result<MaintenanceResult, String> {
    let export_directory = state.app_data_dir.join("diagnostics");
    std::fs::create_dir_all(&export_directory).map_err(|error| error.to_string())?;
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let path = export_directory.join(format!("here-for-work-diagnostics-{timestamp}.json"));
    let store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    let integrity = store.integrity_check().map_err(|error| error.to_string())?;
    let diagnostics = store
        .redacted_diagnostics()
        .map_err(|error| error.to_string())?;
    let payload = serde_json::json!({
        "schemaVersion": 1,
        "exportedAt": chrono::Utc::now().to_rfc3339(),
        "integrity": integrity,
        "diagnostics": diagnostics,
    });
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&payload).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(MaintenanceResult {
        path: path.display().to_string(),
        integrity,
    })
}

#[tauri::command]
fn preflight_latest_backup(state: tauri::State<'_, AppState>) -> Result<RestorePreflight, String> {
    let backup_directory = state.app_data_dir.join("backups");
    let latest = std::fs::read_dir(&backup_directory)
        .map_err(|_| "Create a backup before running restore preflight.".to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("sqlite3"))
        .max_by_key(|path| path.file_name().map(|value| value.to_os_string()))
        .ok_or("No operational backup is available for preflight.")?;
    Store::restore_preflight(&latest).map_err(|error| error.to_string())
}

#[tauri::command]
fn check_integrations(state: tauri::State<'_, AppState>) -> IntegrationHealth {
    let career_ops = match state.adapter.health() {
        Ok(true) => CheckResult {
            ready: true,
            detail: "career-ops canonical history is available.".to_string(),
        },
        Ok(false) => CheckResult {
            ready: false,
            detail: "career-ops adapter checks did not all pass.".to_string(),
        },
        Err(error) => CheckResult {
            ready: false,
            detail: error.to_string(),
        },
    };
    IntegrationHealth {
        career_ops,
        codex: provider::check_cli(state.codex_path.as_ref(), "Codex"),
        claude: provider::check_cli(state.claude_path.as_ref(), "Claude"),
    }
}

#[tauri::command]
fn reconcile_application_history(
    state: tauri::State<'_, AppState>,
) -> Result<ReconcileResult, String> {
    let records = state
        .adapter
        .history_snapshot()
        .map_err(|error| error.to_string())?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store
        .set_adapter_ready(true)
        .map_err(|error| error.to_string())?;
    store
        .reconcile_history(&records)
        .map_err(|error| error.to_string())
}

fn madrid_today() -> String {
    chrono::Utc::now()
        .with_timezone(&chrono_tz::Europe::Madrid)
        .format("%Y-%m-%d")
        .to_string()
}

#[tauri::command]
fn dismiss_role(
    role_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<DashboardState, String> {
    discard_role_internal(&role_id, &state)
}

fn discard_role_internal(role_id: &str, state: &AppState) -> Result<DashboardState, String> {
    let effect = {
        let mut store = state
            .store
            .lock()
            .map_err(|_| "Operational store lock was poisoned".to_string())?;
        store
            .begin_discard_effect(role_id)
            .map_err(|error| error.to_string())?
    };
    let input = CanonicalRoleInput {
        idempotency_key: effect.idempotency_key.clone(),
        event_date: madrid_today(),
        company: effect.role.company,
        title: effect.role.title,
        location: effect.role.location,
        url: effect.role.application_url,
    };
    let canonical = {
        let _canonical_write = state
            .canonical_write_lock
            .lock()
            .map_err(|_| "Canonical writer lock was poisoned".to_string())?;
        match state.adapter.discard_role(&input) {
            Ok(canonical) => canonical,
            Err(error) => {
                if let Ok(mut store) = state.store.lock() {
                    let _ = store
                        .fail_adapter_effect(&effect.idempotency_key, "canonical_writer_failed");
                }
                return Err(error.to_string());
            }
        }
    };
    if canonical.idempotency_key != effect.idempotency_key {
        return Err("career-ops returned a mismatched idempotency key".to_string());
    }
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store
        .complete_discard_effect(
            role_id,
            &effect.idempotency_key,
            canonical.tracker_id,
            &canonical.status,
        )
        .map_err(|error| error.to_string())?;
    store.dashboard().map_err(|error| error.to_string())
}

#[tauri::command]
fn undo_preparation(
    preparation_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<DashboardState, String> {
    let work = {
        let mut store = state
            .store
            .lock()
            .map_err(|_| "Operational store lock was poisoned".to_string())?;
        store
            .begin_preparation_cleanup(&preparation_id)
            .map_err(|error| error.to_string())?
    };
    let input = CanonicalRoleInput {
        idempotency_key: work.effect.idempotency_key.clone(),
        event_date: madrid_today(),
        company: work.effect.role.company.clone(),
        title: work.effect.role.title.clone(),
        location: work.effect.role.location.clone(),
        url: work.effect.role.application_url.clone(),
    };
    let (canonical, cleanup_result) = {
        let _canonical_write = state
            .canonical_write_lock
            .lock()
            .map_err(|_| "Canonical writer lock was poisoned".to_string())?;
        let canonical = match state.adapter.discard_role(&input) {
            Ok(canonical) => canonical,
            Err(error) => {
                if let Ok(mut store) = state.store.lock() {
                    let _ = store.fail_preparation_cleanup(
                        &work.preparation_id,
                        &work.effect.idempotency_key,
                        "canonical_writer_failed",
                    );
                }
                return Err(error.to_string());
            }
        };
        let cleanup_result = state.adapter.delete_preparation_artifacts(
            &work.preparation_id,
            &work.report_path,
            &work.cv_pdf_path,
        );
        (canonical, cleanup_result)
    };
    if canonical.idempotency_key != work.effect.idempotency_key {
        return Err("career-ops returned a mismatched idempotency key".to_string());
    }
    if let Err(error) = cleanup_result {
        if let Ok(mut store) = state.store.lock() {
            let _ = store.fail_preparation_cleanup(
                &work.preparation_id,
                &work.effect.idempotency_key,
                "artifact_cleanup_failed",
            );
        }
        return Err(error.to_string());
    }
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store
        .complete_preparation_cleanup(&work, canonical.tracker_id, &canonical.status)
        .map_err(|error| error.to_string())?;
    store.dashboard().map_err(|error| error.to_string())
}

#[tauri::command]
fn undo_dismissal(
    role_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<DashboardState, String> {
    let effect = {
        let mut store = state
            .store
            .lock()
            .map_err(|_| "Operational store lock was poisoned".to_string())?;
        store
            .begin_undo_discard_effect(&role_id)
            .map_err(|error| error.to_string())?
    };
    let parent_effect_key = effect
        .parent_effect_key
        .as_deref()
        .ok_or("Undo is missing its dismissal effect key")?;
    let tracker_id = effect.tracker_id.ok_or("Undo is missing its tracker id")?;
    let canonical = {
        let _canonical_write = state
            .canonical_write_lock
            .lock()
            .map_err(|_| "Canonical writer lock was poisoned".to_string())?;
        match state.adapter.undo_discard(
            &effect.idempotency_key,
            parent_effect_key,
            tracker_id,
            &madrid_today(),
        ) {
            Ok(canonical) => canonical,
            Err(error) => {
                if let Ok(mut store) = state.store.lock() {
                    let _ = store
                        .fail_adapter_effect(&effect.idempotency_key, "canonical_writer_failed");
                }
                return Err(error.to_string());
            }
        }
    };
    if canonical.idempotency_key != effect.idempotency_key {
        return Err("career-ops returned a mismatched idempotency key".to_string());
    }
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Operational store lock was poisoned".to_string())?;
    store
        .complete_undo_discard_effect(
            &role_id,
            &effect.idempotency_key,
            canonical.tracker_id,
            &canonical.status,
        )
        .map_err(|error| error.to_string())?;
    store.dashboard().map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let application = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            app.handle().plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec!["--background"]),
            ))?;
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            let mut store = Store::open(app_data_dir.join("here-for-work.sqlite3"))?;
            store.recover_interrupted_preparations()?;
            let _ = store.reconcile_due_runs(chrono::Utc::now())?;
            let home_dir = app.path().home_dir()?;
            let bundled_adapter = app
                .path()
                .resource_dir()?
                .join("career-ops-adapter/adapter.mjs");
            let development_adapter = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../packages/career-ops-adapter/adapter.mjs");
            let script_path = if bundled_adapter.is_file() {
                bundled_adapter
            } else {
                development_adapter
            };
            let bundled_provider_schema = app
                .path()
                .resource_dir()?
                .join("contracts/provider-probe.schema.json");
            let development_provider_schema = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../contracts/provider-probe.schema.json");
            let provider_schema_path = if bundled_provider_schema.is_file() {
                bundled_provider_schema
            } else {
                development_provider_schema
            };
            let bundled_preparation_schema = app
                .path()
                .resource_dir()?
                .join("contracts/preparation-result.schema.json");
            let development_preparation_schema = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../contracts/preparation-result.schema.json");
            let preparation_schema_path = if bundled_preparation_schema.is_file() {
                bundled_preparation_schema
            } else {
                development_preparation_schema
            };
            let bundled_answer_schema = app
                .path()
                .resource_dir()?
                .join("contracts/answer-draft.schema.json");
            let development_answer_schema = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../contracts/answer-draft.schema.json");
            let answer_schema_path = if bundled_answer_schema.is_file() {
                bundled_answer_schema
            } else {
                development_answer_schema
            };
            let provider_sandbox_path = app_data_dir.join("provider-sandbox");
            std::fs::create_dir_all(&provider_sandbox_path)?;
            let career_ops_staging_path = app_data_dir.join("career-ops-adapter-staging");
            std::fs::create_dir_all(&career_ops_staging_path)?;
            let bundled_extension = app.path().resource_dir()?.join("extension");
            let development_extension =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../packages/extension/dist");
            let extension_directory = if bundled_extension.is_dir() {
                bundled_extension
            } else {
                development_extension
            };
            let sibling_native_host = std::env::current_exe()?
                .parent()
                .map(|directory| directory.join("here-for-work-native-host"));
            let development_native_host = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target/release/here-for-work-native-host");
            let native_host_path = sibling_native_host
                .filter(|path| path.is_file())
                .unwrap_or(development_native_host);
            let node_path = discover_executable(&home_dir, "node")
                .ok_or("A compatible Node executable was not detected")?;
            let launch_in_background = std::env::args().any(|argument| argument == "--background");
            let background_enabled = store.background_enabled()?;
            if background_enabled && !app.autolaunch().is_enabled().unwrap_or(false) {
                let _ = app.autolaunch().enable();
            }
            if !background_enabled && app.autolaunch().is_enabled().unwrap_or(false) {
                let _ = app.autolaunch().disable();
            }
            let career_ops_root = home_dir.join("Work/career-ops");
            let tracker_index_path = career_ops_root.join("data/applications.db");
            let adapter = AdapterConfig {
                node_path,
                script_path,
                career_ops_root,
                tracker_index_path,
                staging_path: career_ops_staging_path,
            };
            if !store.queue_filters_configured()? {
                if let Ok(defaults) = adapter.queue_filter_defaults() {
                    store.set_queue_filters(&defaults)?;
                }
            }
            app.manage(AppState {
                store: Mutex::new(store),
                adapter,
                codex_path: discover_executable(&home_dir, "codex"),
                claude_path: discover_executable(&home_dir, "claude"),
                provider_schema_path,
                preparation_schema_path,
                answer_schema_path,
                provider_sandbox_path,
                home_dir: home_dir.clone(),
                extension_directory,
                native_host_path,
                app_data_dir: app_data_dir.clone(),
                preparation_cancellations: Mutex::new(HashMap::new()),
                canonical_write_lock: Mutex::new(()),
            });
            bridge::start(app.handle().clone(), bridge::socket_path(&app_data_dir))?;
            start_preparation_workers(app.handle().clone())?;
            start_browser_handoff_worker(app.handle().clone())?;
            start_answer_worker(app.handle().clone())?;
            if launch_in_background {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard,
            import_dataset,
            set_background_enabled,
            save_queue_filters,
            request_notification_permission,
            send_test_notification,
            queue_manual_discovery,
            run_provider_probe,
            prepare_role,
            cancel_preparation,
            get_preparation_detail,
            open_preparation_artifact,
            get_browser_setup,
            configure_browser_bridge,
            get_browser_sessions,
            start_browser_connection_check,
            retry_browser_session,
            continue_in_browser,
            confirm_application_applied,
            quit_app,
            create_operational_backup,
            export_operational_summary,
            export_redacted_diagnostics,
            preflight_latest_backup,
            check_integrations,
            reconcile_application_history,
            dismiss_role,
            undo_preparation,
            undo_dismissal
        ])
        .build(tauri::generate_context!())
        .expect("error while building HereForWork");

    application.run(|app_handle, event| {
        if let tauri::RunEvent::Reopen { .. } = event {
            if let Ok(mut store) = app_handle.state::<AppState>().store.lock() {
                let _ = store.reconcile_due_runs(chrono::Utc::now());
            }
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::PreparationGate;

    #[test]
    fn chrome_handoff_opens_the_selected_profile_without_automation_or_focus_flags() {
        let command = super::background_chrome_command(
            "Profile 1",
            "https://jobs.example.test/application",
        );
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(arguments.first().map(String::as_str), Some("-g"));
        assert!(arguments.contains(&"--profile-directory=Profile 1".to_string()));
        assert!(arguments.contains(&"https://jobs.example.test/application".to_string()));
        assert!(!arguments.iter().any(|argument| {
            argument.contains("automation")
                || argument.contains("remote-debugging")
                || argument.contains("webdriver")
        }));
    }

    #[test]
    fn preparation_artifacts_cannot_escape_career_ops_root() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("career-ops");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("report.md"), "verified").unwrap();
        std::fs::write(directory.path().join("outside.md"), "private").unwrap();

        assert!(super::checked_career_ops_artifact(&root, "report.md").is_ok());
        assert!(super::checked_career_ops_artifact(&root, "../outside.md").is_err());
    }

    #[test]
    fn cv_upload_descriptor_is_bound_to_the_verified_preparation_artifact() {
        use sha2::{Digest, Sha256};

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("career-ops");
        let cv_directory = root.join("output/role/cv/tailored/v001");
        std::fs::create_dir_all(&cv_directory).unwrap();
        let bytes = b"%PDF-1 verified tailored CV";
        std::fs::write(cv_directory.join("cv.pdf"), bytes).unwrap();
        let hash = format!("{:x}", Sha256::digest(bytes));
        let work = crate::domain::BrowserAnswerWork {
            session_id: "session".to_string(),
            preparation_id: "preparation".to_string(),
            provider: "codex".to_string(),
            report_path: "reports/role.md".to_string(),
            cv_pdf_path: "output/role/cv/tailored/v001/cv.pdf".to_string(),
            cv_pdf_hash: hash.clone(),
            snapshot: serde_json::json!({
                "fields": [{
                    "id": "resume",
                    "control": "input",
                    "inputType": "file",
                    "classification": "safe_verified"
                }]
            }),
            snapshot_fingerprint: "a".repeat(64),
        };

        let descriptor = super::verified_cv_upload_descriptor(&root, &work)
            .unwrap()
            .unwrap();

        assert_eq!(descriptor["fieldId"], "resume");
        assert_eq!(descriptor["relativePath"], work.cv_pdf_path);
        assert_eq!(descriptor["sha256"], hash);
        assert!(descriptor.get("contentBase64").is_none());
    }

    #[test]
    fn preparation_gate_allows_viable_matches_when_authorization_is_not_explicitly_blocked() {
        for (legitimacy, authorization) in [
            ("High Confidence", "excellent"),
            ("High Confidence", "interesting"),
            ("High Confidence", "investigate"),
            ("Proceed with Caution", "excellent"),
        ] {
            let result = serde_json::json!({
                "score": 4.0,
                "legitimacy": legitimacy,
                "authorizationConfidence": authorization
            });

            assert!(matches!(
                super::preparation_gate(&result).unwrap(),
                PreparationGate::Proceed
            ));
        }
    }

    #[test]
    fn preparation_gate_holds_low_match_scores_before_artifacts() {
        let result = serde_json::json!({
            "score": 3.9,
            "legitimacy": "High Confidence",
            "authorizationConfidence": "excellent"
        });
        assert!(matches!(
            super::preparation_gate(&result).unwrap(),
            PreparationGate::NeedsDecision(_)
        ));
    }

    #[test]
    fn preparation_gate_allows_caution_when_no_concrete_blocker_exists() {
        let result = serde_json::json!({
            "score": 4.5,
            "legitimacy": "Proceed with Caution",
            "authorizationConfidence": "investigate"
        });
        assert!(matches!(
            super::preparation_gate(&result).unwrap(),
            PreparationGate::Proceed
        ));
    }

    #[test]
    fn preparation_gate_discards_confirmed_blockers_before_artifacts() {
        for result in [
            serde_json::json!({
                "score": 5.0,
                "legitimacy": "High Confidence",
                "authorizationConfidence": "problem"
            }),
            serde_json::json!({
                "score": 5.0,
                "legitimacy": "Suspicious",
                "authorizationConfidence": "excellent"
            }),
        ] {
            assert!(matches!(
                super::preparation_gate(&result).unwrap(),
                PreparationGate::Discard(_)
            ));
        }
    }
}
