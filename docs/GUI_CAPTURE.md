# GUI screenshot capture procedure

**Status:** Genuine app captures added on 2026-11-28. The historical screenshots
under `docs/assets/screenshots/` remain unverified; the README uses the separate
verified capture set described below.

The older screenshots under `docs/assets/screenshots/` are **not verified captures of the running Relift GUI**. Audit found that `gui-home.png` shows an editor/agent conversation rather than Relift, while CLI images expose a local Windows profile path. Their capture history is unavailable. Do not use those files as product evidence.

## Current capture record

- **Capture set:** `docs/assets/demo/verified-2026-11-28/`
- **Application:** Running Windows Tauri app, 1280 × 800 viewport
- **Source:** Fresh 128 MiB synthetic FAT32 image from `docs/assets/demo/scripts/genimg.rs`
- **Workflow:** Quick metadata scan, saved results, and recovery destination screen
- **Results:** 11 deleted-file candidates; the scan source hash remained unchanged
- **Writes:** No recovered files were written; no physical device was scanned
- **Privacy:** The source-selection image is cropped above the host device list. Other images show only the synthetic path under `C:\Users\Public`.
- **GIF:** `walkthrough.gif` is encoded from the actual app captures; it has no added UI or mock results.

See the set's [`PROVENANCE.md`](assets/demo/verified-2026-11-28/PROVENANCE.md) for the image hash and exact capture details.

## What to capture

Capture the **real running application** using only a deterministic synthetic fixture. Required screens:

1. **Home / source selection** — device picker and image open dialog
2. **Scan in progress** — genuine progress bar, bytes processed, candidate count
3. **Results** — filterable table with realistic synthetic filenames and metadata
4. **Recovery** — destination selection and recovery summary

## How to capture

### Prerequisites

- A machine with a graphical desktop (Windows, macOS, or Linux)
- Rust 1.90+ and Node.js 22.17+ installed
- The Relift repository cloned

### Build the GUI

```sh
cargo build --release --workspace
cd crates/relift-gui
npm ci
npm run tauri build
```

### Create a deterministic synthetic fixture

Use `relift-testkit` or create a raw image file with known deleted content. Do not use real user data or physical devices.

### Run and capture

1. Launch the GUI binary
2. Select the synthetic fixture as the source
3. Run a quick scan, then a deep scan
4. Capture each screen at a consistent window size (recommended 1200×800)
5. Use a clean light or dark theme consistently
6. Ensure no private paths, usernames, device IDs, or real user data appear

### Asset requirements

- Consistent window size across all screens
- Clean theme (light or dark, not mixed)
- Readable text at thumbnail size
- Descriptive alt text for each image
- Store PNG or optimized WebP in `docs/assets/screenshots/`
- Document capture date, version, and fixture used

### Regenerating

There is no automated capture script checked in. If you add one (e.g., using a UI automation tool), store it under `scripts/` and document how to run it in CI.

## What not to do

- Do not fabricate UI art or mockups and label them as product screenshots.
- Do not capture a VS Code or terminal window and label it as the GUI.
- Do not include private paths, usernames, or physical device identifiers.
- Do not claim a feature is implemented if the screenshot shows a placeholder.
