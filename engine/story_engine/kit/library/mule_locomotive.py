# Kit part "mule_locomotive", generated for "How Panama Canal locks lift giant ships over a mountain using only gravity" and kept for reuse.
# Validated with `python -m story_engine validate-part`. Edit freely; keep PART and build(k, spec).
PART = {
    "name": "mule_locomotive",
    "use": "electric rack railway tow locomotive mounted on canal wall/rail; actions: on, off, highlight",
    "actions": ["on", "off", "highlight"]
}

def build(k, spec):
    th = k.theme
    accent = spec.get("color", th.accent)

    # Frame defining bounding area for scale-invariant updaters
    frame = Rectangle(width=2.4, height=1.8, stroke_width=0, fill_opacity=0)

    # Track & rack rail
    track_line = Line(P(-1.2, -0.65), P(1.2, -0.65), stroke_color=th.muted, stroke_width=4)
    rack_line = Line(P(-1.2, -0.75), P(1.2, -0.75), stroke_color=th.ink, stroke_width=2)
    teeth = VGroup(*[
        Line(P(x, -0.75), P(x, -0.65), stroke_color=th.ink, stroke_width=2)
        for x in np.linspace(-1.15, 1.15, 16)
    ])
    track = VGroup(track_line, rack_line, teeth)

    # Symmetrical electric locomotive body (Panama Canal mule profile)
    body_pts = [
        P(-1.0, -0.5),
        P(-0.85, -0.05),
        P(-0.5, 0.15),
        P(0.5, 0.15),
        P(0.85, -0.05),
        P(1.0, -0.5)
    ]
    body = Polygon(*body_pts, stroke_color=th.ink, stroke_width=4, fill_color=th.bg2, fill_opacity=0.85)

    # Cab windows
    win_left = Polygon(P(-0.75, -0.05), P(-0.48, 0.1), P(-0.15, 0.1), P(-0.15, -0.05),
                       stroke_color=th.ink, stroke_width=2, fill_color=th.ink, fill_opacity=0.35)
    win_right = Polygon(P(0.15, -0.05), P(0.15, 0.1), P(0.48, 0.1), P(0.75, -0.05),
                        stroke_color=th.ink, stroke_width=2, fill_color=th.ink, fill_opacity=0.35)
    cabs = VGroup(win_left, win_right)

    # Tow Winch / Cable Spool
    winch_base = Rectangle(width=0.5, height=0.25, stroke_color=th.ink, stroke_width=3,
                           fill_color=th.muted, fill_opacity=0.5).move_to(P(0, 0.275))
    winch_spool = Circle(radius=0.18, stroke_color=th.ink, stroke_width=3,
                         fill_color=accent, fill_opacity=0.4).move_to(P(0, 0.35))
    winch_spoke = Line(P(0, 0.17), P(0, 0.53), stroke_color=th.ink, stroke_width=2)
    winch = VGroup(winch_base, winch_spool, winch_spoke)

    # Drive wheels and center rack pinion
    w1 = Circle(radius=0.12, stroke_color=th.ink, stroke_width=3, fill_color=th.ink, fill_opacity=0.2).move_to(P(-0.6, -0.55))
    w2 = Circle(radius=0.12, stroke_color=th.ink, stroke_width=3, fill_color=th.ink, fill_opacity=0.2).move_to(P(0.6, -0.55))
    cog = Circle(radius=0.1, stroke_color=accent, stroke_width=3, fill_color=accent, fill_opacity=0.5).move_to(P(0, -0.6))
    cog_cross = VGroup(
        Line(P(-0.08, -0.6), P(0.08, -0.6), stroke_color=th.ink, stroke_width=2),
        Line(P(0, -0.68), P(0, -0.52), stroke_color=th.ink, stroke_width=2)
    )
    wheels = VGroup(w1, w2, cog, cog_cross)

    # Warning lights / beacons
    light_l = Dot(P(-0.92, -0.2), radius=0.05, color=accent)
    light_r = Dot(P(0.92, -0.2), radius=0.05, color=accent)
    beacon = Circle(radius=0.07, stroke_color=accent, stroke_width=2, fill_color=accent, fill_opacity=0.8).move_to(P(0, 0.55))
    lights = VGroup(light_l, light_r, beacon)

    # Highlight box for emphasis action
    hl_box = RoundedRectangle(corner_radius=0.15, width=2.2, height=1.3,
                              stroke_color=accent, stroke_width=4, fill_opacity=0).move_to(P(0, -0.05))
    hl_box.set_stroke(opacity=0)

    main_group = VGroup(frame, track, body, cabs, winch, wheels, lights, hl_box)

    part = Part(main_group, ports={
        "in": (-0.5, 0),
        "out": (0.5, 0),
        "cable": (0.0, 0.35),
        "top": (0, 0.55),
        "bottom": (0, -0.65)
    })

    # Rotating components linked to part.speed
    part.spinner(w1, 5.0)
    part.spinner(w2, 5.0)
    part.spinner(cog, 8.0)
    part.spinner(cog_cross, 8.0)
    part.spinner(winch_spool, -3.0)
    part.spinner(winch_spoke, -3.0)

    # Tow cable line always positioned relative to frame sub-mobject
    def draw_cable():
        c = frame.get_center()
        s = frame.height / 1.8
        spool_center = c + np.array([0, 0.35 * s, 0])
        cable_end = c + np.array([1.15 * s, 0.65 * s, 0])
        cable = Line(spool_center, cable_end, stroke_color=accent, stroke_width=3)
        if part.speed.get_value() > 0.05:
            cable.set_stroke(color=accent, width=4)
        return cable

    cable_mobject = always_redraw(draw_cable)
    part.mob.add(cable_mobject)

    # Motor actions for smooth speed transitions (on/off)
    part.motor_actions()

    # Highlight action tracker
    hl_tracker = ValueTracker(0.0)

    def update_hl(m):
        hl_box.set_stroke(opacity=hl_tracker.get_value())

    hl_box.add_updater(update_hl)

    def action_highlight(value=True, instant=False):
        target = 1.0 if value else 0.0
        if instant:
            hl_tracker.set_value(target)
            return []
        return [hl_tracker.animate.set_value(target)]

    part.actions["highlight"] = action_highlight

    return part
