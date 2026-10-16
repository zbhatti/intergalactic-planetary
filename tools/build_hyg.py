#!/usr/bin/env python3
"""Convert the HYG star database (v4.x CSV, optionally .gz) into IP's compact binary catalog.

Usage:
    python3 -I tools/build_hyg.py <hyg_v44.csv.gz> data/hyg_stars.bin

Output format (all little-endian):
    header : magic b"IPHYG\\0\\0\\0" (8 bytes), version u32 (=1), star count u32, name count u32
    stars  : count x { x f32, y f32, z f32  (parsecs, HYG equatorial frame: +X RA 0h, +Y RA 6h, +Z north)
                       lum f32               (visual luminosity, solar units)
                       hip u32               (Hipparcos number, 0 if none)
                       hyg u32               (HYG id)
                       class u8              (spectral class letter O B A F G K M, or 0 if unknown)
                       pad 3 bytes }         = 28 bytes
    names  : name count x { star index u32, byte length u16, UTF-8 bytes }

Stars without a usable parallax distance (dist >= 100000) and the Sun (id 0) are skipped.
Writes <output>.manifest.txt next to the binary with the source hash and counts.

HYG is (c) David Nash / astronexus.com, licensed CC BY-SA 4.0; the output is a derived
database under the same license.
"""

import csv
import gzip
import hashlib
import math
import struct
import sys

MAGIC = b"IPHYG\0\0\0"
VERSION = 1
CLASSES = "OBAFGKM"


def class_from_ci(ci):
    """Spectral class from B-V colour index, via Ballesteros (2012) temperature."""
    t = 4600.0 * (1.0 / (0.92 * ci + 1.7) + 1.0 / (0.92 * ci + 0.62))
    for limit, cls in ((30000, "O"), (10000, "B"), (7500, "A"), (6000, "F"), (5200, "G"), (3700, "K")):
        if t >= limit:
            return cls
    return "M"


def spectral_class(spect, ci):
    s = (spect or "").strip()
    if s and s[0] in CLASSES:
        return s[0]
    if ci:
        try:
            return class_from_ci(float(ci))
        except (ValueError, ZeroDivisionError):
            pass
    return ""


def main(src, dst):
    raw = open(src, "rb").read()
    sha = hashlib.sha256(raw).hexdigest()
    text = gzip.decompress(raw).decode("utf-8") if src.endswith(".gz") else raw.decode("utf-8")

    stars, names = [], []
    rows = skipped_no_dist = skipped_no_dist_naked_eye = 0
    for row in csv.DictReader(text.splitlines()):
        rows += 1
        if row["id"] == "0":
            continue  # the Sun is rendered separately
        if float(row["dist"]) >= 100000:
            skipped_no_dist += 1
            if row["mag"] and float(row["mag"]) <= 6.5:
                skipped_no_dist_naked_eye += 1
            continue
        lum = float(row["lum"]) if row["lum"] else 0.0
        if not math.isfinite(lum) or lum <= 0.0:
            # Recover visual luminosity from absolute magnitude when lum is missing.
            lum = 10 ** ((4.85 - float(row["absmag"])) / 2.5)
        cls = spectral_class(row["spect"], row["ci"])
        index = len(stars)
        stars.append(struct.pack(
            "<ffffIIB3x",
            float(row["x"]), float(row["y"]), float(row["z"]),
            lum,
            int(row["hip"]) if row["hip"] else 0,
            int(row["id"]),
            ord(cls) if cls else 0,
        ))
        if row["proper"]:
            names.append((index, row["proper"].encode("utf-8")))

    with open(dst, "wb") as f:
        f.write(MAGIC + struct.pack("<III", VERSION, len(stars), len(names)))
        for s in stars:
            f.write(s)
        for index, name in names:
            f.write(struct.pack("<IH", index, len(name)) + name)

    manifest = (
        "IP star catalog derived from the HYG database\n"
        "Source: https://codeberg.org/astronexus/hyg (data/hyg/CURRENT/hyg_v44.csv.gz)\n"
        "License: CC BY-SA 4.0 (https://creativecommons.org/licenses/by-sa/4.0/); this file is a derived database under the same license\n"
        f"Source sha256: {sha}\n"
        f"Source rows: {rows}\n"
        f"Stars written: {len(stars)}\n"
        f"Proper names: {len(names)}\n"
        f"Skipped (no usable distance): {skipped_no_dist}, of which naked-eye (mag <= 6.5): {skipped_no_dist_naked_eye}\n"
        "Built by: python3 -I tools/build_hyg.py <hyg_v44.csv.gz> data/hyg_stars.bin\n"
    )
    open(dst + ".manifest.txt", "w").write(manifest)
    sys.stdout.write(manifest)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
