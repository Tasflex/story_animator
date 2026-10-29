"""Built-in parts. Each builder is `build(k, spec) -> Part`.

`k` is the KitEnv (theme colours, text, Part, helpers); `spec` is the part's
entry from the storyboard (`label`, `color`, `size`, plus part-specific
fields such as `level` or `teeth`). Parts are drawn around the origin at
roughly 2 units tall; the process layout scales and places them.

Style: schematic, like an engineering drawing brought to life: ink strokes
for structure, the accent for the moving or active element, translucent
fills, round caps. No flat clip-art.
"""

from __future__ import annotations

import math

import numpy as np
from manim import (
    DOWN,
    LEFT,
    PI,
    RIGHT,
    TAU,
    UP,
    Arc,
    Circle,
    Dot,
    Line,
    Polygon,
    Rectangle,
    RoundedRectangle,
    Triangle,
    ValueTracker,
    VGroup,
    VMobject,
    always_redraw,
)

from .core import Part, colour

SW = 4  # default stroke width


def P(x, y):
    return np.array([x, y, 0.0])


def _hl(k, part: Part, target=None):
    """Highlight action: flash the outline in the accent."""
    from manim import Indicate

    def act(value, instant):
        if instant:
            return []
        return [Indicate(target or part.mob, color=k.theme.accent, scale_factor=1.06)]
    part.actions["highlight"] = act


# ---------------------------------------------------------------------------
# fluids
# ---------------------------------------------------------------------------

def tank(k, spec):
    """Open-top container; `level` 0-1 animates. Wide tanks read as chambers/reservoirs."""
    th = k.theme
    w = 2.2 * float(spec.get("width", 1.0))
    h = 1.8
    liquid_col = colour(th, spec.get("liquid"), "#4FC3F7")
    walls = VMobject(stroke_color=th.ink, stroke_width=SW + 1)
    walls.set_points_as_corners([P(-w / 2, h / 2), P(-w / 2, -h / 2), P(w / 2, -h / 2), P(w / 2, h / 2)])
    level = ValueTracker(min(max(float(spec.get("level", 0.6)), 0.0), 1.0))
    t0 = {"t": 0.0}

    def water():
        # Drawn relative to the walls, so it follows the part when the layout scales/moves it.
        x0, x1 = walls.get_left()[0], walls.get_right()[0]
        y0, y1 = walls.get_bottom()[1], walls.get_top()[1]
        s = (x1 - x0) / w
        lv = level.get_value()
        pad = 0.04 * s
        top = y0 + pad + (y1 - y0 - 2 * pad) * lv
        pts = [np.array([x0 + pad, y0 + pad, 0])]
        for x in np.linspace(x0 + pad, x1 - pad, 16):
            wave = 0.03 * s * math.sin(4 * (x - x0) / s + t0["t"]) if lv > 0.02 else 0.0
            pts.append(np.array([x, top + wave if lv > 0.02 else y0 + pad, 0]))
        pts.append(np.array([x1 - pad, y0 + pad, 0]))
        return Polygon(*pts, stroke_width=0, fill_color=liquid_col, fill_opacity=0.55)

    body = always_redraw(water)
    clock = VMobject()
    clock.add_updater(lambda m, dt: t0.__setitem__("t", t0["t"] + dt * 2.0))
    group = VGroup(body, walls, clock)
    part = Part(group, ports={"in": (-0.5, -0.32), "out": (0.5, -0.32), "top": (0.0, 0.5),
                              "bottom": (0.0, -0.5), "left": (-0.5, -0.32), "right": (0.5, -0.32)})

    def surface():
        y0, y1 = walls.get_bottom()[1], walls.get_top()[1]
        pad = 0.04 * (walls.get_right()[0] - walls.get_left()[0]) / w
        return y0 + pad + (y1 - y0 - 2 * pad) * level.get_value()
    part.surface = surface

    def set_level(value, instant):
        v = min(max(float(value if value is not None else 1.0), 0.0), 1.0)
        if instant:
            level.set_value(v)
            return []
        return [level.animate.set_value(v)]
    part.actions.update(level=set_level, fill=lambda v, i: set_level(1.0 if v is None else v, i),
                        drain=lambda v, i: set_level(0.0 if v is None else v, i))
    _hl(k, part, walls)
    return part


