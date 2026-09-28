The game's icon. `source.png` is the drawing (512 px); the rest is made
from it:

    magick icon/source.png -define icon:auto-resize=256,128,64,48,32,24,16 icon/necromy.ico
    magick icon/source.png -resize 128x128 icon/window.png

`necromy.ico` goes into the Windows exe (`build.rs`, `necromy.rc`),
`window.png` onto the window at run time (`src/icon.rs`).
