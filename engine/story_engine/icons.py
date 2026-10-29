"""Procedural flat-icon library.

Every icon is drawn from primitives inside a ~2x2 box, then normalised by
`build_icon`. Theme-coloured icons use the theme accents; "natural" objects
(trees, water, fire, money...) keep semantic colours so they stay recognisable
on any theme. Unknown names resolve through SYNONYMS, then fall back to a
lettered badge — `build_icon` never raises for a bad name.
"""

from __future__ import annotations

from types import SimpleNamespace
from typing import Callable

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
    Difference,
    Ellipse,
    Intersection,
    LineJointType,
    Line,
    ParametricFunction,
    Polygon,
    Rectangle,
    RoundedRectangle,
    Sector,
    Star,
    Union,
    VGroup,
    VMobject,
)

from .stage import make_text, round_caps, stroke_line
from .theme import Theme

GREEN, DKGREEN = "#3DBE6A", "#23864A"
BLUE, LTBLUE = "#3FA7D6", "#9ED8F0"
GOLD, DKGOLD = "#FFC93C", "#D99A00"
RED, ORANGE = "#FF5A5F", "#FF8C42"
BROWN, PINK = "#8B5E3C", "#FF8FAB"
WHITE, CLOUD = "#FFFFFF", "#EEF2F7"


def P(x: float, y: float) -> np.ndarray:
    return np.array([x, y, 0.0])


def circ(r, fill, x=0.0, y=0.0, stroke=None, sw=0.0, op=1.0):
    c = Circle(radius=r, fill_color=fill, fill_opacity=op,
               stroke_color=stroke or fill, stroke_width=sw)
    return c.move_to(P(x, y))


def rrect(w, h, r, fill, x=0.0, y=0.0, stroke=None, sw=0.0, op=1.0):
    m = RoundedRectangle(corner_radius=r, width=w, height=h, fill_color=fill,
                         fill_opacity=op, stroke_color=stroke or fill, stroke_width=sw)
    return m.move_to(P(x, y))


def poly(pts, fill, stroke=None, sw=0.0, op=1.0):
    return Polygon(*[P(*p) for p in pts], fill_color=fill, fill_opacity=op,
                   stroke_color=stroke or fill, stroke_width=sw)


def smooth(pts, fill, op=1.0):
    m = VMobject(fill_color=fill, fill_opacity=op, stroke_width=0)
    m.set_points_smoothly([P(*p) for p in pts])
    return m


def filled(m, fill, op=1.0):
    return m.set_fill(fill, opacity=op).set_stroke(width=0)


def L(a, b, color, w=8.0):
    return stroke_line(a, b, color, w)


def glyph(ch, color, size=60):
    return make_text(ch, size=size, color=color, weight="BOLD")


# ---------------------------------------------------------------------------
# Icon builders: (palette) -> VGroup
# palette c: main/second/third (theme accents), ink, bg, light, dark
# ---------------------------------------------------------------------------

def i_person(c):
    head = circ(0.4, c.main, y=0.55)
    body = Intersection(rrect(1.5, 1.4, 0.6, c.main, y=-0.6), Rectangle(width=2, height=1.1).move_to(P(0, -0.45)))
    return VGroup(filled(body, c.main), head)


def i_people(c):
    def one(col, x, s):
        g = i_person(SimpleNamespace(main=col))
        return g.scale(s).move_to(P(x, -0.05 * (1 / s)))
    return VGroup(one(c.second, -0.7, 0.7), one(c.third, 0.7, 0.7), one(c.main, 0, 0.9))


def i_lightbulb(c):
    bulb = VGroup(circ(0.68, GOLD, y=0.3), poly([(-0.42, -0.05), (0.42, -0.05), (0.3, -0.55), (-0.3, -0.55)], GOLD))
    base = VGroup(rrect(0.62, 0.16, 0.06, c.light, y=-0.66), rrect(0.55, 0.16, 0.06, c.muted, y=-0.84),
                  rrect(0.3, 0.12, 0.05, c.light, y=-0.98))
    fil = round_caps(VMobject(stroke_color=DKGOLD, stroke_width=5).set_points_as_corners(
        [P(-0.2, -0.4), P(-0.2, 0.15), P(-0.07, 0.35), P(0.07, 0.15), P(0.2, 0.35), P(0.2, -0.4)]))
    shine = Arc(radius=0.45, start_angle=PI * 0.6, angle=PI * 0.35, stroke_color=WHITE, stroke_width=7).shift(P(0, 0.3))
    return VGroup(bulb, fil, shine, base)


def i_gear(c):
    teeth = VGroup()
    for k in range(8):
        t = Rectangle(width=0.36, height=0.42, fill_color=c.main, fill_opacity=1, stroke_width=0)
        t.move_to(P(0, 0.82)).rotate(k * TAU / 8, about_point=P(0, 0))
        teeth.add(t)
    body = Union(circ(0.72, c.main), *teeth)
    return VGroup(filled(Difference(body, circ(0.3, c.main)), c.main))


def i_heart(c):
    h = VGroup(circ(0.47, RED, x=-0.43, y=0.28), circ(0.47, RED, x=0.43, y=0.28),
               poly([(-0.86, 0.12), (0, 0.45), (0.86, 0.12), (0, -0.9)], RED))
    shine = circ(0.12, WHITE, x=-0.5, y=0.45, op=0.7)
    return VGroup(h, shine)


def i_star(c):
    return VGroup(Star(5, outer_radius=1.0, inner_radius=0.45, fill_color=GOLD, fill_opacity=1,
                       stroke_color=DKGOLD, stroke_width=4))


def i_globe(c):
    ball = circ(0.92, BLUE)
    land = VGroup(
        smooth([(-0.55, 0.55), (-0.1, 0.65), (0.0, 0.25), (-0.3, -0.05), (-0.6, 0.1), (-0.55, 0.55)], GREEN),
        smooth([(0.2, -0.15), (0.6, 0.0), (0.7, -0.45), (0.35, -0.7), (0.15, -0.45), (0.2, -0.15)], GREEN),
    )
    grid = VGroup(Ellipse(width=0.9, height=1.84, stroke_color=WHITE, stroke_width=2.5, stroke_opacity=0.5),
                  L((-0.92, 0), (0.92, 0), WHITE, 2.5).set_opacity(0.5))
    return VGroup(ball, land, grid)


def i_rocket(c):
    body = Union(rrect(0.7, 1.3, 0.3, c.light, y=-0.1),
                 smooth([(-0.35, 0.35), (-0.2, 0.75), (0, 0.98), (0.2, 0.75), (0.35, 0.35)], c.light))
    fins = VGroup(poly([(-0.35, -0.2), (-0.7, -0.75), (-0.35, -0.6)], c.main),
                  poly([(0.35, -0.2), (0.7, -0.75), (0.35, -0.6)], c.main))
    window = circ(0.18, c.third, y=0.25, stroke=c.main, sw=5)
    flame = VGroup(smooth([(-0.25, -0.78), (0, -1.25), (0.25, -0.78)], ORANGE),
                   smooth([(-0.12, -0.78), (0, -1.05), (0.12, -0.78)], GOLD))
    return VGroup(flame, fins, filled(body, c.light), window).rotate(-PI / 6)


def i_house(c):
    roof = poly([(-1.0, 0.15), (0, 0.95), (1.0, 0.15)], c.main)
    wall = rrect(1.5, 1.05, 0.06, c.light, y=-0.38)
    door = rrect(0.38, 0.6, 0.08, c.second, x=0.3, y=-0.6)
    win = rrect(0.38, 0.34, 0.05, c.third, x=-0.35, y=-0.25)
    chimney = rrect(0.22, 0.4, 0.02, c.main, x=0.55, y=0.7)
    return VGroup(chimney, wall, roof, door, win)