def valve(k, spec):
    """Bow-tie valve with a handwheel; open/close turns the wheel and recolours it."""
    th = k.theme
    left = Polygon(P(-0.7, 0.45), P(0, 0), P(-0.7, -0.45), stroke_color=th.ink, stroke_width=SW)
    right = Polygon(P(0.7, 0.45), P(0, 0), P(0.7, -0.45), stroke_color=th.ink, stroke_width=SW)
    is_open = bool(spec.get("open", False))
    fill = th.accent if is_open else th.accent2
    for m in (left, right):
        m.set_fill(fill, opacity=0.55)
    stem = Line(P(0, 0), P(0, 0.75), stroke_color=th.ink, stroke_width=SW)
    wheel = VGroup(Circle(radius=0.28, stroke_color=th.ink, stroke_width=SW).move_to(P(0, 0.9)),
                   Line(P(-0.28, 0.9), P(0.28, 0.9), stroke_color=th.ink, stroke_width=3),
                   Line(P(0, 0.62), P(0, 1.18), stroke_color=th.ink, stroke_width=3))
    group = VGroup(left, right, stem, wheel)
    part = Part(group, ports={"in": (-0.5, -0.18), "out": (0.5, -0.18), "left": (-0.5, -0.18),
                              "right": (0.5, -0.18), "bottom": (0.0, -0.5)})

    def setter(opened):
        def act(value, instant):
            col = th.accent if opened else th.accent2
            if instant:
                left.set_fill(col, opacity=0.55)
                right.set_fill(col, opacity=0.55)
                return []
            from manim import Rotate
            return [left.animate.set_fill(col, opacity=0.55), right.animate.set_fill(col, opacity=0.55),
                    Rotate(wheel, angle=(-1 if opened else 1) * PI, about_point=wheel[0].get_center())]
        return act
    part.actions.update(open=setter(True), close=setter(False))
    _hl(k, part)
    return part


def pump(k, spec):
    """Centrifugal pump: round casing with a spinning impeller."""
    th = k.theme
    casing = Circle(radius=0.8, stroke_color=th.ink, stroke_width=SW, fill_color=th.bg2, fill_opacity=0.6)
    outlet = Rectangle(width=0.5, height=0.36, stroke_color=th.ink, stroke_width=SW).move_to(P(0.55, 0.72))
    blades = VGroup(*[Arc(radius=0.5, start_angle=a, angle=PI / 3, stroke_color=th.accent, stroke_width=SW)
                      .shift(P(0.25 * math.cos(a), 0.25 * math.sin(a)) * 0) for a in np.arange(0, TAU, TAU / 6)])
    imp = VGroup(blades, Dot(radius=0.09, color=th.accent))
    group = VGroup(outlet, casing, imp)
    part = Part(group, ports={"in": (-0.5, -0.1), "out": (0.36, 0.5), "top": (0.36, 0.5)})
    part.spinner(imp, 3.5)
    part.motor_actions()
    _hl(k, part, casing)
    return part


def turbine(k, spec):
    """Axial turbine / fan seen face-on: a rotor of blades in a ring."""
    th = k.theme
    n = int(spec.get("blades", 8))
    ring = Circle(radius=0.9, stroke_color=th.ink, stroke_width=SW)
    blades = VGroup(*[Polygon(P(0.08, 0.05), P(0.8, 0.18), P(0.8, -0.04), P(0.08, -0.05),
                              stroke_color=th.accent, stroke_width=2, fill_color=th.accent, fill_opacity=0.35)
                      .rotate(a, about_point=P(0, 0)) for a in np.arange(0, TAU, TAU / n)])
    rotor = VGroup(blades, Circle(radius=0.14, stroke_color=th.accent, stroke_width=3, fill_color=th.bg,
                                  fill_opacity=1))
    group = VGroup(ring, rotor)
    part = Part(group)
    part.spinner(rotor, 4.0)
    part.motor_actions()
    _hl(k, part, ring)
    return part


def propeller(k, spec):
    """Two/three-bladed propeller or wind rotor."""
    th = k.theme
    n = int(spec.get("blades", 3))
    blades = VGroup(*[Polygon(P(0, 0.1), P(0.15, 0.25), P(0.12, 0.95), P(-0.06, 0.95), P(-0.08, 0.2),
                              stroke_color=th.ink, stroke_width=2.5, fill_color=th.accent, fill_opacity=0.4)
                      .rotate(a, about_point=P(0, 0)) for a in np.arange(0, TAU, TAU / n)])
    rotor = VGroup(blades, Circle(radius=0.12, stroke_color=th.ink, stroke_width=3, fill_color=th.bg2,
                                  fill_opacity=1))
    part = Part(VGroup(rotor))
    part.spinner(rotor, 3.0)
    part.motor_actions()
    _hl(k, part)
    return part


# ---------------------------------------------------------------------------
# mechanisms
# ---------------------------------------------------------------------------

