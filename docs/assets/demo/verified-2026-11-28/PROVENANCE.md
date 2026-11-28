# Relift app capture set

Captured on 2026-11-28 from the running Windows Tauri application. These files
are real UI captures, not mockups.

## Test source and scan

- Source: generated 128 MiB FAT32 image from `docs/assets/demo/scripts/genimg.rs`
- Synthetic-image self-check: 15 known files; 11 marked deleted and 4 live controls
- SHA-256 before and after the GUI scan: `0cc2c65aaea9fbb4ded6b240d6171d64f06bba9861a4b29337951fb6d6ad2f33`
- GUI workflow: quick metadata scan, no signature carving; 11 deleted-file candidates shown
- The app was driven through its WebView2 debugging interface to start the scan and capture states. The actual Tauri `scan_cmd` handled the image and wrote the session.
- No physical device was scanned. No recovery destination was selected, and no recovered-file write was run.

## Capture details

Screenshots were captured from the running app at a 1280 × 800 WebView viewport. `source-selection.png` is a crop of the real source-selection page; it ends above the host's physical-device list. The remaining PNGs show the scan, result list, and recovery-destination safety page. `walkthrough.gif` is encoded from four of those actual app frames without added graphics or text.

The temporary image and scan sessions were created under `C:\Users\Public\ReliftBrandDemo-20261128` and are not included in the repository. That image contains only generated test data.

| File | Captured state |
|---|---|
| `source-selection.png` | App header, safety notice, and image-selection control (cropped) |
| `scan-progress.png` | Real scan at the start of probing |
| `scan-complete.png` | Completed quick scan with 11 candidates |
| `results.png` | Candidate list loaded from that scan session |
| `recovery-destination.png` | Recovery destination and safety requirements; no destination chosen |
| `walkthrough.gif` | Scan, completed results, and recovery safety states |