def i_tree(c):
    trunk = rrect(0.28, 0.8, 0.08, BROWN, y=-0.6)
    crown = Union(circ(0.5, GREEN, y=0.45), circ(0.42, GREEN, x=-0.42, y=0.05), circ(0.42, GREEN, x=0.42, y=0.05),
                  circ(0.38, GREEN, y=0.0))
    return VGroup(trunk, filled(crown, GREEN), circ(0.12, WHITE, x=-0.2, y=0.6, op=0.35))


def i_sun(c):
    rays = VGroup(*[L((0.78 * np.cos(a), 0.78 * np.sin(a)), (1.0 * np.cos(a), 1.0 * np.sin(a)), ORANGE, 10)
                    for a in np.arange(0, TAU, TAU / 10)])
    return VGroup(rays, circ(0.58, GOLD))


def i_moon(c):
    return VGroup(filled(Difference(circ(0.85, "#FFE8A3"), circ(0.75, "#FFE8A3", x=0.42, y=0.3)), "#FFE8A3"))


def _cloud(fill):
    return filled(Union(circ(0.42, fill, x=-0.5, y=-0.1), circ(0.58, fill, x=0.05, y=0.15),
                        circ(0.4, fill, x=0.55, y=-0.08), rrect(1.75, 0.6, 0.3, fill, y=-0.25)), fill)


def i_cloud(c):
    return VGroup(_cloud(CLOUD).set_stroke(c.muted, width=2, opacity=0.6))


def i_rain(c):
    drops = VGroup(*[L((x, -0.75), (x - 0.12, -1.05), BLUE, 9) for x in (-0.45, 0.05, 0.55)])
    return VGroup(drops, _cloud(CLOUD).shift(UP * 0.25).set_stroke(c.muted, width=2, opacity=0.6))


def i_lightning(c):
    return VGroup(poly([(0.15, 1.0), (-0.55, -0.1), (-0.05, -0.1), (-0.25, -1.0), (0.55, 0.15), (0.05, 0.15)],
                       GOLD, stroke=DKGOLD, sw=3))


def i_coin(c):
    return VGroup(circ(0.9, GOLD, stroke=DKGOLD, sw=8), circ(0.66, GOLD, stroke=DKGOLD, sw=3),
                  glyph("$", DKGOLD, 80))


def i_money(c):
    note = rrect(1.95, 1.1, 0.12, GREEN, stroke=DKGREEN, sw=4)
    inner = rrect(1.6, 0.8, 0.1, GREEN, stroke=WHITE, sw=2, op=0)
    back = note.copy().set_fill(DKGREEN).shift(UP * 0.15 + RIGHT * 0.15)
    return VGroup(back, note, inner, circ(0.28, WHITE, op=0.25), glyph("$", WHITE, 46))


def i_clock(c):
    face = circ(0.9, c.light, stroke=c.main, sw=10)
    ticks = VGroup(*[L((0.7 * np.cos(a), 0.7 * np.sin(a)), (0.8 * np.cos(a), 0.8 * np.sin(a)), c.dark, 5)
                     for a in np.arange(0, TAU, TAU / 12)])
    hands = VGroup(L((0, 0), (0, 0.55), c.dark, 8), L((0, 0), (0.4, -0.15), c.main, 8))
    return VGroup(face, ticks, hands, circ(0.07, c.dark))


def i_book(c):
    left = poly([(-1.0, 0.65), (-0.05, 0.5), (-0.05, -0.75), (-1.0, -0.6)], c.main)
    right = poly([(1.0, 0.65), (0.05, 0.5), (0.05, -0.75), (1.0, -0.6)], c.second)
    lines = VGroup(*[L((-0.8, y), (-0.25, y - 0.08), c.light, 3) for y in (0.3, 0.05, -0.2)],
                   *[L((0.25, y - 0.08), (0.8, y), c.light, 3) for y in (0.3, 0.05, -0.2)])
    return VGroup(left, right, lines.set_opacity(0.8))


def i_phone(c):
    return VGroup(rrect(0.95, 1.85, 0.16, c.dark, stroke=c.ink, sw=4),
                  rrect(0.78, 1.45, 0.06, c.main, y=0.05),
                  rrect(0.25, 0.06, 0.03, c.ink, y=0.85), circ(0.06, c.ink, y=-0.78))


def i_laptop(c):
    screen = rrect(1.6, 1.05, 0.08, c.dark, y=0.2, stroke=c.ink, sw=4)
    inner = rrect(1.4, 0.85, 0.04, c.main, y=0.2)
    base = poly([(-1.0, -0.4), (1.0, -0.4), (0.85, -0.6), (-0.85, -0.6)], c.muted)
    return VGroup(screen, inner, base)


def i_chart(c):
    bars = VGroup(rrect(0.4, 0.55, 0.06, c.third, x=-0.6, y=-0.47),
                  rrect(0.4, 0.95, 0.06, c.second, x=0.0, y=-0.27),
                  rrect(0.4, 1.45, 0.06, c.main, x=0.6, y=-0.02))
    axis = L((-1.0, -0.78), (1.0, -0.78), c.ink, 5)
    trend = round_caps(VMobject(stroke_color=c.ink, stroke_width=6).set_points_as_corners(
        [P(-0.85, -0.2), P(-0.2, 0.2), P(0.15, 0.0), P(0.85, 0.8)]))
    return VGroup(bars, axis, trend, circ(0.08, c.ink, x=0.85, y=0.8))


def i_chart_down(c):
    bars = VGroup(rrect(0.4, 1.45, 0.06, c.main, x=-0.6, y=-0.02),
                  rrect(0.4, 0.95, 0.06, c.second, x=0.0, y=-0.27),
                  rrect(0.4, 0.55, 0.06, RED, x=0.6, y=-0.47))
    return VGroup(bars, L((-1.0, -0.78), (1.0, -0.78), c.ink, 5))


def i_lock(c):
    shackle = Arc(radius=0.45, start_angle=0, angle=PI, stroke_color=c.ink, stroke_width=14).shift(UP * 0.2)
    legs = VGroup(L((-0.45, 0.2), (-0.45, -0.1), c.ink, 14), L((0.45, 0.2), (0.45, -0.1), c.ink, 14))
    body = rrect(1.35, 1.05, 0.18, c.main, y=-0.45)
    hole = VGroup(circ(0.13, c.dark, y=-0.35), rrect(0.1, 0.3, 0.04, c.dark, y=-0.55))
    return VGroup(round_caps(shackle), legs, body, hole)


def i_key(c):
    ring = circ(0.38, c.main, x=-0.5, op=0, stroke=c.main, sw=16)
    shaft = L((-0.12, 0), (0.95, 0), c.main, 16)
    teeth = VGroup(L((0.6, 0), (0.6, -0.3), c.main, 14), L((0.85, 0), (0.85, -0.25), c.main, 14))
    return VGroup(ring, shaft, teeth).rotate(PI / 6)


def i_check(c):
    mark = round_caps(VMobject(stroke_color=WHITE, stroke_width=22).set_points_as_corners(
        [P(-0.45, 0.0), P(-0.1, -0.35), P(0.5, 0.35)]))
    return VGroup(circ(0.92, GREEN), mark)


def i_cross(c):
    return VGroup(circ(0.92, RED), L((-0.38, -0.38), (0.38, 0.38), WHITE, 22), L((-0.38, 0.38), (0.38, -0.38), WHITE, 22))


def i_question(c):
    return VGroup(circ(0.92, c.main), glyph("?", c.bg, 96))


def i_warning(c):
    tri = poly([(0, 0.95), (1.0, -0.8), (-1.0, -0.8)], GOLD, stroke=GOLD, sw=24)
    tri.set_joint_type(LineJointType.ROUND)
    return VGroup(tri, glyph("!", "#2A2A2A", 90).shift(DOWN * 0.12))