def gear(k, spec):
    """Toothed gear. `teeth` sets size ratio; meshed gears turn opposite ways."""
    th = k.theme
    teeth = int(min(max(float(spec.get("teeth", 12)), 6), 40))
    r = 0.55 + 0.035 * teeth
    pts = []
    for i in range(teeth):
        a0 = i * TAU / teeth
        for da, rr in ((0.0, r), (0.12, r), (0.2, r + 0.18), (0.36, r + 0.18), (0.44, r), (0.56, r)):
            a = a0 + da * TAU / teeth / 0.56 * 0.56
            pts.append(P(rr * math.cos(a), rr * math.sin(a)))
    body = Polygon(*pts, stroke_color=th.ink, stroke_width=3,
                   fill_color=colour(th, spec.get("color"), th.accent), fill_opacity=0.3)
    hub = VGroup(Circle(radius=0.16, stroke_color=th.ink, stroke_width=3, fill_color=th.bg, fill_opacity=1),
                 Line(P(0, 0), P(r * 0.75, 0), stroke_color=th.ink, stroke_width=3))
    g = VGroup(body, hub)
    part = Part(VGroup(g))
    part.teeth = teeth
    sign = -1 if str(spec.get("dir", "cw")).lower() in ("cw", "clockwise") else 1
    part.spinner(g, 2.0 * sign * 12 / teeth)
    part.motor_actions()
    _hl(k, part, body)
    return part


def piston(k, spec):
    """Cylinder with a reciprocating piston and rod."""
    th = k.theme
    cyl = VMobject(stroke_color=th.ink, stroke_width=SW + 1)
    cyl.set_points_as_corners([P(-0.55, -0.9), P(-0.55, 0.9), P(0.55, 0.9), P(0.55, -0.9)])
    phase = {"a": 0.0}
    head = Rectangle(width=1.0, height=0.3, stroke_color=th.ink, stroke_width=3, fill_color=th.accent,
                     fill_opacity=0.6)
    rod = Line(P(0, 0), P(0, -0.8), stroke_color=th.ink, stroke_width=SW)
    gas = Rectangle(width=1.0, height=0.5, stroke_width=0, fill_color="#FF8A65", fill_opacity=0.25)
    moving = VGroup(gas, head, rod)

    def place(m, dt):
        phase["a"] += dt * part.speed.get_value() * 5.0
        c, s = cyl.get_center(), cyl.width / 1.1
        y = c[1] + s * (0.25 + 0.3 * math.sin(phase["a"]))
        top = c[1] + 0.9 * s
        head.move_to([c[0], y, 0])
        rod.put_start_and_end_on(np.array([c[0], y - 0.15 * s, 0]), np.array([c[0], y - 1.15 * s, 0]))
        gas.stretch_to_fit_height(max(top - y - 0.15 * s, 0.02)).move_to([c[0], (top + y + 0.15 * s) / 2, 0])
    moving.add_updater(place)
    part = Part(VGroup(cyl, moving), ports={"top": (0.0, 0.5), "bottom": (0.0, -0.5)})
    part.motor_actions()
    _hl(k, part, cyl)
    return part


def wheel(k, spec):
    """Spoked wheel / drum / pulley."""
    th = k.theme
    rim = Circle(radius=0.9, stroke_color=th.ink, stroke_width=SW + 1)
    spokes = VGroup(*[Line(P(0, 0), P(0.85 * math.cos(a), 0.85 * math.sin(a)), stroke_color=th.accent,
                           stroke_width=3) for a in np.arange(0, TAU, TAU / 8)])
    w = VGroup(spokes, Circle(radius=0.12, stroke_color=th.ink, stroke_width=3, fill_color=th.bg2,
                              fill_opacity=1))
    part = Part(VGroup(rim, w))
    part.spinner(w, 2.5)
    part.motor_actions()
    _hl(k, part, rim)
    return part


def spring(k, spec):
    """Coil spring that oscillates when on (suspension, vibration, energy storage)."""
    th = k.theme
    phase = {"a": 0.0}
    frame = Rectangle(width=0.9, height=2.2, stroke_width=0, fill_opacity=0)

    def coil():
        c, sc = frame.get_center(), frame.height / 2.2
        s = 1.0 + 0.2 * math.sin(phase["a"])
        bottom = c[1] - 0.9 * sc
        pts = [np.array([c[0], bottom, 0])]
        for i in range(1, 12):
            pts.append(np.array([c[0] + (0.35 if i % 2 else -0.35) * sc, bottom + i * 1.8 * sc * s / 12, 0]))
        pts.append(np.array([c[0], bottom + 1.8 * sc * s, 0]))
        m = VMobject(stroke_color=th.accent, stroke_width=SW)
        m.set_points_as_corners(pts)
        return m
    c = always_redraw(coil)
    tick = VMobject()
    part = Part(VGroup(frame, c, tick), ports={"top": (0.0, 0.5), "bottom": (0.0, -0.5)})
    tick.add_updater(lambda m, dt: phase.__setitem__("a", phase["a"] + dt * part.speed.get_value() * 6))
    part.motor_actions()
    _hl(k, part)
    return part


