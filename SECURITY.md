# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| Latest release | ✓ |
| Previous release | ✓ (security fixes only) |

## Reporting a vulnerability

**Do not open a public issue.** Report security vulnerabilities privately via:

- GitHub Security Advisory: https://github.com/ShahabAhmed01/relift/security/advisories/new
- Email: security@relift.rs (if available)

Include:
- Description of the vulnerability
- Steps to reproduce
- Affected versions
- Potential impact
- Suggested fix (if any)

We aim to acknowledge within 48 hours and provide a fix timeline within 7 days.

## Threat model

Relift operates on untrusted storage media. The threat model assumes:

1. **Hostile disk images/volumes:** Attackers control filesystem structures, file contents, and metadata. Malformed NTFS/FAT/ext4/APFS structures, zip bombs, billion-laughs XML, crafted JPEGs, integer overflows in runlists, infinite B-tree loops.

2. **Untrusted recovered files:** Recovered files may contain malware, exploits, or malicious payloads. They must never be executed, opened with vulnerable parsers, or processed without sandboxing.

3. **Local privilege escalation:** Raw device access requires elevated privileges. The CLI/GUI must not escalate privileges beyond what the user explicitly grants.

4. **Supply chain:** Dependencies must be audited for known vulnerabilities and license compliance.

## Mitigations

### Parser hardening
- All parsers use checked arithmetic (`checked_add`, `checked_mul`, `saturating_sub`)
- Explicit recursion/iteration limits on all loops (max 1M iterations default)
- Allocation caps per structure (max 1 GB per file, 4 GB per container)
- Sanity bounds on every offset/length before use
- No `unwrap()`/`expect()` on attacker-controlled data
- Timeouts per candidate via cancellation token

### Fuzzing
- `cargo-fuzz` targets for every filesystem parser and file-format validator
- Nightly CI runs each target for 5+ minutes
- Corpus minimized and committed; regressions detected

### Dependency auditing
- `cargo-deny` enforces license allowlist (no GPL in linked code except our own)
- `cargo-audit` checks for known vulnerabilities (RUSTSEC)
- Dependabot weekly updates for Cargo + GitHub Actions
- Pinned GitHub Actions by SHA

### Runtime safety
- **Zero network calls** in the recovery engine (proven by test with network denied)
- No telemetry, no analytics, no auto-update checks in core
- `relift self-update` is opt-in, verifies minisign signature
- Recovered files written with restrictive permissions (no execute bit)
- Preview sandbox: time/memory/dimension limits, no external resources

### Destination safety
- Source device opened read-only at OS level (`O_RDONLY`, `GENERIC_READ`)
- Recovery destination verified on different physical device (by device ID, not path)
- Path traversal in filenames neutralized (`..`, absolute paths, reserved names, ADS colons)
- Atomic writes: temp file → fsync → rename
- Optional `--verify` re-reads and hashes written files

## Secure release process

1. Release tags are signed (GPG)
2. Windows: Authenticode signed (if cert available)
3. macOS: Developer ID signed + notarized (if cert available)
4. Linux: minisign detached signatures
5. All artifacts: SHA-256 checksums published
6. SBOM (CycloneDX) generated per artifact
7. GitHub artifact attestations (SLSA provenance)
8. Submitted to VirusTotal; clean report linked in release notes

## Verifying releases

```sh
# Verify minisign signature (Linux/macOS)
minisign -V -P <pubkey> -m relift-x86_64-unknown-linux-gnu.tar.gz

# Verify GitHub attestation
gh attestation verify relift-x86_64-unknown-linux-gnu.tar.gz \
  --repo ShahabAhmed01/relift

# Verify SHA-256
sha256sum -c SHA256SUMS
```

## Hardening checklist (pre-release)

- [ ] `cargo audit` clean
- [ ] `cargo deny check` clean
- [ ] All fuzz targets run 5+ min with no crashes
- [ ] Network-denied test passes
- [ ] Source-write attempt test fails (read-only invariant)
- [ ] Destination-on-source test fails (safety guard)
- [ ] Path traversal test passes (sanitization)
- [ ] Fuzzing corpus updated with new crashers
- [ ] Dependency licenses audited
- [ ] SBOM generated and attached

## Disclosure timeline

1. Day 0: Vulnerability reported privately
2. Day 1-2: Acknowledgment sent
3. Day 1-7: Fix developed and tested
4. Day 7-14: Coordinated disclosure (CVE requested if applicable)
5. Day 14+: Public advisory published with release

## Contact

For security questions not related to vulnerability reporting, open a Discussion or see `SUPPORT.md`.