# CLAUDE.md — Developer Guide for IP (Intergalactic Planetary)

## What this project is

A 3D universe explorer built with Rust and Bevy. The player can fly through a physically accurate model of the solar system and a catalog of 58 real named stars at their true positions, with physically correct luminosity-based brightness that updates in real time as the camera moves.

## Quick start

```bash
cargo run          # build and launch
cargo test         # run all unit + headless app tests (no GPU needed)
cargo build --release --no-default-features   # standalone release binary
```

Rust 1.88+ required. First build downloads Bevy and takes a few minutes; incremental builds are fast because the default `dev` feature turns on `bevy/dynamic_linking`. Release builds must use `--no-default-features`, otherwise the binary needs Bevy's shared library next to it.

The project is named **IP** (crate/binary `intergalactic-planetary`, window title "IP - Intergalactic Planetary"). Don't use the name "SpaceEngine"/"Space Engine" for it — that's Cosmographic Software's product, which this project is inspired by but doesn't share code or data with. New features are modelled on SpaceEngine's *visible behaviour* only (run it, observe, reimplement from public data/papers); never decompile or extract data from its install — its license forbids it.

### Dev machine note (Surface Book)

The main dev machine's NVIDIA dGPU sits in a keyboard base that can overheat and hard power-off. A git-ignored `.cargo/config.toml` sets `WGPU_POWER_PREF=low` so `cargo run` uses the Intel iGPU (check the `AdapterInfo` log line). Prefer functionality over graphical polish, and keep GPU-heavy test runs short. To run from an SSH session, target the local X display: `DISPLAY=:0 XAUTHORITY=/run/user/1000/gdm/Xauthority cargo run`.

## Tech stack

| Layer | Choice | Why |
|---|---|---|
| Language | Rust | Memory safety, performance |
| Engine | Bevy 0.16 | ECS architecture, wgpu backend |
| GPU backend | Metal (macOS) / Vulkan / DX12 | wgpu picks the right one per platform |
| Build | Cargo with `dynamic_linking` feature | Fast incremental dev builds |
| Bevy features | Trimmed list in `Cargo.toml` (`default-features = false`) | Smaller binary, faster build; add a feature only when a system needs it |

CI runs on Ubuntu, macOS (ARM), and Windows via GitHub Actions (`.github/workflows/ci.yml`), and uploads a standalone release binary per OS as the `IP-Linux` / `IP-macOS` / `IP-Windows` artifacts.

## Coordinate system — read this first

```
World space: f64, 1 unit = 1 AU, Sun at the origin (WorldPos component)
1 pc  = 206,264.806 AU   (AU_PER_PC — real scale; before Phase 1 it was a compressed 1000)
1 ly  = 63,241.077 AU    (AU_PER_LY)
Render space: f32 Transform = WorldPos - FloatingOrigin (origin kept near the camera)
```

**Never read or write `Transform::translation` for game logic.** Use `WorldPos` (f64). `coords.rs` writes render
transforms in `NavSet::Rebase`. For screen projection, project `(object_world - camera_world)` through
`coords::view_transform(camera_rotation)`. That's independent of where the origin is, so it never jumps on a
rebase. Systems run in `Update` in three chained sets: `NavSet::Input` (camera moves) → `NavSet::Rebase`
(nearest-body scan, origin shift, transform sync) → `NavSet::View` (labels, HUD, brightness, gizmos).

Precision: f64 resolves ~150 m out to ~50 pc but only ~2000 km at ~0.5 Mpc. Planets around distant stars will
need star-local frames (see `coords.rs` docs). The camera's frustum `far` is 1e15 AU: Bevy culls past `far`
even though depth is infinite reverse-Z, so it must exceed the farthest object.

The Sun is at the origin. The world frame is **equatorial J2000/ICRF with +Y = north celestial pole**:
`world = (x_eq, z_eq, -y_eq)` (`orbit::equatorial_to_world`). That's a rotation. An earlier `(x_eq, z_eq, y_eq)`
mapping was a mirror image of the real sky (Orion appeared flipped), and `test_sky_is_not_mirrored` guards it.
Planets orbit near the **ecliptic**, tilted 23.44° from the equator (`orbit::ecliptic_to_world`), not in the XZ plane.

