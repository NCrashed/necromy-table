Hex tiles, 64×64, flat-top, seen from above (PixelLab `create_tiles_pro`,
hex, top-down, segmentation outlines). The hexagon fills the top 55 rows.

- `ground/<kind>-<n>.png`: bare ground under billboard props
  (`assets/props/`, placed by `src/props.rs`). Variants are picked per hex.
- `temple-<n>.png`: the temple, still painted with its shrine baked in.

Region and god stage tint them in `board.rs`.
