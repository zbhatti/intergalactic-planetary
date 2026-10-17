# IP (Intergalactic Planetary)

A 3D universe explorer built with Rust and Bevy, inspired by SpaceEngine. Fly from the surface of the
Moon out past the nearest stars at real scale: real positions for the Sun, planets, dwarf planets and
major moons, and ~109,000 real stars from the HYG catalog. Functionality comes first, so it runs at
60 fps on an Intel HD 520. See [ROADMAP.md](ROADMAP.md) for what's next (the goal is the Laniakea
supercluster).

## Features

- **Real scale**: f64 world coordinates with a floating origin, so there's no jitter from metres
  up to megaparsecs. Speed adapts to the distance from the nearest surface.
- **Go-to flight**: select anything, press G, and you fly there in seconds whatever the distance.
- **Solar system** placed for 2026-06-25 from NASA/JPL orbital elements: 8 planets, 5 dwarf
  planets, 21 major moons, 9 notable asteroids and 5 comets (Halley is out near aphelion, ~35 AU).
- **109k catalog stars** drawn as one instanced point cloud. Any visible star can be clicked, and
  the 540 named ones can be searched for. Close up (< 2000 AU), the nearest star becomes a sphere.
- **6,400 exoplanets** in 4,800 systems from the NASA Exoplanet Archive: search a host or planet
  (e.g. TRAPPIST-1 e), press G, and its planets appear as you arrive. Transiting planets
  really do cross their star as seen from Earth (orbit node and phase are assumed).
- **Nebulae**: 168 H II regions and 52 reflection nebulae at their catalogued distances and sizes,
  coloured from their visible spectra (emission lines, or starlight scattered by dust) the way an
  eye would see them. Faint at normal exposure, as in reality: turn auto-exposure off (`\`) and
  raise the exposure (`]`) for a long-exposure view, or fly into one.
- **Interstellar dust** from a real 3D dust map (Edenhofer et al. 2024, 1.25 kpc around the Sun):
  stars and nebulae behind dust are dimmed and reddened as seen from wherever you are. From Earth
  the sky looks as observed; fly past a cloud and what's behind it changes. Toggle in the Esc menu.
- **Info panel** (type, radius, mass, temperature, luminosity, distance), **solar system browser**,
  **star browser** (nearest or brightest from wherever you are), **search**, and **saved locations**
  that persist between runs.
- **Zoom**: mouse wheel, from 120 deg down to 0.01 deg. Stars keep their size in pixels, like a
  telescope.
- **Sky overlays**: the 88 IAU constellation figures (C), which distort as you fly away from home
  because their stars are at real distances, and equatorial/ecliptic/galactic grids (Esc menu).
- **Auto-exposure**: approach a star and the sky dims until its disk is visible, as for an eye or
  camera. It never brightens past your manual exposure.
- **Labels**, filtered by type and by star brightness from where you are, and decluttered.
  Labels never overlap each other or the HUD, and they hide behind planets.

## Controls

| Input | Action |
|---|---|
| W / A / S / D | Fly forward / left / back / right |
| Mouse | Look around (slower when zoomed in) |
| Mouse wheel / middle click | Zoom / reset zoom |
| Shift | Cycle throttle (x0.1 to x30 of the adaptive speed) |
| Left click | Select the object under the crosshair |
| G | Go to the selection |
| I | Show/hide the info panel |
| B | Solar system browser |
| N | Nearest stars (Tab: brightest from here) |
| K | Saved locations (save the view, jump back, Delete removes) |
| L | Labels on/off (per-type filters in the Esc menu) |
| C | Constellation figures and names |
| Enter | Search (arrows to pick, Enter to select, Esc to close) |
| `[` / `]` | Exposure down / up |
| `\` | Auto-exposure on/off |
| Esc | Pause menu (labels, star catalog, quit) |
| F3 / F12 | Performance overlay / screenshot |

## Running

```bash
cargo run                                       # dev build (dynamic linking)
cargo build --release --no-default-features     # standalone release binary
```

Requires Rust 1.88+ and a Metal-capable GPU (macOS) or Vulkan/DX12 (other platforms). OpenGL works
as a fallback (`WGPU_BACKEND=gl cargo run`). CI builds Linux, macOS and Windows binaries on every
PR (artifacts `IP-Linux`, `IP-macOS`, `IP-Windows`).

### Troubleshooting older GPUs

- **Debug build aborts with `Unable to load cmd_begin_debug_utils_label_ext`** (seen on Intel Haswell
  with Mesa 21): fixed in IP, which no longer turns on wgpu's debug labels by default. If you turned
  them on yourself with `WGPU_DEBUG=1`, turn them off again.
- **Panic creating `bloom_texture` (`Rg11b10Ufloat`)**: IP now skips bloom on GPUs that can't render
  that format, and logs `bloom off`. `IP_BLOOM=0` turns bloom off by hand.
- **OpenGL panics on `downsample depth` pipelines**: fixed when GL is chosen with
  `WGPU_BACKEND=gl`. IP then turns off Bevy's GPU occlusion culling, which IP doesn't use.
- Pick the GPU with `WGPU_POWER_PREF=low` (integrated) or `high` (discrete), and the backend with
  `WGPU_BACKEND=vulkan|gl|dx12|metal`. The `AdapterInfo` log line shows which one was used.

## Development

`cargo test` runs the unit tests and the headless app tests (real Bevy schedules, no GPU).
`CLAUDE.md` has the architecture notes and conventions, and `PERF.md` has the benchmark procedure
and results.

## Data

Star positions come from the [HYG Database](https://codeberg.org/astronexus/hyg) v4.4 (c) astronexus.com, CC BY-SA 4.0. Constellation figures come from [Stellarium](https://github.com/Stellarium/stellarium)'s modern sky culture, CC BY-SA 4.0. The 3D dust map is Edenhofer et al. (2024, A&A 685, A82), [CC BY 4.0](https://zenodo.org/records/10658339). Nebulae come from the Sharpless and van den Bergh catalogs, with distances from Mendez-Delgado et al. (2022), Foster & Brunt (2015) and Anderson et al. (2014), via [VizieR](https://vizier.cds.unistra.fr/) (CDS, Strasbourg). This project has made use of the [NASA Exoplanet Archive](https://exoplanetarchive.ipac.caltech.edu/), which is operated by the California Institute of Technology, under contract with the National Aeronautics and Space Administration under the Exoplanet Exploration Program. Solar system orbits come from NASA/JPL. See [DATA_LICENSES.md](DATA_LICENSES.md).