def _flame(scale, fill):
    return smooth([(0, 1.0), (0.42, 0.3), (0.62, -0.3), (0.38, -0.78), (0, -0.92),
                   (-0.38, -0.78), (-0.62, -0.3), (-0.42, 0.3), (0, 1.0)], fill).scale(scale)


def i_fire(c):
    return VGroup(_flame(1.0, ORANGE), _flame(0.55, GOLD).shift(DOWN * 0.38), _flame(0.22, WHITE).shift(DOWN * 0.6))


def i_drop(c):
    d = Union(circ(0.62, BLUE, y=-0.3), poly([(0, 1.0), (-0.54, -0.02), (0.54, -0.02)], BLUE))
    shine = Arc(radius=0.38, start_angle=PI * 0.95, angle=PI * 0.4, stroke_color=WHITE, stroke_width=7).shift(DOWN * 0.3)
    return VGroup(filled(d, BLUE), round_caps(shine))


def i_leaf(c):
    leaf = Intersection(circ(1.0, GREEN, x=-0.55), circ(1.0, GREEN, x=0.55))
    rib = L((0, -0.8), (0, 0.7), DKGREEN, 5)
    veins = VGroup(*[L((0, y), (s * 0.28, y + 0.25), DKGREEN, 3) for y in (-0.35, 0.0, 0.3) for s in (-1, 1)])
    return VGroup(filled(leaf, GREEN), rib, veins).rotate(-PI / 5)


def i_brain(c):
    lobes = Union(*[circ(r, PINK, x=x, y=y) for x, y, r in
                    [(-0.5, 0.3, 0.45), (0.0, 0.45, 0.48), (0.5, 0.3, 0.45),
                     (-0.6, -0.2, 0.4), (0.0, -0.15, 0.5), (0.6, -0.2, 0.4)]])
    folds = VGroup(L((0, 0.85), (0, -0.55), "#D9547A", 5),
                   Arc(0.25, PI * 0.2, PI * 0.8, stroke_color="#D9547A", stroke_width=4).move_to(P(-0.45, 0.25)),
                   Arc(0.25, 0, PI * 0.8, stroke_color="#D9547A", stroke_width=4).move_to(P(0.45, 0.25)),
                   Arc(0.22, PI, PI * 0.8, stroke_color="#D9547A", stroke_width=4).move_to(P(-0.5, -0.25)),
                   Arc(0.22, -PI * 0.2, PI * 0.8, stroke_color="#D9547A", stroke_width=4).move_to(P(0.5, -0.25)))
    return VGroup(filled(lobes, PINK), round_caps(folds))


def i_atom(c):
    orbits = VGroup(*[Ellipse(width=2.0, height=0.7, stroke_color=c.main, stroke_width=5).rotate(a)
                      for a in (0, PI / 3, 2 * PI / 3)])
    return VGroup(orbits, circ(0.2, c.second), circ(0.08, c.third, x=1.0))


def i_search(c):
    lens = circ(0.55, c.light, x=-0.2, y=0.2, stroke=c.main, sw=16, op=0.25)
    handle = L((0.22, -0.22), (0.8, -0.8), c.main, 22)
    return VGroup(handle, lens)


def i_trophy(c):
    cup = VGroup(filled(Sector(radius=0.7, angle=PI, start_angle=PI), GOLD).move_to(P(0, 0.12)),
                rrect(1.4, 0.45, 0.04, GOLD, y=0.5))
    handles = VGroup(Arc(0.28, PI / 2, PI, stroke_color=GOLD, stroke_width=10).move_to(P(-0.78, 0.35)),
                     Arc(0.28, -PI / 2, PI, stroke_color=GOLD, stroke_width=10).move_to(P(0.78, 0.35)))
    stem = rrect(0.2, 0.42, 0.04, DKGOLD, y=-0.42)
    base = rrect(0.9, 0.22, 0.06, DKGOLD, y=-0.72)
    return VGroup(handles, cup, stem, base, Star(5, outer_radius=0.2, fill_color=WHITE,
                                                                fill_opacity=0.9, stroke_width=0).move_to(P(0, 0.25)))


def i_flag(c):
    pole = L((-0.7, -0.95), (-0.7, 0.95), c.muted, 8)
    cloth = smooth([(-0.7, 0.9), (-0.1, 1.0), (0.35, 0.75), (0.85, 0.85), (0.85, 0.05),
                    (0.35, -0.05), (-0.1, 0.2), (-0.7, 0.1), (-0.7, 0.9)], c.main)
    return VGroup(pole, cloth)


def i_target(c):
    rings = VGroup(*[circ(r, RED if k % 2 == 0 else WHITE) for k, r in enumerate((0.95, 0.72, 0.49, 0.24))])
    dart = VGroup(L((0.0, 0.0), (0.85, 0.85), c.dark, 7),
                  poly([(0.75, 0.95), (1.0, 1.0), (0.95, 0.75)], c.main))
    return VGroup(rings, dart)


def i_chat(c):
    bubble = Union(rrect(1.9, 1.25, 0.4, c.main, y=0.15), poly([(-0.5, -0.4), (-0.75, -0.9), (-0.1, -0.4)], c.main))
    dots = VGroup(*[circ(0.12, c.bg, x=x, y=0.15) for x in (-0.45, 0, 0.45)])
    return VGroup(filled(bubble, c.main), dots)


def i_shield(c):
    s = smooth([(0, 0.98), (0.45, 0.82), (0.82, 0.78), (0.78, 0.0), (0.45, -0.6), (0, -0.98),
                (-0.45, -0.6), (-0.78, 0.0), (-0.82, 0.78), (-0.45, 0.82), (0, 0.98)], c.main)
    mark = round_caps(VMobject(stroke_color=c.light, stroke_width=16).set_points_as_corners(
        [P(-0.35, 0.0), P(-0.08, -0.28), P(0.38, 0.28)]))
    return VGroup(s, mark)


def i_battery(c):
    shell = rrect(1.7, 0.95, 0.14, c.dark, stroke=c.ink, sw=7, op=0.4)
    tip = rrect(0.14, 0.38, 0.04, c.ink, x=0.95)
    cells = VGroup(*[rrect(0.4, 0.65, 0.06, GREEN, x=x) for x in (-0.55, -0.05, 0.45)])
    return VGroup(shell, tip, cells)


def i_mountain(c):
    back = poly([(-0.2, -0.75), (0.45, 0.5), (1.05, -0.75)], c.second)
    front = poly([(-1.05, -0.75), (-0.25, 0.85), (0.6, -0.75)], c.main)
    snow = poly([(-0.25, 0.85), (-0.48, 0.38), (-0.33, 0.45), (-0.25, 0.35), (-0.12, 0.47), (-0.03, 0.4)], WHITE)
    return VGroup(back, front, snow)


def i_car(c):
    cabin = poly([(-0.55, 0.0), (-0.35, 0.5), (0.4, 0.5), (0.65, 0.0)], c.main)
    win = VGroup(poly([(-0.4, 0.05), (-0.27, 0.4), (-0.02, 0.4), (-0.02, 0.05)], LTBLUE),
                 poly([(0.08, 0.05), (0.08, 0.4), (0.33, 0.4), (0.5, 0.05)], LTBLUE))
    body = rrect(2.0, 0.6, 0.22, c.main, y=-0.2)
    wheels = VGroup(*[VGroup(circ(0.28, "#222222", x=x, y=-0.5), circ(0.11, "#BBBBBB", x=x, y=-0.5))
                      for x in (-0.55, 0.55)])
    lights = VGroup(rrect(0.14, 0.12, 0.04, GOLD, x=0.93, y=-0.12), rrect(0.1, 0.12, 0.04, RED, x=-0.95, y=-0.12))
    return VGroup(cabin, win, body, lights, wheels)


