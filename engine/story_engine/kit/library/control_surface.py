# Kit part "control_surface", generated for "How Fighter Jets Don't Tear Themselves Apart Spinning" and kept for reuse.
# Validated with `python -m story_engine validate-part`. Edit freely; keep PART and build(k, spec).
PART = {
    "name": "control_surface",
    "use": "hinged trailing edge flap or elevon that deflects up/down; actions: on, off, up, down, neutral, highlight",
    "actions": ["on", "off", "up", "down", "neutral", "highlight"],
}

def build(k, spec):
    th = k.theme
    acc_color = colour(th, "accent", th.accent)

    # Frame bounding box
    frame = Rectangle(width=2.4, height=1.6, stroke_width=0, fill_opacity=0)

    # Fixed main wing section (leading edge to hinge)
    main_wing = VMobject()
    main_wing.set_points_smoothly([
        P(-1.05, 0),
        P(-0.8, 0.24),
        P(-0.35, 0.34),
        P(0.08, 0.26),
        P(0.08, -0.26),
        P(-0.35, -0.34),
        P(-0.8, -0.24),
        P(-1.05, 0),
    ])
    main_wing.set_stroke(color=th.ink, width=4)
    main_wing.set_fill(color=th.muted, opacity=0.3)

    # Hinge pivot point
    hinge_pin = Circle(
        radius=0.07,
        stroke_color=th.ink,
        stroke_width=3,
        fill_color=th.bg,
        fill_opacity=1.0,
    ).move_to(P(0.08, 0))
    hinge_dot = Dot(P(0.08, 0), radius=0.03, color=th.ink)

    base_group = VGroup(frame, main_wing, hinge_pin, hinge_dot)

    part = Part(
        base_group,
        ports={
            "in": (-0.45, 0),
            "out": (0.45, 0),
            "top": (0, 0.45),
            "bottom": (0, -0.45),
            "hinge": (0.03, 0),
        },
    )

    init_angle = float(spec.get("angle", 0.0))
    angle_tracker = ValueTracker(init_angle)
    highlight_tracker = ValueTracker(0.0)

    def rot_vec(v, deg):
        rad = math.radians(deg)
        c, s = math.cos(rad), math.sin(rad)
        return np.array([v[0] * c - v[1] * s, v[0] * s + v[1] * c, 0])

    def draw_dynamic():
        c = frame.get_center()
        s = frame.width / 2.4

        deg = angle_tracker.get_value()
        hl = highlight_tracker.get_value()

        stroke_w = 4 + hl * 2
        fill_op = 0.35 + hl * 0.25
        cur_accent = acc_color

        # Hinge center in world coordinates
        h_pt = c + s * P(0.08, 0)

        # Flap vertices relative to hinge center at 0 deg
        v_top_front = s * P(0.01, 0.24)
        v_top_back = s * P(0.92, 0.03)
        v_bot_back = s * P(0.92, -0.03)
        v_bot_front = s * P(0.01, -0.24)

        p1 = h_pt + rot_vec(v_top_front, deg)
        p2 = h_pt + rot_vec(v_top_back, deg)
        p3 = h_pt + rot_vec(v_bot_back, deg)
        p4 = h_pt + rot_vec(v_bot_front, deg)

        is_active = (deg != 0) or (hl > 0)
        flap = Polygon(
            p1,
            p2,
            p3,
            p4,
            stroke_color=cur_accent if is_active else th.ink,
            stroke_width=stroke_w,
            fill_color=cur_accent if is_active else th.muted,
            fill_opacity=fill_op,
        )

        # Internal structural rib
        p_mid_front = (p1 + p4) * 0.5
        p_mid_back = (p2 + p3) * 0.5
        rib = Line(p_mid_front, p_mid_back, stroke_color=th.ink, stroke_width=2, stroke_opacity=0.4)

        # Actuator pushrod
        act_base = c + s * P(-0.25, -0.15)
        act_bracket = h_pt + rot_vec(s * P(0.18, -0.16), deg)
        act_line = Line(act_base, act_bracket, stroke_color=th.ink, stroke_width=3)
        act_cyl = Line(act_base, act_base + (act_bracket - act_base) * 0.5, stroke_color=th.ink, stroke_width=6)

        g = VGroup(flap, rib, act_line, act_cyl)

        # Deflection indicator arc and aerodynamic force vector
        if abs(deg) > 2.0:
            start_a = 0 if deg > 0 else math.radians(deg)
            sweep_a = math.radians(abs(deg))
            arc = Arc(
                radius=0.5 * s,
                start_angle=start_a,
                angle=sweep_a,
                arc_center=h_pt,
                stroke_color=cur_accent,
                stroke_width=2,
                stroke_opacity=0.8,
            )
            g.add(arc)

            te_pt = (p2 + p3) * 0.5
            force_dir = P(0, 0.28 * s if deg < 0 else -0.28 * s)
            force_arrow = Line(te_pt, te_pt + force_dir, stroke_color=cur_accent, stroke_width=3)

            dy = -0.06 * s if deg < 0 else 0.06 * s
            tip_left = te_pt + force_dir + P(-0.06 * s, dy)
            tip_right = te_pt + force_dir + P(0.06 * s, dy)
            arrow_head = Polygon(
                te_pt + force_dir,
                tip_left,
                tip_right,
                stroke_color=cur_accent,
                fill_color=cur_accent,
                fill_opacity=0.9,
                stroke_width=1,
            )
            g.add(force_arrow, arrow_head)

        return g

    dynamic_mob = always_redraw(draw_dynamic)
    part.mob.add(dynamic_mob)

    def do_on(value=1.0, instant=False):
        target = 25.0 * (value if isinstance(value, (int, float)) and value != 0 else 1.0)
        if instant:
            angle_tracker.set_value(target)
            part.speed.set_value(1.0)
            return []
        return [
            angle_tracker.animate.set_value(target),
            part.speed.animate.set_value(1.0),
        ]

    def do_off(value=0.0, instant=False):
        if instant:
            angle_tracker.set_value(0.0)
            part.speed.set_value(0.0)
            return []
        return [
            angle_tracker.animate.set_value(0.0),
            part.speed.animate.set_value(0.0),
        ]

    def do_up(value=25.0, instant=False):
        target = float(value) if isinstance(value, (int, float)) else 25.0
        if instant:
            angle_tracker.set_value(target)
            return []
        return [angle_tracker.animate.set_value(target)]

    def do_down(value=-25.0, instant=False):
        target = float(value) if isinstance(value, (int, float)) else -25.0
        if instant:
            angle_tracker.set_value(target)
            return []
        return [angle_tracker.animate.set_value(target)]

    def do_neutral(value=0.0, instant=False):
        if instant:
            angle_tracker.set_value(0.0)
            return []
        return [angle_tracker.animate.set_value(0.0)]

    def do_highlight(value=1.0, instant=False):
        target = 1.0 if value else 0.0
        if instant:
            highlight_tracker.set_value(target)
            return []
        return [highlight_tracker.animate.set_value(target)]

    part.actions["on"] = do_on
    part.actions["off"] = do_off
    part.actions["up"] = do_up
    part.actions["down"] = do_down
    part.actions["neutral"] = do_neutral
    part.actions["highlight"] = do_highlight

    return part
