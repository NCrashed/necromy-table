#!/usr/bin/env bash
# Starts the game from this checkout: the release build, inside
# `nix develop` (the libraries it needs), built first if it has to be.
# The desktop entry (`scripts/install-desktop.sh`) runs this.
set -euo pipefail
cd "$(dirname "$0")/.."
exec nix develop -c cargo run --release
