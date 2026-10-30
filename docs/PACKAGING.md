# Packaging: thumbnails, titles, descriptions and tags

A video is only watched if its thumbnail and title make someone click. These
are the rules for every video on the channel. The pipeline writes a first
draft (`youtube.txt`); a person, or an AI assistant, finishes it against
this page before upload.

## The one rule: open a question, don't answer it

The thumbnail and the title work as a pair. One raises a question, the other
gives just enough to make it urgent, and only the video answers it. The
thumbnail never restates the title's claim or gives away the answer. Naming
the same subject is fine ("INTERNET?" with "The Internet Is Hiding…");
repeating the claim is not ("NO PUMPS?" with "…Without Pumps").

| | Thumbnail | Title |
|---|---|---|
| Bad (all answer, no question) | "99% OF THE INTERNET IS UNDERWATER" | How Undersea Cables Carry 99% of Global Data |
| Good (question + where, not how) | "THE WHOLE INTERNET?" with an arrow to a tiny glowing core | The Internet Is Hiding on the Ocean Floor |
| Bad | "300 BILLION REQUESTS" | How Computer Servers Work |
| Still bad (a question, but flat: navy on navy, an empty box, nothing glows) | "WHAT'S IN HERE?" ringed on an empty card next to "Personal PC VS" | The Computer You Use Every Day but Never See |
| Good | "NO SCREEN. NO SLEEP." over a glowing server-rack corridor, an orange data stream running from a phone into the light | The Computer You Use Every Day but Never See |

Every hook must be true and paid off in the video. A click the video
doesn't deliver on costs more watch time than it brings.

## Thumbnails

0. **Stop the scroll first.** A question is not enough if the image is flat.
   People scroll past a page of dark boxes. The thumbnail needs **depth** (a
   place you could step into), **light** (one bright focal point with glow),
   and **colour contrast** (neon cyan against hot orange, warm text on
   near-black). Our animation frames are flat diagrams, so for most videos
   put the frame's subject in front of a drawn **backdrop** (`--backdrop`)
   with a beam from the subject into its light. Squint at it: if nothing
   glows and nothing pulls the eye in, it's not done.
1. **One subject, big.** The mechanism fills at least 40% of the frame. Crop
   out the scene title, long labels, and anything that isn't the subject.
2. **2–4 words, ending on a question or an unfinished claim.** Never the
   answer, never the title again.
3. **Point at the mystery.** A red ring and arrow on the one detail that
   raises the question (`--pointer`).
4. **Colour that grabs:** dark background, one bright glow behind the
   subject, a saturated frame (the module boosts it), warm text (yellow and
   white) against the cool scene, and red only for the pointer. At most two
   text colours.
5. **Text never covers the subject.** Text left, subject right
   (`--art right`), or text on top, subject below (`--art bottom`) for wide
   diagrams. Text is outlined thick and dark so it reads on anything.
6. **Readable at phone size.** Shrink it to 320×180; if the words or the
   subject are not clear, cut words or crop tighter.
7. **Use a finished frame.** Not one where elements are still fading in
   (`--at` 0.75 of the scene is the default), and never one showing empty
   placeholder boxes.
8. **Or catch the moment before the reveal.** The one exception to rule 7:
   when a scene reveals the answer (a `compare` card, a `stat`, a hidden
   part), a frame just before it appears can be a hook. Ring the empty spot.
   Step through `--at` until the answer is not readable yet. Rule 0 still
   applies: an empty box on a dark page is flat, so the frame needs depth
   and light around it.

Older runs have no `master.mp4`. The module then uses `final.mp4`, whose
captions are burned in at the bottom, so keep `--crop`'s `y1` at 0.85 or less.

### Making one

Pick the frame first: look at the run's scenes (`storyboard.json` +
`timeline.json`) and grab candidates from `master.mp4`, which has no
captions. Then, on the VPS, from `/root/story_animator/engine`:

```bash
../venv/bin/python -m story_engine thumbnail --run ../storage/runs/<id> --scene 12 \
    --lines "THE WHOLE|INTERNET?" --highlight "INTERNET?" \
    --crop 0.15,0.16,0.55,0.87 --pointer 0.505,0.5

# With a backdrop: the video's phone icon in the corner, beaming into a server-rack corridor
../venv/bin/python -m story_engine thumbnail --run ../storage/runs/<id> --scene 0 \
    --lines "NO SCREEN.|NO SLEEP." --highlight "NO SLEEP." \
    --crop 0.36,0.15,0.64,0.65 --art inset --backdrop datacenter
```

| Option | What it does |
|---|---|
| `--scene` | Scene whose frame is used |
| `--at` | How far through the scene to grab it (0–1, default 0.75) |
| `--lines` | Text lines, separated by `\|` |
| `--highlight` | The line drawn biggest, in the theme's warm accent |
| `--crop` | `x0,y0,x1,y1` of the frame as fractions: the subject only |
| `--art` | `right` (default), `bottom` (wide diagrams), or `inset` (small subject bottom-left, for backdrops) |
| `--pointer` | `x,y` inside the cropped frame (fractions) to ring and point at |
| `--backdrop` | Drawn scene behind the subject, with a glowing beam from the subject into its vanishing point. Now: `datacenter`. Add more in `engine/story_engine/backdrops.py` (an undersea trench, a canal lock, a sky…) |

It writes `storage/runs/<id>/thumbnail.png` (1280×720). Look at it before
using it.

## Titles

- **Under 50 characters** where possible (60 at most), so phones don't cut it off.
- **A surprising claim or a hidden thing**, with concrete nouns: "The
  Internet Is Hiding on the Ocean Floor", "Fighter Jets Are Designed to Fall
  Out of the Sky", "How Ships Climb a Mountain Without a Single Pump".
- **The subject's search words early** ("Internet", "Fighter Jets", "Ships").
- **No** plain "How X Works", colons, jargon, ALL CAPS, or a claim the video
  doesn't back up.

## Descriptions

1. **First two lines** (shown above "…more"): restate the hook in plain
   words, with the main keyword.
2. **What you'll see:** the 3–4 mechanisms the video shows, as one short paragraph.
3. **Chapters** from `youtube.txt`: first at `0:00`, at least three, each at
   least 10 seconds. Copy them from the final narrated run, since timings
   change between a silent preview and the narrated render. One-minute
   videos have no chapters; leave this part out.
4. **Channel line:** "New to Blueprinted? We explain how the world's
   machines and megastructures actually work, with animated diagrams.
   Subscribe for the next one."
5. **Three hashtags** at the end: `#engineering`, the topic, `#howitworks`.

Only use numbers the video states and that you can check. Leave out the
rest.

## Tags

- 10–20 tags, under 500 characters in total.
- In order: the exact topic phrase, its synonyms, the specific terms the
  video explains, broad terms ("engineering explained", "how it works",
  "animated explainer"), then `blueprinted`.
- No unrelated trending names or misleading terms.
- Add them to `youtube.txt` as a `Tags:` line.

## Checklist before upload

- [ ] Thumbnail stops the scroll: depth, one glowing focal point, cyan/orange contrast
- [ ] Thumbnail asks a question the title doesn't answer, and the video does
- [ ] Thumbnail: one big subject, 2–4 words, readable at 320×180, no placeholder boxes
- [ ] Title under 50–60 characters, true, search words early
- [ ] Description: hook, what you'll see, chapters from the narrated run, channel line, 3 hashtags
- [ ] 10–20 tags, under 500 characters
- [ ] Video is the narrated render, not a `--silent` preview
