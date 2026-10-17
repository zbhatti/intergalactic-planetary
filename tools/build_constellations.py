#!/usr/bin/env python3
"""Extract IAU constellation stick figures from Stellarium's "modern" sky culture.

Usage:
    python3 -I tools/build_constellations.py <skycultures/modern/index.json> data/constellations.txt

Output: one line per constellation, tab-separated:
    abbreviation <TAB> Latin name <TAB> English name <TAB> polylines
where polylines are separated by ';' and each is a space-separated list of Hipparcos numbers.
Names are transliterated to ASCII (Bevy's default font has no other glyphs).
IP resolves the HIP numbers against the embedded HYG catalog at startup.

Source: https://github.com/Stellarium/stellarium/tree/master/skycultures/modern
(c) Stellarium's team. Text and data: CC BY-SA 4.0; the output is a derived database under the
same license. (The illustrations, under the Free Art License, aren't used.)
"""

import hashlib
import json
import sys
import unicodedata


def ascii_name(s):
    """Bevy's default font is ASCII-only: 'Bo\u00f6tes' -> 'Bootes'."""
    out = unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode("ascii")
    if not out:
        sys.exit(f"no ASCII form for {s!r}")
    return out


def main(src, dst):
    raw = open(src, "rb").read()
    sha = hashlib.sha256(raw).hexdigest()
    data = json.loads(raw)
    lines, segments = [], 0
    for c in data["constellations"]:
        abbr = c["id"].split()[-1]
        name = c["common_name"]
        polylines = []
        for poly in c["lines"]:
            hips = [int(h) for h in poly]
            segments += len(hips) - 1
            polylines.append(" ".join(map(str, hips)))
        native = ascii_name(name["native"])
        english = ascii_name(name.get("english", name["native"]))
        fields = [abbr, native, english, ";".join(polylines)]
        if any("\t" in f or "\n" in f for f in fields):
            sys.exit(f"bad characters in {abbr}")
        lines.append("\t".join(fields))
    lines.sort()
    with open(dst, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    manifest = (
        "IP constellation figures derived from Stellarium's modern sky culture\n"
        "Source: https://github.com/Stellarium/stellarium/tree/master/skycultures/modern (index.json)\n"
        "License: CC BY-SA 4.0 (https://creativecommons.org/licenses/by-sa/4.0/), (c) Stellarium's team; "
        "this file is a derived database under the same license\n"
        f"Source sha256: {sha}\n"
        f"Constellations: {len(lines)}\n"
        f"Line segments: {segments}\n"
        "Built by: python3 -I tools/build_constellations.py <index.json> data/constellations.txt\n"
    )
    open(dst + ".manifest.txt", "w").write(manifest)
    sys.stdout.write(manifest)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
