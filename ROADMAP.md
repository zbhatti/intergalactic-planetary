# IP Roadmap — SpaceEngine feature parity (functionality first)

Features below are things SpaceEngine shows on screen or lets you do interactively. Each is checked by
**running SpaceEngine and looking**, then reimplemented from public data and published methods —
never from SpaceEngine's files or binaries (its license forbids that).

Ranking favours **distances and content** first: the goal of the early phases is to be able to boost
from Earth out across the Laniakea supercluster (~160 Mpc across) with real objects along the way.
Motion and time simulation come after that. Visual fidelity comes last, because the main dev machine
(Surface Book, Intel HD 520 by default) can't afford GPU-heavy iteration.

- **GPU cost:** Low = gizmo lines/UI/math, safe on the iGPU · Med = many more draw instances ·
  High = new shaders/textures/post-processing (defer).
- **Check in SE:** what to look at in SpaceEngine to define "done". Menu/key names are from general
  knowledge of SpaceEngine and should be confirmed against the installed copy before implementing.

## What IP has today

Free-fly camera (WASD + mouse), speed multiplier up to 10^6 c, Sun + 8 planets as plain spheres at
fixed 2026-06-25 positions (circular orbits, nothing moves), 58 catalog stars at true RA/Dec/distance
with luminosity-based brightness and exposure control, 2000 random decorative background stars, name
labels, search + auto-orient to target, on-screen reticle and off-screen arrow,
distance/speed/exposure HUD, pause menu, F12 screenshots. Positions are f32 with 1 pc = 1000 units.

## Phase 1 — Scale foundation ✅ (2026-10-16)

Without these, nothing beyond the Milky Way is reachable or renders without jitter.

Done: f64 `WorldPos` + hand-rolled floating origin (no `big_space` dependency, ~150 lines),
nearest-surface adaptive speed with throttle, G go-to with log-distance travel, HUD to Mly/Gly,
far plane 1e15 AU. Also fixed the star distance scale: it was compressed to 1000 AU/pc and is now
the real 206,265. Headless tests fly to a body ~5 Mpc away and check arrival and render precision.

| # | Feature | Check in SE | IP today | GPU |
|---|---|---|---|---|
| 1 | **Large-coordinate precision**: f64 positions + floating origin (camera stays near 0, world shifts). 1 Mpc = 10^9 units, far past f32's useful range. Evaluate the `big_space` crate (confirm a Bevy 0.16 release) vs. a hand-rolled origin shift | Fly between galaxies — no jitter or snapping | f32 transforms | Low (architectural) |
| 2 | **Distance-adaptive speed**: speed scales with distance to the nearest object, slow near planets and up to ~10^15 c between galaxy groups. At today's 10^6 c cap, crossing 1 Mpc takes ~3 years | Fly toward a planet — speed drops by itself; in deep space it climbs | Manual multiplier, 10^6 c max | Low |
| 3 | **Go-to travel**: smooth automatic flight to a selected object, stopping at a sensible viewing distance (log-scale interpolation across huge distances) | Select object → "go to" | Only orients the camera | Low |
| 4 | **Distance units in HUD up to Mpc/Gly**, and depth range that works from km to Gpc (reverse-Z / per-scale near/far planes) | HUD readout at intergalactic scale | AU/ly/pc only | Low |

## Phase 2 — Real content, local (solar neighbourhood → Milky Way)

| # | Feature | Check in SE | IP today | GPU |
|---|---|---|---|---|
| 5 | **Selection by clicking** objects/labels + **object info panel** (type, radius, mass, temperature, luminosity, spectral class, distance) | Click object → info panel | Search only; name + distance | Low |
| 6 | **Major moons, dwarf planets, notable small bodies**, parented to their primaries. Static positions at the epoch date until Phase 5 | Solar system browser / planet tree | Planets only | Low |
| 7 | **Solar system browser**: tree of star → planets → moons, click to select | Solar system browser window | None | Low |
| 8 | **Larger star catalog**: HYG (~120k stars) as **one instanced draw with camera-relative positions on the GPU**, not 120k entities. Measured in Phase 1: a full rebase over 120k entities costs 3.4 ms/frame on the i7-6600U. Only the brightest get labels | Night sky density from Earth | 58 stars | Med — profile on iGPU |
| 9 | **Exoplanet systems** from the NASA Exoplanet Archive attached to their host stars | Visit a known exoplanet host | None | Low–Med |
| 10 | **Field of view zoom** (mouse wheel), FOV in HUD | Zoom in on a distant object | Fixed FOV | Low |
| 11 | **Label/marker filtering** by type and by distance/brightness — essential once there are 10^5+ objects | Label/marker settings | One on/off toggle | Low |
| 12 | **Coordinate grids and constellation lines** (equatorial/ecliptic/galactic, IAU lines) | Grid and constellation toggles | None | Low (gizmos) |
| 13 | **Star browser**: nearest/brightest stars around the camera, sortable, click to go | Star browser window | None | Low |
| 14 | **Bookmarks / saved locations** (position, orientation, target) | Locations / bookmarks | None | Low |
| 14a | **Auto-exposure** (eye adaptation from the brightest visible flux, computed on the CPU) to replace the fixed close-range brightness cap | Approach a star: the sky dims, then the star's disk becomes visible | Manual EV + cap | Low |

