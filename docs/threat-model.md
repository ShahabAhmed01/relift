# Threat model

Relift is a data-recovery tool that reads evidence media and writes recovered
files to a separate destination. This threat model describes the adversarial and
accidental threats Relift is designed to resist, and those it explicitly does
not address.

## Assets

- Source media (disk image or raw device) — the evidence; must not be modified.
- Recovered output files — must be byte-faithful to the source.
- Session metadata — must accurately reflect what was found.

## Threat model

### T1: Accidental write to source media

**Threat:** The tool writes to the image or device being recovered, destroying
evidence.

**Mitigation:** `ReadOnlySource` exposes no write method. All recovery code
receives `&dyn ReadOnlySource`. Images open read-only via `File::open`; Windows
raw devices open `GENERIC_READ` only; Unix raw devices open `O_RDONLY`.

**Residual risk:** A mounted filesystem can still be modified by the OS in the
background. Relift warns and recommends imaging first.

### T2: Recovery output written onto evidence

**Threat:** Output files are written back to the same disk as the source,
overwriting recoverable data.

**Mitigation:** `validate_destination` proves the destination is separate from
the source before any write:
- Image: canonical same-file rejection.
- Windows: `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS` physical-disk mapping.
- Linux: `/proc/self/mountinfo` + `rdev` comparison.
- macOS: fail-closed refusal with imaging advice.

### T3: Path traversal / arbitrary write via filesystem names

**Threat:** A corrupted or malicious filesystem entry name contains `..`,
absolute paths, symlinks, or reserved device names (`CON`, `NUL`, etc.), causing
output to escape the destination or overwrite system files.

**Mitigation:** Every path component is sanitized (`sanitize_component`),
traversal components are dropped, reserved names are escaped (`_CON`), symlinks
are refused, and the final path is confined to the destination root.

### T4: Source swapped between scan and recover

**Threat:** A different image is supplied at recovery time, producing output
that does not match the session's expectations.

**Mitigation:** `SourceIdentity` (size + SHA-256 fingerprint over head/tail
bytes) is recorded at scan time and checked at recovery time. A mismatch is
refused with exit code 4.

### T5: Truncated / partial recovery presented as complete

**Threat:** An incomplete write is presented as a fully recovered file.

**Mitigation:** Output files are opened with `create_new`; a file with an I/O
error is renamed to `.partial` so a complete filename always means the byte copy
finished. Each output is re-read and SHA-256 verified against the copy stream
(verification on by default).

### T6: False carving positives

**Threat:** Signature carving emits byte ranges that merely look like a file
header but do not parse.

**Mitigation:** Only structurally validated carves are emitted (end-marker/CRC/
EOCD/box-chain validators + format-specific size caps). Unvalidated formats are
not claimed.

### T7: Integer overflow / out-of-bounds reads from corrupted metadata

**Threat:** A corrupt filesystem length or offset field causes a panic or
out-of-bounds read.

**Mitigation:** All multi-byte reads go through bounds-checked helpers
(`u16_le`, `u32_le`, etc.) returning `Option`; `read_vec` is capped at
`MAX_BUFFERED_READ` (64 MiB); all offset arithmetic uses `checked_*`.

### T8: Supply chain / trojaned dependencies

**Threat:** A dependency is compromised.

**Mitigation:** Minimal dependency set (`serde`, `serde_json`, `sha2`, `memchr`,
`crc32fast`, `flate2`, `thiserror`, `clap`, `anyhow`). Offline default; no
network client.

## Out of scope

- Relift does not protect against a compromised host OS.
- Relift does not encrypt or authenticate recovered output beyond SHA-256.
- Relift does not provide chain-of-custody features beyond session headers and
  source identity fingerprints.
- Real-device forensics on mounted volumes is explicitly discouraged; image
  first.
