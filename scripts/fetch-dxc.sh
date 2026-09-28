#!/usr/bin/env bash
#
# Fetches the two DLLs that make DX12 shader compilation take seconds instead of
# tens of minutes: Microsoft's DirectX shader compiler. They ship beside the exe
# and `src/gpu.rs` points wgpu at them there, ahead of the FXC that Bevy
# would otherwise fall back to.
#
# Straight from Microsoft's own release rather than through a crate that
# repackages it — see `src/gpu.rs` for why the tidier
# `statically-linked-dxc` route is not usable. The release is pinned by tag
# *and* by hash below, so this downloads the same bytes every time or fails.
#
# Bumping the version means changing all three of `tag`, `asset` and `sha256`.
# wgpu wants at least v1.8.2502.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

tag=v1.9.2607
asset=dxc_2026_07_29.zip
sha256=a1dfb116ba3eeae6a1582291b53a8e7bf65ad760676bd3194685c8f7367cd241
url=https://github.com/microsoft/DirectXShaderCompiler/releases/download/$tag/$asset

dest=$root/vendor/dxc

if [[ -f $dest/dxcompiler.dll && -f $dest/dxil.dll && -z ${FORCE:-} ]]; then
    exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

echo "fetch-dxc: $tag"
curl -fsSL -o "$work/$asset" "$url"
printf '%s  %s\n' "$sha256" "$work/$asset" | sha256sum -c - >/dev/null

mkdir -p "$dest"
# `-j` because the archive carries a whole SDK and only these two files are
# wanted, flat, next to the exe. The globs around `x64` are not decoration: the
# archive stores its paths with **backslashes**, so `bin/x64/dxcompiler.dll`
# matches nothing at all and unzip says so in a warning rather than an error.
# They still pick out exactly one architecture — neither `arm64` nor `x86`
# contains the string `x64`.
#
# And the exit status has to be forgiven up to 1: unzip reports those same
# backslashes as a *warning*, which it counts as a non-zero exit, and `set -e`
# would take that for a failed extraction. Anything above 1 is a real error.
status=0
unzip -joq "$work/$asset" 'bin*x64*dxcompiler.dll' 'bin*x64*dxil.dll' -d "$dest" || status=$?
if ((status > 1)); then
    exit "$status"
fi

# Which is worth proving rather than believing: a pattern that quietly matched
# the arm64 build would produce a folder that looks right and an exe that
# cannot load its compiler.
sha256sum -c - <<EOF >/dev/null
9a5100511e127c6a2fc78edf984f95074a76d35b90c90c4d342430a5ae160e9b  $dest/dxcompiler.dll
feb57253eff0a622561e29b44cedbe86b89fc9a5bc8dc00fa2f98fafd712c2d8  $dest/dxil.dll
EOF

printf 'fetch-dxc: %s\n' "$dest"