# ---------------------------------------------------------------------------
# electrical
# ---------------------------------------------------------------------------

def _round_box(k, w, h, letter=None, fill=None):
    th = k.theme
    box = RoundedRectangle(corner_radius=0.18, width=w, height=h, stroke_color=th.ink, stroke_width=SW,
                           fill_color=fill or th.bg2, fill_opacity=0.7)
    grp = VGroup(box)
    if letter:
        grp.add(k.text(letter, size=40, weight="BOLD", color=th.ink).move_to(box))
    return grp, box


def motor(k, spec):
    """Electric motor: body, end cap and a turning shaft marker."""
    th = k.theme
    body, box = _round_box(k, 1.5, 1.1)
    cap = Circle(radius=0.42, stroke_color=th.ink, stroke_width=3, fill_color=th.bg, fill_opacity=1).move_to(P(0.15, 0))
    marker = VGroup(Line(P(0.15, 0), P(0.5, 0), stroke_color=th.accent, stroke_width=SW),
                    Dot(radius=0.06, color=th.accent).move_to(P(0.15, 0)))
    shaft = Line(P(0.75, 0), P(1.05, 0), stroke_color=th.ink, stroke_width=SW + 2)
    part = Part(VGroup(body, shaft, cap, marker), ports={"out": (0.5, 0.0), "right": (0.5, 0.0)})
    part.spinner(marker, 4.0)
    part.motor_actions()
    _hl(k, part, box)
    return part


def generator(k, spec):
    """Generator: circle with spinning coil; makes current."""
    th = k.theme
    casing = Circle(radius=0.85, stroke_color=th.ink, stroke_width=SW, fill_color=th.bg2, fill_opacity=0.6)
    coil = VGroup(*[Arc(radius=0.5, start_angle=a, angle=PI / 2.4, stroke_color=th.accent, stroke_width=SW)
                    for a in (0, PI)])
    coil.add(Line(P(-0.5, 0), P(0.5, 0), stroke_color="#FFD54F", stroke_width=3))
    lab = k.text("G", size=30, weight="BOLD", color=th.muted).move_to(P(0, -0.55))
    part = Part(VGroup(casing, coil, lab))
    part.spinner(coil, 3.0)
    part.motor_actions()
    _hl(k, part, casing)
    return part


def battery(k, spec):
    """Cell: long and short plates, + and - terminals."""
    th = k.theme
    plates = VGroup(Line(P(-0.15, -0.7), P(-0.15, 0.7), stroke_color=th.ink, stroke_width=SW + 1),
                    Line(P(0.15, -0.38), P(0.15, 0.38), stroke_color=th.ink, stroke_width=SW + 4))
    leads = VGroup(Line(P(-0.9, 0), P(-0.15, 0), stroke_color=th.ink, stroke_width=3),
                   Line(P(0.15, 0), P(0.9, 0), stroke_color=th.ink, stroke_width=3))
    signs = VGroup(k.text("+", size=36, color=th.accent).move_to(P(-0.45, 0.45)),
                   k.text("−", size=36, color=th.accent2).move_to(P(0.45, 0.45)))
    part = Part(VGroup(leads, plates, signs))
    _hl(k, part)
    return part


def bulb(k, spec):
    """Light bulb / lamp / load; glows when on."""
    th = k.theme
    glass = Circle(radius=0.6, stroke_color=th.ink, stroke_width=SW).move_to(P(0, 0.25))
    base = Rectangle(width=0.5, height=0.4, stroke_color=th.ink, stroke_width=3, fill_color=th.muted,
                     fill_opacity=0.6).move_to(P(0, -0.55))
    fil = VMobject(stroke_color=th.muted, stroke_width=3)
    fil.set_points_as_corners([P(-0.2, -0.3), P(-0.2, 0.25), P(-0.07, 0.4), P(0.07, 0.25), P(0.2, 0.4),
                               P(0.2, -0.3)])
    halo = VGroup(*[Circle(radius=0.6 + 0.18 * i, stroke_width=0, fill_color="#FFE082", fill_opacity=0.0)
                    .move_to(P(0, 0.25)) for i in range(3)])
    part = Part(VGroup(halo, glass, fil, base), ports={"in": (-0.5, -0.3), "out": (0.5, -0.3),
                                                       "bottom": (0.0, -0.5)})

    def light(on):
        def act(value, instant):
            ops = [0.18, 0.1, 0.05] if on else [0, 0, 0]
            col = "#FFD54F" if on else th.muted
            if instant:
                for h, o in zip(halo, ops):
                    h.set_fill(opacity=o)
                glass.set_fill("#FFF3C4", opacity=0.35 if on else 0)
                fil.set_stroke(col)
                return []
            return [*(h.animate.set_fill(opacity=o) for h, o in zip(halo, ops)),
                    glass.animate.set_fill("#FFF3C4", opacity=0.35 if on else 0), fil.animate.set_stroke(col)]
        return act
    part.actions.update(on=light(True), off=light(False))
    _hl(k, part, glass)
    return part