```rust
// stars: RA/Dec/distance -> world AU
star_world_pos(ra_h, dec, d_pc) = equatorial_to_world(radec_to_equatorial(ra_h * 15, dec)) * d_pc * AU_PER_PC
```

Solar-system bodies come from published orbital elements evaluated with a Kepler solver at `orbit::SCENE_JD`
(2026-06-25): planets from the JPL Standish table, dwarf planets from JPL SBDB, and moons from JPL satellite
mean elements in each planet's Laplace/equatorial plane. Don't hand-enter positions. Mercury, Venus and Earth were
once wrong by 80–250° that way.

Camera euler convention: `EulerRot::YXZ` — yaw around Y first, then pitch around X. The yaw/pitch extraction from a direction vector must match this: `yaw = atan2(-dx, -dz)`, `pitch = asin(dy)`.

## Module map

| File | Responsibility |
|---|---|
| `main.rs` | App wiring only — registers resources and systems |
| `constants.rs` | Physical constants: `C_AU_PER_S`, `KM_PER_AU`, `AU_PER_LY` |
| `coords.rs` | `WorldPos`/`BodyRadius`, floating origin + rebase, nearest-body scan, `NavSet` ordering |
| `info.rs` | `BodyInfo`/`BodyKind` (name + facts per object), crosshair click-select, info panel (I) |
| `browser.rs` | B: solar system browser (Sun → planets/dwarfs → moons by orbit size) |
| `goto.rs` | G: go-to flight (turn, then log-distance travel to a few radii from the target) |
| `camera.rs` | `FlyCam` component, `CursorLocked` resource, camera spawn + WASD movement in world space |
| `search.rs` | `Search` + `ObjectCatalog` resources, search UI, `catalog_matches` |
| `speed.rs` | Distance-adaptive speed (`CurrentSpeed`), Shift-to-cycle throttle |
| `orbit.rs` | Reference frames (equatorial/ecliptic → world), Kepler solver, `Elements`, `SCENE_JD` |
| `solar_system.rs` | Sun, 8 planets, 5 dwarf planets, 21 major moons from JPL elements; min apparent size for bodies |
| `stars.rs` | 58 curated sphere stars (matched to HYG for visual luminosity), `star_world_pos`, brightness model, dynamic scaling |
| `hyg.rs` | Embedded HYG catalog (`data/hyg_stars.bin`): little-endian parser, magnitudes, direction matching |
| `hyg_bodies.rs` | HYG stars as entities on demand: 540 named (search/go-to), crosshair-picked, and a close-range sphere proxy for the nearest star (<2000 AU) |
| `starfield.rs` + `starfield.wgsl` | 109k HYG stars as billboard meshes in 200 pc chunks (one draw each); shader brightness/size = same model as `stars.rs`; nearest-catalog-star scan |
| `zoom.rs` | Mouse-wheel FOV (`CameraFov`), `min_star_angle(fov)` keeps points the same pixel size at any zoom |
| `labels.rs` | Name labels: type/magnitude filters (`LabelSettings`), primary-separation rule for moons, occlusion by planet disks, greedy declutter, HUD avoidance (`LabelBlocker`) |
| `ui.rs` | Distance/speed/exposure HUD labels, `format_distance`, `format_speed` |
| `navigation.rs` | HUD reticle, off-screen arrow gizmo, camera auto-orient |
| `menu.rs` | Esc pause menu: data-driven `MenuItem` toggles whose text is regenerated from the settings |
| `gpu_compat.rs` | Old-GPU workarounds: wgpu debug labels off, bloom only if `Rg11b10Ufloat` is renderable, no GPU culling on GL-only |
| `perf.rs` | F3 perf overlay, `IP_BENCH` scripted benchmark mode, frame-time stats |
| `testing.rs` | Headless test harness (test builds only): fixed 60 Hz clock, real key events |

## Controls

