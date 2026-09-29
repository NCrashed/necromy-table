The game's icon. `source.png` is the drawing (512 px); the rest is made
from it:

    magick icon/source.png -define icon:auto-resize=256,128,64,48,32,24,16 icon/necromy.ico
    magick icon/source.png -resize 128x128 icon/window.png

`necromy.ico` goes into the Windows exe (`build.rs`, `necromy.rc`),
`window.png` onto the window at run time (`src/icon.rs`).

On Wayland a window cannot set its own icon: GNOME matches the window's
app id (`necromy-table`, set in `main_window`) to a desktop entry.
`scripts/install-desktop.sh` writes that entry (launching
`scripts/play.sh` from the checkout) and puts `source.png` into the user's
hicolor icons.
