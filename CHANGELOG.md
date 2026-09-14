# Changelog

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