def i_plane(c):
    body = rrect(0.32, 2.0, 0.16, c.light)
    wings = poly([(-1.05, -0.05), (0, 0.4), (1.05, -0.05), (1.05, -0.25), (0, 0.0), (-1.05, -0.25)], c.main)
    tail = poly([(-0.4, -0.85), (0, -0.6), (0.4, -0.85), (0.4, -0.95), (0, -0.82), (-0.4, -0.95)], c.main)
    return VGroup(wings, tail, body, circ(0.08, c.third, y=0.7)).rotate(-PI / 4)


def i_bell(c):
    b = VGroup(circ(0.55, GOLD, y=0.25), poly([(-0.55, 0.22), (0.55, 0.22), (0.8, -0.55), (-0.8, -0.55)], GOLD))
    rim = rrect(1.8, 0.2, 0.1, DKGOLD, y=-0.58)
    return VGroup(circ(0.15, DKGOLD, y=-0.78), b, rim, circ(0.1, DKGOLD, y=0.85))


def i_mail(c):
    env = rrect(1.9, 1.3, 0.1, c.light, stroke=c.muted, sw=3)
    flap = round_caps(VMobject(stroke_color=c.main, stroke_width=10).set_points_as_corners(
        [P(-0.88, 0.55), P(0, -0.1), P(0.88, 0.55)]))
    return VGroup(env, flap)


def i_camera(c):
    body = rrect(1.9, 1.25, 0.2, c.dark, y=-0.1, stroke=c.ink, sw=4)
    top = rrect(0.6, 0.3, 0.06, c.dark, x=-0.35, y=0.6, stroke=c.ink, sw=4)
    lens = VGroup(circ(0.45, c.muted, y=-0.1), circ(0.32, c.main, y=-0.1), circ(0.1, WHITE, x=0.1, y=0.0, op=0.8))
    return VGroup(top, body, lens, circ(0.08, GOLD, x=0.65, y=0.3))


def i_music(c):
    heads = VGroup(Ellipse(width=0.55, height=0.4, fill_color=c.main, fill_opacity=1, stroke_width=0)
                   .rotate(PI / 7).move_to(P(-0.55, -0.6)),
                   Ellipse(width=0.55, height=0.4, fill_color=c.main, fill_opacity=1, stroke_width=0)
                   .rotate(PI / 7).move_to(P(0.6, -0.4)))
    stems = VGroup(L((-0.32, -0.55), (-0.32, 0.7), c.main, 9), L((0.83, -0.35), (0.83, 0.9), c.main, 9))
    beam = poly([(-0.36, 0.55), (0.87, 0.75), (0.87, 1.0), (-0.36, 0.8)], c.main)
    return VGroup(stems, beam, heads)


def i_gift(c):
    box = rrect(1.5, 1.05, 0.08, c.main, y=-0.4)
    lid = rrect(1.75, 0.35, 0.06, c.second, y=0.27)
    ribbon = VGroup(rrect(0.24, 1.05, 0.0, c.light, y=-0.4), rrect(0.26, 0.35, 0.0, c.light, y=0.27))
    bow = VGroup(Ellipse(width=0.55, height=0.32, fill_color=c.light, fill_opacity=1, stroke_width=0)
                 .rotate(PI / 6).move_to(P(-0.25, 0.6)),
                 Ellipse(width=0.55, height=0.32, fill_color=c.light, fill_opacity=1, stroke_width=0)
                 .rotate(-PI / 6).move_to(P(0.25, 0.6)))
    return VGroup(box, lid, ribbon, bow)


def i_cart(c):
    basket = poly([(-0.75, 0.45), (0.95, 0.45), (0.75, -0.3), (-0.55, -0.3)], c.main)
    handle = round_caps(VMobject(stroke_color=c.ink, stroke_width=8).set_points_as_corners(
        [P(-1.0, 0.75), P(-0.78, 0.75), P(-0.55, -0.3), P(-0.55, -0.5), P(0.8, -0.5)]))
    wheels = VGroup(circ(0.14, c.ink, x=-0.4, y=-0.75), circ(0.14, c.ink, x=0.6, y=-0.75))
    return VGroup(handle, basket, wheels)


def i_factory(c):
    bldg = poly([(-1.0, -0.8), (-1.0, 0.1), (-0.5, 0.45), (-0.5, 0.1), (0.0, 0.45), (0.0, 0.1),
                 (0.5, 0.45), (0.5, -0.8)], c.muted)
    chimney = rrect(0.32, 1.35, 0.03, c.second, x=0.75, y=-0.12)
    smoke = VGroup(circ(0.16, CLOUD, x=0.8, y=0.75, op=0.8), circ(0.22, CLOUD, x=0.95, y=1.0, op=0.6))
    wins = VGroup(*[rrect(0.22, 0.22, 0.03, GOLD, x=x, y=-0.3) for x in (-0.7, -0.25, 0.2)])
    return VGroup(smoke, chimney, bldg, wins)


def i_dna(c):
    s1 = ParametricFunction(lambda t: P(0.55 * np.sin(t), t / 3.2, ), t_range=[-3.1, 3.1], stroke_color=c.main, stroke_width=8)
    s2 = ParametricFunction(lambda t: P(-0.55 * np.sin(t), t / 3.2), t_range=[-3.1, 3.1], stroke_color=c.second, stroke_width=8)
    rungs = VGroup(*[L((0.55 * np.sin(t), t / 3.2), (-0.55 * np.sin(t), t / 3.2), c.third, 4)
                     for t in np.linspace(-2.8, 2.8, 9)])
    return VGroup(rungs, s1, s2).rotate(PI / 8)


def i_virus(c):
    spikes = VGroup()
    for a in np.arange(0, TAU, TAU / 8):
        d = np.array([np.cos(a), np.sin(a)])
        spikes.add(L(tuple(d * 0.6), tuple(d * 0.92), GREEN, 7), circ(0.11, GREEN, x=d[0] * 0.95, y=d[1] * 0.95))
    body = circ(0.62, GREEN)
    spots = VGroup(circ(0.12, DKGREEN, x=-0.2, y=0.15), circ(0.09, DKGREEN, x=0.22, y=-0.18),
                   circ(0.07, DKGREEN, x=0.15, y=0.28))
    return VGroup(spikes, body, spots)


def i_plant(c):
    pot = poly([(-0.55, -0.25), (0.55, -0.25), (0.42, -1.0), (-0.42, -1.0)], "#D9764A")
    rim = rrect(1.25, 0.22, 0.05, "#B85C33", y=-0.22)
    stem = L((0, -0.2), (0, 0.6), DKGREEN, 8)
    leaves = VGroup(filled(Intersection(circ(0.5, GREEN, x=-0.28), circ(0.5, GREEN, x=0.28)), GREEN)
                    .rotate(PI / 3).move_to(P(-0.35, 0.45)),
                    filled(Intersection(circ(0.5, GREEN, x=-0.28), circ(0.5, GREEN, x=0.28)), GREEN)
                    .rotate(-PI / 3).move_to(P(0.35, 0.65)))
    return VGroup(stem, leaves, pot, rim)


def i_cup(c):
    mug = rrect(1.1, 1.2, 0.15, c.main, x=-0.15, y=-0.3)
    handle = circ(0.3, c.main, x=0.5, y=-0.25, op=0, sw=12)
    steam = VGroup(*[smooth_stroke([(x, 0.4), (x + 0.12, 0.6), (x - 0.06, 0.8), (x + 0.06, 1.0)], c.muted, 6)
                     for x in (-0.4, -0.1, 0.2)])
    return VGroup(handle, mug, steam)


def smooth_stroke(pts, color, w):
    m = VMobject(stroke_color=color, stroke_width=w)
    m.set_points_smoothly([P(*p) for p in pts])
    return round_caps(m)