| Key | Action |
|---|---|
| W/A/S/D | Fly forward/left/back/right (cancels go-to) |
| Mouse | Look around |
| Shift | Cycle throttle (x0.1 → x30) on the adaptive speed (speed = throttle x distance to nearest surface, per second) |
| G | Go to the selected object |
| Left click | Select the object under the center crosshair |
| I | Show/hide the info panel for the selection |
| B | Solar system browser (Up/Down, Enter to select, B/Esc to close) |
| `[` / `]` | Decrease / increase exposure (EV-5 to EV+20) |
| Enter | Open object search |
| Arrow Up/Down | Navigate search results |
| Enter (in search) | Select object, orient camera toward it |
| Esc (in search) | Close search |
| Mouse wheel / middle click | Zoom (FOV 120 deg to 0.01 deg) / reset |
| L | Labels on/off |
| Esc | Open/close pause menu |
| F3 | Toggle performance overlay (fps, frame times, entity count) |
| F12 | Save screenshot |
| Quit button | Exit cleanly via `AppExit::Success` |

## Known gotchas and past bugs

### 1. SOLAR_RADIUS_UNITS must NOT divide by 206265

The correct formula is:
```rust
pub const SOLAR_RADIUS_UNITS: f32 = 695_700.0 / 149_597_870.7; // ≈ 0.00465 AU
```

An earlier version included `/ 206_265.0 * PARSEC_TO_UNITS`, which is the parallax-distance formula, not a radius conversion. It made all star spheres ~206,000× too small and invisible. The test `test_solar_radius_units_is_correct_au` catches this.

### 2. Stars at real distances are sub-pixel — dynamic scaling required

Even with the correct physical radius, Sirius at 543,920 AU (2.64 pc) has a sphere radius of ~0.008 AU, which is ~0.0000017° on screen — far below one pixel. Whenever the camera moves, `update_star_brightness` scales each star's `Transform` so its angular diameter is always ≥ `MIN_STAR_ANGLE` (0.002 rad ≈ 2–3 pixels):

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

Bloom is added by `gpu_compat::add_bloom_if_supported`, not in the camera bundle: some GPUs (GL on
Mesa Haswell, issue #11) can't render bloom's `Rg11b10Ufloat` texture. Test old-driver paths with
`WGPU_BACKEND=gl cargo run` on the Intel iGPU (it has no `Rg11b10Ufloat` rendering there either).


Stars use high emissive values to appear bright. Without HDR (`Camera { hdr: true }`), values above 1.0 clip to white and lose color information. Without `Bloom::NATURAL`, a tiny bright sphere may not be visually obvious even if it's rendering correctly. Both are set in `setup_camera`.

### 5. Speed display threshold: c is ~0.002004 AU/s

```rust
pub const C_AU_PER_S: f64 = 299_792.458 / KM_PER_AU; // ≈ 0.002004 AU/s
```

`format_speed` switches to km/s only below 0.01c (~0.00002 AU/s). Passing 0.001 AU/s gives `"0.4990c"`, not km/s — this tripped up a test and is the expected behavior.

### 6. The starfield is real catalog data, on by default

The old 2,000 random background stars are gone. The pause-menu toggle now shows or hides the HYG starfield
(`ShowCatalogStars`, default `true`, test `test_catalog_stars_on_by_default`). The 58 curated stars are matched to
HYG entries by direction (`match_curated`, test `test_every_curated_star_matches_a_distinct_hyg_star`) and left
out of the starfield, so they're never drawn twice. All rendered brightness uses **visual** luminosity
(HYG `lum`). The curated table's luminosity is bolometric and only feeds the info panel. Starfield shader
constants come from Rust as uniforms (`StarfieldParams::new`), so keep them in one place: don't hard-code numbers
in `starfield.wgsl`. Data license: CC BY-SA 4.0, see `DATA_LICENSES.md`. Keep the pause-menu credit.

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

### 9. Close-range star brightness is capped

Point-source brightness (`L / d²`) reaches ~1e10 within a few AU of a star, and bloom then whites out the screen.
`star_brightness` caps it at `MAX_STAR_BRIGHTNESS` (30). Remove the cap only together with real auto-exposure.

### 11. Catalog stars: points far, spheres near

Starfield points use f32 chunk-local positions (error up to ~8 AU). Within `PROXY_RANGE_AU` (2000 AU) of a
catalog star, `hyg_bodies::update_star_proxy` gives that star's entity a real sphere (same `Star` brightness/size
system as the curated stars) and hides its point through the `hide` uniform. Don't remove the hide: a point drawn
a few AU off would sit visibly beside the sphere. The starfield blends additively (`AlphaMode::Add`, fragment
alpha 0). Opaque billboards would draw dim stars as black disks over things behind them.

