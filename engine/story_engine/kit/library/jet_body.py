# Kit part "jet_body", generated for "How Fighter Jets Don't Tear Themselves Apart Spinning" and kept for reuse.
# Validated with `python -m story_engine validate-part`. Edit freely; keep PART and build(k, spec).
PART = {
    "name": "jet_body",
    "use": "side view silhouette of a fighter jet fuselage with canopy, tail fin, nozzle, and engine thrust glow; actions: highlight, on, off",
    "actions": ["highlight", "on", "off"]
}

def build(k, spec):
    th = k.theme

    # Frame anchor for scaling and positioning reference
    frame = Rectangle(width=2.6, height=1.5, stroke_width=0, fill_opacity=0)

    # Jet hull outline coordinates (facing right, centered near origin)
    hull_pts = [
        P(1.25, -0.05),   # Nose tip
        P(0.7, 0.1),      # Upper nose
        P(0.4, 0.28),     # Cockpit front
        P(-0.05, 0.22),   # Cockpit rear / upper fuselage
        P(-0.5, 0.22),    # Spine to tail base
        P(-0.95, 0.75),   # Tail fin apex
        P(-1.15, 0.75),   # Tail fin trailing top
        P(-0.95, 0.18),   # Tail fin trailing root
        P(-1.15, 0.15),   # Nozzle upper attachment
        P(-1.15, -0.15),  # Nozzle lower attachment
        P(-0.85, -0.18),  # Under tail
        P(-0.2, -0.28),   # Intake scoop
        P(0.2, -0.25),    # Lower fuselage
        P(0.75, -0.12),   # Lower nose
    ]

    hull = Polygon(*hull_pts, stroke_color=th.ink, stroke_width=4, fill_color=th.ink, fill_opacity=0.15)

    # Stress / structural highlight overlay
    highlight_hull = Polygon(*hull_pts, stroke_color=th.accent, stroke_width=6, fill_color=th.accent, fill_opacity=0.35)
    highlight_hull.set_opacity(0)

    # Cockpit canopy bubble
    canopy_pts = [
        P(0.65, 0.11),
        P(0.4, 0.28),
        P(0.0, 0.23),
        P(0.1, 0.12)
    ]
    canopy = Polygon(*canopy_pts, stroke_color=th.ink, stroke_width=3, fill_color=th.accent3, fill_opacity=0.5)

    # Cockpit glint line
    shine = Line(P(0.45, 0.21), P(0.25, 0.20), stroke_color=th.bg, stroke_width=2)

    # Panel line details
    panel1 = Line(P(0.3, -0.22), P(0.25, 0.12), stroke_color=th.muted, stroke_width=2)
    panel2 = Line(P(-0.3, -0.20), P(-0.3, 0.22), stroke_color=th.muted, stroke_width=2)

    # Wing profile outline
    wing_line = Polygon(
        P(0.1, -0.05), P(-0.4, 0.08), P(-0.65, -0.12), P(-0.25, -0.12),
        stroke_color=th.ink, stroke_width=3, fill_color=th.ink, fill_opacity=0.25
    )

    # Engine nozzle at the rear
    nozzle = Rectangle(width=0.18, height=0.3, stroke_color=th.ink, stroke_width=3, fill_color=th.ink, fill_opacity=0.8)
    nozzle.move_to(P(-1.15, 0))

    # Thrust flames (outer glow and inner core)
    flame_bg = Polygon(
        P(-1.24, 0.12), P(-1.8, 0.0), P(-1.24, -0.12),
        stroke_color="#FF7043", stroke_width=2, fill_color="#FF7043", fill_opacity=0.85
    )
    flame_core = Polygon(
        P(-1.24, 0.07), P(-1.55, 0.0), P(-1.24, -0.07),
        stroke_color="#FFE082", stroke_width=0, fill_color="#FFE082", fill_opacity=0.95
    )
    flame_group = VGroup(flame_bg, flame_core)
    flame_group.set_opacity(0)

    body_grp = VGroup(frame, highlight_hull, hull, wing_line, panel1, panel2, canopy, shine, nozzle, flame_group)

    part = Part(body_grp, ports={
        "in": (-0.5, 0),
        "out": (0.5, 0),
        "nose": (0.48, -0.03),
        "tail": (-0.48, 0),
        "top": (0, 0.4),
        "bottom": (0, -0.35),
        "wing": (-0.15, -0.05)
    })

    state = {"time": 0.0, "highlighted": False}

    # Dynamic thrust flame updater linked to part.speed and relative geometry
    def update_flame(m, dt):
        state["time"] += dt
        sp = part.speed.get_value()
        if sp > 0.01:
            nc = nozzle.get_center()
            nw = nozzle.width
            s = frame.width / 2.6  # Scale factor relative to initial size

            sy = 0.8 + 0.2 * np.sin(state["time"] * 25)
            flame_len = 0.65 * sp * (0.9 + 0.1 * np.sin(state["time"] * 30)) * s

            back_x = nc[0] - nw / 2
            top_y = nc[1] + 0.12 * sy * s
            bot_y = nc[1] - 0.12 * sy * s
            tip_x = back_x - flame_len

            flame_group.set_opacity(sp)
            flame_bg.set_points_as_corners([
                np.array([back_x, top_y, 0]),
                np.array([tip_x, nc[1], 0]),
                np.array([back_x, bot_y, 0]),
                np.array([back_x, top_y, 0])
            ])

            top_y_core = nc[1] + 0.06 * sy * s
            bot_y_core = nc[1] - 0.06 * sy * s
            tip_x_core = back_x - flame_len * 0.6

            flame_core.set_points_as_corners([
                np.array([back_x, top_y_core, 0]),
                np.array([tip_x_core, nc[1], 0]),
                np.array([back_x, bot_y_core, 0]),
                np.array([back_x, top_y_core, 0])
            ])
        else:
            flame_group.set_opacity(0)

    clock = VMobject()
    clock.add_updater(update_flame)
    part.mob.add(clock)

    # Set up standard motor actions for thrust speed
    part.motor_actions()

    # Custom action: highlight structural stress / hull outline
    def do_highlight(value=True, instant=False):
        state["highlighted"] = bool(value)
        target_opacity = 1.0 if value else 0.0
        if instant:
            highlight_hull.set_opacity(target_opacity)
            return []
        return [
            highlight_hull.animate.set_opacity(target_opacity)
        ]

    part.actions["highlight"] = do_highlight

    return part
