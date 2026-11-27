# GUI screenshot capture procedure

**Status:** OWNER TASK — requires a graphical desktop environment.

The screenshots currently checked in under `docs/assets/screenshots/` (gui-home.png, gui-scan.png, gui-results.png, gui-recover.png) are **not** genuine captures of the running Relift GUI. They are identical placeholder images (a VS Code window). Do not use them as product evidence.

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
