# Desktopia

A living desktop wallpaper for Windows. It shows **Vesper**, an alien world spread across two 4K monitors, where creatures and plants evolve, learn, form societies and die with no scripted behavior.

## Plan

1. **Art direction and world bible.** See [docs/world-bible.md](docs/world-bible.md) and the reference sheets in [docs/references](docs/references).
2. **Wallpaper engine.** Rust and wgpu on DirectX 12. Runs behind the desktop icons across both monitors, with HDR on the HDR one, at low priority with a frame cap, and pauses itself for fullscreen apps or heavy load.
3. **Living simulation.** Genomes grow procedural bodies and small learning brains. Mutation, speciation, emergent social behavior and culture, predation, disease, famine and extinction. Matter is conserved and the world persists across reboots.
4. **Chronicle and polish.** A history view from the tray or a hotkey: species tree, notable individuals, and turning points.

## Status

Step 1 is done: world bible and 10 reference sheets.

Step 2 is in testing. The wallpaper runs behind the icons with a placeholder Vesper landscape (no creatures yet): flowing river, volcano and smoke, storm with lightning, glacier with meltwater, drifting spores, and a twilight band that slides with a four-day season.

## Running it

Download `desktopia.exe` from the [latest build](https://github.com/noalear/Desktopia/releases/tag/latest) and run it. Windows may warn that the app is unrecognised, because it isn't signed: choose **More info**, then **Run anyway**.

Desktopia lives in the tray. Right-click its icon to pause, to start it with Windows, or to quit. Quitting gives the normal wallpaper back.

It stays out of the way:

- It draws at 30 fps at most, at below-normal priority, in Windows efficiency mode.
- It stops for exclusive-fullscreen games, presentation mode, a locked PC and a sleeping display.
- It skips any monitor covered by a maximized or fullscreen window.
- It drops to 10 fps on battery or while other programs keep the CPU busy.

On a monitor with HDR switched on, it draws in HDR, using the Windows SDR brightness setting as its white so both monitors match.

Settings are in `%APPDATA%\Desktopia\config.toml`, and the log is in `%LOCALAPPDATA%\Desktopia\desktopia.log`.

## Building

- `cargo build --release -p desktopia` builds the wallpaper on Windows.
- `cargo run --release -p preview -- --scale 0.5` renders Vesper to PNG files on any OS with a GPU or a software Vulkan driver.
  Add `--crop X Y W H` to render a window of the world up close, `--lineup` to line up one creature of each body plan in that window, `--frames N --step S` for an animation, and `--storm X STRENGTH`, `--volcano V`, `--lava REACH`, `--ash A`, `--cold X STRENGTH` or `--dust X STRENGTH` to pin the weather. In `--lineup`, the creatures after the first show a sickness, a parasite, a symbiont, bleeding, and light signals in turn.

![Key art](docs/references/desktopia-ref-01-key-art.png)
