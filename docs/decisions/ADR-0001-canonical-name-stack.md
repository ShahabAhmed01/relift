# ADR-0001: Canonical name and stack

## Status

Accepted.

## Context

The user's original master prompt contained many conflicting names
(`Unlost`, `Vestige`, `Exhume`, `Remanence`, `Slackspace`, `Phoenix`,
`ApexRecover`, `Palimpsest`), stacks (Rust, Node, Python), and licenses. Building
multiple products or keeping several names would fragment effort and violate the
"one canonical name and stack" requirement.

## Decision

- **Name:** `Relift` — verified free on crates.io, npm, PyPI; no data-recovery or
  DFIR collision found. Rejected: `Unlost` (tryunlost.com), `Vestige` (DFIR
  tooling), `Exhume` (academic ExtFS recovery), `Remanence` (DFIR workbench),
  `Slackspace` (forensics firm), `Phoenix`, `ApexRecover`, `Palimpsest`.
- **License:** GPL-3.0-or-later — keeps the recovery engine and derivatives free.
  License text fetched to `LICENSE`.
- **Stack:** Rust 1.100.0 GNU toolchain via rustup; no MSVC/admin needed.
  Verified compile/link/run on Windows 11.
- **Workspace layout:** `crates/relift-core`, `crates/relift-cli` (binary
  `relift`), `crates/relift-testkit`.
- **Dependencies:** `serde`, `serde_json`, `sha2`, `memchr`, `crc32fast`,
  `flate2`, `thiserror`, `clap`, `anyhow`.

## Consequences

- One product, one name, one license. All conflicts are recorded here rather than
  building multiple products.
- Windows uses the GNU toolchain; MSVC is not required and not supported as the
  primary path.
- Node and Python are not part of the product.

## Related

- `docs/decisions/CONFLICT_REGISTER.md`
