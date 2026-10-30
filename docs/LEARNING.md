# How the system learns from its videos

Every video is planned against what the earlier ones got wrong and what the
best ones got right, and each video is pushed to score at least as well as
the best one before it. The memory is one Markdown file on the server,
**`storage/LESSONS.md`**:

| Section | What it holds | Who reads it |
|---|---|---|
| **Lessons** | Up to 30 rules: what went wrong, don't repeat it. *(owner)* lessons are yours. | Writer, packager, reviewer |
| **What works** | Up to 12 techniques from videos that scored 7.5 or more: keep doing them. | Writer, packager, reviewer |
| **Scorecard** | One row per video: date, run, title, score, biggest problem. | Reviewer, `improve` |

```
                ┌────────────── storage/LESSONS.md ◄────────────────────┐
                │  lessons · what works · scorecard                      │
                ▼                                                        │
 generate (outline + chapters written      review: measures + looks at every scene
 with lessons + what works in the prompt)  of the video AND the 9:16 short, scores it
                │                          against the best earlier video, adds lessons
                ▼                                                        ▲
             review ──► improve: below the best so far? fix the scenes ──┤
                         at fault, re-render, re-review; keep only if    │
                         the score went up                               │
                ▼                                                        │
 package (title/thumbnail with lessons    your feedback in Telegram ─────┘
 + PACKAGING.md, thumbnail checked)       (/feedback, change requests)
```

## Where lessons come from

1. **The self-review** (`story_animator review <run>`) after every render. It
   measures what can be measured:
   - process parts the kit can't draw (they show as empty placeholder boxes);
   - scenes that fell back to a safe layout, failed custom scenes, and
     scenes of the short that failed to render;
   - how much of the video shows a mechanism, and how hard the speech was sped up;
   - every number shown on screen.

   It shows the LLM a **contact sheet** (`<run>/contact.png`): one frame
   per scene of the 16:9 video, then a SHORT row with one frame per scene of
   the finished 9:16 short (hook title and captions burned in), together
   with the script and the **scorecard** (the best video so far and the
   most recent ones). The result is a score out of 10 on the same scale as
   before, the problems, facts to check, one sentence on how it compares
   with the best earlier video (`vs_best`), and 2-5 general lessons, all in
   `<run>/review.json`. Lessons are merged into **Lessons**; if the video
   scored 7.5 or more, its strengths are merged into **What works**.
2. **You.** `/feedback` in the bot, `story_animator feedback "<rule>"`, or a
   change request that states a general preference. These are saved as
   **(owner)** lessons: the merge never drops or rewrites them, and they
   override any lesson that contradicts them.

## Each video should beat the last ones: `improve`

After the review, `story_animator improve <run>` (run by the bot on every
video) compares the score with a **target**: the best score on the
scorecard, or `IMPROVE_TARGET` (8) if that is higher, capped at 9.5.

Below the target, the review's problems and measured failures become a
change request for the scene editor (the same one `revise` uses). Only the
scenes at fault are rewritten, re-narrated (narration is cached by text, so
unchanged scenes cost nothing) and re-rendered, then the run is reviewed
again. **The new version is kept only if it scores higher.** Otherwise the
previous video, storyboard, narration and review are put back, so an
attempt never makes a video worse. Each attempt is logged in
`<run>/improve.json`, and the bot reports it with the review. A failed
attempt still adds its review's lessons, so it teaches the next video too.

Settings: `AUTO_IMPROVE` (1), `IMPROVE_TARGET` (8.0), `IMPROVE_ROUNDS` (1).

## How lessons are used

- **Writing:** the outline and every chapter prompt include Lessons and What
  works (`llm.rs`, via `StoryRequest.lessons`).
- **Packaging:** the title, description, tags and thumbnail prompt includes
  them and [PACKAGING.md](PACKAGING.md). The rendered thumbnail is then
  shown back to the LLM, scored against the rules, and redone once if it
  scores under 7.

## Keeping it useful

New items are merged by an LLM into de-duplicated lists, most important
first. Where a new lesson contradicts an old one, the newer wins. Lessons
that only apply to one topic are left out. A video reviewed again (after a
revision or an improvement) replaces its scorecard row rather than adding a
second one. The file is plain Markdown; edit or delete anything by hand.

The scorecard shows whether the videos are actually getting better. The
scores are an LLM's judgement, not audience data; YouTube's retention and
click-through numbers are the real test. Feed what you learn from them back
with `/feedback`.

## Commands

```bash
story_animator review <run_id>                 # score + lessons for a finished run
story_animator improve <run_id> [--force]      # fix it if it scores below the best so far
story_animator package <run_id> [--notes ...]  # title, description, tags, thumbnail
story_animator revise <run_id> "<request>"     # apply a change, re-render only what changed
story_animator feedback "<rule>"               # an owner lesson for every future video
```
