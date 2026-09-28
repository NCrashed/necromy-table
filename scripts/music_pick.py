"""Rank the takes of a track (run through scripts/music-keep.sh).

Prints the takes best first as `score path`. Lower is better:
- the jump across the loop point against a typical step (a click),
- the share of near-silent half seconds (the model sometimes stalls),
- the level change across the loop point (a bump every time it wraps).
"""

import sys

import numpy as np
import soundfile as sf


def score(path):
    a, sr = sf.read(path)
    a = a.reshape(len(a), -1)
    step = np.abs(np.diff(a, axis=0)).mean() + 1e-9
    click = np.abs(a[0] - a[-1]).mean() / step
    w = sr // 2
    rms = np.sqrt((a[: len(a) // w * w].reshape(-1, w, a.shape[1]) ** 2).mean(axis=(1, 2)))
    db = 20 * np.log10(rms + 1e-9)
    silent = (db < db.max() - 40).mean()
    bump = abs(db[0] - db[-1]) / 6
    return click / 4 + silent * 10 + bump


takes = sorted(sys.argv[1:], key=score)
for t in takes:
    print(f"{score(t):.2f} {t}")
