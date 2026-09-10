//! Read-only discovery shadow window accounting.
//!
//! Compares expected schedule nominals against imported sealed discovery-runs.
//! Does not execute sources, mutate Gmail/canonical state, or advance executor cursors.

use chrono::{DateTime, Utc};

use crate::domain::DiscoveryShadowWindowStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowWindowClassification {
    pub status: DiscoveryShadowWindowStatus,
    pub linked_run_id: Option<String>,
    pub linked_digest: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedShadowRun {
    pub run_id: String,
    pub digest: String,
    pub status: String,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
}

pub fn classify_expected_window(
    expected_at: DateTime<Utc>,
    runs: &[ImportedShadowRun],
) -> ShadowWindowClassification {
    let covering: Vec<&ImportedShadowRun> = runs
        .iter()
        .filter(|run| run.window_start < expected_at && expected_at <= run.window_end)
        .collect();
    if covering.is_empty() {
        return ShadowWindowClassification {
            status: DiscoveryShadowWindowStatus::Missing,
            linked_run_id: None,
            linked_digest: None,
            detail: Some("no imported discovery-run covers this expected nominal".to_string()),
        };
    }
    let run = covering
        .iter()
        .max_by_key(|run| run.window_end)
        .copied()
        .expect("covering is non-empty");
    let status = match run.status.as_str() {
        "completed" => DiscoveryShadowWindowStatus::MatchedCompleted,
        "partial" => DiscoveryShadowWindowStatus::MatchedPartial,
        "failed" => DiscoveryShadowWindowStatus::MatchedFailed,
        other => {
            return ShadowWindowClassification {
                status: DiscoveryShadowWindowStatus::Unexpected,
                linked_run_id: Some(run.run_id.clone()),
                linked_digest: Some(run.digest.clone()),
                detail: Some(format!("unsupported imported run status {other}")),
            };
        }
    };
    ShadowWindowClassification {
        status,
        linked_run_id: Some(run.run_id.clone()),
        linked_digest: Some(run.digest.clone()),
        detail: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn run(
        run_id: &str,
        status: &str,
        start: &str,
        end: &str,
    ) -> ImportedShadowRun {
        ImportedShadowRun {
            run_id: run_id.to_string(),
            digest: format!("digest-{run_id}"),
            status: status.to_string(),
            window_start: DateTime::parse_from_rfc3339(start)
                .unwrap()
                .with_timezone(&Utc),
            window_end: DateTime::parse_from_rfc3339(end)
                .unwrap()
                .with_timezone(&Utc),
        }
    }

    #[test]
    fn missing_when_no_covering_run() {
        let expected = Utc.with_ymd_and_hms(2026, 9, 10, 8, 0, 0).unwrap();
        let classification = classify_expected_window(expected, &[]);
        assert_eq!(classification.status, DiscoveryShadowWindowStatus::Missing);
    }

    #[test]
    fn matches_completed_covering_run() {
        let expected = Utc.with_ymd_and_hms(2026, 9, 10, 8, 0, 0).unwrap();
        let runs = vec![run(
            "eu-job-radar:run:1",
            "completed",
            "2026-09-10T06:00:00Z",
            "2026-09-10T10:00:00Z",
        )];
        let classification = classify_expected_window(expected, &runs);
        assert_eq!(
            classification.status,
            DiscoveryShadowWindowStatus::MatchedCompleted
        );
        assert_eq!(
            classification.linked_run_id.as_deref(),
            Some("eu-job-radar:run:1")
        );
    }

    #[test]
    fn matches_partial_and_failed() {
        let expected = Utc.with_ymd_and_hms(2026, 9, 10, 8, 0, 0).unwrap();
        let partial = classify_expected_window(
            expected,
            &[run(
                "r1",
                "partial",
                "2026-09-10T06:00:00Z",
                "2026-09-10T10:00:00Z",
            )],
        );
        assert_eq!(partial.status, DiscoveryShadowWindowStatus::MatchedPartial);
        let failed = classify_expected_window(
            expected,
            &[run(
                "r2",
                "failed",
                "2026-09-10T06:00:00Z",
                "2026-09-10T10:00:00Z",
            )],
        );
        assert_eq!(failed.status, DiscoveryShadowWindowStatus::MatchedFailed);
    }
}
