"""Frame geometry and small drawing helpers shared by every layout.

Coordinates are Manim units. Landscape frames are 14.22 x 8; portrait frames
are 8 x 14.22, so element sizes stay comparable across aspect ratios and
layouts only have to decide *arrangement*, not scale.
"""

from __future__ import annotations

import textwrap
from dataclasses import dataclass

import numpy as np
from manim import (
    DOWN,
    LEFT,
    ORIGIN,
    RIGHT,
    UP,
    CapStyleType,
    Line,
    RoundedRectangle,
    Text,
    VGroup,
    VMobject,
)

from .theme import Theme, ensure_font


@dataclass(frozen=True)
class Stage:
    width: float
    height: float
    portrait: bool

    @classmethod
    def for_pixels(cls, px_w: int, px_h: int) -> "Stage":
        portrait = px_h > px_w
        long_side = 8.0 * 16 / 9
        if portrait:
            return cls(width=8.0, height=long_side, portrait=True)
        return cls(width=long_side, height=8.0, portrait=False)

    # --- reserved regions -------------------------------------------------
    @property
    def title_y(self) -> float:
        return self.height / 2 - (1.25 if self.portrait else 0.8)

    @property
    def caption_top(self) -> float:
        """Nothing may be drawn below this y: burned-in captions live there."""
        return -self.height / 2 + (4.0 if self.portrait else 1.35)

    @property
    def content_top(self) -> float:
        """Nothing may be drawn above this y when the scene has a title."""
        return self.title_y - 0.6

    @property
    def content_center(self) -> np.ndarray:
        top = self.title_y - 0.6
        return np.array([0.0, (top + self.caption_top) / 2, 0.0])

    @property
    def content_w(self) -> float:
        return self.width - (1.0 if self.portrait else 1.8)

    @property
    def content_h(self) -> float:
        return (self.title_y - 0.6) - self.caption_top - 0.3


# ---------------------------------------------------------------------------
# Text
# ---------------------------------------------------------------------------

def wrap(text: str, max_chars: int) -> str:
    text = " ".join(str(text).split())
    return "\n".join(textwrap.wrap(text, max_chars)) or text


def make_text(
    text: str,
    size: float = 36,
    color: str = "#FFFFFF",
    weight: str = "NORMAL",
    max_width: float | None = None,
    max_chars: int | None = None,
    align: str = "center",
    font: str | None = None,
    slant: str = "NORMAL",
) -> VMobject:
    """Text in the bundled font, optionally wrapped and shrunk to fit a width.

    Pango lays out the whole block (correct baselines and line spacing) but
    left-aligns it; for centred text each line's glyphs are then slid so the
    line is centred.
    """
    body = wrap(text, max_chars) if max_chars else " ".join(str(text).split())
    body = body or " "
    t = Text(body, font=font or ensure_font(), font_size=size, color=color, weight=weight,
             slant=slant, line_spacing=0.85)
    lines = body.split("\n")
    if len(lines) > 1 and align == "center":
        glyphs = list(t.submobjects)
        k = 0
        for line in lines:
            n = len("".join(line.split()))
            if n:
                VGroup(*glyphs[k : k + n]).set_x(t.get_x())
            k += n
    if max_width is not None and t.width > max_width:
        t.scale_to_fit_width(max_width)
    return t


# ---------------------------------------------------------------------------
# Shapes
# ---------------------------------------------------------------------------

def stroke_line(a, b, color: str, width: float = 8) -> Line:
    """A line with round caps — reads as 'drawn', not 'technical'."""
    line = Line(np.array([*a, 0])[:3], np.array([*b, 0])[:3], color=color, stroke_width=width)
    line.set_cap_style(CapStyleType.ROUND)
    return line


def round_caps(m: VMobject) -> VMobject:
    for sub in m.family_members_with_points():
        sub.set_cap_style(CapStyleType.ROUND)
    return m


def card(width: float, height: float, theme: Theme, fill: str | None = None,
         opacity: float = 0.9, radius: float = 0.28) -> VGroup:
    """Rounded panel with a soft drop shadow."""
    fill = fill or theme.bg2
    shadow = RoundedRectangle(corner_radius=radius, width=width, height=height,
                              stroke_width=0, fill_color="#000000",
                              fill_opacity=0.28 if theme.dark else 0.10)
    shadow.shift(DOWN * 0.08 + RIGHT * 0.05)
    panel = RoundedRectangle(corner_radius=radius, width=width, height=height,
                             stroke_color=theme.muted, stroke_width=1.2,
                             stroke_opacity=0.35, fill_color=fill, fill_opacity=opacity)
    return VGroup(shadow, panel)


def glow(m: VMobject, color: str, layers: int = 3, spread: float = 0.12) -> VGroup:
    """Faux bloom: a few enlarged, translucent copies behind a shape."""
    rings = VGroup()
    for i in range(layers, 0, -1):
        c = m.copy()
        c.set_fill(color, opacity=0.06)
        c.set_stroke(color, width=6 * i, opacity=0.08)
        c.scale(1 + spread * i)
        rings.add(c)
    return rings


def arrange_grid(items: list, stage: Stage, max_per_row: int, buff: float) -> VGroup:
    """Arrange items in centred rows; used when portrait frames need to wrap."""
    rows = VGroup()
    for i in range(0, len(items), max_per_row):
        rows.add(VGroup(*items[i : i + max_per_row]).arrange(RIGHT, buff=buff))
    rows.arrange(DOWN, buff=buff * 0.8)
    return rows


def fit(m, max_w: float, max_h: float):
    if m.width > max_w:
        m.scale_to_fit_width(max_w)
    if m.height > max_h:
        m.scale_to_fit_height(max_h)
    return m


__all__ = [
    "Stage", "wrap", "make_text", "stroke_line", "round_caps", "card", "glow",
    "arrange_grid", "fit", "ORIGIN", "UP", "DOWN", "LEFT", "RIGHT",
]


def fill(m, max_w: float, max_h: float, frac: float = 0.9, cap: float = 1.35):
    """Fit, and also *grow* small compositions (up to `cap`x) to use the stage."""
    s = min(max_w * frac / max(m.width, 1e-6), max_h * frac / max(m.height, 1e-6), cap)
    if s > 1.02 or s < 1.0:
        m.scale(s)
    return m
