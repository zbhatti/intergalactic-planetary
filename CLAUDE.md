# CLAUDE.md — Developer Guide for intergalactic-planetary

## What this project is

A 3D universe explorer built with Rust and Bevy. The player can fly through a physically accurate model of the solar system and a catalog of 58 real named stars at their true positions, with physically correct luminosity-based brightness that updates in real time as the camera moves.

## Quick start

```bash
cargo run          # build and launch
cargo test         # run all 38 unit tests (headless, no GPU needed)
```

Rust 1.80+ required. First build downloads Bevy and takes a few minutes; incremental builds are fast because dynamic linking is enabled in dev mode.

## Tech stack

| Layer | Choice | Why |
|---|---|---|
| Language | Rust | Memory safety, performance |
| Engine | Bevy 0.16 | ECS architecture, wgpu backend |
| GPU backend | Metal (macOS) / Vulkan / DX12 | wgpu picks the right one per platform |
| Build | Cargo with `dynamic_linking` feature | Fast incremental dev builds |

CI runs on Ubuntu, macOS (ARM), and Windows via GitHub Actions (`.github/workflows/ci.yml`).

## Coordinate system — read this first

```
1 AU  = 1 game unit
1 pc  = 1000 game units   (PARSEC_TO_UNITS)
1 ly  ≈ 206.3 game units
```

The Sun is at the origin. Planets orbit in the XZ plane (Y = 0). Star positions are converted from real RA/Dec/distance catalog data using spherical coordinates:

```rust
// ra_hours → radians; dec_deg → radians; dist_pc → game units
x = d * cos(dec) * cos(ra)
y = d * sin(dec)
z = d * cos(dec) * sin(ra)
```

Camera euler convention: `EulerRot::YXZ` — yaw around Y first, then pitch around X. The yaw/pitch extraction from a direction vector must match this: `yaw = atan2(-dx, -dz)`, `pitch = asin(dy)`.

## Module map

| File | Responsibility |
|---|---|
| `main.rs` | App wiring only — registers resources and systems |
| `constants.rs` | Physical constants: `C_AU_PER_S`, `KM_PER_AU`, `AU_PER_LY` |
| `camera.rs` | `FlyCam` component, `CursorLocked` resource, camera spawn + movement |
| `search.rs` | `Search` + `ObjectCatalog` resources, search UI, `catalog_matches` |
| `speed.rs` | `SpeedState` resource, Shift-to-cycle speed multiplier |
| `solar_system.rs` | Spawns Sun + 8 planets with heliocentric positions for 2026-06-25 |
| `stars.rs` | 58-star catalog, `star_world_pos`, brightness model, dynamic scaling |
| `ui.rs` | Distance/speed/exposure HUD labels, `format_distance`, `format_speed` |
| `navigation.rs` | HUD reticle, off-screen arrow gizmo, camera auto-orient |
| `menu.rs` | Esc pause menu, background stars toggle, quit button |

## Controls

| Key | Action |
|---|---|
| W/A/S/D | Fly forward/left/back/right |
| Mouse | Look around |
| Shift | Cycle speed multiplier (x0.1 → x1,000,000 of c) |
| `[` / `]` | Decrease / increase exposure (EV-5 to EV+20) |
| Enter | Open object search |
| Arrow Up/Down | Navigate search results |
| Enter (in search) | Select object, orient camera toward it |
| Esc (in search) | Close search |
| Esc | Open/close pause menu |
| Quit button | Exit cleanly via `AppExit::Success` |

## Known gotchas and past bugs

### 1. SOLAR_RADIUS_UNITS must NOT divide by 206265

The correct formula is:
```rust
pub const SOLAR_RADIUS_UNITS: f32 = 695_700.0 / 149_597_870.7; // ≈ 0.00465 AU
```

An earlier version included `/ 206_265.0 * PARSEC_TO_UNITS`, which is the parallax-distance formula, not a radius conversion. It made all star spheres ~206,000× too small and invisible. The test `test_solar_radius_units_is_correct_au` catches this.

### 2. Stars at real distances are sub-pixel — dynamic scaling required

