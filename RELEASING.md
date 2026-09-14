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

## Publish version 0.0.1

From the repository root:

```powershell
git add emulator_hub_gui
git commit -m "Release Emulator Hub 0.0.1"
git tag v0.0.1
git push origin main --tags
```

The workflow builds the Windows executable, creates a SHA-256 checksum file,
and publishes both as GitHub Release assets.

## Publish a later update

1. Change the `version` in `Cargo.toml`, for example to `0.0.2`.
2. Commit the change.
3. Create and push the matching tag:

```powershell
git tag v0.0.2
git push origin main --tags
```

Users can open **Settings → Check for Updates** in Emulator Hub. If a newer
release is found, the app downloads, verifies, installs, and restarts itself.
