# Changelog

## [0.6.0] - 2026-11-14

First stable release since 0.4.1. The 0.5.x versions were refactoring betas and are not listed separately: everything below is relative to 0.4.1. Regressions that only ever existed in the betas are listed in their own section.

### Added
- **Board profiles.** Each supported board (N155A, N155C, N155D, N161A) has its own register map and feature set. The daemon reports these capabilities to the CLI, which adapts its help and arguments. Features a board does not have are rejected with a clear message instead of writing to registers that mean something else there.
- **N161A battery charge control**, based on @YuchuLiao's reverse engineering: `charge lifespan` arms the firmware's own battery protection (keeps the battery at about 61-76%), `charge hold` stops charging at the current level (works between 62% and 84%).
- `battery`: charging state, power draw, time left or time until the charge limit, voltage per cell, remaining and full capacity (mAh and Wh), health, cycle count and the active charge limit. Available on N155A, N155C and N161A.
- `charge` reworked:
  - Without arguments it shows the requested mode, what the EC is actually doing, the battery level and why a request is still pending.
  - Custom ranges with `charge --min <N> --max <N>` on N155 boards (20-100%).
  - Presets are now per board: `full`, `high`, `balanced`, `lifespan`, `desk` on N155 boards; `full`, `lifespan`, `hold` on N161A.
  - A mode that cannot be applied right now (N161A preconditions) is remembered and applied automatically once possible: on AC connect, after wake, or within a minute.
- LED ring:
  - `led breath <preset>`: hardware breathing animations with 11 presets (smooth, sleep, alert, zen, ping, energetic, warning, vacuum, panic, sonar, toxic).
  - `led sleep <auto|custom N|breath preset>`: what the ring shows while the laptop sleeps (default: the `sleep` animation).
  - `led flash [preset] [--for <seconds>]`: plays an effect once, then returns to the saved mode. Meant for scripts and notifications. Boards without the breathing engine blink at full brightness instead.
- `daemon overheat [on|off|<°C>]`: flashes the LED ring while the CPU is above the threshold (`on` = 95 °C, the alarm turns off 5 °C below it). Without an argument it shows the current setting.
- `fan <target> turbo`: maximum fan duty (220), with a warning about fan wear.
- `hwtest`: interactive hardware test. Cycles the keyboard backlight, fans and LED ring and asks what you observed, verifies power profiles by reading them back, then restores your settings (also on Ctrl+C).
- Daemon-side metrics: the daemon samples temperatures and fan speeds itself. `monitoring` reads these samples, and the daemon speeds up to the rate a client asks for (down to 1 s) while someone is watching, then drops back to once a minute.
- Daemon flags `--profile <id>` and `--dump-profile` (prints the resolved profile, register addresses and live values; read-only).
- CLI: a "Hardware Capabilities" section in `help` with the board's charge presets and power profiles, "did you mean" suggestions for mistyped commands, case-insensitive arguments, `perf` as an alias for `performance`.
- Hidden developer commands for reverse engineering: `read <addr> [count]` dumps raw EC bytes, `write <addr> <val>` is only honored by daemons built with the `unsafe_write_using_ipc` feature.

### Changed
- Boards that are not on the supported list are no longer driven with the N155A register map. A profile can be forced with `--profile <id>`, and `--insecure` on such a board needs it too.
- `fan <target> full` now runs the fans at duty 165 instead of 150.
- Restoring settings on startup and wake now goes feature by feature: one failing feature no longer stops the others from being restored, and features the board does not have are skipped.
- The charge mode is owned by the daemon and is no longer overwritten at sleep or shutdown by whatever the EC reports. Keyboard backlight and power profile are still read back, since hotkeys can change them.
- The state file is written atomically (temp file + rename), so a crash during a save no longer leaves a broken config behind.
- Linux: the daemon logs to the terminal when it is not started by systemd (handy for manual runs and `--dump-profile`).
- Windows: the daemon starts the `inpoutx64` driver service itself if it is stopped (#19, thanks @C0ntrolDev).
- Telemetry (still optional: `lecoo-ctrl daemon telemetry disable`) uses a new JSON format and endpoint. In addition to what 0.4.x sent, it includes the resolved board profile and feature set, host vendor and product, BIOS version, CPU architecture, the applied settings, battery level and daemon uptime in periodic status reports, daemon warnings and errors (rate-limited), a per-run session id, and the board name and EC chip on unsupported boards. Events that fail to send are kept (up to 32) and retried.

### Fixed
- N161A: the daemon no longer writes N155A fan, LED ring and battery indicator registers. They had no effect or hit unrelated hardware: the N155A orange-LED bit is the keyboard backlight pin on N161A.
- Battery indicator: plugging in the charger in `full` mode lit the white "charged" LED at any battery level.

### Fixed in the 0.5.x betas
- 0.5.0-beta wrote N155 fan duty to the wrong RAM addresses.
- On unsupported boards a daemon thread panicked every time a client disconnected.
- The saved charge mode was overwritten at sleep and shutdown by the decoded EC state. On N155A this could save `Freeze`, which the board does not support, and restoring the charge mode then failed on every boot.
- N161A `lifespan` never armed ("Cannot confirm active charging").
- A client with a mismatched IPC protocol got a dropped connection ("failed to fill whole buffer") instead of a version mismatch message.
- Fatal EC initialization errors were not logged.

### Refactored
- New `lecoo-types` crate with the types shared by the daemon, the CLI and the upcoming GUI.
- IPC and the daemon state file use JSON instead of bincode. IPC messages are length-prefixed JSON frames with typed connections for both sides, structured errors (`Internal`, `UnsupportedRequest`, `UnsupportedHardware`, `Precondition`). Unknown fields are ignored on both sides.
- `EcOffsets` replaced by board profiles with typed addresses (`Reg`, `Ram`, `Banked`) and per-feature backends (`KbdOps`, `LedOps`, `ChargeOps`). Capabilities are derived from them.
- CLI: clap replaced with a small `lexopt` parser, so help and arguments can follow the board's capabilities. The Chinese locale file is renamed to `zh-CN.yml`.
- Unit tests for board profiles (ID and DMI shadowing, address collisions, capability derivation) and for the JSON wire format.
- Added `rustfmt.toml`, code formatted.

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
