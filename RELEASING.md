# Releasing Emulator Hub

The updater checks the latest stable GitHub Release for a newer Windows
executable and verifies its SHA-256 checksum before replacing the running app.

## First-time setup

1. Put this project in a GitHub repository.
2. Ensure the repository can run GitHub Actions.
3. Do not change the release asset naming convention in
   `.github/workflows/release.yml`.

Release builds automatically receive the repository name through
`EMULATOR_HUB_GITHUB_REPOSITORY`. Local builds will show an update-configuration
error until that environment variable is set.

## Publish version 0.0.5 alpha 1

This alpha bundles Magpie with the installer and adds the Upscaling tab.

```powershell
git add .
git commit -m "Release Emulator Hub 0.0.5-alpha1"
git tag v0.0.5-alpha1
git push origin main --tags
```

The alpha release is marked as a prerelease by GitHub Actions because the tag
contains a hyphen. The in-app updater intentionally ignores prereleases. The
version comparison also treats a later stable release such as `0.0.5` as newer
than `0.0.5-alpha1`.

Magpie is bundled under its own GPLv3 license. The installer includes the
Magpie binaries, effects, and `LICENSE.txt` under
`tools\Magpie\LICENSE.txt`. Thank you to the Magpie project and contributors.

## Publish version 0.0.4.2 hotfix

From the repository root:

```powershell
git add .
git commit -m "Release Emulator Hub 0.0.4.2 hotfix"
git tag v0.0.4.2
git push origin main --tags
```

The workflow builds the standalone Windows executable, creates the Windows
installer, creates a SHA-256 checksum for both files, and publishes all four
files as GitHub Release assets.

The installer is per-user and installs to
`%LOCALAPPDATA%\Programs\EmulatorHub`, so users do not need administrator
permission. The library database is stored separately at
`%LOCALAPPDATA%\EmulatorHub\library.sqlite3` and is preserved when the app is
uninstalled.

Version 0.0.4.2 is a hotfix for the cleaned-up main library layout.

## Publish a later update

1. Update the display version in `src/main.rs` and the changelog, for example
   to `0.0.6`.
2. Commit the change.
3. Create and push the matching tag:

```powershell
git tag v0.0.6
git push origin main --tags
```

Users can open **Settings -> Check for Updates** in Emulator Hub. If a newer
release is found, the app downloads, verifies, installs, and restarts itself.

## Build the installer locally

Install Inno Setup 6, then run:

```powershell
cargo build --locked --release
.\installer\build.ps1 -Version 0.0.5-alpha1
```

The script downloads `vc_redist.x64.exe` into the untracked `installer`
directory and creates the installer and checksum in `release`. Before each
build, older local release executables and installers are automatically
compressed into timestamped ZIP files under `release\archive`.
