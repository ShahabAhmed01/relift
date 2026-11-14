# Architecture

## Overview

Relift is a data recovery tool built around a **read-only recovery engine** (`relift-core`) with two frontends: a CLI (`relift-cli`) and a GUI (`relift-gui` via Tauri 2 + Svelte 5). The engine never writes to the source device; all writes go through an explicit recovery writer that validates the destination first.

```
┌─────────────────────────────────────────────────────────────────┐
│                        relift-core                              │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │  Source I/O  │  │  Partition   │  │   Filesystem Parsers │  │
│  │  (read-only) │──►│   Detection  │──►│  (NTFS, FAT, exFAT,  │  │
│  │  BlockDevice │  │  (MBR/GPT/   │  │   ext2/3/4, APFS,    │  │
│  │  memmap2,    │  │   APM, LVM,  │  │   HFS+, Btrfs, XFS)  │  │
│  │  io_uring)   │  │   RAID)      │  └──────────┬───────────┘  │
│  └──────────────┘  └──────────────┘             │              │
│         │                                       ▼              │
│         │  ┌──────────────────────────────────────────────┐   │
│         │  │           ScanEngine                         │   │
│         │  │  • Orchestrates probes & carvers             │   │
│         │  │  • Streams candidates via CandidateSink      │   │
│         │  │  • Manages session persistence (SQLite)      │   │
│         │  │  • Progress/cancellation                     │   │
│         │  └──────────────────────────────────────────────┘   │
│         │                     │                                │
│         │                     ▼                                │
│         │  ┌──────────────────────────────────────────────┐   │
│         │  │           Carver Registry                     │   │
│         │  │  • 25+ format validators (CRC, Huffman, etc.) │   │
│         │  │  • SIMD-accelerated multi-pattern search      │   │
│         │  │  • Fragmented file reconstruction             │   │
│         │  └──────────────────────────────────────────────┘   │
│         │                     │                                │
│         ▼                     ▼                                │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │              Session Store (SQLite + WAL)                 │  │
│  │  • Candidates, extents, progress checkpoints              │  │
│  │  • Resumable after crash/kill                             │  │
│  │  • Portable .relift-session files                         │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
          ┌───────────────────┼───────────────────┐
          ▼                   ▼                   ▼
    ┌──────────┐        ┌──────────┐        ┌──────────┐
    │   CLI    │        │   GUI    │        │   MCP    │
    │ (clap)   │        │ (Tauri2) │        │  Server  │
    └──────────┘        └──────────┘        └──────────┘
```

## Core principles

### Read-only invariant
The `ReadOnlySource` trait exposes **no write methods**. It is the only way the engine accesses source data. The CLI/GUI open sources via `open_source()` which returns `Box<dyn ReadOnlySource>`. The recovery writer (`recover.rs`) is the **only** code that writes, and it:
1. Verifies destination is on a different physical device (by device ID, not path)
2. Creates output directory with `create_dir_all`
3. Writes each file to a temp name, `fsync`, then renames
4. Optionally re-reads and verifies SHA-256

### Streaming architecture
Scans never buffer entire volumes in memory. The `ScanEngine`:
1. Reads source in 256 MB chunks (configurable)
2. Runs filesystem probes and carvers in parallel via `rayon`
3. Emits candidates through `CandidateSink` trait (implementations: `Session` for persistence, `VecSink` for tests)
4. Backpressure via bounded channels

### Validation over heuristics
Every carved candidate must pass a **structural validator**:
- JPEG: SOI → segments → SOF → scan data → EOI, Huffman table check
- PNG: IHDR → IDAT chunks with CRC32 → IEND
- ZIP: Local headers → Central Directory → EOCD, CRC32 of each entry
- MP4: ftyp → moov/moof/mdat box tree, size consistency
- etc.

Unvalidated hits are **never emitted**. This eliminates the false-positive flood common in header-only carvers.

### Confidence scoring
Every candidate carries a `Confidence`:
```rust
struct Confidence {
    label: ConfidenceLabel,  // Verified / Likely / Partial / Unverified
    score: u8,               // 0-100
    reasons: Vec<String>,    // Human-readable evidence
}
```
- Metadata-recovered (MFT, FAT dir, ext4 inode): 95-100, `Verified`
- Carved with full structural validation: 80-95, `Verified`/`Likely`
- Carved with partial validation: 40-70, `Partial`
- Signature only (no validator): never emitted

## Data flow

### Scan
```
Source → PartitionTable → VolumeRef(s) → probe() → FsInfo
                                    │
                  ┌─────────────────┼─────────────────┐
                  ▼                 ▼                 ▼
             scan_metadata      scan_metadata    scan_volume (carve)
             (NTFS MFT)         (FAT dirents)    (signatures)
                  │                 │                 │
                  └─────────────────┼─────────────────┘
                                    ▼
                           CandidateSink.emit()
                                    │
                                    ▼
                           Session (SQLite)
                                    │
                                    ▼
                           UI/CLI progress
```

### Recovery
```
Session → load candidates → filter/select → RecoveryWriter
                                                    │
                              ┌─────────────────────┼─────────────────────┐
                              ▼                     ▼                     ▼
                        Source read           Temp file write          SHA-256 verify
                        (ReadOnlySource)      (atomic rename)          (optional)
                              │                     │                     │
                              └─────────────────────┼─────────────────────┘
                                                    ▼
                                           Recovery manifest
                                           (JSON + HTML + CSV)
```

## Key crates

| Crate | Responsibility |
|-------|----------------|
| `relift-core` | Engine, parsers, carvers, imaging, session, recovery |
| `relift-cli` | CLI commands, progress rendering, JSON output |
| `relift-gui` | Tauri 2 app, Svelte 5 frontend, IPC commands |
| `relift-testkit` | Deterministic filesystem fixtures (FAT32, exFAT, NTFS, ext4) |
| `relift-bench` | Criterion benchmarks for scan throughput, validator speed |

## Concurrency model

- **I/O threads:** 1 per device (sequential reads optimal for HDD; for NVMe up to `num_cpus()`)
- **Carver pool:** `rayon` thread pool, work-stealing, chunked by 256 MB regions
- **Session writes:** Single writer thread, batched transactions (WAL mode)
- **GUI updates:** Tauri events from background tasks, debounced at 30 Hz

## Memory management

- **Zero-copy parsing:** `nom` parsers operate on `&[u8]` slices from `memmap2` or `read_vec`
- **Bounded buffers:** 256 MB read chunks, reused via pool
- **Candidate streaming:** Never more than `max_candidates` in memory; oldest spilled to SQLite
- **Typical RSS:** < 200 MB scanning 1 TB NVMe with deep carving

## Error handling

- Library code: `thiserror` enums, no `unwrap()` on fallible I/O
- CLI boundary: `anyhow` for context, mapped to exit codes
- GUI: Tauri commands return `Result<T, String>`, displayed as toasts

## Extensibility

### Adding a carver
1. Create `crates/relift-core/src/carve/<format>.rs` with `validate_<format>()`
2. Add `Marker` to `MARKERS` array
3. Add `MAX_<FORMAT>` constant
4. Add match arms in `marker_max()` and `validate()`
5. Write unit tests + fuzz target

### Adding a filesystem
1. Create `crates/relift-core/src/fs/<fs>.rs` with `probe()` and `scan()`
2. Implement `FsInfo` detection and `CandidateSink` emission
3. Add module in `fs/mod.rs`, update `probe()` and `scan_metadata()` match arms
4. Add fixture generator in `relift-testkit` if possible