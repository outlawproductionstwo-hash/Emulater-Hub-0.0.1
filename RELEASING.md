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

## Publish version 0.0.2

From the repository root:

```powershell
git add .
git commit -m "Release Emulator Hub 0.0.2"
git tag v0.0.2
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

## Publish a later update

1. Change the `version` in `Cargo.toml`, for example to `0.0.3`.
2. Commit the change.
3. Create and push the matching tag:

```powershell
git tag v0.0.3
git push origin main --tags
```

Users can open **Settings -> Check for Updates** in Emulator Hub. If a newer
release is found, the app downloads, verifies, installs, and restarts itself.

## Build the installer locally

Install Inno Setup 6, then run:

```powershell
cargo build --locked --release
.\installer\build.ps1 -Version 0.0.2
```

The script downloads `vc_redist.x64.exe` into the untracked `installer`
directory and creates the installer and checksum in `release`.