def i_calendar(c):
    page = rrect(1.7, 1.6, 0.14, c.light, y=-0.1)
    head = rrect(1.7, 0.42, 0.14, c.main, y=0.5)
    rings = VGroup(rrect(0.1, 0.35, 0.05, c.dark, x=-0.45, y=0.75), rrect(0.1, 0.35, 0.05, c.dark, x=0.45, y=0.75))
    dots = VGroup(*[rrect(0.24, 0.2, 0.04, c.second if (r, k) == (1, 2) else c.muted, x=-0.55 + k * 0.37, y=0.0 - r * 0.32)
                    for r in range(3) for k in range(4)])
    return VGroup(page, head, rings, dots)


def i_eye(c):
    almond = Intersection(circ(1.0, WHITE, y=-0.55), circ(1.0, WHITE, y=0.55))
    return VGroup(filled(almond, WHITE).scale([1.25, 1, 1]), circ(0.36, c.main), circ(0.16, "#111111"),
                  circ(0.07, WHITE, x=0.1, y=0.1))


def i_pin(c):
    p = VGroup(circ(0.6, c.main, y=0.3), poly([(-0.52, 0.02), (0.52, 0.02), (0, -0.98)], c.main))
    return VGroup(p, circ(0.24, c.light, y=0.3))


def i_wifi(c):
    arcs = VGroup(*[round_caps(Arc(radius=r, start_angle=PI / 4, angle=PI / 2, stroke_color=c.main, stroke_width=14))
                    .shift(DOWN * 0.6) for r in (0.45, 0.85, 1.25)])
    return VGroup(arcs, circ(0.14, c.main, y=-0.6))


def i_chip(c):
    pins = VGroup()
    for k in np.linspace(-0.45, 0.45, 4):
        pins.add(L((k, 0.65), (k, 0.95), c.muted, 7), L((k, -0.65), (k, -0.95), c.muted, 7),
                 L((0.65, k), (0.95, k), c.muted, 7), L((-0.65, k), (-0.95, k), c.muted, 7))
    return VGroup(pins, rrect(1.4, 1.4, 0.14, c.dark, stroke=c.muted, sw=4), rrect(0.75, 0.75, 0.08, c.main))


def i_hourglass(c):
    frame = VGroup(rrect(1.4, 0.16, 0.06, BROWN, y=0.95), rrect(1.4, 0.16, 0.06, BROWN, y=-0.95))
    glass = poly([(-0.55, 0.85), (0.55, 0.85), (0.08, 0.0), (0.55, -0.85), (-0.55, -0.85), (-0.08, 0.0)], LTBLUE, op=0.45)
    sand = VGroup(poly([(-0.32, 0.45), (0.32, 0.45), (0.0, 0.08)], GOLD),
                  poly([(-0.48, -0.83), (0.48, -0.83), (0.0, -0.4)], GOLD), L((0, 0.05), (0, -0.45), GOLD, 3))
    return VGroup(glass, sand, frame)


def i_planet(c):
    back = Arc(radius=1, start_angle=0, angle=PI, stroke_color=c.second, stroke_width=9).scale([1.05, 0.28, 1])
    ball = circ(0.62, c.main)
    band = Intersection(ball.copy(), Rectangle(width=2, height=0.18).move_to(P(0, 0.18)))
    front = Arc(radius=1, start_angle=PI, angle=PI, stroke_color=c.second, stroke_width=9).scale([1.05, 0.28, 1])
    return VGroup(back, ball, filled(band, c.third, 0.6), front).rotate(PI / 10)


def i_pill(c):
    left = Intersection(rrect(1.9, 0.75, 0.375, c.main), Rectangle(width=0.95, height=1).move_to(P(-0.475, 0)))
    right = Intersection(rrect(1.9, 0.75, 0.375, c.light), Rectangle(width=0.95, height=1).move_to(P(0.475, 0)))
    return VGroup(filled(left, c.main), filled(right, c.light)).rotate(PI / 5)


def i_graduation(c):
    top = poly([(0, 0.55), (1.1, 0.1), (0, -0.35), (-1.1, 0.1)], c.dark, stroke=c.ink, sw=2)
    base = poly([(-0.6, -0.1), (0.6, -0.1), (0.55, -0.6), (-0.55, -0.6)], c.dark, stroke=c.ink, sw=2)
    tassel = VGroup(L((0, 0.1), (0.85, -0.05), GOLD, 4), L((0.85, -0.05), (0.85, -0.6), GOLD, 5),
                    circ(0.08, GOLD, x=0.85, y=-0.65))
    return VGroup(base, top, tassel)


def i_balance(c):
    post = VGroup(L((0, -0.85), (0, 0.75), c.ink, 8), rrect(0.9, 0.16, 0.06, c.ink, y=-0.9))
    beam = L((-0.85, 0.6), (0.85, 0.6), c.ink, 7)
    pans = VGroup(*[VGroup(L((x, 0.6), (x - 0.3, -0.05), c.muted, 3), L((x, 0.6), (x + 0.3, -0.05), c.muted, 3),
                           Sector(radius=0.36, angle=PI, start_angle=PI, fill_color=c.main, fill_opacity=1,
                                  stroke_width=0).move_to(P(x, -0.15))) for x in (-0.85, 0.85)])
    return VGroup(post, beam, pans, circ(0.1, c.main, y=0.75))


def i_flower(c):
    petals = VGroup(*[Ellipse(width=0.62, height=0.95, fill_color=PINK, fill_opacity=1, stroke_width=0)
                      .move_to(P(0, 0.45)).rotate(k * TAU / 6, about_point=P(0, 0)) for k in range(6)])
    head = VGroup(petals, circ(0.3, GOLD), circ(0.12, DKGOLD, x=0.06, y=0.05)).shift(UP * 0.3)
    stem = L((0, -0.25), (0, -1.0), DKGREEN, 9)
    leaf = filled(Intersection(circ(0.4, GREEN, x=-0.22), circ(0.4, GREEN, x=0.22)), GREEN).rotate(PI / 4).move_to(P(0.28, -0.65))
    return VGroup(stem, leaf, head)


def i_bee(c):
    wings = VGroup(Ellipse(width=0.6, height=0.85, fill_color=LTBLUE, fill_opacity=0.7, stroke_color=WHITE,
                           stroke_width=3).rotate(-PI / 7).move_to(P(-0.25, 0.55)),
                   Ellipse(width=0.6, height=0.85, fill_color=LTBLUE, fill_opacity=0.7, stroke_color=WHITE,
                           stroke_width=3).rotate(PI / 7).move_to(P(0.3, 0.55)))
    body = Ellipse(width=1.7, height=1.05, fill_color=GOLD, fill_opacity=1, stroke_width=0)
    stripes = VGroup(*[Intersection(body.copy(), Rectangle(width=0.2, height=1.2).move_to(P(x, 0)))
                       for x in (-0.25, 0.15, 0.55)])
    for st_ in stripes:
        filled(st_, "#2A2A2A")
    head = circ(0.36, "#2A2A2A", x=-0.85, y=0.05)
    eye = circ(0.08, WHITE, x=-0.95, y=0.12)
    sting = poly([(0.83, 0.08), (1.05, 0.0), (0.83, -0.08)], "#2A2A2A")
    return VGroup(sting, body, stripes, head, eye, wings)


def i_fish(c):
    body = Intersection(circ(1.0, c.main, y=-0.5), circ(1.0, c.main, y=0.5)).scale([1.1, 1, 1])
    tail = poly([(0.75, 0.0), (1.15, 0.45), (1.15, -0.45)], c.second)
    return VGroup(tail, filled(body, c.main).shift(LEFT * 0.15), circ(0.1, c.dark, x=-0.65, y=0.12),
                  Arc(0.35, -PI / 4, PI / 2, stroke_color=c.second, stroke_width=5).move_to(P(-0.25, 0)))