def switch(k, spec):
    """Knife switch; close completes the circuit."""
    th = k.theme
    a, b = Dot(P(-0.7, 0), radius=0.09, color=th.ink), Dot(P(0.7, 0), radius=0.09, color=th.ink)
    blade = Line(P(-0.7, 0), P(0.5, 0.6), stroke_color=th.accent, stroke_width=SW + 1)
    if spec.get("closed"):
        blade.put_start_and_end_on(P(-0.7, 0), P(0.7, 0))
    part = Part(VGroup(Line(P(-1, 0), P(-0.7, 0), stroke_color=th.ink, stroke_width=3),
                       Line(P(0.7, 0), P(1, 0), stroke_color=th.ink, stroke_width=3), a, b, blade),
                ports={"in": (-0.5, -0.2), "out": (0.5, -0.2)})

    def setter(closed):
        def act(value, instant):
            s, e = a.get_center(), b.get_center()
            if not closed:  # blade raised ~30 degrees
                v = (e - s) * 0.9
                ang = 0.5
                e = s + np.array([v[0] * math.cos(ang) - v[1] * math.sin(ang),
                                  v[0] * math.sin(ang) + v[1] * math.cos(ang), 0.0])
            if instant:
                blade.put_start_and_end_on(s, e)
                return []
            return [blade.animate.put_start_and_end_on(s, e)]
        return act
    part.actions.update(close=setter(True), open=setter(False), on=setter(True), off=setter(False))
    _hl(k, part)
    return part


def heater(k, spec):
    """Resistor / heating element; glows orange when on."""
    th = k.theme
    zz = VMobject(stroke_color=th.ink, stroke_width=SW)
    zz.set_points_as_corners([P(-1, 0), P(-0.6, 0)] + [P(-0.6 + 0.15 * i, 0.3 if i % 2 else -0.3)
                                                         for i in range(1, 8)] + [P(0.6, 0), P(1, 0)])
    part = Part(VGroup(zz), ports={"in": (-0.5, 0.0), "out": (0.5, 0.0)})

    def setter(on):
        def act(value, instant):
            col = "#FF7043" if on else th.ink
            if instant:
                zz.set_stroke(col, width=SW + (3 if on else 0))
                return []
            return [zz.animate.set_stroke(col, width=SW + (3 if on else 0))]
        return act
    part.actions.update(on=setter(True), off=setter(False))
    _hl(k, part)
    return part


def chip(k, spec):
    """Integrated circuit with pins; traces pulse when on."""
    th = k.theme
    body = RoundedRectangle(corner_radius=0.1, width=1.4, height=1.4, stroke_color=th.ink, stroke_width=SW,
                            fill_color=th.bg2, fill_opacity=0.85)
    pins = VGroup(*[Line(P(x, 0.7), P(x, 0.95), stroke_color=th.ink, stroke_width=3) for x in (-0.45, -0.15, 0.15, 0.45)],
                  *[Line(P(x, -0.7), P(x, -0.95), stroke_color=th.ink, stroke_width=3) for x in (-0.45, -0.15, 0.15, 0.45)],
                  *[Line(P(0.7, y), P(0.95, y), stroke_color=th.ink, stroke_width=3) for y in (-0.45, -0.15, 0.15, 0.45)],
                  *[Line(P(-0.7, y), P(-0.95, y), stroke_color=th.ink, stroke_width=3) for y in (-0.45, -0.15, 0.15, 0.45)])
    core = Rectangle(width=0.6, height=0.6, stroke_color=th.accent, stroke_width=3, fill_color=th.accent,
                     fill_opacity=0.15)
    phase = {"a": 0.0}
    core.add_updater(lambda m, dt: (phase.__setitem__("a", phase["a"] + dt * part.speed.get_value() * 6),
                                    m.set_fill(opacity=0.15 + 0.35 * part.speed.get_value() * (0.5 + 0.5 * math.sin(phase["a"])))))
    part = Part(VGroup(pins, body, core))
    part.motor_actions()
    _hl(k, part, body)
    return part


# ---------------------------------------------------------------------------
# light, waves, signals
# ---------------------------------------------------------------------------

