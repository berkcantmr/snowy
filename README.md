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

## Updates

Snowy checks GitHub Releases for a newer version about once a day (and shortly after
start). When one is available the tray menu offers "Update now" and "Skip this version";
"Update now" opens Settings > About, which downloads the installer with a progress bar,
verifies its SHA-256 checksum against the one published with the release, then runs the
installer silently. The installer closes Snowy, replaces it and starts it again. You can also
use "Check now" on the About page. Turn the daily check off under Settings > System >
"Check for updates".

Only `https` downloads from github.com are accepted, the installer is capped at 60 MB, and a
file whose checksum does not match is deleted and never run.

Maintainers: `powershell -ExecutionPolicy Bypass -File installer
elease.ps1` builds
everything, writes `dist\Snowy-<version>-setup.exe.sha256` and prints the `gh release create`
command to publish (add `-Publish` to run it). A release needs the assets
`Snowy-<version>-setup.exe` and `Snowy-<version>-setup.exe.sha256`, and a tag `v<version>`.

## Privacy

Snowy works fully offline by default. Two features use the network. "Match my weather"
(opt-in) makes an IP-based rough location lookup and then queries
[Open-Meteo](https://open-meteo.com/). The update check (on by default, can be turned off)
asks GitHub's public API for the latest release of this repository; the request carries only
the app name and version (`Snowy/<version>`) as User-Agent, plus the IP address any web
request has. The local `update.json` keeps the last check time and version numbers only.
Nothing else is sent, and there is no telemetry.

## License

Snowy is **source-available** under the [PolyForm Noncommercial License 1.0.0](LICENSE).

- ✅ Use it for personal, hobby, educational or other noncommercial purposes.
- ✅ Change it and share it (including your changes) for free, keeping the license and the
  `Required Notice` line.
- ❌ No selling it and no commercial use of any kind (including bundling it in a paid product
  or using it in a business).

For commercial licensing, open an issue on this repository.

Versions published before this change (up to commit `ed52214`, including the v0.1.0
release assets) were released under the MIT License, and those copies stay under MIT.
