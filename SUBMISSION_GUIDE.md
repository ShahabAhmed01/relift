# Package manager submission status

No package-manager submission is currently verified or accepted. The files under packaging/ are drafts and must not be treated as installable release manifests.

The public v0.1.0 GitHub release is a draft. Do not submit manifests using guessed URLs, placeholder versions, or checksums. There are no published stable artifacts with verified checksums to cite.

## Before submitting

1. Publish a stable GitHub release with verified, unsigned or signed assets clearly identified.
2. Generate SHA-256 checksums directly from the final assets.
3. Test each manifest using the package manager's current validator and a clean install environment.
4. Submit the manifest to the upstream repository using that project's current contribution process.
5. Record the upstream pull request and validation evidence here.

## Current status

| Ecosystem | Repository manifest | Submission status |
|---|---|---|
| Homebrew | packaging/homebrew/relift.rb | Draft; not submitted |
| Scoop | packaging/scoop/relift.json | Draft; not submitted |
| winget | packaging/winget/relift.vestige.yaml | Draft; not submitted |
| AUR | packaging/aur/PKGBUILD and PKGBUILD-bin | Draft; not submitted |
| Flatpak | packaging/flatpak/com.github.ShahabAhmed01.relift.yml | Draft; not submitted |
| Snap | packaging/snap/snapcraft.yaml | Draft; not submitted |

Do not copy, upload, or publish a draft manifest until its source URL and checksum match a real release asset.
