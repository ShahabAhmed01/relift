# ADR-0005: Desktop GUI selection

- **Status:** Superseded
- **Original decision date:** 2026-11-14
- **Superseded:** 2026-11-15

## Context

The initial architecture deferred GUI and TUI work until the shared core and
CLI were established. The master prompt proposed several incompatible desktop
frameworks and frontend stacks.

## Current decision

The repository now contains a Tauri 2 desktop application with a Svelte 5 and
TypeScript frontend in `crates/relift-gui`. It uses the shared `relift-core`
library through typed Tauri commands. CI builds the workspace on Windows,
macOS, and Linux; the release workflow defines platform GUI bundles.

The CLI remains the full scriptable interface. The GUI does not currently
expose every CLI capability: imaging and report export are CLI-only, and
in-progress pause/resume is not implemented. No TUI is selected or shipped.

## Rationale

Tauri and Svelte are already integrated and avoid introducing a second recovery
engine. Keeping the TUI deferred avoids duplicating workflows without a
demonstrated user need.

## Consequences

- Keep recovery logic in `relift-core`; frontend code must call shared APIs.
- Describe GUI-only and CLI-only capabilities accurately in the support matrix.
- Do not expose pause/resume controls until the backend can pause and resume a
  scan safely.
- Validate GUI behavior and performance separately from Rust workspace CI.
