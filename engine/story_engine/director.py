"""The director: turns one render spec into a timed Manim scene.

Layouts don't call `self.play` directly. They register *beats* — "at time t,
play these animations for r seconds" — via `Ctx.at`. The director sorts the
beats and plays them against the scene clock, filling gaps with `wait` (during
which ambient updaters keep everything alive). Beat times come from the
narration's word timestamps, so visuals land on the words that name them.

Total clip length is guaranteed: the director always pads/clamps to
`spec["duration"]`.
"""

from __future__ import annotations

import dataclasses
import math
import random
import re
import shutil
import sys
from functools import lru_cache
from dataclasses import dataclass, field
from typing import Callable

import numpy as np
from manim import (
    DOWN,
    LEFT,
    RIGHT,
    UP,
    Animation,
    AnimationGroup,
    Create,
    Line,
    FadeIn,
    GrowFromCenter,
    MovingCameraScene,
    Rectangle,
    VGroup,
    rate_functions,
)

from .backgrounds import build_background
from .icons import build_icon
from .stage import Stage, make_text
from .theme import Theme, get_theme, set_font

CUE_LEAD = 0.12  # visuals land a hair before the word — feels "on beat"
FIRST_REVEAL_MAX = 0.9  # seconds after narration starts by which something must be on screen


@lru_cache(maxsize=1)
def latex_available() -> bool:
    return bool(shutil.which("latex") and shutil.which("dvisvgm"))


@dataclass
class Beat:
    t: float
    make: Callable[[], list[Animation]]
    run_time: float = 0.8
    order: int = 0


def _norm(token: str) -> str:
    return re.sub(r"[^a-z0-9]", "", token.lower())


