# Validation evidence

This directory holds reproducible validation artifacts for Relift's recovery
behavior. All evidence is generated from deterministic synthetic fixtures and
loopback filesystem images. **No physical drives are tested here.**

## Structure

| Path | Contents |
|------|----------|
| `scripts/` | Reproducible image-generation and test scripts |
| `corpus/` | Checksums and metadata for pinned test corpora (no actual image files committed) |
| `logs/` | Raw test logs from CI runs |
| `scorecards/` | Generated per-format and per-filesystem result scorecards |

## Evidence states

Relift distinguishes four evidence states, which are never collapsed into a
vague "supported" label:

1. **Synthetic fixture tested** — deterministic in-repo fixtures, SHA-256
   byte comparison. (Current state for FAT32, exFAT, NTFS, ext4 metadata
   recovery and all 36 carving formats.)
2. **Loopback filesystem image tested** — filesystem created as a regular
   file, populated, deleted, and recovered. (Planned; not yet scripted.)
3. **Public corpus tested** — CFReDS, DFTT, Digital Corpora, or DFRWS images
   with independently verified checksums and ground truth. (Not yet done;
   no corpus licensed or downloaded.)
4. **Physical-media tested** — real drives or volumes. (Not done; prohibited
   by rule 3.1 of the master prompt for this agent.)

## Adding validation evidence

1. Create a script under `scripts/` that generates a disposable filesystem
   image as a regular file in a temporary directory.
2. Populate it with a deterministic known corpus.
3. Delete controlled files and run Relift against the image.
4. Compare recovered output to original SHA-256 hashes.
5. Record the raw log under `logs/` with date, toolchain, environment, and
   command.
6. Generate a scorecard under `scorecards/` with pass/fail per file.
7. Update `docs/VERIFICATION.md` and `docs/TEST_RESULTS.md` accordingly.

## What not to commit

- Do not commit actual disk image files (too large, may contain identifiable data).
- Do not commit copyrighted or restricted forensic corpora.
- Do not commit private paths, usernames, or device identifiers.
- Do not claim physical-media validation without actual drive testing.
