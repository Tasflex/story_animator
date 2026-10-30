# Story Animator

**Topic in: a five-minute animated explainer for YouTube (16:9) and a vertical short (9:16) out.**

```bash
story_animator generate "How undersea fiber cables carry the internet"
```

That one command plans the video in chapters, writes the script, narrates it
with an expressive voice, animates it scene by scene, and delivers:

- `final.mp4`: **exactly 5:00**, 1920x1080, with chapters, word-highlight captions and quiet background music
- `short.mp4`: a **59-second-or-less** 1080x1920 short, its scenes **rendered natively in 9:16** (same script and narration), with a hook title and vertical captions
- `youtube.txt`: title, description and chapter timestamps, ready to paste

---

## The channel: engineering marvels and how things work

Targeting everything is targeting nothing. Story Animator makes one kind of
video well: **how machines, structures, systems and nature's engineering
work**, explained with animated diagrams and made memorable with fun facts,
records and true-scale comparisons.

- Every topic is scored against the channel brief while it is planned
  ([src/niche.rs](src/niche.rs)). Off-niche topics (news, celebrities,
  opinions, advice) are refused with the reason; `--force` overrides.
- `story_animator topics` suggests topics that suit the format, each with a hook and three wow facts.
- Every middle chapter must land a "wow" on screen (`fun_fact`, `scale` or `stat`).

## What you get

| | |
|---|---|
| **Five-minute videos, built in chapters** | An outline comes first (chapters, the wow each lands, which chapter becomes the short), then each chapter is written in parallel against its own word budget. Silent chapter cards separate the acts. |
| **Exact length** | Every video lands on `TARGET_DURATION_SECS` (300s) to the frame. Narration is measured, pauses are spread, and a script that would run long or short is rewritten before any narration is paid for. |
| **Expressive narration** | ElevenLabs `eleven_v4` with audio tags (`[curious]`, `[amazed]`, `[serious]`…) chosen per scene by the writer. Tags never show in captions. |
| **Fewer TTS credits** | Silent scenes (chapter cards, visual beats where the animation speaks for itself) are never sent to ElevenLabs. Narration is cached per scene, so edits and re-runs only pay for what changed. `--silent` previews cost nothing. |
| **Two formats, both fully framed** | A clean 16:9 master gets captions burned in for YouTube. The short's scenes are rendered again in 9:16 by the portrait layouts, filling the whole frame (no crop, no blurred fill), with a header band for the hook title and a footer for captions that the animation never runs under. Narration is reused, so the short costs render time, not TTS credits. |
| **Gets better every video** | Each video is self-reviewed (16:9 and the short) against the scorecard of earlier ones. If it scores below the best so far, the scenes at fault are fixed and re-reviewed, and the fix is kept only if it scores higher. What failed and what worked go into `storage/LESSONS.md` for the next video. See [docs/LEARNING.md](docs/LEARNING.md). |
| **See how it works** | Most scenes are **working schematics**, not icon cards: the `process` layout builds the system from kit parts (tanks, valves, pumps, gears, pistons, lasers, ships…) joined by pipes, wires, rays or fibres, then runs it on the narrator's words. Valves open, water flows, levels rise, a ship floats up, rotors spin, the camera moves in on the part being explained. One diagram grows across a chapter (`continue`). `cross_section` cuts things open layer by layer. Icon-based layouts are capped at 20% of scenes. |
| **A kit that grows** | When a video needs a part the kit doesn't have (a canal mule locomotive, a heat exchanger…), the AI draws it, it is sandbox-checked and test-run, and it is **saved to the kit library** for every later video. `story_animator kit` lists the parts. |
| **Orchestrated visuals** | Each element appears as the narrator says its name, driven by word timestamps, and every scene shows something within about a second. |
| **23 animated layouts + custom scenes** | Mechanism: process, cross_section. Long-form: chapter, fun_fact, scale. Diagram: equation, graph, vectors, dot_grid, network. General: title, hero, icon_row, list, compare, stat, bar_chart, flow, timeline, character, quote, cycle. Plus **custom** (max 2 per video): the AI writes a one-off animation, safety-checked, with a fallback. |
| **25+ kit parts, 86 icons** | Schematic parts with ports, actions and flowing particles; flat icons (including an engineering set) for the occasional card. 11 themes, 12 animated backgrounds, 6 camera moves, 14 transitions. |
| **Background music** | A small library of tracks generated once with ElevenLabs Music (`story_animator music`), reused across videos, mixed low and ducked under the voice. |
| **Robust rendering** | Scenes render in parallel as independent processes. A failing scene falls back to a safe layout instead of killing the video. |
| **CLI + HTTP API** | Use it from the terminal or as a job-queue service. |

## How it works

```
 topic ─► niche fit + outline ─► chapters (parallel) ─► free length check ─► ElevenLabs v4 (narrated scenes only)
                                                                                   │
            timeline fit (= 300.000s) ◄────────────────────────────────────────────┘
                    │
                    ▼
     new kit parts (built once, kept) ─► Manim scenes (parallel)
                    │
                    ▼
               master.mp4 ──────────────────┬─► + captions ─────────────────► final.mp4   (16:9, 5:00)
                                             └─► short's scenes rendered in 9:16 ─► short.mp4   (≤ 59s)
```