def i_bird(c):
    body = Ellipse(width=1.5, height=1.1, fill_color=c.main, fill_opacity=1, stroke_width=0).move_to(P(0.1, -0.1))
    head = circ(0.42, c.main, x=-0.55, y=0.35)
    wing = Ellipse(width=0.9, height=0.5, fill_color=c.second, fill_opacity=1, stroke_width=0).rotate(-PI / 8).move_to(P(0.25, 0.0))
    beak = poly([(-0.95, 0.42), (-1.2, 0.32), (-0.95, 0.24)], GOLD)
    tail = poly([(0.8, 0.0), (1.2, 0.35), (1.15, -0.2)], c.main)
    legs = VGroup(L((0.0, -0.62), (0.0, -0.9), ORANGE, 5), L((0.3, -0.62), (0.3, -0.9), ORANGE, 5))
    return VGroup(tail, legs, body, head, wing, beak, circ(0.07, c.dark, x=-0.62, y=0.42))


def i_apple(c):
    a = VGroup(circ(0.55, RED, x=-0.28, y=-0.1), circ(0.55, RED, x=0.28, y=-0.1), circ(0.5, RED, y=-0.35))
    stem = L((0, 0.35), (0.08, 0.7), BROWN, 7)
    leaf = filled(Intersection(circ(0.3, GREEN, x=-0.16), circ(0.3, GREEN, x=0.16)), GREEN).rotate(PI / 5).move_to(P(0.3, 0.62))
    return VGroup(a, stem, leaf, circ(0.11, WHITE, x=-0.42, y=0.08, op=0.6))


def i_paw(c):
    pad = Ellipse(width=0.95, height=0.78, fill_color=c.main, fill_opacity=1, stroke_width=0).move_to(P(0, -0.35))
    toes = VGroup(*[Ellipse(width=0.34, height=0.44, fill_color=c.main, fill_opacity=1, stroke_width=0)
                    .rotate(r).move_to(P(x, y)) for x, y, r in
                    [(-0.62, 0.15, 0.4), (-0.22, 0.52, 0.1), (0.22, 0.52, -0.1), (0.62, 0.15, -0.4)]])
    return VGroup(pad, toes)


def i_crown(c):
    body = poly([(-0.95, -0.45), (-0.95, 0.55), (-0.48, 0.1), (0, 0.75), (0.48, 0.1), (0.95, 0.55), (0.95, -0.45)], GOLD)
    band = rrect(1.9, 0.28, 0.06, DKGOLD, y=-0.42)
    gems = VGroup(circ(0.1, RED, x=-0.5, y=-0.42), circ(0.12, BLUE, y=-0.42), circ(0.1, GREEN, x=0.5, y=-0.42))
    tips = VGroup(*[circ(0.09, GOLD, x=x, y=y) for x, y in [(-0.95, 0.6), (0, 0.8), (0.95, 0.6)]])
    return VGroup(body, band, gems, tips)


def i_drone(c):
    arms = VGroup(*[L((0, 0), (x * 0.68, y * 0.68), c.muted, 9) for x in (-1, 1) for y in (-1, 1)])
    rotors = VGroup()
    for x in (-1, 1):
        for y in (-1, 1):
            p = P(x * 0.68, y * 0.68)
            rotors.add(circ(0.3, c.main, x=p[0], y=p[1], op=0.25, stroke=c.main, sw=4),
                       circ(0.07, c.ink, x=p[0], y=p[1]))
    body = rrect(0.6, 0.6, 0.16, c.dark, stroke=c.ink, sw=4)
    return VGroup(arms, rotors, body, circ(0.08, c.second))


# --- engineering & infrastructure (the channel's core vocabulary) -----------

def i_bridge(c):
    water = rrect(2.1, 0.32, 0.06, LTBLUE, y=-0.8, op=0.55)
    towers = VGroup(*[rrect(0.13, 1.55, 0.03, c.main, x=x, y=0.12) for x in (-0.55, 0.55)])
    deck = rrect(2.1, 0.13, 0.03, c.muted, y=-0.38)
    span = ParametricFunction(lambda t: P(t, -0.08 + 0.95 * (t / 0.55) ** 2), t_range=[-0.55, 0.55],
                              stroke_color=c.light, stroke_width=6)
    sides = VGroup(L((-1.05, -0.32), (-0.55, 0.87), c.light, 6), L((1.05, -0.32), (0.55, 0.87), c.light, 6))
    hangers = VGroup(*[L((x, -0.08 + 0.95 * (x / 0.55) ** 2), (x, -0.33), c.light, 2.5).set_opacity(0.8)
                       for x in (-0.42, -0.28, -0.14, 0.14, 0.28, 0.42)])
    piers = VGroup(*[rrect(0.2, 0.4, 0.03, c.muted, x=x, y=-0.62) for x in (-0.55, 0.55)])
    return VGroup(water, piers, hangers, span, sides, towers, deck)


def i_tower(c):
    legs = poly([(-0.75, -1.0), (-0.11, 0.72), (0.11, 0.72), (0.75, -1.0), (0.42, -1.0), (0.0, -0.5),
                 (-0.42, -1.0)], c.main)
    braces = VGroup(L((-0.5, -0.38), (0.25, 0.12), c.light, 3), L((0.5, -0.38), (-0.25, 0.12), c.light, 3))
    decks = VGroup(rrect(1.05, 0.11, 0.02, c.light, y=-0.38), rrect(0.58, 0.09, 0.02, c.light, y=0.12))
    spire = poly([(-0.07, 0.72), (0.0, 1.02), (0.07, 0.72)], c.main)
    return VGroup(legs, braces.set_opacity(0.55), decks, spire)


def i_skyscraper(c):
    side = rrect(0.5, 1.2, 0.04, c.second, x=-0.64, y=-0.4)
    side2 = rrect(0.46, 0.95, 0.04, c.third, x=0.62, y=-0.53)
    main = poly([(-0.36, -1.0), (-0.36, 0.62), (0.0, 0.85), (0.36, 0.62), (0.36, -1.0)], c.main)
    antenna = L((0.0, 0.85), (0.0, 1.05), c.light, 4)
    wins = VGroup(*[rrect(0.12, 0.1, 0.02, c.light, x=x, y=y, op=0.85)
                    for y in np.arange(-0.8, 0.5, 0.24) for x in (-0.15, 0.15)])
    return VGroup(side, side2, antenna, main, wins)


def i_dam(c):
    water = poly([(-1.0, 0.55), (-0.18, 0.55), (0.02, -0.9), (-1.0, -0.9)], BLUE)
    ripples = VGroup(*[L((-0.85 + dx, 0.35 - dy), (-0.55 + dx, 0.35 - dy), WHITE, 3).set_opacity(0.5)
                       for dx, dy in ((0.0, 0.0), (0.25, 0.35), (-0.05, 0.7))])
    wall = poly([(-0.22, 0.75), (0.14, 0.75), (0.62, -0.9), (-0.02, -0.9)], c.muted)
    crest = rrect(0.5, 0.1, 0.02, c.light, x=-0.04, y=0.78)
    spill = VGroup(*[L((0.5, -0.45 - k * 0.15), (1.0, -0.62 - k * 0.15), LTBLUE, 5) for k in range(3)])
    return VGroup(water, ripples, spill, wall, crest)


def i_turbine(c):
    mast = poly([(-0.06, 0.3), (0.06, 0.3), (0.12, -1.0), (-0.12, -1.0)], c.light)
    blades = VGroup(*[poly([(0.0, 0.0), (0.09, 0.16), (0.03, 0.78), (-0.05, 0.16)], c.light)
                      .rotate(k * TAU / 3 + 0.35, about_point=P(0, 0)) for k in range(3)]).shift(P(0, 0.32))
    hub = circ(0.11, c.main, y=0.32)
    ground = rrect(0.9, 0.08, 0.03, c.muted, y=-1.0)
    return VGroup(ground, mast, blades, hub)


