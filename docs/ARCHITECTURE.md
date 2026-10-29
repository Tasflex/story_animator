# Architecture

Story Animator has two halves:

- **Orchestrator (Rust, `src/`)**: talks to the LLM and TTS, plans timing,
  runs renders in parallel, and composites the final video.
- **Render engine (Python + Manim, `engine/story_engine/`)**: turns one
  scene spec into one MP4 clip, deterministically. No network, no LLM.

## Why this design

The previous version asked an LLM to write a full Manim program for the
whole video. Three problems followed from that:

- one syntax or API mistake anywhere killed the whole render;
- the timing drifted, so videos came out anywhere from 38 to 83 seconds;
- visual quality depended on luck.

Now the LLM mostly writes **data**: a storyboard that picks from a fixed
vocabulary of layouts, icons and motions. All the craft (composition,
easing, typography, timing) lives in tested code that always renders.

The old version had one real strength: a bespoke diagram for each idea.
That survives in two contained forms:

- **The mechanism kit** (the main one): `process` scenes are data (which
  parts, how they connect, what happens on which word), drawn from a kit of
  schematic parts. When a video needs a part the kit lacks, the LLM writes
  *that part*, it is validated, and it is kept for every later video.
- **`custom` scenes** (rare, max 2 per video): the LLM writes code for one
  scene against the same small `ctx` API the built-in layouts use.

Both are statically sandbox-checked, dry-run, and retried with the error
message, and both have a safe fallback (a labelled block, or the scene's
declared `fallback` layout). The blast radius of a mistake is one part or
one scene, never the whole video.

## Pipeline

```
                ┌──────────────┐
  topic ───────►│ 1a outline   │  llm.rs: niche fit score (refuse < MIN_TOPIC_FIT unless
                └──────┬───────┘  --force), 5-6 chapters, a wow per chapter, the short pick
                       ▼
                ┌──────────────┐
                │ 1b chapters  │  one LLM call per chapter, in parallel, each against its
                └──────┬───────┘  own word budget; validated + repaired up to 3 rounds;
                       │          joined with silent chapter cards
                       ▼
                ┌──────────────┐
                │ 1c length    │  free: estimated word timings → timeline fit. A script that
                │    check     │  would run long/short is rewritten BEFORE any TTS is bought
                └──────┬───────┘
                       ▼
                ┌──────────────┐
                │ 2 narration  │  tts.rs: ElevenLabs eleven_v4 /with-timestamps, narrated
                └──────┬───────┘  scenes only; audio tags for emotion; cached per scene
                       ▼
                ┌──────────────┐
                │ 3 timeline   │  timeline.rs: slots summing to exactly 300.000s
                └──────┬───────┘  (one rewrite if the real voice still misses)
                       ▼
                ┌──────────────┐
                │ 4 captions   │  subtitles.rs: ASS, word-by-word highlight
                │   + music    │  a track from the music library, mixed low
                └──────┬───────┘
                       ▼
                ┌──────────────┐
                │ 4a kit       │  kit.rs: parts the storyboard needs but the kit lacks → LLM writes
                │    growth    │  PART + build(k, spec) → `story_engine validate-part` → retry →
                └──────┬───────┘  saved to engine/story_engine/kit/library/ for reuse
                       ▼
                ┌──────────────┐
                │ 4b custom    │  custom.rs: LLM writes build(ctx) per custom scene →
                │    scenes    │  `story_engine validate` (sandbox + dry run) → retry
                └──────┬───────┘  with error → else scene's fallback layout
                       ▼
                ┌──────────────┐
                │ 5 render     │  render.rs → `python -m story_engine render`
                └──────┬───────┘  N scenes in parallel, safe fallback per scene
                       ▼
                ┌──────────────┐
                │ 6 master     │  media.rs: one ffmpeg graph: xfade chain, narration at
                └──────┬───────┘  exact offsets, ducked music, loudness normalisation
                       ├──────────────────────────────┐
                       ▼                              ▼
                ┌──────────────┐               ┌──────────────┐
                │ 7 YouTube    │ burn captions │ 8 short      │ segment → 9:16 reframe,
                └──────┬───────┘               └──────┬───────┘ title + vertical captions
                       ▼                              ▼
                   final.mp4 (16:9, 5:00)         short.mp4 (9:16, ≤ 59s)   + youtube.txt
```

## Writing five-minute scripts

One LLM call can write a tight 60-second script; a five-minute one drifts.
So videos of `LONG_FORM_SECS` (120s) or more are written in two stages:

