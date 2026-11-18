# Snowy — Roadmap

A lightweight desktop-weather app for Windows written in Rust: weather effects over the
desktop, playful desktop props, painted wallpapers. Goal: a rich feature set in a single
small exe, with all code and art original.

## Phase 1 — Core engine (implemented, needs visual check)
- [x] Transparent click-through overlay per monitor (D3D11 + DirectComposition) — `crates/gfx`
- [x] "Behind windows" mode (wallpaper layer, under apps, above wallpaper) — `crates/gfx`
- [x] Effects: snow, heavy snow, rain, fireflies, sakura, autumn leaves, starfall — `crates/effects`
- [x] Tray snowflake menu: pick effect, season, random, off, pause, settings, quit — `crates/sys`
- [x] Pause on battery / in fullscreen apps, start with Windows, multi-monitor — `crates/sys`
- [x] Settings window (separate process, egui), EN/TR — `crates/settings`
- [x] Main loop, settings hot-reload, season/random resolution, changing wind — `app`

## Phase 2 — Desktop interaction (implemented, needs visual check)
- [x] Snow/petals piling up on top edges of windows and the taskbar (EnumWindows rects)
- [x] "Keep windows clear" option; piles slide off when a window is dragged
- [x] Leaves pile at the bottom and scatter on mouse sweep
- [x] Snowballs: roll with mouse, throw, they break on impact

## Perf follow-up
- [x] Profiled (SNOWY_PROFILE=1): update ~35 µs, render ~0.55 ms per frame, ~5% of one core
      at 60 fps with piles. Earlier 22% reading did not reproduce.
- [x] Fixed pacing: 60 fps on 144 Hz ran at ~50 fps; now an accumulating vblank deadline.

## Phase 3 — Playful objects (snowmen, trees, plough implemented; needs visual check)
- [x] Build snowmen from snowballs (stack 2–3); dress-up editor (hat, face, item, outfit × 8)
- [x] Christmas tree: grow, decorate (lights, ornaments, topper, garland × 8), drag; persists
- [x] Desktop string lights (stretchable ends, 8×4 looks) behind windows, persisted
- [x] Snowplough that clears ground snow, balls and leaves (menu action)
- [x] Yeti visitor that tries to carry off a snowman (click it to scare it away)
- [x] Daily gift (parachute box on the main screen; opens into a new snowman)

## Phase 4 — Environment
- [x] Match my weather (Open-Meteo, rough IP-based location, opt-in, every 3 h)
- [x] Day & night wallpapers (6 original painted themes × 7 times of day)
- [x] 9 languages: en, tr, de, es, fr, ja, pt-BR, ru, zh-Hans (CJK fonts loaded on demand)

## Out of scope for now
- Accounts, license server, friends/train sending (needs a backend)
- Telemetry

## Distribution
- [x] App icon, version info, DPI manifest embedded in snowy.exe
- [x] Inno Setup per-user installer (EN/TR) + portable zip: `installer/build.ps1` → `dist/`
- [ ] Code signing, auto-update
