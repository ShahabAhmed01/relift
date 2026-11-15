# Verification — Relift Gate Status

This document tracks the verification status of all acceptance gates from the master prompt. Each gate must have real evidence (command output, test results, benchmark data) to be marked PASS.

## Gate Status Summary

| Gate | Description | Status | Evidence |
|------|-------------|--------|----------|
| G1 | CI builds on Linux/macOS/Windows | PASS | [Run 43691011208](https://github.com/ShahabAhmed01/relift/actions/runs/43691011208) |
| G2 | Metadata recall ≥99%, precision ≥99% | ⬜ PENDING | Corpus results JSON |
| G3 | Carving recall ≥95%, precision ≥98% | ⬜ PENDING | Corpus results JSON |
| G4 | Zero writes to source (hash before/after) | PARTIAL | Synthetic image scan hash check; raw-device check unavailable |
| G5 | Destination-on-source refused | PARTIAL | Synthetic device ancestry tests; mounted raw-device check unavailable |
| G6 | Throughput targets met | PARTIAL | Local synthetic Criterion smoke run; no published hardware baseline |
| G7 | Quick scan 1TB NTFS < 90s | ⬜ PENDING | Benchmark CSV |
| G8 | GUI cold start <500ms, 60fps at 500k rows | ⬜ PENDING | Perf measurement |
| G9 | CLI JSON schema stable, exit codes tested | PASS | `relift-cli` contract tests |
| G10 | Resume after kill -9 works | ⬜ PENDING | Test output |
| G11 | Fuzzing clean 24h | ⬜ PENDING | Fuzz logs |
| G12 | Signed releases all platforms | ⬜ PENDING | Release assets + attestation |
| G13 | Real screenshots from running app | ⬜ PENDING | Screenshot files |
| G14 | Docs site live, indexed, JSON-LD valid | ⬜ PENDING | Link check + validator |
| G15 | Anti-slop + honesty review passed | ⬜ PENDING | Checklist |
| G16 | All package managers installable | ⬜ PENDING | Install test logs |
| G17 | Launch kit complete | ⬜ PENDING | Growth assets |
| G18 | This file complete with real evidence | ⬜ PENDING | This file |

---

## Detailed Evidence (to be filled during verification)

### G1: CI Builds
```
# Command: git push origin main
# Expected: All 3 OS matrix jobs green
# Evidence: [CI run URL]
```

### G2: Metadata Recovery Accuracy

The checked-in tests use synthetic fixtures and do not measure recall or
precision over a representative labeled corpus. G2 remains pending until those
metrics are collected.

### G3: Carving Accuracy

Format-specific tests validate deterministic examples, but there is no mixed
labeled corpus or per-format recall and precision report. G3 remains pending.

### G4: Read-Only Invariant

The CLI contract test hashes a deterministic 1 MiB image before and after a
scan and confirms the bytes are unchanged. This verifies the image-file path;
raw-device before/after hashing on physical media remains unavailable.

### G5: Destination Safety

- Synthetic Linux sysfs tests cover partition ancestry, stacked devices,
  multiple NVMe namespaces and fail-closed virtual devices. Raw-device writes
  against mounted physical disks have not been exercised; G5 remains partial.

### G6: Performance

- `cargo +1.90 bench -p relift-bench --offline -- --measurement-time 1
  --sample-size 10 --warm-up-time 1 --noplot`: completed all synthetic
  signature, validator, metadata and probe benchmark groups on Windows.
- This short local run confirms the harness executes. It does not establish
  release performance targets or provide the full hardware and storage profile
  required for a public comparison; G6 remains partial.

### G9: CLI Contract

- `cargo +1.90 test -p relift-cli --all-targets --offline`: passed (2 unit and
  3 process-level contract tests).
- Tests validate version, scan, list and dry-run recovery JSON fields; source
  mismatch returns 4, usage error returns 2, and missing-source runtime error
  returns 1. Unit tests cover all documented error and partial-result exit-code
  mappings (0 through 5).

#### Linux physical-disk ancestry hardening (2026-11-15)

- Added synthetic sysfs tests proving that two partitions resolve to their
  shared parent disk, different disks remain distinct, NVMe namespaces on one
  controller share an identity, and virtual devices without dependencies are
  rejected.
- `cargo test --workspace --all-targets`: passed on Windows (62 tests total).
- `cargo check -p relift-core --all-targets --target x86_64-unknown-linux-gnu`:
  passed.
- A Linux-only fixture test also exercises `/sys/dev/block` symlink resolution;
  cross-target checking compiles it, but this Windows host cannot execute it.
- `cargo clippy -p relift-core --all-targets --target x86_64-unknown-linux-gnu
  -- -D warnings`: passed.
- This is compile-time and synthetic mapping evidence only. The Linux guard has
  not been exercised against mounted raw devices; G5 remains partial.

#### Destination writer race hardening (2026-11-15)

- Recovery and imaging now open destination paths component-by-component with
  no-follow semantics and keep the directory handles anchored while writing.
  Linux/macOS finalization uses no-replace rename; Windows uses a no-overwrite
  move and keeps the destination ancestor handles open.
- Regression coverage includes symlinked parent rejection, destination-root
  replacement, Windows ancestor rename prevention, collision-safe output
  creation/finalization, and preserving an existing image manifest.
- `cargo +1.90 test --workspace --all-targets --offline`: passed on Windows
  (59 core unit tests, 6 core end-to-end tests, 2 fixture tests, and 5 CLI
  contract tests).
- `cargo +stable check -p relift-core --all-targets --target
  x86_64-unknown-linux-gnu --offline`: passed. This compiles Linux-specific
  code but does not execute Linux tests.
- `cargo +1.90 build --workspace --release --offline`: passed on Windows
  (optimized workspace build, including CLI and Tauri GUI).
- `cargo +1.90 clippy --workspace --all-targets --offline -- -D warnings`:
  passed after replacing the CLI's explicit closure drops with scoped borrows.
- `RUSTDOCFLAGS='-D warnings' cargo +1.90 doc --workspace --no-deps --offline`:
  passed after correcting the CLI option documentation's literal angle-bracket
  placeholders and scoping the Linux-only safety helper.
- Full GitHub Actions run [43691011208](https://github.com/ShahabAhmed01/relift/actions/runs/43691011208)
  passed stable and Rust 1.90 checks on Windows, Linux, and macOS, including
  workspace tests, Clippy, docs, dependency checks, and release builds.
- `cargo deny check`: advisory, ban, license, and source checks passed. It
  reported existing duplicate transitive crate versions and unused license
  allowances as warnings; neither blocked the check.
- Linux runtime tests and physical-device destination checks remain unverified;
  macOS runtime device checks remain unverified. G5 remains partial.

### G6: Performance Benchmarks
```bash
cargo bench -p relift-bench 2>&1 | tail -30
# Expected: All targets in docs/benchmarks.md met
# Evidence: [benchmark output with MB/s, RSS, times]
```

### G7: Quick Scan Performance
```bash
# Run on 1TB NTFS fixture (or scaled)
# Evidence: [benchmark output showing <90s]
```

### G8: GUI Performance
```bash
# Measure cold start with Tauri devtools
# Measure 500k row virtualization FPS
# Evidence: [screenshots of perf overlay]
```

### G9: CLI Contract

See the contract tests and results under [G9: CLI Contract](#g9-cli-contract).

### G10: Resume After Kill
```bash
# Test: start scan, kill -9, resume, compare results
# Evidence: [test output showing identical results]
```

### G11: Fuzzing
```bash
# Run cargo-fuzz for 24h on all targets
# Evidence: [fuzz log summary showing 0 crashes]
```

### G12: Signed Releases
```bash
# Evidence: [gh attestation verify output + minisign verify + codesign verify]
```

### G13: Real Screenshots
```bash
# Evidence: [ls docs/assets/screenshots/ showing 8+ real captures]
```

### G14: Docs Site
```bash
# Evidence: [linkcheck output + structured data validator output + sitemap]
```

### G15: Anti-Slop Checklist
- [ ] No emoji in headings/UI
- [ ] No "🚀 Blazingly fast" marketing fluff
- [ ] No gradient heroes, glassmorphism, purple-blue AI gradients
- [ ] No stock illustrations, lorem ipsum, placeholder names
- [ ] No fake progress bars, invented ETAs
- [ ] No dead buttons or "coming soon" in shipped UI
- [ ] No mock/stub data in production paths
- [ ] No `unwrap()` on external input in library code
- [ ] No swallowed errors
- [ ] Consistent spacing/typography
- [ ] Honest limitations table in README

### G16: Package Managers
```bash
# Test each in clean container:
brew install relift          # macOS
winget install relift.vestige # Windows
scoop install relift         # Windows
cargo install relift-cli     # Anywhere
npx relift-cli --version     # Node
pipx install relift-cli      # Python
# AUR, Flatpak, Snap: manual verification
# Evidence: [install logs from clean containers]
```

### G17: Launch Kit
- [ ] Show HN draft (`growth/show-hn.md`)
- [ ] Reddit drafts for r/rust, r/datarecovery, r/sysadmin, r/opensource
- [ ] Product Hunt assets (`growth/producthunt.md`)
- [ ] 90s demo video (`growth/demo.mp4`)
- [ ] asciinema cast (`growth/demo.cast`)
- [ ] Chinese track (`growth/zh/` with README.zh-CN.md, 掘金/知乎/Bilibili posts)
- [ ] Press kit (`growth/press-kit/`)

### G18: This File
- [ ] All gates have real command output pasted above
- [ ] No gate marked PASS without evidence
- [ ] Known gaps documented below

---

## Known Gaps (Honest Assessment)

### Not Yet Implemented
- [ ] ReFS metadata recovery
- [ ] F2FS metadata recovery  
- [ ] ZFS/ReiserFS support
- [ ] RAID 0/1/5 reconstruction
- [ ] E01/AFF4 write support
- [ ] FLAC, RAR5, ZIP64, OLE carvers
- [ ] TUI (ratatui)
- [ ] MCP server
- [ ] WASM carver plugins

### Not Yet Verified
- [ ] Real NTFS/ext4 volumes (only synthetic fixtures)
- [ ] macOS raw device paths
- [ ] Windows ARM64 builds
- [ ] Full 24h fuzz campaign
- [ ] Signed releases (no certificates yet)
- [ ] Package manager submissions
- [ ] Docs site deployment
- [ ] SEO indexing
- [ ] Launch execution

### Performance Not Yet Measured
- [ ] 1TB NTFS quick scan on NVMe
- [ ] 4TB deep scan memory profile
- [ ] GUI 500k row virtualization FPS
- [ ] Cold start on all platforms

---

## Verification Log

| Date | Gate | Status | Verifier | Notes |
|------|------|--------|----------|-------|
| 2024-11-14 | All | PENDING | — | Initial document creation |
| 2026-11-15 | G5 | PARTIAL | Local agent | Linux ancestry mapping tests and cross-target checks pass; raw-device runtime validation remains pending. |

---

## Final Sign-Off

This project is **not** verified for release until all 18 gates have real evidence pasted above and all are marked PASS.

**Do not mark gates PASS without pasted command output.** An accurate gap report is a success; a fake "100% complete" is a failure.
