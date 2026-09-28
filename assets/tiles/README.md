Hex tiles, 128×128, flat-top, seen from above (PixelLab `create_tiles_pro`,
hex, top-down, segmentation outlines), a little above the middle. They set
the texel density of the whole table (`board::TEXELS`).

`ground/<kind>-<n>.png`: bare ground under billboard props
(`assets/props/`, placed by `src/props.rs`), temples and the Table
included. Variants are picked per hex. Region and god stage tint them in
`board.rs`.