Even with the correct physical radius, Sirius at 2637 AU has a sphere radius of ~0.008 AU, which is ~0.00034° on screen — far below one pixel. Every frame, `update_star_brightness` scales each star's `Transform` so its angular diameter is always ≥ `MIN_STAR_ANGLE` (0.002 rad ≈ 2–3 pixels):

```rust
pub fn star_visual_radius(physical_radius: f32, cam_dist_units: f32) -> f32 {
    physical_radius.max(cam_dist_units * MIN_STAR_ANGLE)
}
```

When the player is close to a star, physical size takes over naturally. Do not remove this scaling or distant stars will disappear.

Background stars (randomly generated, 2000 of them) are spawned as unit spheres scaled to `dist_from_origin * MIN_STAR_ANGLE` at startup. They don't need per-frame updates because they're decorative filler — their distance from any player position doesn't change drastically.

### 3. Bevy's default font only supports ASCII

The default Bevy font has no glyphs for Unicode characters like `▶`, `×`, `✓`, `→`, or any emoji. They render as white rectangles. Always use ASCII alternatives (`>`, `x`, `[x]`, `->`). The tests `test_format_distance_is_ascii_only` and `test_format_speed_is_ascii_only` guard against this.

### 4. HDR and bloom must be enabled together

Stars use high emissive values to appear bright. Without HDR (`Camera { hdr: true }`), values above 1.0 clip to white and lose color information. Without `Bloom::NATURAL`, a tiny bright sphere may not be visually obvious even if it's rendering correctly. Both are set in `setup_camera`.

### 5. Speed display threshold: c is ~0.002004 AU/s

```rust
pub const C_AU_PER_S: f32 = 299_792.458 / 149_597_870.7; // ≈ 0.002004 AU/s
```

`format_speed` switches to km/s only below 0.01c (~0.00002 AU/s). Passing 0.001 AU/s gives `"0.4990c"`, not km/s — this tripped up a test and is the expected behavior.

### 6. Background stars are hidden by default

`ShowBackgroundStars` defaults to `false`. Background star entities spawn with `Visibility::Hidden`. The pause menu checkbox reflects this. If you change the default, also update the menu label initialization and the test `test_background_stars_off_by_default`.

### 7. Query conflicts between camera and star transforms

`update_star_brightness` mutates `Transform` on star entities. The camera also has a `Transform`. Bevy's borrow checker requires the `Without<FlyCam>` filter on the star query to disambiguate:

```rust
mut star_query: Query<(&mut Transform, ..., &Star), Without<FlyCam>>,
```

Without this filter the system won't compile.

### 8. Bevy 0.16 API notes

- Use `.single()` / `.single_mut()` — `.get_single()` / `.get_single_mut()` are deprecated.
- `AmbientLight` gained an `affects_lightmapped_meshes` field — always use `..default()` to future-proof struct literals.
- Bloom: `bevy::core_pipeline::bloom::Bloom` — add to camera entity alongside `Camera { hdr: true }`.
- Events use `.write()` not `.send()` (e.g., `exit.write(AppExit::Success)`).
- `MeshMaterial3d<M>` is a newtype: access the handle via `.0`.

## Brightness model

```
emissive = 2^EV × 0.5 × luminosity_solar / dist_pc²
```

- `BRIGHTNESS_SCALE = 0.5` calibrated so Vega (L=40.12 L☉, d=7.68 pc) ≈ emissive 0.34 at EV 0.
- Spectral color (from `spectral_color`) is applied as a tint multiplied by brightness.
- Distance is clamped to a minimum of 1e-4 pc to avoid infinite brightness on close approach.
- Exposure range: EV -5 (dim) to EV +20 (very sensitive). Each step doubles/halves all star brightness.

## Adding new catalog stars

Extend `CATALOG_STARS` in `stars.rs`. Each entry is:
```rust
("Name", ra_hours, dec_deg, dist_parsecs, spectral_class, radius_solar_radii, luminosity_solar)
```

The star is automatically added to `ObjectCatalog` (searchable) and rendered with correct position, size, and brightness.

## Running CI locally

```bash
# same checks as CI
cargo check --all-targets
cargo test --all-targets
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Linux builds additionally require: `libasound2-dev libudev-dev libxkbcommon-dev pkg-config`
