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

## Micro-benchmarks (`cargo test --release -- --ignored --nocapture perf_`)

Same machine, single-threaded, per call/frame.

| Date | Benchmark | n = 2,000 | n = 120,000 | Notes |
|---|---|---|---|---|
| 2026-10-16 | `perf_find_nearest` (nearest-surface scan) | 6.2 µs | 0.48 ms | Linear scan; fine up to ~10^5 |
| 2026-10-16 | `perf_rebase_sync` (camera moves + full rebase, headless frame) | 0.20 ms | 3.44 ms | 120k entities would cost ~20% of a 60 Hz frame while flying fast, so the HYG catalog must be an instanced draw, not entities |
