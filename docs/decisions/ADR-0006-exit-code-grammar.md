# ADR-0006: CLI exit-code grammar

## Status

Accepted.

## Context

The CLI needs stable, documented exit codes so scripts can distinguish success
from usage errors, safety refusals, source mismatches, and partial results.

## Decision

| Code | Name | Meaning |
|------|------|---------|
| 0 | OK | Command completed successfully. |
| 1 | RUNTIME | Generic runtime error. |
| 2 | USAGE | Command-line usage error. |
| 3 | UNSAFE | Destination safety refusal (destination overlaps evidence). |
| 4 | MISMATCH | Source does not match the session (re-scan required). |
| 5 | PARTIAL | Completed but results are partial (failed/partial/cancelled). |

A `PARTIAL` exit still produced valid output; inspect summaries for details.

## Consequences

- Scripts can branch on exit code reliably.
- `relift recover` returns 5 when any candidate failed or was partial, even
  though other candidates recovered fine.
- Documented in `docs/CLI.md`.

## Related

- `crates/relift-cli/src/main.rs`
