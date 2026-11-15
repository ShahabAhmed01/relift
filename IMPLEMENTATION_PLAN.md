# Remaining implementation plan

This replaces the original planning draft, which described features that were not present in the repository. It is an open-work checklist, not a claim that these items are complete. See docs/GAP_REPORT.md for the canonical gap list.

## Evidence before broad release

1. Create and review a representative labeled recovery corpus.
2. Record per-filesystem and per-format precision and recall.
3. Test source immutability and destination-device refusal on controlled real hardware.
4. Add parser fuzz targets and sustained fuzzing evidence.
5. Measure the published performance targets on documented hardware.
6. Test the GUI on each supported OS, including accessibility and large-result behavior.

## Release engineering

1. Resolve signing and notarization credentials.
2. Build, sign, verify, and checksum each release artifact.
3. Validate installation on clean Windows, macOS, and Linux systems.
4. Submit package-manager manifests after stable release assets exist.
5. Publish a documentation site only after its links and claims are verified.

## Deferred product work

Checkpoint resume, multi-pass imaging maps, GUI report export, and additional image/container formats remain unimplemented. Any new carver or filesystem parser needs deterministic fixtures and a documented evidence level. TUI and MCP support remain deferred.

Do not run scans or recovery against a person's physical device without their explicit confirmation of source and destination. Always start with a dry run and keep recovery output on a different physical volume.