def i_crane(c):
    mast = rrect(0.16, 1.75, 0.02, GOLD, x=-0.4, y=-0.12)
    lattice = VGroup(*[L((-0.47, y), (-0.33, y + 0.2), DKGOLD, 3) for y in np.arange(-0.95, 0.55, 0.2)])
    jib = rrect(1.85, 0.12, 0.02, GOLD, x=0.18, y=0.76)
    peak = VGroup(L((-0.4, 1.05), (1.0, 0.8), c.light, 2.5), L((-0.4, 1.05), (-0.95, 0.8), c.light, 2.5))
    counter = rrect(0.32, 0.24, 0.03, c.muted, x=-0.86, y=0.62)
    cab = rrect(0.26, 0.22, 0.03, c.main, x=-0.4, y=0.58)
    cable = L((0.72, 0.7), (0.72, -0.12), c.light, 3)
    load = rrect(0.42, 0.3, 0.03, c.second, x=0.72, y=-0.3)
    base = rrect(0.6, 0.12, 0.02, c.muted, x=-0.4, y=-1.0)
    return VGroup(base, mast, lattice, peak, jib, counter, cab, cable, load)


def i_ship(c):
    water = smooth([(-1.05, -0.7), (-0.6, -0.62), (-0.1, -0.72), (0.4, -0.62), (1.05, -0.7)], LTBLUE, op=0.0)
    water.set_stroke(LTBLUE, width=5)
    hull = poly([(-1.0, -0.12), (1.0, -0.12), (0.78, -0.6), (-0.82, -0.6)], c.main)
    cols = [c.second, c.third, c.light]
    boxes = VGroup(*[rrect(0.24, 0.2, 0.02, cols[(i + j) % 3], x=-0.2 + i * 0.27, y=0.0 + j * 0.22)
                     for i in range(4) for j in range(2)])
    bridge = VGroup(rrect(0.32, 0.5, 0.03, c.light, x=-0.68, y=0.15), rrect(0.26, 0.08, 0.02, LTBLUE, x=-0.68, y=0.3))
    return VGroup(boxes, bridge, hull, water)


def i_train(c):
    rail = L((-1.05, -0.72), (1.05, -0.72), c.muted, 5)
    body = poly([(-1.0, -0.45), (-1.0, 0.4), (0.45, 0.4), (1.0, -0.05), (1.0, -0.45)], c.main)
    stripe = rrect(1.95, 0.08, 0.02, c.third, x=-0.02, y=-0.28)
    wins = VGroup(*[rrect(0.26, 0.24, 0.04, LTBLUE, x=x, y=0.12) for x in (-0.75, -0.4, -0.05, 0.3)])
    wheels = VGroup(*[circ(0.13, "#222222", x=x, y=-0.55) for x in (-0.7, -0.35, 0.35, 0.7)])
    return VGroup(rail, body, stripe, wins, wheels)


def i_jet_engine(c):
    cowl = circ(0.98, c.muted)
    inner = circ(0.82, c.dark)
    blades = VGroup(*[poly([(0.0, 0.1), (0.8, 0.12), (0.8, -0.02), (0.0, -0.02)], c.light)
                      .rotate(k * TAU / 14, about_point=P(0, 0)) for k in range(14)])
    return VGroup(cowl, inner, blades.set_opacity(0.9), circ(0.24, c.main), circ(0.08, c.light))


def i_satellite(c):
    def panel(x):
        p = rrect(0.62, 0.42, 0.03, BLUE, x=x)
        grid = VGroup(L((x, -0.21), (x, 0.21), LTBLUE, 2), L((x - 0.31, 0), (x + 0.31, 0), LTBLUE, 2))
        return VGroup(p, grid)
    bar = L((-0.95, 0), (0.95, 0), c.light, 4)
    body = rrect(0.42, 0.55, 0.06, GOLD)
    dish = Arc(radius=0.22, start_angle=PI * 0.15, angle=PI * 0.7, stroke_color=c.light, stroke_width=5).shift(P(0, 0.2))
    return VGroup(bar, panel(-0.68), panel(0.68), body, dish, circ(0.05, c.light, y=0.48)).rotate(-PI / 8)


def i_pylon(c):
    legs = poly([(-0.6, -1.0), (-0.1, 0.85), (0.1, 0.85), (0.6, -1.0), (0.35, -1.0), (0.0, 0.0), (-0.35, -1.0)], c.muted)
    arms = VGroup(rrect(1.6, 0.08, 0.02, c.light, y=0.5), rrect(1.1, 0.08, 0.02, c.light, y=0.82))
    wires = VGroup(*[L((x, y - 0.12), (x + 0.35 * (1 if x > 0 else -1), y - 0.3), c.main, 3)
                     for x, y in ((-0.78, 0.5), (0.78, 0.5), (-0.53, 0.82), (0.53, 0.82))])
    insulators = VGroup(*[circ(0.05, GOLD, x=x, y=y - 0.1) for x, y in ((-0.78, 0.5), (0.78, 0.5), (-0.53, 0.82), (0.53, 0.82))])
    return VGroup(legs, wires, arms, insulators)


def i_wrench(c):
    handle = rrect(0.28, 1.5, 0.14, c.light, y=-0.3)
    head = filled(Difference(circ(0.45, c.light, y=0.6), Rectangle(width=0.32, height=0.6).move_to(P(0, 0.9))), c.light)
    return VGroup(handle, head).rotate(-PI / 4)


def i_ruler(c):
    body = rrect(2.0, 0.55, 0.06, GOLD)
    ticks = VGroup(*[L((x, 0.27), (x, 0.27 - (0.22 if k % 2 == 0 else 0.12)), DKGOLD, 3)
                     for k, x in enumerate(np.linspace(-0.85, 0.85, 11))])
    return VGroup(body, ticks).rotate(PI / 8)


def i_magnet(c):
    arch = filled(Intersection(Difference(circ(0.75, RED), circ(0.38, RED)),
                               Rectangle(width=2.0, height=0.8).move_to(P(0, 0.4))), RED)
    legs = VGroup(*[rrect(0.37, 0.72, 0.0, RED, x=x, y=-0.36) for x in (-0.565, 0.565)])
    tips = VGroup(*[rrect(0.37, 0.26, 0.02, "#CFD6DE", x=x, y=-0.85) for x in (-0.565, 0.565)])
    return VGroup(arch, legs, tips)


ICONS: dict[str, Callable] = {
    name[2:]: fn for name, fn in globals().items() if name.startswith("i_") and callable(fn)
}

