use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QueueGroup {
    StrongMatch,
    OtherNew,
    NeedsDecision,
}

impl QueueGroup {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StrongMatch => "strong_match",
            Self::OtherNew => "other_new",
            Self::NeedsDecision => "needs_decision",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "strong_match" => Some(Self::StrongMatch),
            "other_new" => Some(Self::OtherNew),
            "needs_decision" => Some(Self::NeedsDecision),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryLegitimacy {
    HighConfidence,
    ProceedWithCaution,
    Suspicious,
}

impl DiscoveryLegitimacy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HighConfidence => "high_confidence",
            Self::ProceedWithCaution => "proceed_with_caution",
            Self::Suspicious => "suspicious",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleSummary {
    pub id: String,
    pub company: String,
    pub title: String,
    pub location: String,
    pub source: String,
    pub source_count: i64,
    pub queue_group: QueueGroup,
    pub eligibility_summary: String,
    pub uncertainty: Option<String>,
    pub discovered_at: String,
    pub application_url: Option<String>,
    pub preparation_state: String,
    pub canonical_tracker_id: Option<i64>,
    pub canonical_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub id: String,
    pub kind: String,
    pub message: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardState {
    pub roles: Vec<RoleSummary>,
    pub recently_dismissed: Vec<RoleSummary>,
    pub preparations: Vec<PreparationSummary>,
    pub activity: Vec<ActivityEntry>,
    pub queue_filters: QueueFilters,
    pub last_successful_discovery_at: Option<String>,
    pub background_enabled: bool,
    pub adapter_status: String,
    pub handled_count: i64,
    pub pending_run_count: i64,
    pub action_required_run_count: i64,
    pub sources: Vec<SourceScheduleSummary>,
    pub recent_runs: Vec<RunSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QueueFilters {
    pub role_families: Vec<String>,
    pub seniority: Vec<String>,
    pub locations: Vec<String>,
    pub remote_allowed: bool,
    pub require_authorization_path: bool,
}

impl Default for QueueFilters {
    fn default() -> Self {
        Self {
            role_families: Vec::new(),
            seniority: Vec::new(),
            locations: Vec::new(),
            remote_allowed: true,
            require_authorization_path: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareRoleOutcome {
    pub dashboard: DashboardState,
    pub disposition: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct PreparationWork {
    pub id: String,
    pub role: AdapterRoleContext,
}

#[derive(Debug, Clone)]
pub struct PreparationCleanupWork {
    pub preparation_id: String,
    pub role_id: String,
    pub report_path: String,
    pub cv_pdf_path: String,
    pub effect: AdapterEffectContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationSummary {
    pub id: String,
    pub role_id: String,
    pub company: String,
    pub title: String,
    pub provider: String,
    pub status: String,
    pub step: String,
    pub attempt: i64,
    pub report_path: Option<String>,
    pub cv_pdf_path: Option<String>,
    pub error_class: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationDetail {
    pub preparation_id: String,
    pub report_markdown: String,
    pub report_path: String,
    pub cv_pdf_path: String,
}

#[derive(Debug, Clone)]
pub struct AdapterRoleContext {
    pub company: String,
    pub title: String,
    pub location: String,
    pub application_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdapterEffectContext {
    pub idempotency_key: String,
    pub role: AdapterRoleContext,
    pub parent_effect_key: Option<String>,
    pub tracker_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledRun {
    pub id: String,
    pub source_id: String,
    pub coverage_start: String,
    pub coverage_end: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceScheduleSummary {
    pub source_id: String,
    pub display_name: String,
    pub timezone: String,
    pub schedule_hours: String,
    pub execution_mode: String,
    pub last_successful_at: Option<String>,
    pub action_required_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub id: String,
    pub source_id: String,
    pub kind: String,
    pub coverage_start: String,
    pub coverage_end: String,
    pub status: String,
    pub attempt: i64,
    pub error_class: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(test), allow(dead_code))]
pub struct LeasedRun {
    pub id: String,
    pub source_id: String,
    pub coverage_start: String,
    pub coverage_end: String,
    pub attempt: i64,
    pub step_name: String,
    pub lease_expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromeProfile {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSetup {
    pub profiles: Vec<ChromeProfile>,
    pub selected_profile_id: Option<String>,
    pub approved_extension_id: Option<String>,
    pub pending_extension_id: Option<String>,
    pub approved_installation_id: Option<String>,
    pub pending_installation_id: Option<String>,
    pub extension_directory: String,
    pub native_host_registered: bool,
    pub last_connected_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSessionSummary {
    pub id: String,
    pub purpose: String,
    pub role_id: Option<String>,
    pub preparation_id: Option<String>,
    pub status: String,
    pub ats: Option<String>,
    pub page_title: Option<String>,
    pub page_url: Option<String>,
    pub snapshot_fingerprint: Option<String>,
    pub field_count: i64,
    pub safe_field_count: i64,
    pub needs_user_count: i64,
    pub error_code: Option<String>,
    pub review_items: Option<serde_json::Value>,
    pub fill_results: Option<serde_json::Value>,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct BrowserAnswerWork {
    pub session_id: String,
    pub preparation_id: String,
    pub provider: String,
    pub report_path: String,
    pub cv_pdf_path: String,
    pub cv_pdf_hash: String,
    pub snapshot: serde_json::Value,
    pub snapshot_fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct BrowserAnswerCommitWork {
    pub session_id: String,
    pub preparation_id: String,
    pub report_path: String,
    pub cv_pdf_path: String,
    pub context_hash: String,
    pub review_items: serde_json::Value,
    pub fill_results: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCommand {
    pub command_id: String,
    pub session_id: String,
    pub command_type: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct BrowserInspection {
    pub ats: String,
    pub page_title: String,
    pub page_url: String,
    pub snapshot_fingerprint: String,
    pub fields: serde_json::Value,
    pub safe_field_count: usize,
    pub needs_user_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceResult {
    pub path: String,
    pub integrity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePreflight {
    pub path: String,
    pub integrity: String,
    pub schema_version: i64,
    pub role_count: i64,
    pub run_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryDataset {
    pub schema_version: u32,
    pub generated_at: String,
    pub findings: Vec<DiscoveryFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryFinding {
    pub source_id: String,
    pub source: String,
    pub source_role_id: String,
    pub company: String,
    pub title: String,
    pub location: String,
    pub discovered_at: String,
    pub application_url: Option<String>,
    pub normalized_key: String,
    pub queue_group: QueueGroup,
    pub eligibility_summary: String,
    pub uncertainty: Option<String>,
    #[serde(default)]
    pub legitimacy: Option<DiscoveryLegitimacy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: usize,
    pub updated: usize,
    pub unchanged: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub id: i64,
    pub date: String,
    pub company: String,
    pub role: String,
    pub score: String,
    pub status: String,
    pub pdf: String,
    pub report: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileResult {
    pub matched: usize,
    pub cleared: usize,
    pub unmatched: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub ready: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationHealth {
    pub career_ops: CheckResult,
    pub codex: CheckResult,
    pub claude: CheckResult,
}
