# Space Engine

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

Requires Rust 1.80+ and a Metal-capable GPU (macOS) or Vulkan/DX12 (other platforms).

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

16 unit tests cover coordinate conversion (`star_world_pos`), distance/speed formatting, catalog string matching, and camera yaw/pitch derivation.
