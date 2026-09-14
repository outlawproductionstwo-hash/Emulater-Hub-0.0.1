<p align="center">
  <img src="assets/eframe_icon.png" alt="Emulator Hub icon" width="180">
</p>

<h1 align="center">Emulator Hub</h1>

<p align="center">
  A Windows desktop library for organizing emulators and ROMs in one place.
</p>

## Version 0.0.5-alpha1

This alpha adds an integrated Upscaling tab with a bundled Magpie companion.
Magpie can be started from Emulator Hub or automatically when a ROM launches.

## Version 0.0.4.2

This hotfix cleans up the main library layout, moving selected ROM details and
emulator controls higher while keeping library navigation at the bottom.

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
- Show platform initials next to ROMs and emulator initials next to emulators.
- Automatically detect common ROM and emulator platforms when the platform is
  left as Unknown.
- Load matching local ROM artwork and allow artwork to be selected manually.
- Optionally download ROM artwork automatically from TheGamesDB using a
  user-provided API key.
- Use the bundled Magpie companion to upscale emulator windows.
- Create and apply Magpie scaling profiles from the Upscaling tab, including
  FSR, Anime4K, CAS, xBRZ, Lanczos, and Nearest.
- Adjust the selected profile's scale multiplier and sharpening amount.
- Automatically focus a newly launched emulator window and trigger Magpie's
  configured scaling shortcut after a short startup delay.

## Download and install

Download the installer from the GitHub Release assets:

```text
EmulatorHub-v0.0.5-alpha1-Setup.exe
```

The installer places the app in:

```text
%LOCALAPPDATA%\Programs\EmulatorHub
```

It creates Start Menu and desktop shortcuts and installs the Microsoft Visual
C++ runtime required by native Windows dependencies. A standalone executable
is also published for portable use.

## Using Emulator Hub

1. Enter a platform name, such as `NES`, `SNES`, or `PlayStation`.
2. Import one or more emulator executables.
3. Import ROM files or a ROM folder.
4. Open **Settings** to configure launch arguments.
5. Open **Upscaling**, choose a scaling profile, and click **Apply Profile**.
6. Start Magpie there or enable automatic startup.
7. Click a ROM in the library to launch it, then use Magpie's configured
   scaling hotkey on the emulator window. To remove that manual step, enable
   automatic scaling in the Upscaling tab and set the matching
   `Alt+Shift+key` there.

The app keeps imported file paths in the library. ROM files are not copied into
the database.

## Persistent data

The SQLite library is stored at:

```text
%LOCALAPPDATA%\EmulatorHub\library.sqlite3
```

The database stores library records and metadata, while emulator and ROM files
remain in their original locations.

Magpie is installed under:

```text
tools\Magpie\
```

It is managed as a background companion process rather than linked into the
Emulator Hub executable. Its license is included at
`tools\Magpie\LICENSE.txt`.

## Building from source

### Requirements

- Windows
- Rust stable toolchain
- A C compiler, required by SQLite's bundled build

Build the application:

```powershell
cd emulator_hub_gui
cargo build --locked --release
```

The executable is created at:

```text
target\release\emulator_hub_gui.exe
```

Run checks with:

```powershell
cargo fmt -- --check
cargo check --locked
cargo test --locked
```

## Updates

The updater checks the latest stable GitHub Release from **Settings -> Check
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
git commit -m "Release Emulator Hub 0.0.5-alpha1"
git tag v0.0.5-alpha1
git push origin main --tags
```

The workflow in `.github/workflows/release.yml` builds and publishes:

- The standalone Windows executable.
- The Windows installer.
- A SHA-256 checksum for each file.

See [`RELEASING.md`](RELEASING.md) for more details.

## Credits

<details>
<summary>Show / hide</summary>

This project uses and bundles the following open-source companion:

| Contributor | Contribution | License |
| --- | --- | --- |
| [Magpie](https://github.com/Blinue/Magpie) project and contributors | Open-source Windows upscaling companion used by Emulator Hub to provide FSR, Anime4K, CAS, xBRZ, Lanczos, and Nearest scaling profiles. | [GNU General Public License version 3 (GPLv3)](installer/Magpie/LICENSE.txt) |

Magpie remains a separate companion process and is not linked into the
Emulator Hub executable. The complete Magpie license is also installed with
the application at `tools\Magpie\LICENSE.txt`.

</details>

## License

Emulator Hub is licensed under the [MIT License](LICENSE).

Magpie is licensed separately under the
[GNU General Public License version 3 (GPLv3)](installer/Magpie/LICENSE.txt).
