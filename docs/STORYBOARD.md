# Writing storyboards

A storyboard is a JSON file describing the script and the visual plan.
The LLM writes one for you from a topic, but you can write or edit them by
hand for full control. Rendering a hand-written storyboard costs nothing
with `--silent`, and only TTS otherwise.

```bash
story_animator storyboard "How wind turbines work" -o turbine.json   # LLM draft to start from
story_animator generate --storyboard turbine.json --silent -q draft  # free preview
story_animator generate --storyboard turbine.json                    # render it (video + short)
```

Complete examples:
- [`examples/undersea_cables.json`](../examples/undersea_cables.json): a generated five-minute video:
  6 chapters, chapter cards, silent visual beats, audio tags, fun facts, custom scenes and a short range
- [`examples/base_rate.json`](../examples/base_rate.json): 60-second diagram style (dot grids, equation, graph)
- [`examples/bees.json`](../examples/bees.json): 60-second infographic style (icons, characters)

The 60-second examples need `TARGET_DURATION_SECS=60`, for example
`TARGET_DURATION_SECS=60 story_animator generate --storyboard examples/bees.json --silent -q draft`.
At the 300-second default they stop with "narration is too short".
`story_animator catalog` prints the current vocabulary.

## Shape

```json
{
  "title": "How Undersea Cables Carry the Internet",
  "theme": "ocean",
  "music": "wonder-filled documentary underscore, no vocals",
  "description": "Over 99% of intercontinental data travels through glass threads on the seabed...",
  "chapters": ["The Invisible Grid", "Trapping Light", "The Armor Layers"],
  "short": { "from": 11, "to": 16, "title": "How Internet Cables Survive Deep Oceans" },
  "scenes": [
    {
      "narration": "[amazed] Over ninety nine percent of intercontinental internet traffic runs along the ocean floor.",
      "layout": "stat",
      "title": "The Invisible Grid",
      "background": "glow",
      "camera": "push_in",
      "transition": "fade",
      "chapter": 0,
      "content": { "value": 99, "suffix": "%", "label": "of intercontinental data", "cue": "ninety" }
    },
    {
      "narration": "",
      "layout": "chapter",
      "hold": 3.2,
      "chapter": 1,
      "transition": "fadeblack",
      "content": { "number": 1, "total": 5, "name": "Trapping Light" }
    }
  ]
}
```

| Field | Required | Notes |
|---|---|---|
| `title` | yes | Video title |
| `theme` | no | Colour theme for the whole video (default `blueprint`) |
| `music` | no | Brief for generated music (used when `BACKGROUND_MUSIC=elevenlabs`) |
| `description` | no | YouTube description, copied into `youtube.txt` |
| `chapters` | no | Chapter names. With 3 or more, `youtube.txt` gets chapter timestamps |
| `short` | no | `{from, to, title}`: scene range (inclusive, 0-based) for the vertical short, and the title shown above it. Without it, no short is made |
| `scenes[].narration` | yes | What the narrator says, with optional audio tags. Spoken English only; write numbers as words. Empty = silent scene |
| `scenes[].layout` | no | One of the layouts below (default `hero`) |
| `scenes[].title` | no | On-screen heading, ≤ 5 words. Not shown for `title`/`quote`/`chapter`/`fun_fact` layouts |
| `scenes[].background` | no | Default `gradient` |
| `scenes[].camera` | no | Default `drift` |
| `scenes[].transition` | no | Transition *into* this scene (default `fade`) |
| `scenes[].hold` | silent scenes | Length of a silent scene in seconds (2–8) |
| `scenes[].chapter` | no | Index into `chapters` |
| `scenes[].content` | no | Layout-specific fields (below) |

Unknown values are silently replaced with safe defaults, so a typo never
breaks a render.

## Timing rules (for exactly five minutes)

- **About 610 spoken words in total** (accepted range about 475–660), over **25–50 scenes**.
  Videos generated from a topic typically land around 35 scenes in 6 chapters.
