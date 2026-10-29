"""Simple rigged 2D characters.

A Character is a VGroup with named parts (head, eyes, mouth, torso, arms,
legs). Poses swap the arm geometry, expressions swap brows + mouth, and two
updaters bring it to life: blinking, and a mouth that flaps only while the
narrator is actually speaking (driven by word timings from the TTS).

Parts are always *built* in rig space (character ~3.2 units tall, centred at
the origin). Two invisible reference dots travel with the group, so any later
move/scale can be re-applied to freshly built parts via `_to_world`.
"""

from __future__ import annotations

import math

import numpy as np
from manim import (
    PI,
    AnimationGroup,
    CapStyleType,
    Circle,
    Dot,
    Ellipse,
    Line,
    RoundedRectangle,
    Sector,
    Transform,
    VGroup,
)

from .stage import stroke_line
from .theme import Theme

SKIN_TONES = ["#F5C6A5", "#E0A47E", "#C68863", "#8D5A3B", "#5C3A24"]
HAIR_COLORS = ["#2B2118", "#5A3825", "#C98B3C", "#1B1B1B", "#B04A2E"]

POSES = ["stand", "wave", "point", "cheer", "think", "shrug", "present"]
EXPRESSIONS = ["happy", "neutral", "sad", "surprised", "worried", "angry", "excited"]
LOOKS = ["adult", "kid", "elder", "robot"]

# (shoulder->elbow, elbow->hand) angles in degrees for screen-left / screen-right arm.
_ARM_ANGLES = {
    "stand":   ((-105, -95), (-75, -85)),
    "wave":    ((-105, -95), (25, 85)),
    "point":   ((-105, -95), (5, 8)),
    "cheer":   ((125, 100), (55, 80)),
    "think":   ((-105, -95), (-35, 125)),
    "shrug":   ((-160, 115), (-20, 65)),
    "present": ((-160, -175), (-20, -5)),
}

LINE = "#1F1A24"


def P(x, y):
    return np.array([x, y, 0.0])


def _polar(r, deg):
    a = math.radians(deg)
    return P(r * math.cos(a), r * math.sin(a))


