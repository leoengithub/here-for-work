# HereForWork stack-selection spike

Status: selected for implementation planning
Date: 2026-08-30
Decision scope: personal macOS proof first; trusted-user distribution later

This decision implements the approved product and safety contracts in `PRODUCT.md` and `MVP_SHAPE.md`. It does not authorize application scaffolding, dependency installation, changes to career-ops, credential setup, browser-profile changes, or scheduled-task cutover.

## Decision

Use this application stack:

- **Desktop shell and native core:** Tauri 2 with Rust.
- **Interface:** React, TypeScript, and Vite, rendered in the macOS system webview.
- **JavaScript package management:** Corepack-managed pnpm with a repository-local `packageManager` declaration and the machine-global pnpm store.
- **Operational persistence:** SQLite owned from Rust through `rusqlite`, behind semantic repository APIs and a dedicated database actor/thread.
- **Schema and process contracts:** versioned JSON Schema Draft 2020-12 documents; generated TypeScript types; Rust validation and `serde` DTOs.
- **Background lifecycle:** one single-instance Tauri core process that can run without an open webview window. The UI is disposable; closing it does not terminate enabled background work.
- **External execution:** Rust starts argument arrays directly without a shell. career-ops, Codex, and Claude remain separately installed dependencies with explicitly resolved executable paths.
- **Browser assistance:** a separate TypeScript Manifest V3 Chrome extension plus a small Rust native-messaging-host binary built in the same Cargo workspace.
- **Initial platform target:** Apple Silicon macOS 13 or newer. Universal packaging is deferred until trusted-user distribution requires it.

No final visual component library is selected. Detailed visual design remains deferred, and the first interface should use a small semantic token layer rather than committing the product to a large component dependency.

## Runtime shape

```text
HereForWork.app
├── Rust/Tauri core process
│   ├── scheduler and wake reconciliation
│   ├── SQLite database actor
│   ├── run queue and state machines
│   ├── career-ops adapter client
│   ├── Codex/Claude process runner
│   ├── notification and deep-link handling
│   └── authenticated Unix-domain-socket endpoint
├── disposable WKWebView window
│   └── React/TypeScript interface
└── bundled native-messaging host executable
    └── Chrome service worker ↔ content scripts
```

There is no application HTTP server, browser-accessible localhost port, menu-bar item, or embedded browser automation runtime.

## Why Tauri 2

The approved architecture has a privileged local core and an untrusted-data-heavy interface. Tauri's capability model constrains which commands a window can call, while native work remains in Rust. Its IPC is serialized message passing rather than exposing Node or filesystem APIs directly to the renderer. The system webview also avoids bundling a second Chromium runtime. See [Tauri security](https://v2.tauri.app/security/), [capabilities](https://v2.tauri.app/reference/acl/capability/), and [IPC](https://v2.tauri.app/concept/inter-process-communication/).

