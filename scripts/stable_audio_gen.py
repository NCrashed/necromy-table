"""Generate music or ambience with Stable Audio 3 (run through scripts/stable-audio.sh).

Loads the model once, writes `count` variants as WAV. With --loop each variant
is made seamless: the take is rotated by half its length, so its end and start
meet in the middle as neighbouring samples of the original, and then the model
inpaints a window around that seam with the same prompt.
"""

import argparse
import os
import random
import time

import torch
import torchaudio

from stable_audio_3 import StableAudioModel


def splice(take, filled, sr, mid, seam, fade=0.5):
    """The take with the inpainted window from `filled` crossfaded in.

    The model redraws the whole clip while inpainting (and fades its end out
    like any track's), so only the window is taken from it: the wrap point
    stays the original's own continuous middle.
    """
    t = torch.arange(take.shape[-1]) / sr
    lo, hi = mid - seam / 2, mid + seam / 2
    ramp = torch.minimum((t - lo) / fade + 0.5, (hi - t) / fade + 0.5).clamp(0, 1)
    w = torch.sin(ramp * torch.pi / 2)  # equal power
    return take * torch.cos(ramp * torch.pi / 2) + filled * w


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--model", default="small-music")
    p.add_argument("--duration", type=float, default=60.0)
    p.add_argument("--count", type=int, default=1)
    p.add_argument("--steps", type=int, default=8)
    p.add_argument("--seed", type=int, default=-1)
    p.add_argument("--negative", default=None)
    p.add_argument("--loop", action="store_true")
    p.add_argument("--seam", type=float, default=10.0, help="inpainted window, seconds")
    p.add_argument("--out", required=True, help="output base path without extension")
    p.add_argument("prompt")
    a = p.parse_args()

    t0 = time.time()
    model = StableAudioModel.from_pretrained(a.model, device="cpu")
    sr = model.model.sample_rate
    print(f"model {a.model} loaded in {time.time() - t0:.1f}s, {sr} Hz", flush=True)

    for i in range(a.count):
        seed = a.seed + i if a.seed >= 0 else random.randrange(2**31)
        t0 = time.time()
        audio = model.generate(
            prompt=a.prompt,
            negative_prompt=a.negative,
            duration=a.duration,
            steps=a.steps,
            seed=seed,
        )[0].float().cpu()
        if a.loop:
            half = audio.shape[-1] // 2
            audio = torch.cat([audio[..., half:], audio[..., :half]], dim=-1)
            mid = audio.shape[-1] / sr / 2
            filled = model.generate(
                prompt=a.prompt,
                negative_prompt=a.negative,
                duration=a.duration,
                steps=a.steps,
                seed=seed,
                inpaint_audio=(sr, audio),
                inpaint_mask_start_seconds=mid - a.seam / 2,
                inpaint_mask_end_seconds=mid + a.seam / 2,
            )[0].float().cpu()[..., : audio.shape[-1]]
            audio = splice(audio, filled, sr, mid, a.seam)
        # Peak-normalise to -1 dBFS: takes vary a lot in level.
        audio = audio * (0.89 / audio.abs().max().clamp(min=1e-6))
        path = f"{a.out}-{seed}.wav"
        os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
        torchaudio.save(path, audio, sr)
        print(f"{path}  ({time.time() - t0:.1f}s)", flush=True)


if __name__ == "__main__":
    main()
