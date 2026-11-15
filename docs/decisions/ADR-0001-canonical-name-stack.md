# ADR-0001: Canonical name and stack

## Status

Accepted; maintained in [`CONFLICT_REGISTER.md`](CONFLICT_REGISTER.md).

## Context

The source prompt contains several proposed product names, languages, GUI
frameworks, and licenses. This repository already has a public identity and
working architecture; replacing it with a second product or incompatible stack
would discard existing work.

## Decision

- Keep the product name **Relift** and binary name `relift`.
- Keep the existing **GPL-3.0-or-later** license.
- Use the shared Rust workspace, with **Rust 1.90** as the minimum declared
  version.
- Keep `relift-core` as the shared engine and `relift-cli` as the full
  scriptable interface.
- Keep the implemented **Tauri 2 + Svelte 5** desktop application. Node is a
  build-time dependency for the frontend; Python is not part of the product.
- Build the Windows release target for MSVC. CI and the release workflow are
  authoritative for platform build coverage.
- Defer a TUI until a concrete user need justifies another interface.

## Consequences

- Alternatives are recorded with their disposition in the conflict register;
  they are not parallel product requirements.
- The support matrix distinguishes fixture-validated behavior from experimental
  parsers and unimplemented work.
- Product identity or license changes require an explicit decision record and
  compatibility review.
