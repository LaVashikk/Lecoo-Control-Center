# Changelog

## [0.4.1] - 2026-07-15

### Added
- Added keyboard backlight support for the Lecoo N161A (IT5570-02) chassis. Note: battery charge limits and fan control for N161A are not yet supported.

## [0.4.0] - 2026-05-06

### Added
- Added insecure mode for daemon operation.
- Implemented flexible keyboard backlight control with custom brightness values.
- Added comprehensive i18n support for all CLI arguments (English and Russian).
- Added charge indicator status control logic.
- Added Simplified Chinese README documentation.

### Changed
- Improved help messages and output for the `charge` CLI command.
- Updated telemetry with additional system information.
- Updated README with installation instructions.

### Fixed
- Fixed EC state preservation during system Suspend/S4 (hibernation).
- Fixed PWM mux and bypass timer for custom keyboard backlight control.
- Fixed charge limit state being overridden on unknown values.

### Refactored
- Introduced atomic batching for EC I/O operations for better performance and safety.
- Centralized EC offsets into board-specific profiles.
- Simplified IPC protocol by removing `IpcResponse::Message`.

## [0.3.3] - 2026-03-23

### Added
- Added a new `monitoring` CLI command for real-time, continuous tracking of CPU/System temperatures and fan speeds.
- Added a `both` target option to the `fan` CLI command, allowing users to control CPU and GPU fans simultaneously (e.g., `fan both auto`).

### Changed
- Updated the Windows daemon initialization logic to explicitly require a `--service` flag and introduced a 3-second startup delay to improve service stability.

### Fixed
- Ensured that panic logs in the Windows service are explicitly flushed to disk before telemetry is sent, preventing log data loss during a crash.


## [0.3.2] - 2026-03-21

### Changed
- The daemon now completely restores all device settings (including battery charge limits) upon system wake-up, whereas previously only the LED mode was restored.
- The Linux installation script (`install.sh`) now features extensive pre-installation safety checks, including `/dev/port` accessibility validation and virtualization detection.
- The Linux uninstallation script (`uninstall.sh`) now interactively prompts the user before deleting saved daemon configuration data.

### Fixed
- Fixed an issue where battery charge limits were not being saved to the configuration state during system shutdown or hibernation.
- Fixed Linux daemon failing to properly detect and handle system hibernation (S4) by implementing `systemd` job monitoring alongside `logind`.
- Fixed Windows daemon power event handling to properly map Suspend events to the new hibernation logic, ensuring state is saved correctly on Windows platforms.


## [0.3.1] - 2026-03-20

### Added
- Battery charge limits are now actively applied when loading daemon settings.
- Added `RpcSs` as a dependency in the Windows installation script (`install.bat`) to ensure more reliable service startup.

### Changed
- CPU and OS information on Windows is now fetched directly from the Windows Registry instead of using `raw_cpuid`.
- Refactored the Windows daemon startup flow to explicitly wait for the OS service initialization to complete before binding the IPC server and initializing EC communication, preventing potential race conditions.

### Fixed
- Fixed a bug in EC HRAM window base address detection by correctly treating `0xFF` (instead of `0`) as the uninitialized offset.
- Prevented the daemon from crashing (panicking) during system shutdown if reading the keyboard backlight state fails.
- Ensure the active power profile is correctly read and saved alongside the keyboard backlight state during system shutdown.
- Fixed the Windows uninstaller script (`uninstall.bat`) attempting to remove the wrong service name (`LecooControlCenter` instead of `LecooControlDaemon`).
