"""The mechanism kit: reusable schematic parts for `process` diagrams.

Parts come from two places:

- `parts.py`: hand-written built-ins (tank, valve, pump, gear, laser...);
- `library/`: parts the AI wrote when a video needed something the kit
  didn't have. Each is one file defining `PART` (name, use, actions) and
  `build(k, spec)`. It is sandbox-checked and dry-run before it is saved,
  then reused by every later video, so the kit grows with the channel.

`build_part` never raises for an unknown kind: it falls back to a labelled
`block`, so a missing part can't break a render.
"""

from __future__ import annotations

import sys
from pathlib import Path

from .core import Link, Part, colour, polyline, route
from .parts import ALIASES, BUILTIN, DOCS

LIBRARY_DIR = Path(__file__).resolve().parent / "library"


class KitEnv:
    """What a part builder can use: theme colours, text, icons and the Part type."""

    def __init__(self, ctx):
        self.ctx = ctx
        self.theme = ctx.theme
        self.Part = Part

    def text(self, s, size=28, color=None, weight="NORMAL"):
        return self.ctx.text(s, size=size, color=color, weight=weight)

    def icon(self, name, size=1.0):
        return self.ctx.icon(name, size=size)

    def color(self, key, default=None):
        return colour(self.theme, key, default or self.theme.ink)


def normalize_kind(kind: str) -> str:
    k = "".join(ch if ch.isalnum() else "_" for ch in str(kind or "").strip().lower()).strip("_")
    while "__" in k:
        k = k.replace("__", "_")
    return k[:32]


_library_cache: dict | None = None


def library() -> dict:
    """name -> (build, PART meta) for every valid file in library/."""
    global _library_cache
    if _library_cache is not None:
        return _library_cache
    out = {}
    if LIBRARY_DIR.is_dir():
        for f in sorted(LIBRARY_DIR.glob("*.py")):
            if f.name.startswith("_"):
                continue
            try:
                build, meta = load_part_file(f)
                out[normalize_kind(meta.get("name") or f.stem)] = (build, meta)
            except Exception as exc:  # a broken library file must not break rendering
                print(f"[story_engine] skipping kit part {f.name}: {exc}", file=sys.stderr)
    _library_cache = out
    return out


def load_part_file(path: Path):
    from ..custom import load_namespace

    ns = load_namespace(path.read_text(encoding="utf-8"), extra=_builder_names())
    meta = ns.get("PART") if isinstance(ns.get("PART"), dict) else {"name": path.stem}
    return ns["build"], meta


def _builder_names() -> dict:
    import numpy as np

    from . import core

    return {"Part": Part, "colour": colour, "P": lambda x, y: np.array([x, y, 0.0]), "core": core}


def resolve(kind: str):
    """(builder, canonical name) or (None, name) if unknown."""
    k = normalize_kind(kind)
    if k in BUILTIN:
        return BUILTIN[k], k
    lib = library()
    if k in lib:
        return lib[k][0], k
    if k in ALIASES:
        return BUILTIN[ALIASES[k]], ALIASES[k]
    if k.endswith("s") and k[:-1] in BUILTIN:
        return BUILTIN[k[:-1]], k[:-1]
    return None, k


def build_part(env: KitEnv, spec: dict) -> Part:
    builder, name = resolve(spec.get("kind", "block"))
    if builder is not None:
        try:
            part = builder(env, spec)
            if isinstance(part, Part):
                return part
        except Exception as exc:
            if env.ctx.spec.get("strict"):
                raise
            print(f"[story_engine] part '{name}' failed ({exc}); using a block", file=sys.stderr)
    return BUILTIN["block"](env, {**spec, "text": spec.get("label") or name.replace("_", " ")})


def catalog_docs() -> dict:
    docs = dict(DOCS)
    for name, (_build, meta) in library().items():
        use = str(meta.get("use") or name)
        acts = meta.get("actions") or []
        docs[name] = use + (f"; actions {', '.join(acts)}" if acts and "action" not in use.lower() else "")
    return docs


