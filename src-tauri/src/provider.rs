use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::CheckResult;

const PROBE_PROMPT: &str = "Return a JSON object with contractVersion 1 and message exactly provider-ready. Do not use tools, inspect files, or include any other fields.";

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Codex,
    Claude,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProbeResult {
    pub provider: String,
    pub contract_version: u32,
    pub message: String,
}

#[derive(Debug, Clone)]
struct ProviderInvocation {
    args: Vec<String>,
    stdin: Option<String>,
}

pub fn check_cli(path: Option<&PathBuf>, label: &str) -> CheckResult {
    let Some(path) = path else {
        return CheckResult {
            ready: false,
            detail: format!("{label} CLI was not detected."),
        };
    };
    match Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
    {
        Ok(output) if output.status.success() => CheckResult {
            ready: true,
            detail: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        },
        Ok(output) => CheckResult {
            ready: false,
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(error) => CheckResult {
            ready: false,
            detail: format!("{label} CLI could not run: {error}"),
        },
    }
}

pub fn probe(
    kind: ProviderKind,
    executable: &Path,
    schema_path: &Path,
    working_directory: &Path,
) -> Result<ProviderProbeResult, String> {
    let value = invoke_structured(
        kind,
        executable,
        schema_path,
        working_directory,
        PROBE_PROMPT,
        Duration::from_secs(90),
    )?;
    parse_probe_value(kind, value)
}

pub fn invoke_structured(
    kind: ProviderKind,
    executable: &Path,
    schema_path: &Path,
    working_directory: &Path,
    prompt: &str,
    timeout: Duration,
) -> Result<Value, String> {
    invoke_structured_cancellable(
        kind,
        executable,
        schema_path,
        working_directory,
        prompt,
        timeout,
        None,
    )
}

pub fn invoke_structured_cancellable(
    kind: ProviderKind,
    executable: &Path,
    schema_path: &Path,
    working_directory: &Path,
    prompt: &str,
    timeout: Duration,
    cancelled: Option<&AtomicBool>,
) -> Result<Value, String> {
    let schema = std::fs::read_to_string(schema_path).map_err(|error| error.to_string())?;
    let mut claude_schema: Value =
        serde_json::from_str(&schema).map_err(|error| error.to_string())?;
    if let Some(object) = claude_schema.as_object_mut() {
        object.remove("$schema");
    }
    let invocation = build_invocation(kind, schema_path, &claude_schema, prompt);
    let mut command = Command::new(executable);
    command.args(&invocation.args);
    command
        .current_dir(working_directory)
        .env("PATH", provider_search_path(executable))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let mut stdin_error = None;
    if let Some(mut stdin) = child.stdin.take() {
        if let Some(payload) = invocation.stdin {
            stdin_error = stdin.write_all(payload.as_bytes()).err();
        }
    }
    let stdout = child.stdout.take().expect("piped stdout is available");
    let stderr = child.stderr.take().expect("piped stderr is available");
    let stdout_reader = thread::spawn(move || read_bounded(stdout, 2_000_000));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, 64_000));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if cancelled.is_some_and(|value| value.load(Ordering::Relaxed)) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Provider invocation cancelled.".to_string());
        }
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Provider invocation timed out.".to_string());
        }
        thread::sleep(Duration::from_millis(40));
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| "Provider stdout reader failed.".to_string())?
        .map_err(|error| error.to_string())?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "Provider stderr reader failed.".to_string())?
        .map_err(|error| error.to_string())?;
    if stdout.len() > 2_000_000 {
        return Err("Provider output exceeded the 2 MB safety limit.".to_string());
    }
    if !status.success() {
        return Err(redact_provider_error(&String::from_utf8_lossy(&stderr)));
    }
    if let Some(error) = stdin_error {
        return Err(format!(
            "Provider CLI closed before reading the prompt: {}",
            redact_provider_error(&error.to_string())
        ));
    }
    parse_provider_output(kind, &String::from_utf8_lossy(&stdout))
}

