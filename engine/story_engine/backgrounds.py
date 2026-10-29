"""Scene backdrops.

Each builder returns a VGroup sized ~1.4x the frame so camera moves never
reveal an edge. Some attach gentle time-based updaters (twinkle, drift,
rotation) — ambient motion keeps "holds" from feeling frozen.
"""

from __future__ import annotations

import math
import random

import numpy as np
from manim import (
    DOWN,
    PI,
    TAU,
    UP,
    AnnularSector,
    Annulus,
    Circle,
    Dot,
    Ellipse,
    Line,
    Rectangle,
    VGroup,
    VMobject,
)

from .stage import Stage
from .theme import Theme, mix

OVERSCAN = 1.45


def _base(stage: Stage, theme: Theme, gradient: bool = True) -> Rectangle:
    r = Rectangle(width=stage.width * OVERSCAN, height=stage.height * OVERSCAN, stroke_width=0)
    if gradient:
        r.set_fill([theme.bg2, theme.bg, theme.bg], opacity=1)
        r.set_sheen_direction(DOWN)
    else:
        r.set_fill(theme.bg, opacity=1)
    return r


def _spotlight(stage: Stage, theme: Theme, center=None, strength: float = 1.0) -> VGroup:
    """Faux radial gradient: stacked translucent discs."""
    center = np.zeros(3) if center is None else center
    g = VGroup()
    rmax = max(stage.width, stage.height) * 0.55
    for k in range(7):
        r = rmax * (1 - k / 8)
        g.add(Circle(radius=r, stroke_width=0, fill_color=theme.bg2, fill_opacity=0.09 * strength).move_to(center))
    return g


def bg_plain(stage, theme, rng):
    return VGroup(_base(stage, theme, gradient=False), _spotlight(stage, theme))


def bg_gradient(stage, theme, rng):
    return VGroup(_base(stage, theme), _spotlight(stage, theme, strength=0.6))


def bg_stars(stage, theme, rng):
    w, h = stage.width * OVERSCAN, stage.height * OVERSCAN
    stars = VGroup()
    twinklers = []
    for i in range(140):
        x, y = rng.uniform(-w / 2, w / 2), rng.uniform(-h / 2, h / 2)
        r = rng.choice([0.012, 0.015, 0.02, 0.03]) if i % 9 else 0.045
        col = theme.ink if rng.random() > 0.2 else theme.accent3
        d = Dot([x, y, 0], radius=r, color=col, fill_opacity=rng.uniform(0.25, 0.85))
        stars.add(d)
        if i % 9 == 0:
            twinklers.append((d, rng.uniform(0, TAU), rng.uniform(1.5, 3.0)))

    clock = {"t": 0.0}

    def tick(m, dt):
        clock["t"] += dt
        for d, ph, sp in twinklers:
            d.set_fill(opacity=0.35 + 0.6 * (0.5 + 0.5 * math.sin(clock["t"] * sp + ph)))

    stars.add_updater(tick)
    return VGroup(_base(stage, theme), _spotlight(stage, theme, strength=0.5), stars)


def bg_grid(stage, theme, rng):
    w, h = stage.width * OVERSCAN, stage.height * OVERSCAN
    lines = VGroup()
    step = 0.8
    for x in np.arange(-w / 2, w / 2, step):
        lines.add(Line([x, -h / 2, 0], [x, h / 2, 0], stroke_color=theme.muted, stroke_width=1.2, stroke_opacity=0.12))
    for y in np.arange(-h / 2, h / 2, step):
        lines.add(Line([-w / 2, y, 0], [w / 2, y, 0], stroke_color=theme.muted, stroke_width=1.2, stroke_opacity=0.12))
    return VGroup(_base(stage, theme), lines, _spotlight(stage, theme, strength=0.7))


def bg_dots(stage, theme, rng):
    w, h = stage.width * OVERSCAN, stage.height * OVERSCAN
    dots = VGroup(*[Dot([x, y, 0], radius=0.025, color=theme.muted, fill_opacity=0.22)
                    for x in np.arange(-w / 2, w / 2, 0.6) for y in np.arange(-h / 2, h / 2, 0.6)])
    return VGroup(_base(stage, theme), dots, _spotlight(stage, theme, strength=0.6))


