"""Mechanism kit core: parts, links and flowing particles.

A *part* is a schematic object (tank, valve, gear, laser...) drawn from
strokes in the theme's colours, not a flat icon. Every part knows:

- its **ports** (where pipes, wires and rays attach), as fractions of its
  bounding box so they stay right after the layout scales and moves it;
- its **actions** (`on`, `off`, `open`, `level`...), each of which either
  returns animations or, with `instant=True`, jumps straight to the end
  state (used to replay earlier scenes of a continuing diagram).

A *link* joins two ports with a pipe, wire, cable, ray, fibre, belt, shaft
or arrow, and carries particles (water, current, data, light, air, heat)
whose speed is a ValueTracker, so flow can ease on and off.
"""

from __future__ import annotations

import math
from typing import Callable

import numpy as np
from manim import (
    DOWN,
    LEFT,
    ORIGIN,
    RIGHT,
    UP,
    Animation,
    Circle,
    Dot,
    Line,
    ValueTracker,
    VGroup,
    VMobject,
)

# Particle look per thing carried: (colour key or hex, radius, spacing in units)
CARRIES = {
    "water": ("#4FC3F7", 0.055, 0.32),
    "oil": ("#C9A227", 0.055, 0.34),
    "air": ("muted", 0.04, 0.3),
    "gas": ("#B39DDB", 0.045, 0.3),
    "steam": ("#ECEFF1", 0.05, 0.34),
    "heat": ("#FF7043", 0.05, 0.3),
    "current": ("#FFD54F", 0.045, 0.26),
    "data": ("accent", 0.05, 0.22),
    "light": ("#FFFFFF", 0.065, 0.2),
    "signal": ("accent2", 0.045, 0.26),
    "force": ("accent3", 0.05, 0.4),
    "fuel": ("#FF8A65", 0.05, 0.3),
    "blood": ("#E53935", 0.055, 0.3),
}


def colour(theme, key: str | None, default: str) -> str:
    named = {"accent": theme.accent, "accent2": theme.accent2, "accent3": theme.accent3,
             "ink": theme.ink, "muted": theme.muted, "bg": theme.bg, "bg2": theme.bg2}
    k = str(key or "").strip()
    if k in named:
        return named[k]
    if k.startswith("#") and len(k) == 7:
        return k
    return default


class Part:
    """A drawn component with ports, a label and named actions."""

    def __init__(self, mob: VGroup, ports: dict | None = None):
        self.mob = mob
        # name -> (fx, fy): fractions of the bounding box from its centre
        self.ports = {"in": (-0.5, 0.0), "out": (0.5, 0.0), "top": (0.0, 0.5), "bottom": (0.0, -0.5),
                      "left": (-0.5, 0.0), "right": (0.5, 0.0), "center": (0.0, 0.0)}
        self.ports.update(ports or {})
        self.actions: dict[str, Callable[..., list[Animation]]] = {}
        self.speed = ValueTracker(0.0)  # drives spinning / oscillating parts
        self.label = None  # set by the layout
        self.teeth = 0  # gears: for mesh ratios
        self.surface = None  # containers: () -> y of the liquid surface (things can float on it)

    def port(self, name: str) -> np.ndarray:
        fx, fy = self.ports.get(name, self.ports["center"])
        c = self.mob.get_center()
        return c + np.array([fx * self.mob.width, fy * self.mob.height, 0.0])

    def act(self, name: str, value=None, instant: bool = False) -> list[Animation]:
        fn = self.actions.get(name)
        if fn is None:
            return []
        out = fn(value, instant)
        return [a for a in (out or []) if a is not None]

    # -- helpers builders use -------------------------------------------------
    def spinner(self, m: VMobject, rate: float = 2.4):
        """Rotate `m` about its own centre at speed * rate rad/s."""
        def upd(mob, dt):
            s = self.speed.get_value()
            if abs(s) > 1e-4:
                mob.rotate(s * rate * dt, about_point=mob.get_center())
        m.add_updater(upd)
        return m

    def motor_actions(self, on_speed: float = 1.0):
        """Standard on/off: ease the speed tracker."""
        def on(value, instant):
            v = on_speed if value is None else float(value)
            if instant:
                self.speed.set_value(v)
                return []
            return [self.speed.animate.set_value(v)]

        def off(value, instant):
            if instant:
                self.speed.set_value(0.0)
                return []
            return [self.speed.animate.set_value(0.0)]
        self.actions.update(on=on, off=off, start=on, stop=off, run=on)


