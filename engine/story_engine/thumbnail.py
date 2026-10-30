"""YouTube thumbnail (1280x720) for a finished run: a frame of the video plus big text.

    python -m story_engine thumbnail --run storage/runs/<id> --scene 12 \
        --lines "THE WHOLE|INTERNET?" --highlight "INTERNET?" \
        --crop 0.2,0.16,0.53,0.86 --pointer 0.5,0.5

The frame comes from master.mp4 (no burned-in captions), `--at` of the way
through the scene's slot so its elements are already on screen. `--crop`
(fractions of the frame) picks the mechanism and leaves out the scene title;
it is colour-boosted and placed with feathered edges over a glow in its own
panel, right of the text (`--art right`) or under it (`--art bottom`), so the
text never covers it. `--pointer` (fractions of the cropped art) rings one
detail and points the text at it. Rules for what to put on a thumbnail are in
docs/PACKAGING.md. Writes <run>/thumbnail.png.
"""

from __future__ import annotations

import json
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageEnhance, ImageFilter, ImageFont

from .backdrops import BACKDROPS, bloom
from .theme import FONT_DIR, get_theme

W, H = 1280, 720
MARGIN = 60
FEATHER = 40
POINTER = "#FF2D55"  # attention red, the same on every theme
BEAM = "#FF7A1A"  # hot orange against the cool backdrops
OUTLINE = "#05070D"


def _frame(video: Path, t: float) -> Image.Image:
    with tempfile.TemporaryDirectory() as tmp:
        png = Path(tmp) / "frame.png"
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", f"{t:.3f}", "-i", str(video),
                        "-frames:v", "1", str(png)], check=True)
        return Image.open(png).convert("RGB")