## Phase 3 — Real content, large scale (Milky Way → Laniakea)

| # | Feature | Check in SE | IP today | GPU |
|---|---|---|---|---|
| 15 | **Milky Way as an object**: shape, size and the Sun's position in it, visible from outside | Fly out until the galaxy is visible | None | Med |
| 16 | **Star clusters and nebulae** (Messier/NGC basics) as markers/billboards | Cluster/nebula markers | None | Med |
| 17 | **Local Group and nearby galaxies** (~900 within ~11 Mpc) as point sprites/billboards sized by real diameter | Galaxy markers out to ~10 Mpc | None | Med |
| 18 | **Galaxies out to ~200 Mpc** from all-sky redshift/distance surveys, so Virgo, the Great Attractor region and Laniakea's structure are visible as you fly out | Supercluster-scale view | None | Med — profile on iGPU |
| 19 | **Galaxy browser / scale-aware labels**: label groups and clusters (Local Group, Virgo Cluster, …) instead of every galaxy at large scales | Labels at supercluster scale | None | Low |

**Milestone:** fly from Earth to the edge of Laniakea, using go-to and adaptive speed, with real
stars, clusters and galaxies visible along the way.

## Phase 4 — Simulated stars and galaxies (optional layer)

A toggle (pause menu, replacing today's random background stars) that fills in the
universe beyond what catalogs cover, using a **pre-generated population** drawn from distributions
fitted to known objects.

| # | Feature | Notes |
|---|---|---|
| 20 | **Simulated Milky Way stars** | Positions from a galactic density model (thin + thick disk, bulge, halo). Masses from an initial mass function, giving luminosity, radius, temperature and spectral class. Only generated where catalogs are incomplete (fainter than the catalog's magnitude limit, or beyond its distance), so simulated stars don't duplicate real ones |
| 21 | **Simulated galaxies** | Fill regions and magnitudes the surveys miss, following an observed luminosity function and morphology mix, clustered to follow the real large-scale structure from #18 rather than placed uniformly |
| 22 | **Clear real vs. simulated distinction** | Info panel and labels mark simulated objects; search defaults to real objects |
| 23 | **Reproducible generation** | Fixed seed, so every run and every machine sees the same universe; generator is an offline tool with tests for its distributions |
| 23a | **Star-local frames** | Planets of simulated (or exoplanet) systems far from the Sun need positions relative to their star. f64 heliocentric coordinates only resolve ~2000 km at 0.5 Mpc |

Open design choice: one **pre-generated data file** (simple, but caps the count to what's
reasonable to ship — millions, not 10^11), or **seeded generation per spatial cell** at runtime (same
reproducibility, unbounded count, more code). Decide when Phase 3's data format exists.

## Phase 5 — Time and motion

| # | Feature | Check in SE | GPU |
|---|---|---|---|
| 24 | **Simulation time**: date/time HUD, pause, speed up / reverse time, jump to now / a date | Time panel; speed time up and watch planets move | Low |
| 25 | **Real orbits**: Keplerian elements for planets, moons, small bodies, driven by #24 | Positions on a given date match | Low |
| 26 | **Orbit lines**, toggleable | Orbit display toggle | Low (gizmos) |
| 27 | **Camera modes**: center/track, follow (move with object), orbit around selected object | Center / follow / sync-orbit modes | Low |
| 28 | **Stellar proper motion** over long time spans (optional) | Speed time way up near the Sun | Low |

## Phase 6 — Visual fidelity (defer; GPU-heavy)

Textured planets, rings, atmosphere scattering, eclipses/shadows, star glare/diffraction, galaxy
and nebula volumes, procedural star systems/planets, surface terrain and landing, spacecraft, VR.
These make SpaceEngine look like SpaceEngine, but they mostly add shader and texture load. Test in
short sessions, and preferably on CI/other machines.

## Working rules

- Each item lands with unit tests for its math (coordinate conversion, speed curve, catalog parsing,
  generator distributions), so most verification stays headless.
- Profile Med-cost items on the iGPU before merging: keep a frame cap and watch draw counts.

## Public data sources (check each license before shipping data)

- Stars: HYG database (Hipparcos + Yale + Gliese); later Gaia DR3 subsets
- Exoplanets: NASA Exoplanet Archive (Planetary Systems table)
- Moons/small bodies: JPL Solar System Dynamics (satellite mean elements, small-body database)
- Planetary orbits: JPL "Approximate Positions of the Planets" (Standish), VSOP87
- Constellations: IAU boundaries; Stellarium sky-culture line data
- Clusters/nebulae: OpenNGC (CC-BY-SA — attribution required)
- Nearby galaxies: Updated Nearby Galaxy Catalog (Karachentsev et al., ~11 Mpc)
- Large-scale structure: 2MASS Redshift Survey (2MRS), Cosmicflows-4 distances
- Milky Way structure and star population models: published galactic models (e.g. Besançon model
  papers), Kroupa/Chabrier IMF, Schechter luminosity function for galaxies
