# Support

## Getting help

### Documentation
- **Quickstart:** `README.md` — 3-command quickstart for CLI and GUI
- **CLI reference:** `docs/CLI.md` — every command, flag, exit code, JSON schema
- **GUI guide:** `docs/GUI.md` — four-screen walkthrough with screenshots
- **Filesystems:** `docs/filesystems.md` — per-FS capabilities and limitations
- **Formats:** `docs/formats.md` — carved formats with validation details
- **FAQ:** `docs/FAQ.md` — common questions answered

### Community
- **GitHub Discussions:** Q&A, ideas, recovery stories — best for general questions
- **GitHub Issues:** Bug reports, false positives/negatives, feature requests (use templates)
- **Discord/Matrix:** Not yet available; Discussions is the primary venue

### Professional support
No commercial support is offered. Relift is a volunteer-maintained open-source project. For critical data recovery cases, consider professional data recovery services.

## Reporting issues

### Before reporting
1. Run `relift doctor` and include the output
2. Check `docs/GAP_REPORT.md` and `docs/limitations.md` — your issue may be a known gap
3. Search existing issues and discussions

### Bug reports
Use the **Bug report** template. Required:
- `relift doctor` output
- OS, filesystem type, source device type (HDD/SSD/USB/SD/image)
- Scan mode used (`quick`, `deep`, `formatted`, `lost-partition`)
- Session directory (if small) or scan log
- Expected vs actual behavior

### False positives / false negatives
Use the **Recovery failure** template. Required:
- Filesystem type and deletion scenario
- Original file details (name, size, format, SHA-256 if known)
- Relift output (candidate ID, confidence, validation notes)
- Ground truth if available

We use these reports to improve carvers and add regression tests.

### Feature requests
Use the **Feature request** template. Explain:
- The use case and why it matters
- Expected behavior
- Any alternatives considered

## Contributing

See `CONTRIBUTING.md` for:
- Development setup (5 commands)
- Architecture overview
- How to add a carver (30-line walkthrough)
- How to add a filesystem parser
- PR checklist

## Security

See `SECURITY.md` for:
- Vulnerability reporting (private GitHub Advisory)
- Threat model
- Signed-release verification
- Hardening checklist

## License

GPL-3.0-or-later. See `LICENSE`. Contributions are dual-licensed under the same terms.

## Citation

If you use Relift in research or forensics work, please cite:

```bibtex
@software{relift,
  title = {Relift: Read-only deleted-file recovery},
  author = {Shahab Ahmed and contributors},
  year = {2024},
  url = {https://github.com/ShahabAhmed01/relift},
  version = {0.1.0}
}
```

See `CITATION.cff` for machine-readable citation.

## Acknowledgments

Relift builds on the work of many open-source projects:
- `serde`, `clap`, `memchr`, `crc32fast`, `flate2`, `sha2`, `thiserror`, `anyhow`
- `rayon`, `nom`, `parking_lot`, `smallvec`, `blake3`, `aho-corasick`, `memmap2`
- `tauri`, `svelte`, `tailwindcss`, `lucide-svelte`
- `criterion`, `proptest`, `insta`, `cargo-fuzz`
- TestDisk/PhotoRec (reference behavior, not code)
- The Sleuth Kit (file system reference implementations)