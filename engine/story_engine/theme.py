"""Colour themes and typography.

Themes are curated palettes rather than free-form LLM colours: every theme has
been picked for contrast between `bg`, `ink` and the accents, so any layout
rendered with any theme stays legible.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

FONT_FAMILY = "Poppins"
SERIF_FAMILY = "Libre Baskerville"
FALLBACK_FONT = "Arial"
FONT_DIR = Path(__file__).resolve().parents[2] / "assets" / "fonts"
_FONT_FILES = {FONT_FAMILY: "Poppins-*.ttf", SERIF_FAMILY: "LibreBaskerville*.ttf"}

_current = {"family": None}


def set_font(family: str | None) -> None:
    """Select the family used by make_text for the current scene (per theme)."""
    _current["family"] = family


def ensure_font() -> str:
    """The family to use for Text. Fonts are wired up by fonts.configure_fonts()."""
    family = _current["family"] or FONT_FAMILY
    pattern = _FONT_FILES.get(family)
    if pattern is None or any(FONT_DIR.glob(pattern)):
        return family
    return FALLBACK_FONT


@dataclass(frozen=True)
class Theme:
    name: str
    description: str
    bg: str
    bg2: str
    ink: str
    muted: str
    accent: str
    accent2: str
    accent3: str
    dark: bool = True
    font: str = FONT_FAMILY
    # "diagram": 3Blue1Brown-style (serif, thin rules, centre glow);
    # "infographic": flat cards, icons and bold sans-serif.
    style: str = "infographic"

    @property
    def accents(self) -> list[str]:
        return [self.accent, self.accent2, self.accent3]

    def to_dict(self) -> dict:
        return {
            "name": self.name,
            "description": self.description,
            "bg": self.bg,
            "bg2": self.bg2,
            "ink": self.ink,
            "muted": self.muted,
            "accent": self.accent,
            "accent2": self.accent2,
            "accent3": self.accent3,
            "dark": self.dark,
            "font": self.font,
            "style": self.style,
        }


THEMES: dict[str, Theme] = {
    t.name: t
    for t in [
        Theme("blueprint", "navy with sky-blue lines and rose highlights, serif — the classic "
              "diagram look: maths, science, tech, how things work",
              "#0B1026", "#16213F", "#E2E8F0", "#64748B", "#38BDF8", "#F43F5E", "#FBBF24",
              font=SERIF_FAMILY, style="diagram"),
        Theme("graphite", "charcoal chalkboard with green, blue and yellow, serif — geometry, "
              "physics, vectors, step-by-step derivations",
              "#1B1B1D", "#28282C", "#ECECEC", "#6E6E73", "#83C167", "#58C4DD", "#F4D35E",
              font=SERIF_FAMILY, style="diagram"),
        Theme("midnight", "deep navy night, warm gold highlights — science, space, big ideas",
              "#0B1026", "#1C2B4F", "#F4F1DE", "#8D99AE", "#FFB703", "#FB8500", "#4CC9F0"),
        Theme("sunset", "plum dusk with coral and peach — stories, emotion, history",
              "#2B1B3D", "#5B2A55", "#FFF4E6", "#C9A9C2", "#FF7B54", "#FFB26B", "#FFD56F"),
        Theme("ocean", "abyssal teal with aqua glow — nature, water, calm explainers",
              "#03212F", "#0A4D68", "#E8F6F3", "#7FB3C2", "#00D7C0", "#3FA7D6", "#FFD166"),
        Theme("forest", "dark pine with lime and amber — ecology, growth, health",
              "#0F2417", "#1F4529", "#F1F7E9", "#9CB59A", "#A7D129", "#F4A259", "#E9C46A"),
        Theme("ember", "charred red-black with flame orange — danger, energy, drama",
              "#1A0F0F", "#3D1C1C", "#FDF0E7", "#B79B91", "#FF5A36", "#FFB400", "#F7E1D7"),
        Theme("neon", "synthwave violet with hot pink and cyan — tech, future, gaming",
              "#0D0221", "#261447", "#F6F7FF", "#9A8FBF", "#FF2A6D", "#05D9E8", "#FFE66D"),
        Theme("paper", "warm cream paper with terracotta ink — light, friendly, editorial",
              "#F4F1DE", "#E6DFC2", "#1D1E2C", "#6B705C", "#E07A5F", "#3D405B", "#81B29A",
              dark=False),
        Theme("candy", "soft pink-white with bubblegum and violet — kids, playful, lifestyle",
              "#FFF5F7", "#FFE0EA", "#2D1E2F", "#8E7C93", "#FF5D8F", "#6C63FF", "#FFB830",
              dark=False),
        Theme("mono", "charcoal with a single yellow accent — serious, finance, news",
              "#121212", "#262626", "#F5F5F5", "#8A8A8A", "#FFD000", "#E5E5E5", "#5E9CFF"),
    ]
}

DEFAULT_THEME = "blueprint"


def get_theme(spec_theme: dict | str | None) -> Theme:
    """Accept a theme name or a full palette dict (as written by the orchestrator)."""
    if isinstance(spec_theme, str):
        return THEMES.get(spec_theme, THEMES[DEFAULT_THEME])
    if isinstance(spec_theme, dict):
        base = THEMES.get(str(spec_theme.get("name", "")), THEMES[DEFAULT_THEME])
        merged = base.to_dict()
        for k, v in spec_theme.items():
            if k in merged and isinstance(v, type(merged[k])):
                merged[k] = v
        return Theme(**merged)
    return THEMES[DEFAULT_THEME]


def contrast_ink(theme: Theme, fill: str) -> str:
    """Pick bg or ink, whichever reads better on top of `fill`."""
    return theme.bg if luminance(fill) > 0.55 else ("#FFFFFF" if theme.dark else theme.ink)


def luminance(hex_color: str) -> float:
    h = hex_color.lstrip("#")
    if len(h) != 6:
        return 0.0
    r, g, b = (int(h[i : i + 2], 16) / 255 for i in (0, 2, 4))
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def mix(a: str, b: str, t: float) -> str:
    """Linear blend of two hex colours (t=0 -> a, t=1 -> b)."""
    ha, hb = a.lstrip("#"), b.lstrip("#")
    ca = [int(ha[i : i + 2], 16) for i in (0, 2, 4)]
    cb = [int(hb[i : i + 2], 16) for i in (0, 2, 4)]
    return "#" + "".join(f"{round(x + (y - x) * t):02X}" for x, y in zip(ca, cb))