### 12. Zoom changes the minimum point size

Points and min-size spheres use `zoom::min_star_angle(fov)`, not `MIN_STAR_ANGLE` directly, so a
star stays ~2-3 px wide at every FOV. Anything that caches sizes on "camera moved" must also
recompute on `CameraFov` changes (`update_star_brightness`, `update_body_visual_size`, the
starfield uniform).

### 13. Don't touch Assets or UI nodes when nothing changed

`Assets::iter_mut` marks *every* asset it visits as modified (a GPU re-upload), even if you don't
write. Keep a handle and compare before `get_mut` (see `starfield::sync_starfield_params`). Likewise
for UI: writing `Text` or `Node` re-runs layout, so compare first. Hide labels with
`Display::None` (skipped by layout), not `Visibility::Hidden`.

### 10. Benchmarks need an awake display

With the screen blanked, GNOME throttles presentation to a constant 250 ms per frame. Run `bash ~/bin/wake-screen.sh`
first (see `PERF.md`).

## Brightness model

```
emissive = 2^EV × 0.5 × luminosity_solar / dist_pc²
```

- `BRIGHTNESS_SCALE = 0.5` calibrated so Vega (L=40.12 L☉, d=7.68 pc) ≈ emissive 0.34 at EV 0.
- Spectral color (from `spectral_color`) is applied as a tint multiplied by brightness.
- Distance is clamped to a minimum of 1e-4 pc, and the result is capped at `MAX_STAR_BRIGHTNESS` (see gotcha 9).
- Exposure range: EV -5 (dim) to EV +20 (very sensitive). Each step doubles/halves all star brightness.

## Adding new catalog stars

Extend `CATALOG_STARS` in `stars.rs`. Each entry is:
```rust
("Name", ra_hours, dec_deg, dist_parsecs, spectral_class, radius_solar_radii, luminosity_solar)
```

The star is automatically added to `ObjectCatalog` (searchable) and rendered with correct position, size, and brightness.

## Iterative development workflow

Each feature lands as its own PR, and each one should:

1. **Test the logic headlessly.** Keep pure math in plain functions with unit tests. For behaviour
   across frames, use `testing::headless_app()`. It runs real Bevy schedules with `MinimalPlugins`
   and a fixed 60 Hz clock, with no window or GPU, so results are deterministic on CI. Drive input
   with `press_key`/`release_key` (key codes) or `tap_logical`/`type_text` (logical keys, which
   text input such as search reads). These send real `KeyboardInput` events, because setting
   `ButtonInput` directly gets cleared in `PreUpdate` before game systems see it. Keep systems that
   need to run headless free of render/asset parameters.
2. **Benchmark before merging.** Run `IP_BENCH=20 cargo run --release` and append the `IP_BENCH` line
   to `PERF.md`. For hot paths that scale with object count, add `#[ignore]` timing tests whose
   names start with `perf_`, and record those too.
3. **Keep it lightweight.** No new crate or Bevy feature without a reason in the PR. Prefer
   change-detection over per-frame work, and don't touch `Assets<…>` mutably every frame for
   every entity: each `get_mut` re-uploads that asset to the GPU.

## Running CI locally

```bash
# same checks as CI
cargo check --all-targets
cargo test --all-targets
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

To regenerate the star catalog: `python3 -I tools/build_hyg.py <hyg_v44.csv.gz> data/hyg_stars.bin`.

Linux builds additionally require: `libxkbcommon-dev pkg-config`. (Audio and gamepad support are compiled out, so `libasound2-dev`/`libudev-dev` are no longer needed.)
