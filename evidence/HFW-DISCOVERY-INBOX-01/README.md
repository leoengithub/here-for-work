# HFW-DISCOVERY-INBOX-01

Sanitized Phase A evidence for observation-only discovery inbox auto-consume.

## Bundle

- Branch: `hfw-discovery-handoff-phase-a`
- Worktree: `/Users/leo/Work/here-for-work-worktrees/hfw-discovery-handoff`
- App: `src-tauri/target/release/bundle/macos/HereForWork.app` (built 2026-09-10)
- Do not confuse with `/Users/leo/Desktop/HereForWork.app` (older personal proof install)

## Fixed inbox observation

Path: `/Users/leo/Work/here-for-work/inbox/discovery-runs`

At evidence time:

- 16 sealed `.json` files
- 14 `eu-job-radar`
- 2 `frontend-role-scan`
- `*.partial` ignored by design

No personal job titles, URLs, or scores are recorded here.

## Automated validation

- Rust `discovery_inbox*` tests: 9 passed
- `src/App.test.tsx`: 34 passed
- `tsc -p tsconfig.app.json --noEmit`: clean

## Authority

Sources remain `staged`. Auto-consume is observation-only typed ingestion. No scheduled-task
pause, no Gmail mutation, no application submit during this work.
