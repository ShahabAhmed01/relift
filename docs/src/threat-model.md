# Threat Model

## Scope and assumptions

Relift reads disk images and raw devices that may contain corrupt or hostile
metadata. It writes recovered files or images only under a user-selected
destination. The source, session, command-line paths, and file contents are
untrusted inputs. Relift is not a certified forensic tool and does not defend
against a compromised operating system or a local process that controls the
destination volume.

## Assets

1. **Source integrity:** scanning and imaging must not write to the source.
2. **Destination confinement:** recovered paths must remain inside the chosen
   destination and must not overwrite an existing file unexpectedly.
3. **Recovered-data integrity:** report successful output verification and
   distinguish partial or failed recovery.
4. **User privacy:** do not send device data, file contents, or telemetry over
   the network.
5. **Process stability:** reject malformed offsets and structures safely and
   avoid unbounded result accumulation where the session writer can stream data.

## Controls implemented

### Source and destination

- The core scanner accepts a `ReadOnlySource` with no write operation.
- Imaging and recovery use explicit destination writers. Destination paths are
  opened component-by-component, without following links, and retained as
  handles while output is created. Output files use collision-safe creation.
- Source identity is recorded in the session; recovery refuses a source whose
  size or fingerprint differs from the recorded source.
- Platform-specific physical-device checks reject known same-device recovery
  destinations. When backing cannot be proven, supported paths fail closed.
- Recovered output is SHA-256 checked by default. The user can disable the
  post-write read-back check with `--no-verify`.

### Parser and output handling

- Parsers use bounded reads and checked range validation at input boundaries.
- Signature carving emits a candidate only when its format validator accepts
  the structure.
- Filesystem and carve limitations are recorded in the support matrix and gap
  report; a signature match alone is not represented as validated recovery.
- Candidate records stream to JSONL session files rather than being retained as
  one in-memory result set. `--max-candidates` limits the number of candidates.
- Recovered names are sanitized and destination traversal is rejected. See
  [`GAP_REPORT.md`](GAP_REPORT.md) for platform-specific output behavior.

### Privacy and dependencies

- The core and CLI contain no network client or telemetry.
- The `doctor` command reports network/telemetry status but also attempts a
  read-only open and 512-byte read from a platform-specific raw-device path.
- `cargo deny check` runs in CI for dependency advisories, licenses, bans, and
  source configuration.

## Remaining threats and evidence gaps

- Synthetic tests do not prove safety against every malformed image or real
  device behavior. No comprehensive fuzz harness or 24-hour fuzz campaign is
  checked in.
- The Linux device-ancestry tests are synthetic; there has been no same-device
  refusal test against mounted physical disks.
- Raw-device behavior and physical destination checks are not validated on
  actual media. macOS raw-device access is untested and destination checks fail
  closed.
- Resource behavior on multi-terabyte images and very large candidate sets has
  not been profiled. There is no `--max-mem` or `--timeout` option.
- Scan resume is volume-level: an interrupted volume is rescanned from its
  start, and candidates written after the last completed volume are discarded.
  The source fingerprint samples head/tail bytes rather than hashing the whole
  source, so a change outside the sampled ranges is not detected. A process
  crash may leave partial destination output; failed-output cleanup differs by
  platform.
- A local process with write access to the destination can alter output or
  cause denial of service.
- No signed release artifacts or notarized GUI bundles exist. The published
  v0.1.0 preview has unsigned assets; its SHA256SUMS file does not authenticate
  the publisher.

## Forensic limits

Relift is a recovery utility, not an evidence-acquisition or chain-of-custody
system. Session fingerprints sample the source rather than hashing every byte;
session files are not tamper-evident. No case-management mode, DFXML export, or
source-range hashing is implemented. For forensic use, follow organizational
procedures, use an appropriate write blocker, and independently acquire and
hash evidence with validated tools.

See [`VERIFICATION.md`](VERIFICATION.md) for test results and release gates.
