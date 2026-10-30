"""YouTube channel art in the blueprint theme, so the channel looks like its videos.

    python -m story_engine branding --out storage/branding

Writes banner_2560x1440.png (text inside YouTube's 1546x423 safe area; the
schematics either side show only on desktop and TV), profile_800x800.png and
profile_circle_preview.png (how YouTube crops the profile picture; not for upload).
"""

from __future__ import annotations

import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

from .theme import FONT_DIR, THEMES

CHANNEL_NAME = "BLUEPRINTED"
KICKER = "ANIMATED ENGINEERING EXPLAINERS"
TAGLINE = "How the world's machines, megastructures and natural wonders actually work"

T = THEMES["blueprint"]
GRID_MINOR, GRID_MAJOR = "#14203E", "#1E3056"
S = 2  # supersampling factor; drawn at 2x, downscaled for smooth lines


def _font(name: str, size: int) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(str(FONT_DIR / name), size * S)


def _glow_bg(w: int, h: int, cx: int, cy: int, radius: int) -> Image.Image:
    img = Image.new("RGB", (w, h), T.bg)
    g = Image.new("L", (w, h), 0)
    ImageDraw.Draw(g).ellipse((cx - radius, cy - radius * 0.6, cx + radius, cy + radius * 0.6), fill=255)
    g = g.filter(ImageFilter.GaussianBlur(radius * 0.45))
    return Image.composite(Image.new("RGB", (w, h), T.bg2), img, g)


def _grid(d: ImageDraw.ImageDraw, w: int, h: int, step: int) -> None:
    for i, x in enumerate(range(0, w, step)):
        d.line((x, 0, x, h), fill=GRID_MAJOR if i % 5 == 0 else GRID_MINOR, width=S)
    for i, y in enumerate(range(0, h, step)):
        d.line((0, y, w, y), fill=GRID_MAJOR if i % 5 == 0 else GRID_MINOR, width=S)


