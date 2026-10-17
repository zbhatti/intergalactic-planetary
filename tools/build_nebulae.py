#!/usr/bin/env python3
"""Build IP's nebula catalog from published catalogs of emission (H II) and reflection nebulae.

Usage:
    python3 -I tools/build_nebulae.py <src_dir> <hyg_v44.csv.gz> data/nebulae.tsv

<src_dir> holds these VizieR (CDS) files, downloaded from https://cdsarc.cds.unistra.fr/ftp/<catalog>/:
    VII_20_catalog.dat        Sharpless (1959) catalogue of H II regions        (VII/20)
    VII_21_catalog.dat        van den Bergh (1966) reflection nebulae           (VII/21)
    J_AJ_150_147_table2.dat   Foster & Brunt (2015) stellar distances to H II   (J/AJ/150/147)
    J_ApJS_212_1_table2.dat   Anderson et al. (2014) WISE H II catalog          (J/ApJS/212/1)
    J_ApJS_212_1_table6.dat   ... and its distances

Output, tab-separated, one nebula per line:
    kind(E|R)  name[|alias]  glon_deg  glat_deg  dist_pc  radius_arcmin  brightness_class(1-5)  dist_source  extra
Emission (E): Sharpless H II regions with a distance, from (in order of preference) the Gaia EDR3
parallaxes of Mendez-Delgado et al. (2022, MNRAS 510, 4436, Table 1), Foster & Brunt (2015), or the
WISE catalog (Anderson et al. 2014; kinematic or parallax). Regions without any distance are skipped.
Brightness class is Sharpless' 1 (faintest) .. 3 (brightest). `extra` is unused.
Reflection (R): van den Bergh Type I and I-II nebulae (star embedded in the nebulosity) whose
illuminating star (by HD number) is in HYG with a distance; the position and distance are the
star's. Type II (star outside the nebula, e.g. the Witch Head next to Rigel) are skipped: the
catalogue doesn't give where the nebula itself is. Brightness class from the catalogue's blue-plate
surface brightness: vF=1, F=2, M=3, Br=4, vBr=5. `extra` is "<HIP number (0 if none)>:<observed colour code>" (vB, B, mB, I, mR, R, vR; van den Bergh's
plate colours, kept to check IP's computed colours).
"""

import csv
import gzip
import hashlib
import re
import statistics
import sys

# Mendez-Delgado et al. (2022), MNRAS 510, 4436, Table 1: heliocentric distances (kpc), mostly from
# Gaia EDR3 parallaxes of the ionizing stars. Non-Sharpless entries map to their Sharpless number
# where one exists.
MD22_KPC = {
    29: 1.15, 32: 1.46, 47: 1.54, 48: 3.59, 53: 3.38, 54: 1.91, 57: 2.10, 61: 2.39, 82: 0.78,
    83: 16.12, 88: 2.06, 90: 3.36, 93: 3.64, 98: 11.24, 100: 11.40, 127: 10.13, 128: 7.27,
    132: 4.53, 152: 4.54, 156: 2.56, 158: 2.84, 175: 2.02, 203: 2.39, 206: 2.96, 207: 3.59,
    208: 4.02, 209: 9.33, 212: 6.65, 219: 4.16, 228: 2.56, 235: 1.66, 237: 2.07, 255: 1.96,
    257: 1.96, 266: 4.60, 270: 8.08, 271: 3.25, 283: 5.37, 285: 4.38, 288: 5.06, 297: 1.08,
    298: 4.12, 301: 3.21, 308: 1.51, 311: 5.28,
    125: 0.76,  # IC 5146
    162: 2.83,  # NGC 7635
    105: 1.67,  # NGC 6888
    25: 1.22,   # M8
    49: 1.71,   # M16
    45: 1.82,   # M17
    30: 1.42,   # M20
    281: 0.41,  # M42
}

# Common names (IDs from SIMBAD cross-identifications).
SHARPLESS_NAMES = {
    281: "Orion Nebula (M42)", 25: "Lagoon Nebula (M8)", 49: "Eagle Nebula (M16)",
    45: "Omega Nebula (M17)", 30: "Trifid Nebula (M20)", 162: "Bubble Nebula (NGC 7635)",
    125: "Cocoon Nebula (IC 5146)", 105: "Crescent Nebula (NGC 6888)",
    117: "North America Nebula (NGC 7000)", 275: "Rosette Nebula", 220: "California Nebula",
    190: "Heart Nebula (IC 1805)", 199: "Soul Nebula (IC 1848)", 264: "Lambda Orionis Ring",
    276: "Barnard's Loop", 155: "Cave Nebula", 184: "Pacman Nebula (NGC 281)",
    277: "Flame Nebula (NGC 2024)", 279: "Running Man Nebula (NGC 1977)",
    229: "Flaming Star Nebula (IC 405)", 236: "IC 410", 238: "Hind's Variable Nebula",
}

BRIGHTNESS_R = {"vF": 1, "F": 2, "M": 3, "Br": 4, "vBr": 5}


def sharpless(path):
    out = {}
    for line in open(path):
        n = int(line[0:4])
        out[n] = {
            "glon": int(line[12:16]) / 10.0,
            "glat": int(line[16:20]) / 10.0,
            "diam": int(line[48:52]),
            "bright": int(line[54]),
        }
    return out


def foster_brunt(path):
    out = {}
    for line in open(path):
        m = re.match(r"Sh2-(\d+)", line[2:12].strip())
        if m:
            out[int(m.group(1))] = float(line[50:55])
    return out


def wise(table2, table6):
    dist = {}
    for line in open(table6):
        d = line[43:47].strip()
        if d:
            dist[line[0:15]] = float(d)
    out = {}
    for line in open(table2):
        if line[0:15] not in dist:
            continue
        for m in re.finditer(r"\bS(\d+)\b", line[39:75]):
            out.setdefault(int(m.group(1)), []).append(dist[line[0:15]])
    return {n: statistics.median(ds) for n, ds in out.items()}