- 12–30 words per narrated scene (40 at most).
- Narration runs at about 2.5 words/second. Pauses, silent scenes and transitions fill the rest.
- If the narration is slightly long, it is sped up (up to 1.15x); if slightly
  short, pauses grow. If it's far off, generation stops with a clear message,
  or an LLM-written script is automatically rewritten, before any narration
  is paid for whenever possible.

For short targets (`TARGET_DURATION_SECS` under 120), the budget scales
down: a 60-second video is about 135 words over 5–10 scenes.

## Silent scenes

A scene with `"narration": ""` costs no TTS. It lasts `hold` seconds
(2–8) and plays under the music, and its elements spread across the hold.

- **Chapter cards** (`layout: "chapter"`) are added automatically before
  every chapter after the cold open.
- **Visual beats**: at most one per chapter, right after a big reveal, where
  the animation speaks for itself (a network pulsing, a scale reveal).
  Never for anything that needs explaining, and never as a chapter's first
  scene.

## Audio tags (expressive narration)

With `ELEVENLABS_MODEL=eleven_v3`/`eleven_v4`, narration may contain tags
in square brackets that direct the delivery:

```json
"narration": "[curious] So how does light stay inside a glass thread? [short pause] It bounces."
```

Useful tags: `[curious]`, `[excited]`, `[amazed]`, `[impressed]`, `[serious]`,
`[thoughtful]`, `[whispers]`, `[chuckles]`, `[dramatic pause]`, `[short pause]`.
Use at most 2 per scene. Tags are not spoken, don't count as words, and
never appear in captions. Don't use them with older models, which would read them aloud.

## Cues: syncing visuals to words

Most items accept a `cue`: **one word copied from that scene's narration**.
The item appears the moment the narrator says that word.

```json
"narration": "A plant needs sunlight, water and carbon dioxide.",
"content": { "items": [
  { "icon": "sun",   "label": "Sunlight", "cue": "sunlight" },
  { "icon": "drop",  "label": "Water",    "cue": "water" },
  { "icon": "cloud", "label": "CO2",      "cue": "carbon" }
]}
```

If `cue` is missing, the item's label is tried as a cue. If neither appears
in the narration, items are spread evenly through the narration. The first
item always appears within about a second of the narration starting, even
if its cue word comes later, so no scene opens on an empty frame.

## Mechanism layouts (the main tool)

Viewers follow a mechanism by watching it work, so most scenes should be
`process` or `cross_section`. Icon-led layouts (`hero`, `icon_row`, `list`,
`compare`, `flow`, `cycle`, `character`) may be at most 20% of narrated
scenes, and every middle chapter needs a mechanism scene. Generated
storyboards are validated against this.

### `process`: a working schematic, run step by step
```json
{ "parts": [ { "id": "lake",  "kind": "tank",  "label": "Upper lake",   "width": 1.3, "level": 0.9,  "col": 0 },
             { "id": "valve", "kind": "valve", "label": "Inlet valve",  "col": 1, "row": 0.6 },
             { "id": "lock",  "kind": "tank",  "label": "Lock chamber", "width": 1.6, "level": 0.15, "col": 2 },
             { "id": "ship",  "kind": "ship",  "label": "Ship", "floats_on": "lock" } ],
  "links": [ { "id": "culvert",  "from": "lake",  "to": "valve", "kind": "pipe", "carries": "water" },
             { "id": "culvert2", "from": "valve", "to": "lock",  "kind": "pipe", "carries": "water" } ],
  "steps": [ { "cue": "valve",   "do": [ { "open": "valve" }, { "focus": "valve" } ] },
             { "cue": "gravity", "do": [ { "on": "culvert culvert2" }, { "focus": "all" }, { "note": "No pumps" } ] },
             { "cue": "rises",   "do": [ { "level": ["lock", 0.85] }, { "level": ["lake", 0.8] } ] } ] }
```
- **parts**: `id`, `kind` (a kit part; `story_animator kit` lists them),
  `label` (≤ 3 words), `col`/`row` (small grid positions in the order things
  happen; fractions are fine), optional `size` (0.4–2.2), `hidden: true`
  (appears later via `show`), plus part fields such as `level`, `width`,
  `open`, `teeth`, `blades`, `angle`, `floats_on`.