def bg_city(stage, theme, rng):
    w = stage.width * OVERSCAN
    ground = -stage.height / 2 + (2.6 if stage.portrait else 0.9)
    far = VGroup()
    near = VGroup()
    x = -w / 2
    far_col = mix(theme.bg, theme.bg2, 0.8)
    near_col = mix(theme.bg, "#000000", 0.25) if theme.dark else mix(theme.bg2, theme.ink, 0.25)
    while x < w / 2:
        bw = rng.uniform(0.6, 1.3)
        bh = rng.uniform(1.0, 3.2)
        far.add(Rectangle(width=bw, height=bh, stroke_width=0, fill_color=far_col, fill_opacity=1)
                .move_to([x + bw / 2, ground + bh / 2 + 0.3, 0]))
        x += bw * 0.85
    x = -w / 2
    windows = VGroup()
    while x < w / 2:
        bw = rng.uniform(0.8, 1.5)
        bh = rng.uniform(0.8, 2.2)
        b = Rectangle(width=bw, height=bh, stroke_width=0, fill_color=near_col, fill_opacity=1)
        b.move_to([x + bw / 2, ground + bh / 2, 0])
        near.add(b)
        for wy in np.arange(ground + 0.3, ground + bh - 0.2, 0.38):
            for wx in np.arange(x + 0.2, x + bw - 0.15, 0.32):
                if rng.random() < 0.35:
                    windows.add(Rectangle(width=0.12, height=0.16, stroke_width=0, fill_color=theme.accent,
                                          fill_opacity=rng.uniform(0.35, 0.8)).move_to([wx, wy, 0]))
        x += bw + rng.uniform(0.05, 0.25)
    floor = Rectangle(width=w, height=stage.height, stroke_width=0, fill_color=near_col, fill_opacity=1)
    floor.next_to([0, ground, 0], DOWN, buff=0)
    moon = Circle(radius=0.42, stroke_width=0, fill_color=theme.ink if theme.dark else theme.accent,
                  fill_opacity=0.8 if theme.dark else 0.35).move_to([stage.width * 0.4, stage.height * 0.36, 0])
    return VGroup(_base(stage, theme), moon, far, near, windows, floor)


def bg_hills(stage, theme, rng):
    w = stage.width * OVERSCAN
    base_y = -stage.height / 2 + (2.0 if stage.portrait else 0.6)
    sun = VGroup(Circle(radius=0.75, stroke_width=0, fill_color=theme.accent, fill_opacity=0.12),
                 Circle(radius=0.48, stroke_width=0, fill_color=theme.accent, fill_opacity=0.55)).move_to(
        [stage.width * 0.4, stage.height * 0.36, 0])
    layers = VGroup()
    cols = [mix(theme.bg2, theme.accent3, 0.25), mix(theme.bg2, theme.accent3, 0.45),
            mix(theme.bg, theme.accent3, 0.35)]
    for k, col in enumerate(cols):
        hill = VMobject(stroke_width=0, fill_color=col, fill_opacity=1)
        xs = np.linspace(-w / 2, w / 2, 40)
        ph = rng.uniform(0, TAU)
        top = [np.array([x, base_y + 1.3 - k * 0.55 + 0.35 * math.sin(x * (0.45 + 0.15 * k) + ph), 0]) for x in xs]
        pts = top + [np.array([w / 2, -stage.height, 0]), np.array([-w / 2, -stage.height, 0])]
        hill.set_points_as_corners([*pts, pts[0]])
        layers.add(hill)
    return VGroup(_base(stage, theme), sun, layers)


def bg_waves(stage, theme, rng):
    w = stage.width * OVERSCAN
    base_y = -stage.height / 2 + (2.2 if stage.portrait else 0.8)
    bands = VGroup()
    for k in range(3):
        col = mix(theme.bg2, theme.accent3, 0.15 + 0.15 * k)
        band = VMobject(stroke_width=0, fill_color=col, fill_opacity=0.85)
        band.k = k
        bands.add(band)
    clock = {"t": 0.0}

    def shape(band, t):
        k = band.k
        xs = np.linspace(-w / 2, w / 2, 48)
        top = [np.array([x, base_y + 0.9 - 0.45 * k + 0.18 * math.sin(x * 0.9 + t * (0.6 + 0.2 * k) + k), 0])
               for x in xs]
        pts = top + [np.array([w / 2, -stage.height, 0]), np.array([-w / 2, -stage.height, 0])]
        band.set_points_as_corners([*pts, pts[0]])

    for b in bands:
        shape(b, 0.0)

    def tick(m, dt):
        clock["t"] += dt
        for b in m:
            shape(b, clock["t"])

    bands.add_updater(tick)
    return VGroup(_base(stage, theme), _spotlight(stage, theme, strength=0.5), bands)


