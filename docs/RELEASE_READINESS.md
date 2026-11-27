# Release Readiness — Relift

**Date:** 2026-11-27  
**Branch:** main  
**Current release:** v1.0.0 (stable, published)  
**Previous release:** v0.1.0 Preview (prerelease, unsigned)

## Release state

| Item | Status | Evidence |
|------|--------|----------|
| Tag | v1.0.0 exists (stable) | `git tag v1.0.0`, GitHub release page |
| Previous tag | v0.1.0 exists (prerelease) | `git tag`, GitHub release page |
| Assets | 13 binaries + SHA256SUMS | GitHub release assets |
| Signing | Not signed, not notarized | Release notes state "All release assets are unsigned" |
| Notarization | Not configured | No Apple Developer ID, no Authenticode certificate |
| SBOM | Not generated | No CycloneDX or equivalent |
| Attestations | Not generated | No GitHub artifact attestations/provenance |
| Package managers | Submitted as PRs | Scoop #8883, Homebrew #328237, winget #490778, Flathub #11383 |
| Checksums | SHA256SUMS attached | Verified against GitHub asset digests |

## Release artifact audit

v1.0.0 assets (from GitHub API):

| Asset | SHA-256 | Size |
|-------|---------|------|
| Relift-0.1.0-1.x86_64.rpm | 831421f0d58521f08d18c460f8715b935b72438eeef6fd3687472dd9eedea968 | 3,579,617 |
| relift-linux-aarch64.tar.gz | 7bf9a4255ef0eac1565a933b8d4c8d33e0fa233e7416ab61754d3aa72b2b6c69 | 869,991 |
| relift-linux-x86_64.tar.gz | 13b94aa21ef683dc9350cc37ad7c4be1057429b4d2b39f80d4eb6b889b1e9ad5 | 932,099 |
| relift-macos-aarch64.tar.gz | 7fc1b18f1ede08e227fd771477140a79387cc9fd95f537e9c2b3196af5143295 | 781,807 |
| relift-macos-x86_64.tar.gz | 50bd5b04359d35e45efc5a4aab2ea2aa8164260e8251554b9a7837b230a53a04 | 886,611 |
| relift-windows-x86_64.zip | 13a65e4a5de5c1ba7dfdfa755c27de3ff396e1dfd6f5bcb53d15a37545431196 | 874,091 |
| Relift_0.1.0_amd64.AppImage | 5458d545b3e84559a67c6f4948cadaf4cc4cb5c89d7e6ba2397fe6f19bbeb21f | 87,054,840 |
| Relift_0.1.0_amd64.deb | 5beedb2c26ff0a238c7944a54b2c78e62ffbb1ed670b319671c0b2a134f690f8 | 3,574,588 |
| Relift_0.1.0_universal.dmg | 907a13f3c8ab4047a11cf1edb0861b3af8bce42d871e368b19aba7fa07af86d6 | 6,691,431 |
| Relift_0.1.0_x64-setup.exe | 8fac6d057f570c8c21734f6879997408681ff3e38e9b71435ed7be5791f2e906 | 2,502,745 |
| Relift_0.1.0_x64_en-US.msi | 4b719fe397dc8e46e1b75176c88d9e18780dd3f515edad87d0816408054cda94 | 3,629,056 |
| Relift_universal.app.tar.gz | d3a1e3bb48a9ef791b70810fdfd9fda31d85b943134ecdd85320c51e9b86d6 | 6,601,994 |
| SHA256SUMS | 77d8d9b283f1a859b1ba8ad895b27bc60f51d5a601ba0ba319fff0ec9a4627aa | 1,138 |

All 13 entries present. Checksums verified against GitHub's reported asset digests.

## Stable release gates (v1.0.0)

Per the master prompt (Section 10.3), a stable release requires:

| Gate | Status | Evidence |
|------|--------|----------|
| Resolved P0/P1 issues | PASS | CI fixed, claims reconciled (PR #1 merged) |
| Current documentation | PASS | README, docs, llms files reconciled |
| Accurate support matrix | PASS | Corrected to reflect probe-tested experimental parsers |
| Safe end-to-end validation | PARTIAL | Synthetic fixtures only; no real-media validation |
| Tested clean installs | NOT DONE | Installer testing not performed |
| Verified artifacts | PARTIAL | Checksums exist; no SBOM/attestations/signing |
| Release notes with limitations | PASS | v1.0.0 notes are honest about limitations |
| Owner approval | PASS | Owner authorized tag and release creation |

## Package manager submissions

| Channel | PR URL | Status |
|---------|--------|--------|
| Scoop | https://github.com/ScoopInstaller/Main/pull/8883 | Submitted |
| Homebrew | https://github.com/Homebrew/homebrew-core/pull/328237 | Submitted |
| winget | https://github.com/microsoft/winget-pkgs/pull/490778 | Submitted |
| Flathub | https://github.com/flathub/flathub/pull/11383 | Submitted |
| AUR | Not submitted | Requires SSH access to AUR git repository |
| Snap | Not submitted | Requires snapcraft command (not available on Windows) |

## Blockers

| Blocker | Severity | Required action | Gate |
|---------|----------|-----------------|------|
| No signing credentials | P3 | Owner must approve enrollment in signing program | Owner |
| No notarization | P3 | macOS bundles not notarized; owner must approve | Owner |
| No SBOM/attestations | P3 | Generate CycloneDX SBOM and GitHub artifact attestations | Engineering |
| No clean-install testing | P2 | Test install/uninstall on claimed platforms | Engineering |
| No real-media validation | P2 | Cannot be done by this agent (rule 3.1) | Owner |
| AUR submission | P3 | Requires SSH access to AUR git repo | Owner/manual |
| Snap submission | P3 | Requires snapcraft command | Owner/manual |

## What must NOT happen

- Do not overwrite existing v1.0.0 assets.
- Do not publish to crates.io or any package registry without owner approval.
- Do not claim binaries are signed/notarized if they are not.
- Do not use placeholder checksums or invented URLs in any manifest.
