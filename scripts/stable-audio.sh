#!/usr/bin/env bash
# Music and ambience with Stable Audio 3 Small, locally on the CPU.
#
#   scripts/stable-audio.sh [options] NAME "prompt"
#   scripts/stable-audio.sh setup      # clone and install into $SA3_DIR
#
#   -m MODEL    small-music | small-sfx (default small-music)
#   -d SECONDS  length, up to 120 (default 60)
#   -n COUNT    number of variants (default 2)
#   -s SEED     first seed (default random; variants take SEED, SEED+1, ...)
#   -N TEXT     negative prompt (default "poor quality")
#   -l          make each variant a seamless loop (inpaints the seam)
#   -S          skip the shared style prefix from art/music-style.txt
#
# Writes art/audio/NAME-SEED.wav (gitignored). Keep a take as
#   ffmpeg -i art/audio/NAME-SEED.wav -c:a libvorbis -q:a 5 assets/music/NAME.ogg
# The weights are gated on Hugging Face: accept the licence on the model pages
# (stabilityai/stable-audio-3-small-music, -small-sfx, SAME-S) and put HF_TOKEN
# in .env. Needs `nix develop` (uv, NECROMY_PYLIBS).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
sa3="${SA3_DIR:-$HOME/models/stable-audio-3}"
style_file="$root/art/music-style.txt"

usage() { sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

if [ "${1:-}" = setup ]; then
  [ -d "$sa3" ] || git clone --depth 1 https://github.com/Stability-AI/stable-audio-3 "$sa3"
  cd "$sa3"
  export UV_PYTHON_DOWNLOADS=never UV_HTTP_TIMEOUT=300
  uv venv --allow-existing
  # CPU wheels: the project's own lock pulls CUDA torch (gigabytes, useless here).
  uv pip install --python .venv/bin/python torch==2.7.1 torchaudio==2.7.1 \
    --index-url https://download.pytorch.org/whl/cpu
  uv pip install --python .venv/bin/python --no-sources -e .
  exit 0
fi

model=small-music
duration=60
count=2
seed=-1
negative="poor quality"
loop=()
use_style=1

while getopts "m:d:n:s:N:lSh" opt; do
  case "$opt" in
    m) model="$OPTARG" ;;
    d) duration="$OPTARG" ;;
    n) count="$OPTARG" ;;
    s) seed="$OPTARG" ;;
    N) negative="$OPTARG" ;;
    l) loop=(--loop) ;;
    S) use_style=0 ;;
    h) usage 0 ;;
    *) usage 1 ;;
  esac
done
shift $((OPTIND - 1))
[ $# -ge 2 ] || usage 1
name="$1"; shift
prompt="$*"

if [ "$use_style" = 1 ] && [ "$model" = small-music ] && [ -s "$style_file" ]; then
  prompt="$(tr '\n' ' ' < "$style_file" | sed 's/ *$//') $prompt"
fi

[ -x "$sa3/.venv/bin/python" ] || { echo "run: scripts/stable-audio.sh setup" >&2; exit 1; }
: "${NECROMY_PYLIBS:?run inside nix develop}"
[ -n "${HF_TOKEN:-}" ] || echo "warning: HF_TOKEN is empty; gated weights will not download" >&2

echo "prompt: $prompt"
LD_LIBRARY_PATH="$NECROMY_PYLIBS" HF_TOKEN="${HF_TOKEN:-}" \
  "$sa3/.venv/bin/python" "$root/scripts/stable_audio_gen.py" \
  --model "$model" --duration "$duration" --count "$count" --seed "$seed" \
  --negative "$negative" "${loop[@]}" --out "$root/art/audio/$name" "$prompt"
