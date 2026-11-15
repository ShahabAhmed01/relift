# Forensic Use and Evidence Handling

Relift is a data-recovery utility, not a certified forensic acquisition tool.
It has no case-management mode, DFXML export, source-range hashing, investigator
fields, hardware write-blocker control, or checkpoint-resume workflow. Do not
describe its output as a validated forensic image or a complete chain-of-
custody record.

For legally or professionally consequential evidence, follow your
organization's procedures and applicable standards. Use a hardware write
blocker and a validated forensic acquisition tool when required. Have a
qualified examiner review the acquisition and analysis plan.

## Safer recovery workflow

1. Stop using the source device. Do not write recovered output back to it.
2. Confirm the source and destination with the operator. The destination must
   be a different physical volume.
3. If appropriate, create a raw image on the separate destination. Relift's
   image command makes a single-pass copy and writes a SHA-256 manifest; it
   does not provide bad-sector maps, retries, or multi-pass resume.
4. Scan the image, review candidates, and run a dry-run recovery.
5. Recover only after confirming the dry-run and destination. Keep the image,
   session directory, report, and recovered output under controlled storage.

Example for an already-created raw image:

```sh
relift scan /evidence/source.img --session /evidence/session --mode quick
relift list /evidence/session --json
relift recover /evidence/session --dest /recovery --dry-run
# After reviewing the plan and confirming /recovery is a separate physical volume:
relift recover /evidence/session --dest /recovery
relift report /evidence/session --format json --out /evidence/report.json
```

For metadata plus carving, use `--mode deep`. Raw device paths, permissions,
and destination checks vary by operating system; see
[`support-matrix.md`](support-matrix.md).

## What the session records

The session directory stores a versioned JSON header, candidate JSONL records,
recovery-result JSONL records, and a README. The source identity uses a
fingerprint derived from source size and sampled bytes; it is not a cryptographic
hash of every source byte. Recovered output files are SHA-256 verified by
default after writing. The image manifest contains the image copy's hash.

These records are useful for repeatable recovery work, but are not a tamper-proof
audit log and do not replace independent evidence acquisition, full-source
hashing, timestamps from a trusted clock, or examiner notes. Preserve copies
and compute any hashes required by your procedure independently.

## Important limits

- The source abstraction is read-only, but this alone does not certify forensic
  soundness or prevent the operating system from changing a mounted source.
- Destination separation is checked by Relift where the platform can establish
  physical backing. Review warnings and do not use an ambiguous destination.
- Only raw disk images are supported directly; other containers must be
  converted with an appropriate tool.
- Filesystem recovery has deterministic synthetic-fixture evidence only.
  Real-media accuracy and forensic validation have not been established.
- Relift does not decrypt BitLocker, LUKS, or FileVault, reconstruct RAID, or
  repair partition tables.
- No `--forensic`, `--case-id`, `--investigator`, `--verify`, or DFXML export
  option exists. SHA-256 recovery verification is on by default and can be
  disabled with `--no-verify`.

See [`limitations.md`](limitations.md), [`GAP_REPORT.md`](GAP_REPORT.md), and
[`VERIFICATION.md`](VERIFICATION.md) for current support and evidence status.
