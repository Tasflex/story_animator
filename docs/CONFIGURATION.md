# Configuration

All settings come from environment variables, usually set in `.env` in the
project root (copy `.env.example`). Inline `# comments` are allowed.
Empty values mean "use the default".

## Storyboard writer (LLM)

| Variable | Default | Description |
|---|---|---|
| `GEMINI_API_KEY` | — | Google AI Studio key. The primary writer. |
| `GEMINI_MODEL` | `gemini-2.5-flash` | Any Gemini model that supports JSON output (the server uses `gemini-3.5-flash`) |
| `DEEPSEEK_API_KEY` | — | Fallback writer (used if Gemini fails or isn't configured) |
| `DEEPSEEK_MODEL` | `deepseek-chat` | |
| `LLM_PROVIDER` | `auto` | `auto` (Gemini, then DeepSeek), `gemini`, or `deepseek` |

At least one key is needed to generate from a topic. Hand-written
storyboards (`--storyboard`) don't need one.

## Narration

| Variable | Default | Description |
|---|---|---|
| `ELEVENLABS_API_KEY` | — | Required unless `TTS_PROVIDER=silent` |
| `ELEVENLABS_VOICE_ID` | first premade voice | The voice. Find IDs in the ElevenLabs Voice Library. Can be overridden per video. |
| `ELEVENLABS_MODEL` | `eleven_v4` | `eleven_v3`/`eleven_v4` follow audio tags (`[curious]`, `[excited]`…) that the writer adds for expressive delivery. `eleven_v4_turbo` costs half as much per character. Older models (`eleven_multilingual_v2`, `eleven_flash_v2_5`) work too; the writer then leaves tags out. |
| `TTS_PROVIDER` | `elevenlabs` | `silent` = no audio, estimated word timings (free previews) |
| `TTS_CONCURRENCY` | `3` | Parallel TTS requests (lower it if you hit 429 rate limits) |

Only narrated scenes are sent to ElevenLabs; chapter cards and visual beats are silent.

## Channel

| Variable | Default | Description |
|---|---|---|
| `MIN_TOPIC_FIT` | `6` | The outline scores each topic 1–10 against the channel brief in `src/niche.rs`. Below this, generation stops with the reason. `--force` (or `"force": true` in the API) overrides. `0` disables the check. |

## Visual style

| Variable | Default | Description |
|---|---|---|
| `VISUAL_STYLE` | `diagram` | `diagram`: 3Blue1Brown-like (serif, LaTeX, graphs, vectors, dot grids, networks, `blueprint`/`graphite` themes). `infographic`: flat icons, cards and characters. Override per video with `--style`. |
| `CUSTOM_SCENES` | `1` | Allow the AI to write one-off `custom` diagram scenes (validated, with fallbacks). `0` uses each scene's fallback layout instead. |
| `CUSTOM_ATTEMPTS` | `3` | Write → validate → fix rounds per custom scene, and per new kit part, before falling back (1–6) |

## Video

| Variable | Default | Description |
|---|---|---|
| `TARGET_DURATION_SECS` | `300` | Exact length of every video (15–600). The word budget scales with it. From 120s up, scripts are written chapter by chapter. |
| `ASPECT` | `landscape` | `landscape` (1920×1080) or `portrait` (1080×1920). Keep `landscape` for long videos: the vertical short is cut from it. |
| `SHORTS` | `1` | Also cut a 9:16 short from every landscape video (from the chapter the writer picked) |
| `SHORT_MAX_SECS` | `59` | Upper bound for the short (15–60). The cut is trimmed at scene boundaries to fit. |
| `RENDER_QUALITY` | `high` | `draft` 540p/15fps · `standard` 720p/30 · `high` 1080p/30 · `ultra` 1080p/60 |
| `TRANSITION_SECS` | `0.5` | Crossfade length between scenes (0 = hard cuts) |
| `MAX_SPEECH_TEMPO` | `1.15` | Maximum narration speed-up when the script runs long (1.0–1.3) |

## Audio

| Variable | Default | Description |
|---|---|---|
| `BACKGROUND_MUSIC` | `library` | `library`: one track per video from `assets/music/` (fill it once with `story_animator music`), or a path to another folder. `elevenlabs`: generate a score per video from the storyboard's `music` brief (costs every time). A path to one audio file (looped and trimmed). `none`. |
| `MUSIC_VOLUME` | `0.18` | Music level (0–1) applied after each track is loudness-normalised to -16 LUFS: 0.18 puts the bed about 16 dB under the voice. It ducks further while the narrator speaks, so it is mainly heard in silent scenes. |
| `BURN_SUBTITLES` | `1` | Burn word-highlight captions into the video |
| `SUBTITLE_FONT` | `Poppins` | Must be in `assets/fonts` or installed system-wide |

Narration is loudness-normalised to about -15 LUFS (a good level for YouTube and social platforms).

## Rendering

| Variable | Default | Description |
|---|---|---|
| `PYTHON_BIN` | `./venv` python, else `python`/`python3` | Interpreter that has `manim` installed |
| `RENDER_CONCURRENCY` | half your CPU cores (max 4) | Scenes rendered in parallel |
| `RENDER_TIMEOUT_SECS` | `900` | Per-scene render timeout |
| `FFMPEG_BIN` / `FFPROBE_BIN` | `ffmpeg` / `ffprobe` | Needs a build with libass, `xfade`, `loudnorm` |
| `STORAGE_DIR` | `storage` | Where runs are written |

## Server

| Variable | Default | Description |
|---|---|---|
| `SERVER_ADDR` | `0.0.0.0:3000` | Bind address for `serve` |
| `MAX_CONCURRENT_JOBS` | `1` | Videos generated at once (each already renders scenes in parallel) |

## Logging

| Variable | Default | Description |
|---|---|---|
| `RUST_LOG` | `info` | e.g. `debug`, `story_animator=debug` |
| `LOG_JSON` | `0` | `1` for JSON log lines |

## Per-video overrides

The CLI flags and API fields override the environment for one video:
`--style`, `--aspect`, `-q/--quality`, `--theme`, `--voice`, `--audience`, `--silent`, `--no-music`, `--force`.
