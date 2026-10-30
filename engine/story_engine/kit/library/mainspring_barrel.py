# Kit part "mainspring_barrel", generated for "How a mechanical watch keeps perfect time with a spring and a wobbling wheel" and kept for reuse.
# Validated with `python -m story_engine validate-part`. Edit freely; keep PART and build(k, spec).
PART = {
    "name": "mainspring_barrel",
    "use": "gear-toothed mainspring drum containing coiled ribbon spring; actions: on, off, highlight",
    "actions": ["on", "off", "highlight"],
}

def build(k, spec):
    th = k.theme
    accent_col = colour(th, spec.get("color"), th.accent)

    frame = Rectangle(width=2.4, height=2.4, stroke_width=0, fill_opacity=0)
    
    state = {"angle": 0.0}
    highlight_vt = ValueTracker(0.0)
    tightness_vt = ValueTracker(0.3)

    ports = {
        "in": (-0.5, 0),
        "out": (0.5, 0),
        "top": (0, 0.5),
        "bottom": (0, -0.5),
        "center": (0, 0),
        "arbor": (0, 0),
        "mesh": (0.5, 0),
    }

    part = Part(VGroup(frame), ports=ports)

    def draw_barrel():
        C = frame.get_center()
        S = frame.width / 2.4
        A = state["angle"]
        T = tightness_vt.get_value()
        H = highlight_vt.get_value()

        g = VGroup()

        # Outer drum casing fill
        casing = Circle(
            radius=0.90 * S,
            stroke_color=th.ink,
            stroke_width=4,
            fill_color=th.bg2,
            fill_opacity=0.45,
        ).move_to(C)
        g.add(casing)

        # Gear teeth around outer rim
        num_teeth = 24
        for i in range(num_teeth):
            ang = A + i * (2 * math.pi / num_teeth)
            r_mid = 0.96 * S
            tooth = Rectangle(
                width=0.06 * S,
                height=0.12 * S,
                stroke_color=th.ink,
                stroke_width=2,
                fill_color=th.ink,
                fill_opacity=0.75,
            ).move_to(C + P(r_mid * math.cos(ang), r_mid * math.sin(ang)))
            tooth.rotate(ang)
            g.add(tooth)

        # Coiled mainspring spiral
        turns = 3.2 + 2.3 * T
        r_in = 0.22 * S
        r_out = (0.88 - 0.28 * T) * S
        max_theta = turns * 2 * math.pi
        n_pts = 110

        pts = []
        for i in range(n_pts + 1):
            t = i / n_pts
            theta = t * max_theta
            r = r_in + (r_out - r_in) * (t ** 0.85)
            ang = theta + A
            pts.append(C + P(r * math.cos(ang), r * math.sin(ang)))

        spring_line = VMobject()
        spring_line.set_points_as_corners(pts)
        
        active_col = th.accent2 if H > 0.5 else accent_col
        spring_line.set_stroke(color=active_col, width=3.5 + 2.5 * H, opacity=0.95)
        g.add(spring_line)

        # Central arbor hub
        arbor = Circle(
            radius=0.20 * S,
            stroke_color=th.ink,
            stroke_width=3,
            fill_color=th.ink,
            fill_opacity=0.85,
        ).move_to(C)

        slot = Square(
            side_length=0.11 * S,
            stroke_color=th.bg,
            stroke_width=1.5,
            fill_color=th.bg,
            fill_opacity=1.0,
        ).move_to(C)
        slot.rotate(A)
        g.add(arbor, slot)

        # Highlight ring outer illumination
        if H > 0.01:
            hl_ring = Circle(
                radius=1.08 * S,
                stroke_color=th.accent2,
                stroke_width=3 + 3 * H,
                stroke_opacity=H * 0.85,
            ).move_to(C)
            g.add(hl_ring)

        return g

    barrel_mob = always_redraw(draw_barrel)
    part.mob.add(barrel_mob)

    def tick(m, dt):
        state["angle"] = (state["angle"] + dt * 1.8 * part.speed.get_value()) % (2 * math.pi)

    clock = VMobject()
    clock.add_updater(tick)
    part.mob.add(clock)

    part.motor_actions()

    def highlight_action(value=True, instant=False):
        target = 1.0 if value else 0.0
        if instant:
            highlight_vt.set_value(target)
            tightness_vt.set_value(0.75 if value else 0.3)
            return []
        return [
            highlight_vt.animate.set_value(target),
            tightness_vt.animate.set_value(0.75 if value else 0.3),
        ]

    part.actions["highlight"] = highlight_action

    return part
