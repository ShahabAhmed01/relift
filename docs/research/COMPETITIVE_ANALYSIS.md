# Open source recovery landscape and Relift gaps

**Reviewed:** 2026-11-28

**Scope:** Representative public projects and their documented behavior. This is not an exhaustive GitHub census or a head-to-head recovery benchmark. Product claims below come from the linked primary project documentation. Relift has not been tested against a shared real-media corpus.

## Projects that define useful baselines

| Project | Main strength | What Relift should learn from it |
|---|---|---|
| [TestDisk and PhotoRec](https://github.com/cgsecurity/testdisk) | Partition and boot-sector recovery; PhotoRec carving across hundreds of format families; Windows, Linux and macOS support | Partition discovery and filesystem breadth are mature domains. PhotoRec documents per-format selection, free-space-only scans, resuming interrupted recovery, and the limits of fragmented-file carving. Relift should grow format coverage only with validators and ground-truth fixtures. |
| [GNU ddrescue](https://www.gnu.org/software/ddrescue/manual/ddrescue_manual.html) | Resilient acquisition of failing media using a persistent mapfile, resumable passes and selective retries | Relift now resumes interrupted copies from checksummed prefix checkpoints. Its next imaging gaps are unreadable-range accounting, sector retries, multiple passes, and ddrescue map interoperability. |
| [The Sleuth Kit](https://github.com/sleuthkit/sleuthkit) | Reusable forensic filesystem analysis libraries and command-line tools | Mature engines can extend filesystem support faster than writing many new parsers. Any integration needs a careful license, packaging, source-safety and evidence-provenance review. |
| [RecovMe](https://github.com/trevnedai/recovme) | Open-source GUI and CLI orchestration around established recovery engines | A product can provide a coherent workflow by coordinating proven tools instead of cloning every parser. Relift can retain its native engine while adding opt-in adapters where the license and security boundary permit. |
| [Foremost](https://github.com/korczis/foremost) | Small, configurable header/footer data carving utility | User-defined signatures and clear per-format controls are useful, but a header/footer match alone is weak recovery evidence. Any custom format support should expose validation strength and avoid presenting unvalidated bytes as verified. |

PhotoRec documents more than 480 extensions across roughly 300 file families, supports selecting formats, and warns that fragmentation can make carved files incomplete. TestDisk documents partition recovery and undelete workflows across several filesystem families. GNU ddrescue's mapfile records which areas have been copied or remain problematic so a run can continue without rereading successful regions. These are source-documented capabilities, not Relift benchmark results.

## Relift as implemented in this checkout

- The Rust workspace has a shared recovery core, CLI, Tauri/Svelte GUI, fixture toolkit and benchmarks.
- The source abstraction has no write operation. Raw-device destination checks reject destinations whose physical backing cannot be proven separate.
- Metadata recovery is end-to-end fixture-tested for FAT12/16/32, exFAT, NTFS and ext2/3/4. HFS+, APFS, Btrfs and XFS remain experimental; their detection fixtures do not prove deleted-file recovery.
- The carver registry contains 153 format identifiers. Some registry entries are markers without an emitting validator; the number 153 is not a count of equally mature or end-to-end-tested formats. Candidates are emitted only when their format validator accepts the structure.
- Scan sessions resume at volume checkpoints. Imaging resumes at 256 MiB durable prefix checkpoints by default, verifies the partial prefix against the current source, and truncates uncheckpointed output. This re-reads the checkpointed range on resume. Scan-session identity still uses sampled blocks, so a change in an unsampled region may not be detected there.
- Recovery verifies output bytes by default. `relift verify` independently streams SHA-256 checks and confines output paths to the session's recorded destination.
- `relift partitions-scan` looks for known filesystem signatures and probes candidates. Its estimates are leads for inspection, not proof of a recoverable partition or a write-ready partition table.
- Virtual-disk and forensic-image commands are not end-to-end scan support. Some readers are incomplete, some formats return `NotImplemented`, and `open_source` still reads regular raw image files and raw devices.
- TOML settings apply to scan, list and recover; unknown config keys and invalid defaults are rejected.

The support boundary and validation evidence are maintained in the [support matrix](../src/support-matrix.md), [verification record](../src/VERIFICATION.md), and [claims ledger](../src/claims-ledger.md).

## What Relift should add next

1. **Complete bad-sector-aware imaging.** Add unreadable-range accounting, bounded sector retries, multi-pass recovery and ddrescue map import/export. Keep the source handle read-only and require a separate destination before the first write.
2. **A real-media validation corpus.** Obtain legally redistributable or user-generated fixtures with known ground truth. Record source provenance, filesystem, deletion method, expected bytes and hashes. Publish false-positive, byte-accuracy and path/name recovery results.
3. **Broader metadata support through maintained engines.** Prototype an optional Sleuth Kit adapter for filesystems Relift does not implement. Verify license compatibility, binary provenance, no-write behavior, and a clear distinction between native and delegated results before shipping it.
4. **Safer partition workflows.** Improve lost-partition candidate validation, size confidence and user review. Never silently write a reconstructed partition table.
5. **Honest parser coverage.** Track registered signatures, validators, positive fixtures, malformed-input fixtures and real-corpus validation as separate measures. Prefer fewer well-validated formats over a large headline count.
6. **Operational usability.** Add bounded-memory candidate pagination to the GUI/CLI, clear cancellation and resume semantics, exportable scan diagnostics, and signed reproducible release artifacts.

These are ranked by recovery risk reduction and user value. They are roadmap items, not current features. No comparative accuracy or speed claim is justified until Relift and peer tools are run on the same published corpus and hardware.

## References

- [PhotoRec documentation](https://github.com/cgsecurity/testdisk_documentation/blob/master/photorec.rst)
- [PhotoRec user manual](https://www.cgsecurity.org/testdisk_doc/photorec.html)
- [TestDisk and PhotoRec repository](https://github.com/cgsecurity/testdisk)
- [GNU ddrescue manual](https://www.gnu.org/software/ddrescue/manual/ddrescue_manual.html)
- [The Sleuth Kit repository](https://github.com/sleuthkit/sleuthkit)
- [RecovMe repository](https://github.com/trevnedai/recovme)
- [Foremost repository](https://github.com/korczis/foremost)
- [Relift support matrix](../src/support-matrix.md)
