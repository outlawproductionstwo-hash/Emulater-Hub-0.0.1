# Changelog

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
