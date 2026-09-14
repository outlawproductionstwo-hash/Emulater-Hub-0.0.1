<p align="center">
  <img src="assets/eframe_icon.png" alt="Emulator Hub icon" width="180">
</p>

<h1 align="center">Emulator Hub</h1>

<p align="center">
  A Windows desktop library for organizing emulators and ROMs in one place.
</p>


Emulator Hub GUI is a Windows desktop library for organizing emulators and
ROMs in one place.

## Version 0.0.1

This is the first public release.

## Features

- Import emulator executables or emulator folders.
- Import ROM files or ROM folders.
- Associate emulators and ROMs with platforms.
- Persist the library between launches using SQLite.
- Store per-emulator fullscreen, borderless, and resolution launch settings.
- Mark ROMs as favorites.
- Track play count and last-played time.
- Detect missing emulator and ROM files without deleting their records.
- Check for updates from GitHub Releases.
- Verify downloaded updates with SHA-256 before installing them.

## Download and run

Download the Windows executable from the GitHub Release assets:

```text
EmulatorHub-v0.0.1-windows-x86_64.exe
```

Run the executable directly. No installer is required for version 0.0.1.

## Using Emulator Hub

1. Enter a platform name, such as `NES`, `SNES`, or `PlayStation`.
2. Import one or more emulator executables.
3. Import ROM files or a ROM folder.
4. Open **Settings** to configure launch arguments.
5. Click a ROM in the library to launch it.

The app keeps imported file paths in the library. ROM files are not copied into
the database.

## Persistent data

The SQLite library is stored at:

```text
%LOCALAPPDATA%\EmulatorHub\library.sqlite3
```

The database stores library records and metadata, while emulator and ROM files
remain in their original locations.

## Building from source

### Requirements

- Windows
- Rust stable toolchain
- A C compiler, required by SQLite's bundled build

Build the application:

```powershell
cd emulator_hub_gui
cargo build --release
```

The executable is created at:

```text
target\release\emulator_hub_gui.exe
```

Run checks with:

```powershell
cargo fmt -- --check
cargo check
cargo test
```

## Updates

The updater checks the latest stable GitHub Release from **Settings → Check
for Updates**. Release builds receive the repository name automatically from
the GitHub Actions workflow.

Local builds need the repository environment variable if update checking is
required:

```powershell
$env:EMULATOR_HUB_GITHUB_REPOSITORY = "OWNER/REPOSITORY"
cargo run
```

## Releasing

Update the version in `Cargo.toml`, commit the change, then create and push a
matching tag:

```powershell
git add .
git commit -m "Release Emulator Hub 0.0.1"
git tag v0.0.1
git push origin main --tags
```

The workflow in `.github/workflows/release.yml` builds the Windows executable
and publishes a SHA-256 checksum alongside it.

See [`RELEASING.md`](RELEASING.md) for more details.

## License

This project is licensed under the [MIT License](LICENSE).
