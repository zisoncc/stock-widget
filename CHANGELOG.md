# Changelog

All notable changes to this project will be documented in this file.

## v0.1.1 - 2026-06-27

### Fixed

- Fixed corrupted Chinese text in the right-click menu and dialogs.
- Fixed stock removal menu command IDs so deleting items no longer triggers refresh, exit, or opacity commands.
- Fixed Win32 window class name lifetimes during class registration and window creation.
- Fixed the first-run config save path so `stock_widget_config.json` is consistently stored next to the executable.
- Fixed an input dialog parameter leak after the dialog closes.
- Removed invalid main-window keyboard/focus handling that could treat the main window state pointer as input-dialog data.

### Changed

- Confirmed the quote refresh flow runs once at startup and then every 1 second.
- Updated quote rendering to show price, signed change, and signed percentage.
- Render upward moves in red and downward moves in green.
- Updated documentation to match the Tencent quote API and current app behavior.

## v0.1.0 - 2026-06-24

### Added

- Initial Windows floating stock widget.
- Added configurable stock symbols, window position persistence, opacity adjustment, and Tencent quote fetching.