def laser(k, spec):
    """Emitter / light source; pair with a `ray` or `fibre` link carrying light."""
    th = k.theme
    body, box = _round_box(k, 1.6, 0.8)
    aperture = Rectangle(width=0.12, height=0.34, stroke_width=0, fill_color=th.accent, fill_opacity=0.9).move_to(P(0.8, 0))
    glow = Circle(radius=0.22, stroke_width=0, fill_color=th.accent, fill_opacity=0).move_to(P(0.86, 0))
    part = Part(VGroup(body, aperture, glow), ports={"out": (0.5, 0.0), "right": (0.5, 0.0)})

    def setter(on):
        def act(value, instant):
            if instant:
                glow.set_fill(opacity=0.5 if on else 0)
                return []
            return [glow.animate.set_fill(opacity=0.5 if on else 0)]
        return act
    part.actions.update(on=setter(True), off=setter(False))
    _hl(k, part, box)
    return part


def detector(k, spec):
    """Receiver / sensor / photodiode; blinks when it detects something (on)."""
    th = k.theme
    body, box = _round_box(k, 1.3, 0.9)
    eye = Arc(radius=0.3, start_angle=PI / 2, angle=PI, stroke_color=th.accent, stroke_width=SW).move_to(P(-0.5, 0))
    led = Dot(P(0.35, 0.2), radius=0.09, color=th.muted)
    part = Part(VGroup(body, eye, led), ports={"in": (-0.5, 0.0), "left": (-0.5, 0.0)})
    phase = {"a": 0.0}
    led.add_updater(lambda m, dt: (phase.__setitem__("a", phase["a"] + dt * 8),
                                   m.set_color(th.accent if part.speed.get_value() > 0.5 and math.sin(phase["a"]) > 0
                                               else th.muted)))
    part.motor_actions()
    _hl(k, part, box)
    return part


def mirror(k, spec):
    """Flat mirror with hatched back; `angle` in degrees."""
    th = k.theme
    face = Line(P(0, -0.9), P(0, 0.9), stroke_color=th.ink, stroke_width=SW + 2)
    hatch = VGroup(*[Line(P(0, y), P(0.22, y - 0.16), stroke_color=th.muted, stroke_width=2) for y in np.linspace(-0.8, 0.9, 9)])
    m = VGroup(face, hatch).rotate(math.radians(float(spec.get("angle", 0))))
    part = Part(m, ports={"in": (-0.5, 0.0), "out": (-0.5, 0.0), "left": (-0.5, 0.0), "right": (-0.5, 0.0)})
    _hl(k, part)
    return part


def lens(k, spec):
    """Converging lens."""
    th = k.theme
    a1 = Arc(radius=1.6, start_angle=-0.6, angle=1.2, stroke_color=th.accent, stroke_width=SW).shift(LEFT * 1.3)
    a2 = Arc(radius=1.6, start_angle=PI - 0.6, angle=1.2, stroke_color=th.accent, stroke_width=SW).shift(RIGHT * 1.3)
    part = Part(VGroup(a1, a2))
    _hl(k, part)
    return part


def antenna(k, spec):
    """Mast that radiates waves when on (radio, cell tower, satellite dish feed)."""
    th = k.theme
    mast = VGroup(Line(P(0, -1), P(0, 0.6), stroke_color=th.ink, stroke_width=SW),
                  Line(P(-0.4, -1), P(0, 0.2), stroke_color=th.ink, stroke_width=3),
                  Line(P(0.4, -1), P(0, 0.2), stroke_color=th.ink, stroke_width=3),
                  Dot(P(0, 0.65), radius=0.09, color=th.accent))
    phase = {"a": 0.0}

    def waves():
        g = VGroup()
        s = part.speed.get_value()
        sc = mast[0].get_length() / 1.6
        for i in range(3):
            f = (phase["a"] + i / 3) % 1.0
            r = (0.25 + f * 0.8) * sc
            for side in (0, PI):
                g.add(Arc(radius=r, start_angle=side - 0.6, angle=1.2, stroke_color=th.accent, stroke_width=3,
                          stroke_opacity=max(s * (1 - f), 0)).move_arc_center_to(mast[3].get_center()))
        return g
    wv = always_redraw(waves)
    tick = VMobject()
    part = Part(VGroup(mast, wv, tick), ports={"bottom": (0.0, -0.5), "top": (0.0, 0.3)})
    tick.add_updater(lambda m, dt: phase.__setitem__("a", (phase["a"] + dt * 0.8) % 1.0))
    part.motor_actions()
    _hl(k, part, mast)
    return part