pub fn bind_context_hash(mut result: Value, expected_hash: &str) -> Result<Value, String> {
    if expected_hash.len() != 64
        || !expected_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("Provider context binding received an invalid expected hash.".to_string());
    }
    let object = result
        .as_object_mut()
        .ok_or("Provider result must be an object before context binding.")?;
    if object.get("contractVersion").and_then(Value::as_u64) != Some(1) {
        return Err("Provider result has the wrong contract version.".to_string());
    }
    let returned_hash = object
        .get("contextHash")
        .and_then(Value::as_str)
        .ok_or("Provider result omitted contextHash.")?;
    if returned_hash.len() != 64
        || !returned_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("Provider result returned an invalid contextHash.".to_string());
    }
    object.insert(
        "contextHash".to_string(),
        Value::String(expected_hash.to_string()),
    );
    Ok(result)
}

fn provider_search_path(executable: &Path) -> OsString {
    let mut paths = Vec::new();
    if let Some(parent) = executable.parent() {
        paths.push(parent.to_path_buf());
    }
    for path in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ] {
        let path = PathBuf::from(path);
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    std::env::join_paths(paths).unwrap_or_else(|_| OsString::from("/usr/bin:/bin"))
}

fn build_invocation(
    kind: ProviderKind,
    schema_path: &Path,
    claude_schema: &Value,
    prompt: &str,
) -> ProviderInvocation {
    match kind {
        ProviderKind::Codex => ProviderInvocation {
            args: vec![
                "exec".to_string(),
                "--ephemeral".to_string(),
                "--sandbox".to_string(),
                "read-only".to_string(),
                "--skip-git-repo-check".to_string(),
                "--ignore-rules".to_string(),
                "--ignore-user-config".to_string(),
                "--output-schema".to_string(),
                schema_path.display().to_string(),
                "-".to_string(),
            ],
            stdin: Some(prompt.to_string()),
        },
        ProviderKind::Claude => ProviderInvocation {
            args: vec![
                "-p".to_string(),
                "--no-session-persistence".to_string(),
                "--safe-mode".to_string(),
                "--disable-slash-commands".to_string(),
                "--permission-mode".to_string(),
                "plan".to_string(),
                "--tools".to_string(),
                String::new(),
                "--json-schema".to_string(),
                claude_schema.to_string(),
                "--output-format".to_string(),
                "json".to_string(),
            ],
            stdin: Some(prompt.to_string()),
        },
    }
}

#[cfg(test)]
fn parse_probe_output(kind: ProviderKind, stdout: &str) -> Result<ProviderProbeResult, String> {
    parse_probe_value(kind, parse_provider_output(kind, stdout)?)
}

fn parse_provider_output(kind: ProviderKind, stdout: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(stdout.trim()).map_err(|error| error.to_string())?;
    let result_value = match kind {
        ProviderKind::Codex => value,
        ProviderKind::Claude => value
            .get("structured_output")
            .or_else(|| value.get("result"))
            .cloned()
            .unwrap_or(value),
    };
    let parsed = if let Some(text) = result_value.as_str() {
        serde_json::from_str(text).map_err(|error| error.to_string())?
    } else {
        result_value
    };
    Ok(parsed)
}

fn parse_probe_value(
    kind: ProviderKind,
    result_value: Value,
) -> Result<ProviderProbeResult, String> {
    let contract_version = result_value
        .get("contractVersion")
        .and_then(Value::as_u64)
        .ok_or("Provider result omitted contractVersion")? as u32;
    let message = result_value
        .get("message")
        .and_then(Value::as_str)
        .ok_or("Provider result omitted message")?
        .to_string();
    if contract_version != 1 || message != "provider-ready" {
        return Err("Provider result did not satisfy the conformance contract.".to_string());
    }
    Ok(ProviderProbeResult {
        provider: match kind {
            ProviderKind::Codex => "codex",
            ProviderKind::Claude => "claude",
        }
        .to_string(),
        contract_version,
        message,
    })
}

