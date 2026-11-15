# Roadmap

This roadmap lists open work only. The authoritative support and release gaps are in docs/GAP_REPORT.md and docs/VERIFICATION.md. No dates are promises.

## Priority 1: Evidence and safety

- Validate read-only source behavior and destination separation on real devices using an approved, controlled test setup.
- Build a labeled corpus for filesystem metadata and carving precision/recall.
- Add parser fuzz targets and run sustained fuzzing.
- Measure release performance on documented hardware, including large images and large result sets.
- Exercise macOS raw-device behavior and Windows/Linux destination edge cases.

## Priority 2: Release engineering

- Resolve or explicitly accept the development-only deepmerge advisory when upstream guidance changes.
- Configure Windows code signing and Apple Developer ID signing/notarization.
- Generate and publish checksums and provenance for release assets.
- Validate installation and upgrade paths for each supported bundle.
- Submit package-manager manifests only after a stable release has verified assets.

## Priority 3: Product scope

- Add checkpoint-based scan resume.
- Add multi-pass imaging with a bad-sector map and resume support.
- Consider E01/AFF4/VHD/VMDK/QCOW2/DMG input after format-specific validation.
- Add additional carvers only with structural validation and deterministic fixtures.
- Consider GUI report export, pause/resume, accessibility review, and large-result testing.
- Reassess TUI or MCP work only when there is a concrete supported use case.

Do not represent a roadmap item as implemented until its tests and evidence are added to the support matrix and verification record.