- **links**: `from`/`to` (a part id, optionally `id.port` with
  in/out/top/bottom/left/right), `kind` (`pipe`, `wire`, `cable`, `ray`,
  `fibre`, `belt`, `shaft`, `arrow`, `duct`, or `mesh` for two gears),
  `carries` (`water`, `oil`, `air`, `gas`, `steam`, `heat`, `current`,
  `data`, `light`, `signal`, `force`, `fuel`, `blood`), optional `id`,
  `label`, `flow: true` (running from the start), `reverse`.
- **steps**: each has a `cue` word and a `do` list. Actions:
  `on`/`off` (parts run or spin; links flow), `open`/`close`,
  `level [id, 0-1]`, `fill`, `drain`, `show`, `hide`, `highlight`, `dim`,
  `undim`, `label [id, "text"]`, `focus id` / `focus all` (camera),
  `note "text"`, `reverse` (a link), and part-specific ones (`forward` for
  a ship). Targets can be several ids separated by spaces. Strings like
  `"open valve"` work too.
- **`"continue": true`** on the next process scene keeps the same diagram:
  reuse the ids, add parts or links if needed, and give only the new steps.
  The diagram picks up exactly where the previous scene left it. Use this
  to grow one machine over 2–4 scenes per chapter. Consecutive process
  scenes are only allowed with `continue`.
- A silent `process` scene (no narration, a `hold`) with no steps just runs
  the machine: a good visual beat after a reveal.

**Parts the kit doesn't have.** Use a new generic snake_case `kind` and add
`"make"`: one line describing how to draw it, for example
`{"id": "gate", "kind": "miter_gate", "make": "pair of lock gates that swing shut into a V against the water", "col": 3}`.
The part is written by the AI, sandbox-checked and test-run, then saved to
`engine/story_engine/kit/library/` and reused by every later video. If it
can't be validated, it draws as a labelled block. Name it generically
(`miter_gate`, not `panama_gate_2`) and prefer existing parts. At most 3 new
parts are built per video.

Built-in parts: tank, valve, pump, turbine, propeller, gear, piston, wheel,
spring, motor, generator, battery, bulb, switch, heater, chip, laser,
detector, mirror, lens, antenna, flame, ship, block (abstract units only),
node. Aliases include chamber/reservoir→tank, sluice/gate→valve,
fan/compressor→turbine, lamp→bulb, emitter→laser, receiver/sensor→detector,
boat/vessel→ship.

### `cross_section`: cut it open, layer by layer
```json
{ "shape": "rings",
  "layers": [ { "label": "Glass fibres", "material": "core", "thickness": 0.6, "cue": "fibres" },
              { "label": "Copper sleeve", "material": "copper", "thickness": 0.5, "note": "carries power", "cue": "copper" },
              { "label": "Steel armour", "material": "steel_wires", "thickness": 0.7, "cue": "armour" } ],
  "highlight": 1, "highlight_cue": "power" }
```
`rings` lists layers inside out (cable, pipe, planet); `stack` lists them top
down (road, dam, chip) and builds foundations first. Materials: glass, core,
fibre, steel, steel_wires, copper, aluminium, gold, plastic, polyethylene,
rubber, insulation, gel, water, air, soil, sand, gravel, concrete, asphalt,
rock, clay, silicon, oxide, iron, wood, foam, tar. Each has its own fill and
texture.

## Other layouts

### `title`: opening hook or closing card
```json
{ "heading": "Why Bees Matter", "subheading": "The tiny workers feeding the world", "icon": "bee" }
```
`heading` defaults to the scene `title`. Letters cascade in, then the icon pulses.

### `hero`: one big symbol for a single idea
```json
{ "icon": "flower", "label": "Pollination", "tag": "75% of crops",
  "effect": "pulse | orbit | spin | shake | grow", "effect_cue": "bees" }
```

### `icon_row`: 2–5 parallel things
```json
{ "items": [ { "icon": "apple", "label": "Apples", "sub": "optional", "cue": "apples" } ],
  "highlight": 0, "highlight_cue": "word" }
```
Cards pop in on their cues; `highlight` dims the others and outlines one.

