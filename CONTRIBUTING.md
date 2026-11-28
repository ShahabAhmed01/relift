# Contributing to Relift

Thanks for considering a contribution. Keep implementation, tests, support claims, and documentation aligned.

## Development setup

Install Rust 1.90 or newer. On Windows, use the MSVC toolchain. Run the Rust checks from the repository root:

    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace --all-targets

For GUI work, install Node.js 22.17 or newer and the Tauri prerequisites for your operating system. From crates/relift-gui:

    npm ci
    npm run check
    npm run build

A desktop package can be built with:

    npm run tauri build

## Workspace layout

- crates/relift-core: read-only sources, filesystem parsing, carving, recovery, imaging, sessions, reports, and safety checks.
- crates/relift-cli: command-line interface.
- crates/relift-testkit: deterministic synthetic filesystem fixtures.
- crates/relift-gui: SvelteKit frontend.
- crates/relift-gui/src-tauri: Tauri command and desktop backend.
- crates/relift-bench: Criterion benchmarks.

Sessions use JSON and JSONL files. A cancelled or crashed scan can be continued with `relift scan --resume`; the checkpoint is volume-level, so the interrupted volume is rescanned. Imaging resumes from durable prefix checkpoints with `relift image --resume`, but it does not retry unreadable sectors. Check docs/CLI.md and docs/GAP_REPORT.md before relying on a capability.

## Safety requirements

- Keep source access read-only.
- Never test by writing to the source device.
- Do not access a person's physical device without explicit confirmation of the source and destination.
- Use synthetic fixtures or disk-image files for tests.
- Run recovery with dry-run first and keep output on a different physical volume.
- Do not claim real-media accuracy from synthetic fixtures.

## Adding a carver

Carving registrations and validators are in crates/relift-core/src/carve.rs. Add a marker, bounded maximum length, validator dispatch, and a format-specific validator. A magic-byte match alone is insufficient: reject malformed structures and validate checksums or terminal markers where the format supports them. Add deterministic positive and negative fixtures and update docs/formats.md and docs/support-matrix.md only after the implementation passes.

## Pull requests

Describe the behavior change and evidence. Run relevant Rust and frontend checks. Update docs/GAP_REPORT.md when a limitation or release gate changes. Do not include physical-media evidence unless it was collected under an explicitly approved, controlled test plan.
