use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize};

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
    pub posted_at: Option<String>,
    pub discovered_at: String,
    pub application_url: Option<String>,
    pub preparation_state: String,
    pub canonical_tracker_id: Option<i64>,
    pub canonical_status: Option<String>,
    pub evaluation: Option<QueueEvaluationSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueEvaluationSummary {
    pub native_score: f64,
    pub legitimacy: String,
    pub risk_level: String,
    pub strengths: Vec<String>,
    pub blockers: Vec<String>,
    pub gaps: Vec<String>,
    pub compensation: Option<String>,
    pub authorization_confidence: String,
    pub authorization_question: String,
    pub material_uncertainty: EvaluationMaterialUncertainty,
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
    pub pre_queue_roles: Vec<PreQueueRoleSummary>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreQueueRoleSummary {
    pub role_id: String,
    pub company: String,
    pub title: String,
    pub state: String,
    pub reason: String,
    pub attempt: i64,
    pub updated_at: String,
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
    pub role_id: String,
    pub provider: String,
    pub role: AdapterRoleContext,
}

#[derive(Debug, Clone)]
pub struct PreparationCleanupWork {
    pub preparation_id: String,
    pub role_id: String,
    pub report_path: Option<String>,
    pub cv_pdf_path: Option<String>,
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
    pub cv_source: Option<String>,
    pub error_class: Option<String>,
    pub error_stage: Option<String>,
    pub error_detail: Option<String>,
    pub retry_policy: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationDetail {
    pub preparation_id: String,
    pub role_id: String,
    pub company: String,
    pub title: String,
    pub provider: String,
    pub status: String,
    pub stage: String,
    pub error_class: Option<String>,
    pub error_detail: Option<String>,
    pub retry_policy: Option<String>,
    pub report_markdown: Option<String>,
    pub report_path: Option<String>,
    pub cv_pdf_path: Option<String>,
    pub cv_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CvFallbackSetting {
    pub path: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeNotification {
    pub id: String,
    pub event_kind: String,
    pub title: String,
    pub body: String,
    pub action_kind: String,
    pub action_label: String,
    pub role_id: String,
    pub preparation_id: String,
    pub browser_session_id: Option<String>,
    pub created_at: String,
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
    #[serde(default)]
    pub posted_at: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
pub struct EvaluationResultRead {
    pub contract: String,
    pub schema_version: u32,
    pub upstream_revision: String,
    pub compatibility_fingerprint: String,
    pub report: EvaluationReportReference,
    pub role: EvaluationRoleIdentity,
    pub canonical: EvaluationCanonicalIdentity,
    pub evaluation: EvaluationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
pub struct EvaluationReportReference {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
pub struct EvaluationRoleIdentity {
    pub company: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
pub struct EvaluationCanonicalIdentity {
    pub tracker_id: i64,
    pub status: String,
    pub score: f64,
    pub report_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
pub struct EvaluationSummary {
    pub score: f64,
    pub final_decision: String,
    pub legitimacy_tier: String,
    pub archetype: String,
    pub next_action: String,
    pub strengths: Vec<String>,
    pub blockers: Vec<String>,
    pub gaps: Vec<String>,
    pub compensation: EvaluationCompensation,
    pub authorization: EvaluationAuthorization,
    pub risk_level: String,
    pub confidence: String,
    pub risk_summary: EvaluationRiskSummary,
    pub material_uncertainty: EvaluationMaterialUncertainty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
pub struct EvaluationCompensation {
    pub advertised: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
pub struct EvaluationAuthorization {
    pub confidence: String,
    #[serde(deserialize_with = "deserialize_non_empty_evidence")]
    pub evidence: Vec<String>,
    pub scope: String,
    pub engagement_mechanism: String,
    pub question: String,
    pub legacy_work_auth: String,
}

#[allow(dead_code)]
fn deserialize_non_empty_evidence<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let evidence = Vec::<String>::deserialize(deserializer)?;
    if evidence.is_empty()
        || evidence.len() > 8
        || evidence
            .iter()
            .any(|item| item.trim().is_empty() || item.len() > 2_000)
    {
        return Err(serde::de::Error::custom(
            "authorization evidence must contain one to eight non-empty items",
        ));
    }
    Ok(evidence)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
pub struct EvaluationRiskSummary {
    pub legitimacy: String,
    pub classification: String,
    pub culture: String,
    pub interview_redflags: String,
    pub ai_infra: String,
    pub ai_screening_disclosure: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
pub struct EvaluationMaterialUncertainty {
    pub confidence: String,
    pub authorization_question: String,
    pub not_evaluated_risk_signals: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EvaluationSyncRole {
    pub role_id: String,
    pub company: String,
    pub title: String,
    pub source_identity_hash: String,
    pub canonical_status: Option<String>,
    pub canonical_tracker_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationSyncResult {
    pub promoted: usize,
    pub unchanged: usize,
    pub held: usize,
    pub terminal: usize,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CareerOpsCapabilityId {
    #[serde(rename = "discovery.reverse_ats.run.v1")]
    DiscoveryReverseAtsRunV1,
    #[serde(rename = "discovery.company_ats.preview.v1")]
    DiscoveryCompanyAtsPreviewV1,
    #[serde(rename = "liveness.role.read.v1")]
    LivenessRoleReadV1,
    #[serde(rename = "evaluation.full_ag.run.v1")]
    EvaluationFullAgRunV1,
    #[serde(rename = "evaluation.result.read.v1")]
    EvaluationResultReadV1,
    #[serde(rename = "artifacts.inspect.v1")]
    ArtifactsInspectV1,
    #[serde(rename = "browser.review_fallback.v1")]
    BrowserReviewFallbackV1,
    #[serde(rename = "canonical.applied.write.v1")]
    CanonicalAppliedWriteV1,
}

impl CareerOpsCapabilityId {
    fn all() -> [Self; 8] {
        [
            Self::DiscoveryReverseAtsRunV1,
            Self::DiscoveryCompanyAtsPreviewV1,
            Self::LivenessRoleReadV1,
            Self::EvaluationFullAgRunV1,
            Self::EvaluationResultReadV1,
            Self::ArtifactsInspectV1,
            Self::BrowserReviewFallbackV1,
            Self::CanonicalAppliedWriteV1,
        ]
    }

    fn expected_interface_class(self) -> CareerOpsInterfaceClass {
        match self {
            Self::DiscoveryReverseAtsRunV1 | Self::EvaluationResultReadV1 => {
                CareerOpsInterfaceClass::Conditional
            }
            Self::DiscoveryCompanyAtsPreviewV1 | Self::CanonicalAppliedWriteV1 => {
                CareerOpsInterfaceClass::Contracted
            }
            Self::LivenessRoleReadV1
            | Self::EvaluationFullAgRunV1
            | Self::ArtifactsInspectV1
            | Self::BrowserReviewFallbackV1 => CareerOpsInterfaceClass::Missing,
        }
    }

    fn permits_status(self, status: CareerOpsCapabilityStatus) -> bool {
        match self {
            Self::DiscoveryReverseAtsRunV1
            | Self::DiscoveryCompanyAtsPreviewV1
            | Self::EvaluationResultReadV1 => matches!(
                status,
                CareerOpsCapabilityStatus::Degraded | CareerOpsCapabilityStatus::Unavailable
            ),
            Self::LivenessRoleReadV1
            | Self::EvaluationFullAgRunV1
            | Self::ArtifactsInspectV1
            | Self::BrowserReviewFallbackV1 => status == CareerOpsCapabilityStatus::Unavailable,
            Self::CanonicalAppliedWriteV1 => matches!(
                status,
                CareerOpsCapabilityStatus::Degraded | CareerOpsCapabilityStatus::Unavailable
            ),
        }
    }

    fn expected_constraints(self) -> &'static [CareerOpsCapabilityConstraint] {
        use CareerOpsCapabilityConstraint as Constraint;
        match self {
            Self::DiscoveryReverseAtsRunV1 => &[
                Constraint::RequiresExactUpstreamRevision,
                Constraint::RequiresIsolatedExecution,
                Constraint::WritesNoCareerOpsCheckoutState,
            ],
            Self::DiscoveryCompanyAtsPreviewV1 => &[
                Constraint::RequiresExactUpstreamRevision,
                Constraint::RequiresSafeShapeProbe,
                Constraint::PreviewOnly,
                Constraint::NoImplicitConfigWrite,
            ],
            Self::LivenessRoleReadV1 => &[
                Constraint::RequiresExactUpstreamRevision,
                Constraint::RequiresTypedPerRoleEvidence,
            ],
            Self::EvaluationFullAgRunV1 => &[
                Constraint::RequiresExactUpstreamRevision,
                Constraint::RequiresAtomicEvaluationReceipt,
                Constraint::NativeScore1To5,
            ],
            Self::EvaluationResultReadV1 => &[
                Constraint::RequiresExactUpstreamRevision,
                Constraint::RequiresSafeShapeProbe,
                Constraint::NativeScore1To5,
                Constraint::RequiresReportTrackerIdentity,
            ],
            Self::ArtifactsInspectV1 => &[
                Constraint::RequiresExactUpstreamRevision,
                Constraint::RequiresStructuredArtifactProvenance,
            ],
            Self::BrowserReviewFallbackV1 => &[
                Constraint::RequiresExactUpstreamRevision,
                Constraint::RequiresSingleDriverLeaseTransfer,
                Constraint::ReviewOnlyNoSubmit,
            ],
            Self::CanonicalAppliedWriteV1 => &[
                Constraint::RequiresExactUpstreamRevision,
                Constraint::CanonicalTrackingOnly,
                Constraint::RequiresUserConfirmedSubmission,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CareerOpsCapabilityStatus {
    Supported,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CareerOpsInterfaceClass {
    Contracted,
    Conditional,
    Missing,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CareerOpsCapabilityConstraint {
    RequiresExactUpstreamRevision,
    RequiresIsolatedExecution,
    WritesNoCareerOpsCheckoutState,
    RequiresSafeShapeProbe,
    PreviewOnly,
    NoImplicitConfigWrite,
    RequiresTypedPerRoleEvidence,
    RequiresAtomicEvaluationReceipt,
    #[serde(rename = "native_score_1_to_5")]
    NativeScore1To5,
    RequiresReportTrackerIdentity,
    RequiresStructuredArtifactProvenance,
    RequiresSingleDriverLeaseTransfer,
    ReviewOnlyNoSubmit,
    CanonicalTrackingOnly,
    RequiresUserConfirmedSubmission,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerOpsCapability {
    pub id: CareerOpsCapabilityId,
    pub status: CareerOpsCapabilityStatus,
    pub interface_class: CareerOpsInterfaceClass,
    pub source_revision: Option<String>,
    pub probe_revision: String,
    pub compatibility_fingerprint: Option<String>,
    pub constraints: Vec<CareerOpsCapabilityConstraint>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CareerOpsThresholdSource {
    Configured,
    UpstreamDefault,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerOpsAutoPdfScoreThreshold {
    pub value: Option<f64>,
    pub source: CareerOpsThresholdSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerOpsSourceOfTruth {
    pub profile_facts: String,
    pub evaluation: String,
    pub generated_artifacts: String,
    pub grounded_answers: String,
    pub application_history: String,
    pub operational_state: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CareerOpsCapabilityDiagnosticCode {
    UpstreamRevisionUnavailable,
    UpstreamVersionUnavailable,
    UpstreamVersionMismatch,
    AutoPdfThresholdInvalid,
    AutoPdfThresholdProductMismatch,
    IsolatedExecutionRequired,
    SafeShapeProbeRequired,
    TypedInterfaceUnavailable,
    StructuredProvenanceUnavailable,
    PublicBrowserFallbackUnavailable,
    CanonicalWriterUnavailable,
    CanonicalWriterCompatibilityUnverified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerOpsCapabilityDiagnostic {
    pub code: CareerOpsCapabilityDiagnosticCode,
    pub capability_id: Option<CareerOpsCapabilityId>,
    pub message: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareerOpsCapabilityManifest {
    pub contract: String,
    pub schema_version: u32,
    pub adapter_protocol_version: u32,
    pub upstream_revision: Option<String>,
    pub upstream_declared_version: Option<String>,
    pub auto_pdf_score_threshold: CareerOpsAutoPdfScoreThreshold,
    pub operations: Vec<String>,
    pub source_of_truth: CareerOpsSourceOfTruth,
    pub forbidden_operations: Vec<String>,
    pub capabilities: Vec<CareerOpsCapability>,
    pub diagnostics: Vec<CareerOpsCapabilityDiagnostic>,
}

impl CareerOpsCapabilityManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.contract != "hereforwork.career-ops-capabilities"
            || self.schema_version != 1
            || self.adapter_protocol_version != 1
        {
            return Err("capability manifest has an unsupported contract version".to_string());
        }
        if let Some(revision) = &self.upstream_revision
            && !is_lowercase_git_sha(revision)
        {
            return Err("capability manifest has an invalid upstream revision".to_string());
        }
        if let Some(version) = &self.upstream_declared_version
            && !is_safe_declared_version(version)
        {
            return Err("capability manifest has an invalid declared version".to_string());
        }
        match (
            self.auto_pdf_score_threshold.value,
            self.auto_pdf_score_threshold.source,
        ) {
            (None, CareerOpsThresholdSource::Unavailable) => {}
            (
                Some(value),
                CareerOpsThresholdSource::Configured | CareerOpsThresholdSource::UpstreamDefault,
            ) if value.is_finite() && (0.0..=5.0).contains(&value) => {}
            _ => return Err("capability manifest has an invalid PDF threshold".to_string()),
        }
        let forbidden = [
            "application.submit",
            "application.finalize",
            "message.send",
            "shell.run",
            "browser.command",
        ];
        if self.forbidden_operations.len() != forbidden.len()
            || forbidden.iter().any(|operation| {
                !self
                    .forbidden_operations
                    .iter()
                    .any(|value| value == operation)
            })
            || forbidden
                .iter()
                .any(|operation| self.operations.iter().any(|value| value == operation))
        {
            return Err("capability manifest changed the forbidden operation boundary".to_string());
        }
        let expected_operations = [
            "capabilities.get",
            "health.check",
            "history.snapshot",
            "evaluation.result.read.v1",
            "profile.queue_filters.get",
            "preparation.context.get",
            "preparation.result.recover",
            "preparation.result.commit",
            "preparation.artifacts.delete",
            "answers.context.get",
            "answers.result.validate",
            "answers.result.commit",
            "role.discard",
            "role.discard.undo",
            "application.applied.confirm",
        ];
        if self.operations.len() != expected_operations.len()
            || expected_operations.iter().any(|required| {
                !self
                    .operations
                    .iter()
                    .any(|operation| operation == required)
            })
        {
            return Err("capability manifest changed the adapter operation set".to_string());
        }
        if self.source_of_truth.profile_facts != "career-ops"
            || self.source_of_truth.evaluation != "career-ops"
            || self.source_of_truth.generated_artifacts != "career-ops"
            || self.source_of_truth.grounded_answers != "career-ops"
            || self.source_of_truth.application_history != "career-ops"
            || self.source_of_truth.operational_state != "here-for-work"
        {
            return Err("capability manifest changed source ownership".to_string());
        }
        if self.capabilities.len() != CareerOpsCapabilityId::all().len() {
            return Err("capability manifest has the wrong capability count".to_string());
        }
        let mut ids = HashSet::new();
        for capability in &self.capabilities {
            if !ids.insert(capability.id) {
                return Err("capability manifest contains a duplicate capability".to_string());
            }
            if capability.probe_revision != "hereforwork.career-ops-capability-probes.v1" {
                return Err("capability manifest has an unsupported probe revision".to_string());
            }
            if let Some(fingerprint) = &capability.compatibility_fingerprint
                && !is_lowercase_sha256(fingerprint)
            {
                return Err(
                    "capability manifest has an invalid compatibility fingerprint".to_string(),
                );
            }
            if capability.status == CareerOpsCapabilityStatus::Unavailable
                && capability.compatibility_fingerprint.is_some()
            {
                return Err(
                    "capability manifest exposed a fingerprint for an unavailable capability"
                        .to_string(),
                );
            }
            if capability.source_revision != self.upstream_revision {
                return Err("capability manifest mixes upstream revisions".to_string());
            }
            if self.upstream_revision.is_none()
                && capability.status != CareerOpsCapabilityStatus::Unavailable
            {
                return Err(
                    "capability manifest enabled a capability without an exact upstream revision"
                        .to_string(),
                );
            }
            if capability.interface_class != capability.id.expected_interface_class()
                || !capability.id.permits_status(capability.status)
            {
                return Err(
                    "capability manifest changed a semantic capability boundary".to_string()
                );
            }
            let mut constraints = HashSet::new();
            if capability
                .constraints
                .iter()
                .any(|constraint| !constraints.insert(*constraint))
            {
                return Err("capability manifest contains duplicate constraints".to_string());
            }
            let expected_constraints = capability.id.expected_constraints();
            if capability.constraints.len() != expected_constraints.len()
                || expected_constraints
                    .iter()
                    .any(|expected| !constraints.contains(expected))
            {
                return Err("capability manifest changed capability constraints".to_string());
            }
        }
        if CareerOpsCapabilityId::all()
            .iter()
            .any(|expected| !ids.contains(expected))
        {
            return Err("capability manifest omitted a required capability".to_string());
        }
        if self.diagnostics.iter().any(|diagnostic| {
            diagnostic.message.trim().is_empty()
                || diagnostic.message.len() > 500
                || diagnostic.action.trim().is_empty()
                || diagnostic.action.len() > 500
        }) {
            return Err("capability manifest contains an empty diagnostic".to_string());
        }
        Ok(())
    }
}

fn is_lowercase_git_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_safe_declared_version(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 100
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
    {
        return false;
    }
    let suffix_index = value.find(['-', '+']).unwrap_or(value.len());
    let core = &value[..suffix_index];
    let parts = core.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && (suffix_index == value.len() || suffix_index + 1 < value.len())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CareerOpsCheckResult {
    pub ready: bool,
    pub detail: String,
    pub capabilities: Option<CareerOpsCapabilityManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationHealth {
    pub career_ops: CareerOpsCheckResult,
    pub codex: CheckResult,
    pub claude: CheckResult,
}

#[cfg(test)]
mod capability_manifest_tests {
    use serde_json::{Value, json};

    use super::{CareerOpsCapabilityManifest, EvaluationAuthorization};

    fn manifest_value() -> Value {
        let revision = "a".repeat(40);
        let capability = |id: &str, status: &str, interface_class: &str, constraints: Value| {
            json!({
                "id": id,
                "status": status,
                "interfaceClass": interface_class,
                "sourceRevision": revision,
                "probeRevision": "hereforwork.career-ops-capability-probes.v1",
                "compatibilityFingerprint": null,
                "constraints": constraints,
            })
        };
        json!({
            "contract": "hereforwork.career-ops-capabilities",
            "schemaVersion": 1,
            "adapterProtocolVersion": 1,
            "upstreamRevision": revision,
            "upstreamDeclaredVersion": "1.31.0",
            "autoPdfScoreThreshold": { "value": 3.0, "source": "upstream_default" },
            "operations": [
                "capabilities.get",
                "health.check",
                "history.snapshot",
                "evaluation.result.read.v1",
                "profile.queue_filters.get",
                "preparation.context.get",
                "preparation.result.recover",
                "preparation.result.commit",
                "preparation.artifacts.delete",
                "answers.context.get",
                "answers.result.validate",
                "answers.result.commit",
                "role.discard",
                "role.discard.undo",
                "application.applied.confirm"
            ],
            "sourceOfTruth": {
                "profileFacts": "career-ops",
                "evaluation": "career-ops",
                "generatedArtifacts": "career-ops",
                "groundedAnswers": "career-ops",
                "applicationHistory": "career-ops",
                "operationalState": "here-for-work"
            },
            "forbiddenOperations": [
                "application.submit",
                "application.finalize",
                "message.send",
                "shell.run",
                "browser.command"
            ],
            "capabilities": [
                capability("discovery.reverse_ats.run.v1", "degraded", "conditional", json!([
                    "requires_exact_upstream_revision", "requires_isolated_execution",
                    "writes_no_career_ops_checkout_state"
                ])),
                capability("discovery.company_ats.preview.v1", "degraded", "contracted", json!([
                    "requires_exact_upstream_revision", "requires_safe_shape_probe", "preview_only",
                    "no_implicit_config_write"
                ])),
                capability("liveness.role.read.v1", "unavailable", "missing", json!([
                    "requires_exact_upstream_revision", "requires_typed_per_role_evidence"
                ])),
                capability("evaluation.full_ag.run.v1", "unavailable", "missing", json!([
                    "requires_exact_upstream_revision", "requires_atomic_evaluation_receipt",
                    "native_score_1_to_5"
                ])),
                capability("evaluation.result.read.v1", "degraded", "conditional", json!([
                    "requires_exact_upstream_revision", "requires_safe_shape_probe",
                    "native_score_1_to_5", "requires_report_tracker_identity"
                ])),
                capability("artifacts.inspect.v1", "unavailable", "missing", json!([
                    "requires_exact_upstream_revision", "requires_structured_artifact_provenance"
                ])),
                capability("browser.review_fallback.v1", "unavailable", "missing", json!([
                    "requires_exact_upstream_revision", "requires_single_driver_lease_transfer",
                    "review_only_no_submit"
                ])),
                capability("canonical.applied.write.v1", "degraded", "contracted", json!([
                    "requires_exact_upstream_revision", "canonical_tracking_only",
                    "requires_user_confirmed_submission"
                ])),
            ],
            "diagnostics": [{
                "code": "auto_pdf_threshold_product_mismatch",
                "capabilityId": "evaluation.full_ag.run.v1",
                "message": "Configured threshold differs from the product target.",
                "action": "Change career-ops configuration explicitly if desired."
            }]
        })
    }

    #[test]
    fn capability_manifest_deserializes_and_validates_the_fixed_semantic_set() {
        let manifest: CareerOpsCapabilityManifest =
            serde_json::from_value(manifest_value()).expect("manifest deserializes");

        manifest.validate().expect("manifest validates");
    }

    #[test]
    fn capability_manifest_rejects_unknown_fields() {
        let mut value = manifest_value();
        value
            .as_object_mut()
            .expect("manifest object")
            .insert("unexpected".to_string(), json!(true));

        assert!(serde_json::from_value::<CareerOpsCapabilityManifest>(value).is_err());
    }

    #[test]
    fn capability_manifest_cannot_enable_without_an_exact_revision() {
        let mut value = manifest_value();
        value["upstreamRevision"] = Value::Null;
        for capability in value["capabilities"]
            .as_array_mut()
            .expect("capabilities array")
        {
            capability["sourceRevision"] = Value::Null;
        }
        let manifest: CareerOpsCapabilityManifest =
            serde_json::from_value(value).expect("manifest shape remains valid");

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn capability_manifest_rejects_an_invalid_evaluation_result_fingerprint() {
        let mut value = manifest_value();
        value["capabilities"][4]["compatibilityFingerprint"] = json!("not-a-sha256");
        let manifest: CareerOpsCapabilityManifest =
            serde_json::from_value(value).expect("manifest deserializes");
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn capability_manifest_rejects_a_fingerprint_for_an_unavailable_capability() {
        let mut value = manifest_value();
        value["capabilities"][4]["status"] = json!("unavailable");
        value["capabilities"][4]["compatibilityFingerprint"] = json!("a".repeat(64));
        let manifest: CareerOpsCapabilityManifest =
            serde_json::from_value(value).expect("manifest deserializes");

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn evaluation_authorization_rejects_empty_evidence() {
        let authorization = json!({
            "confidence": "investigate",
            "evidence": [],
            "scope": "job-specific",
            "engagementMechanism": "unknown",
            "question": "Confirm the authorization path.",
            "legacyWorkAuth": "unstated"
        });

        assert!(serde_json::from_value::<EvaluationAuthorization>(authorization).is_err());
    }
}
