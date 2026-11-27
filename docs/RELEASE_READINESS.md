# Release Readiness — Relift

**Date:** 2026-11-27  
**Branch:** `fix/ci-and-claim-reconciliation`  
**Current release:** v0.1.0 Preview (prerelease, unsigned)  
**Target:** Stable release when all gates pass

## Release state

| Item | Status | Evidence |
|------|--------|----------|
| Tag | v0.1.0 exists (prerelease) | `git tag`, GitHub release page |
| Assets | 12 binaries + SHA256SUMS | GitHub release assets |
| Signing | Not signed, not notarized | Release notes state "All release assets are unsigned" |
| Notarization | Not configured | No Apple Developer ID, no Authenticode certificate |
| SBOM | Not generated | No CycloneDX or equivalent |
| Attestations | Not generated | No GitHub artifact attestations/provenance |
| Package managers | Drafts only, placeholders | `packaging/` contains unreleased templates |
| Checksums | SHA256SUMS attached | Verified against GitHub asset digests |

## Release artifact audit

v0.1.0 assets (from GitHub API):

| Asset | SHA-256 (from manifest) | Size |
|-------|------------------------|------|
| Relift-0.1.0-1.x86_64.rpm | be02bc6d... | 3,572,089 |
| relift-linux-aarch64.tar.gz | 07c68ca9... | 857,483 |
| relift-linux-x86_64.tar.gz | 950ee2fa... | 919,959 |
| relift-macos-aarch64.tar.gz | 4cbe0bef... | 770,041 |
| relift-macos-x86_64.tar.gz | d40f9830... | 874,157 |
| relift-windows-x86_64.zip | 7063a9b9... | 860,909 |
| Relift_0.1.0_amd64.AppImage | 9e4e2c97... | 82,205,176 |
| Relift_0.1.0_amd64.deb | 496a1728... | 3,566,508 |
| Relift_0.1.0_universal.dmg | c3c8978e... | 6,676,333 |
| Relift_0.1.0_x64-setup.exe | 2b496c7d... | 2,500,895 |
| Relift_0.1.0_x64_en-US.msi | 0b5aa19b... | 3,616,768 |
| Relift_universal.app.tar.gz | d0d73e96... | 6,584,162 |
| SHA256SUMS | 536658eb... | 1,138 |

All 13 entries present. Checksums verified against GitHub's reported asset digests during the v0.1.0 release workflow (run 43701736196).

## Stable release gates

Per the master prompt (Section 10.3), a stable release requires:

1. [ ] Resolved P0/P1 issues — CI fixed on branch; PR #1 open for review
2. [ ] Current documentation — README, docs, llms files reconciled
3. [ ] Accurate support matrix — corrected to reflect probe-tested experimental parsers
4. [ ] Safe end-to-end validation — synthetic fixtures only; no real-media validation
5. [ ] Tested clean installs for claimed platforms — not performed (no installer testing)
6. [ ] Verified artifacts — checksums exist; no SBOM/attestations/signing
7. [ ] Release notes with limitations — v0.1.0 notes are honest about limitations
8. [ ] Owner approval to create tag/release — REQUIRED (gate)

## Blockers

| Blocker | Severity | Required action | Gate |
|---------|----------|-----------------|------|
| No signing credentials | P3 | Owner must approve enrollment in signing program (SignPath, Apple Developer ID, etc.) | Owner |
| No notarization | P3 | macOS bundles not notarized; owner must approve | Owner |
| No SBOM/attestations | P3 | Generate CycloneDX SBOM and GitHub artifact attestations in release workflow | Engineering |
| Package manifests are placeholders | P3 | Replace `REPLACE_WITH_*` values with real URLs/checksums; owner must approve submission per channel | Owner |
| No clean-install testing | P2 | Test install/uninstall on Windows, macOS, Linux in disposable environments | Engineering |
| No real-media validation | P2 | Cannot be done by this agent (rule 3.1); prepare human-run protocol | Owner |

## What must NOT happen

- Do not convert v0.1.0 prerelease into a stable release without owner approval.
- Do not overwrite existing v0.1.0 assets.
- Do not publish to crates.io or any package registry without owner approval.
- Do not claim binaries are signed/notarized if they are not.
- Do not use placeholder checksums or invented URLs in any manifest.

## Next steps

1. Owner reviews PR #1 and merges to main.
2. Engineering adds SBOM and attestation generation to release.yml.
3. Owner approves signing/notarization enrollment (or explicitly declines).
4. Replace packaging placeholders with verified values.
5. Perform clean-install testing on each claimed platform.
6. Owner creates stable tag and release.