### `list`: 2–5 readable bullet points
```json
{ "items": [ { "icon": "clock", "text": "Wake up at the same time", "cue": "wake" } ] }
```
Earlier rows dim as each new one arrives, then all return at the end.

### `compare`: A versus B
```json
{ "left":  { "icon": "car", "label": "Petrol", "points": ["Cheap upfront", "Loud"], "cue": "petrol" },
  "right": { "icon": "battery", "label": "Electric", "points": ["Quiet"], "cue": "electric" },
  "winner": "right", "winner_cue": "cheaper" }
```

### `stat`: one striking number that counts up
```json
{ "value": 71, "prefix": "", "suffix": "%", "decimals": 0,
  "label": "of Earth is covered by water", "context": "optional extra line",
  "cue": "seventy", "icon": "optional (non-% stats)" }
```
A `%` suffix with a value between 0 and 100 draws a progress ring that fills as it counts.
The counter starts rolling early in the scene and lands on the value as the `cue` word is spoken.

### `bar_chart`: 2–6 quantities
```json
{ "bars": [ { "label": "2000", "value": 12, "cue": "two", "display": "optional text" } ],
  "unit": "%", "highlight": 2 }
```

### `flow`: 2–5 step process
```json
{ "steps": [ { "icon": "search", "label": "Research", "cue": "research" } ] }
```
Steps chain together with arrows, then a glowing dot travels the path.

### `timeline`: 2–5 dated events
```json
{ "events": [ { "date": "1969", "text": "Moon landing", "icon": "rocket", "cue": "moon" } ] }
```

### `character`: a human moment
```json
{ "characters": [
    { "look": "kid", "pose": "think", "expression": "worried", "color": "accent", "speaking": true },
    { "look": "elder", "pose": "point", "expression": "happy", "color": "accent3", "speaking": false } ],
  "bubble": "But why is the sky blue?", "bubble_cue": "why",
  "prop": "sun", "prop_cue": "sun",
  "beats": [ { "cue": "smiled", "character": 1, "pose": "cheer", "expression": "excited" } ] }
```
- looks: `adult`, `kid`, `elder`, `robot`
- poses: `stand`, `wave`, `point`, `cheer`, `think`, `shrug`, `present`
- expressions: `happy`, `neutral`, `sad`, `surprised`, `worried`, `angry`, `excited`

Characters blink. Speaking characters move their mouth only while words are
being spoken. `beats` change pose or expression on cue.

### `quote`: a memorable line revealed word by word
```json
{ "text": "The important thing is not to stop questioning.", "author": "Albert Einstein",
  "highlight": "questioning" }
```
If the quote's words are in the narration, each word appears exactly as it is spoken.

### `cycle`: 3–5 stages that loop
```json
{ "items": [ { "icon": "sun", "label": "Evaporate", "cue": "evaporates" } ], "center": "Water cycle" }
```

## Long-form layouts

### `chapter`: chapter card (silent)
```json
{ "number": 2, "total": 5, "name": "The Armor Layers", "icon": "shield" }
```
A big translucent chapter number, a letter-spaced kicker, the chapter name,
and progress dots. Added automatically by the writer; you only need it in
hand-written storyboards. An icon that doesn't resolve is left out.

### `fun_fact`: the "wait, really?" moment
```json
{ "kind": "engineering marvel", "icon": "bridge",
  "fact": "Its main cables hold enough wire to circle the Earth three times",
  "highlight": "three times", "highlight_cue": "three" }
```
`kind` is the badge text (`fun fact`, `engineering marvel`, `did you know`,
`world record`). The fact appears word by word; `highlight` words turn the
accent colour with an underline on their cue. Every middle chapter of a
generated video lands its wow with this, `scale` or `stat`.

