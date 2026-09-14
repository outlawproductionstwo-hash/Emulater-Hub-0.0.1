# Changelog

## [0.0.5-alpha1] - 2026-09-14

### Added

- Added an **Upscaling** tab above Settings.
- Bundled the portable Magpie companion with the Windows installer, including
  its executable, required Windows runtime files, effects, and license text.
- Added controls to start and stop Magpie from Emulator Hub.
- Added an option to start Magpie automatically when launching a ROM.
- Added in-app Magpie profile controls for FSR, Anime4K, CAS, xBRZ, Lanczos,
  and Nearest scaling modes.
- Added a configurable scale multiplier and automatic `ScalingModes.json`
  profile generation.
- Added an in-app sharpening control for FSR and CAS profiles.
- Added automatic scaling after ROM launch by focusing the emulator window and
  sending the configured Magpie hotkey.
- Persisted the Magpie integration preferences in the SQLite app state.
- Added setup guidance explaining the Magpie companion workflow, hotkey
  configuration, automatic-start behavior, and where bundled Magpie files are
  installed.

### Attribution and licensing

- Thank you to the Magpie project and its contributors for providing the
  open-source Windows upscaling companion used by this alpha.
- Emulator Hub remains licensed under the MIT License.
- Bundled Magpie files remain under Magpie's GNU General Public License
  version 3 (GPLv3). The complete Magpie license is included with the
  installer at `tools\Magpie\LICENSE.txt`.
- Magpie runs as a separate companion process; it is not linked into the
  Emulator Hub executable.

### Setup requirements

- Windows 10 or later with a DirectX-capable graphics system.
- Install the alpha using the provided per-user installer so Magpie is placed
  under `%LOCALAPPDATA%\Programs\EmulatorHub\tools\Magpie`.
- Open **Upscaling**, choose a profile, apply it, and configure the matching
  Magpie scaling hotkey.
- Enable automatic Magpie startup or start it manually before launching ROMs.
- The standalone executable does not include the bundled Magpie files; use the
  installer for the complete integrated setup.

## [0.0.4.2] - 2026-09-14

### Changed

- Reorganized the main library screen so selected ROM details and emulator
  controls appear higher and align more naturally with the ROM library.
- Anchored the Home, Go Up, and Refresh Library controls at the bottom of the
  main content area.
- Grouped import controls and library statistics with the ROM library column
  for a cleaner dashboard layout.

## [0.0.4.1] - 2026-09-14

### Added

- Added automatic ROM artwork lookup through TheGamesDB using the ROM title.
- Added a Settings field for the user's TheGamesDB API key.
- Added artwork caching under `%LOCALAPPDATA%\EmulatorHub\artwork` so
  downloaded artwork is reused between launches.
- Added local artwork discovery beside ROM files and manual artwork selection
  from the ROM details panel.
- Added Windows executable icon extraction for imported emulators, with the
  existing initials tile retained as a fallback.
- Added cleanup for helper executables that are not emulators, including SDL
  launchers and uninstaller files.

### Fixed

- Fixed the favorite button displaying corrupted symbols.
- Fixed existing ROM and emulator records remaining assigned to `Unknown`
  when their platform can be inferred.
- Fixed known PlayStation 2 ISO titles such as `God Hand` being misidentified
  as GameCube games.
- Added platform repair for archived Pokemon GBA ROM names such as Emerald,
  Ruby, Sapphire, FireRed, and LeafGreen.
- Prevented SDL/SDL2 variants such as `mgba-sdl` from appearing as emulators
  after an emulator-folder import.

## [0.0.4] - 2026-09-14

### Added

- Added styled icon tiles beside ROMs using platform initials.
- Added styled icon tiles beside emulators using emulator initials.
- Added colored platform and emulator accents with missing-file indicators.
- Added the first library visuals for quickly distinguishing ROM and emulator
  entries.

## [0.0.3.2] - 2026-09-14

### Fixed

- Fixed ROM launches in mGBA where choosing windowed mode could still start the
  game in fullscreen because mGBA reused its previously saved display state.
- ROM launch settings now explicitly enforce the selected fullscreen or
  windowed mode every time a ROM is started, so switching between those modes
  works reliably from the Emulator Hub settings.
- Launch options are now placed before the ROM path, improving compatibility
  with emulator command-line parsers that expect options before the file name.

## [0.0.3.1] - 2026-09-14

### Fixed

- Fixed the home-screen emulator **Configure** action loading default launch
  settings instead of that emulator's saved profile.
- Fullscreen, borderless, and resolution settings now apply correctly after
  configuring an emulator from the home screen.

## [0.0.3] - 2026-09-14

### Added

- Modern dark dashboard UI inspired by the new Emulator Hub design.
- Sidebar navigation for the library, favorites, and settings.
- Library search and favorite filtering.
- ROM cards with launch actions, status indicators, and details.
- Emulator summary cards and library statistics.

## [0.0.2] - 2026-09-14

### Added

- Per-user Windows installer built with Inno Setup.
- Start Menu and desktop shortcuts.
- Microsoft Visual C++ runtime bootstrap in the installer.
- Installer and standalone executable SHA-256 release checksums.

## [0.0.1] - 2026-09-14

Initial public release.

### Added

- Persistent SQLite library for emulators, ROMs, platforms, and metadata.
- Emulator and ROM file/folder import.
- Per-emulator launch settings.
- Favorites, play count, and last-played tracking.
- Missing-file detection.
- GitHub Release update checking and SHA-256 verified installation.
- Windows release workflow and versioned application title.
