# Kit part "spring_barrel", generated for "How a mechanical watch keeps perfect time with a spring and a wobbling wheel" and kept for reuse.
# Validated with `python -m story_engine validate-part`. Edit freely; keep PART and build(k, spec).
PART = {
    "name": "spring_barrel",
    "use": "flat cylindrical gear drum housing a coiled spring steel ribbon (mainspring); actions on, off, highlight",
    "actions": ["on", "off", "highlight"],
}


def build(k, spec):
    th = k.theme

    # Frame for scaling/positioning reference
    frame = Rectangle(width=2.2, height=2.2, stroke_width=0, fill_opacity=0)

    # Main outer drum casing
    barrel_bg = Circle(
        radius=0.86,
        stroke_color=th.ink,
        stroke_width=3,
        fill_color=th.bg2,
        fill_opacity=0.4,
    )
    barrel_rim = Circle(
        radius=0.86, stroke_color=th.ink, stroke_width=4, fill_opacity=0
    )

    # Outer gear teeth
    num_teeth = 16
    teeth = VGroup()
    r_in_t = 0.86
    r_out_t = 0.96
    da = 2 * math.pi / num_teeth
    for i in range(num_teeth):
        a = i * da
        p1 = P(
            r_in_t * math.cos(a - da * 0.25), r_in_t * math.sin(a - da * 0.25)
        )
        p2 = P(
            r_out_t * math.cos(a - da * 0.15), r_out_t * math.sin(a - da * 0.15)
        )
        p3 = P(
            r_out_t * math.cos(a + da * 0.15), r_out_t * math.sin(a + da * 0.15)
        )
        p4 = P(
            r_in_t * math.cos(a + da * 0.25), r_in_t * math.sin(a + da * 0.25)
        )
        tooth = Polygon(
            p1,
            p2,
            p3,
            p4,
            stroke_color=th.ink,
            stroke_width=2,
            fill_color=th.ink,
            fill_opacity=0.3,
        )
        teeth.add(tooth)

    gear_drum = VGroup(barrel_bg, teeth, barrel_rim)

    # Central arbor / shaft pin
    arbor_outer = Circle(
        radius=0.22,
        stroke_color=th.ink,
        stroke_width=3,
        fill_color=th.muted,
        fill_opacity=0.6,
    )
    arbor_core = Square(
        side_length=0.14,
        stroke_color=th.ink,
        stroke_width=2,
        fill_color=th.ink,
        fill_opacity=0.8,
    )
    arbor = VGroup(arbor_outer, arbor_core)

    # Outer wall spring catch hook
    hook = Line(P(0.80, 0), P(0.86, 0.05), stroke_color=th.ink, stroke_width=4)

    # Base assembly
    base_group = VGroup(frame, gear_drum, hook, arbor)

    part = Part(
        base_group,
        ports={
            "in": (-0.5, 0),
            "out": (0.5, 0),
            "top": (0, 0.5),
            "bottom": (0, -0.5),
            "center": (0, 0),
        },
    )

    hl_tracker = ValueTracker(0.0)
    state = {"rot": 0.0}

    # Tick updater for rotation phase
    def tick(m, dt):
        spd = part.speed.get_value()
        state["rot"] = (state["rot"] + dt * 1.2 * spd) % (2 * math.pi)

    clock = VMobject()
    clock.add_updater(tick)
    part.mob.add(clock)

    # Synchronized rotation for gear teeth
    part.spinner(gear_drum, rate=1.2)

    # Dynamic spring coil drawing relative to frame
    def make_spring():
        c = frame.get_center()
        s = frame.height / 2.2
        hl = hl_tracker.get_value()
        spd = part.speed.get_value()

        r_start = 0.22
        r_end = 0.82
        turns = 4.2
        max_a = turns * 2 * math.pi
        rot = state["rot"]

        stroke_col = (
            th.accent
            if hl > 0.3
            else (th.accent2 if spd > 0.1 else th.ink)
        )
        sw = 4.0 if hl > 0.3 else 3.2

        num_pts = 90
        pts = []
        for i in range(num_pts):
            f = i / (num_pts - 1)
            angle = f * max_a + rot
            r = r_start + (r_end - r_start) * f
            pt = c + P(r * s * math.cos(angle), r * s * math.sin(angle))
            pts.append(pt)

        spiral = VMobject()
        spiral.set_points_smoothly(pts)
        spiral.set_stroke(color=stroke_col, width=sw)

        if hl > 0.05:
            glow = spiral.copy()
            glow.set_stroke(
                color=th.accent, width=sw + 4 * hl, opacity=0.4 * hl
            )
            return VGroup(glow, spiral)

        return spiral

    spring_mob = always_redraw(make_spring)
    part.mob.add(spring_mob)

    # Motor actions for on/off motor speed control
    part.motor_actions()

    def do_highlight(val=True, instant=False):
        target = 1.0 if val else 0.0
        if instant:
            hl_tracker.set_value(target)
            return []
        return [hl_tracker.animate.set_value(target)]

    part.actions["highlight"] = do_highlight

    return part
