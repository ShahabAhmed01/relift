# Relift application icons

All desktop, Microsoft Store, Android, and iOS icons are generated from the
canonical brand mark at `docs/assets/brand/relift-mark.svg`. The SVG uses the
product palette and the disk-stack/file-recovery symbol; generated binaries in
this folder are the assets consumed by Tauri packaging.

Regenerate after editing the source mark from `crates/relift-gui`:

```sh
npx tauri icon ../../docs/assets/brand/relift-mark.svg -o src-tauri/icons
```

`static/favicon.svg` and `static/brand/relift-mark.svg` use the same source.
The README social preview is maintained as SVG at `docs/assets/social-preview.svg`
and rendered to PNG for GitHub.
