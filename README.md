# Snowy

Snowy is an original desktop-weather app for Windows, written in Rust. Snow, rain,
fog and more drift across your desktop, with a snowman, a tree and a snow plough
living on the taskbar edge.

## Features

- Animated weather effects rendered over the desktop (snow, rain, fog, and others)
- Decorative props: snowman, tree, snow plough
- Tray icon with quick controls
- Settings window and editor (same executable, launched with flags)
- Optional "Match my weather" mode that mirrors your current local weather
- Per-monitor-v2 DPI aware

## Build

Requires Rust (stable, MSVC toolchain) and the Windows SDK.

    cargo build --release

The executable is `target\release\snowy.exe`. To build the installer and a portable zip:

    powershell -ExecutionPolicy Bypass -File installer\build.ps1

Output goes to `dist\`. The installer needs [Inno Setup 6](https://jrsoftware.org/isinfo.php)
(`winget install JRSoftware.InnoSetup`); without it only the portable zip is produced.

The app icon is generated procedurally:

    cargo run --manifest-path assets/gen/Cargo.toml --release

## Data

Settings and state are stored in `%APPDATA%\Snowy`. Uninstalling removes the program
and its autostart entry but leaves that folder in place.

## Privacy

Snowy works fully offline by default. Only the opt-in "Match my weather" feature uses
the network: it makes an IP-based rough location lookup and then queries
[Open-Meteo](https://open-meteo.com/). Nothing else is sent, and there is no telemetry.

## License

MIT
