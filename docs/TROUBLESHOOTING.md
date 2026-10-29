# Troubleshooting

Start with:

```bash
story_animator doctor
```

## Setup

**`engine` FAIL: could not load engine catalog**
Python can't import the engine or Manim. Create the venv and install:
`python -m venv venv && venv/Scripts/pip install -r engine/requirements.txt`
(macOS/Linux: `venv/bin/pip`). If your interpreter lives elsewhere, set `PYTHON_BIN`.

**`ffmpeg` FAIL: missing filters**
Your ffmpeg build lacks libass (`subtitles`) or other filters. Install a full
build: on Windows, the gyan.dev "full" build; `brew install ffmpeg`; or your
distro's package.

**Text renders in the wrong font**
`assets/fonts/Poppins-*.ttf` must exist. The engine writes a fontconfig file
to `storage/cache/fonts.conf`. Delete `storage/cache/` to regenerate it.

## Generation

**`timeline: narration is 68.2s and would need 1.28x speed to fit`**
The script is too long for 60 seconds. LLM-written scripts are automatically
rewritten once; this error means a hand-written storyboard is too long. Cut
it to about 135 words, or raise `MAX_SPEECH_TEMPO` (max 1.3).

**`could not produce a valid storyboard in 3 rounds`**
The LLM kept missing the word budget or scene rules. Retry, try a narrower
topic, or switch model (`GEMINI_MODEL`, or `LLM_PROVIDER=deepseek`).

**`llm: gemini: HTTP 404`**
`GEMINI_MODEL` names a model your key can't access. Try `gemini-2.5-flash`.

**`tts: HTTP 401` / `quota_exceeded`**
Check `ELEVENLABS_API_KEY` and your character quota. Use `--silent` for free previews.

**`tts: HTTP 429`**
Rate limited. Lower `TTS_CONCURRENCY` to 1 or 2. Retries with back-off are automatic.

**Music generation failed (warning)**
ElevenLabs Music may not be on your plan. The video still renders without
music. Use `BACKGROUND_MUSIC=path/to/track.mp3` instead.

## Rendering

**Equations show as plain italic text instead of typeset maths**
LaTeX isn't installed or isn't on PATH. The engine needs both `latex` and
`dvisvgm`. Install MiKTeX (Windows), MacTeX, or `texlive` plus `dvisvgm`, then
check with `story_animator catalog`.

**A `custom` scene rendered its fallback layout**
The AI's code didn't pass validation within `CUSTOM_ATTEMPTS` tries. The log
shows each attempt's error (`custom scene failed validation`). Options:
- Make the scene's `brief` more concrete and `resume` the run.
- Raise `CUSTOM_ATTEMPTS`.
- Write or fix `storage/runs/<id>/scenes/scene_XX.custom.py` yourself and test it:
  ```bash
  cd engine
  ../venv/Scripts/python -m story_engine validate --spec ../storage/runs/<id>/scenes/scene_XX.validate.json
  ```

**A scene looks plain (just a title card)**
That scene's layout failed and the safe fallback was used. Check
`storage/runs/<id>/scenes/scene_XX.fallback.json` and run the original spec
directly to see the error:
```bash
cd engine
../venv/Scripts/python -m story_engine render --spec ../storage/runs/<id>/scenes/scene_03.json --out test.mp4
```

**Rendering is slow**
Use `-q draft` while iterating (about 10x faster), raise `RENDER_CONCURRENCY`
if you have spare cores, and use `resume` so unchanged scenes aren't re-rendered.

**`resume` doesn't pick up an engine code change**
Resume reuses a scene clip when its *spec* is unchanged. After editing
engine code, delete `storage/runs/<id>/scenes/*.mp4` to force a re-render.

## Output

**Video isn't exactly 60s**
It should be, to the frame. `result.json` records the measured duration. If
it differs, attach `final.filtergraph.txt` from the run folder when reporting.

**Captions overlap the visuals in portrait**
Captions sit in the lower quarter, above platform UI. Layouts keep that zone
clear. If you use a custom `SUBTITLE_FONT` that's much wider, captions may
wrap. Keep Poppins or reduce the font size in `src/subtitles.rs`.
