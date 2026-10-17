#!/usr/bin/env python3
"""Downsample the Edenhofer et al. (2023) 3D dust map into IP's embedded dust grid.

Source: "A parsec-scale Galactic 3D dust map out to 1.25 kpc from the Sun", Edenhofer, Zucker,
Frank, Saydjari, Speagle, Finkbeiner & Ensslin (2024, A&A 685, A82), dataset
https://zenodo.org/records/10658339 (CC BY 4.0), file mean_and_std_xyz.fits: posterior mean dust
density on a heliocentric Galactic Cartesian grid, 1251^3 voxels of 2 pc from -1250 to +1250 pc,
big-endian float32, MEAN extension data starting at byte 5760, X fastest, then Y, then Z.

Only the planes with |z| <= ~400 pc are needed (the Galactic dust layer; the paper's own figures use
the same slab). Fetch them with HTTP range requests, one file per plane (k = 427..822):
    PLANE=$((1251*1251*4)); START=$((5760 + k*PLANE))
    curl -r $START-$((START+PLANE-1)) -o plane_$(printf %04d $k).bin \\
        "https://zenodo.org/records/10658339/files/mean_and_std_xyz.fits?download=1"
Usage (needs numpy):
    python3 -I tools/build_dust.py <planes_dir> data/dust_edenhofer.bin

Units: the map is in "E" of Zhang, Green & Rix (2023) per parsec. Edenhofer et al. (Sec. 5)
convert to Johnson V (540 nm) with A_V = 2.8 E; this tool stores A_V per parsec.

Output format (little-endian):
    magic b"IPDUST\\0\\0", version u32 (=1), nx u32, ny u32, nz u32,
    voxel_pc f32, min_x_pc f32, min_y_pc f32, min_z_pc f32 (corner of the first voxel),
    log_lo f32, log_hi f32, pad f32 (=0)   -> 52-byte header
    nx*ny*nz u8 (X fastest, then Y, then Z): 0 = no dust, else A_V/pc = 10^(log_lo + (v-1)/254 (log_hi-log_lo))
Voxels without data (inside the map's inner 69 pc, the Local Bubble, which is nearly dust-free,
and outside its 1.25 kpc sphere) are stored as 0.
"""

import hashlib
import os
import struct
import sys

import numpy as np

N = 1251
VOXEL_PC = 2.0
FACTOR = 6                 # 2 pc -> 12 pc
K_FIRST, K_LAST = 427, 822  # source planes kept: z = -396 .. +394 pc
AV_PER_E = 2.8
LOG_LO, LOG_HI = -6.0, 0.0  # A_V per pc stored over 1e-6 .. 1


def encode(av):
    out = np.zeros(av.shape, dtype=np.uint8)
    pos = av > 10 ** LOG_LO
    v = 1 + np.round((np.log10(np.clip(av[pos], 10 ** LOG_LO, 10 ** LOG_HI)) - LOG_LO)
                     / (LOG_HI - LOG_LO) * 254)
    out[pos] = v.astype(np.uint8)
    return out


def main(planes_dir, dst):
    nk = K_LAST - K_FIRST + 1
    assert nk % FACTOR == 0
    nb = N // FACTOR           # 208 blocks (the last 3 source voxels are dropped)
    nz = nk // FACTOR          # 66
    sha = hashlib.sha256()
    out = np.zeros((nz, nb, nb), dtype=np.float64)
    stack = []
    for k in range(K_FIRST, K_LAST + 1):
        path = os.path.join(planes_dir, f"plane_{k:04d}.bin")
        raw = open(path, "rb").read()
        sha.update(raw)
        plane = np.frombuffer(raw, dtype=">f4").reshape(N, N)[: nb * FACTOR, : nb * FACTOR]
        stack.append(plane.astype(np.float64))
        if len(stack) == FACTOR:
            block = np.stack(stack)  # (z, y, x)
            b = block.reshape(FACTOR, nb, FACTOR, nb, FACTOR)
            with np.errstate(invalid="ignore"):
                mean = np.nanmean(b, axis=(0, 2, 4))
            out[(k - K_FIRST) // FACTOR] = np.nan_to_num(mean, nan=0.0)
            stack = []
    av = out * AV_PER_E
    enc = encode(av)
    min_xyz = -1250.0 - VOXEL_PC / 2  # first source voxel is centred on -1250
    min_z = -1250.0 + K_FIRST * VOXEL_PC - VOXEL_PC / 2
    header = b"IPDUST\0\0" + struct.pack(
        "<IIIIfffffff", 1, nb, nb, nz, VOXEL_PC * FACTOR, min_xyz, min_xyz, min_z, LOG_LO, LOG_HI, 0.0
    )
    with open(dst, "wb") as f:
        f.write(header)
        f.write(enc.tobytes(order="C"))  # (z, y, x) C order = X fastest
    manifest = (
        "IP 3D dust grid derived from Edenhofer et al. (2024, A&A 685, A82) 3D dust map\n"
        "Source: https://zenodo.org/records/10658339 mean_and_std_xyz.fits (MEAN), CC BY 4.0\n"
        f"Source planes: z index {K_FIRST}..{K_LAST} (z = {-1250 + 2 * K_FIRST} .. {-1250 + 2 * K_LAST} pc)\n"
        f"Source sha256 (concatenated planes): {sha.hexdigest()}\n"
        f"Grid: {nb} x {nb} x {nz} voxels of {VOXEL_PC * FACTOR:.0f} pc (Galactic X toward the Galactic centre, Z north)\n"
        f"Units: A_V per pc = {AV_PER_E} x E(ZGR23), log-encoded u8 over 1e{LOG_LO:.0f} .. 1e{LOG_HI:.0f}\n"
        f"Max A_V/pc: {av.max():.4g}; voxels with dust: {(enc > 0).mean() * 100:.1f}%\n"
        "Built by: python3 -I tools/build_dust.py <planes_dir> data/dust_edenhofer.bin (see the script for the download)\n"
    )
    open(dst + ".manifest.txt", "w").write(manifest)
    sys.stdout.write(manifest)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
