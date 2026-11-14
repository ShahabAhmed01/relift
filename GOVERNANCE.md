# Governance

## Project structure

Relift is a **benevolent dictator** project with a transparent, documented decision process. The BDFL (Shahab Ahmed) has final say on merging, but all significant decisions are documented in ADRs and discussed publicly.

## Roles

| Role | Responsibilities | Selection |
|------|------------------|-----------|
| **BDFL** | Final merge authority, architectural direction, release signing | Project founder |
| **Maintainers** | Review PRs, triage issues, merge approved PRs, cut releases | Invited by BDFL after sustained contribution |
| **Contributors** | Submit PRs, report issues, improve docs, add tests | Anyone |
| **Security team** | Handle vulnerability reports, coordinate disclosure | BDFL + 1 maintainer |

## Decision making

### Minor decisions (bug fixes, docs, small features)
- PR author + 1 maintainer approval → merge

### Major decisions (architecture, new dependencies, breaking changes)
1. Open GitHub Discussion with proposal
2. 7-day comment period
2. ADR written documenting options, decision, rationale
3. BDFL decides; records in `docs/decisions/ADR-XXXX.md`

### Security decisions
- Handled per `SECURITY.md` (private disclosure, 90-day timeline)
- BDFL + security team coordinate

## Release process

1. **Versioning**: Semantic Versioning (MAJOR.MINOR.PATCH)
2. **Branching**: `main` only; tags for releases (`v1.0.0`)
3. **Changelog**: Keep a Changelog format, human-written from Conventional Commits
4. **Signing**: All artifacts signed (Authenticode, Developer ID, minisign)
5. **Attestation**: GitHub artifact attestations + SLSA provenance
6. **Publish**: GitHub Release + package managers + website

## Code of Conduct

Enforced per `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1). All participants expected to comply.

## Security

See `SECURITY.md`. Vulnerabilities reported privately via GitHub Security Advisory.

## License

GPL-3.0-or-later. Contributions dual-licensed under same terms. No CLA required; submitting a PR implies agreement.

## Trademark

"Relift" and the logo are trademarks of the project. Use in derivative works requires permission. Forks must rename.

## Funding

No corporate sponsor. Donations via GitHub Sponsors / Open Collective fund:
- CI compute credits
- Code signing certificates
- Domain/hosting
- Conference travel for maintainers

Transparent reporting in `FUNDING.yml` and annual report.

## Archival policy

If project becomes unmaintained:
1. 6 months no commits + no maintainer response → "unmaintained" badge
2. 12 months → Archive repo, publish final signed release
3. Forks encouraged; trademark reverts to community