@dataclass
class Ctx:
    scene: "SpecScene"
    stage: Stage
    theme: Theme
    spec: dict
    duration: float
    content_end: float
    words: list[dict]
    rng: random.Random
    beats: list[Beat] = field(default_factory=list)
    title_mob: object = None  # the scene heading, so layouts that move the camera can pin it

    # --- scene data -------------------------------------------------------
    @property
    def s(self) -> dict:
        return self.spec.get("scene", {})

    @property
    def content(self) -> dict:
        c = self.s.get("content")
        return c if isinstance(c, dict) else {}

    @property
    def narr_start(self) -> float:
        return self.words[0]["s"] if self.words else 0.3

    @property
    def narr_end(self) -> float:
        return self.words[-1]["e"] if self.words else max(self.content_end - 0.5, 0.5)

    def clock(self) -> float:
        return self.scene.renderer.time

    # --- timing -----------------------------------------------------------
    def cue(self, phrase, fallback: float | None = None, after: float = -1.0) -> float | None:
        """Time (s) the narrator starts saying `phrase`, searching after `after`."""
        if not phrase or not self.words:
            return fallback
        target = [_norm(t) for t in str(phrase).split() if _norm(t)]
        if not target:
            return fallback
        toks = [_norm(w["w"]) for w in self.words]
        n = len(target)
        for strict in (True, False):
            for i in range(len(toks) - n + 1):
                if self.words[i]["s"] < after:
                    continue
                ok = True
                for j, tt in enumerate(target):
                    w = toks[i + j]
                    if strict and w != tt:
                        ok = False
                        break
                    if not strict and not (w.startswith(tt[: max(3, len(tt) - 2)]) or tt.startswith(w) and len(w) >= 3):
                        ok = False
                        break
                if ok:
                    return max(0.0, self.words[i]["s"] - CUE_LEAD)
        return fallback

    @property
    def first_by(self) -> float:
        """Latest time the first element of a scene may appear. Cue words often come
        mid-sentence; waiting for them leaves an empty frame, so the first reveal is
        pulled forward to shortly after the narration starts."""
        return self.narr_start + FIRST_REVEAL_MAX

    def schedule(self, cues: list, start: float | None = None, end: float | None = None,
                 min_gap: float = 0.55) -> list[float]:
        """Times for a sequence of reveals. Uses cues where found, interpolates the
        rest, and guarantees increasing times inside [start, end]. The first reveal
        never waits past `first_by`."""
        n = len(cues)
        if n == 0:
            return []
        start = self.narr_start + 0.1 if start is None else start
        end = min(self.narr_end, self.content_end - 1.2) if end is None else end
        end = max(end, start + min_gap * max(n - 1, 1))
        found: list[float | None] = []
        # Search from the start of the narration, not from `start`: a cue word
        # spoken just before `start` (e.g. during an intro build) must still
        # match its first occurrence, not a repeat of the word later on. The
        # monotonic pass below pushes such early matches to `start`.
        search_from = min(start, self.narr_start) - 0.3
        after = search_from
        for c in cues:
            t = self.cue(c, None, after=after)
            if t is not None and search_from <= t <= end + 0.5:
                found.append(t)
                after = t + 0.05
            else:
                found.append(None)
        # Interpolate unknowns between known anchors. Virtual anchors at index -1
        # and n are placed so that, with no cues at all, items spread evenly
        # from `start` to `end`.
        if n == 1:
            left, right = (-1, start - 1.0), (1, start + 1.0)
        else:
            span = (end - start) * (n + 1) / (n - 1)
            left, right = (-1, start - span / (n + 1)), (n, start - span / (n + 1) + span)
        anchors = [left] + [(i, t) for i, t in enumerate(found) if t is not None] + [right]
        out = list(found)
        for (i0, t0), (i1, t1) in zip(anchors, anchors[1:]):
            for k in range(i0 + 1, i1):
                out[k] = t0 + (t1 - t0) * (k - i0) / (i1 - i0)
        # monotonic + min gap + clamp
        res: list[float] = []
        for t in out:
            t = max(float(t), start - 0.2 if not res else res[-1] + min_gap)
            res.append(t)
        limit = self.content_end - 0.6
        if res and res[-1] > limit:
            # compress uniformly into the available window
            span = max(limit - res[0], 0.2)
            first = res[0]
            scale = span / max(res[-1] - first, 1e-6)
            res = [first + (t - first) * scale for t in res]
        if res:
            res[0] = min(res[0], max(self.first_by, start - 0.2))
        return [max(0.0, t) for t in res]

    def at(self, t: float | None, make: Callable[[], list[Animation] | Animation], run_time: float = 0.8):
        if t is None:
            return
        def wrapped():
            r = make()
            return r if isinstance(r, (list, tuple)) else [r]
        self.beats.append(Beat(float(t), wrapped, run_time, len(self.beats)))

    # --- building blocks --------------------------------------------------
    def text(self, s, size=34, color=None, weight="NORMAL", max_width=None, max_chars=None, align="center"):
        return make_text(s, size=size, color=color or self.theme.ink, weight=weight,
                         max_width=max_width, max_chars=max_chars, align=align)

    def tex(self, latex: str, size=48, color=None):
        """Typeset maths with LaTeX (MathTex); plain text if LaTeX is unavailable or fails."""
        color = color or self.theme.ink
        if latex_available():
            try:
                from manim import MathTex
                return MathTex(latex, font_size=size, color=color)
            except Exception as exc:  # bad TeX from the storyboard shouldn't kill the scene
                print(f"[story_engine] MathTex failed ({exc}); using text", file=sys.stderr)
        plain = re.sub(r"\\[a-zA-Z]+", " ", latex).replace("{", "").replace("}", "").replace("^", "")
        return make_text(" ".join(plain.split()), size=size * 0.9, color=color, slant="ITALIC")

    def add(self, *mobjects):
        """Show mobjects immediately, without animation."""
        self.scene.add(*mobjects)
        return mobjects[0] if len(mobjects) == 1 else mobjects

    def icon(self, name, size=1.6, accent=0, label=""):
        return build_icon(name, self.theme, size=size, accent_index=accent, label_fallback=label)

    def float(self, m, amp: float = 0.06, speed: float = 1.4):
        phase = self.rng.uniform(0, math.tau)
        def upd(mob, dt):
            t = self.clock()
            mob.shift(UP * amp * speed * math.cos(t * speed + phase) * dt)
        m.add_updater(upd)
        return m

    def accent(self, i: int) -> str:
        return self.theme.accents[i % 3]

    def pin_to_frame(self, m):
        """Keep `m` fixed on screen while the camera moves or zooms (headings, notes).
        `m` must be laid out for the full, unzoomed frame; it is moved to the
        camera's current view straight away and follows it from then on.

        The size is set absolutely every frame (from the size it was laid out
        at), not by increments: an animation on `m` (a FadeIn) restores the
        geometry it captured when it began, and an incremental pin would never
        notice that it had been undone."""
        if getattr(m, "_pinned", False):
            return m
        m._pinned = True
        frame = self.scene.camera.frame
        w0 = self.stage.width
        off = m.get_center().copy()
        base = max(m.width, m.height)

        def upd(mob):
            s = frame.width / w0
            cur = max(mob.width, mob.height)
            if base > 1e-6 and cur > 1e-6 and abs(cur - base * s) > 1e-4 * base:
                mob.scale(base * s / cur)
            mob.move_to(frame.get_center() + off * s)
        upd(m)
        m.add_updater(upd)
        return m


