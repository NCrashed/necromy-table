# Champions: briefs and concept prompts

The codex (`~/dev/necromycon`) describes the gods, not their lands or
followers, so the champions below are new: mortals from each god's domain,
built from the codex's palettes and symbols. The names are working titles;
rename freely.

Rules that come from the codex and keep the set coherent:

- Gold belongs to Ahamar alone. Zaga: no gold at all.
- Bhava has no face and no image; his champion carries his green
  (`#73CC66`) and his growth, never a picture of him.
- Every silhouette must read at 30 px tall from a tilted table camera: one
  big shape per champion (cleaver, maul, spear, lantern, staff).
- Vivid, not grey-brown: dark fantasy in tone, not in palette.

## Pipeline

1. **Concept (ChatGPT, by hand).** Paste a prompt below. Keep a variant
   that is full body, facing the viewer straight on, feet visible, on a plain
   background: PixelLab rotates it into directions and needs the front view.
   Save the keeper to `art/refs/<god>.png` (`bhava.png`, `trishna.png`,
   `zaga.png`, `ahamar.png`, `maya.png`).
2. **Sprite (PixelLab).** `create_character` in v3 mode from the ref at a
   48×48 canvas (the hero about 30 px tall, feet on the bottom row), then
   `walking` and `breathing-idle` animations. We use the four straight
   directions. About 15 generations per champion.
3. **Portrait (PixelLab).** A 64×64 bust from the south sprite for the UI.
4. **In game.** Sheets go to `assets/sprites/<god>-<champion>-<anim>.png`
   (one row per direction), the portrait to `assets/portraits/<god>.png`.

## The five

### Bhava (wood): the Warden of the Untouched

A young forest warden who guards what must not be touched: he walks behind
the living and leaves the dead to grow. Barefoot, lean, a cloak of living
moss and broad leaves; bark-plate bracers; a tall crooked staff that is a
living sapling, still in leaf, with a green light in its knot. Twigs
sprout from his shoulders. Face half hidden by a hood of leaves.
Palette: leaf green `#73CC66` as the one vivid colour, deep moss, bark
brown, pale birch white.
Silhouette: tall and narrow, the staff taller than him.

### Trishna (fire): the Feast Cook

A broad, cheerful festival cook-warrior from the feasting lands, who fights
with the same cleaver she cuts the harvest meat with. Saffron and white
festival robe tucked up for work, a red sash, sleeves rolled, a bread-and-
fruit basket strapped on her back, a huge curved cleaver. An open golden-
orange spiral sigil embroidered on her apron. Long hair tied up, flying
strands. Forward lean, grin; a single overripe fruit with a dark spot in
the basket as the hidden second reading.
Palette: saffron `#e0712e`, red `#c23a22`, honey `#e8a13c`, sun-warmed skin
`#d98a5a`, white cloth.
Silhouette: stocky, round, the cleaver big as her torso.

### Zaga (earth): the Penitent of the Iron Skete

A huge monk from a hermitage, who carries the weight on purpose. Hooded,
shoulders bowed, one lower than the other; chains wound around his chest
and forearms as a hair shirt of penance, not a prisoner's shackles. He
leans on a colossal blunt iron-and-stone maul planted head-down like a
pilgrim's staff. Rough amethyst robe over dull iron plates. A small wooden
flute hangs at his belt: Zaga loves music.
Palette: amethyst `#7C5C9E`, indigo shadow `#2E2540`, pale violet `#B9A3D6`,
dull iron `#5A5560`, tarnished silver `#8A8694`. No gold.
Silhouette: wide, hunched, heavy; the maul a second pillar beside him.

### Ahamar (metal): the Registrar-Knight

A knight of order who writes the dead into the legion. Perfectly upright
and symmetrical. White marble-like plate armour with gold trim; an Art
Deco half-mask with flat planar facets covering the upper face; a small
rigid ring halo behind the head; a sun-at-zenith emblem on the breastplate.
A tall straight spear in one hand, a heavy ledger chained to the belt.
Ornament is heraldic, never organic.
Palette: gold `#c9a24b`, pale gold `#e6c878`, deep gold `#b5872f`, marble
`#e9e3d6`, ash `#6b6862`.
Silhouette: a straight vertical line, the spear the tallest thing on the
board.

### Maya (water): the Mourner of the Quiet Meadow

A veiled mourner from the Quiet Meadow, where the dead walk half seen.
Slim and fluid, ash-grey skin with glowing turquoise kintsugi cracks on the
collarbones and wrists; dark eyes that look through you. A long dark robe
whose hem on one side breaks into smoke with turquoise sparks. She carries
a lantern on a long pole with a turquoise flame, and a handful of fresh,
living flowers. No skulls, no scythe.
Palette: ash `#9a9c98`, turquoise `#1fb6a6` and `#2bd4c0`, smoke `#14181a`,
dark sclera `#0c1413`, a few fresh flower colours.
Silhouette: slim vertical, the lantern pole leaning forward.

## Concept prompts (ChatGPT)

Each prompt already includes the shared style of `art/style.txt`. Ask for
the size 1024×1536 (portrait). If ChatGPT turns the figure three-quarters,
reply: "same character, facing the viewer straight on, full body".