def flame(k, spec):
    """Heat source / burner / combustion; flickers, grows when on."""
    phase = {"a": 0.0}
    frame = Rectangle(width=1.0, height=1.8, stroke_width=0, fill_opacity=0)

    def fl():
        c, sc = frame.get_center(), frame.height / 1.8
        s = 0.55 + 0.45 * part.speed.get_value()
        j = 0.06 * math.sin(phase["a"] * 7) * s

        def Q(x, y):
            return np.array([c[0] + x * sc, c[1] + (y - 0.1) * sc, 0])
        outer = Polygon(Q(-0.45 * s, -0.7), Q(-0.5 * s, -0.1), Q(-0.15 * s + j, 0.3 * s), Q(j, 0.95 * s),
                        Q(0.2 * s + j, 0.35 * s), Q(0.5 * s, -0.1), Q(0.45 * s, -0.7),
                        stroke_width=0, fill_color="#FF7043", fill_opacity=0.8)
        inner = Polygon(Q(-0.22 * s, -0.7), Q(-0.25 * s, -0.25), Q(-j, 0.35 * s), Q(0.25 * s, -0.25),
                        Q(0.22 * s, -0.7), stroke_width=0, fill_color="#FFD54F", fill_opacity=0.9)
        return VGroup(outer, inner)
    f = always_redraw(fl)
    tick = VMobject()
    part = Part(VGroup(frame, f, tick), ports={"top": (0.0, 0.5), "bottom": (0.0, -0.5)})
    tick.add_updater(lambda m, dt: phase.__setitem__("a", phase["a"] + dt))
    part.motor_actions()
    return part


# ---------------------------------------------------------------------------
# vessels
# ---------------------------------------------------------------------------

def ship(k, spec):
    """Side view of a ship (cargo/container/tanker). With "floats_on": <tank id> it sits on that
    tank's water and rises and falls with its level."""
    th = k.theme
    hull = Polygon(P(-1.25, 0.05), P(1.25, 0.05), P(1.05, -0.45), P(-1.0, -0.45),
                   stroke_color=th.ink, stroke_width=SW, fill_color=colour(th, spec.get("color"), th.accent2),
                   fill_opacity=0.55)
    waterline = Line(P(-1.12, -0.2), P(1.15, -0.2), stroke_color=th.ink, stroke_width=2, stroke_opacity=0.5)
    cols = [th.accent, th.accent3, th.muted]
    boxes = VGroup(*[Rectangle(width=0.3, height=0.2, stroke_color=th.bg, stroke_width=1.5, fill_color=cols[(i + j) % 3],
                               fill_opacity=0.85).move_to(P(-0.55 + i * 0.32, 0.15 + j * 0.2))
                     for i in range(5) for j in range(2 if i % 2 else 1)])
    bridge = VGroup(Rectangle(width=0.32, height=0.5, stroke_color=th.ink, stroke_width=2, fill_color=th.bg2,
                              fill_opacity=0.9).move_to(P(-0.95, 0.3)),
                    Rectangle(width=0.24, height=0.07, stroke_width=0, fill_color="#9ED8F0", fill_opacity=0.9).move_to(P(-0.95, 0.45)))
    funnel = Rectangle(width=0.12, height=0.22, stroke_width=0, fill_color=th.accent2, fill_opacity=0.9).move_to(P(-0.95, 0.66))
    part = Part(VGroup(hull, waterline, boxes, bridge, funnel), ports={"left": (-0.5, -0.1), "right": (0.5, -0.1),
                                                                      "in": (-0.5, -0.1), "out": (0.5, -0.1)})
    part.floats = 0.62  # fraction of its height that sits above the surface

    def sail(direction):
        def act(value, instant):
            dist = float(value) if isinstance(value, (int, float)) else 1.6
            shift = (RIGHT if direction > 0 else LEFT) * dist * part.mob.width / 2.5
            if instant:
                part.mob.shift(shift)
                return []
            return [part.mob.animate(rate_func=lambda t: t * t * (3 - 2 * t)).shift(shift)]
        return act
    part.actions.update(forward=sail(1), back=sail(-1), sail=sail(1))
    _hl(k, part, hull)
    return part


# ---------------------------------------------------------------------------
# generic
# ---------------------------------------------------------------------------

