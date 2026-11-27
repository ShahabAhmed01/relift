# Desktop GUI

Relift's desktop interface uses Tauri 2 and Svelte 5. It calls the shared
`relift-core` library through Tauri commands. The GUI is a subset of the CLI;
see [`CLI.md`](CLI.md) for imaging, report export, shell completions, and other
commands.

## Choose a source

The first screen lists devices visible to the operating system and lets the
user select a raw image file. It shows the device path, type, size, filesystem
label when available, accessibility status, and any OS-provided note. It does
not inspect SMART health or reliably determine SSD TRIM status. Only raw image
files are supported directly.

The safety notice recommends stopping use of the source and recovering to a
different physical drive. If the source is failing, consider professional
recovery or image it before scanning.

Advanced options currently include:

- Quick scan: filesystem metadata.
- Deep scan: metadata plus validated signature carving.
- Include files that are still allocated.

Formatted-partition recovery, lost-partition recovery, and worker-thread
controls are not implemented.

## Scan

The scan screen receives progress events from the running scan and displays
phase, bytes processed, elapsed time, estimated throughput, ETA, and the number
of candidates saved in the session. ETA and throughput are estimates, not
promises. The user can stop the scan and keep results already written to the
session. Live pause is not implemented, and the GUI does not expose the CLI's
checkpoint-resume workflow.

Candidates are reviewed after the scan finishes or is stopped. The GUI does
not preview or recover candidates while scanning.

## Review and recover

The results screen supports text search, type and confidence filters, sorting,
selecting visible rows, and selecting Verified/Likely candidates. Its preview
shows candidate metadata and validator notes; it does not render the recovered
file contents.

The user selects a destination folder and starts recovery. The shared recovery
engine validates the destination and verifies output hashes by default. The GUI
shows recovered, partial, failed, and byte counts when the operation finishes.
Recovery destination safety is still subject to the platform limits in
[`support-matrix.md`](support-matrix.md).

## Current exclusions

- No imaging workflow in the GUI; use `relift image` from the CLI.
- No report format picker in the GUI; use `relift report` from the CLI.
- No content viewer, folder-tree view, or 500,000-row performance claim.
- No pause/resume for a running scan, and no resume control in the GUI; the
  backend supports `relift scan --resume` from the CLI.
- No device-health, SMART, or TRIM diagnosis.
- No real screenshot evidence is included in the repository yet. See [GUI_CAPTURE.md](../GUI_CAPTURE.md) for the owner-run capture procedure using deterministic synthetic fixtures.

The GUI is built in the release workflow, but platform runtime behavior,
accessibility, cold-start time, and large-result performance have not been
measured as release gates. See [`VERIFICATION.md`](VERIFICATION.md).