SYNONYMS: dict[str, str] = {
    "man": "person", "woman": "person", "user": "person", "human": "person", "child": "person",
    "kid": "person", "customer": "person", "worker": "person",
    "team": "people", "crowd": "people", "community": "people", "family": "people", "society": "people",
    "idea": "lightbulb", "bulb": "lightbulb", "insight": "lightbulb", "innovation": "lightbulb",
    "settings": "gear", "machine": "gear", "process": "gear", "engine": "gear", "cog": "gear", "system": "gear",
    "love": "heart", "health": "heart", "care": "heart", "emotion": "heart",
    "earth": "globe", "world": "globe", "planet_earth": "globe", "international": "globe", "global": "globe",
    "launch": "rocket", "startup": "rocket", "space": "rocket", "growth_fast": "rocket",
    "home": "house", "building": "house",
    "forest": "tree", "nature": "tree",
    "day": "sun", "light": "sun", "summer": "sun", "energy_solar": "sun", "solar": "sun",
    "night": "moon", "sleep": "moon",
    "weather": "cloud", "storage": "cloud",
    "storm": "rain", "lightning_bolt": "lightning", "energy": "lightning", "power": "lightning",
    "electricity": "lightning", "fast": "lightning", "bolt": "lightning",
    "dollar": "coin", "price": "coin", "cost": "coin", "cash": "money", "salary": "money", "wealth": "money",
    "time": "clock", "deadline": "clock", "watch": "clock",
    "learn": "book", "education": "book", "study": "book", "knowledge": "book", "story": "book",
    "mobile": "phone", "smartphone": "phone", "app": "phone",
    "computer": "laptop", "tech": "laptop", "technology": "laptop", "internet": "wifi", "online": "wifi",
    "data": "chart", "growth": "chart", "statistics": "chart", "stats": "chart", "graph": "chart",
    "decline": "chart_down", "loss": "chart_down", "drop_chart": "chart_down",
    "security": "lock", "privacy": "lock", "secret": "lock", "safe": "shield", "protection": "shield",
    "password": "key", "access": "key", "solution": "key",
    "yes": "check", "correct": "check", "success": "check", "done": "check", "true": "check",
    "no": "cross", "wrong": "cross", "fail": "cross", "false": "cross", "error": "cross",
    "mystery": "question", "why": "question", "unknown": "question", "help": "question",
    "danger": "warning", "alert": "warning", "risk": "warning", "caution": "warning",
    "hot": "fire", "heat": "fire", "flame": "fire", "burn": "fire",
    "water": "drop", "liquid": "drop", "ocean": "drop", "rainwater": "drop",
    "plant_leaf": "leaf", "ecology": "leaf", "green": "leaf", "environment": "leaf",
    "mind": "brain", "think": "brain", "intelligence": "brain", "ai": "brain", "memory": "brain",
    "science": "atom", "physics": "atom", "chemistry": "atom", "nuclear": "atom",
    "find": "search", "discover": "search", "research": "search", "magnifier": "search", "investigate": "search",
    "win": "trophy", "award": "trophy", "victory": "trophy", "champion": "trophy",
    "goal": "target", "aim": "target", "focus": "target",
    "country": "flag", "milestone": "flag", "finish": "flag",
    "talk": "chat", "message": "chat", "conversation": "chat", "speech": "chat", "communication": "chat",
    "defense": "shield", "immune": "shield",
    "charge": "battery",
    "climb": "mountain", "challenge": "mountain", "peak": "mountain",
    "travel": "plane", "flight": "plane", "airplane": "plane",
    "transport": "car", "vehicle": "car", "drive": "car",
    "notification": "bell", "reminder": "bell",
    "email": "mail", "letter": "mail",
    "photo": "camera", "picture": "camera",
    "song": "music", "sound": "music", "audio": "music",
    "present": "gift", "reward": "gift", "bonus": "gift",
    "shop": "cart", "shopping": "cart", "buy": "cart", "market": "cart", "commerce": "cart",
    "industry": "factory", "pollution": "factory", "manufacturing": "factory",
    "suspension_bridge": "bridge", "viaduct": "bridge", "span": "bridge", "overpass": "bridge",
    "landmark": "tower", "monument": "tower", "eiffel": "tower", "eiffel_tower": "tower", "lattice": "tower",
    "statue": "tower", "skyline": "skyscraper", "highrise": "skyscraper", "city": "skyscraper",
    "reservoir": "dam", "hydro": "dam", "hydropower": "dam", "hydroelectric": "dam",
    "wind": "turbine", "wind_turbine": "turbine", "windmill": "turbine", "wind_power": "turbine",
    "construction": "crane", "building_site": "crane", "lift": "crane",
    "boat": "ship", "cargo": "ship", "container_ship": "ship", "port": "ship", "shipping": "ship",
    "railway": "train", "rail": "train", "metro": "train", "subway": "train", "locomotive": "train",
    "jet": "jet_engine", "turbofan": "jet_engine", "engine_jet": "jet_engine", "fan": "jet_engine",
    "orbit": "satellite", "gps": "satellite", "space_station": "satellite",
    "power_line": "pylon", "power_grid": "pylon", "grid": "pylon", "transmission": "pylon",
    "tool": "wrench", "tools": "wrench", "repair": "wrench", "maintenance": "wrench", "engineering": "wrench",
    "measure": "ruler", "length": "ruler", "height": "ruler", "size": "ruler", "scale": "ruler",
    "magnetism": "magnet", "magnetic": "magnet", "attraction": "magnet",
    "gene": "dna", "genetics": "dna", "biology": "dna",
    "germ": "virus", "disease": "virus", "bacteria": "virus", "infection": "virus",
    "grow": "plant", "seed": "plant", "sprout": "plant",
    "coffee": "cup", "tea": "cup", "drink": "cup",
    "date": "calendar", "schedule": "calendar", "plan": "calendar",
    "see": "eye", "vision": "eye", "watch_eye": "eye",
    "location": "pin", "place": "pin", "map": "pin",
    "network": "wifi", "connection": "wifi", "signal": "wifi",
    "processor": "chip", "cpu": "chip", "hardware": "chip", "robot_brain": "chip",
    "waiting": "hourglass", "patience": "hourglass", "history": "hourglass",
    "saturn": "planet", "universe": "planet", "astronomy": "planet",
    "medicine": "pill", "drug": "pill", "pharmacy": "pill",
    "school": "graduation", "university": "graduation", "degree": "graduation", "graduate": "graduation",
    "flowers": "flower", "garden": "flower", "spring": "flower", "blossom": "flower", "pollen": "flower",
    "insect": "bee", "honey": "bee", "pollinator": "bee", "bug": "bee",
    "sea": "fish", "marine": "fish", "fishing": "fish",
    "animal": "paw", "pet": "paw", "dog": "paw", "cat": "paw", "wildlife": "paw",
    "freedom": "bird", "tweet": "bird", "migration": "bird",
    "food": "apple", "fruit": "apple", "nutrition": "apple", "diet": "apple", "eat": "apple",
    "king": "crown", "queen": "crown", "royal": "crown", "leader": "crown", "best": "crown",
    "quadcopter": "drone", "uav": "drone", "multicopter": "drone",
    "justice": "balance", "law": "balance", "fair": "balance", "fairness": "balance", "compare": "balance",
}


def resolve_icon(name: str | None) -> str | None:
    if not name:
        return None
    key = str(name).strip().lower().replace(" ", "_").replace("-", "_")
    if key in ICONS:
        return key
    if key in SYNONYMS:
        return SYNONYMS[key]
    if key.endswith("s") and key[:-1] in ICONS:
        return key[:-1]
    if key.endswith("s") and key[:-1] in SYNONYMS:
        return SYNONYMS[key[:-1]]
    return None


def palette_for(theme: Theme, accent_index: int = 0) -> SimpleNamespace:
    a = theme.accents
    k = accent_index % 3
    return SimpleNamespace(
        main=a[k], second=a[(k + 1) % 3], third=a[(k + 2) % 3],
        ink=theme.ink, bg=theme.bg, muted=theme.muted,
        light="#F7F7F2" if theme.dark else "#FFFFFF",
        dark=theme.bg if theme.dark else theme.ink,
    )


def build_icon(name: str | None, theme: Theme, size: float = 1.6, accent_index: int = 0,
               label_fallback: str = "") -> VGroup:
    """Return a normalised icon `size` units tall. Never raises on unknown names."""
    c = palette_for(theme, accent_index)
    key = resolve_icon(name)
    icon: VGroup
    if key:
        try:
            icon = VGroup(*ICONS[key](c))
        except Exception:
            icon = _badge(c, name or label_fallback)
    else:
        icon = _badge(c, name or label_fallback)
    icon.scale_to_fit_height(size)
    if icon.width > size * 1.25:
        icon.scale_to_fit_width(size * 1.25)
    return icon.move_to(np.zeros(3))


def _badge(c, word: str) -> VGroup:
    letter = (str(word).strip()[:1] or "?").upper()
    return VGroup(circ(0.92, c.main), circ(0.92, c.main, op=0, stroke=c.light, sw=4).scale(0.85),
                  glyph(letter, c.bg, 90))


__all__ = ["ICONS", "SYNONYMS", "build_icon", "resolve_icon", "palette_for"]
