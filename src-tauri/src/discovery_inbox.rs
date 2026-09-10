//! Observation-only discovery-run inbox scan for the fixed producer handoff path.
//!
//! This module does not execute sources and does not change `execution_mode`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::domain::{
    DiscoveryInboxFileStatus, DiscoveryInboxInventory, DiscoveryInboxItem,
    DiscoveryInboxSourceCounts,
};

pub const DEFAULT_DISCOVERY_INBOX_DIR: &str =
    "/Users/leo/Work/here-for-work/inbox/discovery-runs";

const MAX_INBOX_FILES: usize = 500;
const MAX_INBOX_FILE_BYTES: usize = 2_000_000;
const SUPPORTED_SOURCE_IDS: [&str; 2] = ["frontend-role-scan", "eu-job-radar"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryRunEnvelopeIdentity {
    pub source_id: String,
    pub run_id: String,
    pub digest: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryInboxError {
    #[error("discovery inbox is unreadable: {0}")]
    Io(#[from] std::io::Error),
    #[error("discovery inbox entry is invalid: {0}")]
    Invalid(String),
}

pub fn parse_discovery_run_filename(file_name: &str) -> Option<(String, String)> {
    let stem = file_name.strip_suffix(".json")?;
    let rest = stem.strip_prefix("discovery-run--")?;
    let (source_id, run_id) = rest.split_once("--")?;
    if source_id.is_empty() || run_id.is_empty() {
        return None;
    }
    if !SUPPORTED_SOURCE_IDS.contains(&source_id) {
        return None;
    }
    Some((source_id.to_string(), run_id.to_string()))
}

pub fn list_sealed_discovery_run_paths(dir: &Path) -> Result<Vec<PathBuf>, DiscoveryInboxError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.ends_with(".partial") {
            continue;
        }
        if parse_discovery_run_filename(name).is_none() {
            continue;
        }
        paths.push(path);
        if paths.len() >= MAX_INBOX_FILES {
            break;
        }
    }
    paths.sort();
    Ok(paths)
}

pub fn read_discovery_run_envelope(
    path: &Path,
) -> Result<DiscoveryRunEnvelopeIdentity, DiscoveryInboxError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() as usize > MAX_INBOX_FILE_BYTES {
        return Err(DiscoveryInboxError::Invalid(
            "discovery run exceeds the maximum payload size".to_string(),
        ));
    }
    let payload = fs::read_to_string(path)?;
    let raw: Value = serde_json::from_str(&payload)
        .map_err(|error| DiscoveryInboxError::Invalid(error.to_string()))?;
    let contract = raw
        .get("contract")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if contract != "hereforwork.discovery-run" {
        return Err(DiscoveryInboxError::Invalid(
            "unsupported discovery-run contract".to_string(),
        ));
    }
    let source_id = raw
        .pointer("/source/sourceId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let run_id = raw
        .get("runId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let digest = raw
        .pointer("/integrity/digest")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if source_id.is_empty() || run_id.is_empty() || digest.is_empty() {
        return Err(DiscoveryInboxError::Invalid(
            "discovery run is missing sourceId, runId, or integrity.digest".to_string(),
        ));
    }
    if !SUPPORTED_SOURCE_IDS.contains(&source_id.as_str()) {
        return Err(DiscoveryInboxError::Invalid(format!(
            "unsupported discovery source {source_id}"
        )));
    }
    if let Some(file_name) = path.file_name().and_then(|value| value.to_str()) {
        if let Some((file_source, file_run)) = parse_discovery_run_filename(file_name) {
            if file_source != source_id || file_run != run_id {
                return Err(DiscoveryInboxError::Invalid(
                    "filename identity does not match payload sourceId/runId".to_string(),
                ));
            }
        }
    }
    Ok(DiscoveryRunEnvelopeIdentity {
        source_id,
        run_id,
        digest,
    })
}

pub fn classify_discovery_inbox_item(
    recorded_digest: Option<&str>,
    envelope_digest: &str,
    envelope_error: Option<&str>,
) -> (DiscoveryInboxFileStatus, Option<String>) {
    if let Some(error) = envelope_error {
        return (DiscoveryInboxFileStatus::Invalid, Some(error.to_string()));
    }
    match recorded_digest {
        None => (DiscoveryInboxFileStatus::PendingImport, None),
        Some(existing) if existing == envelope_digest => {
            (DiscoveryInboxFileStatus::Imported, None)
        }
        Some(_) => (
            DiscoveryInboxFileStatus::Invalid,
            Some("run identity was reused with a different digest".to_string()),
        ),
    }
}

pub fn inventory_discovery_inbox<F>(
    dir: &Path,
    mut recorded_digest: F,
) -> Result<DiscoveryInboxInventory, DiscoveryInboxError>
where
    F: FnMut(&str, &str) -> Result<Option<String>, DiscoveryInboxError>,
{
    let paths = list_sealed_discovery_run_paths(dir)?;
    let mut items = Vec::with_capacity(paths.len());
    let mut pending = 0usize;
    let mut imported = 0usize;
    let mut invalid = 0usize;
    let mut by_source = SUPPORTED_SOURCE_IDS
        .iter()
        .map(|source_id| DiscoveryInboxSourceCounts {
            source_id: (*source_id).to_string(),
            pending: 0,
            imported: 0,
            invalid: 0,
        })
        .collect::<Vec<_>>();

    for path in paths {
        let envelope = read_discovery_run_envelope(&path);
        let (status, last_error, source_id, run_id) = match envelope {
            Ok(identity) => {
                let recorded = recorded_digest(&identity.source_id, &identity.run_id)?;
                let (status, last_error) = classify_discovery_inbox_item(
                    recorded.as_deref(),
                    &identity.digest,
                    None,
                );
                (status, last_error, identity.source_id, identity.run_id)
            }
            Err(error) => {
                let file_name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default();
                let (source_id, run_id) = parse_discovery_run_filename(file_name)
                    .unwrap_or_else(|| ("unknown".to_string(), file_name.to_string()));
                (
                    DiscoveryInboxFileStatus::Invalid,
                    Some(error.to_string()),
                    source_id,
                    run_id,
                )
            }
        };
        match status {
            DiscoveryInboxFileStatus::PendingImport => pending += 1,
            DiscoveryInboxFileStatus::Imported => imported += 1,
            DiscoveryInboxFileStatus::Invalid => invalid += 1,
        }
        if let Some(counts) = by_source
            .iter_mut()
            .find(|counts| counts.source_id == source_id)
        {
            match status {
                DiscoveryInboxFileStatus::PendingImport => counts.pending += 1,
                DiscoveryInboxFileStatus::Imported => counts.imported += 1,
                DiscoveryInboxFileStatus::Invalid => counts.invalid += 1,
            }
        }
        items.push(DiscoveryInboxItem {
            path: path.display().to_string(),
            source_id,
            run_id,
            status,
            last_error,
        });
    }

    Ok(DiscoveryInboxInventory {
        directory: dir.display().to_string(),
        pending,
        imported,
        invalid,
        by_source,
        files: items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_filename_with_colon_bearing_run_id() {
        let parsed = parse_discovery_run_filename(
            "discovery-run--frontend-role-scan--frontend-role-scan:run:20260910T131316Z.json",
        );
        assert_eq!(
            parsed.as_ref().map(|(source, run)| (source.as_str(), run.as_str())),
            Some((
                "frontend-role-scan",
                "frontend-role-scan:run:20260910T131316Z"
            ))
        );
    }

    #[test]
    fn ignores_partial_and_unrelated_files() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join(
                "discovery-run--eu-job-radar--eu-job-radar:run:1.json.partial",
            ),
            "{}",
        )
        .unwrap();
        fs::write(directory.path().join("notes.txt"), "skip").unwrap();
        fs::write(
            directory
                .path()
                .join("discovery-run--eu-job-radar--eu-job-radar:run:1.json"),
            r#"{"contract":"hereforwork.discovery-run","runId":"eu-job-radar:run:1","source":{"sourceId":"eu-job-radar"},"integrity":{"digest":"abc"}}"#,
        )
        .unwrap();

        let paths = list_sealed_discovery_run_paths(directory.path()).unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("discovery-run--eu-job-radar--eu-job-radar:run:1.json"));
    }

    #[test]
    fn inventory_marks_unknown_store_identity_as_pending_import() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(
            "discovery-run--frontend-role-scan--frontend-role-scan:run:pending-1.json",
        );
        let mut file = fs::File::create(&path).unwrap();
        write!(
            file,
            r#"{{"contract":"hereforwork.discovery-run","runId":"frontend-role-scan:run:pending-1","source":{{"sourceId":"frontend-role-scan"}},"integrity":{{"digest":"abc123"}}}}"#
        )
        .unwrap();

        let inventory = inventory_discovery_inbox(directory.path(), |_, _| Ok(None)).unwrap();
        assert_eq!(inventory.pending, 1);
        assert_eq!(inventory.imported, 0);
        assert_eq!(inventory.invalid, 0);
        assert_eq!(inventory.files[0].status, DiscoveryInboxFileStatus::PendingImport);
        assert_eq!(
            inventory
                .by_source
                .iter()
                .find(|row| row.source_id == "frontend-role-scan")
                .map(|row| row.pending),
            Some(1)
        );
    }

    #[test]
    fn inventory_marks_matching_digest_as_imported() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("discovery-run--eu-job-radar--eu-job-radar:run:done.json");
        fs::write(
            &path,
            r#"{"contract":"hereforwork.discovery-run","runId":"eu-job-radar:run:done","source":{"sourceId":"eu-job-radar"},"integrity":{"digest":"digest-1"}}"#,
        )
        .unwrap();

        let inventory = inventory_discovery_inbox(directory.path(), |source, run| {
            assert_eq!(source, "eu-job-radar");
            assert_eq!(run, "eu-job-radar:run:done");
            Ok(Some("digest-1".to_string()))
        })
        .unwrap();
        assert_eq!(inventory.imported, 1);
        assert_eq!(inventory.files[0].status, DiscoveryInboxFileStatus::Imported);
    }

    #[test]
    fn inventory_marks_malformed_json_as_invalid() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory
                .path()
                .join("discovery-run--eu-job-radar--eu-job-radar:run:bad.json"),
            "{",
        )
        .unwrap();

        let inventory = inventory_discovery_inbox(directory.path(), |_, _| Ok(None)).unwrap();
        assert_eq!(inventory.invalid, 1);
        assert_eq!(inventory.files[0].status, DiscoveryInboxFileStatus::Invalid);
        assert!(inventory.files[0].last_error.as_ref().unwrap().contains("invalid"));
    }
}
