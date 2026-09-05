# HFW-BROWSER-HANDSHAKE-01 — Retry browser step did not claim inspect

Date: 2026-09-05 (Europe/Madrid)

Scope: store/retry/handshake only. No application was submitted. No CV fact-check
or `verify-cv-facts` change. No second browser driver. Desktop was not rebuilt
(Manager ships).

## Live Buffer-like session (sanitized)

Desktop 0.1.6. Role status `Evaluated`. Preparation `completed`.

| Signal | Value |
| --- | --- |
| Session status | `action_required` |
| Session error | `extension_command_expired` |
| Driver lease | `released` |
| Fallback eligible | `1` |
| Page host | `buffer.com` (public HTTPS journey URL) |
| First inspect | `permanent` / `lease_expired` / attempt `3` |
| Retry inspect | `permanent` / `extension_handshake_timeout` / attempt `0` |

First inspect was claimed three times (leases 30 s) and never completed. Retry
inserted a new `inspect_request` and set `waiting_for_extension`, then launched
the selected Chrome profile. Within one handoff-worker tick (~350 ms) the session
was forced back to `action_required` because older permanent inspect failures
were still present. The new command stayed `pending` until the 15 s handshake
timeout, so the extension never received it (`attempt = 0`).

An earlier Applied-role session showed the same first-inspect lease exhaustion.
It was not retried in this trace and is out of scope.

Pairing was intact: approved extension ID matched the unpacked Profile 1 install;
native-host `allowed_origins` listed that ID; `browser_last_connected_at` was
current. The silent no-op was not an ID mismatch.

## Root cause

`retry_browser_session` correctly requeues inspect and holds the driver lease.
`claim_browser_command` and `recover_stalled_browser_commands` then treated
**any** historical `permanent` `lease_expired` / `extension_handshake_timeout`
inspect as proof the session had just stalled.

`recover_stalled_browser_commands` runs every 350 ms. After Retry it saw the old
permanent inspect, the session was not `action_required`, and it reverted the
retried session. The following poll could not claim: FIFO only offers sessions
in `waiting_for_extension` / `inspecting` / later driver states. The new inspect
expired as `extension_handshake_timeout`.

`failed_browser_command_can_be_retried` stayed green because that path uses
`failed`, not `permanent` + `lease_expired`.

## Fix

Do not mark a session `action_required` for historical inspect transport
failures while it still has a `pending` or `leased` command. Exhausting the
current command still fails the session. Retry after inspect-only lease or
handshake timeout stays claimable.

No submit path. No CV fact-check change. No second driver.

## Tests

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib retry_after_inspect_lease_exhaustion_stays_claimable
cargo test --manifest-path src-tauri/Cargo.toml --lib delayed_extension_handshake_expires_boundedly
cargo test --manifest-path src-tauri/Cargo.toml --lib expired_command_ack_releases
cargo test --manifest-path src-tauri/Cargo.toml --lib failed_browser_command_can_be_retried
```

RED: `retry_after_inspect_lease_exhaustion_stays_claimable` failed because
`recover_stalled_browser_commands` immediately re-listed the retried session.
GREEN after the pending/leased guard.

## How to Retry Buffer after ship

1. Install the Manager-shipped Desktop that includes this fix. Do not expect
   0.1.6 to recover.
2. Open the HereForWork Chrome profile and confirm Settings still shows the
   approved extension as connected.
3. On the Buffer Applications row, choose **Retry browser step**.
4. The session should stay `waiting_for_extension` / `inspecting` long enough
   for the extension to claim the new `inspect_request`. Chrome should open or
   focus the public Buffer journey URL. Review the form; do not submit.

If Settings shows pairing required or a different extension ID than the loaded
unpacked install, reconnect/re-approve in Settings before Retry. Do not treat
another handshake timeout as a reason to change CV fact checks.

## Remaining risks

- The first Buffer inspect was claimed three times and never returned a result.
  After this fix, Retry claims again. If the content script still hangs past the
  30 s lease on that journey URL, the user may see another bounded lease
  exhaustion — and Retry will be claimable again, not a no-op.
- This machine’s live driver is the unpacked repo `packages/extension/dist` and
  a cargo-bundle native-host path, not the files inside the shipped 0.1.6 app.
  That is operational drift, not the silent Retry bug. Reconnect/re-approve if
  a later bundle ID diverges.
- An Applied-role session with the same first-inspect lease pattern remains
  terminal for Prepare; do not retry it as a handshake fix.
- Handshake timeout remains 15 s. That bound is enough once the retried command
  stays claimable.

## Artifacts

- `live-trace.json` — sanitized command/session timeline (no profile facts,
  titles, or artifact paths).