def _gear(d, cx, cy, r, teeth, color, width, phase=0.0, hub=True) -> None:
    pts = []
    depth = r * 0.16
    for i in range(teeth * 4):
        a = phase + 2 * math.pi * i / (teeth * 4)
        rr = r if i % 4 in (1, 2) else r - depth
        pts.append((cx + rr * math.cos(a), cy + rr * math.sin(a)))
    d.line(pts + pts[:2], fill=color, width=width, joint="curve")
    if hub:
        hr = r * 0.32
        d.ellipse((cx - hr, cy - hr, cx + hr, cy + hr), outline=color, width=width)
        d.ellipse((cx - 6 * S, cy - 6 * S, cx + 6 * S, cy + 6 * S), fill=color)
        for k in range(teeth // 3):
            a = phase + 2 * math.pi * k / (teeth // 3)
            d.line((cx + hr * math.cos(a), cy + hr * math.sin(a),
                    cx + (r - depth) * 0.82 * math.cos(a), cy + (r - depth) * 0.82 * math.sin(a)),
                   fill=color, width=width)


def _bridge(d, x0, x1, deck_y, tower_h, color, width) -> None:
    span = x1 - x0
    t1, t2 = x0 + span * 0.2, x0 + span * 0.8
    top = deck_y - tower_h
    d.line((x0, deck_y, x1, deck_y), fill=color, width=width * 2)
    for tx in (t1, t2):
        d.line((tx, top, tx, deck_y + tower_h * 0.35), fill=color, width=width * 2)
        d.line((tx - 14 * S, top + 20 * S, tx + 14 * S, top + 20 * S), fill=color, width=width)

    def cable_y(x):  # main span parabola, side spans straight
        if x < t1:
            return top + (deck_y - 8 * S - top) * (t1 - x) / (t1 - x0)
        if x > t2:
            return top + (deck_y - 8 * S - top) * (x - t2) / (x1 - t2)
        u = (x - (t1 + t2) / 2) / ((t2 - t1) / 2)
        return deck_y - 14 * S - (deck_y - 14 * S - top) * u * u

    d.line([(x, cable_y(x)) for x in range(int(x0), int(x1) + 1, 4)], fill=color, width=width, joint="curve")
    for x in range(int(x0 + 24 * S), int(x1), 26 * S):
        if abs(x - t1) > 8 * S and abs(x - t2) > 8 * S:
            d.line((x, cable_y(x), x, deck_y), fill=color, width=max(1, width // 2))
    water_y = deck_y + tower_h * 0.35
    for x in range(int(x0 - 30 * S), int(x1 + 30 * S), 34 * S):
        d.arc((x, water_y - 6 * S, x + 34 * S, water_y + 6 * S), 180, 360, fill=T.muted, width=width)


def _turbine(d, cx, hub_y, ground_y, blade, color, width, angle) -> None:
    d.polygon([(cx - 9 * S, ground_y), (cx + 9 * S, ground_y), (cx + 4 * S, hub_y), (cx - 4 * S, hub_y)],
              outline=color, width=width)
    for k in range(3):
        a = angle + 2 * math.pi * k / 3
        tip = (cx + blade * math.cos(a), hub_y + blade * math.sin(a))
        side = (cx + blade * 0.25 * math.cos(a + 0.18), hub_y + blade * 0.25 * math.sin(a + 0.18))
        d.line([(cx, hub_y), side, tip, (cx, hub_y)], fill=color, width=width, joint="curve")
    d.ellipse((cx - 10 * S, hub_y - 10 * S, cx + 10 * S, hub_y + 10 * S), fill=T.bg, outline=T.accent3, width=width)


def _dimension(d, x0, x1, y, label, font) -> None:
    d.line((x0, y, x1, y), fill=T.muted, width=S)
    for x, sgn in ((x0, 1), (x1, -1)):
        d.line((x, y - 10 * S, x, y + 10 * S), fill=T.muted, width=S)
        d.polygon([(x, y), (x + sgn * 14 * S, y - 5 * S), (x + sgn * 14 * S, y + 5 * S)], fill=T.muted)
    tw = d.textlength(label, font=font)
    mid = (x0 + x1) / 2
    d.rectangle((mid - tw / 2 - 10 * S, y - 14 * S, mid + tw / 2 + 10 * S, y + 14 * S), fill=T.bg)
    d.text((mid, y), label, font=font, fill=T.muted, anchor="mm")


def _spaced(d, xy, text, font, fill, tracking) -> None:
    widths = [d.textlength(c, font=font) for c in text]
    x = xy[0] - (sum(widths) + tracking * (len(text) - 1)) / 2
    for c, cw in zip(text, widths):
        d.text((x, xy[1]), c, font=font, fill=fill, anchor="ls")
        x += cw + tracking


def banner(out: Path) -> Path:
    W, H = 2560 * S, 1440 * S
    img = _glow_bg(W, H, W // 2, H // 2, 1100 * S)
    d = ImageDraw.Draw(img)
    _grid(d, W, H, 40 * S)
    label = _font("Poppins-SemiBold.ttf", 15)

    # Left: suspension bridge (outside the mobile safe area)
    _bridge(d, 90 * S, 470 * S, 800 * S, 210 * S, T.accent, 2 * S)
    _dimension(d, 90 * S, 470 * S, 905 * S, "SPAN 1,991 m", _font("Poppins-Regular.ttf", 15))
    d.text((90 * S, 560 * S), "FIG. 01  —  SUSPENSION", font=label, fill=T.muted)

    # Right: meshing gears and a wind turbine
    _gear(d, 2165 * S, 770 * S, 110 * S, 18, T.accent, 3 * S)
    _gear(d, 2298 * S, 684 * S, 60 * S, 10, T.accent2, 3 * S, phase=0.16)
    _turbine(d, 2462 * S, 660 * S, 905 * S, 92 * S, T.ink, 2 * S, angle=-math.pi / 2)
    d.text((2090 * S, 560 * S), "FIG. 02  —  DRIVETRAIN", font=label, fill=T.muted)

    # Centre text, inside the 1546x423 safe area (x 507-2053, y 508-931)
    cx = W // 2
    d.text((cx, 600 * S), KICKER, font=_font("Poppins-SemiBold.ttf", 24), fill=T.accent, anchor="mm")
    _spaced(d, (cx, 742 * S), CHANNEL_NAME, _font("Poppins-ExtraBold.ttf", 150), T.ink, 14 * S)
    d.line((cx - 120 * S, 780 * S, cx + 120 * S, 780 * S), fill=T.accent, width=4 * S)
    d.ellipse((cx + 128 * S, 774 * S, cx + 140 * S, 786 * S), fill=T.accent2)
    d.text((cx, 838 * S), TAGLINE, font=_font("LibreBaskerville-Italic[wght].ttf", 34), fill=T.ink, anchor="mm")

    path = out / "banner_2560x1440.png"
    img.resize((2560, 1440), Image.LANCZOS).save(path, optimize=True)
    return path


def profile(out: Path) -> list[Path]:
    W = 800 * S
    img = _glow_bg(W, W, W // 2, W // 2, 420 * S)
    d = ImageDraw.Draw(img)
    _grid(d, W, W, 50 * S)
    c = W // 2
    _gear(d, c, c, 300 * S, 16, T.accent, 14 * S, phase=0.1, hub=False)
    d.ellipse((c - 205 * S, c - 205 * S, c + 205 * S, c + 205 * S), fill=T.bg, outline=T.accent, width=6 * S)
    d.text((c - 6 * S, c + 12 * S), CHANNEL_NAME[0], font=_font("Poppins-ExtraBold.ttf", 300), fill=T.ink,
           anchor="mm")
    d.ellipse((c + 92 * S, c + 92 * S, c + 132 * S, c + 132 * S), fill=T.accent2)
    square = img.resize((800, 800), Image.LANCZOS)
    path = out / "profile_800x800.png"
    square.save(path, optimize=True)

    mask = Image.new("L", (800, 800), 0)
    ImageDraw.Draw(mask).ellipse((0, 0, 799, 799), fill=255)
    preview = square.convert("RGBA")
    preview.putalpha(mask)
    preview_path = out / "profile_circle_preview.png"
    preview.save(preview_path)
    return [path, preview_path]


def render_all(out: Path) -> list[Path]:
    out.mkdir(parents=True, exist_ok=True)
    return [banner(out), *profile(out)]
