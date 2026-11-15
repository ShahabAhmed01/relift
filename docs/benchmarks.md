# Benchmarks

No benchmark results have been published yet. Earlier versions of this file
contained illustrative numbers that were never measured; they have been removed
so the project does not quote unverified performance.

## Running the benchmarks

```bash
# Build and run every Criterion benchmark
cargo bench -p relift-bench

# Run one benchmark group
cargo bench -p relift-bench -- signature_sweep

# Save a baseline and compare later
cargo bench -p relift-bench -- --save-baseline main
cargo bench -p relift-bench -- --baseline main
```

Criterion writes HTML reports to `target/criterion/`.

## Recording a result

When publishing numbers, include:

- Commit SHA and release version.
- CPU, RAM, storage model and interface (for example NVMe 4.0).
- Operating system and kernel version.
- Rust version and build profile (`release`, LTO settings).
- Image size, filesystem and candidate count.
- Command line used.
- Wall time, throughput, peak RSS, candidate count, verified count.

Use the methodology in `docs/benches/METHODOLOGY.md` so results are comparable.

## Current benchmark groups

`relift-bench` includes Criterion groups for:

- Signature sweep throughput on a synthetic buffer with many false positives.
- Validator throughput for representative carved formats.
- Metadata scan throughput on generated FAT32, exFAT, NTFS and ext4 fixtures.
- Filesystem probe throughput.

These are smoke/regression benchmarks, not a published performance claim. The
CI workflow runs a benchmark smoke test on a schedule; it does not publish or
compare results against a hosted baseline.