def bg_bokeh(stage, theme, rng):
    w, h = stage.width * OVERSCAN, stage.height * OVERSCAN
    blobs = VGroup()
    for i in range(22):
        r = rng.uniform(0.2, 0.9)
        c = Circle(radius=r, stroke_width=0, fill_color=theme.accents[i % 3], fill_opacity=rng.uniform(0.05, 0.14))
        c.move_to([rng.uniform(-w / 2, w / 2), rng.uniform(-h / 2, h / 2), 0])
        c.speed = rng.uniform(0.08, 0.25)
        blobs.add(c)

    def drift(m, dt):
        for c in m:
            c.shift(UP * c.speed * dt)
            if c.get_center()[1] > h / 2 + 1:
                c.shift(DOWN * (h + 2))

    blobs.add_updater(drift)
    return VGroup(_base(stage, theme), _spotlight(stage, theme, strength=0.6), blobs)


def bg_rays(stage, theme, rng):
    n = 16
    r = max(stage.width, stage.height) * OVERSCAN
    rays = VGroup(*[AnnularSector(inner_radius=0, outer_radius=r, angle=TAU / (2 * n), start_angle=k * TAU / n,
                                  fill_color=theme.bg2, fill_opacity=0.35, stroke_width=0) for k in range(n)])
    rays.move_to(stage.content_center)
    rays.add_updater(lambda m, dt: m.rotate(0.05 * dt, about_point=stage.content_center))
    return VGroup(_base(stage, theme, gradient=False), rays, _spotlight(stage, theme, stage.content_center, 0.8))


def _vignette(stage, strength: float = 1.0) -> VGroup:
    """Darken the frame edges with stacked translucent rings (soft falloff)."""
    r_out = max(stage.width, stage.height) * OVERSCAN
    g = VGroup()
    for k in range(6):
        inner = stage.width * (0.42 + 0.07 * k) if not stage.portrait else stage.height * (0.36 + 0.06 * k)
        g.add(Annulus(inner_radius=inner, outer_radius=r_out, stroke_width=0,
                      fill_color="#000000", fill_opacity=0.075 * strength))
    return g


def bg_glow(stage, theme, rng):
    """The classic diagram look: dark field, soft light in the middle, dark corners."""
    return VGroup(_base(stage, theme, gradient=False), _spotlight(stage, theme, strength=1.4),
                  _vignette(stage))


def bg_blueprint(stage, theme, rng):
    w, h = stage.width * OVERSCAN, stage.height * OVERSCAN
    step = 1.0
    lines = VGroup()
    for x in np.arange(-w / 2, w / 2, step):
        lines.add(Line([x, -h / 2, 0], [x, h / 2, 0], stroke_color=theme.accent, stroke_width=1, stroke_opacity=0.07))
    for y in np.arange(-h / 2, h / 2, step):
        lines.add(Line([-w / 2, y, 0], [w / 2, y, 0], stroke_color=theme.accent, stroke_width=1, stroke_opacity=0.07))
    return VGroup(_base(stage, theme, gradient=False), lines, _spotlight(stage, theme, strength=1.2),
                  _vignette(stage))


BACKGROUNDS = {
    "glow": bg_glow,
    "blueprint": bg_blueprint,
    "plain": bg_plain,
    "gradient": bg_gradient,
    "stars": bg_stars,
    "grid": bg_grid,
    "dots": bg_dots,
    "city": bg_city,
    "hills": bg_hills,
    "waves": bg_waves,
    "bokeh": bg_bokeh,
    "rays": bg_rays,
}

BACKGROUND_NOTES = {
    "glow": "dark field with a soft central glow and vignette — the classic diagram look",
    "blueprint": "faint drafting grid under a central glow — maths, engineering, how-it-works",
    "plain": "flat colour with a soft centre glow",
    "gradient": "vertical two-tone gradient",
    "stars": "twinkling starfield — space, night, wonder",
    "grid": "faint blueprint grid — tech, maths, planning",
    "dots": "dot matrix — data, minimal",
    "city": "night skyline — society, economy, modern life",
    "hills": "layered hills with a sun — nature, journeys, history",
    "waves": "animated waves — ocean, flow, calm",
    "bokeh": "drifting soft lights — emotion, dreamy, lifestyle",
    "rays": "slowly rotating sunburst — reveals, titles, celebrations",
}


def build_background(name: str, stage: Stage, theme: Theme, seed: int = 7) -> VGroup:
    fn = BACKGROUNDS.get(str(name).lower(), bg_gradient)
    bg = fn(stage, theme, random.Random(seed))
    bg.set_z_index(-100)
    return bg
