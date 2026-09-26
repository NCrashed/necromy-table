# necromy-table

Pixel-art hex board game on Bevy: champions of five gods move across a hex
board; heroes are pixel-art billboards. Design: [`docs/design.md`](docs/design.md).

## Setup

```bash
cp .env.example .env     # fill in OPENAI_API_KEY and PIXELLAB_API_KEY
nix develop
cargo run --features dev
```

You play Trishna (red) against four bots. Click a lit hex to walk, a red hex
to attack, a card to play it (then a gold hex to aim; right click cancels).
In a battle click cards to burn and press Enter to throw. P passes in a
reaction window, Space ends the turn. `NECROMY_SEED=<n>` replays a board; rules tests: `cargo test -p necromy-rules`.

## Concept art (GPT Image)

```bash
scripts/gpt-image.sh -o ahamar-champion -n 4 "champion of Ahamar, gilded crown-mask, registry scrolls"
scripts/gpt-image.sh -o ahamar-champion-side -r art/refs/ahamar-champion.png "same character, side view"
scripts/gpt-image.sh -t -s 1024x1536 "champion of Trishna, festival cook with a burning ladle, full body"   # transparent background
```

Results land in `art/concepts/` next to a `.prompt.txt`. Edit `art/style.txt` to
change the shared art direction. Run `scripts/gpt-image.sh -h` for all options.

## Sprites (PixelLab MCP)

`.mcp.json` registers PixelLab for Claude Code in this project. It reads the key
from `PIXELLAB_API_KEY`, so start Claude Code from inside `nix develop` and
approve the `pixellab` server on first launch. Check it with `/mcp`.