class SpecScene(MovingCameraScene):
    def __init__(self, spec: dict, **kwargs):
        self.spec = spec
        super().__init__(**kwargs)

    def construct(self):
        from .layouts import LAYOUTS, layout_title

        spec = self.spec
        stage = Stage.for_spec(spec)
        theme = get_theme(spec.get("theme"))
        duration = float(spec["duration"])
        content_end = float(spec.get("content_end", duration))
        words = [wd for wd in spec.get("words", []) if wd.get("e", 0) > wd.get("s", 0) or True]
        ctx = Ctx(self, stage, theme, spec, duration, content_end, words,
                  random.Random(int(spec.get("seed", 7))))
        self.camera.background_color = theme.bg
        set_font(theme.font)

        scene = ctx.s
        self.add(build_background(scene.get("background", "gradient"), stage, theme,
                                  seed=int(spec.get("seed", 7))))
        self._camera_motion(str(scene.get("camera", "drift")), duration)

        layout = str(scene.get("layout", "hero")).lower()
        if layout not in ("title", "quote", "chapter", "fun_fact") and scene.get("title"):
            self._scene_title(ctx, str(scene["title"]))

        builder = LAYOUTS.get(layout, LAYOUTS["hero"])
        try:
            builder(ctx)
        except Exception as exc:  # never let one bad scene kill the video
            if spec.get("strict"):
                raise
            import traceback
            print(f"[story_engine] layout '{layout}' failed, using fallback: {exc}", file=sys.stderr)
            traceback.print_exc()
            for m in list(self.mobjects):
                if m is not self.camera.frame and getattr(m, "z_index", 0) > -100:
                    self.remove(m)
            ctx.beats.clear()
            fb = ctx.content.get("fallback") if isinstance(ctx.content.get("fallback"), dict) else None
            if fb and str(fb.get("layout", "")).lower() in LAYOUTS and fb.get("layout") != layout:
                scene["layout"], scene["content"] = fb["layout"], fb.get("content", {})
                try:
                    LAYOUTS[str(fb["layout"]).lower()](ctx)
                except Exception:
                    ctx.beats.clear()
                    layout_title(ctx)
            else:
                layout_title(ctx)

        self._frame_bands(ctx)
        self._run_beats(ctx)

    # --- reserved bands -----------------------------------------------------
    def _frame_bands(self, ctx: Ctx):
        """A frame with a reserved top band (the short) gets a header under the
        burned-in hook title and a footer under the captions, pinned over
        everything: a diagram the camera zooms into slides behind them instead
        of under the text."""
        st, th = ctx.stage, ctx.theme
        if not st.top_band:
            return
        w = st.width + 0.4
        bands = VGroup()

        def strip(y_top, y_bottom, opacity):
            h = max(y_top - y_bottom, 1e-3)
            return Rectangle(width=w, height=h, stroke_width=0, fill_color=th.bg,
                             fill_opacity=opacity).move_to([0, (y_top + y_bottom) / 2, 0])

        solid, steps = 1.0, 8
        bands.add(strip(st.height / 2 + 0.1, st.top, solid))
        fade = 0.25
        for k in range(steps):  # soft lower edge
            y0 = st.top - fade * k / steps
            bands.add(strip(y0, y0 - fade / steps, solid * (1 - (k + 1) / (steps + 1))))
        foot, rise = 0.9, 0.6
        for k in range(steps):  # captions sit on a darkened footer that fades in above them
            y0 = st.caption_top + 0.15 - rise * k / steps
            bands.add(strip(y0, y0 - rise / steps, foot * (k + 1) / (steps + 1)))
        bands.add(strip(st.caption_top + 0.15 - rise, -st.height / 2 - 0.1, foot))
        bands.set_z_index(40)
        ctx.pin_to_frame(bands)
        self.add(bands)

    # --- camera -----------------------------------------------------------
    def _camera_motion(self, kind: str, duration: float):
        frame = self.camera.frame
        base_w = frame.width
        base_c = frame.get_center().copy()
        kind = kind if kind in ("static", "push_in", "pull_out", "pan_left", "pan_right", "drift") else "drift"
        if kind == "static":
            return

        def upd(f, dt):
            t = self.renderer.time
            p = rate_functions.smooth(min(max(t / max(duration, 0.1), 0), 1))
            scale, dx, dy = 1.0, 0.0, 0.0
            if kind == "push_in":
                scale = 1 - 0.08 * p
            elif kind == "pull_out":
                scale = 0.92 + 0.08 * p
            elif kind == "pan_left":
                dx = 0.35 - 0.7 * p
                scale = 0.96
            elif kind == "pan_right":
                dx = -0.35 + 0.7 * p
                scale = 0.96
            elif kind == "drift":
                scale = 0.985 - 0.02 * p
                dx = 0.08 * math.sin(t * 0.35)
                dy = 0.05 * math.sin(t * 0.27 + 1.0)
            f.set_width(base_w * scale)
            f.move_to(base_c + np.array([dx, dy, 0.0]))

        frame.add_updater(upd)
        self.add(frame)

    # --- title ------------------------------------------------------------
    def _scene_title(self, ctx: Ctx, title: str):
        st = ctx.stage
        if ctx.theme.style == "diagram":
            # Serif heading with a hairline rule spanning its width.
            t = ctx.text(title, size=44 if not st.portrait else 40, weight="BOLD",
                         max_width=st.content_w, max_chars=30 if not st.portrait else 18)
            t.move_to([0, st.title_y, 0])
            if t.height > 0.8:
                t.shift(DOWN * (t.height - 0.8) / 2)
            rule = Line(t.get_corner(DOWN + LEFT), t.get_corner(DOWN + RIGHT), stroke_color=ctx.theme.accent,
                        stroke_width=2.5).shift(DOWN * 0.14)
            self._set_title(ctx, VGroup(t, rule))
            ctx.at(0.15, lambda: [FadeIn(t, shift=DOWN * 0.15), Create(rule)], 0.8)
            return
        t = ctx.text(title, size=50 if not st.portrait else 46, weight="BOLD",
                     max_width=st.content_w, max_chars=28 if not st.portrait else 18)
        t.move_to([0, st.title_y, 0])
        if t.height > 0.8:  # two-line titles grow downward, not off the top
            t.shift(DOWN * (t.height - 0.8) / 2)
        bar = Rectangle(width=0.9, height=0.07, stroke_width=0, fill_color=ctx.theme.accent, fill_opacity=1)
        bar.next_to(t, DOWN, buff=0.18)
        self._set_title(ctx, VGroup(t, bar))
        ctx.at(0.15, lambda: [FadeIn(t, shift=DOWN * 0.25), GrowFromCenter(bar)], 0.7)

    @staticmethod
    def _set_title(ctx: Ctx, head: VGroup):
        """Register the heading. A heading that wraps to two lines grows downward, so
        the content box starts that much lower; in the short the heading is pinned to
        the frame like the bands around it."""
        ctx.title_mob = head
        extra = max(0.0, (ctx.stage.title_y - 0.4) - head.get_bottom()[1] - 0.15)
        if extra > 0.01:
            ctx.stage = dataclasses.replace(ctx.stage, title_extra=extra)
        if ctx.stage.top_band:
            for m in head:  # the children: they are what gets added to the scene
                ctx.pin_to_frame(m)

    # --- scheduler --------------------------------------------------------
    def _run_beats(self, ctx: Ctx):
        fps = float(self.spec.get("fps", 30))
        frame = 1.0 / fps
        end = ctx.duration
        beats = sorted(ctx.beats, key=lambda b: (b.t, b.order))
        for i, beat in enumerate(beats):
            now = self.renderer.time
            if beat.t - now > frame:
                self.wait(beat.t - now)
                now = self.renderer.time
            next_t = beats[i + 1].t if i + 1 < len(beats) else end
            remaining = end - now - frame
            if remaining < 0.1:
                break
            rt = min(beat.run_time, remaining)
            # don't let a long animation swallow the next beat, but keep motion readable
            if next_t > now:
                rt = min(rt, max(next_t - now, 0.35))
            # An empty group (e.g. a LaggedStart over a blank heading) can't be played; skip it.
            anims = [a for a in beat.make()
                     if a is not None and not (isinstance(a, AnimationGroup) and not a.animations)]
            if anims:
                self.play(*anims, run_time=max(rt, frame * 2))
        rest = end - self.renderer.time
        if rest > frame / 2:
            self.wait(rest)
