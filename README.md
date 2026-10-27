# Desktopia

A living desktop wallpaper for Windows. It shows **Vesper**, an alien world spread across two 4K monitors, where creatures and plants evolve, learn, form societies and die with no scripted behavior.

## Plan

1. **Art direction and world bible.** See [docs/world-bible.md](docs/world-bible.md) and the reference sheets in [docs/references](docs/references).
2. **Wallpaper engine.** Rust and wgpu on DirectX 12. Runs behind the desktop icons across both monitors, with HDR on the HDR one, at low priority with a frame cap, and pauses itself for fullscreen apps or heavy load.
3. **Living simulation.** Genomes grow procedural bodies and small learning brains. Mutation, speciation, emergent social behavior and culture, predation, disease, famine and extinction. Matter is conserved and the world persists across reboots.
4. **Chronicle and polish.** A history view from the tray or a hotkey: species tree, notable individuals, and turning points.

## Status

Step 1 is done. Step 2 is next.
