## Pull Request Checklist

Before submitting, please ensure all items are checked:

### Code Quality
- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo deny check` passes
- [ ] No `unwrap()`/`expect()` on external input in library code
- [ ] No `TODO`/`FIXME`/`XXX` in shipped code
- [ ] No commented-out code blocks
- [ ] No `dbg!`/`println!` debugging left in code

### Testing
- [ ] Unit tests added for new functionality
- [ ] Integration tests added for new parsers/carvers
- [ ] Fixture tests added for new filesystem support
- [ ] Fuzz target added for new parsers/carvers
- [ ] CLI examples in README/docs tested (`cargo test -p relift-cli -- examples`)

### Documentation
- [ ] `CHANGELOG.md` updated (Keep a Changelog format)
- [ ] Public API documented with `///` comments
- [ ] User-facing docs updated (`docs/CLI.md`, `docs/GUI.md`, `docs/filesystems.md`, etc.)
- [ ] `--help` text updated for new CLI flags
- [ ] Man page updated (if CLI changed)

### Architecture
- [ ] No new dependencies without ADR justification
- [ ] Read-only invariant maintained (no writes to `ReadOnlySource`)
- [ ] Destination safety checks in place
- [ ] Confidence scoring follows established patterns
- [ ] Session persistence works correctly

### GUI (if applicable)
- [ ] Follows design tokens (colors, spacing, typography)
- [ ] No emoji in UI
- [ ] No gradient heroes, glassmorphism, purple-blue AI gradients
- [ ] Accessible: keyboard navigation, ARIA labels, focus visible
- [ ] Dark/Light themes work
- [ ] Responsive at 1366×768 minimum
- [ ] Virtualized lists for large datasets

### Security
- [ ] No network calls in core engine
- [ ] Path sanitization for recovered filenames
- [ ] Destination safety validated
- [ ] Fuzzing target added for new parsers

### Commits
- [ ] Conventional Commits format (`feat:`, `fix:`, `docs:`, etc.)
- [ ] Commit messages explain *why*, not just *what*
- [ ] No "Generated with Claude Code" / "Co-Authored-By: Claude" footers
- [ ] Signed commits (GPG/SSH)

### Reviewer Guidelines
- [ ] Code compiles and tests pass
- [ ] Architecture matches existing patterns
- [ ] Error handling is consistent
- [ ] Performance impact considered
- [ ] No breaking changes without version bump

---

**Description of Changes**

<!-- Describe what this PR does and why -->

**Related Issues**

<!-- Link related issues: Fixes #123, Related to #456 -->

**Testing Performed**

<!-- Describe how you tested this change -->

**Screenshots (GUI changes)**

<!-- Add screenshots if UI changed -->

**Checklist Confirmed By**: @your-github-username