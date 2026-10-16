# Data licenses

IP's code is the project's own. It embeds third-party data, which keeps its original license:

| Data | Files | Source | License |
|---|---|---|---|
| HYG star database v4.4 (109,389 stars with distances) | `data/hyg_stars.bin` (derived; see `data/hyg_stars.bin.manifest.txt`) | David Nash / astronexus.com, https://codeberg.org/astronexus/hyg | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). `data/hyg_stars.bin` is a derived database and is distributed under the same license |
| Planet orbital elements | `src/solar_system.rs` (`PLANETS`) | NASA/JPL, "Approximate Positions of the Planets" (E. M. Standish) | US government work, public domain |
| Dwarf planet orbital elements | `src/solar_system.rs` (`DWARF_PLANETS`) | NASA/JPL Small-Body Database API | US government work, public domain |
| Moon mean orbital elements | `src/solar_system.rs` (`MOONS`) | NASA/JPL Solar System Dynamics, planetary satellite mean elements | US government work, public domain |

The app shows the HYG attribution in the pause menu (Esc).

To regenerate `data/hyg_stars.bin` from a new HYG release: `python3 -I tools/build_hyg.py <hyg_vNN.csv.gz> data/hyg_stars.bin`.
