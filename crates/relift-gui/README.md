# Relift GUI development

The desktop interface uses Tauri 2, SvelteKit 3, Svelte 5, and Vite 8. The frontend calls the shared Rust recovery engine through Tauri commands.

## Requirements

- Rust 1.90 or newer.
- Node.js 22.17 or newer.
- Tauri platform prerequisites: WebView2 and Visual Studio Build Tools on Windows; Xcode Command Line Tools on macOS; WebKitGTK development packages on Linux.

## Install and check

From this directory:

    npm ci
    npm run check
    npm run build

For a live desktop session:

    npm run tauri dev

Build desktop packages:

    npm run tauri build

The frontend type check currently passes with no errors or warnings. The Windows package command produces MSI and NSIS installers. Release bundles are unsigned unless release credentials are configured.

## Current interface

The supported flow is source selection, scan progress, results review, and recovery. The results screen includes destination selection and recovery. Imaging and report export are CLI-only. The separate recover route remains in the source tree but is not linked from the main results flow.

The GUI does not provide pause/resume, checkpoint recovery, content viewing, SMART or TRIM diagnosis, or an imaging workflow. See ../../docs/GUI.md and ../../docs/GAP_REPORT.md for details.

## Project files

- src/routes: home, scan, results, and an unlinked recover route.
- src/lib/components/Icons.svelte: shared icon component.
- src-tauri/src/commands.rs: Tauri command handlers.
- src-tauri/tauri.conf.json: desktop application and bundling configuration.
- vite.config.ts: SvelteKit, static adapter, and frontend build configuration.
