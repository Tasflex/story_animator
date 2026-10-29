# Story Animator

**Topic in, polished 60-second 2D animated video out.**

```bash
story_animator generate "How do black holes form?"
```

That one command writes a script, narrates it, animates it scene by scene,
and delivers an MP4 that is **exactly 60.00 seconds** long. It has
word-synced visuals, word-by-word captions, crossfade transitions and optional
background music, in 16:9 or 9:16 (Shorts, Reels, TikTok).

---

## What you get

| | |
|---|---|
| **Exact length** | Every video lands on `TARGET_DURATION_SECS` (60s) to the frame. Narration is measured, pauses are distributed, and over-long scripts are gently sped up or sent back to the writer. |
| **Two visual styles** | **diagram** (the default): 3Blue1Brown-like, with serif type, LaTeX maths, graphs, vectors, dot populations, networks and navy glow. **infographic**: flat icons, cards and characters. |
| **Orchestrated visuals** | Each element appears *as the narrator says its name*, driven by ElevenLabs word timestamps. |
| **17 animated layouts + custom scenes** | Diagram: equation, graph, vectors, dot_grid, network. General: title, hero, icon_row, list, compare, stat, bar_chart, flow, timeline, character, quote, cycle. Plus **custom**: the AI writes a one-off animation for one scene, which is safety-checked and test-run, with a fallback layout if it fails. |
| **72 vector icons, 11 themes, 12 animated backgrounds, 6 camera moves, 14 transitions** | All drawn procedurally, so nothing has to be downloaded and everything follows the theme colours. |
| **Rigged characters** | Poses, expressions, blinking, and a mouth that moves only while words are spoken. |
| **Captions** | Burned-in, with the currently spoken word highlighted in the theme accent. |
| **Robust rendering** | Scenes render in parallel as independent processes. A failing scene falls back to a safe layout instead of killing the video. |
| **Resumable runs** | Edit a storyboard and re-run: only changed scenes are re-narrated or re-rendered. |
| **CLI + HTTP API** | Use it from the terminal or as a job-queue service. |

## How it works

```
 topic ──► LLM storyboard (JSON) ──► ElevenLabs TTS + word timings ──► timeline fit (= 60.000s)
                                                                              │
     final.mp4 ◄── ffmpeg: crossfades · narration · music · captions ◄── Manim scenes (parallel)
```

The LLM mostly writes **data**: a structured storyboard using the vocabulary
the engine supports, which a deterministic Python/Manim engine turns into
animation. For the odd scene that needs a bespoke diagram (a drone's rotors,
a shape being cut and rearranged), it writes code for **that one scene**.
The code is sandbox-checked, dry-run, retried with the error, and falls back
to a safe layout if it still fails, so one bad diagram can never break the
video. Read more in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Quick start

### 1. Prerequisites

- **Rust** 1.85+ (`rustup`)
- **Python** 3.10–3.12
- **ffmpeg + ffprobe**: a full build with libass (Windows: [gyan.dev](https://www.gyan.dev/ffmpeg/builds/) "full"; macOS: `brew install ffmpeg`; Linux: your distro's `ffmpeg`)
- Manim's system dependencies. On Linux: `sudo apt install libcairo2-dev libpango1.0-dev pkg-config`. Windows and macOS wheels include them.
- *(Recommended)* **LaTeX** for typeset maths in the diagram style: MiKTeX (Windows), MacTeX, or `texlive` + `dvisvgm`. Without it, equations render as plain italic text.

### 2. Install

```bash
git clone <repo> story_animator && cd story_animator

python -m venv venv
# Windows:  venv\Scripts\activate      macOS/Linux:  source venv/bin/activate
pip install -r engine/requirements.txt

cargo build --release
cp .env.example .env        # then add your API keys
```

### 3. Check your setup

```bash
cargo run --release -- doctor
```

```
[ok  ] engine   {"ok": true, "manim": "0.21.0", "font": "Poppins", "layouts": 18}
[ok  ] ffmpeg   all required filters present
[ok  ] ffprobe  ok
[ok  ] fonts    assets/fonts
[ok  ] llm      gemini (gemini-2.5-flash) + deepseek fallback
[ok  ] tts      elevenlabs (eleven_multilingual_v2), voice auto
[ok  ] music    none
```

### 4. Make videos

```bash
# Free previews: no API calls, silent narration, 540p. About 30 seconds each.
story_animator generate --storyboard examples/base_rate.json --silent -q draft   # diagram style
story_animator generate --storyboard examples/bees.json --silent -q draft        # infographic style

# The real thing (diagram style by default)
story_animator generate "Why do we dream?" -o dreams.mp4

# Flat infographic style, vertical for Shorts / Reels / TikTok
story_animator generate "The history of coffee" --style infographic --aspect portrait \
    --theme sunset --audience "curious teenagers" -o coffee.mp4

# Review the script before spending TTS credits
story_animator storyboard "How vaccines work" -o vaccines.json
#   ...edit vaccines.json...
story_animator generate --storyboard vaccines.json

# Tweak a finished run: edit storage/runs/<id>/storyboard.json, then
story_animator resume <id>
```

(`story_animator` here means `target/release/story_animator`, or `cargo run --release --`.)

Each run lives in `storage/runs/<run_id>/` along with its storyboard,
narration, timeline, scene clips and `final.mp4`.

### 5. Or run it as a service

```bash
story_animator serve           # http://localhost:3000
curl -X POST localhost:3000/api/videos -H 'content-type: application/json' \
     -d '{"topic": "Why is the sky blue?", "aspect": "portrait"}'
curl localhost:3000/api/videos/<id>              # progress
curl -o sky.mp4 localhost:3000/api/videos/<id>/video
```

See [docs/API.md](docs/API.md).

## Documentation

| Doc | What's in it |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Pipeline stages, timing model, how exact 60s is guaranteed, project layout |
| [docs/STORYBOARD.md](docs/STORYBOARD.md) | Write your own storyboards: every layout, field and option |
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md) | Every `.env` setting |
| [docs/API.md](docs/API.md) | HTTP endpoints |
| [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) | Common problems and fixes |

## Project layout

```
src/                 Rust orchestrator (CLI, API, pipeline)
engine/story_engine/ Python render engine (Manim): layouts, icons, characters, backgrounds
assets/fonts/        Poppins + Libre Baskerville (SIL Open Font License)
examples/            Ready-to-render storyboards
docs/                Documentation
storage/runs/        One folder per generated video (git-ignored)
```

## Costs

A 60-second video uses about 1–3 LLM calls and about 900 characters of
ElevenLabs TTS. `--silent` and hand-written storyboards cost nothing.
`BACKGROUND_MUSIC=elevenlabs` adds one music generation per video.

## License

Code: yours. Fonts: Poppins and Libre Baskerville, SIL Open Font License 1.1
(`assets/fonts/OFL-*.txt`).
