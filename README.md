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

Maintainers: `powershell -ExecutionPolicy Bypass -File installer\release.ps1` builds everything, writes `dist\Snowy-<version>-setup.exe.sha256` and prints the `gh release create`
command to publish (add `-Publish` to run it). A release needs the assets
`Snowy-<version>-setup.exe` and `Snowy-<version>-setup.exe.sha256`, and a tag `v<version>`.

## Troubleshooting

If no weather or desktop objects appear, hover the tray icon: it says when Snowy is paused
on battery or while a fullscreen app is open (both can be turned off under Settings > System),
or when it could not start drawing at all. Snowy writes a short log to
`%APPDATA%\Snowy\snowy.log` (the previous run's is `snowy.old.log`) with the monitors, the
graphics adapter it uses and any errors; it contains no personal data, so you can attach it
to a bug report. If the GPU path fails, Snowy falls back to Windows' software renderer (WARP)
by itself; setting the environment variable `SNOWY_RENDERER=warp` forces that.

On laptops with two GPUs Snowy draws on the power-saving one (usually the integrated
graphics), so the discrete GPU can stay asleep. `SNOWY_GPU=performance` picks the fast one.

## Privacy

Snowy has no accounts and no telemetry. It uses the network for two things only:

- **Update check** (on by default; turn it off under Settings > System > "Check for
  updates"): about once a day it asks GitHub's public API for the latest release of this
  repository. The request carries only the app name and version (`Snowy/<version>`) as
  User-Agent, plus the IP address any web request has. The local `update.json` keeps the last
  check time and version numbers only.
- **Match my weather** (off unless you pick it): an IP-based rough location lookup, then a
  forecast query to [Open-Meteo](https://open-meteo.com/). The location is not stored; the
  cached `weather.json` holds only the weather.

Nothing else leaves your computer. The installer shows this notice before installing.

## Code signing policy

Free code signing provided by [SignPath.io](https://signpath.io), certificate by
[SignPath Foundation](https://signpath.org).

Release binaries are built from this repository by the
[release workflow](.github/workflows/release.yml) on GitHub Actions and signed through
SignPath; every signing request is approved by hand.

Team roles (this is a one-person project):

- Committers and reviewers: [berkcantmr](https://github.com/berkcantmr)
- Approvers: [berkcantmr](https://github.com/berkcantmr)

Privacy: see [Privacy](#privacy) above.

## License

Snowy is open source under the [MIT License](LICENSE).

Releases v0.1.1 to v0.1.3 were published under the PolyForm Noncommercial License 1.0.0;
those copies keep that license. Everything from v0.1.4 on, and v0.1.0, is MIT.