The LLM mostly writes **data**: a structured storyboard (which parts, how
they connect, what happens on which word) that a deterministic Python/Manim
engine turns into animation. Code is written only for a new kit part (kept
and reused) or a rare one-off `custom` scene, and both are sandbox-checked,
dry-run and retried with the error, with a safe fallback if they still fail.
Read more in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Quick start

### 1. Prerequisites

- **Rust** 1.85+ (`rustup`)
- **Python** 3.10–3.12
- **ffmpeg + ffprobe**: a full build with libass (Linux: your distro's `ffmpeg`)
- Manim's system dependencies. On Linux: `sudo apt install libcairo2-dev libpango1.0-dev pkg-config`
- *(Recommended)* **LaTeX** for typeset maths: `texlive` + `dvisvgm`. Without it, equations render as plain italic text.

### 2. Install

```bash
git clone <repo> story_animator && cd story_animator

python3 -m venv venv
venv/bin/pip install -r engine/requirements.txt

cargo build --release
cp .env.example .env        # then add your API keys
```

### 3. Check your setup and fill the music library

```bash
target/release/story_animator doctor
target/release/story_animator music      # once: generates 3 background tracks into assets/music
```

```
[ok  ] engine   {"ok": true, "manim": "0.21.0", "font": "Poppins", "layouts": 23}
[ok  ] ffmpeg   all required filters present
[ok  ] llm      gemini (gemini-3.5-flash) + deepseek fallback
[ok  ] tts      elevenlabs (eleven_v4), voice JBFqnCBsd6RMkjVDRZzb
[ok  ] music    library: 3 track(s) in assets/music
```

### 4. Make videos

```bash
# Ideas that fit the channel
story_animator topics
story_animator topics --area "bridges and tunnels" --count 5

# The mechanism kit (built-in and generated parts)
story_animator kit

# Review the script before spending TTS credits (recommended)
story_animator storyboard "How a mechanical watch keeps time" -o watch.json
#   ...read/edit watch.json...
story_animator generate --storyboard watch.json --silent -q draft    # free preview, ~5 minutes
story_animator generate --storyboard watch.json -o watch.mp4         # the real thing (+ watch_short.mp4)

# Or straight from a topic
story_animator generate "How Panama Canal locks lift ships" -o canal.mp4

# Tweak a finished run: edit storage/runs/<id>/storyboard.json, then
story_animator resume <id>
```

(`story_animator` here means `target/release/story_animator`.)

Each run lives in `storage/runs/<run_id>/` with its storyboard, narration,
timeline, scene clips, `master.mp4`, `final.mp4`, `short.mp4` and `youtube.txt`.

### 5. Or run it as a service

```bash
story_animator serve           # http://localhost:3000
curl -X POST localhost:3000/api/videos -H 'content-type: application/json' \
     -d '{"topic": "How wind turbines turn wind into power"}'
curl localhost:3000/api/videos/<id>                        # progress
curl -o turbine.mp4       localhost:3000/api/videos/<id>/video
curl -o turbine_short.mp4 localhost:3000/api/videos/<id>/short
```

See [docs/API.md](docs/API.md).

## Documentation

| Doc | What's in it |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Pipeline stages, chaptered writing, timing model, the short, credit savings, project layout |
| [docs/STORYBOARD.md](docs/STORYBOARD.md) | Write your own storyboards: every layout, field and option, silent scenes, audio tags |
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md) | Every `.env` setting |
| [docs/API.md](docs/API.md) | HTTP endpoints |
| [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) | Common problems and fixes |
| [docs/PACKAGING.md](docs/PACKAGING.md) | Rules for thumbnails, titles, descriptions and tags, and the thumbnail command |
| [docs/LEARNING.md](docs/LEARNING.md) | How each video's self-review and your feedback make the next one better (`storage/LESSONS.md`) |
| [docs/TELEGRAM.md](docs/TELEGRAM.md) | The Telegram bot: daily render queue, delivery, and changes by chat |

## Project layout

```
src/                 Rust orchestrator (CLI, API, pipeline, niche, writer, kit growth)
engine/story_engine/ Python render engine (Manim): layouts, mechanism kit, icons, characters, backgrounds
engine/story_engine/kit/library/  Kit parts generated for earlier videos (committed; reused)
assets/fonts/        Poppins + Libre Baskerville (SIL Open Font License)
assets/music/        Background-music library (generated; git-ignored)
examples/            Ready-to-render storyboards
docs/                Documentation
storage/runs/        One folder per generated video (git-ignored)
```

## Costs

A five-minute video uses about 8–15 LLM calls (outline, chapters, repairs,
custom scenes) and about 3,000–3,500 characters of ElevenLabs TTS: roughly
550–600 spoken words, since chapter cards and visual beats are silent. The
short reuses the same narration, so it costs no TTS (only render time, plus an LLM call per custom scene in it). `--silent` and
re-runs of unchanged scenes cost nothing. Music is generated once for the
library (`story_animator music`); ElevenLabs bills music separately from TTS
characters.

## License

Code: yours. Fonts: Poppins and Libre Baskerville, SIL Open Font License 1.1
(`assets/fonts/OFL-*.txt`).
