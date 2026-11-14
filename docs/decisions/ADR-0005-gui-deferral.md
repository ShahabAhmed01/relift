# ADR-0005: Desktop GUI/TUI decision deferred

## Status

Accepted (documented fallback candidate).

## Context

The product targets CLI first-class and scriptable, with GUI/TUI "where
justified." Tauri 2 was considered but requires a WebView2/MSVC-verified path
that is not available in this environment (Windows 11, non-admin, GNU Rust only,
no MSVC).

## Decision

Defer the desktop GUI framework decision. The documented fallback candidate is a
Rust-native GUI. Record this in an ADR before implementing any GUI/TUI. CLI
remains the primary interface; GUI/TUI is optional and shares the same engine.

## Consequences

- No GUI/TUI code is written yet.
- Any future GUI must run on the GNU toolchain without MSVC/admin.
- Tauri 2 remains a candidate only after a WebView2/MSVC path is verified.

## Related

- `docs/decisions/CONFLICT_REGISTER.md`