def block(k, spec):
    """Labelled component box (a server, a repeater, a control unit...). Also the fallback
    for any part kind that doesn't exist yet. An optional `icon` sits inside."""
    th = k.theme
    title = str(spec.get("text") or spec.get("label") or spec.get("kind") or "")[:18]
    box = RoundedRectangle(corner_radius=0.16, width=2.0, height=1.2, stroke_color=colour(th, spec.get("color"), th.ink),
                           stroke_width=SW, fill_color=th.bg2, fill_opacity=0.75)
    inner = VGroup()
    if spec.get("icon"):
        inner.add(k.icon(spec["icon"], 0.62))
    if title and not spec.get("label"):
        inner.add(k.text(title, size=26, weight="BOLD", color=th.ink).scale_to_fit_width(1.7) if len(title) > 10 else
                  k.text(title, size=26, weight="BOLD", color=th.ink))
    if len(inner):
        inner.arrange(DOWN, buff=0.12).move_to(box)
        if inner.height > 1.0:
            inner.scale_to_fit_height(1.0)
    led = Dot(P(0.8, 0.42), radius=0.06, color=th.muted)
    phase = {"a": 0.0}
    led.add_updater(lambda m, dt: (phase.__setitem__("a", phase["a"] + dt * 6),
                                   m.set_color(th.accent if part.speed.get_value() > 0.5 and math.sin(phase["a"]) > 0
                                               else th.muted)))
    part = Part(VGroup(box, inner, led))
    part.motor_actions()
    _hl(k, part, box)
    return part


def node(k, spec):
    """Small junction dot, for splitting or routing flows."""
    th = k.theme
    d = Dot(radius=0.12, color=colour(th, spec.get("color"), th.ink))
    part = Part(VGroup(d, Circle(radius=0.5, stroke_width=0, fill_opacity=0)))
    return part


BUILTIN = {
    "tank": tank, "valve": valve, "pump": pump, "turbine": turbine, "propeller": propeller, "gear": gear,
    "piston": piston, "wheel": wheel, "spring": spring, "motor": motor, "generator": generator,
    "battery": battery, "bulb": bulb, "switch": switch, "heater": heater, "chip": chip, "laser": laser,
    "detector": detector, "mirror": mirror, "lens": lens, "antenna": antenna, "flame": flame, "block": block,
    "node": node, "ship": ship,
}

ALIASES = {
    "chamber": "tank", "reservoir": "tank", "container": "tank", "basin": "tank", "lock": "tank",
    "cistern": "tank", "boiler": "tank", "gate": "valve", "sluice": "valve", "tap": "valve", "fan": "turbine",
    "compressor": "turbine", "rotor": "propeller", "windmill": "propeller", "cog": "gear", "pulley": "wheel",
    "drum": "wheel", "lamp": "bulb", "light": "bulb", "load": "bulb", "resistor": "heater", "element": "heater",
    "processor": "chip", "cpu": "chip", "emitter": "laser", "transmitter": "laser", "source": "laser",
    "receiver": "detector", "sensor": "detector", "photodiode": "detector", "tower": "antenna",
    "burner": "flame", "fire": "flame", "box": "block", "component": "block", "unit": "block",
    "server": "block", "junction": "node", "cell": "battery", "boat": "ship", "vessel": "ship", "tanker": "ship",
    "cargo_ship": "ship", "container_ship": "ship", "barge": "ship", "ferry": "ship",
}

DOCS = {
    "tank": "open container with liquid; actions level(0-1), fill, drain. Fields: level, width (1=normal, 2=wide chamber), liquid colour",
    "valve": "pipe valve / sluice gate; actions open, close. Field: open (start state)",
    "pump": "centrifugal pump with spinning impeller; actions on, off",
    "turbine": "axial turbine / fan / compressor face-on; actions on, off. Field: blades",
    "propeller": "propeller / wind rotor; actions on, off. Field: blades",
    "gear": "toothed gear; actions on, off. Fields: teeth (6-40), dir cw|ccw. Link two gears with kind 'mesh' and they turn together",
    "piston": "cylinder with reciprocating piston (engines, pumps, hydraulics); actions on, off",
    "wheel": "spoked wheel / pulley / drum; actions on, off",
    "spring": "coil spring that oscillates; actions on, off",
    "motor": "electric motor with turning shaft; actions on, off",
    "generator": "generator with spinning coil; actions on, off",
    "battery": "electrical cell (+/-)",
    "bulb": "lamp / electrical load; actions on (glows), off",
    "switch": "knife switch; actions close (on), open (off). Field: closed",
    "heater": "resistor / heating element; actions on (glows orange), off",
    "chip": "integrated circuit; actions on (core pulses), off",
    "laser": "light / signal emitter; actions on, off. Pair with a ray or fibre link carrying light",
    "detector": "receiver / sensor / photodiode; actions on (LED blinks), off",
    "mirror": "flat mirror. Field: angle (degrees)",
    "lens": "converging lens",
    "antenna": "mast radiating waves; actions on, off",
    "flame": "burner / combustion / heat source; actions on (bigger), off",
    "block": "labelled box for an ABSTRACT unit only (server, control unit). Physical objects should be real "
             "parts (request one with make). Actions on (LED), off. Fields: text, icon",
    "ship": "side-view ship; field floats_on: <tank id> makes it ride that tank's water (rises/falls as it fills/"
            "drains); actions forward, back, highlight",
    "node": "small junction dot for splitting / routing flows",
}