PART_API = r'''
Write ONE Python file that adds a reusable schematic PART to an animation kit
(Manim Community, Cairo). It will be reused by future videos, so make it generic:
parameterise it through `spec`, never hard-code a topic or a label.

The file must define:
  PART = {"name": "<snake_case_name>", "use": "<one line: what it is, when to use it>",
          "actions": ["on", "off", ...]}
  def build(k, spec):  -> returns a Part

Available names: everything from `manim` (VGroup, Circle, Polygon, Line, Arc, Rectangle,
RoundedRectangle, Dot, ValueTracker, always_redraw, Rotate, ...), `np`, `math`, plus:
  Part(mob, ports=None)   the part object (see below)
  P(x, y)                 np.array([x, y, 0])
  colour(theme, key, default)
No other imports, no file/OS access.

k (the kit environment):
  k.theme.ink / .muted / .accent / .accent2 / .accent3 / .bg / .bg2    hex colours (use ONLY these,
                          plus natural colours for materials: water #4FC3F7, fire #FF7043, ...)
  k.text("label", size=28, color=None, weight="NORMAL"|"BOLD")
  k.icon("name", size)    a small flat icon (use sparingly; the point is a schematic drawing)

spec: the part's storyboard entry (label, size, color, and any custom fields you document in "use").

Part(mob, ports={...}):
  mob     a VGroup drawn around the origin, about 2 units tall/wide. The layout scales and moves it.
  ports   name -> (fx, fy) fractions of the bounding box from its centre; defaults exist for
          in/left (-0.5, 0), out/right (0.5, 0), top (0, 0.5), bottom (0, -0.5), center.
  part.speed              ValueTracker 0..1 for motion. part.motor_actions() adds on/off that ease it.
  part.spinner(mobject, rate)   rotates that sub-mobject about its own centre at speed*rate rad/s
  part.actions[name] = fn(value, instant) -> list of Animations
          instant=True must set the end state directly and return [] (used to replay history).
          Typical actions: on, off, open, close, level, highlight, extend, retract.

RULES
  1. Schematic engineering style: ink strokes (stroke_width 3-5) for structure, the accent for the
     moving/active element, translucent fills (fill_opacity 0.2-0.6). Not clip-art.
  2. Anything animated with updaters or always_redraw MUST compute positions relative to a sub-mobject
     of the part (e.g. frame.get_center(), frame.height) - never absolute coordinates - because the
     layout scales and moves the part after build().
  3. Motion through part.speed (so on/off ease), or a ValueTracker you animate in an action.
  4. Deterministic. No text longer than 3 words inside the part (the layout adds the label).
  5. Sandbox: no imports, no dunder attributes (write `state["x"] = v` inside a def, not
     `state.__setitem__`), no global/nonlocal, no getattr/setattr/eval/open.
  6. Return ONLY a ```python fenced block.

EXAMPLE
```python
PART = {"name": "conveyor", "use": "belt conveyor moving items along; actions on, off", "actions": ["on", "off"]}

def build(k, spec):
    th = k.theme
    frame = Rectangle(width=2.4, height=0.9, stroke_width=0, fill_opacity=0)
    belt = RoundedRectangle(corner_radius=0.3, width=2.4, height=0.6, stroke_color=th.ink, stroke_width=4)
    rollers = VGroup(*[Circle(radius=0.22, stroke_color=th.ink, stroke_width=3).move_to(P(x, 0))
                       for x in (-0.95, 0.95)])
    phase = {"x": 0.0}
    part = Part(VGroup(frame, belt, rollers), ports={"in": (-0.5, 0), "out": (0.5, 0)})

    def items():
        c, s = frame.get_center(), frame.width / 2.4
        g = VGroup()
        for i in range(4):
            x = ((phase["x"] + i / 4) % 1.0) * 2.0 - 1.0
            g.add(Square(side_length=0.28 * s, stroke_color=th.accent, stroke_width=3, fill_color=th.accent,
                         fill_opacity=0.4).move_to(c + np.array([x * s, 0.45 * s, 0])))
        return g
    boxes = always_redraw(items)
    part.mob.add(boxes)

    def tick(m, dt):  # plain functions for state updates; dunder calls like __setitem__ are rejected
        phase["x"] = (phase["x"] + dt * 0.3 * part.speed.get_value()) % 1.0
    clock = VMobject()
    clock.add_updater(tick)
    part.mob.add(clock)
    for r in rollers:
        part.spinner(r, -4.0)
    part.motor_actions()
    return part
```
'''

__all__ = ["KitEnv", "Part", "Link", "build_part", "resolve", "normalize_kind", "catalog_docs", "library",
           "load_part_file", "route", "polyline", "colour", "PART_API", "LIBRARY_DIR"]
