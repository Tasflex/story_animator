# Writing storyboards

A storyboard is a JSON file describing the script and the visual plan.
The LLM writes one for you from a topic, but you can write or edit them by
hand for full control. Rendering a hand-written storyboard costs nothing
with `--silent`, and only TTS otherwise.

```bash
story_animator storyboard "How tides work" -o tides.json   # LLM draft to start from
story_animator generate --storyboard tides.json            # render it
story_animator generate --storyboard tides.json --silent -q draft   # free preview
```

Complete examples:
- [`examples/base_rate.json`](../examples/base_rate.json): diagram style (dot grids, equation, graph)
- [`examples/bees.json`](../examples/bees.json): infographic style (icons, characters)
`story_animator catalog` prints the current vocabulary.

## Shape

```json
{
  "title": "Why Bees Matter",
  "theme": "forest",
  "music": "light curious pizzicato, warm, 95 bpm, no vocals",
  "scenes": [
    {
      "narration": "Most flowering plants can't move, so they need help to spread their pollen.",
      "layout": "hero",
      "title": "Pollination",
      "background": "hills",
      "camera": "drift",
      "transition": "slideleft",
      "content": { "icon": "flower", "label": "Pollen on the move", "effect": "orbit" }
    }
  ]
}
```

| Field | Required | Notes |
|---|---|---|
| `title` | yes | Video title |
| `theme` | no | Colour theme for the whole video (default `blueprint`) |
| `music` | no | Brief for generated music (used when `BACKGROUND_MUSIC=elevenlabs`) |
| `scenes[].narration` | yes | What the narrator says. Spoken English only; write numbers as words |
| `scenes[].layout` | no | One of the layouts below (default `hero`) |
| `scenes[].title` | no | On-screen heading, ≤ 5 words. Not shown for `title`/`quote` layouts |
| `scenes[].background` | no | Default `gradient` |
| `scenes[].camera` | no | Default `drift` |
| `scenes[].transition` | no | Transition *into* this scene (default `fade`) |
| `scenes[].content` | no | Layout-specific fields (below) |

Unknown values are silently replaced with safe defaults, so a typo never
breaks a render.

## Timing rules (for exactly 60 seconds)

- **About 135 words in total** (accepted range 105–145), split over **5–10 scenes** (7 is ideal).
- 12–30 words per scene.
- Narration runs at about 2.5 words/second. Pauses and transitions fill the rest.
- If the narration is slightly long, it is sped up (up to 1.15x). If it's far
  too long, generation stops with a clear message, or an LLM-written script
  is automatically rewritten shorter.

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
in the narration, items are spread evenly through the narration.

## Layouts

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

Use at most two per video, and make the brief concrete: objects, positions,
colours, and which word triggers each step. Validated code is cached in
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

`apple, atom, balance, battery, bee, bell, bird, book, brain, calendar,
camera, car, cart, chart, chart_down, chat, check, chip, clock, cloud, coin,
cross, crown, cup, dna, drop, eye, factory, fire, fish, flag, flower, gear,
gift, globe, graduation, heart, hourglass, house, key, laptop, leaf,
lightbulb, lightning, lock, mail, money, moon, mountain, music, paw, people,
person, phone, pill, pin, plane, planet, plant, question, rain, rocket,
search, shield, star, sun, target, tree, trophy, virus, warning, wifi`

About 260 synonyms also work: `idea`→lightbulb, `money`/`cash`, `ai`→brain,
`ocean`→drop, `team`→people, `danger`→warning, `food`→apple, and so on.
An unknown name renders as a lettered badge, so it never breaks.

## Tips for great videos

- **Hook in scene 1.** Start with a question or a surprising number (`stat` and `hero` work well).
- **Never repeat a layout back to back.** Use 5 or more different layouts.
- **Name what you show.** If a card says "Water", say "water" in the narration and cue it.
- **Short on-screen text.** Labels of 3 words or fewer. Captions carry the sentences.
- **End on a closer.** A `quote` or `title` card gives the final 2–3 seconds a composed frame.