### `scale`: sizes at true proportion
```json
{ "items": [ { "label": "Person", "value": 1.8, "display": "1.8 m", "icon": "person", "cue": "person" },
             { "label": "Eiffel Tower", "value": 330, "display": "330 m", "icon": "tower", "cue": "eiffel" },
             { "label": "Millau Viaduct", "value": 343, "display": "343 m", "icon": "bridge", "cue": "viaduct" } ],
  "unit": "m", "callout": "Taller than Eiffel", "callout_cue": "taller" }
```
2–4 columns on a hatched ground line, drawn like an engineering drawing:
dimension arrows, an icon cap, the value above, a dashed guide at the
tallest. Columns grow on their cues, then the callout pops.

## Diagram layouts (3Blue1Brown style)

These are the default style's main tools. LaTeX goes in `tex` fields; in
JSON, escape backslashes (`"\\frac{1}{2}"`).

### `equation`: a formula that morphs step by step
```json
{ "steps": [
    { "tex": "A = 8 \\times 6", "cue": "rectangle", "note": "the rectangle" },
    { "tex": "A = 48", "cue": "forty" },
    { "tex": "A_{\\triangle} = \\frac{48}{2} = 24", "cue": "half", "note": "cut it in half" } ],
  "stack": false, "highlight_cue": "always" }
```
Each step transforms into the next, with matching symbols gliding into place.
`"stack": true` keeps earlier lines above, like a written derivation. The
final line is circled.

### `graph`: functions on axes
```json
{ "x_range": [0, 6], "y_range": [0, 6],
  "functions": [ { "expr": "sqrt(4*x)", "label": "f(x)=2\\sqrt{x}", "cue": "curve", "color": "accent" } ],
  "points": [ { "x": 4, "label": "(4,4)", "cue": "fixed" } ],
  "area":  { "function": 0, "from": 0, "to": 2, "cue": "area" },
  "trace": { "function": 0, "from": 0.2, "to": 4, "cue": "follow" },
  "x_label": "x", "y_label": "y" }
```
`expr` is Python maths of `x`: `+ - * / **`, `sin cos tan exp log sqrt abs`
and more, `pi`, `e`. It is evaluated through a strict whitelist. `y_range`
is optional; it's derived from the curves if omitted.

### `vectors`: arrows on a grid
```json
{ "vectors": [ { "vec": [0, 3], "label": "3", "cue": "north" },
               { "vec": [4, 0], "label": "4", "cue": "current" } ],
  "chain": true, "sum": { "label": "5", "cue": "together" } }
```
With `chain`, vectors are drawn tip to tail, and `sum` draws the resultant.
Use `"from"`/`"to"` for explicit positions.

### `dot_grid`: a population with groups lighting up
```json
{ "total": 1000,
  "groups": [ { "count": 1,  "label": "1 sick",            "cue": "sick",    "color": "accent2" },
              { "count": 10, "label": "10 false positives", "cue": "falsely", "color": "accent" } ],
  "focus_cue": "isolate", "result": "1 in 11", "result_cue": "chance" }
```
Good for probability, statistics and "1 in N" ideas. Takes 4–1000 dots.

### `network`: nodes with signals flowing
```json
{ "layers": [3, 5, 5, 2], "layer_labels": ["input", "hidden", "", "output"],
  "pulses": [ { "cue": "flows" } ], "path": [1, 2, 3, 0], "path_cue": "strongest" }
```
Or `"nodes": ["A","B","C"], "edges": [[0,1],[1,2]]` for a ring of labelled nodes.

### `custom`: a one-off diagram written by the AI
For a scene no layout can show (a machine, a physical process, geometry
being rearranged):
```json
{ "brief": "Top-down quadcopter: white body circle at centre, four sky-blue rotor circles on diagonal arms. On 'spin' draw rotation arcs: front-left and back-right rose clockwise, the others amber anticlockwise. On 'cancel' show a small torque arrow on each side shrinking to nothing, then the label 'balanced' under the drone.",
  "fallback": { "layout": "hero", "content": { "icon": "gear", "label": "Balanced rotors" } } }
```
How it works:
1. The AI writes a `build(ctx)` animation for this scene only.
2. It's checked against a sandbox: no imports beyond manim, numpy and math, and no file, process or network access.
3. It's dry-run, which executes every animation without drawing frames.
4. Errors go back to the AI for up to `CUSTOM_ATTEMPTS` tries.
5. If it still fails, the `fallback` renders instead.

