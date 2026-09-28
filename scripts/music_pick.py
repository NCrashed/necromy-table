"""Rank the takes of a track (run through scripts/music-keep.sh).

Prints the takes best first as `score path`. Lower is better:
- the jump across the loop point against a typical step (a click),
- the share of near-silent half seconds (the model sometimes stalls),
- the level change across the loop point (a bump every time it wraps),
- instruments out of tune with each other (`tuning` below 0.7).
"""

import sys

import numpy as np
import soundfile as sf
import torch


def median(x, k, dim):
    """Median filter of a spectrogram along time (dim 0) or frequency (1)."""
    x = x[None, None]
    if dim == 0:
        x = torch.nn.functional.pad(x, (0, 0, k // 2, k // 2), mode="reflect")
        return x.unfold(2, k, 1).median(-1).values[0, 0]
    x = torch.nn.functional.pad(x, (k // 2, k // 2, 0, 0), mode="reflect")
    return x.unfold(3, k, 1).median(-1).values[0, 0]


def tuning(mono, sr):
    """How well the spectral peaks agree on one tuning, 0..1.

    Each strong peak between 80 Hz and 2 kHz is placed within its semitone
    (cents mod 100) as an angle; the length of their weighted mean is 1 when
    every note sits on the same grid, whatever the grid's offset from A440,
    and falls towards 0 when parts disagree or pitch wobbles. Drums are
    taken out first (harmonic-percussive separation by median filters), so
    busy percussion does not read as out of tune.
    """
    n, hop = 8192, 4096
    win = np.hanning(n)
    frames = np.stack([mono[i : i + n] * win for i in range(0, len(mono) - n, hop)])
    freqs = np.fft.rfftfreq(n, 1 / sr)
    band = (freqs > 80) & (freqs < 2000)
    f = freqs[band]
    spec = torch.from_numpy(np.abs(np.fft.rfft(frames, axis=1))[:, band]).float()
    sustained = median(spec, 9, 0)
    hits = median(spec, 17, 1)
    spec = (spec * sustained**2 / (sustained**2 + hits**2 + 1e-12)).numpy()
    vec, total = 0j, 0.0
    for y in spec:
        mid = y[1:-1]
        peaks = np.where((mid > y[:-2]) & (mid > y[2:]) & (mid > np.median(y) * 8))[0] + 1
        if len(peaks) == 0:
            continue
        peaks = peaks[np.argsort(y[peaks])[-12:]]
        a, b, c = (np.log(y[peaks + k] + 1e-12) for k in (-1, 0, 1))
        exact = f[peaks] + 0.5 * (a - c) / (a - 2 * b + c + 1e-12) * (f[1] - f[0])
        cents = 1200 * np.log2(exact / 440.0)
        vec += (y[peaks] * np.exp(2j * np.pi * (cents % 100) / 100)).sum()
        total += y[peaks].sum()
    return abs(vec) / total if total else 0.0


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
    tune = tuning(a.mean(axis=1), sr)
    return click / 4 + silent * 10 + bump + max(0.0, 0.7 - tune) * 10, tune


ranked = sorted((score(t), t) for t in sys.argv[1:])
for (s, tune), t in ranked:
    print(f"{s:.2f} {t} tuning {tune:.2f}")
