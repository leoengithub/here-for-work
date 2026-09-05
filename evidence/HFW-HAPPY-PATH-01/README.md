# HFW-HAPPY-PATH-01 — Continue anyway + native title open

Automated evidence only. No personal career data.

## What landed

- Role titles keep `href` and open public HTTPS listings through `open_external_url` / `/usr/bin/open` in Tauri.
- `cv_fact_check_failed` + `action_required` exposes **Continue anyway** beside **Prepare again**. Confirm completes the same preparation as `user_accepted_unverified` and queues the browser session.

## Commands that passed

```bash
node --test packages/career-ops-adapter/preparation-transaction.test.mjs
node --test packages/career-ops-adapter/adapter.test.mjs
cargo test --manifest-path src-tauri/Cargo.toml --lib -- public_https_url_requires_https_and_a_host unverified_provenance_requires_the_fact_check_warning continue_unverified_accepts_only_fact_check_action_required
corepack pnpm exec vitest --config vite.config.ts run src/App.test.tsx
```

Desktop was not rebuilt. Version stays 0.1.7.