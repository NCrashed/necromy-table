#!/usr/bin/env bash
#
# Stands up the local language model behind the gods' voices (wishes and the
# storyteller, docs/design.md §7, §8): a llama-server with
# Qwen3-4B-Instruct on http://127.0.0.1:8080, where the game looks unless
# NECROMY_ORACLE says otherwise. Same model, file and port as
# ../necromy-firstperson's talk-server.sh, so one running server serves both.
# Run it in a second terminal; the game works without it, on prepared wishes
# and template voices.
#
# The model file is fetched on first run (~2.4 GB, into ~/models). On this
# laptop the NVIDIA card runs nouveau, so inference is CPU-only; 4B in
# Q4_K_M is the size that still answers in seconds.
set -euo pipefail

model_dir=${NECROMY_MODEL_DIR:-$HOME/models}
model=$model_dir/Qwen3-4B-Instruct-2507-Q4_K_M.gguf
url=https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf

if [[ ! -f $model ]]; then
    mkdir -p "$model_dir"
    echo "fetching the model (~2.4 GB, once) -> $model"
    curl -L --fail -o "$model.part" "$url"
    mv "$model.part" "$model"
fi

# Half the threads: the game renders on this CPU too.
args=(
    -m "$model"
    --host 127.0.0.1 --port "${NECROMY_ORACLE_PORT:-8080}"
    --ctx-size 8192
    --threads "$(( $(nproc) / 2 ))"
)

if command -v llama-server >/dev/null; then
    exec llama-server "${args[@]}"
fi
exec nix shell nixpkgs#llama-cpp -c llama-server "${args[@]}"
