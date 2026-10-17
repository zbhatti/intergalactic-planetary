# Data licenses

IP's code is the project's own. It embeds third-party data, which keeps its original license:

| Data | Files | Source | License |
|---|---|---|---|
| HYG star database v4.4 (109,819 stars: those with distances, plus 430 stars of mag 7 or brighter without one) | `data/hyg_stars.bin` (derived; see `data/hyg_stars.bin.manifest.txt`) | David Nash / astronexus.com, https://codeberg.org/astronexus/hyg | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). `data/hyg_stars.bin` is a derived database and is distributed under the same license |
| IAU constellation figures (88 constellations, 695 lines) | `data/constellations.txt` (derived; see `data/constellations.txt.manifest.txt`) | Stellarium's team, "modern" sky culture, https://github.com/Stellarium/stellarium/tree/master/skycultures/modern | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). Derived database, distributed under the same license |
| Exoplanet systems (4,813 hosts, 6,416 planets) | `data/exoplanets.tsv` (derived; see `data/exoplanets.tsv.manifest.txt`) | NASA Exoplanet Archive, Planetary Systems Composite table, https://exoplanetarchive.ipac.caltech.edu/ | Freely usable; the archive asks for this acknowledgment: "This research has made use of the NASA Exoplanet Archive, which is operated by the California Institute of Technology, under contract with the National Aeronautics and Space Administration under the Exoplanet Exploration Program." |
| Nebulae (168 H II regions, 52 reflection nebulae) | `data/nebulae.tsv` (derived; see `data/nebulae.tsv.manifest.txt`) | Sharpless (1959, VII/20), van den Bergh (1966, VII/21), distances from Mendez-Delgado et al. (2022, MNRAS 510, 4436), Foster & Brunt (2015, J/AJ/150/147), Anderson et al. (2014, J/ApJS/212/1); via VizieR, CDS Strasbourg | Published scientific catalogs, used with citation. "This research has made use of the VizieR catalogue access tool, CDS, Strasbourg, France." |
| Planet orbital elements | `src/solar_system.rs` (`PLANETS`) | NASA/JPL, "Approximate Positions of the Planets" (E. M. Standish) | US government work, public domain |
| Dwarf planet, asteroid and comet orbital elements | `src/solar_system.rs` (`DWARF_PLANETS`, `SMALL_BODIES`) | NASA/JPL Small-Body Database API | US government work, public domain |
| Moon mean orbital elements | `src/solar_system.rs` (`MOONS`) | NASA/JPL Solar System Dynamics, planetary satellite mean elements | US government work, public domain |

The app shows the HYG, Stellarium, NASA Exoplanet Archive and nebula catalog attributions in the pause menu (Esc).

To regenerate `data/hyg_stars.bin` from a new HYG release: `python3 -I tools/build_hyg.py <hyg_vNN.csv.gz> data/hyg_stars.bin`.
To regenerate `data/exoplanets.tsv`: download the CSV with the TAP query in `tools/build_exoplanets.py`, then `python3 -I tools/build_exoplanets.py <pscomppars.csv> data/exoplanets.tsv`.
To regenerate `data/nebulae.tsv`: download the VizieR files listed in `tools/build_nebulae.py`, then `python3 -I tools/build_nebulae.py <dir> <hyg_v44.csv.gz> data/nebulae.tsv`.
To regenerate `data/constellations.txt`: `python3 -I tools/build_constellations.py <skycultures/modern/index.json> data/constellations.txt`.