class Character(VGroup):
    def __init__(self, theme: Theme, look: str = "adult", color: str | None = None,
                 skin: int = 0, hair: int = 0, pose: str = "stand",
                 expression: str = "happy", facing: int = 1, **kwargs):
        super().__init__(**kwargs)
        self.theme = theme
        self.look = look if look in LOOKS else "adult"
        self.robot = self.look == "robot"
        self.kid = self.look == "kid"
        self.shirt = color or theme.accent
        self.skin = "#A9B4C2" if self.robot else SKIN_TONES[skin % len(SKIN_TONES)]
        self.hair_color = "#D9D9D9" if self.look == "elder" else HAIR_COLORS[hair % len(HAIR_COLORS)]
        self.facing = 1 if facing >= 0 else -1
        self.face_dx = 0.08 * self.facing
        self.pose = pose if pose in POSES else "stand"
        self.expression = expression if expression in EXPRESSIONS else "happy"

        self.head_r = 0.62 if self.kid else 0.55
        self.head_c = P(0, 0.95 if self.kid else 1.05)
        self.shoulder_y = 0.28 if self.kid else 0.35
        self.arm_len = 0.48 if self.kid else 0.55

        self._ref0 = Dot(P(0, 0), radius=0.001).set_opacity(0)
        self._ref1 = Dot(P(1, 0), radius=0.001).set_opacity(0)
        self._build()

    # --- rig-space -> world-space ----------------------------------------
    def _scale_origin(self):
        o = self._ref0.get_center()
        s = float(np.linalg.norm(self._ref1.get_center() - o)) or 1.0
        return s, o

    def _to_world(self, m, face: bool = False):
        if face:
            m.shift(P(self.face_dx, 0))
        s, o = self._scale_origin()
        return m.scale(s, about_point=np.zeros(3)).shift(o)

    # --- construction -----------------------------------------------------
    def _build(self):
        body_h = 0.95 if self.kid else 1.15
        leg_col = "#7D8794" if self.robot else "#3A3F58"
        legs = VGroup(stroke_line((-0.2, -0.6), (-0.24, -1.52), leg_col, 16),
                      stroke_line((0.2, -0.6), (0.24, -1.52), leg_col, 16))
        shoes = VGroup(*[Ellipse(width=0.42, height=0.2, fill_color=LINE, fill_opacity=1, stroke_width=0)
                         .move_to(P(s * 0.3, -1.58)) for s in (-1, 1)])

        if self.robot:
            shell = RoundedRectangle(corner_radius=0.18, width=1.15, height=body_h, fill_color="#C9D2DC",
                                     fill_opacity=1, stroke_color="#7D8794", stroke_width=3)
            shell.move_to(P(0, self.shoulder_y - body_h / 2 + 0.05))
            panel = RoundedRectangle(corner_radius=0.06, width=0.55, height=0.35, fill_color=self.shirt,
                                     fill_opacity=1, stroke_width=0).move_to(shell.get_center() + P(0, 0.1))
            torso = VGroup(shell, panel)
            head = RoundedRectangle(corner_radius=0.2, width=1.15, height=1.0, fill_color="#DDE3EA",
                                    fill_opacity=1, stroke_color="#7D8794", stroke_width=3).move_to(self.head_c)
            topper = VGroup(Line(self.head_c + P(0, 0.5), self.head_c + P(0, 0.78), color="#7D8794", stroke_width=5),
                            Circle(radius=0.08, fill_color=self.theme.accent2, fill_opacity=1, stroke_width=0)
                            .move_to(self.head_c + P(0, 0.84)))
        else:
            torso = RoundedRectangle(corner_radius=0.32, width=1.1, height=body_h, fill_color=self.shirt,
                                     fill_opacity=1, stroke_width=0)
            torso.move_to(P(0, self.shoulder_y - body_h / 2 + 0.12))
            head = Circle(radius=self.head_r, fill_color=self.skin, fill_opacity=1, stroke_width=0).move_to(self.head_c)
            topper = self._hair()

        neck = RoundedRectangle(corner_radius=0.06, width=0.24, height=0.3, fill_color=self.skin,
                                fill_opacity=1, stroke_width=0).move_to(P(0, self.shoulder_y + 0.12))

        self.eyes = self._eyes(1.0)
        self.brows = self._brows(self.expression)
        self.mouth = self._mouth(self.expression, 0.0)
        cheeks = VGroup(*[Circle(radius=0.09, fill_color="#FF8FAB", fill_opacity=0 if self.robot else 0.35,
                                 stroke_width=0).move_to(self.head_c + P(s * 0.33, -0.14)) for s in (-1, 1)])
        self.face = VGroup(self.eyes, self.brows, self.mouth, cheeks).shift(P(self.face_dx, 0))

        self.arm_l = self._arm(-1, self.pose)
        self.arm_r = self._arm(1, self.pose)
        extras = self._glasses() if self.look == "elder" else VGroup()
        self.add(self._ref0, self._ref1, legs, shoes, self.arm_l, neck, torso, self.arm_r,
                 head, topper, self.face, extras)

    def _hair(self):
        c, r = self.head_c, self.head_r
        cap = Sector(radius=r * 1.06, angle=PI * 1.06, start_angle=-PI * 0.03, fill_color=self.hair_color,
                     fill_opacity=1, stroke_width=0)
        cap.shift(c - cap.get_arc_center() if hasattr(cap, "get_arc_center") else 0).shift(P(0, 0.06))
        hair = VGroup(cap)
        if self.kid:
            hair.add(Circle(radius=0.17, fill_color=self.hair_color, fill_opacity=1, stroke_width=0)
                     .move_to(c + P(0.12, r + 0.08)))
        if self.look == "adult" and (hash(self.shirt) % 2 == 0):
            # longer hair variant
            hair.add(*[RoundedRectangle(corner_radius=0.1, width=0.22, height=0.75, fill_color=self.hair_color,
                                        fill_opacity=1, stroke_width=0).move_to(c + P(s * (r - 0.02), -0.2))
                       for s in (-1, 1)])
        return hair

    def _eyes(self, open_frac: float):
        c = self.head_c
        if self.robot:
            return VGroup(*[RoundedRectangle(corner_radius=0.05, width=0.2, height=max(0.14 * open_frac, 0.015),
                                             fill_color="#29B6F6", fill_opacity=1, stroke_width=0)
                            .move_to(c + P(s * 0.22, 0.08)) for s in (-1, 1)])
        return VGroup(*[Ellipse(width=0.13, height=max(0.19 * open_frac, 0.02), fill_color=LINE, fill_opacity=1,
                                stroke_width=0).move_to(c + P(s * 0.2, 0.08)) for s in (-1, 1)])

    def _brows(self, expr):
        c = self.head_c
        tilt = {"angry": 22, "worried": -18, "sad": -14}.get(expr, 0)
        lift = 0.34 if expr in ("surprised", "excited") else 0.27
        brows = VGroup()
        for s in (-1, 1):
            mid = c + P(s * 0.2, lift)
            ln = stroke_line((mid[0] - 0.09, mid[1]), (mid[0] + 0.09, mid[1]), LINE, 5)
            ln.rotate(math.radians(-tilt * s), about_point=mid)
            brows.add(ln)
        if self.robot:
            brows.set_opacity(0)
        return brows

    def _mouth(self, expr, openness):
        c = self.head_c + P(0, -0.22)
        if openness > 0.05:
            return VGroup(Ellipse(width=0.24, height=0.06 + 0.16 * openness, fill_color="#7A2E3A",
                                  fill_opacity=1, stroke_color=LINE, stroke_width=3).move_to(c))
        if expr in ("happy", "excited"):
            arc = Sector(radius=0.17, angle=PI, start_angle=PI, fill_color="#7A2E3A", fill_opacity=1,
                         stroke_color=LINE, stroke_width=3) if expr == "excited" else None
            if arc is not None:
                return VGroup(arc.move_to(c))
            from manim import Arc
            return VGroup(Arc(radius=0.17, start_angle=-PI * 0.85, angle=PI * 0.7, stroke_color=LINE,
                              stroke_width=5).move_to(c))
        if expr in ("sad", "worried"):
            from manim import Arc
            return VGroup(Arc(radius=0.17, start_angle=PI * 0.15, angle=PI * 0.7, stroke_color=LINE,
                              stroke_width=5).move_to(c + P(0, -0.04)))
        if expr == "surprised":
            return VGroup(Ellipse(width=0.14, height=0.18, fill_color="#7A2E3A", fill_opacity=1,
                                  stroke_color=LINE, stroke_width=3).move_to(c))
        if expr == "angry":
            return VGroup(stroke_line((c[0] - 0.12, c[1] - 0.03), (c[0] + 0.12, c[1] + 0.02), LINE, 5))
        return VGroup(stroke_line((c[0] - 0.1, c[1]), (c[0] + 0.1, c[1]), LINE, 5))

    def _arm(self, side, pose):
        a1, a2 = _ARM_ANGLES.get(pose, _ARM_ANGLES["stand"])[0 if side < 0 else 1]
        shoulder = P(side * 0.5, self.shoulder_y - 0.05)
        elbow = shoulder + _polar(self.arm_len, a1)
        hand = elbow + _polar(self.arm_len * 0.95, a2)
        upper = Line(shoulder, elbow, color="#A9B4C2" if self.robot else self.shirt, stroke_width=15)
        lower = Line(elbow, hand, color=self.skin, stroke_width=13)
        for ln in (upper, lower):
            ln.set_cap_style(CapStyleType.ROUND)
        return VGroup(upper, lower, Circle(radius=0.1, fill_color=self.skin, fill_opacity=1,
                                           stroke_width=0).move_to(hand))

    def _glasses(self):
        c = self.head_c + P(self.face_dx, 0)
        lenses = VGroup(*[Circle(radius=0.13, stroke_color=LINE, stroke_width=3).move_to(c + P(s * 0.2, 0.08))
                          for s in (-1, 1)])
        return VGroup(lenses, stroke_line((c[0] - 0.07, c[1] + 0.08), (c[0] + 0.07, c[1] + 0.08), LINE, 3))

    # --- animations ---------------------------------------------------------
    def hand_point(self, side: int = 1) -> np.ndarray:
        return (self.arm_r if side > 0 else self.arm_l)[2].get_center()

    def pose_to(self, pose: str):
        pose = pose if pose in POSES else "stand"
        self.pose = pose
        new_l = self._to_world(self._arm(-1, pose))
        new_r = self._to_world(self._arm(1, pose))
        return AnimationGroup(Transform(self.arm_l, new_l), Transform(self.arm_r, new_r))

    def express(self, expression: str):
        expression = expression if expression in EXPRESSIONS else "neutral"
        self.expression = expression
        anims = [Transform(self.brows, self._to_world(self._brows(expression), face=True))]
        if not self.mouth.get_updaters():
            anims.append(Transform(self.mouth, self._to_world(self._mouth(expression, 0.0), face=True)))
        return AnimationGroup(*anims)

    def attach_life(self, clock, speech: list[tuple[float, float]] | None, talking: bool):
        """Blink + lip flap. `clock()` returns current scene time in seconds."""
        speech = speech or []
        phase = (sum(map(ord, self.shirt)) % 100) / 37.0

        def eyes_updater(eyes):
            t = clock() + phase
            closed = (t % 3.7) < 0.13
            eyes.become(self._to_world(self._eyes(0.1 if closed else 1.0), face=True))

        self.eyes.add_updater(eyes_updater)

        if talking and speech:
            def mouth_updater(m):
                t = clock()
                if any(s <= t <= e for s, e in speech):
                    amt = 0.35 + 0.65 * abs(math.sin(t * 16.0 + phase))
                    m.become(self._to_world(self._mouth(self.expression, amt), face=True))
                else:
                    m.become(self._to_world(self._mouth(self.expression, 0.0), face=True))

            self.mouth.add_updater(mouth_updater)
        return self


def make_character(theme: Theme, spec: dict, index: int = 0) -> Character:
    key = str(spec.get("color", "")).strip()
    color = {"accent": theme.accent, "accent2": theme.accent2, "accent3": theme.accent3}.get(key)
    if color is None:
        color = key if (key.startswith("#") and len(key) == 7) else theme.accents[(index + 1) % 3 if index else 0]
    skin = spec.get("skin", index * 2)
    return Character(
        theme,
        look=str(spec.get("look", "adult")).lower(),
        color=color,
        skin=int(skin) if str(skin).isdigit() else index * 2,
        hair=index + 1,
        pose=str(spec.get("pose", "stand")).lower(),
        expression=str(spec.get("expression", "happy")).lower(),
        facing=1 if index % 2 == 0 else -1,
    )
