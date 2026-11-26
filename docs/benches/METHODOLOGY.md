# Benchmark Methodology

Relift benchmarks use [Criterion.rs](https://bheisler.github.io/criterion.rs/book/) with warm-up and
statistical analysis. Results below are from a single development host; they are
indicative of relative performance, not a substitute for hardware-specific
measurement.

## Host

- OS: Windows 11 x86_64
- CPU: not recorded (no CPU identification tooling available in CI)
- RAM: not recorded
- Disk: NTFS, local SSD
- Toolchain: Rust stable 1.100.0 MSVC, release profile (LTO, codegen-units=1, panic=abort)
- Criterion backend: plotters (gnuplot not available)

## Benchmarks

| Benchmark | Input | What it measures |
|---|---|---|
| `signature_sweep/memchr_jpeg_64mb` | 64 MB buffer with 64 JPEG SOI markers | memmem signature search throughput on false-positive-heavy data |
| `carvers/validate_jpeg_1mb` | 1 MB structurally valid JPEG | End-to-end JPEG carving validation (scan_volume) |
| `carvers/validate_zip_1mb` | 1 MB structurally valid ZIP | End-to-end ZIP carving validation (scan_volume) |
| `metadata_scan/fat32_metadata` | Deterministic FAT32 fixture (~34 MiB) | FAT32 deleted-entry metadata recovery |
| `metadata_scan/ntfs_metadata` | Deterministic NTFS fixture | NTFS MFT deleted-record metadata recovery |
| `metadata_scan/ext4_metadata` | Deterministic ext4 fixture | ext4 deleted-inode metadata recovery |
| `probe/fat32` | Deterministic FAT32 fixture | Filesystem signature probing only |
| `probe/exfat` | Deterministic exFAT fixture | Filesystem signature probing only |
| `probe/ntfs` | Deterministic NTFS fixture | Filesystem signature probing only |
| `probe/ext4` | Deterministic ext4 fixture | Filesystem signature probing only |

## How to run

```sh
# Run all benchmarks once to verify they compile and execute:
cargo bench -p relift-bench -- --test

# Run full benchmarks with statistical analysis:
cargo bench -p relift-bench

# Compare against a saved baseline:
cargo bench -p relift-bench -- --baseline relift-2026-11-26
```

## Results (2026-11-26)

Saved in `docs/benches/results/benchmark-2026-11-26.log`. These are **local
development measurements**, not published performance targets. No accuracy or
throughput SLA is claimed.
