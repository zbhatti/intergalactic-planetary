# IP (Intergalactic Planetary)

A 3D universe explorer built with Rust and Bevy, inspired by SpaceEngine. Navigate through a physically accurate solar system and a catalog of 57 named stars at real astronomical coordinates.

## Features

- **Free-fly camera** — WASD to move, mouse to look, Esc to release cursor
- **Speed cycling** — press Shift to cycle through 8 speed multipliers (x0.1 → x1,000,000 base speed of light)
- **Object search** — press Enter to open search, type a name, use arrow keys to pick, Enter to select
- **Direction ring** — white ring drawn in front of the camera pointing at the selected object
- **Off-screen arrow** — gizmo arrow at the screen edge when the target is out of view
- **Distance HUD** — live distance to target in appropriate units (m / km / AU / ly)
- **Speed HUD** — current speed in km/s, fraction of c, or multiples of c

## Astronomical accuracy

- Star positions from real RA/Dec/distance catalog data (57 named stars)
- Heliocentric planet positions approximated for June 25, 2026
- Physical sizes: stars in solar radii, planets in km converted to AU
- Coordinate system: 1 AU = 1 game unit, 1 parsec = 1000 game units
- Base speed = speed of light (≈0.002004 AU/s)

## Controls

| Input | Action |
|---|---|
| W / A / S / D | Move forward / left / back / right |
| Mouse | Look around |
| Shift | Cycle speed multiplier |
| Enter | Open search |
| Arrow Up / Down | Navigate search results |
| Enter (in search) | Select object and orient camera |
| Esc (in search) | Close search |
| Esc (in flight) | Toggle cursor lock |

## Running

```bash
cargo run
```

Requires Rust 1.88+ and a Metal-capable GPU (macOS) or Vulkan/DX12 (other platforms). OpenGL works
as a fallback (`WGPU_BACKEND=gl cargo run`).

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

## Project structure

```
src/
├── main.rs          # App wiring
├── constants.rs     # Physical constants (c, AU, ly)
├── camera.rs        # FlyCam component and movement systems
├── search.rs        # Search UI and object catalog
├── speed.rs         # Speed multiplier state
├── solar_system.rs  # Planet and Sun spawning
├── stars.rs         # 57-star catalog and coordinate math
├── ui.rs            # Distance/speed HUD labels
└── navigation.rs    # Direction ring, off-screen arrow, camera orient
```

## Tests

```bash
cargo test
```

42 unit tests cover coordinate conversion (`star_world_pos`), distance/speed formatting, catalog string matching, and camera yaw/pitch derivation.

## Data

Star positions come from the [HYG Database](https://codeberg.org/astronexus/hyg) v4.4 (c) astronexus.com, CC BY-SA 4.0. Solar system orbits come from NASA/JPL. See [DATA_LICENSES.md](DATA_LICENSES.md).