def main(src, hyg_path, dst):
    sha = hashlib.sha256()
    files = ["VII_20_catalog.dat", "VII_21_catalog.dat", "J_AJ_150_147_table2.dat",
             "J_ApJS_212_1_table2.dat", "J_ApJS_212_1_table6.dat"]
    for f in files:
        sha.update(open(f"{src}/{f}", "rb").read())

    sh = sharpless(f"{src}/VII_20_catalog.dat")
    fb = foster_brunt(f"{src}/J_AJ_150_147_table2.dat")
    wi = wise(f"{src}/J_ApJS_212_1_table2.dat", f"{src}/J_ApJS_212_1_table6.dat")

    rows, counts = [], {"MD22": 0, "FB15": 0, "WISE": 0}
    skipped_e = 0
    for n, s in sorted(sh.items()):
        if n in MD22_KPC:
            d, src_tag = MD22_KPC[n], "MD22"
        elif n in fb:
            d, src_tag = fb[n], "FB15"
        elif n in wi:
            d, src_tag = wi[n], "WISE"
        else:
            skipped_e += 1
            continue
        counts[src_tag] += 1
        # Display name, then '|' and the catalog designation as a searchable alias.
        name = f"{SHARPLESS_NAMES[n]}|Sh2-{n}" if n in SHARPLESS_NAMES else f"Sh2-{n}"
        rows.append(["E", name, f"{s['glon']:.1f}", f"{s['glat']:.1f}", f"{d * 1000:.0f}",
                     f"{s['diam'] / 2:.1f}", str(s["bright"]), src_tag, ""])

    hd = {}
    for r in csv.DictReader(gzip.open(hyg_path, "rt")):
        if r["hd"] and float(r["dist"]) < 100000:
            hd[int(r["hd"])] = r
    skipped_r = 0
    skipped_type = 0
    n_r = 0
    for line in open(f"{src}/VII_21_catalog.dat"):
        num = int(line[1:4])
        if line[50:56].strip() not in ("I", "I-II"):
            skipped_type += 1
            continue
        hd_s = line[16:22].strip()
        star = hd.get(int(hd_s)) if hd_s else None
        if star is None:
            skipped_r += 1
            continue
        b_rad = line[70:75].strip()
        r_rad = line[76:81].strip()
        radius = max(float(b_rad or 0), float(r_rad or 0)) or 2.0
        sb = line[57:61].strip().rstrip(":")
        # Galactic coordinates of the star, from HYG's equatorial position.
        x, y, z = float(star["x"]), float(star["y"]), float(star["z"])
        glon, glat = equatorial_to_galactic(x, y, z)
        proper = star["proper"]
        name = f"vdB {num}" + (f" ({proper})" if proper else "")
        color_code = line[61:65].strip().rstrip(":")
        rows.append(["R", name, f"{glon:.3f}", f"{glat:.3f}", f"{float(star['dist']):.1f}",
                     f"{radius:.1f}", str(BRIGHTNESS_R.get(sb, 2)), "HYG",
                     f"{star['hip'] or '0'}:{color_code}"])
        n_r += 1

    text = "".join("\t".join(r) + "\n" for r in rows)
    if not text.isascii():
        sys.exit("non-ASCII names: Bevy's default font can't draw them")
    open(dst, "w", encoding="ascii").write(text)
    manifest = (
        "IP nebulae from published catalogs (via VizieR, CDS Strasbourg)\n"
        "Emission: Sharpless (1959, ApJS 4, 257; VII/20) positions/sizes/brightness classes; distances from\n"
        "  Mendez-Delgado et al. (2022, MNRAS 510, 4436, Table 1), Foster & Brunt (2015, AJ 150, 147; J/AJ/150/147),\n"
        "  Anderson et al. (2014, ApJS 212, 1; J/ApJS/212/1)\n"
        "Reflection: van den Bergh (1966, AJ 71, 990; VII/21) joined by HD number to HYG v4.4 stars\n"
        "This research has made use of the VizieR catalogue access tool, CDS, Strasbourg, France.\n"
        f"Source sha256 (concatenated VizieR files): {sha.hexdigest()}\n"
        f"Emission nebulae written: {sum(counts.values())} (MD22 {counts['MD22']}, FB15 {counts['FB15']}, "
        f"WISE {counts['WISE']}); skipped without a distance: {skipped_e}\n"
        f"Reflection nebulae written: {n_r}; skipped: {skipped_type} not Type I/I-II (star outside the "
        f"nebula), {skipped_r} star not in HYG with a distance\n"
        "Built by: python3 -I tools/build_nebulae.py <src_dir> <hyg_v44.csv.gz> data/nebulae.tsv\n"
    )
    open(dst + ".manifest.txt", "w").write(manifest)
    sys.stdout.write(manifest)


def equatorial_to_galactic(x, y, z):
    """ICRS cartesian -> Galactic (l, b) in degrees (IAU 1958 rotation, J2000 values)."""
    import math
    # Rows: Galactic X, Y, Z axes in ICRS coordinates.
    m = ((-0.0548755604, -0.8734370902, -0.4838350155),
         (0.4941094279, -0.4448296300, 0.7469822445),
         (-0.8676661490, -0.1980763734, 0.4559837762))
    gx, gy, gz = (r[0] * x + r[1] * y + r[2] * z for r in m)
    r = math.sqrt(gx * gx + gy * gy + gz * gz)
    return math.degrees(math.atan2(gy, gx)) % 360.0, math.degrees(math.asin(gz / r))


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2], sys.argv[3])
