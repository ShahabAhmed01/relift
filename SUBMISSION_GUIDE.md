# Package Manager Submissions

This document describes how to submit Relift to each package manager. All
packaging manifests are in `packaging/` and have been validated with real
checksums from the `v0.1.0` GitHub release assets.

## Release Assets

| Asset | SHA-256 |
|-------|---------|
| `relift-linux-x86_64.tar.gz` | `61b2df5b38dbdc9c97ecf353b26e2d44693eabbd2f0422bd06e51b95d78dd5a1` |
| `relift-linux-aarch64.tar.gz` | `11b17d4367947c34083b1416f8702837262a85684d1880251530eb746a8ff9c9` |
| `relift-windows-x86_64.zip` | `b35b9f72581ca90e5932689d2753e7d24a4bff9b3cffe8fd75945b1bbb0d78bd` |
| `relift-macos-x86_64.tar.gz` | `44297d319ea951deb8f857566c0545048046eeb8577e1178bdfb240f97cc393c` |
| `relift-macos-aarch64.tar.gz` | `aa4b5c0746bf7e28e311c26501f5c11d22fba982af1b1002343fe8d21a9c181f` |
| `Relift_0.1.0_x64_en-US.msi` | `615b0f0c5e72d03ee1cf916861e90baea74cd070b968b81b8e87f3ff9ed399ae` |
| `Relift_0.1.0_amd64.deb` | `0af3196f8ab4de4390232387e83709687938d8e4b962fcb0ab0e32100531a9ec` |
| `Relift-0.1.0-1.x86_64.rpm` | `5a37f4054bb17b9112e39c8ac638d787c5201892e83fc6aa74fee0c4b0dc25da` |
| `Relift_0.1.0_amd64.AppImage` | `b8d5df945f00f8355fab55a461e14694fc5445e3fe6948dee9f5442802d29b8b` |
| `Relift_0.1.0_universal.dmg` | `a4b1f931312fa0f3b8bc5126344e7ef61d03c2a4f36cb7e95917c10cb85a9a4e` |

## 1. Homebrew (homebrew-core)

**File:** `packaging/homebrew/relift.rb`

**Submit:** Create a PR to https://github.com/Homebrew/homebrew-core

```sh
# Fork Homebrew/homebrew-core, clone, then:
mkdir -p Formula
cp packaging/homebrew/relift.rb Formula/relift.rb
git add Formula/relift.rb
git commit -m "relift 0.1.0"
git push
# Open PR at https://github.com/Homebrew/homebrew-core/compare
```

**Validation:** `brew audit --strict Formula/relift.rb` (on macOS/Linux with Homebrew)

## 2. Scoop (ScoopInstaller/Scoop)

**File:** `packaging/scoop/relift.json`

**Submit:** Create a PR to https://github.com/ScoopInstaller/Scoop

```sh
# Fork ScoopInstaller/Scoop, clone, then:
mkdir -p bucket
cp packaging/scoop/relift.json bucket/relift.json
git add bucket/relift.json
git commit -m "relift 0.1.0"
git push
# Open PR at https://github.com/ScoopInstaller/Scoop/compare
```

**Validation:** The JSON is syntactically valid. Test locally with:
```sh
scoop install packaging/scoop/relift.json
```

## 3. winget (microsoft/winget-pkgs)

**File:** `packaging/winget/relift.vestige.yaml`

**Status:** ✅ Validated with `winget validate`

**Submit:** Create a PR to https://github.com/microsoft/winget-pkgs

```sh
# Fork microsoft/winget-pkgs, clone, then:
mkdir -p manifests/r/relift/vestige/0.1.0
cp packaging/winget/relift.vestige.yaml manifests/r/relift/vestige/0.1.0/relift.vestige.yaml
git add manifests/r/relift/vestige/0.1.0/relift.vestige.yaml
git commit -m "relift 0.1.0"
git push
# Open PR at https://github.com/microsoft/winget-pkgs/compare
```

**Validation:** `winget validate packaging/winget/relift.vestige.yaml` — PASSED

## 4. AUR (Arch User Repository)

**Files:** `packaging/aur/PKGBUILD` (source), `packaging/aur/PKGBUILD-bin` (binary)

**Submit:** Create a package on https://aur.archlinux.org

```sh
# Source build:
git clone https://aur.archlinux.org/relift.git
cp packaging/aur/PKGBUILD relift/PKGBUILD
cd relift
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "relift 0.1.0"
git push

# Binary package:
git clone https://aur.archlinux.org/relift-bin.git
cp packaging/aur/PKGBUILD-bin relift-bin/PKGBUILD
cd relift-bin
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "relift-bin 0.1.0"
git push
```

## 5. Flatpak (Flathub)

**File:** `packaging/flatpak/com.github.ShahabAhmed01.relift.yml`

**Submit:** Create a PR to https://github.com/flathub/flathub

```sh
# Fork flathub/flathub, clone, then:
mkdir -p com.github.ShahabAhmed01.relift
cp packaging/flatpak/com.github.ShahabAhmed01.relift.yml com.github.ShahabAhmed01.relift/com.github.ShahabAhmed01.relift.yml
git add com.github.ShahabAhmed01.relift/com.github.ShahabAhmed01.relift.yml
git commit -m "Add relift 0.1.0"
git push
# Open PR at https://github.com/flathub/flathub/compare
```

**Validation:** `flatpak-builder --require-changes --repo=repo --install-deps-from=flathub packaging/flatpak/com.github.ShahabAhmed01.relift.yml`

## 6. Snap (Snapcraft)

**File:** `packaging/snap/snapcraft.yaml`

**Submit:** Push to https://snapcraft.io

```sh
snapcraft login
snapcraft pack packaging/snap/snapcraft.yaml
snapcraft upload relift_0.1.0_amd64.snap --release=stable
```

## Checklist

- [x] Release `v0.1.0` tagged and pushed
- [x] GitHub Release workflow completed (CLI archives + GUI bundles)
- [x] All packaging manifests updated with real SHA-256 checksums
- [x] Winget manifest validated with `winget validate`
- [ ] Homebrew PR submitted
- [ ] Scoop PR submitted
- [ ] winget PR submitted
- [ ] AUR packages submitted
- [ ] Flatpak PR submitted
- [ ] Snap published
