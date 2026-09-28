#!/usr/bin/env bash
# Build the dedicated server as one static binary and vendor it into the
# aerospace deployment (../aerospace, or $AEROSPACE), whose NixOS module
# service/necromy-table.nix runs it. Then deploy from there:
#
#   scripts/deploy-server.sh
#   cd ../aerospace/deploy && ./manage production deploy
#
# Clients must be built from the same commit: the wire protocol
# (necromy-net PROTOCOL) refuses a mismatch.
set -euo pipefail
cd "$(dirname "$0")/.."

aerospace="${AEROSPACE:-../aerospace}"
if [ ! -f "$aerospace/service/necromy-table.nix" ]; then
  echo "aerospace checkout not found at $aerospace (set AEROSPACE)" >&2
  exit 1
fi

echo "==> building the static server"
out="$(nix build .#necromy-server-static --no-link --print-out-paths)"

dest="$aerospace/necromy-table"
mkdir -p "$dest"
install -m755 "$out/bin/necromy-server" "$dest/necromy-server"
rev="$(git rev-parse --short HEAD)$(git diff --quiet HEAD -- crates Cargo.toml Cargo.lock || echo -dirty)"
echo "$rev" > "$dest/VERSION"

echo "==> $dest/necromy-server ($(du -h "$dest/necromy-server" | cut -f1), $rev)"
echo "    next: cd $aerospace/deploy && ./manage production deploy"
