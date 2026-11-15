# Benchmark Methodology

## Purpose

This document defines how to measure Relift reproducibly. It describes the
process, not results. No reference benchmark numbers are published yet.

## Reference environment

Record the following for every published run:

| Component | Value |
|-----------|-------|
| CPU | model, cores/threads, base/boost clock |
| RAM | size, speed, configuration |
| Source storage | model, interface, advertised sequential read |
| Destination storage | physically separate model/interface |
| OS | distribution and kernel (or Windows/macOS build) |
| Rust | `rustc --version`, target triple, build profile |
| Node.js | version used for the GUI build, if measured |
| Power/thermal | CPU governor, swap state, any isolation settings |

## Build and run

```bash
cargo build --release -p relift-bench
cargo bench -p relift-bench
```

Criterion writes HTML reports under `target/criterion/`. Use
`--save-baseline <name>` and `--baseline <name>` for before/after comparisons.

## Test data

Benchmarks use generated data, not committed multi-gigabyte images:

- `relift-testkit` creates FAT32, exFAT, NTFS and ext4 fixtures with known
  deleted files. The end-to-end tests verify recovered bytes with SHA-256.
- Signature-sweep and validator benchmarks use in-memory buffers built by the
  benchmark harness.

Do not claim accuracy (recall/precision) from synthetic fixtures beyond the
specific deletion scenarios encoded in them. Real-media validation is tracked
in `docs/GAP_REPORT.md`.

## Metrics

- **Wall time** — total duration of the measured operation.
- **Throughput** — bytes processed per second (state whether this counts the
  whole image or only carved regions).
- **Peak RSS** — maximum resident set size (`/usr/bin/time -v` on Unix;
  Task Manager/Process Explorer on Windows).
- **Candidates** — emitted candidates, and how many are Verified/Likely.
- **Verified bytes** — bytes successfully re-read and compared by SHA-256.

## Reporting rules

- Never publish a number without the command, image size and hardware.
- Compare like for like: same image, same filesystem, same candidate filters.
- Report medians and sample counts; Criterion already does this.
- Mark any result from synthetic fixtures as synthetic.
