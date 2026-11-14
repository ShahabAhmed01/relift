# Threat Model

## Scope

Relift is a **read-only data recovery tool** for disk images and raw block devices. It operates on storage media that may contain:
- Malicious filesystem structures (crafted NTFS/FAT/ext4/APFS)
- Hostile file contents (exploits, malware, zip bombs, billion-laughs XML)
- Corrupted or intentionally damaged data

The threat model assumes the **source media is untrusted**. The tool must not crash, hang, allocate unbounded memory, or execute code when processing adversarial input.

## Assets to protect

1. **Source device integrity** — Relift must never write to the source device.
2. **Destination integrity** — Recovered files must not escape the chosen output directory.
3. **System stability** — No crashes, hangs, or resource exhaustion from malformed input.
4. **User data privacy** — No telemetry, no network calls, no data exfiltration.
5. **Forensic soundness** — Recovery process must be auditable and reproducible.

## Threat actors

| Actor | Capability | Motivation |
|-------|------------|------------|
| Malicious disk image creator | Controls every byte of image | Exploit parser bugs, cause DoS, escape output dir |
| Compromised storage device | Returns crafted data on read | Same as above |
| Local attacker | Can run Relift with crafted args | Privilege escalation, data theft |
| Remote attacker | N/A (no network) | N/A |

## Attack surfaces

### 1. Filesystem parsers (NTFS, FAT, exFAT, ext4, APFS, HFS+, Btrfs, XFS)

**Threats:**
- Integer overflow in offset/length calculations
- Out-of-bounds reads/writes via corrupted metadata
- Infinite loops in B-tree/directory traversal
- Unbounded recursion in attribute/record parsing
- Allocation exhaustion via huge metadata structures (e.g., 1M fake MFT records)

**Mitigations:**
- All arithmetic uses `checked_add`, `checked_mul`, `saturating_sub`
- Explicit bounds check before every `read_at`: `offset + len <= source.len()`
- Hard caps: max 1M MFT records, 1M directory entries, 256 B-tree depth
- Iteration limits with `options.cancelled` token checked every 1000 iterations
- `nom` parser combinators with `take_while` bounds
- No `unwrap()` on parsed data; all errors propagated as `Error::Corrupt`

### 2. File carvers (25+ format validators)

**Threats:**
- Zip bombs (nested ZIP, compression ratio 1000:1)
- Billion-laughs XML in OOXML (DOCX/XLSX/PPTX)
- Malformed JPEG with huge segment lengths
- PNG with 2^31 chunk sizes
- MP4 with deep box nesting
- SQLite with corrupted freelist causing infinite loop

**Mitigations:**
- Every validator has hard `MAX_<FORMAT>` size limit (see `carve.rs`)
- Per-structure allocation caps (e.g., max 1M ZIP entries, 1M PNG chunks)
- CRC32/checksum validation before trusting lengths
- Deflate stream decode with output limit (`MAX_GZIP_OUT = 512 MB`)
- XML not parsed; DOCX/XLSX/PPTX validated by ZIP structure + content-type presence only
- Recursion depth limits on box/tree walking (max 1000)

### 3. Imaging engine

**Threats:**
- Infinite retry loops on bad sectors
- Unbounded memory for error map
- Map file corruption leading to resume on wrong device

**Mitigations:**
- Bounded retries (configurable, default 3)
- Exponential backoff with max delay
- Error map stored in SQLite (bounded, indexed)
- Source identity (serial + size + partition offset) verified on resume

### 4. Recovery writer

**Threats:**
- Path traversal in recovered filenames (`../../etc/passwd`)
- Absolute paths (`/etc/shadow`, `C:\Windows\System32`)
- Symlink/junction/reparse point attacks
- ADS colons in filenames (`file.txt:malware.exe`)
- Reserved Windows names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`...)
- Overlong paths (>260 chars on Windows without `\\?\`)

**Mitigations:**
- `sanitize_path()` in `recover.rs`:
  - Strips `..` components
  - Rejects absolute paths
  - Replaces reserved names with `_reserved_`
  - Removes ADS suffixes (`:stream`)
  - Truncates to 255 chars per component
  - Uses `\\?\` prefix on Windows for long paths
- Destination verified on different physical device (by device ID, not path)
- Atomic write: temp file → `fsync` → rename
- No execute bit on recovered files

### 5. CLI / GUI input handling

**Threats:**
- Command injection via crafted arguments
- Malicious session directory paths
- JSON injection in `--json` output

**Mitigations:**
- `clap` validates all CLI arguments; no shell invocation
- Paths resolved with `canonicalize()` where possible
- JSON output uses `serde_json::to_writer` (safe)
- GUI: Tauri IPC uses typed commands; no `eval` or dynamic code

### 6. Resource exhaustion

**Threats:**
- 100 TB image → unbounded memory
- Millions of candidates → RAM exhaustion
- Deeply nested archives → stack overflow
- Malformed filesystem with 1B directory entries → CPU hang

**Mitigations:**
- `--max-mem` flag (default 256 MB) caps RSS via streaming + SQLite spill
- `--max-candidates` (default 1M) stops scan
- `--timeout` flag stops after duration
- Iteration guards on all loops
- `rayon` thread pool bounded by `num_cpus()`

### 7. Supply chain

**Threats:**
- Compromised dependency (typosquatting, malicious publish)
- License incompatibility (GPL in MIT project)
- Known vulnerability in transitive dependency

**Mitigations:**
- `cargo-deny` with license allowlist (no GPL in linked code)
- `cargo-audit` (RUSTSEC) in CI
- Dependabot weekly updates
- Pinned GitHub Actions by SHA
- Minimal dependencies; each justified in `docs/dependencies.md`

### 8. Privacy / Telemetry

**Threats:**
- Hidden analytics, crash reporting, update checks
- File contents uploaded
- Device serial numbers logged

**Mitigations:**
- **Zero network calls** in `relift-core` and `relift-cli` (enforced by test with network denied)
- `relift self-update` is opt-in, off by default, verifies minisign signature
- Logs redact serial numbers and user paths by default (`--verbose` opts in)
- No crash reporter, no analytics SDK

## Security testing

### Fuzzing
- `cargo-fuzz` targets for every parser and validator
- Nightly CI: 5 min per target, corpus minimized
- Regression corpus committed

### Property testing
- `proptest` for round-trip encode/decode of on-disk structures
- Random byte streams fed to parsers; must not panic

### Static analysis
- `cargo clippy -D warnings` (pedantic, with justified allows)
- `cargo audit` for known vulnerabilities
- `cargo deny` for license/advisory/ban checks

### Penetration testing (pre-release)
- Corpus of 1000+ malicious images (from DFIR community, generated)
- Run full scan pipeline; assert no crashes, no writes to source
- Verify destination confinement with path traversal attempts

## Incident response

If a vulnerability is found:

1. **Report privately** via GitHub Security Advisory
2. **Acknowledge within 48 hours**
3. **Fix within 7 days** for critical (RCE, source write), 30 days for moderate
4. **Coordinated disclosure** with CVE if applicable
5. **Release patched version** with signed artifacts
6. **Update `CHANGELOG.md`** with security note

## Forensic soundness

Relift is **not a certified forensic tool**. It is designed for data recovery, not evidence acquisition. For forensic use:

- Use hardware write blocker
- Image first (`relift image --verify`)
- Run Relift on the image, not the original
- Enable `--forensic` flag (SHA-256 of source ranges, DFXML export)
- Document tool version, command line, environment
- Verify recovered files independently

The read-only invariant and session persistence support chain of custody, but Relift does not meet NIST SP 800-86 or ISO 27037 certification requirements.