Tauri can bundle architecture-specific sidecars and additional executables, which fits the native-messaging host. See [embedding external binaries](https://v2.tauri.app/develop/sidecar/).

The official plugin set covers notifications, autostart, deep links, single-instance behavior, logging, and process integration. These are candidates, not blanket permissions: each plugin command must be individually scoped. See [Tauri plugins](https://v2.tauri.app/plugin/), [notifications](https://v2.tauri.app/plugin/notification/), and [autostart](https://v2.tauri.app/plugin/autostart/).

## Compared options

| Criterion | Tauri 2 + Rust | Electron + TypeScript | Swift/AppKit + React webview |
|---|---|---|---|
| React renderer | Strong | Strong | Possible but custom |
| Native process and SQLite ownership | Strong | Strong, but Node/native-module packaging | Strong |
| Privileged-renderer separation | Capability-scoped by design | Requires careful preload/context bridge | Entirely custom bridge |
| Background core without a window | Supported | Supported | Supported |
| Native-messaging executable | Small Rust binary in same workspace | Requires another native or heavyweight Electron entrypoint | Small Swift binary |
| App footprint | Small; uses system webview | Bundles Chromium and Node | Small |
| Existing toolchain on this Mac | Rust must be installed | Node is installed | Swift is installed |
| macOS platform integration | Good; verify lifecycle plugins in package | Good; current login-item APIs exposed | Best |
| Bespoke platform work | Moderate | Low initially, moderate at native host | Highest |
| Long-term fit | **Best balance** | Best fastest prototype, weaker product fit | Best only if fully native UI becomes a goal |

### Rejected: local web application

A localhost UI would reintroduce port ownership, CSRF/origin protection, browser-tab lifecycle, and awkward notifications. It would not remove the need for a background native process or extension host.

### Rejected: Electron as the default

Electron provides useful process and login-item APIs, but it bundles Chromium for a product that already hands work to ordinary Chrome. Its native-messaging bridge still needs a separately launchable executable. Electron remains the fallback if the first packaged Tauri lifecycle spike cannot reliably satisfy login, notification, or webview accessibility requirements. See [Electron process model](https://www.electronjs.org/docs/latest/tutorial/process-model) and [login-item settings](https://www.electronjs.org/docs/latest/api/app).

### Rejected: custom Swift shell as the default

Swift has the strongest direct access to macOS APIs, and the installed toolchain is ready. However, a custom Swift-to-React bridge, application state layer, build pipeline, and test harness would be product-specific infrastructure before validating the core workflow. A small native Swift module remains an escape hatch for a macOS API that Tauri cannot expose reliably.

## Deliberate Tauri boundaries

### Renderer

The React renderer may:

- Query view models through semantic commands.
- Request user-owned actions such as Prepare, Dismiss, Retry, or Open Browser.
- Subscribe to typed state-change events.

It may not receive raw filesystem, shell, SQLite, browser-control, provider-token, or arbitrary process APIs. Remote content is never loaded in the app webview.

### Rust core

Rust owns:

- Operational state machines and persistence.
- Scheduling, leases, retries, idempotency, and recovery.
- Validation of every renderer, adapter, extension, and provider message.
- Explicit executable-path resolution and constrained process launch.
- Notification delivery and deep-link routing.
- The Unix socket used by the Chrome native host.

These are end-state component responsibilities, not evidence that scheduling authority has
already moved. Per-source executor authority remains with the existing scheduled tasks
until the stages, gates, and explicit cutover in
[SCHEDULING_MIGRATION.md](SCHEDULING_MIGRATION.md) are complete.

### SQLite

Use `rusqlite`, not the Tauri SQL plugin. SQL must not be exposed to the renderer. One database actor serializes writes; WAL mode may support non-blocking read projections after its packaged-filesystem behavior is verified. Migrations are embedded, transactional, backed up first, and integrity-checked.

### Contracts

Checked-in JSON Schemas are the source of truth for cross-process protocols:

- career-ops adapter messages;
- provider execution requests and results;
- native-host and extension messages;
- persisted export format.

The in-process React-to-Rust surface stays smaller and semantic. Generated TypeScript clients are checked for drift in CI.

## Background lifecycle decision

For the personal proof, the Tauri core itself is the user-level worker:

- Login launch uses a background argument and creates no initial window.
- Opening the app focuses or creates the window in the existing core process.
- Closing the window destroys or hides the webview but keeps the core alive when background work is enabled.
- Explicit Quit terminates the core after safely checkpointing work.
- No menu-bar presence is created.

The first packaged spike must verify Tauri's LaunchAgent-based autostart against the desired macOS behavior. If it cannot provide reliable approval/status/recovery semantics, replace only that boundary with a small `SMAppService` bridge rather than changing the application stack.

## Chrome profile decision

HereForWork does not own the user's Chrome profile name.

Onboarding offers:

1. Select an existing Chrome profile.
2. Create a new profile in ordinary Chrome, then connect it.
3. Accept a suggested name such as “HereForWork,” which the user may change.

Persist a stable profile identifier/path only after the user confirms it; the display name is mutable. Validate the chosen profile by a native-messaging handshake with both Chrome's extension ID and a profile-local installation UUID, not by its visible name. The user may select any existing profile or create and name one in ordinary Chrome before pairing it.

Production Chrome is launched normally, without WebDriver, CDP, remote debugging, or automation flags. A separate test profile is mandatory for Playwright or other automated browser tests. If an application page reports automation markers or rejects assisted interaction, stop and require manual completion; never disguise or bypass detection.

## Provider and executable resolution

Packaged macOS apps do not inherit interactive shell startup files. The current machine's Node installation is under an nvm path, and Rust is not installed. Therefore:

- Onboarding resolves and displays the exact career-ops root, Node, Codex, and Claude executable paths.
- User-selected paths override discovery.
- Resolved executables are capability-probed and versioned before use.
- No operation executes through `zsh -c`, `bash -c`, or another shell.
- Missing or moved executables create an `action_required` state.

Tauri also documents the GUI `$PATH` difference for macOS bundles. See [macOS application bundles](https://v2.tauri.app/distribute/macos-application-bundle/).

## Packaging decision

- Personal proof: Apple Silicon `.app`, ad-hoc or local development signing, manually installed.
- First lifecycle gate: packaged app, not only `tauri dev`.
- Trusted-user distribution: Developer ID signing, notarization, DMG, stable native-host registration, upgrade and uninstall tests.
- No updater in the personal proof.
- Codex, Claude, Chrome, Node, and career-ops are detected dependencies and are not repackaged.

Tauri supports ad-hoc signing for local Apple Silicon builds and requires normal signing/notarization for external distribution. See [macOS code signing](https://v2.tauri.app/distribute/sign/macos/) and [distribution](https://v2.tauri.app/distribute/).

## Toolchain findings and setup gates

Observed on 2026-08-30:

- Apple Silicon, macOS 26.6.2.
- Node 22.18.0 installed through nvm.
- Swift 6.3.2 installed.
- Rust and Cargo absent.
- Approximately 12 GiB free on the worktree volume, above the repository's 8 GiB minimum but close enough to recheck before every installation or build.
- `pnpm` exists, but the empty repository inherits `/Users/leo/package.json` and is currently interpreted as a Yarn project. The future repository-local `package.json` must declare the approved pnpm version before any package-manager command.

Installing Rust, initializing package metadata, selecting exact dependency versions, or scaffolding requires a separate implementation action and a fresh disk/store check.

## Required validation spikes

These are tests of the selected stack, not alternative products:

1. **Packaged lifecycle:** launch hidden at login, open/focus window, close window while work continues, explicit quit, disable background operation.
2. **Notification:** request authorization in context, deliver while window is closed, click through to a specific local route.
3. **GUI executable resolution:** run read-only health checks for the configured Node, Codex, and Claude paths from the packaged app.
4. **SQLite recovery:** migrate a copy, interrupt a write, recover a lease, restore a pre-migration backup.
5. **Native messaging:** bundled host authenticates the exact extension and connects to the core socket without localhost or browser automation.
6. **Accessibility:** VoiceOver and complete keyboard operation in a packaged WKWebView.
7. **Chrome profile:** user-selected profile reconnects by extension handshake and never enters an automation-controlled session.

Failure of one plugin leads to a narrow native bridge. Failure of Tauri's core process, packaged webview accessibility, or bundled-host model across several spikes would reopen the Electron comparison.

## Obsidian-vault decision for this iteration

Do not duplicate this artifact into the private vault. It is stable, implementation-relevant architecture and therefore belongs in the repository before code depends on it. A future vault note is appropriate for private credential onboarding, personal rollout observations, or decisions that should not be committed; any stable conclusion from such a note must still be distilled back into the repository.
