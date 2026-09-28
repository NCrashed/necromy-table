#!/usr/bin/env bash
#
# Cross-builds the Windows client from Linux and zips the folder the artist
# unzips: the exe, assets beside it, the DirectX shader compiler DLLs and a
# note. Output: dist/necromy-table-windows.zip.
#
# Re-enters `nix develop .#windows` by itself. The first run downloads the
# MSVC CRT and Windows SDK into target-windows/.xwin (about 1 GB) and DXC
# into vendor/dxc; later runs are offline. Extra arguments go to cargo.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

if [[ -z ${NECROMY_WINDOWS_SHELL:-} ]]; then
    export NECROMY_WINDOWS_SHELL=1
    exec nix develop "$root#windows" --command "${BASH_SOURCE[0]}" "$@"
fi

target=x86_64-pc-windows-msvc
name=necromy-table-windows
out=$root/dist/$name

cargo xwin build --release --target "$target" -p necromy-table "$@"

exe="${CARGO_TARGET_DIR:-target}/$target/release/necromy-table.exe"
if [[ $(head -c 2 "$exe" 2>/dev/null) != MZ ]]; then
    echo "build-windows: no PE binary at $exe" >&2
    exit 1
fi

rm -rf "$out"
mkdir -p "$out"
cp "$exe" "$out/"
# Bevy looks for assets/ beside the exe when CARGO_MANIFEST_DIR is unset.
cp -r "$root/assets" "$out/"

# The point of the build: without these two DX12 falls back to FXC and the
# first start takes minutes again. Checked, not assumed.
"$root/scripts/fetch-dxc.sh"
cp "$root"/vendor/dxc/{dxcompiler.dll,dxil.dll} "$out/"
for dll in dxcompiler.dll dxil.dll; do
    [[ -s $out/$dll ]] || { echo "build-windows: $dll missing" >&2; exit 1; }
done

# Notepad wants CRLF, and a BOM to read UTF-8 as UTF-8.
{ printf '\xef\xbb\xbf'; sed 's/$/\r/' "$root/docs/windows-readme.txt"; } >"$out/ЧИТАЙ-МЕНЯ.txt"
sed 's/$/\r/' >"$out/run-with-log.bat" <<'BAT'
@echo off
rem Runs the game and keeps everything it prints in log.txt beside it.
cd /d "%~dp0"
necromy-table.exe > log.txt 2>&1
BAT

(cd "$root/dist" && rm -f "$name.zip" && zip -qr "$name.zip" "$name")
printf 'build-windows: %s (%s)\n' "$root/dist/$name.zip" "$(du -sh "$root/dist/$name.zip" | cut -f1)"
