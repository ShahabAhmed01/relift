# Relift GUI Development

## Prerequisites

- Rust 1.82+ (via rustup)
- Node.js 20+ (for frontend)
- Tauri 2 prerequisites:
  - **Windows**: WebView2 Runtime (Win 10 1809+ includes it), Visual Studio Build Tools
  - **macOS**: Xcode Command Line Tools
  - **Linux**: WebKitGTK 4.1+, `libwebkit2gtk-4.1-dev` (Ubuntu/Debian)

## Quick Start

```bash
# From repo root
cd crates/relift-gui

# Install frontend dependencies
npm install

# Development (hot reload)
npm run tauri dev

# Production build
npm run tauri build
```

## Project Structure

```
crates/relift-gui/
├── src-tauri/           # Tauri 2 backend (Rust)
│   ├── src/
│   │   ├── main.rs      # App entry point + Tauri commands
│   │   ├── commands.rs  # Tauri command handlers
│   │   └── state.rs     # Shared state management
│   ├── tauri.conf.json  # Tauri configuration
│   └── Cargo.toml
├── src/                 # Svelte 5 frontend
│   ├── routes/
│   │   ├── +page.svelte          # Source picker
│   │   ├── scan/+page.svelte     # Live scan progress
│   │   ├── results/+page.svelte  # Filterable results + preview
│   │   └── recover/+page.svelte  # Destination + recovery progress
│   ├── lib/
│   │   ├── components/   # Reusable UI components
│   │   ├── stores.ts     # Svelte stores (bridged from Tauri)
│   │   └── api.ts        # Tauri invoke wrappers
│   ├── app.css           # Tailwind CSS + design tokens
│   └── app.html
├── static/               # Static assets
├── package.json
├── svelte.config.js
├── vite.config.ts
└── tsconfig.json
```

## Design System

Tokens defined in `src/app.css`:
- **Colors**: Dark theme (`#0E1116` bg, `#2DD4BF` teal accent), Light theme
- **Typography**: Inter (UI), JetBrains Mono (mono), 13-14px base
- **Spacing**: 4px base (4, 8, 12, 16, 24, 32, 48)
- **Radius**: 4px/6px/10px
- **Icons**: Lucide (inline SVG)

Anti-slop rules: no emoji, no gradients, no glassmorphism, no stock illustrations.

## Tauri Commands (Rust → Frontend)

Defined in `src-tauri/src/main.rs`:

- `list_devices_cmd()` → `DeviceInfoDto[]`
- `scan_cmd(options)` → `SessionSummaryDto` + emits `scan-progress`
- `cancel_scan_cmd()` → `()`
- `list_candidates_cmd(session)` → `CandidateDto[]`
- `recover_cmd(options)` → `RecoverySummaryDto` + emits `recover-progress`
- `cancel_recover_cmd()` → `()`
- `create_image_cmd(options)` → `ImageResultDto` + emits `image-progress`
- `doctor_cmd()` → `DoctorReportDto`
- `select_directory()` → `string?`
- `get_sessions_dir()` → `string`

Frontend invokes via `@tauri-apps/api/core`: `invoke('command_name', { args })`.

## State Management

Svelte stores in `src/lib/stores.ts` bridge Tauri events:
- `devices` — device list
- `scanProgress` — live progress
- `candidates` — recovered files
- `recoveryProgress` — recovery progress
- `session` — session summary
- `ui` — theme, language, advanced panel visibility

## Adding a New Screen

1. Create `src/routes/new-screen/+page.svelte`
2. Add route to `src/routes/+layout.svelte` if needed
3. Add Tauri command in `src-tauri/src/main.rs` if backend needed
4. Add store in `src/lib/stores.ts` if state needed
4. Update navigation in `src/routes/+layout.svelte`

## Testing

```bash
# Frontend lint/check
npm run check
npm run lint

# Backend tests
cd ../..
cargo test -p relift-gui

# E2E tests (requires running app)
# playwright test (configured in CI)
```

## Packaging

```bash
# Development
npm run tauri dev

# Production (creates installer + portable)
npm run tauri build

# Output in src-tauri/target/release/bundle/
```

## Debugging

- **Rust backend**: `RUST_LOG=debug cargo tauri dev` or attach debugger to `target/debug/relift-gui`
- **Frontend**: DevTools in WebView (Ctrl+Shift+I on Windows/Linux, Cmd+Opt+I on macOS)
- **Tauri logs**: Console output + `RUST_LOG=tauri=debug`

## Common Issues

| Issue | Solution |
|-------|----------|
| "WebView2 not found" | Install WebView2 Runtime (Windows) |
| "libwebkit2gtk not found" | `sudo apt install libwebkit2gtk-4.1-dev` |
| Frontend not hot-reloading | Check `npm run dev` running, port 1420 |
| Tauri commands not found | Rebuild: `cargo build --manifest-path src-tauri/Cargo.toml` |
| Icons not showing | Rebuild: `npm run build` (SVGs inlined at compile time) |

## Performance Notes

- Virtualized table (TanStack Table) handles 500k+ rows
- Preview generated lazily in worker pool
- Progress events debounced at 30 Hz
- Stores use `$derived` for computed values (no unnecessary reactivity)