Use at most two per video (extra ones render their fallback layouts),
and make the brief concrete: objects, positions, colours, and which
word triggers each step. Validated code is cached in
`storage/runs/<id>/scenes/scene_XX.custom.py`, and you can edit it by hand.

> Custom code runs on your machine. The sandbox guards against mistakes, not
> malicious input, so only render storyboards you trust.

## Themes

| Theme | Feel |
|---|---|
| `blueprint` | **default.** Navy with a soft glow, sky-blue lines, rose highlights, serif type. The classic diagram look |
| `graphite` | Charcoal chalkboard, green/blue/yellow, serif: geometry, physics, derivations |
| `midnight` | deep navy, warm gold: science, space, big ideas |
| `sunset` | plum dusk, coral and peach: stories, emotion, history |
| `ocean` | abyssal teal, aqua glow: nature, water, calm |
| `forest` | dark pine, lime and amber: ecology, growth, health |
| `ember` | charred red-black, flame orange: danger, energy, drama |
| `neon` | synthwave violet, pink and cyan: tech, future, gaming |
| `paper` | cream paper, terracotta (light): friendly, editorial |
| `candy` | soft pink-white (light): kids, playful, lifestyle |
| `mono` | charcoal, one yellow accent: serious, finance, news |

## Backgrounds

Diagram style: `glow` (soft centre light with dark vignette, the classic
look) and `blueprint` (the same over a faint drafting grid).

Others: `plain`, `gradient`, `stars` (twinkling), `grid`, `dots`,
`city` (night skyline), `hills` (layered, with sun), `waves` (animated),
`bokeh` (drifting lights), `rays` (rotating sunburst).

## Cameras

`static`, `drift` (gentle float, the default), `push_in`, `pull_out`,
`pan_left`, `pan_right`. These are slow continuous moves across the whole scene.

## Transitions

`fade`, `fadeblack`, `dissolve`, `slideleft`, `slideright`, `slideup`,
`smoothleft`, `smoothright`, `wipeleft`, `wiperight`, `circleopen`,
`circleclose`, `zoomin`, `radial`.

## Icons

Engineering: `bridge, crane, dam, jet_engine, magnet, pylon, ruler,
satellite, ship, skyscraper, tower, train, turbine, wrench`

General: `apple, atom, balance, battery, bee, bell, bird, book, brain, calendar,
camera, car, cart, chart, chart_down, chat, check, chip, clock, cloud, coin,
cross, crown, cup, dna, drone, drop, eye, factory, fire, fish, flag, flower, gear,
gift, globe, graduation, heart, hourglass, house, key, laptop, leaf,
lightbulb, lightning, lock, mail, money, moon, mountain, music, paw, people,
person, phone, pill, pin, plane, planet, plant, question, rain, rocket,
search, shield, star, sun, target, tree, trophy, virus, warning, wifi`

About 330 synonyms also work: `viaduct`→bridge, `landmark`→tower,
`wind`→turbine, `gps`→satellite, `power_grid`→pylon, `idea`→lightbulb,
`ai`→brain, `ocean`→drop, `team`→people, `danger`→warning, and so on.
An unknown name renders as a lettered badge, so it never breaks.

## Tips for great videos

- **Hook in scene 1.** Start with a question, a surprising number or a size comparison (`stat`, `scale` and `hero` work well).
- **Show the machine working.** One `process` diagram per chapter, grown with `continue`; tie every step to a cue word.
- **One wow per chapter.** Relate it to something familiar ("thinner than a hair", "taller than the Eiffel Tower").
- **Make the short's chapter standalone.** Its first line must hook without earlier context.
- **Never repeat a layout back to back**, except `process` scenes that `continue` the same diagram.
- **Name what you show.** If a card says "Water", say "water" in the narration and cue it.
- **Short on-screen text.** Labels of 3 words or fewer. Captions carry the sentences.
- **End on a closer.** A `quote` or `title` card gives the final 2–3 seconds a composed frame.
