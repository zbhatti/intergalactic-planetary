#!/usr/bin/env python3
"""Convert the NASA Exoplanet Archive "Planetary Systems Composite" table into IP's exoplanet file.

Download (CSV, the columns below):
    https://exoplanetarchive.ipac.caltech.edu/TAP/sync?query=select+pl_name,hostname,hip_name,ra,dec,
      sy_dist,sy_vmag,st_spectype,st_teff,st_rad,st_lum,st_mass,pl_orbsmax,pl_orbper,pl_orbeccen,
      pl_orbincl,pl_rade,pl_bmasse,pl_eqt,discoverymethod,disc_year+from+pscomppars&format=csv
Usage:
    python3 -I tools/build_exoplanets.py <pscomppars.csv> data/exoplanets.tsv

Output, tab-separated, empty field = unknown. Each host line is followed by its planet lines:
    H  hostname  hip  ra_deg  dec_deg  dist_pc  vmag  class  teff_k  radius_sun  log10_lum_sun  mass_sun
    P  name  a_au  period_days  ecc  incl_deg  radius_earth  mass_earth  eq_temp_k  method  year
Hosts without a distance are skipped. A missing semi-major axis is derived from the period and the
host mass (Kepler's third law); planets with neither are skipped.

Data: NASA Exoplanet Archive, operated by the California Institute of Technology under contract with
NASA under the Exoplanet Exploration Program. The archive asks for its acknowledgment to be shown.
"""

import csv
import hashlib
import sys
from collections import OrderedDict

CLASSES = "OBAFGKM"


def num(s, digits=5):
    """Compact float: `digits` significant digits, empty if unknown."""
    if s is None or s.strip() == "":
        return ""
    return f"{float(s):.{digits}g}"


def spectral_class(spect, teff):
    s = (spect or "").strip()
    if s and s[0] in CLASSES:
        return s[0]
    if teff:
        t = float(teff)
        for limit, cls in ((30000, "O"), (10000, "B"), (7500, "A"), (6000, "F"), (5200, "G"), (3700, "K")):
            if t >= limit:
                return cls
        return "M"
    return ""


def clean(s):
    return (s or "").replace("\t", " ").strip()


def main(src, dst):
    raw = open(src, "rb").read()
    sha = hashlib.sha256(raw).hexdigest()
    rows = list(csv.DictReader(raw.decode("utf-8").splitlines()))
    hosts = OrderedDict()
    for r in rows:
        hosts.setdefault(r["hostname"], []).append(r)

    out, n_hosts, n_planets, no_dist, no_orbit, derived_a = [], 0, 0, 0, 0, 0
    for host, planets in sorted(hosts.items()):
        h = planets[0]
        if not h["sy_dist"]:
            no_dist += 1
            continue
        lines = []
        for p in sorted(planets, key=lambda p: p["pl_name"]):
            a = p["pl_orbsmax"]
            if not a:
                if not p["pl_orbper"]:
                    no_orbit += 1
                    continue
                years = float(p["pl_orbper"]) / 365.25
                mass = float(h["st_mass"]) if h["st_mass"] else 1.0
                a = str((mass * years * years) ** (1.0 / 3.0))
                derived_a += 1
            lines.append("\t".join([
                "P", clean(p["pl_name"]), num(a), num(p["pl_orbper"], 7), num(p["pl_orbeccen"], 3),
                num(p["pl_orbincl"], 4), num(p["pl_rade"], 4), num(p["pl_bmasse"], 4),
                num(p["pl_eqt"], 4), clean(p["discoverymethod"]), clean(p["disc_year"]),
            ]))
        if not lines:
            continue
        hip = h["hip_name"].split()[1] if h["hip_name"] else ""
        out.append("\t".join([
            "H", clean(host), hip, num(h["ra"], 9), num(h["dec"], 9), num(h["sy_dist"], 6),
            num(h["sy_vmag"], 4), spectral_class(h["st_spectype"], h["st_teff"]), num(h["st_teff"], 4),
            num(h["st_rad"], 4), num(h["st_lum"], 4), num(h["st_mass"], 4),
        ]))
        out.extend(lines)
        n_hosts += 1
        n_planets += len(lines)

    text = "\n".join(out) + "\n"
    if not text.isascii():
        sys.exit("non-ASCII names: Bevy's default font can't draw them")
    open(dst, "w", encoding="ascii").write(text)
    manifest = (
        "IP exoplanet systems derived from the NASA Exoplanet Archive Planetary Systems Composite table (pscomppars)\n"
        "Source: https://exoplanetarchive.ipac.caltech.edu/ (TAP query in tools/build_exoplanets.py)\n"
        "Acknowledgment: This research has made use of the NASA Exoplanet Archive, which is operated by the California\n"
        "Institute of Technology, under contract with the National Aeronautics and Space Administration under the\n"
        "Exoplanet Exploration Program.\n"
        f"Source sha256: {sha}\n"
        f"Source rows: {len(rows)}\n"
        f"Hosts written: {n_hosts}\n"
        f"Planets written: {n_planets}\n"
        f"Skipped hosts without a distance: {no_dist}; planets without a or period: {no_orbit}\n"
        f"Semi-major axes derived from period and host mass: {derived_a}\n"
        "Built by: python3 -I tools/build_exoplanets.py <pscomppars.csv> data/exoplanets.tsv\n"
    )
    open(dst + ".manifest.txt", "w").write(manifest)
    sys.stdout.write(manifest)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