1. **Outline**: title, theme, description, a niche **fit score** with its
   reason, 5–6 chapters (name, goal, beats, the *wow* it lands, narrated
   seconds) and which chapter becomes the short. Chapter 1 is a 15–30s cold
   open; the last chapter pays off the hook.
2. **Chapters**: each chapter is written in parallel with the whole plan as
   context and its share of the word budget. Each is validated on its own
   (word range, scene count, no back-to-back layouts, middle chapters must
   show their wow with `fun_fact`, `scale` or `stat`, at most one silent
   visual beat) and repaired with the problems fed back.

The chapters are joined with a silent `chapter` card before each one after
the cold open, and the short's scene range is recorded in the storyboard.
Shorter targets still use the single-call writer.

The channel brief lives in `src/niche.rs`: what fits (mechanisms,
megastructures, systems, nature's engineering, everyday physics) and what
is refused (news, politics, celebrities, opinion, advice, footage-dependent
topics). `story_animator topics` asks for ideas against the same brief.

## The mechanism kit: showing how things work

Icon cards are reliable but viewers can't follow a mechanism through them,
so most scenes are working schematics and icon-led layouts (hero,
icon_row, list, compare, flow, cycle, character) are capped at 20% of
narrated scenes by validation. Every middle chapter must contain a
`process` or `cross_section` scene.

**Parts** (`engine/story_engine/kit/`). A part is drawn from strokes in the
theme colours (an engineering drawing brought to life, not clip-art) and
knows:

- its **ports** (`in`, `out`, `top`, `bottom`…) as fractions of its bounding
  box, so connections stay right after the layout scales and moves it;
- its **actions** (`on`, `off`, `open`, `close`, `level`, `highlight`…). Each
  returns animations, or with `instant=True` jumps to the end state.
  Motion runs off a `speed` ValueTracker so it eases on and off.

Built-ins: tank (liquid level, things can float on it), valve, pump,
turbine, propeller, gear (meshes by tooth ratio), piston, wheel, spring,
motor, generator, battery, bulb, switch, heater, chip, laser, detector,
mirror, lens, antenna, flame, ship (floats on a tank's water), block,
node, plus aliases (`chamber`→tank, `sluice`→valve…).

**Links** join ports with orthogonal elbow routes and look like what they
are: a hollow pipe, a wire, a cable, a glowing ray, a glass fibre (light
zig-zags inside), a belt, a shaft or an arrow. Particles flow along them,
coloured by what they carry (water, current, data, light, air, heat…), at
a speed that eases on and off.

**The `process` layout** places parts on a `col`/`row` grid, grows the
whole machine to fill the stage, places labels (resized and moved to avoid
collisions), routes the links, builds the diagram in, then plays the
storyboard's steps on their cue words. `focus` moves the camera in on a
part (moderate zoom, clamped to the frame); `note` pops a short callout.
A scene with no steps (a silent beat) just runs the machine.

**Continuing diagrams.** Consecutive `process` scenes where the later one
sets `"continue": true` form a chain. The orchestrator gives every scene in
the chain all the chain's contents and its position (`kit::chains`). The
engine lays out the union of all parts, so positions never jump between
scenes, adds the parts introduced earlier straight away, replays earlier
steps instantly, and animates only this scene's new parts and steps. A
chapter builds one machine across 2–4 scenes, and each cut is seamless.

**Growing the kit** (`src/kit.rs`). After the storyboard is written,
`new_part_requests` finds part kinds that neither the catalog nor its
aliases know, each with the storyboard's `make` description and the
actions its steps use. For up to 3 per video, the LLM writes a part file
(`PART` metadata + `build(k, spec)`) against `PART_API`. The engine's
`validate-part` sandbox-checks it, builds it in a dry-run `process` scene,
scales it and fires every action. Errors go back to the LLM for up to
`CUSTOM_ATTEMPTS` tries. A passing part is saved to
`engine/story_engine/kit/library/<kind>.py` with a header naming the video
it was made for. The catalog picks it up next time, so the writer sees it
as an ordinary part. Parts that fail render as labelled blocks. Commit new
library files so the kit persists.

**`cross_section`** draws concentric rings (inside out: cable, pipe,
planet) or a stack (top down: road, dam, chip; built foundations first),
each layer with a material fill and texture (steel hatching, armour wires,
gel dots, glass shine…) and a leader-line label.

## Saving TTS credits

- **Silent scenes**: a scene with empty narration (chapter cards, and
  "visual beats" with a `hold` of 3–6s) is never sent to ElevenLabs. About
  a quarter to a third of the scenes in a typical video are silent.
- **Free length check**: before narration, the timeline is fitted with
  estimated word timings. If the script would need more than
  `MAX_SPEECH_TEMPO` or leave dead air, it is rewritten at a scaled word
  budget, which costs only LLM tokens.
- **Caching**: each scene's audio is cached by text, voice and model.
  Editing a storyboard and `resume`-ing pays only for changed scenes.
- **The short is free**: it reuses the long video's narration.

## Expressive narration

The default model is `eleven_v4`, which follows inline **audio tags**. The
writer starts most scenes with one tag (`[curious]`, `[impressed]`,
`[serious]`, `[amazed]`…) and saves the excited ones for real wow moments.
Tags are passed to ElevenLabs as part of the text. They are removed from
word counts (`storyboard::spoken`) and from the returned character
alignment (`tts::words_from_alignment`), so captions and cue matching only
see spoken words. Neighbouring narrated scenes go along as
`previous_text`/`next_text` so intonation flows across cuts. Voice settings
leave more room for expression when the model supports tags.

## The timing model (how "exactly 5:00" works)

Each narrated scene `i` gets a **slot** on the final timeline:

```
slot_i = lead_i + narration_i / tempo + tail_i
```

A silent scene's slot is its `hold`.

- `lead` is a short silence before the words (0.6s for the first scene, 0.35s otherwise).
- `tail` is a breathing pause (≥ 0.45s; the last scene holds ≥ 1.8s as an outro).
- If narration is long, `tempo` rises up to `MAX_SPEECH_TEMPO` (1.15x, applied with pitch-preserving `atempo`).
- If narration is short, the spare time is spread: up to 1.3s after each
  narrated scene, then up to 2.5s on silent scenes, then up to 4s on the
  outro, then evenly again. More than 3s of padding per scene means the
  script is too short, which is an error (`TooShort`) rather than dead air.

Slots always sum to exactly the target. Scene clips are rendered
`slot + transition` long, except the last. The ffmpeg `xfade` for scene `i`
starts at `start_i`. So the output length is:

```
Σ clip_len − (n−1)·transition = Σ slot = 300.000s
```

Narration clips are placed with `adelay` at `start_i + lead_i`, so audio
and visuals never drift. Unit tests in `timeline.rs` lock this in.

## One render, two formats

The scenes are composited once into `master.mp4`: no captions,
near-lossless (CRF 14). Both deliverables are cut from it:

- **YouTube (16:9)**: the master with word-highlight captions burned in.
- **Short (9:16)**: `pipeline::short_segment` takes the storyboard's short
  range, drops leading/trailing silent scenes, trims whole scenes from the
  end until it fits `SHORT_MAX_SECS` (59s), and extends with following
  scenes if it's under 40s. The cut starts 0.2s into the first scene to
  skip the crossfade tail. In the 1080x1920 frame:

  ```
    0 ┌──────────────┐
      │  hook title  │  short's title (ASS, wraps)
  600 ├══════════════┤  accent hairline
      │  landscape   │  master, bottom 12% (the empty caption band) cropped,
      │  video       │  scaled to the full 1080 width
 1134 ├══════════════┤  accent hairline
      │  captions    │  vertical word-highlight captions, re-timed to the cut
      │              │
      │  CTA         │  "Full video on the channel" for the last 3.5s
 1920 └──────────────┘
  ```

  The rest of the frame is a blurred, darkened copy of the same video.
  Audio fades in and out at the cut.

Layouts reserve the bottom band of the landscape frame for captions, which
is why the master can be captioned for YouTube and reframed for the short
without anything colliding.

## Background music

`BACKGROUND_MUSIC=library` (the default) picks one track per video from
`assets/music/`, chosen by the run id. `story_animator music` fills the
library once with ElevenLabs Music: tracks a little longer than the video,
with briefs suited to engineering explainers (wonder, circuit, scale,
workshop). Each track is loudness-normalised to -16 LUFS, then played at
`MUSIC_VOLUME` (0.18, about 16 dB under the voice) and sidechain-ducked
under the narration, so it is only clearly audible during silent scenes.

## Visual orchestration

The engine receives each scene's **word timings** relative to the clip start.
Layouts don't play animations directly. They register *beats* ("at time t,
play X for r seconds") through `Ctx.at()`. Beat times come from
`Ctx.schedule(cues)`:

1. each element's `cue` word is looked up in the narration (exact, then prefix match);
2. elements without a found cue are interpolated between found ones;
3. times are made monotonic, with minimum gaps, and kept inside the slot.
   Cue words are searched from the start of the narration even when the
   sequence starts later (after an intro build), so a word said early is
   never matched to a repeat of it at the end;
4. the first element never waits more than 0.9s after the narration starts
   (`Ctx.first_by`), so a cue word late in the sentence can't leave an empty frame.

Silent scenes have no words, so their elements spread across the hold.

The director sorts the beats and plays them against the scene clock. Gaps
become `wait()`s, during which ambient updaters (camera drift, floating
icons, twinkling stars, blinking characters, lip-flap) keep the frame alive.
The clip is always padded or clamped to the exact spec duration.

## Project layout

```
src/
  main.rs        CLI (generate, storyboard, topics, kit, music, resume, serve, doctor, catalog)
  config.rs      .env → Config (all knobs, defaults, validation)
  catalog.rs     loads the engine's vocabulary (single source of truth)
  niche.rs       the channel brief + topic ideas
  kit.rs         grows the mechanism kit (new parts: write, validate, save); process chains
  storyboard.rs  storyboard model, chapters, silent scenes, budgets, validation (unit-tested)
  llm.rs         single-call writer, outline + parallel chapter writer, Gemini/DeepSeek, repair loops
  tts.rs         ElevenLabs TTS with timestamps + audio tags, voices, music, silent mode
  timeline.rs    exact-duration fitting incl. silent scenes (unit-tested)
  subtitles.rs   ASS captions with word highlight, long and short layouts (unit-tested)
  custom.rs      LLM-written custom scenes: generate, validate, retry, cache
  render.rs      parallel engine processes, resume, fallback
  media.rs       ffmpeg/ffprobe helpers, master compositor, caption burn, short reframe
  pipeline.rs    the end-to-end run, run directory, caching, short segment, youtube.txt
  jobs.rs        background job queue for the API
  server.rs      axum HTTP API
  doctor.rs      environment checks

engine/story_engine/
  __main__.py    CLI: render | catalog | check | validate | validate-part
  fonts.py       fontconfig setup (bundled fonts; must run before manim import)
  theme.py       11 curated themes (each with font + style: diagram/infographic)
  stage.py       frame geometry (landscape/portrait), text + shape helpers
  director.py    Ctx (cue timing, first-reveal rule, beats) + SpecScene (camera, scheduler)
  layouts.py     12 general layouts + LAYOUT_DOCS (what the LLM may write)
  diagrams.py    equation, graph, vectors, dot_grid, network (3Blue1Brown style)
  marvels.py     chapter, fun_fact, scale (long-form format)
  process.py     process (working schematics, chains, focus) and cross_section
  kit/           mechanism kit: core.py (Part, Link, particles, routing), parts.py (built-ins),
                 __init__.py (registry, library loader, PART_API), library/ (generated parts)
  custom.py      sandbox checker, loader and API reference for custom scenes
  icons.py       86 procedural icons (incl. an engineering set) + ~330 synonyms
  characters.py  rigged characters: poses, expressions, blink, lip-flap
  backgrounds.py 12 animated backdrops (incl. glow / blueprint)
```

## Run directory

```
storage/runs/<run_id>/
  request.json        the original request (used by `resume`)
  storyboard.json     edit this, then `resume`
  audio/scene_XX.mp3  narration for narrated scenes, plus scene_XX.json (text, voice, model, word timings)
  audio/music.mp3     if BACKGROUND_MUSIC=elevenlabs
  timeline.json       slots, offsets, tempo, per-scene word timings
  scenes/scene_XX.json/.mp4   render spec + clip (re-rendered only if the spec changed)
  master.mp4          clean composite (no captions); both outputs are cut from it
  master.filtergraph.txt      the exact ffmpeg graph (handy for debugging)
  subtitles.ass       captions for the long video
  final.mp4           16:9 YouTube video
  short.ass           title, captions and CTA for the short
  short.mp4           9:16 short
  short.filtergraph.txt
  youtube.txt         title, description, chapter timestamps, short title
  result.json
```

## Performance notes

On the 6-core VPS (render concurrency 3):

- A five-minute video has about 35 scenes. Draft quality (540p15) runs end
  to end, including custom scenes and the short, in about 5 minutes.
- At 1080p30, expect roughly 15–25 minutes, most of it scene rendering.
  The master, caption burn and short are three ffmpeg encodes of a few
  minutes in total.
- Fonts are loaded through a generated fontconfig file. `manimpango.register_font` is avoided on purpose: on Windows it forces Pango to rebuild its font map for every new string (about 2.5s per `Text`).
