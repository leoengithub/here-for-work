# HFW-EVAL-PATH-01 — adapter spawn PATH includes discovered Claude

Desktop **0.1.4** source fix. No live career-ops writes. No Desktop rebuild.

## Root cause

`evaluation.full_ag.run.v1` stayed `unavailable` in the GUI adapter subprocess because `fullAgCompatibilityProbe()` uses `command -v claude` on the inherited PATH only.

Tauri already resolved Claude via `discover_executable` (typically `~/.local/bin/claude`) for provider probes. Adapter spawn did not pass that directory, so a GUI-minimal PATH (`/usr/bin:/bin:/usr/sbin:/sbin`) made `claude` invisible. `evaluation.result.read.v1` still returned `degraded` + fingerprint. Sync then held roles as `canonical_evaluation_missing_executor_unavailable`.

## Fix

`AdapterConfig` now carries the discovered Claude path. Adapter spawn sets `PATH` to that executable’s parent directory plus the process PATH. No username or home path is hardcoded.

## Tests

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib adapter::
```

- `adapter_child_path_prepends_discovered_claude_directory` — PATH helper prepends the Claude directory and keeps existing entries.
- `adapter_spawn_env_includes_discovered_claude_directory` — spawned adapter child sees the discovered Claude directory on PATH.

No live A–G evaluations. Allowlist semantics unchanged.

## Manager verify (after Desktop ship)

1. Launch the rebuilt Desktop app (this change is source-only until Manager ships a rebuild).
2. Run `capabilities.get`.
3. Expect `evaluation.full_ag.run.v1` **degraded** with a 64-character fingerprint (not `unavailable`).
4. `evaluation.result.read.v1` should remain **degraded** + fingerprint.
5. Do **not** batch-eval held roles until that capability check passes. Probe one allowlisted role only if Manager authorizes it.

## Risks

- Claude must still be discoverable at launch (`discover_executable`). If it is missing, PATH is unchanged and the run capability stays unavailable.
- A Desktop rebuild is required before the GUI process picks this up.
- Existing held rows will not clear until evaluation sync sees a fingerprinted run capability.