def contact_sheet(run: Path, out: Path, cols: int = 6) -> Path:
    """One labelled frame per scene (at 0.75 of its slot) in a grid, so a reviewer can see the whole video.
    If the run has a 9:16 short, a SHORT row follows with one frame per short scene, taken from the
    finished short (hook title and captions burned in), labelled with the long video's scene number."""
    slots = json.loads((run / "timeline.json").read_text(encoding="utf-8"))["scenes"]
    video = run / "master.mp4" if (run / "master.mp4").exists() else run / "final.mp4"
    tw, th = 320, 180
    rows = (len(slots) + cols - 1) // cols
    short_slots, short_from = [], 0
    if (run / "short.mp4").exists() and (run / "short" / "timeline.json").exists():
        short_slots = json.loads((run / "short" / "timeline.json").read_text(encoding="utf-8"))["scenes"]
        short_from = json.loads((run / "short" / "short.json").read_text(encoding="utf-8")).get("from", 0)
    sw, sh, bar = 180, 320, 34
    per_row = (cols * tw) // sw
    short_rows = (len(short_slots) + per_row - 1) // per_row
    height = rows * th + (bar + short_rows * sh if short_slots else 0)
    sheet = Image.new("RGB", (cols * tw, height), "black")
    d = ImageDraw.Draw(sheet)
    font = ImageFont.truetype(str(FONT_DIR / "Poppins-Bold.ttf"), 22)
    for i, slot in enumerate(slots):
        frame = _frame(video, slot["start"] + slot["slot"] * 0.75).resize((tw, th), Image.LANCZOS)
        x, y = (i % cols) * tw, (i // cols) * th
        sheet.paste(frame, (x, y))
        d.rectangle((x, y, x + 44, y + 30), fill="#000000")
        d.text((x + 6, y + 2), str(i), font=font, fill="#FFE066")
    if short_slots:
        y0 = rows * th
        d.rectangle((0, y0, cols * tw, y0 + bar), fill="#202020")
        d.text((8, y0 + 3), "SHORT (9:16)", font=font, fill="#FFE066")
        for k, slot in enumerate(short_slots):
            frame = _frame(run / "short.mp4", slot["start"] + slot["slot"] * 0.6).resize((sw, sh), Image.LANCZOS)
            x, y = (k % per_row) * sw, y0 + bar + (k // per_row) * sh
            sheet.paste(frame, (x, y))
            d.rectangle((x, y + sh - 30, x + 44, y + sh), fill="#000000")
            d.text((x + 6, y + sh - 28), str(short_from + k), font=font, fill="#FFE066")
    out.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(out, optimize=True)
    return out


def _glow(canvas: Image.Image, center: tuple[int, int], radius: int, color: str, strength: float) -> None:
    mask = Image.new("L", canvas.size, 0)
    cx, cy = center
    ImageDraw.Draw(mask).ellipse((cx - radius, cy - radius, cx + radius, cy + radius), fill=round(255 * strength))
    mask = mask.filter(ImageFilter.GaussianBlur(radius * 0.5))
    canvas.paste(Image.new("RGB", canvas.size, color), (0, 0), mask)


def _vignette(canvas: Image.Image) -> Image.Image:
    mask = Image.new("L", canvas.size, 255)
    ImageDraw.Draw(mask).ellipse((-W * 0.15, -H * 0.25, W * 1.15, H * 1.25), fill=0)
    mask = mask.filter(ImageFilter.GaussianBlur(120)).point(lambda v: round(v * 0.75))
    return Image.composite(Image.new("RGB", canvas.size, OUTLINE), canvas, mask)


def _place(canvas: Image.Image, art: Image.Image, box: tuple[int, int, int, int],
           oval: bool) -> tuple[int, int, float]:
    """Fit `art` inside `box` (keeping its aspect), centred, with feathered edges.

    `oval` fades it out from the centre so a frame whose background differs from
    the canvas shows no edge. Returns the art's top-left corner and scale, to map
    points in the art onto the canvas.
    """
    bw, bh = box[2] - box[0], box[3] - box[1]
    scale = min(bw / art.width, bh / art.height)
    art = art.resize((round(art.width * scale), round(art.height * scale)), Image.LANCZOS)
    mask = Image.new("L", art.size, 0)
    inset = FEATHER * 2 if oval else FEATHER
    shape = ImageDraw.Draw(mask).ellipse if oval else ImageDraw.Draw(mask).rectangle
    shape((inset, inset, art.width - inset, art.height - inset), fill=255)
    mask = mask.filter(ImageFilter.GaussianBlur(inset * 0.75 if oval else FEATHER / 2))
    x, y = box[0] + (bw - art.width) // 2, box[1] + (bh - art.height) // 2
    canvas.paste(art, (x, y), mask)
    return x, y, scale


def _pointer(d: ImageDraw.ImageDraw, start: tuple[float, float], target: tuple[float, float], ring: float) -> None:
    """A ring around `target` and an arrow from `start` that stops at the ring."""
    tx, ty = target
    sx, sy = start
    dist = max(1.0, ((tx - sx) ** 2 + (ty - sy) ** 2) ** 0.5)
    ux, uy = (tx - sx) / dist, (ty - sy) / dist
    ex, ey = tx - ux * (ring + 18), ty - uy * (ring + 18)  # arrow tip, just outside the ring
    head = 46
    bx, by = ex - ux * head, ey - uy * head
    px, py = -uy * head * 0.6, ux * head * 0.6
    for color, extra in ((OUTLINE, 8), (POINTER, 0)):
        d.ellipse((tx - ring, ty - ring, tx + ring, ty + ring), outline=color, width=12 + extra)
        d.line((sx, sy, bx, by), fill=color, width=16 + extra)
        d.polygon([(ex + ux * extra, ey + uy * extra), (bx + px * (1 + extra / 40), by + py * (1 + extra / 40)),
                   (bx - px * (1 + extra / 40), by - py * (1 + extra / 40))], fill=color)


def _fit(d: ImageDraw.ImageDraw, text: str, max_w: int, size: int) -> ImageFont.FreeTypeFont:
    while True:
        font = ImageFont.truetype(str(FONT_DIR / "Poppins-ExtraBold.ttf"), size)
        if d.textlength(text, font=font) <= max_w or size <= 40:
            return font
        size -= 4


def _shade_left(img: Image.Image) -> Image.Image:
    """Darken the text side of a busy backdrop so the words stay the brightest thing there."""
    mask = Image.new("L", (W, H))
    solid, clear = int(W * 0.12), int(W * 0.55)
    for x in range(W):
        mask.paste(210 if x < solid else max(0, round(210 * (clear - x) / (clear - solid))), (x, 0, x + 1, H))
    return Image.composite(Image.new("RGB", (W, H), OUTLINE), img, mask)


def _beam(img: Image.Image, start: tuple[float, float], end: tuple[int, int]) -> Image.Image:
    """A glowing data stream from the subject into the backdrop's vanishing point, with packets on it."""
    (sx, sy), (ex, ey) = start, end
    halo = Image.new("RGB", (W, H), "black")
    ImageDraw.Draw(halo).line((sx, sy, ex, ey), fill=BEAM, width=34)
    img = ImageChops.screen(img, halo.filter(ImageFilter.GaussianBlur(16)))
    # Drawn straight on the image, not screened, so the orange stays saturated instead of blooming to white
    d = ImageDraw.Draw(img)
    d.line((sx, sy, ex, ey), fill=BEAM, width=9)
    d.line((sx, sy, ex, ey), fill="#FFC48A", width=3)
    for k in range(1, 9):
        t = k / 9
        x, y, r = sx + (ex - sx) * t, sy + (ey - sy) * t, 13 * (1 - t) + 3
        d.rounded_rectangle((x - r * 1.6, y - r, x + r * 1.6, y + r), radius=r / 2, fill="#FF9A3D",
                            outline="#FFE6C7", width=2)
    return img


def render(run: Path, scene: int, lines: list[str], highlight: str | None, at: float = 0.75,
           crop: tuple[float, float, float, float] = (0.1, 0.18, 0.9, 0.92), art: str = "right",
           pointer: tuple[float, float] | None = None, backdrop: str | None = None) -> Path:
    board = json.loads((run / "storyboard.json").read_text(encoding="utf-8"))
    slot = json.loads((run / "timeline.json").read_text(encoding="utf-8"))["scenes"][scene]
    theme = get_theme(board.get("theme"))
    video = run / "master.mp4" if (run / "master.mp4").exists() else run / "final.mp4"

    frame = _frame(video, slot["start"] + slot["slot"] * at)
    fw, fh = frame.size
    frame = frame.crop((round(crop[0] * fw), round(crop[1] * fh), round(crop[2] * fw), round(crop[3] * fh)))
    frame = ImageEnhance.Contrast(ImageEnhance.Color(frame).enhance(1.6)).enhance(1.15)

    if art == "bottom":
        art_box, text_box = (0, int(H * 0.44), W, H), (MARGIN, 70, W - MARGIN, int(H * 0.44))
    elif art == "inset":
        art_box, text_box = (10, int(H * 0.42), int(W * 0.34), H), (MARGIN, 30, int(W * 0.52), int(H * 0.46))
    else:
        art_box, text_box = (int(W * 0.40), -20, W + 20, H + 20), (MARGIN, 0, int(W * 0.46), H)
    centre = ((art_box[0] + art_box[2]) // 2, (art_box[1] + art_box[3]) // 2)
    if backdrop:
        vp = (int(W * 0.66), int(H * 0.46))
        img = _shade_left(BACKDROPS[backdrop](W, H, vp))
    else:
        img = Image.new("RGB", (W, H), theme.bg)
        _glow(img, centre, int(min(art_box[2] - art_box[0], art_box[3] - art_box[1]) * 0.55), theme.accent, 0.45)
    ax, ay, scale = _place(img, frame, art_box, oval=art != "bottom")
    if backdrop and art == "inset":  # a big subject sits in the light itself; a small one beams into it
        img = _beam(img, (ax + frame.width * scale * 0.62, ay + frame.height * scale * 0.5), vp)
    img = _vignette(img)

    d = ImageDraw.Draw(img)
    max_w = text_box[2] - text_box[0]
    fonts = [_fit(d, line, max_w, 150 if line == highlight else 104) for line in lines]
    heights = [f.getbbox("H")[3] - f.getbbox("H")[1] for f in fonts]
    gap = 28
    y = text_box[1] + (text_box[3] - text_box[1] - sum(heights) - gap * (len(lines) - 1)) // 2
    text_end = (MARGIN, y)
    for line, font, h in zip(lines, fonts, heights):
        color = theme.accent3 if line == highlight else "#FFFFFF"
        d.text((MARGIN, y + h), line, font=font, fill=color, anchor="ls", stroke_width=8, stroke_fill=OUTLINE)
        text_end = (MARGIN + d.textlength(line, font=font) + 20, y + h * 0.5)
        y += h + gap

    if pointer is not None:
        target = (ax + pointer[0] * frame.width * scale, ay + pointer[1] * frame.height * scale)
        _pointer(d, text_end, target, ring=70)

    out = run / "thumbnail.png"
    img.save(out, optimize=True)
    return out
