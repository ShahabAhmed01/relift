# Security policy

## Release support

The v0.1.0 GitHub release is a draft, not a published stable release. There is no formal security-support window. Check the repository for current status before use.

## Report a vulnerability

Do not post exploitable details in a public issue. Submit a private report through GitHub Security Advisories if private reporting is enabled for the repository. If it is unavailable, contact the repository owner using the contact options on the GitHub profile and ask for a private channel.

Include the affected version or commit, operating system, reproduction steps, impact, and any safe proof of concept. Do not include personal data or disk images containing private information.

No response-time or coordinated-disclosure deadline is guaranteed.

## Security boundaries

Relift parses untrusted filesystem metadata and file structures. The source abstraction opens evidence read-only, and recovery checks that the destination is separate from the source device. These controls have synthetic test coverage, but no physical drive or representative real-media corpus has been validated.

Release artifacts are unsigned. See docs/threat-model.md, docs/GAP_REPORT.md, and docs/VERIFICATION.md for the implementation controls and remaining gaps.

Do not test against a person's physical device without their explicit confirmation of the source and destination.