class Link:
    """A routed connection that can carry flowing particles."""

    def __init__(self, path: VMobject, body: VGroup, carries: str, theme, reverse: bool = False,
                 zigzag: bool = False):
        self.path = path
        self.body = body
        self.flow = ValueTracker(0.0)
        self.reverse = reverse
        hexcol, r, gap = CARRIES.get(carries, CARRIES["data"])
        col = colour(theme, hexcol, hexcol if hexcol.startswith("#") else theme.accent)
        length = max(_length(path), 0.2)
        n = max(3, min(60, int(length / gap)))
        self.particles = VGroup(*[Dot(radius=r, color=col).set_opacity(0) for _ in range(n)])
        track = _zigzag(path, amp=0.09, pitch=0.35) if zigzag else path
        state = {"phase": 0.0}

        def upd(group, dt):
            f = self.flow.get_value()
            state["phase"] = (state["phase"] + dt * f * 0.55 / max(length / 4, 0.5)) % 1.0
            vis = min(1.0, f * 1.4)
            for k, d in enumerate(group):
                p = (k / n + state["phase"]) % 1.0
                d.move_to(track.point_from_proportion(1 - p if self.reverse else p))
                d.set_opacity(vis)
        self.particles.add_updater(upd)

    def start(self, speed=None, instant=False) -> list[Animation]:
        v = 1.0 if speed is None else float(speed)
        if instant:
            self.flow.set_value(v)
            return []
        return [self.flow.animate.set_value(v)]

    def stop(self, instant=False) -> list[Animation]:
        if instant:
            self.flow.set_value(0.0)
            return []
        return [self.flow.animate.set_value(0.0)]


def _length(path: VMobject) -> float:
    pts = [path.point_from_proportion(t) for t in np.linspace(0, 1, 24)]
    return float(sum(np.linalg.norm(b - a) for a, b in zip(pts, pts[1:])))


def _zigzag(path: VMobject, amp: float, pitch: float) -> VMobject:
    """A path bouncing side to side along `path` (light inside a fibre)."""
    length = _length(path)
    n = max(4, int(length / pitch))
    pts = []
    for k in range(n + 1):
        t = k / n
        p = path.point_from_proportion(min(t, 0.999))
        q = path.point_from_proportion(min(t + 0.001, 1.0))
        d = q - p
        nrm = np.array([-d[1], d[0], 0.0])
        nrm = nrm / (np.linalg.norm(nrm) or 1.0)
        pts.append(p + nrm * (amp if k % 2 else -amp))
    z = VMobject()
    z.set_points_as_corners(pts)
    return z


def route(a: np.ndarray, b: np.ndarray, a_side: str, b_side: str) -> list[np.ndarray]:
    """Orthogonal elbow route between two port positions."""
    if abs(a[1] - b[1]) < 0.04 or abs(a[0] - b[0]) < 0.04:
        return [a, b]
    horizontal = a_side in ("in", "out", "left", "right") or b_side in ("in", "out", "left", "right")
    if horizontal:
        mx = (a[0] + b[0]) / 2
        return [a, np.array([mx, a[1], 0]), np.array([mx, b[1], 0]), b]
    my = (a[1] + b[1]) / 2
    return [a, np.array([a[0], my, 0]), np.array([b[0], my, 0]), b]


def polyline(points) -> VMobject:
    m = VMobject()
    m.set_points_as_corners([np.array(p, dtype=float) for p in points])
    return m


__all__ = ["Part", "Link", "CARRIES", "colour", "route", "polyline",
           "UP", "DOWN", "LEFT", "RIGHT", "ORIGIN", "Line", "Circle", "VGroup", "math", "np"]
