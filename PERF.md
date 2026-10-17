# Performance log

Every feature PR records a benchmark here, so regressions show up as soon as they land.

## How to measure

```bash
IP_BENCH=20 cargo run --release   # 20 s scripted run, prints one IP_BENCH line, then exits
cargo test --release -- --ignored --nocapture perf_   # CPU micro-benchmarks for hot paths
```

- Always run through `cargo` on the Surface Book. The git-ignored `.cargo/config.toml` sets
  `WGPU_POWER_PREF=low`, which keeps the run on the Intel HD 520 rather than the overheat-prone GTX 965M.
  Check the `AdapterInfo` log line.
- Runs are vsync-capped at 60 Hz, so `p50` sits near 16.7 ms whenever the frame budget is met. What to
  watch: `p95`/`max` creeping toward or past 16.7 ms, and `hitches` (frames > 1.5x median).
- Bench mode turns on every label and the background stars (worst case), then loops a go-to tour
  (override with `IP_BENCH_TOUR=Saturn,Titan` to screenshot specific objects, or `IP_BENCH_LOOK=Betelgeuse`
  to stay 1 AU from the Sun and just turn toward a target, for the sky as seen from home; `IP_BENCH_FOV=2` to zoom to a 2 deg field of view; `IP_BENCH_KEYS=B,I`
  presses keys at startup to put UI panels in the screenshot)
  (Earth -> Jupiter -> Sirius -> Deneb -> Sun) that covers planetary to ~800 pc scales.
  Before Phase 1 (2026-10-16 baseline) it panned 360 degrees instead, so rows before and after
  that change aren't strictly comparable.
- F3 toggles the same stats live in-game.
- **The display must be awake.** When GNOME has blanked the screen, presentation is throttled to
  4 fps (a constant 250 ms per frame), which says nothing about the game. Over SSH, run
  `bash ~/bin/wake-screen.sh` right before benchmarking. The bench prints a warning if the median
  frame exceeds 100 ms.

## Results (Surface Book 1, i7-6600U, Intel HD 520, Mesa, X11, 1 window at default size)

| Date | Change | Build | frames | mean ms | p95 ms | max ms | hitches | entities |
|---|---|---|---|---|---|---|---|---|
| 2026-10-16 | Baseline after Bevy feature trim + perf tooling | release | 1201 | 16.67 | 17.94 | 21.48 | 0 | 2156 |
| 2026-10-16 | Phase 1: f64 world + floating origin, adaptive speed, go-to tour bench | release | 1201 | 16.67 | 17.99 | 23.84 | 0 | 2156 |
| 2026-10-16 | Phase 2.1: click-select + info panel | release | 1201 | 16.67 | 17.65 | 22.99 | 0 | 2158 |
| 2026-10-16 | Phase 2.2: 5 dwarf planets + 21 moons, min apparent size for bodies (tour: Jupiter) | release | 481 | 16.67 | 17.53 | 19.56 | 0 | 2210 |
| 2026-10-16 | Phase 2.3: solar system browser open (tour: Saturn, keys: B) | release | 361 | 16.67 | 17.70 | 19.66 | 0 | 2211 |
| 2026-10-16 | Phase 2.4: HYG starfield, 109k stars / 623 chunks; 2000 random bg stars removed | release | 1201 | 16.67 | 17.57 | 27.72 | 1 | 835 |
| 2026-10-16 | Phase 2.5: named HYG entities (+483), sphere proxy, additive starfield (tour: Barnard's Star) | release | 720 | 16.67 | 17.45 | 21.67 | 0 | 1318 |
| 2026-10-17 | Old-GPU fixes (#10, #11): Vulkan | release | 1201 | 16.67 | 17.62 | 22.28 | 0 | 1319 |
| 2026-10-17 | Old-GPU fixes: `WGPU_BACKEND=gl` (bloom off, no GPU culling) | release | 1192 | 16.78 | 17.02 | 34.41 | 4 | 1319 |
| 2026-10-17 | Phase 2.10-11: zoom, filtered/decluttered labels (+540 named-star labels, mag limit 6 in bench) | release | 1200 | 16.67 | 17.64 | 18.89 | 0 | 1810 |

## Micro-benchmarks (`cargo test --release -- --ignored --nocapture perf_`)

Same machine, single-threaded, per call/frame.

| Date | Benchmark | n = 2,000 | n = 120,000 | Notes |
|---|---|---|---|---|
| 2026-10-16 | `perf_find_nearest` (nearest-surface scan) | 6.2 µs | 0.48 ms | Linear scan; fine up to ~10^5 |
| 2026-10-16 | `perf_rebase_sync` (camera moves + full rebase, headless frame) | 0.20 ms | 3.44 ms | 120k entities would cost ~20% of a 60 Hz frame while flying fast, so the HYG catalog must be an instanced draw, not entities |
| 2026-10-16 | `perf_nearest_catalog_star` (HYG, f64 scan; only on frames the camera moved) | — | 0.26 ms (n = 109,389) | Fine for now; a chunk-grid lookup would make it ~O(1) if needed |