**Bhava:**

```
Concept art for a pixel-art hex board game: champions of five gods from different regions of a fantasy kingdom. Juicy, high-contrast look in the register of Warcraft III: saturated colours, bold readable silhouettes, chunky exaggerated proportions, strong light and shadow. Dark fantasy in tone, but not in palette: vivid, not grey-brown. Game-ready character design shown full body on a plain light grey background, facing the viewer straight on, feet visible, symmetrical front view.

Character: the Warden of the Untouched, champion of the god of growth. A lean young forest warden, barefoot, cloak of living moss and broad leaves, bark-plate bracers, small twigs sprouting from his shoulders, face half hidden under a hood of leaves. He holds a tall crooked staff that is a living sapling still in leaf, with a soft green light in its knot, taller than he is. Palette: vivid leaf green #73CC66 as the one bright colour, deep moss green, bark brown, pale birch white. Tall narrow silhouette. No skulls, no gold.
```

**Trishna:**

```
Concept art for a pixel-art hex board game: champions of five gods from different regions of a fantasy kingdom. Juicy, high-contrast look in the register of Warcraft III: saturated colours, bold readable silhouettes, chunky exaggerated proportions, strong light and shadow. Dark fantasy in tone, but not in palette: vivid, not grey-brown. Game-ready character design shown full body on a plain light grey background, facing the viewer straight on, feet visible, front view.

Character: the Feast Cook, champion of the goddess of thirst and feasts. A broad, cheerful, strong woman, festival cook-warrior: saffron and white harvest-festival robe tucked up for work, red sash, rolled sleeves, a basket of bread and fruit strapped on her back with one overripe fruit showing a dark spot, a huge curved cleaver as big as her torso held ready. An open golden-orange spiral that never closes embroidered on her apron. Long hair tied up with flying strands, a hungry grin. Palette: saffron #e0712e, red #c23a22, honey #e8a13c, warm skin #d98a5a, white cloth. Stocky round silhouette.
```

**Zaga:**

```
Concept art for a pixel-art hex board game: champions of five gods from different regions of a fantasy kingdom. Juicy, high-contrast look in the register of Warcraft III: saturated colours, bold readable silhouettes, chunky exaggerated proportions, strong light and shadow. Dark fantasy in tone, but not in palette: vivid, not grey-brown. Game-ready character design shown full body on a plain light grey background, facing the viewer straight on, feet visible, front view.

Character: the Penitent of the Iron Skete, champion of the goddess of renunciation. A huge hooded monk from a mountain hermitage, shoulders bowed, one lower than the other; heavy chains wound around his chest and forearms as a hair shirt of penance, not shackles. He leans with both hands on a colossal blunt iron-and-stone maul planted head-down like a pilgrim's staff. Rough amethyst robe over dull iron plates, a small wooden flute at his belt. Grief and restraint, not wrath. Palette: amethyst #7C5C9E, indigo shadow #2E2540, pale violet #B9A3D6, dull iron #5A5560, tarnished silver #8A8694. Strictly no gold. Wide, hunched, heavy silhouette.
```

**Ahamar:**

```
Concept art for a pixel-art hex board game: champions of five gods from different regions of a fantasy kingdom. Juicy, high-contrast look in the register of Warcraft III: saturated colours, bold readable silhouettes, chunky exaggerated proportions, strong light and shadow. Dark fantasy in tone, but not in palette: vivid, not grey-brown. Game-ready character design shown full body on a plain light grey background, facing the viewer straight on, feet visible, perfectly symmetrical front view.

Character: the Registrar-Knight, champion of the god of order and self. A perfectly upright woman knight in white marble-like plate armour with gold trim, an Art Deco half-mask with flat planar gold facets covering the upper face, a small rigid ring halo behind her head, a sun-at-zenith emblem on the breastplate. She holds a tall straight spear upright beside her; a heavy ledger book hangs chained to her belt. Heraldic geometric ornament, never organic. Cold pale white-gold light. Palette: gold #c9a24b, pale gold #e6c878, deep gold #b5872f, marble #e9e3d6, ash #6b6862. Straight vertical silhouette.
```

**Maya:**

```
Concept art for a pixel-art hex board game: champions of five gods from different regions of a fantasy kingdom. Juicy, high-contrast look in the register of Warcraft III: saturated colours, bold readable silhouettes, chunky exaggerated proportions, strong light and shadow. Dark fantasy in tone, but not in palette: vivid, not grey-brown. Game-ready character design shown full body on a plain light grey background, facing the viewer straight on, feet visible, front view.

Character: the Mourner of the Quiet Meadow, champion of the goddess of death as release. A slim veiled mourner with ash-grey skin and glowing turquoise kintsugi cracks on her collarbones and wrists, dark eyes that look through the viewer. A long dark robe whose hem on one side breaks into dark smoke with turquoise sparks. She holds a lantern on a long pole with a turquoise flame, and a small bunch of fresh living flowers. Calm, not kind. No skulls, no scythe. Palette: ash #9a9c98, turquoise #1fb6a6 and #2bd4c0, smoke #14181a, a few fresh flower colours. Slim vertical silhouette.
```
