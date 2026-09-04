# HFW-ADAPTER-READY-01 — adapter_ready false after stack merge

## Root cause

Packaged adapter emits `evaluation_executor_runtime_unavailable` (and can emit
`hfw_composed_evaluation_receipt`). Rust `CareerOpsCapabilityDiagnosticCode` omitted
those variants. With `deny_unknown_fields` on the diagnostic object and a closed enum,
`capabilities.get` failed to deserialize → `AdapterConfig::health()` errored → setup
set `adapter_ready=false` → Queue Prepare/Dismiss disabled and evaluation sync
mass-held roles as `evaluation_result_capability_unavailable`.

## Fixes

1. Register T1 diagnostic codes in Rust enum, JSON schema, and generated TS.
   Soft-fail unknown future codes via `#[serde(other)] Unknown`.
2. Schema v24 rehydrates roles that still have a durable `current_receipt_key` from
   the false capability hold back to `needs_decision` / `ready` / `hidden` using
   receipt fields only (no invented scores).
3. Schema v25 restores roles whose receipts used the composed executor fingerprint
   and were cleared by read-probe-only invalidation; sync now accepts both fingerprints.
4. Supersede obsolete `source_adapter_not_configured` catch-up runs and stop counting
   them as actionable Queue blockers.
5. Reset stuck preparation on rehydrated Queue-eligible roles (covers BCNC).
6. Bump Desktop version `0.1.0` → `0.1.1` (`package.json`, `tauri.conf.json`, Cargo).

## Live before → after

| Signal | Before | After |
| --- | --- | --- |
| `adapter_ready` | `false` @ 2026-09-04T12:27:12Z | `true` @ 2026-09-04T13:15:33Z |
| schema | 23 | 25 |
| Queue-eligible (`ready`/`needs_decision`) | 0 | 4 (coches + Wizeline×2 + BCNC) |
| Nunegal (Skip receipt) | capability hold | `hidden` |
| BCNC prep | `failed` / `action_required` | `not_started` / cancelled |
| staged catch-up blockers | 19 `action_required` | 19 `cancelled` superseded |
| Desktop version | 0.1.0 | 0.1.1 |

## Evidence files

- `before-live-db.txt` / `before-captured-at.txt`
- `after-live-db.txt` / `after-captured-at.txt`
- `codesign-verify.txt`
- `desktop-version.txt`

## Validation

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib schema_v24_rehydrates
cargo test --manifest-path src-tauri/Cargo.toml --lib schema_v25_restores
cargo test --manifest-path src-tauri/Cargo.toml --lib capability_manifest_deserializes_t1
cargo test --manifest-path src-tauri/Cargo.toml --lib capability_manifest_soft_fails
corepack pnpm build:extension && corepack pnpm tauri build
codesign --verify --deep --strict /Users/leo/Desktop/HereForWork.app
```
