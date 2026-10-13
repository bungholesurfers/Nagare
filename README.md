# PlayStation 3 XMB Waves Recreation

> Fork of [linkev/PlayStation-3-XMB](https://github.com/linkev/PlayStation-3-XMB). Issues opened here are about this fork; the original project keeps its own.

**Click the image for a demo!**
[![Demo of PlayStation 3 XMB Waves Recreation](demo.png)](https://faquino.github.io/PlayStation-3-XMB/)


## About the Project

WebGL-based recreation of the PlayStation 3 XrossMediaBar (XMB) background wave system (the wave, the sparkles around it and the backdrop behind).

This repo used to be mostly "looks close enough, ship it" shader guesswork (still archived in `old-research`). The active implementation, in `ps3xmbwave`, is now a port of the console's own code, read out of PS3 firmware 4.93 running in RPCS3: for the wave, the simulation of its lines that `custom_render_plugin` runs on the PPU, the deformation lattice the GPU works out and the B-spline surface `spline.elf` builds on an SPU; for the sparkles, `particles.elf`'s SPU task and the plugin's emitter; and for the backdrop and the passes after the wave, the XMB's own programs. Each piece is checked against what the console recorded while it ran - savestates and frame captures.

What is still modelled rather than read out of the firmware is marked as such, both in the code and in the reverse-engineering notes.

The project was started by Mart in [linkev/PlayStation-3-XMB](https://github.com/linkev/PlayStation-3-XMB). This fork took it from there with the goal of achieving a visually faithful reproduction of the scene appearing in the XMB background, using RPCS3 as a reverse-engineering tool.


## Credits

While this is inspired by the official PlayStation 3 XMB background wave design, Mart's first version started from this [CodePen by Alphardex](https://codepen.io/alphardex/pen/poPZNwE), which was subsequently refined and optimized. That code lives on in `old-research` and the current implementation in `ps3xmbwave` no longer derives from it.


## Current Features (new implementation in `ps3xmbwave`)

- **Reverse-engineered wave**: `wave-reverse.js` builds the wave the way the console does - the PPU's simulation of 19 lines, the GPU's deformation lattice and the SPU's B-spline surface - into the console's own 128 × 128 mesh, and `spline.js` draws it with the XMB's line programs re-authored in GLSL. The firmware's parameter sets move it as they move the particles: through the day, the XMB's start, a game's launch and the music. The scene's fade - Theme Settings' Brightness, or another module taking the screen - dims it with the backdrop and the sparkles, as the console's does.
- **WebGL2 renderer**: Pure WebGL2 rendering path (backdrop + wave mesh + the passes after it + particles), no framework dependency.
- **The console's backdrop**: the default, `Auto (date and time)`, runs the XMB's own backdrop program, re-authored, over fits of its month textures: this month's and next month's colours taken through hue, saturation and value, lit by clocks that run through the day with two glows going round, next month wiped in up the screen as the month goes on, and the night's textures mixed in by the hour. Under the music it switches, as the console does, to the console's other program. The 12 month gradients with day/night variants, and an "Original (RGB Sliders)" mode, remain as presets.
- **Live control panels**: separate spline and particle panels with a slider or select per setting, named after the firmware's own parameters. Tooltips indicate what each setting does and where it comes from; a reset per row, a lock that keeps a value from the theme, and colours that tell who set each value - by hand, the theme, a boot sequence or the music.
- **Reverse-engineered particle system**: `particles-reverse.js` is a JavaScript port of the SPU update task in the PS3's `particles.elf` and of the PPU side that feeds it - the emitter, the flow grid and the controller's response - drawn in the two passes the XMB uses, re-authored in GLSL. The firmware's own parameter sets come with it: the day cycle runs by the clock, and the controller stand-in (mouse drag, pointer, arrow keys) stirs the field the way the real one does.
- **Reverse engineering notes included**: [SPLINE_REVERSE_ENGINEER.md](SPLINE_REVERSE_ENGINEER.md), [WAVE_REVERSE_ENGINEER.md](WAVE_REVERSE_ENGINEER.md) (an index into [`docs/wave/`](docs/wave/)), [PARTICLES_REVERSE_ENGINEER.md](PARTICLES_REVERSE_ENGINEER.md) (an index into [`docs/particles/`](docs/particles/)) and [BACKGROUND_REVERSE_ENGINEER.md](BACKGROUND_REVERSE_ENGINEER.md) (an index into [`docs/background/`](docs/background/)) document traced functions, memory ranges, and what is still modelled, each one saying which is which.


## Reality Check and What Still Needs Work

- There are no firmware assets or code in this repository, so the page generates approximate fits of the textures it needs: the backdrop's month textures, the sparkles' iridescent colours and the table used to light the wave's edges where it turns away from view (`_FresLUT`). The shader programs that draw the backdrop, on the other hand, have been rewritten faithfully, checked against the console's to float precision: the default one (which runs over those month fits) and the one used while music plays (which needs no texture at all).
- Sparkles run the console's own update task, fed by the PPU side as traced - the emitter, the flow grid's rule, the controller - and a headless bench (`tools/bench/particles.js`) compares their pool with one read out of a savestate and what they draw with two frame captures, each at its own moment. What is left is the XMB's side: how its icons move, which writes the flow grid, and when it sends a step.
- The wave's geometry is the console's own now, checked piece by piece against savestates, and so are the passes the console runs after it - the composite with the backdrop, the tone curve and the glare: fed the console's own mesh, the page draws RPCS3's screenshot of the moment. What is left is the very first minutes after a start: the console starts its lines from a state baked into its firmware, so the page makes one the same way, which keeps the console's pace but not its exact course.
- The glare follows RPCS3, which runs the CLAMP its textures are set to as clamp-to-edge. On a real RSX, CLAMP may instead blend the edge with a border colour, as the GPUs it descends from did, which would dim the glare along the screen's edges; only real hardware can tell.


## How It Was Reverse Engineered

Everything `ps3xmbwave` ports comes from PS3 firmware 4.93 running in RPCS3: the PPU and SPU code of its modules, read and traced, and checked against what RPCS3 recorded while the XMB ran - savestates, which hold the console's memory, and RSX frame captures, which hold every draw of a frame with its uniforms. The tools written in Python that do it are in [`tools/re`](tools/re/README.md): they read your own firmware and write what they extract into `re-work/`, which git ignores. No firmware files are in this repo: only values read out of it, such as the `.mnu` parameters, and data fitted to it, such as the backdrop's month textures in `background-months.js`.

Two headless benches compare the page with the console's own figures, recorded inside them, and need no firmware:

```bash
node tools/bench/wave.js --seconds 10 --every 2
node tools/bench/particles.js --seconds 30 --runs 3 --seed 1
```


## Local Development

1. Clone the repository:
```bash
git clone https://github.com/faquino/PlayStation-3-XMB.git
cd PlayStation-3-XMB
```

2. Install dependencies:
```bash
npm install
```

3. Start the development server:
```bash
npm run start
```

4. Open in browser:
- Main current implementation: `http://localhost:8000/ps3xmbwave/`
- Old implementation archive: `http://localhost:8000/old-research/`
- DDS gradient utility: `http://localhost:8000/dds/`


## Explanation of the Files and Folders

- `ps3xmbwave`: Current active implementation (reverse-engineering-based wave, particles, backdrop and settings UI), published as the demo site.
- `old-research`: Previous implementation and experiments (guesswork/prototyping era), kept for history and references.
- `dds`: "Poor man's gradient extraction" tool. Loads PS3 `.dds` files, solves best-fit 2D linear gradients (angle + colors), previews reconstruction, and exports JS preset code.
- `WAVE_REVERSE_ENGINEER.md`, `PARTICLES_REVERSE_ENGINEER.md`, `BACKGROUND_REVERSE_ENGINEER.md`: the indexes of the reverse-engineering notes - what each file of `ps3xmbwave` ports as verified, what it models, and what is still missing.
- `docs`: the notes themselves, one topic per file under `wave`, `particles` and `background`.
- `SPLINE_REVERSE_ENGINEER.md`: upstream's reverse-engineering notes from a Ghidra pass on `spline.elf`, kept as they were written; the wave notes say what they confirm or overturn.
- `tools/re`: the reverse-engineering tools, which read your own firmware - see [How it was reverse engineered](#how-it-was-reverse-engineered).
- `tools/bench`: the headless benches that compare the page with the console.
- `.github/workflows/pages.yml`: publishes `ps3xmbwave` to GitHub Pages on every push to `main` that changes it.
- `CLAUDE.md`: guidance for Claude Code working in the repository - its architecture, conventions and how the notes are kept.
- `demo.png`: Preview image used in this README.


## How to Use (`ps3xmbwave`)

Every setting's name has a tooltip that says what it does and where it comes from.

1. **Spline Controls (top-right)**:
   - Leave the gradient on `Auto (date and time)`, or pick a preset (`MM (Day)` / `MM (Night)`), or go back to `Original (RGB Sliders)`.
   - Tweak the wave with `LINE1.mnu`'s own parameters - its springs, noise, placement, deformation and lighting - and the passes after it with `HDR.mnu`'s (exposure, tone curve, glare) and `BACKGROUND.mnu`'s colours.

2. **Particles Controls (top-left)**:
   - `Theme` picks the parameter set the whole scene runs on: `Auto (time of day)` blends dawn, day, dusk and night with the clock, and the others hold one of the firmware's sets.
   - `Sequence` plays one of the XMB's boot sequences over it: `XMB start`, `Game launch` or `Other launch`.
   - Tweak the sparkles with `PARTICLES.mnu`'s own parameters - emission, life, wind and noise, lighting, size and focus, glare - and how the controller stirs them with `PARTICLES_UI.mnu`'s.
   - Set the console's state the scene is drawn under: the video output, What's New's board, music playback, Theme Settings' Brightness and Colour, and the background given away to another module.

3. **Interacting with the scene**:
   - Moving the pointer across the screen steps the XMB's focus over a virtual 7 × 7 grid of icons, as the arrow keys do; each step turns and kicks the sparkles as the D-pad does on the console.
   - Dragging with the mouse tilts and shakes a virtual Sixaxis.

4. **UI visibility**:
   - Each panel has a `Hide` button and a matching `Show ...` button.
   - `H` hides or shows every control, and the notice at the bottom; opening the page with `?ui=off` starts it without them, for the scene alone - in a frame or as a wallpaper.


## Technical Details (subject to change anytime)

### WebGL Implementation (`ps3xmbwave`)
- **Rendering path**: WebGL2 only; without `EXT_color_buffer_float` the glare is left out.
- **Layering**: the backdrop into a 64 × 32 buffer, the wave's light into one of its own, the console's composite, tone curve and glare over both, then the additive particles.
- **Backdrop source**: the console's backdrop programs, `back_colours0` (the months') and `back_colours1` (the music's), drawn into that buffer and eased in from a copy of it.
- **Wave source**: the console's 128 × 128 mesh, built on the CPU each frame by `wave-reverse.js` and uploaded as a vertex buffer.
- **Particle source**: the port of `particles.elf`'s SPU task, stepped on the CPU at a fixed 60 Hz (at most four steps a frame), drawn as instanced quads in the XMB's two passes.

### Reverse-Engineered Wave Status
- **What is implemented**: the lines' simulation, the deformation lattice, the free-form deformation, the camera and the B-spline surface, with the line programs' shading, the passes that follow the wave, and the firmware's `LINE1.mnu`, `HDR.mnu` and `BACKGROUND.mnu` sets - see [WAVE_REVERSE_ENGINEER.md](WAVE_REVERSE_ENGINEER.md) and [BACKGROUND_REVERSE_ENGINEER.md](BACKGROUND_REVERSE_ENGINEER.md).
- **What is still missing**: the very first minutes after a start, and what keeps a run from the reset from landing exactly on a savestate.


## License and Disclaimer

This is an unofficial, non-commercial fan project. Open sourced and available under the [MIT License](LICENSE). The license covers this repository's own code and notes; it grants nothing over Sony's trademarks, firmware or assets.

It's not affiliated with, endorsed by or sponsored by Sony Interactive Entertainment. "PlayStation", "PS3" and "XMB" are trademarks or registered trademarks of Sony Interactive Entertainment Inc., used here only to name what the project recreates.

No Sony firmware, code, shaders or assets are distributed here. The renderer and its shaders are written anew in GLSL, not copied; the few textures the page needs are generated at start-up or approximated by fitted curves; and the tools in `tools/re/` only read firmware the user already has, writing into `re-work/`, which `.gitignore` keeps out of the repository.


## Acknowledgments and Thanks

- Mart, for starting the project this fork comes from.
- Everyone involved in the PS3 XMB and its background animation both at Sony and Q-Games. I think it is the best video game console interface design ever, and I love it.
- [Alphardex](https://codepen.io/alphardex/pen/poPZNwE) for the original code that served as the starting point for the project from which this one is derived.
- The [RPCS3](https://rpcs3.net/) team. This reverse engineering was made possible by RPCS3's savestates, frame captures and shader decompiler.


## Support

Found a bug or something that looks different from the console? [Open an issue on GitHub!](https://github.com/faquino/PlayStation-3-XMB/issues)


## TODO

- Bring the wave's first minutes after a start closer to the console's: its lines start from a state baked into the firmware, which the page has to make its own way - see [WAVE_REVERSE_ENGINEER.md](WAVE_REVERSE_ENGINEER.md#still-missing).
- Find what keeps a run of the lines from the reset from landing exactly on a savestate.
- Trace the XMB's side of the sparkles: how it moves its icons and when it repeats a held direction, which the page models, and which of its actions open What's New's board - see [PARTICLES_REVERSE_ENGINEER.md](PARTICLES_REVERSE_ENGINEER.md#still-missing).
- Find where `_Gamma` comes from, what starts `anim_coldboot` and who sends the scene its events 2 and 3.
- Catch the backdrop's ease in a frame capture, to confirm the copy it starts from - see [BACKGROUND_REVERSE_ENGINEER.md](BACKGROUND_REVERSE_ENGINEER.md#still-missing).