fn redact_provider_error(stderr: &str) -> String {
    let compact = stderr
        .replace(['\r', '\n', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if compact.is_empty() {
        return "Provider CLI exited without a diagnostic.".to_string();
    }
    compact.chars().take(2_000).collect()
}

fn read_bounded(mut reader: impl Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut kept = Vec::new();
    let mut chunk = [0_u8; 8_192];
    loop {
        let count = reader.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_add(1).saturating_sub(kept.len());
        if remaining > 0 {
            kept.extend_from_slice(&chunk[..count.min(remaining)]);
        }
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::os::unix::fs::PermissionsExt;

    use serde_json::json;

    use super::{
        ProviderKind, bind_context_hash, build_invocation, parse_probe_output,
        parse_provider_output, provider_search_path,
    };

    #[test]
    fn provider_kind_rejects_unknown_values() {
        assert!(serde_json::from_str::<ProviderKind>("\"codex\"").is_ok());
        assert!(serde_json::from_str::<ProviderKind>("\"claude\"").is_ok());
        assert!(serde_json::from_str::<ProviderKind>("\"shell\"").is_err());
    }

    #[test]
    fn codex_probe_is_ephemeral_read_only_and_ignores_workspace_rules() {
        let invocation = build_invocation(
            ProviderKind::Codex,
            std::path::Path::new("/tmp/schema.json"),
            &json!({}),
            super::PROBE_PROMPT,
        );

        assert!(
            invocation
                .args
                .windows(2)
                .any(|pair| pair[0] == "--sandbox" && pair[1] == "read-only")
        );
        assert!(invocation.args.contains(&"--ephemeral".to_string()));
        assert!(invocation.args.contains(&"--ignore-rules".to_string()));
        assert!(
            invocation
                .args
                .contains(&"--ignore-user-config".to_string())
        );
        assert_eq!(invocation.stdin.as_deref(), Some(super::PROBE_PROMPT));
    }

    #[test]
    fn claude_probe_disables_tools_and_sessions() {
        let invocation = build_invocation(
            ProviderKind::Claude,
            std::path::Path::new("/tmp/schema.json"),
            &json!({ "type": "object" }),
            super::PROBE_PROMPT,
        );

        assert!(invocation.args.contains(&"--safe-mode".to_string()));
        assert!(
            invocation
                .args
                .contains(&"--no-session-persistence".to_string())
        );
        assert!(
            invocation
                .args
                .windows(2)
                .any(|pair| pair[0] == "--tools" && pair[1].is_empty())
        );
        assert_eq!(invocation.stdin.as_deref(), Some(super::PROBE_PROMPT));
        assert_eq!(invocation.args.last().map(String::as_str), Some("json"));
    }

    #[test]
    fn both_provider_output_shapes_use_the_same_contract() {
        let codex = parse_probe_output(
            ProviderKind::Codex,
            r#"{"contractVersion":1,"message":"provider-ready"}"#,
        )
        .unwrap();
        let claude = parse_probe_output(
            ProviderKind::Claude,
            r#"{"structured_output":{"contractVersion":1,"message":"provider-ready"}}"#,
        )
        .unwrap();

        assert_eq!(codex.contract_version, 1);
        assert_eq!(claude.contract_version, 1);
        assert!(
            parse_probe_output(
                ProviderKind::Codex,
                r#"{"contractVersion":1,"message":"unexpected"}"#,
            )
            .is_err()
        );
    }

    #[test]
    fn structured_results_share_one_parser() {
        let codex = parse_provider_output(ProviderKind::Codex, r#"{"value":"ok"}"#).unwrap();
        let claude = parse_provider_output(
            ProviderKind::Claude,
            r#"{"structured_output":{"value":"ok"}}"#,
        )
        .unwrap();
        assert_eq!(codex, claude);
    }

    #[test]
    fn application_binds_schema_valid_provider_hash_to_the_current_context() {
        let expected = "a".repeat(64);
        let generated = json!({
            "contractVersion": 1,
            "contextHash": "b".repeat(64),
            "value": "kept",
        });

        let bound = bind_context_hash(generated, &expected).unwrap();

        assert_eq!(
            bound.get("contextHash").and_then(|value| value.as_str()),
            Some(expected.as_str())
        );
        assert_eq!(
            bound.get("value").and_then(|value| value.as_str()),
            Some("kept")
        );
    }

    #[test]
    fn context_binding_does_not_hide_invalid_provider_contracts() {
        let expected = "a".repeat(64);
        assert!(
            bind_context_hash(
                json!({ "contractVersion": 2, "contextHash": "b".repeat(64) }),
                &expected,
            )
            .is_err()
        );
        assert!(
            bind_context_hash(
                json!({ "contractVersion": 1, "contextHash": "not-a-hash" }),
                &expected,
            )
            .is_err()
        );
    }

    #[test]
    fn structured_invocation_honors_cancellation_before_parsing_output() {
        use std::sync::atomic::AtomicBool;

        let directory = tempfile::tempdir().unwrap();
        let schema = directory.path().join("schema.json");
        std::fs::write(&schema, r#"{"type":"object"}"#).unwrap();
        let cancelled = AtomicBool::new(true);

        let error = super::invoke_structured_cancellable(
            super::ProviderKind::Codex,
            std::path::Path::new("/usr/bin/true"),
            &schema,
            directory.path(),
            "return json",
            std::time::Duration::from_secs(1),
            Some(&cancelled),
        )
        .unwrap_err();

        assert_eq!(error, "Provider invocation cancelled.");
    }

    #[test]
    fn provider_path_starts_with_the_launcher_directory_for_gui_app_launches() {
        let path = provider_search_path(std::path::Path::new(
            "/Users/example/.nvm/versions/node/v22/bin/codex",
        ));
        let entries = std::env::split_paths(&path).collect::<Vec<_>>();

        assert_eq!(
            entries.first().map(std::path::PathBuf::as_path),
            Some(std::path::Path::new(
                "/Users/example/.nvm/versions/node/v22/bin"
            ))
        );
        assert!(entries.contains(&std::path::PathBuf::from("/usr/bin")));
    }

    #[test]
    fn provider_startup_failure_preserves_stderr_when_prompt_pipe_closes() {
        let directory = tempfile::tempdir().unwrap();
        let schema = directory.path().join("schema.json");
        let executable = directory.path().join("provider");
        std::fs::write(&schema, r#"{"type":"object"}"#).unwrap();
        std::fs::write(
            &executable,
            "#!/bin/sh\necho provider-startup-diagnostic >&2\nexit 1\n",
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&executable, permissions).unwrap();

        let error = super::invoke_structured(
            ProviderKind::Codex,
            &executable,
            &schema,
            directory.path(),
            &"prompt".repeat(100_000),
            std::time::Duration::from_secs(2),
        )
        .unwrap_err();

        assert_eq!(error, "provider-startup-diagnostic");
    }

    fn assert_codex_strict_schema(node: &serde_json::Value, path: &str) {
        match node {
            serde_json::Value::Object(object) => {
                assert!(
                    !object.contains_key("oneOf"),
                    "Codex structured outputs reject oneOf at {path}"
                );
                if object.contains_key("const") || object.contains_key("enum") {
                    assert!(
                        object.contains_key("type"),
                        "Codex requires const and enum nodes to declare type at {path}"
                    );
                }
                if let Some(properties) =
                    object.get("properties").and_then(|value| value.as_object())
                {
                    assert_eq!(
                        object.get("additionalProperties"),
                        Some(&serde_json::Value::Bool(false)),
                        "Codex requires closed objects at {path}"
                    );
                    let property_names = properties.keys().cloned().collect::<BTreeSet<_>>();
                    let required_names = object
                        .get("required")
                        .and_then(|value| value.as_array())
                        .expect("Codex requires every object property to be required")
                        .iter()
                        .map(|value| {
                            value
                                .as_str()
                                .expect("required entries are strings")
                                .to_string()
                        })
                        .collect::<BTreeSet<_>>();
                    assert_eq!(
                        required_names, property_names,
                        "Codex requires every property to appear in required at {path}"
                    );
                }
                for (key, value) in object {
                    assert_codex_strict_schema(value, &format!("{path}/{key}"));
                }
            }
            serde_json::Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    assert_codex_strict_schema(value, &format!("{path}/{index}"));
                }
            }
            _ => {}
        }
    }

    #[test]
    fn provider_contracts_stay_in_codex_strict_json_schema_subset() {
        for (name, raw) in [
            (
                "preparation-result",
                include_str!("../../contracts/preparation-result.schema.json"),
            ),
            (
                "answer-draft",
                include_str!("../../contracts/answer-draft.schema.json"),
            ),
        ] {
            let schema: serde_json::Value = serde_json::from_str(raw).unwrap();
            assert_codex_strict_schema(&schema, name);
        }
    }
}
