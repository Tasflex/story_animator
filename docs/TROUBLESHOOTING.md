# Troubleshooting

Start with:

```bash
story_animator doctor
```

## Setup

**`engine` FAIL: could not load engine catalog**
Python can't import the engine or Manim. Create the venv and install:
`python3 -m venv venv && venv/bin/pip install -r engine/requirements.txt`
(Windows: `venv/Scripts/pip`). If your interpreter lives elsewhere, set `PYTHON_BIN`.

**`ffmpeg` FAIL: missing filters**
Your ffmpeg build lacks libass (`subtitles`) or other filters. Install a full
build: on Windows, the gyan.dev "full" build; `brew install ffmpeg`; or your
distro's package.

**Text renders in the wrong font**
`assets/fonts/Poppins-*.ttf` must exist. The engine writes a fontconfig file
to `storage/cache/fonts.conf`. Delete `storage/cache/` to regenerate it.

## Generation

**`invalid request: topic doesn't fit the channel niche (scored 3/10: ...)`**
The outline judged the topic off-niche (see the brief in `src/niche.rs`).
Pick an engineering or how-it-works angle on it, ask `story_animator topics`
for ideas, or pass `--force` (API: `"force": true`) to make it anyway. To
turn the check off entirely, set `MIN_TOPIC_FIT=0`.

**`timeline: narration is 290.4s and would need 1.24x speed to fit`**
The script is too long for the target. LLM-written scripts are rewritten
automatically (first using free estimated timings, then once more after
narration if needed); this error means a hand-written storyboard is too
long. Cut it to about 610 spoken words for five minutes, or raise
`MAX_SPEECH_TEMPO` (max 1.3).

**`timeline: narration is only 150.0s (...); lengthen the script`**
The narration would leave too much dead air. As above, this only stops
hand-written storyboards: add narration or a few scenes, or lower
`TARGET_DURATION_SECS`.

**`could not produce a valid outline` / `chapter N failed after 3 rounds`**
The LLM kept missing the chapter rules or word budget. Retry, try a
narrower topic, or switch model (`GEMINI_MODEL`, or `LLM_PROVIDER=deepseek`).
A chapter that is close enough is used anyway with a warning.

**`llm: gemini: HTTP 404`**
`GEMINI_MODEL` names a model your key can't access. Try `gemini-3.5-flash` or `gemini-2.5-flash`.

**`tts: HTTP 401` / `quota_exceeded`**
Check `ELEVENLABS_API_KEY` and your character quota. Use `--silent` for free previews.

**`tts: HTTP 429`**
Rate limited. Lower `TTS_CONCURRENCY` to 1 or 2. Retries with back-off are automatic.

**The narrator reads "[excited]" out loud**
The model doesn't support audio tags. Use `ELEVENLABS_MODEL=eleven_v4` (or
`eleven_v3`). With older models the writer leaves tags out, but a
hand-written storyboard may still contain them.

**`music library ... is empty` (warning) / no music**
Run `story_animator music` once to generate the library into
`assets/music/`, or drop your own tracks (mp3, wav, m4a, ogg, flac) into that
folder. Tracks shorter than the video loop.

**`story_animator music` fails**
ElevenLabs Music may not be on your plan. Videos still render without
music. Use your own tracks in `assets/music/` instead.

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
  ../venv/bin/python -m story_engine validate --spec ../storage/runs/<id>/scenes/scene_XX.validate.json
  ```

**A scene looks plain (just a title card)**
That scene's layout failed and the safe fallback was used. Check
`storage/runs/<id>/scenes/scene_XX.fallback.json` and run the original spec
directly to see the error:
```bash
cd engine
../venv/bin/python -m story_engine render --spec ../storage/runs/<id>/scenes/scene_03.json --out test.mp4
```

**Rendering is slow**
Use `-q draft` while iterating (about 10x faster), raise `RENDER_CONCURRENCY`
if you have spare cores, and use `resume` so unchanged scenes aren't re-rendered.

**`resume` doesn't pick up an engine code change**
Resume reuses a scene clip when its *spec* is unchanged. After editing
engine code, delete `storage/runs/<id>/scenes/*.mp4` to force a re-render.

## Output

**Video isn't exactly 5:00**
It should be, to the frame. `result.json` records the measured duration. If
it differs, attach `master.filtergraph.txt` from the run folder when reporting.

**No `short.mp4`**
A short is made only for landscape videos with `SHORTS=1` and a `short`
range in the storyboard. LLM-written storyboards always have one;
hand-written ones need `"short": {"from": ..., "to": ..., "title": ...}`.
The log says `storyboard picks no segment for a short` when it's missing.

**The short starts or ends awkwardly**
The cut follows scene boundaries inside the chosen chapter, trimmed to fit
`SHORT_MAX_SECS`. Edit `short.from`/`short.to` in
`storage/runs/<id>/storyboard.json` and `resume`. No scene is re-rendered
or re-narrated; only the final ffmpeg encodes run again.

**Captions overlap the visuals in portrait**
Captions sit in the lower quarter, above platform UI. Layouts keep that zone
clear. If you use a custom `SUBTITLE_FONT` that's much wider, captions may
wrap. Keep Poppins or reduce the font size in `src/subtitles.rs`.
