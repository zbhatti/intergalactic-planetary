#!/usr/bin/env python3
"""Convert the HYG star database (v4.x CSV, optionally .gz) into IP's compact binary catalog.

Usage:
    python3 -I tools/build_hyg.py <hyg_v44.csv.gz> data/hyg_stars.bin

Output format (all little-endian):
    header : magic b"IPHYG\\0\\0\\0" (8 bytes), version u32 (=2), star count u32, name count u32
    stars  : count x { x f32, y f32, z f32  (parsecs, HYG equatorial frame: +X RA 0h, +Y RA 6h, +Z north)
                       lum f32               (visual luminosity, solar units)
                       hip u32               (Hipparcos number, 0 if none)
                       hyg u32               (HYG id)
                       class u8              (spectral class letter O B A F G K M, or 0 if unknown)
                       flags u8              (bit 0: distance unknown, placed at FALLBACK_PC)
                       pad 2 bytes }         = 28 bytes
    names  : name count x { star index u32, byte length u16, UTF-8 bytes }

Stars without a usable parallax distance (HYG dist >= 100000) are skipped, except naked-eye ones
(mag <= 7, covering every constellation-figure star): those are mostly distant supergiants (e.g. Polis, HIP 89341) that belong in the sky and
in constellation figures, so they're placed at FALLBACK_PC in their true direction, with luminosity
from their apparent magnitude at that distance, and flagged. The Sun (id 0) is skipped.
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
VERSION = 2
FALLBACK_PC = 1000.0
NAKED_EYE_MAG = 7.0
FLAG_DIST_UNKNOWN = 1
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
    rows = skipped_no_dist = fallback = 0
    for row in csv.DictReader(text.splitlines()):
        rows += 1
        if row["id"] == "0":
            continue  # the Sun is rendered separately
        flags = 0
        x, y, z = float(row["x"]), float(row["y"]), float(row["z"])
        if float(row["dist"]) >= 100000:
            if not (row["mag"] and float(row["mag"]) <= NAKED_EYE_MAG):
                skipped_no_dist += 1
                continue
            fallback += 1
            flags |= FLAG_DIST_UNKNOWN
            ra, dec = math.radians(float(row["ra"]) * 15.0), math.radians(float(row["dec"]))
            x = FALLBACK_PC * math.cos(dec) * math.cos(ra)
            y = FALLBACK_PC * math.cos(dec) * math.sin(ra)
            z = FALLBACK_PC * math.sin(dec)
            absmag = float(row["mag"]) - 5.0 * math.log10(FALLBACK_PC / 10.0)
            lum = 10 ** ((4.85 - absmag) / 2.5)
        else:
            lum = float(row["lum"]) if row["lum"] else 0.0
            if not math.isfinite(lum) or lum <= 0.0:
                # Recover visual luminosity from absolute magnitude when lum is missing.
                lum = 10 ** ((4.85 - float(row["absmag"])) / 2.5)
        cls = spectral_class(row["spect"], row["ci"])
        index = len(stars)
        stars.append(struct.pack(
            "<ffffIIBB2x",
            x, y, z,
            lum,
            int(row["hip"]) if row["hip"] else 0,
            int(row["id"]),
            ord(cls) if cls else 0,
            flags,
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
        f"Stars of mag <= 7 without a distance, placed at {FALLBACK_PC:.0f} pc and flagged: {fallback}\n"
        f"Skipped (no usable distance, fainter than mag {NAKED_EYE_MAG}): {skipped_no_dist}\n"
        "Built by: python3 -I tools/build_hyg.py <hyg_v44.csv.gz> data/hyg_stars.bin\n"
    )
    open(dst + ".manifest.txt", "w").write(manifest)
    sys.stdout.write(manifest)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
