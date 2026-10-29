# Kit part "lock_gate", generated for "How Panama Canal locks lift giant ships over a mountain using only gravity" and kept for reuse.
# Validated with `python -m story_engine validate-part`. Edit freely; keep PART and build(k, spec).
PART = {
    "name": "lock_gate",
    "use": "tall hollow steel miter gate barrier for canal locks; actions: on (close/seal gate), off (open gate), highlight",
    "actions": ["on", "off", "highlight"]
}

def build(k, spec):
    th = k.theme

    # Reference bounding frame
    frame = Rectangle(width=2.6, height=2.2, stroke_width=0, fill_opacity=0)

    # State trackers: 1.0 = closed (sealed miter position), 0.0 = open (recessed into walls)
    state_tracker = ValueTracker(1.0)
    highlight_tracker = ValueTracker(0.0)

    part = Part(
        VGroup(frame),
        ports={
            "in": (0, -0.5),
            "out": (0, 0.5),
            "left": (-0.5, 0),
            "right": (0.5, 0),
            "top": (0, 0.5),
            "bottom": (0, -0.5),
            "center": (0, 0)
        }
    )

    def draw_gate():
        c = frame.get_center()
        w = frame.width
        s = w / 2.6

        u = state_tracker.get_value()
        hl_val = highlight_tracker.get_value()

        g = VGroup()

        # Water channel (background)
        water = Rectangle(
            width=1.7 * s, height=2.0 * s, stroke_width=0,
            fill_color="#4FC3F7", fill_opacity=0.25
        ).move_to(c)
        g.add(water)

        # Water flow indicator lines
        for y_off in (-0.7, 0.0, 0.6):
            flow = Line(
                c + P(-0.45 * s, y_off * s), c + P(0.45 * s, y_off * s),
                stroke_color="#4FC3F7", stroke_width=1.5, stroke_opacity=0.4
            )
            g.add(flow)

        # Concrete lock walls with gate recesses
        pts_L = [
            c + P(-1.30 * s, -1.00 * s),
            c + P(-0.85 * s, -1.00 * s),
            c + P(-0.85 * s, -0.25 * s),
            c + P(-1.10 * s, -0.20 * s),
            c + P(-1.10 * s,  0.65 * s),
            c + P(-0.85 * s,  0.75 * s),
            c + P(-0.85 * s,  1.00 * s),
            c + P(-1.30 * s,  1.00 * s),
        ]
        wall_L = Polygon(
            *pts_L, stroke_color=th.ink, stroke_width=3,
            fill_color=th.bg2, fill_opacity=0.9
        )

        pts_R = [
            c + P(1.30 * s, -1.00 * s),
            c + P(0.85 * s, -1.00 * s),
            c + P(0.85 * s, -0.25 * s),
            c + P(1.10 * s, -0.20 * s),
            c + P(1.10 * s,  0.65 * s),
            c + P(0.85 * s,  0.75 * s),
            c + P(0.85 * s,  1.00 * s),
            c + P(1.30 * s,  1.00 * s),
        ]
        wall_R = Polygon(
            *pts_R, stroke_color=th.ink, stroke_width=3,
            fill_color=th.bg2, fill_opacity=0.9
        )

        g.add(wall_L, wall_R)

        # Sill line at bottom of lock chamber
        sill = Line(
            c + P(-0.85 * s, -0.25 * s), c + P(0.85 * s, -0.25 * s),
            stroke_color=th.muted, stroke_width=2
        )
        g.add(sill)

        # Hinge pins
        hinge_L_pos = c + P(-0.85 * s, -0.25 * s)
        hinge_R_pos = c + P( 0.85 * s, -0.25 * s)

        hinge_L = Circle(
            radius=0.05 * s, stroke_color=th.ink, stroke_width=2,
            fill_color=th.accent, fill_opacity=1.0
        ).move_to(hinge_L_pos)

        hinge_R = Circle(
            radius=0.05 * s, stroke_color=th.ink, stroke_width=2,
            fill_color=th.accent, fill_opacity=1.0
        ).move_to(hinge_R_pos)

        g.add(hinge_L, hinge_R)

        # Leaf geometry calculations
        len_leaf = math.hypot(0.85, 0.45) * s
        thick = 0.11 * s

        ang_closed_L = math.atan2(0.45, 0.85)
        ang_open_L = math.atan2(0.80, -0.10)
        ang_L = (1.0 - u) * ang_open_L + u * ang_closed_L

        ang_closed_R = math.atan2(0.45, -0.85)
        ang_open_R = math.atan2(0.80, 0.10)
        ang_R = (1.0 - u) * ang_open_R + u * ang_closed_R

        active_col = th.accent if (u > 0.5 or hl_val > 0.1) else th.muted

        # --- LEFT LEAF ---
        tip_L = hinge_L_pos + P(len_leaf * math.cos(ang_L), len_leaf * math.sin(ang_L))
        perp_L = P(-math.sin(ang_L) * thick, math.cos(ang_L) * thick)

        p1_L, p2_L = hinge_L_pos, tip_L
        p3_L, p4_L = tip_L + perp_L, hinge_L_pos + perp_L

        leaf_body_L = Polygon(
            p1_L, p2_L, p3_L, p4_L,
            stroke_color=th.ink, stroke_width=3,
            fill_color=th.bg2, fill_opacity=0.9
        )

        girders_L = VGroup()
        for frac in (0.33, 0.66):
            pt_a = p1_L + frac * (p2_L - p1_L)
            pt_b = p4_L + frac * (p3_L - p4_L)
            girders_L.add(Line(pt_a, pt_b, stroke_color=th.muted, stroke_width=1.5))
        girders_L.add(Line(p1_L, p3_L, stroke_color=th.muted, stroke_width=1, stroke_opacity=0.5))
        girders_L.add(Line(p4_L, p2_L, stroke_color=th.muted, stroke_width=1, stroke_opacity=0.5))

        seal_L = Line(p2_L, p3_L, stroke_color=active_col, stroke_width=4 + 2 * hl_val)

        g.add(leaf_body_L, girders_L, seal_L)

        # --- RIGHT LEAF ---
        tip_R = hinge_R_pos + P(len_leaf * math.cos(ang_R), len_leaf * math.sin(ang_R))
        perp_R = P(math.sin(ang_R) * thick, -math.cos(ang_R) * thick)

        p1_R, p2_R = hinge_R_pos, tip_R
        p3_R, p4_R = tip_R + perp_R, hinge_R_pos + perp_R

        leaf_body_R = Polygon(
            p1_R, p2_R, p3_R, p4_R,
            stroke_color=th.ink, stroke_width=3,
            fill_color=th.bg2, fill_opacity=0.9
        )

        girders_R = VGroup()
        for frac in (0.33, 0.66):
            pt_a = p1_R + frac * (p2_R - p1_R)
            pt_b = p4_R + frac * (p3_R - p4_R)
            girders_R.add(Line(pt_a, pt_b, stroke_color=th.muted, stroke_width=1.5))
        girders_R.add(Line(p1_R, p3_R, stroke_color=th.muted, stroke_width=1, stroke_opacity=0.5))
        girders_R.add(Line(p4_R, p2_R, stroke_color=th.muted, stroke_width=1, stroke_opacity=0.5))

        seal_R = Line(p2_R, p3_R, stroke_color=active_col, stroke_width=4 + 2 * hl_val)

        g.add(leaf_body_R, girders_R, seal_R)

        # Miter center seal glow
        if u > 0.8 or hl_val > 0.1:
            miter_center = (tip_L + tip_R) / 2.0
            glow = Circle(
                radius=0.15 * s * (1.0 + 0.3 * hl_val),
                stroke_color=active_col, stroke_width=2,
                fill_color=active_col, fill_opacity=0.4 + 0.3 * hl_val
            ).move_to(miter_center)
            g.add(glow)

        return g

    part.mob.add(always_redraw(draw_gate))

    def _parse_val(val, default=1.0):
        if val is None:
            return default
        if isinstance(val, bool):
            return 1.0 if val else 0.0
        try:
            return float(val)
        except (TypeError, ValueError):
            return default

    def action_on(value=1.0, instant=False):
        v = _parse_val(value, 1.0)
        target = 1.0 if v > 0.5 else 0.0
        if instant:
            state_tracker.set_value(target)
            return []
        return [state_tracker.animate.set_value(target)]

    def action_off(value=1.0, instant=False):
        v = _parse_val(value, 1.0)
        target = 0.0 if v > 0.5 else 1.0
        if instant:
            state_tracker.set_value(target)
            return []
        return [state_tracker.animate.set_value(target)]

    def action_highlight(value=1.0, instant=False):
        v = _parse_val(value, 1.0)
        if instant:
            highlight_tracker.set_value(v)
            return []
        return [highlight_tracker.animate.set_value(v)]

    part.actions["on"] = action_on
    part.actions["off"] = action_off
    part.actions["highlight"] = action_highlight
    part.actions["open"] = action_off
    part.actions["close"] = action_on

    return part
