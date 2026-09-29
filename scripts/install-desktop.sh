#!/usr/bin/env bash
# Puts the game into the desktop's app list with its name and icon.
#
#   scripts/install-desktop.sh
#
# On Wayland a window cannot hand its icon over: GNOME matches the
# window's app id (`necromy-table`, `main_window` in src/main.rs) to a
# desktop entry of that name and takes the name and icon from there.
# Writes ~/.local/share/applications/necromy-table.desktop (launching
# scripts/play.sh from this checkout) and the icon, from icon/source.png,
# into the hicolor theme. Rerun after moving the checkout.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
data="${XDG_DATA_HOME:-$HOME/.local/share}"

for size in 32 48 64 128 256 512; do
  dir="$data/icons/hicolor/${size}x${size}/apps"
  mkdir -p "$dir"
  nix shell nixpkgs#imagemagick -c magick "$root/icon/source.png" \
    -resize "${size}x${size}" "PNG32:$dir/necromy-table.png"
done

mkdir -p "$data/applications"
cat > "$data/applications/necromy-table.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Necromy Table
Comment=Стол пяти богов
Exec=$root/scripts/play.sh
Path=$root
Icon=necromy-table
StartupWMClass=necromy-table
Categories=Game;StrategyGame;
Terminal=false
EOF

# Let the shell see the new entry now. (No icon cache: the user's hicolor
# has no index.theme, so a cache there comes out invalid; GNOME reads the
# folder without one.)
command -v update-desktop-database >/dev/null && update-desktop-database "$data/applications" || true
echo "installed $data/applications/necromy-table.desktop"
