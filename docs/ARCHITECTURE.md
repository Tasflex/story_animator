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
That survives in a contained form. A storyboard may mark up to two scenes
as `custom`, and for those the LLM writes code for that **one scene**,
against the same small `ctx` API the built-in layouts use. The code is
statically sandbox-checked, dry-run, and retried with the error message.
If it still fails, the scene's declared `fallback` layout renders instead.
The blast radius of a mistake is one scene, not the whole video.

## Pipeline

```
                ┌──────────────┐
  topic ───────►│ 1 storyboard │  llm.rs: Gemini (DeepSeek fallback), JSON only,
                └──────┬───────┘  validated + repaired up to 3 rounds
                       ▼
                ┌──────────────┐
                │ 2 narration  │  tts.rs: ElevenLabs /with-timestamps per scene,
                └──────┬───────┘  previous/next text for natural flow, cached
                       ▼
                ┌──────────────┐
                │ 3 timeline   │  timeline.rs: slots summing to exactly 60.000s
                └──────┬───────┘  (auto-tightens script once if too long)
                       ▼
                ┌──────────────┐
                │ 4 captions   │  subtitles.rs: ASS, word-by-word highlight
                │   + music    │  optional ElevenLabs Music bed
                └──────┬───────┘
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
                │ 6 compose    │  media.rs: one ffmpeg graph: xfade chain,
                └──────┬───────┘  narration at exact offsets, ducked music,
                       ▼          loudness normalisation, captions
                   final.mp4
```

## The timing model (how "exactly 60s" works)

Each scene `i` gets a **slot** on the final timeline:

```
slot_i = lead_i + narration_i / tempo + tail_i
```

- `lead` is a short silence before the words (0.6s for the first scene, 0.35s otherwise).
- `tail` is a breathing pause (≥ 0.45s; the last scene holds ≥ 1.8s as an outro).
- If narration is short, the spare time (slack) is spread across the tails.
- If narration is long, `tempo` rises up to `MAX_SPEECH_TEMPO` (1.15x, applied with pitch-preserving `atempo`).
- If even that isn't enough, the writer is asked once to shorten the script.

Slots always sum to exactly the target. Scene clips are rendered
`slot + transition` long, except the last. The ffmpeg `xfade` for scene `i`
starts at `start_i`. So the output length is:

```
Σ clip_len − (n−1)·transition = Σ slot = 60.000s
```

Narration clips are placed with `adelay` at `start_i + lead_i`, so audio
and visuals never drift. Unit tests in `timeline.rs` lock this in.

## Visual orchestration

The engine receives each scene's **word timings** relative to the clip start.
Layouts don't play animations directly. They register *beats* ("at time t,
play X for r seconds") through `Ctx.at()`. Beat times come from
`Ctx.schedule(cues)`:

1. each element's `cue` word is looked up in the narration (exact, then prefix match);
2. elements without a found cue are interpolated between found ones;
3. times are made monotonic, with minimum gaps, and kept inside the slot.

The director sorts the beats and plays them against the scene clock. Gaps
become `wait()`s, during which ambient updaters (camera drift, floating
icons, twinkling stars, blinking characters, lip-flap) keep the frame alive.
The clip is always padded or clamped to the exact spec duration.

## Project layout

```
src/
  main.rs        CLI (generate, storyboard, resume, serve, doctor, catalog)
  config.rs      .env → Config (all knobs, defaults, validation)
  catalog.rs     loads the engine's vocabulary (single source of truth)
  storyboard.rs  storyboard model, word budget, validation, normalisation
  llm.rs         prompt + Gemini/DeepSeek + repair loop
  tts.rs         ElevenLabs TTS with timestamps, voices, music, silent mode
  timeline.rs    exact-duration fitting (unit-tested)
  subtitles.rs   ASS captions with word highlight (unit-tested)
  custom.rs      LLM-written custom scenes: generate, validate, retry, cache
  render.rs      parallel engine processes, resume, fallback
  media.rs       ffmpeg/ffprobe helpers + final compositor
  pipeline.rs    the end-to-end run, run directory, caching
  jobs.rs        background job queue for the API
  server.rs      axum HTTP API
  doctor.rs      environment checks

engine/story_engine/
  __main__.py    CLI: render | catalog | check | validate
  fonts.py       fontconfig setup (bundled fonts; must run before manim import)
  theme.py       11 curated themes (each with font + style: diagram/infographic)
  stage.py       frame geometry (landscape/portrait), text + shape helpers
  director.py    Ctx (cue timing, beats) + SpecScene (camera, scheduler)
  layouts.py     12 general layouts + LAYOUT_DOCS (what the LLM may write)
  diagrams.py    equation, graph, vectors, dot_grid, network (3Blue1Brown style)
  custom.py      sandbox checker, loader and API reference for custom scenes
  icons.py       72 procedural icons + 260 synonyms
  characters.py  rigged characters: poses, expressions, blink, lip-flap
  backgrounds.py 12 animated backdrops (incl. glow / blueprint)
```

## Run directory

```
storage/runs/<run_id>/
  request.json        the original request (used by `resume`)
  storyboard.json     edit this, then `resume`
  audio/scene_XX.mp3  narration, plus scene_XX.json (text, voice, word timings)
  audio/music.mp3     if generated
  timeline.json       slots, offsets, tempo, per-scene word timings
  scenes/scene_XX.json/.mp4   render spec + clip (re-rendered only if the spec changed)
  subtitles.ass
  final.mp4
  final.filtergraph.txt       the exact ffmpeg graph (handy for debugging)
  result.json
```

## Performance notes

- A 60s video at 1080p30 renders 8 scenes on 3–4 parallel processes in a few minutes; draft quality takes about 30 seconds.
- Fonts are loaded through a generated fontconfig file. `manimpango.register_font` is avoided on purpose: on Windows it forces Pango to rebuild its font map for every new string (about 2.5s per `Text`).
