# Kit part "wing_section", generated for "How Fighter Jets Don't Tear Themselves Apart Spinning" and kept for reuse.
# Validated with `python -m story_engine validate-part`. Edit freely; keep PART and build(k, spec).
PART = {
    "name": "wing_section",
    "use": "airfoil cross-section with hinged elevon flap and aerodynamic airflow; actions: on, off, highlight",
    "actions": ["on", "off", "highlight"],
}


def build(k, spec):
    th = k.theme

    # Frame defining reference scale and positioning area
    frame = Rectangle(width=2.6, height=1.2, stroke_width=0, fill_opacity=0)

    # Static main wing structure (forebody)
    main_pts = [
        P(-1.15, 0.0),
        P(-0.7, 0.32),
        P(-0.1, 0.24),
        P(0.3, 0.10),
        P(0.25, 0.01),
        P(0.3, -0.08),
        P(-0.1, -0.15),
        P(-0.7, -0.18),
    ]
    main_wing = VMobject(
        stroke_color=th.ink, stroke_width=4, fill_color=th.bg2, fill_opacity=0.6
    )
    main_wing.set_points_smoothly([*main_pts, main_pts[0]])

    # Internal structural spars and rib cross-bracing
    spar1 = Line(P(-0.6, 0.30), P(-0.6, -0.17), stroke_color=th.muted, stroke_width=2)
    spar2 = Line(P(-0.05, 0.23), P(-0.05, -0.14), stroke_color=th.muted, stroke_width=2)
    rib1 = Line(P(-0.6, 0.30), P(-0.05, -0.14), stroke_color=th.muted, stroke_width=1.5)
    rib2 = Line(P(-0.6, -0.17), P(-0.05, 0.23), stroke_color=th.muted, stroke_width=1.5)

    # Hinge pivot pin
    hinge_pin = Circle(
        radius=0.045, stroke_color=th.ink, stroke_width=3, fill_color=th.accent, fill_opacity=1.0
    ).move_to(P(0.3, 0.01))

    static_wing = VGroup(frame, main_wing, spar1, spar2, rib1, rib2, hinge_pin)

    part = Part(
        static_wing,
        ports={
            "in": (-0.45, 0.0),
            "out": (0.45, 0.0),
            "top": (0.0, 0.45),
            "bottom": (0.0, -0.45),
            "hinge": (0.12, 0.0),
        },
    )

    hl_tracker = ValueTracker(0.0)
    phase = {"x": 0.0}

    def tick(m, dt):
        phase["x"] = (phase["x"] + dt * 0.6 * part.speed.get_value()) % 1.0

    clock = VMobject()
    clock.add_updater(tick)
    part.mob.add(clock)

    def draw_dynamic():
        c = frame.get_center()
        s = frame.width / 2.6
        sp = part.speed.get_value()
        hl = hl_tracker.get_value()

        # Elevon flap deflection angle (downward deflection when active)
        theta = -0.4 * sp
        pivot = c + s * P(0.3, 0.01)

        raw_flap_pts = [
            P(0.0, 0.09),
            P(0.35, 0.05),
            P(0.75, -0.01),
            P(0.35, -0.06),
            P(0.0, -0.08),
            P(-0.03, 0.0),
        ]

        cos_t, sin_t = math.cos(theta), math.sin(theta)
        rot_pts = []
        for pt in raw_flap_pts:
            rx = pt[0] * cos_t - pt[1] * sin_t
            ry = pt[0] * sin_t + pt[1] * cos_t
            rot_pts.append(pivot + s * np.array([rx, ry, 0]))

        flap_stroke = th.accent if hl > 0.3 else th.ink
        flap_fill = th.accent if hl > 0.3 else th.accent3
        flap_opacity = 0.6 if hl > 0.3 else 0.4

        flap = VMobject(
            stroke_color=flap_stroke,
            stroke_width=4,
            fill_color=flap_fill,
            fill_opacity=flap_opacity,
        )
        flap.set_points_smoothly([*rot_pts, rot_pts[0]])

        g = VGroup(flap)

        if hl > 0.3:
            glow = Circle(
                radius=0.12 * s,
                stroke_color=th.accent,
                stroke_width=3,
                fill_color=th.accent,
                fill_opacity=0.25,
            ).move_to(pivot)
            g.add(glow)

        # Streamlines around airfoil
        y_baselines = [0.45, 0.20, -0.18, -0.40]
        for y0 in y_baselines:
            pts = []
            for x_val in np.linspace(-1.25, 1.25, 15):
                if y0 > 0:
                    y_bump = 0.15 * math.exp(-(((x_val + 0.3) / 0.45) ** 2))
                else:
                    y_bump = -0.07 * math.exp(-(((x_val + 0.3) / 0.45) ** 2))

                flap_def = math.sin(theta) * max(0.0, x_val - 0.3) if x_val > 0.3 else 0.0
                pts.append(c + s * np.array([x_val, y0 + y_bump + flap_def, 0]))

            line = VMobject(stroke_color=th.muted, stroke_width=1.5, stroke_opacity=0.35)
            line.set_points_smoothly(pts)
            g.add(line)

            # Airflow velocity streaks
            if sp > 0.01:
                for i in range(3):
                    t = (phase["x"] + i / 3.0) % 1.0
                    xp = -1.25 + 2.5 * t
                    yp_bump = (0.15 if y0 > 0 else -0.07) * math.exp(-(((xp + 0.3) / 0.45) ** 2))
                    f_def = math.sin(theta) * max(0.0, xp - 0.3) if xp > 0.3 else 0.0
                    p1 = c + s * np.array([xp, y0 + yp_bump + f_def, 0])

                    xp2 = xp + 0.14
                    yp2_bump = (0.15 if y0 > 0 else -0.07) * math.exp(-(((xp2 + 0.3) / 0.45) ** 2))
                    f_def2 = math.sin(theta) * max(0.0, xp2 - 0.3) if xp2 > 0.3 else 0.0
                    p2 = c + s * np.array([xp2, y0 + yp2_bump + f_def2, 0])

                    streak = Line(
                        p1,
                        p2,
                        stroke_color=th.accent2,
                        stroke_width=3,
                        stroke_opacity=min(1.0, sp * 1.2),
                    )
                    g.add(streak)

        return g

    dynamic_group = always_redraw(draw_dynamic)
    part.mob.add(dynamic_group)

    part.motor_actions()

    def act_highlight(val=1.0, instant=False):
        if val is None:
            target = 1.0
        elif isinstance(val, bool):
            target = 1.0 if val else 0.0
        else:
            target = float(val)

        if instant:
            hl_tracker.set_value(target)
            return []
        return [hl_tracker.animate.set_value(target)]

    part.actions["highlight"] = act_highlight

    return part
